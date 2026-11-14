//! The desktop's accent colour and light/dark preference, from the XDG desktop portal (KDE, GNOME and
//! others answer the same way), and live changes to them. On this PC the accent follows the wallpaper.

use dbus::arg::{ArgType, RefArg, Variant};
use dbus::blocking::Connection;
use dbus::message::MatchRule;
use serde::Serialize;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

#[derive(Clone, Debug, Default, Serialize)]
pub struct Theme {
    /// #rrggbb, when the desktop has one
    pub accent: Option<String>,
    /// "dark", "light", or None when the desktop doesn't say
    pub scheme: Option<String>,
}

const DEST: &str = "org.freedesktop.portal.Desktop";
const PATH: &str = "/org/freedesktop/portal/desktop";
const IFACE: &str = "org.freedesktop.portal.Settings";

pub fn read() -> Theme {
    let Ok(conn) = Connection::new_session() else { return Theme::default() };
    let proxy = conn.with_proxy(DEST, PATH, Duration::from_millis(500));
    let one = |key: &str| -> Option<Variant<Box<dyn RefArg>>> {
        proxy.method_call(IFACE, "ReadOne", ("org.freedesktop.appearance", key)).ok().map(|(v,): (Variant<Box<dyn RefArg>>,)| v)
    };
    let accent = one("accent-color").and_then(|v| {
        let rgb: Vec<f64> = unwrap(&*v.0).as_iter()?.filter_map(|x| x.as_f64()).collect();
        (rgb.len() == 3 && rgb.iter().all(|c| (0.0..=1.0).contains(c)))
            .then(|| format!("#{:02x}{:02x}{:02x}", (rgb[0] * 255.0).round() as u8, (rgb[1] * 255.0).round() as u8, (rgb[2] * 255.0).round() as u8))
    });
    let scheme = one("color-scheme").and_then(|v| unwrap(&*v.0).as_u64()).and_then(|n| match n {
        1 => Some("dark".to_string()),
        2 => Some("light".to_string()),
        _ => None,
    });
    Theme { accent, scheme }
}

/// Values can arrive wrapped in a variant once more.
fn unwrap(a: &dyn RefArg) -> &dyn RefArg {
    if a.arg_type() == ArgType::Variant
        && let Some(mut it) = a.as_iter()
        && let Some(inner) = it.next()
    {
        return unwrap(inner);
    }
    a
}

/// Tells the window when the accent or the light/dark preference changes.
pub fn watch(app: AppHandle) {
    std::thread::spawn(move || {
        let Ok(conn) = Connection::new_session() else { return };
        let rule = MatchRule::new_signal(IFACE, "SettingChanged");
        let handle = app.clone();
        if conn
            .add_match(rule, move |(): (), _, _| {
                let _ = handle.emit("theme-changed", read());
                true
            })
            .is_err()
        {
            return;
        }
        loop {
            if conn.process(Duration::from_secs(60)).is_err() {
                break;
            }
        }
    });
}
