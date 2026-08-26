# GAP-004 review cycle

**Status:** Closed for the popout surface. The in-app Jabber view remains uncovered.
**Branch:** `harness/gap-004-jabber-popout`
**Why now:** UI-020 and UI-021 are both in this surface and neither could be verified without it.


## Resolution

| | |
|---|---|
| **Outcome** | Popout covered |
| **Agent time** | 20.5 min across 1 round, 92 tool calls |
| **Patches rejected on review** | 0 |
| **App code changed** | 44/44 lines (added/removed), excluding the harness |
| **Harness code changed** | 249/2 lines |
| **Suite** | 435 to 437 passing |
| **Follow-ups** | unblocked UI-020, UI-021 |

## What landed

Four permanent scenes rendering `jabber_window_body`, all flowing through `uitest_layout`, the
census and the screenshots:

| Scene | Size | Covers |
|---|---|---|
| `jabber_popout` | 520x480 | the size a new popout actually opens at |
| `jabber_popout_drafting` | 520x480 | a 3-line draft, UI-021's case |
| `jabber_popout_dm` | 520x480 | DM tab active, the contact-star branch |
| `jabber_popout_min` | 360x260 | the popout's `with_min_inner_size`, tab bar overflowing |

Fixtures build a `JabberFrame` and a `JabberState` directly: 2 rooms, 1 DM, a 16-message history,
unread flags, a mention, a room MOTD. Plus `uitest_jabber_popout_renders_a_conversation`, which
fails if the scene silently degrades to the login form or to "No conversations in this window".

## The keyring, sidestepped rather than seamed

The gap ticket assumed a test hook would be needed for `jabber::has_password`. It was not.
`jabber_frame()` is the only caller, and the popout path never calls it, so constructing
`JabberFrame` directly with `configured: true` keeps the keyring out entirely. **Zero production
change for the hardest-looking part of this gap.**

## Visibility

44 lines in `app.rs`, every one a bare `pub(crate)` prefix, which I verified by pairing each `-`
against its `+`. `JabberFrame`'s fields, `Convo`, `ChannelRow`, `ChatWinKey`, `TabAction`,
`ChatWindow`, `jabber_window_body`, and four `SpaiApp` fields the body reads. Nothing in `jabber.rs`
needed touching; it was already public.

## A real bug in my own checker, found by this surface

The scene tripped two false positives in the text-overlap pass I added earlier, both of which would
hit **any** chat-shaped surface:

1. **Scrolled-away text.** A label keeps its true rect when scrolled out of the clip rect, so every
   `stick_to_bottom` history reported its offscreen rows as overlapping whatever sits above the
   viewport. egui emits a label's `TextRun` child only when it actually painted, which is the one
   signal in the tree that separates drawn from scrolled-away. Now filtered on that, with
   `inspect_ignores_text_scrolled_out_of_view` as a self-test.
2. **Wrapped lead-in.** A label that wraps onto a second row starts its galley box at the row's left
   edge, so the box swallows the nick that shared its first row, while the painted first row is
   indented clear. Suppressed on that specific shape: shared origin plus a taller box.

Both are narrowed to the text pass. The click-target, escape and overflow checks are untouched, and
**all four pre-existing self-tests still fire**, which I confirmed rather than assumed:
`inspect_catches_a_deliberate_overlap`, `inspect_catches_overlapping_text`,
`inspect_catches_a_widget_pushed_off_the_side` and `inspect_is_quiet_on_a_clean_layout` all pass.

That is the second time a new surface has exposed a fault in the checker rather than in the app.

## Census, reported honestly

```
       jabber_popout    4 hit targets   Button:3 Label:46 MultilineTextInput:1 ScrollBar:1 TextRun:24 Unknown:5
jabber_popout_drafting  4 hit targets   Button:3 Label:46 MultilineTextInput:1 ScrollBar:1 TextRun:26 Unknown:5
    jabber_popout_dm    5 hit targets   Button:4 Label:8  MultilineTextInput:1 TextRun:9  Unknown:5
   jabber_popout_min    4 hit targets   Button:3 Label:46 MultilineTextInput:1 ScrollBar:1 TextRun:10 Unknown:4
```

4 to 5 hit targets is far below the ~12 chrome baseline, but for a different reason than an empty
scene: this surface has no chrome and its content is text. 46 labels, 24 painted runs, the composer
and the scrollbar are all inspected. **The tab strip paints its own tabs and emits `Unknown` nodes
with no role, so the tabs are invisible to the hit-target checks.** Screenshots stay the primary
signal for the tab bar.

## UI-020 is now provable, and deliberately not proved yet

The agent built the scene with `ontop_pin` included and measured the result. `uitest_layout` fails
in all four scenes with:

```
[overlapping click targets] Button "PUSH_PIN" [[461.8 8.0] - [512.0 35.0]]
                        <-> Button "CARET"    [[481.0 6.0] - [514.0 33.0]]
```

The pin sits on the tab bar's overflow caret. That is UI-020 itself, so the pin was left out of the
scene to keep the suite green, and `ontop_pin` stays private.

The scenes deliberately carry content in the top-right corner for this. If UI-020 moves the pin into
the tab-bar row, these scenes cover it with no further work. If it keeps the `Area`, the fixer makes
`ontop_pin` `pub(crate)`, calls it from the scene, and the failure above is the before/after gate.

## Verification

| | Before | After |
|---|---|---|
| `cargo test --workspace` | 435 passed, 2 ignored (+32) | **437 passed**, 2 ignored (+32) |
| with `--features fc-rescue` | 461 passed | **463 passed** |
| `cargo test --bin eve-spai uitest` | 26 passed | 28 passed |

**No rendering changed**, which matters because the whole point was to add coverage without touching
behaviour. Verified by copying the tree, restoring the four modified files from `HEAD` in the copy,
and rendering both. Of 70 pre-existing PNGs: 33 byte-identical, 2 differing by at most 2/255 from
antialiasing, and 35 differing only in wall-clock bands (the top-bar clock, the status-bar RAM
figure, ping ages, alert countdowns, the resolving spinner's phase). No geometry difference
anywhere, which is consistent with a diff of nothing but `pub(crate)`.

## Still uncovered

The in-app Jabber view (`View::Jabber` through `root_central`) still renders nothing, because that
path does go through `jabber_frame()` and its keyring probe. This ticket covers the popout only.
