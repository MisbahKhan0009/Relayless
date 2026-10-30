import { useMemo, useState } from "react";
import { api, errorText, type Job } from "./api";
import { isActive, bytes, duration, speed, STATUS_TEXT } from "./format";
import { useStore } from "./store";
import { Button } from "./ui";
import { ChunkRing, LinkLine } from "./hud";

// Which unfinished download gets the big ring: moving data first, then the
// ones about to move, then the ones that wait on the user.
const FOCUS_ORDER: Job["status"][] = [
  "downloading",
  "verifying",
  "reconnecting",
  "connecting",
  "waiting_direct",
  "waiting_peer",
  "queued",
  "needs_attention",
  "paused",
];

export function useFocusJob(): { job: Job | null; index: number; count: number } {
  const { jobs } = useStore();
  return useMemo(() => {
    const list = Object.values(jobs).filter((j) => FOCUS_ORDER.includes(j.status));
    list.sort((a, b) => FOCUS_ORDER.indexOf(a.status) - FOCUS_ORDER.indexOf(b.status) || b.speedBps - a.speedBps || b.updatedAt - a.updatedAt);
    return { job: list[0] ?? null, index: list.length ? 1 : 0, count: list.length };
  }, [jobs]);
}

function two(n: number) {
  return String(n).padStart(2, "0");
}

/** The download that matters most right now, as a chunk ring with its facts. */
export function FocusTransfer({ job, index, count, compact = false }: { job: Job; index: number; count: number; compact?: boolean }) {
  const { settings, integrity, setPage, toast } = useStore();
  const [busy, setBusy] = useState(false);
  const size = Math.max(1, job.size);
  const verified = job.status === "completed" ? 1 : job.verifiedBytes / size;
  const done = job.doneBytes / size;
  const attention = job.status === "needs_attention" || job.status === "failed";
  const state = attention ? "attention" : job.status === "paused" ? "paused" : "active";
  const integ = integrity[job.id];
  const checking = job.status === "verifying" && integ ? integ.checked / Math.max(1, integ.total) : null;
  const pctShown = (checking ?? verified) * 100;
  const moving = job.status === "downloading";
  const routeCaption = job.route === "direct" ? "Direct · no relay" : job.route === "relay" ? "Relay · end-to-end encrypted" : "No path yet";
  const head = moving ? `Receiving · from ${job.peerName}` : `${STATUS_TEXT[job.status]} · ${job.peerName}`;

  const run = async (f: () => Promise<void>) => {
    setBusy(true);
    try {
      await f();
    } catch (e) {
      toast(errorText(e), "err");
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className={`focus-wrap ${compact ? "compact" : ""}`}>
      <section className={`panel focus ${attention ? "attn" : ""}`} aria-label="Active transfer">
        <div className="focus-head">
          <span className="lab">Active transfer</span>
          <span className="num faint">{two(index)} / {two(count)}</span>
        </div>
        <div className="focus-body">
          <ChunkRing
            verified={verified}
            done={done}
            size={compact ? 200 : 228}
            state={state}
            label={`${Math.floor(pctShown)} percent ${checking != null ? "checked" : "verified"}`}
          >
            <span className="ring-pct">
              {pctShown.toFixed(1)}
              <small>%</small>
            </span>
            <span className="lab">{checking != null ? "Checking" : "Verified"}</span>
            <span className="ring-sub">
              {bytes(job.verifiedBytes)} / {bytes(job.size)}
            </span>
          </ChunkRing>
          <div className="focus-facts">
            <span className={`lab ${moving || attention ? "red" : ""}`}>{head}</span>
            <span className="focus-name selectable" title={job.remotePath.join(" / ")}>{job.displayName}</span>
            <span className="focus-path num faint truncate">{job.remotePath.slice(0, -1).join(" / ") || "Shared folder root"}</span>
            {attention && job.error && <span className="focus-err small">{job.error}</span>}
            <dl className="focus-kv">
              <div>
                <dt className="lab">Speed</dt>
                <dd className="num">{moving ? speed(job.speedBps) : "–"}</dd>
              </div>
              <div>
                <dt className="lab">Time left</dt>
                <dd className="num">{moving && job.etaSecs != null ? duration(job.etaSecs) : "–"}</dd>
              </div>
              <div>
                <dt className="lab">Chunks</dt>
                <dd className="num">
                  {job.verifiedChunks.toLocaleString()}
                  <span className="faint">{compact ? "/" : " / "}{job.totalChunks.toLocaleString()}</span>
                </dd>
              </div>
              {!compact && (
                <>
                  <div>
                    <dt className="lab">Blocks verified</dt>
                    <dd className="num">{Math.floor(job.verifiedBytes / 1048576).toLocaleString()}</dd>
                  </div>
                  <div>
                    <dt className="lab">Route</dt>
                    <dd className="num">{job.route === "direct" ? "Direct" : job.route === "relay" ? "Relay" : "–"}</dd>
                  </div>
                  <div>
                    <dt className="lab">Encryption</dt>
                    <dd className="num">QUIC · TLS 1.3</dd>
                  </div>
                </>
              )}
            </dl>
            <div className="focus-foot">
              <LinkLine local={settings?.deviceName || "This device"} peer={job.peerName} kind={job.route} caption={routeCaption} moving={moving} />
              <div className="focus-actions">
                {isActive(job.status) && (
                  <Button size="sm" loading={busy} onClick={() => run(() => api.pauseJob(job.id))}>Pause</Button>
                )}
                {job.status === "paused" && (
                  <Button size="sm" variant="primary" loading={busy} onClick={() => run(() => api.resumeJob(job.id))}>Resume</Button>
                )}
                <Button size="sm" variant="ghost" onClick={() => setPage("transfers")}>Details</Button>
              </div>
            </div>
          </div>
        </div>
      </section>
    </div>
  );
}

