use nocturne::{list_midi_input_ports, Instrument};

use crossbeam_channel as channel;
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
// TODO: use paw
fn main() {
    env_logger::init();

    let opt = Opt::from_args();

    // Set SIGINT handler.
    let (exit_tx, exit_rx) = channel::bounded(1);
    ctrlc::set_handler(move || {
        exit_tx.send(()).expect("Failed to send exit signal");
    })
    .expect("Error setting Ctrl-C handler");

    match opt {
        Opt::ListMidiPorts => list_midi_input_ports(),
        Opt::PlayDevice {
            midi_input_port,
            recording_path,
        } => {
            let instrument = Instrument::new(exit_rx, recording_path);
            instrument.run_midi_device(midi_input_port);
        }
        Opt::PlayFile {
            midi_path,
            recording_path,
        } => {
            let instrument = Instrument::new(exit_rx, recording_path);
            instrument.run_midi_file(midi_path);
        }
    }
}
