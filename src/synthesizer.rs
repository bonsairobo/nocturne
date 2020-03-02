use crate::{
    audio_device::AudioOutputDeviceStream,
    midi::{get_midi_key_hz, MidiInputStream, RawMidiMessage},
    recording::RecordingOutputStream,
    wave_table::{self, WaveTableIndex},
    AudioFrame, CHANNELS, FRAME_SIZE, SAMPLES_PER_FRAME,
};

use crossbeam_channel::select;
use log::{info, trace};
use std::collections::HashMap;
use std::convert::TryFrom;
use wmidi::MidiMessage;

// The synthesizer thread will attempt to queue samples ahead of the audio output thread. This
// represents an additional fixed latency of 5 buffers * 512 samples per channel * (1 / 44100)
// seconds = 0.06 seconds.
const BUFFERS_AHEAD: u32 = 5;

// TODO: extract instrument interface
// TODO: replace attack/decay with envelopes
// TODO: legato polyphony

pub struct Synthesizer<M> {
    midi_input: M,
    output_device: AudioOutputDeviceStream,
    recording: Option<RecordingOutputStream>,
    notes_playing: HashMap<wmidi::Note, SynthNote>,
}

impl<M: MidiInputStream> Synthesizer<M> {
    pub fn new(
        midi_input: M,
        output_device: AudioOutputDeviceStream,
        recording: Option<RecordingOutputStream>,
    ) -> Synthesizer<M> {
        Self {
            midi_input,
            output_device,
            recording,
            notes_playing: HashMap::new(),
        }
    }

    pub fn buffer_ahead(&mut self) {
        // Get ahead of the CPAL buffering.
        for _ in 0..BUFFERS_AHEAD {
            self.sample_notes();
        }
    }

    pub fn start_output_device(&self) {
        self.output_device.play();
    }

    pub fn handle_events(&mut self) {
        select! {
            recv(self.midi_input.get_message_rx()) -> item => {
                let raw_message = item.expect("Couldn't receive MIDI message.");
                self.handle_midi_message(raw_message);
            },
            recv(self.output_device.get_buffer_request_rx()) -> item => {
                item.expect("Couldn't receive buffer request.");
                self.send_frame();
            }
        }
    }

    fn handle_midi_message(&mut self, raw_message: RawMidiMessage) {
        let (_timestamp, message) = raw_message;
        // TODO: replace with midly::Event::read
        let message = MidiMessage::try_from(&message[..]).expect("Failed to parse MIDI message.");

        match message {
            MidiMessage::NoteOn(_, key, velocity) => {
                info!("NoteOn key = {} vel = {:?}", key, velocity);
                if u8::from(velocity) == 0 {
                    self.stop_key(key);
                } else {
                    self.start_note(key, velocity);
                }
            }
            MidiMessage::NoteOff(_, key, _) => {
                info!("NoteOff key = {}", key);
                self.stop_key(key);
            }
            MidiMessage::TimingClock => (),
            MidiMessage::ActiveSensing => (),
            other => trace!("unsupported MIDI message = {:?}", other),
        }
    }

    fn send_frame(&mut self) {
        let frame = self.sample_notes();

        // TODO: abstract separate outputs
        if let Some(recording) = self.recording.as_ref() {
            self.output_device.write_frame(frame.clone());
            recording.write_frame(frame);
        } else {
            self.output_device.write_frame(frame.clone());
        }
    }

    fn sample_notes(&mut self) -> AudioFrame {
        let oscillator = &wave_table::get_sine_wave();
        let mut remove_keys = vec![];
        let mut frame = [0.0; FRAME_SIZE];
        for (key, note) in self.notes_playing.iter_mut() {
            let mut i = 0;
            for _ in 0..SAMPLES_PER_FRAME {
                // TODO: scale down note sample generator instead of clipping
                let sample = note.sample_table(oscillator).min(1.0);
                // TODO: configurable # of channels
                for _ in 0..CHANNELS {
                    frame[i] += sample;
                    i += 1;
                }
            }

            note.update_after_buffer();
            if note.done_playing() {
                remove_keys.push(*key);
            }
        }

        for key in remove_keys {
            self.notes_playing.remove(&key);
        }

        frame
    }

    fn start_note(&mut self, key: wmidi::Note, velocity: wmidi::U7) {
        self.notes_playing.insert(
            key,
            SynthNote {
                table_index: WaveTableIndex::from_hz(get_midi_key_hz(key)),
                stop_requested: false,
                decay_factor: 1.0,
                attack_factor: 0.0,
                velocity: u8::from(velocity) as f32 / 100.0,
            },
        );
    }

    fn stop_key(&mut self, key: wmidi::Note) {
        if let Some(n) = self.notes_playing.get_mut(&key) {
            n.stop_requested = true;
        }
    }

    pub fn close(self) {
        self.recording.map(|r| r.close());
        self.midi_input.close();
    }
}

struct SynthNote {
    table_index: WaveTableIndex,
    attack_factor: f32,
    decay_factor: f32,
    velocity: f32,
    stop_requested: bool,
}

impl SynthNote {
    fn amplitude(&self) -> f32 {
        // BUG: there is some artifacting on the attack/release of notes, likely caused here
        0.2 * self.attack_factor * self.decay_factor * self.velocity
    }

    fn sample_table(&mut self, table: &[f32]) -> f32 {
        self.amplitude() * self.table_index.sample_table(table)
    }

    fn update_after_buffer(&mut self) {
        if self.stop_requested {
            self.decay_factor -= 0.05;
        }
        if self.attack_factor < 1.0 {
            self.attack_factor += 0.02;
        }
    }

    fn done_playing(&self) -> bool {
        self.decay_factor < 0.05
    }
}
