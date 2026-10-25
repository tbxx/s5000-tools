//! Akai S5000/S6000 file formats: programs (`.akp`), multis (`.akm`) and
//! samples (`.wav`). See `docs/file-formats.md`.
//!
//! Files are kept lossless: chunks are stored as raw bytes and fields are
//! read and written in place, so an unmodified file is reproduced exactly,
//! including the stale bytes the sampler leaves in unused space.

pub mod field;
pub mod multi;
pub mod program;
pub mod riff;
pub mod wav;

pub use multi::Multi;
pub use program::{KeygroupSection, Program, Section};
pub use wav::{AkaiInfo, Wav};

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    NotRiff,
    Truncated,
    WrongForm {
        expected: &'static str,
        found: String,
    },
    MissingChunk(String),
    UnknownField(String),
    /// The field lies beyond the end of its chunk (an older file version).
    FieldAbsent(&'static str),
    OutOfRange {
        field: &'static str,
        value: i32,
    },
    BadName(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NotRiff => write!(f, "not a RIFF file"),
            Error::Truncated => write!(f, "file is truncated"),
            Error::WrongForm { expected, found } => write!(f, "expected {expected} file, found {found}"),
            Error::MissingChunk(id) => write!(f, "missing '{id}' chunk"),
            Error::UnknownField(name) => write!(f, "unknown field '{name}'"),
            Error::FieldAbsent(name) => write!(f, "field '{name}' is not present in this file version"),
            Error::OutOfRange { field, value } => write!(f, "{value} is out of range for '{field}'"),
            Error::BadName(name) => write!(
                f,
                "'{name}' is not a valid name (ASCII, at most {} characters)",
                program::NAME_CAPACITY
            ),
        }
    }
}

impl std::error::Error for Error {}
