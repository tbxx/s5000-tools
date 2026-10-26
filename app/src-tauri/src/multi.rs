//! The multi editor's sampler side: a multi in memory read as its `.akm`,
//! the values the file lacks (mute, solo, multi number) read live, and edits
//! written by catalog id.
//!
//! The library's catalog addresses everything here through
//! `Target::Multi(index)`, with one exception it cannot encode:
//! `multi.program_number` is a composite (an on/off byte, then the number), so
//! it is read and written with raw indexed messages.

use s5k::ItemKind;
use serde::Serialize;

use crate::params::{self, MultiView, Values};
use crate::samplers::Samplers;
use crate::transfer::{Kind, index_of};
use crate::worker::{CmdResult, Device};

/// Multis by index (`2C <multi hi> <multi lo> <item> …`).
const MULTI_BY_INDEX: u8 = 0x2C;
const PROGRAM_NUMBER_GET: u8 = 0x41;
const PROGRAM_NUMBER_SET: u8 = 0x31;
const PROGRAM_NUMBER: &str = "multi.program_number";

fn str_err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn param(id: &str) -> s5k::Result<&'static s5k::params::Param> {
    s5k::catalog()
        .get(id)
        .ok_or_else(|| s5k::Error::Invalid(format!("unknown parameter {id}")))
}

/// A multi in memory, read as its `.akm`.
pub async fn read(samplers: &Samplers, sampler: Option<u32>, name: String) -> CmdResult<MultiView> {
    let n = name.clone();
    let bytes = samplers
        .worker(sampler)?
        .with(move |dev| {
            let i = index_of(dev, Kind::Multi, &n)?;
            dev.get(ItemKind::Multi, i, &mut |_, _| {})
        })
        .await?;
    params::read_multi(name, &bytes).map_err(str_err)
}

/// An `.akm` on the computer (read-only in the editor).
pub fn read_file(path: &std::path::Path) -> CmdResult<MultiView> {
    let bytes = std::fs::read(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let name = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    params::read_multi(name, &bytes).map_err(str_err)
}

/// Values a saved multi does not hold.
#[derive(Serialize, Default, Debug)]
pub struct MultiLive {
    /// Multi-level values (the multi number).
    pub values: Values,
    /// Per part, from part 1: mute and solo.
    pub parts: Vec<Values>,
}

/// Reads mute, solo and the multi number from the sampler. Values the
/// sampler refuses are left out; a timeout fails the whole read.
pub async fn read_live(samplers: &Samplers, sampler: Option<u32>, multi: String) -> CmdResult<MultiLive> {
    samplers
        .worker(sampler)?
        .with(move |dev| {
            let m = index_of(dev, Kind::Multi, &multi)?;
            let mut live = MultiLive::default();
            if let Some(v) = soft(read_program_number(dev, m))? {
                live.values.insert(PROGRAM_NUMBER.into(), v);
            }
            let ids: Vec<String> = params::multi_catalog()
                .into_iter()
                .filter(|i| i.scope == "multi_part" && !i.in_file)
                .map(|i| i.id)
                .collect();
            let readable: Vec<_> = ids
                .iter()
                .filter_map(|id| param(id).ok())
                .filter(|p| p.get_item.is_some() && !matches!(p.encoding.as_deref(), None | Some("string")))
                .collect();
            for part in 0..dev.multi_parts(m)? {
                let mut values = Values::new();
                for p in &readable {
                    if let Some(s5k::Value::Number(v)) =
                        soft(dev.read_param(p, s5k::Target::Multi(m), &[part as u8]))?
                    {
                        values.insert(p.id.clone(), v);
                    }
                }
                live.parts.push(values);
            }
            Ok(live)
        })
        .await
}

/// A refused or unreadable value is `None`; a timeout is an error.
fn soft<T>(r: s5k::Result<T>) -> s5k::Result<Option<T>> {
    match r {
        Ok(v) => Ok(Some(v)),
        Err(e) if e.is_timeout() => Err(e),
        Err(_) => Ok(None),
    }
}

/// 0 = OFF, else 1–128.
fn read_program_number(dev: &mut Device, multi: u16) -> s5k::Result<i32> {
    let d = dev.indexed(MULTI_BY_INDEX, multi, PROGRAM_NUMBER_GET, &[])?;
    match d.as_slice() {
        [0, ..] => Ok(0),
        [_, n, ..] => Ok(*n as i32 + 1),
        _ => Err(s5k::Error::Protocol("short reply for the multi number".into())),
    }
}

/// Sets a multi or part parameter of a multi in memory. `part` counts from
/// 0, as on the wire; multi-level parameters take none.
pub async fn write(
    samplers: &Samplers,
    sampler: Option<u32>,
    multi: String,
    id: String,
    part: Option<u8>,
    value: i32,
) -> CmdResult<()> {
    let p = param(&id).map_err(str_err)?;
    match (p.scope.as_str(), part) {
        ("multi", None) | ("multi_part", Some(_)) => {}
        ("multi", Some(_)) => return Err(format!("{id} is not a part setting")),
        _ => return Err(format!("{id} needs a part")),
    }
    if id == PROGRAM_NUMBER && !(0..=128).contains(&value) {
        return Err(format!("{id} must be 0..128"));
    }
    samplers
        .worker(sampler)?
        .with(move |dev| {
            let m = index_of(dev, Kind::Multi, &multi)?;
            if id == PROGRAM_NUMBER {
                let data = if value == 0 {
                    [0, 0]
                } else {
                    [1, (value - 1) as u8]
                };
                return dev
                    .indexed(MULTI_BY_INDEX, m, PROGRAM_NUMBER_SET, &data)
                    .map(drop);
            }
            dev.write_param(
                p,
                s5k::Target::Multi(m),
                part.as_slice(),
                &s5k::Value::Number(value),
            )
        })
        .await
}

/// Assigns a program in memory, by name, to a part (from 0).
pub async fn set_part_program(
    samplers: &Samplers,
    sampler: Option<u32>,
    multi: String,
    part: u8,
    program: String,
) -> CmdResult<()> {
    let by_index = param("multi_part.program_index").map_err(str_err)?;
    samplers
        .worker(sampler)?
        .with(move |dev| {
            let m = index_of(dev, Kind::Multi, &multi)?;
            let p = index_of(dev, Kind::Program, &program)?;
            dev.write_param(
                by_index,
                s5k::Target::Multi(m),
                &[part],
                &s5k::Value::Number(p.into()),
            )
        })
        .await
}
