//! The screens that are on and their current modes, asked from the Wayland compositor (KDE, GNOME,
//! Hyprland, Sway, … all answer the same way). It takes a few milliseconds, and nothing keeps running.
//! GSET_SCREENS="3440x1440@165,1920x1080@60" replaces the answer (tests, trying presets out).

use serde::Serialize;
use std::time::Duration;
use wayland_client::protocol::{wl_output, wl_registry};
use wayland_client::{Connection, Dispatch, QueueHandle, WEnum};

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Screen {
    pub name: String,
    pub description: String,
    pub width: u32,
    pub height: u32,
    /// Hz, rounded
    pub refresh: u32,
}

impl Screen {
    pub fn mode(&self) -> String {
        format!("{}x{}", self.width, self.height)
    }
}

/// The screens, or none when no compositor answers within half a second.
pub fn screens() -> Vec<Screen> {
    if let Ok(fake) = std::env::var("GSET_SCREENS") {
        return parse_fake(&fake);
    }
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(ask_compositor().unwrap_or_default());
    });
    rx.recv_timeout(Duration::from_millis(500)).unwrap_or_default()
}

fn parse_fake(text: &str) -> Vec<Screen> {
    text.split(',')
        .filter_map(|s| {
            let (size, hz) = s.trim().split_once('@').unwrap_or((s.trim(), "60"));
            let (w, h) = crate::plan::parse_size(size)?;
            Some(Screen { name: "test".into(), description: String::new(), width: w, height: h, refresh: hz.parse().unwrap_or(60) })
        })
        .collect()
}

#[derive(Default)]
struct Output {
    name: String,
    description: String,
    width: i32,
    height: i32,
    refresh_mhz: i32,
}

#[derive(Default)]
struct State {
    outputs: Vec<Output>,
}

impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(state: &mut Self, registry: &wl_registry::WlRegistry, event: wl_registry::Event, _: &(), _: &Connection, qh: &QueueHandle<Self>) {
        if let wl_registry::Event::Global { name, interface, version } = event
            && interface == "wl_output"
        {
            let index = state.outputs.len();
            registry.bind::<wl_output::WlOutput, _, _>(name, version.min(4), qh, index);
            state.outputs.push(Output::default());
        }
    }
}

impl Dispatch<wl_output::WlOutput, usize> for State {
    fn event(state: &mut Self, _: &wl_output::WlOutput, event: wl_output::Event, index: &usize, _: &Connection, _: &QueueHandle<Self>) {
        let Some(out) = state.outputs.get_mut(*index) else { return };
        match event {
            wl_output::Event::Mode { flags: WEnum::Value(flags), width, height, refresh } if flags.contains(wl_output::Mode::Current) => {
                out.width = width;
                out.height = height;
                out.refresh_mhz = refresh;
            }
            wl_output::Event::Name { name } => out.name = name,
            wl_output::Event::Description { description } => out.description = description,
            _ => {}
        }
    }
}

fn ask_compositor() -> Option<Vec<Screen>> {
    let conn = Connection::connect_to_env().ok()?;
    let mut queue = conn.new_event_queue();
    let qh = queue.handle();
    let _registry = conn.display().get_registry(&qh, ());
    let mut state = State::default();
    queue.roundtrip(&mut state).ok()?; // the globals: binds the outputs
    queue.roundtrip(&mut state).ok()?; // the outputs' modes and names
    Some(
        state
            .outputs
            .into_iter()
            .filter(|o| o.width > 0 && o.height > 0)
            .map(|o| Screen {
                name: o.name,
                description: o.description,
                width: o.width as u32,
                height: o.height as u32,
                refresh: ((o.refresh_mhz + 500) / 1000) as u32,
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn fake_screens() {
        let s = super::parse_fake("3440x1440@165, 1920x1080");
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].mode(), "3440x1440");
        assert_eq!(s[0].refresh, 165);
        assert_eq!(s[1].refresh, 60);
    }
}
