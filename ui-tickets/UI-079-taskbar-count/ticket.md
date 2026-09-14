# UI-079 — the taskbar entry never shows the unread count

> "The task bar icon (not the tray) does not show the number of new messages"

The code for this was already there and had never worked on the reporter's desktop.
`sync_taskbar_badge` draws the number onto the window icon with `badge::draw` and hands the result
over as `ViewportCommand::Icon`. That is the whole story on Windows and on anything that takes a
window's icon at face value.

Plasma is not one of those. It matches a window to a `.desktop` file — here by `WM_CLASS`, which is
`eve-spai`, against the installed `eve-spai.desktop` — and then uses **that file's** `Icon=`. The
badged icon is composed every time the count moves, handed over, and ignored.

## Fix

`app/src/launcher.rs` broadcasts the Unity LauncherEntry signal, which is the API those desktops do
implement: Plasma's task manager has honoured it since 5.7, as have Dash-to-Dock and Latte.

```
path   /com/canonical/Unity/LauncherEntry
member com.canonical.Unity.LauncherEntry.Update
body   ("application://eve-spai.desktop", {count: <int64>, count-visible: <bool>})
```

Zero is sent as `count-visible: false` rather than as a zero, since a launcher showing "0 unread" is
worse than showing nothing — and hiding it is how the badge gets cleared at all.

The icon badge stays. The two cover different desktops and neither is redundant.

Notes on the dependency: `zbus` was already in the tree via accesskit and ashpd, with `blocking` and
`async-io` already enabled, so taking it directly changes no feature resolution. That matters here
specifically — the neighbouring comment in `Cargo.toml` records that forcing zbus into tokio mode
breaks keyring and notify-rust at runtime, so the new entry sits under the same target section and
takes the same features.

`EVE_SPAI_DESKTOP_ID` overrides the desktop file name, for a package that installs under another.
