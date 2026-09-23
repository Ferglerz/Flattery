use pleasant_dsp::filters::BiquadCoefficients;

#[derive(Clone, Copy, Default)]
pub struct BiquadState {
    pub x1: f64,
    pub x2: f64,
    pub y1: f64,
    pub y2: f64,
}

impl BiquadState {
    #[inline(always)]
    pub fn process(&mut self, input: f64, coeffs: &[f64; 5]) -> f64 {
        // coeffs: [b0, b1, b2, a1, a2]
        let out = coeffs[0] * input + coeffs[1] * self.x1 + coeffs[2] * self.x2
            - coeffs[3] * self.y1
            - coeffs[4] * self.y2;
        self.x2 = self.x1;
        self.x1 = input;
        self.y2 = self.y1;
        self.y1 = out;
        out
    }

    pub fn reset(&mut self) {
        self.x1 = 0.0;
        self.x2 = 0.0;
        self.y1 = 0.0;
        self.y2 = 0.0;
    }
}

#[derive(Clone)]
pub struct PeakingFilter {
    pub center_hz: f64,
    pub gain_linear: f64,
    pub prev_gain_linear: f64,
    pub coeffs: [f64; 5],
    pub state_l: BiquadState,
    pub state_r: BiquadState,
}

impl PeakingFilter {
    pub fn new(center_hz: f64) -> Self {
        Self {
            center_hz,
            gain_linear: 1.0,
            prev_gain_linear: 1.0,
            coeffs: [1.0, 0.0, 0.0, 0.0, 0.0],
            state_l: BiquadState::default(),
            state_r: BiquadState::default(),
        }
    }

    pub fn reset(&mut self) {
        self.state_l.reset();
        self.state_r.reset();
        self.gain_linear = 1.0;
        self.prev_gain_linear = 1.0;
        self.coeffs = [1.0, 0.0, 0.0, 0.0, 0.0];
    }

    pub fn update_coeffs(&mut self, srate: f64, q: f64) {
        if (self.gain_linear - 1.0).abs() < 0.001 {
            self.coeffs = [1.0, 0.0, 0.0, 0.0, 0.0];
            return;
        }
        self.coeffs =
            BiquadCoefficients::design_peak_linear(self.center_hz, self.gain_linear, q, srate)
                .as_direct_form_array();
    }

    #[inline(always)]
    pub fn process_diff(&mut self, in_l: f64, in_r: f64) -> (f64, f64) {
        if (self.gain_linear - 1.0).abs() < 0.001 {
            return (0.0, 0.0);
        }
        let out_l = self.state_l.process(in_l, &self.coeffs);
        let out_r = self.state_r.process(in_r, &self.coeffs);
        (out_l - in_l, out_r - in_r)
    }
}
