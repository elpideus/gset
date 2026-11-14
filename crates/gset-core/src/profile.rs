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
            return Err(format!("written for a newer GSet (profile format {}, this GSet reads {SCHEMA})", self.schema));
        }
        if self.id.is_empty() || !self.id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
            return Err(format!("id “{}”: use lower-case letters, digits and -", self.id));
        }
        let builtin = schema::builtin();
        for k in &self.hide {
            if !builtin.iter().any(|d| d.key == *k) {
                return Err(format!("hide: “{k}” isn't one of GSet's settings"));
            }
        }
        for (key, s) in &self.settings {
            if key.is_empty() || !key.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_') {
                return Err(format!("settings.{key}: use lower-case letters, digits and _"));
            }
            if builtin.iter().any(|d| d.key == *key) {
                return Err(format!("settings.{key}: GSet already has a setting by that name"));
            }
            if s.label.trim().is_empty() {
                return Err(format!("settings.{key}: needs a label"));
            }
            for name in s.env.keys() {
                env_allowed(name).map_err(|e| format!("settings.{key}.env: {e}"))?;
            }
            if s.args.iter().any(|a| a == "--" || a.trim().is_empty()) {
                return Err(format!("settings.{key}.args: “--” and empty arguments aren't allowed"));
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
            let def = schema.get(key).ok_or_else(|| format!("suggest.{key}: no such setting"))?;
            if !def.suggestable {
                return Err(format!("suggest.{key}: profiles can't suggest this setting"));
            }
            fits(def, &s.value).map_err(|e| format!("suggest.{key}: {e}"))?;
        }
        for (key, extra) in &self.options {
            let def = builtin.iter().find(|d| d.key == *key).ok_or_else(|| format!("options.{key}: no such setting"))?;
            if def.kind != Kind::Choice {
                return Err(format!("options.{key}: only lists take extra choices"));
            }
            if def.mono || !def.suggestable {
                return Err(format!("options.{key}: profiles can't add choices to this setting"));
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
            if def.choices.iter().any(|c| c.value.same(v)) { Ok(()) } else { Err(format!("“{v}” isn't one of its choices")) }
        }
        Kind::Number | Kind::Choice => {
            let x = v.as_f64().ok_or("expected a number")?;
            if def.kind == Kind::Number && (def.min.is_some_and(|m| x < m) || def.max.is_some_and(|m| x > m)) {
                return Err(format!("{x} is out of range"));
            }
            Ok(())
        }
        Kind::Resolution => {
            let s = v.as_str();
            if s.is_empty() || s == "native" || crate::plan::parse_size(s).is_some() { Ok(()) } else { Err("expected WIDTHxHEIGHT".into()) }
        }
        Kind::Text => Ok(()),
    }
}

/// Variables a profile may set: graphics and audio switches, never paths or anything that loads code.
pub fn env_allowed(name: &str) -> Result<(), String> {
    const PREFIXES: &[&str] =
        &["DXVK_", "VKD3D_", "PROTON_", "WINE", "MANGOHUD", "__GL_", "__NV_", "RADV_", "MESA_", "SDL_", "PULSE_", "NV_", "ENABLE_", "DISABLE_"];
    const NEVER: &[&str] = &["PATH", "FILE", "PRELOAD", "LIBRARY", "LOADER", "SERVER", "PREFIX", "DIR", "LAYER", "AUDIT", "EXEC"];
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_') {
        return Err(format!("“{name}”: variable names are upper-case letters, digits and _"));
    }
    if !PREFIXES.iter().any(|p| name.starts_with(p)) {
        return Err(format!("“{name}” isn't one of the variables profiles may set"));
    }
    if NEVER.iter().any(|w| name.contains(w)) {
        return Err(format!("“{name}”: profiles may not set paths or anything that loads code"));
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
            let Ok(entries) = std::fs::read_dir(dir) else { continue };
            let mut files: Vec<_> = entries.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "toml")).collect();
            files.sort();
            for f in files {
                match std::fs::read_to_string(&f).map_err(|e| e.to_string()).and_then(|t| Profile::parse(&t)) {
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
    fn a_good_profile_reads() {
        let p = Profile::parse(NTE).unwrap();
        assert_eq!(p.steam, vec![4508340]);
        let s = crate::schema::Schema::for_game(Some(&p));
        assert!(s.get("low_latency").unwrap().hidden);
        assert!(s.get("dlss_preset").unwrap().choices.iter().any(|c| c.label.starts_with('L')));
        assert!(s.get("descriptor_heap").unwrap().from_profile);
    }

    #[test]
    fn profiles_cannot_run_things() {
        let bad = |extra: &str| Profile::parse(&format!("schema = 1\nid = \"x\"\nversion = 1\nname = \"X\"\n{extra}")).unwrap_err();
        assert!(bad("[settings.a]\nlabel = \"A\"\nenv = { LD_PRELOAD = \"/tmp/x.so\" }").contains("LD_PRELOAD"));
        assert!(bad("[settings.a]\nlabel = \"A\"\nenv = { DXVK_CONFIG_FILE = \"/tmp/x\" }").contains("may not"));
        assert!(bad("[settings.a]\nlabel = \"A\"\nenv = { HOME = \"/tmp\" }").contains("isn't one of"));
        assert!(bad("[settings.a]\nlabel = \"A\"\nargs = [\"--\", \"sh\"]").contains("--"));
        assert!(bad("[settings.a]\nlabel = \"A\"\nfix = \"rm-rf\"").contains("no fix"));
        assert!(bad("[suggest]\nrun_before = { value = \"curl x | sh\" }").contains("can't suggest"));
        assert!(bad("[suggest]\nenv = { value = \"LD_PRELOAD=x\" }").contains("can't suggest"));
        assert!(bad("[suggest]\nupscaler = { value = \"magic\" }").contains("isn't one of"));
        assert!(bad("[settings.gamescope]\nlabel = \"G\"").contains("already"));
        assert!(bad("unknown = 1").contains("unknown"));
        assert!(Profile::parse("schema = 9\nid = \"x\"\nversion = 1\nname = \"X\"").unwrap_err().contains("newer GSet"));
    }
}
