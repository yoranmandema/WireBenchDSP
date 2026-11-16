//! The pedal editor UI (egui): channel strips.
//!
//! Channels are stacked **vertically**, one per row, and the blocks inside a
//! channel run **horizontally**, left to right — so a row reads as a signal
//! path. Drag a block sideways to reorder it, click to select it (params in
//! the inspector), use the `+` to add a block, `×` to remove it. The channel
//! header routes audio in: tick `In` and/or another channel. Every change
//! rebuilds the executor on the UI thread and hands it to the audio thread;
//! parameter changes go straight to the running executor by index, so turning
//! a knob never recompiles anything.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use egui::{pos2, vec2, Align2, Color32, FontId, Rect, Sense, Stroke};

use dsp_graph::{BlockSpec, Category, ParamSpec, Unit};

use crate::audio::{read_f32, Handle, Meters};
use crate::channels::{master_id, Rig, IN};
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
    /// A block drag in progress: (channel, block id, drag-start x, accumulated offset).
    pub drag: Option<(String, String, f32, f32)>,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            selected: None,
            doc_name: String::new(),
            dirty: false,
            status: String::new(),
            source: Source::Silence,
            add_for: None,
            drag: None,
        }
    }
}

pub const BLOCK_W: f32 = 148.0;
pub const BLOCK_H: f32 = 56.0;
/// Gap between two blocks in a strip.
pub const BLOCK_GAP: f32 = 8.0;

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

/// Paint one block card into `rect`. Interaction belongs to the caller, which
/// allocates the rect — that way a strip can lay blocks out with the normal
/// egui layout (and scroll) instead of absolute coordinates.
fn paint_block(ui: &mut egui::Ui, rect: Rect, spec: &BlockSpec, block: &dsp_graph::NodeDoc, selected: bool) {
    let min = rect.min;
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
}

/// One source checkbox. Returns true when it toggled.
fn source_toggle(ui: &mut egui::Ui, id: &str, label: &str, srcs: &mut Vec<String>) -> bool {
    let on = srcs.iter().any(|s| s == id);
    let mut on2 = on;
    if ui.checkbox(&mut on2, label).changed() {
        if on2 {
            srcs.push(id.to_string());
        } else {
            srcs.retain(|s| s != id);
        }
        return true;
    }
    false
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
    ui.label(egui::RichText::new("Drag a block left or right to reorder the strip.").weak());
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

/// Top bar: preset, source, meters.
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
    // Routings are realised here, at compile time; `to_doc` only carries them.
    rig.apply_routings(&mut g, &handle.registry)?;
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

/// One channel: a header row (name, volume, source routing) above a
/// horizontally scrolling strip of blocks that read as a signal path.
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
    let other_ids: Vec<String> =
        rig.channels.iter().filter(|c| c.id != channel).map(|c| c.id.clone()).collect();

    // --- header ---------------------------------------------------------
    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new(&name).strong());
        ui.separator();
        // Channel output volume. The master node lives in the compiled graph,
        // so this sends a param change by node id.
        if let Some(ch) = rig.channel_mut(channel) {
            let mut vol = ch.master_volume;
            if ui
                .add_sized(
                    [110.0, 20.0],
                    egui::Slider::new(&mut vol, -60.0..=12.0).text("Volume"),
                )
                .changed()
            {
                ch.master_volume = vol;
                if let Some(idx) = handle.node_index(&master_id(channel)) {
                    handle.push(crate::audio::Cmd::SetParam { node: idx, param: 0, value: vol }).ok();
                }
            }
        }
        ui.separator();
        ui.label("Source");
        let mut srcs = sources.clone();
        let mut changed = false;
        if source_toggle(ui, IN, "In", &mut srcs) {
            changed = true;
        }
        for other in &other_ids {
            if source_toggle(ui, other, other.trim_start_matches("ch"), &mut srcs) {
                changed = true;
            }
        }
        if changed {
            if let Some(c) = rig.channel_mut(channel) {
                c.sources = if srcs.is_empty() { vec![IN.into()] } else { srcs };
            }
            state.dirty = true;
            let _ = rebuild(handle, rig);
        }
        ui.separator();
    });

    // --- the strip: blocks run left to right ----------------------------
    egui::ScrollArea::horizontal()
        .id_salt(("strip", channel))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                for (i, block) in blocks.iter().enumerate() {
                    let spec = registry.get(&block.block).cloned().unwrap_or_else(|| empty_spec(&block.block));
                    let selected = state
                        .selected
                        .as_ref()
                        .map(|(c, b)| c == channel && b == &block.id)
                        .unwrap_or(false);
                    // Re-read the drag state each iteration: a move mid-loop
                    // clears it, and the remaining blocks must not inherit it.
                    let this_drag = match state.drag.as_ref() {
                        Some((c, _, s, _)) if c == channel => {
                            ui.input(|i| i.pointer.interact_pos().map(|p| p.x - *s))
                        }
                        _ => None,
                    };
                    let (rect, resp) =
                        ui.allocate_exact_size(vec2(BLOCK_W, BLOCK_H), Sense::click_and_drag());
                    paint_block(ui, rect, &spec, block, selected);
                    if let Some(start) = this_drag {
                        if resp.dragged() {
                            let total = resp.interact_pointer_pos().map(|p| p.x - start).unwrap_or(0.0);
                            let steps = (total / (BLOCK_W + BLOCK_GAP)).round();
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
                        }
                    } else if resp.drag_started() {
                        let x = ui.input(|i| i.pointer.interact_pos().map(|p| p.x)).unwrap_or(0.0);
                        state.drag = Some((channel.to_string(), block.id.clone(), x, 0.0));
                    } else if resp.clicked() {
                        state.selected = Some((channel.to_string(), block.id.clone()));
                    }
                    ui.add_space(BLOCK_GAP);
                }
            });
        });

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
