// Small pieces of logic the window needs: conditions (as gset-core reads them), labels, finding games.

import type { Choice, GameView, LibraryGame, SettingDef, Source, Value } from "./api";

/** Whether a setting's condition holds ("gamescope", "mouse=dpi", "render_scale<100", commas = and). */
export function holds(when: string | null | undefined, values: Record<string, Value>): boolean {
  if (!when) return true;
  return when
    .split(",")
    .map((p) => p.trim())
    .filter(Boolean)
    .every((part) => {
      const truthy = (v: Value | undefined) => v === true || v === 1 || v === "1" || v === "true" || v === "on";
      const eq = (v: Value | undefined, t: string) =>
        typeof v === "string" ? v === t : typeof v === "boolean" ? v === ["1", "true", "on", "yes"].includes(t) : Number(t) === v;
      let m: RegExpMatchArray | null;
      if ((m = part.match(/^([\w.]+)\s*!=\s*(.*)$/))) return !m[2].split("|").some((t) => eq(values[m![1]], t.trim()));
      if ((m = part.match(/^([\w.]+)\s*=\s*(.*)$/))) return m[2].split("|").some((t) => eq(values[m![1]], t.trim()));
      if ((m = part.match(/^([\w.]+)\s*<\s*(.*)$/))) return Number(values[m[1]]) < Number(m[2]);
      if ((m = part.match(/^([\w.]+)\s*>\s*(.*)$/))) return Number(values[m[1]]) > Number(m[2]);
      if (part.startsWith("!")) return !truthy(values[part.slice(1)]);
      return truthy(values[part]);
    });
}

export const same = (a: Value | undefined, b: Value | undefined) =>
  a === b || (typeof a !== "string" && typeof b !== "string" && Number(a) === Number(b)) || String(a) === String(b);

export function label(def: SettingDef | undefined, v: Value | undefined): string {
  if (!def) return String(v ?? "");
  if (def.kind === "toggle") return v ? "On" : "Off";
  const c = def.choices.find((c: Choice) => same(c.value, v));
  if (c) return c.label;
  if (def.kind === "resolution") return v === "native" ? "Screen" : v === "" ? "Off" : String(v).replace("x", " × ");
  if (def.unit) return `${v} ${def.unit}`;
  return v === "" ? "None" : String(v);
}

export function presetLabel(g: GameView | null, id: string): string {
  if (id === "base") return "Base";
  const p = g?.presets.find((p) => p.id === id);
  return p ? (p.ratio ? `${p.name} ${p.ratio}` : p.name) : id;
}

export function sourceText(g: GameView, src: Source | undefined): string | null {
  if (!src) return null;
  if (src.kind === "preset") return presetLabel(g, src.preset).split(" ")[0];
  if (src.kind === "game_preset") return `${presetLabel(g, src.preset).split(" ")[0]} · this game`;
  return null;
}

export function ago(secs?: number | null): string {
  if (!secs) return "never";
  const d = Date.now() / 1000 - secs;
  if (d < 3600) return "just now";
  if (d < 86400) return `${Math.round(d / 3600)} h ago`;
  if (d < 86400 * 30) return `${Math.round(d / 86400)} days ago`;
  return new Date(secs * 1000).toLocaleDateString(undefined, { day: "numeric", month: "short", year: "numeric" });
}

export const fmtSize = (s?: string | null) => (s ? s.replace("x", " × ") : "");

/** A shell-quoted command, for showing and copying. */
export function quote(argv: string[]): string {
  return argv.map((a) => (/^[\w@%+=:,./-]+$/.test(a) ? a : `'${a.replace(/'/g, "'\\''")}'`)).join(" ");
}

// ------------------------------------------------------------------ finding games

export type Sort = "played" | "name" | "changed" | "updates";
export const SORTS: [Sort, string][] = [
  ["played", "Recently played"],
  ["name", "Name"],
  ["changed", "Recently changed"],
  ["updates", "Updates first"],
];

export interface Filter {
  id: string;
  title: string;
  opts: [string, string][];
  test: (g: LibraryGame, v: string) => boolean;
}

const runnerKind = (g: LibraryGame) => {
  const r = (g.runner ?? "").toLowerCase();
  if (g.script) return "script";
  if (r.includes("dw")) return "dw";
  if (r.includes("cachyos")) return "cachyos";
  if (r.includes("ge")) return "ge";
  if (r.includes("proton")) return "proton";
  return "default";
};

export const FILTERS: Filter[] = [
  {
    id: "source",
    title: "Started by",
    opts: [["steam", "Steam"], ["shortcut", "Steam shortcut"], ["other", "Other launchers"]],
    test: (g, v) => {
      const kind = (g.via ?? "").includes("Heroic") || !g.steam ? "other" : g.steam > 0x7fffffff ? "shortcut" : "steam";
      return kind === v;
    },
  },
  {
    id: "runner",
    title: "Runs on",
    opts: [["dw", "DW-Proton"], ["cachyos", "Proton-CachyOS"], ["ge", "GE-Proton"], ["proton", "Valve's Proton"], ["default", "Steam's default"], ["script", "Its own script"]],
    test: (g, v) => runnerKind(g) === v,
  },
  {
    id: "engine",
    title: "Engine",
    opts: [["unreal", "Unreal"], ["unity", "Unity"], ["other", "Other"]],
    test: (g, v) => (v === "other" ? !["unreal", "unity"].includes(g.engine ?? "") : g.engine === v),
  },
  {
    id: "status",
    title: "Status",
    opts: [["update", "Profile update"], ["suggest", "Has suggestions"], ["noprofile", "No profile yet"]],
    test: (g, v) => (v === "update" ? g.update : v === "suggest" ? g.suggestions > 0 : !g.profile && !g.script),
  },
];

export function findGames(lib: LibraryGame[], q: string, filters: Record<string, string[]>, sort: Sort): LibraryGame[] {
  const words = q.toLowerCase().split(/\s+/).filter(Boolean);
  const hit = (g: LibraryGame) => {
    const hay = [g.name, g.key, g.runner ?? "", g.engine ?? "", ...g.aliases].join(" ").toLowerCase();
    return words.every((w) => hay.includes(w));
  };
  const by: Record<Sort, (a: LibraryGame, b: LibraryGame) => number> = {
    played: (a, b) => (b.last_played ?? 0) - (a.last_played ?? 0) || a.name.localeCompare(b.name),
    name: (a, b) => a.name.localeCompare(b.name),
    changed: (a, b) => (b.changed ?? 0) - (a.changed ?? 0) || a.name.localeCompare(b.name),
    updates: (a, b) => Number(b.update) - Number(a.update) || b.suggestions - a.suggestions || (b.last_played ?? 0) - (a.last_played ?? 0),
  };
  return lib
    .filter((g) => hit(g) && FILTERS.every((f) => !filters[f.id]?.length || filters[f.id].some((v) => f.test(g, v))))
    .sort(by[sort]);
}

export const optCount = (lib: LibraryGame[], f: Filter, v: string) => lib.filter((g) => f.test(g, v)).length;
