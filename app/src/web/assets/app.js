// The shell. Panes arrive in their own tickets; this establishes the page, the palette and the
// poll, so the server can be verified end to end on its own.

export const state = { snapshot: null, icons: {} };

const $ = (id) => document.getElementById(id);

function setStatus(text, kind) {
  const el = $("status");
  el.textContent = text;
  el.dataset.state = kind;
}

export function ico(name) {
  const glyph = state.icons[name];
  if (!glyph) return "";
  const span = document.createElement("span");
  span.className = "ph";
  span.textContent = glyph;
  return span.outerHTML;
}

export const esc = (s) =>
  String(s ?? "").replace(/[&<>"']/g, (c) =>
    ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]
  );

/// Posts one action to the app. Resolves false without trying when the page may not write back.
export function send(action) {
  if (!state.snapshot?.meta?.allow_writeback) return Promise.resolve(false);
  return fetch("/api/action", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(action),
  })
    .then((r) => r.ok)
    .catch(() => false);
}

/// A dialog over the page, dismissed by its close button or a click on the backdrop. Escape closes it
/// only when `escape` is set: most dialogs leave Escape to the field that has focus. `onClose` runs
/// once, however it closed.
export function modal(cls, html, { escape = false, onClose } = {}) {
  const wrap = document.createElement("div");
  wrap.className = ["jstartdlg", cls].filter(Boolean).join(" ");
  wrap.innerHTML = `<div class="mpanel"><button class="mclose" aria-label="Close">${ico("x")}</button>${html}</div>`;
  document.body.append(wrap);
  let open = true;
  const key = (e) => e.key === "Escape" && close();
  const close = () => {
    if (!open) return;
    open = false;
    wrap.remove();
    document.removeEventListener("keydown", key);
    onClose?.();
  };
  if (escape) document.addEventListener("keydown", key);
  wrap.addEventListener("click", (e) => {
    if (e.target === wrap || e.target.closest(".mclose")) close();
  });
  return { wrap, close };
}

export const PANES = ["intel", "alerts", "pings", "map", "jabber", "rescue"];

const TITLES = { intel: "Intel", alerts: "Alerts", pings: "Fleets", map: "Map", jabber: "Jabber", rescue: "Rescue" };

/// Each pane ticket replaces its own entry. Until then the slot says what it is waiting for, which
/// is more honest than an empty box.
export const renderers = {};

/// A pane module registers itself here.
///
/// Module scripts run in document order, so `main()` has already painted the placeholders by the
/// time a pane module is evaluated. Registering therefore has to repaint, or the pane never appears
/// until the next push.
export function register(name, fn) {
  renderers[name] = fn;
  render();
}

function count(pane) {
  const s = state.snapshot;
  switch (pane) {
    case "intel": return s?.intel?.cards?.length ?? 0;
    case "alerts": return s?.alerts?.msg?.feed?.length ?? 0;
    case "pings": return s?.pings?.pings?.length ?? 0;
    case "map": return s?.map?.intel?.length ?? 0;
    case "jabber": return s?.jabber?.convos?.reduce((n, c) => n + (c.listed ? c.unread : 0), 0) ?? 0;
    case "rescue": return s?.rescue?.pings?.length ?? 0;
  }
  return 0;
}

const MODES = [["auto", "Auto"], ["tabs", "Tabs"], ["columns", "Columns"], ["grid", "Grid"]];

/// Panes this build and this user actually have. Rescue is an opt-in feature in an opt-in build, so
/// on every other install it is not a pane that is switched off, it is a pane that does not exist.
export function available() {
  return PANES.filter((p) => p !== "rescue" || state.snapshot?.meta?.rescue);
}

function renderTabs() {
  // `data-tab`, not `data-pane`: the sections below already own that attribute, and a shared one
  // makes `querySelector("[data-pane=...]")` match whichever comes first in the document, which is
  // the button. Every pane then renders inside the header.
  document.getElementById("tabs").innerHTML = available().map(
    (p) =>
      `<button class="tab" data-tab="${p}"><span class="tname">${TITLES[p]}</span> <b>${count(p)}</b></button>`
  ).join("");

  // Rebuilt in place so the panel keeps its open/closed state across a render.
  const menu = document.querySelector("#menu .modebar");
  if (menu) {
    menu.innerHTML =
      `<div class="mbrow">` +
      MODES.map(([m, label]) => `<button data-mode="${m}">${label}</button>`).join("") +
      `</div>` +
      `<div class="mbrow mbpanes">` +
      available()
        .filter((p) => p !== "rescue")
        .map((p) => `<button class="panetoggle" data-toggle="${p}">${TITLES[p]}</button>`)
        .join("") +
      `</div>`;
  }
}

/// `dirty` is the set of panes worth redrawing, or `null` for all of them.
export function render(dirty = null) {
  renderTabs();
  // The tab bar is rebuilt from scratch here, so whatever the layout put on those buttons has to be
  // put back. `afterRender` is called at the end of this function for that.
  for (const pane of PANES) {
    if (dirty && !dirty.has(pane)) continue;
    const el = document.querySelector(`#panes [data-pane="${pane}"]`);
    if (!el || !renderers[pane]) continue;
    // Re-rendering replaces the scrolled element, which would jump the reader back to the top of
    // whatever they were reading.
    const keep = el.querySelector(".feed");
    const at = keep ? keep.scrollTop : 0;
    renderers[pane](el, state.snapshot);
    if (at !== 0) {
      const now = el.querySelector(".feed");
      if (now) now.scrollTop = at;
    }
  }
  for (const fn of afterRender) fn();
}

/// Things that have to run after every repaint, chiefly the layout reapplying pane order and tab
/// selection to elements this function just replaced.
export const afterRender = [];

let es = null;
let stale = null;

// Panes arrive whole or not at all: the server sends a pane only when it changed, so merging is a
// field-by-field replace rather than a patch.
/// Merge an update and report which panes it actually touched.
///
/// The server only sends a pane when it changed, so this is already the answer; the point is to stop
/// throwing it away. Re-rendering every pane on every push is what made a 5000-node map redraw twice
/// a second and took the whole page down with it.
function merge(update) {
  const s = state.snapshot ?? { seq: 0, gen: update.gen };
  if (update.gen !== s.gen) {
    state.snapshot = update;
    return null; // a new generation invalidates everything
  }
  const dirty = new Set();
  for (const pane of ["intel", "alerts", "pings", "map", "status", "jabber", "rescue", "notes", "meta"]) {
    if (update[pane] !== undefined) {
      s[pane] = update[pane];
      dirty.add(pane);
    }
  }
  // Status and notes have no pane of their own; they are drawn on the cards and the map.
  if (dirty.has("status")) dirty.add("map");
  if (dirty.has("notes")) {
    dirty.add("intel");
    dirty.add("alerts");
    dirty.add("map");
  }
  s.seq = update.seq;
  state.snapshot = s;
  // Meta carries compact mode and the theme, which every pane draws with.
  return dirty.has("meta") ? null : dirty;
}

function markStale() {
  clearTimeout(stale);
  // A stream that has gone quiet past the keepalive is not a stream that is working.
  stale = setTimeout(() => setStatus("stale", "stale"), 20000);
}

function connect() {
  es?.close();
  es = new EventSource("/api/events");
  es.onopen = () => {
    setStatus("live", "live");
    markStale();
  };
  es.onmessage = (e) => {
    const dirty = merge(JSON.parse(e.data));
    setStatus("live", "live");
    markStale();
    render(dirty);
  };
  // A reset means the server could not fill the gap, so what is held is thrown away.
  es.addEventListener("reset", (e) => {
    state.snapshot = JSON.parse(e.data);
    setStatus("live", "live");
    markStale();
    render();
  });
  es.onerror = () => setStatus("reconnecting", "down");
}

// iOS kills the stream when the tab backgrounds and sometimes hands back a dead one on return, so
// the page reconnects deliberately rather than trusting what it is given.
document.addEventListener("visibilitychange", () => {
  if (document.visibilityState === "visible" && es?.readyState !== EventSource.OPEN) connect();
});

function boot() {
  try {
    const raw = document.getElementById("boot")?.textContent;
    if (raw && raw !== '"__BOOT__"') state.snapshot = JSON.parse(raw);
  } catch {
    // A page served without its island still works; it just paints empty for one round trip.
  }
  try {
    const raw = document.getElementById("icons")?.textContent;
    if (raw && raw !== '"__ICONS__"') state.icons = JSON.parse(raw);
  } catch {
    // A missing icon renders as nothing, which is better than a tofu square.
  }
}

/// Take the token out of the address bar.
///
/// It used to leave via a redirect, which cost the first page load its cookie on any device that
/// arrived from outside the browser. Doing it here means the token is gone from the bar, from
/// history and from any screenshot of the page, without the pairing response having to be a
/// redirect at all.
function scrubToken() {
  if (!location.search.includes("t=")) return;
  try {
    const url = new URL(location.href);
    url.searchParams.delete("t");
    history.replaceState(null, "", url.pathname + (url.search || "") + url.hash);
  } catch {
    // An address bar that cannot be rewritten is cosmetic, not fatal.
  }
}

function main() {
  scrubToken();
  boot();
  render();
  connect();
}

main();
