/// Audio backend using cpal.

use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{HostId, Stream};
use rtrb::Consumer;

use crate::synth::{ControlEvent, MidiEvent, SynthEngine};

pub struct AudioConfig {
    pub host_id: HostId,
    pub sample_rate: u32,
    pub buffer_size: u32,
}

pub fn available_hosts() -> Vec<(HostId, &'static str)> {
    cpal::available_hosts()
        .into_iter()
        .map(|id| {
            let name = match id.name() {
                "ALSA" => "ALSA",
                "JACK" => "JACK",
                n => n,
            };
            (id, name)
        })
        .collect()
}

pub fn supported_sample_rates(host_id: HostId) -> Vec<u32> {
    let common_rates = [22050, 44100, 48000, 88200, 96000, 176400, 192000];

    let Ok(host) = cpal::host_from_id(host_id) else {
        return vec![44100, 48000];
    };
    let Some(device) = host.default_output_device() else {
        return vec![44100, 48000];
    };
    let Ok(configs) = device.supported_output_configs() else {
        return vec![44100, 48000];
    };

    let mut rates: Vec<u32> = Vec::new();
    for cfg in configs {
        let min = cfg.min_sample_rate().0;
        let max = cfg.max_sample_rate().0;
        for &rate in &common_rates {
            if rate >= min && rate <= max && !rates.contains(&rate) {
                rates.push(rate);
            }
        }
    }
    rates.sort();
    if rates.is_empty() {
        vec![44100, 48000]
    } else {
        rates
    }
}

pub struct AudioBackend {
    #[allow(dead_code)]
    stream: Stream,
}

impl AudioBackend {
    pub fn new(
        config: AudioConfig,
        mut synth: SynthEngine,
        mut midi_rx: Consumer<MidiEvent>,
        mut ctrl_rx: Consumer<ControlEvent>,
    ) -> Result<(Self, u32)> {
        let host = cpal::host_from_id(config.host_id)
            .map_err(|e| anyhow::anyhow!("Failed to init audio host: {e}"))?;
        let device = host
            .default_output_device()
            .context("No audio output device found")?;

        let supported = device.default_output_config()?;
        let channels = supported.channels() as usize;
        let device_sr = supported.sample_rate().0;

        // Try requested config first, fall back to device defaults
        let (actual_sr, actual_buf) = Self::negotiate_config(
            &device,
            supported.channels(),
            config.sample_rate,
            config.buffer_size,
            device_sr,
        );

        let stream_config = cpal::StreamConfig {
            channels: supported.channels(),
            sample_rate: cpal::SampleRate(actual_sr),
            buffer_size: if actual_buf > 0 {
                cpal::BufferSize::Fixed(actual_buf)
            } else {
                cpal::BufferSize::Default
            },
        };

        synth.set_sample_rate(actual_sr as f32);

        let stream = device.build_output_stream(
            &stream_config,
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                while let Ok(event) = ctrl_rx.pop() {
                    synth.handle_control(event);
                }

                while let Ok(event) = midi_rx.pop() {
                    synth.handle_event(event);
                }

                for frame in data.chunks_mut(channels) {
                    let (left, right) = synth.tick();
                    if channels >= 2 {
                        frame[0] = left;
                        frame[1] = right;
                        for s in frame.iter_mut().skip(2) {
                            *s = 0.0;
                        }
                    } else {
                        frame[0] = (left + right) * 0.5;
                    }
                }
            },
            |err| {
                eprintln!("Audio stream error: {err}");
            },
            None,
        )?;

        stream.play()?;

        Ok((Self { stream }, actual_sr))
    }

    /// Try requested SR + buffer, fall back step by step to device defaults.
    fn negotiate_config(
        device: &cpal::Device,
        _channels: u16,
        requested_sr: u32,
        requested_buf: u32,
        device_sr: u32,
    ) -> (u32, u32) {
        let configs_to_try = [
            (requested_sr, requested_buf),
            (requested_sr, 0),        // requested SR, default buffer
            (device_sr, requested_buf), // device SR, requested buffer
            (device_sr, 0),            // all defaults
        ];

        for (sr, buf) in configs_to_try {
            if sr == 0 {
                continue;
            }
            let supported = device.supported_output_configs();
            if let Ok(mut configs) = supported {
                let valid = configs.any(|c| {
                    sr >= c.min_sample_rate().0 && sr <= c.max_sample_rate().0
                });
                if valid {
                    return (sr, buf);
                }
            }
        }

        (device_sr, 0)
    }
}
