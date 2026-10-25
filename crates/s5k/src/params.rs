//! Reading and writing sampler parameters by name, driven by the catalog in
//! `spec/params.toml`.

use std::collections::HashMap;
use std::sync::OnceLock;

use s5k_protocol::encode::{self, read_signed7, read_signed14, read_u14, read_u28, split7};
use s5k_protocol::sysex::section;
use s5k_protocol::{Message, Transport};
use serde::Deserialize;

use crate::error::{Error, Result};
use crate::sampler::Sampler;

const CATALOG: &str = include_str!("../../../spec/params.toml");

#[derive(Debug, Clone, Deserialize)]
pub struct ValueList {
    pub id: String,
    pub values: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct IndexSpec {
    pub name: String,
    pub min: i32,
    pub max: i32,
}

/// A selector: one (`{name, min, max}`) or, for effects, several.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum Index {
    One(IndexSpec),
    Many(Vec<IndexSpec>),
}

#[derive(Debug, Clone, Deserialize)]
pub struct FileField {
    pub format: String,
    pub section: String,
    pub field: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Param {
    pub id: String,
    pub scope: String,
    pub label: String,
    pub section: u8,
    pub set_item: Option<u8>,
    pub get_item: Option<u8>,
    pub index: Option<Index>,
    pub encoding: Option<String>,
    pub offset: Option<i32>,
    pub list: Option<String>,
    pub values: Option<Vec<String>>,
    pub min: Option<i32>,
    pub max: Option<i32>,
    pub units: Option<String>,
    pub display_scale: Option<f64>,
    pub file: Option<FileField>,
    pub fx_module_type: Option<u8>,
    pub fx_param: Option<u8>,
    pub notes: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CatalogFile {
    list: Vec<ValueList>,
    param: Vec<Param>,
}

#[derive(Debug)]
pub struct Catalog {
    pub params: Vec<Param>,
    lists: HashMap<String, Vec<String>>,
}

/// The built-in catalog.
pub fn catalog() -> &'static Catalog {
    static CAT: OnceLock<Catalog> = OnceLock::new();
    CAT.get_or_init(|| {
        let file: CatalogFile = toml::from_str(CATALOG).expect("spec/params.toml is valid");
        Catalog {
            lists: file.list.into_iter().map(|l| (l.id, l.values)).collect(),
            params: file.param,
        }
    })
}

impl Catalog {
    pub fn get(&self, id: &str) -> Option<&Param> {
        self.params.iter().find(|p| p.id == id)
    }

    pub fn scope<'a>(&'a self, scope: &'a str) -> impl Iterator<Item = &'a Param> {
        self.params.iter().filter(move |p| p.scope == scope)
    }

    /// Names for an enum parameter's values.
    pub fn values<'a>(&'a self, param: &'a Param) -> Option<&'a [String]> {
        param.values.as_deref().or_else(|| {
            param
                .list
                .as_ref()
                .and_then(|l| self.lists.get(l))
                .map(Vec::as_slice)
        })
    }
}

/// A parameter value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Number(i32),
    Text(String),
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Number(n) => write!(f, "{n}"),
            Value::Text(t) => write!(f, "{t}"),
        }
    }
}

impl Param {
    fn encoding(&self) -> Result<&str> {
        self.encoding
            .as_deref()
            .ok_or_else(|| Error::Invalid(format!("{} has no simple encoding", self.id)))
    }

    /// Encodes a value for a set message.
    pub fn encode(&self, value: &Value) -> Result<Vec<u8>> {
        let enc = self.encoding()?;
        if enc == "string" {
            let Value::Text(t) = value else {
                return Err(Error::Invalid(format!("{} takes text", self.id)));
            };
            return Ok(encode::string(t));
        }
        let Value::Number(n) = *value else {
            return Err(Error::Invalid(format!("{} takes a number", self.id)));
        };
        if let (Some(lo), Some(hi)) = (self.min, self.max) {
            if n < lo || n > hi {
                return Err(Error::Invalid(format!("{} must be {lo}..{hi}", self.id)));
            }
        }
        Ok(match enc {
            "u7" | "enum" | "bool" => vec![(n & 0x7F) as u8],
            "offset" => vec![((n + self.offset.unwrap_or(0)) & 0x7F) as u8],
            "signed7" => encode::signed7(n).to_vec(),
            "u14" => encode::u14(n as u16).to_vec(),
            "signed14" => encode::signed14(n).to_vec(),
            "u28" => split7(n as u64, 4),
            other => return Err(Error::Invalid(format!("unsupported encoding {other}"))),
        })
    }

    /// Decodes a reply value.
    pub fn decode(&self, data: &[u8]) -> Result<Value> {
        let short = || Error::Protocol(format!("short reply for {}", self.id));
        let enc = self.encoding()?;
        Ok(match enc {
            "string" => Value::Text(encode::read_string(data).0),
            "u7" | "enum" | "bool" => Value::Number(*data.first().ok_or_else(short)? as i32),
            "offset" => Value::Number(*data.first().ok_or_else(short)? as i32 - self.offset.unwrap_or(0)),
            "signed7" => Value::Number(read_signed7(data).ok_or_else(short)?),
            "u14" => Value::Number(read_u14(data).ok_or_else(short)? as i32),
            "signed14" => Value::Number(read_signed14(data).ok_or_else(short)?),
            "u28" => Value::Number(read_u28(data).ok_or_else(short)? as i32),
            other => return Err(Error::Invalid(format!("unsupported encoding {other}"))),
        })
    }

    fn selector_count(&self) -> usize {
        match &self.index {
            None => 0,
            Some(Index::One(_)) => 1,
            Some(Index::Many(v)) => v.len(),
        }
    }
}

/// Which item a parameter belongs to. Keygroups and zones are numbered from
/// 1, as on the sampler; other indexes from 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// System or MIDI settings.
    Global,
    Program(u16),
    Keygroup {
        program: u16,
        keygroup: u8,
    },
    Multi(u16),
    Sample(u16),
    /// Effects of the current multi.
    Effects,
}

impl<T: Transport> Sampler<T> {
    /// Reads a parameter. `selectors` supplies the catalog's `index` values
    /// (LFO number, zone, part, FX channel/module, …).
    pub fn read_param(&mut self, param: &Param, target: Target, selectors: &[u8]) -> Result<Value> {
        let item = param
            .get_item
            .ok_or_else(|| Error::Invalid(format!("{} cannot be read", param.id)))?;
        let msg = self.param_message(param, target, item, selectors, None)?;
        let data = self.request(msg)?.data;
        // Indexed replies to keygroup/zone blocks echo the keygroup and omit it from the data.
        param.decode(&data)
    }

    /// Writes a parameter on the sampler.
    pub fn write_param(
        &mut self,
        param: &Param,
        target: Target,
        selectors: &[u8],
        value: &Value,
    ) -> Result<()> {
        let item = param
            .set_item
            .ok_or_else(|| Error::Invalid(format!("{} cannot be written", param.id)))?;
        let bytes = param.encode(value)?;
        let msg = self.param_message(param, target, item, selectors, Some(bytes))?;
        self.request(msg).map(drop)
    }

    fn param_message(
        &mut self,
        param: &Param,
        target: Target,
        item: u8,
        selectors: &[u8],
        value: Option<Vec<u8>>,
    ) -> Result<Message> {
        if selectors.len() != param.selector_count() {
            return Err(Error::Invalid(format!(
                "{} needs {} selector(s)",
                param.id,
                param.selector_count()
            )));
        }
        let mut args = selectors.to_vec();
        args.extend(value.unwrap_or_default());
        let indexed = |section: u8, index: u16, rest: &[u8]| {
            let [hi, lo] = encode::u14(index);
            let mut data = vec![lo];
            data.extend_from_slice(rest);
            Message::new(section, hi, data)
        };
        let reading = param.get_item == Some(item) && param.set_item != Some(item);
        Ok(match (param.scope.as_str(), target) {
            ("system" | "midi", Target::Global) => Message::new(param.section, item, args),
            ("program", Target::Program(p)) => Message::by_index(section::PROGRAM_BY_INDEX, p, item, &args),
            ("multi" | "multi_part", Target::Multi(m)) => {
                Message::by_index(section::MULTI_BY_INDEX, m, item, &args)
            }
            ("sample", Target::Sample(s)) => Message::by_index(section::SAMPLE_BY_INDEX, s, item, &args),
            ("fx", Target::Effects) => Message::new(section::FX, item, args),
            ("keygroup", Target::Keygroup { program, keygroup }) => {
                // As ak.Sys: blocked keygroup message, 3 data bytes per entry.
                let mut rest = vec![keygroup, item];
                let mut padded = args;
                padded.resize(padded.len().max(3), 0);
                rest.extend(padded);
                indexed(section::KEYGROUP_BLOCK, program, &rest)
            }
            ("zone", Target::Keygroup { program, keygroup }) => {
                // Blocked zone message: item, zone, then the value (or padding).
                let mut rest = vec![keygroup, item];
                rest.extend(&args);
                if reading {
                    rest.resize(rest.len().max(5), 0);
                }
                indexed(section::ZONE_BLOCK, program, &rest)
            }
            (scope, target) => {
                return Err(Error::Invalid(format!(
                    "{} ({scope}) does not apply to {target:?}",
                    param.id
                )));
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_loads_and_encodes() {
        let cat = catalog();
        assert!(cat.params.len() > 200);
        let pan = cat.get("zone.pan").unwrap();
        assert_eq!(pan.encode(&Value::Number(50)).unwrap(), vec![114]);
        assert_eq!(pan.decode(&[114]).unwrap(), Value::Number(50));
        let wave = cat.get("program.lfo_waveform").unwrap();
        assert_eq!(cat.values(wave).unwrap()[8], "RANDOM");
        assert!(
            cat.get("program.loudness")
                .unwrap()
                .encode(&Value::Number(101))
                .is_err()
        );
    }

    #[test]
    fn messages_match_captures() {
        let cat = catalog();
        let mut s = Sampler::connect(crate::Simulator::empty()).unwrap();
        // Captured: 2A 00 01 20 64 (loudness 100 on program 1).
        let m = s
            .param_message(
                cat.get("program.loudness").unwrap(),
                Target::Program(1),
                0x20,
                &[],
                Some(vec![100]),
            )
            .unwrap();
        assert_eq!(m.sysex()[6..11], [0x2A, 0x00, 0x01, 0x20, 0x64]);
        // Captured: 3A 00 00 01 20 0D 00 00 (filter type 13, program 0 keygroup 1).
        let ft = cat.get("keygroup.filter_type").unwrap();
        let bytes = ft.encode(&Value::Number(13)).unwrap();
        let m = s
            .param_message(
                ft,
                Target::Keygroup {
                    program: 0,
                    keygroup: 1,
                },
                0x20,
                &[],
                Some(bytes),
            )
            .unwrap();
        assert_eq!(m.sysex()[6..14], [0x3A, 0x00, 0x00, 0x01, 0x20, 0x0D, 0x00, 0x00]);
        // Captured: 38 00 00 01 03 01 72 (zone 1 pan R50).
        let pan = cat.get("zone.pan").unwrap();
        let bytes = pan.encode(&Value::Number(50)).unwrap();
        let m = s
            .param_message(
                pan,
                Target::Keygroup {
                    program: 0,
                    keygroup: 1,
                },
                0x03,
                &[1],
                Some(bytes),
            )
            .unwrap();
        assert_eq!(m.sysex()[6..13], [0x38, 0x00, 0x00, 0x01, 0x03, 0x01, 0x72]);
        // Captured: 2C 00 03 15 00 72 (multi 3, part 0 pan R50).
        let mp = cat.get("multi_part.pan").unwrap();
        let bytes = mp.encode(&Value::Number(50)).unwrap();
        let m = s
            .param_message(mp, Target::Multi(3), 0x15, &[0], Some(bytes))
            .unwrap();
        assert_eq!(m.sysex()[6..12], [0x2C, 0x00, 0x03, 0x15, 0x00, 0x72]);
    }
}
