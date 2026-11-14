//! The launcher Steam runs (`gset %command%`) ships inside the app, next to it.
//!
//! Steam shouldn't start every game through an AppImage: it mounts itself and sets library paths
//! that could reach the game. So when GSet runs from an AppImage, it keeps a copy of the launcher in
//! ~/.local/bin/gset, refreshed whenever the AppImage carries a different one (after an update).
//! From a .deb or .rpm the launcher is installed beside the app, in /usr/bin.

use gset_core::Paths;
use std::path::PathBuf;

/// The launcher that came with this copy of the app.
fn bundled() -> Option<PathBuf> {
    let p = std::env::current_exe().ok()?.parent()?.join("gset");
    p.is_file().then_some(p)
}

fn in_appimage() -> bool {
    std::env::var_os("APPIMAGE").is_some()
}

/// Copies the AppImage's launcher to ~/.local/bin/gset when it's missing or different.
/// Returns what it did, for the log.
pub fn ensure(paths: &Paths) -> Option<String> {
    if !in_appimage() {
        return None;
    }
    let src = bundled()?;
    let dst = paths.home.join(".local/bin/gset");
    let new = std::fs::read(&src).ok()?;
    if std::fs::read(&dst).ok().as_deref() == Some(new.as_slice()) {
        return None;
    }
    let tmp = dst.with_file_name(".gset.new");
    let done = (|| -> std::io::Result<()> {
        use std::os::unix::fs::PermissionsExt;
        std::fs::create_dir_all(dst.parent().expect("a folder"))?;
        std::fs::write(&tmp, &new)?;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))?;
        std::fs::rename(&tmp, &dst)
    })();
    Some(match done {
        Ok(()) => format!("installed the launcher in {}", dst.display()),
        Err(e) => format!("couldn't install the launcher in {}: {e}", dst.display()),
    })
}

/// The launcher Steam's launch option should name.
pub fn path(paths: &Paths) -> Option<PathBuf> {
    let local = paths.home.join(".local/bin/gset");
    if in_appimage() {
        return local.is_file().then_some(local);
    }
    if let Some(b) = bundled() {
        return Some(b);
    }
    if local.is_file() {
        return Some(local);
    }
    std::env::var_os("PATH").and_then(|p| {
        std::env::split_paths(&p)
            .map(|d| d.join("gset"))
            .find(|f| f.is_file())
    })
}
