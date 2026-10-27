import { useMemo, useState } from "react";
import {
  ChevronDown,
  ChevronRight,
  Download,
  ExternalLink,
  FolderOpen,
  Pause,
  Play,
  RotateCcw,
  Trash2,
  Upload as UploadIcon,
  ShieldCheck,
  Clock,
  X,
} from "lucide-react";
import { android, api, errorText, type Job, type Outgoing } from "../api";
import { isAndroid } from "../platform";
import { bytes, dateTime, duration, isActive, mbit, speed, statusTone } from "../format";
import { useStore } from "../store";
import { LiveFlow, useFlowActive } from "../flow";
import { Button, Empty, IconButton, Modal, Progress, RouteBadge, StatusIcon, StatusPill } from "../ui";

function explain(job: Job): string | null {
  switch (job.status) {
    case "waiting_peer":
      return "The sender's computer isn't reachable right now. Relayless keeps trying automatically and continues from the verified progress when it's back.";
    case "reconnecting":
      return "The connection dropped. Every verified piece is saved on disk, so only the missing pieces will be fetched again.";
    case "waiting_direct":
      return "The routers between you block a direct path and direct-only mode is on, so no file data is sent. Relayless keeps trying. To continue now, choose Direct first in Settings on both devices; data then goes through an end-to-end encrypted relay.";
    case "paused":
      return job.verifiedBytes > 0 ? "Resume continues from the verified progress below; nothing already downloaded is fetched again." : null;
    case "needs_attention":
      return job.error;
    default:
      return null;
  }
}

export function JobRow({ job, compact = false }: { job: Job; compact?: boolean }) {
  const { integrity, activity, toast } = useStore();
  const [open, setOpen] = useState(false);
  const [confirm, setConfirm] = useState(false);
  const [busy, setBusy] = useState(false);
  const tone = statusTone(job.status);
  const frac = job.size > 0 ? job.doneBytes / job.size : job.status === "completed" ? 1 : 0;
  const integ = integrity[job.id];
  const sourceChanged = job.status === "needs_attention" && (job.error ?? "").toLowerCase().includes("changed");

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

  const canPause = isActive(job.status);
  const canResume = ["paused", "failed", "canceled", "needs_attention"].includes(job.status) && !sourceChanged;
  const timeline = open ? activity.filter((a) => a.jobId === job.id).slice(0, 12) : [];

  return (
    <div className="job">
      <div className={`job-ico tone-${tone}`}>
        <StatusIcon status={job.status} size={17} />
      </div>
      <div className="stack">
        <div className="hstack">
          {!compact && (
            <IconButton label={open ? "Hide details" : "Show details"} onClick={() => setOpen(!open)} style={{ width: 22, height: 22, marginLeft: -4 }}>
              {open ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
            </IconButton>
          )}
          <span className="job-title truncate selectable" title={job.remotePath.join(" / ")}>{job.displayName}</span>
        </div>
        <div className="job-meta">
          <StatusPill status={job.status} />
          <span className="num">
            {job.status === "completed" ? bytes(job.size) : `${bytes(job.doneBytes)} of ${bytes(job.size)}`}
          </span>
          {job.status !== "completed" && job.size > 0 && <span className="num job-pct">{pct(job.doneBytes, job.size)}</span>}
          {job.status === "downloading" && <span className="num">{speed(job.speedBps)}</span>}
          {job.status === "downloading" && job.etaSecs != null && <span className="num">{duration(job.etaSecs)} left</span>}
          {job.status === "verifying" && integ && <span className="num">checked {Math.round((integ.checked / Math.max(1, integ.total)) * 100)}%</span>}
          <span className="faint truncate">from {job.peerName}</span>
        </div>
      </div>
      <div className="job-actions">
        {canPause && (
          <IconButton label="Pause" disabled={busy} onClick={() => run(() => api.pauseJob(job.id))}>
            <Pause size={16} />
          </IconButton>
        )}
        {canResume && (
          <IconButton label={job.status === "paused" ? "Resume" : "Retry"} disabled={busy} onClick={() => run(() => api.resumeJob(job.id))}>
            {job.status === "paused" ? <Play size={16} /> : <RotateCcw size={16} />}
          </IconButton>
        )}
        {job.status === "completed" && (
          <IconButton
            label={isAndroid() ? "Open" : "Show in folder"}
            onClick={() => run(() => (isAndroid() ? android.openFile(job.destPath) : api.revealPath(job.destPath)))}
          >
            {isAndroid() ? <ExternalLink size={16} /> : <FolderOpen size={16} />}
          </IconButton>
        )}
        {job.status !== "completed" && job.status !== "canceled" && (
          <IconButton label="Cancel" disabled={busy} onClick={() => setConfirm(true)}>
            <X size={16} />
          </IconButton>
        )}
        {["completed", "canceled", "failed"].includes(job.status) && (
          <IconButton label="Remove from list" disabled={busy} onClick={() => run(() => api.removeJob(job.id))}>
            <Trash2 size={15} />
          </IconButton>
        )}
      </div>
      {job.status !== "completed" && job.status !== "canceled" && (
        <div className="job-progress">
          <Progress
            value={job.status === "verifying" && integ ? integ.checked / Math.max(1, integ.total) : frac}
            tone={tone === "info" ? "info" : tone === "err" ? "err" : job.status === "paused" ? "muted" : "accent"}
            indeterminate={job.status === "connecting" || (job.status === "verifying" && !integ)}
          />
        </div>
      )}
      {sourceChanged && (
        <div className="job-details">
          <span className="small">
            The file was modified on the sender's computer after this download started. To avoid mixing two versions, Relayless stopped. You can start over with the new version.
          </span>
          <div className="hstack">
            <Button size="sm" variant="primary" loading={busy} icon={<Download size={14} />} onClick={() => run(() => api.restartJob(job.id))}>
              Download new version
            </Button>
          </div>
        </div>
      )}
      {open && (
        <div className="job-details">
          {explain(job) && !sourceChanged && <span className="small">{explain(job)}</span>}
          <dl className="kv">
            <dt>Saved to</dt>
            <dd className="selectable">{job.destPath}</dd>
            <dt>On sender</dt>
            <dd className="selectable">{job.remotePath.join(" / ")}</dd>
            <dt>Verified</dt>
            <dd className="num">
              {bytes(job.verifiedBytes)} ({job.status === "completed" ? job.totalChunks : job.verifiedChunks} of {job.totalChunks} pieces)
            </dd>
            {job.status === "downloading" && (
              <>
                <dt>Network speed</dt>
                <dd className="num">{speed(job.speedBps)} · {mbit(job.speedBps)}</dd>
              </>
            )}
            <dt>Connection</dt>
            <dd>
              <RouteBadge route={{ kind: job.route, rttMs: null, remote: null }} connected={job.route !== "none"} />
            </dd>
            <dt>Started</dt>
            <dd>{dateTime(job.createdAt)}</dd>
            {job.completedAt && (
              <>
                <dt>Completed</dt>
                <dd>{dateTime(job.completedAt)}</dd>
              </>
            )}
          </dl>
          {timeline.length > 0 && (
            <div className="timeline">
              {timeline.map((a) => (
                <div key={a.id} className="hstack">
                  <span className="t">{dateTime(a.ts)}</span>
                  <span>{a.message}</span>
                </div>
              ))}
            </div>
          )}
        </div>
      )}
      {confirm && (
        <Modal
          title="Cancel this transfer?"
          onClose={() => setConfirm(false)}
          footer={
            <>
              <Button variant="ghost" onClick={() => setConfirm(false)}>Keep downloading</Button>
              <Button variant="secondary" onClick={() => { setConfirm(false); run(() => api.cancelJob(job.id, false)); }}>
                Cancel, keep partial data
              </Button>
              <Button variant="danger" onClick={() => { setConfirm(false); run(() => api.cancelJob(job.id, true)); }}>
                Cancel and delete
              </Button>
            </>
          }
        >
          <span>
            <b>{job.displayName}</b> has {bytes(job.verifiedBytes)} of {bytes(job.size)} verified.
            {job.verifiedBytes > 0 && " Keeping the partial data lets you retry later without starting over."}
          </span>
        </Modal>
      )}
    </div>
  );
}

function pct(done: number, size: number) {
  if (size <= 0) return "100%";
  const p = (done / size) * 100;
  return p > 0 && p < 1 ? "<1%" : `${Math.floor(p)}%`;
}

const SEND_TEXT: Record<Outgoing["status"], string> = {
  sending: "Sending",
  waiting: "Waiting for receiver",
  delivered: "Sent, receiver is checking",
  verified: "Delivered and verified",
};

/** A file this device is sending, shown like a download. */
export function SendRow({ o, compact = false }: { o: Outgoing; compact?: boolean }) {
  const done = o.status === "verified" || o.status === "delivered";
  const tone = o.status === "verified" ? "ok" : o.status === "waiting" ? "info" : "out";
  const left = o.bps > 1 && !done ? (o.size - o.sentBytes) / o.bps : null;
  return (
    <div className="job">
      <div className={`job-ico tone-${tone}`}>
        {o.status === "verified" ? <ShieldCheck size={17} /> : o.status === "waiting" ? <Clock size={17} /> : <UploadIcon size={17} />}
      </div>
      <div className="stack">
        <div className="hstack">
          <span className="job-title truncate selectable" title={o.path.join(" / ")}>{o.name}</span>
        </div>
        <div className="job-meta">
          <span className={`pill pill-${tone === "out" ? "out" : tone}`}>
            {o.status === "verified" ? <ShieldCheck size={13} /> : o.status === "waiting" ? <Clock size={13} /> : <UploadIcon size={13} />}
            {o.status === "waiting" ? `Waiting for ${o.deviceName}` : SEND_TEXT[o.status]}
          </span>
          <span className="num">{done ? bytes(o.size) : `${bytes(o.sentBytes)} of ${bytes(o.size)}`}</span>
          {!done && <span className="num job-pct">{pct(o.sentBytes, o.size)}</span>}
          {o.status === "sending" && o.bps > 1 && <span className="num">{speed(o.bps)}</span>}
          {!compact && left != null && <span className="num">{duration(left)} left</span>}
          <span className="faint truncate">to {o.deviceName}</span>
        </div>
      </div>
      <div className="job-actions" />
      {!done && (
        <div className="job-progress">
          <Progress value={o.size ? o.sentBytes / o.size : 0} tone={o.status === "waiting" ? "muted" : "out"} />
        </div>
      )}
    </div>
  );
}

type Filter = "all" | "receiving" | "sending" | "done" | "attention";

export default function Transfers() {
  const { jobs, outgoing, setPage, toast } = useStore();
  const flowActive = useFlowActive();
  const [filter, setFilter] = useState<Filter>("all");
  const list = useMemo(() => Object.values(jobs).sort((a, b) => b.createdAt - a.createdAt), [jobs]);
  const sendingNow = outgoing.filter((o) => o.status === "sending" || o.status === "waiting");
  const sentDone = outgoing.filter((o) => o.status === "verified" || o.status === "delivered");
  const counts = {
    all: list.length + outgoing.length,
    receiving: list.filter((j) => isActive(j.status) || j.status === "paused").length,
    sending: sendingNow.length,
    done: list.filter((j) => j.status === "completed").length + sentDone.length,
    attention: list.filter((j) => j.status === "needs_attention" || j.status === "failed").length,
  };
  const shown = list.filter((j) =>
    filter === "all"
      ? true
      : filter === "receiving"
        ? isActive(j.status) || j.status === "paused"
        : filter === "done"
          ? j.status === "completed"
          : filter === "attention"
            ? j.status === "needs_attention" || j.status === "failed"
            : false,
  );
  const sends = filter === "all" || filter === "sending" ? outgoing : filter === "done" ? sentDone : [];
  const sendLeft = sendingNow.reduce((a, o) => a + (o.size - o.sentBytes), 0);
  const total = list.filter((j) => j.status === "downloading").reduce((s, j) => s + j.speedBps, 0);
  const remaining = list.filter((j) => isActive(j.status)).reduce((s, j) => s + (j.size - j.doneBytes), 0);

  return (
    <div className="page">
      {(flowActive || total > 0) && <LiveFlow />}
      {(list.length > 0 || outgoing.length > 0) && (
        <p className="transfers-sum muted">
          {counts.receiving > 0 && (
            <>
              Receiving: <b className="num">{bytes(remaining)}</b> left across {counts.receiving} file{counts.receiving === 1 ? "" : "s"}
              {total > 0 && remaining > 0 ? <>, about <b className="num">{duration(remaining / total)}</b> at the current speed</> : null}.{" "}
            </>
          )}
          {counts.sending > 0 && (
            <>
              Sending: <b className="num">{bytes(sendLeft)}</b> still to go across {counts.sending} file{counts.sending === 1 ? "" : "s"}.{" "}
            </>
          )}
          {counts.receiving === 0 && counts.sending === 0 && "Nothing is in progress. "}
          {counts.done} completed and verified end to end.
        </p>
      )}
      <div>
        <div className="tabs" role="tablist">
          {(
            [
              ["all", "All"],
              ["receiving", "Receiving"],
              ["sending", "Sending"],
              ["done", "Completed"],
              ["attention", "Needs attention"],
            ] as [Filter, string][]
          ).map(([k, label]) => (
            <button key={k} role="tab" aria-selected={filter === k} className={`tab ${filter === k ? "on" : ""}`} onClick={() => setFilter(k)}>
              {label} <span className="c">{counts[k]}</span>
            </button>
          ))}
        </div>
        {sends.length > 0 && (
          <div className="panel send-panel">
            {filter !== "done" && (
              <div className="send-head">
                <span className="small muted">Sending from this device</span>
                {sentDone.length > 0 && filter === "sending" && (
                  <Button size="sm" variant="ghost" onClick={() => api.clearOutgoing().catch((e) => toast(errorText(e), "err"))}>
                    Clear finished
                  </Button>
                )}
              </div>
            )}
            {sends.map((o) => (
              <SendRow key={o.id} o={o} />
            ))}
          </div>
        )}
        {shown.length === 0 && sends.length === 0 ? (
          <div className="panel">
            <Empty
              icon={filter === "sending" ? <UploadIcon size={20} /> : <Download size={20} />}
              title={filter === "sending" ? "Nothing being sent" : list.length === 0 && outgoing.length === 0 ? "No transfers yet" : `Nothing ${filter === "done" ? "completed" : "here"}`}
            >
              {filter === "sending" && "When someone downloads from your shared folders, each file shows here with its progress."}
              {filter !== "sending" && list.length === 0 && outgoing.length === 0 && (
                <>
                  Connect to someone who shared a folder with you, then pick files to download.{" "}
                  <a href="#" onClick={(e) => { e.preventDefault(); setPage("devices"); }}>Go to remote devices</a>
                </>
              )}
            </Empty>
          </div>
        ) : shown.length > 0 ? (
          <div className={`panel ${sends.length > 0 ? "recv-panel" : ""}`}>
            {sends.length > 0 && filter !== "done" && (
              <div className="send-head">
                <span className="small muted">Receiving on this device</span>
              </div>
            )}
            {shown.map((j) => (
              <JobRow key={j.id} job={j} />
            ))}
          </div>
        ) : null}
      </div>
      <span className="hint">
        Downloads keep their verified progress through disconnects, app restarts and reboots.
      </span>
    </div>
  );
}
