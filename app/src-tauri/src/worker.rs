//! The one thread that owns the sampler. Commands send it closures and await
//! the result; between jobs it sends keep-alives, as ak.Sys does.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use s5k::{Sampler, Transport};
use tokio::sync::oneshot;

pub type Device = Sampler<Box<dyn Transport + Send>>;

/// The connection, if any, and whether it is the simulator.
#[derive(Default)]
pub struct Session {
    pub device: Option<Device>,
    pub simulated: bool,
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
    pub fn spawn(on_lost: impl Fn(String) + Send + 'static) -> Self {
        let (jobs, rx) = mpsc::channel::<Job>();
        thread::Builder::new()
            .name("sampler".into())
            .spawn(move || run(rx, on_lost))
            .expect("cannot start the sampler thread");
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
        self.call(move |session| match session.device.as_mut() {
            Some(device) => f(device).map_err(|e| e.to_string()),
            None => Err("not connected".into()),
        })
        .await
    }
}

fn run(rx: mpsc::Receiver<Job>, on_lost: impl Fn(String)) {
    let mut session = Session::default();
    let mut last_keep_alive = Instant::now();
    let mut failures = 0;
    loop {
        match rx.recv_timeout(KEEP_ALIVE) {
            Ok(job) => job(&mut session),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        let Some(device) = session.device.as_mut() else {
            failures = 0;
            continue;
        };
        if last_keep_alive.elapsed() < KEEP_ALIVE {
            continue;
        }
        last_keep_alive = Instant::now();
        match device.keep_alive() {
            Ok(()) => failures = 0,
            Err(e) => {
                failures += 1;
                if failures >= MAX_FAILURES {
                    session.device = None;
                    failures = 0;
                    on_lost(format!("lost the sampler: {e}"));
                }
            }
        }
    }
}
