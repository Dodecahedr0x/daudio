# daudio-ui Rotary Knob — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add a from-scratch rotary `Knob` widget to `daudio-ui`, bound to nih-plug params with full gesture support (drag, scroll, double-click-to-reset, host automation), and switch the filter's editor to use knobs instead of sliders.

**Architecture:** The `Knob` is a custom Vizia `View` built on `nih_plug_vizia::widgets::param_base::ParamWidgetBase` — the SAME helper `ParamSlider`/`ParamButton` use, which owns all param↔host plumbing (begin/set-normalized/end gestures, default value, string formatting). The widget adds only (1) arc drawing via Vizia's canvas and (2) mouse/scroll → normalized-value-delta handling. `ParamControl` is repointed to stack its label above a `Knob` instead of a `ParamSlider`, so the filter editor needs no structural change.

**Tech Stack:** Rust nightly, nih_plug_vizia (pinned rev `f36931f7af4646065488a9845d8f8c2f95252c23`), Vizia + its femtovg canvas.

**AUTHORITATIVE REFERENCES (read first, match the pinned rev):**
- `nih_plug_vizia/src/widgets/param_slider.rs` — how a real param widget is built on `ParamWidgetBase`: construction, the `begin_set_parameter`/`set_parameter_normalized`/`end_set_parameter` gesture pattern, and `ParamWidgetBase::view` for reactive rebuilds.
- `nih_plug_vizia/src/widgets/param_base.rs` — the `ParamWidgetBase` API surface (methods for normalized value, default, gesture begin/end, set).
- `nih_plug_vizia/src/widgets/generic_ui.rs` and any existing knob in the repo, if present.
- A Vizia canvas-drawing example (`impl View::draw` using `Canvas`, `vg::Path`, `vg::Paint`) for the current draw API.

---

## Scope

In scope: a `Knob` param widget (draw + interact), repointing `ParamControl` to use it, theme/size updates, and the filter editor using knobs. One effect param type (FloatParam) is all that's needed now.

Out of scope: value tooltip/text entry on the knob, modulation rings, meter, other widgets. If text-entry-on-double-click conflicts with reset, prefer double-click = reset (simplest) and defer text entry. YAGNI.

---

## Task 1: The `Knob` widget

**Files:**
- Create: `crates/daudio-ui/src/knob.rs`
- Modify: `crates/daudio-ui/src/lib.rs` (module + re-exports/prelude)

**Step 1: struct + construction.** Model construction on `ParamSlider::new`. The widget owns a `ParamWidgetBase` plus interaction state:
```rust
use nih_plug::prelude::Param;
use nih_plug_vizia::vizia::prelude::*;
use nih_plug_vizia::vizia::vg;
use nih_plug_vizia::widgets::param_base::ParamWidgetBase;

pub struct Knob {
    param_base: ParamWidgetBase,
    // drag state
    dragging: bool,
    // normalized value captured at drag start + the mouse-y at drag start
    drag_start_y: f32,
    drag_start_value: f32,
}

impl Knob {
    pub fn new<L, Params, P, FMap>(cx: &mut Context, params: L, params_to_param: FMap) -> Handle<Self>
    where
        L: Lens<Target = Params> + Clone,
        Params: 'static,
        P: Param + 'static,
        FMap: Fn(&Params) -> &P + Copy + 'static,
    {
        Self {
            param_base: ParamWidgetBase::new(cx, params.clone(), params_to_param),
            dragging: false,
            drag_start_y: 0.0,
            drag_start_value: 0.0,
        }
        .build(cx, |_cx| {})   // leaf view; drawing done in draw()
    }
}
```
> Match `ParamWidgetBase::new`'s exact signature/generics to `ParamSlider::new` in the pinned rev. `build`'s closure signature must match Vizia's `View::build`. Adjust as the reference dictates.

**Step 2: drawing.** Implement `View` with a `draw` that renders a rotary arc. Read the current normalized value from `self.param_base.unmodulated_normalized_value()` (verify the accessor name). Map [0,1] → angle over a 270° sweep starting at 135° (i.e. 7π/6) going clockwise to 45° (i.e. −π/6), leaving a gap at the bottom. Draw: a background track arc, a value arc from start to the current angle, and a center pointer line. Use the widget's bounding box and theme-ish colors (hard-code sensible dark-theme colors for now; wire to CSS later if easy).
```rust
impl View for Knob {
    fn element(&self) -> Option<&'static str> { Some("daudio-knob") }

    fn draw(&self, cx: &mut DrawContext, canvas: &mut Canvas) {
        let bounds = cx.bounds();
        // center, radius from bounds; sweep 270°; draw track arc, value arc, pointer.
        // Use vg::Path::arc / move_to / line_to, vg::Paint::color(...) with stroke width.
        // value = self.param_base.unmodulated_normalized_value();
    }

    fn event(&mut self, cx: &mut EventContext, event: &mut Event) { /* Step 3 */ }
}
```
> The draw API (`DrawContext`, `Canvas`, `vg::Path`, `Paint`, arc direction/`Solidity`) must match the pinned vizia. Copy the calling conventions from a working vizia draw impl. Colors can be literals now.

**Step 3: interaction** in `event`. Handle `WindowEvent`s:
- `MouseDown(Left)`: `self.param_base.begin_set_parameter(cx);` capture `self.drag_start_value = self.param_base.unmodulated_normalized_value();` and `self.drag_start_y` from `cx.mouse()`; set `dragging = true`; `cx.capture()`.
- `MouseMove`: if dragging, compute `delta = (drag_start_y - current_y) * SENSITIVITY` (up = increase; SENSITIVITY ≈ 1/200), `new = (drag_start_value + delta).clamp(0.0, 1.0)`, `self.param_base.set_parameter_normalized(cx, new);`.
- `MouseUp(Left)`: if dragging, `self.param_base.end_set_parameter(cx); dragging = false; cx.release();`.
- `MouseScroll`: nudge normalized value by a small step (e.g. ±0.02) wrapped in begin/set/end.
- `MouseDoubleClick(Left)` (or `MouseDown` with a double-click check): reset to default — `let d = self.param_base.default_normalized_value(); begin/set(d)/end`.
> Verify every `ParamWidgetBase` method name (`begin_set_parameter`, `set_parameter_normalized`, `end_set_parameter`, `default_normalized_value`, `unmodulated_normalized_value`) against the pinned source — names may differ slightly; use the real ones. Match how `param_slider.rs` fires these.

**Step 4: lib.rs** — `mod knob; pub use knob::Knob;` and add `Knob` to the `prelude`.

**Step 5: verify** `cargo build -p daudio-ui` compiles; clippy `-D warnings` clean; fmt clean. (No unit test — GUI; the gate is Task 3's build + human visual check.) **Commit** `feat(ui): add rotary Knob param widget`

---

## Task 2: Repoint `ParamControl` to use the Knob

**Files:** `crates/daudio-ui/src/param_control.rs`, `crates/daudio-ui/src/theme.css`

**Step 1:** In `param_control.rs`, replace the internal `ParamSlider::new(...)` with `crate::knob::Knob::new(...)`. Keep the same `ParamControl::new` signature (label + lens + map-fn) and the label above the control. The `.daudio-control` container may want centering.

**Step 2:** In `theme.css`, add a `.daudio-knob` rule giving the knob a fixed `width`/`height` (e.g. 56px square) so it lays out as a knob, and center it under the label. Adjust `.daudio-control` alignment (`child-space`, `col-between`, `alignment`) so label+knob stack centered.

**Step 3: verify** `cargo build -p daudio-ui` + `cargo build -p filter` compile; clippy/fmt clean. **Commit** `refactor(ui): ParamControl uses the rotary Knob`

---

## Task 3: Filter editor with knobs (the proof)

**Files:** `plugins/filter/src/lib.rs`

**Step 1:** The filter editor already uses `ParamControl` for Cutoff/Gain, so it now renders knobs with no code change. Adjust ONLY the editor window size in `FilterParams::default` (`daudio_ui::editor_state(...)`) if two knobs need different dimensions (e.g. lay them in an `HStack` side by side — update the editor closure to use `HStack` if that reads better for knobs; keep the title above).

**Step 2: verify (behavior + build):**
- `cargo build -p filter` and `cargo build -p filter --bin standalone` — compile.
- `cargo test --workspace` — 17 pass (DSP/core unaffected).
- `cargo clippy --workspace -- -D warnings` — clean.
- `cargo fmt --check` — clean.
- `cargo xtask bundle filter --release` — produces both bundles.
- MANUAL (human): `cargo run -p filter --bin standalone` shows two rotary knobs (Cutoff, Gain) that respond to drag (up=increase), scroll, and double-click-reset, and that move when automated by the host.

**Step 3: commit** `feat(filter): use rotary knobs in the editor`

---

## Definition of Done

- `daudio-ui` exports a `Knob` built on `ParamWidgetBase`; `ParamControl` uses it.
- `cargo test --workspace` green (17); clippy `-D warnings` clean; fmt clean; filter bundles.
- Filter editor shows working rotary knobs (human-verified via standalone): drag, scroll, double-click reset, host-automation tracking.

## Follow-up (not this plan)

- Wire knob colors to the theme (CSS custom properties / accent) — reintroduce the accent constant, wired up.
- Value tooltip / text entry on the knob; fine-drag with modifier key.
- Meter, Toggle, ComboBox widgets; a second plugin.
