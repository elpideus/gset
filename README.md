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

> **Status:** early (0.1). It works, and it's tested against the launch scripts it replaced, but
> expect rough edges.

## Install

| | Download |
| --- | --- |
| **Any distribution** (Arch, CachyOS, SteamOS, Bazzite…) | [GSet-x86_64.AppImage](https://github.com/elpideus/gset/releases/latest/download/GSet-x86_64.AppImage). Make it executable and run it. |
| **Debian, Ubuntu** and derivatives | [gset_amd64.deb](https://github.com/elpideus/gset/releases/latest/download/gset_amd64.deb) |
| **Fedora, openSUSE** and derivatives | [gset.x86_64.rpm](https://github.com/elpideus/gset/releases/latest/download/gset.x86_64.rpm) |
| **Only the launcher**, no window | [gset-x86_64-linux](https://github.com/elpideus/gset/releases/latest/download/gset-x86_64-linux), to put in your PATH as `gset` |

Then, in Steam: a game → Properties → Launch Options:

```
/home/you/.local/bin/gset %command%
```

For a non-Steam game added to Steam, the line goes in the same Launch Options field, never in
the shortcut's Target: Steam fills in `%command%` only in Launch Options. GSet can also start some
games outside Steam by itself: Roblox through Sober, Windows games through umu-run, and games
installed with TwintailLauncher. Their shortcut's Target is the launcher itself; see
[docs/settings.md](docs/settings.md#games-outside-steam).

The AppImage puts the launcher in `~/.local/bin/gset` the first time it runs; the packages install
it as `/usr/bin/gset`. GSet's window shows the exact line for each game, with a Copy button.
GSet updates itself from its settings (signed updates from this repository's releases).

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

GSet downloads the profiles when its window opens (never while a game starts). The bundle is signed
with the same key as GSet's releases, and a bundle that doesn't match is thrown away. A newer
version of a profile you have waits behind an **Update** button with a list of what changes, unless
you turn on *Update profiles automatically*.

## The files

Everything is plain TOML you can also edit by hand; GSet keeps your comments when it saves.
See [docs/settings.md](docs/settings.md).

| File | What's in it |
|---|---|
| `~/.config/gset/gset.toml` | GSet's options, your defaults for every game, the presets |
| `~/.config/gset/games/<game>.toml` | one game: how it starts, its values, its preset changes |
| `~/.local/share/gset/profiles/` | the downloaded profiles (updates you haven't taken wait in `profiles-pending/`) |
| `~/.local/state/gset/launch.log` | one entry per launch |

## The launcher

```
gset [--game=NAME] [--preset=NAME] [--dry-run] COMMAND…
gset --import          bring games over from the old launch-options scripts
gset --list            the games GSet knows
gset --screens         the screens that are on, and which preset applies
gset --check-profiles=DIR   check a folder of profiles (what gset-profiles' CI runs)
```

`--dry-run` prints what would run and changes nothing.

## Building

```
cargo build --release            # the launcher (target/release/gset)
cd app && pnpm install && pnpm tauri build --no-bundle   # the window (target/release/gset-app)
```

The window needs WebKitGTK 4.1 and its development files. Releases are built and signed by GitHub
Actions: see [docs/releasing.md](docs/releasing.md).

## Licence

GPL-3.0-or-later. See [LICENSE](LICENSE).
