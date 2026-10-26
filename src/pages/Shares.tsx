import { useCallback, useEffect, useState } from "react";

/** Share created via the folder picker that should open its invite dialog next. */
let pendingInvite: string | null = null;
import { open } from "@tauri-apps/plugin-dialog";
import { Ban, FolderOpen, FolderUp, Link2, Monitor, Pause, Play, Trash2, UserX } from "lucide-react";
import { api, errorText, type Share } from "../api";
import { InviteDialog } from "../dialogs";
import { ago, speed, until } from "../format";
import { useStore } from "../store";
import { Button, Empty, IconButton, Modal, Pill, RouteBadge } from "../ui";

/** Opens the native folder picker and creates a share; returns the new share. */
export function useShareFolder() {
  const { toast, setPage, refreshShares } = useStore();
  return useCallback(async () => {
    try {
      const picked = await open({ directory: true, multiple: false, title: "Choose a folder to share" });
      if (!picked || Array.isArray(picked)) return;
      const share = await api.createShare(picked);
      refreshShares();
      setPage("shares");
      toast(`Sharing “${share.name}”. Create an invitation to let someone in.`, "ok");
      pendingInvite = share.id;
      window.dispatchEvent(new Event("relayless:invite"));
    } catch (e) {
      toast(errorText(e), "err");
    }
  }, [toast, setPage, refreshShares]);
}

function ShareCard({ share, onInvite }: { share: Share; onInvite: () => void }) {
  const { grants, uploads, toast } = useStore();
  const [confirmDelete, setConfirmDelete] = useState(false);
  const devices = grants.filter((g) => g.shareId === share.id);
  const invites = share.invitations.filter((i) => i.state === "active");
  const act = async (f: () => Promise<void>) => {
    try {
      await f();
    } catch (e) {
      toast(errorText(e), "err");
    }
  };

  return (
    <div className="panel share-card">
      <div className="share-head">
        <div className={`share-ico ${share.active ? "" : "off"}`}><FolderOpen size={20} /></div>
        <div className="stack grow">
          <div className="hstack">
            <b style={{ fontSize: 15 }} className="truncate">{share.name}</b>
            {share.active ? <Pill tone="ok">Sharing</Pill> : <Pill tone="muted">Paused</Pill>}
          </div>
          <span className="small faint mono truncate selectable" title={share.rootPath}>{share.rootPath}</span>
        </div>
        <Button variant="primary" icon={<Link2 size={15} />} onClick={onInvite} disabled={!share.active}>Invite</Button>
        <IconButton label={share.active ? "Pause sharing" : "Resume sharing"} onClick={() => act(() => api.setShareActive(share.id, !share.active))}>
          {share.active ? <Pause size={16} /> : <Play size={16} />}
        </IconButton>
        <IconButton label="Stop sharing and remove" onClick={() => setConfirmDelete(true)}>
          <Trash2 size={16} />
        </IconButton>
      </div>

      {devices.length > 0 && (
        <div>
          <div className="sub-title">Devices with access</div>
          <div className="sub-list">
            {devices.map((g) => {
              const up = uploads.find((u) => u.peerId === g.peerId);
              return (
                <div className="row" key={g.peerId}>
                  <Monitor size={15} className="faint" />
                  <div className="stack grow">
                    <span className="truncate"><b>{g.deviceName}</b> <span className="faint small mono">{g.peerShort}</span></span>
                    <span className="small faint truncate">
                      {up && up.bps > 0 ? `Downloading ${up.files.join(", ")} at ${speed(up.bps)}` : g.connected ? "Connected" : `Last seen ${ago(g.lastSeenAt)}`}
                    </span>
                  </div>
                  <RouteBadge route={g.route} connected={g.connected} />
                  <IconButton label="Remove access" onClick={() => act(() => api.revokeDevice(g.peerId))}>
                    <UserX size={15} />
                  </IconButton>
                </div>
              );
            })}
          </div>
        </div>
      )}

      {invites.length > 0 && (
        <div>
          <div className="sub-title">Open invitations</div>
          <div className="sub-list">
            {invites.map((i) => (
              <div className="row" key={i.id}>
                <Link2 size={15} className="faint" />
                <div className="stack grow">
                  <span className="small">
                    Expires {until(i.expiresAt)} · used {i.useCount}
                    {i.maxUses != null ? ` of ${i.maxUses}` : ""} · {i.requireApproval ? "needs your approval" : "no approval needed"}
                    {i.hasShortCode ? " · short code active" : ""}
                  </span>
                </div>
                <Button size="sm" variant="ghost" icon={<Ban size={14} />} onClick={() => act(() => api.revokeInvitation(i.id))}>
                  Revoke
                </Button>
              </div>
            ))}
          </div>
        </div>
      )}

      {confirmDelete && (
        <Modal
          title={`Stop sharing “${share.name}”?`}
          onClose={() => setConfirmDelete(false)}
          footer={
            <>
              <Button variant="ghost" onClick={() => setConfirmDelete(false)}>Keep sharing</Button>
              <Button variant="danger" onClick={() => { setConfirmDelete(false); act(() => api.deleteShare(share.id)); }}>Stop and remove</Button>
            </>
          }
        >
          <span>All invitations and device access for this folder are removed immediately. Active downloads from it stop. Your files are not touched.</span>
        </Modal>
      )}
    </div>
  );
}

export default function Shares() {
  const { shares } = useStore();
  const shareFolder = useShareFolder();
  const [inviting, setInviting] = useState<Share | null>(null);

  // A newly created share opens the invite dialog straight away.
  useEffect(() => {
    const check = () => {
      const id = pendingInvite;
      if (!id) return;
      pendingInvite = null;
      api.listShares().then((list) => {
        const s = list.find((x) => x.id === id);
        if (s) setInviting(s);
      });
    };
    check();
    window.addEventListener("relayless:invite", check);
    return () => window.removeEventListener("relayless:invite", check);
  }, []);

  return (
    <div className="page">
      <div className="hstack">
        <span className="muted grow">
          People you invite can browse and download from these folders only. They can't see anything else on your computer or change your files.
        </span>
        <Button variant="primary" icon={<FolderUp size={15} />} onClick={shareFolder}>Share a folder</Button>
      </div>
      {shares.length === 0 ? (
        <div className="panel">
          <Empty icon={<FolderUp size={20} />} title="You're not sharing anything">
            Choose a specific folder to share. Relayless never shares a whole drive.
          </Empty>
        </div>
      ) : (
        shares.map((s) => <ShareCard key={s.id} share={s} onInvite={() => setInviting(s)} />)
      )}
      {inviting && <InviteDialog share={inviting} onClose={() => setInviting(null)} />}
    </div>
  );
}
