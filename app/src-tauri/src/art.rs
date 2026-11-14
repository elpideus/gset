//! Steam's artwork for the window, served as art://localhost/<id>/<kind>: your own grid images
//! first (Steam's "Set custom artwork"), then Steam's library cache. Only these files are reachable.

use std::borrow::Cow;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use tauri::http::{Request, Response, StatusCode};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Kind {
    Hero,
    Logo,
    Capsule,
    Header,
}

impl Kind {
    pub fn parse(s: &str) -> Option<Kind> {
        Some(match s {
            "hero" => Kind::Hero,
            "logo" => Kind::Logo,
            "capsule" => Kind::Capsule,
            "header" => Kind::Header,
            _ => return None,
        })
    }
    /// file name endings in Steam's grid folder: <id>_hero.png, <id>_logo.png, <id>p.jpg, <id>.jpg
    fn grid_suffix(self) -> &'static str {
        match self {
            Kind::Hero => "_hero",
            Kind::Logo => "_logo",
            Kind::Capsule => "p",
            Kind::Header => "",
        }
    }
    /// names in the library cache
    fn cache_names(self) -> &'static [&'static str] {
        match self {
            Kind::Hero => &["library_hero.jpg", "library_hero.png"],
            Kind::Logo => &["logo.png"],
            Kind::Capsule => &[
                "library_capsule.jpg",
                "library_600x900.jpg",
                "library_capsule.png",
            ],
            Kind::Header => &["library_header.jpg", "header.jpg", "library_header.png"],
        }
    }
}

/// Found files (or None: not there), by game and kind.
type Found = Mutex<HashMap<(u64, Kind), Option<PathBuf>>>;

fn cache() -> &'static Found {
    static C: OnceLock<Found> = OnceLock::new();
    C.get_or_init(Default::default)
}

pub fn find(steam_root: &Path, id: u64, kind: Kind) -> Option<PathBuf> {
    if let Some(hit) = cache()
        .lock()
        .ok()
        .and_then(|c| c.get(&(id, kind)).cloned())
    {
        return hit;
    }
    let found = look(steam_root, id, kind);
    if let Ok(mut c) = cache().lock() {
        c.insert((id, kind), found.clone());
    }
    found
}

fn look(steam_root: &Path, id: u64, kind: Kind) -> Option<PathBuf> {
    // your own artwork wins, as in Steam
    if let Ok(users) = std::fs::read_dir(steam_root.join("userdata")) {
        for u in users.flatten() {
            let grid = u.path().join("config/grid");
            for ext in ["png", "jpg", "jpeg", "webp"] {
                let f = grid.join(format!("{id}{}.{ext}", kind.grid_suffix()));
                if f.is_file() {
                    return Some(f);
                }
            }
        }
    }
    // the library cache: <id>/<hash>/<name> (new), or <id>_<name> (old)
    let dir = steam_root.join("appcache/librarycache");
    let own = dir.join(id.to_string());
    for name in kind.cache_names() {
        if let Some(f) = find_in(&own, name, 2) {
            return Some(f);
        }
        let old = dir.join(format!("{id}_{name}"));
        if old.is_file() {
            return Some(old);
        }
    }
    None
}

fn find_in(dir: &Path, name: &str, depth: u32) -> Option<PathBuf> {
    let direct = dir.join(name);
    if direct.is_file() {
        return Some(direct);
    }
    if depth == 0 {
        return None;
    }
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .find_map(|e| find_in(&e.path(), name, depth - 1))
}

pub fn url(id: u64, kind: &str) -> String {
    format!("art://localhost/{id}/{kind}")
}

pub fn handle(steam_root: &Path, req: &Request<Vec<u8>>) -> Response<Cow<'static, [u8]>> {
    let parts: Vec<&str> = req.uri().path().trim_matches('/').split('/').collect();
    let file = match parts.as_slice() {
        [id, kind] => id
            .parse::<u64>()
            .ok()
            .zip(Kind::parse(kind))
            .and_then(|(id, k)| find(steam_root, id, k)),
        _ => None,
    };
    let Some(bytes) = file.as_ref().and_then(|f| std::fs::read(f).ok()) else {
        return Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Cow::Borrowed(&[][..]))
            .expect("a response");
    };
    let mime = match file
        .as_ref()
        .and_then(|f| f.extension())
        .and_then(|e| e.to_str())
    {
        Some("png") => "image/png",
        Some("webp") => "image/webp",
        _ => "image/jpeg",
    };
    Response::builder()
        .header("Content-Type", mime)
        .header("Cache-Control", "max-age=3600")
        .body(Cow::Owned(bytes))
        .expect("a response")
}
