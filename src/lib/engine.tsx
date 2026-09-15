import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState } from "react";
import { api, onConfig, onTick, type AppRate, type Config, type Sample, type Status, type Tick } from "./api";

const MAX_HISTORY = 3600;

interface EngineCtx {
  tick: Tick | null;
  history: Sample[];
  config: Config | null;
  status: Status | null;
  /** id -> app metadata from the latest tick (for chart tooltips). */
  appsById: Map<number, AppRate>;
  updateConfig: (mutate: (c: Config) => Config) => Promise<void>;
  refreshStatus: () => Promise<void>;
  error: string | null;
}

const Ctx = createContext<EngineCtx | null>(null);

export function EngineProvider({ children }: { children: React.ReactNode }) {
  const [tick, setTick] = useState<Tick | null>(null);
  const [history, setHistory] = useState<Sample[]>([]);
  const [config, setConfig] = useState<Config | null>(null);
  const [status, setStatus] = useState<Status | null>(null);
  const [error, setError] = useState<string | null>(null);
  const historyRef = useRef<Sample[]>([]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let unlistenConfig: (() => void) | undefined;
    let cancelled = false;
    (async () => {
      try {
        const [st, cfg, hist, snap] = await Promise.all([
          api.status(),
          api.config(),
          api.history(),
          api.snapshot(),
        ]);
        if (cancelled) return;
        setStatus(st);
        setConfig(cfg);
        historyRef.current = hist;
        setHistory(hist);
        if (snap) setTick(snap);
        unlistenConfig = await onConfig((c) => setConfig(c));
        unlisten = await onTick((t) => {
          setTick(t);
          const s: Sample = {
            ts: t.ts,
            total: t.total,
            internet: t.internet,
            local: t.local,
            hotspot: t.hotspot,
            top: t.apps
              .filter((a) => a.dl + a.ul > 0)
              .sort((a, b) => b.dl + b.ul - (a.dl + a.ul))
              .slice(0, 24)
              .map((a) => [a.id, { dl: a.dl, ul: a.ul }]),
          };
          const next = historyRef.current.length >= MAX_HISTORY
            ? [...historyRef.current.slice(1), s]
            : [...historyRef.current, s];
          historyRef.current = next;
          setHistory(next);
        });
      } catch (e) {
        setError(String(e));
      }
    })();
    return () => {
      cancelled = true;
      unlisten?.();
      unlistenConfig?.();
    };
  }, []);

  const updateConfig = useCallback(async (mutate: (c: Config) => Config) => {
    const base = config ?? (await api.config());
    const next = mutate(structuredClone(base));
    setConfig(next); // optimistic
    try {
      const saved = await api.setConfig(next);
      setConfig(saved);
    } catch (e) {
      setError(String(e));
      setConfig(base);
    }
  }, [config]);

  const refreshStatus = useCallback(async () => setStatus(await api.status()), []);

  const appsById = useMemo(() => {
    const m = new Map<number, AppRate>();
    tick?.apps.forEach((a) => m.set(a.id, a));
    return m;
  }, [tick]);

  const value = useMemo<EngineCtx>(
    () => ({ tick, history, config, status, appsById, updateConfig, refreshStatus, error }),
    [tick, history, config, status, appsById, updateConfig, refreshStatus, error],
  );
  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}

export function useEngine(): EngineCtx {
  const ctx = useContext(Ctx);
  if (!ctx) throw new Error("useEngine outside EngineProvider");
  return ctx;
}

const iconCache = new Map<string, Promise<string | null>>();

export function useAppIcon(key: string, hasExe: boolean): string | null {
  const [icon, setIcon] = useState<string | null>(null);
  useEffect(() => {
    if (!hasExe) return;
    let p = iconCache.get(key);
    if (!p) {
      p = api.icon(key).catch(() => null);
      iconCache.set(key, p);
    }
    let alive = true;
    p.then((v) => alive && setIcon(v));
    return () => {
      alive = false;
    };
  }, [key, hasExe]);
  return icon;
}
