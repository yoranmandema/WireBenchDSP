//! Run a real .nam through the graph's Nam block and report what happens.
//! Usage: cargo run -p dsp-graph --example nam_check -- <model.nam>

fn main() {
    let path = std::env::args().nth(1).expect("usage: nam_check <model.nam> [rate] [out.raw]");
    let host_rate = std::env::args()
        .nth(2)
        .map(|s| s.parse::<f32>().expect("rate must be a number"))
        .unwrap_or(48_000.0);
    let dump_path = std::env::args().nth(3);
    let nam = dsp_graph::blocks::Nam::default();
    match nam.load_file(&path) {
        Ok(()) => println!("loaded: {}", nam.display_name()),
        Err(e) => {
            println!("LOAD FAILED: {e}");
            std::process::exit(1);
        }
    }
    println!("expected rate: {:?}", nam.expected_rate());
    println!("loaded: {}", nam.is_loaded());

    let mut g = dsp_graph::Graph::new();
    g.add("in", Box::new(dsp_graph::blocks::IoInput)).unwrap();
    g.add("nam", Box::new(nam)).unwrap();
    g.add("out", Box::new(dsp_graph::blocks::IoOutput)).unwrap();
    g.connect("in", "out", "nam", "in").unwrap();
    g.connect("nam", "out", "out", "in").unwrap();

    let mut ex = match dsp_graph::Executor::compile_owned(&mut g, host_rate, 64) {
        Ok(e) => e,
        Err(e) => {
            println!("COMPILE FAILED: {e}");
            std::process::exit(1);
        }
    };
    println!(
        "compiled: {} nodes, latency {}",
        ex.node_count(),
        ex.total_latency()
    );

    // A plucked-ish test signal: descending power chords, ~3 seconds.
    let n = (host_rate as usize) * 3;
    let mut input = vec![0.0f32; n];
    for (i, s) in input.iter_mut().enumerate() {
        let t = i as f32 / 48_000.0;
        let note = (t * 2.0) as usize % 4;
        let f = [82.4, 110.0, 146.8, 196.0][note];
        let saw = 2.0 * (t * f).fract() - 1.0;
        *s = 0.3 * saw * (-(t * 2.0).fract() * 6.0).exp();
    }

    let mut wet = Vec::with_capacity(n);
    for (ci, chunk) in input.chunks_exact(64).enumerate() {
        let out = match ex.run(chunk, 64) {
            Ok(o) => o,
            Err(e) => {
                println!("RUN FAILED at chunk {ci}: {e}");
                std::process::exit(1);
            }
        };
        wet.extend_from_slice(out);
    }

    if let Some(p) = dump_path {
        let mut f = std::fs::File::create(&p).expect("create dump");
        use std::io::Write;
        for &v in &wet {
            f.write_all(&v.to_le_bytes()).unwrap();
        }
        println!("dumped {} samples to {p}", wet.len());
    }

    let n = wet.len();
    let mut peak = 0.0f32;
    let mut sum_sq = 0.0f64;
    let mut nonfinite = 0usize;
    let mut dry_sq = 0.0f64;
    let mut diff_sq = 0.0f64;
    for (i, &v) in wet.iter().enumerate() {
        if !v.is_finite() {
            nonfinite += 1;
        }
        peak = peak.max(v.abs());
        sum_sq += (v * v) as f64;
        dry_sq += (input[i] * input[i]) as f64;
        let d = v - input[i];
        diff_sq += (d * d) as f64;
    }
    let rms = (sum_sq / n as f64).sqrt();
    // How much of the output is *not* explained by the input alone.
    let colour = (diff_sq / dry_sq.max(1e-12)).sqrt();
    println!("peak {peak:.4}  rms {rms:.4}  non-finite {nonfinite}");
    println!("difference from dry signal: {colour:.2}x dry rms");
    if nonfinite > 0 || peak <= 0.0 {
        println!("MODEL PRODUCED NO AUDIO");
        std::process::exit(1);
    }
    println!("OK");
}
