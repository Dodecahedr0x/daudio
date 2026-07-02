use nih_plug::prelude::*;

/// A polyphonic instrument. Implement this and annotate the struct with
/// `#[daudio_plugin(...)]` to get a full nih-plug VST3+CLAP instrument.
///
/// The annotated struct MUST have a field `params: std::sync::Arc<Self::Params>`.
pub trait DaudioSynth: Send {
    type Params: Params + Default;

    /// Called from `Plugin::initialize`: set sample rate on voices/oscillators.
    fn activate(&mut self, sample_rate: f32);

    /// Called from `Plugin::reset`. Default: no-op.
    fn reset(&mut self) {}

    /// Called once at the start of each `process` block, before the sample loop.
    /// Use for per-block work like recomputing envelope rates. Default: no-op.
    fn pre_block(&mut self) {}

    /// Handle a note-on event.
    fn note_on(&mut self, note: u8, velocity: f32);

    /// Handle a note-off event.
    fn note_off(&mut self, note: u8);

    /// Render one stereo frame. Called per sample.
    fn render_frame(&mut self) -> (f32, f32);

    /// Optional custom editor. Return `None` (default) for the host's generic UI.
    fn editor(&mut self) -> Option<Box<dyn Editor>> {
        None
    }
}
