//! Transposed Direct Form II biquad, RBJ cookbook lowpass coefficients.

use crate::processor::Processor;
use std::f32::consts::PI;

/// RBJ cookbook lowpass biquad filter.
///
/// The constructor uses a 48 kHz default sample rate. Callers MUST call
/// [`BiquadLowpass::set_sample_rate`] before processing (plugins do this in
/// their `initialize` callback) so coefficients match the host rate.
pub struct BiquadLowpass {
    sample_rate: f32,
    cutoff_hz: f32,
    q: f32,
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    // TODO(denormals): IIR filter state (z1/z2) can accumulate denormal
    // values on some platforms, which may cause CPU spikes. Flushing to zero
    // may be added later if profiling shows it is a problem.
    z1: f32,
    z2: f32,
}

impl BiquadLowpass {
    /// Construct with a 48 kHz default sample rate. Callers MUST call
    /// [`BiquadLowpass::set_sample_rate`] before processing.
    pub fn new(cutoff_hz: f32, q: f32) -> Self {
        let mut f = Self {
            sample_rate: 48_000.0,
            cutoff_hz,
            q,
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
            z1: 0.0,
            z2: 0.0,
        };
        f.recompute();
        f
    }

    pub fn set_cutoff(&mut self, cutoff_hz: f32) {
        self.cutoff_hz = cutoff_hz;
        self.recompute();
    }

    fn recompute(&mut self) {
        let cutoff = self.cutoff_hz.clamp(10.0, self.sample_rate * 0.49);
        let w0 = 2.0 * PI * cutoff / self.sample_rate;
        let (sin_w0, cos_w0) = w0.sin_cos();
        let q = self.q.max(1e-3);
        let alpha = sin_w0 / (2.0 * q);

        let b0 = (1.0 - cos_w0) / 2.0;
        let b1 = 1.0 - cos_w0;
        let b2 = (1.0 - cos_w0) / 2.0;
        let a0 = 1.0 + alpha;
        let a1 = -2.0 * cos_w0;
        let a2 = 1.0 - alpha;

        self.b0 = b0 / a0;
        self.b1 = b1 / a0;
        self.b2 = b2 / a0;
        self.a1 = a1 / a0;
        self.a2 = a2 / a0;
    }
}

impl Processor for BiquadLowpass {
    fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate;
        self.recompute();
    }

    fn reset(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }

    fn process_sample(&mut self, input: f32) -> f32 {
        let y = self.b0 * input + self.z1;
        self.z1 = self.b1 * input - self.a1 * y + self.z2;
        self.z2 = self.b2 * input - self.a2 * y;
        y
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passes_dc() {
        let mut f = BiquadLowpass::new(1000.0, 0.707);
        f.set_sample_rate(48_000.0);
        let mut out = 0.0;
        for _ in 0..10_000 {
            out = f.process_sample(1.0);
        }
        assert!((out - 1.0).abs() < 1e-2, "DC gain off: {out}");
    }

    #[test]
    fn attenuates_high_freq() {
        let mut f = BiquadLowpass::new(1000.0, 0.707);
        f.set_sample_rate(48_000.0);
        let mut peak = 0.0f32;
        for n in 0..10_000 {
            let x = if n % 2 == 0 { 1.0 } else { -1.0 };
            let y = f.process_sample(x);
            if n > 5_000 {
                peak = peak.max(y.abs());
            }
        }
        assert!(peak < 0.1, "high freq not attenuated: {peak}");
    }

    #[test]
    fn q_zero_does_not_produce_nan() {
        let mut f = BiquadLowpass::new(1000.0, 0.0);
        f.set_sample_rate(48_000.0);
        let out = f.process_sample(1.0);
        assert!(out.is_finite(), "q=0 produced non-finite output: {out}");
    }

    #[test]
    fn reset_clears_state() {
        let mut f = BiquadLowpass::new(1000.0, 0.707);
        f.set_sample_rate(48_000.0);
        for _ in 0..100 {
            f.process_sample(1.0);
        }
        f.reset();
        assert!(f.process_sample(0.0).abs() < 1e-6);
    }
}
