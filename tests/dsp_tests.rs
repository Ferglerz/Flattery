use flattery::{
    dsp::{
        biquad::PeakingFilter,
        filter_bank::FilterBank,
        ring_buffer::{AnalysisRing, DelayLine},
        tilt::{apply_tilt_compensation, calculate_tilt_multiplier, calculate_tilt_multiplier_scaled},
        Engine, Shared,
    },
    params::FlatteryParams,
};
use pleasant_ui::math::{flattery_freq_to_pos, flattery_pos_to_freq};
use std::sync::Arc;

#[test]
fn test_delay_line() {
    let mut dl = DelayLine::new();
    let delay = 256;
    for i in 0..1000 {
        let sample = i as f64;
        let out = dl.write_and_read(sample, delay);
        if i >= delay {
            assert_eq!(out, (i - delay) as f64);
        }
    }
}

#[test]
fn test_analysis_ring() {
    let mut ring = AnalysisRing::new();
    for i in 0..100 {
        ring.push(i as f64);
    }
    let mut window = vec![0.0; 10];
    ring.read_window(10, &mut window);
    for (idx, &val) in window.iter().enumerate() {
        assert_eq!(val, (90 + idx) as f64);
    }
}

#[test]
fn test_frequency_coordinate_mapping_roundtrip() {
    let min_f = 10.0;
    let max_f = 22050.0;
    for &f in &[20.0, 100.0, 500.0, 1000.0, 1800.0, 5000.0, 10000.0, 20000.0] {
        let pos = flattery_freq_to_pos(f, min_f, max_f);
        assert!((0.0..=1.0).contains(&pos));
        let recovered = flattery_pos_to_freq(pos, min_f, max_f);
        let diff = (recovered - f).abs();
        assert!(
            diff < 0.1,
            "Failed roundtrip for freq {f}: got {recovered} (diff: {diff})"
        );
    }
}

#[test]
fn test_tilt_multiplier_curve() {
    let srate = 44100.0;
    let center_f = 1800.0;

    // Multiplier at 0 Hz should be ~0.25
    let m_low = calculate_tilt_multiplier(0.0, center_f, srate);
    assert!((m_low - 0.25).abs() < 0.01);

    // Multiplier at center frequency should be ~1.0
    let m_center = calculate_tilt_multiplier(center_f, center_f, srate);
    assert!(
        (m_center - 1.0).abs() < 0.05,
        "Center multiplier should be ~1.0, got {m_center}"
    );

    // Multiplier at Nyquist should be ~4.0
    let m_nyq = calculate_tilt_multiplier(srate * 0.5, center_f, srate);
    assert!((m_nyq - 4.0).abs() < 0.01);

    // Scaled with 0% tilt should be 1.0 everywhere
    let scaled_0 = calculate_tilt_multiplier_scaled(100.0, center_f, 0.0, srate);
    assert_eq!(scaled_0, 1.0);

    // Compensation with 0% tilt should be identity
    let comp_0 = apply_tilt_compensation(2.0, 1.0, 0.0);
    assert_eq!(comp_0, 2.0);
}

#[test]
fn test_filter_bank_unity_transparency() {
    let mut fb = FilterBank::new();
    fb.init_frequencies(512, 44100.0);
    assert!(fb.all_unity);

    let in_l = 0.707;
    let in_r = -0.5;
    let (out_l, out_r) = fb.process(in_l, in_r);
    assert_eq!(out_l, in_l);
    assert_eq!(out_r, in_r);
}

#[test]
fn test_peaking_filter_transparency() {
    let mut filter = PeakingFilter::new(1000.0);
    filter.update_coeffs(44100.0, 10.0);
    let (diff_l, diff_r) = filter.process_diff(0.5, 0.5);
    assert_eq!(diff_l, 0.0);
    assert_eq!(diff_r, 0.0);
}

#[test]
fn test_engine_audio_stream_no_nans() {
    let shared = Arc::new(Shared::new());
    let mut engine = Engine::new(shared, 44100.0);
    let params = FlatteryParams::default();

    // Send impulse followed by silence
    for i in 0..1024 {
        let input = if i == 0 { 1.0 } else { 0.0 };
        let (out_l, out_r) = engine.tick(input, input, &params);
        assert!(out_l.is_finite(), "Sample {i} produced non-finite L: {out_l}");
        assert!(out_r.is_finite(), "Sample {i} produced non-finite R: {out_r}");
    }
}
