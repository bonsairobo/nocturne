use crate::{AudioFrame, FRAME_SIZE};

use crossbeam_channel as channel;
use crossbeam_channel::{select, Receiver, Sender};
use log::info;
use std::path::PathBuf;
use std::thread;

pub struct RecordingOutputStream {
    sample_tx: Sender<AudioFrame>,
    exit_tx: Sender<()>,
}

impl RecordingOutputStream {
    pub fn connect(path: &PathBuf, num_channels: u16, sample_hz: u32) -> Self {
        let path_str = path
            .as_path()
            .to_str()
            .expect("Invalid path for recording file.")
            .to_string();
        let (sample_tx, sample_rx) = channel::unbounded();
        let (exit_tx, exit_rx) = channel::bounded(1);
        thread::spawn(move || {
            buffered_file_writer_thread(path_str, num_channels, sample_hz, &sample_rx, &exit_rx)
        });

        RecordingOutputStream { sample_tx, exit_tx }
    }

    pub fn write_frame(&self, frame: AudioFrame) {
        self.sample_tx
            .send(frame)
            .expect("Failed to send frame to WAV writer");
    }

    pub fn close(&self) {
        self.exit_tx.send(()).expect("Failed to close WAV writer");
    }
}

fn buffered_file_writer_thread(
    path: String,
    channels: u16,
    sample_hz: u32,
    samples_rx: &Receiver<[f32; FRAME_SIZE]>,
    exit_rx: &Receiver<()>,
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
            recv(samples_rx) -> samples => {
                let samples = samples.expect("Failed to receive samples.");
                let amplitude = i16::max_value() as f32;
                for s in samples.iter() {
                    writer.write_sample((amplitude * s) as i16)
                        .expect("WAV writer failed to write sample.");
                }
            },
            recv(exit_rx) -> _ => break,
        }
    }

    writer.finalize().expect("Failed to finalize sample file.");
    info!("Flushed WAV file buffer.");
}
