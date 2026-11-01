//! The keyboard windows' notes: MIDI channel messages played on a sampler
//! over its USB connection (`Sampler::midi`), one keyboard window per sampler.
//!
//! The notes and pedals each window holds down are remembered here, so that
//! closing the window, switching it to another sampler or quitting never
//! leaves a note hanging.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use tauri::{State, Webview};

use crate::samplers::Samplers;
use crate::worker::{CmdResult, Worker};

#[derive(Default)]
struct Held {
    /// (channel, note) of the notes sounding.
    notes: BTreeSet<(u8, u8)>,
    /// Channels with the sustain pedal down.
    sustain: BTreeSet<u8>,
}

impl Held {
    fn track(&mut self, msg: &[u8]) {
        let ch = msg[0] & 0x0F;
        match (msg[0] & 0xF0, msg.get(1), msg.get(2)) {
            (0x90, Some(&n), Some(&v)) if v > 0 => drop(self.notes.insert((ch, n))),
            (0x80 | 0x90, Some(&n), _) => drop(self.notes.remove(&(ch, n))),
            (0xB0, Some(64), Some(&v)) if v >= 64 => drop(self.sustain.insert(ch)),
            (0xB0, Some(64), _) => drop(self.sustain.remove(&ch)),
            // All sound off / all notes off.
            (0xB0, Some(120 | 123), _) => self.notes.retain(|&(c, _)| c != ch),
            _ => {}
        }
    }

    /// The messages that let go of everything held, which is then forgotten.
    fn release(&mut self) -> Vec<Vec<u8>> {
        let notes = std::mem::take(&mut self.notes);
        let pedals = std::mem::take(&mut self.sustain);
        notes
            .into_iter()
            .map(|(ch, n)| vec![0x80 | ch, n, 64])
            .chain(pedals.into_iter().map(|ch| vec![0xB0 | ch, 64, 0]))
            .collect()
    }
}

struct Played {
    sampler: u32,
    worker: Arc<Worker>,
    held: Held,
}

impl Played {
    fn release(&mut self) {
        post(&self.worker, self.held.release());
    }
}

/// Sends messages in order, after whatever the sampler's thread is doing.
fn post(worker: &Worker, msgs: Vec<Vec<u8>>) {
    if msgs.is_empty() {
        return;
    }
    worker.post(move |session| {
        if let Some(dev) = session.device.as_mut() {
            for m in msgs {
                let _ = dev.midi(&m);
            }
        }
    });
}

/// What each keyboard window (by label) plays and holds.
#[derive(Default)]
pub struct Keyboards(Mutex<BTreeMap<String, Played>>);

impl Keyboards {
    fn lock(&self) -> std::sync::MutexGuard<'_, BTreeMap<String, Played>> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Lets go of everything the window `label` holds.
    pub fn release(&self, label: &str) {
        if let Some(mut p) = self.lock().remove(label) {
            p.release();
        }
    }

    /// Lets go of everything every window holds (quitting).
    pub fn release_all(&self) {
        for (_, mut p) in std::mem::take(&mut *self.lock()) {
            p.release();
        }
    }
}

/// Plays one MIDI channel message on `sampler` from this keyboard window.
/// It is queued, not awaited: the sampler does not answer notes, and queuing
/// keeps note on and note off in order.
#[tauri::command]
pub fn midi_send(
    webview: Webview,
    keyboards: State<'_, Keyboards>,
    samplers: State<'_, Samplers>,
    sampler: u32,
    msg: Vec<u8>,
) -> CmdResult<()> {
    if !s5k::is_channel_message(&msg) {
        return Err("not a MIDI channel message".into());
    }
    let mut map = keyboards.lock();
    let label = webview.label().to_string();
    if map.get(&label).is_none_or(|p| p.sampler != sampler) {
        if let Some(mut old) = map.remove(&label) {
            old.release();
        }
        let worker = samplers.worker(Some(sampler))?;
        map.insert(
            label.clone(),
            Played {
                sampler,
                worker,
                held: Held::default(),
            },
        );
    }
    let played = map.get_mut(&label).expect("inserted above");
    played.held.track(&msg);
    post(&played.worker, vec![msg]);
    Ok(())
}

/// Lets go of every note and pedal this window holds.
#[tauri::command]
pub fn midi_release_all(webview: Webview, keyboards: State<'_, Keyboards>) {
    keyboards.release(webview.label());
}

#[cfg(test)]
mod tests {
    use super::Held;

    #[test]
    fn remembers_what_is_held() {
        let mut h = Held::default();
        h.track(&[0x90, 60, 100]);
        h.track(&[0x91, 64, 100]);
        h.track(&[0x90, 60, 0]);
        h.track(&[0xB1, 64, 127]);
        assert_eq!(h.release(), vec![vec![0x81, 64, 64], vec![0xB1, 64, 0]]);
        assert!(h.release().is_empty());
    }
}
