# eframe 0.34.3, patched

A copy of the crates.io `eframe` 0.34.3 package, pulled in through `[patch.crates-io]` in the
workspace `Cargo.toml`. One change, in `src/native/run.rs`:

`WinitAppWrapper::new_events` runs `check_redraw_requests` inside
`event_loop_context::with_event_loop_context`.

Why: on Windows an invisible or minimized window gets no `RedrawRequested`, so eframe paints it
from `check_redraw_requests` on the timer path (`new_events`). That path had no event-loop
context. An immediate viewport first shown in such a pass (a pilot, system, ship, killmail or chat
pop-out opened from the alert or fleet ping window while the main window is minimized or in the
tray) could not create its window, so its callback never ran and egui panicked with
"egui backend is implemented incorrectly - the user callback was never called": no window, app
gone. Linux and macOS repaint hidden windows normally and never took that path.

Upstream (`emilk/egui` main) still has the unwrapped call as of October 2026. Drop this copy once
a release wraps it.
