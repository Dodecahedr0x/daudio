# Pitch2MIDI — Lower Latency — Design

**Date:** 2026-07-03
**Status:** Design approved, ready for implementation planning

## Goal

Roughly halve Pitch2MIDI's note-on latency for **voice/lead** sources (≈ C3 and up), without
adding jitter or false notes. Bass tracking is explicitly out of scope for this change.

## Latency budget

Where note-on latency comes from today (at 48 kHz):

| Source | Now | Note |
|--------|-----|------|
| Detection **window** | 2048 samples (~43 ms) | Dominant term — McLeod needs a full window to estimate pitch. |
| **Debounce** (Hold param) | 40 ms default | New note must persist several hops before committing (anti-glitch). |
| **Hop** + worker | 256 samples (~5 ms) | Detection cadence on the worker thread. |

Worst-case note-on ≈ **~88 ms**.

Three changes bring typical note-on to **~25 ms**:
1. Window 2048 → **1024** (~21 ms). Still resolves ~131 Hz (C3) and up.
2. Hop 256 → **128** (~3 ms). Faster cadence, modest worker CPU.
3. **Confidence-aware commit**: commit in ~1 hop when clarity is high; fall back to the Hold
   debounce only when clarity is marginal.

Accepted trade-off: a shorter window is slightly noisier and won't track bass — fine for the
voice/lead scope, and the clarity gate guards the added noise.

## `daudio-dsp::PitchTracker` (crates/daudio-dsp/src/pitch.rs)

1. **Constants:** `WINDOW: 2048 → 1024`, `PADDING: 512`, `HOP: 256 → 128`. The `McLeodDetector`
   is built with the new size; the ring/scratch reconstruction already derives from these.
2. **Publish clarity with frequency.** McLeod already returns a `clarity` (0–1); today it's
   discarded. Replace the result channel's `AtomicU32` (bit-cast `f32` freq, `NaN` = no pitch)
   with a single **`AtomicU64`** packing both: `(freq_bits as u64) << 32 | clarity_bits`. One
   atomic keeps the pair consistent (no torn freq/clarity across hops). `NaN` freq still = no pitch.
3. **`Detection` type:** `Pitch(f32)` → `Pitch { freq: f32, clarity: f32 }`. `push()` unpacks the
   `AtomicU64`; `NoPitch` unchanged.

Real-time safety preserved: the audio thread still only pushes to the ring and reads one atomic;
detection stays on the worker thread. No new `unsafe`. `PitchTracker`'s public API is unchanged
except `push`'s return gains `clarity` (pitch-to-midi is the only caller).

## Confidence-aware trigger (plugins/pitch-to-midi/src/trigger.rs)

`on_hop(target, velocity, emit)` → `on_hop(target, clarity, velocity, emit)`:

- **Fast path:** a new candidate with `clarity ≥ CLARITY_FAST` (internal const, ≈ 0.9) commits on
  the **first hop** — no debounce — but only if there is **no note currently held**, or the
  candidate is within a few semitones of the held note. A large jump against a held note still
  debounces (guards against a high-clarity octave error firing instantly).
- **Slow path:** clarity below the threshold keeps the existing `hold_hops` debounce (from the
  Hold param) — the safety net for ambiguous input.
- **Gate-close:** release on silence/no-pitch stays immediate.

## Parameters

No new user-facing param (the clarity threshold is internal — YAGNI). Lower the **Hold** default
40 ms → **~25 ms** (it's now the low-confidence fallback, not the common path). Sensitivity gate
unchanged. The `Hold` knob still reads in ms and still works.

## Wiring

In `process_sample`, the plugin already holds the `Detection`; it now passes `clarity` into
`trigger.on_hop(...)`. The detected/output readout atomics are unaffected.

## Testing

- **`PitchDetectorCore`:** a ~131 Hz (C3) sine detected within ±3 Hz at the 1024 window; clarity
  > 0.8 for a clean sine; the freq+clarity `AtomicU64` pack/unpack round-trips (incl. `NaN`).
- **`Trigger`:** extend the table-driven tests — (a) high-clarity new note commits on hop 1;
  (b) low-clarity new note waits `hold_hops`; (c) high-clarity large-jump against a held note
  debounces (octave guard); (d) gate-close still releases immediately.
- **Latency (measurable, headless):** `run_analyzer` prints note-on timestamps. Feed a tone
  starting at t=0; the printed note-on time ≈ latency. Observe it drop from ~80 ms to ~25–30 ms.

## Risks & mitigations

- Shorter window noisier / weaker low-end → bounded by the ≥ C3 scope; clarity gate + jump guard absorb noise.
- High-clarity octave error fires fast → the semitone-jump guard restricts the fast path to first/near notes.
- No bass tracking → out of scope, documented.

## Out of scope (YAGNI)

- Host latency reporting (MIDI-output compensation is murky, DAW-inconsistent).
- A user-facing window/latency selector (the range choice fixed the window).
- Polyphony.

## Rollout

One focused change set — `daudio-dsp` (window/hop/clarity), the trigger, and the plugin's
`process_sample` wiring — verified by the tests above and a before/after latency read from the
`demo` binary.
