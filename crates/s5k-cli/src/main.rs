//! `s5k`: command-line control of an Akai S5000/S6000 over USB.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use s5k::{ItemKind, Sampler};
use s5k_usb::UsbTransport;

#[derive(Parser)]
#[command(name = "s5k", version, about = "Control an Akai S5000/S6000 over USB")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Clone, Copy, ValueEnum)]
enum Kind {
    Sample,
    Program,
    Multi,
}

impl From<Kind> for ItemKind {
    fn from(k: Kind) -> Self {
        match k {
            Kind::Sample => ItemKind::Sample,
            Kind::Program => ItemKind::Program,
            Kind::Multi => ItemKind::Multi,
        }
    }
}

#[derive(Subcommand)]
enum Command {
    /// List connected samplers.
    Devices,
    /// Show the sampler's name, OS version and free memory.
    Info,
    /// List samples, programs and multis in memory.
    List,
    /// Copy items from memory to files. NAME is an item name or index.
    Get {
        kind: Kind,
        /// Names or indexes; omit to get every item of this kind.
        names: Vec<String>,
        /// Folder to write into.
        #[arg(short, long, default_value = ".")]
        out: PathBuf,
        /// Also get what the items use: a program's samples, a multi's programs and samples.
        #[arg(short, long)]
        deps: bool,
        /// Replace files that already exist.
        #[arg(short, long)]
        force: bool,
    },
    /// Send files (.wav, .akp, .akm) into memory.
    Put {
        files: Vec<PathBuf>,
        /// For programs, first send samples the program uses that are not in
        /// memory, looking for NAME.wav next to the program.
        #[arg(short, long)]
        deps: bool,
    },
    /// Delete items from memory.
    Delete {
        kind: Kind,
        names: Vec<String>,
        /// Delete without asking.
        #[arg(short = 'y', long)]
        yes: bool,
    },
    /// Rename an item in memory.
    Rename {
        kind: Kind,
        name: String,
        new_name: String,
    },
    /// Play a sample on the sampler's outputs (Ctrl-C does not stop it; use `stop`).
    Play { name: String },
    /// Stop playing a sample.
    Stop { name: String },
    /// Show level meters once.
    Meters,
    /// Delete everything in memory.
    Clear {
        #[arg(short = 'y', long)]
        yes: bool,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli.command) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("s5k: {e}");
            ExitCode::FAILURE
        }
    }
}

type Res<T> = Result<T, Box<dyn std::error::Error>>;

fn connect() -> Res<Sampler<UsbTransport>> {
    Ok(Sampler::connect(UsbTransport::open_first()?)?)
}

fn names_of(s: &mut Sampler<UsbTransport>, kind: ItemKind) -> Res<Vec<String>> {
    Ok(match kind {
        ItemKind::Sample => s.sample_names()?,
        ItemKind::Program => s.program_names()?,
        ItemKind::Multi => s.multi_names()?,
        ItemKind::MidiFile => Vec::new(),
    })
}

/// Resolves names or indexes to (index, name) pairs; empty means all.
fn resolve(all: &[String], wanted: &[String]) -> Res<Vec<(u16, String)>> {
    if wanted.is_empty() {
        return Ok(all
            .iter()
            .cloned()
            .enumerate()
            .map(|(i, n)| (i as u16, n))
            .collect());
    }
    wanted
        .iter()
        .map(|w| {
            let i = all
                .iter()
                .position(|n| n == w)
                .or_else(|| all.iter().position(|n| n.eq_ignore_ascii_case(w)))
                .or_else(|| w.parse::<usize>().ok().filter(|&i| i < all.len()))
                .ok_or_else(|| format!("nothing called '{w}' in memory"))?;
            Ok((i as u16, all[i].clone()))
        })
        .collect()
}

fn confirm(question: &str) -> bool {
    eprint!("{question} [y/N] ");
    let mut answer = String::new();
    std::io::stdin().read_line(&mut answer).is_ok() && answer.trim().eq_ignore_ascii_case("y")
}

fn write_file(path: &Path, bytes: &[u8], force: bool) -> Res<()> {
    if path.exists() && !force {
        return Err(format!("{} exists (use --force to replace it)", path.display()).into());
    }
    std::fs::write(path, bytes)?;
    Ok(())
}

fn kib(bytes: u64) -> String {
    if bytes >= 1 << 20 {
        format!("{:.1} MB", bytes as f64 / (1u64 << 20) as f64)
    } else {
        format!("{:.0} KB", (bytes as f64 / 1024.0).ceil())
    }
}

fn run(command: Command) -> Res<()> {
    match command {
        Command::Devices => {
            let devices = s5k_usb::list()?;
            if devices.is_empty() {
                println!("no sampler found");
            }
            for d in devices {
                println!("{d}");
            }
        }
        Command::Info => {
            let i = connect()?.info()?;
            println!("{}  OS {}.{:02}", i.name, i.os_version.0, i.os_version.1);
            println!(
                "wave memory {}% free, CPU memory {}% free",
                i.wave_free_percent, i.cpu_free_percent
            );
        }
        Command::List => {
            let m = connect()?.memory()?;
            println!("Samples ({})", m.samples.len());
            for s in &m.samples {
                let secs = s.frames as f64 / s.sample_rate.max(1) as f64;
                let ch = if s.channels == 2 { "stereo" } else { "mono" };
                println!(
                    "  {:3}  {:<20} {:>7.2} s  {} Hz {}",
                    s.index, s.name, secs, s.sample_rate, ch
                );
            }
            println!("Programs ({})", m.programs.len());
            for p in &m.programs {
                println!("  {:3}  {:<20} {} keygroups", p.index, p.name, p.keygroups);
            }
            println!("Multis ({})", m.multis.len());
            for x in &m.multis {
                println!("  {:3}  {:<20} {} parts", x.index, x.name, x.parts);
            }
        }
        Command::Get {
            kind,
            names,
            out,
            deps,
            force,
        } => {
            let kind = ItemKind::from(kind);
            let mut s = connect()?;
            std::fs::create_dir_all(&out)?;
            let all = names_of(&mut s, kind)?;
            for (index, name) in resolve(&all, &names)? {
                if deps && kind != ItemKind::Sample {
                    let bundle = if kind == ItemKind::Program {
                        s.get_program_bundle(index, &mut |n| eprintln!("  getting {n}"))?
                    } else {
                        s.get_multi_bundle(index, &mut |n| eprintln!("  getting {n}"))?
                    };
                    for (file, bytes) in &bundle.files {
                        write_file(&out.join(file), bytes, force)?;
                    }
                    for m in &bundle.missing {
                        eprintln!("  warning: '{m}' is used by {name} but not in memory");
                    }
                    println!("{name}: {} files", bundle.files.len());
                } else {
                    let path = out.join(format!("{name}.{}", kind.extension()));
                    let bytes = s.get(kind, index, &mut |done, _| {
                        eprint!("\r  {name}: {}   ", kib(done))
                    })?;
                    eprintln!();
                    write_file(&path, &bytes, force)?;
                    println!("{}", path.display());
                }
            }
        }
        Command::Put { files, deps } => {
            let mut s = connect()?;
            for path in files {
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                let kind = ItemKind::from_extension(ext)
                    .ok_or_else(|| format!("{}: not a .wav, .akp or .akm file", path.display()))?;
                let file_name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .ok_or("bad file name")?
                    .to_string();
                let bytes = std::fs::read(&path)?;
                if deps && kind == ItemKind::Program {
                    let folder = path.parent().unwrap_or(Path::new(".")).to_path_buf();
                    let missing = s.put_program_bundle(
                        &file_name,
                        &bytes,
                        &mut |sample| find_wav(&folder, sample),
                        &mut |n| eprintln!("  sending {n}"),
                    )?;
                    for m in missing {
                        eprintln!("  warning: no {m}.wav found next to {file_name}");
                    }
                } else {
                    let total = bytes.len() as u64;
                    s.put(kind, &file_name, &bytes, &mut |done, _| {
                        eprint!("\r  {file_name}: {} of {}   ", kib(done), kib(total))
                    })?;
                    eprintln!();
                }
                println!("sent {file_name}");
            }
        }
        Command::Delete { kind, names, yes } => {
            if names.is_empty() {
                return Err("name at least one item (use `clear` to delete everything)".into());
            }
            let kind = ItemKind::from(kind);
            let mut s = connect()?;
            let all = names_of(&mut s, kind)?;
            let mut targets = resolve(&all, &names)?;
            if !yes && !confirm(&format!("Delete {} item(s) from memory?", targets.len())) {
                return Ok(());
            }
            // Delete from the highest index down so earlier indexes stay valid.
            targets.sort_by_key(|t| std::cmp::Reverse(t.0));
            for (index, name) in targets {
                s.delete(kind, index)?;
                println!("deleted {name}");
            }
        }
        Command::Rename { kind, name, new_name } => {
            let kind = ItemKind::from(kind);
            let mut s = connect()?;
            let all = names_of(&mut s, kind)?;
            let (index, old) = resolve(&all, &[name])?.remove(0);
            s.rename(kind, index, &new_name)?;
            println!("{old} -> {new_name}");
        }
        Command::Play { name } => audition(&name, true)?,
        Command::Stop { name } => audition(&name, false)?,
        Command::Meters => {
            let m = connect()?.meters()?;
            let show = |raw: u8| s5k_protocol_db(raw);
            println!("inputs   L {}  R {}", show(m.inputs[0]), show(m.inputs[1]));
            for (i, o) in m.outputs.iter().enumerate() {
                println!("output {:2}  {}", i + 1, show(*o));
            }
        }
        Command::Clear { yes } => {
            if !yes && !confirm("Delete EVERYTHING in the sampler's memory?") {
                return Ok(());
            }
            connect()?.clear_memory()?;
            println!("memory cleared");
        }
    }
    Ok(())
}

fn s5k_protocol_db(raw: u8) -> String {
    if raw >= 0x60 {
        "   -".into()
    } else {
        format!("{:>4} dB", -(raw as i32))
    }
}

fn audition(name: &str, play: bool) -> Res<()> {
    let mut s = connect()?;
    let all = s.sample_names()?;
    let (index, _) = resolve(&all, &[name.to_string()])?.remove(0);
    s.audition(index, play)?;
    Ok(())
}

/// Finds `<sample>.wav` in a folder, ignoring case.
fn find_wav(folder: &Path, sample: &str) -> Option<Vec<u8>> {
    let wanted = format!("{sample}.wav").to_ascii_lowercase();
    std::fs::read_dir(folder)
        .ok()?
        .flatten()
        .find(|e| e.file_name().to_string_lossy().to_ascii_lowercase() == wanted)
        .and_then(|e| std::fs::read(e.path()).ok())
}
