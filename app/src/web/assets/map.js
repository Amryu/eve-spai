// The map, drawn on a canvas.
//
// # Why canvas
//
// SVG put one element per system in the DOM. With the real SDE that is 5255 circles, and every
// viewBox change re-rasterises all of them, so a zoom gesture rebuilt the whole layer tree each
// frame. Canvas draws the same 5255 dots in a millisecond or two and redraws on every frame without
// touching the DOM at all.
//
// It also fixes sizing. In SVG the radius is in map units, so zooming in made the dots enormous;
// here everything is drawn in screen pixels and stays the size it should be at any zoom.
//
// Hit testing is a nearest-node search rather than the browser's, which is what a canvas costs.

import { ico, register, state } from "./app.js";

let geo = null;
let loading = false;
let el = null;
let canvas = null;
let ctx = null;
let raf = null;

/// Map units per pixel. One number instead of a viewBox: the projection is `screen = (map - o) / k`.
const view = { ox: 0, oz: 0, k: 1 };
let fitted = false;
/// The region the map is framed on, or null for the whole universe.
let focused = null;

export const layers = load();

function load() {
  const dflt = {
    // "off" | "alliance" | "coalition", matching the app's SovMode.
    sov: "alliance",
    // "off" | "k" | "p" | "n" | "j", matching ActivityMode: ship kills, pod kills, NPC kills, jumps.
    activity: "off",
    adm: false,
    bridges: true,
    holes: true,
    camps: true,
    cyno: false,
    upgrades: false,
    jove: false,
    labels: true,
  };
  try {
    return { ...dflt, ...JSON.parse(localStorage.getItem("spai_map_layers") ?? "{}") };
  } catch {
    return dflt;
  }
}

/// The app's own heat ramp: yellow into red as the count approaches the scale.
function activityColour(v, scale) {
  const heat = Math.min(1, v / scale);
  return `rgb(255, ${Math.round(192 * (1 - heat))}, 48)`;
}

/// The scale each activity mode is measured against, as the app scales them.
const ACTIVITY_SCALE = { k: 20, p: 10, n: 400, j: 400 };
const ACTIVITY_LABEL = { off: "Off", k: "Ship kills", p: "Pod kills", n: "NPC kills", j: "Jumps" };
const SOV_LABEL = { off: "Off", alliance: "By alliance", coalition: "By coalition" };

function saveLayers() {
  try {
    localStorage.setItem("spai_map_layers", JSON.stringify(layers));
  } catch {
    // Private browsing; the choice lasts one session.
  }
}

const css = (name) => getComputedStyle(document.documentElement).getPropertyValue(name).trim();

/// Resolved once per draw. Reading a custom property per dot would be thousands of style lookups.
let pal = null;
function palette() {
  return {
    sec: Array.from({ length: 11 }, (_, i) => css(`--sec-${i}`)),
    sev: ["info", "warning", "danger", "critical"].map((s) => css(`--sev-${s}`)),
    line: css("--line"),
    accent: css("--accent"),
    muted: css("--muted"),
    alliance: css("--alliance"),
    hostile: css("--hostile"),
    warning: css("--warning"),
    fg: css("--fg"),
  };
}

async function loadGeometry() {
  if (geo || loading) return;
  loading = true;
  try {
    geo = await (await fetch("/api/map/geometry")).json();
    geo.byId = new Map(geo.nodes.map((n, idx) => [n.i, idx]));
    fitted = false;
    build();
  } catch {
    geo = { extent: 4096, nodes: [], edges: [], bridges: [], byId: new Map() };
    build();
  } finally {
    loading = false;
  }
}

const sx = (x) => (x - view.ox) / view.k;
const sy = (z) => (z - view.oz) / view.k;

/// Frame a set of systems. `region` frames one region, the way the app's region view does; with no
/// argument it frames everything, which is the universe view.
function fit(region = null) {
  if (!geo?.nodes.length || !canvas) return;
  let [x0, x1, z0, z1] = [Infinity, -Infinity, Infinity, -Infinity];
  const within = region == null ? geo.nodes : geo.nodes.filter((n) => n.r === region);
  if (!within.length) return;
  for (const n of within) {
    x0 = Math.min(x0, n.x); x1 = Math.max(x1, n.x);
    z0 = Math.min(z0, n.z); z1 = Math.max(z1, n.z);
  }
  const w = canvas.clientWidth || 1;
  const h = canvas.clientHeight || 1;
  // A single region is a handful of systems, so it wants more air around it than the universe does.
  const pad = region == null ? 0.04 : 0.15;
  view.k = Math.max((x1 - x0) / w, (z1 - z0) / h) * (1 + pad * 2) || 1;
  view.ox = (x0 + x1) / 2 - (w * view.k) / 2;
  view.oz = (z0 + z1) / 2 - (h * view.k) / 2;
  fitted = true;
}

/// Dot radius in screen pixels.
///
/// A star map is mostly space; the dots are markers, not planets. This tops out at 3px, which is
/// about what the desktop draws, and the overlay rings are multiples of it so they shrink with it.
function radius() {
  return Math.max(1.1, Math.min(3, 1.6 / Math.sqrt(view.k) * 6));
}

function schedule() {
  if (raf) return;
  raf = requestAnimationFrame(() => {
    raf = null;
    paint();
  });
}

function paint() {
  if (!ctx || !geo) return;
  const dpr = window.devicePixelRatio || 1;
  const w = canvas.clientWidth;
  const h = canvas.clientHeight;
  if (canvas.width !== Math.round(w * dpr) || canvas.height !== Math.round(h * dpr)) {
    canvas.width = Math.round(w * dpr);
    canvas.height = Math.round(h * dpr);
  }
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.clearRect(0, 0, w, h);
  if (!pal) pal = palette();

  const live = state.snapshot?.map ?? {};
  const status = state.snapshot?.status?.systems ?? {};
  const r = radius();
  const pad = 40;
  const onScreen = (px, py) => px >= -pad && px <= w + pad && py >= -pad && py <= h + pad;

  // Gates. One path for all of them, clipped to the viewport as we go: off-screen segments still
  // cost the rasteriser if they are in the path.
  ctx.lineWidth = 1;
  ctx.strokeStyle = pal.line;
  ctx.beginPath();
  for (const [a, b] of geo.edges) {
    const p = geo.nodes[a];
    const q = geo.nodes[b];
    const px = sx(p.x), py = sy(p.z), qx = sx(q.x), qy = sy(q.z);
    if (!onScreen(px, py) && !onScreen(qx, qy)) continue;
    ctx.moveTo(px, py);
    ctx.lineTo(qx, qy);
  }
  ctx.stroke();

  if (layers.bridges && geo.bridges?.length) {
    ctx.strokeStyle = pal.alliance;
    ctx.setLineDash([4, 3]);
    ctx.beginPath();
    for (const [a, b] of geo.bridges) {
      const p = geo.nodes[a];
      const q = geo.nodes[b];
      const px = sx(p.x), py = sy(p.z), qx = sx(q.x), qy = sy(q.z);
      if (!onScreen(px, py) && !onScreen(qx, qy)) continue;
      ctx.moveTo(px, py);
      ctx.lineTo(qx, qy);
    }
    ctx.stroke();
    ctx.setLineDash([]);
  }

  if (layers.holes && live.holes?.length) {
    ctx.strokeStyle = pal.accent;
    ctx.setLineDash([2, 4]);
    ctx.beginPath();
    for (const [a, b] of live.holes) {
      const p = geo.nodes[geo.byId.get(a)];
      const q = geo.nodes[geo.byId.get(b)];
      if (!p || !q) continue;
      ctx.moveTo(sx(p.x), sy(p.z));
      ctx.lineTo(sx(q.x), sy(q.z));
    }
    ctx.stroke();
    ctx.setLineDash([]);
  }

  // Sovereignty sits under the systems as a soft wash, the way the app shades it.
  if (layers.sov !== "off") {
    const key = layers.sov === "coalition" ? "coal" : "sov";
    ctx.globalAlpha = 0.3;
    for (const n of geo.nodes) {
      const colour = status[n.i]?.[key];
      if (!colour) continue;
      const px = sx(n.x), py = sy(n.z);
      if (!onScreen(px, py)) continue;
      ctx.fillStyle = colour;
      ctx.beginPath();
      ctx.arc(px, py, r * 2.6, 0, Math.PI * 2);
      ctx.fill();
    }
    ctx.globalAlpha = 1;
  }

  // Activity, on the app's yellow-to-red heat ramp, sized by how busy the system is.
  if (layers.activity !== "off") {
    const scale = ACTIVITY_SCALE[layers.activity] ?? 100;
    ctx.globalAlpha = 0.55;
    for (const n of geo.nodes) {
      const v = status[n.i]?.[layers.activity] ?? 0;
      if (!v) continue;
      const px = sx(n.x), py = sy(n.z);
      if (!onScreen(px, py)) continue;
      ctx.fillStyle = activityColour(v, scale);
      ctx.beginPath();
      ctx.arc(px, py, r * (1.6 + Math.min(1, v / scale) * 2), 0, Math.PI * 2);
      ctx.fill();
    }
    ctx.globalAlpha = 1;
  }

  // Intel heat.
  ctx.globalAlpha = 0.45;
  for (const [id, sev] of live.intel ?? []) {
    const n = geo.nodes[geo.byId.get(id)];
    if (!n) continue;
    const px = sx(n.x), py = sy(n.z);
    if (!onScreen(px, py)) continue;
    ctx.fillStyle = pal.sev[sev] ?? pal.sev[0];
    ctx.beginPath();
    ctx.arc(px, py, r * 2.2, 0, Math.PI * 2);
    ctx.fill();
  }
  ctx.globalAlpha = 1;

  // Systems.
  let drawn = 0;
  for (const n of geo.nodes) {
    const px = sx(n.x), py = sy(n.z);
    if (!onScreen(px, py)) continue;
    drawn++;
    ctx.fillStyle = pal.sec[Math.min(10, Math.max(0, Math.round(n.s * 10)))] ?? pal.muted;
    ctx.beginPath();
    ctx.arc(px, py, r, 0, Math.PI * 2);
    ctx.fill();
  }

  // ADM, as a number beside the system, which is how the app shows it.
  if (layers.adm) {
    ctx.fillStyle = pal.fg;
    ctx.font = "10px system-ui, sans-serif";
    ctx.textBaseline = "middle";
    for (const n of geo.nodes) {
      const adm = status[n.i]?.adm;
      if (adm == null) continue;
      const px = sx(n.x), py = sy(n.z);
      if (!onScreen(px, py)) continue;
      ctx.fillText(adm.toFixed(1), px + r + 2, py - r - 4);
    }
  }

  // Sov upgrades: one mark each, coloured by level the way `level_color` does, mining marks tinted
  // to say they are ore.
  if (layers.upgrades && live.upgrades?.length) {
    for (const [id, marks] of live.upgrades) {
      const n = geo.nodes[geo.byId.get(id)];
      if (!n) continue;
      const px = sx(n.x), py = sy(n.z);
      if (!onScreen(px, py)) continue;
      marks.forEach((m, i) => {
        ctx.fillStyle = m.l >= 3 ? pal.hostile : m.l === 2 ? css("--friendly") : pal.fg;
        const w = Math.max(2, r * 0.9);
        const x = px - r + i * (w + 1.5);
        // Mining marks are drawn as a diamond so a glance tells them apart without an ore icon.
        if (m.k === 2) {
          ctx.beginPath();
          ctx.moveTo(x + w / 2, py - r - w);
          ctx.lineTo(x + w, py - r - w / 2);
          ctx.lineTo(x + w / 2, py - r);
          ctx.lineTo(x, py - r - w / 2);
          ctx.fill();
        } else {
          ctx.fillRect(x, py - r - w, w, w);
        }
      });
    }
  }

  // Cyno generators.
  if (layers.cyno && live.cyno?.length) {
    ctx.strokeStyle = pal.warning;
    ctx.lineWidth = 1.5;
    for (const id of live.cyno) {
      const n = geo.nodes[geo.byId.get(id)];
      if (!n) continue;
      const px = sx(n.x), py = sy(n.z);
      if (!onScreen(px, py)) continue;
      ctx.beginPath();
      ctx.moveTo(px - r * 2, py);
      ctx.lineTo(px + r * 2, py);
      ctx.moveTo(px, py - r * 2);
      ctx.lineTo(px, py + r * 2);
      ctx.stroke();
    }
  }

  if (layers.jove) {
    ctx.strokeStyle = pal.muted;
    ctx.lineWidth = 1;
    for (const n of geo.nodes) {
      if (!n.j) continue;
      const px = sx(n.x), py = sy(n.z);
      if (!onScreen(px, py)) continue;
      ctx.beginPath();
      ctx.rect(px - r * 1.8, py - r * 1.8, r * 3.6, r * 3.6);
      ctx.stroke();
    }
  }

  if (layers.camps && live.camps?.length) {
    ctx.strokeStyle = pal.hostile;
    ctx.lineWidth = 2;
    for (const id of live.camps) {
      const n = geo.nodes[geo.byId.get(id)];
      if (!n) continue;
      const px = sx(n.x), py = sy(n.z);
      if (!onScreen(px, py)) continue;
      ctx.beginPath();
      ctx.arc(px, py, r * 2, 0, Math.PI * 2);
      ctx.stroke();
    }
  }

  // Where your characters are, and which one is you.
  ctx.strokeStyle = pal.accent;
  ctx.lineWidth = 2;
  for (const [id] of live.chars ?? []) {
    const n = geo.nodes[geo.byId.get(id)];
    if (!n) continue;
    ctx.beginPath();
    ctx.arc(sx(n.x), sy(n.z), r * 2.2, 0, Math.PI * 2);
    ctx.stroke();
  }
  const you = geo.nodes[geo.byId.get(live.you)];
  if (you) {
    ctx.lineWidth = 2.5;
    ctx.beginPath();
    ctx.arc(sx(you.x), sy(you.z), r * 3.2, 0, Math.PI * 2);
    ctx.stroke();
  }

  // Labels, only when they would be readable and only for what is on screen.
  if (layers.labels && r >= 3) {
    ctx.fillStyle = pal.muted;
    ctx.font = "11px system-ui, sans-serif";
    ctx.textBaseline = "middle";
    let n = 0;
    for (const s of geo.nodes) {
      const px = sx(s.x), py = sy(s.z);
      if (!onScreen(px, py)) continue;
      ctx.fillText(s.n, px + r + 3, py);
      if (++n > 300) break;
    }
  }

  const hint = el?.querySelector(".maphint");
  if (hint) {
    hint.textContent = focused
      ? `${drawn} shown, region view`
      : `${drawn} of ${geo.nodes.length} systems`;
  }
}

const TOGGLES = [
  ["adm", "ADM"],
  ["bridges", "Bridges"],
  ["holes", "Wormholes"],
  ["camps", "Camps"],
  ["cyno", "Cyno"],
  ["upgrades", "Upgrades"],
  ["jove", "Jove"],
  ["labels", "Labels"],
];

const CYCLES = [
  ["sov", "Sov", ["off", "alliance", "coalition"], SOV_LABEL],
  ["activity", "Activity", ["off", "k", "p", "n", "j"], ACTIVITY_LABEL],
];

/// The layer controls live behind one button at every width.
///
/// Ten controls in a row is most of a phone's screen and a third of a pane on a desktop, and the map
/// is the thing worth the space. Same panel either way, anchored on a desktop and a sheet on a
/// phone, which is the pattern the layout menu already set.
function layerPanel() {
  return (
    `<div class="mlayers" hidden>` +
    CYCLES.map(
      ([key, label, , names]) =>
        `<div class="mlrow"><span>${label}</span>` +
        `<button class="mlcycle" data-cycle="${key}">${names[layers[key]]}</button></div>`
    ).join("") +
    `<div class="mlgrid">` +
    TOGGLES.map(
      ([k, label]) =>
        `<button class="ml${layers[k] ? " on" : ""}" data-layer="${k}">${label}</button>`
    ).join("") +
    `</div></div>`
  );
}

function build() {
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
    `<div class="maptools">` +
    `<button data-fit>${ico("crosshair")} Fit</button>` +
    `<button data-layers>${ico("squares-four")} Layers</button>` +
    `<span class="maphint"></span>` +
    layerPanel() +
    `</div>` +
    `<div class="mapwrap"><canvas class="starmap"></canvas></div>`;

  canvas = el.querySelector("canvas");
  ctx = canvas.getContext("2d");
  pal = null;
  if (!fitted) fit();
  wire();
  schedule();
}

function wire() {
  el.querySelector("[data-fit]")?.addEventListener("click", () => {
    focused = null;
    fit();
    schedule();
  });
  const panel = el.querySelector(".mlayers");
  // `#maplayers` opens it on load, for the same reason the dialogs and the layout menu take deep
  // links: the harness cannot click.
  if (location.hash === "#maplayers" && panel) panel.hidden = false;
  el.querySelector("[data-layers]")?.addEventListener("click", (e) => {
    e.stopPropagation();
    panel.hidden = !panel.hidden;
  });
  // Anywhere else closes it, the same as the layout menu.
  document.addEventListener("click", (e) => {
    if (panel && !panel.hidden && !e.target.closest(".maptools")) panel.hidden = true;
  });

  el.querySelectorAll("[data-layer]").forEach((b) =>
    b.addEventListener("click", () => {
      layers[b.dataset.layer] = !layers[b.dataset.layer];
      b.classList.toggle("on", layers[b.dataset.layer]);
      saveLayers();
      schedule();
    })
  );
  el.querySelectorAll("[data-cycle]").forEach((b) =>
    b.addEventListener("click", () => {
      const [, , order, names] = CYCLES.find(([k]) => k === b.dataset.cycle);
      const next = order[(order.indexOf(layers[b.dataset.cycle]) + 1) % order.length];
      layers[b.dataset.cycle] = next;
      b.textContent = names[next];
      saveLayers();
      schedule();
    })
  );

  const pointers = new Map();
  let pinch = null;
  let moved = 0;

  const zoomAt = (factor, px, py) => {
    const k = Math.max(geo.extent / 200000, Math.min(geo.extent / 200, view.k * factor));
    // Keep the map point under the cursor where it is.
    view.ox += px * (view.k - k);
    view.oz += py * (view.k - k);
    view.k = k;
    schedule();
  };

  canvas.addEventListener("pointerdown", (e) => {
    canvas.setPointerCapture(e.pointerId);
    pointers.set(e.pointerId, e);
    moved = 0;
  });
  canvas.addEventListener("pointermove", (e) => {
    if (!pointers.has(e.pointerId)) return;
    const prev = pointers.get(e.pointerId);
    pointers.set(e.pointerId, e);
    if (pointers.size === 2) {
      const [a, b] = [...pointers.values()];
      const dist = Math.hypot(a.clientX - b.clientX, a.clientY - b.clientY);
      if (pinch) {
        const box = canvas.getBoundingClientRect();
        zoomAt(pinch / dist, (a.clientX + b.clientX) / 2 - box.left, (a.clientY + b.clientY) / 2 - box.top);
      }
      pinch = dist;
      return;
    }
    moved += Math.abs(e.clientX - prev.clientX) + Math.abs(e.clientY - prev.clientY);
    view.ox -= (e.clientX - prev.clientX) * view.k;
    view.oz -= (e.clientY - prev.clientY) * view.k;
    schedule();
  });
  const up = (e) => {
    pointers.delete(e.pointerId);
    if (pointers.size < 2) pinch = null;
  };
  canvas.addEventListener("pointerup", up);
  canvas.addEventListener("pointercancel", up);

  canvas.addEventListener(
    "wheel",
    (e) => {
      e.preventDefault();
      const box = canvas.getBoundingClientRect();
      zoomAt(e.deltaY > 0 ? 1.2 : 1 / 1.2, e.clientX - box.left, e.clientY - box.top);
    },
    { passive: false }
  );

  // Nearest node within a thumb's reach. This is what a canvas costs in place of the browser's own
  // hit testing, and it is cheaper than 5000 elements.
  // Double-click frames the region, which is the app's region view. Fit goes back to the universe.
  canvas.addEventListener("dblclick", (e) => {
    const n = nearest(e);
    if (!n) return;
    focused = n.r;
    fit(n.r);
    schedule();
  });

  canvas.addEventListener("click", (e) => {
    if (moved > 6) return;
    const box = canvas.getBoundingClientRect();
    const mx = e.clientX - box.left;
    const my = e.clientY - box.top;
    const best = nearest(e);
    if (best) location.hash = `#system/${best.i}`;
  });

  if (window.ResizeObserver) {
    new ResizeObserver(() => {
      pal = null;
      schedule();
    }).observe(canvas);
  }
}

/// Nearest system to a pointer event, within a thumb's reach. This is what a canvas costs in place
/// of the browser's own hit testing, and for 5000 systems it is cheaper than 5000 hit targets.
function nearest(e) {
  const box = canvas.getBoundingClientRect();
  const mx = e.clientX - box.left;
  const my = e.clientY - box.top;
  let best = null;
  let bestD = 18 * 18;
  for (const n of geo.nodes) {
    const dx = sx(n.x) - mx;
    const dy = sy(n.z) - my;
    const d = dx * dx + dy * dy;
    if (d < bestD) {
      bestD = d;
      best = n;
    }
  }
  return best;
}

register("map", (node, snap) => {
  const first = el !== node;
  el = node;
  if (first || !el.querySelector("canvas")) {
    build();
    return;
  }
  // A snapshot only changes the overlays, and a repaint is one canvas frame.
  schedule();
});
