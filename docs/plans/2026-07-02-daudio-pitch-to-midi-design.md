# daudio Pitch→MIDI — Design

**Date:** 2026-07-02
**Status:** Design approved, ready for implementation planning

## Goal

A plugin that listens to a **monophonic** audio input, detects its pitch, and
emits **MIDI notes**, quantized to a user-defined scale. The scale is a 12-note
mask relative to a selectable root; presets fill the mask and the user can then
toggle individual notes. This introduces a third plugin shape to the SDK —
**audio in → MIDI out** — alongside effects and synths.

## Key Decisions

| Area              | Decision |
|-------------------|----------|
| Scope             | **Monophonic** pitch tracking (one note at a time) |
| Detection         | Wrap the **`pitch-detection`** crate (McLeod method; gives a clarity score) |
| Scale model       | **Root param + 12 relative-degree toggles**; changing root rotates the allowed notes live |
| Presets           | GUI action (buttons) that write the 12 degree toggles; not a persisted param |
| Toggle labels     | **Absolute note names** (C, C♯, D…), relabeled live when the root changes |
| Trigger           | **Stable-pitch gate + debounce + level gate**; velocity from input level |
| SDK seam          | New `DaudioAudioToMidi` trait + `midi_out` macro mode |

## Architecture — the new SDK seam

A third trait beside `DaudioEffect` (audio→audio) and `DaudioSynth` (MIDI→audio):

```rust
pub trait DaudioAudioToMidi: Send {
    type Params: Params + Default;
    fn activate(&mut self, sample_rate: f32);
    fn reset(&mut self) {}
    /// Feed one mono input sample; push any MIDI events via `emit`.
    fn process_sample(&mut self, input: f32, emit: &mut dyn FnMut(NoteEvent<()>));
    fn editor(&mut self) -> Option<Box<dyn Editor>> { None }
}
```

Detection needs a window, so the plugin accumulates samples internally and runs
detection on a hop; `emit` fires note-on/off when the trigger state machine
decides.

**New macro mode** `midi_out = true` on `#[daudio_plugin(...)]` generates a
`Plugin` with:
- `AUDIO_IO_LAYOUTS` = **stereo in / stereo out, audio passed through** (nih-plug
  processes in place, so an input-only 0-output plugin has no buffer to read; we
  pass the audio through untouched and emit MIDI alongside it),
- `MIDI_OUTPUT = MidiConfig::Basic`,
- a `process` loop that sums each frame to mono, calls
  `process_sample(mono, timing, emit)`, forwards emitted events via
  `context.send_event(...)`, and leaves the audio frame unchanged.

The effect and synth codegen paths stay byte-for-byte unchanged; the mode is
selected by the `midi_out` flag exactly as `midi` selects the synth path.

**DSP placement.** The pitch detector wraps `pitch-detection` behind a small,
reusable `daudio-dsp::PitchTracker`. The quantizer is a pure `daudio-dsp`
function. The trigger state machine lives in the plugin.

**Plugin crate:** `plugins/pitch-to-midi` composes `PitchTracker` + quantizer +
trigger and provides the editor.

## Detection, Quantization & Trigger

**`daudio-dsp::PitchTracker`** — a thin wrapper over `pitch-detection`
(McLeod method). Owns a ring buffer; the plugin pushes samples and every **hop**
(≈256 samples) runs detection on the latest **window** (≈2048 samples ≈ 43 ms @
48 kHz). Returns `Option<(freq_hz, clarity)>` — `None` when no clear pitch.
Window/hop are internal constants (not exposed — YAGNI).

**Quantizer** — pure, fully testable:

```rust
fn quantize(midi_note: i32, root: u8, degree_mask: u16) -> Option<i32>
```

- `frequency → midi = round(69 + 12·log2(f/440))`.
- `degree = (midi - root).rem_euclid(12)`; if that bit is set in `degree_mask`,
  keep the note.
- Otherwise search outward (±1, ±2, …) for the nearest note whose degree bit is
  set; ties resolve upward.
- Empty mask → `None`.

**Trigger state machine** (in the plugin) — monophonic, at most one active note:
- **Level gate:** track input peak; below the `sensitivity` threshold → force
  note-off.
- **Debounce:** a newly detected quantized note must persist for the `hold` time
  (≈30–50 ms) before it is committed — kills jitter and octave-glitch spam.
- **Velocity:** captured from the input level at commit, mapped to 1–127.
- **Retrigger:** when the committed note changes → note-off (old) then note-on
  (new); on gate-close → note-off.

**Real-time safety:** detection now runs on a dedicated worker thread. The audio
thread only pushes samples into a lock-free ring (`rtrb`) and reads the latest
published frequency via an atomic, so it is strictly RT-safe — the possibly
allocating `get_pitch` FFT never runs on the audio thread. The quantizer and
trigger are allocation-free.

## Parameters & UI

**Parameters** (automatable, persisted):
- `root` — `EnumParam<Root>` (C, C♯, … B).
- `degree_0` … `degree_11` — **12 `BoolParam`s**, the scale-degree mask
  (degree 0 = root). The source of truth for the scale.
- `sensitivity` — `FloatParam` (dB), the input level gate.
- `hold` — `FloatParam` (ms), the debounce time.

**Presets are a GUI action, not a param:** buttons — Chromatic, Major, Minor,
Major Pentatonic, Minor Pentatonic, Blues, Clear — write the 12 degree toggles
via the param setter. Root is unchanged, so "Major" + root A gives A-major; the
user can then edit any degree.

**New widget `NoteToggle`** (in `daudio-ui`): a small labeled on/off button bound
to a `BoolParam`. Its label is the **absolute note name** of
`(root + degree_index) mod 12` — a reactive binding on the `root` param, so the
whole row relabels live when the root changes (C C♯ D… → D D♯ E…) while the
enabled pattern rotates with it. Twelve of them in a row form the scale editor.

**Editor layout:** root selector on the left; the 12 `NoteToggle`s in a row;
preset buttons beneath; `sensitivity`/`hold` knobs (reusing `Knob`) on the right;
and a **live readout** of the currently detected note name and the emitted
(quantized) note. The readout uses a `PeakLevel`-style atomic channel (audio→UI)
to publish the current detected/output note.

## Testing & Preview

**Unit tests (offline, the bulk of correctness):**
- **`quantize`** — table-driven: nearest-with-upward-tie, empty mask → `None`,
  every degree honored, octave wrapping.
- **Trigger state machine** — synthetic detection results: debounce (a one-hop
  blip never commits), level gate, retrigger on note change, velocity scaling.
- **`PitchTracker`** — feed a generated sine at a known frequency; assert
  detected Hz within a cent tolerance and high clarity; silence → `None`.

**Reusable analyzer preview** — extend the `daudio-preview` idea to audio→MIDI:
`run_analyzer::<A: DaudioAudioToMidi + Default>()` reads a WAV, feeds it through
the plugin's real path, and **prints the emitted MIDI events**
(`t=0.42s  NoteOn A3 vel=90`), optionally writing a `.mid` file. Every future
analyzer gets this via a one-line shim, and it is verifiable offline (feed a
220 Hz tone → expect A3).

**Manual/DAW:** route the plugin's MIDI output to any instrument; play or sing
and hear quantized notes.

**Error handling:** unclear pitch, silence, or an empty scale simply emit
nothing — never a wrong note.

## Out of Scope (YAGNI)

- Polyphonic detection.
- Pitch bend / glide / portamento output (notes retrigger, no bend).
- Exposed window/hop/detection-algorithm controls.
- MIDI file export beyond the simple preview writer (optional, deferred).
- Velocity-curve shaping beyond a linear level→velocity map.

## Suggested Milestones

1. `daudio-dsp`: `quantize` (+ tests) and `PitchTracker` wrapper (+ sine test).
2. `daudio-sdk`: `DaudioAudioToMidi` trait; `midi_out` macro mode (filter/synth
   unaffected).
3. `daudio-ui`: `NoteToggle` widget (root-reactive label).
4. `plugins/pitch-to-midi`: trigger state machine, params, editor.
5. `daudio-preview`: `run_analyzer` (prints emitted notes); verify on a tone WAV.
