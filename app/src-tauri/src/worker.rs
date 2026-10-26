//! One thread per connected sampler, owning its `Sampler`. Commands send it
//! closures and await the result; when it has been idle for 200 ms it sends
//! a keep-alive, as ak.Sys does.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use s5k::{Sampler, Transport};
use tokio::sync::oneshot;

use crate::disk::DiskPos;

pub type Device = Sampler<Box<dyn Transport + Send>>;

/// The connection, if any, and where its disk tools are pointed.
#[derive(Default)]
pub struct Session {
    pub device: Option<Device>,
    /// The selected disk and current folder, as last set by `disk::goto`.
    /// `None` when unknown (after an error or a reconnect).
    pub disk_at: Option<DiskPos>,
}

type Job = Box<dyn FnOnce(&mut Session) + Send>;

const KEEP_ALIVE: Duration = Duration::from_millis(200);
/// Consecutive keep-alive failures before the connection is dropped.
const MAX_FAILURES: u32 = 3;

pub struct Worker {
    jobs: mpsc::Sender<Job>,
}

pub type CmdResult<T> = Result<T, String>;

impl Worker {
    /// Starts the thread. `on_lost` is called when keep-alives stop working.
    /// The thread ends when the `Worker` is dropped.
    pub fn spawn(name: &str, on_lost: impl Fn(String) + Send + 'static) -> Self {
        let (jobs, rx) = mpsc::channel::<Job>();
        thread::Builder::new()
            .name(format!("sampler {name}"))
            .spawn(move || run(rx, on_lost))
            .expect("cannot start a sampler thread");
        Self { jobs }
    }

    /// Runs `f` on the worker thread with the session.
    pub async fn call<R: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Session) -> CmdResult<R> + Send + 'static,
    ) -> CmdResult<R> {
        let (tx, rx) = oneshot::channel();
        let job: Job = Box::new(move |session| {
            let result = catch_unwind(AssertUnwindSafe(|| f(session)))
                .unwrap_or_else(|_| Err("internal error in the sampler thread".into()));
            let _ = tx.send(result);
        });
        self.jobs
            .send(job)
            .map_err(|_| "the sampler thread has stopped".to_string())?;
        rx.await
            .map_err(|_| "the sampler thread has stopped".to_string())?
    }

    /// Runs `f` with the connected sampler, or fails if there is none.
    pub async fn with<R: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Device) -> s5k::Result<R> + Send + 'static,
    ) -> CmdResult<R> {
        self.call(move |session| f(session.device()?).map_err(|e| e.to_string()))
            .await
    }
}

impl Session {
    pub fn device(&mut self) -> CmdResult<&mut Device> {
        self.device.as_mut().ok_or_else(|| "not connected".to_string())
    }
}

fn run(rx: mpsc::Receiver<Job>, on_lost: impl Fn(String)) {
    let mut session = Session::default();
    // Keep-alives go out only after 200 ms without traffic: a job is traffic,
    // so a long transfer is not interrupted between its steps.
    let mut last_traffic = Instant::now();
    let mut failures = 0;
    loop {
        let wait = KEEP_ALIVE.saturating_sub(last_traffic.elapsed());
        match rx.recv_timeout(wait) {
            Ok(job) => {
                job(&mut session);
                last_traffic = Instant::now();
                continue;
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        last_traffic = Instant::now();
        let Some(device) = session.device.as_mut() else {
            failures = 0;
            continue;
        };
        match device.keep_alive() {
            Ok(()) => failures = 0,
            Err(e) => {
                failures += 1;
                if failures >= MAX_FAILURES {
                    session.device = None;
                    session.disk_at = None;
                    failures = 0;
                    on_lost(format!("lost the sampler: {e}"));
                }
            }
        }
    }
}
