use crate::{AudioFrame, CHANNEL_MAX_BUFFER, FRAME_SIZE};

use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    Host, StreamConfig,
};
use log::{info, trace, warn};
use tokio::sync::{
    broadcast::{self, TryRecvError},
    mpsc::{self, error::TrySendError},
};

pub struct AudioOutputDeviceStream {
    stream: cpal::Stream,
    config: StreamConfig,
}

fn default_output_device() -> (<Host as HostTrait>::Device, StreamConfig) {
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
    pub fn connect_default(
        frame_rx: broadcast::Receiver<AudioFrame>,
        buffer_request_tx: mpsc::Sender<()>,
    ) -> AudioOutputDeviceStream {
        let (device, config) = default_output_device();

        Self::connect_device(device, config, frame_rx)
    }

    pub fn connect_device(
        device: <Host as HostTrait>::Device,
        config: StreamConfig,
        mut frame_rx: broadcast::Receiver<AudioFrame>,
        mut buffer_request_tx: mpsc::Sender<()>,
    ) -> AudioOutputDeviceStream {
        info!("Creating output device stream with config:\n{:?}", config);

        let mut leftover_buffer = LeftoverBuffer::new();

        let stream = device
            .build_output_stream(
                &config,
                move |data: &mut [f32]| {
                    service_cpal_output_stream_callback(
                        data,
                        &mut leftover_buffer,
                        &mut buffer_request_tx,
                        &mut frame_rx,
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
        }
    }

    pub fn get_config(&self) -> &StreamConfig {
        &self.config
    }

    pub fn get_buffer_request_rx(&mut self) -> &mut mpsc::Receiver<()> {
        &mut self.buffer_request_rx
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
    buffer_request_tx: &mut mpsc::Sender<()>,
    frame_rx: &mut broadcast::Receiver<AudioFrame>,
) {
    // Zero out the buffer for safety.
    let zeroes = vec![0.0; data.len()];
    data.copy_from_slice(&zeroes);

    let items_requested = data.len();
    let mut items_fulfilled = 0;
    let mut buffer_request_debt = 0;
    while items_fulfilled < items_requested {
        // Try to pay down our buffer request debt.
        if buffer_request_debt > 0 {
            match buffer_request_tx.try_send(()) {
                Ok(_) => {
                    buffer_request_debt -= 1;
                }
                Err(TrySendError::Full(_)) => (),
                Err(TrySendError::Closed(_)) => {
                    panic!("Audio device buffer request stream was closed");
                }
            }
        }

        if leftover_buffer.is_empty() {
            // Tell the synthesizer that we're buffering so it knows to queue up more samples. This
            // shouldn't block, so instead we accumulate a retry count and pay it down later.
            match buffer_request_tx.try_send(()) {
                Ok(_) => (),
                Err(TrySendError::Full(_)) => {
                    buffer_request_debt += 1;
                }
                Err(TrySendError::Closed(_)) => {
                    // All we can really do is break, because this thread is out of our control.
                    warn!("Audio device buffer request stream is closed during buffer callback");
                    break;
                }
            }

            // Replenish our buffer. We shouldn't block to receive samples from the synthesizer
            // since this callback executes in a realtime priority thread. This means the
            // synthesizer thread needs to queue up samples at least as quickly as CPAL can consume
            // them, or else we'll play frames with gaps.
            match frame_rx.try_recv() {
                Ok(samples) => leftover_buffer.overwrite(&samples),
                Err(TryRecvError::Empty) => {
                    warn!("No frames ready when requested");
                    break;
                }
                Err(TryRecvError::Closed) => {
                    // All we can really do is break, because this thread is out of our control.
                    warn!("Audio device buffering stream is closed during buffer callback");
                    break;
                }
                Err(TryRecvError::Lagged(num_missed_frames)) => {
                    warn!(
                        "Device lagged behind audio frame producer by {} frames",
                        num_missed_frames
                    );
                }
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
