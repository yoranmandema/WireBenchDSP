//! The engine process: audio + IPC, no UI.
//!
//! ```text
//!   UI (webview)  ──WebSocket──▶  this process  ──Cmd──▶  audio thread
//!      ▲                                │
//!      └────────── state / meters ◀─────┘
//! ```
//!
//! The UI can be killed at any moment; audio keeps running.

use std::sync::atomic::AtomicU8;
use std::sync::Arc;

use pedal_engine::audio::{new_engine, Cmd, Setup};
use pedal_engine::channels::Rig;
use pedal_engine::host::{self, BLOCK, LATENCY_FRAMES};
use pedal_engine::server::{serve, Shared, PORT};
use pedal_engine::store::Store;

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let preset = args
        .iter()
        .position(|a| a == "--rig")
        .and_then(|i| args.get(i + 1))
        .cloned()
        .unwrap_or_else(|| "04-rainbow".into());

    let store = Store::discover();
    if let Some(e) = &store.error {
        eprintln!("presets: {e}");
    }

    // Pick the devices first: the executor and the audio thread must agree on
    // the sample rate.
    let devices = host::pick();

    let setup = Setup {
        sample_rate: devices.rate as f32,
        out_channels: devices.out_channels,
        block: BLOCK,
        channel_mode: pedal_engine::audio::ChannelMode::First(2),
        latency_frames: LATENCY_FRAMES,
    };
    let source = Arc::new(AtomicU8::new(2)); // silence by default; the riff is opt-in
    let (mut handle, cmd_rx, trash_tx, running) = new_engine(setup, source.clone());
    handle.preset_dir = store.dir.clone();
    let meters = handle.meters.clone();
    let registry = handle.registry.clone();

    // Compile and install the initial rig before the streams start.
    let doc = store
        .presets
        .iter()
        .find(|(n, _)| *n == preset)
        .map(|(_, d)| d.clone())
        .or_else(|| store.presets.first().map(|(_, d)| d.clone()))
        .unwrap_or_else(|| Rig::single("Untitled", vec![]).to_doc());
    let rig = Rig::from_doc(&doc);
    {
        let mut g = rig
            .to_doc()
            .build_in(&handle.registry, &handle.preset_dir)
            .expect("initial rig builds");
        let ex = dsp_graph::Executor::compile_owned(&mut g, setup.sample_rate, BLOCK)
            .expect("initial rig compiles");
        *handle.node_ids.lock().unwrap() = ex.node_ids().to_vec();
        handle.push(Cmd::Swap(ex, Vec::new())).expect("initial swap");
    }
    handle.set_source(2); // silence

    // The streams own the command queue from here on.
    host::start(&devices, cmd_rx, trash_tx, running, meters, source.clone());

    // --- IPC ---------------------------------------------------------------
    let sh = Arc::new(Shared::new(handle, registry, source, store));
    {
        let mut r = sh.rig.lock().unwrap();
        *r = rig;
        drop(r);
        let mut p = sh.preset.lock().unwrap();
        *p = preset.clone();
    }
    eprintln!("pedal-engine: preset {preset}, ipc on ws://127.0.0.1:{PORT}");
    if let Err(e) = serve(sh).await {
        eprintln!("pedal-engine: {e}");
        std::process::exit(1);
    }
}
