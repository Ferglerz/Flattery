use crate::dsp::constants::MAX_FFT_SIZE;

pub struct DelayLine {
    buffer: Vec<f64>,
    write_pos: usize,
    capacity: usize,
}

impl Default for DelayLine {
    fn default() -> Self {
        Self::new()
    }
}

impl DelayLine {
    pub fn new() -> Self {
        Self {
            buffer: vec![0.0; MAX_FFT_SIZE],
            write_pos: 0,
            capacity: MAX_FFT_SIZE,
        }
    }

    pub fn reset(&mut self) {
        self.buffer.fill(0.0);
        self.write_pos = 0;
    }

    pub fn write_and_read(&mut self, sample: f64, delay: usize) -> f64 {
        self.buffer[self.write_pos] = sample;
        let delay_clamped = delay.min(self.capacity - 1);
        let read_pos = (self.write_pos + self.capacity - delay_clamped) % self.capacity;
        self.write_pos = (self.write_pos + 1) % self.capacity;
        self.buffer[read_pos]
    }
}

pub struct AnalysisRing {
    buffer: Vec<f64>,
    write_pos: usize,
    capacity: usize,
}

impl Default for AnalysisRing {
    fn default() -> Self {
        Self::new()
    }
}

impl AnalysisRing {
    pub fn new() -> Self {
        Self {
            buffer: vec![0.0; MAX_FFT_SIZE],
            write_pos: 0,
            capacity: MAX_FFT_SIZE,
        }
    }

    pub fn reset(&mut self) {
        self.buffer.fill(0.0);
        self.write_pos = 0;
    }

    pub fn push(&mut self, sample: f64) {
        self.buffer[self.write_pos] = sample;
        self.write_pos = (self.write_pos + 1) % self.capacity;
    }

    /// Read `fft_size` oldest samples in chronological order starting at `(write_pos + capacity - fft_size) % capacity`
    pub fn read_window(&self, fft_size: usize, out: &mut [f64]) {
        let start = (self.write_pos + self.capacity - fft_size) % self.capacity;
        for (i, item) in out.iter_mut().enumerate().take(fft_size) {
            *item = self.buffer[(start + i) % self.capacity];
        }
    }
}
