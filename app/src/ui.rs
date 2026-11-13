//! The pedal editor UI (egui): channel strips.
//!
//! A channel is a vertical strip of blocks. Drag a block up or down to
//! reorder it, click to select it (params in the inspector), use the `+` to
//! add a block, `×` to remove it. The **source** row at the top of each strip
//! routes audio in: tick `In` and/or another channel. Every change rebuilds
//! the executor on the UI thread and hands it to the audio thread; parameter
//! changes go straight to the running executor by index, so turning a knob
//! never recompiles anything.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use egui::{pos2, vec2, Align2, Color32, FontId, Rect, Sense, Stroke};

use dsp_graph::{BlockSpec, Category, ParamSpec, Unit};

use crate::audio::{read_f32, Handle, Meters};
use crate::channels::{master_id, Rig, IN, MAX_CHANNELS};
use crate::store::Store;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Riff,
    Live,
    Silence,
}

impl Source {
    pub fn label(self) -> &'static str {
        match self {
            Source::Riff => "Plucked riff",
            Source::Live => "Live input",
            Source::Silence => "Silence",
        }
    }
    pub fn as_u8(self) -> u8 {
        self as u8
    }
}

pub struct UiState {
    pub selected: Option<(String, String)>,
    pub doc_name: String,
    pub dirty: bool,
    pub status: String,
    pub source: Source,
    /// Channel whose `+ Block` palette is open.
    pub add_for: Option<String>,
    /// A block drag in progress: (channel, block id, drag-start y, accumulated offset).
    pub drag: Option<(String, String, f32, f32)>,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            selected: None,
            doc_name: String::new(),
            dirty: false,
            status: String::new(),
            source: Source::Riff,
            add_for: None,
            drag: None,
        }
    }
}

pub const BLOCK_W: f32 = 148.0;
pub const BLOCK_H: f32 = 56.0;
pub const STRIP_X: f32 = 16.0;
pub const STRIP_Y: f32 = 150.0;

fn category_color(c: Category) -> Color32 {
    match c {
        Category::Io => Color32::from_rgb(90, 90, 90),
        Category::Split => Color32::from_rgb(120, 120, 160),
        Category::Dynamics => Color32::from_rgb(80, 170, 120),
        Category::Tone => Color32::from_rgb(200, 170, 60),
        Category::Distortion => Color32::from_rgb(220, 90, 70),
        Category::Modulation => Color32::from_rgb(150, 110, 220),
        Category::Pitch => Color32::from_rgb(230, 120, 180),
        Category::Time => Color32::from_rgb(90, 160, 220),
        Category::Modelling => Color32::from_rgb(200, 120, 90),
        Category::Utility => Color32::from_rgb(130, 130, 130),
        Category::Control => Color32::from_rgb(170, 170, 120),
    }
}

/// One block card. Returns the accumulated drag offset in blocks, if any.
pub fn draw_block(ui: &mut egui::Ui, channel: &str, index: usize, spec: &BlockSpec, block: &dsp_graph::NodeDoc, selected: bool, drag_start: Option<f32>) -> Option<f32> {
    let min = pos2(STRIP_X, STRIP_Y + index as f32 * (BLOCK_H + 8.0));
    let rect = Rect::from_min_size(min, vec2(BLOCK_W, BLOCK_H));
    let painter = ui.painter();
    let fill = if block.bypass { Color32::from_rgb(48, 48, 52) } else { Color32::from_rgb(45, 48, 55) };
    let border = if selected { Color32::from_rgb(240, 200, 90) } else { Color32::from_rgb(90, 95, 105) };
    painter.rect(rect, 6.0, fill, Stroke::new(if selected { 2.0 } else { 1.0 }, border), egui::StrokeKind::Middle);
    painter.rect(Rect::from_min_size(min, vec2(BLOCK_W, 16.0)), 6.0, category_color(spec.category), Stroke::NONE, egui::StrokeKind::Middle);
    painter.text(
        pos2(min.x + BLOCK_W / 2.0, min.y + 8.0),
        Align2::CENTER_CENTER,
        &spec.name,
        FontId::proportional(11.0),
        if block.bypass { Color32::from_rgb(150, 150, 150) } else { Color32::WHITE },
    );
    let summary: Vec<String> = spec
        .params
        .iter()
        .map(|p| {
            let v = block.params.get(p.id).copied().unwrap_or(p.default);
            format!("{} {}", p.name, short_value(p, v))
        })
        .collect();
    painter.text(
        pos2(min.x + BLOCK_W / 2.0, min.y + BLOCK_H - 10.0),
        Align2::CENTER_CENTER,
        summary.join("  "),
        FontId::proportional(9.0),
        Color32::from_rgb(180, 185, 195),
    );
    let resp = ui.interact(rect, ui.scope_id().with(("block", channel, index)), Sense::click_and_drag());
    if let Some(start) = drag_start {
        if resp.dragged() {
            let total = resp.interact_pointer_pos().map(|p| p.y - start).unwrap_or(0.0);
            return Some((total / (BLOCK_H + 8.0)).round());
        }
    }
    None
}

fn short_value(p: &ParamSpec, v: f32) -> String {
    let unit = match p.unit {
        Unit::Decibels => "dB",
        Unit::Hertz => "Hz",
        Unit::Milliseconds => "ms",
        Unit::Percent => "%",
        Unit::Semitones => "st",
        Unit::Cents => "c",
        Unit::Ratio => "x",
        Unit::Linear | Unit::Choice(_) => "",
    };
    if let Some(c) = p.choices() {
        return c[v.round().clamp(0.0, (c.len() - 1) as f32) as usize].into();
    }
    if unit.is_empty() {
        format!("{v:.2}")
    } else {
        format!("{v:.1}{unit}")
    }
}

/// Slider (or combo box) for one param. Returns true when the value changed.
pub fn param_control(ui: &mut egui::Ui, spec: &ParamSpec, value: &mut f32) -> bool {
    if let Some(choices) = spec.choices() {
        let idx = value.round().clamp(0.0, (choices.len() - 1) as f32) as usize;
        let mut changed = false;
        egui::ComboBox::from_id_salt(("param", spec.id))
            .selected_text(choices[idx])
            .show_ui(ui, |ui| {
                for (i, c) in choices.iter().enumerate() {
                    if ui.selectable_label(idx == i, *c).clicked() {
                        *value = i as f32;
                        changed = true;
                    }
                }
            });
        ui.label(format!("{}: {}", spec.name, choices[idx]));
        return changed;
    }
    let mut v = *value;
    let changed = ui.add(egui::Slider::new(&mut v, spec.min..=spec.max).text(spec.name)).changed();
    ui.label(format!("{}: {}", spec.name, short_value(spec, v)));
    if changed {
        *value = v;
    }
    changed
}

/// The inspector for the selected block.
pub fn inspector(ui: &mut egui::Ui, rig: &mut Rig, sel: &(String, String), spec: &BlockSpec, handle: &Handle, state: &mut UiState) {
    ui.heading(spec.name);
    ui.label(format!("{} · {}", spec.id, spec.category));
    ui.separator();
    let Some(ch) = rig.channel_mut(&sel.0) else { return };
    let Some(block) = ch.blocks.iter_mut().find(|b| b.id == sel.1) else { return };
    let mut changed: Vec<(usize, f32)> = Vec::new();
    for (i, p) in spec.params.iter().enumerate() {
        let mut v = block.params.get(p.id).copied().unwrap_or(p.default);
        if param_control(ui, p, &mut v) {
            block.params.insert(p.id.to_string(), v);
            changed.push((i, v));
        }
    }
    let mut bypass = block.bypass;
    let bypass_changed = ui.checkbox(&mut bypass, "Bypass").changed();
    if bypass_changed {
        block.bypass = bypass;
    }
    if !changed.is_empty() || bypass_changed {
        if let Some(idx) = handle.node_index(&sel.1) {
            for (pi, v) in changed {
                handle.push(crate::audio::Cmd::SetParam { node: idx, param: pi, value: v }).ok();
            }
            if bypass_changed {
                handle.push(crate::audio::Cmd::SetBypass { node: idx, on: bypass }).ok();
            }
        }
    }
    // Sample-rate mismatch warning.
    let mismatch = handle
        .rate_mismatches
        .lock()
        .unwrap()
        .iter()
        .find(|(id, _, _)| id == &sel.1)
        .map(|(_, e, h)| (*e, *h));
    if let Some((expected, host)) = mismatch {
        ui.separator();
        ui.colored_label(
            Color32::from_rgb(240, 160, 60),
            format!("⚠ {expected:.0} Hz model on a {host:.0} Hz host — resampling"),
        );
    }
    // NAM blocks: load a `.nam` file.
    if spec.id == "mod.nam" {
        ui.separator();
        ui.label(egui::RichText::new("Model").strong());
        let current = rig.channel(&sel.0).and_then(|c| c.blocks.iter().find(|b| b.id == sel.1)).and_then(|b| b.model.clone());
        match current {
            Some(path) => ui.label(format!("{path}")),
            None => ui.label(egui::RichText::new("no model loaded").weak()),
        };
        if ui.button("Load .nam…").clicked() {
            if let Some(path) = rfd::FileDialog::new().add_filter("NAM model", &["nam"]).pick_file() {
                let path = path.to_string_lossy().into_owned();
                if let Some(ch) = rig.channel_mut(&sel.0) {
                    if let Some(b) = ch.blocks.iter_mut().find(|b| b.id == sel.1) {
                        b.model = Some(path);
                    }
                }
                if let Err(e) = rebuild(handle, rig) {
                    state.status = e;
                }
            }
        }
    }
    ui.separator();
    ui.label(egui::RichText::new("Drag a block up or down to reorder the strip.").weak());
}

/// The block palette, grouped by category. Returns the picked block's
/// (id, display name).
pub fn palette(ui: &mut egui::Ui, registry: &dsp_graph::Registry) -> Option<(String, String)> {
    let mut picked = None;
    let mut cats: Vec<(Category, Vec<&BlockSpec>)> = Vec::new();
    for spec in registry.specs() {
        if spec.category == Category::Io {
            continue;
        }
        match cats.iter_mut().find(|(c, _)| *c == spec.category) {
            Some((_, v)) => v.push(spec),
            None => cats.push((spec.category, vec![spec])),
        }
    }
    for (cat, specs) in cats {
        ui.label(egui::RichText::new(cat.to_string()).strong());
        for spec in specs {
            if ui.button(spec.name.clone()).on_hover_text(spec.id.clone()).clicked() {
                picked = Some((spec.id.to_string(), spec.name.to_string()));
            }
        }
        ui.separator();
    }
    picked
}

/// Top bar: preset, source, meters, add channel.
pub fn top_bar(ui: &mut egui::Ui, handle: &Handle, store: &mut Store, rig: &mut Rig, state: &mut UiState, meters: &Arc<Meters>) {
    ui.horizontal(|ui| {
        ui.heading("Pedal");
        ui.separator();
        ui.label("Preset");
        ui.text_edit_singleline(&mut state.doc_name);
        if ui.button("Load").clicked() {
            if let Some((_, doc)) = store.presets.iter().find(|(n, _)| n == &state.doc_name) {
                *rig = Rig::from_doc(doc);
                state.dirty = false;
                match rebuild(handle, rig) {
                    Ok(_) => state.status = format!("loaded {}", state.doc_name),
                    Err(e) => state.status = e,
                }
            } else {
                state.status = format!("no preset named {}", state.doc_name);
            }
        }
        if ui.button("Save").clicked() {
            rig.name = state.doc_name.clone();
            match store.save(&state.doc_name, &rig.to_doc()) {
                Ok(p) => {
                    state.status = format!("saved {}", p.display());
                    state.dirty = false;
                }
                Err(e) => state.status = e,
            }
        }
        if ui.button("Reload").clicked() {
            *store = Store::discover();
            state.status = "reloaded presets".into();
        }
        ui.separator();
        ui.label("Source");
        for s in [Source::Riff, Source::Live, Source::Silence] {
            if ui.radio_value(&mut state.source, s, s.label()).changed() {
                handle.set_source(s.as_u8());
            }
        }
        ui.separator();
        let in_peak = read_f32(&meters.in_peak);
        let out_peak = read_f32(&meters.out_peak);
        ui.add(egui::ProgressBar::new(in_peak.clamp(0.0, 1.0)).text(format!("in {in_peak:.2}")));
        ui.add(egui::ProgressBar::new(out_peak.clamp(0.0, 1.0)).text(format!("out {out_peak:.2}")));
        let cpu = read_f32(&meters.callback_s) / (crate::audio::MAX_BLOCK as f32 / handle.sample_rate);
        ui.label(format!("cpu {:.1}%", cpu * 100.0));
        ui.label(format!("xruns {}", meters.xruns.load(Ordering::Relaxed)));
        ui.separator();
        if rig.channels.len() < MAX_CHANNELS && ui.button("+ Channel").clicked() {
            if let Some(id) = rig.add_channel() {
                state.status = format!("added {id}");
                state.dirty = true;
                let _ = rebuild(handle, rig);
            }
        }
        if !state.status.is_empty() {
            ui.label(egui::RichText::new(state.status.clone()).weak());
        }
    });
}

/// Rebuild the executor from the rig. UI thread only.
pub fn rebuild(handle: &Handle, rig: &Rig) -> Result<(), String> {
    let doc = rig.to_doc();
    let mut g = doc
        .build_in(&handle.registry, &handle.preset_dir)
        .map_err(|e| e.to_string())?;
    let block = handle.block();
    let ex = dsp_graph::Executor::compile_owned(&mut g, handle.sample_rate, block).map_err(|e| e.to_string())?;
    let ids = ex.node_ids().to_vec();
    let bypass: Vec<(String, bool)> = rig
        .channels
        .iter()
        .flat_map(|c| c.blocks.iter().filter(|b| b.bypass).map(move |b| (b.id.clone(), true)))
        .collect();
    let mismatches: Vec<(String, f64, f32)> = ex
        .rate_mismatches()
        .iter()
        .map(|(id, e, h)| (id.clone(), *e, *h))
        .collect();
    handle.push(crate::audio::Cmd::Swap(ex, bypass)).map_err(|e| e.to_string())?;
    *handle.node_ids.lock().unwrap() = ids;
    *handle.rate_mismatches.lock().unwrap() = mismatches;
    Ok(())
}

/// One channel strip: source row, blocks, add/remove.
pub fn channel_strip(
    ui: &mut egui::Ui,
    handle: &Handle,
    rig: &mut Rig,
    state: &mut UiState,
    channel: &str,
    registry: &dsp_graph::Registry,
) {
    let Some(ch) = rig.channel(channel) else { return };
    let name = ch.name.clone();
    let sources = ch.sources.clone();
    let blocks: Vec<dsp_graph::NodeDoc> = ch.blocks.clone();

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(&name).strong());
        ui.separator();
        if ui.button("✕").on_hover_text("Remove channel").clicked() {
            rig.remove_channel(channel);
            state.dirty = true;
            let _ = rebuild(handle, rig);
            return;
        }
    });
    // Channel output volume. The master node lives in the compiled graph, so
    // this sends a param change by node id.
    if let Some(ch) = rig.channel_mut(channel) {
        let mut vol = ch.master_volume;
        if ui.add(egui::Slider::new(&mut vol, -60.0..=12.0).text("Volume")).changed() {
            ch.master_volume = vol;
            if let Some(idx) = handle.node_index(&master_id(channel)) {
                handle.push(crate::audio::Cmd::SetParam { node: idx, param: 0, value: vol }).ok();
            }
        }
    }
    ui.label("Source");
    ui.horizontal_wrapped(|ui| {
        let mut srcs = sources.clone();
        let mut changed = false;
        let on = srcs.iter().any(|s| s == IN);
        let mut on2 = on;
        if ui.checkbox(&mut on2, "In").changed() {
            changed = true;
            if on2 {
                srcs.push(IN.into());
            } else {
                srcs.retain(|s| s != IN);
            }
        }
        for other in rig.channels.iter().map(|c| c.id.clone()) {
            if other == channel {
                continue;
            }
            let on = srcs.iter().any(|s| s == &other);
            let mut on2 = on;
            if ui.checkbox(&mut on2, other.trim_start_matches("ch")).changed() {
                changed = true;
                if on2 {
                    srcs.push(other.clone());
                } else {
                    srcs.retain(|s| s != &other);
                }
            }
        }
        if changed {
            if let Some(c) = rig.channel_mut(channel) {
                c.sources = if srcs.is_empty() { vec![IN.into()] } else { srcs };
            }
            state.dirty = true;
            let _ = rebuild(handle, rig);
        }
    });
    ui.separator();

    let drag_channel = state.drag.as_ref().map(|(c, _, _, _)| c.clone());
    let drag_start = state.drag.as_ref().and_then(|(_, _, s, _)| {
        ui.input(|i| i.pointer.interact_pos().map(|p| p.y - *s))
    });
    for (i, block) in blocks.iter().enumerate() {
        let spec = registry.get(&block.block).cloned().unwrap_or_else(|| empty_spec(&block.block));
        let selected = state.selected.as_ref().map(|(c, b)| c == channel && b == &block.id).unwrap_or(false);
        let this_drag = if drag_channel.as_deref() == Some(channel) { drag_start } else { None };
        // Begin a drag when this block is dragged and none is in progress.
        if this_drag.is_none() && state.drag.is_none() {
            let min = pos2(STRIP_X, STRIP_Y + i as f32 * (BLOCK_H + 8.0));
            let rect = Rect::from_min_size(min, vec2(BLOCK_W, BLOCK_H));
            let r = ui.interact(rect, ui.scope_id().with(("blockdrag", channel, i)), Sense::click_and_drag());
            if r.drag_started() {
                let y = ui.input(|i| i.pointer.interact_pos().map(|p| p.y)).unwrap_or(0.0);
                state.drag = Some((channel.to_string(), block.id.clone(), y, 0.0));
            }
        }
        if let Some(steps) = draw_block(ui, channel, i, &spec, block, selected, this_drag) {
            if steps != 0.0 {
                let to = (i as f32 + steps).max(0.0) as usize;
                if to != i {
                    rig.move_block(channel, &block.id, to);
                    state.dirty = true;
                    let _ = rebuild(handle, rig);
                    // Reset the drag origin so the block doesn't keep moving.
                    state.drag = None;
                }
            }
        } else {
            let min = pos2(STRIP_X, STRIP_Y + i as f32 * (BLOCK_H + 8.0));
            let rect = Rect::from_min_size(min, vec2(BLOCK_W, BLOCK_H));
            if ui.interact(rect, ui.scope_id().with(("blockclick", channel, i)), Sense::click()).clicked() {
                state.selected = Some((channel.to_string(), block.id.clone()));
            }
        }
    }

    if state.add_for.as_deref() == Some(channel) {
        ui.label(egui::RichText::new("Add block").strong());
        if let Some((picked, name)) = palette(ui, registry) {
            let id = rig.add_block_as(channel, &picked, &name, [0.0, 0.0]);
            state.selected = Some((channel.to_string(), id));
            state.add_for = None;
            state.dirty = true;
            let _ = rebuild(handle, rig);
        }
        if ui.button("Cancel").clicked() {
            state.add_for = None;
        }
    } else if ui.button("+ Block").clicked() {
        state.add_for = Some(channel.to_string());
    }

    if let Some((c, b)) = &state.selected {
        if c == channel && ui.button(format!("Remove {b}")).clicked() {
            rig.remove_block(channel, b);
            state.selected = None;
            state.dirty = true;
            let _ = rebuild(handle, rig);
        }
    }
    ui.add_space(12.0);
}

fn empty_spec(_id: &str) -> BlockSpec {
    BlockSpec {
        id: "unknown",
        name: "Unknown block",
        category: Category::Utility,
        inputs: vec![],
        outputs: vec![],
        params: vec![],
        controls_in: vec![],
        controls_out: vec![],
    }
}
