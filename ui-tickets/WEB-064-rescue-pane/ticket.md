# WEB-064 &mdash; No rescue pane, and switching a pane on switches another off

| | |
|---|---|
| **Severity** | Medium |
| **Status** | Open |
| **Region** | `web/rescue.rs`, `assets/panes-rescue.*`, `assets/layout.js`, `app.rs` |
| **Reported by** | user |

## Ask

"I would like to add one more panel: The rescue feature. It should be an available panel if built with
the fc-rescue flag. If enabled, this panel is always active. Also, lift the limit that only 4 views can
be enabled at one time. However, the grid/columns will only show the first views that fit."

## Gap

The rescue mode is the one part of the app with no remote view at all, and it is the part most likely
to be run from a phone: the FC is coordinating, not sitting at the map.

The four-pane budget switched a pane off to make room for another, which meant the layout quietly
forgot a choice the user had made.
