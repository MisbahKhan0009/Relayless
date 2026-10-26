import { useMemo } from "react";
import {
  ArrowDownUp,
  CheckCircle2,
  FolderOpen,
  FolderUp,
  Info,
  KeyRound,
  LayoutGrid,
  ListChecks,
  Monitor,
  Settings as SettingsIcon,
  XCircle,
} from "lucide-react";
import { AccessRequests, ConnectDialog, Onboarding } from "./dialogs";
import { isActive, speed } from "./format";
import { useStore, type Page } from "./store";
import { Button } from "./ui";
import Overview from "./pages/Overview";
import Transfers from "./pages/Transfers";

import Shares from "./pages/Shares";
import Devices from "./pages/Devices";
import Browser from "./pages/Browser";
import Activity from "./pages/Activity";
import Settings from "./pages/Settings";

const TITLES: Record<Page, string> = {
  overview: "Overview",
  shares: "Shared folders",
  devices: "Remote devices",
  browse: "Browse",
  transfers: "Transfers",
  activity: "Activity",
  settings: "Settings",
};

function Logo() {
  return (
    <svg width="16" height="16" viewBox="0 0 24 24" aria-hidden="true">
      <circle cx="6" cy="12" r="3.6" fill="var(--accent)" />
      <rect x="6" y="10.6" width="12" height="2.8" rx="1.4" fill="var(--accent)" />
      <circle cx="18" cy="12" r="3.6" fill="var(--accent)" />
      <circle cx="18" cy="12" r="1.7" fill="#15181c" />
    </svg>
  );
}

export default function App() {
  const s = useStore();
  const jobs = useMemo(() => Object.values(s.jobs), [s.jobs]);
  const activeCount = jobs.filter((j) => isActive(j.status)).length;
  const attention = jobs.filter((j) => j.status === "needs_attention").length;
  const down = jobs.filter((j) => j.status === "downloading").reduce((a, j) => a + j.speedBps, 0);

  const nav: { id: Page; label: string; icon: React.ReactNode; count?: number; hot?: boolean; alert?: boolean }[] = [
    { id: "overview", label: "Overview", icon: <LayoutGrid size={17} /> },
    { id: "shares", label: "Shared folders", icon: <FolderUp size={17} />, count: s.shares.length || undefined },
    { id: "devices", label: "Remote devices", icon: <Monitor size={17} />, count: s.remotes.length || undefined },
    { id: "transfers", label: "Transfers", icon: <ArrowDownUp size={17} />, count: attention || activeCount || undefined, hot: activeCount > 0, alert: attention > 0 },
    { id: "activity", label: "Activity", icon: <ListChecks size={17} /> },
    { id: "settings", label: "Settings", icon: <SettingsIcon size={17} /> },
  ];
  if (s.browse) nav.splice(3, 0, { id: "browse", label: s.browse.shareName, icon: <FolderOpen size={17} /> });

  const page = (() => {
    switch (s.page) {
      case "overview":
        return <Overview />;
      case "shares":
        return <Shares />;
      case "devices":
        return <Devices />;
      case "browse":
        return <Browser />;
      case "transfers":
        return <Transfers />;
      case "activity":
        return <Activity />;
      case "settings":
        return <Settings />;
    }
  })();

  return (
    <div className="shell">
      <aside className="sidebar">
        <div className="brand">
          <span className="brand-mark"><Logo /></span>
          Relayless
        </div>
        <nav className="nav" aria-label="Main">
          {nav.map((n) => (
            <button key={n.id} className={`nav-item ${s.page === n.id ? "active" : ""}`} onClick={() => s.setPage(n.id)} aria-current={s.page === n.id ? "page" : undefined}>
              {n.icon}
              <span className="truncate">{n.label}</span>
              {n.count != null && <span className={`nav-count ${n.alert ? "alert" : n.hot ? "hot" : ""}`} title={n.alert ? "Needs attention" : undefined}>{n.count}</span>}
            </button>
          ))}
        </nav>
        <div className="sidebar-foot">
          <span className="me-name" title={s.settings?.deviceName}>{s.settings?.deviceName}</span>
          <span className="me-sub">
            <span className={`dot ${s.network?.online ? "ok" : "warn"}`} />
            {s.network?.online ? "Online" : "Connecting to network…"}
            {down > 0 && <span className="num" style={{ marginLeft: "auto" }}>↓ {speed(down)}</span>}
          </span>
        </div>
      </aside>
      <main className="main">
        <header className="topbar">
          <h1>{s.page === "browse" && s.browse ? `${s.browse.shareName}` : TITLES[s.page]}</h1>
          {s.page === "browse" && s.browse && <span className="faint">on {s.browse.peerName}</span>}
          <span className="spacer" />
          {s.settings?.connectionMode === "direct_only" ? (
            <span className="pill pill-muted" title="File data only uses direct paths">Direct only</span>
          ) : (
            <span className="pill pill-warn" title="Relay fallback is allowed">Relay fallback on</span>
          )}
          <Button variant="primary" icon={<KeyRound size={15} />} onClick={() => s.setConnectOpen(true)}>
            Connect
          </Button>
        </header>
        <div className="content">
          {s.ready ? page : null}
        </div>
      </main>

      {s.connectOpen && <ConnectDialog onClose={() => s.setConnectOpen(false)} />}
      <AccessRequests />
      {s.ready && <Onboarding />}

      <div className="toasts" aria-live="polite">
        {s.toasts.map((t) => (
          <div key={t.id} className={`toast ${t.tone}`}>
            {t.tone === "ok" ? <CheckCircle2 size={16} /> : t.tone === "err" ? <XCircle size={16} /> : <Info size={16} />}
            <span>{t.text}</span>
          </div>
        ))}
      </div>
    </div>
  );
}
