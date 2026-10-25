# Spec vs. observation

Differences between Akai's S5000/S6000 SysEx specification (v2.10, "spec") and
what the project's docs record from hardware and saved files ("observed":
[protocol.md](../docs/protocol.md), [file-formats.md](../docs/file-formats.md),
[effects.md](../docs/effects.md)). "Catalog" is what
[params.toml](params.toml) uses. Observation wins unless it is itself
tentative.

## Disagreements

| What | Spec says | Observed | Catalog |
|---|---|---|---|
| Zone playback values (`06 09`/`29`, zone +40) | 0 no looping, 1 one shot, 2 loop in release, 3 loop until release, 4 LIR→retrig, 5 play→retrig, 6 as sample | 0 = as sample | Observed: 0–6, value names left unassigned until the remaining order is checked |
| Program aftertouch pitch range (`0A 43`/`4B`) | −12…+12 | −24…+24 | Observed (±24) |
| Zone mute and solo (`06 0D`, `0E`) | `0D` mute, `0E` solo, separate | `0E` listed as one "solo/mute zone" control | Spec's split; `0E` = solo. Observation doesn't contradict it, `0D` just wasn't seen |
| Keygroup level (`08 12`/`1A`) | 0–10 in 6 dB steps, −30…+30 dB (5 = 0 dB) | Item 12 "probably", signed, −30…+30 dB (marked tentative) | Spec (enum); recheck with a capture |
| FX EQ parameter order (type `09`) | 0 lo freq, 1 lo gain, 2–4 low-mid freq/gain/width, 5–7 high-mid freq/gain/width, 8 hi freq, 9 hi gain, 10–11 low-mid sweep rate/depth, 12–13 high-mid sweep rate/depth | 0–1 lo freq/gain, 2–6 low-mid freq/gain/width/rate/depth, 7–11 high-mid the same, 12–13 hi freq/gain; stated as assumed from screen order | Spec, since the observed order is an assumption; the spec's range column matches its own order |
| Multi part summary (`0C 47`) | 12 bytes: values of items `20`–`2B` (channel, mute, solo, level, output, pan, FX channel, FX send, fine, transpose, low, high) | 11 bytes: channel, mute, level, ?, pan, ?, ?, fine, semi, low, high | Not a catalog parameter. The observed reply matches the spec's layout without solo, so the unknowns are output, FX channel and FX send |
| Sample block `0E 34` | Type (RAM/virtual, 1 byte), channels (1 byte), length (u28), rate (u28) | Channels as a 14-bit value, length, rate | Spec's split (`sample.type`, `sample.channels`); identical bytes for RAM samples |
| Sample block `0E 4B` | start, end, root, semitone (signed, 2 bytes), fine (signed, 2 bytes), playback mode (1 byte), loop start, loop end: 22 bytes | 5 bytes after the root note: "probably semitone and fine plus one unknown" | Spec: the unknown byte is playback mode |
| `02 31` | Free MPKS memory (multis, programs, keygroups, samples), % | "CPU memory free", % | Spec's meaning (`system.mpks_memory_free_pct`) |
| Keygroup item `09`/`0F` name | Zone crossfade | Velocity crossfade (docs, `kloc` field `velocity_crossfade`) | Observed name; same item |
| Mod source codes 12–14 | Names repeat those of 1, 2, 4 (the inversion mark is lost in the extracted text) | !WHEEL, !BEND, !EXT | Observed names |
| Get item numbering | Get = set + 8 for most groups, but keygroup general options +6 (`04`→`0A`), filter envelope +0x10 (`30`→`40`), LFOs +0x10 (`50`→`60`), program KG mod sources +4 (`70`→`74`) | "Generally set + 8" | Spec numbers, item by item |
| Blocked sections `38`/`3A`/`3C` | Described as user-defined blocks of **get** requests | ak.Sys also sends single **sets** through `38` and `3A`; keygroup values padded to 3 bytes | Not encoded per parameter; see protocol.md §7 |
| Program amp and pan mods (`0A 22`–`25`) | Source of each slot assignable (`22`, `24`; get `2A`, `2C`) | Slots labelled with fixed sources: amp 1 KBD, 2 AFT; pan 1 LFO2, 2 KBD, 3 wheel | Both: source params from the spec, labels in notes |

## Filled in by one side only

| What | Spec | Observed | Catalog |
|---|---|---|---|
| `02 01`, `02 04`, `02 20`, `02 21` | OS sub-version, model, play mode, front-panel lock | Polled by ak.Sys, meaning unknown | Spec meanings |
| FX reverb type names, reverb input sources, FM/pan AM modes | Ranges only (0–6, 0–4, "0–3 = pan…tremolo") | Full value names | Observed names |
| FX parameter ranges and units | Table 25 | Names and order only | Spec ranges |
| Output mix order (type `0F`) | 0–6 as listed | Same order, "needs checking" | Agrees; spec confirms it |
| Zone high/low velocity (`06 0B`/`0C`), zone mute (`0D`) | Present | Not seen | Spec |
| Program tuning template, user tuning, tuning key (`0A 32`–`34`), program number (`0A`), LFO mod amounts (`57`, `59`, `5B`), LFO1 aftertouch (`5D`), LFO2 MIDI clock sync on/off (`5E`) | Present | Not seen; not in the file maps | Spec |
| MIDI configuration (`04`) | Set items only, no gets | Not seen | Spec; write-only |
| Sample set items (`0E 20`–`2A`) | Present | Untested (ak.Sys does not use them) | Spec, flagged in notes |
| System clock, memory sizes (`02 05`/`06`, `33`, `34`) | Present | Not seen | Spec |
| Multi part program by name (`0C 33`) | Present | Only by index (`32`) seen | Both included |
| Resonance range (`08 22`) | 0–15 | "byte" | Spec |
| File positions (`.akp`, `.akm`, `.wav`) | Not covered | file-formats.md | Observed |
| USB framing, transfers, screen, meters, keep-alive | Not covered | protocol.md | Outside the catalog |

## Problems within the spec

| What | Detail | Catalog |
|---|---|---|
| Play mode (`02 10`) | Range column says 0–2; the description lists four modes including 3 = muted | 0–3 |
| Aux envelope get `6C` | Decimal given as 107 (should be 108) | `0x6C` |
| FX pitch shift semitone ranges (types `07`, `08`) | ±50, same as fine; unusual for a semitone control | As given; unverified |
| FX EQ gain | −37…+12 dB, asymmetric | As given; unverified |
