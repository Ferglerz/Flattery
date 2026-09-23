use super::*;

impl FlatteryView {
    pub(super) fn render(&self, cx: &mut DrawContext, canvas: &mut Canvas) {
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
            if is_light { "LIGHT" } else { "DARK" },
            false,
            MUTED,
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
        layout.draw_grid_and_labels(&mut d, self.selected.is_none());

        let low_cut = self.params.low_cut_hz.value() as f64;
        let high_cut = self.params.high_cut_hz.value() as f64;
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
        let bin_hz = if fft_size == 0 || srate <= 0.0 {
            0.0
        } else {
            srate / fft_size as f64
        };
        let mut boost_tint = COLOR_BOOST;
        boost_tint.a = 0.09;
        let mut cut_tint = COLOR_CUT;
        cut_tint.a = 0.09;
        layout.draw_radius_bins(&mut d, &boost_nodes, bin_hz, boost_tint);
        layout.draw_radius_bins(&mut d, &cut_nodes, bin_hz, cut_tint);
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

        if self.selected.is_some() {
            for &id in NODE_SLIDERS {
                let r = Self::slider_rect(id);
                let (label, val_str, color) = self.slider_info(id);
                let n = self.get_slider_norm(id);
                let val_r = slider_value_rect(r);
                let baseline = r.1 + 16.0;
                d.text(r.0 + 8.0, baseline, label, 15.0, MUTED);
                let editing = self.edit.as_ref().is_some_and(|edit| edit.target == id);
                if editing {
                    if let Some(edit) = &self.edit {
                        d.value_edit(edit, color);
                    }
                } else {
                    d.text_centered(
                        val_r.0 + val_r.2 * 0.5,
                        baseline,
                        &val_str,
                        15.0,
                        color,
                    );
                    if let Some((hx, hy)) = self.idle_hover() {
                        if Self::inside(hx, hy, val_r) {
                            d.value_underline(val_r, color);
                        }
                    }
                }
                let bar_x = r.0 + 8.0;
                let bar_w = r.2 - 16.0;
                let bar_y = r.1 + 24.0;
                d.rect(bar_x, bar_y, bar_w, 5.0, LINE);
                d.rect(bar_x, bar_y, bar_w * n.clamp(0.0, 1.0), 5.0, color);
            }
        }
    }
}

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

