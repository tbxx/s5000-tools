//! The keyboard window's MIDI output: a MIDI port on this computer, cabled to
//! the sampler's MIDI In. (The sampler's USB link carries SysEx; Akai's spec
//! has no way to play notes over it.)
//!
//! The connection lives here rather than in the window, and remembers which
//! notes are down, so that closing the window, switching ports or quitting
//! never leaves a note hanging.

use std::collections::BTreeSet;
use std::sync::Mutex;

use midir::{MidiOutput, MidiOutputConnection};
use tauri::State;

const CLIENT: &str = "s5000-tools";

struct Open {
    port: String,
    conn: MidiOutputConnection,
    /// (channel, note) of the notes sounding.
    notes: BTreeSet<(u8, u8)>,
    /// Channels with the sustain pedal down.
    sustain: BTreeSet<u8>,
}

impl Open {
    /// Note off for every note down and pedal up on every held channel.
    fn release_all(&mut self) {
        for (ch, note) in std::mem::take(&mut self.notes) {
            let _ = self.conn.send(&[0x80 | ch, note, 0]);
        }
        for ch in std::mem::take(&mut self.sustain) {
            let _ = self.conn.send(&[0xB0 | ch, 64, 0]);
        }
    }

    fn track(&mut self, msg: &[u8]) {
        let ch = msg[0] & 0x0F;
        match (msg[0] & 0xF0, msg.get(1), msg.get(2)) {
            (0x90, Some(&n), Some(&v)) if v > 0 => drop(self.notes.insert((ch, n))),
            (0x80 | 0x90, Some(&n), _) => drop(self.notes.remove(&(ch, n))),
            (0xB0, Some(64), Some(&v)) if v >= 64 => drop(self.sustain.insert(ch)),
            (0xB0, Some(64), _) => drop(self.sustain.remove(&ch)),
            // All notes off / all sound off.
            (0xB0, Some(120 | 123), _) => self.notes.retain(|&(c, _)| c != ch),
            _ => {}
        }
    }
}

#[derive(Default)]
pub struct Midi(Mutex<Option<Open>>);

impl Midi {
    /// Silences everything this app is holding down.
    pub fn release_all(&self) {
        if let Some(open) = self.0.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
            open.release_all();
        }
    }
}

/// A channel voice message: status 80–EF and its one or two data bytes.
/// Nothing else (no SysEx) goes out from here.
fn check(msg: &[u8]) -> Result<(), String> {
    let len = match msg.first() {
        Some(0x80..=0xBF | 0xE0..=0xEF) => 3,
        Some(0xC0..=0xDF) => 2,
        _ => return Err("not a channel message".into()),
    };
    if msg.len() != len || msg[1..].iter().any(|&b| b > 0x7F) {
        return Err("malformed MIDI message".into());
    }
    Ok(())
}

/// The names of this computer's MIDI outputs.
#[tauri::command]
pub fn midi_outputs() -> Result<Vec<String>, String> {
    let out = MidiOutput::new(CLIENT).map_err(|e| e.to_string())?;
    Ok(out.ports().iter().filter_map(|p| out.port_name(p).ok()).collect())
}

/// Sends one message to the output named `port`, opening it first if
/// another port (or none) is open.
#[tauri::command]
pub fn midi_send(midi: State<'_, Midi>, port: String, msg: Vec<u8>) -> Result<(), String> {
    check(&msg)?;
    let mut slot = midi.0.lock().unwrap_or_else(|e| e.into_inner());
    if slot.as_ref().is_none_or(|o| o.port != port) {
        if let Some(mut old) = slot.take() {
            old.release_all();
        }
        let out = MidiOutput::new(CLIENT).map_err(|e| e.to_string())?;
        let found = out
            .ports()
            .into_iter()
            .find(|p| out.port_name(p).is_ok_and(|n| n == port))
            .ok_or_else(|| format!("The MIDI output “{port}” is not there any more."))?;
        let conn = out
            .connect(&found, CLIENT)
            .map_err(|e| format!("Cannot open the MIDI output “{port}”: {e}"))?;
        *slot = Some(Open {
            port: port.clone(),
            conn,
            notes: BTreeSet::new(),
            sustain: BTreeSet::new(),
        });
    }
    let open = slot.as_mut().expect("opened above");
    if let Err(e) = open.conn.send(&msg) {
        // The port may have gone (an interface unplugged); open it afresh next time.
        *slot = None;
        return Err(format!("Cannot send to “{port}”: {e}"));
    }
    open.track(&msg);
    Ok(())
}

/// Note off for every note down (the keyboard's panic button, and when its
/// window loses focus).
#[tauri::command]
pub fn midi_release_all(midi: State<'_, Midi>) {
    midi.release_all();
}

#[cfg(test)]
mod tests {
    use super::check;

    #[test]
    fn only_channel_messages() {
        assert!(check(&[0x90, 60, 100]).is_ok());
        assert!(check(&[0xC3, 5]).is_ok());
        assert!(check(&[0xE0, 0, 64]).is_ok());
        assert!(check(&[0xF0, 0x47, 0x5E, 0xF7]).is_err());
        assert!(check(&[0x90, 60]).is_err());
        assert!(check(&[0x90, 60, 128]).is_err());
        assert!(check(&[]).is_err());
    }
}
