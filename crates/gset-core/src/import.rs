//! Moves games from the shell-script setup (~/.config/launch-options/*.conf, read by
//! ~/.local/bin/*-launch and game-wrapper) to GSet's own files, so that each game starts exactly as
//! before. Values the old scripts didn't have keep GSet's neutral defaults, written out, so that
//! your defaults for new games never change an imported game.

use crate::config::{AppConfig, GameConfig, GameInfo};
use crate::paths::Paths;
use crate::plan::{render_size, words};
use crate::profile::Profiles;
use crate::schema::Schema;
use crate::value::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// A legacy .conf (or a script's defaults block): its NAME=value lines and #@ lines.
#[derive(Debug, Default)]
pub struct Legacy {
    pub values: BTreeMap<String, String>,
    pub meta: BTreeMap<String, String>,
}

/// Reads NAME=value lines as sh would, without expanding anything; `#@ key: value` lines too.
/// With `stop_at_conf`, stops at the script line that reads the conf (a script's defaults block).
pub fn read_legacy(text: &str, stop_at_conf: bool) -> Legacy {
    let mut out = Legacy::default();
    for line in text.lines() {
        let t = line.trim_start();
        if stop_at_conf && t.starts_with("[ -f \"$conf\" ]") {
            break;
        }
        if let Some(rest) = t.strip_prefix("#@") {
            if let Some((k, v)) = rest.split_once(':') {
                out.meta.insert(
                    k.trim().to_ascii_lowercase().replace([' ', '\t'], ""),
                    v.trim().to_string(),
                );
            }
            continue;
        }
        if t.starts_with('#') {
            continue;
        }
        let Some((name, rest)) = t.split_once('=') else {
            continue;
        };
        if name.is_empty()
            || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            || name.starts_with(|c: char| c.is_ascii_digit())
        {
            continue;
        }
        if let Some(v) = sh_value(rest) {
            out.values.insert(name.to_string(), v);
        }
    }
    out
}

/// The value after NAME=, up to the first unquoted blank or ;.
fn sh_value(s: &str) -> Option<String> {
    let mut out = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            ' ' | '\t' | ';' | '&' | '|' => break,
            '\'' => loop {
                match chars.next()? {
                    '\'' => break,
                    x => out.push(x),
                }
            },
            '"' => loop {
                match chars.next()? {
                    '"' => break,
                    '\\' if matches!(chars.peek(), Some('"' | '\\' | '`' | '$')) => {
                        out.push(chars.next()?)
                    }
                    x => out.push(x),
                }
            },
            '\\' => out.push(chars.next()?),
            x => out.push(x),
        }
    }
    Some(out)
}

/// What came of one game.
#[derive(Debug)]
pub struct Imported {
    pub key: String,
    pub game: GameConfig,
    /// started by its own script, as before (Twintail, umu-run, Sober…)
    pub script_mode: bool,
    /// old options GSet doesn't have yet, which the game had switched on
    pub unsupported: Vec<String>,
    pub notes: Vec<String>,
    pub exists: bool,
}

/// The scripts GSet takes over: the per-game ones built on the same options.
fn known_script_engine(name: &str) -> Option<Option<&'static str>> {
    match name {
        "nte-launch" => Some(Some("unreal")),
        "endfield-launch" => Some(Some("unity")),
        "warframe-launch" | "rocketleague-launch" => Some(None),
        _ => None,
    }
}

/// Imports every *.conf in `dir`. Nothing is written: save the games (and the app file) yourself.
pub fn import_dir(dir: &Path, paths: &Paths, profiles: &Profiles) -> Vec<Imported> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|e| {
            e.flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|x| x == "conf"))
                .collect()
        })
        .unwrap_or_default();
    files.sort();
    files
        .iter()
        .filter_map(|f| import_one(f, paths, profiles).ok())
        .collect()
}

pub fn import_one(
    conf_path: &Path,
    paths: &Paths,
    profiles: &Profiles,
) -> Result<Imported, String> {
    let key = conf_path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or("no name")?
        .to_string();
    let conf = read_legacy(
        &std::fs::read_to_string(conf_path).map_err(|e| e.to_string())?,
        false,
    );
    let script = conf.meta.get("script").map(|s| paths.expand(s));
    let script_name = script
        .as_ref()
        .and_then(|s| s.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();
    let script_text = script
        .as_ref()
        .and_then(|s| std::fs::read_to_string(s).ok())
        .unwrap_or_default();

    let steam = conf.meta.get("steam").and_then(|s| s.parse::<u64>().ok());
    let profile = steam
        .and_then(|id| profiles.for_steam(id))
        .map(|p| p.id.clone());
    let mut info = GameInfo {
        name: conf
            .meta
            .get("game")
            .cloned()
            .unwrap_or_else(|| key.clone()),
        steam,
        profile,
        engine: None,
        via: conf.meta.get("via").cloned(),
        mouse_section: conf
            .meta
            .get("game-mouse")
            .cloned()
            .filter(|s| !s.is_empty()),
        script: None,
    };
    let mut imported = Imported {
        key: key.clone(),
        game: GameConfig::new(
            paths,
            &key,
            GameInfo {
                name: info.name.clone(),
                ..GameInfo::default()
            },
        ),
        script_mode: false,
        unsupported: Vec::new(),
        notes: Vec::new(),
        exists: GameConfig::path_for(paths, &key).exists(),
    };

    // which defaults the old script started from, and which engine it assumed
    let wrapper = paths.home.join(".local/bin/game-wrapper");
    let (defaults, engine) = if let Some(engine) = known_script_engine(&script_name) {
        (read_legacy(&script_text, true), engine.map(String::from))
    } else if script_name == "universal-launch" || runs_game_wrapper(&script_text) {
        let gw_engine = script_text
            .split_whitespace()
            .find_map(|w| w.strip_prefix("GW_ENGINE="))
            .filter(|e| !e.starts_with('$'))
            .map(String::from)
            .or_else(|| conf.values.get("GAME_ENGINE").cloned());
        let gw_mouse = script_text
            .split_whitespace()
            .find_map(|w| w.strip_prefix("GW_MOUSE="))
            .filter(|m| !m.starts_with('$'))
            .map(String::from);
        if info.mouse_section.is_none() {
            info.mouse_section = gw_mouse;
        }
        let d = std::fs::read_to_string(&wrapper)
            .map(|t| read_legacy(&t, true))
            .unwrap_or_default();
        (d, gw_engine.filter(|e| e == "unity" || e == "unreal"))
    } else {
        // a script with logic of its own: it keeps starting the game, with its own conf
        info.script = script.as_ref().map(|s| tilde(s, &paths.home));
        imported.script_mode = true;
        imported.notes.push(format!(
            "{script_name} keeps starting it with {}",
            conf_path.display()
        ));
        imported.game.set_info(info);
        return Ok(imported);
    };
    info.engine = engine;

    // the values the script ran with: its defaults, then the conf
    let prefix = conf
        .values
        .keys()
        .chain(defaults.values.keys())
        .find_map(|k| k.split_once('_').map(|(p, _)| p.to_string()))
        .unwrap_or_default();
    let prefix = if prefix == "GW" {
        "GAME".to_string()
    } else {
        prefix
    };
    let mut v: BTreeMap<String, String> = BTreeMap::new();
    for (k, val) in defaults.values.iter().chain(conf.values.iter()) {
        if let Some(short) = k.strip_prefix(&format!("{prefix}_")) {
            v.insert(short.to_string(), val.clone());
        }
    }
    let on = |s: &str| v.get(s).is_some_and(|x| x == "1");
    let mut set: BTreeMap<String, Value> = BTreeMap::new();
    let schema = Schema::for_game(info.profile.as_deref().and_then(|p| profiles.get(p)));

    // what every old script did, whether or not its conf mentions it
    set.insert("gamescope".into(), on("GAMESCOPE").into());
    set.insert("mangohud".into(), on("MANGOHUD").into());
    set.insert("mangohud_hidden".into(), on("MANGOHUD_HIDDEN").into());
    set.insert("obs_capture".into(), on("OBS_CAPTURE").into());
    set.insert("clipboard".into(), on("CLIPBOARD").into());
    set.insert("gpu_check".into(), on("GPU_CHECK").into());
    set.insert("nvidia_only".into(), on("FORCE_NVIDIA").into());
    if let Some(n) = v.get("VRAM_MIN").and_then(|n| n.parse::<i64>().ok()) {
        set.insert("vram_warn".into(), Value::Int(n));
    }
    set.insert(
        "mouse".into(),
        Value::str(
            v.get("MOUSE")
                .map(String::as_str)
                .filter(|m| matches!(*m, "dpi" | "gamescope" | "off"))
                .unwrap_or("off"),
        ),
    );
    if let Some(x) = v.get("GAMESCOPE_MOUSE").and_then(|n| n.parse::<f64>().ok()) {
        set.insert("gamescope_mouse".into(), Value::Float(x));
    }
    set.insert(
        "game_resolution".into(),
        Value::str(v.get("RESOLUTION").cloned().unwrap_or_default()),
    );
    set.insert(
        "vkd3d_config".into(),
        Value::str(v.get("VKD3D_CONFIG").cloned().unwrap_or_default()),
    );
    set.insert(
        "run_before".into(),
        Value::str(v.get("PREFIX").cloned().unwrap_or_default()),
    );
    if script_name == "rocketleague-launch" {
        set.insert("strip_wrappers".into(), true.into());
    }
    gamescope_args(
        v.get("GAMESCOPE_ARGS").map(String::as_str).unwrap_or(""),
        &mut set,
        &mut imported.notes,
    );

    // the graphics API: from the API option, or an API argument in the game arguments
    let engine = info.engine.clone().unwrap_or_default();
    let mut api = v
        .get("API")
        .cloned()
        .filter(|a| matches!(a.as_str(), "d3d11" | "d3d12" | "vulkan"))
        .unwrap_or_else(|| "steam".into());
    let mut game_args = words(v.get("ARGS").map(String::as_str).unwrap_or("")).unwrap_or_default();
    if api == "steam" && matches!(engine.as_str(), "unreal" | "unity") {
        let as_api = |a: &str| match (engine.as_str(), a) {
            ("unreal", "-d3d11") | ("unity", "-force-d3d11") => Some("d3d11"),
            ("unreal", "-d3d12") | ("unity", "-force-d3d12") => Some("d3d12"),
            ("unreal", "-vulkan") | ("unity", "-force-vulkan") => Some("vulkan"),
            _ => None,
        };
        // only the last argument: GSet adds the API argument at the end, where it was
        if let Some(found) = game_args.last().and_then(|a| as_api(a)) {
            api = found.into();
            game_args.pop();
        }
    }
    set.insert("graphics_api".into(), Value::str(api));
    if on("IN_PROCESS_GPU") {
        // Warframe's launcher switch: the profile's own setting when there is one
        if schema.get("in_process_gpu").is_some_and(|d| d.from_profile) {
            set.insert("in_process_gpu".into(), true.into());
        } else if !game_args.iter().any(|a| a == "--in-process-gpu") {
            game_args.insert(0, "--in-process-gpu".into());
        }
    }
    if script_name == "nte-launch" {
        // nte-launch always cleared the launcher's window list
        if schema
            .get("launcher_windows")
            .is_some_and(|d| d.from_profile)
        {
            set.insert("launcher_windows".into(), true.into());
        } else {
            imported.unsupported.push(
                "nte-launch's launcher-crash fix needs NTE's profile, which GSet can't find".into(),
            );
        }
    }
    set.insert(
        "game_args".into(),
        Value::str(shell_words::join(&game_args)),
    );

    // the variables: the ones GSet has settings for become settings
    let mut env: Vec<(String, String)> = Vec::new();
    for w in words(v.get("ENV").map(String::as_str).unwrap_or("")).unwrap_or_default() {
        if let Some((k, val)) = w.split_once('=') {
            env.push((k.to_string(), val.to_string()));
        }
    }
    let get = |env: &Vec<(String, String)>, k: &str| {
        env.iter()
            .rev()
            .find(|(n, _)| n == k)
            .map(|(_, v)| v.clone())
    };
    let mut dlss = "game".to_string();
    if get(&env, "DXVK_NVAPI_DRS_NGX_DLSS_SR_OVERRIDE").as_deref() == Some("on")
        && let Some(sel) = get(
            &env,
            "DXVK_NVAPI_DRS_NGX_DLSS_SR_OVERRIDE_RENDER_PRESET_SELECTION",
        )
        && let Some(p) = sel.strip_prefix("render_preset_")
        && get(&env, "PROTON_ENABLE_NVAPI").as_deref() == Some("1")
    {
        dlss = p.to_ascii_uppercase();
        env.retain(|(k, _)| {
            !k.starts_with("DXVK_NVAPI_DRS_NGX_DLSS_SR_OVERRIDE") && k != "PROTON_ENABLE_NVAPI"
        });
    }
    set.insert("dlss_preset".into(), Value::str(dlss));
    let low = get(&env, "PROTON_DXVK_LOWLATENCY").as_deref() == Some("1");
    env.retain(|(k, _)| k != "PROTON_DXVK_LOWLATENCY");
    set.insert("low_latency".into(), low.into());
    let cap = get(&env, "DXVK_FRAME_RATE")
        .and_then(|n| n.parse::<i64>().ok())
        .filter(|n| *n > 0);
    let vcap = get(&env, "VKD3D_FRAME_RATE").and_then(|n| n.parse::<i64>().ok());
    match cap {
        Some(n) if vcap.is_none() || vcap == Some(n) => {
            env.retain(|(k, _)| k != "DXVK_FRAME_RATE" && k != "VKD3D_FRAME_RATE");
            set.insert("frame_cap".into(), Value::Int(n));
            if vcap.is_none() {
                imported.notes.push(format!("frame cap {n}: GSet also sets VKD3D_FRAME_RATE, which only DirectX 12 games read"));
            }
        }
        _ => {
            set.insert("frame_cap".into(), Value::Int(0));
        }
    }
    set.insert(
        "env".into(),
        Value::str(shell_words::join(
            env.iter().map(|(k, v)| format!("{k}={v}")),
        )),
    );
    if v.get("FORCE_NVIDIA").is_some_and(|x| x == "1") && script_name == "rocketleague-launch" {
        imported.notes.push("NVIDIA only: GSet also sets VKD3D_FILTER_DEVICE_NAME, which only DirectX 12 games read".into());
    }
    set.insert("vsync".into(), Value::str("game"));

    for (opt, what) in [
        ("LOG", "the session log (NTE_LOG)"),
        ("VKD3D_LOG", "vkd3d-proton's own log"),
    ] {
        if v.get(opt).is_some_and(|x| !x.is_empty() && x != "0") {
            imported.unsupported.push(format!(
                "{what} is on in the old file: GSet doesn't have it yet"
            ));
        }
    }

    // in the order GSet shows them, so the file reads like the window
    for def in &schema.settings {
        if let Some(val) = set.remove(&def.key) {
            imported.game.set(&def.key, Some(val));
        }
    }
    imported.game.set_info(info);
    Ok(imported)
}

/// Gamescope's options, split into GSet's settings; the rest stays as "More Gamescope options".
fn gamescope_args(text: &str, set: &mut BTreeMap<String, Value>, notes: &mut Vec<String>) {
    let ws = words(text).unwrap_or_default();
    let mut out: Option<(u32, u32)> = None;
    let mut render: Option<(u32, u32)> = None;
    let mut filter: Option<String> = None;
    let mut extra: Vec<String> = Vec::new();
    let mut fullscreen = false;
    let mut vrr = false;
    let mut refresh = 0i64;
    let (mut w, mut h, mut rw, mut rh) = (None, None, None, None);
    let mut i = 0;
    while i < ws.len() {
        let a = ws[i].as_str();
        let next = ws.get(i + 1).cloned();
        let num = || next.as_deref().and_then(|n| n.parse::<u32>().ok());
        match a {
            "-W" | "--output-width" if num().is_some() => (w, i) = (num(), i + 1),
            "-H" | "--output-height" if num().is_some() => (h, i) = (num(), i + 1),
            "-w" | "--nested-width" if num().is_some() => (rw, i) = (num(), i + 1),
            "-h" | "--nested-height" if num().is_some() => (rh, i) = (num(), i + 1),
            "-r" | "--nested-refresh" if num().is_some() => {
                (refresh, i) = (i64::from(num().unwrap_or(0)), i + 1)
            }
            "-f" | "--fullscreen" => fullscreen = true,
            "--adaptive-sync" => vrr = true,
            "-F" | "--filter" if next.is_some() => (filter, i) = (next.clone(), i + 1),
            "-S" | "--scaler" if next.as_deref() == Some("integer") => {
                (filter, i) = (Some("integer".into()), i + 1)
            }
            _ => extra.push(a.to_string()),
        }
        i += 1;
    }
    if let (Some(w), Some(h)) = (w, h) {
        out = Some((w, h));
    }
    if let (Some(rw), Some(rh)) = (rw, rh) {
        render = Some((rw, rh));
    }
    let mut scale = 100i64;
    match (out, render) {
        (Some((w, h)), Some(r)) => {
            let s = (f64::from(r.0) / f64::from(w) * 100.0).round() as i64;
            if render_size(w, h, s as f64) == r && s < 100 {
                scale = s;
            } else {
                notes.push(format!(
                    "Gamescope's render size {}x{} isn't a whole render scale: kept as an option",
                    r.0, r.1
                ));
                extra.splice(
                    0..0,
                    ["-w".into(), r.0.to_string(), "-h".into(), r.1.to_string()],
                );
                if let Some(f) = filter.take() {
                    extra.extend(if f == "integer" {
                        ["-S".to_string(), f]
                    } else {
                        ["-F".to_string(), f]
                    });
                }
            }
        }
        (None, Some(r)) => extra
            .splice(
                0..0,
                ["-w".into(), r.0.to_string(), "-h".into(), r.1.to_string()],
            )
            .for_each(drop),
        _ => {}
    }
    if scale == 100
        && let Some(f) = filter.take()
    {
        // a filter without a smaller render size does nothing in GSet; keep it as it was
        extra.extend(if f == "integer" {
            ["-S".to_string(), f]
        } else {
            ["-F".to_string(), f]
        });
    }
    set.insert(
        "gamescope_output".into(),
        Value::str(
            out.map(|(w, h)| format!("{w}x{h}"))
                .unwrap_or_else(|| "native".into()),
        ),
    );
    set.insert("render_scale".into(), Value::Int(scale));
    if let Some(f) = filter {
        set.insert("upscaler".into(), Value::str(f));
    }
    set.insert("refresh_rate".into(), Value::Int(refresh));
    set.insert("fullscreen".into(), fullscreen.into());
    set.insert("adaptive_sync".into(), vrr.into());
    set.insert(
        "gamescope_args".into(),
        Value::str(shell_words::join(&extra)),
    );
}

/// A per-game script that hands over to game-wrapper (not one that only mentions it in a comment).
fn runs_game_wrapper(script: &str) -> bool {
    script
        .lines()
        .any(|l| !l.trim_start().starts_with('#') && l.contains("game-wrapper"))
}

fn tilde(p: &Path, home: &Path) -> String {
    match p.strip_prefix(home) {
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => p.display().to_string(),
    }
}

/// Your defaults for games GSet meets for the first time, from what universal-launch gave new games.
pub fn defaults_from_universal(app: &mut AppConfig) {
    if !app.defaults.is_empty() {
        return;
    }
    let d: [(&str, Value); 11] = [
        ("mangohud", true.into()),
        ("mangohud_hidden", true.into()),
        ("obs_capture", true.into()),
        ("clipboard", true.into()),
        ("adaptive_sync", true.into()),
        ("gpu_check", true.into()),
        ("nvidia_only", true.into()),
        ("mouse", "dpi".into()),
        ("dlss_preset", "K".into()),
        ("low_latency", true.into()),
        ("run_before", "taskset -c 0-15 gamemoderun".into()),
    ];
    for (k, v) in d {
        app.set_default(k, Some(v));
    }
}

/// Writes your defaults' settings into a game that doesn't have them, with GSet's own default,
/// so that your defaults (meant for new games) don't change it.
pub fn pin_against_defaults(game: &mut GameConfig, app: &AppConfig, schema: &Schema) {
    for k in app.defaults.keys() {
        if !game.values.contains_key(k)
            && let Some(def) = schema.get(k)
        {
            game.set(k, Some(def.default.clone()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sh_values() {
        let l = read_legacy(
            "#@ game: NTE\n#@ Steam : 4508340\nA=1\nB=\"x y\" # note\nC='a b'\nD=\"q\\\"q\"\nE=\n # comment\nnot a line\n",
            false,
        );
        assert_eq!(l.meta["game"], "NTE");
        assert_eq!(l.meta["steam"], "4508340");
        assert_eq!(l.values["A"], "1");
        assert_eq!(l.values["B"], "x y");
        assert_eq!(l.values["C"], "a b");
        assert_eq!(l.values["D"], "q\"q");
        assert_eq!(l.values["E"], "");
    }

    #[test]
    fn gamescope_options_become_settings() {
        let mut s = BTreeMap::new();
        let mut n = Vec::new();
        gamescope_args(
            "-W 3440 -H 1440 -w 2580 -h 1080 -r 160 -f -F fsr --adaptive-sync",
            &mut s,
            &mut n,
        );
        assert_eq!(s["gamescope_output"], Value::str("3440x1440"));
        assert_eq!(s["render_scale"], Value::Int(75));
        assert_eq!(s["upscaler"], Value::str("fsr"));
        assert_eq!(s["refresh_rate"], Value::Int(160));
        assert_eq!(s["gamescope_args"], Value::str(""));
        assert!(n.is_empty());
        let mut s = BTreeMap::new();
        gamescope_args(
            "-W 3440 -H 1440 -w 2560 -h 1080 -r 165 -f -F nis --hdr-enabled",
            &mut s,
            &mut n,
        );
        assert_eq!(s["render_scale"], Value::Int(100));
        assert_eq!(
            s["gamescope_args"],
            Value::str("-w 2560 -h 1080 --hdr-enabled -F nis")
        );
    }
}
