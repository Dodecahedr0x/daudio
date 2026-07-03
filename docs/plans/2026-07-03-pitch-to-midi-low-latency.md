# Pitch2MIDI Low-Latency Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Cut Pitch2MIDI note-on latency for voice/lead (≈ C3+) from ~88 ms to ~25 ms by shortening the detection window/hop and adding a confidence-aware trigger that commits fast on high-clarity detections.

**Architecture:** `daudio-dsp::PitchTracker` shrinks its window (2048→1024) and hop (256→128) and now publishes detection **clarity** alongside frequency (packed into one `AtomicU64`, so the audio thread reads a consistent pair). The plugin's `Trigger` becomes clarity-aware: a new note with high clarity commits on the first hop (with a semitone-jump guard); marginal clarity falls back to the existing Hold debounce.

**Tech Stack:** Rust nightly, `daudio-dsp` (pure DSP), `pitch-detection` 0.3 (McLeod, exposes `Pitch { frequency, clarity }`), the `pitch-to-midi` plugin.

**Reference:** design at `docs/plans/2026-07-03-pitch-to-midi-low-latency-design.md`. Use superpowers:test-driven-development. Run rs-check (fmt + `cargo clippy --workspace -- -D warnings` + tests) after each task; ignore the pre-existing `block v0.1.6` future-incompat warning.

**Sequencing note:** changing `Detection`'s shape (Task 1) would break the plugin, and changing `Trigger::on_hop`'s signature (Task 2) would break its caller. Each task therefore updates its call sites so the whole workspace stays green after every commit.

---

## Task 1: `daudio-dsp` — shorter window/hop + publish clarity

**Files:**
- Modify: `crates/daudio-dsp/src/pitch.rs`
- Modify: `plugins/pitch-to-midi/src/lib.rs` (only the two `Detection::Pitch(..)` patterns, to keep it compiling)

**Step 1: change constants.** In `pitch.rs`:
```rust
const WINDOW: usize = 1024;
const PADDING: usize = WINDOW / 2;
pub const HOP: usize = 128;
```
(`POWER_THRESHOLD` / `CLARITY_THRESHOLD` unchanged.)

**Step 2: extend `Detection` to carry clarity.**
```rust
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Detection {
    Pitch { freq: f32, clarity: f32 },
    NoPitch,
}
```

**Step 3: add a pack/unpack helper (testable) and switch the result channel to `AtomicU64`.** At the top of `pitch.rs` change the import `AtomicU32` → `AtomicU64`, and add:
```rust
/// Pack a detection frequency + clarity into one u64 so the audio thread reads a
/// consistent pair with a single atomic load. A `NaN` frequency means "no pitch".
fn pack(freq: f32, clarity: f32) -> u64 {
    ((freq.to_bits() as u64) << 32) | (clarity.to_bits() as u64)
}
fn unpack(bits: u64) -> (f32, f32) {
    (f32::from_bits((bits >> 32) as u32), f32::from_bits(bits as u32))
}
const NO_PITCH_BITS: u64 = 0; // (freq_bits high) — actual value set via pack(NAN, 0.0)
```
> Note: `NO_PITCH_BITS` is just documentation; use `pack(f32::NAN, 0.0)` wherever you need the "no pitch" sentinel (do NOT rely on the literal `0`).

- `PitchTracker.result` field type: `Arc<AtomicU32>` → `Arc<AtomicU64>`.
- `PitchTracker::new`: `result: Arc::new(AtomicU64::new(pack(f32::NAN, 0.0)))`.
- `reset`: `self.result.store(pack(f32::NAN, 0.0), Ordering::Relaxed);`.

**Step 4: `PitchDetectorCore::push` returns clarity.** Change its tail:
```rust
Some(match pitch {
    Some(p) => Detection::Pitch { freq: p.frequency, clarity: p.clarity },
    None => Detection::NoPitch,
})
```

**Step 5: worker packs freq+clarity.** In `spawn_worker`'s loop, replace the `match det { ... }` that computed `bits` with:
```rust
if let Some(det) = core.push(sample) {
    let bits = match det {
        Detection::Pitch { freq, clarity } => pack(freq, clarity),
        Detection::NoPitch => pack(f32::NAN, 0.0),
    };
    result.store(bits, Ordering::Relaxed);
}
```

**Step 6: `PitchTracker::push` unpacks.** Replace its tail:
```rust
let (freq, clarity) = unpack(self.result.load(Ordering::Relaxed));
Some(if freq.is_nan() {
    Detection::NoPitch
} else {
    Detection::Pitch { freq, clarity }
})
```

**Step 7: keep the plugin compiling.** In `plugins/pitch-to-midi/src/lib.rs`, update the two `Detection::Pitch(..)` patterns (destructure, keep the `freq` name; ignore clarity for now — Task 2 uses it):
- In `process_sample`'s match arm (was `Detection::Pitch(f) if gated =>`):
  ```rust
  Detection::Pitch { freq, .. } if gated => {
      let midi = notes::freq_to_midi(freq);
      self.detected.store(midi, Relaxed);
      notes::quantize(midi, self.params.root_pc(), self.params.degree_mask())
  }
  ```
- In the pitch-bend block (was `if let (Some(note), Detection::Pitch(f)) = ...`):
  ```rust
  if let (Some(note), Detection::Pitch { freq: f, .. }) = (self.active_note, detection) {
  ```

**Step 8: update the `pitch.rs` unit tests to the new shape and add clarity coverage.** Replace the existing `detects_a_sine_frequency` / `silence_is_no_pitch` bodies to match `Detection::Pitch { freq, clarity }`, and:
- Assert the sine test detects **131 Hz** (C3) within ±3 Hz at the 1024 window (change the test frequency to 131.0), and `clarity > 0.8`.
- Add `pack_unpack_roundtrips`:
  ```rust
  #[test]
  fn pack_unpack_roundtrips() {
      let (f, c) = unpack(pack(261.63, 0.93));
      assert!((f - 261.63).abs() < 1e-2 && (c - 0.93).abs() < 1e-3);
      assert!(unpack(pack(f32::NAN, 0.0)).0.is_nan());
  }
  ```
  (Keep the existing 220 Hz threaded-tracker test working — update its `Detection::Pitch(f)` match to `Detection::Pitch { freq: f, .. }`.)

**Step 9: verify.** `cargo nextest run -p daudio-dsp` (C3 sine detected ±3 Hz, clarity high, roundtrip passes). If the 1024 window fails to detect 131 Hz cleanly (octave error or low clarity), that's a real signal — try clarity/power thresholds, but do NOT loosen the ±3 Hz tolerance to hide an octave error; report if it can't be met. Then `cargo build -p pitch-to-midi`, `cargo clippy --workspace -- -D warnings`, `cargo fmt`.

**Step 10: commit** `feat(dsp): shorter pitch window/hop and publish detection clarity`.

---

## Task 2: confidence-aware `Trigger`

**Files:**
- Modify: `plugins/pitch-to-midi/src/trigger.rs`
- Modify: `plugins/pitch-to-midi/src/lib.rs` (pass clarity into `on_hop`; lower Hold default)

**Step 1: write failing tests first.** In `trigger.rs` tests, update the `collect` helper to pass a clarity, and add the new cases. Full new test module:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    // clarity defaults high unless a test needs marginal
    fn collect(t: &mut Trigger, target: Option<i32>, clarity: f32, vel: f32) -> Vec<NoteAction> {
        let mut v = Vec::new();
        t.on_hop(target, clarity, vel, &mut |a| v.push(a));
        v
    }

    #[test]
    fn high_clarity_commits_on_first_hop() {
        let mut t = Trigger::new(); // hold_hops = 2
        // First hop, no note held, high clarity -> immediate commit.
        assert_eq!(collect(&mut t, Some(60), 0.95, 0.8),
            vec![NoteAction::On { note: 60, velocity: 0.8 }]);
    }

    #[test]
    fn low_clarity_still_debounces() {
        let mut t = Trigger::new();
        assert!(collect(&mut t, Some(60), 0.5, 0.8).is_empty());        // hop 1
        assert_eq!(collect(&mut t, Some(60), 0.5, 0.8),                 // hop 2 -> commit
            vec![NoteAction::On { note: 60, velocity: 0.8 }]);
    }

    #[test]
    fn high_clarity_big_jump_from_held_note_debounces() {
        let mut t = Trigger::new();
        collect(&mut t, Some(60), 0.95, 0.8); // 60 committed immediately (no held note)
        // A far, high-clarity candidate must NOT fast-commit (octave-error guard).
        assert!(collect(&mut t, Some(72), 0.95, 0.8).is_empty());      // 12 semis away -> debounce
        assert_eq!(collect(&mut t, Some(72), 0.95, 0.8),
            vec![NoteAction::Off { note: 60 }, NoteAction::On { note: 72, velocity: 0.8 }]);
    }

    #[test]
    fn gate_close_releases_active_note() {
        let mut t = Trigger::new();
        collect(&mut t, Some(60), 0.95, 0.8); // immediate commit
        assert_eq!(collect(&mut t, None, 0.0, 0.0), vec![NoteAction::Off { note: 60 }]);
    }
}
```
Run `cargo nextest run -p pitch-to-midi trigger` → FAIL (signature mismatch / logic missing).

**Step 2: implement.** Add a fast-clarity constant and a near-jump guard, and thread clarity through `on_hop`:
```rust
/// A new candidate at or above this clarity commits without waiting out the
/// Hold debounce — unless it is a large jump from the currently-held note.
const CLARITY_FAST: f32 = 0.9;
/// Max semitone distance from the held note for a fast (undebounced) commit.
const FAST_MAX_JUMP: i32 = 7;
```
Replace `on_hop`:
```rust
pub fn on_hop(
    &mut self,
    target: Option<i32>,
    clarity: f32,
    velocity: f32,
    emit: &mut dyn FnMut(NoteAction),
) {
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

    // Track how long this candidate has persisted.
    if self.candidate == Some(target) {
        self.candidate_hops += 1;
    } else {
        self.candidate = Some(target);
        self.candidate_hops = 1;
    }

    // Fast path: high clarity commits immediately, unless it's a big jump from a
    // currently-held note (guards against a confident octave error).
    let near = match self.active {
        Some(n) => (target - n as i32).abs() <= FAST_MAX_JUMP,
        None => true,
    };
    let fast = clarity >= CLARITY_FAST && near;

    if fast || self.candidate_hops >= self.hold_hops {
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
```
Run `cargo nextest run -p pitch-to-midi trigger` → PASS.

**Step 3: pass clarity from the plugin.** In `plugins/pitch-to-midi/src/lib.rs` `process_sample`:
- Extract clarity in the match. Change the arm to also yield clarity, e.g.:
  ```rust
  let (target, clarity) = match detection {
      Detection::Pitch { freq, clarity } if gated => {
          let midi = notes::freq_to_midi(freq);
          self.detected.store(midi, Relaxed);
          (notes::quantize(midi, self.params.root_pc(), self.params.degree_mask()), clarity)
      }
      _ => {
          self.detected.store(-1, Relaxed);
          (None, 0.0)
      }
  };
  ```
- Update the `on_hop` call to pass `clarity`: `self.trigger.on_hop(target, clarity, velocity, &mut |action| ...)`.
- The pitch-bend block still uses `Detection::Pitch { freq: f, .. }` (unchanged from Task 1).

**Step 4: lower the Hold default.** In `PitchToMidiParams::default`, change the `hold` FloatParam default `40.0` → `25.0` (keep the 10–200 ms range and `" ms"` unit).

**Step 5: verify.** `cargo build -p pitch-to-midi --bin standalone`; `cargo test --workspace`; `cargo clippy --workspace -- -D warnings`; `cargo fmt`.

**Step 6: commit** `feat(pitch-to-midi): confidence-aware fast note commit + lower Hold default`.

---

## Task 3: latency check + final gate

**Files:** none (verification only).

**Step 1: build a synthetic C4 tone that starts at t=0** (a few seconds, loud, harmonic-rich):
```bash
python3 - <<'PY'
import wave, struct, math
sr=44100; f=261.63
with wave.open('/tmp/c4.wav','w') as w:
    w.setnchannels(1); w.setsampwidth(2); w.setframerate(sr)
    for i in range(int(sr*2)):
        v=sum(math.sin(2*math.pi*f*k*i/sr)/k for k in range(1,8))*0.5
        w.writeframes(struct.pack('<h', max(-32768,min(32767,int(v*32767)))))
PY
```

**Step 2: read the note-on latency from the demo.** (The demo prints `t=<sec>  NoteOn ...`. Because the analyzer's offline mode feeds faster than real time, it may print fewer events; if it prints a NoteOn, its `t` ≈ the algorithmic latency.)
```bash
cargo run -p pitch-to-midi --bin demo -- /tmp/c4.wav 2>&1 | grep -v "block v0.1" | grep NoteOn | head -1
```
Expected: a `NoteOn C4` whose timestamp is well under the old ~0.08 s (target ≈ 0.025–0.04 s). Report the number. If no NoteOn prints offline (worker can't keep up with faster-than-real-time feeding), that's an accepted limitation of the offline path — note it; the real-time/standalone path is what benefits. Do NOT force it.

**Step 3: full gate.** `cargo test --workspace` (all pass), `cargo clippy --workspace -- -D warnings` (clean), `cargo fmt --check` (clean), `cargo xtask bundle pitch-to-midi --release` (bundles).

**Step 4 (optional): human check.** `cargo run -p pitch-to-midi --bin standalone`, sing/play a clean note — it should track noticeably faster than before.

**Step 5: commit** (if any doc/notes changed) `docs: note measured Pitch2MIDI latency`, else skip.

---

## Definition of Done

- `daudio-dsp`: window 1024 / hop 128; `Detection::Pitch { freq, clarity }`; worker publishes both via a packed `AtomicU64`; C3 sine still detected ±3 Hz with high clarity; pack/unpack test passes.
- `Trigger`: high-clarity new notes commit on hop 1 with a ±7-semitone fast-path guard; marginal clarity debounces via Hold; gate-close immediate — all covered by tests.
- Plugin passes clarity through; Hold default 25 ms; builds, tests green, clippy `-D warnings` clean, fmt clean, bundles.
- Measured demo note-on latency reported (or the offline limitation noted).

## Out of scope (YAGNI)

- Host latency reporting; a user-facing window/latency selector; polyphony; bass-range tracking.
