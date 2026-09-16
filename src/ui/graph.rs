use crate::dsp::tilt::calculate_tilt_multiplier_scaled;
use pleasant_ui::{
    draw::Draw,
    math::{flattery_freq_to_pos, flattery_pos_to_freq, linear_to_db},
    theme::{rgb, LINE, MUTED},
};
use nih_plug_vizia::vizia::vg::Color;

pub const GRAPH_X: f32 = 60.0;
pub const GRAPH_Y: f32 = 90.0;
pub const GRAPH_W: f32 = 920.0;
pub const GRAPH_H: f32 = 280.0;
pub const TRIANGLE_SIZE: f32 = 9.0;
pub const HIT_DIST: f32 = 12.0;

pub const COLOR_LOW_CUT: Color = rgb(235, 95, 95);
pub const COLOR_LOW_CUT_HOVER: Color = rgb(255, 130, 130);
pub const COLOR_HIGH_CUT: Color = rgb(95, 220, 120);
pub const COLOR_HIGH_CUT_HOVER: Color = rgb(130, 255, 150);
pub const COLOR_TILT: Color = rgb(240, 80, 150);
pub const COLOR_GAIN_LINE: Color = rgb(245, 215, 50);

pub struct GraphLayout {
    pub gx: f32,
    pub gy: f32,
    pub gw: f32,
    pub gh: f32,
    pub min_freq: f64,
    pub max_freq: f64,
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
        }
    }
}

impl GraphLayout {
    pub fn freq_to_x(&self, freq: f64) -> f32 {
        let pos = flattery_freq_to_pos(freq, self.min_freq, self.max_freq) as f32;
        self.gx + pos * self.gw
    }

    pub fn x_to_freq(&self, x: f32) -> f64 {
        let pos = ((x - self.gx) / self.gw).clamp(0.0, 1.0) as f64;
        flattery_pos_to_freq(pos, self.min_freq, self.max_freq)
    }

    pub fn db_to_y(&self, db: f64, db_scale: f64) -> f32 {
        let center = self.gy + self.gh * 0.5;
        center - (db / db_scale) as f32 * (self.gh * 0.5)
    }

    pub fn draw_background(&self, d: &mut Draw, fft_size: usize, srate: f64) {
        let pos_bins = fft_size / 2;
        let bin_hz = srate / fft_size as f64;
        let c_dark = rgb(23, 27, 33);
        let c_light = rgb(27, 32, 39);

        // Draw alternating bands for FFT bins
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

        // Draw border
        d.outline((self.gx, self.gy, self.gw, self.gh), LINE);
    }

    pub fn draw_grid_and_labels(&self, d: &mut Draw) {
        // Horizontal dB lines (-12 to +12 in steps of 3)
        for db in [-12, -9, -6, -3, 0, 3, 6, 9, 12] {
            let y = self.db_to_y(db as f64, 12.0);
            d.line(
                self.gx,
                y,
                self.gx + self.gw,
                y,
                if db == 0 { rgb(75, 82, 92) } else { LINE },
                1.0,
            );
            let label = if db > 0 {
                format!("+{}", db)
            } else {
                format!("{}", db)
            };
            d.text(self.gx - 32.0, y + 4.0, &label, 10.5, MUTED);
        }

        // Vertical frequency lines
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
            // -120 dB at bottom, 0 dB at center line
            let norm_mag = ((db as f64 + 120.0) / 120.0).clamp(0.0, 1.0);
            let y = (self.gy + self.gh) - norm_mag as f32 * (self.gh * 0.5);
            points.push((x, y));
        }

        if points.is_empty() {
            return;
        }

        // Fill area under magnitude curve
        d.area(&points, self.gy + self.gh, Color::rgba(65, 140, 240, 45));
        d.poly(&points, Color::rgba(90, 160, 255, 160), 1.2);
    }

    pub fn draw_filter_gains(
        &self,
        d: &mut Draw,
        filters: &[(f32, f32)],
        _low_cut_hz: f64,
        _high_cut_hz: f64,
    ) {
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
            let y = self.db_to_y(gain_db as f64, 12.0);
            points.push((x, y));
        }

        if points.len() >= 2 {
            d.poly(&points, COLOR_GAIN_LINE, 2.0);
        }
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
            // Quarter height around 0 dB center
            let y = self.gy + self.gh * 0.5 - (db / 12.0) as f32 * (self.gh * 0.25);
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
        let y = self.gy + self.gh * 0.5 - (db / 12.0) as f32 * (self.gh * 0.25);

        // Purple outer circle
        d.circle(x, y, 7.0, rgb(160, 45, 175), true);
        // White inner circle
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

        // Low cut vertical line
        d.line(low_x, self.gy, low_x, self.gy + self.gh, color_low, 2.5);
        // Low cut right-angle triangle handle at top (pointing right)
        let tri_l = [
            (low_x, self.gy),
            (low_x + TRIANGLE_SIZE, self.gy),
            (low_x, self.gy + TRIANGLE_SIZE * 1.5),
        ];
        d.poly(&tri_l, color_low, 1.5);
        d.area(&tri_l, self.gy + TRIANGLE_SIZE * 1.5, color_low);

        // High cut vertical line
        d.line(high_x, self.gy, high_x, self.gy + self.gh, color_high, 2.5);
        // High cut right-angle triangle handle at top (pointing left)
        let tri_h = [
            (high_x, self.gy),
            (high_x - TRIANGLE_SIZE, self.gy),
            (high_x, self.gy + TRIANGLE_SIZE * 1.5),
        ];
        d.poly(&tri_h, color_high, 1.5);
        d.area(&tri_h, self.gy + TRIANGLE_SIZE * 1.5, color_high);
    }
}
