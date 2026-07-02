use nih_plug::prelude::*;

/// A stereo audio effect. Implement this and annotate the struct with
/// `#[daudio_plugin(...)]` to get a full nih-plug VST3+CLAP plugin.
///
/// The annotated struct MUST have a field `params: std::sync::Arc<Self::Params>`.
pub trait DaudioEffect: Send {
    type Params: Params + Default;

    /// Called from `Plugin::initialize`: set sample rate, snap smoothers.
    fn activate(&mut self, sample_rate: f32);

    /// Called from `Plugin::reset`. Default: no-op.
    fn reset(&mut self) {}

    /// Called once at the start of each `process` block, before the sample loop.
    /// Use for per-block work like recomputing filter coefficients. Default: no-op.
    fn pre_block(&mut self) {}

    /// Process one stereo frame. Called per sample.
    fn process_frame(&mut self, left: f32, right: f32) -> (f32, f32);

    /// Optional custom editor. Return `None` (default) for the host's generic UI.
    fn editor(&mut self) -> Option<Box<dyn nih_plug::prelude::Editor>> {
        None
    }
}
