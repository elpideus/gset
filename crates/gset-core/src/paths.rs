//! Where GSet keeps things, following the XDG base directories.
//! GSET_CONFIG_DIR, GSET_DATA_DIR and GSET_STATE_DIR move them (tests and trying things out).

use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct Paths {
    /// your settings: gset.toml, games/, profiles/ (~/.config/gset)
    pub config: PathBuf,
    /// downloaded profiles (~/.local/share/gset)
    pub data: PathBuf,
    /// the launch log (~/.local/state/gset)
    pub state: PathBuf,
    pub home: PathBuf,
}

impl Paths {
    pub fn new() -> Paths {
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"));
        let xdg = |var: &str, fallback: &str| {
            std::env::var_os(var).map(PathBuf::from).filter(|p| p.is_absolute()).unwrap_or_else(|| home.join(fallback))
        };
        let pick = |own: &str, var: &str, fallback: &str| {
            std::env::var_os(own).map(PathBuf::from).unwrap_or_else(|| xdg(var, fallback).join("gset"))
        };
        Paths {
            config: pick("GSET_CONFIG_DIR", "XDG_CONFIG_HOME", ".config"),
            data: pick("GSET_DATA_DIR", "XDG_DATA_HOME", ".local/share"),
            state: pick("GSET_STATE_DIR", "XDG_STATE_HOME", ".local/state"),
            home,
        }
    }

    pub fn games(&self) -> PathBuf {
        self.config.join("games")
    }

    pub fn app_file(&self) -> PathBuf {
        self.config.join("gset.toml")
    }

    pub fn log_file(&self) -> PathBuf {
        self.state.join("launch.log")
    }

    /// `~/x`, `$HOME/x` and `${HOME}/x` in a path you wrote.
    pub fn expand(&self, p: &str) -> PathBuf {
        for prefix in ["~/", "$HOME/", "${HOME}/"] {
            if let Some(rest) = p.strip_prefix(prefix) {
                return self.home.join(rest);
            }
        }
        if p == "~" { self.home.clone() } else { PathBuf::from(p) }
    }
}

impl Default for Paths {
    fn default() -> Self {
        Paths::new()
    }
}
