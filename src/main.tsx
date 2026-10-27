import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import { StoreProvider } from "./store";
import { FlowProvider } from "./flow";
import "@fontsource-variable/figtree";
import "@fontsource-variable/oxanium";
import "@fontsource-variable/jetbrains-mono";
import "./styles.css";

// Desktop app: no browser context menu or accidental reloads.
window.addEventListener("contextmenu", (e) => {
  const t = e.target as HTMLElement;
  if (!t.closest("input, textarea, .selectable")) e.preventDefault();
});
window.addEventListener("keydown", (e) => {
  if (e.key === "F5" || ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "r")) e.preventDefault();
});

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <StoreProvider>
      <FlowProvider>
        <App />
      </FlowProvider>
    </StoreProvider>
  </StrictMode>,
);
