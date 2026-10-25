//! Disk tools (docs/protocol.md §13; spec tables 20–21). Untested on
//! hardware so far.

use s5k_protocol::Transport;
use s5k_protocol::encode::{self, read_strings, read_u14, read_u28, read_u56};

use crate::error::{Error, Result};
use crate::sampler::Sampler;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiskType {
    Floppy,
    HardDisk,
    CdRom,
    Removable,
    Unknown(u8),
}

impl From<u8> for DiskType {
    fn from(v: u8) -> Self {
        match v {
            0 => DiskType::Floppy,
            1 => DiskType::HardDisk,
            2 => DiskType::CdRom,
            3 => DiskType::Removable,
            other => DiskType::Unknown(other),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiskFormat {
    Other,
    MsDos,
    Fat32,
    Iso9660,
    S1000,
    S3000,
    Emu,
    Roland,
    Unknown(u8),
}

impl From<u8> for DiskFormat {
    fn from(v: u8) -> Self {
        use DiskFormat::*;
        [Other, MsDos, Fat32, Iso9660, S1000, S3000, Emu, Roland]
            .get(v as usize)
            .copied()
            .unwrap_or(Unknown(v))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Disk {
    pub handle: u16,
    pub kind: DiskType,
    pub format: DiskFormat,
    pub scsi_id: u8,
    pub writable: bool,
    pub name: String,
}

/// What `save` writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveType {
    Multi = 1,
    Program = 2,
    Sample = 3,
    MidiFile = 4,
    Setlist = 5,
    Scenelist = 6,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadMode {
    Normal = 0,
    SamplesToRam = 1,
    SamplesVirtual = 2,
}

/// Parses the disk list: (handle, type, format, SCSI ID, writable, name) repeated.
pub fn parse_disk_list(mut d: &[u8]) -> Vec<Disk> {
    let mut disks = Vec::new();
    while d.len() >= 6 {
        let handle = read_u14(d).unwrap_or(0);
        let (name, rest) = encode::read_string(&d[6..]);
        disks.push(Disk {
            handle,
            kind: DiskType::from(d[2]),
            format: DiskFormat::from(d[3]),
            scsi_id: d[4],
            writable: d[5] != 0,
            name,
        });
        d = rest;
    }
    disks
}

impl<T: Transport> Sampler<T> {
    /// Rescans the connected disks. Do this after a disk is added or removed.
    pub fn refresh_disks(&mut self) -> Result<()> {
        self.disk_request(0x01, &[]).map(drop)
    }

    pub fn disks(&mut self) -> Result<Vec<Disk>> {
        Ok(parse_disk_list(&self.disk_request(0x05, &[])?))
    }

    /// Selects the disk all other disk operations act on (separate from the
    /// disk chosen on the front panel).
    pub fn select_disk(&mut self, handle: u16) -> Result<()> {
        self.disk_request(0x02, &encode::u14(handle)).map(drop)
    }

    /// Current folder on the selected disk; empty at the root.
    pub fn disk_path(&mut self) -> Result<String> {
        Ok(encode::read_string(&self.disk_request(0x09, &[])?).0)
    }

    /// Free space on the selected disk, in bytes.
    pub fn disk_free(&mut self) -> Result<u64> {
        read_u56(&self.disk_request(0x0B, &[])?)
            .ok_or_else(|| Error::Protocol("short free-space reply".into()))
    }

    pub fn folders(&mut self) -> Result<Vec<String>> {
        if self.u14_disk(0x10)? == 0 {
            return Ok(Vec::new());
        }
        Ok(read_strings(&self.disk_request(0x12, &[])?))
    }

    pub fn files(&mut self) -> Result<Vec<String>> {
        if self.u14_disk(0x20)? == 0 {
            return Ok(Vec::new());
        }
        Ok(read_strings(&self.disk_request(0x22, &[])?))
    }

    pub fn file_size(&mut self, index: u16) -> Result<u32> {
        read_u28(&self.disk_request(0x23, &encode::u14(index))?)
            .ok_or_else(|| Error::Protocol("short file-size reply".into()))
    }

    /// Opens a sub-folder; an empty name goes to the root.
    pub fn open_folder(&mut self, name: &str) -> Result<()> {
        self.disk_request(0x13, &encode::string(name)).map(drop)
    }

    pub fn parent_folder(&mut self) -> Result<()> {
        self.disk_request(0x14, &[]).map(drop)
    }

    pub fn create_folder(&mut self, name: &str) -> Result<()> {
        self.disk_request(0x16, &encode::string(name)).map(drop)
    }

    pub fn delete_folder(&mut self, name: &str) -> Result<()> {
        self.disk_request(0x17, &encode::string(name)).map(drop)
    }

    /// Loads a folder and everything in it into memory.
    pub fn load_folder(&mut self, name: &str) -> Result<()> {
        self.disk_request(0x15, &encode::string(name)).map(drop)
    }

    pub fn delete_file(&mut self, name: &str) -> Result<()> {
        self.disk_request(0x29, &encode::string(name)).map(drop)
    }

    /// Renames a file; `new_name` has no extension.
    pub fn rename_file(&mut self, name: &str, new_name: &str) -> Result<()> {
        let mut data = encode::string(name);
        data.extend(encode::string(new_name));
        self.disk_request(0x28, &data).map(drop)
    }

    /// Loads a file (name with extension) into memory. With
    /// `with_dependencies`, a program's samples or a multi's programs load too.
    pub fn load_file(&mut self, name: &str, mode: LoadMode, with_dependencies: bool) -> Result<()> {
        if with_dependencies {
            self.disk_request(0x2B, &encode::string(name)).map(drop)
        } else {
            let mut data = encode::string(name);
            data.push(mode as u8);
            self.disk_request(0x2A, &data).map(drop)
        }
    }

    /// Saves a memory item into the current folder.
    pub fn save_to_disk(
        &mut self,
        index: u16,
        kind: SaveType,
        overwrite: bool,
        with_dependencies: bool,
    ) -> Result<()> {
        let mut data = encode::u14(index).to_vec();
        data.extend([kind as u8, overwrite as u8, with_dependencies as u8]);
        self.disk_request(0x2C, &data).map(drop)
    }

    pub fn save_all_to_disk(
        &mut self,
        kind: SaveType,
        overwrite: bool,
        with_dependencies: bool,
    ) -> Result<()> {
        self.disk_request(0x2D, &[kind as u8, overwrite as u8, with_dependencies as u8])
            .map(drop)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_disk_list() {
        let mut d = vec![0, 5, 1, 2, 3, 1];
        d.extend(b"Disk A\0");
        d.extend([1, 0, 0, 1, 0, 0]);
        d.extend(b"Floppy\0");
        let disks = parse_disk_list(&d);
        assert_eq!(disks.len(), 2);
        assert_eq!(disks[0].handle, 5);
        assert_eq!(disks[0].kind, DiskType::HardDisk);
        assert_eq!(disks[0].format, DiskFormat::Fat32);
        assert!(disks[0].writable);
        assert_eq!(disks[1].handle, 128);
        assert_eq!(disks[1].name, "Floppy");
    }
}
