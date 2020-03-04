use crate::CHANNEL_MAX_BUFFER;

use futures::executor::block_on;
use log::{error, info, trace};
use midly::Smf;
use pitch_calc::Step;
use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::time::Duration;
use time_calc::{Bpm, Ppqn, Ticks};
use tokio::{
    stream::Stream,
    sync::{
        mpsc,
        oneshot::{self, error::TryRecvError},
    },
    task,
    time::delay_for,
};

pub fn get_midi_key_hz(key: wmidi::Note) -> f32 {
    // PERF: compute note frequencies on synth creation.
    Step(u8::from(key) as f32).to_hz().0 as f32
}

pub fn list_midi_input_ports() {
    let midi_in =
        midir::MidiInput::new("nocturne_midi_temporary").expect("Failed to load MIDI input");
    println!("--- Available MIDI input ports ---");
    for i in 0..midi_in.port_count() {
        println!(
            "{}: {}",
            i,
            midi_in.port_name(i).expect("Failed to get MIDI port name")
        );
    }
}

pub type RawMidiMessage = (u64, [u8; 3]);

pub trait MidiInputStream {
    type MessageStream: Stream<Item = RawMidiMessage>;

    /// Get the stream of incoming messages.
    fn get_message_stream(&mut self) -> &mut Self::MessageStream;

    /// Stop and tear down stream. Blocks the current thread.
    fn close(self);
}

pub struct MidiInputDeviceStream {
    connection: midir::MidiInputConnection<()>,
    message_rx: mpsc::Receiver<RawMidiMessage>,
}

impl MidiInputDeviceStream {
    pub fn connect(port: usize) -> Self {
        let (mut message_tx, message_rx) = mpsc::channel(CHANNEL_MAX_BUFFER);

        let mut midi_in = midir::MidiInput::new(&format!("nocturne_midi_{}", port))
            .expect("Failed to create MIDI input");
        midi_in.ignore(midir::Ignore::None);

        // QUESTION: do MIDI messages arrive in timestamp order?
        let connection = midi_in
            .connect(
                port,
                "midi_input_connection",
                move |timestamp, message, _| {
                    let mut message_copy: [u8; 3] = [0; 3];
                    message_copy.copy_from_slice(&message);
                    block_on(message_tx.send((timestamp, message_copy)))
                        .expect("Failed to send MIDI message");
                },
                (),
            )
            .expect("Failed to open MIDI input connection.");

        MidiInputDeviceStream {
            connection,
            message_rx,
        }
    }
}

impl MidiInputStream for MidiInputDeviceStream {
    type MessageStream = mpsc::Receiver<RawMidiMessage>;

    fn get_message_stream(&mut self) -> &mut mpsc::Receiver<RawMidiMessage> {
        &mut self.message_rx
    }

    fn close(self) {
        self.connection.close();
    }
}

pub struct MidiTrackInputStream {
    message_rx: mpsc::Receiver<RawMidiMessage>,
    exit_tx: oneshot::Sender<()>,
    join_handle: task::JoinHandle<()>,
}

impl MidiTrackInputStream {
    pub fn start(midi_bytes: MidiBytes, track_num: usize) -> Self {
        let (message_tx, message_rx) = mpsc::channel(CHANNEL_MAX_BUFFER);
        let (exit_tx, exit_rx) = oneshot::channel();

        let join_handle = task::spawn(async move {
            quantize_midi_track_task(midi_bytes, track_num, message_tx, exit_rx).await
        });

        MidiTrackInputStream {
            message_rx,
            exit_tx,
            join_handle,
        }
    }
}

async fn quantize_midi_track_task(
    midi_bytes: MidiBytes,
    track_num: usize,
    mut message_tx: mpsc::Sender<RawMidiMessage>,
    mut exit_rx: oneshot::Receiver<()>,
) {
    let smf = midi_bytes.parse();

    // TODO: configurable/dynamic BPM
    let bpm: Bpm = 120.0;
    let ppqn = match smf.header.timing {
        midly::Timing::Metrical(m) => m.as_int() as Ppqn,
        midly::Timing::Timecode(_, _) => panic!("WTF is a timecode"),
    };

    for event in smf.tracks[track_num].iter() {
        match exit_rx.try_recv() {
            Err(TryRecvError::Empty) => (),
            Ok(()) => break,
            Err(TryRecvError::Closed) => {
                error!("Cancellation channel to recording task close early");
                break;
            }
        }

        let midly::Event { delta, kind } = event;
        let timestamp = 0; // TODO?
        let mut raw_message = Vec::with_capacity(3);
        kind.write(&mut None, &mut raw_message)
            .expect("Failed to serialize MIDI message");
        let mut raw_message_buf = [0u8; 3];
        let message_len = raw_message.len();
        if message_len > 3 {
            // HACK: ignore certain events
            trace!("Ignoring {}-byte event {:?}", message_len, kind);
            continue;
        }
        raw_message_buf[..message_len].copy_from_slice(&raw_message[..]);
        message_tx
            .send((timestamp, raw_message_buf))
            .await
            .expect("Failed to send MIDI message");

        // Sleep until next event.
        let delta_ticks = Ticks(delta.as_int() as i64);
        let millis = delta_ticks.ms(bpm, ppqn);
        let mut nanos = (millis * 1_000_000.0).floor() as u64;
        let seconds = nanos / 1_000_000_000;
        nanos -= seconds * 1_000_000;

        delay_for(Duration::new(seconds as u64, nanos as u32)).await;
    }

    info!("Exiting MIDI file playback thread")
}

impl MidiInputStream for MidiTrackInputStream {
    type MessageStream = mpsc::Receiver<RawMidiMessage>;

    fn get_message_stream(&mut self) -> &mut mpsc::Receiver<RawMidiMessage> {
        &mut self.message_rx
    }

    fn close(self) {
        self.exit_tx
            .send(())
            .expect("Failed to interrupt midi track task");
        block_on(self.join_handle).expect("Failed to join on MIDI track task");
    }
}

#[derive(Clone)]
pub struct MidiBytes {
    bytes: Vec<u8>,
}

impl Send for MidiBytes {}

impl MidiBytes {
    pub fn read_file(midi_file_path: &PathBuf) -> Self {
        let mut bytes = Vec::new();
        let mut file = fs::File::open(midi_file_path).unwrap();
        file.read_to_end(&mut bytes)
            .expect("Failed to read MIDI file");

        MidiBytes { bytes }
    }

    pub fn parse<'a>(&'a self) -> Smf<'a> {
        Smf::parse(&self.bytes).unwrap()
    }
}

pub fn num_tracks_in_midi_file(midi_bytes: &MidiBytes) -> usize {
    midi_bytes.parse().tracks.len()
}
