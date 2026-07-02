//! Audible/offline preview for the filter — no DAW, host, or input device.
//!
//!   cargo run -p filter --bin demo                          # saw through the filter
//!   cargo run -p filter --bin demo -- input.wav             # play a WAV, filtered
//!   cargo run -p filter --bin demo -- input.wav output.wav  # render to a file
//!
//! All the logic lives in the reusable `daudio_preview` harness, so every
//! effect's demo binary is this same one-liner.

fn main() {
    daudio_preview::run::<filter::FilterPlugin>();
}
