//! Every channel slot is rendered, empty or not.
//!
//! The model keeps a fixed [`CHANNEL_COUNT`] set of slots (see
//! `engine/tests/channel_slots.rs`); this pins the other half of the contract —
//! the egui panel paints all of them. A `.filter(|c| !c.blocks.is_empty())` in
//! the render loop would leave the user staring at three rows after deleting a
//! block, and nothing else in the suite would notice.

use std::sync::atomic::AtomicU8;
use std::sync::Arc;

use pedal_app::audio::{new_engine, ChannelMode, Handle, Setup};
use pedal_app::channels::{Rig, CHANNEL_COUNT};
use pedal_app::ui::{channel_strip, UiState};

/// A handle with no audio device behind it. Nothing here changes a control, so
/// the command channel is never written to.
fn test_handle() -> Handle {
    let setup = Setup {
        sample_rate: 48_000.0,
        out_channels: 2,
        block: 64,
        channel_mode: ChannelMode::First(2),
        latency_frames: 0,
    };
    let (handle, _cmd_rx, _trash_tx, _running) = new_engine(setup, Arc::new(AtomicU8::new(0)));
    handle
}

/// Paint the central panel exactly as `App::ui` does and return every string
/// that reached a text shape.
fn painted_text(handle: &Handle, rig: &mut Rig) -> Vec<String> {
    let ctx = egui::Context::default();
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 760.0))),
        ..Default::default()
    };
    let mut out = ctx.run_ui(input, |ui| {
        egui::CentralPanel::default().show(ui, |ui| {
            let mut state = UiState::default();
            // Collect first: `channel_strip` needs `&mut Rig` while it paints.
            let ids: Vec<String> = rig.channels.iter().map(|c| c.id.clone()).collect();
            for id in ids {
                channel_strip(ui, handle, rig, &mut state, &id, &handle.registry);
            }
        });
    });
    // There is no GPU here to own the font atlas.
    out.textures_delta.clear();
    out.shapes
        .iter()
        .filter_map(|cs| match &cs.shape {
            egui::Shape::Text(t) => Some(t.galley.job.text.clone()),
            _ => None,
        })
        .collect()
}

/// A fresh rig has four slots and the panel paints all four, including the
/// three that are empty.
#[test]
fn every_slot_is_rendered_even_when_empty() {
    let handle = test_handle();
    let mut rig = Rig::new("slots");
    assert_eq!(rig.channels.len(), CHANNEL_COUNT);
    assert!(rig.channels.iter().all(|c| c.blocks.is_empty()));

    let text = painted_text(&handle, &mut rig);
    for c in &rig.channels {
        assert!(
            text.iter().any(|t| t.contains(&c.name)),
            "channel {:?} was not painted; painted text: {text:?}",
            c.name
        );
    }
}

/// Loading a real preset must not shrink the panel either: `Rig::from_doc` pads
/// the slots back up, and the render loop shows them.
#[test]
fn a_loaded_preset_still_renders_every_slot() {
    let handle = test_handle();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../presets/01-clean.json");
    let json = std::fs::read_to_string(path).unwrap();
    let doc = dsp_graph::Doc::from_json(&json).unwrap();
    let mut rig = Rig::from_doc(&doc);
    assert_eq!(rig.channels.len(), CHANNEL_COUNT);

    let text = painted_text(&handle, &mut rig);
    let painted = rig.channels.iter().filter(|c| text.iter().any(|t| t.contains(&c.name))).count();
    assert_eq!(painted, CHANNEL_COUNT, "painted text: {text:?}");
}
