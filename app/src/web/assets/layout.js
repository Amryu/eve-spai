// Pane arrangement: tabs, columns or grid, ordered and persisted per device.
//
// Per device, not in Settings: a phone and a desktop browser want different layouts, and the app
// pushing one to both would be the app fighting the user.

import { afterRender, ico, PANES, render } from "./app.js";

const KEY = "spai_layout";

// Jabber is off to begin with: five panes do not fit in four cells, and a pane switched off by the
// budget without the user asking reads as one that is broken.
const DEFAULTS = { mode: "auto", order: [...PANES], active: 0, off: ["jabber"], span: {} };

/// The grid is 2x2 and nothing else, so four cells is the whole budget. Columns and tabs take the
/// same number of panes, which is what the user asked for and also what four columns can hold
/// without each one being too narrow to read.
const CELLS = 4;

/// What a pane costs in cells. Only the grid has cells to spend.
function cost(p, mode) {
  return mode === "grid" && layout.span[p] ? 2 : 1;
}

export const layout = load();

function load() {
  try {
    const saved = JSON.parse(localStorage.getItem(KEY) ?? "null");
    if (!saved) return { ...DEFAULTS };
    // A pane added or removed by an upgrade must not strand the saved order.
    const order = [...new Set([...(saved.order ?? []).filter((p) => PANES.includes(p)), ...PANES])];
    const off = (saved.off ?? []).filter((p) => PANES.includes(p));
    const span = {};
    for (const [p, v] of Object.entries(saved.span ?? {})) {
      if (PANES.includes(p) && (v === "wide" || v === "tall")) span[p] = v;
    }
    // Never hide everything: a page with no panes has no way back except clearing storage.
    return { ...DEFAULTS, ...saved, order, span, off: off.length < PANES.length ? off : [] };
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
  fit(on ? pane : null);
  save();
  apply();
}

/// Switch off whatever does not fit in the budget, newest demand first.
///
/// "If no space is available, the view will be disabled" is the rule, so something has to give when
/// a fifth pane is switched on or a fourth is widened. `protect` is whatever the user just asked
/// for: they get it, and the cost comes out of the far end of their own order instead.
function fit(protect = null) {
  const mode = effectiveMode();
  let on = shown();
  const total = () => on.reduce((n, p) => n + cost(p, mode), 0);
  while (total() > CELLS && on.length > 1) {
    const victim = [...on].reverse().find((p) => p !== protect) ?? on[on.length - 1];
    layout.off = [...new Set([...layout.off, victim])];
    on = shown();
  }
  layout.active = Math.min(layout.active, Math.max(0, on.length - 1));
}

/// A pane's span: normal, two columns wide, or two rows tall. One button, cycled, because three
/// states do not each deserve a control in a corner this small.
export function cycleSpan(pane) {
  const next = { "": "wide", wide: "tall", tall: "" }[layout.span[pane] ?? ""];
  if (next) layout.span[pane] = next;
  else delete layout.span[pane];
  fit(pane);
  save();
  apply();
}

/// Swap two panes' places in the order, which is what a drop on top of one means.
export function swap(a, b) {
  const i = layout.order.indexOf(a);
  const j = layout.order.indexOf(b);
  if (i < 0 || j < 0 || i === j) return;
  layout.order[i] = b;
  layout.order[j] = a;
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
  fit();
  const on = shown();
  // With nothing set by hand, an odd count still leaves a spare cell and the map is the pane that
  // gains most from the width. Once the user has spanned anything themselves, their choice stands
  // and this stays out of it.
  const auto =
    mode === "grid" && !Object.keys(layout.span).length && on.length % 2 === 1 && on.includes("map")
      ? "map"
      : null;
  for (const [i, name] of layout.order.entries()) {
    const el = main.querySelector(`[data-pane="${name}"]`);
    if (!el) continue;
    el.style.order = String(i);
    el.hidden = !on.includes(name);
    const s = name === auto ? "wide" : layout.span[name] ?? "";
    if (s) el.dataset.span = s;
    else delete el.dataset.span;
  }
  // Columns divide by what is actually showing, so switching one off widens the rest instead of
  // leaving a gap.
  main.style.setProperty("--cols", String(Math.max(1, Math.min(on.length, CELLS))));
  // Rows are the cells actually spent, so two panes fill the height instead of sitting in the top
  // half of an empty 2x2.
  const spent = on.reduce((n, p) => n + cost(p, mode), 0);
  const tall = on.some((p) => layout.span[p] === "tall");
  main.style.setProperty("--rows", String(Math.min(2, Math.max(tall ? 2 : 1, Math.ceil(spent / 2)))));
  paneChrome(mode);
  // Not during a swipe or the smooth scroll that follows a tab tap: jumping the strip to the active
  // pane mid-animation is what made the slide cut off at the end.
  if (mode === "tabs" && !settling) scrollToActive(false);
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

const SPAN_ICON = { "": "arrows-out-line-horizontal", wide: "arrows-out-line-vertical", tall: "arrows-in-line-horizontal" };
const SPAN_TIP = { "": "Widen to two columns", wide: "Make it two rows tall", tall: "Back to one cell" };

/// The per-pane controls, put back after every repaint.
///
/// Injected rather than written by each pane's renderer: a pane draws its own contents and should
/// not have to know it lives in a grid. `render()` replaces the pane's HTML wholesale, so this runs
/// from `afterRender` and is written to be idempotent.
function paneChrome(mode) {
  const grid = mode === "grid";
  for (const name of PANES) {
    const h2 = document.querySelector(`#panes [data-pane="${name}"] > h2`);
    if (!h2) continue;
    let bar = h2.querySelector(".pgrip");
    if (!bar) {
      bar = document.createElement("span");
      bar.className = "pgrip";
      h2.append(bar);
    }
    const want =
      (grid
        ? `<button class="pbtn pspan" data-span-btn="${name}" title="${SPAN_TIP[layout.span[name] ?? ""]}">${ico(SPAN_ICON[layout.span[name] ?? ""])}</button>`
        : "") +
      `<button class="pbtn pdrag" data-drag="${name}" title="Drag to rearrange">${ico("dots-six-vertical")}</button>`;
    // Rewriting identical HTML would destroy the node a drag is holding on to.
    if (bar.innerHTML !== want) bar.innerHTML = want;
  }
}

/// Drag a pane by its handle onto another pane to swap the two.
///
/// Pointer events rather than HTML5 drag and drop: the latter has no touch story at all, and the
/// tab strip next to this already works this way.
function wirePanes() {
  const main = document.getElementById("panes");
  if (!main) return;
  let dragging = null;

  const under = (e) =>
    document.elementFromPoint(e.clientX, e.clientY)?.closest("#panes > .pane") ?? null;

  const lift = () => {
    main.querySelectorAll(".pane.dragging, .pane.dropinto").forEach((p) => {
      p.classList.remove("dragging", "dropinto");
    });
  };

  main.addEventListener("pointerdown", (e) => {
    const g = e.target.closest("[data-drag]");
    if (!g) return;
    // Or the browser starts a text selection and the pane scrolls under the finger instead.
    e.preventDefault();
    dragging = g.dataset.drag;
    main.querySelector(`[data-pane="${dragging}"]`)?.classList.add("dragging");
    g.setPointerCapture(e.pointerId);
  });
  main.addEventListener("pointermove", (e) => {
    if (!dragging) return;
    const over = under(e);
    main.querySelectorAll(".pane.dropinto").forEach((p) => p.classList.remove("dropinto"));
    if (over && over.dataset.pane !== dragging) over.classList.add("dropinto");
  });
  const drop = (e) => {
    if (!dragging) return;
    const over = under(e);
    const from = dragging;
    dragging = null;
    lift();
    if (over && over.dataset.pane !== from) swap(from, over.dataset.pane);
  };
  main.addEventListener("pointerup", drop);
  main.addEventListener("pointercancel", () => {
    dragging = null;
    lift();
  });

  main.addEventListener("click", (e) => {
    const b = e.target.closest("[data-span-btn]");
    if (b) cycleSpan(b.dataset.spanBtn);
  });
}

/// True while the strip is moving under its own momentum or a smooth scroll.
let settling = false;
let settleTimer = null;

function markSettling(ms) {
  settling = true;
  clearTimeout(settleTimer);
  settleTimer = setTimeout(() => {
    settling = false;
  }, ms);
}

function scrollToActive(smooth) {
  const main = document.getElementById("panes");
  const name = shown()[layout.active];
  const el = main?.querySelector(`[data-pane="${name}"]`);
  if (!el || !main) return;
  const want = el.offsetLeft - main.offsetLeft;
  // Already there, to the pixel the browser rounds to. Scrolling again would restart an animation
  // that has just finished.
  if (Math.abs(main.scrollLeft - want) < 2) return;
  if (smooth) markSettling(600);
  main.scrollTo({ left: want, behavior: smooth ? "smooth" : "auto" });
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
      // A scroll in progress, however it started.
      markSettling(220);
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

  document.getElementById("menu")?.addEventListener("click", (e) => {
    const m = e.target.closest("[data-mode]");
    if (m) {
      setMode(m.dataset.mode);
      document.body.classList.remove("sheet");
      return;
    }
    const t = e.target.closest("[data-toggle]");
    if (t) {
      setOn(t.dataset.toggle, !isOn(t.dataset.toggle));
    }
  });

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
  document.addEventListener("click", (e) => {
    if (!e.target.closest("#menu")) document.body.classList.remove("sheet");
  });
}

export function wire() {
  // `#pane/<name>` selects a pane on load, so a link can point at one. Also the only way a
  // load-time screenshot can reach a pane that is not the first.
  const deep = /^#pane\/(\w+)$/.exec(location.hash);
  if (deep) {
    const i = shown().indexOf(deep[1]);
    if (i >= 0) {
      layout.active = i;
      save();
    }
  }
  // `?mode=grid` picks a layout on load. A link can then point at one, and it is the only way a
  // load-time screenshot reaches a mode that is not the stored one.
  const m = new URLSearchParams(location.search).get("mode");
  if (["auto", "tabs", "columns", "grid"].includes(m)) {
    layout.mode = m;
    save();
  }
  // `?panes=intel,jabber` picks what is showing. A link can carry a layout, and it is how a
  // load-time screenshot reaches a pane the budget switched off.
  const want = new URLSearchParams(location.search).get("panes");
  if (want) {
    const on = want.split(",").filter((p) => PANES.includes(p));
    if (on.length) {
      layout.off = PANES.filter((p) => !on.includes(p));
      fit();
      save();
    }
  }
  // `#layout` opens the menu on load. Same reason the dialogs take deep links: the harness cannot
  // click, so without this the menu is the one control no screenshot can show.
  if (location.hash === "#layout") document.body.classList.add("sheet");
  watchScroll();
  wireTabs();
  wirePanes();
  wireSheet();
  window.matchMedia("(min-width: 900px)").addEventListener("change", apply);
  apply();
}

afterRender.push(apply);

wire();
