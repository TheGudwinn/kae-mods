import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { call, type Category, type Mod, type Page, type Sort } from "../../lib/api";
import { catClass, compact, humanizeError } from "../../lib/format";
import Select from "../../components/Select";
import { heroKey } from "../../lib/portraits";
import Icon from "../../components/Icons";
import EmptyState from "../../components/EmptyState";

type Props = {
  installedIds: Set<string>;
  onOpen: (m: Mod) => void;
  /** Text typed in the top-bar search; `n` changes on every submit so the same text can be sent again. */
  search?: { text: string; n: number };
  /** Category (and hero) picked on the home page; `n` changes on every pick. */
  preset?: { category: number | null; sub: number | null; n: number };
  onGoSettings: () => void;
};

const SORTS: Exclude<Sort, "default">[] = ["most_liked", "newest", "latest_modified", "most_viewed"];

function Heart() {
  return (
    <svg width="12" height="12" viewBox="0 0 16 16" aria-hidden="true" fill="currentColor">
      <path d="M8 14.2 2.3 8.6a3.6 3.6 0 0 1 5.1-5.1L8 4.1l.6-.6a3.6 3.6 0 0 1 5.1 5.1L8 14.2Z" />
    </svg>
  );
}

export default function Catalog({ installedIds, onOpen, search, preset, onGoSettings }: Props) {
  const { t } = useTranslation();
  const [text, setText] = useState(search?.text ?? "");
  const [query, setQuery] = useState("");
  const [category, setCategory] = useState<number | null>(null);
  const [sort, setSort] = useState<Sort>("most_liked");
  const [page, setPage] = useState(1);
  const [cats, setCats] = useState<Category[]>([]);
  const [data, setData] = useState<Page | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [reload, setReload] = useState(0);
  const [hideInstalled, setHideInstalled] = useState(false);
  // second level of the category tree: for Skins these are the heroes
  const [sub, setSub] = useState<number | null>(null);
  const [subs, setSubs] = useState<Category[]>([]);
  const [subQuery, setSubQuery] = useState("");
  // Sharp hero cards from the game's asset list; the 40 px GameBanana icon is the fallback.
  const [portraits, setPortraits] = useState<Record<string, string>>({});
  useEffect(() => {
    call<Record<string, string>>("hero_portraits").then((r) => setPortraits(r ?? {}), () => {});
  }, []);
  const portrait = (c: Category) => portraits[c.name.toLowerCase().replace(/^the /, "").replace(/[^\p{L}\p{N}]/gu, "")] ?? c.icon ?? undefined;
  const [subAll, setSubAll] = useState(false);
  // the list grows as the page is scrolled instead of switching pages
  const [acc, setAcc] = useState<Mod[]>([]);
  const sentinel = useRef<HTMLDivElement>(null);
  // a hero picked elsewhere is applied once the list of heroes of its category has loaded
  const pendingSub = useRef<number | null>(null);

  useEffect(() => {
    call<Category[]>("list_categories", { parent: null }).then(setCats, () => {});
  }, []);

  useEffect(() => {
    setSub(null);
    setSubs([]);
    setSubQuery("");
    setSubAll(false);
    const c = cats.find((x) => x.id === category);
    if (category === null || !c?.has_children) return;
    let stale = false;
    call<Category[]>("list_categories", { parent: category }).then(
      (r) => {
        if (stale) return;
        setSubs([...r].sort((a, b) => b.item_count - a.item_count || a.name.localeCompare(b.name)));
        if (pendingSub.current != null) {
          setSub(pendingSub.current);
          pendingSub.current = null;
        }
      },
      () => {},
    );
    return () => {
      stale = true;
    };
  }, [category, cats]);

  useEffect(() => {
    if (!preset) return;
    pendingSub.current = preset.sub;
    if (preset.category === category) {
      if (preset.sub != null) setSub(preset.sub);
      pendingSub.current = null;
    } else setCategory(preset.category);
    setPage(1);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [preset?.n]);

  useEffect(() => {
    if (search) setText(search.text);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [search?.n]);

  // Debounce typing so every keystroke is not a request.
  useEffect(() => {
    const id = setTimeout(() => {
      setQuery(text.trim());
      setPage(1);
    }, 400);
    return () => clearTimeout(id);
  }, [text]);

  useEffect(() => {
    let stale = false;
    setLoading(true);
    setError(null);
    call<Page>("search_mods", { query: { text: query || null, category: sub ?? category, sort, page, per_page: 24 } }).then(
      (res) => {
        if (stale) return;
        setData(res);
        setAcc((prev) => (page === 1 ? res.items : [...prev, ...res.items.filter((m) => !prev.some((x) => x.id === m.id))]));
        setLoading(false);
      },
      (e) => {
        if (stale) return;
        setError(humanizeError(t, e));
        setLoading(false);
      },
    );
    return () => {
      stale = true;
    };
  }, [query, category, sub, sort, page, reload, t]);

  const searching = query.length > 0;
  const items = hideInstalled ? acc.filter((m) => !installedIds.has(m.id)) : acc;
  const dirty = !!text || category !== null || sub !== null || sort !== "most_liked" || hideInstalled;
  const reset = () => {
    setText("");
    setQuery("");
    setCategory(null);
    setSub(null);
    setSort("most_liked");
    setHideInstalled(false);
    setPage(1);
  };

  const more = !!data?.has_more && !loading;
  useEffect(() => {
    const el = sentinel.current;
    if (!el || !more) return;
    const io = new IntersectionObserver((entries) => entries[0]?.isIntersecting && setPage((p) => p + 1), { rootMargin: "400px" });
    io.observe(el);
    return () => io.disconnect();
  }, [more, acc.length]);

  const shownSubs = useMemo(() => {
    const q = subQuery.trim().toLowerCase();
    const list = q ? subs.filter((c) => c.name.toLowerCase().includes(q)) : subs;
    return subAll || q ? list : list.slice(0, 14);
  }, [subs, subQuery, subAll]);

  return (
    <section className="view">
      <header className="view-head">
        <div className="view-title-row">
          <h1 className="view-title">{t("catalog.title")}</h1>
          {data && <span className="stat">{t("catalog.found", { count: data.total })}</span>}
        </div>
        <div className="toolbar">
          <div className="search-box">
            <input className="field search" data-page-search type="search" value={text} placeholder={t("catalog.search")} aria-label={t("catalog.search")} onChange={(e) => setText(e.target.value)} />
          </div>
          <Select
            label={t("catalog.sort.most_liked")}
            className="select-sort"
            value={sort}
            disabled={searching}
            onChange={(v) => {
              setSort(v as Sort);
              setPage(1);
            }}
            options={SORTS.map((s) => ({ value: s, label: t(`catalog.sort.${s}`) }))}
          />
          <label className="check">
            <input type="checkbox" checked={hideInstalled} onChange={(e) => setHideInstalled(e.target.checked)} />
            <span>{t("catalog.hideInstalled")}</span>
          </label>
          {dirty && (
            <button className="btn btn-quiet" onClick={reset}>
              <Icon name="close" size={14} />
              {t("catalog.reset")}
            </button>
          )}
        </div>
        <div className="cats" role="group" aria-label={t("catalog.allCategories")}>
          {[{ id: null as number | null, name: t("catalog.allCategories") }, ...cats].map((c) => (
            <button
              key={c.id ?? "all"}
              className="cat"
              aria-pressed={category === c.id}
              disabled={searching}
              onClick={() => {
                setCategory(c.id);
                setPage(1);
              }}
            >
              {c.name}
            </button>
          ))}
        </div>
        {subs.length > 0 && (
          <div className="subcats" role="group" aria-label={t("catalog.subs")}>
            <div className="subcats-head">
              <span className="subcats-title">{t("catalog.subs")}</span>
              {subs.length > 14 && (
                <input className="field subcats-search" type="search" value={subQuery} placeholder={t("catalog.subsSearch")} aria-label={t("catalog.subsSearch")} onChange={(e) => setSubQuery(e.target.value)} />
              )}
            </div>
            <div className="subcats-list">
              <button
                className="cat cat-sub"
                aria-pressed={sub === null}
                disabled={searching}
                onClick={() => {
                  setSub(null);
                  setPage(1);
                }}
              >
                {t("catalog.subAll")}
              </button>
              {shownSubs.map((c) => (
                <button
                  key={c.id}
                  className="cat cat-sub"
                  aria-pressed={sub === c.id}
                  disabled={searching}
                  onClick={() => {
                    setSub(c.id);
                    setPage(1);
                  }}
                >
                  {portrait(c) && <img className="cat-ico" src={portrait(c)} alt="" loading="lazy" />}
                  {c.name}
                  <small>{c.item_count}</small>
                </button>
              ))}
              {!subQuery && subs.length > 14 && (
                <button className="cat cat-more" onClick={() => setSubAll((v) => !v)}>
                  {subAll ? t("catalog.subsLess") : t("catalog.subsMore", { count: subs.length - 14 })}
                </button>
              )}
            </div>
          </div>
        )}
        {searching && <p className="note">{t("catalog.searchNote")}</p>}
        {data?.cached_at && <p className="note note-warn">{t("catalog.offline", { date: new Date(data.cached_at * 1000).toLocaleString() })}</p>}
      </header>

      {error && !data && (
        <EmptyState tone="problem" title={t("catalog.errorTitle")} text={error}>
          <button className="btn btn-primary" onClick={() => setReload((n) => n + 1)}>
            {t("catalog.retry")}
          </button>
          <button className="btn" onClick={onGoSettings}>
            {t("catalog.checkNetwork")}
          </button>
        </EmptyState>
      )}
      {error && data && (
        <div className="banner" role="alert">
          <span>{error}</span>
          <button className="btn" onClick={() => setReload((n) => n + 1)}>
            {t("catalog.retry")}
          </button>
        </div>
      )}

      {loading && !data && (
        <ul className="tiles" aria-hidden="true">
          {Array.from({ length: 8 }, (_, i) => (
            <li key={i}>
              <div className="skeleton skeleton-tile" />
            </li>
          ))}
        </ul>
      )}

      {data && items.length === 0 && !loading && (
        <EmptyState title={t("catalog.emptyTitle")} text={hideInstalled && acc.length > 0 ? t("catalog.allInstalled") : t("catalog.empty")}>
          {dirty && (
            <button className="btn btn-primary" onClick={reset}>
              {t("catalog.reset")}
            </button>
          )}
        </EmptyState>
      )}

      {data && items.length > 0 && (
        <>
          <ul className={`tiles catalog-tiles ${loading && page === 1 ? "grid-busy" : ""}`}>
            {items.map((m) => {
              const have = installedIds.has(m.id);
              return (
                <li key={m.id} className={`tile ${have ? "tile-have" : ""}`}>
                  <button className="tile-cover tile-cover-btn" onClick={() => onOpen(m)} aria-label={`${t("catalog.details")}: ${m.name}`}>
                    {m.thumbnail ? (
                      <img className="tile-img" src={m.thumbnail} alt="" loading="lazy" decoding="async" />
                    ) : (
                      <span className="tile-img tile-img-empty" aria-hidden="true">
                        {m.category ?? ""}
                      </span>
                    )}
                    <span className="tile-likes">
                      <Heart /> {compact(m.likes)}
                    </span>
                    {have && (
                      <span className="tile-have-badge" aria-hidden="true">
                        <Icon name="check" size={16} />
                      </span>
                    )}
                  </button>
                  <div className="tile-body">
                    <div className="tile-title" title={m.name}>
                      {m.name}
                    </div>
                    {m.category && (
                      <span className={`tile-cat ${catClass(m.category)}`}>
                        {portraits[heroKey(m.category)] && <img className="tag-ico" src={portraits[heroKey(m.category)]} alt="" />}
                        {m.category}
                      </span>
                    )}
                  </div>
                  <div className="tile-actions">
                    {have ? (
                      <span className="tile-done">
                        <Icon name="check" size={16} />
                        {t("catalog.installed")}
                      </span>
                    ) : (
                      <button className="btn btn-sm" onClick={() => onOpen(m)}>
                        {t("drawer.install")}
                      </button>
                    )}
                  </div>
                </li>
              );
            })}
          </ul>
          <div ref={sentinel} className="more" aria-live="polite">
            {loading ? (
              <span className="more-spin" aria-label={t("catalog.loadingMore")} />
            ) : data.has_more ? (
              <button className="btn" onClick={() => setPage((p) => p + 1)}>
                {t("catalog.showMore")}
              </button>
            ) : (
              <span className="stat">{t("catalog.end", { count: items.length })}</span>
            )}
          </div>
        </>
      )}
    </section>
  );
}
