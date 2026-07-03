# Pitch2MIDI — Auto Pitch-Bend by Volume Continuity — Design

**Date:** 2026-07-03
**Status:** Design (self-directed), ready to implement

## Goal

Automatically decide whether a detected pitch change is a **pitch bend** (a
slur/glide) or a **separate note** — using the volume envelope. Musical fact: a
bend sustains its volume (no dip), whereas a separately-articulated note has a
re-attack, i.e. the volume dips at the note boundary. So:

- pitch moves **and volume stayed continuous** → **bend**: keep the held MIDI note,
  emit pitch-bend toward the new pitch; do **not** retrigger.
- pitch moves **and volume dipped** (re-articulation) → **new note**: note-off + note-on.

## Control

Replace the current `pitch_bend: BoolParam` with a **`bend_mode: EnumParam<BendMode>`**:
- **Off** — never bend; every quantized note change retriggers (current default).
- **On** — manual bend: emit pitch-bend tracking the detected pitch within the held
  note, but note changes still retrigger (fine expression only).
- **Auto** — volume-gated: a quantized note change **bends** instead of retriggering
  when the volume didn't dip *and* the new note is within the bend range; otherwise
  it's a separate note. This is the new feature.

`bend_range` (semitones) is unchanged and applies to On/Auto.

## Volume-dip detection (per hop)

The existing `level` peak-follower (slow ~50 ms release) is too smooth to see a
short re-articulation dip, so add a fast per-hop amplitude measure:
- Each sample: `hop_peak = max(hop_peak, |input|)`.
- At each detection hop: `amp = hop_peak; hop_peak = 0`.
- While a note is held: `note_peak = max(note_peak, amp)`; if
  `amp < note_peak * DIP_RATIO` set `dipped = true`. `DIP_RATIO = 0.5` (−6 dB) — a
  re-articulation typically dips more than 6 dB; a legato slide stays within it.
- On each committed note-on: `note_peak = amp; dipped = false`.

## Decision (pure, testable)

```rust
/// True when a quantized-note change should be treated as a bend rather than a
/// new note: the volume stayed continuous (no dip) AND the new note is within the
/// bend range of the held note.
fn is_bend(dipped: bool, held: u8, target: i32, bend_range: i32) -> bool {
    !dipped && (target - held as i32).abs() <= bend_range
}
```

## Wiring (`process_sample`, per hop)

```text
raw_target = quantize(detected)                // Option<i32>
effective_target =
    if Auto and a note is held and is_bend(dipped, held, target, bend_range)
        Some(held)     // feed the trigger the *same* note → no retrigger
    else
        raw_target
trigger.on_hop(effective_target, clarity, velocity, emit)   // on note-on: note_peak=amp, dipped=false
if bend_mode != Off and a note is held:
    emit MidiPitchBend from bend_value(detected_freq, held_note, bend_range)   // continuous
else recenter bend once
```

So during a bend the trigger sees an unchanged note (no retrigger) and the
pitch-bend expresses the moving pitch; the held note keeps its velocity — the
volume does not drop, matching the intent.

## UI

Replace the "Pitch Bend" toggle in the DETECTION card with a **Bend Mode**
selector (a `ParamSlider` over the enum, like Root/Response). `Bend Range` stays.

## Testing

- `is_bend`: table — no dip + small step (≤ range) → bend; dip → not bend; jump
  beyond range → not bend.
- Build + screenshot the editor (Bend Mode selector renders).
- Manual: sing a slur vs two separate notes; Auto should bend the slur and
  retrigger the separate notes.

## Out of scope (YAGNI)

- Exposing `DIP_RATIO` as a control (internal for now; could become a "Legato"
  knob later); polyphonic bends; per-note MPE.
