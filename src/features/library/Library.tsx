import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import type { AddonContent, Content, InstalledMod, LastAction, ModUpdate, UpdateReport } from "../../lib/api";
import { groupState, groupTitle, type ForeignGroup, type GroupState } from "../../lib/foreign";
import { smallThumb, type CachedMod } from "../../lib/modcache";
import { contentForFiles, mainKinds } from "../../lib/content";
import { formatBytes } from "../../lib/format";
import { camStateOf, isFixable, repairTitle, shortName, type CamState, type Compat } from "../../lib/compat";
import StateMark from "../../components/StateMark";
import Select from "../../components/Select";
import Icon from "../../components/Icons";
import EmptyState from "../../components/EmptyState";
import { openExternal } from "../../lib/open";
import { Tags } from "./ContentView";
import { useHeroPortraits } from "../../lib/portraits";
import { realHeroes } from "../../lib/heroes";

type Props = {
  mods: InstalledMod[] | null;
  /** Mods installed by other managers, one entry per mod, and what the catalog knows about them. */
  groups: ForeignGroup[];
  known: Record<string, CachedMod | null>;
  /** What each addon installed by this app contains. */
  contents: AddonContent[];
  last: LastAction | null;
  busy: boolean;
  onToggle: (id: number, enabled: boolean) => void;
  onToggleForeign: (g: ForeignGroup, enabled: boolean) => void;
  onRemove: (id: number) => void;
  onUndo: () => void;
  onReorder: (ids: number[]) => void;
  /** Current game build, to flag mods installed on an older one. */
  gameBuild: string | null;
  onGoFixes: () => void;
  onGoProfiles: () => void;
  report: UpdateReport | null;
  checking: boolean;
  onCheckUpdates: () => void;
  onUpdate: (id: number) => void;
  /** Updates a mod that another manager installed, inside the app. */
  onUpdateForeign: (remoteId: string, name: string) => void;
  /** Updates the selected mods that have an update: ours by id, other managers' by catalog id. */
  onBulkUpdate: (ids: number[], foreignMods: { id: string; name: string }[]) => void;
  onGoCatalog: () => void;
  /** Camera check of the hero models inside the mods. */
  compat: Compat;
  gameFound: boolean;
  /** The game is running now: changes apply on its next start. */
  gameRunning: boolean;
  onBulkToggle: (ids: number[], groups: ForeignGroup[], enabled: boolean) => void;
  onBulkRemove: (ids: number[]) => void;
};

/** One tile of the library, whoever installed the mod. */
type Item = {
  key: string;
  title: string;
  thumb: string | null;
  sub: string;
  state: GroupState;
  content: Content | null;
  size: number;
  hero: string | null;
  kinds: string[];
  own?: InstalledMod;
  group?: ForeignGroup;
  update?: ModUpdate;
  /** Another manager's mod whose catalog page changed after the file was put here: maybe a newer version. */
  newer?: { modified: number; installed: number; url: string };
  /** Camera check: needs the repair, or has been repaired. */
  cam?: CamState;
  error?: string;
  /** The catalog (GameBanana) knows this mod. */
  found: boolean;
  /** Position in the load order for our own mods. */
  order: number;
};

type Status = "all" | "on" | "off";
type Sort = "load" | "name" | "size" | "state";

/** Catalog thumbnail at the size the catalog gives (a tile is wider than the 100 px preview). Defined here, not inside the page, so tiles keep their image when the list re-renders. */
function Thumb({ url }: { url: string }) {
  return (
    <img
      className="tile-img"
      src={url}
      alt=""
      loading="lazy"
      decoding="async"
      onError={(e) => {
        // the full-size picture is the default; the small one is only a fallback
        const img = e.currentTarget;
        const small = smallThumb(url);
        if (img.src !== small) img.src = small;
      }}
    />
  );
}

export default function Library({ mods, groups, known, contents, last, busy, onToggle, onToggleForeign, onRemove, onUndo, gameBuild, onGoFixes, onGoProfiles, report, checking, onCheckUpdates, onUpdate, onUpdateForeign, onBulkUpdate, onGoCatalog, compat, gameFound, gameRunning, onBulkToggle, onBulkRemove }: Props) {
  const { t } = useTranslation();
  const [picked, setPicked] = useState<Set<string>>(new Set()); // tiles selected for a bulk action
  const [query, setQuery] = useState("");
  const [status, setStatus] = useState<Status>("on");
  const [kind, setKind] = useState("");
  const [hero, setHero] = useState("");
  const [onlyUpdates, setOnlyUpdates] = useState(false);
  const [sort, setSort] = useState<Sort>("load");

  const pick = (key: string) =>
    setPicked((prev) => {
      const next = new Set(prev);
      if (!next.delete(key)) next.add(key);
      return next;
    });

  const own = mods ?? [];
  const portrait = useHeroPortraits();
  // the check is cheap to repeat and reads the mod files themselves, so it follows what is installed
  const filesKey = useMemo(() => [...own.flatMap((m) => m.files.map((f) => `${f.name}:${f.enabled}`)), ...groups.flatMap((g) => g.files.map((f) => `${f.name}:${f.enabled}`))].sort().join("|"), [own, groups]);
  const { scan: scanCompat } = compat;
  useEffect(() => {
    if (gameFound && mods && filesKey) void scanCompat();
  }, [gameFound, mods, filesKey, scanCompat]);
  // the catalog page was changed more than a day after the file arrived (a description edit does this too, so it is only a hint)
  const newerOnCatalog = (g: ForeignGroup, mod: CachedMod | null | undefined) => {
    const at = Math.min(...g.files.map((f) => f.installed_at ?? Infinity));
    if (!mod?.updated_at || !Number.isFinite(at) || mod.updated_at <= at + 86400) return undefined;
    return { modified: mod.updated_at, installed: at, url: mod.page_url };
  };
  const updateFor = (id: number): ModUpdate | undefined => report?.updates.find((u) => u.mod_id === id);

  const items: Item[] = useMemo(() => {
    const list: Item[] = [];
    own.forEach((m, i) => {
      const c = contentForFiles(contents, m.files.map((f) => f.name));
      const cat = m.remote_id ? known[m.remote_id] : null;
      const size = m.files.reduce((s, f) => s + f.size, 0);
      list.push({
        key: `mod-${m.id}`,
        title: m.name,
        thumb: cat?.thumbnail ?? null,
        sub: [m.version && `v${m.version}`, formatBytes(size), m.files.length > 1 && t("library.files", { count: m.files.length })].filter(Boolean).join(" · "),
        state: m.enabled ? "on" : "off",
        content: c,
        size,
        hero: (c ? realHeroes(c.heroes)[0]?.name : null) ?? null,
        kinds: c ? mainKinds(c, 6) : [],
        found: !!cat,
        own: m,
        update: updateFor(m.id),
        cam: camStateOf(compat.report, m.files.map((f) => f.name)) ?? undefined,
        order: i,
      });
    });
    groups.forEach((g, i) => {
      const mod = g.id ? known[g.id] : null;
      const hint = !g.id && g.content ? [...new Set(g.content.sample.map((p) => p.split("/").pop() ?? p))].slice(0, 2).join(", ") : null;
      const err = g.files.find((f) => f.error)?.error;
      list.push({
        key: `foreign-${g.key}`,
        title: groupTitle(g, known, t),
        thumb: mod?.thumbnail ?? null,
        sub: [mod ? [mod.author, mod.category].filter(Boolean).join(" · ") : hint, g.files.length > 1 ? t("library.files", { count: g.files.length }) : null, formatBytes(g.size)].filter(Boolean).join(" · "),
        state: groupState(g),
        content: g.content,
        size: g.size,
        hero: (g.content ? realHeroes(g.content.heroes)[0]?.name : null) ?? null,
        kinds: g.content ? mainKinds(g.content, 6) : [],
        found: !!mod,
        newer: newerOnCatalog(g, mod),
        group: g,
        error: err ?? undefined,
        cam: camStateOf(compat.report, g.files.map((f) => f.name)) ?? undefined,
        order: own.length + i,
      });
    });
    return list;
    // eslint-disable-next-line react-hooks/exhaustive-deps -- updateFor only reads `report`
  }, [own, groups, known, contents, report, compat.report, t]);

  const heroes = useMemo(() => [...new Set(items.map((i) => i.hero).filter((h): h is string => !!h))].sort((a, b) => a.localeCompare(b)), [items]);
  const kinds = useMemo(() => [...new Set(items.flatMap((i) => i.kinds))].sort(), [items]);

  const q = query.trim().toLowerCase();
  const visible = useMemo(() => {
    const hay = (i: Item) =>
      [i.title, i.sub, ...(i.content?.heroes.map((h) => `${h.name} ${h.code}`) ?? []), ...i.kinds.map((k) => t(`kind.${k}`)), ...(i.own?.files.map((f) => f.name) ?? []), ...(i.group?.files.map((f) => f.name) ?? []), i.group?.id ?? ""]
        .join(" ")
        .toLowerCase();
    const out = items.filter(
      (i) =>
        (!q || hay(i).includes(q)) &&
        (status === "all" || (status === "on" ? i.state !== "off" : i.state === "off")) &&
        (!kind || i.kinds.includes(kind)) &&
        (!hero || i.hero === hero) &&
        (!onlyUpdates || !!i.update || !!i.newer),
    );
    // mods the catalog does not know go after the ones it knows
    const known1 = (a: Item, b: Item) => Number(b.found) - Number(a.found);
    const byName = (a: Item, b: Item) => known1(a, b) || a.title.localeCompare(b.title);
    if (sort === "name") out.sort(byName);
    else if (sort === "size") out.sort((a, b) => known1(a, b) || b.size - a.size || byName(a, b));
    else if (sort === "state") out.sort((a, b) => Number(b.state !== "off") - Number(a.state !== "off") || byName(a, b));
    else out.sort((a, b) => a.order - b.order);
    return out;
  }, [items, q, status, kind, hero, onlyUpdates, sort, t]);

  const filtered = !!q || status !== "on" || !!kind || !!hero || onlyUpdates;
  const total = items.length;
  const totals = useMemo(
    () => ({ count: items.length, enabled: items.filter((i) => i.state !== "off").length, bytes: items.reduce((s, i) => s + i.size, 0) }),
    [items],
  );
  // mods that still need the camera repair, and the package that is written but not switched on
  const camMods = new Set((compat.report?.findings ?? []).filter((f) => f.kind.startsWith("camera_")).flatMap((f) => f.mods.map(shortName)));
  const canRepair = compat.report?.findings.some(isFixable) ?? false;
  const stale = gameBuild ? own.filter((m) => m.game_build && m.game_build !== gameBuild) : [];

  const pickedItems = items.filter((i) => picked.has(i.key));
  const pickedOwn = pickedItems.flatMap((i) => (i.own ? [i.own.id] : []));
  const updOwn = pickedItems.flatMap((i) => (i.own && i.update ? [i.own.id] : []));
  const updForeign = pickedItems.flatMap((i) => (i.group?.id && i.newer ? [{ id: i.group.id, name: i.title }] : []));
  const bulkUpdate = () => {
    onBulkUpdate(updOwn, updForeign);
    setPicked(new Set());
  };
  const bulk = (enabled: boolean) => {
    onBulkToggle(pickedOwn.length ? pickedItems.filter((i) => i.own && (enabled ? i.state !== "on" : i.state !== "off")).map((i) => i.own!.id) : [], pickedItems.filter((i) => i.group && (enabled ? i.state !== "on" : i.state !== "off")).map((i) => i.group!), enabled);
    setPicked(new Set());
  };

  const reset = () => {
    setQuery("");
    setStatus("on");
    setKind("");
    setHero("");
    setOnlyUpdates(false);
  };

  const undoLabel = last
    ? t("library.undo", {
        what: t(`action.${["install", "uninstall", "toggle", "profile", "reorder", "update", "foreign"].includes(last.kind) ? last.kind : "generic"}`, { name: last.mod_name ?? "" }),
      })
    : null;

  /** One mod as a rack row: the numbered slot plate first (the load order is the point of the page), then what it is. */
  const renderRow = (it: Item) => {
    const m = it.own;
    const g = it.group;
    const toggle = () => (m ? onToggle(m.id, it.state !== "on") : g && onToggleForeign(g, it.state !== "on"));
    const label = `${it.state === "on" ? t("home.disable") : t("home.enable")}: ${it.title}`;
    return (
      <li key={it.key} className={`rack-row ${it.state === "off" ? "rack-off" : ""} ${picked.has(it.key) ? "rack-picked" : ""}`}>
        <label className={`rack-pick ${picked.size > 0 ? "rack-pick-shown" : ""}`} title={t("library.select")}>
          <input type="checkbox" checked={picked.has(it.key)} aria-label={`${t("library.select")}: ${it.title}`} onChange={() => pick(it.key)} />
        </label>
        <div className="rack-thumb">{it.thumb ? <Thumb url={it.thumb} /> : <span className="no-pic" aria-hidden>KAE<br />MODS</span>}</div>
        <div className="rack-main">
          <div className="rack-title" title={it.title}>
            {it.title}
          </div>
          <div className="rack-sub">
            {it.error ? <span className="slot-error">{t("library.unreadable")}: {it.error}</span> : it.sub}
            {it.update && <span className="flag flag-update">{t("library.updateAvailable", { version: it.update.latest_version ? ` ${it.update.latest_version}` : "" })}</span>}
            {it.newer && <span className="flag flag-update">{t("library.newerOnCatalog")}</span>}
            {it.update && m?.remote_id && (
              <button className="flag flag-link" onClick={() => void openExternal(`https://gamebanana.com/mods/${m.remote_id}`)}>
                {t("library.whatsNew")}
                <Icon name="external" size={12} />
              </button>
            )}
            {it.cam && <span className={`flag flag-cam-${it.cam.state}`}>{t(`library.cam_${it.cam.state}`)}</span>}
          </div>
        </div>
        <div className="rack-tags">{it.content ? <Tags content={it.content} skipHero={!it.thumb && !!it.hero && !!g && !g.id} /> : <span className="rack-sub">{t("library.noContent")}</span>}</div>
        <div className="rack-actions">
          {m && it.update && (
            <button className="btn btn-sm" disabled={busy} onClick={() => onUpdate(m.id)}>
              {t("library.update")}
            </button>
          )}
          {g?.id && it.newer && (
            <button className="btn btn-sm" disabled={busy || gameRunning} onClick={() => onUpdateForeign(g.id!, it.title)}>
              {t("library.update")}
            </button>
          )}
          {m && (
            <button className="btn-icon btn-icon-danger" disabled={busy} aria-label={`${t("library.remove")}: ${m.name}`} title={t("library.remove")} onClick={() => onRemove(m.id)}>
              <Icon name="trash" size={16} />
            </button>
          )}
          <StateMark state={it.state} label={label} disabled={busy || !!it.error} onClick={toggle} />
        </div>
      </li>
    );
  };

  return (
    <section className="view">
      <header className="view-head">
        <div className="view-title-row">
          <h1 className="view-title">{t("library.title")}</h1>
          <div className="head-actions">
            <button className="btn" onClick={onGoProfiles}>
              {t("library.profilesLink")}
            </button>
            {total > 0 && gameFound && (
              <button className="btn" disabled={busy || compat.scanning || compat.repairing} onClick={() => void compat.scan()}>
                {compat.scanning ? t("fixes.compat.scanning") : t("library.checkCameras")}
              </button>
            )}
            {own.length > 0 && (
              <button className="btn" disabled={busy || checking} onClick={onCheckUpdates}>
                {checking ? t("library.checking") : t("library.checkUpdates")}
              </button>
            )}
            {undoLabel && (
              <button className="btn" disabled={busy} onClick={onUndo} title={undoLabel} aria-label={undoLabel}>
                {t("library.undoShort")}
              </button>
            )}
          </div>
        </div>
        <p className="view-sub">{t("library.subtitle")}</p>
        {groups.length > 0 && (
          <details className="howto">
            <summary>{t("library.howItWorks")}</summary>
            {groups.length > 0 && <p className="note">{t("library.foreignWarn")}</p>}
          </details>
        )}
        {report && !checking && (
          <p className="note">
            {report.updates.length > 0 ? t("library.updatesFound", { count: report.updates.length }) : t("library.upToDate", { count: report.checked })}
            {report.failed > 0 && ` ${t("library.checkFailed", { count: report.failed })}`}
            {report.offline && ` ${t("library.checkOffline")}`}
          </p>
        )}
        {compat.error && <p className="form-error">{compat.error}</p>}
        {compat.done && (repairTitle(t, compat.done) || compat.done.skipped.length > 0) && (
          <div className="notice ok-notice" role="status">
            {repairTitle(t, compat.done) && <b>{repairTitle(t, compat.done)}</b>}
            {compat.done.skipped.map(([m, why]) => (
              <span key={m}>
                {m}: {why}
              </span>
            ))}
            {repairTitle(t, compat.done) && <span>{t("library.camRestart")}</span>}
          </div>
        )}
        {canRepair && (
          <div className="notice" role="note">
            <b>{camMods.size > 0 ? t("library.camNeedsTitle", { count: camMods.size }) : t("fixes.compat.package_inactive")}</b>
            <span>{camMods.size > 0 ? t("library.camNeedsText") : t("fixes.compat.package_inactive_text")}</span>
            <div className="row">
              <button className="btn btn-primary" disabled={busy || compat.scanning || compat.repairing} onClick={() => void compat.repair()}>
                {compat.repairing ? "…" : t("fixes.compat.repair")}
              </button>
            </div>
          </div>
        )}
        {gameRunning && (
          <div className="notice" role="status">
            <b>{t("library.runningTitle")}</b>
            <span>{t("library.runningText")}</span>
          </div>
        )}
        {stale.length > 0 && (
          <div className="notice" role="note">
            <b>{t("library.patchTitle")}</b>
            <span>{t("library.patchText", { count: stale.length, build: gameBuild })}</span>
            <button className="btn" onClick={onGoFixes}>
              {t("library.patchOpen")}
            </button>
          </div>
        )}
      </header>

      {mods && total === 0 && (
        <EmptyState title={t("library.emptyTitle")} text={t("library.empty")}>
          <button className="btn btn-primary" onClick={onGoCatalog}>
            {t("library.toCatalog")}
          </button>
        </EmptyState>
      )}

      {total > 0 && (
        <div className="filterbar">
          <div className="lib-tools">
            <input className="field search" data-page-search type="search" value={query} placeholder={t("library.search")} aria-label={t("library.search")} onChange={(e) => setQuery(e.target.value)} />
            <span className="lib-totals">{t("library.totals", { count: totals.count, enabled: totals.enabled, size: formatBytes(totals.bytes) })}</span>
          </div>
          <div className="lib-filters" role="group" aria-label={t("library.filters")}>
            <div className="seg" role="radiogroup" aria-label={t("library.status")}>
              {(["all", "on", "off"] as const).map((s) => (
                <button key={s} type="button" role="radio" aria-checked={status === s} className={`seg-btn ${status === s ? "seg-on" : ""}`} onClick={() => setStatus(s)}>
                  {t(`library.status_${s}`)}
                </button>
              ))}
            </div>
            <Select className="select-filter" label={t("library.filterHero")} value={hero} onChange={setHero} options={[{ value: "", label: t("library.allHeroes") }, ...heroes.map((h) => ({ value: h, label: h, icon: portrait(h) }))]} />
            <Select className="select-filter" label={t("library.filterKind")} value={kind} onChange={setKind} options={[{ value: "", label: t("library.allKinds") }, ...kinds.map((k) => ({ value: k, label: t(`kind.${k}`) }))]} />
            {((report && report.updates.length > 0) || items.some((i) => i.newer)) && (
              <label className="check">
                <input type="checkbox" checked={onlyUpdates} onChange={(e) => setOnlyUpdates(e.target.checked)} />
                <span>{t("library.onlyUpdates")}</span>
              </label>
            )}
            <Select className="select-filter select-sort" label={t("library.sort")} value={sort} onChange={(v) => setSort(v as Sort)} options={(["load", "name", "size", "state"] as const).map((v) => ({ value: v, label: t(`library.sort_${v}`) }))} />
            {filtered && (
              <button className="btn btn-quiet" onClick={reset}>
                {t("library.resetFilters")}
              </button>
            )}
          </div>
        </div>
      )}

      {total > 0 && visible.length === 0 && (
        <EmptyState title={t("library.noMatchTitle")} text={t("library.noMatch")}>
          <button className="btn btn-primary" onClick={reset}>
            {t("library.resetFilters")}
          </button>
        </EmptyState>
      )}

      {visible.length > 0 && <ul className="rack">
          {visible.map(renderRow)}
        </ul>}

      {picked.size > 0 && (
        <div className="bulkbar" role="region" aria-label={t("library.bulk")}>
          <span className="bulk-count">{t("library.picked", { count: picked.size })}</span>
          <div className="bulk-actions">
            <button className="btn btn-sm" disabled={busy} onClick={() => bulk(true)}>
              {t("library.bulkEnable")}
            </button>
            <button className="btn btn-sm" disabled={busy} onClick={() => bulk(false)}>
              {t("library.bulkDisable")}
            </button>
            <button className="btn btn-sm btn-primary" disabled={busy || gameRunning || updOwn.length + updForeign.length === 0} onClick={bulkUpdate}>
              {t("library.bulkUpdate", { count: updOwn.length + updForeign.length })}
            </button>
            <button className="btn btn-sm btn-danger" disabled={busy || pickedOwn.length === 0} onClick={() => onBulkRemove(pickedOwn)}>
              {t("library.bulkRemove")}
            </button>
            <button className="btn btn-sm btn-quiet" onClick={() => setPicked(new Set(visible.map((i) => i.key)))}>
              {t("library.pickAll")}
            </button>
            <button className="btn btn-sm btn-quiet" onClick={() => setPicked(new Set())}>
              {t("library.pickNone")}
            </button>
          </div>
        </div>
      )}
    </section>
  );
}
