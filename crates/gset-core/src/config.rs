//! Your settings, in TOML files you can also edit by hand:
//!
//! - `~/.config/gset/gset.toml`: GSet's own options, your defaults for every game, and the presets
//! - `~/.config/gset/games/<game>.toml`: one file per game, with its values and its preset changes
//!
//! Saving changes only what was edited: comments, order and formatting stay as they are. The file
//! before each save is kept in a `.backup` folder beside it (the last 20 per file).

use crate::paths::Paths;
use crate::value::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use toml_edit::{DocumentMut, Item, Table};

const KEEP_BACKUPS: usize = 20;

/// What a new game starts with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NewGames {
    /// its profile's suggested values
    Suggested,
    /// GSet's defaults and yours only
    Defaults,
}

#[derive(Clone, Debug)]
pub struct Preset {
    pub id: String,
    pub name: String,
    /// used by itself while a screen with this mode (WIDTHxHEIGHT) is on
    pub screen: Option<String>,
    pub values: BTreeMap<String, Value>,
}

impl Preset {
    /// "21:9", "16:9": from the screen rule, when there is one
    pub fn ratio(&self) -> Option<String> {
        let (w, h) = crate::plan::parse_size(self.screen.as_deref()?)?;
        Some(ratio_name(w, h))
    }
}

/// The usual name of an aspect ratio: 3440×1440 is "21:9", not "43:18".
pub fn ratio_name(w: u32, h: u32) -> String {
    let r = f64::from(w) / f64::from(h);
    // "21:9" is a marketing name: 2560×1080 (2.37), 3440×1440 (2.39) and 3840×1600 (2.4) all go by it
    for (name, x, tol) in [("32:9", 32.0 / 9.0, 0.05), ("21:9", 2.37, 0.06), ("16:9", 16.0 / 9.0, 0.03), ("16:10", 1.6, 0.03), ("4:3", 4.0 / 3.0, 0.03), ("5:4", 1.25, 0.03)] {
        if (r - x).abs() < tol {
            return name.into();
        }
    }
    format!("{:.2}:1", r)
}

#[derive(Debug)]
pub struct AppConfig {
    pub path: PathBuf,
    doc: DocumentMut,
    pub check_updates: bool,
    pub auto_profile_updates: bool,
    pub new_games: NewGames,
    /// your values for every game; a game's own value wins
    pub defaults: BTreeMap<String, Value>,
    pub presets: Vec<Preset>,
}

const APP_HEADER: &str = "\
# GSet's settings. Edit them in GSet, or here: GSet keeps your comments when it saves.
";

impl AppConfig {
    pub fn load(paths: &Paths) -> Result<AppConfig, String> {
        let path = paths.app_file();
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            // the header goes on a table, or it would end up at the bottom of the file
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => format!("{APP_HEADER}\n[app]\n"),
            Err(e) => return Err(format!("{}: {e}", path.display())),
        };
        let doc: DocumentMut = text.parse().map_err(|e| format!("{}: {e}", path.display()))?;
        let app = doc.get("app").and_then(Item::as_table_like);
        let flag = |k: &str, d: bool| app.and_then(|t| t.get(k)).and_then(Item::as_bool).unwrap_or(d);
        let new_games = match app.and_then(|t| t.get("new_games")).and_then(Item::as_str) {
            Some("defaults") => NewGames::Defaults,
            _ => NewGames::Suggested,
        };
        let mut presets = Vec::new();
        if let Some(t) = doc.get("presets").and_then(Item::as_table_like) {
            for (id, item) in t.iter() {
                let Some(p) = item.as_table_like() else { continue };
                presets.push(Preset {
                    id: id.to_string(),
                    name: p.get("name").and_then(Item::as_str).unwrap_or(id).to_string(),
                    screen: p.get("screen").and_then(Item::as_str).filter(|s| !s.is_empty()).map(String::from),
                    values: values_of(p.get("settings")),
                });
            }
        }
        Ok(AppConfig {
            check_updates: flag("check_updates", true),
            auto_profile_updates: flag("auto_profile_updates", false),
            new_games,
            defaults: values_of(doc.get("defaults")),
            presets,
            path,
            doc,
        })
    }

    pub fn preset(&self, id: &str) -> Option<&Preset> {
        self.presets.iter().find(|p| p.id == id)
    }

    pub fn set_option(&mut self, key: &str, v: Value) {
        table(&mut self.doc, &["app"]).insert(key, toml_edit::value(v.to_toml()));
        match key {
            "check_updates" => self.check_updates = v.as_bool(),
            "auto_profile_updates" => self.auto_profile_updates = v.as_bool(),
            "new_games" => self.new_games = if v.as_str() == "defaults" { NewGames::Defaults } else { NewGames::Suggested },
            _ => {}
        }
    }

    pub fn set_default(&mut self, key: &str, v: Option<Value>) {
        set_in(&mut self.doc, &["defaults"], key, v.clone());
        match v {
            Some(v) => self.defaults.insert(key.into(), v),
            None => self.defaults.remove(key),
        };
    }

    /// Adds or replaces a preset (its name, screen rule and values).
    pub fn put_preset(&mut self, p: Preset) {
        let t = table(&mut self.doc, &["presets", &p.id]);
        t.insert("name", toml_edit::value(p.name.as_str()));
        match &p.screen {
            Some(s) => t.insert("screen", toml_edit::value(s.as_str())),
            None => t.remove("screen"),
        };
        let mut st = Table::new();
        for (k, v) in &p.values {
            st.insert(k, toml_edit::value(v.to_toml()));
        }
        t.insert("settings", Item::Table(st));
        match self.presets.iter_mut().find(|x| x.id == p.id) {
            Some(slot) => *slot = p,
            None => self.presets.push(p),
        }
    }

    pub fn text(&self) -> String {
        self.doc.to_string()
    }

    pub fn save(&self) -> Result<(), String> {
        write_with_backup(&self.path, &self.doc.to_string())
    }
}

/// How GSet starts a game.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GameInfo {
    pub name: String,
    /// Steam app id, or a non-Steam shortcut's id
    pub steam: Option<u64>,
    /// the community profile that describes it
    pub profile: Option<String>,
    /// unity, unreal, or empty when not known: how the API and resolution reach the game
    pub engine: Option<String>,
    /// what starts it: Steam, Steam shortcut, Heroic, …
    pub via: Option<String>,
    /// the game's section in game-mouse.conf, for the mouse DPI switch
    pub mouse_section: Option<String>,
    /// a script of yours that starts the game instead of GSet's own launch (the settings then don't apply)
    pub script: Option<String>,
}

#[derive(Debug)]
pub struct GameConfig {
    pub key: String,
    pub path: PathBuf,
    doc: DocumentMut,
    pub info: GameInfo,
    pub values: BTreeMap<String, Value>,
    /// this game's changes to the shared presets
    pub presets: BTreeMap<String, BTreeMap<String, Value>>,
    /// suggestions you hid: the suggested value at the time (a different suggestion shows again)
    pub dismissed: BTreeMap<String, Value>,
    /// a preset used whatever the screen
    pub pinned: Option<String>,
}

impl GameConfig {
    pub fn path_for(paths: &Paths, key: &str) -> PathBuf {
        paths.games().join(format!("{key}.toml"))
    }

    /// A game file that isn't on disk yet.
    pub fn new(paths: &Paths, key: &str, info: GameInfo) -> GameConfig {
        let header = format!(
            "# GSet settings for {}.\n# Edit them in GSet, or here: GSet keeps your comments when it saves.\n\n[game]\n",
            if info.name.is_empty() { key } else { &info.name }
        );
        let mut g = GameConfig {
            key: key.into(),
            path: Self::path_for(paths, key),
            doc: header.parse().expect("a comment is valid TOML"),
            info: GameInfo::default(),
            values: BTreeMap::new(),
            presets: BTreeMap::new(),
            dismissed: BTreeMap::new(),
            pinned: None,
        };
        g.set_info(info);
        g
    }

    pub fn load(path: &Path) -> Result<GameConfig, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Self::parse(path, &text)
    }

    pub fn parse(path: &Path, text: &str) -> Result<GameConfig, String> {
        let doc: DocumentMut = text.parse().map_err(|e| format!("{}: {e}", path.display()))?;
        let key = path.file_stem().and_then(|s| s.to_str()).unwrap_or("game").to_string();
        let g = doc.get("game").and_then(Item::as_table_like);
        let s = |k: &str| g.and_then(|t| t.get(k)).and_then(Item::as_str).filter(|s| !s.is_empty()).map(String::from);
        let steam = g.and_then(|t| t.get("steam")).and_then(|i| {
            i.as_integer().map(|n| n as u64).or_else(|| i.as_str().and_then(|s| s.trim().parse().ok()))
        });
        let info = GameInfo {
            name: s("name").unwrap_or_else(|| key.clone()),
            steam,
            profile: s("profile"),
            engine: s("engine"),
            via: s("via"),
            mouse_section: s("mouse_section"),
            script: s("script"),
        };
        let mut presets = BTreeMap::new();
        if let Some(t) = doc.get("presets").and_then(Item::as_table_like) {
            for (id, item) in t.iter() {
                presets.insert(id.to_string(), values_of(Some(item)));
            }
        }
        let pinned = doc.get("pinned").and_then(Item::as_str).filter(|s| !s.is_empty()).map(String::from);
        Ok(GameConfig {
            key,
            path: path.to_path_buf(),
            info,
            values: values_of(doc.get("settings")),
            presets,
            dismissed: values_of(doc.get("dismissed")),
            pinned,
            doc,
        })
    }

    pub fn set_info(&mut self, info: GameInfo) {
        let t = table(&mut self.doc, &["game"]);
        let mut put = |k: &str, v: Option<&str>| match v {
            Some(v) if !v.is_empty() => {
                t.insert(k, toml_edit::value(v));
            }
            _ => {
                t.remove(k);
            }
        };
        put("name", Some(&info.name));
        put("via", info.via.as_deref());
        put("profile", info.profile.as_deref());
        put("engine", info.engine.as_deref());
        put("mouse_section", info.mouse_section.as_deref());
        put("script", info.script.as_deref());
        match info.steam {
            Some(id) => {
                t.insert("steam", toml_edit::value(id as i64));
            }
            None => {
                t.remove("steam");
            }
        }
        self.info = info;
    }

    /// Sets or removes (None) one of this game's values.
    pub fn set(&mut self, key: &str, v: Option<Value>) {
        set_in(&mut self.doc, &["settings"], key, v.clone());
        match v {
            Some(v) => self.values.insert(key.into(), v),
            None => self.values.remove(key),
        };
    }

    /// Sets or removes this game's change to a preset.
    pub fn set_preset(&mut self, preset: &str, key: &str, v: Option<Value>) {
        set_in(&mut self.doc, &["presets", preset], key, v.clone());
        let m = self.presets.entry(preset.into()).or_default();
        match v {
            Some(v) => m.insert(key.into(), v),
            None => m.remove(key),
        };
    }

    pub fn dismiss(&mut self, key: &str, v: Option<Value>) {
        set_in(&mut self.doc, &["dismissed"], key, v.clone());
        match v {
            Some(v) => self.dismissed.insert(key.into(), v),
            None => self.dismissed.remove(key),
        };
    }

    pub fn pin(&mut self, preset: Option<&str>) {
        match preset {
            Some(p) => {
                self.doc.insert("pinned", toml_edit::value(p));
            }
            None => {
                self.doc.remove("pinned");
            }
        }
        self.pinned = preset.map(String::from);
    }

    /// Adds a comment line above a setting (used by the import to say where a value came from).
    pub fn note(&mut self, key: &str, comment: &str) {
        if let Some(mut k) = self.doc.get_mut("settings").and_then(Item::as_table_mut).and_then(|t| t.key_mut(key)) {
            k.leaf_decor_mut().set_prefix(format!("# {comment}\n"));
        }
    }

    pub fn text(&self) -> String {
        self.doc.to_string()
    }

    pub fn save(&self) -> Result<(), String> {
        write_with_backup(&self.path, &self.doc.to_string())
    }
}

/// Every game file, sorted by file name, with the ones that couldn't be read.
pub fn all_games(paths: &Paths) -> (Vec<GameConfig>, Vec<String>) {
    let mut games = Vec::new();
    let mut problems = Vec::new();
    let Ok(entries) = std::fs::read_dir(paths.games()) else { return (games, problems) };
    let mut files: Vec<_> = entries.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "toml")).collect();
    files.sort();
    for f in files {
        match GameConfig::load(&f) {
            Ok(g) => games.push(g),
            Err(e) => problems.push(e),
        }
    }
    (games, problems)
}

/// The game file with this Steam id. Two files with the same id: the first by name, and a warning.
pub fn game_for_steam(paths: &Paths, id: u64) -> (Option<GameConfig>, Option<String>) {
    let (games, _) = all_games(paths);
    let mut found = games.into_iter().filter(|g| g.info.steam == Some(id));
    let first = found.next();
    let warning = found.next().map(|other| {
        format!("{} also has Steam id {id}; using {}", other.path.display(), first.as_ref().map(|g| g.key.as_str()).unwrap_or(""))
    });
    (first, warning)
}

fn values_of(item: Option<&Item>) -> BTreeMap<String, Value> {
    let mut out = BTreeMap::new();
    if let Some(t) = item.and_then(Item::as_table_like) {
        for (k, v) in t.iter() {
            if let Some(v) = Value::from_toml(v) {
                out.insert(k.to_string(), v);
            }
        }
    }
    out
}

/// The table at this path, made if missing.
fn table<'a>(doc: &'a mut DocumentMut, path: &[&str]) -> &'a mut Table {
    let mut t = doc.as_table_mut();
    for (i, part) in path.iter().enumerate() {
        let entry = t.entry(part).or_insert_with(|| {
            let mut n = Table::new();
            n.set_implicit(i + 1 < path.len());
            Item::Table(n)
        });
        if !entry.is_table() {
            // an inline table or a value where a table should be: make it a table
            let mut n = Table::new();
            if let Some(it) = entry.as_inline_table() {
                for (k, v) in it.iter() {
                    n.insert(k, Item::Value(v.clone()));
                }
            }
            *entry = Item::Table(n);
        }
        t = entry.as_table_mut().expect("just made a table");
    }
    t
}

fn set_in(doc: &mut DocumentMut, path: &[&str], key: &str, v: Option<Value>) {
    match v {
        Some(v) => {
            let t = table(doc, path);
            match t.get_mut(key).and_then(Item::as_value_mut) {
                // keep the comment around an existing value
                Some(old) => {
                    let decor = old.decor().clone();
                    *old = v.to_toml();
                    *old.decor_mut() = decor;
                }
                None => {
                    t.insert(key, toml_edit::value(v.to_toml()));
                }
            }
        }
        None => {
            let mut cur = doc.as_table_mut();
            for part in path {
                match cur.get_mut(part).and_then(Item::as_table_mut) {
                    Some(t) => cur = t,
                    None => return,
                }
            }
            cur.remove(key);
        }
    }
}

/// Writes the file through a temporary one, after copying the old one to .backup/.
pub fn write_with_backup(path: &Path, text: &str) -> Result<(), String> {
    let dir = path.parent().ok_or("no folder")?;
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    if let Ok(old) = std::fs::read_to_string(path) {
        if old == text {
            return Ok(());
        }
        let backups = dir.join(".backup");
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("file");
        if std::fs::create_dir_all(&backups).is_ok() {
            let _ = std::fs::write(backups.join(format!("{stem}-{}.toml", crate::clock::now_stamp())), old);
            prune_backups(&backups, stem);
        }
    }
    // check it reads back before replacing anything
    text.parse::<DocumentMut>().map_err(|e| format!("not saved, the new file wouldn't read back: {e}"))?;
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, text).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("{}: {e}", path.display()))
}

fn prune_backups(dir: &Path, stem: &str) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let prefix = format!("{stem}-");
    let mut mine: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with(&prefix) && n.ends_with(".toml")))
        .collect();
    mine.sort();
    while mine.len() > KEEP_BACKUPS {
        let _ = std::fs::remove_file(mine.remove(0));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(dir: &Path) -> Paths {
        Paths { config: dir.join("config"), data: dir.join("data"), state: dir.join("state"), home: dir.to_path_buf() }
    }

    #[test]
    fn game_files_keep_comments_and_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let p = paths(dir.path());
        let text = "# my notes\n[game]\nname = \"NTE\"\nsteam = 4508340\n\n[settings]\n# keep me\nrender_scale = 75\ngamescope = true\n";
        std::fs::create_dir_all(p.games()).unwrap();
        let file = GameConfig::path_for(&p, "nte");
        std::fs::write(&file, text).unwrap();
        let mut g = GameConfig::load(&file).unwrap();
        assert_eq!(g.info.steam, Some(4508340));
        assert_eq!(g.values["render_scale"], Value::Int(75));
        g.set("render_scale", Some(Value::Int(67)));
        g.set("gamescope", None);
        g.set_preset("work", "render_scale", Some(Value::Int(100)));
        g.save().unwrap();
        let saved = std::fs::read_to_string(&file).unwrap();
        assert!(saved.contains("# my notes") && saved.contains("# keep me"), "{saved}");
        assert!(saved.contains("render_scale = 67") && !saved.contains("gamescope"));
        let again = GameConfig::load(&file).unwrap();
        assert_eq!(again.presets["work"]["render_scale"], Value::Int(100));
        assert_eq!(std::fs::read_dir(p.games().join(".backup")).unwrap().count(), 1);
        let (found, warning) = game_for_steam(&p, 4508340);
        assert_eq!(found.unwrap().key, "nte");
        assert!(warning.is_none());
    }

    #[test]
    fn app_file_presets() {
        let dir = tempfile::tempdir().unwrap();
        let p = paths(dir.path());
        let mut app = AppConfig::load(&p).unwrap();
        assert!(!app.auto_profile_updates, "profile updates are manual by default");
        app.put_preset(Preset {
            id: "work".into(),
            name: "Work".into(),
            screen: Some("1920x1080".into()),
            values: [("refresh_rate".to_string(), Value::Int(60))].into(),
        });
        app.set_default("mangohud", Some(Value::Bool(true)));
        app.save().unwrap();
        let again = AppConfig::load(&p).unwrap();
        assert_eq!(again.preset("work").unwrap().ratio().as_deref(), Some("16:9"));
        assert_eq!(again.defaults["mangohud"], Value::Bool(true));
        assert_eq!(ratio_name(3440, 1440), "21:9");
    }
}
