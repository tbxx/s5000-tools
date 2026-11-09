//! Memory listings, block replies and one-off actions that are not
//! parameters (docs/protocol.md §5, §8, §9; spec tables 5, 13, 16, 18, 26,
//! 28). Parameters are in [`crate::params`], disks in [`crate::disk`].

use s5k_protocol::blocks::{self, PartState, PartSummary, SampleFormat, SamplePlayback};
use s5k_protocol::encode::{self, read_u14};
use s5k_protocol::sysex::section;
use s5k_protocol::{ItemKind, Message, Transport};

use crate::error::{Error, Result};
use crate::sampler::Sampler;

/// Error number for "item not found" (spec table 3).
const NOT_FOUND: u16 = 0x04;

/// Turns "item not found" into `None`: the sampler answers that way when
/// there is nothing to report (no current item, an empty list).
fn optional<V>(r: Result<V>) -> Result<Option<V>> {
    match r {
        Ok(v) => Ok(Some(v)),
        Err(Error::Sampler { code: NOT_FOUND, .. }) => Ok(None),
        Err(e) => Err(e),
    }
}

fn short(what: &str) -> Error {
    Error::Protocol(format!("short {what} reply"))
}

/// Everything known about one multi part.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Part {
    /// From 0.
    pub part: u8,
    /// The program assigned, or empty.
    pub program: String,
    pub settings: PartSummary,
    pub state: PartState,
}

impl<T: Transport> Sampler<T> {
    fn data(&mut self, section: u8, item: u8, data: &[u8]) -> Result<Vec<u8>> {
        Ok(self.request(Message::new(section, item, data.to_vec()))?.data)
    }

    // --- SysEx configuration ----------------------------------------------

    /// Checks the sampler answers (`00 00`).
    pub fn ping(&mut self) -> Result<()> {
        self.data(section::SYSEX_CONFIG, 0x00, &[]).map(drop)
    }

    /// Sends four bytes and returns what comes back (`00 06`); they should
    /// match. A test of the link.
    pub fn echo(&mut self, bytes: [u8; 4]) -> Result<[u8; 4]> {
        let d = self.data(section::SYSEX_CONFIG, 0x06, &bytes.map(|b| b & 0x7F))?;
        d.get(..4)
            .and_then(|d| d.try_into().ok())
            .ok_or_else(|| short("echo"))
    }

    // --- Current items and names ------------------------------------------

    /// Index of the current sample, program, multi or MIDI file (`0E 13`,
    /// `0A 12`, `0C 42`, `16 13`); `None` when there is none.
    pub fn current(&mut self, kind: ItemKind) -> Result<Option<u16>> {
        let item = match kind {
            ItemKind::Program => 0x12,
            ItemKind::Multi => 0x42,
            ItemKind::Sample | ItemKind::MidiFile => 0x13,
        };
        let r = self.data(Self::kind_section(kind)?, item, &[]);
        optional(r)?
            .map(|d| read_u14(&d).ok_or_else(|| short("current index")))
            .transpose()
    }

    /// Makes the item with this name current (`xx 05 <name>`). An unknown
    /// name is an error.
    pub fn select_by_name(&mut self, kind: ItemKind, name: &str) -> Result<()> {
        self.data(Self::kind_section(kind)?, 0x05, &encode::string(name))
            .map(drop)
    }

    /// One sample's name, by index (`0E 11`).
    pub fn sample_name(&mut self, index: u16) -> Result<String> {
        let d = self.data(section::SAMPLE, 0x11, &encode::u14(index))?;
        Ok(encode::read_string(&d).0)
    }

    /// The program number of every program, in memory order (`0A 18`):
    /// 1–128, or `None` when off.
    pub fn program_numbers(&mut self) -> Result<Vec<Option<u8>>> {
        if self.program_count()? == 0 {
            return Ok(Vec::new());
        }
        Ok(blocks::parse_numbers(&self.data(section::PROGRAM, 0x18, &[])?))
    }

    /// The multi number of every multi (`0C 50`).
    pub fn multi_numbers(&mut self) -> Result<Vec<Option<u8>>> {
        if self.multi_count()? == 0 {
            return Ok(Vec::new());
        }
        Ok(blocks::parse_numbers(&self.data(section::MULTI, 0x50, &[])?))
    }

    /// The number of parts of every multi (`0C 52`).
    pub fn multi_part_counts(&mut self) -> Result<Vec<u16>> {
        if self.multi_count()? == 0 {
            return Ok(Vec::new());
        }
        let d = self.data(section::MULTI, 0x52, &[])?;
        Ok(d.iter().map(|&n| n as u16 + 1).collect())
    }

    // --- Programs ---------------------------------------------------------

    /// Creates a program with one keygroup (`0A 02`). It becomes current.
    pub fn create_empty_program(&mut self, name: &str) -> Result<()> {
        self.data(section::PROGRAM, 0x02, &encode::string(name)).map(drop)
    }

    /// Adds `count` keygroups (1–98) at the end of a program (`0A 0B`).
    pub fn add_keygroups(&mut self, program: u16, count: u8) -> Result<()> {
        if !(1..=98).contains(&count) {
            return Err(Error::Invalid("add 1 to 98 keygroups".into()));
        }
        self.indexed(section::PROGRAM_BY_INDEX, program, 0x0B, &[count])
            .map(drop)
    }

    /// Deletes one keygroup of a program, numbered from 1 (`0A 0C`, which
    /// counts from 0).
    pub fn delete_keygroup(&mut self, program: u16, keygroup: u8) -> Result<()> {
        if !(1..=99).contains(&keygroup) {
            return Err(Error::Invalid("keygroups are numbered 1 to 99".into()));
        }
        self.indexed(section::PROGRAM_BY_INDEX, program, 0x0C, &[keygroup - 1])
            .map(drop)
    }

    // --- Multis -----------------------------------------------------------

    /// The program assigned to each part, from part 0; empty for an
    /// unassigned part (`0C 46`).
    pub fn multi_part_names(&mut self, multi: u16) -> Result<Vec<String>> {
        let parts = self.multi_parts(multi)?;
        let d = self.indexed(section::MULTI_BY_INDEX, multi, 0x46, &[])?;
        Ok(blocks::parse_names(&d, parts as usize))
    }

    /// One part's settings in one message (`0C 47`), parts from 0.
    pub fn multi_part_summary(&mut self, multi: u16, part: u8) -> Result<PartSummary> {
        // For a part beyond the multi's size the sampler answers with
        // whatever lies past the end, not an error.
        if part as u16 >= self.multi_parts(multi)? {
            return Err(Error::Invalid(format!(
                "the multi has no part {}",
                part as u16 + 1
            )));
        }
        self.part_summary_unchecked(multi, part)
    }

    fn part_summary_unchecked(&mut self, multi: u16, part: u8) -> Result<PartSummary> {
        let d = self.indexed(section::MULTI_BY_INDEX, multi, 0x47, &[part])?;
        PartSummary::parse(&d).ok_or_else(|| short("part summary"))
    }

    /// Mute and solo of every part (`0C 48`). A part silenced by another
    /// part's solo reads as muted.
    pub fn multi_part_states(&mut self, multi: u16) -> Result<Vec<PartState>> {
        let d = self.indexed(section::MULTI_BY_INDEX, multi, 0x48, &[])?;
        Ok(blocks::parse_part_states(&d))
    }

    /// Every part of a multi that has a program assigned.
    pub fn multi_assigned_parts(&mut self, multi: u16) -> Result<Vec<Part>> {
        let names = self.multi_part_names(multi)?;
        let states = self.multi_part_states(multi)?;
        let mut parts = Vec::new();
        for (i, program) in names.into_iter().enumerate() {
            if program.is_empty() {
                continue;
            }
            parts.push(Part {
                part: i as u8,
                settings: self.part_summary_unchecked(multi, i as u8)?,
                state: states.get(i).copied().unwrap_or(PartState::Normal),
                program,
            });
        }
        Ok(parts)
    }

    /// Removes the program from a part, numbered from 0 (`0C 34`). The
    /// part's other settings stay.
    pub fn clear_part(&mut self, multi: u16, part: u8) -> Result<()> {
        self.indexed(section::MULTI_BY_INDEX, multi, 0x34, &[part])
            .map(drop)
    }

    // --- Samples ----------------------------------------------------------

    /// Type, channels, length and rate (`0E 34`).
    pub fn sample_format(&mut self, sample: u16) -> Result<SampleFormat> {
        let d = self.indexed(section::SAMPLE_BY_INDEX, sample, 0x34, &[])?;
        SampleFormat::parse(&d).ok_or_else(|| short("sample format"))
    }

    /// Play range, tuning, playback mode and loop (`0E 4B`).
    pub fn sample_playback(&mut self, sample: u16) -> Result<SamplePlayback> {
        let d = self.indexed(section::SAMPLE_BY_INDEX, sample, 0x4B, &[])?;
        SamplePlayback::parse(&d).ok_or_else(|| short("sample playback"))
    }

    // --- Scenelists, MIDI files and set lists -------------------------------

    /// Names by count then one request per index: these sections have no
    /// "all names" item.
    fn names_by_index(&mut self, section: u8, count_item: u8, name_item: u8) -> Result<Vec<String>> {
        let d = self.data(section, count_item, &[])?;
        let n = read_u14(&d).ok_or_else(|| short("count"))?;
        (0..n)
            .map(|i| Ok(encode::read_string(&self.data(section, name_item, &encode::u14(i))?).0))
            .collect()
    }

    /// Scenelists in memory (`14 10`, `14 11`).
    pub fn scenelist_names(&mut self) -> Result<Vec<String>> {
        self.names_by_index(section::SCENELIST, 0x10, 0x11)
    }

    /// MIDI song files in memory (`16 10`, `16 11`).
    pub fn midi_file_names(&mut self) -> Result<Vec<String>> {
        self.names_by_index(section::SONG, 0x10, 0x11)
    }

    /// Set lists in memory (`16 20`, `16 21`).
    pub fn setlist_names(&mut self) -> Result<Vec<String>> {
        self.names_by_index(section::SONG, 0x20, 0x21)
    }

    /// Makes a scenelist current (`14 06`).
    pub fn select_scenelist(&mut self, index: u16) -> Result<()> {
        self.data(section::SCENELIST, 0x06, &encode::u14(index)).map(drop)
    }
}
