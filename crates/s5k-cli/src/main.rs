//! `s5k`: command-line control of an Akai S5000/S6000 over USB.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use s5k::{ItemKind, Sampler};
use s5k_usb::UsbTransport;

mod png;

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
    /// Create an empty program or multi in memory.
    Create {
        kind: Kind,
        name: String,
        /// Keygroups (programs, 1–99) or parts (multis: 32, 64 or 128).
        #[arg(short, long)]
        size: Option<u16>,
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
    /// Show one item in detail: a sample's loop and tuning, a program's
    /// keygroups and number, a multi's assigned parts.
    Show { kind: Kind, name: String },
    /// Add keygroups to a program, or delete one.
    Keygroup {
        #[command(subcommand)]
        action: KeygroupAction,
    },
    /// Remove the program from multi parts (parts count from 0).
    ClearPart { multi: String, parts: Vec<u8> },
    /// Check the sampler answers, and that bytes come back unchanged.
    Ping,
    /// List the sampler's disks.
    Disks,
    /// Show level meters once.
    Meters,
    /// List, read or change parameters (see spec/params.toml).
    Param {
        #[command(subcommand)]
        action: ParamAction,
    },
    /// Save the sampler's screen as a PNG.
    Screen {
        #[arg(default_value = "screen.png")]
        file: PathBuf,
        /// Pixel scale.
        #[arg(short, long, default_value_t = 2)]
        scale: usize,
    },
    /// Press front-panel keys, e.g. `s5k press multi f1 f2`.
    Press { keys: Vec<String> },
    /// Delete everything in memory.
    Clear {
        #[arg(short = 'y', long)]
        yes: bool,
    },
}

#[derive(Subcommand)]
enum KeygroupAction {
    /// Add COUNT keygroups at the end of a program.
    Add { program: String, count: u8 },
    /// Delete keygroup KEYGROUP (from 1) of a program.
    Delete {
        program: String,
        keygroup: u8,
        #[arg(short = 'y', long)]
        yes: bool,
    },
}

#[derive(Subcommand)]
enum ParamAction {
    /// List parameter ids, optionally for one scope (program, keygroup, zone, multi_part, sample, fx, system, midi).
    List {
        scope: Option<String>,
        /// One JSON object per parameter (id, scope, label, range, value names, selectors).
        #[arg(long)]
        json: bool,
    },
    /// Read a parameter, e.g. `s5k param get program.loudness Pad`.
    Get {
        id: String,
        /// Program, multi or sample name (not needed for system/midi/fx).
        item: Option<String>,
        /// Keygroup number (from 1) for keygroup and zone parameters.
        #[arg(short, long, default_value_t = 1)]
        keygroup: u8,
        /// Selector values (LFO, zone, part, …), comma-separated.
        #[arg(short, long, value_delimiter = ',')]
        select: Vec<u8>,
        #[arg(long)]
        json: bool,
    },
    /// Change a parameter, e.g. `s5k param set keygroup.cutoff Pad 60 -k 2`.
    #[command(allow_negative_numbers = true)]
    Set {
        id: String,
        /// `[NAME] VALUE`: the program/multi/sample name (if needed), then the value.
        #[arg(num_args = 1..=2, required = true)]
        args: Vec<String>,
        #[arg(short, long, default_value_t = 1)]
        keygroup: u8,
        #[arg(short, long, value_delimiter = ',')]
        select: Vec<u8>,
    },
    /// Read every readable parameter of a scope in one connection, e.g.
    /// `s5k param read keygroup Pad -k 2`. A parameter with a selector is read for
    /// each of its values (LFO 1 and 2, zones 1-4, …) unless `-s` fixes them.
    Read {
        scope: String,
        item: Option<String>,
        #[arg(short, long, default_value_t = 1)]
        keygroup: u8,
        #[arg(short, long, value_delimiter = ',')]
        select: Vec<u8>,
        /// One JSON object per value instead of a table.
        #[arg(long)]
        json: bool,
    },
    /// Change several parameters of one item in one connection and read each back, e.g.
    /// `s5k param apply Pad keygroup.cutoff=60 zone.level@2=-6 -k 2`. `@` gives the
    /// selectors (comma-separated).
    #[command(allow_negative_numbers = true)]
    Apply {
        /// The program, multi or sample (omit for system/midi/fx parameters).
        #[arg(long)]
        item: Option<String>,
        /// `id[@selectors]=value`, one or more.
        #[arg(required = true)]
        assignments: Vec<String>,
        #[arg(short, long, default_value_t = 1)]
        keygroup: u8,
        #[arg(long)]
        json: bool,
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
        ItemKind::MidiFile => s.midi_file_names()?,
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
            let mut s = connect()?;
            let m = s.memory()?;
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
            for (title, names) in [
                ("MIDI files", s.midi_file_names()?),
                ("Set lists", s.setlist_names()?),
                ("Scenelists", s.scenelist_names()?),
            ] {
                if !names.is_empty() {
                    println!("{title} ({})", names.len());
                    for (i, n) in names.iter().enumerate() {
                        println!("  {i:3}  {n}");
                    }
                }
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
        Command::Create { kind, name, size } => {
            let mut s = connect()?;
            match kind {
                Kind::Program => s.create_program(&name, size.unwrap_or(1).clamp(1, 99) as u8)?,
                Kind::Multi => s.create_multi(&name, size.unwrap_or(32))?,
                Kind::Sample => return Err("samples are created by recording or sending a .wav".into()),
            }
            println!("created {name}");
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
        Command::Show { kind, name } => show(kind, &name)?,
        Command::Keygroup { action } => keygroup(action)?,
        Command::ClearPart { multi, parts } => {
            let mut s = connect()?;
            let all = s.multi_names()?;
            let (index, name) = resolve(&all, &[multi])?.remove(0);
            for part in parts {
                s.clear_part(index, part)?;
                println!("{name}: part {} cleared", part + 1);
            }
        }
        Command::Ping => {
            let mut s = connect()?;
            s.ping()?;
            let sent = [0x01, 0x2A, 0x55, 0x7F];
            let back = s.echo(sent)?;
            if back != sent {
                return Err(format!("echo came back as {back:02X?}").into());
            }
            println!("ok");
        }
        Command::Disks => {
            let mut s = connect()?;
            for d in s.disks()? {
                let ready = if s.check_disk(d.handle)? {
                    ""
                } else {
                    "  (not ready)"
                };
                let rw = if d.writable { "" } else { ", read-only" };
                println!("  {:5}  {:<20} {:?}{rw}{ready}", d.handle, d.name, d.kind);
            }
        }
        Command::Meters => {
            let m = connect()?.meters()?;
            let show = |raw: u8| s5k_protocol_db(raw);
            println!("inputs   L {}  R {}", show(m.inputs[0]), show(m.inputs[1]));
            for (i, o) in m.outputs.iter().enumerate() {
                println!("output {:2}  {}", i + 1, show(*o));
            }
        }
        Command::Param { action } => param(action)?,
        Command::Screen { file, scale } => {
            let screen = connect()?.screen()?;
            std::fs::write(&file, png::encode(&screen, scale.clamp(1, 8)))?;
            println!("{}", file.display());
        }
        Command::Press { keys } => {
            let keys = keys.iter().map(|k| parse_key(k)).collect::<Res<Vec<_>>>()?;
            let mut s = connect()?;
            for key in keys {
                s.tap(key)?;
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

fn parse_key(name: &str) -> Res<s5k::Key> {
    use s5k::Key;
    let n = name.to_ascii_lowercase().replace(['-', '_', ' ', '/'], "");
    if let Some(f) = n.strip_prefix('f').and_then(|d| d.parse::<u8>().ok()) {
        return Key::function(f).ok_or_else(|| format!("no key F{f}").into());
    }
    if let Ok(d) = n.parse::<u8>() {
        return Key::from_code(0x58 + d)
            .filter(|_| d <= 9)
            .ok_or_else(|| format!("no key {d}").into());
    }
    Ok(match n.as_str() {
        "fx" => Key::Fx,
        "record" => Key::Record,
        "editsample" | "sample" => Key::EditSample,
        "editprogram" | "program" => Key::EditProgram,
        "multi" => Key::Multi,
        "utilities" | "utility" => Key::Utilities,
        "save" => Key::Save,
        "load" => Key::Load,
        "plus" | "+" => Key::Plus,
        "minus" | "-" => Key::Minus,
        "left" => Key::CursorLeft,
        "right" => Key::CursorRight,
        "window" => Key::Window,
        "mark" => Key::Mark,
        "jump" => Key::Jump,
        "exit" => Key::Exit,
        "ent" | "play" | "entplay" | "enter" => Key::EntPlay,
        "user1" => Key::User1,
        "user2" => Key::User2,
        "user3" => Key::User3,
        _ => return Err(format!("unknown key '{name}'").into()),
    })
}

fn s5k_protocol_db(raw: u8) -> String {
    if raw >= 0x60 {
        "   -".into()
    } else {
        format!("{:>4} dB", -(raw as i32))
    }
}

/// MIDI note as the sampler shows it: 21 = A-1, 60 = C3.
fn note_name(n: u8) -> String {
    const NAMES: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
    format!("{}{}", NAMES[n as usize % 12], n as i32 / 12 - 2)
}

const PLAYBACK: [&str; 6] = [
    "no looping",
    "one shot",
    "loop in release",
    "loop until release",
    "LIR > retrig",
    "play > retrig",
];

fn show(kind: Kind, name: &str) -> Res<()> {
    let kind_ = ItemKind::from(kind);
    let mut s = connect()?;
    let all = names_of(&mut s, kind_)?;
    let (index, name) = resolve(&all, &[name.to_string()])?.remove(0);
    println!("{name}");
    match kind {
        Kind::Sample => {
            let f = s.sample_format(index)?;
            let p = s.sample_playback(index)?;
            let ch = if f.channels == 2 { "stereo" } else { "mono" };
            let ram = if f.is_virtual { "virtual" } else { "RAM" };
            println!("  {} frames, {} Hz {ch}, {ram}", f.frames, f.sample_rate);
            println!("  play     {} – {}", p.start, p.end);
            println!(
                "  root     {} ({}), semitone {:+}, fine {:+}",
                note_name(p.root_note),
                p.root_note,
                p.semitone,
                p.fine
            );
            let mode = PLAYBACK.get(p.playback as usize).copied().unwrap_or("?");
            println!("  playback {mode}");
            match p.loop_points {
                Some((a, b)) => println!("  loop     {a} – {b}"),
                None => println!("  loop     none"),
            }
        }
        Kind::Program => {
            println!("  {} keygroups", s.program_keygroups(index)?);
            match s.program_numbers()?.get(index as usize).copied().flatten() {
                Some(n) => println!("  program number {n}"),
                None => println!("  program number off"),
            }
        }
        Kind::Multi => {
            let parts = s.multi_assigned_parts(index)?;
            println!("  {} parts, {} assigned", s.multi_parts(index)?, parts.len());
            for p in parts {
                let c = &p.settings;
                let channel = format!(
                    "{}{}",
                    c.midi_channel % 16 + 1,
                    if c.midi_channel < 16 { 'A' } else { 'B' }
                );
                let state = match p.state {
                    s5k::PartState::Normal => "",
                    s5k::PartState::Muted => "  muted",
                    s5k::PartState::Solo => "  solo",
                };
                println!(
                    "  {:3}  {:<20} ch {channel:<3} level {:3}  pan {:+3}  {}–{}{state}",
                    p.part + 1,
                    p.program,
                    c.level,
                    c.pan,
                    note_name(c.low_note),
                    note_name(c.high_note),
                );
            }
        }
    }
    Ok(())
}

fn keygroup(action: KeygroupAction) -> Res<()> {
    let mut s = connect()?;
    let all = s.program_names()?;
    match action {
        KeygroupAction::Add { program, count } => {
            let (index, name) = resolve(&all, &[program])?.remove(0);
            s.add_keygroups(index, count)?;
            println!("{name}: {} keygroups", s.program_keygroups(index)?);
        }
        KeygroupAction::Delete {
            program,
            keygroup,
            yes,
        } => {
            let (index, name) = resolve(&all, &[program])?.remove(0);
            if !yes && !confirm(&format!("Delete keygroup {keygroup} of {name}?")) {
                return Ok(());
            }
            s.delete_keygroup(index, keygroup)?;
            println!("{name}: {} keygroups", s.program_keygroups(index)?);
        }
    }
    Ok(())
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

fn param(action: ParamAction) -> Res<()> {
    let cat = s5k::catalog();
    match action {
        ParamAction::List { scope, json } => {
            for p in &cat.params {
                if scope.as_deref().is_none_or(|s| s == p.scope) {
                    if json {
                        println!("{}", param_json(cat, p));
                        continue;
                    }
                    let range = match (p.min, p.max) {
                        (Some(a), Some(b)) => format!("{a}..{b}"),
                        _ => String::new(),
                    };
                    println!("{:<40} {:<28} {}", p.id, p.label, range);
                }
            }
            Ok(())
        }
        ParamAction::Get {
            id,
            item,
            keygroup,
            select,
            json,
        } => {
            let p = find_param(&id)?;
            let mut s = connect()?;
            let target = param_target(&mut s, p, item.as_deref(), keygroup)?;
            let v = s.read_param(p, target, &select)?;
            if json {
                println!("{}", value_json(cat, p, &select, Ok(&v)));
            } else {
                println!("{} = {}", p.id, shown(cat, p, &v));
            }
            Ok(())
        }
        ParamAction::Set {
            id,
            mut args,
            keygroup,
            select,
        } => {
            let p = find_param(&id)?;
            let text = args.pop().ok_or("give a value")?;
            let item = args.pop();
            let mut s = connect()?;
            let target = param_target(&mut s, p, item.as_deref(), keygroup)?;
            let v = parse_value(cat, p, &text)?;
            s.write_param(p, target, &select, &v)?;
            println!("{} := {v}", p.id);
            Ok(())
        }
        ParamAction::Read {
            scope,
            item,
            keygroup,
            select,
            json,
        } => {
            let params: Vec<_> = cat
                .scope(&scope)
                .filter(|p| p.get_item.is_some() && p.encoding.is_some())
                .collect();
            if params.is_empty() {
                return Err(format!("no readable parameters in scope '{scope}'").into());
            }
            let mut s = connect()?;
            let target = param_target(&mut s, params[0], item.as_deref(), keygroup)?;
            // A module type's own parameters apply only where a module of that type is
            // loaded (spec/README.md): learn each slot's type first and read only those.
            let mut slot_types = std::collections::HashMap::new();
            let module_type = cat.get("fx.module_type").filter(|_| scope == "fx");
            for sel in module_type.map(|mt| selector_sets(mt, &[])).unwrap_or_default() {
                let mt = module_type.expect("only listed with a module type");
                if let Ok(s5k::params::Value::Number(t)) = s.read_param(mt, target, &sel) {
                    slot_types.insert(sel, t);
                }
            }
            for p in params {
                for sel in selector_sets(p, &select) {
                    // A (channel, module) slot the board does not have, or a module
                    // type's parameter where another type is loaded, is skipped.
                    let absent = scope == "fx" && sel.len() == 2 && !slot_types.contains_key(&sel);
                    let other_type = p
                        .fx_module_type
                        .is_some_and(|t| slot_types.get(&sel) != Some(&i32::from(t)));
                    if absent || other_type {
                        continue;
                    }
                    let v = s.read_param(p, target, &sel);
                    if json {
                        println!(
                            "{}",
                            value_json(cat, p, &sel, v.as_ref().map_err(|e| e.to_string()))
                        );
                    } else {
                        let at = if sel.is_empty() {
                            String::new()
                        } else {
                            format!("@{}", join(&sel))
                        };
                        match v {
                            Ok(v) => println!("{:<40} {}", format!("{}{at}", p.id), shown(cat, p, &v)),
                            Err(e) => println!("{:<40} ({e})", format!("{}{at}", p.id)),
                        }
                    }
                }
            }
            Ok(())
        }
        ParamAction::Apply {
            item,
            assignments,
            keygroup,
            json,
        } => {
            let mut planned = Vec::new();
            for a in &assignments {
                let (lhs, text) = a
                    .split_once('=')
                    .ok_or_else(|| format!("'{a}' is not id=value"))?;
                let (id, sel) = match lhs.split_once('@') {
                    Some((id, sel)) => (
                        id,
                        sel.split(',')
                            .map(str::parse::<u8>)
                            .collect::<Result<Vec<_>, _>>()?,
                    ),
                    None => (lhs, Vec::new()),
                };
                let p = find_param(id)?;
                planned.push((p, sel, parse_value(cat, p, text)?));
            }
            let mut s = connect()?;
            for (p, sel, v) in planned {
                let target = param_target(&mut s, p, item.as_deref(), keygroup)?;
                s.write_param(p, target, &sel, &v)?;
                let back = s.read_param(p, target, &sel);
                if json {
                    println!(
                        "{}",
                        value_json(cat, p, &sel, back.as_ref().map_err(|e| e.to_string()))
                    );
                } else {
                    match back {
                        Ok(b) => println!("{} := {}", p.id, shown(cat, p, &b)),
                        Err(e) => println!("{} := {v} (not read back: {e})", p.id),
                    }
                }
            }
            Ok(())
        }
    }
}

fn find_param(id: &str) -> Res<&'static s5k::params::Param> {
    s5k::catalog()
        .get(id)
        .ok_or_else(|| format!("no parameter '{id}' (try `s5k param list`)").into())
}

fn param_target(
    s: &mut Sampler<UsbTransport>,
    p: &s5k::params::Param,
    item: Option<&str>,
    keygroup: u8,
) -> Res<s5k::params::Target> {
    use s5k::params::Target;
    let mut index_of = |kind: ItemKind| -> Res<u16> {
        let name = item.ok_or("name the program, multi or sample")?;
        let all = names_of(s, kind)?;
        Ok(resolve(&all, &[name.to_string()])?[0].0)
    };
    Ok(match p.scope.as_str() {
        "system" | "midi" => Target::Global,
        "fx" => Target::Effects,
        "program" => Target::Program(index_of(ItemKind::Program)?),
        "keygroup" | "zone" => Target::Keygroup {
            program: index_of(ItemKind::Program)?,
            keygroup,
        },
        "multi" | "multi_part" => Target::Multi(index_of(ItemKind::Multi)?),
        "sample" => Target::Sample(index_of(ItemKind::Sample)?),
        other => return Err(format!("unknown scope {other}").into()),
    })
}

fn parse_value(cat: &s5k::params::Catalog, p: &s5k::params::Param, text: &str) -> Res<s5k::params::Value> {
    use s5k::params::Value;
    Ok(if p.encoding.as_deref() == Some("string") {
        Value::Text(text.to_string())
    } else if let Some(i) = cat
        .values(p)
        .and_then(|l| l.iter().position(|n| n.eq_ignore_ascii_case(text)))
    {
        Value::Number(i as i32)
    } else {
        Value::Number(
            text.parse()
                .map_err(|_| format!("'{text}' is not a number or a known value"))?,
        )
    })
}

fn shown(cat: &s5k::params::Catalog, p: &s5k::params::Param, v: &s5k::params::Value) -> String {
    // A value from a list is shown as the list spells it, units and all ("0 dB").
    let named = match (v, cat.values(p)) {
        (s5k::params::Value::Number(n), Some(list)) => list.get(*n as usize),
        _ => None,
    };
    if let Some(name) = named {
        return name.clone();
    }
    format!(
        "{v}{}",
        p.units.as_ref().map(|u| format!(" {u}")).unwrap_or_default()
    )
}

#[cfg(test)]
mod tests {
    use super::selector_sets;

    #[test]
    fn a_selector_is_read_for_each_value_unless_fixed() {
        let cat = s5k::catalog();
        let zone = cat.get("zone.level").unwrap();
        assert_eq!(selector_sets(zone, &[]), vec![vec![1], vec![2], vec![3], vec![4]]);
        assert_eq!(selector_sets(zone, &[2]), vec![vec![2]]);
        let plain = cat.get("keygroup.cutoff").unwrap();
        assert_eq!(selector_sets(plain, &[]), vec![Vec::<u8>::new()]);
        assert_eq!(selector_sets(plain, &[1]), vec![Vec::<u8>::new()]);
    }
}

/// The selector combinations a parameter is read for: `fixed` if given (and it has a
/// selector), else every value of each of its selectors.
fn selector_sets(p: &s5k::params::Param, fixed: &[u8]) -> Vec<Vec<u8>> {
    use s5k::params::Index;
    let specs = match &p.index {
        None => return vec![Vec::new()], // a fixed selector does not apply to it
        Some(Index::One(i)) => vec![i.clone()],
        Some(Index::Many(v)) => v.clone(),
    };
    if !fixed.is_empty() {
        return if fixed.len() == specs.len() {
            vec![fixed.to_vec()]
        } else {
            Vec::new()
        };
    }
    let mut sets = vec![Vec::new()];
    for spec in specs {
        sets = sets
            .into_iter()
            .flat_map(|prefix: Vec<u8>| {
                (spec.min..=spec.max).map(move |v| {
                    let mut next = prefix.clone();
                    next.push(v as u8);
                    next
                })
            })
            .collect();
    }
    sets
}

fn join(sel: &[u8]) -> String {
    sel.iter().map(u8::to_string).collect::<Vec<_>>().join(",")
}

fn param_json(cat: &s5k::params::Catalog, p: &s5k::params::Param) -> serde_json::Value {
    use s5k::params::Index;
    let index = match &p.index {
        None => Vec::new(),
        Some(Index::One(i)) => vec![i.clone()],
        Some(Index::Many(v)) => v.clone(),
    };
    serde_json::json!({
        "id": p.id, "scope": p.scope, "label": p.label, "min": p.min, "max": p.max, "units": p.units,
        "values": cat.values(p), "encoding": p.encoding,
        "index": index.iter().map(|i| serde_json::json!({"name": i.name, "min": i.min, "max": i.max})).collect::<Vec<_>>(),
        "readable": p.get_item.is_some(), "writable": p.set_item.is_some(),
    })
}

fn value_json(
    cat: &s5k::params::Catalog,
    p: &s5k::params::Param,
    sel: &[u8],
    v: Result<&s5k::params::Value, String>,
) -> serde_json::Value {
    use s5k::params::Value;
    match v {
        Ok(v) => {
            let value = match v {
                Value::Number(n) => serde_json::json!(n),
                Value::Text(t) => serde_json::json!(t),
            };
            serde_json::json!({"id": p.id, "select": sel, "value": value, "shown": shown(cat, p, v)})
        }
        Err(e) => serde_json::json!({"id": p.id, "select": sel, "error": e}),
    }
}
