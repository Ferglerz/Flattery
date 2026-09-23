use flattery::{
    dsp::{
        biquad::PeakingFilter,
        filter_bank::FilterBank,
        ring_buffer::{AnalysisRing, DelayLine},
        tilt::{
            apply_tilt_compensation, calculate_tilt_multiplier, calculate_tilt_multiplier_scaled,
        },
        Engine, Shared,
    },
    params::FlatteryParams,
};
use nih_plug::prelude::Params;
use pleasant_dsp::axis::{flattery_freq_to_pos, flattery_pos_to_freq};
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
    let settings = params.process_settings();

    // Send impulse followed by silence
    for i in 0..1024 {
        let input = if i == 0 { 1.0 } else { 0.0 };
        let (out_l, out_r) = engine.tick(input, input, &settings);
        assert!(
            out_l.is_finite(),
            "Sample {i} produced non-finite L: {out_l}"
        );
        assert!(
            out_r.is_finite(),
            "Sample {i} produced non-finite R: {out_r}"
        );
    }
}

#[test]
fn strength_nodes_round_trip_in_host_state() {
    use flattery::strength::StrengthNode;

    let params = FlatteryParams::default();
    params
        .boost_nodes
        .lock()
        .unwrap()
        .push(StrengthNode::new(3, 1234.0, 0.4));
    params
        .cut_nodes
        .lock()
        .unwrap()
        .push(StrengthNode::new(7, 8000.0, 0.1));
    let fields = params.serialize_fields();
    let restored = FlatteryParams::default();
    restored.deserialize_fields(&fields);
    assert_eq!(
        *params.boost_nodes.lock().unwrap(),
        *restored.boost_nodes.lock().unwrap()
    );
    assert_eq!(
        *params.cut_nodes.lock().unwrap(),
        *restored.cut_nodes.lock().unwrap()
    );
}

#[test]
fn test_quantize_time_ms_stepping() {
    use flattery::ui::quantize_time_ms;

    // Under 10ms: 0.1ms increments
    assert_eq!(quantize_time_ms(0.12), 0.1);
    assert_eq!(quantize_time_ms(1.56), 1.6);
    assert_eq!(quantize_time_ms(9.94), 9.9);
    assert_eq!(quantize_time_ms(10.0), 10.0);

    // Over 10ms (10-25ms): 1ms increments
    assert_eq!(quantize_time_ms(10.4), 10.0);
    assert_eq!(quantize_time_ms(10.6), 11.0);
    assert_eq!(quantize_time_ms(24.8), 25.0);

    // Over 25ms (25-50ms): 5ms increments
    assert_eq!(quantize_time_ms(26.0), 25.0);
    assert_eq!(quantize_time_ms(28.0), 30.0);
    assert_eq!(quantize_time_ms(47.0), 45.0);
    assert_eq!(quantize_time_ms(48.0), 50.0);

    // Over 50ms (50-100ms): 10ms increments
    assert_eq!(quantize_time_ms(54.0), 50.0);
    assert_eq!(quantize_time_ms(56.0), 60.0);
    assert_eq!(quantize_time_ms(94.0), 90.0);
    assert_eq!(quantize_time_ms(96.0), 100.0);

    // Over 100ms (100-200ms): 25ms increments
    assert_eq!(quantize_time_ms(110.0), 100.0);
    assert_eq!(quantize_time_ms(115.0), 125.0);
    assert_eq!(quantize_time_ms(190.0), 200.0);

    // Over 200ms (200-500ms): 50ms increments
    assert_eq!(quantize_time_ms(220.0), 200.0);
    assert_eq!(quantize_time_ms(230.0), 250.0);
    assert_eq!(quantize_time_ms(480.0), 500.0);

    // Over 500ms: 100ms increments
    assert_eq!(quantize_time_ms(540.0), 500.0);
    assert_eq!(quantize_time_ms(560.0), 600.0);
    assert_eq!(quantize_time_ms(1980.0), 2000.0);
}

#[test]
fn test_per_node_radius_interpolation() {
    use flattery::strength::{fill_bin_radii, radius_at, StrengthNode};

    // 0 nodes -> default radius everywhere
    let nodes: Vec<StrengthNode> = Vec::new();
    assert_eq!(radius_at(&nodes, 100.0, 3), 3.0);
    assert_eq!(radius_at(&nodes, 5000.0, 3), 3.0);

    // 1 node -> holds that node's radius everywhere
    let mut n1 = StrengthNode::new(1, 1000.0, 1.0);
    n1.radius = 6;
    let single = vec![n1];
    assert_eq!(radius_at(&single, 100.0, 1), 6.0);
    assert_eq!(radius_at(&single, 1000.0, 1), 6.0);
    assert_eq!(radius_at(&single, 10000.0, 1), 6.0);

    // 2 nodes -> 200 Hz (radius 2) and 2000 Hz (radius 8)
    let mut n_low = StrengthNode::new(1, 200.0, 1.0);
    n_low.radius = 2;
    let mut n_high = StrengthNode::new(2, 2000.0, 1.0);
    n_high.radius = 8;
    let two_nodes = vec![n_low, n_high];

    // Clamped below lowest and above highest
    assert_eq!(radius_at(&two_nodes, 50.0, 1), 2.0);
    assert_eq!(radius_at(&two_nodes, 200.0, 1), 2.0);
    assert_eq!(radius_at(&two_nodes, 2000.0, 1), 8.0);
    assert_eq!(radius_at(&two_nodes, 15000.0, 1), 8.0);

    // Midpoint in log-frequency space: sqrt(200 * 2000) = ~632.45 Hz
    let mid_f = (200.0_f64 * 2000.0).sqrt();
    let r_mid = radius_at(&two_nodes, mid_f, 1);
    // Smoothstep at t = 0.5 is 0.5 * 0.5 * (3 - 2 * 0.5) = 0.5. Radius is 2 + (8 - 2) * 0.5 = 5.0.
    assert!(
        (r_mid - 5.0).abs() < 1e-6,
        "Midpoint radius should be 5.0, got {r_mid}"
    );

    // Test fill_bin_radii
    let mut radii = vec![0; 8];
    fill_bin_radii(&two_nodes, 8, 500.0, 1, &mut radii);
    for &r in &radii {
        assert!((2..=8).contains(&r));
    }
}
