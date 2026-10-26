//! Transfers that include dependencies: a program with its samples, a multi
//! with its programs and their samples.

use std::collections::BTreeMap;

use s5k_formats::{Multi, Program};
use s5k_protocol::{ItemKind, Transport};

use crate::error::{Error, Result};
use crate::sampler::Sampler;

/// Files fetched from memory, keyed by file name (`name.wav` etc.).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Bundle {
    pub files: BTreeMap<String, Vec<u8>>,
    /// Names referenced but not found in memory.
    pub missing: Vec<String>,
}

fn find(names: &[String], wanted: &str) -> Option<u16> {
    names
        .iter()
        .position(|n| n == wanted)
        .or_else(|| names.iter().position(|n| n.eq_ignore_ascii_case(wanted)))
        .map(|i| i as u16)
}

impl<T: Transport> Sampler<T> {
    /// Gets a program and every sample its zones use.
    pub fn get_program_bundle(&mut self, index: u16, progress: &mut dyn FnMut(&str)) -> Result<Bundle> {
        let names = self.program_names()?;
        let name = names
            .get(index as usize)
            .cloned()
            .ok_or_else(|| Error::Invalid(format!("no program {index}")))?;
        let sample_names = self.sample_names()?;
        let mut bundle = Bundle::default();
        self.add_program(&name, index, &sample_names, &mut bundle, progress)?;
        Ok(bundle)
    }

    fn add_program(
        &mut self,
        name: &str,
        index: u16,
        sample_names: &[String],
        bundle: &mut Bundle,
        progress: &mut dyn FnMut(&str),
    ) -> Result<()> {
        progress(name);
        let bytes = self.get(ItemKind::Program, index, &mut |_, _| {})?;
        let program = Program::parse(&bytes)?;
        bundle.files.insert(format!("{name}.akp"), bytes);
        for sample in program.sample_names() {
            let file = format!("{sample}.wav");
            if bundle.files.contains_key(&file) {
                continue;
            }
            match find(sample_names, &sample) {
                Some(i) => {
                    progress(&sample);
                    bundle
                        .files
                        .insert(file, self.get(ItemKind::Sample, i, &mut |_, _| {})?);
                }
                None => bundle.missing.push(sample),
            }
        }
        Ok(())
    }

    /// Gets a multi, its programs and their samples.
    pub fn get_multi_bundle(&mut self, index: u16, progress: &mut dyn FnMut(&str)) -> Result<Bundle> {
        let multi_names = self.multi_names()?;
        let name = multi_names
            .get(index as usize)
            .cloned()
            .ok_or_else(|| Error::Invalid(format!("no multi {index}")))?;
        progress(&name);
        let bytes = self.get(ItemKind::Multi, index, &mut |_, _| {})?;
        let multi = Multi::parse(&bytes)?;
        let mut bundle = Bundle::default();
        bundle.files.insert(format!("{name}.akm"), bytes);
        let program_names = self.program_names()?;
        let sample_names = self.sample_names()?;
        for program in multi.program_names() {
            match find(&program_names, &program) {
                Some(i) => self.add_program(&program, i, &sample_names, &mut bundle, progress)?,
                None => bundle.missing.push(program),
            }
        }
        Ok(bundle)
    }

    /// Sends a program, first sending any of its samples that are not already
    /// in memory. `sample_file` looks a sample up by name (e.g. in the
    /// program's folder) and returns the `.wav` bytes. Returns the names of
    /// samples it could not find.
    pub fn put_program_bundle(
        &mut self,
        file_name: &str,
        akp: &[u8],
        sample_file: &mut dyn FnMut(&str) -> Option<Vec<u8>>,
        progress: &mut dyn FnMut(&str),
    ) -> Result<Vec<String>> {
        let program = Program::parse(akp)?;
        let in_memory = self.sample_names()?;
        let mut missing = Vec::new();
        for sample in program.sample_names() {
            if find(&in_memory, &sample).is_some() {
                continue;
            }
            match sample_file(&sample) {
                Some(wav) => {
                    progress(&sample);
                    self.put(ItemKind::Sample, &format!("{sample}.wav"), &wav, &mut |_, _| {})?;
                }
                None => missing.push(sample),
            }
        }
        progress(file_name);
        self.put(ItemKind::Program, file_name, akp, &mut |_, _| {})?;
        Ok(missing)
    }
}

/// Something a program or multi refers to by name, and where it is in
/// memory (`None` if it is not loaded).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemRef {
    pub name: String,
    pub index: Option<u16>,
}

/// A program and the samples its zones use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgramDeps {
    pub program: ItemRef,
    pub samples: Vec<ItemRef>,
}

impl<T: Transport> Sampler<T> {
    /// The samples a program in memory uses. Fetches the program's `.akp`
    /// (a few KB) to read its zones.
    pub fn program_dependencies(&mut self, index: u16) -> Result<ProgramDeps> {
        let program_names = self.program_names()?;
        let sample_names = self.sample_names()?;
        self.program_deps(index, &program_names, &sample_names)
    }

    fn program_deps(
        &mut self,
        index: u16,
        program_names: &[String],
        sample_names: &[String],
    ) -> Result<ProgramDeps> {
        let name = program_names
            .get(index as usize)
            .cloned()
            .ok_or_else(|| Error::Invalid(format!("no program {index}")))?;
        let program = Program::parse(&self.get(ItemKind::Program, index, &mut |_, _| {})?)?;
        let samples = program
            .sample_names()
            .into_iter()
            .map(|s| ItemRef {
                index: find(sample_names, &s),
                name: s,
            })
            .collect();
        Ok(ProgramDeps {
            program: ItemRef {
                name,
                index: Some(index),
            },
            samples,
        })
    }

    /// The programs a multi in memory uses, each with its samples. Programs
    /// the multi names but memory lacks have no samples listed.
    pub fn multi_dependencies(&mut self, index: u16) -> Result<Vec<ProgramDeps>> {
        let program_names = self.program_names()?;
        let sample_names = self.sample_names()?;
        let mut out = Vec::new();
        for name in self.multi_part_programs(index)? {
            match find(&program_names, &name) {
                Some(i) => out.push(self.program_deps(i, &program_names, &sample_names)?),
                None => out.push(ProgramDeps {
                    program: ItemRef { name, index: None },
                    samples: Vec::new(),
                }),
            }
        }
        Ok(out)
    }
}
