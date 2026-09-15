// Notes and tags on systems and pilots, kept in folders.
//
// The app owns the book and every rule about it: this draws the merged view the snapshot carries and
// posts edits back as `Notes` actions. Nothing here edits the snapshot optimistically, because a
// refused edit (a name taken, a note too long) would leave the page showing something the app does
// not have. The next push, half a second later, is the confirmation.

import { afterRender, ico, state } from "./app.js";
import { menu } from "./route.js";

const esc = (s) =>
  String(s ?? "").replace(/[&<>"']/g, (c) =>
    ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]
  );

const NOTE_MAX = 2000;
const NAME_MAX = 64;
const FORMAT = "eve-spai-notes";
const PREFIX = "SPAINOTES1:";

export const rgb = (c) => `rgb(${c[0]}, ${c[1]}, ${c[2]})`;
const hex = (c) => `#${c.map((v) => v.toString(16).padStart(2, "0")).join("")}`;
const unhex = (h) => [1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16) || 0);

const pane = () => state.snapshot?.notes ?? null;
const view = () => pane()?.view ?? null;
const book = () => pane()?.book ?? { folders: [] };
const canWrite = () => !!state.snapshot?.meta?.allow_writeback;
const KIND = { system: "System", pilot: "Pilot" };

/// Its own copy of `dialogs.send`: importing dialogs.js from here would close an import cycle
/// through panes-intel.js, and dialogs.js runs a deep link at load that can reach this module before
/// it has finished evaluating.
async function post(action) {
  if (!canWrite()) return false;
  try {
    const r = await fetch("/api/action", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(action),
    });
    return r.ok;
  } catch {
    return false;
  }
}

/// Lookups rebuilt only when the snapshot hands over a new view object, which is once per edit.
let memoFor = null;
let tagIndex = new Map();
let nameIndex = new Map();
function memo() {
  const v = view();
  if (v === memoFor) return;
  memoFor = v;
  tagIndex = new Map();
  for (const t of [...(v?.system_tags ?? []), ...(v?.pilot_tags ?? [])]) tagIndex.set(t.id, t);
  // Tags in offline folders are not in the view, and the manager still has to name them.
  for (const f of flatten()) for (const t of f.folder.tags ?? []) if (!tagIndex.has(t.id)) tagIndex.set(t.id, t);
  nameIndex = new Map(Object.entries(v?.pilots ?? {}).map(([id, m]) => [m.name.toLowerCase(), Number(id)]));
}

export function tagById(id) {
  memo();
  return tagIndex.get(id) ?? null;
}

function catalogue(kind) {
  const v = view();
  return (kind === "system" ? v?.system_tags : v?.pilot_tags) ?? [];
}

export function entryOf(kind, id) {
  const v = view();
  return (kind === "system" ? v?.systems : v?.pilots)?.[id] ?? null;
}

function pilotIdByName(name) {
  memo();
  return nameIndex.get(String(name).toLowerCase()) ?? null;
}

/// Every folder, depth first, with its breadcrumb and whether its top-level folder is online.
function flatten() {
  const out = [];
  const walk = (list, depth, trail, online) => {
    for (const f of list ?? []) {
      const path = [...trail, f.name];
      const on = depth === 0 ? f.online !== false : online;
      out.push({ folder: f, depth, path: path.join(" / "), online: on });
      walk(f.children, depth + 1, path, on);
    }
  };
  walk(book().folders, 0, [], true);
  return out;
}

function findFolder(id) {
  return flatten().find((r) => r.folder.id === id) ?? null;
}

const tagChip = (t) =>
  `<span class="ntag" style="--c:${rgb(t.color)}">${esc(t.name)}</span>`;

/// The tag chips drawn after a system or pilot badge on a card.
export function chips(kind, id) {
  const m = entryOf(kind, id);
  if (!m) return "";
  const tags = m.tags.map(tagById).filter(Boolean).map(tagChip).join("");
  const noted = m.parts.some((p) => p.note);
  return tags + (noted ? `<span class="nnote">${ico("note")}</span>` : "");
}

/// Tags and notes as plain lines, for a badge's `title`.
export function titleLines(kind, id) {
  const m = entryOf(kind, id);
  if (!m) return [];
  const lines = [];
  const names = m.tags.map(tagById).filter(Boolean).map((t) => t.name);
  if (names.length) lines.push(`Tags: ${names.join(", ")}`);
  for (const p of m.parts) if (p.note) lines.push(`${p.path}: ${p.note}`);
  return lines;
}

function merged(m, q) {
  return (
    m.parts.some((p) => p.note.toLowerCase().includes(q)) ||
    m.tags.some((t) => (tagById(t)?.name ?? "").toLowerCase().includes(q))
  );
}

/// Whether a report's systems or pilots carry a tag or a note matching the filter text.
export function queryHits(report, q) {
  if (!view()) return false;
  const needle = q.toLowerCase();
  for (const s of report.systems ?? []) {
    const m = entryOf("system", s.id);
    if (m && merged(m, needle)) return true;
  }
  for (const name of report.pilots ?? []) {
    const id = pilotIdByName(name);
    const m = id == null ? null : entryOf("pilot", id);
    if (m && merged(m, needle)) return true;
  }
  return false;
}

/// The notes card in the system and pilot dialogs. Refreshed in place when the book changes, so an
/// edit made from the card's menu shows up in a dialog that is already open.
export function section(kind, id, name) {
  return (
    `<section class="scard snotes" data-nkind="${kind}" data-nid="${id}" data-nname="${esc(name)}">` +
    sectionBody(kind, id) +
    `</section>`
  );
}

function sectionBody(kind, id) {
  const m = entryOf(kind, id);
  const edit = canWrite()
    ? `<button class="nedit" data-note-edit>${ico("note-pencil")} Edit</button>`
    : "";
  const head = `<h4>${ico("tag")} Notes and tags${edit}</h4>`;
  if (!m) return `${head}<p class="nempty">Nothing noted.</p>`;
  const tags = m.tags.map(tagById).filter(Boolean).map(tagChip).join("");
  const parts = m.parts
    .filter((p) => p.note)
    .map(
      (p) =>
        `<div class="npart"><span class="npath">${ico("folder")} ${esc(p.path)}</span>` +
        `<p>${esc(p.note)}</p></div>`
    )
    .join("");
  return head + (tags ? `<p class="ntags">${tags}</p>` : "") + parts;
}

function subjectOf(kind, id, name) {
  return kind === "system"
    ? { System: Number(id) }
    : { Pilot: { id: Number(id), name: String(name) } };
}

/// The small modal every prompt here is built on. Resolves with whatever `done` is called with, and
/// with null when dismissed.
function modal(cls, html, wire) {
  return new Promise((resolve) => {
    const wrap = document.createElement("div");
    wrap.className = `jstartdlg altdlg ${cls}`;
    wrap.innerHTML = `<div class="mpanel"><button class="mclose" aria-label="Close">${ico("x")}</button>${html}</div>`;
    document.body.append(wrap);
    let settled = false;
    const done = (v) => {
      if (settled) return;
      settled = true;
      wrap.remove();
      document.removeEventListener("keydown", key);
      resolve(v);
    };
    const key = (e) => {
      if (e.key === "Escape") done(null);
    };
    document.addEventListener("keydown", key);
    wrap.addEventListener("click", (e) => {
      if (e.target === wrap || e.target.closest(".mclose")) done(null);
    });
    wire(wrap, done);
  });
}

function ask(title, value = "", placeholder = "Name") {
  return modal(
    "nask",
    `<h3>${esc(title)}</h3><input class="jq" maxlength="${NAME_MAX}" placeholder="${esc(placeholder)}" value="${esc(value)}">` +
      `<p class="rsave"><button data-ok>OK</button></p>`,
    (wrap, done) => {
      const f = wrap.querySelector(".jq");
      f.focus();
      f.select();
      const go = () => f.value.trim() && done(f.value.trim());
      f.addEventListener("keydown", (e) => e.key === "Enter" && go());
      wrap.querySelector("[data-ok]").addEventListener("click", go);
    }
  );
}

/// `buttons` is `[key, label, class]`.
function choose(title, text, buttons) {
  return modal(
    "nask",
    `<h3>${esc(title)}</h3><p>${esc(text)}</p><div class="mbtns">` +
      buttons.map(([k, label, cls]) => `<button data-k="${k}" class="${cls ?? ""}">${esc(label)}</button>`).join("") +
      `</div>`,
    (wrap, done) =>
      wrap.querySelectorAll("[data-k]").forEach((b) => b.addEventListener("click", () => done(b.dataset.k)))
  );
}

/// A folder picker. `top` offers the top level, as `""`; `skip` leaves a folder and its subfolders out.
function pickFolder(title, { top = false, skip = null } = {}) {
  let skipping = -1;
  const rows = flatten().filter((r) => {
    if (skipping >= 0 && r.depth > skipping) return false;
    skipping = -1;
    if (r.folder.id === skip) {
      skipping = r.depth;
      return false;
    }
    return true;
  });
  return modal(
    "nask",
    `<h3>${esc(title)}</h3><ul class="ravoidrows npick">` +
      (top ? `<li><button data-f="">${ico("folder")} Top level</button></li>` : "") +
      rows
        .map((r) => `<li><button data-f="${esc(r.folder.id)}" style="--d:${r.depth}">${ico("folder")} ${esc(r.folder.name)}</button></li>`)
        .join("") +
      `</ul>`,
    (wrap, done) =>
      wrap.querySelectorAll("[data-f]").forEach((b) => b.addEventListener("click", () => done(b.dataset.f)))
  );
}

// ---------------------------------------------------------------------------------------------
// The editor for one system or pilot in one folder.

let editor = null;

export function openEditor(subject, name, folderId = null) {
  if (!canWrite()) return;
  editor?.close();
  const kind = subject.System != null ? "system" : "pilot";
  const key = kind === "system" ? subject.System : subject.Pilot.id;
  const folders = flatten();
  const target = view()?.target ?? "";
  let folder = folderId ?? (findFolder(target) ? target : folders[0]?.folder.id ?? "");
  let draft = new Set();
  let pending = null;

  const wrap = document.createElement("div");
  wrap.className = "jstartdlg altdlg noteeditor";
  document.body.append(wrap);

  const entryIn = (fid) => (findFolder(fid)?.folder?.[kind === "system" ? "systems" : "pilots"] ?? {})[key] ?? null;
  const load = () => {
    const e = entryIn(folder);
    draft = new Set(e?.tags ?? []);
    const area = wrap.querySelector("textarea");
    if (area) area.value = e?.note ?? "";
  };

  const tagButtons = () => {
    const cat = catalogue(kind);
    return (
      cat
        .map(
          (t) =>
            `<button class="ntog${draft.has(t.id) ? " on" : ""}" data-tog="${esc(t.id)}" style="--c:${rgb(t.color)}">` +
            `${ico(draft.has(t.id) ? "check-square" : "square")} ${esc(t.name)}</button>`
        )
        .join("") || `<p class="nempty">No tags.</p>`
    );
  };

  const others = () => {
    const m = entryOf(kind, key);
    return (m?.parts ?? [])
      .filter((p) => p.folder !== folder && (p.note || p.tags.length))
      .map(
        (p) =>
          `<div class="npart ro"><span class="npath">${ico("folder")} ${esc(p.path)}</span>` +
          (p.tags.length ? `<p class="ntags">${p.tags.map(tagById).filter(Boolean).map(tagChip).join("")}</p>` : "") +
          (p.note ? `<p>${esc(p.note)}</p>` : "") +
          `</div>`
      )
      .join("");
  };

  const folderOptions = () =>
    (folders.length
      ? folders
      : [{ folder: { id: "", name: "Default (new)" }, depth: 0 }]
    )
      .map(
        (r) =>
          `<option value="${esc(r.folder.id)}"${r.folder.id === folder ? " selected" : ""}>` +
          `${"  ".repeat(r.depth)}${esc(r.folder.name)}</option>`
      )
      .join("");

  wrap.innerHTML =
    `<div class="mpanel"><button class="mclose" aria-label="Close">${ico("x")}</button>` +
    `<h3>${ico(kind === "system" ? "planet" : "user")} ${esc(name)}</h3>` +
    `<label class="nfield">${ico("folder")} <select data-folder>${folderOptions()}</select></label>` +
    `<textarea class="jq nta" maxlength="${NOTE_MAX}" rows="5" placeholder="Note"></textarea>` +
    `<div class="ntogs" data-tags>${tagButtons()}</div>` +
    `<div class="nnew"><input class="jq" data-newname maxlength="${NAME_MAX}" placeholder="New tag">` +
    `<input type="color" data-newcolor value="#42a5f5"><button data-newtag>${ico("plus")} Add</button></div>` +
    `<div data-others>${others()}</div>` +
    `<p class="nerr" data-err hidden></p>` +
    `<div class="mbtns"><button class="mok" data-save>Save</button><button data-clear>Clear</button>` +
    `<button data-cancel>Cancel</button></div></div>`;
  load();

  const close = () => {
    wrap.remove();
    document.removeEventListener("keydown", onKey);
    if (editor?.wrap === wrap) editor = null;
  };
  const onKey = (e) => e.key === "Escape" && close();
  document.addEventListener("keydown", onKey);
  const err = (msg) => {
    const p = wrap.querySelector("[data-err]");
    p.textContent = msg;
    p.hidden = !msg;
  };
  const paintTags = () => {
    wrap.querySelector("[data-tags]").innerHTML = tagButtons();
    wrap.querySelector("[data-others]").innerHTML = others();
  };

  wrap.querySelector("[data-folder]").addEventListener("change", (e) => {
    folder = e.target.value;
    load();
    paintTags();
  });
  wrap.addEventListener("click", async (e) => {
    if (e.target === wrap || e.target.closest(".mclose") || e.target.closest("[data-cancel]")) return close();
    const tog = e.target.closest("[data-tog]");
    if (tog) {
      const id = tog.dataset.tog;
      if (draft.has(id)) draft.delete(id);
      else draft.add(id);
      paintTags();
      return;
    }
    if (e.target.closest("[data-newtag]")) {
      const n = wrap.querySelector("[data-newname]");
      const nameVal = n.value.trim();
      if (!nameVal) return;
      const ok = await post({
        Notes: { PutTag: { folder, id: null, kind: KIND[kind], name: nameVal, color: unhex(wrap.querySelector("[data-newcolor]").value) } },
      });
      if (!ok) return err("Could not add the tag.");
      pending = nameVal.toLowerCase();
      n.value = "";
      err("");
      return;
    }
    if (e.target.closest("[data-save]") || e.target.closest("[data-clear]")) {
      const clear = !!e.target.closest("[data-clear]");
      const note = clear ? "" : wrap.querySelector("textarea").value;
      const tags = clear ? [] : [...draft];
      const ok = await post({ Notes: { SetEntry: { folder, subject, note, tags } } });
      if (!ok) return err("Could not save. The web view may be read only.");
      if (folder) post({ NotesTarget: { folder } });
      close();
    }
  });

  editor = {
    wrap,
    close,
    // A new tag arrives with the next push; select it then, so adding a tag means using it.
    refresh() {
      if (pending) {
        const t = catalogue(kind).find((x) => x.name.toLowerCase() === pending && !x.id.startsWith("d:"));
        if (t) {
          draft.add(t.id);
          pending = null;
        }
      }
      if (!folder && flatten().length) {
        // The first save made the Default folder; follow it rather than making a second one.
        folder = view()?.target || flatten()[0].folder.id;
        wrap.querySelector("[data-folder]").innerHTML = flatten()
          .map((r) => `<option value="${esc(r.folder.id)}"${r.folder.id === folder ? " selected" : ""}>${"  ".repeat(r.depth)}${esc(r.folder.name)}</option>`)
          .join("");
      }
      paintTags();
    },
  };
  wrap.querySelector("textarea").focus();
}

// ---------------------------------------------------------------------------------------------
// The quick menu on a badge or a map system.

export function quickMenu(x, y, subject, name) {
  if (!canWrite()) return false;
  const kind = subject.System != null ? "system" : "pilot";
  const key = kind === "system" ? subject.System : subject.Pilot.id;
  const v = view();
  const target = v?.target ?? "";
  const m = entryOf(kind, key);
  const mine = new Set(m?.parts.find((p) => p.folder === target)?.tags ?? []);
  const where = v?.target_path || findFolder(target)?.path || "Default";
  const items = [
    ["edit", `${ico("note-pencil")} Notes and tags…`],
    ["manage", `${ico("folder")} Into ${esc(where)}`, "nhead"],
    null,
  ];
  for (const t of catalogue(kind)) {
    const elsewhere = (m?.parts ?? []).filter((p) => p.folder !== target && p.tags.includes(t.id)).map((p) => p.path);
    items.push([
      `tag:${t.id}`,
      `${ico(mine.has(t.id) ? "check-square" : "square")} <i class="ndot" style="--c:${rgb(t.color)}"></i>${esc(t.name)}` +
        (elsewhere.length ? ` <em class="nelse">${esc(elsewhere.join(", "))}</em>` : ""),
    ]);
  }
  menu(x, y, items, (pick) => {
    if (pick === "edit") return openEditor(subject, name);
    if (pick === "manage") return openManager(kind);
    if (pick.startsWith("tag:")) {
      const tag = pick.slice(4);
      post({ Notes: { SetTag: { folder: target, subject, tag, on: !mine.has(tag) } } });
    }
  });
  return true;
}

document.addEventListener("contextmenu", (e) => {
  const chip = e.target.closest(".card [data-pilot][data-pid], .card [data-system]");
  if (!chip || !canWrite()) return;
  e.preventDefault();
  if (chip.dataset.pid) {
    const name = chip.dataset.pilot;
    quickMenu(e.clientX, e.clientY, subjectOf("pilot", chip.dataset.pid, name), name);
  } else {
    const id = chip.dataset.system;
    quickMenu(e.clientX, e.clientY, subjectOf("system", id), chip.dataset.name ?? id);
  }
});

// ---------------------------------------------------------------------------------------------
// The manager: folders, and what each one holds.

let mgr = null;

export function openManager(kind = "pilot") {
  mgr?.close();
  const wrap = document.createElement("div");
  wrap.className = "jstartdlg altdlg notesdlg";
  document.body.append(wrap);
  const target = view()?.target;
  const st = { kind, sel: findFolder(target) ? target : flatten()[0]?.folder.id ?? null, q: "" };
  const close = () => {
    wrap.remove();
    document.removeEventListener("keydown", onKey);
    if (mgr?.wrap === wrap) mgr = null;
  };
  const onKey = (e) => {
    // A prompt opened over the manager takes Escape for itself.
    if (e.key === "Escape" && !document.querySelector(".nask")) close();
  };
  document.addEventListener("keydown", onKey);
  mgr = { wrap, close, st, paint: () => paintManager(wrap, st) };
  paintManager(wrap, st);
  wireManager(wrap, st, close);
}

function paintManager(wrap, st) {
  if (st.sel && !findFolder(st.sel)) st.sel = flatten()[0]?.folder.id ?? null;
  const w = canWrite();
  const target = view()?.target;
  const rows = flatten();
  const tree = rows.length
    ? rows
        .map(({ folder: f, depth, online }) => {
          const cls = [f.id === st.sel ? "on" : "", online ? "" : "off"].join(" ");
          return (
            `<button class="nfold ${cls}" data-msel="${esc(f.id)}" style="--d:${depth}">` +
            `${ico(f.id === st.sel ? "folder-open" : "folder")}<span>${esc(f.name)}</span>` +
            (f.id === target ? `<i title="Quick edits go here">${ico("target")}</i>` : "") +
            (depth === 0 && f.online === false ? `<em>offline</em>` : "") +
            `</button>`
          );
        })
        .join("")
    : `<p class="placeholder">No folders yet. The first note saved makes a Default folder.</p>`;

  const sel = st.sel ? findFolder(st.sel) : null;
  let side = `<p class="placeholder">Pick a folder.</p>`;
  if (sel) {
    const f = sel.folder;
    const top = sel.depth === 0;
    const act = (k, icon, label, extra = "") =>
      `<button data-fact="${k}"${extra}>${ico(icon)} ${label}</button>`;
    const actions = w
      ? `<div class="nacts">` +
        act("sub", "folder-plus", "Subfolder") +
        act("rename", "pencil-simple", "Rename") +
        act("move", "arrow-bend-up-right", "Move") +
        (top ? act("online", f.online === false ? "globe" : "globe-x", f.online === false ? "Go online" : "Go offline") : "") +
        (f.id === target ? "" : act("target", "target", "Quick edits here")) +
        act("export", "download-simple", "Export") +
        act("delete", "trash", "Delete", ` class="warn"`) +
        `</div>`
      : `<div class="nacts">${act("export", "download-simple", "Export")}</div>`;

    const kindKey = st.kind === "system" ? "systems" : "pilots";
    const tags = (f.tags ?? []).filter((t) => t.kind === KIND[st.kind]);
    const usage = usageCounts();
    const tagRows =
      tags
        .map(
          (t) =>
            `<li class="ntagrow">` +
            (w ? `<input type="color" value="${hex(t.color)}" data-tcolor="${esc(t.id)}">` : "") +
            tagChip(t) +
            `<em>${usage.get(t.id) ?? 0} used</em>` +
            (w
              ? `<span class="nrowacts"><button data-trename="${esc(t.id)}" title="Rename">${ico("pencil-simple")}</button>` +
                `<button data-tmove="${esc(t.id)}" title="Move to folder">${ico("arrow-bend-up-right")}</button>` +
                `<button data-tdel="${esc(t.id)}" class="warn" title="Delete">${ico("trash")}</button></span>`
              : "") +
            `</li>`
        )
        .join("") || `<li class="nempty">No tags of this kind in this folder.</li>`;
    const builtin = catalogue(st.kind).filter((t) => t.id.startsWith("d:"));
    const newTag = w
      ? `<div class="nnew"><input class="jq" data-tnew maxlength="${NAME_MAX}" placeholder="New tag">` +
        `<input type="color" data-tnewcolor value="#42a5f5"><button data-tadd>${ico("plus")} Add</button></div>`
      : "";

    side =
      `<p class="npathhead">${ico("folder-open")} ${esc(sel.path)}${sel.online ? "" : " <em>offline</em>"}</p>` +
      actions +
      `<section class="scard"><h4>${ico("tag")} Tags</h4><ul class="ntaglist">${tagRows}</ul>${newTag}` +
      `<details class="nbuiltin"${st.builtinOpen ? " open" : ""}><summary>Built in</summary>` +
      (w
        ? `<ul class="ntaglist">${builtin
            .map(
              (t) =>
                `<li class="ntagrow">${tagChip(t)}<input type="color" data-dcolor="${esc(t.id)}" value="${hex(t.color)}" title="Colour">` +
                `<button data-dreset="${esc(t.id)}" title="Back to the shipped colour">Reset</button></li>`
            )
            .join("")}</ul>`
        : `<p class="ntags">${builtin.map(tagChip).join("")}</p>`) +
      `</details></section>` +
      `<section class="scard"><h4>${ico("note")} ${st.kind === "system" ? "Systems" : "Pilots"}</h4>` +
      `<input type="search" class="jq" data-mq placeholder="Filter by name, note or tag" value="${esc(st.q)}">` +
      `<div data-entries>${entryRows(f[kindKey] ?? {}, st, w)}</div></section>`;
  }

  wrap.innerHTML =
    `<div class="mpanel nmgr"><button class="mclose" aria-label="Close">${ico("x")}</button>` +
    `<h3>${ico("tag")} Notes and tags</h3>` +
    `<div class="ntabs">` +
    `<button data-mkind="system" class="${st.kind === "system" ? "on" : ""}">${ico("planet")} Systems</button>` +
    `<button data-mkind="pilot" class="${st.kind === "pilot" ? "on" : ""}">${ico("user")} Pilots</button>` +
    `</div>` +
    `<div class="nmain"><div class="ntree">` +
    (w
      ? `<div class="nacts"><button data-mnew>${ico("folder-plus")} New folder</button>` +
        `<button data-mimport>${ico("upload-simple")} Import</button></div>`
      : "") +
    tree +
    `</div><div class="nside">${side}</div></div></div>`;
}

function usageCounts() {
  const n = new Map();
  for (const { folder } of flatten()) {
    for (const e of [...Object.values(folder.systems ?? {}), ...Object.values(folder.pilots ?? {})]) {
      for (const t of e.tags ?? []) n.set(t, (n.get(t) ?? 0) + 1);
    }
  }
  return n;
}

function entryRows(entries, st, w) {
  const q = st.q.trim().toLowerCase();
  const rows = Object.entries(entries)
    .filter(([, e]) => {
      if (!q) return true;
      const names = (e.tags ?? []).map((t) => tagById(t)?.name ?? "");
      return [e.name, e.note, ...names].some((s) => String(s).toLowerCase().includes(q));
    })
    .sort((a, b) => a[1].name.localeCompare(b[1].name));
  if (!rows.length) return `<p class="nempty">${q ? "Nothing matches." : "Nothing in this folder yet."}</p>`;
  return rows
    .map(([key, e]) => {
      const go =
        st.kind === "system"
          ? `<button class="chip nlink" data-system="${esc(key)}">${ico("planet")} ${esc(e.name)}</button>`
          : `<button class="chip nlink" data-pilot="${esc(e.name)}">${ico("user")} ${esc(e.name)}</button>`;
      const tags = (e.tags ?? []).map(tagById).filter(Boolean).map(tagChip).join("");
      return (
        `<div class="nentry">` +
        `<div class="nerow">${go}${tags}</div>` +
        (e.note ? `<p class="nnotetext">${esc(e.note)}</p>` : "") +
        (w
          ? `<div class="nrowacts"><button data-eedit="${esc(key)}">${ico("note-pencil")} Edit</button>` +
            `<button data-emove="${esc(key)}">${ico("arrow-bend-up-right")} Move</button>` +
            `<button data-edel="${esc(key)}" class="warn">${ico("trash")} Remove</button></div>`
          : "") +
        `</div>`
      );
    })
    .join("");
}

function wireManager(wrap, st, close) {
  const repaint = () => paintManager(wrap, st);
  const kindKey = () => (st.kind === "system" ? "systems" : "pilots");
  const entry = (key) => findFolder(st.sel)?.folder?.[kindKey()]?.[key] ?? null;
  const subject = (key) => subjectOf(st.kind, key, entry(key)?.name ?? "");
  const tagOf = (id) => (findFolder(st.sel)?.folder.tags ?? []).find((t) => t.id === id);

  wrap.addEventListener("input", (e) => {
    const q = e.target.closest("[data-mq]");
    if (!q) return;
    st.q = q.value;
    const f = findFolder(st.sel)?.folder;
    wrap.querySelector("[data-entries]").innerHTML = entryRows(f?.[kindKey()] ?? {}, st, canWrite());
  });
  wrap.addEventListener("toggle", (e) => {
    if (e.target.matches?.(".nbuiltin")) st.builtinOpen = e.target.open;
  }, true);
  wrap.addEventListener("change", (e) => {
    const d = e.target.closest("[data-dcolor]");
    if (d) return post({ DefaultTagColor: { id: d.dataset.dcolor, color: unhex(d.value) } });
    const c = e.target.closest("[data-tcolor]");
    if (!c) return;
    const t = tagOf(c.dataset.tcolor);
    if (t) post({ Notes: { PutTag: { folder: st.sel, id: t.id, kind: t.kind, name: t.name, color: unhex(c.value) } } });
  });
  wrap.addEventListener("keydown", (e) => {
    if (e.key === "Enter" && e.target.closest("[data-tnew]")) wrap.querySelector("[data-tadd]")?.click();
  });

  wrap.addEventListener("click", async (e) => {
    const t = e.target;
    if (t === wrap || t.closest(".mclose")) return close();
    const dr = t.closest("[data-dreset]");
    if (dr) return post({ DefaultTagColor: { id: dr.dataset.dreset, color: null } });
    // Going to a system or pilot opens its dialog, which sits under this modal.
    if (t.closest(".nlink")) return close();
    const k = t.closest("[data-mkind]");
    if (k) {
      st.kind = k.dataset.mkind;
      st.q = "";
      return repaint();
    }
    const s = t.closest("[data-msel]");
    if (s) {
      st.sel = s.dataset.msel;
      st.q = "";
      return repaint();
    }
    if (t.closest("[data-mnew]")) {
      const name = await ask("New folder");
      if (name) post({ Notes: { CreateFolder: { parent: null, name } } });
      return;
    }
    if (t.closest("[data-mimport]")) return openImport();

    const fact = t.closest("[data-fact]")?.dataset.fact;
    const sel = st.sel ? findFolder(st.sel) : null;
    if (fact && sel) {
      const f = sel.folder;
      switch (fact) {
        case "sub": {
          const name = await ask(`New folder in ${f.name}`);
          if (name) post({ Notes: { CreateFolder: { parent: f.id, name } } });
          break;
        }
        case "rename": {
          const name = await ask("Rename folder", f.name);
          if (name && name !== f.name) post({ Notes: { RenameFolder: { id: f.id, name } } });
          break;
        }
        case "move": {
          const to = await pickFolder(`Move ${f.name} into`, { top: true, skip: f.id });
          if (to != null) post({ Notes: { MoveFolder: { id: f.id, parent: to || null } } });
          break;
        }
        case "online":
          post({ Notes: { SetOnline: { id: f.id, on: f.online === false } } });
          break;
        case "target":
          post({ NotesTarget: { folder: f.id } });
          break;
        case "export":
          openExport(f.id);
          break;
        case "delete": {
          const inside = [];
          const walk = (x) => {
            inside.push(x);
            (x.children ?? []).forEach(walk);
          };
          walk(f);
          const items = inside.reduce(
            (n, x) => n + Object.keys(x.systems ?? {}).length + Object.keys(x.pilots ?? {}).length + (x.tags ?? []).length,
            0
          );
          const sure = await choose(
            `Delete ${f.name}?`,
            `This deletes ${inside.length} folder${inside.length === 1 ? "" : "s"} and ${items} tag${items === 1 ? "" : "s"} and entr${items === 1 ? "y" : "ies"} in them. Export it first to keep a copy.`,
            [["delete", "Delete", "mno"], ["cancel", "Cancel"]]
          );
          if (sure === "delete") post({ Notes: { DeleteFolder: { id: f.id } } });
          break;
        }
      }
      return;
    }

    const rn = t.closest("[data-trename]");
    if (rn) {
      const tag = tagOf(rn.dataset.trename);
      const name = tag && (await ask("Rename tag", tag.name));
      if (name && name !== tag.name) post({ Notes: { PutTag: { folder: st.sel, id: tag.id, kind: tag.kind, name, color: tag.color } } });
      return;
    }
    const tm = t.closest("[data-tmove]");
    if (tm) {
      const to = await pickFolder("Move tag to");
      if (to) post({ Notes: { MoveTag: { id: tm.dataset.tmove, to } } });
      return;
    }
    const td = t.closest("[data-tdel]");
    if (td) {
      const tag = tagOf(td.dataset.tdel);
      const used = usageCounts().get(td.dataset.tdel) ?? 0;
      const sure = await choose(
        `Delete ${tag?.name ?? "tag"}?`,
        `It comes off ${used} entr${used === 1 ? "y" : "ies"}. Alert rules naming it stop matching.`,
        [["delete", "Delete", "mno"], ["cancel", "Cancel"]]
      );
      if (sure === "delete") post({ Notes: { DeleteTag: { id: td.dataset.tdel } } });
      return;
    }
    if (t.closest("[data-tadd]")) {
      const n = wrap.querySelector("[data-tnew]");
      const name = n.value.trim();
      if (!name) return;
      const color = unhex(wrap.querySelector("[data-tnewcolor]").value);
      post({ Notes: { PutTag: { folder: st.sel, id: null, kind: KIND[st.kind], name, color } } });
      n.value = "";
      return;
    }

    const ee = t.closest("[data-eedit]");
    if (ee) {
      const e2 = entry(ee.dataset.eedit);
      return openEditor(subject(ee.dataset.eedit), e2?.name ?? "", st.sel);
    }
    const em = t.closest("[data-emove]");
    if (em) {
      const to = await pickFolder("Move to folder", { skip: null });
      if (to && to !== st.sel) post({ Notes: { MoveEntry: { from: st.sel, to, subject: subject(em.dataset.emove) } } });
      return;
    }
    const ed = t.closest("[data-edel]");
    if (ed) post({ Notes: { RemoveEntry: { folder: st.sel, subject: subject(ed.dataset.edel) } } });
  });
}

// ---------------------------------------------------------------------------------------------
// Export and import.

/// The Clipboard API only exists in a secure context, and a phone on the LAN reaches this page over
/// plain http. The selected text box is the fallback that always works.
async function copy(text, area) {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    area.value = text;
    area.focus();
    area.select();
    try {
      return document.execCommand("copy");
    } catch {
      return false;
    }
  }
}

async function openExport(id) {
  let data;
  try {
    const r = await fetch(`/api/notes/export/${encodeURIComponent(id)}`);
    if (!r.ok) return;
    data = await r.json();
  } catch {
    return;
  }
  modal(
    "nask nexport",
    `<h3>${ico("download-simple")} Export ${esc(data.name)}</h3>` +
      `<div class="mbtns"><button data-x="file">${ico("download-simple")} Download file</button>` +
      `<button data-x="json">${ico("copy")} Copy JSON</button>` +
      `<button data-x="compressed">${ico("copy")} Copy compressed</button></div>` +
      `<p class="nerr" data-said hidden></p>` +
      `<textarea class="jq nta" rows="4" readonly></textarea>`,
    (wrap) => {
      const area = wrap.querySelector("textarea");
      const said = wrap.querySelector("[data-said]");
      area.value = data.compressed;
      wrap.addEventListener("click", async (e) => {
        const b = e.target.closest("[data-x]");
        if (!b) return;
        if (b.dataset.x === "file") {
          const url = URL.createObjectURL(new Blob([data.json], { type: "application/json" }));
          const a = document.createElement("a");
          a.href = url;
          a.download = `${data.name.replace(/[^\w.-]+/g, "_") || "notes"}.spainotes.json`;
          document.body.append(a);
          a.click();
          a.remove();
          setTimeout(() => URL.revokeObjectURL(url), 1000);
          return;
        }
        const text = data[b.dataset.x];
        const ok = await copy(text, area);
        area.value = text;
        said.textContent = ok ? "Copied." : "Select the text below and copy it.";
        said.hidden = false;
      });
    }
  );
}

async function decode(text) {
  const t = text.trim();
  if (t.startsWith("{")) return JSON.parse(t);
  if (typeof DecompressionStream === "undefined") {
    throw new Error("This browser cannot read the compressed form. Paste the JSON form instead.");
  }
  const b64 = (t.startsWith(PREFIX) ? t.slice(PREFIX.length) : t).replace(/\s+/g, "").replace(/-/g, "+").replace(/_/g, "/");
  const bin = atob(b64);
  const bytes = Uint8Array.from(bin, (c) => c.charCodeAt(0));
  const stream = new Blob([bytes]).stream().pipeThrough(new DecompressionStream("deflate"));
  return JSON.parse(await new Response(stream).text());
}

function openImport() {
  const rows = flatten();
  modal(
    "nask nimport",
    `<h3>${ico("upload-simple")} Import a folder</h3>` +
      `<textarea class="jq nta" rows="5" placeholder="Paste exported text, or pick a file"></textarea>` +
      `<p><input type="file" accept=".json,.txt,application/json,text/plain" data-file></p>` +
      `<label class="nfield">Into <select data-parent><option value="">Top level</option>` +
      rows
        .map((r) => `<option value="${esc(r.folder.id)}">${"  ".repeat(r.depth + 1)}${esc(r.folder.name)}</option>`)
        .join("") +
      `</select></label>` +
      `<p class="nerr" data-err hidden></p>` +
      `<p class="rsave"><button data-go>Import</button></p>`,
    (wrap, done) => {
      const area = wrap.querySelector("textarea");
      const err = (m) => {
        const p = wrap.querySelector("[data-err]");
        p.textContent = m;
        p.hidden = !m;
      };
      wrap.querySelector("[data-file]").addEventListener("change", async (e) => {
        const file = e.target.files?.[0];
        if (file) area.value = await file.text();
      });
      wrap.querySelector("[data-go]").addEventListener("click", async () => {
        let exp;
        try {
          exp = await decode(area.value);
        } catch (x) {
          return err(x?.message?.includes("browser") ? x.message : "That is not exported notes text.");
        }
        if (exp?.format !== FORMAT || !exp.folder?.id) return err("That is not an EVE Spai notes export.");
        let mode = "Add";
        const parent = wrap.querySelector("[data-parent]").value || null;
        if (findFolder(exp.folder.id)) {
          const pick = await choose(
            `${exp.folder.name} is already here`,
            "Overwrite replaces the folder with the import. Merge keeps both, and the import wins wherever the two hold the same item.",
            [["Overwrite", "Overwrite", "mno"], ["Merge", "Merge", "mok"], ["cancel", "Cancel"]]
          );
          if (pick !== "Overwrite" && pick !== "Merge") return;
          mode = pick;
        }
        const ok = await post({ Notes: { Import: { export: exp, mode, parent } } });
        if (!ok) return err("Could not import. The web view may be read only.");
        done(true);
      });
    }
  );
}

// ---------------------------------------------------------------------------------------------

document.addEventListener("click", (e) => {
  const edit = e.target.closest(".snotes [data-note-edit]");
  if (edit) {
    const s = edit.closest(".snotes");
    return openEditor(subjectOf(s.dataset.nkind, s.dataset.nid, s.dataset.nname), s.dataset.nname);
  }
  const m = e.target.closest("[data-notes-manage]");
  if (m) openManager(m.dataset.notesManage);
});

/// Open dialogs follow the book. Only when it actually changed: this runs after every pane render,
/// and rebuilding the manager under a pointer mid-click loses the click.
let seenRev = null;
afterRender.push(() => {
  const rev = pane()?.rev ?? null;
  if (rev === seenRev) return;
  seenRev = rev;
  for (const s of document.querySelectorAll(".snotes")) {
    s.innerHTML = sectionBody(s.dataset.nkind, s.dataset.nid);
  }
  editor?.refresh();
  if (mgr && !document.querySelector(".nask")) {
    const focus = document.activeElement?.closest?.("[data-mq]") ? mgr.wrap.querySelector("[data-mq]")?.selectionStart : null;
    mgr.paint();
    if (focus != null) {
      const q = mgr.wrap.querySelector("[data-mq]");
      q?.focus();
      q?.setSelectionRange(focus, focus);
    }
  }
});

/// `#notes/system` and `#notes/pilot` open the manager, and `#notes/edit/<kind>/<id>[/<name>]` the
/// editor, for the same reason every dialog takes a deep link: a load-time screenshot cannot click.
function fromHash() {
  const m = /^#notes\/(system|pilot)$/.exec(location.hash);
  if (m) return openManager(m[1]);
  const e = /^#notes\/edit\/(system|pilot)\/(\d+)(?:\/(.+))?$/.exec(location.hash);
  if (e) {
    const name = e[3] ? decodeURIComponent(e[3]) : entryOf(e[1], e[2])?.name ?? e[2];
    openEditor(subjectOf(e[1], e[2], name), name);
  }
}
window.addEventListener("hashchange", fromHash);
fromHash();
