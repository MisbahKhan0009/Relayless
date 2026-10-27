import { ArrowUpRight, FolderUp, KeyRound, Monitor } from "lucide-react";
import { useMemo } from "react";
import { isActive, ago } from "../format";
import { useStore } from "../store";
import { Button, Empty, RouteBadge, SectionHeader, usePhone } from "../ui";
import { JobRow, SendRow } from "./Transfers";
import { useShareFolder } from "./Shares";
import { StorageAccessNotice } from "../android";
import { LiveFlow, useFlowActive } from "../flow";
import PhoneHome from "./PhoneHome";

export default function Overview() {
  return usePhone() ? <PhoneHome /> : <DesktopOverview />;
}

function DesktopOverview() {
  const { jobs, uploads, outgoing, remotes, grants, activity, setPage, setConnectOpen, openBrowser } = useStore();
  const sendingNow = outgoing.filter((o) => o.status === "sending" || o.status === "waiting");
  const shareFolder = useShareFolder();
  const list = useMemo(() => Object.values(jobs).sort((a, b) => b.updatedAt - a.updatedAt), [jobs]);
  const active = list.filter((j) => isActive(j.status) || j.status === "needs_attention");
  const down = list.filter((j) => j.status === "downloading").reduce((s, j) => s + j.speedBps, 0);
  const up = uploads.reduce((s, u) => s + u.bps, 0);
  const flowActive = useFlowActive();

  return (
    <div className="page">
      <StorageAccessNotice />
      <div className="hero-actions">
        <button className="hero-tile" onClick={shareFolder}>
          <span className="ico"><FolderUp size={22} /></span>
          <span className="stack">
            <span className="t">Share a folder</span>
            <span className="d">Pick a folder and invite someone to download from it.</span>
          </span>
        </button>
        <button className="hero-tile" onClick={() => setConnectOpen(true)}>
          <span className="ico"><KeyRound size={22} /></span>
          <span className="stack">
            <span className="t">Connect with code</span>
            <span className="d">Use an invitation or short code someone sent you.</span>
          </span>
        </button>
      </div>

      {(flowActive || down > 0 || up > 0) && <LiveFlow />}

      {sendingNow.length > 0 && (
        <div>
          <SectionHeader title="Sending now">
            <Button size="sm" variant="ghost" onClick={() => setPage("transfers")} icon={<ArrowUpRight size={14} />}>
              All transfers
            </Button>
          </SectionHeader>
          <div className="panel">
            {sendingNow.slice(0, 5).map((o) => (
              <SendRow key={o.id} o={o} compact />
            ))}
          </div>
        </div>
      )}

      <div>
        <SectionHeader title="Transfers">
          <Button size="sm" variant="ghost" onClick={() => setPage("transfers")} icon={<ArrowUpRight size={14} />}>
            All transfers
          </Button>
        </SectionHeader>
        {active.length === 0 ? (
          <div className="panel">
            <Empty icon={<ArrowUpRight size={20} />} title="Nothing in progress">
              {list.length > 0 ? `${list.filter((j) => j.status === "completed").length} completed transfer(s).` : "Downloads you start appear here with live progress."}
            </Empty>
          </div>
        ) : (
          <div className="panel">
            {active.slice(0, 5).map((j) => (
              <JobRow key={j.id} job={j} compact />
            ))}
          </div>
        )}
      </div>

      <div className="two-col">
        <div>
          <SectionHeader title="Devices" />
          <div className="panel row-list">
            {remotes.length === 0 && grants.length === 0 && (
              <Empty icon={<Monitor size={20} />} title="No devices yet">Devices you connect to or share with show up here.</Empty>
            )}
            {remotes.slice(0, 4).map((r) => (
              <div className="row" key={r.peerId}>
                <Monitor size={16} className="faint" />
                <div className="stack grow">
                  <span className="truncate"><b>{r.name}</b></span>
                  <span className="small faint">Shares with you · {r.shares.length} folder(s)</span>
                </div>
                <RouteBadge route={r.status.route} connected={r.status.state === "connected"} />
                {r.shares[0] && (
                  <Button size="sm" onClick={() => openBrowser({ peerId: r.peerId, shareId: r.shares[0].id, shareName: r.shares[0].name, peerName: r.name })}>
                    Browse
                  </Button>
                )}
              </div>
            ))}
            {grants.slice(0, 4).map((g) => (
              <div className="row" key={g.peerId + g.shareId}>
                <Monitor size={16} className="faint" />
                <div className="stack grow">
                  <span className="truncate"><b>{g.deviceName}</b></span>
                  <span className="small faint truncate">Can open “{g.shareName}”</span>
                </div>
                <RouteBadge route={g.route} connected={g.connected} />
              </div>
            ))}
          </div>
        </div>
        <div>
          <SectionHeader title="Recent activity">
            <Button size="sm" variant="ghost" onClick={() => setPage("activity")}>View all</Button>
          </SectionHeader>
          <div className="panel">
            {activity.length === 0 ? (
              <Empty icon={<ArrowUpRight size={20} />} title="No activity yet" />
            ) : (
              activity.slice(0, 6).map((a) => (
                <div className="activity-item" key={a.id}>
                  <span className={`dot ${a.level === "success" ? "ok" : a.level === "warning" || a.level === "error" ? "warn" : ""}`} style={{ marginTop: 7 }} />
                  <span className="grow small">{a.message}</span>
                  <span className="faint small">{ago(a.ts)}</span>
                </div>
              ))
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
