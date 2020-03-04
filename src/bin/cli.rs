use nocturne::{list_midi_input_ports, Instrument};

use std::path::PathBuf;
use structopt::StructOpt;

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

    let opt = Opt::from_args();

    let mut runtime = tokio::runtime::Builder::new()
        .threaded_scheduler()
        .enable_time()
        .enable_io()
        .build()
        .unwrap();

    match opt {
        Opt::ListMidiPorts => list_midi_input_ports(),
        Opt::PlayDevice {
            midi_input_port,
            recording_path,
        } => {
            let instrument = Instrument::new(recording_path);

            // Run the synth.
            runtime.block_on(async move { instrument.play_midi_device(midi_input_port).await });
        }
        Opt::PlayFile {
            midi_path,
            recording_path,
        } => {
            let instrument = Instrument::new(recording_path);
            runtime.block_on(async move { instrument.play_midi_file(midi_path).await });
        }
    }
}
