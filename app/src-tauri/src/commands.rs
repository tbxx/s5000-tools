//! Tauri commands. Everything that talks to the sampler runs on the worker
//! thread; file I/O on the Mac side happens there too, next to the transfer.

use std::fs;
use std::path::PathBuf;

use s5k::{ItemKind, Key, Sampler, Simulator, Transport};
use serde::{Deserialize, Serialize};
use tauri::ipc::Response;
use tauri::{AppHandle, Emitter, State};

use crate::params::{self, ParamInfo, ProgramView};
use crate::transfer::{self, Kind, Progress, TransferResult, index_of};
use crate::worker::{CmdResult, Device, Worker};

fn str_err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

// --- Connection and listing ---------------------------------------------------

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Usb,
    Simulator,
}

#[derive(Serialize)]
pub struct Info {
    name: String,
    os_version: String,
    wave_free: u8,
    cpu_free: u8,
    simulated: bool,
}

#[tauri::command]
pub async fn connect(worker: State<'_, Worker>, mode: Mode) -> CmdResult<Info> {
    worker
        .call(move |session| {
            session.device = None;
            let (transport, simulated): (Box<dyn Transport + Send>, bool) = match mode {
                Mode::Usb => (
                    Box::new(s5k_usb::UsbTransport::open_first().map_err(str_err)?),
                    false,
                ),
                Mode::Simulator => (Box::new(Simulator::demo()), true),
            };
            let mut device = Sampler::connect(transport).map_err(str_err)?;
            let info = info_of(&mut device, simulated).map_err(str_err)?;
            session.device = Some(device);
            session.simulated = simulated;
            Ok(info)
        })
        .await
}

#[tauri::command]
pub async fn disconnect(worker: State<'_, Worker>) -> CmdResult<()> {
    worker
        .call(|session| {
            session.device = None;
            Ok(())
        })
        .await
}

fn info_of(dev: &mut Device, simulated: bool) -> s5k::Result<Info> {
    let i = dev.info()?;
    Ok(Info {
        name: i.name,
        os_version: format!("{}.{:02}", i.os_version.0, i.os_version.1),
        wave_free: i.wave_free_percent,
        cpu_free: i.cpu_free_percent,
        simulated,
    })
}

#[tauri::command]
pub async fn info(worker: State<'_, Worker>) -> CmdResult<Info> {
    worker
        .call(|session| {
            let simulated = session.simulated;
            let dev = session.device.as_mut().ok_or("not connected")?;
            info_of(dev, simulated).map_err(str_err)
        })
        .await
}

#[derive(Serialize)]
pub struct Sample {
    index: u16,
    name: String,
    channels: u16,
    frames: u32,
    sample_rate: u32,
    /// Size of the sample data in memory (16-bit).
    bytes: u64,
}

#[derive(Serialize)]
pub struct Item {
    index: u16,
    name: String,
    /// Keygroups of a program, parts of a multi.
    count: u16,
}

#[derive(Serialize)]
pub struct Memory {
    samples: Vec<Sample>,
    programs: Vec<Item>,
    multis: Vec<Item>,
}

#[tauri::command]
pub async fn memory(worker: State<'_, Worker>) -> CmdResult<Memory> {
    worker
        .with(|dev| {
            let m = dev.memory()?;
            Ok(Memory {
                samples: m
                    .samples
                    .into_iter()
                    .map(|s| Sample {
                        bytes: s.frames as u64 * s.channels as u64 * 2,
                        index: s.index,
                        name: s.name,
                        channels: s.channels,
                        frames: s.frames,
                        sample_rate: s.sample_rate,
                    })
                    .collect(),
                programs: m
                    .programs
                    .into_iter()
                    .map(|p| Item {
                        index: p.index,
                        name: p.name,
                        count: p.keygroups.into(),
                    })
                    .collect(),
                multis: m
                    .multis
                    .into_iter()
                    .map(|p| Item {
                        index: p.index,
                        name: p.name,
                        count: p.parts,
                    })
                    .collect(),
            })
        })
        .await
}

// --- Managing memory ------------------------------------------------------------

#[tauri::command]
pub async fn delete_items(worker: State<'_, Worker>, kind: Kind, names: Vec<String>) -> CmdResult<()> {
    worker
        .with(move |dev| {
            for name in &names {
                let i = index_of(dev, kind, name)?;
                dev.delete(kind.item(), i)?;
            }
            Ok(())
        })
        .await
}

#[tauri::command]
pub async fn rename_item(
    worker: State<'_, Worker>,
    kind: Kind,
    name: String,
    new_name: String,
) -> CmdResult<()> {
    if !new_name.is_ascii() || new_name.is_empty() || new_name.len() > 20 {
        return Err("names are 1–20 plain ASCII characters".into());
    }
    worker
        .with(move |dev| {
            let i = index_of(dev, kind, &name)?;
            dev.rename(kind.item(), i, &new_name)
        })
        .await
}

#[tauri::command]
pub async fn create_program(worker: State<'_, Worker>, name: String, keygroups: u8) -> CmdResult<()> {
    worker.with(move |dev| dev.create_program(&name, keygroups)).await
}

#[tauri::command]
pub async fn create_multi(worker: State<'_, Worker>, name: String, parts: u16) -> CmdResult<()> {
    worker.with(move |dev| dev.create_multi(&name, parts)).await
}

#[tauri::command]
pub async fn audition(worker: State<'_, Worker>, name: String, play: bool) -> CmdResult<()> {
    worker
        .with(move |dev| {
            let i = index_of(dev, Kind::Sample, &name)?;
            dev.audition(i, play)
        })
        .await
}

// --- Transfers --------------------------------------------------------------------

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn get_item(
    app: AppHandle,
    worker: State<'_, Worker>,
    job: u32,
    kind: Kind,
    name: String,
    folder: PathBuf,
    deps: bool,
    replace: bool,
) -> CmdResult<TransferResult> {
    let mut progress = Progress::new(job, move |e| drop(app.emit("transfer-progress", e)));
    worker
        .call(move |session| {
            let dev = session.device.as_mut().ok_or("not connected")?;
            transfer::get(dev, kind, &name, &folder, deps, replace, &mut progress)
        })
        .await
}

#[tauri::command]
pub async fn put_file(
    app: AppHandle,
    worker: State<'_, Worker>,
    job: u32,
    path: PathBuf,
    deps: bool,
) -> CmdResult<TransferResult> {
    let mut progress = Progress::new(job, move |e| drop(app.emit("transfer-progress", e)));
    worker
        .call(move |session| {
            let dev = session.device.as_mut().ok_or("not connected")?;
            transfer::put(dev, &path, deps, &mut progress)
        })
        .await
}

// --- Mac folder and settings -------------------------------------------------------------

#[derive(Serialize)]
pub struct Entry {
    name: String,
    path: PathBuf,
    /// "dir", "sample", "program" or "multi".
    kind: &'static str,
    size: u64,
}

#[tauri::command]
pub fn list_folder(path: PathBuf) -> CmdResult<Vec<Entry>> {
    let mut entries: Vec<Entry> = fs::read_dir(&path)
        .map_err(|e| format!("cannot open {}: {e}", path.display()))?
        .flatten()
        .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
        .filter_map(|e| {
            let meta = e.metadata().ok()?;
            let kind = if meta.is_dir() {
                "dir"
            } else {
                match Kind::from_path(&e.path())? {
                    Kind::Sample => "sample",
                    Kind::Program => "program",
                    Kind::Multi => "multi",
                }
            };
            Some(Entry {
                name: e.file_name().to_string_lossy().into_owned(),
                path: e.path(),
                kind,
                size: meta.len(),
            })
        })
        .collect();
    entries.sort_by(|a, b| {
        (a.kind != "dir")
            .cmp(&(b.kind != "dir"))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(entries)
}

/// Where settings live; `S5000_TOOLS_CONFIG_DIR` overrides it (for testing).
fn settings_path(app: &AppHandle) -> CmdResult<PathBuf> {
    use tauri::Manager;
    let dir = match std::env::var_os("S5000_TOOLS_CONFIG_DIR") {
        Some(d) => PathBuf::from(d),
        None => app.path().app_config_dir().map_err(str_err)?,
    };
    Ok(dir.join("settings.json"))
}

#[tauri::command]
pub fn load_settings(app: AppHandle) -> CmdResult<serde_json::Value> {
    let path = settings_path(&app)?;
    Ok(fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or(serde_json::Value::Null))
}

#[tauri::command]
pub fn save_settings(app: AppHandle, settings: serde_json::Value) -> CmdResult<()> {
    let path = settings_path(&app)?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(str_err)?;
    }
    fs::write(&path, serde_json::to_vec_pretty(&settings).map_err(str_err)?).map_err(str_err)
}

// --- Front panel and meters ---------------------------------------------------------------

/// The screen as 320 × 240 bytes (1 = dark), then the mode-LED byte.
#[tauri::command]
pub async fn screen(worker: State<'_, Worker>) -> CmdResult<Response> {
    use s5k::panel_size::{HEIGHT, WIDTH};
    let screen = worker.with(|dev| dev.screen()).await?;
    let mut out = Vec::with_capacity(WIDTH * HEIGHT + 1);
    for y in 0..HEIGHT {
        out.extend((0..WIDTH).map(|x| screen.pixel(x, y) as u8));
    }
    out.push(screen.leds);
    Ok(Response::new(out))
}

fn key(code: u8) -> CmdResult<Key> {
    Key::from_code(code).ok_or_else(|| format!("unknown key {code:#04x}"))
}

#[tauri::command]
pub async fn press(worker: State<'_, Worker>, code: u8) -> CmdResult<()> {
    let k = key(code)?;
    worker.with(move |dev| dev.press(k)).await
}

#[tauri::command]
pub async fn release(worker: State<'_, Worker>, code: u8) -> CmdResult<()> {
    let k = key(code)?;
    worker.with(move |dev| dev.release(k)).await
}

#[tauri::command]
pub async fn wheel(worker: State<'_, Worker>, clicks: i8) -> CmdResult<()> {
    worker.with(move |dev| dev.wheel(clicks)).await
}

#[derive(Serialize)]
pub struct Levels {
    inputs: [u8; 2],
    outputs: [u8; 16],
}

#[tauri::command]
pub async fn meters(worker: State<'_, Worker>) -> CmdResult<Levels> {
    let m = worker.with(|dev| dev.meters()).await?;
    Ok(Levels {
        inputs: m.inputs,
        outputs: m.outputs,
    })
}

// --- Program editor ----------------------------------------------------------------------

#[tauri::command]
pub fn param_catalog() -> Vec<ParamInfo> {
    params::catalog()
}

#[tauri::command]
pub async fn read_program(worker: State<'_, Worker>, name: String) -> CmdResult<ProgramView> {
    let bytes = worker
        .with({
            let name = name.clone();
            move |dev| {
                let i = index_of(dev, Kind::Program, &name)?;
                dev.get(ItemKind::Program, i, &mut |_, _| {})
            }
        })
        .await?;
    params::read_program(name, &bytes).map_err(str_err)
}

#[tauri::command]
pub fn read_program_file(path: PathBuf) -> CmdResult<ProgramView> {
    let bytes = fs::read(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let name = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    params::read_program(name, &bytes).map_err(str_err)
}

fn param(id: &str) -> CmdResult<&'static s5k::params::Param> {
    s5k::catalog()
        .get(id)
        .ok_or_else(|| format!("unknown parameter {id}"))
}

/// Where a program, keygroup or zone parameter lives on the sampler.
/// `keygroup` counts from 0 here and from 1 on the sampler.
fn target(scope: &str, program: u16, keygroup: Option<u8>) -> CmdResult<s5k::Target> {
    match (scope, keygroup) {
        ("program", _) => Ok(s5k::Target::Program(program)),
        ("keygroup" | "zone", Some(kg)) => Ok(s5k::Target::Keygroup {
            program,
            keygroup: kg + 1,
        }),
        _ => Err("a keygroup is needed".into()),
    }
}

/// Reads the parameters a saved program does not hold (aux envelope, some
/// mod sources, …) for the program and one keygroup. Parameters the sampler
/// refuses are left out.
#[tauri::command]
pub async fn read_live(
    worker: State<'_, Worker>,
    program: String,
    keygroup: u8,
) -> CmdResult<params::Values> {
    worker
        .with(move |dev| {
            let p = index_of(dev, Kind::Program, &program)?;
            let mut values = params::Values::new();
            for info in params::catalog().into_iter().filter(|i| !i.in_file) {
                let Ok(param) = param(&info.id) else { continue };
                if param.get_item.is_none() || matches!(param.encoding.as_deref(), None | Some("string")) {
                    continue;
                }
                let Ok(t) = target(&info.scope, p, Some(keygroup)) else {
                    continue;
                };
                let selectors: Vec<Option<u8>> = match &info.index {
                    Some((_, lo, hi)) => (*lo..=*hi).map(Some).collect(),
                    None => vec![None],
                };
                for n in selectors {
                    match dev.read_param(param, t, n.as_slice()) {
                        Ok(s5k::Value::Number(v)) => {
                            let key = n.map_or(info.id.clone(), |n| format!("{}#{n}", info.id));
                            values.insert(key, v);
                        }
                        Ok(_) => {}
                        Err(e) if e.is_timeout() => return Err(e),
                        Err(_) => {}
                    }
                }
            }
            Ok(values)
        })
        .await
}

/// Sets one parameter of a program in memory.
#[tauri::command]
pub async fn write_param(
    worker: State<'_, Worker>,
    program: String,
    id: String,
    keygroup: Option<u8>,
    selector: Option<u8>,
    value: i32,
) -> CmdResult<()> {
    let param = param(&id)?;
    worker
        .with(move |dev| {
            let p = index_of(dev, Kind::Program, &program)?;
            let t = target(&param.scope, p, keygroup).map_err(s5k::Error::Invalid)?;
            dev.write_param(param, t, selector.as_slice(), &s5k::Value::Number(value))
        })
        .await
}
