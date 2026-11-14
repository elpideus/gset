//! A game's values, from the layers that make them, each overriding the one before:
//!
//! 1. GSet's defaults (they change nothing about how a game starts)
//! 2. your defaults for every game (gset.toml's [defaults])
//! 3. the game's own values
//! 4. the preset in use: its values (gset.toml), then this game's changes to it
//!
//! Profiles are not a layer: they only suggest. A suggestion becomes a value when you use it, or when
//! GSet meets a game for the first time (see `start_values`).

use crate::config::{AppConfig, GameConfig, NewGames};
use crate::profile::Profile;
use crate::schema::Schema;
use crate::screen::Screen;
use crate::value::Value;
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "preset", rename_all = "snake_case")]
pub enum Source {
    Default,
    AllGames,
    Game,
    Preset(String),
    GamePreset(String),
}

#[derive(Clone, Debug)]
pub struct Resolved {
    pub values: BTreeMap<String, Value>,
    pub source: BTreeMap<String, Source>,
    pub preset: Option<String>,
}

impl Resolved {
    pub fn get(&self, key: &str) -> Value {
        self.values.get(key).cloned().unwrap_or(Value::Str(String::new()))
    }
    pub fn on(&self, key: &str) -> bool {
        self.values.get(key).is_some_and(Value::as_bool)
    }
    pub fn text(&self, key: &str) -> String {
        match self.values.get(key) {
            Some(Value::Str(s)) => s.clone(),
            Some(v) => v.to_string(),
            None => String::new(),
        }
    }
    pub fn num(&self, key: &str) -> f64 {
        self.values.get(key).and_then(Value::as_f64).unwrap_or(0.0)
    }
}

pub fn resolve(schema: &Schema, app: &AppConfig, game: &GameConfig, preset: Option<&str>) -> Resolved {
    let mut values = BTreeMap::new();
    let mut source = BTreeMap::new();
    let mut put = |m: &BTreeMap<String, Value>, s: Source| {
        for (k, v) in m {
            values.insert(k.clone(), v.clone());
            source.insert(k.clone(), s.clone());
        }
    };
    put(&schema.defaults(), Source::Default);
    put(&app.defaults, Source::AllGames);
    put(&game.values, Source::Game);
    let mut used = None;
    if let Some(id) = preset
        && let Some(p) = app.preset(id)
    {
        put(&p.values, Source::Preset(id.into()));
        if let Some(own) = game.presets.get(id) {
            put(own, Source::GamePreset(id.into()));
        }
        used = Some(id.to_string());
    }
    values.insert("engine".into(), Value::str(game.info.engine.clone().unwrap_or_default()));
    Resolved { values, source, preset: used }
}

/// Why a preset is in use.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PresetWhy {
    /// `gset --preset=…`
    Asked,
    /// pinned on the game's page
    Pinned,
    /// a screen with this mode is on
    Screen { mode: String },
    /// no preset applies
    None,
}

/// The preset a launch uses: the one asked for, else the game's pinned one, else the first whose
/// screen rule matches a screen that's on.
pub fn pick_preset(app: &AppConfig, game: &GameConfig, screens: &[Screen], asked: Option<&str>) -> (Option<String>, PresetWhy) {
    if let Some(a) = asked {
        if a == "none" || a.is_empty() {
            return (None, PresetWhy::Asked);
        }
        if app.preset(a).is_some() {
            return (Some(a.into()), PresetWhy::Asked);
        }
    }
    if let Some(p) = &game.pinned
        && app.preset(p).is_some()
    {
        return (Some(p.clone()), PresetWhy::Pinned);
    }
    for p in &app.presets {
        if let Some(rule) = &p.screen
            && let Some(s) = screens.iter().find(|s| s.mode() == *rule)
        {
            return (Some(p.id.clone()), PresetWhy::Screen { mode: s.mode() });
        }
    }
    (None, PresetWhy::None)
}

/// The values a game starts with the first time GSet meets it.
pub fn start_values(profile: Option<&Profile>, new_games: NewGames) -> BTreeMap<String, Value> {
    match (profile, new_games) {
        (Some(p), NewGames::Suggested) => p.suggest.iter().map(|(k, s)| (k.clone(), s.value.clone())).collect(),
        _ => BTreeMap::new(),
    }
}

/// Suggestions that differ from the game's value and that you haven't hidden.
pub fn open_suggestions(schema: &Schema, profile: Option<&Profile>, game: &GameConfig, base: &Resolved) -> Vec<String> {
    let Some(p) = profile else { return Vec::new() };
    p.suggest
        .iter()
        .filter(|(k, s)| {
            schema.get(k).is_some_and(|d| !d.hidden)
                && !base.values.get(*k).is_some_and(|v| v.same(&s.value))
                && !game.dismissed.get(*k).is_some_and(|d| d.same(&s.value))
        })
        .map(|(k, _)| k.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{GameInfo, Preset};
    use crate::paths::Paths;

    fn setup() -> (tempfile::TempDir, Paths) {
        let d = tempfile::tempdir().unwrap();
        let p = Paths { config: d.path().join("c"), data: d.path().join("d"), state: d.path().join("s"), home: d.path().into() };
        (d, p)
    }

    #[test]
    fn layers_and_presets() {
        let (_d, paths) = setup();
        let mut app = AppConfig::load(&paths).unwrap();
        app.set_default("mangohud", Some(true.into()));
        app.set_default("frame_cap", Some(Value::Int(120)));
        app.put_preset(Preset { id: "work".into(), name: "Work".into(), screen: Some("1920x1080".into()), values: [("frame_cap".into(), Value::Int(60))].into() });
        let mut game = GameConfig::new(&paths, "nte", GameInfo { name: "NTE".into(), engine: Some("unreal".into()), ..Default::default() });
        game.set("render_scale", Some(Value::Int(75)));
        game.set_preset("work", "render_scale", Some(Value::Int(100)));
        let schema = Schema::for_game(None);

        let base = resolve(&schema, &app, &game, None);
        assert_eq!(base.get("mangohud"), Value::Bool(true));
        assert_eq!(base.source["mangohud"], Source::AllGames);
        assert_eq!(base.get("render_scale"), Value::Int(75));
        assert_eq!(base.get("frame_cap"), Value::Int(120));
        assert_eq!(base.get("engine"), Value::str("unreal"));

        let screens = vec![Screen { name: "DP-2".into(), description: String::new(), width: 1920, height: 1080, refresh: 60 }];
        let (p, why) = pick_preset(&app, &game, &screens, None);
        assert_eq!(p.as_deref(), Some("work"));
        assert_eq!(why, PresetWhy::Screen { mode: "1920x1080".into() });
        let work = resolve(&schema, &app, &game, p.as_deref());
        assert_eq!(work.get("frame_cap"), Value::Int(60));
        assert_eq!(work.get("render_scale"), Value::Int(100));
        assert_eq!(work.source["render_scale"], Source::GamePreset("work".into()));

        let ultrawide = vec![Screen { name: "DP-2".into(), description: String::new(), width: 3440, height: 1440, refresh: 165 }];
        assert_eq!(pick_preset(&app, &game, &ultrawide, None).0, None);
        assert_eq!(pick_preset(&app, &game, &ultrawide, Some("work")).0.as_deref(), Some("work"));
        assert_eq!(pick_preset(&app, &game, &screens, Some("none")).0, None);
        game.pin(Some("work"));
        assert_eq!(pick_preset(&app, &game, &ultrawide, None), (Some("work".into()), PresetWhy::Pinned));
    }

    #[test]
    fn suggestions_never_become_values_by_themselves() {
        let (_d, paths) = setup();
        let app = AppConfig::load(&paths).unwrap();
        let profile = crate::profile::Profile::parse(
            "schema = 1\nid = \"g\"\nversion = 2\nname = \"G\"\n[suggest]\nrender_scale = { value = 67, why = \"faster\" }\nvsync = { value = \"off\" }\n",
        )
        .unwrap();
        let schema = Schema::for_game(Some(&profile));
        let mut game = GameConfig::new(&paths, "g", GameInfo::default());
        for (k, v) in start_values(Some(&profile), crate::config::NewGames::Suggested) {
            game.set(&k, Some(v));
        }
        game.set("render_scale", Some(Value::Int(75)));
        let base = resolve(&schema, &app, &game, None);
        assert_eq!(base.get("render_scale"), Value::Int(75), "your value, not the suggestion");
        assert_eq!(open_suggestions(&schema, Some(&profile), &game, &base), vec!["render_scale".to_string()]);
        game.dismiss("render_scale", Some(Value::Int(67)));
        assert!(open_suggestions(&schema, Some(&profile), &game, &base).is_empty());
        assert!(start_values(Some(&profile), crate::config::NewGames::Defaults).is_empty());
    }
}
