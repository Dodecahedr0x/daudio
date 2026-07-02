# daudio Meter + Theme Accent — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add a custom themed level `Meter` widget to `daudio-ui`, backed by a lock-free audio→UI `PeakLevel` channel in `daudio-sdk`, wire a single-source `ACCENT` color into the canvas-drawn widgets (Knob + Meter), and show an output meter in the filter editor.

**Architecture:** The audio↔UI channel is a `PeakLevel` (an `Arc<AtomicU32>` holding a bit-cast f32; no new dependency). The audio thread writes a per-sample peak-with-decay; the editor reads the latest value each repaint. The `Meter` is a custom Vizia leaf view (like `Knob`) that holds a `PeakLevel` clone, draws a bar filled with the theme `ACCENT`, and uses a Vizia timer to repaint at ~30 fps (its own repaint driver, analogous to the Knob's `needs_redraw`).

**Tech Stack:** Rust nightly, nih_plug_vizia (pinned rev `f36931f7af4646065488a9845d8f8c2f95252c23`), Vizia + femtovg, `std::sync::atomic`.

**REFERENCE:** `nih_plug_vizia/src/widgets/peak_meter.rs` in the pinned checkout — for the timer/repaint-driver pattern and dB→position mapping. Read it before Task 2. (We build our own themed meter but copy its repaint approach.)

---

## Scope

In scope: `PeakLevel` (daudio-sdk) with a unit test; `theme::ACCENT` reintroduced + used by `Knob` and the new `Meter`; a custom `Meter` widget; the filter output meter (per-sample peak tracking + editor meter).

Out of scope: RMS/LUFS metering, stereo split meters, numeric readout, meter on the synth, gain-reduction meters. YAGNI.

---

## Task 1: `PeakLevel` audio→UI channel (daudio-sdk)

**Files:** create `crates/daudio-sdk/src/meter.rs`; export from lib.rs (+ prelude).

A lock-free single-f32 channel. Bit-cast f32 into an `AtomicU32` to avoid an external atomic-float dependency.
```rust
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

/// A lock-free peak-level channel shared between the audio thread (writer) and
/// the editor (reader). Cheap `Clone` (shared `Arc`). Real-time-safe: writes and
/// reads are single relaxed atomic ops, no allocation or locking.
#[derive(Clone)]
pub struct PeakLevel(Arc<AtomicU32>);

impl PeakLevel {
    pub fn new() -> Self { Self(Arc::new(AtomicU32::new(0f32.to_bits()))) }
    /// Audio thread: store the current linear peak (>= 0).
    pub fn write(&self, value: f32) { self.0.store(value.to_bits(), Ordering::Relaxed); }
    /// UI thread: read the latest linear peak.
    pub fn read(&self) -> f32 { f32::from_bits(self.0.load(Ordering::Relaxed)) }
}

impl Default for PeakLevel { fn default() -> Self { Self::new() } }
```
**Test (write first):**
```rust
#[test]
fn write_then_read_roundtrips() {
    let p = PeakLevel::new();
    assert_eq!(p.read(), 0.0);
    p.write(0.5);
    assert_eq!(p.read(), 0.5);
    let clone = p.clone();          // shares state
    p.write(0.25);
    assert_eq!(clone.read(), 0.25);
}
```
Add `pub mod meter; pub use meter::PeakLevel;` to lib.rs and `PeakLevel` to the prelude. Verify test + clippy + fmt. **Commit** `feat(sdk): add lock-free PeakLevel audio->UI channel`

---

## Task 2: Theme accent + `Meter` widget (daudio-ui)

**Files:** modify `crates/daudio-ui/src/theme.rs`, `crates/daudio-ui/src/knob.rs`, `crates/daudio-ui/src/lib.rs`; create `crates/daudio-ui/src/meter.rs`.

**Step 1 — reintroduce ACCENT and use it.** In `theme.rs`:
```rust
use nih_plug_vizia::vizia::vg;
/// Suite accent color — the single branding source for canvas-drawn widgets.
pub const ACCENT: vg::Color = vg::Color { r: 0.369, g: 0.545, b: 1.0, a: 1.0 }; // ~#5e8bff
```
> Use whatever `vg::Color` literal/const-constructor the pinned femtovg supports (fields or `vg::Color::rgbf`). If a `const` color isn't constructible, expose `pub fn accent() -> vg::Color` instead and use that. The point: ONE definition consumed by both widgets.

In `knob.rs`, replace the hard-coded `value_color = vg::Color::rgb(0x5e,0x8b,0xff)` with `crate::theme::ACCENT` (or `accent()`), so the knob's value arc is the accent color. Leave track/pointer colors as-is.

**Step 2 — the `Meter` widget** in `meter.rs`. Model the repaint driver on `nih_plug_vizia`'s `PeakMeter` (read that source):
```rust
use daudio_sdk::PeakLevel;
use nih_plug_vizia::vizia::prelude::*;
use nih_plug_vizia::vizia::vg;

pub struct Meter { level: PeakLevel }

impl Meter {
    pub fn new(cx: &mut Context, level: PeakLevel) -> Handle<Self> {
        let view = Self { level };
        let handle = view.build(cx, |_| {});
        // Drive ~30fps repaints so the meter animates as audio levels change.
        // Use vizia's timer API (match peak_meter.rs): start a repeating timer
        // that calls cx.needs_redraw() on tick.
        handle
    }
}

impl View for Meter {
    fn element(&self) -> Option<&'static str> { Some("daudio-meter") }
    fn draw(&self, cx: &mut DrawContext, canvas: &mut Canvas) {
        let b = cx.bounds();
        // read linear peak -> dB -> normalized over [-60, 0]:
        let db = nih_plug::util::gain_to_db(self.level.read().max(1e-6));
        let t = ((db + 60.0) / 60.0).clamp(0.0, 1.0);
        // draw a background rounded rect (track color) and a filled portion of
        // height t*bounds.h from the bottom, colored crate::theme::ACCENT.
    }
}
```
> Verify the vizia timer API against `peak_meter.rs` for the pinned rev (`cx.add_timer` / `Timer` / `cx.start_timer` — names differ across versions). The timer MUST call `cx.needs_redraw()` on tick so the meter repaints continuously; otherwise it freezes like the Knob would. If the timer approach is unavailable, fall back to the same pattern `peak_meter.rs` uses. Use `nih_plug::util::gain_to_db` (verify path) or `daudio_dsp::gain::gain_to_db`.

**Step 3 — exports.** Add `mod meter; pub use meter::Meter;` to lib.rs and `Meter` (+ re-export `daudio_sdk::PeakLevel` or expect the plugin to import it) to the prelude.

Verify `cargo build -p daudio-ui` + `cargo build -p filter` compile; clippy/fmt clean. **Commit** `feat(ui): add themed Meter widget and wire ACCENT into canvas widgets`

---

## Task 3: Filter output meter (the proof)

**Files:** `plugins/filter/src/lib.rs`, `plugins/filter/src/dsp.rs` (maybe), `theme.css` if a meter class is needed.

**Step 1 — plugin state.** Add to `FilterPlugin`: `meter: daudio_sdk::PeakLevel` (a plain field, not a param). In `Default`, `meter: PeakLevel::new()`. Add a `peak_decay: f32` and `peak_val: f32` (or keep them in FilterCore — simplest: keep in the plugin struct). Set `peak_decay` in `activate` from sample rate: e.g. `let t = 0.3; self.peak_decay = (-1.0/(t*sr)).exp();` (≈300 ms fall).

**Step 2 — write the peak in `process_frame`.** After computing `(ol, or)`:
```rust
let level = ol.abs().max(or.abs());
self.peak_val = level.max(self.peak_val * self.peak_decay);
self.meter.write(self.peak_val);
```
(These are cheap scalar ops + one relaxed atomic store — RT-safe.)

**Step 3 — editor.** In `editor()`, clone `self.meter` and add a `Meter::new(cx, meter.clone())` beside the two knobs (e.g. knobs in an HStack, meter to their right), sized via `theme.css` (`.daudio-meter { width: 16px; height: 80px; }`). Bump `editor_state` size if needed.

**Step 4 — verify:**
- `cargo build -p filter` + `--bin standalone` compile.
- `cargo test --workspace` — all prior tests + the PeakLevel test pass.
- `cargo clippy --workspace -- -D warnings` clean; `cargo fmt --check` clean.
- `cargo xtask bundle filter --release` produces the bundles.
- MANUAL (human): `cargo run -p filter --bin standalone`, feed audio → the meter bar rises with input level and falls smoothly; knob value arcs are the accent color.

**Step 5 — commit** `feat(filter): output level meter in the editor`

---

## Definition of Done

- `daudio-sdk` has `PeakLevel` (tested); `daudio-ui` has a themed `Meter` and `ACCENT` used by Knob + Meter.
- `cargo test --workspace` green; clippy `-D warnings` clean; fmt clean; filter bundles.
- Filter editor shows a working, smoothly-decaying output meter and accent-colored knobs (human-verified via standalone).

## Follow-up (not this plan)

- Stereo/dual meters, numeric dB readout, RMS/LUFS.
- Meter on the synth (master output).
- Gain-reduction meter (needs a compressor first).
