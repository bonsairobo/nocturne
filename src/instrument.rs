use crate::{
    audio_device::AudioOutputDeviceStream,
    midi::{MidiInputDeviceStream, RawMidiMessage},
    recording::RecordingOutputStream,
    synthesizer::Synthesizer,
    CHANNEL_MAX_BUFFER,
};

use cpal::SampleRate;
use log::{debug, info};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::{
    select,
    stream::{Stream, StreamExt},
    sync::{broadcast, mpsc},
};

/// Need to synchronize access to the stream, since it is !Send, and we want to use it across
/// awaits (threads).
struct SafeAudioStream {
    stream: Arc<Mutex<AudioOutputDeviceStream>>,
}

impl SafeAudioStream {
    fn new(stream: AudioOutputDeviceStream) -> Self {
        SafeAudioStream {
            stream: Arc::new(Mutex::new(stream)),
        }
    }

    fn play(&self) {
        self.stream.lock().unwrap().play();
    }

    fn pause(&self) {
        self.stream.lock().unwrap().pause();
    }
}

unsafe impl Send for SafeAudioStream {}

pub async fn play_midi_device<C>(
    midi_input_port: usize,
    cancel_stream: C,
    recording_path: Option<PathBuf>,
)
where
    C: Stream<Item = ()> + Unpin,
{
    let midi_input = MidiInputDeviceStream::connect(midi_input_port);
    play_midi(midi_input.message_rx, cancel_stream, recording_path).await;
}

/// Plays the MIDI input on a synth until there is no input left or we are cancelled.
pub async fn play_midi<S, C>(
    mut midi_input_stream: S,
    mut cancel_stream: C,
    recording_path: Option<PathBuf>,
)
where
    S: Stream<Item = RawMidiMessage> + Unpin,
    C: Stream<Item = ()> + Unpin,
{
    // Audio output can have many subscribers.
    let (frame_tx, device_frame_rx) = broadcast::channel(CHANNEL_MAX_BUFFER);
    let (buffer_request_tx, mut buffer_request_rx) = mpsc::channel(CHANNEL_MAX_BUFFER);

    // Create the synth and output stream.
    let (mut synth, recorder, audio_output_stream, num_channels) = {
        let audio_output_stream =
            AudioOutputDeviceStream::connect_default(device_frame_rx, buffer_request_tx);
        let num_channels = audio_output_stream.get_config().channels;
        let SampleRate(sample_hz) = audio_output_stream.get_config().sample_rate;
        let recorder = recording_path.as_ref().map(|p| {
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

        (
            synth,
            recorder,
            SafeAudioStream::new(audio_output_stream),
            num_channels,
        )
    };

    audio_output_stream.play();
    loop {
        select! {
            maybe_raw_message = midi_input_stream.next() => {
                if let Some(raw_message) = maybe_raw_message {
                    synth.handle_midi_message(raw_message);
                } else {
                    break;
                }
            },
            item = buffer_request_rx.recv() => {
                item.expect("Couldn't receive buffer request.");
                let frame = synth.sample_notes(num_channels as usize);
                if frame_tx.send(frame).is_err() {
                    panic!("Failed to send audio frame");
                }
            },
            _ = cancel_stream.next() => {
                info!("Interrupted instrument");
                break;
            }
        };
    }
    audio_output_stream.pause();

    // Tear down.
    debug!("Waiting for recorder to drain");
    if let Some(r) = recorder {
        r.close().await;
    }
}
