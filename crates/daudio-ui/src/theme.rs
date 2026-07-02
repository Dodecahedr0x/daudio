//! Shared dark theme for daudio plugin editors.

use nih_plug_vizia::vizia::prelude::*;

/// The daudio suite accent color.
pub const ACCENT: Color = Color::rgb(0x5e, 0x8b, 0xff);

/// Register the embedded daudio stylesheet on the given context.
///
/// Mirrors `nih_plug_vizia`'s own theme registration (`cx.add_stylesheet` +
/// `include_style!`); the stylesheet is embedded at compile time and its errors
/// are logged rather than propagated, matching the upstream pattern.
pub fn apply_theme(cx: &mut Context) {
    if let Err(err) = cx.add_stylesheet(include_style!("src/theme.css")) {
        nih_plug::nih_error!("Failed to load daudio stylesheet: {err:?}");
    }
}
