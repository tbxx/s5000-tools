//! A simulated S5000 that answers like the real one: memory with samples,
//! programs and multis, transfers, the screen and meters. Used for tests and
//! to develop or demonstrate the tools without hardware.

use std::collections::VecDeque;
use std::io;
use std::time::Duration;

use s5k_formats::{Multi, Program, Wav};
use s5k_protocol::encode::{self, read_u14, split7};
use s5k_protocol::panel::SCREEN_BYTES;
use s5k_protocol::sysex::{self, section};
use s5k_protocol::{Message, Transport};

#[derive(Debug)]
enum Mode {
    Idle,
    /// Sending a file to the host in blocks; waiting for the host's `00`.
    Download {
        data: Vec<u8>,
        sent: usize,
    },
    /// Receiving a file: requested `want` bytes at `offset`.
    Upload {
        kind: u8,
        name: String,
        file: Vec<u8>,
        offset: usize,
        want: usize,
        got: usize,
    },
}

/// The simulated sampler. Implements [`Transport`], so a [`crate::Sampler`]
/// can be connected to it directly.
#[derive(Debug)]
pub struct Simulator {
    pub samples: Vec<(String, Vec<u8>)>,
    pub programs: Vec<(String, Vec<u8>)>,
    pub multis: Vec<(String, Vec<u8>)>,
    current: [usize; 3],
    out: VecDeque<Vec<u8>>,
    pending: Vec<u8>,
    mode: Mode,
    upload_size: usize,
    /// Every SysEx message received, for tests.
    pub log: Vec<Message>,
}

const BLOCK: usize = 16384;

/// A short stereo WAV of silence.
pub fn demo_wav(frames: usize) -> Vec<u8> {
    let mut fmt = Vec::new();
    for v in [1u16, 2] {
        fmt.extend_from_slice(&v.to_le_bytes());
    }
    fmt.extend_from_slice(&44100u32.to_le_bytes());
    fmt.extend_from_slice(&(44100u32 * 4).to_le_bytes());
    fmt.extend_from_slice(&4u16.to_le_bytes());
    fmt.extend_from_slice(&16u16.to_le_bytes());
    let mut body = Vec::new();
    for (id, data) in [(b"fmt ", fmt), (b"data", vec![0u8; frames * 4])] {
        body.extend_from_slice(id);
        body.extend_from_slice(&(data.len() as u32).to_le_bytes());
        body.extend_from_slice(&data);
    }
    let mut out = b"RIFF".to_vec();
    out.extend_from_slice(&(body.len() as u32 + 4).to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(&body);
    out
}

impl Default for Simulator {
    fn default() -> Self {
        Self::empty()
    }
}

impl Simulator {
    pub fn empty() -> Self {
        Self {
            samples: Vec::new(),
            programs: Vec::new(),
            multis: Vec::new(),
            current: [0; 3],
            out: VecDeque::new(),
            pending: Vec::new(),
            mode: Mode::Idle,
            upload_size: 0,
            log: Vec::new(),
        }
    }

    /// A sampler with a few made-up items in memory.
    pub fn demo() -> Self {
        let mut sim = Self::empty();
        sim.samples.push(("Kick".into(), demo_wav(11025)));
        sim.samples.push(("Snare".into(), demo_wav(8000)));
        sim.samples.push(("Pad C3".into(), demo_wav(88200)));
        let mut drums = Program::blank(2);
        drums.keygroup_mut(0).unwrap().set_zone_sample(0, "Kick").unwrap();
        drums
            .keygroup_mut(1)
            .unwrap()
            .set_zone_sample(0, "Snare")
            .unwrap();
        sim.programs.push(("Drums".into(), drums.to_bytes()));
        let mut pad = Program::blank(1);
        pad.keygroup_mut(0).unwrap().set_zone_sample(0, "Pad C3").unwrap();
        sim.programs.push(("Pad".into(), pad.to_bytes()));
        let mut multi = Multi::blank(32);
        multi.set_part_program(0, "Drums").unwrap();
        multi.set_part_program(1, "Pad").unwrap();
        sim.multis.push(("Song".into(), multi.to_bytes()));
        sim
    }

    fn list(&mut self, kind: usize) -> &mut Vec<(String, Vec<u8>)> {
        match kind {
            0 => &mut self.samples,
            1 => &mut self.programs,
            _ => &mut self.multis,
        }
    }

    fn reply(&mut self, msg: &Message, status: u8, data: &[u8]) {
        let mut out = vec![
            0xF0,
            sysex::AKAI,
            sysex::MODEL,
            sysex::DEVICE,
            0,
            0,
            status,
            msg.section,
        ];
        out.push(msg.reply_item());
        out.extend_from_slice(data);
        out.push(0xF7);
        // The real sampler sends long replies in 64-byte packets.
        for chunk in out.chunks(64) {
            self.out.push_back(chunk.to_vec());
        }
    }

    fn done(&mut self, msg: &Message) {
        self.reply(msg, 0x44, &[]);
    }

    fn error(&mut self, msg: &Message, code: u16) {
        self.reply(msg, 0x45, &encode::u14(code));
    }

    fn names(&self, kind: usize) -> Vec<u8> {
        let list = match kind {
            0 => &self.samples,
            1 => &self.programs,
            _ => &self.multis,
        };
        list.iter().flat_map(|(n, _)| encode::string(n)).collect()
    }

    fn handle(&mut self, msg: Message) {
        self.log.push(msg.clone());
        let d = msg.data.clone();
        let indexed = sysex::is_indexed(msg.section);
        let (index, rest) = if indexed {
            (read_u14(&[msg.item, d[0]]).unwrap_or(0) as usize, &d[1..])
        } else {
            (0, &d[..])
        };
        let item = if indexed {
            rest.first().copied().unwrap_or(0)
        } else {
            msg.item
        };
        let args = if indexed { &rest[1.min(rest.len())..] } else { rest };
        let kind_of = |s: u8| match s {
            section::SAMPLE | section::SAMPLE_BY_INDEX => Some(0),
            section::PROGRAM | section::PROGRAM_BY_INDEX => Some(1),
            section::MULTI | section::MULTI_BY_INDEX => Some(2),
            _ => None,
        };
        let count = |sim: &Self, k: usize| match k {
            0 => sim.samples.len(),
            1 => sim.programs.len(),
            _ => sim.multis.len(),
        };

        match (msg.section, item) {
            (section::SYSEX_CONFIG, _) => self.done(&msg),
            (section::SYSTEM, 0x03) => self.reply(&msg, 0x52, &encode::string("AKAI S5000 (simulated)")),
            (section::SYSTEM, 0x00) => self.reply(&msg, 0x52, &[2, 14]),
            (section::SYSTEM, 0x30 | 0x31) => self.reply(&msg, 0x52, &[99]),
            (section::SYSTEM, 0x32) => {
                self.samples.clear();
                self.programs.clear();
                self.multis.clear();
                self.done(&msg)
            }
            (section::SAMPLE, 0x10) | (section::PROGRAM, 0x10) | (section::MULTI, 0x40) => {
                let k = kind_of(msg.section).unwrap();
                let n = count(self, k) as u16;
                self.reply(&msg, 0x52, &encode::u14(n))
            }
            (section::SAMPLE, 0x12) | (section::PROGRAM, 0x19) | (section::MULTI, 0x51) => {
                let k = kind_of(msg.section).unwrap();
                if count(self, k) == 0 {
                    return self.error(&msg, 4);
                }
                let names = self.names(k);
                self.reply(&msg, 0x52, &names)
            }
            (s, 0x06) if kind_of(s).is_some() && !indexed => {
                let k = kind_of(s).unwrap();
                let i = read_u14(args).unwrap_or(0) as usize;
                if i >= count(self, k) {
                    return self.error(&msg, 4);
                }
                self.current[k] = i;
                self.done(&msg)
            }
            (s, 0x08) if kind_of(s).is_some() && !indexed => {
                let k = kind_of(s).unwrap();
                let i = self.current[k];
                if i >= count(self, k) {
                    return self.error(&msg, 6);
                }
                self.list(k).remove(i);
                self.done(&msg)
            }
            (section::SAMPLE | section::PROGRAM, 0x09) | (section::MULTI, 0x30) => {
                let k = kind_of(msg.section).unwrap();
                let i = self.current[k];
                let name = encode::read_string(args).0;
                match self.list(k).get_mut(i) {
                    Some(entry) => {
                        entry.0 = name;
                        self.done(&msg)
                    }
                    None => self.error(&msg, 4),
                }
            }
            (section::PROGRAM, 0x03) => {
                let kgs = args.first().copied().unwrap_or(1) as usize;
                let name = encode::read_string(&args[1.min(args.len())..]).0;
                self.programs.push((name, Program::blank(kgs).to_bytes()));
                self.current[1] = self.programs.len() - 1;
                self.done(&msg)
            }
            (section::MULTI, 0x01) => {
                let parts = [32, 64, 128][args.first().copied().unwrap_or(0).min(2) as usize];
                let n = self.multis.len() + 1;
                self.multis
                    .push((format!("New Multi  {n}"), Multi::blank(parts).to_bytes()));
                self.current[2] = self.multis.len() - 1;
                self.done(&msg)
            }
            (section::MULTI, 0x02) => {
                let name = encode::read_string(args).0;
                let i = self.current[2];
                if let Some(entry) = self.multis.get_mut(i) {
                    entry.0 = name;
                }
                self.done(&msg)
            }
            (section::SAMPLE_BY_INDEX, 0x34) => match self.samples.get(index) {
                Some((_, bytes)) => {
                    let (channels, frames, rate) = Wav::parse(bytes)
                        .map(|w| (w.format().channels, w.frames(), w.format().sample_rate))
                        .unwrap_or((0, 0, 0));
                    let mut data = encode::u14(channels).to_vec();
                    data.extend(split7(frames as u64, 4));
                    data.extend(split7(rate as u64, 4));
                    self.reply(&msg, 0x52, &data)
                }
                None => self.error(&msg, 4),
            },
            (section::SAMPLE_BY_INDEX, 0x0A | 0x0B) => {
                if index < self.samples.len() {
                    self.done(&msg)
                } else {
                    self.error(&msg, 4)
                }
            }
            (section::PROGRAM_BY_INDEX, 0x14) => match self.programs.get(index) {
                Some((_, bytes)) => {
                    let n = Program::parse(bytes).map(|p| p.keygroups().count()).unwrap_or(0);
                    self.reply(&msg, 0x52, &[n as u8])
                }
                None => self.error(&msg, 4),
            },
            (section::MULTI_BY_INDEX, 0x44) => match self.multis.get(index) {
                Some((_, bytes)) => {
                    let n = Multi::parse(bytes).map(|m| m.part_count()).unwrap_or(32);
                    self.reply(&msg, 0x52, &[(n - 1) as u8])
                }
                None => self.error(&msg, 4),
            },
            (section::MULTI_BY_INDEX, 0x45) => {
                let part = args.first().copied().unwrap_or(0) as usize;
                let name = self
                    .multis
                    .get(index)
                    .and_then(|(_, b)| Multi::parse(b).ok())
                    .and_then(|m| m.part_program(part))
                    .unwrap_or_default();
                self.reply(&msg, 0x52, &encode::string(&name))
            }
            (section::PANEL, _) => self.done(&msg),
            (section::DISK, 0x04) => self.reply(&msg, 0x52, &[0]),
            (section::DISK, 0x01) => self.done(&msg),
            _ => self.error(&msg, 0),
        }
    }

    fn handle_raw(&mut self, p: &[u8]) {
        if let Mode::Download { data, sent } = &mut self.mode
            && p == [0]
        {
            let (data, sent) = (std::mem::take(data), *sent);
            self.next_block(data, sent);
            return;
        }
        if let Mode::Upload { file, want, got, .. } = &mut self.mode {
            if *got < *want {
                file.extend_from_slice(p);
                *got += p.len();
                return;
            }
            if p == [0] {
                self.next_request();
                return;
            }
        }
        match p.first() {
            Some(0x03 | 0x04) => {}
            Some(0x30) => self.out.push_back(vec![0, 0, 0, 0]),
            Some(0x01) => {
                let screen = vec![0u8; SCREEN_BYTES];
                for chunk in screen.chunks(4096) {
                    self.out.push_back(chunk.to_vec());
                }
                self.out.push_back(vec![0x10]);
            }
            Some(0x50) => {
                let mut m = vec![0x60u8; 32];
                m[..3].copy_from_slice(&[0x50, 0x40, 0x83]);
                self.out.push_back(m);
            }
            Some(&cmd @ (0x21 | 0x23 | 0x25)) if p.len() == 3 => {
                let k = match cmd {
                    0x21 => 0,
                    0x23 => 1,
                    _ => 2,
                };
                let i = read_u14(&p[1..]).unwrap_or(0) as usize;
                let data = self.list(k).get(i).map(|e| e.1.clone()).unwrap_or_default();
                self.next_block(data, 0);
            }
            Some(&cmd @ (0x20 | 0x22 | 0x24)) if p.len() > 5 => {
                let size = u32::from_be_bytes(p[1..5].try_into().unwrap()) as usize;
                let name = encode::read_string(&p[5..]).0;
                self.mode = Mode::Upload {
                    kind: cmd,
                    name,
                    file: Vec::with_capacity(size),
                    offset: 0,
                    want: 0,
                    got: 0,
                };
                self.next_request_with_size(size);
            }
            _ => {}
        }
    }

    fn next_block(&mut self, data: Vec<u8>, sent: usize) {
        let n = (data.len() - sent.min(data.len())).min(BLOCK);
        self.out.push_back((n as u32).to_be_bytes().to_vec());
        if n == 0 {
            self.mode = Mode::Idle;
            return;
        }
        for chunk in data[sent..sent + n].chunks(4096) {
            self.out.push_back(chunk.to_vec());
        }
        self.mode = Mode::Download { data, sent: sent + n };
    }

    fn next_request_with_size(&mut self, size: usize) {
        if let Mode::Upload { file, .. } = &mut self.mode {
            file.reserve(size);
        }
        self.upload_size = size;
        self.next_request();
    }

    fn next_request(&mut self) {
        let size = self.upload_size;
        let Mode::Upload {
            kind,
            name,
            file,
            offset,
            want,
            got,
        } = &mut self.mode
        else {
            return;
        };
        *offset += *want;
        let remaining = size.saturating_sub(*offset);
        if remaining == 0 {
            let k = match kind {
                0x20 => 0,
                0x22 => 1,
                _ => 2,
            };
            let base = name
                .rsplit_once('.')
                .map_or(name.as_str(), |(b, _)| b)
                .to_string();
            let bytes = std::mem::take(file);
            self.out.push_back(vec![0]);
            self.out.push_back(vec![0, 0, 0, [4, 2, 1][k]]);
            self.mode = Mode::Idle;
            let list = self.list(k);
            match list.iter().position(|e| e.0 == base) {
                Some(i) => list[i].1 = bytes,
                None => list.push((base, bytes)),
            }
            return;
        }
        *want = remaining.min(BLOCK);
        *got = 0;
        let mut req = (*offset as u32).to_be_bytes().to_vec();
        req.extend_from_slice(&(*want as u32).to_be_bytes());
        self.out.push_back(req);
    }
}

impl Transport for Simulator {
    fn write(&mut self, packet: &[u8]) -> io::Result<()> {
        let in_upload_data = matches!(&self.mode, Mode::Upload { want, got, .. } if got < want);
        if packet.first() == Some(&sysex::WRAP) && packet.len() >= 3 && !in_upload_data {
            self.pending.extend_from_slice(&packet[3..]);
            if self.pending.last() == Some(&0xF7) {
                let bytes = std::mem::take(&mut self.pending);
                // F0 47 5E 20 00 00 <section> <item> <data…> F7 (no checksum, as
                // with checksums disabled on the real sampler).
                if bytes.len() >= 9 {
                    let msg = Message::new(bytes[6], bytes[7], bytes[8..bytes.len() - 1].to_vec());
                    self.handle(msg);
                }
            }
            return Ok(());
        }
        self.handle_raw(packet);
        Ok(())
    }

    fn read(&mut self, _timeout: Duration) -> io::Result<Vec<u8>> {
        self.out
            .pop_front()
            .ok_or_else(|| io::Error::from(io::ErrorKind::TimedOut))
    }
}
