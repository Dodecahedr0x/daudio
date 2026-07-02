//! Windowed monophonic pitch tracking over `pitch-detection` (McLeod method).
//! Detection (`get_pitch` runs an FFT and may allocate) happens on a worker
//! thread; the audio thread only pushes samples into a lock-free ring and reads
//! the latest published frequency via an atomic, so it stays RT-safe.

use pitch_detection::detector::mcleod::McLeodDetector;
use pitch_detection::detector::PitchDetector;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

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

const RING_CAPACITY: usize = 8192;

/// Threaded monophonic pitch tracker. The audio thread only pushes samples into
/// a lock-free ring and reads the latest published frequency; the (possibly
/// allocating) `get_pitch` runs on a worker thread. Naturally `Send`.
pub struct PitchTracker {
    producer: Option<rtrb::Producer<f32>>,
    result: Arc<AtomicU32>, // bit-cast f32 latest frequency; NaN = no pitch
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
    hop_counter: usize,
    sample_rate: f32,
}

impl Default for PitchTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl PitchTracker {
    pub fn new() -> Self {
        Self {
            producer: None,
            result: Arc::new(AtomicU32::new(f32::NAN.to_bits())),
            stop: Arc::new(AtomicBool::new(false)),
            worker: None,
            hop_counter: 0,
            sample_rate: 48_000.0,
        }
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate;
        self.spawn_worker();
    }

    fn spawn_worker(&mut self) {
        self.stop_worker();
        let (producer, mut consumer) = rtrb::RingBuffer::<f32>::new(RING_CAPACITY);
        self.producer = Some(producer);
        let result = self.result.clone();
        let stop = self.stop.clone();
        stop.store(false, Ordering::Relaxed);
        let sr = self.sample_rate;
        self.worker = Some(std::thread::spawn(move || {
            // Detector CREATED here on the worker thread, so its !Send Rc
            // internals never cross a thread boundary — no `unsafe` needed.
            let mut core = PitchDetectorCore::new();
            core.set_sample_rate(sr);
            loop {
                if stop.load(Ordering::Relaxed) {
                    break;
                }
                let mut did_work = false;
                while let Ok(sample) = consumer.pop() {
                    did_work = true;
                    if let Some(det) = core.push(sample) {
                        let bits = match det {
                            Detection::Pitch(f) => f.to_bits(),
                            Detection::NoPitch => f32::NAN.to_bits(),
                        };
                        result.store(bits, Ordering::Relaxed);
                    }
                }
                if !did_work {
                    std::thread::sleep(std::time::Duration::from_micros(500));
                }
            }
        }));
    }

    fn stop_worker(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.worker.take() {
            let _ = h.join();
        }
        self.producer = None;
    }

    pub fn reset(&mut self) {
        self.hop_counter = 0;
        self.result.store(f32::NAN.to_bits(), Ordering::Relaxed);
    }

    pub fn push(&mut self, sample: f32) -> Option<Detection> {
        if let Some(p) = self.producer.as_mut() {
            let _ = p.push(sample);
        }
        self.hop_counter += 1;
        if self.hop_counter < HOP {
            return None;
        }
        self.hop_counter = 0;
        let f = f32::from_bits(self.result.load(Ordering::Relaxed));
        Some(if f.is_nan() {
            Detection::NoPitch
        } else {
            Detection::Pitch(f)
        })
    }
}

impl Drop for PitchTracker {
    fn drop(&mut self) {
        self.stop_worker();
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

    #[test]
    fn tracker_constructs_and_drops_cleanly() {
        // Spawns a worker on set_sample_rate; Drop must join it without hanging.
        let mut t = PitchTracker::new();
        t.set_sample_rate(44_100.0);
        t.reset();
        drop(t);
    }

    #[test]
    fn tracker_detects_a_sine_on_worker_thread() {
        let sr = 44_100.0;
        let freq = 220.0;
        let mut t = PitchTracker::new();
        t.set_sample_rate(sr);
        let mut seen: Option<f32> = None;
        // Feed generously (well beyond WINDOW*8) and periodically yield so the
        // worker thread can drain the ring and publish a frequency.
        for n in 0..(WINDOW * 16) {
            let s = (TAU * freq * n as f32 / sr).sin();
            if let Some(Detection::Pitch(f)) = t.push(s) {
                if (f - freq).abs() < 3.0 {
                    seen = Some(f);
                    break;
                }
            }
            if n % 2000 == 0 {
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
        }
        assert!(
            seen.is_some(),
            "expected a pitch ~{freq} Hz from the worker thread"
        );
    }
}
