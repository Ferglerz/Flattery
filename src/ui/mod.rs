mod adapter;
mod controls;
mod edit;
mod events;
mod nodes;
pub mod graph;
mod render;

use crate::{
    dsp::Shared,
    params::{FftSize, FlatteryParams, ProcessDomain},
    strength::{
        default_node_radius, next_node_id, norm_to_q, q_to_norm, q_to_width_pct, weight_at,
        width_pct_to_q, Polarity, StrengthNode, DEFAULT_NODE_Q,
    },
    ui::graph::{
        snap_to_bin_center, GraphLayout, AXIS_STRIP_H, COLOR_BOOST, COLOR_BOOST_HOVER, COLOR_CUT,
        COLOR_CUT_HOVER,
        CURVE_HIT_DIST, EDGE_PAD, GRAPH_H, GRAPH_W, GRAPH_X, GRAPH_Y, NODE_ROW_GAP, SIDE_W, SIDE_X,
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
const THEME_BUTTON: (f32, f32, f32, f32) = (WINDOW_W - EDGE_PAD - 72.0, 22.0, 72.0, 26.0);
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
    hover_max_boost: bool,
    hover_max_cut: bool,
    hover_strength: Option<Polarity>,
    hover_curve: Option<Polarity>,
    hover_node: Option<(Polarity, u64)>,
    selected: Option<(Polarity, u64)>,
    mouse: (f32, f32),
    hover: Option<(f32, f32)>,
    edit: Option<ValueEdit<SliderId>>,
    graph_zoomed: bool,
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


impl View for FlatteryView {
    fn element(&self) -> Option<&'static str> {
        Some("flattery-view")
    }

    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        self.handle_event(cx, event);
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &mut Canvas) {
        self.render(cx, canvas);
    }
}

pub fn create(params: Arc<FlatteryParams>, shared: Arc<Shared>) -> Option<Box<dyn Editor>> {
    adapter::create(params, shared)
}
