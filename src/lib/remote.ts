import { useEffect, useState } from "react";
import { call } from "./api";

export type NewsItem = { id: string; date: string; title: string; text: string; tag?: string; url?: string };
export type Collection = { id: string; name: string; author: string; description: string; code: string };

const text = (v: unknown, max: number) => (typeof v === "string" ? v.trim().slice(0, max) : "");
const https = (v: unknown) => (typeof v === "string" && /^https:\/\/[^\s]{3,480}$/i.test(v.trim()) ? v.trim() : undefined);
const list = (raw: string): unknown[] => {
  try {
    const v: unknown = JSON.parse(raw);
    return Array.isArray(v) ? v.slice(0, 50) : [];
  } catch {
    return [];
  }
};

/** The files come from the internet: every field is checked and cut, and only https links are kept. */
export function parseNews(raw: string): NewsItem[] {
  return list(raw).flatMap((x) => {
    const o = (x ?? {}) as Record<string, unknown>;
    const title = text(o.title, 140);
    const id = text(o.id, 64);
    if (!title || !id) return [];
    return [{ id, date: text(o.date, 10), title, text: text(o.text, 400), tag: text(o.tag, 20) || undefined, url: https(o.url) }];
  });
}

export function parseCollections(raw: string): Collection[] {
  return list(raw).flatMap((x) => {
    const o = (x ?? {}) as Record<string, unknown>;
    const name = text(o.name, 80);
    const id = text(o.id, 64);
    const code = text(o.code, 400_000);
    if (!name || !id || !/^ka[ea]1:/.test(code)) return [];
    return [{ id, name, author: text(o.author, 40), description: text(o.description, 300), code }];
  });
}

/** How many mods a profile code holds, read without importing it. */
export function codeModCount(code: string): number | null {
  try {
    const b = code.slice(5).replace(/-/g, "+").replace(/_/g, "/");
    const json = JSON.parse(decodeURIComponent(escape(atob(b)))) as { mods?: unknown[] };
    return Array.isArray(json.mods) ? json.mods.length : null;
  } catch {
    return null;
  }
}

/** Loads one of the repository's content files (news, collections); `items` is null until it arrives. */
export function useRemote<T>(kind: "news" | "collections", parse: (raw: string) => T[]) {
  const [items, setItems] = useState<T[] | null>(null);
  useEffect(() => {
    let stale = false;
    call<string>("fetch_content", { kind }).then(
      (raw) => !stale && setItems(parse(raw)),
      () => !stale && setItems([]),
    );
    return () => {
      stale = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- parse is a module-level function
  }, [kind]);
  return items;
}
