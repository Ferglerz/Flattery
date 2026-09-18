pub mod graph;

use crate::{
    dsp::Shared,
    params::{DifferenceMode, FftSize, FlatteryParams, ProcessDomain},
    ui::graph::{GraphLayout, HIT_DIST},
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
    preferences::AppearanceStore,
    theme::{rgb, BG, COLORS, GOLD, MUTED, PANEL, TEAL, TEXT},
    value_edit::{parse_number_with_units, ValueEdit},
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
const THEME_BUTTON: (f32, f32, f32, f32) = (870.0, 22.0, 72.0, 26.0);
const BYPASS_BUTTON: (f32, f32, f32, f32) = (954.0, 22.0, 32.0, 26.0);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum KnobId {
    StrengthBoost,
    StrengthCut,
    MaxBoost,
    MaxCut,
    OutputGain,
    Attack,
    Release,
    InputRms,
    MinOperate,
    MaxOperate,
    StereoLink,
    TiltAmount,
    TiltFreq,
    LowCut,
    HighCut,
    NeighborRadius,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum DragState {
    LowCut { start_x: f32, start_val: f32 },
    HighCut { start_x: f32, start_val: f32 },
    TiltHandle { start_x: f32, start_y: f32, start_freq: f32, start_tilt: f32 },
    Knob { id: KnobId, start_y: f32, start_norm: f32 },
}

pub struct FlatteryView {
    params: Arc<FlatteryParams>,
    shared: Arc<Shared>,
    layout: GraphLayout,
    font: Cell<Option<FontId>>,
    drag: Option<DragState>,
    hover_low_cut: bool,
    hover_high_cut: bool,
    hover_tilt: bool,
    edit: Option<ValueEdit<KnobId>>,
}

const ALL_KNOBS: &[KnobId] = &[
    KnobId::StrengthBoost,
    KnobId::StrengthCut,
    KnobId::MaxBoost,
    KnobId::MaxCut,
    KnobId::OutputGain,
    KnobId::Attack,
    KnobId::Release,
    KnobId::InputRms,
    KnobId::MinOperate,
    KnobId::MaxOperate,
    KnobId::StereoLink,
    KnobId::TiltAmount,
    KnobId::TiltFreq,
    KnobId::LowCut,
    KnobId::HighCut,
    KnobId::NeighborRadius,
];

fn fmt_hz(f: f32) -> String {
    if f >= 1000.0 {
        format!("{:.1}k", f / 1000.0)
    } else {
        format!("{:.0}Hz", f)
    }
}

impl FlatteryView {
    pub fn param(&self, id: KnobId) -> &FloatParam {
        match id {
            KnobId::StrengthBoost => &self.params.strength_boost,
            KnobId::StrengthCut => &self.params.strength_cut,
            KnobId::MaxBoost => &self.params.max_boost_db,
            KnobId::MaxCut => &self.params.max_cut_db,
            KnobId::OutputGain => &self.params.output_gain_db,
            KnobId::Attack => &self.params.attack_ms,
            KnobId::Release => &self.params.release_ms,
            KnobId::InputRms => &self.params.input_rms_ms,
            KnobId::MinOperate => &self.params.min_operate_db,
            KnobId::MaxOperate => &self.params.max_operate_db,
            KnobId::StereoLink => &self.params.stereo_link,
            KnobId::TiltAmount => &self.params.tilt,
            KnobId::TiltFreq => &self.params.tilt_freq_hz,
            KnobId::LowCut => &self.params.low_cut_hz,
            KnobId::HighCut => &self.params.high_cut_hz,
            KnobId::NeighborRadius => unreachable!(),
        }
    }

    fn knob_rect(id: KnobId) -> (f32, f32, f32, f32) {
        let (row, col) = match id {
            // Row 0: Core Leveling
            KnobId::StrengthBoost => (0, 0),
            KnobId::StrengthCut => (0, 1),
            KnobId::MaxBoost => (0, 2),
            KnobId::MaxCut => (0, 3),
            KnobId::OutputGain => (0, 4),
            // Row 1: Dynamics & Range
            KnobId::Attack => (1, 0),
            KnobId::Release => (1, 1),
            KnobId::InputRms => (1, 2),
            KnobId::MinOperate => (1, 3),
            KnobId::MaxOperate => (1, 4),
            KnobId::StereoLink => (1, 5),
            // Row 2: Shaping & Filtering
            KnobId::TiltAmount => (2, 0),
            KnobId::TiltFreq => (2, 1),
            KnobId::LowCut => (2, 2),
            KnobId::HighCut => (2, 3),
            KnobId::NeighborRadius => (2, 4),
        };

        let start_x = 60.0;
        let start_y = 390.0;
        let col_w = 98.0;
        let col_gap = 14.0;
        let row_h = 92.0;

        let x = start_x + col as f32 * (col_w + col_gap);
        let y = start_y + row as f32 * row_h;
        (x, y, col_w, row_h)
    }

    fn fft_button_rect() -> (f32, f32, f32, f32) {
        (728.0, 390.0, 106.0, 28.0)
    }

    fn domain_button_rect() -> (f32, f32, f32, f32) {
        (728.0, 428.0, 106.0, 28.0)
    }

    fn diff_mode_button_rect() -> (f32, f32, f32, f32) {
        (728.0, 466.0, 106.0, 28.0)
    }

    fn emit_param_norm(&self, cx: &mut EventContext, ptr: ParamPtr, norm: f32) {
        let norm = norm.clamp(0.0, 1.0);
        cx.emit(RawParamEvent::BeginSetParameter(ptr));
        cx.emit(RawParamEvent::SetParameterNormalized(ptr, norm));
        cx.emit(RawParamEvent::EndSetParameter(ptr));
    }

    fn emit_knob_norm(&self, cx: &mut EventContext, id: KnobId, norm: f32) {
        if id == KnobId::NeighborRadius {
            self.emit_param_norm(cx, self.params.neighbor_radius.as_ptr(), norm);
        } else {
            self.emit_param_norm(cx, self.param(id).as_ptr(), norm);
        }
    }

    fn get_knob_norm(&self, id: KnobId) -> f32 {
        if id == KnobId::NeighborRadius {
            self.params.neighbor_radius.unmodulated_normalized_value()
        } else {
            self.param(id).unmodulated_normalized_value()
        }
    }

    fn knob_info(&self, id: KnobId) -> (&'static str, String, Color) {
        match id {
            KnobId::StrengthBoost => (
                "BOOST",
                format!("{:.0}%", self.params.strength_boost.value()),
                COLORS[0],
            ),
            KnobId::StrengthCut => (
                "CUT",
                format!("{:.0}%", self.params.strength_cut.value()),
                COLORS[4],
            ),
            KnobId::MaxBoost => (
                "MAX BOOST",
                format!("{:.1}dB", self.params.max_boost_db.value()),
                COLORS[2],
            ),
            KnobId::MaxCut => (
                "MAX CUT",
                format!("{:.1}dB", self.params.max_cut_db.value()),
                COLORS[2],
            ),
            KnobId::OutputGain => (
                "OUT GAIN",
                format!("{:.1}dB", self.params.output_gain_db.value()),
                GOLD,
            ),
            KnobId::StereoLink => (
                "STEREO LINK",
                format!("{:.0}%", self.params.stereo_link.value()),
                TEAL,
            ),
            KnobId::NeighborRadius => (
                "RADIUS",
                format!("{} bins", self.params.neighbor_radius.value()),
                COLORS[1],
            ),
            KnobId::Attack => (
                "ATTACK",
                format!("{:.1}ms", self.params.attack_ms.value()),
                COLORS[3],
            ),
            KnobId::Release => (
                "RELEASE",
                format!("{:.0}ms", self.params.release_ms.value()),
                COLORS[3],
            ),
            KnobId::InputRms => (
                "INPUT RMS",
                format!("{:.1}ms", self.params.input_rms_ms.value()),
                COLORS[1],
            ),
            KnobId::MinOperate => (
                "OP MIN",
                format!("{:.0}dB", self.params.min_operate_db.value()),
                MUTED,
            ),
            KnobId::MaxOperate => (
                "OP MAX",
                format!("{:.0}dB", self.params.max_operate_db.value()),
                MUTED,
            ),
            KnobId::TiltAmount => (
                "TILT",
                format!("{:.0}%", self.params.tilt.value()),
                rgb(240, 80, 150),
            ),
            KnobId::TiltFreq => ("TILT FREQ", fmt_hz(self.params.tilt_freq_hz.value()), rgb(240, 80, 150)),
            KnobId::LowCut => ("LOW CUT", fmt_hz(self.params.low_cut_hz.value()), rgb(235, 95, 95)),
            KnobId::HighCut => ("HIGH CUT", fmt_hz(self.params.high_cut_hz.value()), rgb(95, 220, 120)),
        }
    }

    fn inside(px: f32, py: f32, rect: (f32, f32, f32, f32)) -> bool {
        px >= rect.0 && px <= rect.0 + rect.2 && py >= rect.1 && py <= rect.1 + rect.3
    }

    fn commit_edit(&mut self, cx: &mut EventContext) {
        if let Some(edit) = self.edit.take() {
            let target = edit.target;
            let text = edit.text;
            let val = match target {
                KnobId::TiltFreq | KnobId::LowCut | KnobId::HighCut => {
                    parse_number_with_units(&text, &[("khz", 1000.0), ("hz", 1.0), ("k", 1000.0)])
                }
                KnobId::Attack | KnobId::Release | KnobId::InputRms => {
                    parse_number_with_units(&text, &[("ms", 1.0), ("s", 1000.0)])
                }
                KnobId::StrengthBoost | KnobId::StrengthCut | KnobId::StereoLink | KnobId::TiltAmount => {
                    parse_number_with_units(&text, &[("%", 1.0)])
                }
                _ => parse_number_with_units(&text, &[("db", 1.0)]),
            };

            if let Some(v) = val {
                if target == KnobId::NeighborRadius {
                    let norm = self.params.neighbor_radius.preview_normalized(v as i32);
                    self.emit_param_norm(cx, self.params.neighbor_radius.as_ptr(), norm);
                } else {
                    let p = self.param(target);
                    let norm = p.preview_normalized(v as f32);
                    self.emit_param_norm(cx, p.as_ptr(), norm);
                }
            }
        }
    }
}

impl View for FlatteryView {
    fn element(&self) -> Option<&'static str> {
        Some("flattery-view")
    }

    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|window_event, meta| {
            let bounds = cx.bounds();
            let scale = bounds.w / 1040.0;
            let mouse_x = (cx.mouse().cursorx - bounds.x) / scale;
            let mouse_y = (cx.mouse().cursory - bounds.y) / scale;

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
                    WindowEvent::KeyDown(Code::Enter, _) => {
                        self.commit_edit(cx);
                        meta.consume();
                        cx.needs_redraw();
                        return;
                    }
                    WindowEvent::KeyDown(Code::Escape, _) => {
                        self.edit = None;
                        meta.consume();
                        cx.needs_redraw();
                        return;
                    }
                    WindowEvent::KeyDown(Code::Backspace, _) => {
                        self.edit.as_mut().unwrap().erase(true);
                        meta.consume();
                        cx.needs_redraw();
                        return;
                    }
                    WindowEvent::MouseDown(MouseButton::Left) => {
                        self.commit_edit(cx);
                        meta.consume();
                        cx.needs_redraw();
                        return;
                    }
                    _ => return,
                }
            }

            match window_event {
                WindowEvent::MouseDown(MouseButton::Left) => {
                    // Theme toggle button
                    if Self::inside(mouse_x, mouse_y, THEME_BUTTON) {
                        prefs().toggle();
                        cx.needs_redraw();
                        return;
                    }

                    // Bypass button
                    if Self::inside(mouse_x, mouse_y, BYPASS_BUTTON) {
                        let current = self.params.bypass.value();
                        let norm = if !current { 1.0 } else { 0.0 };
                        self.emit_param_norm(cx, self.params.bypass.as_ptr(), norm);
                        cx.needs_redraw();
                        return;
                    }

                    // FFT size button
                    if Self::inside(mouse_x, mouse_y, Self::fft_button_rect()) {
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

                    // Domain button (L/R vs M/S)
                    if Self::inside(mouse_x, mouse_y, Self::domain_button_rect()) {
                        let current = self.params.ms_mode.value();
                        let norm = if current == ProcessDomain::LR { 1.0 } else { 0.0 };
                        self.emit_param_norm(cx, self.params.ms_mode.as_ptr(), norm);
                        cx.needs_redraw();
                        return;
                    }

                    // Difference mode button (Reduce vs Amplify)
                    if Self::inside(mouse_x, mouse_y, Self::diff_mode_button_rect()) {
                        let current = self.params.amplify_mode.value();
                        let norm = if current == DifferenceMode::Reduce { 1.0 } else { 0.0 };
                        self.emit_param_norm(cx, self.params.amplify_mode.as_ptr(), norm);
                        cx.needs_redraw();
                        return;
                    }

                    // Graph handle hits
                    let low_x = self.layout.freq_to_x(self.params.low_cut_hz.value() as f64);
                    let high_x = self.layout.freq_to_x(self.params.high_cut_hz.value() as f64);
                    let tilt_x = self.layout.freq_to_x(self.params.tilt_freq_hz.value() as f64);

                    if (mouse_x - low_x).abs() <= HIT_DIST
                        && mouse_y >= self.layout.gy
                        && mouse_y <= self.layout.gy + self.layout.gh
                    {
                        self.drag = Some(DragState::LowCut {
                            start_x: mouse_x,
                            start_val: self.params.low_cut_hz.value(),
                        });
                        cx.needs_redraw();
                        return;
                    }

                    if (mouse_x - high_x).abs() <= HIT_DIST
                        && mouse_y >= self.layout.gy
                        && mouse_y <= self.layout.gy + self.layout.gh
                    {
                        self.drag = Some(DragState::HighCut {
                            start_x: mouse_x,
                            start_val: self.params.high_cut_hz.value(),
                        });
                        cx.needs_redraw();
                        return;
                    }

                    if (mouse_x - tilt_x).abs() <= HIT_DIST + 4.0
                        && mouse_y >= self.layout.gy
                        && mouse_y <= self.layout.gy + self.layout.gh
                    {
                        self.drag = Some(DragState::TiltHandle {
                            start_x: mouse_x,
                            start_y: mouse_y,
                            start_freq: self.params.tilt_freq_hz.value(),
                            start_tilt: self.params.tilt.value(),
                        });
                        cx.needs_redraw();
                        return;
                    }

                    // Knobs hit testing
                    for &id in ALL_KNOBS {
                        let r = Self::knob_rect(id);
                        if Self::inside(mouse_x, mouse_y, r) {
                            self.drag = Some(DragState::Knob {
                                id,
                                start_y: mouse_y,
                                start_norm: self.get_knob_norm(id),
                            });
                            cx.needs_redraw();
                            return;
                        }
                    }
                }

                WindowEvent::MouseDoubleClick(MouseButton::Left) => {
                    for &id in ALL_KNOBS {
                        let r = Self::knob_rect(id);
                        if Self::inside(mouse_x, mouse_y, r) {
                            let (_, val_str, _) = self.knob_info(id);
                            self.edit = Some(ValueEdit::new(id, r, val_str));
                            cx.needs_redraw();
                            return;
                        }
                    }
                }

                WindowEvent::MouseUp(MouseButton::Left) => {
                    self.drag = None;
                    cx.needs_redraw();
                }

                WindowEvent::MouseMove(_, _) => {
                    if let Some(drag) = self.drag {
                        match drag {
                            DragState::LowCut { start_x, start_val } => {
                                let delta = mouse_x - start_x;
                                let new_freq = (self.layout.x_to_freq(self.layout.freq_to_x(start_val as f64) + delta) as f32)
                                    .clamp(10.0, self.params.high_cut_hz.value());
                                let norm = self.params.low_cut_hz.preview_normalized(new_freq);
                                self.emit_param_norm(cx, self.params.low_cut_hz.as_ptr(), norm);
                                cx.needs_redraw();
                            }
                            DragState::HighCut { start_x, start_val } => {
                                let delta = mouse_x - start_x;
                                let new_freq = (self.layout.x_to_freq(self.layout.freq_to_x(start_val as f64) + delta) as f32)
                                    .clamp(self.params.low_cut_hz.value(), 20000.0);
                                let norm = self.params.high_cut_hz.preview_normalized(new_freq);
                                self.emit_param_norm(cx, self.params.high_cut_hz.as_ptr(), norm);
                                cx.needs_redraw();
                            }
                            DragState::TiltHandle { start_x, start_y, start_freq, start_tilt } => {
                                let delta_x = mouse_x - start_x;
                                let delta_y = start_y - mouse_y;
                                let new_freq = (self.layout.x_to_freq(self.layout.freq_to_x(start_freq as f64) + delta_x) as f32)
                                    .clamp(20.0, 20000.0);
                                let new_tilt = (start_tilt + delta_y * 1.0).clamp(-100.0, 100.0);
                                let norm_freq = self.params.tilt_freq_hz.preview_normalized(new_freq);
                                let norm_tilt = self.params.tilt.preview_normalized(new_tilt);
                                self.emit_param_norm(cx, self.params.tilt_freq_hz.as_ptr(), norm_freq);
                                self.emit_param_norm(cx, self.params.tilt.as_ptr(), norm_tilt);
                                cx.needs_redraw();
                            }
                            DragState::Knob { id, start_y, start_norm } => {
                                let delta_y = start_y - mouse_y;
                                let step = if cx.modifiers().shift() { 0.001 } else { 0.005 };
                                let new_norm = (start_norm + delta_y * step).clamp(0.0, 1.0);
                                self.emit_knob_norm(cx, id, new_norm);
                                cx.needs_redraw();
                            }
                        }
                    } else {
                        // Hover checking
                        let low_x = self.layout.freq_to_x(self.params.low_cut_hz.value() as f64);
                        let high_x = self.layout.freq_to_x(self.params.high_cut_hz.value() as f64);
                        let tilt_x = self.layout.freq_to_x(self.params.tilt_freq_hz.value() as f64);

                        let was_low = self.hover_low_cut;
                        let was_high = self.hover_high_cut;
                        let was_tilt = self.hover_tilt;

                        self.hover_low_cut = (mouse_x - low_x).abs() <= HIT_DIST
                            && mouse_y >= self.layout.gy
                            && mouse_y <= self.layout.gy + self.layout.gh;
                        self.hover_high_cut = (mouse_x - high_x).abs() <= HIT_DIST
                            && mouse_y >= self.layout.gy
                            && mouse_y <= self.layout.gy + self.layout.gh;
                        self.hover_tilt = (mouse_x - tilt_x).abs() <= HIT_DIST + 4.0
                            && mouse_y >= self.layout.gy
                            && mouse_y <= self.layout.gy + self.layout.gh;

                        if was_low != self.hover_low_cut || was_high != self.hover_high_cut || was_tilt != self.hover_tilt {
                            cx.needs_redraw();
                        }
                    }
                }

                _ => {}
            }
        });
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &mut Canvas) {
        let bounds = cx.bounds();
        if self.font.get().is_none() {
            self.font
                .set(canvas.add_font_mem(FONT_JETBRAINS_MONO).ok());
        }

        let mut d = Draw::new(
            canvas,
            prefs().light(),
            bounds.w / 1040.0,
            bounds.x,
            bounds.y,
            self.font.get(),
        );

        // Window background
        d.rect(0.0, 0.0, 1040.0, 700.0, BG);

        // Header
        d.rect(0.0, 0.0, 1040.0, HEADER_HEIGHT, PANEL);
        d.text(36.0, 44.0, "FLATTERY", 24.0, GOLD);
        d.text(192.0, 44.0, "SPECTRAL LEVELER & SHAPER", 13.0, TEXT);

        // Theme button
        let is_light = prefs().light();
        d.button(
            THEME_BUTTON,
            if is_light { "DARK" } else { "LIGHT" },
            false,
            MUTED,
        );

        // Bypass button
        let bypassed = self.params.bypass.value();
        d.bypass_button(BYPASS_BUTTON, bypassed, TEAL);

        // Graph display
        let srate = self.shared.sample_rate.load(Ordering::Relaxed) as f64;
        let fft_size = self.params.fft_size.value().size();
        self.layout.draw_background(&mut d, fft_size, srate);
        self.layout.draw_grid_and_labels(&mut d);

        let low_cut = self.params.low_cut_hz.value() as f64;
        let high_cut = self.params.high_cut_hz.value() as f64;
        let tilt = self.params.tilt.value() as f64;
        let tilt_freq = self.params.tilt_freq_hz.value() as f64;

        // Draw live spectrum
        if let Ok(mags) = self.shared.spectrum_mags_db.read() {
            self.layout.draw_spectrum(&mut d, &mags, fft_size, srate, low_cut, high_cut);
        }

        // Draw active filter gains
        if let Ok(filters) = self.shared.filter_display.read() {
            self.layout.draw_filter_gains(&mut d, &filters, low_cut, high_cut);
        }

        // Draw tilt response curve & handle
        self.layout.draw_tilt_curve(&mut d, tilt, tilt_freq, srate);
        self.layout.draw_tilt_handle(&mut d, tilt, tilt_freq, srate, self.hover_tilt);

        // Draw low & high cut vertical handles
        self.layout.draw_cut_handles(
            &mut d,
            low_cut,
            high_cut,
            self.hover_low_cut,
            self.hover_high_cut,
        );

        // Draw bottom parameter cards
        for &id in ALL_KNOBS {
            let r = Self::knob_rect(id);
            let (label, val_str, color) = self.knob_info(id);
            let n = self.get_knob_norm(id);

            if let Some(edit) = &self.edit {
                if edit.target == id {
                    d.rect(r.0, r.1, r.2, r.3, PANEL);
                    d.outline(r, GOLD);
                    d.text_centered(r.0 + r.2 * 0.5, r.1 + 22.0, label, 9.2, TEXT);
                    d.text_centered(r.0 + r.2 * 0.5, r.1 + 50.0, &edit.text, 12.0, GOLD);
                    continue;
                }
            }

            d.knob(r, label, &val_str, n, color, bypassed);
        }

        // FFT Size selector button
        let fft_rect = Self::fft_button_rect();
        let fft_label = format!("FFT: {}", fft_size);
        d.button(fft_rect, &fft_label, false, GOLD);

        // Process Domain button (LR / MS)
        let domain_rect = Self::domain_button_rect();
        let domain_label = match self.params.ms_mode.value() {
            ProcessDomain::LR => "MODE: L/R",
            ProcessDomain::MS => "MODE: M/S",
        };
        d.button(domain_rect, domain_label, false, TEAL);

        // Difference mode button (Reduce / Amplify)
        let diff_rect = Self::diff_mode_button_rect();
        let diff_label = match self.params.amplify_mode.value() {
            DifferenceMode::Reduce => "DIFF: REDUCE",
            DifferenceMode::Amplify => "DIFF: AMPLIFY",
        };
        d.button(diff_rect, diff_label, false, COLORS[1]);
    }
}

pub fn create(params: Arc<FlatteryParams>, shared: Arc<Shared>) -> Option<Box<dyn Editor>> {
    create_vizia_editor(
        params.editor_state.clone(),
        ViziaTheming::Custom,
        move |cx, _| {
            nih_plug_vizia::assets::register_noto_sans_light(cx);
            FlatteryView {
                params: params.clone(),
                shared: shared.clone(),
                layout: GraphLayout::default(),
                font: Cell::new(None),
                drag: None,
                hover_low_cut: false,
                hover_high_cut: false,
                hover_tilt: false,
                edit: None,
            }
            .build(cx, |cx| {
                let timer = cx.add_timer(Duration::from_millis(16), None, |cx, action| {
                    if let TimerAction::Tick(_) = action {
                        cx.needs_redraw();
                    }
                });
                cx.start_timer(timer);
            })
            .width(Stretch(1.0))
            .height(Stretch(1.0));
        },
    )
}
