//! The sampler's disks. Disk tools act on the selected disk and its current
//! folder, so every operation first goes to a `DiskPos`; the worker
//! remembers where it is, so repeated operations in one folder cost no
//! extra requests.

use serde::{Deserialize, Serialize};

use crate::transfer::Kind;
use crate::worker::{CmdResult, Device, Session};

/// A folder on one of a sampler's disks. `path` is empty at the root.
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub struct DiskPos {
    pub disk: u16,
    pub path: Vec<String>,
}

#[derive(Serialize, Debug)]
pub struct DiskInfo {
    pub handle: u16,
    pub name: String,
    pub writable: bool,
    /// "hard disk", "floppy", "CD-ROM", "removable" or "unknown".
    pub kind: &'static str,
}

#[derive(Serialize, Debug, Clone)]
pub struct DiskFile {
    pub name: String,
    /// "sample", "program", "multi" or "other".
    pub kind: &'static str,
    pub size: Option<u64>,
}

#[derive(Serialize, Debug)]
pub struct Listing {
    pub folders: Vec<String>,
    pub files: Vec<DiskFile>,
}

fn kind_name(file: &str) -> &'static str {
    match Kind::from_name(file) {
        Some(Kind::Sample) => "sample",
        Some(Kind::Program) => "program",
        Some(Kind::Multi) => "multi",
        None => "other",
    }
}

/// Selects `pos.disk` and opens `pos.path`, from wherever the sampler is.
/// On failure the position is forgotten, so the next call starts over.
pub fn goto(session: &mut Session, pos: &DiskPos) -> CmdResult<()> {
    let at = session.disk_at.take();
    let dev = session.device()?;
    match navigate(dev, at.as_ref(), pos) {
        Ok(()) => {
            session.disk_at = Some(pos.clone());
            Ok(())
        }
        Err(e) => Err(e.to_string()),
    }
}

fn navigate(dev: &mut Device, at: Option<&DiskPos>, pos: &DiskPos) -> s5k::Result<()> {
    if let Some(at) = at.filter(|a| a.disk == pos.disk) {
        if at.path == pos.path {
            return Ok(());
        }
        // Down from here, or up a few levels.
        if pos.path.starts_with(&at.path) {
            for name in &pos.path[at.path.len()..] {
                dev.open_folder(name)?;
            }
            return Ok(());
        }
        if at.path.starts_with(&pos.path) {
            for _ in pos.path.len()..at.path.len() {
                dev.parent_folder()?;
            }
            return Ok(());
        }
    } else {
        dev.select_disk(pos.disk)?;
    }
    dev.open_folder("")?;
    for name in &pos.path {
        dev.open_folder(name)?;
    }
    Ok(())
}

/// The connected disks. `rescan` asks the sampler to look for new ones
/// first, which can take a while.
pub fn disks(session: &mut Session, rescan: bool) -> CmdResult<Vec<DiskInfo>> {
    let dev = session.device()?;
    if rescan {
        dev.refresh_disks().map_err(|e| e.to_string())?;
        session.disk_at = None;
    }
    let dev = session.device()?;
    let disks = dev.disks().map_err(|e| e.to_string())?;
    Ok(disks
        .into_iter()
        .map(|d| DiskInfo {
            handle: d.handle,
            name: d.name,
            writable: d.writable,
            kind: match d.kind {
                s5k::disk::DiskType::HardDisk => "hard disk",
                s5k::disk::DiskType::Floppy => "floppy",
                s5k::disk::DiskType::CdRom => "CD-ROM",
                s5k::disk::DiskType::Removable => "removable",
                s5k::disk::DiskType::Unknown(_) => "unknown",
            },
        })
        .collect())
}

/// File names in the current folder (after `goto`).
pub fn file_names(session: &mut Session) -> CmdResult<Vec<String>> {
    session.device()?.files().map_err(|e| e.to_string())
}

/// Folders and files at `pos`. Each file size is one request; with
/// `sizes` false they are left out (for very large folders).
pub fn list(session: &mut Session, pos: &DiskPos, sizes: bool) -> CmdResult<Listing> {
    goto(session, pos)?;
    let dev = session.device()?;
    let result = (|| -> s5k::Result<Listing> {
        let folders = dev.folders()?;
        let names = dev.files()?;
        let mut files = Vec::with_capacity(names.len());
        for (i, name) in names.into_iter().enumerate() {
            let size = if sizes {
                Some(dev.file_size(i as u16)? as u64)
            } else {
                None
            };
            files.push(DiskFile {
                kind: kind_name(&name),
                name,
                size,
            });
        }
        Ok(Listing { folders, files })
    })();
    result.map_err(|e| {
        session.disk_at = None;
        e.to_string()
    })
}

/// Runs a disk-tools call at `pos`; forgets the position if it fails.
pub fn at<R>(
    session: &mut Session,
    pos: &DiskPos,
    f: impl FnOnce(&mut Device) -> s5k::Result<R>,
) -> CmdResult<R> {
    goto(session, pos)?;
    let r = f(session.device()?);
    if r.is_err() {
        session.disk_at = None;
    }
    r.map_err(|e| e.to_string())
}
