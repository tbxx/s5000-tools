//! The transfer engine against a real S5000/S6000 on USB. Skipped unless
//! asked for; nothing else may be using the sampler:
//!
//!   S5K_HARDWARE=1 cargo test -p s5000-tools-app hardware -- --ignored --test-threads=1 --nocapture
//!
//! The tests only read existing items, or write items whose names start
//! with "s5k hw", and delete those afterwards. They need a program with
//! samples in memory; if memory is completely empty they copy the
//! simulator's demo program in and delete it again at the end.

use std::path::PathBuf;
use std::time::Instant;

use crate::samplers::{Origin, Samplers};
use crate::transfer::{self, Deps, Kind, Place, Progress, Request, Source, index_of};

fn block<F: Future>(f: F) -> F::Output {
    tauri::async_runtime::block_on(f)
}

/// The real sampler (id 1) and a simulator (id 2).
fn connect() -> Option<Samplers> {
    if std::env::var("S5K_HARDWARE").is_err() {
        eprintln!("set S5K_HARDWARE=1 to run hardware tests");
        return None;
    }
    let samplers = Samplers::new(|id, why| eprintln!("sampler {id} lost: {why}"));
    let usb = samplers.usb().expect("list USB samplers");
    let key = usb.first().expect("no sampler on USB").key.clone();
    block(samplers.add(Origin::Usb { key })).expect("open the sampler");
    block(samplers.add(Origin::Simulator)).unwrap();
    Some(samplers)
}

/// Items a test copied into the real sampler's empty memory, deleted when
/// the test ends, whether it passed or not.
struct Seeded<'a> {
    samplers: &'a Samplers,
    items: Vec<(Kind, String)>,
}

impl Drop for Seeded<'_> {
    fn drop(&mut self) {
        for (kind, name) in self.items.drain(..) {
            let w = self.samplers.worker(Some(1)).unwrap();
            let _ = block(w.with(move |dev| {
                let i = index_of(dev, kind, &name)?;
                dev.delete(kind.item(), i)
            }));
        }
    }
}

/// Copies the simulator's "Pad" program and its sample into the real
/// sampler, but only when its memory is completely empty.
fn seed(samplers: &Samplers) -> Seeded<'_> {
    let mut seeded = Seeded {
        samplers,
        items: Vec::new(),
    };
    let empty = [Kind::Sample, Kind::Program, Kind::Multi]
        .into_iter()
        .all(|k| names(samplers, 1, k).is_empty());
    if empty {
        let source = Source::Memory {
            sampler: 2,
            kind: Kind::Program,
            name: "Pad".into(),
        };
        let result = run(samplers, source, Place::Memory { sampler: 1 }, Deps::All, false);
        eprintln!("memory was empty; copied in {:?}", result.done);
        seeded.items = vec![(Kind::Program, "Pad".into()), (Kind::Sample, "Pad C3".into())];
    }
    seeded
}

fn names(samplers: &Samplers, id: u32, kind: Kind) -> Vec<String> {
    block(samplers.worker(Some(id)).unwrap().with(move |dev| match kind {
        Kind::Sample => dev.sample_names(),
        Kind::Program => dev.program_names(),
        Kind::Multi => dev.multi_names(),
    }))
    .unwrap()
}

/// A program in memory that uses at least one sample, with its samples.
fn program_with_samples(samplers: &Samplers) -> (String, Vec<String>) {
    block(samplers.worker(Some(1)).unwrap().with(|dev| {
        for i in 0..dev.program_count()? {
            let deps = dev.program_dependencies(i)?;
            if !deps.samples.is_empty() && deps.samples.iter().all(|s| s.index.is_some()) {
                let samples = deps.samples.into_iter().map(|s| s.name).collect();
                return Ok((deps.program.name, samples));
            }
        }
        Err(s5k::Error::Invalid(
            "load a program with its samples first".into(),
        ))
    }))
    .unwrap()
}

fn delete(samplers: &Samplers, id: u32, kind: Kind, name: &str) {
    let name = name.to_string();
    block(samplers.worker(Some(id)).unwrap().with(move |dev| {
        let i = index_of(dev, kind, &name)?;
        dev.delete(kind.item(), i)
    }))
    .unwrap();
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("s5k-hw-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(
    samplers: &Samplers,
    source: Source,
    dest: Place,
    deps: Deps,
    replace: bool,
) -> transfer::TransferResult {
    let req = Request {
        source,
        dest,
        deps,
        replace,
    };
    block(transfer::run(samplers, req, Progress::new(1, |_| {}))).unwrap()
}

#[test]
#[ignore]
fn hardware_program_with_samples_to_the_mac_and_back() {
    let Some(samplers) = connect() else { return };
    let _seeded = seed(&samplers);
    let (program, samples) = program_with_samples(&samplers);
    eprintln!("using '{program}' with {} samples", samples.len());

    // The plan lists the program and its samples, all found in memory.
    let dir = temp_dir("plan");
    let source = Source::Memory {
        sampler: 1,
        kind: Kind::Program,
        name: program.clone(),
    };
    let lines = block(transfer::plan(
        &samplers,
        &source,
        &Place::Mac { folder: dir.clone() },
    ))
    .unwrap();
    assert_eq!(lines.len(), 1 + samples.len());
    assert!(lines.iter().all(|l| l.found && !l.exists));

    // Memory -> Mac with dependencies.
    let t = Instant::now();
    let result = run(
        &samplers,
        source,
        Place::Mac { folder: dir.clone() },
        Deps::All,
        false,
    );
    let bytes: u64 = result
        .paths
        .iter()
        .map(|p| std::fs::metadata(p).unwrap().len())
        .sum();
    eprintln!(
        "memory -> Mac: {} files, {} KB in {:.2} s ({:.0} KB/s)",
        result.paths.len(),
        bytes / 1024,
        t.elapsed().as_secs_f64(),
        bytes as f64 / 1024.0 / t.elapsed().as_secs_f64()
    );
    assert_eq!(result.paths.len(), 1 + samples.len());

    // Mac -> memory under a test name: the samples are already in memory,
    // so with dependencies only the program is sent.
    let test_file = dir.join("s5k hw prog.akp");
    std::fs::copy(dir.join(format!("{program}.akp")), &test_file).unwrap();
    let result = run(
        &samplers,
        Source::Mac { path: test_file },
        Place::Memory { sampler: 1 },
        Deps::All,
        false,
    );
    eprintln!(
        "Mac -> memory: done {:?}, skipped {:?}",
        result.done, result.skipped
    );
    assert!(names(&samplers, 1, Kind::Program).contains(&"s5k hw prog".to_string()));
    assert_eq!(
        result.skipped.len(),
        samples.len(),
        "samples already in memory are skipped"
    );
    delete(&samplers, 1, Kind::Program, "s5k hw prog");

    // A sample under a test name, alone.
    let wav = dir.join(format!("{}.wav", samples[0]));
    let test_wav = dir.join("s5k hw smp.wav");
    std::fs::copy(&wav, &test_wav).unwrap();
    let t = Instant::now();
    run(
        &samplers,
        Source::Mac {
            path: test_wav.clone(),
        },
        Place::Memory { sampler: 1 },
        Deps::Skip,
        false,
    );
    let kb = std::fs::metadata(&test_wav).unwrap().len() as f64 / 1024.0;
    eprintln!(
        "Mac -> memory: {kb:.0} KB sample in {:.2} s",
        t.elapsed().as_secs_f64()
    );
    assert!(names(&samplers, 1, Kind::Sample).contains(&"s5k hw smp".to_string()));
    delete(&samplers, 1, Kind::Sample, "s5k hw smp");

    assert!(
        !names(&samplers, 1, Kind::Program)
            .iter()
            .any(|n| n.starts_with("s5k hw"))
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
#[ignore]
fn hardware_sampler_to_simulator_and_back() {
    let Some(samplers) = connect() else { return };
    let _seeded = seed(&samplers);
    let (program, samples) = program_with_samples(&samplers);

    // Real -> simulator with dependencies.
    block(samplers.worker(Some(2)).unwrap().with(|dev| dev.clear_memory())).unwrap();
    let source = Source::Memory {
        sampler: 1,
        kind: Kind::Program,
        name: program.clone(),
    };
    let result = run(&samplers, source, Place::Memory { sampler: 2 }, Deps::All, false);
    eprintln!("real -> simulator: {:?}", result.done);
    assert_eq!(names(&samplers, 2, Kind::Program), vec![program.clone()]);
    let mut got = names(&samplers, 2, Kind::Sample);
    let mut want = samples.clone();
    got.sort();
    want.sort();
    assert_eq!(got, want);

    // Simulator -> real, renamed first so nothing of the owner's is touched.
    block(samplers.worker(Some(2)).unwrap().with(move |dev| {
        let i = index_of(dev, Kind::Program, &program)?;
        dev.rename(s5k::ItemKind::Program, i, "s5k hw copy")
    }))
    .unwrap();
    let source = Source::Memory {
        sampler: 2,
        kind: Kind::Program,
        name: "s5k hw copy".into(),
    };
    let result = run(&samplers, source, Place::Memory { sampler: 1 }, Deps::All, false);
    eprintln!(
        "simulator -> real: done {:?}, skipped {:?}",
        result.done, result.skipped
    );
    assert!(names(&samplers, 1, Kind::Program).contains(&"s5k hw copy".to_string()));
    assert_eq!(
        result.skipped.len(),
        samples.len(),
        "the real sampler already has the samples"
    );
    delete(&samplers, 1, Kind::Program, "s5k hw copy");
}

#[test]
#[ignore]
fn hardware_disks_are_listed() {
    let Some(samplers) = connect() else { return };
    let disks = block(samplers.worker(Some(1)).unwrap().with(|dev| dev.disks())).unwrap();
    eprintln!("disks: {disks:?}");
}
