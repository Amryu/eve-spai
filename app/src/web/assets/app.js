// The shell. Panes arrive in their own tickets; this establishes the page, the palette and the
// poll, so the server can be verified end to end on its own.

export const state = { snapshot: null, icons: {} };

const $ = (id) => document.getElementById(id);

function setStatus(text, kind) {
  const el = $("status");
  el.textContent = text;
  el.dataset.state = kind;
}

export function ico(name) {
  const glyph = state.icons[name];
  if (!glyph) return "";
  const span = document.createElement("span");
  span.className = "ph";
  span.textContent = glyph;
  return span.outerHTML;
}

function render() {
  const s = state.snapshot;
  const counts = [
    ["intel", s?.intel?.cards?.length ?? 0],
    ["alerts", s?.alerts?.msg?.feed?.length ?? 0],
    ["pings", s?.pings?.pings?.length ?? 0],
    ["systems with intel", s?.map?.intel?.length ?? 0],
  ];
  $("panes").innerHTML = `
    <p class="placeholder">Paired. The feed is reaching this device; the panes that draw it land in
    the tickets after this one.</p>
    <ul class="counts">
      ${counts.map(([k, n]) => `<li>${ico("broadcast")} ${k} <b>${n}</b></li>`).join("")}
    </ul>
    <p class="placeholder">sequence <code>${s?.seq ?? 0}</code></p>`;
}

async function poll() {
  try {
    const r = await fetch("/api/snapshot", { cache: "no-store" });
    if (!r.ok) throw new Error(r.status);
    state.snapshot = await r.json();
    setStatus("live", "live");
  } catch {
    setStatus("no connection", "down");
  }
  render();
}

async function main() {
  try {
    state.icons = await (await fetch("/api/icons.json")).json();
  } catch {
    // A missing icon renders as nothing, which is better than a tofu square.
  }
  await poll();
  // Placeholder cadence. WEB-004 replaces this with a push channel, at which point the page stops
  // asking and starts being told.
  setInterval(poll, 2000);
}

main();
