import { AnimatePresence, LayoutGroup } from "motion/react";
import { useEffect, useRef } from "react";
import { Toaster } from "sonner";
import { inTauri } from "./api";
import { PresetDialog, ReviewDialog, SettingsDialog } from "./components/dialogs";
import { GamePage } from "./components/GamePage";
import { LibraryPage, Sidebar } from "./components/Library";
import { useStore } from "./store";

export function App() {
  const ready = useStore((s) => s.ready);
  const error = useStore((s) => s.error);
  const expanded = useStore((s) => s.expanded);
  const side = useStore((s) => s.side);
  const theme = useStore((s) => s.theme);
  const wasExpanded = useRef(false);

  useEffect(() => {
    void useStore.getState().init();
  }, []);

  // the desktop's accent and light/dark preference
  useEffect(() => {
    const root = document.documentElement;
    if (theme.accent) root.style.setProperty("--signal", theme.accent);
    const dark = theme.scheme ? theme.scheme === "dark" : matchMedia("(prefers-color-scheme: dark)").matches;
    root.dataset.theme = dark ? "dark" : "light";
  }, [theme]);

  // "/" or Ctrl+F: search; Escape: back from the library to the settings
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const typing = /^(INPUT|TEXTAREA)$/.test((e.target as HTMLElement).tagName);
      if ((e.key === "/" && !typing) || (e.key === "f" && e.ctrlKey)) {
        e.preventDefault();
        document.getElementById("search")?.focus();
      } else if (e.key === "Escape" && !typing && useStore.getState().expanded && !useStore.getState().dialog) {
        useStore.getState().setExpanded(false);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  // the window shows once there's something to show (no white flash)
  useEffect(() => {
    if (ready && inTauri) void import("@tauri-apps/api/window").then((w) => w.getCurrentWindow().show());
  }, [ready]);

  const entering = wasExpanded.current && !expanded;
  wasExpanded.current = expanded;

  if (!ready) return <div className="loading">Loading your games…</div>;
  if (error) return <div className="error">GSet couldn’t read its settings: {error}</div>;
  return (
    <div className="app" style={{ ["--side" as string]: `${side}px` }}>
      <LayoutGroup>
        <Sidebar />
        <AnimatePresence mode="popLayout" initial={false}>
          {expanded ? <LibraryPage key="library" /> : <GamePage key="game" entering={entering} />}
        </AnimatePresence>
      </LayoutGroup>
      <SettingsDialog />
      <ReviewDialog />
      <PresetDialog />
      <Toaster position="bottom-right" theme={(theme.scheme as "dark" | "light") ?? "dark"} toastOptions={{ style: { fontFamily: "var(--sans)", fontSize: 13 } }} />
    </div>
  );
}
