//! A connected sampler. All calls are blocking and must be made from one
//! thread at a time; the GUI runs them on a worker thread.

use std::time::{Duration, Instant};

use s5k_protocol::encode::{self, read_strings, read_u14, read_u28};
use s5k_protocol::meters::Meters;
use s5k_protocol::panel::{self, Key, SCREEN_BYTES, Screen};
use s5k_protocol::sysex::section;
use s5k_protocol::transport::{HANDSHAKE, KEEP_ALIVE, MAX_TRANSFER, PACKET};
use s5k_protocol::{Assembler, Download, ItemKind, Message, Reply, Status, Step, Transport, Upload};

use crate::error::{Error, Result};

/// How long to wait for a normal reply.
pub const REPLY_TIMEOUT: Duration = Duration::from_secs(3);
/// How long to wait for the optional notice after an upload completes.
const NOTICE_TIMEOUT: Duration = Duration::from_millis(150);
/// How long to wait during disk operations, which can take minutes.
pub const DISK_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SamplerInfo {
    pub name: String,
    pub os_version: (u8, u8),
    pub wave_free_percent: u8,
    pub cpu_free_percent: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SampleInfo {
    pub index: u16,
    pub name: String,
    pub channels: u16,
    pub frames: u32,
    pub sample_rate: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgramInfo {
    pub index: u16,
    pub name: String,
    pub keygroups: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MultiInfo {
    pub index: u16,
    pub name: String,
    pub parts: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryList {
    pub samples: Vec<SampleInfo>,
    pub programs: Vec<ProgramInfo>,
    pub multis: Vec<MultiInfo>,
}

/// Progress callback: (bytes done, bytes total if known).
pub type Progress<'a> = &'a mut dyn FnMut(u64, Option<u64>);

pub struct Sampler<T: Transport> {
    transport: T,
    assembler: Assembler,
    timeout: Duration,
    /// Set after a timeout or unexpected data: late bytes may still arrive
    /// and must be cleared before the next command.
    stale: bool,
}

fn index_data(index: u16) -> [u8; 2] {
    encode::u14(index)
}

impl<T: Transport> Sampler<T> {
    /// Starts a session: the ak.Sys handshake, then turns off "OK"
    /// acknowledgements and front-panel item sync.
    pub fn connect(transport: T) -> Result<Self> {
        let mut s = Self {
            transport,
            assembler: Assembler::new(),
            timeout: REPLY_TIMEOUT,
            stale: true,
        };
        for packet in HANDSHAKE {
            s.transport.write(packet)?;
        }
        s.request(Message::new(section::SYSEX_CONFIG, 0x01, [0]))?;
        s.request(Message::new(section::SYSEX_CONFIG, 0x03, [0]))?;
        Ok(s)
    }

    pub fn transport_mut(&mut self) -> &mut T {
        &mut self.transport
    }

    /// Reads one transfer of at most `len` bytes (the exact length expected;
    /// see `Transport::read`). A timeout marks the line as stale.
    fn read_raw(&mut self, len: usize, timeout: Duration) -> Result<Vec<u8>> {
        self.transport.read(len, timeout).map_err(|e| {
            self.stale = true;
            e.into()
        })
    }

    /// Discards anything left over from an earlier exchange that timed out
    /// or went wrong, so it cannot be mistaken for the next reply.
    fn drain(&mut self) {
        if !std::mem::take(&mut self.stale) {
            return;
        }
        self.assembler = Assembler::new();
        let deadline = Instant::now() + Duration::from_millis(500);
        while Instant::now() < deadline
            && self
                .transport
                .read(MAX_TRANSFER, Duration::from_millis(30))
                .is_ok()
        {}
    }

    /// Writes a raw command after clearing any stale input.
    fn write_raw(&mut self, packet: &[u8]) -> Result<()> {
        self.drain();
        Ok(self.transport.write(packet)?)
    }

    fn write_message(&mut self, msg: &Message) -> Result<()> {
        self.drain();
        for packet in msg.usb_packets() {
            self.transport.write(&packet)?;
        }
        Ok(())
    }

    /// Sends a SysEx command and returns its "done" or "reply" confirmation.
    /// Error confirmations become `Error::Sampler`.
    pub fn request(&mut self, msg: Message) -> Result<Reply> {
        self.request_with_timeout(msg, self.timeout)
    }

    pub fn request_with_timeout(&mut self, msg: Message, timeout: Duration) -> Result<Reply> {
        self.write_message(&msg)?;
        let deadline = Instant::now() + timeout;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(std::io::Error::from(std::io::ErrorKind::TimedOut).into());
            }
            let packet = self.read_raw(PACKET, left)?;
            let Some(bytes) = self.assembler.push(&packet) else {
                continue;
            };
            let reply = Reply::parse(&bytes).inspect_err(|_| self.stale = true)?;
            if reply.section != msg.section || reply.item != msg.reply_item() {
                continue; // a stale or unsolicited message
            }
            match reply.status {
                Status::Ok => continue,
                Status::Done | Status::Reply => return Ok(reply),
                Status::Error => {
                    return Err(Error::Sampler {
                        section: msg.section,
                        item: msg.item,
                        code: reply.error_code().unwrap_or(0xFFFF),
                    });
                }
            }
        }
    }

    fn reply_data(&mut self, section: u8, item: u8, data: &[u8]) -> Result<Vec<u8>> {
        Ok(self.request(Message::new(section, item, data.to_vec()))?.data)
    }

    fn u14_reply(&mut self, section: u8, item: u8, data: &[u8]) -> Result<u16> {
        let d = self.reply_data(section, item, data)?;
        read_u14(&d).ok_or_else(|| Error::Protocol(format!("short reply to {section:02X} {item:02X}")))
    }

    // --- System ---------------------------------------------------------

    pub fn info(&mut self) -> Result<SamplerInfo> {
        let name = encode::read_string(&self.reply_data(section::SYSTEM, 0x03, &[])?).0;
        let v = self.reply_data(section::SYSTEM, 0x00, &[])?;
        let wave = self.reply_data(section::SYSTEM, 0x30, &[])?;
        let cpu = self.reply_data(section::SYSTEM, 0x31, &[])?;
        Ok(SamplerInfo {
            name,
            os_version: (v.first().copied().unwrap_or(0), v.get(1).copied().unwrap_or(0)),
            wave_free_percent: wave.first().copied().unwrap_or(0),
            cpu_free_percent: cpu.first().copied().unwrap_or(0),
        })
    }

    /// Raw keep-alive. ak.Sys sends one about every 200 ms; call it when idle.
    pub fn keep_alive(&mut self) -> Result<()> {
        self.write_raw(&[KEEP_ALIVE])?;
        let reply = self.read_raw(4, self.timeout)?;
        if reply.first() == Some(&1) {
            // A 128-byte block follows once multis exist (meaning unknown).
            self.read_raw(128, self.timeout)?;
        }
        Ok(())
    }

    /// Deletes everything in memory.
    pub fn clear_memory(&mut self) -> Result<()> {
        self.request(Message::new(section::SYSTEM, 0x32, []))?;
        Ok(())
    }

    // --- Listing memory ---------------------------------------------------

    pub fn sample_count(&mut self) -> Result<u16> {
        self.u14_reply(section::SAMPLE, 0x10, &[])
    }

    pub fn program_count(&mut self) -> Result<u16> {
        self.u14_reply(section::PROGRAM, 0x10, &[])
    }

    pub fn multi_count(&mut self) -> Result<u16> {
        self.u14_reply(section::MULTI, 0x40, &[])
    }

    /// Names via a "list all" command; the sampler answers an empty list
    /// with an error, so the count is checked first.
    fn all_names(&mut self, count: u16, section: u8, item: u8) -> Result<Vec<String>> {
        if count == 0 {
            return Ok(Vec::new());
        }
        let mut names = read_strings(&self.reply_data(section, item, &[])?);
        names.truncate(count as usize);
        Ok(names)
    }

    pub fn sample_names(&mut self) -> Result<Vec<String>> {
        let n = self.sample_count()?;
        self.all_names(n, section::SAMPLE, 0x12)
    }

    pub fn program_names(&mut self) -> Result<Vec<String>> {
        let n = self.program_count()?;
        self.all_names(n, section::PROGRAM, 0x19)
    }

    pub fn multi_names(&mut self) -> Result<Vec<String>> {
        let n = self.multi_count()?;
        self.all_names(n, section::MULTI, 0x51)
    }

    pub fn sample_info(&mut self, index: u16, name: String) -> Result<SampleInfo> {
        let d = self.indexed(section::SAMPLE_BY_INDEX, index, 0x34, &[])?;
        Ok(SampleInfo {
            index,
            name,
            channels: read_u14(&d).unwrap_or(0),
            frames: d.get(2..).and_then(read_u28).unwrap_or(0),
            sample_rate: d.get(6..).and_then(read_u28).unwrap_or(0),
        })
    }

    pub fn program_keygroups(&mut self, index: u16) -> Result<u8> {
        let d = self.indexed(section::PROGRAM_BY_INDEX, index, 0x14, &[])?;
        Ok(d.first().copied().unwrap_or(0))
    }

    pub fn multi_parts(&mut self, index: u16) -> Result<u16> {
        let d = self.indexed(section::MULTI_BY_INDEX, index, 0x44, &[])?;
        Ok(d.first().map_or(0, |&n| n as u16 + 1))
    }

    /// Everything in memory, with sizes.
    pub fn memory(&mut self) -> Result<MemoryList> {
        let samples = self
            .sample_names()?
            .into_iter()
            .enumerate()
            .map(|(i, name)| self.sample_info(i as u16, name))
            .collect::<Result<_>>()?;
        let programs = self
            .program_names()?
            .into_iter()
            .enumerate()
            .map(|(i, name)| {
                Ok(ProgramInfo {
                    index: i as u16,
                    keygroups: self.program_keygroups(i as u16)?,
                    name,
                })
            })
            .collect::<Result<_>>()?;
        let multis = self
            .multi_names()?
            .into_iter()
            .enumerate()
            .map(|(i, name)| {
                Ok(MultiInfo {
                    index: i as u16,
                    parts: self.multi_parts(i as u16)?,
                    name,
                })
            })
            .collect::<Result<_>>()?;
        Ok(MemoryList {
            samples,
            programs,
            multis,
        })
    }

    /// Program names used by a multi's parts (empty parts omitted).
    pub fn multi_part_programs(&mut self, multi: u16) -> Result<Vec<String>> {
        let parts = self.multi_parts(multi)?;
        let mut names = Vec::new();
        for part in 0..parts {
            let d = self.indexed(section::MULTI_BY_INDEX, multi, 0x45, &[part as u8])?;
            let name = encode::read_string(&d).0;
            if !name.is_empty() && !names.contains(&name) {
                names.push(name);
            }
        }
        Ok(names)
    }

    // --- Managing memory --------------------------------------------------

    fn kind_section(kind: ItemKind) -> Result<u8> {
        match kind {
            ItemKind::Sample => Ok(section::SAMPLE),
            ItemKind::Program => Ok(section::PROGRAM),
            ItemKind::Multi => Ok(section::MULTI),
            ItemKind::MidiFile => Ok(section::SONG),
        }
    }

    /// Makes an item current (`xx 06 <index>`).
    pub fn select(&mut self, kind: ItemKind, index: u16) -> Result<()> {
        self.request(Message::new(Self::kind_section(kind)?, 0x06, index_data(index)))?;
        Ok(())
    }

    pub fn delete(&mut self, kind: ItemKind, index: u16) -> Result<()> {
        self.select(kind, index)?;
        self.request(Message::new(Self::kind_section(kind)?, 0x08, []))?;
        Ok(())
    }

    pub fn rename(&mut self, kind: ItemKind, index: u16, name: &str) -> Result<()> {
        let item = match kind {
            ItemKind::Sample | ItemKind::Program => 0x09,
            ItemKind::Multi => 0x30,
            ItemKind::MidiFile => return Err(Error::Invalid("MIDI files cannot be renamed here".into())),
        };
        self.select(kind, index)?;
        self.request(Message::new(
            Self::kind_section(kind)?,
            item,
            encode::string(name),
        ))?;
        Ok(())
    }

    pub fn create_program(&mut self, name: &str, keygroups: u8) -> Result<()> {
        let mut data = vec![keygroups.clamp(1, 99)];
        data.extend(encode::string(name));
        self.request(Message::new(section::PROGRAM, 0x03, data))?;
        Ok(())
    }

    /// Creates a multi with 32, 64 or 128 parts and names it.
    pub fn create_multi(&mut self, name: &str, parts: u16) -> Result<()> {
        let size = match parts {
            32 => 0,
            64 => 1,
            128 => 2,
            _ => return Err(Error::Invalid("a multi has 32, 64 or 128 parts".into())),
        };
        self.request(Message::new(section::MULTI, 0x01, [size]))?;
        self.request(Message::new(section::MULTI, 0x02, encode::string(name)))?;
        Ok(())
    }

    /// Starts or stops playing a sample on the sampler's outputs.
    pub fn audition(&mut self, sample: u16, play: bool) -> Result<()> {
        let item = if play { 0x0A } else { 0x0B };
        self.indexed(section::SAMPLE_BY_INDEX, sample, item, &[])
            .map(drop)
    }

    // --- Transfers ----------------------------------------------------------

    /// Fetches an item from memory as a complete file (`.wav`, `.akp`, `.akm`).
    pub fn get(&mut self, kind: ItemKind, index: u16, progress: Progress) -> Result<Vec<u8>> {
        let (download, request) = Download::start(kind, index);
        self.run_download(download, &request, progress)
    }

    /// Fetches a file from the current folder of the selected disk
    /// (untested on hardware).
    pub fn get_from_disk(&mut self, file_name: &str, progress: Progress) -> Result<Vec<u8>> {
        let (download, request) = Download::start_disk(file_name);
        self.run_download(download, &request, progress)
    }

    fn run_download(
        &mut self,
        mut download: Download,
        request: &[u8],
        progress: Progress,
    ) -> Result<Vec<u8>> {
        self.write_raw(request)?;
        loop {
            let packet = self.read_raw(download.next_read_len(), self.timeout)?;
            match download.feed(&packet).inspect_err(|_| self.stale = true)? {
                Step::Read => {}
                Step::Send(packets) => {
                    for p in packets {
                        self.transport.write(&p)?;
                    }
                }
                Step::Done => return Ok(download.into_data()),
            }
            progress(download.received() as u64, None);
        }
    }

    /// Sends a file into memory. `name` must carry the extension
    /// (`.wav`, `.akp`, `.akm`); the sampler uses it for the item's name.
    pub fn put(&mut self, kind: ItemKind, name: &str, file: &[u8], progress: Progress) -> Result<()> {
        let (upload, request) = Upload::start(kind, name, file);
        self.run_upload(upload, &request, file, progress)
    }

    /// Writes a file into the current folder of the selected disk
    /// (untested on hardware).
    pub fn put_to_disk(&mut self, file_name: &str, file: &[u8], progress: Progress) -> Result<()> {
        let (upload, request) = Upload::start_disk(file_name, file);
        self.run_upload(upload, &request, file, progress)
    }

    fn run_upload(
        &mut self,
        mut upload: Upload,
        request: &[u8],
        file: &[u8],
        progress: Progress,
    ) -> Result<()> {
        self.write_raw(request)?;
        let total = Some(file.len() as u64);
        loop {
            // The file is complete once the sampler says so; the notice that
            // may follow is optional and often arrives late or not at all, so
            // wait for it only briefly (a late one is drained before the next
            // command).
            let timeout = if upload.finished() {
                NOTICE_TIMEOUT
            } else {
                self.timeout
            };
            let packet = match self.read_raw(upload.next_read_len(), timeout) {
                Ok(p) => p,
                Err(e) if upload.finished() && e.is_timeout() => return Ok(()),
                Err(e) => return Err(e),
            };
            match upload.feed(&packet).inspect_err(|_| self.stale = true)? {
                Step::Read => {}
                Step::Send(packets) => {
                    for p in packets {
                        self.transport.write(&p)?;
                    }
                    progress(upload.progress() as u64, total);
                }
                Step::Done => {
                    progress(file.len() as u64, total);
                    return Ok(());
                }
            }
        }
    }

    // --- Front panel and meters -------------------------------------------

    pub fn screen(&mut self) -> Result<Screen> {
        self.write_raw(&[panel::SCREEN_REQUEST])?;
        // 9600 bytes in transfers of 4096, 4096 and 1408, then the LED byte.
        let mut bits = Vec::with_capacity(SCREEN_BYTES);
        while bits.len() < SCREEN_BYTES {
            let want = (SCREEN_BYTES - bits.len()).min(MAX_TRANSFER);
            let p = self.read_raw(want, self.timeout)?;
            if p == s5k_protocol::sysex::BUSY {
                continue;
            }
            bits.extend_from_slice(&p);
        }
        let leds = self.read_raw(1, self.timeout)?.first().copied().unwrap_or(0);
        Screen::from_reply(&bits, leds).ok_or_else(|| {
            self.stale = true;
            Error::Protocol("short screen image".into())
        })
    }

    pub fn press(&mut self, key: Key) -> Result<()> {
        self.request(panel::press(key)).map(drop)
    }

    pub fn release(&mut self, key: Key) -> Result<()> {
        self.request(panel::release(key)).map(drop)
    }

    /// Presses and releases a key.
    pub fn tap(&mut self, key: Key) -> Result<()> {
        self.press(key)?;
        self.release(key)
    }

    pub fn wheel(&mut self, clicks: i8) -> Result<()> {
        self.request(panel::wheel(clicks)).map(drop)
    }

    pub fn meters(&mut self) -> Result<Meters> {
        self.write_raw(&[s5k_protocol::meters::REQUEST])?;
        let reply = self.read_raw(s5k_protocol::meters::REPLY_LEN, self.timeout)?;
        Meters::parse(&reply).ok_or_else(|| {
            self.stale = true;
            Error::Protocol("bad meter reply".into())
        })
    }

    // --- Parameters by index ---------------------------------------------------

    /// A command on a program, multi, sample or FX setup given by index (the
    /// spec's alternative sections `2A`/`2C`/`2E`/`32`). Returns the reply data.
    pub fn indexed(&mut self, by_index_section: u8, index: u16, item: u8, data: &[u8]) -> Result<Vec<u8>> {
        Ok(self
            .request(Message::by_index(by_index_section, index, item, data))?
            .data)
    }

    /// Blocked request (`38`/`3A`/`3C`/`3E`): several parameters of one
    /// program (and keygroup) in one message. Used for reads, and with a
    /// single entry for writes, as ak.Sys does. Returns the reply data: the
    /// values concatenated in request order.
    pub fn block(
        &mut self,
        block_section: u8,
        index: u16,
        keygroup: Option<u8>,
        entries: &[(u8, [u8; 3])],
    ) -> Result<Vec<u8>> {
        if entries.is_empty() {
            return Err(Error::Invalid("no parameters requested".into()));
        }
        Ok(self
            .request(Message::block(block_section, index, keygroup, entries))?
            .data)
    }

    // --- Disks ------------------------------------------------------------------

    pub fn disk_request(&mut self, item: u8, data: &[u8]) -> Result<Vec<u8>> {
        let timeout = if matches!(item, 0x15 | 0x2A | 0x2B | 0x2C | 0x2D) {
            DISK_TIMEOUT
        } else {
            self.timeout
        };
        Ok(self
            .request_with_timeout(Message::new(section::DISK, item, data.to_vec()), timeout)?
            .data)
    }

    pub fn u14_disk(&mut self, item: u8) -> Result<u16> {
        self.u14_reply(section::DISK, item, &[])
    }
}
