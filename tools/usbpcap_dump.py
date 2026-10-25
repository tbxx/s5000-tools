"""Dump S5000 USB traffic from a USBPcap .pcap capture.

Usage: python3 usbpcap_dump.py <capture.pcap> [--all] [--quiet] [--max N]

  --all     include packets with no payload (URB submissions/completions)
  --quiet   hide keep-alive polling and collapse runs of bulk data packets
  --max N   bytes of payload to print per packet (default 48)
"""
import struct
import sys

XFER = {0: "ISO", 1: "INT", 2: "CTRL", 3: "BULK"}
LINKTYPE_USBPCAP = 249

# Requests ak.Sys sends continuously, and the S5000's routine answers.
POLL_OUT = {
    bytes([0x30]),  # keep-alive
    bytes([0x50]),  # level meters
    bytes.fromhex("100A00F0475E200000023032F7"),  # wave memory free
    bytes.fromhex("100A00F0475E200000023133F7"),  # CPU memory free
}
POLL_IN_PREFIXES = (
    bytes(4),
    bytes.fromhex("504083"),
    bytes.fromhex("F0475E200000520230"),
    bytes.fromhex("F0475E200000520231"),
)


def packets(path):
    """Yield (timestamp, raw packet) from a little-endian pcap file."""
    with open(path, "rb") as f:
        magic, _, _, _, _, _, linktype = struct.unpack("<IHHiIII", f.read(24))
        if magic != 0xA1B2C3D4:
            raise ValueError(f"not a little-endian pcap file: magic {magic:#x}")
        if linktype != LINKTYPE_USBPCAP:
            raise ValueError(f"not a USBPcap capture: linktype {linktype}")
        while True:
            header = f.read(16)
            if len(header) < 16:
                return
            ts_s, ts_us, incl, _orig = struct.unpack("<IIII", header)
            yield ts_s + ts_us / 1e6, f.read(incl)


def parse(pkt):
    """Split a USBPcap packet into its header fields and payload."""
    (hlen, irp, status, func, info, bus, dev, ep, xfer, _dlen) = struct.unpack(
        "<HQIHBHHBBI", pkt[:27])
    hdr = {"irp": irp, "status": status, "func": func,
           "dir": "IN " if info & 1 else "OUT", "bus": bus, "dev": dev,
           "ep": ep, "xfer": XFER.get(xfer, xfer)}
    if xfer == 2 and hlen >= 28:
        hdr["stage"] = pkt[27]
    return hdr, pkt[hlen:]


def fmt(data, limit):
    shown = data[:limit]
    hx = " ".join(f"{b:02X}" for b in shown)
    asc = "".join(chr(b) if 32 <= b < 127 else "." for b in shown)
    more = f" …(+{len(data) - limit})" if len(data) > limit else ""
    return f"{hx}{more}  |{asc}|"


def is_poll(data):
    return data in POLL_OUT or (len(data) <= 32 and data.startswith(POLL_IN_PREFIXES))


def main():
    args = sys.argv[1:]
    if not args:
        sys.exit(__doc__)
    path = args[0]
    show_all = "--all" in args
    quiet = "--quiet" in args
    limit = int(args[args.index("--max") + 1]) if "--max" in args else 48

    t0 = None
    run = None  # [direction, count, bytes, first packet number, start time]

    def flush():
        nonlocal run
        if run:
            print(f"{run[3]:6d} {run[4]:9.4f}   … {run[1]} {run[0]} data packets, "
                  f"{run[2]} bytes")
        run = None

    for n, (ts, pkt) in enumerate(packets(path)):
        hdr, data = parse(pkt)
        t0 = ts if t0 is None else t0
        t = ts - t0
        if not show_all and not data:
            continue
        if quiet and is_poll(data):
            continue
        if quiet and len(data) > 64:
            if run and run[0] == hdr["dir"]:
                run[1] += 1
                run[2] += len(data)
            else:
                flush()
                run = [hdr["dir"], 1, len(data), n, t]
            continue
        flush()
        stage = f" stage={hdr['stage']}" if "stage" in hdr else ""
        print(f"{n:6d} {t:9.4f} dev{hdr['dev']} ep{hdr['ep']:02X} "
              f"{hdr['xfer']:4} {hdr['dir']} st={hdr['status']:X}{stage} "
              f"len={len(data):5d}  {fmt(data, limit)}")
    flush()


if __name__ == "__main__":
    main()
