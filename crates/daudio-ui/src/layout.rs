//! Small layout helpers for building daudio editors.
//!
//! Layout is driven by inline vizia modifiers rather than the stylesheet's
//! layout properties (`child-space`, `row-between`, `col-between`, …), which do
//! not lay out nested containers reliably in the pinned `nih_plug_vizia` /
//! `vizia_baseview` build. The stylesheet is used only for *decoration*
//! (colours, borders, radii); spacing and sizing live here in code.

use nih_plug_vizia::vizia::prelude::*;

/// A titled "card": a `.daudio-card`-styled column with a small section heading
/// above a horizontal row of `content` controls.
///
/// ```ignore
/// card(cx, "FILTER", |cx| {
///     ParamControl::new(cx, "Cutoff", lens, |p| &p.cutoff);
///     ParamControl::new(cx, "Reso",   lens, |p| &p.reso);
/// });
/// ```
pub fn card(cx: &mut Context, title: &str, content: impl FnOnce(&mut Context)) {
    VStack::new(cx, |cx| {
        Label::new(cx, title).class("daudio-section");
        HStack::new(cx, |cx| content(cx))
            .height(Auto)
            .width(Auto)
            .child_top(Pixels(2.0))
            .col_between(Pixels(14.0));
    })
    .class("daudio-card")
    .height(Auto)
    .width(Auto)
    .child_space(Pixels(14.0))
    .row_between(Pixels(10.0));
}

/// A vertical group like [`card`] but stacking its `content` in a column (for a
/// heading over rows of controls rather than a single row).
pub fn card_column(cx: &mut Context, title: &str, content: impl FnOnce(&mut Context)) {
    VStack::new(cx, |cx| {
        Label::new(cx, title).class("daudio-section");
        VStack::new(cx, |cx| content(cx))
            .height(Auto)
            .width(Auto)
            .child_top(Pixels(2.0))
            .row_between(Pixels(10.0));
    })
    .class("daudio-card")
    .height(Auto)
    .width(Auto)
    .child_space(Pixels(14.0))
    .row_between(Pixels(10.0));
}
