use crate::{
    audio_device::AudioOutputDeviceStream, midi::MidiInputStream, recording::RecordingOutputStream,
    synthesizer::Synthesizer,
};

use crossbeam_channel as channel;
use log::debug;
use std::path::PathBuf;

/// The main server that wires all inputs through the synthesizer and into the outputs.
pub struct NocturneServer {
    midi_input_port: usize,
    recording_path: Option<PathBuf>,
}

impl NocturneServer {
    pub fn new(midi_input_port: usize, recording_path: Option<PathBuf>) -> Self {
        NocturneServer {
            midi_input_port,
            recording_path,
        }
    }

    pub fn run(&self) {
        // Set SIGINT handler.
        let (exit_tx, exit_rx) = channel::bounded(1);
        ctrlc::set_handler(move || {
            exit_tx.send(()).expect("Failed to send exit signal");
        })
        .expect("Error setting Ctrl-C handler");

        // Create the synth.
        let midi_input_stream = MidiInputStream::connect(self.midi_input_port);
        let audio_output_stream = AudioOutputDeviceStream::connect_default();
        let recorder = self
            .recording_path
            .as_ref()
            .map(|p| RecordingOutputStream::connect(p));
        let mut synth = Synthesizer::new(midi_input_stream, audio_output_stream, recorder);

        // Run the synth.
        synth.buffer_ahead();
        synth.start_output_device();
        loop {
            synth.handle_events();

            if exit_rx.try_recv().is_ok() {
                debug!("Received exit signal. Destroying process...");
                break;
            }
        }
        synth.close();
    }
}
