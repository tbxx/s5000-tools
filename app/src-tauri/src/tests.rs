//! Samplers, transfers, dependency plans and disks against simulators.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use s5k_formats::Program;

use crate::disk::{self, DiskPos};
use crate::samplers::{Origin, Samplers, Status};
use crate::transfer::{self, Deps, Kind, Line, Place, Progress, ProgressEvent, Request, Source};
use crate::worker::CmdResult;

fn block<F: Future>(f: F) -> F::Output {
    tauri::async_runtime::block_on(f)
}

/// A registry with `n` simulators (ids 1..=n).
fn simulators(n: usize) -> Samplers {
    let samplers = Samplers::new(|_, _| {});
    for _ in 0..n {
        block(samplers.add(Origin::Simulator)).unwrap();
    }
    samplers
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

fn mem(sampler: u32) -> Place {
    Place::Memory { sampler }
}

fn mac(folder: &Path) -> Place {
    Place::Mac {
        folder: folder.to_path_buf(),
    }
}

fn disk_at(sampler: u32, path: &[&str]) -> Place {
    Place::Disk {
        sampler,
        disk: 5,
        path: path.iter().map(|s| s.to_string()).collect(),
    }
}

fn from_mem(sampler: u32, kind: Kind, name: &str) -> Source {
    Source::Memory {
        sampler,
        kind,
        name: name.into(),
    }
}

fn from_disk(sampler: u32, path: &[&str], file: &str) -> Source {
    Source::Disk {
        sampler,
        disk: 5,
        path: path.iter().map(|s| s.to_string()).collect(),
        file: file.into(),
    }
}

fn run(
    samplers: &Samplers,
    source: Source,
    dest: Place,
    deps: Deps,
    replace: bool,
) -> transfer::TransferResult {
    let (progress, _) = recorder();
    let req = Request {
        source,
        dest,
        deps,
        replace,
    };
    block(transfer::run(samplers, req, progress)).unwrap()
}

/// Sample, program and multi names in a sampler's memory.
fn names(samplers: &Samplers, id: u32) -> [Vec<String>; 3] {
    block(
        samplers
            .worker(Some(id))
            .unwrap()
            .with(|dev| Ok([dev.sample_names()?, dev.program_names()?, dev.multi_names()?])),
    )
    .unwrap()
}

fn clear(samplers: &Samplers, id: u32) {
    block(samplers.worker(Some(id)).unwrap().with(|dev| dev.clear_memory())).unwrap();
}

fn files_in(dir: &Path) -> BTreeSet<String> {
    std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect()
}

fn set(items: &[&str]) -> BTreeSet<String> {
    items.iter().map(|s| s.to_string()).collect()
}

fn line<'a>(lines: &'a [Line], name: &str) -> &'a Line {
    lines.iter().find(|l| l.name == name).unwrap()
}

// --- Routing by file type ----------------------------------------------------------------

#[test]
fn files_route_to_memory_by_type() {
    assert_eq!(Kind::from_name("Kick.wav"), Some(Kind::Sample));
    assert_eq!(Kind::from_name("BASS.AKP"), Some(Kind::Program));
    assert_eq!(Kind::from_name("Song.Akm"), Some(Kind::Multi));
    assert_eq!(Kind::from_name("notes.txt"), None);
    assert_eq!(Kind::from_name("wav"), None);

    // Dropping a mixed set of files anywhere on a sampler: each goes to its list.
    let samplers = simulators(1);
    let dir = temp_dir("routing");
    run(
        &samplers,
        from_mem(1, Kind::Multi, "Song"),
        mac(&dir),
        Deps::All,
        false,
    );
    clear(&samplers, 1);
    for file in ["Kick.wav", "Drums.akp", "Song.akm"] {
        let source = Source::Mac { path: dir.join(file) };
        run(&samplers, source, mem(1), Deps::Skip, false);
    }
    assert_eq!(names(&samplers, 1), [vec!["Kick"], vec!["Drums"], vec!["Song"]]);

    std::fs::write(dir.join("readme.txt"), "x").unwrap();
    let req = Request {
        source: Source::Mac {
            path: dir.join("readme.txt"),
        },
        dest: mem(1),
        deps: Deps::Skip,
        replace: false,
    };
    assert!(block(transfer::run(&samplers, req, Progress::new(1, |_| {}))).is_err());
    std::fs::remove_dir_all(dir).unwrap();
}

// --- Dependency plans -------------------------------------------------------------------

#[test]
fn plan_lists_a_multis_programs_and_samples() {
    let samplers = simulators(1);
    let dir = temp_dir("plan");
    std::fs::write(dir.join("Kick.wav"), b"old").unwrap();
    let lines = block(transfer::plan(
        &samplers,
        &from_mem(1, Kind::Multi, "Song"),
        &mac(&dir),
    ))
    .unwrap();
    let order: Vec<&str> = lines.iter().map(|l| l.name.as_str()).collect();
    assert_eq!(order, ["Song", "Drums", "Kick", "Snare", "Pad", "Pad C3"]);
    assert!(lines[0].main && lines.iter().skip(1).all(|l| !l.main && l.found));
    assert_eq!(line(&lines, "Kick").parent.as_deref(), Some("Drums"));
    assert!(line(&lines, "Kick").exists, "Kick.wav is in the folder");
    assert!(!line(&lines, "Snare").exists);
    // Sample sizes come from the sampler: 11025 stereo frames + header.
    assert_eq!(line(&lines, "Kick").size, Some(11025 * 4 + 44));

    // Into another sampler's memory: what it has, by name.
    let samplers = simulators(2);
    block(samplers.worker(Some(2)).unwrap().with(|dev| {
        dev.clear_memory()?;
        dev.put(
            s5k::ItemKind::Sample,
            "Pad C3.wav",
            &s5k::sim::demo_wav(10),
            &mut |_, _| {},
        )
    }))
    .unwrap();
    let lines = block(transfer::plan(
        &samplers,
        &from_mem(1, Kind::Program, "Pad"),
        &mem(2),
    ))
    .unwrap();
    assert_eq!(lines.len(), 2);
    assert!(!lines[0].exists && line(&lines, "Pad C3").exists);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn plan_reads_files_on_the_computer_and_reports_missing_samples() {
    let samplers = simulators(1);
    let dir = temp_dir("plan-files");
    let mut program = Program::blank(2);
    program
        .keygroup_mut(0)
        .unwrap()
        .set_zone_sample(0, "Kick")
        .unwrap();
    program
        .keygroup_mut(1)
        .unwrap()
        .set_zone_sample(0, "Gone")
        .unwrap();
    std::fs::write(dir.join("Beat.akp"), program.to_bytes()).unwrap();
    std::fs::write(dir.join("KICK.WAV"), s5k::sim::demo_wav(100)).unwrap();
    let source = Source::Mac {
        path: dir.join("Beat.akp"),
    };
    let lines = block(transfer::plan(&samplers, &source, &mem(1))).unwrap();
    let kick = line(&lines, "Kick");
    assert!(
        kick.found && kick.exists,
        "found by name ignoring case; Kick is in memory"
    );
    assert_eq!(kick.size, Some(100 * 4 + 44));
    assert!(!line(&lines, "Gone").found);

    // Everything missing at the destination is sent; Gone is reported.
    clear(&samplers, 1);
    let r = run(&samplers, source, mem(1), Deps::All, false);
    assert_eq!(r.done, ["Kick.wav", "Beat.akp"]);
    assert_eq!(r.missing, ["Gone"]);
    // Sent under the name the program uses, so its zones find it.
    assert_eq!(names(&samplers, 1)[0], ["Kick"]);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn chosen_dependencies_and_replace() {
    let samplers = simulators(1);
    let dir = temp_dir("chosen");
    std::fs::write(dir.join("Kick.wav"), b"old").unwrap();

    // Only what was ticked, overwriting what exists.
    let only = Deps::Only {
        files: vec!["drums.akp".into(), "Kick.wav".into()],
    };
    let r = run(
        &samplers,
        from_mem(1, Kind::Program, "Drums"),
        mac(&dir),
        only,
        false,
    );
    assert_eq!(r.done, ["Kick.wav", "Drums.akp"]);
    assert_eq!(r.paths, [dir.join("Kick.wav"), dir.join("Drums.akp")]);
    assert_ne!(std::fs::read(dir.join("Kick.wav")).unwrap(), b"old");
    assert_eq!(files_in(&dir), set(&["Drums.akp", "Kick.wav"]));

    // All: what exists is kept, unless replace.
    let r = run(
        &samplers,
        from_mem(1, Kind::Program, "Drums"),
        mac(&dir),
        Deps::All,
        false,
    );
    assert_eq!(
        (r.done, r.skipped),
        (
            vec!["Snare.wav".to_string()],
            vec!["Drums.akp".into(), "Kick.wav".into()]
        )
    );
    let r = run(
        &samplers,
        from_mem(1, Kind::Program, "Drums"),
        mac(&dir),
        Deps::All,
        true,
    );
    assert_eq!(r.done.len(), 3);
    // Skip: the item alone.
    let r = run(
        &samplers,
        from_mem(1, Kind::Multi, "Song"),
        mac(&dir),
        Deps::Skip,
        false,
    );
    assert_eq!(r.done, ["Song.akm"]);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn progress_is_throttled_and_counts_the_whole_job() {
    let samplers = simulators(1);
    let dir = temp_dir("progress");
    let (progress, events) = recorder();
    let req = Request {
        source: from_mem(1, Kind::Multi, "Song"),
        dest: mac(&dir),
        deps: Deps::All,
        replace: false,
    };
    block(transfer::run(&samplers, req, progress)).unwrap();
    let events = events.lock().unwrap();
    // One forced event per file (6) and the end, plus at most a few timed ones.
    assert!(events.len() >= 7 && events.len() <= 12, "{} events", events.len());
    let last = events.last().unwrap();
    assert_eq!((last.step, last.steps), (6, 6));
    let total = last.total.unwrap();
    assert!(last.done >= total && total >= (11025 + 8000 + 88200) * 4);
    assert!(events.iter().any(|e| e.item == "Kick.wav" && e.job == 7));
    std::fs::remove_dir_all(dir).unwrap();
}

// --- Several samplers -----------------------------------------------------------------

#[test]
fn sampler_to_sampler_with_dependencies() {
    let samplers = simulators(2);
    assert_eq!(samplers.list().len(), 2);
    assert_eq!(samplers.list()[1].short_id, "SIM 2");
    assert_eq!(samplers.list()[0].product, "S5000");
    clear(&samplers, 2);
    let r = run(
        &samplers,
        from_mem(1, Kind::Multi, "Song"),
        mem(2),
        Deps::All,
        false,
    );
    assert_eq!(r.done.len(), 6);
    let [s, p, m] = names(&samplers, 2);
    assert_eq!((s.len(), p.len(), m), (3, 2, vec!["Song".to_string()]));
    // Again: all there, nothing sent.
    let r = run(
        &samplers,
        from_mem(1, Kind::Multi, "Song"),
        mem(2),
        Deps::All,
        false,
    );
    assert!(r.done.is_empty() && r.skipped.len() == 6);
    // The same memory is refused.
    let req = Request {
        source: from_mem(1, Kind::Sample, "Kick"),
        dest: mem(1),
        deps: Deps::Skip,
        replace: false,
    };
    assert!(block(transfer::run(&samplers, req, Progress::new(1, |_| {}))).is_err());

    samplers.remove(1).unwrap();
    assert_eq!(samplers.active(), Some(2));
    assert!(samplers.worker(Some(1)).is_err());
}

// --- Disks -------------------------------------------------------------------------------

#[test]
fn disk_listing_and_navigation() {
    let samplers = simulators(1);
    let w = samplers.worker(Some(1)).unwrap();
    let disks = block(w.call(disk::disks)).unwrap();
    assert_eq!(disks.len(), 1);
    assert_eq!((disks[0].handle, disks[0].name.as_str()), (5, "SIM DISK"));

    let pos = |p: &[&str]| DiskPos {
        disk: 5,
        path: p.iter().map(|s| s.to_string()).collect(),
    };
    let root = block(w.call(move |s| disk::list(s, &pos(&[]), true))).unwrap();
    assert_eq!(root.folders, ["DRUMS", "SYNTHS", "EMPTY"]);
    assert_eq!(root.files[0].name, "Song.akm");
    assert_eq!(root.files[0].kind, "multi");
    assert!(root.files[0].size.unwrap() > 0);

    // Into a folder, across to another, and back to the root.
    for p in [&["SYNTHS"][..], &["DRUMS"], &[], &["SYNTHS"], &["SYNTHS"]] {
        let want = pos(p);
        let got = block(w.call(move |s| {
            let l = disk::list(s, &want, false)?;
            Ok((l, s.device()?.disk_path().map_err(|e| e.to_string())?))
        }))
        .unwrap();
        let expected: String = p.iter().map(|n| format!("\\{n}")).collect();
        assert_eq!(got.1, expected);
        if p == ["SYNTHS"] {
            assert_eq!(got.0.files.len(), 4);
            assert!(got.0.files.iter().all(|f| f.size.is_none()));
        }
    }
    let err: CmdResult<_> = block(w.call(move |s| disk::list(s, &pos(&["NOPE"]), false)));
    assert!(err.is_err());
    // The position was forgotten, so the next listing starts over.
    assert!(block(w.call(move |s| disk::list(s, &pos(&["DRUMS"]), false))).is_ok());
}

#[test]
fn disk_to_memory_and_back() {
    let samplers = simulators(1);
    clear(&samplers, 1);
    // A program with its samples: one load with dependencies.
    let r = run(
        &samplers,
        from_disk(1, &["SYNTHS"], "Lead.akp"),
        mem(1),
        Deps::All,
        false,
    );
    assert_eq!(r.done, ["Lead A3.wav", "Lead.akp"]);
    assert_eq!(names(&samplers, 1)[..2], [vec!["Lead A3"], vec!["Lead"]]);
    // A ticked subset: file by file.
    let only = Deps::Only {
        files: vec!["Kick.wav".into()],
    };
    run(
        &samplers,
        from_disk(1, &["DRUMS"], "Drums.akp"),
        mem(1),
        only,
        false,
    );
    assert_eq!(names(&samplers, 1)[0], ["Lead A3", "Kick"]);
    // Plan from the disk: Lead A3 is now in memory.
    let lines = block(transfer::plan(
        &samplers,
        &from_disk(1, &["SYNTHS"], "Lead.akp"),
        &mem(1),
    ))
    .unwrap();
    assert!(line(&lines, "Lead A3").exists && line(&lines, "Lead A3").size == Some(44100 * 4 + 44));

    // Memory to a disk folder, with children, in one save.
    let r = run(
        &samplers,
        from_mem(1, Kind::Program, "Lead"),
        disk_at(1, &["EMPTY"]),
        Deps::All,
        false,
    );
    assert_eq!(r.done, ["Lead A3.wav", "Lead.akp"]);
    let w = samplers.worker(Some(1)).unwrap();
    let listing = block(w.call(|s| {
        disk::list(
            s,
            &DiskPos {
                disk: 5,
                path: vec!["EMPTY".into()],
            },
            false,
        )
    }))
    .unwrap();
    let files: Vec<&str> = listing.files.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(files, ["Lead.akp", "Lead A3.wav"]);
    // Saving again keeps what is there.
    let r = run(
        &samplers,
        from_mem(1, Kind::Program, "Lead"),
        disk_at(1, &["EMPTY"]),
        Deps::All,
        false,
    );
    assert!(r.done.is_empty() && r.skipped.len() == 2);
}

#[test]
fn disk_files_to_the_computer_and_other_samplers() {
    let samplers = simulators(2);
    let dir = temp_dir("disk-mac");
    // Disk to the computer, with what the multi uses (all in other folders,
    // so only the multi is found).
    let r = run(
        &samplers,
        from_disk(1, &[], "Song.akm"),
        mac(&dir),
        Deps::All,
        false,
    );
    assert_eq!(r.done, ["Song.akm"]);
    assert_eq!(r.missing, ["Drums", "Pad"]);
    let r = run(
        &samplers,
        from_disk(1, &["DRUMS"], "Drums.akp"),
        mac(&dir),
        Deps::All,
        false,
    );
    assert_eq!(
        files_in(&dir),
        set(&["Drums.akp", "Kick.wav", "Snare.wav", "Song.akm"])
    );
    assert_eq!(r.paths.len(), 3);

    // The computer to a disk folder.
    let source = Source::Mac {
        path: dir.join("Drums.akp"),
    };
    let r = run(&samplers, source, disk_at(2, &["EMPTY"]), Deps::All, false);
    assert_eq!(r.done, ["Kick.wav", "Snare.wav", "Drums.akp"]);
    let w = samplers.worker(Some(2)).unwrap();
    let bytes = block(w.call(|s| {
        let pos = DiskPos {
            disk: 5,
            path: vec!["EMPTY".into()],
        };
        disk::at(s, &pos, |dev| dev.get_from_disk("Kick.wav", &mut |_, _| {}))
    }))
    .unwrap();
    assert_eq!(bytes, std::fs::read(dir.join("Kick.wav")).unwrap());

    // One sampler's disk into another sampler's memory.
    clear(&samplers, 2);
    run(
        &samplers,
        from_disk(1, &["SYNTHS"], "Pad.akp"),
        mem(2),
        Deps::All,
        false,
    );
    assert_eq!(names(&samplers, 2)[..2], [vec!["Pad C3"], vec!["Pad"]]);
    // And one disk folder to another.
    run(
        &samplers,
        from_disk(1, &["SYNTHS"], "Lead A3.wav"),
        disk_at(2, &[]),
        Deps::Skip,
        false,
    );
    let root = block(w.call(|s| {
        disk::list(
            s,
            &DiskPos {
                disk: 5,
                path: vec![],
            },
            false,
        )
    }))
    .unwrap();
    assert!(root.files.iter().any(|f| f.name == "Lead A3.wav"));
    std::fs::remove_dir_all(dir).unwrap();
}

// --- Worker ----------------------------------------------------------------------------

#[test]
fn idle_keep_alive_and_panel() {
    let samplers = simulators(1);
    let worker = samplers.worker(None).unwrap();
    // Several keep-alives go out while idle; the session must still work.
    std::thread::sleep(Duration::from_millis(700));
    let screen = block(worker.with(|dev| dev.screen())).unwrap();
    assert_eq!(screen.bits.len(), 320 * 240 / 8);
    let meters = block(worker.with(|dev| dev.meters())).unwrap();
    assert_eq!(meters.outputs, [0x60; 16]);
    block(worker.call(|s| {
        s.device = None;
        Ok(())
    }))
    .unwrap();
    assert!(block(worker.with(|dev| dev.info())).is_err());
    assert_eq!(samplers.list()[0].status, Status::Connected);
}

// --- Multi editor --------------------------------------------------------------------------

#[test]
fn multi_editor_reads_a_multi_in_memory() {
    use crate::multi;
    let samplers = simulators(1);
    // The simulator's demo multi: 32 parts, programs on parts 1 and 2.
    let view = block(multi::read(&samplers, None, "Song".into())).unwrap();
    assert_eq!(view.name, "Song");
    assert_eq!(view.values.get("multi.part_count"), Some(&32));
    assert_eq!(view.parts.len(), 32);
    assert_eq!(view.parts[0].program.as_deref(), Some("Drums"));
    assert_eq!(view.parts[1].program.as_deref(), Some("Pad"));
    assert_eq!(view.parts[2].program, None);
    let part = &view.parts[5].values;
    assert_eq!(part.get("multi_part.midi_channel"), Some(&5));
    assert_eq!(part.get("multi_part.level"), Some(&100));
    assert_eq!(part.get("multi_part.low_key"), Some(&21));
    assert_eq!(part.get("multi_part.high_key"), Some(&127));
    // Mute, solo and the multi number are read live; the simulator refuses
    // them, which leaves them out rather than failing the read.
    let live = block(multi::read_live(&samplers, None, "Song".into())).unwrap();
    assert_eq!(live.parts.len(), 32);
    assert!(live.values.is_empty());
    assert!(live.parts.iter().all(|p| p.is_empty()));
    // A multi that is not in memory.
    assert!(
        block(multi::read(&samplers, None, "Nope".into()))
            .unwrap_err()
            .contains("no longer in memory")
    );
}

#[test]
fn multi_editor_new_multi_and_file() {
    use crate::multi;
    let samplers = simulators(1);
    let w = samplers.worker(None).unwrap();
    block(w.with(|dev| dev.create_multi("Big", 64))).unwrap();
    assert!(names(&samplers, 1)[2].contains(&"Big".to_string()));
    let view = block(multi::read(&samplers, None, "Big".into())).unwrap();
    assert_eq!(view.parts.len(), 64);
    assert_eq!(view.values.get("multi.part_count"), Some(&64));

    let dir = temp_dir("multi-file");
    let mut akm = s5k_formats::Multi::blank(32);
    akm.set_part_program(3, "Keys").unwrap();
    akm.part_set(3, "fx_send", 40).unwrap();
    let path = dir.join("Set A.akm");
    std::fs::write(&path, akm.to_bytes()).unwrap();
    let view = multi::read_file(&path).unwrap();
    assert_eq!(view.name, "Set A");
    assert_eq!(view.parts[3].program.as_deref(), Some("Keys"));
    assert_eq!(view.parts[3].values.get("multi_part.fx_send"), Some(&40));
    assert!(multi::read_file(&dir.join("missing.akm")).is_err());
    std::fs::remove_dir_all(dir).unwrap();
}

/// The simulator does not take multi edits (as with programs), so this checks
/// that each write is addressed, validated and sent: the simulator's refusal
/// comes back as the sampler's error, while bad requests never reach it.
#[test]
fn multi_editor_writes_part_parameters() {
    use crate::multi;
    let samplers = simulators(1);
    let write = |id: &str, part: Option<u8>, value: i32| {
        block(multi::write(
            &samplers,
            None,
            "Song".into(),
            id.into(),
            part,
            value,
        ))
    };
    let refused = |r: CmdResult<()>| {
        let e = r.unwrap_err();
        assert!(e.contains("the sampler refused 2C"), "{e}");
    };
    refused(write("multi_part.level", Some(3), 80));
    refused(write("multi_part.pan", Some(0), -50));
    refused(write("multi_part.mute", Some(31), 1));
    refused(write("multi.program_number", None, 12));
    refused(block(multi::set_part_program(
        &samplers,
        None,
        "Song".into(),
        2,
        "Pad".into(),
    )));
    // Checked before anything is sent.
    assert!(
        write("multi_part.level", Some(3), 101)
            .unwrap_err()
            .contains("0..100")
    );
    assert!(
        write("multi_part.level", None, 80)
            .unwrap_err()
            .contains("needs a part")
    );
    assert!(
        write("multi.program_number", Some(0), 1)
            .unwrap_err()
            .contains("not a part")
    );
    assert!(
        write("multi.program_number", None, 129)
            .unwrap_err()
            .contains("0..128")
    );
    assert!(
        write("multi_part.nope", Some(0), 1)
            .unwrap_err()
            .contains("unknown")
    );
    assert!(
        block(multi::set_part_program(
            &samplers,
            None,
            "Song".into(),
            2,
            "Nope".into()
        ))
        .unwrap_err()
        .contains("no longer in memory")
    );
    // The multi itself is unchanged.
    let view = block(multi::read(&samplers, None, "Song".into())).unwrap();
    assert_eq!(view.parts[3].values.get("multi_part.level"), Some(&100));
}
