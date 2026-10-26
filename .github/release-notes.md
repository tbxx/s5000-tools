**Alpha.** An open replacement for Akai's ak.Sys for the S5000/S6000 over
USB, on macOS, Windows and Linux. It works for its author on an S5000 with
OS 2.14 on macOS; everything else is untested so far. Keep backups, and
please [report](https://github.com/tbxx/s5000-tools/issues) what works and
what doesn't.

**Downloads**

| System | App | Command line |
|---|---|---|
| macOS (Apple Silicon and Intel) | `.dmg` | `s5k-macos-universal.tar.gz` |
| Windows | `.msi` or `-setup.exe` | `s5k-windows-x86_64.zip` |
| Linux | `.AppImage`, `.deb` or `.rpm` | `s5k-linux-x86_64.tar.gz` |

Before first use, see
[Install](https://github.com/tbxx/s5000-tools#install). macOS needs one extra
click to open an app that isn't signed by Apple. Linux needs a udev rule.
Windows needs the WinUSB driver, which replaces Akai's driver until you
switch back.

Disk operations (load, save, delete on the sampler's disks) are
**experimental** and the app asks before the first one.
