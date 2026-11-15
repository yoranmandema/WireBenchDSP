//! Layout contract for the editor: channels stack **vertically** and the
//! blocks inside a channel run **left to right**, so a row reads as a signal
//! path.
//!
//! These drive a real egui frame and read the painted shapes back, so they
//! check what the user actually sees — not our arithmetic. A layout that
//! regresses (blocks stacked in a column, channels side by side) fails here
//! rather than in a screenshot nobody reads.

use std::sync::atomic::AtomicU8;
use std::sync::Arc;

use pedal_app::audio::{new_engine, ChannelMode, Handle, Setup};
use pedal_app::channels::Rig;
use pedal_app::ui::{channel_strip, UiState, BLOCK_H, BLOCK_W};

/// A handle with no audio device behind it. Nothing in these tests changes a
/// control, so the command channel is never written to.
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

/// Paint the given channels through a real egui frame and return the rects of
/// the shapes that are exactly one block card.
fn render(handle: &Handle, rig: &mut Rig, channels: &[&str]) -> Vec<egui::Rect> {
    let ctx = egui::Context::default();
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 760.0))),
        ..Default::default()
    };
    let mut out = ctx.run_ui(input, |ui| {
        egui::CentralPanel::default().show(ui, |ui| {
            let mut state = UiState::default();
            for id in channels {
                channel_strip(ui, handle, rig, &mut state, id, &handle.registry);
            }
        });
    });
    // We only care about the shapes; there is no GPU here to own the font atlas.
    out.textures_delta.clear();
    out.shapes
        .iter()
        .filter_map(|cs| match &cs.shape {
            egui::Shape::Rect(r) => Some(r.rect),
            _ => None,
        })
        .filter(|r| (r.width() - BLOCK_W).abs() < 0.5 && (r.height() - BLOCK_H).abs() < 0.5)
        .collect()
}

#[test]
fn blocks_in_a_channel_run_left_to_right() {
    let handle = test_handle();
    let mut rig = Rig::new("lay");
    rig.add_block_as("ch1", "amp.basic", "Amp", [0.0, 0.0]);
    rig.add_block_as("ch1", "cab.sim", "Cab", [0.0, 0.0]);
    rig.add_block_as("ch1", "delay.basic", "Delay", [0.0, 0.0]);

    let rects = render(&handle, &mut rig, &["ch1"]);
    assert_eq!(rects.len(), 3, "expected three block cards, painted {rects:#?}");

    // One row: all three share a top edge.
    for r in &rects[1..] {
        assert!(
            (r.min.y - rects[0].min.y).abs() < 1.0,
            "blocks are not on one row: {rects:#?}"
        );
    }
    // Left to right: each starts past the previous one's end, with an even gap.
    // The gap is `BLOCK_GAP` plus egui's item spacing, so assert consistency
    // rather than an exact width.
    let gaps: Vec<f32> = rects.windows(2).map(|p| p[1].min.x - p[0].max.x).collect();
    for g in &gaps {
        assert!(*g > 0.0, "blocks overlap: {rects:#?}");
    }
    for g in &gaps[1..] {
        assert!(
            (g - gaps[0]).abs() < 0.5,
            "uneven gaps between blocks: {gaps:?}"
        );
    }
}

#[test]
fn channels_stack_vertically() {
    let handle = test_handle();
    let mut rig = Rig::new("lay");
    rig.add_block_as("ch1", "amp.basic", "Amp", [0.0, 0.0]);
    rig.add_block_as("ch2", "delay.basic", "Delay", [0.0, 0.0]);

    let rects = render(&handle, &mut rig, &["ch1", "ch2"]);
    assert_eq!(rects.len(), 2, "expected one block per channel, painted {rects:#?}");
    let (a, b) = (rects[0], rects[1]);
    assert!(
        b.min.y > a.max.y,
        "channels are not stacked vertically: ch1 at y={}, ch2 at y={}",
        a.min.y,
        b.min.y
    );
}
