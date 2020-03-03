use crate::AudioFrame;

use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    StreamConfig,
};
use crossbeam_channel as channel;
use crossbeam_channel::{Receiver, Sender};
use log::{info, trace};

pub struct AudioOutputDeviceStream {
    stream: cpal::Stream,
    config: StreamConfig,

    /// Receive a message when the device wants us to buffer another frame.
    buffer_request_rx: Receiver<()>,

    /// Send the audio samples to be played by the device.
    sample_tx: Sender<AudioFrame>,
}

impl AudioOutputDeviceStream {
    pub fn connect_default() -> AudioOutputDeviceStream {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .expect("no output device available");
        let mut supported_configs_range = device
            .supported_output_configs()
            .expect("error while querying configs");
        let supported_config = supported_configs_range
            .next()
            .expect("no supported config?!")
            .with_max_sample_rate();
        let config = supported_config.config();
        info!("Creating output device stream with config:\n{:?}", config);

        let (buffer_request_tx, buffer_request_rx) = channel::unbounded();
        let (sample_tx, sample_rx) = channel::unbounded();

        let stream = device
            .build_output_stream(
                &config,
                move |data: &mut [f32]| {
                    service_cpal_output_stream_callback(data, &buffer_request_tx, &sample_rx)
                },
                move |err| {
                    // TODO
                },
            )
            .expect("Failed to build CPAL output stream");

        AudioOutputDeviceStream {
            stream,
            config,
            buffer_request_rx,
            sample_tx,
        }
    }

    pub fn get_config(&self) -> &StreamConfig {
        &self.config
    }

    pub fn get_buffer_request_rx(&self) -> &Receiver<()> {
        &self.buffer_request_rx
    }

    pub fn write_frame(&self, frame: AudioFrame) {
        self.sample_tx
            .send(frame)
            .expect("Failed to send frame to output device");
    }

    pub fn play(&self) {
        self.stream
            .play()
            .expect("Failed to play output device stream");
    }

    pub fn pause(&self) {
        self.stream
            .pause()
            .expect("Failed to pause output device stream");
    }
}

fn service_cpal_output_stream_callback(
    data: &mut [f32],
    buffer_request_tx: &Sender<()>,
    sample_rx: &Receiver<AudioFrame>,
) {
    // Zero out the buffer for safety.
    let zeroes = vec![0.0; data.len()];
    data.copy_from_slice(&zeroes);

    // Tell the synthesizer that we're buffering so it knows to queue up more samples. This
    // shouldn't block.
    buffer_request_tx
        .send(())
        .expect("Failed to send buffer request");

    // We shouldn't block to receive samples from the synthesizer since this callback executes in a
    // realtime priority thread. This means the synthesizer thread needs to queue up samples at
    // least as quickly as CPAL can consume them, or else we'll play empty frames.
    match sample_rx.try_recv() {
        Ok(samples) => {
            // TODO: dynamic-buffer-length queueing? could mark the channel entries with a "#
            // consumed" field. may require changes to the buffer_notify scheme
            let samples_requested = data.len();
            let samples_provided = samples.len();
            let overlap = samples_requested.min(samples_provided);
            trace!("Samples requested = {}", samples_requested);
            trace!("Samples provided = {}", samples_provided);
            data[..overlap].copy_from_slice(&samples[..overlap]);
        }
        Err(_) => trace!("CPAL received empty frame"), // Oh no! A glitch!
    }
}
