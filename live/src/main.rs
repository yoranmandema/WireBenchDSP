//! Live pedal model: the *same* `pedal_dsp::chain::EffectChain` that runs on
//! the STM32H723, driven in real time from a GUI.
//!
//!   * audio out via CoreAudio (cpal)
//!   * source = a looped WAV, a test tone, or silence
//!   * every parameter is an atomic, so the GUI thread never blocks the
//!     audio callback
//!
//! Usage: `cargo run --release`

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use pedal_dsp::chain::EffectChain;
use pedal_dsp::waveshape::ShaperKind;

const TAU: f32 = std::f32::consts::TAU;

// ---------------------------------------------------------------------------
// Parameters: atomics so the GUI can write and the audio thread can read
// without any lock in the audio path.
// ---------------------------------------------------------------------------

pub struct Params {
    pub drive: AtomicU32,
    pub distortion_mix: AtomicU32,
    pub distortion_level: AtomicU32,
    pub tone: AtomicU32,
    pub level: AtomicU32,
    pub delay_ms: AtomicU32,
    pub delay_mix: AtomicU32,
    pub delay_feedback: AtomicU32,
    pub reverb_mix: AtomicU32,
    pub shaper: AtomicU32,
    pub enabled: AtomicBool,
}

pub const SHAPERS: &[(&str, ShaperKind)] = &[
    ("Tanh", ShaperKind::Tanh),
    ("Soft clip", ShaperKind::SoftClip),
    ("Hard clip", ShaperKind::HardClip),
    ("Tube", ShaperKind::Tube),
    ("Fuzz", ShaperKind::Fuzz),
    ("Half rectify", ShaperKind::RectifyHalf),
    ("Full rectify", ShaperKind::RectifyFull),
];

impl Params {
    fn new() -> Self {
        Self {
            drive: AtomicU32::new(3.0f32.to_bits()),
            distortion_mix: AtomicU32::new(1.0f32.to_bits()),
            distortion_level: AtomicU32::new(0.8f32.to_bits()),
            tone: AtomicU32::new(0.0f32.to_bits()),
            level: AtomicU32::new(0.9f32.to_bits()),
            delay_ms: AtomicU32::new(250.0f32.to_bits()),
            delay_mix: AtomicU32::new(0.0f32.to_bits()),
            delay_feedback: AtomicU32::new(0.35f32.to_bits()),
            reverb_mix: AtomicU32::new(0.0f32.to_bits()),
            shaper: AtomicU32::new(0),
            enabled: AtomicBool::new(true),
        }
    }
}

#[inline]
fn get_f32(a: &AtomicU32) -> f32 {
    f32::from_bits(a.load(Ordering::Relaxed))
}
#[inline]
fn set_f32(a: &AtomicU32, v: f32) {
    a.store(v.to_bits(), Ordering::Relaxed);
}

// ---------------------------------------------------------------------------
// Audio source
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
enum ToneKind {
    Sine,
    Saw,
    Square,
}

enum Source {
    Silence,
    Tone { kind: ToneKind, freq: f32, phase: f32 },
    Wav { samples: Vec<f32>, pos: usize },
}

impl Source {
    fn sample(&mut self, fs: f32) -> f32 {
        match self {
            Source::Silence => 0.0,
            Source::Tone { kind, freq, phase } => {
                let s = match kind {
                    ToneKind::Sine => (TAU * *phase).sin(),
                    ToneKind::Saw => 2.0 * *phase - 1.0,
                    ToneKind::Square => {
                        if *phase < 0.5 {
                            1.0
                        } else {
                            -1.0
                        }
                    }
                };
                *phase = (*phase + *freq / fs).fract();
                s * 0.35
            }
            Source::Wav { samples, pos } => {
                if samples.is_empty() {
                    return 0.0;
                }
                let s = samples[*pos];
                *pos = (*pos + 1) % samples.len();
                s
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Shared audio state
// ---------------------------------------------------------------------------

struct AudioState {
    chain: EffectChain,
    source: Source,
    channels: usize,
    fs: f32,
    peak: Arc<Mutex<f32>>,
    cb_count: u64,
    last_print: std::time::Instant,
}

impl AudioState {
    fn new(fs: f32, channels: usize, params: &Params, peak: Arc<Mutex<f32>>) -> Self {
        let mut state = Self {
            chain: EffectChain::new(fs),
            source: Source::Silence,
            channels,
            fs,
            peak,
            cb_count: 0,
            last_print: std::time::Instant::now(),
        };
        state.apply_params(params);
        state
    }

    fn apply_params(&mut self, p: &Params) {
        self.chain.drive = get_f32(&p.drive);
        self.chain.distortion_mix = get_f32(&p.distortion_mix);
        self.chain.distortion_level = get_f32(&p.distortion_level);
        self.chain.tone = get_f32(&p.tone);
        self.chain.level = get_f32(&p.level);
        self.chain.delay_ms = get_f32(&p.delay_ms);
        self.chain.delay_mix = get_f32(&p.delay_mix);
        self.chain.delay_feedback = get_f32(&p.delay_feedback);
        self.chain.reverb_mix = get_f32(&p.reverb_mix);
        self.chain.enabled = p.enabled.load(Ordering::Relaxed);
        let idx = p.shaper.load(Ordering::Relaxed) as usize % SHAPERS.len();
        self.chain.set_shaper(SHAPERS[idx].1);
        self.chain.update_params();
    }

    fn process(&mut self, p: &Params, out: &mut [f32]) {
        self.apply_params(p);
        let mut peak = 0.0f32;
        for frame in out.chunks_mut(self.channels.max(1)) {
            let s = self.source.sample(self.fs);
            let (l, r) = self.chain.process_stereo(s, s);
            peak = peak.max(l.abs()).max(r.abs());
            if frame.len() >= 2 {
                frame[0] = l;
                frame[1] = r;
            } else {
                frame[0] = (l + r) * 0.5;
            }
        }
        if let Ok(mut pk) = self.peak.lock() {
            *pk = (*pk * 0.9 + peak * 0.1).max(peak);
        }
        self.cb_count += 1;
        if self.last_print.elapsed().as_secs_f32() > 1.0 {
            eprintln!("audio: {} callbacks, peak {:.3}", self.cb_count, peak);
            self.last_print = std::time::Instant::now();
        }
    }
}

// ---------------------------------------------------------------------------
// cpal glue
// ---------------------------------------------------------------------------

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

fn build_stream<T: FromF32 + 'static>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    state: Arc<Mutex<AudioState>>,
    params: Arc<Params>,
) -> Result<cpal::Stream, cpal::Error> {
    device.build_output_stream(
        config,
        move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
            let mut st = state.lock().unwrap();
            let mut buf = vec![0.0f32; data.len()];
            st.process(&params, &mut buf);
            for (o, s) in data.iter_mut().zip(buf.iter()) {
                *o = T::from_f32(*s);
            }
        },
        |err| eprintln!("audio callback error: {err}"),
        // A timeout is essential: cpal may call `set_sample_rate` with this
        // value, and `None` can block indefinitely on some CoreAudio devices.
        Some(std::time::Duration::from_millis(500)),
    )
}

// ---------------------------------------------------------------------------
// GUI
// ---------------------------------------------------------------------------

struct App {
    params: Arc<Params>,
    state: Arc<Mutex<AudioState>>,
    source_name: String,
    shaper_idx: usize,
}

impl App {
    fn new(state: Arc<Mutex<AudioState>>, params: Arc<Params>, source_name: String) -> Self {
        Self {
            params,
            state,
            source_name,
            shaper_idx: 0,
        }
    }
}

fn slider(ui: &mut egui::Ui, label: &str, value: &AtomicU32, range: std::ops::RangeInclusive<f32>) {
    let mut v = get_f32(value);
    if ui.add(egui::Slider::new(&mut v, range).text(label)).changed() {
        set_f32(value, v);
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::containers::Panel::left("controls")
            .resizable(true)
            .show(ui, |ui| {
                ui.heading("Pedal DSP — live");
                ui.label(egui::RichText::new("same code as the STM32H723 firmware").weak());
                ui.separator();

                let mut bypass = !self.params.enabled.load(Ordering::Relaxed);
                if ui.checkbox(&mut bypass, "Bypass").changed() {
                    self.params.enabled.store(!bypass, Ordering::Relaxed);
                }

                ui.add_space(6.0);
                ui.label(egui::RichText::new("Distortion").strong());
                slider(ui, "Drive", &self.params.drive, 0.1..=20.0);
                slider(ui, "Mix", &self.params.distortion_mix, 0.0..=1.0);
                slider(ui, "Level", &self.params.distortion_level, 0.0..=2.0);
                slider(ui, "Tone", &self.params.tone, -1.0..=1.0);

                ui.add_space(6.0);
                ui.label(egui::RichText::new("Time").strong());
                slider(ui, "Delay ms", &self.params.delay_ms, 20.0..=1000.0);
                slider(ui, "Delay mix", &self.params.delay_mix, 0.0..=1.0);
                slider(ui, "Feedback", &self.params.delay_feedback, 0.0..=0.95);
                slider(ui, "Reverb mix", &self.params.reverb_mix, 0.0..=1.0);

                ui.add_space(6.0);
                ui.label(egui::RichText::new("Output").strong());
                slider(ui, "Master", &self.params.level, 0.0..=2.0);

                ui.add_space(10.0);
                ui.label(egui::RichText::new("Shaper").strong());
                egui::ComboBox::from_id_salt("shaper")
                    .selected_text(SHAPERS[self.shaper_idx].0)
                    .show_ui(ui, |ui| {
                        for (i, (name, _)) in SHAPERS.iter().enumerate() {
                            if ui.selectable_value(&mut self.shaper_idx, i, *name).changed() {
                                self.params.shaper.store(i as u32, Ordering::Relaxed);
                            }
                        }
                    });

                ui.add_space(10.0);
                ui.label(format!("Source: {}", self.source_name));
            });

        egui::containers::CentralPanel::default().show(ui, |ui| {
            ui.heading("Output");
            let peak = self
                .state
                .lock()
                .map(|s| *s.peak.lock().unwrap())
                .unwrap_or(0.0);
            ui.add(egui::ProgressBar::new(peak.clamp(0.0, 1.0)).text(format!("peak {peak:.2}")));
            ui.label("Drag the sliders while audio plays. Parameters are read by the audio callback atomically — no dropouts.");
            ui.label(egui::RichText::new("Tip: the guitar WAV is the default source — drive it into the distortion.").weak());
        });
    }
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

fn main() -> eframe::Result<()> {
    let host = cpal::default_host();

    // Enumerate output devices so we can avoid one that is busy or hanging
    // (e.g. an external interface held by another application).
    let mut devices: Vec<(String, cpal::Device)> = Vec::new();
    if let Ok(iter) = host.output_devices() {
        for d in iter {
            if let Ok(desc) = d.description() {
                devices.push((desc.name().to_string(), d));
            }
        }
    }
    if devices.is_empty() {
        panic!("no audio output devices found");
    }
    eprintln!("output devices:");
    for (i, (name, _)) in devices.iter().enumerate() {
        eprintln!("  [{i}] {name}");
    }

    // Prefer the built-in speakers; fall back to the first device.
    let idx = devices
        .iter()
        .position(|(n, _)| n.contains("MacBook") || n.contains("Built-in") || n.contains("Speakers"))
        .unwrap_or(0);
    let device_name = devices[idx].0.clone();
    let device = &devices[idx].1;
    eprintln!("using output device: {device_name}");

    let config = device
        .default_output_config()
        .expect("no default output configuration");
    let fs = config.sample_rate() as f32;
    let channels = config.channels() as usize;
    eprintln!("output: {} Hz, {} ch, {:?}", fs, channels, config.sample_format());

    let params = Arc::new(Params::new());
    let peak = Arc::new(Mutex::new(0.0f32));
    let state = Arc::new(Mutex::new(AudioState::new(fs, channels, &params, Arc::clone(&peak))));

    // Pick a source: the guitar WAV if present, else a test tone.
    let wav_path = std::path::Path::new("/Users/yoran/Downloads/3-Audio 0010 [2026-11-03 215835].wav");
    let (source, source_name) = if wav_path.exists() {
        match hound::WavReader::open(wav_path) {
            Ok(mut r) => {
                let spec = r.spec();
                let samples: Vec<f32> = match (spec.sample_format, spec.bits_per_sample) {
                    (hound::SampleFormat::Int, 16) => r
                        .samples::<i16>()
                        .map(|s| s.unwrap() as f32 / 32768.0)
                        .collect(),
                    (hound::SampleFormat::Int, 24) => r
                        .samples::<i32>()
                        .map(|s| s.unwrap() as f32 / 8_388_608.0)
                        .collect(),
                    (hound::SampleFormat::Float, 32) => {
                        r.samples::<f32>().map(|s| s.unwrap()).collect()
                    }
                    _ => Vec::new(),
                };
                (
                    Source::Wav { samples, pos: 0 },
                    "guitar WAV (looped)".to_string(),
                )
            }
            Err(e) => {
                eprintln!("could not read WAV: {e}");
                (
                    Source::Tone { kind: ToneKind::Saw, freq: 110.0, phase: 0.0 },
                    "110 Hz saw (WAV unreadable)".to_string(),
                )
            }
        }
    } else {
        (
            Source::Tone { kind: ToneKind::Saw, freq: 110.0, phase: 0.0 },
            "110 Hz saw".to_string(),
        )
    };

    {
        let mut st = state.lock().unwrap();
        st.source = source;
    }

    let stream_config: cpal::StreamConfig = config.into();
    let sample_format = config.sample_format();
    eprintln!("building stream ({sample_format:?})...");
    let stream = match sample_format {
        cpal::SampleFormat::F32 => {
            build_stream::<f32>(&device, stream_config, Arc::clone(&state), Arc::clone(&params))
        }
        cpal::SampleFormat::I16 => {
            build_stream::<i16>(&device, stream_config, Arc::clone(&state), Arc::clone(&params))
        }
        cpal::SampleFormat::I32 => {
            build_stream::<i32>(&device, stream_config, Arc::clone(&state), Arc::clone(&params))
        }
        cpal::SampleFormat::U16 => {
            build_stream::<u16>(&device, stream_config, Arc::clone(&state), Arc::clone(&params))
        }
        other => panic!("unsupported output sample format: {other:?}"),
    }
    .expect("failed to build output stream");
    stream.play().expect("failed to start audio");
    eprintln!("stream playing");

    // Keep the stream alive for the lifetime of the app.
    std::mem::forget(stream);

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([900.0, 520.0])
            .with_title("Pedal DSP — live"),
        ..Default::default()
    };

    eprintln!("starting UI...");
    eframe::run_native(
        "Pedal DSP — live",
        options,
        Box::new(move |_cc| Ok(Box::new(App::new(state, params, source_name)))),
    )
}
