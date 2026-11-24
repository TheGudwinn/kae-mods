import { Fragment, useEffect, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { call } from "../../lib/api";
import Icon from "../../components/Icons";

export type Hit = { provider: string; id: string; slug: string; title: string; description: string; icon: string | null; downloads: number; author: string; kind: string };

type Version = { id: string; name: string; version_number: string; version_type: string; game_versions: string[]; loaders: string[]; date_published: string; downloads: number; changelog: string | null; size: number };
type Detail = {
  id: string;
  slug: string;
  title: string;
  description: string;
  body: string;
  icon: string | null;
  downloads: number;
  followers: number;
  categories: string[];
  game_versions: string[];
  loaders: string[];
  updated: string;
  gallery: { url: string; title: string | null; description: string | null; featured: boolean }[] | null;
  team: string[];
  versions: Version[];
  source_url: string | null;
  issues_url: string | null;
  wiki_url: string | null;
};

const TABS = ["overview", "changelog", "gallery", "versions"] as const;
type Tab = (typeof TABS)[number];

const safeUrl = (u: string) => /^https:\/\//i.test(u.trim());

async function openLink(u: string) {
  if (!safeUrl(u)) return;
  try {
    const { openUrl } = await import("@tauri-apps/plugin-opener");
    await openUrl(u.trim());
  } catch {
    /* nothing to open with */
  }
}

const fmt = (n: number) => (n >= 1e6 ? `${(n / 1e6).toFixed(1)}M` : n >= 1e3 ? `${(n / 1e3).toFixed(1)}K` : String(n));
const size = (b: number) => (b >= 1048576 ? `${(b / 1048576).toFixed(1)} MB` : `${Math.max(1, Math.round(b / 1024))} KB`);

/** Inline markdown: code, images, links, bold and italics. Everything else is text, so nothing from a page runs. */
function inline(src: string, key: string): ReactNode[] {
  const out: ReactNode[] = [];
  const re = /`([^`]+)`|!\[([^\]]*)\]\(([^)\s]+)[^)]*\)|\[([^\]]+)\]\(([^)\s]+)[^)]*\)|\*\*([^*]+)\*\*|__([^_]+)__|\*([^*\s][^*]*)\*/g;
  let last = 0;
  let m: RegExpExecArray | null;
  let n = 0;
  while ((m = re.exec(src))) {
    if (m.index > last) out.push(src.slice(last, m.index));
    const k = `${key}-${n++}`;
    if (m[1] !== undefined) out.push(<code key={k}>{m[1]}</code>);
    else if (m[3] !== undefined) out.push(safeUrl(m[3]) ? <img key={k} src={m[3]} alt={m[2]} loading="lazy" referrerPolicy="no-referrer" /> : null);
    else if (m[5] !== undefined)
      out.push(
        <a
          key={k}
          href={m[5]}
          onClick={(ev) => {
            ev.preventDefault();
            void openLink(m![5]);
          }}
        >
          {inline(m[4], k)}
        </a>,
      );
    else if (m[6] !== undefined || m[7] !== undefined) out.push(<strong key={k}>{m[6] ?? m[7]}</strong>);
    else out.push(<em key={k}>{m[8]}</em>);
    last = m.index + m[0].length;
  }
  if (last < src.length) out.push(src.slice(last));
  return out;
}

function Markdown({ text }: { text: string }) {
  // html in a description is dropped, its text stays
  const clean = text
    .replace(/<!--[\s\S]*?-->/g, "")
    .replace(/<img[^>]*src=["']([^"']+)["'][^>]*>/gi, "![]($1)")
    .replace(/<br\s*\/?>/gi, "\n")
    .replace(/<\/?[a-z][^>]*>/gi, "");
  const lines = clean.split(/\r?\n/);
  const blocks: ReactNode[] = [];
  let i = 0;
  let k = 0;
  while (i < lines.length) {
    const l = lines[i];
    if (/^```/.test(l)) {
      const code: string[] = [];
      i++;
      while (i < lines.length && !/^```/.test(lines[i])) code.push(lines[i++]);
      i++;
      blocks.push(<pre key={k++}>{code.join("\n")}</pre>);
    } else if (/^\s*$/.test(l)) {
      i++;
    } else if (/^(-{3,}|\*{3,})\s*$/.test(l)) {
      blocks.push(<hr key={k++} />);
      i++;
    } else if (/^#{1,6}\s/.test(l)) {
      const level = Math.min(l.match(/^#+/)![0].length + 1, 6);
      const Tag = `h${level}` as "h2";
      blocks.push(<Tag key={k++}>{inline(l.replace(/^#+\s*/, ""), `h${k}`)}</Tag>);
      i++;
    } else if (/^\s*([-*+]|\d+\.)\s/.test(l)) {
      const ordered = /^\s*\d+\./.test(l);
      const items: string[] = [];
      while (i < lines.length && /^\s*([-*+]|\d+\.)\s/.test(lines[i])) items.push(lines[i++].replace(/^\s*([-*+]|\d+\.)\s+/, ""));
      const kk = k++;
      const li = items.map((x, j) => <li key={j}>{inline(x, `l${kk}-${j}`)}</li>);
      blocks.push(ordered ? <ol key={kk}>{li}</ol> : <ul key={kk}>{li}</ul>);
    } else if (/^>\s?/.test(l)) {
      const q: string[] = [];
      while (i < lines.length && /^>\s?/.test(lines[i])) q.push(lines[i++].replace(/^>\s?/, ""));
      blocks.push(<blockquote key={k++}>{inline(q.join(" "), `q${k}`)}</blockquote>);
    } else {
      const para: string[] = [];
      while (i < lines.length && !/^\s*$/.test(lines[i]) && !/^(#{1,6}\s|```|>\s?|\s*([-*+]|\d+\.)\s)/.test(lines[i])) para.push(lines[i++]);
      const kk = k++;
      blocks.push(
        <p key={kk}>
          {para.map((x, j) => (
            <Fragment key={j}>
              {j > 0 && <br />}
              {inline(x, `p${kk}-${j}`)}
            </Fragment>
          ))}
        </p>,
      );
    }
  }
  return <div className="mc-md">{blocks}</div>;
}

export default function ProjectPage({ hit, installed, installing, canInstall, installLabel, onInstall, onBack }: { hit: Hit; installed: boolean; installing: boolean; canInstall: boolean; installLabel: string; onInstall: () => void; onBack: () => void }) {
  const { t } = useTranslation();
  const [tab, setTab] = useState<Tab>("overview");
  const [d, setD] = useState<Detail | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [retry, setRetry] = useState(0);
  useEffect(() => {
    let gone = false;
    setError(null);
    call<Detail>("mc_project", { id: hit.id }).then(
      (x) => !gone && setD(x),
      (e) => !gone && setError(String(e)),
    );
    return () => {
      gone = true;
    };
  }, [hit.id, retry]);

  const gallery = d?.gallery ?? [];
  const latest = d?.game_versions?.[d.game_versions.length - 1];
  const date = (s: string) => new Date(s).toLocaleDateString();

  return (
    <div className="mc-proj">
      <button className="btn btn-quiet mc-back" onClick={onBack}>
        <Icon name="left" size={18} />
        {t("mc.proj.back")}
      </button>
      <header className="mc-proj-head">
        {hit.icon ? <img className="mc-proj-ico" src={hit.icon} alt="" /> : <span className="mc-proj-ico" />}
        <div className="mc-proj-title">
          <h2>{hit.title}</h2>
          <span className="note">{t("mc.proj.by", { who: d?.team?.length ? d.team.join(", ") : hit.author })}</span>
          <p>{d?.description ?? hit.description}</p>
        </div>
        <div className="mc-proj-act">
          {installed ? (
            <span className="mc-have">
              <Icon name="check" size={16} />
              {t("mc.alreadyHave")}
            </span>
          ) : (
            <button className="btn btn-primary" disabled={!canInstall || installing} aria-busy={installing} onClick={onInstall}>
              {installing ? t("mc.installing") : installLabel}
            </button>
          )}
        </div>
        <div className="mc-proj-meta">
          {d?.categories?.slice(0, 5).map((c) => (
            <span key={c} className="mc-chip">
              {t(`mc.cat.${c}`, { defaultValue: c })}
            </span>
          ))}
          <span className="mc-stats">
            <span>{t("mc.downloads", { n: fmt(d?.downloads ?? hit.downloads) })}</span>
            {d && <span>{t("mc.proj.updated", { date: date(d.updated) })}</span>}
            {latest && <span>{latest}</span>}
            {d?.loaders?.length ? <span>{d.loaders.map((l) => t(`mc.loaders.${l}`, { defaultValue: l })).join(", ")}</span> : null}
          </span>
        </div>
      </header>
      <div className="tabs" role="tablist">
        {TABS.map((x) => (
          <button key={x} role="tab" aria-selected={tab === x} className={`tab ${tab === x ? "tab-on" : ""}`} onClick={() => setTab(x)}>
            {t(`mc.proj.${x}`)}
          </button>
        ))}
      </div>
      {error && (
        <div className="banner" role="alert">
          <span>{t("mc.proj.failed")}</span>
          <button className="btn" onClick={() => setRetry(retry + 1)}>
            {t("mc.retry")}
          </button>
        </div>
      )}
      {!d && !error && <div className="mc-skeleton mc-proj-skel" aria-busy="true" />}
      {d && tab === "overview" && (
        <div className="mc-proj-body">
          <Markdown text={d.body || d.description} />
          {(d.source_url || d.issues_url || d.wiki_url) && (
            <div className="mc-row">
              {d.source_url && (
                <button className="btn" onClick={() => void openLink(d.source_url!)}>
                  {t("mc.proj.source")}
                </button>
              )}
              {d.issues_url && (
                <button className="btn" onClick={() => void openLink(d.issues_url!)}>
                  {t("mc.proj.issues")}
                </button>
              )}
              {d.wiki_url && (
                <button className="btn" onClick={() => void openLink(d.wiki_url!)}>
                  {t("mc.proj.wiki")}
                </button>
              )}
            </div>
          )}
        </div>
      )}
      {d && tab === "changelog" && (
        <div className="mc-proj-body">
          {d.versions.filter((v) => v.changelog?.trim()).length === 0 && <p className="note">{t("mc.proj.noChangelog")}</p>}
          {d.versions
            .filter((v) => v.changelog?.trim())
            .map((v) => (
              <section key={v.id} className="mc-ver-log">
                <h3>
                  {v.name || v.version_number} <span className="note">{date(v.date_published)}</span>
                </h3>
                <Markdown text={v.changelog!} />
              </section>
            ))}
        </div>
      )}
      {d && tab === "gallery" && (
        <div className="mc-proj-body">
          {gallery.length === 0 && <p className="note">{t("mc.proj.noGallery")}</p>}
          <ul className="mc-gallery">
            {gallery
              .filter((g) => safeUrl(g.url))
              .map((g) => (
                <li key={g.url}>
                  <button className="mc-thumb" onClick={() => void openLink(g.url)} title={g.title ?? ""}>
                    <img src={g.url} alt={g.title ?? ""} loading="lazy" referrerPolicy="no-referrer" />
                  </button>
                  {g.title && <span className="note">{g.title}</span>}
                </li>
              ))}
          </ul>
        </div>
      )}
      {d && tab === "versions" && (
        <div className="mc-proj-body">
          <table className="mc-table">
            <thead>
              <tr>
                <th>{t("mc.col.name")}</th>
                <th>{t("mc.proj.game")}</th>
                <th>{t("mc.loader")}</th>
                <th>{t("mc.proj.published")}</th>
                <th>{t("mc.col.size")}</th>
              </tr>
            </thead>
            <tbody>
              {d.versions.map((v) => (
                <tr key={v.id}>
                  <td>
                    <strong>{v.version_number}</strong>
                    <span className={`mc-chip mc-chip-${v.version_type}`}>{v.version_type}</span>
                  </td>
                  <td>{v.game_versions.slice(-3).join(", ")}</td>
                  <td>{v.loaders.join(", ")}</td>
                  <td>{date(v.date_published)}</td>
                  <td>{size(v.size)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
