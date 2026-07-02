use nih_plug::prelude::*;
use pitch_to_midi::PitchToMidi;

fn main() {
    nih_export_standalone::<PitchToMidi>();
}
