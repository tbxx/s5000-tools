"""Extract S5000 front-panel screens and control events from a USBPcap capture.

Usage: python3 panel_frames.py <capture.pcap> <outdir> [--scale N]

Writes frame_NNNN.png for every distinct 320x240 screen and prints a timeline
of panel key/wheel messages, screen changes and the mode-LED byte.
"""
import os
import struct
import sys
import zlib

from usbpcap_dump import packets, parse

FRAME_BYTES = 9600  # 320 x 240, 1 bit per pixel
WIDTH = 320
SYSEX_HEAD = bytes.fromhex("F0475E200000")

KEYS = {
    0x40: "FX", 0x41: "Record", 0x42: "Edit Sample", 0x43: "Edit Program",
    0x44: "Multi", 0x45: "Utilities", 0x46: "Save", 0x47: "Load",
    0x64: "Cursor left", 0x65: "Cursor right", 0x67: "Window", 0x68: "Mark",
    0x69: "Jump", 0x6A: "Exit", 0x6B: "Ent/Play", 0x6C: "User 1",
    0x6D: "User 2", 0x6E: "User 3",
}
KEYS.update({0x48 + n: f"F{n + 1}" for n in range(16)})


def write_png(path, rows):
    height, width = len(rows), len(rows[0])
    raw = b"".join(b"\x00" + bytes(r) for r in rows)

    def chunk(kind, data):
        body = kind + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body))

    with open(path, "wb") as f:
        f.write(b"\x89PNG\r\n\x1a\n")
        f.write(chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 0, 0, 0, 0)))
        f.write(chunk(b"IDAT", zlib.compress(raw, 9)))
        f.write(chunk(b"IEND", b""))


def render(frame, scale):
    """1-bpp, rows of 40 bytes, least significant bit = leftmost pixel."""
    stride = WIDTH // 8
    rows = []
    for y in range(len(frame) // stride):
        px = []
        for byte in frame[y * stride:(y + 1) * stride]:
            for bit in range(8):
                px.extend([0 if byte >> bit & 1 else 235] * scale)
        rows.extend([px] * scale)
    return rows


def describe_control(sysex):
    kind, *rest = sysex
    if kind in (1, 2) and rest:
        name = KEYS.get(rest[0], f"key {rest[0]:#04x}")
        return f"{'press  ' if kind == 1 else 'release'} {name}"
    if kind == 3 and len(rest) >= 2:
        return f"wheel {'fwd ' if rest[0] == 0 else 'back'} x{rest[1]}"
    return "control " + sysex.hex(" ")


def main():
    args = sys.argv[1:]
    if len(args) < 2:
        sys.exit(__doc__)
    capture, outdir = args[0], args[1]
    scale = int(args[args.index("--scale") + 1]) if "--scale" in args else 2
    os.makedirs(outdir, exist_ok=True)

    buf, collecting, last, t0, count = b"", False, None, None, 0
    for ts, pkt in packets(capture):
        hdr, data = parse(pkt)
        t0 = ts if t0 is None else t0
        t = ts - t0
        if not data:
            continue
        if hdr["dir"] == "OUT" and data == b"\x01":
            collecting, buf = True, b""
            continue
        if collecting and hdr["dir"] == "IN ":
            if len(buf) < FRAME_BYTES:
                buf += data
                continue
            collecting = False
            leds = data[0]
            frame = buf[:FRAME_BYTES]
            if frame != last:
                name = f"frame_{count:04d}.png"
                write_png(os.path.join(outdir, name), render(frame, scale))
                print(f"{t:8.3f}  screen {count:4d}  LEDs {leds:08b}  -> {name}")
                count += 1
                last = frame
            continue
        # Panel controls are SysEx section 0x20 inside the 10 <len16> wrapper.
        if hdr["dir"] == "OUT" and data[3:9] == SYSEX_HEAD and data[9] == 0x20:
            body = data[10:data.index(0xF7) - 1]  # drop the checksum byte
            print(f"{t:8.3f}  {describe_control(body)}")


if __name__ == "__main__":
    main()
