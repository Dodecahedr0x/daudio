//! Shared dark theme for daudio plugin editors.

use nih_plug_vizia::vizia::prelude::*;
use nih_plug_vizia::vizia::vg;

/// Suite accent — the single branding source for the canvas-drawn widgets
/// ([`crate::Knob`], [`crate::Meter`]).
///
/// `rgb(94, 139, 255)` ≈ `#5e8bff`. NOTE: the `.daudio-*` rules in `theme.css`
/// hard-code this same hex separately — CSS-styled and canvas-drawn widgets do
/// not share one literal. Keep the two in sync by hand if the brand changes.
pub const ACCENT: vg::Color = vg::Color {
    r: 0.369,
    g: 0.545,
    b: 1.0,
    a: 1.0,
};

/// Register the embedded daudio stylesheet on the given context.
///
/// Mirrors `nih_plug_vizia`'s own theme registration (`cx.add_stylesheet` +
/// `include_style!`); the stylesheet is embedded at compile time and its errors
/// are logged rather than propagated, matching the upstream pattern.
pub fn apply_theme(cx: &mut Context) {
    // Embed the stylesheet at compile time (`&'static str: IntoCssStr`) rather
    // than `include_style!`, whose debug-build variant reads the file from a
    // compile-time absolute path at runtime — fragile once the crate moves.
    if let Err(err) = cx.add_stylesheet(include_str!("theme.css")) {
        nih_plug::nih_error!("Failed to load daudio stylesheet: {err:?}");
    }
}
