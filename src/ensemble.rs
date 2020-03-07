use crate::{
    instrument::play_midi,
    midi::{quantize_midi_tracks, MidiBytes},
    wave_table::Wave,
    CHANNEL_MAX_BUFFER,
};

use futures::future::join_all;
use log::{debug, info};
use tokio::{
    stream::StreamExt,
    sync::{broadcast, mpsc},
    task,
};

pub async fn play_all_midi_tracks(
    midi_bytes: MidiBytes,
    track_instruments: &[Wave],
    cancel_tx: &broadcast::Sender<()>,
) {
    let smf = midi_bytes.parse();

    let mut handles = Vec::with_capacity(smf.tracks.len() + 1);

    // Each track plays an instrument which runs in its own task.
    let mut track_message_txs = Vec::with_capacity(smf.tracks.len());
    for (track_i, track) in smf.tracks.iter().enumerate() {
        let cancel_rx = cancel_tx.subscribe().map(|_| ());
        let (message_tx, message_rx) = mpsc::channel(CHANNEL_MAX_BUFFER);
        let instrument_i = track_i % track_instruments.len();
        info!("Starting track {} with instrument {}", track_i, instrument_i);
        let wave = track_instruments[instrument_i];
        handles.push(task::spawn(async move {
            play_midi(message_rx, wave, cancel_rx, None).await;
        }));
        track_message_txs.push(message_tx);

        debug!("Track {} has {} events", track_i, track.len());
    }

    // One task produces the MIDI input streams for all tracks.
    let cancel_rx = cancel_tx.subscribe().map(|_| ());
    handles.push(task::spawn(async move {
        quantize_midi_tracks(midi_bytes, track_message_txs, cancel_rx).await;
    }));

    join_all(handles).await;
}
