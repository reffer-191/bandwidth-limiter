import { useEffect, useState } from "react";
import { ArrowDown, ArrowUp, ChevronRight } from "lucide-react";
import type { Limit, Quota, Rule, RuleState, Schedule } from "../lib/api";
import { formatBytes, formatRate, unitOptions, type Units } from "../lib/format";
import { useT } from "../lib/i18n";
import { Segmented, Switch } from "./ui";

/** Number input that keeps its own text while the user types. */
function RateField({
  limit,
  units,
  onChange,
  warn = false,
}: {
  limit: Limit;
  units: Units;
  onChange: (l: Limit) => void;
  warn?: boolean;
}) {
  const opts = unitOptions(units);
  const pick = (rate: number) => {
    // Prefer the larger unit when it yields a value >= 1.
    const big = opts[1];
    return rate >= big.factor ? big : opts[0];
  };
  const [unit, setUnit] = useState(pick(limit.rate).label);
  const factor = opts.find((o) => o.label === unit)?.factor ?? opts[0].factor;
  const [text, setText] = useState(fmt(limit.rate / factor));

  useEffect(() => {
    // Sync when units system changes or the rule is replaced externally.
    const u = pick(limit.rate);
    setUnit(u.label);
    setText(fmt(limit.rate / u.factor));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [units, limit.rate]);

  const commit = (t: string, f: number) => {
    const v = parseFloat(t.replace(",", "."));
    if (isFinite(v) && v > 0) onChange({ ...limit, rate: Math.round(v * f) });
  };

  return (
    <div className={`field ${warn ? "warn" : ""}`}>
      <input
        type="number"
        min={0}
        step="any"
        value={text}
        disabled={!limit.enabled}
        onChange={(e) => setText(e.target.value)}
        onBlur={() => commit(text, factor)}
        onKeyDown={(e) => e.key === "Enter" && (e.target as HTMLInputElement).blur()}
      />
      <select
        value={unit}
        disabled={!limit.enabled}
        onChange={(e) => {
          const next = opts.find((o) => o.label === e.target.value)!;
          setUnit(next.label);
          setText(fmt(limit.rate / next.factor));
        }}
      >
        {opts.map((o) => (
          <option key={o.label} value={o.label}>
            {o.label}
          </option>
        ))}
      </select>
    </div>
  );
}

function fmt(n: number) {
  if (!isFinite(n)) return "0";
  return n >= 100 ? n.toFixed(0) : Number(n.toFixed(2)).toString();
}

const DEFAULT_DL = 125_000; // 1 Mbit/s
const DEFAULT_UL = 125_000; // 1 Mbit/s — same unit as download so a typed number means the same thing in both rows

/** Below these rates a limit turns into a de-facto block (ACKs starve). */
export const MIN_RATE_GENERAL = 16_000; // 128 kbit/s for the whole PC / hotspot
export const MIN_RATE_APP = 1_000; // 8 kbit/s for a single app

/** Enabling a limit whose rate is still 0 would block everything; seed it. */
function enable(l: Limit, v: boolean, fallback: number): Limit {
  return { enabled: v, rate: v && l.rate <= 0 ? fallback : l.rate };
}

const toTime = (m: number) => `${String(Math.floor(m / 60)).padStart(2, "0")}:${String(m % 60).padStart(2, "0")}`;
const fromTime = (s: string) => {
  const [h, m] = s.split(":").map(Number);
  return isFinite(h) && isFinite(m) ? Math.min(23 * 60 + 59, h * 60 + m) : 0;
};

function ScheduleEditor({ schedule, onChange, state }: { schedule: Schedule; onChange: (s: Schedule) => void; state?: RuleState }) {
  const t = useT();
  const days = t("rule.days").split(",");
  return (
    <div className="adv-block">
      <div className="adv-row">
        <Switch small label={t("rule.schedule")} on={schedule.enabled} onChange={(enabled) => onChange({ ...schedule, enabled })} />
        <span className="adv-label">{t("rule.schedule")}</span>
      </div>
      {schedule.enabled && (
        <>
          <div className="adv-row">
            <div className="days" role="group">
              {days.map((d, i) => (
                <button
                  key={i}
                  type="button"
                  className={`day ${schedule.days[i] ? "on" : ""}`}
                  aria-pressed={schedule.days[i]}
                  onClick={() => {
                    const next = [...schedule.days];
                    next[i] = !next[i];
                    onChange({ ...schedule, days: next });
                  }}
                >
                  {d}
                </button>
              ))}
            </div>
          </div>
          <div className="adv-row">
            <span className="adv-label">{t("rule.schedule.from")}</span>
            <div className="field time">
              <input type="time" value={toTime(schedule.from)} onChange={(e) => onChange({ ...schedule, from: fromTime(e.target.value) })} />
            </div>
            <span className="adv-label">{t("rule.schedule.to")}</span>
            <div className="field time">
              <input type="time" value={toTime(schedule.to)} onChange={(e) => onChange({ ...schedule, to: fromTime(e.target.value) })} />
            </div>
          </div>
          <div className={`rule-summary ${state && !state.active ? "warn" : ""}`}>
            {state && !state.active ? t("rule.schedule.inactive") : schedule.from === schedule.to ? t("rule.schedule.allDay") : null}
          </div>
        </>
      )}
    </div>
  );
}

const QUOTA_UNITS = [
  { label: "MB", factor: 1024 ** 2 },
  { label: "GB", factor: 1024 ** 3 },
];

function QuotaEditor({ quota, onChange, state }: { quota: Quota; onChange: (q: Quota) => void; state?: RuleState }) {
  const t = useT();
  const pick = (b: number) => (b >= QUOTA_UNITS[1].factor ? QUOTA_UNITS[1] : QUOTA_UNITS[0]);
  const [unit, setUnit] = useState(pick(quota.bytes).label);
  const factor = QUOTA_UNITS.find((u) => u.label === unit)?.factor ?? QUOTA_UNITS[0].factor;
  const [text, setText] = useState(fmt(quota.bytes / factor));
  useEffect(() => {
    const u = pick(quota.bytes);
    setUnit(u.label);
    setText(fmt(quota.bytes / u.factor));
  }, [quota.bytes]);
  const commit = () => {
    const v = parseFloat(text.replace(",", "."));
    if (isFinite(v) && v > 0) onChange({ ...quota, bytes: Math.round(v * factor) });
  };
  const used = state?.quotaUsed ?? 0;
  const pct = quota.bytes > 0 ? Math.min(100, Math.round((used / quota.bytes) * 100)) : 0;
  return (
    <div className="adv-block">
      <div className="adv-row">
        <Switch small label={t("rule.quota")} on={quota.enabled} onChange={(enabled) => onChange({ ...quota, enabled })} />
        <span className="adv-label">{t("rule.quota")}</span>
      </div>
      {quota.enabled && (
        <>
          <div className="adv-row">
            <div className="field">
              <input type="number" min={0} step="any" value={text} onChange={(e) => setText(e.target.value)} onBlur={commit} onKeyDown={(e) => e.key === "Enter" && (e.target as HTMLInputElement).blur()} />
              <select
                value={unit}
                onChange={(e) => {
                  const next = QUOTA_UNITS.find((u) => u.label === e.target.value)!;
                  setUnit(next.label);
                  setText(fmt(quota.bytes / next.factor));
                }}
              >
                {QUOTA_UNITS.map((u) => (
                  <option key={u.label} value={u.label}>{u.label}</option>
                ))}
              </select>
            </div>
            <div className="field">
              <select value={quota.period} onChange={(e) => onChange({ ...quota, period: e.target.value as Quota["period"] })} style={{ borderLeft: "none" }}>
                <option value="day">{t("rule.quota.day")}</option>
                <option value="week">{t("rule.quota.week")}</option>
                <option value="month">{t("rule.quota.month")}</option>
              </select>
            </div>
          </div>
          <div className="adv-row">
            <Segmented<Quota["action"]>
              value={quota.action}
              onChange={(action) => onChange({ ...quota, action })}
              options={[
                { value: "block", label: t("rule.quota.block") },
                { value: "notify", label: t("rule.quota.notify") },
              ]}
            />
          </div>
          {state && (
            <div className={`quota-bar ${state.quotaExceeded ? "over" : ""}`} title={t("rule.quota.hint")}>
              <div className="track"><div className="fill" style={{ width: `${pct}%` }} /></div>
              <span>{state.quotaExceeded ? t("rule.quota.exceeded") + " · " : ""}{t("rule.quota.used", { used: formatBytes(used), quota: formatBytes(quota.bytes), pct })}</span>
            </div>
          )}
        </>
      )}
    </div>
  );
}

export function RuleEditor({
  rule,
  units,
  onChange,
  showBlock = true,
  showPriority = false,
  showAdvanced = true,
  minRate = MIN_RATE_APP,
  state,
}: {
  rule: Rule;
  units: Units;
  onChange: (r: Rule) => void;
  showBlock?: boolean;
  /** Priority only makes sense for apps/devices sharing a general limit. */
  showPriority?: boolean;
  /** Schedule + quota (+ priority) disclosure. */
  showAdvanced?: boolean;
  /** Rates below this are refused by the engine; the editor warns about them. */
  minRate?: number;
  /** Live schedule/quota state from the engine, when known. */
  state?: RuleState;
}) {
  const t = useT();
  const hasAdvanced = rule.priority !== "normal" || rule.schedule.enabled || rule.quota.enabled;
  const [open, setOpen] = useState(hasAdvanced);
  useEffect(() => {
    if (hasAdvanced) setOpen(true);
  }, [hasAdvanced]);
  const low = (l: Limit) => l.enabled && l.rate < minRate;
  const summary = [
    rule.blockDl ? `↓ ${t("rule.blocked")}` : rule.dl.enabled ? `↓ ${formatRate(rule.dl.rate, units)}` : null,
    rule.blockUl ? `↑ ${t("rule.blocked")}` : rule.ul.enabled ? `↑ ${formatRate(rule.ul.rate, units)}` : null,
  ].filter(Boolean);
  return (
    <div className="rule-editor">
      <div className="rule-row">
        <Switch label={t("download")} on={rule.dl.enabled} onChange={(v) => onChange({ ...rule, dl: enable(rule.dl, v, DEFAULT_DL) })} />
        <span className="name dl">
          <ArrowDown /> {t("download")}
        </span>
        <RateField limit={rule.dl} units={units} onChange={(dl) => onChange({ ...rule, dl })} warn={low(rule.dl)} />
      </div>
      <div className="rule-row">
        <Switch label={t("upload")} on={rule.ul.enabled} onChange={(v) => onChange({ ...rule, ul: enable(rule.ul, v, DEFAULT_UL) })} />
        <span className="name ul">
          <ArrowUp /> {t("upload")}
        </span>
        <RateField limit={rule.ul} units={units} onChange={(ul) => onChange({ ...rule, ul })} warn={low(rule.ul)} />
      </div>
      {(summary.length > 0 || low(rule.dl) || low(rule.ul)) && (
        <div className={`rule-summary ${low(rule.dl) || low(rule.ul) ? "warn" : ""}`}>
          {low(rule.dl) || low(rule.ul)
            ? t("rule.tooLow", { min: formatRate(minRate, units) })
            : `${t("rule.effective")}: ${summary.join(" · ")}`}
        </div>
      )}
      {showBlock && (
        <div className="rule-block">
          <span>{t("rule.block")}</span>
          <div className="opts">
            <label>
              <Switch small on={rule.blockDl} onChange={(v) => onChange({ ...rule, blockDl: v })} /> {t("rule.in")}
            </label>
            <label>
              <Switch small on={rule.blockUl} onChange={(v) => onChange({ ...rule, blockUl: v })} /> {t("rule.out")}
            </label>
          </div>
        </div>
      )}
      {showAdvanced && (
        <div className="rule-advanced">
          <button type="button" className={`disclosure ${open ? "open" : ""}`} onClick={() => setOpen((v) => !v)} aria-expanded={open}>
            <ChevronRight />
            {showPriority ? t("rule.advanced") : t("rule.advanced.noPriority")}
            {!open && hasAdvanced && <span className="dot" />}
          </button>
          {open && (
            <div className="adv">
              {showPriority && (
                <div className="adv-block">
                  <div className="adv-row">
                    <span className="adv-label" style={{ minWidth: 60 }}>{t("rule.priority")}</span>
                    <Segmented<Rule["priority"]>
                      value={rule.priority}
                      onChange={(priority) => onChange({ ...rule, priority })}
                      options={[
                        { value: "high", label: t("rule.priority.high") },
                        { value: "normal", label: t("rule.priority.normal") },
                        { value: "low", label: t("rule.priority.low") },
                      ]}
                    />
                  </div>
                  {rule.priority !== "normal" && <div className="rule-summary">{t("rule.priority.hint")}</div>}
                </div>
              )}
              <ScheduleEditor schedule={rule.schedule} onChange={(schedule) => onChange({ ...rule, schedule })} state={state} />
              <QuotaEditor quota={rule.quota} onChange={(quota) => onChange({ ...rule, quota })} state={state} />
            </div>
          )}
        </div>
      )}
    </div>
  );
}
