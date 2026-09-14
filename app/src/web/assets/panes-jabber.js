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
// `?jabber=<jid>` opens one, which is how a link can point at a conversation and the only way a
// load-time screenshot can capture the chat header: the harness cannot click.
{
  const want = new URLSearchParams(location.search).get("jabber");
  if (want) sel = want;
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
  markRead();
}

/// Tell the app the open conversation has been looked at.
///
/// Reading it here had been clearing nothing: the unread marker lives in the app's jabber state, the
/// chat endpoint is a plain read, and nothing ever said the page was looking. So a message read on a
/// phone stayed bold on the desktop and kept its badge on both.
///
/// Three conditions, because "the page has this conversation selected" is not the same as "somebody
/// is reading it": the pane has to be the one on screen, the tab has to be in the foreground, and
/// there has to be something unread to clear. The last one is what keeps this from posting on every
/// snapshot for the rest of the session.
function markRead() {
  if (!sel || document.visibilityState !== "visible") return;
  if (!el || el.hidden || !el.offsetParent) return;
  if (!(convo(sel)?.unread > 0)) return;
  post({ JabberRead: { jid: sel } });
}

const stamp = (at) => {
  const d = new Date(at * 1000);
  return `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
};

/// A roll-call collapsed to a count, the way `condense_attention_list` does it.
///
/// The ping bot echoes every broadcast back with every recipient named, which is a multi-kilobyte
/// line that buries the conversation it is in.
function condense(body) {
  const marker = "requests the attention of:";
  const at = body.toLowerCase().indexOf(marker);
  if (at < 0) return body;
  const after = at + marker.length;
  const list = body.slice(after).trim();
  if (!list) return body;
  const n = list.split(",").filter((p) => p.trim()).length;
  return `${body.slice(0, after).trimEnd()} [${n} ${n === 1 ? "user" : "users"}]`;
}

/// Trailing sentence punctuation is not part of a URL: a copied link with a full stop on the end
/// fails when it is pasted. The same set `trim_url_tail` strips.
const TAIL = /[.,;:!?)\]}>"']+$/;

/// The message body as HTML: links clickable, mentions marked, everything else escaped.
///
/// Tokenised rather than escaped-then-regexed. Escaping first turns an `&` inside a URL into
/// `&amp;` and the href stops working; linkifying first means the surrounding text never gets
/// escaped at all.
function bodyHtml(text, names) {
  const rx = names.length
    ? new RegExp(`(^|[^\\w])(${names.map(esc4rx).join("|")})(?![\\w])`, "gi")
    : null;
  const mark = (t) => {
    const e = esc(t);
    return rx ? e.replace(rx, (_, pre, hit) => `${pre}<b class="jmention">${hit}</b>`) : e;
  };
  return linkify(condense(text), mark);
}

/// Every http(s) URL in `text` as an anchor, with `mark` escaping everything between them.
///
/// `mark` is the seam: a chat line also wants its mentions highlighted, a MOTD wants nothing but
/// the links, and both want exactly one implementation of where a URL ends.
function linkify(text, mark = esc) {
  let out = "";
  let rest = String(text ?? "");
  for (;;) {
    const at = rest.search(/https?:\/\//);
    if (at < 0) break;
    out += mark(rest.slice(0, at));
    const end = rest.slice(at).search(/\s/);
    const raw = end < 0 ? rest.slice(at) : rest.slice(at, at + end);
    const url = raw.replace(TAIL, "");
    out += `<a href="${esc(url)}" target="_blank" rel="noopener">${esc(url)}</a>`;
    rest = rest.slice(at + url.length);
  }
  return out + mark(rest);
}

const esc4rx = (s) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

/// The part of a JID anyone says out loud. A sender arrives as a bare JID in a DM and as a nick in a
/// room, and "@goonfleet.com" after every line in a conversation with one other person is noise.
const who = (from) => String(from ?? "").split("@")[0];

/// A MOTD is a notice board: several lines, blank ones between them, sometimes a rule of dashes.
/// Collapsed it is a paragraph, so the separator keeps the first line reading as the headline.
const motdOneLine = (m) =>
  String(m ?? "")
    .split("\n")
    .map((l) => l.trim())
    .filter(Boolean)
    // A row of dashes or equals is a rule: it separates the lines above from the ones below, and on
    // one line it separates nothing while taking a third of the bar.
    .filter((l) => /[\p{L}\p{N}]/u.test(l))
    .join("  ·  ");

/// The first `max` non-empty lines, marked when there is more. A tooltip carrying a whole MOTD
/// covers the conversation it is describing.
function motdPreview(m, max) {
  const lines = String(m ?? "")
    .split("\n")
    .map((l) => l.trimEnd())
    .filter((l) => l.trim());
  return lines.slice(0, max).join("\n") + (lines.length > max ? "\n…" : "");
}

function row(c) {
  const dot = c.room
    ? `<span class="jico">${ico("users-three")}</span>`
    : `<span class="jdot" style="background:${c.presence ?? "var(--muted)"}"></span>`;
  const badge = c.unread
    ? `<span class="jcount${c.mention ? " mention" : ""}">${c.unread > 99 ? "99+" : c.unread}</span>`
    : "";
  // The close button is a sibling, not nested: a button inside a button is invalid HTML and the
  // browser hoists it out, which is how the row would stop being clickable at all.
  return (
    `<div class="jrowwrap">` +
    `<button class="jrow${c.jid === sel ? " on" : ""}${c.mention ? " mentioned" : ""}" data-convo="${esc(c.jid)}" ` +
    // A room's tooltip is its topic, capped: that is what you want off a room in a list, and the
    // JID is the same words as the name plus a domain.
    `title="${esc(c.motd ? motdPreview(c.motd, 6) : c.jid)}">` +
    `${dot}<span class="jname">${esc(c.name)}</span>${badge}</button>` +
    `<button class="jshut" data-close="${esc(c.jid)}" title="Close. It comes back on the next message.">` +
    `${ico("x")}</button></div>`
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
  const names = side().mention_names ?? [];
  // One person talking for a while is one block. Repeating the same name and the same minute on
  // every line is the noise a chat client exists to remove; five minutes, and only while nobody
  // else has spoken, which is what makes it still a block and not a merge.
  const GROUP_SECS = 300;
  const lines = chat.msgs
    .map((m, i) => {
      const prev = chat.msgs[i - 1];
      const grouped = prev && prev.from === m.from && m.at - prev.at <= GROUP_SECS;
      return (
        `<div class="jmsg${m.me ? " me" : ""}${grouped ? " cont" : ""}">` +
        (grouped
          ? ""
          : `<span class="jwho">${esc(who(m.from))}</span>` +
            `<span class="jat" title="${esc(new Date(m.at * 1000).toLocaleString())}">${stamp(m.at)}</span>`) +
        `<span class="jbody">${bodyHtml(m.body, names)}</span></div>`
      );
    })
    .join("");
  const write = state.snapshot?.meta?.allow_writeback
    ? `<form class="jsend"><input name="body" autocomplete="off" placeholder="Message ${esc(c.name)}"><button type="submit">${ico("paper-plane-right")}</button></form>`
    : "";
  return (
    `<div class="jhead">${c.room ? ico("users-three") : ico("chat-circle-dots")} <b>${esc(c.name)}</b>` +
    // The room's topic on the room's own bar, one line of it. The rest is a tap away rather than
    // wrapped into the header, which on a phone would be most of the screen before any message.
    (c.motd
      ? `<span class="jtopic" title="${esc(motdPreview(c.motd, 6))}">${esc(motdOneLine(c.motd))}</span>` +
        `<button class="jmotd" data-motd="${esc(c.jid)}" title="Show the full MOTD">${ico("article")}</button>`
      : "") +
    `</div>` +
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
  markRead();
});

// Coming back to the tab is reading it, and so is switching to this pane. Neither goes through
// `register`, which only fires on a snapshot.
document.addEventListener("visibilitychange", markRead);
window.addEventListener("spai:panes", markRead);

/// A room's MOTD in full, which is the only place it is shown whole.
///
/// The header shows one line of it; this is where the ping format, the comms details and the forum
/// link actually live, so the text stays selectable and the whitespace it was written with is kept.
function motdDialog(jid) {
  const c = convo(jid);
  if (!c?.motd) return;
  const wrap = document.createElement("div");
  wrap.className = "jstartdlg motddlg";
  wrap.innerHTML =
    `<div class="mpanel"><button class="mclose" aria-label="Close">${ico("x")}</button>` +
    `<h3>${ico("article")} ${esc(c.name)} MOTD</h3>` +
    // Linkified, not escaped flat: a MOTD is where the doctrine and forum links live, and they are
    // the part people actually want out of it.
    `<pre class="jmotdtext">${linkify(c.motd)}</pre></div>`;
  document.body.append(wrap);
  wrap.addEventListener("click", (e) => {
    if (e.target === wrap || e.target.closest(".mclose")) wrap.remove();
  });
}

/// One listener, on the pane, for every row there will ever be: the list is rebuilt on every push.
document.addEventListener("click", (e) => {
  const m = e.target.closest("[data-motd]");
  if (m) {
    motdDialog(m.dataset.motd);
    return;
  }
  const shut = e.target.closest("[data-close]");
  if (shut) {
    const jid = shut.dataset.close;
    // Hiding, not leaving: the same thing the app's tab X does, and it comes back unread.
    post({ JabberClose: { jid } });
    if (sel === jid) {
      remember(null);
      chat = { jid: null, msgs: [], at: 0 };
    }
    // Dropped from the list now rather than waiting for the next snapshot, so the click looks like
    // it did something.
    shut.closest(".jrowwrap")?.remove();
    return;
  }
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
