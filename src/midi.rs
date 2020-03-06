use crate::CHANNEL_MAX_BUFFER;

use futures::executor::block_on;
use log::{info, trace};
use midly::Smf;
use pitch_calc::Step;
use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::time::Duration;
use time_calc::{Bpm, Ppqn, Ticks};
use tokio::{
    select,
    stream::{Stream, StreamExt},
    sync::mpsc,
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

pub struct MidiInputDeviceStream {
    pub connection: midir::MidiInputConnection<()>,
    pub message_rx: mpsc::Receiver<RawMidiMessage>,
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

#[derive(Clone)]
pub struct MidiBytes {
    bytes: Vec<u8>,
}

impl MidiBytes {
    pub fn read_file(midi_file_path: &PathBuf) -> Self {
        let mut bytes = Vec::new();
        let mut file = fs::File::open(midi_file_path).unwrap();
        file.read_to_end(&mut bytes)
            .expect("Failed to read MIDI file");

        MidiBytes { bytes }
    }

    pub fn parse(&self) -> Smf<'_> {
        Smf::parse(&self.bytes).unwrap()
    }
}

/// Sequences, in real time, every MIDI event for every track in the SMF.
pub async fn quantize_midi_tracks<C>(
    midi_bytes: MidiBytes,
    mut track_message_txs: Vec<mpsc::Sender<RawMidiMessage>>,
    mut cancel_stream: C,
) where
    C: Stream<Item = ()> + Unpin,
{
    let smf = midi_bytes.parse();

    // TODO: configurable/dynamic BPM
    let bpm: Bpm = 120.0;
    let ppqn = match smf.header.timing {
        midly::Timing::Metrical(m) => m.as_int() as Ppqn,
        midly::Timing::Timecode(_, _) => panic!("WTF is a timecode"),
    };

    // Collapse the events into one queue and sort them by absolute timestamp.
    let all_events = single_timeline_of_events(&smf);
    if all_events.is_empty() {
        return;
    }

    let num_events = all_events.len();
    let mut i = 0;
    while i < num_events - 1 {
        // Send all of the events that happen at the same time.
        let mut delta_t = 0;
        while delta_t == 0 && i < num_events - 1 {
            let (this_t, this_track, this_event) = all_events[i];
            let (next_t, _, _) = all_events[i + 1];

            send_event_to_track(
                this_t as u64,
                &this_event,
                &mut track_message_txs[this_track],
            )
            .await;

            // We'll end iteration if the next event does not start at the same time.
            delta_t = next_t - this_t;

            // Move to the next pair of adjacent events.
            i += 1;
        }

        // Sleep until next event.
        select! {
            _ = cancel_stream.next() => break,
            _ = delay_for(ticks_to_duration(bpm, ppqn, delta_t)) => (),
        }
    }

    // Send the last event.
    let (this_t, this_track, this_event) = all_events[i];
    send_event_to_track(
        this_t as u64,
        this_event,
        &mut track_message_txs[this_track],
    )
    .await;

    info!("Exiting MIDI file playback thread")
}

pub fn ticks_to_duration(bpm: Bpm, ppqn: Ppqn, delta_t: i64) -> Duration {
    let delta_ticks = Ticks(delta_t);
    let millis = delta_ticks.ms(bpm, ppqn);
    let mut nanos = (millis * 1_000_000.0).floor() as u64;
    let seconds = nanos / 1_000_000_000;
    nanos -= seconds * 1_000_000;

    Duration::new(seconds as u64, nanos as u32)
}

pub fn single_timeline_of_events<'a>(smf: &'a Smf<'a>) -> Vec<(i64, usize, &'a midly::Event<'a>)> {
    let mut all_events = Vec::new();
    for (track_num, track) in smf.tracks.iter().enumerate() {
        let mut abs_t: i64 = 0;
        for event in track.iter() {
            let delta_t = event.delta;
            all_events.push((abs_t, track_num, event));
            abs_t += delta_t.as_int() as i64;
        }
    }
    all_events.sort_by(|(t1, _, _), (t2, _, _)| t1.cmp(&t2));

    all_events
}

fn convert_event_to_raw_message(event: &midly::Event<'_>) -> Option<[u8; 3]> {
    let mut raw_message = Vec::with_capacity(3);
    event
        .kind
        .write(&mut None, &mut raw_message)
        .expect("Failed to serialize MIDI message");
    let message_len = raw_message.len();

    if message_len <= 3 {
        let mut raw_message_buf = [0u8; 3];
        raw_message_buf[..message_len].copy_from_slice(&raw_message[..]);

        Some(raw_message_buf)
    } else {
        // HACK: ignore certain events
        trace!("Ignoring {}-byte event {:?}", message_len, event.kind);

        None
    }
}

async fn send_event_to_track(
    timestamp: u64,
    event: &midly::Event<'_>,
    message_tx: &mut mpsc::Sender<RawMidiMessage>,
) {
    // Send one event to the corresponding track receiver.
    if let Some(raw_message) = convert_event_to_raw_message(event) {
        message_tx
            .send((timestamp, raw_message))
            .await
            .expect("Failed to send MIDI message");
    }
}
