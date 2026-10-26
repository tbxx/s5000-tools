# s5000-tools app

Desktop editor for the Akai S5000/S6000: file transfers, front panel, meters
and a program editor. Tauri 2 + Svelte 5. The Rust side (`src-tauri/`) uses
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
- **Meters (⌘3):** 16 outputs and the record inputs.
- **Program (⌘4):** every program, keygroup, zone and envelope setting; edits
  go to the sampler as you turn knobs or drag envelope points. A `.akp`
  opened from the Mac is read-only.

The **Simulator** needs no hardware. It holds a few demo samples, programs, a
multi and a disk with folders. Its screen is blank, its meters are silent,
and it does not take parameter edits.

For development, a URL hash connects to the simulator and opens a view:

```
npx tauri dev --config '{"build":{"devUrl":"http://localhost:1420/#connect=simulator&view=program&program=Drums"}}'
```

`view` is `transfers`, `panel`, `meters` or `program`. Set
`S5000_TOOLS_CONFIG_DIR` to keep settings (the last Mac folder and toggles)
somewhere other than the app's config folder.

## Build

```
npm run build                  # type-check and bundle the frontend
cargo build -p s5000-tools-app # backend only, from the repo root
cargo test -p s5000-tools-app  # transfers, disks and routing against simulators
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
- `src-tauri/src/params.rs`: the program editor's catalog (`spec/params.toml`
  through `s5k::catalog`) and values read from an `.akp`.
- `src/lib/`: the views (`Transfers`, `SamplerTree`, `DepDialog`,
  `FrontPanel`, `Meters`, `ProgramEditor`) and their state.
