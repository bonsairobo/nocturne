use crate::AudioFrame;

use log::info;
use std::path::PathBuf;
use tokio::{
    select,
    sync::{broadcast::{self, RecvError}, oneshot},
    task,
};

pub struct RecordingOutputStream {
    exit_tx: oneshot::Sender<()>,
    join_handle: task::JoinHandle<()>,
}

impl RecordingOutputStream {
    pub fn connect(
        path: &PathBuf,
        num_channels: u16,
        sample_hz: u32,
        frame_rx: broadcast::Receiver<AudioFrame>,
    ) -> Self {
        let path_str = path
            .as_path()
            .to_str()
            .expect("Invalid path for recording file.")
            .to_string();
        let (exit_tx, exit_rx) = oneshot::channel();
        let join_handle = task::spawn(async move {
            buffered_file_writer_task(path_str, num_channels, sample_hz, frame_rx, exit_rx).await
        });

        RecordingOutputStream {
            exit_tx,
            join_handle,
        }
    }

    pub async fn close(self) {
        self.exit_tx
            .send(())
            .expect("Failed to send exit signal to WAV writer task");
        self.join_handle
            .await
            .expect("Failed to join on WAV writer task");
    }
}

/// Runs until being told to stop, at which point it flushes outstanding file writes.
async fn buffered_file_writer_task(
    path: String,
    channels: u16,
    sample_hz: u32,
    mut frame_rx: broadcast::Receiver<AudioFrame>,
    mut exit_rx: oneshot::Receiver<()>,
) {
    let spec = hound::WavSpec {
        channels,
        sample_rate: sample_hz,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec).expect("Failed to create WAV file");

    loop {
        select! {
            item = &mut exit_rx => {
                item.expect("Cancellation channel to recording task close early");
                info!("WAV file writing task interrupted");
                break;
            },
            frame = frame_rx.recv() => {
                match frame {
                    Ok(samples) => {
                        let amplitude = i16::max_value() as f32;
                        for s in samples.iter() {
                            // TODO: make async?
                            writer.write_sample((amplitude * s) as i16)
                                .expect("WAV writer failed to write sample.");
                        }
                    }
                    Err(RecvError::Closed) => break,
                    Err(RecvError::Lagged(_)) => (),
                }
            },
        }
    }

    // TODO: make async?
    writer.finalize().expect("Failed to finalize sample file.");
    info!("Flushed WAV file buffer.");
}
