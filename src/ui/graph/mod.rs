use crate::strength::{weight_at, Polarity, StrengthNode, STRENGTH_REST_PX};
use nih_plug_vizia::vizia::vg::Color;
use pleasant_ui::{
    draw::{Draw, TRACE_REST_PX},
    graph::{AxisMapping, FlatteryFrequencyAxis},
    handles::{tag_contains, TagPointer},
    math::{flattery_freq_to_pos, flattery_pos_to_freq},
    spectrum::smooth_bins_f32,
    theme::{rgb, LINE, MUTED},
};

pub const WINDOW_W: f32 = 1040.0;
pub const WINDOW_H: f32 = 660.0;
/// Matches header-to-graph gap (`GRAPH_Y - 70`).
pub const EDGE_PAD: f32 = 56.0;
pub const GRAPH_X: f32 = EDGE_PAD;
pub const GRAPH_Y: f32 = 70.0 + EDGE_PAD;
pub const SIDE_W: f32 = 168.0;
pub const DB_LABEL_GUTTER: f32 = 40.0;
pub const SIDE_GAP: f32 = 8.0;
pub const GRAPH_W: f32 = WINDOW_W - GRAPH_X - DB_LABEL_GUTTER - SIDE_GAP - SIDE_W - EDGE_PAD;
pub const FREQ_LABEL_SPACE: f32 = 26.0;
/// Selected-node strip that replaces the frequency-axis labels.
pub const AXIS_STRIP_H: f32 = 40.0;
pub const NODE_SLIDER_H: f32 = 50.0;
pub const GRAPH_H: f32 = WINDOW_H - GRAPH_Y - FREQ_LABEL_SPACE - NODE_SLIDER_H - EDGE_PAD;
pub const SIDE_X: f32 = GRAPH_X + GRAPH_W + DB_LABEL_GUTTER + SIDE_GAP;
pub const NODE_ROW_GAP: f32 = 16.0;
pub const NODE_ROW_Y: f32 = GRAPH_Y + GRAPH_H + FREQ_LABEL_SPACE + NODE_ROW_GAP;
pub const HIT_DIST: f32 = 12.0;
pub const CURVE_HIT_DIST: f32 = 10.0;
pub const MAX_LINE_WIDTH: f32 = 3.0;
pub const STRENGTH_STEPS: usize = 180;

pub const COLOR_LOW_CUT: Color = rgb(171, 151, 238);
pub const COLOR_LOW_CUT_HOVER: Color = rgb(195, 180, 250);
pub const COLOR_HIGH_CUT: Color = rgb(95, 220, 120);
pub const COLOR_HIGH_CUT_HOVER: Color = rgb(130, 255, 150);
pub const COLOR_GAIN_LINE: Color = Color {
    r: 245.0 / 255.0,
    g: 215.0 / 255.0,
    b: 50.0 / 255.0,
    a: 0.5,
};
pub const COLOR_BOOST: Color = rgb(70, 150, 255);
pub const COLOR_BOOST_HOVER: Color = rgb(130, 185, 255);
pub const COLOR_CUT: Color = rgb(235, 85, 85);
pub const COLOR_CUT_HOVER: Color = rgb(255, 130, 130);

const OUTSIDE_ALPHA: f32 = 0.38;
const WORK_NARROW: f64 = 0.80;
const ZOOM_PAD: f64 = 0.05;
pub const MIN_ZOOM_HZ: f64 = 500.0;
pub const FULL_MIN_FREQ: f64 = 10.0;
pub const FULL_MAX_FREQ: f64 = 22050.0;

pub fn snap_to_bin_edge(freq: f64, bin_hz: f64) -> f64 {
    if !(bin_hz.is_finite() && bin_hz > 0.0) || !freq.is_finite() {
        return freq;
    }
    (freq / bin_hz).round() * bin_hz
}

/// Nearest FFT bin center: `(k + 0.5) * bin_hz`.
pub fn snap_to_bin_center(freq: f64, bin_hz: f64) -> f64 {
    if !(bin_hz.is_finite() && bin_hz > 0.0) || !freq.is_finite() {
        return freq;
    }
    let k = (freq / bin_hz - 0.5).round();
    (k + 0.5) * bin_hz
}

const STRIPE_DARK: Color = rgb(23, 27, 33);
const STRIPE_LIGHT: Color = rgb(34, 40, 48);

/// Even bins are dark; they fade into the light stripe as a band gets narrower
/// than ~4px so sub-pixel FFT bins don't strobe. Wide low-end bands stay
/// fully contrasted.
fn stripe_color(bin_start: usize, band_width_px: f64) -> Color {
    if bin_start % 2 != 0 {
        return STRIPE_LIGHT;
    }
    let t = (band_width_px * 0.25).clamp(0.0, 1.0) as f32;
    let mix = |light: f32, dark: f32| light + (dark - light) * t;
    Color {
        r: mix(STRIPE_LIGHT.r, STRIPE_DARK.r),
        g: mix(STRIPE_LIGHT.g, STRIPE_DARK.g),
        b: mix(STRIPE_LIGHT.b, STRIPE_DARK.b),
        a: 1.0,
    }
}

fn desaturate(c: Color, alpha_scale: f32) -> Color {
    let g = c.r * 0.2126 + c.g * 0.7152 + c.b * 0.0722;
    Color {
        r: g,
        g,
        b: g,
        a: (c.a * alpha_scale).clamp(0.0, 1.0),
    }
}

fn expand_to_min_hz(min_freq: f64, max_freq: f64, min_span: f64) -> (f64, f64) {
    let mut a = min_freq.min(max_freq);
    let mut b = min_freq.max(max_freq);
    if b - a >= min_span {
        return (a, b);
    }
    let mid = (a + b) * 0.5;
    a = mid - min_span * 0.5;
    b = mid + min_span * 0.5;
    if a < FULL_MIN_FREQ {
        a = FULL_MIN_FREQ;
        b = (a + min_span).min(FULL_MAX_FREQ);
    } else if b > FULL_MAX_FREQ {
        b = FULL_MAX_FREQ;
        a = (b - min_span).max(FULL_MIN_FREQ);
    }
    (a, b)
}

fn lerp_x(a: (f32, f32), b: (f32, f32), x: f32) -> (f32, f32) {
    let dx = b.0 - a.0;
    if dx.abs() < 1e-6 {
        (x, a.1)
    } else {
        let t = (x - a.0) / dx;
        (x, a.1 + (b.1 - a.1) * t)
    }
}

fn x_region(x: f32, lo: f32, hi: f32) -> i32 {
    if x < lo {
        0
    } else if x > hi {
        2
    } else {
        1
    }
}

fn split_polyline(points: &[(f32, f32)], x_lo: f32, x_hi: f32) -> [Vec<(f32, f32)>; 3] {
    let mut regions = [Vec::new(), Vec::new(), Vec::new()];
    if points.is_empty() {
        return regions;
    }
    let lo = x_lo.min(x_hi);
    let hi = x_lo.max(x_hi);
    let push = |regions: &mut [Vec<(f32, f32)>; 3], r: i32, p: (f32, f32)| {
        regions[r as usize].push(p);
    };

    let mut prev = points[0];
    let mut pr = x_region(prev.0, lo, hi);
    push(&mut regions, pr, prev);

    for &cur in &points[1..] {
        let cr = x_region(cur.0, lo, hi);
        if cr != pr {
            let cuts: &[f32] = match (pr, cr) {
                (0, 1) | (1, 0) => &[lo],
                (1, 2) | (2, 1) => &[hi],
                (0, 2) => &[lo, hi],
                (2, 0) => &[hi, lo],
                _ => &[],
            };
            for &cx in cuts {
                let pt = lerp_x(prev, cur, cx);
                push(&mut regions, pr, pt);
                pr += if cr > pr { 1 } else { -1 };
                push(&mut regions, pr, pt);
            }
        }
        push(&mut regions, cr, cur);
        prev = cur;
        pr = cr;
    }
    regions
}

fn draw_split_poly(
    d: &mut Draw,
    points: &[(f32, f32)],
    x_lo: f32,
    x_hi: f32,
    color: Color,
    width: f32,
    baseline: Option<f32>,
) {
    let [left, mid, right] = split_polyline(points, x_lo, x_hi);
    let muted = desaturate(color, OUTSIDE_ALPHA);
    let stroke = |d: &mut Draw, pts: &[(f32, f32)], color: Color| {
        if pts.len() < 2 {
            return;
        }
        if let Some(baseline) = baseline {
            d.poly_above(pts, baseline, color, width);
        } else {
            d.poly(pts, color, width);
        }
    };
    stroke(d, &left, muted);
    stroke(d, &mid, color);
    stroke(d, &right, muted);
}

fn draw_split_area(
    d: &mut Draw,
    points: &[(f32, f32)],
    bottom: f32,
    x_lo: f32,
    x_hi: f32,
    color: Color,
) {
    let [left, mid, right] = split_polyline(points, x_lo, x_hi);
    let muted = desaturate(color, OUTSIDE_ALPHA);
    if left.len() >= 2 {
        d.area(&left, bottom, muted);
    }
    if mid.len() >= 2 {
        d.area(&mid, bottom, color);
    }
    if right.len() >= 2 {
        d.area(&right, bottom, muted);
    }
}

#[derive(Clone, Copy, Debug)]
pub struct GraphLayout {
    pub gx: f32,
    pub gy: f32,
    pub gw: f32,
    pub gh: f32,
    pub min_freq: f64,
    pub max_freq: f64,
    pub db_scale: f64,
    pub db_min: f64,
    pub db_max: f64,
}

impl Default for GraphLayout {
    fn default() -> Self {
        Self {
            gx: GRAPH_X,
            gy: GRAPH_Y,
            gw: GRAPH_W,
            gh: GRAPH_H,
            min_freq: FULL_MIN_FREQ,
            max_freq: FULL_MAX_FREQ,
            db_scale: 12.0,
            db_min: -12.0,
            db_max: 12.0,
        }
    }
}

impl GraphLayout {
    pub fn update_db_scale(&mut self, max_boost_db: f32, max_cut_db: f32) {
        let raw = max_boost_db.max(max_cut_db).max(12.0) as f64;
        self.db_scale = (raw / 3.0).ceil() * 3.0;
        self.min_freq = FULL_MIN_FREQ;
        self.max_freq = FULL_MAX_FREQ;
        self.db_min = -self.db_scale;
        self.db_max = self.db_scale;
    }

    pub fn apply_work_zoom(
        &mut self,
        low_hz: f64,
        high_hz: f64,
        max_boost_db: f32,
        max_cut_db: f32,
        bin_hz: f64,
    ) {
        let p0 = flattery_freq_to_pos(low_hz, FULL_MIN_FREQ, FULL_MAX_FREQ);
        let p1 = flattery_freq_to_pos(high_hz, FULL_MIN_FREQ, FULL_MAX_FREQ);
        let span = (p1 - p0).max(0.04);
        let pad = span * ZOOM_PAD;
        let a = (p0.min(p1) - pad).clamp(0.0, 1.0);
        let b = (p0.max(p1) + pad).clamp(0.0, 1.0);
        let mut min_freq = flattery_pos_to_freq(a, FULL_MIN_FREQ, FULL_MAX_FREQ);
        let mut max_freq = flattery_pos_to_freq(b.max(a + 0.02), FULL_MIN_FREQ, FULL_MAX_FREQ);
        (min_freq, max_freq) = expand_to_min_hz(min_freq, max_freq, MIN_ZOOM_HZ);
        if bin_hz.is_finite() && bin_hz > 0.0 {
            min_freq = ((min_freq / bin_hz).floor() * bin_hz).max(FULL_MIN_FREQ);
            max_freq = ((max_freq / bin_hz).ceil() * bin_hz).min(FULL_MAX_FREQ);
            if max_freq - min_freq < MIN_ZOOM_HZ {
                (min_freq, max_freq) = expand_to_min_hz(min_freq, max_freq, MIN_ZOOM_HZ);
            }
        }
        self.min_freq = min_freq;
        self.max_freq = max_freq;

        let span_db = (max_boost_db as f64 + max_cut_db as f64).max(1.0);
        let pad_db = span_db * ZOOM_PAD;
        self.db_max = max_boost_db as f64 + pad_db;
        self.db_min = -(max_cut_db as f64) - pad_db;
        if self.db_max - self.db_min < 1.0 {
            let mid = (self.db_max + self.db_min) * 0.5;
            self.db_max = mid + 0.5;
            self.db_min = mid - 0.5;
        }
    }

    pub fn set_view(
        &mut self,
        max_boost_db: f32,
        max_cut_db: f32,
        _low_hz: f64,
        _high_hz: f64,
        zoomed: bool,
    ) {
        if zoomed {
            return;
        }
        self.update_db_scale(max_boost_db, max_cut_db);
    }

    pub fn work_is_narrow(
        &self,
        low_hz: f64,
        high_hz: f64,
        max_boost_db: f32,
        max_cut_db: f32,
    ) -> bool {
        let p0 = flattery_freq_to_pos(low_hz, FULL_MIN_FREQ, FULL_MAX_FREQ);
        let p1 = flattery_freq_to_pos(high_hz, FULL_MIN_FREQ, FULL_MAX_FREQ);
        let x_frac = (p1 - p0).abs();
        let y_frac = (max_boost_db as f64 + max_cut_db as f64) / (2.0 * self.db_scale).max(1e-6);
        x_frac < WORK_NARROW || y_frac < WORK_NARROW
    }

    pub fn zoom_would_tighten(
        &self,
        low_hz: f64,
        high_hz: f64,
        max_boost_db: f32,
        max_cut_db: f32,
        bin_hz: f64,
    ) -> bool {
        let mut next = *self;
        next.apply_work_zoom(low_hz, high_hz, max_boost_db, max_cut_db, bin_hz);
        let cur_x = self.max_freq - self.min_freq;
        let next_x = next.max_freq - next.min_freq;
        let cur_y = self.db_max - self.db_min;
        let next_y = next.db_max - next.db_min;
        next_x + 1.0 < cur_x || next_y + 0.05 < cur_y
    }

    pub fn center_y(&self) -> f32 {
        self.gy + self.gh * 0.5
    }

    pub fn zero_y(&self) -> f32 {
        self.db_to_y(0.0)
    }

    pub fn in_graph(&self, x: f32, y: f32) -> bool {
        x >= self.gx && x <= self.gx + self.gw && y >= self.gy && y <= self.gy + self.gh
    }

    pub fn freq_to_x(&self, freq: f64) -> f32 {
        let pos = FlatteryFrequencyAxis {
            minimum_hz: self.min_freq,
            maximum_hz: self.max_freq,
        }
        .value_to_position(freq) as f32;
        self.gx + pos * self.gw
    }

    pub fn x_to_freq(&self, x: f32) -> f64 {
        let pos = ((x - self.gx) / self.gw).clamp(0.0, 1.0) as f64;
        FlatteryFrequencyAxis {
            minimum_hz: self.min_freq,
            maximum_hz: self.max_freq,
        }
        .position_to_value(pos)
    }

    pub fn db_to_y(&self, db: f64) -> f32 {
        let span = (self.db_max - self.db_min).max(1e-6);
        let t = (self.db_max - db) / span;
        self.gy + t as f32 * self.gh
    }

    pub fn y_to_db(&self, y: f32) -> f64 {
        let t = ((y - self.gy) / self.gh.max(1.0)) as f64;
        self.db_max - t * (self.db_max - self.db_min)
    }

    pub fn mag_to_y(&self, db: f64) -> f32 {
        let norm = ((db + 120.0) / 120.0).clamp(0.0, 1.0);
        (self.gy + self.gh) - norm as f32 * (self.gh * 0.5)
    }

    pub fn y_to_mag(&self, y: f32) -> f64 {
        let norm = ((self.gy + self.gh - y) / (self.gh * 0.5)).clamp(0.0, 1.0);
        -120.0 + norm as f64 * 120.0
    }

    fn axis_room(&self, polarity: Polarity) -> f32 {
        let zero = self.zero_y();
        let span = match polarity {
            Polarity::Boost => zero - self.gy,
            Polarity::Cut => self.gy + self.gh - zero,
        };
        (span - 6.0).max(STRENGTH_REST_PX + 1.0)
    }

    pub fn strength_offset_px(&self, polarity: Polarity, strength_pct: f32) -> f32 {
        let t = (strength_pct / 200.0).clamp(0.0, 1.0);
        let max_px = self.axis_room(polarity);
        STRENGTH_REST_PX + t * (max_px - STRENGTH_REST_PX)
    }

    pub fn strength_line_y(&self, polarity: Polarity, strength_pct: f32) -> f32 {
        let offset = self.strength_offset_px(polarity, strength_pct);
        match polarity {
            Polarity::Boost => self.zero_y() - offset,
            Polarity::Cut => self.zero_y() + offset,
        }
    }

    pub fn strength_y(&self, polarity: Polarity, strength_pct: f32, weight: f64) -> f32 {
        let zero = self.zero_y();
        let line = self.strength_line_y(polarity, strength_pct);
        zero + (line - zero) * weight as f32
    }

    pub fn y_to_weight(&self, polarity: Polarity, strength_pct: f32, y: f32) -> f64 {
        let zero = self.zero_y();
        let line = self.strength_line_y(polarity, strength_pct);
        let denom = line - zero;
        if denom.abs() < 1.0 {
            return 1.0;
        }
        ((y - zero) / denom).max(0.0) as f64
    }

    pub fn y_to_strength(&self, polarity: Polarity, y: f32) -> f32 {
        let offset = match polarity {
            Polarity::Boost => (self.zero_y() - y).max(STRENGTH_REST_PX),
            Polarity::Cut => (y - self.zero_y()).max(STRENGTH_REST_PX),
        };
        let max_px = self.axis_room(polarity);
        let t =
            ((offset - STRENGTH_REST_PX) / (max_px - STRENGTH_REST_PX).max(1.0)).clamp(0.0, 1.0);
        t * 200.0
    }

    pub fn clip_y(&self, polarity: Polarity, y: f32, max_boost_db: f32, max_cut_db: f32) -> f32 {
        match polarity {
            Polarity::Boost => y.max(self.db_to_y(max_boost_db as f64)),
            Polarity::Cut => y.min(self.db_to_y(-(max_cut_db as f64))),
        }
    }

    pub fn sample_strength_curve(
        &self,
        nodes: &[StrengthNode],
        polarity: Polarity,
        strength_pct: f32,
    ) -> Vec<(f32, f32)> {
        let mut points = Vec::with_capacity(STRENGTH_STEPS + 1);
        for i in 0..=STRENGTH_STEPS {
            let norm = i as f64 / STRENGTH_STEPS as f64;
            let freq = flattery_pos_to_freq(norm, self.min_freq, self.max_freq);
            let w = weight_at(nodes, freq, self.min_freq, self.max_freq);
            let x = self.gx + norm as f32 * self.gw;
            let y = self.strength_y(polarity, strength_pct, w);
            points.push((x, y));
        }
        points
    }

    pub fn curve_distance(
        &self,
        nodes: &[StrengthNode],
        polarity: Polarity,
        strength_pct: f32,
        x: f32,
        y: f32,
    ) -> f32 {
        if !self.in_graph(x, y) {
            return f32::MAX;
        }
        let freq = self.x_to_freq(x);
        let w = weight_at(nodes, freq, self.min_freq, self.max_freq);
        let cy = self.strength_y(polarity, strength_pct, w);
        (y - cy).abs()
    }

    pub fn strength_handle_pos(&self, polarity: Polarity, strength_pct: f32) -> (f32, f32) {
        (self.gx, self.strength_line_y(polarity, strength_pct))
    }

}

mod draw;
mod hit;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cut_tag_is_grabbable_above_graph() {
        let layout = GraphLayout::default();
        let cut_x = layout.freq_to_x(80.0);
        assert!(layout.hit_cut_line(cut_x, layout.gy - 10.0, cut_x));
        assert!(layout.hit_cut_line(cut_x, layout.center_y(), cut_x));
        assert!(!layout.hit_cut_line(cut_x + 40.0, layout.gy - 10.0, cut_x));
    }

    #[test]
    fn split_polyline_keeps_outside_segments() {
        let pts = [(0.0, 1.0), (50.0, 1.0), (100.0, 1.0)];
        let [left, mid, right] = split_polyline(&pts, 20.0, 80.0);
        assert!(left.len() >= 2);
        assert!(mid.len() >= 2);
        assert!(right.len() >= 2);
        assert!(left.iter().all(|p| p.0 <= 20.001));
        assert!(mid.iter().all(|p| (20.0..=80.0).contains(&p.0)
            || (p.0 - 20.0).abs() < 0.01
            || (p.0 - 80.0).abs() < 0.01));
        assert!(right.iter().all(|p| p.0 >= 79.999));
    }

    #[test]
    fn wide_even_stripes_stay_darker_than_odd() {
        let even = stripe_color(0, 20.0);
        let odd = stripe_color(1, 20.0);
        assert!((even.r - STRIPE_DARK.r).abs() < 1e-5);
        assert!((odd.r - STRIPE_LIGHT.r).abs() < 1e-5);
        assert!(odd.r - even.r > 0.01);
    }

    #[test]
    fn narrow_even_stripes_fade_into_light() {
        let even = stripe_color(0, 0.1);
        let odd = stripe_color(1, 0.1);
        assert!((even.r - odd.r).abs() < 0.005);
    }

    #[test]
    #[test]
    fn zero_db_stays_centered_on_full_view() {
        let layout = GraphLayout::default();
        assert!((layout.db_to_y(0.0) - layout.center_y()).abs() < 0.01);
        assert!((layout.db_to_y(layout.db_scale) - layout.gy).abs() < 0.01);
        assert!((layout.db_to_y(-layout.db_scale) - (layout.gy + layout.gh)).abs() < 0.01);
    }

    #[test]
    fn zoom_pads_work_range_by_five_percent() {
        let mut layout = GraphLayout::default();
        layout.update_db_scale(6.0, 6.0);
        layout.apply_work_zoom(200.0, 2000.0, 6.0, 6.0, 0.0);
        let p0 = flattery_freq_to_pos(200.0, FULL_MIN_FREQ, FULL_MAX_FREQ);
        let p1 = flattery_freq_to_pos(2000.0, FULL_MIN_FREQ, FULL_MAX_FREQ);
        let span = p1 - p0;
        let view0 = flattery_freq_to_pos(layout.min_freq, FULL_MIN_FREQ, FULL_MAX_FREQ);
        let view1 = flattery_freq_to_pos(layout.max_freq, FULL_MIN_FREQ, FULL_MAX_FREQ);
        assert!((view0 - (p0 - span * 0.05)).abs() < 0.01);
        assert!((view1 - (p1 + span * 0.05)).abs() < 0.01);
        assert!((layout.db_max - 6.6).abs() < 1e-6);
        assert!((layout.db_min + 6.6).abs() < 1e-6);
    }

    #[test]
    fn narrow_work_range_offers_zoom() {
        let mut layout = GraphLayout::default();
        layout.update_db_scale(12.0, 12.0);
        assert!(!layout.work_is_narrow(20.0, 20000.0, 12.0, 12.0));
        assert!(layout.work_is_narrow(200.0, 2000.0, 12.0, 12.0));
        assert!(layout.work_is_narrow(20.0, 20000.0, 3.0, 12.0));
    }

    #[test]
    fn zoomed_zero_follows_asymmetric_limits() {
        let mut layout = GraphLayout::default();
        layout.update_db_scale(6.0, 24.0);
        layout.apply_work_zoom(20.0, 20000.0, 6.0, 24.0, 0.0);
        assert!(layout.db_to_y(0.0) < layout.center_y());
        assert!(layout.db_to_y(layout.db_max) <= layout.gy + 0.5);
        assert!(layout.db_to_y(layout.db_min) >= layout.gy + layout.gh - 0.5);
    }

    #[test]
    fn zoomed_view_stays_frozen_when_work_range_moves() {
        let mut layout = GraphLayout::default();
        layout.apply_work_zoom(200.0, 2000.0, 6.0, 6.0, 0.0);
        let min_freq = layout.min_freq;
        let max_freq = layout.max_freq;
        let db_min = layout.db_min;
        let db_max = layout.db_max;
        layout.set_view(12.0, 24.0, 400.0, 800.0, true);
        assert_eq!(layout.min_freq, min_freq);
        assert_eq!(layout.max_freq, max_freq);
        assert_eq!(layout.db_min, db_min);
        assert_eq!(layout.db_max, db_max);
        layout.apply_work_zoom(400.0, 800.0, 12.0, 24.0, 0.0);
        assert!(layout.min_freq > min_freq);
        assert!(layout.max_freq < max_freq);
        layout.set_view(12.0, 12.0, 400.0, 800.0, false);
        assert_eq!(layout.min_freq, FULL_MIN_FREQ);
        assert_eq!(layout.max_freq, FULL_MAX_FREQ);
    }

    #[test]
    fn zoom_never_narrower_than_500hz() {
        let mut layout = GraphLayout::default();
        layout.apply_work_zoom(1000.0, 1100.0, 6.0, 6.0, 0.0);
        assert!(layout.max_freq - layout.min_freq >= MIN_ZOOM_HZ - 1e-6);
    }

    #[test]
    fn zoom_snaps_to_bin_edges() {
        let bin_hz = 44100.0 / 512.0;
        let mut layout = GraphLayout::default();
        layout.apply_work_zoom(200.0, 2000.0, 6.0, 6.0, bin_hz);
        let min_bins = layout.min_freq / bin_hz;
        let max_bins = layout.max_freq / bin_hz;
        assert!((min_bins - min_bins.round()).abs() < 1e-6 || layout.min_freq == FULL_MIN_FREQ);
        assert!((max_bins - max_bins.round()).abs() < 1e-6 || layout.max_freq == FULL_MAX_FREQ);
        assert!(layout.max_freq - layout.min_freq >= MIN_ZOOM_HZ - 1e-6);
    }

    #[test]
    fn zoom_in_disabled_when_already_at_work_view() {
        let mut layout = GraphLayout::default();
        layout.update_db_scale(12.0, 12.0);
        assert!(layout.zoom_would_tighten(200.0, 2000.0, 6.0, 6.0, 0.0));
        layout.apply_work_zoom(200.0, 2000.0, 6.0, 6.0, 0.0);
        assert!(!layout.zoom_would_tighten(200.0, 2000.0, 6.0, 6.0, 0.0));
        layout.apply_work_zoom(1000.0, 1100.0, 6.0, 6.0, 0.0);
        assert!(!layout.zoom_would_tighten(1000.0, 1050.0, 6.0, 6.0, 0.0));
    }

    #[test]
    fn snap_to_bin_edge_rounds_to_nearest_multiple() {
        let bin_hz = 100.0;
        assert!((snap_to_bin_edge(149.0, bin_hz) - 100.0).abs() < 1e-9);
        assert!((snap_to_bin_edge(150.0, bin_hz) - 200.0).abs() < 1e-9);
        assert_eq!(snap_to_bin_edge(440.0, 0.0), 440.0);
    }

    #[test]
    fn snap_to_bin_center_uses_half_bin() {
        let bin_hz = 100.0;
        assert!((snap_to_bin_center(149.0, bin_hz) - 150.0).abs() < 1e-9);
        assert!((snap_to_bin_center(150.0, bin_hz) - 150.0).abs() < 1e-9);
        assert!((snap_to_bin_center(200.0, bin_hz) - 250.0).abs() < 1e-9);
        assert_eq!(snap_to_bin_center(440.0, 0.0), 440.0);
    }

    fn strength_tag_is_grabbable_left_of_graph() {
        let layout = GraphLayout::default();
        assert!(layout.hit_strength_handle(
            Polarity::Boost,
            35.0,
            layout.gx - 12.0,
            layout.strength_line_y(Polarity::Boost, 35.0)
        ));
        assert!(!layout.hit_strength_handle(
            Polarity::Boost,
            35.0,
            layout.gx + 40.0,
            layout.strength_line_y(Polarity::Boost, 35.0)
        ));
    }
}
