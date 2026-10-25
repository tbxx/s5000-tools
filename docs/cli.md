# The `s5k` command

`s5k` talks to an S5000/S6000 connected by USB. Only one program can use the
sampler at a time, so quit ak.Sys or release the sampler from any virtual
machine first.

Build it with `cargo build --release -p s5k-cli`; the program is
`target/release/s5k`.

Items are named by their name in memory or by their index in `s5k list`.
Names with spaces need quotes.

## Looking

```
s5k devices                 # samplers connected
s5k info                    # name, OS version, free memory
s5k list                    # samples, programs and multis in memory
s5k screen panel.png        # save the sampler's display
s5k meters                  # output and input levels, once
```

## Transfers

```
s5k get sample Kick                      # → ./Kick.wav
s5k get program "Pad" --deps -o pads/    # the program and every sample it uses
s5k get multi Song --deps -o song/       # a multi, its programs and their samples
s5k get sample                           # every sample in memory

s5k put Kick.wav Snare.wav               # into memory
s5k put pads/Pad.akp --deps              # first sends samples the program needs
                                         # that are not in memory, from the same folder
```

`get` never overwrites files unless you add `--force`. Files are the
sampler's own formats: `.wav` (with its loop and tuning data), `.akp`, `.akm`.

## Managing memory

```
s5k rename program "Pad" "Warm Pad"
s5k delete sample Kick Snare             # asks first; -y to skip the question
s5k create program "New Kit" --size 16   # 16 keygroups
s5k create multi "Live" --size 64        # 32, 64 or 128 parts
s5k play Kick                            # audition on the sampler's outputs
s5k stop Kick
s5k clear                                # delete everything (asks first)
```

## Parameters

Every parameter in [spec/params.toml](../spec/params.toml) can be read and
changed live:

```
s5k param list keygroup                          # what's available
s5k param get program.loudness "Warm Pad"
s5k param set keygroup.filter_type "Warm Pad" VOWELISER
s5k param set keygroup.cutoff "Warm Pad" 60 -k 2      # keygroup 2
s5k param set program.lfo_waveform "Warm Pad" RANDOM -s 2   # LFO 2
s5k param set zone.sample "Warm Pad" "Pad C3" -s 1          # zone 1
s5k param set multi_part.pan Live -30 -s 0                  # part 1 (parts count from 0)
s5k param set sample.loop_start "Pad C3" 1000
```

- `-k` picks the keygroup (from 1).
- `-s` gives the selector the parameter needs: LFO 1–2, zone 1–4, multi
  part, or effects channel and module (`-s 0,2`).
- Lists such as filter types or waveforms accept the name or its number.

## Front panel

```
s5k press multi                  # tap a key
s5k press edit-program f1 f2     # several in order
```

Key names: `fx`, `record`, `edit-sample`, `edit-program`, `multi`,
`utilities`, `save`, `load`, `f1`…`f16`, `0`…`9`, `plus`, `minus`, `left`,
`right`, `window`, `mark`, `jump`, `exit`, `enter`, `user1`…`user3`.
