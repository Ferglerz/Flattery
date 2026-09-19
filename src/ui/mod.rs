pub mod graph;

use crate::{
    dsp::Shared,
    params::{DifferenceMode, FftSize, FlatteryParams, ProcessDomain},
    strength::{next_node_id, Polarity, StrengthNode},
    ui::graph::{
        GraphLayout, COLOR_BOOST, COLOR_CUT, CURVE_HIT_DIST, FOOTER_Y, GRAPH_W, GRAPH_X, HIT_DIST,
        WINDOW_H, WINDOW_W,
    },
};
use nih_plug::prelude::*;
use nih_plug_vizia::widgets::util::ModifiersExt;
use nih_plug_vizia::{
    create_vizia_editor,
    vizia::{
        prelude::*,
        vg::{Color, FontId},
    },
    widgets::RawParamEvent,
    ViziaTheming,
};
use pleasant_ui::{
    draw::Draw,
    preferences::AppearanceStore,
    theme::{BG, COLORS, GOLD, MUTED, PANEL, TEAL, TEXT},
    value_edit::{parse_number_with_units, ValueEdit},
    FONT_JETBRAINS_MONO,
};
use std::{
    cell::Cell,
    sync::{atomic::Ordering, Arc, OnceLock},
    time::Duration,
};

static PREFS: OnceLock<AppearanceStore> = OnceLock::new();
fn prefs() -> &'static AppearanceStore {
    PREFS.get_or_init(|| AppearanceStore::new("Flattery"))
}

const HEADER_HEIGHT: f32 = 70.0;
const THEME_BUTTON: (f32, f32, f32, f32) = (870.0, 22.0, 72.0, 26.0);
const BYPASS_BUTTON: (f32, f32, f32, f32) = (954.0, 22.0, 32.0, 26.0);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SliderId {
    OutputGain,
    StereoLink,
    Attack,
    Release,
    InputRms,
    NeighborRadius,
}

const ALL_SLIDERS: &[SliderId] = &[
    SliderId::OutputGain,
    SliderId::StereoLink,
    SliderId::Attack,
    SliderId::Release,
    SliderId::InputRms,
    SliderId::NeighborRadius,
];

#[derive(Clone, Copy, PartialEq, Debug)]
enum DragState {
    LowCut { start_x: f32, start_val: f32 },
    HighCut { start_x: f32, start_val: f32 },
    TiltHandle {
        start_x: f32,
        start_y: f32,
        start_freq: f32,
        start_tilt: f32,
    },
    MaxBoost { start_y: f32, start_val: f32 },
    MaxCut { start_y: f32, start_val: f32 },
    OpMin { start_y: f32, start_val: f32 },
    OpMax { start_y: f32, start_val: f32 },
    StrengthOffset { polarity: Polarity, start_y: f32, start_val: f32 },
    StrengthNode { polarity: Polarity, id: u64 },
    Slider { id: SliderId },
}

pub struct FlatteryView {
    params: Arc<FlatteryParams>,
    shared: Arc<Shared>,
    layout: GraphLayout,
    font: Cell<Option<FontId>>,
    drag: Option<DragState>,
    hover_low_cut: bool,
    hover_high_cut: bool,
    hover_tilt: bool,
    hover_max_boost: bool,
    hover_max_cut: bool,
    hover_curve: Option<Polarity>,
    hover_node: Option<(Polarity, u64)>,
    selected: Option<(Polarity, u64)>,
    mouse: (f32, f32),
    edit: Option<ValueEdit<SliderId>>,
}

fn fmt_hz(f: f32) -> String {
    if f >= 1000.0 {
        format!("{:.1}k", f / 1000.0)
    } else {
        format!("{:.0}Hz", f)
    }
}

fn snapshot_nodes(params: &FlatteryParams, polarity: Polarity) -> Vec<StrengthNode> {
    let lock = match polarity {
        Polarity::Boost => &params.boost_nodes,
        Polarity::Cut => &params.cut_nodes,
    };
    match lock.lock() {
        Ok(guard) => guard.clone(),
        Err(poisoned) => poisoned.into_inner().clone(),
    }
}

impl FlatteryView {
    fn slider_rect(id: SliderId) -> (f32, f32, f32, f32) {
        let col = match id {
            SliderId::OutputGain => 0,
            SliderId::StereoLink => 1,
            SliderId::Attack => 2,
            SliderId::Release => 3,
            SliderId::InputRms => 4,
            SliderId::NeighborRadius => 5,
        };
        let gap = 10.0;
        let n = ALL_SLIDERS.len() as f32;
        let w = (GRAPH_W - gap * (n - 1.0)) / n;
        let x = GRAPH_X + col as f32 * (w + gap);
        (x, FOOTER_Y, w, 50.0)
    }

    fn fft_button_rect(&self) -> (f32, f32, f32, f32) {
        (
            self.layout.gx + self.layout.gw - 114.0,
            self.layout.gy + 8.0,
            106.0,
            22.0,
        )
    }

    fn domain_button_rect(&self) -> (f32, f32, f32, f32) {
        (
            self.layout.gx + self.layout.gw - 114.0,
            self.layout.gy + 34.0,
            106.0,
            22.0,
        )
    }

    fn diff_mode_button_rect(&self) -> (f32, f32, f32, f32) {
        (
            self.layout.gx + self.layout.gw - 114.0,
            self.layout.gy + 60.0,
            106.0,
            22.0,
        )
    }

    fn emit_param_norm(&self, cx: &mut EventContext, ptr: ParamPtr, norm: f32) {
        let norm = norm.clamp(0.0, 1.0);
        cx.emit(RawParamEvent::BeginSetParameter(ptr));
        cx.emit(RawParamEvent::SetParameterNormalized(ptr, norm));
        cx.emit(RawParamEvent::EndSetParameter(ptr));
    }

    fn slider_param_ptr(&self, id: SliderId) -> ParamPtr {
        match id {
            SliderId::OutputGain => self.params.output_gain_db.as_ptr(),
            SliderId::StereoLink => self.params.stereo_link.as_ptr(),
            SliderId::Attack => self.params.attack_ms.as_ptr(),
            SliderId::Release => self.params.release_ms.as_ptr(),
            SliderId::InputRms => self.params.input_rms_ms.as_ptr(),
            SliderId::NeighborRadius => self.params.neighbor_radius.as_ptr(),
        }
    }

    fn get_slider_norm(&self, id: SliderId) -> f32 {
        match id {
            SliderId::OutputGain => self.params.output_gain_db.unmodulated_normalized_value(),
            SliderId::StereoLink => self.params.stereo_link.unmodulated_normalized_value(),
            SliderId::Attack => self.params.attack_ms.unmodulated_normalized_value(),
            SliderId::Release => self.params.release_ms.unmodulated_normalized_value(),
            SliderId::InputRms => self.params.input_rms_ms.unmodulated_normalized_value(),
            SliderId::NeighborRadius => self.params.neighbor_radius.unmodulated_normalized_value(),
        }
    }

    fn slider_info(&self, id: SliderId) -> (&'static str, String, Color) {
        match id {
            SliderId::OutputGain => (
                "OUT GAIN",
                format!("{:.1}dB", self.params.output_gain_db.value()),
                GOLD,
            ),
            SliderId::StereoLink => (
                "STEREO LINK",
                format!("{:.0}%", self.params.stereo_link.value()),
                TEAL,
            ),
            SliderId::Attack => (
                "ATTACK",
                format!("{:.1}ms", self.params.attack_ms.value()),
                COLORS[3],
            ),
            SliderId::Release => (
                "RELEASE",
                format!("{:.0}ms", self.params.release_ms.value()),
                COLORS[3],
            ),
            SliderId::InputRms => (
                "INPUT RMS",
                format!("{:.1}ms", self.params.input_rms_ms.value()),
                COLORS[1],
            ),
            SliderId::NeighborRadius => (
                "RADIUS",
                format!("{} bins", self.params.neighbor_radius.value()),
                COLORS[1],
            ),
        }
    }

    fn set_slider_from_x(&self, cx: &mut EventContext, id: SliderId, mouse_x: f32) {
        let r = Self::slider_rect(id);
        let bar_x = r.0 + 12.0;
        let bar_w = (r.2 - 24.0).max(1.0);
        let norm = ((mouse_x - bar_x) / bar_w).clamp(0.0, 1.0);
        self.emit_param_norm(cx, self.slider_param_ptr(id), norm);
    }

    fn inside(px: f32, py: f32, rect: (f32, f32, f32, f32)) -> bool {
        px >= rect.0 && px <= rect.0 + rect.2 && py >= rect.1 && py <= rect.1 + rect.3
    }

    fn sync_scale(&mut self) {
        self.layout.update_db_scale(
            self.params.max_boost_db.value(),
            self.params.max_cut_db.value(),
        );
    }

    fn strength_pct(&self, polarity: Polarity) -> f32 {
        match polarity {
            Polarity::Boost => self.params.strength_boost.value(),
            Polarity::Cut => self.params.strength_cut.value(),
        }
    }

    fn with_nodes_mut<R>(
        &self,
        polarity: Polarity,
        f: impl FnOnce(&mut Vec<StrengthNode>) -> R,
    ) -> R {
        let lock = match polarity {
            Polarity::Boost => &self.params.boost_nodes,
            Polarity::Cut => &self.params.cut_nodes,
        };
        match lock.lock() {
            Ok(mut guard) => f(&mut guard),
            Err(poisoned) => f(&mut poisoned.into_inner()),
        }
    }

    fn create_node(&mut self, polarity: Polarity, x: f32, y: f32) -> u64 {
        let freq = self.layout.x_to_freq(x);
        let weight = self.layout.y_to_weight(polarity, self.strength_pct(polarity), y);
        self.with_nodes_mut(polarity, |nodes| {
            let id = next_node_id(nodes);
            nodes.push(StrengthNode { id, freq, weight });
            id
        })
    }

    fn delete_selected(&mut self) {
        if let Some((polarity, id)) = self.selected.take() {
            self.with_nodes_mut(polarity, |nodes| {
                nodes.retain(|n| n.id != id);
            });
        }
    }

    fn commit_edit(&mut self, cx: &mut EventContext) {
        if let Some(edit) = self.edit.take() {
            let target = edit.target;
            let text = edit.text;
            let val = match target {
                SliderId::Attack | SliderId::Release | SliderId::InputRms => {
                    parse_number_with_units(&text, &[("ms", 1.0), ("s", 1000.0)])
                }
                SliderId::StereoLink => parse_number_with_units(&text, &[("%", 1.0)]),
                SliderId::NeighborRadius => parse_number_with_units(&text, &[("bins", 1.0), ("bin", 1.0)]),
                SliderId::OutputGain => parse_number_with_units(&text, &[("db", 1.0)]),
            };

            if let Some(v) = val {
                if target == SliderId::NeighborRadius {
                    let norm = self.params.neighbor_radius.preview_normalized(v as i32);
                    self.emit_param_norm(cx, self.params.neighbor_radius.as_ptr(), norm);
                } else {
                    let ptr = self.slider_param_ptr(target);
                    let norm = match target {
                        SliderId::OutputGain => self.params.output_gain_db.preview_normalized(v as f32),
                        SliderId::StereoLink => self.params.stereo_link.preview_normalized(v as f32),
                        SliderId::Attack => self.params.attack_ms.preview_normalized(v as f32),
                        SliderId::Release => self.params.release_ms.preview_normalized(v as f32),
                        SliderId::InputRms => self.params.input_rms_ms.preview_normalized(v as f32),
                        SliderId::NeighborRadius => unreachable!(),
                    };
                    self.emit_param_norm(cx, ptr, norm);
                }
            }
        }
    }
}

impl View for FlatteryView {
    fn element(&self) -> Option<&'static str> {
        Some("flattery-view")
    }

    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|window_event, meta| {
            let bounds = cx.bounds();
            let scale = bounds.w / WINDOW_W;
            let mouse_x = (cx.mouse().cursorx - bounds.x) / scale;
            let mouse_y = (cx.mouse().cursory - bounds.y) / scale;
            self.mouse = (mouse_x, mouse_y);
            self.sync_scale();

            if self.edit.is_some() {
                match window_event {
                    WindowEvent::CharInput(c) => {
                        if !cx.modifiers().command() && c.is_ascii() && !c.is_control() {
                            self.edit.as_mut().unwrap().insert(&c.to_string());
                        }
                        meta.consume();
                        cx.needs_redraw();
                        return;
                    }
                    WindowEvent::KeyDown(Code::Enter, _) => {
                        self.commit_edit(cx);
                        meta.consume();
                        cx.needs_redraw();
                        return;
                    }
                    WindowEvent::KeyDown(Code::Escape, _) => {
                        self.edit = None;
                        meta.consume();
                        cx.needs_redraw();
                        return;
                    }
                    WindowEvent::KeyDown(Code::Backspace, _) => {
                        self.edit.as_mut().unwrap().erase(true);
                        meta.consume();
                        cx.needs_redraw();
                        return;
                    }
                    WindowEvent::MouseDown(MouseButton::Left) => {
                        self.commit_edit(cx);
                        meta.consume();
                        cx.needs_redraw();
                        return;
                    }
                    _ => return,
                }
            }

            match window_event {
                WindowEvent::KeyDown(Code::Delete | Code::Backspace, _) => {
                    if self.selected.is_some() {
                        self.delete_selected();
                        meta.consume();
                        cx.needs_redraw();
                    }
                }

                WindowEvent::MouseDown(MouseButton::Right) => {
                    let max_boost = self.params.max_boost_db.value();
                    let max_cut = self.params.max_cut_db.value();
                    for polarity in [Polarity::Boost, Polarity::Cut] {
                        let nodes = snapshot_nodes(&self.params, polarity);
                        if let Some(id) = self.layout.hit_node(
                            &nodes,
                            polarity,
                            self.strength_pct(polarity),
                            max_boost,
                            max_cut,
                            mouse_x,
                            mouse_y,
                        ) {
                            self.selected = Some((polarity, id));
                            self.delete_selected();
                            meta.consume();
                            cx.needs_redraw();
                            return;
                        }
                    }
                }

                WindowEvent::MouseDown(MouseButton::Left) => {
                    if Self::inside(mouse_x, mouse_y, THEME_BUTTON) {
                        prefs().toggle();
                        cx.needs_redraw();
                        return;
                    }

                    if Self::inside(mouse_x, mouse_y, BYPASS_BUTTON) {
                        let current = self.params.bypass.value();
                        let norm = if !current { 1.0 } else { 0.0 };
                        self.emit_param_norm(cx, self.params.bypass.as_ptr(), norm);
                        cx.needs_redraw();
                        return;
                    }

                    if Self::inside(mouse_x, mouse_y, self.fft_button_rect()) {
                        let current = self.params.fft_size.value();
                        let next_idx = match current {
                            FftSize::Fft128 => 1,
                            FftSize::Fft256 => 2,
                            FftSize::Fft512 => 3,
                            FftSize::Fft1024 => 4,
                            FftSize::Fft2048 => 5,
                            FftSize::Fft4096 => 6,
                            FftSize::Fft8192 => 0,
                        };
                        let norm = next_idx as f32 / 6.0;
                        self.emit_param_norm(cx, self.params.fft_size.as_ptr(), norm);
                        cx.needs_redraw();
                        return;
                    }

                    if Self::inside(mouse_x, mouse_y, self.domain_button_rect()) {
                        let current = self.params.ms_mode.value();
                        let norm = if current == ProcessDomain::LR { 1.0 } else { 0.0 };
                        self.emit_param_norm(cx, self.params.ms_mode.as_ptr(), norm);
                        cx.needs_redraw();
                        return;
                    }

                    if Self::inside(mouse_x, mouse_y, self.diff_mode_button_rect()) {
                        let current = self.params.amplify_mode.value();
                        let norm = if current == DifferenceMode::Reduce {
                            1.0
                        } else {
                            0.0
                        };
                        self.emit_param_norm(cx, self.params.amplify_mode.as_ptr(), norm);
                        cx.needs_redraw();
                        return;
                    }

                    for &id in ALL_SLIDERS {
                        let r = Self::slider_rect(id);
                        if Self::inside(mouse_x, mouse_y, r) {
                            self.drag = Some(DragState::Slider { id });
                            self.set_slider_from_x(cx, id, mouse_x);
                            cx.needs_redraw();
                            return;
                        }
                    }

                    let max_boost = self.params.max_boost_db.value();
                    let max_cut = self.params.max_cut_db.value();

                    for polarity in [Polarity::Boost, Polarity::Cut] {
                        let nodes = snapshot_nodes(&self.params, polarity);
                        if let Some(id) = self.layout.hit_node(
                            &nodes,
                            polarity,
                            self.strength_pct(polarity),
                            max_boost,
                            max_cut,
                            mouse_x,
                            mouse_y,
                        ) {
                            self.selected = Some((polarity, id));
                            self.drag = Some(DragState::StrengthNode { polarity, id });
                            cx.needs_redraw();
                            return;
                        }
                    }

                    if self.layout.hit_max_boost(mouse_x, mouse_y, max_boost) {
                        self.drag = Some(DragState::MaxBoost {
                            start_y: mouse_y,
                            start_val: max_boost,
                        });
                        cx.needs_redraw();
                        return;
                    }
                    if self.layout.hit_max_cut(mouse_x, mouse_y, max_cut) {
                        self.drag = Some(DragState::MaxCut {
                            start_y: mouse_y,
                            start_val: max_cut,
                        });
                        cx.needs_redraw();
                        return;
                    }

                    let low_x = self.layout.freq_to_x(self.params.low_cut_hz.value() as f64);
                    let high_x = self.layout.freq_to_x(self.params.high_cut_hz.value() as f64);
                    let tilt_x = self.layout.freq_to_x(self.params.tilt_freq_hz.value() as f64);

                    if (mouse_x - low_x).abs() <= HIT_DIST
                        && mouse_y >= self.layout.gy
                        && mouse_y <= self.layout.gy + self.layout.gh
                    {
                        self.drag = Some(DragState::LowCut {
                            start_x: mouse_x,
                            start_val: self.params.low_cut_hz.value(),
                        });
                        cx.needs_redraw();
                        return;
                    }

                    if (mouse_x - high_x).abs() <= HIT_DIST
                        && mouse_y >= self.layout.gy
                        && mouse_y <= self.layout.gy + self.layout.gh
                    {
                        self.drag = Some(DragState::HighCut {
                            start_x: mouse_x,
                            start_val: self.params.high_cut_hz.value(),
                        });
                        cx.needs_redraw();
                        return;
                    }

                    if (mouse_x - tilt_x).abs() <= HIT_DIST + 4.0
                        && mouse_y >= self.layout.gy
                        && mouse_y <= self.layout.gy + self.layout.gh
                    {
                        self.drag = Some(DragState::TiltHandle {
                            start_x: mouse_x,
                            start_y: mouse_y,
                            start_freq: self.params.tilt_freq_hz.value(),
                            start_tilt: self.params.tilt.value(),
                        });
                        cx.needs_redraw();
                        return;
                    }

                    if self
                        .layout
                        .hit_op_min(mouse_x, mouse_y, self.params.min_operate_db.value())
                    {
                        self.drag = Some(DragState::OpMin {
                            start_y: mouse_y,
                            start_val: self.params.min_operate_db.value(),
                        });
                        cx.needs_redraw();
                        return;
                    }
                    if self
                        .layout
                        .hit_op_max(mouse_x, mouse_y, self.params.max_operate_db.value())
                    {
                        self.drag = Some(DragState::OpMax {
                            start_y: mouse_y,
                            start_val: self.params.max_operate_db.value(),
                        });
                        cx.needs_redraw();
                        return;
                    }

                    if self.layout.in_graph(mouse_x, mouse_y) {
                        let boost_nodes = snapshot_nodes(&self.params, Polarity::Boost);
                        let cut_nodes = snapshot_nodes(&self.params, Polarity::Cut);
                        let d_boost = self.layout.curve_distance(
                            &boost_nodes,
                            Polarity::Boost,
                            self.strength_pct(Polarity::Boost),
                            mouse_x,
                            mouse_y,
                        );
                        let d_cut = self.layout.curve_distance(
                            &cut_nodes,
                            Polarity::Cut,
                            self.strength_pct(Polarity::Cut),
                            mouse_x,
                            mouse_y,
                        );
                        let (polarity, dist) = if d_boost <= d_cut {
                            (Polarity::Boost, d_boost)
                        } else {
                            (Polarity::Cut, d_cut)
                        };

                        if dist <= CURVE_HIT_DIST {
                            if cx.modifiers().command() {
                                self.drag = Some(DragState::StrengthOffset {
                                    polarity,
                                    start_y: mouse_y,
                                    start_val: self.strength_pct(polarity),
                                });
                            } else {
                                let id = self.create_node(polarity, mouse_x, mouse_y);
                                self.selected = Some((polarity, id));
                                self.drag = Some(DragState::StrengthNode { polarity, id });
                            }
                            cx.needs_redraw();
                            return;
                        }

                        self.selected = None;
                    }
                }

                WindowEvent::MouseDoubleClick(MouseButton::Left) => {
                    for &id in ALL_SLIDERS {
                        let r = Self::slider_rect(id);
                        if Self::inside(mouse_x, mouse_y, r) {
                            let (_, val_str, _) = self.slider_info(id);
                            self.edit = Some(ValueEdit::new(id, r, val_str));
                            cx.needs_redraw();
                            return;
                        }
                    }

                    if self.layout.in_graph(mouse_x, mouse_y) {
                        let max_boost = self.params.max_boost_db.value();
                        let max_cut = self.params.max_cut_db.value();
                        for polarity in [Polarity::Boost, Polarity::Cut] {
                            let nodes = snapshot_nodes(&self.params, polarity);
                            if self
                                .layout
                                .hit_node(
                                    &nodes,
                                    polarity,
                                    self.strength_pct(polarity),
                                    max_boost,
                                    max_cut,
                                    mouse_x,
                                    mouse_y,
                                )
                                .is_some()
                            {
                                return;
                            }
                        }
                        let polarity = if mouse_y <= self.layout.center_y() {
                            Polarity::Boost
                        } else {
                            Polarity::Cut
                        };
                        let id = self.create_node(polarity, mouse_x, mouse_y);
                        self.selected = Some((polarity, id));
                        cx.needs_redraw();
                    }
                }

                WindowEvent::MouseUp(MouseButton::Left) => {
                    self.drag = None;
                    cx.needs_redraw();
                }

                WindowEvent::MouseScroll(_, dy) => {
                    if *dy != 0.0
                        && cx.modifiers().alt()
                        && self.layout.in_graph(mouse_x, mouse_y)
                    {
                        let cur = self.params.neighbor_radius.value();
                        let next = (cur + dy.signum() as i32).clamp(1, 12);
                        let norm = self.params.neighbor_radius.preview_normalized(next);
                        self.emit_param_norm(cx, self.params.neighbor_radius.as_ptr(), norm);
                        meta.consume();
                        cx.needs_redraw();
                    }
                }

                WindowEvent::MouseMove(_, _) => {
                    if let Some(drag) = self.drag {
                        match drag {
                            DragState::LowCut { start_x, start_val } => {
                                let delta = mouse_x - start_x;
                                let new_freq = (self
                                    .layout
                                    .x_to_freq(self.layout.freq_to_x(start_val as f64) + delta)
                                    as f32)
                                    .clamp(10.0, self.params.high_cut_hz.value());
                                let norm = self.params.low_cut_hz.preview_normalized(new_freq);
                                self.emit_param_norm(cx, self.params.low_cut_hz.as_ptr(), norm);
                                cx.needs_redraw();
                            }
                            DragState::HighCut { start_x, start_val } => {
                                let delta = mouse_x - start_x;
                                let new_freq = (self
                                    .layout
                                    .x_to_freq(self.layout.freq_to_x(start_val as f64) + delta)
                                    as f32)
                                    .clamp(self.params.low_cut_hz.value(), 20000.0);
                                let norm = self.params.high_cut_hz.preview_normalized(new_freq);
                                self.emit_param_norm(cx, self.params.high_cut_hz.as_ptr(), norm);
                                cx.needs_redraw();
                            }
                            DragState::TiltHandle {
                                start_x,
                                start_y,
                                start_freq,
                                start_tilt,
                            } => {
                                let delta_x = mouse_x - start_x;
                                let delta_y = start_y - mouse_y;
                                let new_freq = (self
                                    .layout
                                    .x_to_freq(self.layout.freq_to_x(start_freq as f64) + delta_x)
                                    as f32)
                                    .clamp(20.0, 20000.0);
                                let new_tilt = (start_tilt + delta_y * 1.0).clamp(-100.0, 100.0);
                                let norm_freq = self.params.tilt_freq_hz.preview_normalized(new_freq);
                                let norm_tilt = self.params.tilt.preview_normalized(new_tilt);
                                self.emit_param_norm(cx, self.params.tilt_freq_hz.as_ptr(), norm_freq);
                                self.emit_param_norm(cx, self.params.tilt.as_ptr(), norm_tilt);
                                cx.needs_redraw();
                            }
                            DragState::MaxBoost { start_y, start_val } => {
                                let start_db_y = self.layout.db_to_y(start_val as f64);
                                let new_db = self
                                    .layout
                                    .y_to_db(start_db_y + (mouse_y - start_y))
                                    .clamp(0.0, 48.0);
                                let norm = self.params.max_boost_db.preview_normalized(new_db as f32);
                                self.emit_param_norm(cx, self.params.max_boost_db.as_ptr(), norm);
                                self.sync_scale();
                                cx.needs_redraw();
                            }
                            DragState::MaxCut { start_y, start_val } => {
                                let start_db_y = self.layout.db_to_y(-(start_val as f64));
                                let new_db = (-self.layout.y_to_db(start_db_y + (mouse_y - start_y)))
                                    .clamp(0.0, 48.0);
                                let norm = self.params.max_cut_db.preview_normalized(new_db as f32);
                                self.emit_param_norm(cx, self.params.max_cut_db.as_ptr(), norm);
                                self.sync_scale();
                                cx.needs_redraw();
                            }
                            DragState::OpMin { start_y, start_val } => {
                                let start_y_pos = self.layout.mag_to_y(start_val as f64);
                                let new_db = self
                                    .layout
                                    .y_to_mag(start_y_pos + (mouse_y - start_y))
                                    .clamp(-120.0, self.params.max_operate_db.value() as f64);
                                let norm = self.params.min_operate_db.preview_normalized(new_db as f32);
                                self.emit_param_norm(cx, self.params.min_operate_db.as_ptr(), norm);
                                cx.needs_redraw();
                            }
                            DragState::OpMax { start_y, start_val } => {
                                let start_y_pos = self.layout.mag_to_y(start_val as f64);
                                let new_db = self
                                    .layout
                                    .y_to_mag(start_y_pos + (mouse_y - start_y))
                                    .clamp(self.params.min_operate_db.value() as f64, 0.0);
                                let norm = self.params.max_operate_db.preview_normalized(new_db as f32);
                                self.emit_param_norm(cx, self.params.max_operate_db.as_ptr(), norm);
                                cx.needs_redraw();
                            }
                            DragState::StrengthOffset {
                                polarity,
                                start_y,
                                start_val,
                            } => {
                                let start_line = self.layout.strength_line_y(polarity, start_val);
                                let new_y = start_line + (mouse_y - start_y);
                                let new_pct = self.layout.y_to_strength(polarity, new_y);
                                match polarity {
                                    Polarity::Boost => {
                                        let norm = self.params.strength_boost.preview_normalized(new_pct);
                                        self.emit_param_norm(cx, self.params.strength_boost.as_ptr(), norm);
                                    }
                                    Polarity::Cut => {
                                        let norm = self.params.strength_cut.preview_normalized(new_pct);
                                        self.emit_param_norm(cx, self.params.strength_cut.as_ptr(), norm);
                                    }
                                }
                                cx.needs_redraw();
                            }
                            DragState::StrengthNode { polarity, id } => {
                                let freq = self.layout.x_to_freq(mouse_x);
                                let weight = self.layout.y_to_weight(
                                    polarity,
                                    self.strength_pct(polarity),
                                    mouse_y,
                                );
                                self.with_nodes_mut(polarity, |nodes| {
                                    if let Some(node) = nodes.iter_mut().find(|n| n.id == id) {
                                        node.freq = freq;
                                        node.weight = weight;
                                        node.sanitize();
                                    }
                                });
                                cx.needs_redraw();
                            }
                            DragState::Slider { id } => {
                                self.set_slider_from_x(cx, id, mouse_x);
                                cx.needs_redraw();
                            }
                        }
                    } else {
                        let max_boost = self.params.max_boost_db.value();
                        let max_cut = self.params.max_cut_db.value();
                        let low_x = self.layout.freq_to_x(self.params.low_cut_hz.value() as f64);
                        let high_x = self.layout.freq_to_x(self.params.high_cut_hz.value() as f64);
                        let tilt_x = self.layout.freq_to_x(self.params.tilt_freq_hz.value() as f64);

                        self.hover_low_cut = (mouse_x - low_x).abs() <= HIT_DIST
                            && mouse_y >= self.layout.gy
                            && mouse_y <= self.layout.gy + self.layout.gh;
                        self.hover_high_cut = (mouse_x - high_x).abs() <= HIT_DIST
                            && mouse_y >= self.layout.gy
                            && mouse_y <= self.layout.gy + self.layout.gh;
                        self.hover_tilt = (mouse_x - tilt_x).abs() <= HIT_DIST + 4.0
                            && mouse_y >= self.layout.gy
                            && mouse_y <= self.layout.gy + self.layout.gh;
                        self.hover_max_boost = self.layout.hit_max_boost(mouse_x, mouse_y, max_boost);
                        self.hover_max_cut = self.layout.hit_max_cut(mouse_x, mouse_y, max_cut);

                        self.hover_node = None;
                        for polarity in [Polarity::Boost, Polarity::Cut] {
                            let nodes = snapshot_nodes(&self.params, polarity);
                            if let Some(id) = self.layout.hit_node(
                                &nodes,
                                polarity,
                                self.strength_pct(polarity),
                                max_boost,
                                max_cut,
                                mouse_x,
                                mouse_y,
                            ) {
                                self.hover_node = Some((polarity, id));
                                break;
                            }
                        }

                        if self.hover_node.is_none() && self.layout.in_graph(mouse_x, mouse_y) {
                            let boost_nodes = snapshot_nodes(&self.params, Polarity::Boost);
                            let cut_nodes = snapshot_nodes(&self.params, Polarity::Cut);
                            let d_boost = self.layout.curve_distance(
                                &boost_nodes,
                                Polarity::Boost,
                                self.strength_pct(Polarity::Boost),
                                mouse_x,
                                mouse_y,
                            );
                            let d_cut = self.layout.curve_distance(
                                &cut_nodes,
                                Polarity::Cut,
                                self.strength_pct(Polarity::Cut),
                                mouse_x,
                                mouse_y,
                            );
                            self.hover_curve = if d_boost.min(d_cut) <= CURVE_HIT_DIST {
                                Some(if d_boost <= d_cut {
                                    Polarity::Boost
                                } else {
                                    Polarity::Cut
                                })
                            } else {
                                None
                            };
                        } else {
                            self.hover_curve = None;
                        }

                        cx.needs_redraw();
                    }
                }

                _ => {}
            }
        });
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &mut Canvas) {
        let bounds = cx.bounds();
        if self.font.get().is_none() {
            self.font.set(canvas.add_font_mem(FONT_JETBRAINS_MONO).ok());
        }

        let mut d = Draw::new(
            canvas,
            prefs().light(),
            bounds.w / WINDOW_W,
            bounds.x,
            bounds.y,
            self.font.get(),
        );

        d.rect(0.0, 0.0, WINDOW_W, WINDOW_H, BG);
        d.rect(0.0, 0.0, WINDOW_W, HEADER_HEIGHT, PANEL);
        d.text(36.0, 44.0, "FLATTERY", 24.0, GOLD);
        d.text(192.0, 44.0, "SPECTRAL LEVELER & SHAPER", 13.0, TEXT);

        let is_light = prefs().light();
        d.button(
            THEME_BUTTON,
            if is_light { "DARK" } else { "LIGHT" },
            false,
            MUTED,
        );

        d.bypass_button(BYPASS_BUTTON, self.params.bypass.value(), TEAL);

        let srate = self.shared.sample_rate.load(Ordering::Relaxed) as f64;
        let fft_size = self.params.fft_size.value().size();
        let mut layout = self.layout;
        layout.update_db_scale(
            self.params.max_boost_db.value(),
            self.params.max_cut_db.value(),
        );

        layout.draw_background(&mut d, fft_size, srate);
        layout.draw_grid_and_labels(&mut d);

        let low_cut = self.params.low_cut_hz.value() as f64;
        let high_cut = self.params.high_cut_hz.value() as f64;
        let tilt = self.params.tilt.value() as f64;
        let tilt_freq = self.params.tilt_freq_hz.value() as f64;
        let max_boost = self.params.max_boost_db.value();
        let max_cut = self.params.max_cut_db.value();
        let boost_pct = self.params.strength_boost.value();
        let cut_pct = self.params.strength_cut.value();

        layout.draw_operate_window(
            &mut d,
            self.params.min_operate_db.value() as f64,
            self.params.max_operate_db.value() as f64,
        );

        if let Ok(mags) = self.shared.spectrum_mags_db.read() {
            layout.draw_spectrum(&mut d, &mags, fft_size, srate, low_cut, high_cut);
        }

        if self.layout.in_graph(self.mouse.0, self.mouse.1) {
            layout.draw_radius_halo(
                &mut d,
                self.mouse.0,
                fft_size,
                srate,
                self.params.neighbor_radius.value(),
            );
        }

        if let Ok(filters) = self.shared.filter_display.read() {
            layout.draw_filter_gains(&mut d, &filters);
        }

        layout.draw_max_handles(
            &mut d,
            max_boost,
            max_cut,
            self.hover_max_boost,
            self.hover_max_cut,
        );

        let boost_nodes = snapshot_nodes(&self.params, Polarity::Boost);
        let cut_nodes = snapshot_nodes(&self.params, Polarity::Cut);
        let selected_boost = self.selected.and_then(|(p, id)| {
            if p == Polarity::Boost {
                Some(id)
            } else {
                None
            }
        });
        let selected_cut = self.selected.and_then(|(p, id)| {
            if p == Polarity::Cut {
                Some(id)
            } else {
                None
            }
        });

        d.scissor(layout.gx, layout.gy, layout.gw, layout.gh);
        layout.draw_strength_curve(
            &mut d,
            &boost_nodes,
            Polarity::Boost,
            boost_pct,
            max_boost,
            max_cut,
            self.hover_curve == Some(Polarity::Boost) || self.hover_node.map(|h| h.0) == Some(Polarity::Boost),
            selected_boost,
        );
        layout.draw_strength_curve(
            &mut d,
            &cut_nodes,
            Polarity::Cut,
            cut_pct,
            max_boost,
            max_cut,
            self.hover_curve == Some(Polarity::Cut) || self.hover_node.map(|h| h.0) == Some(Polarity::Cut),
            selected_cut,
        );
        d.reset_scissor();

        layout.draw_tilt_curve(&mut d, tilt, tilt_freq, srate);
        layout.draw_tilt_handle(&mut d, tilt, tilt_freq, srate, self.hover_tilt);
        layout.draw_cut_handles(
            &mut d,
            low_cut,
            high_cut,
            self.hover_low_cut,
            self.hover_high_cut,
        );

        if let Some((polarity, id)) = self.selected {
            let nodes = snapshot_nodes(&self.params, polarity);
            if let Some(node) = nodes.iter().find(|n| n.id == id) {
                let x = layout.freq_to_x(node.freq);
                let y = layout.strength_y(polarity, self.strength_pct(polarity), node.weight);
                let label = format!("{}  {:.0}%", fmt_hz(node.freq as f32), node.weight * 100.0);
                d.text(
                    x + 10.0,
                    y - 10.0,
                    &label,
                    11.0,
                    if polarity == Polarity::Boost {
                        COLOR_BOOST
                    } else {
                        COLOR_CUT
                    },
                );
            }
        }

        let fft_rect = self.fft_button_rect();
        d.button(fft_rect, &format!("FFT: {fft_size}"), false, GOLD);
        let domain_label = match self.params.ms_mode.value() {
            ProcessDomain::LR => "MODE: L/R",
            ProcessDomain::MS => "MODE: M/S",
        };
        d.button(self.domain_button_rect(), domain_label, false, TEAL);
        let diff_label = match self.params.amplify_mode.value() {
            DifferenceMode::Reduce => "DIFF: REDUCE",
            DifferenceMode::Amplify => "DIFF: AMPLIFY",
        };
        d.button(self.diff_mode_button_rect(), diff_label, false, COLORS[1]);

        for &id in ALL_SLIDERS {
            let r = Self::slider_rect(id);
            let (label, val_str, color) = self.slider_info(id);
            let n = self.get_slider_norm(id);
            if let Some(edit) = &self.edit {
                if edit.target == id {
                    d.rect(r.0, r.1, r.2, r.3, PANEL);
                    d.outline(r, GOLD);
                    d.text(r.0 + 12.0, r.1 + 18.0, label, 9.5, TEXT);
                    d.text(r.0 + 12.0, r.1 + 40.0, &edit.text, 12.0, GOLD);
                    continue;
                }
            }
            d.control(r, label, &val_str, n, color);
        }
    }
}

pub fn create(params: Arc<FlatteryParams>, shared: Arc<Shared>) -> Option<Box<dyn Editor>> {
    create_vizia_editor(
        params.editor_state.clone(),
        ViziaTheming::Custom,
        move |cx, _| {
            nih_plug_vizia::assets::register_noto_sans_light(cx);
            FlatteryView {
                params: params.clone(),
                shared: shared.clone(),
                layout: GraphLayout::default(),
                font: Cell::new(None),
                drag: None,
                hover_low_cut: false,
                hover_high_cut: false,
                hover_tilt: false,
                hover_max_boost: false,
                hover_max_cut: false,
                hover_curve: None,
                hover_node: None,
                selected: None,
                mouse: (0.0, 0.0),
                edit: None,
            }
            .build(cx, |cx| {
                let timer = cx.add_timer(Duration::from_millis(16), None, |cx, action| {
                    if let TimerAction::Tick(_) = action {
                        cx.needs_redraw();
                    }
                });
                cx.start_timer(timer);
            })
            .width(Stretch(1.0))
            .height(Stretch(1.0));
        },
    )
}
