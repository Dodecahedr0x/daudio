//! Monophonic note trigger: debounced, level-gated, velocity from level.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NoteAction {
    On { note: u8, velocity: f32 },
    Off { note: u8 },
}

pub struct Trigger {
    hold_hops: u32,
    /// A new candidate at or above this clarity commits without waiting out the
    /// Hold debounce — unless it is a large jump from the currently-held note.
    /// Clarity is pitch-dependent at a fixed window (fewer periods -> lower
    /// clarity), so this threshold intentionally gives the fast path to mid/high
    /// notes; low notes debounce via Hold.
    fast_clarity: f32,
    /// Max semitone distance from the held note for a fast (undebounced) commit.
    max_jump: i32,
    active: Option<u8>,
    candidate: Option<i32>,
    candidate_hops: u32,
}

impl Default for Trigger {
    fn default() -> Self {
        Self::new()
    }
}

impl Trigger {
    pub fn new() -> Self {
        Self {
            hold_hops: 2,
            fast_clarity: 0.8,
            max_jump: 7,
            active: None,
            candidate: None,
            candidate_hops: 0,
        }
    }

    pub fn set_hold(&mut self, hold_ms: f32, hop_seconds: f32) {
        self.hold_hops = ((hold_ms / 1000.0) / hop_seconds).ceil().max(1.0) as u32;
    }

    pub fn set_fast_clarity(&mut self, c: f32) {
        self.fast_clarity = c;
    }

    pub fn set_max_jump(&mut self, j: i32) {
        self.max_jump = j.max(0);
    }

    pub fn reset(&mut self) {
        self.active = None;
        self.candidate = None;
        self.candidate_hops = 0;
    }

    pub fn on_hop(
        &mut self,
        target: Option<i32>,
        clarity: f32,
        velocity: f32,
        emit: &mut dyn FnMut(NoteAction),
    ) {
        if target.is_none() {
            if let Some(n) = self.active.take() {
                emit(NoteAction::Off { note: n });
            }
            self.candidate = None;
            self.candidate_hops = 0;
            return;
        }
        let target = target.unwrap();
        if Some(target) == self.active.map(|n| n as i32) {
            self.candidate = None;
            self.candidate_hops = 0;
            return;
        }
        if self.candidate == Some(target) {
            self.candidate_hops += 1;
        } else {
            self.candidate = Some(target);
            self.candidate_hops = 1;
        }

        let near = match self.active {
            Some(n) => (target - n as i32).abs() <= self.max_jump,
            None => true,
        };
        let fast = clarity >= self.fast_clarity && near;

        if fast || self.candidate_hops >= self.hold_hops {
            if let Some(n) = self.active.take() {
                emit(NoteAction::Off { note: n });
            }
            let note = target.clamp(0, 127) as u8;
            emit(NoteAction::On { note, velocity });
            self.active = Some(note);
            self.candidate = None;
            self.candidate_hops = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn collect(t: &mut Trigger, target: Option<i32>, clarity: f32, vel: f32) -> Vec<NoteAction> {
        let mut v = Vec::new();
        t.on_hop(target, clarity, vel, &mut |a| v.push(a));
        v
    }
    #[test]
    fn high_clarity_commits_on_first_hop() {
        let mut t = Trigger::new();
        assert_eq!(
            collect(&mut t, Some(60), 0.95, 0.8),
            vec![NoteAction::On {
                note: 60,
                velocity: 0.8
            }]
        );
    }
    #[test]
    fn low_clarity_still_debounces() {
        let mut t = Trigger::new();
        assert!(collect(&mut t, Some(60), 0.5, 0.8).is_empty());
        assert_eq!(
            collect(&mut t, Some(60), 0.5, 0.8),
            vec![NoteAction::On {
                note: 60,
                velocity: 0.8
            }]
        );
    }
    #[test]
    fn high_clarity_big_jump_from_held_note_debounces() {
        let mut t = Trigger::new();
        collect(&mut t, Some(60), 0.95, 0.8); // 60 committed immediately (no held note)
        assert!(collect(&mut t, Some(72), 0.95, 0.8).is_empty()); // 12 semis -> debounce
        assert_eq!(
            collect(&mut t, Some(72), 0.95, 0.8),
            vec![
                NoteAction::Off { note: 60 },
                NoteAction::On {
                    note: 72,
                    velocity: 0.8
                }
            ]
        );
    }
    #[test]
    fn raising_fast_clarity_forces_debounce() {
        let mut t = Trigger::new();
        t.set_fast_clarity(0.95);
        // Clarity 0.9 is below the raised threshold, so it no longer fast-commits.
        assert!(collect(&mut t, Some(60), 0.9, 0.8).is_empty());
        assert_eq!(
            collect(&mut t, Some(60), 0.9, 0.8),
            vec![NoteAction::On {
                note: 60,
                velocity: 0.8
            }]
        );
    }
    #[test]
    fn raising_max_jump_allows_big_fast_commit() {
        let mut t = Trigger::new();
        t.set_max_jump(12);
        collect(&mut t, Some(60), 0.95, 0.8); // 60 committed immediately (no held note)
                                              // 12 semis is within the raised max jump, so it fast-commits at high clarity.
        assert_eq!(
            collect(&mut t, Some(72), 0.95, 0.8),
            vec![
                NoteAction::Off { note: 60 },
                NoteAction::On {
                    note: 72,
                    velocity: 0.8
                }
            ]
        );
    }
    #[test]
    fn gate_close_releases_active_note() {
        let mut t = Trigger::new();
        collect(&mut t, Some(60), 0.95, 0.8);
        assert_eq!(
            collect(&mut t, None, 0.0, 0.0),
            vec![NoteAction::Off { note: 60 }]
        );
    }
}
