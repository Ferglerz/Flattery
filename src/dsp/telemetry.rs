use atomic_float::AtomicF32;
use std::sync::atomic::{AtomicBool, AtomicUsize};
use std::sync::RwLock;

pub struct Shared {
    pub sample_rate: AtomicF32,
    pub fft_size: AtomicUsize,
    pub is_bypassed: AtomicBool,
    pub low_cut_hz: AtomicF32,
    pub high_cut_hz: AtomicF32,
    pub tilt: AtomicF32,
    pub tilt_freq_hz: AtomicF32,
    pub max_boost_db: AtomicF32,
    pub max_cut_db: AtomicF32,
    pub output_gain_db: AtomicF32,
    pub spectrum_mags_db: RwLock<Vec<f32>>,
    pub filter_display: RwLock<Vec<(f32, f32)>>, // (center_hz, gain_db)
}

impl Default for Shared {
    fn default() -> Self {
        Self::new()
    }
}

impl Shared {
    pub fn new() -> Self {
        Self {
            sample_rate: AtomicF32::new(44100.0),
            fft_size: AtomicUsize::new(512),
            is_bypassed: AtomicBool::new(false),
            low_cut_hz: AtomicF32::new(20.0),
            high_cut_hz: AtomicF32::new(20000.0),
            tilt: AtomicF32::new(0.0),
            tilt_freq_hz: AtomicF32::new(1800.0),
            max_boost_db: AtomicF32::new(12.0),
            max_cut_db: AtomicF32::new(12.0),
            output_gain_db: AtomicF32::new(0.0),
            spectrum_mags_db: RwLock::new(Vec::with_capacity(2048)),
            filter_display: RwLock::new(Vec::with_capacity(1024)),
        }
    }
}
