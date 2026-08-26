//! Headless UI harness.
//!
//! Renders any UI surface to a PNG without launching the app (no real profile, no network, no
//! background threads, no display server) and drives hover/click through the AccessKit tree.
//!
//! Two tiers:
//! - `cargo test --bin eve-spai uitest` runs the layout and interaction assertions in [`checks`].
//!   No GPU, fast enough for every test run.
//! - `cargo test --bin eve-spai uitest -- --ignored --nocapture` additionally renders every scene
//!   to `target/uishots/*.png` for eyeballing.
//!
//! # Covering a dialog
//!
//! `App::ui` calls `root_chrome`, `root_central` and `root_dialogs`; the harness calls the same
//! three. To reach one more dialog:
//!
//! 1. Make its gate field on `SpaiApp` `pub(crate)`.
//! 2. `scenes::dialog_scene(name, size, |a| ...)`, setting the gate and whatever the body reads.
//!    Size it to the `[w, h]` the dialog passes to `dialog_viewport`; the ones that are plain
//!    `egui::Window`s or `Modal`s float, so give those room around them instead.
//! 3. Push it into `scenes::all()` and add its landmark string to
//!    `uitest_dialog_scenes_render_their_dialog`.
//!
//! `dialog_scene` handles the viewport routing (see its own docs). Watch for a gate field that is
//! not the whole story: `coalitions_window` also needs `coal_edit`, which the settings button that
//! opens it fills in.

pub(crate) mod checks;
pub(crate) mod fixtures;
pub(crate) mod harness;
mod scenes;
