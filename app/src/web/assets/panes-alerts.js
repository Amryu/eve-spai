// The alerts pane: the cards a rule fired on, newest first, each carrying the severity that fired it.
// The card body is the intel card, so an alert and its feed entry cannot look like different events.

import { ico, state, register } from "./app.js";
import { card } from "./panes-intel.js";

/// Per device, so two people watching the same app do not clear each other's unseen markers.
const SEEN = "spai_seen_alerts";

function seen() {
  try {
    return new Set(JSON.parse(localStorage.getItem(SEEN) ?? "[]"));
  } catch {
    return new Set();
  }
}

function markSeen(ids) {
  try {
    // The alert feed is capped at 100, so remembering far past that is pure growth.
    localStorage.setItem(SEEN, JSON.stringify([...ids].slice(-300)));
  } catch {
    // Private browsing refuses storage. Everything then reads as unseen, which is the safe side.
  }
}

const renderAlerts = (el, snap) => {
  const msg = snap?.alerts?.msg;
  const feed = msg?.feed ?? [];
  const now = Math.floor(Date.now() / 1000);
  const compact = !!snap?.meta?.compact;

  const tools =
    `<div class="toolbar"><button class="tf" data-notes-manage="pilot">${ico("tag")} Pilot tags</button></div>`;
  if (!feed.length) {
    el.innerHTML = `<h2>Alerts</h2>${tools}<p class="placeholder">Nothing has fired.</p>`;
    return;
  }

  const was = seen();
  const ids = new Set();
  // `feed` is oldest-first, as the overlay keeps it; the pane reads newest-first.
  const rows = feed
    .map(([report, severity], i) => ({ report, severity, i }))
    .reverse()
    .map(({ report, severity, i }) => {
      ids.add(report.id);
      const fresh = !was.has(report.id);
      const c = {
        report,
        severity,
        from_you: msg.from_you?.[i] ?? null,
        via: msg.via?.[i] ?? "Gates",
        chars: msg.chars?.[i] ?? { hops: [], selected: null },
      };
      // No severity header or age: the card already shows both.
      return (
        `<div class="alert${fresh ? " fresh" : ""}">` +
        card(c, { resolved_pilots: msg.resolved_pilots, uncertain: msg.uncertain }, compact, now) +
        `</div>`
      );
    })
    .join("");

  el.innerHTML = `<h2>Alerts</h2>${tools}<div class="feed${compact ? " compact" : ""}">${rows}</div>`;
  markSeen(ids);
};

register("alerts", renderAlerts);
