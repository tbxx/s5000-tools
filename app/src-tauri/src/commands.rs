//! Tauri commands. Everything that talks to a sampler runs on its worker
//! thread. Commands that take an optional `sampler` id act on the active
//! sampler without one (the front panel, meters and program editor).

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use s5k::{ItemKind, Key};
use serde::Serialize;
use tauri::ipc::Response;
use tauri::{AppHandle, Emitter, State};

use crate::disk::{self, DiskInfo, DiskPos, Listing};
use crate::params::{self, ParamInfo, ProgramView};
use crate::samplers::{Origin, SamplerMeta, Samplers, UsbDevice};
use crate::transfer::{self, Kind, Line, Place, Progress, Request, Source, TransferResult, index_of};
use crate::worker::{CmdResult, Device};

fn str_err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

async fn with<R: Send + 'static>(
    samplers: &Samplers,
    sampler: Option<u32>,
    f: impl FnOnce(&mut Device) -> s5k::Result<R> + Send + 'static,
) -> CmdResult<R> {
    samplers.worker(sampler)?.with(f).await
}

// --- Samplers --------------------------------------------------------------------

#[tauri::command]
pub fn samplers(samplers: State<'_, Samplers>) -> Vec<SamplerMeta> {
    samplers.list()
}

/// USB samplers on the bus. Listing does not open them.
#[tauri::command]
pub fn usb_devices(samplers: State<'_, Samplers>) -> CmdResult<Vec<UsbDevice>> {
    samplers.usb()
}

#[tauri::command]
pub async fn add_sampler(samplers: State<'_, Samplers>, origin: Origin) -> CmdResult<SamplerMeta> {
    samplers.add(origin).await
}

#[tauri::command]
pub fn remove_sampler(samplers: State<'_, Samplers>, id: u32) -> CmdResult<()> {
    samplers.remove(id)
}

#[tauri::command]
pub async fn reconnect_sampler(samplers: State<'_, Samplers>, id: u32) -> CmdResult<SamplerMeta> {
    samplers.reconnect(id).await
}

#[tauri::command]
pub fn set_active(samplers: State<'_, Samplers>, id: u32) -> CmdResult<()> {
    samplers.set_active(id)
}

/// The sampler's details with fresh free-memory figures.
#[tauri::command]
pub async fn sampler_info(samplers: State<'_, Samplers>, id: Option<u32>) -> CmdResult<SamplerMeta> {
    let id = id.or(samplers.active()).ok_or("not connected")?;
    samplers.refresh_info(id).await
}

// --- Listing memory -----------------------------------------------------------------

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
pub async fn memory(samplers: State<'_, Samplers>, sampler: Option<u32>) -> CmdResult<Memory> {
    with(&samplers, sampler, |dev| {
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
pub async fn delete_items(
    samplers: State<'_, Samplers>,
    sampler: Option<u32>,
    kind: Kind,
    names: Vec<String>,
) -> CmdResult<()> {
    with(&samplers, sampler, move |dev| {
        for name in &names {
            let i = index_of(dev, kind, name)?;
            dev.delete(kind.item(), i)?;
        }
        Ok(())
    })
    .await
}

fn check_name(name: &str) -> CmdResult<()> {
    if !name.is_ascii() || name.is_empty() || name.len() > 20 {
        return Err("names are 1–20 plain ASCII characters".into());
    }
    Ok(())
}

#[tauri::command]
pub async fn rename_item(
    samplers: State<'_, Samplers>,
    sampler: Option<u32>,
    kind: Kind,
    name: String,
    new_name: String,
) -> CmdResult<()> {
    check_name(&new_name)?;
    with(&samplers, sampler, move |dev| {
        let i = index_of(dev, kind, &name)?;
        dev.rename(kind.item(), i, &new_name)
    })
    .await
}

#[tauri::command]
pub async fn create_program(
    samplers: State<'_, Samplers>,
    sampler: Option<u32>,
    name: String,
    keygroups: u8,
) -> CmdResult<()> {
    with(&samplers, sampler, move |dev| {
        dev.create_program(&name, keygroups)
    })
    .await
}

#[tauri::command]
pub async fn create_multi(
    samplers: State<'_, Samplers>,
    sampler: Option<u32>,
    name: String,
    parts: u16,
) -> CmdResult<()> {
    with(&samplers, sampler, move |dev| dev.create_multi(&name, parts)).await
}

#[tauri::command]
pub async fn audition(
    samplers: State<'_, Samplers>,
    sampler: Option<u32>,
    name: String,
    play: bool,
) -> CmdResult<()> {
    with(&samplers, sampler, move |dev| {
        let i = index_of(dev, Kind::Sample, &name)?;
        dev.audition(i, play)
    })
    .await
}

// --- Disks ------------------------------------------------------------------------

#[tauri::command]
pub async fn disks(samplers: State<'_, Samplers>, sampler: u32, rescan: bool) -> CmdResult<Vec<DiskInfo>> {
    samplers
        .worker(Some(sampler))?
        .call(move |s| disk::disks(s, rescan))
        .await
}

#[tauri::command]
pub async fn disk_list(
    samplers: State<'_, Samplers>,
    sampler: u32,
    disk: u16,
    path: Vec<String>,
    sizes: bool,
) -> CmdResult<Listing> {
    let pos = DiskPos { disk, path };
    samplers
        .worker(Some(sampler))?
        .call(move |s| disk::list(s, &pos, sizes))
        .await
}

#[tauri::command]
pub async fn disk_create_folder(
    samplers: State<'_, Samplers>,
    sampler: u32,
    disk: u16,
    path: Vec<String>,
    name: String,
) -> CmdResult<()> {
    check_name(&name)?;
    let pos = DiskPos { disk, path };
    samplers
        .worker(Some(sampler))?
        .call(move |s| disk::at(s, &pos, |dev| dev.create_folder(&name)))
        .await
}

/// Deletes files (names with extension) and empty folders at `path`.
#[tauri::command]
pub async fn disk_delete(
    samplers: State<'_, Samplers>,
    sampler: u32,
    disk: u16,
    path: Vec<String>,
    files: Vec<String>,
    folders: Vec<String>,
) -> CmdResult<()> {
    let pos = DiskPos { disk, path };
    samplers
        .worker(Some(sampler))?
        .call(move |s| {
            disk::at(s, &pos, |dev| {
                for f in &files {
                    dev.delete_file(f)?;
                }
                for f in &folders {
                    dev.delete_folder(f)?;
                }
                Ok(())
            })
        })
        .await
}

// --- Transfers --------------------------------------------------------------------

/// What a transfer would copy and what already exists at the destination.
#[tauri::command]
pub async fn plan_transfer(
    samplers: State<'_, Samplers>,
    source: Source,
    dest: Place,
) -> CmdResult<Vec<Line>> {
    transfer::plan(&samplers, &source, &dest).await
}

#[tauri::command]
pub async fn transfer(
    app: AppHandle,
    samplers: State<'_, Samplers>,
    job: u32,
    request: Request,
) -> CmdResult<TransferResult> {
    let progress = Progress::new(job, move |e| drop(app.emit("transfer-progress", e)));
    transfer::run(&samplers, request, progress).await
}

/// Where files fetched for a drag to the Finder are put first.
pub fn drag_root() -> PathBuf {
    std::env::temp_dir().join("s5000-tools-drag")
}

/// A new, empty folder for one drag to the Finder.
#[tauri::command]
pub fn drag_folder() -> CmdResult<PathBuf> {
    static NEXT: AtomicU32 = AtomicU32::new(1);
    let dir = drag_root().join(format!(
        "{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    Ok(dir)
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
pub async fn screen(samplers: State<'_, Samplers>, sampler: Option<u32>) -> CmdResult<Response> {
    use s5k::panel_size::{HEIGHT, WIDTH};
    let screen = with(&samplers, sampler, |dev| dev.screen()).await?;
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
pub async fn press(samplers: State<'_, Samplers>, sampler: Option<u32>, code: u8) -> CmdResult<()> {
    let k = key(code)?;
    with(&samplers, sampler, move |dev| dev.press(k)).await
}

#[tauri::command]
pub async fn release(samplers: State<'_, Samplers>, sampler: Option<u32>, code: u8) -> CmdResult<()> {
    let k = key(code)?;
    with(&samplers, sampler, move |dev| dev.release(k)).await
}

#[tauri::command]
pub async fn wheel(samplers: State<'_, Samplers>, sampler: Option<u32>, clicks: i8) -> CmdResult<()> {
    with(&samplers, sampler, move |dev| dev.wheel(clicks)).await
}

#[derive(Serialize)]
pub struct Levels {
    inputs: [u8; 2],
    outputs: [u8; 16],
}

#[tauri::command]
pub async fn meters(samplers: State<'_, Samplers>, sampler: Option<u32>) -> CmdResult<Levels> {
    let m = with(&samplers, sampler, |dev| dev.meters()).await?;
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
pub async fn read_program(
    samplers: State<'_, Samplers>,
    sampler: Option<u32>,
    name: String,
) -> CmdResult<ProgramView> {
    let n = name.clone();
    let bytes = with(&samplers, sampler, move |dev| {
        let i = index_of(dev, Kind::Program, &n)?;
        dev.get(ItemKind::Program, i, &mut |_, _| {})
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
    samplers: State<'_, Samplers>,
    sampler: Option<u32>,
    program: String,
    keygroup: u8,
) -> CmdResult<params::Values> {
    with(&samplers, sampler, move |dev| {
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
    samplers: State<'_, Samplers>,
    sampler: Option<u32>,
    program: String,
    id: String,
    keygroup: Option<u8>,
    selector: Option<u8>,
    value: i32,
) -> CmdResult<()> {
    let param = param(&id)?;
    with(&samplers, sampler, move |dev| {
        let p = index_of(dev, Kind::Program, &program)?;
        let t = target(&param.scope, p, keygroup).map_err(s5k::Error::Invalid)?;
        dev.write_param(param, t, selector.as_slice(), &s5k::Value::Number(value))
    })
    .await
}
