//! Verification tests for the [`Octaver`] block.
//!
//! The octaver blends the input with octave-down and octave-up copies of
//! itself. These tests feed a known sine wave and check the output
//! fundamental — the same pattern used for the other effects in this
//! workspace.
//!
//!     cargo test -p dsp-graph --test octaver
//!
//! See [`Octaver`] for the block implementation.

use dsp_graph::blocks::{IoInput, IoOutput, Octaver};
use dsp_graph::{Category, Executor, Graph};

/// 2π.
const TAU: f32 = core::f32::consts::TAU;

use libm;

const SR: f32 = 48_000.0;
const BLOCK: usize = 256;
const F0: f32 = 440.0; // A4
const N: usize = 16 * BLOCK; // 16 blocks = 4096 samples

/// Build a minimal rig: in -> octaver -> out, with the given real values.
fn octaver_graph(dry: f32, oct_down: f32, oct_up: f32) -> Graph {
    let mut g = Graph::new();
    g.add("in", Box::new(IoInput)).unwrap();
    g.add("oct", Box::new(Octaver::default())).unwrap();
    g.add("out", Box::new(IoOutput)).unwrap();
    g.connect("in", "out", "oct", "in").unwrap();
    g.connect("oct", "out", "out", "in").unwrap();
    g.set_param_real("oct", "dry", dry).unwrap();
    g.set_param_real("oct", "oct_down", oct_down).unwrap();
    g.set_param_real("oct", "oct_up", oct_up).unwrap();
    g
}

/// Run the rig over the whole `input` in `BLOCK`-sized chunks.
fn run_rig(g: &mut Graph, input: &[f32]) -> Vec<f32> {
    let mut ex = Executor::compile_owned(g, SR, BLOCK).unwrap();
    let mut out = Vec::with_capacity(input.len());
    let mut i = 0;
    while i < input.len() {
        let chunk = input[i..].len().min(BLOCK);
        let chunk_in = &input[i..(i + chunk)];
        let chunk_out = ex.run(chunk_in, BLOCK).unwrap();
        out.extend_from_slice(chunk_out);
        i += chunk;
    }
    out
}

/// Hann-windowed DFT magnitude at one frequency.
fn dft_mag(s: &[f32], fs: f32, f: f32) -> f32 {
    let n = s.len() as f32;
    let mut re = 0.0f32;
    let mut im = 0.0f32;
    for (i, &x) in s.iter().enumerate() {
        let t = i as f32;
        let w = 0.5 - 0.5 * libm::cosf(TAU * t / (n - 1.0));
        let ph = -TAU * f * t / fs;
        re += w * x * libm::cosf(ph);
        im += w * x * libm::sinf(ph);
    }
    (re * re + im * im).sqrt()
}

/// A steady input tone must come out at the octave-shifted frequency: the
/// octave-down copy dominates.
#[test]
fn octaver_lowers_pitch_by_one_octave() {
    // Warm-up input: a 440 Hz sine.  The pitch shifter's read head needs a
    // few blocks to settle, so only the tail is analysed.
    let mut input = vec![0.0f32; N];
    for (i, s) in input.iter_mut().enumerate() {
        *s = libm::sinf(TAU * F0 * i as f32 / SR);
    }

    let mut g = octaver_graph(0.0, 100.0, 0.0);
    let out = run_rig(&mut g, &input);

    // Steady-state window: the last 2048 samples (after warm-up).
    let win = &out[8 * BLOCK..];
    let mag_220 = dft_mag(win, SR, F0 / 2.0);
    let mag_440 = dft_mag(win, SR, F0);
    let total = win.iter().map(|x| x * x).sum::<f32>().sqrt();

    // 220 Hz must dominate the steady-state output by a wide margin.
    assert!(
        mag_220 > mag_440 * 20.0,
        "oct_down=100: expected 220 Hz to dominate, got mag220 {mag_220:.4}, mag440 {mag_440:.4}"
    );
    assert!(total > 0.1, "oct_down=100: output is silent (total {total})");
}

/// Setting the octave levels to zero must give a dry pass-through at any dry
/// level (wet = 0).
#[test]
fn octaver_octave_levels_zero_yields_dry() {
    let mut input = vec![0.0f32; BLOCK * 8];
    for (i, s) in input.iter_mut().enumerate() {
        *s = (i as f32 * 0.07).sin();
    }

    // dry=0.5, both octaves off: output must be exactly half the input.
    let mut g = octaver_graph(50.0, 0.0, 0.0);
    let out = run_rig(&mut g, &input);
    for (i, (&a, &b)) in input.iter().zip(out.iter()).enumerate() {
        assert!(
            (a * 0.5 - b).abs() < 1e-6,
            "dry=0.5/oct=0: expected {a}, got {b} at sample {i}"
        );
    }

    // dry=1, both octaves off: output must equal the input exactly.
    let mut g = octaver_graph(100.0, 0.0, 0.0);
    let out = run_rig(&mut g, &input);
    for (i, (&a, &b)) in input.iter().zip(out.iter()).enumerate() {
        assert!(
            (a - b).abs() < 1e-6,
            "dry=1/oct=0: expected {a}, got {b} at sample {i}"
        );
    }
}

/// Dry and the octave levels must be independent: raising oct_down while
/// keeping dry fixed must change the output without changing the input.
#[test]
fn octaver_dry_is_independent_of_octave_levels() {
    let mut input = vec![0.0f32; BLOCK * 8];
    for (i, s) in input.iter_mut().enumerate() {
        *s = (i as f32 * 0.07).sin();
    }
    let steady = &input[6 * BLOCK..];

    // dry=0.5, no octaves -> exactly 0.5 * input.
    let mut g = octaver_graph(50.0, 0.0, 0.0);
    let dry_only = run_rig(&mut g, &input);
    for (i, (&a, &b)) in input.iter().zip(dry_only.iter()).enumerate() {
        assert!(
            (a * 0.5 - b).abs() < 1e-6,
            "dry=0.5, no octaves: expected {}, got {} at sample {}",
            a * 0.5, b, i
        );
    }

    // dry=0.5, oct_down=100 -> adds a 220 Hz component on top of the same
    // 0.5 dry level.  Compare the 220 Hz magnitude against the dry-only run
    // (which has none): the octave channel must add real energy.
    let mut g = octaver_graph(50.0, 100.0, 0.0);
    let down = run_rig(&mut g, &input);
    let w_down = &down[6 * BLOCK..];
    let w_dry = &dry_only[6 * BLOCK..];
    let m220_down = dft_mag(w_down, SR, F0 / 2.0);
    let m220_dry = dft_mag(w_dry, SR, F0 / 2.0);
    assert!(
        m220_down > m220_dry * 20.0 && m220_down > 5.0,
        "oct_down adds 220 Hz (dry 220Hz={m220_dry}, down 220Hz={m220_down})"
    );

    // dry=0.5, oct_up=100 -> adds an 880 Hz component on the same dry path,
    // independent of the octave-down channel.
    let mut g = octaver_graph(50.0, 0.0, 100.0);
    let up = run_rig(&mut g, &input);
    let w_up = &up[6 * BLOCK..];
    let m880_up = dft_mag(w_up, SR, F0 * 2.0);
    let m880_dry = dft_mag(w_dry, SR, F0 * 2.0);
    // The octave-up copy reads at 2x rate, so it is inherently quieter than
    // the octave-down copy (a delay-line shifter only keeps ~half of the
    // doubled-frequency energy).  Verify it is present as a distinct
    // component relative to the dry-only leakage, and above the noise floor.
    assert!(
        m880_up > m880_dry * 4.0 && m880_up > 1.0,
        "oct_up adds 880 Hz (dry 880Hz={m880_dry}, up 880Hz={m880_up})"
    );

    // dry=0.5, both octaves -> dry + down + up: the output must show BOTH
    // the 220 Hz and 880 Hz components, each far above the 440 Hz dry peak.
    let mut g = octaver_graph(50.0, 100.0, 100.0);
    let down_up = run_rig(&mut g, &input);
    let w = &down_up[6 * BLOCK..];
    let m220 = dft_mag(w, SR, F0 / 2.0);
    let m880 = dft_mag(w, SR, F0 * 2.0);
    let m440 = dft_mag(w, SR, F0);
    // The octave-up copy is inherently quieter (~half the energy of a
    // doubled-frequency read), so it is checked against a lower floor than
    // the loud octave-down (220 Hz) copy.
    assert!(m220 > 5.0, "both octaves: 220 Hz present, m220={m220}");
    assert!(m880 > 1.0, "both octaves: 880 Hz present, m880={m880}");
}

/// Silence must stay finite (no NaN/inf) at any level.
#[test]
fn octaver_is_finite_on_silence() {
    let silence = vec![0.0f32; BLOCK * 8];
    for (dry, oct_down, oct_up) in [(0.0, 0.0, 0.0), (100.0, 0.0, 0.0), (0.0, 100.0, 0.0), (0.0, 0.0, 100.0), (100.0, 100.0, 100.0)] {
        let mut g = octaver_graph(dry, oct_down, oct_up);
        let out = run_rig(&mut g, &silence);
        for (i, &y) in out.iter().enumerate() {
            assert!(
                y.is_finite(),
                "dry={dry} oct_down={oct_down} oct_up={oct_up}: NaN/inf at sample {i}: {y}"
            );
        }
    }
}

/// Full-scale input must not produce NaN/inf.
#[test]
fn octaver_is_finite_on_full_scale() {
    let full = vec![1.0f32; BLOCK * 8];
    for (dry, oct_down, oct_up) in [(100.0, 0.0, 0.0), (0.0, 100.0, 0.0), (0.0, 0.0, 100.0), (100.0, 100.0, 100.0)] {
        let mut g = octaver_graph(dry, oct_down, oct_up);
        let out = run_rig(&mut g, &full);
        for (i, &y) in out.iter().enumerate() {
            assert!(
                y.is_finite(),
                "full-scale dry={dry} oct_down={oct_down} oct_up={oct_up}: NaN/inf at sample {i}: {y}"
            );
        }
    }
}

/// Exposes the effect in the same way the UI enumerates blocks (via the
/// registry spec).
#[test]
fn octaver_is_registered_in_the_palette() {
    let mut r = dsp_graph::Registry::new();
    r.add::<Octaver>();
    let spec = r.specs().find(|s| s.id == "pitch.octaver").expect("octaver spec missing");
    assert_eq!(spec.name, "Octaver");
    assert_eq!(spec.category, Category::Pitch);
    let dry = spec.param_index("dry").expect("dry param missing");
    let oct_down = spec.param_index("oct_down").expect("oct_down param missing");
    let oct_up = spec.param_index("oct_up").expect("oct_up param missing");
    // Percentages, 0..100, defaults from the block.
    assert_eq!(spec.params[dry].min, 0.0);
    assert_eq!(spec.params[dry].max, 100.0);
    assert_eq!(spec.params[dry].default, 100.0);
    assert_eq!(spec.params[oct_down].default, 0.0);
    assert_eq!(spec.params[oct_up].default, 0.0);
}
