import { useMemo } from "react";
import { ArrowUpRight, ChevronRight, FolderUp, KeyRound, Lock, Monitor } from "lucide-react";
import { bytes, isActive } from "../format";
import { useStore } from "../store";
import { LiveFlow, useFlowActive } from "../flow";
import { StorageAccessNotice } from "../android";
import { Button, RouteBadge } from "../ui";
import { JobRow, SendRow } from "./Transfers";
import { useShareFolder } from "./Shares";
import { DayDot } from "./History";
import { FocusTransfer, useFocusJob } from "../focus";

const DAY = 86_400_000;

function startOfDay(ms: number) {
  const d = new Date(ms);
  return new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
}

/** The last seven days as dots, the same marks the History calendar uses. */
function WeekStrip() {
  const { history, setPage } = useStore();
  const days = useMemo(() => {
    const today = startOfDay(Date.now());
    const out = Array.from({ length: 7 }, (_, i) => ({ t: today - (6 - i) * DAY, inB: 0, outB: 0, n: 0 }));
    for (const e of history) {
      const k = Math.round((today - startOfDay(e.finishedAt)) / DAY);
      if (k < 0 || k > 6) continue;
      const d = out[6 - k];
      if (e.direction === "received") d.inB += e.size;
      else d.outB += e.size;
      d.n++;
    }
    return out;
  }, [history]);
  const max = Math.max(1, ...days.map((d) => d.inB + d.outB));
  const inB = days.reduce((a, d) => a + d.inB, 0);
  const outB = days.reduce((a, d) => a + d.outB, 0);
  const files = days.reduce((a, d) => a + d.n, 0);
  const relayed = history.some((e) => e.finishedAt >= days[0].t && e.relayBytes > 0);
  const maxD = 30;
  const minD = 7;
  const tinyBelow = max * (minD / maxD) ** 2;

  return (
    <button className="panel week" onClick={() => setPage("history")} aria-label="Open history">
      <span className="week-head">
        <span className="week-title">This week</span>
        <ChevronRight size={18} className="faint" />
      </span>
      <span className="week-days" aria-hidden="true">
        {days.map((d, i) => {
          const total = d.inB + d.outB;
          const s = total ? Math.max(minD, maxD * Math.sqrt(total / max)) : 0;
          return (
            <span key={d.t} className={`week-day ${i === 6 ? "today" : ""}`}>
              <span className="week-dot">{s > 0 ? <DayDot i={i} d={s} inB={d.inB} outB={d.outB} tiny={total < tinyBelow} dir="both" /> : <span className="week-empty" />}</span>
              <span className="week-wd">{new Date(d.t).toLocaleDateString(undefined, { weekday: "narrow" })}</span>
            </span>
          );
        })}
      </span>
      <span className="week-text">
        {files === 0 ? (
          "Nothing moved in the last seven days."
        ) : (
          <>
            {inB > 0 && <>Received <b>{bytes(inB)}</b></>}
            {inB > 0 && outB > 0 && ", sent "}
            {inB === 0 && outB > 0 && "Sent "}
            {outB > 0 && <b>{bytes(outB)}</b>} across {files} file{files === 1 ? "" : "s"}. {relayed ? "Some went through an encrypted relay." : "All of it went directly."}
          </>
        )}
      </span>
    </button>
  );
}

export default function PhoneHome() {
  const { jobs, uploads, outgoing, remotes, grants, shares, history, setPage, setConnectOpen, openBrowser } = useStore();
  const sendingNow = outgoing.filter((o) => o.status === "sending" || o.status === "waiting");
  const shareFolder = useShareFolder();
  const flowActive = useFlowActive();
  const list = useMemo(() => Object.values(jobs).sort((a, b) => b.updatedAt - a.updatedAt), [jobs]);
  const active = list.filter((j) => isActive(j.status) || j.status === "needs_attention");
  const moving = flowActive || active.some((j) => j.status === "downloading") || uploads.some((u) => u.bps > 0);
  const last = history[0];
  const focus = useFocusJob();

  return (
    <div className="page phome">
      <StorageAccessNotice />

      {focus.job && <FocusTransfer job={focus.job} index={focus.index} count={focus.count} compact />}

      <div className="phome-actions">
        <button className="pact pact-send" onClick={shareFolder}>
          <FolderUp size={24} aria-hidden="true" />
          <span className="pact-t">Send</span>
          <span className="pact-d">Share a folder</span>
        </button>
        <button className="pact" onClick={() => setConnectOpen(true)}>
          <KeyRound size={24} aria-hidden="true" />
          <span className="pact-t">Receive</span>
          <span className="pact-d">Enter a code</span>
        </button>
      </div>

      {moving && <LiveFlow />}

      {(active.length > 0 || sendingNow.length > 0) && (
        <section>
          <div className="phome-head">
            <h2>In progress</h2>
            <button className="link small" onClick={() => setPage("transfers")}>All transfers</button>
          </div>
          <div className="panel">
            {sendingNow.slice(0, 3).map((o) => (
              <SendRow key={o.id} o={o} compact />
            ))}
            {active.slice(0, 3).map((j) => (
              <JobRow key={j.id} job={j} compact />
            ))}
          </div>
        </section>
      )}

      <WeekStrip />

      <section>
        <div className="phome-head">
          <h2>Devices</h2>
          <button className="link small" onClick={() => setPage("devices")}>See all</button>
        </div>
        <div className="panel row-list">
          {remotes.length === 0 && (
            <div className="row">
              <Monitor size={16} className="faint" aria-hidden="true" />
              <span className="grow small muted">No devices yet. Tap Receive and enter the code someone sent you.</span>
            </div>
          )}
          {remotes.slice(0, 3).map((r) => (
            <div className="row" key={r.peerId}>
              <span className="stack grow">
                <span className="truncate"><b>{r.name}</b></span>
                <RouteBadge route={r.status.route} connected={r.status.state === "connected"} />
              </span>
              {r.shares[0] && (
                <Button size="sm" onClick={() => openBrowser({ peerId: r.peerId, shareId: r.shares[0].id, shareName: r.shares[0].name, peerName: r.name })}>
                  Browse
                </Button>
              )}
            </div>
          ))}
          <button className="row phome-link" onClick={() => setPage("shares")}>
            <FolderUp size={16} className="faint" aria-hidden="true" />
            <span className="grow">
              <span className="block">Your shared folders</span>
              <span className="small faint">
                {shares.length === 0
                  ? "Nothing shared yet"
                  : `${shares.length} folder${shares.length === 1 ? "" : "s"}, ${new Set(grants.map((g) => g.peerId)).size} device${new Set(grants.map((g) => g.peerId)).size === 1 ? "" : "s"} with access`}
              </span>
            </span>
            <ChevronRight size={18} className="faint" aria-hidden="true" />
          </button>
        </div>
      </section>

      <button className="phome-proof" onClick={() => setPage("history")}>
        <Lock size={14} aria-hidden="true" />
        <span>
          {last ? (
            <>
              Last transfer: {last.name || "a file"} {last.direction === "received" ? "from" : "to"} {last.peerName},{" "}
              {last.relayBytes > 0 ? "partly relayed (encrypted)" : "direct"}, verified. Nothing kept on any server.
            </>
          ) : (
            <>Files go device to device, encrypted. Nothing is kept on any server.</>
          )}
        </span>
        <ArrowUpRight size={14} aria-hidden="true" />
      </button>
    </div>
  );
}
