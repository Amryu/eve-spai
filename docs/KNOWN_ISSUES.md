# Known issues / to fix later

- **Intel alert window forgets size + position after timeout-close.** While open it
  stays where dragged/resized, but after it auto-hides on timeout and re-appears on
  the next alert it resets to the default size/position. Likely the
  `with_position`/`with_inner_size` builder values aren't re-applied when the
  immediate viewport reopens (and Wayland ignores app-set window position). Fix:
  explicitly re-send `ViewportCommand::OuterPosition` + `InnerSize` from the saved
  `alerts.window_pos`/`window_size` on each open. (app.rs `alert_window`)

- **EVEWorkbench fitting view does not work.** The "open fit" integration for the
  EVEWorkbench site fails. Investigate the URL format / handler in the fit-site
  selector (`FIT_SITES`) and ship/fit linking.
