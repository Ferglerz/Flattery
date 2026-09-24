use super::*;

impl FlatteryView {
    pub(super) fn press_value(
        &mut self,
        cx: &mut EventContext,
        target: SliderId,
        rect: (f32, f32, f32, f32),
        origin: (f32, f32),
    ) {
        self.value_press = Some(pleasant_ui::pointer::ValuePress::new(target, rect, origin));
        cx.focus();
        cx.capture();
    }

    pub(super) fn start_edit(
        &mut self,
        cx: &mut EventContext,
        target: SliderId,
        rect: (f32, f32, f32, f32),
        val_str: String,
    ) {
        cx.focus();
        self.edit = Some(ValueEdit::new(target, rect, val_str));
    }

    pub(super) fn commit_edit(&mut self, cx: &mut EventContext) {
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
