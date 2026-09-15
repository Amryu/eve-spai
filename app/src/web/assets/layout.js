// Pane arrangement: tabs, columns or grid, ordered and persisted per device rather than in
// Settings, since a phone and a desktop browser want different layouts.

import { afterRender, available, ico, PANES, render } from "./app.js";

const KEY = "spai_layout";

// Jabber starts off: five panes do not fit in four cells, and a pane the budget silently drops looks
// broken.
const DEFAULTS = { mode: "auto", order: [...PANES], active: 0, off: ["jabber"], span: {} };

/// The grid is 2x2, so four cells is the whole budget. Columns share it, since more than four are
/// too narrow to read.
const CELLS = 4;

/// What a pane costs in cells. Only the grid has cells to spend.
function cost(p, mode) {
  return mode === "grid" && layout.span[p] ? 2 : 1;
}

const layout = load();

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
    // Private browsing. The layout then lasts one session.
  }
}

/// Panes the user has left switched on, in their chosen order.
function shown() {
  const have = available();
  // Rescue is always on where it exists: the build feature and the setting already opt in twice.
  return layout.order.filter(
    (p) => have.includes(p) && (p === "rescue" || !layout.off.includes(p))
  );
}

function isOn(pane) {
  return !layout.off.includes(pane);
}

/// Switch a pane off and drop its span, so it comes back as one cell instead of pushing another pane
/// out.
function hide(pane) {
  delete layout.span[pane];
  setOn(pane, false);
}

function setOn(pane, on) {
  const off = new Set(layout.off);
  if (on) off.delete(pane);
  else off.add(pane);
  if (off.size >= PANES.length) return;
  layout.off = [...off];
  fit();
  save();
  apply();
}

/// Which panes get drawn. Tabs shows every pane switched on. Grid and columns take the ones that fit,
/// in order, without switching the rest off.
function fitting(mode = effectiveMode()) {
  const on = shown();
  if (mode === "tabs") return on;
  const out = [];
  let spent = 0;
  for (const p of on) {
    const c = cost(p, mode);
    if (spent + c > CELLS) continue;
    spent += c;
    out.push(p);
  }
  return out;
}

/// Keep `active` inside the list.
function fit() {
  layout.active = Math.min(layout.active, Math.max(0, shown().length - 1));
}

/// Toggle a pane's span. Wide and tall are separate, exclusive toggles, so reaching one never passes
/// through the other and reflows the grid.
function setSpan(pane, kind) {
  // Any deliberate choice retires the seeded default for good.
  layout.autoSpan = true;
  if (layout.span[pane] === kind) delete layout.span[pane];
  else layout.span[pane] = kind;
  fit();
  save();
  apply();
}

/// Swap two panes' places. The span stays with the place, so the grid keeps its shape after a drop.
function swap(a, b) {
  const i = layout.order.indexOf(a);
  const j = layout.order.indexOf(b);
  if (i < 0 || j < 0 || i === j) return;
  layout.order[i] = b;
  layout.order[j] = a;
  const sa = layout.span[a];
  const sb = layout.span[b];
  if (sb) layout.span[a] = sb;
  else delete layout.span[a];
  if (sa) layout.span[b] = sa;
  else delete layout.span[b];
  save();
  apply();
}

/// `auto` is a media query rather than a stored choice, so rotating a tablet adapts.
function effectiveMode() {
  if (layout.mode !== "auto") return layout.mode;
  return window.matchMedia("(min-width: 900px)").matches ? "columns" : "tabs";
}

function apply() {
  const main = document.getElementById("panes");
  if (!main) return;
  const mode = effectiveMode();
  main.dataset.mode = mode;

  // Order is applied with `order` rather than by moving nodes, so a re-render never has to rebuild
  // the DOM and an input inside a pane keeps its focus.
  fit();
  const on = shown();
  const drawn = fitting(mode);
  // An odd count leaves a spare cell, so the map starts wide. Seeded once as a stored value, since a
  // rule computed every time would put the span back right after the user cleared it.
  if (
    !layout.autoSpan &&
    mode === "grid" &&
    drawn.length % 2 === 1 &&
    drawn.includes("map") &&
    !Object.keys(layout.span).length
  ) {
    layout.span.map = "wide";
    layout.autoSpan = true;
    save();
  }
  for (const [i, name] of layout.order.entries()) {
    const el = main.querySelector(`[data-pane="${name}"]`);
    if (!el) continue;
    el.style.order = String(i);
    el.hidden = !drawn.includes(name);
    const s = layout.span[name] ?? "";
    if (s) el.dataset.span = s;
    else delete el.dataset.span;
  }
  // Columns divide by what is actually showing, so switching one off widens the rest instead of
  // leaving a gap.
  main.style.setProperty("--cols", String(Math.max(1, Math.min(drawn.length, CELLS))));
  // Rows are the cells actually spent, so two panes fill the height instead of sitting in the top
  // half of an empty 2x2.
  const spent = drawn.reduce((n, p) => n + cost(p, mode), 0);
  const tall = drawn.some((p) => layout.span[p] === "tall");
  main.style.setProperty("--rows", String(Math.min(2, Math.max(tall ? 2 : 1, Math.ceil(spent / 2)))));
  paneChrome(mode);
  // Not while a swipe or smooth scroll settles: jumping the strip mid-animation cuts the slide off.
  if (mode === "tabs" && !settling) scrollToActive(false);
  paintTabs();
  // Panes that act only while visible need this, since `register` fires on a snapshot, not a tab tap.
  window.dispatchEvent(new Event("spai:panes"));
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

const SPANS = [
  ["wide", "arrows-out-line-horizontal", "Two columns wide"],
  ["tall", "arrows-out-line-vertical", "Two rows tall"],
];

/// Per-pane controls, injected so pane renderers need not know about the grid. `render()` replaces
/// pane HTML wholesale, so this reruns from `afterRender` and must be idempotent.
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
    const now = layout.span[name] ?? "";
    const want =
      (grid
        ? SPANS.map(
            ([kind, glyph, tip]) =>
              `<button class="pbtn pspan${now === kind ? " on" : ""}" data-span-btn="${name}" data-kind="${kind}" title="${now === kind ? "Back to one cell" : tip}">${ico(glyph)}</button>`
          ).join("")
        : "") +
      `<button class="pbtn pdrag" data-drag="${name}" title="Drag to rearrange">${ico("dots-six-vertical")}</button>` +
      `<button class="pbtn pclose" data-close="${name}" title="Hide this view">${ico("x")}</button>`;
    // Rewriting identical HTML would destroy the node a drag is holding on to.
    if (bar.innerHTML !== want) bar.innerHTML = want;
  }
}

/// Drag a pane's handle onto another pane to swap them. Pointer events, since HTML5 drag and drop
/// does not work on touch.
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
    if (b) return setSpan(b.dataset.spanBtn, b.dataset.kind);
    const x = e.target.closest("[data-close]");
    if (x) return hide(x.dataset.close);
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

/// Swipe is the browser's scroll-snap, since hand-rolled touch handlers get momentum and interrupted
/// gestures wrong and fight iOS. This only reads back which pane the strip landed on.
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

function setMode(mode) {
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
  b.textContent = "\u2261"; // the icon font may not have loaded this early
  b.title = "Layout";
  b.addEventListener("click", (e) => {
    e.stopPropagation();
    document.body.classList.toggle("sheet");
  });
  document.addEventListener("click", (e) => {
    if (!e.target.closest("#menu")) document.body.classList.remove("sheet");
  });
}

function wire() {
  // `#pane/<name>` selects a pane on load, for links and load-time screenshots.
  const deep = /^#pane\/(\w+)$/.exec(location.hash);
  if (deep) {
    const i = shown().indexOf(deep[1]);
    if (i >= 0) {
      layout.active = i;
      save();
    }
  }
  // `?mode=grid` picks a layout on load, for links and load-time screenshots.
  const m = new URLSearchParams(location.search).get("mode");
  if (["auto", "tabs", "columns", "grid"].includes(m)) {
    layout.mode = m;
    save();
  }
  // `?panes=intel,jabber` picks what is showing, for links and load-time screenshots.
  const want = new URLSearchParams(location.search).get("panes");
  if (want) {
    const on = want.split(",").filter((p) => PANES.includes(p));
    if (on.length) {
      layout.off = PANES.filter((p) => !on.includes(p));
      fit();
      save();
    }
  }
  // `#layout` opens the menu on load, since the screenshot harness cannot click.
  if (location.hash === "#layout") document.body.classList.add("sheet");
  watchScroll();
  wireTabs();
  wirePanes();
  wireSheet();
  // The map rebuilds its pane outside `render` when geometry lands, dropping the heading controls.
  window.addEventListener("spai:map", () => paneChrome(effectiveMode()));
  window.matchMedia("(min-width: 900px)").addEventListener("change", apply);
  apply();
}

afterRender.push(apply);

wire();
