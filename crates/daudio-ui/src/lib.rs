//! Shared Vizia UI toolkit for daudio plugins: a dark theme, a labeled
//! [`ParamControl`] widget, and an editor helper that hides the
//! `create_vizia_editor` + Lens/Model boilerplate.

mod editor;
mod knob;
mod layout;
mod meter;
mod note_toggle;
mod param_control;
mod theme;

pub use editor::{create_editor, editor_state, DaudioData};
pub use knob::Knob;
pub use layout::{card, card_column};
pub use meter::Meter;
pub use note_toggle::NoteToggle;
pub use param_control::ParamControl;
pub use theme::{
    apply_theme, ACCENT, ACCENT_BRIGHT, BG, BORDER, PANEL, SURFACE, TEXT, TEXT_DIM, TEXT_MUTED,
};

// Re-export so plugins can name the audio→UI meter channel via daudio-ui
// (which they already depend on for the `Meter` widget).
pub use daudio_sdk::PeakLevel;

// Re-export so downstream plugins can name `ViziaState` / vizia widgets without
// adding their own `nih_plug_vizia` dependency.
pub use nih_plug_vizia;
pub use nih_plug_vizia::ViziaState;

/// Convenient glob import for building daudio editors.
pub mod prelude {
    pub use crate::{
        apply_theme, card, card_column, create_editor, editor_state, DaudioData, Knob, Meter,
        NoteToggle, ParamControl, PeakLevel,
    };
    pub use nih_plug_vizia::vizia::prelude::*;
    pub use nih_plug_vizia::{ViziaState, ViziaTheming};
}
