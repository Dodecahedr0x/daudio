//! A click-to-toggle [`NoteToggle`] button bound to a [`BoolParam`], whose
//! caption is an absolute note name derived from a shared `root` pitch class
//! plus this toggle's fixed `degree` — and relabelled live when the root moves.
//!
//! Built on the same [`ParamWidgetBase`] plumbing as [`crate::Knob`], so
//! host/automation wiring is shared. Unlike the knob (a canvas leaf), this is a
//! *composite*: a container styled from the bool param's value, wrapping a
//! [`Label`] whose text is recomputed inside a [`Binding`] on the `root` lens.

use nih_plug::prelude::{BoolParam, Param};
use nih_plug_vizia::vizia::prelude::*;
use nih_plug_vizia::widgets::param_base::ParamWidgetBase;

/// The twelve pitch-class names, indexed by pitch class `0..=11` (`0` = C).
/// Inlined here so `daudio-ui` need not depend on `daudio-dsp`.
pub const PITCH_CLASS_NAMES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

/// Background when the toggle is on. `rgb(94, 139, 255)` ≈ `#5e8bff`, the suite
/// accent (kept in sync by hand with [`crate::ACCENT`] / `theme.css`).
const ON_COLOR: Color = Color::rgb(94, 139, 255);
/// Background when the toggle is off — a dark neutral matching the theme.
const OFF_COLOR: Color = Color::rgb(0x2a, 0x2a, 0x32);

/// A click-to-toggle button bound to a [`BoolParam`], captioned with an absolute
/// note name.
///
/// The caption is `PITCH_CLASS_NAMES[(root + degree) % 12]`, where `root` is a
/// live lens over the current root pitch class and `degree` is fixed at
/// construction. Twelve of these sharing one `root` lens, each with a distinct
/// `degree`, form a scale-degree keyboard that relabels itself when the root
/// changes.
pub struct NoteToggle {
    param_base: ParamWidgetBase,
}

impl NoteToggle {
    /// Create a new [`NoteToggle`].
    ///
    /// `params` / `params_to_param` select the [`BoolParam`] exactly as for
    /// [`crate::Knob::new`]. `degree` is this toggle's fixed offset above the
    /// root; `root` is a lens over the current root pitch class (`0..=11`) used
    /// to compute — and live-update — the note-name caption.
    #[allow(clippy::new_ret_no_self)]
    pub fn new<'a, L, Params, FMap, RL>(
        cx: &'a mut Context,
        params: L,
        params_to_param: FMap,
        degree: u8,
        root: RL,
    ) -> Handle<'a, Self>
    where
        L: Lens<Target = Params> + Clone,
        Params: 'static,
        FMap: Fn(&Params) -> &BoolParam + Copy + 'static,
        RL: Lens<Target = u8> + Clone,
    {
        Self {
            param_base: ParamWidgetBase::new(cx, params, params_to_param),
        }
        .build(cx, move |cx| {
            // The caption recomputes whenever the root pitch class changes.
            Binding::new(cx, root, move |cx, root| {
                let name = PITCH_CLASS_NAMES[(root.get(cx) as usize + degree as usize) % 12];
                Label::new(cx, name).hoverable(false);
            });
        })
        // Default inline styling so the toggle is visible and legible without a
        // stylesheet; `.daudio-note-toggle` rules can still override these.
        .width(Pixels(34.0))
        .height(Pixels(44.0))
        .child_space(Stretch(1.0))
        // Background reflects the param's current value, live.
        .background_color(ParamWidgetBase::make_lens(
            params,
            params_to_param,
            |param| {
                if param.modulated_normalized_value() >= 0.5 {
                    ON_COLOR
                } else {
                    OFF_COLOR
                }
            },
        ))
        // Also expose the on/off state as the `:checked` pseudoclass for theming.
        .checked(ParamWidgetBase::make_lens(
            params,
            params_to_param,
            |param| param.modulated_normalized_value() >= 0.5,
        ))
    }

    /// Flip the bool param between its min (0.0) and max (1.0) normalized value.
    fn toggle_value(&self, cx: &mut EventContext) {
        let current_value = self.param_base.unmodulated_normalized_value();
        let new_value = if current_value >= 0.5 { 0.0 } else { 1.0 };

        self.param_base.begin_set_parameter(cx);
        self.param_base.set_normalized_value(cx, new_value);
        self.param_base.end_set_parameter(cx);
    }
}

impl View for NoteToggle {
    fn element(&self) -> Option<&'static str> {
        Some("daudio-note-toggle")
    }

    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|window_event, meta| match window_event {
            WindowEvent::MouseDown(MouseButton::Left)
            | WindowEvent::MouseDoubleClick(MouseButton::Left)
            | WindowEvent::MouseTripleClick(MouseButton::Left) => {
                self.toggle_value(cx);
                meta.consume();
            }
            _ => {}
        });
    }
}
