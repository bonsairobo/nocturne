use nocturne::{
    list_midi_input_ports, play_all_midi_tracks, play_midi_device, wave_table, MidiBytes
};

use std::path::PathBuf;
use structopt::StructOpt;
use tokio::{select, signal, stream::StreamExt, sync::broadcast};

#[derive(StructOpt, Debug)]
#[structopt(name = "cli")]
enum Opt {
    ListMidiPorts,
    PlayDevice {
        #[structopt(short = "p", long = "port")]
        midi_input_port: usize,

        #[structopt(short = "r", long = "recording", parse(from_os_str))]
        recording_path: Option<PathBuf>,
    },
    PlayFile {
        #[structopt(short = "m", long = "midi", parse(from_os_str))]
        midi_path: PathBuf,

        #[structopt(short = "r", long = "recording", parse(from_os_str))]
        recording_path: Option<PathBuf>,
    },
}

// TODO: return Result
fn main() {
    env_logger::init();

    let mut runtime = tokio::runtime::Builder::new()
        .threaded_scheduler()
        .enable_all()
        .build()
        .unwrap();

    let opt = Opt::from_args();
    match opt {
        Opt::ListMidiPorts => {
            list_midi_input_ports();
        }
        Opt::PlayDevice {
            midi_input_port,
            recording_path,
        } => {
            runtime.block_on(async move {
                select! {
                    result = play_midi_device(
                        midi_input_port, wave_table::triangle_wave(), recording_path
                    ) => {
                        match result {
                            Err(e) => {
                                println!(
                                    "Failed to open midi port {}, try the list-midi-ports command: \
                                     {}",
                                    midi_input_port,
                                    e,
                                );
                            }
                            Ok(()) => (),
                        }
                    },
                    _ = signal::ctrl_c() => (),
                }
            })
        }
        Opt::PlayFile {
            midi_path,
            recording_path, // TODO: support recording (requires mixing)
        } => {
            let instruments = [
                wave_table::sawtooth_wave(),
                wave_table::sine_wave(),
                wave_table::triangle_wave(),
                wave_table::square_wave(),
            ];
            runtime.block_on(async move {
                select! {
                    _ = play_all_midi_tracks(
                        MidiBytes::read_file(&midi_path), &instruments
                    ) => (),
                    _ = signal::ctrl_c() => (),
                }
            });
        }
    }
}
