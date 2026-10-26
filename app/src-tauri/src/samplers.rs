//! The connected samplers: any number of USB samplers and simulators, each
//! with its own worker thread. One of them is "active": commands that take
//! no sampler id (front panel, meters, program editor) go to it.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use s5k::{Sampler, Simulator, Transport};
use serde::{Deserialize, Serialize};

use crate::worker::{CmdResult, Device, Worker};

fn str_err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// What a sampler entry connects to.
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Origin {
    Simulator,
    /// A USB sampler, by the key `usb_devices` reports.
    Usb {
        key: String,
    },
}

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Connected,
    /// Keep-alives stopped working; the entry stays so it can reconnect.
    Lost,
}

/// What the frontend shows for a sampler.
#[derive(Serialize, Clone, Debug)]
pub struct SamplerMeta {
    pub id: u32,
    /// "S5000" or "S6000" (or the sampler's own name if neither).
    pub product: String,
    /// The name the sampler reports, e.g. "AKAI S5000 (simulated)".
    pub name: String,
    /// Tells samplers apart when several are connected: "SIM 2", "USB 1-4".
    pub short_id: String,
    pub simulated: bool,
    pub os_version: String,
    pub wave_free: u8,
    pub cpu_free: u8,
    pub status: Status,
    pub active: bool,
}

/// A USB sampler found on the bus (not opened).
#[derive(Serialize, Clone, Debug)]
pub struct UsbDevice {
    pub key: String,
    pub label: String,
    /// Already connected as this sampler id.
    pub connected: Option<u32>,
}

struct Entry {
    worker: Arc<Worker>,
    origin: Origin,
    meta: SamplerMeta,
}

#[derive(Default)]
struct Inner {
    next: u32,
    next_sim: u32,
    entries: BTreeMap<u32, Entry>,
    active: Option<u32>,
}

/// Lost-connection notifications: (sampler id, message).
pub type OnLost = Arc<dyn Fn(u32, String) + Send + Sync>;

pub struct Samplers {
    inner: Arc<Mutex<Inner>>,
    on_lost: OnLost,
}

fn usb_key(d: &s5k_usb::DeviceInfo) -> String {
    format!("{}-{}", d.bus(), d.address())
}

pub fn usb_devices() -> CmdResult<Vec<s5k_usb::DeviceInfo>> {
    s5k_usb::list().map_err(str_err)
}

/// The product from the name the sampler reports.
pub fn product_of(name: &str) -> String {
    ["S6000", "S5000"]
        .into_iter()
        .find(|p| name.to_ascii_uppercase().contains(p))
        .map(str::to_string)
        .unwrap_or_else(|| name.trim().to_string())
}

/// Opens the transport and starts a session. Runs on the sampler's thread.
fn open(origin: &Origin) -> CmdResult<Device> {
    let transport: Box<dyn Transport + Send> = match origin {
        Origin::Simulator => Box::new(Simulator::demo()),
        Origin::Usb { key } => {
            let info = usb_devices()?
                .into_iter()
                .find(|d| &usb_key(d) == key)
                .ok_or("that sampler is no longer on USB")?;
            Box::new(s5k_usb::UsbTransport::open(&info).map_err(str_err)?)
        }
    };
    Sampler::connect(transport).map_err(str_err)
}

pub struct Info {
    pub name: String,
    pub os_version: String,
    pub wave_free: u8,
    pub cpu_free: u8,
}

pub fn info_of(dev: &mut Device) -> s5k::Result<Info> {
    let i = dev.info()?;
    Ok(Info {
        name: i.name,
        os_version: format!("{}.{:02}", i.os_version.0, i.os_version.1),
        wave_free: i.wave_free_percent,
        cpu_free: i.cpu_free_percent,
    })
}

impl Samplers {
    pub fn new(on_lost: impl Fn(u32, String) + Send + Sync + 'static) -> Self {
        Self {
            inner: Arc::default(),
            on_lost: Arc::new(on_lost),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The worker for `id`, or for the active sampler.
    pub fn worker(&self, id: Option<u32>) -> CmdResult<Arc<Worker>> {
        let inner = self.lock();
        let id = id.or(inner.active).ok_or("not connected")?;
        inner
            .entries
            .get(&id)
            .map(|e| e.worker.clone())
            .ok_or_else(|| "that sampler is not connected".to_string())
    }

    pub fn list(&self) -> Vec<SamplerMeta> {
        let inner = self.lock();
        inner
            .entries
            .values()
            .map(|e| SamplerMeta {
                active: inner.active == Some(e.meta.id),
                ..e.meta.clone()
            })
            .collect()
    }

    pub fn meta(&self, id: u32) -> CmdResult<SamplerMeta> {
        self.list()
            .into_iter()
            .find(|m| m.id == id)
            .ok_or_else(|| "that sampler is not connected".to_string())
    }

    pub fn active(&self) -> Option<u32> {
        self.lock().active
    }

    pub fn set_active(&self, id: u32) -> CmdResult<()> {
        let mut inner = self.lock();
        if !inner.entries.contains_key(&id) {
            return Err("that sampler is not connected".into());
        }
        inner.active = Some(id);
        Ok(())
    }

    /// USB samplers on the bus, marked with the entry they are connected as.
    pub fn usb(&self) -> CmdResult<Vec<UsbDevice>> {
        let devices = usb_devices()?;
        let inner = self.lock();
        Ok(devices
            .iter()
            .map(|d| {
                let key = usb_key(d);
                let connected = inner
                    .entries
                    .values()
                    .find(|e| e.origin == Origin::Usb { key: key.clone() })
                    .map(|e| e.meta.id);
                UsbDevice {
                    label: d.to_string(),
                    key,
                    connected,
                }
            })
            .collect())
    }

    /// Connects a new sampler and makes it active if none is.
    pub async fn add(&self, origin: Origin) -> CmdResult<SamplerMeta> {
        let (id, short_id) = {
            let mut inner = self.lock();
            if let Origin::Usb { key } = &origin
                && let Some(e) = inner.entries.values().find(|e| e.origin == origin)
            {
                return Err(format!(
                    "the sampler on USB {key} is already connected as {}",
                    e.meta.id
                ));
            }
            inner.next += 1;
            let short_id = match &origin {
                Origin::Simulator => {
                    inner.next_sim += 1;
                    format!("SIM {}", inner.next_sim)
                }
                Origin::Usb { key } => format!("USB {key}"),
            };
            (inner.next, short_id)
        };
        let worker = Arc::new(Worker::spawn(&short_id, self.lost_handler(id)));
        let o = origin.clone();
        let info = worker
            .call(move |session| {
                let mut device = open(&o)?;
                let info = info_of(&mut device).map_err(str_err)?;
                session.device = Some(device);
                session.disk_at = None;
                Ok(info)
            })
            .await?;
        let meta = SamplerMeta {
            id,
            product: product_of(&info.name),
            simulated: origin == Origin::Simulator,
            name: info.name,
            short_id,
            os_version: info.os_version,
            wave_free: info.wave_free,
            cpu_free: info.cpu_free,
            status: Status::Connected,
            active: false,
        };
        let mut inner = self.lock();
        inner.entries.insert(
            id,
            Entry {
                worker,
                origin,
                meta: meta.clone(),
            },
        );
        if inner.active.is_none() {
            inner.active = Some(id);
        }
        Ok(SamplerMeta {
            active: inner.active == Some(id),
            ..meta
        })
    }

    fn lost_handler(&self, id: u32) -> impl Fn(String) + Send + 'static {
        let inner = Arc::downgrade(&self.inner);
        let on_lost = self.on_lost.clone();
        move |message| {
            if let Some(inner) = inner.upgrade()
                && let Some(e) = inner
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .entries
                    .get_mut(&id)
            {
                e.meta.status = Status::Lost;
            }
            on_lost(id, message);
        }
    }

    /// Opens a lost sampler again on its own thread.
    pub async fn reconnect(&self, id: u32) -> CmdResult<SamplerMeta> {
        let (worker, origin) = {
            let inner = self.lock();
            let e = inner.entries.get(&id).ok_or("that sampler is not connected")?;
            (e.worker.clone(), e.origin.clone())
        };
        let info = worker
            .call(move |session| {
                session.device = None;
                let mut device = open(&origin)?;
                let info = info_of(&mut device).map_err(str_err)?;
                session.device = Some(device);
                session.disk_at = None;
                Ok(info)
            })
            .await?;
        {
            let mut inner = self.lock();
            if let Some(e) = inner.entries.get_mut(&id) {
                e.meta.status = Status::Connected;
                e.meta.os_version = info.os_version;
                e.meta.wave_free = info.wave_free;
                e.meta.cpu_free = info.cpu_free;
            }
        }
        self.meta(id)
    }

    /// Updates the free-memory figures after a change.
    pub async fn refresh_info(&self, id: u32) -> CmdResult<SamplerMeta> {
        let info = self.worker(Some(id))?.with(info_of).await?;
        if let Some(e) = self.lock().entries.get_mut(&id) {
            e.meta.wave_free = info.wave_free;
            e.meta.cpu_free = info.cpu_free;
            e.meta.status = Status::Connected;
        }
        self.meta(id)
    }

    /// Disconnects and forgets a sampler; its thread ends once its queued
    /// jobs are done. Another sampler becomes active if this one was.
    pub fn remove(&self, id: u32) -> CmdResult<()> {
        let mut inner = self.lock();
        inner.entries.remove(&id).ok_or("that sampler is not connected")?;
        if inner.active == Some(id) {
            inner.active = inner.entries.keys().next().copied();
        }
        Ok(())
    }
}
