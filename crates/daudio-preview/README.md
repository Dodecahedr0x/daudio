# daudio-preview

Reusable preview harnesses that drive a plugin's real audio path with zero DAW setup. Each
plugin's `demo` binary is a one-line shim over these.

## Entry points

- `run::<E: DaudioEffect>()` — for effects:
  - `demo` — play a saw through the effect (live)
  - `demo <in.wav>` — play a WAV through the effect (live, looped)
  - `demo <in.wav> <out.wav>` — render the WAV through the effect to a file (offline, deterministic)
- `run_analyzer::<A: DaudioAudioToMidi>()` — for audio→MIDI analyzers; prints emitted MIDI notes:
  - `demo` — live, default input device (e.g. mic)
  - `demo --list` — list input devices
  - `demo --input "<name>"` — live, chosen device
  - `demo <in.wav>` — offline WAV analysis
- `choose_input_device()` — interactive input-device picker, used by standalone binaries to
  inject `--input-device`.

Only uncompressed WAV is supported (via `hound`); live audio uses `cpal`.

Note: the analyzer's offline WAV mode feeds faster than real time, so with the worker-thread
pitch detector it may emit fewer/no notes — that path is a smoke test; the live/mic path is the
real one.
