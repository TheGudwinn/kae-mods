import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import type { TFunction } from "i18next";
import { call } from "../../lib/api";
import Select from "../../components/Select";
import Icon from "../../components/Icons";
import GeneralSettings from "./GeneralSettings";
import Confirm, { type ConfirmState } from "../../components/Confirm";
import type { Cfg, Common, Instance, Launch, McFile, McPage, OpenRequest } from "./Minecraft";

const LOADERS = ["vanilla", "fabric", "quilt", "forge", "neoforge"];
const SUBS = ["mod", "datapack", "resourcepack", "shader", "world"] as const;
type Sub = (typeof SUBS)[number];
type Tab = "content" | "logs" | "shots";
type Shot = { name: string; size: number; modified: number };
type LogChunk = { text: string; offset: number; reset: boolean };
type Update = { kind: string; file: string; new_name: string; version: string };
type Covers = Record<string, string>;
type FileMeta = { kind: string; file: string; project: string; title: string; icon: string | null; author: string; more_authors: number; client: string; server: string };

/** The picture of a pack: a colour taken from its id, since an instance has no art of its own. */
function coverStyle(id: string, uri?: string): React.CSSProperties {
  if (uri) return { backgroundImage: `url(${uri})`, backgroundSize: "cover", backgroundPosition: "center" };
  void id;
  return { background: "var(--ink-0)" };
}

/** A pack without its own picture shows the brand, like a game does. */
function Brand() {
  return (
    <span className="mc-cover-letter mc-brand">
      KAE
      <br />
      MODS
    </span>
  );
}

function Cover({ i, big, uri }: { i: Instance; big?: boolean; uri?: string }) {
  return (
    <div className={`mc-cover ${big ? "mc-cover-big" : ""}`} style={coverStyle(i.id, uri)} aria-hidden>
      {!uri && <Brand />}
    </div>
  );
}

const FOCUSABLE = 'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

function Modal({ title, onClose, children }: { title: string; onClose: () => void; children: ReactNode }) {
  const { t } = useTranslation();
  const box = useRef<HTMLDivElement>(null);
  useEffect(() => {
    // focus goes into the dialog and comes back to what opened it
    const before = document.activeElement as HTMLElement | null;
    const first = box.current?.querySelector<HTMLElement>(".mc-dialog-body " + FOCUSABLE) ?? box.current?.querySelector<HTMLElement>(FOCUSABLE);
    first?.focus();
    return () => before?.focus?.();
  }, []);
  useEffect(() => {
    const h = (e: KeyboardEvent) => {
      if (e.key === "Escape") return onClose();
      if (e.key !== "Tab" || !box.current) return;
      const items = [...box.current.querySelectorAll<HTMLElement>(FOCUSABLE)].filter((x) => x.offsetParent !== null);
      if (items.length === 0) return;
      const a = items[0]!;
      const z = items[items.length - 1]!;
      if (e.shiftKey && document.activeElement === a) (e.preventDefault(), z.focus());
      else if (!e.shiftKey && document.activeElement === z) (e.preventDefault(), a.focus());
    };
    window.addEventListener("keydown", h);
    return () => window.removeEventListener("keydown", h);
  }, [onClose]);
  return (
    <>
      <div className="scrim" onClick={onClose} />
      <div className="modal-wrap" onClick={onClose}>
        <div ref={box} className="mc-dialog" role="dialog" aria-modal="true" aria-label={title} onClick={(e) => e.stopPropagation()}>
          <div className="mc-dialog-head">
            <h2>{title}</h2>
            <button className="btn btn-icon btn-quiet" aria-label={t("mc.close")} onClick={onClose}>
              <Icon name="close" size={18} />
            </button>
          </div>
          {children}
        </div>
      </div>
    </>
  );
}

function store(key: string, fallback: string) {
  try {
    return localStorage.getItem(key) ?? fallback;
  } catch {
    return fallback;
  }
}

function playTime(t: TFunction, secs: number) {
  if (secs < 60) return t("mc.timeLess");
  const h = Math.floor(secs / 3600);
  const m = Math.floor((secs % 3600) / 60);
  return h > 0 ? t("mc.timeHours", { h, m }) : t("mc.timeMinutes", { m });
}

function Console({ id, running, active }: { id: string; running: boolean; active: boolean }) {
  const { t } = useTranslation();
  const [text, setText] = useState("");
  const offset = useRef(0);
  const box = useRef<HTMLPreElement>(null);
  const stick = useRef(true);

  useEffect(() => {
    setText("");
    offset.current = 0;
  }, [id]);
  useEffect(() => {
    if (!active) return;
    let stop = false;
    const tick = async () => {
      try {
        const c = await call<LogChunk>("mc_log", { id, offset: offset.current });
        if (stop) return;
        offset.current = c.offset;
        if (c.reset) setText(c.text);
        else if (c.text) setText((old) => (old + c.text).slice(-600_000));
      } catch {
        /* no log yet */
      }
    };
    void tick();
    const timer = setInterval(tick, running ? 700 : 3000);
    return () => {
      stop = true;
      clearInterval(timer);
    };
  }, [id, running, active]);
  useEffect(() => {
    const el = box.current;
    if (el && stick.current) el.scrollTop = el.scrollHeight;
  }, [text]);
  // the log takes what is left of the window, so the page does not have to be scrolled to reach its end
  useEffect(() => {
    if (!active) return;
    let raf = 0;
    const fit = () => {
      cancelAnimationFrame(raf);
      raf = requestAnimationFrame(() => {
        const el = box.current;
        if (!el) return;
        const top = el.getBoundingClientRect().top;
        el.style.height = `${Math.max(220, Math.floor(window.innerHeight - top - 24))}px`;
      });
    };
    fit();
    window.addEventListener("resize", fit);
    const ro = new ResizeObserver(fit);
    ro.observe(document.body);
    return () => {
      cancelAnimationFrame(raf);
      window.removeEventListener("resize", fit);
      ro.disconnect();
    };
  }, [active]);

  return (
    <div className="mc-console">
      <div className="mc-row">
        <span className="note">{running ? t("mc.consoleLive") : t("mc.consoleIdle")}</span>
        <span className="mc-grow" />
        <button className="btn btn-quiet" onClick={() => void navigator.clipboard.writeText(text).catch(() => {})}>
          {t("mc.copy")}
        </button>
        <button className="btn btn-quiet" onClick={() => setText("")}>
          {t("mc.clear")}
        </button>
        <button className="btn btn-quiet" onClick={() => void call("mc_open_folder", { id, sub: "logs" }).catch(() => {})}>
          {t("mc.menu.openLogs")}
        </button>
      </div>
      <pre
        ref={box}
        className="mc-log"
        role="log"
        aria-live="off"
        aria-label={t("mc.tabs.logs")}
        tabIndex={0}
        onScroll={(e) => {
          const el = e.currentTarget;
          stick.current = el.scrollHeight - el.scrollTop - el.clientHeight < 40;
        }}
      >
        {text || t("mc.logEmpty")}
      </pre>
    </div>
  );
}

/** A screenshot, read when it comes into view. */
function Thumb({ id, name }: { id: string; name: string }) {
  const [src, setSrc] = useState<string | null>(null);
  const box = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    const el = box.current;
    if (!el) return;
    let gone = false;
    const io = new IntersectionObserver((es) => {
      if (es.some((e) => e.isIntersecting)) {
        io.disconnect();
        void call<string>("mc_screenshot", { id, name }).then((s) => !gone && setSrc(s), () => {});
      }
    });
    io.observe(el);
    return () => {
      gone = true;
      io.disconnect();
    };
  }, [id, name]);
  return (
    <button ref={box} className="mc-thumb" title={name} onClick={() => void call("mc_open_screenshot", { id, name }).catch(() => {})}>
      {src ? <img src={src} alt={name} /> : <span className="note">…</span>}
    </button>
  );
}

function Options({ inst, onClose, onSaved, fail }: { inst: Instance; onClose: () => void; onSaved: () => Promise<void>; fail: (e: unknown) => void }) {
  const { t } = useTranslation();
  const [name, setName] = useState(inst.name);
  const [custom, setCustom] = useState(inst.memory_custom);
  const [mem, setMem] = useState(inst.memory_mb);
  const [lv, setLv] = useState(inst.loader_version ?? "");
  const [lvs, setLvs] = useState<string[]>([]);
  const [java, setJava] = useState(inst.java_path ?? "");
  const [args, setArgs] = useState(inst.extra_args);
  const [debug, setDebug] = useState(inst.debug_console);
  const [saving, setSaving] = useState(false);
  const [ram, setRam] = useState(0);
  const [modCount, setModCount] = useState(0);
  useEffect(() => {
    void call<number>("mc_system_memory").then(setRam, () => {});
    void call<McFile[]>("mc_files", { id: inst.id }).then((f) => setModCount(f.filter((x) => x.kind === "mod" && x.enabled).length), () => {});
  }, [inst.id]);
  // advice: more mods need more memory, but never more than about 70% of what the computer has
  const cap = ram ? Math.max(2048, Math.floor((ram * 0.7) / 256) * 256) : 8192;
  const advice = Math.min(cap, modCount < 30 ? 3072 : modCount < 100 ? 5120 : 8192);
  const tooMuch = ram > 0 && custom && mem > ram * 0.75;

  useEffect(() => {
    if (inst.loader === "vanilla") return;
    void call<string[]>("mc_loader_versions", { loader: inst.loader, mcVersion: inst.mc_version }).then(setLvs, () => setLvs([]));
  }, [inst.loader, inst.mc_version]);

  const save = async () => {
    setSaving(true);
    try {
      await call("mc_update_instance", {
        id: inst.id,
        patch: { name, memory_custom: custom, memory_mb: mem, loader_version: lv || null, java_path: java, extra_args: args, debug_console: debug },
      });
      await onSaved();
      onClose();
    } catch (e) {
      fail(e);
    } finally {
      setSaving(false);
    }
  };
  const lvOptions = lvs.includes(lv) || !lv ? lvs : [lv, ...lvs];

  return (
    <Modal title={t("mc.opt.title")} onClose={onClose}>
      <div className="mc-dialog-body mc-opt">
        <section className="mc-sect">
          <label className="mc-lab" htmlFor="mc-opt-name">
            {t("mc.opt.name")}
          </label>
          <input id="mc-opt-name" className="field" value={name} onChange={(e) => setName(e.target.value)} />
        </section>

        <section className="mc-sect">
          <h3 className="mc-sub">{t("mc.opt.memory")}</h3>
          <div className="seg" role="radiogroup" aria-label={t("mc.opt.memory")}>
            <button role="radio" aria-checked={!custom} className={`seg-btn ${!custom ? "seg-on" : ""}`} onClick={() => setCustom(false)}>
              {t("mc.opt.memDefault", { mb: 4096 })}
            </button>
            <button role="radio" aria-checked={custom} className={`seg-btn ${custom ? "seg-on" : ""}`} onClick={() => setCustom(true)}>
              {t("mc.opt.memCustom")}
            </button>
          </div>
          <div className="mc-memrow">
            <input className="mc-range" type="range" min={1024} max={ram ? Math.min(32768, ram) : 32768} step={256} value={mem} disabled={!custom} aria-label={t("mc.opt.memCustom")} onChange={(e) => setMem(Number(e.target.value))} />
            <span className="mc-memfield">
              <input className="field" type="number" min={1024} max={65536} step={256} value={mem} disabled={!custom} aria-label="MB" onChange={(e) => setMem(Number(e.target.value))} />
              <span className="note">MB</span>
            </span>
          </div>
          {ram > 0 && (
            <div className="mc-advice">
              <span className="note">{t("mc.opt.ramInfo", { ram, advice, mods: modCount })}</span>
              <button
                className="btn btn-quiet"
                onClick={() => {
                  setCustom(true);
                  setMem(advice);
                }}
              >
                {t("mc.opt.useAdvice", { mb: advice })}
              </button>
            </div>
          )}
          {tooMuch && (
            <p className="mc-warn" role="alert">
              {t("mc.opt.ramWarn", { ram })}
            </p>
          )}
        </section>

        <section className="mc-sect">
          <h3 className="mc-sub">{t("mc.opt.advanced")}</h3>
          {inst.loader !== "vanilla" && (
            <div className="mc-fieldgroup">
              <span className="mc-lab">{t("mc.opt.loaderVersion", { loader: t(`mc.loaders.${inst.loader}`) })}</span>
              <Select label={t("mc.opt.loaderVersion", { loader: "" })} value={lv} onChange={setLv} options={lvOptions.map((v) => ({ value: v, label: v === lvs[0] ? `${v} · ${t("mc.latest")}` : v }))} />
              {lv && lv !== inst.loader_version && <p className="note">{t("mc.opt.loaderWarn")}</p>}
            </div>
          )}
          <div className="mc-fieldgroup">
            <label className="mc-lab" htmlFor="mc-opt-java">
              {t("mc.opt.java")}
            </label>
            <input id="mc-opt-java" className="field" value={java} placeholder={t("mc.opt.javaDefault")} onChange={(e) => setJava(e.target.value)} />
          </div>
          <div className="mc-fieldgroup">
            <label className="mc-lab" htmlFor="mc-opt-args">
              {t("mc.opt.args")}
            </label>
            <input id="mc-opt-args" className="field" value={args} placeholder="-XX:+UseG1GC" onChange={(e) => setArgs(e.target.value)} />
          </div>
          <div className="mc-switchrow">
            <div className="mc-switchtext">
              <span className="mc-lab" id="mc-opt-debug">
                {t("mc.opt.debug")}
              </span>
              <p className="note">{t("mc.opt.debugHint")}</p>
            </div>
            <button className="switch" role="switch" aria-checked={debug} aria-labelledby="mc-opt-debug" onClick={() => setDebug(!debug)} />
          </div>
        </section>
      </div>
      <div className="mc-dialog-foot">
        <button className="btn" onClick={onClose}>
          {t("mc.cancel")}
        </button>
        <button className="btn btn-primary" disabled={saving || !name.trim()} onClick={() => void save()}>
          {t("mc.done")}
        </button>
      </div>
    </Modal>
  );
}

function Detail(p: Common & { inst: Instance; running: boolean; play: (i: Instance) => Promise<void>; back: () => void; share: (i: Instance) => Promise<void>; onGo: (p: McPage) => void; onAdd: (id: string) => void; launch: Launch | null; cover?: string; reloadCovers: () => void; initialTab: Tab }) {
  const { t } = useTranslation();
  const { inst, running } = p;
  const [tab, setTab] = useState<Tab>(p.initialTab);
  const [sub, setSub] = useState<Sub>("mod");
  const [files, setFiles] = useState<McFile[]>([]);
  const [shots, setShots] = useState<Shot[]>([]);
  const [menu, setMenu] = useState(false);
  const [options, setOptions] = useState(false);
  const [wasRunning, setWasRunning] = useState(running);
  const [updates, setUpdates] = useState<Update[]>([]);
  const [unknown, setUnknown] = useState<string[]>([]);
  const [meta, setMeta] = useState<Record<string, FileMeta>>({});
  const [packUpdate, setPackUpdate] = useState<{ available: boolean; version: string } | null>(null);
  const [checking, setChecking] = useState(false);
  const [updating, setUpdating] = useState<string | null>(null);
  const [shotLimit, setShotLimit] = useState(24);
  const [confirm, setConfirm] = useState<ConfirmState | null>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  const menuList = useRef<HTMLUListElement>(null);

  const check = useCallback(async () => {
    setChecking(true);
    try {
      const rep = await call<{ updates: Update[]; unknown: string[] }>("mc_check_updates", { id: inst.id });
      setUpdates(rep.updates);
      setUnknown(rep.unknown);
    } catch {
      setUpdates([]); // offline or Modrinth is down: no updates are shown
    } finally {
      setChecking(false);
    }
  }, [inst.id]);
  useEffect(() => {
    void check();
  }, [check]);
  useEffect(() => {
    setPackUpdate(null);
    if (inst.source?.startsWith("modrinth:")) void call<{ available: boolean; version: string }>("mc_check_pack_update", { id: inst.id }).then(setPackUpdate, () => {});
  }, [inst.id, inst.source]);

  const load = useCallback(() => {
    void call<McFile[]>("mc_files", { id: inst.id }).then(setFiles, () => setFiles([]));
    void call<Shot[]>("mc_screenshots", { id: inst.id }).then(setShots, () => setShots([]));
  }, [inst.id]);
  useEffect(load, [load, tab]);
  useEffect(() => {
    if (menu) menuList.current?.querySelector<HTMLElement>("button")?.focus();
  }, [menu]);
  useEffect(() => {
    if (!menu) return;
    const h = () => setMenu(false);
    window.addEventListener("click", h);
    return () => window.removeEventListener("click", h);
  }, [menu]);
  // the console opens by itself when the game starts, when it is asked for in the options
  useEffect(() => {
    if (running && !wasRunning && inst.debug_console) setTab("logs");
    setWasRunning(running);
  }, [running, wasRunning, inst.debug_console]);

  const counts = useMemo(() => Object.fromEntries(SUBS.map((s) => [s, files.filter((f) => f.kind === s).length])) as Record<Sub, number>, [files]);
  const shown = files.filter((f) => f.kind === sub);
  // title, picture, author and where it runs, from Modrinth; the rows show the file name until it arrives
  const fileCount = files.length;
  useEffect(() => {
    let gone = false;
    const put = (l: FileMeta[]) => !gone && setMeta(Object.fromEntries(l.map((m) => [m.kind + "/" + m.file.replace(/.disabled$/, ""), m])));
    // the saved answer shows at once, the fresh one replaces it
    void call<FileMeta[]>("mc_file_meta_cached", { id: inst.id }).then(put, () => {});
    void call<FileMeta[]>("mc_file_meta", { id: inst.id }).then(put, () => {});
    return () => {
      gone = true;
    };
  }, [inst.id, fileCount]);
  const act = async (fn: () => Promise<unknown>, after?: () => void) => {
    try {
      await fn();
      after?.();
    } catch (e) {
      p.fail(e);
    }
  };
  const updateOf = (f: McFile) => updates.find((u) => u.kind === f.kind && u.file === f.name);
  const runUpdates = async (list: Update[]) => {
    for (const u of list) {
      setUpdating(u.file);
      try {
        await call("mc_update_file", { id: inst.id, kind: u.kind, name: u.file });
      } catch (e) {
        p.fail(e);
        break;
      }
    }
    setUpdating(null);
    load();
    await check();
  };
  const changeImage = async () => {
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const f = await open({ multiple: false, title: t("mc.menu.changeImage"), filters: [{ name: t("mc.images"), extensions: ["png", "jpg", "jpeg", "webp"] }] });
      if (!f || Array.isArray(f)) return;
      await call("mc_set_cover", { id: inst.id, path: f });
      p.reloadCovers();
    } catch (e) {
      p.fail(e);
    }
  };
  const clone = async () => {
    try {
      const c = await call<Instance>("mc_clone_instance", { id: inst.id, name: null });
      await p.reload();
      p.say(t("mc.cloned", { name: c.name }), c.id);
    } catch (e) {
      p.fail(e);
    }
  };
  const repair = async () => {
    p.setBusy(`repair:${inst.id}`);
    try {
      await call("mc_repair_instance", { id: inst.id });
      p.say(t("mc.repaired", { name: inst.name }));
    } catch (e) {
      p.fail(e);
    } finally {
      p.setBusy(null);
      p.setProgress(null);
    }
  };
  const backup = async () => {
    try {
      const f = await call<string>("mc_backup_saves", { id: inst.id });
      p.say(t("mc.backedUp", { file: f }));
    } catch (e) {
      p.fail(e);
    }
  };
  const copyCode = async () => {
    try {
      const c = await call<{ code: string; skipped: number }>("mc_export_code", { id: inst.id });
      await navigator.clipboard.writeText(c.code);
      p.say(c.skipped > 0 ? t("mc.codeCopiedSkipped", { count: c.skipped }) : t("mc.codeCopied"));
    } catch (e) {
      p.fail(e);
    }
  };
  const updatePack = async () => {
    p.setBusy(`update:${inst.id}`);
    try {
      const n = await call<Instance>("mc_update_pack", { id: inst.id });
      await p.reload();
      p.say(t("mc.packUpdated", { name: n.name }), n.id);
    } catch (e) {
      p.fail(e);
    } finally {
      p.setBusy(null);
      p.setProgress(null);
    }
  };
  const playLabel = p.busy === `play:${inst.id}` ? t("mc.preparing") : t("mc.play");

  return (
    <div className="mc-detail">
      <button className="btn btn-quiet mc-back" onClick={p.back}>
        <Icon name="left" size={18} />
        {t("mc.back")}
      </button>

      <div className="mc-head">
        <Cover i={inst} big uri={p.cover} />
        <div className="mc-head-main">
          <h2 className="mc-title">{inst.name}</h2>
          <span className="note">{t("mc.myCreation")}</span>
          {p.launch && (
            <div className="mc-load" role="status" aria-live="polite">
              <span className="mc-load-text">{p.launch.text}</span>
              <span className="mc-load-track" aria-hidden="true">
                <span className={`mc-load-fill ${p.launch.total ? "" : "mc-load-wait"}`} style={p.launch.total ? { width: `${Math.min(100, Math.round((p.launch.done / p.launch.total) * 100))}%` } : undefined} />
              </span>
              <button className="mc-load-x" aria-label={t("mc.cancel")} data-tip={t("mc.cancel")} onClick={() => void call("mc_cancel", { key: p.launch!.id }).catch(() => {})}>
                <Icon name="close" size={14} />
              </button>
            </div>
          )}
        </div>
        <div className="mc-head-actions">
          {running ? (
            <button className="btn" onClick={() => void act(() => call("mc_stop", { id: inst.id }))}>
              {t("mc.stop")}
            </button>
          ) : (
            <button className="btn btn-primary btn-play" disabled={!!p.busy} aria-busy={p.busy === `play:${inst.id}`} onClick={() => void p.play(inst)}>
              <Icon name="play" />
              {playLabel}
            </button>
          )}
          <div className="mc-menu-wrap">
            <button
              ref={trigger}
              className="btn btn-icon"
              aria-label={t("mc.menu.more")}
              aria-haspopup="menu"
              aria-expanded={menu}
              onClick={(e) => {
                e.stopPropagation();
                setMenu(!menu);
              }}
            >
              <Icon name="sliders" size={18} />
            </button>
            {menu && (
              <ul
                ref={menuList}
                className="mc-menu"
                role="menu"
                onKeyDown={(e) => {
                  const items = [...(menuList.current?.querySelectorAll<HTMLElement>("button") ?? [])];
                  const at = items.indexOf(document.activeElement as HTMLElement);
                  if (e.key === "Escape") (e.preventDefault(), e.stopPropagation(), setMenu(false), trigger.current?.focus());
                  else if (e.key === "ArrowDown") (e.preventDefault(), items[(at + 1) % items.length]?.focus());
                  else if (e.key === "ArrowUp") (e.preventDefault(), items[(at - 1 + items.length) % items.length]?.focus());
                  else if (e.key === "Home") (e.preventDefault(), items[0]?.focus());
                  else if (e.key === "End") (e.preventDefault(), items[items.length - 1]?.focus());
                }}
              >
                <li>
                  <button role="menuitem" onClick={() => p.onAdd(inst.id)}>
                    {t("mc.menu.addContent")}
                  </button>
                </li>
                <li>
                  <button role="menuitem" onClick={() => void act(() => call("mc_open_folder", { id: inst.id, sub: null }))}>
                    {t("mc.folder")}
                  </button>
                </li>
                <li>
                  <button role="menuitem" onClick={() => setOptions(true)}>
                    {t("mc.menu.options")}
                  </button>
                </li>
                <li>
                  <button role="menuitem" onClick={() => void changeImage()}>
                    {t("mc.menu.changeImage")}
                  </button>
                </li>
                {p.cover && (
                  <li>
                    <button role="menuitem" onClick={() => void act(() => call("mc_set_cover", { id: inst.id, path: null }), p.reloadCovers)}>
                      {t("mc.menu.resetImage")}
                    </button>
                  </li>
                )}
                <li>
                  <button role="menuitem" onClick={() => void clone()}>
                    {t("mc.menu.clone")}
                  </button>
                </li>
                <li>
                  <button role="menuitem" onClick={() => void repair()}>
                    {t("mc.menu.repair")}
                  </button>
                </li>
                <li>
                  <button role="menuitem" onClick={() => void backup()}>
                    {t("mc.menu.backup")}
                  </button>
                </li>
                <li>
                  <button role="menuitem" onClick={() => void copyCode()}>
                    {t("mc.menu.copyCode")}
                  </button>
                </li>
                <li>
                  <button role="menuitem" onClick={() => void p.share(inst)}>
                    {t("mc.share")}
                  </button>
                </li>
                <li>
                  <button
                    role="menuitem"
                    className="mc-danger"
                    onClick={() => {
                      setConfirm({ title: t("mc.menu.delete"), text: t("mc.deleteAsk", { name: inst.name }), confirm: t("mc.delete"), danger: true, onConfirm: () => { void act(() => call("mc_delete_instance", { id: inst.id }).then(() => p.reload()), p.back); } });
                    }}
                  >
                    {t("mc.menu.delete")}
                  </button>
                </li>
              </ul>
            )}
          </div>
        </div>
        <div className="mc-stats">
          <span>
            {t("mc.lastPlayed")}: {inst.last_played ? new Date(inst.last_played * 1000).toLocaleDateString() : t("mc.never")}
          </span>
          <span>
            {t("mc.timePlayed")}: {playTime(t, inst.play_secs)}
          </span>
          <span>{inst.mc_version}</span>
          <span>
            {t(`mc.loaders.${inst.loader}`)}
            {inst.loader_version ? ` ${inst.loader_version}` : ""}
          </span>
        </div>
      </div>

      {packUpdate?.available && (
        <div className="banner" role="status">
          <span>{t("mc.packUpdateAvail", { version: packUpdate.version })}</span>
          <button className="btn btn-primary" disabled={!!p.busy} onClick={() => void updatePack()}>
            {p.busy === `update:${inst.id}` ? t("mc.updatingShort") : t("mc.packUpdate")}
          </button>
        </div>
      )}
      <div className="tabs" role="tablist">
        {(["content", "logs", "shots"] as const).map((x) => (
          <button key={x} role="tab" aria-selected={tab === x} className={`tab ${tab === x ? "tab-on" : ""}`} onClick={() => setTab(x)}>
            {x === "content" ? `${t("mc.tabs.content")} (${files.length})` : t(`mc.tabs.${x}`)}
          </button>
        ))}
      </div>

      {tab === "content" && (
        <div className="mc-content">
          <div className="seg" role="tablist">
            {SUBS.map((s) => (
              <button key={s} role="tab" aria-selected={sub === s} className={`seg-btn ${sub === s ? "seg-on" : ""}`} onClick={() => setSub(s)}>
                {t(`mc.sub.${s}`)} ({counts[s]})
              </button>
            ))}
          </div>
          {sub !== "world" && (
            <div className="mc-row">
              {sub !== "datapack" && (
                <>
                  <button className="btn" disabled={updates.length === 0 || !!updating} onClick={() => void runUpdates(updates)}>
                    <Icon name="down" size={18} />
                    {updating ? t("mc.updating", { name: updating }) : t("mc.updateAll")}
                    {updates.length > 0 && !updating && <span className="mc-count">{updates.length}</span>}
                  </button>
                  <button className="btn btn-icon btn-quiet" aria-label={t("mc.checkUpdates")} data-tip={t("mc.checkUpdates")} disabled={checking || !!updating} onClick={() => void check()}>
                    <Icon name="spin" size={18} />
                  </button>
                  {checking && <span className="note">{t("mc.checking")}</span>}
                </>
              )}
              <button className="btn btn-primary" style={{ marginLeft: "auto" }} onClick={() => p.onAdd(inst.id)}>
                <Icon name="plus" size={18} />
                {t("mc.addMods")}
              </button>
            </div>
          )}
          {shown.length === 0 ? (
            <div className="mc-empty">
              <p className="note">{t("mc.noFiles")}</p>
              {sub !== "world" && (
                <button className="btn" onClick={() => p.onAdd(inst.id)}>
                  <Icon name="plus" size={18} />
                  {t("mc.menu.addContent")}
                </button>
              )}
            </div>
          ) : (
            <table className="mc-table">
              <thead>
                <tr>
                  <th>{t("mc.col.name")}</th>
                  <th>{t("mc.col.action")}</th>
                  <th>{t("mc.col.author")}</th>
                  <th>{t("mc.col.env")}</th>
                  <th>{t("mc.col.active")}</th>
                  <th />
                </tr>
              </thead>
              <tbody>
                {shown.map((f) => (
                  <tr key={f.kind + f.name} className={f.enabled ? "" : "mc-off"}>
                    <td className="mc-fname" title={f.name}>
                      {(() => {
                        const m = meta[f.kind + "/" + f.name.replace(/.disabled$/, "")];
                        const plain = f.kind === "datapack" ? f.name.replace("/", " / ").replace(/.disabled$/, "") : f.name.replace(/.disabled$/, "");
                        const bytes = f.kind === "world" ? "" : f.size >= 1e6 ? `${(f.size / 1e6).toFixed(1)} MB` : `${Math.max(1, Math.round(f.size / 1e3))} KB`;
                        return (
                          <span className="mc-fcell">
                            {m?.icon ? <img className="mc-fico" src={m.icon} alt="" loading="lazy" /> : <span className="mc-fico" />}
                            <span className="mc-ftext">
                              <strong>{m?.title || plain}</strong>
                              <span className="note">{m?.title ? plain : bytes}</span>
                            </span>
                          </span>
                        );
                      })()}
                    </td>
                    <td>
                      {unknown.includes(`${f.kind}/${f.name}`) && (
                        <span className="note" title={t("mc.unknownHint")}>
                          {t("mc.unknown")}
                        </span>
                      )}
                      {updateOf(f) && (
                        <button className="btn" disabled={!!updating} title={updateOf(f)?.new_name} onClick={() => void runUpdates([updateOf(f)!])}>
                          {updating === f.name ? t("mc.updatingShort") : t("mc.update")}
                        </button>
                      )}
                    </td>
                    <td className="note">
                      {(() => {
                        const m = meta[f.kind + "/" + f.name.replace(/.disabled$/, "")];
                        return m?.author ? `${m.author}${m.more_authors ? ` +${m.more_authors}` : ""}` : "";
                      })()}
                    </td>
                    <td className="mc-env">
                      {(() => {
                        const m = meta[f.kind + "/" + f.name.replace(/.disabled$/, "")];
                        if (!m || f.kind !== "mod") return null;
                        const side = (v: string) => v !== "unsupported" && v !== "unknown";
                        return (
                          <>
                            {side(m.client) && <span className="mc-chip" title={t("mc.env.client")}>{t("mc.env.clientShort")}</span>}
                            {side(m.server) && <span className="mc-chip" title={t("mc.env.server")}>{t("mc.env.serverShort")}</span>}
                          </>
                        );
                      })()}
                    </td>
                    <td>
                      {f.kind !== "world" && (
                        <button
                          className="switch"
                          role="switch"
                          aria-checked={f.enabled}
                          aria-label={f.name}
                          onClick={() => void act(() => call("mc_file_enabled", { id: inst.id, kind: f.kind, name: f.name, enabled: !f.enabled }), load)}
                        />
                      )}
                    </td>
                    <td>
                      <button
                        className="btn btn-icon btn-quiet"
                        aria-label={t("mc.delete")}
                        onClick={() => {
                          setConfirm({ title: t("mc.delete"), text: t("mc.deleteFileAsk", { name: f.name.replace(/\.disabled$/, "") }), confirm: t("mc.delete"), danger: true, onConfirm: () => { void act(() => call("mc_file_remove", { id: inst.id, kind: f.kind, name: f.name }), load); } });
                        }}
                      >
                        <Icon name="trash" size={18} />
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </div>
      )}

      <div hidden={tab !== "logs"}>
        <Console id={inst.id} running={running} active={tab === "logs"} />
      </div>

      {tab === "shots" && (
        <div className="mc-content">
          <div className="mc-row">
            <span className="note">{t("mc.shotsCount", { count: shots.length })}</span>
            <span className="mc-grow" />
            <button className="btn" onClick={() => void act(() => call("mc_open_folder", { id: inst.id, sub: "screenshots" }))}>
              {t("mc.openShots")}
            </button>
          </div>
          {shots.length === 0 ? (
            <p className="note">{t("mc.shotsEmpty")}</p>
          ) : (
            <>
              <ul className="mc-thumbs">
                {shots.slice(0, shotLimit).map((s) => (
                  <li key={s.name}>
                    <Thumb id={inst.id} name={s.name} />
                  </li>
                ))}
              </ul>
              {shots.length > shotLimit && (
                <button className="btn" onClick={() => setShotLimit(shotLimit + 24)}>
                  {t("mc.showMore")}
                </button>
              )}
            </>
          )}
        </div>
      )}

      {confirm && <Confirm state={confirm} onCancel={() => setConfirm(null)} />}
      {options && <Options inst={inst} onClose={() => setOptions(false)} onSaved={p.reload} fail={p.fail} />}
    </div>
  );
}

function CodeImport(p: Common & { onClose: () => void }) {
  const { t } = useTranslation();
  const [code, setCode] = useState("");
  const go = async () => {
    p.setBusy("code");
    try {
      const r = await call<{ instance: Instance; failed: number }>("mc_import_code", { code });
      await p.reload();
      p.setSelected(r.instance.id);
      p.say(r.failed > 0 ? t("mc.codeImportedFailed", { name: r.instance.name, count: r.failed }) : t("mc.codeImported", { name: r.instance.name }), r.instance.id);
      p.onClose();
    } catch (e) {
      p.fail(e);
    } finally {
      p.setBusy(null);
      p.setProgress(null);
    }
  };
  return (
    <Modal title={t("mc.pasteCode")} onClose={p.onClose}>
      <div className="mc-dialog-body">
        <p className="note">{t("mc.codeHint")}</p>
        <textarea className="field mc-code" rows={4} value={code} placeholder="KAE1-…" aria-label={t("mc.pasteCode")} onChange={(e) => setCode(e.target.value)} />
      </div>
      <div className="mc-dialog-foot">
        <button className="btn" onClick={p.onClose}>
          {t("mc.cancel")}
        </button>
        <button className="btn btn-primary" disabled={!code.trim() || !!p.busy} onClick={() => void go()}>
          {p.busy === "code" ? t("mc.importing") : t("mc.import")}
        </button>
      </div>
    </Modal>
  );
}

function Create(p: Common & { onClose: () => void }) {
  const { t } = useTranslation();
  const { versions } = p;
  const [name, setName] = useState("");
  const [ver, setVer] = useState("");
  const [snap, setSnap] = useState(false);
  const [loader, setLoader] = useState("fabric");
  const releases = useMemo(() => versions.filter((v) => snap || v.kind === "release"), [versions, snap]);
  useEffect(() => {
    if (!ver && releases[0]) setVer(releases[0].id);
  }, [releases, ver]);
  const create = async () => {
    p.setBusy("create");
    try {
      const i = await call<Instance>("mc_create_instance", { name: name || `Minecraft ${ver}`, mcVersion: ver, loader });
      await p.reload();
      p.setSelected(i.id);
      p.say(t("mc.created", { name: i.name }));
      p.onClose();
    } catch (e) {
      p.fail(e);
    } finally {
      p.setBusy(null);
      p.setProgress(null);
    }
  };
  return (
    <Modal title={t("mc.new")} onClose={p.onClose}>
      <div className="mc-dialog-body">
        <input className="field" value={name} placeholder={t("mc.namePh")} aria-label={t("mc.namePh")} onChange={(e) => setName(e.target.value)} />
        <div className="mc-row">
          <Select label={t("mc.version")} value={ver} onChange={setVer} options={releases.map((v) => ({ value: v.id, label: v.id }))} />
          <Select label={t("mc.loader")} value={loader} onChange={setLoader} options={LOADERS.map((l) => ({ value: l, label: t(`mc.loaders.${l}`) }))} />
        </div>
        <label className="check">
          <input type="checkbox" checked={snap} onChange={(e) => setSnap(e.target.checked)} />
          <span>{t("mc.snapshots")}</span>
        </label>
      </div>
      <div className="mc-dialog-foot">
        <button className="btn" onClick={p.onClose}>
          {t("mc.cancel")}
        </button>
        <button className="btn btn-primary" disabled={!ver || !!p.busy} onClick={() => void create()}>
          {p.busy === "create" ? t("mc.preparing") : t("mc.create")}
        </button>
      </div>
    </Modal>
  );
}

function Settings(p: Common & { cfg: Cfg | null; onClose: () => void }) {
  const { t } = useTranslation();
  return (
    <Modal title={t("mc.settings")} onClose={p.onClose}>
      <div className="mc-dialog-body">
        <GeneralSettings onChanged={() => void p.reload()} />
      </div>
    </Modal>
  );
}

/** The first screen: no packs yet. A name and one click make a ready pack. */
function Onboarding(p: Common & { cfg: Cfg | null; onCreate: () => void; onImport: () => void }) {
  const { t } = useTranslation();
  const [player, setPlayer] = useState(p.cfg?.account_name ?? "");
  useEffect(() => {
    if (p.cfg) setPlayer(p.cfg.account_name);
  }, [p.cfg]);
  const latest = p.versions.find((v) => v.kind === "release")?.id ?? "";
  const make = async (preset: "vanilla" | "fabric" | "fast") => {
    p.setBusy("create");
    try {
      if (player.trim() && player.trim() !== p.cfg?.account_name) await call("mc_save_config", { accountName: player.trim() });
      const i = await call<Instance>("mc_create_instance", { name: `Minecraft ${latest}`, mcVersion: latest, loader: preset === "vanilla" ? "vanilla" : "fabric" });
      if (preset === "fast") {
        for (const slug of ["sodium", "lithium", "ferrite-core"]) await call("mc_install_project", { id: i.id, provider: "modrinth", projectId: slug, kind: "mod" }).catch(() => {});
      }
      await p.reload();
      p.setSelected(i.id);
      p.say(t("mc.created", { name: i.name }), i.id);
    } catch (e) {
      p.fail(e);
    } finally {
      p.setBusy(null);
      p.setProgress(null);
    }
  };
  const disabled = !latest || !!p.busy;
  return (
    <section className="mc-onboard">
      <h2>{t("mc.onb.title")}</h2>
      <p className="note">{t("mc.onb.text")}</p>
      <label className="mc-lab" htmlFor="mc-onb-name">
        {t("mc.player")}
      </label>
      <input id="mc-onb-name" className="field" value={player} placeholder={t("mc.player")} onChange={(e) => setPlayer(e.target.value)} />
      <div className="mc-presets">
        {(["fast", "fabric", "vanilla"] as const).map((k) => (
          <button key={k} className="mc-preset" disabled={disabled} onClick={() => void make(k)}>
            <strong>{t(`mc.onb.${k}`)}</strong>
            <span className="note">{t(`mc.onb.${k}Hint`, { version: latest })}</span>
          </button>
        ))}
      </div>
      {p.busy === "create" && <p className="note">{t("mc.preparing")}</p>}
      <div className="mc-row">
        <button className="btn btn-quiet" disabled={!!p.busy} onClick={p.onCreate}>
          {t("mc.onb.manual")}
        </button>
        <button className="btn btn-quiet" disabled={!!p.busy} onClick={p.onImport}>
          {t("mc.import")}
        </button>
      </div>
    </section>
  );
}

export default function Instances(p: Common & { running: Record<string, boolean>; play: (i: Instance) => Promise<void>; onGo: (p: McPage) => void; openRequest: OpenRequest | null; onOpened: () => void; onDetail: (id: string | null) => void; onAdd: (id: string) => void; launch: Launch | null }) {
  const { t } = useTranslation();
  const { instances, running } = p;
  const [openId, setOpenId] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);
  const [codeOpen, setCodeOpen] = useState(false);
  const [settings, setSettings] = useState(false);
  const [covers, setCovers] = useState<Covers>({});
  const [openTab, setOpenTab] = useState<Tab>("content");
  const [query, setQuery] = useState("");
  const [sort, setSort] = useState<string>(() => store("kae.mc.sort", "recent"));
  const [view, setView] = useState<string>(() => store("kae.mc.view", "grid"));
  const [pins, setPins] = useState<string[]>(() => {
    try {
      return JSON.parse(localStorage.getItem("kae.mc.pins") ?? "[]") as string[];
    } catch {
      return [];
    }
  });
  const keep = (k: string, v: string) => {
    try {
      localStorage.setItem(k, v);
    } catch {
      /* the choice lasts until restart */
    }
  };
  const shown = useMemo(() => {
    const q = query.trim().toLowerCase();
    const list = instances.filter((x) => !q || x.name.toLowerCase().includes(q) || x.mc_version.includes(q));
    const by = (a: Instance, b: Instance) => (sort === "name" ? a.name.localeCompare(b.name) : sort === "created" ? b.created - a.created : b.last_played - a.last_played || b.created - a.created);
    return list.sort((a, b) => Number(pins.includes(b.id)) - Number(pins.includes(a.id)) || by(a, b));
  }, [instances, query, sort, pins]);
  const togglePin = (id: string) => {
    const next = pins.includes(id) ? pins.filter((x) => x !== id) : [...pins, id];
    setPins(next);
    keep("kae.mc.pins", JSON.stringify(next));
  };
  const { openRequest, onOpened } = p;
  useEffect(() => {
    if (openRequest) {
      if (openRequest.id) {
        setOpenId(openRequest.id);
        setOpenTab((openRequest.tab as Tab | undefined) ?? "content");
      }
      if (openRequest.settings) setSettings(true);
      onOpened();
    }
  }, [openRequest, onOpened]);
  const reloadCovers = useCallback(() => {
    void call<Covers>("mc_covers").then(setCovers, () => setCovers({}));
  }, []);
  useEffect(reloadCovers, [reloadCovers, instances.length]);
  const open = instances.find((i) => i.id === openId);
  const openKey = open?.id;
  useEffect(() => {
    if (openKey) p.setSelected(openKey); // the catalog installs into the pack that is open
    p.onDetail(openKey ?? null);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [openKey]);
  useEffect(() => () => p.onDetail(null), []); // eslint-disable-line react-hooks/exhaustive-deps

  const importPack = async (given?: string) => {
    try {
      let f: string | string[] | null = given ?? null;
      if (!f) {
        const { open: pick } = await import("@tauri-apps/plugin-dialog");
        f = await pick({ multiple: false, title: t("mc.import"), filters: [{ name: t("mc.packFiles"), extensions: ["kaepack", "mrpack"] }] });
      }
      if (!f || Array.isArray(f)) return;
      p.setBusy("import");
      const i = await call<Instance>("mc_import_pack", { path: f });
      await p.reload();
      p.setSelected(i.id);
      p.say(t("mc.packInstalled", { name: i.name }));
    } catch (e) {
      p.fail(e);
    } finally {
      p.setBusy(null);
      p.setProgress(null);
    }
  };
  const importRef = useRef(importPack);
  importRef.current = importPack;
  // a pack file dropped on the window is imported
  useEffect(() => {
    let off: (() => void) | undefined;
    void import("@tauri-apps/api/webview").then(({ getCurrentWebview }) =>
      getCurrentWebview()
        .onDragDropEvent((e) => {
          if (e.payload.type !== "drop") return;
          const f = e.payload.paths.find((x) => /\.(kaepack|mrpack)$/i.test(x));
          if (f) void importRef.current(f);
        })
        .then((u) => (off = u)),
    );
    return () => off?.();
  }, []);
  const share = async (i: Instance) => {
    try {
      const { save } = await import("@tauri-apps/plugin-dialog");
      const f = await save({ title: t("mc.share"), defaultPath: `${i.name.replace(/[\\/:*?"<>|]/g, "_")}.kaepack`, filters: [{ name: "Kae pack", extensions: ["kaepack"] }] });
      if (!f) return;
      p.setBusy(`share:${i.id}`);
      await call("mc_export_instance", { id: i.id, path: f });
      p.say(t("mc.shared", { path: f }));
    } catch (e) {
      p.fail(e);
    } finally {
      p.setBusy(null);
      p.setProgress(null);
    }
  };

  if (open) {
    return <Detail {...p} key={open.id} initialTab={openTab} cover={covers[open.id]} reloadCovers={reloadCovers} inst={open} running={!!running[open.id]} back={() => setOpenId(null)} share={share} />;
  }
  return (
    <div className="mc-library">
      <div className="mc-toolbar">
        <button className="btn btn-primary" disabled={!!p.busy} onClick={() => setCreating(true)}>
          <Icon name="plus" size={18} />
          {t("mc.create")}
        </button>
        <button className="btn" disabled={!!p.busy} onClick={() => void importPack()}>
          {p.busy === "import" ? t("mc.importing") : t("mc.import")}
        </button>
        <button className="btn" disabled={!!p.busy} onClick={() => setCodeOpen(true)}>
          {t("mc.pasteCode")}
        </button>
        <input className="field mc-search" type="search" value={query} placeholder={t("mc.searchPacks")} aria-label={t("mc.searchPacks")} onChange={(e) => setQuery(e.target.value)} />
        <Select label={t("mc.sortBy")} value={sort} onChange={(v) => (setSort(v), keep("kae.mc.sort", v))} options={["recent", "name", "created"].map((s) => ({ value: s, label: t(`mc.packSort.${s}`) }))} />
        <div className="seg" role="radiogroup" aria-label={t("mc.view.label")}>
          {["grid", "list"].map((v) => (
            <button key={v} type="button" role="radio" aria-checked={view === v} className={`seg-btn ${view === v ? "seg-on" : ""}`} onClick={() => (setView(v), keep("kae.mc.view", v))}>
              {t(`mc.view.${v}`)}
            </button>
          ))}
        </div>
        <span className="mc-grow" />
        <button className="btn btn-icon" aria-label={t("mc.settings")} data-tip={t("mc.settings")} onClick={() => setSettings(true)}>
          <Icon name="settings" size={18} />
        </button>
      </div>
      {instances.length === 0 ? (
        <Onboarding {...p} cfg={p.cfg} onCreate={() => setCreating(true)} onImport={() => void importPack()} />
      ) : (
        shown.length === 0 ? (
          <p className="note">{t("mc.noMatch")}</p>
        ) : view === "list" ? (
          <ul className="mc-rows">
            {shown.map((x) => (
              <li key={x.id} className="mc-rowitem">
                <button className="mc-rowmain" onClick={() => setOpenId(x.id)}>
                  <Cover i={x} uri={covers[x.id]} />
                  <span className="mc-rowtext">
                    <strong>{x.name}</strong>
                    <span className="note">
                      {x.mc_version} · {t(`mc.loaders.${x.loader}`)}
                    </span>
                  </span>
                  <span className="note">{x.last_played ? new Date(x.last_played * 1000).toLocaleDateString() : t("mc.never")}</span>
                  {running[x.id] && <span className="mc-live mc-live-inline">{t("mc.runningNow")}</span>}
                </button>
                <button className="btn btn-icon btn-quiet" aria-pressed={pins.includes(x.id)} aria-label={t("mc.pin")} data-tip={t("mc.pin")} onClick={() => togglePin(x.id)}>
                  <Icon name="heart" size={18} />
                </button>
              </li>
            ))}
          </ul>
        ) : (
          <ul className="mc-tiles">
            {shown.map((x) => (
              <li key={x.id} className="mc-tilewrap">
                <button className="mc-tile" onClick={() => setOpenId(x.id)}>
                  <div className="mc-tile-art" style={coverStyle(x.id, covers[x.id])}>
                    {!covers[x.id] && <Brand />}
                    <span className="mc-badge">
                      {t(`mc.loaders.${x.loader}`)} · {x.mc_version}
                    </span>
                    {running[x.id] && <span className="mc-live">{t("mc.runningNow")}</span>}
                  </div>
                  <strong className="mc-tile-name">{x.name}</strong>
                  <span className="note">{t("mc.myCreation")}</span>
                </button>
                <button className="mc-pin" aria-pressed={pins.includes(x.id)} aria-label={t("mc.pin")} data-tip={t("mc.pin")} onClick={() => togglePin(x.id)}>
                  <Icon name="heart" size={16} />
                </button>
              </li>
            ))}
          </ul>
        )
      )}
      {codeOpen && <CodeImport {...p} onClose={() => setCodeOpen(false)} />}
      {creating && <Create {...p} onClose={() => setCreating(false)} />}
      {settings && <Settings {...p} onClose={() => setSettings(false)} />}
    </div>
  );
}
