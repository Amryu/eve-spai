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

// --- Static wormhole-type catalogue ----------------------------------------

/// A wormhole signature code and what it nominally tells us. `dest`/`size` are the
/// type's nominal attributes (the real destination can be more specific once
/// scouted, and is `Unknown` for codes that vary, like K162 / k-space / LS-NS holes).
/// Sourced from the EVE University wiki (`Wormhole_attributes`).
pub struct Wh(pub &'static str, pub DestClass, pub Option<ShipSize>, pub bool);

impl Wh {
    pub fn dest(&self) -> DestClass {
        self.1
    }
    pub fn size(&self) -> Option<ShipSize> {
        self.2
    }
    pub fn is_drifter(&self) -> bool {
        self.3
    }
}

/// Look up a wormhole code (case-insensitive); `None` if it isn't a known code.
pub fn lookup_type(code: &str) -> Option<&'static Wh> {
    let code = code.trim();
    WH_TYPES.iter().find(|w| w.0.eq_ignore_ascii_case(code))
}

/// Is this token a valid wormhole signature code (e.g. "K162")?
pub fn is_wh_code(token: &str) -> bool {
    lookup_type(token).is_some()
}

/// Every CCP wormhole signature code. `K162` is the generic exit (real type known
/// only from the far side).
#[rustfmt::skip]
pub static WH_TYPES: &[Wh] = &[
    Wh("A009", DestClass::Wspace, Some(ShipSize::Frigate), false),
    Wh("A239", DestClass::Wspace, Some(ShipSize::Large), false),
    Wh("A641", DestClass::Highsec, Some(ShipSize::XLarge), false),
    Wh("A982", DestClass::Wspace, Some(ShipSize::Large), false),
    Wh("B041", DestClass::Wspace, Some(ShipSize::Large), false),
    Wh("B274", DestClass::Wspace, Some(ShipSize::Large), false),
    Wh("B449", DestClass::Highsec, Some(ShipSize::XLarge), false),
    Wh("B520", DestClass::Wspace, Some(ShipSize::Large), false),
    Wh("B735", DestClass::Wspace, Some(ShipSize::Large), true),
    Wh("C008", DestClass::Wspace, Some(ShipSize::Frigate), false),
    Wh("C125", DestClass::Wspace, Some(ShipSize::Medium), false),
    Wh("C140", DestClass::Wspace, Some(ShipSize::XLarge), false),
    Wh("C247", DestClass::Wspace, Some(ShipSize::Large), false),
    Wh("C248", DestClass::Wspace, Some(ShipSize::XLarge), false),
    Wh("C391", DestClass::Wspace, Some(ShipSize::XLarge), false),
    Wh("C414", DestClass::Wspace, Some(ShipSize::Large), true),
    Wh("C729", DestClass::Unknown, Some(ShipSize::Large), false),
    Wh("D364", DestClass::Wspace, Some(ShipSize::Large), false),
    Wh("D382", DestClass::Wspace, Some(ShipSize::Large), false),
    Wh("D792", DestClass::Wspace, Some(ShipSize::XLarge), false),
    Wh("D845", DestClass::Wspace, Some(ShipSize::Large), false),
    Wh("E004", DestClass::Wspace, Some(ShipSize::Frigate), false),
    Wh("E175", DestClass::Wspace, Some(ShipSize::Large), false),
    Wh("E545", DestClass::Wspace, Some(ShipSize::Large), false),
    Wh("E587", DestClass::Thera, Some(ShipSize::XLarge), false),
    Wh("F135", DestClass::Thera, Some(ShipSize::Large), false),
    Wh("F216", DestClass::Unknown, Some(ShipSize::Large), false),
    Wh("F353", DestClass::Thera, Some(ShipSize::Medium), false),
    Wh("G008", DestClass::Wspace, Some(ShipSize::Frigate), false),
    Wh("G024", DestClass::Wspace, Some(ShipSize::Large), false),
    Wh("H121", DestClass::Wspace, Some(ShipSize::Medium), false),
    Wh("H296", DestClass::Wspace, Some(ShipSize::XLarge), false),
    Wh("H900", DestClass::Wspace, Some(ShipSize::Large), false),
    Wh("I182", DestClass::Wspace, Some(ShipSize::Large), false),
    Wh("J244", DestClass::Wspace, Some(ShipSize::Medium), false),
    Wh("J377", DestClass::Turnur, Some(ShipSize::Medium), false),
    Wh("K162", DestClass::Unknown, None, false),
    Wh("K329", DestClass::Wspace, Some(ShipSize::Large), false),
    Wh("K346", DestClass::Wspace, Some(ShipSize::Large), false),
    Wh("L005", DestClass::Wspace, Some(ShipSize::Frigate), false),
    Wh("L031", DestClass::Thera, Some(ShipSize::XLarge), false),
    Wh("L477", DestClass::Wspace, Some(ShipSize::Large), false),
    Wh("L614", DestClass::Wspace, Some(ShipSize::Medium), false),
    Wh("M001", DestClass::Wspace, Some(ShipSize::Frigate), false),
    Wh("M164", DestClass::Thera, Some(ShipSize::Large), false),
    Wh("M267", DestClass::Wspace, Some(ShipSize::Large), false),
    Wh("M555", DestClass::Wspace, Some(ShipSize::XLarge), false),
    Wh("M609", DestClass::Wspace, Some(ShipSize::Medium), false),
    Wh("N062", DestClass::Wspace, Some(ShipSize::Large), false),
    Wh("N110", DestClass::Wspace, Some(ShipSize::Medium), false),
    Wh("N290", DestClass::Wspace, Some(ShipSize::Large), false),
    Wh("N432", DestClass::Wspace, Some(ShipSize::XLarge), false),
    Wh("N766", DestClass::Wspace, Some(ShipSize::Large), false),
    Wh("N770", DestClass::Wspace, Some(ShipSize::Large), false),
    Wh("N944", DestClass::Unknown, Some(ShipSize::XLarge), false),
    Wh("N968", DestClass::Wspace, Some(ShipSize::Large), false),
    Wh("O128", DestClass::Wspace, Some(ShipSize::Large), false),
    Wh("O477", DestClass::Wspace, Some(ShipSize::Large), false),
    Wh("O883", DestClass::Wspace, Some(ShipSize::Medium), false),
    Wh("P060", DestClass::Wspace, Some(ShipSize::Medium), false),
    Wh("Q003", DestClass::Nullsec, Some(ShipSize::Frigate), false),
    Wh("Q063", DestClass::Thera, Some(ShipSize::Medium), false),
    Wh("Q317", DestClass::Wspace, Some(ShipSize::Medium), false),
    Wh("R051", DestClass::Lowsec, Some(ShipSize::XLarge), false),
    Wh("R081", DestClass::Wspace, Some(ShipSize::Large), false),
    Wh("R259", DestClass::Wspace, Some(ShipSize::Large), true),
    Wh("R474", DestClass::Wspace, Some(ShipSize::Large), false),
    Wh("R943", DestClass::Wspace, Some(ShipSize::Large), false),
    Wh("S047", DestClass::Wspace, Some(ShipSize::Large), false),
    Wh("S199", DestClass::Unknown, Some(ShipSize::XLarge), false),
    Wh("S804", DestClass::Wspace, Some(ShipSize::Medium), false),
    Wh("S877", DestClass::Wspace, Some(ShipSize::Large), true),
    Wh("T405", DestClass::Wspace, Some(ShipSize::Large), false),
    Wh("T458", DestClass::Thera, Some(ShipSize::Medium), false),
    Wh("U210", DestClass::Wspace, Some(ShipSize::Large), false),
    Wh("U319", DestClass::Wspace, Some(ShipSize::XLarge), false),
    Wh("U372", DestClass::Unknown, Some(ShipSize::Large), false),
    Wh("U574", DestClass::Wspace, Some(ShipSize::Large), false),
    Wh("V283", DestClass::Nullsec, Some(ShipSize::XLarge), false),
    Wh("V301", DestClass::Wspace, Some(ShipSize::Medium), false),
    Wh("V753", DestClass::Wspace, Some(ShipSize::XLarge), false),
    Wh("V898", DestClass::Thera, Some(ShipSize::Large), false),
    Wh("V911", DestClass::Wspace, Some(ShipSize::XLarge), false),
    Wh("V928", DestClass::Wspace, Some(ShipSize::Large), true),
    Wh("W237", DestClass::Wspace, Some(ShipSize::XLarge), false),
    Wh("X450", DestClass::Nullsec, Some(ShipSize::Large), false),
    Wh("X702", DestClass::Wspace, Some(ShipSize::Large), false),
    Wh("X877", DestClass::Wspace, Some(ShipSize::Large), false),
    Wh("Y683", DestClass::Wspace, Some(ShipSize::Large), false),
    Wh("Y790", DestClass::Wspace, Some(ShipSize::Medium), false),
    Wh("Z006", DestClass::Wspace, Some(ShipSize::Frigate), false),
    Wh("Z060", DestClass::Nullsec, Some(ShipSize::Medium), false),
    Wh("Z142", DestClass::Nullsec, Some(ShipSize::XLarge), false),
    Wh("Z457", DestClass::Wspace, Some(ShipSize::Large), false),
    Wh("Z647", DestClass::Wspace, Some(ShipSize::Medium), false),
    Wh("Z971", DestClass::Wspace, Some(ShipSize::Medium), false),
];

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
    fn wh_catalogue_lookup() {
        // K162 is the generic exit: known code, no inferable size/destination.
        let k = lookup_type("k162").expect("K162 present");
        assert_eq!(k.dest(), DestClass::Unknown);
        assert!(k.size().is_none());
        // J377 leads to Turnur, medium (matches the live EVE-Scout sizing).
        let j = lookup_type("J377").unwrap();
        assert_eq!(j.dest(), DestClass::Turnur);
        assert_eq!(j.size(), Some(ShipSize::Medium));
        // Drifter flag is set for the five drifter holes.
        assert!(lookup_type("B735").unwrap().is_drifter());
        assert!(!lookup_type("N968").unwrap().is_drifter());
        // Negatives.
        assert!(!is_wh_code("hello"));
        assert!(!is_wh_code("1DQ1"));
        assert!(is_wh_code("e587"));
        // No duplicate codes.
        let mut codes: Vec<&str> = WH_TYPES.iter().map(|w| w.0).collect();
        codes.sort_unstable();
        let n = codes.len();
        codes.dedup();
        assert_eq!(codes.len(), n, "duplicate wormhole codes");
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
