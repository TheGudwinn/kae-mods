import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { call, type Category, type GameInstall, type InstalledMod, type Mod, type Page, type UpdateReport } from "../../lib/api";
import { compact, formatBytes } from "../../lib/format";
import Icon from "../../components/Icons";
import Carousel from "../../components/Carousel";
import Onboarding from "../../components/Onboarding";
import StateMark from "../../components/StateMark";
import { groupState, groupTitle, type ForeignGroup } from "../../lib/foreign";
import { smallThumb, type CachedMod } from "../../lib/modcache";
import { useAppearance } from "../../lib/appearance";
import { openExternal } from "../../lib/open";
import { parseNews, useRemote } from "../../lib/remote";

type Props = {
  installed: InstalledMod[] | null;
  /** Mods installed by other managers (one entry per mod) and what the catalog knows about them. */
  groups: ForeignGroup[];
  known: Record<string, CachedMod | null>;
  report: UpdateReport | null;
  busy: boolean;
  onOpen: (m: Mod) => void;
  onToggle: (id: number, enabled: boolean) => void;
  onToggleForeign: (g: ForeignGroup, enabled: boolean) => void;
  onUpdate: (id: number) => void;
  onGo: (view: "catalog" | "library" | "settings") => void;
  game: GameInstall | null | undefined;
  launched: boolean;
  showOnboarding: boolean;
  onHideOnboarding: () => void;
  onPlay: () => void;
  /** Open the catalog on a category, and on a hero inside it. */
  onBrowse: (category: number | null, sub: number | null) => void;
};

type Shelf = { cat: Category; items: Mod[] };

/** GameBanana serves a 530 px thumbnail by default; an 800 px version of the same file exists and is sharp enough for the hero. */
const larger = (url: string) => url.replace("/530-90_", "/800-90_");

export default function Home({ installed, groups, known, report, busy, onOpen, onToggle, onToggleForeign, onUpdate, onGo, game, launched, showOnboarding, onHideOnboarding, onPlay, onBrowse }: Props) {
  const { t } = useTranslation();
  const news = useRemote("news", parseNews);
  const { home: show } = useAppearance();
  const [popular, setPopular] = useState<Mod[] | null>(null);
  const [offline, setOffline] = useState(false);

  useEffect(() => {
    let stale = false;
    call<Page>("search_mods", { query: { text: null, category: null, sort: "most_liked", page: 1, per_page: 13 } }).then(
      (p) => {
        if (stale) return;
        setPopular(p.items);
        setOffline(p.cached_at != null);
      },
      () => !stale && setPopular([]),
    );
    return () => {
      stale = true;
    };
  }, []);

  // Heroes (second level of Skins) and one "trending" shelf per biggest category.
  const [skins, setSkins] = useState<Category | null>(null);
  const [heroes, setHeroes] = useState<Category[]>([]);
  const [portraits, setPortraits] = useState<Record<string, string>>({});
  const [shelves, setShelves] = useState<Shelf[] | null>(null);
  useEffect(() => {
    let stale = false;
    (async () => {
      const top = await call<Category[]>("list_categories", { parent: null });
      const sk = top.find((c) => c.name.toLowerCase() === "skins") ?? null;
      if (stale) return;
      setSkins(sk);
      if (sk) call<Category[]>("list_categories", { parent: sk.id }).then((r) => !stale && setHeroes(r), () => {});
      const chosen = [...top].filter((c) => c.item_count > 0).sort((a, b) => b.item_count - a.item_count).slice(0, 4);
      const rows = await Promise.all(
        chosen.map((cat) =>
          call<Page>("search_mods", { query: { text: null, category: cat.id, sort: "most_liked", page: 1, per_page: 12 } }).then(
            (p) => ({ cat, items: p.items }),
            () => ({ cat, items: [] as Mod[] }),
          ),
        ),
      );
      if (!stale) setShelves(rows.filter((r) => r.items.length > 0));
    })().catch(() => !stale && setShelves([]));
    return () => {
      stale = true;
    };
  }, []);

  useEffect(() => {
    // Sharp hero cards (the GameBanana icons are only 40 px); without them the small icon is used.
    call<Record<string, string>>("hero_portraits").then((r) => setPortraits(r ?? {}), () => {});
  }, []);
  const portrait = (c: Category) => portraits[c.name.toLowerCase().replace(/^the /, "").replace(/[^\p{L}\p{N}]/gu, "")] ?? c.icon;

  const hero = popular?.[0];
  const mods = installed ?? [];
  // Everything in the game folder counts, not only what this manager installed.
  const total = mods.length + groups.length;
  const enabled = mods.filter((m) => m.enabled).length + groups.filter((g) => g.enabled > 0).length;
  const SHOWN = 6;
  // Enabled mods first in this short list (the library keeps a fixed order).
  const foreignShown = [...groups].sort((a, b) => Number(b.enabled > 0) - Number(a.enabled > 0)).slice(0, Math.max(0, SHOWN - mods.length));
  const updateOf = (id: number) => report?.updates.find((u) => u.mod_id === id);
  const disk = mods.reduce((s, m) => s + m.files.reduce((a, f) => a + f.size, 0), 0) + groups.reduce((s, g) => s + g.size, 0);

  return (
    <section className="view">
      <header className="view-head">
        <h1 className="view-title">{t("home.title")}</h1>
        <p className="view-sub">{t("home.subtitle")}</p>
      </header>

      {showOnboarding && !(!!game && total > 0 && launched) && game !== undefined && (
        <Onboarding gameFound={!!game} hasMods={total > 0} launched={launched} onGoSettings={() => onGo("settings")} onGoCatalog={() => onGo("catalog")} onPlay={onPlay} onHide={onHideOnboarding} />
      )}

      <div className="home">
        {show.hero && (
          <div className="hero">
            {hero?.thumbnail && <img className="hero-art" src={larger(hero.thumbnail)} alt="" decoding="async" />}
            <div className="hero-body">
              <span className="tag-pill">{t("home.popularNow")}</span>
              {hero ? (
                <>
                  <h2 className="hero-title">{hero.name}</h2>
                  <p className="hero-meta">
                    {[hero.author, hero.category].filter(Boolean).join(" · ")}
                    <span className="like like-inline">
                      <Icon name="heart" /> {compact(hero.likes)}
                    </span>
                  </p>
                  <div className="row">
                    <button className="btn btn-primary" onClick={() => onOpen(hero)}>
                      {t("home.open")}
                    </button>
                    <button className="btn" onClick={() => onGo("catalog")}>
                      {t("home.toCatalog")}
                    </button>
                  </div>
                </>
              ) : (
                <>
                  <h2 className="hero-title">{popular ? t("home.noCatalog") : "…"}</h2>
                  {popular && popular.length === 0 && <p className="hero-meta">{t("home.noCatalogHint")}</p>}
                </>
              )}
              {offline && <p className="note note-warn">{t("home.offline")}</p>}
            </div>
          </div>
        )}

        {show.hero && (
          <div className="hot">
            <h3 className="hot-title">{t("home.popularNow")}</h3>
            <ul className="hot-list">
              {(popular ?? []).slice(1, 4).map((m) => (
                <li key={m.id}>
                  <button className="hot-row" onClick={() => onOpen(m)}>
                    {m.thumbnail ? <img src={m.thumbnail} alt="" loading="lazy" decoding="async" /> : <span className="hot-ph">{m.name.slice(0, 1)}</span>}
                    <span className="hot-name">
                      <b title={m.name}>{m.name}</b>
                      <small>{[m.author, m.category].filter(Boolean).join(" · ")}</small>
                    </span>
                    <span className="hot-go" aria-hidden="true">
                      <Icon name="right" size={16} />
                    </span>
                  </button>
                </li>
              ))}
              {popular === null && [0, 1, 2].map((i) => <li key={i} className="hot-row hot-skel" />)}
            </ul>
          </div>
        )}

        {show.popular && (
          <div className="popular">
            {heroes.length > 0 && skins && (
              <Carousel className="heroes" title={t("home.heroes")}>
                {heroes.map((c) => (
                  <li key={c.id}>
                    <button className="hero-chip" title={t("home.pickHero", { name: c.name })} aria-label={t("home.pickHero", { name: c.name })} onClick={() => onBrowse(skins.id, c.id)}>
                      {portrait(c) ? <img src={portrait(c)!} alt="" loading="lazy" decoding="async" /> : <span className="hero-chip-ph">{c.name.slice(0, 1)}</span>}
                      <span className="hero-chip-name">{c.name}</span>
                    </button>
                  </li>
                ))}
              </Carousel>
            )}
            {(shelves ?? []).map(({ cat, items }) => (
              <Carousel
                key={cat.id}
                title={
                  <>
                    <Icon name="trend" size={18} /> {t("home.trendingIn")} <em>{cat.name}</em>
                  </>
                }
                action={
                  <button className="btn btn-sm btn-ghost" onClick={() => onBrowse(cat.id, null)}>
                    {t("home.seeAll")} <Icon name="right" size={14} />
                  </button>
                }
              >
                {items.map((m) => (
                  <li key={m.id}>
                    <button className="tcard" onClick={() => onOpen(m)}>
                      {m.thumbnail ? <img src={m.thumbnail} alt="" loading="lazy" decoding="async" /> : <span className="tcard-ph">{m.name.slice(0, 1)}</span>}
                      <span className="tcard-go" aria-hidden="true">
                        <Icon name="downloads" size={16} />
                      </span>
                      <span className="tcard-body">
                        <b title={m.name}>{m.name}</b>
                        <small>{m.author}</small>
                        <span className="tcard-likes">
                          <Icon name="heart" size={13} /> {compact(m.likes)}
                        </span>
                      </span>
                    </button>
                  </li>
                ))}
              </Carousel>
            ))}
            {shelves && shelves.length === 0 && <p className="note">{t("home.noCatalog")}</p>}
          </div>
        )}

        <aside className="side">
          <div className="panel panel-news">
            <h3 className="panel-title">{t("news.title")}</h3>
            {news === null && <p className="note">{t("news.loading")}</p>}
            {news && news.length === 0 && <p className="note">{t("news.empty")}</p>}
            {news && news.length > 0 && (
              <ul className="newslist">
                {news.slice(0, 4).map((n) => (
                  <li key={n.id}>
                    <div className="news-meta">
                      {n.tag && <span className="tag">{n.tag}</span>}
                      <time>{n.date}</time>
                    </div>
                    {n.url ? (
                      <button className="link news-title" onClick={() => void openExternal(n.url!)}>
                        {n.title}
                      </button>
                    ) : (
                      <b className="news-title">{n.title}</b>
                    )}
                    {n.text && <p className="note">{n.text}</p>}
                  </li>
                ))}
              </ul>
            )}
          </div>
          {show.mine && (
          <div className="panel panel-mine">
            <h3 className="panel-title">{t("home.mine")}</h3>
            {total === 0 ? (
              <div className="empty-panel">
                <p>{t("library.empty")}</p>
                <button className="btn btn-primary" onClick={() => onGo("catalog")}>
                  {t("library.toCatalog")}
                </button>
              </div>
            ) : (
              <ul className="mine-list">
                {mods.slice(0, SHOWN).map((m) => {
                  const up = updateOf(m.id);
                  return (
                    <li key={m.id} className={`mine-row ${m.enabled ? "" : "mine-off"}`}>
                      <div className="plate plate-sm plate-nopic">
                        <span className="no-pic" aria-hidden>KAE<br />MODS</span>
                      </div>
                      <div className="mine-name">
                        <b>{m.name}</b>
                        <small>{[m.version && `v${m.version}`, m.enabled ? t("library.enabled") : t("library.disabled")].filter(Boolean).join(" · ")}</small>
                        {up && (
                          <button className="btn btn-sm mine-update" disabled={busy} title={t("library.updateAvailable", { version: up.latest_version ? ` ${up.latest_version}` : "" })} onClick={() => onUpdate(m.id)}>
                            {t("library.update")}
                            {up.latest_version ? ` → ${up.latest_version}` : ""}
                          </button>
                        )}
                      </div>
                      <StateMark state={m.enabled ? "on" : "off"} label={`${m.enabled ? t("home.disable") : t("home.enable")}: ${m.name}`} disabled={busy} onClick={() => onToggle(m.id, !m.enabled)} />
                    </li>
                  );
                })}
                {foreignShown.map((g) => {
                  const mod = g.id ? known[g.id] : undefined;
                  const state = groupState(g);
                  return (
                    <li key={g.key} className={`mine-row ${state === "off" ? "mine-off" : ""}`}>
                      {mod?.thumbnail ? <img className="mine-thumb" src={smallThumb(mod.thumbnail)} alt="" loading="lazy" decoding="async" /> : <div className="mine-thumb mine-thumb-empty" />}
                      <div className="mine-name">
                        <b>{groupTitle(g, known, t)}</b>
                        <small>{[mod?.author, formatBytes(g.size)].filter(Boolean).join(" · ")}</small>
                      </div>
                      <StateMark state={state} label={`${state === "on" ? t("home.disable") : t("home.enable")}: ${groupTitle(g, known, t)}`} disabled={busy} onClick={() => onToggleForeign(g, state !== "on")} />
                    </li>
                  );
                })}
              </ul>
            )}
            {total > SHOWN && (
              <button className="btn btn-quiet" onClick={() => onGo("library")}>
                {t("home.allMods", { count: total })}
              </button>
            )}
          </div>
          )}

          {show.stats && (
          <div className="panel panel-stats">
            <h3 className="panel-title">{t("home.stats")}</h3>
            <div className="ring-card">
              <svg className="ring-svg" viewBox="0 0 120 120" aria-hidden="true">
                <circle cx="60" cy="60" r="50" className="ring-track" />
                <circle cx="60" cy="60" r="50" className="ring-fill" pathLength={100} strokeDasharray={`${total ? Math.round((enabled / total) * 100) : 0} 100`} />
              </svg>
              <div className="ring-center">
                <small>{t("home.ringEnabled")}</small>
                <b>{total ? `${enabled}/${total}` : "–"}</b>
              </div>
            </div>
            <ul className="stat-pills">
              <li>
                <small>{t("home.disk")}</small>
                <b>{formatBytes(disk)}</b>
              </li>
              <li>
                <small>{t("home.foreign")}</small>
                <b>{groups.length}</b>
              </li>
              <li>
                <small>{t("home.ringUpToDate")}</small>
                <b>{report ? (report.updates.length > 0 ? t("home.uptodate", { count: report.updates.length }) : t("home.allFresh")) : t("home.notChecked")}</b>
              </li>
            </ul>
          </div>
          )}
        </aside>
      </div>
    </section>
  );
}
