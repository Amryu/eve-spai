// The jabber pane: the Convos list on the left, the selected conversation on the right. No tabs, so
// the list is the selection and the two cannot disagree.

import { esc, ico, modal, register, send, state } from "./app.js";

const KEY = "spai_jabber";

/// Which conversation is open, per device rather than pushed from the app, since each reader picks
/// their own.
let sel = null;
try {
  sel = localStorage.getItem(KEY);
} catch {
  // Private browsing. The selection then lasts one session.
}
// `?jabber=<jid>` opens one, for links and load-time screenshots, since the harness cannot click.
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

/// Fetch the open conversation only when its `last_at` moved, rather than polling the backlog.
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

function open(jid) {
  remember(jid);
  chat = { jid: null, msgs: [], at: 0 };
  paint();
  sync(true);
  markRead();
}

/// Tell the app the open conversation was read. The unread marker lives in the app and the chat
/// endpoint is a plain read, so this is the only thing that clears it.
///
/// Only when the pane is on screen, the tab is visible and something is unread, so a selected but
/// unwatched conversation stays unread and this does not post on every snapshot.
function markRead() {
  if (!sel || document.visibilityState !== "visible") return;
  if (!el || el.hidden || !el.offsetParent) return;
  if (!(convo(sel)?.unread > 0)) return;
  send({ JabberRead: { jid: sel } });
}

const stamp = (at) => {
  const d = new Date(at * 1000);
  return `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
};

/// A roll-call collapsed to a count, like `condense_attention_list`. The ping bot echoes broadcasts
/// with every recipient named, a multi-kilobyte line that buries the conversation.
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

/// Trailing sentence punctuation is not part of a URL. The same set `trim_url_tail` strips.
const TAIL = /[.,;:!?)\]}>"']+$/;

/// The message body as HTML: links clickable, mentions marked, everything else escaped.
///
/// Tokenised, since escaping first breaks `&` in hrefs and linkifying first leaves the text unescaped.
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

/// Every http(s) URL in `text` as an anchor, with `mark` escaping everything between them, so chat
/// lines can also highlight mentions while sharing one URL boundary rule with the MOTD.
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

/// The local part of a JID. A DM sender arrives as a bare JID, and the domain on every line is noise.
const who = (from) => String(from ?? "").split("@")[0];

/// A multi-line MOTD on one line. The separator keeps the first line reading as the headline.
const motdOneLine = (m) =>
  String(m ?? "")
    .split("\n")
    .map((l) => l.trim())
    .filter(Boolean)
    // Drop rules of dashes or equals, which separate nothing on one line.
    .filter((l) => /[\p{L}\p{N}]/u.test(l))
    .join("  ·  ");

/// The first `max` non-empty lines, marked when there is more, so a tooltip does not cover the chat.
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
  // The close button is a sibling: a nested button is invalid HTML and the browser hoists it out.
  return (
    `<div class="jrowwrap">` +
    `<button class="jrow${c.jid === sel ? " on" : ""}${c.mention ? " mentioned" : ""}" data-convo="${esc(c.jid)}" ` +
    // A room's tooltip is its capped topic, since its JID only repeats the name.
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
  // Consecutive lines from one sender within five minutes share a header.
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
    // One line of topic, since a wrapped MOTD would fill a phone screen before any message.
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
  // Keep the log pinned to the bottom, or at its scroll position if the user scrolled up.
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

/// A room's full MOTD, selectable and with its original whitespace kept.
function motdDialog(jid) {
  const c = convo(jid);
  if (!c?.motd) return;
  modal(
    "motddlg",
    `<h3>${ico("article")} ${esc(c.name)} MOTD</h3>` +
      // Linkified, since MOTDs carry the doctrine and forum links.
      `<pre class="jmotdtext">${linkify(c.motd)}</pre>`
  );
}

/// One document listener, since the list is rebuilt on every push.
document.addEventListener("click", (e) => {
  const m = e.target.closest("[data-motd]");
  if (m) {
    motdDialog(m.dataset.motd);
    return;
  }
  const shut = e.target.closest("[data-close]");
  if (shut) {
    const jid = shut.dataset.close;
    // Hides rather than leaves, like the app's tab X. It comes back on the next message.
    send({ JabberClose: { jid } });
    if (sel === jid) {
      remember(null);
      chat = { jid: null, msgs: [], at: 0 };
    }
    // Removed now rather than on the next snapshot, so the click has visible effect.
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
  send({ JabberSend: { jid: sel, body: text } });
});

/// Start a conversation, the way the app's own dialog does: a field that takes an exact name, and
/// the conversations you have had recently, filtered as you type.
function startDialog(kind) {
  const room = kind === "room";
  const { wrap, close } = modal(
    "",
    `<h3>${room ? "Join a room" : "Start a DM"}</h3>` +
      `<input class="jq" autocomplete="off" placeholder="${room ? "room@conference.…" : "Name"}">` +
      `<div class="jrecent"></div>`
  );
  const q = wrap.querySelector(".jq");
  const recent = wrap.querySelector(".jrecent");

  const draw = () => {
    const term = q.value.trim().toLowerCase();
    // Everything with history, including conversations the list does not show.
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
    send({ JabberOpen: { name, room } });
    // The app resolves the name and the next push carries the conversation. Open it now if known.
    const known = side().convos.find(
      (c) => c.room === room && (c.name.toLowerCase() === name.toLowerCase() || c.jid === name)
    );
    if (known) open(known.jid);
  };

  wrap.addEventListener("click", (e) => {
    const p = e.target.closest("[data-pick]");
    if (p) go(p.dataset.name);
  });
  q.addEventListener("keydown", (e) => {
    if (e.key === "Enter" && q.value.trim()) go(q.value.trim());
    if (e.key === "Escape") close();
  });
}
