import { useMemo, type ReactNode } from "react";

const SEGMENTS = 96;

/**
 * Segmented progress ring. Each of the 96 segments stands for 1/96 of the
 * file: white once that share is verified on disk, red while it is arriving,
 * dim while still to fetch. Counts are by amount, not position, because
 * chunks arrive in parallel.
 */
export function ChunkRing({
  verified,
  done,
  size = 236,
  state = "active",
  children,
  label,
}: {
  verified: number;
  done: number;
  size?: number;
  state?: "active" | "paused" | "attention" | "idle";
  children?: ReactNode;
  label: string;
}) {
  const c = size / 2;
  const r = size * 0.36;
  const w = Math.max(6, size * 0.05);
  const inner = r - w * 1.55;
  const outer = r + w * 1.65;

  const segs = useMemo(() => {
    const step = (Math.PI * 2) / SEGMENTS;
    const gap = step * 0.27;
    const pt = (a: number) => `${(c + r * Math.sin(a)).toFixed(2)} ${(c - r * Math.cos(a)).toFixed(2)}`;
    return Array.from({ length: SEGMENTS }, (_, i) => `M${pt(i * step + gap / 2)}A${r} ${r} 0 0 1 ${pt((i + 1) * step - gap / 2)}`);
  }, [c, r]);

  const v = clamp01(verified);
  const d = Math.max(v, clamp01(done));
  const lit = Math.floor(v * SEGMENTS + 1e-9);
  let arriving = Math.floor(d * SEGMENTS + 1e-9) - lit;
  if (state === "active" && d > v && arriving < 1 && lit < SEGMENTS) arriving = 1;

  const ticks = 60;
  const tickLen = (2 * Math.PI * inner) / ticks;

  return (
    <div className={`ring ring-${state}`} style={{ width: size, height: size }} role="img" aria-label={label}>
      <svg width={size} height={size} viewBox={`0 0 ${size} ${size}`} aria-hidden="true">
        <circle cx={c} cy={c} r={outer} className="ring-outer" />
        <circle cx={c} cy={c} r={inner} className="ring-ticks" strokeWidth={w * 0.42} strokeDasharray={`0.8 ${(tickLen - 0.8).toFixed(3)}`} />
        {segs.map((p, i) => (
          <path key={i} d={p} strokeWidth={w} className={i < lit ? "seg-v" : i < lit + arriving ? "seg-a" : "seg-o"} />
        ))}
        <path d={`M${c - 4} ${c - outer - 7}h8l-4 5z`} className="ring-cursor" />
        <path d={`M${c} ${c + outer - 5}v10M${c - outer - 5} ${c}h10M${c + outer - 5} ${c}h10`} className="ring-cross" />
      </svg>
      <div className="ring-center">{children}</div>
    </div>
  );
}

/** Small route glyph: solid line for direct, dashed amber for relay, hollow for none. */
export function RouteGlyph({ kind }: { kind: string }) {
  const cls = kind === "direct" ? "rg-direct" : kind === "relay" ? "rg-relay" : "rg-none";
  return (
    <svg className={`rg ${cls}`} width="22" height="6" viewBox="0 0 22 6" aria-hidden="true">
      <line x1="0" y1="3" x2="22" y2="3" />
    </svg>
  );
}

/**
 * Node, line, node: this device and the peer, joined the way data actually
 * flows. Packets move toward the receiver.
 */
export function LinkLine({
  local,
  peer,
  kind,
  caption,
  incoming = true,
  moving = false,
}: {
  local: string;
  peer: string;
  kind: string;
  caption: string;
  incoming?: boolean;
  moving?: boolean;
}) {
  const k = kind === "direct" ? "direct" : kind === "relay" ? "relay" : "none";
  return (
    <div className={`link-line link-${k} ${incoming ? "in" : "out"} ${moving && k !== "none" ? "moving" : ""}`}>
      <span className="ll-cap">{caption}</span>
      <span className="ll-track" aria-hidden="true">
        <i className="ll-node" />
        <span className="ll-wire">
          <b />
          <b />
          <b />
          <b />
          {k === "relay" && <i className="ll-relay" />}
        </span>
        <i className="ll-node hollow" />
      </span>
      <span className="ll-ends">
        <span className="truncate">{local}</span>
        <span className="truncate">{peer}</span>
      </span>
    </div>
  );
}

/**
 * Wireframe globe for the network panel. Decorative: it stands for "online"
 * and never shows where any device is.
 */
export function Globe({ size = 164, online }: { size?: number; online: boolean }) {
  const c = size / 2;
  const r = size * 0.34;
  const lat = [-0.66, -0.33, 0.33, 0.66];
  return (
    <svg className={`globe ${online ? "on" : "off"}`} width={size} height={size} viewBox={`0 0 ${size} ${size}`} aria-hidden="true">
      <circle cx={c} cy={c} r={r * 1.22} className="g-ticks" strokeDasharray={`0.8 ${((2 * Math.PI * r * 1.22) / 72 - 0.8).toFixed(3)}`} />
      <circle cx={c} cy={c} r={r} className="g-rim" />
      {[0.78, 0.5, 0.17].map((k) => (
        <ellipse key={k} cx={c} cy={c} rx={r * k} ry={r} className="g-line" />
      ))}
      {lat.map((y) => (
        <ellipse key={y} cx={c} cy={c + y * r} rx={r * Math.sqrt(1 - y * y)} ry={r * 0.08} className="g-line" />
      ))}
      <ellipse cx={c} cy={c} rx={r} ry={r * 0.12} className="g-eq" />
      <g className="g-orbit-g">
        <ellipse cx={c} cy={c} rx={r * 1.42} ry={r * 0.38} transform={`rotate(-18 ${c} ${c})`} className="g-orbit" />
      </g>
    </svg>
  );
}

function clamp01(x: number) {
  return Number.isFinite(x) ? Math.max(0, Math.min(1, x)) : 0;
}
