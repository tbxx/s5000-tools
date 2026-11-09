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
fn keep_alive_flags_memory_changes() {
    let mut s = connect();
    s.keep_alive().unwrap();
    assert!(!s.keep_alive().unwrap().memory_changed);
    s.create_program("New", 1).unwrap();
    assert!(s.keep_alive().unwrap().memory_changed);
    assert!(!s.keep_alive().unwrap().memory_changed);
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

#[test]
fn midi_notes_leave_commands_working() {
    let mut s = connect();
    s.midi(&[0x90, 60, 100]).unwrap();
    s.midi(&[0x80, 60, 64]).unwrap();
    assert!(s.midi(&[0xF0, 0x47, 0xF7]).is_err());
    assert!(!s.program_names().unwrap().is_empty());
}

#[test]
fn link_checks() {
    let mut s = connect();
    s.ping().unwrap();
    assert_eq!(s.echo([1, 2, 3, 0x7F]).unwrap(), [1, 2, 3, 0x7F]);
}

#[test]
fn never_sends_disk_refresh() {
    let mut s = connect();
    assert!(matches!(s.disk_request(0x01, &[]), Err(s5k::Error::Invalid(_))));
    // Nothing reached the sampler; the next command still works.
    assert_eq!(s.disk_count().unwrap(), 1);
}

#[test]
fn current_items_and_selection_by_name() {
    let mut s = connect();
    s.select_by_name(ItemKind::Program, "Pad").unwrap();
    assert_eq!(s.current(ItemKind::Program).unwrap(), Some(1));
    s.select_by_name(ItemKind::Sample, "Snare").unwrap();
    assert_eq!(s.current(ItemKind::Sample).unwrap(), Some(1));
    assert_eq!(s.current(ItemKind::Multi).unwrap(), Some(0));
    assert!(s.select_by_name(ItemKind::Program, "Nope").is_err());
    assert_eq!(s.sample_name(2).unwrap(), "Pad C3");

    let mut empty = Sampler::connect(Simulator::empty()).unwrap();
    assert_eq!(empty.current(ItemKind::Multi).unwrap(), None);
    assert!(empty.program_numbers().unwrap().is_empty());
    assert!(empty.multi_part_counts().unwrap().is_empty());
}

#[test]
fn numbers_and_part_counts_of_everything() {
    let mut s = connect();
    assert_eq!(s.program_numbers().unwrap(), [None, None]);
    assert_eq!(s.multi_numbers().unwrap(), [None]);
    assert_eq!(s.multi_part_counts().unwrap(), [32]);
}

#[test]
fn program_keygroups() {
    let mut s = connect();
    s.create_empty_program("Blank").unwrap();
    let i = s
        .program_names()
        .unwrap()
        .iter()
        .position(|n| n == "Blank")
        .unwrap() as u16;
    assert_eq!(s.current(ItemKind::Program).unwrap(), Some(i));
    assert_eq!(s.program_keygroups(i).unwrap(), 1);
    s.add_keygroups(i, 3).unwrap();
    assert_eq!(s.program_keygroups(i).unwrap(), 4);
    s.delete_keygroup(i, 4).unwrap();
    assert_eq!(s.program_keygroups(i).unwrap(), 3);
    assert!(s.delete_keygroup(i, 9).is_err());
    assert!(s.delete_keygroup(i, 0).is_err());
    assert!(s.add_keygroups(i, 0).is_err());
}

#[test]
fn multi_parts_in_one_message() {
    use s5k::PartState;
    let mut s = connect();
    let names = s.multi_part_names(0).unwrap();
    assert_eq!(names.len(), 32);
    assert_eq!(
        (names[0].as_str(), names[1].as_str(), names[2].as_str()),
        ("Drums", "Pad", "")
    );
    assert_eq!(s.multi_part_programs(0).unwrap(), ["Drums", "Pad"]);
    let parts = s.multi_assigned_parts(0).unwrap();
    assert_eq!(parts.len(), 2);
    assert_eq!(parts[1].part, 1);
    assert_eq!(parts[1].program, "Pad");
    assert_eq!(parts[1].settings.midi_channel, 1);
    assert_eq!(
        (parts[1].settings.low_note, parts[1].settings.high_note),
        (21, 127)
    );
    assert_eq!(parts[1].state, PartState::Normal);
    assert_eq!(s.multi_part_states(0).unwrap().len(), 32);
    assert_eq!(s.multi_part_summary(0, 1).unwrap(), parts[1].settings);
    assert!(s.multi_part_summary(0, 32).is_err()); // past the last part

    s.clear_part(0, 0).unwrap();
    assert_eq!(s.multi_part_programs(0).unwrap(), ["Pad"]);
}

#[test]
fn sample_blocks() {
    let mut s = connect();
    let f = s.sample_format(2).unwrap();
    assert!(!f.is_virtual);
    assert_eq!((f.channels, f.frames, f.sample_rate), (2, 88200, 44100));
    let p = s.sample_playback(2).unwrap();
    assert_eq!((p.start, p.end, p.root_note, p.loop_points), (0, 88199, 60, None));
    assert!(s.sample_playback(9).is_err());
    assert!(!s.memory().unwrap().samples[0].is_virtual);
}

#[test]
fn scenelists_midi_files_and_set_lists() {
    let mut s = connect();
    assert!(s.scenelist_names().unwrap().is_empty());
    assert!(s.midi_file_names().unwrap().is_empty());
    assert!(s.setlist_names().unwrap().is_empty());
}

#[test]
fn disk_queries() {
    use s5k::disk::{DiskFormat, DiskType};
    let mut s = connect();
    assert_eq!(s.disk_count().unwrap(), 1);
    assert_eq!(s.selected_disk().unwrap(), None);
    assert!(s.check_disk(5).unwrap());
    assert!(s.check_disk(6).is_err());
    assert_eq!(s.disk_type(5).unwrap(), DiskType::HardDisk);
    assert_eq!(s.disk_name(5).unwrap(), "SIM DISK");
    s.select_disk(5).unwrap();
    assert_eq!(s.selected_disk().unwrap(), Some(5));
    assert_eq!(s.selected_disk_type().unwrap(), DiskType::HardDisk);
    assert_eq!(s.disk_format().unwrap(), DiskFormat::Fat32);
    assert_eq!(s.folder_name(1).unwrap(), "SYNTHS");
    assert_eq!(s.file_name(0).unwrap(), "Song.akm");
    s.open_folder("DRUMS").unwrap();
    let kick = s.file_index("kick.wav").unwrap();
    assert_eq!(s.file_name(kick).unwrap(), "Kick.wav");
    s.audition_disk_file(kick, true).unwrap();
    s.audition_disk_file(kick, false).unwrap();
    let program = s.file_index("Drums.akp").unwrap();
    assert!(s.audition_disk_file(program, true).is_err());
    assert!(s.file_index("Nope.wav").is_err());
}
