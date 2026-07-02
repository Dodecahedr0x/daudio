//! One-pole exponential smoother for click-free parameter changes.

/// One-pole exponential parameter smoother.
///
/// The constructor uses a 48 kHz default sample rate. Callers MUST call
/// [`OnePole::set_sample_rate`] before processing (plugins do this in their
/// `initialize` callback) so the smoothing time matches the host rate.
pub struct OnePole {
    coeff: f32,
    state: f32,
    time_ms: f32,
    sample_rate: f32,
}

impl OnePole {
    /// `time_ms` is the ~63% settling time toward a new target.
    ///
    /// Uses a 48 kHz default sample rate. Callers MUST call
    /// [`OnePole::set_sample_rate`] before processing.
    pub fn new(time_ms: f32) -> Self {
        let mut s = Self {
            coeff: 0.0,
            state: 0.0,
            time_ms,
            sample_rate: 48_000.0,
        };
        s.recompute();
        s
    }

    fn recompute(&mut self) {
        let t = (self.time_ms / 1000.0).max(1e-6);
        self.coeff = (-1.0 / (t * self.sample_rate)).exp();
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate;
        self.recompute();
    }

    /// Set the current value immediately (no smoothing).
    pub fn snap_to(&mut self, value: f32) {
        self.state = value;
    }

    /// Advance one sample toward `target`, returning the smoothed value.
    pub fn tick(&mut self, target: f32) -> f32 {
        self.state = target + self.coeff * (self.state - target);
        self.state
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snap_sets_value() {
        let mut s = OnePole::new(10.0);
        s.set_sample_rate(48_000.0);
        s.snap_to(0.7);
        assert!((s.tick(0.7) - 0.7).abs() < 1e-6);
    }

    #[test]
    fn converges_toward_target() {
        let mut s = OnePole::new(5.0);
        s.set_sample_rate(48_000.0);
        s.snap_to(0.0);
        let mut v = 0.0;
        for _ in 0..48_000 {
            v = s.tick(1.0);
        }
        assert!((v - 1.0).abs() < 1e-3, "did not converge: {v}");
    }

    #[test]
    fn moves_gradually_not_instantly() {
        let mut s = OnePole::new(50.0);
        s.set_sample_rate(48_000.0);
        s.snap_to(0.0);
        let first = s.tick(1.0);
        assert!(first > 0.0 && first < 0.5, "should be partway: {first}");
    }
}
