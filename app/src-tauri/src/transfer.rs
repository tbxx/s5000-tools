//! Copying items between places: a sampler's memory, a folder on one of its
//! disks, and a folder on the computer. A program brings its samples and a
//! multi its programs and their samples, as chosen in the dependency dialog.
//!
//! A transfer runs as a few steps on the worker threads of the samplers it
//! touches (on a blocking thread for the computer's files): list the
//! destination, resolve what the item uses at the source, then copy each
//! file. Within a sampler, a step is one closure on its thread, so there is
//! no extra traffic between steps.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use s5k::ItemKind;
use s5k::disk::{LoadMode, SaveType};
use s5k_formats::{Multi, Program};
use serde::{Deserialize, Serialize};

use crate::disk::{self, DiskPos};
use crate::samplers::Samplers;
use crate::worker::{CmdResult, Device, Session};

fn str_err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
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

    fn save_type(self) -> SaveType {
        match self {
            Kind::Sample => SaveType::Sample,
            Kind::Program => SaveType::Program,
            Kind::Multi => SaveType::Multi,
        }
    }

    /// Where a file goes in memory, by its extension: `.wav` to samples,
    /// `.akp` to programs, `.akm` to multis.
    pub fn from_name(file: &str) -> Option<Self> {
        match file.rsplit_once('.')?.1.to_ascii_lowercase().as_str() {
            "wav" => Some(Kind::Sample),
            "akp" => Some(Kind::Program),
            "akm" => Some(Kind::Multi),
            _ => None,
        }
    }

    pub fn from_path(path: &Path) -> Option<Self> {
        Self::from_name(path.file_name()?.to_str()?)
    }
}

/// The item's index by name, looked up at the time of use so that queued
/// jobs survive other items being added or removed.
pub fn index_of(dev: &mut Device, kind: Kind, name: &str) -> s5k::Result<u16> {
    let names = names_of(dev, kind)?;
    find(&names, name)
        .map(|i| i as u16)
        .ok_or_else(|| s5k::Error::Invalid(format!("'{name}' is no longer in memory")))
}

fn names_of(dev: &mut Device, kind: Kind) -> s5k::Result<Vec<String>> {
    match kind {
        Kind::Sample => dev.sample_names(),
        Kind::Program => dev.program_names(),
        Kind::Multi => dev.multi_names(),
    }
}

/// Exact match first, then ignoring case, as the library's bundles do.
fn find(names: &[String], wanted: &str) -> Option<usize> {
    names
        .iter()
        .position(|n| n == wanted)
        .or_else(|| names.iter().position(|n| n.eq_ignore_ascii_case(wanted)))
}

/// Replaces characters that cannot appear in a Mac/Windows file name.
pub fn file_name(name: &str, ext: &str) -> String {
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

fn key(kind: Kind, name: &str) -> String {
    file_name(name, kind.ext()).to_lowercase()
}

fn stem(file: &str) -> &str {
    file.rsplit_once('.').map_or(file, |(b, _)| b)
}

// --- Places -----------------------------------------------------------------------

/// Where items live.
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "at", rename_all = "lowercase")]
pub enum Place {
    Memory {
        sampler: u32,
    },
    Mac {
        folder: PathBuf,
    },
    Disk {
        sampler: u32,
        disk: u16,
        path: Vec<String>,
    },
}

impl Place {
    pub fn sampler(&self) -> Option<u32> {
        match self {
            Place::Memory { sampler } | Place::Disk { sampler, .. } => Some(*sampler),
            Place::Mac { .. } => None,
        }
    }

    fn disk_pos(&self) -> Option<DiskPos> {
        match self {
            Place::Disk { disk, path, .. } => Some(DiskPos {
                disk: *disk,
                path: path.clone(),
            }),
            _ => None,
        }
    }
}

/// The item a transfer starts from.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(tag = "at", rename_all = "lowercase")]
pub enum Source {
    Memory {
        sampler: u32,
        kind: Kind,
        name: String,
    },
    Mac {
        path: PathBuf,
    },
    Disk {
        sampler: u32,
        disk: u16,
        path: Vec<String>,
        file: String,
    },
}

impl Source {
    /// The place, and the item's kind and name.
    pub fn split(&self) -> CmdResult<(Place, Kind, String)> {
        let by_file = |file: &str| {
            Kind::from_name(file)
                .map(|k| (k, stem(file).to_string()))
                .ok_or_else(|| format!("{file}: only .wav, .akp and .akm files can be transferred"))
        };
        Ok(match self {
            Source::Memory { sampler, kind, name } => {
                (Place::Memory { sampler: *sampler }, *kind, name.clone())
            }
            Source::Mac { path } => {
                let file = path
                    .file_name()
                    .map(|f| f.to_string_lossy().into_owned())
                    .ok_or("not a file")?;
                let (k, n) = by_file(&file)?;
                let folder = path.parent().unwrap_or(Path::new(".")).to_path_buf();
                (Place::Mac { folder }, k, n)
            }
            Source::Disk {
                sampler,
                disk,
                path,
                file,
            } => {
                let (k, n) = by_file(file)?;
                (
                    Place::Disk {
                        sampler: *sampler,
                        disk: *disk,
                        path: path.clone(),
                    },
                    k,
                    n,
                )
            }
        })
    }
}

/// An item found at a place.
#[derive(Clone, Debug)]
struct Found {
    kind: Kind,
    /// The file name there (Mac and disk) or `name.ext` (memory).
    file: String,
    /// Index in memory, or in the disk folder's file list.
    index: Option<u16>,
    size: Option<u64>,
}

/// What is at a place, by lower-case file name.
type Listing = HashMap<String, Found>;

fn list_place(place: &Place, session: Option<&mut Session>) -> CmdResult<Listing> {
    let mut out = Listing::new();
    match place {
        Place::Memory { .. } => {
            let dev = session.ok_or("no sampler")?.device()?;
            for kind in [Kind::Sample, Kind::Program, Kind::Multi] {
                for (i, name) in names_of(dev, kind).map_err(str_err)?.into_iter().enumerate() {
                    out.entry(key(kind, &name)).or_insert(Found {
                        kind,
                        file: format!("{name}.{}", kind.ext()),
                        index: Some(i as u16),
                        size: None,
                    });
                }
            }
        }
        Place::Mac { folder } => {
            let entries =
                fs::read_dir(folder).map_err(|e| format!("cannot open {}: {e}", folder.display()))?;
            for e in entries.flatten() {
                let file = e.file_name().to_string_lossy().into_owned();
                let Some(kind) = Kind::from_name(&file) else {
                    continue;
                };
                let size = e.metadata().ok().filter(|m| m.is_file()).map(|m| m.len());
                if size.is_none() {
                    continue;
                }
                out.insert(
                    file.to_lowercase(),
                    Found {
                        kind,
                        file,
                        index: None,
                        size,
                    },
                );
            }
        }
        Place::Disk { .. } => {
            let session = session.ok_or("no sampler")?;
            disk::goto(session, &place.disk_pos().unwrap())?;
            for (i, file) in disk::file_names(session)?.into_iter().enumerate() {
                let Some(kind) = Kind::from_name(&file) else {
                    continue;
                };
                out.entry(file.to_lowercase()).or_insert(Found {
                    kind,
                    file,
                    index: Some(i as u16),
                    size: None,
                });
            }
        }
    }
    Ok(out)
}

/// Bytes an item takes: a WAV's size from the sample's length in memory,
/// a disk file's size from the sampler; programs and multis in memory are
/// small and left unknown.
fn size_of(place: &Place, session: Option<&mut Session>, f: &Found) -> CmdResult<Option<u64>> {
    if f.size.is_some() {
        return Ok(f.size);
    }
    let (Some(session), Some(index)) = (session, f.index) else {
        return Ok(None);
    };
    Ok(match place {
        Place::Memory { .. } if f.kind == Kind::Sample => {
            let s = session
                .device()?
                .sample_info(index, String::new())
                .map_err(str_err)?;
            Some(s.frames as u64 * s.channels as u64 * 2 + 44)
        }
        Place::Disk { .. } => {
            disk::goto(session, &place.disk_pos().unwrap())?;
            Some(session.device()?.file_size(index).map_err(str_err)? as u64)
        }
        _ => None,
    })
}

fn fetch(place: &Place, session: Option<&mut Session>, f: &Found, progress: &Progress) -> CmdResult<Vec<u8>> {
    let p = progress.clone();
    let mut on_bytes = move |done: u64, _total: Option<u64>| p.bytes(done);
    match place {
        Place::Memory { .. } => {
            let index = f.index.ok_or("no index")?;
            session
                .ok_or("no sampler")?
                .device()?
                .get(f.kind.item(), index, &mut on_bytes)
                .map_err(str_err)
        }
        Place::Mac { folder } => {
            let path = folder.join(&f.file);
            fs::read(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))
        }
        Place::Disk { .. } => disk::at(session.ok_or("no sampler")?, &place.disk_pos().unwrap(), |dev| {
            dev.get_from_disk(&f.file, &mut on_bytes)
        }),
    }
}

/// Writes an item; returns the path for a file on the computer.
fn store(
    place: &Place,
    session: Option<&mut Session>,
    kind: Kind,
    name: &str,
    bytes: &[u8],
    progress: &Progress,
) -> CmdResult<Option<PathBuf>> {
    let file = file_name(name, kind.ext());
    let p = progress.clone();
    let mut on_bytes = move |done: u64, _total: Option<u64>| p.bytes(done);
    match place {
        Place::Memory { .. } => {
            let dev = session.ok_or("no sampler")?.device()?;
            dev.put(kind.item(), &file, bytes, &mut on_bytes)
                .map_err(str_err)?;
            Ok(None)
        }
        Place::Mac { folder } => {
            fs::create_dir_all(folder).map_err(|e| format!("cannot create {}: {e}", folder.display()))?;
            let path = folder.join(&file);
            fs::write(&path, bytes).map_err(|e| format!("cannot write {}: {e}", path.display()))?;
            Ok(Some(path))
        }
        Place::Disk { .. } => {
            disk::at(session.ok_or("no sampler")?, &place.disk_pos().unwrap(), |dev| {
                dev.put_to_disk(&file, bytes, &mut on_bytes)
            })?;
            Ok(None)
        }
    }
}

// --- Dependencies -------------------------------------------------------------------

/// One line of a transfer plan: the item itself or something it uses.
#[derive(Serialize, Clone, Debug)]
pub struct Line {
    pub kind: Kind,
    pub name: String,
    /// The file name it is stored under (`Kick.wav`); `Deps::Only` names these.
    pub file: String,
    /// The item being transferred (as opposed to a dependency).
    pub main: bool,
    /// For a multi's samples: the program that uses it.
    pub parent: Option<String>,
    /// Present at the source.
    pub found: bool,
    /// Already at the destination (by name).
    pub exists: bool,
    pub size: Option<u64>,
    #[serde(skip)]
    src: Option<Found>,
}

impl Line {
    fn new(kind: Kind, name: &str, main: bool, parent: Option<&str>, src: Option<&Found>) -> Self {
        Line {
            kind,
            name: name.to_string(),
            file: file_name(name, kind.ext()),
            main,
            parent: parent.map(str::to_string),
            found: src.is_some(),
            exists: false,
            size: src.and_then(|f| f.size),
            src: src.cloned(),
        }
    }

    fn key(&self) -> String {
        key(self.kind, &self.name)
    }
}

/// The item and, with `deps`, what it uses, as found in `listing`. Programs
/// and multis are read (through `read`) to find their dependencies.
fn resolve(
    listing: &Listing,
    kind: Kind,
    name: &str,
    deps: bool,
    read: &mut dyn FnMut(&Found) -> CmdResult<Vec<u8>>,
) -> CmdResult<Vec<Line>> {
    let main = listing
        .get(&key(kind, name))
        .ok_or_else(|| format!("'{name}' is no longer there"))?;
    let mut lines = vec![Line::new(kind, name, true, None, Some(main))];
    if !deps || kind == Kind::Sample {
        return Ok(lines);
    }
    let mut seen: HashSet<String> = HashSet::from([key(kind, name)]);
    let mut add_samples = |lines: &mut Vec<Line>,
                           read: &mut dyn FnMut(&Found) -> CmdResult<Vec<u8>>,
                           program: &Found,
                           parent: Option<&str>|
     -> CmdResult<()> {
        let bytes = read(program)?;
        let p = Program::parse(&bytes).map_err(|e| format!("{}: {e}", program.file))?;
        for s in p.sample_names() {
            if seen.insert(key(Kind::Sample, &s)) {
                lines.push(Line::new(
                    Kind::Sample,
                    &s,
                    false,
                    parent,
                    listing.get(&key(Kind::Sample, &s)),
                ));
            }
        }
        Ok(())
    };
    match kind {
        Kind::Program => add_samples(&mut lines, read, main, None)?,
        Kind::Multi => {
            // The .akm is a few KB and names every part's program; asking the
            // sampler per part (`multi_dependencies`) costs a request per part.
            let bytes = read(main)?;
            let multi = Multi::parse(&bytes).map_err(|e| format!("{}: {e}", main.file))?;
            for program in multi.program_names() {
                let found = listing.get(&key(Kind::Program, &program));
                lines.push(Line::new(Kind::Program, &program, false, None, found));
                if let Some(f) = found {
                    add_samples(&mut lines, read, f, Some(&program))?;
                }
            }
        }
        Kind::Sample => {}
    }
    Ok(lines)
}

/// Which dependencies to copy.
#[derive(Deserialize, Serialize, Clone, Debug, Default)]
#[serde(tag = "mode", rename_all = "lowercase")]
pub enum Deps {
    /// The item alone.
    #[default]
    Skip,
    /// The item and everything it uses that is not at the destination
    /// (or everything, with `replace`).
    All,
    /// Exactly these files (as in `Line::file`), replacing what is there.
    Only { files: Vec<String> },
}

#[derive(Deserialize, Clone, Debug)]
pub struct Request {
    pub source: Source,
    pub dest: Place,
    #[serde(default)]
    pub deps: Deps,
    /// Overwrite what exists at the destination (for `Skip` and `All`).
    #[serde(default)]
    pub replace: bool,
}

fn check_places(src: &Place, dest: &Place) -> CmdResult<()> {
    match (src, dest) {
        (Place::Mac { .. }, Place::Mac { .. }) => Err("use the Finder to copy files on the computer".into()),
        (a, b) if a == b => Err("it is already there".into()),
        _ => Ok(()),
    }
}

/// Runs `f` where `place` lives: on its sampler's thread, or on a blocking
/// thread for files on the computer.
async fn on<R: Send + 'static>(
    samplers: &Samplers,
    place: &Place,
    f: impl FnOnce(Option<&mut Session>) -> CmdResult<R> + Send + 'static,
) -> CmdResult<R> {
    match place.sampler() {
        Some(id) => samplers.worker(Some(id))?.call(move |s| f(Some(s))).await,
        None => tauri::async_runtime::spawn_blocking(move || f(None))
            .await
            .map_err(str_err)?,
    }
}

struct Prepared {
    lines: Vec<Line>,
    /// Bytes already read while resolving (programs, multis), by key.
    cache: HashMap<String, Vec<u8>>,
}

/// Lists the source, resolves dependencies, marks what exists at the
/// destination and fills in sizes (all, or only for `wanted` lines).
fn prepare(
    place: &Place,
    mut session: Option<&mut Session>,
    kind: Kind,
    name: &str,
    deps: bool,
    existing: &HashSet<String>,
    wanted: &dyn Fn(&Line) -> bool,
) -> CmdResult<Prepared> {
    let listing = list_place(place, session.as_deref_mut())?;
    let mut cache = HashMap::new();
    let mut read = |f: &Found| -> CmdResult<Vec<u8>> {
        let k = key(f.kind, stem(&f.file));
        if let Some(b) = cache.get(&k) {
            return Ok(Vec::clone(b));
        }
        let b = fetch(place, session.as_deref_mut(), f, &Progress::silent())?;
        cache.insert(k, b.clone());
        Ok(b)
    };
    let mut lines = resolve(&listing, kind, name, deps, &mut read)?;
    for line in &mut lines {
        line.exists = existing.contains(&line.key());
        if let Some(b) = cache.get(&line.key()) {
            line.size = Some(b.len() as u64);
        }
        if line.found && line.size.is_none() && wanted(line) {
            line.size = size_of(place, session.as_deref_mut(), line.src.as_ref().unwrap())?;
        }
    }
    Ok(Prepared { lines, cache })
}

async fn existing_at(samplers: &Samplers, dest: &Place) -> CmdResult<HashSet<String>> {
    let d = dest.clone();
    let listing = on(samplers, dest, move |s| match list_place(&d, s) {
        // A folder on the computer that does not exist yet holds nothing.
        Err(_) if matches!(d, Place::Mac { ref folder } if !folder.exists()) => Ok(Listing::new()),
        r => r,
    })
    .await?;
    Ok(listing.into_keys().collect())
}

/// What a transfer would copy, for the dependency dialog.
pub async fn plan(samplers: &Samplers, source: &Source, dest: &Place) -> CmdResult<Vec<Line>> {
    let (place, kind, name) = source.split()?;
    check_places(&place, dest)?;
    let existing = existing_at(samplers, dest).await?;
    let p = place.clone();
    let prepared = on(samplers, &place, move |s| {
        prepare(&p, s, kind, &name, true, &existing, &|_| true)
    })
    .await?;
    Ok(prepared.lines)
}

// --- Running a transfer ----------------------------------------------------------

#[derive(Serialize, Clone, Debug)]
pub struct ProgressEvent {
    pub job: u32,
    /// The file being transferred.
    pub item: String,
    /// Bytes done over the whole job.
    pub done: u64,
    pub total: Option<u64>,
    /// 1-based file number, and how many files the job copies.
    pub step: u32,
    pub steps: u32,
}

/// Progress of one job, sent at most 10 times a second (and at each new
/// file). Clones share the same state.
#[derive(Clone)]
pub struct Progress(Arc<Mutex<ProgressState>>);

struct ProgressState {
    job: u32,
    sink: Option<Box<dyn FnMut(ProgressEvent) + Send>>,
    last: Instant,
    item: String,
    base: u64,
    current: u64,
    total: Option<u64>,
    step: u32,
    steps: u32,
}

pub const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);

impl Progress {
    pub fn new(job: u32, sink: impl FnMut(ProgressEvent) + Send + 'static) -> Self {
        Self::with_sink(job, Some(Box::new(sink)))
    }

    fn silent() -> Self {
        Self::with_sink(0, None)
    }

    fn with_sink(job: u32, sink: Option<Box<dyn FnMut(ProgressEvent) + Send>>) -> Self {
        Self(Arc::new(Mutex::new(ProgressState {
            job,
            sink,
            last: Instant::now() - Duration::from_secs(1),
            item: String::new(),
            base: 0,
            current: 0,
            total: None,
            step: 0,
            steps: 0,
        })))
    }

    fn update(&self, force: bool, f: impl FnOnce(&mut ProgressState)) {
        let mut s = self.0.lock().unwrap_or_else(|e| e.into_inner());
        f(&mut s);
        if s.sink.is_none() || !(force || s.last.elapsed() >= PROGRESS_INTERVAL) {
            return;
        }
        s.last = Instant::now();
        let e = ProgressEvent {
            job: s.job,
            item: s.item.clone(),
            done: s.base + s.current,
            total: s.total,
            step: s.step,
            steps: s.steps,
        };
        (s.sink.as_mut().unwrap())(e);
    }

    fn start(&self, total: Option<u64>, steps: u32) {
        self.update(false, |s| {
            s.total = total;
            s.steps = steps;
        });
    }

    /// A new file starts.
    fn begin(&self, item: &str) {
        self.update(true, |s| {
            s.item = item.to_string();
            s.step += 1;
            s.current = 0;
        });
    }

    fn bytes(&self, done: u64) {
        self.update(false, |s| s.current = done);
    }

    /// The current file is done; it took `size` bytes.
    fn end(&self, size: u64) {
        self.update(false, |s| {
            s.base += size;
            s.current = 0;
        });
    }

    fn finish(&self) {
        self.update(true, |s| {
            s.current = 0;
            s.item.clear();
        });
    }
}

#[derive(Serialize, Default, Debug)]
pub struct TransferResult {
    /// Files written or items sent.
    pub done: Vec<String>,
    /// Items that already existed at the destination and were left alone.
    pub skipped: Vec<String>,
    /// Dependencies that could not be found at the source.
    pub missing: Vec<String>,
    /// Files written on the computer, with their paths.
    pub paths: Vec<PathBuf>,
}

fn order(kind: Kind) -> u8 {
    // What is used goes first, so the sampler can resolve it on arrival.
    match kind {
        Kind::Sample => 0,
        Kind::Program => 1,
        Kind::Multi => 2,
    }
}

/// Copies an item (and what it uses, per `req.deps`) to `req.dest`.
pub async fn run(samplers: &Samplers, req: Request, progress: Progress) -> CmdResult<TransferResult> {
    let (src, kind, name) = req.source.split()?;
    let dest = req.dest.clone();
    check_places(&src, &dest)?;
    let existing = existing_at(samplers, &dest).await?;

    let deps = req.deps.clone();
    let replace = req.replace;
    let selected = move |l: &Line| -> bool {
        l.found
            && match &deps {
                Deps::Skip => l.main && (!l.exists || replace),
                Deps::All => !l.exists || replace,
                Deps::Only { files } => files.iter().any(|f| f.eq_ignore_ascii_case(&l.file)),
            }
    };
    let with_deps = !matches!(req.deps, Deps::Skip);
    let p = src.clone();
    let sel = selected.clone();
    let Prepared { lines, mut cache } = on(samplers, &src, move |s| {
        prepare(&p, s, kind, &name, with_deps, &existing, &sel)
    })
    .await?;

    let mut result = TransferResult::default();
    let mut todo: Vec<Line> = Vec::new();
    for l in &lines {
        if !l.found {
            result.missing.push(l.name.clone());
        } else if selected(l) {
            todo.push(l.clone());
        } else if l.exists {
            result.skipped.push(l.file.clone());
        }
    }
    todo.sort_by_key(|l| order(l.kind));
    // Bytes cross the wire once per sampler end: twice from one sampler to
    // another. Programs and multis in memory have no known size (a few KB).
    let wire = src.sampler().is_some() as u64 + dest.sampler().is_some() as u64;
    let known = todo.iter().any(|l| l.size.is_some());
    let total = known.then(|| wire * todo.iter().filter_map(|l| l.size).sum::<u64>());
    progress.start(total, todo.len() as u32);
    let everything = with_deps && todo.len() == lines.iter().filter(|l| l.found).count();

    // On one sampler, between memory and a disk, the sampler copies by itself.
    match (&src, &dest) {
        (Place::Memory { sampler: a }, Place::Disk { sampler: b, .. }) if a == b && !todo.is_empty() => {
            let pos = dest.disk_pos().unwrap();
            let pr = progress.clone();
            let done = samplers
                .worker(Some(*a))?
                .call(move |s| {
                    let main = todo.iter().find(|l| l.main).filter(|_| everything);
                    let items: Vec<&Line> = main.map_or_else(|| todo.iter().collect(), |m| vec![m]);
                    for l in items {
                        pr.begin(&l.file);
                        let index = l.src.as_ref().and_then(|f| f.index).ok_or("no index")?;
                        disk::at(s, &pos, |dev| {
                            dev.save_to_disk(index, l.kind.save_type(), true, everything)
                        })?;
                        pr.end(l.size.unwrap_or(0));
                    }
                    Ok(todo.into_iter().map(|l| l.file).collect::<Vec<_>>())
                })
                .await?;
            result.done = done;
        }
        (Place::Disk { sampler: a, .. }, Place::Memory { sampler: b }) if a == b && !todo.is_empty() => {
            let pos = src.disk_pos().unwrap();
            let pr = progress.clone();
            let done = samplers
                .worker(Some(*a))?
                .call(move |s| {
                    let main = todo
                        .iter()
                        .find(|l| l.main)
                        .filter(|_| everything && kind != Kind::Sample);
                    let items: Vec<&Line> = main.map_or_else(|| todo.iter().collect(), |m| vec![m]);
                    for l in items {
                        let file = &l.src.as_ref().ok_or("no file")?.file;
                        pr.begin(file);
                        disk::at(s, &pos, |dev| {
                            dev.load_file(file, LoadMode::Normal, main.is_some())
                        })?;
                        pr.end(l.size.unwrap_or(0));
                    }
                    Ok(todo.into_iter().map(|l| l.file).collect::<Vec<_>>())
                })
                .await?;
            result.done = done;
        }
        _ => {
            for l in todo {
                progress.begin(&l.file);
                let found = l.src.clone().ok_or("no source")?;
                let bytes = match cache.remove(&l.key()) {
                    Some(b) => b,
                    None => {
                        let (p, pr) = (src.clone(), progress.clone());
                        on(samplers, &src, move |s| fetch(&p, s, &found, &pr)).await?
                    }
                };
                let size = bytes.len() as u64;
                if src.sampler().is_some() {
                    progress.end(size);
                }
                let (d, pr) = (dest.clone(), progress.clone());
                let (k, n) = (l.kind, l.name.clone());
                let path = on(samplers, &dest, move |s| store(&d, s, k, &n, &bytes, &pr)).await?;
                if dest.sampler().is_some() {
                    progress.end(size);
                }
                result.done.push(l.file);
                result.paths.extend(path);
            }
        }
    }
    progress.finish();
    Ok(result)
}
