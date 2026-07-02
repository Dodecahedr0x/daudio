# daudio-ui

Shared Vizia UI toolkit for daudio plugin editors: a modern dark theme, custom widgets, and
layout helpers. Built on `nih_plug_vizia` (backend: `vizia_baseview`).

## Contents

- Widgets: `Knob` (rotary, accent glow arc, hover), `Meter` (gradient level bar), `NoteToggle`
  (pill toggle with a root-reactive note-name label), `ParamControl` (caption over a `Knob`).
- `create_editor` / `editor_state` / `DaudioData` — hide the `create_vizia_editor` + Lens/Model boilerplate.
- `card` / `card_column` — titled, bordered group containers.
- Theme: `apply_theme`, an embedded `theme.css`, and a `vg::Color` palette (`ACCENT`, `SURFACE`, …).

## Gotchas that shaped this crate (read before editing)

- **Vizia timers never tick under `vizia_baseview`.** Don't use them for reactivity. baseview
  redraws every frame, so a `draw()` leaf that reads an atomic updates live (see `Meter`).
- **CSS layout properties don't nest reliably** (`child-space`, `row-between`, `col-between`).
  Drive layout with **inline modifiers** (see `card`); use CSS for decoration only.
- A plain `.map` over `Arc<Params>` isn't reactive — use `ParamWidgetBase::make_lens` for a
  param-backed lens (that's how `NoteToggle`'s label and value stay live).
- Canvas leaf widgets have no intrinsic size — set inline defaults in `new()`.
- Canvas widgets must `cx.needs_redraw()` on value change and on `RawParamEvent` to track automation.
