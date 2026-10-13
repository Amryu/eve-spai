//! A personal intel feed the user connects to: signed pulls, libsodium sealed-box answers, events
//! mapped into Spai's intel. Every protocol detail is a setting (`proto`), so no feed is built in.
//!
//! Isolated: its reports carry `IntelReport::origin`, which keeps them off phone pushes and every
//! sharing path. The assistant sees them only under their own Data access permission, and the
//! LAN web view shows them.

pub mod mapping;
pub mod poller;
pub mod proto;
pub mod wire;

/// One item as received, the systems it was mapped to, and when it arrived.
#[derive(Clone, Debug)]
pub struct Item {
    pub event: wire::Event,
    pub systems: Vec<String>,
    pub seen: i64,
}

/// The latest items, newest first: the feeds window lists them and the assistant reads them.
pub type SharedItems = std::sync::Arc<std::sync::Mutex<std::collections::VecDeque<Item>>>;

/// The assistant's permission for outside feed reports, under Intel.
pub const AI_PERM: &str = "intel.external";

/// Whether the assistant may see this report. A conversation that sees an outside feed's report
/// becomes opsec: no web lookups and no memories, so its content does not reach another service.
pub fn ai_sees(r: &crate::intel::IntelReport, allowed: bool, opsec: &std::sync::atomic::AtomicBool) -> bool {
    if r.origin.is_none() {
        return true;
    }
    if allowed {
        opsec.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    allowed
}

/// Whether a report may be relayed through a third-party push service.
pub fn may_push(r: &crate::intel::IntelReport) -> bool {
    r.origin.is_none()
}
