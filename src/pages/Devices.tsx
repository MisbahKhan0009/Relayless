import { useState } from "react";
import { FolderOpen, KeyRound, Monitor, PlugZap, Trash2, Waypoints, Zap } from "lucide-react";
import { api, errorText, type Remote } from "../api";
import { ago } from "../format";
import { useStore } from "../store";
import { Button, Empty, IconButton, Modal, RouteBadge } from "../ui";

function RemoteCard({ r }: { r: Remote }) {
  const { openBrowser, toast } = useStore();
  const [busy, setBusy] = useState(false);
  const [confirm, setConfirm] = useState(false);
  const connected = r.status.state === "connected";
  const connect = async () => {
    setBusy(true);
    try {
      await api.connectRemote(r.peerId);
    } catch (e) {
      toast(errorText(e), "err");
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className="panel share-card">
      <div className="share-head">
        <div className={`share-ico ${connected ? "" : "off"}`}><Monitor size={20} /></div>
        <div className="stack grow">
          <div className="hstack">
            <b style={{ fontSize: 15 }} className="truncate">{r.name}</b>
            <RouteBadge route={r.status.route} connected={connected} />
          </div>
          <span className="small faint">
            {r.status.state === "connecting"
              ? "Connecting…"
              : r.status.state === "unreachable" || r.status.state === "denied"
                ? r.status.message ?? "Not reachable"
                : `Paired ${ago(r.createdAt)} · last seen ${ago(r.lastSeenAt)}`}
          </span>
        </div>
        {!connected && (
          <Button icon={<PlugZap size={15} />} loading={busy} onClick={connect}>Connect</Button>
        )}
        <IconButton label="Forget this device" onClick={() => setConfirm(true)}>
          <Trash2 size={16} />
        </IconButton>
      </div>
      {r.shares.length > 0 ? (
        <div className="sub-list">
          {r.shares.map((s) => (
            <div className="row" key={s.id}>
              <FolderOpen size={15} className="faint" />
              <span className="grow truncate">{s.name}</span>
              <Button size="sm" variant="secondary" icon={<FolderOpen size={14} />} onClick={() => openBrowser({ peerId: r.peerId, shareId: s.id, shareName: s.name, peerName: r.name })}>
                Browse
              </Button>
            </div>
          ))}
        </div>
      ) : (
        <span className="small faint">No folders are shared with you right now.</span>
      )}
      {confirm && (
        <Modal
          title={`Forget ${r.name}?`}
          onClose={() => setConfirm(false)}
          footer={
            <>
              <Button variant="ghost" onClick={() => setConfirm(false)}>Keep</Button>
              <Button variant="danger" onClick={() => { setConfirm(false); api.removeRemote(r.peerId).catch((e) => toast(errorText(e), "err")); }}>Forget device</Button>
            </>
          }
        >
          <span>You'll need a new invitation to connect again. Unfinished downloads from this device stay in your transfer list.</span>
        </Modal>
      )}
    </div>
  );
}

export default function Devices() {
  const { remotes, setConnectOpen, settings } = useStore();
  return (
    <div className="page">
      <div className="hstack">
        <span className="muted grow">Computers that have shared folders with you. Connections are end-to-end encrypted and verified against each device's identity.</span>
        <Button variant="primary" icon={<KeyRound size={15} />} onClick={() => setConnectOpen(true)}>Connect with code</Button>
      </div>
      {remotes.length === 0 ? (
        <div className="panel">
          <Empty icon={<Monitor size={20} />} title="No remote devices">
            Ask the sender to share a folder in Relayless and send you an invitation or short code.
          </Empty>
        </div>
      ) : (
        remotes.map((r) => <RemoteCard key={r.peerId} r={r} />)
      )}
      <div className="two-col">
        <div className="notice">
          <Zap size={16} style={{ color: "var(--ok)" }} />
          <span><b>Direct</b> means data flows straight between the two computers. This is the fastest path and the only one used in direct-only mode.</span>
        </div>
        <div className="notice">
          <Waypoints size={16} style={{ color: "var(--warn)" }} />
          <span>
            <b>Relayed</b> means a public relay forwards encrypted packets because routers blocked a direct path.{" "}
            {settings?.connectionMode === "direct_only" ? "Direct-only mode is on, so file data waits for a direct path." : "Relay fallback is allowed in your settings."}
          </span>
        </div>
      </div>
    </div>
  );
}
