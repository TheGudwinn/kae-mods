import type { TFunction } from "i18next";

export function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  const units = ["KB", "MB", "GB"];
  let v = n / 1024;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v >= 100 ? v.toFixed(0) : v.toFixed(1)} ${units[i]}`;
}

export function compact(n: number): string {
  return new Intl.NumberFormat(undefined, { notation: "compact", maximumFractionDigits: 1 }).format(n);
}

export function fileExt(name: string): string {
  const i = name.lastIndexOf(".");
  return i >= 0 ? name.slice(i).toLowerCase() : "";
}

export function isInstallable(name: string): boolean {
  return [".zip", ".7z", ".rar", ".vpk"].includes(fileExt(name));
}

/** Turn a backend error code or message into a sentence that says what to do. */
export function humanizeError(t: TFunction, e: unknown): string {
  const raw = e instanceof Error ? e.message : String(e);
  const [code, ...rest] = raw.split(":");
  const arg = rest.join(":");
  switch (code) {
    case "game_not_found":
    case "no_files":
    case "already_installed":
    case "no_free_slots":
      return t(`errors.${code}`);
    case "unsupported_format":
      return t("errors.unsupported_format", { name: arg });
    case "duplicate":
      return t("errors.duplicate", { name: arg });
  }
  switch (code) {
    case "name_taken":
    case "empty_name":
    case "invalid_name":
    case "profile_not_found":
    case "file_too_large":
    case "not_a_profile":
    case "unsupported_version":
      return t(`errors.${code}`);
    case "invalid_entry":
    case "duplicate_entry":
    case "too_many_mods":
      return t("errors.bad_profile");
  }
  if (code === "peek_too_big") return t("errors.peek_too_big");
  if (code === "already_queued") return t("errors.already_queued");
  if (["no_update", "update_no_source", "update_slot_conflict", "mod_not_found"].includes(code)) return t(`errors.${code}`);
  if (code === "updates_not_configured") return t("errors.updates_not_configured");
  if (code === "update_install_failed") return t("errors.update_install_failed", { message: arg });
  if (/signature/i.test(raw)) return t("errors.update_signature");
  if (code === "order_mismatch") return t("errors.order_mismatch");
  if (/os error (5|32)\b|access is denied|used by another process|permission denied/i.test(raw)) return t("errors.file_busy");
  if (/external chunk files/i.test(raw)) return t("errors.chunks");
  if (/password-protected/i.test(raw)) return t("errors.encrypted");
  if (/forbidden file type/i.test(raw)) return t("errors.forbidden");
  if (/unsafe path|escapes the allowed/i.test(raw)) return t("errors.unsafe");
  if (/unrecognized file format|cannot read archive/i.test(raw)) return t("errors.badArchive");
  if (/mirror list is empty/i.test(raw)) return t("errors.mirrors_empty");
  if (/too many mirrors/i.test(raw)) return t("errors.mirrors_many");
  const bad = /invalid mirror address: (.*)/i.exec(raw);
  if (bad) return t("errors.mirror_bad", { value: bad[1] });
  if (/proxy host and port are required|host and port are required/i.test(raw)) return t("errors.proxy_incomplete");
  if (/proxy host must be/i.test(raw)) return t("errors.proxy_host");
  if (/invalid proxy configuration/i.test(raw)) return t("errors.proxy_bad");
  const missing = /path does not exist: (.*)/i.exec(raw);
  if (missing) return t("errors.path_missing", { path: missing[1] });
  const nd = /not a Deadlock folder[^:]*: (.*)/i.exec(raw);
  if (nd) return t("errors.not_deadlock", { path: nd[1] });
  if (/mirror|request error|http status|timed out|connect/i.test(raw)) return t("errors.network");
  return t("errors.generic", { message: raw });
}

/** Strip markup from source-provided HTML: only text is ever rendered. */
export function htmlToText(html: string): string {
  const doc = new DOMParser().parseFromString(html, "text/html");
  return (doc.body.textContent ?? "").replace(/\s+/g, " ").trim();
}

/** CSS class of a catalog category, so every category has its own badge colour (design.css: .tag-c-*). */
export const catClass = (c: string | null | undefined) => `tag-c-${(c ?? "other").toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "")}`;
