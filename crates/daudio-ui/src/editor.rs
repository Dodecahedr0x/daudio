//! Editor helper: hides the `create_vizia_editor` + Lens/Model boilerplate.

use std::sync::Arc;

use nih_plug::prelude::Editor;
use nih_plug_vizia::vizia::prelude::*;
use nih_plug_vizia::{assets, create_vizia_editor, ViziaState, ViziaTheming};

use crate::theme::apply_theme;

/// Create the persisted [`ViziaState`] for an editor of the given logical size.
///
/// Thin wrapper over [`ViziaState::new`] so plugins don't need to depend on
/// `nih_plug_vizia` directly just to size their window.
pub fn editor_state(width: u32, height: u32) -> Arc<ViziaState> {
    ViziaState::new(move || (width, height))
}

/// Data model holding the plugin's params, exposed to widgets via the
/// `DaudioData::<Params>::params` lens (the same shape `ParamSlider::new`
/// expects: `L: Lens<Target = Arc<Params>>`).
#[derive(Lens)]
pub struct DaudioData<Params: 'static> {
    /// Shared parameter handle. Widgets read this via the generated `params` lens.
    pub params: Arc<Params>,
}

impl<Params: 'static> Model for DaudioData<Params> {}

/// Build a themed vizia editor.
///
/// Registers the fonts required by [`ViziaTheming::Custom`], applies the daudio
/// stylesheet, builds the [`DaudioData`] model into the context (so the
/// `DaudioData::<Params>::params` lens resolves), and then runs the caller's
/// `content` closure to build the widget tree.
pub fn create_editor<Params, C>(
    state: Arc<ViziaState>,
    params: Arc<Params>,
    content: C,
) -> Option<Box<dyn Editor>>
where
    Params: 'static + Send + Sync,
    C: Fn(&mut Context) + 'static + Send + Sync,
{
    create_vizia_editor(state, ViziaTheming::Custom, move |cx, _| {
        // Required for the `Custom` theming level.
        assets::register_noto_sans_light(cx);

        apply_theme(cx);

        DaudioData {
            params: params.clone(),
        }
        .build(cx);

        content(cx);
    })
}
