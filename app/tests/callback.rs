//! The output callback must handle buffers larger than one block.

use std::sync::Arc;
use std::sync::atomic::AtomicU8;

use pedal_app::audio::{AudioThread, ChannelMode, Cmd, Meters, Setup};
use pedal_app::channels::Rig;
use pedal_app::store::Riff;
use dsp_graph::blocks::builtin_registry;

const SR: f32 = 48_000.0;
const BLOCK: usize = 64;

fn engine_with_rig() -> AudioThread {
    let mut g = Rig::single(
        "test",
        vec![
            dsp_graph::NodeDoc {
                id: "amp".into(),
                block: "amp.basic".into(),
                params: [("gain".to_string(), 4.0)].into_iter().collect(),
                bypass: false,
                pos: None,
                model: None,
            },
            dsp_graph::NodeDoc {
                id: "master".into(),
                block: "util.master".into(),
                params: [("volume".to_string(), 0.0)].into_iter().collect(),
                bypass: false,
                pos: None,
                model: None,
            },
        ],
    )
    .to_doc()
    .build(&builtin_registry())
    .unwrap();
    let ex = dsp_graph::Executor::compile_owned(&mut g, SR, BLOCK).unwrap();
    let (_cmd_tx, cmd_rx) = rtrb::RingBuffer::<Cmd>::new(8);
    let (trash_tx, _trash_rx) = rtrb::RingBuffer::<dsp_graph::Executor>::new(4);
    let mut e = AudioThread::new(
        Setup {
            sample_rate: SR,
            out_channels: 2,
            block: BLOCK,
            channel_mode: ChannelMode::First(1),
            latency_frames: 1024,
        },
        Arc::new(Meters::default()),
        cmd_rx,
        trash_tx,
        Arc::new(std::sync::atomic::AtomicBool::new(true)),
        Arc::new(AtomicU8::new(0)),
    );
    e.set_executor(ex);
    e.set_block(BLOCK);
    e
}

/// Fill a buffer through the real callback path.
fn run_callback(e: &mut AudioThread, frames: usize) -> Vec<f32> {
    let mut data = vec![0.0f32; frames * 2];
    let ring = pedal_app::audio::Ring::new(8192);
    let meters = Arc::new(Meters::default());
    pedal_app::audio::run_callback(e, &ring, &meters, &mut data, 2);
    data
}

/// A 512-frame callback (8 blocks) must be fully filled, not just the first 64.
#[test]
fn large_callback_is_fully_filled() {
    let mut e = engine_with_rig();
    let frames = 512;
    let data = run_callback(&mut e, frames);
    let peak = data.iter().map(|v| v.abs()).fold(0.0f32, f32::max);
    assert!(peak > 0.1, "large callback produced silence (peak {peak})");
    // The last block must carry signal too — that's what was broken.
    let tail = &data[(frames - BLOCK) * 2..];
    let tail_peak = tail.iter().map(|v| v.abs()).fold(0.0f32, f32::max);
    assert!(tail_peak > 0.1, "tail block silent (peak {tail_peak})");
}

/// A callback that isn't a multiple of the block size leaves a short tail.
#[test]
fn odd_sized_callback_works() {
    let mut e = engine_with_rig();
    let data = run_callback(&mut e, 100);
    assert!(data.iter().all(|v| v.is_finite()));
}

/// The riff source advances across chunked calls.
#[test]
fn riff_advances_across_chunks() {
    let mut r = Riff::new(SR);
    let a = r.tick(SR);
    let b = r.tick(SR);
    assert!(a != b);
}
