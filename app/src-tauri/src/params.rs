//! The program and multi editors' data: the parameter catalog
//! (`spec/params.toml`, via `s5k::catalog`) and values read from an `.akp` or
//! `.akm` through its file mappings.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use s5k::params::Index;
use s5k_formats::program::Keygroup;
use s5k_formats::{KeygroupSection, Multi, Program, Section};
use serde::Serialize;

/// A parameter as the UI needs it.
#[derive(Serialize, Clone)]
pub struct ParamInfo {
    pub id: String,
    pub scope: String,
    pub label: String,
    pub encoding: Option<String>,
    pub min: Option<i32>,
    pub max: Option<i32>,
    pub units: Option<String>,
    pub display_scale: Option<f64>,
    /// Value names for enums.
    pub options: Option<Vec<String>>,
    /// Selector range, e.g. LFO 1–2, as (name, min, max).
    pub index: Option<(String, u8, u8)>,
    /// Whether the value can be read from a saved `.akp`.
    pub in_file: bool,
    /// Whether the sampler accepts a numeric value for it.
    pub writable: bool,
    pub notes: Option<String>,
}

struct Entry {
    info: ParamInfo,
    /// `.akp` or `.akm` location as (section, field), possibly containing `{n}`.
    file: Option<(String, String)>,
}

const PROGRAM_SCOPES: [&str; 3] = ["program", "keygroup", "zone"];
const MULTI_SCOPES: [&str; 2] = ["multi", "multi_part"];

/// Program, keygroup, zone, multi and multi-part parameters from the
/// library's catalog.
static CATALOG: LazyLock<Vec<Entry>> = LazyLock::new(|| {
    let catalog = s5k::catalog();
    catalog
        .params
        .iter()
        .filter(|p| PROGRAM_SCOPES.contains(&p.scope.as_str()) || MULTI_SCOPES.contains(&p.scope.as_str()))
        .map(|p| {
            let index = match &p.index {
                Some(Index::One(i)) => Some((i.name.clone(), i.min as u8, i.max as u8)),
                _ => None,
            };
            let file = p
                .file
                .as_ref()
                .filter(|f| f.format == "akp" || f.format == "akm")
                .map(|f| (f.section.clone(), f.field.clone()));
            let mut entry = Entry {
                info: ParamInfo {
                    id: p.id.clone(),
                    scope: p.scope.clone(),
                    label: p.label.clone(),
                    encoding: p.encoding.clone(),
                    min: p.min,
                    max: p.max,
                    units: p.units.clone(),
                    display_scale: p.display_scale,
                    options: catalog.values(p).map(<[String]>::to_vec),
                    index,
                    in_file: file.is_some(),
                    writable: p.set_item.is_some() && !matches!(p.encoding.as_deref(), None | Some("string")),
                    notes: p.notes.clone(),
                },
                file,
            };
            adjust_multi(&mut entry.info);
            entry
        })
        .collect()
});

/// Multi parameters the catalog describes in wire terms, recast as the
/// editor shows them. `multi.program_number` is a composite (on/off, then
/// 0–127) that the app reads and writes itself (see `crate::multi`); here it
/// is one value, 0 = OFF and 1–128. `multi.part_count` is the real count, not
/// the reply's count − 1. `multi.new_part_count` is set-only and global (not
/// per multi), so it is applied by creating a multi rather than written.
fn adjust_multi(info: &mut ParamInfo) {
    match info.id.as_str() {
        "multi.program_number" => {
            info.encoding = Some("enum".into());
            info.min = Some(0);
            info.max = Some(128);
            info.options = Some(
                std::iter::once("OFF".to_string())
                    .chain((1..=128).map(|n| n.to_string()))
                    .collect(),
            );
            info.writable = true;
        }
        "multi.part_count" => {
            info.min = Some(32);
            info.max = Some(128);
            info.notes = Some("32, 64 or 128; fixed when the multi is created.".into());
        }
        "multi.new_part_count" => {
            info.writable = false;
            info.notes = Some("Applied when you create a multi.".into());
        }
        _ => {}
    }
}

/// Program, keygroup and zone parameters (the program editor).
pub fn catalog() -> Vec<ParamInfo> {
    scoped(&PROGRAM_SCOPES)
}

/// Multi and multi-part parameters (the multi editor).
pub fn multi_catalog() -> Vec<ParamInfo> {
    scoped(&MULTI_SCOPES)
}

fn scoped(scopes: &[&str]) -> Vec<ParamInfo> {
    CATALOG
        .iter()
        .filter(|e| scopes.contains(&e.info.scope.as_str()))
        .map(|e| e.info.clone())
        .collect()
}

/// Values keyed by parameter id, with `#n` appended for indexed ones.
pub type Values = BTreeMap<String, i32>;

#[derive(Serialize)]
pub struct ZoneView {
    pub sample: Option<String>,
    pub values: Values,
}

#[derive(Serialize)]
pub struct KeygroupView {
    pub values: Values,
    pub zones: Vec<ZoneView>,
}

#[derive(Serialize)]
pub struct ProgramView {
    pub name: String,
    pub values: Values,
    pub keygroups: Vec<KeygroupView>,
}

fn program_section(name: &str) -> Option<Section> {
    Some(match name {
        "Prg" => Section::Prg,
        "Out" => Section::Out,
        "Tune" => Section::Tune,
        "Lfo1" => Section::Lfo1,
        "Lfo2" => Section::Lfo2,
        "Mods" => Section::Mods,
        _ => return None,
    })
}

fn keygroup_section(name: &str) -> Option<KeygroupSection> {
    Some(match name {
        "Kloc" => KeygroupSection::Kloc,
        "AmpEnv" => KeygroupSection::AmpEnv,
        "FilterEnv" => KeygroupSection::FilterEnv,
        "AuxEnv" => KeygroupSection::AuxEnv,
        "Filter" => KeygroupSection::Filter,
        _ => return None,
    })
}

/// Calls `read(section, field)` for every mapped parameter of `scope`,
/// expanding `{n}` over the parameter's index range. Zone and part
/// parameters are read per zone or part, so that index is not expanded.
fn collect(scope: &str, mut read: impl FnMut(&str, &str) -> Option<i32>) -> Values {
    let mut values = Values::new();
    for entry in CATALOG.iter().filter(|e| e.info.scope == scope) {
        let Some((section, field)) = &entry.file else {
            continue;
        };
        match &entry.info.index {
            Some((_, lo, hi)) if !matches!(scope, "zone" | "multi_part") => {
                for n in *lo..=*hi {
                    let sub = |s: &str| s.replace("{n}", &n.to_string());
                    if let Some(v) = read(&sub(section), &sub(field)) {
                        values.insert(format!("{}#{n}", entry.info.id), v);
                    }
                }
            }
            _ => {
                if let Some(v) = read(section, field) {
                    values.insert(entry.info.id.clone(), v);
                }
            }
        }
    }
    values
}

fn keygroup_view(kg: &Keygroup) -> KeygroupView {
    let values = collect("keygroup", |section, field| {
        kg.get(keygroup_section(section)?, field)
    });
    let zones = (0..kg.zone_count())
        .map(|z| ZoneView {
            sample: kg.zone_sample(z),
            values: collect("zone", |_, field| kg.zone_get(z, field)),
        })
        .collect();
    KeygroupView { values, zones }
}

pub fn read_program(name: String, bytes: &[u8]) -> Result<ProgramView, s5k_formats::Error> {
    let program = Program::parse(bytes)?;
    Ok(ProgramView {
        name,
        values: collect("program", |section, field| {
            program.get(program_section(section)?, field)
        }),
        keygroups: program.keygroups().map(keygroup_view).collect(),
    })
}

#[derive(Serialize, Debug)]
pub struct PartView {
    /// The program assigned to the part, if any.
    pub program: Option<String>,
    pub values: Values,
}

#[derive(Serialize, Debug)]
pub struct MultiView {
    pub name: String,
    pub values: Values,
    pub parts: Vec<PartView>,
}

/// A multi's settings and parts from its `.akm`. Mute, solo and the multi
/// number are not in the file; `crate::multi::read_live` reads them.
pub fn read_multi(name: String, bytes: &[u8]) -> Result<MultiView, s5k_formats::Error> {
    let multi = Multi::parse(bytes)?;
    let mut values = Values::new();
    values.insert("multi.part_count".into(), multi.part_count() as i32);
    Ok(MultiView {
        name,
        values,
        parts: (0..multi.part_count())
            .map(|p| PartView {
                program: multi.part_program(p),
                values: collect("multi_part", |_, field| multi.part_get(p, field)),
            })
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_maps_to_files() {
        let cat = catalog();
        assert!(cat.iter().any(|p| p.id == "keygroup.cutoff" && p.in_file));
        let view = read_program("Blank".into(), &Program::blank(2).to_bytes()).unwrap();
        assert_eq!(view.keygroups.len(), 2);
        assert_eq!(view.values.get("program.loudness"), Some(&85));
        assert_eq!(view.keygroups[0].values.get("keygroup.cutoff"), Some(&100));
        assert_eq!(view.keygroups[0].values.get("keygroup.high_key"), Some(&127));
        assert!(view.values.contains_key("program.lfo_rate#2"));
        assert_eq!(view.keygroups[0].zones.len(), 4);
        assert!(view.keygroups[0].zones[0].values.contains_key("zone.level"));
        // The program editor's catalog holds no multi parameters, and vice versa.
        assert!(cat.iter().all(|p| PROGRAM_SCOPES.contains(&p.scope.as_str())));
    }

    #[test]
    fn multi_catalog_and_akm() {
        let cat = multi_catalog();
        assert_eq!(cat.iter().filter(|p| p.scope == "multi").count(), 4);
        assert_eq!(cat.iter().filter(|p| p.scope == "multi_part").count(), 14);
        let number = cat.iter().find(|p| p.id == "multi.program_number").unwrap();
        assert_eq!(number.options.as_ref().unwrap().len(), 129);
        assert!(
            cat.iter()
                .any(|p| p.id == "multi_part.level" && p.in_file && p.writable)
        );
        assert!(
            cat.iter()
                .any(|p| p.id == "multi_part.mute" && !p.in_file && p.writable)
        );

        let mut multi = Multi::blank(64);
        multi.set_part_program(2, "Bass").unwrap();
        multi.part_set(2, "pan", -20).unwrap();
        multi.part_set(2, "semitone", 12).unwrap();
        let view = read_multi("Set".into(), &multi.to_bytes()).unwrap();
        assert_eq!(view.values.get("multi.part_count"), Some(&64));
        assert_eq!(view.parts.len(), 64);
        assert_eq!(view.parts[2].program.as_deref(), Some("Bass"));
        assert_eq!(view.parts[0].program, None);
        let p = &view.parts[2].values;
        assert_eq!(p.get("multi_part.pan"), Some(&-20));
        assert_eq!(p.get("multi_part.semitone"), Some(&12));
        assert_eq!(p.get("multi_part.midi_channel"), Some(&2));
        assert_eq!(p.get("multi_part.level"), Some(&100));
        assert_eq!(p.get("multi_part.high_key"), Some(&127));
        assert!(!p.contains_key("multi_part.mute"));
    }
}
