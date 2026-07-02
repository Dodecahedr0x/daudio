pub mod dsp;

use crate::dsp::FilterCore;
use nih_plug::prelude::*;
use std::sync::Arc;

#[derive(Params)]
struct FilterParams {
    #[id = "cutoff"]
    cutoff: FloatParam,
    #[id = "gain"]
    gain: FloatParam,
}

impl Default for FilterParams {
    fn default() -> Self {
        Self {
            cutoff: FloatParam::new(
                "Cutoff",
                1000.0,
                FloatRange::Skewed {
                    min: 20.0,
                    max: 20_000.0,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_unit(" Hz")
            .with_value_to_string(formatters::v2s_f32_hz_then_khz(1))
            .with_string_to_value(formatters::s2v_f32_hz_then_khz()),
            gain: FloatParam::new(
                "Gain",
                0.0,
                FloatRange::Linear {
                    min: -60.0,
                    max: 6.0,
                },
            )
            .with_unit(" dB"),
        }
    }
}

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

impl Plugin for FilterPlugin {
    const NAME: &'static str = "daudio Filter";
    const VENDOR: &'static str = "daudio";
    const URL: &'static str = "https://example.com";
    const EMAIL: &'static str = "hexadecifish@gmail.com";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: NonZeroU32::new(2),
        main_output_channels: NonZeroU32::new(2),
        ..AudioIOLayout::const_default()
    }];

    const SAMPLE_ACCURATE_AUTOMATION: bool = true;

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn initialize(
        &mut self,
        _layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        self.core.set_sample_rate(buffer_config.sample_rate);
        self.core.snap_gain(self.params.gain.value());
        true
    }

    fn reset(&mut self) {
        // Clear filter state, then re-snap the gain smoother to the current
        // target so a transport restart doesn't glide from a stale value.
        // (FilterCore::reset only clears the biquads; the gain target lives in
        // the param, which only the adapter can see — mirror `initialize`.)
        self.core.reset();
        self.core.snap_gain(self.params.gain.value());
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        // Cutoff is applied once per buffer (not smoothed) — a deliberate
        // first-milestone tradeoff; fast cutoff automation may zipper at buffer
        // boundaries. Add smoothing here if a later plugin needs it.
        self.core.set_cutoff(self.params.cutoff.value());

        // FilterCore's internal OnePole smooths toward this target, so pull the
        // param value once and let the core be the single 20 ms smoother.
        let gain_db = self.params.gain.value();

        for mut frame in buffer.iter_samples() {
            // Stereo is guaranteed by AUDIO_IO_LAYOUTS; this guard prevents an
            // RT-thread panic/UB if a host ignores the declared layout.
            if frame.len() < 2 {
                continue;
            }
            let l = *frame.get_mut(0).unwrap();
            let r = *frame.get_mut(1).unwrap();
            let (ol, or) = self.core.process_frame(l, r, gain_db);
            *frame.get_mut(0).unwrap() = ol;
            *frame.get_mut(1).unwrap() = or;
        }
        ProcessStatus::Normal
    }
}

impl ClapPlugin for FilterPlugin {
    const CLAP_ID: &'static str = "com.daudio.filter";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("A simple lowpass filter with gain");
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Stereo,
        ClapFeature::Filter,
    ];
}

impl Vst3Plugin for FilterPlugin {
    const VST3_CLASS_ID: [u8; 16] = *b"daudioFilter0001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Filter];
}

nih_export_clap!(FilterPlugin);
nih_export_vst3!(FilterPlugin);
