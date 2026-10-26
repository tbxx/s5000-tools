//! The byte pipe to the sampler: USB bulk endpoints 02 (out) and 82 (in).

use std::time::Duration;

/// One bulk write or read at a time. Implemented by `s5k-usb` for real
/// hardware and by test doubles.
pub trait Transport {
    /// Writes one packet to the sampler.
    fn write(&mut self, packet: &[u8]) -> std::io::Result<()>;

    /// Reads one bulk transfer of at most `max_len` bytes. Returns
    /// `ErrorKind::TimedOut` if nothing arrives in time; a timed-out read
    /// must not leave a transfer pending.
    ///
    /// Callers pass the exact length they expect next. The sampler sends no
    /// zero-length packet after a reply that is a multiple of 64 bytes, so a
    /// read larger than the reply would not end there and would run on into
    /// the next reply.
    fn read(&mut self, max_len: usize, timeout: Duration) -> std::io::Result<Vec<u8>>;
}

/// Read length for SysEx replies: one USB packet at a time.
pub const PACKET: usize = 64;
/// Largest transfer the sampler sends in one piece.
pub const MAX_TRANSFER: usize = 4096;

impl<T: Transport + ?Sized> Transport for Box<T> {
    fn write(&mut self, packet: &[u8]) -> std::io::Result<()> {
        (**self).write(packet)
    }

    fn read(&mut self, max_len: usize, timeout: Duration) -> std::io::Result<Vec<u8>> {
        (**self).read(max_len, timeout)
    }
}

/// Raw writes that start a session (captured from ak.Sys).
pub const HANDSHAKE: [&[u8]; 2] = [&[0x03, 0x14], &[0x04, 0x03]];

/// Raw keep-alive request.
pub const KEEP_ALIVE: u8 = 0x30;

/// USB identity of the S5000/S6000 interface.
pub const VENDOR_ID: u16 = 0x09E8;
pub const PRODUCT_ID: u16 = 0x005E;
pub const ENDPOINT_OUT: u8 = 0x02;
pub const ENDPOINT_IN: u8 = 0x82;
