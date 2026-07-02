//! Lock-free single-value audio→UI channel for meters.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

/// Lock-free peak-level channel: audio thread writes, editor reads. Cheap Clone
/// (shared Arc). RT-safe: single relaxed atomic op each side, no alloc/lock.
#[derive(Clone)]
pub struct PeakLevel(Arc<AtomicU32>);

impl PeakLevel {
    /// Create a new channel initialised to `0.0`.
    pub fn new() -> Self {
        Self(Arc::new(AtomicU32::new(0f32.to_bits())))
    }

    /// Store `value` for the reader (audio-thread side). RT-safe.
    pub fn write(&self, value: f32) {
        self.0.store(value.to_bits(), Ordering::Relaxed);
    }

    /// Load the most recently written value (editor side).
    pub fn read(&self) -> f32 {
        f32::from_bits(self.0.load(Ordering::Relaxed))
    }
}

impl Default for PeakLevel {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_then_read_roundtrips() {
        let p = PeakLevel::new();
        assert_eq!(p.read(), 0.0);
        p.write(0.5);
        assert_eq!(p.read(), 0.5);
        let clone = p.clone();
        p.write(0.25);
        assert_eq!(clone.read(), 0.25);
    }
}
