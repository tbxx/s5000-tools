"""Rank unique .akp programs under a folder by complexity.

Usage: python3 -I rank_akp.py <root> [--zone-dump]
"""
import hashlib
import os
import sys


def chunks(b):
    """Yield (id, payload) for the top-level chunks after the APRG header."""
    i = 12
    while i + 8 <= len(b):
        cid = b[i:i + 4]
        ln = int.from_bytes(b[i + 4:i + 8], "little")
        yield cid, b[i + 8:i + 8 + ln]
        i += 8 + ln


def zone_names(kgrp):
    """Sample names from the zone sub-chunks of one keygroup."""
    names = []
    i = 0
    while i + 8 <= len(kgrp):
        cid = kgrp[i:i + 4]
        ln = int.from_bytes(kgrp[i + 4:i + 8], "little")
        if cid == b"zone":
            z = kgrp[i + 8:i + 8 + ln]
            nl = z[1]
            n = z[2:2 + nl].decode("latin1").strip("\0 ")
            if n:
                names.append(n)
        i += 8 + ln
    return names


def main():
    root = sys.argv[1]
    seen, rows = set(), []
    for d, _, files in os.walk(root):
        wavs = {os.path.splitext(f)[0].lower() for f in files
                if f.lower().endswith(".wav")}
        for f in files:
            if not f.lower().endswith(".akp"):
                continue
            p = os.path.join(d, f)
            b = open(p, "rb").read()
            h = hashlib.md5(b).hexdigest()
            if h in seen or b[:4] != b"RIFF" or b[8:12] != b"APRG":
                continue
            seen.add(h)
            kgs = [c for cid, c in chunks(b) if cid == b"kgrp"]
            if "--zone-dump" in sys.argv and kgs:
                print(p, kgs[0][:0x60].hex(" "))
                return
            names = {n for k in kgs for n in zone_names(k)}
            have = sum(1 for n in names if n.lower() in wavs)
            rows.append((len(kgs), len(names), have, len(b), p))
    print("unique programs:", len(rows))
    rows.sort(reverse=True)
    print(f"{'kgrps':>5} {'samples':>7} {'wavs here':>9} {'bytes':>7}  path")
    for r in rows[:30]:
        print(f"{r[0]:5d} {r[1]:7d} {r[2]:9d} {r[3]:7d}  {os.path.relpath(r[4], root)}")


main()
