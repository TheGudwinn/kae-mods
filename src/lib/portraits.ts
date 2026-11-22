import { useEffect, useState } from "react";
import { call } from "./api";

/** Lookup key of a hero name: lower case, no "the", no spaces or punctuation. */
export const heroKey = (name: string) => name.toLowerCase().replace(/^the /, "").replace(/[^\p{L}\p{N}]/gu, "");

let cache: Record<string, string> | null = null;
let pending: Promise<Record<string, string>> | null = null;

function load(): Promise<Record<string, string>> {
  if (cache) return Promise.resolve(cache);
  pending ??= call<Record<string, string>>("hero_portraits").then(
    (r) => (cache = r ?? {}),
    () => ({}),
  );
  return pending;
}

/** Hero portraits shared by the whole app (fetched once). Returns a function: hero name -> picture url, or undefined. */
export function useHeroPortraits(): (name: string | null | undefined) => string | undefined {
  const [map, setMap] = useState<Record<string, string>>(cache ?? {});
  useEffect(() => {
    let stale = false;
    void load().then((r) => !stale && setMap(r));
    return () => {
      stale = true;
    };
  }, []);
  return (name) => (name ? map[heroKey(name)] : undefined);
}
