//! The program editor's data: the parameter catalog (`spec/params.toml`, via
//! `s5k::catalog`) and values read from an `.akp` through its file mappings.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use s5k::params::Index;
use s5k_formats::program::Keygroup;
use s5k_formats::{KeygroupSection, Program, Section};
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
    /// `.akp` location as (section, field), possibly containing `{n}`.
    file: Option<(String, String)>,
}

/// Program, keygroup and zone parameters from the library's catalog.
static CATALOG: LazyLock<Vec<Entry>> = LazyLock::new(|| {
    let catalog = s5k::catalog();
    catalog
        .params
        .iter()
        .filter(|p| matches!(p.scope.as_str(), "program" | "keygroup" | "zone"))
        .map(|p| {
            let index = match &p.index {
                Some(Index::One(i)) => Some((i.name.clone(), i.min as u8, i.max as u8)),
                _ => None,
            };
            let file = p
                .file
                .as_ref()
                .filter(|f| f.format == "akp")
                .map(|f| (f.section.clone(), f.field.clone()));
            Entry {
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
            }
        })
        .collect()
});

pub fn catalog() -> Vec<ParamInfo> {
    CATALOG.iter().map(|e| e.info.clone()).collect()
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
/// expanding `{n}` over the parameter's index range. Zone parameters are
/// read per zone, so their zone index is not expanded.
fn collect(scope: &str, mut read: impl FnMut(&str, &str) -> Option<i32>) -> Values {
    let mut values = Values::new();
    for entry in CATALOG.iter().filter(|e| e.info.scope == scope) {
        let Some((section, field)) = &entry.file else {
            continue;
        };
        match &entry.info.index {
            Some((_, lo, hi)) if scope != "zone" => {
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
    }
}
