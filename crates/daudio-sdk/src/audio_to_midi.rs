use nih_plug::prelude::*;

/// An audio-input -> MIDI-output analyzer (e.g. pitch-to-MIDI). The macro's
/// `midi_out = true` mode drives this: audio is passed through unchanged and
/// MIDI events are emitted alongside it.
///
/// The annotated struct MUST have a field `params: std::sync::Arc<Self::Params>`.
pub trait DaudioAudioToMidi: Send {
    type Params: Params + Default;
    fn activate(&mut self, sample_rate: f32);
    fn reset(&mut self) {}
    /// Feed one mono input sample. `timing` is the sample's offset within the
    /// current block; stamp emitted events with it. Push events via `emit`.
    fn process_sample(&mut self, input: f32, timing: u32, emit: &mut dyn FnMut(NoteEvent<()>));
    fn editor(&mut self) -> Option<Box<dyn Editor>> {
        None
    }
}
