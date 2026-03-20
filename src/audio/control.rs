use std::sync::atomic::AtomicU64;
use std::sync::{atomic::{AtomicBool, AtomicU32, Ordering}, Arc};

#[derive(Clone)]
pub struct AudioControl {
    pub(crate) volume_bits: Arc<AtomicU32>,
    pub(crate) paused: Arc<AtomicBool>,
    pub(crate) started: Arc<AtomicBool>,
    pub(crate) elapsed_samples: Arc<AtomicU64>,
    pub(crate) total_samples: Arc<AtomicU64>,
    pub(crate) sample_rate: Arc<AtomicU32>,
}

impl AudioControl {
    pub fn new() -> Self {
        Self {
            volume_bits: Arc::new(AtomicU32::new(1.0f32.to_bits())),
            paused: Arc::new(AtomicBool::new(false)),
            started: Arc::new(AtomicBool::new(false)),
            elapsed_samples: Arc::new(AtomicU64::new(0)),
            total_samples: Arc::new(AtomicU64::new(0)),
            sample_rate: Arc::new(AtomicU32::new(48000)),
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

    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::Relaxed)
    }

    pub fn start(&self) {
        self.started.store(true, Ordering::Relaxed);
    }

    pub fn is_started(&self) -> bool {
        self.started.load(Ordering::Relaxed)
    }

    pub fn elapsed(&self) -> u64 {
        self.elapsed_samples.load(Ordering::Relaxed)
    }

    pub fn set_elapsed(&self, samples: u64) {
        self.elapsed_samples.store(samples, Ordering::Relaxed)
    }

    // pub fn reset_elapsed(&self) {
    //     self.elapsed_samples.store(0, Ordering::Relaxed);
    // }

    pub fn set_total_samples(&self, total: u64) {
        self.total_samples.store(total, Ordering::Relaxed);
    }

    pub fn set_sample_rate(&self, rate: u32) {
        self.sample_rate.store(rate, Ordering::Relaxed);
    }

    pub fn total(&self) -> u64 {
        self.total_samples.load(Ordering::Relaxed)
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate.load(Ordering::Relaxed)
    }

    pub fn reset_for_new_track(&self) {
        self.elapsed_samples.store(0, Ordering::Relaxed);
        self.total_samples.store(0, Ordering::Relaxed);

        // penting: jangan reset started
        // karena engine butuh tetap jalan

        // reset pause supaya konsisten
        self.paused.store(false, Ordering::Relaxed);
    }
}