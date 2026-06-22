//! Persisted application settings (M0 subset — docs/DESIGN.md §7.1 E10).

use serde::{Deserialize, Serialize};

use crate::theme::Theme;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub theme: Theme,
    pub nav_expanded: bool,
    /// Show times in EVE time (UTC) rather than local.
    pub use_eve_time: bool,
    /// EVE chat-log directory (empty = unset / auto-detect later).
    pub eve_logs_dir: String,
    /// EVE character-settings directory (empty = unset).
    pub eve_settings_dir: String,
    /// Intel chat channels to watch.
    pub intel_channels: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: Theme::default(),
            nav_expanded: false,
            use_eve_time: true,
            eve_logs_dir: String::new(),
            eve_settings_dir: String::new(),
            intel_channels: Vec::new(),
        }
    }
}
