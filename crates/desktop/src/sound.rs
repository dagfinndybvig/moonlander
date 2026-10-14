//! The GT40's bell. Any write to the display status register rings it;
//! Moonlander rings it once when the fuel falls under 200 pounds, and when
//! the ship explodes.
//!
//! MODEL: the GT40 manual says only "tone generator". The tone here is a
//! soft square-ish wave of 880 Hz for 150 ms, with a fast attack and an
//! exponential fall, as a small speaker behind a panel would give.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

const BELL_HZ: f32 = 880.0;
const BELL_S: f32 = 0.15;
const VOLUME: f32 = 0.18;

pub struct Bell {
    pending: Arc<AtomicU32>,
    #[cfg(feature = "sound")]
    _stream: Option<cpal::Stream>,
}

impl Bell {
    pub fn new(enabled: bool) -> Bell {
        let pending = Arc::new(AtomicU32::new(0));
        #[cfg(feature = "sound")]
        {
            let stream = if enabled {
                match open(pending.clone()) {
                    Ok(s) => Some(s),
                    Err(e) => {
                        eprintln!("moonlander: no sound ({e})");
                        None
                    }
                }
            } else {
                None
            };
            Bell { pending, _stream: stream }
        }
        #[cfg(not(feature = "sound"))]
        {
            let _ = enabled;
            Bell { pending }
        }
    }

    pub fn ring(&self, times: u32) {
        if times > 0 {
            self.pending.fetch_add(times, Ordering::Relaxed);
        }
    }
}

/// The tone, sample by sample. Bells rung while one is sounding follow it.
struct Tone {
    rate: f32,
    /// Samples into the current tone, if one is sounding.
    at: Option<u32>,
}

impl Tone {
    fn next(&mut self, pending: &AtomicU32) -> f32 {
        if self.at.is_none() && pending.load(Ordering::Relaxed) > 0 {
            pending.fetch_sub(1, Ordering::Relaxed);
            self.at = Some(0);
        }
        let Some(n) = self.at else { return 0.0 };
        let s = n as f32 / self.rate;
        if s >= BELL_S {
            self.at = None;
            return 0.0;
        }
        self.at = Some(n + 1);
        let ph = std::f32::consts::TAU * BELL_HZ * s;
        // Odd harmonics, rolled off.
        let wave = ph.sin() + (ph * 3.0).sin() / 5.0 + (ph * 5.0).sin() / 12.0;
        let attack = (s / 0.004).min(1.0);
        let fall = (-s / (BELL_S / 4.0)).exp();
        VOLUME * wave * attack * fall
    }
}

#[cfg(feature = "sound")]
fn open(pending: Arc<AtomicU32>) -> Result<cpal::Stream, Box<dyn std::error::Error>> {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use cpal::{SampleFormat, SizedSample, FromSample};

    fn build<T: SizedSample + FromSample<f32>>(
        device: &cpal::Device,
        config: &cpal::StreamConfig,
        pending: Arc<AtomicU32>,
    ) -> Result<cpal::Stream, cpal::BuildStreamError> {
        let channels = config.channels as usize;
        let mut tone = Tone { rate: config.sample_rate.0 as f32, at: None };
        device.build_output_stream(
            config,
            move |data: &mut [T], _| {
                for frame in data.chunks_mut(channels) {
                    let v = T::from_sample(tone.next(&pending));
                    frame.iter_mut().for_each(|s| *s = v);
                }
            },
            |e| eprintln!("moonlander: sound: {e}"),
            None,
        )
    }

    let device = cpal::default_host().default_output_device().ok_or("no output device")?;
    let supported = device.default_output_config()?;
    let config: cpal::StreamConfig = supported.config();
    let stream = match supported.sample_format() {
        SampleFormat::F32 => build::<f32>(&device, &config, pending)?,
        SampleFormat::I16 => build::<i16>(&device, &config, pending)?,
        SampleFormat::U16 => build::<u16>(&device, &config, pending)?,
        SampleFormat::I32 => build::<i32>(&device, &config, pending)?,
        f => return Err(format!("unsupported sample format {f}").into()),
    };
    stream.play()?;
    Ok(stream)
}
