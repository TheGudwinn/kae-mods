import { useCallback, useEffect, useId, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import Icon from "./Icons";
import { useExit } from "../lib/motion";

export type Option = { value: string; label: string; /** Small round picture shown before the label (a hero portrait). */ icon?: string };

type Props = {
  value: string;
  options: Option[];
  onChange: (value: string) => void;
  /** Accessible name when there is no visible label. */
  label: string;
  disabled?: boolean;
  className?: string;
};

/** A listbox that looks like the rest of the app. The menu is drawn in a portal, so scroll containers and labels around the button cannot clip or re-trigger it. */
export default function Select({ value, options, onChange, label, disabled, className = "" }: Props) {
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(0);
  const [q, setQ] = useState("");
  const [pos, setPos] = useState<{ left: number; top?: number; bottom?: number; width: number; max: number } | null>(null);
  const btn = useRef<HTMLButtonElement>(null);
  const menu = useRef<HTMLUListElement>(null);
  const menuBox = useRef<HTMLDivElement>(null);
  const typed = useRef({ text: "", at: 0 });
  const id = useId();

  const { mounted: menuOn, closing } = useExit(open, 150);
  const current = options.find((o) => o.value === value);
  // long lists (heroes) get a search box; the list narrows as you type
  const searchable = options.length > 8;
  const shown = q.trim() ? options.filter((o) => o.label.toLowerCase().includes(q.trim().toLowerCase())) : options;
  const close = useCallback((refocus = true) => {
    setOpen(false);
    if (refocus) btn.current?.focus();
  }, []);

  const show = () => {
    if (disabled) return;
    setQ("");
    setActive(Math.max(0, options.findIndex((o) => o.value === value)));
    setOpen(true);
  };

  // Place the menu under the button, or above it when the window ends first.
  useLayoutEffect(() => {
    if (!open || !btn.current) return;
    const r = btn.current.getBoundingClientRect();
    const below = window.innerHeight - r.bottom - 12;
    const above = r.top - 12;
    const flip = below < 200 && above > below;
    setPos({
      left: Math.min(r.left, window.innerWidth - Math.max(r.width, 200) - 8),
      width: r.width,
      max: Math.min(320, flip ? above : below),
      ...(flip ? { bottom: window.innerHeight - r.top + 6 } : { top: r.bottom + 6 }),
    });
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const away = (e: Event) => {
      const n = e.target as Node;
      if (!menuBox.current?.contains(n) && !btn.current?.contains(n)) setOpen(false);
    };
    const onScroll = (e: Event) => {
      if (!menuBox.current?.contains(e.target as Node)) setOpen(false);
    };
    // Escape closes the menu even when focus is elsewhere, and does not also close a dialog behind it.
    const esc = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        setOpen(false);
      }
    };
    const shut = () => setOpen(false);
    document.addEventListener("pointerdown", away);
    document.addEventListener("keydown", esc, true);
    window.addEventListener("scroll", onScroll, true);
    window.addEventListener("resize", shut);
    return () => {
      document.removeEventListener("pointerdown", away);
      document.removeEventListener("keydown", esc, true);
      window.removeEventListener("scroll", onScroll, true);
      window.removeEventListener("resize", shut);
    };
  }, [open]);

  useEffect(() => {
    if (open) menu.current?.querySelector<HTMLElement>('[data-active="true"]')?.scrollIntoView({ block: "nearest" });
  }, [open, active]);

  const pick = (i: number) => {
    const o = shown[i];
    if (o) onChange(o.value);
    close();
  };

  const onKey = (e: React.KeyboardEvent) => {
    if (!open) {
      if (["ArrowDown", "ArrowUp", "Enter", " "].includes(e.key)) {
        e.preventDefault();
        show();
      }
      return;
    }
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      close();
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      setActive((a) => Math.min(shown.length - 1, a + 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setActive((a) => Math.max(0, a - 1));
    } else if (e.key === "Home" || e.key === "End") {
      e.preventDefault();
      setActive(e.key === "Home" ? 0 : shown.length - 1);
    } else if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      pick(active);
    } else if (e.key === "Tab") {
      setOpen(false);
    } else if (e.key.length === 1) {
      // type-ahead: letters typed in quick succession jump to the matching option
      const now = Date.now();
      typed.current = { text: (now - typed.current.at > 700 ? "" : typed.current.text) + e.key.toLowerCase(), at: now };
      const hit = options.findIndex((o) => o.label.toLowerCase().startsWith(typed.current.text));
      if (hit >= 0) setActive(hit);
    }
  };

  /** Keys inside the search box: arrows move, Enter picks, Escape closes. */
  const onSearchKey = (e: React.KeyboardEvent) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setActive((a) => Math.min(shown.length - 1, a + 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setActive((a) => Math.max(0, a - 1));
    } else if (e.key === "Enter") {
      e.preventDefault();
      pick(active);
    } else if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      close();
    }
  };

  return (
    <>
      <button
        ref={btn}
        type="button"
        className={`select ${open ? "select-open" : ""} ${className}`}
        role="combobox"
        aria-label={label}
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-controls={open ? id : undefined}
        disabled={disabled}
        onClick={() => (open ? close(false) : show())}
        onKeyDown={onKey}
      >
        <span className="select-value">
          {current?.icon && <img className="sel-ico" src={current.icon} alt="" />}
          {current?.label ?? ""}
        </span>
        <Icon name="down" size={16} />
      </button>
      {menuOn &&
        pos &&
        createPortal(
          <div ref={menuBox} className={`select-menu ${closing ? "is-closing" : ""}`} style={{ left: pos.left, top: pos.top, bottom: pos.bottom, minWidth: pos.width, maxHeight: pos.max }}>
            {searchable && (
              <input
                className="select-search"
                autoFocus
                value={q}
                placeholder={label}
                aria-label={label}
                onChange={(e) => {
                  setQ(e.target.value);
                  setActive(0);
                }}
                onKeyDown={onSearchKey}
              />
            )}
          <ul ref={menu} id={id} role="listbox" aria-label={label} className="select-list">
            {shown.length === 0 && <li className="select-none">—</li>}
            {shown.map((o, i) => (
              <li
                key={o.value}
                role="option"
                aria-selected={o.value === value}
                data-active={i === active}
                className="select-opt"
                onPointerEnter={() => setActive(i)}
                onClick={() => pick(i)}
              >
                <span className="sel-label">
                  {o.icon && <img className="sel-ico" src={o.icon} alt="" loading="lazy" />}
                  {o.label}
                </span>
                {o.value === value && <Icon name="check" size={16} />}
              </li>
            ))}
          </ul>
          </div>,
          document.body,
        )}
    </>
  );
}
