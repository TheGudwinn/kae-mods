import type { Mod } from "../../lib/api";
import type { CachedMod } from "../../lib/modcache";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { call, type Profile } from "../../lib/api";
import { openIssue } from "../../lib/github";
import { codeModCount, parseCollections, useRemote } from "../../lib/remote";
import { useModInfo } from "../../lib/thumbs";
import { useHeroPortraits } from "../../lib/portraits";
import { catClass } from "../../lib/format";
import Icon from "../../components/Icons";
import Select from "../../components/Select";
import EmptyState from "../../components/EmptyState";

/** Catalog categories that name a kind of mod; any other category is a hero. */
const GENERIC = new Set(["Skins", "HUD", "Quality of Life/Fixes", "Animations", "Gameplay Modifications", "Maps", "Model Replacement", "Other/Misc", "Sounds", "Misc"]);

type Props = {
  profiles: Profile[] | null;
  busy: boolean;
  onCreate: (name: string) => Promise<boolean>;
  onApply: (p: Profile) => void;
  onUpdate: (p: Profile) => void;
  onRename: (id: number, name: string) => Promise<boolean>;
  onDelete: (id: number) => void;
  onExport: (p: Profile) => void;
  onImport: () => void;
  onCopyCode: (p: Profile) => void;
  onImportCode: (code: string) => Promise<boolean>;
  onGoLibrary: () => void;
  onOpen: (m: Mod) => void;
};

export default function Profiles({ profiles, busy, onCreate, onApply, onUpdate, onRename, onDelete, onExport, onImport, onCopyCode, onImportCode, onGoLibrary, onOpen }: Props) {
  const { t } = useTranslation();
  const [name, setName] = useState("");
  const [code, setCode] = useState("");
  const collections = useRemote("collections", parseCollections);
  const [suggest, setSuggest] = useState<{ id: string; note: string } | null>(null);
  const [suggestNote, setSuggestNote] = useState<string | null>(null);
  const sendSuggestion = async () => {
    if (!suggest || !suggest.id) return;
    const p = (profiles ?? []).find((x) => String(x.id) === suggest.id);
    if (!p) return;
    const profileCode = await call<string>("profile_code", { id: p.id });
    const fence = "`".repeat(3);
    const body = `**${p.name}**\n\n${suggest.note.trim()}\n\n${fence}\n${profileCode}\n${fence}`;
    const fits = await openIssue(`[collection] ${p.name}`, body);
    if (!fits) {
      await navigator.clipboard.writeText(profileCode).catch(() => {});
      setSuggestNote(t("collections.copied"));
    } else setSuggestNote(t("collections.opened"));
  };
  const [editing, setEditing] = useState<{ id: number; value: string } | null>(null);
  const portrait = useHeroPortraits();
  // a tile opens the mod's card, built from what the catalog told us about it
  const open = (m: { source: string; remote_id: string; name: string; version: string | null }, c?: CachedMod) =>
    onOpen({ source: m.source, id: m.remote_id, name: c?.name ?? m.name, author: c?.author ?? null, category: c?.category ?? null, thumbnail: c?.thumbnail ?? null, version: m.version, likes: 0, views: 0, updated_at: c?.updated_at ?? 0, page_url: c?.page_url ?? `https://gamebanana.com/mods/${m.remote_id}` });
  const info = useModInfo((profiles ?? []).flatMap((p) => p.mods.map((m) => m.remote_id)));

  return (
    <section className="view">
      <header className="view-head">
        <div className="view-title-row">
          <h1 className="view-title">{t("profiles.title")}</h1>
          <button className="btn" disabled={busy} onClick={onImport}>
            {t("profiles.import")}
          </button>
        </div>
        <p className="view-sub">{t("profiles.subtitle")}</p>
        <form
          className="toolbar"
          onSubmit={async (e) => {
            e.preventDefault();
            if (await onCreate(name)) setName("");
          }}
        >
          <input className="field search" value={name} maxLength={100} placeholder={t("profiles.newName")} aria-label={t("profiles.newName")} onChange={(e) => setName(e.target.value)} />
          <button className="btn btn-primary" type="submit" disabled={busy || !name.trim()}>
            {t("profiles.create")}
          </button>
        </form>
        <form
          className="toolbar"
          onSubmit={async (e) => {
            e.preventDefault();
            if (await onImportCode(code)) setCode("");
          }}
        >
          <input className="field search" value={code} placeholder={t("profiles.codePlaceholder")} aria-label={t("profiles.codePlaceholder")} onChange={(e) => setCode(e.target.value)} />
          <button className="btn" type="submit" disabled={busy || !code.trim()}>
            {t("profiles.codeImport")}
          </button>
        </form>
      </header>

      {profiles && profiles.length === 0 && (
        <EmptyState title={t("profiles.emptyTitle")} text={t("profiles.empty")}>
          <button className="btn btn-primary" onClick={onGoLibrary}>
            {t("profiles.toLibrary")}
          </button>
          <button className="btn" disabled={busy} onClick={onImport}>
            {t("profiles.import")}
          </button>
        </EmptyState>
      )}

      <ul className="profiles">
        {(profiles ?? []).map((p) => {
          const on = p.mods.filter((m) => m.enabled).length;
          const missing = p.mods.filter((m) => !m.installed).length;
          return (
            <li key={p.id} className={`profile ${p.active ? "profile-active" : ""}`}>
              <div className="profile-main">
                {editing?.id === p.id ? (
                  <form
                    className="rename"
                    onSubmit={async (e) => {
                      e.preventDefault();
                      if (await onRename(p.id, editing.value)) setEditing(null);
                    }}
                  >
                    <input className="field" autoFocus value={editing.value} maxLength={100} aria-label={t("profiles.rename")} onChange={(e) => setEditing({ id: p.id, value: e.target.value })} />
                    <button className="btn btn-primary" type="submit" disabled={busy || !editing.value.trim()}>
                      {t("profiles.save")}
                    </button>
                    <button className="btn btn-quiet" type="button" onClick={() => setEditing(null)}>
                      {t("profiles.cancel")}
                    </button>
                  </form>
                ) : (
                  <div className="profile-name">
                    {p.name}
                    {p.active && <span className="tag-active">{t("profiles.active")}</span>}
                  </div>
                )}
                <div className="slot-meta">
                  {t("profiles.counts", { total: p.mods.length, on })}
                  {missing > 0 && <span className="missing"> · {t("profiles.missingCount", { count: missing })}</span>}
                </div>
                <ul className="bm-grid">
                  {p.mods.length === 0 && <li className="note">{t("profiles.empty_set")}</li>}
                  {p.mods.map((m) => {
                    const c = info[m.remote_id];
                    return (
                      <li key={`${m.source}:${m.remote_id}`} className={`bm ${m.installed ? "bm-have" : ""} ${m.enabled ? "" : "bm-off"}`} title={m.name} role="button" tabIndex={0} onClick={() => open(m, c)} onKeyDown={(e) => (e.key === "Enter" || e.key === " ") && (e.preventDefault(), open(m, c))}>
                        <span className="bm-pic" style={c?.thumbnail ? { backgroundImage: `url(${c.thumbnail})` } : undefined}>
                          {!c?.thumbnail && <span className="no-pic" aria-hidden>KAE MODS</span>}
                          {m.installed && (
                            <span className="bm-check" aria-hidden="true">
                              <Icon name="check" size={13} />
                            </span>
                          )}
                        </span>
                        <span className="bm-name">{m.name}</span>
                        <span className="bm-foot">
                          {c?.category ? (
                            <span className="tags bm-tags">
                              <span className={`tag ${GENERIC.has(c.category) ? catClass(c.category) : "tag-hero"}`}>
                                {!GENERIC.has(c.category) && portrait(c.category) && <img className="tag-ico" src={portrait(c.category)} alt="" />}
                                {c.category}
                              </span>
                            </span>
                          ) : (
                            <span />
                          )}
                          <span className={`bm-state ${m.installed ? "bm-state-ok" : ""}`}>{m.installed ? (m.enabled ? t("profiles.on") : t("profiles.off")) : t("profiles.notInstalled")}</span>
                        </span>
                      </li>
                    );
                  })}
                </ul>
              </div>

              <div className="profile-actions">
                <button className="btn btn-primary" disabled={busy} onClick={() => onApply(p)}>
                  {t("profiles.apply")}
                </button>
                <button className="btn" disabled={busy} onClick={() => onUpdate(p)}>
                  {t("profiles.update")}
                </button>
                <button className="btn btn-quiet" disabled={busy} onClick={() => onCopyCode(p)}>
                  {t("profiles.copyCode")}
                </button>
                <button className="btn btn-quiet" disabled={busy} onClick={() => onExport(p)}>
                  {t("profiles.export")}
                </button>
                <button className="btn btn-quiet" disabled={busy} onClick={() => setEditing({ id: p.id, value: p.name })}>
                  {t("profiles.rename")}
                </button>
                <button className="btn btn-quiet" disabled={busy} onClick={() => onDelete(p.id)}>
                  {t("profiles.remove")}
                </button>
              </div>
            </li>
          );
        })}
      </ul>

      <section className="block collections">
        <div className="view-title-row">
          <h2 className="block-title">{t("collections.title")}</h2>
          <button className="btn" disabled={!profiles || profiles.length === 0} onClick={() => setSuggest(suggest ? null : { id: String(profiles?.[0]?.id ?? ""), note: "" })}>
            {t("collections.suggest")}
          </button>
        </div>
        <p className="note">{t("collections.text")}</p>
        {suggest && (
          <div className="notice ok-notice">
            <b>{t("collections.suggestTitle")}</b>
            <span>{t("collections.suggestText")}</span>
            <Select label={t("collections.pickProfile")} value={suggest.id} onChange={(v) => setSuggest({ ...suggest, id: v })} options={(profiles ?? []).map((p) => ({ value: String(p.id), label: p.name }))} />
            <input className="field" maxLength={300} value={suggest.note} placeholder={t("collections.notePlaceholder")} aria-label={t("collections.notePlaceholder")} onChange={(e) => setSuggest({ ...suggest, note: e.target.value })} />
            <div className="row">
              <button className="btn btn-primary" onClick={() => void sendSuggestion()}>
                {t("collections.send")}
              </button>
            </div>
            {suggestNote && <span>{suggestNote}</span>}
          </div>
        )}
        {collections === null && <p className="note">{t("news.loading")}</p>}
        {collections && collections.length === 0 && <p className="note">{t("collections.empty")}</p>}
        {collections && collections.length > 0 && (
          <ul className="collections-grid">
            {collections.map((c) => {
              const count = codeModCount(c.code);
              return (
                <li key={c.id} className="collection">
                  <b>{c.name}</b>
                  <p className="note">{c.description}</p>
                  <div className="collection-foot">
                    <span className="note">{[c.author, count != null ? t("collections.mods", { count }) : null].filter(Boolean).join(" · ")}</span>
                    <button className="btn btn-sm" disabled={busy} onClick={() => void onImportCode(c.code)}>
                      {t("collections.add")}
                    </button>
                  </div>
                </li>
              );
            })}
          </ul>
        )}
      </section>
    </section>
  );
}
