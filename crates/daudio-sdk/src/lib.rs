//! daudio-sdk: author-facing facade for building daudio plugins.

pub mod audio_to_midi;
pub mod effect;
pub mod meter;
pub mod params;
pub mod synth;
pub mod voice;

pub use audio_to_midi::DaudioAudioToMidi;
pub use daudio_dsp;
pub use daudio_sdk_macros::daudio_plugin;
pub use effect::DaudioEffect;
pub use meter::PeakLevel;
pub use nih_plug;
pub use params::{db_gain_param, hz_param};
pub use synth::DaudioSynth;
pub use voice::{Voice, VoiceManager};

/// Glob-import for plugin authors.
pub mod prelude {
    pub use crate::audio_to_midi::DaudioAudioToMidi;
    pub use crate::effect::DaudioEffect;
    pub use crate::meter::PeakLevel;
    pub use crate::params::{db_gain_param, hz_param};
    pub use crate::synth::DaudioSynth;
    pub use crate::voice::{Voice, VoiceManager};
    pub use daudio_sdk_macros::daudio_plugin;
    pub use nih_plug::prelude::*;
    pub use std::sync::Arc;
}
