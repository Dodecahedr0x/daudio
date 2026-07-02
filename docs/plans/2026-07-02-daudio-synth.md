# daudio Synth — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add the synth half of the SDK — oscillator + ADSR DSP primitives, a generic `VoiceManager`/`Voice` polyphony layer, a `DaudioSynth` trait, a synth codegen mode in the `#[daudio_plugin]` macro — and prove it with a polyphonic subtractive synth plugin (osc → per-voice lowpass → ADSR amp, with the amp envelope also opening the filter).

**Architecture:** Pure DSP primitives (`Oscillator`, `Adsr`) go in `daudio-dsp` (unit-tested, no nih-plug). The polyphony layer (`Voice` trait + `VoiceManager<V>`) and the `DaudioSynth` trait go in `daudio-sdk`; `VoiceManager` is pure allocation/stealing logic and is unit-testable. The `#[daudio_plugin]` macro gains a `midi`/synth mode that generates a `Plugin` with MIDI input, no audio input, and a sample-accurate note-event `process` loop that delegates to `DaudioSynth`. The synth plugin composes existing primitives (biquad from `daudio-dsp`, knobs from `daudio-ui`).

**Tech Stack:** Rust nightly, nih-plug + nih_plug_vizia (pinned rev `f36931f7af4646065488a9845d8f8c2f95252c23`), syn/quote (macro).

**REFERENCE:** nih-plug's `plugins/examples/poly_mod_synth` (or `sine`) in the pinned checkout shows the MIDI-input plugin shape: `MIDI_INPUT`, `main_input_channels: None`, and the sample-accurate `next_event()` loop in `process`. Read it before Task 5.

---

## Scope

In scope: `Oscillator` (saw + sine), `Adsr`; `Voice` + `VoiceManager` (poly, oldest-note stealing); `DaudioSynth` trait; macro synth mode; a `plugins/synth` plugin (osc, filter cutoff/resonance, filter env amount, ADSR A/D/S/R, master gain) with a knob editor.

Out of scope: anti-aliased oscillators (naive first), second oscillator, LFOs, unison, velocity→filter, pitch bend / mod wheel, per-sample param smoothing beyond what exists. YAGNI.

---

## Task 1: `Oscillator` primitive (daudio-dsp)

**Files:** create `crates/daudio-dsp/src/oscillator.rs`; add `pub mod oscillator;` to lib.rs.

TDD. A phase-accumulator oscillator with selectable waveform.
```rust
#[derive(Clone, Copy, PartialEq)]
pub enum Waveform { Sine, Saw }

pub struct Oscillator {
    sample_rate: f32,
    phase: f32,       // [0, 1)
    freq_hz: f32,
    waveform: Waveform,
}
```
Methods: `new(waveform) -> Self` (48k default; document set_sample_rate contract like the other primitives), `set_sample_rate(sr)`, `set_frequency(hz)`, `set_waveform(w)`, `reset()` (phase = 0), `next_sample() -> f32` (advance phase by freq/sr, wrap; Sine = `(2π·phase).sin()`, Saw = `2·phase − 1`).

**Tests (write first):**
- `sine_is_bounded`: 10k samples at 440 Hz, all in [-1.001, 1.001], and both a positive and a negative sample occur.
- `saw_ramps_and_wraps`: saw output increases then jumps down (detect at least one large negative jump across a cycle).
- `frequency_affects_period`: zero-crossings-per-second of a sine roughly match 2·freq (loose bound).
- `reset_zeroes_phase`: after reset, first sine sample ≈ 0.

Commit: `feat(dsp): add oscillator primitive (sine, saw)`

---

## Task 2: `Adsr` envelope (daudio-dsp)

**Files:** create `crates/daudio-dsp/src/adsr.rs`; add `pub mod adsr;` to lib.rs.

TDD. A linear ADSR (attack→decay→sustain→release) driven per sample.
```rust
pub struct Adsr {
    sample_rate: f32,
    stage: Stage,        // Idle, Attack, Decay, Sustain, Release
    level: f32,          // current output [0,1]
    attack_s: f32, decay_s: f32, sustain: f32, release_s: f32,
}
```
Methods: `new()`, `set_sample_rate(sr)`, `set_params(attack_s, decay_s, sustain, release_s)`, `trigger()` (→ Attack), `release()` (→ Release from current level), `is_active() -> bool` (false only in Idle), `next_sample() -> f32` (advance the stage machine linearly, return level).

Stage logic: Attack ramps 0→1 over attack_s then →Decay; Decay ramps 1→sustain over decay_s then →Sustain (holds sustain); Release ramps current→0 over release_s then →Idle. Guard zero-length stages (jump instantly). Clamp times to a small minimum.

**Tests (write first):**
- `idle_is_silent_and_inactive`: fresh Adsr → `next_sample()==0`, `!is_active()`.
- `attack_rises_to_one`: trigger, run attack_s worth of samples, level ≈ 1.0.
- `sustain_holds`: trigger, run past attack+decay, level ≈ sustain and stays.
- `release_falls_to_zero_then_idle`: from sustain, release, run release_s samples → level ≈ 0 and `!is_active()`.
- `retrigger_from_idle`: after full release, trigger again rises from ~0.

Commit: `feat(dsp): add ADSR envelope`

---

## Task 3: `Voice` trait + `VoiceManager` (daudio-sdk)

**Files:** create `crates/daudio-sdk/src/voice.rs`; add `pub mod voice;` + re-exports to lib.rs.

This is pure polyphony logic — UNIT TEST IT with a tiny fake voice. No nih-plug needed here.
```rust
pub trait Voice: Default {
    fn set_sample_rate(&mut self, sr: f32);
    fn note_on(&mut self, note: u8, velocity: f32);
    fn note_off(&mut self);            // enter release
    fn is_active(&self) -> bool;       // false once fully released → reusable
    fn note(&self) -> u8;              // MIDI note this voice is playing
    fn render(&mut self) -> f32;       // one mono sample
}

pub struct VoiceManager<V: Voice> { voices: Vec<V>, order: Vec<usize>, /* age for stealing */ }
```
Methods:
- `new(max_voices: usize)` — allocate `max_voices` default voices.
- `set_sample_rate(sr)` — fan out.
- `note_on(note, velocity)` — pick an inactive voice; if none, STEAL the oldest active voice; call its `note_on`; record age/order.
- `note_off(note)` — call `note_off` on the (most recent) active voice matching `note`.
- `render() -> f32` — sum `render()` of active voices.
- `reset()` — reset all voices to inactive.
- `for_each_active(&mut self, f: impl FnMut(&mut V))` — for per-block param pushes into live voices.

**Tests (write first)** with a `struct TestVoice { active: bool, note: u8, samples_left: i32 }` implementing `Voice` (render returns 1.0 while active; note_off/decrement makes it inactive):
- `note_on_activates_a_voice`: after `note_on(60, 1.0)`, exactly one active voice with note 60; `render()` > 0.
- `note_off_releases_matching_note`: note_on then note_off(60) → drives the voice inactive.
- `polyphony_sums_voices`: two note_ons → `render()` sums both.
- `stealing_when_full`: manager with 2 voices, three note_ons → still ≤2 active, the oldest was reused (its note replaced).
- `render_sums_only_active`.

Commit: `feat(sdk): add Voice trait and polyphonic VoiceManager`

---

## Task 4: `DaudioSynth` trait (daudio-sdk)

**Files:** modify `crates/daudio-sdk/src/effect.rs` (or a new `synth.rs`); export from lib.rs + prelude.

```rust
pub trait DaudioSynth: Send {
    type Params: Params + Default;
    fn activate(&mut self, sample_rate: f32);
    fn reset(&mut self) {}
    /// Once per block, before rendering: push current param values into voices.
    fn pre_block(&mut self) {}
    fn note_on(&mut self, note: u8, velocity: f32);
    fn note_off(&mut self, note: u8);
    /// Render one stereo frame (sum voices + master gain). Called per sample.
    fn render_frame(&mut self) -> (f32, f32);
}
```
The annotated struct still needs `params: Arc<Self::Params>`. Add `editor()` default `None` too (synths can have editors). Commit: `feat(sdk): add DaudioSynth trait`

---

## Task 5 + 6: Macro synth mode + the synth plugin (developed together)

Develop these together (macro codegen exercised by the real synth), like the filter refactor.

### Task 5 — macro synth mode (`crates/daudio-sdk-macros/src/lib.rs`)
Add a way to select synth codegen. Add an attribute key `midi = true` (default false). When set, generate a synth `impl Plugin` instead of the effect one:
- `const MIDI_INPUT: MidiConfig = MidiConfig::Basic;`
- `AUDIO_IO_LAYOUTS`: `main_input_channels: None`, `main_output_channels: NonZeroU32::new(2)`.
- `initialize` → `DaudioSynth::activate`; `reset` → `DaudioSynth::reset`; `editor` → `DaudioSynth::editor`.
- `process`: the sample-accurate event loop (match nih-plug's poly synth example):
  ```
  let mut next_event = context.next_event();
  for (sample_id, mut channel_samples) in buffer.iter_samples().enumerate() {
      while let Some(event) = next_event {
          if event.timing() > sample_id as u32 { break; }
          match event {
              NoteEvent::NoteOn { note, velocity, .. } => synth.note_on(note, velocity),
              NoteEvent::NoteOff { note, .. }          => synth.note_off(note),
              _ => {}
          }
          next_event = context.next_event();
      }
      // (call pre_block once per block — before the sample loop, not here)
      let (l, r) = <Self as DaudioSynth>::render_frame(self);
      if channel_samples.len() >= 2 { *channel_samples.get_mut(0).unwrap()=l; *channel_samples.get_mut(1).unwrap()=r; }
  }
  ```
  Call `pre_block` once before the loop. Route ALL paths through `::daudio_sdk::nih_plug::...` as the effect codegen does. Verify `NoteEvent`, `MidiConfig`, `event.timing()`, `context.next_event()` against the pinned rev's example.
The macro decides effect-vs-synth by the `midi` flag and emits the corresponding trait bound (`DaudioEffect` vs `DaudioSynth`). Keep effect codegen unchanged. Commit: `feat(sdk): add synth (MIDI) mode to daudio_plugin macro`

### Task 6 — `plugins/synth`
- `plugins/synth/Cargo.toml`: `daudio-sdk`, `daudio-dsp`, `daudio-ui`, `nih_plug` (features `standalone`), cdylib+lib. Add to workspace members. Standalone bin like the filter.
- `SynthVoice` (implements `daudio_sdk::Voice`): `Oscillator` → `BiquadLowpass` → `Adsr` (amp). Fields for waveform, base cutoff, resonance, env amount, ADSR times. `note_on` sets osc frequency from MIDI note (`440 * 2^((note-69)/12)`), triggers ADSR. `render`: `osc.next_sample()` → filter (cutoff = base_cutoff modulated by `env_level * env_amount`) → `* amp_env.next_sample()`. `is_active` = amp env active. Provide setters the synth calls in `pre_block` (`set_waveform`, `set_filter`, `set_env_amount`, `set_adsr`).
- `SynthParams`: waveform (EnumParam or bool/int), filter cutoff (hz_param), resonance (FloatParam), filter env amount (0..1), attack/decay/sustain/release (FloatParam, seconds/level), master gain (db_gain_param), plus `#[persist] editor_state`.
- `Synth` struct: `params: Arc<SynthParams>`, `voices: VoiceManager<SynthVoice>`, `gain smoother` optional. Annotate `#[daudio_plugin(name="daudio Synth", ..., clap_id="com.daudio.synth", vst3_id="daudioSynth00001"(16 bytes!), clap_features=[Instrument, Synthesizer, Stereo], vst3_categories=[Instrument, Synth], midi = true)]`. Implement `DaudioSynth`: `activate` sets sr on voice manager; `pre_block` reads params and pushes into active voices via `for_each_active`; `note_on/note_off` delegate to `voices`; `render_frame` = `let s = voices.render() * gain; (s, s)`; `editor` builds a knob panel via daudio-ui for the main params.
- vst3_id must be exactly 16 bytes — pick e.g. `"daudioSynth00001"` and COUNT it.

### Acceptance gate (Tasks 5+6)
- `cargo build -p synth`, `cargo build -p synth --bin standalone` compile.
- `cargo build -p filter` still compiles (effect codegen unchanged).
- `cargo test --workspace` — all prior tests + new dsp/voice tests pass.
- `cargo clippy --workspace -- -D warnings` clean; `cargo fmt --check` clean.
- `cargo xtask bundle synth --release` produces `daudio Synth.vst3` + `.clap`.
- MANUAL (human): `cargo run -p synth --bin standalone`, play MIDI (or the on-screen keyboard if available) → hear polyphonic notes with filter + envelope; the editor shows knobs.
Commit Task 6: `feat(synth): polyphonic subtractive synth on the SDK`

---

## Definition of Done

- `daudio-dsp` has `Oscillator` + `Adsr` (unit-tested); `daudio-sdk` has `Voice`/`VoiceManager` (unit-tested) + `DaudioSynth`.
- `#[daudio_plugin]` supports `midi = true` synth codegen; the filter (effect) is unchanged.
- `plugins/synth` builds, bundles, runs standalone, and is polyphonic with filter + ADSR.
- `cargo test --workspace` green; clippy `-D warnings` clean; fmt clean.

## Follow-up (not this plan)

- Anti-aliased oscillators (PolyBLEP); second osc / unison; LFO.
- Pitch bend + mod wheel; velocity→filter.
- A Meter widget; wire the theme accent.
