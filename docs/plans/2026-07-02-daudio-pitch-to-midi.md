# daudio Pitch→MIDI — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** A monophonic audio→MIDI plugin that detects input pitch, quantizes it to a user-defined scale (root + 12 relative-degree toggles), and emits debounced, level-gated MIDI notes — introducing a third SDK seam (`DaudioAudioToMidi` + `midi_out` macro mode), a `NoteToggle` widget, and a reusable analyzer preview.

**Architecture:** Pure, tested DSP in `daudio-dsp` (`quantize`, `freq_to_midi`, `note_name`, and a `PitchTracker` wrapping the `pitch-detection` crate). A new `DaudioAudioToMidi` trait in `daudio-sdk` and a `midi_out` codegen branch in the macro (stereo audio pass-through + MIDI out). The plugin `plugins/pitch-to-midi` composes tracker + quantizer + a testable trigger state machine, with a `daudio-ui` `NoteToggle` widget and a knob/readout editor.

**Tech Stack:** Rust nightly, nih-plug + nih_plug_vizia (pinned rev `f36931f7af4646065488a9845d8f8c2f95252c23`), `pitch-detection` 0.3, syn/quote.

**Verified crate API (`pitch-detection` 0.3.0):**
- `McLeodDetector::<f32>::new(size, padding)` → detector.
- `PitchDetector::get_pitch(&mut self, signal: &[f32], sample_rate: usize, power_threshold: f32, clarity_threshold: f32) -> Option<Pitch<f32>>`; `signal.len()` MUST equal `size` (asserts).
- `Pitch { frequency: f32, clarity: f32 }`.
- Import paths: `pitch_detection::detector::mcleod::McLeodDetector`, `pitch_detection::detector::PitchDetector`.

**nih-plug notes:** `NoteEvent<()>::NoteOn { timing: u32, voice_id: Option<i32>, channel: u8, note: u8, velocity: f32 }` (velocity is 0.0–1.0, NOT 0–127); `NoteOff { timing, voice_id, channel, note, velocity }`. `context.send_event(NoteEvent<Self::SysExMessage>)`. `const MIDI_OUTPUT: MidiConfig = MidiConfig::Basic;`.

Reference skills: superpowers:test-driven-development; rs-check after each task.

---

## Scope

In: `quantize`/`freq_to_midi`/`note_name`, `PitchTracker`, `DaudioAudioToMidi` trait, `midi_out` macro mode, trigger state machine, `plugins/pitch-to-midi`, `NoteToggle` widget, editor, `run_analyzer` preview.

Out (YAGNI): polyphony, pitch-bend/glide output, exposed detection controls, worker-thread detection, MIDI file export.

---

## Task 1: `daudio-dsp` — note math (`freq_to_midi`, `note_name`, `quantize`)

**Files:** create `crates/daudio-dsp/src/notes.rs`; add `pub mod notes;` to `crates/daudio-dsp/src/lib.rs`.

**Step 1: write failing tests** in `notes.rs`:
```rust
//! MIDI note math and scale quantization (pure, host-agnostic).

/// Convert a frequency in Hz to the nearest MIDI note number.
pub fn freq_to_midi(freq_hz: f32) -> i32 {
    (69.0 + 12.0 * (freq_hz / 440.0).log2()).round() as i32
}

/// Note name like "A4" for a MIDI note (A4 = 69). Uses sharps.
pub fn note_name(midi: i32) -> String {
    const NAMES: [&str; 12] = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    let pc = midi.rem_euclid(12) as usize;
    let octave = midi.div_euclid(12) - 1;
    format!("{}{}", NAMES[pc], octave)
}

/// Snap `midi` to the nearest note allowed by a scale defined as a `root`
/// pitch-class (0=C..11=B) and a 12-bit `degree_mask` (bit d set = the note
/// `root + d` mod 12 is allowed). Ties resolve upward. Empty mask → `None`.
pub fn quantize(midi: i32, root: u8, degree_mask: u16) -> Option<i32> {
    if degree_mask & 0x0fff == 0 {
        return None;
    }
    let allowed = |note: i32| -> bool {
        let degree = (note - root as i32).rem_euclid(12) as u16;
        degree_mask & (1 << degree) != 0
    };
    if allowed(midi) {
        return Some(midi);
    }
    // Search outward; +offset checked before -offset so ties resolve upward.
    for offset in 1..=6 {
        if allowed(midi + offset) {
            return Some(midi + offset);
        }
        if allowed(midi - offset) {
            return Some(midi - offset);
        }
    }
    Some(midi) // unreachable given non-empty mask, but keep total
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAJOR: u16 = 0b1010_1101_0101; // degrees 0,2,4,5,7,9,11

    #[test]
    fn freq_to_midi_landmarks() {
        assert_eq!(freq_to_midi(440.0), 69);
        assert_eq!(freq_to_midi(220.0), 57);
        assert_eq!(freq_to_midi(880.0), 81);
    }

    #[test]
    fn note_names() {
        assert_eq!(note_name(69), "A4");
        assert_eq!(note_name(60), "C4");
        assert_eq!(note_name(61), "C#4");
    }

    #[test]
    fn in_scale_notes_pass_through() {
        assert_eq!(quantize(60, 0, MAJOR), Some(60)); // C in C-major
        assert_eq!(quantize(64, 0, MAJOR), Some(64)); // E
    }

    #[test]
    fn out_of_scale_snaps_upward_on_tie() {
        // C# (61) is equidistant from C (60) and D (62); tie -> up -> D.
        assert_eq!(quantize(61, 0, MAJOR), Some(62));
    }

    #[test]
    fn root_shifts_the_scale() {
        // A-major (root 9): A(69) in scale; C(60) -> degree 3 (not in major)
        // nearest is C# (61, degree 4) up vs B (59, degree 2) down: tie -> up.
        assert_eq!(quantize(69, 9, MAJOR), Some(69));
        assert_eq!(quantize(60, 9, MAJOR), Some(61));
    }

    #[test]
    fn empty_mask_is_none() {
        assert_eq!(quantize(60, 0, 0), None);
    }
}
```

> The `MAJOR` bit literal: verify degrees 0,2,4,5,7,9,11 → `1<<0|1<<2|1<<4|1<<5|1<<7|1<<9|1<<11 = 2741 = 0b101011010101`. Fix the literal if the binary grouping differs; the test intent is those degrees.

**Step 2:** Run `cargo nextest run -p daudio-dsp notes` → expect PASS (functions are written above; this task ships impl+tests together since the logic is small and total).

**Step 3:** rs-check + commit `feat(dsp): add MIDI note math and scale quantization`.

---

## Task 2: `daudio-dsp` — `PitchTracker` (wraps `pitch-detection`)

**Files:** create `crates/daudio-dsp/src/pitch.rs`; add `pub mod pitch;` to lib.rs; add dep to `crates/daudio-dsp/Cargo.toml`.

**Step 1:** add to `crates/daudio-dsp/Cargo.toml` `[dependencies]`:
```toml
pitch-detection = "0.3"
```

**Step 2: write the tracker** in `pitch.rs`:
```rust
//! Windowed monophonic pitch tracking over `pitch-detection` (McLeod method).
//!
//! Feed samples one at a time with [`PitchTracker::push`]; once every
//! [`HOP`] samples it runs detection over the most recent [`WINDOW`] samples
//! and returns a [`Detection`].
//!
//! NOTE: `get_pitch` runs an FFT and may allocate, so `push` is only
//! *approximately* real-time-safe on hop boundaries. Acceptable for v1; a
//! worker-thread version is a documented follow-up.

use pitch_detection::detector::mcleod::McLeodDetector;
use pitch_detection::detector::PitchDetector;

const WINDOW: usize = 2048;
const PADDING: usize = WINDOW / 2;
const HOP: usize = 256;
const POWER_THRESHOLD: f32 = 0.15;
const CLARITY_THRESHOLD: f32 = 0.6;

/// The result of a detection hop.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Detection {
    /// A clear pitch was found (frequency in Hz).
    Pitch(f32),
    /// No clear pitch this hop.
    NoPitch,
}

pub struct PitchTracker {
    detector: McLeodDetector<f32>,
    ring: Vec<f32>,     // most-recent WINDOW samples, ring-ordered
    scratch: Vec<f32>,  // chronological copy handed to the detector
    write: usize,
    hop_counter: usize,
    sample_rate: usize,
}

impl Default for PitchTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl PitchTracker {
    pub fn new() -> Self {
        Self {
            detector: McLeodDetector::new(WINDOW, PADDING),
            ring: vec![0.0; WINDOW],
            scratch: vec![0.0; WINDOW],
            write: 0,
            hop_counter: 0,
            sample_rate: 48_000,
        }
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate as usize;
    }

    pub fn reset(&mut self) {
        self.ring.iter_mut().for_each(|s| *s = 0.0);
        self.write = 0;
        self.hop_counter = 0;
    }

    /// Feed one sample. Returns `Some(Detection)` once every `HOP` samples,
    /// `None` in between.
    pub fn push(&mut self, sample: f32) -> Option<Detection> {
        self.ring[self.write] = sample;
        self.write = (self.write + 1) % WINDOW;
        self.hop_counter += 1;
        if self.hop_counter < HOP {
            return None;
        }
        self.hop_counter = 0;

        // Copy the ring into chronological order for the detector.
        for i in 0..WINDOW {
            self.scratch[i] = self.ring[(self.write + i) % WINDOW];
        }
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
        let mut t = PitchTracker::new();
        t.set_sample_rate(sr);
        let mut last = Detection::NoPitch;
        // Feed ~4 windows so at least one hop detects steady state.
        for n in 0..(WINDOW * 4) {
            let s = (TAU * freq * n as f32 / sr).sin();
            if let Some(d) = t.push(s) {
                last = d;
            }
        }
        match last {
            Detection::Pitch(f) => {
                assert!((f - freq).abs() < 3.0, "detected {f}, expected ~{freq}");
            }
            Detection::NoPitch => panic!("expected a pitch"),
        }
    }

    #[test]
    fn silence_is_no_pitch() {
        let mut t = PitchTracker::new();
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
```

**Step 3:** Run `cargo nextest run -p daudio-dsp pitch` → expect PASS. If the sine test detects an octave (110 or 440), widen tolerance or adjust `POWER_THRESHOLD`/`CLARITY_THRESHOLD`, but do NOT loosen past ±3 Hz on the fundamental — an octave error is a real bug to investigate (check ring→scratch ordering).

**Step 4:** rs-check + commit `feat(dsp): add windowed PitchTracker over pitch-detection`.

---

## Task 3: `daudio-sdk` — `DaudioAudioToMidi` trait

**Files:** create `crates/daudio-sdk/src/audio_to_midi.rs`; export from `crates/daudio-sdk/src/lib.rs` (+ prelude).

**Step 1:** `audio_to_midi.rs`:
```rust
use nih_plug::prelude::*;

/// An audio-input → MIDI-output analyzer (e.g. pitch-to-MIDI). The macro's
/// `midi_out = true` mode drives this: audio is passed through unchanged and
/// MIDI events are emitted alongside it.
///
/// The annotated struct MUST have a field `params: std::sync::Arc<Self::Params>`.
pub trait DaudioAudioToMidi: Send {
    type Params: Params + Default;

    /// Called from `Plugin::initialize`.
    fn activate(&mut self, sample_rate: f32);

    /// Called from `Plugin::reset`. Default: no-op.
    fn reset(&mut self) {}

    /// Feed one mono input sample. `timing` is the sample's offset within the
    /// current block; stamp emitted events with it. Push events via `emit`.
    fn process_sample(&mut self, input: f32, timing: u32, emit: &mut dyn FnMut(NoteEvent<()>));

    /// Optional custom editor.
    fn editor(&mut self) -> Option<Box<dyn Editor>> {
        None
    }
}
```

**Step 2:** in `lib.rs` add `pub mod audio_to_midi;`, `pub use audio_to_midi::DaudioAudioToMidi;`, and add `DaudioAudioToMidi` to the `prelude`.

**Step 3:** verify `cargo build -p daudio-sdk`; existing tests still pass; clippy/fmt clean. Commit `feat(sdk): add DaudioAudioToMidi trait`.

---

## Task 4 + 5: macro `midi_out` mode + the plugin (developed together)

Like the synth, build the macro branch and its first consumer together so the codegen is exercised by a real compile. Read `crates/daudio-sdk-macros/src/lib.rs` (the existing `midi`/synth branch is the template).

### Task 4 — macro `midi_out` mode

**Files:** `crates/daudio-sdk-macros/src/lib.rs`.

Add a boolean attribute key `midi_out` (default false), parsed exactly like `midi`. When `midi_out = true`, generate a `Plugin` impl delegating to `DaudioAudioToMidi` (mutually exclusive with `midi`; if both set, emit a `compile_error!`). All paths routed through `::daudio_sdk::nih_plug::`:
- `const MIDI_OUTPUT: MidiConfig = MidiConfig::Basic;` (and `MIDI_INPUT` left default `None`).
- `AUDIO_IO_LAYOUTS`: stereo in / stereo out (copy the effect layout exactly).
- `SAMPLE_ACCURATE_AUTOMATION = true; type SysExMessage = (); type BackgroundTask = ();`
- `params()` from `self.params`.
- `initialize` → `<Self as DaudioAudioToMidi>::activate(self, buffer_config.sample_rate); true`.
- `reset` → `<Self as DaudioAudioToMidi>::reset(self)`.
- `editor` → `<Self as DaudioAudioToMidi>::editor(self)`.
- `process`:
  ```rust
  for (sample_id, mut channel_samples) in buffer.iter_samples().enumerate() {
      let mut sum = 0.0f32;
      let mut count = 0u32;
      for s in channel_samples.iter_mut() {
          sum += *s;
          count += 1;
      }
      let mono = if count > 0 { sum / count as f32 } else { 0.0 };
      <Self as ::daudio_sdk::DaudioAudioToMidi>::process_sample(
          self,
          mono,
          sample_id as u32,
          &mut |event| context.send_event(event),
      );
      // audio passes through unchanged
  }
  ::daudio_sdk::nih_plug::prelude::ProcessStatus::Normal
  ```
  Verify `context.send_event`, `NoteEvent<()>`, and `iter_samples().iter_mut()` compile against the pinned rev; adjust to real names if needed.

Keep the effect and synth branches byte-for-byte unchanged. Commit `feat(sdk): add midi_out (audio→MIDI) mode to daudio_plugin macro` once the macro compiles and the filter + synth still build.

### Task 5 — trigger state machine (TDD, in the plugin)

**Files:** create `plugins/pitch-to-midi/Cargo.toml`, `plugins/pitch-to-midi/src/trigger.rs`, `plugins/pitch-to-midi/src/lib.rs`; add `"plugins/pitch-to-midi"` to root `Cargo.toml` members.

`Cargo.toml`:
```toml
[package]
name = "pitch-to-midi"
version = "0.1.0"
edition.workspace = true
license.workspace = true

[lib]
crate-type = ["cdylib", "lib"]

[dependencies]
daudio-sdk = { path = "../../crates/daudio-sdk" }
daudio-dsp = { path = "../../crates/daudio-dsp" }
daudio-ui = { path = "../../crates/daudio-ui" }
nih_plug = { git = "https://github.com/robbert-vdh/nih-plug.git", rev = "f36931f7af4646065488a9845d8f8c2f95252c23", features = ["standalone"] }
```

**Trigger** — a pure, testable state machine (no nih-plug types; emits abstract events):
```rust
//! Monophonic note trigger: debounced, level-gated, velocity from level.

/// A decision the trigger emits.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NoteAction {
    On { note: u8, velocity: f32 },
    Off { note: u8 },
}

pub struct Trigger {
    hold_hops: u32,      // debounce, in detection hops
    active: Option<u8>,  // currently sounding note
    candidate: Option<i32>,
    candidate_hops: u32,
}

impl Trigger {
    pub fn new() -> Self {
        Self { hold_hops: 2, active: None, candidate: None, candidate_hops: 0 }
    }

    /// Configure debounce from a hold time in ms and the hop rate.
    pub fn set_hold(&mut self, hold_ms: f32, hop_seconds: f32) {
        self.hold_hops = ((hold_ms / 1000.0) / hop_seconds).ceil().max(1.0) as u32;
    }

    pub fn reset(&mut self) {
        self.active = None;
        self.candidate = None;
        self.candidate_hops = 0;
    }

    /// Advance one detection hop. `target` is the quantized note this hop
    /// (already level-gated: `None` = gate closed / no pitch). Calls `emit`
    /// with any resulting actions (Off before On on a change).
    pub fn on_hop(&mut self, target: Option<i32>, velocity: f32, emit: &mut dyn FnMut(NoteAction)) {
        // Gate closed → release immediately, no debounce.
        if target.is_none() {
            if let Some(n) = self.active.take() {
                emit(NoteAction::Off { note: n });
            }
            self.candidate = None;
            self.candidate_hops = 0;
            return;
        }
        let target = target.unwrap();
        if Some(target) == self.active.map(|n| n as i32) {
            self.candidate = None;
            self.candidate_hops = 0;
            return;
        }
        // Debounce a new candidate.
        if self.candidate == Some(target) {
            self.candidate_hops += 1;
        } else {
            self.candidate = Some(target);
            self.candidate_hops = 1;
        }
        if self.candidate_hops >= self.hold_hops {
            if let Some(n) = self.active.take() {
                emit(NoteAction::Off { note: n });
            }
            let note = target.clamp(0, 127) as u8;
            emit(NoteAction::On { note, velocity });
            self.active = Some(note);
            self.candidate = None;
            self.candidate_hops = 0;
        }
    }
}

impl Default for Trigger {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collect(t: &mut Trigger, target: Option<i32>, vel: f32) -> Vec<NoteAction> {
        let mut v = Vec::new();
        t.on_hop(target, vel, &mut |a| v.push(a));
        v
    }

    #[test]
    fn debounce_blocks_one_hop_blip() {
        let mut t = Trigger::new(); // hold_hops = 2
        assert!(collect(&mut t, Some(60), 0.8).is_empty()); // hop 1: candidate
        // a one-hop blip to a different note resets the candidate
        assert!(collect(&mut t, Some(67), 0.8).is_empty());
        assert!(collect(&mut t, Some(67), 0.8) == vec![NoteAction::On { note: 67, velocity: 0.8 }]);
    }

    #[test]
    fn gate_close_releases_active_note() {
        let mut t = Trigger::new();
        collect(&mut t, Some(60), 0.8);
        collect(&mut t, Some(60), 0.8); // commits note 60
        let out = collect(&mut t, None, 0.0);
        assert_eq!(out, vec![NoteAction::Off { note: 60 }]);
    }

    #[test]
    fn note_change_sends_off_then_on() {
        let mut t = Trigger::new();
        collect(&mut t, Some(60), 0.8);
        collect(&mut t, Some(60), 0.8); // note 60 on
        collect(&mut t, Some(64), 0.9);
        let out = collect(&mut t, Some(64), 0.9); // 64 commits
        assert_eq!(out, vec![NoteAction::Off { note: 60 }, NoteAction::On { note: 64, velocity: 0.9 }]);
    }
}
```
Write these tests first (stub `on_hop` with `unimplemented!()`), see them fail, implement, see them pass. Commit `feat(pitch-to-midi): add debounced note trigger`.

### Task 6 — plugin: params + `DaudioAudioToMidi` impl + wiring

**Files:** `plugins/pitch-to-midi/src/lib.rs`, `plugins/pitch-to-midi/src/bin/standalone.rs`, `bundler.toml`.

- `#[derive(Enum)] enum Root { C, Cs, D, Ds, E, F, Fs, G, Gs, A, As, B }` (nih-plug `EnumParam<Root>`; `Root as u8` gives 0..11 — verify the derive gives that ordering, else add a `fn pc(&self) -> u8`).
- `PitchToMidiParams` (`#[derive(Params)]`, pub): `root: EnumParam<Root>`, `degree_0..degree_11: BoolParam` (defaults = major scale: degrees 0,2,4,5,7,9,11 true), `sensitivity: FloatParam` (dB, e.g. -60..0, default -40, gate threshold), `hold: FloatParam` (ms, 10..200, default 40), plus `#[persist = "editor-state"] editor_state: Arc<nih_plug_vizia::ViziaState>`.
  - Add a helper `fn degree_mask(&self) -> u16` building the 12-bit mask from the bool params, and `fn root_pc(&self) -> u8`.
- Note-readout channel: two `Arc<AtomicI32>` (or reuse a small `NoteChannel` like `PeakLevel`) for "detected midi" and "output midi" (−1 = none), shared audio→UI.
- `PitchToMidi` struct (pub): `params: Arc<PitchToMidiParams>`, `tracker: PitchTracker`, `trigger: Trigger`, `level: f32` (peak envelope), `level_decay: f32`, `sample_rate: f32`, plus the readout channels.
  - `#[daudio_plugin(name = "daudio Pitch2MIDI", vendor = "daudio", url = "https://example.com", email = "hexadecifish@gmail.com", clap_id = "com.daudio.pitch2midi", clap_description = "Monophonic pitch to MIDI with scale quantization", vst3_id = "daudioPitch2Midi", clap_features = [AudioEffect, Utility, Analyzer], vst3_categories = [Fx, Analyzer], midi_out = true)]`.
    - COUNT `vst3_id`: `daudioPitch2Midi` = 16 bytes (verify). Adjust `ClapFeature`/`Vst3SubCategory` variants to ones that exist in the pinned rev (e.g. `ClapFeature::Analyzer`, `ClapFeature::Utility`; `Vst3SubCategory::Analyzer` — if a variant is missing, drop it).
- `impl DaudioAudioToMidi for PitchToMidi`:
  - `activate`: `tracker.set_sample_rate(sr)`, `sample_rate = sr`, `level_decay = (-1.0/(0.05*sr)).exp()` (~50 ms peak fall), `trigger.set_hold(self.params.hold.value(), HOP as f32 / sr)` — expose `HOP` from `daudio_dsp::pitch` as `pub const HOP` (add `pub` to it in Task 2 or a `pub fn hop_seconds(sr)` helper; simplest: `pub const HOP: usize`).
  - `reset`: `tracker.reset(); trigger.reset(); level = 0.0;`.
  - `process_sample(&mut self, input, timing, emit)`:
    ```rust
    // level envelope (peak follower)
    self.level = input.abs().max(self.level * self.level_decay);
    if let Some(detection) = self.tracker.push(input) {
        self.trigger.set_hold(self.params.hold.value(), HOP as f32 / self.sample_rate);
        let threshold = daudio_dsp::gain::db_to_gain(self.params.sensitivity.value());
        let gated = self.level >= threshold;
        let target = match detection {
            Detection::Pitch(f) if gated => {
                let midi = daudio_dsp::notes::freq_to_midi(f);
                self.detected.store(midi, Relaxed);
                daudio_dsp::notes::quantize(midi, self.params.root_pc(), self.params.degree_mask())
            }
            _ => { self.detected.store(-1, Relaxed); None }
        };
        let velocity = self.level.clamp(0.0, 1.0); // 0..1 as nih-plug expects
        self.output.store(target.unwrap_or(-1), Relaxed);
        self.trigger.on_hop(target, velocity, &mut |action| match action {
            NoteAction::On { note, velocity } => emit(NoteEvent::NoteOn {
                timing, voice_id: None, channel: 0, note, velocity,
            }),
            NoteAction::Off { note } => emit(NoteEvent::NoteOff {
                timing, voice_id: None, channel: 0, note, velocity: 0.0,
            }),
        });
    }
    ```
  - `editor`: `None` for now (Task 8 fills it in).
- `standalone.rs`: `use nih_plug::prelude::*; use pitch_to_midi::PitchToMidi; fn main() { nih_export_standalone::<PitchToMidi>(); }`.
- `bundler.toml`: add `[pitch-to-midi] name = "daudio Pitch2MIDI"`.

Gate: `cargo build -p pitch-to-midi` + `--bin standalone`; filter + synth still build; `cargo test --workspace` green; clippy `-D warnings`; fmt; `cargo xtask bundle pitch-to-midi --release` produces `daudio Pitch2MIDI.vst3` + `.clap`. Commit `feat(pitch-to-midi): pitch tracker + quantizer + trigger on the SDK`.

---

## Task 7: `daudio-ui` — `NoteToggle` widget (root-reactive label)

**Files:** create `crates/daudio-ui/src/note_toggle.rs`; export from lib.rs + prelude; theme.css class.

A labeled on/off button bound to a `BoolParam`, whose caption is an absolute note name derived from a `root` value + this toggle's degree index, updated live when root changes.

- Signature mirrors nih_plug_vizia param widgets but adds a `root` lens and a `degree` index:
  ```rust
  pub fn new<L, P, RL>(
      cx: &mut Context,
      params: L,
      params_to_param: impl Fn(&P) -> &BoolParam + Copy + 'static,
      degree: u8,
      root: RL,           // Lens<Target = u8> (root pitch class), for the label
  ) -> Handle<Self>
  ```
  Build on `ParamWidgetBase` (like `Knob`) for the toggle behavior (click → toggle the bool via begin/set/end). Draw a rounded rect filled with `theme::ACCENT` when on, dark when off, with the note-name text centered. Use a vizia `Binding` on `root` so the label recomputes `note_name_pc((root + degree) % 12)` when root changes.
- Add a helper `pub fn pitch_class_name(pc: u8) -> &'static str` (C, C#, …) — or reuse `daudio_dsp::notes` (daudio-ui would then depend on daudio-dsp; acceptable, or inline the 12 names to avoid the dep — prefer inline, it's trivial).

> This is the fiddliest widget (reactive label + click-toggle). Match `ParamSlider`/`ParamButton` in the pinned `nih_plug_vizia` for the toggle gesture and `Binding` usage. Verify against those sources; adjust signatures to what compiles.

Gate: `cargo build -p daudio-ui`; clippy/fmt clean. Commit `feat(ui): add NoteToggle widget with root-reactive label`.

---

## Task 8: plugin editor — scale editor + presets + knobs + readout

**Files:** `plugins/pitch-to-midi/src/lib.rs` (editor), `crates/daudio-ui/src/theme.css` (styles).

Implement `DaudioAudioToMidi::editor` via `daudio_ui::create_editor`:
- Title "daudio Pitch2MIDI".
- **Root selector:** a compact control setting `root` (a small dropdown or a `Knob`-like stepper bound to the `EnumParam`; simplest is nih_plug_vizia's `ParamButton`/a labeled `ParamSlider` over the enum — pick what's cleanest).
- **Scale editor:** an `HStack` of 12 `NoteToggle`s, `degree` = 0..11, each bound to the matching `degree_i` bool and the shared `root` lens (from `DaudioData::params` → `.root`).
- **Preset buttons:** a row of buttons (Chromatic, Major, Minor, Maj Pent, Min Pent, Blues, Clear). Each button, on press, uses the param setter to write the 12 `degree_i` bools to the preset pattern (relative to root — the pattern is degree-relative, so root need not change). Define the patterns as `const [bool; 12]` arrays.
- **Knobs:** `Knob` for `sensitivity` and `hold` (reuse `ParamControl`).
- **Readout:** a `Label` bound (via a timer-repainted view or a `Binding`) to the readout channels showing "in: A3  →  out: A3". Reuse the timer/atomic pattern from `Meter`, or a small `Binding` polling — simplest is a tiny custom view that reads the `AtomicI32`s and draws text, driven by a repaint timer like `Meter`.
- Set `editor_state` size to fit (e.g. 640×220).

Gate: `cargo build -p pitch-to-midi` + `--bin standalone`; `cargo test --workspace`; clippy/fmt; bundle. MANUAL (human): standalone opens; toggles show note names that relabel when root changes; presets set the toggles. Commit `feat(pitch-to-midi): scale-editor GUI with presets and note readout`.

---

## Task 9: `daudio-preview` — `run_analyzer` (prints emitted MIDI)

**Files:** `crates/daudio-preview/src/lib.rs` (add `run_analyzer`), `plugins/pitch-to-midi/src/bin/demo.rs` (shim).

- Add `pub fn run_analyzer<A: DaudioAudioToMidi + Default>()` mirroring `run`: parse args, read a WAV via the existing `read_wav`, feed samples through the analyzer's real path, and collect emitted events with sample timestamps, printing `t=0.42s  NoteOn A3 vel=0.71` / `NoteOff A3`. No cpal needed (offline). Usage:
  ```text
  demo <input.wav>          # print detected/emitted MIDI notes
  ```
  (No-arg tone mode is optional; the file mode is the point.)
- The analyzer's `process_sample(mono, timing, emit)` is called per sample with `timing` = sample index within a synthetic block (use a running sample counter for the printed time: `t = n / sr`). Sum stereo→mono using the same convention as the macro.
- `plugins/pitch-to-midi/src/bin/demo.rs`:
  ```rust
  fn main() {
      daudio_preview::run_analyzer::<pitch_to_midi::PitchToMidi>();
  }
  ```
  Add `daudio-preview = { path = "../../crates/daudio-preview" }` to the plugin Cargo.toml.
- Use `daudio_dsp::notes::note_name` for the printed note names.

Gate (VERIFIABLE OFFLINE HERE): generate a WAV of a steady 220 Hz tone (a few seconds), run `cargo run -p pitch-to-midi --bin demo -- tone.wav`, and confirm it prints a `NoteOn A3` (MIDI 57) after the debounce, and a `NoteOff` at the end. `cargo test --workspace` green; clippy/fmt clean. Commit `feat(preview): add run_analyzer for audio→MIDI plugins`.

---

## Definition of Done

- `daudio-dsp`: `quantize`/`freq_to_midi`/`note_name` + `PitchTracker`, unit-tested.
- `daudio-sdk`: `DaudioAudioToMidi` trait; `midi_out` macro mode (filter + synth unchanged).
- `plugins/pitch-to-midi`: builds, bundles, runs standalone; trigger + quantizer wired.
- `daudio-ui`: `NoteToggle` with root-reactive labels; scale-editor GUI with presets + readout.
- `daudio-preview`: `run_analyzer` prints emitted notes; verified on a 220 Hz tone → NoteOn A3.
- `cargo test --workspace` green; clippy `-D warnings` clean; fmt clean.

## Follow-up (not this plan)

- Worker-thread detection (strict RT-safety).
- Pitch-bend output; velocity-curve shaping.
- MIDI file export from the preview.
- Note-range clamp and MIDI-channel param.
