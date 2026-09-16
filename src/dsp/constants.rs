pub const MAX_FFT_SIZE: usize = 8192;
pub const MAX_DISPLAY_BINS: usize = MAX_FFT_SIZE / 2;
pub const MAX_FILTERS: usize = 1024;
pub const FILTER_MIN_HZ: f64 = 20.0;
pub const FILTER_MAX_HZ: f64 = 18000.0;
pub const NOTES_PER_OCTAVE: f64 = 12.0;
pub const MIN_FILTER_NOTE_SPACING: f64 = 0.25;
pub const RATE_OF_CHANGE_THRESHOLD_DB: f64 = 1.0;
