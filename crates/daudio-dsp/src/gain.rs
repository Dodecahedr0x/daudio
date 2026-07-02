//! Decibel <-> linear gain conversions.

/// Convert decibels to a linear amplitude factor. 0 dB -> 1.0, -6 dB ~ 0.5.
pub fn db_to_gain(db: f32) -> f32 {
    10.0_f32.powf(db / 20.0)
}

/// Convert a linear amplitude factor to decibels. Returns f32::NEG_INFINITY at 0.
pub fn gain_to_db(gain: f32) -> f32 {
    if gain <= 0.0 {
        f32::NEG_INFINITY
    } else {
        20.0 * gain.log10()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f32, b: f32, eps: f32) {
        assert!((a - b).abs() < eps, "{a} !~ {b}");
    }

    #[test]
    fn zero_db_is_unity() {
        approx(db_to_gain(0.0), 1.0, 1e-6);
    }

    #[test]
    fn minus_six_db_is_about_half() {
        approx(db_to_gain(-6.0), 0.5012, 1e-3);
    }

    #[test]
    fn roundtrip() {
        approx(gain_to_db(db_to_gain(-12.0)), -12.0, 1e-3);
    }

    #[test]
    fn zero_gain_is_neg_inf_db() {
        assert_eq!(gain_to_db(0.0), f32::NEG_INFINITY);
    }
}
