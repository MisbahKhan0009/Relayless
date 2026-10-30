import { useEffect, useRef, useState, type ButtonHTMLAttributes, type ReactNode } from "react";
import {
  AlertTriangle,
  CheckCircle2,
  CircleDashed,
  CirclePause,
  Clock,
  Loader2,
  RefreshCw,
  ShieldCheck,
  Waypoints,
  X,
  XCircle,
  Zap,
} from "lucide-react";
import type { JobStatus, RouteInfo } from "./api";
import { STATUS_TEXT, statusTone, type Tone } from "./format";
import { RouteGlyph } from "./hud";
import { useSlide } from "./slide";

export function Button({
  variant = "secondary",
  size = "md",
  icon,
  loading,
  children,
  className = "",
  ...rest
}: ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: "primary" | "secondary" | "ghost" | "danger";
  size?: "sm" | "md" | "lg";
  icon?: ReactNode;
  loading?: boolean;
}) {
  return (
    <button className={`btn btn-${variant} btn-${size} ${className}`} disabled={loading || rest.disabled} {...rest}>
      {loading ? <Loader2 size={15} className="spin" /> : icon}
      {children && <span>{children}</span>}
    </button>
  );
}

export function IconButton({
  label,
  children,
  className = "",
  ...rest
}: ButtonHTMLAttributes<HTMLButtonElement> & { label: string }) {
  return (
    <button className={`icon-btn ${className}`} aria-label={label} title={label} {...rest}>
      {children}
    </button>
  );
}

export function Pill({ tone = "muted", icon, children }: { tone?: Tone; icon?: ReactNode; children: ReactNode }) {
  return (
    <span className={`pill pill-${tone}`}>
      {icon}
      {children}
    </span>
  );
}

export function Progress({ value, tone = "accent", indeterminate }: { value: number; tone?: Tone; indeterminate?: boolean }) {
  const pct = Math.max(0, Math.min(100, value * 100));
  return (
    <div
      className={`progress ${indeterminate ? "indeterminate" : ""}`}
      role="progressbar"
      aria-valuenow={Math.round(pct)}
      aria-valuemin={0}
      aria-valuemax={100}
    >
      <div className={`progress-bar bar-${tone}`} style={{ width: indeterminate ? undefined : `${pct}%` }} />
    </div>
  );
}

export function StatusIcon({ status, size = 16 }: { status: JobStatus; size?: number }) {
  const p = { size, "aria-hidden": true };
  switch (status) {
    case "downloading":
      return <Zap {...p} />;
    case "verifying":
      return <ShieldCheck {...p} />;
    case "completed":
      return <CheckCircle2 {...p} />;
    case "paused":
      return <CirclePause {...p} />;
    case "queued":
      return <Clock {...p} />;
    case "connecting":
      return <Loader2 {...p} className="spin" />;
    case "reconnecting":
    case "waiting_peer":
      return <RefreshCw {...p} />;
    case "waiting_direct":
      return <Waypoints {...p} />;
    case "needs_attention":
      return <AlertTriangle {...p} />;
    case "failed":
    case "canceled":
      return <XCircle {...p} />;
    default:
      return <CircleDashed {...p} />;
  }
}

export function StatusPill({ status }: { status: JobStatus }) {
  return (
    <Pill tone={statusTone(status)} icon={<StatusIcon status={status} size={13} />}>
      {STATUS_TEXT[status]}
    </Pill>
  );
}

export function RouteBadge({ route, connected = true }: { route: RouteInfo | null | undefined; connected?: boolean }) {
  if (!connected || !route || route.kind === "none") {
    return <Pill tone="muted" icon={<RouteGlyph kind="none" />}>Offline</Pill>;
  }
  if (route.kind === "direct") {
    return (
      <Pill tone="ok" icon={<RouteGlyph kind="direct" />}>
        Direct{route.rttMs != null ? ` · ${route.rttMs} ms` : ""}
      </Pill>
    );
  }
  return (
    <Pill tone="warn" icon={<RouteGlyph kind="relay" />}>
      Relayed{route.rttMs != null ? ` · ${route.rttMs} ms` : ""}
    </Pill>
  );
}

export function Modal({
  title,
  onClose,
  children,
  footer,
  width = 520,
}: {
  title: ReactNode;
  onClose: () => void;
  children: ReactNode;
  footer?: ReactNode;
  width?: number;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const closeRef = useRef(onClose);
  closeRef.current = onClose;
  // Runs once per opening. Re-running on every render (e.g. when the phone
  // keyboard resizes the screen) would steal focus and make the keyboard flicker.
  useEffect(() => {
    const prev = document.activeElement as HTMLElement | null;
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && closeRef.current();
    window.addEventListener("keydown", onKey);
    // On touch screens don't pop the keyboard open uninvited.
    const touch = window.matchMedia("(pointer: coarse)").matches;
    const first = touch ? null : ref.current?.querySelector<HTMLElement>("input, textarea, select, button.btn-primary");
    (first ?? ref.current)?.focus({ preventScroll: true });
    return () => {
      window.removeEventListener("keydown", onKey);
      if (!touch) prev?.focus?.();
    };
  }, []);
  return (
    <div className="modal-backdrop" onMouseDown={(e) => e.target === e.currentTarget && closeRef.current()}>
      <div className="modal" role="dialog" aria-modal="true" tabIndex={-1} style={{ maxWidth: width }} ref={ref}>
        <div className="sheet-handle" aria-hidden="true" />
        <div className="modal-head">
          <h2>{title}</h2>
          <IconButton label="Close" onClick={() => closeRef.current()}>
            <X size={16} />
          </IconButton>
        </div>
        <div className="modal-body">{children}</div>
        {footer && <div className="modal-foot">{footer}</div>}
      </div>
    </div>
  );
}

export function Empty({ icon, title, children }: { icon: ReactNode; title: string; children?: ReactNode }) {
  return (
    <div className="empty">
      <div className="empty-icon">{icon}</div>
      <div className="empty-title">{title}</div>
      {children && <div className="empty-text">{children}</div>}
    </div>
  );
}

export function Toggle({
  checked,
  onChange,
  label,
  hint,
  disabled,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
  label: ReactNode;
  hint?: ReactNode;
  disabled?: boolean;
}) {
  return (
    <label className={`toggle-row ${disabled ? "disabled" : ""}`}>
      <span className="toggle-text">
        <span className="toggle-label">{label}</span>
        {hint && <span className="hint">{hint}</span>}
      </span>
      <input
        type="checkbox"
        role="switch"
        className="switch"
        checked={checked}
        disabled={disabled}
        onChange={(e) => onChange(e.target.checked)}
      />
    </label>
  );
}

export function Segmented<T extends string | number>({
  value,
  options,
  onChange,
  label,
}: {
  value: T;
  options: { value: T; label: string }[];
  onChange: (v: T) => void;
  label: string;
}) {
  const slide = useSlide<HTMLDivElement>("button", "button.on");
  return (
    <div className="segmented slides" role="radiogroup" aria-label={label} ref={slide}>
      {options.map((o) => (
        <button
          key={String(o.value)}
          role="radio"
          aria-checked={o.value === value}
          className={o.value === value ? "on" : ""}
          onClick={() => onChange(o.value)}
          type="button"
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}

export function SectionHeader({ title, children }: { title: ReactNode; children?: ReactNode }) {
  return (
    <div className="section-head">
      <h3>{title}</h3>
      <div className="section-actions">{children}</div>
    </div>
  );
}

/** True on phone-sized screens; updates on rotation/resize. */
export function usePhone(): boolean {
  const q = "(max-width: 760px)";
  const [phone, setPhone] = useState(() => window.matchMedia(q).matches);
  useEffect(() => {
    const m = window.matchMedia(q);
    const on = () => setPhone(m.matches);
    m.addEventListener("change", on);
    return () => m.removeEventListener("change", on);
  }, []);
  return phone;
}

/**
 * Five words both devices derive from their two identities. Matching words
 * on both screens mean nobody sits in the middle of the connection.
 */
export function SafetyWords({ words, compact }: { words: string[]; compact?: boolean }) {
  if (!words?.length) return null;
  return (
    <span className={`words ${compact ? "compact" : ""}`} role="group" aria-label={`Safety words: ${words.join(", ")}`}>
      {words.map((w, i) => (
        <span className="word" key={i}>{w}</span>
      ))}
    </span>
  );
}
