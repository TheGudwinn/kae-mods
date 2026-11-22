import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { call, type GamePrefs, type Profile } from "../../lib/api";
import Select from "../../components/Select";
import EmptyState from "../../components/EmptyState";

/** A profile = a mod build + the game's launch settings, switched together. Kept on this computer. */
type Loadout = { id: string; name: string; build: number | null; prefs: GamePrefs };

const KEY = "kae.loadouts";
const ACTIVE = "kae.loadout.active";

function read(): Loadout[] {
  try {
    return JSON.parse(localStorage.getItem(KEY) ?? "[]") as Loadout[];
  } catch {
    return [];
  }
}
function write(list: Loadout[]) {
  try {
    localStorage.setItem(KEY, JSON.stringify(list));
  } catch {
    /* lasts until restart */
  }
}

/** Short labels of what a launch config changes, to show on the card. */
function chips(p: GamePrefs, t: (k: string) => string): string[] {
  const out: string[] = [];
  if (p.novid) out.push("-novid");
  if (p.console) out.push("-console");
  if (p.api !== "default") out.push(p.api === "dx11" ? "DX11" : "Vulkan");
  if (p.window !== "default") out.push(p.window === "windowed" ? t("loadouts.windowed") : t("loadouts.fullscreen"));
  if (p.fps_max > 0) out.push(`FPS ≤ ${p.fps_max}`);
  if (p.vsync !== "default") out.push(`VSync ${p.vsync === "on" ? t("loadouts.on") : t("loadouts.off")}`);
  if (p.show_fps) out.push(t("loadouts.showFps"));
  if (p.high_priority) out.push(t("loadouts.priority"));
  if (p.no_focus_sleep) out.push(t("loadouts.noSleep"));
  if (p.extra.trim()) out.push(p.extra.trim());
  return out;
}

type Props = {
  builds: Profile[] | null;
  busy: boolean;
  onApplyBuild: (p: Profile) => void;
  onGoBuilds: () => void;
};

export default function Loadouts({ builds, busy, onApplyBuild, onGoBuilds }: Props) {
  const { t } = useTranslation();
  const [list, setList] = useState<Loadout[]>(read);
  const [active, setActive] = useState<string | null>(() => {
    try {
      return localStorage.getItem(ACTIVE);
    } catch {
      return null;
    }
  });
  const [current, setCurrent] = useState<GamePrefs | null>(null);
  const [name, setName] = useState("");
  const [build, setBuild] = useState("");
  const [note, setNote] = useState<string | null>(null);

  useEffect(() => {
    call<GamePrefs>("get_game_prefs").then(setCurrent, () => {});
  }, []);

  const commit = (next: Loadout[]) => {
    setList(next);
    write(next);
  };
  const buildName = (id: number | null) => builds?.find((b) => b.id === id)?.name ?? null;
  const buildOptions = [{ value: "", label: t("loadouts.noBuild") }, ...(builds ?? []).map((b) => ({ value: String(b.id), label: b.name }))];

  const create = () => {
    if (!name.trim() || !current) return;
    commit([...list, { id: String(Date.now()), name: name.trim(), build: build ? Number(build) : null, prefs: current }]);
    setName("");
    setBuild("");
    setNote(t("loadouts.created", { name: name.trim() }));
  };

  const activate = async (l: Loadout) => {
    try {
      const saved = await call<GamePrefs>("save_game_prefs", { prefs: l.prefs });
      setCurrent(saved);
      const b = builds?.find((x) => x.id === l.build);
      if (b) onApplyBuild(b);
      setActive(l.id);
      try {
        localStorage.setItem(ACTIVE, l.id);
      } catch {
        /* forgotten on restart */
      }
      setNote(t("loadouts.applied", { name: l.name }));
    } catch {
      setNote(t("loadouts.failed"));
    }
  };

  return (
    <section className="view">
      <header className="view-head">
        <div className="view-title-row">
          <h1 className="view-title">{t("loadouts.title")}</h1>
        </div>
        <p className="view-sub">{t("loadouts.subtitle")}</p>
        <form
          className="toolbar lo-create"
          onSubmit={(e) => {
            e.preventDefault();
            create();
          }}
        >
          <input className="field search" value={name} maxLength={60} placeholder={t("loadouts.newName")} aria-label={t("loadouts.newName")} onChange={(e) => setName(e.target.value)} />
          <Select label={t("loadouts.build")} value={build} onChange={setBuild} options={buildOptions} />
          <button className="btn btn-primary" type="submit" disabled={busy || !name.trim() || !current}>
            {t("loadouts.create")}
          </button>
        </form>
        {note && <p className="note lo-note">{note}</p>}
      </header>

      {list.length === 0 && (
        <EmptyState title={t("loadouts.emptyTitle")} text={t("loadouts.empty")}>
          <button className="btn" onClick={onGoBuilds}>
            {t("loadouts.toBuilds")}
          </button>
        </EmptyState>
      )}

      <ul className="lo-grid">
        {list.map((l) => {
          const bn = buildName(l.build);
          const b = builds?.find((x) => x.id === l.build);
          const cs = chips(l.prefs, (k) => t(k));
          return (
            <li key={l.id} className={`lo ${active === l.id ? "lo-active" : ""}`}>
              <div className="lo-head">
                <h2 className="lo-name">{l.name}</h2>
                {active === l.id && <span className="tag-active">{t("loadouts.active")}</span>}
              </div>

              <div className="lo-part">
                <small>{t("loadouts.partBuild")}</small>
                <Select
                  label={t("loadouts.build")}
                  value={l.build == null ? "" : String(l.build)}
                  onChange={(v) => commit(list.map((x) => (x.id === l.id ? { ...x, build: v ? Number(v) : null } : x)))}
                  options={buildOptions}
                />
                <span className="note">{b ? t("loadouts.modsCount", { count: b.mods.length }) : bn ? "" : t("loadouts.noBuildHint")}</span>
              </div>

              <div className="lo-part">
                <small>{t("loadouts.partConfig")}</small>
                <div className="lo-chips">
                  {cs.length === 0 ? <span className="note">{t("loadouts.defaultConfig")}</span> : cs.map((c) => <span key={c} className="tile-cat lo-chip">{c}</span>)}
                </div>
              </div>

              <div className="profile-actions">
                <button className="btn btn-primary" disabled={busy} onClick={() => void activate(l)}>
                  {t("loadouts.apply")}
                </button>
                <button
                  className="btn"
                  disabled={busy || !current}
                  onClick={() => {
                    if (!current) return;
                    commit(list.map((x) => (x.id === l.id ? { ...x, prefs: current } : x)));
                    setNote(t("loadouts.updated", { name: l.name }));
                  }}
                >
                  {t("loadouts.update")}
                </button>
                <button
                  className="btn btn-quiet"
                  onClick={() => {
                    commit(list.filter((x) => x.id !== l.id));
                    if (active === l.id) setActive(null);
                  }}
                >
                  {t("loadouts.remove")}
                </button>
              </div>
            </li>
          );
        })}
      </ul>
    </section>
  );
}
