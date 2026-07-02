//! daudio Pitch2MIDI: monophonic audio→MIDI converter with scale quantization.

pub mod trigger;

use daudio_dsp::notes;
use daudio_dsp::pitch::{Detection, PitchTracker, HOP};
use daudio_sdk::prelude::*;
use daudio_ui::nih_plug_vizia;
use daudio_ui::nih_plug_vizia::widgets::{ParamSlider, RawParamEvent};
use daudio_ui::prelude::*;
use std::sync::atomic::{AtomicI32, Ordering::Relaxed};
use std::sync::Arc;
use std::time::Duration;
use trigger::{NoteAction, Trigger};

/// Root pitch-class, as a host-automatable enum parameter.
#[derive(Enum, PartialEq, Clone, Copy)]
pub enum Root {
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
pub fn root_pc(r: Root) -> u8 {
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
            editor_state: daudio_ui::editor_state(640, 320),
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
        // Clone the shared atomics and the params handle out before the editor
        // closure so the audio thread and the GUI share them (as the filter's
        // meter does). `params` is needed by the preset buttons, which set the
        // twelve degree params by pointer.
        let detected = self.detected.clone();
        let output = self.output.clone();
        let params = self.params.clone();
        daudio_ui::create_editor(
            self.params.editor_state.clone(),
            self.params.clone(),
            move |cx| build_editor(cx, params.clone(), detected.clone(), output.clone()),
        )
    }
}

/// Preset scales as `(label, degrees-relative-to-root)`. The buttons write the
/// twelve `degree_*` bools from these patterns; the root is left untouched.
const PRESETS: &[(&str, &[u8])] = &[
    ("Chromatic", &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]),
    ("Major", &[0, 2, 4, 5, 7, 9, 11]),
    ("Minor", &[0, 2, 3, 5, 7, 8, 10]),
    ("Maj Pent", &[0, 2, 4, 7, 9]),
    ("Min Pent", &[0, 3, 5, 7, 10]),
    ("Blues", &[0, 3, 5, 6, 7, 10]),
    ("Clear", &[]),
];

/// Refresh interval for the note readout (~20 fps). The readout reads plain
/// atomics, so — like the [`daudio_ui::Meter`] — it needs a repaint/refresh
/// driver; here a timer re-reads the atomics into a bound label.
const READOUT_INTERVAL: Duration = Duration::from_millis(50);

/// Write a preset pattern to the twelve degree params via `RawParamEvent`s,
/// which the wrapper's root `ParamModel` turns into host-visible automation.
fn apply_preset(cx: &mut EventContext, params: &PitchToMidiParams, degrees: &[u8]) {
    let toggles: [&BoolParam; 12] = [
        &params.degree_0,
        &params.degree_1,
        &params.degree_2,
        &params.degree_3,
        &params.degree_4,
        &params.degree_5,
        &params.degree_6,
        &params.degree_7,
        &params.degree_8,
        &params.degree_9,
        &params.degree_10,
        &params.degree_11,
    ];
    for (i, p) in toggles.iter().enumerate() {
        let on = degrees.contains(&(i as u8));
        cx.emit(RawParamEvent::BeginSetParameter(p.as_ptr()));
        cx.emit(RawParamEvent::SetParameterNormalized(
            p.as_ptr(),
            if on { 1.0 } else { 0.0 },
        ));
        cx.emit(RawParamEvent::EndSetParameter(p.as_ptr()));
    }
}

/// A note-name readout model: holds the shared `detected`/`output` atomics and a
/// formatted `text` field that a bound [`Label`] renders. A timer emits
/// [`ReadoutEvent::Tick`] to re-read the atomics into `text`.
#[derive(Lens)]
struct Readout {
    detected: Arc<AtomicI32>,
    output: Arc<AtomicI32>,
    text: String,
}

/// Event that drives the [`Readout`] to re-read its atomics.
enum ReadoutEvent {
    Tick,
}

impl Readout {
    /// Format `in: <note>   out: <note>`, rendering `-1` (no note) as an em dash.
    fn format(detected: i32, output: i32) -> String {
        let name = |m: i32| {
            if m < 0 {
                "—".to_string()
            } else {
                notes::note_name(m)
            }
        };
        format!("in: {}   out: {}", name(detected), name(output))
    }
}

impl Model for Readout {
    fn event(&mut self, _cx: &mut EventContext, event: &mut Event) {
        event.map(|e, _| match e {
            ReadoutEvent::Tick => {
                self.text = Readout::format(self.detected.load(Relaxed), self.output.load(Relaxed));
            }
        });
    }
}

/// Build the note-name readout: a [`Readout`] model, a label bound to its text,
/// and a repeating timer that refreshes it from the atomics.
fn build_readout(cx: &mut Context, detected: Arc<AtomicI32>, output: Arc<AtomicI32>) {
    HStack::new(cx, move |cx| {
        Readout {
            text: Readout::format(-1, -1),
            detected,
            output,
        }
        .build(cx);
        Label::new(cx, Readout::text).class("daudio-label");

        let timer = cx.add_timer(READOUT_INTERVAL, None, |cx, action| {
            if let TimerAction::Tick(_) = action {
                cx.emit(ReadoutEvent::Tick);
            }
        });
        cx.start_timer(timer);
    })
    .class("daudio-row");
}

/// Build the full Pitch2MIDI editor tree.
fn build_editor(
    cx: &mut Context,
    params: Arc<PitchToMidiParams>,
    detected: Arc<AtomicI32>,
    output: Arc<AtomicI32>,
) {
    // Lens giving the current root pitch class, used to caption the toggles.
    // NOTE: this maps over the params Arc, whose pointer never changes, so the
    // toggle captions may not relabel *live* when the root moves — a human
    // should confirm relabelling behaviour.
    let root_lens = DaudioData::<PitchToMidiParams>::params.map(|p| root_pc(p.root.value()));

    VStack::new(cx, move |cx| {
        Label::new(cx, "daudio Pitch2MIDI").class("daudio-title");

        // Root selector: ParamSlider works over the `EnumParam<Root>`.
        HStack::new(cx, |cx| {
            Label::new(cx, "Root").class("daudio-label");
            ParamSlider::new(cx, DaudioData::<PitchToMidiParams>::params, |p| &p.root);
        })
        .class("daudio-row");

        // Scale editor: twelve note toggles sharing the root lens.
        HStack::new(cx, move |cx| {
            macro_rules! toggle {
                ($field:ident, $deg:expr) => {
                    NoteToggle::new(
                        cx,
                        DaudioData::<PitchToMidiParams>::params,
                        |p| &p.$field,
                        $deg,
                        root_lens,
                    );
                };
            }
            toggle!(degree_0, 0);
            toggle!(degree_1, 1);
            toggle!(degree_2, 2);
            toggle!(degree_3, 3);
            toggle!(degree_4, 4);
            toggle!(degree_5, 5);
            toggle!(degree_6, 6);
            toggle!(degree_7, 7);
            toggle!(degree_8, 8);
            toggle!(degree_9, 9);
            toggle!(degree_10, 10);
            toggle!(degree_11, 11);
        })
        .class("daudio-row");

        // Preset buttons: each writes its pattern to the degree params.
        HStack::new(cx, move |cx| {
            for &(name, degrees) in PRESETS {
                let params = params.clone();
                Button::new(
                    cx,
                    move |cx| apply_preset(cx, &params, degrees),
                    move |cx| Label::new(cx, name),
                );
            }
        })
        .class("daudio-row");

        // Knobs.
        HStack::new(cx, |cx| {
            ParamControl::new(
                cx,
                "Sensitivity",
                DaudioData::<PitchToMidiParams>::params,
                |p| &p.sensitivity,
            );
            ParamControl::new(cx, "Hold", DaudioData::<PitchToMidiParams>::params, |p| {
                &p.hold
            });
        })
        .class("daudio-row");

        // Note-name readout.
        build_readout(cx, detected, output);
    })
    .class("daudio-panel")
    // Guaranteed background even if the stylesheet fails to apply, so the editor
    // never renders as a bare white window (mirrors the filter editor).
    .background_color(Color::rgb(0x1c, 0x1c, 0x22))
    .width(Percentage(100.0))
    .height(Percentage(100.0));
}
