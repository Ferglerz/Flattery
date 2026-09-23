use crate::strength::StrengthNode;
use atomic_float::AtomicF32;
use crossbeam_queue::ArrayQueue;
use std::sync::atomic::{AtomicBool, AtomicUsize};
use std::sync::{Arc, RwLock};

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
    boost_nodes: RwLock<Arc<[StrengthNode]>>,
    cut_nodes: RwLock<Arc<[StrengthNode]>>,
    retired_nodes: ArrayQueue<Arc<[StrengthNode]>>,
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
            spectrum_mags_db: RwLock::new(Vec::with_capacity(4096)),
            filter_display: RwLock::new(Vec::with_capacity(1024)),
            boost_nodes: RwLock::new(Arc::from([])),
            cut_nodes: RwLock::new(Arc::from([])),
            retired_nodes: ArrayQueue::new(8),
        }
    }

    pub fn publish_nodes(&self, boost: bool, nodes: Arc<[StrengthNode]>) {
        while self.retired_nodes.pop().is_some() {}
        let lock = if boost {
            &self.boost_nodes
        } else {
            &self.cut_nodes
        };
        match lock.write() {
            Ok(mut current) => *current = nodes,
            Err(poisoned) => *poisoned.into_inner() = nodes,
        }
    }

    pub fn node_snapshot(&self, boost: bool) -> Option<Arc<[StrengthNode]>> {
        let lock = if boost {
            &self.boost_nodes
        } else {
            &self.cut_nodes
        };
        lock.try_read().ok().map(|nodes| Arc::clone(&nodes))
    }

    pub fn refresh_node_snapshot(&self, boost: bool, current: &mut Arc<[StrengthNode]>) {
        if self.retired_nodes.is_full() {
            return;
        }
        let Some(next) = self.node_snapshot(boost) else {
            return;
        };
        if Arc::ptr_eq(current, &next) {
            return;
        }
        let previous = std::mem::replace(current, next);
        if let Err(previous) = self.retired_nodes.push(previous) {
            *current = previous;
        }
    }
}
