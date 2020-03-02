use nocturne::{NocturneServer, list_midi_input_ports};

use std::path::PathBuf;
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
#[structopt(name = "cli")]
enum Opt {
    List,
    Run {
        #[structopt(short = "p", long = "port")]
        midi_input_port: usize,

        #[structopt(short = "r", long = "recording", parse(from_os_str))]
        recording_path: Option<PathBuf>,
    }
}

// TODO: return Result
// TODO: use paw
fn main() {
    env_logger::init();

    let opt = Opt::from_args();

    match opt {
        Opt::List => list_midi_input_ports(),
        Opt::Run { midi_input_port, recording_path } => {
            let server = NocturneServer::new(midi_input_port, recording_path);
            server.run();
        }
    }
}
