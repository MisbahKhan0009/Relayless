import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import createGlobe, { type COBEOptions } from "cobe";

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

// ------------------------------------------------------------------ dotted globe (cobe)

/**
 * Colours for cobe, as 0..1 RGB. Dots are pewter-to-white on a black sphere,
 * with a faint signal-red halo; offline drops the halo to grey. Kept in step
 * with --text, --text-3 and --accent in styles.css.
 */
const GLOBE_LOOK = {
  online: { baseColor: [0.34, 0.34, 0.36], glowColor: [0.26, 0.06, 0.07], mapBrightness: 9, diffuse: 2.4, opacity: 0.95 },
  offline: { baseColor: [0.24, 0.24, 0.25], glowColor: [0.1, 0.1, 0.11], mapBrightness: 5, diffuse: 1.8, opacity: 0.7 },
} satisfies Record<string, Partial<COBEOptions>>;

let webgl: boolean | null = null;
function hasWebGL(): boolean {
  if (webgl == null) {
    try {
      const c = document.createElement("canvas");
      webgl = !!(c.getContext("webgl2") || c.getContext("webgl"));
    } catch {
      webgl = false;
    }
  }
  return webgl;
}

/**
 * The public relay regions Relayless can use: iroh's default relay map
 * (`iroh::defaults::prod`, hostnames `<region>-1.relay.n0.iroh.link`).
 * Positions are approximate, at the metro area each region code names, and
 * are only used to place the markers. Update this list together with iroh.
 */
export const RELAY_REGIONS = [
  { code: "use1", name: "US East", location: [38.95, -77.45] },
  { code: "usw1", name: "US West", location: [37.4, -122.0] },
  { code: "euc1", name: "EU Central", location: [50.11, 8.68] },
  { code: "aps1", name: "Asia Pacific", location: [19.08, 72.88] },
] as const satisfies readonly { code: string; name: string; location: [number, number] }[];

/** "https://euc1-1.relay.n0.iroh.link./" -> "euc1". Null for custom relays. */
export function relayRegion(url: string | null | undefined): string | null {
  if (!url) return null;
  try {
    const code = new URL(url).hostname.split(".")[0].replace(/-\d+$/, "");
    return RELAY_REGIONS.some((r) => r.code === code) ? code : null;
  } catch {
    return null;
  }
}

const RED: [number, number, number] = [1, 0.23, 0.28];
const DOT: [number, number, number] = [0.92, 0.92, 0.93];

/** Rotation that puts a longitude in the middle of the globe's face. */
const facing = (lon: number) => (3 * Math.PI) / 2 - (lon * Math.PI) / 180;

/**
 * The Network panel's globe, after cobe's "CDN" showcase: a slowly turning
 * dotted Earth (WebGL) with a labelled dot at each public relay region,
 * the home relay in red. These are the servers that help devices find each
 * other and, when routers block a direct path, forward encrypted packets.
 *
 * Unlike the showcase it draws no arcs: relays don't send data to each
 * other, and Relayless never knows where a peer is, so any line between
 * places would be made up. No device is ever placed on the globe.
 *
 * It only animates while it's on screen, the device is online and the user
 * hasn't asked for reduced motion, at about 30 frames a second, so it costs
 * almost nothing when nobody is looking at it.
 */
export function DotGlobe({ size = 170, online, home }: { size?: number; online: boolean; home: string | null }) {
  const ref = useRef<HTMLCanvasElement>(null);
  const stage = useRef<HTMLDivElement>(null);
  const [ok] = useState(hasWebGL);
  // Starts with the home relay (or Europe) facing you; keeps the rotation
  // across re-creations.
  const phi = useRef(facing(RELAY_REGIONS.find((r) => r.code === home)?.location[1] ?? 8.68));

  useEffect(() => {
    const canvas = ref.current;
    const gl = canvas?.parentElement;
    if (!canvas || !gl || !ok) return;
    const look = online ? GLOBE_LOOK.online : GLOBE_LOOK.offline;
    const globe = createGlobe(canvas, {
      width: size,
      height: size,
      devicePixelRatio: Math.min(2, window.devicePixelRatio || 1),
      phi: phi.current,
      theta: 0.24,
      dark: 1,
      mapSamples: 14000,
      mapBaseBrightness: 0.02,
      markerColor: DOT,
      markerElevation: 0.02,
      markers: RELAY_REGIONS.map((r) => ({
        location: [...r.location] as [number, number],
        size: r.code === home ? 0.045 : 0.032,
        id: `rl-relay-${r.code}`,
        ...(r.code === home && online ? { color: RED } : {}),
      })),
      scale: 1,
      ...look,
    });

    // Label fading. cobe places one anchor per marker (left/top in % of the
    // canvas) and sets --cobe-visible-<id> on :root while it faces us. On a
    // small globe, tags near the rim crowd each other, so each tag also fades
    // as its marker nears the edge of the disc.
    const labels = RELAY_REGIONS.map((r) => {
      const id = `rl-relay-${r.code}`;
      return {
        id,
        anchor: canvas.parentElement?.querySelector<HTMLElement>(`[style*="--cobe-${id}"]`) ?? null,
        label: stage.current?.querySelector<HTMLElement>(`[data-relay="${r.code}"]`) ?? null,
      };
    });
    const root = document.documentElement;
    const fadeLabels = () => {
      const vars = getComputedStyle(root);
      // Nearest the centre first, so a label that would overlap one closer
      // to the middle is the one that gives way.
      const placed = labels
        .filter((l) => l.anchor && l.label)
        .map((l) => {
          const x = parseFloat(l.anchor!.style.left) / 100;
          const y = parseFloat(l.anchor!.style.top) / 100;
          return { l, x, y, d: Math.hypot(x - 0.5, y - 0.5) };
        })
        .sort((a, b) => a.d - b.d);
      const kept: { x: number; y: number }[] = [];
      for (const { l, x, y, d } of placed) {
        if (!l.anchor || !l.label) continue;
        const facingUs = vars.getPropertyValue(`--cobe-visible-${l.id}`).trim() !== "";
        // The disc's rim is at about 0.4 of the canvas from the centre; fade
        // over the last stretch before it.
        const fromRim = Math.min(1, Math.max(0, (0.41 - d) / 0.05));
        // A tag is about 40 x 17 px; keep them apart.
        const clash = kept.some((k) => Math.abs(k.x - x) * size < 42 && Math.abs(k.y - y) * size < 19);
        const o = facingUs && !clash ? fromRim : 0;
        if (o > 0.3) kept.push({ x, y });
        // Placed from the anchor's own coordinates rather than CSS anchor
        // positioning, so labels show in every WebView, not only new ones.
        l.label.style.left = l.anchor.style.left;
        l.label.style.top = l.anchor.style.top;
        l.label.style.opacity = o.toFixed(2);
        l.label.style.filter = o > 0.98 ? "" : `blur(${((1 - o) * 5).toFixed(1)}px)`;
      }
    };
    fadeLabels();

    const reduce = window.matchMedia("(prefers-reduced-motion: reduce)");
    let raf = 0;
    let last = 0;
    let onScreen = true;
    const frame = (t: number) => {
      raf = requestAnimationFrame(frame);
      if (last && t - last < 32) return; // ~30 fps is plenty for a slow spin
      const dt = last ? Math.min(t - last, 100) : 32;
      last = t;
      phi.current += dt * 0.00016; // one turn in about 40 s
      globe.update({ phi: phi.current });
      fadeLabels();
    };
    const stop = () => {
      cancelAnimationFrame(raf);
      raf = 0;
      last = 0;
    };
    const sync = () => {
      const run = online && onScreen && !reduce.matches;
      if (run && !raf) raf = requestAnimationFrame(frame);
      else if (!run && raf) stop();
    };
    const io = new IntersectionObserver(([e]) => {
      onScreen = e.isIntersecting;
      sync();
    });
    io.observe(canvas);
    reduce.addEventListener("change", sync);
    sync();

    return () => {
      stop();
      io.disconnect();
      reduce.removeEventListener("change", sync);
      // Frees the buffers, programs, marker anchors and cobe's <style> tag.
      // The WebGL context stays with the canvas, so a re-run (StrictMode,
      // going on or offline) reuses it.
      globe.destroy();
      // cobe wraps the canvas in a div of its own; undo that so a re-run
      // doesn't nest another one.
      const wrap = canvas.parentElement;
      if (wrap && wrap !== gl) {
        gl.insertBefore(canvas, wrap);
        wrap.remove();
      }
    };
  }, [ok, size, online, home]);

  if (!ok) return <Globe size={size} online={online} />;
  // A HUD frame around the globe: a tick scale, in SVG.
  const box = Math.round(size * 1.12);
  const c = box / 2;
  const tr = size * 0.5;
  return (
    <div className={`dot-globe ${online ? "on" : "off"}`} style={{ width: box, height: box }}>
      <svg className="dg-frame" width={box} height={box} viewBox={`0 0 ${box} ${box}`} aria-hidden="true">
        <circle cx={c} cy={c} r={tr} className="dg-ticks" strokeDasharray={`0.8 ${((2 * Math.PI * tr) / 90 - 0.8).toFixed(3)}`} />
      </svg>
      <div className="dg-stage" ref={stage} style={{ width: size, height: size }}>
        {/* cobe moves the canvas into a wrapper of its own, so React keeps
            nothing else in this div. */}
        <div className="dg-gl">
          <canvas ref={ref} className="dg-canvas" style={{ width: size, height: size }} aria-hidden="true" />
        </div>
        {/* Labels follow cobe's marker anchors and fade out near the rim and
            behind the globe. */}
        {RELAY_REGIONS.map((r) => {
          return (
            <span
              key={r.code}
              data-relay={r.code}
              className={`dg-relay ${r.code === home ? "home" : ""}`}
              // Position, opacity and blur are set per frame by fadeLabels().
              style={{ opacity: 0 }}
              aria-hidden="true"
            >
              <span className="dg-tag">{r.code}</span>
            </span>
          );
        })}
      </div>
      <span className="sr-only">
        Public relay regions: {RELAY_REGIONS.map((r) => `${r.name} (${r.code})${r.code === home ? ", your home relay" : ""}`).join("; ")}.
      </span>
    </div>
  );
}

function clamp01(x: number) {
  return Number.isFinite(x) ? Math.max(0, Math.min(1, x)) : 0;
}
