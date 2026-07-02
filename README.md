# daudio

A suite of audio plugins written in Rust, built on a small reusable SDK. Each plugin
compiles to **VST3** and **CLAP** and can also run as a **standalone** app. The SDK does
the plugin plumbing so a new plugin is mostly parameters plus a short bit of DSP.

Built on [nih-plug](https://github.com/robbert-vdh/nih-plug) (VST3/CLAP export, parameter
handling) and [nih_plug_vizia](https://github.com/robbert-vdh/nih-plug) (GUI).

## Plugins

| Plugin | Type | What it does |
|--------|------|--------------|
| **daudio Filter** | Effect | Resonant low-pass filter with output gain and a level meter. |
| **daudio Synth** | Instrument | Polyphonic subtractive synth: oscillator → per-voice filter → ADSR. |
| **daudio Pitch2MIDI** | Analyzer | Monophonic pitch → MIDI, quantized to a user-defined scale. |

All three share one modern dark UI toolkit (glow knobs, meters, a scale keyboard) and one DSP core.

## Requirements

- **Rust nightly** (pinned via `rust-toolchain.toml` — nih-plug requires it).
- macOS or Windows. (Linux should work but isn't regularly tested here.)

## Build & install

Bundle a plugin into installable VST3 + CLAP:

```bash
cargo xtask bundle filter --release
cargo xtask bundle synth --release
cargo xtask bundle pitch-to-midi --release
```

Bundles land in `target/bundled/`, e.g. `daudio Filter.vst3` and `daudio Filter.clap`.
Copy them to your plugin folders:

- **VST3** — macOS: `~/Library/Audio/Plug-Ins/VST3/` · Windows: `%COMMONPROGRAMFILES%\VST3\`
- **CLAP** — macOS: `~/Library/Audio/Plug-Ins/CLAP/` · Windows: `%COMMONPROGRAMFILES%\CLAP\`

Then rescan in your DAW.

## Run without a DAW

**Standalone GUI** (real audio I/O + the plugin editor):

```bash
cargo run -p synth        --bin standalone
cargo run -p filter       --bin standalone
cargo run -p pitch-to-midi --bin standalone   # prompts to pick an input device (e.g. your mic)
```

Notes:
- The synth needs MIDI input; effects/analyzers need audio input. The standalone connects
  **no audio input by default** — pass `--input-device "<name>"` (or use the pitch-to-midi
  prompt). Use headphones to avoid feedback when monitoring a mic.

**Offline / preview binaries** (no DAW, no device setup):

```bash
# Effect: play a tone or a WAV through the filter, or render to a file
cargo run -p filter --bin demo                       # tone sweep, live
cargo run -p filter --bin demo -- in.wav             # play a WAV through the filter
cargo run -p filter --bin demo -- in.wav out.wav     # render offline to a file

# Analyzer: print the MIDI it detects, from a file or live mic
cargo run -p pitch-to-midi --bin demo -- --list      # list input devices
cargo run -p pitch-to-midi --bin demo                # live: default mic → MIDI notes
cargo run -p pitch-to-midi --bin demo -- tone.wav    # offline WAV → MIDI notes
```

## The SDK

Layered so DSP, host glue, and UI stay separate and testable:

- **`daudio-dsp`** — pure, host-agnostic DSP (filters, oscillator, ADSR, smoother, pitch
  tracking, note math). No nih-plug dependency; fully unit-tested.
- **`daudio-sdk`** (+ `daudio-sdk-macros`) — the author-facing layer. Three plugin traits
  and a `#[daudio_plugin(...)]` attribute macro that generates all the VST3/CLAP boilerplate:

  | Trait | Macro flag | Shape |
  |-------|-----------|-------|
  | `DaudioEffect` | *(default)* | audio in → audio out |
  | `DaudioSynth` | `midi = true` | MIDI in → audio out |
  | `DaudioAudioToMidi` | `midi_out = true` | audio in → MIDI out |

- **`daudio-ui`** — shared Vizia widgets (`Knob`, `Meter`, `NoteToggle`, `ParamControl`),
  a dark theme, and layout helpers (`card`, `create_editor`).
- **`daudio-preview`** — reusable offline/live preview harnesses used by the `demo` binaries.

### Writing a plugin

A plugin crate is thin: a `#[derive(Params)]` struct, a `#[daudio_plugin(...)]` struct, and
one trait impl. Sketch of an effect:

```rust
use daudio_sdk::prelude::*;

#[derive(Params)]
struct MyParams { #[id = "gain"] gain: FloatParam }

#[daudio_plugin(
    name = "My Effect", vendor = "me", clap_id = "com.me.fx",
    vst3_id = "myEffect00000001",        // must be exactly 16 bytes
    clap_features = [AudioEffect, Stereo], vst3_categories = [Fx],
)]
struct MyPlugin { params: Arc<MyParams>, /* DSP state */ }

impl DaudioEffect for MyPlugin {
    type Params = MyParams;
    fn activate(&mut self, sample_rate: f32) { /* ... */ }
    fn process_frame(&mut self, l: f32, r: f32) -> (f32, f32) { /* ... */ }
}
```

Add `src/bin/standalone.rs`, a `bundler.toml` entry, and the crate to the workspace `members`.
See `plugins/filter` as the smallest complete example.

## Repository layout

```
crates/
  daudio-dsp/          pure DSP primitives
  daudio-sdk/          traits + param helpers + prelude
  daudio-sdk-macros/   the #[daudio_plugin] proc-macro
  daudio-ui/           Vizia widgets + theme
  daudio-preview/      offline/live preview harnesses
plugins/
  filter/  synth/  pitch-to-midi/
xtask/                 the nih-plug bundler
docs/plans/            design + implementation docs for each feature
CLAUDE.md              orientation + gotchas for AI agents working here
```

## Development

```bash
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo fmt
```

A pre-existing `block v0.1.6` future-incompat warning from a transitive nih-plug dependency
is harmless and can be ignored. Each feature's design and step-by-step plan lives in
`docs/plans/`; `CLAUDE.md` documents the non-obvious pitfalls (especially around the Vizia GUI).

## License

MIT.
