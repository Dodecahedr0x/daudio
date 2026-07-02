//! The uniform per-sample processing contract for DSP blocks.

pub trait Processor {
    /// Called on init and whenever the host sample rate changes.
    fn set_sample_rate(&mut self, sample_rate: f32);
    /// Clear internal state (delay lines, filter memory, etc.).
    fn reset(&mut self);
    /// Process one input sample, returning one output sample.
    fn process_sample(&mut self, input: f32) -> f32;
    /// Process a block in place. Default loops over `process_sample`;
    /// override for SIMD/block efficiency.
    fn process_block(&mut self, buf: &mut [f32]) {
        for s in buf.iter_mut() {
            *s = self.process_sample(*s);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct AddOne;
    impl Processor for AddOne {
        fn set_sample_rate(&mut self, _sr: f32) {}
        fn reset(&mut self) {}
        fn process_sample(&mut self, input: f32) -> f32 {
            input + 1.0
        }
    }

    #[test]
    fn default_block_uses_process_sample() {
        let mut p = AddOne;
        let mut buf = [0.0, 1.0, 2.0];
        p.process_block(&mut buf);
        assert_eq!(buf, [1.0, 2.0, 3.0]);
    }
}
