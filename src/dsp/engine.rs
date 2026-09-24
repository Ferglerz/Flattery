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
    strength::{fill_bin_radii, fill_bin_weights, StrengthNode},
};
use pleasant_dsp::{
    spectrum::{peak_hold, spectrum_fall_db},
    units::{db_to_linear, linear_to_db},
};
use std::sync::{atomic::Ordering, Arc};

const FFT_SIZES: [usize; 7] = [128, 256, 512, 1024, 2048, 4096, 8192];

#[derive(Clone, Copy, Debug)]
pub struct EngineSettings {
    pub fft_size: usize,
    pub bypassed: bool,
    pub low_cut_hz: f64,
    pub high_cut_hz: f64,
    pub tilt: f64,
    pub tilt_frequency_hz: f64,
    pub max_boost_db: f64,
    pub max_cut_db: f64,
    pub output_gain_db: f64,
    pub mid_side: bool,
    pub input_rms_ms: f64,
    pub minimum_operating_db: f64,
    pub maximum_operating_db: f64,
    pub boost_strength: f64,
    pub cut_strength: f64,
    pub stereo_link: f64,
    pub neighbor_radius: usize,
    pub amplify: bool,
    pub attack_ms: f64,
    pub release_ms: f64,
}

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
    prepared_analyzers: Vec<FftAnalyzer>,
    leveler: LevelingProcessor,
    filter_bank: FilterBank,
    window_samples_l: Vec<f64>,
    window_samples_r: Vec<f64>,
    target_gains: Vec<f64>,
    boost_weights: Vec<f64>,
    cut_weights: Vec<f64>,
    boost_radii: Vec<usize>,
    cut_radii: Vec<usize>,
    boost_nodes: Arc<[StrengthNode]>,
    cut_nodes: Arc<[StrengthNode]>,
    last_output_gain_db: f64,
    output_gain_linear: f64,
}

impl Engine {
    pub fn new(shared: Arc<Shared>, sample_rate: f64) -> Self {
        let fft_size = 512;
        let hop_size = fft_size / 2;
        let analysis_delay = hop_size;

        let mut filter_bank = FilterBank::new();
        filter_bank.init_frequencies(fft_size, sample_rate);
        let prepared_analyzers = FFT_SIZES
            .into_iter()
            .filter(|size| *size != fft_size)
            .map(FftAnalyzer::new)
            .collect();
        let boost_nodes = shared.node_snapshot(true).unwrap_or_else(|| Arc::from([]));
        let cut_nodes = shared.node_snapshot(false).unwrap_or_else(|| Arc::from([]));

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
            prepared_analyzers,
            leveler: LevelingProcessor::new(),
            filter_bank,
            window_samples_l: vec![0.0; MAX_FFT_SIZE],
            window_samples_r: vec![0.0; MAX_FFT_SIZE],
            target_gains: vec![1.0; MAX_FFT_SIZE / 2],
            boost_weights: vec![1.0; MAX_FFT_SIZE / 2],
            cut_weights: vec![1.0; MAX_FFT_SIZE / 2],
            boost_radii: vec![1; MAX_FFT_SIZE / 2],
            cut_radii: vec![1; MAX_FFT_SIZE / 2],
            boost_nodes,
            cut_nodes,
            last_output_gain_db: 0.0,
            output_gain_linear: 1.0,
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
        if let Ok(mut lock) = self.shared.spectrum_mags_db.try_write() {
            lock.fill(-120.0);
        }
    }

    pub fn latency(&self) -> u32 {
        self.analysis_delay as u32
    }

    pub fn set_fft_size(&mut self, new_size: usize) {
        if self.fft_size == new_size {
            return;
        }
        let Some(index) = self
            .prepared_analyzers
            .iter()
            .position(|analyzer| analyzer.fft_size == new_size)
        else {
            return;
        };
        std::mem::swap(&mut self.analyzer, &mut self.prepared_analyzers[index]);
        self.analyzer.reset();
        self.fft_size = new_size;
        self.hop_size = new_size / 2;
        self.analysis_delay = self.hop_size;
        self.hop_counter = 0;
        self.filter_bank
            .init_frequencies(new_size, self.sample_rate);
        self.shared.fft_size.store(new_size, Ordering::Relaxed);
    }

    pub fn set_sample_rate(&mut self, srate: f64) {
        self.sample_rate = srate;
        self.filter_bank.init_frequencies(self.fft_size, srate);
        self.shared
            .sample_rate
            .store(srate as f32, Ordering::Relaxed);
    }

    pub fn tick(&mut self, in_l: f64, in_r: f64, settings: &EngineSettings) -> (f64, f64) {
        if settings.fft_size != self.fft_size {
            self.set_fft_size(settings.fft_size);
        }

        let bypassed = settings.bypassed;
        self.shared.is_bypassed.store(bypassed, Ordering::Relaxed);

        let low_cut_hz = settings.low_cut_hz;
        let high_cut_hz = settings.high_cut_hz;
        let tilt = settings.tilt;
        let tilt_freq_hz = settings.tilt_frequency_hz;
        let max_boost_db = settings.max_boost_db;
        let max_cut_db = settings.max_cut_db;
        let output_gain_db = settings.output_gain_db;

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

            self.ring_l.read_window(n, &mut self.window_samples_l[..n]);
            self.ring_r.read_window(n, &mut self.window_samples_r[..n]);

            let is_ms = settings.mid_side;
            let input_rms_ms = settings.input_rms_ms;

            self.analyzer.analyze(
                &self.window_samples_l[..n],
                &self.window_samples_r[..n],
                is_ms,
                input_rms_ms,
                frame_dt,
            );

            let low_cut_bin = (low_cut_hz / bin_hz.max(1e-6)).floor() as usize;
            let high_cut_bin = (high_cut_hz / bin_hz.max(1e-6)).floor() as usize;

            let min_operate_lin = db_to_linear(settings.minimum_operating_db);
            let max_operate_lin = db_to_linear(settings.maximum_operating_db);

            let str_boost = settings.boost_strength;
            let str_cut = settings.cut_strength;
            let stereo_link = settings.stereo_link;
            let default_radius = settings.neighbor_radius;
            let amplify = settings.amplify;
            let att_ms = settings.attack_ms;
            let rel_ms = settings.release_ms;

            self.shared
                .refresh_node_snapshot(true, &mut self.boost_nodes);
            self.shared
                .refresh_node_snapshot(false, &mut self.cut_nodes);
            fill_bin_weights(
                &self.boost_nodes,
                half,
                bin_hz,
                10.0,
                22050.0,
                &mut self.boost_weights,
            );
            fill_bin_weights(
                &self.cut_nodes,
                half,
                bin_hz,
                10.0,
                22050.0,
                &mut self.cut_weights,
            );
            fill_bin_radii(
                &self.boost_nodes,
                half,
                bin_hz,
                default_radius,
                &mut self.boost_radii,
            );
            fill_bin_radii(
                &self.cut_nodes,
                half,
                bin_hz,
                default_radius,
                &mut self.cut_radii,
            );

            self.leveler.process(
                &self.analyzer.mag_l,
                &self.analyzer.mag_r,
                half,
                &self.boost_radii[..half],
                &self.cut_radii[..half],
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

            // Update UI telemetry lock-free. Peak-hold matches Damian Channel Strip.
            if let Ok(mut lock) = self.shared.spectrum_mags_db.try_write() {
                if lock.len() != half {
                    lock.clear();
                    lock.resize(half, -120.0);
                }
                let fall = spectrum_fall_db(self.hop_size as f32);
                for k in 0..half {
                    let avg = 0.5 * (self.analyzer.mag_l[k] + self.analyzer.mag_r[k]);
                    let db = linear_to_db(avg) as f32;
                    lock[k] = peak_hold(lock[k], db, fall);
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
        if output_gain_db != self.last_output_gain_db {
            self.output_gain_linear = db_to_linear(output_gain_db);
            self.last_output_gain_db = output_gain_db;
        }
        (
            wet_l * self.output_gain_linear,
            wet_r * self.output_gain_linear,
        )
    }
}
