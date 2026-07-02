//! A themed vertical peak-level meter.
//!
//! Leaf view that reads a [`PeakLevel`] channel (written by the audio thread)
//! and draws a bottom-anchored fill bar. Unlike the [`crate::Knob`], the meter
//! has no lens to react to: nothing in the vizia data graph changes when the
//! audio thread updates the level. So we drive repaints with a repeating timer
//! that calls `needs_redraw()` each tick, mirroring the approach used to keep
//! `nih_plug_vizia`'s own animated widgets ticking.

use std::time::Duration;

use daudio_sdk::PeakLevel;
use nih_plug_vizia::vizia::prelude::*;
use nih_plug_vizia::vizia::vg;

/// The dBFS value mapped to an empty bar (bottom of the meter).
const MIN_DB: f32 = -60.0;
/// Repaint interval — ~30 fps, enough for a smooth meter without burning CPU.
const REDRAW_INTERVAL: Duration = Duration::from_millis(33);

/// A vertical peak-level meter bound to a [`PeakLevel`] channel.
pub struct Meter {
    level: PeakLevel,
}

impl Meter {
    /// Build a meter reading from `level`. Starts a repeating repaint timer so
    /// the bar animates as the audio thread writes new peaks.
    pub fn new(cx: &mut Context, level: PeakLevel) -> Handle<'_, Self> {
        Self { level }.build(cx, |cx| {
            // Drive ~30 fps repaints so the meter animates without a lens. The
            // callback fires on every tick and just marks the view dirty.
            let timer = cx.add_timer(REDRAW_INTERVAL, None, |cx, action| {
                if let TimerAction::Tick(_) = action {
                    cx.needs_redraw();
                }
            });
            cx.start_timer(timer);
        })
    }
}

impl View for Meter {
    fn element(&self) -> Option<&'static str> {
        Some("daudio-meter")
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &mut Canvas) {
        let b = cx.bounds();
        if b.w == 0.0 || b.h == 0.0 {
            return;
        }

        // Map the current peak (in linear gain) to a [0, 1] fill fraction.
        let db = nih_plug::util::gain_to_db(self.level.read().max(1e-6));
        let t = ((db - MIN_DB) / -MIN_DB).clamp(0.0, 1.0);

        // Dark track over the full height.
        let mut track = vg::Path::new();
        track.rect(b.x, b.y, b.w, b.h);
        canvas.fill_path(&track, &vg::Paint::color(vg::Color::rgb(0x24, 0x24, 0x2c)));

        // Filled portion anchored at the BOTTOM: height `t * b.h`, so it grows
        // upward from the base of the meter.
        if t > 0.0 {
            let fill_h = b.h * t;
            let mut fill = vg::Path::new();
            fill.rect(b.x, b.y + b.h - fill_h, b.w, fill_h);
            canvas.fill_path(&fill, &vg::Paint::color(crate::theme::ACCENT));
        }
    }
}
