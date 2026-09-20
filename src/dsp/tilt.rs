use pleasant_ui::math::{db_to_linear, flattery_freq_to_pos, linear_to_db};

pub const TILT_LOW_MULT: f64 = 0.25;
pub const TILT_HIGH_MULT: f64 = 4.0;

pub fn calculate_tilt_multiplier(freq_hz: f64, tilt_freq_hz: f64, srate: f64) -> f64 {
    let nyquist_hz = srate * 0.5;
    let min_freq = 10.0;
    let max_freq = nyquist_hz;

    if freq_hz <= 0.0 {
        return TILT_LOW_MULT;
    }
    if freq_hz >= nyquist_hz {
        return TILT_HIGH_MULT;
    }

    let freq_pos = flattery_freq_to_pos(freq_hz, min_freq, max_freq);
    let center_pos = flattery_freq_to_pos(tilt_freq_hz, min_freq, max_freq);

    let normalized_pos = freq_pos.clamp(0.0, 1.0);
    let center_normalized = center_pos.clamp(0.0, 1.0);

    let s_curve = normalized_pos * normalized_pos * (3.0 - 2.0 * normalized_pos);
    let ratio_curved = s_curve * s_curve;

    let center_s_curve = center_normalized * center_normalized * (3.0 - 2.0 * center_normalized);
    let center_ratio_curved = center_s_curve * center_s_curve;

    let center_sq = center_ratio_curved * center_ratio_curved;
    let denom = center_sq - center_ratio_curved;

    let tilt_mult = if denom.abs() > 1e-9 {
        let a = (0.75 - 3.75 * center_ratio_curved) / denom;
        let b = 3.75 - a;
        a * ratio_curved * ratio_curved + b * ratio_curved + 0.25
    } else {
        TILT_LOW_MULT + ratio_curved * (TILT_HIGH_MULT - TILT_LOW_MULT)
    };

    tilt_mult.clamp(TILT_LOW_MULT, TILT_HIGH_MULT)
}

pub fn calculate_tilt_multiplier_scaled(
    freq_hz: f64,
    tilt_freq_hz: f64,
    tilt_amount: f64,
    srate: f64,
) -> f64 {
    if tilt_amount.abs() <= 0.001 {
        return 1.0;
    }
    let tilt_mult = calculate_tilt_multiplier(freq_hz, tilt_freq_hz, srate);
    if tilt_amount > 0.0 {
        1.0 + (tilt_mult - 1.0) * tilt_amount
    } else {
        let tilt_mult_inv = 1.0 / tilt_mult;
        1.0 + (tilt_mult_inv - 1.0) * tilt_amount.abs()
    }
}

pub fn apply_tilt_compensation(
    target_gain_linear: f64,
    tilt_mult_scaled: f64,
    tilt_amount: f64,
) -> f64 {
    if tilt_amount.abs() <= 0.001 {
        return target_gain_linear;
    }
    let tilt_mult_db = linear_to_db(tilt_mult_scaled);
    let compensation_db = -tilt_mult_db * tilt_amount.abs() * 0.1;
    target_gain_linear * db_to_linear(compensation_db)
}
