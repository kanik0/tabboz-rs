// Sound: plays the original WAV resources ("Tabs%04d.Wav", like the
// original sndPlaySound) through rodio.

use std::collections::HashMap;
use std::io::Cursor;
use std::sync::mpsc::Receiver;
use std::sync::Arc;

/// Runs on a dedicated thread: each PlaySound plays the embedded WAV
/// (fire and forget, like SND_ASYNC). Sounds are cached after first use;
/// the cache stores Arc<[u8]> so playback never clones the WAV data.
pub fn run(rx: Receiver<u16>) {
    let (_stream, handle) = match rodio::OutputStream::try_default() {
        Ok(pair) => pair,
        Err(_) => return, // no audio device: stay silent, like the original
    };
    let mut cache: HashMap<u16, Option<Arc<[u8]>>> = HashMap::new();
    while let Ok(id) = rx.recv() {
        let entry = cache.entry(id).or_insert_with(|| find_sound(id));
        if let Some(data) = entry {
            // play_once takes a Read + Seek and decodes internally.
            // Cursor<Arc<[u8]>>: Arc<[u8]> implements AsRef<[u8]>, so Cursor
            // gets Read+Seek for free; the Arc clone is just a refcount bump.
            let _ = handle.play_once(Cursor::new(Arc::clone(data)));
        }
    }
}

/// Finds the embedded WAV by its "tabs%04d" name (readkey.c TabbozPlaySound).
fn find_sound(id: u16) -> Option<Arc<[u8]>> {
    let name = format!("tabs{id:04}");
    let asset = tabboz_win::assets::find_sounds(&name)?;
    Some(Arc::from(asset.data))
}
