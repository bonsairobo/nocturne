use crate::{
    audio_device::AudioOutputDeviceStream,
    midi::{MidiBytes, MidiInputDeviceStream, MidiInputStream, MidiTrackInputStream, RawMidiMessage},
    recording::RecordingOutputStream,
    synthesizer::Synthesizer,
    CHANNEL_MAX_BUFFER,
};

use cpal::SampleRate;
use log::{debug, info};
use std::path::PathBuf;
use tokio::{
    select, signal,
    stream::{Stream, StreamExt},
    sync::broadcast,
};

/// Accepts MIDI input via channels and controls a synthesizer, sending audio samples to an output
/// device.
pub struct Instrument {
    recording_path: Option<PathBuf>,
}

impl Instrument {
    pub fn new(recording_path: Option<PathBuf>) -> Self {
        Instrument { recording_path }
    }

    pub async fn play_midi_device(&self, midi_input_port: usize) {
        let midi_input = MidiInputDeviceStream::connect(midi_input_port);
        self.play_midi(midi_input).await;
    }

    pub async fn play_midi_file(&self, midi_bytes: MidiBytes) {
        // TODO: play all the tracks concurrently
        let midi_input = MidiTrackInputStream::start(midi_bytes, 3);
        self.play_midi(midi_input).await;
    }

    pub async fn play_midi<M, S>(&self, mut midi_input_stream: M)
    where
        M: MidiInputStream<MessageStream = S>,
        S: Stream<Item = RawMidiMessage> + Unpin,
    {
        // Audio output can have many subscribers.
        let (frame_tx, device_frame_rx) = broadcast::channel(CHANNEL_MAX_BUFFER);

        // Create the instrument components: input streams --> synth --> output streams.
        let mut audio_output_stream = AudioOutputDeviceStream::connect_default(device_frame_rx);
        let num_channels = audio_output_stream.get_config().channels;
        let SampleRate(sample_hz) = audio_output_stream.get_config().sample_rate;
        let recorder = self.recording_path.as_ref().map(|p| {
            let recorder_frame_rx = frame_tx.subscribe();

            RecordingOutputStream::connect(p, num_channels, sample_hz, recorder_frame_rx)
        });
        let mut synth = Synthesizer::new(sample_hz as f32);

        // Get ahead of the CPAL buffering.
        // The synthesizer thread will attempt to queue samples ahead of the audio output
        // thread. This represents an additional fixed latency of:
        //     5 buffers * 512 samples per channel * (1 / 44100) seconds = 0.06 seconds
        const BUFFERS_AHEAD: u32 = 5;
        for _ in 0..BUFFERS_AHEAD {
            let frame = synth.sample_notes(num_channels as usize);
            if frame_tx.send(frame).is_err() {
                panic!("Failed to send audio frame");
            }
        }

        audio_output_stream.play();
        loop {
            select! {
                Some(raw_message) = midi_input_stream.get_message_stream().next() => {
                    synth.handle_midi_message(raw_message);
                },
                item = audio_output_stream.get_buffer_request_rx().recv() => {
                    item.expect("Couldn't receive buffer request.");
                    let frame = synth.sample_notes(num_channels as usize);
                    if frame_tx.send(frame).is_err() {
                        panic!("Failed to send audio frame");
                    }
                },
                item = signal::ctrl_c() => {
                    item.expect("Couldn't receive cancellation.");
                    info!("Interrupted instrument");
                    break;
                }
            };
        }
        audio_output_stream.pause();

        // Tear down.
        debug!("Waiting for MIDI input stream tear down");
        midi_input_stream.close();
        debug!("Waiting for recorder to drain");
        if let Some(r) = recorder {
            r.close().await
        }
    }
}
