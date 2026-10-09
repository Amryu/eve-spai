//! The assistant: models, the data they may read, and what they may do. No egui here; the tab and
//! the settings live in `app/ai_ui.rs`.

pub mod config;
pub mod perms;
pub mod provider;
pub mod secrets;
pub mod deps;
pub mod tools;
pub mod anthropic;
pub mod openai_compat;
pub mod sse;
pub mod session;
pub mod situation;
