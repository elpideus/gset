// GSet's settings, the review of a profile update, and making or changing a preset.

import { Dialog } from "@base-ui/react/dialog";
import { Copy, Plus } from "lucide-react";
import { useEffect, useState } from "react";
import { toast } from "sonner";
import type { Change, Value } from "../api";
import { fmtSize, label } from "../logic";
import { useStore } from "../store";
import { Dropdown, Segmented, Switch } from "./controls";
import { Ratio } from "./Mark";

function Shell(p: { open: boolean; wide?: boolean; title: string; sub?: string; children: React.ReactNode }) {
  const open = useStore((s) => s.open);
  return (
    <Dialog.Root open={p.open} onOpenChange={(o) => !o && open(null)}>
      <Dialog.Portal>
        <Dialog.Backdrop className="scrim" />
        <Dialog.Popup className={p.wide ? "dlg wide" : "dlg"}>
          <Dialog.Title render={<h2 />}>{p.title}</Dialog.Title>
          {p.sub && <Dialog.Description className="dlg-sub">{p.sub}</Dialog.Description>}
          {p.children}
        </Dialog.Popup>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

export function SettingsDialog() {
  const dialog = useStore((s) => s.dialog);
  const app = useStore((s) => s.app);
  const set = useStore((s) => s.setAppOption);
  const open = useStore((s) => s.open);
  const update = useStore((s) => s.appUpdate);
  const updating = useStore((s) => s.updating);
  const install = useStore((s) => s.installUpdate);
  if (!app) return null;
  const line = `${app.launcher ?? "gset"} %command%`;
  return (
    <Shell open={dialog === "settings"} wide title="Settings">
      <section>
        <h4>GSet</h4>
        <div className="srow">
          <div className="lab">
            <b>Version {app.version}</b>
            <small>
              {update
                ? `Version ${update.version} is available.`
                : app.check_updates
                  ? "Up to date."
                  : "Checking for updates is off."}
            </small>
            {update?.notes && (
              <small className="selectable" style={{ whiteSpace: "pre-line", marginTop: 6 }}>
                {update.notes}
              </small>
            )}
          </div>
          {update && (
            <button type="button" className="btn primary" disabled={updating !== null} onClick={() => void install()}>
              {updating === null ? "Update and restart" : updating >= 1 ? "Restarting…" : `Downloading ${Math.round(updating * 100)}%`}
            </button>
          )}
        </div>
        <div className="srow">
          <div className="lab">
            <b>Check for updates</b>
            <small>When this window opens. Never while a game starts.</small>
          </div>
          <Switch on={app.check_updates} label="Check for updates" onChange={(b) => (void set("check_updates", b), b && void useStore.getState().checkUpdate())} />
        </div>
      </section>
      <section>
        <h4>Game profiles</h4>
        <div className="srow">
          <div className="lab">
            <b>Update profiles automatically</b>
            <small>A profile describes a game: its settings, texts, choices and suggested values. Updates never change your settings. Off: GSet shows an Update button first.</small>
          </div>
          <Switch on={app.auto_profile_updates} label="Update profiles automatically" onChange={(b) => void set("auto_profile_updates", b)} />
        </div>
        <div className="srow">
          <div className="lab">
            <b>New games start with</b>
            <small>Only the first time GSet meets a game. After that, the values are yours.</small>
          </div>
          <Segmented
            value={app.new_games}
            label="New games start with"
            options={[
              { value: "suggested", label: "Suggested values" },
              { value: "defaults", label: "GSet’s defaults" },
            ]}
            onChange={(v) => void set("new_games", v as Value)}
          />
        </div>
        <div className="srow">
          <div className="lab">
            <b>{app.profiles} profiles</b>
            <small>From github.com/elpideus/gset-profiles, signed, checked when this window opens.</small>
          </div>
          <button type="button" className="btn" onClick={() => void useStore.getState().refreshProfiles(false)}>
            Check now
          </button>
        </div>
      </section>
      <section>
        <h4>Presets</h4>
        {app.presets.map((p) => (
          <div className="srow" key={p.id}>
            <div className="lab">
              <b>
                <Ratio ratio={p.ratio} />
                {p.name}
                {p.ratio ? ` ${p.ratio}` : ""}
              </b>
              <small>{p.screen ? `Used when the screen is ${fmtSize(p.screen)}` : "Used only when pinned"}</small>
            </div>
            <button type="button" className="btn ghost" onClick={() => open("preset", p.id)}>
              Edit
            </button>
          </div>
        ))}
        <div className="srow">
          <div className="lab">
            <small>{app.presets.length ? "A preset changes some settings while its screen is on: one for your monitor, one for streaming." : "No presets yet. A preset changes some settings while its screen is on: one for your monitor, one for streaming."}</small>
          </div>
          <button type="button" className="btn" onClick={() => open("preset", null)}>
            <Plus size={14} />
            New preset
          </button>
        </div>
      </section>
      <section>
        <h4>Launcher</h4>
        <div className="srow">
          <div className="lab">
            <b>Steam launch option</b>
            <small className="selectable" style={{ fontFamily: "var(--mono)" }}>
              {line}
            </small>
          </div>
          <button type="button" className="btn" onClick={() => void navigator.clipboard.writeText(line).then(() => toast("Launch option copied."))}>
            <Copy size={14} />
            Copy
          </button>
        </div>
        <div className="srow">
          <div className="lab">
            <b>Your settings</b>
            <small className="selectable">{app.config_dir}: plain files you can also edit; GSet keeps your comments.</small>
          </div>
        </div>
        {app.problems.length > 0 && (
          <div className="problems">
            {app.problems.map((p) => (
              <div key={p}>{p}</div>
            ))}
          </div>
        )}
      </section>
      <div className="dlg-act">
        <Dialog.Close className="btn">Done</Dialog.Close>
      </div>
    </Shell>
  );
}

function ChangeItem({ c }: { c: Change }) {
  const g = useStore((s) => s.game)!;
  const def = "key" in c ? g.settings.find((d) => d.key === c.key) : undefined;
  const yours = "key" in c ? g.views.base.values[c.key] : undefined;
  switch (c.type) {
    case "new_setting":
      return (
        <li>
          <div className="ch-k">New setting</div>
          <div className="ch-v">{c.label}</div>
          <div className="ch-d">{c.help ? `${c.help} ` : ""}It stays off until you turn it on.</div>
        </li>
      );
    case "removed_setting": {
      const set = def && yours !== undefined && String(yours) !== String(def.default);
      return (
        <li>
          <div className="ch-k">Removed setting</div>
          <div className="ch-v">{c.label}</div>
          <div className="ch-d">
            {c.why ? `${c.why} ` : ""}
            {set ? `You have it ${label(def, yours)}: it stays applied until you remove it.` : "You never changed it, so nothing changes."}
          </div>
        </li>
      );
    }
    case "suggestion":
      return (
        <li>
          <div className="ch-k">{c.before === null || c.before === undefined ? "New suggestion" : "Changed suggestion"}</div>
          <div className="ch-v">
            {c.label}: {label(def, c.value)}
          </div>
          <div className="ch-d">
            {c.why ? `${c.why} ` : ""}
            {String(yours) === String(c.value) ? "You already use it." : `Yours stays ${label(def, yours)}; the suggestion appears next to the setting.`}
          </div>
        </li>
      );
    case "dropped_suggestion":
      return (
        <li>
          <div className="ch-k">Suggestion dropped</div>
          <div className="ch-v">{c.label}</div>
          <div className="ch-d">Your value stays.</div>
        </li>
      );
    case "new_choice":
      return (
        <li>
          <div className="ch-k">New choice</div>
          <div className="ch-v">
            {c.label}: {c.choice}
          </div>
          <div className="ch-d">Added to the list. Your choice stays as it is.</div>
        </li>
      );
    case "notes":
      return (
        <li>
          <div className="ch-k">Updated notes</div>
          {c.notes.map((n) => (
            <div className="ch-d" key={n} style={{ color: "var(--ink)" }}>
              “{n}”
            </div>
          ))}
        </li>
      );
    case "name":
      return (
        <li>
          <div className="ch-k">New name</div>
          <div className="ch-v">{c.name}</div>
        </li>
      );
  }
}

export function ReviewDialog() {
  const dialog = useStore((s) => s.dialog);
  const g = useStore((s) => s.game);
  const apply = useStore((s) => s.applyUpdate);
  const u = g?.update;
  return (
    <Shell
      open={dialog === "review" && !!u}
      title={u ? `${g!.info.name.split(":")[0]} profile v${u.to}` : ""}
      sub="A profile update changes the game’s description and suggestions only: none of your settings change."
    >
      <ul className="changes">{u?.changes.map((c, i) => <ChangeItem key={i} c={c} />)}</ul>
      <div className="dlg-act">
        <Dialog.Close className="btn ghost">Not now</Dialog.Close>
        <button type="button" className="btn primary" onClick={() => void apply()}>
          Update profile
        </button>
      </div>
    </Shell>
  );
}

export function PresetDialog() {
  const dialog = useStore((s) => s.dialog);
  const id = useStore((s) => s.editPreset);
  const app = useStore((s) => s.app);
  const save = useStore((s) => s.savePreset);
  const remove = useStore((s) => s.deletePreset);
  const editing = app?.presets.find((p) => p.id === id) ?? null;
  const [name, setName] = useState("");
  const [screen, setScreen] = useState("");
  const [typing, setTyping] = useState(false);
  useEffect(() => {
    if (dialog === "preset") {
      setName(editing?.name ?? "");
      setScreen(editing?.screen ?? "");
      setTyping(false);
    }
  }, [dialog, editing]);
  if (!app) return null;
  const here = app.screens.map((s) => `${s.width}x${s.height}`);
  const sizes = [...new Set([...here, "3440x1440", "2560x1440", "2560x1080", "1920x1080", "1280x800"])];
  const options = [{ value: "", label: "No screen: only when pinned" }, ...sizes.map((s) => ({ value: s, label: fmtSize(s) + (here.includes(s) ? "  · on now" : "") }))];
  const ok = name.trim().length > 0 && (!screen || /^\d{3,5}x\d{3,5}$/.test(screen));
  return (
    <Shell
      open={dialog === "preset"}
      title={editing ? `Preset ${editing.name}` : "New preset"}
      sub="A preset changes some settings while a screen in its mode is on, for every game. Each game can change it further on its own."
    >
      <label className="field">
        <span>Name</span>
        <input className="inp" value={name} autoFocus placeholder="Home, Work, Streaming…" onChange={(e) => setName(e.target.value)} onKeyDown={(e) => e.key === "Enter" && ok && void save(id, name, screen || null)} />
      </label>
      <div className="field">
        <span>Used when the screen is</span>
        {typing ? (
          <input className="inp" value={screen} placeholder="1920x1080" onChange={(e) => setScreen(e.target.value.replace(/\s|×/g, (c) => (c === "×" ? "x" : "")))} />
        ) : (
          <Dropdown value={screen} options={options} label="Screen" other="Another size…" onChange={(v) => setScreen(String(v))} onOther={() => setTyping(true)} />
        )}
      </div>
      <div className="dlg-act">
        {editing && (
          <button type="button" className="btn ghost danger" onClick={() => void remove(editing.id)}>
            Delete
          </button>
        )}
        <span className="grow" />
        <Dialog.Close className="btn ghost">Cancel</Dialog.Close>
        <button type="button" className="btn primary" disabled={!ok} onClick={() => void save(id, name, screen || null)}>
          {editing ? "Save" : "Make it"}
        </button>
      </div>
    </Shell>
  );
}
