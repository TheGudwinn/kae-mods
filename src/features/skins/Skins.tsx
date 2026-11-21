import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { call, type AddonContent, type InstalledMod } from "../../lib/api";
import { groupTitle, type ForeignGroup } from "../../lib/foreign";
import type { CachedMod } from "../../lib/modcache";
import { HEROES, heroOfCode } from "../../lib/heroes";
import EmptyState from "../../components/EmptyState";
import HeroViewer from "./HeroViewer";

type Props = {
  mods: InstalledMod[] | null;
  groups: ForeignGroup[];
  known: Record<string, CachedMod | null>;
  contents: AddonContent[];
  gameFound: boolean;
  onToggle: (modId: number, enabled: boolean) => void;
  onToggleForeign: (g: ForeignGroup, enabled: boolean) => void;
  onGoCatalog: () => void;
  /** Opens the catalog searching for this text (a hero name). */
  onFind: (text: string) => void;
};

/** A hero picture fetched through the app (shrunk and kept on disk), shown once it has arrived. */
function ArtImage({ url, className }: { url: string; className: string }) {
  const [src, setSrc] = useState<string | null>(null);
  useEffect(() => {
    let stale = false;
    let made: string | null = null;
    call<ArrayBuffer | number[]>("hero_art", { url, width: 960 }).then(
      (raw) => {
        if (stale) return;
        made = URL.createObjectURL(new Blob([raw instanceof ArrayBuffer ? raw : new Uint8Array(raw)], { type: "image/jpeg" }));
        setSrc(made);
      },
      () => {},
    );
    return () => {
      stale = true;
      if (made) URL.revokeObjectURL(made);
    };
  }, [url]);
  return src ? <img className={className} src={src} alt="" draggable={false} /> : <span className={className} aria-hidden="true" />;
}

/** Poster material of a hero (see the `hero_posters` command). */
type Poster = { name: string; tags: string[]; role: string; lore: string; color: string | null; background: string | null; cutout: string | null };
const heroKey = (name: string) => name.toLowerCase().replace(/^the /, "").replace(/[^\p{L}\p{N}]/gu, "");

type Skin = {
  key: string;
  title: string;
  thumb: string | null;
  /** File in the addons folder (without `.disabled`) that carries this hero's model. */
  file: string;
  slot: number;
  enabled: boolean;
  own: number | null;
  foreign: ForeignGroup | null;
  /** The folder name this mod uses for the hero (`archer`), needed to find its model. */
  code: string;
};

const slotOf = (name: string) => {
  const m = /pak(\d+)_dir/i.exec(name);
  return m ? Number(m[1]) : 99;
};
const plain = (n: string) => n.replace(/\.disabled$/i, "");

export default function Skins({ mods, groups, known, contents, gameFound, onToggle, onToggleForeign, onGoCatalog, onFind }: Props) {
  const { t, i18n } = useTranslation();
  const [posters, setPosters] = useState<Record<string, Poster>>({});
  useEffect(() => {
    let stale = false;
    call<Record<string, Poster>>("hero_posters", { lang: i18n.language.startsWith("ru") ? "russian" : "english" }).then(
      (r) => !stale && setPosters(r ?? {}),
      () => {},
    );
    return () => {
      stale = true;
    };
  }, [i18n.language]);
  // hero portraits for the list (a hero without one just shows the name)
  const [portraits, setPortraits] = useState<Record<string, string>>({});
  useEffect(() => {
    call<Record<string, string>>("hero_portraits").then((r) => setPortraits(r ?? {}), () => {});
  }, []);
  const portrait = (name: string) => portraits[name.toLowerCase().replace(/^the /, "").replace(/[^\p{L}\p{N}]/gu, "")];
  const [query, setQuery] = useState("");
  const [picked, setPicked] = useState<string | null>(null);
  // undefined: what the hero wears now; null: the base game's model; a string: that mod's file
  const [preview, setPreview] = useState<string | null | undefined>(undefined);

  // hero name -> one skin per mod (a mod that ships several VPKs still counts once)
  const byHero = useMemo(() => {
    const heroOf = new Map(contents.map((c) => [c.name, c.content.model_heroes ?? []] as const));
    const out = new Map<string, Map<string, Skin & { weight: number }>>();
    const add = (code: string, weight: number, modKey: string, s: Skin) => {
      const hero = heroOfCode(code);
      if (!hero) return;
      const per = out.get(hero.name) ?? new Map();
      const cur = per.get(modKey);
      if (!cur || weight > cur.weight) per.set(modKey, { ...s, code, weight });
      out.set(hero.name, per);
    };
    for (const m of mods ?? []) {
      const cat = m.remote_id ? known[m.remote_id] : null;
      for (const f of m.files) {
        for (const h of heroOf.get(plain(f.name)) ?? []) {
          add(h.code, h.files, `m${m.id}`, { key: `m${m.id}`, title: m.name, thumb: cat?.thumbnail ?? null, file: plain(f.name), slot: slotOf(f.name), enabled: m.enabled, own: m.id, foreign: null, code: h.code });
        }
      }
    }
    for (const g of groups) {
      for (const f of g.files) {
        const cat = g.id ? known[g.id] : null;
        for (const h of f.content?.model_heroes ?? []) {
          add(h.code, h.files, `f${g.key}`, { key: `f${g.key}`, title: groupTitle(g, known, t), thumb: cat?.thumbnail ?? null, file: plain(f.name), slot: slotOf(f.name), enabled: g.enabled > 0, own: null, foreign: g, code: h.code });
        }
      }
    }
    return new Map([...out].map(([name, per]) => [name, [...per.values()]] as const));
  }, [mods, groups, known, contents, t]);

  const roster = HEROES;

  const list = roster.filter((h) => h.name.toLowerCase().includes(query.trim().toLowerCase()));
  const hero = roster.find((h) => h.name === picked) ?? list.find((h) => byHero.has(h.name)) ?? list[0] ?? null;
  const skins = hero ? [...(byHero.get(hero.name) ?? [])].sort((a, b) => a.slot - b.slot) : [];
  // when files overlap the game is assumed to take the lowest slot (not verified in the game)
  const wearing = skins.find((s) => s.enabled) ?? null;
  const shown = preview === undefined ? (wearing?.file ?? null) : preview;
  const shownSkin = skins.find((s) => s.file === shown) ?? null;

  if (!gameFound) return <EmptyState title={t("skins.noGame")} text={t("skins.noGameText")} />;

  const toggle = (s: Skin) => {
    if (s.own != null) onToggle(s.own, !s.enabled);
    else if (s.foreign) onToggleForeign(s.foreign, !s.enabled);
  };
  const pick = (name: string) => {
    setPicked(name);
    setPreview(undefined);
  };

  return (
    <div className="view skins">
      <div className="view-head">
        <h1 className="view-title">{t("skins.title")}</h1>
        <p className="view-sub">{t("skins.subtitle")}</p>
      </div>
      <div className="skins-grid">
        <aside className="skins-heroes block">
          <input className="field" placeholder={t("skins.search")} value={query} onChange={(e) => setQuery(e.target.value)} />
          <ul>
            {list.map((h) => {
              const all = byHero.get(h.name) ?? [];
              const on = all.filter((s) => s.enabled).sort((a, b) => a.slot - b.slot)[0];
              return (
                <li key={h.name}>
                  <button className="skins-hero" aria-current={hero?.name === h.name ? "true" : undefined} onClick={() => pick(h.name)}>
                    {portrait(h.name) ? <img className="skins-hero-ico" src={portrait(h.name)} alt="" loading="lazy" /> : <span className="skins-hero-ico skins-hero-ph" aria-hidden="true">{h.name.slice(0, 1)}</span>}
                    <span className="skins-hero-name">{h.name}</span>
                    <span className="skins-hero-sub">{on ? on.title : t("skins.default")}</span>
                    {all.length > 0 && <span className="rail-badge">{all.length}</span>}
                  </button>
                </li>
              );
            })}
          </ul>
        </aside>
        <div className="skins-stage">
        <aside className="skins-preview block">
          <h3 className="block-title">{t("skins.preview")}</h3>
          <p className="note">{shownSkin ? shownSkin.title : t("skins.default")}</p>
          {hero && <HeroViewer hero={hero.codes[0]} codes={shownSkin ? [shownSkin.code, ...hero.codes] : hero.codes} addon={shownSkin?.file ?? null} />}
        </aside>
        <section className="skins-center">
          {hero && (
            <>
              {(() => {
                const poster = posters[heroKey(hero.name)];
                return (
                  <header className="hero-poster" style={{ ...(poster?.color ? { "--hero": poster.color } : {}), "--n": Math.max(4, (poster?.name ?? hero.name).length) } as React.CSSProperties}>
                    {poster?.background && <ArtImage key={poster.background} className="hero-poster-bg" url={poster.background} />}
                    {poster?.cutout && <img key={poster.cutout} className="hero-poster-cut" src={poster.cutout} alt="" decoding="async" draggable={false} />}
                    <div className="hero-poster-body">
                      {poster?.role && <span className="hero-poster-role">{poster.role}</span>}
                      <h2 className="hero-poster-name">{poster?.name ?? hero.name}</h2>
                      {poster && poster.tags.length > 0 && (
                        <div className="hero-poster-tags">
                          {poster.tags.map((x) => (
                            <span key={x} className="hero-sticker">
                              {x}
                            </span>
                          ))}
                        </div>
                      )}
                      {poster?.lore && <p className="hero-poster-lore">{poster.lore}</p>}
                      <p className="hero-poster-wear">
                        {t("skins.wearing")}: <b>{wearing ? wearing.title : t("skins.default")}</b> · {t("skins.count", { count: skins.length })}
                      </p>
                    </div>
                  </header>
                );
              })()}
              <div className="block skins-head">
                <h2 className="block-title">{hero.name}</h2>
                <p className="note">
                  {t("skins.wearing")}: <b>{wearing ? wearing.title : t("skins.default")}</b> · {t("skins.count", { count: skins.length })}
                </p>
              </div>
              <div className="block skins-find">
                <div>
                  <b>{t("skins.findTitle")}</b>
                  <p className="note">{t("skins.findText", { name: hero.name })}</p>
                </div>
                <button className="btn" onClick={() => onFind(hero.name)}>
                  {t("skins.findBtn", { name: hero.name })}
                </button>
              </div>
              <div className="skins-list">
                <button className="skin-card" aria-pressed={shown === null} onClick={() => setPreview(null)}>
                  <span className="skin-thumb skin-thumb-default" />
                  <b>{t("skins.default")}</b>
                  <span className="note">{t("skins.defaultText")}</span>
                </button>
                {skins.map((s) => (
                  <div key={s.key} className="skin-card-wrap">
                    <button className="skin-card" aria-pressed={shown === s.file} onClick={() => setPreview(s.file)}>
                      {s.thumb ? <img className="skin-thumb" src={s.thumb} alt="" loading="lazy" /> : <span className="skin-thumb skin-thumb-default" />}
                      <b>{s.title}</b>
                      <span className="note">{s.enabled ? (s === wearing ? t("skins.worn") : t("skins.on")) : t("skins.off")}</span>
                    </button>
                    <button className="btn btn-sm" onClick={() => toggle(s)}>
                      {s.enabled ? t("skins.disable") : t("skins.enable")}
                    </button>
                  </div>
                ))}
              </div>
              {skins.length === 0 && (
                <p className="note">
                  {t("skins.none")}{" "}
                  <button className="link" onClick={onGoCatalog}>
                    {t("skins.browse")}
                  </button>
                </p>
              )}
            </>
          )}
        </section>
        </div>
      </div>
    </div>
  );
}
