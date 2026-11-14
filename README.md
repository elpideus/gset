# GSet

Game launch settings for Linux. One launch option for every Steam game:

```
/home/you/.local/bin/gset %command%
```

GSet finds the game Steam is starting, picks the preset for the screen that's on (your 21:9 monitor
at home, 1920×1080 when you stream to a laptop), and starts the game with its settings: Gamescope,
MangoHud, the OBS capture hook, the graphics API, DLSS presets, frame caps, VSync, environment
variables and more. Then it gets out of the way: GSet replaces itself with the game, so nothing of
it keeps running while you play.

> **Status:** early. The launcher works and is tested against the shell scripts it replaces; the
> settings window, signed releases and the in-app updater come next.

## How settings work

Each game's value comes from these layers, each one overriding the one before:

1. GSet's defaults, which change nothing about how a game starts
2. your defaults for every game
3. the game's own values
4. the preset in use (shared by every game), with this game's own changes to it

**Game profiles** come from the community ([gset-profiles](https://github.com/elpideus/gset-profiles)).
A profile describes a game: its names and notes, which settings apply, settings of its own, and
*suggested* values with a reason. Profiles never change your values. A new game starts from its
profile's suggestions the first time GSet meets it; after that, a suggestion only shows next to the
setting until you use it or hide it. Profiles are data: they can't run commands.

## The files

Everything is plain TOML you can also edit by hand; GSet keeps your comments when it saves.
See [docs/settings.md](docs/settings.md).

| File | What's in it |
|---|---|
| `~/.config/gset/gset.toml` | GSet's options, your defaults for every game, the presets |
| `~/.config/gset/games/<game>.toml` | one game: how it starts, its values, its preset changes |
| `~/.local/state/gset/launch.log` | one entry per launch |

## The launcher

```
gset [--game=NAME] [--preset=NAME] [--dry-run] COMMAND…
gset --import          bring games over from the old launch-options scripts
gset --list            the games GSet knows
gset --screens         the screens that are on, and which preset applies
```

`--dry-run` prints what would run and changes nothing.

## Building

```
cargo build --release
```

## Licence

GPL-3.0-or-later. See [LICENSE](LICENSE).
