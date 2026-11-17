//! Audio device selection and stream construction, shared by the engine
//! process and the egui development app.

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use rtrb::{Consumer, Producer};

use crate::audio::{run_callback, AudioThread, ChannelMode, Cmd, FromF32, Meters, Ring, Setup};

/// Mono samples buffered between the input and output callbacks.
///
/// The input callback pushes one mono sample per frame and the output
/// callback drains this ring on every block, so in steady state the level is
/// ~0 and the capacity is only ever a *backlog guard* (if the output thread
/// lags the input, the oldest samples are dropped rather than allowed to
/// pile up). It is therefore capped at ~2 blocks: the previous 8192 frames
/// (~170 ms at 48 kHz) added no audible latency but was a large, needless
/// inter-thread queue that could overflow on a slow output, so it is gone.
/// Capacity is deliberately small so a falling output can exhaust the ring
/// and starve (silence) instead of buffering forever.
pub const RING_CAPACITY: usize = 128;
/// Initial buffer level: samples discarded from the front of the ring at
/// stream start so the very first output blocks are not silence. A few
/// blocks of lead is enough — the ~1024 (21 ms) default was far larger than
/// any device needed, added audible boot lag, and made startup latency
/// predictable. 128 frames (~2.7 ms at 48 kHz) is kept here so the UI ring
/// level reflects reality.
pub const LATENCY_FRAMES: usize = 128;
/// The tuner ring is a separate buffer (input -> tuner_thread) and is NOT on
/// the input->output audio signal path, so it is not part of the 1-2 block
/// budget. It must be at least one YIN window (the tuner only drains every
/// 20 ms) to avoid overwriting the window while it drains.
pub const TUNER_RING_CAPACITY: usize = 2048;
/// Block size the executor runs in.
pub const BLOCK: usize = 64;

pub fn device_name(d: &cpal::Device) -> String {
    d.description().map(|d| d.name().to_string()).unwrap_or_else(|_| "unknown".into())
}

pub fn pick_input(host: &cpal::Host) -> (cpal::Device, String) {
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

pub fn pick_output(host: &cpal::Host, prefer: &str) -> (cpal::Device, String) {
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
        .or_else(|| {
            devices
                .iter()
                .position(|d| device_name(d).contains("MacBook") || device_name(d).contains("Built-in"))
        })
        .unwrap_or(0);
    let name = device_name(&devices[pick]);
    eprintln!("using output: {name}");
    (devices[pick].clone(), name)
}

/// Prefer 48 kHz, then 44.1 kHz, but only if both devices support it.
pub fn pick_rate(in_dev: &cpal::Device, out_dev: &cpal::Device, in_rate: u32, out_rate: u32) -> u32 {
    for r in [48_000, 44_100] {
        let i_ok = in_dev
            .supported_input_configs()
            .map(|mut c| c.any(|c| c.min_sample_rate() <= r && r <= c.max_sample_rate()))
            .unwrap_or(false);
        let o_ok = out_dev
            .supported_output_configs()
            .map(|mut c| c.any(|c| c.min_sample_rate() <= r && r <= c.max_sample_rate()))
            .unwrap_or(false);
        if i_ok && o_ok {
            return r;
        }
    }
    if in_rate == out_rate {
        return in_rate;
    }
    in_rate
}

/// The `cpal` sample conversions, as a trait so the stream builders can be
/// generic over the device's sample format.
pub trait ToF32: cpal::SizedSample {
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

pub fn build_input<T: ToF32 + 'static>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    channels: usize,
    ring: Arc<Ring>,
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
        |_err| {{
        }},
        Some(Duration::from_millis(500)),
    )
}

#[allow(clippy::too_many_arguments)]
pub fn build_output<T: FromF32 + cpal::SizedSample + 'static>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    channels: usize,
    ring: Arc<Ring>,
    cmd_rx: Consumer<Cmd>,
    trash_tx: Producer<dsp_graph::Executor>,
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
            run_callback(&mut engine, &ring, &meters, data, channels);
        },
        |_err| {{
        }},
        Some(Duration::from_millis(500)),
    )
}

/// Everything the stream builders need, picked once so the executor and the
/// audio thread agree on the sample rate.
pub struct Devices {
    pub in_dev: cpal::Device,
    pub out_dev: cpal::Device,
    pub in_cfg: cpal::StreamConfig,
    pub out_cfg: cpal::StreamConfig,
    pub in_format: cpal::SampleFormat,
    pub out_format: cpal::SampleFormat,
    pub rate: u32,
    pub in_channels: usize,
    pub out_channels: usize,
    pub in_name: String,
    pub out_name: String,
}

/// Pick the input and output devices and a sample rate both support.
pub fn pick() -> Devices {
    let host = cpal::default_host();
    let (in_dev, in_name) = pick_input(&host);
    let (out_dev, out_name) = pick_output(&host, &in_name);
    let in_default = in_dev.default_input_config().expect("no input config");
    let out_default = out_dev.default_output_config().expect("no output config");
    let rate = pick_rate(&in_dev, &out_dev, in_default.sample_rate(), out_default.sample_rate());
    let out_channels = out_default.channels().max(1) as usize;
    let in_channels = in_default.channels().max(1) as usize;
    // The sample format lives on the supported config, not on StreamConfig.
    let in_format = in_default.sample_format();
    let out_format = out_default.sample_format();

    let mut in_cfg: cpal::StreamConfig = in_default.into();
    in_cfg.sample_rate = rate;
    let mut out_cfg: cpal::StreamConfig = out_default.into();
    out_cfg.sample_rate = rate;

    Devices {
        in_dev,
        out_dev,
        in_cfg,
        out_cfg,
        in_format,
        out_format,
        rate,
        in_channels,
        out_channels,
        in_name,
        out_name,
    }
}

/// Start both streams. The ring is returned so the caller can trim it.
#[allow(clippy::too_many_arguments)]
pub fn start(
    d: &Devices,
    cmd_rx: Consumer<Cmd>,
    trash_tx: Producer<dsp_graph::Executor>,
    running: Arc<std::sync::atomic::AtomicBool>,
    meters: Arc<Meters>,
    source: Arc<AtomicU8>,
) -> Arc<Ring> {
    let in_channels = d.in_channels;
    let out_channels = d.out_channels;
    let ring = Arc::new(Ring::new(RING_CAPACITY));
    let ring_in = ring.clone();
    let in_format = d.in_format;
    let in_stream = match in_format {
        cpal::SampleFormat::F32 => build_input::<f32>(&d.in_dev, d.in_cfg, in_channels, ring_in, meters.clone()),
        cpal::SampleFormat::I16 => build_input::<i16>(&d.in_dev, d.in_cfg, in_channels, ring_in, meters.clone()),
        cpal::SampleFormat::I32 => build_input::<i32>(&d.in_dev, d.in_cfg, in_channels, ring_in, meters.clone()),
        cpal::SampleFormat::U16 => build_input::<u16>(&d.in_dev, d.in_cfg, in_channels, ring_in, meters.clone()),
        other => panic!("unsupported input format {other:?}"),
    }
    .expect("failed to build input stream");
    in_stream.play().expect("failed to start input");
    std::mem::forget(in_stream);

    let ring_out = ring.clone();
    let out_format = d.out_format;
    let out_stream = match out_format {
        cpal::SampleFormat::F32 => build_output::<f32>(
            &d.out_dev, d.out_cfg, out_channels, ring_out, cmd_rx, trash_tx, running, meters.clone(), source,
        ),
        cpal::SampleFormat::I16 => build_output::<i16>(
            &d.out_dev, d.out_cfg, out_channels, ring_out, cmd_rx, trash_tx, running, meters.clone(), source,
        ),
        cpal::SampleFormat::I32 => build_output::<i32>(
            &d.out_dev, d.out_cfg, out_channels, ring_out, cmd_rx, trash_tx, running, meters.clone(), source,
        ),
        cpal::SampleFormat::U16 => build_output::<u16>(
            &d.out_dev, d.out_cfg, out_channels, ring_out, cmd_rx, trash_tx, running, meters.clone(), source,
        ),
        other => panic!("unsupported output format {other:?}"),
    }
    .expect("failed to build output stream");
    out_stream.play().expect("failed to start output");
    std::mem::forget(out_stream);
    ring.trim_to(LATENCY_FRAMES);

    eprintln!("input : {} @ {} Hz, {in_channels} ch", d.in_name, d.rate);
    eprintln!("output: {} @ {} Hz, {out_channels} ch", d.out_name, d.rate);
    ring
}
