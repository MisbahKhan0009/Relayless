import { useEffect, useMemo, useRef, useState } from "react";
import { Check, ChevronDown, ChevronLeft, ChevronRight, Copy, History as HistoryIcon, Lock, Waypoints, Zap } from "lucide-react";
import type { HistoryEntry } from "../api";
import { CATEGORIES, categoryInfo, categoryOf, type Category } from "../categories";
import { SERIES } from "../flow";
import { bytes, duration, speed } from "../format";
import { useStore } from "../store";
import { Button, Empty, IconButton, SafetyWords, Segmented, usePhone } from "../ui";

export type Dir = "both" | "received" | "sent";

const DAY = 86_400_000;

function dayKey(ms: number) {
  const d = new Date(ms);
  return `${d.getFullYear()}-${d.getMonth()}-${d.getDate()}`;
}

function weekStart(): number {
  try {
    // 1 = Monday ... 7 = Sunday
    const info = (new Intl.Locale(navigator.language) as unknown as { weekInfo?: { firstDay: number }; getWeekInfo?: () => { firstDay: number } });
    const first = info.getWeekInfo?.().firstDay ?? info.weekInfo?.firstDay ?? 1;
    return first % 7;
  } catch {
    return 1;
  }
}

const RETENTION_TEXT: Record<string, string> = {
  forever: "until you clear it",
  "365": "for 1 year",
  "90": "for 90 days",
  "30": "for 30 days",
  off: "",
};

function moved(e: HistoryEntry) {
  return e.size;
}

function sum(list: HistoryEntry[], dir?: "received" | "sent") {
  return list.filter((e) => !dir || e.direction === dir).reduce((a, e) => a + moved(e), 0);
}

function fileLabel(e: HistoryEntry) {
  return e.name || categoryInfo(categoryOf(e.ext)).noun + (e.ext ? ` (.${e.ext})` : "");
}

function fmtDay(ms: number, opts: Intl.DateTimeFormatOptions = { weekday: "short", month: "short", day: "numeric" }) {
  return new Date(ms).toLocaleDateString(undefined, opts);
}

function fmtTime(ms: number) {
  return new Date(ms).toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" });
}

// ---------------------------------------------------------------- receipt

function receiptText(e: HistoryEntry) {
  const total = e.directBytes + e.relayBytes || e.size;
  const route =
    e.relayBytes === 0
      ? "Direct, device to device (100%)"
      : `${bytes(e.directBytes)} direct, ${bytes(e.relayBytes)} through a relay (end-to-end encrypted)`;
  return [
    `Relayless privacy receipt`,
    `File: ${fileLabel(e)} (${bytes(e.size)})`,
    `${e.direction === "received" ? "From" : "To"}: ${e.peerName}`,
    `Safety words: ${e.safetyWords.join(" ")}`,
    `Route: ${route}`,
    `Encryption: QUIC with TLS 1.3; keys exist only on the two devices`,
    `Verified: ${e.blocks.toLocaleString()} blocks of 1 MiB with BLAKE3, then the whole file`,
    `Fingerprint: ${e.fingerprint}`,
    `Copies on servers: none`,
    `Finished: ${new Date(e.finishedAt).toLocaleString()}`,
    `Active time: ${duration(e.activeMs / 1000)}, average ${speed(e.activeMs ? total / (e.activeMs / 1000) : 0)}, peak ${speed(e.peakBps)}`,
  ].join("\n");
}

function Receipt({ e }: { e: HistoryEntry }) {
  const [copied, setCopied] = useState(false);
  const total = e.directBytes + e.relayBytes || e.size;
  const avg = e.activeMs ? total / (e.activeMs / 1000) : 0;
  const directShare = total ? e.directBytes / total : 1;
  const fp = e.fingerprint.slice(0, 32).match(/.{1,4}/g) ?? [];
  return (
    <div className="receipt">
      <dl className="receipt-kv">
        <dt>Route</dt>
        <dd>
          {e.relayBytes === 0 ? (
            <span className="ok-text"><Zap size={13} aria-hidden="true" /> Direct, device to device</span>
          ) : (
            <span>
              <Waypoints size={13} aria-hidden="true" className="warn-text" /> {bytes(e.directBytes)} direct and {bytes(e.relayBytes)} through a relay.
              The relay only forwarded encrypted packets it could not read.
            </span>
          )}
          <span className="route-meter" aria-hidden="true">
            <span style={{ width: `${directShare * 100}%` }} />
          </span>
        </dd>
        <dt>Encryption</dt>
        <dd>QUIC with TLS 1.3. The keys exist only on the two devices.</dd>
        <dt>Verified</dt>
        <dd>
          {e.blocks.toLocaleString()} block{e.blocks === 1 ? "" : "s"} of 1 MiB, each checked with BLAKE3 on arrival, then the whole file again before it was saved.
        </dd>
        <dt>Fingerprint</dt>
        <dd>
          <span className="mono selectable fp" title={e.fingerprint}>{fp.join(" ")}</span>
          <span className="hint">The same on both devices for the same file.</span>
        </dd>
        <dt>{e.direction === "received" ? "Sender" : "Receiver"}</dt>
        <dd className="receipt-peer">
          <span>{e.peerName}</span>
        </dd>
        {e.safetyWords.length > 0 && (
          <>
            <dt>Safety words</dt>
            <dd>
              <SafetyWords words={e.safetyWords} compact />
              <span className="hint">{e.peerName} shows the same five words. Same words on both screens means nobody was in between.</span>
            </dd>
          </>
        )}
        <dt>On servers</dt>
        <dd>Nothing. No account, no upload, no copy anywhere but the two devices.</dd>
        <dt>Timing</dt>
        <dd className="num">
          {duration(e.activeMs / 1000)} of transfer time, averaging {speed(avg)}, peak {speed(e.peakBps)}
        </dd>
      </dl>
      <div className="receipt-foot">
        <span className="hint">This receipt lives only in this device's history.</span>
        <Button
          size="sm"
          icon={copied ? <Check size={14} /> : <Copy size={14} />}
          onClick={async () => {
            await navigator.clipboard.writeText(receiptText(e));
            setCopied(true);
            window.setTimeout(() => setCopied(false), 1600);
          }}
        >
          {copied ? "Copied" : "Copy receipt"}
        </Button>
      </div>
    </div>
  );
}

function ReceiptRow({ e, open, onToggle }: { e: HistoryEntry; open: boolean; onToggle: () => void }) {
  const cat = categoryInfo(categoryOf(e.ext));
  const Icon = cat.icon;
  const relayed = e.relayBytes > 0;
  return (
    <div className={`hrow ${open ? "open" : ""}`}>
      <button className="hrow-main" onClick={onToggle} aria-expanded={open}>
        <span className={`hrow-ico ${e.direction}`} aria-hidden="true"><Icon size={16} /></span>
        <span className="stack grow" style={{ minWidth: 0 }}>
          <span className={`truncate ${e.name ? "" : "muted"}`}>{fileLabel(e)}</span>
          <span className="small faint hrow-meta">
            {e.direction === "received" ? `From ${e.peerName}` : `To ${e.peerName}`}, {fmtDay(e.finishedAt, { month: "short", day: "numeric" })} at {fmtTime(e.finishedAt)}
          </span>
        </span>
        <span className="num hrow-size">{bytes(e.size)}</span>
        <span className={`hrow-route ${relayed ? "relay" : ""}`} aria-label={relayed ? "Partly relayed, encrypted" : "Direct"} title={relayed ? "Partly relayed, encrypted" : "Direct"}>
          {relayed ? <Waypoints size={12} aria-hidden="true" /> : <Zap size={12} aria-hidden="true" />}
          <span className="hrow-route-text">{relayed ? "Partly relayed" : "Direct"}</span>
        </span>
        <ChevronDown size={16} className="hrow-chev" aria-hidden="true" />
      </button>
      {open && <Receipt e={e} />}
    </div>
  );
}

// ---------------------------------------------------------------- calendar

/**
 * One day's mark. Area follows bytes moved. With both directions shown, the
 * disc splits into received and sent wedges. Days too small to draw at scale
 * get a hollow ring, so a 3 MB day never looks like a 1 GB one.
 */
export function DayDot({ d, inB, outB, tiny, dir, i = 0 }: { d: number; inB: number; outB: number; tiny: boolean; dir: Dir; i?: number }) {
  const r = d / 2;
  const style = { ["--i" as string]: i } as React.CSSProperties;
  const main = dir === "sent" || (dir === "both" && outB > inB) ? SERIES.out : SERIES.in;
  if (tiny) {
    return (
      <svg className="cal-dot" width={d} height={d} aria-hidden="true" style={style}>
        <circle cx={r} cy={r} r={r - 1} fill="none" stroke={main} strokeWidth={1.75} />
      </svg>
    );
  }
  const total = inB + outB;
  if (dir !== "both" || inB === 0 || outB === 0 || total === 0) {
    return (
      <svg className="cal-dot" width={d} height={d} aria-hidden="true" style={style}>
        <circle cx={r} cy={r} r={r} fill={main} />
      </svg>
    );
  }
  // Received wedge from 12 o'clock clockwise, sent wedge for the rest.
  const a = (inB / total) * Math.PI * 2;
  const x = r + r * Math.sin(a);
  const y = r - r * Math.cos(a);
  const large = a > Math.PI ? 1 : 0;
  const inPath = `M${r},${r}L${r},0A${r},${r} 0 ${large} 1 ${x},${y}Z`;
  const outPath = `M${r},${r}L${x},${y}A${r},${r} 0 ${1 - large} 1 ${r},0Z`;
  return (
    <svg className="cal-dot" width={d} height={d} aria-hidden="true" style={style}>
      <path d={inPath} fill={SERIES.in} />
      <path d={outPath} fill={SERIES.out} />
      <path d={`M${r},0L${r},${r}L${x},${y}`} fill="none" stroke="#202020" strokeWidth={1.5} strokeLinejoin="round" />
    </svg>
  );
}

interface DayAgg {
  key: string;
  date: Date;
  total: number;
  inBytes: number;
  outBytes: number;
  entries: HistoryEntry[];
}

function MonthCalendar({
  month,
  days,
  max,
  dir,
  selected,
  onSelect,
}: {
  month: Date;
  days: Map<string, DayAgg>;
  max: number;
  dir: Dir;
  selected: string | null;
  onSelect: (k: string | null) => void;
}) {
  const phone = usePhone();
  const first = weekStart();
  const [hover, setHover] = useState<{ key: string; x: number; y: number; below: boolean } | null>(null);
  const grid = useRef<HTMLDivElement>(null);

  const cells = useMemo(() => {
    const y = month.getFullYear();
    const m = month.getMonth();
    const lead = (new Date(y, m, 1).getDay() - first + 7) % 7;
    const count = new Date(y, m + 1, 0).getDate();
    const out: (Date | null)[] = Array.from({ length: lead }, () => null);
    for (let d = 1; d <= count; d++) out.push(new Date(y, m, d));
    while (out.length % 7) out.push(null);
    return out;
  }, [month, first]);

  const weekdays = useMemo(
    () => Array.from({ length: 7 }, (_, i) => new Date(2023, 0, 1 + ((first + i) % 7)).toLocaleDateString(undefined, { weekday: phone ? "narrow" : "short" })),
    [first, phone],
  );
  const todayKey = dayKey(Date.now());
  const maxD = phone ? 30 : 40;
  const minD = 7;
  const size = (b: number) => (b <= 0 ? 0 : Math.max(minD, maxD * Math.sqrt(b / max)));
  // Below this, a true-to-scale disc would be smaller than the minimum mark.
  // Snapped up to a round size so the legend can name it.
  const tinyBelow = useMemo(() => {
    const raw = max * (minD / maxD) ** 2;
    const MB = 1024 ** 2;
    const steps = [MB, 10 * MB, 100 * MB, 250 * MB, 500 * MB, 1024 * MB, 2048 * MB, 5120 * MB, 10240 * MB, 25600 * MB, 51200 * MB];
    return steps.find((x) => x >= raw) ?? raw;
  }, [max, maxD]);

  const showTip = (key: string, el: HTMLElement) => {
    const g = grid.current?.getBoundingClientRect();
    const r = el.getBoundingClientRect();
    if (!g) return;
    const top = r.top - g.top;
    // Open below the day when there isn't room above it.
    const below = top < 190;
    const x = Math.min(Math.max(r.left - g.left + r.width / 2, 126), g.width - 126);
    setHover({ key, x, y: below ? top + r.height : top, below });
  };
  const tip = hover ? days.get(hover.key) : null;

  // Reference sizes for the legend, in round numbers under the current scale.
  const refs = useMemo(() => {
    if (max <= 0) return [];
    const GB = 1024 ** 3;
    const MB = 1024 ** 2;
    const steps = [MB, 10 * MB, 100 * MB, GB, 5 * GB, 10 * GB, 25 * GB, 50 * GB, 100 * GB, 250 * GB, 500 * GB, 1024 * GB];
    const fit = steps.filter((s) => s <= max);
    if (fit.length === 0) return [max];
    const top = fit[fit.length - 1];
    const lo = fit.find((s) => s >= Math.max(top / 30, tinyBelow)) ?? top;
    const mid = fit.find((s) => s >= Math.sqrt(top * lo)) ?? top;
    return Array.from(new Set([lo, mid, top]));
  }, [max, tinyBelow]);

  return (
    <div className="cal" ref={grid} onPointerLeave={() => setHover(null)}>
      <div className="cal-grid" key={`${month.getTime()}-${dir}`} role="grid" aria-label={month.toLocaleDateString(undefined, { month: "long", year: "numeric" })}>
        <div className="cal-row cal-wd" role="row">
          {weekdays.map((w, i) => (
            <span key={i} role="columnheader" className="cal-wdn">{w}</span>
          ))}
        </div>
        {Array.from({ length: cells.length / 7 }, (_, r) => (
          <div className="cal-row" role="row" key={r}>
            {cells.slice(r * 7, r * 7 + 7).map((d, i) => {
              if (!d) return <span key={i} className="cal-day blank" role="gridcell" />;
              const k = dayKey(d.getTime());
              const a = days.get(k);
              const total = a?.total ?? 0;
              const future = d.getTime() > Date.now();
              const s = size(total);
              const label = `${d.toLocaleDateString(undefined, { weekday: "long", month: "long", day: "numeric" })}: ${total ? `${bytes(total)} in ${a!.entries.length} transfer${a!.entries.length === 1 ? "" : "s"}` : "nothing moved"}`;
              return (
                <button
                  key={i}
                  role="gridcell"
                  className={`cal-day ${k === todayKey ? "today" : ""} ${k === selected ? "sel" : ""} ${future ? "future" : ""} ${total ? "has" : ""}`}
                  aria-label={label}
                  aria-pressed={k === selected}
                  disabled={!total && k !== selected}
                  onClick={() => onSelect(k === selected ? null : k)}
                  onPointerEnter={(e) => total && e.pointerType === "mouse" && showTip(k, e.currentTarget)}
                  onFocus={(e) => total && showTip(k, e.currentTarget)}
                  onBlur={() => setHover(null)}
                >
                  <span className="cal-num num">{d.getDate()}</span>
                  {s > 0 && <DayDot i={r * 7 + i} d={s} inB={a!.inBytes} outB={a!.outBytes} tiny={total < tinyBelow} dir={dir} />}
                </button>
              );
            })}
          </div>
        ))}
      </div>
      {refs.length > 0 && (
        <div className="cal-scale" aria-hidden="true">
          <span className="faint small">Dot area shows how much moved</span>
          <span className="cal-refs">
            <span className="cal-ref">
              <DayDot d={minD} inB={1} outB={0} tiny dir={dir} />
              <span className="small faint num">under {bytes(tinyBelow)}</span>
            </span>
            {refs.map((r) => (
              <span key={r} className="cal-ref">
                <DayDot d={size(r)} inB={dir === "sent" ? 0 : r} outB={dir === "sent" ? r : 0} tiny={false} dir={dir} />
                <span className="small faint num">{bytes(r)}</span>
              </span>
            ))}
            {dir === "both" && (
              <span className="cal-ref">
                <DayDot d={22} inB={2} outB={1} tiny={false} dir="both" />
                <span className="small faint">received and sent</span>
              </span>
            )}
          </span>
        </div>
      )}
      {tip && hover && (
        <div className={`cal-tip ${hover.below ? "below" : ""}`} style={{ left: hover.x, top: hover.y }} role="tooltip">
          <span className="tip-title">{fmtDay(tip.date.getTime(), { weekday: "long", month: "short", day: "numeric" })}</span>
          <span className="small muted">
            {bytes(tip.total)} in {tip.entries.length} transfer{tip.entries.length === 1 ? "" : "s"}
          </span>
          {dir === "both" && (
            <span className="tip-split">
              <span className="tip-row"><i style={{ background: SERIES.in }} />Received<b className="num">{bytes(tip.inBytes)}</b></span>
              <span className="tip-row"><i style={{ background: SERIES.out }} />Sent<b className="num">{bytes(tip.outBytes)}</b></span>
            </span>
          )}
          <span className="tip-files">
            {[...tip.entries]
              .sort((a, b) => b.size - a.size)
              .slice(0, 3)
              .map((e) => (
                <span key={e.id} className="tip-file">
                  <span className="truncate">{fileLabel(e)}</span>
                  <span className="num faint">{bytes(e.size)}</span>
                </span>
              ))}
            {tip.entries.length > 3 && <span className="faint small">and {tip.entries.length - 3} more</span>}
          </span>
          <span className="small faint truncate">With {Array.from(new Set(tip.entries.map((e) => e.peerName))).join(", ")}</span>
        </div>
      )}
    </div>
  );
}

// ---------------------------------------------------------------- page

export default function History() {
  const { history, settings, setPage } = useStore();
  const [dir, setDir] = useState<Dir>("both");
  const [month, setMonth] = useState(() => {
    const d = new Date();
    return new Date(d.getFullYear(), d.getMonth(), 1);
  });
  const [selected, setSelected] = useState<string | null>(null);
  const [open, setOpen] = useState<string | null>(null);
  const [limit, setLimit] = useState(40);

  useEffect(() => {
    setSelected(null);
    setLimit(40);
  }, [month, dir]);

  const filtered = useMemo(() => (dir === "both" ? history : history.filter((e) => e.direction === dir)), [history, dir]);

  // Per-day totals across all history, so dot sizes compare between months.
  const { days, max } = useMemo(() => {
    const days = new Map<string, DayAgg>();
    for (const e of filtered) {
      const k = dayKey(e.finishedAt);
      let a = days.get(k);
      if (!a) {
        const d = new Date(e.finishedAt);
        a = { key: k, date: new Date(d.getFullYear(), d.getMonth(), d.getDate()), total: 0, inBytes: 0, outBytes: 0, entries: [] };
        days.set(k, a);
      }
      a.total += moved(e);
      if (e.direction === "received") a.inBytes += moved(e);
      else a.outBytes += moved(e);
      a.entries.push(e);
    }
    let max = 0;
    days.forEach((a) => (max = Math.max(max, a.total)));
    return { days, max };
  }, [filtered]);

  const monthStart = month.getTime();
  const monthEnd = new Date(month.getFullYear(), month.getMonth() + 1, 1).getTime();
  const inMonth = useMemo(() => filtered.filter((e) => e.finishedAt >= monthStart && e.finishedAt < monthEnd), [filtered, monthStart, monthEnd]);
  const scope = selected ? days.get(selected)?.entries ?? [] : inMonth;
  const scopeAll = useMemo(
    () => (selected ? history.filter((e) => dayKey(e.finishedAt) === selected) : history.filter((e) => e.finishedAt >= monthStart && e.finishedAt < monthEnd)),
    [history, selected, monthStart, monthEnd],
  );

  // With whom: per device, both directions (the bar shows the split).
  const pairs = useMemo(() => {
    const m = new Map<string, { id: string; name: string; inB: number; outB: number; n: number; words: string[] }>();
    for (const e of scopeAll) {
      if (dir !== "both" && e.direction !== dir) continue;
      const p = m.get(e.peerId) ?? { id: e.peerId, name: e.peerName, inB: 0, outB: 0, n: 0, words: e.safetyWords };
      if (e.direction === "received") p.inB += e.size;
      else p.outB += e.size;
      p.n++;
      p.name = e.peerName;
      m.set(e.peerId, p);
    }
    return [...m.values()].sort((a, b) => b.inB + b.outB - (a.inB + a.outB));
  }, [scopeAll, dir]);
  const pairMax = Math.max(1, ...pairs.map((p) => p.inB + p.outB));

  const cats = useMemo(() => {
    const m = new Map<Category, { b: number; n: number }>();
    for (const e of scope) {
      const c = categoryOf(e.ext);
      const v = m.get(c) ?? { b: 0, n: 0 };
      v.b += e.size;
      v.n++;
      m.set(c, v);
    }
    return CATEGORIES.filter((c) => m.has(c.id))
      .map((c) => ({ ...c, ...m.get(c.id)! }))
      .sort((a, b) => b.b - a.b);
  }, [scope]);
  const catTotal = cats.reduce((a, c) => a + c.b, 0) || 1;

  const receipts = useMemo(() => [...scope].sort((a, b) => b.finishedAt - a.finishedAt), [scope]);

  const now = new Date();
  const isCurrent = month.getFullYear() === now.getFullYear() && month.getMonth() === now.getMonth();
  const earliest = history.length ? Math.min(...history.map((e) => e.finishedAt)) : Date.now();
  const canBack = monthStart > earliest - 31 * DAY;
  const monthName = month.toLocaleDateString(undefined, { month: "long", year: month.getFullYear() === now.getFullYear() ? undefined : "numeric" });
  const shift = (n: number) => setMonth((m) => new Date(m.getFullYear(), m.getMonth() + n, 1));

  const historyOff = settings?.historyRetention === "off";

  // The plain-language summary of the period.
  const inB = sum(scopeAll, "received");
  const outB = sum(scopeAll, "sent");
  const relay = scopeAll.reduce((a, e) => a + e.relayBytes, 0);
  const files = scopeAll.length;
  const when = selected ? `On ${fmtDay(days.get(selected)!.date.getTime(), { weekday: "long", month: "long", day: "numeric" })}` : isCurrent ? "This month" : `In ${monthName}`;

  return (
    <div className="page history">
      {historyOff && (
        <div className="notice">
          <Lock size={16} />
          <span>
            History is off, so finished transfers aren't recorded on this device.{" "}
            <button className="link" onClick={() => setPage("settings")}>Turn it on in Settings</button>
          </span>
        </div>
      )}
      <div className="history-top">
        <section className="panel cal-panel" aria-label="Calendar">
          <div className="cal-head">
            <IconButton label="Previous month" onClick={() => shift(-1)} disabled={!canBack}>
              <ChevronLeft size={18} />
            </IconButton>
            <h2 className="cal-title">{monthName}</h2>
            <IconButton label="Next month" onClick={() => shift(1)} disabled={isCurrent}>
              <ChevronRight size={18} />
            </IconButton>
            {!isCurrent && (
              <Button size="sm" variant="ghost" onClick={() => setMonth(new Date(now.getFullYear(), now.getMonth(), 1))}>
                Today
              </Button>
            )}
            <span className="spacer" />
            <Segmented
              label="Direction"
              value={dir}
              onChange={setDir}
              options={[
                { value: "both", label: "Both" },
                { value: "received", label: "Received" },
                { value: "sent", label: "Sent" },
              ]}
            />
          </div>
          <MonthCalendar month={month} days={days} max={max} dir={dir} selected={selected} onSelect={setSelected} />
        </section>

        <aside className="history-side">
          <div className="summary">
            {files === 0 ? (
              <p className="summary-text">
                {when}, nothing {dir === "sent" ? "was sent" : dir === "received" ? "was received" : "moved"}.
                {history.length === 0 && " Finished transfers show up here, with a privacy receipt for each one."}
              </p>
            ) : (
              <p className="summary-text">
                {when} you {inB > 0 && <>received <b>{bytes(inB)}</b></>}
                {inB > 0 && outB > 0 && " and "}
                {outB > 0 && <>sent <b>{bytes(outB)}</b></>} across {files} file{files === 1 ? "" : "s"}.{" "}
                {relay === 0 ? (
                  <>Every byte went <b>directly</b> between devices and was verified block by block.</>
                ) : (
                  <>
                    <b>{bytes(relay)}</b> went through a relay, encrypted end to end so the relay couldn't read it. The rest went directly.
                  </>
                )}
              </p>
            )}
            {selected && (
              <button className="link small" onClick={() => setSelected(null)}>
                Show all of {monthName}
              </button>
            )}
          </div>

          <section className="side-block" aria-label="With whom">
            <div className="side-head">
              <h3>With whom</h3>
              {dir === "both" && pairs.length > 0 && (
                <span className="side-legend">
                  <span><i style={{ background: SERIES.in }} />Received</span>
                  <span><i style={{ background: SERIES.out }} />Sent</span>
                </span>
              )}
            </div>
            {pairs.length === 0 ? (
              <span className="small faint">No devices in this period.</span>
            ) : (
              pairs.map((p) => (
                <div className="pair" key={p.id} title={`Safety words: ${p.words.join(" ")}`}>
                  <div className="pair-top">
                    <span className="truncate pair-name">{p.name}</span>
                    <span className="num small muted pair-val">
                      {p.inB > 0 && <>{bytes(p.inB)} in</>}
                      {p.inB > 0 && p.outB > 0 && ", "}
                      {p.outB > 0 && <>{bytes(p.outB)} out</>}
                    </span>
                  </div>
                  <div className="pair-bar" aria-hidden="true" style={{ width: `${((p.inB + p.outB) / pairMax) * 100}%` }}>
                    {p.inB > 0 && <span style={{ flexGrow: p.inB, background: SERIES.in }} />}
                    {p.outB > 0 && <span style={{ flexGrow: p.outB, background: SERIES.out }} />}
                  </div>
                </div>
              ))
            )}
          </section>

          <section className="side-block" aria-label="What moved">
            <div className="side-head">
              <h3>What</h3>
            </div>
            {cats.length === 0 ? (
              <span className="small faint">No files in this period.</span>
            ) : (
              cats.map((c) => {
                const Icon = c.icon;
                return (
                  <div className="cat" key={c.id}>
                    <Icon size={15} className="faint" aria-hidden="true" />
                    <span className="cat-name truncate">{c.label}</span>
                    <span className="cat-track" aria-hidden="true">
                      <span className={c.b / cats[0].b < 0.03 ? "tiny" : ""} style={{ width: `${Math.max(1, (c.b / cats[0].b) * 100)}%` }} />
                    </span>
                    <span className="num small cat-val">{bytes(c.b)}</span>
                    <span className="num small faint cat-pct">{c.b / catTotal < 0.01 ? "<1%" : `${Math.round((c.b / catTotal) * 100)}%`}</span>
                  </div>
                );
              })
            )}
          </section>
        </aside>
      </div>

      <section aria-label="Receipts">
        <div className="section-head">
          <h3>Receipts</h3>
          <span className="faint small">{receipts.length > 0 && `${receipts.length} in ${selected ? "this day" : "this month"}`}</span>
        </div>
        <div className="panel">
          {receipts.length === 0 ? (
            <Empty icon={<HistoryIcon size={20} />} title={history.length === 0 ? "No finished transfers yet" : "Nothing in this period"}>
              {history.length === 0
                ? "Each finished transfer gets a privacy receipt here: the route it took, how it was encrypted and verified, and what servers kept (nothing)."
                : "Pick another month or day on the calendar."}
            </Empty>
          ) : (
            <>
              {receipts.slice(0, limit).map((e) => (
                <ReceiptRow key={e.id} e={e} open={open === e.id} onToggle={() => setOpen(open === e.id ? null : e.id)} />
              ))}
              {receipts.length > limit && (
                <div className="more">
                  <Button size="sm" variant="ghost" onClick={() => setLimit((l) => l + 40)}>
                    Show {Math.min(40, receipts.length - limit)} more
                  </Button>
                </div>
              )}
            </>
          )}
        </div>
      </section>

      {!historyOff && (
        <p className="history-foot small faint">
          <Lock size={13} aria-hidden="true" />
          History stays on this device and is kept {RETENTION_TEXT[settings?.historyRetention ?? "365"]}. Nothing here is ever uploaded.{" "}
          <button className="link" onClick={() => setPage("settings")}>Privacy settings</button>
        </p>
      )}
    </div>
  );
}
