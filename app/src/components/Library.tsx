// The games: a grid of covers in the sidebar, or the whole page when expanded. Each cover has the same
// layoutId in both, so expanding and collapsing moves every cover from one place to the other.

import { Popover } from "@base-ui/react/popover";
import clsx from "clsx";
import { Check, ListFilter, Maximize2, Minimize2, Search, Settings2, X } from "lucide-react";
import { motion } from "motion/react";
import { useMemo, useRef } from "react";
import type { LibraryGame } from "../api";
import { FILTERS, findGames, optCount, SORTS, type Sort } from "../logic";
import { useStore } from "../store";
import { Dropdown } from "./controls";
import { Mark } from "./Mark";

const initials = (name: string) =>
  name
    .split(/[\s:]+/)
    .filter(Boolean)
    .slice(0, 2)
    .map((w) => w[0])
    .join("")
    .toUpperCase();

export function Tile({ g, current, big }: { g: LibraryGame; current: boolean; big?: boolean }) {
  const select = useStore((s) => s.select);
  const setExpanded = useStore((s) => s.setExpanded);
  const expanded = useStore((s) => s.expanded);
  return (
    <button
      type="button"
      className="tile"
      aria-current={current}
      title={g.name}
      onClick={() => {
        void select(g.key);
        if (expanded) setExpanded(false);
      }}
    >
      <span className="lift">
        <motion.span layoutId={`cover-${g.key}`} className="cov" transition={{ type: "spring", duration: 0.45, bounce: 0.12 }}>
          {g.art.capsule ? <img src={g.art.capsule} alt="" loading="lazy" decoding="async" draggable={false} /> : <span className="fb">{initials(g.name)}</span>}
          {(g.update || g.suggestions > 0) && <span className="dot" title={g.update ? "Profile update" : `${g.suggestions} suggestions`} />}
        </motion.span>
      </span>
      <motion.span className="nm" initial={{ opacity: 0 }} animate={{ opacity: 1 }} transition={{ delay: big ? 0.18 : 0.12, duration: 0.2 }}>
        {g.name}
      </motion.span>
    </button>
  );
}

function useFound() {
  const library = useStore((s) => s.library);
  const q = useStore((s) => s.q);
  const filters = useStore((s) => s.filters);
  const sort = useStore((s) => s.sort);
  return useMemo(() => findGames(library, q, filters, sort), [library, q, filters, sort]);
}

const count = (n: number, all: number) => (n === all ? `${all} games` : `${n} of ${all} games`);

function ActiveChips() {
  const filters = useStore((s) => s.filters);
  const toggle = useStore((s) => s.toggleFilter);
  const clear = useStore((s) => s.clearFilters);
  const chips = FILTERS.flatMap((f) => (filters[f.id] ?? []).map((v) => ({ f, v, label: f.opts.find((o) => o[0] === v)?.[1] ?? v })));
  if (!chips.length) return null;
  return (
    <div className="chips">
      {chips.map(({ f, v, label }) => (
        <button type="button" key={`${f.id}:${v}`} className="achip" onClick={() => toggle(f.id, v)} aria-label={`Remove the filter ${label}`}>
          {label}
          <X size={12} />
        </button>
      ))}
      <button type="button" className="link" onClick={clear}>
        Clear
      </button>
    </div>
  );
}

function Empty() {
  const setQ = useStore((s) => s.setQ);
  const clear = useStore((s) => s.clearFilters);
  return (
    <div className="empty">
      No games match.
      <button type="button" className="btn" onClick={() => (setQ(""), clear())}>
        Clear the search and filters
      </button>
    </div>
  );
}

function SearchBox({ withFilters }: { withFilters: boolean }) {
  const q = useStore((s) => s.q);
  const setQ = useStore((s) => s.setQ);
  const library = useStore((s) => s.library);
  const filters = useStore((s) => s.filters);
  const toggle = useStore((s) => s.toggleFilter);
  const clear = useStore((s) => s.clearFilters);
  const found = useFound();
  const n = Object.values(filters).reduce((a, v) => a + v.length, 0);
  return (
    <div className="search">
      <Search size={14} />
      <input
        id="search"
        className={clsx("inp", withFilters && "with-btn")}
        value={q}
        placeholder={`Search ${library.length} games`}
        aria-label="Search games"
        spellCheck={false}
        autoComplete="off"
        onChange={(e) => setQ(e.target.value)}
        onKeyDown={(e) => e.key === "Escape" && q && (e.stopPropagation(), setQ(""))}
      />
      {withFilters && (
        <Popover.Root>
          <Popover.Trigger className="f-btn" aria-label="Filters" title="Filters">
            <ListFilter size={14} />
            {n > 0 && <span className="badge">{n}</span>}
          </Popover.Trigger>
          <Popover.Portal>
            <Popover.Positioner sideOffset={8} align="end">
              <Popover.Popup className="f-pop">
                {FILTERS.map((f) => (
                  <div key={f.id}>
                    <h5>{f.title}</h5>
                    <div className="f-opts">
                      {f.opts.map(([v, l]) => {
                        const c = optCount(library, f, v);
                        if (!c && !(filters[f.id] ?? []).includes(v)) return null;
                        return (
                          <button type="button" key={v} className="fchip" aria-pressed={(filters[f.id] ?? []).includes(v)} onClick={() => toggle(f.id, v)}>
                            {l}
                            <span className="n">{c}</span>
                          </button>
                        );
                      })}
                    </div>
                  </div>
                ))}
                <div className="f-foot">
                  <span>{count(found.length, library.length)}</span>
                  {n > 0 && (
                    <button type="button" className="link" onClick={clear}>
                      Clear filters
                    </button>
                  )}
                </div>
              </Popover.Popup>
            </Popover.Positioner>
          </Popover.Portal>
        </Popover.Root>
      )}
    </div>
  );
}

function FilterList() {
  const library = useStore((s) => s.library);
  const filters = useStore((s) => s.filters);
  const toggle = useStore((s) => s.toggleFilter);
  const sort = useStore((s) => s.sort);
  const setSort = useStore((s) => s.setSort);
  return (
    <motion.div className="side-filters" initial={{ opacity: 0, y: 4 }} animate={{ opacity: 1, y: 0 }} transition={{ delay: 0.1, duration: 0.25 }}>
      <h5>Sort by</h5>
      {SORTS.map(([v, l]) => (
        <button type="button" key={v} className="f-row" role="radio" aria-checked={sort === v} onClick={() => setSort(v)}>
          <span className="cb" />
          {l}
        </button>
      ))}
      {FILTERS.map((f) => (
        <div key={f.id}>
          <h5>{f.title}</h5>
          {f.opts.map(([v, l]) => {
            const c = optCount(library, f, v);
            if (!c && !(filters[f.id] ?? []).includes(v)) return null;
            return (
              <button type="button" key={v} className="f-row" role="checkbox" aria-checked={(filters[f.id] ?? []).includes(v)} onClick={() => toggle(f.id, v)}>
                <span className="cb">
                  <Check size={11} strokeWidth={3} />
                </span>
                {l}
                <span className="n">{c}</span>
              </button>
            );
          })}
        </div>
      ))}
    </motion.div>
  );
}

export function Sidebar() {
  const expanded = useStore((s) => s.expanded);
  const setExpanded = useStore((s) => s.setExpanded);
  const library = useStore((s) => s.library);
  const selected = useStore((s) => s.selected);
  const sort = useStore((s) => s.sort);
  const setSort = useStore((s) => s.setSort);
  const app = useStore((s) => s.app);
  const appUpdate = useStore((s) => s.appUpdate);
  const open = useStore((s) => s.open);
  const setSide = useStore((s) => s.setSide);
  const found = useFound();
  const handle = useRef<HTMLDivElement>(null);

  const drag = (e: React.PointerEvent) => {
    const el = handle.current;
    if (!el) return;
    e.preventDefault();
    const left = el.parentElement!.getBoundingClientRect().left;
    const root = document.querySelector<HTMLElement>(".app");
    el.setPointerCapture(e.pointerId);
    el.classList.add("drag");
    let w = 0;
    const move = (ev: PointerEvent) => {
      w = Math.round(Math.min(640, Math.max(232, ev.clientX - left)));
      root?.style.setProperty("--side", `${w}px`);
    };
    const up = () => {
      el.removeEventListener("pointermove", move);
      el.classList.remove("drag");
      if (w) setSide(w);
    };
    el.addEventListener("pointermove", move);
    el.addEventListener("pointerup", up, { once: true });
  };

  return (
    <aside className="side">
      <div className="brand">
        <Mark />
        <span className="grow">GSet</span>
        <button
          type="button"
          className="btn ghost icon"
          aria-expanded={expanded}
          aria-label={expanded ? "Back to the settings" : "Show the whole library"}
          title={expanded ? "Back to the settings (Esc)" : "Show the whole library"}
          onClick={() => setExpanded(!expanded)}
        >
          <motion.span key={String(expanded)} initial={{ opacity: 0, scale: 0.6, rotate: -90 }} animate={{ opacity: 1, scale: 1, rotate: 0 }} transition={{ duration: 0.25 }} style={{ display: "grid" }}>
            {expanded ? <Minimize2 size={15} /> : <Maximize2 size={15} />}
          </motion.span>
        </button>
      </div>
      <div className="find">
        <SearchBox withFilters={!expanded} />
        {!expanded && <ActiveChips />}
        {!expanded && (
          <div className="f-line">
            <span>{count(found.length, library.length)}</span>
            <Dropdown small value={sort} options={SORTS.map(([value, label]) => ({ value, label }))} label="Sort games" onChange={(v) => setSort(v as Sort)} />
          </div>
        )}
      </div>
      {expanded ? (
        <FilterList />
      ) : (
        <motion.div className="grid-side" layoutScroll>
          {found.length ? found.map((g) => <Tile key={g.key} g={g} current={g.key === selected} />) : <Empty />}
        </motion.div>
      )}
      <div className="foot">
        {appUpdate ? (
          <button type="button" className="link ver" style={{ textAlign: "left", paddingLeft: 0 }} onClick={() => open("settings")}>
            <b>GSet {appUpdate.version}</b> is available
          </button>
        ) : (
          <span className="ver">GSet {app?.version}</span>
        )}
        <button type="button" className="btn ghost icon" aria-label="Settings" title="Settings" onClick={() => open("settings")}>
          <Settings2 size={16} />
        </button>
      </div>
      {!expanded && <div className="handle" ref={handle} onPointerDown={drag} onDoubleClick={() => (setSide(344), document.querySelector<HTMLElement>(".app")?.style.setProperty("--side", "344px"))} title="Drag to resize, double-click to reset" />}
    </aside>
  );
}

export function LibraryPage() {
  const library = useStore((s) => s.library);
  const selected = useStore((s) => s.selected);
  const found = useFound();
  return (
    <motion.div className="lib" layoutScroll>
      <motion.div className="lib-head" initial={{ opacity: 0, y: 4 }} animate={{ opacity: 1, y: 0 }} transition={{ delay: 0.1, duration: 0.25 }}>
        <h1>Library</h1>
        <span>{count(found.length, library.length)}</span>
        <ActiveChips />
      </motion.div>
      <div className="grid-lib">{found.length ? found.map((g) => <Tile key={g.key} g={g} current={g.key === selected} big />) : <Empty />}</div>
    </motion.div>
  );
}
