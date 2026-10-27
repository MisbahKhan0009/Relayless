import { useEffect, useState } from "react";
import {
  Check,
  Copy,
  FolderOpen,
  KeyRound,
  Link2,
  Lock,
  Monitor,
  RefreshCw,
  ShieldCheck,
  Waypoints,
  Wifi,
  Zap,
} from "lucide-react";
import { android, api, errorText, on, type CreatedInvitation, type JoinResult, type Share } from "./api";
import { isAndroid } from "./platform";
import { until } from "./format";
import { useStore } from "./store";
import { Button, Modal, SafetyWords, Segmented, Toggle } from "./ui";

function CopyButton({ text, label = "Copy" }: { text: string; label?: string }) {
  const [done, setDone] = useState(false);
  return (
    <Button
      size="sm"
      variant="secondary"
      icon={done ? <Check size={14} /> : <Copy size={14} />}
      onClick={async () => {
        await navigator.clipboard.writeText(text);
        setDone(true);
        window.setTimeout(() => setDone(false), 1600);
      }}
    >
      {done ? "Copied" : label}
    </Button>
  );
}

// ---------------------------------------------------------------- invite

export function InviteDialog({ share, onClose }: { share: Share; onClose: () => void }) {
  const { toast } = useStore();
  const [expires, setExpires] = useState(86400);
  const [uses, setUses] = useState<number>(1);
  const [approval, setApproval] = useState(true);
  const [withCode, setWithCode] = useState(true);
  const [busy, setBusy] = useState(false);
  const [created, setCreated] = useState<CreatedInvitation | null>(null);
  const [, tick] = useState(0);

  useEffect(() => {
    if (!created) return;
    const t = window.setInterval(() => tick((x) => x + 1), 1000);
    return () => window.clearInterval(t);
  }, [created]);

  const create = async () => {
    setBusy(true);
    try {
      setCreated(await api.createInvitation(share.id, expires, uses === 0 ? null : uses, approval, withCode));
    } catch (e) {
      toast(errorText(e), "err");
    } finally {
      setBusy(false);
    }
  };

  if (created) {
    return (
      <Modal title={`Invite to “${share.name}”`} onClose={onClose} width={620} footer={<Button variant="primary" onClick={onClose}>Done</Button>}>
        <div className="hstack invite-layout" style={{ alignItems: "flex-start", gap: 18 }}>
          <div className="qr" dangerouslySetInnerHTML={{ __html: created.qrSvg }} aria-label="Invitation QR code" />
          <div className="stack grow" style={{ gap: 14 }}>
            {created.shortCode && (
              <div className="stack">
                <span className="label">Short code</span>
                <div className="hstack">
                  <span className="short-code">{created.shortCode}</span>
                  <CopyButton text={created.shortCode} />
                </div>
                <span className="hint">
                  Easy to read out over the phone. Works {created.shortCodeExpiresAt ? until(created.shortCodeExpiresAt) : "for 30 minutes"}, only while Relayless stays open here.
                </span>
              </div>
            )}
            <div className="stack">
              <span className="label">Invitation</span>
              <div className="code-box">
                <span className="val selectable">{created.invite}</span>
                <CopyButton text={created.invite} />
              </div>
              <span className="hint">
                Send this in any chat app. It contains this computer's identity and a one-time access key, never your files.
              </span>
            </div>
          </div>
        </div>
        <div className="notice accent">
          <Monitor size={16} />
          <span>
            <b>Keep this computer on and Relayless running</b> while they download. If either side goes offline, the
            transfer pauses and continues from where it stopped.
            {approval && " You'll be asked to approve their device when they connect."}
          </span>
        </div>
      </Modal>
    );
  }

  return (
    <Modal
      title={`Invite someone to “${share.name}”`}
      onClose={onClose}
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>Cancel</Button>
          <Button variant="primary" loading={busy} onClick={create} icon={<Link2 size={15} />}>
            Create invitation
          </Button>
        </>
      }
    >
      <div className="field">
        <span className="label">Invitation expires</span>
        <Segmented
          label="Expiry"
          value={expires}
          onChange={setExpires}
          options={[
            { value: 3600, label: "1 hour" },
            { value: 86400, label: "24 hours" },
            { value: 7 * 86400, label: "7 days" },
          ]}
        />
      </div>
      <div className="field">
        <span className="label">Can be used by</span>
        <Segmented
          label="Uses"
          value={uses}
          onChange={setUses}
          options={[
            { value: 1, label: "1 device" },
            { value: 5, label: "Up to 5" },
            { value: 0, label: "Unlimited" },
          ]}
        />
        <span className="hint">Once a device has joined, it keeps access until you remove it, even after the invitation expires.</span>
      </div>
      <div>
        <Toggle checked={approval} onChange={setApproval} label="Ask me to approve each device" hint="Recommended. You'll see the device name before it can browse anything." />
        <Toggle checked={withCode} onChange={setWithCode} label="Also create a short code" hint="A 12-character code they can type instead of pasting the invitation. Valid 30 minutes." />
      </div>
      <div className="notice">
        <Lock size={16} />
        <span>They can only see and download from <b>{share.name}</b>. Nothing outside this folder is reachable, and they can't change or delete anything.</span>
      </div>
    </Modal>
  );
}

// ---------------------------------------------------------------- connect

export function ConnectDialog({ onClose }: { onClose: () => void }) {
  const { openBrowser, toast } = useStore();
  const [input, setInput] = useState("");
  const [busy, setBusy] = useState(false);
  const [status, setStatus] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<JoinResult | null>(null);
  const [words, setWords] = useState<string[] | null>(null);

  useEffect(() => {
    const u = on<{ message: string; safetyWords?: string[] | null }>("join-status", (p) => {
      setStatus(p.message);
      if (p.safetyWords?.length) setWords(p.safetyWords);
    });
    return () => {
      u.then((f) => f());
    };
  }, []);

  const join = async () => {
    setBusy(true);
    setError(null);
    setStatus("Connecting…");
    try {
      const r = await api.join(input);
      setResult(r);
      toast(`Paired with ${r.senderName}`, "ok");
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
      setStatus(null);
    }
  };

  if (result) {
    return (
      <Modal title={`Connected to ${result.senderName}`} onClose={onClose}>
        <div className="notice accent">
          <ShieldCheck size={16} />
          <span>This device is now trusted by <b>{result.senderName}</b>. You can come back any time without a new invitation.</span>
        </div>
        {result.safetyWords?.length > 0 && (
          <div className="words-block">
            <span className="label">Safety words</span>
            <SafetyWords words={result.safetyWords} />
            <span className="hint">{result.senderName} sees the same five words for this connection. If they differ, someone is in between: forget the device and pair again.</span>
          </div>
        )}
        <div className="stack" style={{ gap: 8 }}>
          <span className="label">Shared with you</span>
          {result.shares.map((s) => (
            <button
              key={s.id}
              className="hero-tile"
              onClick={() => {
                openBrowser({ peerId: result.peerId, shareId: s.id, shareName: s.name, peerName: result.senderName });
                onClose();
              }}
            >
              <span className="ico" style={{ width: 38, height: 38 }}><FolderOpen size={18} /></span>
              <span className="stack grow">
                <span className="t" style={{ fontSize: 14 }}>{s.name}</span>
                <span className="d">Browse and choose files</span>
              </span>
            </button>
          ))}
        </div>
      </Modal>
    );
  }

  return (
    <Modal
      title="Connect with a code"
      onClose={onClose}
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>Cancel</Button>
          <Button variant="primary" loading={busy} disabled={!input.trim()} onClick={join} icon={<KeyRound size={15} />}>
            Connect
          </Button>
        </>
      }
    >
      <div className="field">
        <label htmlFor="inv">Invitation or short code</label>
        <textarea
          id="inv"
          className="input mono"
          rows={4}
          placeholder="Paste the invitation (RL1-…) or type a code like K7QM-9XRT-4HWD"
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey && input.trim() && !busy) {
              e.preventDefault();
              join();
            }
          }}
          spellCheck={false}
        />
      </div>
      {busy && status && (
        <div className="notice accent">
          <RefreshCw size={16} className="spin" />
          <span>{status}</span>
        </div>
      )}
      {busy && words && (
        <div className="words-block">
          <span className="label">Safety words</span>
          <SafetyWords words={words} />
          <span className="hint">Read these to the sender. They should see the same five words before they approve this device.</span>
        </div>
      )}
      {error && (
        <div className="notice err">
          <Wifi size={16} />
          <span>{error}</span>
        </div>
      )}
      {!busy && !error && (
        <span className="hint">
          Your computer verifies the sender's identity cryptographically. Files travel directly between the two computers, end-to-end encrypted.
        </span>
      )}
    </Modal>
  );
}

// ---------------------------------------------------------------- approvals

export function AccessRequests() {
  const { pending, toast } = useStore();
  const req = pending[0];
  if (!req) return null;
  const respond = async (approve: boolean) => {
    try {
      await api.respondAccess(req.id, approve);
      toast(approve ? `${req.deviceName} can now open “${req.shareName}”` : "Request declined", approve ? "ok" : "info");
    } catch (e) {
      toast(errorText(e), "err");
    }
  };
  return (
    <Modal
      title="A device wants access"
      onClose={() => respond(false)}
      footer={
        <>
          <Button variant="ghost" onClick={() => respond(false)}>Decline</Button>
          <Button variant="primary" onClick={() => respond(true)} icon={<Check size={15} />}>Allow access</Button>
        </>
      }
    >
      <div className="hstack" style={{ gap: 14 }}>
        <div className="share-ico"><Monitor size={20} /></div>
        <div className="stack">
          <b style={{ fontSize: 15 }}>{req.deviceName}</b>
          <span className="faint small mono">Device ID {req.peerShort}…</span>
        </div>
      </div>
      <span>
        used your invitation and wants to browse and download from <b>“{req.shareName}”</b>. Only allow devices you expect.
      </span>
      {req.safetyWords?.length > 0 && (
        <div className="words-block">
          <span className="label">Safety words</span>
          <SafetyWords words={req.safetyWords} />
          <span className="hint">Ask them which words their screen shows. Allow only if all five match; different words mean someone else is answering.</span>
        </div>
      )}
      {pending.length > 1 && <span className="hint">{pending.length - 1} more request(s) waiting.</span>}
    </Modal>
  );
}

// ---------------------------------------------------------------- onboarding

export function Onboarding() {
  const { settings, saveSettings } = useStore();
  const [name, setName] = useState(settings?.deviceName ?? "");
  if (!settings || settings.onboarded) return null;
  const done = () => {
    if (isAndroid()) {
      android.requestNotificationPermission().catch(() => {});
    }
    return saveSettings({ ...settings, deviceName: name.trim() || settings.deviceName, onboarded: true });
  };
  return (
    <Modal title="Welcome to Relayless" onClose={done} width={560} footer={<Button variant="primary" onClick={done}>Get started</Button>}>
      <div className="onboard-points">
        <div className="onboard-point">
          <span className="i"><Zap size={16} /></span>
          <span><b>Your files never sit on a server</b><span className="muted">Files go straight from the sender's device to yours, end-to-end encrypted. No account, no upload, no cloud copy.</span></span>
        </div>
        <div className="onboard-point">
          <span className="i"><Waypoints size={16} /></span>
          <span><b>A small coordination service helps you find each other</b><span className="muted">Public iroh relays and DNS exchange connection details so the two devices can punch through routers. If a router blocks a direct path, a relay forwards the end-to-end encrypted data; it can't read or keep it, and the receipt shows how much went that way. You can choose direct only in Settings.</span></span>
        </div>
        <div className="onboard-point">
          <span className="i"><ShieldCheck size={16} /></span>
          <span><b>Proof stays with you</b><span className="muted">Every finished transfer gets a privacy receipt: the route it took, how it was verified, and what servers kept (nothing). Your history never leaves this device.</span></span>
        </div>
        <div className="onboard-point">
          <span className="i"><RefreshCw size={16} /></span>
          <span><b>Interruptions don't lose progress</b><span className="muted">Both devices need to be online while a transfer runs. If either drops, restarts or reboots, the download continues from its last verified piece.{isAndroid() ? " On Android a notification shows while files are moving so the transfer keeps running with the screen off." : ""}</span></span>
        </div>
      </div>
      <div className="field">
        <label htmlFor="dn">{isAndroid() ? "This device's name" : "This computer's name"}</label>
        <input id="dn" className="input" value={name} onChange={(e) => setName(e.target.value)} maxLength={64} />
        <span className="hint">Shown to people you share with or connect to.</span>
      </div>
    </Modal>
  );
}
