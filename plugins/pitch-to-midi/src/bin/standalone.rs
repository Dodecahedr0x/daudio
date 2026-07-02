//! Standalone runner for the pitch-to-MIDI plugin.
//!
//! Unlike an effect, this analyzer needs audio *input* to detect pitch — but
//! nih-plug's standalone connects no input by default (to avoid feedback). So
//! before launching, we interactively pick an input device (defaulting to the
//! system mic) and pass it through as `--input-device`, so the mic feeds the
//! detector and the editor shows notes live. An explicit `--input-device` on
//! the command line skips the prompt.
//!
//! Tip: use headphones — the plugin passes audio through to the output, so mic
//! → speakers can feed back.

use nih_plug::prelude::*;
use pitch_to_midi::PitchToMidi;

fn main() {
    let mut args: Vec<String> = std::env::args().collect();

    if !args.iter().any(|a| a == "--input-device") {
        if let Some(device) = daudio_preview::choose_input_device() {
            println!("Using input device: {device}\n");
            args.push("--input-device".to_string());
            args.push(device);
        }
    }

    nih_export_standalone_with_args::<PitchToMidi, _>(args);
}
