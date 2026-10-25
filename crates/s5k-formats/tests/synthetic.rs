//! Tests on files built from scratch (no sampler content in the repo).

use s5k_formats::{AkaiInfo, KeygroupSection, Multi, Program, Section, Wav};

fn chunk(id: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut out = id.to_vec();
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(data);
    out
}

fn riff(form: &[u8; 4], size_field: u32, body: &[u8]) -> Vec<u8> {
    let mut out = b"RIFF".to_vec();
    out.extend_from_slice(&size_field.to_le_bytes());
    out.extend_from_slice(form);
    out.extend_from_slice(body);
    out
}

/// A program with the newer chunk sizes. Every byte is a marker so that
/// round-trip errors show up; `stale` fills zone-name padding like the
/// sampler does.
fn program_bytes(keygroups: usize, lfo_len: usize) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend(chunk(b"prg ", &[1, 0, 0, 0, 0, 1]));
    body.extend(chunk(b"out ", &[1, 85, 0, 0, 0, 0, 0, 25]));
    body.extend(chunk(b"tune", &[1; 24]));
    body.extend(chunk(b"lfo ", &vec![1; lfo_len]));
    body.extend(chunk(b"lfo ", &vec![1; lfo_len]));
    body.extend(chunk(b"mods", &[1; 38]));
    for kg in 0..keygroups {
        let mut kgrp = Vec::new();
        let mut kloc = [0u8; 16];
        kloc[4] = 21 + kg as u8;
        kloc[5] = 127;
        kgrp.extend(chunk(b"kloc", &kloc));
        for _ in 0..3 {
            kgrp.extend(chunk(b"env ", &[0; 18]));
        }
        kgrp.extend(chunk(b"filt", &[0; 10]));
        for zone in 0..4 {
            let mut z = [0x5Au8; 48]; // stale memory everywhere
            if zone == 0 {
                z[1] = 5;
                z[2..7].copy_from_slice(b"Kick1");
            } else {
                z[1] = 0;
            }
            z[36..48].fill(0);
            kgrp.extend(chunk(b"zone", &z));
        }
        body.extend(chunk(b"kgrp", &kgrp));
    }
    riff(b"APRG", 0, &body)
}

#[test]
fn program_round_trips_byte_for_byte() {
    let bytes = program_bytes(3, 14);
    let program = Program::parse(&bytes).unwrap();
    assert_eq!(program.to_bytes(), bytes);
    assert_eq!(program.keygroups().count(), 3);
}

#[test]
fn program_fields_read_and_write() {
    let mut program = Program::parse(&program_bytes(2, 14)).unwrap();
    assert_eq!(program.get(Section::Out, "loudness"), Some(85));
    assert_eq!(program.get(Section::Out, "velocity_sensitivity"), Some(25));

    program.set(Section::Tune, "semitone", -12).unwrap();
    program.set(Section::Lfo2, "clock_division", 68).unwrap();
    let kg = program.keygroup_mut(1).unwrap();
    kg.set(KeygroupSection::Filter, "type", 25).unwrap();
    kg.set(KeygroupSection::FilterEnv, "depth", -40).unwrap();
    kg.zone_set(0, "velocity_start", -9999).unwrap();

    let reread = Program::parse(&program.to_bytes()).unwrap();
    assert_eq!(reread.get(Section::Tune, "semitone"), Some(-12));
    assert_eq!(reread.get(Section::Lfo2, "clock_division"), Some(68));
    let kg = reread.keygroup(1).unwrap();
    assert_eq!(kg.get(KeygroupSection::Filter, "type"), Some(25));
    assert_eq!(kg.get(KeygroupSection::FilterEnv, "depth"), Some(-40));
    assert_eq!(kg.zone_get(0, "velocity_start"), Some(-9999));
    assert_eq!(kg.get(KeygroupSection::Kloc, "low_key"), Some(22));
}

#[test]
fn older_short_chunks_report_absent_fields() {
    let mut program = Program::parse(&program_bytes(1, 12)).unwrap();
    assert_eq!(program.get(Section::Lfo2, "clock_division"), None);
    assert!(program.set(Section::Lfo2, "clock_division", 3).is_err());
    assert_eq!(program.get(Section::Lfo2, "rate"), Some(1));
}

#[test]
fn zone_names_ignore_stale_bytes() {
    let mut program = Program::parse(&program_bytes(1, 14)).unwrap();
    let kg = program.keygroup(0).unwrap();
    assert_eq!(kg.zone_sample(0).as_deref(), Some("Kick1"));
    assert_eq!(kg.zone_sample(1), None);
    assert_eq!(program.sample_names(), vec!["Kick1".to_string()]);

    let kg = program.keygroup_mut(0).unwrap();
    kg.set_zone_sample(1, "Snare 2").unwrap();
    assert!(kg.set_zone_sample(2, "a name that is much too long").is_err());
    assert_eq!(kg.zone_sample(1).as_deref(), Some("Snare 2"));
}

#[test]
fn out_of_range_values_are_rejected() {
    let mut program = Program::parse(&program_bytes(1, 14)).unwrap();
    assert!(program.set(Section::Tune, "semitone", 200).is_err());
    assert!(program.set(Section::Tune, "no_such_field", 0).is_err());
}

fn multi_bytes(parts: usize) -> Vec<u8> {
    let mut body = chunk(b"fx  ", &[0; 530]);
    body.extend(chunk(b"mver", &[1, 0x20, 0, 0]));
    for part in 0..parts {
        let mut p = [0x33u8; 48];
        p[1] = 0;
        p[34] = part as u8;
        p[35..48].copy_from_slice(&[0, 21, 127, 100, 0, 0, 0, 0, 0, 1, 0, 0, 0]);
        body.extend(chunk(b"part", &p));
    }
    riff(b"AMUL", 0, &body)
}

#[test]
fn multi_round_trips_and_edits() {
    let bytes = multi_bytes(32);
    let mut multi = Multi::parse(&bytes).unwrap();
    assert_eq!(multi.to_bytes(), bytes);
    assert_eq!(multi.part_count(), 32);
    assert_eq!(multi.part_program(0), None);
    assert_eq!(multi.part_get(5, "midi_channel"), Some(5));
    assert_eq!(multi.part_get(0, "level"), Some(100));

    multi.set_part_program(0, "Pad").unwrap();
    multi.part_set(0, "pan", -50).unwrap();
    multi.part_set(31, "semitone", -36).unwrap();
    let reread = Multi::parse(&multi.to_bytes()).unwrap();
    assert_eq!(reread.part_program(0).as_deref(), Some("Pad"));
    assert_eq!(reread.part_get(0, "pan"), Some(-50));
    assert_eq!(reread.part_get(31, "semitone"), Some(-36));
    assert_eq!(reread.program_names(), vec!["Pad".to_string()]);
}

fn wav_bytes(frames: usize) -> Vec<u8> {
    let mut fmt = Vec::new();
    fmt.extend_from_slice(&1u16.to_le_bytes()); // PCM
    fmt.extend_from_slice(&2u16.to_le_bytes()); // stereo
    fmt.extend_from_slice(&44100u32.to_le_bytes());
    fmt.extend_from_slice(&(44100u32 * 4).to_le_bytes());
    fmt.extend_from_slice(&4u16.to_le_bytes());
    fmt.extend_from_slice(&16u16.to_le_bytes());
    let mut body = chunk(b"fmt ", &fmt);
    body.extend(chunk(b"data", &vec![0; frames * 4]));
    let size = body.len() as u32 + 4;
    riff(b"WAVE", size, &body)
}

#[test]
fn wav_without_akai_record_gains_one() {
    let mut wav = Wav::parse(&wav_bytes(1000)).unwrap();
    assert_eq!(wav.frames(), 1000);
    assert_eq!(wav.akai_info(), None);

    let info = AkaiInfo {
        root_note: 69,
        semitone: 5,
        fine: -7,
        loop_points: Some((100, 900)),
        play_start: 0,
        play_end: 1000,
    };
    wav.set_akai_info(&info);
    let reread = Wav::parse(&wav.to_bytes()).unwrap();
    assert_eq!(reread.akai_info(), Some(info));
    assert_eq!(reread.frames(), 1000);
}

#[test]
fn wav_without_loop_uses_the_samplers_marker() {
    let mut wav = Wav::parse(&wav_bytes(50)).unwrap();
    let info = AkaiInfo {
        root_note: 60,
        semitone: 0,
        fine: 0,
        loop_points: None,
        play_start: 0,
        play_end: 50,
    };
    wav.set_akai_info(&info);
    let bytes = wav.to_bytes();
    let reread = Wav::parse(&bytes).unwrap();
    assert_eq!(reread.akai_info(), Some(info));
    let marker = [0xFE, 0xFF, 0xFF, 0xFF, 0xF0, 0xFF, 0xFF, 0xFF];
    assert!(bytes.windows(8).any(|w| w == marker));
}

#[test]
fn wav_round_trips() {
    let bytes = wav_bytes(10);
    assert_eq!(Wav::parse(&bytes).unwrap().to_bytes(), bytes);
}

#[test]
fn blank_items_parse_back() {
    let p = Program::blank(3);
    let reread = Program::parse(&p.to_bytes()).unwrap();
    assert_eq!(reread.keygroups().count(), 3);
    assert_eq!(reread.get(Section::Out, "loudness"), Some(85));
    assert_eq!(reread.get(Section::Lfo2, "clock_division"), Some(0));
    assert!(reread.sample_names().is_empty());

    let m = Multi::blank(64);
    let reread = Multi::parse(&m.to_bytes()).unwrap();
    assert_eq!(reread.part_count(), 64);
    assert_eq!(reread.part_get(33, "midi_channel"), Some(1));
    assert_eq!(reread.part_program(0), None);
}
