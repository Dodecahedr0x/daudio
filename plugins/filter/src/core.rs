use daudio_dsp::biquad::BiquadLowpass;
use daudio_dsp::gain::db_to_gain;
use daudio_dsp::processor::Processor;
use daudio_dsp::smoother::OnePole;

/// Host-agnostic stereo processing core: lowpass per channel + smoothed gain.
///
/// Callers MUST call [`FilterCore::set_sample_rate`] before processing
/// (the nih-plug adapter does this in `initialize`).
pub struct FilterCore {
    left: BiquadLowpass,
    right: BiquadLowpass,
    gain: OnePole,
}

impl FilterCore {
    pub fn new() -> Self {
        Self {
            left: BiquadLowpass::new(1000.0, 0.707),
            right: BiquadLowpass::new(1000.0, 0.707),
            gain: OnePole::new(20.0),
        }
    }

    pub fn set_sample_rate(&mut self, sr: f32) {
        self.left.set_sample_rate(sr);
        self.right.set_sample_rate(sr);
        self.gain.set_sample_rate(sr);
    }

    pub fn set_cutoff(&mut self, hz: f32) {
        self.left.set_cutoff(hz);
        self.right.set_cutoff(hz);
    }

    pub fn reset(&mut self) {
        self.left.reset();
        self.right.reset();
    }

    /// Snap the smoothed gain immediately to `gain_db` (use on init to avoid a ramp).
    pub fn snap_gain(&mut self, gain_db: f32) {
        self.gain.snap_to(db_to_gain(gain_db));
    }

    /// Process one stereo frame given a target gain in dB.
    pub fn process_frame(&mut self, l: f32, r: f32, gain_db: f32) -> (f32, f32) {
        let g = self.gain.tick(db_to_gain(gain_db));
        (
            self.left.process_sample(l) * g,
            self.right.process_sample(r) * g,
        )
    }
}

impl Default for FilterCore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unity_gain_passes_dc() {
        let mut c = FilterCore::new();
        c.set_sample_rate(48_000.0);
        c.snap_gain(0.0);
        let mut out = (0.0, 0.0);
        for _ in 0..10_000 {
            out = c.process_frame(1.0, 1.0, 0.0);
        }
        assert!((out.0 - 1.0).abs() < 1e-2, "got {}", out.0);
    }

    #[test]
    fn minus_six_db_halves_amplitude() {
        let mut c = FilterCore::new();
        c.set_sample_rate(48_000.0);
        c.snap_gain(-6.0);
        let mut out = (0.0, 0.0);
        for _ in 0..10_000 {
            out = c.process_frame(1.0, 1.0, -6.0);
        }
        assert!((out.0 - 0.5012).abs() < 1e-2, "got {}", out.0);
    }
}
