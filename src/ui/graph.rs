use crate::dsp::tilt::calculate_tilt_multiplier_scaled;
use crate::strength::{weight_at, Polarity, StrengthNode, STRENGTH_REST_PX};
use nih_plug_vizia::vizia::vg::Color;
use pleasant_ui::{
    draw::Draw,
    handles::{tag_contains, TagPointer},
    math::{flattery_freq_to_pos, flattery_pos_to_freq, linear_to_db},
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
pub const COLOR_TILT: Color = rgb(240, 80, 150);
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

const STRIPE_DARK: Color = rgb(23, 27, 33);
const STRIPE_LIGHT: Color = rgb(27, 32, 39);

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
) {
    let [left, mid, right] = split_polyline(points, x_lo, x_hi);
    let muted = desaturate(color, OUTSIDE_ALPHA);
    if left.len() >= 2 {
        d.poly(&left, muted, width);
    }
    if mid.len() >= 2 {
        d.poly(&mid, color, width);
    }
    if right.len() >= 2 {
        d.poly(&right, muted, width);
    }
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
}

impl Default for GraphLayout {
    fn default() -> Self {
        Self {
            gx: GRAPH_X,
            gy: GRAPH_Y,
            gw: GRAPH_W,
            gh: GRAPH_H,
            min_freq: 10.0,
            max_freq: 22050.0,
            db_scale: 12.0,
        }
    }
}

impl GraphLayout {
    pub fn update_db_scale(&mut self, max_boost_db: f32, max_cut_db: f32) {
        let raw = max_boost_db.max(max_cut_db).max(12.0) as f64;
        self.db_scale = (raw / 3.0).ceil() * 3.0;
    }

    pub fn center_y(&self) -> f32 {
        self.gy + self.gh * 0.5
    }

    pub fn in_graph(&self, x: f32, y: f32) -> bool {
        x >= self.gx && x <= self.gx + self.gw && y >= self.gy && y <= self.gy + self.gh
    }

    pub fn freq_to_x(&self, freq: f64) -> f32 {
        let pos = flattery_freq_to_pos(freq, self.min_freq, self.max_freq) as f32;
        self.gx + pos * self.gw
    }

    pub fn x_to_freq(&self, x: f32) -> f64 {
        let pos = ((x - self.gx) / self.gw).clamp(0.0, 1.0) as f64;
        flattery_pos_to_freq(pos, self.min_freq, self.max_freq)
    }

    pub fn db_to_y(&self, db: f64) -> f32 {
        let center = self.center_y();
        center - (db / self.db_scale) as f32 * (self.gh * 0.5)
    }

    pub fn y_to_db(&self, y: f32) -> f64 {
        let center = self.center_y();
        ((center - y) / (self.gh * 0.5)) as f64 * self.db_scale
    }

    pub fn mag_to_y(&self, db: f64) -> f32 {
        let norm = ((db + 120.0) / 120.0).clamp(0.0, 1.0);
        (self.gy + self.gh) - norm as f32 * (self.gh * 0.5)
    }

    pub fn y_to_mag(&self, y: f32) -> f64 {
        let norm = ((self.gy + self.gh - y) / (self.gh * 0.5)).clamp(0.0, 1.0);
        -120.0 + norm as f64 * 120.0
    }

    pub fn strength_offset_px(&self, strength_pct: f32) -> f32 {
        let t = (strength_pct / 200.0).clamp(0.0, 1.0);
        let max_px = self.gh * 0.5 - 6.0;
        STRENGTH_REST_PX + t * (max_px - STRENGTH_REST_PX)
    }

    pub fn strength_line_y(&self, polarity: Polarity, strength_pct: f32) -> f32 {
        let offset = self.strength_offset_px(strength_pct);
        match polarity {
            Polarity::Boost => self.center_y() - offset,
            Polarity::Cut => self.center_y() + offset,
        }
    }

    pub fn strength_y(&self, polarity: Polarity, strength_pct: f32, weight: f64) -> f32 {
        let center = self.center_y();
        let line = self.strength_line_y(polarity, strength_pct);
        center + (line - center) * weight as f32
    }

    pub fn y_to_weight(&self, polarity: Polarity, strength_pct: f32, y: f32) -> f64 {
        let center = self.center_y();
        let line = self.strength_line_y(polarity, strength_pct);
        let denom = line - center;
        if denom.abs() < 1.0 {
            return 1.0;
        }
        ((y - center) / denom).max(0.0) as f64
    }

    pub fn y_to_strength(&self, polarity: Polarity, y: f32) -> f32 {
        let offset = match polarity {
            Polarity::Boost => (self.center_y() - y).max(STRENGTH_REST_PX),
            Polarity::Cut => (y - self.center_y()).max(STRENGTH_REST_PX),
        };
        let max_px = self.gh * 0.5 - 6.0;
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

    pub fn draw_background(&self, d: &mut Draw, fft_size: usize, srate: f64) {
        let bin_hz = srate / fft_size as f64;
        let bin_hz_inv = 1.0 / bin_hz;
        let right = self.gx + self.gw;
        let columns = self.gw.ceil() as usize;

        for i in 0..columns {
            let x0 = self.gx + i as f32;
            let x1 = (self.gx + (i + 1) as f32).min(right);
            let w = x1 - x0;
            if w <= 0.0 {
                continue;
            }

            let norm = (i as f32 / self.gw).clamp(0.0, 1.0) as f64;
            let pixel_bin = flattery_pos_to_freq(norm, self.min_freq, self.max_freq) * bin_hz_inv;
            let bin_start = pixel_bin.max(0.0) as usize;

            let next_norm = ((i + 1) as f32 / self.gw).min(1.0) as f64;
            let next_bin =
                flattery_pos_to_freq(next_norm, self.min_freq, self.max_freq) * bin_hz_inv;
            let bins_per_pixel = (next_bin - pixel_bin).max(0.0);
            let band_width_px = if bins_per_pixel > 0.0 {
                1.0 / bins_per_pixel
            } else {
                9999.0
            };

            d.rect(
                x0,
                self.gy,
                w,
                self.gh,
                stripe_color(bin_start, band_width_px),
            );
        }

        d.outline((self.gx, self.gy, self.gw, self.gh), LINE);
    }

    pub fn draw_grid_and_labels(&self, d: &mut Draw) {
        let scale = self.db_scale.round() as i32;
        let step = if scale <= 12 {
            3
        } else if scale <= 24 {
            6
        } else {
            12
        };

        let mut db = -scale;
        while db <= scale {
            let y = self.db_to_y(db as f64);
            d.line(
                self.gx,
                y,
                self.gx + self.gw,
                y,
                if db == 0 { rgb(75, 82, 92) } else { LINE },
                1.0,
            );
            let label = if db > 0 {
                format!("+{db}")
            } else {
                format!("{db}")
            };
            d.text(self.gx + self.gw + 10.0, y + 4.0, &label, 10.5, MUTED);
            db += step;
        }

        for &(freq, label) in &[
            (20.0, ""),
            (50.0, ""),
            (100.0, ""),
            (200.0, "200"),
            (500.0, "500"),
            (1000.0, "1k"),
            (2000.0, "2k"),
            (5000.0, "5k"),
            (10000.0, "10k"),
            (20000.0, ""),
        ] {
            if freq < self.max_freq {
                let x = self.freq_to_x(freq);
                d.line(x, self.gy, x, self.gy + self.gh, LINE, 1.0);
                if !label.is_empty() {
                    d.text(x - 9.0, self.gy + self.gh + 18.0, label, 10.5, MUTED);
                }
            }
        }
    }

    pub fn draw_operate_window(&self, d: &mut Draw, op_min_db: f64, op_max_db: f64) {
        let bottom = self.gy + self.gh;
        let center = self.center_y();
        let min_y = self.mag_to_y(op_min_db).min(bottom);
        let max_y = self.mag_to_y(op_max_db).max(center);

        let dim = Color::rgba(8, 10, 14, 90);
        if min_y < bottom - 1.0 {
            d.rect(self.gx, min_y, self.gw, bottom - min_y, dim);
        }
        if max_y > center + 1.0 {
            d.rect(self.gx, center, self.gw, max_y - center, dim);
        }

        d.line(
            self.gx,
            min_y,
            self.gx + 28.0,
            min_y,
            Color::rgba(160, 170, 185, 180),
            1.5,
        );
        d.line(
            self.gx,
            max_y.min(center),
            self.gx + 28.0,
            max_y.min(center),
            Color::rgba(160, 170, 185, 180),
            1.5,
        );
    }

    pub fn draw_spectrum(
        &self,
        d: &mut Draw,
        mags_db: &[f32],
        fft_size: usize,
        srate: f64,
        low_cut_hz: f64,
        high_cut_hz: f64,
    ) {
        if mags_db.is_empty() {
            return;
        }
        let mut smoothed = vec![0.0_f32; mags_db.len()];
        smooth_bins_f32(mags_db, &mut smoothed);

        let bin_hz = srate / fft_size as f64;
        let mut points = Vec::with_capacity(smoothed.len());

        for (k, &db) in smoothed.iter().enumerate() {
            let freq = (k as f64 + 0.5) * bin_hz;
            if freq > self.max_freq {
                break;
            }
            let x = self.freq_to_x(freq);
            let y = self.mag_to_y(db as f64);
            points.push((x, y));
        }

        if points.is_empty() {
            return;
        }

        let x_lo = self.freq_to_x(low_cut_hz);
        let x_hi = self.freq_to_x(high_cut_hz);
        draw_split_area(
            d,
            &points,
            self.gy + self.gh,
            x_lo,
            x_hi,
            Color::rgba(65, 140, 240, 45),
        );
        draw_split_poly(d, &points, x_lo, x_hi, Color::rgba(90, 160, 255, 160), 1.2);
    }

    pub fn draw_filter_gains(
        &self,
        d: &mut Draw,
        filters: &[(f32, f32)],
        fft_size: usize,
        srate: f64,
        low_cut_hz: f64,
        high_cut_hz: f64,
    ) {
        if filters.is_empty() || fft_size == 0 || srate <= 0.0 {
            return;
        }

        let bin_hz = srate / fft_size as f64;
        let y_center = self.center_y();

        for &(center_hz, gain_db) in filters {
            let freq = center_hz as f64;
            if freq < self.min_freq || freq > self.max_freq {
                continue;
            }

            let k = (freq / bin_hz).floor() as usize;
            let bin_start_hz = k as f64 * bin_hz;
            let bin_end_hz = (k + 1) as f64 * bin_hz;

            let x_left = self.freq_to_x(bin_start_hz.max(self.min_freq));
            let x_right = self.freq_to_x(bin_end_hz.min(self.max_freq));

            let bw = (x_right - x_left).max(1.0);
            let bar_w = if bw > 2.0 { bw - 1.0 } else { bw };
            let bar_x = if bw > 2.0 { x_left + 0.5 } else { x_left };

            let y_gain = self
                .db_to_y(gain_db as f64)
                .clamp(self.gy, self.gy + self.gh);
            let (bar_y, bar_h) = if y_gain < y_center {
                (y_gain, y_center - y_gain)
            } else {
                (y_center, y_gain - y_center)
            };

            if bar_h >= 0.5 {
                let color = if freq < low_cut_hz || freq > high_cut_hz {
                    desaturate(COLOR_GAIN_LINE, OUTSIDE_ALPHA)
                } else {
                    COLOR_GAIN_LINE
                };
                d.rounded_rect(bar_x, bar_y, bar_w, bar_h, 0.0, color);
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw_strength_curve(
        &self,
        d: &mut Draw,
        nodes: &[StrengthNode],
        polarity: Polarity,
        strength_pct: f32,
        max_boost_db: f32,
        max_cut_db: f32,
        low_cut_hz: f64,
        high_cut_hz: f64,
        hover: bool,
        selected: Option<u64>,
    ) {
        let (color, hover_color, fill) = match polarity {
            Polarity::Boost => (
                COLOR_BOOST,
                COLOR_BOOST_HOVER,
                Color::rgba(70, 150, 255, 32),
            ),
            Polarity::Cut => (COLOR_CUT, COLOR_CUT_HOVER, Color::rgba(235, 85, 85, 32)),
        };
        let stroke = if hover { hover_color } else { color };
        let raw = self.sample_strength_curve(nodes, polarity, strength_pct);
        let clipped: Vec<(f32, f32)> = raw
            .iter()
            .map(|&(x, y)| (x, self.clip_y(polarity, y, max_boost_db, max_cut_db)))
            .collect();
        let x_lo = self.freq_to_x(low_cut_hz);
        let x_hi = self.freq_to_x(high_cut_hz);

        if clipped.len() >= 2 {
            draw_split_area(d, &clipped, self.center_y(), x_lo, x_hi, fill);
            let exceeded = raw
                .iter()
                .zip(clipped.iter())
                .any(|(a, b)| (a.1 - b.1).abs() > 0.5);
            if exceeded {
                let ghost = match polarity {
                    Polarity::Boost => Color::rgba(70, 150, 255, 70),
                    Polarity::Cut => Color::rgba(235, 85, 85, 70),
                };
                draw_split_poly(d, &raw, x_lo, x_hi, ghost, 1.0);
            }
            draw_split_poly(
                d,
                &clipped,
                x_lo,
                x_hi,
                stroke,
                if hover { 2.4 } else { 1.8 },
            );
        }

        for node in nodes {
            let x = self.freq_to_x(node.freq);
            let y = self.strength_y(polarity, strength_pct, node.weight);
            let outside = node.freq < low_cut_hz || node.freq > high_cut_hz;
            let node_stroke = if outside {
                desaturate(stroke, OUTSIDE_ALPHA + 0.2)
            } else {
                stroke
            };
            let base_r = if selected == Some(node.id) { 7.0 } else { 5.5 };

            // Every 2 bins larger, add an outer ring with 85% opacity scaling per tier
            let rings = node.radius / 2;
            for i in (1..=rings).rev() {
                let ring_r = base_r + i as f32 * 3.5;
                let ring_alpha = (node_stroke.a * 0.85_f32.powi(i as i32)).clamp(0.0, 1.0);
                d.circle(
                    x,
                    y,
                    ring_r,
                    Color {
                        a: ring_alpha,
                        ..node_stroke
                    },
                    true,
                );
            }

            d.circle(x, y, base_r, node_stroke, true);
            d.circle(
                x,
                y,
                base_r * 0.45,
                if selected == Some(node.id) {
                    rgb(255, 255, 255)
                } else {
                    rgb(230, 230, 235)
                },
                true,
            );
        }
    }

    pub fn strength_handle_pos(&self, polarity: Polarity, strength_pct: f32) -> (f32, f32) {
        (self.gx, self.strength_line_y(polarity, strength_pct))
    }

    pub fn hit_strength_handle(
        &self,
        polarity: Polarity,
        strength_pct: f32,
        x: f32,
        y: f32,
    ) -> bool {
        let (hx, hy) = self.strength_handle_pos(polarity, strength_pct);
        tag_contains(hx, hy, TagPointer::Right, x, y)
    }

    pub fn draw_strength_handle(
        &self,
        d: &mut Draw,
        polarity: Polarity,
        strength_pct: f32,
        hover: bool,
    ) {
        let (x, y) = self.strength_handle_pos(polarity, strength_pct);
        let color = match (polarity, hover) {
            (Polarity::Boost, true) => COLOR_BOOST_HOVER,
            (Polarity::Boost, false) => COLOR_BOOST,
            (Polarity::Cut, true) => COLOR_CUT_HOVER,
            (Polarity::Cut, false) => COLOR_CUT,
        };
        d.tag_handle(x, y, TagPointer::Right, color);
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw_max_handles(
        &self,
        d: &mut Draw,
        max_boost_db: f32,
        max_cut_db: f32,
        low_cut_hz: f64,
        high_cut_hz: f64,
        hover_boost: bool,
        hover_cut: bool,
    ) {
        let boost_y = self.db_to_y(max_boost_db as f64);
        let cut_y = self.db_to_y(-(max_cut_db as f64));
        let boost_c = if hover_boost {
            COLOR_BOOST_HOVER
        } else {
            COLOR_BOOST
        };
        let cut_c = if hover_cut {
            COLOR_CUT_HOVER
        } else {
            COLOR_CUT
        };
        let x_lo = self.freq_to_x(low_cut_hz);
        let x_hi = self.freq_to_x(high_cut_hz);
        let right = self.gx + self.gw;

        if boost_y > self.gy + 1.0 {
            let fill = Color::rgba(70, 150, 255, 16);
            draw_split_area(
                d,
                &[(self.gx, self.gy), (right, self.gy)],
                boost_y,
                x_lo,
                x_hi,
                fill,
            );
        }
        let bottom = self.gy + self.gh;
        if cut_y < bottom - 1.0 {
            let fill = Color::rgba(235, 85, 85, 16);
            draw_split_area(
                d,
                &[(self.gx, cut_y), (right, cut_y)],
                bottom,
                x_lo,
                x_hi,
                fill,
            );
        }

        let boost_w = if hover_boost {
            MAX_LINE_WIDTH + 0.8
        } else {
            MAX_LINE_WIDTH
        };
        let cut_w = if hover_cut {
            MAX_LINE_WIDTH + 0.8
        } else {
            MAX_LINE_WIDTH
        };
        draw_split_poly(
            d,
            &[(self.gx, boost_y), (right, boost_y)],
            x_lo,
            x_hi,
            boost_c,
            boost_w,
        );
        draw_split_poly(
            d,
            &[(self.gx, cut_y), (right, cut_y)],
            x_lo,
            x_hi,
            cut_c,
            cut_w,
        );
    }

    pub fn hit_max_line(&self, x: f32, y: f32, line_y: f32) -> bool {
        x >= self.gx
            && x <= self.gx + self.gw
            && (y - line_y).abs() <= HIT_DIST
            && y >= self.gy
            && y <= self.gy + self.gh
    }

    pub fn hit_max_boost(&self, x: f32, y: f32, max_boost_db: f32) -> bool {
        self.hit_max_line(x, y, self.db_to_y(max_boost_db as f64))
    }

    pub fn hit_max_cut(&self, x: f32, y: f32, max_cut_db: f32) -> bool {
        self.hit_max_line(x, y, self.db_to_y(-(max_cut_db as f64)))
    }

    pub fn draw_line_grab(&self, d: &mut Draw, mouse_x: f32, line_y: f32, color: Color) {
        let x = mouse_x.clamp(self.gx + 12.0, self.gx + self.gw - 12.0);
        d.tag_handle(x, line_y, TagPointer::None, color);
    }

    pub fn hit_op_min(&self, x: f32, y: f32, op_min_db: f32) -> bool {
        let hy = self.mag_to_y(op_min_db as f64);
        x >= self.gx && x <= self.gx + 32.0 && (y - hy).abs() <= HIT_DIST
    }

    pub fn hit_op_max(&self, x: f32, y: f32, op_max_db: f32) -> bool {
        let hy = self.mag_to_y(op_max_db as f64);
        x >= self.gx && x <= self.gx + 32.0 && (y - hy).abs() <= HIT_DIST
    }

    #[allow(clippy::too_many_arguments)]
    pub fn hit_node(
        &self,
        nodes: &[StrengthNode],
        polarity: Polarity,
        strength_pct: f32,
        _max_boost_db: f32,
        _max_cut_db: f32,
        x: f32,
        y: f32,
    ) -> Option<u64> {
        let mut best = None;
        let mut best_d = crate::strength::NODE_HIT_R;
        for node in nodes {
            let nx = self.freq_to_x(node.freq);
            let ny = self.strength_y(polarity, strength_pct, node.weight);
            let d = ((x - nx).powi(2) + (y - ny).powi(2)).sqrt();
            let rings = (node.radius / 2) as f32;
            let hit_r = crate::strength::NODE_HIT_R.max(7.0 + rings * 3.5);
            if d <= hit_r && d <= best_d {
                best_d = d;
                best = Some(node.id);
            }
        }
        best
    }

    pub fn draw_tilt_curve(
        &self,
        d: &mut Draw,
        tilt_amount: f64,
        tilt_freq_hz: f64,
        srate: f64,
        low_cut_hz: f64,
        high_cut_hz: f64,
    ) {
        if tilt_amount.abs() <= 0.001 {
            return;
        }

        let steps = 180;
        let mut points = Vec::with_capacity(steps + 1);

        for i in 0..=steps {
            let norm = i as f64 / steps as f64;
            let freq = flattery_pos_to_freq(norm, self.min_freq, self.max_freq);
            let mult =
                calculate_tilt_multiplier_scaled(freq, tilt_freq_hz, tilt_amount * 0.01, srate);
            let db = linear_to_db(mult);
            let y = self.center_y() - (db / 12.0) as f32 * (self.gh * 0.25);
            let x = self.gx + norm as f32 * self.gw;
            points.push((x, y));
        }

        draw_split_poly(
            d,
            &points,
            self.freq_to_x(low_cut_hz),
            self.freq_to_x(high_cut_hz),
            COLOR_TILT,
            1.8,
        );
    }

    pub fn draw_tilt_handle(
        &self,
        d: &mut Draw,
        tilt_amount: f64,
        tilt_freq_hz: f64,
        srate: f64,
        hover: bool,
    ) {
        let x = self.freq_to_x(tilt_freq_hz);
        let mult =
            calculate_tilt_multiplier_scaled(tilt_freq_hz, tilt_freq_hz, tilt_amount * 0.01, srate);
        let db = linear_to_db(mult);
        let y = self.center_y() - (db / 12.0) as f32 * (self.gh * 0.25);

        d.circle(x, y, 7.0, rgb(160, 45, 175), true);
        d.circle(
            x,
            y,
            3.5,
            if hover {
                rgb(255, 255, 255)
            } else {
                rgb(220, 220, 220)
            },
            true,
        );
    }

    pub fn draw_cut_handles(
        &self,
        d: &mut Draw,
        low_cut_hz: f64,
        high_cut_hz: f64,
        hover_low: bool,
        hover_high: bool,
    ) {
        let low_x = self.freq_to_x(low_cut_hz);
        let high_x = self.freq_to_x(high_cut_hz);

        let color_low = if hover_low {
            COLOR_LOW_CUT_HOVER
        } else {
            COLOR_LOW_CUT
        };
        let color_high = if hover_high {
            COLOR_HIGH_CUT_HOVER
        } else {
            COLOR_HIGH_CUT
        };

        d.line(low_x, self.gy, low_x, self.gy + self.gh, color_low, 2.5);
        d.tag_handle(low_x, self.gy, TagPointer::Down, color_low);

        d.line(high_x, self.gy, high_x, self.gy + self.gh, color_high, 2.5);
        d.tag_handle(high_x, self.gy, TagPointer::Down, color_high);
    }

    pub fn hit_cut_line(&self, x: f32, y: f32, cut_x: f32) -> bool {
        let line_hit = (x - cut_x).abs() <= HIT_DIST && y >= self.gy && y <= self.gy + self.gh;
        line_hit || tag_contains(cut_x, self.gy, TagPointer::Down, x, y)
    }
}

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
