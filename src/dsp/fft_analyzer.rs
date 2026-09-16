use rustfft::num_complex::Complex;
use rustfft::{Fft, FftPlanner};
use std::sync::Arc;

type Complex64 = Complex<f64>;

pub struct FftAnalyzer {
    pub fft_size: usize,
    fft: Arc<dyn Fft<f64>>,
    window: Vec<f64>,
    input_l: Vec<Complex64>,
    input_r: Vec<Complex64>,
    pub mag_l: Vec<f64>,
    pub mag_r: Vec<f64>,
    rms_state_l: Vec<f64>,
    rms_state_r: Vec<f64>,
}

impl FftAnalyzer {
    pub fn new(fft_size: usize) -> Self {
        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(fft_size);
        let window = Self::make_hann_window(fft_size);
        let half = fft_size / 2;

        Self {
            fft_size,
            fft,
            window,
            input_l: vec![Complex64::default(); fft_size],
            input_r: vec![Complex64::default(); fft_size],
            mag_l: vec![0.0; half],
            mag_r: vec![0.0; half],
            rms_state_l: vec![0.0; half],
            rms_state_r: vec![0.0; half],
        }
    }

    pub fn resize(&mut self, new_size: usize) {
        if self.fft_size == new_size {
            return;
        }
        let mut planner = FftPlanner::new();
        self.fft = planner.plan_fft_forward(new_size);
        self.fft_size = new_size;
        self.window = Self::make_hann_window(new_size);
        self.input_l = vec![Complex64::default(); new_size];
        self.input_r = vec![Complex64::default(); new_size];
        let half = new_size / 2;
        self.mag_l = vec![0.0; half];
        self.mag_r = vec![0.0; half];
        self.rms_state_l = vec![0.0; half];
        self.rms_state_r = vec![0.0; half];
    }

    pub fn reset(&mut self) {
        self.input_l.fill(Complex64::default());
        self.input_r.fill(Complex64::default());
        self.mag_l.fill(0.0);
        self.mag_r.fill(0.0);
        self.rms_state_l.fill(0.0);
        self.rms_state_r.fill(0.0);
    }

    fn make_hann_window(n: usize) -> Vec<f64> {
        let mut w = vec![0.0; n];
        let inv_n = 1.0 / (n.saturating_sub(1).max(1) as f64);
        for (i, item) in w.iter_mut().enumerate() {
            let x = 2.0 * std::f64::consts::PI * i as f64 * inv_n;
            *item = 0.5 * (1.0 - x.cos());
        }
        // Normalize for 50% overlap
        let hop = n / 2;
        let mut sum = 0.0;
        for i in 0..hop {
            sum += w[i] + w[(i + hop) % n];
        }
        let avg_sum = sum / hop.max(1) as f64;
        let norm = 1.0 / avg_sum.max(1e-9);
        for x in &mut w {
            *x *= norm;
        }
        w
    }

    pub fn analyze(
        &mut self,
        samples_l: &[f64],
        samples_r: &[f64],
        is_ms: bool,
        input_rms_ms: f64,
        frame_dt: f64,
    ) {
        let n = self.fft_size;
        let half = n / 2;
        let norm = 2.0 / n as f64;

        if is_ms {
            for i in 0..n {
                let m = 0.5 * (samples_l[i] + samples_r[i]);
                let s = 0.5 * (samples_l[i] - samples_r[i]);
                let win = self.window[i];
                self.input_l[i] = Complex64::new(m * win, 0.0);
                self.input_r[i] = Complex64::new(s * win, 0.0);
            }
        } else {
            for i in 0..n {
                let win = self.window[i];
                self.input_l[i] = Complex64::new(samples_l[i] * win, 0.0);
                self.input_r[i] = Complex64::new(samples_r[i] * win, 0.0);
            }
        }

        self.fft.process(&mut self.input_l);
        self.fft.process(&mut self.input_r);

        for k in 0..half {
            let cl = self.input_l[k];
            let cr = self.input_r[k];
            self.mag_l[k] = (cl.re * cl.re + cl.im * cl.im).sqrt() * norm;
            self.mag_r[k] = (cr.re * cr.re + cr.im * cr.im).sqrt() * norm;
        }

        if input_rms_ms > 0.001 {
            let rms_s = input_rms_ms * 0.001;
            let rms_coeff = (-frame_dt / rms_s.max(1e-9)).exp();
            let rms_inv = 1.0 - rms_coeff;

            for k in 0..half {
                let vl = self.mag_l[k];
                let vr = self.mag_r[k];
                self.rms_state_l[k] += (vl * vl - self.rms_state_l[k]) * rms_inv;
                self.rms_state_r[k] += (vr * vr - self.rms_state_r[k]) * rms_inv;
                self.mag_l[k] = self.rms_state_l[k].max(0.0).sqrt();
                self.mag_r[k] = self.rms_state_r[k].max(0.0).sqrt();
            }
        }
    }
}
