//! The byte pipe to the sampler: USB bulk endpoints 02 (out) and 82 (in).

use std::time::Duration;

/// One bulk write or read at a time. Implemented by `s5k-usb` for real
/// hardware and by test doubles.
pub trait Transport {
    /// Writes one packet to the sampler.
    fn write(&mut self, packet: &[u8]) -> std::io::Result<()>;

    /// Reads one packet (up to 4096 bytes). Returns `ErrorKind::TimedOut` if
    /// nothing arrives in time.
    fn read(&mut self, timeout: Duration) -> std::io::Result<Vec<u8>>;
}

impl<T: Transport + ?Sized> Transport for Box<T> {
    fn write(&mut self, packet: &[u8]) -> std::io::Result<()> {
        (**self).write(packet)
    }

    fn read(&mut self, timeout: Duration) -> std::io::Result<Vec<u8>> {
        (**self).read(timeout)
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
