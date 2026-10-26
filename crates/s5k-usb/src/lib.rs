//! USB transport for the Akai S5000/S6000, built on the pure-Rust `nusb`
//! crate. Blocking: run it on its own thread. See `docs/protocol.md` §1.

use std::fmt;
use std::io;
use std::time::Duration;

use nusb::transfer::{Buffer, Bulk, In, Out, TransferError};
use nusb::{Endpoint, Interface, MaybeFuture};
use s5k_protocol::transport::{ENDPOINT_IN, ENDPOINT_OUT, PACKET, PRODUCT_ID, VENDOR_ID};

/// The largest read: the sampler sends at most 4096 bytes per transfer.
const READ_SIZE: usize = 4096;

/// How long a single bulk write may take before it is cancelled.
const WRITE_TIMEOUT: Duration = Duration::from_secs(2);

/// Win32 `ERROR_ACCESS_DENIED`: another program has the device open.
#[cfg(target_os = "windows")]
const ERROR_ACCESS_DENIED: u32 = 5;

/// A sampler found on the bus.
#[derive(Debug, Clone)]
pub struct DeviceInfo {
    inner: nusb::DeviceInfo,
}

impl DeviceInfo {
    /// Identifier of the bus / host controller.
    pub fn bus(&self) -> &str {
        self.inner.bus_id()
    }

    /// Device address on its bus.
    pub fn address(&self) -> u8 {
        self.inner.device_address()
    }

    /// Serial number string, if the device reports one.
    pub fn serial(&self) -> Option<&str> {
        self.inner.serial_number()
    }

    /// Product string, if the device reports one.
    pub fn product(&self) -> Option<&str> {
        self.inner.product_string()
    }
}

impl fmt::Display for DeviceInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.product().unwrap_or("Akai S5000/S6000"))?;
        write!(f, " (bus {}, address {}", self.bus(), self.address())?;
        if let Some(serial) = self.serial() {
            write!(f, ", serial {serial}")?;
        }
        write!(f, ")")
    }
}

/// Why the sampler could not be found or opened.
#[derive(Debug)]
pub enum Error {
    /// No S5000/S6000 is connected.
    NotFound,
    /// Another program (a virtual machine, ak.Sys, …) holds the device.
    Busy(nusb::Error),
    /// The OS denied access to the device (Linux: missing udev rule).
    PermissionDenied(nusb::Error),
    /// Windows: the WinUSB driver is not installed for the device.
    Driver(nusb::Error),
    /// The device was unplugged.
    Disconnected(nusb::Error),
    /// Any other USB error.
    Usb(nusb::Error),
}

impl Error {
    fn from_nusb(e: nusb::Error) -> Self {
        use nusb::ErrorKind as K;
        match e.kind() {
            K::Busy => Error::Busy(e),
            K::Disconnected => Error::Disconnected(e),
            #[cfg(target_os = "windows")]
            K::Unsupported => Error::Driver(e),
            #[cfg(target_os = "windows")]
            K::PermissionDenied => Error::Busy(e),
            #[cfg(target_os = "windows")]
            K::Other if e.os_error() == Some(ERROR_ACCESS_DENIED) => Error::Busy(e),
            #[cfg(not(target_os = "windows"))]
            K::PermissionDenied => Error::PermissionDenied(e),
            _ => Error::Usb(e),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NotFound => write!(
                f,
                "no Akai S5000/S6000 found on USB (is it switched on and connected?)"
            ),
            Error::Busy(e) => write!(
                f,
                "the sampler is in use by another program, e.g. a virtual machine \
                 or ak.Sys; close it or release the device ({e})"
            ),
            Error::PermissionDenied(e) => write!(
                f,
                "permission denied opening the sampler ({e}); on Linux add a udev rule: \
                 SUBSYSTEM==\"usb\", ATTR{{idVendor}}==\"09e8\", ATTR{{idProduct}}==\"005e\", \
                 MODE=\"0666\""
            ),
            Error::Driver(e) => write!(
                f,
                "the sampler has no WinUSB driver ({e}); install WinUSB for it with Zadig"
            ),
            Error::Disconnected(e) => write!(f, "the sampler was disconnected ({e})"),
            Error::Usb(e) => write!(f, "USB error: {e}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::NotFound => None,
            Error::Busy(e)
            | Error::PermissionDenied(e)
            | Error::Driver(e)
            | Error::Disconnected(e)
            | Error::Usb(e) => Some(e),
        }
    }
}

impl From<nusb::Error> for Error {
    fn from(e: nusb::Error) -> Self {
        Error::from_nusb(e)
    }
}

/// Lists the S5000/S6000s connected.
pub fn list() -> Result<Vec<DeviceInfo>, Error> {
    Ok(nusb::list_devices()
        .wait()?
        .filter(|d| d.vendor_id() == VENDOR_ID && d.product_id() == PRODUCT_ID)
        .map(|inner| DeviceInfo { inner })
        .collect())
}

/// An open sampler: bulk OUT `02` and bulk IN `82` on interface 0.
pub struct UsbTransport {
    out: Endpoint<Bulk, Out>,
    inp: Endpoint<Bulk, In>,
    _interface: Interface,
}

impl UsbTransport {
    /// Opens the first sampler found.
    pub fn open_first() -> Result<Self, Error> {
        let info = list()?.into_iter().next().ok_or(Error::NotFound)?;
        Self::open(&info)
    }

    /// Opens a sampler and claims interface 0, detaching a kernel driver
    /// first on Linux.
    pub fn open(info: &DeviceInfo) -> Result<Self, Error> {
        let device = info.inner.open().wait()?;
        // The device class (DC) matches no OS driver, so macOS may leave it
        // unconfigured. Linux and Windows configure it themselves (WinUSB
        // cannot set a configuration at all).
        if device.active_configuration().is_err() {
            device.set_configuration(1).wait()?;
        }
        let interface = device.detach_and_claim_interface(0).wait()?;
        let mut out = interface.endpoint::<Bulk, Out>(ENDPOINT_OUT)?;
        let mut inp = interface.endpoint::<Bulk, In>(ENDPOINT_IN)?;
        // A program that quit mid-exchange can leave an endpoint stalled,
        // which blocks every later transfer until the host clears it.
        let _ = out.clear_halt().wait();
        let _ = inp.clear_halt().wait();
        Ok(Self {
            out,
            inp,
            _interface: interface,
        })
    }
}

fn transfer_error(e: TransferError) -> io::Error {
    match e {
        // Our own cancellation after a timeout.
        TransferError::Cancelled => io::Error::new(io::ErrorKind::TimedOut, e),
        _ => e.into(),
    }
}

impl s5k_protocol::Transport for UsbTransport {
    fn write(&mut self, packet: &[u8]) -> io::Result<()> {
        let completion = self.out.transfer_blocking(Buffer::from(packet), WRITE_TIMEOUT);
        completion.status.map_err(transfer_error)
    }

    /// Returns one bulk IN transfer of at most `max_len` bytes. On timeout
    /// the transfer is cancelled, so nothing stays queued to swallow the
    /// next reply.
    fn read(&mut self, max_len: usize, timeout: Duration) -> io::Result<Vec<u8>> {
        // macOS requires a multiple of the 64-byte packet size. A shorter
        // reply still ends the transfer, so rounding up is safe.
        let len = max_len.clamp(1, READ_SIZE).div_ceil(PACKET) * PACKET;
        self.inp.submit(Buffer::new(len));
        let Some(completion) = self.inp.wait_next_complete(timeout) else {
            self.inp.cancel_all();
            // Collect the cancelled transfer (or data that just arrived).
            while self.inp.pending() > 0 {
                if self.inp.wait_next_complete(Duration::from_millis(100)).is_none() {
                    break;
                }
            }
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "no data from the sampler",
            ));
        };
        completion.status.map_err(transfer_error)?;
        Ok(completion.buffer.into_vec())
    }
}
