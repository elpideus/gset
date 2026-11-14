//! The launcher Steam runs (`gset %command%`) ships inside the app, next to it.
//!
//! Steam shouldn't start every game through an AppImage: it mounts itself and sets library paths
//! that could reach the game. So when GSet runs from an AppImage, it keeps a copy of the launcher in
//! ~/.local/bin/gset, refreshed whenever the AppImage carries a different one (after an update).
//! From a .deb or .rpm the launcher is installed beside the app, in /usr/bin.
//!
//! The copy written to ~/.local/bin comes from inside this program, not from the AppImage's
//! usr/bin: the AppImage's packager gives every program there a RUNPATH of $ORIGIN/../lib, which
//! outside the AppImage would make the launcher look in ~/.local/lib before the system's libraries.

use gset_core::Paths;
use std::path::PathBuf;

/// The launcher as cargo built it, untouched (build.rs says where it is).
const CLEAN: &[u8] = include_bytes!(env!("GSET_LAUNCHER_PATH"));

/// The launcher that came with this copy of the app.
fn bundled() -> Option<PathBuf> {
    let p = std::env::current_exe().ok()?.parent()?.join("gset");
    p.is_file().then_some(p)
}

fn in_appimage() -> bool {
    std::env::var_os("APPIMAGE").is_some()
}

/// Writes the launcher to ~/.local/bin/gset when it's missing or different.
/// Returns what it did, for the log.
pub fn ensure(paths: &Paths) -> Option<String> {
    if !in_appimage() {
        return None;
    }
    write_to(&paths.home.join(".local/bin/gset"))
}

fn write_to(dst: &std::path::Path) -> Option<String> {
    let new = CLEAN;
    if std::fs::read(dst).ok().as_deref() == Some(new) {
        return None;
    }
    let tmp = dst.with_file_name(".gset.new");
    let done = (|| -> std::io::Result<()> {
        use std::os::unix::fs::PermissionsExt;
        std::fs::create_dir_all(dst.parent().expect("a folder"))?;
        std::fs::write(&tmp, new)?;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))?;
        std::fs::rename(&tmp, dst)
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

#[cfg(test)]
mod tests {
    #[test]
    fn writes_the_clean_launcher_once() {
        let d = std::env::temp_dir().join(format!("gset-launcher-test-{}", std::process::id()));
        let dst = d.join("bin/gset");
        assert!(super::write_to(&dst).unwrap().starts_with("installed"));
        assert_eq!(std::fs::read(&dst).unwrap(), super::CLEAN);
        assert!(
            super::write_to(&dst).is_none(),
            "an identical launcher isn't written again"
        );
        // the embedded launcher is the plain ELF cargo made: no RUNPATH pointing outside it
        assert!(super::CLEAN.starts_with(b"\x7fELF"));
        assert!(!super::CLEAN.windows(13).any(|w| w == b"$ORIGIN/../li"));
        let _ = std::fs::remove_dir_all(&d);
    }
}
