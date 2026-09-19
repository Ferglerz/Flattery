use crate::dsp::tilt::calculate_tilt_multiplier_scaled;
use crate::strength::{weight_at, Polarity, StrengthNode, STRENGTH_REST_PX};
use nih_plug_vizia::vizia::vg::Color;
use pleasant_ui::{
    draw::Draw,
    math::{flattery_freq_to_pos, flattery_pos_to_freq, linear_to_db},
    theme::{rgb, LINE, MUTED},
};

pub const WINDOW_W: f32 = 1040.0;
pub const WINDOW_H: f32 = 600.0;
pub const GRAPH_X: f32 = 60.0;
pub const GRAPH_Y: f32 = 88.0;
pub const GRAPH_W: f32 = 920.0;
pub const GRAPH_H: f32 = 400.0;
pub const FOOTER_Y: f32 = 528.0;
pub const TRIANGLE_SIZE: f32 = 9.0;
pub const HIT_DIST: f32 = 12.0;
pub const CURVE_HIT_DIST: f32 = 10.0;
pub const STRENGTH_STEPS: usize = 180;

pub const COLOR_LOW_CUT: Color = rgb(235, 95, 95);
pub const COLOR_LOW_CUT_HOVER: Color = rgb(255, 130, 130);
pub const COLOR_HIGH_CUT: Color = rgb(95, 220, 120);
pub const COLOR_HIGH_CUT_HOVER: Color = rgb(130, 255, 150);
pub const COLOR_TILT: Color = rgb(240, 80, 150);
pub const COLOR_GAIN_LINE: Color = rgb(245, 215, 50);
pub const COLOR_BOOST: Color = rgb(70, 150, 255);
pub const COLOR_BOOST_HOVER: Color = rgb(130, 185, 255);
pub const COLOR_CUT: Color = rgb(235, 85, 85);
pub const COLOR_CUT_HOVER: Color = rgb(255, 130, 130);

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
        ((y - center) / denom).clamp(0.0, 1.0) as f64
    }

    pub fn y_to_strength(&self, polarity: Polarity, y: f32) -> f32 {
        let offset = match polarity {
            Polarity::Boost => (self.center_y() - y).max(STRENGTH_REST_PX),
            Polarity::Cut => (y - self.center_y()).max(STRENGTH_REST_PX),
        };
        let max_px = self.gh * 0.5 - 6.0;
        let t = ((offset - STRENGTH_REST_PX) / (max_px - STRENGTH_REST_PX).max(1.0)).clamp(0.0, 1.0);
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
        let pos_bins = fft_size / 2;
        let bin_hz = srate / fft_size as f64;
        let c_dark = rgb(23, 27, 33);
        let c_light = rgb(27, 32, 39);

        let mut prev_x = self.gx;
        for k in 0..pos_bins {
            let next_freq = (k + 1) as f64 * bin_hz;
            let next_x = if next_freq >= self.max_freq {
                self.gx + self.gw
            } else {
                self.freq_to_x(next_freq).min(self.gx + self.gw)
            };

            let bw = next_x - prev_x;
            if bw > 0.5 {
                let color = if k % 2 == 0 { c_dark } else { c_light };
                d.rect(prev_x, self.gy, bw, self.gh, color);
            }
            prev_x = next_x;
            if prev_x >= self.gx + self.gw {
                break;
            }
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
            d.text(self.gx - 36.0, y + 4.0, &label, 10.5, MUTED);
            db += step;
        }

        for &(freq, label) in &[
            (20.0, "20"),
            (50.0, "50"),
            (100.0, "100"),
            (200.0, "200"),
            (500.0, "500"),
            (1000.0, "1k"),
            (2000.0, "2k"),
            (5000.0, "5k"),
            (10000.0, "10k"),
            (20000.0, "20k"),
        ] {
            if freq < self.max_freq {
                let x = self.freq_to_x(freq);
                d.line(x, self.gy, x, self.gy + self.gh, LINE, 1.0);
                d.text(x - 9.0, self.gy + self.gh + 18.0, label, 10.5, MUTED);
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
        _low_cut_hz: f64,
        _high_cut_hz: f64,
    ) {
        if mags_db.is_empty() {
            return;
        }
        let bin_hz = srate / fft_size as f64;
        let mut points = Vec::with_capacity(mags_db.len());

        for (k, &db) in mags_db.iter().enumerate() {
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

        d.area(&points, self.gy + self.gh, Color::rgba(65, 140, 240, 45));
        d.poly(&points, Color::rgba(90, 160, 255, 160), 1.2);
    }

    pub fn draw_radius_halo(
        &self,
        d: &mut Draw,
        mouse_x: f32,
        fft_size: usize,
        srate: f64,
        radius: i32,
    ) {
        if !self.in_graph(mouse_x, self.center_y()) {
            return;
        }
        let bin_hz = srate / fft_size as f64;
        let freq = self.x_to_freq(mouse_x);
        let bin = (freq / bin_hz.max(1e-6)).floor() as i32;
        let lo = (bin - radius).max(0) as f64 * bin_hz;
        let hi = (bin + radius + 1) as f64 * bin_hz;
        let x0 = self.freq_to_x(lo).max(self.gx);
        let x1 = self.freq_to_x(hi).min(self.gx + self.gw);
        if x1 > x0 {
            d.rect(
                x0,
                self.gy,
                x1 - x0,
                self.gh,
                Color::rgba(180, 140, 255, 28),
            );
        }
    }

    pub fn draw_filter_gains(&self, d: &mut Draw, filters: &[(f32, f32)]) {
        if filters.is_empty() {
            return;
        }

        let mut points = Vec::with_capacity(filters.len());
        for &(center_hz, gain_db) in filters {
            let freq = center_hz as f64;
            if freq < self.min_freq || freq > self.max_freq {
                continue;
            }
            let x = self.freq_to_x(freq);
            let y = self.db_to_y(gain_db as f64);
            points.push((x, y));
        }

        if points.len() >= 2 {
            d.poly(&points, COLOR_GAIN_LINE, 2.0);
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

        if clipped.len() >= 2 {
            d.area(&clipped, self.center_y(), fill);
            // Show the unclipped request as a faint line when it exceeds the clamp.
            let exceeded = raw.iter().zip(clipped.iter()).any(|(a, b)| (a.1 - b.1).abs() > 0.5);
            if exceeded {
                let ghost = match polarity {
                    Polarity::Boost => Color::rgba(70, 150, 255, 70),
                    Polarity::Cut => Color::rgba(235, 85, 85, 70),
                };
                d.poly(&raw, ghost, 1.0);
            }
            d.poly(&clipped, stroke, if hover { 2.4 } else { 1.8 });
        }

        for node in nodes {
            let x = self.freq_to_x(node.freq);
            let y = self.clip_y(
                polarity,
                self.strength_y(polarity, strength_pct, node.weight),
                max_boost_db,
                max_cut_db,
            );
            let r = if selected == Some(node.id) { 7.0 } else { 5.5 };
            d.circle(x, y, r, stroke, true);
            d.circle(
                x,
                y,
                r * 0.45,
                if selected == Some(node.id) {
                    rgb(255, 255, 255)
                } else {
                    rgb(230, 230, 235)
                },
                true,
            );
        }
    }

    pub fn draw_max_handles(
        &self,
        d: &mut Draw,
        max_boost_db: f32,
        max_cut_db: f32,
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

        d.line(
            self.gx,
            boost_y,
            self.gx + self.gw,
            boost_y,
            Color::rgba(70, 150, 255, 140),
            1.2,
        );
        d.line(
            self.gx,
            cut_y,
            self.gx + self.gw,
            cut_y,
            Color::rgba(235, 85, 85, 140),
            1.2,
        );

        if boost_y > self.gy + 1.0 {
            d.rect(
                self.gx,
                self.gy,
                self.gw,
                boost_y - self.gy,
                Color::rgba(70, 150, 255, 16),
            );
        }
        let bottom = self.gy + self.gh;
        if cut_y < bottom - 1.0 {
            d.rect(
                self.gx,
                cut_y,
                self.gw,
                bottom - cut_y,
                Color::rgba(235, 85, 85, 16),
            );
        }

        let left = [
            (self.gx, boost_y - TRIANGLE_SIZE),
            (self.gx + TRIANGLE_SIZE, boost_y),
            (self.gx, boost_y + TRIANGLE_SIZE),
        ];
        d.area(&left, boost_y + TRIANGLE_SIZE, boost_c);
        d.poly(&left, boost_c, 1.2);

        let right = [
            (self.gx + self.gw, cut_y - TRIANGLE_SIZE),
            (self.gx + self.gw - TRIANGLE_SIZE, cut_y),
            (self.gx + self.gw, cut_y + TRIANGLE_SIZE),
        ];
        d.area(&right, cut_y + TRIANGLE_SIZE, cut_c);
        d.poly(&right, cut_c, 1.2);
    }

    pub fn hit_max_boost(&self, x: f32, y: f32, max_boost_db: f32) -> bool {
        let hy = self.db_to_y(max_boost_db as f64);
        (x - self.gx).abs() <= HIT_DIST + 4.0 && (y - hy).abs() <= HIT_DIST + 2.0
    }

    pub fn hit_max_cut(&self, x: f32, y: f32, max_cut_db: f32) -> bool {
        let hy = self.db_to_y(-(max_cut_db as f64));
        (x - (self.gx + self.gw)).abs() <= HIT_DIST + 4.0 && (y - hy).abs() <= HIT_DIST + 2.0
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
        max_boost_db: f32,
        max_cut_db: f32,
        x: f32,
        y: f32,
    ) -> Option<u64> {
        let mut best = None;
        let mut best_d = crate::strength::NODE_HIT_R;
        for node in nodes {
            let nx = self.freq_to_x(node.freq);
            let ny = self.clip_y(
                polarity,
                self.strength_y(polarity, strength_pct, node.weight),
                max_boost_db,
                max_cut_db,
            );
            let d = ((x - nx).powi(2) + (y - ny).powi(2)).sqrt();
            if d <= best_d {
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
    ) {
        if tilt_amount.abs() <= 0.001 {
            return;
        }

        let steps = 180;
        let mut points = Vec::with_capacity(steps + 1);

        for i in 0..=steps {
            let norm = i as f64 / steps as f64;
            let freq = flattery_pos_to_freq(norm, self.min_freq, self.max_freq);
            let mult = calculate_tilt_multiplier_scaled(freq, tilt_freq_hz, tilt_amount * 0.01, srate);
            let db = linear_to_db(mult);
            let y = self.center_y() - (db / 12.0) as f32 * (self.gh * 0.25);
            let x = self.gx + norm as f32 * self.gw;
            points.push((x, y));
        }

        d.poly(&points, COLOR_TILT, 1.8);
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
        let mult = calculate_tilt_multiplier_scaled(tilt_freq_hz, tilt_freq_hz, tilt_amount * 0.01, srate);
        let db = linear_to_db(mult);
        let y = self.center_y() - (db / 12.0) as f32 * (self.gh * 0.25);

        d.circle(x, y, 7.0, rgb(160, 45, 175), true);
        d.circle(
            x,
            y,
            3.5,
            if hover { rgb(255, 255, 255) } else { rgb(220, 220, 220) },
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
        let tri_l = [
            (low_x, self.gy),
            (low_x + TRIANGLE_SIZE, self.gy),
            (low_x, self.gy + TRIANGLE_SIZE * 1.5),
        ];
        d.poly(&tri_l, color_low, 1.5);
        d.area(&tri_l, self.gy + TRIANGLE_SIZE * 1.5, color_low);

        d.line(high_x, self.gy, high_x, self.gy + self.gh, color_high, 2.5);
        let tri_h = [
            (high_x, self.gy),
            (high_x - TRIANGLE_SIZE, self.gy),
            (high_x, self.gy + TRIANGLE_SIZE * 1.5),
        ];
        d.poly(&tri_h, color_high, 1.5);
        d.area(&tri_h, self.gy + TRIANGLE_SIZE * 1.5, color_high);
    }
}
