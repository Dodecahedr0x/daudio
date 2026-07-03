//! Windowed monophonic pitch tracking over `pitch-detection` (McLeod method).
//! Detection (`get_pitch` runs an FFT and may allocate) happens on a worker
//! thread; the audio thread only pushes samples into a lock-free ring and reads
//! the latest published frequency via an atomic, so it stays RT-safe.

use pitch_detection::detector::mcleod::McLeodDetector;
use pitch_detection::detector::PitchDetector;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

const WINDOW: usize = 1024;
const PADDING: usize = WINDOW / 2;
pub const HOP: usize = 128;
const POWER_THRESHOLD: f32 = 0.15;
const CLARITY_THRESHOLD: f32 = 0.6;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Detection {
    Pitch { freq: f32, clarity: f32 },
    NoPitch,
}

/// Pack a detection frequency + clarity into one u64 so the audio thread reads a
/// consistent pair with a single atomic load. A `NaN` frequency means "no pitch".
fn pack(freq: f32, clarity: f32) -> u64 {
    ((freq.to_bits() as u64) << 32) | (clarity.to_bits() as u64)
}
fn unpack(bits: u64) -> (f32, f32) {
    (
        f32::from_bits((bits >> 32) as u32),
        f32::from_bits(bits as u32),
    )
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
            Some(p) => Detection::Pitch {
                freq: p.frequency,
                clarity: p.clarity,
            },
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
    result: Arc<AtomicU64>, // packed (freq, clarity); NaN freq = no pitch
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
            result: Arc::new(AtomicU64::new(pack(f32::NAN, 0.0))),
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
                            Detection::Pitch { freq, clarity } => pack(freq, clarity),
                            Detection::NoPitch => pack(f32::NAN, 0.0),
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
        self.result.store(pack(f32::NAN, 0.0), Ordering::Relaxed);
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
        let (freq, clarity) = unpack(self.result.load(Ordering::Relaxed));
        Some(if freq.is_nan() {
            Detection::NoPitch
        } else {
            Detection::Pitch { freq, clarity }
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
    // Note: McLeod (NSDF) clarity scales with the number of periods inside the
    // detection window, so at a short (1024) window low notes read a lower
    // clarity than high notes. C3 (~131 Hz) fits only ~3 periods and reads
    // clarity ~0.68, so we assert its frequency only; C4 (~262 Hz) fits ~6 and
    // exercises the clarity plumbing honestly.
    #[test]
    fn detects_c3_frequency() {
        let sr = 44_100.0;
        let freq = 131.0; // C3
        let mut t = PitchDetectorCore::new();
        t.set_sample_rate(sr);
        let mut last = Detection::NoPitch;
        for n in 0..(WINDOW * 8) {
            let s = (TAU * freq * n as f32 / sr).sin();
            if let Some(d) = t.push(s) {
                last = d;
            }
        }
        match last {
            // Frequency only: ±4 Hz still rules out an octave error (~65 or ~262).
            Detection::Pitch { freq: f, .. } => {
                assert!((f - freq).abs() < 4.0, "detected {f}, expected ~{freq}");
            }
            Detection::NoPitch => panic!("expected a pitch"),
        }
    }

    #[test]
    fn detects_c4_frequency_and_clarity() {
        let sr = 44_100.0;
        let freq = 261.63; // C4
        let mut t = PitchDetectorCore::new();
        t.set_sample_rate(sr);
        let mut last = Detection::NoPitch;
        for n in 0..(WINDOW * 8) {
            let s = (TAU * freq * n as f32 / sr).sin();
            if let Some(d) = t.push(s) {
                last = d;
            }
        }
        match last {
            Detection::Pitch { freq: f, clarity } => {
                assert!((f - freq).abs() < 3.0, "detected {f}, expected ~{freq}");
                assert!(clarity > 0.8, "clarity {clarity} too low");
            }
            Detection::NoPitch => panic!("expected a pitch"),
        }
    }

    #[test]
    fn pack_unpack_roundtrips() {
        let (f, c) = unpack(pack(261.63, 0.93));
        assert!((f - 261.63).abs() < 1e-2 && (c - 0.93).abs() < 1e-3);
        assert!(unpack(pack(f32::NAN, 0.0)).0.is_nan());
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
            if let Some(Detection::Pitch { freq: f, .. }) = t.push(s) {
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
