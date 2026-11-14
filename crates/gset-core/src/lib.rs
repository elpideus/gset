//! GSet's core: settings, presets, game profiles, and the command that starts a game.
//! The `gset` launcher and the GSet window are both built on it.

pub mod clock;
pub mod config;
pub mod fixes;
pub mod import;
pub mod paths;
pub mod plan;
pub mod profile;
pub mod resolve;
pub mod schema;
pub mod screen;
pub mod steam;
pub mod value;
pub mod when;

pub use config::{AppConfig, GameConfig, GameInfo, NewGames, Preset};
pub use paths::Paths;
pub use profile::{Profile, Profiles};
pub use value::Value;

/// The game a launch is for.
pub struct Found {
    pub game: GameConfig,
    /// how GSet knew which game it is
    pub how: String,
    /// GSet met the game for the first time and made its file
    pub created: bool,
    pub warnings: Vec<String>,
}

/// Finds the game Steam is starting (or the one named with --game), and makes its file the first
/// time GSet meets it: from its profile's suggestions and what its files say about it.
/// Ok(None): there's no way to tell which game it is.
pub fn find_or_create(
    paths: &Paths,
    app: &AppConfig,
    profiles: &Profiles,
    args: &[String],
    env: impl Fn(&str) -> Option<String>,
    named: Option<&str>,
    dry_run: bool,
) -> Result<Option<Found>, String> {
    if let Some(key) = named {
        if key.is_empty() || key.contains('/') || key.starts_with('.') {
            return Err(format!("--game={key}: expected the name of a file in {}, without .toml", paths.games().display()));
        }
        let path = GameConfig::path_for(paths, key);
        let game = GameConfig::load(&path)?;
        return Ok(Some(Found { game, how: format!("--game={key}"), created: false, warnings: Vec::new() }));
    }
    let Some((id, from)) = steam::app_id(args, env) else { return Ok(None) };
    let how = format!("Steam id {id} ({})", from.text());
    let (found, dup) = config::game_for_steam(paths, id);
    if let Some(game) = found {
        return Ok(Some(Found { game, how, created: false, warnings: dup.into_iter().collect() }));
    }

    // a game GSet hasn't seen: its profile, else what its files say
    let profile = profiles.for_steam(id);
    let seen = steam::inspect(args, Some(id), &steam::steam_root(&paths.home));
    let name = profile.map(|p| p.name.clone()).or(seen.name.clone()).unwrap_or_else(|| format!("Steam game {id}"));
    let base = profile.map(|p| p.id.clone()).unwrap_or_else(|| slug(&name));
    let base = if base.is_empty() { format!("steam-{id}") } else { base };
    let mut key = base.clone();
    let mut n = 2;
    while GameConfig::path_for(paths, &key).exists() {
        key = format!("{base}-{n}");
        n += 1;
    }
    let info = GameInfo {
        name,
        steam: Some(id),
        profile: profile.map(|p| p.id.clone()),
        engine: profile.and_then(|p| p.engine.clone()).or(seen.engine.clone()),
        via: Some(if id > u64::from(u32::MAX >> 1) { "Steam shortcut" } else { "Steam" }.into()),
        mouse_section: None,
        script: None,
    };
    let mut game = GameConfig::new(paths, &key, info);
    for (k, v) in resolve::start_values(profile, app.new_games) {
        game.set(&k, Some(v));
    }
    if !dry_run {
        game.save()?;
    }
    Ok(Some(Found { game, how, created: true, warnings: Vec::new() }))
}

/// "NTE: Neverness to Everness" → "nte-neverness-to-everness"
pub fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    out.trim_end_matches('-').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_launch_makes_a_file_from_the_profile() {
        let d = tempfile::tempdir().unwrap();
        let paths = Paths { config: d.path().join("c"), data: d.path().join("d"), state: d.path().join("s"), home: d.path().into() };
        std::fs::create_dir_all(paths.config.join("profiles")).unwrap();
        std::fs::write(
            paths.config.join("profiles/g.toml"),
            "schema = 1\nid = \"g\"\nversion = 1\nname = \"Good Game\"\nsteam = [42]\nengine = \"unity\"\n[suggest]\ngraphics_api = { value = \"vulkan\" }\n",
        )
        .unwrap();
        let profiles = Profiles::load(&profile::dirs(&paths));
        let app = AppConfig::load(&paths).unwrap();
        let args = vec!["AppId=42".to_string(), "/g/Game.exe".to_string()];
        let f = find_or_create(&paths, &app, &profiles, &args, |_| None, None, false).unwrap().unwrap();
        assert!(f.created);
        assert_eq!(f.game.key, "g");
        assert_eq!(f.game.values["graphics_api"], Value::str("vulkan"));
        let again = find_or_create(&paths, &app, &profiles, &args, |_| None, None, false).unwrap().unwrap();
        assert!(!again.created, "the second launch finds the file");
        assert!(find_or_create(&paths, &app, &profiles, &["/x".into()], |_| None, None, false).unwrap().is_none());
        assert_eq!(slug("NTE: Neverness to Everness"), "nte-neverness-to-everness");
    }
}
