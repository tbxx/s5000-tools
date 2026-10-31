# s5000-tools app

Desktop editor for the Akai S5000/S6000: file transfers, front panel, meters
and program and multi editors. Tauri 2 + Svelte 5. The Rust side (`src-tauri/`) uses
the workspace crates `s5k`, `s5k-usb` and `s5k-formats`. The look follows
the NYC Transit signage style; tokens and components are in
[STYLE.md](STYLE.md).

## Develop

Node 22 and Rust are needed.

```
cd app
npm install
npx tauri dev
```

Only one program can use a sampler at a time: quit ak.Sys and release it from
any virtual machine first.

## Using it

- **Transfers (⌘1):** a tree per connected sampler, with Multis, Programs,
  Samples and Disks (disks → folders → files), next to a folder on the Mac.
  - **+ Add** connects more samplers (USB, or a Simulator); **Make active**
    picks the sampler the other views use.
  - The lists follow programs, samples and multis created, deleted or
    renamed on the sampler's front panel.
  - Drag between memory, disk folders, the Mac folder, the Finder and other
    samplers. Dropped files go where their type belongs (`.wav` → Samples,
    `.akp` → Programs, `.akm` → Multis), wherever they land on a sampler.
  - Programs and multis bring a dialog listing the samples and programs they
    use, with what already exists at the destination unticked.
  - Dragging out to the Finder fetches the items first (shown in the queue),
    then starts the drag.
- **Front panel (⌘2):** the live screen with F1–F8 to its left and F9–F16 to
  its right, each beside its screen row. Clicking a row of the screen presses
  that side's key. Keyboard: arrows, 0–9, +/−, Enter = Ent/Play, Esc = Exit.
- **Meters (⌘3):** 16 outputs and the record inputs, filling the window.
  Styles: Signage, Console LED, Plasma, Flying spot, Twin column, Studio
  console, and Ivory, Amber and Black VU (`src/lib/meters/`). VU needles have
  VU ballistics; LED meters peak hold. The style, peak hold and the 0 VU
  reference are remembered.
- **Windows:** the ⧉ button beside the view tabs, or the Window menu, opens
  a view in a window of its own. The front panel and meters move there
  (**Back to main window** returns them); program and multi editors can have
  any number of windows, and ⌥-double-click in the tree opens one. Open
  windows reopen at the next start. Closing the main window quits.
- **Keyboard:** Tab moves between panels, ← → between a panel's controls,
  ↑ ↓ change a value (Shift for bigger steps), F6 or ⌘] / ⌘[ between panes,
  ⌥↑ ⌥↓ through keygroups or parts. The full list is in Help.
- **Help (⌘⇧?):** a guide to every view, Preferences and the keyboard.
- **Preferences (⌘,):** interface size (⌘+ / ⌘− / ⌘0 anywhere; the window
  grows with it) and connecting automatically: off, at startup (the
  default), or also whenever a sampler is switched on. A sampler you disconnect isn't
  reconnected until it is switched off or unplugged.
- **Program (⌘4):** two panes, as in ak.Sys: the program's settings on the
  left, the keygroup's and zone's on the right, each scrolling on its own
  (one above the other in narrow windows). Every program, keygroup, zone and
  envelope setting; a knob's range is in its tooltip. **Edit all** sends
  keygroup and zone edits to every keygroup. Edits
  go to the sampler as you turn knobs or drag envelope points. A `.akp`
  opened from the Mac is read-only.
- **Multi (⌘5):** a multi's settings and a parts table like the sampler's
  Multi page: program, MIDI channel, level, pan, output, mute, solo, FX bus
  and send, transpose, fine tune and key range per part. Drag a number up or
  down (Shift for fine steps), or double-click it to type a value; edits go to
  the sampler as you change them. Selecting a row shows that part as knobs.
  **Table | Mixer** switches to a mixing desk: a strip per part with
  menus, send and pan knobs, key range, mute/solo and a level fader.
  **New multi…** creates one in memory with 32, 64 or 128 parts. A `.akm`
  opened from the Mac is read-only, and lacks mute, solo and the multi
  number, which are only read live. The effects are not edited here.

The **Simulator** needs no hardware. It holds a few demo samples, programs, a
multi and a disk with folders. Its screen is blank, its meters are silent,
and it does not take parameter edits.

For development, a URL hash connects to the simulator and opens a view:

```
npx tauri dev --config '{"build":{"devUrl":"http://localhost:1420/#connect=simulator&view=program&program=Drums"}}'
```

`view` is `transfers`, `panel`, `meters`, `program` or `multi`; `multi=Song`
opens a multi. Set
`S5000_TOOLS_CONFIG_DIR` to keep settings (the last Mac folder and toggles)
somewhere other than the app's config folder.

## Build

```
npm run build                  # type-check and bundle the frontend
cargo build -p s5000-tools-app # backend only, from the repo root
cargo test -p s5000-tools-app  # transfers, disks, routing and multis against simulators
npx tauri build --debug        # app bundle in target/debug/bundle
```

## Layout

- `src-tauri/src/samplers.rs`: the connected samplers, each with its own
  worker thread (`worker.rs`) that owns the `Sampler`. Commands send it
  closures; after 200 ms idle it sends keep-alives.
- `src-tauri/src/transfer.rs`: every transfer (memory, disks, Mac, other
  samplers) with dependencies and progress. `disk.rs` tracks the selected
  disk and folder per sampler.
- `src-tauri/src/commands.rs`: the Tauri commands.
- `src-tauri/src/params.rs`: the program and multi editors' catalogs
  (`spec/params.toml` through `s5k::catalog`) and values read from an `.akp`
  or `.akm`. `multi.rs` reads multis in memory, the values a saved multi
  lacks, and writes multi and part settings.
- `src/lib/`: the views (`Transfers`, `SamplerTree`, `DepDialog`,
  `FrontPanel`, `Meters`, `ProgramEditor`, `MultiEditor`) and their state.
