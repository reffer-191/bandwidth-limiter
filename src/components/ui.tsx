import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Box, Smartphone, HelpCircle } from "lucide-react";
import { useAppIcon } from "../lib/engine";
import { api, isTauri } from "../lib/api";

export function Switch({
  on,
  onChange,
  small,
  disabled,
}: {
  on: boolean;
  onChange: (v: boolean) => void;
  small?: boolean;
  disabled?: boolean;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={on}
      disabled={disabled}
      className={`switch ${on ? "on" : ""} ${small ? "small" : ""}`}
      onClick={() => onChange(!on)}
    />
  );
}

export function Segmented<T extends string | number>({
  value,
  options,
  onChange,
  allowReselect = false,
}: {
  value: T;
  options: { value: T; label: string }[];
  onChange: (v: T) => void;
  /** Fire onChange even when the already-active option is clicked. */
  allowReselect?: boolean;
}) {
  return (
    <div className="segmented">
      {options.map((o) => (
        <button
          key={String(o.value)}
          className={o.value === value ? "active" : ""}
          onClick={() => (allowReselect || o.value !== value) && onChange(o.value)}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}

export function AppIcon({
  appKey,
  hasExe,
  isDevice,
  large,
}: {
  appKey: string;
  hasExe: boolean;
  isDevice?: boolean;
  large?: boolean;
}) {
  const icon = useAppIcon(appKey, hasExe);
  return (
    <div className={`app-icon ${large ? "lg" : ""}`}>
      {icon ? (
        <img src={icon} alt="" draggable={false} />
      ) : isDevice ? (
        <Smartphone />
      ) : appKey === "unknown" ? (
        <HelpCircle />
      ) : (
        <Box />
      )}
    </div>
  );
}

/** Classic Windows caption buttons (minimize / maximize-restore / close). */
export function WindowControls({ maximized }: { maximized: boolean }) {
  const win = isTauri ? getCurrentWindow() : null;
  return (
    <div className="win-controls">
      <button className="wc" onClick={() => (isTauri ? api.minimizeWindow() : undefined)} title="Minimizar" aria-label="Minimizar">
        <svg viewBox="0 0 10 10"><path d="M0 5h10" /></svg>
      </button>
      <button className="wc" onClick={() => win?.toggleMaximize()} title={maximized ? "Restaurar" : "Maximizar"} aria-label="Maximizar">
        {maximized ? (
          <svg viewBox="0 0 10 10"><path d="M2 0.5h7.5v7.5H2z M0.5 2v7.5H8" /></svg>
        ) : (
          <svg viewBox="0 0 10 10"><path d="M0.5 0.5h9v9h-9z" /></svg>
        )}
      </button>
      <button className="wc close" onClick={() => win?.close()} title="Cerrar" aria-label="Cerrar">
        <svg viewBox="0 0 10 10"><path d="M0 0l10 10M10 0L0 10" /></svg>
      </button>
    </div>
  );
}

/** Panel width persisted per viewer (localStorage), clamped to [min, max]. */
export function useStoredWidth(key: string, initial: number, min: number, max: number): [number, (w: number) => void] {
  const [width, setWidthState] = useState(() => {
    try {
      const v = Number(localStorage.getItem(`width:${key}`));
      if (v >= min && v <= max) return v;
    } catch {}
    return initial;
  });
  const setWidth = (w: number) => {
    const c = Math.round(Math.min(max, Math.max(min, w)));
    setWidthState(c);
    try {
      localStorage.setItem(`width:${key}`, String(c));
    } catch {}
  };
  return [width, setWidth];
}

/**
 * Invisible grab strip on a panel edge. `side` is the edge it sits on; dragging
 * a "right" edge grows the panel to the right, a "left" edge grows it to the left.
 */
export function ResizeHandle({ side, width, onResize }: { side: "left" | "right"; width: number; onResize: (w: number) => void }) {
  const [dragging, setDragging] = useState(false);
  const onPointerDown = (e: React.PointerEvent<HTMLDivElement>) => {
    if (e.button !== 0) return;
    e.preventDefault();
    const startX = e.clientX;
    const startW = width;
    const el = e.currentTarget;
    el.setPointerCapture(e.pointerId);
    setDragging(true);
    const move = (ev: PointerEvent) => {
      const dx = ev.clientX - startX;
      onResize(side === "right" ? startW + dx : startW - dx);
    };
    const up = () => {
      setDragging(false);
      el.removeEventListener("pointermove", move);
      el.removeEventListener("pointerup", up);
      el.removeEventListener("pointercancel", up);
    };
    el.addEventListener("pointermove", move);
    el.addEventListener("pointerup", up);
    el.addEventListener("pointercancel", up);
  };
  return <div className={`resize-handle ${side} ${dragging ? "dragging" : ""}`} onPointerDown={onPointerDown} onDoubleClick={() => onResize(-1)} />;
}

/** Tracks window focus + maximized state for chrome styling. */
export function useWindowChrome() {
  const [focused, setFocused] = useState(true);
  const [maximized, setMaximized] = useState(false);
  useEffect(() => {
    if (!isTauri) return;
    const win = getCurrentWindow();
    const unsubs: Promise<() => void>[] = [];
    unsubs.push(win.onFocusChanged(({ payload }) => setFocused(payload)));
    unsubs.push(win.onResized(async () => setMaximized(await win.isMaximized())));
    win.isMaximized().then(setMaximized);
    return () => {
      unsubs.forEach((p) => p.then((f) => f()));
    };
  }, []);
  return { focused, maximized };
}
