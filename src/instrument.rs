use crate::{
    audio_device::AudioOutputDeviceStream,
    midi::{MidiInputDeviceStream, MidiInputStream, MidiTrackInputStream},
    recording::RecordingOutputStream,
    synthesizer::Synthesizer,
    AudioFrame,
};

use cpal::SampleRate;
use crossbeam_channel::{Receiver, select};
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

    pub fn play_midi_device(&self, midi_input_port: usize) {
        let midi_input = MidiInputDeviceStream::connect(midi_input_port);
        self.play_midi(midi_input);
    }

    pub fn play_midi_file(&self, path: PathBuf) {
        // TODO: play all the tracks concurrently
        let midi_input = MidiTrackInputStream::start(path, 3);
        self.play_midi(midi_input);
    }

    pub fn play_midi<M: MidiInputStream>(&self, midi_input_stream: M) {
        // Create the instrument components: input streams --> synth --> output streams.
        let audio_output_stream = AudioOutputDeviceStream::connect_default();
        let num_channels = audio_output_stream.get_config().channels;
        let SampleRate(sample_hz) = audio_output_stream.get_config().sample_rate;
        let recorder = self
            .recording_path
            .as_ref()
            .map(|p| RecordingOutputStream::connect(p, num_channels, sample_hz));
        let mut synth = Synthesizer::new(sample_hz as f32);

        let send_frame = |frame: AudioFrame| {
            if let Some(recorder) = recorder.as_ref() {
                audio_output_stream.write_frame(frame.clone());
                recorder.write_frame(frame);
            } else {
                audio_output_stream.write_frame(frame.clone());
            }
        };

        // Get ahead of the CPAL buffering.
        // The synthesizer thread will attempt to queue samples ahead of the audio output thread.
        // This represents an additional fixed latency of:
        //     5 buffers * 512 samples per channel * (1 / 44100) seconds = 0.06 seconds
        const BUFFERS_AHEAD: u32 = 5;
        for _ in 0..BUFFERS_AHEAD {
            send_frame(synth.sample_notes(num_channels as usize));
        }

        // Run the synth.
        // TODO: this event loop could benefit from async rust
        audio_output_stream.play();
        loop {
            select! {
                recv(midi_input_stream.get_message_rx()) -> item => {
                    let raw_message = item.expect("Couldn't receive MIDI message.");
                    synth.handle_midi_message(raw_message);
                },
                recv(audio_output_stream.get_buffer_request_rx()) -> item => {
                    item.expect("Couldn't receive buffer request.");
                    send_frame(synth.sample_notes(num_channels as usize));
                },
                recv(self.canceller) -> item => {
                    item.expect("Couldn't receive cancellation.");
                    debug!("Interrupted instrument");
                    break;
                }
            }
        }

        // Tear down.
        if let Some(r) = recorder { r.close() }
        midi_input_stream.close();
    }
}
