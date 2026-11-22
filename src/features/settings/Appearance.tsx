import { useTranslation } from "react-i18next";
import { useRef, useState } from "react";
import { WALL_PRESETS, clearWallpaper, pickWallpaper, resetAppearance, setAppearance, useAppearance, useEffectiveWallpaper, useWallpaper, type Appearance as Look } from "../../lib/appearance";

/** A row of mutually exclusive choices, shown as a segmented control. */
function Seg<K extends string>({ label, value, options, onChange }: { label: string; value: K; options: { value: K; label: string }[]; onChange: (v: K) => void }) {
  return (
    <div className="seg" role="radiogroup" aria-label={label}>
      {options.map((o) => (
        <button key={o.value} type="button" role="radio" aria-checked={value === o.value} className={`seg-btn ${value === o.value ? "seg-on" : ""}`} onClick={() => onChange(o.value)}>
          {o.label}
        </button>
      ))}
    </div>
  );
}

function Row({ title, hint, children }: { title: string; hint?: string; children: React.ReactNode }) {
  return (
    <div className="pref">
      <div className="pref-text">
        <span className="pref-title">{title}</span>
        {hint && <span className="pref-hint">{hint}</span>}
      </div>
      <div className="pref-control">{children}</div>
    </div>
  );
}

/** The "how it looks" block of the settings page. Changes apply at once and are remembered on this computer. */
export default function Appearance() {
  const { t } = useTranslation();
  const a = useAppearance();
  const o = <K extends keyof Look>(_key: K, values: Look[K][], tkey: string) => values.map((v) => ({ value: v, label: t(`appearance.${tkey}.${String(v)}`) }));
  const own = useWallpaper();
  const wall = useEffectiveWallpaper();
  const file = useRef<HTMLInputElement>(null);
  const [wallErr, setWallErr] = useState(false);
  const [drag, setDrag] = useState(false);
  const choose = async (f?: File) => {
    if (!f) return;
    setWallErr(false);
    try {
      await pickWallpaper(f);
    } catch {
      setWallErr(true);
    }
  };
  const setHome = (k: keyof Look["home"], v: boolean) => setAppearance({ home: { ...a.home, [k]: v } });

  return (
    <fieldset className="block">
      <legend>{t("appearance.title")}</legend>
      <p className="note">{t("appearance.hint")}</p>

      <Row title={t("appearance.theme")} hint={t("appearance.themeHint")}>
        <Seg label={t("appearance.theme")} value={a.theme} options={o("theme", ["graphite", "deadlock"], "themes")} onChange={(v) => setAppearance({ theme: v })} />
      </Row>

      <div className="wallset">
        <div
          className={`wallprev ${drag ? "is-drag" : ""}`}
          role="button"
          tabIndex={0}
          aria-label={t("appearance.wallDrop")}
          onClick={() => file.current?.click()}
          onKeyDown={(e) => (e.key === "Enter" || e.key === " ") && file.current?.click()}
          onDragOver={(e) => {
            e.preventDefault();
            setDrag(true);
          }}
          onDragLeave={() => setDrag(false)}
          onDrop={(e) => {
            e.preventDefault();
            setDrag(false);
            void choose(e.dataTransfer.files?.[0]);
          }}
        >
          {/* a small copy of the window: the picture sits in the block with the page, not behind the sidebar */}
          <span className="wp-rail" aria-hidden="true">
            <i />
            <i />
            <i />
          </span>
          <span className="wp-block" aria-hidden="true">
            {wall ? (
              <>
                <span className="wp-img" style={{ backgroundImage: `url(${wall})`, filter: `blur(${a.wallBlur * 0.35}px)` }} />
                <span className="wp-dim" style={{ opacity: a.wallDim / 100 }} />
              </>
            ) : (
              <span className="wp-empty">{t("appearance.wallDrop")}</span>
            )}
            <span className="wp-cards">
              <i />
              <i />
              <i />
            </span>
          </span>
        </div>
        <div className="wallset-side">
          <div className="pref-text">
            <span className="pref-title">{t("appearance.wall")}</span>
            <span className="pref-hint">{wallErr ? t("appearance.wallErr") : t("appearance.wallHint")}</span>
          </div>
          {a.theme !== "deadlock" && (
            <div className="wall-presets" role="radiogroup" aria-label={t("appearance.wallPresets")}>
              {(["none", "gradient", "waves"] as const).map((k) => {
                const on = !own && a.wallPreset === k;
                return (
                  <button
                    key={k}
                    type="button"
                    role="radio"
                    aria-checked={on}
                    className={`wall-preset ${on ? "is-on" : ""}`}
                    onClick={() => {
                      clearWallpaper();
                      setAppearance({ wallPreset: k });
                    }}
                  >
                    <span className="wall-preset-img" style={k === "none" ? undefined : { backgroundImage: `url(${WALL_PRESETS[k]})` }} />
                    <span>{t(`appearance.presets.${k}`)}</span>
                  </button>
                );
              })}
            </div>
          )}
          <input ref={file} type="file" accept="image/*" hidden onChange={(e) => { void choose(e.target.files?.[0]); e.target.value = ""; }} />
          <div className="row">
            <button className="btn" type="button" onClick={() => file.current?.click()}>
              {own ? t("appearance.wallChange") : t("appearance.wallPick")}
            </button>
            {own && (
              <button className="btn btn-ghost" type="button" onClick={clearWallpaper}>
                {t("appearance.wallRemove")}
              </button>
            )}
          </div>
          {wall && (
            <div className="wall-sliders">
              <label>
                <span>{t("appearance.wallDim")}</span>
                <output>{a.wallDim}%</output>
                <input className="range" type="range" min={0} max={90} value={a.wallDim} aria-label={t("appearance.wallDim")} onChange={(e) => setAppearance({ wallDim: Number(e.target.value) })} />
              </label>
              <label>
                <span>{t("appearance.wallBlur")}</span>
                <output>{a.wallBlur}px</output>
                <input className="range" type="range" min={0} max={24} value={a.wallBlur} aria-label={t("appearance.wallBlur")} onChange={(e) => setAppearance({ wallBlur: Number(e.target.value) })} />
              </label>
            </div>
          )}
        </div>
      </div>

      <Row title={t("appearance.motion")} hint={t("appearance.motionHint")}>
        <Seg label={t("appearance.motion")} value={a.motion} options={o("motion", ["full", "calm", "off"], "motions")} onChange={(v) => setAppearance({ motion: v })} />
      </Row>

      <Row title={t("appearance.density")}>
        <Seg label={t("appearance.density")} value={a.density} options={o("density", ["compact", "normal", "roomy"], "densities")} onChange={(v) => setAppearance({ density: v })} />
      </Row>

      <Row title={t("appearance.rail")}>
        <Seg label={t("appearance.rail")} value={a.rail} options={o("rail", ["open", "closed"], "rails")} onChange={(v) => setAppearance({ rail: v })} />
      </Row>

      <Row title={t("appearance.home")} hint={t("appearance.homeHint")}>
        <div className="checks">
          {(["hero", "popular", "mine", "stats"] as const).map((k) => (
            <label key={k} className="check">
              <input type="checkbox" checked={a.home[k]} onChange={(e) => setHome(k, e.target.checked)} />
              <span>{t(`appearance.homeBlocks.${k}`)}</span>
            </label>
          ))}
        </div>
      </Row>

      <div className="row">
        <button className="btn" type="button" onClick={resetAppearance}>
          {t("appearance.reset")}
        </button>
      </div>
    </fieldset>
  );
}
