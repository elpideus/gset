//! Every setting GSet knows: its name, how it is shown and edited, its default and when it applies.
//! The window builds its pages from this; profiles add choices and game-specific settings to it.

use crate::profile::Profile;
use crate::value::Value;
use crate::when::When;
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Toggle,
    Choice,
    Number,
    Text,
    Resolution,
}

#[derive(Clone, Debug, Serialize)]
pub struct Choice {
    pub value: Value,
    pub label: String,
}

/// What a game-specific setting from a profile does while it is on.
#[derive(Clone, Debug, Default, Serialize)]
pub struct Effect {
    pub env: BTreeMap<String, String>,
    pub args: Vec<String>,
    pub fix: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct SettingDef {
    pub key: String,
    pub label: String,
    pub help: String,
    pub group: String,
    pub kind: Kind,
    pub default: Value,
    pub choices: Vec<Choice>,
    /// a choice that also takes values not in the list
    pub editable: bool,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub step: Option<f64>,
    pub unit: Option<String>,
    pub placeholder: Option<String>,
    /// shown in a monospaced font (commands, variables)
    pub mono: bool,
    pub when: Option<String>,
    /// why the setting doesn't apply, when `when` doesn't hold
    pub off: Option<String>,
    /// profiles may suggest a value for it (never for settings that run commands)
    pub suggestable: bool,
    /// hidden by the game's profile (shown only while you have a value of your own for it)
    pub hidden: bool,
    /// added by the game's profile
    pub from_profile: bool,
    #[serde(skip)]
    pub effect: Effect,
}

#[derive(Clone, Debug, Serialize)]
pub struct Group {
    pub id: &'static str,
    pub title: &'static str,
}

pub const GROUPS: &[Group] = &[
    Group {
        id: "screen",
        title: "Screen",
    },
    Group {
        id: "pacing",
        title: "Frame pacing",
    },
    Group {
        id: "overlay",
        title: "Overlay and capture",
    },
    Group {
        id: "graphics",
        title: "Graphics",
    },
    Group {
        id: "mouse",
        title: "Mouse",
    },
    Group {
        id: "game",
        title: "This game",
    },
    Group {
        id: "advanced",
        title: "Advanced",
    },
];

struct B(SettingDef);

impl B {
    fn new(key: &str, group: &str, label: &str, kind: Kind, default: impl Into<Value>) -> B {
        B(SettingDef {
            key: key.into(),
            label: label.into(),
            help: String::new(),
            group: group.into(),
            kind,
            default: default.into(),
            choices: Vec::new(),
            editable: false,
            min: None,
            max: None,
            step: None,
            unit: None,
            placeholder: None,
            mono: false,
            when: None,
            off: None,
            suggestable: true,
            hidden: false,
            from_profile: false,
            effect: Effect::default(),
        })
    }
    fn help(mut self, h: &str) -> B {
        self.0.help = h.into();
        self
    }
    fn when(mut self, w: &str, off: &str) -> B {
        self.0.when = Some(w.into());
        self.0.off = Some(off.into());
        self
    }
    fn choices(mut self, c: &[(&str, &str)]) -> B {
        self.0.choices = c
            .iter()
            .map(|(v, l)| Choice {
                value: Value::str(*v),
                label: (*l).into(),
            })
            .collect();
        self
    }
    fn number_choices(mut self, c: &[(i64, &str)]) -> B {
        self.0.choices = c
            .iter()
            .map(|(v, l)| Choice {
                value: Value::Int(*v),
                label: (*l).into(),
            })
            .collect();
        self
    }
    fn range(mut self, min: f64, max: f64, step: f64, unit: &str) -> B {
        self.0.min = Some(min);
        self.0.max = Some(max);
        self.0.step = Some(step);
        self.0.unit = (!unit.is_empty()).then(|| unit.into());
        self
    }
    fn editable(mut self) -> B {
        self.0.editable = true;
        self
    }
    fn placeholder(mut self, p: &str) -> B {
        self.0.placeholder = Some(p.into());
        self
    }
    fn mono(mut self) -> B {
        self.0.mono = true;
        self
    }
    fn local_only(mut self) -> B {
        self.0.suggestable = false;
        self
    }
}

/// GSet's own settings. Their defaults change nothing about how a game starts.
pub fn builtin() -> Vec<SettingDef> {
    use Kind::*;
    let gs = "Needs Gamescope";
    vec![
        // screen
        B::new("gamescope", "screen", "Gamescope", Toggle, false)
            .help("Runs the game in its own compositor, to render below the screen’s resolution and upscale."),
        B::new("gamescope_output", "screen", "Output", Resolution, "native")
            .help("The size of Gamescope’s window. Screen: the size of the screen the game starts on.")
            .when("gamescope", gs),
        B::new("render_scale", "screen", "Render scale", Choice, 100)
            .number_choices(&[(100, "100%"), (85, "85%"), (75, "75%"), (67, "67%"), (50, "50%")])
            .editable()
            .when("gamescope", gs),
        B::new("upscaler", "screen", "Upscaler", Choice, "fsr")
            .choices(&[("fsr", "AMD FSR 1"), ("nis", "NVIDIA NIS"), ("linear", "Linear"), ("integer", "Integer")])
            .when("gamescope, render_scale<100", "Only below 100%"),
        B::new("refresh_rate", "screen", "Refresh rate", Choice, 0)
            .number_choices(&[(0, "The screen’s"), (240, "240 Hz"), (165, "165 Hz"), (160, "160 Hz"), (144, "144 Hz"), (120, "120 Hz"), (60, "60 Hz")])
            .editable()
            .when("gamescope", "Needs Gamescope; otherwise the screen’s own"),
        B::new("adaptive_sync", "screen", "Adaptive sync", Toggle, false).help("VRR inside Gamescope.").when("gamescope", gs),
        B::new("fullscreen", "screen", "Fullscreen", Toggle, true).when("gamescope", gs),
        B::new("clipboard", "screen", "Paste from the desktop", Toggle, false)
            .help("Hands the game the text you copy on the desktop (Gamescope only passes the clipboard out of the game).")
            .when("gamescope", gs),
        B::new("gamescope_args", "screen", "More Gamescope options", Text, "")
            .placeholder("--hdr-enabled")
            .mono()
            .local_only()
            .when("gamescope", gs),
        B::new("game_resolution", "screen", "Game resolution", Resolution, "")
            .help("The resolution the game starts at, through Unity’s or Unreal’s launch arguments.")
            .when("engine=unity|unreal", "Only for Unity and Unreal games"),
        // frame pacing
        B::new("vsync", "pacing", "VSync", Choice, "game")
            .choices(&[("game", "Game"), ("off", "Off"), ("on", "On"), ("fast", "Fast")])
            .help("Forced underneath the game. Fast: no tearing, no cap."),
        B::new("frame_cap", "pacing", "Frame cap", Choice, 0)
            .number_choices(&[(0, "Off"), (30, "30 FPS"), (60, "60 FPS"), (120, "120 FPS"), (141, "141 FPS"), (160, "160 FPS")])
            .editable(),
        // overlay and capture
        B::new("mangohud", "overlay", "MangoHud", Toggle, false).help("FPS, frame times and temperatures."),
        B::new("mangohud_hidden", "overlay", "Start hidden", Toggle, true)
            .help("Right Shift + F12 shows it.")
            .when("mangohud", "Needs MangoHud"),
        B::new("obs_capture", "overlay", "OBS game capture", Toggle, false).help("Lets OBS record the game directly (obs-vkcapture)."),
        // graphics
        B::new("graphics_api", "graphics", "Graphics API", Choice, "steam")
            .choices(&[("steam", "Steam"), ("d3d11", "DX11"), ("d3d12", "DX12"), ("vulkan", "Vulkan")])
            .help("Replaces the one Steam’s launch dialog picks.")
            .when("engine=unity|unreal", "This game picks its own"),
        B::new("dlss_preset", "graphics", "DLSS preset", Choice, "game")
            .choices(&[("game", "Game decides"), ("K", "K"), ("J", "J"), ("E", "E")])
            .help("Only does something while DLSS is on in the game."),
        B::new("low_latency", "graphics", "Low latency", Toggle, false).help("A shorter frame queue (DXVK low-latency builds)."),
        B::new("nvidia_only", "graphics", "NVIDIA card only", Toggle, false)
            .help("DXVK and vkd3d-proton never fall back to another GPU."),
        B::new("vkd3d_config", "graphics", "vkd3d-proton options", Choice, "")
            .choices(&[("", "Default"), ("descriptor_heap", "Descriptor heap")])
            .editable()
            .mono(),
        B::new("gpu_check", "graphics", "Check the GPU first", Toggle, false)
            .help("A quick Vulkan test on the NVIDIA card: if the driver refuses it, you get a notification instead of a crash."),
        B::new("vram_warn", "graphics", "Warn below this much free video memory", Number, 3000)
            .range(0.0, 49152.0, 250.0, "MiB")
            .when("gpu_check", "Needs the GPU check"),
        // mouse
        B::new("mouse", "mouse", "Slow the mouse", Choice, "off")
            .choices(&[("dpi", "DPI"), ("gamescope", "Gamescope"), ("off", "Off")])
            .help("Only while the game is focused."),
        B::new("mouse_dpi", "mouse", "DPI in game", Number, 1800)
            .range(100.0, 30000.0, 100.0, "DPI")
            .when("mouse=dpi", "Only with DPI"),
        B::new("gamescope_mouse", "mouse", "Gamescope mouse multiplier", Number, 0.2)
            .range(0.05, 1.0, 0.05, "×")
            .help("Mouse movement × this value. Slow moves can get lost.")
            .when("mouse=gamescope", "Only with Gamescope"),
        // advanced
        B::new("run_before", "advanced", "Run before", Text, "")
            .help("Commands in front of the game, such as taskset or gamemoderun. VAR=value words set variables.")
            .placeholder("gamemoderun")
            .mono()
            .local_only(),
        B::new("game_args", "advanced", "Game arguments", Text, "").placeholder("-skipintro").mono().local_only(),
        B::new("env", "advanced", "Environment", Text, "")
            .help("Variables for the game, as VAR=value words.")
            .placeholder("PROTON_ENABLE_NVAPI=1")
            .mono()
            .local_only(),
        B::new("strip_wrappers", "advanced", "Drop the launcher’s own wrappers", Toggle, false)
            .help("Removes MangoHud and GameMode that Heroic or Lutris put in front of the game, so these settings alone decide."),
    ]
    .into_iter()
    .map(|b| b.0)
    .collect()
}

/// The settings of one game: GSet's own, with what its profile hides and adds.
#[derive(Clone, Debug, Serialize)]
pub struct Schema {
    pub settings: Vec<SettingDef>,
}

impl Schema {
    pub fn for_game(profile: Option<&Profile>) -> Schema {
        let mut settings = builtin();
        if let Some(p) = profile {
            for def in &mut settings {
                if p.hide.contains(&def.key) {
                    def.hidden = true;
                }
                if let Some(extra) = p.options.get(&def.key) {
                    for c in extra {
                        if !def.choices.iter().any(|have| have.value.same(&c.value)) {
                            def.choices.push(crate::schema::Choice {
                                value: c.value.clone(),
                                label: c.label.clone(),
                            });
                        }
                    }
                }
            }
            for (key, s) in &p.settings {
                let mut def = B::new(key, "game", &s.label, Kind::Toggle, false)
                    .help(&s.help)
                    .0;
                def.when = s.when.clone();
                def.off = s.off.clone();
                def.from_profile = true;
                def.effect = Effect {
                    env: s.env.clone(),
                    args: s.args.clone(),
                    fix: s.fix.clone(),
                };
                settings.push(def);
            }
        }
        Schema { settings }
    }

    pub fn get(&self, key: &str) -> Option<&SettingDef> {
        self.settings.iter().find(|d| d.key == key)
    }

    pub fn defaults(&self) -> BTreeMap<String, Value> {
        self.settings
            .iter()
            .map(|d| (d.key.clone(), d.default.clone()))
            .collect()
    }

    /// Whether a setting applies, given the other values.
    pub fn applies(&self, key: &str, values: &BTreeMap<String, Value>) -> bool {
        match self.get(key).and_then(|d| d.when.as_deref()) {
            Some(w) => When::parse(w).map(|w| w.holds(values)).unwrap_or(true),
            None => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_settings_are_consistent() {
        let all = builtin();
        let mut keys: Vec<_> = all.iter().map(|d| d.key.as_str()).collect();
        keys.sort_unstable();
        let n = keys.len();
        keys.dedup();
        assert_eq!(n, keys.len(), "two settings share a key");
        for d in &all {
            assert!(
                GROUPS.iter().any(|g| g.id == d.group),
                "{} has an unknown group",
                d.key
            );
            if let Some(w) = &d.when {
                let parsed = When::parse(w).unwrap_or_else(|e| panic!("{}: {e}", d.key));
                for k in parsed.keys() {
                    assert!(
                        k == "engine" || all.iter().any(|o| o.key == k),
                        "{} depends on unknown {k}",
                        d.key
                    );
                }
            }
            if d.kind == Kind::Choice && !d.editable {
                assert!(
                    d.choices.iter().any(|c| c.value.same(&d.default)),
                    "{}: default isn't a choice",
                    d.key
                );
            }
        }
    }
}
