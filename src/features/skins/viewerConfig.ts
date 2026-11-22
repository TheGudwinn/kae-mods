import { isTauri } from "../../lib/api";

export type Bg = "dark" | "slate" | "light";
export type LightPreset = "studio" | "soft" | "dramatic";

export type ViewerCfg = {
  wire: boolean;
  floor: boolean;
  gizmo: boolean;
  spin: boolean;
  bg: Bg;
  /** Smooth edges (SMAA post-process). */
  smaa: boolean;
  /** Glow on strongly self-lit parts. */
  glow: boolean;
  /** The toon outline the game draws around NPR materials. */
  outlines: boolean;
  light: LightPreset;
  /** 0.5 to 1.8: scales every light. */
  exposure: number;
  /** Camera field of view in degrees. */
  fov: number;
};

export const DEFAULT_CFG: ViewerCfg = { wire: false, floor: true, gizmo: true, spin: false, bg: "dark", smaa: true, glow: true, outlines: true, light: "studio", exposure: 1, fov: 35 };

export const BG_COLOR: Record<Bg, number> = { dark: 0x09090b, slate: 0x1b1b1e, light: 0xd9dce2 };

/** Light intensities per preset: [environment, key, rim, fill]. */
export const LIGHTS: Record<LightPreset, [number, number, number, number]> = {
  studio: [1, 2.6, 1.2, 2.4],
  soft: [1.5, 1.4, 0.6, 3.2],
  dramatic: [0.3, 3.6, 2.4, 0.8],
};

const KEY = "kea.viewer";

export function loadCfg(): ViewerCfg {
  try {
    const raw: unknown = JSON.parse(localStorage.getItem(KEY) ?? "{}");
    const o = (raw && typeof raw === "object" ? raw : {}) as Record<string, unknown>;
    const pick = <T extends string>(v: unknown, all: readonly T[], d: T): T => (all.includes(v as T) ? (v as T) : d);
    const num = (v: unknown, lo: number, hi: number, d: number) => (typeof v === "number" && v >= lo && v <= hi ? v : d);
    const bool = (v: unknown, d: boolean) => (typeof v === "boolean" ? v : d);
    return {
      wire: bool(o.wire, DEFAULT_CFG.wire),
      floor: bool(o.floor, DEFAULT_CFG.floor),
      gizmo: bool(o.gizmo, DEFAULT_CFG.gizmo),
      spin: bool(o.spin, DEFAULT_CFG.spin),
      bg: pick(o.bg, ["dark", "slate", "light"], DEFAULT_CFG.bg),
      smaa: bool(o.smaa, DEFAULT_CFG.smaa),
      glow: bool(o.glow, DEFAULT_CFG.glow),
      outlines: bool(o.outlines, DEFAULT_CFG.outlines),
      light: pick(o.light, ["studio", "soft", "dramatic"], DEFAULT_CFG.light),
      exposure: num(o.exposure, 0.5, 1.8, DEFAULT_CFG.exposure),
      fov: num(o.fov, 20, 70, DEFAULT_CFG.fov),
    };
  } catch {
    return DEFAULT_CFG;
  }
}

export function storeCfg(c: ViewerCfg) {
  try {
    localStorage.setItem(KEY, JSON.stringify(c));
  } catch {
    /* storage can be unavailable */
  }
}

/** Saves a capture: through the save dialog in the app, as a normal download in a plain browser. Returns false when cancelled. */
export async function saveCapture(blob: Blob, name: string, ext: "png" | "webm", title: string): Promise<boolean> {
  if (!isTauri) {
    const a = document.createElement("a");
    a.href = URL.createObjectURL(blob);
    a.download = `${name}.${ext}`;
    a.click();
    setTimeout(() => URL.revokeObjectURL(a.href), 2000);
    return true;
  }
  const { save } = await import("@tauri-apps/plugin-dialog");
  const path = await save({ title, defaultPath: `${name}.${ext}`, filters: [{ name: ext.toUpperCase(), extensions: [ext] }] });
  if (!path) return false;
  const { invoke } = await import("@tauri-apps/api/core");
  await invoke("save_capture", new Uint8Array(await blob.arrayBuffer()), { headers: { path: encodeURIComponent(path) } });
  return true;
}

