import { useEffect, useState } from "react";
import { call, type ModDetails } from "./api";
import { loadCachedMods, saveCachedMods, toCached, type CachedMod } from "./modcache";

/** Catalog info (picture, category) for the mods of a build, looked up once and kept between runs. */
export function useModInfo(ids: string[]): Record<string, CachedMod> {
  const [info, setInfo] = useState<Record<string, CachedMod>>(() => loadCachedMods());
  const key = ids.join(",");
  useEffect(() => {
    let stale = false;
    const missing = ids.filter((id) => id && !info[id]);
    if (missing.length === 0) return;
    (async () => {
      const found: Record<string, CachedMod> = {};
      // a few at a time: the catalog is a public site
      for (let i = 0; i < missing.length; i += 3) {
        await Promise.all(
          missing.slice(i, i + 3).map((id) =>
            call<ModDetails>("mod_details", { id }).then(
              (d) => {
                found[id] = toCached(d.summary);
              },
              () => {},
            ),
          ),
        );
        if (stale) return;
        if (Object.keys(found).length) {
          saveCachedMods(found);
          setInfo((cur) => ({ ...cur, ...found }));
        }
      }
    })();
    return () => {
      stale = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- `info` is only read to skip what is known
  }, [key]);
  return info;
}
