use crate::{
    audio_device::AudioOutputDeviceStream,
    midi::{MidiInputDeviceStream, MidiInputStream, MidiTrackInputStream},
    recording::RecordingOutputStream,
    synthesizer::Synthesizer,
};

use cpal::SampleRate;
use crossbeam_channel::Receiver;
use log::debug;
use std::path::PathBuf;

/// Accepts MIDI input via channels and controls a synthesizer, sending audio samples to an output
/// device.
pub struct Instrument {
    canceller: Receiver<()>,
    recording_path: Option<PathBuf>,
}

impl Instrument {
    pub fn new(canceller: Receiver<()>, recording_path: Option<PathBuf>) -> Self {
        Instrument {
            canceller,
            recording_path,
        }
    }

    pub fn run_midi_device(&self, midi_input_port: usize) {
        let midi_input = MidiInputDeviceStream::connect(midi_input_port);
        self.run_midi(midi_input);
    }

    pub fn run_midi_file(&self, path: PathBuf) {
        let midi_input = MidiTrackInputStream::start(path, 1);
        self.run_midi(midi_input);
    }

    pub fn run_midi<M: MidiInputStream>(&self, midi_input_stream: M) {
        // Create the synth.
        let audio_output_stream = AudioOutputDeviceStream::connect_default();
        let channels = audio_output_stream.get_config().channels;
        let SampleRate(sample_hz) = audio_output_stream.get_config().sample_rate;
        let recorder = self
            .recording_path
            .as_ref()
            .map(|p| RecordingOutputStream::connect(p, channels, sample_hz));
        let mut synth = Synthesizer::new(
            midi_input_stream,
            sample_hz as f32,
            audio_output_stream,
            recorder,
        );

        // Run the synth.
        synth.buffer_ahead();
        synth.start_output_device();
        loop {
            synth.handle_events();

            if self.canceller.try_recv().is_ok() {
                debug!("Interrupted instrument");
                break;
            }
        }
        synth.close();
    }
}
