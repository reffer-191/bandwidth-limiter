import { useEffect, useState } from "react";
import { Activity, ShieldCheck, SlidersHorizontal } from "lucide-react";
import { useT } from "../lib/i18n";

/** Three-step introduction shown on the first run (and from Settings). */
export function Onboarding({ onDone }: { onDone: () => void }) {
  const t = useT();
  const [step, setStep] = useState(0);
  const steps = [
    { icon: <Activity />, title: t("ob.1.title"), body: t("ob.1.body") },
    { icon: <ShieldCheck />, title: t("ob.2.title"), body: t("ob.2.body") },
    { icon: <SlidersHorizontal />, title: t("ob.3.title"), body: t("ob.3.body") },
  ];
  const last = step === steps.length - 1;

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onDone();
      if (e.key === "ArrowRight" || e.key === "Enter") (last ? onDone() : setStep((s) => s + 1));
      if (e.key === "ArrowLeft") setStep((s) => Math.max(0, s - 1));
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [last, onDone]);

  const s = steps[step];
  return (
    <div className="modal-backdrop" role="dialog" aria-modal="true" aria-labelledby="ob-title">
      <div className="modal onboarding">
        <div className="ob-icon">{s.icon}</div>
        <h2 id="ob-title">{s.title}</h2>
        <p>{s.body}</p>
        <div className="ob-dots" aria-hidden="true">
          {steps.map((_, i) => <i key={i} className={i === step ? "on" : ""} />)}
        </div>
        <div className="ob-actions">
          {step > 0 ? (
            <button className="btn ghost" onClick={() => setStep(step - 1)}>{t("ob.back")}</button>
          ) : (
            <button className="btn ghost" onClick={onDone}>{t("ob.skip")}</button>
          )}
          <button className="btn primary" autoFocus onClick={() => (last ? onDone() : setStep(step + 1))}>
            {last ? t("ob.done") : t("ob.next")}
          </button>
        </div>
      </div>
    </div>
  );
}
