//! Monophonic note trigger: debounced, level-gated, velocity from level.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NoteAction {
    On { note: u8, velocity: f32 },
    Off { note: u8 },
}

pub struct Trigger {
    hold_hops: u32,
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
            active: None,
            candidate: None,
            candidate_hops: 0,
        }
    }

    pub fn set_hold(&mut self, hold_ms: f32, hop_seconds: f32) {
        self.hold_hops = ((hold_ms / 1000.0) / hop_seconds).ceil().max(1.0) as u32;
    }

    pub fn reset(&mut self) {
        self.active = None;
        self.candidate = None;
        self.candidate_hops = 0;
    }

    pub fn on_hop(&mut self, target: Option<i32>, velocity: f32, emit: &mut dyn FnMut(NoteAction)) {
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
        if self.candidate_hops >= self.hold_hops {
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
    fn collect(t: &mut Trigger, target: Option<i32>, vel: f32) -> Vec<NoteAction> {
        let mut v = Vec::new();
        t.on_hop(target, vel, &mut |a| v.push(a));
        v
    }
    #[test]
    fn debounce_blocks_one_hop_blip() {
        let mut t = Trigger::new();
        assert!(collect(&mut t, Some(60), 0.8).is_empty());
        assert!(collect(&mut t, Some(67), 0.8).is_empty());
        assert_eq!(
            collect(&mut t, Some(67), 0.8),
            vec![NoteAction::On {
                note: 67,
                velocity: 0.8
            }]
        );
    }
    #[test]
    fn gate_close_releases_active_note() {
        let mut t = Trigger::new();
        collect(&mut t, Some(60), 0.8);
        collect(&mut t, Some(60), 0.8);
        assert_eq!(
            collect(&mut t, None, 0.0),
            vec![NoteAction::Off { note: 60 }]
        );
    }
    #[test]
    fn note_change_sends_off_then_on() {
        let mut t = Trigger::new();
        collect(&mut t, Some(60), 0.8);
        collect(&mut t, Some(60), 0.8);
        collect(&mut t, Some(64), 0.9);
        assert_eq!(
            collect(&mut t, Some(64), 0.9),
            vec![
                NoteAction::Off { note: 60 },
                NoteAction::On {
                    note: 64,
                    velocity: 0.9
                }
            ]
        );
    }
}
