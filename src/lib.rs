mod audio_device;
mod midi;
mod recording;
mod server;
mod synthesizer;
mod wave_table;

// TODO: use output stream to configure these parameters
const SAMPLE_HZ: f32 = 44_100.0;
const CHANNELS: i32 = 2;

const FRAME_SIZE: usize = 1024;
const SAMPLES_PER_FRAME: u32 = FRAME_SIZE as u32 / CHANNELS as u32;

type AudioFrame = [f32; FRAME_SIZE];

pub use midi::list_midi_input_ports;
pub use server::NocturneServer;
