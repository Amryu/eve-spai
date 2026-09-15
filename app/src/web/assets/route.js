// Routing from the map: how far it is, and what to do about it.
//
// Distances are computed in the browser from the map geometry already loaded. Single-source, once
// per drag, so each hover is an array lookup.

import { ico } from "./app.js";

const UNREACHED = -1;

/// Breadth-first distance from one node to every other, over `extra` as well as the gates.
function spread(geo, from, extra) {
  const n = geo.nodes.length;
  const dist = new Int32Array(n).fill(UNREACHED);
  if (from == null || from < 0 || from >= n) return dist;
  if (!geo.adj || geo.adjKind !== (extra ? "bridged" : "gates")) {
    const adj = Array.from({ length: n }, () => []);
    for (const [a, b] of geo.edges) {
      adj[a].push(b);
      adj[b].push(a);
    }
    if (extra) {
      for (const [a, b] of geo.bridges ?? []) {
        adj[a].push(b);
        adj[b].push(a);
      }
    }
    geo.adj = adj;
    geo.adjKind = extra ? "bridged" : "gates";
  }
  const adj = geo.adj;
  dist[from] = 0;
  // Read head instead of shift(), which is quadratic over 5000 nodes.
  const q = [from];
  for (let head = 0; head < q.length; head++) {
    const v = q[head];
    for (const w of adj[v]) {
      if (dist[w] === UNREACHED) {
        dist[w] = dist[v] + 1;
        q.push(w);
      }
    }
  }
  return dist;
}

/// Gate jumps and bridge-assisted jumps from one system to every other. Both are shown, and they
/// often differ a lot.
export function reach(geo, from) {
  return { gates: spread(geo, from, false), bridged: spread(geo, from, true) };
}

/// From the real 3D positions, since two systems adjacent on the projection can be far apart in z.
export function lightYears(geo, a, b) {
  const p = geo.pos3?.[a];
  const q = geo.pos3?.[b];
  if (!p || !q) return null;
  return Math.hypot(p[0] - q[0], p[1] - q[1], p[2] - q[2]) / 100;
}

/// Radial menu options around the drop point, each the same distance from the pointer.
const OPTIONS = [
  ["gate", "Gate route", "sign-in", "Route by gates, and set it as the destination"],
  ["jump", "Jump route", "spiral", "Plan it as capital jumps"],
  ["titan", "Titan route", "crosshair-simple", "Fewest jumps, bridged from as close as possible"],
  ["cancel", "Cancel", "x", ""],
];

/// Dismisses the open menu. Held rather than found in the DOM, because removing only the element
/// would leave its outside-click listener behind.
let closeMenu = null;

/// A `null` entry is a separator. There is no backdrop, so a pointerdown outside dismisses it.
export function menu(x, y, items, pick) {
  closeMenu?.();
  const el = document.createElement("div");
  el.className = "ctxmenu";
  el.innerHTML = items
    .map((it) =>
      it === null
        ? `<hr>`
        : `<button data-pick="${it[0]}"${it[2] ? ` class="${it[2]}"` : ""}>${it[1]}</button>`
    )
    .join("");
  document.body.append(el);
  // Placed after measuring, so a menu opened near an edge stays on screen.
  const r = el.getBoundingClientRect();
  el.style.left = `${Math.max(4, Math.min(x, window.innerWidth - r.width - 4))}px`;
  el.style.top = `${Math.max(4, Math.min(y, window.innerHeight - r.height - 4))}px`;
  const done = (kind) => {
    el.remove();
    document.removeEventListener("pointerdown", away, true);
    if (closeMenu === dismiss) closeMenu = null;
    if (kind) pick(kind);
  };
  const dismiss = () => done(null);
  closeMenu = dismiss;
  const away = (e) => {
    if (!e.target.closest(".ctxmenu")) done(null);
  };
  el.addEventListener("click", (e) => {
    const b = e.target.closest("[data-pick]");
    if (b) done(b.dataset.pick);
  });
  setTimeout(() => document.addEventListener("pointerdown", away, true), 0);
  return el;
}

export function radial(x, y, pick) {
  document.querySelector(".radial")?.remove();
  const el = document.createElement("div");
  el.className = "radial";
  el.style.left = `${x}px`;
  el.style.top = `${y}px`;
  const r = 62;
  el.innerHTML = OPTIONS.map(([kind, label, icon, tip], i) => {
    // Clockwise from the top, in `OPTIONS` order.
    const a = (i / OPTIONS.length) * Math.PI * 2 - Math.PI / 2;
    const dx = Math.round(Math.cos(a) * r);
    const dy = Math.round(Math.sin(a) * r);
    return (
      `<button class="rbtn${kind === "cancel" ? " cancel" : ""}" data-pick="${kind}" title="${tip}"` +
      ` style="transform:translate(-50%,-50%) translate(${dx}px,${dy}px)">${ico(icon)}<span>${label}</span></button>`
    );
  }).join("");
  document.body.append(el);

  const done = (kind) => {
    el.remove();
    document.removeEventListener("pointerdown", away, true);
    if (kind && kind !== "cancel") pick(kind);
  };
  const away = (e) => {
    if (!e.target.closest(".radial")) done(null);
  };
  el.addEventListener("click", (e) => {
    const b = e.target.closest("[data-pick]");
    if (b) done(b.dataset.pick);
  });
  // Capture, or the map's own pointerdown starts a pan under the menu first.
  setTimeout(() => document.addEventListener("pointerdown", away, true), 0);
  return el;
}
