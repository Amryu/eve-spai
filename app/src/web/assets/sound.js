// Alert sound, in the browser.
//
// AudioContext behind one GainNode rather than <audio> elements: one mute, lower latency, and far
// better behaviour on iOS.

import { register, renderers, state } from "./app.js";

const MUTE = "spai_muted";
const VOL = "spai_volume";

let ctx = null;
const buffers = new Map();
let gain = null;
let armed = false;
const played = new Set();

const read = (k, dflt) => {
  try {
    const v = localStorage.getItem(k);
    return v === null ? dflt : JSON.parse(v);
  } catch {
    return dflt;
  }
};
const write = (k, v) => {
  try {
    localStorage.setItem(k, JSON.stringify(v));
  } catch {
    // Private browsing. The setting then lasts one session.
  }
};

const audio = { muted: read(MUTE, false), volume: read(VOL, 1) };

/// A mobile browser refuses audio until a user gesture, once per page load. The silent one-sample
/// buffer is the iOS unlock, since resuming the context alone is not enough.
async function arm() {
  if (armed) return true;
  try {
    ctx = new (window.AudioContext || window.webkitAudioContext)();
    gain = ctx.createGain();
    gain.gain.value = audio.muted ? 0 : audio.volume;
    gain.connect(ctx.destination);
    await ctx.resume();
    const silent = ctx.createBufferSource();
    silent.buffer = ctx.createBuffer(1, 1, 22050);
    silent.connect(gain);
    silent.start(0);
    armed = true;
    paint();
    return true;
  } catch {
    return false;
  }
}

async function buffer(name) {
  if (buffers.has(name)) return buffers.get(name);
  const rev = state.snapshot?.meta?.sound_rev ?? 6;
  const res = await fetch(`/assets/sound/${name}-v${rev}.wav`);
  if (!res.ok) return null;
  const buf = await ctx.decodeAudioData(await res.arrayBuffer());
  buffers.set(name, buf);
  return buf;
}

/// Ported from `sound::gate_allows`: one sound per two seconds unless something worse arrives.
const COOLDOWN_MS = 2000;
let lastAt = 0;
let lastSev = 0;

function gateAllows(sev, now) {
  if (!lastAt) return true;
  return now - lastAt >= COOLDOWN_MS || sev > lastSev;
}

async function play(name, sev) {
  if (!armed || audio.muted || !name || name === "off") return;
  const now = Date.now();
  if (!gateAllows(sev, now)) return;
  lastAt = now;
  lastSev = sev;
  const buf = await buffer(name);
  if (!buf) return;
  const src = ctx.createBufferSource();
  src.buffer = buf;
  src.connect(gain);
  src.start(0);
}

function setMuted(m) {
  audio.muted = m;
  write(MUTE, m);
  if (gain) gain.gain.value = m ? 0 : audio.volume;
  paint();
}

const SEV = { Info: 0, Warning: 1, Danger: 2, Critical: 3 };

/// Plays for report ids not yet sounded on this device. The loudest severity in the batch wins.
function onSnapshot(snap) {
  const feed = snap?.alerts?.msg?.feed ?? [];
  let best = null;
  for (const [report, severity] of feed) {
    if (played.has(report.id)) continue;
    played.add(report.id);
    const s = SEV[severity] ?? 0;
    if (!best || s > best.sev) best = { sev: s, severity };
  }
  // A first load must not replay every alert already in the feed as a burst.
  if (best && ready) play(sounds(best.severity), best.sev);
  ready = true;
}

let ready = false;

function sounds(severity) {
  const map = state.snapshot?.meta?.sounds ?? {};
  return map[severity] ?? severity.toLowerCase();
}

function paint() {
  const el = document.getElementById("sound");
  if (!el) return;
  if (!armed) {
    el.innerHTML =
      `<button class="armbtn" data-arm title="Enable sound">` +
      `<span class="armfull">Tap to enable sound</span><span class="armshort">\u{1F507}+</span></button>`;
    el.querySelector("[data-arm]").addEventListener("click", arm);
    return;
  }
  el.innerHTML = `<button class="mutebtn" data-mute aria-pressed="${audio.muted}">${
    audio.muted ? "\u{1F507}" : "\u{1F50A}"
  }</button>`;
  el.querySelector("[data-mute]").addEventListener("click", () => setMuted(!audio.muted));
}

// The alert bar is rebuilt on every render, so the sound control is repainted with it.
const wrapped = renderers.alerts;
register("alerts", (el, snap) => {
  wrapped?.(el, snap);
  onSnapshot(snap);
  paint();
});

paint();
