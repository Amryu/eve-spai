# UI-046 Every joined room and every remembered DM gets a tab on every start

| | |
|---|---|
| **Severity** | High |
| **Status** | Open |
| **Region** | `jabber_reconcile`, `SpaiApp::build`, `sync_popout_settings` |
| **Reported by** | user |

## Symptom

The main window's open tabs are not remembered. On every start the tab bar is rebuilt from
scratch out of everything the app knows about, so tabs the user closed in the last session
come back.

The user's requirement: after a restart only the previously open tabs should be there, and
especially nothing that was closed.

## Measured

`jabber_tabs` starts `Vec::new()` (`app.rs:1144`), and nothing persists it. Pop-out windows
already persist theirs through `ChatWindowCfg { tabs, active, .. }`; the main window is the
only chat window whose tab set is thrown away on exit.

`jabber_reconcile` then repopulates it every frame:

```rust
for r in &f.rooms   { if !closed_rooms.contains(r) && !want.contains(r) { want.push(...) } }
for k in &f.dm_keys { if !closed_dms.contains(k)  && !want.contains(k) { want.push(...) } }
```

`f.rooms` is every joined room. `f.dm_keys` is every JID with stored chat history. So the tab
bar is "everything, minus an explicit closed-list", rebuilt from nothing on each start.

That makes the closed-lists load-bearing in a way they cannot support. They only ever hold
conversations the user closed *since the feature existed*, so any older room or DM reappears
forever, and a closed-list entry is the only thing standing between a tab and permanent
resurrection. UI-045 fixed one way that list stayed empty; this is the reason an empty list is
catastrophic rather than merely untidy.

## Wanted

1. The main window's tabs and active tab persist, like a pop-out's already do.
2. Restore exactly that set on start. Nothing else opens by itself.
3. `jabber_reconcile` stops bulk-adding from `f.rooms` and `f.dm_keys`.

New traffic must still be able to surface a conversation, or an incoming DM to someone with no
tab would be invisible outside the sidebar. That stays, narrowly: an unread DM and a mention in
a room, which is the UI-039 rule, and only when the conversation is not on a closed-list.

## Notes

- Restoring a tab for a room that is no longer joined is correct: history-only tabs already
  exist, and the Channels pane already renders that case struck-through.
- The pinned Rescue Mode rooms (UI-045) are about being *joined*, not about having a tab. They
  must not be force-opened here.
- `reconcile_tabs` takes the full wanted set across every window, so the main window's restored
  tabs and the pop-outs' restored tabs have to be in it or they will be pruned on frame one.

## How to verify

`cargo test --bin eve-spai jabber_tab_persist`. The fix is WRONG if a closed conversation ever
returns without new traffic, if a restored tab is pruned on the first frame, if an incoming DM
can no longer surface a tab, or if a fresh profile opens tabs it was never told to open.
