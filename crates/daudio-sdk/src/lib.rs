//! daudio-sdk: author-facing facade for building daudio plugins.

pub mod effect;
pub mod params;
pub mod voice;

pub use daudio_dsp;
pub use daudio_sdk_macros::daudio_plugin;
pub use effect::DaudioEffect;
pub use nih_plug;
pub use params::{db_gain_param, hz_param};
pub use voice::{Voice, VoiceManager};

/// Glob-import for plugin authors.
pub mod prelude {
    pub use crate::effect::DaudioEffect;
    pub use crate::params::{db_gain_param, hz_param};
    pub use crate::voice::{Voice, VoiceManager};
    pub use daudio_sdk_macros::daudio_plugin;
    pub use nih_plug::prelude::*;
    pub use std::sync::Arc;
}
