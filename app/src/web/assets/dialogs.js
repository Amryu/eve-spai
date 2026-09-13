// Dialogs render here, in the browser, not as a window on the desktop. A tap on a phone must not
// raise a viewport on a machine in another room, which is why a badge tap is a plain GET and never
// an IntelClick.

import { ico, state } from "./app.js";

const esc = (s) =>
  String(s ?? "").replace(/[&<>"']/g, (c) =>
    ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]
  );

const secVar = (sec) => `var(--sec-${Math.min(10, Math.max(0, Math.round(sec * 10)))})`;

let dlg = null;

/// A floating window, not a modal.
///
/// A modal takes the whole page to show one system, which is wrong for something you open while
/// reading the map: the map is the context. This floats, can be dragged, and does not block anything
/// behind it. Escape and its close button dismiss it; clicking the map does not, because clicking
/// the map is how you open the next one.
function shell() {
  if (dlg) return dlg;
  dlg = document.createElement("div");
  dlg.className = "float";
  dlg.hidden = true;
  dlg.innerHTML = `<div class="mpanel" role="dialog"></div>`;
  document.body.append(dlg);
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape") close();
  });
  dragify(dlg);
  // A pane that changes size moves the map with it, and the window was parked against where the map
  // used to be. Same for the map arriving after the window did.
  const repark = () => {
    if (!dlg.hidden) place(dlg);
  };
  window.addEventListener("resize", repark);
  window.addEventListener("spai:map", repark);
  return dlg;
}

/// Drag by the panel's own chrome. Pointer events so a phone can move it too, and clamped on release
/// so it cannot be parked off screen.
function dragify(node) {
  let from = null;
  node.addEventListener("pointerdown", (e) => {
    if (e.target.closest("button, a, input")) return;
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

export function close() {
  if (dlg) dlg.hidden = true;
}

/// Park the window at the top right of the map, below whatever controls are showing.
///
/// The canvas's own top is not low enough: on a narrow pane the layer panel is an overlay sitting
/// over the top of the canvas, so a window aligned to the canvas covers the filters. This clears
/// whichever of the toolbar and the open panel reaches furthest down, so it works whether the panel
/// is in the flow or floating.
function place(d, tries = 10) {
  if (d.dataset.moved || d.hidden) return;
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
  d.style.left = "auto";
  d.style.top = `${Math.round(Math.min(top + 8, r.bottom - 80))}px`;
  d.style.right = `${Math.round(window.innerWidth - r.right + 8)}px`;
  d.style.bottom = "auto";
}

function open(html) {
  const d = shell();
  d.querySelector(".mpanel").innerHTML =
    `<button class="mclose" aria-label="Close">${ico("x")}</button>${html}`;
  d.querySelector(".mclose").addEventListener("click", close);
  d.hidden = false;
  place(d);
}

function rows(pairs) {
  return pairs
    .filter(([, v]) => v !== null && v !== undefined && v !== "" && v !== false)
    .map(([k, v]) => `<div class="mrow"><span>${k}</span><span>${v}</span></div>`)
    .join("");
}

async function showSystem(id) {
  open(`<h3>System</h3><p class="placeholder">Loading.</p>`);
  const r = await fetch(`/api/system/${id}`);
  if (!r.ok) return open(`<h3>System</h3><p class="placeholder">Not in the star map.</p>`);
  const s = await r.json();
  const jumps = s.jumps_from_you == null ? "no route" : s.jumps_from_you === 0 ? "you are here" : `${s.jumps_from_you} jumps`;
  open(
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
  open(`<h3>Ship</h3><p class="placeholder">Loading.</p>`);
  const r = await fetch(`/api/ship/${id}`);
  if (!r.ok) return open(`<h3>Ship</h3><p class="placeholder">Not in the static data.</p>`);
  const s = await r.json();
  const res = (label, v) =>
    `<div class="mrow"><span>${label}</span><span class="mres">` +
    ["EM", "Th", "Ki", "Ex"].map((t, i) => `<b>${t} ${v[i]}%</b>`).join("") +
    `</span></div>`;
  open(
    `<h3><img class="mhull" src="https://images.evetech.net/types/${s.id}/render?size=128" alt=""> ${esc(s.name)}</h3>` +
      `<p class="mgroup">${esc(s.group)}</p>` +
      rows([
        ["Shield", Math.round(s.shield_hp)],
        ["Armor", Math.round(s.armor_hp)],
        ["Hull", Math.round(s.hull_hp)],
        ["Turrets", s.turrets || null],
        ["Launchers", s.launchers || null],
        ["Drone bay", s.drone_cap ? `${Math.round(s.drone_cap)} m³` : null],
        ["Drone bandwidth", s.drone_bw ? `${Math.round(s.drone_bw)} Mbit` : null],
      ]) +
      res("Shield resists", s.shield_resist) +
      res("Armor resists", s.armor_resist) +
      res("Hull resists", s.hull_resist) +
      (s.traits.length ? `<ul class="mtraits">${s.traits.map((t) => `<li>${esc(t)}</li>`).join("")}</ul>` : "")
  );
}

/// The uncertain-pilot prompt, carrying the app's own wording. Getting this wrong hides real pilots,
/// which is the failure the whole "?" mechanism exists to avoid.
function showVerdict(name) {
  open(
    `<h3>Uncertain pilot (?)</h3>` +
      `<p>This name was parsed out of chat but could not be confirmed as a character. ` +
      `Marking it correctly keeps the feed honest.</p>` +
      `<p class="mname">${esc(name)}</p>` +
      `<div class="mbtns">` +
      `<button class="mok" data-verdict="real">Real pilot</button>` +
      `<button class="mno" data-verdict="hide">Not a pilot (hide)</button>` +
      `</div>`
  );
  dlg.querySelectorAll("[data-verdict]").forEach((b) =>
    b.addEventListener("click", async () => {
      await send({ Verdict: { name, hidden: b.dataset.verdict === "hide" } });
      close();
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
    `<h3>${ico("user")} ${esc(name)}</h3>` +
      (id
        ? `<img class="mport" src="https://images.evetech.net/characters/${id}/portrait?size=256" alt="">` +
          `<p><a href="https://zkillboard.com/character/${id}/" target="_blank" rel="noopener">` +
          `${ico("arrow-square-out")} zKillboard</a></p>`
        : `<p class="placeholder">Not resolved.</p>`)
  );
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
