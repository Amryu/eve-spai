// The rescue pane, read-only: a page on the LAN may watch the rescue mode but not ping an alliance.

import { esc, ico, register, state } from "./app.js";
import { fmtAge } from "./panes-intel.js";

const side = () => state.snapshot?.rescue ?? null;

function rangeBlock(r) {
  if (!r) return "";
  const jumps = (n) => (n == null ? "no route" : n === 1 ? "1 jump" : `${n} jumps`);
  return (
    `<div class="rsrange">` +
    `<div class="rsbig">${r.ly.toFixed(1)} ly from staging</div>` +
    `<div class="rsub">closest in range: <b>${esc(r.closest)}</b> · ${r.ly_to_target.toFixed(1)} ly out</div>` +
    `<div class="rsub">${jumps(r.ansiblex_jumps)} with ansiblex · ${jumps(r.gate_jumps)} by gate</div>` +
    `</div>`
  );
}

function ping(p, now) {
  const bits = [
    p.system_name ? `<button class="chip sys" data-system="${p.system ?? 0}">${ico("planet")} ${esc(p.system_name)}</button>` : "",
    p.class ? `<span class="chip">${esc(p.class)}</span>` : "",
    p.pilot ? `<span class="chip">${ico("user")} ${esc(p.pilot)}</span>` : "",
    p.cyno ? `<span class="chip">${ico("crosshair-simple")} ${esc(p.cyno)}</span>` : "",
    p.anomaly ? `<span class="chip">${esc(p.anomaly)}</span>` : "",
  ].join("");
  return (
    `<article class="card rsping${p.selected ? " on" : ""}">` +
    `<span class="age">${fmtAge(now - p.at)}</span>` +
    `<span class="rswho">${esc(p.author)}</span>` +
    bits +
    `</article>`
  );
}

register("rescue", (el) => {
  const s = side();
  if (!s) {
    el.innerHTML = `<h2>Rescue</h2><p class="placeholder">Rescue mode is not running.</p>`;
    return;
  }
  const now = Math.floor(Date.now() / 1000);
  el.innerHTML =
    `<h2>Rescue</h2>` +
    `<p class="rshead">` +
    `<span class="${s.active ? "rson" : "rsoff"}">${s.active ? "active" : "standing by"}</span>` +
    (s.test_mode ? ` <span class="rstest">test scenario</span>` : "") +
    ` · op ${s.op_channel} · ${esc(s.doctrine || "no doctrine")}</p>` +
    (s.capital_system
      ? `<p class="rscap">${ico("warning")} <b>${esc(s.capital_system)}</b>` +
        (s.capital_pilot ? ` · ${esc(s.capital_pilot)}` : "") +
        `</p>`
      : "") +
    rangeBlock(s.range) +
    `<div class="feed">` +
    (s.pings.length
      ? s.pings.map((p) => ping(p, now)).join("")
      : `<p class="placeholder">No open calls.</p>`) +
    `</div>`;
});
