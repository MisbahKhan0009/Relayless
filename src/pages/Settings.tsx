import { useEffect, useState } from "react";
import { disable, enable, isEnabled } from "@tauri-apps/plugin-autostart";
import { BatteryCharging, Bell, Copy, FolderLock, Info, Lock } from "lucide-react";
import { android, api, errorText, type Settings as S } from "../api";
import { isAndroid, pickFolder } from "../platform";
import { useStorageAccess } from "../android";
import { mbit } from "../format";
import { useStore } from "../store";
import { Button, Modal, Segmented, Toggle } from "../ui";

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

// `value` is KiB/s (0 = unlimited); shown as MB/s. Keep three decimals so a
// small limit (e.g. 50 KiB/s = 0.049) never displays as "0".
const kibToMbText = (kib: number) => (kib ? String(Number((kib / 1024).toFixed(3))) : "");

function LimitInput({ value, onChange }: { value: number; onChange: (v: number) => void }) {
  const [text, setText] = useState(kibToMbText(value));
  useEffect(() => setText(kibToMbText(value)), [value]);
  return (
    <div className="hstack">
      <input
        className="input num"
        inputMode="numeric"
        placeholder="Unlimited"
        value={text}
        onChange={(e) => setText(e.target.value.replace(/[^0-9.]/g, ""))}
        onBlur={() => {
          // Leaving the field untouched must not re-save a rounded value.
          if (text === kibToMbText(value)) return;
          const mb = parseFloat(text);
          onChange(Number.isFinite(mb) && mb > 0 ? Math.round(mb * 1024) : 0);
        }}
      />
      <span className="faint small" style={{ whiteSpace: "nowrap" }}>MB/s</span>
    </div>
  );
}

export default function Settings() {
  const { settings, saveSettings, toast, network, version, dataDir, history } = useStore();
  const [confirmClear, setConfirmClear] = useState<null | "clear" | "off">(null);
  const [autostart, setAutostart] = useState<boolean | null>(null);
  const [restartNote, setRestartNote] = useState(false);
  const [relayNote, setRelayNote] = useState(false);

  const storage = useStorageAccess();
  const [battery, setBattery] = useState<boolean | null>(null);
  useEffect(() => {
    if (isAndroid()) {
      const check = () => android.batteryStatus().then((b) => setBattery(b.unrestricted)).catch(() => {});
      check();
      window.addEventListener("focus", check);
      document.addEventListener("visibilitychange", check);
      return () => {
        window.removeEventListener("focus", check);
        document.removeEventListener("visibilitychange", check);
      };
    }
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
    <div className="page page-settings">
      <section>
        <div className="sub-title">General</div>
        <div className="panel settings-group">
          <Row label={isAndroid() ? "Device name" : "Computer name"} hint="Shown to devices you share with or connect to.">
            <input className="input" defaultValue={settings.deviceName} maxLength={64} onBlur={(e) => e.target.value.trim() && e.target.value !== settings.deviceName && save({ deviceName: e.target.value })} />
          </Row>
          <Row label="Default download folder">
            <div className="hstack">
              <input className="input mono" value={settings.downloadDir} readOnly />
              <Button
                onClick={async () => {
                  const p = await pickFolder("Default download folder", settings.downloadDir);
                  if (p) save({ downloadDir: p });
                }}
              >
                Change…
              </Button>
            </div>
          </Row>
          {!isAndroid() && (<>
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
          </>)}
          <Toggle checked={settings.notifications} label="Notifications" hint="When a transfer finishes, needs attention, or a device asks for access." onChange={(v) => save({ notifications: v })} />
        </div>
      </section>

      {isAndroid() && (
        <section>
          <div className="sub-title">Android</div>
          <div className="panel settings-group">
            <Row label="All files access" hint="Needed to share folders and save downloads to real folders such as Download/Relayless.">
              {storage.granted ? (
                <span className="pill pill-ok"><FolderLock size={12} /> Allowed</span>
              ) : (
                <Button variant="primary" onClick={storage.request}>Allow</Button>
              )}
            </Row>
            <Row label="Run in the background" hint="Lets transfers continue with the screen off. Tap Allow, pick Relayless in the list and choose “Don't optimize” or “Unrestricted”.">
              {battery ? (
                <span className="pill pill-ok"><BatteryCharging size={12} /> Unrestricted</span>
              ) : (
                <Button onClick={() => android.requestIgnoreBatteryOptimizations().then((b) => setBattery(b.unrestricted)).catch(() => {})}>Allow</Button>
              )}
            </Row>
            <Row label="Notification permission" hint="Shows transfer progress and access requests.">
              <Button icon={<Bell size={14} />} onClick={() => android.requestNotificationPermission().catch(() => {})}>Request</Button>
            </Row>
          </div>
        </section>
      )}

      <section>
        <div className="sub-title">Speed and resources</div>
        <div className="panel settings-group">
          <Row
            label={`Parallel streams per file: ${settings.parallelStreams}`}
            hint={<>More streams fill long, high-latency links (like Bangladesh to Europe). {settings.parallelStreams} streams can carry about {mbit(streamsCap)} at 200 ms latency.</>}
          >
            <input type="range" min={1} max={32} value={settings.parallelStreams} onChange={(e) => save({ parallelStreams: Number(e.target.value) })} aria-label="Parallel streams" />
          </Row>
          <Row
            label="Connections per sender"
            hint="On long or lossy direct links, several connections carry more than one. Relayless adds them only while each one raises the speed, and never over a relay."
          >
            <Segmented
              label="Connections per sender"
              value={settings.maxConnections ?? 8}
              onChange={(v) => save({ maxConnections: v })}
              options={[
                { value: 1, label: "1 (off)" },
                { value: 4, label: "Up to 4" },
                { value: 8, label: "Up to 8" },
              ]}
            />
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
            disabled={isAndroid()}
            label="Yield CPU to other apps"
            hint="Runs Relayless at below-normal priority. Network speed is unaffected; your other programs stay responsive."
            onChange={(v) => save({ lowPriority: v })}
          />
          <Row label="Congestion control" hint="CUBIC gives the steadiest speed and is recommended. BBR (experimental) can help on very lossy links but sometimes stalls at a few KB/s. Matters most on the sending computer. Applies after restarting Relayless.">
            <Segmented
              label="Congestion control"
              value={settings.congestion}
              onChange={(v) => {
                save({ congestion: v });
                setRestartNote(true);
              }}
              options={[
                { value: "cubic", label: "CUBIC (recommended)" },
                { value: "bbr", label: "BBR (experimental)" },
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
                ? "File data only flows over a direct path. If routers block one (common on mobile data), transfers wait at 0 instead of using a relay."
                : "Tries a direct path first. If none opens within about 5 seconds, data goes through a public relay, still end-to-end encrypted: the relay can't read or keep it. Switches to direct as soon as possible."
            }
          >
            <Segmented
              label="Route mode"
              value={settings.connectionMode}
              onChange={(v) => save({ connectionMode: v })}
              options={[
                { value: "allow_relay", label: "Direct first (recommended)" },
                { value: "direct_only", label: "Direct only" },
              ]}
            />
          </Row>
          <Row
            label="Relay server"
            hint={
              <>
                Leave empty to use the free public relays, which are rate-limited (often around 1 Mbit/s). For fast transfers when routers block a
                direct path, run your own relay and enter its address on both devices. Setup guide: docs/RELAY.md on the Relayless GitHub page.
              </>
            }
          >
            <input
              className="input mono"
              placeholder="relay.example.org"
              defaultValue={settings.relayUrls}
              spellCheck={false}
              onBlur={(e) => {
                const v = e.target.value.trim();
                if (v !== settings.relayUrls) {
                  save({ relayUrls: v });
                  setRelayNote(true);
                }
              }}
            />
          </Row>
        </div>
        {relayNote && (
          <div className="notice" style={{ marginTop: 10 }}>
            <Info size={16} />
            <span>Quit Relayless and open it again to use the new relay server. Set the same relay on the other device too.</span>
          </div>
        )}
      </section>

      <section>
        <div className="sub-title">Privacy</div>
        <div className="panel settings-group">
          <div className="privacy-facts">
            <Lock size={16} aria-hidden="true" />
            <span>
              Relayless has no account and sends no analytics. Transfer history, receipts and these settings are stored only in this device's
              database. The coordination service sees that two devices want to connect, never file names or contents.
            </span>
          </div>
          <Row label="Keep history" hint="Finished transfers and their privacy receipts, for the History page. Older entries are deleted automatically.">
            <Segmented
              label="History retention"
              value={settings.historyRetention}
              onChange={(v) => (v === "off" && history.length > 0 ? setConfirmClear("off") : save({ historyRetention: v }))}
              options={[
                { value: "365", label: "1 year" },
                { value: "90", label: "90 days" },
                { value: "30", label: "30 days" },
                { value: "forever", label: "Forever" },
                { value: "off", label: "Off" },
              ]}
            />
          </Row>
          <Toggle
            checked={settings.historyNames}
            disabled={settings.historyRetention === "off"}
            label="Keep file names in history"
            hint="When off, new entries keep only the file type, size, device and receipt."
            onChange={(v) => save({ historyNames: v })}
          />
          <Row label="Clear history" hint={history.length ? `${history.length.toLocaleString()} finished transfer${history.length === 1 ? "" : "s"} on this device.` : "Nothing stored right now."}>
            <Button variant="danger" disabled={history.length === 0} onClick={() => setConfirmClear("clear")}>Clear history</Button>
          </Row>
        </div>
        {confirmClear && (
          <Modal
            title={confirmClear === "off" ? "Turn off history?" : "Clear history?"}
            onClose={() => setConfirmClear(null)}
            footer={
              <>
                <Button variant="ghost" onClick={() => setConfirmClear(null)}>Keep it</Button>
                <Button
                  variant="danger"
                  onClick={async () => {
                    const mode = confirmClear;
                    setConfirmClear(null);
                    try {
                      if (mode === "off") await save({ historyRetention: "off" });
                      else await api.clearHistory();
                      toast(mode === "off" ? "History is off and was deleted" : "History cleared", "ok");
                    } catch (e) {
                      toast(errorText(e), "err");
                    }
                  }}
                >
                  {confirmClear === "off" ? "Turn off and delete" : "Delete history"}
                </Button>
              </>
            }
          >
            <span>
              This deletes {history.length.toLocaleString()} receipt{history.length === 1 ? "" : "s"} and the calendar from this device. Downloaded files are not touched.
              {confirmClear === "off" && " New transfers won't be recorded until you turn history back on."}
            </span>
          </Modal>
        )}
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
