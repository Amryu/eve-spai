// The intel card, reproducing `intel_row` (app.rs): a flex-wrap row of chips in a fixed order, on a
// severity-tinted card. Every colour is a custom property from /api/theme.css; none are written here.

import { state, ico, register, renderers } from "./app.js";

const CDN = "https://images.evetech.net";

// The app snaps image requests to the CDN's own buckets rather than asking for arbitrary sizes.
const bucket = (px) => [32, 64, 128, 256, 512].find((b) => b >= px) ?? 512;

const esc = (s) =>
  String(s ?? "").replace(/[&<>"']/g, (c) =>
    ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]
  );

export function fmtAge(secs, compact) {
  const s = Math.max(0, Math.floor(secs));
  if (compact) {
    if (s < 60) return `${s}s`;
    if (s < 3600) return `${Math.floor(s / 60)}m`;
    return `${Math.floor(s / 3600)}h`;
  }
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}m ${String(s % 60).padStart(2, "0")}s`;
  const h = Math.floor(m / 60);
  return `${h}h ${String(m % 60).padStart(2, "0")}m`;
}

/// EVE's security ramp, indexed exactly as `security_color` indexes it.
const secVar = (sec) => `var(--sec-${Math.min(10, Math.max(0, Math.round(sec * 10)))})`;

/// A kilometre figure, in AU once it is large enough to be meaningless in km.
///
/// The app shows km throughout; on a phone a seven-digit km reading is just noise, and "2.4 AU" is
/// the number anyone actually wants.
const KM_PER_AU = 149597870.7;

export function fmtDistance(km) {
  const au = km / KM_PER_AU;
  if (au >= 0.1) return `${au.toFixed(1)} AU`;
  return `${Math.round(km)} km`;
}

export function fmtIsk(v) {
  if (v >= 1e9) return `${(v / 1e9).toFixed(1)}b`;
  if (v >= 1e6) return `${(v / 1e6).toFixed(1)}m`;
  if (v >= 1e3) return `${(v / 1e3).toFixed(1)}k`;
  return String(v);
}

function typeIcon(r) {
  if (r.clear) return ["check-circle", "var(--chip-clear)"];
  if (r.channel === "zKill" || r.channel === "zkill") return ["crosshair", "var(--chip-kill-icon)"];
  if (r.killmail) return ["skull", null];
  if (r.spike || r.camp || r.bubble || r.cyno || r.dropper || r.help) return ["warning-octagon", null];
  if (r.no_visual) return ["eye-slash", null];
  if (r.systems.length || r.count != null) return ["warning", null];
  return ["info", null];
}

const chip = (body, fg, bg, extra = "") =>
  `<span class="chip" style="color:${fg};background:${bg};${extra}">${body}</span>`;

function jumpText(from) {
  if (from == null) return "-";
  return from === 0 ? "here" : `${from}j`;
}

/// `Gates` reads as your own corp blue; a bridge-shortened or bridge-only number reads purple,
/// because it is a number a hostile does not face.
const viaVar = (via) => (via === "Gates" ? "var(--corp)" : "var(--alliance)");

function flagTags(r, isKill) {
  const t = [];
  const add = (txt, v) => t.push(`<span class="tag" style="color:${v}">${txt}</span>`);
  if (r.status) add("STATUS?", "var(--chip-probes)");
  if (r.clear) add("CLEAR", "var(--chip-clear)");
  if (r.no_visual) add("NV", "var(--warning)");
  if (r.spike) add("SPIKE", "var(--hostile)");
  if (r.camp) add("CAMP", "var(--hostile)");
  if (r.help) add("HELP", "var(--hostile)");
  if (r.bubble) add("BUBBLE", "var(--warning)");
  if (r.nullified) add("NULLIFIED", "var(--warning)");
  // A zKill card is already a kill: it has the crosshair icon and its own dark card. The tag is
  // only worth saying on a chat report that mentions one.
  if (r.killmail && !isKill) add("KILL", "var(--hostile)");
  if (r.cyno) add("CYNO", "var(--hostile)");
  if (r.dropper) add("DROPPER", "var(--hostile)");
  if (r.cap_tackled) add("CAP TACKLED", "var(--hostile)");
  if (r.wormhole) add(r.wh_type ? `WH ${esc(r.wh_type)}` : "WORMHOLE", "var(--alliance)");
  if (r.ess) add(r.ess_time ? `ESS ${esc(r.ess_time)}` : "ESS", "var(--warning)");
  if (r.filament) add("FILAMENT", "var(--warning)");
  if (r.diamond_rats) add("◆ Rats ◆", "var(--hostile)");
  for (const [kind, code] of r.anom_sigs ?? []) {
    add(`${kind === "Anomaly" ? "Anom" : "Sig"} ${esc(code)}`, "var(--warning)");
  }
  return t.join("");
}

export function card(c, lookups, compact, now) {
  const r = c.report;
  const isKill = r.channel === "zKill" || r.channel === "zkill";
  const stale = state.snapshot?.meta?.intel_ttl_secs > 0 &&
    now - r.received > state.snapshot.meta.intel_ttl_secs;
  const [icon, iconVar] = typeIcon(r);
  const sev = `var(--sev-${String(c.severity).toLowerCase()})`;
  const parts = [];

  parts.push(`<span class="tico" style="color:${iconVar ?? sev}">${ico(icon)}</span>`);
  parts.push(`<span class="age">${fmtAge(now - r.received, compact)}</span>`);

  // The character ring only exists when there is more than one character to confuse, matching
  // `CardChars`; with one, the card draws the plain number it always did.
  const hops = c.chars?.hops ?? [];
  if (hops.length) {
    parts.push(
      hops
        .map((h) => {
          const img = h.id
            ? `<img src="${CDN}/characters/${h.id}/portrait?size=${bucket(compact ? 16 : 20)}" alt="">`
            : ico("user");
          return chip(
            `${img}<span class="jn">${jumpText(h.jumps)}</span>`,
            viaVar(typeof h.via === "string" ? h.via : "Bridge"),
            "var(--surface-hi)",
            `title="${esc(h.name)}"`
          );
        })
        .join("")
    );
  } else {
    parts.push(`<span class="jn plain" style="color:${viaVar(typeof c.via === "string" ? c.via : "Bridge")}">${jumpText(c.from_you)}</span>`);
  }

  for (const sys of r.systems) {
    const col = secVar(sys.security);
    parts.push(
      `<button class="chip sys" data-system="${sys.id}" style="color:${col};background:color-mix(in srgb, ${col} 28%, var(--bg))">${ico("planet")} ${esc(sys.name)}</button>`
    );
  }
  if (r.near_celestial) {
    const [label, km] = r.near_celestial;
    parts.push(chip(`${ico("map-pin-line")} ${esc(label)} <b>${fmtDistance(km)}</b>`, "var(--chip-celestial)", "var(--chip-celestial-bg)"));
  }
  if (r.count != null) {
    parts.push(chip(`${ico("users")} ${r.count}${r.count_plus ? "+" : ""}`, "#fff", "var(--hostile)"));
  }
  if (r.isk != null) {
    parts.push(chip(`${ico("coins")} ${fmtIsk(r.isk)}`, "var(--chip-isk)", "var(--chip-isk-bg)"));
  }
  for (const [name] of r.structures ?? []) {
    parts.push(chip(`${ico("castle-turret")} ${esc(name)}`, "var(--chip-structure)", "var(--chip-structure-bg)"));
  }
  for (const cel of r.celestials ?? []) {
    parts.push(chip(esc(cel), "var(--chip-celestial)", "var(--chip-celestial-bg)"));
  }
  if (r.probes) {
    parts.push(chip(`${ico("magnifying-glass")} ${esc(r.probes)}`, "var(--chip-probes)", "var(--chip-probes-bg)"));
  }
  for (const ship of r.ships) {
    const px = bucket(compact ? 16 : 24);
    parts.push(
      `<button class="chip ship" data-ship="${ship.id}"><img src="${CDN}/types/${ship.id}/icon?size=${px}" alt=""><b>${esc(ship.name)}</b></button>`
    );
  }
  for (const a of r.ambiguous_ships ?? []) {
    parts.push(`<span class="chip amb">${esc(a.abbrev)}?</span>`);
  }
  for (const cls of r.classes ?? []) {
    parts.push(`<span class="chip cls">${esc(cls)}</span>`);
  }
  for (const t of r.tackled_targets ?? []) {
    parts.push(chip(`${esc(t)} <b>TACKLED</b>`, "var(--chip-tackled)", "var(--chip-tackled-bg)"));
  }

  const resolved = lookups?.resolved_pilots ?? {};
  const uncertain = new Set((lookups?.uncertain ?? []).map((u) => u.toLowerCase()));
  for (const name of r.pilots ?? []) {
    const id = resolved[name];
    // `intel_row` skips a pilot it cannot resolve, so the two feeds show the same names.
    if (id == null) continue;
    const un = uncertain.has(name.toLowerCase());
    const px = bucket(compact ? 16 : 20);
    parts.push(
      `<button class="chip pilot${un ? " un" : ""}" data-pilot="${esc(name)}">` +
        `<img src="${CDN}/characters/${id}/portrait?size=${px}" alt="">${esc(name)}` +
        `${un ? '<b class="q">?</b>' : ""}</button>`
    );
  }
  for (const g of r.gates ?? []) {
    parts.push(`<span class="chip gate">${ico("sign-in")} ${esc(g)} gate</span>`);
  }
  for (const link of r.links ?? []) {
    const kind = link.kind === "Killmail" ? "zKill" : link.kind === "BattleReport" ? "BR" : "dscan";
    const icn = link.kind === "Killmail" ? "arrow-square-out" : link.kind === "BattleReport" ? "chart-line" : "scan";
    const col = link.kind === "Killmail" ? "var(--hostile)" : "var(--accent)";
    parts.push(`<a class="chip lnk" style="color:${col}" href="${esc(link.url)}" target="_blank" rel="noopener">${ico(icn)} ${kind}</a>`);
  }
  parts.push(flagTags(r, isKill));
  if (r.movement) {
    const j = r.movement.jumps != null ? ` (${r.movement.jumps}j)` : "";
    parts.push(`<span class="move">${ico("arrow-left")} ${esc(r.movement.from)}${j}</span>`);
  }
  if (stale) parts.push(`<span class="move">outdated</span>`);

  const fill = isKill
    ? "var(--chip-kill-card-bg)"
    : `color-mix(in srgb, ${sev} ${stale ? 5 : 13}%, transparent)`;
  const footer =
    !isKill && r.reporter
      ? `<div class="rep">${esc(r.reporter)} · ${esc(r.channel)}</div>`
      : "";
  return `<article class="card" style="background:${fill}">${parts.join("")}${footer}</article>`;
}

/// The app's own filters: type, free text, and a jump ceiling.
export function matches(c, f) {
  const r = c.report;
  if (!r.systems.length && !(r.gates ?? []).length) return false;
  switch (f.type) {
    case "Hostile":
      if (r.clear || r.killmail || !(r.count != null || r.systems.length)) return false;
      break;
    case "Clear":
      if (!r.clear) return false;
      break;
    case "Kill":
      if (!r.killmail) return false;
      break;
    case "Threat":
      if (!(r.spike || r.camp || r.bubble || r.cyno || r.dropper || r.help || r.tackled || r.cap_tackled))
        return false;
      break;
  }
  if (f.jumps > 0 && !(c.from_you != null && c.from_you <= f.jumps)) return false;
  if (f.q) {
    const q = f.q.toLowerCase();
    const hay = [r.text, r.channel, ...r.systems.map((s) => s.name)].join(" ").toLowerCase();
    if (!hay.includes(q)) return false;
  }
  return true;
}

export const filter = { type: "All", q: "", jumps: 0 };

const TYPES = ["All", "Hostile", "Clear", "Kill", "Threat"];

const renderIntel = (el, snap) => {
  const pane = snap?.intel;
  const now = Math.floor(Date.now() / 1000);
  const compact = !!snap?.meta?.compact;
  const cards = (pane?.cards ?? []).filter((c) => matches(c, filter));

  el.innerHTML =
    `<h2>Intel</h2>` +
    `<div class="toolbar">` +
    TYPES.map(
      (t) => `<button class="tf${t === filter.type ? " on" : ""}" data-type="${t}">${t}</button>`
    ).join("") +
    `<label class="jf">${ico("arrow-right")}<input type="number" min="0" value="${filter.jumps}" data-jumps></label>` +
    `<label class="qf">${ico("magnifying-glass")}<input type="search" placeholder="system, text, or channel" value="${esc(filter.q)}" data-q></label>` +
    `</div>` +
    `<p class="count">${cards.length} reports</p>` +
    `<div class="feed${compact ? " compact" : ""}">` +
    cards.map((c) => card(c, pane?.lookups, compact, now)).join("") +
    `</div>`;

  el.querySelectorAll("[data-type]").forEach((b) =>
    b.addEventListener("click", () => {
      filter.type = b.dataset.type;
      renderIntel(el, state.snapshot);
    })
  );
  const jf = el.querySelector("[data-jumps]");
  jf?.addEventListener("change", () => {
    filter.jumps = Number(jf.value) || 0;
    renderIntel(el, state.snapshot);
  });
  const qf = el.querySelector("[data-q]");
  qf?.addEventListener("input", () => {
    filter.q = qf.value;
    const at = qf.selectionStart;
    renderIntel(el, state.snapshot);
    const again = el.querySelector("[data-q]");
    again.focus();
    again.setSelectionRange(at, at);
  });
};

register("intel", renderIntel);
