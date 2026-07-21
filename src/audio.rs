//! Audio backend using cpal.

use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{HostId, Stream};
use rtrb::Consumer;
use std::sync::{Arc, atomic::{AtomicBool, AtomicU32, Ordering}};

use crate::synth::{ControlEvent, MidiEvent, SynthEngine};

/// Soft limiter: transparent below threshold, tanh-shaped saturation above.
/// Zero-latency, no lookahead needed for a soft synth.
#[inline]
fn soft_limit(x: f32) -> f32 {
    const THRESH: f32 = 0.8;
    if x.abs() <= THRESH {
        x
    } else {
        let sign = x.signum();
        let excess = (x.abs() - THRESH) / (1.0 - THRESH);
        // tanh approximation: x/(1+|x|)
        let shaped = excess / (1.0 + excess);
        sign * (THRESH + (1.0 - THRESH) * shaped)
    }
}

pub struct AudioConfig {
    pub host_id: HostId,
    pub sample_rate: u32,
    pub buffer_size: u32,
}

pub fn available_hosts() -> Vec<(HostId, &'static str)> {
    cpal::available_hosts()
        .into_iter()
        .map(|id| {
            (id, id.name())
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
    /// Set to true when the audio device disconnects (e.g. headphones unplugged).
    /// GUI polls this to show a "restart app" message.
    pub disconnected: Arc<AtomicBool>,
    /// Set to true when the app is backgrounded (Android onPause).
    /// Audio callback outputs silence when paused to save battery.
    #[allow(dead_code)]
    pub paused: Arc<AtomicBool>,
}

impl AudioBackend {
    pub fn new(
        config: AudioConfig,
        mut synth: SynthEngine,
        mut midi_rx: Consumer<MidiEvent>,
        mut ctrl_rx: Consumer<ControlEvent>,
        amidi_port: std::sync::Arc<crate::amidi::AmidiPort>,
        note_state: crate::midi::NoteState,
        pad_state: crate::midi::PadState,
        paused: Arc<AtomicBool>,
    ) -> Result<(Self, u32)> {
        let host = cpal::host_from_id(config.host_id)
            .map_err(|e| anyhow::anyhow!("Failed to init audio host: {e}"))?;
        let device = host
            .default_output_device()
            .context("No audio output device found")?;

        let supported = device.default_output_config()?;
        let channels = supported.channels() as usize;
        let device_sr = supported.sample_rate().0;

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

        // JACK/PipeWire reports its initial SR via "sample rate changed to: X"
        // error callbacks after stream start.  We track this so we can update
        // the synth and return the real SR to the GUI.
        let jack_sr = Arc::new(AtomicU32::new(actual_sr));
        let jack_sr_err = jack_sr.clone();
        let jack_sr_cb  = jack_sr.clone();
        let mut synth_sr = actual_sr;
        let disconnected = Arc::new(AtomicBool::new(false));
        let disconnected_err = disconnected.clone();
        let paused_cb = paused.clone();
        let amidi_port = amidi_port.clone();
        let note_state = note_state.clone();
        let pad_state = pad_state.clone();

        let stream = device.build_output_stream(
            &stream_config,
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                // If app is paused (backgrounded), output silence to save battery
                if paused_cb.load(Ordering::Relaxed) {
                    for sample in data.iter_mut() { *sample = 0.0; }
                    return;
                }
                no_denormals::no_denormals(|| {
                    // If JACK changed the sample rate, update the synth immediately.
                    let new_sr = jack_sr_cb.load(Ordering::Relaxed);
                    if new_sr != synth_sr {
                        synth_sr = new_sr;
                        synth.set_sample_rate(new_sr as f32);
                    }

                    while let Ok(event) = ctrl_rx.pop() {
                        synth.handle_control(event);
                    }

                    let total_frames = data.len() / channels;
                    let mut frame_offset = 0;

                    while frame_offset < total_frames {
                        let block_len = (total_frames - frame_offset).min(crate::synth::BLOCK_SIZE);

                        // USB MIDI via AMidi (non-blocking, RT-safe, zero-latency)
                        {
                            let mut amidi_buf = [0u8; 256];
                            while let Some(nbytes) = amidi_port.receive(&mut amidi_buf) {
                                let mut off = 0;
                                while off < nbytes {
                                    if let Some((event, consumed)) = crate::midi::parse_midi_message(&amidi_buf[off..nbytes]) {
                                        // Update GUI note display
                                        match &event {
                                            crate::synth::MidiEvent::NoteOn { channel, note, velocity } => {
                                                let st = if *channel == 9 { &pad_state } else { &note_state };
                                                if *velocity == 0 { st[*note as usize].store(0, Ordering::Relaxed); }
                                                else { st[*note as usize].store(*velocity, Ordering::Relaxed); }
                                            }
                                            crate::synth::MidiEvent::NoteOff { channel, note } => {
                                                let st = if *channel == 9 { &pad_state } else { &note_state };
                                                st[*note as usize].store(0, Ordering::Relaxed);
                                            }
                                            _ => {}
                                        }
                                        synth.handle_event(event);
                                        off += consumed;
                                    } else {
                                        break;
                                    }
                                }
                            }
                        }

                        while let Ok(event) = midi_rx.pop() {
                            synth.handle_event(event);
                        }

                        let mut bl = [0.0f32; crate::synth::BLOCK_SIZE];
                        let mut br = [0.0f32; crate::synth::BLOCK_SIZE];
                        synth.tick_block(&mut bl[..block_len], &mut br[..block_len]);

                        for i in 0..block_len {
                            let left = soft_limit(bl[i]);
                            let right = soft_limit(br[i]);
                            let base = (frame_offset + i) * channels;
                            if channels >= 2 {
                                data[base] = left;
                                data[base + 1] = right;
                                for s in data[base + 2..base + channels].iter_mut() {
                                    *s = 0.0;
                                }
                            } else {
                                data[base] = (left + right) * 0.5;
                            }
                        }

                        frame_offset += block_len;
                    }
                });
            },
            move |err| {
                let msg = err.to_string();
                if let Some(sr_str) = msg.strip_prefix("A backend-specific error has occurred: sample rate changed to: ") {
                    if let Ok(sr) = sr_str.trim().parse::<u32>() {
                        jack_sr_err.store(sr, Ordering::Relaxed);
                    }
                } else {
                    // Device disconnected (headphones unplugged etc.)
                    disconnected_err.store(true, Ordering::Relaxed);
                }
                log::info!("Audio stream error: {err}");
                eprintln!("Audio stream error: {err}");
            },
            None,
        )?;

        stream.play()?;

        // Give JACK time to fire the "sample rate changed" callbacks so we
        // return the real SR to the GUI rather than the initial cpal value.
        std::thread::sleep(std::time::Duration::from_millis(150));
        let final_sr = jack_sr.load(Ordering::Relaxed);

        Ok((Self { stream, disconnected, paused }, final_sr))
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
