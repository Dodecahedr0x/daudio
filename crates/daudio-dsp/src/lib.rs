//! daudio-dsp: pure, host-agnostic DSP primitives.

pub mod biquad;
pub mod gain;
pub mod oscillator;
pub mod processor;
pub mod smoother;

#[cfg(test)]
mod smoke {
    #[test]
    fn workspace_builds() {
        assert_eq!(2 + 2, 4);
    }
}
