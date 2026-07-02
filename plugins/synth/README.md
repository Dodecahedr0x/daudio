# daudio Synth

A polyphonic subtractive **instrument** (MIDI in → audio out). Each voice is
oscillator → per-voice low-pass filter → ADSR amplitude envelope, with the envelope also
opening the filter.

- **Params:** Waveform (saw/sine), Cutoff, Resonance, Env Amount, Attack, Decay, Sustain,
  Release, Gain.
- **DSP/SDK:** `SynthVoice` (built from `daudio-dsp` `Oscillator`/`BiquadLowpass`/`Adsr`) driven
  by the SDK's `VoiceManager` (16 voices, oldest-note stealing); implemented via `DaudioSynth`
  (`midi = true`).
- **UI:** oscillator / filter / envelope / output cards of knobs.

## Run

```bash
cargo run -p synth --bin standalone       # GUI; play via a connected MIDI device
cargo xtask bundle synth --release        # -> target/bundled/daudio Synth.{vst3,clap}
```

In a DAW, put it on an instrument track and play it with MIDI.
