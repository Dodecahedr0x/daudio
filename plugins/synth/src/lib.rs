//! daudio Synth: a minimal polyphonic subtractive synthesizer built on the SDK.

pub mod voice;

use daudio_dsp::gain::db_to_gain;
use daudio_dsp::oscillator::Waveform;
use daudio_sdk::prelude::*;
use daudio_ui::nih_plug_vizia;
use daudio_ui::prelude::*;

use crate::voice::SynthVoice;

/// Oscillator waveform, as a host-automatable enum parameter. Maps 1:1 onto
/// `daudio_dsp::Waveform` (which lives in the host-agnostic DSP crate and so
/// cannot derive nih-plug's `Enum` itself).
#[derive(Enum, Debug, Clone, Copy, PartialEq, Eq)]
enum WaveformChoice {
    Saw,
    Sine,
}

impl From<WaveformChoice> for Waveform {
    fn from(w: WaveformChoice) -> Self {
        match w {
            WaveformChoice::Saw => Waveform::Saw,
            WaveformChoice::Sine => Waveform::Sine,
        }
    }
}

#[derive(Params)]
pub struct SynthParams {
    #[persist = "editor-state"]
    editor_state: Arc<nih_plug_vizia::ViziaState>,
    #[id = "waveform"]
    waveform: EnumParam<WaveformChoice>,
    #[id = "cutoff"]
    cutoff: FloatParam,
    #[id = "resonance"]
    resonance: FloatParam,
    #[id = "envamt"]
    env_amount: FloatParam,
    #[id = "attack"]
    attack: FloatParam,
    #[id = "decay"]
    decay: FloatParam,
    #[id = "sustain"]
    sustain: FloatParam,
    #[id = "release"]
    release: FloatParam,
    #[id = "gain"]
    gain: FloatParam,
}

/// A time-in-seconds parameter with a perceptual (skewed) range.
fn seconds_param(name: &str, default: f32, min: f32, max: f32) -> FloatParam {
    FloatParam::new(
        name,
        default,
        FloatRange::Skewed {
            min,
            max,
            factor: FloatRange::skew_factor(-2.0),
        },
    )
    .with_unit(" s")
}

/// A plain 0..1 parameter.
fn unit_param(name: &str, default: f32) -> FloatParam {
    FloatParam::new(name, default, FloatRange::Linear { min: 0.0, max: 1.0 })
}

impl Default for SynthParams {
    fn default() -> Self {
        Self {
            editor_state: daudio_ui::editor_state(520, 220),
            waveform: EnumParam::new("Waveform", WaveformChoice::Saw),
            cutoff: hz_param("Cutoff", 2_000.0, 20.0, 20_000.0),
            resonance: FloatParam::new(
                "Resonance",
                0.707,
                FloatRange::Linear { min: 0.3, max: 8.0 },
            ),
            env_amount: unit_param("Env Amount", 0.5),
            attack: seconds_param("Attack", 0.01, 0.001, 5.0),
            decay: seconds_param("Decay", 0.1, 0.001, 5.0),
            sustain: unit_param("Sustain", 0.8),
            release: seconds_param("Release", 0.2, 0.001, 5.0),
            gain: db_gain_param("Gain", -60.0, 6.0, -12.0),
        }
    }
}

/// Snapshot of the per-voice configuration, refreshed each block and pushed
/// into active voices. A voice triggered mid-block picks up the config on the
/// next block's `pre_block` (a one-block, inaudible latency).
#[derive(Clone, Copy)]
struct VoiceConfig {
    waveform: Waveform,
    cutoff: f32,
    resonance: f32,
    env_amount: f32,
    attack: f32,
    decay: f32,
    sustain: f32,
    release: f32,
}

#[daudio_plugin(
    name = "daudio Synth",
    vendor = "daudio",
    url = "https://example.com",
    email = "hexadecifish@gmail.com",
    clap_id = "com.daudio.synth",
    clap_description = "A polyphonic subtractive synth",
    vst3_id = "daudioSynth00001",
    clap_features = [Instrument, Synthesizer, Stereo],
    vst3_categories = [Instrument, Synth],
    midi = true
)]
pub struct Synth {
    params: Arc<SynthParams>,
    voices: VoiceManager<SynthVoice>,
}

impl Default for Synth {
    fn default() -> Self {
        Self {
            params: Arc::new(SynthParams::default()),
            voices: VoiceManager::new(16),
        }
    }
}

impl DaudioSynth for Synth {
    type Params = SynthParams;

    fn activate(&mut self, sample_rate: f32) {
        self.voices.set_sample_rate(sample_rate);
    }

    fn reset(&mut self) {
        self.voices.reset();
    }

    fn pre_block(&mut self) {
        let cfg = VoiceConfig {
            waveform: self.params.waveform.value().into(),
            cutoff: self.params.cutoff.value(),
            resonance: self.params.resonance.value(),
            env_amount: self.params.env_amount.value(),
            attack: self.params.attack.value(),
            decay: self.params.decay.value(),
            sustain: self.params.sustain.value(),
            release: self.params.release.value(),
        };
        self.voices.for_each_active(|v| {
            v.set_waveform(cfg.waveform);
            v.set_filter(cfg.cutoff, cfg.resonance);
            v.set_env_amount(cfg.env_amount);
            v.set_adsr(cfg.attack, cfg.decay, cfg.sustain, cfg.release);
        });
    }

    fn note_on(&mut self, note: u8, velocity: f32) {
        self.voices.note_on(note, velocity);
    }

    fn note_off(&mut self, note: u8) {
        self.voices.note_off(note);
    }

    fn render_frame(&mut self) -> (f32, f32) {
        let s = self.voices.render() * db_to_gain(self.params.gain.value());
        (s, s)
    }

    fn editor(&mut self) -> Option<Box<dyn Editor>> {
        daudio_ui::create_editor(
            self.params.editor_state.clone(),
            self.params.clone(),
            |cx| {
                VStack::new(cx, |cx| {
                    Label::new(cx, "daudio Synth").class("daudio-title");
                    HStack::new(cx, |cx| {
                        ParamControl::new(cx, "Cutoff", DaudioData::<SynthParams>::params, |p| {
                            &p.cutoff
                        });
                        ParamControl::new(cx, "Reso", DaudioData::<SynthParams>::params, |p| {
                            &p.resonance
                        });
                        ParamControl::new(cx, "Env Amt", DaudioData::<SynthParams>::params, |p| {
                            &p.env_amount
                        });
                        ParamControl::new(cx, "Gain", DaudioData::<SynthParams>::params, |p| {
                            &p.gain
                        });
                    })
                    .class("daudio-row");
                    HStack::new(cx, |cx| {
                        ParamControl::new(cx, "Attack", DaudioData::<SynthParams>::params, |p| {
                            &p.attack
                        });
                        ParamControl::new(cx, "Decay", DaudioData::<SynthParams>::params, |p| {
                            &p.decay
                        });
                        ParamControl::new(cx, "Sustain", DaudioData::<SynthParams>::params, |p| {
                            &p.sustain
                        });
                        ParamControl::new(cx, "Release", DaudioData::<SynthParams>::params, |p| {
                            &p.release
                        });
                    })
                    .class("daudio-row");
                })
                .class("daudio-panel");
            },
        )
    }
}
