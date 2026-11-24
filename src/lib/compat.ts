import type { TFunction } from "i18next";
import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { call } from "./api";
import { humanizeError } from "./format";

export type CompatFinding = { kind: string; severity: string; model: string; mods: string[] };
export type FixedMod = { file: string; models: string[] };
export type CompatReport = {
  findings: CompatFinding[];
  /** Mods whose hero models were repaired and are covered by the compatibility package. */
  fixed: FixedMod[];
  details: { mods_scanned: number; mods_unreadable: string[]; hero_models: number; models_looked_into: number; skipped: [string, string, string][] };
};
export type RepairResult = { fixed: string[]; skipped: [string, string][]; /** Nothing was repaired, the package was only switched back on. */ reconnected?: boolean };

/** What the camera check says about one mod: it still needs the repair, or it has been repaired. */
export type CamState = { state: "needs" | "fixed"; models: string[] };

export type Compat = {
  report: CompatReport | null;
  scanning: boolean;
  repairing: boolean;
  error: string | null;
  done: RepairResult | null;
  scan: () => Promise<void>;
  repair: () => Promise<void>;
};

export const shortName = (n: string) => n.replace(/\.disabled$/i, "");

/** Findings the repair can act on. */
export const isFixable = (f: CompatFinding) => f.kind.startsWith("camera_") || f.kind === "package_inactive" || f.kind === "compat_stale";

/** The camera state of a mod made of these files (names as in the addons folder), or null when it is fine. */
export function camStateOf(report: CompatReport | null, files: string[]): CamState | null {
  if (!report) return null;
  const names = new Set(files.map(shortName));
  const models = new Set<string>();
  for (const f of report.findings) if (f.kind.startsWith("camera_") && f.mods.some((m) => names.has(shortName(m)))) models.add(f.model);
  if (models.size > 0) return { state: "needs", models: [...models] };
  for (const x of report.fixed) if (names.has(shortName(x.file))) x.models.forEach((m) => models.add(m));
  return models.size > 0 ? { state: "fixed", models: [...models] } : null;
}

/** What to tell after a repair: the count when models were repaired, that the package is back on, or nothing at all. */
export function repairTitle(t: TFunction, done: RepairResult): string | null {
  if (done.fixed.length > 0) return t("fixes.compat.repaired", { count: done.fixed.length });
  return done.reconnected ? t("fixes.compat.reconnected") : null;
}

/** Scan and repair of hero cameras, shared by the library and the fixes page. */
export function useCompat(): Compat {
  const { t } = useTranslation();
  const [report, setReport] = useState<CompatReport | null>(null);
  const [scanning, setScanning] = useState(false);
  const [repairing, setRepairing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [done, setDone] = useState<RepairResult | null>(null);
  const running = useRef(false);
  const again = useRef(false);
  const cleaned = useRef(false); // an out-of-date package was already cleaned automatically for this state

  const scan = useCallback(async () => {
    if (running.current) {
      again.current = true; // the files changed while scanning: scan once more afterwards
      return;
    }
    running.current = true;
    setScanning(true);
    setError(null);
    try {
      do {
        again.current = false;
        const r = await call<CompatReport>("scan_compat");
        setReport(r);
        // a package made from mods that have changed since overrides the real models and shows up as red shapes
        // in the game: rebuild it at once, without waiting for a click
        // the same when the game was updated or another manager's search path put the package out of play
        if (r.findings.some((f) => f.kind === "compat_stale" || f.kind === "package_inactive")) {
          if (!cleaned.current) {
            cleaned.current = true;
            await call("repair_cameras");
            again.current = true;
          }
        } else cleaned.current = false;
      } while (again.current);
    } catch (e) {
      setError(humanizeError(t, e));
    } finally {
      running.current = false;
      setScanning(false);
    }
  }, [t]);

  const repair = useCallback(async () => {
    setRepairing(true);
    setError(null);
    try {
      const wasInactive = report?.findings.some((f) => f.kind === "package_inactive") ?? false;
      const r = await call<RepairResult>("repair_cameras");
      setDone({ ...r, reconnected: wasInactive && r.fixed.length === 0 });
      await scan();
    } catch (e) {
      setError(humanizeError(t, e));
    } finally {
      setRepairing(false);
    }
  }, [scan, report, t]);

  // The game updates while the launcher is open or closed: scan at start and when the window is used again
  // (at most every ten minutes), so a package an update put out of play is rebuilt without a click.
  useEffect(() => {
    let last = Date.now();
    void scan();
    const onFocus = () => {
      if (Date.now() - last < 10 * 60 * 1000) return;
      last = Date.now();
      void scan();
    };
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, [scan]);

  return { report, scanning, repairing, error, done, scan, repair };
}
