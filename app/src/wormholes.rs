//! Wormhole tracking (docs/WORMHOLES_AND_NEXT.md). A wormhole is a transient
//! connection located in a system, seeded from EVE-Scout (Thera/Turnur) and from
//! intel-channel reports. Lifetimes are bounded (2 days, 1 for drifters), so expired
//! entries are pruned.

const DAY: i64 = 86_400;

/// Where a wormhole leads. Either a space *class* (we don't know the exact system),
/// the special hubs Thera/Turnur, or a specific scouted system.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DestClass {
    Highsec,
    Lowsec,
    Nullsec,
    /// J-space / unknown wormhole space (a "disconnected" system).
    Wspace,
    Thera,
    Turnur,
    /// A specific system, known because someone scouted through.
    System(i64),
    Unknown,
}

impl DestClass {
    /// Short tag stored in the DB (paired with a system id for `System`).
    pub fn code(self) -> &'static str {
        match self {
            DestClass::Highsec => "hs",
            DestClass::Lowsec => "ls",
            DestClass::Nullsec => "ns",
            DestClass::Wspace => "wspace",
            DestClass::Thera => "thera",
            DestClass::Turnur => "turnur",
            DestClass::System(_) => "system",
            DestClass::Unknown => "unknown",
        }
    }

    pub fn from_code(code: &str, system_id: Option<i64>) -> DestClass {
        match code {
            "hs" => DestClass::Highsec,
            "ls" => DestClass::Lowsec,
            "ns" => DestClass::Nullsec,
            "wspace" => DestClass::Wspace,
            "thera" => DestClass::Thera,
            "turnur" => DestClass::Turnur,
            "system" => system_id.map_or(DestClass::Unknown, DestClass::System),
            _ => DestClass::Unknown,
        }
    }

    /// The specific destination system id, if known.
    pub fn system_id(self) -> Option<i64> {
        match self {
            DestClass::System(id) => Some(id),
            _ => None,
        }
    }

    /// Short human label (the specific-system case is rendered by the caller, which
    /// has the name lookup).
    pub fn label(self) -> &'static str {
        match self {
            DestClass::Highsec => "Highsec",
            DestClass::Lowsec => "Lowsec",
            DestClass::Nullsec => "Nullsec",
            DestClass::Wspace => "J-space",
            DestClass::Thera => "Thera",
            DestClass::Turnur => "Turnur",
            DestClass::System(_) => "System",
            DestClass::Unknown => "Unknown",
        }
    }
}

/// Largest hull that can transit, smallest → largest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShipSize {
    Frigate,
    Medium,
    Large,
    XLarge,
}

impl ShipSize {
    pub fn code(self) -> &'static str {
        match self {
            ShipSize::Frigate => "frigate",
            ShipSize::Medium => "medium",
            ShipSize::Large => "large",
            ShipSize::XLarge => "xlarge",
        }
    }

    pub fn from_code(code: &str) -> Option<ShipSize> {
        match code.to_ascii_lowercase().as_str() {
            "frigate" | "small" => Some(ShipSize::Frigate),
            "medium" => Some(ShipSize::Medium),
            "large" => Some(ShipSize::Large),
            // EVE-Scout reports caps as "capital"/"xlarge".
            "xlarge" | "capital" => Some(ShipSize::XLarge),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ShipSize::Frigate => "Frigate",
            ShipSize::Medium => "Medium",
            ShipSize::Large => "Large",
            ShipSize::XLarge => "XL / Capital",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    EveScout,
    Intel,
    Manual,
}

impl Source {
    pub fn code(self) -> &'static str {
        match self {
            Source::EveScout => "eve-scout",
            Source::Intel => "intel",
            Source::Manual => "manual",
        }
    }

    pub fn from_code(code: &str) -> Source {
        match code {
            "eve-scout" => Source::EveScout,
            "manual" => Source::Manual,
            _ => Source::Intel,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Source::EveScout => "EVE-Scout",
            Source::Intel => "Intel",
            Source::Manual => "Manual",
        }
    }
}

/// One known wormhole.
#[derive(Clone, Debug)]
pub struct Wormhole {
    /// DB row id (0 before insert).
    pub id: i64,
    /// System the wormhole is located in.
    pub system_id: i64,
    pub signature: Option<String>,
    pub wh_type: Option<String>,
    pub dest: DestClass,
    pub size: Option<ShipSize>,
    pub is_drifter: bool,
    pub reported_at: i64,
    /// Explicit end-of-life if a report gave one; otherwise derived from lifetime.
    pub explicit_expiry: Option<i64>,
    pub source: Source,
    pub updated_at: i64,
}

impl Wormhole {
    /// Maximum lifetime in seconds: 1 day for drifter holes, 2 days otherwise.
    pub fn max_life_secs(&self) -> i64 {
        if self.is_drifter {
            DAY
        } else {
            2 * DAY
        }
    }

    /// When the hole is considered gone.
    pub fn expiry(&self) -> i64 {
        self.explicit_expiry.unwrap_or(self.reported_at + self.max_life_secs())
    }

    pub fn is_expired(&self, now: i64) -> bool {
        now >= self.expiry()
    }

    /// Hours of life remaining (None once expired).
    pub fn hours_left(&self, now: i64) -> Option<i64> {
        let s = self.expiry() - now;
        (s > 0).then(|| (s + 3599) / 3600)
    }

    /// Identity for de-duplication: the signature pins it exactly; without one we fall
    /// back to the (system, type, destination) triple.
    pub fn dedup_key(&self) -> String {
        match &self.signature {
            Some(sig) if !sig.is_empty() => format!("{}|sig:{}", self.system_id, sig.to_uppercase()),
            _ => format!(
                "{}|{}|{}",
                self.system_id,
                self.wh_type.as_deref().unwrap_or("?").to_uppercase(),
                self.dest.code()
            ),
        }
    }

    /// Merge a fresher report of the same hole into this one: fill in optional fields
    /// we didn't have, and advance the freshness/expiry.
    pub fn merge_from(&mut self, other: &Wormhole) {
        self.signature = self.signature.clone().or_else(|| other.signature.clone());
        self.wh_type = self.wh_type.clone().or_else(|| other.wh_type.clone());
        self.size = self.size.or(other.size);
        self.is_drifter |= other.is_drifter;
        // A more specific destination wins (a scouted system beats a bare class).
        if matches!(self.dest, DestClass::Unknown)
            || (self.dest.system_id().is_none() && other.dest.system_id().is_some())
        {
            self.dest = other.dest;
        }
        if other.explicit_expiry.is_some() {
            self.explicit_expiry = other.explicit_expiry;
        }
        self.updated_at = self.updated_at.max(other.updated_at);
        // EVE-Scout is authoritative over an intel guess for the source label.
        if matches!(other.source, Source::EveScout) {
            self.source = Source::EveScout;
        }
    }
}

// --- EVE-Scout seeding -----------------------------------------------------

const SCOUT_URL: &str = "https://api.eve-scout.com/v2/public/signatures";
const SCOUT_POLL: std::time::Duration = std::time::Duration::from_secs(300);

#[derive(serde::Deserialize)]
struct ScoutSig {
    in_system_id: i64,
    in_signature: Option<String>,
    out_system_id: i64,
    out_system_name: Option<String>,
    wh_type: Option<String>,
    max_ship_size: Option<String>,
    remaining_hours: Option<i64>,
    signature_type: Option<String>,
    created_at: Option<String>,
}

/// Poll EVE-Scout's public Thera/Turnur signatures into the wormhole store.
pub fn spawn_scout(ctx: egui::Context) {
    std::thread::spawn(move || {
        let Ok(client) = reqwest::blocking::Client::builder()
            .user_agent("eve-spai/0.1 (EVE intel tool)")
            .timeout(std::time::Duration::from_secs(30))
            .build()
        else {
            return;
        };
        loop {
            if let Some(sigs) = fetch_scout(&client) {
                if let Ok(store) = crate::store::Store::open() {
                    let now = chrono::Utc::now().timestamp();
                    for s in &sigs {
                        if let Some(wh) = scout_to_wormhole(s, now) {
                            store.upsert_wormhole(&wh);
                        }
                    }
                    store.prune_wormholes(now);
                    ctx.request_repaint();
                }
            }
            std::thread::sleep(SCOUT_POLL);
        }
    });
}

fn fetch_scout(client: &reqwest::blocking::Client) -> Option<Vec<ScoutSig>> {
    client.get(SCOUT_URL).send().ok()?.error_for_status().ok()?.json().ok()
}

fn scout_to_wormhole(s: &ScoutSig, now: i64) -> Option<Wormhole> {
    if s.signature_type.as_deref() != Some("wormhole") {
        return None;
    }
    // The hole sits in the connected system (`in_system`) and leads to the hub.
    let dest = match s.out_system_name.as_deref() {
        Some("Thera") => DestClass::Thera,
        Some("Turnur") => DestClass::Turnur,
        _ => DestClass::System(s.out_system_id),
    };
    let reported = s.created_at.as_deref().and_then(parse_rfc3339).unwrap_or(now);
    Some(Wormhole {
        id: 0,
        system_id: s.in_system_id,
        signature: s.in_signature.clone(),
        wh_type: s.wh_type.clone(),
        dest,
        size: s.max_ship_size.as_deref().and_then(ShipSize::from_code),
        is_drifter: false,
        reported_at: reported,
        explicit_expiry: s.remaining_hours.map(|h| now + h * 3600),
        source: Source::EveScout,
        updated_at: now,
    })
}

fn parse_rfc3339(s: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(s).ok().map(|d| d.timestamp())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wh(drifter: bool, reported: i64) -> Wormhole {
        Wormhole {
            id: 0,
            system_id: 30000142,
            signature: None,
            wh_type: Some("K162".into()),
            dest: DestClass::Nullsec,
            size: None,
            is_drifter: drifter,
            reported_at: reported,
            explicit_expiry: None,
            source: Source::Intel,
            updated_at: reported,
        }
    }

    #[test]
    fn lifetime_caps() {
        let normal = wh(false, 1000);
        assert_eq!(normal.expiry(), 1000 + 2 * DAY);
        let drift = wh(true, 1000);
        assert_eq!(drift.expiry(), 1000 + DAY);
        assert!(!normal.is_expired(1000 + DAY));
        assert!(drift.is_expired(1000 + DAY));
    }

    #[test]
    fn explicit_expiry_overrides() {
        let mut w = wh(false, 1000);
        w.explicit_expiry = Some(1000 + 3600);
        assert_eq!(w.expiry(), 1000 + 3600);
        assert_eq!(w.hours_left(1000), Some(1));
        assert_eq!(w.hours_left(1000 + 3600), None);
    }

    #[test]
    fn dedup_prefers_signature() {
        let mut a = wh(false, 1000);
        a.signature = Some("abc-123".into());
        assert_eq!(a.dedup_key(), "30000142|sig:ABC-123");
        let b = wh(false, 1000); // no sig
        assert_eq!(b.dedup_key(), "30000142|K162|ns");
    }

    #[test]
    fn merge_fills_optionals_and_specializes_destination() {
        let mut base = wh(false, 1000); // dest Nullsec, no size
        let mut scouted = wh(false, 2000);
        scouted.dest = DestClass::System(31000005);
        scouted.size = Some(ShipSize::Large);
        scouted.source = Source::EveScout;
        base.merge_from(&scouted);
        assert_eq!(base.dest, DestClass::System(31000005));
        assert_eq!(base.size, Some(ShipSize::Large));
        assert_eq!(base.source, Source::EveScout);
        assert_eq!(base.updated_at, 2000);
    }

    #[test]
    fn dest_code_roundtrips() {
        for d in [
            DestClass::Highsec,
            DestClass::Lowsec,
            DestClass::Nullsec,
            DestClass::Wspace,
            DestClass::Thera,
            DestClass::Turnur,
            DestClass::Unknown,
        ] {
            assert_eq!(DestClass::from_code(d.code(), None), d);
        }
        assert_eq!(DestClass::from_code("system", Some(42)), DestClass::System(42));
    }
}
