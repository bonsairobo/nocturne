use crate::{AudioFrame, CHANNEL_MAX_BUFFER, FRAME_SIZE};

use log::info;
use std::path::PathBuf;
use tokio::{
    select,
    sync::{mpsc::{self, error::SendError}, oneshot},
    task,
};

pub struct RecordingOutputStream {
    sample_tx: mpsc::Sender<AudioFrame>,
    exit_tx: oneshot::Sender<()>,
    join_handle: task::JoinHandle<()>,
}

impl RecordingOutputStream {
    pub fn connect(path: &PathBuf, num_channels: u16, sample_hz: u32) -> Self {
        let path_str = path
            .as_path()
            .to_str()
            .expect("Invalid path for recording file.")
            .to_string();
        let (sample_tx, sample_rx) = mpsc::channel(CHANNEL_MAX_BUFFER);
        let (exit_tx, exit_rx) = oneshot::channel();
        let join_handle = task::spawn(async move {
            buffered_file_writer_task(path_str, num_channels, sample_hz, sample_rx, exit_rx).await
        });

        RecordingOutputStream {
            sample_tx,
            exit_tx,
            join_handle,
        }
    }

    pub async fn write_frame(&mut self, frame: AudioFrame) {
        match self.sample_tx.send(frame).await {
            Ok(_) => (),
            Err(SendError(_)) => panic!("Failed to send audio frame to output device"),
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
    mut samples_rx: mpsc::Receiver<[f32; FRAME_SIZE]>,
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
            frame = samples_rx.recv() => {
                frame.expect("Couldn't receive frame from instrument.");
                let samples = frame.expect("Failed to receive samples.");
                let amplitude = i16::max_value() as f32;
                for s in samples.iter() {
                    // TODO: make async?
                    writer.write_sample((amplitude * s) as i16)
                        .expect("WAV writer failed to write sample.");
                }
            },
        }
    }

    // TODO: make async?
    writer.finalize().expect("Failed to finalize sample file.");
    info!("Flushed WAV file buffer.");
}
