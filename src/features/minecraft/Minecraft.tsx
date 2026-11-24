import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { call } from "../../lib/api";
import Select from "../../components/Select";
import Instances from "./Instances";
import { setPlaying } from "../../lib/presence";
import Icon from "../../components/Icons";

export type OpenRequest = { id?: string; tab?: string; settings?: boolean };
export type McPage = "instances" | "browse";

export type Instance = {
  id: string;
  name: string;
  mc_version: string;
  loader: string;
  loader_version: string | null;
  memory_mb: number;
  memory_custom: boolean;
  java_path: string | null;
  extra_args: string;
  last_played: number;
  created: number;
  play_secs: number;
  debug_console: boolean;
  source?: string;
};
export type GameVersion = { id: string; kind: string; released: string };
type Hit = { provider: string; id: string; slug: string; title: string; description: string; icon: string | null; downloads: number; author: string; kind: string };
type Page = { hits: Hit[]; total: number };
export type McFile = { kind: string; name: string; size: number; enabled: boolean };
export type Cfg = { has_curseforge_key: boolean; account_name: string };
export type Progress = { id: string; stage: string; done: number; total: number };

const KINDS = ["mod", "modpack", "resourcepack", "shader"] as const;

function fmt(n: number) {
  return n >= 1e6 ? `${(n / 1e6).toFixed(1)}M` : n >= 1e3 ? `${(n / 1e3).toFixed(1)}K` : String(n);
}

export default function Minecraft({ page, onGo }: { page: McPage; onGo: (p: McPage) => void }) {
  const { t } = useTranslation();
  const [instances, setInstances] = useState<Instance[]>([]);
  const [cfg, setCfg] = useState<Cfg | null>(null);
  const [versions, setVersions] = useState<GameVersion[]>([]);
  const [selected, setSelected] = useState<string>(() => localStorage.getItem("kae.mc.selected") ?? "");
  const [running, setRunning] = useState<Record<string, boolean>>({});
  const [progress, setProgress] = useState<Progress | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [msg, setMsg] = useState<{ kind: "ok" | "error"; text: string; open?: string; openTab?: string } | null>(null);
  // what the pack list should open as soon as it is shown: a pack (on a tab) or the settings
  const [openRequest, setOpenRequest] = useState<OpenRequest | null>(null);
  const openInstance = (id: string, tab?: string) => {
    setSelected(id);
    setOpenRequest({ id, tab });
    setMsg(null);
    onGo("instances");
  };
  const openSettings = () => {
    setOpenRequest({ settings: true });
    onGo("instances");
  };

  const explain = useCallback(
    (x: unknown) => {
      const s = String(x);
      if (s === "curseforge_key_missing") return t("mc.keyMissing");
      if (s.startsWith("loader_unsupported:")) return t("mc.loaderUnsupported", { loader: s.split(":")[1] });
      if (s === "curseforge_zip_import") return t("mc.cfZip");
      if (s === "bad_player_name") return t("mc.badName");
      if (s === "name_required") return t("mc.nameRequired");
      if (s === "cancelled") return t("mc.cancelled");
      if (s === "instance_running") return t("mc.runningCant");
      if (s === "no_worlds") return t("mc.noWorlds");
      return s;
    },
    [t],
  );
  const fail = useCallback((x: unknown) => setMsg({ kind: "error", text: explain(x) }), [explain]);

  const reload = useCallback(async () => {
    setInstances(await call<Instance[]>("mc_instances").catch(() => []));
    setCfg(await call<Cfg>("mc_config").catch(() => null));
  }, []);
  useEffect(() => {
    void reload();
    void call<GameVersion[]>("mc_game_versions").then(setVersions, () => {});
    let off: (() => void) | undefined;
    void listen<Progress>("mc-progress", (e) => setProgress(e.payload)).then((u) => (off = u));
    // the game ended: say why when it crashed
    let offExit: (() => void) | undefined;
    void listen<{ id: string; name: string; crashed: boolean; reason: string; detail: string }>("mc-exit", (e) => {
      const x = e.payload;
      setRunning((r) => ({ ...r, [x.id]: false }));
      if (x.crashed) setMsg({ kind: "error", text: t(`mc.crash.${x.reason || "unknown"}`, { name: x.name, detail: x.detail }), open: x.id, openTab: "logs" });
    }).then((u) => (offExit = u));
    return () => {
      off?.();
      offExit?.();
    };
  }, [reload, t]);

  // the status in Discord follows the game
  useEffect(() => {
    const id = Object.keys(running).find((k) => running[k]);
    setPlaying(id ? (instances.find((i) => i.id === id)?.name ?? "Minecraft") : null);
  }, [running, instances]);

  useEffect(() => {
    try {
      localStorage.setItem("kae.mc.selected", selected);
    } catch {
      /* the choice is just forgotten */
    }
  }, [selected]);
  useEffect(() => {
    if (!instances.some((i) => i.id === selected)) setSelected(instances[0]?.id ?? "");
  }, [instances, selected]);

  // which instances run now
  useEffect(() => {
    const poll = () =>
      void Promise.all(instances.map((i) => call<boolean>("mc_running", { id: i.id }).then((r) => [i.id, r] as const, () => [i.id, false] as const))).then((r) => setRunning(Object.fromEntries(r)));
    poll();
    const id = setInterval(poll, 3000);
    return () => clearInterval(id);
  }, [instances]);

  const stageText = (p: Progress | null) => {
    if (!p) return "";
    const pct = p.total ? ` ${Math.min(100, Math.round((p.done / p.total) * 100))}%` : "";
    return `${t(`mc.stage.${p.stage}`, { defaultValue: p.stage })}${pct}`;
  };

  const play = async (i: Instance) => {
    setBusy(`play:${i.id}`);
    setMsg(null);
    try {
      await call("mc_launch", { id: i.id });
      setMsg({ kind: "ok", text: t("mc.started", { name: i.name }) });
      setRunning((r) => ({ ...r, [i.id]: true }));
    } catch (e) {
      fail(e);
    } finally {
      setBusy(null);
      setProgress(null);
    }
  };

  const banner = msg && (
    <div className={`banner ${msg.kind === "error" ? "" : "banner-ok"}`} role={msg.kind === "error" ? "alert" : "status"}>
      <span>{msg.text}</span>
      {msg.open && (
        <button className="btn" onClick={() => openInstance(msg.open!, msg.openTab)}>
          {msg.openTab === "logs" ? t("mc.openLog") : t("mc.openInstance")}
        </button>
      )}
      <button className="btn btn-quiet" onClick={() => setMsg(null)}>
        {t("mc.close")}
      </button>
    </div>
  );
  const bar = busy && progress && (
    <div className="mc-progress" role="status">
      <span>{stageText(progress)}</span>
      {progress.total > 0 && <progress value={progress.done} max={progress.total} aria-label={stageText(progress)} />}
      <button className="btn btn-quiet" onClick={() => void call("mc_cancel", { key: progress.id }).catch(() => {})}>
        {t("mc.cancel")}
      </button>
    </div>
  );

  return (
    <section className="view mc">
      <header className="view-head">
        <div className="view-title-row">
          <h1 className="view-title">{page === "instances" ? t("mc.instances") : t("mc.browse")}</h1>
        </div>
        <div className="tabs" role="tablist">
          {(["instances", "browse"] as const).map((p) => (
            <button key={p} role="tab" aria-selected={page === p} className={`tab ${page === p ? "tab-on" : ""}`} onClick={() => onGo(p)}>
              {t(`mc.${p}`)}
            </button>
          ))}
        </div>
      </header>
      {banner}
      {bar}
      {page === "instances" ? (
        <Instances
          instances={instances}
          versions={versions}
          cfg={cfg}
          selected={selected}
          setSelected={setSelected}
          running={running}
          busy={busy}
          play={play}
          reload={reload}
          fail={fail}
          say={(text, open) => setMsg({ kind: "ok", text, open })}
          setBusy={setBusy}
          setProgress={setProgress}
          onGo={onGo}
          openRequest={openRequest}
          onOpened={() => setOpenRequest(null)}
        />
      ) : (
        <Browse instances={instances} versions={versions} cfg={cfg} selected={selected} setSelected={setSelected} reload={reload} fail={fail} say={(text, open) => setMsg({ kind: "ok", text, open })} busy={busy} setBusy={setBusy} onGo={onGo} setProgress={setProgress} openInstance={openInstance} openSettings={openSettings} />
      )}
    </section>
  );
}

export type Common = {
  instances: Instance[];
  versions: GameVersion[];
  cfg: Cfg | null;
  selected: string;
  setSelected: (id: string) => void;
  reload: () => Promise<void>;
  fail: (e: unknown) => void;
  say: (t: string, openId?: string) => void;
  busy: string | null;
  setBusy: (b: string | null) => void;
  setProgress: (p: Progress | null) => void;
};

const SORTS = ["relevance", "downloads", "newest", "updated"] as const;
const CATEGORIES: Record<string, string[]> = {
  mod: ["optimization", "adventure", "decoration", "equipment", "food", "library", "magic", "technology", "utility", "worldgen", "storage", "transportation", "social", "game-mechanics", "mobs", "economy", "management", "minigame"],
  modpack: ["adventure", "challenging", "combat", "kitchen-sink", "lightweight", "magic", "multiplayer", "optimization", "quests", "technology"],
  resourcepack: ["decoration", "vanilla-like", "realistic", "cartoon", "fantasy", "themed", "simplistic", "utility", "combat", "modded"],
  shader: ["cartoon", "fantasy", "realistic", "vanilla-like", "semi-realistic", "potato", "low", "medium", "high"],
};

function Browse(p: Common & { onGo: (p: McPage) => void; openInstance: (id: string) => void; openSettings: () => void }) {
  const { t } = useTranslation();
  const { instances, versions, cfg, selected } = p;
  const [provider, setProvider] = useState("modrinth");
  const [kind, setKind] = useState<(typeof KINDS)[number]>("mod");
  const [text, setText] = useState("");
  const [query, setQuery] = useState("");
  const [pageNo, setPageNo] = useState(0);
  const [data, setData] = useState<Page | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [allVer, setAllVer] = useState<string>("");
  const [allLoader, setAllLoader] = useState<string>("");
  const [sort, setSort] = useState<string>("");
  const [category, setCategory] = useState<string>("");
  const [retry, setRetry] = useState(0);
  // projects the chosen instance already has, as `provider:id`
  const [have, setHave] = useState<Set<string>>(new Set());
  const [haveTick, setHaveTick] = useState(0);
  const cur = instances.find((i) => i.id === selected);
  const seq = useRef(0);
  const curId = cur?.id;
  useEffect(() => {
    if (!curId || kind === "modpack") {
      setHave(new Set());
      return;
    }
    let gone = false;
    void call<string[]>("mc_installed", { id: curId }).then((l) => !gone && setHave(new Set(l)), () => {});
    return () => {
      gone = true;
    };
  }, [curId, kind, haveTick]);
  const packOf = (h: Hit) => instances.find((i) => i.source === `${h.provider}:${h.id}`);

  // a typed query is sent after a short pause
  useEffect(() => {
    const id = setTimeout(() => (setQuery(text), setPageNo(0)), 350);
    return () => clearTimeout(id);
  }, [text]);

  // mods and packs follow the chosen instance unless the filters are set by hand
  const mcVersion = kind === "modpack" ? allVer : allVer || cur?.mc_version || "";
  const loader = kind === "mod" || kind === "modpack" ? (kind === "modpack" ? allLoader : allLoader || (cur && cur.loader !== "vanilla" ? cur.loader : "")) : "";

  useEffect(() => {
    const n = ++seq.current;
    setLoading(true);
    setError(null);
    call<Page>("mc_search", { req: { provider, query, kind, mc_version: mcVersion || null, loader: loader || null, page: pageNo, sort: sort || null, category: provider === "modrinth" ? category || null : null } }).then(
      (d) => n === seq.current && (setData(d), setLoading(false)),
      // the last results stay on the screen under the error, so a lost connection does not blank the page
      (e) => n === seq.current && (setError(String(e)), setLoading(false)),
    );
  }, [provider, query, kind, mcVersion, loader, pageNo, sort, category, retry]);

  const install = async (h: Hit) => {
    p.setBusy(`i:${h.id}`);
    try {
      if (kind === "modpack") {
        const i = await call<Instance>("mc_install_modpack", { provider: h.provider, projectId: h.id, name: h.title, mcVersion: mcVersion || null, loader: loader || null });
        await p.reload();
        p.setSelected(i.id);
        if (h.icon) void call("mc_set_cover_url", { id: i.id, url: h.icon }).catch(() => {}); // the icon of the project is the picture of the pack
        p.say(t("mc.packInstalled", { name: i.name }));
        p.openInstance(i.id);
      } else {
        if (!cur) throw new Error(t("mc.pickInstance"));
        const r = await call<{ file: string; deps: string[] }>("mc_install_project", { id: cur.id, provider: h.provider, projectId: h.id, kind });
        p.say(t("mc.installed", { file: r.file, count: r.deps.length }), cur.id);
        setHaveTick((n) => n + 1);
      }
    } catch (e) {
      p.fail(e);
    } finally {
      p.setBusy(null);
      p.setProgress(null);
    }
  };

  const keyMissing = provider === "curseforge" && error === "curseforge_key_missing";
  const releases = versions.filter((v) => v.kind === "release");
  return (
    <div className="mc-browse">
      <div className="toolbar mc-toolbar">
        <input className="field search" type="search" value={text} placeholder={t("mc.search")} aria-label={t("mc.search")} onChange={(e) => setText(e.target.value)} />
        <Select label={t("mc.source")} value={provider} onChange={(v) => (setProvider(v), setPageNo(0))} options={[{ value: "modrinth", label: "Modrinth" }, { value: "curseforge", label: "CurseForge" }]} />
        <Select label={t("mc.version")} value={allVer} onChange={(v) => (setAllVer(v), setPageNo(0))} options={[{ value: "", label: kind === "modpack" ? t("mc.anyVersion") : cur ? `${cur.mc_version} (${cur.name})` : t("mc.anyVersion") }, ...releases.map((v) => ({ value: v.id, label: v.id }))]} />
        {(kind === "mod" || kind === "modpack") && (
          <Select label={t("mc.loader")} value={allLoader} onChange={(v) => (setAllLoader(v), setPageNo(0))} options={[{ value: "", label: kind === "modpack" ? t("mc.anyLoader") : cur && cur.loader !== "vanilla" ? t(`mc.loaders.${cur.loader}`) : t("mc.anyLoader") }, { value: "fabric", label: "Fabric" }, { value: "quilt", label: "Quilt" }, { value: "forge", label: "Forge" }, { value: "neoforge", label: "NeoForge" }]} />
        )}
        <Select label={t("mc.sortBy")} value={sort} onChange={(v) => (setSort(v), setPageNo(0))} options={[{ value: "", label: t("mc.sort.auto") }, ...SORTS.map((s) => ({ value: s, label: t(`mc.sort.${s}`) }))]} />
        {provider === "modrinth" && (
          <Select label={t("mc.category")} value={category} onChange={(v) => (setCategory(v), setPageNo(0))} options={[{ value: "", label: t("mc.anyCategory") }, ...(CATEGORIES[kind] ?? []).map((c) => ({ value: c, label: t(`mc.cat.${c}`, { defaultValue: c }) }))]} />
        )}
      </div>
      {kind !== "modpack" && instances.length > 0 && (
        <div className="mc-target">
          <span>{t("mc.installingInto")}</span>
          <Select label={t("mc.installTo")} value={selected} onChange={p.setSelected} options={instances.map((i) => ({ value: i.id, label: i.name }))} />
          {cur && (
            <span className="note">
              {cur.mc_version} · {t(`mc.loaders.${cur.loader}`)}
            </span>
          )}
        </div>
      )}
      <div className="tabs" role="tablist">
        {KINDS.map((k) => (
          <button key={k} role="tab" aria-selected={kind === k} className={`tab ${kind === k ? "tab-on" : ""}`} onClick={() => (setKind(k), setPageNo(0), setCategory(""))}>
            {t(`mc.kindsPlural.${k}`)}
          </button>
        ))}
      </div>
      {kind !== "modpack" && instances.length === 0 && <p className="note">{t("mc.needInstance")}</p>}
      {keyMissing ? (
        <div className="banner">
          <span>{t("mc.keyMissing")}</span>
          <button className="btn" onClick={() => p.openSettings()}>
            {t("mc.connectCf")}
          </button>
        </div>
      ) : (
        error && (
          <div className="banner" role="alert">
            <span>{/failed|error sending|dns|connect|timed out|timeout|network/i.test(error) ? t("mc.offline") : error}</span>
            <button className="btn" onClick={() => setRetry(retry + 1)}>
              {t("mc.retry")}
            </button>
          </div>
        )
      )}
      {!keyMissing && loading && !data && (
        <ul className="mc-hits" aria-busy="true" aria-label={t("mc.loading")}>
          {[0, 1, 2, 3, 4, 5].map((n) => (
            <li key={n} className="mc-hit mc-skeleton" />
          ))}
        </ul>
      )}
      {data && (
        <>
          <p className="note">{t("mc.found", { count: data.total })}</p>
          <ul className="mc-hits">
            {data.hits.map((h) => (
              <li key={h.id} className="mc-hit">
                {h.icon ? <img src={h.icon} alt="" loading="lazy" className="mc-ico" /> : <span className="mc-ico" />}
                <div className="mc-hit-body">
                  <strong>{h.title}</strong>
                  <span className="note">
                    {h.author} · {t("mc.downloads", { n: fmt(h.downloads) })}
                  </span>
                  <p>{h.description}</p>
                </div>
                {kind === "modpack" && packOf(h) ? (
                  <div className="mc-have-actions">
                    <span className="mc-have">
                      <Icon name="check" size={16} />
                      {t("mc.alreadyHave")}
                    </span>
                    <button className="btn" onClick={() => p.openInstance(packOf(h)!.id)}>
                      {t("mc.openInstance")}
                    </button>
                  </div>
                ) : kind !== "modpack" && have.has(`${h.provider}:${h.id}`) ? (
                  <span className="mc-have">
                    <Icon name="check" size={16} />
                    {t("mc.alreadyHave")}
                  </span>
                ) : (
                  <button className="btn btn-primary" disabled={!!p.busy} aria-busy={p.busy === `i:${h.id}`} onClick={() => void install(h)}>
                    {p.busy === `i:${h.id}` ? t("mc.installing") : kind === "modpack" ? t("mc.installPack") : t("mc.install")}
                  </button>
                )}
              </li>
            ))}
          </ul>
          <div className="mc-pager">
            <button className="btn" disabled={pageNo === 0} onClick={() => setPageNo(pageNo - 1)}>
              ←
            </button>
            <span className="note">{pageNo + 1}</span>
            <button className="btn" disabled={(pageNo + 1) * 24 >= data.total} onClick={() => setPageNo(pageNo + 1)}>
              →
            </button>
          </div>
        </>
      )}
      {void cfg}
    </div>
  );
}
