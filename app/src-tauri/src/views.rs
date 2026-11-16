//! What the window asks for and changes. Every change loads the file fresh, changes the one value,
//! saves it (keeping comments and a backup) and answers with the game as it now is.

use crate::art::{self, Kind};
use gset_core::config::{self, AppConfig, GameConfig, NewGames, Preset};
use gset_core::plan::{self, Host};
use gset_core::profile::{self, Change, Profiles};
use gset_core::resolve::{self, PresetWhy, Source};
use gset_core::runner::Runner;
use gset_core::schema::{self, Schema, SettingDef};
use gset_core::screen::{self, Screen};
use gset_core::steam::{self, SteamState};
use gset_core::value::Value;
use gset_core::{Paths, slug};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub struct Ctx {
    pub paths: Paths,
    pub app: AppConfig,
    pub profiles: Profiles,
    pub steam_root: PathBuf,
}

pub fn ctx() -> Result<Ctx, String> {
    let paths = Paths::new();
    let app = AppConfig::load(&paths)?;
    let profiles = Profiles::load(&profile::dirs(&paths));
    let steam_root = steam::steam_root(&paths.home);
    Ok(Ctx {
        paths,
        app,
        profiles,
        steam_root,
    })
}

#[derive(Serialize)]
pub struct Art {
    capsule: Option<String>,
    header: Option<String>,
    hero: Option<String>,
    logo: Option<String>,
}

fn art_for(steam_root: &Path, id: Option<u64>) -> Art {
    let get = |k: &str| {
        id.and_then(|id| art::find(steam_root, id, Kind::parse(k)?).map(|_| art::url(id, k)))
    };
    Art {
        capsule: get("capsule"),
        header: get("header"),
        hero: get("hero"),
        logo: get("logo"),
    }
}

/// What runs the game, for the line under its name: Proton's version, Sober, umu-run's Proton…
fn runner(state: &SteamState, g: &GameConfig) -> Option<String> {
    if g.info.script.is_some() {
        return Some("its own script".into());
    }
    match Runner::of(&g.info) {
        Runner::Command => {
            let id = g.info.steam?;
            state.compat_tool.get(&id).map(|t| pretty_tool(t))
        }
        Runner::Umu => {
            let proton = g
                .values
                .get("proton")
                .map(|v| v.to_string())
                .unwrap_or_default();
            let name = proton
                .trim_end_matches('/')
                .rsplit('/')
                .next()
                .unwrap_or("");
            Some(if name.is_empty() {
                "umu-run".into()
            } else {
                format!("umu-run · {name}")
            })
        }
        r => Some(r.title().into()),
    }
}

/// "dwproton-11.0-14" stays as it is; "proton_experimental" becomes "Proton Experimental".
fn pretty_tool(t: &str) -> String {
    if let Some(rest) = t.strip_prefix("proton_") {
        let rest = rest.replace('_', " ");
        let mut c = rest.chars();
        return format!(
            "Proton {}",
            c.next()
                .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
                .unwrap_or_default()
        );
    }
    t.to_string()
}

fn modified(path: &Path) -> Option<u64> {
    std::fs::metadata(path)
        .ok()?
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs())
}

// ------------------------------------------------------------------------------------------ the library

#[derive(Serialize)]
pub struct LibraryGame {
    key: String,
    name: String,
    via: Option<String>,
    runner: Option<String>,
    steam: Option<u64>,
    engine: Option<String>,
    aliases: Vec<String>,
    art: Art,
    script: bool,
    profile: bool,
    update: bool,
    suggestions: usize,
    last_played: Option<u64>,
    changed: Option<u64>,
}

#[tauri::command]
pub fn library() -> Result<Vec<LibraryGame>, String> {
    let mut c = ctx()?;
    let (games, _problems) = config::all_games(&c.paths);
    if c.app.auto_profile_updates {
        let mut applied = false;
        for g in &games {
            if let Some(id) = &g.info.profile
                && profile::pending(&c.paths, id, c.profiles.get(id)).is_some()
            {
                applied |= profile::apply_pending(&c.paths, id).is_ok();
            }
        }
        if applied {
            c.profiles = Profiles::load(&profile::dirs(&c.paths));
        }
    }
    let state = steam::state(&c.steam_root);
    Ok(games
        .iter()
        .map(|g| {
            let p = g.info.profile.as_deref().and_then(|id| c.profiles.get(id));
            let schema = Schema::of(p, Runner::of(&g.info));
            let base = resolve::resolve(&schema, &c.app, g, None);
            LibraryGame {
                key: g.key.clone(),
                name: g.info.name.clone(),
                via: g.info.via.clone(),
                runner: runner(&state, g),
                steam: g.info.steam,
                engine: g.info.engine.clone(),
                aliases: p.map(|p| p.aliases.clone()).unwrap_or_default(),
                art: art_for(&c.steam_root, g.info.steam),
                script: g.info.script.is_some(),
                profile: p.is_some(),
                update: g
                    .info
                    .profile
                    .as_deref()
                    .is_some_and(|id| profile::pending(&c.paths, id, c.profiles.get(id)).is_some()),
                suggestions: resolve::open_suggestions(&schema, p, g, &base).len(),
                last_played: g
                    .info
                    .steam
                    .and_then(|id| state.last_played.get(&id).copied()),
                changed: modified(&g.path),
            }
        })
        .collect())
}

// ------------------------------------------------------------------------------------------ one game

#[derive(Serialize)]
pub struct CommandView {
    argv: Vec<String>,
    env: Vec<(String, String)>,
    notes: Vec<String>,
}

#[derive(Serialize)]
pub struct ViewValues {
    values: BTreeMap<String, Value>,
    source: BTreeMap<String, Source>,
    command: CommandView,
}

#[derive(Serialize)]
pub struct SuggestionView {
    value: Value,
    why: String,
    dismissed: bool,
}

#[derive(Serialize)]
pub struct PresetView {
    id: String,
    name: String,
    screen: Option<String>,
    ratio: Option<String>,
    values: BTreeMap<String, Value>,
}

fn presets(app: &AppConfig) -> Vec<PresetView> {
    app.presets
        .iter()
        .map(|p| PresetView {
            id: p.id.clone(),
            name: p.name.clone(),
            screen: p.screen.clone(),
            ratio: p.ratio(),
            values: p.values.clone(),
        })
        .collect()
}

#[derive(Serialize)]
pub struct InfoView {
    name: String,
    steam: Option<u64>,
    via: Option<String>,
    engine: Option<String>,
    profile: Option<String>,
    mouse_section: Option<String>,
    script: Option<String>,
    runner: Option<String>,
    /// GSet starts the game itself this way: the title of its "How it starts" settings
    starts_with: Option<&'static str>,
}

#[derive(Serialize)]
pub struct ProfileView {
    id: String,
    version: u32,
    notes: Vec<String>,
}

#[derive(Serialize)]
pub struct UpdateView {
    from: u32,
    to: u32,
    changes: Vec<Change>,
}

#[derive(Serialize)]
pub struct Orphan {
    key: String,
    why: String,
}

#[derive(Serialize)]
pub struct InUse {
    preset: Option<String>,
    why: PresetWhy,
}

#[derive(Serialize)]
pub struct LaunchView {
    /// steam, shortcut or other (Heroic and the like)
    kind: &'static str,
    /// where the line goes: launch_options, target (a shortcut's) or wrapper (Heroic's)
    field: &'static str,
    /// what to put there
    line: String,
    /// what that field says now, when GSet can read it (Steam's files)
    now: Option<String>,
    /// whether it's right
    ok: Option<bool>,
}

#[derive(Serialize)]
pub struct GameView {
    key: String,
    path: String,
    info: InfoView,
    art: Art,
    profile: Option<ProfileView>,
    update: Option<UpdateView>,
    groups: &'static [schema::Group],
    settings: Vec<SettingDef>,
    views: BTreeMap<String, ViewValues>,
    suggestions: BTreeMap<String, SuggestionView>,
    open: Vec<String>,
    orphans: Vec<Orphan>,
    presets: Vec<PresetView>,
    in_use: InUse,
    pinned: Option<String>,
    launch: LaunchView,
    screens: Vec<Screen>,
    last_played: Option<u64>,
}

pub fn launcher_path(paths: &Paths) -> Option<PathBuf> {
    crate::launcher::path(paths)
}

fn launch_kind(g: &GameConfig) -> &'static str {
    match g.info.steam {
        _ if g.info.via.as_deref().is_some_and(|v| v.contains("Heroic")) => "other",
        Some(id) if id > u64::from(u32::MAX >> 1) => "shortcut",
        Some(_) => "steam",
        None => "other",
    }
}

fn starts_with_gset(line: &str) -> bool {
    line.split_whitespace()
        .next()
        .is_some_and(|w| w == "gset" || w.ends_with("/gset"))
}

/// What Steam should have for the game, and whether it does. A game with a script of its own is a
/// shortcut whose Target is that script: GSet needn't be in Steam at all. Any other game has gset
/// in its launch options (a shortcut's too: never in its Target, where Steam doesn't fill in
/// %command%).
fn launch_view(
    c: &Ctx,
    g: &GameConfig,
    state: &SteamState,
    kind: &'static str,
    launcher: &str,
) -> LaunchView {
    let shortcut = (kind == "shortcut")
        .then(|| g.info.steam.and_then(|id| state.shortcuts.get(&id)))
        .flatten();
    let gset_line = format!("{launcher} %command%");
    // a game GSet starts itself: the shortcut's Target is gset, and Steam's ids tell it which game
    if kind == "shortcut" && g.info.script.is_none() && Runner::of(&g.info) != Runner::Command {
        let now = shortcut.map(|s| s.target.clone());
        return LaunchView {
            kind,
            field: "target",
            ok: now.as_deref().map(starts_with_gset),
            now,
            line: launcher.to_string(),
        };
    }
    match (kind, &g.info.script) {
        ("shortcut", Some(script)) => {
            let script = c.paths.expand(script);
            let now = shortcut.map(|s| s.target.clone());
            let ok = now.as_ref().map(|t| {
                std::fs::canonicalize(t).ok() == std::fs::canonicalize(&script).ok()
                    || Path::new(t) == script
            });
            LaunchView {
                kind,
                field: "target",
                line: script.display().to_string(),
                now,
                ok,
            }
        }
        ("shortcut", None) => LaunchView {
            kind,
            field: "launch_options",
            ok: shortcut.map(|s| starts_with_gset(&s.launch_options)),
            now: shortcut.map(|s| s.launch_options.clone()),
            line: gset_line,
        },
        ("steam", _) => {
            let now = g
                .info
                .steam
                .and_then(|id| state.launch_options.get(&id).cloned());
            LaunchView {
                kind,
                field: "launch_options",
                ok: Some(now.as_deref().is_some_and(starts_with_gset)),
                now,
                line: gset_line,
            }
        }
        _ => LaunchView {
            kind,
            field: "wrapper",
            line: format!("{launcher} --game={}", g.key),
            now: None,
            ok: None,
        },
    }
}

pub fn game_view(c: &Ctx, g: &GameConfig, state: &SteamState, screens: &[Screen]) -> GameView {
    let p = g.info.profile.as_deref().and_then(|id| c.profiles.get(id));
    let schema = Schema::of(p, Runner::of(&g.info));
    let host = Host::detect(&c.paths);
    // Steam’s part of the command, as a placeholder; a game GSet starts itself gets none
    let incoming: Vec<String> = if Runner::of(&g.info) == Runner::Command {
        vec!["%command%".to_string()]
    } else {
        Vec::new()
    };
    let mut views = BTreeMap::new();
    let ids = std::iter::once(None).chain(c.app.presets.iter().map(|p| Some(p.id.as_str())));
    for id in ids {
        let r = resolve::resolve(&schema, &c.app, g, id);
        let plan = plan::build(&schema, &r, &g.info, &incoming, screens, &host);
        views.insert(
            id.unwrap_or("base").to_string(),
            ViewValues {
                values: r.values,
                source: r.source,
                command: CommandView {
                    argv: plan.argv,
                    env: plan.env,
                    notes: plan.notes,
                },
            },
        );
    }
    let base = resolve::resolve(&schema, &c.app, g, None);
    let suggestions = p
        .map(|p| {
            p.suggest
                .iter()
                .map(|(k, s)| {
                    (
                        k.clone(),
                        SuggestionView {
                            value: s.value.clone(),
                            why: s.why.clone(),
                            dismissed: g.dismissed.get(k).is_some_and(|d| d.same(&s.value)),
                        },
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    let orphans = schema
        .settings
        .iter()
        .filter(|d| d.hidden && g.values.get(&d.key).is_some_and(|v| !v.same(&d.default)))
        .map(|d| Orphan {
            key: d.key.clone(),
            why: p
                .and_then(|p| p.removed.get(&d.key).cloned())
                .unwrap_or_default(),
        })
        .collect();
    let update = g.info.profile.as_deref().and_then(|id| {
        let new = profile::pending(&c.paths, id, c.profiles.get(id))?;
        let old = c.profiles.get(id).cloned().unwrap_or_default();
        Some(UpdateView {
            from: old.version,
            to: new.version,
            changes: profile::diff(&old, &new),
        })
    });
    let (preset, why) = resolve::pick_preset(&c.app, g, screens, None);
    let kind = launch_kind(g);
    let launcher = launcher_path(&c.paths)
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "gset".into());
    let launch = launch_view(c, g, state, kind, &launcher);
    GameView {
        key: g.key.clone(),
        path: g.path.display().to_string(),
        info: InfoView {
            name: g.info.name.clone(),
            steam: g.info.steam,
            via: g.info.via.clone(),
            engine: g.info.engine.clone(),
            profile: g.info.profile.clone(),
            mouse_section: g.info.mouse_section.clone(),
            script: g.info.script.clone(),
            runner: runner(state, g),
            starts_with: Some(Runner::of(&g.info))
                .filter(|r| *r != Runner::Command)
                .map(Runner::title),
        },
        art: art_for(&c.steam_root, g.info.steam),
        profile: p.map(|p| ProfileView {
            id: p.id.clone(),
            version: p.version,
            notes: p.notes.clone(),
        }),
        update,
        groups: schema::GROUPS,
        open: resolve::open_suggestions(&schema, p, g, &base),
        settings: schema.settings,
        views,
        suggestions,
        orphans,
        presets: presets(&c.app),
        in_use: InUse { preset, why },
        pinned: g.pinned.clone(),
        launch,
        screens: screens.to_vec(),
        last_played: g
            .info
            .steam
            .and_then(|id| state.last_played.get(&id).copied()),
    }
}

fn load(c: &Ctx, key: &str) -> Result<GameConfig, String> {
    if key.is_empty() || key.contains('/') || key.starts_with('.') {
        return Err(format!("no game called {key}"));
    }
    GameConfig::load(&GameConfig::path_for(&c.paths, key))
}

fn answer(c: &Ctx, g: &GameConfig) -> GameView {
    game_view(c, g, &steam::state(&c.steam_root), &screen::screens())
}

#[tauri::command]
pub fn game(key: String) -> Result<GameView, String> {
    let c = ctx()?;
    let g = load(&c, &key)?;
    Ok(answer(&c, &g))
}

fn json_value(v: serde_json::Value) -> Option<Value> {
    match v {
        serde_json::Value::Bool(b) => Some(Value::Bool(b)),
        serde_json::Value::Number(n) => n
            .as_i64()
            .map(Value::Int)
            .or_else(|| n.as_f64().map(Value::Float)),
        serde_json::Value::String(s) => Some(Value::Str(s)),
        _ => None,
    }
}

/// Sets (or with no value, removes) a setting. In a preset's view the change goes to this game's own
/// version of the preset, or with `everywhere` to the preset itself, for every game.
#[tauri::command]
pub fn set_value(
    key: String,
    view: String,
    setting: String,
    value: Option<serde_json::Value>,
    everywhere: Option<bool>,
) -> Result<GameView, String> {
    let mut c = ctx()?;
    let mut g = load(&c, &key)?;
    let v = value.and_then(json_value);
    if view == "base" {
        g.set(&setting, v);
        g.save()?;
    } else if everywhere.unwrap_or(false) {
        let mut p = c
            .app
            .preset(&view)
            .cloned()
            .ok_or_else(|| format!("no preset called {view}"))?;
        match v {
            Some(v) => p.values.insert(setting.clone(), v),
            None => p.values.remove(&setting),
        };
        c.app.put_preset(p);
        c.app.save()?;
        if g.presets
            .get(&view)
            .is_some_and(|m| m.contains_key(&setting))
        {
            g.set_preset(&view, &setting, None);
            g.save()?;
        }
    } else {
        g.set_preset(&view, &setting, v);
        g.save()?;
    }
    Ok(answer(&c, &g))
}

#[tauri::command]
pub fn use_suggestions(key: String, settings: Vec<String>) -> Result<GameView, String> {
    let c = ctx()?;
    let mut g = load(&c, &key)?;
    let p = g
        .info
        .profile
        .as_deref()
        .and_then(|id| c.profiles.get(id))
        .ok_or("this game has no profile")?;
    for k in settings {
        if let Some(s) = p.suggestion(&k) {
            g.set(&k, Some(s.value.clone()));
        }
    }
    g.save()?;
    Ok(answer(&c, &g))
}

#[tauri::command]
pub fn dismiss_suggestion(
    key: String,
    setting: String,
    hide: Option<bool>,
) -> Result<GameView, String> {
    let c = ctx()?;
    let mut g = load(&c, &key)?;
    let value = if hide.unwrap_or(true) {
        let p = g
            .info
            .profile
            .as_deref()
            .and_then(|id| c.profiles.get(id))
            .ok_or("this game has no profile")?;
        Some(
            p.suggestion(&setting)
                .ok_or("no such suggestion")?
                .value
                .clone(),
        )
    } else {
        None
    };
    g.dismiss(&setting, value);
    g.save()?;
    Ok(answer(&c, &g))
}

#[tauri::command]
pub fn pin(key: String, preset: Option<String>) -> Result<GameView, String> {
    let c = ctx()?;
    let mut g = load(&c, &key)?;
    g.pin(preset.as_deref());
    g.save()?;
    Ok(answer(&c, &g))
}

#[tauri::command]
pub fn apply_update(key: String) -> Result<GameView, String> {
    let c = ctx()?;
    let g = load(&c, &key)?;
    let id = g.info.profile.clone().ok_or("this game has no profile")?;
    profile::apply_pending(&c.paths, &id)?;
    let c = ctx()?;
    Ok(answer(&c, &g))
}

#[tauri::command]
pub fn undo_update(key: String) -> Result<GameView, String> {
    let c = ctx()?;
    let g = load(&c, &key)?;
    let id = g.info.profile.clone().ok_or("this game has no profile")?;
    profile::undo_apply(&c.paths, &id)?;
    let c = ctx()?;
    Ok(answer(&c, &g))
}

#[tauri::command]
pub fn play(key: String) -> Result<(), String> {
    let c = ctx()?;
    let g = load(&c, &key)?;
    let id = match (launch_kind(&g), g.info.steam) {
        ("other", _) | (_, None) => {
            return Err(format!(
                "{} isn't started by Steam: start it from its own launcher.",
                g.info.name
            ));
        }
        (_, Some(id)) => steam::run_id(id),
    };
    std::process::Command::new("xdg-open")
        .arg(format!("steam://rungameid/{id}"))
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("couldn't ask Steam to start it: {e}"))
}

// ------------------------------------------------------------------------------------------ GSet's own settings

#[derive(Serialize)]
pub struct AppView {
    version: &'static str,
    check_updates: bool,
    auto_profile_updates: bool,
    new_games: &'static str,
    launcher: Option<String>,
    presets: Vec<PresetView>,
    screens: Vec<Screen>,
    config_dir: String,
    profiles: usize,
    problems: Vec<String>,
}

fn app_view(c: &Ctx) -> AppView {
    let (_, problems) = config::all_games(&c.paths);
    AppView {
        version: env!("CARGO_PKG_VERSION"),
        check_updates: c.app.check_updates,
        auto_profile_updates: c.app.auto_profile_updates,
        new_games: if c.app.new_games == NewGames::Defaults {
            "defaults"
        } else {
            "suggested"
        },
        launcher: launcher_path(&c.paths).map(|p| p.display().to_string()),
        presets: presets(&c.app),
        screens: screen::screens(),
        config_dir: c.paths.config.display().to_string(),
        profiles: c.profiles.by_id.len(),
        problems: problems
            .into_iter()
            .chain(
                c.profiles
                    .problems
                    .iter()
                    .map(|(f, e)| format!("{}: {e}", f.display())),
            )
            .collect(),
    }
}

#[tauri::command]
pub fn app_settings() -> Result<AppView, String> {
    Ok(app_view(&ctx()?))
}

#[tauri::command]
pub fn set_app_option(key: String, value: serde_json::Value) -> Result<AppView, String> {
    if !matches!(
        key.as_str(),
        "check_updates" | "auto_profile_updates" | "new_games"
    ) {
        return Err(format!("no option called {key}"));
    }
    let mut c = ctx()?;
    c.app
        .set_option(&key, json_value(value).ok_or("expected a value")?);
    c.app.save()?;
    Ok(app_view(&c))
}

/// Makes a preset, or renames one and changes its screen rule (its values stay).
#[tauri::command]
pub fn save_preset(
    id: Option<String>,
    name: String,
    screen: Option<String>,
) -> Result<AppView, String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("a preset needs a name".into());
    }
    let screen = screen
        .map(|s| s.trim().replace(['×', ' '], "x").replace("xx", "x"))
        .filter(|s| !s.is_empty());
    if let Some(s) = &screen
        && plan::parse_size(s).is_none()
    {
        return Err(format!("“{s}”: a screen is WIDTHxHEIGHT"));
    }
    let mut c = ctx()?;
    let preset = match id.as_deref().and_then(|id| c.app.preset(id)).cloned() {
        Some(p) => Preset { name, screen, ..p },
        None => {
            let base = {
                let s = slug(&name);
                if s.is_empty() {
                    "preset".to_string()
                } else {
                    s
                }
            };
            let mut id = base.clone();
            let mut n = 2;
            while c.app.preset(&id).is_some() || id == "base" {
                id = format!("{base}-{n}");
                n += 1;
            }
            Preset {
                id,
                name,
                screen,
                values: BTreeMap::new(),
            }
        }
    };
    c.app.put_preset(preset);
    c.app.save()?;
    Ok(app_view(&c))
}

#[tauri::command]
pub fn delete_preset(id: String) -> Result<AppView, String> {
    let mut c = ctx()?;
    c.app.remove_preset(&id);
    c.app.save()?;
    Ok(app_view(&c))
}

#[tauri::command]
pub fn theme() -> crate::theme::Theme {
    crate::theme::read()
}

/// For working on the window in a browser: everything the window shows, as JSON files.
pub fn dump_fixture(dir: &Path) -> Result<(), String> {
    let c = ctx()?;
    std::fs::create_dir_all(dir.join("games")).map_err(|e| e.to_string())?;
    let write = |name: &str, v: &dyn erased::Json| {
        std::fs::write(dir.join(name), v.json()).map_err(|e| e.to_string())
    };
    write("library.json", &library()?)?;
    write("app.json", &app_view(&c))?;
    let state = steam::state(&c.steam_root);
    let screens = screen::screens();
    for g in config::all_games(&c.paths).0 {
        write(
            &format!("games/{}.json", g.key),
            &game_view(&c, &g, &state, &screens),
        )?;
    }
    Ok(())
}

mod erased {
    pub trait Json {
        fn json(&self) -> String;
    }
    impl<T: serde::Serialize> Json for T {
        fn json(&self) -> String {
            serde_json::to_string_pretty(self).unwrap_or_default()
        }
    }
}
