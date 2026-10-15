// Sound: plays the original WAV resources ("Tabs%04d.Wav", like the
// original sndPlaySound) through rodio.

use std::collections::HashMap;
use std::io::Cursor;
use std::sync::mpsc::Receiver;

/// Runs on a dedicated thread: each PlaySound plays the embedded WAV
/// (fire and forget, like SND_ASYNC). Sounds are cached after first use.
pub fn run(rx: Receiver<u16>) {
    let (_stream, handle) = match rodio::OutputStream::try_default() {
        Ok(pair) => pair,
        Err(_) => return, // no audio device: stay silent, like the original
    };
    let mut cache: HashMap<u16, Option<Cursor<Vec<u8>>>> = HashMap::new();
    while let Ok(id) = rx.recv() {
        let entry = cache.entry(id).or_insert_with(|| find_sound(id));
        if let Some(cursor) = entry {
            // play_once takes a Read + Seek and decodes internally.
            let _ = handle.play_once(Cursor::new(cursor.get_ref().clone()));
        }
    }
}

/// Finds the embedded WAV by its "tabs%04d" name (readkey.c TabbozPlaySound).
fn find_sound(id: u16) -> Option<Cursor<Vec<u8>>> {
    let name = format!("tabs{id:04}");
    let asset = tabboz_win::assets::SOUNDS
        .iter()
        .find(|a| a.name == name)?;
    Some(Cursor::new(asset.data.to_vec()))
}
