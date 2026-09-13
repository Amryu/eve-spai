// Dialogs render here, in the browser, not as a window on the desktop. A tap on a phone must not
// raise a viewport on a machine in another room, which is why a badge tap is a plain GET and never
// an IntelClick.

import { ico, state } from "./app.js";
import { menu } from "./route.js";
import { card, fmtAge } from "./panes-intel.js";

const esc = (s) =>
  String(s ?? "").replace(/[&<>"']/g, (c) =>
    ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]
  );

const secVar = (sec) => `var(--sec-${Math.min(10, Math.max(0, Math.round(sec * 10)))})`;

/// One window per kind of thing, not one window reused.
///
/// A ship opened from an intel card was landing in the system window, on top of the map, replacing
/// whatever system was being read. They are different things, opened from different places, and
/// looking at a hull while looking at where it was seen is the normal case.
const shells = new Map();
/// How far each kind's window is offset from the map's corner, so three open at once cascade rather
/// than hiding each other. Only used while they float.
const CASCADE = { system: 0, ship: 1, pilot: 2, route: 0 };
const TAB_NAME = { route: "Route", system: "System", ship: "Ship", pilot: "Pilot" };

/// The dock in the map pane, or null when there is no map pane to dock into.
///
/// One box with tabs rather than one box per kind: three windows docked side by side would leave the
/// map a sliver, and they are read one at a time anyway. Floating is the fallback, for a page with
/// the map switched off.
function dock() {
  const pane = document.querySelector('#panes [data-pane="map"]');
  // Only while the map pane is actually on screen. Docking into a hidden pane puts the window inside
  // something with `display: none`, so a ship opened from the alerts pane rendered into nothing and
  // looked like a dead click.
  if (!pane || pane.hidden || !pane.offsetParent) return null;
  const row = pane.querySelector(".maprow");
  if (!row) return null;
  let d = row.querySelector(":scope > .rdock");
  if (!d) {
    d = document.createElement("div");
    d.className = "rdock";
    d.hidden = true;
    d.innerHTML = `<div class="dtabs"></div><div class="dbody"></div>`;
    row.append(d);
    d.addEventListener("click", (e) => {
      const x = e.target.closest("[data-shut]");
      if (x) return close(x.dataset.shut);
      const t = e.target.closest("[data-tab-kind]");
      if (t) showTab(t.dataset.tabKind);
    });
  }
  return d;
}

/// Which docked panes are open, in a stable order, and which one is showing.
function paintTabs(active) {
  const d = dock();
  if (!d) return;
  const open = [...shells].filter(([, el]) => el.classList.contains("dpane") && !el.hidden);
  d.hidden = !open.length;
  if (!open.length) return;
  const shown = open.some(([k]) => k === active)
    ? active
    : open.find(([, el]) => el.classList.contains("on"))?.[0] ?? open[0][0];
  for (const [k, el] of open) el.classList.toggle("on", k === shown);
  d.querySelector(".dtabs").innerHTML = open
    .map(
      ([k]) =>
        `<button class="dtab${k === shown ? " on" : ""}" data-tab-kind="${k}">${TAB_NAME[k] ?? k}` +
        `<span class="dshut" data-shut="${k}">${ico("x")}</span></button>`
    )
    .join("");
}

function showTab(kind) {
  paintTabs(kind);
}

/// A floating window, not a modal.
///
/// A modal takes the whole page to show one system, which is wrong for something you open while
/// reading the map: the map is the context. This floats, can be dragged, and does not block anything
/// behind it. Escape and its close button dismiss it; clicking the map does not, because clicking
/// the map is how you open the next one.
function shell(kind) {
  const had = shells.get(kind);
  if (had) {
    // The map pane can be switched off while a window is docked inside it. Float it again rather
    // than leaving it in a box nobody can see.
    if (had.classList.contains("dpane") && !dock()) {
      had.className = "float";
      had.style.cssText = "";
      if (!had.querySelector(".mclose")) {
        had
          .querySelector(".mpanel")
          ?.insertAdjacentHTML(
            "afterbegin",
            `<button class="mclose" aria-label="Close">${ico("x")}</button>`
          );
        had.querySelector(".mclose")?.addEventListener("click", () => close(kind));
      }
      document.body.append(had);
    }
    return had;
  }
  const dlg = document.createElement("div");
  const d = dock();
  dlg.className = d ? "dpane" : "float";
  dlg.dataset.kind = kind;
  dlg.hidden = true;
  dlg.innerHTML = `<div class="mpanel" role="dialog"></div>`;
  (d?.querySelector(".dbody") ?? document.body).append(dlg);
  shells.set(kind, dlg);
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape") close(kind);
  });
  dragify(dlg);
  // A pane that changes size moves the map with it, and the window was parked against where the map
  // used to be. Same for the map arriving after the window did.
  const repark = () => {
    // The map pane may not exist yet when a window is first opened, and these belong inside it. The
    // map says when it is ready; this is what moves a window in at that point.
    const body = dock()?.querySelector(".dbody");
    if (body && dlg.parentElement !== body) {
      dlg.className = "dpane";
      dlg.style.cssText = "";
      // Its own close button came with it from floating, and the tab has one: two × in one corner
      // is one too many.
      dlg.querySelector(".mclose")?.remove();
      body.append(dlg);
      paintTabs(kind);
    }
    if (!dlg.hidden) place(dlg);
  };
  window.addEventListener("resize", repark);
  window.addEventListener("spai:map", repark);
  repark();
  return dlg;
}

/// Drag by the panel's own chrome. Pointer events so a phone can move it too, and clamped on release
/// so it cannot be parked off screen.
function dragify(node) {
  let from = null;
  node.addEventListener("pointerdown", (e) => {
    // Anything interactive keeps the pointer: a drop-down or a number field is dragged sideways to
    // use it, and the window was following the pointer instead of the control.
    if (e.target.closest("button, a, input, select, textarea, label, option")) return;
    const r = node.getBoundingClientRect();
    from = { x: e.clientX, y: e.clientY, left: r.left, top: r.top };
    node.setPointerCapture(e.pointerId);
  });
  node.addEventListener("pointermove", (e) => {
    if (!from) return;
    const w = node.offsetWidth;
    const h = node.offsetHeight;
    const left = Math.min(window.innerWidth - 40, Math.max(40 - w, from.left + e.clientX - from.x));
    const top = Math.min(window.innerHeight - 40, Math.max(0, from.top + e.clientY - from.y));
    node.style.left = `${left}px`;
    node.style.top = `${top}px`;
    node.style.right = "auto";
    node.style.bottom = "auto";
    node.style.transform = "none";
    // Once it has been moved by hand it stays where it was put.
    node.dataset.moved = "1";
  });
  const drop = () => {
    from = null;
  };
  node.addEventListener("pointerup", drop);
  node.addEventListener("pointercancel", drop);
}

export function close(kind = null) {
  for (const [k, d] of shells) {
    if (kind == null || k === kind) d.hidden = true;
  }
  if (kind == null || kind === "route") {
    currentRoute = null;
    window.dispatchEvent(new CustomEvent("spai:route", { detail: null }));
  }
  paintTabs(null);
}

/// Park the window at the top right of the map, below whatever controls are showing.
///
/// The canvas's own top is not low enough: on a narrow pane the layer panel is an overlay sitting
/// over the top of the canvas, so a window aligned to the canvas covers the filters. This clears
/// whichever of the toolbar and the open panel reaches furthest down, so it works whether the panel
/// is in the flow or floating.
function place(d, tries = 10) {
  // A docked window is placed by the layout, not by us.
  if (d.classList.contains("dpane") || d.dataset.moved || d.hidden) return;
  const canvas = document.querySelector(".starmap");
  const r = canvas?.getBoundingClientRect();
  // Nothing to measure: either the map is off screen, which is what it is in tabs mode while another
  // pane is showing, or its canvas has not been drawn yet because the geometry is still on the way.
  //
  // Giving up here is what left the window sitting in the CSS corner *on top of the toolbar it is
  // meant to clear*, permanently, since placement only ever ran once as the window opened. So it
  // comes back and looks again for about a second, which is longer than the geometry takes.
  if (!r || r.width < 60 || r.right < 0 || r.left > window.innerWidth) {
    if (tries > 0) setTimeout(() => place(d, tries - 1), 120);
    return;
  }

  let top = r.top;
  for (const sel of [".maptools", ".mlayers:not([hidden])"]) {
    const box = document.querySelector(sel)?.getBoundingClientRect();
    // Only controls that actually overlap the map: a toolbar above it already sits clear.
    if (box && box.height > 0 && box.bottom > top && box.top < r.bottom) {
      top = box.bottom;
    }
  }
  const step = 18 * (CASCADE[d.dataset.kind] ?? 0);
  d.style.left = "auto";
  d.style.top = `${Math.round(Math.min(top + 8 + step, r.bottom - 80))}px`;
  d.style.right = `${Math.round(window.innerWidth - r.right + 8 + step)}px`;
  d.style.bottom = "auto";
}

function open(kind, html) {
  const d = shell(kind);
  const docked = d.classList.contains("dpane");
  d.querySelector(".mpanel").innerHTML = docked
    ? html
    : `<button class="mclose" aria-label="Close">${ico("x")}</button>${html}`;
  d.querySelector(".mclose")?.addEventListener("click", () => close(kind));
  d.hidden = false;
  if (docked) {
    // Whichever was opened last is the one being read, so it becomes the showing tab.
    paintTabs(kind);
    return;
  }
  // Floating, the newest goes on top of the others instead.
  for (const [k, o] of shells) o.style.zIndex = k === kind ? 52 : 50;
  place(d);
}

function rows(pairs) {
  return pairs
    .filter(([, v]) => v !== null && v !== undefined && v !== "" && v !== false)
    .map(([k, v]) => `<div class="mrow"><span>${k}</span><span>${v}</span></div>`)
    .join("");
}

async function showSystem(id) {
  open("system", `<h3>System</h3><p class="placeholder">Loading.</p>`);
  const r = await fetch(`/api/system/${id}`);
  if (!r.ok) return open("system", `<h3>System</h3><p class="placeholder">Not in the star map.</p>`);
  const s = await r.json();
  const jumps = s.jumps_from_you == null ? "no route" : s.jumps_from_you === 0 ? "you are here" : `${s.jumps_from_you} jumps`;
  open(
    "system",
    `<h3 style="color:${secVar(s.security)}">${ico("planet")} ${esc(s.name)} <small>${s.security.toFixed(1)}</small></h3>` +
      rows([
        ["Region", esc(s.region)],
        ["Constellation", esc(s.constellation)],
        ["Faction", esc(s.faction)],
        ["Sovereignty", esc(s.sov)],
        ["ADM", s.adm == null ? null : s.adm.toFixed(1)],
        ["From you", jumps],
        ["Incursion", s.incursion ? "yes" : null],
        ["Jove observatory", s.jove ? "yes" : null],
        ["Ship kills (1h)", s.ship_kills || null],
        ["Pod kills (1h)", s.pod_kills || null],
        ["NPC kills (1h)", s.npc_kills || null],
      ]) +
      (s.gates.length
        ? `<div class="mrow"><span>Gates</span><span class="mgates">` +
          s.gates
            .map(([gid, name]) => `<button class="chip" data-system="${gid}">${esc(name)}</button>`)
            .join("") +
          `</span></div>`
        : "")
  );
}

async function showShip(id) {
  open("ship", `<h3>Ship</h3><p class="placeholder">Loading.</p>`);
  const r = await fetch(`/api/ship/${id}`);
  if (!r.ok) return open("ship", `<h3>Ship</h3><p class="placeholder">Not in the static data.</p>`);
  const s = await r.json();
  // The damage types carry the app's own colours. A row of four bare percentages says nothing about
  // which hole in a resist profile you are looking at; the colour is how that is read at a glance.
  const DMG = [
    ["EM", "#5aa9e0"],
    ["Th", "#d64545"],
    ["Kin", "#9aa3a8"],
    ["Exp", "#d6a645"],
  ];
  const layer = (name, hp, r, ehp) =>
    hp <= 0
      ? ""
      : `<tr><th>${name}</th><td class="num">${Math.round(hp)}</td>` +
        r
          .map(
            (v, i) =>
              `<td class="rcell"><span class="rbar" style="width:${Math.max(0, Math.min(100, v))}%;background:${DMG[i][1]}"></span><span class="rnum">${v}%</span></td>`
          )
          .join("") +
        `<td class="num">${Math.round(ehp)}</td></tr>`;
  const total = Math.round(s.shield_ehp + s.armor_ehp + s.hull_ehp);
  const hardpoints = [
    s.turrets ? `${s.turrets} turret` : null,
    s.launchers ? `${s.launchers} launcher` : null,
  ].filter(Boolean);
  open(
    "ship",
    `<h3><img class="mhull" src="https://images.evetech.net/types/${s.id}/render?size=128" alt=""> ${esc(s.name)}</h3>` +
      `<p class="mgroup">${esc(s.group)}</p>` +
      (s.roles.length
        ? `<p class="mroles">` +
          s.roles
            .map(([glyph, label]) => `<span class="ph" title="${esc(label)}">${esc(glyph)}</span>`)
            .join("") +
          `</p>`
        : "") +
      `<table class="mresists"><thead><tr><th></th><th class="num">HP</th>` +
      DMG.map(([t, c]) => `<th style="color:${c}">${t}</th>`).join("") +
      `<th class="num">EHP</th></tr></thead><tbody>` +
      layer("Shield", s.shield_hp, s.shield_resist, s.shield_ehp) +
      layer("Armor", s.armor_hp, s.armor_resist, s.armor_ehp) +
      layer("Hull", s.hull_hp, s.hull_resist, s.hull_ehp) +
      `</tbody></table>` +
      `<p class="mtotal">Total EHP ${total.toLocaleString("en-US")}</p>` +
      rows([
        ["Hardpoints", hardpoints.join(" · ") || null],
        ["Slots", `${s.high_slots} high · ${s.mid_slots} mid · ${s.low_slots} low`],
        ["Drones", s.drone_cap ? `${Math.round(s.drone_cap)} m³ / ${Math.round(s.drone_bw)} Mbit` : null],
        ["Max velocity", `${Math.round(s.max_velocity)} m/s`],
        ["Warp speed", s.warp_speed ? `${s.warp_speed.toFixed(2)} AU/s` : null],
      ]) +
      s.traits
        .map(
          (g) =>
            `<p class="mskill">${esc(g.skill)}</p><ul class="mtraits">` +
            g.lines
              .map(
                ([bonus, text]) =>
                  `<li>${bonus ? `<b>${bonus % 1 === 0 ? bonus : bonus.toFixed(1)}%</b> ` : ""}${esc(text)}</li>`
              )
              .join("") +
            `</ul>`
        )
        .join("")
  );
}

/// The uncertain-pilot prompt, carrying the app's own wording. Getting this wrong hides real pilots,
/// which is the failure the whole "?" mechanism exists to avoid.
function showVerdict(name) {
  open(
    "pilot",
    `<h3>Uncertain pilot (?)</h3>` +
      `<p>This name was parsed out of chat but could not be confirmed as a character. ` +
      `Marking it correctly keeps the feed honest.</p>` +
      `<p class="mname">${esc(name)}</p>` +
      `<div class="mbtns">` +
      `<button class="mok" data-verdict="real">Real pilot</button>` +
      `<button class="mno" data-verdict="hide">Not a pilot (hide)</button>` +
      `</div>`
  );
  shells.get("pilot").querySelectorAll("[data-verdict]").forEach((b) =>
    b.addEventListener("click", async () => {
      await send({ Verdict: { name, hidden: b.dataset.verdict === "hide" } });
      close("pilot");
    })
  );
}

async function showPilot(name) {
  const un = new Set(
    (state.snapshot?.intel?.lookups?.uncertain ?? []).map((u) => u.toLowerCase())
  );
  if (un.has(name.toLowerCase())) return showVerdict(name);
  const id = state.snapshot?.intel?.lookups?.resolved_pilots?.[name];
  open(
    "pilot",
    `<h3>${ico("user")} ${esc(name)}</h3>` +
      (id
        ? `<img class="mport" src="https://images.evetech.net/characters/${id}/portrait?size=256" alt="">` +
          `<p><a href="https://zkillboard.com/character/${id}/" target="_blank" rel="noopener">` +
          `${ico("arrow-square-out")} zKillboard</a></p>`
        : `<p class="placeholder">Not resolved.</p>`)
  );
}

/// The route window: the jump planner, for a route picked off the map.
///
/// It takes the system window's corner, which is what the user asked for: a route is read against
/// the map the same way a system is, and two windows in the same place would be two windows to move.
let routeOpts = [];
let routeAt = 0;
/// What the window is showing, kept so the hull and skill controls can ask again without the map
/// having to hand the anchors over a second time.
let routeReq = null;
/// The jump setup, per device. Skills default to five, which is what anyone flying a capital has.
const JUMP_KEY = "spai_jump";
let jump = { hull: 0, jdc: 5, jfc: 5, tstart: true, tself: false };
try {
  jump = { ...jump, ...JSON.parse(localStorage.getItem(JUMP_KEY) ?? "{}") };
} catch {
  // Private browsing. The setup then lasts one session.
}

/// Systems left out of the route being planned. Cleared with the route; the permanent lists live in
/// the app's settings, where they survive a reload and reach the desktop too.
export const avoidOnce = new Set();
/// Systems a titan is sitting in, for this route. Not a setting and not pushed from the app: which
/// ships are where is a fact about the operation being planned, so it lives and dies with the route.
export const titansOnce = new Set();
/// The route currently being shown, as a live binding.
///
/// The event below is how the map hears about a change, but an event has no replay: a route opened
/// from a deep link is announced before the map has wired up its listener. So the map reads this
/// once when it wires, and follows the event after that.
export let currentRoute = null;
/// Which alternative is picked for each leg.
let legPick = [];

export async function showRoute(kind, anchors, onPick) {
  if (kind === "cancel") return;
  if (routeReq?.anchors?.length !== anchors.length) legPick = [];
  routeReq = { kind, anchors, onPick };
  open("route", `<h3>Route</h3><p class="placeholder">Working it out.</p>`);
  await fetchRoute();
}

async function fetchRoute() {
  const { kind, anchors, onPick } = routeReq;
  const from = anchors[0];
  const to = anchors[anchors.length - 1];
  const via = anchors.slice(1, -1).join(",");
  let out;
  try {
    const r = await fetch(
      `/api/route?from=${from}&to=${to}&via=${via}&kind=${encodeURIComponent(kind)}` +
        `&hull=${jump.hull}&jdc=${jump.jdc}&jfc=${jump.jfc}&tstart=${jump.tstart ? 1 : 0}&tself=${jump.tself ? 1 : 0}` +
        `&avoid=${[...avoidOnce].join(",")}&titans=${[...titansOnce].join(",")}` +
        `&pick=${legPick.join(",")}`
    );
    out = await r.json();
  } catch {
    return open("route", `<h3>Route</h3><p class="placeholder">Could not reach the app.</p>`);
  }
  routeOpts = out.options ?? [];
  routeAt = 0;
  routeReq.out = out;
  if (!routeOpts.length) {
    return open(
      "route",
      `<h3>Route</h3>${jumpControls(kind, out)}<p class="placeholder">${esc(out.error ?? "No route.")}</p>`
    );
  }
  paintRoute(kind, onPick);
}

/// The hull and the two skills, for the routes where they change the answer.
///
/// Names and ranges come from the app's own `SHIP_CLASSES` rather than a second copy in here: a
/// picker that disagrees with the planner about what a jump freighter can do is worse than no picker.
/// The app's "route via wormholes" setting, shown and changed from here.
///
/// Not a per-device preference: it changes what the desktop plans as well, and a route that used a
/// hole on the phone and not on the machine would be two different routes with one name.
function holeControl(kind, out) {
  if (kind === "jump" || !out) return "";
  return (
    `<label class="jumpcfg tflag"><input data-jump="holes" type="checkbox"${out.via_wormholes ? " checked" : ""}>` +
    ` Route via scanned wormholes</label>`
  );
}

function jumpControls(kind, out) {
  // Which end the titan is at. On, the default, it is in the system the route starts from: one jump
  // out and gates for the rest. Off, it is waiting at the far end and bridges you the last leg.
  if (kind === "titan") {
    return (
      `<label class="jumpcfg tflag"><input data-jump="tstart" type="checkbox"${jump.tstart ? " checked" : ""}>` +
      ` Titan is in the starting system</label>` +
      (jump.tstart
        ? `<label class="jumpcfg tflag"><input data-jump="tself" type="checkbox"${jump.tself ? " checked" : ""}>` +
          ` Titan may reposition first</label>`
        : "")
    );
  }
  if (kind !== "jump" || !out?.hulls?.length) return "";
  return (
    `<div class="jumpcfg">` +
    `<select data-jump="hull">` +
    out.hulls
      .map(
        (h, i) =>
          `<option value="${i}"${i === jump.hull ? " selected" : ""}>${esc(h.name)}</option>`
      )
      .join("") +
    `</select>` +
    `<label>JDC <input data-jump="jdc" type="number" min="0" max="5" value="${jump.jdc}"></label>` +
    `<label>JFC <input data-jump="jfc" type="number" min="0" max="5" value="${jump.jfc}"></label>` +
    `<span class="jumpmax">${out.max_ly.toFixed(1)} ly</span>` +
    `</div>`
  );
}

const KINDS = { gate: "Gate route", jump: "Jump route", titan: "Titan route" };

/// Whether the avoid list is expanded. "Avoiding 3 systems" is only useful if you can see which
/// three, and a list that is always open costs the hops their room.
let avoidOpen = false;

function avoidList(out) {
  const rows = out?.avoided ?? [];
  if (!rows.length) return "";
  const head =
    `<button class="ravoiding" data-avoidlist>${ico("eye-slash")} avoiding ${rows.length} ` +
    `${rows.length === 1 ? "system" : "systems"}</button>`;
  if (!avoidOpen) return `<p class="ravoidbar">${head}</p>`;
  return (
    `<p class="ravoidbar">${head}<button data-unavoid="all">clear this route</button></p>` +
    `<ul class="ravoidrows">` +
    rows
      .map(
        (a) =>
          `<li><span>${esc(a.name)}</span>` +
          (a.always ? `<em>always</em>` : "") +
          `<button data-unavoid="${a.id}" data-always="${a.always ? 1 : 0}" title="Stop avoiding">` +
          `${ico("x")}</button></li>`
      )
      .join("") +
    `</ul>`
  );
}

function paintRoute(kind, onPick) {
  const o = routeOpts[routeAt];
  onPick?.(o);
  // Announced rather than called back, so the map can draw the route without this module importing
  // it: dialogs already imports the pane renderers, and the map imports dialogs. It also means a
  // route opened from a deep link is drawn, which a callback the link cannot pass was not.
  currentRoute = o;
  window.dispatchEvent(new CustomEvent("spai:route", { detail: o }));
  const secCol = (v) => `var(--sec-${Math.min(10, Math.max(0, Math.round(v * 10)))})`;
  // More than one way to do it, so the window offers them rather than picking one silently. This is
  // the titan case: several systems are the same number of gates out and only the pilot knows which
  // staging they would rather burn.
  // One switcher per leg: the alternatives all cost the same number of jumps, so the list reads as
  // "these are the same price, shortest first" rather than as a ranking.
  const legs = (routeReq?.out?.legs ?? [])
    .map((l, i) =>
      l.options.length > 1
        ? `<div class="rleg"><span>${esc(l.from_name)} → ${esc(l.to_name)}</span>` +
          l.options
            .map(
              (o, k) =>
                `<button class="ropt${(legPick[i] ?? 0) === k ? " on" : ""}" data-leg="${i}" data-alt="${k}">` +
                `${o.total_ly ? `${o.total_ly.toFixed(1)} ly` : `${o.jumps}j`}</button>`
            )
            .join("") +
          `</div>`
        : ""
    )
    .join("");
  const tabs =
    routeOpts.length > 1
      ? `<div class="ropts">` +
        routeOpts
          .map(
            (x, i) =>
              `<button class="ropt${i === routeAt ? " on" : ""}" data-ropt="${i}">${esc(x.label)}</button>`
          )
          .join("") +
        `</div>`
      : "";
  const mins = (m) => (m >= 60 ? `${(m / 60).toFixed(1)}h` : `${Math.round(m)}m`);
  const SEV = ["", "", "Danger", "Critical"];
  // Why not to fly through here. Intel below Danger is left off on purpose: a nullsec route passes
  // through dozens of systems someone has said something about, and a warning on all of them is a
  // warning on none.
  const warn = (w, id) => {
    if (!w) return "";
    const bits = [];
    if (w.sev >= 2) bits.push(`${SEV[w.sev]} intel ${fmtAge(Math.max(0, Date.now() / 1000 - w.at))}`);
    // No "this hour": the figures are hourly and every one of them says so, which is three words per
    // row saying the same thing.
    if (w.kills || w.pods) {
      bits.push(`${w.kills} ${w.kills === 1 ? "kill" : "kills"}${w.pods ? ` · ${w.pods} pods` : ""}`);
    }
    if (!bits.length) return "";
    // Clickable when there is intel behind it, because "Danger intel 4m" is a summary of something
    // someone actually wrote and the words are the part worth reading.
    const tag = w.sev >= 2 ? "button" : "span";
    const attr = w.sev >= 2 ? ` data-intel="${id}"` : "";
    return `<${tag} class="rwarn${w.sev >= 3 ? " crit" : ""}"${attr}>${ico("warning")} ${esc(bits.join(" · "))}</${tag}>`;
  };
  const line = (h, i) =>
    `<li class="rhop k${h.kind}">` +
    `<button class="chip${h.anchor ? " anchor" : ""}" data-system="${h.id}" style="color:${secCol(h.security)}">${esc(h.name)}</button>` +
    (i === 0
      ? `<span class="rkind">start</span>`
      : h.kind === 2
        ? `<span class="rkind jump">jump ${h.ly?.toFixed(1) ?? "?"} ly</span>` +
          (h.fuel == null
            ? ""
            : `<span class="rcost">${Math.round(h.fuel).toLocaleString("en-US")} iso` +
              ` · fatigue ${mins(h.fatigue_min)} · ready in ${mins(h.reactivation_min)}</span>`)
        : h.kind === 1
          ? `<span class="rkind bridge">ansiblex</span>`
          : `<span class="rkind">gate</span>`) +
    warn(h.warn, h.id) +
    // One button rather than one per action: a row is a system and a distance, and three buttons
    // beside that is more chrome than content.
    `<button class="ract" data-act="${h.id}" data-at="${i}" title="Actions">${ico("dots-three")}</button>` +
    `</li>`;
  open(
    "route",
    `<h3>${esc(KINDS[kind] ?? "Route")}</h3>` +
      jumpControls(kind, routeReq?.out) +
      holeControl(kind, routeReq?.out) +
      avoidList(routeReq?.out) +
      tabs +
      legs +
      `<p class="mgroup">${o.jumps} ${o.jumps === 1 ? "jump" : "jumps"}` +
      (o.gates ? ` · ${o.gates} ${o.gates === 1 ? "gate" : "gates"}` : "") +
      (o.total_ly ? ` · ${o.total_ly.toFixed(1)} ly` : "") +
      `</p>` +
      (o.note ? `<p class="mgroup">${esc(o.note)}</p>` : "") +
      (o.detour ? `<p class="rdetour">${ico("eye-slash")} ${esc(o.detour)}</p>` : "") +
      (o.saved != null
        ? `<p class="rsaved${o.saved <= 2 ? " thin" : ""}">${ico("sign-in")} saves ${o.saved} ` +
          `${o.saved === 1 ? "gate" : "gates"}` +
          (o.saved <= 2 ? ` — barely worth the cyno, a direct route may be simpler` : "") +
          `</p>`
        : "") +
      (o.titan_jump
        ? `<p class="rtitan">${ico("star-four")} Titan jumps ${esc(o.titan_jump.from_name)} → ` +
          `${esc(o.titan_jump.to_name)}, ${o.titan_jump.ly.toFixed(1)} ly</p>`
        : "") +
      `<ol class="rhops">${o.hops.map(line).join("")}</ol>` +
      `<p class="rsave"><button data-save>${ico("copy")} Save route</button>` +
      `<button data-load-open>${ico("arrow-square-out")} Load…</button></p>`
  );
  win_wire(kind, onPick);
}

function win_wire(kind, onPick) {
  const win = shells.get("route");
  win?.querySelector("[data-save]")?.addEventListener("click", () => saveRoute(kind));
  win?.querySelector("[data-load-open]")?.addEventListener("click", () => loadRoute(onPick));
  win?.querySelectorAll("[data-ropt]").forEach((b) =>
    b.addEventListener("click", () => {
      routeAt = Number(b.dataset.ropt);
      paintRoute(kind, onPick);
    })
  );
  win?.querySelectorAll("[data-act]").forEach((b) =>
    b.addEventListener("click", (e) => {
      const id = Number(b.dataset.act);
      const at = Number(b.dataset.at);
      const r = b.getBoundingClientRect();
      menu(r.left, r.bottom + 4, hopMenu(id, at, kind), (pick) =>
        hopAction(pick, id, at, kind, onPick)
      );
      e.stopPropagation();
    })
  );
  win?.querySelector("[data-avoidlist]")?.addEventListener("click", () => {
    avoidOpen = !avoidOpen;
    paintRoute(kind, onPick);
  });
  win?.querySelectorAll("[data-unavoid]").forEach((b) =>
    b.addEventListener("click", () => {
      const which = b.dataset.unavoid;
      if (which === "all") avoidOnce.clear();
      else if (b.dataset.always === "1") {
        send({
          AvoidSystem: { id: Number(which), jump: routeReq?.kind === "jump", on: false },
        });
        setTimeout(fetchRoute, 150);
        return;
      } else avoidOnce.delete(Number(which));
      fetchRoute();
    })
  );
  win?.querySelectorAll("[data-leg]").forEach((b) =>
    b.addEventListener("click", () => {
      legPick[Number(b.dataset.leg)] = Number(b.dataset.alt);
      fetchRoute();
    })
  );
  win?.querySelectorAll("[data-jump]").forEach((c) =>
    c.addEventListener("change", () => {
      const k = c.dataset.jump;
      const v = Number(c.value);
      if (k === "holes") {
        // The app owns this one, so it goes back there and comes round again in the next answer.
        send({ RouteViaWormholes: { on: c.checked } });
        if (routeReq?.out) routeReq.out.via_wormholes = c.checked;
        setTimeout(fetchRoute, 150);
        return;
      }
      jump[k] =
        k === "tstart" || k === "tself" ? c.checked : k === "hull" ? v : Math.max(0, Math.min(5, v));
      try {
        localStorage.setItem(JUMP_KEY, JSON.stringify(jump));
      } catch {
        /* private browsing */
      }
      fetchRoute();
    })
  );
}

/// The intel behind a route warning, as a modal.
///
/// A modal rather than another floating window: it is opened from one and would otherwise land on top
/// of the thing that named it, and it is read and dismissed rather than kept beside the map.
function showIntel(id) {
  document.querySelector(".intelmodal")?.remove();
  const cards = (state.snapshot?.intel?.cards ?? []).filter((c) =>
    (c.report.systems ?? []).some((s) => s.id === id)
  );
  const name = cards[0]?.report?.systems?.find((s) => s.id === id)?.name ?? id;
  const wrap = document.createElement("div");
  wrap.className = "jstartdlg intelmodal";
  wrap.innerHTML =
    `<div class="mpanel"><button class="mclose" aria-label="Close">${ico("x")}</button>` +
    `<h3>${ico("warning")} ${esc(name)}</h3>` +
    (cards.length
      ? `<div class="feed">${cards.map((c) => card(c, state.snapshot?.intel?.lookups, false, Math.floor(Date.now() / 1000))).join("")}</div>`
      : `<p class="placeholder">Nothing in the feed for this system any more.</p>`) +
    `</div>`;
  document.body.append(wrap);
  wrap.addEventListener("click", (e) => {
    if (e.target === wrap || e.target.closest(".mclose")) wrap.remove();
  });
}

/// What a row offers, which depends on the kind of route and on what the system already is.
function hopMenu(id, at, kind) {
  const o = routeOpts[routeAt];
  const h = o?.hops?.[at];
  const titans = titansOnce;
  const items = [];
  if (h?.warn?.sev >= 2) items.push(["intel", "Show intel"]);
  // An anchor is a choice the user made, so the row it sits on is where taking it back belongs. Not
  // the start: a route has to begin somewhere, and an anchor list without one means nothing.
  const anchorAt = routeReq?.anchors?.indexOf(id) ?? -1;
  if (h?.anchor && anchorAt > 0) {
    items.push([
      "drop",
      anchorAt === routeReq.anchors.length - 1 ? "Remove destination" : "Remove waypoint",
    ]);
  }
  if (!h?.anchor) {
    items.push([avoidOnce.has(id) ? "unavoid" : "avoid", avoidOnce.has(id) ? "Stop avoiding" : "Avoid this system"]);
    items.push(["way", "Add waypoint here"]);
  }
  if (kind === "jump" && at > 0 && at < (o?.hops?.length ?? 0) - 1) {
    items.push(["alts", "Other systems between…"]);
  }
  if (kind === "titan") {
    items.push([titans.has(id) ? "untitan" : "titan", titans.has(id) ? "Not a titan system" : "Set as titan system"]);
  }
  items.push(null);
  items.push(["info", "Show info"]);
  return items;
}

async function hopAction(pick, id, at, kind, onPick) {
  switch (pick) {
    case "intel":
      return showIntel(id);
    case "info":
      return showSystem(id);
    case "avoid":
      avoidOnce.add(id);
      return fetchRoute();
    case "unavoid":
      avoidOnce.delete(id);
      return fetchRoute();
    case "drop": {
      const a = routeReq.anchors.filter((x) => x !== id);
      if (a.length < 2) return;
      routeReq.anchors = a;
      return fetchRoute();
    }
    case "way": {
      // Between the anchors it already sits between, so the route keeps its shape and gains a stop.
      const a = routeReq.anchors;
      const before = a.findIndex((x) => routeOpts[routeAt].path.indexOf(x) > routeOpts[routeAt].path.indexOf(id));
      routeReq.anchors = before < 0 ? [...a.slice(0, -1), id, a[a.length - 1]] : [...a.slice(0, before), id, ...a.slice(before)];
      return fetchRoute();
    }
    case "titan":
      titansOnce.add(id);
      return fetchRoute();
    case "untitan":
      titansOnce.delete(id);
      return fetchRoute();
    case "alts":
      return showAlternatives(at, kind, onPick);
  }
}

/// The systems a capital could stop in between the two hops either side of this one.
///
/// Picking one inserts it as a waypoint rather than replacing anything: that is what makes it a
/// steer rather than a different route.
async function showAlternatives(at, kind, onPick) {
  const o = routeOpts[routeAt];
  const a = o.hops[at - 1]?.id;
  const b = o.hops[at + 1]?.id;
  if (a == null || b == null) return;
  let rows = [];
  try {
    const r = await fetch(
      `/api/alternatives?a=${a}&b=${b}&hull=${jump.hull}&jdc=${jump.jdc}`
    );
    rows = await r.json();
  } catch {
    return;
  }
  const secCol = (v) => `var(--sec-${Math.min(10, Math.max(0, Math.round(v * 10)))})`;
  const wrap = document.createElement("div");
  wrap.className = "jstartdlg altdlg";
  wrap.innerHTML =
    `<div class="mpanel"><button class="mclose" aria-label="Close">${ico("x")}</button>` +
    `<h3>In range of both</h3>` +
    (rows.length
      ? `<ul class="ravoidrows">` +
        rows
          .map(
            (s) =>
              `<li><button data-alt="${s.id}" style="color:${secCol(s.security)}">${esc(s.name)}</button></li>`
          )
          .join("") +
        `</ul>`
      : `<p class="placeholder">Nothing else is in range of both.</p>`) +
    `</div>`;
  document.body.append(wrap);
  wrap.addEventListener("click", (e) => {
    if (e.target === wrap || e.target.closest(".mclose")) return wrap.remove();
    const b2 = e.target.closest("[data-alt]");
    if (!b2) return;
    wrap.remove();
    const anchors = routeReq.anchors;
    routeReq.anchors = [...anchors.slice(0, -1), Number(b2.dataset.alt), anchors[anchors.length - 1]];
    fetchRoute();
  });
}

/// Saving and loading a route.
///
/// The whole thing, not just the endpoints: a route is the anchors and what you told the planner
/// about them, and one that came back without its avoid list or its titans would be a different
/// route with the same name. It lives in the app's settings, so it reaches the desktop too.
async function saveRoute(kind) {
  const name = prompt("Save this route as");
  if (!name?.trim()) return;
  const a = routeReq?.anchors ?? [];
  if (a.length < 2) return;
  const wh = !!routeReq?.out?.via_wormholes;
  if (
    wh &&
    !confirm(
      "This route was planned through scanned wormholes. Those chains move, so it is deleted a day " +
        "after saving rather than quietly becoming wrong.\n\nSave it anyway?"
    )
  ) {
    return;
  }
  await send({
    SaveRoute: {
      route: {
        name: name.trim(),
        kind,
        anchors: a,
        avoid: [...avoidOnce],
        titans: [...titansOnce],
        titan_at_start: jump.tstart,
        titan_self_jump: jump.tself,
        hull: jump.hull,
        jdc: jump.jdc,
        jfc: jump.jfc,
        saved_at: 0,
        via_wormholes: wh,
      },
    },
  });
}

async function loadRoute(onPick) {
  let rows = [];
  try {
    rows = await (await fetch("/api/routes")).json();
  } catch {
    return;
  }
  const wrap = document.createElement("div");
  wrap.className = "jstartdlg altdlg";
  wrap.innerHTML =
    `<div class="mpanel"><button class="mclose" aria-label="Close">${ico("x")}</button>` +
    `<h3>Saved routes</h3>` +
    (rows.length
      ? `<ul class="ravoidrows">` +
        rows
          .map(
            (r, i) =>
              `<li><button data-load="${i}">${esc(r.route.name)}` +
              `<em>${esc(r.from_name ?? "")} → ${esc(r.to_name ?? "")} · ${esc(r.route.kind)}` +
              `${r.route.via_wormholes ? " · expires" : ""}</em></button>` +
              `<button data-forget="${esc(r.route.name)}" title="Forget">${ico("x")}</button></li>`
          )
          .join("") +
        `</ul>`
      : `<p class="placeholder">Nothing saved yet.</p>`) +
    `</div>`;
  document.body.append(wrap);
  wrap.addEventListener("click", async (e) => {
    if (e.target === wrap || e.target.closest(".mclose")) return wrap.remove();
    const f = e.target.closest("[data-forget]");
    if (f) {
      await send({ DeleteRoute: { name: f.dataset.forget } });
      wrap.remove();
      return;
    }
    const b = e.target.closest("[data-load]");
    if (!b) return;
    const r = rows[Number(b.dataset.load)].route;
    wrap.remove();
    avoidOnce.clear();
    for (const id of r.avoid ?? []) avoidOnce.add(id);
    titansOnce.clear();
    for (const id of r.titans ?? []) titansOnce.add(id);
    jump = { ...jump, hull: r.hull ?? 0, jdc: r.jdc ?? 5, jfc: r.jfc ?? 5, tstart: !!r.titan_at_start, tself: !!r.titan_self_jump };
    legPick = [];
    showRoute(r.kind, r.anchors, onPick);
  });
}

export async function send(action) {
  if (!state.snapshot?.meta?.allow_writeback) return false;
  try {
    const r = await fetch("/api/action", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(action),
    });
    return r.ok;
  } catch {
    return false;
  }
}

// One listener on the document rather than one per chip: panes re-render constantly, and rebinding
// on every repaint is how a handler ends up attached twice or not at all.
document.addEventListener("click", (e) => {
  const intel = e.target.closest("[data-intel]");
  if (intel) return showIntel(Number(intel.dataset.intel));
  const sys = e.target.closest("[data-system]");
  if (sys) return showSystem(Number(sys.dataset.system));
  const ship = e.target.closest("[data-ship]");
  if (ship) return showShip(Number(ship.dataset.ship));
  const pilot = e.target.closest("[data-pilot]");
  if (pilot) return showPilot(pilot.dataset.pilot);
});

/// Deep links: `#system/30004759`, `#ship/587`, `#pilot/Some%20Name`.
///
/// Worth having on its own, since a dialog is a thing you want to send someone. It is also the only
/// way a load-time screenshot can capture one, the harness being unable to click.
function fromHash() {
  // The hash holds one route, and a shot of a dialog *over the map* needs two: which pane, and
  // which dialog. `?dlg=system/30004759` is the second channel.
  const src = /^#(system|ship|pilot)\//.test(location.hash)
    ? location.hash.slice(1)
    : new URLSearchParams(location.search).get("dlg") ?? "";
  // `route/<kind>/<from>/<to>`: the route window has no hash route of its own, and this is the only
  // way a load-time screenshot can reach it.
  const r = /^route\/(\w+)\/(\d+)\/(\d+)$/.exec(src);
  if (r) return showRoute(r[1], [Number(r[2]), Number(r[3])]);
  const m = /^(system|ship|pilot)\/(.+)$/.exec(src);
  if (!m) return;
  const [, kind, raw] = m;
  const arg = decodeURIComponent(raw);
  if (kind === "system") showSystem(Number(arg));
  else if (kind === "ship") showShip(Number(arg));
  else showPilot(arg);
}

window.addEventListener("hashchange", fromHash);
fromHash();
