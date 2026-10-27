// Relayless website: live transfer demo, latest-release download links.
(() => {
  const REPO = "MisbahKhan0009/Relayless";
  const $ = (s, r = document) => r.querySelector(s);
  const MB = 1024 * 1024;
  const GB = 1024 * MB;

  const fmt = (b) => (b >= GB ? `${(b / GB).toFixed(2)} GB` : `${Math.round(b / MB)} MB`);
  const fmtSize = (b) => (b >= GB ? `${(b / GB).toFixed(1)} GB` : `${Math.round(b / MB)} MB`);

  // ------------------------------------------------------------------ demo
  const card = $(".demo-card");
  if (card) {
    const el = {
      saved: $("[data-saved]"),
      flight: $("[data-flight]"),
      bar: $("[data-bar]"),
      savedText: $("[data-saved-text]"),
      speed: $("[data-speed]"),
      eta: $("[data-eta]"),
      route: $("[data-route]"),
      toggle: $("[data-toggle]"),
      drop: $("[data-drop]"),
      log: $("[data-log]"),
    };
    const SIZE = 4.2 * GB;
    const COMMIT_MS = 2000; // progress is flushed and recorded every 2 s
    let received = 1.62 * GB;
    let saved = received;
    let state = "running"; // running | paused | reconnecting | verifying | done
    let speed = 0;
    let lastCommit = performance.now();
    let last = performance.now();
    let visible = true;
    let timer = null;
    const t0 = performance.now();

    const stamp = () => {
      const s = Math.floor((performance.now() - t0) / 1000);
      return `${String(Math.floor(s / 60)).padStart(2, "0")}:${String(s % 60).padStart(2, "0")}`;
    };
    const log = (text, good = false) => {
      const li = document.createElement("li");
      if (good) li.className = "good";
      const t = document.createElement("time");
      t.textContent = stamp();
      li.append(t, document.createTextNode(text));
      el.log.prepend(li);
      while (el.log.children.length > 4) el.log.lastChild.remove();
    };
    const setRoute = (text, cls) => {
      el.route.className = `route ${cls || ""}`;
      el.route.lastChild.textContent = text;
    };
    const commit = () => {
      saved = Math.floor(received / MB) * MB;
      lastCommit = performance.now();
    };

    const render = () => {
      const pSaved = (saved / SIZE) * 100;
      const pRecv = (received / SIZE) * 100;
      el.saved.style.width = `${pSaved}%`;
      el.flight.style.width = `${pRecv}%`;
      el.bar.setAttribute("aria-valuenow", String(Math.round(pSaved)));
      el.savedText.textContent = fmt(saved);
      el.speed.textContent = state === "running" ? `${speed.toFixed(1)} MB/s` : "0 MB/s";
      if (state === "running" && speed > 1) {
        const s = Math.round((SIZE - received) / (speed * MB));
        el.eta.textContent = s >= 60 ? `${Math.floor(s / 60)}m ${s % 60}s` : `${s}s`;
      } else {
        el.eta.textContent = state === "done" ? "Done" : "–";
      }
    };

    const tick = (now) => {
      const dt = Math.min(0.25, (now - last) / 1000);
      last = now;
      if (state === "running") {
        const target = 46 + Math.sin(now / 1300) * 4 + Math.sin(now / 370) * 1.5;
        speed += (target - speed) * Math.min(1, dt * 3);
        received = Math.min(SIZE, received + speed * MB * dt);
        if (now - lastCommit > COMMIT_MS) commit();
        if (received >= SIZE) {
          commit();
          state = "verifying";
          setRoute("Verifying", "paused");
          log("All pieces received. Checking the whole file…");
          el.toggle.disabled = true;
          el.drop.disabled = true;
          timer = setTimeout(() => {
            state = "done";
            setRoute("Complete");
            log("Verified 4.20 GB. Saved to Downloads.", true);
            render();
            timer = setTimeout(restart, 5000);
          }, 1600);
        }
      }
      render();
      if (visible) requestAnimationFrame(tick);
    };

    const restart = () => {
      received = 0;
      saved = 0;
      speed = 0;
      state = "running";
      el.toggle.disabled = false;
      el.drop.disabled = false;
      el.toggle.textContent = "Pause";
      setRoute("Direct, 176 ms");
      log("Starting the next download.");
    };

    el.toggle.addEventListener("click", () => {
      if (state === "running") {
        commit();
        received = saved;
        state = "paused";
        el.toggle.textContent = "Resume";
        setRoute("Paused", "paused");
        log(`Paused by you. ${fmt(saved)} is saved on disk.`);
      } else if (state === "paused") {
        state = "running";
        speed = 8;
        el.toggle.textContent = "Pause";
        setRoute("Direct, 176 ms");
        log(`Resumed from ${fmt(saved)}. Nothing downloaded twice.`, true);
      }
      render();
    });

    el.drop.addEventListener("click", () => {
      if (state !== "running" && state !== "paused") return;
      const wasPaused = state === "paused";
      commit();
      received = saved;
      state = "reconnecting";
      el.toggle.disabled = true;
      el.drop.disabled = true;
      setRoute("Reconnecting", "off");
      log(`Connection lost. ${fmt(saved)} is safe on disk.`);
      render();
      clearTimeout(timer);
      timer = setTimeout(() => {
        el.toggle.disabled = false;
        el.drop.disabled = false;
        if (wasPaused) {
          state = "paused";
          setRoute("Paused", "paused");
          log("Back online. Still paused.");
        } else {
          state = "running";
          speed = 6;
          setRoute("Direct, 181 ms");
          log(`Reconnected. Continuing from ${fmt(saved)}.`, true);
        }
        render();
      }, 2600);
    });

    log("Downloading over a direct, encrypted connection.");
    // Only animate while the card is on screen.
    new IntersectionObserver((entries) => {
      const v = entries.some((e) => e.isIntersecting);
      if (v && !visible) {
        visible = true;
        last = performance.now();
        requestAnimationFrame(tick);
      } else if (!v) {
        visible = false;
      }
    }).observe(card);
    requestAnimationFrame(tick);
  }

  // ------------------------------------------------------------------ platform
  const isAndroid = /Android/i.test(navigator.userAgent);
  const primary = $("[data-primary-dl]");
  const secondary = $("[data-secondary-dl]");
  if (isAndroid) {
    primary.textContent = "Download for Android";
    secondary.textContent = "Get the Windows app";
  }
  document.querySelector(`[data-dl="${isAndroid ? "android" : "windows"}"]`)?.classList.add("recommended");

  // ------------------------------------------------------------------ release links
  fetch(`https://api.github.com/repos/${REPO}/releases/latest`, { headers: { Accept: "application/vnd.github+json" } })
    .then((r) => (r.ok ? r.json() : Promise.reject(r.status)))
    .then((rel) => {
      const find = (re) => (rel.assets || []).find((a) => re.test(a.name));
      const win = find(/x64\.exe$/i);
      const apk = find(/android\.apk$/i);
      const date = rel.published_at ? new Date(rel.published_at).toLocaleDateString(undefined, { year: "numeric", month: "long", day: "numeric" }) : "";
      const ver = (rel.name || rel.tag_name || "").replace(/^Relayless\s*/i, "");
      const v = $("[data-version]");
      if (v && ver) v.textContent = `Version ${ver}${date ? `, released ${date}` : ""}`;
      const apply = (key, asset, label) => {
        if (!asset) return;
        const card = document.querySelector(`[data-dl="${key}"]`);
        card.href = asset.browser_download_url;
        $(`[data-meta="${key}"]`).textContent = `${label}, ${fmtSize(asset.size)}`;
      };
      apply("windows", win, "Windows 10 and 11, 64-bit installer");
      apply("android", apk, "Android 7.0 and newer, any phone or tablet");
      const first = isAndroid ? apk : win;
      const second = isAndroid ? win : apk;
      if (first) primary.href = first.browser_download_url;
      if (second) secondary.href = second.browser_download_url;
    })
    .catch(() => {
      /* Links fall back to the releases page. */
    });

  fetch(`https://api.github.com/repos/${REPO}`)
    .then((r) => (r.ok ? r.json() : Promise.reject()))
    .then((repo) => {
      const s = $("[data-stars]");
      if (s && repo.stargazers_count > 0) {
        s.textContent = repo.stargazers_count.toLocaleString();
        s.hidden = false;
      }
    })
    .catch(() => {});
})();
