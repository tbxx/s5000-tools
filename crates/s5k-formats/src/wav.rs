//! Samples (`.wav`) with the sampler's loop and tuning data.

use crate::Error;
use crate::riff::{self, Chunk, Flavor, Riff};

const AKAI_MANUFACTURER: u32 = 0x0100_0047;
const SMPL_LEN: usize = 78;
/// Loop start/end values the sampler writes for a sample without a loop.
/// Files also contain other values in this top range (e.g. `FFFFFFFF`), so
/// anything at or above `NO_LOOP_MIN` means "no loop".
const NO_LOOP: (u32, u32) = (0xFFFF_FFFE, 0xFFFF_FFF0);
const NO_LOOP_MIN: u32 = 0xFFFF_FF00;

/// Offsets within the `smpl` chunk data.
mod at {
    pub const MANUFACTURER: usize = 0;
    pub const PRODUCT: usize = 4;
    pub const PERIOD: usize = 8;
    pub const UNITY_NOTE: usize = 12;
    pub const SAMPLER_DATA_LEN: usize = 32;
    pub const LOOP_TAG: usize = 36;
    pub const LOOP_START: usize = 44;
    pub const LOOP_END: usize = 48;
    pub const TUNING_TAG: usize = 60;
    pub const ROOT: usize = 62;
    pub const SEMITONE: usize = 63;
    pub const FINE: usize = 64;
    pub const PLAY_FLAG: usize = 69;
    pub const PLAY_START: usize = 70;
    pub const PLAY_END: usize = 74;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Format {
    pub channels: u16,
    pub sample_rate: u32,
    pub bits: u16,
}

/// Loop points, tuning and play range, in sample frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AkaiInfo {
    pub root_note: u8,
    pub semitone: i8,
    pub fine: i8,
    /// Loop start and end, or `None` if the sample doesn't loop.
    pub loop_points: Option<(u32, u32)>,
    pub play_start: u32,
    pub play_end: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wav {
    riff: Riff,
}

fn u32_at(data: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(data.get(at..at + 4)?.try_into().ok()?))
}

fn put_u32(data: &mut [u8], at: usize, value: u32) {
    data[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

impl Wav {
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        let riff = riff::parse(bytes, Flavor::Standard)?;
        if &riff.form != b"WAVE" {
            return Err(Error::WrongForm {
                expected: "WAVE",
                found: String::from_utf8_lossy(&riff.form).into_owned(),
            });
        }
        if riff.chunk(b"fmt ").is_none() || riff.chunk(b"data").is_none() {
            return Err(Error::MissingChunk("fmt /data".into()));
        }
        Ok(Self { riff })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        self.riff.to_bytes()
    }

    pub fn format(&self) -> Format {
        let fmt = &self.riff.chunk(b"fmt ").expect("checked in parse").data;
        let u16_at = |at: usize| u16::from_le_bytes([fmt[at], fmt[at + 1]]);
        Format {
            channels: u16_at(2),
            sample_rate: u32_at(fmt, 4).unwrap_or(0),
            bits: u16_at(14),
        }
    }

    pub fn frames(&self) -> u32 {
        let format = self.format();
        let frame_bytes = (format.channels as u32 * format.bits as u32 / 8).max(1);
        self.riff
            .chunk(b"data")
            .map_or(0, |c| c.data.len() as u32 / frame_bytes)
    }

    /// The sampler's loop and tuning record, if this file has one.
    pub fn akai_info(&self) -> Option<AkaiInfo> {
        let d = &self.riff.chunk(b"smpl")?.data;
        if u32_at(d, at::MANUFACTURER)? != AKAI_MANUFACTURER || d.len() < SMPL_LEN {
            return None;
        }
        Some(AkaiInfo {
            root_note: d[at::ROOT],
            semitone: d[at::SEMITONE] as i8,
            fine: d[at::FINE] as i8,
            loop_points: Some((u32_at(d, at::LOOP_START)?, u32_at(d, at::LOOP_END)?))
                .filter(|&(start, end)| start < NO_LOOP_MIN && end < NO_LOOP_MIN),
            play_start: u32_at(d, at::PLAY_START)?,
            play_end: u32_at(d, at::PLAY_END)?,
        })
    }

    /// Writes the sampler's record, creating a `smpl` chunk if needed. The
    /// standard unity-note field is set to root + semitone, as the sampler does.
    pub fn set_akai_info(&mut self, info: &AkaiInfo) {
        let period = 1_000_000_000 / self.format().sample_rate.max(1);
        let existing = self
            .riff
            .chunk(b"smpl")
            .filter(|c| c.data.len() >= SMPL_LEN && u32_at(&c.data, 0) == Some(AKAI_MANUFACTURER))
            .is_some();
        if !existing {
            let mut d = vec![0u8; SMPL_LEN];
            put_u32(&mut d, at::MANUFACTURER, AKAI_MANUFACTURER);
            put_u32(&mut d, at::PRODUCT, 0x5E);
            put_u32(&mut d, at::SAMPLER_DATA_LEN, 18);
            d[at::LOOP_TAG..at::LOOP_TAG + 4].copy_from_slice(b"loop");
            d[at::TUNING_TAG] = 2;
            d[at::PLAY_FLAG] = 1;
            let chunk = Chunk::new(b"smpl", d);
            match self.riff.chunks.iter().position(|c| &c.id == b"smpl") {
                Some(i) => self.riff.chunks[i] = chunk,
                None => {
                    let data_at = self.riff.chunks.iter().position(|c| &c.id == b"data").unwrap();
                    self.riff.chunks.insert(data_at, chunk);
                }
            }
        }
        let d = &mut self.riff.chunk_mut(b"smpl").unwrap().data;
        put_u32(d, at::PERIOD, period);
        let unity = (info.root_note as i32 + info.semitone as i32).clamp(0, 127) as u32;
        put_u32(d, at::UNITY_NOTE, unity);
        let (loop_start, loop_end) = info.loop_points.unwrap_or(NO_LOOP);
        put_u32(d, at::LOOP_START, loop_start);
        put_u32(d, at::LOOP_END, loop_end);
        d[at::ROOT] = info.root_note;
        d[at::SEMITONE] = info.semitone as u8;
        d[at::FINE] = info.fine as u8;
        put_u32(d, at::PLAY_START, info.play_start);
        put_u32(d, at::PLAY_END, info.play_end);
    }
}
