import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { call } from "../../lib/api";

type Cfg = { has_curseforge_key: boolean; account_name: string; ms_name: string | null; ms_ready: boolean };
type Device = { user_code: string; verification_uri: string; device_code: string; interval: number; expires_in: number };

/** Storage folder, player name and the CurseForge key: what is shared by all Minecraft packs. */
export default function GeneralSettings({ onChanged }: { onChanged?: () => void }) {
  const { t } = useTranslation();
  const [cfg, setCfg] = useState<Cfg | null>(null);
  const [player, setPlayer] = useState("");
  const [key, setKey] = useState("");
  const [root, setRoot] = useState<{ path: string; is_default: boolean } | null>(null);
  const [note, setNote] = useState<{ ok: boolean; text: string } | null>(null);
  const [device, setDevice] = useState<Device | null>(null);
  const [clientId, setClientId] = useState("");
  const timer = useRef<number | undefined>(undefined);
  useEffect(() => () => window.clearInterval(timer.current), []);

  const load = useCallback(async () => {
    const c = await call<Cfg>("mc_config").catch(() => null);
    setCfg(c);
    if (c) setPlayer(c.account_name);
  }, []);
  useEffect(() => {
    void load();
    void call<{ path: string; is_default: boolean }>("mc_root").then(setRoot, () => {});
  }, [load]);

  const explain = (e: unknown) => {
    const s = String(e);
    if (s === "bad_player_name") return t("mc.badName");
    if (s.startsWith("ms_error:")) return t("mc.ms.error", { error: s.slice(9) });
    return t(`mc.ms.${s}`, { defaultValue: s });
  };
  const run = async (fn: () => Promise<unknown>, done: string) => {
    try {
      await fn();
      await load();
      onChanged?.();
      setNote({ ok: true, text: done });
    } catch (e) {
      setNote({ ok: false, text: explain(e) });
    }
  };
  const chooseRoot = async (reset: boolean) => {
    try {
      let path: string | null = null;
      if (!reset) {
        const { open } = await import("@tauri-apps/plugin-dialog");
        const r = await open({ directory: true, multiple: false, title: t("mc.rootPick") });
        if (!r || Array.isArray(r)) return;
        path = r;
      }
      setRoot(await call("mc_set_root", { path }));
      onChanged?.();
      setNote({ ok: true, text: t("mc.rootSaved") });
    } catch (e) {
      setNote({ ok: false, text: explain(e) });
    }
  };

  const signIn = async () => {
    try {
      const d = await call<Device>("mc_ms_start");
      setDevice(d);
      setNote(null);
      const until = Date.now() + d.expires_in * 1000;
      window.clearInterval(timer.current);
      timer.current = window.setInterval(async () => {
        if (Date.now() > until) {
          window.clearInterval(timer.current);
          setDevice(null);
          setNote({ ok: false, text: t("mc.ms.expired") });
          return;
        }
        try {
          const name = await call<string | null>("mc_ms_poll", { deviceCode: d.device_code });
          if (name) {
            window.clearInterval(timer.current);
            setDevice(null);
            await load();
            onChanged?.();
            setNote({ ok: true, text: t("mc.ms.signedIn", { name }) });
          }
        } catch (e) {
          window.clearInterval(timer.current);
          setDevice(null);
          setNote({ ok: false, text: explain(e) });
        }
      }, d.interval * 1000);
    } catch (e) {
      setNote({ ok: false, text: explain(e) });
    }
  };
  const openPage = async (url: string) => {
    try {
      const { openUrl } = await import("@tauri-apps/plugin-opener");
      await openUrl(url);
    } catch {
      /* the address is shown, it can be opened by hand */
    }
  };

  return (
    <div className="mc-form">
      {note && (
        <p className={note.ok ? "note" : "mc-warn"} role={note.ok ? "status" : "alert"}>
          {note.text}
        </p>
      )}
      <h3 className="mc-sub">{t("mc.storage")}</h3>
      <p className="note">{t("mc.storageNote")}</p>
      <code className="mc-path" title={root?.path}>
        {root?.path ?? "…"}
      </code>
      <div className="mc-row">
        <button className="btn" onClick={() => void chooseRoot(false)}>
          {t("mc.rootChange")}
        </button>
        {root && !root.is_default && (
          <button className="btn btn-quiet" onClick={() => void chooseRoot(true)}>
            {t("mc.rootReset")}
          </button>
        )}
      </div>

      <h3 className="mc-sub">{t("mc.account")}</h3>
      <h4 className="mc-lab">{t("mc.ms.title")}</h4>
      {cfg?.ms_name ? (
        <div className="mc-row">
          <span className="mc-have">{t("mc.ms.signedIn", { name: cfg.ms_name })}</span>
          <button
            className="btn btn-quiet"
            onClick={() =>
              void run(async () => {
                await call("mc_ms_logout");
              }, t("mc.ms.signedOut"))
            }
          >
            {t("mc.ms.signOut")}
          </button>
        </div>
      ) : device ? (
        <div className="mc-device" role="status">
          <p className="note">{t("mc.ms.enter")}</p>
          <strong className="mc-usercode">{device.user_code}</strong>
          <div className="mc-row">
            <button className="btn btn-primary" onClick={() => void openPage(device.verification_uri)}>
              {t("mc.ms.openPage")}
            </button>
            <button className="btn btn-quiet" onClick={() => (window.clearInterval(timer.current), setDevice(null))}>
              {t("mc.cancel")}
            </button>
          </div>
          <p className="note">
            {device.verification_uri} · {t("mc.ms.waiting")}
          </p>
        </div>
      ) : cfg?.ms_ready ? (
        <div className="mc-row">
          <button className="btn btn-primary" onClick={() => void signIn()}>
            {t("mc.ms.signIn")}
          </button>
        </div>
      ) : (
        <>
          <p className="note">{t("mc.ms.needId")}</p>
          <div className="mc-row">
            <input className="field" value={clientId} placeholder="Application (client) ID" aria-label="Application (client) ID" onChange={(e) => setClientId(e.target.value)} />
            <button className="btn" disabled={!clientId.trim()} onClick={() => void run(() => call("mc_save_config", { msClientId: clientId }), t("mc.saved"))}>
              {t("mc.save")}
            </button>
          </div>
        </>
      )}
      <h4 className="mc-lab">{t("mc.ms.offline")}</h4>
      <p className="note">{t("mc.offlineNote")}</p>
      <div className="mc-row">
        <input className="field" value={player} aria-label={t("mc.player")} placeholder={t("mc.player")} onChange={(e) => setPlayer(e.target.value)} />
        <button className="btn" onClick={() => void run(() => call("mc_save_config", { accountName: player }), t("mc.saved"))}>
          {t("mc.save")}
        </button>
      </div>

      <h3 className="mc-sub">CurseForge</h3>
      <p className="note">{t("mc.cfKeyNote")}</p>
      <div className="mc-row">
        <input className="field" type="password" value={key} aria-label={t("mc.cfKey")} placeholder={cfg?.has_curseforge_key ? t("mc.cfKeySet") : t("mc.cfKey")} onChange={(e) => setKey(e.target.value)} />
        <button
          className="btn"
          onClick={() =>
            void run(async () => {
              await call("mc_save_config", { curseforgeKey: key });
              setKey("");
            }, t("mc.saved"))
          }
        >
          {t("mc.save")}
        </button>
      </div>
    </div>
  );
}
