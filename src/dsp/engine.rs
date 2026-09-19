use crate::{
    dsp::{
        constants::MAX_FFT_SIZE,
        fft_analyzer::FftAnalyzer,
        filter_bank::FilterBank,
        leveling::LevelingProcessor,
        ring_buffer::{AnalysisRing, DelayLine},
        telemetry::Shared,
        tilt::{apply_tilt_compensation, calculate_tilt_multiplier_scaled},
    },
    params::{DifferenceMode, FlatteryParams, ProcessDomain},
    strength::{fill_bin_weights, StrengthNode},
};
use pleasant_ui::math::{db_to_linear, linear_to_db};
use std::sync::{atomic::Ordering, Arc, Mutex};

pub struct Engine {
    pub shared: Arc<Shared>,
    pub sample_rate: f64,
    pub fft_size: usize,
    pub hop_size: usize,
    pub hop_counter: usize,
    pub analysis_delay: usize,
    delay_l: DelayLine,
    delay_r: DelayLine,
    ring_l: AnalysisRing,
    ring_r: AnalysisRing,
    analyzer: FftAnalyzer,
    leveler: LevelingProcessor,
    filter_bank: FilterBank,
    window_samples_l: Vec<f64>,
    window_samples_r: Vec<f64>,
    target_gains: Vec<f64>,
    boost_weights: Vec<f64>,
    cut_weights: Vec<f64>,
}

impl Engine {
    pub fn new(shared: Arc<Shared>, sample_rate: f64) -> Self {
        let fft_size = 512;
        let hop_size = fft_size / 2;
        let analysis_delay = hop_size;

        let mut filter_bank = FilterBank::new();
        filter_bank.init_frequencies(fft_size, sample_rate);

        Self {
            shared,
            sample_rate,
            fft_size,
            hop_size,
            hop_counter: 0,
            analysis_delay,
            delay_l: DelayLine::new(),
            delay_r: DelayLine::new(),
            ring_l: AnalysisRing::new(),
            ring_r: AnalysisRing::new(),
            analyzer: FftAnalyzer::new(fft_size),
            leveler: LevelingProcessor::new(),
            filter_bank,
            window_samples_l: vec![0.0; MAX_FFT_SIZE],
            window_samples_r: vec![0.0; MAX_FFT_SIZE],
            target_gains: vec![1.0; 1024],
            boost_weights: vec![1.0; 1024],
            cut_weights: vec![1.0; 1024],
        }
    }

    pub fn reset(&mut self) {
        self.delay_l.reset();
        self.delay_r.reset();
        self.ring_l.reset();
        self.ring_r.reset();
        self.analyzer.reset();
        self.leveler.reset();
        self.filter_bank.reset();
        self.hop_counter = 0;
    }

    pub fn latency(&self) -> u32 {
        self.analysis_delay as u32
    }

    pub fn set_fft_size(&mut self, new_size: usize) {
        if self.fft_size == new_size {
            return;
        }
        self.fft_size = new_size;
        self.hop_size = new_size / 2;
        self.analysis_delay = self.hop_size;
        self.hop_counter = 0;
        self.analyzer.resize(new_size);
        self.filter_bank.init_frequencies(new_size, self.sample_rate);
        self.shared.fft_size.store(new_size, Ordering::Relaxed);
    }

    pub fn set_sample_rate(&mut self, srate: f64) {
        self.sample_rate = srate;
        self.filter_bank.init_frequencies(self.fft_size, srate);
        self.shared
            .sample_rate
            .store(srate as f32, Ordering::Relaxed);
    }

    pub fn tick(&mut self, in_l: f64, in_r: f64, params: &FlatteryParams) -> (f64, f64) {
        let current_fft_size = params.fft_size.value().size();
        if current_fft_size != self.fft_size {
            self.set_fft_size(current_fft_size);
        }

        let bypassed = params.bypass.value();
        self.shared.is_bypassed.store(bypassed, Ordering::Relaxed);

        let low_cut_hz = params.low_cut_hz.value() as f64;
        let high_cut_hz = params.high_cut_hz.value() as f64;
        let tilt = params.tilt.value() as f64;
        let tilt_freq_hz = params.tilt_freq_hz.value() as f64;
        let max_boost_db = params.max_boost_db.value() as f64;
        let max_cut_db = params.max_cut_db.value() as f64;
        let output_gain_db = params.output_gain_db.value() as f64;

        // Push to analysis rings
        self.ring_l.push(in_l);
        self.ring_r.push(in_r);

        // Delayed dry audio aligned to analysis window
        let delayed_l = self.delay_l.write_and_read(in_l, self.analysis_delay);
        let delayed_r = self.delay_r.write_and_read(in_r, self.analysis_delay);

        self.hop_counter += 1;
        if self.hop_counter >= self.hop_size {
            self.hop_counter = 0;
            let n = self.fft_size;
            let half = n / 2;
            let bin_hz = self.sample_rate / n as f64;
            let frame_dt = self.hop_size as f64 / self.sample_rate;

            self.ring_l
                .read_window(n, &mut self.window_samples_l[..n]);
            self.ring_r
                .read_window(n, &mut self.window_samples_r[..n]);

            let is_ms = params.ms_mode.value() == ProcessDomain::MS;
            let input_rms_ms = params.input_rms_ms.value() as f64;

            self.analyzer.analyze(
                &self.window_samples_l[..n],
                &self.window_samples_r[..n],
                is_ms,
                input_rms_ms,
                frame_dt,
            );

            let low_cut_bin = (low_cut_hz / bin_hz.max(1e-6)).floor() as usize;
            let high_cut_bin = (high_cut_hz / bin_hz.max(1e-6)).floor() as usize;

            let min_operate_lin = db_to_linear(params.min_operate_db.value() as f64);
            let max_operate_lin = db_to_linear(params.max_operate_db.value() as f64);

            let str_boost = params.strength_boost.value() as f64;
            let str_cut = params.strength_cut.value() as f64;
            let stereo_link = params.stereo_link.value() as f64;
            let radius = params.neighbor_radius.value() as usize;
            let amplify = params.amplify_mode.value() == DifferenceMode::Amplify;
            let att_ms = params.attack_ms.value() as f64;
            let rel_ms = params.release_ms.value() as f64;

            if self.boost_weights.len() < half {
                self.boost_weights.resize(half, 1.0);
                self.cut_weights.resize(half, 1.0);
            }
            let boost_nodes = snapshot_nodes(&params.boost_nodes);
            let cut_nodes = snapshot_nodes(&params.cut_nodes);
            fill_bin_weights(
                &boost_nodes,
                half,
                bin_hz,
                10.0,
                22050.0,
                &mut self.boost_weights,
            );
            fill_bin_weights(
                &cut_nodes,
                half,
                bin_hz,
                10.0,
                22050.0,
                &mut self.cut_weights,
            );

            self.leveler.process(
                &self.analyzer.mag_l,
                &self.analyzer.mag_r,
                half,
                radius,
                amplify,
                stereo_link,
                str_boost,
                str_cut,
                max_boost_db,
                max_cut_db,
                low_cut_bin,
                high_cut_bin,
                min_operate_lin,
                max_operate_lin,
                att_ms,
                rel_ms,
                frame_dt,
                &self.boost_weights[..half],
                &self.cut_weights[..half],
            );

            // Map leveler gains to filter bank
            let filter_count = self.filter_bank.filters.len();
            if self.target_gains.len() < filter_count {
                self.target_gains.resize(filter_count, 1.0);
            }

            let tilt_amount = tilt * 0.01;
            let is_linked = stereo_link >= 99.5;

            for i in 0..filter_count {
                let center_hz = self.filter_bank.filters[i].center_hz;
                let bin_idx = ((center_hz / bin_hz.max(1e-6)).floor() as usize).min(half - 1);

                let raw_gain_db = if is_linked {
                    self.leveler.smoothed_gain_db_link[bin_idx]
                } else {
                    0.5 * (self.leveler.smoothed_gain_db_l[bin_idx]
                        + self.leveler.smoothed_gain_db_r[bin_idx])
                };

                let tilt_mult_scaled = calculate_tilt_multiplier_scaled(
                    center_hz,
                    tilt_freq_hz,
                    tilt_amount,
                    self.sample_rate,
                );
                let tilted_gain_db =
                    (raw_gain_db * tilt_mult_scaled).clamp(-max_cut_db, max_boost_db);
                let tilted_gain_lin = db_to_linear(tilted_gain_db);
                let compensated_gain_lin =
                    apply_tilt_compensation(tilted_gain_lin, tilt_mult_scaled, tilt_amount);

                self.target_gains[i] = compensated_gain_lin;
            }

            if str_boost == 0.0 && str_cut == 0.0 {
                self.target_gains[..filter_count].fill(1.0);
            }

            self.filter_bank.set_filter_gains(
                &self.target_gains[..filter_count],
                self.sample_rate,
                low_cut_hz,
                high_cut_hz,
            );

            // Update UI telemetry lock-free
            if let Ok(mut lock) = self.shared.spectrum_mags_db.try_write() {
                lock.clear();
                for k in 0..half {
                    let avg = 0.5 * (self.analyzer.mag_l[k] + self.analyzer.mag_r[k]);
                    lock.push(linear_to_db(avg) as f32);
                }
            }
            if let Ok(mut lock) = self.shared.filter_display.try_write() {
                lock.clear();
                for f in &self.filter_bank.filters {
                    lock.push((f.center_hz as f32, linear_to_db(f.gain_linear) as f32));
                }
            }
        }

        if bypassed {
            return (delayed_l, delayed_r);
        }

        let (wet_l, wet_r) = self.filter_bank.process(delayed_l, delayed_r);
        let out_gain = db_to_linear(output_gain_db);
        (wet_l * out_gain, wet_r * out_gain)
    }
}

fn snapshot_nodes(nodes: &Mutex<Vec<StrengthNode>>) -> Vec<StrengthNode> {
    match nodes.lock() {
        Ok(guard) => guard.clone(),
        Err(poisoned) => poisoned.into_inner().clone(),
    }
}
