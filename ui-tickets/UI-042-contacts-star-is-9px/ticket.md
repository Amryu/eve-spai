# UI-042 The contacts star is a 9px hit target

| | |
|---|---|
| **Severity** | Low |
| **Status** | Open |
| **Region** | the Directory pane rows in `jabber_ui` |
| **Reported by** | spun off UI-041 |

## Symptom

The star that adds or removes a sidebar contact is the smallest clickable thing in the app.

## Measured

`cargo test --bin eve-spai uitest_census -- --ignored --nocapture`, on the scene UI-041 added:

```
jabber_sidebar_directory   24 hit targets   smallest: 9px  Button "\u{e46a}" at [[113.9 198.5] - [122.9 224.5]]
```

9px wide by 26px tall, against the app's ~27px norm (UI-014, UI-019). It is 9px because the
glyph is `.small()` inside a frameless `Button` with no `min_size`, so the button allocates
exactly the glyph's advance width.

For contrast, the remove button UI-041 put next to it in the same row is 24px, and the two
now read as different classes of control despite doing comparable jobs.

## Notes

- UI-019's rule is that an icon control is judged against its neighbours rather than a px
  floor. The neighbour changed in UI-041, which is what makes this worth fixing now.
- The fix is the same one UI-041 used: drop `.small()` and give the button
  `min_size(vec2(24.0, 24.0))`. It was left out of UI-041 to keep that patch inside its own
  region.

## How to verify

The census line above should report a smallest target no worse than the 19px search field.
The fix is WRONG if the star grows a frame, or if the row height changes and the sidebar
rows stop lining up with the Channels pane.
