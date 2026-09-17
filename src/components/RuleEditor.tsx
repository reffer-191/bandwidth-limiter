import { useEffect, useState } from "react";
import { ArrowDown, ArrowUp } from "lucide-react";
import type { Limit, Rule } from "../lib/api";
import { formatRate, unitOptions, type Units } from "../lib/format";
import { useT } from "../lib/i18n";
import { Switch } from "./ui";

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

export function RuleEditor({
  rule,
  units,
  onChange,
  showBlock = true,
  minRate = MIN_RATE_APP,
}: {
  rule: Rule;
  units: Units;
  onChange: (r: Rule) => void;
  showBlock?: boolean;
  /** Rates below this are refused by the engine; the editor warns about them. */
  minRate?: number;
}) {
  const t = useT();
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
    </div>
  );
}
