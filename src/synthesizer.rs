use crate::{
    filters::ExponentialSmoothing,
    midi::{get_midi_key_hz, RawMidiMessage},
    wave_table::{self, WaveTableIndex},
    AudioFrame, FRAME_SIZE,
};

use log::{info, trace};
use std::collections::HashMap;
use std::convert::TryFrom;
use wmidi::MidiMessage;

// TODO: replace attack/decay with envelopes
// TODO: legato polyphony

pub struct Synthesizer {
    sample_hz: f32,
    notes_playing: HashMap<wmidi::Note, SynthNote>,
    filter: ExponentialSmoothing,
}

impl Synthesizer {
    pub fn new(sample_hz: f32) -> Self {
        Self {
            sample_hz,
            notes_playing: HashMap::new(),
            filter: ExponentialSmoothing::new(0.05),
        }
    }

    pub fn handle_midi_message(&mut self, (_timestamp, message): RawMidiMessage) {
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
            other => {
                trace!("unsupported MIDI message = {:?}", other);
                self.notes_playing = HashMap::new();
            }
        }
    }

    pub fn sample_notes(&mut self, num_channels: usize) -> AudioFrame {
        let oscillator = &wave_table::get_triangle_wave();
        let mut frame = [0.0; FRAME_SIZE];
        let samples_per_frame = FRAME_SIZE / num_channels;
        let mut i = 0;
        for _ in 0..samples_per_frame {
            let mut mixed_notes_sample = 0.0;
            for (_, note) in self.notes_playing.iter_mut() {
                // TODO: scale down note sample generator instead of clipping
                mixed_notes_sample += note.sample_table(oscillator).min(1.0);
            }
            let filtered_sample = self.filter.apply(mixed_notes_sample);

            for _ in 0..num_channels {
                frame[i] = filtered_sample;
                i += 1;
            }
        }

        let mut remove_keys = vec![];
        for (key, note) in self.notes_playing.iter_mut() {
            note.update_after_sample();
            if note.is_done_playing() {
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
                table_index: WaveTableIndex::from_hz(self.sample_hz, get_midi_key_hz(key)),
                stop_requested: false,
                off_decay_factor: 1.0,
                online_decay_factor: 1.0,
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
}

struct SynthNote {
    table_index: WaveTableIndex,
    attack_factor: f32,
    off_decay_factor: f32,
    online_decay_factor: f32,
    velocity: f32,
    stop_requested: bool,
}

impl SynthNote {
    fn amplitude(&self) -> f32 {
        0.2 * self.attack_factor * self.online_decay_factor * self.off_decay_factor * self.velocity
    }

    fn sample_table(&mut self, table: &[f32]) -> f32 {
        self.amplitude() * self.table_index.sample_table(table)
    }

    fn update_after_sample(&mut self) {
        self.online_decay_factor -= 0.005;
        if self.stop_requested {
            self.off_decay_factor -= 0.05;
        }
        if self.attack_factor < 1.0 {
            self.attack_factor += 0.5;
        }
    }

    fn is_done_playing(&self) -> bool {
        self.off_decay_factor < 0.05 || self.online_decay_factor < 0.05
    }
}
