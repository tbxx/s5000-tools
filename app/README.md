# s5000-tools app

Desktop editor for the Akai S5000/S6000: file transfers, front panel, meters
and a program editor. Tauri 2 + Svelte 5. The Rust side (`src-tauri/`) uses
the workspace crates `s5k`, `s5k-usb` and `s5k-formats`.

## Develop

Node 22 and Rust are needed.

```
cd app
npm install
npx tauri dev
```

Pick **Simulator** at the top right and press Connect to work without
hardware. The simulator holds a few demo samples, programs and a multi. Its
screen is blank, its meters are silent, and it does not take parameter edits.
**Real sampler** opens the first S5000/S6000 on USB.

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
cargo test -p s5000-tools-app  # worker and transfers against the simulator
npx tauri build --debug        # app bundle in target/debug/bundle
```

## Layout

- `src-tauri/src/worker.rs`: the one thread that owns the sampler. Commands
  send it closures; when idle it sends a keep-alive every 200 ms.
- `src-tauri/src/transfer.rs`: get and put, with dependencies and progress.
- `src-tauri/src/commands.rs`: the Tauri commands.
- `src-tauri/src/params.rs`: the program editor's catalog (`spec/params.toml`
  through `s5k::catalog`) and values read from an `.akp`.
- `src/lib/`: the views (`Transfers`, `FrontPanel`, `Meters`,
  `ProgramEditor`) and their state.
