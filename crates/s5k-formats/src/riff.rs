//! RIFF containers as used by the sampler: a 12-byte header followed by
//! `<id> <u32 LE length> <data>` chunks.

use crate::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chunk {
    pub id: [u8; 4],
    pub data: Vec<u8>,
}

impl Chunk {
    pub fn new(id: &[u8; 4], data: Vec<u8>) -> Self {
        Self { id: *id, data }
    }

    pub fn id_str(&self) -> String {
        String::from_utf8_lossy(&self.id).into_owned()
    }
}

/// How chunk lengths and the RIFF size field are handled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flavor {
    /// Akai `.akp`/`.akm`: RIFF size field is 0, no padding.
    Akai,
    /// Standard WAV: real RIFF size, odd-length chunks padded to even.
    Standard,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Riff {
    pub form: [u8; 4],
    pub size_field: u32,
    pub flavor: Flavor,
    pub chunks: Vec<Chunk>,
}

pub fn parse(bytes: &[u8], flavor: Flavor) -> Result<Riff, Error> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" {
        return Err(Error::NotRiff);
    }
    let size_field = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
    let form = bytes[8..12].try_into().unwrap();
    Ok(Riff {
        form,
        size_field,
        flavor,
        chunks: parse_chunks(&bytes[12..], flavor)?,
    })
}

pub fn parse_chunks(mut bytes: &[u8], flavor: Flavor) -> Result<Vec<Chunk>, Error> {
    let mut chunks = Vec::new();
    while !bytes.is_empty() {
        if bytes.len() < 8 {
            return Err(Error::Truncated);
        }
        let id = bytes[0..4].try_into().unwrap();
        let len = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
        let end = 8 + len;
        if bytes.len() < end {
            return Err(Error::Truncated);
        }
        chunks.push(Chunk {
            id,
            data: bytes[8..end].to_vec(),
        });
        let padded = if flavor == Flavor::Standard && len % 2 == 1 {
            end + 1
        } else {
            end
        };
        bytes = &bytes[padded.min(bytes.len())..];
    }
    Ok(chunks)
}

pub fn write_chunks(chunks: &[Chunk], flavor: Flavor, out: &mut Vec<u8>) {
    for chunk in chunks {
        out.extend_from_slice(&chunk.id);
        out.extend_from_slice(&(chunk.data.len() as u32).to_le_bytes());
        out.extend_from_slice(&chunk.data);
        if flavor == Flavor::Standard && chunk.data.len() % 2 == 1 {
            out.push(0);
        }
    }
}

impl Riff {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut body = Vec::new();
        write_chunks(&self.chunks, self.flavor, &mut body);
        let size = match self.flavor {
            Flavor::Akai => self.size_field,
            Flavor::Standard => (body.len() + 4) as u32,
        };
        let mut out = Vec::with_capacity(body.len() + 12);
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&self.form);
        out.extend_from_slice(&body);
        out
    }

    pub fn chunk(&self, id: &[u8; 4]) -> Option<&Chunk> {
        self.chunks.iter().find(|c| &c.id == id)
    }

    pub fn chunk_mut(&mut self, id: &[u8; 4]) -> Option<&mut Chunk> {
        self.chunks.iter_mut().find(|c| &c.id == id)
    }
}
