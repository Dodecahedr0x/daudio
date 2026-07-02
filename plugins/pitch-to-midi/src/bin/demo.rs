//! Offline preview for the pitch-to-MIDI analyzer — no DAW, host, or input
//! device. Feeds a WAV through the plugin and prints the emitted MIDI events:
//!
//!   cargo run -p pitch-to-midi --bin demo -- input.wav
//!
//! All the logic lives in the reusable `daudio_preview` harness, so every
//! analyzer's demo binary is this same one-liner.

fn main() {
    daudio_preview::run_analyzer::<pitch_to_midi::PitchToMidi>();
}
