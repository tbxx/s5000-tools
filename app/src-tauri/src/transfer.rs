//! Copying items between the sampler's memory and a folder, with progress.
//! Runs on the worker thread; no Tauri types, so it can be tested directly.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use s5k::ItemKind;
use s5k_formats::Multi;
use serde::{Deserialize, Serialize};

use crate::worker::{CmdResult, Device};

#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Sample,
    Program,
    Multi,
}

impl Kind {
    pub fn item(self) -> ItemKind {
        match self {
            Kind::Sample => ItemKind::Sample,
            Kind::Program => ItemKind::Program,
            Kind::Multi => ItemKind::Multi,
        }
    }

    pub fn ext(self) -> &'static str {
        match self {
            Kind::Sample => "wav",
            Kind::Program => "akp",
            Kind::Multi => "akm",
        }
    }

    pub fn from_path(path: &Path) -> Option<Self> {
        match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
            "wav" => Some(Kind::Sample),
            "akp" => Some(Kind::Program),
            "akm" => Some(Kind::Multi),
            _ => None,
        }
    }
}

/// The item's index by name, looked up at the time of use so that queued
/// jobs survive other items being added or removed.
pub fn index_of(dev: &mut Device, kind: Kind, name: &str) -> s5k::Result<u16> {
    let names = match kind {
        Kind::Sample => dev.sample_names()?,
        Kind::Program => dev.program_names()?,
        Kind::Multi => dev.multi_names()?,
    };
    names
        .iter()
        .position(|n| n == name)
        .map(|i| i as u16)
        .ok_or_else(|| s5k::Error::Invalid(format!("'{name}' is no longer in memory")))
}

#[derive(Serialize, Clone)]
pub struct ProgressEvent {
    pub job: u32,
    /// The file being transferred (bundles report each file as it starts).
    pub item: String,
    pub done: u64,
    pub total: Option<u64>,
}

/// Reports progress for one queued job, at most ~20 times a second.
pub struct Progress {
    job: u32,
    sink: Box<dyn FnMut(ProgressEvent) + Send>,
    last: Instant,
}

impl Progress {
    pub fn new(job: u32, sink: impl FnMut(ProgressEvent) + Send + 'static) -> Self {
        Self {
            job,
            sink: Box::new(sink),
            last: Instant::now() - Duration::from_secs(1),
        }
    }

    fn emit(&mut self, item: &str, done: u64, total: Option<u64>, force: bool) {
        if force || self.last.elapsed() >= Duration::from_millis(50) {
            self.last = Instant::now();
            (self.sink)(ProgressEvent {
                job: self.job,
                item: item.to_owned(),
                done,
                total,
            });
        }
    }
}

#[derive(Serialize, Default, Debug)]
pub struct TransferResult {
    /// Files written or items sent.
    pub done: Vec<String>,
    /// Files that already existed and were left alone.
    pub skipped: Vec<String>,
    /// Dependencies that could not be found.
    pub missing: Vec<String>,
}

fn str_err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// Replaces characters that cannot appear in a Mac/Windows file name.
fn file_name(name: &str, ext: &str) -> String {
    let safe: String = name
        .chars()
        .map(|c| {
            if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
                '_'
            } else {
                c
            }
        })
        .collect();
    format!("{safe}.{ext}")
}

/// Copies an item (and, with `deps`, what it uses) from memory into `folder`.
/// Existing files are kept unless `replace` is set.
pub fn get(
    dev: &mut Device,
    kind: Kind,
    name: &str,
    folder: &Path,
    deps: bool,
    replace: bool,
    progress: &mut Progress,
) -> CmdResult<TransferResult> {
    let index = index_of(dev, kind, name).map_err(str_err)?;
    let mut step = |item: &str| progress.emit(item, 0, None, true);
    let (files, missing) = match kind {
        Kind::Program if deps => {
            let b = dev.get_program_bundle(index, &mut step).map_err(str_err)?;
            (b.files.into_iter().collect(), b.missing)
        }
        Kind::Multi if deps => {
            let b = dev.get_multi_bundle(index, &mut step).map_err(str_err)?;
            (b.files.into_iter().collect(), b.missing)
        }
        _ => {
            let bytes = dev
                .get(kind.item(), index, &mut |done, total| {
                    progress.emit(name, done, total, false)
                })
                .map_err(str_err)?;
            let size = bytes.len() as u64;
            progress.emit(name, size, Some(size), true);
            (vec![(format!("{name}.{}", kind.ext()), bytes)], Vec::new())
        }
    };
    let mut result = TransferResult {
        missing,
        ..Default::default()
    };
    for (file, bytes) in files {
        let (stem, ext) = file.rsplit_once('.').unwrap_or((&file, ""));
        let file = file_name(stem, ext);
        let path = folder.join(&file);
        if path.exists() && !replace {
            result.skipped.push(file);
            continue;
        }
        fs::write(&path, bytes).map_err(|e| format!("cannot write {}: {e}", path.display()))?;
        result.done.push(file);
    }
    Ok(result)
}

/// Finds `<name>.<ext>` in `folder`, ignoring case.
fn find_file(folder: &Path, name: &str, ext: &str) -> Option<PathBuf> {
    let exact = folder.join(file_name(name, ext));
    if exact.is_file() {
        return Some(exact);
    }
    let wanted = file_name(name, ext).to_lowercase();
    fs::read_dir(folder)
        .ok()?
        .flatten()
        .find(|e| e.file_name().to_string_lossy().to_lowercase() == wanted)
        .map(|e| e.path())
}

/// Sends a file into memory. With `deps`, a program's samples and a multi's
/// programs (and their samples) that are not in memory are sent first,
/// looked up by name in the file's folder.
pub fn put(dev: &mut Device, path: &Path, deps: bool, progress: &mut Progress) -> CmdResult<TransferResult> {
    let kind = Kind::from_path(path).ok_or("only .wav, .akp and .akm files can be sent")?;
    let bytes = fs::read(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let file = path
        .file_name()
        .map(|f| f.to_string_lossy().into_owned())
        .ok_or("not a file")?;
    let folder = path.parent().unwrap_or(Path::new("."));
    let mut result = TransferResult::default();
    let mut run = |result: &mut TransferResult| -> s5k::Result<()> {
        match kind {
            Kind::Program if deps => {
                result.missing = put_program_with_samples(dev, folder, &file, &bytes, progress)?;
            }
            Kind::Multi if deps => {
                // The client library has no multi bundle upload: send each
                // program (with its samples) that is not in memory first.
                let multi = Multi::parse(&bytes)?;
                let in_memory = dev.program_names()?;
                for program in multi.program_names() {
                    if in_memory.iter().any(|n| n.eq_ignore_ascii_case(&program)) {
                        continue;
                    }
                    match find_file(folder, &program, "akp") {
                        Some(p) => {
                            let akp = fs::read(&p)?;
                            let name = file_name(&program, "akp");
                            let missing = put_program_with_samples(dev, folder, &name, &akp, progress)?;
                            result.missing.extend(missing);
                        }
                        None => result.missing.push(program),
                    }
                }
                send(dev, kind, &file, &bytes, progress)?;
            }
            _ => send(dev, kind, &file, &bytes, progress)?,
        }
        Ok(())
    };
    run(&mut result).map_err(str_err)?;
    result.done.push(file);
    Ok(result)
}

fn send(dev: &mut Device, kind: Kind, file: &str, bytes: &[u8], progress: &mut Progress) -> s5k::Result<()> {
    dev.put(kind.item(), file, bytes, &mut |done, total| {
        progress.emit(file, done, total, false)
    })?;
    progress.emit(file, bytes.len() as u64, Some(bytes.len() as u64), true);
    Ok(())
}

fn put_program_with_samples(
    dev: &mut Device,
    folder: &Path,
    file: &str,
    akp: &[u8],
    progress: &mut Progress,
) -> s5k::Result<Vec<String>> {
    dev.put_program_bundle(
        file,
        akp,
        &mut |sample| fs::read(find_file(folder, sample, "wav")?).ok(),
        &mut |item| progress.emit(item, 0, None, true),
    )
}
