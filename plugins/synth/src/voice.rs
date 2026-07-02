//! `SynthVoice`: one monophonic subtractive voice (osc -> lowpass -> amp env).

use daudio_dsp::adsr::Adsr;
use daudio_dsp::biquad::BiquadLowpass;
use daudio_dsp::oscillator::{Oscillator, Waveform};
use daudio_dsp::processor::Processor;
use daudio_sdk::Voice;

/// How far the envelope can push the filter cutoff, as a multiple of the base
/// cutoff at full `env_amount` and a fully-open envelope. Musical, not exact.
const ENV_CUTOFF_RANGE: f32 = 6.0;

/// Highest cutoff we ever ask the biquad for, to stay clear of Nyquist.
const MAX_CUTOFF_HZ: f32 = 18_000.0;

/// Snapshot of the per-voice configuration, refreshed each block from the
/// params and pushed into voices. Applied to a voice both in `pre_block` (for
/// live edits on sounding voices) and at note-on (so a fresh or stolen voice is
/// correctly configured on its very first sample).
#[derive(Clone, Copy)]
pub struct VoiceConfig {
    pub waveform: Waveform,
    pub cutoff: f32,
    pub resonance: f32,
    pub env_amount: f32,
    pub attack: f32,
    pub decay: f32,
    pub sustain: f32,
    pub release: f32,
}

/// One subtractive voice: saw/sine oscillator into an RBJ lowpass, shaped by an
/// amplitude ADSR that also modulates the filter cutoff.
pub struct SynthVoice {
    osc: Oscillator,
    filter: BiquadLowpass,
    adsr: Adsr,

    // Current per-voice configuration (pushed in by the synth's `pre_block`).
    base_cutoff_hz: f32,
    resonance: f32,
    env_amount: f32,

    note: u8,
    sample_rate: f32,
}

impl Default for SynthVoice {
    fn default() -> Self {
        Self {
            osc: Oscillator::new(Waveform::Saw),
            filter: BiquadLowpass::new(1_000.0, 0.707),
            adsr: Adsr::new(),
            base_cutoff_hz: 1_000.0,
            resonance: 0.707,
            env_amount: 0.5,
            note: 0,
            sample_rate: 48_000.0,
        }
    }
}

impl SynthVoice {
    /// Apply a full configuration snapshot. Single entry point used by both the
    /// synth's `pre_block` (live edits on sounding voices) and note-on (so a
    /// fresh/stolen voice is correct on its very first sample).
    pub fn apply_config(&mut self, cfg: &VoiceConfig) {
        self.set_waveform(cfg.waveform);
        self.set_filter(cfg.cutoff, cfg.resonance);
        self.set_env_amount(cfg.env_amount);
        self.set_adsr(cfg.attack, cfg.decay, cfg.sustain, cfg.release);
    }

    /// Select the oscillator waveform.
    fn set_waveform(&mut self, waveform: Waveform) {
        self.osc.set_waveform(waveform);
    }

    /// Set base cutoff (Hz) and resonance (Q). Q is only fixable at
    /// construction, so a resonance change rebuilds the biquad; its state is
    /// harmlessly cleared (voices reset their filter on note-on anyway).
    fn set_filter(&mut self, cutoff_hz: f32, resonance: f32) {
        self.base_cutoff_hz = cutoff_hz;
        if (resonance - self.resonance).abs() > f32::EPSILON {
            self.resonance = resonance;
            self.filter = BiquadLowpass::new(cutoff_hz, resonance);
            self.filter.set_sample_rate(self.sample_rate);
        }
    }

    /// How strongly the envelope opens the filter (0..1).
    fn set_env_amount(&mut self, env_amount: f32) {
        self.env_amount = env_amount;
    }

    /// Set the amplitude envelope times (seconds) and sustain (0..1).
    fn set_adsr(&mut self, attack: f32, decay: f32, sustain: f32, release: f32) {
        self.adsr.set_params(attack, decay, sustain, release);
    }
}

impl Voice for SynthVoice {
    fn set_sample_rate(&mut self, sr: f32) {
        self.sample_rate = sr;
        self.osc.set_sample_rate(sr);
        self.filter.set_sample_rate(sr);
        self.adsr.set_sample_rate(sr);
    }

    fn note_on(&mut self, note: u8, _velocity: f32) {
        let freq = 440.0 * 2f32.powf((note as f32 - 69.0) / 12.0);
        self.osc.set_frequency(freq);
        self.note = note;
        self.filter.reset();
        self.adsr.trigger();
    }

    fn note_off(&mut self) {
        self.adsr.release();
    }

    fn is_active(&self) -> bool {
        self.adsr.is_active()
    }

    fn note(&self) -> u8 {
        self.note
    }

    fn render(&mut self) -> f32 {
        if !self.is_active() {
            return 0.0;
        }
        let raw = self.osc.next_sample();
        let env = self.adsr.next_sample();
        let cutoff = (self.base_cutoff_hz * (1.0 + env * self.env_amount * ENV_CUTOFF_RANGE))
            .min(MAX_CUTOFF_HZ);
        self.filter.set_cutoff(cutoff);
        let filtered = self.filter.process_sample(raw);
        filtered * env
    }
}
