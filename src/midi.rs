use crossbeam_channel as channel;
use crossbeam_channel::{Receiver, Sender};
use log::{info, trace};
use midly::Smf;
use pitch_calc::Step;
use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;
use time_calc::{Bpm, Ppqn, Ticks};

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
    /// Get the receiver for incoming messages.
    fn get_message_rx(&self) -> &Receiver<RawMidiMessage>;

    /// Stop and tear down stream.
    fn close(self);
}

pub struct MidiInputDeviceStream {
    connection: midir::MidiInputConnection<()>,
    message_rx: Receiver<RawMidiMessage>,
}

impl MidiInputDeviceStream {
    pub fn connect(port: usize) -> Self {
        let (message_tx, message_rx) = channel::unbounded();

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
                    message_tx
                        .send((timestamp, message_copy))
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
    fn get_message_rx(&self) -> &Receiver<RawMidiMessage> {
        &self.message_rx
    }

    fn close(self) {
        self.connection.close();
    }
}

pub struct MidiTrackInputStream {
    message_rx: Receiver<RawMidiMessage>,
    exit_tx: Sender<()>,
}

impl MidiTrackInputStream {
    pub fn start(midi_file_path: PathBuf, track_num: usize) -> Self {
        let (message_tx, message_rx) = channel::unbounded();
        let (exit_tx, exit_rx) = channel::unbounded();

        thread::spawn(move || {
            quantize_midi_track_thread(midi_file_path, track_num, message_tx, exit_rx)
        });

        MidiTrackInputStream {
            message_rx,
            exit_tx,
        }
    }
}

fn quantize_midi_track_thread(
    midi_file_path: PathBuf,
    track_num: usize,
    message_tx: Sender<RawMidiMessage>,
    exit_rx: Receiver<()>,
) {
    let mut bytes = Vec::new();
    let mut file = fs::File::open(&midi_file_path).unwrap();
    file.read_to_end(&mut bytes)
        .expect("Failed to read MIDI file");

    let smf = Smf::parse(&bytes).unwrap();

    let bpm: Bpm = 120.0;
    let ppqn = match smf.header.timing {
        midly::Timing::Metrical(m) => m.as_int() as Ppqn,
        midly::Timing::Timecode(_, _) => panic!("WTF is a timecode"),
    };

    for event in smf.tracks[track_num].iter() {
        if exit_rx.try_recv().is_ok() {
            break;
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
            .expect("Failed to send MIDI message");

        // Sleep until next event.
        let delta_ticks = Ticks(delta.as_int() as i64);
        let millis = delta_ticks.ms(bpm, ppqn);
        let mut nanos = (millis * 1_000_000.0).floor() as u64;
        let seconds = nanos / 1_000_000_000;
        nanos -= seconds * 1_000_000;
        spin_sleep::sleep(Duration::new(seconds as u64, nanos as u32));
    }

    info!("Exiting MIDI file playback thread")
}

impl MidiInputStream for MidiTrackInputStream {
    fn get_message_rx(&self) -> &Receiver<RawMidiMessage> {
        &self.message_rx
    }

    fn close(self) {
        self.exit_tx
            .send(())
            .expect("Failed to stop midi track thread")
    }
}
