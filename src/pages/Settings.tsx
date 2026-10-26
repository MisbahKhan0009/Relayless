import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { disable, enable, isEnabled } from "@tauri-apps/plugin-autostart";
import { Copy, Info } from "lucide-react";
import { errorText, type Settings as S } from "../api";
import { mbit } from "../format";
import { useStore } from "../store";
import { Button, Segmented, Toggle } from "../ui";

function Row({ label, hint, children }: { label: string; hint?: React.ReactNode; children: React.ReactNode }) {
  return (
    <div className="setting">
      <div className="stack">
        <span className="label">{label}</span>
        {hint && <span className="hint">{hint}</span>}
      </div>
      <div>{children}</div>
    </div>
  );
}

function LimitInput({ value, onChange }: { value: number; onChange: (v: number) => void }) {
  const [text, setText] = useState(value ? String(Math.round(value / 1024)) : "");
  useEffect(() => setText(value ? String(Math.round(value / 1024)) : ""), [value]);
  return (
    <div className="hstack">
      <input
        className="input num"
        inputMode="numeric"
        placeholder="Unlimited"
        value={text}
        onChange={(e) => setText(e.target.value.replace(/[^0-9.]/g, ""))}
        onBlur={() => {
          const mb = parseFloat(text);
          onChange(Number.isFinite(mb) && mb > 0 ? Math.round(mb * 1024) : 0);
        }}
      />
      <span className="faint small" style={{ whiteSpace: "nowrap" }}>MB/s</span>
    </div>
  );
}

export default function Settings() {
  const { settings, saveSettings, toast, network, version, dataDir } = useStore();
  const [autostart, setAutostart] = useState<boolean | null>(null);
  const [restartNote, setRestartNote] = useState(false);

  useEffect(() => {
    isEnabled().then(setAutostart).catch(() => setAutostart(null));
  }, []);

  if (!settings) return null;
  const save = async (patch: Partial<S>) => {
    try {
      await saveSettings({ ...settings, ...patch });
    } catch (e) {
      toast(errorText(e), "err");
    }
  };
  const streamsCap = (settings.parallelStreams * 1_000_000) / 0.2; // bytes/s at 200 ms RTT

  return (
    <div className="page" style={{ maxWidth: 860 }}>
      <section>
        <div className="sub-title">General</div>
        <div className="panel settings-group">
          <Row label="Computer name" hint="Shown to devices you share with or connect to.">
            <input className="input" defaultValue={settings.deviceName} maxLength={64} onBlur={(e) => e.target.value.trim() && e.target.value !== settings.deviceName && save({ deviceName: e.target.value })} />
          </Row>
          <Row label="Default download folder">
            <div className="hstack">
              <input className="input mono" value={settings.downloadDir} readOnly />
              <Button
                onClick={async () => {
                  const p = await open({ directory: true, defaultPath: settings.downloadDir, title: "Default download folder" });
                  if (typeof p === "string") save({ downloadDir: p });
                }}
              >
                Change…
              </Button>
            </div>
          </Row>
          <Toggle
            checked={!!autostart}
            disabled={autostart === null}
            label="Start Relayless when I sign in"
            hint="Starts quietly in the tray so shared folders stay available and downloads resume after a reboot."
            onChange={async (v) => {
              try {
                if (v) await enable();
                else await disable();
                setAutostart(await isEnabled());
              } catch (e) {
                toast(errorText(e), "err");
              }
            }}
          />
          <Toggle
            checked={settings.closeToTray}
            label="Keep running in the tray when the window is closed"
            hint="Closing the window releases its memory; transfers continue. Quit from the tray icon."
            onChange={(v) => save({ closeToTray: v })}
          />
          <Toggle checked={settings.notifications} label="Notifications" hint="When a transfer finishes, needs attention, or a device asks for access." onChange={(v) => save({ notifications: v })} />
        </div>
      </section>

      <section>
        <div className="sub-title">Speed and resources</div>
        <div className="panel settings-group">
          <Row
            label={`Parallel streams per file: ${settings.parallelStreams}`}
            hint={<>More streams fill long, high-latency links (like Bangladesh to Europe). {settings.parallelStreams} streams can carry about {mbit(streamsCap)} at 200 ms latency.</>}
          >
            <input type="range" min={1} max={32} value={settings.parallelStreams} onChange={(e) => save({ parallelStreams: Number(e.target.value) })} aria-label="Parallel streams" />
          </Row>
          <Row label="Files downloading at once">
            <Segmented label="Concurrent transfers" value={settings.maxConcurrentJobs} onChange={(v) => save({ maxConcurrentJobs: v })} options={[1, 2, 3, 4].map((n) => ({ value: n, label: String(n) }))} />
          </Row>
          <Row label="Download limit" hint="Leave empty for full speed.">
            <LimitInput value={settings.downloadLimitKbps} onChange={(v) => save({ downloadLimitKbps: v })} />
          </Row>
          <Row label="Upload limit" hint="Caps how fast this computer sends, to keep the connection usable for others at home.">
            <LimitInput value={settings.uploadLimitKbps} onChange={(v) => save({ uploadLimitKbps: v })} />
          </Row>
          <Toggle
            checked={settings.lowPriority}
            label="Yield CPU to other apps"
            hint="Runs Relayless at below-normal priority. Network speed is unaffected; your other programs stay responsive."
            onChange={(v) => save({ lowPriority: v })}
          />
          <Row label="Congestion control" hint="BBR keeps speed high on long or lossy links. CUBIC is the conservative standard. Applies after restarting Relayless.">
            <Segmented
              label="Congestion control"
              value={settings.congestion}
              onChange={(v) => {
                save({ congestion: v });
                setRestartNote(true);
              }}
              options={[
                { value: "bbr", label: "BBR (fast)" },
                { value: "cubic", label: "CUBIC" },
              ]}
            />
          </Row>
          <Row label="Piece size for new downloads" hint="Larger pieces mean less overhead; smaller pieces lose less on each interruption.">
            <Segmented label="Piece size" value={settings.chunkSizeMib} onChange={(v) => save({ chunkSizeMib: v })} options={[8, 16, 32].map((n) => ({ value: n, label: `${n} MB` }))} />
          </Row>
        </div>
        {restartNote && (
          <div className="notice" style={{ marginTop: 10 }}>
            <Info size={16} />
            <span>Quit Relayless from the tray and open it again to use the new congestion control.</span>
          </div>
        )}
      </section>

      <section>
        <div className="sub-title">Connection</div>
        <div className="panel settings-group">
          <Row
            label="Route mode"
            hint={
              settings.connectionMode === "direct_only"
                ? "File data only flows over a verified direct path. If routers block it, transfers wait instead of using a relay."
                : "Prefer direct. If no direct path opens, encrypted data may pass through a public relay (slower, but works behind strict NAT)."
            }
          >
            <Segmented
              label="Route mode"
              value={settings.connectionMode}
              onChange={(v) => save({ connectionMode: v })}
              options={[
                { value: "direct_only", label: "Direct only" },
                { value: "allow_relay", label: "Allow relay fallback" },
              ]}
            />
          </Row>
        </div>
      </section>

      <section>
        <div className="sub-title">About</div>
        <div className="panel settings-group">
          <Row label="Device ID" hint="Your cryptographic identity. Others verify it on every connection.">
            <div className="hstack">
              <span className="mono small truncate selectable">{network?.endpointId}</span>
              <Button size="sm" icon={<Copy size={13} />} onClick={() => network && navigator.clipboard.writeText(network.endpointId)} />
            </div>
          </Row>
          <Row label="Network">
            <span className="muted small">
              {network?.online ? "Online" : "Offline"}
              {network?.relayUrl ? ` · coordination via ${new URL(network.relayUrl).hostname}` : ""}
              {network ? ` · ${network.directAddrs} local address(es)` : ""}
            </span>
          </Row>
          <Row label="Version">
            <span className="muted small">Relayless {version}</span>
          </Row>
          <Row label="App data">
            <span className="mono small faint selectable" style={{ overflowWrap: "anywhere" }}>{dataDir}</span>
          </Row>
        </div>
      </section>
    </div>
  );
}
