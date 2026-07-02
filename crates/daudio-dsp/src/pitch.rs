//! Windowed monophonic pitch tracking over `pitch-detection` (McLeod method).
//! NOTE: get_pitch runs an FFT and may allocate, so push is only approximately
//! real-time-safe on hop boundaries (acceptable for v1).

use pitch_detection::detector::mcleod::McLeodDetector;
use pitch_detection::detector::PitchDetector;

const WINDOW: usize = 2048;
const PADDING: usize = WINDOW / 2;
pub const HOP: usize = 256;
const POWER_THRESHOLD: f32 = 0.15;
const CLARITY_THRESHOLD: f32 = 0.6;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Detection {
    Pitch(f32),
    NoPitch,
}

/// Synchronous core of the pitch tracker. Holds the (`!Send`, because
/// `McLeodDetector` keeps `Rc`-backed scratch pools) detector and runs the
/// hop-gated windowed detection inline. Never crosses a thread boundary: the
/// threaded `PitchTracker` constructs one on its worker thread.
pub(crate) struct PitchDetectorCore {
    detector: McLeodDetector<f32>,
    ring: Vec<f32>,
    scratch: Vec<f32>,
    write: usize,
    hop_counter: usize,
    sample_rate: usize,
}

impl PitchDetectorCore {
    pub(crate) fn new() -> Self {
        Self {
            detector: McLeodDetector::new(WINDOW, PADDING),
            ring: vec![0.0; WINDOW],
            scratch: vec![0.0; WINDOW],
            write: 0,
            hop_counter: 0,
            sample_rate: 48_000,
        }
    }
    pub(crate) fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate as usize;
    }
    pub(crate) fn reset(&mut self) {
        self.ring.iter_mut().for_each(|s| *s = 0.0);
        self.write = 0;
        self.hop_counter = 0;
    }
    pub(crate) fn push(&mut self, sample: f32) -> Option<Detection> {
        self.ring[self.write] = sample;
        self.write = (self.write + 1) % WINDOW;
        self.hop_counter += 1;
        if self.hop_counter < HOP {
            return None;
        }
        self.hop_counter = 0;
        // Reconstruct the window oldest -> newest. `write` points at the oldest
        // sample (the slot about to be overwritten next).
        let (head, tail) = self.ring.split_at(self.write);
        self.scratch[..tail.len()].copy_from_slice(tail);
        self.scratch[tail.len()..].copy_from_slice(head);
        let pitch = self.detector.get_pitch(
            &self.scratch,
            self.sample_rate,
            POWER_THRESHOLD,
            CLARITY_THRESHOLD,
        );
        Some(match pitch {
            Some(p) => Detection::Pitch(p.frequency),
            None => Detection::NoPitch,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::TAU;
    #[test]
    fn detects_a_sine_frequency() {
        let sr = 44_100.0;
        let freq = 220.0;
        let mut t = PitchDetectorCore::new();
        t.set_sample_rate(sr);
        let mut last = Detection::NoPitch;
        for n in 0..(WINDOW * 4) {
            let s = (TAU * freq * n as f32 / sr).sin();
            if let Some(d) = t.push(s) {
                last = d;
            }
        }
        match last {
            Detection::Pitch(f) => {
                assert!((f - freq).abs() < 3.0, "detected {f}, expected ~{freq}")
            }
            Detection::NoPitch => panic!("expected a pitch"),
        }
    }
    #[test]
    fn silence_is_no_pitch() {
        let mut t = PitchDetectorCore::new();
        t.set_sample_rate(44_100.0);
        let mut got = None;
        for _ in 0..(WINDOW * 2) {
            if let Some(d) = t.push(0.0) {
                got = Some(d);
            }
        }
        assert_eq!(got, Some(Detection::NoPitch));
    }
}
