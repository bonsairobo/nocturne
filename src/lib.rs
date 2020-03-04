mod audio_device;
mod instrument;
mod midi;
mod recording;
mod synthesizer;
mod wave_table;

const FRAME_SIZE: usize = 512;

const CHANNEL_MAX_BUFFER: usize = 50;

type AudioFrame = [f32; FRAME_SIZE];

pub use audio_device::AudioOutputDeviceStream;
pub use instrument::Instrument;
pub use midi::{
    list_midi_input_ports, MidiInputDeviceStream, MidiTrackInputStream, RawMidiMessage,
};
pub use recording::RecordingOutputStream;
pub use synthesizer::Synthesizer;
