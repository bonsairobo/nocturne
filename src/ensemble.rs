use crate::{
    instrument::play_midi,
    midi::{quantize_midi_tracks, MidiBytes},
    CHANNEL_MAX_BUFFER
};

use futures::future::join_all;
use tokio::{stream::StreamExt, sync::{broadcast, mpsc}, task};

pub async fn play_all_midi_tracks(
    midi_bytes: MidiBytes, cancel_tx: &broadcast::Sender<()>
) {
    let smf = midi_bytes.parse();

    let mut handles = Vec::with_capacity(smf.tracks.len() + 1);

    // Each track plays an instrument which runs in its own task.
    let mut track_message_txs = Vec::with_capacity(smf.tracks.len());
    for (i, _track) in smf.tracks.iter().enumerate() {
        // HACK: some of the test tracks are just sitting on one note
        if i > 6 {
            break;
        }

        let cancel_rx = cancel_tx.subscribe().map(|_| ());
        let (message_tx, message_rx) = mpsc::channel(CHANNEL_MAX_BUFFER);
        handles.push(task::spawn(async move {
            play_midi(message_rx, cancel_rx, None).await;
        }));
        track_message_txs.push(message_tx);
    }

    // One task produces the MIDI input streams for all tracks.
    let cancel_rx = cancel_tx.subscribe().map(|_| ());
    handles.push(task::spawn(async move {
        quantize_midi_tracks(midi_bytes, track_message_txs, cancel_rx).await;
    }));

    join_all(handles).await;
}
