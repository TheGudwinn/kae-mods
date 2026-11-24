import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import i18n from "../../i18n";
import { call, isTauri, type GameInstall, type HostReport, type ProxyMode, type SettingsView, type UpdateInfo } from "../../lib/api";
import { humanizeError } from "../../lib/format";
import GameOptions from "./GameOptions";
import Presence from "./Presence";
import McSettings from "./McSettings";
import Appearance from "./Appearance";
import Select from "../../components/Select";

type Props = {
  game: GameInstall | null | undefined;
  onSaved: () => void;
  /** Runs a manual update check (also shows the banner when something is found). */
  onCheckUpdate: () => Promise<UpdateInfo | null>;
};

type Form = {
  mode: ProxyMode;
  kind: "http" | "https" | "socks5";
  host: string;
  port: string;
  username: string;
  password: string;
  clearPw: boolean;
  mirrors: string;
  gamePath: string;
  checkUpdates: boolean;
  endpoints: string;
};

const fromView = (v: SettingsView): Form => ({
  mode: v.proxy.mode,
  kind: v.proxy.kind,
  host: v.proxy.host,
  port: v.proxy.port ? String(v.proxy.port) : "",
  username: v.proxy.username ?? "",
  password: "",
  clearPw: false,
  mirrors: v.mirrors.join("\n"),
  gamePath: v.game_path ?? "",
  checkUpdates: v.check_updates,
  endpoints: v.update_endpoints.join("\n"),
});

export default function Settings({ game, onSaved, onCheckUpdate }: Props) {
  const { t, i18n: i } = useTranslation();
  const [view, setView] = useState<SettingsView | null>(null);
  const [form, setForm] = useState<Form | null>(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [savedNote, setSavedNote] = useState(false);
  const [report, setReport] = useState<HostReport[] | null>(null);
  const [checking, setChecking] = useState(false);
  const [version, setVersion] = useState<string | null>(null);
  const [updateNote, setUpdateNote] = useState<string | null>(null);
  const [updChecking, setUpdChecking] = useState(false);

  useEffect(() => {
    call<string>("app_version").then(setVersion, () => {});
    call<SettingsView>("get_settings").then(
      (v) => {
        setView(v);
        setForm(fromView(v));
      },
      (e) => setError(humanizeError(t, e)),
    );
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const dirty = useMemo(() => !!view && !!form && JSON.stringify(form) !== JSON.stringify(fromView(view)), [view, form]);

  if (!form || !view) {
    return (
      <section className="view">
        <header className="view-head">
          <h1 className="view-title">{t("settings.title")}</h1>
          {error && <p className="note note-warn">{error}</p>}
        </header>
      </section>
    );
  }

  const set = <K extends keyof Form>(k: K, v: Form[K]) => {
    setForm({ ...form, [k]: v });
    setSavedNote(false);
  };

  async function save(e: React.FormEvent) {
    e.preventDefault();
    if (!form) return;
    setError(null);
    const port = Number(form.port);
    if (form.mode === "custom" && !(Number.isInteger(port) && port >= 1 && port <= 65535)) {
      setError(t("errors.proxy_incomplete"));
      return;
    }
    setSaving(true);
    try {
      const next = await call<SettingsView>("save_settings", {
        input: {
          proxy: {
            mode: form.mode,
            kind: form.kind,
            host: form.host,
            port: form.port ? port : 0,
            username: form.username.trim() || null,
            password: form.clearPw ? "" : form.password ? form.password : null,
          },
          mirrors: form.mirrors.split("\n"),
          game_path: form.gamePath.trim() || null,
          check_updates: form.checkUpdates,
          update_endpoints: form.endpoints.split("\n"),
        },
      });
      setView(next);
      setForm(fromView(next));
      setSavedNote(true);
      setReport(null);
      onSaved();
    } catch (err) {
      setError(humanizeError(t, err));
    } finally {
      setSaving(false);
    }
  }

  async function pickFolder() {
    if (!isTauri) return;
    const { open } = await import("@tauri-apps/plugin-dialog");
    const picked = await open({ directory: true, multiple: false, title: t("settings.game.pickTitle") });
    if (typeof picked === "string") set("gamePath", picked);
  }

  async function check() {
    setChecking(true);
    setError(null);
    try {
      setReport(await call<HostReport[]>("run_diagnostics"));
    } catch (err) {
      setError(humanizeError(t, err));
    } finally {
      setChecking(false);
    }
  }

  async function checkNow() {
    setUpdChecking(true);
    setUpdateNote(null);
    setError(null);
    try {
      const info = await onCheckUpdate();
      setUpdateNote(info ? t("settings.updates.found", { version: info.version }) : t("settings.updates.latest"));
    } catch (err) {
      setError(humanizeError(t, err));
    } finally {
      setUpdChecking(false);
    }
  }

  const custom = form.mode === "custom";
  const lang = i.language.startsWith("en") ? "en" : "ru";

  return (
    <section className="view">
      <header className="view-head">
        <h1 className="view-title">{t("settings.title")}</h1>
        <p className="view-sub">{t("settings.subtitle")}</p>
      </header>

      <div className="settings">
        <Appearance />
        <Presence />
        <McSettings />
      </div>

      <form className="settings" onSubmit={save}>
        <fieldset className="block">
          <legend>{t("settings.game.title")}</legend>
          <p className="note">
            {game ? t("settings.game.current", { path: game.root }) : t("settings.game.none")}
          </p>
          {game?.build_id && <p className="note">{t("game.build", { id: game.build_id })}</p>}
          <div className="row">
            <input
              className="field grow mono"
              value={form.gamePath}
              placeholder={t("settings.game.placeholder")}
              aria-label={t("settings.game.title")}
              spellCheck={false}
              onChange={(e) => set("gamePath", e.target.value)}
            />
            {isTauri && (
              <button className="btn" type="button" onClick={() => void pickFolder()}>
                {t("settings.game.pick")}
              </button>
            )}
            {form.gamePath && (
              <button className="btn btn-quiet" type="button" onClick={() => set("gamePath", "")}>
                {t("settings.game.auto")}
              </button>
            )}
          </div>
          <p className="note">{t("settings.game.hint")}</p>
        </fieldset>

        <fieldset className="block">
          <legend>{t("settings.proxy.title")}</legend>
          <div className="choices" role="radiogroup" aria-label={t("settings.proxy.title")}>
            {(["system", "none", "custom"] as const).map((m) => (
              <label key={m} className={`choice ${form.mode === m ? "choice-on" : ""}`}>
                <input type="radio" name="proxy-mode" checked={form.mode === m} onChange={() => set("mode", m)} />
                <span className="choice-name">{t(`settings.proxy.mode.${m}`)}</span>
                <span className="choice-desc">{t(`settings.proxy.modeHint.${m}`)}</span>
              </label>
            ))}
          </div>

          {custom && (
            <div className="grid-fields">
              <label>
                <span>{t("settings.proxy.kind")}</span>
                <Select
                  label={t("settings.proxy.kind")}
                  value={form.kind}
                  onChange={(v) => set("kind", v as Form["kind"])}
                  options={[
                    { value: "http", label: "HTTP" },
                    { value: "https", label: "HTTPS" },
                    { value: "socks5", label: "SOCKS5" },
                  ]}
                />
              </label>
              <label className="span-2">
                <span>{t("settings.proxy.host")}</span>
                <input className="field mono" value={form.host} placeholder="127.0.0.1" spellCheck={false} onChange={(e) => set("host", e.target.value)} />
              </label>
              <label>
                <span>{t("settings.proxy.port")}</span>
                <input className="field mono" inputMode="numeric" value={form.port} placeholder="1080" onChange={(e) => set("port", e.target.value.replace(/\D/g, "").slice(0, 5))} />
              </label>
              <label className="span-2">
                <span>{t("settings.proxy.user")}</span>
                <input className="field" value={form.username} autoComplete="off" onChange={(e) => set("username", e.target.value)} />
              </label>
              <label className="span-2">
                <span>{t("settings.proxy.password")}</span>
                <input
                  className="field"
                  type="password"
                  autoComplete="new-password"
                  value={form.password}
                  disabled={form.clearPw}
                  placeholder={view.proxy.has_password && !form.clearPw ? t("settings.proxy.passwordKept") : ""}
                  onChange={(e) => set("password", e.target.value)}
                />
              </label>
              {view.proxy.has_password && (
                <label className="check span-4">
                  <input type="checkbox" checked={form.clearPw} onChange={(e) => set("clearPw", e.target.checked)} />
                  <span>{t("settings.proxy.forget")}</span>
                </label>
              )}
              <p className="note span-4">{t("settings.proxy.plain")}</p>
            </div>
          )}
        </fieldset>

        <fieldset className="block">
          <legend>{t("settings.mirrors.title")}</legend>
          <textarea
            className="field mono"
            rows={4}
            value={form.mirrors}
            spellCheck={false}
            aria-label={t("settings.mirrors.title")}
            onChange={(e) => set("mirrors", e.target.value)}
          />
          <p className="note">{t("settings.mirrors.hint")}</p>
        </fieldset>

        <fieldset className="block">
          <legend>{t("settings.updates.title")}</legend>
          <p className="note">{version ? t("settings.updates.current", { version }) : ""}</p>
          <label className="check">
            <input type="checkbox" checked={form.checkUpdates} onChange={(e) => set("checkUpdates", e.target.checked)} />
            <span>{t("settings.updates.auto")}</span>
          </label>
          <textarea
            className="field mono"
            rows={3}
            value={form.endpoints}
            spellCheck={false}
            placeholder={t("settings.updates.none")}
            aria-label={t("settings.updates.endpoints")}
            onChange={(e) => set("endpoints", e.target.value)}
          />
          <p className="note">{t("settings.updates.hint")}</p>
          <div className="row">
            <button className="btn" type="button" disabled={updChecking || dirty} onClick={() => void checkNow()}>
              {updChecking ? t("settings.diag.running") : t("settings.updates.check")}
            </button>
            {dirty && <span className="note">{t("settings.diag.saveFirst")}</span>}
            {updateNote && !dirty && <span className="saved">{updateNote}</span>}
          </div>
        </fieldset>

        <div className="save-row">
          <button className="btn btn-primary" type="submit" disabled={saving || !dirty}>
            {t("settings.save")}
          </button>
          {savedNote && !dirty && <span className="saved">{t("settings.saved")}</span>}
          {error && (
            <span className="form-error" role="alert">
              {error}
            </span>
          )}
        </div>
      </form>

      <section className="block diag">
        <h2 className="block-title">{t("settings.diag.title")}</h2>
        <p className="note">{t("settings.diag.hint")}</p>
        <div className="row">
          <button className="btn" disabled={checking || dirty} onClick={() => void check()}>
            {checking ? t("settings.diag.running") : t("settings.diag.run")}
          </button>
          {dirty && <span className="note">{t("settings.diag.saveFirst")}</span>}
        </div>

        {report && (
          <ul className="hosts">
            {report.map((h) => (
              <li key={h.url} className={`host host-${h.verdict === "ok" ? "ok" : "bad"}`}>
                <div className="host-head">
                  <span className="host-name">{h.host || h.url}</span>
                  <span className="host-verdict">{t(`settings.diag.verdict.${h.verdict}.title`)}</span>
                </div>
                <ol className="steps">
                  {h.steps.map((s) => (
                    <li key={s.name} className={`step step-${s.state}`}>
                      <span className="step-mark" aria-hidden="true" />
                      <span className="step-name">{t(`settings.diag.step.${s.name}`)}</span>
                      <span className="step-detail">{s.state === "skipped" ? t("settings.diag.viaProxy") : s.detail}</span>
                      <span className="step-ms">{s.ms != null ? `${s.ms} ${t("settings.diag.ms")}` : ""}</span>
                    </li>
                  ))}
                </ol>
                {h.verdict !== "ok" && <p className="note">{t(`settings.diag.verdict.${h.verdict}.advice`)}</p>}
              </li>
            ))}
          </ul>
        )}
      </section>

      <GameOptions />

      <section className="block">
        <h2 className="block-title">{t("settings.language.title")}</h2>
        <div className="row">
          {(["ru", "en"] as const).map((l) => (
            <button
              key={l}
              className={`btn ${lang === l ? "btn-primary" : ""}`}
              aria-pressed={lang === l}
              onClick={() => {
                void i18n.changeLanguage(l);
                try {
                  localStorage.setItem("lang", l);
                } catch {
                  /* storage can be unavailable */
                }
              }}
            >
              {t(`settings.language.${l}`)}
            </button>
          ))}
        </div>
      </section>
    </section>
  );
}
