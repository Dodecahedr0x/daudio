# Pitch→MIDI Follow-ups: Worker-Thread Detection + Pitch-Bend

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** (A) Move pitch detection off the audio thread onto a worker thread fed by a lock-free ring buffer — making the audio thread strictly RT-safe and letting the `unsafe impl Send` go away. (B) Add optional MIDI pitch-bend output that tracks the detected pitch's deviation from the emitted (quantized) note.

**Architecture:** Split `PitchTracker` into a testable synchronous `PitchDetectorCore` (window + `get_pitch`, `!Send`, unit-tested on the test thread) and a threaded `PitchTracker` wrapper (rtrb SPSC ring producer + a worker thread that *creates* the core locally and publishes the latest frequency via an atomic; naturally `Send`, no `unsafe`). Pitch-bend is an optional `BoolParam` in the plugin plus a `MidiConfig::MidiCCs` bump in the macro's `midi_out` mode.

**Tech Stack:** Rust nightly, nih-plug (pinned rev `f36931f7af4646065488a9845d8f8c2f95252c23`), `pitch-detection` 0.3, `rtrb` 0.3 (real-time SPSC ring).

**Verified facts:** `rtrb` 0.3.4 exists. `NoteEvent::<()>::MidiPitchBend { timing: u32, channel: u8, value: f32 }` — value is normalized `0.0..=1.0`, `0.5` = center (VERIFY the field name/type in `~/.cargo/git/checkouts/nih-plug-*/f36931f/src/midi.rs:291`). It requires `MidiConfig::MidiCCs` or higher.

Reference skills: superpowers:test-driven-development; rs-check after each task.

---

## FEATURE A — Worker-thread detection

### Task A1: Extract `PitchDetectorCore` (the testable synchronous detector)

**Files:** `crates/daudio-dsp/src/pitch.rs`.

Rename the current synchronous detection struct to `PitchDetectorCore` (keep ALL current logic and the ring→scratch reconstruction). It stays `!Send` (holds `McLeodDetector` with `Rc`). **Delete the `unsafe impl Send`** — the core is never sent; only the new wrapper (Task A2) crosses threads, and it holds no detector.

```rust
pub(crate) struct PitchDetectorCore {
    detector: McLeodDetector<f32>,
    ring: Vec<f32>, scratch: Vec<f32>, write: usize, hop_counter: usize, sample_rate: usize,
}
impl PitchDetectorCore {
    pub(crate) fn new() -> Self { /* unchanged McLeodDetector::new(WINDOW,PADDING), buffers */ }
    pub(crate) fn set_sample_rate(&mut self, sr: f32) { ... }
    pub(crate) fn reset(&mut self) { ... }
    /// Push one sample; returns `Some(Detection)` every HOP samples. (unchanged body)
    pub(crate) fn push(&mut self, sample: f32) -> Option<Detection> { ... }
}
```

Keep the existing `Detection` enum public, and `pub const HOP`. Move the existing `detects_a_sine_frequency` / `silence_is_no_pitch` tests to exercise `PitchDetectorCore` directly (they run on the test thread synchronously — no threading, no flakiness). They must still pass (sine ≈ 220 Hz within ±3 Hz).

**Verify:** `cargo nextest run -p daudio-dsp pitch` passes. `cargo clippy` — confirm NO `unsafe impl Send` remains in pitch.rs. **Commit** `refactor(dsp): extract testable PitchDetectorCore from PitchTracker`.

### Task A2: Threaded `PitchTracker` wrapper (rtrb + worker)

**Files:** `crates/daudio-dsp/Cargo.toml` (add `rtrb = "0.3"`), `crates/daudio-dsp/src/pitch.rs`.

```rust
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

const RING_CAPACITY: usize = 8192; // several windows of headroom

/// Threaded monophonic pitch tracker. The audio thread only pushes samples into
/// a lock-free ring and reads the latest published frequency — the (possibly
/// allocating) `get_pitch` runs on a worker thread. Naturally `Send`.
pub struct PitchTracker {
    producer: Option<rtrb::Producer<f32>>,
    result: Arc<AtomicU32>,   // bit-cast f32 latest frequency; NaN = no pitch
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
    hop_counter: usize,
    sample_rate: f32,
}
```

- `new()`: dormant — `producer: None`, `result` init to `f32::NAN.to_bits()`, `stop` false, `worker: None`, counters 0, sample_rate 48k.
- `set_sample_rate(sr)`: `self.sample_rate = sr;` then `self.spawn_worker();` (tears down any existing worker first — see `stop_worker`). This is where the worker starts (called from the plugin's `activate`).
- `spawn_worker(&mut self)`:
  ```rust
  self.stop_worker();
  let (producer, mut consumer) = rtrb::RingBuffer::<f32>::new(RING_CAPACITY);
  self.producer = Some(producer);
  let result = self.result.clone();
  let stop = self.stop.clone();
  stop.store(false, Ordering::Relaxed);
  let sr = self.sample_rate;
  self.worker = Some(std::thread::spawn(move || {
      // The detector is CREATED HERE, on the worker thread, so the `!Send`
      // `Rc` internals never cross a thread boundary — no `unsafe` needed.
      let mut core = PitchDetectorCore::new();
      core.set_sample_rate(sr);
      loop {
          if stop.load(Ordering::Relaxed) { break; }
          let mut did_work = false;
          while let Ok(sample) = consumer.pop() {
              did_work = true;
              if let Some(det) = core.push(sample) {
                  let bits = match det { Detection::Pitch(f) => f.to_bits(), Detection::NoPitch => f32::NAN.to_bits() };
                  result.store(bits, Ordering::Relaxed);
              }
          }
          if !did_work { std::thread::sleep(std::time::Duration::from_micros(500)); }
      }
  }));
  ```
- `stop_worker(&mut self)`: if a worker exists, `self.stop.store(true, Relaxed); if let Some(h) = self.worker.take() { let _ = h.join(); } self.producer = None;`.
- `reset(&mut self)`: reset the audio-side state (`hop_counter = 0`) and publish `NaN` to `result`. (The worker's own window keeps rolling; a full clear isn't needed for correctness — silence will flush it. Keep reset light and non-blocking.)
- `push(&mut self, sample: f32) -> Option<Detection>`:
  ```rust
  if let Some(p) = self.producer.as_mut() { let _ = p.push(sample); } // drop if full (shouldn't happen)
  self.hop_counter += 1;
  if self.hop_counter < HOP { return None; }
  self.hop_counter = 0;
  let bits = self.result.load(Ordering::Relaxed);
  let f = f32::from_bits(bits);
  Some(if f.is_nan() { Detection::NoPitch } else { Detection::Pitch(f) })
  ```
  This keeps the plugin's `if let Some(detection) = tracker.push(input)` structure identical — the audio thread does only a ring push + an atomic load (both RT-safe, no allocation, no locking).
- `impl Drop for PitchTracker { fn drop(&mut self) { self.stop_worker(); } }`
- `impl Default for PitchTracker { fn default() -> Self { Self::new() } }`

**Light integration test** (timing-tolerant): feed a 220 Hz sine sample-by-sample into a `PitchTracker` after `set_sample_rate`, sleeping briefly every few thousand samples to let the worker run, and poll `push` return values; assert that within a bounded number of samples/time a `Detection::Pitch(f)` with `f≈220` appears. Keep the bound generous (e.g. allow up to WINDOW*8 samples + a few short sleeps) so it isn't flaky. If threading makes a reliable unit test hard, a minimal test that just constructs/drops a `PitchTracker` (verifies clean spawn+join, no panic/hang) plus the synchronous `PitchDetectorCore` tests from A1 is acceptable — note which you did.

**Verify:** `cargo nextest run -p daudio-dsp` passes; `cargo clippy --workspace -- -D warnings` clean; the plugin still builds (`cargo build -p pitch-to-midi`), since `PitchTracker`'s `push`/`set_sample_rate`/`reset`/`Default` API is unchanged. Confirm `unsafe` is gone (`grep -rn "unsafe impl Send" crates/`). **Commit** `feat(dsp): run pitch detection on a worker thread (removes unsafe Send)`.

### Task A3: Confirm the plugin + RT-safety

**Files:** none expected (the `PitchTracker` API is source-compatible). Verify:
- `cargo build -p pitch-to-midi --bin standalone`.
- `cargo run -p pitch-to-midi --bin demo -- <a 220Hz tone>.wav` STILL prints `NoteOn A3` (the offline analyzer drives `process_sample`; because it feeds samples faster than real time, add: if the analyzer path shows no note now due to worker latency, that's expected offline — the worker can't keep up with faster-than-realtime feeding). **Adjustment:** the offline analyzer feeds samples with no timing, so the worker may not process in lockstep. To keep the offline demo deterministic, the `run_analyzer` harness should, after feeding all samples, sleep briefly and drain — OR (simpler, RECOMMENDED) have the demo/test rely on the synchronous `PitchDetectorCore` path. Practically: verify the DAW/standalone path builds; for the offline `demo` verification, feed the tone and allow a short sleep, and report whether `NoteOn A3` still appears. If worker-vs-offline timing makes the demo unreliable, note it — the real-time path (standalone/DAW) is what the worker thread is for.
- Update `docs/plans/2026-07-02-daudio-pitch-to-midi-design.md`: replace the "Real-time caveat" paragraph with a note that detection now runs on a worker thread (audio thread only pushes to a ring + reads an atomic; strictly RT-safe).

**Commit** `docs: detection is now RT-safe via worker thread` (if only docs changed) or fold into A2.

---

## FEATURE B — Pitch-bend output

### Task B1: macro `midi_out` uses `MidiConfig::MidiCCs`

**Files:** `crates/daudio-sdk-macros/src/lib.rs`.

In the `midi_out` Plugin impl, change `const MIDI_OUTPUT: MidiConfig = MidiConfig::Basic;` to `MidiConfig::MidiCCs` (a superset — still allows note on/off, additionally allows pitch bend/CCs). No other change. Verify filter + synth + pitch-to-midi still build; `cargo test --workspace` green. **Commit** `feat(sdk): midi_out enables MidiCCs so analyzers can emit pitch bend`.

### Task B2: optional pitch-bend in the plugin

**Files:** `plugins/pitch-to-midi/src/lib.rs`.

- Add a param `#[id = "bend"] pitch_bend: BoolParam` (default `false`, name "Pitch Bend"). Add a `NoteToggle`? No — it's a mode switch, use a `ParamButton` or a labeled toggle in the editor (Task B3), or leave it param-only for now and add to the editor in B3.
- Track the active note and last-sent bend on the plugin: add fields `active_note: Option<u8>` and `last_bend: f32` (init 0.5). Update `active_note` from the trigger — expose the trigger's active note: add `pub fn active(&self) -> Option<u8>` to `Trigger`, or capture it in the `on_hop` emit closure (set `active_note` on `NoteAction::On{note}`, clear on `NoteAction::Off`).
- In `process_sample`, on each hop AFTER running the trigger:
  ```rust
  if self.params.pitch_bend.value() {
      if let (Some(note), Detection::Pitch(f)) = (self.active_note, detection) {
          let note_freq = 440.0 * 2.0_f32.powf((note as f32 - 69.0) / 12.0);
          let dev_semitones = 12.0 * (f / note_freq).log2();      // detected vs held note
          // ±2 semitone bend range -> normalized 0..1, 0.5 = center
          let value = (0.5 + dev_semitones / 4.0).clamp(0.0, 1.0);
          if (value - self.last_bend).abs() > 1e-4 {
              emit(NoteEvent::MidiPitchBend { timing, channel: 0, value });
              self.last_bend = value;
          }
      }
  }
  ```
  And when a note turns OFF (or no active note), reset bend to center once:
  ```rust
  // after the trigger, if no active note and last_bend != center:
  if self.active_note.is_none() && (self.last_bend - 0.5).abs() > 1e-4 {
      emit(NoteEvent::MidiPitchBend { timing, channel: 0, value: 0.5 });
      self.last_bend = 0.5;
  }
  ```
  VERIFY the `MidiPitchBend` field name/type (`value: f32`, 0..1) against the pinned `midi.rs`; adjust if different. The `emit` closure is the same `&mut dyn FnMut(NoteEvent<()>)`.
- Reset `active_note = None; last_bend = 0.5;` in `reset()`/`activate()`.

**Test:** add a small unit test for the bend math — a pure helper `fn bend_value(detected_hz: f32, note: u8) -> f32` returning the normalized value; assert: exact note freq → 0.5; +1 semitone sharp → 0.75; -2 semitones → 0.0 (clamped); +3 semitones → 1.0 (clamped). Put `bend_value` as a pure fn (in the plugin or daudio-dsp::notes) and unit-test it.

**Verify:** `cargo build -p pitch-to-midi` + `--bin standalone`; `cargo test --workspace`; clippy `-D warnings`; fmt; bundle. **Commit** `feat(pitch-to-midi): optional pitch-bend output tracking detected pitch`.

### Task B3: expose Pitch-Bend toggle in the editor

**Files:** `plugins/pitch-to-midi/src/lib.rs` (editor).

Add a control for `pitch_bend` to the editor — simplest is nih_plug_vizia's `ParamButton::new(cx, DaudioData::<PitchToMidiParams>::params, |p| &p.pitch_bend)` labeled "Pitch Bend", placed near the sensitivity/hold knobs. Bump `editor_state` height if needed. Do NOT touch the readout/root-lens/preset logic. **Verify** builds + bundles. **Commit** `feat(pitch-to-midi): add Pitch Bend toggle to the editor`.

---

## Definition of Done

- Detection runs on a worker thread; the audio thread only does a ring push + atomic load. `unsafe impl Send` is gone from the codebase.
- `PitchDetectorCore` keeps the synchronous sine/silence tests green; `PitchTracker` spawns/joins its worker cleanly.
- `midi_out` uses `MidiConfig::MidiCCs`; the plugin optionally emits `MidiPitchBend` tracking the detected pitch's deviation from the held note (±2 semitones), off by default, with a tested `bend_value` helper.
- Filter + synth unaffected; `cargo test --workspace` green; clippy `-D warnings` clean; fmt clean; all three bundles build.

## Out of scope

- MIDI-file export (explicitly deferred).
- Configurable bend range / per-note MPE channels.
- Velocity-curve shaping.
