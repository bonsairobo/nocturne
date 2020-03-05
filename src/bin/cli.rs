use nocturne::{
    list_midi_input_ports, play_midi_device, play_all_midi_tracks, MidiBytes
};

use std::path::PathBuf;
use structopt::StructOpt;
use tokio::{stream::StreamExt, sync::broadcast};

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

    // TODO: figure out how to do cancelling on ctrlc (for clean exit of file writer task)
    let (cancel_tx, _) = broadcast::channel(1);

    let opt = Opt::from_args();
    match opt {
        Opt::ListMidiPorts => { list_midi_input_ports(); },
        Opt::PlayDevice {
            midi_input_port,
            recording_path,
        } => {
            let cancel_rx = cancel_tx.subscribe().map(|_| ());
            runtime.block_on(play_midi_device(
                midi_input_port,
                cancel_rx,
                recording_path,
            ));
        },
        Opt::PlayFile {
            midi_path,
            recording_path, // TODO: support recording (requires mixing)
        } => {
            runtime.block_on(play_all_midi_tracks(MidiBytes::read_file(&midi_path), &cancel_tx));
        }
    }
}
