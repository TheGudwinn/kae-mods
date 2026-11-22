import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";

/** Minimise / full screen / close for the frameless window. Rendered only inside the native app. */
export default function WindowControls({ onMaximizedChange }: { onMaximizedChange: (m: boolean) => void }) {
  const { t } = useTranslation();
  const [full, setFull] = useState(false);

  const toggleFull = useCallback(async () => {
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    const win = getCurrentWindow();
    const next = !(await win.isFullscreen());
    await win.setFullscreen(next);
    setFull(next);
  }, []);

  // F11 toggles, Esc leaves
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "F11") {
        e.preventDefault();
        void toggleFull();
      } else if (e.key === "Escape" && full) {
        void toggleFull();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [toggleFull, full]);

  useEffect(() => {
    let off: (() => void) | undefined;
    let alive = true;
    void (async () => {
      const { getCurrentWindow } = await import("@tauri-apps/api/window");
      const win = getCurrentWindow();
      let was = false;
      let timer: number | undefined;
      let busy = false;
      const run = async () => {
        if (busy) return;
        busy = true;
        try {
          const f = await win.isFullscreen();
          const m = await win.isMaximized();
          // Maximise (double-click on the title area, Win+Up) is undone at once; full screen is the supported way.
          if (m && !f) await win.unmaximize();
          if (alive) {
            setFull(f);
            onMaximizedChange(f);
          }
          // Leaving full screen can leave the web view at the old size: nudge the window so it lays out again.
          if (was && !f && !m) {
            const { PhysicalSize } = await import("@tauri-apps/api/dpi");
            const s = await win.innerSize();
            await win.setSize(new PhysicalSize(s.width + 1, s.height));
            await win.setSize(new PhysicalSize(s.width, s.height));
            window.dispatchEvent(new Event("resize"));
          }
          was = f;
        } finally {
          busy = false;
        }
      };
      // resize events come in bursts: handle only the settled state
      const sync = () => {
        window.clearTimeout(timer);
        timer = window.setTimeout(() => void run(), 180);
      };
      await run();
      off = await win.onResized(sync);
    })();
    return () => {
      alive = false;
      off?.();
    };
  }, [onMaximizedChange]);

  const act = async (what: "minimize" | "close") => {
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    await getCurrentWindow()[what]();
  };

  return (
    <div className="winctl">
      <button className="winbtn" aria-label={t("window.minimize")} title={t("window.minimize")} onClick={() => void act("minimize")}>
        <svg width="12" height="12" viewBox="0 0 12 12" aria-hidden="true">
          <path d="M2 6h8" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" />
        </svg>
      </button>
      <button className="winbtn" aria-label={t(full ? "window.exitFull" : "window.full")} title={`${t(full ? "window.exitFull" : "window.full")}  ·  F11`} aria-pressed={full} onClick={() => void toggleFull()}>
        <svg width="12" height="12" viewBox="0 0 12 12" aria-hidden="true">
          {full ? <path d="M4.5 1.5v3h-3M7.5 1.5v3h3M4.5 10.5v-3h-3M7.5 10.5v-3h3" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" /> : <path d="M1.5 4.5v-3h3M10.5 4.5v-3h-3M1.5 7.5v3h3M10.5 7.5v3h-3" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" />}
        </svg>
      </button>
      <button className="winbtn winbtn-close" aria-label={t("window.close")} title={t("window.close")} onClick={() => void act("close")}>
        <svg width="12" height="12" viewBox="0 0 12 12" aria-hidden="true">
          <path d="M2.5 2.5l7 7M9.5 2.5l-7 7" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" />
        </svg>
      </button>
    </div>
  );
}
