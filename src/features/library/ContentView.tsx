import { realHeroes } from "../../lib/heroes";
import { useHeroPortraits } from "../../lib/portraits";
import { useTranslation } from "react-i18next";
import type { Content } from "../../lib/api";
import { formatBytes } from "../../lib/format";

/** Short chips that say what an addon is: the hero and up to three kinds of files. Details live in `Peek`. */
export function Tags({ content, skipHero = false }: { content: Content; skipHero?: boolean }) {
  const { t } = useTranslation();
  const kinds = content.kinds.filter((k) => k.kind !== "other" || content.kinds.length === 1).slice(0, 3);
  const heroes = skipHero ? [] : realHeroes(content.heroes).slice(0, 2);
  const portrait = useHeroPortraits();
  return (
    <div className="tags">
      {heroes.map((h) => (
        <span key={h.code} className="tag tag-hero" title={`${t("library.heroHint")}: ${h.code}`}>
          {portrait(h.name) && <img className="tag-ico" src={portrait(h.name)} alt="" />}
          {h.name}
        </span>
      ))}
      {kinds.map((k) => (
        <span key={k.kind} className={`tag tag-${k.kind}`} title={`${k.count} · ${formatBytes(k.bytes)}`}>
          {t(`kind.${k.kind}`)}
        </span>
      ))}
    </div>
  );
}

/** The "what is inside" panel: breakdown by kind, top folders and a list of file paths. */
export function Peek({ content }: { content: Content }) {
  const { t } = useTranslation();
  const max = Math.max(1, ...content.kinds.map((k) => k.count));
  const hidden = content.total_files - content.sample.length;
  return (
    <div className="peek">
      <div className="peek-col">
        <h3>{t("library.peekKinds")}</h3>
        <ul className="kbars">
          {content.kinds.map((k) => (
            <li key={k.kind}>
              <span>{t(`kind.${k.kind}`)}</span>
              <i className={`kbar kbar-${k.kind}`} style={{ width: `${Math.max(4, (k.count / max) * 100)}%` }} />
              <span>
                {k.count} · {formatBytes(k.bytes)}
              </span>
            </li>
          ))}
        </ul>
        {content.folders.length > 0 && (
          <p className="peek-folders">
            {content.folders.map(([f, n]) => `${f}/ ${n}`).join(" · ")}
          </p>
        )}
        <p className="peek-note">{t("library.peekNote")}</p>
      </div>
      <div className="peek-col">
        <h3>{t("library.peekFiles", { count: content.total_files })}</h3>
        <ul className="paths">
          {content.sample.map((p) => (
            <li key={p} title={p}>
              {p}
            </li>
          ))}
          {hidden > 0 && <li className="paths-more">{t("library.peekMore", { count: hidden })}</li>}
        </ul>
      </div>
    </div>
  );
}
