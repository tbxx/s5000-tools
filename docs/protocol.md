# Akai S5000/S6000 USB protocol

This is how the Akai **ak.Sys** editor talks to an S5000/S6000 over the
IB-S56USB interface (sampler OS 2.x). It was worked out by capturing ak.Sys
traffic with USBPcap and checking the results against files saved by the
sampler. aksy (see [Credits](../README.md#credits)) documents the transfer
framing.

The SysEx command set is defined in Akai's **S5000/S6000 MIDI System
Exclusive Protocol Specification, version 2.10** (cited below as "the spec",
with table numbers). It is not reproduced here. Where this document and the
spec cover the same ground they agree, except where noted. The spec does not
cover the USB layer, file transfers, the screen image or the meters; those
come from captures.

Notation: bytes are hexadecimal. `<...>` is a field. "Index" is an item's
position in sampler memory, starting at 0, sent as a 14-bit value
(`hi lo`, 7 bits each).

Contents:

1. [USB device](#1-usb-device)
2. [Message framing](#2-message-framing)
3. [Session start-up and polling](#3-session-start-up-and-polling)
4. [Value encodings](#4-value-encodings)
5. [System and memory commands](#5-system-and-memory-commands)
6. [File transfers](#6-file-transfers)
7. [Program parameters](#7-program-parameters)
8. [Multi parameters](#8-multi-parameters)
9. [Sample commands](#9-sample-commands)
10. [Effects](#10-effects)
11. [Front panel](#11-front-panel)
12. [Level meters](#12-level-meters)
13. [Disk tools](#13-disk-tools)
14. [Open questions](#14-open-questions)

---

## 1. USB device

| Field | Value |
|---|---|
| Vendor / product | `09E8` (Akai) / `005E` (S5000/S6000) |
| Speed | USB 1.1 full speed |
| Device class | `DC`; one interface, class `FF` (vendor-specific) |
| Endpoints | bulk OUT `02` and bulk IN `82` (64-byte packets) carry everything; bulk `01`/`81` (16-byte) are never used by ak.Sys |

No class driver claims the device, so a user-space program can open it
directly. On macOS this works as is. On Linux it needs a udev rule for access.
On Windows the WinUSB driver has to be installed for the device (e.g. with
Zadig).

Only one program can hold the device at a time.

## 2. Message framing

### Commands (host → sampler, endpoint 02)

Most commands are System Exclusive messages wrapped in a 3-byte header:

```
10 <len lo> <len hi>   F0 47 5E 20 00 00 <section> <item> <data...> [<checksum>] F7
```

- `len` is the byte count of the SysEx part, little-endian.
- `47` is Akai, `5E` the S5000/S6000 device ID, and `20` means a user
  reference of 2 bytes (`00 00`) follows.
- `checksum` = (section + item + all data bytes) & 0x7F, optional. **Checksums
  are off by default, and the sampler then treats a checksum byte as data.**
  Fixed-length commands ignore the extra byte, but variable-length ones (the
  blocked keygroup/zone sets in §7) misparse it: a set is applied and then
  reported as an error, or answered with an unrelated reply. ak.Sys sends
  blocked sets without a checksum. s5000-tools sends none (verified on an
  S5000, OS 2.14); enable checksums with `00 04 01` before sending them.
- **Long messages are split.** Each USB write carries at most 61 bytes of
  SysEx after its own `10 <len16>` header. A message is cut into consecutive
  pieces; only the first starts with `F0` and only the last ends with `F7`.

A few commands are raw bytes, sent without the SysEx wrapper: `01` (screen),
`30` (keep-alive), `50` (meters), and the transfer commands `20`–`29` / `40`–`41`.

### Replies (sampler → host, endpoint 82)

```
F0 47 5E 20 00 00 <status> <section> <item> <data...> F7
```

| Status | Meaning |
|---|---|
| `52` "R" | reply with data |
| `44` "D" | done (a set or action succeeded) |
| `45` "E" | error: two bytes `lo hi` (7-bit) form the error code |
| `4F` "O" | OK / acknowledged; ak.Sys turns these off at start-up |

Replies carry **no** checksum.

The 4-byte reply `41 6B 61 49` ("AkaI") means **busy**: read again.

Error codes (spec table 3), with code = `hi` × 128 + `lo`:

| Code | Meaning | Code | Meaning |
|---|---|---|---|
| `00` | not supported | `106` | disk: unable to delete |
| `01` | invalid message format | `107` | disk: unknown error |
| `02` | parameter out of range | `108` | disk: error during save |
| `03` | unknown error | `109` | disk: insufficient space |
| `04` | item not found | `10A` | disk: write-protected |
| `05` | new item could not be created | `10B` | disk: name not unique |
| `06` | item could not be deleted | `10C` | disk: invalid disk handle |
| `81` | checksum invalid | `10D` | disk: disk is empty |
| `101` | disk: selected disk invalid | `10E` | disk: operation aborted |
| `102` | disk: error during load | `10F` | disk: failed on open |
| `103` | disk: item not found | `110` | disk: read error |
| `104` | disk: unable to create | `111` | disk: not ready |
| `105` | disk: folder not empty | `112` | disk: SCSI error |
| | | `181` | keygroup does not exist |

## 3. Session start-up and polling

After claiming interface 0, ak.Sys sends two raw writes:

```
03 14
04 03
```

and then configures the sampler:

| Command | Purpose |
|---|---|
| `00 01 00` | turn off "OK" acknowledgements |
| `00 03 00` | turn off item sync (front panel follows remote edits) |
| `02 03` | product name, e.g. "AKAI S5000" |
| `02 00` | OS version: major, minor (e.g. `02 0E`) |
| `02 01`, `02 04`, `02 21` | unknown status values |
| `00 07 01` | turn on heartbeat messages |

ak.Sys then polls continuously:

| Request | Every | Reply |
|---|---|---|
| raw `30` | ~200 ms | 4 bytes. `00 00 00 00` normally; `01 00 00 00` followed by a 128-byte block once multis exist (meaning unknown) |
| `02 30` / `02 31` | ~200 ms | wave / CPU memory free, in % (as shown on the LCD) |
| raw `50` | ~60 ms, only with the meter window open | level meters, see [§12](#12-level-meters) |
| raw `01` | ~150 ms, only with the panel window open | screen image, see [§11](#11-front-panel) |
| `0C 42`, `02 20`, `0A 12` | ~4 s | current multi index, current mode (?), current program index |

ak.Sys wraps groups of queries in `00 03 01` … `00 03 00` (item sync on/off).

## 4. Value encodings

Several encodings are in use; each parameter table states which one applies.

| Name | Bytes | Meaning |
|---|---|---|
| byte | 1 | 0–127 |
| signed | 2 | sign (`00` = +, `01` = −), then magnitude 0–127 |
| offset | 1 | value + constant, e.g. pan + 64; constants are given per parameter |
| 14-bit | 2 | `hi lo`, 7 bits each: (hi << 7) \| lo ("compound word") |
| signed 14-bit | 3 | sign, hi, lo |
| 28-bit | 4 | four 7-bit groups, most significant first ("compound double word") |
| 56-bit | 8 | eight 7-bit groups, most significant first ("compound quad word") |
| string | n | ASCII, terminated by `00` |

All SysEx data bytes are 7-bit. Memory indexes are 14-bit. The raw transfer
commands (§6) also carry 2-byte indexes. With fewer than 128 items both
readings agree; with more, the encoding needs checking against the sampler.

## 5. System and memory commands

Sections: `0A` program, `0C` multi, `0E` sample, `10` disk, `12` effects,
`14` song, `16` MIDI file. Most item-level commands act on the **current**
item, chosen with `xx 06 <index>`.

### Listing memory

| Command | Reply |
|---|---|
| `0E 10` | number of samples (14-bit) |
| `0E 11 <index>` | sample name |
| `0A 10` | number of programs |
| `0A 13` | current program's name |
| `0C 40` | number of multis |
| `16 10`, `16 20`, `14 10` | MIDI file / song counts |
| `10 04` / `10 05` | number of disks / disk list (handle, type, name) |

To list programs, ak.Sys selects each one in turn (`0A 06 <index>`) and reads
`0A 13`. It reads multi names with `2C <index> 43`, see [§8](#8-multi-parameters).

### Creating, renaming, deleting

| Command | Action |
|---|---|
| `0A 06 <index>` / `0C 06 <index>` / `0E 06 <index>` | make a program / multi / sample current |
| `0A 03 <keygroups> <name>` | create a program with n keygroups |
| `0C 01 <size>` then `0C 02 <name>` | create a multi (size 0 = 32 parts, 1 = 64, 2 = 128) and name it |
| `0A 09 <name>` / `0E 09 <name>` | rename current program / sample |
| `0C 30 <name>` | rename current multi |
| `0A 08` / `0C 08` / `0E 08` | delete current program / multi / sample |
| `02 32` | **delete everything in memory** |

aksy also lists `0A 02 <name>` (create an empty program) and `0A 07` / `0E 07`
(delete all programs / samples); ak.Sys was not seen using them.

## 6. File transfers

Transfers use raw commands without the SysEx wrapper. Files are exchanged in
the sampler's own file formats (`.wav`, `.akp`, `.akm`; see
[file-formats.md](file-formats.md)).

### Getting an item from memory

```
host:    <cmd> <index hi> <index lo>       21 sample, 23 program, 25 multi, 29 MIDI file
sampler: <block size, 4 bytes BE>
sampler: <block data>                      in packets of up to 4096 bytes
host:    00                                acknowledge
         ... repeat ...
sampler: 00 00 00 00                       block size 0 = finished
```

The data is the complete file, starting with its RIFF header.

### Sending an item to memory

```
host:    <cmd> <file size, 4 bytes BE> <file name> 00
                                           20 sample, 22 program, 24 multi,
                                           26 scene, 28 MIDI file
sampler: <offset, 4 bytes BE> <size, 4 bytes BE>
host:    file bytes [offset, offset + size)
host:    00
         ... repeat ...
sampler: 00                                finished
sampler: <4 bytes>                         notice; last byte = item type
                                           (01 multi, 02 program, 04 sample)
```

**The sampler reads the file non-sequentially.** For a `.wav` it reads the
first block, then asks to restart at the start of the audio data. For `.akp`
and `.akm` files it reads the whole file, then individual chunks again. The
host must serve whatever (offset, size) is requested. Block sizes seen: 16384
for audio, the remaining length at the end.

Sending a program sends only the `.akp`. Its samples must already be in memory
or be sent separately; zones refer to samples by name.

To get a multi with everything it uses, ak.Sys reads each part's program name
(`2C <multi> 45 <part>`) and each zone's sample name (`38 <program> 01 21
<keygroup> 00 00`), then gets those programs and samples by index.

### Disk transfers

`41 <file name> 00` (get from disk) and `40 <size> <file name> 00` (send to
disk) act on the current disk folder; see [§13](#13-disk-tools). Not verified
on hardware.

## 7. Program parameters

Programs, keygroups and zones are addressed by program index. The item
numbers are those of the spec's Program (table 13), Keygroup (table 11) and
Keygroup Zone (table 9) sections; the spec calls these addressed forms
"alternative operations" (table 32). There are two command forms:

**Single parameter** (one value per message, used for edits):

```
2A <prog hi> <prog lo> <item> [<n>] <value>       program level
3A <prog hi> <prog lo> <keygroup> <item> <data, 3 bytes>   keygroup level
38 <prog hi> <prog lo> <keygroup> <item> <zone> <data>    zone level
```

Keygroups and zones are numbered from 1. `<n>` selects which of several
(e.g. LFO 1 or 2, mod source 1–3). Keygroup data is always 3 bytes: the value,
padded with `00`.

**Batched read** (many values per message, used when opening an editor):

```
3C <prog hi> <prog lo> (<item> <n> 00 00)...                program level
3A <prog hi> <prog lo> <keygroup> (<item> <n> 00 00)...     keygroup level
38 <prog hi> <prog lo> <keygroup> (<item> <zone> 00 00)...  zone level
```

The reply's data holds the values in request order, each in its own encoding
(signed values take 2 bytes). Batched reads use the **get** item numbers,
which for the program and keygroup sections are generally the set number + 8.

Edit All (apply to every keygroup) is an ak.Sys feature; it sends one message
per keygroup.

### Program level (`2A`)

| Control | Item | n | Encoding / values |
|---|---|---|---|
| Loudness | `20` | | byte 0–100 |
| Velocity sensitivity | `21` | | signed −100…+100 |
| Amp mod amount | `23` | 1 KBD, 2 AFT | signed |
| Pan mod amount | `25` | 1 LFO2, 2 KBD, 3 Wheel | signed |
| Semitone tune | `30` | | signed −36…+36 |
| Fine tune | `31` | | signed −50…+50 |
| Pitch bend up / down | `40` / `41` | | byte 0–24 |
| Bend mode | `42` | | 0 normal, 1 held |
| Aftertouch pitch | `43` | | signed −24…+24 |
| Mono legato | `44` | | 0/1 |
| Portamento on | `45` | | 0/1 |
| Portamento mode | `46` | | 0 time, 1 rate |
| Portamento time | `47` | | byte 0–100 |
| LFO rate / delay / depth | `50` / `51` / `52` | 1, 2 | byte 0–100 |
| LFO waveform | `53` | 1, 2 | 0 sine, 1 triangle, 2 square, 3 square up, 4 square down, 5 saw, 6 saw up, 7 saw down, 8 random |
| LFO1 key sync | `54` | 1 | 0/1 |
| LFO2 retrigger | `55` | 2 | 0/1 |
| LFO rate / delay / depth mod source | `56` / `58` / `5A` | 1, 2 | mod source |
| LFO1 wheel amount | `5C` | 1 | byte |
| LFO2 MIDI clock division | `5F` | 2 | 0–68: 8, 6, 4, 3, 2 cycles/beat, 1 cycle/beat, then 2…64 beats/cycle |
| KG pitch mod source | `70` | 1, 2 | mod source |
| KG amp mod source | `71` | 1 | mod source |
| Filter mod source | `72` | 1–3 | mod source |
| Keygroup crossfade | `0D` | | 0/1 |
| Add keygroups | `0B` | | number to add |
| Delete keygroup | `0C` | | keygroup number, from 0 |
| Number of keygroups (read) | `14` | | byte |

**Mod sources:** 0 NONE, 1 WHEEL, 2 BEND, 3 AFT, 4 EXT, 5 VEL, 6 KBD, 7 LFO1,
8 LFO2, 9 A EG, 10 F EG, 11 AX EG, 12 !WHEEL, 13 !BEND, 14 !EXT.

### Keygroup level (`3A`)

| Control | Item | Encoding / values |
|---|---|---|
| Low / high key | `04` / `05` | MIDI note 21–127; ak.Sys always sends both |
| Mute group | `06` | 0 off, then group number |
| FX override | `07` | 0 off, 1 FX1, 2 FX2, 3 RV3, 4 RV4 |
| FX send | `08` | byte |
| Velocity crossfade | `09` | 0/1 |
| Semitone / fine | `10` / `11` | signed |
| Level | `12` | probably; signed, −30…+30 dB |
| KG pitch / amp mod amounts | `13 <n>` / `14 <n>` | signed |
| Filter type | `20` | 0–25, see below |
| Cutoff / resonance | `21` / `22` | byte |
| Filter keyboard tracking | `23` | signed −36…+36 |
| Filter mod amount | `24 <n>` | n 1–3, signed |
| Filter attenuation | `25` | 0 0 dB, 1 6 dB … 5 30 dB |
| Filter envelope | `30`–`38` | see envelopes |
| Filter envelope depth | `37` | signed |
| Amp envelope | `50`–`57` | see envelopes |
| Aux envelope | `60`–`64` | see envelopes |

**Filter types:** 0 2-POLE LP, 1 4-POLE LP, 2 2-POLE LP+, 3 2-POLE BP,
4 4-POLE BP, 5 2-POLE BP+, 6 1-POLE HP, 7 2-POLE HP, 8 1-POLE HP+, 9 LO<>HI,
10 LO<>BAND, 11 BAND<>HI, 12 NOTCH 1, 13 NOTCH 2, 14 NOTCH 3, 15 WIDE NOTCH,
16 BI-NOTCH, 17 PEAK 1, 18 PEAK 2, 19 PEAK 3, 20 WIDE PEAK, 21 BI-PEAK,
22 PHASER 1, 23 PHASER 2, 24 BI-PHASE, 25 VOWELISER.

**Envelopes:**

| Control | Amp | Filter | Aux |
|---|---|---|---|
| Attack | `50` | `30` | rate n: `60 <n>` (n 1–4) |
| V>Attack | `51` | `31` | V>R1 / V>R4: `61 1` / `61 4` |
| Decay | `52` | `32` | level n: `63 <n>` |
| Sustain | `53` | `33` | |
| Release | `54` | `34` | |
| V>Release | `55` | `35` | |
| Keyscale | `56` | `36` | `62` |
| Off>Release | `57` | `38` | `64 4` |

Envelope templates are an ak.Sys feature: choosing one sends the four shape
values. Opening a program in ak.Sys re-sends the aux envelope shape.

### Zone level (`38`)

| Control | Item | Encoding |
|---|---|---|
| Sample | `01` | sample name (string) |
| Level | `02` | signed |
| Pan | `03` | offset: pan + 64 (L50…R50) |
| Output | `04` | byte |
| Filter | `05` | signed |
| Fine / semitone | `06` / `07` | signed |
| Keyboard tracking | `08` | 0/1 |
| Playback | `09` | byte (0 = as sample) |
| Velocity > start | `0A` | signed 14-bit, −9999…+9999 |
| Solo/mute zone | `0E` | 0/1 |

## 8. Multi parameters

Multis are addressed by index: `2C <multi hi> <multi lo> <item> ...`.

| Read | Reply |
|---|---|
| `2C <m> 43` | name |
| `2C <m> 44` | number of parts − 1 (`1F` = 32) |
| `2C <m> 45 <part>` | program name assigned to the part |
| `2C <m> 47 <part>` | 11-byte part summary: MIDI channel, mute, level, ?, pan (+64), ?, ?, fine (+50), semi (+36), low key, high key |

Part settings, with parts numbered from 0:

```
2C <multi hi> <multi lo> <item> <part> <value>
```

| Control | Item | Encoding |
|---|---|---|
| MIDI channel | `10` | 0 = 1A … 15 = 16A, 16 = 1B … |
| Mute / solo | `11` / `12` | 0/1 |
| Level | `13` | byte 0–100 |
| Output | `14` | byte |
| Pan | `15` | offset +64 |
| FX channel | `16` | 0 off, 1 FX1, … |
| FX send | `17` | byte |
| Fine | `18` | offset +50 |
| Semitone | `19` | offset +36 |
| Low / high key | `1A` / `1B` | MIDI note |
| Assign program | `32` | `<prog hi> <prog lo>` (program index) |

## 9. Sample commands

Samples are addressed by index with `2E <sample hi> <sample lo> <item>`.

| Command | Reply / action |
|---|---|
| `2E <s> 34` | channels (14-bit), length in frames (28-bit), sample rate (28-bit) |
| `2E <s> 4B` | play start (28-bit), play end (28-bit), root note, 5 bytes (probably semitone and fine, signed, plus one unknown), loop start (28-bit), loop end (28-bit) |
| `2E <s> 0A` | **start auditioning** the sample (plays on the sampler's outputs) |
| `2E <s> 0B` | stop auditioning |

ak.Sys has no sample editor: it gets the `.wav`, opens it in an external
editor, and sends it back. Loop points and tuning travel inside the file (see
[file-formats.md](file-formats.md#wav-samples)).

## 10. Effects

Section `12`. The effects board has 4 channels: FX1, FX2, RV3, RV4. Each runs
a fixed chain of modules. See [effects.md](effects.md) for every module, type
and parameter.

| Command | Meaning |
|---|---|
| `12 10` | number of channels (4) |
| `12 11 <ch>` | number of modules in the channel |
| `12 31 <ch> <mod>` → type | module's effect type |
| `12 30 <ch> <mod> <type>` | change a module's effect type |
| `12 41 <ch> <mod>` / `12 40 <ch> <mod> <0/1>` | read / set module on/off ("Active") |
| `12 21 <ch>` / `12 20 <ch> <0/1>` | read / set channel mute |
| `12 51 <ch> <mod> <param>` → signed 14-bit | read a parameter |
| `12 50 <ch> <mod> <param> <sign> <hi> <lo>` | set a parameter |

Channels and modules are numbered from 0. Effects settings belong to the
current multi and are saved in its `.akm`.

## 11. Front panel

**Screen:** send raw `01`. The sampler replies with 9600 bytes (in packets of
4096, 4096, 1408), followed by 1 byte.

- The 9600 bytes are the 320 × 240 LCD at 1 bit per pixel, 40 bytes per row,
  top row first. Bit 0 of each byte is the leftmost of its 8 pixels; 1 = dark.
- The extra byte holds the mode-button LEDs: bit n lights the button with key
  code `40 + n`.

**Buttons:** SysEx section `20`:

```
20 01 <key>        press
20 02 <key>        release
20 03 <dir> <n>    data wheel: dir 00 forward / 01 back, n clicks
```

| Code | Key | Code | Key |
|---|---|---|---|
| `40` | FX | `48`–`57` | F1–F16 (`48` + n − 1) |
| `41` | Record | `64` | Cursor left |
| `42` | Edit Sample | `65` | Cursor right |
| `43` | Edit Program | `67` | Window |
| `44` | Multi | `68` | Mark |
| `45` | Utilities | `69` | Jump |
| `46` | Save | `6A` | Exit |
| `47` | Load | `6B` | Ent/Play |
| | | `6C`–`6E` | User 1–3 |

From the spec (table 31): number keys 0–9 are `58`–`61`, `−` is `62` and `+`
is `63`. `20 04 <ASCII>` types a character, as from a computer keyboard.
Panel commands are queued: "done" means queued, not processed. Every press
must eventually be followed by a release.

## 12. Level meters

Send raw `50`. The sampler replies with 32 bytes:

| Bytes | Meaning |
|---|---|
| 0–2 | always `50 40 83` |
| 3–11, 14–15 | stale buffer contents; ignore |
| 12, 13 | record input L, R |
| 16–31 | outputs 1–16 |

Each level is attenuation in dB below full scale: `60` (96) is silence and
smaller is louder. The meters follow internal routing, so outputs without
physical sockets still show signal when something is routed to them.

## 13. Disk tools

From the spec (tables 20 and 21); not yet tested on hardware. Section `10`.
All operations act on the **selected disk**, which is separate from the disk
selected on the front panel. Disks are identified by a 14-bit handle. Refresh
the disk list (`10 01`) after any disk is added or removed, then select a disk
before anything else.

### Disks

| Command | Action / reply |
|---|---|
| `10 01` | refresh the list of connected disks |
| `10 02 <handle>` | select a disk |
| `10 03 <handle>` | check a disk is valid (done or error) |
| `10 04` | number of disks |
| `10 05` | all disks: for each, handle (14-bit), type, format, SCSI ID, writable (0/1), name |
| `10 06` | type of the selected disk: 0 floppy, 1 hard disk, 2 CD-ROM, 3 removable |
| `10 07 <handle>` | type of a given disk |
| `10 08` | handle of the selected disk |
| `10 09` | current folder path (a single `00` at the root) |
| `10 0A` | format: 0 other, 1 MS-DOS, 2 FAT32, 3 ISO9660, 4 S1000, 5 S3000, 6 EMU, 7 Roland |
| `10 0B` | free space in bytes (56-bit) |
| `10 0D <handle> <0/1>` | eject; 0 = fail if in use, 1 = discard virtual samples |
| `10 0E <handle>` | name of a disk |

### Folders

| Command | Action / reply |
|---|---|
| `10 10` | number of sub-folders in the current folder (14-bit) |
| `10 11 <index>` | name of a sub-folder |
| `10 12` | names of all sub-folders, one string after another |
| `10 13 <name>` | open a sub-folder; an empty name (`00`) goes to the root |
| `10 14` | go up one level (error at the root) |
| `10 15 <name>` | load a folder and everything in it, including sub-folders |
| `10 16 <name>` | create a sub-folder |
| `10 17 <name>` | delete a sub-folder |
| `10 18 <name> <new name>` | rename a sub-folder |

### Files

| Command | Action / reply |
|---|---|
| `10 20` | number of files in the current folder (14-bit) |
| `10 21 <index>` | name of a file |
| `10 22` | names of all files, one string after another |
| `10 23 <index>` | size of a file in bytes (28-bit) |
| `10 24 <name>` | index of the file with this name |
| `10 28 <name> <new name>` | rename a file; the new name has no extension |
| `10 29 <name>` | delete a file |
| `10 2A <name> <mode>` | load a file (name with extension); mode 0 normal, 1 samples into RAM, 2 samples as virtual |
| `10 2B <name>` | load a file with everything it uses (a program's samples, a multi's programs) |
| `10 2C <index> <type> <overwrite> <children>` | save a memory item to disk; type 1 multi, 2 program, 3 sample, 4 MIDI file, 5 setlist, 6 scenelist; overwrite 0/1; children 0/1 |
| `10 2D <type> <overwrite> <children>` | save every memory item of a type |
| `10 30 <index>` | audition a sample file from disk |
| `10 31` | stop auditioning |

Indexes of files and folders change whenever the disk changes, so refer to
items by name where possible. Plain "load" does not load dependencies; use
`10 2B` for that.

## 14. Open questions

- Disk tools: commands come from the spec, not yet tested on hardware; disk
  transfers `40`/`41` are untested.
- 2-byte index encoding in the raw transfer commands for indexes ≥ 128.
- The 128-byte block after a `01 00 00 00` keep-alive reply.
- `02 01`, `02 04`, `02 20`, `02 21` status values (see the spec's System
  Setup tables 6–7).
- Sample parameter set commands (`2E` items, spec table 18): ak.Sys does not
  use them, so they are untested.
