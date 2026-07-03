//! daudio Pitch2MIDI: monophonic audio→MIDI converter with scale quantization.

pub mod trigger;

use daudio_dsp::notes;
use daudio_dsp::pitch::{Detection, PitchTracker};
use daudio_sdk::prelude::*;
use daudio_ui::nih_plug_vizia;
use daudio_ui::nih_plug_vizia::assets::fonts;
use daudio_ui::nih_plug_vizia::vizia::vg;
use daudio_ui::nih_plug_vizia::widgets::param_base::ParamWidgetBase;
use daudio_ui::nih_plug_vizia::widgets::{ParamSlider, RawParamEvent};
use daudio_ui::prelude::*;
use std::cell::Cell;
use std::sync::atomic::{AtomicI32, Ordering::Relaxed};
use std::sync::Arc;
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

/// Detection window/hop trade-off: latency vs how low a note it can track.
#[derive(Enum, PartialEq, Clone, Copy)]
pub enum Response {
    Fast,     // 512 / 64  — lowest latency, lead/voice only
    Balanced, // 1024 / 128 — default
    Full,     // 2048 / 256 — reaches lower notes, more latency
}

impl Response {
    /// (window, hop) for the detector.
    pub fn window_hop(self) -> (usize, usize) {
        match self {
            Response::Fast => (512, 64),
            Response::Balanced => (1024, 128),
            Response::Full => (2048, 256),
        }
    }
}

/// How pitch-bend output behaves.
#[derive(Enum, PartialEq, Clone, Copy)]
pub enum BendMode {
    /// Never bend; every quantized note change retriggers.
    Off,
    /// Emit pitch-bend within the held note, but note changes still retrigger.
    On,
    /// Volume-gated: a quantized note change *bends* (no retrigger) when the
    /// volume stayed continuous and the new note is within the bend range;
    /// a volume dip (re-articulation) is a separate note.
    Auto,
}

/// True when a quantized-note change should be treated as a bend rather than a
/// new note: the volume stayed continuous (no dip) AND the new note is within the
/// bend range of the held note. Encodes "volume should not decrease during a bend".
fn is_bend(dipped: bool, held: u8, target: i32, bend_range: i32) -> bool {
    !dipped && (target - held as i32).abs() <= bend_range
}

/// A hop-to-hop amplitude below `note_peak * DIP_RATIO` counts as a volume dip
/// (re-articulation), i.e. a separate note rather than a bend. −6 dB.
const DIP_RATIO: f32 = 0.5;

/// Velocity response shaping. `curve` 0.5 = linear; below eases in (slow start,
/// quiet stays quiet), above eases out (fast start, quiet lifts sooner).
/// Endpoints are preserved (0→0, 1→1) for every curve.
fn velocity_curve(t: f32, curve: f32) -> f32 {
    let gamma = 2f32.powf((0.5 - curve) * 3.0); // 0.5 -> 1.0, 0 -> ~2.8, 1 -> ~0.35
    t.clamp(0.0, 1.0).powf(gamma)
}

/// Map a linear input level to a MIDI velocity fraction (0..1) over an input dB
/// window [`floor_db`, `ceil_db`] and an output range [`min_v`, `max_v`] (already
/// normalized as velocity/127), shaped by `curve`. Encodes "velocity based on an
/// input volume range".
fn map_velocity(
    level: f32,
    floor_db: f32,
    ceil_db: f32,
    min_v: f32,
    max_v: f32,
    curve: f32,
) -> f32 {
    let db = daudio_dsp::gain::gain_to_db(level.max(1e-6));
    let span = (ceil_db - floor_db).max(1e-3);
    let t = ((db - floor_db) / span).clamp(0.0, 1.0);
    (min_v + velocity_curve(t, curve) * (max_v - min_v)).clamp(0.0, 1.0)
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
    #[id = "response"]
    response: EnumParam<Response>,
    #[id = "sens"]
    sensitivity: FloatParam,
    #[id = "confidence"]
    confidence: FloatParam,
    #[id = "hold"]
    hold: FloatParam,
    #[id = "maxjump"]
    max_jump: IntParam,
    #[id = "bendmode"]
    bend_mode: EnumParam<BendMode>,
    #[id = "bendrange"]
    bend_range: IntParam,
    #[id = "velfloor"]
    vel_floor: FloatParam,
    #[id = "velceil"]
    vel_ceil: FloatParam,
    #[id = "velmin"]
    vel_min: IntParam,
    #[id = "velmax"]
    vel_max: IntParam,
    #[id = "velcurve"]
    vel_curve: FloatParam,
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
            response: EnumParam::new("Response", Response::Balanced),
            sensitivity: FloatParam::new(
                "Sensitivity",
                -40.0,
                FloatRange::Linear {
                    min: -60.0,
                    max: 0.0,
                },
            )
            .with_unit(" dB"),
            confidence: FloatParam::new(
                "Confidence",
                0.6,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            ),
            hold: FloatParam::new(
                "Hold",
                25.0,
                FloatRange::Linear {
                    min: 10.0,
                    max: 200.0,
                },
            )
            .with_unit(" ms"),
            max_jump: IntParam::new("Max Jump", 7, IntRange::Linear { min: 1, max: 12 })
                .with_unit(" st"),
            bend_mode: EnumParam::new("Bend Mode", BendMode::Off),
            bend_range: IntParam::new("Bend Range", 2, IntRange::Linear { min: 1, max: 12 })
                .with_unit(" st"),
            vel_floor: FloatParam::new(
                "Vel Floor",
                -40.0,
                FloatRange::Linear {
                    min: -80.0,
                    max: 0.0,
                },
            )
            .with_unit(" dB"),
            vel_ceil: FloatParam::new(
                "Vel Ceil",
                -6.0,
                FloatRange::Linear {
                    min: -60.0,
                    max: 0.0,
                },
            )
            .with_unit(" dB"),
            vel_min: IntParam::new("Vel Min", 1, IntRange::Linear { min: 1, max: 127 }),
            vel_max: IntParam::new("Vel Max", 127, IntRange::Linear { min: 1, max: 127 }),
            vel_curve: FloatParam::new("Curve", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 }),
            editor_state: daudio_ui::editor_state(640, 680),
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
    /// The MIDI note currently sounding (mirrors the trigger), used as the
    /// reference pitch for optional pitch-bend output.
    active_note: Option<u8>,
    /// Last pitch-bend value emitted, to avoid resending unchanged bends.
    last_bend: f32,
    /// Max `|input|` seen in the current detection hop (reset each hop).
    hop_peak: f32,
    /// Max hop amplitude since the current note began (for dip detection).
    note_peak: f32,
    /// Whether the volume dipped below `note_peak * DIP_RATIO` during the held
    /// note — marks a re-articulation, so the next pitch change is a new note.
    dipped: bool,
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
            active_note: None,
            last_bend: 0.5,
            hop_peak: 0.0,
            note_peak: 0.0,
            dipped: false,
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
        self.active_note = None;
        self.last_bend = 0.5;
        self.hop_peak = 0.0;
        self.note_peak = 0.0;
        self.dipped = false;
    }

    fn reset(&mut self) {
        self.tracker.reset();
        self.trigger.reset();
        self.level = 0.0;
        self.active_note = None;
        self.last_bend = 0.5;
        self.hop_peak = 0.0;
        self.note_peak = 0.0;
        self.dipped = false;
    }

    fn process_sample(&mut self, input: f32, timing: u32, emit: &mut dyn FnMut(NoteEvent<()>)) {
        self.level = input.abs().max(self.level * self.level_decay);
        // Fast per-hop amplitude for volume-dip detection (Auto bend).
        self.hop_peak = self.hop_peak.max(input.abs());
        if let Some(detection) = self.tracker.push(input) {
            let (window, hop) = self.params.response.value().window_hop();
            let confidence = self.params.confidence.value();
            // Sensitivity governs both the trigger level gate (below) and the
            // detector's power gate — map its -60..0 dB range to a modest
            // 0.02..0.30 energy gate.
            let power =
                0.02 + 0.28 * ((self.params.sensitivity.value() + 60.0) / 60.0).clamp(0.0, 1.0);
            self.tracker.set_config(window, hop, power, confidence);
            self.trigger.set_fast_clarity((confidence + 0.2).min(0.98));
            self.trigger.set_max_jump(self.params.max_jump.value());
            self.trigger
                .set_hold(self.params.hold.value(), hop as f32 / self.sample_rate);
            let threshold = daudio_dsp::gain::db_to_gain(self.params.sensitivity.value());
            let gated = self.level >= threshold;
            let (target, clarity) = match detection {
                Detection::Pitch { freq, clarity } if gated => {
                    let midi = notes::freq_to_midi(freq);
                    self.detected.store(midi, Relaxed);
                    (
                        notes::quantize(midi, self.params.root_pc(), self.params.degree_mask()),
                        clarity,
                    )
                }
                _ => {
                    self.detected.store(-1, Relaxed);
                    (None, 0.0)
                }
            };
            // Velocity shaped from the input level over the user's dB window.
            let velocity = map_velocity(
                self.level,
                self.params.vel_floor.value(),
                self.params.vel_ceil.value(),
                self.params.vel_min.value() as f32 / 127.0,
                self.params.vel_max.value() as f32 / 127.0,
                self.params.vel_curve.value(),
            );
            self.output.store(target.unwrap_or(-1), Relaxed);

            // Volume-dip tracking for Auto bend. `amp` is this hop's peak level;
            // a drop below `note_peak * DIP_RATIO` since the note began marks a
            // re-articulation (a separate note, not a bend).
            let amp = self.hop_peak;
            self.hop_peak = 0.0;
            let mut target_for_trigger = target;
            if let Some(held) = self.active_note {
                self.note_peak = self.note_peak.max(amp);
                if amp < self.note_peak * DIP_RATIO {
                    self.dipped = true;
                }
                // Auto: a quantized change with continuous volume and within the
                // bend range is a bend — feed the trigger the *held* note so it
                // does not retrigger (the pitch-bend below expresses the move).
                if self.params.bend_mode.value() == BendMode::Auto {
                    if let Some(t) = target {
                        if is_bend(self.dipped, held, t, self.params.bend_range.value()) {
                            target_for_trigger = Some(held as i32);
                        }
                    }
                }
            }

            // Track the sounding note out of the closure: `on_hop` borrows
            // `self.trigger` mutably, so the closure captures a local mirror
            // instead of `self.active_note`, which we write back afterwards.
            let mut new_active = self.active_note;
            let mut committed = false;
            self.trigger.on_hop(
                target_for_trigger,
                clarity,
                velocity,
                &mut |action| match action {
                    NoteAction::On { note, velocity } => {
                        new_active = Some(note);
                        committed = true;
                        emit(NoteEvent::NoteOn {
                            timing,
                            voice_id: None,
                            channel: 0,
                            note,
                            velocity,
                        });
                    }
                    NoteAction::Off { note } => {
                        new_active = None;
                        emit(NoteEvent::NoteOff {
                            timing,
                            voice_id: None,
                            channel: 0,
                            note,
                            velocity: 0.0,
                        });
                    }
                },
            );
            self.active_note = new_active;
            // A fresh note-on starts the dip envelope clean for the new note.
            if committed {
                self.note_peak = amp;
                self.dipped = false;
            }

            // Pitch bend (On or Auto): track the detected pitch relative to the
            // held note over the bend range, resending only on change. In Auto
            // this expresses the slide that was kept as a bend above.
            if self.params.bend_mode.value() != BendMode::Off {
                if let (Some(note), Detection::Pitch { freq: f, .. }) =
                    (self.active_note, detection)
                {
                    let value = daudio_dsp::notes::bend_value(
                        f,
                        note,
                        self.params.bend_range.value() as f32,
                    );
                    if (value - self.last_bend).abs() > 1e-4 {
                        emit(NoteEvent::MidiPitchBend {
                            timing,
                            channel: 0,
                            value,
                        });
                        self.last_bend = value;
                    }
                }
            }
            // Recenter the bend once when no note is held.
            if self.active_note.is_none() && (self.last_bend - 0.5).abs() > 1e-4 {
                emit(NoteEvent::MidiPitchBend {
                    timing,
                    channel: 0,
                    value: 0.5,
                });
                self.last_bend = 0.5;
            }
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

/// A leaf view that renders the current input/output note names.
///
/// Modelled on [`daudio_ui::Meter`]: it holds shared atomics written by the
/// audio thread and reads them directly in [`View::draw`] every frame. Under
/// `nih_plug_vizia`'s `vizia_baseview` backend the editor redraws every frame
/// unconditionally (and vizia timers never tick there), so a per-frame `draw`
/// is the reliable way to reflect audio-thread state — no timer/model needed.
struct NoteReadout {
    detected: Arc<AtomicI32>,
    output: Arc<AtomicI32>,
    /// femtovg font id, added to the canvas lazily on first draw and cached
    /// (adding it every frame would leak duplicate fonts).
    font: Cell<Option<vg::FontId>>,
}

impl NoteReadout {
    /// Build a readout reading from the shared `detected`/`output` atomics.
    fn new(cx: &mut Context, detected: Arc<AtomicI32>, output: Arc<AtomicI32>) -> Handle<'_, Self> {
        Self {
            detected,
            output,
            font: Cell::new(None),
        }
        .build(cx, |_| {})
        // Default inline size so the text has room without a stylesheet.
        .width(Pixels(200.0))
        .height(Pixels(24.0))
    }

    /// Note name for a MIDI value, rendering `< 0` (no note) as an em dash.
    fn name(midi: i32) -> String {
        if midi < 0 {
            "—".to_string()
        } else {
            notes::note_name(midi)
        }
    }
}

impl View for NoteReadout {
    fn element(&self) -> Option<&'static str> {
        Some("daudio-note-readout")
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &mut Canvas) {
        let b = cx.bounds();
        if b.w == 0.0 || b.h == 0.0 {
            return;
        }

        // Lazily register the font on the femtovg canvas and cache its id.
        let font = match self.font.get() {
            Some(f) => f,
            None => match canvas.add_font_mem(fonts::NOTO_SANS_REGULAR) {
                Ok(f) => {
                    self.font.set(Some(f));
                    f
                }
                Err(_) => return,
            },
        };

        let text = format!(
            "in: {}   out: {}",
            Self::name(self.detected.load(Relaxed)),
            Self::name(self.output.load(Relaxed)),
        );

        let mut paint = vg::Paint::color(vg::Color::rgb(0xf0, 0xf0, 0xf4));
        paint.set_font(&[font]);
        paint.set_font_size(15.0);
        paint.set_text_align(vg::Align::Center);
        paint.set_text_baseline(vg::Baseline::Middle);
        let _ = canvas.fill_text(b.x + b.w / 2.0, b.y + b.h / 2.0, &text, &paint);
    }
}

/// Build the full Pitch2MIDI editor tree.
fn build_editor(
    cx: &mut Context,
    params: Arc<PitchToMidiParams>,
    detected: Arc<AtomicI32>,
    output: Arc<AtomicI32>,
) {
    // Lens giving the current root pitch class, used to caption the toggles.
    // Built via `ParamWidgetBase::make_lens` so nih_plug refreshes it on
    // `RawParamEvent::ParametersChanged` — the same mechanism that keeps a
    // `ParamSlider`'s value display live. (A plain `.map` over the params `Arc`
    // never re-fires, since the Arc pointer is stable.)
    let root_lens = ParamWidgetBase::make_lens(
        DaudioData::<PitchToMidiParams>::params,
        |p| &p.root,
        |root| root_pc(root.value()),
    );

    VStack::new(cx, move |cx| {
        Label::new(cx, "daudio Pitch2MIDI").class("daudio-title");

        // Scale card: root selector, the twelve note toggles, and the presets.
        card_column(cx, "SCALE", move |cx| {
            // Root selector: ParamSlider works over the `EnumParam<Root>`.
            HStack::new(cx, |cx| {
                Label::new(cx, "Root")
                    .class("daudio-label")
                    .child_top(Stretch(1.0))
                    .child_bottom(Stretch(1.0));
                ParamSlider::new(cx, DaudioData::<PitchToMidiParams>::params, |p| &p.root)
                    .width(Pixels(130.0));
            })
            .height(Auto)
            .width(Auto)
            .col_between(Pixels(10.0));

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
            .height(Auto)
            .width(Auto)
            .col_between(Pixels(5.0));

            // Preset buttons: each writes its pattern to the degree params.
            HStack::new(cx, move |cx| {
                for &(name, degrees) in PRESETS {
                    let params = params.clone();
                    Button::new(
                        cx,
                        move |cx| apply_preset(cx, &params, degrees),
                        move |cx| Label::new(cx, name),
                    )
                    .class("daudio-preset");
                }
            })
            .height(Auto)
            .width(Auto)
            .col_between(Pixels(6.0));
        });

        // Detection + readout side by side.
        HStack::new(cx, move |cx| {
            card_column(cx, "DETECTION", |cx| {
                HStack::new(cx, |cx| {
                    // Response selector (ParamSlider over the enum, like Root).
                    VStack::new(cx, |cx| {
                        Label::new(cx, "Response").class("daudio-label");
                        ParamSlider::new(cx, DaudioData::<PitchToMidiParams>::params, |p| {
                            &p.response
                        })
                        .width(Pixels(120.0));
                    })
                    .height(Auto)
                    .width(Auto)
                    .row_between(Pixels(6.0))
                    .child_left(Stretch(1.0))
                    .child_right(Stretch(1.0));
                    ParamControl::new(
                        cx,
                        "Sensitivity",
                        DaudioData::<PitchToMidiParams>::params,
                        |p| &p.sensitivity,
                    );
                    ParamControl::new(
                        cx,
                        "Confidence",
                        DaudioData::<PitchToMidiParams>::params,
                        |p| &p.confidence,
                    );
                    ParamControl::new(cx, "Hold", DaudioData::<PitchToMidiParams>::params, |p| {
                        &p.hold
                    });
                })
                .height(Auto)
                .width(Auto)
                .col_between(Pixels(14.0));
                HStack::new(cx, |cx| {
                    ParamControl::new(
                        cx,
                        "Max Jump",
                        DaudioData::<PitchToMidiParams>::params,
                        |p| &p.max_jump,
                    );
                    VStack::new(cx, |cx| {
                        Label::new(cx, "Bend Mode").class("daudio-label");
                        ParamSlider::new(cx, DaudioData::<PitchToMidiParams>::params, |p| {
                            &p.bend_mode
                        })
                        .width(Pixels(120.0));
                    })
                    .height(Auto)
                    .width(Auto)
                    .row_between(Pixels(6.0))
                    .child_left(Stretch(1.0))
                    .child_right(Stretch(1.0));
                    ParamControl::new(
                        cx,
                        "Bend Range",
                        DaudioData::<PitchToMidiParams>::params,
                        |p| &p.bend_range,
                    );
                })
                .height(Auto)
                .width(Auto)
                .col_between(Pixels(14.0))
                .child_top(Pixels(6.0));
            });
            // Readout: a draw()-based leaf reading the shared atomics every frame
            // (baseview redraws each frame; vizia timers are inert).
            card(cx, "DETECTED → OUT", move |cx| {
                NoteReadout::new(cx, detected, output);
            });
        })
        .height(Auto)
        .width(Auto)
        .col_between(Pixels(14.0));

        // Output note shaping: velocity mapped from the input volume range.
        card_column(cx, "DYNAMICS", |cx| {
            HStack::new(cx, |cx| {
                ParamControl::new(
                    cx,
                    "Vel Floor",
                    DaudioData::<PitchToMidiParams>::params,
                    |p| &p.vel_floor,
                );
                ParamControl::new(
                    cx,
                    "Vel Ceil",
                    DaudioData::<PitchToMidiParams>::params,
                    |p| &p.vel_ceil,
                );
                ParamControl::new(
                    cx,
                    "Vel Min",
                    DaudioData::<PitchToMidiParams>::params,
                    |p| &p.vel_min,
                );
                ParamControl::new(
                    cx,
                    "Vel Max",
                    DaudioData::<PitchToMidiParams>::params,
                    |p| &p.vel_max,
                );
                ParamControl::new(cx, "Curve", DaudioData::<PitchToMidiParams>::params, |p| {
                    &p.vel_curve
                });
            })
            .height(Auto)
            .width(Auto)
            .col_between(Pixels(14.0));
        });
    })
    .class("daudio-panel")
    // Guaranteed background even if the stylesheet fails to apply, so the editor
    // never renders as a bare white window (matches the theme `BG` #16161c).
    .background_color(Color::rgb(0x16, 0x16, 0x1c))
    .child_space(Pixels(18.0))
    .row_between(Pixels(14.0))
    .width(Percentage(100.0))
    .height(Percentage(100.0));
}

#[cfg(test)]
mod tests {
    use super::{is_bend, map_velocity, velocity_curve};

    fn db_gain(db: f32) -> f32 {
        10f32.powf(db / 20.0)
    }

    #[test]
    fn velocity_maps_input_range_to_output_range() {
        // At the ceiling → max; at the floor → min (linear curve).
        let at_ceil = map_velocity(db_gain(-6.0), -40.0, -6.0, 0.2, 1.0, 0.5);
        let at_floor = map_velocity(db_gain(-40.0), -40.0, -6.0, 0.2, 1.0, 0.5);
        assert!((at_ceil - 1.0).abs() < 1e-3, "ceil -> {at_ceil}");
        assert!((at_floor - 0.2).abs() < 1e-3, "floor -> {at_floor}");
    }

    #[test]
    fn velocity_clamps_outside_the_window() {
        // Louder than ceil clamps to max; quieter than floor clamps to min.
        assert!((map_velocity(db_gain(0.0), -40.0, -6.0, 0.2, 1.0, 0.5) - 1.0).abs() < 1e-3);
        assert!((map_velocity(db_gain(-80.0), -40.0, -6.0, 0.2, 1.0, 0.5) - 0.2).abs() < 1e-3);
    }

    #[test]
    fn equal_min_max_is_fixed_velocity() {
        // Vel Min == Vel Max → constant regardless of level.
        for db in [-60.0, -30.0, -6.0, 0.0] {
            let v = map_velocity(db_gain(db), -40.0, -6.0, 0.75, 0.75, 0.5);
            assert!((v - 0.75).abs() < 1e-3, "db {db} -> {v}");
        }
    }

    #[test]
    fn linear_curve_is_identity_and_endpoints_preserved() {
        assert!((velocity_curve(0.5, 0.5) - 0.5).abs() < 1e-6);
        for curve in [0.0, 0.25, 0.5, 0.75, 1.0] {
            assert!(velocity_curve(0.0, curve).abs() < 1e-6);
            assert!((velocity_curve(1.0, curve) - 1.0).abs() < 1e-6);
        }
    }

    #[test]
    fn continuous_volume_small_step_is_a_bend() {
        // No dip, new note within the bend range → treat as a bend.
        assert!(is_bend(false, 60, 62, 2)); // whole step up, range 2
        assert!(is_bend(false, 60, 58, 2)); // whole step down
    }

    #[test]
    fn volume_dip_is_a_separate_note() {
        // A re-articulation (dip) is never a bend, even for a small step.
        assert!(!is_bend(true, 60, 62, 2));
    }

    #[test]
    fn jump_beyond_range_is_a_separate_note() {
        // Too far to bend even with continuous volume.
        assert!(!is_bend(false, 60, 67, 2)); // 7 semis, range 2
        assert!(is_bend(false, 60, 67, 7)); // but bendable with a wide range
    }
}
