use crate::dsp::constants::MAX_DISPLAY_BINS;
use pleasant_dsp::units::{db_to_linear, linear_to_db};

pub struct LevelingProcessor {
    median_buf: Vec<f64>,
    pub smoothed_gain_db_l: Vec<f64>,
    pub smoothed_gain_db_r: Vec<f64>,
    pub smoothed_gain_db_link: Vec<f64>,
}

impl Default for LevelingProcessor {
    fn default() -> Self {
        Self::new()
    }
}

impl LevelingProcessor {
    pub fn new() -> Self {
        Self {
            median_buf: Vec::with_capacity(32),
            smoothed_gain_db_l: vec![0.0; MAX_DISPLAY_BINS],
            smoothed_gain_db_r: vec![0.0; MAX_DISPLAY_BINS],
            smoothed_gain_db_link: vec![0.0; MAX_DISPLAY_BINS],
        }
    }

    pub fn reset(&mut self) {
        self.smoothed_gain_db_l.fill(0.0);
        self.smoothed_gain_db_r.fill(0.0);
        self.smoothed_gain_db_link.fill(0.0);
    }

    fn median(&mut self) -> f64 {
        let len = self.median_buf.len();
        if len == 0 {
            return 0.0;
        }
        self.median_buf
            .sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        self.median_buf[len / 2]
    }

    fn collect_neighbor_median_target(&mut self, mags: &[f64], bin: usize, radius: usize) -> f64 {
        let n = mags.len();
        let r = radius.clamp(1, 12);
        self.median_buf.clear();

        for d in -(r as isize)..=(r as isize) {
            if d != 0 {
                let idx = (bin as isize + d).clamp(0, n as isize - 1) as usize;
                self.median_buf.push(linear_to_db(mags[idx]));
            }
        }
        let med_db = self.median();
        db_to_linear(med_db)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn process(
        &mut self,
        mag_l: &[f64],
        mag_r: &[f64],
        pos_bin_count: usize,
        boost_radii: &[usize],
        cut_radii: &[usize],
        amplify_mode: bool,
        stereo_link_pct: f64,
        strength_boost: f64,
        strength_cut: f64,
        max_boost_db: f64,
        max_cut_db: f64,
        low_cut_bin: usize,
        high_cut_bin: usize,
        min_operate_lin: f64,
        max_operate_lin: f64,
        attack_ms: f64,
        release_ms: f64,
        frame_dt: f64,
        boost_weights: &[f64],
        cut_weights: &[f64],
    ) {
        let pos_bin_count = pos_bin_count.min(MAX_DISPLAY_BINS);
        let link_factor = (stereo_link_pct * 0.01).clamp(0.0, 1.0);
        let att_coeff = (-frame_dt / (attack_ms * 0.001).max(1e-6)).exp();
        let rel_coeff = (-frame_dt / (release_ms * 0.001).max(1e-6)).exp();

        let boost_factor = strength_boost * 0.01;
        let cut_factor = strength_cut * 0.01;

        let start = low_cut_bin.clamp(1, pos_bin_count.saturating_sub(1));
        let end = high_cut_bin.clamp(start, pos_bin_count.saturating_sub(1));

        for k in 0..pos_bin_count {
            if k < start || k > end {
                // Decay to 0 dB
                self.smoothed_gain_db_l[k] +=
                    (0.0 - self.smoothed_gain_db_l[k]) * (1.0 - rel_coeff);
                self.smoothed_gain_db_r[k] +=
                    (0.0 - self.smoothed_gain_db_r[k]) * (1.0 - rel_coeff);
                self.smoothed_gain_db_link[k] +=
                    (0.0 - self.smoothed_gain_db_link[k]) * (1.0 - rel_coeff);
                continue;
            }

            let ml = mag_l[k];
            let mr = mag_r[k];
            let m_avg = 0.5 * (ml + mr);

            let active_l = ml >= min_operate_lin && ml <= max_operate_lin;
            let active_r = mr >= min_operate_lin && mr <= max_operate_lin;
            let active_avg = m_avg >= min_operate_lin && m_avg <= max_operate_lin;

            let r_b = boost_radii.get(k).copied().unwrap_or(1).clamp(1, 12);
            let r_c = cut_radii.get(k).copied().unwrap_or(1).clamp(1, 12);

            let (target_l, target_r) = if r_b == r_c {
                (
                    self.collect_neighbor_median_target(mag_l, k, r_b),
                    self.collect_neighbor_median_target(mag_r, k, r_b),
                )
            } else {
                let tb_l = self.collect_neighbor_median_target(mag_l, k, r_b);
                let tc_l = self.collect_neighbor_median_target(mag_l, k, r_c);
                let tl = if tb_l > ml {
                    tb_l
                } else if tc_l < ml {
                    tc_l
                } else {
                    ml
                };

                let tb_r = self.collect_neighbor_median_target(mag_r, k, r_b);
                let tc_r = self.collect_neighbor_median_target(mag_r, k, r_c);
                let tr = if tb_r > mr {
                    tb_r
                } else if tc_r < mr {
                    tc_r
                } else {
                    mr
                };
                (tl, tr)
            };

            let mut delta_l = if active_l {
                linear_to_db(target_l / (ml + 1e-9))
            } else {
                0.0
            };
            let mut delta_r = if active_r {
                linear_to_db(target_r / (mr + 1e-9))
            } else {
                0.0
            };

            // Linked delta
            let target_link = 0.5 * (target_l + target_r);
            let mut delta_link = if active_avg {
                linear_to_db(target_link / (m_avg + 1e-9))
            } else {
                0.0
            };

            if amplify_mode {
                delta_l = -delta_l;
                delta_r = -delta_r;
                delta_link = -delta_link;
            }

            // 0.5 dB deadzone threshold
            if delta_l.abs() < 0.5 {
                delta_l = 0.0;
            }
            if delta_r.abs() < 0.5 {
                delta_r = 0.0;
            }
            if delta_link.abs() < 0.5 {
                delta_link = 0.0;
            }

            // Stereo link blend
            let eff_delta_l = delta_l * (1.0 - link_factor) + delta_link * link_factor;
            let eff_delta_r = delta_r * (1.0 - link_factor) + delta_link * link_factor;

            let boost_w = boost_weights.get(k).copied().unwrap_or(1.0).clamp(0.0, 8.0);
            let cut_w = cut_weights.get(k).copied().unwrap_or(1.0).clamp(0.0, 8.0);
            let scale_l = if eff_delta_l > 0.0 {
                boost_factor * boost_w
            } else {
                cut_factor * cut_w
            };
            let scale_r = if eff_delta_r > 0.0 {
                boost_factor * boost_w
            } else {
                cut_factor * cut_w
            };
            let scale_link = if delta_link > 0.0 {
                boost_factor * boost_w
            } else {
                cut_factor * cut_w
            };

            let target_gain_l = (eff_delta_l * scale_l).clamp(-max_cut_db, max_boost_db);
            let target_gain_r = (eff_delta_r * scale_r).clamp(-max_cut_db, max_boost_db);
            let target_gain_link = (delta_link * scale_link).clamp(-max_cut_db, max_boost_db);

            // Smooth gains
            let c_l = if target_gain_l > self.smoothed_gain_db_l[k] {
                att_coeff
            } else {
                rel_coeff
            };
            let c_r = if target_gain_r > self.smoothed_gain_db_r[k] {
                att_coeff
            } else {
                rel_coeff
            };
            let c_link = if target_gain_link > self.smoothed_gain_db_link[k] {
                att_coeff
            } else {
                rel_coeff
            };

            self.smoothed_gain_db_l[k] +=
                (target_gain_l - self.smoothed_gain_db_l[k]) * (1.0 - c_l);
            self.smoothed_gain_db_r[k] +=
                (target_gain_r - self.smoothed_gain_db_r[k]) * (1.0 - c_r);
            self.smoothed_gain_db_link[k] +=
                (target_gain_link - self.smoothed_gain_db_link[k]) * (1.0 - c_link);
        }
    }
}
