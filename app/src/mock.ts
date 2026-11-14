// A stand-in for GSet's Rust side when the window runs in a plain browser (pnpm dev), answering from
// JSON written by `gset-app --dump-fixture public/dev-fixture`. Changes live in memory only. It only
// mirrors what the window needs to look right; the real rules live in gset-core.

import type { AppView, Backend, GameView, LibraryGame, Theme, Value } from "./api";

const base = "/dev-fixture";
const games = new Map<string, GameView>();
let lib: LibraryGame[] = [];
let app: AppView;

const artUrl = (u?: string | null) => (u ? u.replace(/^art:\/\/localhost\/(\d+)\/(\w+)$/, `${base}/art/$1/$2`) : u);

function fixArt<T extends { art: GameView["art"] }>(x: T): T {
  x.art = { capsule: artUrl(x.art.capsule), header: artUrl(x.art.header), hero: artUrl(x.art.hero), logo: artUrl(x.art.logo) };
  return x;
}

async function load<T>(path: string): Promise<T> {
  const r = await fetch(`${base}/${path}`);
  if (!r.ok) throw new Error(`${path}: ${r.status}`);
  return r.json() as Promise<T>;
}

async function game(key: string): Promise<GameView> {
  let g = games.get(key);
  if (!g) {
    g = fixArt(await load<GameView>(`games/${key}.json`));
    games.set(key, g);
  }
  return structuredClone(g);
}

function save(g: GameView): GameView {
  const open = Object.entries(g.suggestions)
    .filter(([k, s]) => !s.dismissed && g.views.base.values[k] !== s.value && !g.settings.find((d) => d.key === k)?.hidden)
    .map(([k]) => k);
  g.open = open;
  games.set(g.key, g);
  const entry = lib.find((x) => x.key === g.key);
  if (entry) {
    entry.suggestions = open.length;
    entry.update = !!g.update;
    entry.changed = Math.floor(Date.now() / 1000);
  }
  return structuredClone(g);
}

const wait = () => new Promise((r) => setTimeout(r, 40));

export async function mockBackend(): Promise<Backend> {
  lib = (await load<LibraryGame[]>("library.json")).map(fixArt);
  app = await load<AppView>("app.json");
  const params = new URLSearchParams(location.search);
  const listeners: ((t: Theme) => void)[] = [];
  return {
    library: async () => structuredClone(lib),
    game,
    async setValue(key, view, setting, value, everywhere) {
      await wait();
      const g = await game(key);
      const target = g.views[view] ?? g.views.base;
      const def = g.settings.find((d) => d.key === setting);
      if (value === null) {
        target.values[setting] = view === "base" ? (def?.default as Value) : (g.views.base.values[setting] as Value);
        target.source[setting] = { kind: "default" };
      } else {
        target.values[setting] = value;
        target.source[setting] = view === "base" ? { kind: "game" } : everywhere ? { kind: "preset", preset: view } : { kind: "game_preset", preset: view };
      }
      if (view === "base") {
        for (const p of g.presets) {
          const pv = g.views[p.id];
          if (pv && (pv.source[setting]?.kind ?? "game") === "game") pv.values[setting] = target.values[setting];
        }
      }
      return save(g);
    },
    async useSuggestions(key, settings) {
      await wait();
      const g = await game(key);
      for (const k of settings) {
        g.views.base.values[k] = g.suggestions[k].value;
        g.views.base.source[k] = { kind: "game" };
      }
      return save(g);
    },
    async dismissSuggestion(key, setting, hide = true) {
      const g = await game(key);
      g.suggestions[setting].dismissed = hide;
      return save(g);
    },
    async pin(key, preset) {
      const g = await game(key);
      g.pinned = preset;
      g.in_use = preset ? { preset, why: { kind: "pinned" } } : g.in_use.why.kind === "pinned" ? { preset: null, why: { kind: "none" } } : g.in_use;
      return save(g);
    },
    async applyUpdate(key) {
      await wait();
      const g = await game(key);
      if (g.update && g.profile) g.profile.version = g.update.to;
      g.update = null;
      return save(g);
    },
    async undoUpdate(key) {
      return game(key);
    },
    async play() {},
    appSettings: async () => structuredClone(app),
    async setAppOption(key, value) {
      (app as unknown as Record<string, Value>)[key] = value;
      return structuredClone(app);
    },
    async savePreset(id, name, screen) {
      const existing = app.presets.find((p) => p.id === id);
      if (existing) Object.assign(existing, { name, screen });
      else {
        const nid = name.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "") || "preset";
        const [w, h] = (screen ?? "").split("x").map(Number);
        const ratio = w && h ? (Math.abs(w / h - 2.37) < 0.06 ? "21:9" : Math.abs(w / h - 16 / 9) < 0.03 ? "16:9" : null) : null;
        app.presets.push({ id: nid, name, screen, ratio, values: {} });
        for (const g of games.values()) g.views[nid] = structuredClone(g.views.base);
      }
      for (const g of games.values()) g.presets = structuredClone(app.presets);
      return structuredClone(app);
    },
    async deletePreset(id) {
      app.presets = app.presets.filter((p) => p.id !== id);
      for (const g of games.values()) {
        g.presets = structuredClone(app.presets);
        delete g.views[id];
      }
      return structuredClone(app);
    },
    theme: async () => ({ accent: params.get("accent") ?? "#e6a5d9", scheme: (params.get("scheme") as Theme["scheme"]) ?? "dark" }),
    onTheme: (cb) => listeners.push(cb),
  };
}
