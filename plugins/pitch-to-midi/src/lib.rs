//! daudio Pitch2MIDI: monophonic audio→MIDI converter with scale quantization.

pub mod trigger;

use daudio_dsp::notes;
use daudio_dsp::pitch::{Detection, PitchTracker, HOP};
use daudio_sdk::prelude::*;
use daudio_ui::nih_plug_vizia;
use std::sync::atomic::{AtomicI32, Ordering::Relaxed};
use std::sync::Arc;
use trigger::{NoteAction, Trigger};

/// Root pitch-class, as a host-automatable enum parameter.
#[derive(Enum, PartialEq, Clone, Copy)]
enum Root {
    C,
    #[name = "C#"]
    Cs,
    D,
    #[name = "D#"]
    Ds,
    E,
    F,
    #[name = "F#"]
    Fs,
    G,
    #[name = "G#"]
    Gs,
    A,
    #[name = "A#"]
    As,
    B,
}

/// Map a [`Root`] to its pitch-class 0..=11.
fn root_pc(r: Root) -> u8 {
    match r {
        Root::C => 0,
        Root::Cs => 1,
        Root::D => 2,
        Root::Ds => 3,
        Root::E => 4,
        Root::F => 5,
        Root::Fs => 6,
        Root::G => 7,
        Root::Gs => 8,
        Root::A => 9,
        Root::As => 10,
        Root::B => 11,
    }
}

#[derive(Params)]
pub struct PitchToMidiParams {
    #[id = "root"]
    root: EnumParam<Root>,
    #[id = "deg0"]
    degree_0: BoolParam,
    #[id = "deg1"]
    degree_1: BoolParam,
    #[id = "deg2"]
    degree_2: BoolParam,
    #[id = "deg3"]
    degree_3: BoolParam,
    #[id = "deg4"]
    degree_4: BoolParam,
    #[id = "deg5"]
    degree_5: BoolParam,
    #[id = "deg6"]
    degree_6: BoolParam,
    #[id = "deg7"]
    degree_7: BoolParam,
    #[id = "deg8"]
    degree_8: BoolParam,
    #[id = "deg9"]
    degree_9: BoolParam,
    #[id = "deg10"]
    degree_10: BoolParam,
    #[id = "deg11"]
    degree_11: BoolParam,
    #[id = "sens"]
    sensitivity: FloatParam,
    #[id = "hold"]
    hold: FloatParam,
    #[persist = "editor-state"]
    editor_state: Arc<nih_plug_vizia::ViziaState>,
}

impl Default for PitchToMidiParams {
    fn default() -> Self {
        // Major scale: degrees 0,2,4,5,7,9,11.
        Self {
            root: EnumParam::new("Root", Root::C),
            degree_0: BoolParam::new("Degree 0", true),
            degree_1: BoolParam::new("Degree 1", false),
            degree_2: BoolParam::new("Degree 2", true),
            degree_3: BoolParam::new("Degree 3", false),
            degree_4: BoolParam::new("Degree 4", true),
            degree_5: BoolParam::new("Degree 5", true),
            degree_6: BoolParam::new("Degree 6", false),
            degree_7: BoolParam::new("Degree 7", true),
            degree_8: BoolParam::new("Degree 8", false),
            degree_9: BoolParam::new("Degree 9", true),
            degree_10: BoolParam::new("Degree 10", false),
            degree_11: BoolParam::new("Degree 11", true),
            sensitivity: FloatParam::new(
                "Sensitivity",
                -40.0,
                FloatRange::Linear {
                    min: -60.0,
                    max: 0.0,
                },
            )
            .with_unit(" dB"),
            hold: FloatParam::new(
                "Hold",
                40.0,
                FloatRange::Linear {
                    min: 10.0,
                    max: 200.0,
                },
            )
            .with_unit(" ms"),
            editor_state: daudio_ui::editor_state(640, 220),
        }
    }
}

impl PitchToMidiParams {
    /// Build the 12-bit scale mask from the individual degree toggles.
    fn degree_mask(&self) -> u16 {
        let bits = [
            self.degree_0.value(),
            self.degree_1.value(),
            self.degree_2.value(),
            self.degree_3.value(),
            self.degree_4.value(),
            self.degree_5.value(),
            self.degree_6.value(),
            self.degree_7.value(),
            self.degree_8.value(),
            self.degree_9.value(),
            self.degree_10.value(),
            self.degree_11.value(),
        ];
        let mut mask = 0u16;
        for (i, on) in bits.iter().enumerate() {
            if *on {
                mask |= 1 << i;
            }
        }
        mask
    }

    fn root_pc(&self) -> u8 {
        root_pc(self.root.value())
    }
}

#[daudio_plugin(
    name = "daudio Pitch2MIDI",
    vendor = "daudio",
    url = "https://example.com",
    email = "hexadecifish@gmail.com",
    clap_id = "com.daudio.pitch2midi",
    clap_description = "Monophonic pitch to MIDI with scale quantization",
    vst3_id = "daudioPitch2Midi",
    clap_features = [AudioEffect, Analyzer, Utility],
    vst3_categories = [Fx, Analyzer],
    midi_out = true
)]
pub struct PitchToMidi {
    params: Arc<PitchToMidiParams>,
    tracker: PitchTracker,
    trigger: Trigger,
    level: f32,
    level_decay: f32,
    sample_rate: f32,
    detected: Arc<AtomicI32>,
    output: Arc<AtomicI32>,
}

impl Default for PitchToMidi {
    fn default() -> Self {
        Self {
            params: Arc::new(PitchToMidiParams::default()),
            tracker: PitchTracker::new(),
            trigger: Trigger::new(),
            level: 0.0,
            level_decay: 0.0,
            sample_rate: 44_100.0,
            detected: Arc::new(AtomicI32::new(-1)),
            output: Arc::new(AtomicI32::new(-1)),
        }
    }
}

impl DaudioAudioToMidi for PitchToMidi {
    type Params = PitchToMidiParams;

    fn activate(&mut self, sample_rate: f32) {
        self.tracker.set_sample_rate(sample_rate);
        self.sample_rate = sample_rate;
        self.level_decay = (-1.0 / (0.05 * sample_rate)).exp();
        self.level = 0.0;
    }

    fn reset(&mut self) {
        self.tracker.reset();
        self.trigger.reset();
        self.level = 0.0;
    }

    fn process_sample(&mut self, input: f32, timing: u32, emit: &mut dyn FnMut(NoteEvent<()>)) {
        self.level = input.abs().max(self.level * self.level_decay);
        if let Some(detection) = self.tracker.push(input) {
            self.trigger
                .set_hold(self.params.hold.value(), HOP as f32 / self.sample_rate);
            let threshold = daudio_dsp::gain::db_to_gain(self.params.sensitivity.value());
            let gated = self.level >= threshold;
            let target = match detection {
                Detection::Pitch(f) if gated => {
                    let midi = notes::freq_to_midi(f);
                    self.detected.store(midi, Relaxed);
                    notes::quantize(midi, self.params.root_pc(), self.params.degree_mask())
                }
                _ => {
                    self.detected.store(-1, Relaxed);
                    None
                }
            };
            let velocity = self.level.clamp(0.0, 1.0);
            self.output.store(target.unwrap_or(-1), Relaxed);
            self.trigger
                .on_hop(target, velocity, &mut |action| match action {
                    NoteAction::On { note, velocity } => emit(NoteEvent::NoteOn {
                        timing,
                        voice_id: None,
                        channel: 0,
                        note,
                        velocity,
                    }),
                    NoteAction::Off { note } => emit(NoteEvent::NoteOff {
                        timing,
                        voice_id: None,
                        channel: 0,
                        note,
                        velocity: 0.0,
                    }),
                });
        }
    }

    fn editor(&mut self) -> Option<Box<dyn Editor>> {
        None
    }
}
