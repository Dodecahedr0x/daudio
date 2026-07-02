# daudio First Plugin & DSP Foundation — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Stand up the Cargo workspace, a tested pure-Rust DSP foundation, and a first working filter/gain VST3+CLAP plugin that also runs as a standalone app — proving the SDK's core shape end-to-end.

**Architecture:** All signal processing lives in a pure, host-agnostic `daudio-dsp` crate (fully unit-testable, no nih-plug dependency). Each plugin's processing is a plain, testable struct built from those primitives; the nih-plug `Plugin` impl is a thin adapter that pumps params into that struct. We deliberately do NOT build the `daudio_plugin!` proc-macro or the `daudio-ui`/`daudio-sdk` abstractions yet — those get extracted in a follow-up plan once the first plugin reveals the real boilerplate (YAGNI).

**Tech Stack:** Rust (nightly), nih-plug (VST3 + CLAP export, git dependency), `cargo nextest` for tests, `nih_plug_xtask` for bundling. GUI deferred — this plan ships with nih-plug's generic auto-editor.

**Reference skills:** Use superpowers:test-driven-development for every DSP task. Use rs-check (cargo fmt/clippy/nextest) after each task before committing.

---

## Scope of THIS plan

In scope: workspace, `daudio-dsp` (gain utils, `Processor` trait, one-pole smoother, biquad lowpass), `plugins/filter` (a testable `FilterCore` + a nih-plug adapter exporting VST3/CLAP + a standalone binary).

Out of scope (later plans): `daudio-ui` widget library, `daudio-sdk` trait + `daudio_plugin!` macro, `TestHost` for full nih-plug plugins, synth/voice management, additional plugins, code signing.

---

## Task 1: Workspace skeleton + `daudio-dsp` crate

**Files:**
- Create: `Cargo.toml` (workspace root)
- Create: `crates/daudio-dsp/Cargo.toml`
- Create: `crates/daudio-dsp/src/lib.rs`
- Create: `rust-toolchain.toml`

**Step 1: Pin the toolchain**

`rust-toolchain.toml`:
```toml
[toolchain]
channel = "nightly"
```

**Step 2: Workspace root `Cargo.toml`**

```toml
[workspace]
resolver = "2"
members = ["crates/daudio-dsp"]

[workspace.package]
edition = "2021"
license = "MIT"

[profile.release]
lto = "thin"
```

**Step 3: `crates/daudio-dsp/Cargo.toml`**

```toml
[package]
name = "daudio-dsp"
version = "0.1.0"
edition.workspace = true
license.workspace = true

[dependencies]
```

**Step 4: Minimal `lib.rs`**

```rust
//! daudio-dsp: pure, host-agnostic DSP primitives.

#[cfg(test)]
mod smoke {
    #[test]
    fn workspace_builds() {
        assert_eq!(2 + 2, 4);
    }
}
```

**Step 5: Verify it builds and tests pass**

Run: `cargo nextest run -p daudio-dsp`
Expected: 1 test passes. (If `nextest` is unavailable, `cargo test -p daudio-dsp`.)

**Step 6: Commit**

```bash
git add -A
git commit -m "feat(dsp): scaffold workspace and daudio-dsp crate"
```

---

## Task 2: Gain / dB utilities

**Files:**
- Create: `crates/daudio-dsp/src/gain.rs`
- Modify: `crates/daudio-dsp/src/lib.rs` (add `pub mod gain;`)

**Step 1: Write failing tests** in `crates/daudio-dsp/src/gain.rs`:

```rust
//! Decibel <-> linear gain conversions.

/// Convert decibels to a linear amplitude factor. 0 dB -> 1.0, -6 dB ~ 0.5.
pub fn db_to_gain(db: f32) -> f32 {
    unimplemented!()
}

/// Convert a linear amplitude factor to decibels. Returns f32::NEG_INFINITY at 0.
pub fn gain_to_db(gain: f32) -> f32 {
    unimplemented!()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f32, b: f32, eps: f32) {
        assert!((a - b).abs() < eps, "{a} !~ {b}");
    }

    #[test]
    fn zero_db_is_unity() {
        approx(db_to_gain(0.0), 1.0, 1e-6);
    }

    #[test]
    fn minus_six_db_is_about_half() {
        approx(db_to_gain(-6.0), 0.5012, 1e-3);
    }

    #[test]
    fn roundtrip() {
        approx(gain_to_db(db_to_gain(-12.0)), -12.0, 1e-3);
    }

    #[test]
    fn zero_gain_is_neg_inf_db() {
        assert_eq!(gain_to_db(0.0), f32::NEG_INFINITY);
    }
}
```

**Step 2: Run tests, verify they fail**

Run: `cargo nextest run -p daudio-dsp gain`
Expected: FAIL (`unimplemented!`).

**Step 3: Implement**

```rust
pub fn db_to_gain(db: f32) -> f32 {
    10.0_f32.powf(db / 20.0)
}

pub fn gain_to_db(gain: f32) -> f32 {
    if gain <= 0.0 {
        f32::NEG_INFINITY
    } else {
        20.0 * gain.log10()
    }
}
```

Add to `lib.rs`: `pub mod gain;`

**Step 4: Run tests, verify pass**

Run: `cargo nextest run -p daudio-dsp gain`
Expected: PASS (4 tests).

**Step 5: rs-check + commit**

```bash
cargo fmt && cargo clippy -p daudio-dsp -- -D warnings
git add -A && git commit -m "feat(dsp): add db/gain conversion utilities"
```

---

## Task 3: `Processor` trait

**Files:**
- Create: `crates/daudio-dsp/src/processor.rs`
- Modify: `crates/daudio-dsp/src/lib.rs` (add `pub mod processor;`)

**Step 1: Write the trait + a trivial test double**

```rust
//! The uniform per-sample processing contract for DSP blocks.

pub trait Processor {
    /// Called on init and whenever the host sample rate changes.
    fn set_sample_rate(&mut self, sample_rate: f32);
    /// Clear internal state (delay lines, filter memory, etc.).
    fn reset(&mut self);
    /// Process one input sample, returning one output sample.
    fn process_sample(&mut self, input: f32) -> f32;
    /// Process a block in place. Default loops over `process_sample`;
    /// override for SIMD/block efficiency.
    fn process_block(&mut self, buf: &mut [f32]) {
        for s in buf.iter_mut() {
            *s = self.process_sample(*s);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct AddOne;
    impl Processor for AddOne {
        fn set_sample_rate(&mut self, _sr: f32) {}
        fn reset(&mut self) {}
        fn process_sample(&mut self, input: f32) -> f32 {
            input + 1.0
        }
    }

    #[test]
    fn default_block_uses_process_sample() {
        let mut p = AddOne;
        let mut buf = [0.0, 1.0, 2.0];
        p.process_block(&mut buf);
        assert_eq!(buf, [1.0, 2.0, 3.0]);
    }
}
```

**Step 2: Run test, verify pass**

Run: `cargo nextest run -p daudio-dsp processor`
Expected: PASS. (No red step here — this task defines a contract; the test guards the default `process_block`.)

**Step 3: rs-check + commit**

```bash
cargo fmt && cargo clippy -p daudio-dsp -- -D warnings
git add -A && git commit -m "feat(dsp): add Processor trait with default block processing"
```

---

## Task 4: One-pole parameter smoother

**Files:**
- Create: `crates/daudio-dsp/src/smoother.rs`
- Modify: `crates/daudio-dsp/src/lib.rs` (add `pub mod smoother;`)

**Step 1: Write failing tests**

```rust
//! One-pole exponential smoother for click-free parameter changes.

pub struct OnePole {
    coeff: f32,
    state: f32,
    time_ms: f32,
    sample_rate: f32,
}

impl OnePole {
    /// `time_ms` is the ~63% settling time toward a new target.
    pub fn new(time_ms: f32) -> Self {
        unimplemented!()
    }
    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        unimplemented!()
    }
    /// Set the current value immediately (no smoothing).
    pub fn snap_to(&mut self, value: f32) {
        unimplemented!()
    }
    /// Advance one sample toward `target`, returning the smoothed value.
    pub fn next(&mut self, target: f32) -> f32 {
        unimplemented!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snap_sets_value() {
        let mut s = OnePole::new(10.0);
        s.set_sample_rate(48_000.0);
        s.snap_to(0.7);
        assert!((s.next(0.7) - 0.7).abs() < 1e-6);
    }

    #[test]
    fn converges_toward_target() {
        let mut s = OnePole::new(5.0);
        s.set_sample_rate(48_000.0);
        s.snap_to(0.0);
        let mut v = 0.0;
        for _ in 0..48_000 {
            v = s.next(1.0);
        }
        assert!((v - 1.0).abs() < 1e-3, "did not converge: {v}");
    }

    #[test]
    fn moves_gradually_not_instantly() {
        let mut s = OnePole::new(50.0);
        s.set_sample_rate(48_000.0);
        s.snap_to(0.0);
        let first = s.next(1.0);
        assert!(first > 0.0 && first < 0.5, "should be partway: {first}");
    }
}
```

**Step 2: Run tests, verify fail**

Run: `cargo nextest run -p daudio-dsp smoother`
Expected: FAIL (`unimplemented!`).

**Step 3: Implement**

```rust
impl OnePole {
    pub fn new(time_ms: f32) -> Self {
        let mut s = Self { coeff: 0.0, state: 0.0, time_ms, sample_rate: 48_000.0 };
        s.recompute();
        s
    }

    fn recompute(&mut self) {
        // coeff = exp(-1 / (time_seconds * sample_rate))
        let t = (self.time_ms / 1000.0).max(1e-6);
        self.coeff = (-1.0 / (t * self.sample_rate)).exp();
    }

    pub fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate;
        self.recompute();
    }

    pub fn snap_to(&mut self, value: f32) {
        self.state = value;
    }

    pub fn next(&mut self, target: f32) -> f32 {
        self.state = target + self.coeff * (self.state - target);
        self.state
    }
}
```

**Step 4: Run tests, verify pass**

Run: `cargo nextest run -p daudio-dsp smoother`
Expected: PASS (3 tests).

**Step 5: rs-check + commit**

```bash
cargo fmt && cargo clippy -p daudio-dsp -- -D warnings
git add -A && git commit -m "feat(dsp): add one-pole parameter smoother"
```

---

## Task 5: Biquad lowpass filter

**Files:**
- Create: `crates/daudio-dsp/src/biquad.rs`
- Modify: `crates/daudio-dsp/src/lib.rs` (add `pub mod biquad;`)

**Step 1: Write failing tests** (verify DC gain ~= 1 for a lowpass, and high-frequency attenuation)

```rust
//! Transposed Direct Form II biquad, RBJ cookbook lowpass coefficients.

use crate::processor::Processor;

pub struct BiquadLowpass {
    sample_rate: f32,
    cutoff_hz: f32,
    q: f32,
    // coefficients
    b0: f32, b1: f32, b2: f32, a1: f32, a2: f32,
    // state (TDF-II)
    z1: f32, z2: f32,
}

impl BiquadLowpass {
    pub fn new(cutoff_hz: f32, q: f32) -> Self {
        unimplemented!()
    }
    pub fn set_cutoff(&mut self, cutoff_hz: f32) {
        unimplemented!()
    }
    fn recompute(&mut self) {
        unimplemented!()
    }
}

impl Processor for BiquadLowpass {
    fn set_sample_rate(&mut self, sample_rate: f32) { unimplemented!() }
    fn reset(&mut self) { unimplemented!() }
    fn process_sample(&mut self, input: f32) -> f32 { unimplemented!() }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Feed a DC signal; a lowpass should pass it ~unchanged (gain ~1).
    #[test]
    fn passes_dc() {
        let mut f = BiquadLowpass::new(1000.0, 0.707);
        f.set_sample_rate(48_000.0);
        let mut out = 0.0;
        for _ in 0..10_000 {
            out = f.process_sample(1.0);
        }
        assert!((out - 1.0).abs() < 1e-2, "DC gain off: {out}");
    }

    /// Feed Nyquist-ish alternating signal; lowpass should attenuate it heavily.
    #[test]
    fn attenuates_high_freq() {
        let mut f = BiquadLowpass::new(1000.0, 0.707);
        f.set_sample_rate(48_000.0);
        let mut peak = 0.0f32;
        for n in 0..10_000 {
            let x = if n % 2 == 0 { 1.0 } else { -1.0 };
            let y = f.process_sample(x);
            if n > 5_000 {
                peak = peak.max(y.abs());
            }
        }
        assert!(peak < 0.1, "high freq not attenuated: {peak}");
    }

    #[test]
    fn reset_clears_state() {
        let mut f = BiquadLowpass::new(1000.0, 0.707);
        f.set_sample_rate(48_000.0);
        for _ in 0..100 { f.process_sample(1.0); }
        f.reset();
        // first output after reset with 0 input should be 0
        assert!(f.process_sample(0.0).abs() < 1e-6);
    }
}
```

**Step 2: Run tests, verify fail**

Run: `cargo nextest run -p daudio-dsp biquad`
Expected: FAIL.

**Step 3: Implement** (RBJ lowpass, transposed direct form II)

```rust
use std::f32::consts::PI;

impl BiquadLowpass {
    pub fn new(cutoff_hz: f32, q: f32) -> Self {
        let mut f = Self {
            sample_rate: 48_000.0, cutoff_hz, q,
            b0: 1.0, b1: 0.0, b2: 0.0, a1: 0.0, a2: 0.0,
            z1: 0.0, z2: 0.0,
        };
        f.recompute();
        f
    }

    pub fn set_cutoff(&mut self, cutoff_hz: f32) {
        self.cutoff_hz = cutoff_hz;
        self.recompute();
    }

    fn recompute(&mut self) {
        let cutoff = self.cutoff_hz.clamp(10.0, self.sample_rate * 0.49);
        let w0 = 2.0 * PI * cutoff / self.sample_rate;
        let (sin_w0, cos_w0) = w0.sin_cos();
        let alpha = sin_w0 / (2.0 * self.q);

        let b0 = (1.0 - cos_w0) / 2.0;
        let b1 = 1.0 - cos_w0;
        let b2 = (1.0 - cos_w0) / 2.0;
        let a0 = 1.0 + alpha;
        let a1 = -2.0 * cos_w0;
        let a2 = 1.0 - alpha;

        self.b0 = b0 / a0;
        self.b1 = b1 / a0;
        self.b2 = b2 / a0;
        self.a1 = a1 / a0;
        self.a2 = a2 / a0;
    }
}

impl Processor for BiquadLowpass {
    fn set_sample_rate(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate;
        self.recompute();
    }

    fn reset(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }

    fn process_sample(&mut self, input: f32) -> f32 {
        // Transposed Direct Form II
        let y = self.b0 * input + self.z1;
        self.z1 = self.b1 * input - self.a1 * y + self.z2;
        self.z2 = self.b2 * input - self.a2 * y;
        y
    }
}
```

**Step 4: Run tests, verify pass**

Run: `cargo nextest run -p daudio-dsp biquad`
Expected: PASS (3 tests).

**Step 5: rs-check + commit**

```bash
cargo fmt && cargo clippy -p daudio-dsp -- -D warnings
git add -A && git commit -m "feat(dsp): add RBJ biquad lowpass filter"
```

---

## Task 6: `FilterCore` — the plugin's testable processing struct

This is the plugin's DSP brain, independent of nih-plug so it can be unit-tested
directly (our "offline test" story for now). It owns two channels of filter +
a smoothed output gain.

**Files:**
- Create: `plugins/filter/Cargo.toml`
- Create: `plugins/filter/src/core.rs`
- Create: `plugins/filter/src/lib.rs`
- Modify: root `Cargo.toml` (add `"plugins/filter"` to members)

**Step 1: `plugins/filter/Cargo.toml`**

```toml
[package]
name = "filter"
version = "0.1.0"
edition.workspace = true
license.workspace = true

[lib]
crate-type = ["cdylib", "lib"]

[dependencies]
daudio-dsp = { path = "../../crates/daudio-dsp" }
nih_plug = { git = "https://github.com/robbert-vdh/nih-plug.git" }

[dependencies.nih_plug_xtask]
git = "https://github.com/robbert-vdh/nih-plug.git"
optional = true
```

> NOTE: nih-plug is a git dependency (not published to crates.io). Pin to a
> specific rev once building, e.g. `rev = "..."`, to keep builds reproducible.

**Step 2: Write failing tests** in `plugins/filter/src/core.rs`:

```rust
use daudio_dsp::biquad::BiquadLowpass;
use daudio_dsp::gain::db_to_gain;
use daudio_dsp::processor::Processor;
use daudio_dsp::smoother::OnePole;

/// Host-agnostic stereo processing core: lowpass per channel + smoothed gain.
pub struct FilterCore {
    left: BiquadLowpass,
    right: BiquadLowpass,
    gain: OnePole,
}

impl FilterCore {
    pub fn new() -> Self {
        Self {
            left: BiquadLowpass::new(1000.0, 0.707),
            right: BiquadLowpass::new(1000.0, 0.707),
            gain: OnePole::new(20.0),
        }
    }

    pub fn set_sample_rate(&mut self, sr: f32) {
        self.left.set_sample_rate(sr);
        self.right.set_sample_rate(sr);
        self.gain.set_sample_rate(sr);
    }

    pub fn set_cutoff(&mut self, hz: f32) {
        self.left.set_cutoff(hz);
        self.right.set_cutoff(hz);
    }

    pub fn reset(&mut self) {
        self.left.reset();
        self.right.reset();
    }

    /// Process one stereo frame given a target gain in dB.
    pub fn process_frame(&mut self, l: f32, r: f32, gain_db: f32) -> (f32, f32) {
        let g = self.gain.next(db_to_gain(gain_db));
        (self.left.process_sample(l) * g, self.right.process_sample(r) * g)
    }
}

impl Default for FilterCore {
    fn default() -> Self { Self::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unity_gain_passes_dc() {
        let mut c = FilterCore::new();
        c.set_sample_rate(48_000.0);
        c.gain.snap_to(db_to_gain(0.0));
        let mut out = (0.0, 0.0);
        for _ in 0..10_000 {
            out = c.process_frame(1.0, 1.0, 0.0);
        }
        assert!((out.0 - 1.0).abs() < 1e-2);
    }

    #[test]
    fn minus_six_db_halves_amplitude() {
        let mut c = FilterCore::new();
        c.set_sample_rate(48_000.0);
        c.gain.snap_to(db_to_gain(-6.0));
        let mut out = (0.0, 0.0);
        for _ in 0..10_000 {
            out = c.process_frame(1.0, 1.0, -6.0);
        }
        assert!((out.0 - 0.5012).abs() < 1e-2, "got {}", out.0);
    }
}
```

> The test reaches `c.gain` directly, so mark the field `pub(crate)` or add a
> `snap_gain(&mut self, db: f32)` helper — prefer the helper (cleaner API). Add:
> ```rust
> pub fn snap_gain(&mut self, gain_db: f32) { self.gain.snap_to(db_to_gain(gain_db)); }
> ```
> and use `c.snap_gain(0.0)` / `c.snap_gain(-6.0)` in the tests.

**Step 3: Run tests, verify fail then pass** as you fill in the helper.

Run: `cargo nextest run -p filter`
Expected: PASS (2 tests).

**Step 4: `plugins/filter/src/lib.rs`** (declare the module for now)

```rust
pub mod core;
```

**Step 5: rs-check + commit**

```bash
cargo fmt && cargo clippy -p filter -- -D warnings
git add -A && git commit -m "feat(filter): add testable FilterCore processing struct"
```

---

## Task 7: nih-plug adapter — params + `Plugin` impl + format exports

Wire `FilterCore` into a real plugin. There's no clean unit test for the
nih-plug glue itself; the acceptance check is "it builds and runs standalone"
(Task 8). Keep this adapter as thin as possible.

**Files:**
- Modify: `plugins/filter/src/lib.rs`

**Step 1: Params + Plugin impl.** Replace `lib.rs` with the adapter below.

```rust
pub mod core;

use crate::core::FilterCore;
use nih_plug::prelude::*;
use std::sync::Arc;

#[derive(Params)]
struct FilterParams {
    #[id = "cutoff"]
    cutoff: FloatParam,
    #[id = "gain"]
    gain: FloatParam,
}

impl Default for FilterParams {
    fn default() -> Self {
        Self {
            cutoff: FloatParam::new(
                "Cutoff",
                1000.0,
                FloatRange::Skewed {
                    min: 20.0,
                    max: 20_000.0,
                    factor: FloatRange::skew_factor(-2.0),
                },
            )
            .with_unit(" Hz")
            .with_value_to_string(formatters::v2s_f32_hz_then_khz(1))
            .with_string_to_value(formatters::s2v_f32_hz_then_khz()),
            gain: FloatParam::new(
                "Gain",
                0.0,
                FloatRange::Linear { min: -60.0, max: 6.0 },
            )
            .with_unit(" dB")
            .with_smoother(SmoothingStyle::Linear(20.0)),
        }
    }
}

struct FilterPlugin {
    params: Arc<FilterParams>,
    core: FilterCore,
}

impl Default for FilterPlugin {
    fn default() -> Self {
        Self { params: Arc::new(FilterParams::default()), core: FilterCore::new() }
    }
}

impl Plugin for FilterPlugin {
    const NAME: &'static str = "daudio Filter";
    const VENDOR: &'static str = "daudio";
    const URL: &'static str = "https://example.com";
    const EMAIL: &'static str = "hexadecifish@gmail.com";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: NonZeroU32::new(2),
        main_output_channels: NonZeroU32::new(2),
        ..AudioIOLayout::const_default()
    }];

    const SAMPLE_ACCURATE_AUTOMATION: bool = true;

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn initialize(
        &mut self,
        _layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        self.core.set_sample_rate(buffer_config.sample_rate);
        self.core.snap_gain(self.params.gain.value());
        true
    }

    fn reset(&mut self) {
        self.core.reset();
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        // Keep filter cutoff in sync (cheap; recomputes coeffs).
        self.core.set_cutoff(self.params.cutoff.value());

        for mut frame in buffer.iter_samples() {
            let gain_db = self.params.gain.smoothed.next();
            let l = *frame.get_mut(0).unwrap();
            let r = *frame.get_mut(1).unwrap();
            let (ol, or) = self.core.process_frame(l, r, gain_db);
            *frame.get_mut(0).unwrap() = ol;
            *frame.get_mut(1).unwrap() = or;
        }
        ProcessStatus::Normal
    }
}

impl ClapPlugin for FilterPlugin {
    const CLAP_ID: &'static str = "com.daudio.filter";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("A simple lowpass filter with gain");
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] =
        &[ClapFeature::AudioEffect, ClapFeature::Stereo, ClapFeature::Filter];
}

impl Vst3Plugin for FilterPlugin {
    const VST3_CLASS_ID: [u8; 16] = *b"daudioFilter0001";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Filter];
}

nih_export_clap!(FilterPlugin);
nih_export_vst3!(FilterPlugin);
```

> NOTE: nih-plug's API occasionally shifts. If any symbol/signature above does
> not compile against the pinned rev, consult the nih-plug docs/examples
> (`plugins/examples/gain` in the nih-plug repo) and adjust — the shape is
> correct; exact names may need a tweak. `NonZeroU32` comes from
> `nih_plug::prelude`.

**Step 2: Build**

Run: `cargo build -p filter`
Expected: compiles to a `cdylib`. Fix any API drift per the note above.

**Step 3: Confirm the core tests still pass**

Run: `cargo nextest run -p filter`
Expected: PASS (the Task 6 tests still green).

**Step 4: rs-check + commit**

```bash
cargo fmt && cargo clippy -p filter -- -D warnings
git add -A && git commit -m "feat(filter): add nih-plug adapter with VST3+CLAP export"
```

---

## Task 8: Standalone binary — the "no DAW" preview loop

**Files:**
- Create: `plugins/filter/src/bin/standalone.rs`
- Modify: `plugins/filter/Cargo.toml` (enable the standalone feature)

**Step 1: Enable the standalone runner** in `Cargo.toml`:

```toml
nih_plug = { git = "https://github.com/robbert-vdh/nih-plug.git", features = ["standalone"] }
```

**Step 2: `plugins/filter/src/bin/standalone.rs`**

```rust
use nih_plug::prelude::*;

// Re-import the plugin type from the library crate.
use filter::FilterPlugin;

fn main() {
    nih_export_standalone::<FilterPlugin>();
}
```

> This requires `FilterPlugin` to be `pub` in `lib.rs`. Change
> `struct FilterPlugin` to `pub struct FilterPlugin` (and ensure the crate name
> `filter` matches the `[package] name`).

**Step 3: Run it**

Run: `cargo run -p filter --bin standalone`
Expected: a window opens (nih-plug's generic parameter editor) and the plugin
processes live audio from your default input to output. Adjust Cutoff/Gain and
confirm audible change. (On macOS, grant mic permission if prompted; use
headphones to avoid feedback.)

**Step 4: Commit**

```bash
git add -A && git commit -m "feat(filter): add standalone preview binary"
```

---

## Task 9: Bundling via xtask (produce installable VST3/CLAP)

**Files:**
- Create: `xtask/Cargo.toml`
- Create: `xtask/src/main.rs`
- Create: `.cargo/config.toml`
- Modify: root `Cargo.toml` (add `"xtask"` to members)

**Step 1: `xtask/Cargo.toml`**

```toml
[package]
name = "xtask"
version = "0.1.0"
edition.workspace = true

[dependencies]
nih_plug_xtask = { git = "https://github.com/robbert-vdh/nih-plug.git" }
```

**Step 2: `xtask/src/main.rs`**

```rust
fn main() -> nih_plug_xtask::Result<()> {
    nih_plug_xtask::main()
}
```

**Step 3: `.cargo/config.toml`** (so `cargo xtask` aliases to the xtask bin)

```toml
[alias]
xtask = "run --package xtask --release --"
```

**Step 4: Bundle**

Run: `cargo xtask bundle filter --release`
Expected: `target/bundled/daudio Filter.vst3` and `... .clap` are produced.

**Step 5: (macOS) smoke-test in a DAW (manual)**

Copy the `.clap`/`.vst3` to the user plugin folder and confirm a DAW scans and
loads it. Document any signing prompts (deferred work).

**Step 6: Commit**

```bash
git add -A && git commit -m "build: add xtask bundler for VST3/CLAP artifacts"
```

---

## Definition of Done

- `cargo nextest run` is green across the workspace (dsp + filter core).
- `cargo clippy --workspace -- -D warnings` is clean.
- `cargo run -p filter --bin standalone` opens and audibly filters live audio.
- `cargo xtask bundle filter --release` produces `.vst3` and `.clap` bundles.

## Follow-up plans (not this plan)

1. Extract `daudio-sdk` (`DaudioPlugin` trait + `daudio_plugin!` macro) once a
   second plugin makes the Task 7 boilerplate a concrete DRY target.
2. Build `daudio-ui` (Vizia `Knob`/`ParamControl`/theme) and give the filter a
   custom editor.
3. Build the full-plugin offline `TestHost` (constructing nih-plug `Buffer`s).
4. Add the second plugin type (compressor or synth) to stress voice mgmt/MIDI.
