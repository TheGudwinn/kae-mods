// Development-only sample data so the UI can be previewed in a regular browser.
import type { AddonContent, Content, InstalledMod, Mod, ModDetails, QueueRow, SettingsView } from "./api";

const img = "https://images.gamebanana.com/img/ss/mods/";
const mk = (id: number, name: string, author: string, category: string, likes: number, views: number, thumb: string): Mod => ({
  source: "gamebanana",
  id: String(id),
  name,
  author,
  category,
  thumbnail: thumb ? `${img}530-90_${thumb}` : null,
  version: "1.0",
  likes,
  views,
  updated_at: 1794371049,
  page_url: `https://gamebanana.com/mods/${id}`,
});

const MODS: Mod[] = [
  mk(650634, "QOL Lock [INSTANT SAVE/LOAD UPDATE]", "civo", "Quality of Life/Fixes", 307, 236549, "6995649268824.jpg"),
  mk(664041, "Enhanced QuickBuy", "northwind", "HUD", 211, 98211, ""),
  mk(671416, "Old Ability Icons Restored", "mara_k", "HUD", 154, 61804, ""),
  mk(698127, "Cynthia as Silver HUD", "lowtide", "Skins", 98, 40112, ""),
  mk(725413, "4:3 Stretched HUD Fix (Healthbar + Shop)", "pk4", "HUD", 76, 25560, ""),
  mk(694343, "Vindicta Minimal Scope Crosshair", "ashgrove", "Gameplay Modifications", 64, 19907, ""),
];

const INSTALLED: InstalledMod[] = [
  {
    id: 3,
    source: "gamebanana",
    name: "QOL Lock [INSTANT SAVE/LOAD UPDATE]",
    version: "4.0.5",
    remote_id: "650634",
    slot: 1,
    enabled: true,
    game_build: "26546669",
    files: [{ name: "ddmm3_pak01_dir.vpk", size: 10116149, present: true, enabled: true }],
  },
  {
    id: 7,
    source: "gamebanana",
    name: "Old Ability Icons Restored",
    version: "1.2",
    remote_id: "671416",
    slot: 2,
    enabled: true,
    game_build: "26401200",
    files: [{ name: "ddmm7_pak02_dir.vpk", size: 2318004, present: true, enabled: true }],
  },
];

let settings: SettingsView = {
  proxy: { mode: "system", kind: "http", host: "", port: 0, username: null, has_password: false },
  mirrors: ["https://gamebanana.com"],
  game_path: null,
  check_updates: true,
  update_endpoints: ["https://github.com/example/deadlock-mm/releases/latest/download/latest.json"],
};

const hud: Content = {
  total_files: 64,
  total_bytes: 10116149,
  kinds: [
    { kind: "interface", count: 58, bytes: 612_000 },
    { kind: "textures", count: 6, bytes: 9_504_149 },
  ],
  heroes: [],
  sample: ["panorama/layout/hud/hud_ability_bar.vxml_c", "panorama/styles/hud/ability_bar.vcss_c", "panorama/images/hud/lock.vtex_c"],
  folders: [["panorama", 58], ["materials", 6]],
};
const icons: Content = {
  total_files: 31,
  total_bytes: 2318004,
  kinds: [{ kind: "textures", count: 31, bytes: 2_318_004 }],
  heroes: [],
  sample: ["materials/hud/ability_icons/abrams_1.vtex_c", "materials/hud/ability_icons/haze_1.vtex_c"],
  folders: [["materials", 31]],
};
const skin = (name: string, code: string): Content => ({
  total_files: 412,
  total_bytes: 67_069_166,
  kinds: [
    { kind: "textures", count: 268, bytes: 51_000_000 },
    { kind: "models", count: 96, bytes: 14_200_000 },
    { kind: "effects", count: 48, bytes: 1_869_166 },
  ],
  heroes: [{ code, name, files: 380 }],
  sample: [`models/heroes_wip/${code}/${code}.vmdl_c`, `models/heroes_wip/${code}/${code}_arms.vmdl_c`, `materials/models/heroes_wip/${code}/body_color.vtex_c`],
  folders: [["materials", 268], ["models", 96], ["particles", 48]],
});
const CONTENTS: AddonContent[] = [
  { name: "ddmm3_pak01_dir.vpk", content: hud },
  { name: "ddmm7_pak02_dir.vpk", content: icons },
];

const QUEUE: QueueRow[] = [
  { id: 1, remote_id: "698127", name: "Cynthia as Silver HUD", file_name: "cynthia_silver_hud.zip", state: "downloading", downloaded: 14_300_000, total: 41_000_000, error: null, slot: null, overlaps: null },
  { id: 2, remote_id: "725413", name: "4:3 Stretched HUD Fix (Healthbar + Shop)", file_name: "hud43_fix.rar", state: "queued", downloaded: 0, total: 3_200_000, error: null, slot: null, overlaps: null },
  { id: 3, remote_id: "694343", name: "Vindicta Minimal Scope Crosshair", file_name: "vindicta_scope.7z", state: "paused", downloaded: 1_100_000, total: 5_000_000, error: null, slot: null, overlaps: null },
  { id: 4, remote_id: "664041", name: "Enhanced QuickBuy", file_name: "quickbuy.zip", state: "failed", downloaded: 0, total: 900_000, error: "cannot read archive: bad zip", slot: null, overlaps: null },
  { id: 5, remote_id: "650634", name: "QOL Lock [INSTANT SAVE/LOAD UPDATE]", file_name: "qollock_405.zip", state: "done", downloaded: 10_116_149, total: 10_116_149, error: null, slot: 1, overlaps: 1 },
];

const wait = (ms: number) => new Promise((r) => setTimeout(r, ms));

let mockForeign = true; // design preview: another manager's set is in gameinfo.gi until the user presses Fix

export async function mockInvoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  await wait(120);
  const r = (v: unknown) => v as T;
  switch (cmd) {
    case "game_running":
      return r(false);
    case "detect_game":
      return r({ root: "D:\\SteamLibrary\\steamapps\\common\\Deadlock", addons_dir: "", gameinfo: "", build_id: "26546669" });
    case "list_categories":
      if (args?.parent === 2) {
        const heroes = ["Abrams", "Apollo", "Bebop", "Billy", "Calico", "Celeste", "Doorman", "Drifter", "Dynamo", "Grey Talon", "Haze", "Holliday", "Infernus", "Ivy", "Kelvin", "Lady Geist", "Lash", "McGinnis", "Mina", "Mirage", "Mo & Krill", "Paige", "Paradox", "Pocket", "Seven", "Shiv", "Sinclair", "Victor", "Vindicta", "Viscous", "Vyper", "Warden", "Wraith", "Yamato"];
        return r(heroes.map((name, i) => ({ id: 100 + i, name, item_count: 200 - i * 4, has_children: false, icon: null })));
      }
      if (args?.parent === 1) return r([{ id: 201, name: "Crosshairs", item_count: 120, has_children: false, icon: null }, { id: 202, name: "Health bars", item_count: 90, has_children: false, icon: null }]);
      return r([
        { id: 1, name: "HUD", item_count: 865, has_children: true, icon: null },
        { id: 2, name: "Skins", item_count: 2210, has_children: true, icon: null },
        { id: 3, name: "Quality of Life/Fixes", item_count: 140, has_children: false, icon: null },
      ]);
    case "perf_fix_foreign":
      mockForeign = false;
      return r({ applied: null, settings: 0, foreign: false });
    case "perf_state": {
      const P = (id: string, name: string, author: string, tier: string, blurb: string, settings: number, recommended = false) => ({ id, name, author, tier, blurb, recommended, url: null, license: "MIT", settings, skipped: 61 });
      return r({
        presets: [
          P("eskay", "Eskay's Config", "Eskay", "pretty", "A fork of Sqooky's config with moderate changes to suit Eskay's preferences.", 351),
          P("dmm", "DMM Clean", "Deadlock Mod Manager", "balanced", "The settings most community configs agree on, without anything that hides objects, changes the camera or what you can see, or overrides the in-game video menu.", 96, true),
          P("optlock", "OptimizationLock", "Sqooky", "balanced", "Sqooky's main config. Upstream describes it as performance-oriented without making the game ugly and recommends it for most players.", 353),
          P("sqooky", "Sqooky's Max FPS", "Sqooky", "sweaty", "Sqooky's frame-rate-first config. Upstream marks it as under development and not fully documented.", 356),
          P("potato", "OptiLock Potato", "dacooderr", "potato", "Lowest settings for weak machines.", 340),
          P("kai", "Kaizuchaneru's Minimum Spec", "Kaizuchaneru", "potato", "Strips everything that is not needed to keep the game playable.", 310),
        ],
        status: { applied: null, settings: 0, foreign: mockForeign },
      });
    }
    case "search_mods":
      return r({ items: MODS, total: 6062, page: 1, has_more: true, cached_at: null });
    case "mod_details": {
      const m = MODS.find((x) => x.id === args?.id) ?? MODS[0];
      const d: ModDetails = {
        summary: m,
        description_html: "<p>Sample description for the design preview.</p>",
        downloads: 202826,
        images: m.thumbnail ? [m.thumbnail] : [],
        files: [
          { id: "1", name: "qollock_405_16october.zip", size: 10116149, url: "", md5: "8901a2ab494ba23669b4f18533651830", av_clean: true },
          { id: "2", name: "alt_version.rar", size: 4412009, url: "", md5: null, av_clean: false },
        ],
        cached_at: null,
      };
      return r(d);
    }
    case "list_installed":
      return r(INSTALLED);
    case "list_foreign_addons":
      return r([
        // One mod, several files, all switched off (id prefix): shown as a single entry.
        { name: "569650_pak06_dir.vpk", size: 67069166, entries: 412, error: null, content: skin("Vindicta", "hornet"), remote_id: "569650", enabled: false },
        { name: "569650_pak07_dir.vpk", size: 5_900_000, entries: 40, error: null, content: hud, remote_id: "569650", enabled: false },
        { name: "pak01_dir.vpk", size: 78026643, entries: 530, error: null, content: { ...skin("Haze", "haze"), total_files: 530 }, remote_id: "659083", enabled: true },
        { name: "pak05_dir.vpk", size: 27_900_000, entries: 90, error: null, content: hud, remote_id: null, enabled: true },
        { name: "broken_pak99_dir.vpk", size: 1024, entries: null, error: "bad vpk header", content: null, remote_id: null, enabled: false },
      ]);
    case "fetch_content":
      return r(
        args?.kind === "news"
          ? JSON.stringify([
              { id: "n1", date: "2026-11-20", title: "Патч изменил камеру героев", text: "Часть модов с моделями может показывать героя со сбитой камерой. Диагностика находит такие моды.", tag: "Патч" },
              { id: "n2", date: "2026-11-18", title: "Kae Mods 0.2: диагностика после патча", text: "Список модов на старой сборке и поиск виновника делением пополам.", tag: "Приложение" },
            ])
          : JSON.stringify([]),
      );
    case "scan_compat":
      return r({ findings: [], fixed: [], details: { mods_scanned: 4, hero_models: 1, models_looked_into: 1, mods_unreadable: [], skipped: [] } });
    case "describe_installed":
      return r(CONTENTS);
    case "list_profiles":
      return r([
        {
          id: 1,
          name: "Рейтинг",
          active: true,
          mods: [
            { source: "gamebanana", remote_id: "650634", name: "QOL Lock [INSTANT SAVE/LOAD UPDATE]", version: "4.0.5", enabled: true, position: 0, installed: true },
            { source: "gamebanana", remote_id: "671416", name: "Old Ability Icons Restored", version: "1.2", enabled: false, position: 1, installed: true },
          ],
        },
        {
          id: 2,
          name: "Со скинами",
          active: false,
          mods: [
            { source: "gamebanana", remote_id: "650634", name: "QOL Lock [INSTANT SAVE/LOAD UPDATE]", version: "4.0.5", enabled: true, position: 0, installed: true },
            { source: "gamebanana", remote_id: "698127", name: "Cynthia as Silver HUD", version: "1.0", enabled: true, position: 1, installed: false },
          ],
        },
      ]);
    case "apply_profile":
      return r({ enabled: 1, disabled: 1, missing: [] });
    case "get_settings":
      return r(settings);
    case "save_settings": {
      const inp = (args as { input: { proxy: { mode: string; kind: string; host: string; port: number; username: string | null; password: string | null }; mirrors: string[]; game_path: string | null } }).input;
      settings = {
        proxy: {
          mode: inp.proxy.mode as "system",
          kind: inp.proxy.kind as "http",
          host: inp.proxy.host,
          port: inp.proxy.port,
          username: inp.proxy.username,
          has_password: !!inp.proxy.password || (settings.proxy.has_password && inp.proxy.password === null),
        },
        mirrors: inp.mirrors.map((m) => m.trim()).filter(Boolean),
        game_path: inp.game_path,
        check_updates: (args as { input: { check_updates: boolean } }).input.check_updates,
        update_endpoints: (args as { input: { update_endpoints: string[] } }).input.update_endpoints.map((m) => m.trim()).filter(Boolean),
      };
      return r(settings);
    }
    case "run_diagnostics":
      return r([
        {
          url: "https://gamebanana.com",
          host: "gamebanana.com",
          verdict: "ok",
          steps: [
            { name: "dns", state: "ok", ms: 14, detail: "172.67.12.8, 104.21.4.3" },
            { name: "tcp", state: "ok", ms: 31, detail: "172.67.12.8:443" },
            { name: "http", state: "ok", ms: 212, detail: "200" },
          ],
        },
        {
          url: "https://api.github.com",
          host: "api.github.com",
          verdict: "tcp_failed",
          steps: [
            { name: "dns", state: "ok", ms: 22, detail: "140.82.121.6" },
            { name: "tcp", state: "failed", ms: 8001, detail: "timeout" },
          ],
        },
      ]);
    case "reorder_mods": {
      const ids = (args as { order: number[] }).order;
      const slots = INSTALLED.map((m) => m.slot).sort((a, b) => (a ?? 0) - (b ?? 0));
      const next = ids.map((id) => INSTALLED.find((m) => m.id === id)!);
      next.forEach((m, i) => (m.slot = slots[i]));
      INSTALLED.splice(0, INSTALLED.length, ...next);
      return r(undefined);
    }
    case "check_mod_updates":
      return r({ updates: [{ mod_id: 7, name: "Old Ability Icons Restored", installed_version: "1.2", latest_version: "1.3", file_name: "old_icons_v13.zip" }], checked: 2, failed: 0, offline: false });
    case "update_mod":
      return r({ name: "Old Ability Icons Restored", slot: 2, overlaps: [] });
    case "list_queue":
      return r(QUEUE);
    case "enqueue_install":
    case "pause_download":
    case "resume_download":
    case "cancel_download":
    case "remove_download":
    case "clear_finished_downloads":
    case "pause_all_downloads":
    case "resume_all_downloads":
      return r(1);
    case "library_stats":
      return r({ foreign: 3, disk_bytes: 12_434_153, conflicts: 1 });
    case "app_version":
      return r("0.1.0");
    case "check_update":
      return r({ version: "0.2.0", current: "0.1.0", notes: "Порядок модов, профили, настройки сети." });
    case "install_update":
      return r(undefined);
    case "last_action":
      return r({ kind: "install", mod_name: "Old Ability Icons Restored" });
    case "install_mod":
      return r({ mod_id: 9, name: "Sample", slot: 3, overlaps: [] });
    default:
      return r(undefined);
  }
}
