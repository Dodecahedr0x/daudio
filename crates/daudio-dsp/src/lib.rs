//! daudio-dsp: pure, host-agnostic DSP primitives.

pub mod gain;
pub mod processor;

#[cfg(test)]
mod smoke {
    #[test]
    fn workspace_builds() {
        assert_eq!(2 + 2, 4);
    }
}
