# daudio-sdk Extraction — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Extract a reusable `daudio-sdk` (a `DaudioEffect` trait + param helpers + a `#[daudio_plugin(...)]` proc-macro) that collapses the nih-plug export boilerplate, and prove it by refactoring `plugins/filter` onto it with zero behavior change.

**Architecture:** Two new crates. `daudio-sdk-macros` is a proc-macro crate (syn/quote/proc-macro2) exporting one attribute macro `#[daudio_plugin(...)]` that generates the `impl Plugin` / `impl ClapPlugin` / `impl Vst3Plugin` blocks plus `nih_export_clap!` / `nih_export_vst3!`. `daudio-sdk` is the author-facing facade: it defines the `DaudioEffect` trait and param helpers, and re-exports `daudio-dsp`, `nih_plug`, and the macro so a plugin depends on ONE crate. The macro delegates all unique behavior to the author's `DaudioEffect` impl.

**Tech Stack:** Rust nightly, nih-plug (pinned rev `f36931f7af4646065488a9845d8f8c2f95252c23`), syn 2 / quote / proc-macro2.

**Reference skills:** superpowers:test-driven-development for the param helpers; rs-check after each task.

**KEY DE-RISKER:** `plugins/filter/src/lib.rs` at HEAD already contains a WORKING, compiling set of nih-plug impls against the pinned rev. The macro's generated code must match that exact structure with metadata substituted. Read that file as the authoritative codegen target before writing the macro.

---

## Scope

In scope: `daudio-sdk-macros` (attribute macro), `daudio-sdk` (`DaudioEffect` trait + `db_gain_param`/`hz_param` helpers + re-exports), and refactoring `plugins/filter` onto the SDK.

Out of scope (later plans): `DaudioSynth`/`DaudioMidi` traits, `daudio-ui`, `TestHost`, mono/multichannel layouts, editor generation. YAGNI — build only what the filter needs.

---

## The target authoring experience (what filter becomes)

```rust
use daudio_sdk::prelude::*;
mod dsp;
use crate::dsp::FilterCore;

#[derive(Params)]
struct FilterParams { /* unchanged: cutoff + gain */ }
impl Default for FilterParams { /* uses daudio_sdk::hz_param / db_gain_param */ }

#[daudio_plugin(
    name = "daudio Filter",
    vendor = "daudio",
    url = "https://example.com",
    email = "hexadecifish@gmail.com",
    clap_id = "com.daudio.filter",
    clap_description = "A simple lowpass filter with gain",
    vst3_id = "daudioFilter0001",
    clap_features = [AudioEffect, Stereo, Filter],
    vst3_categories = [Fx, Filter],
)]
struct FilterPlugin {
    params: Arc<FilterParams>,
    core: FilterCore,
}

impl DaudioEffect for FilterPlugin {
    type Params = FilterParams;
    fn activate(&mut self, sample_rate: f32) {
        self.core.set_sample_rate(sample_rate);
        self.core.snap_gain(self.params.gain.value());
    }
    fn reset(&mut self) {
        self.core.reset();
        self.core.snap_gain(self.params.gain.value());
    }
    fn pre_block(&mut self) {
        self.core.set_cutoff(self.params.cutoff.value());
    }
    fn process_frame(&mut self, l: f32, r: f32) -> (f32, f32) {
        let gain_db = self.params.gain.value();
        self.core.process_frame(l, r, gain_db)
    }
}
```

The macro assumes the struct has a field named `params: Arc<Self::Params>`.

---

## Task 1: Scaffold `daudio-sdk-macros` and `daudio-sdk` crates

**Files:**
- Create: `crates/daudio-sdk-macros/Cargo.toml`, `crates/daudio-sdk-macros/src/lib.rs`
- Create: `crates/daudio-sdk/Cargo.toml`, `crates/daudio-sdk/src/lib.rs`
- Modify: root `Cargo.toml` members.

**Step 1: `daudio-sdk-macros/Cargo.toml`**
```toml
[package]
name = "daudio-sdk-macros"
version = "0.1.0"
edition.workspace = true
license.workspace = true

[lib]
proc-macro = true

[dependencies]
syn = { version = "2", features = ["full"] }
quote = "1"
proc-macro2 = "1"
```

**Step 2: passthrough macro** in `daudio-sdk-macros/src/lib.rs`:
```rust
use proc_macro::TokenStream;

/// Attribute macro that generates nih-plug Plugin/ClapPlugin/Vst3Plugin impls
/// and format exports for a struct implementing `daudio_sdk::DaudioEffect`.
#[proc_macro_attribute]
pub fn daudio_plugin(_attr: TokenStream, item: TokenStream) -> TokenStream {
    // Task 3 fills this in. For now, passthrough so the crate compiles.
    item
}
```

**Step 3: `daudio-sdk/Cargo.toml`**
```toml
[package]
name = "daudio-sdk"
version = "0.1.0"
edition.workspace = true
license.workspace = true

[dependencies]
daudio-dsp = { path = "../daudio-dsp" }
daudio-sdk-macros = { path = "../daudio-sdk-macros" }
nih_plug = { git = "https://github.com/robbert-vdh/nih-plug.git", rev = "f36931f7af4646065488a9845d8f8c2f95252c23" }
```

**Step 4: `daudio-sdk/src/lib.rs`** (minimal, compiles):
```rust
//! daudio-sdk: author-facing facade for building daudio plugins.

pub use daudio_dsp;
pub use daudio_sdk_macros::daudio_plugin;
pub use nih_plug;
```

**Step 5:** add `"crates/daudio-sdk-macros"` and `"crates/daudio-sdk"` to root `Cargo.toml` members.

**Step 6: verify** `cargo build -p daudio-sdk` compiles. **Commit:** `feat(sdk): scaffold daudio-sdk and daudio-sdk-macros crates`

---

## Task 2: `DaudioEffect` trait + param helpers (TDD the helpers)

**Files:**
- Create: `crates/daudio-sdk/src/effect.rs`, `crates/daudio-sdk/src/params.rs`
- Modify: `crates/daudio-sdk/src/lib.rs` (modules + `prelude`)

**Step 1: `effect.rs` — the trait**
```rust
use nih_plug::prelude::*;

/// A stereo audio effect. Implement this and annotate the struct with
/// `#[daudio_plugin(...)]` to get a full nih-plug VST3+CLAP plugin.
///
/// The annotated struct MUST have a field `params: std::sync::Arc<Self::Params>`.
pub trait DaudioEffect: Send {
    type Params: Params + Default;

    /// Called from `Plugin::initialize`: set sample rate, snap smoothers.
    fn activate(&mut self, sample_rate: f32);

    /// Called from `Plugin::reset`. Default: no-op.
    fn reset(&mut self) {}

    /// Called once at the start of each `process` block, before the sample loop.
    /// Use for per-block work like recomputing filter coefficients. Default: no-op.
    fn pre_block(&mut self) {}

    /// Process one stereo frame. Called per sample.
    fn process_frame(&mut self, left: f32, right: f32) -> (f32, f32);
}
```

**Step 2: `params.rs` — write failing tests first**, then implement:
```rust
use nih_plug::prelude::*;

/// A gain parameter in dB with linear smoothing.
pub fn db_gain_param(name: impl Into<String>, min_db: f32, max_db: f32, default_db: f32) -> FloatParam {
    FloatParam::new(name.into(), default_db, FloatRange::Linear { min: min_db, max: max_db })
        .with_unit(" dB")
        .with_smoother(SmoothingStyle::Linear(20.0))
}

/// A frequency parameter in Hz with a perceptual (skewed) range and Hz/kHz display.
pub fn hz_param(name: impl Into<String>, default_hz: f32, min_hz: f32, max_hz: f32) -> FloatParam {
    FloatParam::new(
        name.into(),
        default_hz,
        FloatRange::Skewed { min: min_hz, max: max_hz, factor: FloatRange::skew_factor(-2.0) },
    )
    .with_unit(" Hz")
    .with_value_to_string(formatters::v2s_f32_hz_then_khz(1))
    .with_string_to_value(formatters::s2v_f32_hz_then_khz())
}

#[cfg(test)]
mod tests {
    use super::*;
    use nih_plug::prelude::Param;

    #[test]
    fn db_gain_defaults_and_range() {
        let p = db_gain_param("Gain", -60.0, 6.0, 0.0);
        assert_eq!(p.default_plain_value(), 0.0);
        assert_eq!(p.preview_plain(0.0), -60.0);
        assert_eq!(p.preview_plain(1.0), 6.0);
    }

    #[test]
    fn hz_default_is_set() {
        let p = hz_param("Cutoff", 1000.0, 20.0, 20_000.0);
        assert_eq!(p.default_plain_value(), 1000.0);
    }
}
```
> NOTE: the exact `Param` trait method names (`default_plain_value`, `preview_plain`) must match the pinned nih-plug rev. If they differ, consult nih-plug's `Param` trait and adjust the assertions to equivalent accessors. The intent: verify default + range endpoints.

**Step 3: `lib.rs`** — add modules + prelude:
```rust
pub mod effect;
pub mod params;

pub use effect::DaudioEffect;
pub use params::{db_gain_param, hz_param};

/// Glob-import for plugin authors.
pub mod prelude {
    pub use crate::effect::DaudioEffect;
    pub use crate::params::{db_gain_param, hz_param};
    pub use daudio_sdk_macros::daudio_plugin;
    pub use nih_plug::prelude::*;
    pub use std::sync::Arc;
}
```

**Step 4: verify** `cargo nextest run -p daudio-sdk` (2 param tests pass), clippy, fmt. **Commit:** `feat(sdk): add DaudioEffect trait and param helpers`

---

## Task 3: Implement the `#[daudio_plugin]` attribute macro

**Files:** `crates/daudio-sdk-macros/src/lib.rs` (+ optional `attrs.rs` module for parsing).

**READ FIRST:** `plugins/filter/src/lib.rs` at HEAD — the hand-written `impl Plugin`/`impl ClapPlugin`/`impl Vst3Plugin` + `nih_export_*!` there is the EXACT codegen target. The macro reproduces that structure with attribute values substituted and behavior delegated to the `DaudioEffect` impl.

**Step 1: parse the attribute args.** Accept these keys (all string literals unless noted):
`name`, `vendor`, `url`, `email`, `clap_id`, `clap_description`, `vst3_id` (exactly 16 ASCII bytes), `clap_features` (list of `ClapFeature` variant idents), `vst3_categories` (list of `Vst3SubCategory` variant idents). `url`/`email`/`clap_description` may default (`""` / `None`). Parse the struct's ident from `item`. Use `syn::parse_macro_input!` and a custom `Parse` impl or `syn::meta::parser`.

**Step 2: validate `vst3_id` is 16 bytes** at macro-expansion time; emit a `compile_error!` with a clear message if not. (This replaces the raw `*b"..."` foot-gun with a checked attribute.)

**Step 3: generate** (using `quote!`), for struct `#ident`:
- Re-emit the original struct unchanged.
- `impl Plugin for #ident` with: `NAME`/`VENDOR`/`URL`/`EMAIL` from attrs, `VERSION = env!("CARGO_PKG_VERSION")`, stereo `AUDIO_IO_LAYOUTS`, `SAMPLE_ACCURATE_AUTOMATION = true`, `type SysExMessage = ()`, `type BackgroundTask = ()`, `fn params(&self) -> Arc<dyn Params> { self.params.clone() }`, `initialize` → `<Self as DaudioEffect>::activate(self, buffer_config.sample_rate); true`, `reset` → `<Self as DaudioEffect>::reset(self)`, and `process` → call `pre_block`, then the sample loop WITH the `if frame.len() < 2 { continue; }` guard, calling `process_frame` and writing channels 0/1, returning `ProcessStatus::Normal`.
- `impl ClapPlugin` with `CLAP_ID`, `CLAP_DESCRIPTION` (Some/None), manual/support URLs, `CLAP_FEATURES` built from the `clap_features` idents as `&[ClapFeature::#variant, ...]`.
- `impl Vst3Plugin` with `VST3_CLASS_ID` from the 16-byte literal (`*b"..."`), `VST3_SUBCATEGORIES` from `vst3_categories` idents.
- `nih_plug::nih_export_clap!(#ident);` and `nih_plug::nih_export_vst3!(#ident);`

Reference the trait via fully-qualified `daudio_sdk::DaudioEffect` / nih-plug via `nih_plug::prelude::*` inside generated code so the macro works regardless of the caller's imports. (Add a `use nih_plug::prelude::*;` inside a generated `const _: () = { ... }`-style scope, OR fully-qualify — implementer's choice; must not require specific imports at the call site beyond `daudio_sdk::prelude::*`.)

**Step 4: cannot unit-test codegen easily.** The acceptance gate is Task 4 (filter compiles + runs on the macro). For this task, verify the macro crate compiles (`cargo build -p daudio-sdk-macros`) and add a doc comment. Optionally add a `trybuild` compile-fail test for the 16-byte `vst3_id` check ONLY if quick; otherwise defer.

**Step 5: commit** `feat(sdk): implement daudio_plugin attribute macro`

---

## Task 4: Refactor `plugins/filter` onto the SDK (the proof)

**Files:** `plugins/filter/Cargo.toml`, `plugins/filter/src/lib.rs`.

**Step 1: swap deps.** In `plugins/filter/Cargo.toml`, replace the direct `nih_plug` + `daudio-dsp` deps with `daudio-sdk = { path = "../../crates/daudio-sdk" }`. KEEP `nih_plug` ONLY if the standalone bin needs the `standalone` feature — in that case keep `nih_plug = { ..., features = ["standalone"] }` (the standalone runner still needs it directly), plus `daudio-sdk`. Keep `[lib] crate-type = ["cdylib", "lib"]`.

**Step 2: rewrite `lib.rs`** to the "target authoring experience" shown above: `use daudio_sdk::prelude::*;`, `FilterParams` using `hz_param`/`db_gain_param` in its `Default`, the struct annotated with `#[daudio_plugin(...)]`, and the `impl DaudioEffect for FilterPlugin`. DELETE the hand-written `impl Plugin`/`ClapPlugin`/`Vst3Plugin` and `nih_export_*!` — the macro generates them now. Keep `pub mod dsp;` and `pub struct FilterPlugin` (standalone bin needs pub).

**Step 3: verify — behavior unchanged:**
- `cargo build -p filter` — compiles (cdylib).
- `cargo build -p filter --bin standalone` — compiles.
- `cargo nextest run -p filter` — the 2 FilterCore tests pass.
- `cargo clippy --workspace -- -D warnings` — clean.
- `cargo fmt --check` — clean.
- `cargo xtask bundle filter --release` — still produces `daudio Filter.vst3` / `.clap`.

**Step 4: commit** `refactor(filter): build on daudio-sdk macro + trait`

---

## Definition of Done

- `cargo nextest run --workspace` green (dsp + sdk param helpers + filter core).
- `cargo clippy --workspace -- -D warnings` clean; `cargo fmt --check` clean.
- `plugins/filter/src/lib.rs` has NO hand-written nih-plug trait impls — only `#[daudio_plugin]` + `DaudioEffect`.
- `cargo xtask bundle filter --release` produces the same two bundles.
- The filter still builds as a standalone binary.

## Follow-up (not this plan)

- `daudio-ui` (Vizia widgets) + generated/custom editor.
- `DaudioSynth` + voice management; a synth plugin to validate it.
- `TestHost` for full-plugin offline rendering.
