//! Shared Vizia UI toolkit for daudio plugins: a dark theme, a labeled
//! [`ParamControl`] widget, and an editor helper that hides the
//! `create_vizia_editor` + Lens/Model boilerplate.

mod editor;
mod knob;
mod param_control;
mod theme;

pub use editor::{create_editor, editor_state, DaudioData};
pub use knob::Knob;
pub use param_control::ParamControl;
pub use theme::apply_theme;

// Re-export so downstream plugins can name `ViziaState` / vizia widgets without
// adding their own `nih_plug_vizia` dependency.
pub use nih_plug_vizia;
pub use nih_plug_vizia::ViziaState;

/// Convenient glob import for building daudio editors.
pub mod prelude {
    pub use crate::{apply_theme, create_editor, editor_state, DaudioData, Knob, ParamControl};
    pub use nih_plug_vizia::vizia::prelude::*;
    pub use nih_plug_vizia::{ViziaState, ViziaTheming};
}
