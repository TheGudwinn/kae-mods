import React from "react";
import ReactDOM from "react-dom/client";
import "./i18n";
import "./styles.css";
import "./themes.css";
import "./design.css";
import App from "./App";
import { isTauri } from "./lib/api";
import { applyAppearance } from "./lib/appearance";
import { initSliders } from "./lib/sliders";

applyAppearance();
initSliders();

// In the native app the window is transparent and the rounded shell is the window.
if (isTauri) {
  document.documentElement.classList.add("native");
  // The browser context menu (reload, inspect, ...) has no place in an app window.
  window.addEventListener("contextmenu", (e) => e.preventDefault());
}

// Pause the moving background light while the window is not in use.
const idle = () => {
  document.documentElement.dataset.idle = document.hidden || !document.hasFocus() ? "1" : "0";
};
window.addEventListener("blur", idle);
window.addEventListener("focus", idle);
document.addEventListener("visibilitychange", idle);

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
