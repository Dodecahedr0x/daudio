//! Audible smoke-test for the filter DSP — no DAW, no plugin host, no input
//! device. Generates a saw wave, runs it through the real [`FilterCore`], and
//! plays it to the default output while sweeping the cutoff so you can hear the
//! filter working. Press Enter to stop.
//!
//! Run with: `cargo run -p filter --bin demo`

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SizedSample};
use daudio_dsp::oscillator::{Oscillator, Waveform};
use filter::dsp::FilterCore;

fn main() {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .expect("no default output device available");
    let config = device
        .default_output_config()
        .expect("no default output config available");

    println!(
        "Output: {} @ {} Hz ({:?})",
        device.name().unwrap_or_else(|_| "<unknown>".into()),
        config.sample_rate().0,
        config.sample_format(),
    );

    match config.sample_format() {
        cpal::SampleFormat::F32 => run::<f32>(&device, &config.into()),
        cpal::SampleFormat::I16 => run::<i16>(&device, &config.into()),
        cpal::SampleFormat::U16 => run::<u16>(&device, &config.into()),
        other => panic!("unsupported sample format: {other:?}"),
    }
}

fn run<T>(device: &cpal::Device, config: &cpal::StreamConfig)
where
    T: SizedSample + FromSample<f32>,
{
    let sample_rate = config.sample_rate.0 as f32;
    let channels = config.channels as usize;

    // The actual plugin DSP: a saw source into the real FilterCore.
    let mut core = FilterCore::new();
    core.set_sample_rate(sample_rate);
    core.snap_gain(0.0); // 0 dB
    let mut osc = Oscillator::new(Waveform::Saw);
    osc.set_sample_rate(sample_rate);
    osc.set_frequency(110.0); // low A, rich in harmonics so the sweep is obvious

    // Slow LFO (~0.2 Hz) sweeping the cutoff logarithmically over 80–4000 Hz.
    let mut lfo_phase = 0.0f32;
    let lfo_inc = 0.2 / sample_rate;

    let mut next_sample = move || {
        lfo_phase += lfo_inc;
        if lfo_phase >= 1.0 {
            lfo_phase -= 1.0;
        }
        let unipolar = (std::f32::consts::TAU * lfo_phase).sin() * 0.5 + 0.5; // 0..1
        let cutoff = 80.0 * 50.0f32.powf(unipolar); // 80..4000 Hz, log
        core.set_cutoff(cutoff);

        let raw = osc.next_sample();
        let (left, _right) = core.process_frame(raw, raw, 0.0);
        left * 0.2 // headroom
    };

    let err_fn = |err| eprintln!("audio stream error: {err}");
    let stream = device
        .build_output_stream(
            config,
            move |data: &mut [T], _| {
                for frame in data.chunks_mut(channels) {
                    let value = T::from_sample(next_sample());
                    for slot in frame.iter_mut() {
                        *slot = value;
                    }
                }
            },
            err_fn,
            None,
        )
        .expect("failed to build output stream");
    stream.play().expect("failed to start playback");

    println!("\n♪ 110 Hz saw → lowpass, cutoff sweeping 80–4000 Hz.");
    println!("Press Enter to stop.");
    let mut line = String::new();
    let _ = std::io::stdin().read_line(&mut line);
}
