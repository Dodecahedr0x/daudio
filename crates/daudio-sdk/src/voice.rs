//! Polyphony layer: the [`Voice`] trait and a [`VoiceManager`] that allocates
//! and steals voices. Pure logic — no host or DSP dependencies.

/// A single monophonic voice. Implementors own their own oscillators,
/// envelopes and state.
pub trait Voice: Default {
    fn set_sample_rate(&mut self, sr: f32);
    fn note_on(&mut self, note: u8, velocity: f32);
    fn note_off(&mut self);
    fn is_active(&self) -> bool;
    fn note(&self) -> u8;
    fn render(&mut self) -> f32;
}

/// Fixed-size pool of voices with oldest-voice stealing.
pub struct VoiceManager<V: Voice> {
    voices: Vec<V>,
    ages: Vec<u64>,
    counter: u64,
    sample_rate: f32,
}

impl<V: Voice> VoiceManager<V> {
    /// Create a pool of `max_voices` default voices.
    pub fn new(max_voices: usize) -> Self {
        Self {
            voices: (0..max_voices).map(|_| V::default()).collect(),
            ages: vec![0; max_voices],
            counter: 0,
            sample_rate: 48_000.0,
        }
    }

    /// Fan the sample rate out to every voice.
    pub fn set_sample_rate(&mut self, sr: f32) {
        self.sample_rate = sr;
        for v in &mut self.voices {
            v.set_sample_rate(sr);
        }
    }

    /// Allocate a voice for `note`, reusing a free voice if one exists,
    /// otherwise stealing the oldest.
    pub fn note_on(&mut self, note: u8, velocity: f32) {
        self.note_on_with(note, velocity, |_| {});
    }

    /// Allocate/steal a voice, configure it, then trigger it. The closure runs
    /// on the chosen voice BEFORE `note_on`, so a freshly allocated or stolen
    /// voice starts with correct configuration on its very first sample.
    pub fn note_on_with(&mut self, note: u8, velocity: f32, configure: impl FnOnce(&mut V)) {
        let target = self
            .voices
            .iter()
            .position(|v| !v.is_active())
            .unwrap_or_else(|| {
                // Steal the oldest (smallest age).
                self.ages
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, age)| **age)
                    .map(|(i, _)| i)
                    .unwrap_or(0)
            });
        self.counter += 1;
        self.ages[target] = self.counter;
        configure(&mut self.voices[target]);
        self.voices[target].note_on(note, velocity);
    }

    /// Release the most recently triggered active voice matching `note`.
    pub fn note_off(&mut self, note: u8) {
        let target = self
            .voices
            .iter()
            .enumerate()
            .filter(|(_, v)| v.is_active() && v.note() == note)
            .max_by_key(|(i, _)| self.ages[*i])
            .map(|(i, _)| i);
        if let Some(i) = target {
            self.voices[i].note_off();
        }
    }

    /// Sum the output of all active voices for one sample.
    pub fn render(&mut self) -> f32 {
        self.voices
            .iter_mut()
            .filter(|v| v.is_active())
            .map(|v| v.render())
            .sum()
    }

    /// Reset every voice to its default state, preserving the sample rate.
    pub fn reset(&mut self) {
        for v in &mut self.voices {
            *v = V::default();
            v.set_sample_rate(self.sample_rate);
        }
        for age in &mut self.ages {
            *age = 0;
        }
        self.counter = 0;
    }

    /// Apply `f` to each active voice.
    pub fn for_each_active(&mut self, mut f: impl FnMut(&mut V)) {
        for v in self.voices.iter_mut().filter(|v| v.is_active()) {
            f(v);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct TestVoice {
        active: bool,
        note: u8,
        sr: f32,
    }

    impl Voice for TestVoice {
        fn set_sample_rate(&mut self, sr: f32) {
            self.sr = sr;
        }
        fn note_on(&mut self, note: u8, _velocity: f32) {
            self.active = true;
            self.note = note;
        }
        fn note_off(&mut self) {
            self.active = false;
        }
        fn is_active(&self) -> bool {
            self.active
        }
        fn note(&self) -> u8 {
            self.note
        }
        fn render(&mut self) -> f32 {
            if self.active {
                1.0
            } else {
                0.0
            }
        }
    }

    fn active_count(m: &mut VoiceManager<TestVoice>) -> usize {
        let mut n = 0;
        m.for_each_active(|_| n += 1);
        n
    }

    #[test]
    fn note_on_activates_one() {
        let mut m = VoiceManager::<TestVoice>::new(8);
        m.note_on(60, 1.0);
        assert_eq!(active_count(&mut m), 1);
        let mut found_note = 0;
        m.for_each_active(|v| found_note = v.note());
        assert_eq!(found_note, 60);
        assert_eq!(m.render(), 1.0);
    }

    #[test]
    fn note_off_releases_note() {
        let mut m = VoiceManager::<TestVoice>::new(8);
        m.note_on(60, 1.0);
        m.note_off(60);
        assert_eq!(active_count(&mut m), 0);
        assert_eq!(m.render(), 0.0);
    }

    #[test]
    fn polyphony_sums() {
        let mut m = VoiceManager::<TestVoice>::new(8);
        m.note_on(60, 1.0);
        m.note_on(64, 1.0);
        assert_eq!(active_count(&mut m), 2);
        assert_eq!(m.render(), 2.0);
    }

    #[test]
    fn note_on_with_configures_before_trigger() {
        // The closure must run on the chosen voice BEFORE `note_on`, so a fresh
        // voice is configured on its very first sample. TestVoice records the
        // note it was triggered with; the closure bumps `sr` and we confirm the
        // voice was both configured and triggered.
        let mut m = VoiceManager::<TestVoice>::new(4);
        let mut configured_active_at_call = true;
        m.note_on_with(72, 1.0, |v| {
            // At configure time the voice has NOT yet been triggered.
            configured_active_at_call = v.is_active();
            v.set_sample_rate(96_000.0);
        });
        assert!(
            !configured_active_at_call,
            "configure closure ran after note_on"
        );
        let mut seen_note = 0;
        let mut seen_sr = 0.0;
        m.for_each_active(|v| {
            seen_note = v.note();
            seen_sr = v.sr;
        });
        assert_eq!(seen_note, 72, "voice not triggered with the note");
        assert_eq!(seen_sr, 96_000.0, "configure closure did not apply");
    }

    #[test]
    fn steals_oldest_when_full() {
        let mut m = VoiceManager::<TestVoice>::new(2);
        m.note_on(60, 1.0);
        m.note_on(64, 1.0);
        m.note_on(67, 1.0);
        assert!(active_count(&mut m) <= 2);
        let mut has_60 = false;
        let mut has_67 = false;
        m.for_each_active(|v| {
            if v.note() == 60 {
                has_60 = true;
            }
            if v.note() == 67 {
                has_67 = true;
            }
        });
        assert!(!has_60, "oldest note 60 should have been stolen");
        assert!(has_67, "newest note 67 should be present");
    }
}
