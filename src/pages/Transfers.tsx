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
  X,
} from "lucide-react";
import { android, api, errorText, type Job } from "../api";
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
      return "Only a relayed path is available and direct-only mode is on, so no file data is being sent. Relayless keeps trying to open a direct path. You can allow relay fallback in Settings.";
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

type Filter = "all" | "active" | "done" | "attention";

export default function Transfers() {
  const { jobs, setPage } = useStore();
  const flowActive = useFlowActive();
  const [filter, setFilter] = useState<Filter>("all");
  const list = useMemo(() => Object.values(jobs).sort((a, b) => b.createdAt - a.createdAt), [jobs]);
  const counts = {
    all: list.length,
    active: list.filter((j) => isActive(j.status) || j.status === "paused").length,
    done: list.filter((j) => j.status === "completed").length,
    attention: list.filter((j) => j.status === "needs_attention" || j.status === "failed").length,
  };
  const shown = list.filter((j) =>
    filter === "all"
      ? true
      : filter === "active"
        ? isActive(j.status) || j.status === "paused"
        : filter === "done"
          ? j.status === "completed"
          : j.status === "needs_attention" || j.status === "failed",
  );
  const total = list.filter((j) => j.status === "downloading").reduce((s, j) => s + j.speedBps, 0);
  const remaining = list.filter((j) => isActive(j.status)).reduce((s, j) => s + (j.size - j.doneBytes), 0);

  return (
    <div className="page">
      {(flowActive || total > 0) && <LiveFlow />}
      {list.length > 0 && (
        <p className="transfers-sum muted">
          {counts.active > 0 ? (
            <>
              <b className="num">{bytes(remaining)}</b> left across {counts.active} active transfer{counts.active === 1 ? "" : "s"}
              {total > 0 && remaining > 0 ? <>, about <b className="num">{duration(remaining / total)}</b> at the current speed</> : null}.{" "}
            </>
          ) : (
            "Nothing is in progress. "
          )}
          {counts.done} completed and verified end to end.
        </p>
      )}
      <div>
        <div className="tabs" role="tablist">
          {(
            [
              ["all", "All"],
              ["active", "Active"],
              ["done", "Completed"],
              ["attention", "Needs attention"],
            ] as [Filter, string][]
          ).map(([k, label]) => (
            <button key={k} role="tab" aria-selected={filter === k} className={`tab ${filter === k ? "on" : ""}`} onClick={() => setFilter(k)}>
              {label} <span className="c">{counts[k]}</span>
            </button>
          ))}
        </div>
        {shown.length === 0 ? (
          <div className="panel">
            <Empty icon={<Download size={20} />} title={list.length === 0 ? "No transfers yet" : `Nothing ${filter === "done" ? "completed" : "here"}`}>
              {list.length === 0 && (
                <>
                  Connect to someone who shared a folder with you, then pick files to download.{" "}
                  <a href="#" onClick={(e) => { e.preventDefault(); setPage("devices"); }}>Go to remote devices</a>
                </>
              )}
            </Empty>
          </div>
        ) : (
          <div className="panel">
            {shown.map((j) => (
              <JobRow key={j.id} job={j} />
            ))}
          </div>
        )}
      </div>
      <span className="hint">
        Downloads keep their verified progress through disconnects, app restarts and reboots.
      </span>
    </div>
  );
}
