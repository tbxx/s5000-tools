//! Finds a sampler, starts a session and asks for its product name.
//!
//! `cargo run -p s5k-usb --example probe`

use std::io::ErrorKind;
use std::time::{Duration, Instant};

use s5k_protocol::transport::HANDSHAKE;
use s5k_protocol::{Assembler, Message, Reply, Transport};
use s5k_usb::UsbTransport;

fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let devices = s5k_usb::list()?;
    for info in &devices {
        println!("found {info}");
    }
    let Some(info) = devices.first() else {
        return Err(s5k_usb::Error::NotFound.into());
    };
    let mut usb = UsbTransport::open(info)?;

    for packet in HANDSHAKE {
        usb.write(packet)?;
    }
    for packet in Message::new(0x02, 0x03, []).usb_packets() {
        println!("> {}", hex(&packet));
        usb.write(&packet)?;
    }

    let deadline = Instant::now() + Duration::from_secs(2);
    let mut assembler = Assembler::new();
    let reply = loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err("no reply within 2 s".into());
        }
        let packet = match usb.read(left) {
            Ok(packet) => packet,
            Err(e) if e.kind() == ErrorKind::TimedOut => continue,
            Err(e) => return Err(e.into()),
        };
        println!("< {}", hex(&packet));
        if let Some(message) = assembler.push(&packet) {
            break message;
        }
    };

    let reply = Reply::parse(&reply)?;
    let name: String = reply
        .data
        .iter()
        .take_while(|&&b| b != 0)
        .map(|&b| b as char)
        .collect();
    println!("product: {name}");
    Ok(())
}
