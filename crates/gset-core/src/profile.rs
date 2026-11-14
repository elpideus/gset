//! Game profiles: what the community knows about a game. A profile describes the game (names, Steam
//! ids, engine, notes), says which of GSet's settings apply, adds settings and choices of its own, and
//! suggests values with a reason. It never holds anyone's values: updating a profile cannot change
//! how a game is set up, it can only change what the window shows and suggests.
//!
//! Profiles are data. They can't run commands: a game-specific setting may add environment variables
//! from an allowed set, game arguments, or switch on one of GSet's built-in fixes, nothing else.

use crate::fixes;
use crate::schema::{self, Kind};
use crate::value::Value;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The profile format this GSet reads. A profile with a higher number needs a newer GSet.
pub const SCHEMA: u32 = 1;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub schema: u32,
    pub id: String,
    pub version: u32,
    pub name: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub steam: Vec<u64>,
    #[serde(default)]
    pub engine: Option<String>,
    #[serde(default)]
    pub notes: Vec<String>,
    /// GSet settings that don't apply to this game
    #[serde(default)]
    pub hide: Vec<String>,
    /// why settings were taken out (shown next to a value you still have for one)
    #[serde(default)]
    pub removed: BTreeMap<String, String>,
    #[serde(default)]
    pub suggest: BTreeMap<String, Suggestion>,
    /// extra choices for GSet's settings
    #[serde(default)]
    pub options: BTreeMap<String, Vec<ChoiceDef>>,
    /// settings of this game's own
    #[serde(default)]
    pub settings: BTreeMap<String, ProfileSetting>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Suggestion {
    pub value: Value,
    #[serde(default)]
    pub why: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChoiceDef {
    pub value: Value,
    pub label: String,
}

/// A switch of the game's own. While it's on, it adds these variables and arguments, or a fix.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileSetting {
    pub label: String,
    #[serde(default)]
    pub help: String,
    #[serde(default)]
    pub when: Option<String>,
    #[serde(default)]
    pub off: Option<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub fix: Option<String>,
}

impl Profile {
    pub fn parse(text: &str) -> Result<Profile, String> {
        let p: Profile = toml::from_str(text).map_err(|e| e.to_string())?;
        p.check()?;
        Ok(p)
    }

    pub fn suggestion(&self, key: &str) -> Option<&Suggestion> {
        self.suggest.get(key)
    }

    /// Everything a profile must get right before GSet uses it.
    pub fn check(&self) -> Result<(), String> {
        if self.schema > SCHEMA {
            return Err(format!(
                "written for a newer GSet (profile format {}, this GSet reads {SCHEMA})",
                self.schema
            ));
        }
        if self.id.is_empty()
            || !self
                .id
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        {
            return Err(format!(
                "id “{}”: use lower-case letters, digits and -",
                self.id
            ));
        }
        let builtin = schema::builtin();
        for k in &self.hide {
            if !builtin.iter().any(|d| d.key == *k) {
                return Err(format!("hide: “{k}” isn't one of GSet's settings"));
            }
        }
        for (key, s) in &self.settings {
            if key.is_empty()
                || !key
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
            {
                return Err(format!(
                    "settings.{key}: use lower-case letters, digits and _"
                ));
            }
            if builtin.iter().any(|d| d.key == *key) {
                return Err(format!(
                    "settings.{key}: GSet already has a setting by that name"
                ));
            }
            if s.label.trim().is_empty() {
                return Err(format!("settings.{key}: needs a label"));
            }
            for name in s.env.keys() {
                env_allowed(name).map_err(|e| format!("settings.{key}.env: {e}"))?;
            }
            if s.args.iter().any(|a| a == "--" || a.trim().is_empty()) {
                return Err(format!(
                    "settings.{key}.args: “--” and empty arguments aren't allowed"
                ));
            }
            if let Some(f) = &s.fix
                && !fixes::known(f)
            {
                return Err(format!("settings.{key}.fix: GSet has no fix called “{f}”"));
            }
            if let Some(w) = &s.when {
                crate::when::When::parse(w).map_err(|e| format!("settings.{key}.when: {e}"))?;
            }
        }
        let schema = crate::schema::Schema::for_game(Some(self));
        for (key, s) in &self.suggest {
            let def = schema
                .get(key)
                .ok_or_else(|| format!("suggest.{key}: no such setting"))?;
            if !def.suggestable {
                return Err(format!(
                    "suggest.{key}: profiles can't suggest this setting"
                ));
            }
            fits(def, &s.value).map_err(|e| format!("suggest.{key}: {e}"))?;
        }
        for (key, extra) in &self.options {
            let def = builtin
                .iter()
                .find(|d| d.key == *key)
                .ok_or_else(|| format!("options.{key}: no such setting"))?;
            if def.kind != Kind::Choice {
                return Err(format!("options.{key}: only lists take extra choices"));
            }
            if def.mono || !def.suggestable {
                return Err(format!(
                    "options.{key}: profiles can't add choices to this setting"
                ));
            }
            if extra.iter().any(|c| c.label.trim().is_empty()) {
                return Err(format!("options.{key}: every choice needs a label"));
            }
        }
        Ok(())
    }
}

/// Whether a value suits a setting.
fn fits(def: &crate::schema::SettingDef, v: &Value) -> Result<(), String> {
    match def.kind {
        Kind::Toggle => match v {
            Value::Bool(_) => Ok(()),
            _ => Err("expected true or false".into()),
        },
        Kind::Choice if !def.editable => {
            if def.choices.iter().any(|c| c.value.same(v)) {
                Ok(())
            } else {
                Err(format!("“{v}” isn't one of its choices"))
            }
        }
        Kind::Number | Kind::Choice => {
            let x = v.as_f64().ok_or("expected a number")?;
            if def.kind == Kind::Number
                && (def.min.is_some_and(|m| x < m) || def.max.is_some_and(|m| x > m))
            {
                return Err(format!("{x} is out of range"));
            }
            Ok(())
        }
        Kind::Resolution => {
            let s = v.as_str();
            if s.is_empty() || s == "native" || crate::plan::parse_size(s).is_some() {
                Ok(())
            } else {
                Err("expected WIDTHxHEIGHT".into())
            }
        }
        Kind::Text => Ok(()),
    }
}

/// Variables a profile may set: graphics and audio switches, never paths or anything that loads code.
pub fn env_allowed(name: &str) -> Result<(), String> {
    const PREFIXES: &[&str] = &[
        "DXVK_", "VKD3D_", "PROTON_", "WINE", "MANGOHUD", "__GL_", "__NV_", "RADV_", "MESA_",
        "SDL_", "PULSE_", "NV_", "ENABLE_", "DISABLE_",
    ];
    const NEVER: &[&str] = &[
        "PATH", "FILE", "PRELOAD", "LIBRARY", "LOADER", "SERVER", "PREFIX", "DIR", "LAYER",
        "AUDIT", "EXEC",
    ];
    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
    {
        return Err(format!(
            "“{name}”: variable names are upper-case letters, digits and _"
        ));
    }
    if !PREFIXES.iter().any(|p| name.starts_with(p)) {
        return Err(format!(
            "“{name}” isn't one of the variables profiles may set"
        ));
    }
    if NEVER.iter().any(|w| name.contains(w)) {
        return Err(format!(
            "“{name}”: profiles may not set paths or anything that loads code"
        ));
    }
    Ok(())
}

/// All the profiles GSet can see, by id and by Steam id.
#[derive(Default, Debug)]
pub struct Profiles {
    pub by_id: BTreeMap<String, Profile>,
    /// files that couldn't be used, and why
    pub problems: Vec<(PathBuf, String)>,
}

impl Profiles {
    /// Reads every `*.toml` in these folders. A later folder's profile replaces an earlier one's.
    pub fn load(dirs: &[PathBuf]) -> Profiles {
        let mut out = Profiles::default();
        for dir in dirs {
            let Ok(entries) = std::fs::read_dir(dir) else {
                continue;
            };
            let mut files: Vec<_> = entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|x| x == "toml"))
                .collect();
            files.sort();
            for f in files {
                match std::fs::read_to_string(&f)
                    .map_err(|e| e.to_string())
                    .and_then(|t| Profile::parse(&t))
                {
                    Ok(p) => {
                        out.by_id.insert(p.id.clone(), p);
                    }
                    Err(e) => out.problems.push((f, e)),
                }
            }
        }
        out
    }

    pub fn get(&self, id: &str) -> Option<&Profile> {
        self.by_id.get(id)
    }

    pub fn for_steam(&self, steam: u64) -> Option<&Profile> {
        self.by_id.values().find(|p| p.steam.contains(&steam))
    }
}

/// Where profiles come from, lowest priority first: the downloaded set, a folder named by
/// GSET_PROFILES_DIR (for writing profiles), and your own in ~/.config/gset/profiles.
pub fn dirs(paths: &crate::paths::Paths) -> Vec<PathBuf> {
    let mut v = vec![paths.data.join("profiles")];
    if let Some(d) = std::env::var_os("GSET_PROFILES_DIR") {
        v.push(PathBuf::from(d));
    }
    v.push(paths.config.join("profiles"));
    v
}

pub fn read(path: &Path) -> Result<Profile, String> {
    Profile::parse(&std::fs::read_to_string(path).map_err(|e| e.to_string())?)
}

// ------------------------------------------------------------------------------------------ updates

/// One way a newer profile differs from the one in use. None of them touches your values.
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Change {
    NewSetting {
        key: String,
        label: String,
        help: String,
    },
    RemovedSetting {
        key: String,
        label: String,
        why: String,
    },
    Suggestion {
        key: String,
        label: String,
        value: Value,
        why: String,
        before: Option<Value>,
    },
    DroppedSuggestion {
        key: String,
        label: String,
    },
    NewChoice {
        key: String,
        label: String,
        choice: String,
    },
    Notes {
        notes: Vec<String>,
    },
    Name {
        name: String,
    },
}

/// What a newer version of a profile changes.
pub fn diff(old: &Profile, new: &Profile) -> Vec<Change> {
    let before = crate::schema::Schema::for_game(Some(old));
    let after = crate::schema::Schema::for_game(Some(new));
    let label = |key: &str| {
        after
            .get(key)
            .or_else(|| before.get(key))
            .map(|d| d.label.clone())
            .unwrap_or_else(|| key.to_string())
    };
    let why = |key: &str| new.removed.get(key).cloned().unwrap_or_default();
    let mut out = Vec::new();
    if old.name != new.name {
        out.push(Change::Name {
            name: new.name.clone(),
        });
    }
    for (key, s) in &new.settings {
        if !old.settings.contains_key(key) {
            out.push(Change::NewSetting {
                key: key.clone(),
                label: s.label.clone(),
                help: s.help.clone(),
            });
        }
    }
    for key in old.hide.iter().filter(|k| !new.hide.contains(k)) {
        out.push(Change::NewSetting {
            key: key.clone(),
            label: label(key),
            help: String::new(),
        });
    }
    for key in old
        .settings
        .keys()
        .filter(|k| !new.settings.contains_key(*k))
        .chain(new.hide.iter().filter(|k| !old.hide.contains(k)))
    {
        out.push(Change::RemovedSetting {
            key: key.clone(),
            label: label(key),
            why: why(key),
        });
    }
    for (key, s) in &new.suggest {
        let prev = old.suggest.get(key);
        if prev.is_none_or(|p| !p.value.same(&s.value)) {
            out.push(Change::Suggestion {
                key: key.clone(),
                label: label(key),
                value: s.value.clone(),
                why: s.why.clone(),
                before: prev.map(|p| p.value.clone()),
            });
        }
    }
    for key in old
        .suggest
        .keys()
        .filter(|k| !new.suggest.contains_key(*k) && after.get(k).is_some_and(|d| !d.hidden))
    {
        out.push(Change::DroppedSuggestion {
            key: key.clone(),
            label: label(key),
        });
    }
    for (key, choices) in &new.options {
        let had = old.options.get(key);
        for c in choices {
            if !had.is_some_and(|h| h.iter().any(|x| x.value.same(&c.value))) {
                out.push(Change::NewChoice {
                    key: key.clone(),
                    label: label(key),
                    choice: c.label.clone(),
                });
            }
        }
    }
    if old.notes != new.notes {
        out.push(Change::Notes {
            notes: new.notes.clone(),
        });
    }
    out
}

/// Downloaded profiles wait in profiles-pending/ until they're applied (by you, or by
/// "Update profiles automatically"). Applying keeps the one replaced, for Undo.
pub fn pending(
    paths: &crate::paths::Paths,
    id: &str,
    current: Option<&Profile>,
) -> Option<Profile> {
    let p = read(
        &paths
            .data
            .join("profiles-pending")
            .join(format!("{id}.toml")),
    )
    .ok()?;
    (current.is_none_or(|c| p.version > c.version)).then_some(p)
}

pub fn apply_pending(paths: &crate::paths::Paths, id: &str) -> Result<(), String> {
    let from = paths
        .data
        .join("profiles-pending")
        .join(format!("{id}.toml"));
    let dir = paths.data.join("profiles");
    let to = dir.join(format!("{id}.toml"));
    let keep = paths.data.join("profiles-previous");
    std::fs::create_dir_all(&dir)
        .and_then(|()| std::fs::create_dir_all(&keep))
        .map_err(|e| e.to_string())?;
    if to.exists() {
        std::fs::copy(&to, keep.join(format!("{id}.toml"))).map_err(|e| e.to_string())?;
    }
    std::fs::rename(&from, &to).map_err(|e| format!("{}: {e}", from.display()))
}

/// Undoes apply_pending: the replaced profile comes back, the new one waits again.
pub fn undo_apply(paths: &crate::paths::Paths, id: &str) -> Result<(), String> {
    let cur = paths.data.join("profiles").join(format!("{id}.toml"));
    let prev = paths
        .data
        .join("profiles-previous")
        .join(format!("{id}.toml"));
    let pend = paths.data.join("profiles-pending");
    std::fs::create_dir_all(&pend).map_err(|e| e.to_string())?;
    std::fs::rename(&cur, pend.join(format!("{id}.toml"))).map_err(|e| e.to_string())?;
    if prev.exists() {
        std::fs::rename(&prev, &cur).map_err(|e| e.to_string())?;
    }
    Ok(())
}

// ------------------------------------------------------------------------------------------ the shared set

/// Every profile of the community repository in one file, which GSet downloads (signed).
/// The profiles stay as their TOML text, so GSet checks them exactly as it checks local ones.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Bundle {
    /// the bundle format (not the profiles')
    pub schema: u32,
    /// id → the profile file's text
    pub profiles: BTreeMap<String, String>,
}

pub const BUNDLE_SCHEMA: u32 = 1;

/// Checks every *.toml in a folder the way the repository's CI does: each profile reads and passes
/// GSet's checks, its file is named after its id, and no two share an id or a Steam id.
pub fn check_dir(dir: &Path) -> (BTreeMap<String, (Profile, String)>, Vec<String>) {
    let mut ok: BTreeMap<String, (Profile, String)> = BTreeMap::new();
    let mut problems = Vec::new();
    let mut files: Vec<PathBuf> = match std::fs::read_dir(dir) {
        Ok(e) => e
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "toml"))
            .collect(),
        Err(e) => return (ok, vec![format!("{}: {e}", dir.display())]),
    };
    files.sort();
    let mut steam: BTreeMap<u64, String> = BTreeMap::new();
    for f in files {
        let name = f
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("?")
            .to_string();
        let text = match std::fs::read_to_string(&f) {
            Ok(t) => t,
            Err(e) => {
                problems.push(format!("{name}: {e}"));
                continue;
            }
        };
        let p = match Profile::parse(&text) {
            Ok(p) => p,
            Err(e) => {
                problems.push(format!("{name}: {e}"));
                continue;
            }
        };
        if name != format!("{}.toml", p.id) {
            problems.push(format!(
                "{name}: its id is “{}”, so the file must be {}.toml",
                p.id, p.id
            ));
        }
        if ok.contains_key(&p.id) {
            problems.push(format!(
                "{name}: another file already has the id “{}”",
                p.id
            ));
        }
        for id in &p.steam {
            if let Some(other) = steam.insert(*id, p.id.clone()) {
                problems.push(format!("{name}: Steam id {id} is also in {other}.toml"));
            }
        }
        ok.insert(p.id.clone(), (p, text));
    }
    (ok, problems)
}

/// The bundle for a folder of profiles, or what's wrong with them.
pub fn make_bundle(dir: &Path) -> Result<String, Vec<String>> {
    let (ok, problems) = check_dir(dir);
    if !problems.is_empty() {
        return Err(problems);
    }
    let b = Bundle {
        schema: BUNDLE_SCHEMA,
        profiles: ok.into_iter().map(|(id, (_, text))| (id, text)).collect(),
    };
    serde_json::to_string_pretty(&b).map_err(|e| vec![e.to_string()])
}

pub fn read_bundle(text: &str) -> Result<Bundle, String> {
    let b: Bundle =
        serde_json::from_str(text).map_err(|e| format!("the profile bundle doesn't read: {e}"))?;
    if b.schema > BUNDLE_SCHEMA {
        return Err(format!(
            "the profile bundle needs a newer GSet (format {})",
            b.schema
        ));
    }
    Ok(b)
}

/// What a downloaded bundle changed: profiles new to this computer go straight in (they only
/// describe games), newer versions of ones you have wait in profiles-pending/ for you to review,
/// unless you update automatically. Games of yours that a new profile describes get linked to it.
#[derive(Clone, Debug, Default, Serialize)]
pub struct Installed {
    pub new: Vec<String>,
    pub updates: Vec<String>,
    pub linked: Vec<String>,
    pub problems: Vec<String>,
}

pub fn install_bundle(paths: &crate::paths::Paths, b: &Bundle, apply_updates: bool) -> Installed {
    let mut out = Installed::default();
    let dir = paths.data.join("profiles");
    let pending = paths.data.join("profiles-pending");
    let have = Profiles::load(std::slice::from_ref(&dir));
    for (id, text) in &b.profiles {
        let p = match Profile::parse(text) {
            Ok(p) if p.id == *id => p,
            Ok(p) => {
                out.problems
                    .push(format!("{id}: the profile inside says “{}”", p.id));
                continue;
            }
            Err(e) => {
                out.problems.push(format!("{id}: {e}"));
                continue;
            }
        };
        let write = |folder: &Path| -> Result<(), String> {
            std::fs::create_dir_all(folder).map_err(|e| e.to_string())?;
            let file = folder.join(format!("{id}.toml"));
            let tmp = folder.join(format!(".{id}.toml.new"));
            std::fs::write(&tmp, text)
                .and_then(|()| std::fs::rename(&tmp, &file))
                .map_err(|e| e.to_string())
        };
        match have.get(id) {
            None => match write(&dir) {
                Ok(()) => out.new.push(p.name.clone()),
                Err(e) => out.problems.push(format!("{id}: {e}")),
            },
            Some(cur) if p.version > cur.version => {
                let already =
                    read(&pending.join(format!("{id}.toml"))).is_ok_and(|w| w.version >= p.version);
                if !already && let Err(e) = write(&pending) {
                    out.problems.push(format!("{id}: {e}"));
                    continue;
                }
                if apply_updates && let Err(e) = apply_pending(paths, id) {
                    out.problems.push(format!("{id}: {e}"));
                }
                if !already || apply_updates {
                    out.updates.push(p.name.clone());
                }
            }
            Some(_) => {}
        }
    }
    // your games without a profile that one of these describes
    let all = Profiles::load(&dirs(paths));
    for mut g in crate::config::all_games(paths).0 {
        if g.info.profile.is_some() || g.info.script.is_some() {
            continue;
        }
        let Some(steam) = g.info.steam else { continue };
        if let Some(p) = all.for_steam(steam) {
            let mut info = g.info.clone();
            info.profile = Some(p.id.clone());
            if info.engine.is_none() {
                info.engine = p.engine.clone();
            }
            g.set_info(info);
            match g.save() {
                Ok(()) => out.linked.push(g.info.name.clone()),
                Err(e) => out.problems.push(format!("{}: {e}", g.key)),
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const NTE: &str = r#"
schema = 1
id = "nte"
version = 15
name = "NTE: Neverness to Everness"
steam = [4508340]
engine = "unreal"
notes = ["Needs DW-Proton for the anti-cheat."]
hide = ["low_latency"]

[removed]
low_latency = "It no longer helps since the 1.4 patch."

[suggest]
graphics_api = { value = "d3d11", why = "The most stable API for NTE." }
render_scale = { value = 67, why = "Keeps 60 FPS in the open world." }

[options]
dlss_preset = [{ value = "L", label = "L · newest" }]

[settings.descriptor_heap]
label = "Descriptor heap fix"
when = "graphics_api=d3d12"
env = { VKD3D_CONFIG = "descriptor_heap" }

[settings.launcher_windows]
label = "Fix the launcher crash"
fix = "nte-launcher-windows"
"#;

    #[test]
    fn bundles_install_new_profiles_and_hold_updates() {
        let d = tempfile::tempdir().unwrap();
        let paths = crate::paths::Paths {
            config: d.path().join("c"),
            data: d.path().join("d"),
            state: d.path().join("s"),
            home: d.path().into(),
        };
        let repo = d.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let p = |v: u32| {
            format!("schema = 1\nid = \"g\"\nversion = {v}\nname = \"Good Game\"\nsteam = [42]\n")
        };
        std::fs::write(repo.join("g.toml"), p(1)).unwrap();
        let b = read_bundle(&make_bundle(&repo).unwrap()).unwrap();
        // a game of yours that no profile described yet
        let mut game = crate::config::GameConfig::new(
            &paths,
            "mine",
            crate::config::GameInfo {
                name: "Mine".into(),
                steam: Some(42),
                ..Default::default()
            },
        );
        game.set("gamescope", Some(true.into()));
        game.save().unwrap();

        let got = install_bundle(&paths, &b, false);
        assert_eq!(
            (got.new.len(), got.updates.len(), got.linked.len()),
            (1, 0, 1),
            "{got:?}"
        );
        let mine = crate::config::GameConfig::load(&game.path).unwrap();
        assert_eq!(mine.info.profile.as_deref(), Some("g"));
        assert_eq!(
            mine.values["gamescope"],
            Value::Bool(true),
            "linking never touches values"
        );
        assert!(
            install_bundle(&paths, &b, false).new.is_empty(),
            "nothing new the second time"
        );

        std::fs::write(repo.join("g.toml"), p(2)).unwrap();
        let b2 = read_bundle(&make_bundle(&repo).unwrap()).unwrap();
        let got = install_bundle(&paths, &b2, false);
        assert_eq!(got.updates, vec!["Good Game".to_string()]);
        let have = Profiles::load(&[paths.data.join("profiles")]);
        assert_eq!(have.get("g").unwrap().version, 1, "an update waits for you");
        assert_eq!(pending(&paths, "g", have.get("g")).unwrap().version, 2);
        assert!(
            install_bundle(&paths, &b2, false).updates.is_empty(),
            "the same update isn't announced twice"
        );
        install_bundle(&paths, &b2, true);
        assert_eq!(
            Profiles::load(&[paths.data.join("profiles")])
                .get("g")
                .unwrap()
                .version,
            2,
            "automatic updates apply it"
        );

        // the repository's checks
        std::fs::write(
            repo.join("wrong-name.toml"),
            p(1).replace("id = \"g\"", "id = \"h\"")
                .replace("[42]", "[43]"),
        )
        .unwrap();
        std::fs::write(
            repo.join("dup.toml"),
            p(1).replace("id = \"g\"", "id = \"dup\""),
        )
        .unwrap();
        let problems = make_bundle(&repo).unwrap_err();
        assert!(
            problems.iter().any(|e| e.contains("must be h.toml")),
            "{problems:?}"
        );
        assert!(
            problems.iter().any(|e| e.contains("Steam id 42")),
            "{problems:?}"
        );
    }

    #[test]
    fn diff_lists_what_changes() {
        let old = Profile::parse("schema = 1\nid = \"n\"\nversion = 14\nname = \"N\"\n[suggest]\nlow_latency = { value = true }\n").unwrap();
        let new = Profile::parse(NTE).unwrap();
        let d = diff(&old, &new);
        assert!(
            d.iter()
                .any(|c| matches!(c, Change::NewSetting { key, .. } if key == "descriptor_heap"))
        );
        assert!(d.iter().any(|c| matches!(c, Change::RemovedSetting { key, why, .. } if key == "low_latency" && why.contains("1.4"))));
        assert!(d.iter().any(
            |c| matches!(c, Change::Suggestion { key, before: None, .. } if key == "render_scale")
        ));
        assert!(
            d.iter()
                .any(|c| matches!(c, Change::NewChoice { choice, .. } if choice.starts_with('L')))
        );
        assert!(
            !d.iter()
                .any(|c| matches!(c, Change::DroppedSuggestion { .. })),
            "a hidden setting's suggestion isn't reported twice"
        );
    }

    #[test]
    fn a_good_profile_reads() {
        let p = Profile::parse(NTE).unwrap();
        assert_eq!(p.steam, vec![4508340]);
        let s = crate::schema::Schema::for_game(Some(&p));
        assert!(s.get("low_latency").unwrap().hidden);
        assert!(
            s.get("dlss_preset")
                .unwrap()
                .choices
                .iter()
                .any(|c| c.label.starts_with('L'))
        );
        assert!(s.get("descriptor_heap").unwrap().from_profile);
    }

    #[test]
    fn profiles_cannot_run_things() {
        let bad = |extra: &str| {
            Profile::parse(&format!(
                "schema = 1\nid = \"x\"\nversion = 1\nname = \"X\"\n{extra}"
            ))
            .unwrap_err()
        };
        assert!(
            bad("[settings.a]\nlabel = \"A\"\nenv = { LD_PRELOAD = \"/tmp/x.so\" }")
                .contains("LD_PRELOAD")
        );
        assert!(
            bad("[settings.a]\nlabel = \"A\"\nenv = { DXVK_CONFIG_FILE = \"/tmp/x\" }")
                .contains("may not")
        );
        assert!(
            bad("[settings.a]\nlabel = \"A\"\nenv = { HOME = \"/tmp\" }").contains("isn't one of")
        );
        assert!(bad("[settings.a]\nlabel = \"A\"\nargs = [\"--\", \"sh\"]").contains("--"));
        assert!(bad("[settings.a]\nlabel = \"A\"\nfix = \"rm-rf\"").contains("no fix"));
        assert!(
            bad("[suggest]\nrun_before = { value = \"curl x | sh\" }").contains("can't suggest")
        );
        assert!(bad("[suggest]\nenv = { value = \"LD_PRELOAD=x\" }").contains("can't suggest"));
        assert!(bad("[suggest]\nupscaler = { value = \"magic\" }").contains("isn't one of"));
        assert!(bad("[settings.gamescope]\nlabel = \"G\"").contains("already"));
        assert!(bad("unknown = 1").contains("unknown"));
        assert!(
            Profile::parse("schema = 9\nid = \"x\"\nversion = 1\nname = \"X\"")
                .unwrap_err()
                .contains("newer GSet")
        );
    }
}
