use crate::{AudioFrame, FRAME_SIZE};

use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    Host,
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

pub fn default_output_device() -> (<Host as HostTrait>::Device, StreamConfig) {
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

    (device, config)
}

impl AudioOutputDeviceStream {
    pub fn connect_default() -> AudioOutputDeviceStream {
        let (device, config) = default_output_device();

        Self::connect_device(device, config)
    }

    pub fn connect_device(
        device: <Host as HostTrait>::Device, config: StreamConfig
    ) -> AudioOutputDeviceStream {
        info!("Creating output device stream with config:\n{:?}", config);

        let (buffer_request_tx, buffer_request_rx) = channel::unbounded();
        let (sample_tx, sample_rx) = channel::unbounded();
        let mut leftover_buffer = LeftoverBuffer::new();

        let stream = device
            .build_output_stream(
                &config,
                move |data: &mut [f32]| {
                    service_cpal_output_stream_callback(
                        data,
                        &mut leftover_buffer,
                        &buffer_request_tx,
                        &sample_rx,
                    )
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
    leftover_buffer: &mut LeftoverBuffer,
    buffer_request_tx: &Sender<()>,
    sample_rx: &Receiver<AudioFrame>,
) {
    // Zero out the buffer for safety.
    let zeroes = vec![0.0; data.len()];
    data.copy_from_slice(&zeroes);

    let items_requested = data.len();
    let mut items_fulfilled = 0;
    while items_fulfilled < items_requested {
        if leftover_buffer.is_empty() {
            // Tell the synthesizer that we're buffering so it knows to queue up more samples. This
            // shouldn't block.
            buffer_request_tx
                .send(())
                .expect("Failed to send buffer request");

            // Replenish our buffer. We shouldn't block to receive samples from the synthesizer
            // since this callback executes in a realtime priority thread. This means the
            // synthesizer thread needs to queue up samples at least as quickly as CPAL can consume
            // them, or else we'll play frames with gaps.
            match sample_rx.try_recv() {
                Ok(samples) => leftover_buffer.overwrite(&samples),
                Err(_) => break,
            }
        }

        items_fulfilled += leftover_buffer.consume(&mut data[items_fulfilled..]);
    }

    if items_fulfilled < items_requested {
        trace!(
            "Fulfilled {} of {} items requested",
            items_fulfilled,
            items_requested
        );
    }
}

struct LeftoverBuffer {
    buffer: [f32; FRAME_SIZE],
    cursor: usize,
}

impl LeftoverBuffer {
    fn new() -> Self {
        LeftoverBuffer {
            buffer: [0.0; FRAME_SIZE],
            cursor: FRAME_SIZE,
        }
    }

    fn is_empty(&self) -> bool {
        self.items_leftover() == 0
    }

    fn items_leftover(&self) -> usize {
        FRAME_SIZE - self.cursor
    }

    /// Returns the number of items consumed from self.
    fn consume(&mut self, data_out: &mut [f32]) -> usize {
        let copy_amt = self.items_leftover().min(data_out.len());
        let src_end = self.cursor + copy_amt;
        data_out[..copy_amt].copy_from_slice(&self.buffer[self.cursor..src_end]);
        self.cursor += copy_amt;

        copy_amt
    }

    fn overwrite(&mut self, data_in: &[f32]) {
        self.buffer[..].copy_from_slice(data_in);
        self.cursor = 0;
    }
}
