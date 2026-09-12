// The fleet pings pane, reproducing `render_ping` (app.rs). A plain ping is not a degenerate fleet
// ping, so it renders as its own shape.

import { ico, register } from "./app.js";
import { fmtAge } from "./panes-intel.js";

const esc = (s) =>
  String(s ?? "").replace(/[&<>"']/g, (c) =>
    ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]
  );

/// `PapType`: strategic reads red, peacetime amber, anything else is free text and stays weak.
function papTag(pap) {
  if (!pap) return "";
  if (pap === "Strategic") return `<span class="pap strat">STRAT</span>`;
  if (pap === "Peacetime") return `<span class="pap peace">PEACE</span>`;
  const txt = typeof pap === "object" ? Object.values(pap)[0] : pap;
  return `<span class="pap">${esc(txt)}</span>`;
}

function formup(list, systems) {
  return (list ?? [])
    .map((f) => {
      if (typeof f === "object" && "System" in f) {
        const id = f.System;
        const name = systems?.[id] ?? id;
        return `<button class="chip sys" data-system="${id}">${ico("planet")} ${esc(name)}</button>`;
      }
      const t = typeof f === "object" ? Object.values(f)[0] : f;
      return `<span>${esc(t)}</span>`;
    })
    .join(" ");
}

function comms(c) {
  if (!c) return "";
  if (typeof c === "object" && "Mumble" in c) {
    const { channel, link } = c.Mumble;
    return (
      `<a class="chip mumble" href="${esc(link)}">${ico("headset")} Join ${esc(channel)}</a>` +
      `<a class="chip lnk" href="${esc(link)}">${ico("link")}</a>`
    );
  }
  const t = typeof c === "object" ? Object.values(c)[0] : c;
  return `<span>${esc(t)}</span>`;
}

function row(label, body) {
  return body ? `<div class="prow"><span class="plabel">${label}</span><span>${body}</span></div>` : "";
}

function pingCard(entry, now, systems) {
  const p = entry.ping;
  const matched = entry.rule && !entry.suppressed;
  const cls = `ping${matched ? " matched" : ""}`;

  if ("Fleet" in p) {
    const f = p.Fleet;
    return (
      `<article class="${cls}">` +
      `<div class="phead">${ico("megaphone")} <b>Fleet ping</b>` +
      (f.fleet ? ` <span class="fname">${esc(f.fleet)}</span>` : "") +
      papTag(f.pap) +
      `<span class="page">${fmtAge(now - f.timestamp, false)} ago</span></div>` +
      row("FC:", esc(f.fc)) +
      row("Formup:", formup(f.formup, systems)) +
      row("Comms:", comms(f.comms)) +
      row("Doctrine:", f.doctrine ? esc(f.doctrine) : "") +
      `<p class="pbody">${esc(f.description)}</p>` +
      (f.source || f.target
        ? `<div class="pfoot">${esc(f.source ?? "")} ${ico("arrow-right")} ${esc(f.target ?? "")}</div>`
        : "") +
      (matched ? `<div class="pfoot">matched ${esc(entry.rule)}</div>` : "") +
      `</article>`
    );
  }

  const pl = p.Plain;
  return (
    `<article class="${cls}">` +
    `<div class="phead">${ico("megaphone")} <b>${esc(pl.sender ?? "Broadcast")}</b>` +
    `<span class="page">${fmtAge(now - pl.timestamp, false)} ago</span></div>` +
    `<p class="pbody">${esc(pl.text)}</p>` +
    (pl.target ? `<div class="pfoot">${ico("arrow-right")} ${esc(pl.target)}</div>` : "") +
    (matched ? `<div class="pfoot">matched ${esc(entry.rule)}</div>` : "") +
    `</article>`
  );
}

const renderPings = (el, snap) => {
  const pings = snap?.pings?.pings ?? [];
  const systems = snap?.pings?.systems ?? {};
  const now = Math.floor(Date.now() / 1000);
  if (!pings.length) {
    el.innerHTML = `<h2>Fleet pings</h2><p class="placeholder">No pings.</p>`;
    return;
  }
  const newest = [...pings].sort((a, b) => ts(b.ping) - ts(a.ping));
  el.innerHTML = `<h2>Fleet pings</h2>${newest.map((p) => pingCard(p, now, systems)).join("")}`;
};

const ts = (p) => ("Fleet" in p ? p.Fleet.timestamp : p.Plain.timestamp);

register("pings", renderPings);
