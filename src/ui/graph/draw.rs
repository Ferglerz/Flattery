use super::*;

impl GraphLayout {
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

    pub fn draw_radius_bins(
        &self,
        d: &mut Draw,
        nodes: &[StrengthNode],
        bin_hz: f64,
        color: Color,
    ) {
        if !(bin_hz.is_finite() && bin_hz > 0.0) {
            return;
        }
        let right = self.gx + self.gw;
        for node in nodes {
            let center = snap_to_bin_center(node.freq, bin_hz);
            let k = (center / bin_hz - 0.5).round() as i64;
            let radius = node.radius as i64;
            let first = (k - radius).max(0);
            let last = k + radius;
            for bin in first..=last {
                let start = bin as f64 * bin_hz;
                let end = (bin as f64 + 1.0) * bin_hz;
                if end < self.min_freq || start > self.max_freq {
                    continue;
                }
                let x0 = self.freq_to_x(start.max(self.min_freq)).clamp(self.gx, right);
                let x1 = self.freq_to_x(end.min(self.max_freq)).clamp(self.gx, right);
                let w = x1 - x0;
                if w > 0.2 {
                    d.rect(x0, self.gy, w, self.gh, color);
                }
            }
        }
    }

    pub fn draw_grid_and_labels(&self, d: &mut Draw, show_freq_labels: bool) {
        let span = self.db_max - self.db_min;
        let step = if span <= 8.0 {
            1
        } else if span <= 16.0 {
            2
        } else if span <= 28.0 {
            3
        } else if span <= 48.0 {
            6
        } else {
            12
        };

        let mut db = (self.db_min / step as f64).ceil() as i32 * step;
        let last = self.db_max.floor() as i32;
        while db <= last {
            let y = self.db_to_y(db as f64);
            if y >= self.gy - 0.5 && y <= self.gy + self.gh + 0.5 {
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
            }
            db += step;
        }

        let full_freq = (self.min_freq - FULL_MIN_FREQ).abs() < 0.5
            && (self.max_freq - FULL_MAX_FREQ).abs() < 1.0;
        if full_freq {
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
                if freq > self.min_freq && freq < self.max_freq {
                    let x = self.freq_to_x(freq);
                    d.line(x, self.gy, x, self.gy + self.gh, LINE, 1.0);
                    if show_freq_labels && !label.is_empty() {
                        d.text(x - 9.0, self.gy + self.gh + 18.0, label, 10.5, MUTED);
                    }
                }
            }
        } else {
            self.draw_zoomed_freq_axis(d, show_freq_labels);
        }
    }

    pub(super) fn draw_zoomed_freq_axis(&self, d: &mut Draw, show_freq_labels: bool) {
        let mut ticks = Vec::new();
        let mut decade = 1.0_f64;
        while decade <= self.max_freq {
            for mult in [1.0, 2.0, 5.0] {
                let freq = decade * mult;
                if freq > self.min_freq && freq < self.max_freq {
                    ticks.push(freq);
                }
            }
            decade *= 10.0;
        }

        let mut last_x = f32::NEG_INFINITY;
        for freq in ticks {
            let x = self.freq_to_x(freq);
            if x - last_x < 36.0 {
                continue;
            }
            last_x = x;
            d.line(x, self.gy, x, self.gy + self.gh, LINE, 1.0);
            if !show_freq_labels {
                continue;
            }
            let label = if freq >= 1000.0 {
                let k = freq / 1000.0;
                if (k - k.round()).abs() < 0.05 {
                    format!("{}k", k.round() as i32)
                } else {
                    format!("{k:.1}k")
                }
            } else {
                format!("{}", freq.round() as i32)
            };
            d.text(x - 9.0, self.gy + self.gh + 18.0, &label, 10.5, MUTED);
        }
    }

    pub fn draw_limit_shade(&self, d: &mut Draw, max_boost_db: f32, max_cut_db: f32) {
        let top = self
            .db_to_y(max_boost_db as f64)
            .clamp(self.gy, self.gy + self.gh);
        let bot = self
            .db_to_y(-(max_cut_db as f64))
            .clamp(self.gy, self.gy + self.gh);
        let y = top.min(bot);
        let h = (bot - top).abs();
        if h > 1.0 {
            d.rounded_rect(self.gx, y, self.gw, h, 0.0, Color::rgba(210, 218, 230, 13));
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
        draw_split_poly(
            d,
            &points,
            x_lo,
            x_hi,
            Color::rgba(90, 160, 255, 160),
            1.2,
            Some(self.gy + self.gh),
        );
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
        let y_center = self.zero_y();

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

            if bar_h > TRACE_REST_PX {
                let color = if freq < low_cut_hz || freq > high_cut_hz {
                    desaturate(COLOR_GAIN_LINE, OUTSIDE_ALPHA)
                } else {
                    COLOR_GAIN_LINE
                };
                d.rounded_rect(bar_x, bar_y, bar_w, bar_h, 0.0, color);
            }
        }
    }

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
            draw_split_area(d, &clipped, self.zero_y(), x_lo, x_hi, fill);
            let exceeded = raw
                .iter()
                .zip(clipped.iter())
                .any(|(a, b)| (a.1 - b.1).abs() > 0.5);
            if exceeded {
                let ghost = match polarity {
                    Polarity::Boost => Color::rgba(70, 150, 255, 70),
                    Polarity::Cut => Color::rgba(235, 85, 85, 70),
                };
                draw_split_poly(d, &raw, x_lo, x_hi, ghost, 1.0, Some(self.zero_y()));
            }
            draw_split_poly(
                d,
                &clipped,
                x_lo,
                x_hi,
                stroke,
                if hover { 2.4 } else { 1.8 },
                Some(self.zero_y()),
            );
        }

        for node in nodes {
            self.draw_strength_node(
                d,
                polarity,
                strength_pct,
                node,
                stroke,
                selected == Some(node.id),
                low_cut_hz,
                high_cut_hz,
                1.0,
            );
        }
    }

    pub fn draw_strength_node(
        &self,
        d: &mut Draw,
        polarity: Polarity,
        strength_pct: f32,
        node: &StrengthNode,
        stroke: Color,
        selected: bool,
        low_cut_hz: f64,
        high_cut_hz: f64,
        alpha: f32,
    ) {
        let x = self.freq_to_x(node.freq);
        let y = self.strength_y(polarity, strength_pct, node.weight);
        let outside = node.freq < low_cut_hz || node.freq > high_cut_hz;
        let mut node_stroke = if outside {
            desaturate(stroke, OUTSIDE_ALPHA + 0.2)
        } else {
            stroke
        };
        node_stroke.a = (node_stroke.a * alpha).clamp(0.0, 1.0);
        let base_r = if selected { 7.0 } else { 5.5 };

        d.circle(x, y, base_r, node_stroke, true);
        let mut fill = if selected {
            rgb(255, 255, 255)
        } else {
            rgb(230, 230, 235)
        };
        fill.a = (fill.a * alpha).clamp(0.0, 1.0);
        d.circle(x, y, base_r * 0.45, fill, true);
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
            None,
        );
        draw_split_poly(
            d,
            &[(self.gx, cut_y), (right, cut_y)],
            x_lo,
            x_hi,
            cut_c,
            cut_w,
            None,
        );
    }

    pub fn draw_line_grab(&self, d: &mut Draw, mouse_x: f32, line_y: f32, color: Color) {
        let x = mouse_x.clamp(self.gx + 12.0, self.gx + self.gw - 12.0);
        d.tag_handle(x, line_y, TagPointer::None, color);
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

}
