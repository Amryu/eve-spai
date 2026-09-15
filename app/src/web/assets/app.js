// The shell: shared state, the event stream, and pane registration.

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

/// Resolves false without trying when the page may not write back.
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

/// Escape closes the dialog only when `escape` is set, since most dialogs leave Escape to the focused
/// field. `onClose` runs once, however it closed.
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

export const renderers = {};

/// Repaints, because `main()` has already painted by the time a pane module is evaluated, and the
/// pane would otherwise wait for the next push.
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

/// Rescue exists only in an `fc-rescue` build with the mode switched on.
export function available() {
  return PANES.filter((p) => p !== "rescue" || state.snapshot?.meta?.rescue);
}

function renderTabs() {
  // `data-tab`, not `data-pane`: the tab buttons come first in the document, so a shared attribute
  // would make the pane lookup match a button.
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
  // The tab bar was just rebuilt, so `afterRender` below restores what the layout put on it.
  for (const pane of PANES) {
    if (dirty && !dirty.has(pane)) continue;
    const el = document.querySelector(`#panes [data-pane="${pane}"]`);
    if (!el || !renderers[pane]) continue;
    // Re-rendering replaces the scrolled element, which would jump the reader back to the top.
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

/// Runs after every repaint, chiefly so the layout reapplies pane order and tab selection.
export const afterRender = [];

let es = null;
let stale = null;

/// Merge an update and report which panes it touched. The server sends a pane whole and only when it
/// changed, so re-rendering just those keeps the map from redrawing on every push.
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
  // Quiet past the 15 s keepalive means the stream is dead.
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

// iOS kills the stream when the tab backgrounds and sometimes hands back a dead one on return.
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

/// Removes the token from the address bar, history and screenshots. A pairing redirect would lose
/// the cookie on a device arriving from outside the browser.
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
