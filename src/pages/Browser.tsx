import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import {
  ArrowDown,
  ArrowLeft,
  ArrowUp,
  ChevronRight,
  Download,
  File,
  FileArchive,
  FileImage,
  FileText,
  FileVideo,
  Folder,
  FolderOpen,
  HardDrive,
  RefreshCw,
  Search,
} from "lucide-react";
import { api, errorText, type RemoteEntry } from "../api";
import { bytes, dateTime } from "../format";
import { useStore } from "../store";
import { Button, Empty, IconButton, Modal } from "../ui";

const ROW = 38;
const PAGE = 500;

function fileIcon(e: RemoteEntry) {
  if (e.isDir) return <Folder size={16} className="dir" />;
  const ext = e.name.split(".").pop()?.toLowerCase() ?? "";
  if (["zip", "7z", "rar", "tar", "gz", "xz", "iso", "img", "vhd", "vhdx"].includes(ext)) return <FileArchive size={16} />;
  if (["mp4", "mkv", "mov", "avi", "webm", "m4v"].includes(ext)) return <FileVideo size={16} />;
  if (["jpg", "jpeg", "png", "gif", "webp", "heic", "tif", "tiff", "raw", "dng"].includes(ext)) return <FileImage size={16} />;
  if (["txt", "md", "pdf", "doc", "docx", "csv", "json", "log"].includes(ext)) return <FileText size={16} />;
  return <File size={16} />;
}

type SortKey = "name" | "size" | "modified";

export default function Browser() {
  const { browse, settings, toast, setPage } = useStore();
  const [path, setPath] = useState<string[]>([]);
  const [entries, setEntries] = useState<RemoteEntry[]>([]);
  const [total, setTotal] = useState(0);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [filter, setFilter] = useState("");
  const [sort, setSort] = useState<{ key: SortKey; asc: boolean }>({ key: "name", asc: true });
  const [scrollTop, setScrollTop] = useState(0);
  const [viewH, setViewH] = useState(600);
  const [confirm, setConfirm] = useState(false);
  const [dest, setDest] = useState("");
  const [starting, setStarting] = useState(false);
  const body = useRef<HTMLDivElement>(null);
  const reqId = useRef(0);

  const load = useCallback(
    async (p: string[], offset: number) => {
      if (!browse) return;
      const id = ++reqId.current;
      setLoading(true);
      setError(null);
      try {
        const r = await api.listRemoteDir(browse.peerId, browse.shareId, p, offset, PAGE);
        if (id !== reqId.current) return;
        setTotal(r.total);
        setEntries((prev) => (offset === 0 ? r.entries : [...prev, ...r.entries]));
      } catch (e) {
        if (id === reqId.current) setError(errorText(e));
      } finally {
        if (id === reqId.current) setLoading(false);
      }
    },
    [browse],
  );

  useEffect(() => {
    setPath([]);
    setSelected(new Set());
    setEntries([]);
    load([], 0);
  }, [browse, load]);

  useEffect(() => {
    const el = body.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setViewH(el.clientHeight));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const go = (p: string[]) => {
    setPath(p);
    setSelected(new Set());
    setEntries([]);
    setFilter("");
    body.current?.scrollTo({ top: 0 });
    load(p, 0);
  };

  const shown = useMemo(() => {
    const f = filter.trim().toLowerCase();
    const list = f ? entries.filter((e) => e.name.toLowerCase().includes(f)) : entries.slice();
    const dir = sort.asc ? 1 : -1;
    list.sort((a, b) => {
      if (a.isDir !== b.isDir) return a.isDir ? -1 : 1;
      if (sort.key === "size") return (a.size - b.size) * dir;
      if (sort.key === "modified") return (a.modifiedMs - b.modifiedMs) * dir;
      return a.name.localeCompare(b.name, undefined, { numeric: true, sensitivity: "base" }) * dir;
    });
    return list;
  }, [entries, filter, sort]);

  // Fetch the next page as the user nears the end of what's loaded.
  useEffect(() => {
    if (!loading && entries.length < total && scrollTop + viewH > entries.length * ROW - 20 * ROW) {
      load(path, entries.length);
    }
  }, [scrollTop, viewH, entries.length, total, loading, load, path]);

  if (!browse) {
    return (
      <div className="page">
        <div className="panel">
          <Empty icon={<FolderOpen size={20} />} title="Nothing open">
            Choose a shared folder from <a href="#" onClick={(e) => { e.preventDefault(); setPage("devices"); }}>Remote devices</a>.
          </Empty>
        </div>
      </div>
    );
  }

  const first = Math.max(0, Math.floor(scrollTop / ROW) - 8);
  const last = Math.min(shown.length, Math.ceil((scrollTop + viewH) / ROW) + 8);
  const sel = shown.filter((e) => selected.has(e.name));
  const selFiles = sel.filter((e) => !e.isDir);
  const selBytes = selFiles.reduce((s, e) => s + e.size, 0);
  const allSel = shown.length > 0 && shown.every((e) => selected.has(e.name));

  const toggle = (name: string) =>
    setSelected((s) => {
      const n = new Set(s);
      if (n.has(name)) n.delete(name);
      else n.add(name);
      return n;
    });

  const sortBy = (key: SortKey) => setSort((s) => ({ key, asc: s.key === key ? !s.asc : key === "name" }));
  const arrow = (key: SortKey) => (sort.key === key ? sort.asc ? <ArrowUp size={12} /> : <ArrowDown size={12} /> : null);

  const startDownload = async () => {
    setStarting(true);
    try {
      const items = sel.map((e) => ({ path: [...path, e.name], isDir: e.isDir }));
      const ids = await api.startDownload(browse.peerId, browse.shareId, items, dest || null);
      setConfirm(false);
      setSelected(new Set());
      toast(`Queued ${ids.length} file(s). Track them in Transfers.`, "ok");
    } catch (e) {
      toast(errorText(e), "err");
    } finally {
      setStarting(false);
    }
  };

  return (
    <div className="page" style={{ gap: 14 }}>
      <div className="toolbar">
        <IconButton label="Up one level" disabled={path.length === 0} onClick={() => go(path.slice(0, -1))}>
          <ArrowLeft size={16} />
        </IconButton>
        <nav className="crumbs" aria-label="Folder path">
          <button className={`crumb ${path.length === 0 ? "last" : ""}`} onClick={() => go([])}>
            <HardDrive size={13} style={{ verticalAlign: -2, marginRight: 6 }} />
            {browse.shareName}
          </button>
          {path.map((p, i) => (
            <span key={i} className="hstack" style={{ gap: 2 }}>
              <ChevronRight size={14} className="faint" />
              <button className={`crumb ${i === path.length - 1 ? "last" : ""}`} onClick={() => go(path.slice(0, i + 1))}>{p}</button>
            </span>
          ))}
        </nav>
        <div className="search">
          <Search size={14} />
          <input placeholder="Filter this folder" value={filter} onChange={(e) => setFilter(e.target.value)} aria-label="Filter" />
        </div>
        <IconButton label="Refresh" onClick={() => go(path)}>
          <RefreshCw size={15} className={loading ? "spin" : ""} />
        </IconButton>
      </div>

      {error ? (
        <div className="notice err">
          <RefreshCw size={16} />
          <span className="grow">{error}</span>
          <Button size="sm" onClick={() => go(path)}>Try again</Button>
        </div>
      ) : (
        <div className="table" role="grid" aria-rowcount={shown.length}>
          <div className="thead" role="row">
            <span>
              <input
                type="checkbox"
                aria-label="Select all"
                checked={allSel}
                onChange={() => setSelected(allSel ? new Set() : new Set(shown.map((e) => e.name)))}
              />
            </span>
            <button onClick={() => sortBy("name")}>Name {arrow("name")}</button>
            <button onClick={() => sortBy("size")} style={{ justifySelf: "end" }}>Size {arrow("size")}</button>
            <button onClick={() => sortBy("modified")} style={{ justifySelf: "end" }}>Modified {arrow("modified")}</button>
            <span />
          </div>
          <div className="tbody" ref={body} onScroll={(e) => setScrollTop(e.currentTarget.scrollTop)}>
            <div style={{ height: shown.length * ROW }} />
            {shown.slice(first, last).map((e, k) => {
              const i = first + k;
              const on = selected.has(e.name);
              return (
                <div
                  key={e.name}
                  role="row"
                  className={`trow ${on ? "sel" : ""}`}
                  style={{ top: i * ROW }}
                  onDoubleClick={() => (e.isDir ? go([...path, e.name]) : toggle(e.name))}
                >
                  <span>
                    <input type="checkbox" aria-label={`Select ${e.name}`} checked={on} onChange={() => toggle(e.name)} />
                  </span>
                  <span className="name">
                    {fileIcon(e)}
                    {e.isDir ? (
                      <button className="crumb" style={{ padding: 0, color: "var(--text)", maxWidth: "100%" }} onClick={() => go([...path, e.name])}>
                        {e.name}
                      </button>
                    ) : (
                      <span className="truncate selectable" title={e.name}>{e.name}</span>
                    )}
                  </span>
                  <span className="cell-r">{e.isDir ? "Folder" : bytes(e.size)}</span>
                  <span className="cell-r">{e.modifiedMs ? dateTime(e.modifiedMs) : "–"}</span>
                  <span>
                    <IconButton
                      label={`Download ${e.name}`}
                      onClick={() => {
                        setSelected(new Set([e.name]));
                        setDest(settings?.downloadDir ?? "");
                        setConfirm(true);
                      }}
                    >
                      <Download size={15} />
                    </IconButton>
                  </span>
                </div>
              );
            })}
            {!loading && shown.length === 0 && (
              <Empty icon={<FolderOpen size={20} />} title={filter ? "No matches" : "This folder is empty"} />
            )}
          </div>
        </div>
      )}

      {sel.length > 0 && (
        <div className="selection-bar">
          <span className="grow">
            <b>{sel.length}</b> selected{selFiles.length > 0 && <span className="muted"> · {bytes(selBytes)}{sel.length > selFiles.length ? " + folders" : ""}</span>}
          </span>
          <Button variant="ghost" onClick={() => setSelected(new Set())}>Clear</Button>
          <Button variant="primary" icon={<Download size={15} />} onClick={() => { setDest(settings?.downloadDir ?? ""); setConfirm(true); }}>
            Download selected
          </Button>
        </div>
      )}
      {total > entries.length && <span className="hint">Showing {entries.length} of {total} items. Scroll to load more.</span>}

      {confirm && (
        <Modal
          title="Download to this computer"
          onClose={() => setConfirm(false)}
          footer={
            <>
              <Button variant="ghost" onClick={() => setConfirm(false)}>Cancel</Button>
              <Button variant="primary" loading={starting} icon={<Download size={15} />} onClick={startDownload}>Start download</Button>
            </>
          }
        >
          <span>
            {sel.length === 1 ? <b>{sel[0].name}</b> : <b>{sel.length} items</b>}
            {selFiles.length > 0 && <span className="muted"> ({bytes(selBytes)}{sel.length > selFiles.length ? " plus folder contents" : ""})</span>}
          </span>
          <div className="field">
            <span className="label">Save to</span>
            <div className="hstack">
              <input className="input mono" value={dest} onChange={(e) => setDest(e.target.value)} />
              <Button
                onClick={async () => {
                  const p = await open({ directory: true, defaultPath: dest || undefined, title: "Choose where to save" });
                  if (typeof p === "string") setDest(p);
                }}
              >
                Change…
              </Button>
            </div>
            <span className="hint">Files are written as “.rlpart” while downloading and renamed only after every byte is verified.</span>
          </div>
        </Modal>
      )}
    </div>
  );
}
