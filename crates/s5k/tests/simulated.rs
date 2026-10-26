//! The client against the simulated sampler.

use s5k::{ItemKind, Key, Sampler, Simulator};
use s5k_formats::Program;

fn connect() -> Sampler<Simulator> {
    Sampler::connect(Simulator::demo()).unwrap()
}

#[test]
fn lists_memory() {
    let mut s = connect();
    let info = s.info().unwrap();
    assert_eq!(info.os_version, (2, 14));
    let m = s.memory().unwrap();
    let names: Vec<_> = m.samples.iter().map(|x| x.name.as_str()).collect();
    assert_eq!(names, ["Kick", "Snare", "Pad C3"]);
    assert_eq!(m.samples[2].frames, 88200);
    assert_eq!(m.samples[2].channels, 2);
    assert_eq!(m.programs[0].keygroups, 2);
    assert_eq!(m.multis[0].parts, 32);
}

#[test]
fn empty_memory_lists_nothing() {
    let mut s = Sampler::connect(Simulator::empty()).unwrap();
    let m = s.memory().unwrap();
    assert!(m.samples.is_empty() && m.programs.is_empty() && m.multis.is_empty());
}

#[test]
fn get_and_put_round_trip() {
    let mut s = connect();
    let pad = s.get(ItemKind::Sample, 2, &mut |_, _| {}).unwrap();
    assert_eq!(pad.len(), 44 + 88200 * 4);
    let mut last = 0;
    s.put(ItemKind::Sample, "Copy.wav", &pad, &mut |done, _| last = done)
        .unwrap();
    assert_eq!(last, pad.len() as u64);
    let names = s.sample_names().unwrap();
    assert_eq!(names.last().map(String::as_str), Some("Copy"));
    let back = s.get(ItemKind::Sample, 3, &mut |_, _| {}).unwrap();
    assert_eq!(back, pad);
}

#[test]
fn bundles_follow_dependencies() {
    let mut s = connect();
    let multi = s.get_multi_bundle(0, &mut |_| {}).unwrap();
    let files: Vec<_> = multi.files.keys().cloned().collect();
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
    assert!(multi.missing.is_empty());

    // Sending a program sends only the samples memory lacks.
    let mut fresh = Sampler::connect(Simulator::empty()).unwrap();
    let akp = &multi.files["Drums.akp"];
    let missing = fresh
        .put_program_bundle(
            "Drums.akp",
            akp,
            &mut |name| multi.files.get(&format!("{name}.wav")).cloned(),
            &mut |_| {},
        )
        .unwrap();
    assert!(missing.is_empty());
    assert_eq!(fresh.sample_names().unwrap(), ["Kick", "Snare"]);
    let sent = fresh.get(ItemKind::Program, 0, &mut |_, _| {}).unwrap();
    assert_eq!(Program::parse(&sent).unwrap().sample_names(), ["Kick", "Snare"]);
}

#[test]
fn manages_memory() {
    let mut s = connect();
    s.rename(ItemKind::Program, 1, "Strings").unwrap();
    assert_eq!(s.program_names().unwrap(), ["Drums", "Strings"]);
    s.delete(ItemKind::Sample, 0).unwrap();
    assert_eq!(s.sample_names().unwrap(), ["Snare", "Pad C3"]);
    s.create_program("Empty", 4).unwrap();
    assert_eq!(s.program_keygroups(2).unwrap(), 4);
    s.create_multi("Big", 128).unwrap();
    assert_eq!(s.multi_parts(1).unwrap(), 128);
    s.audition(0, true).unwrap();
    assert!(s.delete(ItemKind::Sample, 9).is_err());
    s.clear_memory().unwrap();
    assert!(s.memory().unwrap().samples.is_empty());
}

#[test]
fn panel_and_meters() {
    let mut s = connect();
    s.tap(Key::EditProgram).unwrap();
    s.wheel(-3).unwrap();
    let screen = s.screen().unwrap();
    assert!(screen.led(Key::Multi));
    assert!(s.meters().unwrap().outputs.iter().all(|&o| o == 0x60));
    s.keep_alive().unwrap();
}

#[test]
fn lists_dependencies() {
    let mut s = connect();
    let deps = s.program_dependencies(0).unwrap();
    assert_eq!(deps.program.name, "Drums");
    let samples: Vec<_> = deps.samples.iter().map(|x| (x.name.as_str(), x.index)).collect();
    assert_eq!(samples, [("Kick", Some(0)), ("Snare", Some(1))]);
    s.delete(ItemKind::Sample, 1).unwrap();
    let multi = s.multi_dependencies(0).unwrap();
    assert_eq!(multi.len(), 2);
    assert_eq!(multi[0].samples[1].index, None); // Snare no longer in memory
    assert_eq!(multi[1].program.name, "Pad");
}

#[test]
fn simulated_disk() {
    use s5k::disk::{LoadMode, SaveType};
    let mut s = Sampler::connect(Simulator::demo()).unwrap();
    s.refresh_disks().unwrap();
    let disks = s.disks().unwrap();
    assert_eq!(disks.len(), 1);
    assert_eq!(disks[0].name, "SIM DISK");
    s.select_disk(disks[0].handle).unwrap();
    assert_eq!(s.disk_path().unwrap(), "");
    assert_eq!(s.folders().unwrap(), ["DRUMS", "SYNTHS", "EMPTY"]);
    assert_eq!(s.files().unwrap(), ["Song.akm"]);
    assert!(s.disk_free().unwrap() > 0);

    // Load a program with its sample from a folder.
    s.open_folder("SYNTHS").unwrap();
    assert_eq!(s.disk_path().unwrap(), "\\SYNTHS");
    s.load_file("Lead.akp", LoadMode::Normal, true).unwrap();
    assert!(s.program_names().unwrap().contains(&"Lead".to_string()));
    assert!(s.sample_names().unwrap().contains(&"Lead A3".to_string()));

    // Disk <-> host transfers.
    let lead = s.get_from_disk("Lead A3.wav", &mut |_, _| {}).unwrap();
    assert!(lead.starts_with(b"RIFF"));
    s.parent_folder().unwrap();
    s.open_folder("EMPTY").unwrap();
    s.put_to_disk("Copy.wav", &lead, &mut |_, _| {}).unwrap();
    assert_eq!(s.files().unwrap(), ["Copy.wav"]);
    assert_eq!(s.file_size(0).unwrap() as usize, lead.len());

    // Save a multi with everything it uses.
    s.save_to_disk(0, SaveType::Multi, true, true).unwrap();
    let mut files = s.files().unwrap();
    files.sort();
    assert_eq!(
        files,
        [
            "Copy.wav",
            "Drums.akp",
            "Kick.wav",
            "Pad C3.wav",
            "Pad.akp",
            "Snare.wav",
            "Song.akm"
        ]
    );
    assert!(s.delete_folder("DRUMS").is_err()); // not in this folder
}
