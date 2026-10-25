//! Front panel: screen image, keys and data wheel (docs/protocol.md §11).

use crate::sysex::{Message, section};

pub const SCREEN_REQUEST: u8 = 0x01;
pub const WIDTH: usize = 320;
pub const HEIGHT: usize = 240;
pub const SCREEN_BYTES: usize = WIDTH * HEIGHT / 8;

/// A 320 × 240 one-bit LCD image plus the mode-button LEDs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Screen {
    /// 40 bytes per row, top row first; bit 0 of each byte is its leftmost pixel.
    pub bits: Vec<u8>,
    /// Bit n set = mode key `0x40 + n` lit.
    pub leds: u8,
}

impl Screen {
    pub fn from_reply(bits: &[u8], leds: u8) -> Option<Self> {
        (bits.len() == SCREEN_BYTES).then(|| Self {
            bits: bits.to_vec(),
            leds,
        })
    }

    pub fn pixel(&self, x: usize, y: usize) -> bool {
        let byte = self.bits[y * WIDTH / 8 + x / 8];
        byte >> (x % 8) & 1 == 1
    }

    pub fn led(&self, key: Key) -> bool {
        let code = key as u8;
        (0x40..0x48).contains(&code) && self.leds >> (code - 0x40) & 1 == 1
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Key {
    Fx = 0x40,
    Record = 0x41,
    EditSample = 0x42,
    EditProgram = 0x43,
    Multi = 0x44,
    Utilities = 0x45,
    Save = 0x46,
    Load = 0x47,
    F1 = 0x48,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    F13,
    F14,
    F15,
    F16 = 0x57,
    Num0 = 0x58,
    Num1,
    Num2,
    Num3,
    Num4,
    Num5,
    Num6,
    Num7,
    Num8,
    Num9 = 0x61,
    Minus = 0x62,
    Plus = 0x63,
    CursorLeft = 0x64,
    CursorRight = 0x65,
    Window = 0x67,
    Mark = 0x68,
    Jump = 0x69,
    Exit = 0x6A,
    EntPlay = 0x6B,
    User1 = 0x6C,
    User2 = 0x6D,
    User3 = 0x6E,
}

impl Key {
    pub fn function(n: u8) -> Option<Self> {
        (1..=16).contains(&n).then(|| Self::from_code(0x47 + n)).flatten()
    }

    pub fn from_code(code: u8) -> Option<Self> {
        Self::ALL.iter().copied().find(|k| *k as u8 == code)
    }

    pub const ALL: [Key; 46] = {
        use Key::*;
        [
            Fx,
            Record,
            EditSample,
            EditProgram,
            Multi,
            Utilities,
            Save,
            Load,
            F1,
            F2,
            F3,
            F4,
            F5,
            F6,
            F7,
            F8,
            F9,
            F10,
            F11,
            F12,
            F13,
            F14,
            F15,
            F16,
            Num0,
            Num1,
            Num2,
            Num3,
            Num4,
            Num5,
            Num6,
            Num7,
            Num8,
            Num9,
            Minus,
            Plus,
            CursorLeft,
            CursorRight,
            Window,
            Mark,
            Jump,
            Exit,
            EntPlay,
            User1,
            User2,
            User3,
        ]
    };
}

pub fn press(key: Key) -> Message {
    Message::new(section::PANEL, 0x01, [key as u8])
}

pub fn release(key: Key) -> Message {
    Message::new(section::PANEL, 0x02, [key as u8])
}

/// Turns the data wheel `clicks` steps (negative = backwards).
pub fn wheel(clicks: i8) -> Message {
    let dir = (clicks < 0) as u8;
    Message::new(section::PANEL, 0x03, [dir, clicks.unsigned_abs().min(127)])
}

/// Types one ASCII character.
pub fn type_char(c: u8) -> Message {
    Message::new(section::PANEL, 0x04, [c & 0x7F])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_messages_match_captures() {
        // Captured: 20 01 40 61 (press FX; 61 is the checksum, sent only when enabled).
        assert_eq!(
            press(Key::Fx).sysex_with_checksum()[6..10],
            [0x20, 0x01, 0x40, 0x61]
        );
        assert_eq!(wheel(-3).data, vec![1, 3]);
        assert_eq!(Key::function(9), Some(Key::F9));
        assert_eq!(Key::F9 as u8, 0x50);
        assert_eq!(Key::Num9 as u8, 0x61);
        assert!(Key::ALL.iter().all(|&k| Key::from_code(k as u8) == Some(k)));
        assert_eq!(Key::from_code(0x6E), Some(Key::User3));
    }

    #[test]
    fn screen_pixels_are_lsb_first() {
        let mut bits = vec![0u8; SCREEN_BYTES];
        bits[0] = 0b0000_0001; // leftmost pixel of the top row
        bits[SCREEN_BYTES - 1] = 0b1000_0000; // rightmost pixel of the bottom row
        let s = Screen::from_reply(&bits, 0b0001_0000).unwrap();
        assert!(s.pixel(0, 0) && !s.pixel(1, 0));
        assert!(s.pixel(WIDTH - 1, HEIGHT - 1));
        assert!(s.led(Key::Multi) && !s.led(Key::Fx));
    }
}
