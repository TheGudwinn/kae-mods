import { lazy, Suspense, useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { flushSync } from "react-dom";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { useForeignMods, type ForeignGroup } from "./lib/foreign";
import {
  call,
  isTauri,
  type ApplyResult,
  type Profile,
  type AddonContent,
  type ForeignAddon,
  type GameInstall,
  type InstallResult,
  type InstalledMod,
  type ModDetails,
  type ModFile,
  type LastAction,
  type Mod,
  type ProgressEvent,
  type QueueRow,
  type UpdateInfo,
  type UpdateReport,
} from "./lib/api";
import { formatBytes, humanizeError } from "./lib/format";
import { useCompat } from "./lib/compat";
import { findSuspects, patchAcked } from "./lib/patch";
import Activity, { type ActivityState } from "./components/Activity";
import ErrorBoundary from "./components/ErrorBoundary";
import Feedback from "./components/Feedback";
import TooltipLayer from "./components/TooltipLayer";
import UpdateBanner from "./components/UpdateBanner";
import WindowControls from "./components/WindowControls";
import Icon from "./components/Icons";
import Select from "./components/Select";
import Confirm, { type ConfirmState } from "./components/Confirm";
import History, { type HistoryItem } from "./components/History";
import { setAppearance, useAppearance, useEffectiveWallpaper, useWallpaper, wallVideo } from "./lib/appearance";
import Home from "./features/home/Home";
import logo from "./assets/logo.svg";
import Catalog from "./features/store/Catalog";
import ModDrawer from "./features/store/ModDrawer";
import Library from "./features/library/Library";
import Profiles from "./features/profiles/Profiles";
import Settings from "./features/settings/Settings";
import Downloads from "./features/downloads/Downloads";
import Performance from "./features/performance/Performance";
import Fixes from "./features/fixes/Fixes";
const Skins = lazy(() => import("./features/skins/Skins")); // pulls in the 3D engine, so it loads on first visit

type View = "home" | "catalog" | "library" | "profiles" | "downloads" | "performance" | "fixes" | "skins" | "settings";

export default function App() {
  const { t } = useTranslation();
  const look = useAppearance();
  const wallpaper = useEffectiveWallpaper();
  const ownWallpaper = useWallpaper();
  const [view, setViewRaw] = useState<View>("home");
  const viewRef = useRef(view);
  viewRef.current = view;
  // Page changes cross-fade through the browser's View Transitions when it has them.
  const setView = useCallback((v: View) => {
    if (v === viewRef.current) return;
    const doc = document as Document & { startViewTransition?: (cb: () => void) => unknown };
    const still = document.documentElement.dataset.motion === "off" || window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    if (doc.startViewTransition && !still && !document.hidden) {
      doc.startViewTransition(() => flushSync(() => setViewRaw(v)));
      // a window that is not being drawn never runs the transition callback; do not leave the page stuck
      window.setTimeout(() => {
        if (viewRef.current !== v) setViewRaw(v);
      }, 220);
    } else setViewRaw(v);
  }, []);
  const railRef = useRef<HTMLElement>(null);
  const [ind, setInd] = useState<{ y: number; h: number } | null>(null);
  const [indReady, setIndReady] = useState(false);
  const [searchText, setSearchText] = useState("");
  const [search, setSearch] = useState({ text: "", n: 0 });
  const [preset, setPreset] = useState<{ category: number | null; sub: number | null; n: number } | undefined>(undefined);
  const [game, setGame] = useState<GameInstall | null | undefined>(undefined);
  const [installed, setInstalled] = useState<InstalledMod[] | null>(null);
  const [foreign, setForeign] = useState<ForeignAddon[]>([]);
  const [contents, setContents] = useState<AddonContent[]>([]);
  const ownRemoteIds = useMemo(() => (installed ?? []).map((m) => m.remote_id).filter((id): id is string => !!id), [installed]);
  const { groups, known } = useForeignMods(foreign, ownRemoteIds);
  const [launching, setLaunching] = useState(false);
  const [last, setLast] = useState<LastAction | null>(null);
  const [profiles, setProfiles] = useState<Profile[] | null>(null);
  const [open, setOpen] = useState<Mod | null>(null);
  const [busy, setBusy] = useState(false);
  const [activity, setActivity] = useState<ActivityState | null>(null);
  const [update, setUpdate] = useState<UpdateInfo | null>(null);
  const [maximized, setMaximized] = useState(false);
  const [feedbackOpen, setFeedbackOpen] = useState(false);
  const [queue, setQueue] = useState<QueueRow[]>([]);
  // bumped when an update ends, so the history on the downloads page is read again
  const [historyKey, setHistoryKey] = useState(0);
  const prevStates = useRef<Map<number, string>>(new Map());
  const compat = useCompat();
  const [modReport, setModReport] = useState<UpdateReport | null>(null);
  const [checkingMods, setCheckingMods] = useState(false);
  const autoChecked = useRef(false);
  const [updatePct, setUpdatePct] = useState<number | null>(null);
  const [running, setRunning] = useState(false);
  // something changed in the mods since the game was last started
  const [pending, setPending] = useState(false);
  const [confirm, setConfirm] = useState<ConfirmState | null>(null);
  const [history, setHistory] = useState<HistoryItem[]>([]);
  const [unread, setUnread] = useState(0);
  const searchBox = useRef<HTMLInputElement>(null);
  const [launched, setLaunched] = useState(() => {
    try {
      return localStorage.getItem("guddion.launched") === "1";
    } catch {
      return false;
    }
  });
  const [onboardHidden, setOnboardHidden] = useState(() => {
    try {
      return localStorage.getItem("guddion.onboarding.hidden") === "1";
    } catch {
      return false;
    }
  });
  const markChanged = useCallback(() => setPending(true), []);

  // Is the game running? Asked every few seconds; changes made meanwhile apply on the next start.
  useEffect(() => {
    if (!isTauri) return;
    let alive = true;
    const poll = () => void call<boolean>("game_running").then((r) => alive && setRunning(r), () => {});
    poll();
    const id = setInterval(poll, 4000);
    return () => {
      alive = false;
      clearInterval(id);
    };
  }, []);
  const wasRunning = useRef(false);
  useEffect(() => {
    // a fresh start picks up every change made before it
    if (running && !wasRunning.current) setPending(false);
    wasRunning.current = running;
  }, [running]);

  // Toasts vanish; the bell keeps the last ones.
  const historyId = useRef(0);
  useEffect(() => {
    if (!activity || activity.kind === "progress") return;
    const item: HistoryItem = { id: ++historyId.current, at: Date.now(), kind: activity.kind, text: activity.text, sub: activity.kind === "ok" ? activity.sub : undefined };
    setHistory((h) => [...h.slice(-29), item]);
    setUnread((n) => n + 1);
  }, [activity]);

  const refresh = useCallback(async () => {
    // Each list is shown as soon as it is read: the database behind some of them starts on first use,
    // and the folder scan must not wait for it.
    await Promise.all([
      call<InstalledMod[]>("list_installed").catch(() => null).then(setInstalled),
      call<ForeignAddon[]>("list_foreign_addons").catch(() => []).then(setForeign),
      call<AddonContent[]>("describe_installed").catch(() => []).then(setContents),
      call<LastAction | null>("last_action").catch(() => null).then(setLast),
      call<Profile[]>("list_profiles").catch(() => null).then(setProfiles),
    ]);
  }, []);

  const detectGame = useCallback(() => call<GameInstall | null>("detect_game").then(setGame, () => setGame(null)), []);

  useEffect(() => {
    void detectGame();
    void refresh();
  }, [detectGame, refresh]);

  // Startup check: silent on failure (offline, blocked GitHub, no source configured).
  useEffect(() => {
    void (async () => {
      try {
        const s = await call<{ check_updates: boolean }>("get_settings");
        if (!s.check_updates) return;
        setUpdate(await call<UpdateInfo | null>("check_update"));
      } catch {
        /* the settings screen has a manual check that reports errors */
      }
    })();
  }, []);

  const checkUpdate = async () => {
    const info = await call<UpdateInfo | null>("check_update");
    setUpdate(info);
    return info;
  };

  async function installUpdate() {
    setUpdatePct(-1);
    const unlisten = isTauri
      ? await listen<{ downloaded: number; total: number | null }>("update-progress", (ev) => {
          const { downloaded, total } = ev.payload;
          setUpdatePct(total ? Math.min(100, Math.round((downloaded / total) * 100)) : -1);
        })
      : () => {};
    try {
      await call("install_update"); // the app restarts on success
    } catch (e) {
      setActivity({ kind: "error", text: humanizeError(t, e) });
      setUpdatePct(null);
    } finally {
      unlisten();
    }
  }

  const checkMods = useCallback(async (manual: boolean) => {
    setCheckingMods(true);
    try {
      setModReport(await call<UpdateReport>("check_mod_updates"));
    } catch (e) {
      if (manual) setActivity({ kind: "error", text: humanizeError(t, e) });
    } finally {
      setCheckingMods(false);
    }
  }, [t]);

  // Once per launch, as soon as we know there is something installed.
  useEffect(() => {
    if (installed && installed.length > 0 && !autoChecked.current) {
      autoChecked.current = true;
      void checkMods(false);
    }
  }, [installed, checkMods]);

  /** Mods of other managers: the mod page has several files, so the person picks which one to update to. */
  const [pick, setPick] = useState<{ remoteId: string; name: string; files: ModFile[] } | null>(null);

  async function updateForeign(remoteId: string, name: string, fileId?: string) {
    setPick(null);
    if (!fileId) {
      const details = await run(() => call<ModDetails>("mod_details", { id: remoteId }));
      if (!details) return;
      const files = details.files.filter((f) => /\.(zip|7z|rar|vpk)$/i.test(f.name));
      if (files.length === 0) {
        setActivity({ kind: "error", text: t("library.noUpdateFile") });
        return;
      }
      if (files.length > 1) {
        setPick({ remoteId, name, files });
        return;
      }
    }
    setActivity({ kind: "progress", text: t("activity.downloading", { name }), pct: null });
    const unlisten = isTauri
      ? await listen<ProgressEvent>("install-progress", (ev) => {
          const p = ev.payload;
          if (p.stage === "install") setActivity({ kind: "progress", text: t("activity.installing", { name }), pct: null });
          else setActivity({ kind: "progress", text: t("activity.downloading", { name }), pct: p.total ? Math.round((p.downloaded / p.total) * 100) : null });
        })
      : () => {};
    const res = await run(() => call<InstallResult>("update_foreign", { remoteId, fileId: fileId ?? null, name }));
    unlisten();
    setHistoryKey((k) => k + 1);
    if (res) {
      setActivity({ kind: "ok", text: t("library.updated", { name: res.name }), sub: res.overlaps.length > 0 ? t("activity.overlaps", { count: new Set(res.overlaps.map((o) => o.with_addon)).size }) : undefined });
      markChanged();
    } else {
      setActivity((a) => (a?.kind === "progress" ? null : a));
    }
  }

  /** Updates every selected mod that has an update, one after another; a mod that needs a choice of file is left for a single update. */
  async function bulkUpdate(ownIds: number[], foreignMods: { id: string; name: string }[]) {
    const total = ownIds.length + foreignMods.length;
    let done = 0;
    const failed: string[] = [];
    const needChoice: string[] = [];
    const say = (name: string) => setActivity({ kind: "progress", text: t("library.bulkUpdating", { name, n: done + failed.length + needChoice.length + 1, total }), pct: null });
    setBusy(true);
    try {
      for (const id of ownIds) {
        const name = installed?.find((m) => m.id === id)?.name ?? "";
        say(name);
        try {
          await call("update_mod", { id });
          done++;
        } catch {
          failed.push(name);
        }
      }
      for (const m of foreignMods) {
        say(m.name);
        try {
          const details = await call<ModDetails>("mod_details", { id: m.id });
          if (details.files.filter((f) => /\.(zip|7z|rar|vpk)$/i.test(f.name)).length !== 1) {
            needChoice.push(m.name);
            continue;
          }
          await call("update_foreign", { remoteId: m.id, fileId: null, name: m.name });
          done++;
        } catch {
          failed.push(m.name);
        }
      }
    } finally {
      setBusy(false);
      void refresh();
    }
    setHistoryKey((k) => k + 1);
    const sub = [needChoice.length > 0 && t("library.bulkUpdateChoice", { list: needChoice.join(", ") }), failed.length > 0 && t("library.bulkUpdateFailed", { list: failed.join(", ") })].filter(Boolean).join(" ");
    setActivity({ kind: done > 0 ? "ok" : "error", text: t("library.bulkUpdated", { count: done }), sub: sub || undefined });
    setModReport((r) => (r ? { ...r, updates: r.updates.filter((u) => !ownIds.includes(u.mod_id)) } : r));
    markChanged();
  }

  async function updateMod(id: number) {
    const name = installed?.find((m) => m.id === id)?.name ?? "";
    setActivity({ kind: "progress", text: t("activity.downloading", { name }), pct: null });
    const unlisten = isTauri
      ? await listen<ProgressEvent>("install-progress", (ev) => {
          const p = ev.payload;
          if (p.stage === "install") setActivity({ kind: "progress", text: t("activity.installing", { name }), pct: null });
          else setActivity({ kind: "progress", text: t("activity.downloading", { name }), pct: p.total ? Math.round((p.downloaded / p.total) * 100) : null });
        })
      : () => {};
    const res = await run(() => call<InstallResult>("update_mod", { id }));
    unlisten();
    setHistoryKey((k) => k + 1);
    if (res) {
      setActivity({ kind: "ok", text: t("library.updated", { name: res.name }), sub: res.overlaps.length > 0 ? t("activity.overlaps", { count: new Set(res.overlaps.map((o) => o.with_addon)).size }) : undefined });
      setModReport((r) => (r ? { ...r, updates: r.updates.filter((u) => u.mod_id !== id) } : r));
      markChanged();
    } else {
      setActivity((a) => (a?.kind === "progress" ? null : a));
    }
  }

  // The queue lives in the backend; we reload it on every change event and patch progress in place.
  const loadQueue = useCallback(async () => {
    const rows = await call<QueueRow[]>("list_queue").catch(() => null);
    if (!rows) return;
    for (const r of rows) {
      const before = prevStates.current.get(r.id);
      if (!before || before === r.state) continue;
      if (r.state === "done") {
        setActivity({
          kind: "ok",
          text: t("activity.done", { name: r.name, slot: String(r.slot ?? 0).padStart(2, "0") }),
          sub: (r.overlaps ?? 0) > 0 ? t("activity.overlaps", { count: r.overlaps }) : undefined,
        });
        void refresh();
        markChanged();
      } else if (r.state === "failed") {
        setActivity({ kind: "error", text: `${r.name}: ${humanizeError(t, r.error ?? "")}` });
      }
    }
    prevStates.current = new Map(rows.map((r) => [r.id, r.state]));
    setQueue(rows);
  }, [t, refresh, markChanged]);

  useEffect(() => {
    void loadQueue();
    if (!isTauri) return;
    const subs = [
      listen("queue-changed", () => void loadQueue()),
      listen<{ id: number; downloaded: number; total: number | null }>("queue-progress", (ev) => {
        const { id, downloaded, total } = ev.payload;
        setQueue((rows) => rows.map((r) => (r.id === id ? { ...r, downloaded, total: total ?? r.total } : r)));
      }),
    ];
    return () => subs.forEach((p) => void p.then((off) => off()));
  }, [loadQueue]);

  const queueCall = (cmd: string, id?: number) =>
    void run(async () => {
      await call(cmd, id === undefined ? undefined : { id });
      await loadQueue();
    });
  const activeDownloads = queue.filter((r) => ["queued", "downloading", "paused", "installing"].includes(r.state));
  const queuedModIds = useMemo(() => new Set(activeDownloads.map((r) => r.remote_id)), [activeDownloads]);

  // mods installed by this app and by other managers: the catalog marks and can hide both
  const installedIds = useMemo(() => new Set([...(installed ?? []).map((m) => m.remote_id), ...foreign.map((f) => f.remote_id)].filter((x): x is string => !!x)), [installed, foreign]);

  async function run<T>(fn: () => Promise<T>): Promise<T | undefined> {
    setBusy(true);
    try {
      return await fn();
    } catch (e) {
      setActivity({ kind: "error", text: humanizeError(t, e) });
      return undefined;
    } finally {
      setBusy(false);
      void refresh();
    }
  }

  async function install(mod: Mod, fileId: string) {
    const ok = await run(async () => {
      await call("enqueue_install", { modId: mod.id, fileId });
      return true;
    });
    if (ok) {
      setActivity({ kind: "ok", text: t("downloads.added", { name: mod.name }) });
      setOpen(null);
      void loadQueue();
    }
  }

  const say = (text: string, sub?: string) => setActivity({ kind: "ok", text, sub });

  async function createProfile(name: string): Promise<boolean> {
    const done = await run(async () => {
      await call("create_profile", { name });
      return true;
    });
    if (done) say(t("profiles.created", { name: name.trim() }));
    return !!done;
  }

  async function renameProfile(id: number, name: string): Promise<boolean> {
    return !!(await run(async () => {
      await call("rename_profile", { id, name });
      return true;
    }));
  }

  const doApplyProfile = (p: Profile) =>
    void run(async () => {
      const r = await call<ApplyResult>("apply_profile", { id: p.id });
      markChanged();
      say(t("profiles.applied", { name: p.name, on: r.enabled, off: r.disabled }), r.missing.length > 0 ? t("profiles.appliedMissing", { count: r.missing.length }) : undefined);
    });
  const applyProfile = (p: Profile) => {
    const on = p.mods.filter((m) => m.installed && m.enabled).length;
    const missing = p.mods.filter((m) => !m.installed).length;
    setConfirm({
      title: t("confirm.applyTitle", { name: p.name }),
      text: t("confirm.applyText"),
      details: [t("confirm.applyOn", { count: on }), t("confirm.applyOff"), ...(missing > 0 ? [t("confirm.applyMissing", { count: missing })] : [])],
      confirm: t("profiles.apply"),
      onConfirm: () => doApplyProfile(p),
    });
  };

  const updateProfile = (p: Profile) =>
    void run(async () => {
      await call("update_profile_from_current", { id: p.id });
      say(t("profiles.updated", { name: p.name }));
    });

  const deleteProfile = (id: number) => {
    const p = profiles?.find((x) => x.id === id);
    setConfirm({ title: t("confirm.deleteProfileTitle", { name: p?.name ?? "" }), text: t("confirm.deleteProfileText"), confirm: t("profiles.removeYes"), danger: true, onConfirm: () => void run(() => call("delete_profile", { id })) });
  };

  const exportProfile = (p: Profile) =>
    void run(async () => {
      let path: string | null = "mock.json";
      if (isTauri) {
        const { save } = await import("@tauri-apps/plugin-dialog");
        path = await save({ title: t("profiles.exportTitle"), defaultPath: `${p.name}.json`, filters: [{ name: "Profile", extensions: ["json"] }] });
      }
      if (!path) return;
      await call("export_profile", { id: p.id, path });
      say(t("profiles.exported"));
    });

  const copyProfileCode = (p: Profile) =>
    void run(async () => {
      const code = await call<string>("profile_code", { id: p.id });
      await navigator.clipboard.writeText(code);
      say(t("profiles.codeCopied", { name: p.name }));
    });

  const importProfileCode = async (code: string): Promise<boolean> => {
    const id = await run(() => call<number>("import_profile_code", { code }));
    if (id === undefined) return false;
    const list = await call<Profile[]>("list_profiles");
    say(t("profiles.imported", { name: list.find((p) => p.id === id)?.name ?? "" }));
    return true;
  };

  const importProfile = () =>
    void run(async () => {
      let path: string | null = "mock.json";
      if (isTauri) {
        const { open } = await import("@tauri-apps/plugin-dialog");
        const picked = await open({ multiple: false, filters: [{ name: "Profile", extensions: ["json"] }] });
        path = typeof picked === "string" ? picked : null;
      }
      if (!path) return;
      const id = await call<number>("import_profile", { path });
      const list = await call<Profile[]>("list_profiles");
      say(t("profiles.imported", { name: list.find((p) => p.id === id)?.name ?? "" }));
    });

  const toggle = (id: number, enabled: boolean) =>
    void run(async () => {
      await call("set_mod_enabled", { id, enabled });
      markChanged();
    });
  const toggleForeign = (g: ForeignGroup, enabled: boolean) =>
    void run(async () => {
      await call("set_foreign_enabled", { files: g.files.map((f) => f.name), id: g.id, enabled });
      markChanged();
    });
  // Several mods at once: one request at a time, one summary at the end.
  const bulkToggle = (ids: number[], gs: ForeignGroup[], enabled: boolean) =>
    void run(async () => {
      for (const id of ids) await call("set_mod_enabled", { id, enabled });
      for (const g of gs) await call("set_foreign_enabled", { files: g.files.map((f) => f.name), id: g.id, enabled });
      markChanged();
      say(t(enabled ? "library.bulkOn" : "library.bulkOff", { count: ids.length + gs.length }));
    });
  // Switches mods on and off for the "find the broken mod" search; resolves when the files have changed.
  const applyEnabled = (on: number[], off: number[]) =>
    run(async () => {
      for (const id of on) await call("set_mod_enabled", { id, enabled: true });
      for (const id of off) await call("set_mod_enabled", { id, enabled: false });
      markChanged();
      return true;
    });
  const bulkRemove = (ids: number[]) => {
    const names = (installed ?? []).filter((m) => ids.includes(m.id)).map((m) => m.name);
    setConfirm({
      title: t("confirm.removeManyTitle", { count: ids.length }),
      text: t("confirm.removeText"),
      details: names.slice(0, 8).concat(names.length > 8 ? [t("confirm.andMore", { count: names.length - 8 })] : []),
      confirm: t("library.removeYes"),
      danger: true,
      onConfirm: () =>
        void run(async () => {
          for (const id of ids) await call("uninstall_mod", { id });
          markChanged();
        }),
    });
  };
  const launchGame = async () => {
    setLaunching(true);
    try {
      const args = await call<string>("launch_game");
      setPending(false);
      setLaunched(true);
      try {
        localStorage.setItem("guddion.launched", "1");
      } catch {
        /* only the first-run hint depends on it */
      }
      setActivity({ kind: "ok", text: args ? t("game.started", { args }) : t("game.startedPlain") });
    } catch (e) {
      setActivity({ kind: "error", text: humanizeError(t, e) });
    } finally {
      setTimeout(() => setLaunching(false), 4000); // Steam needs a moment; stops double clicks
    }
  };
  const remove = (id: number) => {
    const m = installed?.find((x) => x.id === id);
    setConfirm({
      title: t("confirm.removeTitle", { name: m?.name ?? "" }),
      text: t("confirm.removeText"),
      confirm: t("library.removeYes"),
      danger: true,
      onConfirm: () =>
        void run(async () => {
          await call("uninstall_mod", { id });
          markChanged();
        }),
    });
  };
  const reorder = (ids: number[]) =>
    void run(async () => {
      await call("reorder_mods", { order: ids });
      markChanged();
      say(t("library.reordered"));
    });
  const undo = () =>
    void run(async () => {
      await call("undo_last");
      markChanged();
      setActivity({ kind: "ok", text: t("library.undone") });
    });

  useLayoutEffect(() => {
    const place = () => {
      const el = railRef.current?.querySelector<HTMLElement>('[aria-current="page"]');
      setInd(el ? { y: el.offsetTop, h: el.offsetHeight } : null);
      if (el) requestAnimationFrame(() => requestAnimationFrame(() => setIndReady(true)));
    };
    place();
    window.addEventListener("resize", place);
    return () => window.removeEventListener("resize", place);
  }, [view, look.rail]);
  // mods built for an older game build that a patch may have broken (hidden once the user dismisses the notice)
  const patchBuild = game?.build_id ?? null;
  const patchBadge = patchAcked(patchBuild) ? 0 : findSuspects(installed, contents, patchBuild, modReport).filter((x) => x.risk !== "low").length;
  const order: View[] = ["home", "catalog", "library", "skins", "fixes", "settings"];
  // pages that live under another rail item: profiles under the library, performance under diagnostics
  const railOf = (v: View): View | null => (v === "profiles" ? "library" : v === "performance" ? "fixes" : v === "downloads" ? null : v);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        // the library has its own search box, so the top one is hidden there
        const box = searchBox.current ?? document.querySelector<HTMLInputElement>("input[data-page-search]");
        box?.focus();
        box?.select();
      } else if (e.altKey && !e.ctrlKey && /^[1-9]$/.test(e.key)) {
        e.preventDefault();
        const next = order[Number(e.key) - 1];
        if (next) setView(next);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
    // eslint-disable-next-line react-hooks/exhaustive-deps -- `order` is a constant list
  }, []);
  const diagTabs = (
    <div className="tabs" role="tablist" aria-label={t("nav.fixes")}>
      {(["fixes", "performance"] as const).map((id) => (
        <button key={id} type="button" role="tab" aria-selected={view === id} className={`tab ${view === id ? "tab-on" : ""}`} onClick={() => setView(id)}>
          {t(`diag.${id}`)}
        </button>
      ))}
    </div>
  );
  const hint = (id: View) => `${t(`nav.${id}`)}  ·  Alt+${order.indexOf(id) + 1}`;
  const activeProfile = profiles?.find((x) => x.active);
  const current = activeDownloads.find((r) => r.state === "downloading") ?? activeDownloads[0];
  const currentPct = current?.total ? Math.min(100, Math.round((current.downloaded / current.total) * 100)) : null;

  return (
    <div className={`shell ${maximized ? "maximized" : ""}`}>
      {wallpaper && (
        <div className="wallpaper" aria-hidden="true" style={{ backgroundImage: `url(${wallpaper})` }}>
          {/* the shipped animated background, unless the player chose a picture or turned motion off */}
          {wallVideo && !ownWallpaper && look.motion !== "off" && <video className="wallpaper-video" src={wallVideo} poster={wallpaper} autoPlay loop muted playsInline preload="auto" />}
        </div>
      )}
      <div className="grain" aria-hidden="true" />
      <nav ref={railRef} className={`rail ${look.rail === "open" ? "rail-open" : "rail-closed"}`} aria-label={t("app.name")}>
        <span className={`rail-ind ${indReady ? "rail-ind-ready" : ""}`} aria-hidden="true" style={ind ? { transform: `translateY(${ind.y}px)`, height: ind.h, opacity: 1 } : { opacity: 0 }} />
        <div className="rail-logo" data-tauri-drag-region title={t("app.name")}>
          <img src={logo} alt="" width={30} height={30} />
          <span className="rail-brand">{t("app.name")}</span>
        </div>
        {(
          [
            ["home", "home", 0],
            ["catalog", "catalog", 0],
            ["library", "library", modReport?.updates.length ?? 0],
            ["skins", "skins", 0],
            ["fixes", "fixes", patchBadge],
          ] as const
        ).map(([id, icon, badge]) => (
          <button key={id} className="rail-item" data-tip={hint(id)} aria-current={railOf(view) === id ? "page" : undefined} aria-label={t(`nav.${id}`)} aria-keyshortcuts={`Alt+${order.indexOf(id) + 1}`} onClick={() => setView(id)}>
            <Icon name={icon} />
            <span className="rail-label">{t(`nav.${id}`)}</span>
            {badge > 0 && <span className="rail-badge">{badge}</span>}
          </button>
        ))}
        <div className="rail-spacer" data-tauri-drag-region />
        {update && look.rail === "open" && <UpdateBanner info={update} progress={updatePct} onInstall={() => void installUpdate()} onDismiss={() => setUpdate(null)} />}
        <button className="rail-item" data-tip={t("nav.feedback")} aria-label={t("nav.feedback")} onClick={() => setFeedbackOpen(true)}>
          <Icon name="chat" />
          <span className="rail-label">{t("nav.feedback")}</span>
        </button>
        <button className="rail-item" data-tip={hint("settings")} aria-current={view === "settings" ? "page" : undefined} aria-label={t("nav.settings")} onClick={() => setView("settings")}>
          <Icon name="settings" />
          <span className="rail-label">{t("nav.settings")}</span>
        </button>
        <button
          className="rail-item rail-fold"
          data-tip={look.rail === "open" ? t("nav.collapse") : t("nav.expand")}
          aria-label={look.rail === "open" ? t("nav.collapse") : t("nav.expand")}
          aria-expanded={look.rail === "open"}
          onClick={() => setAppearance({ rail: look.rail === "open" ? "closed" : "open" })}
        >
          <Icon name="sidebar" />
          <span className="rail-label">{t("nav.collapse")}</span>
        </button>
      </nav>

      <div className="stage">
        <header className="topbar" data-tauri-drag-region>
          {view !== "library" && (
          <form
            className="topsearch"
            role="search"
            onSubmit={(e) => {
              e.preventDefault();
              setSearch((x) => ({ text: searchText.trim(), n: x.n + 1 }));
              setView("catalog");
            }}
          >
            <input
              ref={searchBox}
              className="field"
              type="search"
              value={searchText}
              placeholder={t("catalog.search")}
              aria-label={t("catalog.search")}
              aria-keyshortcuts="Control+K"
              onChange={(e) => setSearchText(e.target.value)}
            />
            <kbd className="kbd" aria-hidden="true">
              Ctrl K
            </kbd>
          </form>
          )}
          <div className="spacer" data-tauri-drag-region />
          <div className="status" data-tauri-drag-region>
            <button className="chip chip-btn" onClick={() => setView("downloads")} title={t("downloads.title")} aria-current={view === "downloads" ? "page" : undefined}>
              <Icon name="downloads" size={16} />
              {current && <span className="stat">{`${activeDownloads.length}${currentPct != null ? ` · ${currentPct}%` : ""}`}</span>}
              {current && currentPct != null && (
                <span className="chip-bar" aria-hidden="true">
                  <i style={{ width: `${currentPct}%` }} />
                </span>
              )}
            </button>
            {profiles && profiles.length > 0 && (
              <Select
                className="select-profile"
                label={t("profiles.switch")}
                value={activeProfile ? String(activeProfile.id) : ""}
                onChange={(v) => {
                  const p = profiles.find((x) => String(x.id) === v);
                  if (p && !p.active) applyProfile(p);
                }}
                options={[...(activeProfile ? [] : [{ value: "", label: t("profiles.pick") }]), ...profiles.map((p) => ({ value: String(p.id), label: p.name }))]}
              />
            )}
            <div className="playblock">
              <button className="btn btn-play" disabled={!game || launching || running} aria-busy={launching} onClick={() => void launchGame()} title={game ? t("game.playHint") : t("game.notFoundHint")}>
                <Icon name="play" />
                {running ? t("game.running") : launching ? t("game.starting") : t("game.play")}
              </button>
              <span className={`play-state ${!game && game !== undefined ? "is-bad" : pending && !running ? "is-warn" : running ? "is-live" : ""}`} title={game === null ? t("game.notFoundHint") : undefined}>
                {game === undefined
                  ? "…"
                  : !game
                    ? t("game.notFound")
                    : running
                      ? t("game.runningHint")
                      : pending
                        ? t("game.pending")
                        : [game.build_id ? `#${game.build_id}` : t("game.found"), t("game.modsOn", { count: (installed ?? []).filter((m) => m.enabled).length })].join(" · ")}
              </span>
            </div>
            <History items={history} unread={unread} onOpen={() => setUnread(0)} onClear={() => setHistory([])} />
          </div>
          {isTauri && <WindowControls onMaximizedChange={setMaximized} />}
        </header>

      <main className="main">
        <ErrorBoundary key={view}>
        {view === "home" ? (
          <Home
            installed={installed}
            groups={groups}
            known={known}
            report={modReport}
            busy={busy}
            onOpen={setOpen}
            onToggle={toggle}
            onToggleForeign={toggleForeign}
            onUpdate={(id) => void updateMod(id)}
            onGo={setView}
            game={game}
            launched={launched}
            onBrowse={(category, sub) => {
              setPreset((x) => ({ category, sub, n: (x?.n ?? 0) + 1 }));
              setView("catalog");
            }}
            showOnboarding={!onboardHidden}
            onHideOnboarding={() => {
              setOnboardHidden(true);
              try {
                localStorage.setItem("guddion.onboarding.hidden", "1");
              } catch {
                /* the hint just comes back next time */
              }
            }}
            onPlay={() => void launchGame()}
          />
        ) : view === "catalog" ? (
          <Catalog installedIds={installedIds} onOpen={setOpen} search={search} preset={preset} onGoSettings={() => setView("settings")} />
        ) : view === "downloads" ? (
          <Downloads
            rows={queue}
            onPause={(id) => queueCall("pause_download", id)}
            onResume={(id) => queueCall("resume_download", id)}
            onCancel={(id) => queueCall("cancel_download", id)}
            onRemove={(id) => queueCall("remove_download", id)}
            onPauseAll={() => queueCall("pause_all_downloads")}
            onResumeAll={() => queueCall("resume_all_downloads")}
            onClear={() => queueCall("clear_finished_downloads")}
            onGoCatalog={() => setView("catalog")}
            historyKey={historyKey}
          />
        ) : view === "settings" ? (
          <Settings
            game={game}
            onCheckUpdate={checkUpdate}
            onSaved={() => {
              void detectGame();
              void refresh();
            }}
          />
        ) : view === "skins" ? (
          <Suspense fallback={null}><Skins mods={installed} groups={groups} known={known} contents={contents} gameFound={!!game} onToggle={toggle} onToggleForeign={toggleForeign} onGoCatalog={() => setView("catalog")} onFind={(text) => { setSearchText(text); setSearch((x) => ({ text, n: x.n + 1 })); setView("catalog"); }} /></Suspense>
        ) : view === "performance" ? (
          <>
            {diagTabs}
            <Performance />
          </>
        ) : view === "fixes" ? (
          <>
            {diagTabs}
          <Fixes
            game={game}
            compat={compat}
            mods={installed}
            contents={contents}
            report={modReport}
            checking={checkingMods}
            busy={busy}
            onCheckUpdates={() => void checkMods(true)}
            onUpdate={(id) => void updateMod(id)}
            onUpdateMany={(ids) => void bulkUpdate(ids, [])}
            onDisable={(ids) => bulkToggle(ids, [], false)}
            gameRunning={running}
            onPlay={() => void launchGame()}
            onApply={applyEnabled}
            onChanged={() => {
              void refresh();
            }}
          />
          </>
        ) : view === "profiles" ? (
          <Profiles
            profiles={profiles}
            busy={busy}
            onCreate={createProfile}
            onApply={applyProfile}
            onUpdate={updateProfile}
            onRename={renameProfile}
            onDelete={deleteProfile}
            onGoLibrary={() => setView("library")}
            onExport={exportProfile}
            onImport={importProfile}
            onCopyCode={copyProfileCode}
            onImportCode={importProfileCode}
          />
        ) : (
          <Library
            mods={installed}
            groups={groups}
            known={known}
            contents={contents}
            last={last}
            busy={busy}
            onToggle={toggle}
            onToggleForeign={toggleForeign}
            onRemove={remove}
            onUndo={undo}
            onReorder={reorder}
            gameBuild={game?.build_id ?? null}
            report={modReport}
            checking={checkingMods}
            onCheckUpdates={() => void checkMods(true)}
            onUpdate={(id) => void updateMod(id)}
            onUpdateForeign={(id, name) => void updateForeign(id, name)}
            onBulkUpdate={(ids, foreignMods) => void bulkUpdate(ids, foreignMods)}
            onGoCatalog={() => setView("catalog")}
            onGoFixes={() => setView("fixes")}
            onGoProfiles={() => setView("profiles")}
            compat={compat}
            gameFound={!!game}
            gameRunning={running}
            onBulkToggle={bulkToggle}
            onBulkRemove={bulkRemove}
          />
        )}
        </ErrorBoundary>
      </main>

      </div>

      <TooltipLayer />
      {feedbackOpen && <Feedback build={game?.build_id ?? null} onClose={() => setFeedbackOpen(false)} />}
      {open && <ModDrawer mod={open} installed={installedIds.has(open.id)} queued={queuedModIds.has(open.id)} busy={busy} onClose={() => setOpen(null)} onInstall={(m, f) => void install(m, f)} />}
      {pick && (
        <>
          <div className="scrim" onClick={() => setPick(null)} />
          <div className="modal-wrap" onClick={() => setPick(null)}>
            <section className="pick" role="dialog" aria-modal="true" aria-label={pick.name} onClick={(e) => e.stopPropagation()}>
              <h2 className="pick-title">{t("library.pickTitle", { name: pick.name })}</h2>
              <p className="note">{t("library.pickText")}</p>
              <ul className="pick-list">
                {pick.files.map((f) => (
                  <li key={f.id}>
                    <button className="btn" onClick={() => void updateForeign(pick.remoteId, pick.name, f.id)}>
                      <span className="pick-name">{f.name}</span>
                      <span className="pick-size">{formatBytes(f.size)}</span>
                    </button>
                  </li>
                ))}
              </ul>
              <button className="btn btn-quiet" onClick={() => setPick(null)}>
                {t("library.cancel")}
              </button>
            </section>
          </div>
        </>
      )}
      <Activity state={activity} onDismiss={() => setActivity(null)} />
      {confirm && <Confirm state={confirm} onCancel={() => setConfirm(null)} />}
    </div>
  );
}
