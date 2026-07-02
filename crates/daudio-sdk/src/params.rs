use nih_plug::prelude::*;

/// A gain parameter in dB with linear smoothing.
pub fn db_gain_param(
    name: impl Into<String>,
    min_db: f32,
    max_db: f32,
    default_db: f32,
) -> FloatParam {
    FloatParam::new(
        name.into(),
        default_db,
        FloatRange::Linear {
            min: min_db,
            max: max_db,
        },
    )
    .with_unit(" dB")
    .with_smoother(SmoothingStyle::Linear(20.0))
}

/// A frequency parameter in Hz with a perceptual (skewed) range and Hz/kHz display.
pub fn hz_param(name: impl Into<String>, default_hz: f32, min_hz: f32, max_hz: f32) -> FloatParam {
    FloatParam::new(
        name.into(),
        default_hz,
        FloatRange::Skewed {
            min: min_hz,
            max: max_hz,
            factor: FloatRange::skew_factor(-2.0),
        },
    )
    .with_unit(" Hz")
    .with_value_to_string(formatters::v2s_f32_hz_then_khz(1))
    .with_string_to_value(formatters::s2v_f32_hz_then_khz())
}

#[cfg(test)]
mod tests {
    use super::*;
    use nih_plug::prelude::Param;

    #[test]
    fn db_gain_defaults_and_range() {
        let p = db_gain_param("Gain", -60.0, 6.0, 0.0);
        assert_eq!(p.default_plain_value(), 0.0);
        assert_eq!(p.preview_plain(0.0), -60.0);
        assert_eq!(p.preview_plain(1.0), 6.0);
    }

    #[test]
    fn hz_default_is_set() {
        let p = hz_param("Cutoff", 1000.0, 20.0, 20_000.0);
        assert_eq!(p.default_plain_value(), 1000.0);
    }
}
