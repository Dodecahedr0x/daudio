//! Shared dark theme for daudio plugin editors.
//!
//! This module is the single source of truth for the suite's palette as seen by
//! the *canvas-drawn* widgets ([`crate::Knob`], [`crate::Meter`]) — each color
//! below is a [`vg::Color`]. The *CSS-styled* widgets are themed from
//! `theme.css`, which hard-codes the very same hex values in its `.daudio-*`
//! rules. The two files do NOT share one literal, so **keep them in sync by
//! hand**: every `pub const` here mirrors a `#rrggbb` in `theme.css`.

use nih_plug_vizia::vizia::prelude::*;
use nih_plug_vizia::vizia::vg;

/// Base window background — `#16161c`.
pub const BG: vg::Color = vg::Color::rgbf(
    0x16 as f32 / 255.0,
    0x16 as f32 / 255.0,
    0x1c as f32 / 255.0,
);
/// Elevated card / panel background — `#1f1f29`.
pub const PANEL: vg::Color = vg::Color::rgbf(
    0x1f as f32 / 255.0,
    0x1f as f32 / 255.0,
    0x29 as f32 / 255.0,
);
/// Control background (knob body, toggle-off, meter track) — `#2c2c38`.
pub const SURFACE: vg::Color = vg::Color::rgbf(
    0x2c as f32 / 255.0,
    0x2c as f32 / 255.0,
    0x38 as f32 / 255.0,
);
/// Subtle border / divider — `#383843`.
pub const BORDER: vg::Color = vg::Color::rgbf(
    0x38 as f32 / 255.0,
    0x38 as f32 / 255.0,
    0x43 as f32 / 255.0,
);

/// Suite primary accent — `#5b8cff`. The single branding source for the
/// canvas-drawn widgets; `theme.css` mirrors this hex in its `.daudio-*` rules.
pub const ACCENT: vg::Color = vg::Color::rgbf(
    0x5b as f32 / 255.0,
    0x8c as f32 / 255.0,
    0xff as f32 / 255.0,
);
/// Brighter accent used for hover / glow — `#83a6ff`.
pub const ACCENT_BRIGHT: vg::Color = vg::Color::rgbf(
    0x83 as f32 / 255.0,
    0xa6 as f32 / 255.0,
    0xff as f32 / 255.0,
);

/// Primary text — `#f2f3f7`.
pub const TEXT: vg::Color = vg::Color::rgbf(
    0xf2 as f32 / 255.0,
    0xf3 as f32 / 255.0,
    0xf7 as f32 / 255.0,
);
/// Secondary / label text — `#9a9ba8`.
pub const TEXT_DIM: vg::Color = vg::Color::rgbf(
    0x9a as f32 / 255.0,
    0x9b as f32 / 255.0,
    0xa8 as f32 / 255.0,
);
/// Muted text — `#6a6b78`.
pub const TEXT_MUTED: vg::Color = vg::Color::rgbf(
    0x6a as f32 / 255.0,
    0x6b as f32 / 255.0,
    0x78 as f32 / 255.0,
);

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
