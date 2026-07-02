# daudio-dsp

Pure, host-agnostic DSP primitives for the daudio suite. **No nih-plug dependency** — every
type is plain Rust and unit-tested, so it builds and tests headlessly and is reusable by any
plugin.

## Contents

- `gain` — dB ↔ linear conversions (`db_to_gain`, `gain_to_db`).
- `processor` — the `Processor` trait (`set_sample_rate` / `reset` / `process_sample` / `process_block`).
- `smoother` — `OnePole` parameter smoother.
- `biquad` — `BiquadLowpass` (RBJ cookbook, transposed direct form II).
- `oscillator` — `Oscillator` (sine, saw).
- `adsr` — linear `Adsr` envelope.
- `notes` — MIDI/scale math: `freq_to_midi`, `note_name`, `quantize` (root + 12-bit degree mask), `bend_value`.
- `pitch` — `PitchTracker`: windowed monophonic pitch detection (wraps the `pitch-detection`
  crate) running on a worker thread, so the audio thread only pushes samples and reads a result atomic.

## Conventions

- Real-time safe: constructors size their buffers; `process_*` never allocates or locks.
- Primitives assume a 48 kHz default; callers **must** call `set_sample_rate` before processing
  (plugins do this in `activate`).
