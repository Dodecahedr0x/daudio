# Pitch2MIDI — Expose Tracking Controls — Design

**Date:** 2026-07-03
**Status:** Design approved (self-directed), ready for implementation

## Goal

Make every hidden tracking "assumption" adjustable from the UI — directly for the musically
meaningful ones, or folded into an aggregated control where a raw number would just add clutter.

## Assumptions today (hardcoded constants) → where each becomes accessible

| Assumption | Now | Exposed via |
|------------|-----|-------------|
| Detection **window** | 1024 | **Response** (Fast/Balanced/Full) |
| Detection **hop** | 128 | **Response** (same control) |
| Detector **power gate** (`POWER_THRESHOLD`) | 0.15 | **Sensitivity** (existing level control, now also scales this) |
| Detector **clarity gate** (`CLARITY_THRESHOLD`) | 0.6 | **Confidence** (new) |
| Trigger **fast-commit clarity** (`CLARITY_FAST`) | 0.8 | **Confidence** (derived) |
| Trigger **max-jump guard** (`FAST_MAX_JUMP`) | 7 | **Max Jump** (new) |
| Pitch-bend **range** | ±2 semitones | **Bend Range** (new) |

`level_decay` (~50 ms peak follower for the level gate/meter) stays fixed — it's metering
smoothing, not a tracking parameter.

## Control set (final)

Existing: Root + degree toggles, **Sensitivity** (dB), **Hold** (ms), **Pitch Bend** (toggle).

New:
1. **Response** — `EnumParam { Fast, Balanced, Full }`. Sets detection window + hop:
   Fast = 512/64 (~11 ms, lead/voice only), Balanced = 1024/128 (~21 ms, default),
   Full = 2048/256 (~43 ms, reaches lower notes). The latency ↔ low-note-reach trade-off in one knob.
2. **Confidence** — `FloatParam 0..1` (default 0.6). The minimum detection clarity to accept a
   pitch (`CLARITY_THRESHOLD`); also derives the fast-commit threshold
   `CLARITY_FAST = min(confidence + 0.2, 0.98)`. Aggregates the two clarity assumptions into one
   "how sure must it be" knob.
3. **Max Jump** — `IntParam 1..12` semitones (default 7). `FAST_MAX_JUMP` — how far a confident new
   note may be from the held note and still commit instantly (larger = snappier leaps, more octave risk).
4. **Bend Range** — `IntParam 1..12` semitones (default 2). The ± range mapped to full pitch bend.
   Relevant when Pitch Bend is on.

**Sensitivity** additionally scales the detector's `POWER_THRESHOLD` (monotonic map from its
−60…0 dB range), so the one "how loud" control governs both the trigger's level gate and the
detector's energy gate.

## Making the constants runtime-adjustable (RT-safe)

The window/hop/thresholds are read inside the **worker thread's** detection loop; window changes
require rebuilding the `McLeodDetector`. That rebuild allocates, so it must NOT happen on the
audio thread. Design:

- `PitchTracker` gains shared atomics the audio thread writes and the worker reads:
  `window` (`AtomicUsize`), `hop` (`AtomicUsize`), `power` + `clarity` (`AtomicU32` bit-cast f32).
- **Worker loop:** each iteration, load `window`; if it changed, rebuild its `PitchDetectorCore`
  with the new window (off the audio thread → allocation OK). Load hop/power/clarity and apply
  them before `get_pitch`.
- **Audio thread (`push`)** only *writes* those atomics (via a `set_config(window, hop, power,
  clarity)` method the plugin calls when a param changes) and reads `hop` to gate its result
  cadence. Writing atomics is allocation-free — RT-safe.
- The `PitchDetectorCore` ring is sized to the **max** window (2048); detection runs on the last
  `window` samples. So a window change never resizes the ring.

`PitchDetectorCore::new` takes a `window`; `WINDOW`/`HOP` consts become a `MAX_WINDOW` const and
runtime values. The trigger's `CLARITY_FAST`/`FAST_MAX_JUMP` become fields set via
`set_fast_clarity`/`set_max_jump`. `bend_value` gains a `range_semitones` argument.

## UI

Add the new controls to the existing **DETECTION** card (Sensitivity, Hold already there):
**Response** (a `ParamSlider` over the enum, like Root), **Confidence** knob, **Max Jump** knob,
and — grouped with the Pitch Bend toggle — a **Bend Range** knob. Reuse `daudio_ui::{Knob,
ParamControl, ParamSlider}`; the DETECTION card may split into a second row or the window grow.

## Testing

- `PitchDetectorCore` detects at each window size (512/1024/2048) for an in-range tone; the ring is
  max-sized and a runtime window change produces a valid detection at the new size.
- `set_config` / worker rebuild: feed a tone, switch window mid-stream, assert detection continues.
- Trigger: `set_fast_clarity` / `set_max_jump` change the fast-path behavior (extend existing tests).
- `bend_value(_, _, range)` scales correctly (range 1 vs 2 vs 12).
- UI: screenshot the editor and confirm the new controls render and are laid out cleanly.

## Out of scope (YAGNI)

- Per-note MPE; automatable window as a continuous value (the 3-way Response is enough);
  exposing `level_decay`.
