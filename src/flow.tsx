import { createContext, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { speed } from "./format";
import { useStore } from "./store";

/**
 * Two-series chart colours: received is white, sent is red. They differ in
 * lightness as well as hue, so they stay apart for colour-blind readers.
 * Kept in sync with --series-in and --series-out in styles.css.
 */
export const SERIES = { in: "#F2F2F2", out: "#E5323F" } as const;

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

const MARK_TEXT: Record<MarkKind, string> = { pause: "Paused", reconnect: "Dropped", relay: "Relay", direct: "Direct" };

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

/**
 * Live receive and send speed over the last two minutes, one bar per second.
 * Received bars rise above the baseline, sent bars hang below it, both on the
 * same scale; each side gets height in proportion to its own peak.
 */
export function LiveFlow() {
  const { samples, marks } = useFlow();
  // The legend reads the same live figures as the rest of the screen.
  const { jobs, uploads } = useStore();
  const nowDown = Object.values(jobs).filter((j) => j.status === "downloading").reduce((a, j) => a + j.speedBps, 0);
  const nowUp = uploads.reduce((a, u) => a + u.bps, 0);
  const box = useRef<HTMLDivElement>(null);
  const [w, setW] = useState(640);
  const [hoverT, setHoverT] = useState<number | null>(null);
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
  const from = now - WINDOW * 1000;
  const visible = samples.filter((s) => s.t >= from);
  const hasDown = visible.some((s) => s.down > 0) || nowDown > 0;
  const hasUp = visible.some((s) => s.up > 0) || nowUp > 0;

  const narrow = w < 520;
  const H = narrow ? 150 : 176;
  const pad = { l: narrow ? 36 : 46, r: 8, t: 24, b: 22 };
  const iw = Math.max(10, w - pad.l - pad.r);
  const ih = H - pad.t - pad.b;
  const peakDown = Math.max(0, nowDown, ...visible.map((s) => s.down));
  const peakUp = Math.max(0, nowUp, ...visible.map((s) => s.up));
  let nd = hasDown ? niceMax(peakDown * 1.05) : 0;
  const nu = hasUp ? niceMax(peakUp * 1.05) : 0;
  if (nd + nu === 0) nd = niceMax(1);
  const k = ih / (nd + nu);
  const y0 = pad.t + nd * k;
  const x = (t: number) => pad.l + ((t - from) / (WINDOW * 1000)) * iw;
  const bw = Math.max(1.5, (iw / WINDOW) * 0.56);

  const bars = (key: "down" | "up") =>
    visible
      .filter((s) => s[key] > 0)
      .map((s) => {
        const h = Math.max(1, s[key] * k);
        return key === "down" ? `M${x(s.t).toFixed(1)} ${(y0 - 1).toFixed(1)}v${(-h).toFixed(1)}` : `M${x(s.t).toFixed(1)} ${(y0 + 1).toFixed(1)}v${h.toFixed(1)}`;
      })
      .join("");

  const hv = hoverT == null || visible.length === 0 ? null : visible.reduce((b, s) => (Math.abs(s.t - hoverT) < Math.abs(b.t - hoverT) ? s : b));
  const hoverMarks = hv ? marks.filter((m) => Math.abs(m.t - hv.t) <= 1500) : [];
  const shownMarks = marks.filter((m) => m.t >= from);

  const onMove = (clientX: number) => {
    const r = box.current?.getBoundingClientRect();
    if (!r) return;
    setHoverT(from + ((clientX - r.left - pad.l) / iw) * WINDOW * 1000);
  };

  return (
    <section className="panel flow" aria-label="Live transfer speed">
      <div className="flow-head">
        <h3>Throughput</h3>
        <span className="flow-sub">Last 2 min · 1 bar = 1 s</span>
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
        onPointerLeave={() => setHoverT(null)}
      >
        <svg width={w} height={H} role="img" aria-label={`Receiving ${speed(nowDown)}, sending ${speed(nowUp)}; bars for each second of the last two minutes`}>
          <line x1={pad.l} x2={pad.l + iw} y1={pad.t + 0.5} y2={pad.t + 0.5} className="flow-grid" />
          {hasDown && <line x1={pad.l} x2={pad.l + iw} y1={pad.t + (nd / 2) * k} y2={pad.t + (nd / 2) * k} className="flow-grid" />}
          {hasUp && <line x1={pad.l} x2={pad.l + iw} y1={H - pad.b - 0.5} y2={H - pad.b - 0.5} className="flow-grid" />}
          <line x1={pad.l} x2={pad.l + iw} y1={y0} y2={y0} className="flow-base" />
          {hasDown && (
            <text x={pad.l - 8} y={pad.t + 4} textAnchor="end" className="flow-tick">{speed(nd).replace("/s", "")}</text>
          )}
          <text x={pad.l - 8} y={y0 + 3.5} textAnchor="end" className="flow-tick">0</text>
          {hasUp && (
            <text x={pad.l - 8} y={H - pad.b + 3} textAnchor="end" className="flow-tick">{speed(nu).replace("/s", "")}</text>
          )}
          {shownMarks.map((m, i) => (
            <g key={`${m.t}-${m.kind}-${i}`} className={`flow-mark mark-${m.kind}`}>
              <line x1={x(m.t)} x2={x(m.t)} y1={pad.t - 10} y2={H - pad.b} />
              <text x={x(m.t) + 4} y={pad.t - 12}>{MARK_TEXT[m.kind]}</text>
            </g>
          ))}
          {hasDown && <path d={bars("down")} stroke={SERIES.in} strokeWidth={bw} fill="none" />}
          {hasUp && <path d={bars("up")} stroke={SERIES.out} strokeWidth={bw} fill="none" />}
          {hv && <line x1={x(hv.t)} x2={x(hv.t)} y1={pad.t} y2={H - pad.b} className="flow-cross" />}
          <text x={pad.l} y={H - 4} className="flow-tick">−2:00</text>
          <text x={pad.l + iw / 2} y={H - 4} textAnchor="middle" className="flow-tick">−1:00</text>
          <text x={pad.l + iw} y={H - 4} textAnchor="end" className="flow-tick now">NOW</text>
        </svg>
        {hv && (
          <div className="flow-tip" style={{ left: Math.min(Math.max(x(hv.t), 120), w - 120) }}>
            <span className="lab">{ago(Math.round((now - hv.t) / 1000))}</span>
            <span className="tip-row"><i style={{ background: SERIES.in }} />Receiving<b className="num">{speed(hv.down)}</b></span>
            <span className="tip-row"><i style={{ background: SERIES.out }} />Sending<b className="num">{speed(hv.up)}</b></span>
            {hoverMarks.map((m, i) => (
              <span key={i} className="tip-note small">{m.text}</span>
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
