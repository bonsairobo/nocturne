mod audio_device;
mod ensemble;
mod filters;
mod instrument;
mod midi;
mod recording;
mod synthesizer;
pub mod wave_table;

/// Static sized frames for all internal audio buffering. (External frames are configurable by the
/// audio devices).
const FRAME_SIZE: usize = 512;
type AudioFrame = [f32; FRAME_SIZE];

const CHANNEL_MAX_BUFFER: usize = 50;

pub use audio_device::AudioOutputDeviceStream;
pub use ensemble::play_all_midi_tracks;
pub use instrument::{play_midi, play_midi_device};
pub use midi::{
    list_midi_input_ports, quantize_midi_tracks, single_timeline_of_events, ticks_to_duration,
    MidiBytes, MidiInputDeviceStream, RawMidiMessage,
};
pub use recording::RecordingOutputStream;
pub use synthesizer::Synthesizer;
pub use wave_table::{sawtooth_wave, sine_wave, square_wave, triangle_wave, Wave};
