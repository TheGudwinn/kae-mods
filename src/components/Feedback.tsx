import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { isTauri } from "../lib/api";
import { openIssue } from "../lib/github";

type Kind = "like" | "dislike" | "idea" | "bug";
const KINDS: Kind[] = ["like", "dislike", "idea", "bug"];

/** A short form that opens a pre-filled issue in the project's repository. Nothing is sent from the app itself. */
export default function Feedback({ build, onClose }: { build: string | null; onClose: () => void }) {
  const { t } = useTranslation();
  const [kind, setKind] = useState<Kind>("idea");
  const [text, setText] = useState("");
  const [withInfo, setWithInfo] = useState(true);
  const [version, setVersion] = useState("dev");
  const [note, setNote] = useState<string | null>(null);

  useEffect(() => {
    if (!isTauri) return;
    void import("@tauri-apps/api/app").then((m) => m.getVersion()).then(setVersion, () => {});
  }, []);

  useEffect(() => {
    const esc = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", esc);
    return () => window.removeEventListener("keydown", esc);
  }, [onClose]);

  const send = async () => {
    const info = withInfo ? `\n\n---\nKae Mods ${version}${build ? ` · game build ${build}` : ""}` : "";
    const fits = await openIssue(`[${t(`feedback.kind_${kind}`)}] ${text.trim().slice(0, 60)}`, `${text.trim()}${info}`);
    setNote(fits ? t("feedback.opened") : t("feedback.tooLong"));
  };

  return (
    <>
      <div className="scrim" onClick={onClose} />
      <div className="modal-wrap" onClick={onClose}>
        <section className="pick feedback" role="dialog" aria-modal="true" aria-label={t("feedback.title")} onClick={(e) => e.stopPropagation()}>
          <h2 className="pick-title">{t("feedback.title")}</h2>
          <p className="note">{t("feedback.text")}</p>
          <div className="seg" role="radiogroup" aria-label={t("feedback.kind")}>
            {KINDS.map((k) => (
              <button key={k} type="button" role="radio" aria-checked={kind === k} className={`seg-btn ${kind === k ? "seg-on" : ""}`} onClick={() => setKind(k)}>
                {t(`feedback.kind_${k}`)}
              </button>
            ))}
          </div>
          <textarea className="field feedback-text" rows={6} maxLength={2000} value={text} placeholder={t(`feedback.hint_${kind}`)} aria-label={t("feedback.title")} onChange={(e) => setText(e.target.value)} />
          <label className="check">
            <input type="checkbox" checked={withInfo} onChange={(e) => setWithInfo(e.target.checked)} />
            <span>{t("feedback.withInfo", { version, build: build ?? "-" })}</span>
          </label>
          <p className="note">{t("feedback.public")}</p>
          {note && <p className="note">{note}</p>}
          <div className="row">
            <button className="btn btn-primary" disabled={text.trim().length < 3} onClick={() => void send()}>
              {t("feedback.send")}
            </button>
            <button className="btn btn-quiet" onClick={onClose}>
              {t("feedback.close")}
            </button>
          </div>
        </section>
      </div>
    </>
  );
}
