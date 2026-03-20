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

    pub(crate) seek_target: Arc<AtomicU64>,
    pub(crate) seeking: Arc<AtomicBool>,
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

            seek_target: Arc::new(AtomicU64::new(0)),
            seeking: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn volume(&self) -> f32 {
        f32::from_bits(self.volume_bits.load(Ordering::Relaxed))
    }

    pub fn set_volume(&self, v: f32) {
        self.volume_bits.store(v.clamp(0.05, 2.0).to_bits(), Ordering::Relaxed);
    }

    pub fn toggle_pause(&self) {
        let cur = self.paused.load(Ordering::Relaxed);
        self.paused.store(!cur, Ordering::Relaxed);
    }

    pub fn adjust_volume(&self, delta: f32) {
        let current = self.volume();
        self.set_volume(current + delta);
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
        self.paused.store(false, Ordering::Relaxed);

        self.seeking.store(false, Ordering::Relaxed);
        self.seek_target.store(0, Ordering::Relaxed);
    }

    pub fn request_seek(&self, target_samples: u64) {
        self.seek_target.store(target_samples, Ordering::Relaxed);
        self.seeking.store(true, Ordering::Relaxed);
    }

    pub fn take_seek(&self) -> Option<u64> {
        if self.seeking.swap(false, Ordering::Relaxed) {
            Some(self.seek_target.load(Ordering::Relaxed))
        } else {
            None
        }
    }
}