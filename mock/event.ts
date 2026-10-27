import { data } from "./core";

// Demo backend events: a download and an upload whose speeds wander, so the
// live graph moves in the preview build.
export type UnlistenFn = () => void;
type Cb = (e: { payload: any }) => void;
const subs: Record<string, Set<Cb>> = {};
let started = false;

function emit(name: string, payload: unknown) {
  subs[name]?.forEach((cb) => cb({ payload }));
}

function start() {
  if (started) return;
  started = true;
  let i = 0;
  window.setInterval(() => {
    i++;
    const j = data.list_jobs.find((x: any) => x.id === "j1");
    if (j) {
      const bps = (10.4 + 1.6 * Math.sin(i / 7) + 0.7 * Math.sin(i / 2.3)) * 1024 * 1024;
      j.speedBps = bps;
      j.doneBytes = Math.min(j.size, j.doneBytes + bps);
      j.verifiedBytes = j.doneBytes;
      emit("job-updated", { ...j });
    }
    const up = (3.1 + 0.9 * Math.sin(i / 5) + 0.3 * Math.cos(i / 1.7)) * 1024 * 1024;
    emit("uploads", [{ peerId: "77aa88bb99", deviceName: "Lena's laptop", bps: up, totalSent: 4.2 * 1024 ** 3 + i * up, files: ["color-grade-notes.pdf", "scene-014-take-2.mov"] }]);
  }, 1000);
}

export async function listen<T>(e: string, cb: (e: { payload: T }) => void): Promise<UnlistenFn> {
  (subs[e] ??= new Set()).add(cb as Cb);
  start();
  return () => subs[e]?.delete(cb as Cb);
}
