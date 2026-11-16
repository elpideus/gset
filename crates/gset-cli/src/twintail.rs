//! Starting a game installed with TwintailLauncher, and seeing it through.
//!
//! `twintaillauncher --install=<id>` starts the game, hides itself in the tray and never quits, so
//! Steam would keep showing the game as running. So gset stays, instead of becoming the game:
//!  - it checks Twintail's game manifest for a newer version first; if there is one, it opens
//!    Twintail's window instead, so you update and press Play there;
//!  - it installs a new Steam Runtime 4 before Twintail starts: Twintail would otherwise empty
//!    the runtime and download the new one while --install starts the game in it, and the launch
//!    fails;
//!  - it quits Twintail once the game has stayed closed for a few seconds (Twintail has saved the
//!    playtime and cleared the Discord status by then), but keeps it open with its window shown
//!    when the game closes within a minute (a crash, or the game asking for an update or a repair).
//!
//! Twintail's database and manifests are read with sqlite3, and the network with curl, as the
//! twintail-steam script this replaces did: the launcher stays free of libraries of its own.

use gset_core::runner::{TWINTAIL, TwintailRun, newer};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitCode, Stdio};
use std::time::{Duration, Instant};

const RT_IMAGES: &str = "https://repo.steampowered.com/steamrt4/images";
/// the Steam Runtime 4 branch Twintail follows
const RT_BRANCH: &str = "latest-public-beta";
/// the MangoHud setting GSet keeps in the install's variables while "start hidden" is on
const HIDE: &str = "MANGOHUD_CONFIG=read_cfg,no_display";

pub struct Ctx<'a> {
    pub home: &'a Path,
    /// gset's state folder: the runtime download and its lock go here
    pub state: &'a Path,
    pub game: &'a str,
    pub log: &'a dyn Fn(&str),
}

impl Ctx<'_> {
    fn data(&self) -> PathBuf {
        self.home.join(".local/share/twintaillauncher")
    }
    fn db(&self) -> PathBuf {
        self.data().join("storage.db")
    }
}

fn notify(title: &str, text: &str) {
    let _ = Command::new("notify-send")
        .args([
            "-a",
            "TwintailLauncher",
            "-i",
            "twintaillauncher",
            title,
            text,
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

/// One query on Twintail's database; None when sqlite3 fails.
fn sql(db: &Path, query: &str, write: bool) -> Option<String> {
    let mut c = Command::new("sqlite3");
    if !write {
        c.arg("-readonly");
    }
    let out = c
        .args(["-cmd", ".timeout 3000", "-separator", "|"])
        .arg(db)
        .arg(query)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn curl(url: &str, max_time: u32) -> Option<String> {
    let out = Command::new("curl")
        .args(["-fsS", "--max-time", &max_time.to_string(), url])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Your processes: (name, command line).
fn processes() -> Vec<(String, String)> {
    let uid = std::fs::metadata("/proc/self").map(|m| m.uid()).ok();
    let Ok(dir) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    dir.flatten()
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .bytes()
                .all(|b| b.is_ascii_digit())
        })
        .filter(|e| uid.is_none() || e.metadata().ok().map(|m| m.uid()) == uid)
        .filter_map(|e| {
            // a zombie has closed already, though its name stays until it's reaped
            let stat = std::fs::read_to_string(e.path().join("stat")).ok()?;
            if stat
                .rsplit_once(") ")
                .is_some_and(|(_, rest)| rest.starts_with('Z'))
            {
                return None;
            }
            let comm = std::fs::read_to_string(e.path().join("comm")).ok()?;
            let cmd = std::fs::read(e.path().join("cmdline")).ok()?;
            let cmd = String::from_utf8_lossy(&cmd).replace('\0', " ");
            Some((comm.trim_end().to_string(), cmd))
        })
        .collect()
}

/// Twintail's own process (its name is cut to 15 characters).
fn ttl_running() -> bool {
    processes().iter().any(|(n, _)| n == "twintaillaunche")
}

/// The game's process, as Twintail matches it: the start of its name.
fn game_running(stem: &str) -> bool {
    !stem.is_empty() && processes().iter().any(|(n, _)| n.starts_with(stem))
}

fn alive(c: &mut Child) -> bool {
    matches!(c.try_wait(), Ok(None))
}

/// Twintail's program (GSET_TWINTAIL: another one, to test with).
fn ttl_path() -> String {
    std::env::var("GSET_TWINTAIL").unwrap_or_else(|_| TWINTAIL.to_string())
}

/// Twintail is single-instance: starting it again only shows the open window.
fn show_ttl() {
    if let Ok(mut c) = Command::new(ttl_path())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        std::thread::spawn(move || {
            let _ = c.wait();
        });
    }
}

/// The game's process name, from the manifest: the exe's name before its first dot, 15 characters.
pub fn exe_stem(manifest: &str) -> String {
    let v: serde_json::Value = serde_json::from_str(manifest).unwrap_or_default();
    let exe = v["paths"]["exe_filename"].as_str().unwrap_or("");
    let base = exe.rsplit(['/', '\\']).next().unwrap_or("");
    let stem: String = base
        .split('.')
        .next()
        .unwrap_or("")
        .chars()
        .take(15)
        .collect();
    if stem
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | ' ' | '-'))
    {
        stem
    } else {
        String::new()
    }
}

/// Keeps "start MangoHud hidden" in the install's variables in Twintail, where Twintail's own Play
/// button gets it too. A MANGOHUD_CONFIG of your own there is left alone.
pub fn with_mangohud_hidden(envs: &str, hidden: bool) -> Option<String> {
    let items: Vec<&str> = envs.split(';').filter(|s| !s.is_empty()).collect();
    if items.contains(&HIDE) {
        if hidden {
            return None;
        }
        return Some(
            items
                .iter()
                .filter(|i| **i != HIDE)
                .copied()
                .collect::<Vec<_>>()
                .join(";"),
        );
    }
    if !hidden || items.iter().any(|i| i.starts_with("MANGOHUD_CONFIG=")) {
        return None;
    }
    let mut out = items;
    out.push(HIDE);
    Some(out.join(";"))
}

fn rt_version(dir: &Path) -> Option<String> {
    let text = std::fs::read_to_string(dir.join("VERSIONS.txt")).ok()?;
    text.lines().find_map(|l| {
        let mut f = l.split('\t');
        (f.next() == Some("depot"))
            .then(|| f.next().map(str::to_string))
            .flatten()
    })
}

fn executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(p).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

/// Installs a newer Steam Runtime 4 (only a broken or missing one, with `broken_only`), unpacked
/// next to the old one and swapped in only when complete. False: there's no usable runtime.
fn update_runtime(c: &Ctx, broken_only: bool) -> bool {
    let log = c.log;
    let base = sql(&c.db(), "SELECT default_runner_path FROM settings;", false)
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| c.data().join("compatibility/runners"));
    let rt = base.join("steamrt/steamrt4");
    let have = rt_version(&rt).filter(|_| executable(&rt.join("_v2-entry-point")));
    if broken_only && have.is_some() {
        return true;
    }
    let want = curl(&format!("{RT_IMAGES}/{RT_BRANCH}.txt"), 5)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit() || c == '.'));
    let Some(want) = want else {
        if have.is_some() {
            return true;
        }
        log("Steam Runtime 4 is broken, and the latest version can't be read (offline?)");
        notify(
            "Steam Runtime 4 is missing",
            "It couldn't be downloaded (offline?), so the game can't start.",
        );
        return false;
    };
    if let Some(h) = &have
        && !newer(&want, h)
    {
        return true;
    }
    // a running game uses it (srt-bwrap runs from it)
    let inside = format!("{}/", rt.display());
    if processes().iter().any(|(_, cmd)| cmd.contains(&inside)) {
        log(&format!(
            "Steam Runtime 4 {want} is out, but the current one is in use: not updating now"
        ));
        return true;
    }
    log(&format!(
        "Steam Runtime 4: installing {want} (installed: {})",
        have.as_deref().unwrap_or("none, broken")
    ));
    notify(
        "Updating Steam Runtime 4",
        "The game starts once it's installed, usually within a minute.",
    );
    let tmp = c.state.join("steamrt4-update");
    let _ = std::fs::remove_dir_all(&tmp);
    let new = tmp.join("new");
    let archive = tmp.join("rt.tar.xz");
    let ok = std::fs::create_dir_all(&new).is_ok()
        && curl(&format!("{RT_IMAGES}/{want}/SHA256SUMS"), 10)
            .and_then(|sums| {
                sums.lines().find_map(|l| {
                    let mut f = l.split_whitespace();
                    let sum = f.next()?;
                    (f.next()? == "*SteamLinuxRuntime_4.tar.xz").then(|| sum.to_string())
                })
            })
            .is_some_and(|sum| {
                let url = format!("{RT_IMAGES}/{want}/SteamLinuxRuntime_4.tar.xz");
                let fetched = Command::new("curl")
                    .args(["-fsS", "--retry", "2", "--max-time", "900", "-o"])
                    .arg(&archive)
                    .arg(&url)
                    .stdin(Stdio::null())
                    .status()
                    .is_ok_and(|s| s.success());
                fetched && sha256_is(&archive, &sum)
            })
        && Command::new("tar")
            .arg("-xJf")
            .arg(&archive)
            .args(["--strip-components=1", "-C"])
            .arg(&new)
            .stdin(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
        && rt_version(&new).as_deref() == Some(want.as_str())
        && executable(&new.join("_v2-entry-point"));
    if ok {
        let _ = std::fs::remove_file(&archive);
        let old = tmp.join("old");
        if rt.exists() && std::fs::rename(&rt, &old).is_err() {
            log(&format!(
                "Steam Runtime 4 {want}: couldn't move the old one aside"
            ));
        } else if std::fs::rename(&new, &rt).is_ok() {
            let _ = std::fs::remove_dir_all(&tmp);
            log(&format!("Steam Runtime 4 {want} installed"));
            return true;
        } else {
            log(&format!(
                "Steam Runtime 4 {want}: couldn't move the new one in"
            ));
            if old.exists() && std::fs::rename(&old, &rt).is_err() {
                log(&format!("the old runtime is left in {}", old.display()));
                return false;
            }
        }
    } else {
        log(&format!(
            "Steam Runtime 4 {want}: download, checksum or unpacking failed"
        ));
    }
    let _ = std::fs::remove_dir_all(&tmp);
    if let Some(h) = have {
        log(&format!("keeping Steam Runtime 4 {h}"));
        return true;
    }
    notify(
        "Steam Runtime 4 couldn't be installed",
        "See GSet's log: ~/.local/state/gset/launch.log",
    );
    false
}

fn sha256_is(file: &Path, sum: &str) -> bool {
    let Ok(out) = Command::new("sha256sum").arg(file).output() else {
        return false;
    };
    String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .next()
        .is_some_and(|s| s.eq_ignore_ascii_case(sum))
}

/// One runtime update at a time, also across launches.
fn locked_update_runtime(c: &Ctx, broken_only: bool) -> bool {
    use std::os::unix::io::AsRawFd;
    let _ = std::fs::create_dir_all(c.state);
    let Ok(lock) = std::fs::File::create(c.state.join("steamrt4.lock")) else {
        return update_runtime(c, broken_only);
    };
    // SAFETY: flock on a file descriptor we own; it's released when `lock` is dropped
    unsafe {
        libc::flock(lock.as_raw_fd(), libc::LOCK_EX);
    }
    update_runtime(c, broken_only)
}

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

/// Starts the game through Twintail (or its window, when there's an update) and waits it out.
pub fn run(c: &Ctx, t: &TwintailRun, argv: &[String]) -> ExitCode {
    let log = c.log;
    let db = c.db();
    let id = t.install.as_str();
    let mut installed = String::new();
    let mut ignore = String::new();
    let mut repo = String::new();
    let mut file = String::new();
    let mut name = String::new();
    if valid_id(id) {
        let row = sql(
            &db,
            &format!(
                "SELECT i.version, i.ignore_updates, r.github_id, m.filename, i.name FROM install i \
                 JOIN manifest m ON m.id = i.manifest_id JOIN repository r ON r.id = m.repository_id \
                 WHERE i.id = '{id}';"
            ),
            false,
        )
        .unwrap_or_default();
        let mut f = row.splitn(5, '|').map(str::to_string);
        installed = f.next().unwrap_or_default();
        ignore = f.next().unwrap_or_default();
        repo = f.next().unwrap_or_default();
        file = f.next().unwrap_or_default();
        name = f.next().unwrap_or_default();
        if row.is_empty() {
            log(&format!("Twintail has no install {id}"));
        }
    } else {
        log(&format!("no usable install id: “{id}”"));
    }
    if name.is_empty() {
        name = c.game.to_string();
    }
    let manifest = c.data().join("manifests").join(&repo).join(&file);

    if !installed.is_empty()
        && let Some(envs) = sql(
            &db,
            &format!("SELECT env_vars FROM install WHERE id = '{id}';"),
            false,
        )
    {
        if envs
            .split(';')
            .any(|i| i.starts_with("MANGOHUD_CONFIG=") && i != HIDE)
        {
            log(&format!(
                "{name} has its own MANGOHUD_CONFIG in Twintail: leaving it"
            ));
        } else if let Some(new) = with_mangohud_hidden(&envs, t.mangohud_hidden) {
            let q = format!(
                "UPDATE install SET env_vars = '{}' WHERE id = '{id}';",
                new.replace('\'', "''")
            );
            if sql(&db, &q, true).is_some() {
                log(&format!("Twintail's environment for {name} is now: {new}"));
            }
        }
    }

    let stem = std::fs::read_to_string(&manifest)
        .map(|m| exe_stem(&m))
        .unwrap_or_default();

    if ttl_running() {
        if game_running(&stem) {
            log(&format!("Twintail and {stem} are already running"));
        } else {
            log("Twintail already open: showing its window");
            show_ttl();
            notify(
                "TwintailLauncher is already open",
                "Press Play there to start the game.",
            );
        }
        return ExitCode::SUCCESS;
    }

    // the newest version from the manifest Twintail reads (its bot updates it every 2 hours);
    // Twintail's own copy from its last start when GitHub can't be reached
    let mut latest = String::new();
    if t.update_check && ignore == "0" && !repo.is_empty() && !file.is_empty() {
        let read = |text: &str| -> Option<String> {
            serde_json::from_str::<serde_json::Value>(text).ok()?["latest_version"]
                .as_str()
                .map(str::to_string)
        };
        latest = curl(
            &format!("https://raw.githubusercontent.com/{repo}/main/{file}"),
            5,
        )
        .and_then(|t| read(&t))
        .or_else(|| {
            std::fs::read_to_string(&manifest)
                .ok()
                .and_then(|t| read(&t))
        })
        .unwrap_or_default();
    }

    // before Twintail starts, so it finds the runtime up to date and leaves it alone
    if !game_running(&stem) {
        locked_update_runtime(c, !t.runtime_update);
    }

    let update = !latest.is_empty() && !installed.is_empty() && newer(&latest, &installed);
    let spawned = if update {
        log(&format!(
            "update: {name} has {installed}, {latest} is out: opening Twintail's window"
        ));
        let child = Command::new(ttl_path()).spawn();
        notify(
            &format!("{} {latest} is available", c.game),
            "Update it in TwintailLauncher, then press Play there.",
        );
        child
    } else {
        log(&format!(
            "launch: {name} (installed {}, latest {})",
            if installed.is_empty() {
                "?"
            } else {
                &installed
            },
            if latest.is_empty() {
                "unknown"
            } else {
                &latest
            }
        ));
        Command::new(ttl_path())
            .args(argv.get(1..).unwrap_or_default())
            .spawn()
    };
    let mut ttl = match spawned {
        Ok(child) => child,
        Err(e) => {
            log(&format!("couldn't start Twintail: {e}"));
            notify(
                "TwintailLauncher didn't start",
                &format!("{}: {e}", ttl_path()),
            );
            return ExitCode::from(127);
        }
    };

    let mut seen = false;
    let (mut started, mut last) = (Instant::now(), Instant::now());
    let mut waited = 0u64;
    let mut shown = false;
    loop {
        let now = Instant::now();
        if game_running(&stem) {
            if !seen {
                seen = true;
                started = now;
                log(&format!("{stem} started"));
            }
            last = now;
        } else if seen {
            if now.duration_since(last).as_secs() >= t.exit_grace {
                let ran = last.duration_since(started).as_secs();
                if ran >= t.short_run {
                    log(&format!("{stem} closed after {ran}s"));
                    break;
                }
                log(&format!(
                    "{stem} closed after {ran}s: keeping Twintail open"
                ));
                seen = false;
                waited = 0;
                shown = true;
                if alive(&mut ttl) {
                    show_ttl();
                    notify(
                        &format!("The game closed after {ran} seconds"),
                        "TwintailLauncher stays open in case it needs an update or a repair.",
                    );
                }
            }
        } else {
            // no game: done once the Twintail this launch started is closed
            if !alive(&mut ttl) {
                log("Twintail closed, no game running");
                return ExitCode::SUCCESS;
            }
            waited += 1;
            if !update && !shown && waited >= t.game_wait {
                log(&format!(
                    "no {stem} after {}s: showing Twintail's window",
                    t.game_wait
                ));
                shown = true;
                show_ttl();
                notify(
                    "The game hasn't started",
                    "Have a look at TwintailLauncher's window.",
                );
            }
        }
        std::thread::sleep(Duration::from_secs(1));
    }

    // quit the Twintail this launch started (an instance you opened yourself is left alone)
    if alive(&mut ttl) {
        log("quitting Twintail");
        // SAFETY: a signal to our own child, whose pid can't have been reused (it isn't reaped)
        unsafe {
            libc::kill(ttl.id() as libc::pid_t, libc::SIGTERM);
        }
        for _ in 0..5 {
            if !alive(&mut ttl) {
                break;
            }
            std::thread::sleep(Duration::from_secs(1));
        }
        if alive(&mut ttl) {
            log("Twintail ignored SIGTERM, killing it");
            let _ = ttl.kill();
            let _ = ttl.wait();
        }
    }
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mangohud_hidden_in_twintails_variables() {
        assert_eq!(
            with_mangohud_hidden("OBS_VKCAPTURE=1", true).as_deref(),
            Some("OBS_VKCAPTURE=1;MANGOHUD_CONFIG=read_cfg,no_display")
        );
        assert_eq!(
            with_mangohud_hidden("OBS_VKCAPTURE=1;MANGOHUD_CONFIG=read_cfg,no_display", true),
            None,
            "already there"
        );
        assert_eq!(
            with_mangohud_hidden("OBS_VKCAPTURE=1;MANGOHUD_CONFIG=read_cfg,no_display", false)
                .as_deref(),
            Some("OBS_VKCAPTURE=1")
        );
        assert_eq!(
            with_mangohud_hidden("MANGOHUD_CONFIG=fps_only", true),
            None,
            "yours stays"
        );
        assert_eq!(with_mangohud_hidden("", false), None);
    }

    #[test]
    fn the_games_process_name() {
        assert_eq!(
            exe_stem(r#"{"paths":{"exe_filename":"GenshinImpact.exe"}}"#),
            "GenshinImpact"
        );
        assert_eq!(
            exe_stem(r#"{"paths":{"exe_filename":"Games/BH3.exe"}}"#),
            "BH3"
        );
        assert_eq!(
            exe_stem(r#"{"paths":{"exe_filename":"AVeryLongExecutableName.exe"}}"#),
            "AVeryLongExecut"
        );
        assert_eq!(exe_stem("not json"), "");
    }
}
