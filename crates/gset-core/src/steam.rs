//! What Steam tells a launch option: which game it is starting, and what the game is.

use std::path::{Path, PathBuf};

/// Where the game's id came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdFrom {
    /// `AppId=N` in Steam's command (reaper SteamLaunch AppId=N -- …)
    Command,
    SteamAppId,
    /// a non-Steam shortcut: SteamGameId is (id << 32) | 0x02000000
    SteamGameId,
}

impl IdFrom {
    pub fn text(self) -> &'static str {
        match self {
            IdFrom::Command => "AppId= in the command",
            IdFrom::SteamAppId => "SteamAppId",
            IdFrom::SteamGameId => "SteamGameId",
        }
    }
}

/// The id of the game Steam is starting.
pub fn app_id(args: &[String], env: impl Fn(&str) -> Option<String>) -> Option<(u64, IdFrom)> {
    for a in args {
        if let Some(n) = a.strip_prefix("AppId=").and_then(|n| n.parse::<u64>().ok()).filter(|n| *n != 0) {
            return Some((n, IdFrom::Command));
        }
    }
    if let Some(n) = env("SteamAppId").and_then(|v| v.trim().parse::<u64>().ok()).filter(|n| *n != 0) {
        return Some((n, IdFrom::SteamAppId));
    }
    let n = env("SteamGameId").and_then(|v| v.trim().parse::<u64>().ok()).filter(|n| *n != 0)?;
    Some((if n >= 1 << 32 { n >> 32 } else { n }, IdFrom::SteamGameId))
}

/// What a game is, from Steam's command and its files.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Inspection {
    pub name: Option<String>,
    /// the program Steam's command ends in
    pub exe: Option<String>,
    /// the game's folder
    pub root: Option<PathBuf>,
    pub proton: bool,
    /// unity, unreal, or None
    pub engine: Option<String>,
}

/// The program at the end of Steam's command: after `proton waitforexitandrun` for Proton,
/// after the last `--` for a native game.
pub fn game_program(args: &[String]) -> (Option<String>, bool) {
    let mut want_verb = false;
    let mut want_exe = false;
    let mut after_dd = None;
    let mut saw_dd = false;
    for a in args {
        if want_exe {
            return (Some(a.clone()), true);
        }
        if want_verb {
            want_verb = false;
            if matches!(a.as_str(), "waitforexitandrun" | "run" | "runinprefix") {
                want_exe = true;
                continue;
            }
        }
        if saw_dd {
            after_dd = Some(a.clone());
            saw_dd = false;
        }
        if a == "proton" || a.ends_with("/proton") {
            want_verb = true;
        } else if a == "--" {
            saw_dd = true;
        }
    }
    (after_dd.or_else(|| args.first().cloned()), false)
}

pub fn inspect(args: &[String], id: Option<u64>, steam_root: &Path) -> Inspection {
    let (exe, proton) = game_program(args);
    let mut root = None;
    let mut lib = None;
    if let Some(e) = &exe {
        if let Some(i) = e.find("/steamapps/common/") {
            let l = PathBuf::from(&e[..i]).join("steamapps");
            let rest = &e[i + "/steamapps/common/".len()..];
            root = Some(l.join("common").join(rest.split('/').next().unwrap_or("")));
            lib = Some(l);
        } else if e.to_ascii_lowercase().ends_with(".exe") {
            root = Path::new(e).parent().map(Path::to_path_buf);
        }
    }
    if root.as_ref().is_some_and(|r| is_too_wide(r)) {
        root = None;
    }
    let mut name = None;
    if let Some(id) = id {
        let manifest = lib
            .as_ref()
            .map(|l| l.join(format!("appmanifest_{id}.acf")))
            .filter(|m| m.is_file())
            .or_else(|| libraries(steam_root).into_iter().map(|l| l.join(format!("steamapps/appmanifest_{id}.acf"))).find(|m| m.is_file()));
        if let Some(m) = manifest
            && let Ok(text) = std::fs::read_to_string(&m)
        {
            name = vdf_value(&text, "name");
            if root.is_none()
                && let Some(dir) = vdf_value(&text, "installdir")
            {
                let r = m.parent().unwrap_or(Path::new("/")).join("common").join(dir);
                root = r.is_dir().then_some(r);
            }
        }
    }
    let engine = root.as_deref().and_then(engine_of);
    Inspection { name, exe, root, proton, engine }
}

fn is_too_wide(p: &Path) -> bool {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    p == Path::new("/") || Some(p.to_path_buf()) == home || ["/usr", "/bin", "/opt", "/tmp"].iter().any(|w| p.starts_with(w))
}

/// Every Steam library folder (libraryfolders.vdf's "path" lines).
pub fn libraries(steam_root: &Path) -> Vec<PathBuf> {
    let mut out = vec![steam_root.to_path_buf()];
    if let Ok(text) = std::fs::read_to_string(steam_root.join("steamapps/libraryfolders.vdf")) {
        for line in text.lines() {
            if let Some(v) = vdf_pair(line, "path") {
                let p = PathBuf::from(v);
                if !out.contains(&p) {
                    out.push(p);
                }
            }
        }
    }
    out
}

/// Where Steam is: ~/.local/share/Steam, or ~/.steam/steam.
pub fn steam_root(home: &Path) -> PathBuf {
    let a = home.join(".local/share/Steam");
    if a.is_dir() { a } else { home.join(".steam/steam") }
}

fn vdf_pair(line: &str, key: &str) -> Option<String> {
    let l = line.trim();
    let rest = l.strip_prefix('"')?.strip_prefix(key)?.strip_prefix('"')?.trim_start();
    let v = rest.strip_prefix('"')?.strip_suffix('"')?;
    Some(v.replace("\\\\", "\\"))
}

fn vdf_value(text: &str, key: &str) -> Option<String> {
    text.lines().find_map(|l| vdf_pair(l, key))
}

/// Unity or Unreal, from the game's files.
pub fn engine_of(root: &Path) -> Option<String> {
    if find(root, 3, &|n| matches!(n, "UnityPlayer.dll" | "UnityPlayer.so" | "GameAssembly.dll" | "globalgamemanagers")) {
        return Some("unity".into());
    }
    if root.join("Engine/Binaries").is_dir() || find(root, 5, &|n| n.ends_with("-Shipping.exe") || n.ends_with("-Shipping")) {
        return Some("unreal".into());
    }
    None
}

fn find(dir: &Path, depth: u32, hit: &dyn Fn(&str) -> bool) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else { return false };
    let mut subdirs = Vec::new();
    for e in entries.flatten() {
        let name = e.file_name();
        let n = name.to_string_lossy();
        if hit(&n) {
            return true;
        }
        if depth > 1 && e.file_type().is_ok_and(|t| t.is_dir()) {
            subdirs.push(e.path());
        }
    }
    subdirs.iter().any(|d| find(d, depth - 1, hit))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn ids() {
        let none = |_: &str| None;
        let cmd = s(&["/x/reaper", "SteamLaunch", "AppId=4508340", "--", "/x/proton", "waitforexitandrun", "/g/NTE.exe"]);
        assert_eq!(app_id(&cmd, none), Some((4508340, IdFrom::Command)));
        let env = |k: &str| (k == "SteamAppId").then(|| "230410".to_string());
        assert_eq!(app_id(&[], env), Some((230410, IdFrom::SteamAppId)));
        // a shortcut: (id << 32) | 0x02000000, bigger than a signed 64-bit shell number
        let big = ((2541017638u64 << 32) | 0x0200_0000).to_string();
        let env = move |k: &str| (k == "SteamGameId").then(|| big.clone());
        assert_eq!(app_id(&[], env), Some((2541017638, IdFrom::SteamGameId)));
        assert_eq!(app_id(&s(&["AppId=0"]), none), None);
    }

    #[test]
    fn program_of_the_command() {
        let p = s(&["/r/reaper", "SteamLaunch", "AppId=1", "--", "/r/_v2-entry-point", "--verb=waitforexitandrun", "--", "/p/proton", "waitforexitandrun", "/lib/steamapps/common/Game/Game.exe", "-dx11"]);
        assert_eq!(game_program(&p), (Some("/lib/steamapps/common/Game/Game.exe".into()), true));
        let n = s(&["/r/reaper", "SteamLaunch", "AppId=108600", "--", "/r/_v2-entry-point", "--verb=waitforexitandrun", "--", "/lib/steamapps/common/ProjectZomboid/projectzomboid.sh"]);
        assert_eq!(game_program(&n), (Some("/lib/steamapps/common/ProjectZomboid/projectzomboid.sh".into()), false));
    }

    #[test]
    fn engines() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(d.path().join("u")).unwrap();
        std::fs::write(d.path().join("u/UnityPlayer.dll"), "").unwrap();
        assert_eq!(engine_of(d.path()).as_deref(), Some("unity"));
        let e = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(e.path().join("G/Binaries/Win64")).unwrap();
        std::fs::write(e.path().join("G/Binaries/Win64/G-Win64-Shipping.exe"), "").unwrap();
        assert_eq!(engine_of(e.path()).as_deref(), Some("unreal"));
        assert_eq!(vdf_pair("\t\t\"path\"\t\t\"/mnt/Storage/Games/Steam\"", "path").as_deref(), Some("/mnt/Storage/Games/Steam"));
    }
}
