// The window's state, and every change it makes (each goes through GSet's Rust side and comes back
// as the game's new state). Changes can be undone from their toast.

import { toast } from "sonner";
import { create } from "zustand";
import { api, type AppUpdate, type AppView, type GameView, type LibraryGame, type Theme, type Value } from "./api";
import { label, presetLabel, type Sort } from "./logic";

const stored = <T,>(key: string, fallback: T): T => {
  try {
    const v = localStorage.getItem(`gset.${key}`);
    return v === null ? fallback : (JSON.parse(v) as T);
  } catch {
    return fallback;
  }
};
const remember = (key: string, v: unknown) => {
  try {
    localStorage.setItem(`gset.${key}`, JSON.stringify(v));
  } catch {
    /* private window: nothing to remember */
  }
};

interface State {
  ready: boolean;
  error: string | null;
  library: LibraryGame[];
  selected: string | null;
  game: GameView | null;
  view: string;
  everywhere: boolean;
  expanded: boolean;
  q: string;
  filters: Record<string, string[]>;
  sort: Sort;
  side: number;
  app: AppView | null;
  theme: Theme;
  dialog: null | "settings" | "review" | "preset";
  editPreset: string | null;
  appUpdate: AppUpdate | null;
  /** null: not updating; a number: how much is downloaded */
  updating: number | null;

  init(): Promise<void>;
  select(key: string): Promise<void>;
  refreshLibrary(): Promise<void>;
  setView(v: string): void;
  setEverywhere(b: boolean): void;
  setValue(setting: string, value: Value | null): Promise<void>;
  useSuggestions(keys: string[]): Promise<void>;
  dismiss(setting: string): Promise<void>;
  pin(preset: string | null): Promise<void>;
  applyUpdate(): Promise<void>;
  play(): Promise<void>;
  setExpanded(b: boolean): void;
  setQ(q: string): void;
  toggleFilter(group: string, v: string): void;
  clearFilters(): void;
  setSort(s: Sort): void;
  setSide(px: number): void;
  open(d: State["dialog"], preset?: string | null): void;
  setAppOption(key: string, v: Value): Promise<void>;
  savePreset(id: string | null, name: string, screen: string | null): Promise<void>;
  deletePreset(id: string): Promise<void>;
  checkUpdate(): Promise<void>;
  refreshProfiles(quiet: boolean): Promise<void>;
  installUpdate(): Promise<void>;
}

const fail = (e: unknown) => toast.error(String(e));

export const useStore = create<State>((set, get) => {
  /** Replaces the game, keeping the view on a preset that still exists. */
  const took = (g: GameView) => {
    const view = get().view === "base" || g.views[get().view] ? get().view : "base";
    set({ game: g, view });
    void get().refreshLibrary();
  };

  return {
    ready: false,
    error: null,
    library: [],
    selected: stored<string | null>("selected", null),
    game: null,
    view: "base",
    everywhere: false,
    expanded: false,
    q: "",
    filters: {},
    sort: stored<Sort>("sort", "played"),
    side: stored("side", 344),
    app: null,
    theme: {},
    dialog: null,
    editPreset: null,
    appUpdate: null,
    updating: null,

    async init() {
      try {
        const b = await api();
        const [library, app, theme] = await Promise.all([b.library(), b.appSettings(), b.theme()]);
        set({ library, app, theme });
        b.onTheme((t) => set({ theme: t }));
        const first = library.find((g) => g.key === get().selected) ?? [...library].sort((a, b) => (b.last_played ?? 0) - (a.last_played ?? 0))[0];
        if (first) await get().select(first.key);
        set({ ready: true });
        if (app.check_updates) {
          void get().checkUpdate();
          void get().refreshProfiles(true);
        }
      } catch (e) {
        set({ error: String(e), ready: true });
      }
    },

    async select(key) {
      try {
        const g = await (await api()).game(key);
        const view = get().view === "base" || g.views[get().view] ? get().view : "base";
        set({ selected: key, game: g, view });
        remember("selected", key);
      } catch (e) {
        fail(e);
      }
    },

    async refreshLibrary() {
      try {
        set({ library: await (await api()).library() });
      } catch (e) {
        fail(e);
      }
    },

    setView: (v) => set({ view: v }),
    setEverywhere: (b) => set({ everywhere: b }),

    async setValue(setting, value) {
      const { game, view, everywhere } = get();
      if (!game) return;
      const before = game.views[view]?.values[setting];
      const wasOwn = view === "base" ? game.views.base.source[setting]?.kind === "game" : game.views[view]?.source[setting]?.kind !== "default";
      try {
        const b = await api();
        took(await b.setValue(game.key, view, setting, value, view !== "base" && everywhere));
        const def = game.settings.find((d) => d.key === setting);
        // toggles and dropdowns change in place; a toast only when it reset something
        if (value === null) {
          toast(`${def?.label ?? setting} is back to ${view === "base" ? "the default" : presetLabel(game, view).split(" ")[0] + "’s value"}.`, {
            action: {
              label: "Undo",
              onClick: async () => took(await b.setValue(game.key, view, setting, wasOwn ? (before as Value) : null, view !== "base" && everywhere)),
            },
          });
        }
      } catch (e) {
        fail(e);
      }
    },

    async useSuggestions(keys) {
      const { game } = get();
      if (!game || !keys.length) return;
      const before = Object.fromEntries(keys.map((k) => [k, game.views.base.source[k]?.kind === "game" ? game.views.base.values[k] : null]));
      try {
        const b = await api();
        took(await b.useSuggestions(game.key, keys));
        const def = game.settings.find((d) => d.key === keys[0]);
        toast(keys.length > 1 ? `Used ${keys.length} suggested values.` : `${def?.label} set to ${label(def, game.suggestions[keys[0]].value)}.`, {
          action: {
            label: "Undo",
            onClick: async () => {
              let g = game;
              for (const [k, v] of Object.entries(before)) g = await b.setValue(game.key, "base", k, v as Value | null);
              took(g);
            },
          },
        });
      } catch (e) {
        fail(e);
      }
    },

    async dismiss(setting) {
      const { game } = get();
      if (!game) return;
      try {
        const b = await api();
        took(await b.dismissSuggestion(game.key, setting, true));
        toast("Suggestion hidden. It shows again if the profile suggests something else.", {
          action: { label: "Undo", onClick: async () => took(await b.dismissSuggestion(game.key, setting, false)) },
        });
      } catch (e) {
        fail(e);
      }
    },

    async pin(preset) {
      const { game } = get();
      if (!game) return;
      try {
        took(await (await api()).pin(game.key, preset));
      } catch (e) {
        fail(e);
      }
    },

    async applyUpdate() {
      const { game } = get();
      if (!game?.update) return;
      const to = game.update.to;
      try {
        const b = await api();
        took(await b.applyUpdate(game.key));
        set({ dialog: null });
        toast(`${game.info.name} profile updated to v${to}. Your settings didn’t change.`, {
          action: { label: "Undo", onClick: async () => took(await b.undoUpdate(game.key)) },
        });
      } catch (e) {
        fail(e);
      }
    },

    async play() {
      const { game } = get();
      if (!game) return;
      try {
        await (await api()).play(game.key);
        toast(`Asking Steam to start ${game.info.name}…`);
      } catch (e) {
        fail(e);
      }
    },

    setExpanded: (b) => set({ expanded: b }),
    setQ: (q) => set({ q }),
    toggleFilter(group, v) {
      const cur = get().filters[group] ?? [];
      set({ filters: { ...get().filters, [group]: cur.includes(v) ? cur.filter((x) => x !== v) : [...cur, v] } });
    },
    clearFilters: () => set({ filters: {} }),
    setSort(s) {
      set({ sort: s });
      remember("sort", s);
    },
    setSide(px) {
      set({ side: px });
      remember("side", px);
    },
    open: (d, preset = null) => set({ dialog: d, editPreset: preset }),

    async setAppOption(key, v) {
      try {
        set({ app: await (await api()).setAppOption(key, v) });
        if (key === "auto_profile_updates" && v) await get().refreshLibrary();
      } catch (e) {
        fail(e);
      }
    },
    async savePreset(id, name, screen) {
      try {
        const app = await (await api()).savePreset(id, name, screen);
        set({ app, dialog: null });
        if (get().selected) await get().select(get().selected!);
        if (!id) {
          const made = app.presets[app.presets.length - 1];
          if (made) set({ view: made.id });
        }
      } catch (e) {
        fail(e);
      }
    },
    async checkUpdate() {
      try {
        set({ appUpdate: await (await api()).checkUpdate() });
      } catch (e) {
        // offline, or GitHub unreachable: say nothing, try again next time
        console.warn("update check:", e);
      }
    },
    async refreshProfiles(quiet) {
      try {
        const r = await (await api()).refreshProfiles();
        const names = (l: string[]) => (l.length > 3 ? `${l.slice(0, 3).join(", ")} and ${l.length - 3} more` : l.join(", "));
        const said: string[] = [];
        if (r.new.length) said.push(`New ${r.new.length === 1 ? "profile" : "profiles"}: ${names(r.new)}.`);
        if (r.updates.length) said.push(`${r.updates.length === 1 ? "A profile update" : "Profile updates"} for ${names(r.updates)}.`);
        if (said.length || r.linked.length) {
          await get().refreshLibrary();
          if (get().selected) await get().select(get().selected!);
          set({ app: await (await api()).appSettings() });
        }
        if (said.length) toast(said.join(" "));
        else if (!quiet) toast("All profiles are up to date.");
        if (r.problems.length) console.warn("profiles:", r.problems);
      } catch (e) {
        if (!quiet) fail(`Couldn’t check the profiles: ${e}`);
        else console.warn("profiles:", e);
      }
    },
    async installUpdate() {
      try {
        set({ updating: 0 });
        await (await api()).installUpdate((f) => set({ updating: f ?? 0 }));
      } catch (e) {
        set({ updating: null });
        fail(`The update didn’t install: ${e}`);
      }
    },
    async deletePreset(id) {
      try {
        const app = await (await api()).deletePreset(id);
        set({ app, dialog: null, view: get().view === id ? "base" : get().view });
        if (get().selected) await get().select(get().selected!);
      } catch (e) {
        fail(e);
      }
    },
  };
});
