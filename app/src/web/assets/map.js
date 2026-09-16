// The map, drawn on a canvas. As SVG, the SDE's 5255 systems re-rasterise on every viewBox change;
// canvas redraws them in a millisecond or two per frame. Everything is drawn in screen pixels so it
// keeps its size at any zoom. Hit testing is a nearest-node search, see `nearest`.

import { ico, register, send, state } from "./app.js";
import { avoidOnce, currentRoute, showRoute, titansOnce } from "./dialogs.js";
import { lightYears, menu, radial, reach } from "./route.js";
import { openEditor, quickMenu, rgb, tagById } from "./notes.js";

let geo = null;
let loading = false;
let el = null;
let canvas = null;
let ctx = null;
let raf = null;

/// The drawn view: `screen = (map - o) / k`, with `k` in map units per pixel.
const view = { ox: 0, oz: 0, k: 1 };
/// Where the view is heading. Zoom writes this and the drawn `view` eases towards it; pan writes
/// both at once, because a drag has to track the pointer exactly.
const target = { ox: 0, oz: 0, k: 1 };
/// Upper bound on a zoom's duration. Below it the duration scales with the size of the jump, so a
/// wheel notch is not gluey, a region reframe is not a jump cut, and trackpad deltas land at once.
const ZOOM_EASE_MS = 180;
/// Milliseconds per e-fold of zoom. 180ms is reached at a factor of about two.
const ZOOM_EASE_PER_LN = 260;
/// Starts and stops at rest. Exponential decay lurches in the first frame and drifts in after.
const easeInOut = (p) => (p < 0.5 ? 4 * p * p * p : 1 - Math.pow(-2 * p + 2, 3) / 2);

/// The app's zoom limits, as multiples of the framed universe: `map_zoom.clamp(0.7, 60.0)`.
/// Held as multiples rather than values of `k` so they agree with the app whatever the pane's size.
const ZOOM_MIN = 0.7;
const ZOOM_MAX = 60;
/// `k` for the framed universe: the app's zoom of 1. Set by the universe fit.
let fitK = 0;
/// The zoom in flight: the map point pinned under the cursor, where `k` set off from, and when.
/// Null when the map is settled.
let anim = null;
let fitted = false;
/// The region the map is framed on, or null for the whole universe.
let focused = null;
/// The system under the pointer, if any.
let hovered = null;
/// The system in the hash. It keeps the ring and jump-range tint up after the pointer moves off, as
/// the app does.
let selected = null;
/// A route drag in flight: where it started, where the pointer is, and what it would land on.
let link = null;
/// The route the user picked, drawn until they pick another or clear it.
let picked = null;
/// The systems the drags named, in order: start, waypoints, destination.
let anchors = [];
/// What the route is being planned as. Chosen once, at the first drag.
let routeKind = null;

const layers = load();

function load() {
  const dflt = {
    // "off" | "alliance" | "coalition", matching the app's SovMode.
    sov: "alliance",
    // "off" | "k" | "p" | "n" | "j", matching ActivityMode: ship kills, pod kills, NPC kills, jumps.
    activity: "off",
    adm: false,
    jumprange: false,
    bridges: true,
    holes: true,
    camps: true,
    cyno: false,
    upgrades: false,
    jove: false,
    notes: true,
    labels: true,
  };
  try {
    return { ...dflt, ...JSON.parse(localStorage.getItem("spai_map_layers") ?? "{}") };
  } catch {
    return dflt;
  }
}

/// Jump ranges at maxed skills, and their band colours, both as `map::JUMP_RANGES` and the app's
/// own palette have them.
const JUMP_RANGES = [
  ["Super / Titan", 6],
  ["Capital", 7],
  ["Black Ops", 8],
  ["Jump Freighter", 10],
];
const RANGE_COLOURS = ["#5ac86a", "#e0a43a", "#4f9bd8", "#d84c4c"];

/// Canvas cannot use a webfont until it has loaded, and falls back silently if asked early, which is
/// draws a box instead of a glyph. Repaint once it is in.
let iconFontReady = false;
if (document.fonts?.load) {
  document.fonts.load('16px phosphor, "phosphor"').then(() => {
    iconFontReady = true;
    schedule();
  }).catch(() => {});
} else {
  iconFontReady = true;
}

/// EVE type icons, for the ore an upgrade yields. Fetched once each and repainted on arrival.
const oreIcons = new Map();
function oreIcon(id) {
  let img = oreIcons.get(id);
  if (img === undefined) {
    img = new Image();
    img.crossOrigin = "anonymous";
    img.onload = () => schedule();
    img.src = `https://images.evetech.net/types/${id}/icon?size=32`;
    oreIcons.set(id, img);
  }
  return img.complete && img.naturalWidth ? img : null;
}

/// Draw one of the app's own phosphor glyphs, centred on a point.
function glyph(c, name, x, y, size, colour) {
  const ch = state.icons?.[name];
  if (!ch || !iconFontReady) return false;
  c.save();
  c.font = `${size}px phosphor`;
  c.textAlign = "center";
  c.textBaseline = "middle";
  c.fillStyle = colour;
  c.fillText(ch, x, y);
  c.restore();
  return true;
}

/// The green the app draws a bridge in.
const BRIDGE_GREEN = "#3ad06a";
/// Route colours, matching `Leg::color` in the app: gates cyan, bridges green, holes purple.
const ROUTE_CYAN = "#4fc3f7";
const ROUTE_BRIDGE = "#5ac86a";
const ROUTE_HOLE = "#b07ce8";
/// A route the user asked for, in its own colours so it does not read as the app's travel route.
const PICK_GATE = "#f2b134";
const PICK_JUMP = "#e07be0";
/// The titan's jump is a different ship's move, so it does not share the capital-jump colour.
const TITAN_COL = "#ff7a3d";
/// Dash and gap, matching `dashed_flow` in the app.
const DASH = [6, 6];
/// How far a bridge arch bows out, as a fraction of its own length. Matches `app::BRIDGE_BOW`.
const BRIDGE_BOW = 0.12;

/// Add a bowed arc to the current path, the same quadratic the app samples.
function arc(c, ax, ay, bx, by) {
  const dx = bx - ax;
  const dy = by - ay;
  const len = Math.hypot(dx, dy);
  if (len < 0.5) {
    c.moveTo(ax, ay);
    c.lineTo(bx, by);
    return;
  }
  // Always bows upward: the perpendicular flips with the segment's direction, so otherwise the arch
  // side would depend on node order.
  let nx = -dy / len;
  let ny = dx / len;
  if (ny > 0) {
    nx = -nx;
    ny = -ny;
  }
  const cx = (ax + bx) / 2 + nx * len * BRIDGE_BOW;
  const cy = (ay + by) / 2 + ny * len * BRIDGE_BOW;
  c.moveTo(ax, ay);
  c.quadraticCurveTo(cx, cy, bx, by);
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
    surface: css("--surface"),
    bg: css("--bg"),
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
    centroids = null;
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
  const w = canvas.clientWidth;
  const h = canvas.clientHeight;
  // A hidden pane has no size, and fitting to it puts the map off the canvas once shown. `fitted`
  // stays unset so the first paint with a real size fits.
  if (w < 2 || h < 2) return;
  if (region == null) {
    let [a0, a1, b0, b1] = [Infinity, -Infinity, Infinity, -Infinity];
    for (const n of geo.nodes) {
      a0 = Math.min(a0, n.x); a1 = Math.max(a1, n.x);
      b0 = Math.min(b0, n.z); b1 = Math.max(b1, n.z);
    }
    fitK = Math.max((a1 - a0) / w, (b1 - b0) / h) * 1.08 || 1;
  }
  let [x0, x1, z0, z1] = [Infinity, -Infinity, Infinity, -Infinity];
  const within = region == null ? geo.nodes : geo.nodes.filter((n) => n.r === region);
  if (!within.length) return;
  for (const n of within) {
    x0 = Math.min(x0, n.x); x1 = Math.max(x1, n.x);
    z0 = Math.min(z0, n.z); z1 = Math.max(z1, n.z);
  }
  // A single region is a handful of systems, so it wants more air around it than the universe does.
  const pad = region == null ? 0.04 : 0.15;
  view.k = Math.max((x1 - x0) / w, (z1 - z0) / h) * (1 + pad * 2) || 1;
  view.ox = (x0 + x1) / 2 - (w * view.k) / 2;
  view.oz = (z0 + z1) / 2 - (h * view.k) / 2;
  Object.assign(target, view);
  anim = null;
  fitted = true;
}

/// Dot radius in screen pixels, capped at about what the desktop draws. Overlay rings are multiples
/// of it so they shrink with it.
function radius() {
  return Math.max(1.1, Math.min(3, 1.6 / Math.sqrt(view.k) * 6));
}

/// Advance the zoom in flight, and say whether it still has distance to cover.
///
/// A notch arriving mid-flight restarts the curve from wherever the view has got to, so a fast
/// scroll is one continuous movement rather than a queue of animations fighting each other.
function settle(now) {
  if (!anim) return false;
  const e = easeInOut(Math.min(1, (now - anim.t0) / anim.dur));
  // Geometric in `k`, because zoom is: the halfway point of a zoom is the geometric mean, not the
  // arithmetic one, and interpolating it linearly races at one end and crawls at the other.
  view.k = anim.k0 * Math.pow(target.k / anim.k0, e);
  // The pan is derived from the zoom rather than eased on its own curve, which would let the anchor
  // slide mid-animation. Solving `screen = (map - o) / k` each frame keeps it under the cursor.
  view.ox = anim.ax - anim.px * view.k;
  view.oz = anim.az - anim.py * view.k;
  if (e < 1) return true;
  Object.assign(view, target);
  anim = null;
  return false;
}

function schedule() {
  if (raf) return;
  raf = requestAnimationFrame((now) => {
    raf = null;
    if (settle(now)) schedule();
    paint();
  });
}

/// Region label positions, worked out once from the geometry: the centre of each region's systems.
let centroids = null;
function regionCentroids() {
  if (centroids) return centroids;
  const acc = new Map();
  for (const n of geo.nodes) {
    const a = acc.get(n.r) ?? { x: 0, z: 0, c: 0 };
    a.x += n.x;
    a.z += n.z;
    a.c++;
    acc.set(n.r, a);
  }
  const names = new Map(geo.regions ?? []);
  centroids = [];
  for (const [id, a] of acc) {
    const n = names.get(id);
    if (!n) continue;
    centroids.push([id, { n, x: a.x / a.c, z: a.z / a.c, c: a.c }]);
  }
  // Biggest first, so collision resolution keeps the regions that cover the most map.
  centroids.sort((p, q) => q[1].c - p[1].c);
  return centroids;
}

function paint() {
  if (!ctx || !geo) return;
  const dpr = window.devicePixelRatio || 1;
  const w = canvas.clientWidth;
  const h = canvas.clientHeight;
  // A pane switched on later arrives here once with no size and again with one.
  if (w < 2 || h < 2) return;
  if (!fitted) fit();
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
  /// Markers for a system, collected across the layers and drawn in one centred row above it so
  /// they cannot overlap.
  const marks = new Map();
  /// The ceiling for text and marker size, from the pane size: a fixed 13px is lost on a full-screen
  /// map. Capped because bigger type starts crowding. The actual size is `grow`.
  const uiScale = Math.min(1.9, Math.max(1, Math.min(w, h) / 620));
  const span = view.k * w;
  /// Names and markers grow from base size at the names threshold to `uiScale` at full zoom, so
  /// they are not a wall of type over a map that is still mostly space. Measured in `k` rather than
  /// the span so a wider pane does not move the ends.
  const kNames = geo.extent / 4 / w;
  const zoomT = Math.min(1, Math.max(0, Math.log(kNames / view.k) / Math.log(kNames / (geo.extent / 200000))));
  const grow = 1 + (uiScale - 1) * zoomT;
  const namesOn = layers.labels && span < geo.extent / 4;
  const nameSize = Math.round(13 * grow);
  const mark = (id, name, colour) => {
    const list = marks.get(id);
    if (list) list.push([name, colour]);
    else marks.set(id, [[name, colour]]);
  };
  const ICON = Math.round(16 * grow);
  const pad = 40;
  const onScreen = (px, py) => px >= -pad && px <= w + pad && py >= -pad && py <= h + pad;
  /// Whether a segment's bounding box meets the viewport. An endpoint test would drop links that
  /// span the screen with both ends outside it, such as jump bridges.
  const segmentVisible = (ax, ay, bx, by) =>
    Math.max(ax, bx) >= -pad &&
    Math.min(ax, bx) <= w + pad &&
    Math.max(ay, by) >= -pad &&
    Math.min(ay, by) <= h + pad;

  // Gates, in three passes so each dash pattern is set once rather than per segment. A gate says
  // where a boundary runs as much as where you can go: solid inside a constellation, dotted across
  // one, dashed out of the region. Same reading as the app's map.
  const GATE_DASH = [[], [1, 3], [4, 4]];
  ctx.lineWidth = 1;
  ctx.strokeStyle = pal.line;
  for (let style = 0; style < 3; style++) {
    ctx.setLineDash(GATE_DASH[style]);
    ctx.beginPath();
    for (const [a, b, k] of geo.edges) {
      if ((k ?? 0) !== style) continue;
      const p = geo.nodes[a];
      const q = geo.nodes[b];
      const px = sx(p.x), py = sy(p.z), qx = sx(q.x), qy = sy(q.z);
      if (!segmentVisible(px, py, qx, qy)) continue;
      ctx.moveTo(px, py);
      ctx.lineTo(qx, qy);
    }
    ctx.stroke();
  }
  ctx.setLineDash([]);

  // Jump bridges arch, because colour alone does not tell a bridge from a gate on a busy map. A
  // bridge a route flies is drawn by the route, so it is skipped here.
  const routed = new Set();
  const hops = picked?.hops ?? [];
  for (let i = 1; i < hops.length; i++) {
    if (hops[i].kind === 1) routed.add(`${hops[i - 1].id},${hops[i].id}`);
  }
  // The app's own travel route too: it draws its bridged legs as arcs in the route colour, and the
  // page redraws that route from `live.route` with the same rule. Hidden while this page is planning
  // a route, since two routes over the same systems cannot be told apart.
  const live_route = planning() ? [] : live.route ?? [];
  for (let i = 1; i < live_route.length; i++) {
    routed.add(`${live_route[i - 1]},${live_route[i]}`);
  }
  if (layers.bridges && geo.bridges?.length) {
    ctx.strokeStyle = BRIDGE_GREEN;
    ctx.lineWidth = 1.5;
    ctx.beginPath();
    for (const [a, b] of geo.bridges) {
      const p = geo.nodes[a];
      const q = geo.nodes[b];
      if (routed.has(`${p.i},${q.i}`) || routed.has(`${q.i},${p.i}`)) continue;
      const px = sx(p.x), py = sy(p.z), qx = sx(q.x), qy = sy(q.z);
      if (!segmentVisible(px, py, qx, qy)) continue;
      arc(ctx, px, py, qx, qy);
    }
    ctx.stroke();
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
    for (const [a, b] of live.holes) {
      for (const id of [a, b]) {
        const n = geo.nodes[geo.byId.get(id)];
        if (!n) continue;
        const px = sx(n.x), py = sy(n.z);
        if (onScreen(px, py)) glyph(ctx, "spiral", px, py - r - 7, 12, pal.accent);
      }
    }
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
    ctx.font = `${Math.round(10 * grow)}px system-ui, sans-serif`;
    ctx.textBaseline = "middle";
    for (const n of geo.nodes) {
      const adm = status[n.i]?.adm;
      if (adm == null) continue;
      const px = sx(n.x), py = sy(n.z);
      if (!onScreen(px, py)) continue;
      ctx.fillText(adm.toFixed(1), px + r + 2, py - r - 4);
    }
  }

  // Sov upgrades use the app's glyphs, coloured by level as `level_color` does. A mining upgrade
  // shows its ore icon.
  const UPGRADE_GLYPH = ["skull", "broadcast", null, "gear"];
  if (layers.upgrades && live.upgrades?.length) {
    for (const [id, ups] of live.upgrades) {
      ups.forEach((m) => {
        const colour = m.l >= 3 ? pal.hostile : m.l === 2 ? css("--friendly") : pal.fg;
        mark(id, m.k === 2 ? { ore: m.ore } : UPGRADE_GLYPH[m.k] ?? "gear", colour);
      });
    }
  }

  if (layers.cyno && live.cyno?.length) {
    for (const id of live.cyno) {
      mark(id, "crosshair-simple", pal.warning);
    }
  }

  if (layers.jove) {
    for (const n of geo.nodes) {
      if (n.j) mark(n.i, "cell-tower", pal.muted);
    }
  }

  // Noted systems: one tag glyph per tag in its colour, capped as the app caps it, and a note glyph
  // when any folder has a note.
  if (layers.notes) {
    for (const [id, m] of Object.entries(state.snapshot?.notes?.view?.systems ?? {})) {
      for (const t of m.tags.map(tagById).filter(Boolean).slice(0, 5)) mark(Number(id), "tag", rgb(t.color));
      if (m.parts.some((p) => p.note)) mark(Number(id), "note", pal.muted);
    }
  }

  if (layers.camps && live.camps?.length) {
    for (const id of live.camps) {
      const n = geo.nodes[geo.byId.get(id)];
      if (!n) continue;
      const px = sx(n.x), py = sy(n.z);
      if (!onScreen(px, py)) continue;
      // The ring stays as well: a campfire glyph alone disappears against a dense field, and the
      // ring is what carries at a glance.
      ctx.strokeStyle = pal.hostile;
      ctx.lineWidth = 2;
      ctx.beginPath();
      ctx.arc(px, py, r * 2, 0, Math.PI * 2);
      ctx.stroke();
      mark(id, "campfire", pal.hostile);
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

  // Markers only while names are up: without a name a marker annotates an anonymous dot, and
  // thousands of them hide the map.
  for (const [id, list] of (namesOn ? marks : [])) {
    const n = geo.nodes[geo.byId.get(id)];
    if (!n) continue;
    const px = sx(n.x), py = sy(n.z);
    if (!onScreen(px, py)) continue;
    const step = ICON + 3;
    const y = py - r - ICON / 2 - 4;
    let x = px - ((list.length - 1) * step) / 2;
    for (const [what, colour] of list) {
      if (typeof what === "object") {
        const img = oreIcon(what.ore);
        if (img) ctx.drawImage(img, x - ICON / 2, y - ICON / 2, ICON, ICON);
      } else if (!glyph(ctx, what, x, y, ICON, colour)) {
        // The font has not loaded yet; a filled dot beats a box.
        ctx.fillStyle = colour;
        ctx.beginPath();
        ctx.arc(x, y, ICON / 5, 0, Math.PI * 2);
        ctx.fill();
      }
      x += step;
    }
  }

  // Zoomed in, system names. Zoomed out, region names, as the app shows, since those are what is
  // legible at that scale.
  if (layers.labels) {
    if (namesOn) {
      const size = nameSize;
      ctx.fillStyle = pal.muted;
      ctx.font = `${size}px system-ui, sans-serif`;
      ctx.textBaseline = "middle";
      // Bigger type collides more, so names are placed rather than just drawn: one that would land
      // on a name already down is skipped. Nearest the centre of the view first, so what survives
      // is what the user is looking at.
      const cx = view.ox + (w / 2) * view.k;
      const cz = view.oz + (h / 2) * view.k;
      const near = geo.nodes
        .filter((n) => onScreen(sx(n.x), sy(n.z)))
        .sort(
          (a, b) =>
            (a.x - cx) ** 2 + (a.z - cz) ** 2 - ((b.x - cx) ** 2 + (b.z - cz) ** 2)
        );
      const placed = [];
      for (const n of near) {
        const px = sx(n.x) + r + 4;
        const py = sy(n.z);
        const tw = ctx.measureText(n.n).width;
        const box = [px, py - size / 2, px + tw, py + size / 2];
        if (placed.some((q) => box[0] < q[2] && box[2] > q[0] && box[1] < q[3] && box[3] > q[1])) {
          continue;
        }
        placed.push(box);
        ctx.fillText(n.n, px, py);
      }
    } else {
      ctx.font = `600 ${Math.round(13 * uiScale)}px system-ui, sans-serif`;
      ctx.textAlign = "center";
      ctx.textBaseline = "middle";
      // Sixty-seven region names at full zoom-out overlap into mush, so a label is only drawn where
      // it does not collide with one already placed. Biggest regions first, so the ones that survive
      // are the ones with the most map under them.
      const placed = [];
      for (const [, rg] of regionCentroids()) {
        const px = sx(rg.x), py = sy(rg.z);
        if (!onScreen(px, py)) continue;
        const half = ctx.measureText(rg.n).width / 2 + 3;
        const lh = 9 * uiScale;
        const box = [px - half, py - lh, px + half, py + lh];
        if (placed.some((q) => box[0] < q[2] && box[2] > q[0] && box[1] < q[3] && box[3] > q[1])) {
          continue;
        }
        placed.push(box);
        // Drawn over a dense field of system dots, so the name needs its own ground to sit on.
        ctx.lineWidth = 3;
        ctx.strokeStyle = pal.bg;
        ctx.strokeText(rg.n, px, py);
        ctx.fillStyle = pal.fg;
        ctx.fillText(rg.n, px, py);
      }
      ctx.textAlign = "left";
      ctx.lineWidth = 1;
    }
  }

  // The route, on top of everything it overrides. A leg between gate neighbours is solid; anything
  // else is a bridge or a hole, so it takes the bridge arch or a dash.
  const route = planning() ? [] : live.route ?? [];
  if (route.length > 1) {
    const gate = new Set();
    for (const [a, b] of geo.edges) {
      gate.add(`${geo.nodes[a].i},${geo.nodes[b].i}`);
      gate.add(`${geo.nodes[b].i},${geo.nodes[a].i}`);
    }
    const bridge = new Set();
    for (const [a, b] of geo.bridges ?? []) {
      bridge.add(`${geo.nodes[a].i},${geo.nodes[b].i}`);
      bridge.add(`${geo.nodes[b].i},${geo.nodes[a].i}`);
    }
    ctx.lineWidth = 2.5;
    for (let i = 1; i < route.length; i++) {
      const p = geo.nodes[geo.byId.get(route[i - 1])];
      const q = geo.nodes[geo.byId.get(route[i])];
      if (!p || !q) continue;
      const px = sx(p.x), py = sy(p.z), qx = sx(q.x), qy = sy(q.z);
      const key = `${p.i},${q.i}`;
      ctx.beginPath();
      if (gate.has(key)) {
        ctx.strokeStyle = ROUTE_CYAN;
        ctx.setLineDash([]);
        ctx.moveTo(px, py);
        ctx.lineTo(qx, qy);
      } else if (bridge.has(key)) {
        ctx.strokeStyle = ROUTE_BRIDGE;
        ctx.setLineDash([]);
        arc(ctx, px, py, qx, qy);
      } else {
        ctx.strokeStyle = ROUTE_HOLE;
        ctx.setLineDash([7, 5]);
        ctx.moveTo(px, py);
        ctx.lineTo(qx, qy);
      }
      ctx.stroke();
    }
    ctx.setLineDash([]);
  }

  // A route the user picked from the drag menu, on top of everything.
  if (picked?.path?.length > 1) {
    ctx.lineWidth = 3;
    // Animated dashes, like the app's own route: a static line is hard to pick out of a map already
    // full of lines, and the crawl says which way round the route runs.
    ctx.setLineDash(DASH);
    ctx.lineDashOffset = -((performance.now() / 45) % (DASH[0] + DASH[1]));
    for (let i = 1; i < picked.path.length; i++) {
      const p = geo.nodes[geo.byId.get(picked.path[i - 1])];
      const q = geo.nodes[geo.byId.get(picked.path[i])];
      if (!p || !q) continue;
      const px = sx(p.x), py = sy(p.z), qx = sx(q.x), qy = sy(q.z);
      const kind = picked.hops?.[i]?.kind ?? 0;
      ctx.strokeStyle = kind === 2 ? PICK_JUMP : kind === 1 ? ROUTE_BRIDGE : PICK_GATE;
      ctx.beginPath();
      // Bridges and jumps arc, as elsewhere on the map, so neither reads as a gate.
      if (kind === 2 || kind === 1) arc(ctx, px, py, qx, qy);
      else {
        ctx.moveTo(px, py);
        ctx.lineTo(qx, qy);
      }
      ctx.stroke();
    }
    // The titan's own jump, which the fleet does not fly: a long dash running the other way, so it
    // reads as a second ship moving rather than as part of the route.
    const tj = picked.titan_jump;
    if (tj) {
      const a = geo.nodes[geo.byId.get(tj.from)];
      const b = geo.nodes[geo.byId.get(tj.to)];
      if (a && b) {
        ctx.strokeStyle = TITAN_COL;
        ctx.lineWidth = 2.5;
        ctx.setLineDash([12, 8]);
        // Negative, like the route's: a positive offset runs the dashes backwards.
        ctx.lineDashOffset = -((performance.now() / 35) % 20);
        ctx.beginPath();
        arc(ctx, sx(a.x), sy(a.z), sx(b.x), sy(b.z));
        ctx.stroke();
      }
    }
    ctx.setLineDash([]);
    ctx.lineDashOffset = 0;
    // The systems the user named, as opposed to the ones the route happens to pass through. Two
    // rings and a tint: one thin ring is lost among the highlight rings this map already draws for
    // your characters, camps and the hovered system.
    for (const h of picked.hops ?? []) {
      if (!h.anchor) continue;
      const n = geo.nodes[geo.byId.get(h.id)];
      if (!n) continue;
      const px = sx(n.x), py = sy(n.z);
      if (!onScreen(px, py)) continue;
      ctx.fillStyle = PICK_GATE;
      ctx.globalAlpha = 0.18;
      ctx.beginPath();
      ctx.arc(px, py, r * 4.4, 0, Math.PI * 2);
      ctx.fill();
      ctx.globalAlpha = 1;
      ctx.strokeStyle = PICK_GATE;
      ctx.lineWidth = 2.5;
      ctx.beginPath();
      ctx.arc(px, py, r * 4.4, 0, Math.PI * 2);
      ctx.stroke();
      ctx.lineWidth = 1.2;
      ctx.beginPath();
      ctx.arc(px, py, r * 2.6, 0, Math.PI * 2);
      ctx.stroke();
    }
    // A crawling dash is an animation, so the map keeps painting while one is on screen.
    schedule();
  }

  // The titans. Drawn whether or not the route currently goes near them, because "where are the
  // ships" is the question the titan route is asking and an unused one is still an answer.
  if (routeKind === "titan" && titansOnce.size) {
    for (const id of titansOnce) {
      const n = geo.nodes[geo.byId.get(id)];
      if (!n) continue;
      const px = sx(n.x), py = sy(n.z);
      if (!onScreen(px, py)) continue;
      ctx.fillStyle = TITAN_COL;
      ctx.globalAlpha = 0.22;
      ctx.beginPath();
      ctx.arc(px, py, r * 5, 0, Math.PI * 2);
      ctx.fill();
      ctx.globalAlpha = 1;
      glyph(ctx, "star-four", px, py - r * 5 - 2, Math.round(14 * grow), TITAN_COL);
    }
  }

  // Avoided systems, only while a route is being planned: a long avoid list would otherwise mark
  // much of the map.
  if (routeKind) {
    const always = alwaysAvoided();
    ctx.strokeStyle = pal.hostile;
    ctx.lineWidth = 1.6;
    for (const id of new Set([...always, ...avoidOnce])) {
      const n = geo.nodes[geo.byId.get(id)];
      if (!n) continue;
      const px = sx(n.x), py = sy(n.z);
      if (!onScreen(px, py)) continue;
      // A cross, not a ring: a ring is what this map uses for "look here", and this is the opposite.
      const d = r * 2.2;
      ctx.beginPath();
      ctx.moveTo(px - d, py - d);
      ctx.lineTo(px + d, py + d);
      ctx.moveTo(px + d, py - d);
      ctx.lineTo(px - d, py + d);
      ctx.stroke();
    }
  }

  // A route with a start and nowhere to go yet. Without this the menu's "Start ... Route" looks like
  // it did nothing until a destination is picked.
  if (routeKind && anchors.length === 1) {
    const a = geo.nodes[geo.byId.get(anchors[0])];
    if (a) {
      ctx.strokeStyle = PICK_GATE;
      ctx.lineWidth = 2;
      ctx.setLineDash([4, 3]);
      ctx.beginPath();
      ctx.arc(sx(a.x), sy(a.z), r * 3.8, 0, Math.PI * 2);
      ctx.stroke();
      ctx.setLineDash([]);
    }
  }

  // The drag itself: a line from the system it started on to the pointer, snapped to whatever it is
  // over. Drawn last so nothing covers the thing being aimed.
  if (link) {
    const a = geo.nodes[geo.byId.get(link.from)];
    if (a) {
      const ax = sx(a.x), ay = sy(a.z);
      const t = link.over ? geo.nodes[geo.byId.get(link.over)] : null;
      const bx = t ? sx(t.x) : link.x;
      const by = t ? sy(t.z) : link.y;
      ctx.strokeStyle = pal.accent;
      ctx.lineWidth = t ? 2.5 : 1.5;
      ctx.setLineDash(t ? [] : [5, 4]);
      ctx.beginPath();
      ctx.moveTo(ax, ay);
      ctx.lineTo(bx, by);
      ctx.stroke();
      ctx.setLineDash([]);
      if (t) {
        ctx.beginPath();
        ctx.arc(bx, by, r * 3.4, 0, Math.PI * 2);
        ctx.stroke();
      }
    }
  }

  // Jump range around whatever is being looked at: hovering wins while it lasts, the selection holds
  // it the rest of the time. The app behaves the same way.
  const focus = hovered ?? selected;
  if (layers.jumprange && focus && geo.pos3) {
    const i = geo.byId.get(focus.i);
    const home = geo.pos3[i];
    if (home) {
      // Tint only, no rings: a jump range is a sphere, and a circle on a top-down projection would
      // include systems far above or below it. Distances use the real 3D positions.
      for (let k = 0; k < geo.nodes.length; k++) {
        if (k === i) continue;
        const p = geo.pos3[k];
        const d =
          Math.hypot(p[0] - home[0], p[1] - home[1], p[2] - home[2]) / 100;
        const band = JUMP_RANGES.findIndex(([, ly]) => d <= ly);
        if (band < 0) continue;
        const n = geo.nodes[k];
        const px = sx(n.x), py = sy(n.z);
        if (!onScreen(px, py)) continue;
        ctx.fillStyle = RANGE_COLOURS[band];
        ctx.globalAlpha = 0.7;
        ctx.beginPath();
        ctx.arc(px, py, r + 2, 0, Math.PI * 2);
        ctx.fill();
        ctx.globalAlpha = 1;
      }
    }
  }

  // The system under the pointer, or the selected one: a ring, and its name where it can be read. A
  // cursor change alone is easy to miss on a dense map, and on a touch screen there is no cursor.
  if (focus) {
    const hx = sx(focus.x);
    const hy = sy(focus.z);
    ctx.strokeStyle = pal.fg;
    ctx.lineWidth = 1.5;
    ctx.beginPath();
    ctx.arc(hx, hy, r * 3.4, 0, Math.PI * 2);
    ctx.stroke();

    const info = status[focus.i] ?? {};
    const bits = [focus.n, focus.s.toFixed(1)];
    if (info.adm != null) bits.push(`ADM ${info.adm.toFixed(1)}`);
    const text = bits.join("  ");
    ctx.font = "12px system-ui, sans-serif";
    ctx.textBaseline = "middle";
    const tw = ctx.measureText(text).width;
    // Flip to the other side rather than running off the edge.
    const tx = hx + r * 4 + tw + 10 > w ? hx - r * 4 - tw - 8 : hx + r * 4;
    const ty = Math.max(12, Math.min(h - 12, hy));
    ctx.fillStyle = pal.surface ?? css("--surface");
    ctx.globalAlpha = 0.92;
    ctx.fillRect(tx - 4, ty - 10, tw + 8, 20);
    ctx.globalAlpha = 1;
    ctx.strokeStyle = pal.line;
    ctx.lineWidth = 1;
    ctx.strokeRect(tx - 4, ty - 10, tw + 8, 20);
    ctx.fillStyle = pal.fg;
    ctx.fillText(text, tx, ty);
  }

  const hint = el?.querySelector(".maphint");
  if (hint) {
    hint.textContent = focused
      ? `${drawn} shown, region view`
      : `${drawn} of ${geo.nodes.length} systems`;
  }
}

const TOGGLES = [
  ["jumprange", "Jump range"],
  ["adm", "ADM"],
  ["bridges", "Bridges"],
  ["holes", "Wormholes"],
  ["camps", "Camps"],
  ["cyno", "Cyno"],
  ["upgrades", "Upgrades"],
  ["jove", "Jove"],
  ["notes", "Notes"],
  ["labels", "Labels"],
];

const CYCLES = [
  ["sov", "Sov", ["off", "alliance", "coalition"], SOV_LABEL],
  ["activity", "Activity", ["off", "k", "p", "n", "j"], ACTIVITY_LABEL],
];

/// The layers in four groups, each a button opening its switches: eleven controls in one row take
/// most of a phone screen.
const GROUPS = [
  ["sov", "Sov", ["sov"], ["adm", "upgrades"]],
  ["activity", "Activity", ["activity"], ["camps", "cyno"]],
  ["travel", "Travel", [], ["bridges", "holes", "jumprange"]],
  ["marks", "Marks", [], ["labels", "jove", "notes"]],
];

const TOGGLE_LABEL = Object.fromEntries(TOGGLES);

/// Whether anything in a group is doing something, so the button can say so without being opened.
function groupActive(cycles, toggles) {
  return cycles.some((k) => layers[k] && layers[k] !== "off") || toggles.some((k) => layers[k]);
}

function layerGroups() {
  return GROUPS.map(([id, label, cycles, toggles]) => {
    const body =
      cycles
        .map((key) => {
          const [, name, , names] = CYCLES.find(([k]) => k === key);
          return (
            `<div class="mlrow"><span>${name}</span>` +
            `<button class="mlcycle" data-cycle="${key}">${names[layers[key]]}</button></div>`
          );
        })
        .join("") +
      `<div class="mlgrid">` +
      toggles
        .map(
          (k) =>
            `<button class="ml${layers[k] ? " on" : ""}" data-layer="${k}">${TOGGLE_LABEL[k]}</button>`
        )
        .join("") +
      `</div>`;
    return (
      `<span class="mlgroup">` +
      `<button class="mlhead${groupActive(cycles, toggles) ? " on" : ""}" data-pop="${id}">${label}</button>` +
      `<div class="mlpop" data-panel="${id}" hidden>${body}</div>` +
      `</span>`
    );
  }).join("");
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
    layerGroups() +
    `<button class="mlhead" data-notes-manage="system" title="System tags and notes">${ico("tag")} Tags</button>` +
    `<span class="maphint"></span>` +
    `</div>` +
    // The canvas and the route dock share a row, so docking takes space from the map rather than
    // sitting on top of it. Which way the row runs is decided by the pane's shape.
    `<div class="maprow"><div class="mapwrap"><canvas class="starmap"></canvas></div></div>`;

  canvas = el.querySelector("canvas");
  ctx = canvas.getContext("2d");
  pal = null;
  if (!fitted) fit();
  wire();
  dockSide();
  // A pane switched on later arrives with no size, so watch the canvas to fit once it has one.
  new ResizeObserver(() => {
    if (!fitted) fit();
    schedule();
  }).observe(canvas);
  schedule();
  // The system window parks against the canvas, which does not exist until the geometry is fetched.
  window.dispatchEvent(new Event("spai:map"));
}

function wire() {
  // A `#system/<id>` link highlights that system on the map as well as opening its dialog, so a link
  // someone sends points at something visible rather than just naming it.
  const followHash = () => {
    const m = /^#system\/(\d+)$/.exec(location.hash);
    const n = m ? geo.nodes[geo.byId.get(Number(m[1]))] : null;
    if (n !== selected) {
      selected = n ?? null;
      schedule();
    }
  };
  window.addEventListener("hashchange", followHash);
  followHash();

  // Read the binding first: a route opened from a deep link was announced before this listener existed.
  picked = currentRoute;
  window.addEventListener("spai:route", (e) => {
    picked = e.detail ?? null;
    schedule();
  });

  // The popups live in the body, as the context menu does: a scrolling pane clips whatever hangs out
  // of it.
  document.querySelectorAll("body > .mlpop").forEach((p) => p.remove());
  el.querySelectorAll(".mlpop").forEach((p) => document.body.append(p));

  /// Show one group's popup and close the others. `null` closes them all. A popup stays open while
  /// its switches are used, since layers are usually set several at once.
  const openGroup = (id) => {
    document.querySelectorAll(".mlpop").forEach((p) => {
      p.hidden = p.dataset.panel !== id;
    });
    el.querySelectorAll(".mlhead").forEach((h) => {
      h.classList.toggle("open", h.dataset.pop === id);
    });
    if (!id) return;
    const btn = el.querySelector(`.mlhead[data-pop="${id}"]`);
    const pop = document.querySelector(`.mlpop[data-panel="${id}"]`);
    if (!btn || !pop || getComputedStyle(pop).bottom !== "auto") return;
    // Measured after it is shown and clamped to the viewport, so a group at the right-hand end of a
    // narrow pane opens back onto the screen rather than off it.
    const b = btn.getBoundingClientRect();
    const p = pop.getBoundingClientRect();
    pop.style.left = `${Math.max(4, Math.min(b.left, window.innerWidth - p.width - 4))}px`;
    pop.style.top = `${Math.max(4, Math.min(b.bottom + 4, window.innerHeight - p.height - 4))}px`;
  };
  // `#maplayers` opens the first group on load, for the same reason the dialogs and the layout menu
  // take deep links: the harness cannot click.
  if (location.hash === "#maplayers") openGroup(GROUPS[0][0]);
  el.querySelectorAll("[data-pop]").forEach((b) =>
    b.addEventListener("click", (e) => {
      e.stopPropagation();
      const already = !document.querySelector(`.mlpop[data-panel="${b.dataset.pop}"]`)?.hidden;
      openGroup(already ? null : b.dataset.pop);
    })
  );
  document.addEventListener("click", (e) => {
    if (!e.target.closest(".maptools") && !e.target.closest(".mlpop")) openGroup(null);
  });

  document.querySelectorAll(".mlpop [data-layer]").forEach((b) =>
    b.addEventListener("click", () => {
      layers[b.dataset.layer] = !layers[b.dataset.layer];
      b.classList.toggle("on", layers[b.dataset.layer]);
      markGroups();
      saveLayers();
      schedule();
    })
  );
  document.querySelectorAll(".mlpop [data-cycle]").forEach((b) =>
    b.addEventListener("click", () => {
      const [, , order, names] = CYCLES.find(([k]) => k === b.dataset.cycle);
      const next = order[(order.indexOf(layers[b.dataset.cycle]) + 1) % order.length];
      layers[b.dataset.cycle] = next;
      b.textContent = names[next];
      markGroups();
      saveLayers();
      schedule();
    })
  );

  const pointers = new Map();
  let pinch = null;
  let moved = 0;
  let press = null;
  /// A long press ends with a finger coming off the glass, and the browser sends a click for that.
  /// Without this the menu opens and the tap underneath it opens the system window as well.
  let swallowClick = false;

  const zoomAt = (factor, px, py) => {
    // Clamped to the app's own range, and never tighter than wherever a region fit already put the
    // view: framing a small region is allowed to go past the limit, and being unable to zoom back
    // out of it would not be.
    const lo = Math.min(view.k, (fitK || geo.extent / canvas.clientWidth) / ZOOM_MAX);
    const hi = Math.max(view.k, (fitK || geo.extent / canvas.clientWidth) / ZOOM_MIN);
    const k = Math.max(lo, Math.min(hi, target.k * factor));
    // Everything here and in `settle` is derived from the map point under the cursor, so it cannot
    // drift.
    const ax = view.ox + px * view.k;
    const az = view.oz + py * view.k;
    target.ox = ax - px * k;
    target.oz = az - py * k;
    target.k = k;
    const dur = Math.min(ZOOM_EASE_MS, ZOOM_EASE_PER_LN * Math.abs(Math.log(k / view.k)));
    anim = { ax, az, px, py, k0: view.k, dur: Math.max(1, dur), t0: performance.now() };
    schedule();
  };

  canvas.addEventListener("pointerdown", (e) => {
    canvas.setPointerCapture(e.pointerId);
    pointers.set(e.pointerId, e);
    moved = 0;
    // A second finger is a pinch and cancels a route drag, which would otherwise swallow the zoom.
    if (pointers.size > 1) {
      link = null;
      hideLinkTip();
      clearTimeout(press);
      schedule();
      return;
    }
    const touch = e.pointerType === "touch";
    const on = nearest(e);
    clearTimeout(press);
    if (on && touch) {
      press = setTimeout(() => {
        if (moved > 8) return;
        link = null;
        hideLinkTip();
        swallowClick = true;
        schedule();
        openMenu(e, on);
      }, 500);
    }
    // On touch, a drag off a system draws a route line only once a route exists, or every pan that
    // starts on a system would draw one. A long press starts a route there.
    if (on && (!touch || routeKind)) {
      const box = canvas.getBoundingClientRect();
      link = {
        from: on.i,
        x: e.clientX - box.left,
        y: e.clientY - box.top,
        over: null,
        reach: reach(geo, geo.byId.get(on.i)),
      };
      canvas.style.cursor = "crosshair";
      schedule();
      return;
    }
    canvas.style.cursor = "grabbing";
  });
  canvas.addEventListener("pointermove", (e) => {
    if (!pointers.has(e.pointerId)) return;
    const prev = pointers.get(e.pointerId);
    pointers.set(e.pointerId, e);
    if (link) {
      if (moved > 8) clearTimeout(press);
      const box = canvas.getBoundingClientRect();
      link.x = e.clientX - box.left;
      link.y = e.clientY - box.top;
      const t = nearest(e);
      link.over = t && t.i !== link.from ? t.i : null;
      moved += 4;
      showLinkTip();
      schedule();
      return;
    }
    if (pointers.size === 2) {
      const [a, b] = [...pointers.values()];
      const dist = Math.hypot(a.clientX - b.clientX, a.clientY - b.clientY);
      const box = canvas.getBoundingClientRect();
      const mx = (a.clientX + b.clientX) / 2 - box.left;
      const my = (a.clientY + b.clientY) / 2 - box.top;
      if (pinch && dist > 0) {
        // Two fingers pan as well as pinch. The midpoint moving is a drag, and a pinch that only
        // scales leaves the map sliding out from under the hand doing it.
        const dx = (mx - pinch.mx) * view.k;
        const dz = (my - pinch.my) * view.k;
        view.ox -= dx;
        view.oz -= dz;
        target.ox -= dx;
        target.oz -= dz;
        if (anim) {
          anim.ax -= dx;
          anim.az -= dz;
        }
        // Counts as movement, or lifting off after a pinch lands as a tap and selects whatever was
        // under the last finger.
        moved += Math.abs(mx - pinch.mx) + Math.abs(my - pinch.my) + Math.abs(dist - pinch.d);
        zoomAt(pinch.d / dist, mx, my);
      }
      pinch = { d: dist, mx, my };
      return;
    }
    moved += Math.abs(e.clientX - prev.clientX) + Math.abs(e.clientY - prev.clientY);
    const dx = (e.clientX - prev.clientX) * view.k;
    const dz = (e.clientY - prev.clientY) * view.k;
    view.ox -= dx;
    view.oz -= dz;
    target.ox -= dx;
    target.oz -= dz;
    // The zoom in flight is pinned to a map point, so a drag has to move the pin with it or the
    // next frame drags the map back under the finger.
    if (anim) {
      anim.ax -= dx;
      anim.az -= dz;
    }
    schedule();
  });
  const up = (e) => {
    clearTimeout(press);
    pointers.delete(e.pointerId);
    if (pointers.size < 2) pinch = null;
    if (link) {
      const { from, over } = link;
      link = null;
      hideLinkTip();
      canvas.style.cursor = "";
      schedule();
      if (over != null) {
        // Off a system already on the route, the route is rewritten from there: everything after it
        // goes and the new target becomes the destination. Off the destination that is the same as
        // appending. Anywhere else is a new route, which is the only way to abandon one.
        const at = anchors.indexOf(from);
        const extend = anchors.length > 1 && at >= 0 && routeKind;
        const take = (kind) => {
          routeKind = kind;
          anchors = extend ? [...anchors.slice(0, at + 1), over] : [from, over];
          if (kind === "gate") send({ SetDestination: { id: over } });
          replan();
        };
        // The menu asks the route kind once per route, so extending a route skips it.
        if (extend) take(routeKind);
        else radial(e.clientX, e.clientY, take);
        return;
      }
    }
    if (!pointers.size) {
      // A tap on a touch screen leaves the system under the finger highlighted, which is the only
      // hover feedback a touch screen can give.
      const n = nearest(e);
      hovered = n;
      canvas.style.cursor = n ? "pointer" : "grab";
      schedule();
    }
  };
  canvas.addEventListener("pointerup", up);
  canvas.addEventListener("pointercancel", up);

  // Hover is tracked separately from the drag handler so a pan does not fight it.
  canvas.addEventListener("pointermove", (e) => {
    if (pointers.size) return; // dragging: the grab cursor is the right answer
    const n = nearest(e);
    // Set inline because the stylesheet's `grab` would otherwise win.
    canvas.style.cursor = n ? "pointer" : "grab";
    if (n !== hovered) {
      hovered = n;
      schedule();
    }
  });
  canvas.addEventListener("pointerleave", () => {
    canvas.style.cursor = "";
    if (hovered) {
      hovered = null;
      schedule();
    }
  });

  // Right-click on a desktop, long press on a phone. The press timer is cancelled by the drag, so a
  // press that turns into a route line never also opens a menu.
  canvas.addEventListener("contextmenu", (e) => {
    const n = nearest(e);
    if (!n) return;
    e.preventDefault();
    link = null;
    hideLinkTip();
    schedule();
    openMenu(e, n);
  });

  canvas.addEventListener(
    "wheel",
    (e) => {
      e.preventDefault();
      const box = canvas.getBoundingClientRect();
      // The app's curve, `zoom * exp(scroll * 0.003)`, continuous rather than a step per notch.
      // Normalised to pixels first, since Firefox reports lines and Chrome pixels.
      const unit = e.deltaMode === 1 ? 16 : e.deltaMode === 2 ? 100 : 1;
      const px = Math.max(-240, Math.min(240, e.deltaY * unit));
      // `k` is map units per pixel, so zooming in makes it smaller. A positive `deltaY` is a scroll
      // away from the user, which is zoom out, which is a larger `k`.
      zoomAt(Math.exp(px * 0.003), e.clientX - box.left, e.clientY - box.top);
    },
    { passive: false }
  );

  // Double-click frames the region, as the app's region view does.
  canvas.addEventListener("dblclick", (e) => {
    const n = nearest(e);
    if (!n) return;
    focused = n.r;
    fit(n.r);
    schedule();
  });

  canvas.addEventListener("click", (e) => {
    if (swallowClick) {
      swallowClick = false;
      return;
    }
    if (moved > 6) return;
    const box = canvas.getBoundingClientRect();
    const mx = e.clientX - box.left;
    const my = e.clientY - box.top;
    const best = nearest(e);
    if (!best) return;
    location.hash = `#system/${best.i}`;
    // The desktop map follows, so the phone and the machine look at the same place.
    send({ SelectSystem: { id: best.i } });
  });

  if (window.ResizeObserver) {
    new ResizeObserver(() => {
      pal = null;
      dockSide();
      schedule();
    }).observe(el);
  }
}

/// The readout beside the system a route drag is over: light years, gates, and gates with bridges.
/// A DOM node rather than canvas text, so it stays readable over a dense field.
function showLinkTip() {
  if (!link?.over) return hideLinkTip();
  let tip = document.getElementById("linktip");
  if (!tip) {
    tip = document.createElement("div");
    tip.id = "linktip";
    document.body.append(tip);
  }
  const k = geo.byId.get(link.over);
  const t = k == null ? null : geo.nodes[k];
  // The geometry can be replaced while a drag is in flight, and then the id under the finger is not
  // in the new index. Everything below reads the node's position, so there is nothing to place.
  if (!t) return hideLinkTip();
  const ly = lightYears(geo, geo.byId.get(link.from), k);
  const g = link.reach.gates[k];
  const b = link.reach.bridged[k];
  const jumps = (n) => (n < 0 ? "no route" : n === 1 ? "1 jump" : `${n} jumps`);
  tip.innerHTML =
    `<b>${t?.n ?? ""}</b>` +
    `<span>${ly == null ? "" : `${ly.toFixed(1)} ly`}</span>` +
    `<span>${jumps(g)} by gate</span>` +
    (b >= 0 && b !== g ? `<span>${jumps(b)} with bridges</span>` : "");
  const box = canvas.getBoundingClientRect();
  tip.style.left = `${box.left + sx(t.x) + 14}px`;
  tip.style.top = `${box.top + sy(t.z) + 14}px`;
  tip.hidden = false;
}

function hideLinkTip() {
  const tip = document.getElementById("linktip");
  if (tip) tip.hidden = true;
}

/// The permanent avoid list for the kind of route being planned. Gate and titan routes both fly
/// gates, so they read the same list.
function alwaysAvoided() {
  const m = state.snapshot?.meta;
  return new Set((routeKind === "jump" ? m?.avoid_jump : m?.avoid_gate) ?? []);
}

/// Plan the route the anchors currently describe, and draw it.
function replan() {
  if (!routeKind || anchors.length < 2) {
    picked = null;
    schedule();
    return;
  }
  showRoute(routeKind, anchors);
}

/// What the context menu offers for one system, which depends entirely on whether a route is being
/// built and whether this system is already part of it.
/// Whether this page is drawing a route of its own.
function planning() {
  return !!routeKind && anchors.length > 0;
}

function menuFor(id) {
  const at = anchors.indexOf(id);
  const items = [];
  if (state.snapshot?.meta?.allow_writeback) {
    items.push(["setdest", "Set Destination"]);
    items.push(null);
  }
  if (routeKind && anchors.length) {
    if (at < 0) {
      items.push(["dest", "Set as Destination"]);
      if (anchors.length > 1) items.push(["way", "Add Waypoint"]);
    } else if (at > 0) {
      items.push(["drop", at === anchors.length - 1 ? "Remove Destination" : "Remove Waypoint"]);
    }
    if (routeKind === "titan") {
      // Any system, not only ones on the route: where the ships are is the question the titan route
      // is asking, and the answer is often nowhere near the path it currently takes.
      items.push([
        titansOnce.has(id) ? "untitan" : "titan",
        titansOnce.has(id) ? "Not a titan system" : "Set as titan system",
      ]);
    }
    items.push(["clear", "Clear Route", "warn"]);
    items.push(null);
    // Avoidance only means something while a route is being planned, and which list it lands in
    // depends on what kind of route that is: a system you will not gate through is often perfectly
    // fine to jump over.
    items.push([avoidOnce.has(id) ? "unavoid" : "avoid", avoidOnce.has(id) ? "Stop avoiding here" : "Avoid for this route"]);
    // "Stop avoiding always" needs the app's list, which the snapshot carries.
    if (alwaysAvoided().has(id)) items.push(["avoid:never", "Stop avoiding always"]);
    else items.push(["avoid:always", "Avoid always"]);
    items.push(null);
  }
  const verb = routeKind && anchors.length ? "Restart as" : "Start";
  items.push(["start:gate", `${verb} Gate Route`]);
  items.push(["start:jump", `${verb} Jump Route`]);
  items.push(["start:titan", `${verb} Titan Route`]);
  items.push(null);
  if (state.snapshot?.meta?.allow_writeback) {
    items.push(["notes:tags", "Tags…"]);
    items.push(["notes:edit", "Notes and tags…"]);
  }
  items.push(["info", "Show info"]);
  items.push(["focus", "Show in the app"]);
  return items;
}

function menuPick(id, kind) {
  const at = anchors.indexOf(id);
  if (kind.startsWith("start:")) {
    routeKind = kind.slice(6);
    anchors = [id];
    picked = null;
    avoidOnce.clear();
    titansOnce.clear();
    schedule();
    return;
  }
  switch (kind) {
    case "setdest":
      send({ SetDestination: { id } });
      return;
    case "dest":
      // One anchor is a start with nowhere to go, so this completes it; more than one replaces the
      // destination and leaves the waypoints where they are.
      anchors = anchors.length <= 1 ? [...anchors, id] : [...anchors.slice(0, -1), id];
      if (routeKind === "gate") send({ SetDestination: { id } });
      break;
    case "way":
      anchors = [...anchors.slice(0, -1), id, anchors[anchors.length - 1]];
      break;
    case "drop":
      anchors = anchors.filter((_, i) => i !== at);
      break;
    case "titan":
      titansOnce.add(id);
      break;
    case "untitan":
      titansOnce.delete(id);
      break;
    case "avoid":
      avoidOnce.add(id);
      break;
    case "unavoid":
      avoidOnce.delete(id);
      break;
    case "avoid:always":
      send({ AvoidSystem: { id, jump: routeKind === "jump", on: true } });
      break;
    case "avoid:never":
      send({ AvoidSystem: { id, jump: routeKind === "jump", on: false } });
      break;
    case "clear":
      anchors = [];
      routeKind = null;
      picked = null;
      avoidOnce.clear();
      titansOnce.clear();
      schedule();
      return;
    case "info":
      location.hash = `#system/${id}`;
      return;
    case "focus":
      send({ SelectSystem: { id } });
      return;
    case "notes:tags":
      quickMenu(menuAt.x, menuAt.y, { System: id }, nodeName(id));
      return;
    case "notes:edit":
      openEditor({ System: id }, nodeName(id));
      return;
  }
  replan();
}

/// Where the last menu opened, so a menu picked from it can open the next one in the same place.
const menuAt = { x: 0, y: 0 };

const nodeName = (id) => geo?.nodes[geo.byId.get(id)]?.n ?? String(id);

function openMenu(e, n) {
  menuAt.x = e.clientX;
  menuAt.y = e.clientY;
  menu(e.clientX, e.clientY, menuFor(n.i), (kind) => menuPick(n.i, kind));
}

/// Light the group buttons whose layers are doing something, so the row says what is on without
/// being opened.
function markGroups() {
  for (const [id, , cycles, toggles] of GROUPS) {
    el?.querySelector(`.mlhead[data-pop="${id}"]`)?.classList.toggle(
      "on",
      groupActive(cycles, toggles)
    );
  }
}

/// Which edge the route dock takes: the long one, so neither the map nor the list ends up a sliver.
function dockSide() {
  if (!el) return;
  const w = el.clientWidth;
  const h = el.clientHeight;
  el.classList.toggle("dockright", w >= h * 1.25 && w >= 620);
}

/// Nearest system to a pointer event, within a thumb's reach, standing in for the browser's hit
/// testing.
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
