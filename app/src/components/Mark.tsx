/** GSet's mark: a screen with a setting's slider; the knob takes the accent. */
export function Mark({ size = 18 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 20 20" fill="none" aria-hidden="true">
      <rect x="1.75" y="4" width="16.5" height="12" rx="3" stroke="currentColor" strokeWidth="1.6" />
      <path d="M5.5 10h9" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" opacity=".45" />
      <circle cx="11.5" cy="10" r="2.1" fill="var(--signal)" />
    </svg>
  );
}

/** A screen's shape, for presets: 21:9 is wide, 16:9 less so. */
export function Ratio({ ratio }: { ratio?: string | null }) {
  const [w, h] = (ratio ?? "16:9").split(":").map(Number);
  const r = w && h ? w / h : 16 / 9;
  const width = Math.min(18, 9.4 * r);
  const height = width / r;
  return (
    <svg width="20" height="12" viewBox="0 0 20 12" aria-hidden="true" style={{ flex: "none" }}>
      <rect x={(20 - width) / 2} y={(12 - height) / 2} width={width} height={height} rx="1.6" fill="none" stroke="currentColor" strokeWidth="1.4" />
    </svg>
  );
}
