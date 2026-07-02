# CLAUDE.md — daudio

A Rust **VST3/CLAP plugin suite + reusable SDK** built on [nih-plug]. Three plugins
(`filter`, `synth`, `pitch-to-midi`) share an SDK so a new plugin is mostly params +
a small trait impl. Read this before touching the code — it captures the non-obvious
things that cost the most time to discover.

## Fast facts
- **Toolchain: nightly** (`rust-toolchain.toml`). nih-plug requires it.
- **nih-plug is pinned** to rev `f36931f7af4646065488a9845d8f8c2f95252c23` (git dep, not crates.io). Use the SAME rev for any nih-plug crate. To read its source: `~/.cargo/git/checkouts/nih-plug-*/f36931f/`.
- Workspace crates: `crates/{daudio-dsp, daudio-sdk, daudio-sdk-macros, daudio-ui, daudio-preview}`, `plugins/{filter, synth, pitch-to-midi}`, `xtask`.
- Every feature has a design + step plan in `docs/plans/`. Read the relevant one first.
- Work happens on `master`. Subagents sometimes create stray branches — check `git branch` and `git merge --ff-only` them back.

## Commands
```bash
cargo test --workspace                         # or `cargo nextest run --workspace`
cargo clippy --workspace -- -D warnings        # MUST be clean (see known-warning below)
cargo fmt                                       # run before every commit
cargo build -p <plugin> --bin standalone       # GUI app (see GUI section)
cargo xtask bundle <plugin> --release          # -> target/bundled/<Name>.{vst3,clap}
```
- **Known noise to IGNORE:** clippy/build print `warning: the following packages contain code that will be rejected by a future version of Rust: block v0.1.6` — a pre-existing transitive nih-plug dep, NOT your code. Filter it: `... | grep -v "block v0.1"`.
- **rust-analyzer diagnostics are frequently STALE mid-edit** (E0432/E0560/dead_code that contradict a green build). Trust `cargo build`, not the injected diagnostics — verify before "fixing".

## Architecture — the SDK
Three trait "seams", each with a codegen mode in the `#[daudio_plugin(...)]` attribute macro
(`daudio-sdk-macros`). The macro generates all `impl Plugin/ClapPlugin/Vst3Plugin` + `nih_export_*!`
boilerplate, routing every path through `::daudio_sdk::nih_plug::` so a plugin needs only a
`daudio-sdk` dep. Effect/synth/analyzer branches are selected by attribute flags; keep the
other branches byte-for-byte unchanged when editing the macro.

| Plugin kind | Trait (`daudio-sdk`) | Macro flag | I/O |
|---|---|---|---|
| Effect | `DaudioEffect` (`process_frame`) | *(default)* | stereo in→out |
| Instrument | `DaudioSynth` (`render_frame`, MIDI in) | `midi = true` | MIDI in → stereo out |
| Analyzer | `DaudioAudioToMidi` (`process_sample`) | `midi_out = true` | stereo pass-through + MIDI out (`MidiConfig::MidiCCs`) |

- **`vst3_id` must be exactly 16 ASCII bytes** — the macro compile-errors otherwise. Count it.
- `daudio-sdk` re-exports `nih_plug`, `daudio_dsp`, param helpers (`db_gain_param`, `hz_param`), and a `prelude`.
- `daudio-dsp` is pure/host-agnostic (no nih-plug), fully unit-tested: `gain`, `biquad`, `smoother`, `oscillator`, `adsr`, `notes` (`freq_to_midi`/`note_name`/`quantize`/`bend_value`), `pitch` (`PitchTracker`).
- Recipe for a new plugin: copy an existing `plugins/*` — a `#[derive(Params)]` struct, a `#[daudio_plugin(...)]` struct, one trait impl, `src/bin/standalone.rs`, and a `bundler.toml` entry (`[<crate>] name = "Display Name"`) + workspace member.

## GUI (nih_plug_vizia) — the biggest time sinks
The plugin GUIs use `nih_plug_vizia` (Vizia + femtovg canvas), backend `vizia_baseview`.
`daudio-ui` holds the shared theme + widgets (`Knob`, `Meter`, `NoteToggle`, `ParamControl`) and
layout helpers (`card`, `card_column`). **These gotchas are load-bearing:**

1. **Vizia timers NEVER tick under `vizia_baseview`** (`process_timers` is winit-only). Any feature built on `cx.add_timer`/`start_timer` is DEAD. But baseview **redraws every frame unconditionally**, so a custom leaf `View` whose `draw()` reads an atomic each frame updates live for free (that's how `Meter` and pitch-to-midi's `NoteReadout` work). Never use a timer for reactivity.
2. **CSS layout properties don't lay out nested containers** (`child-space`, `row-between`, `col-between`, stretch `1s` on nested rows → overlapping/collapsed cards). **Drive layout with INLINE vizia modifiers** (`.child_space(Pixels(..))`, `.row_between(..)`, `.col_between(..)`, `.height(Auto)`, `.width(Auto)`) — see `daudio_ui::card`. CSS (`theme.css`) is for **decoration only** (colors, borders, radius, font-size). Text-color/size CSS *does* apply.
3. **Param reactivity:** a plain vizia `.map()` over `Arc<Params>` NEVER re-fires (the Arc pointer is stable). For a lens that tracks param changes, use `ParamWidgetBase::make_lens(params, |p| &p.field, |p| ...)` — nih-plug refreshes those on `RawParamEvent::ParametersChanged` (this is how `ParamSlider`'s value display and `NoteToggle`'s root-reactive label stay live).
4. **Custom canvas leaf widgets have NO intrinsic size** → set default inline sizes (`.width(Pixels(..)).height(Pixels(..))`) in their `new()`, or they render 0×0 (invisible).
5. **Canvas widgets must request repaint on value change:** call `cx.needs_redraw()` after every `set_normalized_value`, AND handle `RawParamEvent` (Begin/Set/End/ParametersChanged → `needs_redraw()`) so the widget tracks DAW automation instead of freezing. (`Knob` shows the pattern.)
6. Embed the stylesheet with `include_str!("theme.css")` (compile-time) — NOT vizia's `include_style!`, whose debug variant reads the file from a compile-time absolute path at runtime (fragile).
7. **`#[derive(Lens)]` reserves the struct-name lens** → a field named `root` collides; name it `root_pitch` etc.
8. Set preset/utility buttons a `.class(...)` and style them dark in CSS — vizia's default `Button` is bright white and clashes with the dark theme.
9. `apply_theme()` keeps a `vg::Color` palette in `theme.rs` in sync **by hand** with hex in `theme.css` (canvas widgets read the consts; CSS reads the hex).
10. IDE "Unknown property: child-space" CSS warnings are a generic linter false-positive — vizia accepts them.

## Seeing the GUI headlessly (how to verify UI work yourself)
You CAN screenshot the standalone and Read the PNG — do this instead of asking the user to verify visuals. The window opens **off the main viewport**, so full-screen `screencapture` shows only the desktop. Procedure:
1. Kill stragglers first: `pkill -f "target/debug/standalone"` (the `standalone` bin name is shared across plugins at `target/debug/standalone`; build the one you want LAST, or it captures a stale window).
2. In ONE shell (keep the process alive during capture): launch `target/debug/standalone &`, `sleep 8`, find the window id, `screencapture`, then `kill`.
3. Find the window id with a Swift script using `CGWindowListCopyWindowInfo` (owner name contains `standalone`, pick largest area) — `CGWindowListCreateImage` is **obsoleted on macOS 15**, so use the id with `screencapture -x -o -l<WID> out.png`, which grabs the window even off-screen.
4. `Read out.png` to view it. Iterate: edit → rebuild → recapture.
Screen-recording permission is already granted (plain `screencapture` works). Neither `/usr/bin/python3` nor homebrew python has `Quartz`, and pyobjc won't build — use Swift.

## RT-safety & threading
- Audio thread (`process`/`process_frame`/`process_sample`/`render`) must never allocate/lock. DSP primitives keep buffers preallocated; the macro's process loop guards `frame.len() < 2`.
- **`pitch-detection`'s `get_pitch` may allocate** (FFT/peak vectors). So `PitchTracker` runs detection on a **worker thread** (`rtrb` lock-free ring for samples in, `AtomicU32` bit-cast-f32 for the result out); the audio thread only pushes + reads an atomic. The `McLeodDetector` is created **inside the worker closure** so its `!Send` `Rc` internals never cross a thread boundary → no `unsafe Send` needed.
- Audio→UI telemetry uses a lock-free `PeakLevel` / `Arc<AtomicI32>` (relaxed) read by a `draw()`-leaf.

## Standalone specifics
- Run: `cargo run -p <plugin> --bin standalone`. Uses CoreAudio; **no input is connected by default** (feedback guard) — effects/analyzers get silence unless you pass `--input-device`.
- Inject a device programmatically with `nih_export_standalone_with_args::<P,_>(args)` (in the prelude). pitch-to-midi's standalone prompts interactively via `daudio_preview::choose_input_device()` and appends `--input-device`.
- **baseview + nightly crash workaround** (already in `Cargo.toml`): `[profile.dev.package.baseview] debug-assertions = false` — recent nightly's null-deref runtime check aborts on baseview messaging a transient nil `NSWindow` during window setup (benign ObjC). Keep this.

## Testing & previews
- Unit tests carry correctness: pure DSP (`daudio-dsp`), the pitch quantizer/trigger, `VoiceManager`, param helpers. GUI is verified by screenshot (above), not tests.
- `daudio-preview` offline harnesses drive a plugin's real path with zero setup:
  - `run::<E: DaudioEffect>()` — tone/WAV through an effect, live playback or offline WAV render.
  - `run_analyzer::<A: DaudioAudioToMidi>()` — WAV **or live mic** through an analyzer, prints emitted MIDI (`--list`, `--input <name>`, `<file.wav>`). Offline render is deterministic and verifiable headlessly; live/GUI is not.
  - Note: with the worker-thread detector, the *offline* analyzer feeds faster than realtime so it may emit 0 notes — expected; the realtime path (standalone/mic) is what the worker serves.

## Conventions
- Commit messages: conventional (`feat(scope):`, `fix(...)`, `refactor(...)`, `docs(...)`), end with the `Co-Authored-By: Claude ...` trailer. Commit only when asked; branch off `master` if needed.
- Development flow used here: `brainstorming` → `writing-plans` (docs/plans/) → `subagent-driven-development` with a two-stage (spec + code-quality) review per task. Those reviews caught real bugs tests missed (double-smoothing, an RT panic, a frozen-knob redraw, a wrong-ADSR synth bug, the dead-timer readout). When editing shared code (the macro, `unsafe`), get it reviewed.
