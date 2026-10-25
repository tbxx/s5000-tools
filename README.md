# s5000-tools

Open tools for the **Akai S5000/S6000** sampler over USB: transfer samples,
programs and multis, edit them, view the front panel and meters. They
replace Akai's Windows-only ak.Sys on macOS, Linux and Windows.

**Status:** the USB protocol and file formats are documented. The tools are
being built.

- [docs/protocol.md](docs/protocol.md): the USB protocol
- [docs/file-formats.md](docs/file-formats.md): `.akp`, `.akm` and `.wav` layouts
- [docs/effects.md](docs/effects.md): the EB20 effects board
- [docs/capturing.md](docs/capturing.md): how the protocol was captured
- [tools/](tools/): Python research scripts

## Planned

- `s5k`: a command-line tool to list memory and get/put samples, programs
  and multis (including a program with its samples).
- An editor app with file transfers, program/multi/effects editing, front
  panel and meters.

Both are written in Rust, sharing one core library, and the app uses Tauri
with Svelte.

## Requirements

An S5000/S6000 with OS 2.x and the USB interface board.

- **macOS:** no driver needed.
- **Linux:** a udev rule granting access to USB device `09e8:005e`.
- **Windows:** the WinUSB driver for the sampler, installed with
  [Zadig](https://zadig.akeo.ie). This replaces Akai's driver.

## Credits

The file-transfer framing was first documented by
[aksy](https://github.com/watzo/aksy) (Walco van Loon). Everything else was
worked out from captures of ak.Sys traffic.

Akai, S5000, S6000 and ak.Sys are trademarks of their owners. This project is
not affiliated with or endorsed by Akai Professional.

## License

[MIT](LICENSE)
