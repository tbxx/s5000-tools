//! Replies that carry several values at once: memory listings and the block
//! replies `0C 47`, `0C 48`, `0E 34` and `0E 4B` (docs/protocol.md §5, §8, §9).

use crate::encode::{read_signed7, read_string, read_u28};

/// `0E 34`: a sample's type, channels, length and rate (10 bytes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SampleFormat {
    /// Played from disk rather than RAM.
    pub is_virtual: bool,
    pub channels: u8,
    /// Length in sample frames.
    pub frames: u32,
    pub sample_rate: u32,
}

impl SampleFormat {
    pub fn parse(d: &[u8]) -> Option<Self> {
        Some(Self {
            is_virtual: *d.first()? != 0,
            channels: *d.get(1)?,
            frames: read_u28(d.get(2..)?)?,
            sample_rate: read_u28(d.get(6..)?)?,
        })
    }
}

/// A loop or play position the sampler reports when there is none
/// (`7F 7F 7F 7F`, all 28 bits set).
pub const NO_POSITION: u32 = 0x0FFF_FFFF;

/// `0E 4B`: a sample's play range, tuning, playback mode and loop (22 bytes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SamplePlayback {
    pub start: u32,
    pub end: u32,
    /// MIDI note, 21–127.
    pub root_note: u8,
    pub semitone: i32,
    pub fine: i32,
    /// 0 no looping, 1 one shot, 2 loop in release, 3 loop until release,
    /// 4 LIR→retrig, 5 play→retrig.
    pub playback: u8,
    /// Loop start and end, or `None` when the sample has no loop.
    pub loop_points: Option<(u32, u32)>,
}

impl SamplePlayback {
    pub const LEN: usize = 22;

    pub fn parse(d: &[u8]) -> Option<Self> {
        if d.len() < Self::LEN {
            return None;
        }
        let (loop_start, loop_end) = (read_u28(&d[14..])?, read_u28(&d[18..])?);
        Some(Self {
            start: read_u28(d)?,
            end: read_u28(&d[4..])?,
            root_note: d[8],
            semitone: read_signed7(&d[9..])?,
            fine: read_signed7(&d[11..])?,
            playback: d[13],
            loop_points: (loop_start != NO_POSITION && loop_end != NO_POSITION)
                .then_some((loop_start, loop_end)),
        })
    }
}

/// `0C 47`: every setting of one multi part (12 bytes, the values of items
/// `20`–`2B` in order), as sent on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PartSummary {
    /// 0 = 1A … 15 = 16A, 16 = 1B … 31 = 16B.
    pub midi_channel: u8,
    pub mute: bool,
    pub solo: bool,
    /// 0–100.
    pub level: u8,
    /// 0 multi, 1–8 stereo pairs, 9–24 single outputs.
    pub output: u8,
    /// −50 (L50) … +50 (R50).
    pub pan: i32,
    /// 0 off, 1 FX1, 2 FX2, 3 RV3, 4 RV4.
    pub fx_channel: u8,
    pub fx_send: u8,
    /// Cents, −50…+50.
    pub fine: i32,
    /// Semitones, −36…+36.
    pub transpose: i32,
    pub low_note: u8,
    pub high_note: u8,
}

impl PartSummary {
    pub const LEN: usize = 12;

    pub fn parse(d: &[u8]) -> Option<Self> {
        let d: &[u8; Self::LEN] = d.get(..Self::LEN)?.try_into().ok()?;
        Some(Self {
            midi_channel: d[0],
            mute: d[1] != 0,
            solo: d[2] != 0,
            level: d[3],
            output: d[4],
            pan: d[5] as i32 - 64,
            fx_channel: d[6],
            fx_send: d[7],
            fine: d[8] as i32 - 50,
            transpose: d[9] as i32 - 36,
            low_note: d[10],
            high_note: d[11],
        })
    }
}

/// One part's entry in `0C 48`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartState {
    Normal,
    /// Muted, or silenced because another part is soloed.
    Muted,
    Solo,
}

/// `0C 48`: one byte per part (0 normal, 1 muted, 2 solo).
pub fn parse_part_states(d: &[u8]) -> Vec<PartState> {
    d.iter()
        .map(|b| match b {
            1 => PartState::Muted,
            2 => PartState::Solo,
            _ => PartState::Normal,
        })
        .collect()
}

/// `0A 18` / `0C 50`: an (on/off, number) pair per item. The number is sent
/// one lower than the front panel shows; this returns the panel's 1–128.
pub fn parse_numbers(d: &[u8]) -> Vec<Option<u8>> {
    d.as_chunks::<2>()
        .0
        .iter()
        .map(|&[on, n]| (on != 0).then_some(n.saturating_add(1)))
        .collect()
}

/// A stream of `count` null-terminated names in which an empty entry is a
/// single `00` (`0C 46`, part program names).
pub fn parse_names(mut d: &[u8], count: usize) -> Vec<String> {
    let mut out = Vec::with_capacity(count);
    while out.len() < count && !d.is_empty() {
        let (name, rest) = read_string(d);
        out.push(name);
        d = rest;
    }
    out.resize(count, String::new());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // Replies recorded from an S5000 (OS 2.14), 2026-11-08.

    #[test]
    fn sample_format() {
        let f = SampleFormat::parse(&[0x00, 0x02, 0x00, 0x01, 0x66, 0x52, 0x00, 0x02, 0x77, 0x00]).unwrap();
        assert_eq!(
            f,
            SampleFormat {
                is_virtual: false,
                channels: 2,
                frames: 29522,
                sample_rate: 48000
            }
        );
        assert!(SampleFormat::parse(&[0, 1, 0]).is_none());
    }

    #[test]
    fn sample_playback() {
        let looped = [
            0x00, 0x00, 0x00, 0x00, 0x00, 0x04, 0x6D, 0x51, 0x1D, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03,
            0x76, 0x72, 0x00, 0x04, 0x6C, 0x6C,
        ];
        let p = SamplePlayback::parse(&looped).unwrap();
        assert_eq!((p.start, p.end, p.root_note, p.playback), (0, 79569, 29, 0));
        assert_eq!(p.loop_points, Some((64370, 79468)));
        let mut unlooped = looped;
        unlooped[14..].fill(0x7F);
        unlooped[9..13].copy_from_slice(&[1, 3, 0, 20]);
        let p = SamplePlayback::parse(&unlooped).unwrap();
        assert_eq!((p.semitone, p.fine, p.loop_points), (-3, 20, None));
        assert!(SamplePlayback::parse(&looped[..21]).is_none());
    }

    #[test]
    fn part_summary() {
        let p = PartSummary::parse(&[0x01, 0, 0, 0x64, 0, 0x40, 0, 0, 0x32, 0x24, 0x15, 0x7F]).unwrap();
        assert_eq!(
            (p.midi_channel, p.level, p.pan, p.fine, p.transpose),
            (1, 100, 0, 0, 0)
        );
        assert_eq!((p.low_note, p.high_note, p.mute, p.solo), (21, 127, false, false));
        assert!(PartSummary::parse(&[0; 11]).is_none());
    }

    #[test]
    fn lists() {
        // Part 2 muted and soloed: it reads as solo, every other part as muted.
        assert_eq!(
            parse_part_states(&[1, 1, 2, 1]),
            [
                PartState::Muted,
                PartState::Muted,
                PartState::Solo,
                PartState::Muted
            ]
        );
        assert_eq!(parse_numbers(&[0, 0, 1, 4, 1]), [None, Some(5)]);
        let mut d = b"Drums\0\0Bass\0".to_vec();
        d.extend([0; 29]);
        let names = parse_names(&d, 32);
        assert_eq!(names.len(), 32);
        assert_eq!(
            (names[0].as_str(), names[1].as_str(), names[2].as_str()),
            ("Drums", "", "Bass")
        );
        assert_eq!(parse_names(b"A\0", 3), ["A", "", ""]);
    }
}
