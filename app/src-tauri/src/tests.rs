//! Worker and transfers against the simulator.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use s5k::{Sampler, Simulator, Transport};

use crate::transfer::{self, Kind, Progress, ProgressEvent};
use crate::worker::{CmdResult, Device, Worker};

fn block<F: Future>(f: F) -> F::Output {
    tauri::async_runtime::block_on(f)
}

fn connected() -> Worker {
    let worker = Worker::spawn(|_| {});
    block(worker.call(|session| {
        let transport: Box<dyn Transport + Send> = Box::new(Simulator::demo());
        session.device = Some(Sampler::connect(transport).map_err(|e| e.to_string())?);
        Ok(())
    }))
    .unwrap();
    worker
}

fn run<R: Send + 'static>(
    worker: &Worker,
    f: impl FnOnce(&mut Device) -> CmdResult<R> + Send + 'static,
) -> R {
    block(worker.call(move |s| f(s.device.as_mut().unwrap()))).unwrap()
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("s5000-tools-test-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn recorder() -> (Progress, Arc<Mutex<Vec<ProgressEvent>>>) {
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = events.clone();
    (Progress::new(7, move |e| sink.lock().unwrap().push(e)), events)
}

#[test]
fn multi_round_trip_with_dependencies() {
    let worker = connected();
    let dir = temp_dir("multi");
    let (mut progress, events) = recorder();
    let d = dir.clone();
    let got = run(&worker, move |dev| {
        transfer::get(dev, Kind::Multi, "Song", &d, true, false, &mut progress)
    });
    let mut files = got.done.clone();
    files.sort();
    assert_eq!(
        files,
        [
            "Drums.akp",
            "Kick.wav",
            "Pad C3.wav",
            "Pad.akp",
            "Snare.wav",
            "Song.akm"
        ]
    );
    assert!(got.missing.is_empty());
    assert!(
        events
            .lock()
            .unwrap()
            .iter()
            .any(|e| e.job == 7 && e.item == "Kick")
    );

    // Again: everything exists, nothing is overwritten.
    let (mut progress, _) = recorder();
    let d = dir.clone();
    let again = run(&worker, move |dev| {
        transfer::get(dev, Kind::Multi, "Song", &d, true, false, &mut progress)
    });
    assert!(again.done.is_empty() && again.skipped.len() == 6);

    // Empty the sampler, then send the multi back with what it uses.
    run(&worker, |dev| dev.clear_memory().map_err(|e| e.to_string()));
    let (mut progress, _) = recorder();
    let path = dir.join("Song.akm");
    let put = run(&worker, move |dev| transfer::put(dev, &path, true, &mut progress));
    assert!(put.missing.is_empty(), "{put:?}");
    let names = run(&worker, |dev| {
        Ok((dev.sample_names(), dev.program_names(), dev.multi_names()))
    });
    assert_eq!(names.0.unwrap().len(), 3);
    assert_eq!(names.1.unwrap().len(), 2);
    assert_eq!(names.2.unwrap(), ["Song"]);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn sample_progress_and_missing_dependencies() {
    let worker = connected();
    let dir = temp_dir("sample");
    let (mut progress, events) = recorder();
    let d = dir.clone();
    run(&worker, move |dev| {
        transfer::get(dev, Kind::Sample, "Pad C3", &d, false, false, &mut progress)
    });
    assert!(dir.join("Pad C3.wav").is_file());
    assert!(events.lock().unwrap().iter().any(|e| e.done > 0));

    // A program whose samples are neither in memory nor in the folder.
    let d = dir.clone();
    let (mut progress, _) = recorder();
    run(&worker, move |dev| {
        transfer::get(dev, Kind::Program, "Drums", &d, false, false, &mut progress)
    });
    run(&worker, |dev| dev.clear_memory().map_err(|e| e.to_string()));
    let (mut progress, _) = recorder();
    let path = dir.join("Drums.akp");
    let put = run(&worker, move |dev| transfer::put(dev, &path, true, &mut progress));
    assert_eq!(put.missing, ["Kick", "Snare"]);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn idle_keep_alive_and_panel() {
    let worker = connected();
    // Several keep-alives go out while idle; the session must still work.
    std::thread::sleep(Duration::from_millis(700));
    let screen = run(&worker, |dev| dev.screen().map_err(|e| e.to_string()));
    assert_eq!(screen.bits.len(), 320 * 240 / 8);
    let meters = run(&worker, |dev| dev.meters().map_err(|e| e.to_string()));
    assert_eq!(meters.outputs, [0x60; 16]);
    let err = block(worker.call(|s| {
        s.device = None;
        Ok(())
    }));
    assert!(err.is_ok());
    assert!(block(worker.with(|dev| dev.info())).is_err());
}
