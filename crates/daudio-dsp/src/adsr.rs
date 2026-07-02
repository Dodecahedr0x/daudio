//! Linear ADSR envelope generator.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Stage {
    Idle,
    Attack,
    Decay,
    Sustain,
    Release,
}

/// Linear attack/decay/sustain/release envelope.
///
/// The constructor uses a 48 kHz default sample rate. Callers MUST call
/// [`Adsr::set_sample_rate`] before processing (plugins do this in their
/// `initialize` callback) so stage rates match the host rate.
pub struct Adsr {
    sample_rate: f32,
    stage: Stage,
    level: f32,
    attack_s: f32,
    decay_s: f32,
    sustain: f32,
    release_s: f32,
    release_rate: f32,
}

impl Adsr {
    /// Construct with a 48 kHz default sample rate and sensible default times.
    /// Callers MUST call [`Adsr::set_sample_rate`] before processing.
    pub fn new() -> Self {
        Self {
            sample_rate: 48_000.0,
            stage: Stage::Idle,
            level: 0.0,
            attack_s: 0.01,
            decay_s: 0.1,
            sustain: 0.8,
            release_s: 0.2,
            release_rate: 0.0,
        }
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate;
    }

    /// Set stage times (seconds) and sustain level. Times are clamped to
    /// `>= 1e-4` to guard against division by zero; sustain is clamped to
    /// `[0, 1]`.
    pub fn set_params(&mut self, attack_s: f32, decay_s: f32, sustain: f32, release_s: f32) {
        self.attack_s = attack_s.max(1e-4);
        self.decay_s = decay_s.max(1e-4);
        self.sustain = sustain.clamp(0.0, 1.0);
        self.release_s = release_s.max(1e-4);
    }

    /// Begin the envelope from the attack stage.
    pub fn trigger(&mut self) {
        self.stage = Stage::Attack;
    }

    /// Begin release from the current level.
    pub fn release(&mut self) {
        self.release_rate = self.level / (self.release_s * self.sample_rate);
        self.stage = Stage::Release;
    }

    /// True unless the envelope is idle.
    pub fn is_active(&self) -> bool {
        self.stage != Stage::Idle
    }

    /// Advance the linear stage machine one sample and return the new level.
    pub fn next_sample(&mut self) -> f32 {
        match self.stage {
            Stage::Idle => {
                self.level = 0.0;
            }
            Stage::Attack => {
                self.level += 1.0 / (self.attack_s * self.sample_rate);
                if self.level >= 1.0 {
                    self.level = 1.0;
                    self.stage = Stage::Decay;
                }
            }
            Stage::Decay => {
                self.level -= (1.0 - self.sustain) / (self.decay_s * self.sample_rate);
                if self.level <= self.sustain {
                    self.level = self.sustain;
                    self.stage = Stage::Sustain;
                }
            }
            Stage::Sustain => {
                self.level = self.sustain;
            }
            Stage::Release => {
                self.level -= self.release_rate;
                if self.level <= 0.0 {
                    self.level = 0.0;
                    self.stage = Stage::Idle;
                }
            }
        }
        self.level
    }
}

impl Default for Adsr {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_is_silent_and_inactive() {
        let mut env = Adsr::new();
        assert_eq!(env.next_sample(), 0.0);
        assert!(!env.is_active());
    }

    #[test]
    fn attack_rises_to_one() {
        let mut env = Adsr::new();
        env.set_sample_rate(48_000.0);
        env.set_params(0.01, 0.1, 0.8, 0.2);
        env.trigger();
        // The peak (~1.0) occurs at the attack/decay boundary; with a=0.01,
        // d=0.1 the level is already decaying toward sustain by sample 960, so
        // assert on the peak reached over the window rather than the final.
        let mut peak = 0.0f32;
        for _ in 0..960 {
            peak = peak.max(env.next_sample());
        }
        assert!(peak > 0.99, "attack did not reach 1.0: {peak}");
    }

    #[test]
    fn sustain_holds() {
        let mut env = Adsr::new();
        env.set_sample_rate(48_000.0);
        env.set_params(0.01, 0.1, 0.8, 0.2);
        env.trigger();
        let mut level = 0.0;
        for _ in 0..(48_000 / 5) {
            level = env.next_sample();
        }
        assert!((level - 0.8).abs() < 0.02, "sustain not held: {level}");
        for _ in 0..1000 {
            let l = env.next_sample();
            assert!((l - 0.8).abs() < 0.02, "sustain unstable: {l}");
        }
    }

    #[test]
    fn release_reaches_zero_and_idle() {
        let mut env = Adsr::new();
        env.set_sample_rate(48_000.0);
        env.set_params(0.01, 0.1, 0.8, 0.2);
        env.trigger();
        for _ in 0..(48_000 / 5) {
            env.next_sample();
        }
        env.release();
        let mut level = 0.0;
        for _ in 0..((0.3 * 48_000.0) as usize) {
            level = env.next_sample();
        }
        assert!(level < 1e-3, "release did not reach 0: {level}");
        assert!(!env.is_active());
    }

    #[test]
    fn retrigger_after_idle() {
        let mut env = Adsr::new();
        env.set_sample_rate(48_000.0);
        env.set_params(0.01, 0.1, 0.8, 0.2);
        env.trigger();
        for _ in 0..(48_000 / 5) {
            env.next_sample();
        }
        env.release();
        for _ in 0..((0.3 * 48_000.0) as usize) {
            env.next_sample();
        }
        assert!(!env.is_active());
        env.trigger();
        let mut level = 0.0;
        for _ in 0..10 {
            level = env.next_sample();
        }
        assert!(level > 0.0, "retrigger did not rise: {level}");
        assert!(env.is_active());
    }
}
