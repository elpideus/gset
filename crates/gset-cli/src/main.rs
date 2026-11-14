//! gset: the launcher Steam runs. In a game's Steam launch options:
//!
//!     /home/you/.local/bin/gset %command%
//!
//! It finds the game, picks the preset for the screen that's on, builds the command from the game's
//! settings and replaces itself with it: nothing of GSet keeps running while you play.

use gset_core::config::{self, AppConfig, GameConfig};
use gset_core::fixes::{self, FixContext};
use gset_core::plan::{self, Host, Plan};
use gset_core::resolve::{self, PresetWhy};
use gset_core::schema::Schema;
use gset_core::{Paths, Profiles, import, profile, screen};
use std::io::{Read, Write};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::time::{Duration, Instant};

const HELP: &str = "\
gset: game launch settings

  gset [options] COMMAND…    start a game; in Steam's launch options: gset %command%
      --game=NAME            use the game file NAME (for what Steam doesn't start, like Heroic)
      --preset=NAME          use this preset whatever the screen (--preset=none: no preset)
      --dry-run              print what would run, and change nothing

  gset --import[=DIR]        bring games over from ~/.config/launch-options (the old scripts)
      --force                replace games GSet already has
      --dry-run              print what would be written
  gset --list                the games GSet knows
  gset --screens             the screens that are on, and the preset each game would use
  gset --check-profiles=DIR  check a folder of game profiles as the profiles repository does
  gset --bundle-profiles=DIR print the bundle of a folder of profiles (JSON), as GSet downloads it
  gset --version

Settings: ~/.config/gset/ (gset.toml, games/*.toml). Log: ~/.local/state/gset/launch.log
";

#[derive(Default)]
struct Opts {
    game: Option<String>,
    preset: Option<String>,
    dry_run: bool,
    force: bool,
    import: Option<Option<String>>,
    list: bool,
    screens: bool,
    check_profiles: Option<String>,
    bundle_profiles: Option<String>,
    help: bool,
    version: bool,
}

fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut o = Opts::default();
    // our options come first; the game's command starts at the first word that isn't one
    while let Some(a) = args.first().cloned() {
        match a.as_str() {
            "--" => {
                args.remove(0);
                break;
            }
            "--dry-run" => o.dry_run = true,
            "--force" => o.force = true,
            "--list" => o.list = true,
            "--screens" => o.screens = true,
            "--help" | "-h" => o.help = true,
            "--version" | "-V" => o.version = true,
            "--import" => o.import = Some(None),
            _ if a.starts_with("--import=") => {
                o.import = Some(Some(a["--import=".len()..].to_string()))
            }
            _ if a.starts_with("--game=") => {
                o.game = Some(a["--game=".len()..].trim_end_matches(".toml").to_string())
            }
            _ if a.starts_with("--preset=") => o.preset = Some(a["--preset=".len()..].to_string()),
            _ if a.starts_with("--check-profiles=") => {
                o.check_profiles = Some(a["--check-profiles=".len()..].to_string())
            }
            _ if a.starts_with("--bundle-profiles=") => {
                o.bundle_profiles = Some(a["--bundle-profiles=".len()..].to_string())
            }
            _ => break,
        }
        args.remove(0);
    }
    let paths = Paths::new();
    if o.help {
        print!("{HELP}");
        return ExitCode::SUCCESS;
    }
    if o.version {
        println!("gset {}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }
    if let Some(dir) = o.import {
        return run_import(&paths, dir, o.dry_run, o.force);
    }
    if o.list {
        return list(&paths);
    }
    if o.screens {
        return screens(&paths);
    }
    if let Some(dir) = &o.check_profiles {
        let (ok, problems) = gset_core::profile::check_dir(&paths.expand(dir));
        for p in &problems {
            eprintln!("{p}");
        }
        if problems.is_empty() {
            let n = ok.len();
            println!("{n} profile{}, all good.", if n == 1 { "" } else { "s" });
            return ExitCode::SUCCESS;
        }
        eprintln!(
            "{} problems in {} profiles.",
            problems.len(),
            ok.len() + problems.len()
        );
        return ExitCode::from(1);
    }
    if let Some(dir) = &o.bundle_profiles {
        return match gset_core::profile::make_bundle(&paths.expand(dir)) {
            Ok(json) => {
                println!("{json}");
                ExitCode::SUCCESS
            }
            Err(problems) => {
                for p in problems {
                    eprintln!("{p}");
                }
                ExitCode::from(1)
            }
        };
    }
    launch(&paths, &o, args)
}

// ------------------------------------------------------------------------------------------ launch

fn launch(paths: &Paths, o: &Opts, args: Vec<String>) -> ExitCode {
    if args.is_empty() && o.game.is_none() {
        eprint!("{HELP}");
        return ExitCode::from(2);
    }
    let app = match AppConfig::load(paths) {
        Ok(a) => a,
        Err(e) => {
            return start_as_is(
                paths,
                o,
                &args,
                &format!("GSet's settings can't be read ({e}): starting the game without them."),
            );
        }
    };
    let profiles = Profiles::load(&profile::dirs(paths));
    let found = match gset_core::find_or_create(
        paths,
        &app,
        &profiles,
        &args,
        |k| std::env::var(k).ok(),
        o.game.as_deref(),
        o.dry_run,
    ) {
        Ok(Some(f)) => f,
        Ok(None) => {
            return start_as_is(
                paths,
                o,
                &args,
                "Steam didn't say which game this is: starting it without settings.",
            );
        }
        Err(e) => {
            return start_as_is(
                paths,
                o,
                &args,
                &format!("{e}: starting the game without its settings."),
            );
        }
    };
    for w in &found.warnings {
        warn(paths, o.dry_run, "GSet", w);
    }
    let game = found.game;
    if args.is_empty() {
        return start_as_is(
            paths,
            o,
            &args,
            &format!(
                "Nothing to start for {}. Steam's launch option is: gset %command%",
                game.info.name
            ),
        );
    }

    // a game that a script of yours starts: the script runs instead, as before
    if let Some(script) = &game.info.script {
        let script = paths.expand(script);
        if !is_executable(&script) {
            return start_as_is(
                paths,
                o,
                &args,
                &format!(
                    "{} (the script of {}) isn't there or can't run: starting the game without its settings.",
                    script.display(),
                    game.info.name
                ),
            );
        }
        let mut argv = vec![script.display().to_string()];
        argv.extend(args.iter().cloned());
        let p = Plan {
            argv,
            ..Default::default()
        };
        log_launch(paths, o.dry_run, &game, &found.how, "its own script", &p);
        return run(&p, o.dry_run);
    }

    let profile = game.info.profile.as_deref().and_then(|id| profiles.get(id));
    let schema = Schema::for_game(profile);
    let screens = screen::screens();
    let (preset, why) = resolve::pick_preset(&app, &game, &screens, o.preset.as_deref());
    let values = resolve::resolve(&schema, &app, &game, preset.as_deref());
    let host = Host::detect(paths);
    let p = plan::build(&schema, &values, &game.info, &args, &screens, &host);
    let preset_text = match (&preset, &why) {
        (Some(id), PresetWhy::Screen { mode }) => {
            format!("preset {} (the screen is {mode})", preset_name(&app, id))
        }
        (Some(id), PresetWhy::Pinned) => format!("preset {} (pinned)", preset_name(&app, id)),
        (Some(id), _) => format!("preset {} (asked for)", preset_name(&app, id)),
        (None, _) => "no preset".into(),
    };

    if o.dry_run {
        println!(
            "game:    {} ({}, {})",
            game.info.name,
            game.path.display(),
            found.how
        );
        if found.created {
            println!("         new: its file would be made now");
        }
        println!("preset:  {preset_text}");
        if !screens.is_empty() {
            let s: Vec<String> = screens
                .iter()
                .map(|s| format!("{} {}@{}", s.name, s.mode(), s.refresh))
                .collect();
            println!("screens: {}", s.join(", "));
        }
        if let Some(v) = p.gpu_check {
            println!("check:   the NVIDIA card first, warn below {v} MiB free");
        }
        if let Some(m) = &p.mouse {
            println!(
                "mouse:   game-mouse [{}] {}",
                m.section,
                if m.start_service {
                    "lowers the DPI while the game is focused"
                } else {
                    "leaves it alone"
                }
            );
        }
        for f in &p.fixes {
            println!("fix:     {f}");
        }
        for (k, v) in &p.env {
            println!("env:     {k}={v}");
        }
        for n in &p.notes {
            println!("note:    {n}");
        }
        println!("run:     {}", plan::quote(&p.argv));
        return ExitCode::SUCCESS;
    }

    if found.created {
        notify(
            "normal",
            "dialog-information",
            &format!("GSet set up {}", game.info.name),
            &format!("Its settings are in GSet now ({}).", game.path.display()),
        );
    }
    for n in &p.notes {
        eprintln!("gset: {}: {n}", game.info.name);
    }
    // before the game: the fixes, the mouse, the GPU
    let compat = std::env::var_os("STEAM_COMPAT_DATA_PATH").map(PathBuf::from);
    for f in &p.fixes {
        match fixes::apply(
            f,
            &FixContext {
                compat_data: compat.as_deref(),
                dry_run: false,
            },
        ) {
            Ok(Some(done)) => append_log(paths, &format!("  fix: {done}")),
            Ok(None) => {}
            Err(e) => warn(paths, false, &game.info.name, &format!("fix {f}: {e}")),
        }
    }
    if let Some(m) = &p.mouse {
        game_mouse(paths, m);
    }
    if let Some(min) = p.gpu_check
        && !gpu_check(paths, &game.info.name, min)
    {
        append_log(
            paths,
            &format!(
                "{} {}: not started, the NVIDIA GPU check failed",
                gset_core::clock::now_text(),
                game.info.name
            ),
        );
        return ExitCode::from(1);
    }
    log_launch(paths, false, &game, &found.how, &preset_text, &p);
    run(&p, false)
}

fn preset_name(app: &AppConfig, id: &str) -> String {
    match app.preset(id) {
        Some(p) => match p.ratio() {
            Some(r) => format!("{} {r}", p.name),
            None => p.name.clone(),
        },
        None => id.into(),
    }
}

/// Starts the command as it came, after telling why.
fn start_as_is(paths: &Paths, o: &Opts, args: &[String], why: &str) -> ExitCode {
    warn(paths, o.dry_run, "GSet", why);
    if args.is_empty() {
        return ExitCode::from(1);
    }
    run(
        &Plan {
            argv: args.to_vec(),
            ..Default::default()
        },
        o.dry_run,
    )
}

fn run(p: &Plan, dry_run: bool) -> ExitCode {
    if dry_run {
        println!("run:     {}", plan::quote(&p.argv));
        return ExitCode::SUCCESS;
    }
    let Some((prog, rest)) = p.argv.split_first() else {
        return ExitCode::from(1);
    };
    let err = Command::new(prog)
        .args(rest)
        .envs(p.env.iter().map(|(k, v)| (k, v)))
        .exec();
    // exec only returns when it failed
    eprintln!("gset: can't start {prog}: {err}");
    notify(
        "critical",
        "dialog-error",
        "The game didn't start",
        &format!("{prog}: {err}"),
    );
    ExitCode::from(127)
}

// ------------------------------------------------------------------------------------------ before the game

/// game-mouse: the skip list says which games it leaves alone; the service lowers the DPI while a
/// game is focused, and exits by itself after the game.
fn game_mouse(paths: &Paths, m: &plan::MouseAction) {
    let file = paths.home.join(".local/state/game-mouse/skip");
    let _ = std::fs::create_dir_all(file.parent().unwrap_or(Path::new("/")));
    let old = std::fs::read_to_string(&file).unwrap_or_default();
    let mut lines: Vec<&str> = old.lines().filter(|l| *l != m.section).collect();
    if m.skip {
        lines.push(&m.section);
    }
    let text = if lines.is_empty() {
        String::new()
    } else {
        format!("{}\n", lines.join("\n"))
    };
    let tmp = file.with_extension("gset-tmp");
    if std::fs::write(&tmp, text)
        .and_then(|()| std::fs::rename(&tmp, &file))
        .is_err()
    {
        eprintln!("gset: can't write {}", file.display());
    }
    if m.start_service {
        let _ = Command::new("systemctl")
            .args(["--user", "start", "--no-block", "game-mouse.service"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

/// When the NVIDIA driver refuses new GPU programs (until Discord is quit, for one), games quietly
/// start on the iGPU or crash. vulkaninfo makes a real device on the NVIDIA card first, with only
/// its driver and the system's libraries; if that fails, the game would fail too.
fn gpu_check(paths: &Paths, game: &str, vram_min: i64) -> bool {
    let icd = Path::new("/usr/share/vulkan/icd.d/nvidia_icd.json");
    if !icd.is_file() || which("vulkaninfo").is_none() {
        return true;
    }
    let mut cmd = Command::new("vulkaninfo");
    cmd.arg("--summary")
        .env_remove("LD_PRELOAD")
        .env_remove("LD_LIBRARY_PATH")
        .env_remove("VK_INSTANCE_LAYERS")
        .env("VK_DRIVER_FILES", icd)
        .env("VK_ICD_FILENAMES", icd)
        .env("VK_LOADER_LAYERS_DISABLE", "~implicit~")
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let ok = output_within(cmd, Duration::from_secs(15)).is_some_and(|out| {
        out.lines().any(|l| {
            l.trim_start().strip_prefix("deviceName").is_some_and(|r| {
                r.trim_start()
                    .trim_start_matches('=')
                    .trim_start()
                    .starts_with("NVIDIA")
            })
        })
    });
    if !ok {
        notify(
            "critical",
            "dialog-warning",
            "NVIDIA GPU not available",
            &format!(
                "The NVIDIA driver refused a new Vulkan device, so {game} would start on another GPU or not at all. Quit Discord completely (tray icon → Quit) and launch again; if that doesn't help, close other GPU programs or restart."
            ),
        );
        return false;
    }
    if vram_min > 0 {
        let mut q = Command::new("nvidia-smi");
        q.args(["--query-gpu=memory.free", "--format=csv,noheader,nounits"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        if let Some(free) = output_within(q, Duration::from_secs(5))
            .and_then(|o| o.lines().next().and_then(|l| l.trim().parse::<i64>().ok()))
            && free < vram_min
        {
            notify(
                "normal",
                "dialog-warning",
                "Low video memory",
                &format!(
                    "Only {free} MiB of video memory is free. If {game} fails or stutters, close GPU-heavy programs first."
                ),
            );
            append_log(paths, &format!("  low video memory: {free} MiB free"));
        }
    }
    true
}

fn output_within(mut cmd: Command, limit: Duration) -> Option<String> {
    let mut child = cmd.spawn().ok()?;
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if start.elapsed() < limit => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
    let mut s = String::new();
    child.stdout.take()?.read_to_string(&mut s).ok()?;
    Some(s)
}

// ------------------------------------------------------------------------------------------ messages

fn notify(urgency: &str, icon: &str, title: &str, text: &str) {
    let _ = Command::new("notify-send")
        .args(["-a", "GSet", "-i", icon, "-u", urgency, title, text])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

fn warn(paths: &Paths, dry_run: bool, title: &str, text: &str) {
    eprintln!("gset: {text}");
    if dry_run {
        return;
    }
    notify("normal", "dialog-warning", title, text);
    append_log(paths, &format!("{} {text}", gset_core::clock::now_text()));
}

fn log_launch(paths: &Paths, dry_run: bool, game: &GameConfig, how: &str, preset: &str, p: &Plan) {
    if dry_run {
        return;
    }
    let mut s = format!(
        "{} {} ({}.toml): {how}, {preset}\n",
        gset_core::clock::now_text(),
        game.info.name,
        game.key
    );
    if !p.env.is_empty() {
        let env: Vec<String> = p.env.iter().map(|(k, v)| format!("{k}={v}")).collect();
        s.push_str(&format!("  env: {}\n", env.join(" ")));
    }
    let shown: Vec<String> = p.argv.iter().map(|a| plan::redact(a)).collect();
    s.push_str(&format!("  run: {}", plan::quote(&shown)));
    append_log(paths, &s);
}

fn append_log(paths: &Paths, line: &str) {
    let file = paths.log_file();
    let _ = std::fs::create_dir_all(&paths.state);
    if std::fs::metadata(&file).is_ok_and(|m| m.len() > 1 << 20) {
        let _ = std::fs::rename(&file, file.with_extension("log.1"));
    }
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&file)
    {
        let _ = writeln!(f, "{line}");
    }
}

fn is_executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(p).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

fn which(name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|p| {
        std::env::split_paths(&p)
            .map(|d| d.join(name))
            .find(|f| is_executable(f))
    })
}

// ------------------------------------------------------------------------------------------ the other commands

fn run_import(paths: &Paths, dir: Option<String>, dry_run: bool, force: bool) -> ExitCode {
    let dir = dir
        .map(|d| paths.expand(&d))
        .unwrap_or_else(|| paths.home.join(".config/launch-options"));
    let profiles = Profiles::load(&profile::dirs(paths));
    let mut app = match AppConfig::load(paths) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("gset: {e}");
            return ExitCode::from(1);
        }
    };
    import::defaults_from_universal(&mut app);
    let results = import::import_dir(&dir, paths, &profiles);
    if results.is_empty() {
        eprintln!("gset: no .conf files in {}", dir.display());
        return ExitCode::from(1);
    }
    let mut failed = false;
    for mut r in results {
        let kind = if r.script_mode {
            "its own script"
        } else {
            "GSet"
        };
        if r.exists && !force {
            println!(
                "{:<16} skipped: GSet already has {} (--force replaces it)",
                r.key,
                r.game.path.display()
            );
            continue;
        }
        let schema = Schema::for_game(r.game.info.profile.as_deref().and_then(|p| profiles.get(p)));
        if !r.script_mode {
            import::pin_against_defaults(&mut r.game, &app, &schema);
        }
        println!(
            "{:<16} {} → {} (started by {kind})",
            r.key,
            r.game.info.name,
            r.game.path.display()
        );
        for n in r.notes.iter().chain(&r.unsupported) {
            println!("{:<16}   {n}", "");
        }
        if dry_run {
            for line in r.game.text().lines() {
                println!("{:<16}   | {line}", "");
            }
        } else if let Err(e) = r.game.save() {
            println!("{:<16}   not saved: {e}", "");
            failed = true;
        }
    }
    if !dry_run && let Err(e) = app.save() {
        eprintln!("gset: {}: {e}", app.path.display());
        failed = true;
    }
    if dry_run {
        println!("\n(dry run: nothing written)");
    }
    if failed {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

fn list(paths: &Paths) -> ExitCode {
    let (games, problems) = config::all_games(paths);
    for g in &games {
        let id = g
            .info
            .steam
            .map(|i| i.to_string())
            .unwrap_or_else(|| "-".into());
        let how = if g.info.script.is_some() {
            "own script"
        } else {
            "GSet"
        };
        println!("{:<16} {:<12} {:<10} {}", g.key, id, how, g.info.name);
    }
    for p in problems {
        eprintln!("gset: {p}");
    }
    ExitCode::SUCCESS
}

fn screens(paths: &Paths) -> ExitCode {
    let s = screen::screens();
    if s.is_empty() {
        println!("No screens found (no Wayland compositor answered).");
    }
    for x in &s {
        println!(
            "{} {} at {} Hz  {}",
            x.name,
            x.mode(),
            x.refresh,
            x.description
        );
    }
    let Ok(app) = AppConfig::load(paths) else {
        return ExitCode::from(1);
    };
    for p in &app.presets {
        let rule = p
            .screen
            .as_deref()
            .map(|r| format!("when the screen is {r}"))
            .unwrap_or_else(|| "no screen rule".into());
        let on = p
            .screen
            .as_deref()
            .is_some_and(|r| s.iter().any(|x| x.mode() == r));
        println!(
            "preset {:<12} {rule}{}",
            p.name,
            if on { "  ← in use now" } else { "" }
        );
    }
    ExitCode::SUCCESS
}
