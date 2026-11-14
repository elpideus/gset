// What the window and GSet's Rust side say to each other. In the Tauri window the calls go to
// src-tauri/src/views.rs; in a plain browser (pnpm dev) a mock answers from dev-fixture/.

export type Value = boolean | number | string;

export type Source =
  | { kind: "default" | "all_games" | "game" }
  | { kind: "preset" | "game_preset"; preset: string };

export interface Art {
  capsule?: string | null;
  header?: string | null;
  hero?: string | null;
  logo?: string | null;
}

export interface LibraryGame {
  key: string;
  name: string;
  via?: string | null;
  runner?: string | null;
  steam?: number | null;
  engine?: string | null;
  aliases: string[];
  art: Art;
  script: boolean;
  profile: boolean;
  update: boolean;
  suggestions: number;
  last_played?: number | null;
  changed?: number | null;
}

export type Kind = "toggle" | "choice" | "number" | "text" | "resolution";

export interface Choice {
  value: Value;
  label: string;
}

export interface SettingDef {
  key: string;
  label: string;
  help: string;
  group: string;
  kind: Kind;
  default: Value;
  choices: Choice[];
  editable: boolean;
  min?: number | null;
  max?: number | null;
  step?: number | null;
  unit?: string | null;
  placeholder?: string | null;
  mono: boolean;
  when?: string | null;
  off?: string | null;
  suggestable: boolean;
  hidden: boolean;
  from_profile: boolean;
}

export interface CommandView {
  argv: string[];
  env: [string, string][];
  notes: string[];
}

export interface ViewValues {
  values: Record<string, Value>;
  source: Record<string, Source>;
  command: CommandView;
}

export type Change =
  | { type: "new_setting"; key: string; label: string; help: string }
  | { type: "removed_setting"; key: string; label: string; why: string }
  | { type: "suggestion"; key: string; label: string; value: Value; why: string; before?: Value | null }
  | { type: "dropped_suggestion"; key: string; label: string }
  | { type: "new_choice"; key: string; label: string; choice: string }
  | { type: "notes"; notes: string[] }
  | { type: "name"; name: string };

export interface Screen {
  name: string;
  description: string;
  width: number;
  height: number;
  refresh: number;
}

export interface PresetView {
  id: string;
  name: string;
  screen?: string | null;
  ratio?: string | null;
  values: Record<string, Value>;
}

export type PresetWhy = { kind: "asked" | "pinned" | "none" } | { kind: "screen"; mode: string };

export interface GameView {
  key: string;
  path: string;
  info: {
    name: string;
    steam?: number | null;
    via?: string | null;
    engine?: string | null;
    profile?: string | null;
    mouse_section?: string | null;
    script?: string | null;
    runner?: string | null;
  };
  art: Art;
  profile?: { id: string; version: number; notes: string[] } | null;
  update?: { from: number; to: number; changes: Change[] } | null;
  groups: { id: string; title: string }[];
  settings: SettingDef[];
  views: Record<string, ViewValues>;
  suggestions: Record<string, { value: Value; why: string; dismissed: boolean }>;
  open: string[];
  orphans: { key: string; why: string }[];
  presets: PresetView[];
  in_use: { preset?: string | null; why: PresetWhy };
  pinned?: string | null;
  launch: { kind: "steam" | "shortcut" | "other"; line: string; steam_now?: string | null; uses_gset?: boolean | null };
  screens: Screen[];
  last_played?: number | null;
}

export interface AppView {
  version: string;
  check_updates: boolean;
  auto_profile_updates: boolean;
  new_games: "suggested" | "defaults";
  launcher?: string | null;
  presets: PresetView[];
  screens: Screen[];
  config_dir: string;
  profiles: number;
  problems: string[];
}

export interface Theme {
  accent?: string | null;
  scheme?: "dark" | "light" | null;
}

export interface Backend {
  library(): Promise<LibraryGame[]>;
  game(key: string): Promise<GameView>;
  setValue(key: string, view: string, setting: string, value: Value | null, everywhere?: boolean): Promise<GameView>;
  useSuggestions(key: string, settings: string[]): Promise<GameView>;
  dismissSuggestion(key: string, setting: string, hide?: boolean): Promise<GameView>;
  pin(key: string, preset: string | null): Promise<GameView>;
  applyUpdate(key: string): Promise<GameView>;
  undoUpdate(key: string): Promise<GameView>;
  play(key: string): Promise<void>;
  appSettings(): Promise<AppView>;
  setAppOption(key: string, value: Value): Promise<AppView>;
  savePreset(id: string | null, name: string, screen: string | null): Promise<AppView>;
  deletePreset(id: string): Promise<AppView>;
  theme(): Promise<Theme>;
  onTheme(cb: (t: Theme) => void): void;
}

export const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

async function tauriBackend(): Promise<Backend> {
  const { invoke } = await import("@tauri-apps/api/core");
  const { listen } = await import("@tauri-apps/api/event");
  return {
    library: () => invoke("library"),
    game: (key) => invoke("game", { key }),
    setValue: (key, view, setting, value, everywhere) => invoke("set_value", { key, view, setting, value, everywhere: everywhere ?? false }),
    useSuggestions: (key, settings) => invoke("use_suggestions", { key, settings }),
    dismissSuggestion: (key, setting, hide) => invoke("dismiss_suggestion", { key, setting, hide: hide ?? true }),
    pin: (key, preset) => invoke("pin", { key, preset }),
    applyUpdate: (key) => invoke("apply_update", { key }),
    undoUpdate: (key) => invoke("undo_update", { key }),
    play: (key) => invoke("play", { key }),
    appSettings: () => invoke("app_settings"),
    setAppOption: (key, value) => invoke("set_app_option", { key, value }),
    savePreset: (id, name, screen) => invoke("save_preset", { id, name, screen }),
    deletePreset: (id) => invoke("delete_preset", { id }),
    theme: () => invoke("theme"),
    onTheme: (cb) => {
      void listen<Theme>("theme-changed", (e) => cb(e.payload));
    },
  };
}

let backend: Promise<Backend> | null = null;

export function api(): Promise<Backend> {
  backend ??= inTauri ? tauriBackend() : import("./mock").then((m) => m.mockBackend());
  return backend;
}
