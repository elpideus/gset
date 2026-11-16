# Settings files

## `~/.config/gset/gset.toml`

```toml
[app]
check_updates = true          # look for a new GSet when the window opens
auto_profile_updates = false  # update game profiles without asking (they never change your values)
new_games = "suggested"       # a new game starts from its profile's suggestions ("defaults": not)

[defaults]                    # your values for every game; a game's own value wins
mangohud = true
obs_capture = true

[presets.work]
name = "Work"
screen = "1920x1080"          # used by itself while a screen in this mode is on
[presets.work.settings]
gamescope_output = "1920x1080"
refresh_rate = 60
vsync = "on"
```

A preset is picked, in this order, by `gset --preset=NAME`, by the game's `pinned` preset, or by the
first preset whose `screen` matches a screen that's on.

## `~/.config/gset/games/<game>.toml`

```toml
[game]
name = "NTE: Neverness to Everness"
steam = 4508340               # Steam app id, or a non-Steam shortcut's id
profile = "nte"               # the community profile that describes it
engine = "unreal"             # unity or unreal: how the API and resolution reach the game
via = "Steam"
mouse_section = "NTE"         # game-mouse's section, for the DPI switch
# script = "~/.local/bin/x"   # a script of yours starts the game instead (the settings don't apply)
# runner = "umu"              # GSet starts a game outside Steam itself: sober, umu or twintail

[settings]
gamescope = true
render_scale = 75

[presets.work]                # this game's own changes to the shared Work preset
render_scale = 100

[dismissed]                   # suggestions you hid, with the value suggested then
vsync = "off"

# pinned = "work"             # use this preset whatever the screen
```

## Games outside Steam

A game file with a `runner` is started by GSet itself, not from the command Steam passes. In
Steam, the game is a non-Steam shortcut whose **Target** is the launcher (`/home/you/.local/bin/gset`)
and whose Launch Options are empty: Steam's ids tell gset which game it is. Outside Steam,
`gset --game=<game>` starts it the same way.

| Runner | For | Its own settings |
|---|---|---|
| `sober` | Roblox through Sober (the Flatpak `org.vinegarhq.Sober`). MangoHud and the OBS hook reach it as Vulkan layers inside the sandbox (`--env`), with the Flatpak extensions for its runtime. | none: MangoHud, OBS capture, the mouse, variables and arguments are GSet's own |
| `umu` | A Windows game outside Steam, through `umu-run` | `umu_program` (the .exe), `wine_prefix`, `proton` (a Proton folder), `umu_game_id` (umu's fixes, `umu-default` for none) |
| `twintail` | A game installed with TwintailLauncher, which starts it with its own per-install settings (runner, MangoHud, FPS unlock…) | `twintail_install` (its `--install` id), `tt_update_check`, `tt_runtime_update`, `tt_mangohud_hidden`, `tt_game_wait`, `tt_exit_grace`, `tt_short_run` |

Sober and umu-run start from the desktop's environment, not Steam's runtime (umu-run also
without Steam's ids). For Twintail, gset stays while the game runs: it checks for a game update
first (Twintail's window opens instead when there is one), installs a newer Steam Runtime 4
before Twintail starts, and quits Twintail a few seconds after the game closes, unless the game
closed within a minute. A runner shows only the settings that reach the game: a Twintail game
keeps the mouse and the GPU check.

## The settings

| Key | What it does |
|---|---|
| `gamescope` | run the game in Gamescope |
| `gamescope_output` | Gamescope's size: `native` (the screen's) or `WIDTHxHEIGHT` |
| `render_scale` | the game's render size in %, upscaled to the output |
| `upscaler` | `fsr`, `nis`, `linear`, `integer` (below 100% only) |
| `refresh_rate` | Gamescope's refresh rate; 0 = the screen's |
| `adaptive_sync`, `fullscreen` | Gamescope's VRR and fullscreen |
| `clipboard` | paste from the desktop into Gamescope (needs gamescope-clipboard) |
| `gamescope_args` | more Gamescope options |
| `game_resolution` | the game's start resolution (Unity and Unreal games) |
| `vsync` | `game`, `off`, `on`, `fast`: forced through MangoHud, or DXVK without it |
| `frame_cap` | DXVK_FRAME_RATE and VKD3D_FRAME_RATE; 0 = off |
| `mangohud`, `mangohud_hidden` | the overlay, and starting it hidden (Right Shift+F12) |
| `obs_capture` | the obs-vkcapture hook |
| `graphics_api` | `steam`, `d3d11`, `d3d12`, `vulkan` (Unity and Unreal games) |
| `dlss_preset` | `game` or a preset letter, through dxvk-nvapi |
| `low_latency` | PROTON_DXVK_LOWLATENCY |
| `nvidia_only` | DXVK and vkd3d-proton use only the NVIDIA card |
| `vkd3d_config` | VKD3D_CONFIG |
| `gpu_check`, `vram_warn` | test the NVIDIA card before starting; warn below this many MiB free |
| `mouse` | `off`, `dpi` (game-mouse lowers the DPI while focused), `gamescope` (Gamescope's multiplier) |
| `mouse_dpi`, `gamescope_mouse` | the DPI, or the multiplier |
| `run_before` | commands in front of everything; `VAR=value` words set variables |
| `game_args` | arguments added after the game's command |
| `env` | variables, as `VAR=value` words |
| `strip_wrappers` | drop the MangoHud and GameMode a launcher (Heroic) puts first |

A game's profile can add settings of its own, such as NTE's `launcher_windows` fix.
