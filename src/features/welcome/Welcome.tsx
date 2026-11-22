import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import Icon from "../../components/Icons";
import wall from "../../assets/welcome.webp";

const STEPS = ["find", "pick", "play"] as const;
const BIG = ["catalog", "library"] as const;
const ROUND = ["skins", "profiles", "fixes"] as const;

/** Pictures for the feature tiles: drop catalog / library / skins / profiles / fixes (.webp) into src/assets/welcome. */
const pics = import.meta.glob("../../assets/welcome/*.webp", { eager: true, query: "?url", import: "default" }) as Record<string, string>;
const pic = (id: string) => pics[`../../assets/welcome/${id}.webp`];

/** The first screen a new player sees: what Kae Mods is and what it can do. Shown once; "Skip" and "Start" both close it. */
export default function Welcome({ onDone }: { onDone: (view?: "catalog" | "library") => void }) {
  const { t } = useTranslation();
  // the highlight walks through the steps by itself and follows the pointer when it is over them
  const [step, setStep] = useState(0);
  const [hold, setHold] = useState(false);
  useEffect(() => {
    if (hold || window.matchMedia("(prefers-reduced-motion: reduce)").matches) return;
    const id = window.setInterval(() => setStep((x) => (x + 1) % STEPS.length), 3200);
    return () => window.clearInterval(id);
  }, [hold]);
  return (
    <section className="welcome" role="dialog" aria-modal="true" aria-label={t("welcome.title")}>
      <div className="wl-hero" style={{ backgroundImage: `url(${wall})` }}>
        <div className="wl-top">
          <span />
          <button className="btn wl-skip" onClick={() => onDone()}>
            {t("welcome.skip")}
          </button>
        </div>
        <div className="wl-word" aria-label={t("app.name")}>
          KAE MODS
        </div>
        <div className="wl-flow" onMouseLeave={() => setHold(false)}>
          <p key={step} className="wl-flow-note">
            {t(`welcome.stepsNote.${STEPS[step]}`)}
          </p>
          <ol className="wl-steps" style={{ "--i": step } as React.CSSProperties}>
            {STEPS.map((id, i) => (
              <li key={id}>
                <button
                  type="button"
                  className={i === step ? "is-on" : i < step ? "is-done" : ""}
                  onMouseEnter={() => {
                    setHold(true);
                    setStep(i);
                  }}
                  onFocus={() => setStep(i)}
                >
                  <b>{String(i + 1).padStart(2, "0")}</b>
                  <span>{t(`welcome.steps.${id}`)}</span>
                </button>
              </li>
            ))}
          </ol>
        </div>
      </div>

      <button className="wl-go" onClick={() => onDone()} aria-label={t("welcome.start")}>
        <span>{t("welcome.start")}</span>
      </button>

      <div className="wl-bottom">
        <div className="wl-intro">
          <h1>{t("welcome.title")}</h1>
          <p>{t("welcome.text")}</p>
        </div>

        <ul className="wl-round">
          {ROUND.map((id) => (
            <li key={id}>
              <span className="wl-dot">{pic(id) && <img src={pic(id)} alt="" />}</span>
              <b>{t(`welcome.features.${id}.title`)}</b>
              <small>{t(`welcome.features.${id}.text`)}</small>
            </li>
          ))}
        </ul>

        <ul className="wl-cards">
          {BIG.map((id) => (
            <li key={id}>
              {pic(id) && <img className="wl-art" src={pic(id)} alt="" />}
              <b>{t(`welcome.features.${id}.title`)}</b>
              <small>{t(`welcome.features.${id}.text`)}</small>
              <button type="button" className="wl-open" onClick={() => onDone(id)} aria-label={t("welcome.open")} title={t("welcome.open")}>
                <Icon name="right" size={18} />
              </button>
            </li>
          ))}
        </ul>
      </div>
    </section>
  );
}
