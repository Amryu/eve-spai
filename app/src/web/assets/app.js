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

let es = null;
let stale = null;

// Panes arrive whole or not at all: the server sends a pane only when it changed, so merging is a
// field-by-field replace rather than a patch.
function merge(update) {
  const s = state.snapshot ?? { seq: 0, gen: update.gen };
  if (update.gen !== s.gen) {
    state.snapshot = update;
    return;
  }
  for (const pane of ["intel", "alerts", "pings", "map", "meta"]) {
    if (update[pane] !== undefined) s[pane] = update[pane];
  }
  s.seq = update.seq;
  state.snapshot = s;
}

function markStale() {
  clearTimeout(stale);
  // A stream that has gone quiet past the keepalive is not a stream that is working.
  stale = setTimeout(() => setStatus("stale", "stale"), 20000);
}

function connect() {
  es?.close();
  es = new EventSource("/api/events");
  es.onopen = () => {
    setStatus("live", "live");
    markStale();
  };
  es.onmessage = (e) => {
    merge(JSON.parse(e.data));
    setStatus("live", "live");
    markStale();
    render();
  };
  // A reset means the server could not fill the gap, so what is held is thrown away.
  es.addEventListener("reset", (e) => {
    state.snapshot = JSON.parse(e.data);
    setStatus("live", "live");
    markStale();
    render();
  });
  es.onerror = () => setStatus("reconnecting", "down");
}

// iOS kills the stream when the tab backgrounds and sometimes hands back a dead one on return, so
// the page reconnects deliberately rather than trusting what it is given.
document.addEventListener("visibilitychange", () => {
  if (document.visibilityState === "visible" && es?.readyState !== EventSource.OPEN) connect();
});

async function main() {
  try {
    state.icons = await (await fetch("/api/icons.json")).json();
  } catch {
    // A missing icon renders as nothing, which is better than a tofu square.
  }
  render();
  connect();
}

main();
