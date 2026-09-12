// The map: one SVG, panned and zoomed by mutating its viewBox.
//
// Geometry arrives once from /api/map/geometry and is cached by ETag; the live layer rides in the
// snapshot and carries system ids only. Edges are drawn as a single <path> rather than one element
// each, because a universe is ~13.5k edges and 13.5k DOM nodes is a stall on a phone.

import { ico, register, state } from "./app.js";

let geo = null;
let loading = false;
const view = { x: 0, y: 0, w: 4096, h: 4096 };
let fitted = false;

const esc = (s) =>
  String(s ?? "").replace(/[&<>"']/g, (c) =>
    ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]
  );

const secVar = (sec) => `var(--sec-${Math.min(10, Math.max(0, Math.round(sec * 10)))})`;

async function loadGeometry(el) {
  if (geo || loading) return;
  loading = true;
  try {
    geo = await (await fetch("/api/map/geometry")).json();
    fitted = false;
    renderMap(el, state.snapshot);
  } catch {
    geo = { extent: 4096, nodes: [], edges: [] };
  } finally {
    loading = false;
  }
}

/// Frame the systems that exist rather than the whole 0..4096 box, so a region does not sit in a
/// corner of mostly nothing.
function fit() {
  if (!geo?.nodes.length) return;
  let [x0, x1, z0, z1] = [Infinity, -Infinity, Infinity, -Infinity];
  for (const n of geo.nodes) {
    x0 = Math.min(x0, n.x); x1 = Math.max(x1, n.x);
    z0 = Math.min(z0, n.z); z1 = Math.max(z1, n.z);
  }
  const pad = Math.max(40, (x1 - x0 + z1 - z0) * 0.06);
  view.x = x0 - pad;
  view.y = z0 - pad;
  view.w = Math.max(1, x1 - x0 + pad * 2);
  view.h = Math.max(1, z1 - z0 + pad * 2);
  fitted = true;
}

function edgePath() {
  const parts = [];
  for (const [a, b] of geo.edges) {
    const p = geo.nodes[a];
    const q = geo.nodes[b];
    if (p && q) parts.push(`M${p.x} ${p.z}L${q.x} ${q.z}`);
  }
  return parts.join("");
}

function renderMap(el, snap) {
  if (!geo) {
    el.innerHTML = `<h2>Map</h2><p class="placeholder">Loading the star map.</p>`;
    loadGeometry(el);
    return;
  }
  if (!geo.nodes.length) {
    el.innerHTML = `<h2>Map</h2><p class="placeholder">No star map: the SDE has not loaded yet.</p>`;
    return;
  }
  if (!fitted) fit();

  const live = snap?.map ?? {};
  const intel = new Map((live.intel ?? []).map(([id, sev, ts]) => [id, { sev, ts }]));
  const chars = new Map((live.chars ?? []).map(([id, n]) => [id, n]));
  const SEV = ["info", "warning", "danger", "critical"];

  // Labels only once zoomed in far enough that they do not overlap into noise.
  const labels = view.w < geo.extent * 0.45;
  // Radii are in user units, which shrink on screen as the viewBox widens, so a fixed number renders
  // sub-pixel when zoomed out. Measure the box and convert from the pixel size actually wanted.
  const px = el.querySelector(".starmap")?.clientWidth || 600;
  const unit = view.w / px;
  const r = Math.max(1, 4 * unit);

  // Only what is on screen, with a margin so a pan does not reveal holes before the next repaint.
  const m = view.w * 0.25;
  const visible = geo.nodes.filter(
    (n) =>
      n.x >= view.x - m && n.x <= view.x + view.w + m && n.z >= view.y - m && n.z <= view.y + view.h + m
  );

  const dots = visible
    .map((n) => {
      const hit = intel.get(n.i);
      const here = chars.get(n.i);
      const you = live.you === n.i;
      const ring = hit
        ? `<circle class="hot" cx="${n.x}" cy="${n.z}" r="${r * 2.2}" fill="var(--sev-${SEV[hit.sev] ?? "info"})"/>`
        : "";
      const mine = here || you
        ? `<circle class="me" cx="${n.x}" cy="${n.z}" r="${r * 1.7}" fill="none" stroke="var(--accent)" stroke-width="${Math.max(1, r / 2)}"/>`
        : "";
      const label = labels
        ? `<text x="${n.x + r * 1.6}" y="${n.z + r * 0.7}" font-size="${11 * unit}">${esc(n.n)}</text>`
        : "";
      return (
        ring +
        `<circle class="sys" data-system="${n.i}" cx="${n.x}" cy="${n.z}" r="${r}" fill="${secVar(n.s)}"><title>${esc(n.n)}</title></circle>` +
        mine +
        label
      );
    })
    .join("");

  el.innerHTML =
    `<h2>Map</h2>` +
    `<div class="maptools"><button data-fit>${ico("crosshair")} Fit</button>` +
    `<span class="maphint">${geo.nodes.length} systems</span></div>` +
    `<svg class="starmap" viewBox="${view.x} ${view.y} ${view.w} ${view.h}" preserveAspectRatio="xMidYMid meet">` +
    `<path class="links" d="${edgePath()}"/>${dots}</svg>`;

  wireSvg(el);
}

function wireSvg(el) {
  const svg = el.querySelector(".starmap");
  if (!svg) return;
  el.querySelector("[data-fit]")?.addEventListener("click", () => {
    fitted = false;
    renderMap(el, state.snapshot);
  });

  const pointers = new Map();
  let pinch = null;

  const toView = (e) => {
    const r = svg.getBoundingClientRect();
    return {
      x: view.x + ((e.clientX - r.left) / r.width) * view.w,
      y: view.y + ((e.clientY - r.top) / r.height) * view.h,
    };
  };
  // Panning only moves the box; sizes do not change, so the cheap path is enough. A zoom changes
  // what "one pixel" is worth and what is on screen, so it repaints, debounced.
  const apply = () => svg.setAttribute("viewBox", `${view.x} ${view.y} ${view.w} ${view.h}`);
  let repaint = null;
  const rescale = () => {
    clearTimeout(repaint);
    repaint = setTimeout(() => renderMap(el, state.snapshot), 120);
  };

  svg.addEventListener("pointerdown", (e) => {
    svg.setPointerCapture(e.pointerId);
    pointers.set(e.pointerId, e);
  });
  svg.addEventListener("pointermove", (e) => {
    if (!pointers.has(e.pointerId)) return;
    const prev = pointers.get(e.pointerId);
    pointers.set(e.pointerId, e);

    if (pointers.size === 2) {
      const [a, b] = [...pointers.values()];
      const dist = Math.hypot(a.clientX - b.clientX, a.clientY - b.clientY);
      if (pinch) {
        const k = pinch / dist;
        const cx = view.x + view.w / 2;
        const cy = view.y + view.h / 2;
        view.w = Math.min(geo.extent * 2, Math.max(50, view.w * k));
        view.h = Math.min(geo.extent * 2, Math.max(50, view.h * k));
        view.x = cx - view.w / 2;
        view.y = cy - view.h / 2;
        apply();
        rescale();
      }
      pinch = dist;
      return;
    }
    const r = svg.getBoundingClientRect();
    view.x -= ((e.clientX - prev.clientX) / r.width) * view.w;
    view.y -= ((e.clientY - prev.clientY) / r.height) * view.h;
    apply();
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
      const at = toView(e);
      const k = e.deltaY > 0 ? 1.15 : 1 / 1.15;
      const w = Math.min(geo.extent * 2, Math.max(50, view.w * k));
      const h = Math.min(geo.extent * 2, Math.max(50, view.h * k));
      // Zoom toward the cursor rather than the centre, so a point stays under the pointer.
      view.x = at.x - (at.x - view.x) * (w / view.w);
      view.y = at.y - (at.y - view.y) * (h / view.h);
      view.w = w;
      view.h = h;
      apply();
      rescale();
    },
    { passive: false }
  );
}

register("map", renderMap);
