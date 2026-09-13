// Dialogs render here, in the browser, not as a window on the desktop. A tap on a phone must not
// raise a viewport on a machine in another room, which is why a badge tap is a plain GET and never
// an IntelClick.

import { ico, state } from "./app.js";
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
  const row = document.querySelector('#panes [data-pane="map"] .maprow');
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
  if (had) return had;
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
        ["Ship kills, last hour", s.ship_kills || null],
        ["Pod kills, last hour", s.pod_kills || null],
        ["NPC kills, last hour", s.npc_kills || null],
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
let jump = { hull: 0, jdc: 5, jfc: 5, tstart: true };
try {
  jump = { ...jump, ...JSON.parse(localStorage.getItem(JUMP_KEY) ?? "{}") };
} catch {
  // Private browsing. The setup then lasts one session.
}

/// Systems left out of the route being planned. Cleared with the route; the permanent lists live in
/// the app's settings, where they survive a reload and reach the desktop too.
export const avoidOnce = new Set();
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
        `&hull=${jump.hull}&jdc=${jump.jdc}&jfc=${jump.jfc}&tstart=${jump.tstart ? 1 : 0}` +
        `&avoid=${[...avoidOnce].join(",")}&pick=${legPick.join(",")}`
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
      ` Titan is in the starting system</label>`
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
    if (w.kills || w.pods) {
      bits.push(`${w.kills} ${w.kills === 1 ? "kill" : "kills"}${w.pods ? ` · ${w.pods} pods` : ""} this hour`);
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
          ? `<span class="rkind bridge">bridge</span>`
          : `<span class="rkind">gate</span>`) +
    warn(h.warn, h.id) +
    // Not on the systems the user named: the endpoints are exempt from avoidance anyway, so the
    // button would be there and do nothing.
    (h.anchor
      ? ""
      : `<button class="ravoid" data-avoid="${h.id}" title="Plan around this system">${ico("eye-slash")}</button>`) +
    `</li>`;
  open(
    "route",
    `<h3>${esc(KINDS[kind] ?? "Route")}</h3>` +
      jumpControls(kind, routeReq?.out) +
      holeControl(kind, routeReq?.out) +
      (avoidOnce.size
        ? `<p class="ravoiding">${ico("eye-slash")} avoiding ${avoidOnce.size} ` +
          `${avoidOnce.size === 1 ? "system" : "systems"} on this route ` +
          `<button data-unavoid="all">clear</button></p>`
        : "") +
      tabs +
      legs +
      `<p class="mgroup">${o.jumps} ${o.jumps === 1 ? "jump" : "jumps"}` +
      (o.gates ? ` · ${o.gates} ${o.gates === 1 ? "gate" : "gates"}` : "") +
      (o.total_ly ? ` · ${o.total_ly.toFixed(1)} ly` : "") +
      `</p>` +
      (o.note ? `<p class="mgroup">${esc(o.note)}</p>` : "") +
      `<ol class="rhops">${o.hops.map(line).join("")}</ol>`
  );
  const win = shells.get("route");
  win?.querySelectorAll("[data-ropt]").forEach((b) =>
    b.addEventListener("click", () => {
      routeAt = Number(b.dataset.ropt);
      paintRoute(kind, onPick);
    })
  );
  win?.querySelectorAll("[data-avoid]").forEach((b) =>
    b.addEventListener("click", () => {
      avoidOnce.add(Number(b.dataset.avoid));
      fetchRoute();
    })
  );
  win?.querySelector("[data-unavoid]")?.addEventListener("click", () => {
    avoidOnce.clear();
    fetchRoute();
  });
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
      jump[k] = k === "tstart" ? c.checked : k === "hull" ? v : Math.max(0, Math.min(5, v));
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
