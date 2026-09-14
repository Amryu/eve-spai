# Review

Tests: 719 passed, 0 failed. No new ones: the whole of this is one D-Bus broadcast, and a test that
emitted it would be writing to the developer's own session bus — a side effect on the real desktop,
which `safe-verification` rules out — while asserting nothing a reader could not see from the code.

Verified end to end instead, on the machine that reported it. The app was built, installed and
restarted under `dbus-monitor --session "interface='com.canonical.Unity.LauncherEntry'"`, which
caught the startup emission:

```
signal path=/com/canonical/Unity/LauncherEntry; interface=com.canonical.Unity.LauncherEntry; member=Update
   string "application://eve-spai.desktop"
   array [ dict entry( string "count" variant int64 0 )
           dict entry( string "count-visible" variant boolean false ) ]
```

Right path, right interface, right desktop id, right signature. `xprop` confirms the window's
`WM_CLASS` is `eve-spai`, which is what Plasma matches against `eve-spai.desktop`, so the association
the API depends on is in place.

**Not verified by me: that Plasma paints the badge.** Emission and window matching are both
confirmed; whether the panel renders it is on the reporter's screen and nowhere I can read. A test
signal with a visible count was sent for them to check.
