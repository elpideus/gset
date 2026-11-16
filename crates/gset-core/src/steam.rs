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
        if let Some(n) = a
            .strip_prefix("AppId=")
            .and_then(|n| n.parse::<u64>().ok())
            .filter(|n| *n != 0)
        {
            return Some((n, IdFrom::Command));
        }
    }
    if let Some(n) = env("SteamAppId")
        .and_then(|v| v.trim().parse::<u64>().ok())
        .filter(|n| *n != 0)
    {
        return Some((n, IdFrom::SteamAppId));
    }
    let n = env("SteamGameId")
        .and_then(|v| v.trim().parse::<u64>().ok())
        .filter(|n| *n != 0)?;
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
            .or_else(|| {
                libraries(steam_root)
                    .into_iter()
                    .map(|l| l.join(format!("steamapps/appmanifest_{id}.acf")))
                    .find(|m| m.is_file())
            });
        if let Some(m) = manifest
            && let Ok(text) = std::fs::read_to_string(&m)
        {
            name = vdf_value(&text, "name");
            if root.is_none()
                && let Some(dir) = vdf_value(&text, "installdir")
            {
                let r = m
                    .parent()
                    .unwrap_or(Path::new("/"))
                    .join("common")
                    .join(dir);
                root = r.is_dir().then_some(r);
            }
        }
    }
    let engine = root.as_deref().and_then(engine_of);
    Inspection {
        name,
        exe,
        root,
        proton,
        engine,
    }
}

fn is_too_wide(p: &Path) -> bool {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    p == Path::new("/")
        || Some(p.to_path_buf()) == home
        || ["/usr", "/bin", "/opt", "/tmp"]
            .iter()
            .any(|w| p.starts_with(w))
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
    if a.is_dir() {
        a
    } else {
        home.join(".steam/steam")
    }
}

fn vdf_pair(line: &str, key: &str) -> Option<String> {
    let l = line.trim();
    let rest = l
        .strip_prefix('"')?
        .strip_prefix(key)?
        .strip_prefix('"')?
        .trim_start();
    let v = rest.strip_prefix('"')?.strip_suffix('"')?;
    Some(v.replace("\\\\", "\\"))
}

fn vdf_value(text: &str, key: &str) -> Option<String> {
    text.lines().find_map(|l| vdf_pair(l, key))
}

/// Unity or Unreal, from the game's files.
pub fn engine_of(root: &Path) -> Option<String> {
    if find(root, 3, &|n| {
        matches!(
            n,
            "UnityPlayer.dll" | "UnityPlayer.so" | "GameAssembly.dll" | "globalgamemanagers"
        )
    }) {
        return Some("unity".into());
    }
    if root.join("Engine/Binaries").is_dir()
        || find(root, 5, &|n| {
            n.ends_with("-Shipping.exe") || n.ends_with("-Shipping")
        })
    {
        return Some("unreal".into());
    }
    None
}

fn find(dir: &Path, depth: u32, hit: &dyn Fn(&str) -> bool) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
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
        let cmd = s(&[
            "/x/reaper",
            "SteamLaunch",
            "AppId=4508340",
            "--",
            "/x/proton",
            "waitforexitandrun",
            "/g/NTE.exe",
        ]);
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
        let p = s(&[
            "/r/reaper",
            "SteamLaunch",
            "AppId=1",
            "--",
            "/r/_v2-entry-point",
            "--verb=waitforexitandrun",
            "--",
            "/p/proton",
            "waitforexitandrun",
            "/lib/steamapps/common/Game/Game.exe",
            "-dx11",
        ]);
        assert_eq!(
            game_program(&p),
            (Some("/lib/steamapps/common/Game/Game.exe".into()), true)
        );
        let n = s(&[
            "/r/reaper",
            "SteamLaunch",
            "AppId=108600",
            "--",
            "/r/_v2-entry-point",
            "--verb=waitforexitandrun",
            "--",
            "/lib/steamapps/common/ProjectZomboid/projectzomboid.sh",
        ]);
        assert_eq!(
            game_program(&n),
            (
                Some("/lib/steamapps/common/ProjectZomboid/projectzomboid.sh".into()),
                false
            )
        );
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
        assert_eq!(
            vdf_pair("\t\t\"path\"\t\t\"/mnt/Storage/Games/Steam\"", "path").as_deref(),
            Some("/mnt/Storage/Games/Steam")
        );
    }
}

// ------------------------------------------------------------------------------------------ Steam's own files

/// A text VDF value: a string, or a list of keyed children (Steam's config files).
#[derive(Clone, Debug, PartialEq)]
pub enum Vdf {
    Str(String),
    Map(Vec<(String, Vdf)>),
}

impl Vdf {
    pub fn parse(text: &str) -> Vdf {
        let mut chars = text.chars().peekable();
        Vdf::Map(parse_map(&mut chars))
    }

    /// The child at this key (keys compare without case, as Steam's do).
    pub fn get(&self, key: &str) -> Option<&Vdf> {
        match self {
            Vdf::Map(m) => m
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(key))
                .map(|(_, v)| v),
            Vdf::Str(_) => None,
        }
    }

    pub fn path(&self, keys: &[&str]) -> Option<&Vdf> {
        keys.iter().try_fold(self, |node, k| node.get(k))
    }

    pub fn str(&self) -> Option<&str> {
        match self {
            Vdf::Str(s) => Some(s),
            Vdf::Map(_) => None,
        }
    }

    pub fn entries(&self) -> &[(String, Vdf)] {
        match self {
            Vdf::Map(m) => m,
            Vdf::Str(_) => &[],
        }
    }
}

fn parse_map(chars: &mut std::iter::Peekable<std::str::Chars>) -> Vec<(String, Vdf)> {
    let mut out = Vec::new();
    let mut key: Option<String> = None;
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                let mut s = String::new();
                while let Some(c) = chars.next() {
                    match c {
                        '\\' => {
                            if let Some(n) = chars.next() {
                                s.push(match n {
                                    'n' => '\n',
                                    't' => '\t',
                                    x => x,
                                });
                            }
                        }
                        '"' => break,
                        x => s.push(x),
                    }
                }
                match key.take() {
                    None => key = Some(s),
                    Some(k) => out.push((k, Vdf::Str(s))),
                }
            }
            '{' => {
                let child = parse_map(chars);
                if let Some(k) = key.take() {
                    out.push((k, Vdf::Map(child)));
                }
            }
            '}' => return out,
            '/' if chars.peek() == Some(&'/') => {
                for c in chars.by_ref() {
                    if c == '\n' {
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// The signed-in user's copy of a file in userdata/<user>/config/: the most recently written one.
fn user_config(steam_root: &Path, name: &str) -> Option<PathBuf> {
    let users = std::fs::read_dir(steam_root.join("userdata")).ok()?;
    users
        .flatten()
        .map(|e| e.path().join("config").join(name))
        .filter(|p| p.is_file())
        .max_by_key(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok())
}

fn localconfig(steam_root: &Path) -> Option<Vdf> {
    let path = user_config(steam_root, "localconfig.vdf")?;
    Some(Vdf::parse(&std::fs::read_to_string(path).ok()?))
}

/// A non-Steam game added to Steam: what its Properties say.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Shortcut {
    /// Target, without the quotes Steam puts around it
    pub target: String,
    pub launch_options: String,
}

/// The shortcuts in shortcuts.vdf, by app id. The file is binary KeyValues: 0x00 opens a map,
/// 0x01 is a string, 0x02 a 32-bit number, 0x08 closes a map; every key is a C string.
pub fn parse_shortcuts(b: &[u8]) -> std::collections::HashMap<u64, Shortcut> {
    fn cstr(b: &[u8], i: &mut usize) -> Option<String> {
        let len = b.get(*i..)?.iter().position(|&c| c == 0)?;
        let s = String::from_utf8_lossy(&b[*i..*i + len]).into_owned();
        *i += len + 1;
        Some(s)
    }
    let mut out = std::collections::HashMap::new();
    let mut i = 0;
    let mut depth = 0usize;
    // the shortcut being read (maps at depth 2: shortcuts → "0", "1", …)
    let mut id: Option<u64> = None;
    let mut cur = Shortcut::default();
    while let Some(&t) = b.get(i) {
        i += 1;
        match t {
            0x00 => {
                let Some(_) = cstr(b, &mut i) else { break };
                depth += 1;
                if depth == 2 {
                    id = None;
                    cur = Shortcut::default();
                }
            }
            0x01 => {
                let (Some(k), Some(v)) = (cstr(b, &mut i), cstr(b, &mut i)) else {
                    break;
                };
                if depth == 2 {
                    match k.to_ascii_lowercase().as_str() {
                        "exe" => cur.target = v.trim().trim_matches('"').to_string(),
                        "launchoptions" => cur.launch_options = v,
                        _ => {}
                    }
                }
            }
            0x02 | 0x03 | 0x07 => {
                let Some(k) = cstr(b, &mut i) else { break };
                let n = if t == 0x07 { 8 } else { 4 };
                let Some(bytes) = b.get(i..i + n) else { break };
                if t == 0x02 && depth == 2 && k.eq_ignore_ascii_case("appid") {
                    id = Some(u64::from(u32::from_le_bytes([
                        bytes[0], bytes[1], bytes[2], bytes[3],
                    ])));
                }
                i += n;
            }
            0x08 => {
                if depth == 2
                    && let Some(id) = id.take()
                {
                    out.insert(id, std::mem::take(&mut cur));
                }
                if depth == 0 {
                    break;
                }
                depth -= 1;
            }
            _ => break,
        }
    }
    out
}

/// What Steam knows about each of your games: when you last played it, its launch options.
#[derive(Clone, Debug, Default)]
pub struct SteamState {
    pub last_played: std::collections::HashMap<u64, u64>,
    pub launch_options: std::collections::HashMap<u64, String>,
    /// the compatibility tool picked for a game in its Properties
    pub compat_tool: std::collections::HashMap<u64, String>,
    /// non-Steam games, by their app id
    pub shortcuts: std::collections::HashMap<u64, Shortcut>,
}

pub fn state(steam_root: &Path) -> SteamState {
    let mut s = SteamState::default();
    if let Some(cfg) = localconfig(steam_root)
        && let Some(apps) =
            cfg.path(&["UserLocalConfigStore", "Software", "Valve", "Steam", "apps"])
    {
        for (id, app) in apps.entries() {
            let Ok(id) = id.parse::<u64>() else { continue };
            if let Some(t) = app
                .get("LastPlayed")
                .and_then(Vdf::str)
                .and_then(|t| t.parse().ok())
            {
                s.last_played.insert(id, t);
            }
            if let Some(o) = app.get("LaunchOptions").and_then(Vdf::str) {
                s.launch_options.insert(id, o.to_string());
            }
        }
    }
    if let Some(bytes) =
        user_config(steam_root, "shortcuts.vdf").and_then(|p| std::fs::read(p).ok())
    {
        s.shortcuts = parse_shortcuts(&bytes);
    }
    if let Ok(text) = std::fs::read_to_string(steam_root.join("config/config.vdf"))
        && let Some(map) = Vdf::parse(&text).path(&[
            "InstallConfigStore",
            "Software",
            "Valve",
            "Steam",
            "CompatToolMapping",
        ])
    {
        for (id, entry) in map.entries() {
            if let (Ok(id), Some(name)) = (id.parse::<u64>(), entry.get("name").and_then(Vdf::str))
                && id != 0
                && !name.is_empty()
            {
                s.compat_tool.insert(id, name.to_string());
            }
        }
    }
    s
}

/// The id Steam uses to start a game: an app id as it is, a shortcut's as (id << 32) | 0x02000000.
pub fn run_id(id: u64) -> u64 {
    if id > u64::from(u32::MAX >> 1) {
        (id << 32) | 0x0200_0000
    } else {
        id
    }
}

#[cfg(test)]
mod vdf_tests {
    use super::*;

    #[test]
    fn reads_shortcuts() {
        // the layout Steam writes: shortcuts → "0" { appid, AppName, Exe, …, tags { } }
        let mut b = Vec::new();
        let map = |b: &mut Vec<u8>, k: &str| {
            b.push(0);
            b.extend(k.as_bytes());
            b.push(0);
        };
        let text = |b: &mut Vec<u8>, k: &str, v: &str| {
            b.push(1);
            b.extend(k.as_bytes());
            b.push(0);
            b.extend(v.as_bytes());
            b.push(0);
        };
        let int = |b: &mut Vec<u8>, k: &str, v: u32| {
            b.push(2);
            b.extend(k.as_bytes());
            b.push(0);
            b.extend(v.to_le_bytes());
        };
        map(&mut b, "shortcuts");
        map(&mut b, "0");
        int(&mut b, "appid", 2293937255);
        text(&mut b, "AppName", "Roblox");
        text(&mut b, "Exe", "/home/u/.local/bin/gset %command%");
        text(&mut b, "LaunchOptions", "");
        map(&mut b, "tags");
        text(&mut b, "0", "Favorites");
        b.push(8);
        b.push(8);
        map(&mut b, "1");
        int(&mut b, "appid", 4245226616);
        text(&mut b, "appname", "Fishao");
        text(&mut b, "exe", "\"/games/Fishao/FishaoGame.exe\"");
        text(&mut b, "LaunchOptions", "/home/u/.local/bin/gset %command%");
        int(&mut b, "LastPlayTime", 1);
        b.push(8);
        b.push(8);
        b.push(8);
        let s = parse_shortcuts(&b);
        assert_eq!(s.len(), 2);
        assert_eq!(s[&2293937255].target, "/home/u/.local/bin/gset %command%");
        assert_eq!(
            s[&4245226616].target, "/games/Fishao/FishaoGame.exe",
            "quotes go"
        );
        assert_eq!(
            s[&4245226616].launch_options,
            "/home/u/.local/bin/gset %command%"
        );
        assert!(
            parse_shortcuts(&b[..b.len() / 2]).len() <= 1,
            "a cut file doesn't panic"
        );
    }

    #[test]
    fn reads_steams_text_files() {
        let text = r#""UserLocalConfigStore" { "Software" { "Valve" { "Steam" { "apps" {
            "4508340" { "LastPlayed" "1731400000" "LaunchOptions" "/home/u/.local/bin/nte-launch %command%" }
            "892970"  { "LastPlayed" "1700000000" }
        } } } } }"#;
        let v = Vdf::parse(text);
        let apps = v
            .path(&["UserLocalConfigStore", "Software", "Valve", "Steam", "apps"])
            .unwrap();
        assert_eq!(apps.entries().len(), 2);
        assert_eq!(
            apps.get("4508340")
                .and_then(|a| a.get("launchoptions"))
                .and_then(Vdf::str),
            Some("/home/u/.local/bin/nte-launch %command%")
        );
        assert_eq!(run_id(2541017638), (2541017638u64 << 32) | 0x0200_0000);
        assert_eq!(run_id(230410), 230410);
    }
}
