//! Phase-accumulator oscillator (sine, saw).

/// Oscillator waveform selection.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Waveform {
    Sine,
    Saw,
}

/// Phase-accumulator oscillator.
///
/// The constructor uses a 48 kHz default sample rate. Callers MUST call
/// [`Oscillator::set_sample_rate`] before processing (plugins do this in their
/// `initialize` callback) so the frequency matches the host rate.
pub struct Oscillator {
    sample_rate: f32,
    phase: f32,
    freq_hz: f32,
    waveform: Waveform,
}

impl Oscillator {
    /// Construct with a 48 kHz default sample rate, phase 0 and 440 Hz.
    /// Callers MUST call [`Oscillator::set_sample_rate`] before processing.
    pub fn new(waveform: Waveform) -> Self {
        Self {
            sample_rate: 48_000.0,
            phase: 0.0,
            freq_hz: 440.0,
            waveform,
        }
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate;
    }

    pub fn set_frequency(&mut self, hz: f32) {
        self.freq_hz = hz;
    }

    pub fn set_waveform(&mut self, waveform: Waveform) {
        self.waveform = waveform;
    }

    /// Reset the phase accumulator to 0.
    pub fn reset(&mut self) {
        self.phase = 0.0;
    }

    /// Compute the output for the current phase, then advance and wrap.
    pub fn next_sample(&mut self) -> f32 {
        let out = match self.waveform {
            Waveform::Sine => (std::f32::consts::TAU * self.phase).sin(),
            Waveform::Saw => 2.0 * self.phase - 1.0,
        };
        self.phase += self.freq_hz / self.sample_rate;
        self.phase -= self.phase.floor();
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sine_is_bounded() {
        let mut osc = Oscillator::new(Waveform::Sine);
        osc.set_sample_rate(48_000.0);
        osc.set_frequency(440.0);
        let mut saw_high = false;
        let mut saw_low = false;
        for _ in 0..10_000 {
            let s = osc.next_sample();
            assert!((-1.001..=1.001).contains(&s), "out of range: {s}");
            if s > 0.5 {
                saw_high = true;
            }
            if s < -0.5 {
                saw_low = true;
            }
        }
        assert!(saw_high && saw_low, "sine never swung fully");
    }

    #[test]
    fn saw_wraps_downward() {
        let mut osc = Oscillator::new(Waveform::Saw);
        osc.set_sample_rate(48_000.0);
        osc.set_frequency(1000.0);
        let mut prev = osc.next_sample();
        let mut saw_wrap = false;
        for _ in 0..500 {
            let s = osc.next_sample();
            if s - prev < -1.0 {
                saw_wrap = true;
            }
            prev = s;
        }
        assert!(saw_wrap, "saw never wrapped downward");
    }

    #[test]
    fn reset_zeroes_phase() {
        let mut osc = Oscillator::new(Waveform::Sine);
        osc.set_sample_rate(48_000.0);
        osc.set_frequency(440.0);
        for _ in 0..100 {
            osc.next_sample();
        }
        osc.reset();
        assert!(osc.next_sample().abs() < 1e-3, "phase not zeroed");
    }

    #[test]
    fn frequency_changes_period() {
        fn sign_changes(freq: f32) -> u32 {
            let mut osc = Oscillator::new(Waveform::Sine);
            osc.set_sample_rate(48_000.0);
            osc.set_frequency(freq);
            let mut count = 0;
            let mut prev = osc.next_sample();
            for _ in 0..48_000 {
                let s = osc.next_sample();
                if (prev < 0.0) != (s < 0.0) {
                    count += 1;
                }
                prev = s;
            }
            count
        }
        let low = sign_changes(100.0);
        let high = sign_changes(400.0);
        assert!(high >= low * 3, "400Hz not clearly faster: {low} vs {high}");
    }
}
