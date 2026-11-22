import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { call, type Mod, type ModDetails } from "../../lib/api";
import { openExternal } from "../../lib/open";
import Icon from "../../components/Icons";
import { fileExt, formatBytes, htmlToText, humanizeError, isInstallable } from "../../lib/format";

type Props = {
  mod: Mod;
  installed: boolean;
  /** Already waiting in the download queue (or downloading / installing). */
  queued: boolean;
  busy: boolean;
  onClose: () => void;
  onInstall: (mod: Mod, fileId: string) => void;
};

export default function ModDrawer({ mod, installed, queued, busy, onClose, onInstall }: Props) {
  const { t } = useTranslation();
  const [d, setD] = useState<ModDetails | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [shot, setShot] = useState(0); // screenshot shown in the header
  const [zoom, setZoom] = useState(false); // full-size viewer
  const [slow, setSlow] = useState(false); // details are taking long: offer a retry
  const [attempt, setAttempt] = useState(0);
  const [closing, setClosing] = useState(false);
  const close = () => {
    setClosing(true);
    window.setTimeout(onClose, 200);
  };

  useEffect(() => {
    let stale = false;
    setD(null);
    setError(null);
    setShot(0);
    setZoom(false);
    setSlow(false);
    const timer = window.setTimeout(() => setSlow(true), 10000);
    call<ModDetails>("mod_details", { id: mod.id }).then(
      (r) => !stale && setD(r),
      (e) => !stale && setError(humanizeError(t, e)),
    );
    return () => {
      stale = true;
      window.clearTimeout(timer);
    };
  }, [mod.id, t, attempt]);

  const shots = d?.images.length ? d.images : mod.thumbnail ? [mod.thumbnail] : [];
  const step = (by: number) => setShot((i) => (shots.length ? (i + by + shots.length) % shots.length : 0));

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        // The viewer closes first, then the drawer.
        if (zoom) setZoom(false);
        else close();
      } else if (shots.length > 1 && (e.key === "ArrowRight" || e.key === "ArrowLeft")) {
        step(e.key === "ArrowRight" ? 1 : -1);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
    // eslint-disable-next-line react-hooks/exhaustive-deps -- step only depends on shots.length
  }, [onClose, zoom, shots.length]);

  const image = shots[Math.min(shot, Math.max(0, shots.length - 1))] ?? null;
  const desc = d?.description_html ? htmlToText(d.description_html) : "";

  return (
    <>
      <div className={`scrim ${closing ? "is-closing" : ""}`} onClick={close} />
      <div className={`modal-wrap ${closing ? "is-closing" : ""}`} onClick={close}>
        <section className="modal" role="dialog" aria-modal="true" aria-label={mod.name} onClick={(e) => e.stopPropagation()}>
          <div className="modal-media">
            {image ? (
              <button type="button" className="modal-shot" onClick={() => setZoom(true)} aria-label={t("drawer.openShot")} title={t("drawer.openShot")}>
                <img src={image} alt="" />
              </button>
            ) : (
              <div className="modal-shot modal-shot-empty" aria-hidden="true">
                {mod.name.slice(0, 1)}
              </div>
            )}
            {shots.length > 1 && (
              <div className="shots" role="list" aria-label={t("drawer.screens", { count: shots.length })}>
                {shots.map((u, i) => (
                  <button key={u} type="button" role="listitem" className={`shot ${i === shot ? "shot-on" : ""}`} aria-label={`${t("drawer.screen")} ${i + 1}`} onClick={() => setShot(i)}>
                    <img src={u} alt="" loading="lazy" decoding="async" />
                  </button>
                ))}
              </div>
            )}
          </div>

          <div className="modal-info">
            <header className="modal-head">
              <h2>{mod.name}</h2>
              <button className="btn-icon modal-close" onClick={close} autoFocus aria-label={t("drawer.close")} title={t("drawer.close")}>
                <Icon name="close" size={18} />
              </button>
            </header>
            <div className="chips">
              {[mod.author, mod.category, mod.version && `v${mod.version}`].filter(Boolean).map((c) => (
                <span key={String(c)} className="chip">
                  {c}
                </span>
              ))}
              {d?.downloads != null && <span className="chip stat">{t("drawer.downloads", { count: d.downloads })}</span>}
            </div>
            {desc && <p className="desc">{desc}</p>}
            <button className="link" onClick={() => void openExternal(mod.page_url)}>
              {t("drawer.openPage")}
            </button>

            <h3 className="modal-label">{t("drawer.files")}</h3>
            {error && <p className="note note-warn">{error}</p>}
            {!d && !error && <div className="files-skeleton" aria-hidden="true" />}
            {!d && !error && slow && (
              <p className="note note-warn">
                {t("drawer.slow")}{" "}
                <button className="link" onClick={() => setAttempt((n) => n + 1)}>
                  {t("drawer.retry")}
                </button>
              </p>
            )}
            {error && (
              <button className="btn btn-sm" onClick={() => setAttempt((n) => n + 1)}>
                {t("drawer.retry")}
              </button>
            )}
            {d && d.files.length === 0 && <p className="note">{t("drawer.noFiles")}</p>}
            <ul className="files">
              {d?.files.map((f) => {
                const ok = isInstallable(f.name);
                return (
                  <li className="file" key={f.id}>
                    <div className="file-info">
                      <div className="file-name">{f.name}</div>
                      <div className="file-sub">
                        {formatBytes(f.size)}
                        {!f.av_clean && (
                          <>
                            {" · "}
                            <span className="av-warn">{t("drawer.av")}</span>
                          </>
                        )}
                      </div>
                      {!ok && <div className="file-sub">{t("drawer.unsupported", { ext: fileExt(f.name) || "?" })}</div>}
                    </div>
                    <button className={`btn ${installed || queued ? "" : "btn-primary"}`} disabled={!ok || installed || queued || busy} onClick={() => onInstall(mod, f.id)}>
                      {installed ? t("drawer.installed") : queued ? t("drawer.queued") : t("drawer.install")}
                    </button>
                  </li>
                );
              })}
            </ul>
          </div>
        </section>
      </div>
      {zoom && image && (
        <div className="lightbox" role="dialog" aria-modal="true" aria-label={mod.name} onClick={() => setZoom(false)}>
          <img src={image} alt="" onClick={(e) => e.stopPropagation()} />
          {shots.length > 1 && (
            <>
              <button
                className="lb-nav lb-prev"
                aria-label={t("drawer.prev")}
                onClick={(e) => {
                  e.stopPropagation();
                  step(-1);
                }}
              >
                <Icon name="left" size={26} />
              </button>
              <button
                className="lb-nav lb-next"
                aria-label={t("drawer.next")}
                onClick={(e) => {
                  e.stopPropagation();
                  step(1);
                }}
              >
                <Icon name="right" size={26} />
              </button>
              <span className="lb-count">
                {Math.min(shot, shots.length - 1) + 1} / {shots.length}
              </span>
            </>
          )}
          <button className="btn-icon lb-close" onClick={() => setZoom(false)} aria-label={t("drawer.close")}>
            <Icon name="close" size={18} />
          </button>
        </div>
      )}
    </>
  );
}
