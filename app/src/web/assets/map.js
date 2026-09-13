// The map: one SVG, panned and zoomed by mutating its viewBox.
//
// # Why this is built in layers
//
// The first version rebuilt the whole `innerHTML` on every snapshot push, every pan and every zoom.
// With a real SDE that is 5000+ circles and a 7000-segment path, several times a second, and it made
// the entire page feel broken, not just the map. So the structure here is deliberate:
//
//   base    built once when geometry arrives, never touched again
//   live    the handful of intel and character markers, rebuilt when the snapshot changes
//   labels  rebuilt on a debounce, and only for what is on screen and only when zoomed in
//
// Pan and zoom set one attribute and touch no DOM at all.

import { ico, register, state } from "./app.js";

let geo = null;
let loading = false;
const view = { x: 0, y: 0, w: 4096, h: 4096 };
let fitted = false;
let el = null;
let lastLiveRev = -1;

const NS = "http://www.w3.org/2000/svg";

const esc = (s) =>
  String(s ?? "").replace(/[&<>"']/g, (c) =>
    ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]
  );

const secVar = (sec) => `var(--sec-${Math.min(10, Math.max(0, Math.round(sec * 10)))})`;

async function loadGeometry() {
  if (geo || loading) return;
  loading = true;
  try {
    geo = await (await fetch("/api/map/geometry")).json();
    geo.byId = new Map(geo.nodes.map((n) => [n.i, n]));
    fitted = false;
    lastLiveRev = -1;
    draw();
  } catch {
    geo = { extent: 4096, nodes: [], edges: [], byId: new Map() };
    draw();
  } finally {
    loading = false;
  }
}

/// Frame the systems that exist, not the 0..4096 box, so a sparse map is not mostly empty space.
function fit() {
  if (!geo?.nodes.length) return;
  let [x0, x1, z0, z1] = [Infinity, -Infinity, Infinity, -Infinity];
  for (const n of geo.nodes) {
    x0 = Math.min(x0, n.x); x1 = Math.max(x1, n.x);
    z0 = Math.min(z0, n.z); z1 = Math.max(z1, n.z);
  }
  const box = el?.querySelector(".starmap")?.getBoundingClientRect();
  const aspect = box && box.height > 0 ? box.width / box.height : 1;
  const pad = Math.max(40, (x1 - x0 + z1 - z0) * 0.04);
  let w = x1 - x0 + pad * 2;
  let h = z1 - z0 + pad * 2;
  // Match the box's shape so `preserveAspectRatio` does not letterbox the map inside its own pane.
  if (w / h > aspect) h = w / aspect;
  else w = h * aspect;
  view.x = (x0 + x1) / 2 - w / 2;
  view.y = (z0 + z1) / 2 - h / 2;
  view.w = Math.max(1, w);
  view.h = Math.max(1, h);
  fitted = true;
}

/// One `<path>` for every gate, because a real map is ~7000 of them and 7000 elements is a stall.
function edgePath() {
  const parts = [];
  for (const [a, b] of geo.edges) {
    const p = geo.nodes[a];
    const q = geo.nodes[b];
    if (p && q) parts.push(`M${p.x} ${p.z}L${q.x} ${q.z}`);
  }
  return parts.join("");
}

const R = 7;

function buildBase(svg) {
  const dots = geo.nodes
    .map(
      (n) =>
        `<circle class="sys" data-system="${n.i}" cx="${n.x}" cy="${n.z}" r="${R}" fill="${secVar(n.s)}"><title>${esc(n.n)}</title></circle>`
    )
    .join("");
  svg.innerHTML =
    `<path class="links" d="${edgePath()}"/>` +
    `<g class="base">${dots}</g><g class="live"></g><g class="labels"></g>`;
}

const SEV = ["info", "warning", "danger", "critical"];

/// Only the markers, which number in the tens even on a busy night.
function drawLive(svg) {
  const g = svg.querySelector(".live");
  if (!g) return;
  const live = state.snapshot?.map ?? {};
  const out = [];
  for (const [id, sev] of live.intel ?? []) {
    const n = geo.byId.get(id);
    if (n) out.push(`<circle class="hot" cx="${n.x}" cy="${n.z}" r="${R * 2.4}" fill="var(--sev-${SEV[sev] ?? "info"})"/>`);
  }
  for (const [id] of live.chars ?? []) {
    const n = geo.byId.get(id);
    if (n) out.push(`<circle class="me" cx="${n.x}" cy="${n.z}" r="${R * 1.9}"/>`);
  }
  const you = geo.byId.get(live.you);
  if (you) out.push(`<circle class="you" cx="${you.x}" cy="${you.z}" r="${R * 2.9}"/>`);
  g.innerHTML = out.join("");
}

/// Labels are the one thing that has to follow the viewport, so they are the one thing rebuilt on a
/// gesture, debounced, and only when zoomed in far enough to read them.
function drawLabels(svg) {
  const g = svg.querySelector(".labels");
  if (!g) return;
  const box = svg.getBoundingClientRect();
  const unit = view.w / Math.max(1, box.width);
  // Below this a label is smaller than the text it would replace, and thousands of them overlap
  // into grey mush.
  if (11 * unit > R * 3) {
    g.innerHTML = "";
    return;
  }
  const m = view.w * 0.1;
  const out = [];
  for (const n of geo.nodes) {
    if (n.x < view.x - m || n.x > view.x + view.w + m) continue;
    if (n.z < view.y - m || n.z > view.y + view.h + m) continue;
    out.push(`<text x="${n.x + R * 1.5}" y="${n.z + R * 0.8}" font-size="${11 * unit}">${esc(n.n)}</text>`);
    if (out.length > 400) break;
  }
  g.innerHTML = out.join("");
}

function applyView(svg) {
  svg.setAttribute("viewBox", `${view.x} ${view.y} ${view.w} ${view.h}`);
}

function draw() {
  if (!el) return;
  if (!geo) {
    el.innerHTML = `<h2>Map</h2><div class="mapwrap"><p class="placeholder">Loading the star map.</p></div>`;
    loadGeometry();
    return;
  }
  if (!geo.nodes.length) {
    el.innerHTML = `<h2>Map</h2><div class="mapwrap"><p class="placeholder">No star map: the SDE has not loaded yet.</p></div>`;
    return;
  }
  el.innerHTML =
    `<h2>Map</h2>` +
    `<div class="maptools"><button data-fit>${ico("crosshair")} Fit</button>` +
    `<span class="maphint">${geo.nodes.length} systems</span></div>` +
    `<div class="mapwrap"><svg class="starmap" preserveAspectRatio="xMidYMid slice"></svg></div>`;

  const svg = el.querySelector(".starmap");
  buildBase(svg);
  if (!fitted) fit();
  applyView(svg);
  drawLive(svg);
  drawLabels(svg);
  wire(svg);
  lastLiveRev = state.snapshot?.map?.rev ?? -1;
}

function wire(svg) {
  el.querySelector("[data-fit]")?.addEventListener("click", () => {
    fitted = false;
    fit();
    applyView(svg);
    drawLabels(svg);
  });

  const pointers = new Map();
  let pinch = null;
  let labels = null;
  const queueLabels = () => {
    clearTimeout(labels);
    labels = setTimeout(() => drawLabels(svg), 140);
  };
  const zoom = (k, ax, ay) => {
    const w = Math.min(geo.extent * 3, Math.max(40, view.w * k));
    const h = Math.min(geo.extent * 3, Math.max(40, view.h * k));
    // Keep the point under the cursor where it is.
    view.x = ax - (ax - view.x) * (w / view.w);
    view.y = ay - (ay - view.y) * (h / view.h);
    view.w = w;
    view.h = h;
    applyView(svg);
    queueLabels();
  };

  svg.addEventListener("pointerdown", (e) => {
    svg.setPointerCapture(e.pointerId);
    pointers.set(e.pointerId, e);
  });
  svg.addEventListener("pointermove", (e) => {
    if (!pointers.has(e.pointerId)) return;
    const prev = pointers.get(e.pointerId);
    pointers.set(e.pointerId, e);
    const box = svg.getBoundingClientRect();

    if (pointers.size === 2) {
      const [a, b] = [...pointers.values()];
      const dist = Math.hypot(a.clientX - b.clientX, a.clientY - b.clientY);
      if (pinch) {
        zoom(pinch / dist, view.x + view.w / 2, view.y + view.h / 2);
      }
      pinch = dist;
      return;
    }
    // Pan touches no DOM beyond the one attribute.
    view.x -= ((e.clientX - prev.clientX) / box.width) * view.w;
    view.y -= ((e.clientY - prev.clientY) / box.height) * view.h;
    applyView(svg);
    queueLabels();
  });
  const up = (e) => {
    pointers.delete(e.pointerId);
    if (pointers.size < 2) pinch = null;
  };
  svg.addEventListener("pointerup", up);
  svg.addEventListener("pointercancel", up);

  svg.addEventListener(
    "wheel",
    (e) => {
      e.preventDefault();
      const box = svg.getBoundingClientRect();
      zoom(
        e.deltaY > 0 ? 1.2 : 1 / 1.2,
        view.x + ((e.clientX - box.left) / box.width) * view.w,
        view.y + ((e.clientY - box.top) / box.height) * view.h
      );
    },
    { passive: false }
  );

  // The pane can change shape without the map changing at all: a layout switch, a pane toggled off,
  // a rotated phone. Refit rather than letting the map sit letterboxed.
  if (window.ResizeObserver) {
    let t = null;
    new ResizeObserver(() => {
      clearTimeout(t);
      t = setTimeout(() => {
        fitted = false;
        fit();
        applyView(svg);
        drawLabels(svg);
      }, 150);
    }).observe(svg);
  }
}

/// A snapshot push must not rebuild the map. Only the live markers move, and only when their pane
/// revision actually changed.
register("map", (node, snap) => {
  const first = el !== node;
  el = node;
  if (first || !el.querySelector(".starmap")) {
    draw();
    return;
  }
  const rev = snap?.map?.rev ?? -1;
  if (rev !== lastLiveRev) {
    lastLiveRev = rev;
    drawLive(el.querySelector(".starmap"));
  }
});
