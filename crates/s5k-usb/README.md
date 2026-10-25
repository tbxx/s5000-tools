# s5k-usb

USB transport for Akai S5000/S6000 samplers (IB-S56USB interface),
implementing `s5k_protocol::Transport` with the pure-Rust
[nusb](https://crates.io/crates/nusb) crate. No libusb needed.

```rust
let mut usb = s5k_usb::UsbTransport::open_first()?;
```

The API is blocking; run it on its own thread. Only one program can hold the
sampler at a time, so quit ak.Sys or detach the device from any virtual
machine first.

Try it with `cargo run -p s5k-usb --example probe`.

## Platform notes

**macOS**: nothing to install.

**Linux**: give your user access to the device with a udev rule, e.g.
`/etc/udev/rules.d/50-akai-s5000.rules`:

```
SUBSYSTEM=="usb", ATTR{idVendor}=="09e8", ATTR{idProduct}=="005e", MODE="0666"
```

or, to limit access to the logged-in user:

```
SUBSYSTEM=="usb", ATTR{idVendor}=="09e8", ATTR{idProduct}=="005e", TAG+="uaccess"
```

Then run `sudo udevadm control --reload-rules && sudo udevadm trigger` and
reconnect the sampler.

**Windows**: install the WinUSB driver for the sampler with
[Zadig](https://zadig.akeo.ie/) (Options → List All Devices, pick the Akai
device, select WinUSB, Install Driver). This replaces Akai's own driver, so
ak.Sys will not see the sampler until that driver is restored.
