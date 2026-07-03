# Pitch2MIDI — Output Note Shaping (Velocity from Volume) — Design

**Date:** 2026-07-03
**Status:** Design (self-directed), ready to implement

## Goal

Give the user control over the **properties of emitted notes**, starting with the
one the request named: **velocity derived from an input volume range**. Today
velocity is just the raw peak level (`self.level.clamp(0,1)`) — quiet input →
weak notes, with no control. Replace it with a configurable mapping from an input
dB window to an output velocity range, with a response curve.

## Controls (new DYNAMICS card)

| Control | Type | Default | Meaning |
|---------|------|---------|---------|
| **Vel Floor** | dB, −80..0 | −40 | input level mapped to the *minimum* velocity |
| **Vel Ceil** | dB, −60..0 | −6 | input level mapped to the *maximum* velocity |
| **Vel Min** | int 1..127 | 1 | velocity emitted at/below Floor |
| **Vel Max** | int 1..127 | 127 | velocity emitted at/above Ceil |
| **Curve** | 0..1 | 0.5 | response shape: 0.5 = linear, <0.5 eases in, >0.5 eases out |

- Floor/Ceil define the **input volume range**; Min/Max the **output velocity range**.
- **Fixed velocity** needs no extra control: set Vel Min = Vel Max.
- Curve shapes how loudness maps across the range (expressive dynamics).

## Mapping (pure, testable)

```rust
/// curve 0.5 = linear; <0.5 eases in (slow start), >0.5 eases out (fast start).
fn velocity_curve(t: f32, curve: f32) -> f32 {
    let gamma = 2f32.powf((0.5 - curve) * 3.0); // 0.5->1, 0->~2.8, 1->~0.35
    t.powf(gamma)
}

/// Map a linear input level to a MIDI velocity fraction (0..1). `min_v`/`max_v`
/// are already normalized (vel/127).
fn map_velocity(level: f32, floor_db: f32, ceil_db: f32, min_v: f32, max_v: f32, curve: f32) -> f32 {
    let db = daudio_dsp::gain::gain_to_db(level.max(1e-6));
    let span = (ceil_db - floor_db).max(1e-3);
    let t = ((db - floor_db) / span).clamp(0.0, 1.0);
    (min_v + velocity_curve(t, curve) * (max_v - min_v)).clamp(0.0, 1.0)
}
```

## Wiring

In `process_sample`, replace `let velocity = self.level.clamp(0.0, 1.0);` with
`map_velocity(self.level, floor, ceil, vel_min/127, vel_max/127, curve)`. The
value flows unchanged into `trigger.on_hop` → the emitted `NoteOn` velocity, so
the velocity is sampled at note-on time (as now).

## UI

Add a **DYNAMICS** card (full-width row under the DETECTION/readout row) with the
five controls as `ParamControl` knobs. Bump `editor_state` height to fit.

## Testing

- `map_velocity`: level at Ceil → ≈ max_v; at Floor → ≈ min_v; below Floor →
  min_v (clamped); above Ceil → max_v; linear (curve 0.5) midpoint dB →
  (min+max)/2; Vel Min == Vel Max → constant regardless of level.
- `velocity_curve`: curve 0.5 is identity; endpoints 0→0, 1→1 for any curve.
- Build + screenshot the DYNAMICS card.

## Out of scope (YAGNI)

- Output MIDI channel / fixed-velocity toggle (Min=Max covers fixed); per-note
  aftertouch/expression beyond the existing pitch-bend.
