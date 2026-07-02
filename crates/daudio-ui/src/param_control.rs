//! A labeled parameter control: a caption stacked above a [`ParamSlider`].

use nih_plug::prelude::Param;
use nih_plug_vizia::vizia::prelude::*;
use nih_plug_vizia::widgets::ParamSlider;

/// A caption label stacked above a themed [`ParamSlider`].
///
/// The generic bounds mirror [`ParamSlider::new`] verbatim (lens over a
/// `Params` value plus a `Fn(&Params) -> &P` accessor); the only addition is
/// the `.daudio-label` caption above the slider.
pub struct ParamControl;

impl ParamControl {
    /// Build a labeled control. `label` is the caption; `params` is the lens to
    /// the params struct and `params_to_param` selects the parameter, exactly as
    /// for [`ParamSlider::new`].
    ///
    /// Returns a [`Handle`] to the wrapping container rather than `Self`,
    /// following vizia's builder convention (as [`ParamSlider::new`] does).
    #[allow(clippy::new_ret_no_self)]
    pub fn new<'a, L, Params, P, FMap>(
        cx: &'a mut Context,
        label: &'static str,
        params: L,
        params_to_param: FMap,
    ) -> Handle<'a, VStack>
    where
        L: Lens<Target = Params> + Clone,
        Params: 'static,
        P: Param + 'static,
        FMap: Fn(&Params) -> &P + Copy + 'static,
    {
        VStack::new(cx, move |cx| {
            Label::new(cx, label).class("daudio-label");
            ParamSlider::new(cx, params, params_to_param);
        })
        .class("daudio-control")
    }
}
