import { useMemo, useState } from "react";
import {
  ArrowDownUp,
  CalendarDays,
  CheckCircle2,
  ChevronRight,
  FolderOpen,
  FolderUp,
  Info,
  KeyRound,
  LayoutGrid,
  ListChecks,
  Menu,
  Monitor,
  Settings as SettingsIcon,
  XCircle,
} from "lucide-react";
import { AccessRequests, ConnectDialog, Onboarding } from "./dialogs";
import { FolderPickerHost } from "./android";
import { isActive, speed } from "./format";
import { useStore, type Page } from "./store";
import { Button, Modal, usePhone } from "./ui";
import Overview from "./pages/Overview";
import Transfers from "./pages/Transfers";

import Shares from "./pages/Shares";
import Devices from "./pages/Devices";
import Browser from "./pages/Browser";
import Activity from "./pages/Activity";
import History from "./pages/History";
import Settings from "./pages/Settings";

const TITLES: Record<Page, string> = {
  overview: "Overview",
  shares: "Shared folders",
  devices: "Remote devices",
  browse: "Browse",
  transfers: "Transfers",
  history: "History",
  activity: "Activity",
  settings: "Settings",
};

function Logo() {
  return (
    <svg width="16" height="16" viewBox="0 0 24 24" aria-hidden="true">
      <circle cx="6" cy="12" r="3.6" fill="var(--accent)" />
      <rect x="6" y="10.6" width="12" height="2.8" rx="1.4" fill="var(--accent)" />
      <circle cx="18" cy="12" r="3.6" fill="var(--accent)" />
      <circle cx="18" cy="12" r="1.7" fill="#121212" />
    </svg>
  );
}

const PHONE_TABS: Page[] = ["overview", "devices", "transfers"];

/** Phone-only sheet for the destinations that don't fit the bottom bar. */
function MoreSheet({ onClose }: { onClose: () => void }) {
  const s = useStore();
  const go = (p: Page) => {
    s.setPage(p);
    onClose();
  };
  const items: { id: Page; label: string; sub: string; icon: React.ReactNode; count?: number }[] = [
    { id: "shares", label: "Shared folders", sub: "Folders others can download from you", icon: <FolderUp size={20} />, count: s.shares.length || undefined },
    { id: "history", label: "History", sub: "Calendar, totals and privacy receipts", icon: <CalendarDays size={20} /> },
    { id: "activity", label: "Activity", sub: "Everything that happened, newest first", icon: <ListChecks size={20} /> },
    { id: "settings", label: "Settings", sub: "Speed, privacy and this device", icon: <SettingsIcon size={20} /> },
  ];
  return (
    <Modal title="More" onClose={onClose}>
      <div className="more-list">
        {items.map((i) => (
          <button key={i.id} className={`more-item ${s.page === i.id ? "on" : ""}`} onClick={() => go(i.id)}>
            <span className="more-ico">{i.icon}</span>
            <span className="stack grow">
              <span className="more-label">{i.label}</span>
              <span className="small faint">{i.sub}</span>
            </span>
            {i.count != null && <span className="num small muted">{i.count}</span>}
            <ChevronRight size={18} className="faint" />
          </button>
        ))}
      </div>
      <div className="more-me">
        <span className={`dot ${s.network?.online ? "ok" : "warn"}`} />
        <span className="grow truncate"><b>{s.settings?.deviceName}</b> {s.network?.online ? "is online" : "is connecting…"}</span>
        <span className="small faint">{s.settings?.connectionMode === "direct_only" ? "Direct only" : "Relay fallback on"}</span>
      </div>
    </Modal>
  );
}

export default function App() {
  const s = useStore();
  const phone = usePhone();
  const [moreOpen, setMoreOpen] = useState(false);
  const jobs = useMemo(() => Object.values(s.jobs), [s.jobs]);
  const activeCount = jobs.filter((j) => isActive(j.status)).length;
  const attention = jobs.filter((j) => j.status === "needs_attention").length;
  const down = jobs.filter((j) => j.status === "downloading").reduce((a, j) => a + j.speedBps, 0);

  const nav: { id: Page; label: string; short: string; icon: React.ReactNode; count?: number; hot?: boolean; alert?: boolean }[] = [
    { id: "overview", label: "Overview", short: "Home", icon: <LayoutGrid size={17} /> },
    { id: "shares", label: "Shared folders", short: "Shared", icon: <FolderUp size={17} />, count: s.shares.length || undefined },
    { id: "devices", label: "Remote devices", short: "Devices", icon: <Monitor size={17} />, count: s.remotes.length || undefined },
    { id: "transfers", label: "Transfers", short: "Transfers", icon: <ArrowDownUp size={17} />, count: attention || activeCount || undefined, hot: activeCount > 0, alert: attention > 0 },
    { id: "history", label: "History", short: "History", icon: <CalendarDays size={17} /> },
    { id: "activity", label: "Activity", short: "Activity", icon: <ListChecks size={17} /> },
    { id: "settings", label: "Settings", short: "Settings", icon: <SettingsIcon size={17} /> },
  ];
  if (s.browse && !phone) nav.splice(3, 0, { id: "browse", label: s.browse.shareName, short: "Browse", icon: <FolderOpen size={17} /> });
  const shownNav = phone ? nav.filter((n) => PHONE_TABS.includes(n.id)) : nav;
  const isOn = (id: Page) => s.page === id || (phone && id === "devices" && s.page === "browse");
  const moreOn = phone && !PHONE_TABS.includes(s.page) && s.page !== "browse";

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
      case "history":
        return <History />;
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
          {shownNav.map((n) => (
            <button key={n.id} className={`nav-item nav-${n.id} ${isOn(n.id) ? "active" : ""}`} onClick={() => s.setPage(n.id)} aria-current={isOn(n.id) ? "page" : undefined}>
              {n.icon}
              <span className="truncate nav-long">{n.label}</span>
              <span className="nav-short">{n.short}</span>
              {n.count != null && <span className={`nav-count ${n.alert ? "alert" : n.hot ? "hot" : ""}`} title={n.alert ? "Needs attention" : undefined}>{n.count}</span>}
            </button>
          ))}
          {phone && (
            <button className={`nav-item nav-more ${moreOn ? "active" : ""}`} onClick={() => setMoreOpen(true)} aria-haspopup="dialog">
              <Menu size={17} />
              <span className="nav-short">More</span>
            </button>
          )}
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
      <main className={`main page-${s.page}`}>
        <header className="topbar">
          <h1>{s.page === "browse" && s.browse ? `${s.browse.shareName}` : phone && s.page === "overview" ? "Relayless" : TITLES[s.page]}</h1>
          {s.page === "browse" && s.browse && <span className="faint">on {s.browse.peerName}</span>}
          <span className="spacer" />
          {s.settings?.connectionMode === "direct_only" ? (
            <span className="pill pill-muted" title="File data only uses direct paths">Direct only</span>
          ) : (
            <span className="pill pill-warn" title="Relay fallback is allowed">Relay fallback on</span>
          )}
          <Button className={`topbar-connect ${phone && s.page === "overview" ? "hide-phone" : ""}`} variant="primary" icon={<KeyRound size={15} />} onClick={() => s.setConnectOpen(true)} aria-label="Connect with code">
            Connect
          </Button>
        </header>
        <div className="content">
          {s.ready ? page : null}
        </div>
      </main>

      {s.connectOpen && <ConnectDialog onClose={() => s.setConnectOpen(false)} />}
      {moreOpen && <MoreSheet onClose={() => setMoreOpen(false)} />}
      <AccessRequests />
      <FolderPickerHost />
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
