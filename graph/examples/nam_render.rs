//! Render dry and wet WAVs from a real .nam so the sound can be judged.
//! Usage: cargo run -p dsp-graph --example nam_render -- <model.nam> <out.wav>

use std::io::Write;

fn write_wav(path: &str, samples: &[f32], rate: u32) {
    let mut buf: Vec<u8> = Vec::new();
    let n = samples.len() as u32;
    let data_len = n * 2;
    let byte_rate = rate * 2;
    let file_len = 36 + data_len;
    buf.extend_from_slice(b"RIFF");
    buf.extend_from_slice(&file_len.to_le_bytes());
    buf.extend_from_slice(b"WAVEfmt ");
    buf.extend_from_slice(&16u32.to_le_bytes());
    buf.extend_from_slice(&1u16.to_le_bytes()); // PCM
    buf.extend_from_slice(&1u16.to_le_bytes()); // mono
    buf.extend_from_slice(&rate.to_le_bytes());
    buf.extend_from_slice(&byte_rate.to_le_bytes());
    buf.extend_from_slice(&2u16.to_le_bytes()); // block align
    buf.extend_from_slice(&16u16.to_le_bytes()); // bits
    buf.extend_from_slice(b"data");
    buf.extend_from_slice(&data_len.to_le_bytes());
    for &s in samples {
        let v = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
        buf.extend_from_slice(&v.to_le_bytes());
    }
    let mut f = std::fs::File::create(path).unwrap();
    f.write_all(&buf).unwrap();
}

fn main() {
    let path = std::env::args().nth(1).expect("usage: nam_render <model.nam> <out.wav>");
    let out_path = std::env::args().nth(2).expect("usage: nam_render <model.nam> <out.wav>");
    let rate = 48_000.0f32;
    let block = 64usize;

    let nam = dsp_graph::blocks::Nam::default();
    nam.load_file(&path).expect("load nam");

    let mut g = dsp_graph::Graph::new();
    g.add("in", Box::new(dsp_graph::blocks::IoInput)).unwrap();
    g.add("nam", Box::new(nam)).unwrap();
    g.add("out", Box::new(dsp_graph::blocks::IoOutput)).unwrap();
    g.connect("in", "out", "nam", "in").unwrap();
    g.connect("nam", "out", "out", "in").unwrap();
    let mut ex = dsp_graph::Executor::compile_owned(&mut g, rate, block).unwrap();

    // 8 seconds of a riff: E5 power chord, then A5, then B5, palm-muted-ish.
    let secs = 8.0;
    let n = (secs * rate as f64) as usize;
    let mut input = vec![0.0f32; n];
    for (i, s) in input.iter_mut().enumerate() {
        let t = i as f32 / rate;
        let bar = (t * 2.0) as usize % 4;
        let f = [82.41, 110.0, 123.47, 146.83][bar];
        let decay = (-(t * 2.0).fract() * 8.0).exp();
        let saw = 2.0 * (t * f).fract() - 1.0;
        let fifth = 2.0 * (t * f * 1.5).fract() - 1.0;
        *s = 0.25 * (saw + 0.6 * fifth) * decay;
    }

    let mut wet = vec![0.0f32; n];
    for (ci, chunk) in input.chunks_exact(block).enumerate() {
        let o = ex.run(chunk, block).unwrap();
        wet[ci * block..ci * block + block].copy_from_slice(o);
    }

    let peak = wet.iter().fold(0.0f32, |a, &b| a.max(b.abs()));
    if peak > 0.0 {
        let norm = 0.95 / peak;
        for s in wet.iter_mut() {
            *s *= norm;
        }
    }
    // Dry reference at the same scale, so the two files can be A/B'd.
    let mut dry = input.clone();
    let dry_peak = dry.iter().fold(0.0f32, |a, &b| a.max(b.abs()));
    if dry_peak > 0.0 {
        let norm = 0.95 / dry_peak;
        for s in dry.iter_mut() {
            *s *= norm;
        }
    }
    let dry_path = format!("{}.dry.wav", out_path);
    write_wav(&dry_path, &dry, rate as u32);
    write_wav(&out_path, &wet, rate as u32);
    let rms = (wet.iter().map(|&v| (v * v) as f64).sum::<f64>() / n as f64).sqrt();
    println!("wrote {out_path}: {n} samples, peak {peak:.3}, rms {rms:.4}");
    println!("wrote {dry_path} (dry reference)");
}
