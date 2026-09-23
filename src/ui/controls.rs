use super::*;

impl FlatteryView {
    pub(super) fn side_item_y(idx: usize) -> f32 {
        let total: f32 = SIDE_STACK_HEIGHTS.iter().sum();
        let gap = (GRAPH_H - total) / (SIDE_STACK_HEIGHTS.len() - 1) as f32;
        let mut y = GRAPH_Y;
        for h in SIDE_STACK_HEIGHTS.iter().take(idx) {
            y += *h + gap;
        }
        y
    }

    pub(super) fn side_rect(idx: usize) -> (f32, f32, f32, f32) {
        (
            SIDE_X,
            Self::side_item_y(idx),
            SIDE_W,
            SIDE_STACK_HEIGHTS[idx],
        )
    }

    pub(super) fn node_slider_rect(idx: usize) -> (f32, f32, f32, f32) {
        let gap = 8.0;
        let w = (GRAPH_W - gap * 3.0) / 4.0;
        let x = GRAPH_X + idx as f32 * (w + gap);
        (x, GRAPH_Y + GRAPH_H, w, AXIS_STRIP_H)
    }

    pub(super) fn slider_rect(id: SliderId) -> (f32, f32, f32, f32) {
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

    pub(super) fn output_knob_rect() -> (f32, f32, f32, f32) {
        let w = SIDE_W;
        let h = 108.0;
        let area_y = GRAPH_Y + GRAPH_H;
        let area_h = WINDOW_H - EDGE_PAD - area_y;
        let y = area_y + (area_h - h) * 0.5 + NODE_ROW_GAP;
        (SIDE_X, y, w, h)
    }

    pub(super) fn output_knob_center() -> (f32, f32) {
        let r = Self::output_knob_rect();
        (r.0 + r.2 * 0.5, r.1 + r.3 * 0.5)
    }

    pub(super) fn output_knob_value_rect() -> (f32, f32, f32, f32) {
        let (cx, cy) = Self::output_knob_center();
        (cx - 45.0, cy + 38.0, 90.0, 20.0)
    }

    pub(super) fn footer_button_row() -> (f32, f32, f32, f32) {
        Self::side_rect(4)
    }

    pub(super) fn fft_button_rect(&self) -> (f32, f32, f32, f32) {
        let (x, y, w, h) = Self::footer_button_row();
        (x, y, w - DOMAIN_BTN_W - FOOTER_BTN_GAP, h)
    }

    pub(super) fn domain_button_rect(&self) -> (f32, f32, f32, f32) {
        let (x, y, w, h) = Self::footer_button_row();
        (x + w - DOMAIN_BTN_W, y, DOMAIN_BTN_W, h)
    }

    pub(super) fn inside(px: f32, py: f32, rect: (f32, f32, f32, f32)) -> bool {
        px >= rect.0 && px <= rect.0 + rect.2 && py >= rect.1 && py <= rect.1 + rect.3
    }

    pub(super) fn zoom_button_rects(
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

    pub(super) fn zoom_would_tighten(&self, layout: &GraphLayout) -> bool {
        layout.zoom_would_tighten(
            self.params.low_cut_hz.value() as f64,
            self.params.high_cut_hz.value() as f64,
            self.params.max_boost_db.value(),
            self.params.max_cut_db.value(),
            self.bin_hz(),
        )
    }

    pub(super) fn idle_hover(&self) -> Option<(f32, f32)> {
        pleasant_ui::idle_hover(self.hover, self.drag.is_some())
    }

    pub(super) fn emit_param_norm(&self, cx: &mut EventContext, ptr: ParamPtr, norm: f32) {
        let norm = norm.clamp(0.0, 1.0);
        cx.emit(RawParamEvent::BeginSetParameter(ptr));
        cx.emit(RawParamEvent::SetParameterNormalized(ptr, norm));
        cx.emit(RawParamEvent::EndSetParameter(ptr));
    }

    pub(super) fn reset_float_param(&self, cx: &mut EventContext, p: &FloatParam) {
        self.emit_param_norm(cx, p.as_ptr(), p.default_normalized_value());
    }

    pub(super) fn reset_host_slider(&self, cx: &mut EventContext, id: SliderId) {
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

    pub(super) fn slider_param_ptr(&self, id: SliderId) -> ParamPtr {
        match id {
            SliderId::OutputGain => self.params.output_gain_db.as_ptr(),
            SliderId::StereoLink => self.params.stereo_link.as_ptr(),
            SliderId::Attack => self.params.attack_ms.as_ptr(),
            SliderId::Release => self.params.release_ms.as_ptr(),
            SliderId::InputRms => self.params.input_rms_ms.as_ptr(),
            _ => unreachable!("node sliders do not have ParamPtr"),
        }
    }

    pub(super) fn get_slider_norm(&self, id: SliderId) -> f32 {
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

    pub(super) fn slider_info(&self, id: SliderId) -> (&'static str, String, Color) {
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

    pub(super) fn set_slider_from_x(&self, cx: &mut EventContext, id: SliderId, mouse_x: f32) {
        let r = Self::slider_rect(id);
        let node = matches!(
            id,
            SliderId::NodeFreq | SliderId::NodeGain | SliderId::NodeQ | SliderId::NodeRadius
        );
        let inset = if node { 8.0 } else { 12.0 };
        let bar_x = r.0 + inset;
        let bar_w = (r.2 - inset * 2.0).max(1.0);
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

}
