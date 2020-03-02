use crossbeam_channel as channel;
use crossbeam_channel::Receiver;
use pitch_calc::Step;

pub fn get_midi_key_hz(key: wmidi::Note) -> f32 {
    // PERF: compute note frequencies on synth creation.
    Step(u8::from(key) as f32).to_hz().0 as f32
}

pub fn list_midi_input_ports() {
    let midi_in = midir::MidiInput::new("nocturne_midi_temporary")
        .expect("Failed to load MIDI input");
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

pub struct MidiInputStream {
    connection: midir::MidiInputConnection<()>,
    message_rx: Receiver<RawMidiMessage>,
}

impl MidiInputStream {
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
                    message_tx.send((timestamp, message_copy))
                        .expect("Failed to send MIDI message");
                },
                (),
            )
            .expect("Failed to open MIDI input connection.");

        MidiInputStream { connection, message_rx }
    }

    pub fn close(self) -> midir::MidiInput {
        self.connection.close().0
    }

    pub fn get_message_rx(&self) -> &Receiver<RawMidiMessage> {
        &self.message_rx
    }
}
