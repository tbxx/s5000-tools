//! Multis (`.akm`).

use crate::Error;
use crate::field::{self, Field, Kind::*, f};
use crate::program::NAME_CAPACITY;
use crate::riff::{self, Flavor, Riff};

pub const PART: &[Field] = &[
    f("midi_channel", 34, U8),
    f("semitone", 35, I8),
    f("low_key", 36, U8),
    f("high_key", 37, U8),
    f("level", 38, U8),
    f("pan", 39, I8),
    f("fine", 41, I8),
    f("fx_channel", 42, U8),
    f("fx_send", 43, U8),
    f("output", 44, U8),
];

const PROGRAM_NAME_AT: usize = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Multi {
    riff: Riff,
}

impl Multi {
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        let riff = riff::parse(bytes, Flavor::Akai)?;
        if &riff.form != b"AMUL" {
            return Err(Error::WrongForm {
                expected: "AMUL",
                found: String::from_utf8_lossy(&riff.form).into_owned(),
            });
        }
        Ok(Self { riff })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        self.riff.to_bytes()
    }

    pub fn part_count(&self) -> usize {
        self.riff.chunks.iter().filter(|c| &c.id == b"part").count()
    }

    fn part(&self, part: usize) -> Option<&[u8]> {
        self.riff
            .chunks
            .iter()
            .filter(|c| &c.id == b"part")
            .nth(part)
            .map(|c| c.data.as_slice())
    }

    fn part_mut(&mut self, part: usize) -> Result<&mut Vec<u8>, Error> {
        self.riff
            .chunks
            .iter_mut()
            .filter(|c| &c.id == b"part")
            .nth(part)
            .map(|c| &mut c.data)
            .ok_or(Error::MissingChunk("part".into()))
    }

    /// Program assigned to `part` (0-based), or `None` if empty.
    pub fn part_program(&self, part: usize) -> Option<String> {
        let name = field::read_name(self.part(part)?, PROGRAM_NAME_AT)?;
        (!name.is_empty()).then_some(name)
    }

    pub fn set_part_program(&mut self, part: usize, name: &str) -> Result<(), Error> {
        field::write_name(self.part_mut(part)?, PROGRAM_NAME_AT, NAME_CAPACITY, name)
    }

    pub fn part_get(&self, part: usize, name: &str) -> Option<i32> {
        field::read(self.part(part)?, field::lookup(PART, name)?)
    }

    pub fn part_set(&mut self, part: usize, name: &str, value: i32) -> Result<(), Error> {
        let field = field::lookup(PART, name).ok_or(Error::UnknownField(name.to_owned()))?;
        field::write(self.part_mut(part)?, field, value)
    }

    /// Raw effects settings (`fx  ` chunk); see docs/file-formats.md.
    pub fn fx(&self) -> Option<&[u8]> {
        self.riff.chunk(b"fx  ").map(|c| c.data.as_slice())
    }

    /// Distinct program names used by any part, in part order.
    pub fn program_names(&self) -> Vec<String> {
        let mut names: Vec<String> = Vec::new();
        for part in 0..self.part_count() {
            if let Some(name) = self.part_program(part)
                && !names.contains(&name)
            {
                names.push(name);
            }
        }
        names
    }
}
