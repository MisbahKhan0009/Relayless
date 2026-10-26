import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import {
  api,
  on,
  type AccessRequest,
  type Activity,
  type Grant,
  type Job,
  type NetworkStatus,
  type PeerStatus,
  type Remote,
  type Settings,
  type Share,
  type Upload,
} from "./api";

export type Page = "overview" | "shares" | "devices" | "browse" | "transfers" | "activity" | "settings";

export interface BrowseTarget {
  peerId: string;
  shareId: string;
  shareName: string;
  peerName: string;
}

export interface Toast {
  id: number;
  tone: "ok" | "err" | "info";
  text: string;
}

interface Store {
  ready: boolean;
  version: string;
  dataDir: string;
  settings: Settings | null;
  network: NetworkStatus | null;
  shares: Share[];
  grants: Grant[];
  remotes: Remote[];
  jobs: Record<string, Job>;
  uploads: Upload[];
  pending: AccessRequest[];
  activity: Activity[];
  integrity: Record<string, { checked: number; total: number }>;
  page: Page;
  browse: BrowseTarget | null;
  toasts: Toast[];
  connectOpen: boolean;
  setPage: (p: Page) => void;
  openBrowser: (t: BrowseTarget) => void;
  setConnectOpen: (v: boolean) => void;
  toast: (text: string, tone?: Toast["tone"]) => void;
  saveSettings: (s: Settings) => Promise<void>;
  refreshShares: () => void;
  refreshGrants: () => void;
  refreshRemotes: () => void;
  refreshJobs: () => void;
}

const Ctx = createContext<Store | null>(null);

export function useStore(): Store {
  const s = useContext(Ctx);
  if (!s) throw new Error("store missing");
  return s;
}

/** Debounces bursts of change events into one refresh. */
function useDebounced(fn: () => void, ms = 150) {
  const t = useRef<number | undefined>(undefined);
  const f = useRef(fn);
  f.current = fn;
  return useCallback(() => {
    window.clearTimeout(t.current);
    t.current = window.setTimeout(() => f.current(), ms);
  }, [ms]);
}

export function StoreProvider({ children }: { children: ReactNode }) {
  const [ready, setReady] = useState(false);
  const [version, setVersion] = useState("");
  const [dataDir, setDataDir] = useState("");
  const [settings, setSettings] = useState<Settings | null>(null);
  const [network, setNetwork] = useState<NetworkStatus | null>(null);
  const [shares, setShares] = useState<Share[]>([]);
  const [grants, setGrants] = useState<Grant[]>([]);
  const [remotes, setRemotes] = useState<Remote[]>([]);
  const [jobs, setJobs] = useState<Record<string, Job>>({});
  const [uploads, setUploads] = useState<Upload[]>([]);
  const [pending, setPending] = useState<AccessRequest[]>([]);
  const [activity, setActivity] = useState<Activity[]>([]);
  const [integrity, setIntegrity] = useState<Record<string, { checked: number; total: number }>>({});
  const [page, setPage] = useState<Page>("overview");
  const [browse, setBrowse] = useState<BrowseTarget | null>(null);
  const [toasts, setToasts] = useState<Toast[]>([]);
  const [connectOpen, setConnectOpen] = useState(false);

  const toast = useCallback((text: string, tone: Toast["tone"] = "info") => {
    const id = Date.now() + Math.random();
    setToasts((t) => [...t.slice(-3), { id, tone, text }]);
    window.setTimeout(() => setToasts((t) => t.filter((x) => x.id !== id)), tone === "err" ? 7000 : 4000);
  }, []);

  const refreshShares = useDebounced(() => api.listShares().then(setShares).catch(() => {}));
  const refreshGrants = useDebounced(() => api.listGrants().then(setGrants).catch(() => {}));
  const refreshRemotes = useDebounced(() => api.listRemotes().then(setRemotes).catch(() => {}));
  const refreshJobs = useDebounced(() =>
    api
      .listJobs()
      .then((list) => setJobs(Object.fromEntries(list.map((j) => [j.id, j]))))
      .catch(() => {}),
  );

  useEffect(() => {
    let disposed = false;
    const unlisten: Array<() => void> = [];
    const sub = <T,>(name: string, cb: (p: T) => void) => on<T>(name, cb).then((u) => (disposed ? u() : unlisten.push(u)));

    sub<Job>("job-updated", (j) => setJobs((m) => ({ ...m, [j.id]: j })));
    sub<{ id: string }>("job-removed", ({ id }) =>
      setJobs((m) => {
        const n = { ...m };
        delete n[id];
        return n;
      }),
    );
    sub<PeerStatus>("peer-status", (p) =>
      setRemotes((rs) => rs.map((r) => (r.peerId === p.peerId ? { ...r, status: p } : r))),
    );
    sub<AccessRequest>("access-requested", (r) => setPending((p) => [...p.filter((x) => x.id !== r.id), r]));
    sub<{ id: string }>("access-resolved", ({ id }) => setPending((p) => p.filter((x) => x.id !== id)));
    sub("shares-changed", () => refreshShares());
    sub("grants-changed", () => refreshGrants());
    sub("remotes-changed", () => refreshRemotes());
    sub<Activity>("activity", (a) => setActivity((l) => [a, ...l].slice(0, 300)));
    sub<NetworkStatus>("network-status", setNetwork);
    sub<Upload[]>("uploads", setUploads);
    sub<{ jobId: string; checked: number; total: number }>("integrity-progress", (p) =>
      setIntegrity((m) => ({ ...m, [p.jobId]: { checked: p.checked, total: p.total } })),
    );

    (async () => {
      try {
        const b = await api.bootstrap();
        setVersion(b.version);
        setDataDir(b.dataDir);
        setSettings(b.settings);
        setNetwork(b.network);
        const [s, g, r, j, p, a] = await Promise.all([
          api.listShares(),
          api.listGrants(),
          api.listRemotes(),
          api.listJobs(),
          api.pendingRequests(),
          api.listActivity(300),
        ]);
        setShares(s);
        setGrants(g);
        setRemotes(r);
        setJobs(Object.fromEntries(j.map((x) => [x.id, x])));
        setPending(p);
        setActivity(a);
      } finally {
        setReady(true);
      }
    })();
    return () => {
      disposed = true;
      unlisten.forEach((u) => u());
    };
  }, [refreshShares, refreshGrants, refreshRemotes]);

  const saveSettings = useCallback(async (s: Settings) => {
    const out = await api.updateSettings(s);
    setSettings(out);
  }, []);

  const openBrowser = useCallback((t: BrowseTarget) => {
    setBrowse(t);
    setPage("browse");
  }, []);

  const value = useMemo<Store>(
    () => ({
      ready,
      version,
      dataDir,
      settings,
      network,
      shares,
      grants,
      remotes,
      jobs,
      uploads,
      pending,
      activity,
      integrity,
      page,
      browse,
      toasts,
      connectOpen,
      setPage,
      openBrowser,
      setConnectOpen,
      toast,
      saveSettings,
      refreshShares,
      refreshGrants,
      refreshRemotes,
      refreshJobs,
    }),
    [ready, version, dataDir, settings, network, shares, grants, remotes, jobs, uploads, pending, activity, integrity, page, browse, toasts, connectOpen, openBrowser, toast, saveSettings, refreshShares, refreshGrants, refreshRemotes, refreshJobs],
  );
  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}
