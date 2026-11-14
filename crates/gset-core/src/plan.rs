//! Turning a game's values and Steam's command into the command that starts the game.
//!
//! The order of the wrappers, from the outside in: commands from "Run before", Gamescope, the
//! clipboard helper, MangoHud, the OBS capture hook, then Steam's own command (with the game's
//! arguments added at its end).

use crate::config::GameInfo;
use crate::resolve::Resolved;
use crate::schema::Schema;
use crate::screen::Screen;
use crate::value::Value;
use serde::Serialize;
use std::path::PathBuf;

/// Arguments that pick a graphics API, dropped when GSet picks one.
const API_ARGS: &[&str] = &["-force-d3d11", "-force-d3d12", "-force-vulkan", "-force-glcore", "-dx11", "-dx12", "-d3d11", "-d3d12", "-vulkan"];

/// What the computer offers at launch time.
#[derive(Clone, Debug, Default)]
pub struct Host {
    pub wayland_display: Option<String>,
    /// MANGOHUD_CONFIG as GSet was started with it
    pub mangohud_config: Option<String>,
    /// gamescope-clipboard, when installed
    pub clipboard_helper: Option<PathBuf>,
    /// game-mouse is set up (its config exists)
    pub game_mouse: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct MouseAction {
    /// the game's section in game-mouse.conf
    pub section: String,
    /// game-mouse leaves this game alone (Gamescope slows the mouse instead, or nothing does)
    pub skip: bool,
    /// start game-mouse.service, which lowers the DPI while the game is focused
    pub start_service: bool,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Plan {
    pub argv: Vec<String>,
    /// variables to set, in order (a later one wins)
    pub env: Vec<(String, String)>,
    /// check the NVIDIA card first; warn below this many MiB of free video memory
    pub gpu_check: Option<i64>,
    pub mouse: Option<MouseAction>,
    /// built-in fixes to run first
    pub fixes: Vec<String>,
    /// settings that couldn't be used, and why
    pub notes: Vec<String>,
}

impl Plan {
    pub fn env_value(&self, key: &str) -> Option<&str> {
        self.env.iter().rev().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }
}

/// "3440x1440" (also "3440×1440").
pub fn parse_size(s: &str) -> Option<(u32, u32)> {
    let (w, h) = s.trim().split_once(['x', 'X', '×'])?;
    let (w, h) = (w.trim().parse::<u32>().ok()?, h.trim().parse::<u32>().ok()?);
    (w > 0 && h > 0).then_some((w, h))
}

/// Gamescope's render size for a render scale: even numbers, as the import found them.
pub fn render_size(w: u32, h: u32, scale: f64) -> (u32, u32) {
    let f = scale / 100.0;
    let even = |x: u32| ((f64::from(x) * f / 2.0).round() as u32) * 2;
    (even(w), even(h))
}

/// Splits text the way a shell splits words, without expanding anything.
pub fn words(text: &str) -> Result<Vec<String>, String> {
    shell_words::split(text).map_err(|e| format!("“{text}”: {e}"))
}

fn is_assignment(w: &str) -> bool {
    w.split_once('=').is_some_and(|(k, _)| !k.is_empty() && k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') && !k.starts_with(|c: char| c.is_ascii_digit()))
}

pub fn build(schema: &Schema, r: &Resolved, info: &GameInfo, incoming: &[String], screens: &[Screen], host: &Host) -> Plan {
    let mut plan = Plan::default();
    let mut args: Vec<String> = incoming.to_vec();
    let engine = r.text("engine");
    let gamescope = r.on("gamescope");

    // a launcher's own MangoHud and GameMode (Heroic puts them first), so these settings alone decide
    if r.on("strip_wrappers") {
        loop {
            match args.first().map(String::as_str) {
                Some(a) if a == "mangohud" || a.ends_with("/mangohud") => {
                    args.remove(0);
                    if args.first().is_some_and(|a| a == "--dlsym") {
                        args.remove(0);
                    }
                }
                Some(a) if a == "gamemoderun" || a.ends_with("/gamemoderun") => {
                    args.remove(0);
                }
                _ => break,
            }
        }
    }

    // graphics API: replaces the one Steam's launch dialog passed
    let api = r.text("graphics_api");
    if !api.is_empty() && api != "steam" {
        let arg = match (engine.as_str(), api.as_str()) {
            ("unity", "d3d11" | "d3d12" | "vulkan") => Some(format!("-force-{api}")),
            ("unreal", "d3d11" | "d3d12") => Some(format!("-{api}")),
            ("unreal", "vulkan") => Some("-vulkan".into()),
            _ => None,
        };
        match arg {
            Some(a) => {
                args.retain(|x| !API_ARGS.contains(&x.as_str()));
                args.push(a);
            }
            None => plan.notes.push(format!("graphics API {api}: GSet doesn't know how to pass it to this game (engine: {})", or_unknown(&engine))),
        }
    }

    // the resolution the game starts at
    let res = r.text("game_resolution");
    if !res.is_empty() {
        match (parse_size(&res), engine.as_str()) {
            (Some((w, h)), "unity") => args.extend(["-screen-width".into(), w.to_string(), "-screen-height".into(), h.to_string(), "-screen-fullscreen".into(), "1".into()]),
            (Some((w, h)), "unreal") => args.extend([format!("-ResX={w}"), format!("-ResY={h}"), "-FULLSCREEN".into()]),
            (Some(_), _) => plan.notes.push(format!("game resolution {res}: this game takes no resolution argument")),
            (None, _) => plan.notes.push(format!("game resolution “{res}”: expected WIDTHxHEIGHT")),
        }
    }

    match words(&r.text("game_args")) {
        Ok(w) => args.extend(w),
        Err(e) => plan.notes.push(format!("game arguments: {e}")),
    }

    // variables
    let mut env: Vec<(String, String)> = Vec::new();
    let mut set = |k: &str, v: String| env.push((k.to_string(), v));
    if r.on("nvidia_only") {
        set("DXVK_FILTER_DEVICE_NAME", "NVIDIA".into());
        set("VKD3D_FILTER_DEVICE_NAME", "NVIDIA".into());
    }
    let vkd3d = r.text("vkd3d_config");
    if !vkd3d.is_empty() {
        set("VKD3D_CONFIG", vkd3d);
    }
    let dlss = r.text("dlss_preset");
    if !dlss.is_empty() && dlss != "game" {
        set("PROTON_ENABLE_NVAPI", "1".into());
        set("DXVK_NVAPI_DRS_NGX_DLSS_SR_OVERRIDE", "on".into());
        set("DXVK_NVAPI_DRS_NGX_DLSS_SR_OVERRIDE_RENDER_PRESET_SELECTION", format!("render_preset_{}", dlss.to_ascii_lowercase()));
    }
    if r.on("low_latency") {
        set("PROTON_DXVK_LOWLATENCY", "1".into());
    }
    let cap = r.num("frame_cap");
    if cap > 0.0 {
        set("DXVK_FRAME_RATE", format!("{}", cap as i64));
        set("VKD3D_FRAME_RATE", format!("{}", cap as i64));
    }
    // the game's own settings from its profile
    for def in schema.settings.iter().filter(|d| d.from_profile && r.on(&d.key) && schema.applies(&d.key, &r.values)) {
        for (k, v) in &def.effect.env {
            set(k, v.clone());
        }
        args.extend(def.effect.args.iter().cloned());
        if let Some(f) = &def.effect.fix {
            plan.fixes.push(f.clone());
        }
    }
    // your own variables come last, so they win
    match words(&r.text("env")) {
        Ok(ws) => {
            for w in ws {
                match w.split_once('=') {
                    Some((k, v)) if is_assignment(&w) => set(k, v.into()),
                    _ => plan.notes.push(format!("environment: “{w}” isn't VAR=value")),
                }
            }
        }
        Err(e) => plan.notes.push(format!("environment: {e}")),
    }

    // VSync, forced underneath the game: by MangoHud when it's loaded, else by DXVK
    let mangohud = r.on("mangohud");
    let vsync = r.text("vsync");
    let mut mh_extra: Vec<String> = Vec::new();
    match vsync.as_str() {
        "off" | "on" | "fast" if mangohud => {
            mh_extra.push(format!("vsync={}", match vsync.as_str() { "off" => 1, "on" => 3, _ => 2 }));
            if vsync != "fast" {
                mh_extra.push(format!("gl_vsync={}", if vsync == "on" { 1 } else { 0 }));
            }
        }
        "off" | "on" | "fast" => {
            let opt = match vsync.as_str() { "off" => "dxgi.syncInterval=0", "on" => "dxgi.syncInterval=1", _ => "dxgi.syncInterval=0;dxgi.tearFree=True" };
            let joined = match env.iter().rev().find(|(k, _)| k == "DXVK_CONFIG") {
                Some((_, old)) if !old.is_empty() => format!("{old};{opt}"),
                _ => opt.to_string(),
            };
            env.push(("DXVK_CONFIG".into(), joined));
        }
        _ => {}
    }
    // MangoHud hidden (Right Shift+F12 shows it). read_cfg keeps ~/.config/MangoHud/MangoHud.conf,
    // and a MANGOHUD_CONFIG of yours stays in front.
    if mangohud && (r.on("mangohud_hidden") || !mh_extra.is_empty()) {
        let existing = env.iter().rev().find(|(k, _)| k == "MANGOHUD_CONFIG").map(|(_, v)| v.clone()).or_else(|| host.mangohud_config.clone());
        let mut parts: Vec<String> = existing.into_iter().filter(|s| !s.is_empty()).collect();
        parts.push("read_cfg".into());
        if r.on("mangohud_hidden") {
            parts.push("no_display".into());
        }
        parts.extend(mh_extra);
        env.push(("MANGOHUD_CONFIG".into(), parts.join(",")));
    }

    // the mouse: game-mouse lowers the DPI while the game is focused, or Gamescope multiplies it
    let mut mouse = r.text("mouse");
    if mouse == "gamescope" && !gamescope {
        plan.notes.push("slowing the mouse through Gamescope needs Gamescope on: leaving the mouse alone".into());
        mouse = "off".into();
    }
    if host.game_mouse
        && let Some(section) = info.mouse_section.clone()
    {
        plan.mouse = Some(MouseAction { section, skip: mouse != "dpi", start_service: mouse == "dpi" });
    } else if mouse == "dpi" {
        plan.notes.push(if host.game_mouse { "the DPI switch needs the game's game-mouse section".into() } else { "the DPI switch needs game-mouse".into() });
    }

    // the wrappers, from the inside out
    let mut argv = args;
    if r.on("obs_capture") {
        argv.insert(0, "obs-gamecapture".into());
    }
    if mangohud {
        argv.insert(0, "mangohud".into());
    }
    if gamescope {
        if r.on("clipboard") {
            match &host.clipboard_helper {
                Some(helper) => {
                    // Gamescope unsets WAYLAND_DISPLAY inside; the helper gets the desktop's this way
                    env.push(("GAMESCOPE_CLIPBOARD_HOST".into(), host.wayland_display.clone().unwrap_or_default()));
                    argv.splice(0..0, [helper.display().to_string(), "--".into()]);
                }
                None => plan.notes.push("paste from the desktop needs gamescope-clipboard".into()),
            }
        }
        let mut g = vec!["gamescope".to_string()];
        let screen = screens.iter().max_by_key(|s| u64::from(s.width) * u64::from(s.height));
        let out = r.text("gamescope_output");
        let size = if out.is_empty() || out == "native" { screen.map(|s| (s.width, s.height)) } else { parse_size(&out) };
        match size {
            Some((w, h)) => {
                g.extend(["-W".into(), w.to_string(), "-H".into(), h.to_string()]);
                let scale = r.num("render_scale");
                if scale > 0.0 && scale < 100.0 {
                    let (rw, rh) = render_size(w, h, scale);
                    g.extend(["-w".into(), rw.to_string(), "-h".into(), rh.to_string()]);
                }
            }
            None => plan.notes.push("Gamescope's output: the screen's size isn't known, Gamescope picks its own".into()),
        }
        let hz = r.num("refresh_rate");
        let hz = if hz > 0.0 { Some(hz as u32) } else { screen.map(|s| s.refresh).filter(|r| *r > 0) };
        if let Some(hz) = hz {
            g.extend(["-r".into(), hz.to_string()]);
        }
        if r.on("fullscreen") {
            g.push("-f".into());
        }
        if size.is_some() && r.num("render_scale") > 0.0 && r.num("render_scale") < 100.0 {
            match r.text("upscaler").as_str() {
                "integer" => g.extend(["-S".into(), "integer".into()]),
                "" => {}
                f => g.extend(["-F".into(), f.into()]),
            }
        }
        if r.on("adaptive_sync") {
            g.push("--adaptive-sync".into());
        }
        match words(&r.text("gamescope_args")) {
            Ok(w) => g.extend(w),
            Err(e) => plan.notes.push(format!("Gamescope options: {e}")),
        }
        if mouse == "gamescope" {
            g.extend(["-s".into(), fmt_num(r.num("gamescope_mouse"))]);
        }
        g.push("--".into());
        argv.splice(0..0, g);
    }
    // Run before: VAR=value words set variables, the rest runs in front of everything
    match words(&r.text("run_before")) {
        Ok(ws) => {
            let split = ws.iter().position(|w| !is_assignment(w)).unwrap_or(ws.len());
            for w in &ws[..split] {
                let (k, v) = w.split_once('=').expect("an assignment");
                env.push((k.into(), v.into()));
            }
            argv.splice(0..0, ws[split..].iter().cloned());
        }
        Err(e) => plan.notes.push(format!("run before: {e}")),
    }

    if r.on("gpu_check") {
        plan.gpu_check = Some(r.values.get("vram_warn").and_then(Value::as_i64).unwrap_or(0));
    }
    plan.argv = argv;
    plan.env = env;
    plan
}

fn or_unknown(s: &str) -> &str {
    if s.is_empty() { "not known" } else { s }
}

fn fmt_num(x: f64) -> String {
    if x.fract() == 0.0 { format!("{}", x as i64) } else { format!("{x}") }
}

/// A command as you'd type it in a shell.
pub fn quote(argv: &[String]) -> String {
    shell_words::join(argv)
}

/// The launch log hides one-time login codes (Epic's, passed by legendary).
pub fn redact(arg: &str) -> String {
    let lower = arg.to_ascii_lowercase();
    for key in ["-auth_password=", "-auth_code=", "--auth-code="] {
        if lower.starts_with(key) {
            return format!("{}…", &arg[..key.len()]);
        }
    }
    arg.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AppConfig, GameConfig};
    use crate::paths::Paths;

    fn cmd() -> Vec<String> {
        ["/s/reaper", "SteamLaunch", "AppId=4508340", "--", "/p/proton", "waitforexitandrun", "/g/NTE.exe"].iter().map(|s| s.to_string()).collect()
    }

    fn plan_for(values: &[(&str, Value)], engine: &str, host: &Host) -> Plan {
        let d = tempfile::tempdir().unwrap();
        let paths = Paths { config: d.path().join("c"), data: d.path().join("d"), state: d.path().join("s"), home: d.path().into() };
        let app = AppConfig::load(&paths).unwrap();
        let info = GameInfo { name: "NTE".into(), engine: Some(engine.into()), mouse_section: Some("NTE".into()), ..Default::default() };
        let mut game = GameConfig::new(&paths, "nte", info.clone());
        for (k, v) in values {
            game.set(k, Some(v.clone()));
        }
        let schema = Schema::for_game(None);
        let r = crate::resolve::resolve(&schema, &app, &game, None);
        build(&schema, &r, &info, &cmd(), &[], host)
    }

    /// NTE as nte-launch starts it: the same command, the same variables.
    #[test]
    fn nte_like_nte_launch() {
        let host = Host { game_mouse: true, ..Default::default() };
        let p = plan_for(
            &[
                ("gamescope", true.into()),
                ("gamescope_output", "3440x1440".into()),
                ("render_scale", Value::Int(75)),
                ("refresh_rate", Value::Int(160)),
                ("adaptive_sync", true.into()),
                ("upscaler", "fsr".into()),
                ("mangohud", true.into()),
                ("obs_capture", true.into()),
                ("mouse", "dpi".into()),
                ("graphics_api", "d3d11".into()),
                ("dlss_preset", "K".into()),
                ("low_latency", true.into()),
                ("frame_cap", Value::Int(60)),
                ("run_before", "taskset -c 0-15 gamemoderun".into()),
            ],
            "unreal",
            &host,
        );
        let expect = "taskset -c 0-15 gamemoderun gamescope -W 3440 -H 1440 -w 2580 -h 1080 -r 160 -f -F fsr --adaptive-sync -- mangohud obs-gamecapture /s/reaper SteamLaunch AppId=4508340 -- /p/proton waitforexitandrun /g/NTE.exe -d3d11";
        assert_eq!(p.argv.join(" "), expect);
        assert_eq!(p.env_value("MANGOHUD_CONFIG"), Some("read_cfg,no_display"));
        assert_eq!(p.env_value("DXVK_NVAPI_DRS_NGX_DLSS_SR_OVERRIDE_RENDER_PRESET_SELECTION"), Some("render_preset_k"));
        assert_eq!(p.env_value("DXVK_FRAME_RATE"), Some("60"));
        assert_eq!(p.mouse, Some(MouseAction { section: "NTE".into(), skip: false, start_service: true }));
        assert!(p.notes.is_empty(), "{:?}", p.notes);
    }

    #[test]
    fn api_replaces_steams_and_vsync_goes_through_mangohud_or_dxvk() {
        let mut c = cmd();
        c.push("-force-d3d11".into());
        let d = tempfile::tempdir().unwrap();
        let paths = Paths { config: d.path().join("c"), data: d.path().join("d"), state: d.path().join("s"), home: d.path().into() };
        let app = AppConfig::load(&paths).unwrap();
        let info = GameInfo { engine: Some("unity".into()), ..Default::default() };
        let mut game = GameConfig::new(&paths, "x", info.clone());
        game.set("graphics_api", Some("vulkan".into()));
        game.set("vsync", Some("off".into()));
        let schema = Schema::for_game(None);
        let r = crate::resolve::resolve(&schema, &app, &game, None);
        let p = build(&schema, &r, &info, &c, &[], &Host::default());
        assert!(!p.argv.contains(&"-force-d3d11".to_string()));
        assert_eq!(p.argv.last().map(String::as_str), Some("-force-vulkan"));
        assert_eq!(p.env_value("DXVK_CONFIG"), Some("dxgi.syncInterval=0"));

        let p = plan_for(&[("mangohud", true.into()), ("mangohud_hidden", false.into()), ("vsync", "on".into())], "unreal", &Host::default());
        assert_eq!(p.env_value("MANGOHUD_CONFIG"), Some("read_cfg,vsync=3,gl_vsync=1"));
    }

    #[test]
    fn defaults_change_nothing() {
        let p = plan_for(&[], "unreal", &Host::default());
        assert_eq!(p.argv, cmd());
        assert!(p.env.is_empty(), "{:?}", p.env);
        assert!(p.gpu_check.is_none() && p.fixes.is_empty() && p.mouse.is_none());
    }

    #[test]
    fn native_output_and_refresh_follow_the_screen() {
        let d = tempfile::tempdir().unwrap();
        let paths = Paths { config: d.path().join("c"), data: d.path().join("d"), state: d.path().join("s"), home: d.path().into() };
        let app = AppConfig::load(&paths).unwrap();
        let info = GameInfo::default();
        let mut game = GameConfig::new(&paths, "x", info.clone());
        game.set("gamescope", Some(true.into()));
        game.set("render_scale", Some(Value::Int(50)));
        game.set("upscaler", Some("integer".into()));
        game.set("mouse", Some("gamescope".into()));
        let schema = Schema::for_game(None);
        let r = crate::resolve::resolve(&schema, &app, &game, None);
        let screens = [Screen { name: "DP-2".into(), description: String::new(), width: 1920, height: 1080, refresh: 60 }];
        let p = build(&schema, &r, &info, &["/game".to_string()], &screens, &Host::default());
        assert_eq!(p.argv.join(" "), "gamescope -W 1920 -H 1080 -w 960 -h 540 -r 60 -f -S integer -s 0.2 -- /game");
    }

    #[test]
    fn heroic_wrappers_and_run_before_variables() {
        let p = {
            let d = tempfile::tempdir().unwrap();
            let paths = Paths { config: d.path().join("c"), data: d.path().join("d"), state: d.path().join("s"), home: d.path().into() };
            let app = AppConfig::load(&paths).unwrap();
            let info = GameInfo::default();
            let mut game = GameConfig::new(&paths, "rl", info.clone());
            game.set("strip_wrappers", Some(true.into()));
            game.set("run_before", Some("PULSE_LATENCY_MSEC=60 gamemoderun".into()));
            game.set("env", Some("A=1 'B=two words'".into()));
            let schema = Schema::for_game(None);
            let r = crate::resolve::resolve(&schema, &app, &game, None);
            let incoming: Vec<String> = ["mangohud", "--dlsym", "gamemoderun", "umu-run", "Launcher.exe"].iter().map(|s| s.to_string()).collect();
            build(&schema, &r, &info, &incoming, &[], &Host::default())
        };
        assert_eq!(p.argv.join(" "), "gamemoderun umu-run Launcher.exe");
        assert_eq!(p.env_value("PULSE_LATENCY_MSEC"), Some("60"));
        assert_eq!(p.env_value("B"), Some("two words"));
        assert_eq!(redact("-AUTH_PASSWORD=abc123"), "-AUTH_PASSWORD=…");
    }

    #[test]
    fn render_sizes() {
        assert_eq!(render_size(3440, 1440, 75.0), (2580, 1080));
        assert_eq!(render_size(3440, 1440, 85.0), (2924, 1224));
        assert_eq!(parse_size("3440×1440"), Some((3440, 1440)));
        assert_eq!(parse_size("wide"), None);
    }
}
