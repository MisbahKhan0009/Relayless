import { useCallback } from "react";

type Kind = "sel" | "hov";

/**
 * Sliding selection and hover marks for a group of items: the sidebar, tabs,
 * segmented controls and menus. Give the container the `slides` class; it
 * draws the hover mark with ::before and the selection mark with ::after from
 * CSS variables this hook keeps in sync, so both glide from item to item
 * instead of jumping.
 *
 * `item` selects the items, `active` the selected one. Selection changes are
 * picked up from class and aria attribute changes, so callers don't need to
 * pass the selected value. Layout changes (resize, fonts loading, counts
 * changing width) move the marks without animation.
 */
export function useSlide<T extends HTMLElement>(item: string, active: string) {
  // A callback ref (with React 19 cleanup), so it follows the element even
  // when the container is rendered conditionally.
  return useCallback((box: T | null) => {
    if (!box) return;
    let hovered: HTMLElement | null = null;
    const frames: number[] = [];

    const hide = (kind: Kind) => {
      box.dataset[kind] = "off";
    };

    const put = (kind: Kind, el: HTMLElement | null, snap: boolean) => {
      // Hidden items (display: none) have no offsetParent.
      if (!el || el.offsetParent !== box) return hide(kind);
      const appearing = box.dataset[kind] !== "on";
      const snapKey = kind === "sel" ? "selSnap" : "hovSnap";
      if (snap || appearing) {
        // Place without the transition, then allow it again after a frame.
        box.dataset[snapKey] = "";
        const a = requestAnimationFrame(() => {
          frames.push(requestAnimationFrame(() => delete box.dataset[snapKey]));
        });
        frames.push(a);
      }
      const s = box.style;
      s.setProperty(`--${kind}-x`, `${el.offsetLeft}px`);
      s.setProperty(`--${kind}-y`, `${el.offsetTop}px`);
      s.setProperty(`--${kind}-w`, `${el.offsetWidth}px`);
      s.setProperty(`--${kind}-h`, `${el.offsetHeight}px`);
      box.dataset[kind] = "on";
    };

    const sync = (snap: boolean) => {
      put("sel", box.querySelector<HTMLElement>(active), snap);
      if (hovered) {
        if (box.contains(hovered)) put("hov", hovered, snap);
        else {
          hovered = null;
          hide("hov");
        }
      }
    };

    const ro = new ResizeObserver(() => sync(true));
    const watch = () => {
      ro.disconnect();
      ro.observe(box);
      box.querySelectorAll<HTMLElement>(item).forEach((n) => ro.observe(n));
    };
    watch();
    const mo = new MutationObserver((list) => {
      if (list.some((m) => m.type === "childList")) watch();
      sync(false);
    });
    mo.observe(box, {
      subtree: true,
      childList: true,
      attributes: true,
      attributeFilter: ["class", "aria-checked", "aria-selected", "aria-current"],
    });

    const over = (e: PointerEvent) => {
      if (e.pointerType !== "mouse") return;
      const it = (e.target as Element).closest<HTMLElement>(item);
      // Moving through the gap between items keeps the mark where it is.
      if (!it || !box.contains(it) || it === hovered) return;
      hovered = it;
      put("hov", it, false);
    };
    const leave = () => {
      hovered = null;
      hide("hov");
    };
    box.addEventListener("pointerover", over);
    box.addEventListener("pointerleave", leave);
    sync(true);

    return () => {
      ro.disconnect();
      mo.disconnect();
      box.removeEventListener("pointerover", over);
      box.removeEventListener("pointerleave", leave);
      frames.forEach(cancelAnimationFrame);
    };
  }, [item, active]);
}
