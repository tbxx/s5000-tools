# Effects (EB20 board)

Four channels: FX1 and FX2 (channels 0, 1) run a chain of six modules, and
RV3 and RV4 (channels 2, 3) run an input selector and a reverb. Commands are
in [protocol.md](protocol.md#10-effects). Parameters are numbered from 0 in
the order below. Values are signed 14-bit; rates are in tenths.

Every module also has an on/off switch ("Active", `12 40`), and every
channel a Mute (`12 20`).

## FX1 / FX2

| Module | Type | Parameters |
|---|---|---|
| 0 Ring mod / distortion | `01` | 0 Distort, 1 Level, 2 Freq (Hz), 3 Depth |
| 1 EQ | `09` | 0 Lo freq, 1 Lo gain, 2 LM freq, 3 LM gain, 4 LM width, 5 LM rate, 6 LM depth, 7 HM freq, 8 HM gain, 9 HM width, 10 HM rate, 11 HM depth, 12 Hi freq, 13 Hi gain |
| 2 Modulation | `02`–`08` | depends on type, below |
| 3 Delay | `0A`–`0D` | depends on type, below |
| 4 Reverb | `0E` | as RV3/RV4 module 1 |
| 5 Output mix | `0F` | 0 Mod/delay level, 1 Mod/delay pan, 2 Mod/delay width, 3 Direct signal, 4 Path, 5 Dist/EQ level, 6 Dist/EQ pan |

The EQ parameter order is assumed to follow the screen order. The output-mix
order is from the screen and needs checking.

### Modulation types

| Type | Name | Parameters |
|---|---|---|
| `02` | Chorus | 0 Rate, 1 Depth, 2 Feedback |
| `03` | Flanger | 0 Rate, 1 Depth, 2 Feedback |
| `04` | Phase | 0 Rate, 1 Depth, 2 Feedback |
| `05` | Rotary | 0 Speed 1, 1 Speed 2, 2 Accel, 3 Depth, 4 Initial speed (0 Speed 1, 1 Speed 2), 5 MIDI controller, 6 Mode (0 Level, 1 Toggle), 7 MIDI channel |
| `06` | FM / pan | 0 FM rate, 1 FM depth, 2 FM feedback, 3 AM rate, 4 AM depth, 5 AM mode (0 Pan, 1 L→R, 2 R→L, 3 Tremolo) |
| `07` | Pitch | 0 L semitone, 1 L fine, 2 R semitone, 3 R fine |
| `08` | Pitch + feedback | 0 L semitone, 1 L fine, 2 L delay, 3 L feedback, 4 R semitone, 5 R fine, 6 R delay, 7 R feedback |

### Delay types

| Type | Name | Parameters |
|---|---|---|
| `0A` | Mono L | 0 Delay, 1 Feedback, 2 HF damp, 3 Ping, 4 Feedback monitor |
| `0B` | Mono L + R | as Mono L |
| `0C` | Crossover | as Mono L |
| `0D` | Stereo | 0 L delay, 1 L feedback, 2 L HF damp, 3 R delay, 4 R feedback, 5 R HF damp, 6 Feedback monitor |

## RV3 / RV4

| Module | Type | Parameters |
|---|---|---|
| 0 Reverb input | `10` | 0 Source: 0 own send, 1 FX send, 2 FX after dist/EQ, 3 FX after mod/delay, 4 FX reverb (FX1 for RV3, FX2 for RV4) |
| 1 Reverb | `0E` | 0 Type (0 Large hall, 1 Small hall, 2 Large room, 3 Small room, 4 Gated 1, 5 Gated 2, 6 Reverse), 1 Pre-delay, 2 Decay, 3 Diffusion, 4 Near, 5 Level, 6 Pan, 7 LF damp, 8 HF damp |
