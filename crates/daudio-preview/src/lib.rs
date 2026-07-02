//! Reusable audible/offline preview harness for daudio **effects**.
//!
//! Every effect ships a tiny `src/bin/demo.rs` that just calls [`run`], giving a
//! zero-setup way to hear or render the effect without a DAW, host, or input
//! device:
//!
//! ```text
//! demo                          # play a saw through the effect (live)
//! demo <input.wav>              # play a WAV through the effect (live, looped)
//! demo <input.wav> <output.wav> # render the WAV through the effect to a file
//! ```
//!
//! The harness drives the plugin's own [`DaudioEffect`] audio path
//! (`activate` → `pre_block` → `process_frame`) at the effect's **default
//! parameter values**, so it works for any effect with no per-plugin glue.
//!
//! Only uncompressed WAV is supported (via `hound`); other formats would need a
//! decoder such as `symphonia` (not wired up yet).

use std::error::Error;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SizedSample};
use daudio_dsp::notes::note_name;
use daudio_sdk::nih_plug::prelude::NoteEvent;
use daudio_sdk::{DaudioAudioToMidi, DaudioEffect};

type Frames = Vec<(f32, f32)>;
type Result<T> = std::result::Result<T, Box<dyn Error>>;

/// Entry point for an effect's `demo` binary. Dispatches on the command line.
///
/// ```ignore
/// // plugins/<effect>/src/bin/demo.rs
/// fn main() {
///     daudio_preview::run::<my_effect::MyPlugin>();
/// }
/// ```
pub fn run<E>()
where
    E: DaudioEffect + Default + Send + 'static,
{
    let args: Vec<String> = std::env::args().skip(1).collect();
    let outcome = match args.as_slice() {
        [] => play_tone::<E>(),
        [input] => play_file::<E>(input),
        [input, output] => render_file::<E>(input, output),
        _ => {
            eprintln!("usage: demo [<input.wav> [<output.wav>]]");
            std::process::exit(2);
        }
    };
    if let Err(err) = outcome {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}

/// Entry point for an audio→MIDI analyzer's `demo` binary. Feeds a WAV through
/// the analyzer offline and prints every emitted MIDI note event.
///
/// ```ignore
/// // plugins/<analyzer>/src/bin/demo.rs
/// fn main() {
///     daudio_preview::run_analyzer::<my_analyzer::MyPlugin>();
/// }
/// ```
pub fn run_analyzer<A>()
where
    A: DaudioAudioToMidi + Default,
{
    let args: Vec<String> = std::env::args().skip(1).collect();
    let input = match args.as_slice() {
        [input, ..] => input.clone(),
        [] => {
            eprintln!("usage: demo <input.wav>");
            std::process::exit(2);
        }
    };
    if let Err(e) = analyze::<A>(&input) {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

/// Offline analysis: run every frame of `input` through the analyzer, printing
/// each emitted note event stamped with its sample-accurate time.
fn analyze<A>(input: &str) -> Result<()>
where
    A: DaudioAudioToMidi + Default,
{
    let (frames, sample_rate) = read_wav(input)?;
    let sr = sample_rate as f32;

    let mut a = A::default();
    a.activate(sr);
    a.reset();

    println!("Analyzing {input} @ {sample_rate} Hz -> MIDI:");
    let mut count = 0usize;
    for (n, &(l, r)) in frames.iter().enumerate() {
        let mono = (l + r) * 0.5;
        let t = n as f32 / sr;
        a.process_sample(mono, 0, &mut |event| match event {
            NoteEvent::NoteOn { note, velocity, .. } => {
                count += 1;
                println!(
                    "t={:.3}s  NoteOn  {} (vel {:.2})",
                    t,
                    note_name(note as i32),
                    velocity
                );
            }
            NoteEvent::NoteOff { note, .. } => {
                count += 1;
                println!("t={:.3}s  NoteOff {}", t, note_name(note as i32));
            }
            _ => {}
        });
    }
    println!("{count} event(s).");
    Ok(())
}

/// No args: play a 110 Hz saw through the effect at its default settings.
fn play_tone<E>() -> Result<()>
where
    E: DaudioEffect + Default + Send + 'static,
{
    // Resolve the device rate up front so the oscillator phase steps correctly.
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or("no default output device")?;
    let rate = device.default_output_config()?.sample_rate().0;

    let inc = 110.0 / rate as f32;
    let mut phase = 0.0f32;
    let source = move || {
        phase += inc;
        if phase >= 1.0 {
            phase -= 1.0;
        }
        let s = (2.0 * phase - 1.0) * 0.2; // saw, with headroom
        (s, s)
    };

    println!("Playing a 110 Hz saw through the effect (default settings).");
    play_live(E::default(), None, source)
}

/// One arg: stream a WAV through the effect, looping until Enter.
fn play_file<E>(input: &str) -> Result<()>
where
    E: DaudioEffect + Default + Send + 'static,
{
    let (frames, file_rate) = read_wav(input)?;
    if frames.is_empty() {
        return Err("input file has no samples".into());
    }
    println!("Playing {input} through the effect (looped) @ {file_rate} Hz.",);

    let mut idx = 0usize;
    let source = move || {
        let frame = frames[idx];
        idx = (idx + 1) % frames.len();
        frame
    };
    play_live(E::default(), Some(file_rate), source)
}

/// Two args: render the WAV through the effect to a 32-bit float WAV. Offline
/// and deterministic — no audio device involved.
fn render_file<E>(input: &str, output: &str) -> Result<()>
where
    E: DaudioEffect + Default,
{
    let (frames, file_rate) = read_wav(input)?;
    let mut effect = E::default();
    effect.activate(file_rate as f32);
    effect.reset();

    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: file_rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut writer = hound::WavWriter::create(output, spec)?;
    for (i, &(in_l, in_r)) in frames.iter().enumerate() {
        // Mirror block processing so per-block work (e.g. reading params) runs.
        if i % 512 == 0 {
            effect.pre_block();
        }
        let (out_l, out_r) = effect.process_frame(in_l, in_r);
        writer.write_sample(out_l)?;
        writer.write_sample(out_r)?;
    }
    writer.finalize()?;
    println!("Wrote {output} — {} frames @ {file_rate} Hz.", frames.len());
    Ok(())
}

/// Open a cpal output stream that pulls input frames from `source`, runs them
/// through `effect`, and plays the result. Blocks until the user presses Enter.
///
/// `requested_rate` is `Some(sr)` to open the device at the file's rate (avoids
/// resampling), or `None` to use the device's default rate.
fn play_live<E, S>(mut effect: E, requested_rate: Option<u32>, source: S) -> Result<()>
where
    E: DaudioEffect + Send + 'static,
    S: FnMut() -> (f32, f32) + Send + 'static,
{
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or("no default output device")?;
    let supported = device.default_output_config()?;
    let format = supported.sample_format();

    let sample_rate = requested_rate
        .map(cpal::SampleRate)
        .unwrap_or_else(|| supported.sample_rate());
    let channels = supported.channels() as usize;
    let config = cpal::StreamConfig {
        channels: supported.channels(),
        sample_rate,
        buffer_size: cpal::BufferSize::Default,
    };

    effect.activate(sample_rate.0 as f32);
    effect.reset();

    let err_fn = |err| eprintln!("audio stream error: {err}");
    let stream = match format {
        cpal::SampleFormat::F32 => {
            build::<f32, _, _>(&device, &config, channels, effect, source, err_fn)?
        }
        cpal::SampleFormat::I16 => {
            build::<i16, _, _>(&device, &config, channels, effect, source, err_fn)?
        }
        cpal::SampleFormat::U16 => {
            build::<u16, _, _>(&device, &config, channels, effect, source, err_fn)?
        }
        other => return Err(format!("unsupported sample format: {other:?}").into()),
    };
    stream.play()?;

    println!("Press Enter to stop.");
    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;
    Ok(())
}

/// Build a format-specific output stream that drives the effect one frame per
/// output frame (channel 0 = left, channel 1 = right, extra channels get left).
fn build<T, E, S>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    channels: usize,
    mut effect: E,
    mut source: S,
    err_fn: impl FnMut(cpal::StreamError) + Send + 'static,
) -> Result<cpal::Stream>
where
    T: SizedSample + FromSample<f32>,
    E: DaudioEffect + Send + 'static,
    S: FnMut() -> (f32, f32) + Send + 'static,
{
    let stream = device.build_output_stream(
        config,
        move |data: &mut [T], _| {
            // One cpal callback ~= one processing block.
            effect.pre_block();
            for frame in data.chunks_mut(channels) {
                let (in_l, in_r) = source();
                let (out_l, out_r) = effect.process_frame(in_l, in_r);
                for (i, slot) in frame.iter_mut().enumerate() {
                    let v = if i == 1 { out_r } else { out_l };
                    *slot = T::from_sample(v);
                }
            }
        },
        err_fn,
        None,
    )?;
    Ok(stream)
}

/// Read a WAV into stereo `f32` frames (mono is duplicated; >2 channels are
/// downmixed to the first two). Returns the frames and the file's sample rate.
fn read_wav(path: &str) -> Result<(Frames, u32)> {
    let mut reader = hound::WavReader::open(path)?;
    let spec = reader.spec();
    let channels = spec.channels as usize;
    if channels == 0 {
        return Err("file has zero channels".into());
    }

    let interleaved: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader
            .samples::<f32>()
            .collect::<std::result::Result<_, _>>()?,
        hound::SampleFormat::Int => {
            let scale = 1.0f32 / (1u32 << (spec.bits_per_sample - 1)) as f32;
            reader
                .samples::<i32>()
                .map(|s| s.map(|v| v as f32 * scale))
                .collect::<std::result::Result<_, _>>()?
        }
    };

    let frames = interleaved
        .chunks(channels)
        .map(|c| (c[0], if channels >= 2 { c[1] } else { c[0] }))
        .collect();
    Ok((frames, spec.sample_rate))
}
