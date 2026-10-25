//! Programs (`.akp`).

use crate::Error;
use crate::field::{self, Field, Kind::*, f};
use crate::riff::{self, Chunk, Flavor, Riff};

/// Longest name written by this library. The sampler limits names to 20
/// characters; files from other sources can hold longer ones, which are read
/// as they are.
pub const NAME_CAPACITY: usize = 20;

pub const PRG: &[Field] = &[f("keygroup_crossfade", 5, U8)];

pub const OUT: &[Field] = &[
    f("loudness", 1, U8),
    f("amp_mod_1", 2, I8),
    f("amp_mod_2", 3, I8),
    f("pan_mod_1", 4, I8),
    f("pan_mod_2", 5, I8),
    f("pan_mod_3", 6, I8),
    f("velocity_sensitivity", 7, I8),
];

pub const TUNE: &[Field] = &[
    f("semitone", 1, I8),
    f("fine", 2, I8),
    f("bend_up", 15, U8),
    f("bend_down", 16, U8),
    f("bend_mode", 17, U8),
    f("aftertouch", 18, I8),
    f("legato", 19, U8),
    f("portamento_time", 21, U8),
    f("portamento", 22, U8),
];

/// Shared by both LFOs; some fields exist on only one of them.
pub const LFO: &[Field] = &[
    f("waveform", 1, U8),
    f("rate", 2, U8),
    f("delay", 3, U8),
    f("depth", 4, U8),
    f("key_sync", 5, U8),
    f("retrigger", 6, U8),
    f("wheel", 7, U8),
    f("clock_division", 13, U8),
];

pub const MODS: &[Field] = &[
    f("lfo1_rate_source", 15, U8),
    f("lfo1_delay_source", 17, U8),
    f("lfo1_depth_source", 19, U8),
    f("lfo2_rate_source", 21, U8),
    f("lfo2_delay_source", 23, U8),
    f("lfo2_depth_source", 25, U8),
    f("pitch_source_1", 27, U8),
    f("pitch_source_2", 29, U8),
    f("amp_source", 31, U8),
    f("filter_source_1", 33, U8),
    f("filter_source_2", 35, U8),
    f("filter_source_3", 37, U8),
];

pub const KLOC: &[Field] = &[
    f("low_key", 4, U8),
    f("high_key", 5, U8),
    f("semitone", 6, I8),
    f("fine", 7, I8),
    f("fx_override", 8, U8),
    f("fx_send", 9, U8),
    f("pitch_mod_1", 10, I8),
    f("pitch_mod_2", 11, I8),
    f("amp_mod", 12, I8),
    f("velocity_crossfade", 13, U8),
    f("mute_group", 14, U8),
];

/// Amp, filter and aux envelopes share a layout; `depth` is filter-only.
pub const ENV: &[Field] = &[
    f("attack", 1, U8),
    f("decay", 3, U8),
    f("release", 4, U8),
    f("sustain", 7, U8),
    f("depth", 9, I8),
    f("velocity_attack", 10, I8),
    f("keyscale", 12, I8),
    f("velocity_release", 14, I8),
    f("off_release", 15, I8),
];

pub const FILT: &[Field] = &[
    f("type", 1, U8),
    f("cutoff", 2, U8),
    f("resonance", 3, U8),
    f("keyboard_tracking", 4, I8),
    f("mod_1", 5, I8),
    f("mod_2", 6, I8),
    f("mod_3", 7, I8),
    f("attenuation", 9, U8),
];

pub const ZONE: &[Field] = &[
    f("fine", 36, I8),
    f("semitone", 37, I8),
    f("filter", 38, I8),
    f("pan", 39, I8),
    f("playback", 40, U8),
    f("output", 41, U8),
    f("level", 42, I8),
    f("keyboard_tracking", 43, U8),
    f("velocity_start", 44, I16Le),
];

const ZONE_NAME_AT: usize = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Prg,
    Out,
    Tune,
    Lfo1,
    Lfo2,
    Mods,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeygroupSection {
    Kloc,
    AmpEnv,
    FilterEnv,
    AuxEnv,
    Filter,
}

impl Section {
    pub const ALL: [Section; 6] = [
        Self::Prg,
        Self::Out,
        Self::Tune,
        Self::Lfo1,
        Self::Lfo2,
        Self::Mods,
    ];

    pub fn fields(self) -> &'static [Field] {
        match self {
            Self::Prg => PRG,
            Self::Out => OUT,
            Self::Tune => TUNE,
            Self::Lfo1 | Self::Lfo2 => LFO,
            Self::Mods => MODS,
        }
    }

    /// Chunk id and which occurrence of it.
    fn locate(self) -> (&'static [u8; 4], usize) {
        match self {
            Self::Prg => (b"prg ", 0),
            Self::Out => (b"out ", 0),
            Self::Tune => (b"tune", 0),
            Self::Lfo1 => (b"lfo ", 0),
            Self::Lfo2 => (b"lfo ", 1),
            Self::Mods => (b"mods", 0),
        }
    }
}

impl KeygroupSection {
    pub const ALL: [KeygroupSection; 5] = [
        Self::Kloc,
        Self::AmpEnv,
        Self::FilterEnv,
        Self::AuxEnv,
        Self::Filter,
    ];

    pub fn fields(self) -> &'static [Field] {
        match self {
            Self::Kloc => KLOC,
            Self::AmpEnv | Self::FilterEnv | Self::AuxEnv => ENV,
            Self::Filter => FILT,
        }
    }

    fn locate(self) -> (&'static [u8; 4], usize) {
        match self {
            Self::Kloc => (b"kloc", 0),
            Self::AmpEnv => (b"env ", 0),
            Self::FilterEnv => (b"env ", 1),
            Self::AuxEnv => (b"env ", 2),
            Self::Filter => (b"filt", 0),
        }
    }
}

fn nth<'a>(chunks: &'a [Chunk], id: &[u8; 4], n: usize) -> Option<&'a Chunk> {
    chunks.iter().filter(|c| &c.id == id).nth(n)
}

fn nth_mut<'a>(chunks: &'a mut [Chunk], id: &[u8; 4], n: usize) -> Option<&'a mut Chunk> {
    chunks.iter_mut().filter(|c| &c.id == id).nth(n)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Keygroup {
    pub chunks: Vec<Chunk>,
}

impl Keygroup {
    pub fn get(&self, section: KeygroupSection, name: &str) -> Option<i32> {
        let (id, n) = section.locate();
        field::read(
            &nth(&self.chunks, id, n)?.data,
            field::lookup(section.fields(), name)?,
        )
    }

    pub fn set(&mut self, section: KeygroupSection, name: &str, value: i32) -> Result<(), Error> {
        let (id, n) = section.locate();
        let field = field::lookup(section.fields(), name).ok_or(Error::UnknownField(name.to_owned()))?;
        let chunk = nth_mut(&mut self.chunks, id, n).ok_or(Error::MissingChunk(id_str(id)))?;
        field::write(&mut chunk.data, field, value)
    }

    pub fn zones(&self) -> impl Iterator<Item = &Chunk> {
        self.chunks.iter().filter(|c| &c.id == b"zone")
    }

    pub fn zone_count(&self) -> usize {
        self.zones().count()
    }

    /// Sample name of zone `zone` (0-based), or `None` for an empty zone.
    pub fn zone_sample(&self, zone: usize) -> Option<String> {
        let name = field::read_name(&self.zones().nth(zone)?.data, ZONE_NAME_AT)?;
        (!name.is_empty()).then_some(name)
    }

    pub fn set_zone_sample(&mut self, zone: usize, name: &str) -> Result<(), Error> {
        let chunk = nth_mut(&mut self.chunks, b"zone", zone).ok_or(Error::MissingChunk("zone".into()))?;
        field::write_name(&mut chunk.data, ZONE_NAME_AT, NAME_CAPACITY, name)
    }

    pub fn zone_get(&self, zone: usize, name: &str) -> Option<i32> {
        field::read(&self.zones().nth(zone)?.data, field::lookup(ZONE, name)?)
    }

    pub fn zone_set(&mut self, zone: usize, name: &str, value: i32) -> Result<(), Error> {
        let field = field::lookup(ZONE, name).ok_or(Error::UnknownField(name.to_owned()))?;
        let chunk = nth_mut(&mut self.chunks, b"zone", zone).ok_or(Error::MissingChunk("zone".into()))?;
        field::write(&mut chunk.data, field, value)
    }
}

fn id_str(id: &[u8; 4]) -> String {
    String::from_utf8_lossy(id).into_owned()
}

/// Top-level items in file order, so files round-trip unchanged.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Item {
    Chunk(Chunk),
    Keygroup(Keygroup),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Program {
    size_field: u32,
    items: Vec<Item>,
}

impl Program {
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        let riff = riff::parse(bytes, Flavor::Akai)?;
        if &riff.form != b"APRG" {
            return Err(Error::WrongForm {
                expected: "APRG",
                found: id_str(&riff.form),
            });
        }
        let items = riff
            .chunks
            .into_iter()
            .map(|chunk| {
                Ok(if &chunk.id == b"kgrp" {
                    Item::Keygroup(Keygroup {
                        chunks: riff::parse_chunks(&chunk.data, Flavor::Akai)?,
                    })
                } else {
                    Item::Chunk(chunk)
                })
            })
            .collect::<Result<_, Error>>()?;
        Ok(Self {
            size_field: riff.size_field,
            items,
        })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let chunks = self
            .items
            .iter()
            .map(|item| match item {
                Item::Chunk(chunk) => chunk.clone(),
                Item::Keygroup(kg) => {
                    let mut data = Vec::new();
                    riff::write_chunks(&kg.chunks, Flavor::Akai, &mut data);
                    Chunk::new(b"kgrp", data)
                }
            })
            .collect();
        Riff {
            form: *b"APRG",
            size_field: self.size_field,
            flavor: Flavor::Akai,
            chunks,
        }
        .to_bytes()
    }

    fn chunks(&self) -> impl Iterator<Item = &Chunk> {
        self.items.iter().filter_map(|i| match i {
            Item::Chunk(c) => Some(c),
            Item::Keygroup(_) => None,
        })
    }

    pub fn get(&self, section: Section, name: &str) -> Option<i32> {
        let (id, n) = section.locate();
        let chunk = self.chunks().filter(|c| &c.id == id).nth(n)?;
        field::read(&chunk.data, field::lookup(section.fields(), name)?)
    }

    pub fn set(&mut self, section: Section, name: &str, value: i32) -> Result<(), Error> {
        let (id, n) = section.locate();
        let field = field::lookup(section.fields(), name).ok_or(Error::UnknownField(name.to_owned()))?;
        let chunk = self
            .items
            .iter_mut()
            .filter_map(|i| match i {
                Item::Chunk(c) if &c.id == id => Some(c),
                _ => None,
            })
            .nth(n)
            .ok_or(Error::MissingChunk(id_str(id)))?;
        field::write(&mut chunk.data, field, value)
    }

    pub fn keygroups(&self) -> impl Iterator<Item = &Keygroup> {
        self.items.iter().filter_map(|i| match i {
            Item::Keygroup(kg) => Some(kg),
            Item::Chunk(_) => None,
        })
    }

    pub fn keygroups_mut(&mut self) -> impl Iterator<Item = &mut Keygroup> {
        self.items.iter_mut().filter_map(|i| match i {
            Item::Keygroup(kg) => Some(kg),
            Item::Chunk(_) => None,
        })
    }

    pub fn keygroup(&self, n: usize) -> Option<&Keygroup> {
        self.keygroups().nth(n)
    }

    pub fn keygroup_mut(&mut self, n: usize) -> Option<&mut Keygroup> {
        self.keygroups_mut().nth(n)
    }

    /// Distinct sample names used by any zone, in first-use order.
    pub fn sample_names(&self) -> Vec<String> {
        let mut names: Vec<String> = Vec::new();
        for kg in self.keygroups() {
            for zone in 0..kg.zone_count() {
                if let Some(name) = kg.zone_sample(zone)
                    && !names.contains(&name)
                {
                    names.push(name);
                }
            }
        }
        names
    }
}
