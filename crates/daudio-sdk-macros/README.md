# daudio-sdk-macros

The `#[daudio_plugin(...)]` attribute proc-macro behind `daudio-sdk`. You normally use it via
`daudio_sdk::prelude::*` rather than depending on this crate directly.

Given a small metadata block, it generates the full nih-plug `impl Plugin`, `impl ClapPlugin`,
`impl Vst3Plugin`, and the `nih_export_clap!` / `nih_export_vst3!` calls — delegating behavior
to the plugin's `DaudioEffect` / `DaudioSynth` / `DaudioAudioToMidi` impl.

## Attribute keys

`name`, `vendor`, `url`, `email`, `clap_id`, `clap_description`, `vst3_id` (**exactly 16
bytes**, checked at compile time), `clap_features = [...]`, `vst3_categories = [...]`, and the
mode flags `midi = true` (synth) / `midi_out = true` (analyzer). Unknown and duplicate keys are
rejected with spanned compile errors.

All generated paths are routed through `::daudio_sdk::nih_plug::` so a plugin needs only a
`daudio-sdk` dependency.
