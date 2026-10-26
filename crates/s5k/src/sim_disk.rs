//! Disks for the simulated sampler: a folder tree per disk and the Disk
//! Tools commands (section `10`) and disk transfers (`40`/`41`) acting on it.

use s5k_formats::{Multi, Program};
use s5k_protocol::Message;
use s5k_protocol::encode::{self, read_u14, split7};

use crate::sim::Simulator;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Folder {
    pub folders: Vec<(String, Folder)>,
    pub files: Vec<(String, Vec<u8>)>,
}

impl Folder {
    fn folder_mut(&mut self, name: &str) -> Option<&mut Folder> {
        self.folders
            .iter_mut()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, f)| f)
    }

    pub fn file(&self, name: &str) -> Option<&[u8]> {
        self.files
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, b)| b.as_slice())
    }

    fn put_file(&mut self, name: &str, bytes: Vec<u8>, overwrite: bool) -> bool {
        match self.files.iter_mut().find(|(n, _)| n.eq_ignore_ascii_case(name)) {
            Some(entry) if overwrite => {
                entry.1 = bytes;
                true
            }
            Some(_) => false,
            None => {
                self.files.push((name.to_string(), bytes));
                true
            }
        }
    }

    fn size(&self) -> u64 {
        self.files.iter().map(|(_, b)| b.len() as u64).sum::<u64>()
            + self.folders.iter().map(|(_, f)| f.size()).sum::<u64>()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimDisk {
    pub handle: u16,
    pub name: String,
    pub root: Folder,
    pub capacity: u64,
}

/// Disk state of the simulator.
#[derive(Debug, Default)]
pub struct DiskState {
    pub disks: Vec<SimDisk>,
    selected: Option<usize>,
    path: Vec<String>,
}

impl DiskState {
    fn current(&mut self) -> Option<&mut Folder> {
        let disk = self.disks.get_mut(self.selected?)?;
        let mut folder = &mut disk.root;
        for name in &self.path {
            folder = folder.folder_mut(name)?;
        }
        Some(folder)
    }
}

fn base_name(file: &str) -> &str {
    file.rsplit_once('.').map_or(file, |(b, _)| b)
}

fn kind_of(file: &str) -> Option<usize> {
    match file.rsplit_once('.')?.1.to_ascii_lowercase().as_str() {
        "wav" => Some(0),
        "akp" => Some(1),
        "akm" => Some(2),
        _ => None,
    }
}

const NOT_FOUND: u16 = 0x103;
const INVALID_DISK: u16 = 0x101;
const CREATE_FAILED: u16 = 0x104;

impl Simulator {
    /// Loads a file from the current folder into memory, optionally with what
    /// it uses (a program's samples, a multi's programs) from the same folder.
    fn load_from_folder(&mut self, folder: &Folder, file: &str, deps: bool) -> bool {
        let (Some(bytes), Some(k)) = (folder.file(file), kind_of(file)) else {
            return false;
        };
        let bytes = bytes.to_vec();
        if deps {
            match k {
                1 => {
                    if let Ok(p) = Program::parse(&bytes) {
                        for s in p.sample_names() {
                            self.load_from_folder(folder, &format!("{s}.wav"), false);
                        }
                    }
                }
                2 => {
                    if let Ok(m) = Multi::parse(&bytes) {
                        for p in m.program_names() {
                            self.load_from_folder(folder, &format!("{p}.akp"), true);
                        }
                    }
                }
                _ => {}
            }
        }
        self.store(k, base_name(file), bytes);
        true
    }

    fn load_folder_tree(&mut self, folder: &Folder) {
        for (name, _) in &folder.files {
            self.load_from_folder(folder, name, false);
        }
        for (_, sub) in &folder.folders {
            self.load_folder_tree(sub);
        }
    }

    /// Saves memory item `index` of kind `k` (0 sample, 1 program, 2 multi)
    /// into the current folder.
    fn save_item(&mut self, k: usize, index: usize, overwrite: bool, children: bool) -> Result<(), u16> {
        let (name, bytes) = self.items(k).get(index).cloned().ok_or(NOT_FOUND)?;
        let ext = ["wav", "akp", "akm"][k];
        let mut writes = vec![(format!("{name}.{ext}"), bytes.clone())];
        if children {
            let find = |list: &[(String, Vec<u8>)], n: &str| list.iter().find(|(x, _)| x == n).cloned();
            let mut programs = Vec::new();
            if k == 2 {
                if let Ok(m) = Multi::parse(&bytes) {
                    programs.extend(m.program_names().iter().filter_map(|p| find(&self.programs, p)));
                }
            } else if k == 1 {
                programs.push((name.clone(), bytes.clone()));
            }
            for (pname, pbytes) in programs {
                if k == 2 {
                    writes.push((format!("{pname}.akp"), pbytes.clone()));
                }
                if let Ok(p) = Program::parse(&pbytes) {
                    for s in p.sample_names() {
                        if let Some((sname, sbytes)) = find(&self.samples, &s) {
                            writes.push((format!("{sname}.wav"), sbytes));
                        }
                    }
                }
            }
        }
        let folder = self.disk.current().ok_or(INVALID_DISK)?;
        for (file, bytes) in writes {
            folder.put_file(&file, bytes, overwrite);
        }
        Ok(())
    }

    pub(crate) fn handle_disk(&mut self, msg: &Message, item: u8, args: &[u8]) {
        let name = encode::read_string(args).0;
        let result: Result<Option<Vec<u8>>, u16> = (|| {
            Ok(match item {
                0x01 => None,
                0x02 => {
                    let h = read_u14(args).unwrap_or(0);
                    let i = self
                        .disk
                        .disks
                        .iter()
                        .position(|d| d.handle == h)
                        .ok_or(0x10C_u16)?;
                    self.disk.selected = Some(i);
                    self.disk.path.clear();
                    None
                }
                0x04 => Some(vec![self.disk.disks.len() as u8]),
                0x05 => Some(
                    self.disk
                        .disks
                        .iter()
                        .flat_map(|d| {
                            let mut e = encode::u14(d.handle).to_vec();
                            e.extend([1, 2, 0, 1]); // hard disk, FAT32, SCSI 0, writable
                            e.extend(encode::string(&d.name));
                            e
                        })
                        .collect(),
                ),
                0x06 => Some(vec![1]),
                0x08 => {
                    let i = self.disk.selected.ok_or(INVALID_DISK)?;
                    Some(encode::u14(self.disk.disks[i].handle).to_vec())
                }
                0x09 => {
                    self.disk.selected.ok_or(INVALID_DISK)?;
                    let path = self
                        .disk
                        .path
                        .iter()
                        .map(|p| format!("\\{p}"))
                        .collect::<String>();
                    Some(encode::string(&path))
                }
                0x0A => Some(vec![2]),
                0x0B => {
                    let i = self.disk.selected.ok_or(INVALID_DISK)?;
                    let d = &self.disk.disks[i];
                    Some(split7(d.capacity.saturating_sub(d.root.size()), 8))
                }
                0x10 => {
                    Some(encode::u14(self.disk.current().ok_or(INVALID_DISK)?.folders.len() as u16).to_vec())
                }
                0x12 => Some(
                    self.disk
                        .current()
                        .ok_or(INVALID_DISK)?
                        .folders
                        .iter()
                        .flat_map(|(n, _)| encode::string(n))
                        .collect(),
                ),
                0x13 => {
                    if name.is_empty() {
                        self.disk.path.clear();
                    } else {
                        self.disk
                            .current()
                            .ok_or(INVALID_DISK)?
                            .folder_mut(&name)
                            .ok_or(NOT_FOUND)?;
                        self.disk.path.push(name);
                    }
                    None
                }
                0x14 => {
                    self.disk.path.pop().ok_or(0x03_u16)?;
                    None
                }
                0x15 => {
                    let folder = self
                        .disk
                        .current()
                        .ok_or(INVALID_DISK)?
                        .folder_mut(&name)
                        .ok_or(NOT_FOUND)?
                        .clone();
                    self.load_folder_tree(&folder);
                    None
                }
                0x16 => {
                    let f = self.disk.current().ok_or(INVALID_DISK)?;
                    if f.folder_mut(&name).is_some() {
                        return Err(CREATE_FAILED);
                    }
                    f.folders.push((name, Folder::default()));
                    None
                }
                0x17 => {
                    let f = self.disk.current().ok_or(INVALID_DISK)?;
                    let i = f
                        .folders
                        .iter()
                        .position(|(n, _)| n.eq_ignore_ascii_case(&name))
                        .ok_or(NOT_FOUND)?;
                    if f.folders[i].1 != Folder::default() {
                        return Err(0x105);
                    }
                    f.folders.remove(i);
                    None
                }
                0x18 => {
                    let new = encode::read_string(&args[name.len() + 1..]).0;
                    let f = self.disk.current().ok_or(INVALID_DISK)?;
                    f.folders
                        .iter_mut()
                        .find(|(n, _)| n.eq_ignore_ascii_case(&name))
                        .ok_or(NOT_FOUND)?
                        .0 = new;
                    None
                }
                0x20 => {
                    Some(encode::u14(self.disk.current().ok_or(INVALID_DISK)?.files.len() as u16).to_vec())
                }
                0x22 => Some(
                    self.disk
                        .current()
                        .ok_or(INVALID_DISK)?
                        .files
                        .iter()
                        .flat_map(|(n, _)| encode::string(n))
                        .collect(),
                ),
                0x23 => {
                    let i = read_u14(args).unwrap_or(0) as usize;
                    let f = self.disk.current().ok_or(INVALID_DISK)?;
                    Some(split7(f.files.get(i).ok_or(NOT_FOUND)?.1.len() as u64, 4))
                }
                0x28 => {
                    let new = encode::read_string(&args[name.len() + 1..]).0;
                    let f = self.disk.current().ok_or(INVALID_DISK)?;
                    let entry = f
                        .files
                        .iter_mut()
                        .find(|(n, _)| n.eq_ignore_ascii_case(&name))
                        .ok_or(NOT_FOUND)?;
                    let ext = name
                        .rsplit_once('.')
                        .map(|(_, e)| e.to_string())
                        .unwrap_or_default();
                    entry.0 = format!("{new}.{ext}");
                    None
                }
                0x29 => {
                    let f = self.disk.current().ok_or(INVALID_DISK)?;
                    let i = f
                        .files
                        .iter()
                        .position(|(n, _)| n.eq_ignore_ascii_case(&name))
                        .ok_or(NOT_FOUND)?;
                    f.files.remove(i);
                    None
                }
                0x2A | 0x2B => {
                    let folder = self.disk.current().ok_or(INVALID_DISK)?.clone();
                    if !self.load_from_folder(&folder, &name, item == 0x2B) {
                        return Err(NOT_FOUND);
                    }
                    None
                }
                0x2C => {
                    let index = read_u14(args).unwrap_or(0) as usize;
                    let k = match args.get(2) {
                        Some(1) => 2,
                        Some(2) => 1,
                        Some(3) => 0,
                        _ => return Err(0x02),
                    };
                    let overwrite = args.get(3) == Some(&1);
                    let children = args.get(4) == Some(&1);
                    self.save_item(k, index, overwrite, children)?;
                    None
                }
                0x2D => {
                    let k = match args.first() {
                        Some(1) => 2,
                        Some(2) => 1,
                        Some(3) => 0,
                        _ => return Err(0x02),
                    };
                    let overwrite = args.get(1) == Some(&1);
                    for i in 0..self.items(k).len() {
                        self.save_item(k, i, overwrite, false)?;
                    }
                    None
                }
                _ => return Err(0x00),
            })
        })();
        match result {
            Ok(Some(data)) => self.reply(msg, 0x52, &data),
            Ok(None) => self.done(msg),
            Err(code) => self.error(msg, code),
        }
    }

    /// Raw `41 <name>`: the bytes of a file in the current folder.
    pub(crate) fn disk_file(&mut self, name: &str) -> Vec<u8> {
        self.disk
            .current()
            .and_then(|f| f.file(name).map(<[u8]>::to_vec))
            .unwrap_or_default()
    }

    /// Completes a raw `40` upload into the current folder.
    pub(crate) fn disk_store(&mut self, name: &str, bytes: Vec<u8>) {
        if let Some(f) = self.disk.current() {
            f.put_file(name, bytes, true);
        }
    }
}
