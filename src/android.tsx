import { useCallback, useEffect, useState } from "react";
import { ArrowLeft, Check, ChevronRight, Folder, FolderPlus, HardDrive, ShieldAlert } from "lucide-react";
import { android, api, errorText, type LocalDir } from "./api";
import { isAndroid, registerPickerHost } from "./platform";
import { Button, IconButton, Modal } from "./ui";

/** Tracks Android "All files access"; re-checks when the user returns from Settings. */
export function useStorageAccess(): { granted: boolean; request: () => void } {
  const [granted, setGranted] = useState(!isAndroid());
  const check = useCallback(() => {
    if (!isAndroid()) return;
    android.hasAllFilesAccess().then((r) => setGranted(r.granted)).catch(() => {});
  }, []);
  useEffect(() => {
    check();
    const onVis = () => document.visibilityState === "visible" && check();
    document.addEventListener("visibilitychange", onVis);
    window.addEventListener("focus", check);
    return () => {
      document.removeEventListener("visibilitychange", onVis);
      window.removeEventListener("focus", check);
    };
  }, [check]);
  return {
    granted,
    request: () => {
      android.requestAllFilesAccess().catch(() => {});
    },
  };
}

export function StorageAccessNotice({ compact = false }: { compact?: boolean }) {
  const { granted, request } = useStorageAccess();
  if (granted) return null;
  return (
    <div className="notice warn">
      <ShieldAlert size={16} />
      <span className="grow">
        <b>Allow file access.</b>{" "}
        {compact
          ? "Needed to share folders and save downloads."
          : "Needed to share folders and save downloads. Tap Allow, switch Relayless on, then come back."}
      </span>
      <Button size="sm" variant="primary" onClick={request}>Allow</Button>
    </div>
  );
}

type Req = { title: string; start?: string; resolve: (p: string | null) => void };

/** In-app folder browser used on Android. */
export function FolderPickerHost() {
  const [req, setReq] = useState<Req | null>(null);
  const [dir, setDir] = useState<LocalDir | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [newName, setNewName] = useState<string | null>(null);
  const access = useStorageAccess();

  useEffect(() => {
    registerPickerHost((r) => setReq(r));
    return () => registerPickerHost(null);
  }, []);

  const load = useCallback((path: string | null) => {
    setError(null);
    api
      .listLocalDir(path)
      .then(setDir)
      .catch((e) => {
        setError(errorText(e));
        if (path) api.listLocalDir(null).then(setDir).catch(() => {});
      });
  }, []);

  useEffect(() => {
    if (req) load(req.start ?? null);
  }, [req, load, access.granted]);

  if (!req) return null;
  const finish = (p: string | null) => {
    req.resolve(p);
    setReq(null);
    setDir(null);
    setNewName(null);
  };
  const rel = dir ? dir.path.slice(dir.root.length).split("/").filter(Boolean) : [];

  return (
    <Modal
      title={req.title}
      onClose={() => finish(null)}
      width={560}
      footer={
        <>
          <Button variant="ghost" onClick={() => finish(null)}>Cancel</Button>
          <Button variant="primary" icon={<Check size={15} />} disabled={!dir || !access.granted} onClick={() => dir && finish(dir.path)}>
            Use this folder
          </Button>
        </>
      }
    >
      {!access.granted && <StorageAccessNotice />}
      <div className="hstack">
        <IconButton label="Up one level" disabled={!dir?.parent} onClick={() => dir?.parent && load(dir.parent)}>
          <ArrowLeft size={16} />
        </IconButton>
        <div className="crumbs">
          <button className={`crumb ${rel.length === 0 ? "last" : ""}`} onClick={() => dir && load(dir.root)}>
            <HardDrive size={13} style={{ verticalAlign: -2, marginRight: 6 }} />
            Phone storage
          </button>
          {rel.map((p, i) => (
            <span key={i} className="hstack" style={{ gap: 2 }}>
              <ChevronRight size={14} className="faint" />
              <button
                className={`crumb ${i === rel.length - 1 ? "last" : ""}`}
                onClick={() => dir && load(dir.root + "/" + rel.slice(0, i + 1).join("/"))}
              >
                {p}
              </button>
            </span>
          ))}
        </div>
        <IconButton label="New folder" disabled={!dir || !access.granted} onClick={() => setNewName("")}>
          <FolderPlus size={16} />
        </IconButton>
      </div>
      {newName !== null && dir && (
        <div className="hstack">
          <input
            className="input"
            autoFocus
            placeholder="Folder name"
            value={newName}
            onChange={(e) => setNewName(e.target.value)}
            onKeyDown={async (e) => {
              if (e.key === "Enter" && newName.trim()) {
                try {
                  const p = await api.makeDir(dir.path, newName);
                  setNewName(null);
                  load(p);
                } catch (err) {
                  setError(errorText(err));
                }
              }
            }}
          />
          <Button
            onClick={async () => {
              if (!newName.trim()) return;
              try {
                const p = await api.makeDir(dir.path, newName);
                setNewName(null);
                load(p);
              } catch (err) {
                setError(errorText(err));
              }
            }}
          >
            Create
          </Button>
        </div>
      )}
      {error && <div className="notice err"><ShieldAlert size={16} /><span>{error}</span></div>}
      <div className="picker-list">
        {dir?.dirs.length === 0 && <div className="faint small" style={{ padding: 14 }}>No folders here.</div>}
        {dir?.dirs.map((d) => (
          <button key={d} className="picker-item" onClick={() => load(dir.path + "/" + d)}>
            <Folder size={16} className="dir" />
            <span className="truncate grow">{d}</span>
            <ChevronRight size={15} className="faint" />
          </button>
        ))}
      </div>
    </Modal>
  );
}
