# daudio Filter

A resonant low-pass **effect**: a biquad low-pass with a smoothed output gain and a live output
level meter.

- **Params:** Cutoff (20 Hz–20 kHz, perceptual), Gain (−60…+6 dB).
- **DSP:** `FilterCore` (per-channel `BiquadLowpass` + one-pole gain smoother) from `daudio-dsp`,
  wired to the SDK via `DaudioEffect`.
- **UI:** two glow knobs and a gradient meter in a card layout.

## Run

```bash
cargo run -p filter --bin standalone                 # GUI (pass --input-device to feed audio)
cargo run -p filter --bin demo                       # hear a tone swept through the filter
cargo run -p filter --bin demo -- in.wav out.wav     # render a filtered WAV
cargo xtask bundle filter --release                  # -> target/bundled/daudio Filter.{vst3,clap}
```

This is the smallest complete plugin — a good template for a new effect.
