// Pane arrangement: tabs, columns or grid, ordered and persisted per device.
//
// Per device, not in Settings: a phone and a desktop browser want different layouts, and the app
// pushing one to both would be the app fighting the user.

import { afterRender, PANES } from "./app.js";

const KEY = "spai_layout";

const DEFAULTS = { mode: "auto", order: [...PANES], active: 0, off: [] };

export const layout = load();

function load() {
  try {
    const saved = JSON.parse(localStorage.getItem(KEY) ?? "null");
    if (!saved) return { ...DEFAULTS };
    // A pane added or removed by an upgrade must not strand the saved order.
    const order = [...new Set([...(saved.order ?? []).filter((p) => PANES.includes(p)), ...PANES])];
    const off = (saved.off ?? []).filter((p) => PANES.includes(p));
    // Never hide everything: a page with no panes has no way back except clearing storage.
    return { ...DEFAULTS, ...saved, order, off: off.length < PANES.length ? off : [] };
  } catch {
    return { ...DEFAULTS };
  }
}

function save() {
  try {
    localStorage.setItem(KEY, JSON.stringify(layout));
  } catch {
    // Private browsing. The layout then lasts one session, which is better than refusing to lay out.
  }
}

/// `auto` is a media query rather than a stored choice, so rotating a tablet does the right thing
/// without the user having picked anything.
/// Panes the user has left switched on, in their chosen order.
export function shown() {
  return layout.order.filter((p) => !layout.off.includes(p));
}

export function isOn(pane) {
  return !layout.off.includes(pane);
}

export function setOn(pane, on) {
  const off = new Set(layout.off);
  if (on) off.delete(pane);
  else off.add(pane);
  if (off.size >= PANES.length) return; // keep at least one
  layout.off = [...off];
  layout.active = Math.min(layout.active, Math.max(0, shown().length - 1));
  save();
  apply();
}

export function effectiveMode() {
  if (layout.mode !== "auto") return layout.mode;
  return window.matchMedia("(min-width: 900px)").matches ? "columns" : "tabs";
}

export function apply() {
  const main = document.getElementById("panes");
  if (!main) return;
  const mode = effectiveMode();
  main.dataset.mode = mode;

  // Order is applied with `order` rather than by moving nodes, so a re-render never has to rebuild
  // the DOM and an input inside a pane keeps its focus.
  const on = shown();
  for (const [i, name] of layout.order.entries()) {
    const el = main.querySelector(`[data-pane="${name}"]`);
    if (!el) continue;
    el.style.order = String(i);
    el.hidden = !on.includes(name);
  }
  // Columns divide by what is actually showing, so switching one off widens the rest instead of
  // leaving a gap.
  main.style.setProperty("--cols", String(Math.max(1, Math.min(on.length, 4))));
  if (mode === "tabs") scrollToActive(false);
  paintTabs();
}

function paintTabs() {
  const mode = effectiveMode();
  document
    .querySelectorAll(".modebar [data-mode]")
    .forEach((b) => b.classList.toggle("on", b.dataset.mode === layout.mode));
  document
    .querySelectorAll(".modebar [data-toggle]")
    .forEach((b) => b.classList.toggle("on", isOn(b.dataset.toggle)));
  const on = shown();
  document.querySelectorAll("#tabs .tab").forEach((b) => {
    const name = b.dataset.tab;
    b.style.order = String(layout.order.indexOf(name));
    b.classList.toggle("off", !on.includes(name));
    b.setAttribute("aria-selected", String(mode === "tabs" && on[layout.active] === name));
  });
}

function scrollToActive(smooth) {
  const main = document.getElementById("panes");
  const name = shown()[layout.active];
  const el = main?.querySelector(`[data-pane="${name}"]`);
  el?.scrollIntoView({ behavior: smooth ? "smooth" : "auto", inline: "start", block: "nearest" });
}

/// Swipe is the browser's scroll-snap, not a touch handler.
///
/// Hand-rolled `touchstart`/`touchmove`/`touchend` gets momentum, over-scroll and interrupted
/// gestures wrong, and fights iOS. The strip snaps; this only reads back which pane it landed on.
function watchScroll() {
  const main = document.getElementById("panes");
  if (!main) return;
  let t = null;
  main.addEventListener(
    "scroll",
    () => {
      if (effectiveMode() !== "tabs") return;
      clearTimeout(t);
      t = setTimeout(() => {
        const i = Math.round(main.scrollLeft / main.clientWidth);
        if (i !== layout.active && i >= 0 && i < shown().length) {
          layout.active = i;
          save();
          paintTabs();
        }
      }, 80);
    },
    { passive: true }
  );
}

export function setMode(mode) {
  layout.mode = mode;
  save();
  apply();
}

function move(name, to) {
  const from = layout.order.indexOf(name);
  if (from < 0 || to < 0 || to >= layout.order.length) return;
  layout.order.splice(to, 0, layout.order.splice(from, 1)[0]);
  save();
  apply();
}

/// Drag on a pointer device, long-press then drag on touch. Both end in the same `move`.
function wireTabs() {
  const bar = document.getElementById("tabs");
  if (!bar) return;
  let dragging = null;
  let held = null;

  bar.addEventListener("pointerdown", (e) => {
    const b = e.target.closest(".tab");
    if (!b) return;
    if (e.pointerType === "touch") {
      held = setTimeout(() => {
        dragging = b.dataset.tab;
        b.classList.add("dragging");
      }, 350);
    } else {
      dragging = b.dataset.tab;
    }
  });
  bar.addEventListener("pointermove", (e) => {
    if (!dragging) return;
    const over = document.elementFromPoint(e.clientX, e.clientY)?.closest(".tab");
    if (over && over.dataset.tab !== dragging) {
      move(dragging, layout.order.indexOf(over.dataset.tab));
    }
  });
  const end = () => {
    clearTimeout(held);
    document.querySelectorAll(".tab.dragging").forEach((b) => b.classList.remove("dragging"));
    // A pointer-device drag that never moved is just a click, which the click handler below serves.
    dragging = null;
  };
  bar.addEventListener("pointerup", end);
  bar.addEventListener("pointercancel", end);

  bar.addEventListener("click", (e) => {
    const m = e.target.closest("[data-mode]");
    if (m) {
      setMode(m.dataset.mode);
      return;
    }
    const t = e.target.closest("[data-toggle]");
    if (t) {
      setOn(t.dataset.toggle, !isOn(t.dataset.toggle));
      return;
    }
    const b = e.target.closest(".tab");
    if (!b) return;
    // A tab that is switched off comes back when tapped, rather than being a dead chip.
    if (!isOn(b.dataset.tab)) {
      setOn(b.dataset.tab, true);
      return;
    }
    const i = shown().indexOf(b.dataset.tab);
    if (i < 0) return;
    if (effectiveMode() === "tabs") {
      layout.active = i;
      save();
      scrollToActive(true);
      paintTabs();
    } else {
      document
        .querySelector(`#panes [data-pane="${b.dataset.tab}"]`)
        ?.scrollIntoView({ behavior: "smooth", block: "start" });
    }
  });
}

function wireSheet() {
  const b = document.getElementById("sheet");
  if (!b) return;
  b.textContent = "\u2261"; // three bars; the icon font may not have loaded this early
  b.title = "Layout";
  b.addEventListener("click", (e) => {
    e.stopPropagation();
    document.body.classList.toggle("sheet");
  });
  // Choosing a mode ends that errand; toggling panes usually does not, so the menu stays open for
  // those.
  document.getElementById("tabs")?.addEventListener("click", (e) => {
    if (e.target.closest("[data-mode]")) document.body.classList.remove("sheet");
  });
  document.addEventListener("click", (e) => {
    if (!e.target.closest(".modebar") && !e.target.closest("#sheet")) {
      document.body.classList.remove("sheet");
    }
  });
}

export function wire() {
  // `#layout` opens the menu on load. Same reason the dialogs take deep links: the harness cannot
  // click, so without this the menu is the one control no screenshot can show.
  if (location.hash === "#layout") document.body.classList.add("sheet");
  watchScroll();
  wireTabs();
  wireSheet();
  window.matchMedia("(min-width: 900px)").addEventListener("change", apply);
  apply();
}

afterRender.push(apply);

wire();
