# Releasing

A release is a `v*` tag on `main`. Pushing one starts
[the release workflow](../.github/workflows/release.yml), which builds, signs
and publishes everything. Nobody builds release files by hand.

## Cutting a release

```bash
node scripts/release.mjs 0.2.0
```

The script refuses unless the working tree is clean, the branch is `main` and
`main` is not behind `origin/main`. Then it sets the version in every file that
holds it ([scripts/version.mjs](../scripts/version.mjs)), commits
"Release v0.2.0", tags `v0.2.0` and pushes both. `--no-push` stops before the
push and prints the command for later.

A version with a `-`, such as `0.3.0-beta.1`, is a **pre-release**. It is
published like any release, but never marked as the latest, so installed copies
do not update to it and the README's download links skip it.

The version lives in `Cargo.toml` (`[workspace.package]`), `Cargo.lock`,
`app/package.json` and `app/src-tauri/tauri.conf.json`. CI fails when they
disagree:

```bash
node scripts/version.mjs --check
```

## What the workflow does

1. **Checks**: the whole of [CI](../.github/workflows/ci.yml) runs on the tag.
2. **Build**, on Ubuntu 22.04 so the binaries need no newer glibc than it has:
   `GSet-x86_64.AppImage`, `gset_amd64.deb` and `gset.x86_64.rpm`, each with
   the `gset` launcher inside ([scripts/sidecar.mjs](../scripts/sidecar.mjs)
   builds it first), and the launcher alone as `gset-x86_64-linux`. The build
   never sees the signing key.
3. **Sign**, on a fresh runner with no checkout and no cache, in the `release`
   environment: the three packages get an updater signature (a `.sig` file
   beside each, made by the Tauri CLI and bound to the version).
4. **Publish**: the release notes (the commit subjects since the previous
   release, then how to install), `latest.json`
   ([scripts/updater-manifest.mjs](../scripts/updater-manifest.mjs)) and
   `SHA256SUMS`, in a GitHub release named "GSet 0.2.0".

The files have no version in their names, so
`releases/latest/download/<file>` always reaches the newest one.

Running the workflow by hand (Actions › Release › Run workflow) builds
everything from the chosen branch and keeps it as workflow artifacts, unsigned,
publishing nothing. Use it to try a build before tagging.

## How installed copies update

`plugins.updater` in `app/src-tauri/tauri.conf.json` points the app at
`https://github.com/elpideus/gset/releases/latest/download/latest.json` and
holds the public half of the updater key. `latest.json` names, for each kind of
installation, the file to download and its signature:

| Target | File |
| --- | --- |
| `linux-x86_64-appimage` | `GSet-x86_64.AppImage` |
| `linux-x86_64-deb` | `gset_amd64.deb` |
| `linux-x86_64-rpm` | `gset.x86_64.rpm` |

When the window opens (and "Check for updates" is on), GSet asks for
`latest.json`. A newer version shows in the sidebar and in Settings, where
"Update and restart" downloads it, refuses it unless its signature checks out
against the public key, installs it and starts GSet again. A launch from Steam
never waits on this.

From an AppImage, GSet copies its launcher to `~/.local/bin/gset` at start
whenever the AppImage carries a different one, so an update reaches the
launcher Steam runs as well. The packages install the launcher as
`/usr/bin/gset`.

The repository must stay public: installed apps read releases without signing
in.

## The signing key

The private key is the `TAURI_SIGNING_PRIVATE_KEY` secret of the repository's
`release` environment (it has no passphrase, so
`TAURI_SIGNING_PRIVATE_KEY_PASSWORD` is unset). Lose it and installed copies
can never update by themselves again: keep a private backup.
