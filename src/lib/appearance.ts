import { useSyncExternalStore } from "react";
import defaultWall from "../assets/deadlock/default-wall.webp";
import gradientWall from "../assets/backgrounds/gradient.webp";
import wavesWall from "../assets/backgrounds/waves.webp";

/** Pictures that ship with the app and can be picked as the background. */
export const WALL_PRESETS = { gradient: gradientWall, waves: wavesWall } as const;
export type WallPreset = keyof typeof WALL_PRESETS | "none";

/** What the user can change about how the launcher looks. Stored in localStorage, applied as attributes and CSS variables on <html>. */
export type Appearance = {
  /** Launcher look: a set of design tokens (see themes.css). */
  theme: "graphite" | "deadlock";
  motion: "full" | "calm" | "off";
  density: "compact" | "normal" | "roomy";
  /** Shipped background used when the player has not picked their own picture. */
  wallPreset: WallPreset;
  /** How much the picture behind the interface is darkened, 0-90, and blurred, 0-24 px. */
  wallDim: number;
  wallBlur: number;
  /** Sidebar: labels shown or icons only. */
  rail: "open" | "closed";
  /** Blocks of the home page. */
  home: { hero: boolean; popular: boolean; mine: boolean; stats: boolean };
};

export const DEFAULTS: Appearance = {
  theme: "graphite",
  motion: "full",
  density: "normal",
  wallPreset: "gradient",
  wallDim: 55,
  wallBlur: 0,
  rail: "closed",
  home: { hero: true, popular: true, mine: true, stats: true },
};

const KEY = "kea.appearance"; // new key: the old look settings (hue, glow, radius...) no longer exist

function load(): Appearance {
  try {
    const raw = localStorage.getItem(KEY);
    if (!raw) return DEFAULTS;
    const p = JSON.parse(raw) as Partial<Appearance>;
    if ((p.theme as string) === "glass") p.theme = "graphite";
    // the redesign made Graphite the base look: settings saved before it start over on Graphite once
    if (!(p as { v?: number }).v) p.theme = "graphite";
    return { ...DEFAULTS, ...p, home: { ...DEFAULTS.home, ...(p.home ?? {}) } };
  } catch {
    return DEFAULTS;
  }
}

let state: Appearance = load();
const listeners = new Set<() => void>();

const DENSITY = { compact: 0.78, normal: 1, roomy: 1.25 } as const;

/** Writes the settings to <html>. Called once at start and after every change. */
export function applyAppearance(a: Appearance = state) {
  const el = document.documentElement;
  el.style.setProperty("--d", String(DENSITY[a.density]));
  el.dataset.theme = a.theme;
  el.dataset.motion = a.motion;
  el.style.setProperty("--wall-dim", String(a.wallDim / 100));
  el.style.setProperty("--wall-blur", `${a.wallBlur}px`);
}

export function setAppearance(patch: Partial<Appearance>) {
  state = { ...state, ...patch };
  try {
    localStorage.setItem(KEY, JSON.stringify({ ...state, v: 2 }));
  } catch {
    /* private mode or full storage: the setting just lasts until restart */
  }
  applyAppearance(state);
  listeners.forEach((l) => l());
}

export const resetAppearance = () => {
  setWallpaper(null);
  setAppearance({ ...DEFAULTS, home: { ...DEFAULTS.home } });
};

/* ---- background picture: shrunk to a screen-sized JPEG and kept as a data URL ---- */
const WALL_KEY = "guddion.wallpaper";
let wall: string | null = null;
try {
  wall = localStorage.getItem(WALL_KEY);
} catch {
  /* storage unavailable */
}
const wallListeners = new Set<() => void>();

function setWallpaper(url: string | null) {
  wall = url;
  try {
    if (url) localStorage.setItem(WALL_KEY, url);
    else localStorage.removeItem(WALL_KEY);
  } catch {
    /* too big for storage: it stays for this session only */
  }
  wallListeners.forEach((l) => l());
}

export const clearWallpaper = () => setWallpaper(null);

/** Reads an image file, scales it down so it fits in storage, and makes it the background. */
export async function pickWallpaper(file: File): Promise<void> {
  if (!file.type.startsWith("image/")) throw new Error("not an image");
  const bmp = await createImageBitmap(file);
  const k = Math.min(1, 1920 / Math.max(bmp.width, bmp.height));
  const c = document.createElement("canvas");
  c.width = Math.round(bmp.width * k);
  c.height = Math.round(bmp.height * k);
  c.getContext("2d")!.drawImage(bmp, 0, 0, c.width, c.height);
  bmp.close();
  setWallpaper(c.toDataURL("image/jpeg", 0.82));
}

export function useWallpaper(): string | null {
  return useSyncExternalStore(
    (cb) => {
      wallListeners.add(cb);
      return () => wallListeners.delete(cb);
    },
    () => wall,
  );
}

/** The animated background shipped with the app: whatever file sits at src/assets/wall.mp4 or wall.webm (none yet). */
const videos = import.meta.glob("../assets/wall.{mp4,webm}", { eager: true, query: "?url", import: "default" }) as Record<string, string>;
export const wallVideo: string | null = Object.values(videos)[0] ?? null;

/** The picture the player chose, or the look's own picture (Deadlock has one) when nothing is chosen. */
export function useEffectiveWallpaper(): string | null {
  const own = useWallpaper();
  const a = useAppearance();
  // the shipped picture and video belong to the Deadlock skin; the base look is plain graphite
  if (own) return own;
  if (a.theme === "deadlock") return defaultWall;
  return a.wallPreset === "none" ? null : WALL_PRESETS[a.wallPreset] ?? null;
}

export function useAppearance(): Appearance {
  return useSyncExternalStore(
    (cb) => {
      listeners.add(cb);
      return () => listeners.delete(cb);
    },
    () => state,
  );
}
