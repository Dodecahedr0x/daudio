pub mod dsp;

use crate::dsp::FilterCore;
use daudio_sdk::prelude::*;
use daudio_ui::nih_plug_vizia;
use daudio_ui::prelude::*;

#[derive(Params)]
pub struct FilterParams {
    #[persist = "editor-state"]
    editor_state: Arc<nih_plug_vizia::ViziaState>,
    #[id = "cutoff"]
    cutoff: FloatParam,
    #[id = "gain"]
    gain: FloatParam,
}

impl Default for FilterParams {
    fn default() -> Self {
        Self {
            editor_state: daudio_ui::editor_state(240, 160),
            cutoff: hz_param("Cutoff", 1000.0, 20.0, 20_000.0),
            gain: db_gain_param("Gain", -60.0, 6.0, 0.0),
        }
    }
}

#[daudio_plugin(
    name = "daudio Filter",
    vendor = "daudio",
    url = "https://example.com",
    email = "hexadecifish@gmail.com",
    clap_id = "com.daudio.filter",
    clap_description = "A simple lowpass filter with gain",
    vst3_id = "daudioFilter0001",
    clap_features = [AudioEffect, Stereo, Filter],
    vst3_categories = [Fx, Filter]
)]
pub struct FilterPlugin {
    params: Arc<FilterParams>,
    core: FilterCore,
}

impl Default for FilterPlugin {
    fn default() -> Self {
        Self {
            params: Arc::new(FilterParams::default()),
            core: FilterCore::new(),
        }
    }
}

impl DaudioEffect for FilterPlugin {
    type Params = FilterParams;

    fn editor(&mut self) -> Option<Box<dyn Editor>> {
        daudio_ui::create_editor(
            self.params.editor_state.clone(),
            self.params.clone(),
            |cx| {
                VStack::new(cx, |cx| {
                    Label::new(cx, "daudio Filter").class("daudio-title");
                    HStack::new(cx, |cx| {
                        ParamControl::new(cx, "Cutoff", DaudioData::<FilterParams>::params, |p| {
                            &p.cutoff
                        });
                        ParamControl::new(cx, "Gain", DaudioData::<FilterParams>::params, |p| {
                            &p.gain
                        });
                    })
                    .class("daudio-row");
                })
                .class("daudio-panel");
            },
        )
    }

    fn activate(&mut self, sample_rate: f32) {
        self.core.set_sample_rate(sample_rate);
        self.core.snap_gain(self.params.gain.value());
    }

    fn reset(&mut self) {
        // Clear filter state, then re-snap the gain smoother to the current
        // target so a transport restart doesn't glide from a stale value.
        // (FilterCore::reset only clears the biquads; the gain target lives in
        // the param, which only the adapter can see — mirror `activate`.)
        self.core.reset();
        self.core.snap_gain(self.params.gain.value());
    }

    fn pre_block(&mut self) {
        // Cutoff is applied once per buffer (not smoothed) — a deliberate
        // first-milestone tradeoff; fast cutoff automation may zipper at buffer
        // boundaries. Add smoothing here if a later plugin needs it.
        self.core.set_cutoff(self.params.cutoff.value());
    }

    fn process_frame(&mut self, left: f32, right: f32) -> (f32, f32) {
        // FilterCore's internal OnePole smooths toward this target, so pull the
        // param value once and let the core be the single 20 ms smoother.
        let gain_db = self.params.gain.value();
        self.core.process_frame(left, right, gain_db)
    }
}
