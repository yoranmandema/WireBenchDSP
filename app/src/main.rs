//! The pedal app: channels, presets, live audio.
//!
//! ```text
//!   preset dir ──▶ Rig ──▶ Doc ──▶ Graph ──▶ Executor ──▶ audio thread
//!                   ▲                                      │
//!                   └────────── params / bypass ◀───────────┘
//! ```
//!
//! The UI edits a [`Rig`] (channels of blocks). Every structural change
//! rebuilds the executor on the UI thread and hands it to the audio thread
//! over a lock-free queue; parameter changes go straight to the running
//! executor by index.

mod audio;
mod channels;
mod store;
mod ui;

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use audio::{AudioThread, ChannelMode, Cmd, Handle, Meters, Setup};
use channels::Rig;
use store::Store;
use ui::{rebuild, UiState};

const RING_CAPACITY: usize = 8192;
const LATENCY_FRAMES: usize = 1024;
const BLOCK: usize = 64;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let selftest = args.iter().any(|a| a == "--selftest");
    let headless = selftest || args.iter().any(|a| a == "--headless");

    let store = Store::discover();
    if let Some(e) = &store.error {
        eprintln!("presets: {e}");
    }
    let preset_name = args
        .iter()
        .position(|a| a == "--rig")
        .and_then(|i| args.get(i + 1))
        .cloned()
        .or_else(|| store.presets.first().map(|(n, _)| n.clone()))
        .unwrap_or_else(|| "01-clean".into());
    let doc = store
        .presets
        .iter()
        .find(|(n, _)| *n == preset_name)
        .map(|(_, d)| d.clone())
        .or_else(|| store.presets.first().map(|(_, d)| d.clone()))
        .unwrap_or_else(|| channels::Rig::single("Untitled", vec![]).to_doc());
    let rig = Rig::from_doc(&doc);

    let host = cpal::default_host();
    let (in_dev, in_name) = pick_input(&host);
    let (out_dev, out_name) = pick_output(&host, &in_name);
    let in_default = in_dev.default_input_config().expect("no input config");
    let out_default = out_dev.default_output_config().expect("no output config");
    let rate = pick_rate(&in_dev, &out_dev, in_default.sample_rate(), out_default.sample_rate());
    let out_channels = out_default.channels().max(1) as usize;
    let in_channels = in_default.channels().max(1) as usize;

    let mut in_cfg: cpal::StreamConfig = in_default.into();
    in_cfg.sample_rate = rate;
    let mut out_cfg: cpal::StreamConfig = out_default.into();
    out_cfg.sample_rate = rate;

    let setup = Setup {
        sample_rate: rate as f32,
        out_channels,
        block: BLOCK,
        channel_mode: ChannelMode::First(2.min(in_channels)),
        latency_frames: LATENCY_FRAMES,
    };
    let source = Arc::new(AtomicU8::new(0));
    let (handle, cmd_rx, trash_tx, running) = audio::new_engine(setup, source.clone());
    let meters = handle.meters.clone();
    // Compile the initial rig before the streams start.
    let block = handle.block();
    let mut g = rig.to_doc().build(&handle.registry).expect("initial rig builds");
    let ex = dsp_graph::Executor::compile_owned(&mut g, rate as f32, block).expect("initial rig compiles");
    *handle.node_ids.lock().unwrap() = ex.node_ids().to_vec();
    handle.push(Cmd::Swap(ex, Vec::new())).expect("initial swap");
    handle.set_source(0); // riff

    if selftest {
        selftest_run(handle, meters, rate as f32, out_channels, &rig)
    }

    // --- input stream ---
    let ring = Arc::new(audio::Ring::new(RING_CAPACITY));
    let ring_in = ring.clone();
    let in_format = in_default.sample_format();
    let in_stream = match in_format {
        cpal::SampleFormat::F32 => build_input::<f32>(&in_dev, in_cfg, in_channels, ring_in, meters.clone()),
        cpal::SampleFormat::I16 => build_input::<i16>(&in_dev, in_cfg, in_channels, ring_in, meters.clone()),
        cpal::SampleFormat::I32 => build_input::<i32>(&in_dev, in_cfg, in_channels, ring_in, meters.clone()),
        cpal::SampleFormat::U16 => build_input::<u16>(&in_dev, in_cfg, in_channels, ring_in, meters.clone()),
        other => panic!("unsupported input format {other:?}"),
    }
    .expect("failed to build input stream");
    in_stream.play().expect("failed to start input");
    std::mem::forget(in_stream);

    // --- output stream ---
    let ring_out = ring.clone();
    let out_format = out_default.sample_format();
    let out_stream = match out_format {
        cpal::SampleFormat::F32 => build_output::<f32>(&out_dev, out_cfg, out_channels, ring_out, cmd_rx, trash_tx, running.clone(), meters.clone(), source.clone()),
        cpal::SampleFormat::I16 => build_output::<i16>(&out_dev, out_cfg, out_channels, ring_out, cmd_rx, trash_tx, running.clone(), meters.clone(), source.clone()),
        cpal::SampleFormat::I32 => build_output::<i32>(&out_dev, out_cfg, out_channels, ring_out, cmd_rx, trash_tx, running.clone(), meters.clone(), source.clone()),
        cpal::SampleFormat::U16 => build_output::<u16>(&out_dev, out_cfg, out_channels, ring_out, cmd_rx, trash_tx, running.clone(), meters.clone(), source.clone()),
        other => panic!("unsupported output format {other:?}"),
    }
    .expect("failed to build output stream");
    out_stream.play().expect("failed to start output");
    std::mem::forget(out_stream);
    ring.trim_to(LATENCY_FRAMES);

    eprintln!("input : {in_name} @ {rate} Hz, {in_channels} ch");
    eprintln!("output: {out_name} @ {rate} Hz, {out_channels} ch");

    if headless {
        eprintln!("headless: running for 60 s");
        std::thread::sleep(Duration::from_secs(60));
        return;
    }

    let mut state = UiState::default();
    state.doc_name = preset_name.clone();
    state.status = format!("{preset_name} · {rate} Hz");
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 760.0])
            .with_title("Pedal"),
        ..Default::default()
    };
    eframe::run_native(
        "Pedal",
        options,
        Box::new(move |_cc| {
            Ok(Box::new(App::new(handle, store, rig, state)))
        }),
    )
    .expect("eframe failed");
}

// ---------------------------------------------------------------------------
// Device selection
// ---------------------------------------------------------------------------

fn device_name(d: &cpal::Device) -> String {
    d.description().map(|d| d.name().to_string()).unwrap_or_else(|_| "unknown".into())
}

fn pick_input(host: &cpal::Host) -> (cpal::Device, String) {
    let devices: Vec<_> = host.input_devices().expect("no input devices").collect();
    if devices.is_empty() {
        panic!("no audio input devices");
    }
    for (i, d) in devices.iter().enumerate() {
        eprintln!("  in [{i}] {}", device_name(d));
    }
    let pick = devices
        .iter()
        .position(|d| device_name(d).contains("Scarlett"))
        .or_else(|| devices.iter().position(|d| device_name(d).to_lowercase().contains("usb")))
        .unwrap_or(0);
    let name = device_name(&devices[pick]);
    eprintln!("using input: {name}");
    (devices[pick].clone(), name)
}

fn pick_output(host: &cpal::Host, prefer: &str) -> (cpal::Device, String) {
    let devices: Vec<_> = host.output_devices().expect("no output devices").collect();
    if devices.is_empty() {
        panic!("no audio output devices");
    }
    for (i, d) in devices.iter().enumerate() {
        eprintln!("  out [{i}] {}", device_name(d));
    }
    let pick = devices
        .iter()
        .position(|d| device_name(d) == prefer)
        .or_else(|| devices.iter().position(|d| device_name(d).contains("MacBook") || device_name(d).contains("Built-in")))
        .unwrap_or(0);
    let name = device_name(&devices[pick]);
    eprintln!("using output: {name}");
    (devices[pick].clone(), name)
}

fn pick_rate(in_dev: &cpal::Device, out_dev: &cpal::Device, in_rate: u32, out_rate: u32) -> u32 {
    for r in [48_000, 44_100] {
        let i_ok = in_dev.supported_input_configs().map(|mut c| c.any(|c| c.min_sample_rate() <= r && r <= c.max_sample_rate())).unwrap_or(false);
        let o_ok = out_dev.supported_output_configs().map(|mut c| c.any(|c| c.min_sample_rate() <= r && r <= c.max_sample_rate())).unwrap_or(false);
        if i_ok && o_ok {
            return r;
        }
    }
    if in_rate == out_rate {
        return in_rate;
    }
    in_rate
}

// ---------------------------------------------------------------------------
// Streams
// ---------------------------------------------------------------------------

trait ToF32: cpal::SizedSample {
    fn to_f32(&self) -> f32;
}
impl ToF32 for f32 {
    fn to_f32(&self) -> f32 {
        *self
    }
}
impl ToF32 for i16 {
    fn to_f32(&self) -> f32 {
        *self as f32 / 32768.0
    }
}
impl ToF32 for i32 {
    fn to_f32(&self) -> f32 {
        *self as f32 / 2_147_483_648.0
    }
}
impl ToF32 for u16 {
    fn to_f32(&self) -> f32 {
        (*self as f32 / 32768.0) - 1.0
    }
}

trait FromF32: cpal::SizedSample {
    fn from_f32(v: f32) -> Self;
}
impl FromF32 for f32 {
    fn from_f32(v: f32) -> Self {
        v
    }
}
impl FromF32 for i16 {
    fn from_f32(v: f32) -> Self {
        (v.clamp(-1.0, 1.0) * 32767.0) as i16
    }
}
impl FromF32 for i32 {
    fn from_f32(v: f32) -> Self {
        (v.clamp(-1.0, 1.0) * 2_147_483_647.0) as i32
    }
}
impl FromF32 for u16 {
    fn from_f32(v: f32) -> Self {
        ((v.clamp(-1.0, 1.0) + 1.0) * 32767.0) as u16
    }
}

fn build_input<T: ToF32 + 'static>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    channels: usize,
    ring: Arc<audio::Ring>,
    meters: Arc<Meters>,
) -> Result<cpal::Stream, cpal::Error> {
    device.build_input_stream(
        config,
        move |data: &[T], _: &cpal::InputCallbackInfo| {
            let ch = channels.max(1);
            let mut peak = 0.0f32;
            for frame in data.chunks(ch) {
                let mut s = 0.0f32;
                for v in frame.iter().take(2.min(ch)) {
                    s += v.to_f32();
                }
                peak = peak.max(s.abs());
                ring.push(s);
            }
            meters.in_peak.fetch_max(peak.to_bits(), Ordering::Relaxed);
        },
        |err| eprintln!("input callback error: {err}"),
        Some(Duration::from_millis(500)),
    )
}

fn build_output<T: FromF32 + 'static>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    channels: usize,
    ring: Arc<audio::Ring>,
    cmd_rx: rtrb::Consumer<Cmd>,
    trash_tx: rtrb::Producer<dsp_graph::Executor>,
    running: Arc<std::sync::atomic::AtomicBool>,
    meters: Arc<Meters>,
    source: Arc<AtomicU8>,
) -> Result<cpal::Stream, cpal::Error> {
    let setup = Setup {
        sample_rate: config.sample_rate as f32,
        out_channels: channels,
        block: BLOCK,
        channel_mode: ChannelMode::First(2),
        latency_frames: LATENCY_FRAMES,
    };
    let mut engine = AudioThread::new(setup, meters.clone(), cmd_rx, trash_tx, running, source);
    engine.set_block(BLOCK);
    device.build_output_stream(
        config,
        move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
            let frames = data.len() / channels.max(1);
            let n = frames.min(BLOCK);
            // Live input comes from the ring; the riff/silence fill themselves.
            if engine.source() == 1 {
                let mut mono = [0.0f32; BLOCK];
                let before = ring.available();
                let starved = ring.pop(&mut mono[..n]);
                if starved || before < n {
                    meters.xruns.fetch_add(1, Ordering::Relaxed);
                }
                engine.copy_input(&mono[..n]);
            } else {
                engine.fill_source(n);
            }
            let out = engine.render(n);
            for (d, s) in data.iter_mut().take(n * channels).zip(out.iter()) {
                *d = T::from_f32(*s);
            }
            for d in data.iter_mut().skip(n * channels) {
                *d = T::from_f32(0.0);
            }
        },
        |err| eprintln!("output callback error: {err}"),
        Some(Duration::from_millis(500)),
    )
}

// ---------------------------------------------------------------------------
// Headless self-test
// ---------------------------------------------------------------------------

fn selftest_run(handle: Handle, meters: Arc<Meters>, rate: f32, out_channels: usize, rig: &Rig) -> ! {
    // Pull audio through the engine directly: the ring is filled by the riff,
    // so no interface is needed.
    let mut engine = AudioThread::new(
        Setup {
            sample_rate: rate,
            out_channels,
            block: BLOCK,
            channel_mode: ChannelMode::First(1),
            latency_frames: LATENCY_FRAMES,
        },
        meters.clone(),
        {
            let (_, rx) = rtrb::RingBuffer::<Cmd>::new(64);
            rx
        },
        {
            let (tx, _) = rtrb::RingBuffer::<dsp_graph::Executor>::new(8);
            tx
        },
        Arc::new(std::sync::atomic::AtomicBool::new(true)),
        handle.source.clone(),
    );
    engine.set_block(BLOCK);
    // The self-test drives the engine directly, so install the executor here
    // rather than sending it over the queue (the queue is empty).
    {
        let mut g = rig.to_doc().build(&handle.registry).expect("selftest rig builds");
        let ex = dsp_graph::Executor::compile_owned(&mut g, rate, BLOCK).expect("selftest rig compiles");
        engine.set_executor(ex);
    }
    let mut peak = 0.0f32;
    let mut sum_sq = 0.0f64;
    let mut nonfinite = 0usize;
    let mut frames = 0usize;
    let start = Instant::now();
    let seconds = 4.0;
    let total = (rate * seconds) as usize;
    while frames < total {
        let n = BLOCK.min(total - frames);
        engine.fill_source(n);
        let out = engine.render(n);
        for &v in out {
            if !v.is_finite() {
                nonfinite += 1;
            }
            peak = peak.max(v.abs());
            sum_sq += (v * v) as f64;
        }
        frames += n;
    }
    let elapsed = start.elapsed();
    let cpu = elapsed.as_secs_f64() / seconds as f64;
    println!("selftest: {frames} frames in {elapsed:?} ({:.1}% of one core)", cpu * 100.0);
    let rms = (sum_sq / frames as f64).sqrt();
    println!("selftest: peak {peak:.3}  rms {rms:.4}  non-finite {nonfinite}");
    println!("selftest: xruns {}", meters.xruns.load(Ordering::Relaxed));
    if nonfinite > 0 || peak <= 0.0 {
        eprintln!("selftest FAILED");
        std::process::exit(1);
    }
    println!("selftest OK");
    std::process::exit(0);
}

// ---------------------------------------------------------------------------
// eframe app
// ---------------------------------------------------------------------------

struct App {
    handle: Handle,
    store: Store,
    rig: Rig,
    state: UiState,
}

impl App {
    fn new(handle: Handle, store: Store, rig: Rig, state: UiState) -> Self {
        Self { handle, store, rig, state }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.handle.collect_trash();
        egui::containers::Panel::top("top").show(ui, |ui| {
            ui::top_bar(ui, &self.handle, &mut self.store, &mut self.rig, &mut self.state, &self.handle.meters);
        });
        egui::containers::Panel::left("inspector").resizable(true).default_size(260.0).show(ui, |ui| {
            ui.set_min_width(220.0);
            match &self.state.selected {
                Some((channel, block)) => {
                    let spec = self
                        .handle
                        .registry
                        .get(&self.rig.channel(channel).and_then(|c| c.blocks.iter().find(|b| &b.id == block)).map(|b| b.block.clone()).unwrap_or_default())
                        .cloned();
                    if let Some(spec) = spec {
                        ui::inspector(ui, &mut self.rig, &(channel.clone(), block.clone()), &spec, &self.handle);
                    }
                }
                None => {
                    ui.label("Select a block to edit its parameters.");
                    ui.separator();
                    if ui.button("Add channel").clicked() {
                        if let Some(id) = self.rig.add_channel() {
                            self.state.status = format!("added {id}");
                            let _ = rebuild(&self.handle, &self.rig);
                        }
                    }
                }
            }
        });
        egui::containers::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.horizontal(|ui| {
                    let ids: Vec<String> = self.rig.channels.iter().map(|c| c.id.clone()).collect();
                    for id in ids {
                        egui::Frame::group(ui.style()).show(ui, |ui| {
                            ui.set_min_width(ui::BLOCK_W + 32.0);
                            ui::channel_strip(ui, &self.handle, &mut self.rig, &mut self.state, &id, &self.handle.registry);
                        });
                        ui.separator();
                    }
                });
            });
        });
        // Continuous repaint: the meters move.
        ctx.request_repaint_after(Duration::from_millis(50));
    }
}
