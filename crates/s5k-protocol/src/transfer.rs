//! File transfers between memory and the host (docs/protocol.md §6), as
//! step-by-step state machines: feed them each USB read and send whatever
//! they return.

use crate::sysex::BUSY;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemKind {
    Sample,
    Program,
    Multi,
    MidiFile,
}

impl ItemKind {
    fn get_command(self) -> u8 {
        match self {
            ItemKind::Sample => 0x21,
            ItemKind::Program => 0x23,
            ItemKind::Multi => 0x25,
            ItemKind::MidiFile => 0x29,
        }
    }

    fn put_command(self) -> u8 {
        match self {
            ItemKind::Sample => 0x20,
            ItemKind::Program => 0x22,
            ItemKind::Multi => 0x24,
            ItemKind::MidiFile => 0x28,
        }
    }

    /// File extension the sampler expects in an upload's name.
    pub fn extension(self) -> &'static str {
        match self {
            ItemKind::Sample => "wav",
            ItemKind::Program => "akp",
            ItemKind::Multi => "akm",
            ItemKind::MidiFile => "mid",
        }
    }

    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_ascii_lowercase().as_str() {
            "wav" => Some(Self::Sample),
            "akp" => Some(Self::Program),
            "akm" => Some(Self::Multi),
            "mid" => Some(Self::MidiFile),
            _ => None,
        }
    }
}

fn index_bytes(index: u16) -> [u8; 2] {
    // Matches captures for indexes below 128; see docs/protocol.md §14.
    crate::encode::u14(index)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransferError {
    /// The sampler sent something the state machine did not expect.
    Unexpected(Vec<u8>),
    /// The sampler rejected the file (bad format or name).
    Rejected,
    /// The sampler asked for bytes beyond the end of the file.
    OutOfBounds { offset: u32, size: u32 },
}

impl std::fmt::Display for TransferError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransferError::Unexpected(b) => write!(f, "unexpected reply {b:02X?}"),
            TransferError::Rejected => write!(f, "the sampler rejected the file"),
            TransferError::OutOfBounds { offset, size } => {
                write!(
                    f,
                    "sampler asked for {size} bytes at {offset}, beyond the end of the file"
                )
            }
        }
    }
}

impl std::error::Error for TransferError {}

/// What to do after feeding a read.
#[derive(Debug, PartialEq, Eq)]
pub enum Step {
    /// Read again.
    Read,
    /// Write these packets, then read again.
    Send(Vec<Vec<u8>>),
    /// Finished.
    Done,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DownloadState {
    BlockSize,
    Data { remaining: u32 },
    Finished,
}

/// Getting an item from memory: the sampler sends a 4-byte block size, then
/// that many bytes; the host acknowledges each block with `00`. A block size
/// of 0 ends the transfer.
#[derive(Debug)]
pub struct Download {
    state: DownloadState,
    data: Vec<u8>,
}

impl Download {
    /// Returns the state machine and the request to send first.
    pub fn start(kind: ItemKind, index: u16) -> (Self, Vec<u8>) {
        let [hi, lo] = index_bytes(index);
        let request = vec![kind.get_command(), hi, lo];
        (
            Self {
                state: DownloadState::BlockSize,
                data: Vec::new(),
            },
            request,
        )
    }

    pub fn received(&self) -> usize {
        self.data.len()
    }

    pub fn feed(&mut self, packet: &[u8]) -> Result<Step, TransferError> {
        match self.state {
            DownloadState::Finished => Ok(Step::Done),
            DownloadState::BlockSize => {
                if packet == BUSY {
                    return Ok(Step::Read);
                }
                let Ok(size) = <[u8; 4]>::try_from(packet) else {
                    return Err(TransferError::Unexpected(packet.to_vec()));
                };
                let size = u32::from_be_bytes(size);
                if size == 0 {
                    self.state = DownloadState::Finished;
                    return Ok(Step::Done);
                }
                self.state = DownloadState::Data { remaining: size };
                Ok(Step::Read)
            }
            DownloadState::Data { remaining } => {
                let take = packet.len().min(remaining as usize);
                self.data.extend_from_slice(&packet[..take]);
                let remaining = remaining - take as u32;
                if remaining == 0 {
                    self.state = DownloadState::BlockSize;
                    Ok(Step::Send(vec![vec![0x00]]))
                } else {
                    self.state = DownloadState::Data { remaining };
                    Ok(Step::Read)
                }
            }
        }
    }

    pub fn into_data(self) -> Vec<u8> {
        self.data
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UploadState {
    Requests,
    /// Finished; draining the 4-byte notice that follows.
    Notice,
    Finished,
}

/// Sending an item to memory: the sampler asks for (offset, size) slices of
/// the file in any order; the host sends each slice followed by `00`. A lone
/// `00` means finished, `01` means the file was rejected.
#[derive(Debug)]
pub struct Upload<'a> {
    file: &'a [u8],
    state: UploadState,
    served: Vec<(u32, u32)>,
}

impl<'a> Upload<'a> {
    /// Returns the state machine and the request to send first. `name`
    /// should include the file extension.
    pub fn start(kind: ItemKind, name: &str, file: &'a [u8]) -> (Self, Vec<u8>) {
        let mut request = vec![kind.put_command()];
        request.extend_from_slice(&(file.len() as u32).to_be_bytes());
        request.extend(name.bytes().filter(|&b| b != 0));
        request.push(0);
        (
            Self {
                file,
                state: UploadState::Requests,
                served: Vec::new(),
            },
            request,
        )
    }

    /// Highest file offset sent so far, for progress display.
    pub fn progress(&self) -> u32 {
        self.served.iter().map(|&(o, s)| o + s).max().unwrap_or(0)
    }

    pub fn feed(&mut self, packet: &[u8]) -> Result<Step, TransferError> {
        match self.state {
            UploadState::Finished => Ok(Step::Done),
            UploadState::Notice => {
                // The trailing notice (item type in the last byte); see §6.
                self.state = UploadState::Finished;
                Ok(Step::Done)
            }
            UploadState::Requests => match packet {
                p if p == BUSY => Ok(Step::Read),
                [0x00] => {
                    self.state = UploadState::Notice;
                    Ok(Step::Read)
                }
                [0x01] => Err(TransferError::Rejected),
                p if p.len() == 8 => {
                    let offset = u32::from_be_bytes(p[0..4].try_into().unwrap());
                    let size = u32::from_be_bytes(p[4..8].try_into().unwrap());
                    let end = offset as usize + size as usize;
                    if end > self.file.len() {
                        return Err(TransferError::OutOfBounds { offset, size });
                    }
                    self.served.push((offset, size));
                    Ok(Step::Send(vec![
                        self.file[offset as usize..end].to_vec(),
                        vec![0x00],
                    ]))
                }
                p => Err(TransferError::Unexpected(p.to_vec())),
            },
        }
    }

    /// True once the notice after completion has been read. Callers that stop
    /// reading at the first `Done` should treat a missing notice as harmless.
    pub fn finished(&self) -> bool {
        self.state != UploadState::Requests
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn download_follows_the_captured_exchange() {
        let (mut d, request) = Download::start(ItemKind::Sample, 1);
        assert_eq!(request, vec![0x21, 0x00, 0x01]);
        assert_eq!(d.feed(b"AkaI").unwrap(), Step::Read);
        assert_eq!(d.feed(&[0, 0, 0, 6]).unwrap(), Step::Read);
        assert_eq!(d.feed(b"RIFF").unwrap(), Step::Read);
        assert_eq!(d.feed(b"..").unwrap(), Step::Send(vec![vec![0]]));
        assert_eq!(d.feed(&[0, 0, 0, 2]).unwrap(), Step::Read);
        assert_eq!(d.feed(b"!!").unwrap(), Step::Send(vec![vec![0]]));
        assert_eq!(d.feed(&[0, 0, 0, 0]).unwrap(), Step::Done);
        assert_eq!(d.into_data(), b"RIFF..!!");
    }

    #[test]
    fn upload_serves_requested_slices_in_any_order() {
        let file: Vec<u8> = (0..100u8).collect();
        let (mut u, request) = Upload::start(ItemKind::Multi, "test.akm", &file);
        assert_eq!(&request[..5], &[0x24, 0, 0, 0, 100]);
        assert_eq!(&request[5..], b"test.akm\0");

        let ask = |o: u32, s: u32| [o.to_be_bytes(), s.to_be_bytes()].concat();
        assert_eq!(
            u.feed(&ask(0, 100)).unwrap(),
            Step::Send(vec![file.clone(), vec![0]])
        );
        assert_eq!(
            u.feed(&ask(88, 12)).unwrap(),
            Step::Send(vec![file[88..].to_vec(), vec![0]])
        );
        assert_eq!(
            u.feed(&ask(12, 76)).unwrap(),
            Step::Send(vec![file[12..88].to_vec(), vec![0]])
        );
        assert_eq!(u.feed(&[0]).unwrap(), Step::Read);
        assert_eq!(u.feed(&[1, 0, 0, 1]).unwrap(), Step::Done);
        assert_eq!(u.progress(), 100);
    }

    #[test]
    fn upload_rejects_bad_requests() {
        let file = [0u8; 10];
        let (mut u, _) = Upload::start(ItemKind::Sample, "x.wav", &file);
        let ask = [0u32.to_be_bytes(), 20u32.to_be_bytes()].concat();
        assert!(matches!(u.feed(&ask), Err(TransferError::OutOfBounds { .. })));
        let (mut u, _) = Upload::start(ItemKind::Sample, "x.wav", &file);
        assert_eq!(u.feed(&[1]), Err(TransferError::Rejected));
    }
}
