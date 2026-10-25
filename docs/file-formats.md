# S5000/S6000 file formats

The sampler stores and transfers programs (`.akp`), multis (`.akm`) and samples
(`.wav`). All three are RIFF-style files: a 12-byte header, then chunks of
`<4-char id> <u32 LE length> <data>`. Offsets below are **within a chunk's
data**, i.e. after its 8-byte chunk header. Multi-byte numbers are
little-endian. "Signed" means a two's-complement byte.

## Saved junk

When the sampler saves a file it leaves stale memory in unused bytes: the
space after names, the last byte of some chunks, unused zones. Readers must
take names by their length byte, never by scanning for text, and ignore the
padding bytes noted below. Writers may zero them.

## Programs (`.akp`)

Header: `RIFF`, a length field that is always 0, `APRG`.

Top-level chunks, in order: `prg `, `out `, `tune`, `lfo ` (LFO1),
`lfo ` (LFO2), `mods`, then one `kgrp` per keygroup. Each `kgrp` contains
`kloc`, `env ` (amp), `env ` (filter), `env ` (aux), `filt`, and four `zone`
chunks.

### `prg `

| Offset | Field |
|---|---|
| +5 | keygroup crossfade (0/1) |

### `out `

| Offset | Field |
|---|---|
| +1 | loudness |
| +2, +3 | amp mod amount 1 (KBD), 2 (AFT); signed |
| +4, +5, +6 | pan mod amount 1–3; signed |
| +7 | velocity sensitivity; signed |

### `tune`

| Offset | Field |
|---|---|
| +1 | semitone; signed |
| +2 | fine; signed |
| +15, +16 | pitch bend up, down |
| +17 | bend mode (0 normal, 1 held) |
| +18 | aftertouch; signed |
| +19 | mono legato |
| +21 | portamento time |
| +22 | portamento on |
| +23 | padding (junk) |

### `lfo ` (LFO1, then LFO2)

| Offset | Field |
|---|---|
| +1 | waveform (see [protocol.md](protocol.md#program-level-2a)) |
| +2, +3, +4 | rate, delay, depth |
| +5 | LFO1: key sync |
| +6 | LFO2: retrigger |
| +7 | LFO1: wheel amount |
| +13 | LFO2: MIDI clock division |

### `mods`

The mod source assignments occupy odd offsets; the byte after each is
presumably its amount.

| Offset | Field |
|---|---|
| +15, +17, +19 | LFO1 rate, delay, depth mod source |
| +21, +23, +25 | LFO2 rate, delay, depth mod source |
| +27, +29 | keygroup pitch mod source 1, 2 |
| +31 | keygroup amp mod source |
| +33, +35, +37 | filter mod source 1, 2, 3 |

### `kloc` (keygroup)

| Offset | Field |
|---|---|
| +4, +5 | low key, high key (MIDI note) |
| +6, +7 | semitone, fine; signed |
| +8 | FX override (0 off, 1 FX1, 2 FX2, 3 RV3, 4 RV4) |
| +9 | FX send |
| +10, +11, +12 | keygroup pitch/amp mod amounts; signed |
| +13 | velocity crossfade |
| +14 | mute group |

### `env ` (amp, filter, aux)

| Offset | Field |
|---|---|
| +1 | attack |
| +3 | decay |
| +4 | release |
| +7 | sustain |
| +9 | filter envelope only: depth; signed |
| +10 | velocity > attack; signed |
| +12 | keyscale; signed |
| +14 | velocity > release; signed |
| +15 | off > release; signed |
| +17 | padding (junk) |

The aux envelope has four rates and four levels; their exact offsets within
+1…+7 still need mapping.

### `filt`

| Offset | Field |
|---|---|
| +1 | filter type (0–25) |
| +2 | cutoff |
| +3 | resonance |
| +4 | keyboard tracking; signed |
| +5, +6, +7 | mod amounts 1–3; signed |
| +9 | attenuation (0–5 = 0–30 dB in 6 dB steps) |

### `zone` (four per keygroup)

| Offset | Field |
|---|---|
| +1 | sample name length (0 = empty zone) |
| +2… | sample name; bytes after the length are junk |
| +36 | fine; signed |
| +37 | semitone; signed |
| +38 | filter; signed |
| +39 | pan; signed (−50…+50) |
| +40 | playback mode |
| +41 | output |
| +42 | level; signed |
| +43 | keyboard tracking |
| +44, +45 | velocity > start; signed 16-bit |

## Multis (`.akm`)

Header: `RIFF`, length 0, `AMUL`. Chunks: `fx  ` (530 bytes, effects
settings), `mver`, then one `part` chunk (48 bytes) per part.

### `part`

| Offset | Field |
|---|---|
| +1 | program name length (0 = no program) |
| +2… | program name; bytes after the length are junk |
| +34 | MIDI channel (0 = 1A …) |
| +35 | semitone; signed |
| +36, +37 | low key, high key |
| +38 | level |
| +39 | pan; signed |
| +40 | always 0 |
| +41 | fine; signed |
| +42 | FX channel |
| +43 | FX send |
| +44 | output |

### `fx  `

Partially mapped:

| Offset | Field |
|---|---|
| +14 | FX1 channel mute |
| +16, +17 | FX1 module 0 parameter 2 (16-bit) |
| +18, +19, +20 | FX1 module 0 parameters 3, 0, 1 |
| +152 + p | FX2 module 1 (EQ) parameter p |
| +370…+381 | RV3 module 1 (reverb) parameters, not in parameter order |

The remaining modules follow the same pattern and still need mapping, by
changing a value and diffing a saved `.akm`.

## Samples (`.wav`)

Standard PCM WAV (16-bit) with `fmt `, `fact`, `smpl` and `data` chunks. The
`smpl` chunk carries Akai-specific data:

| Offset | Field |
|---|---|
| +0 | manufacturer `0x01000047` (Akai) |
| +4 | product `0x5E` (S5000/S6000) |
| +8 | sample period in ns |
| +12 | MIDI unity note = root note + semitone tune |
| +28 | number of standard loops: 0 |
| +32 | sampler data length (18) |
| +36 | `loop` |
| +44, +48 | loop start, loop end (u32 frames) |
| +60 | `02 00`, then root note, semitone (signed), fine (signed) |
| +65… | `00 00 00 00 01`, then play start (u32), play end (u32) |

Because the standard loop count is 0, other software sees no loop; loop
points and tuning are only in Akai's record.
