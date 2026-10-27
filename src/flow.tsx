import { createContext, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { Pause, RefreshCw, Waypoints, Zap } from "lucide-react";
import { speed } from "./format";
import { useStore } from "./store";

/** Validated two-series chart colours (dataviz validator, dark surface). */
export const SERIES = { in: "#16AE4D", out: "#9277F5" } as const;

const WINDOW = 120; // seconds of history the live graph keeps

export interface FlowSample {
  t: number;
  down: number;
  up: number;
}

export type MarkKind = "pause" | "reconnect" | "relay" | "direct";

export interface FlowMark {
  t: number;
  kind: MarkKind;
  text: string;
}

interface Flow {
  samples: FlowSample[];
  marks: FlowMark[];
}

const Ctx = createContext<Flow>({ samples: [], marks: [] });

export function useFlow() {
  return useContext(Ctx);
}

/** Demo history so the preview build shows a populated graph. */
function demoFlow(now: number): Flow {
  const samples: FlowSample[] = [];
  for (let i = WINDOW; i > 0; i--) {
    const t = now - i * 1000;
    const dip = i > 88 && i < 98 ? 0.05 : 1;
    const ramp = i > 78 && i <= 88 ? (88 - i) / 10 : 1;
    const down = (10.4 + 1.6 * Math.sin(i / 7) + 0.7 * Math.sin(i / 2.3)) * 1024 * 1024 * dip * ramp;
    const up = i < 64 ? (3.1 + 0.9 * Math.sin(i / 5) + 0.3 * Math.cos(i / 1.7)) * 1024 * 1024 * Math.min(1, (64 - i) / 6) : 0;
    samples.push({ t, down, up });
  }
  return {
    samples,
    marks: [
      { t: now - 97_000, kind: "reconnect", text: "Connection dropped, resumed from the last saved block" },
      { t: now - 64_000, kind: "direct", text: "Lena's laptop started downloading from you" },
      { t: now - 31_000, kind: "pause", text: "You paused vfx-plates-batch-7.7z" },
    ],
  };
}

/**
 * Samples total receive and send speed once a second and notes the moments
 * that explain the shape of the line: pauses, reconnects and route changes.
 */
export function FlowProvider({ children }: { children: ReactNode }) {
  const { jobs, uploads } = useStore();
  const latest = useRef({ jobs, uploads });
  latest.current = { jobs, uploads };
  const [flow, setFlow] = useState<Flow>(() => (import.meta.env.MODE === "mock" ? demoFlow(Date.now()) : { samples: [], marks: [] }));
  const prev = useRef<Record<string, { status: string; route: string }>>({});

  useEffect(() => {
    const tick = () => {
      const now = Date.now();
      const { jobs, uploads } = latest.current;
      const list = Object.values(jobs);
      const down = list.filter((j) => j.status === "downloading").reduce((a, j) => a + j.speedBps, 0);
      const up = uploads.reduce((a, u) => a + u.bps, 0);
      const marks: FlowMark[] = [];
      const seen: Record<string, { status: string; route: string }> = {};
      for (const j of list) {
        seen[j.id] = { status: j.status, route: j.route };
        const p = prev.current[j.id];
        if (!p) continue;
        if (p.status !== j.status) {
          if (j.status === "paused") marks.push({ t: now, kind: "pause", text: `You paused ${j.displayName}` });
          if (j.status === "reconnecting" && p.status === "downloading")
            marks.push({ t: now, kind: "reconnect", text: `Connection to ${j.peerName} dropped. Progress is saved.` });
        }
        if (j.status === "downloading" && p.route !== j.route && p.route !== "none" && j.route !== "none") {
          marks.push(
            j.route === "relay"
              ? { t: now, kind: "relay", text: `${j.peerName} switched to a relayed path` }
              : { t: now, kind: "direct", text: `${j.peerName} is on a direct path again` },
          );
        }
      }
      prev.current = seen;
      setFlow((f) => {
        const cut = now - WINDOW * 1000;
        const idle = down === 0 && up === 0 && marks.length === 0;
        const recent = f.samples.filter((s) => s.t >= cut);
        // Nothing moving and nothing to show: skip the re-render.
        if (idle && !recent.some((s) => s.down > 0 || s.up > 0)) return recent.length === f.samples.length ? f : { ...f, samples: recent };
        return {
          samples: [...recent, { t: now, down, up }],
          marks: [...f.marks.filter((m) => m.t >= cut), ...marks],
        };
      });
    };
    const id = window.setInterval(tick, 1000);
    return () => window.clearInterval(id);
  }, []);

  return <Ctx.Provider value={flow}>{children}</Ctx.Provider>;
}

const MARK_ICON: Record<MarkKind, typeof Pause> = { pause: Pause, reconnect: RefreshCw, relay: Waypoints, direct: Zap };

function niceMax(v: number): number {
  // Work in MiB so axis labels land on round numbers the way speeds are shown.
  const MB = 1024 * 1024;
  const m = Math.max(v / MB, 0.5);
  const exp = Math.pow(10, Math.floor(Math.log10(m)));
  for (const k of [1, 2, 4, 6, 8, 10]) if (k * exp >= m) return k * exp * MB;
  return 10 * exp * MB;
}

function ago(sec: number) {
  if (sec < 2) return "Now";
  return sec < 60 ? `${sec} s ago` : `${Math.floor(sec / 60)} min ${sec % 60 ? `${sec % 60} s ` : ""}ago`;
}

/** Live receive/send speed over the last two minutes. */
export function LiveFlow() {
  const { samples, marks } = useFlow();
  // The legend reads the same live figures as the rest of the screen.
  const { jobs, uploads } = useStore();
  const nowDown = Object.values(jobs).filter((j) => j.status === "downloading").reduce((a, j) => a + j.speedBps, 0);
  const nowUp = uploads.reduce((a, u) => a + u.bps, 0);
  const box = useRef<HTMLDivElement>(null);
  const [w, setW] = useState(640);
  const [hover, setHover] = useState<number | null>(null);
  const [, force] = useState(0);

  useEffect(() => {
    const el = box.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setW(el.clientWidth));
    ro.observe(el);
    setW(el.clientWidth);
    return () => ro.disconnect();
  }, []);
  // Keep the time axis moving even when no new samples arrive.
  useEffect(() => {
    const id = window.setInterval(() => force((x) => x + 1), 1000);
    return () => window.clearInterval(id);
  }, []);

  const now = Date.now();
  const recent = samples.filter((s) => s.t >= now - WINDOW * 1000);
  // End the lines on the live figures, so the graph and the legend agree.
  const visible = recent.length ? [...recent, { t: now, down: nowDown, up: nowUp }] : recent;
  const last = visible[visible.length - 1];
  const hasUp = visible.some((s) => s.up > 0);
  const hasDown = visible.some((s) => s.down > 0);

  const H = 176;
  const pad = { l: 64, r: 14, t: 18, b: 26 };
  const iw = Math.max(10, w - pad.l - pad.r);
  const ih = H - pad.t - pad.b;
  const max = niceMax(Math.max(1, ...visible.map((s) => Math.max(s.down, s.up))) * 1.1);
  const x = (t: number) => pad.l + ((t - (now - WINDOW * 1000)) / (WINDOW * 1000)) * iw;
  const y = (v: number) => pad.t + ih - (v / max) * ih;

  const path = (key: "down" | "up") => {
    const pts = visible.map((s) => `${x(s.t).toFixed(1)},${y(s[key]).toFixed(1)}`);
    return pts.length ? `M${pts.join("L")}` : "";
  };
  const area = (key: "down" | "up") => {
    if (visible.length < 2) return "";
    const first = visible[0];
    return `${path(key)}L${x(last.t).toFixed(1)},${y(0)}L${x(first.t).toFixed(1)},${y(0)}Z`;
  };

  const ticks = [0, max / 2, max];
  const hv = hover != null ? visible[hover] : null;
  const hoverMarks = hv ? marks.filter((m) => Math.abs(m.t - hv.t) <= 1500) : [];

  const onMove = (clientX: number) => {
    const r = box.current?.getBoundingClientRect();
    if (!r || visible.length === 0) return;
    const px = clientX - r.left;
    const t = now - WINDOW * 1000 + ((px - pad.l) / iw) * WINDOW * 1000;
    let best = 0;
    visible.forEach((s, i) => {
      if (Math.abs(s.t - t) < Math.abs(visible[best].t - t)) best = i;
    });
    setHover(best);
  };

  return (
    <section className="panel flow" aria-label="Live transfer speed">
      <div className="flow-head">
        <h3>Live speed</h3>
        <div className="flow-legend" role="list">
          <span role="listitem" className={hasDown ? "" : "dim"}>
            <i style={{ background: SERIES.in }} />
            Receiving
            <b className="num">{speed(nowDown)}</b>
          </span>
          <span role="listitem" className={hasUp ? "" : "dim"}>
            <i style={{ background: SERIES.out }} />
            Sending
            <b className="num">{speed(nowUp)}</b>
          </span>
        </div>
      </div>
      <div
        className="flow-plot"
        ref={box}
        onPointerMove={(e) => onMove(e.clientX)}
        onPointerDown={(e) => onMove(e.clientX)}
        onPointerLeave={() => setHover(null)}
      >
        <svg width={w} height={H} role="img" aria-label={`Receiving ${speed(nowDown)}, sending ${speed(nowUp)}; graph of the last two minutes`}>
          {ticks.map((v) => (
            <g key={v}>
              <line x1={pad.l} x2={pad.l + iw} y1={y(v)} y2={y(v)} className="flow-grid" />
              <text x={pad.l - 10} y={y(v) + 4} textAnchor="end" className="flow-tick">
                {v === 0 ? "0" : v === max ? speed(v) : speed(v).replace("/s", "")}
              </text>
            </g>
          ))}
          {[WINDOW, WINDOW / 2, 0].map((sec) => (
            <text key={sec} x={x(now - sec * 1000)} y={H - 6} textAnchor={sec === WINDOW ? "start" : sec === 0 ? "end" : "middle"} className="flow-tick">
              {sec === 0 ? "now" : sec === WINDOW ? "2 min ago" : "1 min ago"}
            </text>
          ))}
          {marks.map((m) => (
            <line key={`${m.t}-${m.kind}`} x1={x(m.t)} x2={x(m.t)} y1={pad.t - 6} y2={y(0)} className="flow-mark" />
          ))}
          {hasUp && <path d={area("up")} fill={SERIES.out} opacity={0.1} />}
          {hasDown && <path d={area("down")} fill={SERIES.in} opacity={0.1} />}
          {hasUp && <path d={path("up")} fill="none" stroke={SERIES.out} strokeWidth={2} strokeLinejoin="round" strokeLinecap="round" />}
          {hasDown && <path d={path("down")} fill="none" stroke={SERIES.in} strokeWidth={2} strokeLinejoin="round" strokeLinecap="round" />}
          {hv && <line x1={x(hv.t)} x2={x(hv.t)} y1={pad.t} y2={y(0)} className="flow-cross" />}
          {(hv ? [hv] : last ? [last] : []).map((s) => (
            <g key="dots">
              {hasUp && <circle cx={x(s.t)} cy={y(s.up)} r={4} fill={SERIES.out} className="flow-dot" />}
              {hasDown && <circle cx={x(s.t)} cy={y(s.down)} r={4} fill={SERIES.in} className="flow-dot" />}
            </g>
          ))}
        </svg>
        {marks.map((m) => {
          const Icon = MARK_ICON[m.kind];
          return (
            <span key={`${m.t}-${m.kind}-i`} className={`flow-pin pin-${m.kind}`} style={{ left: x(m.t) }} title={m.text}>
              <Icon size={10} strokeWidth={2.5} aria-hidden="true" />
            </span>
          );
        })}
        {hv && (
          <div className="flow-tip" style={{ left: Math.min(Math.max(x(hv.t), 120), w - 120) }}>
            <span className="faint small">{ago(Math.round((now - hv.t) / 1000))}</span>
            <span className="tip-row"><i style={{ background: SERIES.in }} />Receiving<b className="num">{speed(hv.down)}</b></span>
            <span className="tip-row"><i style={{ background: SERIES.out }} />Sending<b className="num">{speed(hv.up)}</b></span>
            {hoverMarks.map((m) => (
              <span key={m.t} className="tip-note small">{m.text}</span>
            ))}
          </div>
        )}
      </div>
    </section>
  );
}

/** Whether the live graph has anything to show. */
export function useFlowActive(): boolean {
  const { samples } = useFlow();
  return useMemo(() => samples.some((s) => s.t >= Date.now() - WINDOW * 1000 && (s.down > 0 || s.up > 0)), [samples]);
}
