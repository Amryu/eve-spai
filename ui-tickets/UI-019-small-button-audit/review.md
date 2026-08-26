# UI-019 review cycle

**Status:** Fixed and verified
**Branch:** `fix/ui-019-small-button-audit`

## Resolution

| | |
|---|---|
| **Outcome** | 7 changed, 10 kept |
| **Agent time** | 17.7 min across 1 round, 87 tool calls |
| **Patches rejected on review** | 0 |
| **App code changed** | 29/12 lines (added/removed), excluding the harness |
| **Harness code changed** | 131/0 lines |
| **Suite** | 470 to 472 passing |
| **Follow-ups** | UI-030, UI-031 |

## Verdicts

Seven failed the UI-014 standard: a **text label**, sitting beside a full-size control in the same
row, at 17px against the app's 26-28px norm.

| site | renders | verdict |
|---|---|---|
| `app.rs:8492`, `8497` | **Remove**, **Re-auth** a character | fixed, next to a 26px checkbox |
| `app.rs:15441`, `15458` | **Edit** (channels/ships/characters, location) | fixed, the `requires:` chips a row up are 27px |
| `copysettings.rs:366`, `411`, `416` | **Set account**, **Set**, **Clear** | fixed, beside a 26px radio and 27px chips doing the same job |

Ten kept, all icon-only in dense rows: two delve911 ping chips, two dismiss X's on transient
banners, three jump-plan icons in the map, two rule-reorder arrows, and a sound-preview play button
inside a combobox popup.

On the character and alert-rule sites the same-row neighbour is a **checkbox rather than a button**,
but a checkbox is still floored at `interact_size.y`, so the 17-against-26 mismatch reads exactly as
UI-014's did. All seven changes are `small_button` to `button`; the height comes back from
`theme.rs:167`, nothing hardcoded.

## Correction to the ticket

It says "sixteen" and lists **seventeen** line numbers. `grep` finds seventeen `small_button` calls
outside `uitest/`, and each ticket line maps to one by a constant drift. The prose count was wrong,
not the list.

## A real defect the new scene found

`uitest_layout` went red the moment `view_alert_rules` existed:

```
[overlapping click targets] Button "Cyno in Delve" [[137.0 192.0]-[233.4 218.0]]
                        <-> Button "ARROW_DOWN"    [[228.0 196.5]-[261.0 213.5]]
```

`app.rs` reserved a hardcoded `54.0` for the two reorder buttons. They are 33px each plus two 8px
gaps, 82px, and the name button adds its own 20px of padding, so an ordinary rule name ran its click
rect **under the arrows**. Unrelated to `small_button`: the arrows are the same width either way.

Fixed at the cause, replacing the magic number with the arrows' measured width via `layout_no_wrap`
plus `button_padding` and `item_spacing`, so it tracks the theme.

This is the third time a newly-added scene has immediately failed `uitest_layout` on a pre-existing
bug. The pattern is reliable enough to expect.

## The cost, stated plainly

With the true reserve, rule names truncate earlier in the default 240px panel: "Hostiles near home"
shows as "Hostile…". That is the honest consequence of reserving space the arrows actually occupy;
before, the name only fitted by overrunning them.

The panel is user-resizable to 400px. The pre-existing `.max(40.0)` floor can still overlap if
dragged to the 180px minimum, which the harness cannot reach. **Filed as UI-030** rather than
accepted silently, since a heavily truncated rule name is a real usability cost even though it beats
an overlapping click target.

## Reachability, stated honestly

Two scenes added: `view_characters_rows` (seeding a second character that is expired and missing
scopes, the branch that draws Re-auth) and `view_alert_rules`. Both needed one `SpaiApp` field
opened to `pub(crate)`.

Still unreachable, and the verdicts there are **code-reading only, nothing measured or
screenshotted**: `rescue_window_body` (fc-rescue), `jump_plan_content` (map, GAP-002),
`copysettings::ui` (needs a store seam plus an on-disk EVE settings tree). Sites 3, 4 and 14 sit in
covered views but on branches the scenes do not hit; all are keeps, so nothing needed measuring.

**Three of the seven fixes therefore have no test**, the `copysettings` ones. Recorded rather than
implied.

## Census

| scene | before | after |
|---|---|---|
| `view_characters_rows` | 17px `Remove` | 18px nav-rail icon, no 17px target left |
| `view_alert_rules` | 17px, arrows **and all four Edits** | 17px, reorder arrows only, kept deliberately |

## Teeth

Both new tests teeth-checked by reverting the changed lines **by line number**, because
`if ui.button("Remove").clicked() {` appears five times in `app.rs` and a string revert would have
hit the wrong one. That is the trap UI-018 hit; good to see it avoided.

- `Remove is 17.0px tall against 27.0px for Add character`
- `Edit is 17.0px tall against 27.0px for the bubble chip`

The overlap fix is gated by `uitest_layout`; reverting just the `name_w` expression reproduces it.

## Verification

| | Before | After |
|---|---|---|
| `cargo test --workspace` | 470 passed, 5 ignored (+32) | **472 passed**, 5 ignored (+32) |
| with `--features fc-rescue` | 497 passed | 499 passed |
| `cargo test --bin eve-spai uitest` | 57 passed | 59 passed |
