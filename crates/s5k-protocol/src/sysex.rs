//! SysEx command framing and reply parsing (docs/protocol.md §2).

/// Akai manufacturer ID and the S5000/S6000 model ID.
pub const AKAI: u8 = 0x47;
pub const MODEL: u8 = 0x5E;
/// Device ID byte with the top bits set for two user-reference bytes.
pub const DEVICE: u8 = 0x20;
pub const USER_REF: [u8; 2] = [0, 0];

/// Raw-command prefix for wrapped SysEx.
pub const WRAP: u8 = 0x10;
/// Largest SysEx piece carried by one USB write.
pub const MAX_PIECE: usize = 61;

/// The 4-byte "AkaI" reply that means the sampler is busy.
pub const BUSY: [u8; 4] = *b"AkaI";

pub mod section {
    pub const SYSEX_CONFIG: u8 = 0x00;
    pub const SYSTEM: u8 = 0x02;
    pub const MIDI: u8 = 0x04;
    pub const ZONE: u8 = 0x06;
    pub const KEYGROUP: u8 = 0x08;
    pub const PROGRAM: u8 = 0x0A;
    pub const MULTI: u8 = 0x0C;
    pub const SAMPLE: u8 = 0x0E;
    pub const DISK: u8 = 0x10;
    pub const FX: u8 = 0x12;
    pub const SCENELIST: u8 = 0x14;
    pub const SONG: u8 = 0x16;
    pub const PANEL: u8 = 0x20;
    /// Program/multi/sample/FX addressed by index (first two data bytes).
    pub const PROGRAM_BY_INDEX: u8 = 0x2A;
    pub const MULTI_BY_INDEX: u8 = 0x2C;
    pub const SAMPLE_BY_INDEX: u8 = 0x2E;
    pub const FX_BY_INDEX: u8 = 0x32;
    /// Blocked requests: many parameters in one message.
    pub const ZONE_BLOCK: u8 = 0x38;
    pub const KEYGROUP_BLOCK: u8 = 0x3A;
    pub const PROGRAM_BLOCK: u8 = 0x3C;
    pub const MULTI_BLOCK: u8 = 0x3E;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub section: u8,
    pub item: u8,
    pub data: Vec<u8>,
}

/// True for sections whose first two data bytes are an item index
/// (`2A`/`2C`/`2E`/`32` by index, `38`–`3E` blocked).
pub fn is_indexed(section: u8) -> bool {
    matches!(section, 0x2A | 0x2C | 0x2E | 0x32 | 0x38 | 0x3A | 0x3C | 0x3E)
}

impl Message {
    /// A command in an ordinary section: `<section> <item> <data>`.
    pub fn new(section: u8, item: u8, data: impl Into<Vec<u8>>) -> Self {
        Self {
            section,
            item,
            data: data.into(),
        }
    }

    /// A command on an item given by index (spec table 32):
    /// `<section> <index hi> <index lo> <item> <data>`.
    pub fn by_index(section: u8, index: u16, item: u8, data: &[u8]) -> Self {
        let [hi, lo] = crate::encode::u14(index);
        let mut bytes = vec![lo, item];
        bytes.extend_from_slice(data);
        Self {
            section,
            item: hi,
            data: bytes,
        }
    }

    /// A blocked request: `<section> <program hi> <program lo> [<keygroup>]`
    /// followed by `<item> <d1> <d2> <d3>` for each parameter.
    pub fn block(section: u8, index: u16, keygroup: Option<u8>, entries: &[(u8, [u8; 3])]) -> Self {
        let [hi, lo] = crate::encode::u14(index);
        let mut bytes = vec![lo];
        bytes.extend(keygroup);
        for (item, d) in entries {
            bytes.push(*item);
            bytes.extend_from_slice(d);
        }
        Self {
            section,
            item: hi,
            data: bytes,
        }
    }

    /// The item byte the sampler echoes in its confirmation: the byte after
    /// the index in indexed sections, otherwise the item.
    pub fn reply_item(&self) -> u8 {
        if is_indexed(self.section) {
            self.data.get(1).copied().unwrap_or(0)
        } else {
            self.item
        }
    }

    pub fn checksum(&self) -> u8 {
        let sum = self
            .data
            .iter()
            .fold(self.section as u32 + self.item as u32, |a, &b| a + b as u32);
        (sum & 0x7F) as u8
    }

    /// The complete SysEx message, `F0 … F7`, without a checksum.
    ///
    /// Checksums are off by default on the sampler, and it then treats a
    /// checksum byte as data. That is harmless for fixed-length commands but
    /// corrupts variable-length ones (blocked keygroup/zone sets), so none is
    /// sent, as ak.Sys does for those.
    pub fn sysex(&self) -> Vec<u8> {
        let mut out = vec![0xF0, AKAI, MODEL, DEVICE];
        out.extend_from_slice(&USER_REF);
        out.push(self.section);
        out.push(self.item);
        out.extend_from_slice(&self.data);
        out.push(0xF7);
        out
    }

    /// The message with a checksum before `F7`, for use after checksums have
    /// been enabled with `00 04 01`.
    pub fn sysex_with_checksum(&self) -> Vec<u8> {
        let mut out = self.sysex();
        out.insert(out.len() - 1, self.checksum());
        out
    }

    /// The USB writes that carry this message: pieces of at most 61 bytes,
    /// each preceded by `10 <len lo> <len hi>`.
    pub fn usb_packets(&self) -> Vec<Vec<u8>> {
        self.sysex()
            .chunks(MAX_PIECE)
            .map(|piece| {
                let mut packet = vec![WRAP, piece.len() as u8, (piece.len() >> 8) as u8];
                packet.extend_from_slice(piece);
                packet
            })
            .collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// "O": message received (ak.Sys turns these off).
    Ok,
    /// "D": done.
    Done,
    /// "R": reply with data.
    Reply,
    /// "E": error.
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    pub status: Status,
    pub section: u8,
    pub item: u8,
    pub data: Vec<u8>,
}

impl Reply {
    /// Parses a complete `F0 … F7` reply.
    pub fn parse(bytes: &[u8]) -> Result<Self, ParseError> {
        let header = [0xF0, AKAI, MODEL];
        if bytes.len() < 9 || bytes[..3] != header || *bytes.last().unwrap() != 0xF7 {
            return Err(ParseError::NotAkaiSysex);
        }
        // Bits 5–6 of the device ID hold the number of user-ref bytes − 1.
        let refs = ((bytes[3] >> 5) & 0x03) as usize + 1;
        let at = 4 + refs;
        let (status, section, item) = (
            *bytes.get(at).ok_or(ParseError::Short)?,
            *bytes.get(at + 1).ok_or(ParseError::Short)?,
            *bytes.get(at + 2).ok_or(ParseError::Short)?,
        );
        let status = match status {
            0x4F => Status::Ok,
            0x44 => Status::Done,
            0x52 => Status::Reply,
            0x45 => Status::Error,
            other => return Err(ParseError::UnknownStatus(other)),
        };
        let data = bytes[at + 3..bytes.len() - 1].to_vec();
        Ok(Self {
            status,
            section,
            item,
            data,
        })
    }

    /// Error number for an error reply (spec table 3).
    pub fn error_code(&self) -> Option<u16> {
        (self.status == Status::Error).then(|| crate::encode::read_u14(&self.data).unwrap_or(0xFFFF))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    NotAkaiSysex,
    Short,
    UnknownStatus(u8),
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParseError::NotAkaiSysex => write!(f, "not an Akai S5000 SysEx reply"),
            ParseError::Short => write!(f, "reply is too short"),
            ParseError::UnknownStatus(s) => write!(f, "unknown reply status {s:#04x}"),
        }
    }
}

impl std::error::Error for ParseError {}

/// Meaning of an error number (spec table 3).
pub fn error_text(code: u16) -> &'static str {
    match code {
        0x00 => "not supported",
        0x01 => "invalid message format",
        0x02 => "parameter out of range",
        0x03 => "unknown error",
        0x04 => "item not found",
        0x05 => "could not create item",
        0x06 => "could not delete item",
        0x81 => "checksum invalid",
        0x101 => "disk: selected disk is invalid",
        0x102 => "disk: error during load",
        0x103 => "disk: item not found",
        0x104 => "disk: unable to create",
        0x105 => "disk: folder not empty",
        0x106 => "disk: unable to delete",
        0x107 => "disk: unknown error",
        0x108 => "disk: error during save",
        0x109 => "disk: insufficient space",
        0x10A => "disk: media is write-protected",
        0x10B => "disk: name is not unique",
        0x10C => "disk: invalid disk handle",
        0x10D => "disk: disk is empty",
        0x10E => "disk: operation aborted",
        0x10F => "disk: failed on open",
        0x110 => "disk: read error",
        0x111 => "disk: not ready",
        0x112 => "disk: SCSI error",
        0x181 => "keygroup does not exist",
        _ => "unrecognised error",
    }
}

/// Collects USB reads into complete SysEx replies. Long replies arrive in
/// several packets; busy markers are dropped.
#[derive(Debug, Default)]
pub struct Assembler {
    buf: Vec<u8>,
}

impl Assembler {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds one USB read. Returns a complete reply once its `F7` has arrived.
    pub fn push(&mut self, packet: &[u8]) -> Option<Vec<u8>> {
        if packet == BUSY {
            return None;
        }
        if self.buf.is_empty() {
            // Skip anything before the start of a message.
            let start = packet.iter().position(|&b| b == 0xF0)?;
            self.buf.extend_from_slice(&packet[start..]);
        } else {
            self.buf.extend_from_slice(packet);
        }
        let end = self.buf.iter().position(|&b| b == 0xF7)?;
        let message = self.buf[..=end].to_vec();
        self.buf.drain(..=end);
        if !self.buf.contains(&0xF0) {
            self.buf.clear();
        }
        Some(message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_a_short_command() {
        // Wave memory free (02 30). ak.Sys sends a checksum (32) before F7; we don't.
        let packets = Message::new(section::SYSTEM, 0x30, []).usb_packets();
        assert_eq!(
            packets,
            vec![vec![
                0x10, 0x09, 0x00, 0xF0, 0x47, 0x5E, 0x20, 0, 0, 0x02, 0x30, 0xF7
            ]]
        );
    }

    #[test]
    fn checksum_matches_the_spec_example() {
        let m = Message::new(0x10, 0x0C, [0x1B, 0x35, 0x6D]);
        assert_eq!(m.checksum(), 0x59);
    }

    #[test]
    fn splits_long_commands() {
        let m = Message::new(section::PROGRAM_BLOCK, 0, vec![1; 90]);
        let packets = m.usb_packets();
        assert_eq!(packets.len(), 2);
        assert_eq!(packets[0].len(), 3 + 61);
        assert_eq!(packets[0][3], 0xF0);
        assert_eq!(*packets[1].last().unwrap(), 0xF7);
        let total: usize = packets.iter().map(|p| p[1] as usize).sum();
        assert_eq!(total, m.sysex().len());
    }

    #[test]
    fn indexed_messages_match_captures() {
        // Sample info by index: 2E 00 00 34, checksum 62; reply echoes 34.
        let m = Message::by_index(section::SAMPLE_BY_INDEX, 0, 0x34, &[]);
        assert_eq!(m.sysex()[6..], [0x2E, 0x00, 0x00, 0x34, 0xF7]);
        assert_eq!(m.sysex_with_checksum()[6..], [0x2E, 0x00, 0x00, 0x34, 0x62, 0xF7]);
        assert_eq!(m.reply_item(), 0x34);
        // Keygroup set on program 1, keygroup 1: 3A 00 01 01 05 3D 00 00; reply echoes the keygroup.
        let m = Message::block(section::KEYGROUP_BLOCK, 1, Some(1), &[(0x05, [0x3D, 0, 0])]);
        assert_eq!(m.sysex()[6..14], [0x3A, 0x00, 0x01, 0x01, 0x05, 0x3D, 0x00, 0x00]);
        assert_eq!(m.reply_item(), 0x01);
        // Program block read: reply echoes the first item.
        let m = Message::block(section::PROGRAM_BLOCK, 2, None, &[(0x28, [0; 3]), (0x29, [0; 3])]);
        assert_eq!(m.reply_item(), 0x28);
        // Index 200 uses 7-bit halves.
        assert_eq!(
            Message::by_index(0x2A, 200, 0x13, &[]).sysex()[6..9],
            [0x2A, 1, 72]
        );
    }

    #[test]
    fn parses_replies() {
        let r = Reply::parse(&[0xF0, 0x47, 0x5E, 0x20, 0, 0, 0x52, 0x0E, 0x10, 0x00, 0x05, 0xF7]).unwrap();
        assert_eq!(
            (r.status, r.section, r.item, r.data.as_slice()),
            (Status::Reply, 0x0E, 0x10, &[0, 5][..])
        );
        let e = Reply::parse(&[0xF0, 0x47, 0x5E, 0x20, 0, 0, 0x45, 0x0C, 0x42, 0x00, 0x04, 0xF7]).unwrap();
        assert_eq!(e.error_code(), Some(4));
        assert_eq!(error_text(4), "item not found");
    }

    #[test]
    fn assembles_multi_packet_replies_and_skips_busy() {
        let mut a = Assembler::new();
        assert_eq!(a.push(b"AkaI"), None);
        assert_eq!(
            a.push(&[0xF0, 0x47, 0x5E, 0x20, 0, 0, 0x52, 0x10, 0x12, b'A', 0]),
            None
        );
        let done = a.push(&[b'B', 0, 0xF7]).unwrap();
        let r = Reply::parse(&done).unwrap();
        assert_eq!(crate::encode::read_strings(&r.data), vec!["A", "B"]);
    }
}
