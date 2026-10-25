# Research tools

Python 3 scripts used to work out the protocol and file formats. They need no
packages beyond the standard library.

| Script | Purpose |
|---|---|
| `usbpcap_dump.py` | Print the USB traffic in a USBPcap capture; `--quiet` hides polling |
| `panel_frames.py` | Render front-panel screens from a capture to PNG, with a timeline of key presses |
| `rank_akp.py` | List the unique `.akp` programs under a folder by keygroup and sample count |

See [docs/capturing.md](../docs/capturing.md).
