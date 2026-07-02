//! A rotary [`Knob`] that integrates with NIH-plug's [`Param`] types.
//!
//! Built on the same [`ParamWidgetBase`] that `nih_plug_vizia`'s own
//! `ParamSlider` uses, so all host/automation plumbing is shared. The knob is a
//! leaf view that draws itself directly on the femtovg canvas and handles its
//! own vertical-drag / scroll / double-click-reset gestures.

use nih_plug::prelude::Param;
use nih_plug_vizia::vizia::prelude::*;
use nih_plug_vizia::vizia::vg;
use nih_plug_vizia::widgets::param_base::ParamWidgetBase;
use nih_plug_vizia::widgets::RawParamEvent;

/// Pixels of vertical drag for a full `[0, 1]` sweep of the parameter.
const DRAG_PIXELS_PER_RANGE: f32 = 200.0;
/// Normalized amount changed per scroll-wheel notch. Scroll is continuous-only:
/// it does not respect discrete-parameter steps, which is fine for the current
/// continuous params.
const SCROLL_STEP: f32 = 0.02;
/// The arc leaves a gap at the bottom: the sweep spans 270°, centred on the top.
const START_ANGLE_DEG: f32 = 135.0;
const SWEEP_DEG: f32 = 270.0;

/// A rotary knob bound to a NIH-plug [`Param`].
///
/// Construct with [`Knob::new`], whose generic bounds mirror `ParamSlider::new`
/// verbatim: a lens over the `Params` value plus a `Fn(&Params) -> &P`
/// accessor.
pub struct Knob {
    param_base: ParamWidgetBase,
    /// Whether a left-button drag is in progress.
    dragging: bool,
    /// Whether the pointer is currently over the knob (drives the hover accent).
    hovering: bool,
    /// Mouse Y at the start of the current drag (logical pixels).
    drag_start_y: f32,
    /// Normalized parameter value at the start of the current drag.
    drag_start_value: f32,
}

impl Knob {
    /// Create a new [`Knob`] for the given parameter. `params` is a lens to the
    /// `Params` struct and `params_to_param` selects the parameter, exactly as
    /// for `ParamSlider::new`.
    pub fn new<'a, L, Params, P, FMap>(
        cx: &'a mut Context,
        params: L,
        params_to_param: FMap,
    ) -> Handle<'a, Self>
    where
        L: Lens<Target = Params> + Clone,
        Params: 'static,
        P: Param + 'static,
        FMap: Fn(&Params) -> &P + Copy + 'static,
    {
        Self {
            param_base: ParamWidgetBase::new(cx, params, params_to_param),
            dragging: false,
            hovering: false,
            drag_start_y: 0.0,
            drag_start_value: 0.0,
        }
        // Leaf view: no children, we draw everything ourselves.
        .build(cx, |_| {})
        // Default size so the knob is visible without any stylesheet; a theme
        // rule for `.daudio-knob` can still override these.
        .width(Pixels(60.0))
        .height(Pixels(60.0))
    }
}

impl View for Knob {
    fn element(&self) -> Option<&'static str> {
        Some("daudio-knob")
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &mut Canvas) {
        let bounds = cx.bounds();
        if bounds.w == 0.0 || bounds.h == 0.0 {
            return;
        }

        let value = self
            .param_base
            .unmodulated_normalized_value()
            .clamp(0.0, 1.0);

        // Geometry: centred circle inset a little so the widest stroke (the glow)
        // stays inside the bounds.
        let cx_px = bounds.x + bounds.w / 2.0;
        let cy_px = bounds.y + bounds.h / 2.0;
        let pad = 6.0;
        let arc_width = 5.0;
        let radius = (bounds.w.min(bounds.h) / 2.0) - pad;
        if radius <= 0.0 {
            return;
        }

        // femtovg angles run clockwise from the +x axis in radians. Our sweep
        // starts at 135° and runs 270° clockwise to 405° (== 45°).
        let start = START_ANGLE_DEG.to_radians();
        let end = (START_ANGLE_DEG + SWEEP_DEG).to_radians();
        let value_end = (START_ANGLE_DEG + SWEEP_DEG * value).to_radians();

        // The value arc brightens on hover; the body/track stay constant.
        let value_color = if self.hovering {
            crate::theme::ACCENT_BRIGHT
        } else {
            crate::theme::ACCENT
        };

        // (1) Soft glow: the value arc drawn wider and faint underneath, a subtle
        // bloom. Slightly stronger while hovering. Only when there is a value.
        if value > 0.0 {
            let mut glow_color = value_color;
            glow_color.a = if self.hovering { 0.28 } else { 0.18 };
            let mut glow = vg::Path::new();
            glow.arc(cx_px, cy_px, radius, start, value_end, vg::Solidity::Hole);
            let mut glow_paint = vg::Paint::color(glow_color);
            glow_paint.set_line_width(9.0);
            glow_paint.set_line_cap(vg::LineCap::Round);
            canvas.stroke_path(&glow, &glow_paint);
        }

        // (2) Background track arc over the full sweep.
        let mut track = vg::Path::new();
        track.arc(cx_px, cy_px, radius, start, end, vg::Solidity::Hole);
        let mut track_paint = vg::Paint::color(crate::theme::SURFACE);
        track_paint.set_line_width(arc_width);
        track_paint.set_line_cap(vg::LineCap::Round);
        canvas.stroke_path(&track, &track_paint);

        // (3) Crisp value arc from the start up to the current value.
        if value > 0.0 {
            let mut fill = vg::Path::new();
            fill.arc(cx_px, cy_px, radius, start, value_end, vg::Solidity::Hole);
            let mut fill_paint = vg::Paint::color(value_color);
            fill_paint.set_line_width(arc_width);
            fill_paint.set_line_cap(vg::LineCap::Round);
            canvas.stroke_path(&fill, &fill_paint);
        }

        // (4) Physical knob cap: a filled body circle with a subtle rim.
        let body_radius = radius * 0.62;
        let mut body = vg::Path::new();
        body.circle(cx_px, cy_px, body_radius);
        canvas.fill_path(&body, &vg::Paint::color(crate::theme::SURFACE));
        let mut rim_paint = vg::Paint::color(crate::theme::BORDER);
        rim_paint.set_line_width(1.0);
        canvas.stroke_path(&body, &rim_paint);

        // (5) Indicator tick sitting on the cap, pointing at the value angle.
        let (sin, cos) = value_end.sin_cos();
        let inner = radius * 0.34;
        let outer = radius * 0.60;
        let mut tick = vg::Path::new();
        tick.move_to(cx_px + cos * inner, cy_px + sin * inner);
        tick.line_to(cx_px + cos * outer, cy_px + sin * outer);
        let mut tick_paint = vg::Paint::color(crate::theme::TEXT);
        tick_paint.set_line_width(3.0);
        tick_paint.set_line_cap(vg::LineCap::Round);
        canvas.stroke_path(&tick, &tick_paint);
    }

    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|window_event, meta| match window_event {
            WindowEvent::MouseDown(MouseButton::Left) => {
                self.param_base.begin_set_parameter(cx);
                self.drag_start_value = self.param_base.unmodulated_normalized_value();
                self.drag_start_y = cx.mouse().cursory;
                self.dragging = true;
                cx.capture();
                cx.focus();
                meta.consume();
            }
            WindowEvent::MouseMove(_x, y) if self.dragging => {
                let delta = (self.drag_start_y - *y) / DRAG_PIXELS_PER_RANGE;
                let new_value = (self.drag_start_value + delta).clamp(0.0, 1.0);
                self.param_base.set_normalized_value(cx, new_value);
                cx.needs_redraw();
            }
            WindowEvent::MouseUp(MouseButton::Left) if self.dragging => {
                self.dragging = false;
                cx.release();
                self.param_base.end_set_parameter(cx);
                meta.consume();
            }
            WindowEvent::MouseDoubleClick(MouseButton::Left) => {
                // Vizia sends MouseDown(Left) before MouseDoubleClick(Left), so a
                // drag gesture is already open here. Tear it down first so we can't
                // leave a dangling captured/begun gesture if the trailing MouseUp
                // is dropped, then reset to the default value.
                if self.dragging {
                    self.dragging = false;
                    cx.release();
                    self.param_base.end_set_parameter(cx);
                }
                self.param_base.begin_set_parameter(cx);
                self.param_base
                    .set_normalized_value(cx, self.param_base.default_normalized_value());
                self.param_base.end_set_parameter(cx);
                cx.needs_redraw();
                meta.consume();
            }
            WindowEvent::MouseScroll(_scroll_x, scroll_y) if *scroll_y != 0.0 => {
                let current = self.param_base.unmodulated_normalized_value();
                let new_value = (current + scroll_y.signum() * SCROLL_STEP).clamp(0.0, 1.0);
                self.param_base.begin_set_parameter(cx);
                self.param_base.set_normalized_value(cx, new_value);
                self.param_base.end_set_parameter(cx);
                cx.needs_redraw();
                meta.consume();
            }
            WindowEvent::MouseEnter => {
                self.hovering = true;
                cx.needs_redraw();
            }
            WindowEvent::MouseLeave => {
                self.hovering = false;
                cx.needs_redraw();
            }
            _ => {}
        });

        // Host automation, preset loads, and any other external parameter change
        // arrive as `RawParamEvent`s (delivered on idle). Repaint so the knob
        // tracks the DAW instead of freezing at its last drawn value.
        event.map(|param_event, _meta| match param_event {
            RawParamEvent::ParametersChanged
            | RawParamEvent::BeginSetParameter(_)
            | RawParamEvent::SetParameterNormalized(_, _)
            | RawParamEvent::EndSetParameter(_) => cx.needs_redraw(),
        });
    }
}
