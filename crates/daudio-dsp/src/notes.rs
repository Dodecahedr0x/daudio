//! MIDI note math and scale quantization (pure, host-agnostic).

/// Convert a frequency in Hz to the nearest MIDI note number.
pub fn freq_to_midi(freq_hz: f32) -> i32 {
    (69.0 + 12.0 * (freq_hz / 440.0).log2()).round() as i32
}

/// Note name like "A4" for a MIDI note (A4 = 69). Uses sharps.
pub fn note_name(midi: i32) -> String {
    const NAMES: [&str; 12] = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    let pc = midi.rem_euclid(12) as usize;
    let octave = midi.div_euclid(12) - 1;
    format!("{}{}", NAMES[pc], octave)
}

/// Snap `midi` to the nearest note allowed by a scale defined as a `root`
/// pitch-class (0=C..11=B) and a 12-bit `degree_mask` (bit d set = the note
/// `root + d` mod 12 is allowed). Ties resolve upward. Empty mask -> None.
pub fn quantize(midi: i32, root: u8, degree_mask: u16) -> Option<i32> {
    if degree_mask & 0x0fff == 0 {
        return None;
    }
    let allowed = |note: i32| -> bool {
        let degree = (note - root as i32).rem_euclid(12) as u16;
        degree_mask & (1 << degree) != 0
    };
    if allowed(midi) {
        return Some(midi);
    }
    for offset in 1..=6 {
        if allowed(midi + offset) {
            return Some(midi + offset);
        }
        if allowed(midi - offset) {
            return Some(midi - offset);
        }
    }
    Some(midi)
}

/// Normalized 14-bit-style pitch-bend value (0.0..=1.0, 0.5 = no bend) for a
/// detected frequency relative to `note`, over a +/-`range_semitones` range.
pub fn bend_value(detected_hz: f32, note: u8, range_semitones: f32) -> f32 {
    let note_freq = 440.0 * 2.0_f32.powf((note as f32 - 69.0) / 12.0);
    let dev_semitones = 12.0 * (detected_hz / note_freq).log2();
    (0.5 + dev_semitones / (2.0 * range_semitones)).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    const MAJOR: u16 = 0b1010_1011_0101; // degrees 0,2,4,5,7,9,11

    #[test]
    fn freq_to_midi_landmarks() {
        assert_eq!(freq_to_midi(440.0), 69);
        assert_eq!(freq_to_midi(220.0), 57);
        assert_eq!(freq_to_midi(880.0), 81);
    }
    #[test]
    fn note_names() {
        assert_eq!(note_name(69), "A4");
        assert_eq!(note_name(60), "C4");
        assert_eq!(note_name(61), "C#4");
    }
    #[test]
    fn in_scale_notes_pass_through() {
        assert_eq!(quantize(60, 0, MAJOR), Some(60));
        assert_eq!(quantize(64, 0, MAJOR), Some(64));
    }
    #[test]
    fn out_of_scale_snaps_upward_on_tie() {
        assert_eq!(quantize(61, 0, MAJOR), Some(62));
    }
    #[test]
    fn root_shifts_the_scale() {
        assert_eq!(quantize(69, 9, MAJOR), Some(69));
        assert_eq!(quantize(60, 9, MAJOR), Some(61));
    }
    #[test]
    fn empty_mask_is_none() {
        assert_eq!(quantize(60, 0, 0), None);
    }
    #[test]
    fn bend_center_on_exact_note() {
        assert!((bend_value(440.0, 69, 2.0) - 0.5).abs() < 1e-4);
    }
    #[test]
    fn bend_sharp_one_semitone() {
        // +1 semitone (A#4 = 466.16 Hz) at range 2 -> +0.25 -> 0.75.
        assert!((bend_value(466.16, 69, 2.0) - 0.75).abs() < 1e-2);
    }
    #[test]
    fn bend_flat_two_semitones_clamps_to_zero() {
        // -2 semitones (G4 = 392.0 Hz) at range 2 -> dev -2 -> 0.5 - 0.5 = 0.0.
        assert!((bend_value(392.0, 69, 2.0) - 0.0).abs() < 1e-2);
    }
    #[test]
    fn bend_beyond_range_clamps_to_one() {
        // +3 semitones (C5 = 523.25 Hz) at range 2 -> clamped to 1.0.
        assert!((bend_value(523.25, 69, 2.0) - 1.0).abs() < 1e-4);
    }
    #[test]
    fn bend_narrow_range_saturates() {
        // +1 semitone at range 1 -> 0.5 + 0.5 = 1.0 (clamped).
        assert!((bend_value(466.16, 69, 1.0) - 1.0).abs() < 1e-2);
    }
    #[test]
    fn bend_wide_range_small_deviation() {
        // +1 semitone at range 12 -> 0.5 + 1/24 ~= 0.5417.
        assert!((bend_value(466.16, 69, 12.0) - 0.5417).abs() < 1e-2);
    }
}
