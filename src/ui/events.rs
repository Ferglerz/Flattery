use super::*;

impl FlatteryView {
    pub(super) fn handle_event(&mut self, cx: &mut EventContext, event: &mut Event) {
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
}
