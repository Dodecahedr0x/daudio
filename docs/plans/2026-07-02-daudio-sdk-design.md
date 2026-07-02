# daudio — VST Plugin Suite & Reusable SDK

**Date:** 2026-07-02
**Status:** Design approved, ready for implementation planning

## Goal

Build a suite of VST plugins in Rust, and in doing so create a reusable SDK
that makes creating each additional plugin as straightforward as possible. The
SDK is the primary deliverable; the plugins are how we prove and dogfood it.

## Key Decisions

| Area          | Decision |
|---------------|----------|
| Foundation    | Build on **nih-plug** (exports VST3 + CLAP, handles hosting/param plumbing) |
| Plugin types  | Effects, synths/instruments, MIDI utilities (analyzers out of scope for now) |
| GUI           | Shared **Vizia** widget library with a common theme |
| DSP           | **Hybrid** — build our own audio primitives, wrap crates for FFT/resampling/math |
| First plugin  | Simple filter/gain effect (validates the full pipeline) |
| Platforms     | macOS + Windows (Linux deferred) |

## Architecture — Workspace Layout

A Cargo workspace monorepo so the SDK and plugins evolve together.

```
daudio/
├── Cargo.toml            # workspace root
├── crates/
│   ├── daudio-sdk/       # glue over nih-plug; re-exports dsp + ui
│   ├── daudio-dsp/       # DSP primitives (no nih-plug dependency)
│   └── daudio-ui/        # Vizia widget library + shared theme
└── plugins/
    └── filter/           # first plugin — filter/gain effect
```

**Three SDK crates** (the minimum useful split — no more until a real need
appears):

- **`daudio-dsp`** — pure, host-agnostic math. No nih-plug dependency.
  Independently and headlessly testable.
- **`daudio-ui`** — Vizia widget library and shared visual theme. Depends on
  nih-plug's Vizia integration.
- **`daudio-sdk`** — glue over nih-plug: parameter helpers, plugin scaffolding
  trait/macro, MIDI helpers, voice management. Re-exports `daudio-dsp` and
  `daudio-ui` so a plugin adds one dependency.
- **`plugins/*`** — each plugin is its own crate producing VST3 + CLAP bundles.
  Thin: wire params → DSP → UI.

## The SDK Core — How a Plugin Gets Written

The SDK collapses the repetitive parts of nih-plug while staying out of the way
of the interesting parts (the DSP).

**`DaudioPlugin` trait** — a thinner, opinionated trait providing defaults for
what every plugin repeats (bus config, CLAP/VST3 metadata scaffolding,
param-to-smoothing wiring, editor state plumbing) and asking the author only for
what is unique:

- `Params` — the parameter set (still a nih-plug `Params` struct; the SDK adds
  helper constructors like `db_gain_param("Gain", -60.0..=6.0)`).
- `process(&mut self, buffer, ctx)` — DSP entry point, handed already-smoothed
  param values and a scratch/transport context.
- `editor()` — a Vizia view built from `daudio-ui` widgets (optional; a default
  generated editor exists).

**`daudio_plugin!` macro** — generates `impl Plugin`, `impl ClapPlugin`/
`impl Vst3Plugin`, the unique IDs, and the `nih_export_*!` calls from a small
metadata block. A plugin's `lib.rs` becomes: metadata + params + `process` +
editor.

**Guiding rule — additive, never a wall.** The SDK removes boilerplate but never
removes access. Raw nih-plug types are re-exported so authors can always drop
down for anything the SDK doesn't cover.

## DSP Layer (`daudio-dsp`)

Pure signal processing, no knowledge of nih-plug or VST. Unit-testable in
isolation, reusable across effects and synths.

**Core trait:**

```rust
pub trait Processor {
    fn set_sample_rate(&mut self, sr: f32);
    fn reset(&mut self);
    fn process_sample(&mut self, input: f32) -> f32;    // per-sample primitive
    fn process_block(&mut self, buf: &mut [f32]) { /* default: loop process_sample */ }
}
```

Per-sample is the primitive (simplest to write correctly); `process_block` has a
default plugins override when SIMD/block efficiency matters.

**Build ourselves:** biquad filters (LP/HP/BP/shelf/peak), one-pole smoothers,
ADSR envelopes, envelope followers, oscillators (saw/square/sine/triangle;
anti-aliasing later), delay lines, gain/dB utilities.

**Borrow (wrap, don't reinvent):** FFT (`realfft`/`rustfft`), sample-rate
conversion (`rubato`), math helpers — behind thin SDK-friendly wrappers for a
consistent style.

**Real-time-safety rules (enforced SDK-wide):**

- **No allocation or locking in `process`.** Buffers are sized in
  `set_sample_rate`/init.
- **`f32` mono primitives, composed for stereo/multichannel.** A primitive
  processes one channel; stereo is two instances. No channel-count assumptions.

**Testing:** each primitive gets unit tests — impulse/step responses, filter
cutoff vs. known coefficients, envelope timing. Correctness provable without a
DAW.

## UI Layer (`daudio-ui`)

Reusable, param-bound Vizia controls with a shared theme so every plugin looks
like it belongs to the suite.

- **Widgets:** rotary `Knob`, `Slider`, `Toggle`/button, `ComboBox`, `Meter`
  (peak/RMS), and a `ParamControl` wrapper binding any nih-plug param to a widget
  with automatic text entry, drag, scroll, and double-click-to-reset.
- **Theme:** one shared stylesheet plus a `Theme` struct (colors/fonts). A plugin
  picks an accent color and inherits the whole look; rebranding is a one-file
  change.
- **Layout helpers:** thin `row`/`column`/`panel` builders so an editor is
  declarative and short (~20–40 lines).

## Synth & MIDI Support (`daudio-sdk`)

The parts synths and MIDI utilities repeat, dormant for effect plugins:

- **Voice management:** a generic `VoiceManager<V: Voice>` handling note-on/off,
  allocation, stealing (oldest/quietest), and per-voice lifecycle. A synth author
  implements a single `Voice` (osc + envelope); the manager handles polyphony.
- **MIDI helpers:** typed note/CC/pitch-bend events surfaced from nih-plug's
  event stream, so MIDI-utility plugins iterate clean events instead of raw
  parsing.

You only pay for what you use — an effect never touches the voice manager.

## Testing, Preview & Dev Workflow

Three tiers of feedback:

1. **Unit tests** — DSP primitives in `daudio-dsp`.
2. **Offline test host** (`daudio-sdk::testing`) — headless harness:
   - `TestHost::new(plugin)` — instantiate with no audio backend or GUI.
   - `render(input: &[f32]) -> Vec<f32>` — push samples through `process`, get
     output. Feed impulses, sweeps, or WAV files.
   - Helpers to set params, send MIDI events, step blocks — so tests assert
     e.g. "gain at -6 dB halves amplitude" or "note-on produces sound".
   - WAV in/out helpers for eyeballing in an external editor.
3. **Standalone preview (the "no DAW" loop)** — the `daudio_plugin!` macro
   generates a per-plugin standalone binary via nih-plug's
   `nih_export_standalone!`, talking to CoreAudio/WASAPI/JACK with the real Vizia
   editor. `cargo run -p filter --bin standalone` opens the plugin as a live app.
   This is the primary iteration loop.

**Platforms & build:** macOS + Windows. CI matrix builds/tests both;
`cargo xtask bundle` (nih-plug's bundler) produces VST3/CLAP artifacts. macOS
code-signing/notarization deferred until distribution.

**Error handling:** init/config errors surface at load time via `Result`. The
audio thread never fails loudly — it degrades to silence/passthrough, never
panics or allocates.

## Explicitly Out of Scope (YAGNI)

- Analyzer/visualizer plugins (spectrum, scope).
- Linux support (deferred, not designed out).
- Code signing/notarization (deferred until distribution).
- More than three SDK crates (split further only when a real need appears).

## Suggested First Milestones

1. Workspace skeleton + three crates compiling.
2. `daudio-dsp`: biquad filter + gain, with unit tests.
3. `daudio-sdk`: `DaudioPlugin` trait + `daudio_plugin!` macro + offline
   `TestHost`.
4. `daudio-ui`: `Knob` + `ParamControl` + base theme.
5. `plugins/filter`: wire it together; runs in a DAW and as a standalone.
