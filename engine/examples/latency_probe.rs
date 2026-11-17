//! Latency instrument probe for the pedal engine.
//!
//! Measures the device-level audio buffering — the dominant latency stage in
//! the cpal/CoreAudio path — and prints the configuration the engine uses.
//! It reads device capabilities *before* any stream is opened, so no audio is
//! played and nothing is routed to a speaker: purely a config query.
//!
//! Run: `cargo run --example latency_probe`

use cpal::traits::{DeviceTrait, HostTrait};

fn ms_at(frames: f32, rate: f32) -> f32 {
    frames / rate * 1000.0
}

fn list_configs(device: &cpal::Device, kind: &str, in_out: &str) {
    eprintln!("  {kind} configs, {in_out}:");
    // Supported input and output ranges (buffer frames in samples).
    let in_ranges: Vec<_> = device
        .supported_input_configs()
        .map(|c| c.collect::<Vec<_>>())
        .unwrap_or_default();
    let out_ranges: Vec<_> = device
        .supported_output_configs()
        .map(|c| c.collect::<Vec<_>>())
        .unwrap_or_default();
    let mut ranges = in_ranges;
    ranges.extend(out_ranges);
    if ranges.is_empty() {
        eprintln!("    (none)");
    } else {
        for r in &ranges {
            let rate = r.min_sample_rate() as f32;
            let rbuf = r.buffer_size();
            eprintln!(
                "    {} ch {} Hz: buffer {}",
                r.channels(),
                rate,
                if let cpal::SupportedBufferSize::Range { min, max } = rbuf {
                    format!(
                        "{min}..{max} frames ({:.1}..{:.1} ms)",
                        ms_at(*min as f32, rate), ms_at(*max as f32, rate)
                    )
                } else {
                    "unknown".to_string()
                }
            );
        }
    }
    // The engine opens the stream with cpal's BufferSize::Default, which the
    // device picks as its preferred size (not reported here; see the range).
    let def = device
        .default_input_config()
        .map(|d| format!(
            "default input: {} ch {} Hz, format {:?}",
            d.channels(), d.sample_rate(), d.sample_format()
        ))
        .unwrap_or_else(|e| format!("default input config unavailable: {e}"));
    eprintln!("  defaults: {def}");
}

fn main() {
    let host = cpal::default_host();
    let in_devices: Vec<_> = host.input_devices().expect("no input devices").collect();
    let out_devices: Vec<_> = host.output_devices().expect("no output devices").collect();

    eprintln!("=== pedal-engine cpal device probe ===");
    let host_label = host
        .default_output_device()
        .and_then(|d| d.id().ok())
        .map(|i| i.to_string())
        .unwrap_or_else(|| "unknown".to_string());
    eprintln!("cpal host: {host_label}");
    eprintln!("input devices ({}):", in_devices.len());
    for (i, d) in in_devices.iter().enumerate() {
        let cfg = d.default_input_config().expect("input config");
        eprintln!(
            "  [{i}] {}  ({} Hz, {} ch)",
            d.description().map(|d| d.to_string()).unwrap_or_else(|e| format!("err({e})")),
            cfg.sample_rate(),
            cfg.channels()
        );
    }
    eprintln!("output devices ({}):", out_devices.len());
    for (i, d) in out_devices.iter().enumerate() {
        let cfg = d.default_output_config().expect("output config");
        eprintln!(
            "  [{i}] {}  ({} Hz, {} ch)",
            d.description().map(|d| d.to_string()).unwrap_or_else(|e| format!("err({e})")),
            cfg.sample_rate(),
            cfg.channels()
        );
    }

    // Replicate engine::host::pick() so the numbers shown are exactly what the
    // engine streams with.
    let in_dev = &in_devices[0];
    let out_dev = &out_devices[0];
    let in_default = in_dev.default_input_config().expect("no input config");
    let out_default = out_dev.default_output_config().expect("no output config");

    // Engine picks a common rate: 48k then 44.1k, else the input rate.
    let pick_rate = |_: f32, in_rate: f32| -> f32 {
        for r in [48_000.0, 44_100.0] {
            let i_ok = in_dev
                .supported_input_configs()
                .map(|mut c| {
                    c.any(|c| c.min_sample_rate() as f32 <= r && r <= c.max_sample_rate() as f32)
                })
                .unwrap_or(false);
            let o_ok = out_dev
                .supported_output_configs()
                .map(|mut c| {
                    c.any(|c| c.min_sample_rate() as f32 <= r && r <= c.max_sample_rate() as f32)
                })
                .unwrap_or(false);
            if i_ok && o_ok {
                return r;
            }
        }
        in_rate
    };
    let rate = pick_rate(out_default.sample_rate() as f32, in_default.sample_rate() as f32);
    eprintln!("\n=== engine pick ===");
    eprintln!(
        "sample rate: out default {out_default:?}, in default {in_default:?}, using {:.0} Hz",
        rate
    );
    eprintln!(
        "channels: out {:?}, in {:?}",
        out_default.channels(), in_default.channels()
    );
    eprintln!(
        "engine block = 64 frames ({:.2} ms @ {:.0} Hz)",
        64.0 / rate * 1000.0, rate
    );

    // Report what cpal would actually stream: the picked default configs carry
    // buffer_size = BufferSize::Default, i.e. the device's preferred size. See
    // the supported ranges for the frame counts that default actually lands on.
    for (i, d) in in_devices.iter().enumerate() {
        list_configs(d, "in", &format!("[{i}]"));
    }
    for (i, d) in out_devices.iter().enumerate() {
        list_configs(d, "out", &format!("[{i}]"));
    }

    // Per-stage computed latency (engine config constants).
    eprintln!("\n=== per-stage computed latency ===");
    eprintln!("input device default buffer: {:?} frames", out_default.buffer_size());
    eprintln!("output device default buffer: {:?} frames", out_default.buffer_size());
    eprintln!(
        "engine ring capacity    : 8192 frames ({} ms @ {:.0} Hz)",
        8192.0 / rate * 1000.0, rate
    );
    eprintln!("engine ring startup trim : 1024 frames ({:.2} ms)", 1024.0 / rate * 1000.0);
    eprintln!("engine block size        : 64 frames ({:.2} ms)", 64.0 / rate * 1000.0);
    eprintln!(
        "engine executor latency  : 0 samples (the pitch-delay effect's delay is intentional sound, not compensation)"
    );
}
