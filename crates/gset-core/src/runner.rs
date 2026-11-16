//! Games that GSet starts itself, instead of wrapping the command Steam (or a launcher) passes: a
//! runner builds the game's command, and decides which of GSet's settings reach the game.
//!
//! - `sober`: Roblox through Sober, the Flatpak org.vinegarhq.Sober. MangoHud and the OBS hook
//!   are Vulkan layers inside its sandbox, switched on with flatpak's --env.
//! - `umu`: a Windows game outside Steam, through umu-run with a Proton and a Wine prefix.
//! - `twintail`: a game installed with TwintailLauncher, which starts the game with its own
//!   per-install settings. GSet checks for updates first, waits for the game, and quits Twintail
//!   after it (gset-cli's twintail.rs).

use crate::config::GameInfo;
use crate::schema;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Runner {
    /// the command Steam or a launcher passes, wrapped (every Steam game)
    Command,
    Sober,
    Umu,
    Twintail,
}

pub const SOBER_APP: &str = "org.vinegarhq.Sober";
pub const TWINTAIL: &str = "/usr/bin/twintaillauncher";

/// GSet's settings that a game started by Twintail still has: the rest is Twintail's, per install.
const TWINTAIL_KEEPS: &[&str] = &["gpu_check", "vram_warn", "mouse", "mouse_dpi"];

/// What Sober can't use: Gamescope (not offered by the old roblox-launch), and what only Proton
/// (DXVK, vkd3d-proton) or the game's engine reads.
const SOBER_EXCLUDES: &[&str] = &[
    "gamescope",
    "gamescope_output",
    "render_scale",
    "upscaler",
    "refresh_rate",
    "adaptive_sync",
    "fullscreen",
    "clipboard",
    "gamescope_args",
    "game_resolution",
    "vsync",
    "frame_cap",
    "graphics_api",
    "dlss_preset",
    "low_latency",
    "nvidia_only",
    "vkd3d_config",
    "gamescope_mouse",
    "strip_wrappers",
];

/// umu-run runs any Windows program: GSet can't tell its engine, and there's no launcher's wrapper.
const UMU_EXCLUDES: &[&str] = &["graphics_api", "game_resolution", "strip_wrappers"];

impl Runner {
    pub fn of(info: &GameInfo) -> Runner {
        match info.runner.as_deref() {
            Some("sober") => Runner::Sober,
            Some("umu") => Runner::Umu,
            Some("twintail") => Runner::Twintail,
            _ => Runner::Command,
        }
    }

    /// The name in a game file's `runner =`.
    pub fn id(self) -> Option<&'static str> {
        match self {
            Runner::Command => None,
            Runner::Sober => Some("sober"),
            Runner::Umu => Some("umu"),
            Runner::Twintail => Some("twintail"),
        }
    }

    pub fn parse(id: &str) -> Option<Runner> {
        [Runner::Sober, Runner::Umu, Runner::Twintail]
            .into_iter()
            .find(|r| r.id() == Some(id))
    }

    /// How the window names it.
    pub fn title(self) -> &'static str {
        match self {
            Runner::Command => "Steam’s command",
            Runner::Sober => "Sober",
            Runner::Umu => "umu-run",
            Runner::Twintail => "TwintailLauncher",
        }
    }

    /// GSet's own settings that don't reach a game started this way.
    pub fn excluded(self) -> Vec<String> {
        let list: &[&str] = match self {
            Runner::Command => &[],
            Runner::Sober => SOBER_EXCLUDES,
            Runner::Umu => UMU_EXCLUDES,
            Runner::Twintail => {
                return schema::builtin()
                    .into_iter()
                    .map(|d| d.key)
                    .filter(|k| !TWINTAIL_KEEPS.contains(&k.as_str()))
                    .collect();
            }
        };
        list.iter().map(|k| (*k).to_string()).collect()
    }

    /// Whether MangoHud and the OBS hook can wrap the game from outside (not into a sandbox).
    pub fn host_wrappers(self) -> bool {
        matches!(self, Runner::Command | Runner::Umu)
    }

    /// What Steam passed, for a game whose command GSet builds: a leftover command (Steam's own
    /// chain, or a script the shortcut used to start) is dropped, and the rest are arguments for
    /// the game (a roblox:// link, Twintail's --install=…).
    pub fn game_args(self, incoming: &[String]) -> Vec<String> {
        if self == Runner::Command {
            return incoming.to_vec();
        }
        let is_command = incoming.first().is_some_and(|a| {
            a.starts_with('/') && std::path::Path::new(a).is_file()
                || a == "steam-launch-wrapper"
                || a.ends_with("/steam-launch-wrapper")
        });
        if is_command {
            Vec::new()
        } else {
            incoming.to_vec()
        }
    }
}

/// What the launcher needs to start a game through TwintailLauncher and see it through.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct TwintailRun {
    /// the install's id in Twintail (its --install=…)
    pub install: String,
    /// look for a newer game version first; if there is one, open Twintail's window instead
    pub update_check: bool,
    /// install a newer Steam Runtime 4 before Twintail starts (a broken one always is)
    pub runtime_update: bool,
    /// MangoHud (switched on in Twintail) starts hidden
    pub mangohud_hidden: bool,
    /// seconds a launch may take before Twintail's window is shown
    pub game_wait: u64,
    /// seconds the game must stay closed before Twintail is quit
    pub exit_grace: u64,
    /// a game that closes sooner than this keeps Twintail open
    pub short_run: u64,
}

/// "1.2.10" is newer than "1.2.9", the way `sort -V` orders them.
pub fn newer(a: &str, than: &str) -> bool {
    fn parts(s: &str) -> Vec<(u64, String)> {
        let mut out = Vec::new();
        let mut num = String::new();
        let mut text = String::new();
        for c in s.chars() {
            if c.is_ascii_digit() {
                if !text.is_empty() {
                    out.push((0, std::mem::take(&mut text)));
                }
                num.push(c);
            } else {
                if !num.is_empty() {
                    out.push((num.parse().unwrap_or(u64::MAX), String::new()));
                    num.clear();
                }
                text.push(c);
            }
        }
        if !num.is_empty() {
            out.push((num.parse().unwrap_or(u64::MAX), String::new()));
        }
        if !text.is_empty() {
            out.push((0, text));
        }
        out
    }
    a != than && parts(a) > parts(than)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert!(newer("7.2.1", "7.2.0"));
        assert!(newer("1.2.10", "1.2.9"));
        assert!(!newer("7.2.0", "7.2.0"));
        assert!(!newer("9.0.0", "9.1.0"));
        assert!(newer("4.0.20261110.1", "4.0.20261020.2"));
    }

    #[test]
    fn steams_leftover_command_is_dropped() {
        let chain: Vec<String> = ["steam-launch-wrapper", "--", "reaper"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert!(Runner::Sober.game_args(&chain).is_empty());
        let link = vec!["roblox://placeId=1".to_string()];
        assert_eq!(Runner::Sober.game_args(&link), link);
        assert_eq!(Runner::Command.game_args(&chain), chain);
    }

    #[test]
    fn twintail_keeps_only_the_mouse_and_the_gpu_check() {
        let ex = Runner::Twintail.excluded();
        assert!(ex.iter().any(|k| k == "gamescope"));
        assert!(!ex.iter().any(|k| k == "mouse" || k == "gpu_check"));
    }
}
