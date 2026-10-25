# Parameter catalog

`params.toml` lists every settable or readable parameter of the Akai
S5000/S6000 that s5000-tools knows about. The command-line tool, the editor
and the docs are generated from it, so a parameter only needs describing once.

The data is a factual description of the sampler's protocol. It was compiled
from Akai's published **S5000/S6000 MIDI System Exclusive Protocol
Specification, version 2.10** (cited, not reproduced; labels and notes are our
own words) and from what was observed on real hardware and in files saved by
the sampler ([protocol.md](../docs/protocol.md),
[file-formats.md](../docs/file-formats.md), [effects.md](../docs/effects.md)).
Where the two disagree, observation wins unless the observation itself is
marked as tentative. Every disagreement is listed in
[discrepancies.md](discrepancies.md).

## Scope

Covered sections: `02` system, `04` MIDI configuration, `06` keygroup zone,
`08` keygroup, `0A` program, `0C` multi and multi parts, `0E` sample and `12`
multi FX. Not included: one-off actions (create, delete, select, audition),
memory listings (counts, name lists, current index), block replies (`0C 47`,
`0E 34`, `0E 4B`), SysEx configuration (`00`), disk tools, scenelists, MIDI
files and the front panel. Those are documented in
[protocol.md](../docs/protocol.md).

## Schema

`[[list]]` entries are shared value lists: `id` and `values`, where a value's
position is the number sent.

Each `[[param]]` has:

| Key | Meaning |
|---|---|
| `id` | Stable key, `<scope>.<name>`. FX parameters are `fx.<module>.<name>`. |
| `scope` | `system`, `midi`, `zone`, `keygroup`, `program`, `multi`, `multi_part`, `sample` or `fx`. |
| `label` | Short UI label. |
| `section` | Base section number. The addressed forms (`2A`, `2C`, `2E`, `32`, `38`, `3A`, `3C`) carry the same items; see [protocol.md §7](../docs/protocol.md#7-program-parameters). |
| `set_item`, `get_item` | Item numbers. A missing `set_item` means read-only; a missing `get_item` means write-only. Get items are often, but not always, set + 8. |
| `index` | The selector byte(s) sent before the value: LFO 1/2, mod slot, zone, part, aux rate. Values outside `min`–`max` are invalid for this parameter even if another parameter shares the item (e.g. LFO1-only items have `min = max = 1`). When a command takes several selectors (FX channel then module) `index` is an array, in sending order. |
| `encoding` | How the value is sent, below. Missing means a composite value whose layout is given in `notes`. |
| `offset` | For `offset`: the number sent is value + offset. |
| `list` / `values` | For `enum`: a shared list id, or inline values. |
| `min`, `max` | Range of the value as the user sees it (before any offset). |
| `units`, `display_scale` | For display: shown value = value × `display_scale`. |
| `fx_module_type`, `fx_param` | FX only: the module type code (table 24 of the spec, [effects.md](../docs/effects.md)) and the parameter number. |
| `file` | Where the value is stored in a saved file, when known. |
| `source` | `spec`, `capture` or `both`: where the parameter is documented. `both` means observed on hardware and also in the spec, at least in part. |
| `notes` | Anything else, briefly. |

Encodings ([protocol.md §4](../docs/protocol.md#4-value-encodings)):

| Encoding | Bytes |
|---|---|
| `u7` | one byte |
| `bool` | one byte, 0 or 1 |
| `enum` | one byte, index into the list |
| `offset` | one byte, value + `offset` |
| `signed7` | sign (`00` +, `01` −), magnitude |
| `u14` | `hi lo`, 7 bits each |
| `signed14` | sign, `hi`, `lo` |
| `u28` / `u56` | 4 / 8 groups of 7 bits, most significant first |
| `string` | ASCII, ending in `00` |

## Addressing

Most items act on the **current** program, keygroup, multi or sample. Select
it first (`0A 06`, `08 01`, `0C 06`, `0E 06`), or use the addressed sections,
which take the item's memory index first. Zones take the zone number as their
index; in the base sections zone 0 and keygroup 0 mean "all", which must not
be used for reads through the blocked sections.

FX parameters are read and written with `12 51` / `12 50`, followed by the
channel (0 FX1, 1 FX2, 2 RV3, 3 RV4), module and `fx_param`. A parameter only
applies while that module holds the module type given by `fx_module_type`; the
first note of each module lists where that type can sit.

## File mappings

`file = { format, section, field }` names a field in the
[s5k-formats](../crates/s5k-formats/src) tables:

- `akp`: `section` is a `program::Section` (`Prg`, `Out`, `Tune`, `Lfo1`,
  `Lfo2`, `Mods`) or `program::KeygroupSection` (`Kloc`, `AmpEnv`,
  `FilterEnv`, `AuxEnv`, `Filter`) variant, or `Zone` for the `ZONE` table.
- `akm`: `section = "Part"`, the `multi::PART` table.
- `wav`: `smpl` fields are `wav::AkaiInfo` members (`loop_start` and
  `loop_end` are the two halves of `loop_points`); `fmt` fields are
  `wav::Format` members.

`{n}` in `section` or `field` stands for the value of the parameter's index,
e.g. `Lfo{n}` with `lfo = 2` is `Lfo2`, and `pan_mod_{n}` with `pan_mod = 3`
is `pan_mod_3`.

The file stores the plain value (signed values as two's complement), not the
SysEx form: a pan of −10 is −10 in the file and 54 on the wire.

Parameters without a `file` entry are not mapped yet. Zone sample and part
program names are stored as length-prefixed names (`Keygroup::zone_sample`,
`Multi::part_program`) rather than table fields.
