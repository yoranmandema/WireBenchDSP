//! The pedal development app: the egui editor over the shared engine.
//!
//! ```text
//!   preset dir ──▶ Rig ──▶ Doc ──▶ Graph ──▶ Executor ──▶ audio thread
//!                   ▲                                      │
//!                   └────────── params / bypass ◀───────────┘
//! ```
//!
//! The engine (state, audio, device selection) lives in `pedal-engine`; this
//! crate is the UI. The production UI is the Preact app in `ui/`, which talks
//! to the engine process over IPC.

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use pedal_app::{audio, channels, store, ui};

use audio::{AudioThread, ChannelMode, Cmd, Handle, Meters, Setup};
use channels::Rig;
use store::Store;
use ui::{rebuild, UiState};

const LATENCY_FRAMES: usize = pedal_engine::host::LATENCY_FRAMES;
const BLOCK: usize = pedal_engine::host::BLOCK;

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
        .unwrap_or_else(|| channels::Rig::new("Untitled").to_doc());
    let rig = Rig::from_doc(&doc);

    let devices = pedal_engine::host::pick();
    let setup = Setup {
        sample_rate: devices.rate as f32,
        out_channels: devices.out_channels,
        block: BLOCK,
        channel_mode: ChannelMode::First(2.min(devices.in_channels)),
        latency_frames: LATENCY_FRAMES,
    };
    let source = Arc::new(AtomicU8::new(2)); // silence by default; the riff is opt-in
    let (handle, cmd_rx, trash_tx, running) = audio::new_engine(setup, source.clone());
    let handle = {
        let mut h = handle;
        h.preset_dir = store.dir.clone();
        h
    };
    let meters = handle.meters.clone();
    // Compile the initial rig before the streams start.
    let block = handle.block();
    {
        let mut g = rig
            .to_doc()
            .build_in(&handle.registry, &handle.preset_dir)
            .expect("initial rig builds");
        rig.apply_routings(&mut g, &handle.registry)
            .expect("initial rig routings apply");
        let ex = dsp_graph::Executor::compile_owned(&mut g, setup.sample_rate, block)
            .expect("initial rig compiles");
        *handle.node_ids.lock().unwrap() = ex.node_ids().to_vec();
        handle.push(Cmd::Swap(ex, Vec::new())).expect("initial swap");
    }
    handle.set_source(2); // silence

    if selftest {
        selftest_run(handle, meters, setup.sample_rate, devices.out_channels, &rig)
    }

    pedal_engine::host::start(&devices, cmd_rx, trash_tx, running, meters.clone(), source);

    if headless {
        eprintln!("headless: running for 60 s");
        std::thread::sleep(Duration::from_secs(60));
        return;
    }

    let mut state = UiState::default();
    state.doc_name = preset_name.clone();
    state.status = format!("{preset_name} · {} Hz", devices.rate);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 760.0])
            .with_title("Pedal"),
        ..Default::default()
    };
    eframe::run_native(
        "Pedal",
        options,
        Box::new(move |_cc| Ok(Box::new(App::new(handle, store, rig, state)))),
    )
    .expect("eframe failed");
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
    {
        let mut g = rig
            .to_doc()
            .build_in(&handle.registry, &handle.preset_dir)
            .expect("selftest rig builds");
        rig.apply_routings(&mut g, &handle.registry)
            .expect("selftest rig routings apply");
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
        // Delete removes the selected block.
        if ui.input(|i| i.key_pressed(egui::Key::Delete)) {
            if let Some((channel, block)) = self.state.selected.clone() {
                self.rig.remove_block(&channel, &block);
                self.state.selected = None;
                self.state.dirty = true;
                let _ = rebuild(&self.handle, &self.rig);
            }
        }
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
                        ui::inspector(ui, &mut self.rig, &(channel.clone(), block.clone()), &spec, &self.handle, &mut self.state);
                    }
                }
                None => {
                    ui.label("Select a block to edit its parameters.");
                }
            }
        });
        egui::containers::CentralPanel::default().show(ui, |ui| {
            // Channels are rows, top to bottom; each strip scrolls sideways.
            egui::ScrollArea::vertical().show(ui, |ui| {
                let ids: Vec<String> = self.rig.channels.iter().map(|c| c.id.clone()).collect();
                for id in ids {
                    egui::Frame::group(ui.style()).show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        ui::channel_strip(ui, &self.handle, &mut self.rig, &mut self.state, &id, &self.handle.registry);
                    });
                    ui.add_space(8.0);
                }
            });
        });
        // Continuous repaint: the meters move.
        ctx.request_repaint_after(Duration::from_millis(50));
    }
}
