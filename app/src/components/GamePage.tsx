// One game: its settings in the Base view or in a preset's, what the profile suggests, how Steam
// starts it, and the command GSet builds from all of it.

import clsx from "clsx";
import { AlertTriangle, Check, Copy, Pin, PinOff, Play, Plus, RotateCcw, X } from "lucide-react";
import { motion } from "motion/react";
import type { GameView, SettingDef, Value } from "../api";
import { fmtSize, holds, label, presetLabel, quote, same, sourceText } from "../logic";
import { useStore } from "../store";
import { Control, Segmented } from "./controls";
import { Ratio } from "./Mark";
import { toast } from "sonner";

const copy = (text: string, what: string) => {
  void navigator.clipboard.writeText(text).then(
    () => toast(`${what} copied.`),
    () => toast.error("Couldn’t copy to the clipboard."),
  );
};

function renderSize(g: GameView, values: Record<string, Value>): string | null {
  const out = String(values.gamescope_output ?? "native");
  const screen = [...g.screens].sort((a, b) => b.width * b.height - a.width * a.height)[0];
  const [w, h] = out === "native" ? (screen ? [screen.width, screen.height] : [0, 0]) : out.split("x").map(Number);
  const s = Number(values.render_scale ?? 100) / 100;
  if (!w || !h || !s) return null;
  const even = (x: number) => Math.round((x * s) / 2) * 2;
  return `${even(w)} × ${even(h)}`;
}

function Row({ def, g }: { def: SettingDef; g: GameView }) {
  const view = useStore((s) => s.view);
  const everywhere = useStore((s) => s.everywhere);
  const setValue = useStore((s) => s.setValue);
  const useSuggestions = useStore((s) => s.useSuggestions);
  const dismiss = useStore((s) => s.dismiss);
  const V = g.views[view] ?? g.views.base;
  const value = V.values[def.key];
  const src = V.source[def.key];
  const on = holds(def.when, V.values);
  const base = view === "base";
  const sug = base ? g.suggestions[def.key] : undefined;
  const chip = sug && g.open.includes(def.key);
  const matches = sug && same(sug.value, value);
  const fromPreset = !base && (src?.kind === "preset" || src?.kind === "game_preset");
  const resettable = base ? !sug && src?.kind === "game" && !same(value, def.default) : everywhere ? src?.kind === "preset" : src?.kind === "game_preset";
  const resetTitle = base ? "Back to the default" : everywhere ? "Take it out of the preset" : `Back to ${presetLabel(g, view).split(" ")[0]}’s value`;
  const help = def.key === "render_scale" && on ? <small>Renders at <span className="calc">{renderSize(g, V.values)}</span></small> : def.help ? <small>{def.help}</small> : null;
  return (
    <div className="row" data-off={on ? undefined : ""}>
      <div className="lab">
        <b>{def.label}</b>
        {on ? help : <small>{def.off}</small>}
        {chip && (
          <>
            <span className="sugg">
              <span>
                Suggested: <b>{label(def, sug.value)}</b>
              </span>
              <button type="button" className="sugg-use" onClick={() => void useSuggestions([def.key])}>
                Use
              </button>
              <button type="button" className="sugg-x" aria-label="Hide this suggestion" title="Hide this suggestion" onClick={() => void dismiss(def.key)}>
                <X size={12} />
              </button>
            </span>
            {sug.why && <small className="why">{sug.why}</small>}
          </>
        )}
      </div>
      <div className="ctl">
        {def.from_profile && <span className="src new" title="A setting of this game’s own, from its profile">Game</span>}
        {fromPreset && <span className="src preset">{sourceText(g, src)}</span>}
        {!fromPreset && matches && (
          <span className="src sugd" title={sug.why || "The profile suggests this value."}>
            <Check size={11} strokeWidth={2.5} />
            Suggested
          </span>
        )}
        {resettable && (
          <button type="button" className="reset" title={resetTitle} aria-label={resetTitle} onClick={() => void setValue(def.key, null)}>
            <RotateCcw size={13} />
          </button>
        )}
        <Control def={def} value={value} disabled={!on} screens={g.screens} onChange={(v) => void setValue(def.key, v)} />
      </div>
    </div>
  );
}

function Orphan({ def, why, g }: { def: SettingDef; why: string; g: GameView }) {
  const setValue = useStore((s) => s.setValue);
  const value = g.views.base.values[def.key];
  return (
    <div className="row">
      <div className="lab">
        <b>{def.label}</b>
        <small>
          {why || "The profile no longer lists it."} Yours ({label(def, value)}) stays applied until you remove it.
        </small>
      </div>
      <div className="ctl">
        <button type="button" className="btn ghost" onClick={() => void setValue(def.key, null)}>
          Remove
        </button>
      </div>
    </div>
  );
}

function Tabs({ g }: { g: GameView }) {
  const view = useStore((s) => s.view);
  const setView = useStore((s) => s.setView);
  const pin = useStore((s) => s.pin);
  const open = useStore((s) => s.open);
  const inUse = g.in_use.preset ?? "base";
  const why = g.in_use.why;
  const preset = g.presets.find((p) => p.id === view);
  const ruleText = (() => {
    if (view === "base") {
      if (inUse === "base") return g.presets.length ? "No preset matches this screen" : "Presets switch settings with the screen";
      return `${presetLabel(g, inUse)} is in use${why.kind === "screen" ? `: the screen is ${why.mode.replace("x", " × ")}` : why.kind === "pinned" ? " (pinned)" : ""}`;
    }
    if (g.pinned === view) return "Pinned: used whatever the screen";
    return preset?.screen ? `Used when the screen is ${preset.screen.replace("x", " × ")}` : "Used only when pinned";
  })();
  return (
    <div className="tabs" role="tablist">
      <button type="button" className="tab" role="tab" aria-selected={view === "base"} onClick={() => setView("base")}>
        Base
      </button>
      {g.presets.map((p) => (
        <button type="button" key={p.id} className="tab" role="tab" aria-selected={view === p.id} onClick={() => setView(p.id)} onDoubleClick={() => open("preset", p.id)} title="Double-click to rename it or change its screen">
          <Ratio ratio={p.ratio} />
          {p.name}
          {p.ratio ? ` ${p.ratio}` : ""}
          {inUse === p.id && <span className="use">In use</span>}
        </button>
      ))}
      <button type="button" className="tab add" onClick={() => open("preset", null)}>
        <Plus size={13} />
        Preset
      </button>
      <span className="rule-line">
        {ruleText}
        {view !== "base" && (
          <>
            <button type="button" className="btn ghost" onClick={() => void pin(g.pinned === view ? null : view)}>
              {g.pinned === view ? <PinOff size={14} /> : <Pin size={14} />}
              {g.pinned === view ? "Unpin" : "Pin"}
            </button>
            <button type="button" className="btn ghost" onClick={() => open("preset", view)}>
              Edit
            </button>
          </>
        )}
      </span>
    </div>
  );
}

/** On a preset's tab that the next launch won't use: why not, and a way to use it. */
function NotUsed({ g, view }: { g: GameView; view: string }) {
  const pin = useStore((s) => s.pin);
  const inUse = g.in_use.preset ?? "base";
  const p = g.presets.find((x) => x.id === view);
  if (!p || inUse === view) return null;
  const short = g.info.name.split(":")[0];
  const name = presetLabel(g, view);
  const now = g.screens.map((sc) => `${sc.width}x${sc.height}`);
  let why: string;
  if (g.pinned && g.pinned !== view) why = `${presetLabel(g, g.pinned)} is pinned for ${short}.`;
  else if (!p.screen) why = `${name} has no screen, so it’s used only when pinned.`;
  else if (now.includes(p.screen)) why = `${presetLabel(g, inUse)} matches this screen first.`;
  else why = `${name} is for a ${fmtSize(p.screen)} screen${now.length ? `, and the screen is ${now.map(fmtSize).join(" and ")} now` : ""}.`;
  return (
    <div className="notused" role="status">
      <AlertTriangle size={14} />
      <span>
        <b>{short}’s next launch won’t use {name}.</b> {why}
      </span>
      <button
        type="button"
        className="btn primary"
        onClick={() => void pin(view).then(() => useStore.getState().game?.pinned === view && toast(`${short} now starts with ${name}, whatever the screen. Unpin it to go back.`))}
      >
        <Pin size={14} />
        Use it for {short}
      </button>
    </div>
  );
}

function Launch({ g }: { g: GameView }) {
  const L = g.launch;
  const steamNow = L.steam_now?.trim();
  let status: React.ReactNode;
  if (g.info.script) status = <span className="status">Started by its own script</span>;
  else if (L.kind === "steam" && L.uses_gset)
    status = (
      <span className="status">
        <Check size={13} />
        Steam runs GSet for this game
      </span>
    );
  else if (L.kind === "steam")
    status = (
      <span className="status warn" title={steamNow ? `Steam runs: ${steamNow}` : undefined}>
        <AlertTriangle size={13} />
        {steamNow ? "Steam still runs something else" : "Not in Steam’s launch options yet"}
      </span>
    );
  else status = <span className="status">{L.kind === "shortcut" ? "In the shortcut’s launch options" : "As the launcher’s wrapper command"}</span>;
  return (
    <div className="launch">
      <div className="lab">
        <b>{L.kind === "other" ? "Wrapper command" : "Launch option"}</b>
        <code className="selectable">{L.line}</code>
        {L.kind === "steam" && !L.uses_gset && steamNow && <code className="selectable" style={{ color: "var(--ink-3)" }}>now: {steamNow}</code>}
      </div>
      {status}
      <button type="button" className="btn" onClick={() => copy(L.line, "Launch option")}>
        <Copy size={14} />
        Copy
      </button>
    </div>
  );
}

function CommandBar({ g }: { g: GameView }) {
  const view = useStore((s) => s.view);
  const V = g.views[view] ?? g.views.base;
  const inUse = (g.in_use.preset ?? "base") === view;
  const caption = view === "base" ? (g.in_use.preset ? "Base, without a preset" : "Base") : `${presetLabel(g, view)}${inUse ? ", in use" : ", not used now"}`;
  const env = V.command.env;
  const shown = V.command.argv;
  return (
    <footer className="cmdbar">
      <div className="k">
        <b>Runs</b>
        {caption}
      </div>
      <div className="cmd selectable">
        {env.length > 0 && (
          <>
            <span className="envc" title={env.map(([k, v]) => `${k}=${v}`).join("\n")}>
              {env.length} {env.length === 1 ? "variable" : "variables"}
            </span>{" "}
          </>
        )}
        {shown.map((a, i) => (
          <span key={i} className={a === "%command%" ? "steam" : undefined}>
            {i > 0 ? " " : ""}
            {a === "%command%" ? "%command%" : quote([a])}
          </span>
        ))}
        {V.command.notes.length > 0 && (
          <div style={{ color: "var(--warn)", fontFamily: "var(--sans)", fontSize: 12 }}>{V.command.notes.join(" · ")}</div>
        )}
      </div>
      <button
        type="button"
        className="btn ghost icon"
        title="Copy the command"
        aria-label="Copy the command"
        onClick={() => copy([...env.map(([k, v]) => quote([`${k}=${v}`])), quote(shown)].join(" "), "Command")}
      >
        <Copy size={14} />
      </button>
    </footer>
  );
}

export function GamePage({ entering }: { entering: boolean }) {
  const g = useStore((s) => s.game);
  const view = useStore((s) => s.view);
  const everywhere = useStore((s) => s.everywhere);
  const setEverywhere = useStore((s) => s.setEverywhere);
  const useSuggestions = useStore((s) => s.useSuggestions);
  const play = useStore((s) => s.play);
  const open = useStore((s) => s.open);
  if (!g) return <div className="loading">No game yet. Run gset --import, or start a game with gset %command%.</div>;
  const short = g.info.name.split(":")[0];
  const groups = g.groups
    .map((grp) => ({ ...grp, defs: g.settings.filter((d) => d.group === grp.id && !d.hidden) }))
    .filter((grp) => grp.defs.length > 0);
  const notes = g.profile?.notes ?? [];
  return (
    <motion.main
      key={g.key}
      className="main"
      initial={entering ? { opacity: 0, x: 28 } : false}
      animate={{ opacity: 1, x: 0 }}
      transition={{ duration: 0.32, ease: [0.23, 1, 0.32, 1], delay: entering ? 0.06 : 0 }}
    >
      <div className="scroll">
        <header className="hero">
          {g.art.hero && <img className="h" src={g.art.hero} alt="" />}
          {g.art.logo ? <img className="logo" src={g.art.logo} alt={g.info.name} /> : <h1>{g.info.name}</h1>}
          <div className="act">
            <button type="button" className="btn primary" disabled={g.launch.kind === "other"} title={g.launch.kind === "other" ? "Start it from its own launcher" : "Start it through Steam"} onClick={() => void play()}>
              <Play size={13} fill="currentColor" />
              Play
            </button>
          </div>
        </header>
        <div className="meta">
          <span>{[g.info.via, g.info.runner, g.info.engine && g.info.engine[0].toUpperCase() + g.info.engine.slice(1)].filter(Boolean).join(" · ")}</span>
          {g.info.steam && <span>{g.info.steam}</span>}
          <span>{g.profile ? `Community profile v${g.profile.version}` : g.info.script ? "No GSet settings" : "No community profile yet"}</span>
          {notes.map((n) => (
            <span key={n} className="note">
              {n}
            </span>
          ))}
        </div>
        <div className="body">
          {g.update && (
            <div className="upd">
              <div className="t">
                <b>Profile update</b>
                <span>
                  {short} v{g.update.from} → v{g.update.to} · {g.update.changes.length} {g.update.changes.length === 1 ? "change" : "changes"}. Your settings stay as they are.
                </span>
              </div>
              <button type="button" className="btn ghost" onClick={() => open("review")}>
                Review
              </button>
              <button type="button" className="btn primary" onClick={() => void useStore.getState().applyUpdate()}>
                Update
              </button>
            </div>
          )}
          {g.info.script ? (
            <div className="script-note">
              {g.info.name} is started by a script of yours: <code>{g.info.script}</code>. GSet runs it as it is, so the settings below don’t apply to this game; its own settings stay in
              {" "}
              <code>~/.config/launch-options/{g.key}.conf</code>.
            </div>
          ) : (
            <>
              {view === "base" && g.open.length > 1 && (
                <div className="sugbar">
                  <span>{g.open.length} suggested values differ from yours.</span>
                  <button type="button" className="btn ghost" onClick={() => void useSuggestions(g.open)}>
                    Use all
                  </button>
                </div>
              )}
              <Tabs g={g} />
              {view !== "base" && <NotUsed g={g} view={view} />}
              {view !== "base" && (
                <div className="scope">
                  Changes here apply to
                  <Segmented
                    small
                    label="Changes apply to"
                    value={everywhere ? "all" : "game"}
                    options={[
                      { value: "game", label: `${short} only` },
                      { value: "all", label: "Every game" },
                    ]}
                    onChange={(v) => setEverywhere(v === "all")}
                  />
                </div>
              )}
              <div className={clsx("cols")}>
                {groups.map((grp) => (
                  <section key={grp.id} className="group">
                    <h3>{grp.id === "game" ? `${short} only` : grp.title}</h3>
                    {grp.defs.map((d) => (
                      <Row key={d.key} def={d} g={g} />
                    ))}
                  </section>
                ))}
                {g.orphans.length > 0 && (
                  <section className="group">
                    <h3>No longer in the profile</h3>
                    {g.orphans.map((o) => {
                      const def = g.settings.find((d) => d.key === o.key);
                      return def ? <Orphan key={o.key} def={def} why={o.why} g={g} /> : null;
                    })}
                  </section>
                )}
              </div>
            </>
          )}
          <Launch g={g} />
        </div>
      </div>
      {!g.info.script && <CommandBar g={g} />}
    </motion.main>
  );
}
