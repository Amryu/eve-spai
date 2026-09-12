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

function shell() {
  if (dlg) return dlg;
  dlg = document.createElement("div");
  dlg.className = "modal";
  dlg.hidden = true;
  dlg.innerHTML = `<div class="mback"></div><div class="mpanel" role="dialog" aria-modal="true"></div>`;
  dlg.querySelector(".mback").addEventListener("click", close);
  document.body.append(dlg);
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape") close();
  });
  return dlg;
}

export function close() {
  if (dlg) dlg.hidden = true;
}

function open(html) {
  const d = shell();
  d.querySelector(".mpanel").innerHTML =
    `<button class="mclose" aria-label="Close">${ico("x")}</button>${html}`;
  d.querySelector(".mclose").addEventListener("click", close);
  d.hidden = false;
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
  const m = /^#(system|ship|pilot)\/(.+)$/.exec(location.hash);
  if (!m) return;
  const [, kind, raw] = m;
  const arg = decodeURIComponent(raw);
  if (kind === "system") showSystem(Number(arg));
  else if (kind === "ship") showShip(Number(arg));
  else showPilot(arg);
}

window.addEventListener("hashchange", fromHash);
fromHash();
