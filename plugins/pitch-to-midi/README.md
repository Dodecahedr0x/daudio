# daudio Pitch2MIDI

A monophonic **audio → MIDI** analyzer: it detects the pitch of an incoming sound (voice, bass,
lead) and emits MIDI notes, quantized to a scale you define. Audio passes through unchanged;
MIDI is emitted alongside it (route it to an instrument).

- **Scale editor:** a root selector + 12 note toggles (a keyboard) you can edit by hand, filled
  by presets (Major, Minor, Pentatonic, Blues, Chromatic, Clear). Detected notes snap to the
  nearest allowed note. Toggle labels relabel live when you change the root.
- **Detection:** windowed monophonic pitch tracking (`daudio-dsp::PitchTracker`, McLeod method)
  on a worker thread, then a debounced, level-gated trigger with velocity from input level.
- **Options:** Response (detection window/hop trade-off), Sensitivity (input gate), Confidence
  (clarity), Hold (debounce), Max Jump (fast-commit guard), Bend Range, and **Bend Mode**
  (Off / On / **Auto**). In Auto, a pitch change with continuous volume becomes a pitch-bend
  instead of a new note, while a volume dip (re-articulation) retriggers — so slurs bend and
  separate notes retrigger.
- **Readout:** a live "in → out" display of the detected and emitted notes.
- **SDK:** implemented via `DaudioAudioToMidi` (`midi_out = true`).

## Run

```bash
cargo run -p pitch-to-midi --bin standalone          # GUI; prompts to pick an input device (mic)
cargo run -p pitch-to-midi --bin demo                # terminal: live mic → printed MIDI notes
cargo run -p pitch-to-midi --bin demo -- --list      # list input devices
cargo run -p pitch-to-midi --bin demo -- tone.wav    # offline WAV → printed MIDI notes
cargo xtask bundle pitch-to-midi --release           # -> target/bundled/daudio Pitch2MIDI.{vst3,clap}
```

Use headphones with a mic — the plugin passes audio through, so mic → speakers can feed back.
In a DAW, insert it on the source track and route its MIDI output to an instrument.
