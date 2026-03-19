use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU32, Ordering},
};

#[derive(Clone)]
pub struct AudioControl {
    pub(crate) volume_bits: Arc<AtomicU32>,
    pub(crate) paused: Arc<AtomicBool>,
}

impl AudioControl {
    pub fn new() -> Self {
        Self {
            volume_bits: Arc::new(AtomicU32::new(1.0f32.to_bits())),
            paused: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn volume(&self) -> f32 {
        f32::from_bits(self.volume_bits.load(Ordering::Relaxed))
    }

    pub fn toggle_pause(&self) {
        let cur = self.paused.load(Ordering::Relaxed);
        self.paused.store(!cur, Ordering::Relaxed);
    }

    pub fn adjust_volume(&self, delta: f32) {
        let current = self.volume();
        let new = (current + delta).clamp(0.05, 2.0);
        self.volume_bits.store(new.to_bits(), Ordering::Relaxed);
    }
}