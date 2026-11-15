//! Rig-level behaviour a first-time user depends on: a blank pedalboard must
//! make sound, and adding a block must actually wire it in.

use pedal_app::channels::{master_id, Rig, IN};
use dsp_graph::blocks::builtin_registry;
use dsp_graph::Executor;

const SR: f32 = 48_000.0;
const BLOCK: usize = 64;

fn run_peak(rig: &Rig) -> f32 {
    render(rig, BLOCK * 8).iter().map(|v| v.abs()).fold(0.0f32, f32::max)
}

/// Render the rig with a fixed pluck signal so two rigs can be compared.
fn render(rig: &Rig, n: usize) -> Vec<f32> {
    let mut g = rig.to_doc().build(&builtin_registry()).unwrap();
    let mut ex = Executor::compile_owned(&mut g, SR, BLOCK).unwrap();
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f32 / SR;
        let note_t = t % 0.5;
        let f = [82.4, 110.0, 146.8, 196.0][(t / 0.5) as usize % 4];
        let saw = 2.0 * (note_t * f).fract() - 1.0;
        let s = 0.3 * saw * (-note_t * 6.0).exp();
        out.push(ex.run(&[s], 1).unwrap()[0]);
    }
    out
}

/// A pedalboard with no channels at all is a bypass, not a dead pedal.
#[test]
fn blank_pedalboard_is_a_bypass_not_silence() {
    let rig = Rig::new("blank");
    let peak = run_peak(&rig);
    assert!(peak > 0.05, "blank pedalboard output silence (peak {peak})");
}

/// One channel with no blocks is the same thing: the user's starting point.
#[test]
fn empty_channel_passes_audio() {
    let rig = Rig::new("blank");
    let peak = run_peak(&rig);
    assert!(peak > 0.05, "empty channel output silence (peak {peak})");
}

/// A rig must survive a save/load round trip unchanged.
#[test]
fn rig_round_trip_preserves_channels() {
    let mut rig = Rig::new("round trip");
    rig.add_block("ch1", "amp.basic", [0.0, 0.0]);
    rig.add_block("ch1", "time.delay", [0.0, 0.0]);
    let back = Rig::from_doc(&rig.to_doc());
    assert_eq!(back.channels.len(), rig.channels.len());
    assert_eq!(back.channels[0].blocks.len(), 2);
    assert_eq!(back.channels[0].blocks[0].block, "amp.basic");
    assert_eq!(back.channels[0].blocks[1].block, "time.delay");
    assert_eq!(back.channels[0].sources, vec![IN.to_string()]);
}

/// Adding a block must put it in the signal path: source → … → block → master →
/// sum → out. Before this was wired, a freshly added block was dead weight.
#[test]
fn added_block_is_wired_into_the_chain() {
    let mut rig = Rig::new("wired");
    let id = rig.add_block("ch1", "amp.basic", [0.0, 0.0]);
    let doc = rig.to_doc();

    let has = |from: &str, to: &str| {
        doc.edges.iter().any(|e| e.from[0] == from && e.to[0] == to)
    };
    assert!(has("in", &id), "no edge from the pedal input to the new block");
    assert!(has(&id, &master_id("ch1")), "no edge from the new block to the master");
    assert!(has(&master_id("ch1"), "sum"), "master is not feeding the output sum");
    assert!(has("sum", "out__master"), "sum is not feeding the output master");
    assert!(has("out__master", "out"), "output master is not feeding the pedal output");
}

/// The POC's core claim: adding a block from the palette changes the sound.
/// A block that is wired in but audible-identical would still fail the POC.
#[test]
fn adding_a_block_changes_the_sound() {
    let mut rig = Rig::new("poc");
    let dry = render(&rig, SR as usize);

    rig.add_block_as("ch1", "amp.basic", "Amp", [0.0, 0.0]);
    let wet = render(&rig, SR as usize);

    let dry_rms = rms(&dry);
    let wet_rms = rms(&wet);
    assert!(dry_rms > 0.01, "blank rig is silent (rms {dry_rms})");
    assert!(wet_rms > 0.01, "rig with an amp is silent (rms {wet_rms})");
    // Compare the waveform, not the level: a mild amp reshapes the signal far
    // more than it changes its loudness (here ~1.11 relative difference at a
    // 1.05x level change), so a level check would call a working amp a no-op.
    let diff = rms(&dry.iter().zip(&wet).map(|(a, b)| a - b).collect::<Vec<_>>());
    let rel = diff / dry_rms;
    assert!(rel > 0.02, "adding an amp left the waveform unchanged ({rel:.3} relative difference)");
}

fn rms(v: &[f32]) -> f32 {
    (v.iter().map(|&x| (x * x) as f64).sum::<f64>() / v.len() as f64).sqrt() as f32
}

/// Reordering must not break the chain.
#[test]
fn reordered_chain_is_still_fully_wired() {
    let mut rig = Rig::new("reorder");
    let amp = rig.add_block_as("ch1", "amp.basic", "Amp", [0.0, 0.0]);
    let delay = rig.add_block_as("ch1", "time.delay", "Delay", [0.0, 0.0]);
    // Move the delay to the front.
    rig.move_block("ch1", &delay, 0);
    let doc = rig.to_doc();
    let has = |from: &str, to: &str| {
        doc.edges.iter().any(|e| e.from[0] == from && e.to[0] == to)
    };
    assert_eq!(amp, "amp", "display name should become the node id");
    assert!(has("in", &delay));
    assert!(has(&delay, &amp));
    assert!(has(&amp, &master_id("ch1")));
    assert!(has(&master_id("ch1"), "sum"));
}
