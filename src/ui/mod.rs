pub mod graph;

use crate::{
    dsp::Shared,
    params::{FftSize, FlatteryParams, ProcessDomain},
    strength::{
        default_node_radius, next_node_id, norm_to_q, q_to_norm, q_to_width_pct, weight_at,
        width_pct_to_q, Polarity, StrengthNode, DEFAULT_NODE_Q,
    },
    ui::graph::{
        snap_to_bin_edge, GraphLayout, COLOR_BOOST, COLOR_BOOST_HOVER, COLOR_CUT, COLOR_CUT_HOVER,
        CURVE_HIT_DIST, EDGE_PAD, GRAPH_H, GRAPH_W, GRAPH_X, GRAPH_Y, HIT_DIST, NODE_ROW_GAP,
        NODE_ROW_Y, NODE_SLIDER_H, SIDE_W, SIDE_X, WINDOW_H, WINDOW_W,
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
    draw::{ButtonAnim, Draw},
    math::{flattery_freq_to_pos, flattery_pos_to_freq},
    preferences::AppearanceStore,
    theme::{BG, COLORS, GOLD, LINE, MUTED, PANEL, TEAL, TEXT},
    value_edit::{parse_number_with_units, slider_value_rect, typed_char, ValueEdit},
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
const THEME_BUTTON: (f32, f32, f32, f32) =
    (WINDOW_W - EDGE_PAD - 32.0 - 12.0 - 72.0, 22.0, 72.0, 26.0);
const BYPASS_BUTTON: (f32, f32, f32, f32) = (WINDOW_W - EDGE_PAD - 32.0, 22.0, 32.0, 26.0);
const SIDE_SLIDER_H: f32 = 50.0;
const SIDE_BTN_H: f32 = 28.0;
const FOOTER_BTN_GAP: f32 = 6.0;
const DOMAIN_BTN_W: f32 = 40.0;
const SIDE_STACK_HEIGHTS: [f32; 5] = [
    SIDE_SLIDER_H,
    SIDE_SLIDER_H,
    SIDE_SLIDER_H,
    SIDE_SLIDER_H,
    SIDE_BTN_H,
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SliderId {
    OutputGain,
    StereoLink,
    Attack,
    Release,
    InputRms,
    NodeFreq,
    NodeGain,
    NodeQ,
    NodeRadius,
}

const STACKED_SLIDERS: &[SliderId] = &[
    SliderId::Attack,
    SliderId::Release,
    SliderId::InputRms,
    SliderId::StereoLink,
];

const NODE_SLIDERS: &[SliderId] = &[
    SliderId::NodeFreq,
    SliderId::NodeGain,
    SliderId::NodeQ,
    SliderId::NodeRadius,
];

#[derive(Clone, Copy, PartialEq, Debug)]
enum DragState {
    LowCut {
        start_x: f32,
        start_val: f32,
    },
    HighCut {
        start_x: f32,
        start_val: f32,
    },
    TiltHandle {
        start_x: f32,
        start_y: f32,
        start_freq: f32,
        start_tilt: f32,
    },
    MaxBoost {
        start_y: f32,
        start_val: f32,
    },
    MaxCut {
        start_y: f32,
        start_val: f32,
    },
    OpMin {
        start_y: f32,
        start_val: f32,
    },
    OpMax {
        start_y: f32,
        start_val: f32,
    },
    StrengthOffset {
        polarity: Polarity,
        start_y: f32,
        start_val: f32,
    },
    StrengthNode {
        polarity: Polarity,
        id: u64,
    },
    Slider {
        id: SliderId,
    },
    OutputGainKnob {
        start_y: f32,
        start_val: f32,
    },
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
    hover_strength: Option<Polarity>,
    hover_curve: Option<Polarity>,
    hover_node: Option<(Polarity, u64)>,
    selected: Option<(Polarity, u64)>,
    mouse: (f32, f32),
    hover: Option<(f32, f32)>,
    edit: Option<ValueEdit<SliderId>>,
    bypass_anim: ButtonAnim,
    graph_zoomed: bool,
}

#[allow(dead_code)]
fn draw_corner_icon(d: &mut Draw, r: (f32, f32, f32, f32), inward: bool, hot: bool, enabled: bool) {
    let fade = if enabled { 1.0 } else { 0.32 };
    let mut panel = PANEL;
    panel.a *= fade;
    let mut line = LINE;
    line.a *= fade;
    d.rounded_rect(r.0, r.1, r.2, r.3, 4.0, panel);
    d.outline(r, line);
    let color = if !enabled {
        Color { a: fade, ..MUTED }
    } else if hot {
        TEXT
    } else {
        MUTED
    };
    let (x, y, w, h) = r;
    let arm = 5.0;
    if inward {
        let m = 4.5;
        d.poly(
            &[(x + m, y + m + arm), (x + m, y + m), (x + m + arm, y + m)],
            color,
            1.3,
        );
        d.poly(
            &[
                (x + w - m - arm, y + m),
                (x + w - m, y + m),
                (x + w - m, y + m + arm),
            ],
            color,
            1.3,
        );
        d.poly(
            &[
                (x + m, y + h - m - arm),
                (x + m, y + h - m),
                (x + m + arm, y + h - m),
            ],
            color,
            1.3,
        );
        d.poly(
            &[
                (x + w - m - arm, y + h - m),
                (x + w - m, y + h - m),
                (x + w - m, y + h - m - arm),
            ],
            color,
            1.3,
        );
    } else {
        let m = 3.0;
        let inset = 8.0;
        d.poly(
            &[
                (x + m, y + inset),
                (x + inset, y + inset),
                (x + inset, y + m),
            ],
            color,
            1.3,
        );
        d.poly(
            &[
                (x + w - inset, y + m),
                (x + w - inset, y + inset),
                (x + w - m, y + inset),
            ],
            color,
            1.3,
        );
        d.poly(
            &[
                (x + inset, y + h - m),
                (x + inset, y + h - inset),
                (x + m, y + h - inset),
            ],
            color,
            1.3,
        );
        d.poly(
            &[
                (x + w - m, y + h - inset),
                (x + w - inset, y + h - inset),
                (x + w - inset, y + h - m),
            ],
            color,
            1.3,
        );
    }
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

pub fn quantize_time_ms(val: f32) -> f32 {
    if val <= 10.0 {
        (val * 10.0).round() / 10.0
    } else if val <= 25.0 {
        val.round()
    } else if val <= 50.0 {
        (val / 5.0).round() * 5.0
    } else if val <= 100.0 {
        (val / 10.0).round() * 10.0
    } else if val <= 200.0 {
        (val / 25.0).round() * 25.0
    } else if val <= 500.0 {
        (val / 50.0).round() * 50.0
    } else {
        (val / 100.0).round() * 100.0
    }
}

impl FlatteryView {
    fn side_item_y(idx: usize) -> f32 {
        let total: f32 = SIDE_STACK_HEIGHTS.iter().sum();
        let gap = (GRAPH_H - total) / (SIDE_STACK_HEIGHTS.len() - 1) as f32;
        let mut y = GRAPH_Y;
        for h in SIDE_STACK_HEIGHTS.iter().take(idx) {
            y += *h + gap;
        }
        y
    }

    fn side_rect(idx: usize) -> (f32, f32, f32, f32) {
        (
            SIDE_X,
            Self::side_item_y(idx),
            SIDE_W,
            SIDE_STACK_HEIGHTS[idx],
        )
    }

    fn node_slider_rect(idx: usize) -> (f32, f32, f32, f32) {
        let gap = 10.0;
        let w = (GRAPH_W - gap * 3.0) / 4.0;
        let x = GRAPH_X + idx as f32 * (w + gap);
        (x, NODE_ROW_Y, w, NODE_SLIDER_H)
    }

    fn slider_rect(id: SliderId) -> (f32, f32, f32, f32) {
        match id {
            SliderId::Attack => Self::side_rect(0),
            SliderId::Release => Self::side_rect(1),
            SliderId::InputRms => Self::side_rect(2),
            SliderId::StereoLink => Self::side_rect(3),
            SliderId::OutputGain => Self::output_knob_rect(),
            SliderId::NodeFreq => Self::node_slider_rect(0),
            SliderId::NodeGain => Self::node_slider_rect(1),
            SliderId::NodeQ => Self::node_slider_rect(2),
            SliderId::NodeRadius => Self::node_slider_rect(3),
        }
    }

    fn output_knob_rect() -> (f32, f32, f32, f32) {
        let w = SIDE_W;
        let h = 108.0;
        let area_y = GRAPH_Y + GRAPH_H;
        let area_h = WINDOW_H - EDGE_PAD - area_y;
        let y = area_y + (area_h - h) * 0.5 + NODE_ROW_GAP;
        (SIDE_X, y, w, h)
    }

    fn output_knob_center() -> (f32, f32) {
        let r = Self::output_knob_rect();
        (r.0 + r.2 * 0.5, r.1 + r.3 * 0.5)
    }

    fn output_knob_value_rect() -> (f32, f32, f32, f32) {
        let (cx, cy) = Self::output_knob_center();
        (cx - 45.0, cy + 38.0, 90.0, 20.0)
    }

    fn footer_button_row() -> (f32, f32, f32, f32) {
        Self::side_rect(4)
    }

    fn fft_button_rect(&self) -> (f32, f32, f32, f32) {
        let (x, y, w, h) = Self::footer_button_row();
        (x, y, w - DOMAIN_BTN_W - FOOTER_BTN_GAP, h)
    }

    fn domain_button_rect(&self) -> (f32, f32, f32, f32) {
        let (x, y, w, h) = Self::footer_button_row();
        (x + w - DOMAIN_BTN_W, y, DOMAIN_BTN_W, h)
    }

    fn emit_param_norm(&self, cx: &mut EventContext, ptr: ParamPtr, norm: f32) {
        let norm = norm.clamp(0.0, 1.0);
        cx.emit(RawParamEvent::BeginSetParameter(ptr));
        cx.emit(RawParamEvent::SetParameterNormalized(ptr, norm));
        cx.emit(RawParamEvent::EndSetParameter(ptr));
    }

    fn reset_float_param(&self, cx: &mut EventContext, p: &FloatParam) {
        self.emit_param_norm(cx, p.as_ptr(), p.default_normalized_value());
    }

    fn reset_host_slider(&self, cx: &mut EventContext, id: SliderId) {
        let p = match id {
            SliderId::OutputGain => &self.params.output_gain_db,
            SliderId::StereoLink => &self.params.stereo_link,
            SliderId::Attack => &self.params.attack_ms,
            SliderId::Release => &self.params.release_ms,
            SliderId::InputRms => &self.params.input_rms_ms,
            _ => return,
        };
        self.reset_float_param(cx, p);
    }

    fn slider_param_ptr(&self, id: SliderId) -> ParamPtr {
        match id {
            SliderId::OutputGain => self.params.output_gain_db.as_ptr(),
            SliderId::StereoLink => self.params.stereo_link.as_ptr(),
            SliderId::Attack => self.params.attack_ms.as_ptr(),
            SliderId::Release => self.params.release_ms.as_ptr(),
            SliderId::InputRms => self.params.input_rms_ms.as_ptr(),
            _ => unreachable!("node sliders do not have ParamPtr"),
        }
    }

    fn get_slider_norm(&self, id: SliderId) -> f32 {
        match id {
            SliderId::OutputGain => self.params.output_gain_db.unmodulated_normalized_value(),
            SliderId::StereoLink => self.params.stereo_link.unmodulated_normalized_value(),
            SliderId::Attack => self.params.attack_ms.unmodulated_normalized_value(),
            SliderId::Release => self.params.release_ms.unmodulated_normalized_value(),
            SliderId::InputRms => self.params.input_rms_ms.unmodulated_normalized_value(),
            SliderId::NodeFreq => {
                if let Some((polarity, node_id)) = self.selected {
                    let nodes = snapshot_nodes(&self.params, polarity);
                    if let Some(node) = nodes.iter().find(|n| n.id == node_id) {
                        return flattery_freq_to_pos(node.freq, 10.0, 22050.0) as f32;
                    }
                }
                0.0
            }
            SliderId::NodeGain => {
                if let Some((polarity, node_id)) = self.selected {
                    let nodes = snapshot_nodes(&self.params, polarity);
                    if let Some(node) = nodes.iter().find(|n| n.id == node_id) {
                        return (node.weight as f32 / 3.0).clamp(0.0, 1.0);
                    }
                }
                0.0
            }
            SliderId::NodeQ => {
                if let Some((polarity, node_id)) = self.selected {
                    let nodes = snapshot_nodes(&self.params, polarity);
                    if let Some(node) = nodes.iter().find(|n| n.id == node_id) {
                        return q_to_norm(node.q) as f32;
                    }
                }
                0.0
            }
            SliderId::NodeRadius => {
                if let Some((polarity, node_id)) = self.selected {
                    let nodes = snapshot_nodes(&self.params, polarity);
                    if let Some(node) = nodes.iter().find(|n| n.id == node_id) {
                        return ((node.radius as f32 - 1.0) / 11.0).clamp(0.0, 1.0);
                    }
                }
                0.0
            }
        }
    }

    fn slider_info(&self, id: SliderId) -> (&'static str, String, Color) {
        match id {
            SliderId::OutputGain => (
                "OUTPUT GAIN",
                format!("{:.1}dB", self.params.output_gain_db.value()),
                GOLD,
            ),
            SliderId::StereoLink => (
                "STEREO LINK",
                format!("{:.0}%", self.params.stereo_link.value()),
                TEAL,
            ),
            SliderId::Attack => {
                let v = self.params.attack_ms.value();
                let s = if v < 10.0 {
                    format!("{:.1}ms", v)
                } else {
                    format!("{:.0}ms", v)
                };
                ("ATTACK", s, COLORS[3])
            }
            SliderId::Release => {
                let v = self.params.release_ms.value();
                let s = if v < 10.0 {
                    format!("{:.1}ms", v)
                } else {
                    format!("{:.0}ms", v)
                };
                ("RELEASE", s, COLORS[3])
            }
            SliderId::InputRms => (
                "INPUT RMS",
                format!("{:.1}ms", self.params.input_rms_ms.value()),
                COLORS[1],
            ),
            SliderId::NodeFreq => {
                if let Some((polarity, node_id)) = self.selected {
                    let nodes = snapshot_nodes(&self.params, polarity);
                    if let Some(node) = nodes.iter().find(|n| n.id == node_id) {
                        return ("FREQ", fmt_hz(node.freq as f32), GOLD);
                    }
                }
                ("FREQ", "-".to_string(), MUTED)
            }
            SliderId::NodeGain => {
                if let Some((polarity, node_id)) = self.selected {
                    let nodes = snapshot_nodes(&self.params, polarity);
                    if let Some(node) = nodes.iter().find(|n| n.id == node_id) {
                        let color = match polarity {
                            Polarity::Boost => COLOR_BOOST,
                            Polarity::Cut => COLOR_CUT,
                        };
                        return ("GAIN", format!("{:.2}x", node.weight), color);
                    }
                }
                ("GAIN", "-".to_string(), MUTED)
            }
            SliderId::NodeQ => {
                if let Some((polarity, node_id)) = self.selected {
                    let nodes = snapshot_nodes(&self.params, polarity);
                    if let Some(node) = nodes.iter().find(|n| n.id == node_id) {
                        return (
                            "WIDTH",
                            format!("{:.0}%", q_to_width_pct(node.q)),
                            COLORS[2],
                        );
                    }
                }
                ("WIDTH", "-".to_string(), MUTED)
            }
            SliderId::NodeRadius => {
                if let Some((polarity, node_id)) = self.selected {
                    let nodes = snapshot_nodes(&self.params, polarity);
                    if let Some(node) = nodes.iter().find(|n| n.id == node_id) {
                        return ("RADIUS", format!("{} bins", node.radius), COLORS[1]);
                    }
                }
                ("RADIUS", "-".to_string(), MUTED)
            }
        }
    }

    fn set_slider_from_x(&self, cx: &mut EventContext, id: SliderId, mouse_x: f32) {
        let r = Self::slider_rect(id);
        let bar_x = r.0 + 12.0;
        let bar_w = (r.2 - 24.0).max(1.0);
        let raw_norm = ((mouse_x - bar_x) / bar_w).clamp(0.0, 1.0);
        match id {
            SliderId::Attack => {
                let val = self.params.attack_ms.preview_plain(raw_norm);
                let stepped = quantize_time_ms(val).clamp(0.1, 200.0);
                let norm = self.params.attack_ms.preview_normalized(stepped);
                self.emit_param_norm(cx, self.params.attack_ms.as_ptr(), norm);
            }
            SliderId::Release => {
                let val = self.params.release_ms.preview_plain(raw_norm);
                let stepped = quantize_time_ms(val).clamp(1.0, 2000.0);
                let norm = self.params.release_ms.preview_normalized(stepped);
                self.emit_param_norm(cx, self.params.release_ms.as_ptr(), norm);
            }
            SliderId::InputRms => {
                self.emit_param_norm(cx, self.params.input_rms_ms.as_ptr(), raw_norm);
            }
            SliderId::StereoLink => {
                self.emit_param_norm(cx, self.params.stereo_link.as_ptr(), raw_norm);
            }
            SliderId::OutputGain => {
                self.emit_param_norm(cx, self.params.output_gain_db.as_ptr(), raw_norm);
            }
            SliderId::NodeFreq => {
                if let Some((polarity, node_id)) = self.selected {
                    let new_freq =
                        self.snap_hz(flattery_pos_to_freq(raw_norm as f64, 10.0, 22050.0));
                    self.with_nodes_mut(polarity, |nodes| {
                        if let Some(node) = nodes.iter_mut().find(|n| n.id == node_id) {
                            node.freq = new_freq;
                            node.sanitize();
                        }
                    });
                }
            }
            SliderId::NodeGain => {
                if let Some((polarity, node_id)) = self.selected {
                    let new_weight = raw_norm as f64 * 3.0;
                    self.with_nodes_mut(polarity, |nodes| {
                        if let Some(node) = nodes.iter_mut().find(|n| n.id == node_id) {
                            node.weight = new_weight;
                            node.sanitize();
                        }
                    });
                }
            }
            SliderId::NodeQ => {
                if let Some((polarity, node_id)) = self.selected {
                    let new_q = norm_to_q(raw_norm as f64);
                    self.with_nodes_mut(polarity, |nodes| {
                        if let Some(node) = nodes.iter_mut().find(|n| n.id == node_id) {
                            node.q = new_q;
                            node.sanitize();
                        }
                    });
                }
            }
            SliderId::NodeRadius => {
                if let Some((polarity, node_id)) = self.selected {
                    let new_radius = (1.0 + raw_norm * 11.0).round() as usize;
                    self.with_nodes_mut(polarity, |nodes| {
                        if let Some(node) = nodes.iter_mut().find(|n| n.id == node_id) {
                            node.radius = new_radius.clamp(1, 12);
                        }
                    });
                }
            }
        }
    }

    fn inside(px: f32, py: f32, rect: (f32, f32, f32, f32)) -> bool {
        px >= rect.0 && px <= rect.0 + rect.2 && py >= rect.1 && py <= rect.1 + rect.3
    }

    fn sync_scale(&mut self) {
        self.layout.set_view(
            self.params.max_boost_db.value(),
            self.params.max_cut_db.value(),
            self.params.low_cut_hz.value() as f64,
            self.params.high_cut_hz.value() as f64,
            self.graph_zoomed,
        );
    }

    /// In sits at the graph's top-right. Out appears to its left once zoomed.
    fn zoom_button_rects(
        &self,
        layout: &GraphLayout,
    ) -> (Option<(f32, f32, f32, f32)>, Option<(f32, f32, f32, f32)>) {
        let show_in = layout.work_is_narrow(
            self.params.low_cut_hz.value() as f64,
            self.params.high_cut_hz.value() as f64,
            self.params.max_boost_db.value(),
            self.params.max_cut_db.value(),
        );
        let show_out = self.graph_zoomed;
        if !show_in && !show_out {
            return (None, None);
        }
        let size = 22.0;
        let gap = 4.0;
        let y = layout.gy + 6.0;
        let mut left = layout.gx + layout.gw - 6.0 - size;
        let in_r = if show_in {
            let r = (left, y, size, size);
            left -= size + gap;
            Some(r)
        } else {
            None
        };
        let out_r = if show_out {
            Some((left, y, size, size))
        } else {
            None
        };
        (in_r, out_r)
    }

    fn zoom_would_tighten(&self, layout: &GraphLayout) -> bool {
        layout.zoom_would_tighten(
            self.params.low_cut_hz.value() as f64,
            self.params.high_cut_hz.value() as f64,
            self.params.max_boost_db.value(),
            self.params.max_cut_db.value(),
            self.bin_hz(),
        )
    }

    fn strength_pct(&self, polarity: Polarity) -> f32 {
        match polarity {
            Polarity::Boost => self.params.strength_boost.value(),
            Polarity::Cut => self.params.strength_cut.value(),
        }
    }

    fn bin_hz(&self) -> f64 {
        let fft = self.params.fft_size.value().size();
        let srate = self.shared.sample_rate.load(Ordering::Relaxed) as f64;
        if fft == 0 || srate <= 0.0 {
            0.0
        } else {
            srate / fft as f64
        }
    }

    fn snap_hz(&self, freq: f64) -> f64 {
        snap_to_bin_edge(freq, self.bin_hz())
    }

    fn idle_hover(&self) -> Option<(f32, f32)> {
        pleasant_ui::idle_hover(self.hover, self.drag.is_some())
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

    fn create_node(&mut self, polarity: Polarity, x: f32) -> u64 {
        let freq = self.snap_hz(self.layout.x_to_freq(x));
        let min_freq = self.layout.min_freq;
        let max_freq = self.layout.max_freq;
        self.with_nodes_mut(polarity, |nodes| {
            let id = next_node_id(nodes);
            let weight = weight_at(nodes, freq, min_freq, max_freq);
            nodes.push(StrengthNode::new(id, freq, weight));
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

    fn start_edit(
        &mut self,
        cx: &mut EventContext,
        target: SliderId,
        rect: (f32, f32, f32, f32),
        val_str: String,
    ) {
        cx.focus();
        self.edit = Some(ValueEdit::new(target, rect, val_str));
    }

    fn commit_edit(&mut self, cx: &mut EventContext) {
        if let Some(edit) = self.edit.take() {
            let target = edit.target;
            let text = edit.text;
            match target {
                SliderId::Attack | SliderId::Release | SliderId::InputRms => {
                    if let Some(v) = parse_number_with_units(&text, &[("ms", 1.0), ("s", 1000.0)]) {
                        let ptr = self.slider_param_ptr(target);
                        let norm = match target {
                            SliderId::Attack => {
                                let stepped = quantize_time_ms(v as f32).clamp(0.1, 200.0);
                                self.params.attack_ms.preview_normalized(stepped)
                            }
                            SliderId::Release => {
                                let stepped = quantize_time_ms(v as f32).clamp(1.0, 2000.0);
                                self.params.release_ms.preview_normalized(stepped)
                            }
                            SliderId::InputRms => {
                                self.params.input_rms_ms.preview_normalized(v as f32)
                            }
                            _ => unreachable!(),
                        };
                        self.emit_param_norm(cx, ptr, norm);
                    }
                }
                SliderId::StereoLink => {
                    if let Some(v) = parse_number_with_units(&text, &[("%", 1.0)]) {
                        let norm = self.params.stereo_link.preview_normalized(v as f32);
                        self.emit_param_norm(cx, self.params.stereo_link.as_ptr(), norm);
                    }
                }
                SliderId::OutputGain => {
                    if let Some(v) = parse_number_with_units(&text, &[("db", 1.0)]) {
                        let norm = self.params.output_gain_db.preview_normalized(v as f32);
                        self.emit_param_norm(cx, self.params.output_gain_db.as_ptr(), norm);
                    }
                }
                SliderId::NodeFreq => {
                    if let Some(v) = parse_number_with_units(
                        &text,
                        &[("khz", 1000.0), ("k", 1000.0), ("hz", 1.0)],
                    ) {
                        if let Some((polarity, id)) = self.selected {
                            let freq = self.snap_hz(v);
                            self.with_nodes_mut(polarity, |nodes| {
                                if let Some(node) = nodes.iter_mut().find(|n| n.id == id) {
                                    node.freq = freq;
                                    node.sanitize();
                                }
                            });
                        }
                    }
                }
                SliderId::NodeGain => {
                    if let Some(v) = parse_number_with_units(&text, &[("x", 1.0)]) {
                        if let Some((polarity, id)) = self.selected {
                            self.with_nodes_mut(polarity, |nodes| {
                                if let Some(node) = nodes.iter_mut().find(|n| n.id == id) {
                                    node.weight = v;
                                    node.sanitize();
                                }
                            });
                        }
                    }
                }
                SliderId::NodeQ => {
                    if let Some(v) = parse_number_with_units(&text, &[("%", 1.0)]) {
                        if let Some((polarity, id)) = self.selected {
                            self.with_nodes_mut(polarity, |nodes| {
                                if let Some(node) = nodes.iter_mut().find(|n| n.id == id) {
                                    node.q = width_pct_to_q(v);
                                    node.sanitize();
                                }
                            });
                        }
                    }
                }
                SliderId::NodeRadius => {
                    if let Some(v) = parse_number_with_units(&text, &[("bins", 1.0), ("bin", 1.0)])
                    {
                        if let Some((polarity, id)) = self.selected {
                            self.with_nodes_mut(polarity, |nodes| {
                                if let Some(node) = nodes.iter_mut().find(|n| n.id == id) {
                                    node.radius = (v.round() as usize).clamp(1, 12);
                                }
                            });
                        }
                    }
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
                    WindowEvent::KeyDown(code, key) => {
                        match code {
                            Code::Enter | Code::NumpadEnter => {
                                self.commit_edit(cx);
                            }
                            Code::Escape => {
                                self.edit = None;
                            }
                            _ => {
                                self.edit.as_mut().unwrap().handle_key(cx, *code);
                                let has_char = matches!(key, Some(Key::Character(_)));
                                if !has_char && !cx.modifiers().command() {
                                    if let Some(c) = typed_char(*code, cx.modifiers().shift()) {
                                        self.edit.as_mut().unwrap().insert(&c.to_string());
                                    }
                                }
                            }
                        }
                        meta.consume();
                        cx.needs_redraw();
                        return;
                    }
                    WindowEvent::MouseDown(MouseButton::Left) => {
                        let edit_rect = self.edit.as_ref().unwrap().rect;
                        if Self::inside(mouse_x, mouse_y, edit_rect) {
                            self.edit.as_mut().unwrap().handle_mouse_down(mouse_x);
                            meta.consume();
                            cx.needs_redraw();
                            return;
                        }
                        self.commit_edit(cx);
                        meta.consume();
                        cx.needs_redraw();
                        return;
                    }
                    WindowEvent::MouseDoubleClick(MouseButton::Left) => {
                        self.edit.as_mut().unwrap().select_all();
                        meta.consume();
                        cx.needs_redraw();
                        return;
                    }
                    WindowEvent::FocusOut => {
                        self.commit_edit(cx);
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
                        self.bypass_anim.trigger_click();
                        cx.needs_redraw();
                        return;
                    }

                    let (zoom_in, zoom_out) = self.zoom_button_rects(&self.layout);
                    if zoom_in.is_some_and(|r| Self::inside(mouse_x, mouse_y, r)) {
                        if self.zoom_would_tighten(&self.layout) {
                            self.graph_zoomed = true;
                            self.layout.apply_work_zoom(
                                self.params.low_cut_hz.value() as f64,
                                self.params.high_cut_hz.value() as f64,
                                self.params.max_boost_db.value(),
                                self.params.max_cut_db.value(),
                                self.bin_hz(),
                            );
                        }
                        cx.needs_redraw();
                        return;
                    }
                    if zoom_out.is_some_and(|r| Self::inside(mouse_x, mouse_y, r)) {
                        self.graph_zoomed = false;
                        self.sync_scale();
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
                        let norm = if current == ProcessDomain::LR {
                            1.0
                        } else {
                            0.0
                        };
                        self.emit_param_norm(cx, self.params.ms_mode.as_ptr(), norm);
                        cx.needs_redraw();
                        return;
                    }

                    for &id in STACKED_SLIDERS {
                        let r = Self::slider_rect(id);
                        let val_r = slider_value_rect(r);
                        if Self::inside(mouse_x, mouse_y, val_r) {
                            let (_, val_str, _) = self.slider_info(id);
                            self.start_edit(cx, id, val_r, val_str);
                            cx.needs_redraw();
                            return;
                        }
                        if Self::inside(mouse_x, mouse_y, r) {
                            self.drag = Some(DragState::Slider { id });
                            self.set_slider_from_x(cx, id, mouse_x);
                            cx.needs_redraw();
                            return;
                        }
                    }

                    if Self::inside(mouse_x, mouse_y, Self::output_knob_value_rect()) {
                        let (_, val_str, _) = self.slider_info(SliderId::OutputGain);
                        self.start_edit(
                            cx,
                            SliderId::OutputGain,
                            Self::output_knob_value_rect(),
                            val_str,
                        );
                        cx.needs_redraw();
                        return;
                    }
                    let knob_c = Self::output_knob_center();
                    let knob_dist_sq = (mouse_x - knob_c.0).powi(2) + (mouse_y - knob_c.1).powi(2);
                    if knob_dist_sq <= 34.0 * 34.0 {
                        self.drag = Some(DragState::OutputGainKnob {
                            start_y: mouse_y,
                            start_val: self.params.output_gain_db.value(),
                        });
                        cx.needs_redraw();
                        return;
                    }

                    if self.selected.is_some() {
                        for &id in NODE_SLIDERS {
                            let r = Self::slider_rect(id);
                            let val_r = slider_value_rect(r);
                            if Self::inside(mouse_x, mouse_y, val_r) {
                                let (_, val_str, _) = self.slider_info(id);
                                self.start_edit(cx, id, val_r, val_str);
                                cx.needs_redraw();
                                return;
                            }
                            if Self::inside(mouse_x, mouse_y, r) {
                                self.drag = Some(DragState::Slider { id });
                                self.set_slider_from_x(cx, id, mouse_x);
                                cx.needs_redraw();
                                return;
                            }
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

                    for polarity in [Polarity::Boost, Polarity::Cut] {
                        if self.layout.hit_strength_handle(
                            polarity,
                            self.strength_pct(polarity),
                            mouse_x,
                            mouse_y,
                        ) {
                            self.drag = Some(DragState::StrengthOffset {
                                polarity,
                                start_y: mouse_y,
                                start_val: self.strength_pct(polarity),
                            });
                            cx.needs_redraw();
                            return;
                        }
                    }

                    let low_x = self.layout.freq_to_x(self.params.low_cut_hz.value() as f64);
                    let high_x = self
                        .layout
                        .freq_to_x(self.params.high_cut_hz.value() as f64);
                    let tilt_x = self
                        .layout
                        .freq_to_x(self.params.tilt_freq_hz.value() as f64);

                    if self.layout.hit_cut_line(mouse_x, mouse_y, low_x) {
                        self.drag = Some(DragState::LowCut {
                            start_x: mouse_x,
                            start_val: self.params.low_cut_hz.value(),
                        });
                        cx.needs_redraw();
                        return;
                    }

                    if self.layout.hit_cut_line(mouse_x, mouse_y, high_x) {
                        self.drag = Some(DragState::HighCut {
                            start_x: mouse_x,
                            start_val: self.params.high_cut_hz.value(),
                        });
                        cx.needs_redraw();
                        return;
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
                                let id = self.create_node(polarity, mouse_x);
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
                    let knob_c = Self::output_knob_center();
                    let knob_dist_sq = (mouse_x - knob_c.0).powi(2) + (mouse_y - knob_c.1).powi(2);
                    if knob_dist_sq <= 35.0 * 35.0 {
                        self.reset_float_param(cx, &self.params.output_gain_db);
                        self.drag = None;
                        cx.needs_redraw();
                        return;
                    }

                    for &id in STACKED_SLIDERS {
                        let r = Self::slider_rect(id);
                        let val_r = slider_value_rect(r);
                        if Self::inside(mouse_x, mouse_y, val_r) {
                            continue;
                        }
                        if Self::inside(mouse_x, mouse_y, r) {
                            self.reset_host_slider(cx, id);
                            self.drag = None;
                            cx.needs_redraw();
                            return;
                        }
                    }

                    if self.selected.is_some() {
                        for &id in NODE_SLIDERS {
                            let r = Self::slider_rect(id);
                            let val_r = slider_value_rect(r);
                            if Self::inside(mouse_x, mouse_y, val_r) {
                                continue;
                            }
                            if Self::inside(mouse_x, mouse_y, r) {
                                if let Some((polarity, node_id)) = self.selected {
                                    self.with_nodes_mut(polarity, |nodes| {
                                        if let Some(node) =
                                            nodes.iter_mut().find(|n| n.id == node_id)
                                        {
                                            match id {
                                                SliderId::NodeFreq => node.freq = 1000.0,
                                                SliderId::NodeGain => node.weight = 1.0,
                                                SliderId::NodeQ => node.q = DEFAULT_NODE_Q,
                                                SliderId::NodeRadius => {
                                                    node.radius = default_node_radius()
                                                }
                                                _ => {}
                                            }
                                            node.sanitize();
                                        }
                                    });
                                    self.drag = None;
                                    cx.needs_redraw();
                                    return;
                                }
                            }
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
                            self.with_nodes_mut(polarity, |nodes| {
                                if let Some(node) = nodes.iter_mut().find(|n| n.id == id) {
                                    node.weight = 1.0;
                                    node.q = DEFAULT_NODE_Q;
                                    node.radius = default_node_radius();
                                    node.sanitize();
                                }
                            });
                            self.drag = None;
                            cx.needs_redraw();
                            return;
                        }
                    }
                    for polarity in [Polarity::Boost, Polarity::Cut] {
                        if self.layout.hit_strength_handle(
                            polarity,
                            self.strength_pct(polarity),
                            mouse_x,
                            mouse_y,
                        ) {
                            let p = match polarity {
                                Polarity::Boost => &self.params.strength_boost,
                                Polarity::Cut => &self.params.strength_cut,
                            };
                            self.reset_float_param(cx, p);
                            self.drag = None;
                            cx.needs_redraw();
                            return;
                        }
                    }
                    let low_x = self.layout.freq_to_x(self.params.low_cut_hz.value() as f64);
                    let high_x = self
                        .layout
                        .freq_to_x(self.params.high_cut_hz.value() as f64);
                    let tilt_x = self
                        .layout
                        .freq_to_x(self.params.tilt_freq_hz.value() as f64);
                    if self.layout.hit_cut_line(mouse_x, mouse_y, low_x) {
                        self.reset_float_param(cx, &self.params.low_cut_hz);
                        self.drag = None;
                        cx.needs_redraw();
                        return;
                    }
                    if self.layout.hit_cut_line(mouse_x, mouse_y, high_x) {
                        self.reset_float_param(cx, &self.params.high_cut_hz);
                        self.drag = None;
                        cx.needs_redraw();
                        return;
                    }
                    if self.layout.hit_max_boost(mouse_x, mouse_y, max_boost) {
                        self.reset_float_param(cx, &self.params.max_boost_db);
                        self.drag = None;
                        cx.needs_redraw();
                        return;
                    }
                    if self.layout.hit_max_cut(mouse_x, mouse_y, max_cut) {
                        self.reset_float_param(cx, &self.params.max_cut_db);
                        self.drag = None;
                        cx.needs_redraw();
                        return;
                    }
                    if (mouse_x - tilt_x).abs() <= HIT_DIST + 4.0
                        && mouse_y >= self.layout.gy
                        && mouse_y <= self.layout.gy + self.layout.gh
                    {
                        self.reset_float_param(cx, &self.params.tilt);
                        self.reset_float_param(cx, &self.params.tilt_freq_hz);
                        self.drag = None;
                        cx.needs_redraw();
                        return;
                    }
                    if self
                        .layout
                        .hit_op_min(mouse_x, mouse_y, self.params.min_operate_db.value())
                    {
                        self.reset_float_param(cx, &self.params.min_operate_db);
                        self.drag = None;
                        cx.needs_redraw();
                        return;
                    }
                    if self
                        .layout
                        .hit_op_max(mouse_x, mouse_y, self.params.max_operate_db.value())
                    {
                        self.reset_float_param(cx, &self.params.max_operate_db);
                        self.drag = None;
                        cx.needs_redraw();
                        return;
                    }

                    if self.layout.in_graph(mouse_x, mouse_y) {
                        let polarity = if mouse_y <= self.layout.center_y() {
                            Polarity::Boost
                        } else {
                            Polarity::Cut
                        };
                        let id = self.create_node(polarity, mouse_x);
                        self.selected = Some((polarity, id));
                        cx.needs_redraw();
                    }
                }

                WindowEvent::MouseUp(MouseButton::Left) => {
                    self.drag = None;
                    cx.needs_redraw();
                }

                WindowEvent::MouseScroll(_, dy) => {
                    if *dy != 0.0 {
                        let node_id = self.hover_node.or(self.selected);
                        if !cx.modifiers().alt() {
                            if let Some((polarity, id)) = node_id {
                                self.with_nodes_mut(polarity, |nodes| {
                                    if let Some(node) = nodes.iter_mut().find(|n| n.id == id) {
                                        let next = node.q * 1.08_f64.powf(-dy.signum() as f64);
                                        node.q = next.clamp(
                                            crate::strength::MIN_NODE_Q,
                                            crate::strength::MAX_NODE_Q,
                                        );
                                    }
                                });
                                meta.consume();
                                cx.needs_redraw();
                                return;
                            }
                        }
                        if cx.modifiers().alt() {
                            if let Some((polarity, id)) = node_id {
                                self.with_nodes_mut(polarity, |nodes| {
                                    if let Some(node) = nodes.iter_mut().find(|n| n.id == id) {
                                        let next = (node.radius as i32 + dy.signum() as i32)
                                            .clamp(1, 12)
                                            as usize;
                                        node.radius = next;
                                    }
                                });
                                meta.consume();
                                cx.needs_redraw();
                                return;
                            }
                            if self.layout.in_graph(mouse_x, mouse_y) {
                                let cur = self.params.neighbor_radius.value();
                                let next = (cur + dy.signum() as i32).clamp(1, 12);
                                let norm = self.params.neighbor_radius.preview_normalized(next);
                                self.emit_param_norm(
                                    cx,
                                    self.params.neighbor_radius.as_ptr(),
                                    norm,
                                );
                                meta.consume();
                                cx.needs_redraw();
                            }
                        }
                    }
                }

                WindowEvent::MouseMove(_, _) => {
                    self.hover = Some((mouse_x, mouse_y));
                    if let Some(drag) = self.drag {
                        match drag {
                            DragState::LowCut { start_x, start_val } => {
                                let delta = mouse_x - start_x;
                                let new_freq = (self.snap_hz(
                                    self.layout
                                        .x_to_freq(self.layout.freq_to_x(start_val as f64) + delta),
                                ) as f32)
                                    .clamp(10.0, self.params.high_cut_hz.value());
                                let norm = self.params.low_cut_hz.preview_normalized(new_freq);
                                self.emit_param_norm(cx, self.params.low_cut_hz.as_ptr(), norm);
                                cx.needs_redraw();
                            }
                            DragState::HighCut { start_x, start_val } => {
                                let delta = mouse_x - start_x;
                                let new_freq = (self.snap_hz(
                                    self.layout
                                        .x_to_freq(self.layout.freq_to_x(start_val as f64) + delta),
                                ) as f32)
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
                                let norm_freq =
                                    self.params.tilt_freq_hz.preview_normalized(new_freq);
                                let norm_tilt = self.params.tilt.preview_normalized(new_tilt);
                                self.emit_param_norm(
                                    cx,
                                    self.params.tilt_freq_hz.as_ptr(),
                                    norm_freq,
                                );
                                self.emit_param_norm(cx, self.params.tilt.as_ptr(), norm_tilt);
                                cx.needs_redraw();
                            }
                            DragState::MaxBoost { start_y, start_val } => {
                                let start_db_y = self.layout.db_to_y(start_val as f64);
                                let new_db = self
                                    .layout
                                    .y_to_db(start_db_y + (mouse_y - start_y))
                                    .clamp(0.0, 48.0);
                                let norm =
                                    self.params.max_boost_db.preview_normalized(new_db as f32);
                                self.emit_param_norm(cx, self.params.max_boost_db.as_ptr(), norm);
                                self.sync_scale();
                                cx.needs_redraw();
                            }
                            DragState::MaxCut { start_y, start_val } => {
                                let start_db_y = self.layout.db_to_y(-(start_val as f64));
                                let new_db =
                                    (-self.layout.y_to_db(start_db_y + (mouse_y - start_y)))
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
                                let norm =
                                    self.params.min_operate_db.preview_normalized(new_db as f32);
                                self.emit_param_norm(cx, self.params.min_operate_db.as_ptr(), norm);
                                cx.needs_redraw();
                            }
                            DragState::OpMax { start_y, start_val } => {
                                let start_y_pos = self.layout.mag_to_y(start_val as f64);
                                let new_db = self
                                    .layout
                                    .y_to_mag(start_y_pos + (mouse_y - start_y))
                                    .clamp(self.params.min_operate_db.value() as f64, 0.0);
                                let norm =
                                    self.params.max_operate_db.preview_normalized(new_db as f32);
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
                                        let norm =
                                            self.params.strength_boost.preview_normalized(new_pct);
                                        self.emit_param_norm(
                                            cx,
                                            self.params.strength_boost.as_ptr(),
                                            norm,
                                        );
                                    }
                                    Polarity::Cut => {
                                        let norm =
                                            self.params.strength_cut.preview_normalized(new_pct);
                                        self.emit_param_norm(
                                            cx,
                                            self.params.strength_cut.as_ptr(),
                                            norm,
                                        );
                                    }
                                }
                                cx.needs_redraw();
                            }
                            DragState::StrengthNode { polarity, id } => {
                                let freq = self.snap_hz(self.layout.x_to_freq(mouse_x));
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
                            DragState::OutputGainKnob { start_y, start_val } => {
                                let delta_y = start_y - mouse_y;
                                let new_val = (start_val + delta_y * 0.2).clamp(-24.0, 24.0);
                                let norm = self.params.output_gain_db.preview_normalized(new_val);
                                self.emit_param_norm(cx, self.params.output_gain_db.as_ptr(), norm);
                                cx.needs_redraw();
                            }
                        }
                    } else {
                        let max_boost = self.params.max_boost_db.value();
                        let max_cut = self.params.max_cut_db.value();
                        let low_x = self.layout.freq_to_x(self.params.low_cut_hz.value() as f64);
                        let high_x = self
                            .layout
                            .freq_to_x(self.params.high_cut_hz.value() as f64);
                        let tilt_x = self
                            .layout
                            .freq_to_x(self.params.tilt_freq_hz.value() as f64);

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

                        self.hover_low_cut = self.layout.hit_cut_line(mouse_x, mouse_y, low_x);
                        self.hover_high_cut = self.layout.hit_cut_line(mouse_x, mouse_y, high_x);
                        self.hover_tilt = (mouse_x - tilt_x).abs() <= HIT_DIST + 4.0
                            && mouse_y >= self.layout.gy
                            && mouse_y <= self.layout.gy + self.layout.gh;
                        self.hover_strength =
                            [Polarity::Boost, Polarity::Cut]
                                .into_iter()
                                .find(|&polarity| {
                                    self.layout.hit_strength_handle(
                                        polarity,
                                        self.strength_pct(polarity),
                                        mouse_x,
                                        mouse_y,
                                    )
                                });
                        let hover_cut = self.hover_low_cut || self.hover_high_cut;
                        self.hover_max_boost = self.hover_node.is_none()
                            && self.hover_strength.is_none()
                            && !hover_cut
                            && self.layout.hit_max_boost(mouse_x, mouse_y, max_boost);
                        self.hover_max_cut = self.hover_node.is_none()
                            && self.hover_strength.is_none()
                            && !hover_cut
                            && self.layout.hit_max_cut(mouse_x, mouse_y, max_cut);

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

                WindowEvent::MouseLeave => {
                    self.hover = None;
                    self.hover_curve = None;
                    self.hover_node = None;
                    cx.needs_redraw();
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

        let bypass_hovered = self
            .idle_hover()
            .is_some_and(|(hx, hy)| Self::inside(hx, hy, BYPASS_BUTTON));
        let bypass_click = self.bypass_anim.step();
        d.bypass_button(
            BYPASS_BUTTON,
            self.params.bypass.value(),
            TEAL,
            bypass_hovered,
            bypass_click,
        );

        let srate = self.shared.sample_rate.load(Ordering::Relaxed) as f64;
        let fft_size = self.params.fft_size.value().size();
        let mut layout = self.layout;
        layout.set_view(
            self.params.max_boost_db.value(),
            self.params.max_cut_db.value(),
            self.params.low_cut_hz.value() as f64,
            self.params.high_cut_hz.value() as f64,
            self.graph_zoomed,
        );

        layout.draw_background(&mut d, fft_size, srate);
        layout.draw_limit_shade(
            &mut d,
            self.params.max_boost_db.value(),
            self.params.max_cut_db.value(),
        );
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

        if let Ok(filters) = self.shared.filter_display.read() {
            layout.draw_filter_gains(&mut d, &filters, fft_size, srate, low_cut, high_cut);
        }

        layout.draw_max_handles(
            &mut d,
            max_boost,
            max_cut,
            low_cut,
            high_cut,
            self.hover_max_boost,
            self.hover_max_cut,
        );

        let boost_nodes = snapshot_nodes(&self.params, Polarity::Boost);
        let cut_nodes = snapshot_nodes(&self.params, Polarity::Cut);
        let selected_boost =
            self.selected
                .and_then(|(p, id)| if p == Polarity::Boost { Some(id) } else { None });
        let selected_cut = self
            .selected
            .and_then(|(p, id)| if p == Polarity::Cut { Some(id) } else { None });

        d.scissor(layout.gx, layout.gy, layout.gw, layout.gh);
        layout.draw_strength_curve(
            &mut d,
            &boost_nodes,
            Polarity::Boost,
            boost_pct,
            max_boost,
            max_cut,
            low_cut,
            high_cut,
            self.hover_curve == Some(Polarity::Boost)
                || self.hover_node.map(|h| h.0) == Some(Polarity::Boost),
            selected_boost,
        );
        layout.draw_strength_curve(
            &mut d,
            &cut_nodes,
            Polarity::Cut,
            cut_pct,
            max_boost,
            max_cut,
            low_cut,
            high_cut,
            self.hover_curve == Some(Polarity::Cut)
                || self.hover_node.map(|h| h.0) == Some(Polarity::Cut),
            selected_cut,
        );
        if self.drag.is_none() && self.hover_node.is_none() {
            if let Some(polarity) = self.hover_curve {
                let pct = match polarity {
                    Polarity::Boost => boost_pct,
                    Polarity::Cut => cut_pct,
                };
                let freq = self.snap_hz(layout.x_to_freq(self.mouse.0));
                let nodes = match polarity {
                    Polarity::Boost => &boost_nodes,
                    Polarity::Cut => &cut_nodes,
                };
                let weight = weight_at(nodes, freq, layout.min_freq, layout.max_freq);
                let preview = StrengthNode::new(0, freq, weight);
                let stroke = match polarity {
                    Polarity::Boost => COLOR_BOOST_HOVER,
                    Polarity::Cut => COLOR_CUT_HOVER,
                };
                layout.draw_strength_node(
                    &mut d, polarity, pct, &preview, stroke, false, low_cut, high_cut, 0.55,
                );
            }
        }
        d.reset_scissor();

        layout.draw_strength_handle(
            &mut d,
            Polarity::Boost,
            boost_pct,
            self.hover_strength == Some(Polarity::Boost)
                || matches!(
                    self.drag,
                    Some(DragState::StrengthOffset {
                        polarity: Polarity::Boost,
                        ..
                    })
                ),
        );
        layout.draw_strength_handle(
            &mut d,
            Polarity::Cut,
            cut_pct,
            self.hover_strength == Some(Polarity::Cut)
                || matches!(
                    self.drag,
                    Some(DragState::StrengthOffset {
                        polarity: Polarity::Cut,
                        ..
                    })
                ),
        );

        let hide_max_grab = self.hover_node.is_some() || self.hover_low_cut || self.hover_high_cut;
        let show_boost_grab = matches!(self.drag, Some(DragState::MaxBoost { .. }))
            || (self.drag.is_none() && !hide_max_grab && self.hover_max_boost);
        let show_cut_grab = matches!(self.drag, Some(DragState::MaxCut { .. }))
            || (self.drag.is_none() && !hide_max_grab && self.hover_max_cut);
        if show_boost_grab {
            layout.draw_line_grab(
                &mut d,
                self.mouse.0,
                layout.db_to_y(max_boost as f64),
                COLOR_BOOST_HOVER,
            );
        }
        if show_cut_grab {
            layout.draw_line_grab(
                &mut d,
                self.mouse.0,
                layout.db_to_y(-(max_cut as f64)),
                COLOR_CUT_HOVER,
            );
        }

        layout.draw_tilt_curve(&mut d, tilt, tilt_freq, srate, low_cut, high_cut);
        layout.draw_tilt_handle(&mut d, tilt, tilt_freq, srate, self.hover_tilt);
        layout.draw_cut_handles(
            &mut d,
            low_cut,
            high_cut,
            self.hover_low_cut,
            self.hover_high_cut,
        );

        let (zoom_in, zoom_out) = self.zoom_button_rects(&layout);
        let can_zoom_in = self.zoom_would_tighten(&layout);
        let cluster_left = zoom_out
            .or(zoom_in)
            .map(|r| r.0)
            .unwrap_or(layout.gx + layout.gw);
        if let Some(r) = zoom_in {
            let hot = can_zoom_in
                && self
                    .idle_hover()
                    .is_some_and(|(hx, hy)| Self::inside(hx, hy, r));
            draw_corner_icon(&mut d, r, true, hot, can_zoom_in);
            if hot {
                d.text_right(cluster_left - 6.0, r.1 + 15.0, "Zoom to area", 10.0, MUTED);
            }
        }
        if let Some(r) = zoom_out {
            let hot = self
                .idle_hover()
                .is_some_and(|(hx, hy)| Self::inside(hx, hy, r));
            draw_corner_icon(&mut d, r, false, hot, true);
            if hot {
                d.text_right(cluster_left - 6.0, r.1 + 15.0, "Reset zoom", 10.0, MUTED);
            }
        }

        let fft_rect = self.fft_button_rect();
        d.button(fft_rect, &format!("FFT: {fft_size}"), false, GOLD);
        let domain_label = match self.params.ms_mode.value() {
            ProcessDomain::LR => "L/R",
            ProcessDomain::MS => "M/S",
        };
        d.button(self.domain_button_rect(), domain_label, false, TEAL);

        for &id in STACKED_SLIDERS {
            let r = Self::slider_rect(id);
            let (label, val_str, color) = self.slider_info(id);
            let n = self.get_slider_norm(id);
            let val_r = slider_value_rect(r);
            if let Some(edit) = &self.edit {
                if edit.target == id {
                    d.control(r, label, "", n, color);
                    d.value_edit(edit, color);
                    continue;
                }
            }
            d.control(r, label, &val_str, n, color);
            if self.edit.is_none() {
                if let Some((hx, hy)) = self.idle_hover() {
                    if Self::inside(hx, hy, val_r) {
                        d.value_underline(val_r, color);
                    }
                }
            }
        }

        // Out Gain Knob
        let knob_r = Self::slider_rect(SliderId::OutputGain);
        let (label, val_str, color) = self.slider_info(SliderId::OutputGain);
        let n = self.get_slider_norm(SliderId::OutputGain);
        let val_r = Self::output_knob_value_rect();
        let bypassed = self.params.bypass.value();

        if let Some(edit) = &self.edit {
            if edit.target == SliderId::OutputGain {
                d.knob_bipolar(knob_r, label, "", n, color, bypassed);
                d.value_edit(edit, color);
            } else {
                d.knob_bipolar(knob_r, label, &val_str, n, color, bypassed);
            }
        } else {
            d.knob_bipolar(knob_r, label, &val_str, n, color, bypassed);
            if self.edit.is_none() {
                if let Some((hx, hy)) = self.idle_hover() {
                    if Self::inside(hx, hy, val_r) {
                        d.value_underline(val_r, color);
                    }
                }
            }
        }

        // Node Sliders (Frequency, Gain, Width, Radius)
        for &id in NODE_SLIDERS {
            let r = Self::slider_rect(id);
            let (label, val_str, color) = self.slider_info(id);
            let n = self.get_slider_norm(id);
            let val_r = slider_value_rect(r);
            if let Some(edit) = &self.edit {
                if edit.target == id {
                    d.control(r, label, "", n, color);
                    d.value_edit(edit, color);
                    continue;
                }
            }
            d.control(r, label, &val_str, n, color);
            if self.edit.is_none() && self.selected.is_some() {
                if let Some((hx, hy)) = self.idle_hover() {
                    if Self::inside(hx, hy, val_r) {
                        d.value_underline(val_r, color);
                    }
                }
            }
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
                hover_strength: None,
                hover_curve: None,
                hover_node: None,
                selected: None,
                mouse: (0.0, 0.0),
                hover: None,
                edit: None,
                bypass_anim: ButtonAnim::new(),
                graph_zoomed: false,
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
