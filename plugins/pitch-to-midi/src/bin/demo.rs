//! Preview for the pitch-to-MIDI analyzer — no DAW or host needed. Prints the
//! MIDI notes the plugin emits, from a live input device or a WAV file:
//!
//!   cargo run -p pitch-to-midi --bin demo                      # live: default mic
//!   cargo run -p pitch-to-midi --bin demo -- --list            # list input devices
//!   cargo run -p pitch-to-midi --bin demo -- --input "<name>"  # live: chosen device
//!   cargo run -p pitch-to-midi --bin demo -- input.wav         # offline WAV file
//!
//! All the logic lives in the reusable `daudio_preview` harness, so every
//! analyzer's demo binary is this same one-liner.

fn main() {
    daudio_preview::run_analyzer::<pitch_to_midi::PitchToMidi>();
}
