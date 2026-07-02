# daudio-sdk

The author-facing SDK. A plugin depends on this one crate: it re-exports `nih_plug`,
`daudio_dsp`, parameter helpers, the `#[daudio_plugin]` macro, and a `prelude`.

## Plugin traits

Implement one, annotate the struct with `#[daudio_plugin(...)]`, and the macro
(`daudio-sdk-macros`) generates the `Plugin`/`ClapPlugin`/`Vst3Plugin` impls and exports.

| Trait | Macro flag | Shape |
|-------|-----------|-------|
| `DaudioEffect` (`process_frame`) | *(default)* | audio in → audio out |
| `DaudioSynth` (`render_frame`, MIDI in + `VoiceManager`) | `midi = true` | MIDI in → audio out |
| `DaudioAudioToMidi` (`process_sample`) | `midi_out = true` | audio in → MIDI out |

The macro requires a field `params: Arc<Self::Params>`. `vst3_id` must be exactly 16 ASCII bytes.

## Also here

- `db_gain_param`, `hz_param` — parameter constructors with sensible ranges/formatters.
- `VoiceManager<V: Voice>` — polyphony (allocation, oldest-note stealing, per-voice config).
- `PeakLevel` — a lock-free `f32` channel for audio→UI metering.

## Notes

The macro routes all generated paths through `::daudio_sdk::nih_plug::`, so a plugin only needs
a `daudio-sdk` dependency. When editing the macro, keep the effect/synth/analyzer codegen
branches byte-for-byte unchanged except the one you're touching.
