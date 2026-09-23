use crate::dsp::{
    biquad::PeakingFilter,
    constants::{
        FILTER_MAX_HZ, FILTER_MIN_HZ, MAX_FILTERS, MIN_FILTER_NOTE_SPACING, NOTES_PER_OCTAVE,
        RATE_OF_CHANGE_THRESHOLD_DB,
    },
};
use pleasant_dsp::units::{db_to_linear, linear_to_db};

pub struct FilterBank {
    pub filters: Vec<PeakingFilter>,
    pub q: f64,
    pub all_unity: bool,
    active_start: usize,
    active_end: usize,
}

impl Default for FilterBank {
    fn default() -> Self {
        Self::new()
    }
}

impl FilterBank {
    pub fn new() -> Self {
        Self {
            filters: Vec::with_capacity(MAX_FILTERS),
            q: 10.0,
            all_unity: true,
            active_start: 0,
            active_end: 0,
        }
    }

    pub fn reset(&mut self) {
        for f in &mut self.filters {
            f.reset();
        }
        self.all_unity = true;
    }

    pub fn init_frequencies(&mut self, fft_size: usize, srate: f64) {
        self.filters.clear();
        let pos_bin_count = fft_size / 2;
        let bin_hz = srate / fft_size as f64;
        let min_ratio = 2.0_f64.powf(MIN_FILTER_NOTE_SPACING / NOTES_PER_OCTAVE);
        let mut last_kept_hz = 0.0;

        for i in 0..pos_bin_count {
            let bin_freq = (i as f64 + 0.5) * bin_hz;
            if (FILTER_MIN_HZ..=FILTER_MAX_HZ).contains(&bin_freq)
                && (last_kept_hz <= 0.0 || bin_freq >= last_kept_hz * min_ratio)
            {
                self.filters.push(PeakingFilter::new(bin_freq));
                last_kept_hz = bin_freq;
                if self.filters.len() >= MAX_FILTERS {
                    break;
                }
            }
        }
        self.active_start = 0;
        self.active_end = self.filters.len();
        self.all_unity = true;
    }

    pub fn update_active_range(&mut self, low_cut_hz: f64, high_cut_hz: f64) {
        let count = self.filters.len();
        let mut start = 0;
        for (i, f) in self.filters.iter().enumerate() {
            if f.center_hz >= low_cut_hz {
                start = i;
                break;
            }
        }
        let mut end = count;
        for i in (0..count).rev() {
            if self.filters[i].center_hz <= high_cut_hz {
                end = i + 1;
                break;
            }
        }
        self.active_start = start.min(count);
        self.active_end = end.max(self.active_start).min(count);
    }

    pub fn set_filter_gains(
        &mut self,
        target_gains: &[f64],
        srate: f64,
        low_cut_hz: f64,
        high_cut_hz: f64,
    ) {
        self.update_active_range(low_cut_hz, high_cut_hz);
        let mut all_unity = true;

        for (i, filter) in self.filters.iter_mut().enumerate() {
            if i >= self.active_start && i < self.active_end && i < target_gains.len() {
                let target = target_gains[i].clamp(0.01, 1000.0);
                let prev = filter.prev_gain_linear;

                // Rate of change scaling
                let target_db = linear_to_db(target);
                let prev_db = linear_to_db(prev);
                let delta_db = target_db - prev_db;
                let delta_abs = delta_db.abs();

                let scaled_gain = if delta_abs > 0.001 {
                    let scale_factor = 1.0 - (-delta_abs / RATE_OF_CHANGE_THRESHOLD_DB).exp();
                    let scaled_delta_db = delta_db * scale_factor;
                    db_to_linear(prev_db + scaled_delta_db)
                } else {
                    prev
                };

                filter.gain_linear = scaled_gain;
                filter.prev_gain_linear = scaled_gain;
            } else {
                filter.prev_gain_linear = filter.gain_linear;
                filter.gain_linear = 1.0;
            }

            if (filter.gain_linear - 1.0).abs() >= 0.001 {
                all_unity = false;
            }
            filter.update_coeffs(srate, self.q);
        }

        self.all_unity = all_unity;
    }

    #[inline(always)]
    pub fn process(&mut self, in_l: f64, in_r: f64) -> (f64, f64) {
        if self.all_unity {
            return (in_l, in_r);
        }
        let mut sum_l = 0.0;
        let mut sum_r = 0.0;

        for i in self.active_start..self.active_end {
            let (diff_l, diff_r) = self.filters[i].process_diff(in_l, in_r);
            sum_l += diff_l;
            sum_r += diff_r;
        }

        (in_l + sum_l, in_r + sum_r)
    }
}
