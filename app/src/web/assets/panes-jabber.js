// The jabber pane: the Convos list on the left, the selected conversation on the right.
//
// No tabs. The app grew a tab bar over the same list and it was possible to have a conversation
// selected in one and showing in the other; here the list is the selection, which is the whole point
// of the Convos rework this mirrors.

import { register, state, ico } from "./app.js";
import { esc } from "./panes-intel.js";

const KEY = "spai_jabber";

/// Which conversation is open, per device.
///
/// Not pushed from the app: two people reading the same feed from two phones are not reading the
/// same conversation, and the desktop is a third reader again.
let sel = null;
try {
  sel = localStorage.getItem(KEY);
} catch {
  // Private browsing. The selection then lasts one session.
}

function remember(jid) {
  sel = jid;
  try {
    localStorage.setItem(KEY, jid ?? "");
  } catch {
    /* see above */
  }
}

/// The open conversation's messages, and the `last_at` they were fetched for.
let chat = { jid: null, msgs: [], at: 0 };
let loading = false;

function side() {
  return state.snapshot?.jabber ?? { configured: false, connected: false, convos: [] };
}

function convo(jid) {
  return side().convos.find((c) => c.jid === jid) ?? null;
}

/// Fetch the open conversation, but only when it has actually moved.
///
/// The list carries `last_at`, so the page already knows whether there is anything new. Polling on a
/// timer would refetch a room's backlog every few seconds to learn nothing.
async function sync(force) {
  const c = convo(sel);
  if (!c) return;
  if (!force && chat.jid === c.jid && chat.at === c.last_at) return;
  if (loading) return;
  loading = true;
  try {
    const r = await fetch(`/api/jabber/chat?jid=${encodeURIComponent(c.jid)}`);
    const out = await r.json();
    chat = { jid: c.jid, msgs: out.msgs ?? [], at: c.last_at };
    paint();
  } catch {
    // Offline or unpaired. The list still draws; the conversation stays as it was.
  } finally {
    loading = false;
  }
}

export function open(jid) {
  remember(jid);
  chat = { jid: null, msgs: [], at: 0 };
  paint();
  sync(true);
}

const stamp = (at) => {
  const d = new Date(at * 1000);
  return `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
};

/// The part of a JID anyone says out loud. A sender arrives as a bare JID in a DM and as a nick in a
/// room, and "@goonfleet.com" after every line in a conversation with one other person is noise.
const who = (from) => String(from ?? "").split("@")[0];

function row(c) {
  const dot = c.room
    ? `<span class="jico">${ico("users-three")}</span>`
    : `<span class="jdot" style="background:${c.presence ?? "var(--muted)"}"></span>`;
  const badge = c.unread
    ? `<span class="jcount${c.mention ? " mention" : ""}">${c.unread > 99 ? "99+" : c.unread}</span>`
    : "";
  return (
    `<button class="jrow${c.jid === sel ? " on" : ""}${c.mention ? " mentioned" : ""}" data-convo="${esc(c.jid)}" title="${esc(c.jid)}">` +
    `${dot}<span class="jname">${esc(c.name)}</span>${badge}</button>`
  );
}

function list() {
  const convos = side().convos.filter((c) => c.listed);
  const dms = convos.filter((c) => !c.room);
  const rooms = convos.filter((c) => c.room);
  const section = (title, rows, kind) =>
    `<div class="jsec">${title}</div>${rows.map(row).join("")}` +
    `<button class="jstart" data-start="${kind}">${ico("plus")} ${kind === "dm" ? "Start a DM" : "Join a room"}</button>`;
  return section("Direct messages", dms, "dm") + section("Rooms", rooms, "room");
}

function body() {
  const c = convo(sel);
  if (!side().configured) {
    return `<p class="placeholder">Jabber is not configured in the app.</p>`;
  }
  if (!c) {
    return `<p class="placeholder">Pick a conversation.</p>`;
  }
  const lines = chat.msgs
    .map(
      (m) =>
        `<div class="jmsg${m.me ? " me" : ""}">` +
        `<span class="jwho">${esc(who(m.from))}</span>` +
        `<span class="jat" title="${esc(new Date(m.at * 1000).toLocaleString())}">${stamp(m.at)}</span>` +
        `<span class="jbody">${esc(m.body)}</span></div>`
    )
    .join("");
  const write = state.snapshot?.meta?.allow_writeback
    ? `<form class="jsend"><input name="body" autocomplete="off" placeholder="Message ${esc(c.name)}"><button type="submit">${ico("paper-plane-right")}</button></form>`
    : "";
  return (
    `<div class="jhead">${c.room ? ico("users-three") : ico("chat-circle-dots")} <b>${esc(c.name)}</b></div>` +
    `<div class="jlog">${lines || `<p class="placeholder">No messages yet.</p>`}</div>` +
    write
  );
}

let el = null;

function paint() {
  if (!el) return;
  // The log is the one thing worth keeping a scroll position for, and it is nearly always pinned to
  // the bottom, so that is what is restored.
  const log = el.querySelector(".jlog");
  const pinned = !log || log.scrollTop + log.clientHeight >= log.scrollHeight - 24;
  const draft = el.querySelector(".jsend input")?.value ?? "";
  el.innerHTML =
    `<h2>Jabber</h2><div class="jwrap"><div class="jlist">${list()}</div><div class="jchat">${body()}</div></div>`;
  const now = el.querySelector(".jlog");
  if (now && pinned) now.scrollTop = now.scrollHeight;
  else if (now && log) now.scrollTop = log.scrollTop;
  const input = el.querySelector(".jsend input");
  if (input && draft) input.value = draft;
}

register("jabber", (node) => {
  el = node;
  paint();
  sync(false);
});

/// One listener, on the pane, for every row there will ever be: the list is rebuilt on every push.
document.addEventListener("click", (e) => {
  const r = e.target.closest("[data-convo]");
  if (r) {
    open(r.dataset.convo);
    return;
  }
  const s = e.target.closest("[data-start]");
  if (s) startDialog(s.dataset.start);
});

document.addEventListener("submit", (e) => {
  const f = e.target.closest(".jsend");
  if (!f) return;
  e.preventDefault();
  const input = f.querySelector("input");
  const text = input.value.trim();
  if (!text || !sel) return;
  input.value = "";
  post({ JabberSend: { jid: sel, body: text } });
});

function post(action) {
  fetch("/api/action", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(action),
  }).catch(() => {});
}

/// Start a conversation, the way the app's own dialog does: a field that takes an exact name, and
/// the conversations you have had recently, filtered as you type.
function startDialog(kind) {
  const room = kind === "room";
  const wrap = document.createElement("div");
  wrap.className = "jstartdlg";
  wrap.innerHTML =
    `<div class="mpanel"><button class="mclose" aria-label="Close">${ico("x")}</button>` +
    `<h3>${room ? "Join a room" : "Start a DM"}</h3>` +
    `<input class="jq" autocomplete="off" placeholder="${room ? "room@conference.…" : "Name"}">` +
    `<div class="jrecent"></div></div>`;
  document.body.append(wrap);
  const q = wrap.querySelector(".jq");
  const recent = wrap.querySelector(".jrecent");
  const close = () => wrap.remove();

  const draw = () => {
    const term = q.value.trim().toLowerCase();
    // Everything with history, not only what the list shows: the point of this dialog is to reach
    // what the list does not.
    const rows = side()
      .convos.filter((c) => c.room === room && c.last_at > 0)
      .filter((c) => !term || c.name.toLowerCase().includes(term) || c.jid.toLowerCase().includes(term))
      .sort((a, b) => b.last_at - a.last_at)
      .slice(0, 100);
    recent.innerHTML = rows.length
      ? rows
          .map(
            (c) =>
              `<button class="jrrow" data-pick="${esc(c.jid)}" data-name="${esc(c.name)}">` +
              `${room ? ico("users-three") : ico("chat-circle-dots")} ${esc(c.name)}</button>`
          )
          .join("")
      : `<p class="placeholder">Nothing recent. Type an exact name.</p>`;
  };
  draw();
  q.addEventListener("input", draw);
  q.focus();

  const go = (name) => {
    close();
    post({ JabberOpen: { name, room } });
    // Optimistic: the app resolves the name and the next push carries the conversation. Until then
    // the list is what it was, which is better than a pane that looks broken.
    const known = side().convos.find(
      (c) => c.room === room && (c.name.toLowerCase() === name.toLowerCase() || c.jid === name)
    );
    if (known) open(known.jid);
  };

  wrap.addEventListener("click", (e) => {
    if (e.target === wrap || e.target.closest(".mclose")) return close();
    const p = e.target.closest("[data-pick]");
    if (p) go(p.dataset.name);
  });
  q.addEventListener("keydown", (e) => {
    if (e.key === "Enter" && q.value.trim()) go(q.value.trim());
    if (e.key === "Escape") close();
  });
}
