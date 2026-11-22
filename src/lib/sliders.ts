/*
  Sliding highlight for every segmented control (.seg) and tab bar (.tabs).
  One observer watches the document: whenever the chosen button changes, the white pill is moved under it
  (CSS vars --ind-x/--ind-w/--ind-h/--ind-y read by design.css) and the content that follows the control fades in.
  Works with any markup that marks the active button with aria-checked / aria-selected / .is-on / .tab-on / .seg-on.
*/
const ACTIVE = '[aria-checked="true"],[aria-selected="true"],.tab-on,.seg-on,.is-on';
const CONTROL = ".seg,.tabs,.field,.select,button,label,input,.btn,.chip,.cat";

const last = new WeakMap<HTMLElement, HTMLElement | null>();
let queued = false;

function place(box: HTMLElement) {
  const on = Array.from(box.children).find((c) => c.matches(ACTIVE)) as HTMLElement | undefined;
  if (!on || !box.offsetWidth) {
    box.removeAttribute("data-ind");
    return;
  }
  const first = !box.hasAttribute("data-ind");
  box.style.setProperty("--ind-x", `${on.offsetLeft}px`);
  box.style.setProperty("--ind-y", `${on.offsetTop}px`);
  box.style.setProperty("--ind-w", `${on.offsetWidth}px`);
  box.style.setProperty("--ind-h", `${on.offsetHeight}px`);
  box.setAttribute("data-ind", first ? "init" : "1");
  // the first placement must not slide in from the corner: allow the transition only from the next frame on
  if (first) requestAnimationFrame(() => requestAnimationFrame(() => box.getAttribute("data-ind") === "init" && box.setAttribute("data-ind", "1")));

  const prev = last.get(box);
  last.set(box, on);
  if (prev && prev !== on && !document.documentElement.matches('[data-motion="off"]') && !matchMedia("(prefers-reduced-motion: reduce)").matches) swap(box);
}

/** Fades in what the control switches: the blocks after it, or after its container when there are none. */
function swap(box: HTMLElement) {
  const pick = (from: Element | null) => {
    const out: Element[] = [];
    for (let n = from?.nextElementSibling ?? null; n; n = n.nextElementSibling) if (!n.matches(CONTROL) && !n.hasAttribute("aria-hidden")) out.push(n);
    return out;
  };
  let targets = pick(box);
  if (!targets.length) targets = pick(box.parentElement);
  for (const el of targets.slice(0, 4))
    (el as HTMLElement).animate(
      [
        { opacity: 0, transform: "translateY(10px)", filter: "blur(3px)" },
        { opacity: 1, transform: "none", filter: "none" },
      ],
      { duration: 420, easing: "cubic-bezier(0.2, 0.8, 0.2, 1)" },
    );
}

function run() {
  queued = false;
  document.querySelectorAll<HTMLElement>(".seg,.tabs").forEach(place);
}

export function initSliders() {
  const queue = () => {
    if (queued) return;
    queued = true;
    requestAnimationFrame(run);
  };
  new MutationObserver(queue).observe(document.body, { subtree: true, childList: true, attributes: true, attributeFilter: ["aria-checked", "aria-selected", "class"] });
  window.addEventListener("resize", queue);
  queue();
}
