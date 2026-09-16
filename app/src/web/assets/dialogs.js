// Dialogs render in the browser. A badge tap is a plain GET, never an IntelClick, so a tap on a
// phone cannot raise a window on a desktop in another room.

import { esc, ico, modal, send, state } from "./app.js";
import { menu } from "./route.js";
import { card, fmtAge } from "./panes-intel.js";
import { section as notesSection } from "./notes.js";


const secVar = (sec) => `var(--sec-${Math.min(10, Math.max(0, Math.round(sec * 10)))})`;

/// One window per kind, so a ship opened from a card does not replace the system being read.
const shells = new Map();
/// Per-kind offset from the map's corner, so floating windows cascade instead of hiding each other.
const CASCADE = { system: 0, ship: 1, pilot: 2, route: 0 };
const TAB_NAME = { route: "Route", system: "System", ship: "Ship", pilot: "Pilot" };

/// The dock in the map pane, or null when there is no visible map pane to dock into.
///
/// One tabbed box rather than one per kind: three docked side by side would leave the map a sliver.
function dock() {
  const pane = document.querySelector('#panes [data-pane="map"]');
  // Docking into a hidden pane would render the window inside `display: none`, a dead click.
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

/// A draggable floating window, not a modal, so the map stays usable behind it. Clicking the map
/// does not dismiss it, because that is how the next one is opened.
function shell(kind) {
  const had = shells.get(kind);
  if (had) {
    // The map pane can be switched off while a window is docked inside it.
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
  // A pane resize moves the map, and the map can arrive after the window.
  const repark = () => {
    // The map pane may not exist when the window first opens. `spai:map` moves it in once it does.
    const body = dock()?.querySelector(".dbody");
    if (body && dlg.parentElement !== body) {
      dlg.className = "dpane";
      dlg.style.cssText = "";
      // The tab carries its own close button.
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
    // Controls keep the pointer, since dragging a number field or drop-down is how it is used.
    if (e.target.closest("button, a, input, select, textarea, label, option")) return;
    // Docked, the layout places it, and capturing the pointer fought the panel's own touch scroll.
    if (node.classList.contains("dpane")) return;
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
    // Moved by hand, so `place` leaves it alone.
    node.dataset.moved = "1";
  });
  const drop = () => {
    from = null;
  };
  node.addEventListener("pointerup", drop);
  node.addEventListener("pointercancel", drop);
}

function close(kind = null) {
  for (const [k, d] of shells) {
    if (kind == null || k === kind) d.hidden = true;
  }
  if (kind == null || kind === "route") {
    currentRoute = null;
    window.dispatchEvent(new CustomEvent("spai:route", { detail: null }));
  }
  paintTabs(null);
}

/// Park the window at the top right of the map, below the toolbar and the layer panel. On a narrow
/// pane the panel overlays the canvas, so the canvas top alone would cover the filters.
function place(d, tries = 10) {
  if (d.classList.contains("dpane") || d.dataset.moved || d.hidden) return;
  const canvas = document.querySelector(".starmap");
  const r = canvas?.getBoundingClientRect();
  // The map is off screen (tabs mode) or has no geometry yet. Retry for about a second, or the
  // window stays in the CSS corner over the toolbar it should clear.
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
  // Floating, the newest goes on top.
  for (const [k, o] of shells) o.style.zIndex = k === kind ? 52 : 50;
  place(d);
}

function rows(pairs) {
  return pairs
    .filter(([, v]) => v !== null && v !== undefined && v !== "" && v !== false)
    .map(([k, v]) => `<div class="mrow"><span>${k}</span><span>${v}</span></div>`)
    .join("");
}

/// Damage types in the app's order and colours, shared by the resist table and the rat profile.
const DMG = [
  ["EM", "#5aa9e0"],
  ["Th", "#d64545"],
  ["Kin", "#9aa3a8"],
  ["Exp", "#d6a645"],
];

const DMG_COL = (name) => (DMG.find(([t]) => name.toLowerCase().startsWith(t.toLowerCase().slice(0, 2)))
  ?? [null, "var(--fg)"])[1];

/// Colour a counter against its region's average, since the same count is quiet in Delve and a
/// siege in Aridia.
function heat(v, avg) {
  if (avg > 0 && v >= 2 * avg) return "var(--hostile)";
  if (avg > 0 && v > avg) return "var(--warning)";
  return "var(--fg)";
}

const CAMP_TEXT = {
  likely: ["Likely gate camp", "#ef4444"],
  possible: ["Possible camp", "#ffa726"],
  flag: ["Recent gate kills", "#ffd54f"],
};

const ADM_COL = (adm) => (adm >= 5 ? "#5ac86a" : adm >= 3 ? "var(--warning)" : "var(--hostile)");

/// The system dialog, with the app's system window content laid out as cards.
async function showSystem(id) {
  open("system", `<h3>System</h3><p class="placeholder">Loading.</p>`);
  const r = await fetch(`/api/system/${id}`);
  if (!r.ok) return open("system", `<h3>System</h3><p class="placeholder">Not in the star map.</p>`);
  const s = await r.json();

  // Neighbour chips show intel severity from the snapshot, where the app shows a report count.
  const sev = new Map((state.snapshot?.map?.intel ?? []).map(([sid, v]) => [sid, v]));
  const SEV = ["info", "warning", "danger", "critical"];

  const chip = (text, colour) =>
    `<span class="schip" style="--c:${colour}">${esc(text)}</span>`;
  const chips = [
    s.sov ? chip(s.sov, "var(--corp)") : "",
    s.faction && s.security < 0.5 ? chip(s.faction, "var(--neutral)") : "",
    s.jove ? chip("Jove Observatory", "#b88cf0") : "",
    s.incursion ? chip("INCURSION", "var(--alliance)") : "",
    s.fw ? chip(`FW ${s.fw}`, "var(--warning)") : "",
    s.wormhole ? chip("Wormhole space", "var(--accent)") : "",
  ].join("");

  const stat = (label, v, avg) =>
    `<div class="sstat"><b style="color:${heat(v, avg)}">${v.toLocaleString("en-US")}</b>` +
    `<span>${label}</span></div>`;

  const camp = s.camp
    ? `<p class="scamp" style="--c:${CAMP_TEXT[s.camp.level][1]}">${ico("campfire")} ` +
      `<b>${CAMP_TEXT[s.camp.level][0]}</b>: ${s.camp.kills} kills over ${s.camp.span_min}m, ` +
      `last ${s.camp.age_min}m ago</p>`
    : "";

  const rats = s.rats
    ? `<section class="scard"><h4>${ico("skull")} ${esc(s.rats.faction)} rats</h4>` +
      `<div class="srat"><span>Deals</span><span>` +
      s.rats.deal.map((t) => `<b style="color:${DMG_COL(t)}">${esc(t)}</b>`).join(" · ") +
      `</span></div><div class="srat"><span>Weak to</span><span>` +
      s.rats.weak.map((t) => `<b style="color:${DMG_COL(t)}">${esc(t)}</b>`).join(" · ") +
      `</span></div>` +
      (s.rats.ewar ? `<div class="srat"><span>EWAR</span><span>${esc(s.rats.ewar)}</span></div>` : "") +
      `</section>`
    : "";

  const holes = s.holes.length
    ? `<section class="scard"><h4>${ico("spiral")} Wormholes</h4>` +
      s.holes
        .map((h) => {
          const tail = [h.kind, h.size, h.hours == null ? "expiring" : `< ${h.hours}h`]
            .filter(Boolean)
            .map(esc)
            .join(" · ");
          const to = h.to_id
            ? `<button class="chip" data-system="${h.to_id}">${esc(h.to)}</button>`
            : `<b>${esc(h.to)}</b>`;
          return `<div class="shole"><code>${esc(h.sig)}</code> ${ico("arrow-right")} ${to}` +
            `<small>${tail}</small></div>`;
        })
        .join("") +
      `</section>`
    : "";

  const upgrades = s.upgrades.length
    ? `<section class="scard"><h4>${ico("arrow-up")} Sov upgrades</h4><ul class="supg">` +
      s.upgrades.map((u) => `<li>${esc(u)}</li>`).join("") +
      `</ul></section>`
    : "";

  // Tinted when the step leaves the constellation or region, marking a border gate.
  const neigh = s.neighbours.length
    ? `<section class="scard"><h4>Neighbours</h4><div class="sneigh">` +
      s.neighbours
        .map((n) => {
          const v = sev.get(n.id);
          const cls = n.cross_region ? " xregion" : n.cross_const ? " xconst" : "";
          const title = n.cross_region
            ? `${n.constellation} (${n.region})`
            : n.cross_const
              ? n.constellation
              : n.name;
          return `<button class="chip nb${cls}" data-system="${n.id}" title="${esc(title)}" ` +
            `style="color:${secVar(n.security)}">${n.security.toFixed(1)} ${esc(n.name)}` +
            (v == null ? "" : `<i class="ndot" style="background:var(--sev-${SEV[v] ?? "info"})"></i>`) +
            `</button>`;
        })
        .join("") +
      `</div></section>`
    : "";

  const jumpsTo = s.jumps_from_you == null
    ? "no route"
    : s.jumps_from_you === 0
      ? "you are here"
      : `${s.jumps_from_you} jumps`;

  const lyFrom = [
    s.ly_from_staging && [`staging ${s.ly_from_staging[0]}`, s.ly_from_staging[1]],
    s.ly_from_you,
  ]
    .filter(Boolean)
    .map(([from, ly]) => `<span>${ly.toFixed(2)} ly from ${esc(from)}</span>`)
    .join("");

  open(
    "system",
    `<h3 class="shead">` +
      `<span class="ssec" style="background:${secVar(s.security)}">${s.security.toFixed(1)}</span>` +
      `<span class="sname">${esc(s.name)}</span>` +
      `<button class="sstar${s.bookmarked ? " on" : ""}" data-bookmark="${s.id}" ` +
      `data-on="${s.bookmarked ? 0 : 1}" title="${s.bookmarked ? "Remove bookmark" : "Bookmark this system"}">` +
      `${ico("bookmark-simple")}</button>` +
      (s.sov_alliance
        ? `<img class="ssov" src="https://images.evetech.net/alliances/${s.sov_alliance}/logo?size=64" ` +
          `alt="" title="${esc(s.sov ?? "")}">`
        : "") +
      (s.adm == null ? "" : `<span class="sadm" style="color:${ADM_COL(s.adm)}" ` +
        `title="Activity Defense Multiplier">ADM ${s.adm.toFixed(1)}</span>`) +
      `</h3>` +
      `<p class="sloc">${esc(s.constellation)} <span>&lsaquo;</span> ${esc(s.region)}` +
      `<span class="sfrom">${jumpsTo}</span></p>` +
      (lyFrom ? `<p class="sly">${lyFrom}</p>` : "") +
      (chips ? `<p class="schips">${chips}</p>` : "") +
      camp +
      notesSection("system", s.id, s.name) +
      `<section class="scard"><h4>Last hour<span class="shint">Coloured against the ${esc(s.region)} average</span></h4>` +
      `<div class="sstats">` +
      stat("jumps", s.jumps, s.avg_jumps) +
      stat("ship kills", s.ship_kills, s.avg_ship_kills) +
      stat("pod kills", s.pod_kills, s.avg_ship_kills) +
      stat("NPC kills", s.npc_kills, s.avg_npc_kills) +
      `</div></section>` +
      rats +
      holes +
      upgrades +
      neigh
  );
}

async function showShip(id) {
  open("ship", `<h3>Ship</h3><p class="placeholder">Loading.</p>`);
  const r = await fetch(`/api/ship/${id}`);
  if (!r.ok) return open("ship", `<h3>Ship</h3><p class="placeholder">Not in the static data.</p>`);
  const s = await r.json();
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

/// The uncertain-pilot prompt, in the app's wording. Getting this wrong hides real pilots.
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
          `${ico("arrow-square-out")} zKillboard</a></p>` +
          notesSection("pilot", id, name)
        : `<p class="placeholder">Not resolved.</p>`)
  );
}

/// The route window, for a route picked off the map. It shares the system window's corner.
let routeOpts = [];
let routeAt = 0;
/// Kept so the hull and skill controls can refetch without the map resending the anchors.
let routeReq = null;
/// The jump setup, per device. Skills default to five, as for any capital pilot.
const JUMP_KEY = "spai_jump";
let jump = { hull: 0, jdc: 5, jfc: 5, tstart: true, tself: false };
try {
  jump = { ...jump, ...JSON.parse(localStorage.getItem(JUMP_KEY) ?? "{}") };
} catch {
  // Private browsing. The setup then lasts one session.
}

/// Systems left out of this route only. The permanent avoid lists live in the app's settings.
export const avoidOnce = new Set();
/// Systems a titan is sitting in, for this route only, since that belongs to the operation.
export const titansOnce = new Set();
/// The route currently shown, as a live binding. Events do not replay, and a deep-linked route is
/// announced before the map listens, so the map reads this once when it wires up.
export let currentRoute = null;
/// Which alternative is picked for each leg.
let legPick = [];
/// Which way to leave each system the route forks at, by system id.
let forkPick = new Map();

export async function showRoute(kind, anchors, onPick) {
  if (kind === "cancel") return;
  if (routeReq?.anchors?.length !== anchors.length) {
    legPick = [];
    forkPick = new Map();
  }
  routeReq = { kind, anchors, onPick };
  open("route", `<h3>Route</h3><p class="placeholder">Working it out.</p>`);
  await fetchRoute();
}

/// Mirrors `web::route::ingame_waypoints`: the autopilot flies a gate route whole, so every system
/// is a waypoint; a leg the pilot flies by hand only gets its two ends.
function ingameWaypoints(o, player) {
  if (!o?.path?.length) return [];
  const out = [];
  const start = o.path[0];
  if (player !== start) out.push(start);
  if ((o.hops ?? []).every((h) => !h.kind)) {
    out.push(...o.path.slice(1));
  } else {
    o.hops.forEach((h, i) => {
      // A fork the autopilot would take the other way round needs the branch pinned.
      if (h.fork?.length && o.path[i + 1] != null) out.push(o.path[i + 1]);
      if (!h.kind) return;
      if (i > 0) out.push(o.hops[i - 1].id);
      out.push(h.id);
    });
    out.push(o.path[o.path.length - 1]);
  }
  const uniq = out.filter((id, i) => i === 0 || id !== out[i - 1]);
  return uniq[0] === player ? uniq.slice(1) : uniq;
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
        `&pick=${legPick.join(",")}` +
        `&forks=${[...forkPick].map(([at, next]) => `${at}:${next}`).join(",")}`
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

/// The app's "route via wormholes" setting. A shared setting rather than per device, so the phone
/// and the desktop plan the same route.
function holeControl(kind, out) {
  if (kind === "jump" || !out) return "";
  return (
    `<label class="jumpcfg tflag"><input data-jump="holes" type="checkbox"${out.via_wormholes ? " checked" : ""}>` +
    ` ${flagText("Route via scanned wormholes", "Via wormholes")}</label>`
  );
}

/// Both wordings go out and CSS picks one: a bottom dock has no width for three long labels.
const flagText = (long, short) =>
  `<span class="flong">${long}</span><span class="fshort" title="${long}">${short}</span>`;

/// The hull and the two skills, for routes where they change the answer. Hulls come from the app's
/// `SHIP_CLASSES`, so the picker cannot disagree with the planner.
function jumpControls(kind, out) {
  // Which end the titan is at. On, the default, it is in the system the route starts from: one jump
  // out and gates for the rest. Off, it is waiting at the far end and bridges you the last leg.
  if (kind === "titan") {
    return (
      `<label class="jumpcfg tflag"><input data-jump="tstart" type="checkbox"${jump.tstart ? " checked" : ""}>` +
      ` ${flagText("Titan is in the starting system", "Titan at start")}</label>` +
      (jump.tstart
        ? `<label class="jumpcfg tflag"><input data-jump="tself" type="checkbox"${jump.tself ? " checked" : ""}>` +
          ` ${flagText("Titan may reposition first", "May reposition")}</label>`
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

/// Whether the avoid list is expanded. Collapsed by default so it does not crowd the hops.
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
  // An event rather than a callback: the map imports dialogs, so dialogs cannot import the map, and a
  // deep-linked route has no callback to pass.
  currentRoute = o;
  window.dispatchEvent(new CustomEvent("spai:route", { detail: o }));
  const secCol = (v) => `var(--sec-${Math.min(10, Math.max(0, Math.round(v * 10)))})`;
  // One switcher per leg with equal-cost alternatives, since only the pilot knows which staging to
  // use.
  const legs = (routeReq?.out?.legs ?? [])
    .map((l, i) =>
      l.options.length > 1 && !l.whole_route
        ? `<div class="rleg"><span>${esc(l.from_name)} → ${esc(l.to_name)}</span>` +
          l.options
            .map(
              (o, k) =>
                `<button class="ropt${(legPick[i] ?? 0) === k ? " on" : ""}" data-leg="${i}" data-alt="${k}">` +
                `${esc(o.label)}</button>`
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
  // Intel below Danger is left off: a nullsec route passes dozens of systems with some report, and
  // warning on all of them is noise.
  const warn = (w, id) => {
    if (!w) return "";
    const bits = [];
    if (w.sev >= 2) bits.push(`${SEV[w.sev]} intel ${fmtAge(Math.max(0, Date.now() / 1000 - w.at))}`);
    // No "this hour" per row, since every figure is hourly.
    if (w.kills || w.pods) {
      bits.push(`${w.kills} ${w.kills === 1 ? "kill" : "kills"}${w.pods ? ` · ${w.pods} pods` : ""}`);
    }
    if (!bits.length) return "";
    // Clickable when there is intel behind it, so the report itself can be read.
    const tag = w.sev >= 2 ? "button" : "span";
    const attr = w.sev >= 2 ? ` data-intel="${id}"` : "";
    return `<${tag} class="rwarn${w.sev >= 3 ? " crit" : ""}"${attr}>${ico("warning")} ${esc(bits.join(" · "))}</${tag}>`;
  };
  const line = (h, i) =>
    `<li class="rhop k${h.kind}"><span class="rmain">` +
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
    // Every way on is the same length, so the choice is the user's and belongs in the list.
    (h.fork?.length
      ? `<span class="rfork" title="Ways on from here, all the same length">${ico("arrows-split")}` +
        h.fork
          .map((alt) => {
            const on = o.path[i + 1] === alt.id;
            return `<button class="ropt${on ? " on" : ""}" data-fork="${h.id}" data-alt-sys="${alt.id}">${esc(alt.name)}</button>`;
          })
          .join("") +
        `</span>`
      : "") +
    `</span>` +
    // One actions button rather than several, to keep the row compact.
    `<button class="ract" data-act="${h.id}" data-at="${i}" title="Actions">${ico("dots-three")}</button>` +
    `</li>`;
  open(
    "route",
    `<h3>${esc(KINDS[kind] ?? "Route")}</h3>` +
      `<div class="rflags">${jumpControls(kind, routeReq?.out)}${holeControl(kind, routeReq?.out)}</div>` +
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
      `<p class="rsave">` +
      (state.snapshot?.meta?.allow_writeback
        ? `<button data-ingame>${ico("map-pin-line")} Set in game</button>`
        : "") +
      `<button data-save>${ico("copy")} Save route</button>` +
      `<button data-load-open>${ico("arrow-square-out")} Load…</button></p>`
  );
  win_wire(kind, onPick);
}

function win_wire(kind, onPick) {
  const win = shells.get("route");
  win?.querySelector("[data-ingame]")?.addEventListener("click", () => {
    const wp = ingameWaypoints(routeOpts[routeAt], state.snapshot?.meta?.player_system ?? null);
    if (wp.length) send({ SetIngameRoute: { waypoints: wp } });
  });
  win?.querySelector("[data-save]")?.addEventListener("click", () => saveRoute(kind));
  win?.querySelector("[data-load-open]")?.addEventListener("click", () => loadRoute(onPick));
  win?.querySelectorAll("[data-fork]").forEach((b) =>
    b.addEventListener("click", () => {
      forkPick.set(Number(b.dataset.fork), Number(b.dataset.altSys));
      fetchRoute();
    })
  );
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

/// The intel behind a route warning, as a modal, since a floating window would land on top of the
/// route window that opened it.
function showIntel(id) {
  document.querySelector(".intelmodal")?.remove();
  const cards = (state.snapshot?.intel?.cards ?? []).filter((c) =>
    (c.report.systems ?? []).some((s) => s.id === id)
  );
  const name = cards[0]?.report?.systems?.find((s) => s.id === id)?.name ?? id;
  modal(
    "intelmodal",
    `<h3>${ico("warning")} ${esc(name)}</h3>` +
      (cards.length
        ? `<div class="feed">${cards.map((c) => card(c, state.snapshot?.intel?.lookups, false, Math.floor(Date.now() / 1000))).join("")}</div>`
        : `<p class="placeholder">Nothing in the feed for this system any more.</p>`)
  );
}

function hopMenu(id, at, kind) {
  const o = routeOpts[routeAt];
  const h = o?.hops?.[at];
  const titans = titansOnce;
  const items = [];
  if (h?.warn?.sev >= 2) items.push(["intel", "Show intel"]);
  // Any anchor but the start can be removed from its own row.
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

/// Systems a capital could stop in between the hops either side of this one. Picking one inserts a
/// waypoint, steering the route without replacing it.
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
  const { wrap, close } = modal(
    "altdlg",
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
        : `<p class="placeholder">Nothing else is in range of both.</p>`)
  );
  wrap.addEventListener("click", (e) => {
    const b2 = e.target.closest("[data-alt]");
    if (!b2) return;
    close();
    const anchors = routeReq.anchors;
    routeReq.anchors = [...anchors.slice(0, -1), Number(b2.dataset.alt), anchors[anchors.length - 1]];
    fetchRoute();
  });
}

/// Save the whole route, avoids and titans included, into the app's settings so the desktop has it.
function saveRoute(kind) {
  const a = routeReq?.anchors ?? [];
  if (a.length < 2) return;
  // Whether this route flies through a hole, not whether the setting allows one.
  const wh = !!routeOpts[routeAt]?.uses_wormhole;
  // Not `prompt`: it blocks the tab, looks like phishing on a phone, and cannot show the expiry.
  const { wrap, close } = modal(
    "altdlg",
    `<h3>Save route</h3>` +
      `<input class="jq" placeholder="Name" autocomplete="off">` +
      (wh
        ? `<p class="rdetour">${ico("warning")} Planned through scanned wormholes. Those chains move, ` +
          `so this is deleted a day after saving rather than quietly becoming wrong.</p>`
        : "") +
      `<p class="rsave"><button data-do-save>Save</button></p>`
  );
  const field = wrap.querySelector(".jq");
  field.focus();
  const go = () => {
    const name = field.value.trim();
    if (!name) return;
    close();
    commitSave(name, kind, a, wh);
  };
  field.addEventListener("keydown", (e) => {
    if (e.key === "Enter") go();
    if (e.key === "Escape") close();
  });
  wrap.addEventListener("click", (e) => {
    if (e.target.closest("[data-do-save]")) go();
  });
}

async function commitSave(name, kind, a, wh) {
  await send({
    SaveRoute: {
      route: {
        name,
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
  const { wrap, close } = modal(
    "altdlg",
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
        : `<p class="placeholder">Nothing saved yet.</p>`)
  );
  wrap.addEventListener("click", async (e) => {
    const f = e.target.closest("[data-forget]");
    if (f) {
      await send({ DeleteRoute: { name: f.dataset.forget } });
      close();
      return;
    }
    const b = e.target.closest("[data-load]");
    if (!b) return;
    const r = rows[Number(b.dataset.load)].route;
    close();
    avoidOnce.clear();
    for (const id of r.avoid ?? []) avoidOnce.add(id);
    titansOnce.clear();
    for (const id of r.titans ?? []) titansOnce.add(id);
    jump = { ...jump, hull: r.hull ?? 0, jdc: r.jdc ?? 5, jfc: r.jfc ?? 5, tstart: !!r.titan_at_start, tself: !!r.titan_self_jump };
    legPick = [];
    showRoute(r.kind, r.anchors, onPick);
  });
}

// One document listener rather than one per chip, since panes re-render constantly.
document.addEventListener("click", (e) => {
  const intel = e.target.closest("[data-intel]");
  if (intel) return showIntel(Number(intel.dataset.intel));
  const mark = e.target.closest("[data-bookmark]");
  if (mark) {
    const id = Number(mark.dataset.bookmark);
    const on = mark.dataset.on === "1";
    // Flipped immediately, since the app's answer only arrives with the next snapshot.
    mark.dataset.on = on ? "0" : "1";
    mark.classList.toggle("on", on);
    mark.title = on ? "Remove bookmark" : "Bookmark this system";
    send({ Bookmark: { id, on } });
    return;
  }
  const sys = e.target.closest("[data-system]");
  if (sys) return showSystem(Number(sys.dataset.system));
  const ship = e.target.closest("[data-ship]");
  if (ship) return showShip(Number(ship.dataset.ship));
  const pilot = e.target.closest("[data-pilot]");
  if (pilot) return showPilot(pilot.dataset.pilot);
});

/// Deep links: `#system/30004759`, `#ship/587`, `#pilot/Some%20Name`. Also how a load-time
/// screenshot opens a dialog, since the harness cannot click.
function fromHash() {
  // `?dlg=system/30004759` opens a dialog while the hash selects a pane.
  const src = /^#(system|ship|pilot)\//.test(location.hash)
    ? location.hash.slice(1)
    : new URLSearchParams(location.search).get("dlg") ?? "";
  // `?dlg=route/<kind>/<from>/<to>`, for load-time screenshots of the route window.
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
