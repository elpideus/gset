// The editors a setting can have. Each commits its value through onChange; text and numbers commit
// on Enter or when leaving the field, so typing doesn't save half a word.

import { Select } from "@base-ui/react/select";
import { Check, ChevronDown } from "lucide-react";
import { useEffect, useState } from "react";
import type { Screen, SettingDef, Value } from "../api";
import { same } from "../logic";

export function Switch(p: { on: boolean; label: string; disabled?: boolean; onChange: (b: boolean) => void }) {
  return (
    <button type="button" className="sw" role="switch" aria-checked={p.on} aria-label={p.label} disabled={p.disabled} onClick={() => p.onChange(!p.on)} />
  );
}

export function Segmented<T extends Value>(p: {
  value: T;
  options: { value: T; label: string }[];
  label: string;
  disabled?: boolean;
  small?: boolean;
  onChange: (v: T) => void;
}) {
  return (
    <div className={p.small ? "seg sm" : "seg"} role="group" aria-label={p.label}>
      {p.options.map((o) => (
        <button type="button" key={String(o.value)} aria-pressed={same(o.value, p.value)} disabled={p.disabled} onClick={() => p.onChange(o.value)}>
          {o.label}
        </button>
      ))}
    </div>
  );
}

const OTHER = "\u0000other";

/** A dropdown. With `other`, a last item lets you type a value of your own. */
export function Dropdown(p: {
  value: Value;
  options: { value: Value; label: string }[];
  label: string;
  disabled?: boolean;
  small?: boolean;
  other?: string;
  onChange: (v: Value) => void;
  onOther?: () => void;
}) {
  const known = p.options.find((o) => same(o.value, p.value));
  const items = [...p.options];
  if (!known) items.push({ value: p.value, label: String(p.value) });
  const all = p.other ? [...items, { value: OTHER as Value, label: p.other }] : items;
  // base-ui compares values by identity; the items' positions keep numbers and strings apart
  const current = String(Math.max(0, all.findIndex((o) => same(o.value, p.value))));
  return (
    <Select.Root
      items={all.map((o, i) => ({ value: String(i), label: o.label }))}
      value={current}
      disabled={p.disabled}
      onValueChange={(v) => {
        const picked = v === null || v === undefined ? undefined : all[Number(v)];
        if (!picked) return;
        if (picked.value === OTHER) p.onOther?.();
        else p.onChange(picked.value);
      }}
    >
      <Select.Trigger className={p.small ? "sel sm" : "sel"} aria-label={p.label}>
        <Select.Value />
        <Select.Icon>
          <ChevronDown size={14} />
        </Select.Icon>
      </Select.Trigger>
      <Select.Portal>
        <Select.Positioner sideOffset={4} alignItemWithTrigger={false} align="end">
          <Select.Popup className="menu">
            {all.map((o, i) => (
              <Select.Item key={i} value={String(i)} className="item">
                <span className="tick">
                  <Select.ItemIndicator>
                    <Check size={14} />
                  </Select.ItemIndicator>
                </span>
                <Select.ItemText>{o.label}</Select.ItemText>
              </Select.Item>
            ))}
          </Select.Popup>
        </Select.Positioner>
      </Select.Portal>
    </Select.Root>
  );
}

/** Text that's saved on Enter or when the field loses focus; Escape puts the old text back. */
export function TextField(p: { value: string; label: string; mono?: boolean; placeholder?: string | null; disabled?: boolean; width?: number; onChange: (s: string) => void }) {
  const [text, setText] = useState(p.value);
  useEffect(() => setText(p.value), [p.value]);
  const commit = () => text !== p.value && p.onChange(text.trim());
  return (
    <input
      className={p.mono ? "inp mono" : "inp"}
      style={p.width ? { width: p.width } : undefined}
      aria-label={p.label}
      value={text}
      placeholder={p.placeholder ?? undefined}
      disabled={p.disabled}
      spellCheck={false}
      autoComplete="off"
      onChange={(e) => setText(e.target.value)}
      onBlur={commit}
      onKeyDown={(e) => {
        if (e.key === "Enter") (e.target as HTMLInputElement).blur();
        if (e.key === "Escape") {
          setText(p.value);
          (e.target as HTMLInputElement).blur();
        }
      }}
    />
  );
}

/** A number with its unit beside it (not inside: typing never fights a suffix). */
export function NumberField(p: { value: number; label: string; min?: number | null; max?: number | null; step?: number | null; unit?: string | null; disabled?: boolean; onChange: (n: number) => void }) {
  const [text, setText] = useState(String(p.value));
  useEffect(() => setText(String(p.value)), [p.value]);
  const n = Number(text.replace(",", "."));
  const ok = text.trim() !== "" && Number.isFinite(n) && (p.min == null || n >= p.min) && (p.max == null || n <= p.max);
  const commit = () => {
    if (ok && n !== p.value) p.onChange(n);
    else if (!ok) setText(String(p.value));
  };
  return (
    <span style={{ display: "inline-flex", alignItems: "center", gap: 8 }}>
      <input
        className={ok ? "inp num" : "inp num bad"}
        inputMode="decimal"
        aria-label={p.label}
        value={text}
        disabled={p.disabled}
        onChange={(e) => setText(e.target.value)}
        onBlur={commit}
        onKeyDown={(e) => {
          if (e.key === "Enter") (e.target as HTMLInputElement).blur();
          if (e.key === "Escape") {
            setText(String(p.value));
            (e.target as HTMLInputElement).blur();
          }
          if ((e.key === "ArrowUp" || e.key === "ArrowDown") && ok) {
            e.preventDefault();
            const step = p.step ?? 1;
            const next = Math.min(p.max ?? Infinity, Math.max(p.min ?? -Infinity, n + (e.key === "ArrowUp" ? step : -step)));
            setText(String(Math.round(next * 1000) / 1000));
          }
        }}
      />
      {p.unit && <span className="unit">{p.unit}</span>}
    </span>
  );
}

const SIZES = ["3840x2160", "3440x1440", "2560x1440", "2560x1080", "1920x1200", "1920x1080", "1600x900", "1280x800", "1280x720"];

/** A size: the screen's own (or off), the screens that are on, the usual ones, or one you type. */
export function SizeField(p: { value: string; label: string; screens: Screen[]; offLabel: string; offValue: string; disabled?: boolean; onChange: (s: string) => void }) {
  const [typing, setTyping] = useState(false);
  const fromScreens = p.screens.map((s) => `${s.width}x${s.height}`);
  const sizes = [...new Set([...fromScreens, ...SIZES])];
  const options = [{ value: p.offValue, label: p.offLabel }, ...sizes.map((s) => ({ value: s, label: s.replace("x", " × ") + (fromScreens.includes(s) ? "  · this screen" : "") }))];
  if (typing) {
    return (
      <TextField
        value={p.value === p.offValue ? "" : p.value}
        label={p.label}
        mono
        width={130}
        placeholder="1920x1080"
        onChange={(s) => {
          setTyping(false);
          if (/^\d{3,5}\s*[x×]\s*\d{3,5}$/.test(s)) p.onChange(s.replace(/\s|×/g, (c) => (c === "×" ? "x" : "")));
        }}
      />
    );
  }
  return <Dropdown value={p.value} options={options} label={p.label} disabled={p.disabled} other="Other size…" onChange={(v) => p.onChange(String(v))} onOther={() => setTyping(true)} />;
}

/** The right editor for a setting. */
export function Control(p: { def: SettingDef; value: Value; disabled: boolean; screens: Screen[]; onChange: (v: Value) => void }) {
  const { def, value } = p;
  const [other, setOther] = useState(false);
  switch (def.kind) {
    case "toggle":
      return <Switch on={value === true} label={def.label} disabled={p.disabled} onChange={p.onChange} />;
    case "number":
      return <NumberField value={Number(value)} label={def.label} min={def.min} max={def.max} step={def.step} unit={def.unit} disabled={p.disabled} onChange={p.onChange} />;
    case "text":
      return <TextField value={String(value ?? "")} label={def.label} mono={def.mono} placeholder={def.placeholder} disabled={p.disabled} onChange={p.onChange} />;
    case "resolution":
      return def.key === "game_resolution" ? (
        <SizeField value={String(value)} label={def.label} screens={p.screens} offLabel="The game’s own" offValue="" disabled={p.disabled} onChange={p.onChange} />
      ) : (
        <SizeField value={String(value)} label={def.label} screens={p.screens} offLabel="The screen’s" offValue="native" disabled={p.disabled} onChange={p.onChange} />
      );
    case "choice": {
      const short = def.choices.length <= 5 && def.choices.every((c) => c.label.length <= 10) && def.choices.some((c) => same(c.value, value));
      if (short && !other && def.key !== "frame_cap" && def.key !== "refresh_rate" && def.key !== "dlss_preset") {
        return <Segmented value={value} options={def.choices} label={def.label} disabled={p.disabled} onChange={p.onChange} />;
      }
      if (other) {
        const numeric = typeof def.default === "number";
        return numeric ? (
          <NumberField value={Number(value)} label={def.label} min={0} onChange={(n) => (setOther(false), p.onChange(n))} />
        ) : (
          <TextField value={String(value)} label={def.label} mono={def.mono} onChange={(s) => (setOther(false), p.onChange(s))} />
        );
      }
      return (
        <Dropdown
          value={value}
          options={def.choices}
          label={def.label}
          disabled={p.disabled}
          other={def.editable ? "Other…" : undefined}
          onChange={p.onChange}
          onOther={() => setOther(true)}
        />
      );
    }
  }
}
