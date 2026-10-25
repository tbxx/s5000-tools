//! Checks every `.akp`, `.akm` and `.wav` under a folder of real files.
//!
//! Skipped by default. Run with:
//!   S5K_LIBRARY=/path/to/files cargo test -p s5k-formats --test library -- --ignored --nocapture

use std::path::{Path, PathBuf};

use s5k_formats::{KeygroupSection, Multi, Program, Wav};

fn files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            files(&path, out);
        } else {
            out.push(path);
        }
    }
}

fn extension(path: &Path) -> String {
    path.extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

#[test]
#[ignore]
fn library_files_parse_and_round_trip() {
    let root = std::env::var("S5K_LIBRARY").expect("set S5K_LIBRARY to a folder of S5000 files");
    let mut paths = Vec::new();
    files(Path::new(&root), &mut paths);

    let (mut programs, mut multis, mut wavs, mut akai_wavs) = (0, 0, 0, 0);
    let mut failures = Vec::new();
    for path in paths {
        let ext = extension(&path);
        if !["akp", "akm", "wav"].contains(&ext.as_str()) {
            continue;
        }
        let bytes = std::fs::read(&path).unwrap();
        let result: Result<(), String> = match ext.as_str() {
            "akp" => Program::parse(&bytes).map_err(|e| e.to_string()).and_then(|p| {
                programs += 1;
                for kg in p.keygroups() {
                    let (lo, hi) = (
                        kg.get(KeygroupSection::Kloc, "low_key"),
                        kg.get(KeygroupSection::Kloc, "high_key"),
                    );
                    if let (Some(lo), Some(hi)) = (lo, hi)
                        && (lo > hi || hi > 127)
                    {
                        return Err(format!("key range {lo}..{hi}"));
                    }
                }
                if p.sample_names()
                    .iter()
                    .any(|n| n.len() > 34 || n.chars().any(|c| c.is_control()))
                {
                    return Err("implausible sample name".into());
                }
                (p.to_bytes() == bytes)
                    .then_some(())
                    .ok_or("round trip differs".into())
            }),
            "akm" => Multi::parse(&bytes).map_err(|e| e.to_string()).and_then(|m| {
                multis += 1;
                (m.to_bytes() == bytes)
                    .then_some(())
                    .ok_or("round trip differs".into())
            }),
            _ => match Wav::parse(&bytes) {
                // Plenty of .wav files aren't valid RIFF; only count the ones that are.
                Err(_) => Ok(()),
                Ok(w) => {
                    wavs += 1;
                    match w.akai_info() {
                        Some(info)
                            if info.play_end > w.frames() + 1
                                || info.loop_points.is_some_and(|(_, end)| end > w.frames() + 1) =>
                        {
                            akai_wavs += 1;
                            Err(format!("loop/play beyond {} frames: {info:?}", w.frames()))
                        }
                        Some(_) => {
                            akai_wavs += 1;
                            Ok(())
                        }
                        None => Ok(()),
                    }
                }
            },
        };
        if let Err(e) = result {
            failures.push((path, e));
        }
    }

    println!("programs {programs}, multis {multis}, wavs {wavs} ({akai_wavs} with Akai data)");
    for (path, e) in failures.iter().take(60) {
        println!("FAIL {}: {e}", path.display());
    }
    assert!(failures.is_empty(), "{} files failed", failures.len());
}
