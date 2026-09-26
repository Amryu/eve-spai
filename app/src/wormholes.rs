const DAY: i64 = 86_400;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DestClass {
    Highsec,
    Lowsec,
    Nullsec,
    Wspace,
    Thera,
    Turnur,
    #[default]
    Unknown,
}

impl DestClass {
    pub fn code(self) -> &'static str {
        match self {
            DestClass::Highsec => "hs",
            DestClass::Lowsec => "ls",
            DestClass::Nullsec => "ns",
            DestClass::Wspace => "wspace",
            DestClass::Thera => "thera",
            DestClass::Turnur => "turnur",
            DestClass::Unknown => "unknown",
        }
    }

    pub fn from_code(code: &str) -> DestClass {
        match code {
            "hs" => DestClass::Highsec,
            "ls" => DestClass::Lowsec,
            "ns" => DestClass::Nullsec,
            "wspace" => DestClass::Wspace,
            "thera" => DestClass::Thera,
            "turnur" => DestClass::Turnur,
            _ => DestClass::Unknown,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            DestClass::Highsec => "Highsec",
            DestClass::Lowsec => "Lowsec",
            DestClass::Nullsec => "Nullsec",
            DestClass::Wspace => "J-space",
            DestClass::Thera => "Thera",
            DestClass::Turnur => "Turnur",
            DestClass::Unknown => "Unknown",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
        match code.trim().to_ascii_lowercase().replace('-', " ").as_str() {
            "frigate" | "frig" | "small" => Some(ShipSize::Frigate),
            "medium" | "med" => Some(ShipSize::Medium),
            "large" => Some(ShipSize::Large),
            "xlarge" | "xl" | "extra large" | "capital" => Some(ShipSize::XLarge),
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

    /// Compact tag for the wormhole intel badge (wormhole sizes read Small/Medium/Large/XL).
    pub fn short(self) -> &'static str {
        match self {
            ShipSize::Frigate => "Small",
            ShipSize::Medium => "Med",
            ShipSize::Large => "Large",
            ShipSize::XLarge => "XL",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Source {
    EveScout,
    #[default]
    Intel,
    Manual,
    /// Seen by one of this app's own characters moving through it.
    Auto,
}

impl Source {
    pub fn code(self) -> &'static str {
        match self {
            Source::EveScout => "eve-scout",
            Source::Intel => "intel",
            Source::Manual => "manual",
            Source::Auto => "auto",
        }
    }

    pub fn from_code(code: &str) -> Source {
        match code {
            "eve-scout" => Source::EveScout,
            "manual" => Source::Manual,
            "auto" => Source::Auto,
            _ => Source::Intel,
        }
    }

    /// Its bit in `Wormhole::seen_by`.
    pub fn bit(self) -> u8 {
        match self {
            Source::EveScout => 1,
            Source::Intel => 2,
            Source::Manual => 4,
            Source::Auto => 8,
        }
    }

    pub const ALL: [Source; 4] = [Source::EveScout, Source::Intel, Source::Manual, Source::Auto];

    pub fn label(self) -> &'static str {
        match self {
            Source::EveScout => "EVE-Scout",
            Source::Intel => "Intel",
            Source::Manual => "Manual",
            Source::Auto => "Auto-detected",
        }
    }
}

/// How long a hole has left, as its info window reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Life {
    OverDay,
    UnderDay,
    Under4h,
    Under1h,
    /// Past its reliable lifetime: it can close at any moment. CCP gives no length for this stage.
    Expired,
}

impl Life {
    pub const ALL: [Life; 5] = [Life::OverDay, Life::UnderDay, Life::Under4h, Life::Under1h, Life::Expired];

    pub fn short(self) -> &'static str {
        match self {
            Life::OverDay => ">1d",
            Life::UnderDay => "<1d",
            Life::Under4h => "<4h",
            Life::Under1h => "<1h",
            Life::Expired => "Expired",
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Life::OverDay => "gt1d",
            Life::UnderDay => "lt1d",
            Life::Under4h => "lt4h",
            Life::Under1h => "lt1h",
            Life::Expired => "expired",
        }
    }

    pub fn from_code(code: &str) -> Option<Life> {
        Life::ALL.into_iter().find(|l| l.code() == code)
    }

    pub fn label(self) -> &'static str {
        match self {
            Life::OverDay => "More than a day",
            Life::UnderDay => "Less than a day",
            Life::Under4h => "Less than 4 hours",
            Life::Under1h => "Less than an hour",
            Life::Expired => "Expired, closing any moment",
        }
    }

    /// The latest it can close, seen at `at`. More than a day says nothing about an end. An expired
    /// hole is kept for an hour, since nobody knows how long that stage lasts.
    pub fn closes_by(self, at: i64) -> Option<i64> {
        match self {
            Life::OverDay => None,
            Life::UnderDay => Some(at + DAY),
            Life::Under4h => Some(at + 4 * 3600),
            Life::Under1h | Life::Expired => Some(at + 3600),
        }
    }
}

/// How much of a hole's mass is left, as its info window reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mass {
    Fresh,
    Reduced,
    Critical,
}

impl Mass {
    pub const ALL: [Mass; 3] = [Mass::Fresh, Mass::Reduced, Mass::Critical];

    pub fn short(self) -> &'static str {
        match self {
            Mass::Fresh => ">50%",
            Mass::Reduced => "<50%",
            Mass::Critical => "<10%",
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Mass::Fresh => "fresh",
            Mass::Reduced => "reduced",
            Mass::Critical => "critical",
        }
    }

    pub fn from_code(code: &str) -> Option<Mass> {
        Mass::ALL.into_iter().find(|m| m.code() == code)
    }

    pub fn label(self) -> &'static str {
        match self {
            Mass::Fresh => "More than 50%",
            Mass::Reduced => "Less than 50%",
            Mass::Critical => "Less than 10%",
        }
    }
}

/// What a hole connects, for choosing which ones routes may use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HoleKind {
    Thera,
    Turnur,
    /// Straight to another part of k-space (S199, N944 and the like).
    Kspace,
    Jspace,
    Drifter,
    Pochven,
}

impl HoleKind {
    pub const ALL: [HoleKind; 6] =
        [HoleKind::Thera, HoleKind::Turnur, HoleKind::Kspace, HoleKind::Jspace, HoleKind::Drifter, HoleKind::Pochven];

    pub fn code(self) -> &'static str {
        match self {
            HoleKind::Thera => "thera",
            HoleKind::Turnur => "turnur",
            HoleKind::Kspace => "kspace",
            HoleKind::Jspace => "jspace",
            HoleKind::Drifter => "drifter",
            HoleKind::Pochven => "pochven",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            HoleKind::Thera => "Thera",
            HoleKind::Turnur => "Turnur",
            HoleKind::Kspace => "K-space",
            HoleKind::Jspace => "J-space",
            HoleKind::Drifter => "Drifter",
            HoleKind::Pochven => "Pochven",
        }
    }

    /// The kind of a hole from `a` to `b`, by the more particular of its two ends.
    pub fn of(geo: &crate::geo::Systems, a: i64, b: i64, drifter: bool) -> HoleKind {
        use crate::whdata::Class;
        let class = |id: i64| geo.info_of(id).map(|i| crate::whdata::class_of(id, i.security, &i.region));
        let (ca, cb) = (class(a), class(b));
        let either = |f: &dyn Fn(Class) -> bool| ca.is_some_and(f) || cb.is_some_and(f);
        if drifter || either(&|c| matches!(c, Class::Drifter(_))) {
            HoleKind::Drifter
        } else if either(&|c| c == Class::Thera) {
            HoleKind::Thera
        } else if either(&|c| c == Class::Turnur) {
            HoleKind::Turnur
        } else if either(&|c| c == Class::Pochven) {
            HoleKind::Pochven
        } else if either(&|c| matches!(c, Class::W(_))) || crate::geo::is_wormhole_system(a) || crate::geo::is_wormhole_system(b) {
            HoleKind::Jspace
        } else {
            HoleKind::Kspace
        }
    }
}

/// The biggest hull a hole of `jump_mass` kg passes.
pub fn size_for_jump_mass(jump_mass: u64) -> ShipSize {
    match jump_mass {
        m if m <= 5_000_000 => ShipSize::Frigate,
        m if m <= 62_000_000 => ShipSize::Medium,
        m if m <= 410_000_000 => ShipSize::Large,
        _ => ShipSize::XLarge,
    }
}

/// The sizes a hole of one of `codes` can be; every size when none of them is known.
pub fn sizes_for(codes: &[&str]) -> Vec<ShipSize> {
    let mut out: Vec<ShipSize> = codes
        .iter()
        .filter_map(|c| crate::whdata::hole_type(c))
        .filter(|t| t.jump_mass > 0)
        .map(|t| size_for_jump_mass(t.jump_mass))
        .collect();
    let all = [ShipSize::Frigate, ShipSize::Medium, ShipSize::Large, ShipSize::XLarge];
    if out.is_empty() {
        return all.to_vec();
    }
    out.sort_by_key(|s| all.iter().position(|a| a == s));
    out.dedup();
    out
}

/// One row of a probe scanner copy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScanSig {
    pub id: String,
    /// "Cosmic Signature" or "Cosmic Anomaly".
    pub kind: String,
    /// e.g. "Combat Site", "Wormhole"; empty until scanned.
    pub group: String,
    pub name: String,
}

fn is_sig_id(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 7 && b[..3].iter().all(u8::is_ascii_uppercase) && b[3] == b'-' && b[4..].iter().all(u8::is_ascii_digit)
}

/// Every row of a probe scanner copy (select all, copy): id, kind, group, name, strength, distance.
pub fn probe_scan(text: &str) -> Vec<ScanSig> {
    text.lines()
        .filter_map(|l| {
            let cols: Vec<&str> = l.split('\t').map(str::trim).collect();
            let id = *cols.first()?;
            (cols.len() >= 3 && is_sig_id(id)).then(|| ScanSig {
                id: id.to_owned(),
                kind: cols[1].to_owned(),
                group: cols[2].to_owned(),
                name: cols.get(3).map_or(String::new(), |n| (*n).to_owned()),
            })
        })
        .collect()
}

/// The signatures of a probe scanner copy (select all, copy): the id and the group, which is empty
/// until the signature is scanned. Only the ones that are or may be wormholes.
pub fn probe_sigs(text: &str) -> Vec<(String, String)> {
    probe_scan(text)
        .into_iter()
        .filter(|s| s.group.is_empty() || s.group.to_lowercase().contains("wormhole"))
        .map(|s| (s.id, s.group))
        .collect()
}

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

pub fn lookup_type(code: &str) -> Option<&'static Wh> {
    let code = code.trim();
    WH_TYPES.iter().find(|w| w.0.eq_ignore_ascii_case(code))
}

pub fn is_wh_code(token: &str) -> bool {
    lookup_type(token).is_some()
}

/// Every Fenris Creations wormhole signature code. `K162` is the generic exit (real type known
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

/// Whose facts win where two sources disagree. It decides fields only; the entry keeps the source
/// that first reported it.
fn rank(s: Source) -> u8 {
    match s {
        Source::EveScout => 3,
        Source::Manual => 2,
        Source::Auto => 1,
        Source::Intel => 0,
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Wormhole {
    pub id: i64,
    pub system_id: i64,
    pub signature: Option<String>,
    pub wh_type: Option<String>,
    pub dest: DestClass,
    pub dest_system_id: Option<i64>,
    pub dest_signature: Option<String>,
    pub dest_wh_type: Option<String>,
    pub size: Option<ShipSize>,
    pub is_drifter: bool,
    pub reported_at: i64,
    pub explicit_expiry: Option<i64>,
    /// Where the entry came from first. Kept as it is when other sources confirm it.
    pub source: Source,
    pub updated_at: i64,
    /// Every source that has reported this hole, as `Source::bit` flags.
    pub seen_by: u8,
    /// The character that went through, for an auto-detected hole.
    pub detected_by: Option<String>,
    pub jumped_at: Option<i64>,
    pub mass: Option<Mass>,
    pub life: Option<Life>,
    /// When the mass and time left were last read off the hole.
    pub observed_at: Option<i64>,
    pub note: Option<String>,
    /// Stable across machines, for syncing entries later.
    pub uid: String,
}

impl Wormhole {
    pub fn max_life_secs(&self) -> i64 {
        if self.is_drifter {
            3600
        } else {
            2 * DAY
        }
    }

    pub fn effective_size(&self) -> Option<ShipSize> {
        if self.is_drifter {
            Some(ShipSize::Large)
        } else {
            self.size
        }
    }

    pub fn expiry(&self) -> i64 {
        self.explicit_expiry.unwrap_or(self.reported_at + self.max_life_secs())
    }

    pub fn is_expired(&self, now: i64) -> bool {
        now >= self.expiry()
    }

    pub fn hours_left(&self, now: i64) -> Option<i64> {
        let s = self.expiry() - now;
        (s > 0).then(|| (s + 3599) / 3600)
    }

    pub fn dedup_key(&self) -> String {
        // One Thera and one Turnur hole per system: key on system+dest so an intel report and the
        // EVE-Scout entry for the same connection (differing signatures) don't split into two rows.
        if matches!(self.dest, DestClass::Thera | DestClass::Turnur) {
            return format!("{}|{}", self.system_id, self.dest.code());
        }
        // Normalise the signature to its scan id (the 3-char prefix before the dash) so a seeded
        // "ABC-123" and an intel "[ABC]" match.
        let sig_id = self.signature.as_deref().map(|s| {
            s.chars()
                .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
                .collect::<String>()
                .to_uppercase()
                .split('-')
                .next()
                .unwrap_or("")
                .to_owned()
        });
        match sig_id {
            Some(id) if !id.is_empty() => format!("{}|sig:{}", self.system_id, id),
            _ => format!(
                "{}|{}|{}",
                self.system_id,
                self.wh_type.as_deref().unwrap_or("?").to_uppercase(),
                self.dest.code()
            ),
        }
    }

    pub fn merge_from(&mut self, other: &Wormhole) {
        let auth = rank(other.source) >= rank(self.source);
        self.signature = self.signature.take().or_else(|| other.signature.clone());
        self.wh_type = self.wh_type.take().or_else(|| other.wh_type.clone());
        self.dest_system_id = self.dest_system_id.or(other.dest_system_id);
        self.dest_signature = self.dest_signature.take().or_else(|| other.dest_signature.clone());
        self.dest_wh_type = self.dest_wh_type.take().or_else(|| other.dest_wh_type.clone());
        if !matches!(other.dest, DestClass::Unknown) && (auth || matches!(self.dest, DestClass::Unknown)) {
            self.dest = other.dest;
        }
        self.merge_shared(other);
    }

    pub fn merge_shared(&mut self, other: &Wormhole) {
        let auth = rank(other.source) >= rank(self.source);
        if other.size.is_some() && (self.size.is_none() || auth) {
            self.size = other.size;
        }
        self.is_drifter |= other.is_drifter;
        if other.explicit_expiry.is_some() && (auth || self.explicit_expiry.is_none()) {
            self.explicit_expiry = other.explicit_expiry;
        }
        self.updated_at = self.updated_at.max(other.updated_at);
        self.seen_by |= self.source.bit() | other.source.bit() | other.seen_by;
        self.detected_by = self.detected_by.take().or_else(|| other.detected_by.clone());
        self.jumped_at = self.jumped_at.or(other.jumped_at);
        // A later reading of a hole's state is the better one: holes only ever degrade.
        if other.observed_at.is_some() && other.observed_at >= self.observed_at {
            self.mass = other.mass.or(self.mass);
            self.life = other.life.or(self.life);
            self.observed_at = other.observed_at;
        }
        self.note = self.note.take().or_else(|| other.note.clone());
    }

    pub fn confirm_far(&mut self, far: &Wormhole) {
        if self.dest_signature.is_none() {
            self.dest_signature = far.signature.clone();
        }
        if self.dest_wh_type.is_none() {
            self.dest_wh_type = far.wh_type.clone();
        }
        self.merge_shared(far);
    }
}

const SCOUT_URL: &str = "https://api.eve-scout.com/v2/public/signatures";
const SCOUT_POLL: std::time::Duration = std::time::Duration::from_secs(300);

#[derive(serde::Deserialize)]
struct ScoutSig {
    in_system_id: i64,
    in_signature: Option<String>,
    out_system_id: i64,
    out_system_name: Option<String>,
    out_signature: Option<String>,
    wh_type: Option<String>,
    max_ship_size: Option<String>,
    remaining_hours: Option<i64>,
    signature_type: Option<String>,
    created_at: Option<String>,
}

pub fn spawn_scout(ctx: egui::Context) {
    std::thread::spawn(move || {
        let Ok(client) = crate::http::client(30)
        else {
            return;
        };
        loop {
            if let Some(sigs) = fetch_scout(&client) {
                if let Ok(store) = crate::store::Store::open() {
                    let now = chrono::Utc::now().timestamp();
                    let mut keep = std::collections::HashSet::new();
                    for s in &sigs {
                        if let Some(wh) = scout_to_wormhole(s, now) {
                            keep.insert(store.upsert_wormhole(&wh));
                        }
                    }
                    store.retire_missing_evescout(&keep);
                    store.collapse_special_holes();
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
    // By the far system's id: everything that was not Turnur used to read as Thera.
    let dest = match s.out_system_id {
        crate::whdata::THERA => DestClass::Thera,
        crate::whdata::TURNUR => DestClass::Turnur,
        id if crate::geo::is_wormhole_system(id) => DestClass::Wspace,
        _ if s.out_system_name.as_deref() == Some("Turnur") => DestClass::Turnur,
        _ => DestClass::Unknown,
    };
    let reported = s.created_at.as_deref().and_then(parse_rfc3339).unwrap_or(now);
    Some(Wormhole {
        id: 0,
        system_id: s.in_system_id,
        signature: s.in_signature.clone(),
        wh_type: s.wh_type.clone(),
        dest,
        dest_system_id: Some(s.out_system_id),
        dest_signature: s.out_signature.clone(),
        dest_wh_type: None,
        size: s.max_ship_size.as_deref().and_then(ShipSize::from_code),
        is_drifter: false,
        reported_at: reported,
        explicit_expiry: s.remaining_hours.map(|h| now + h * 3600),
        source: Source::EveScout,
        updated_at: now,
        ..Default::default()
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
            dest_system_id: None,
            dest_signature: None,
            dest_wh_type: None,
            size: None,
            is_drifter: drifter,
            reported_at: reported,
            explicit_expiry: None,
            source: Source::Intel,
            updated_at: reported,
            ..Default::default()
        }
    }

    #[test]
    fn dedup_key_normalises_signature() {
        let mut a = wh(false, 0);
        a.signature = Some("ABC-123".into());
        let mut b = wh(false, 0);
        b.signature = Some("[abc]".into());
        assert_eq!(a.dedup_key(), b.dedup_key());
    }

    #[test]
    fn lifetime_caps() {
        let normal = wh(false, 1000);
        assert_eq!(normal.expiry(), 1000 + 2 * DAY);
        let drift = wh(true, 1000);
        assert_eq!(drift.expiry(), 1000 + 3600);
        assert_eq!(drift.effective_size(), Some(ShipSize::Large));
        assert!(!normal.is_expired(1000 + 3600));
        assert!(drift.is_expired(1000 + 3600));
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
        assert_eq!(a.dedup_key(), "30000142|sig:ABC");
        let b = wh(false, 1000);
        assert_eq!(b.dedup_key(), "30000142|K162|ns");
    }

    #[test]
    fn evescout_facts_win_over_intel() {
        let mut fact = wh(false, 1000);
        fact.dest = DestClass::Thera;
        fact.size = Some(ShipSize::Large);
        fact.source = Source::EveScout;
        let mut guess = wh(false, 2000);
        guess.dest = DestClass::Nullsec;
        fact.merge_from(&guess);
        assert_eq!(fact.dest, DestClass::Thera, "intel must not override the fact");
        assert_eq!(fact.size, Some(ShipSize::Large));
        assert_eq!(fact.source, Source::EveScout);
        assert_eq!(fact.updated_at, 2000);
    }

    #[test]
    fn a_probe_scanner_copy_gives_its_wormhole_sigs() {
        let scan = "ABC-123\tCosmic Signature\tWormhole\tUnstable Wormhole\t100,0%\t2,34 AU\n\
                    DEF-456\tCosmic Signature\tData Site\tForgotten Relay\t100,0%\t5 AU\n\
                    GHI-789\tCosmic Signature\t\t\t12,5%\t9 AU\n\
                    not a scan line";
        assert_eq!(
            probe_sigs(scan),
            vec![("ABC-123".to_owned(), "Wormhole".to_owned()), ("GHI-789".to_owned(), String::new())]
        );
    }

    /// Holes only degrade: the latest reading of time and mass stands, an older one never
    /// overwrites it.
    #[test]
    fn the_latest_reading_of_a_hole_wins() {
        let mut hole = wh(false, 1000);
        hole.mass = Some(Mass::Fresh);
        hole.life = Some(Life::OverDay);
        hole.observed_at = Some(1000);
        let mut later = wh(false, 2000);
        later.mass = Some(Mass::Critical);
        later.life = Some(Life::Under4h);
        later.observed_at = Some(2000);
        hole.merge_from(&later);
        assert_eq!((hole.mass, hole.life, hole.observed_at), (Some(Mass::Critical), Some(Life::Under4h), Some(2000)));
        let mut stale = wh(false, 3000);
        stale.mass = Some(Mass::Fresh);
        stale.observed_at = Some(1500);
        hole.merge_from(&stale);
        assert_eq!(hole.mass, Some(Mass::Critical), "an older reading does not undo a newer one");
        assert_eq!(Life::Under4h.closes_by(100), Some(100 + 4 * 3600));
        assert_eq!(Life::OverDay.closes_by(100), None);
    }

    #[test]
    fn sizes_follow_the_hole_type() {
        assert_eq!(sizes_for(&["C247"]), vec![ShipSize::Large]);
        assert_eq!(sizes_for(&["E004"]), vec![ShipSize::Frigate]);
        assert_eq!(sizes_for(&[]).len(), 4, "unknown type, any size");
        assert_eq!(sizes_for(&["S199"]), vec![ShipSize::XLarge]);
    }

    #[test]
    fn intel_fills_unknown_then_evescout_upgrades() {
        let mut base = wh(false, 1000);
        base.dest = DestClass::Unknown;
        let mut intel = wh(false, 1500);
        intel.dest = DestClass::Nullsec;
        base.merge_from(&intel);
        assert_eq!(base.dest, DestClass::Nullsec);
        let mut scout = wh(false, 2000);
        scout.dest = DestClass::Thera;
        scout.size = Some(ShipSize::XLarge);
        scout.source = Source::EveScout;
        base.merge_from(&scout);
        assert_eq!(base.dest, DestClass::Thera);
        assert_eq!(base.size, Some(ShipSize::XLarge));
        assert_eq!(base.source, Source::Intel, "the first report keeps the entry's origin");
        assert_ne!(base.seen_by & Source::EveScout.bit(), 0, "EVE-Scout is recorded as having seen it");
        assert_ne!(base.seen_by & Source::Intel.bit(), 0);
    }

    #[test]
    fn confirm_far_pairs_endpoints() {
        let mut conn = wh(false, 1000);
        conn.system_id = 30000142;
        conn.signature = Some("ABC-123".into());
        conn.dest = DestClass::Thera;
        conn.dest_system_id = Some(31000005);
        let mut far = wh(false, 1200);
        far.system_id = 31000005;
        far.signature = Some("XYZ-789".into());
        far.wh_type = Some("K162".into());
        conn.confirm_far(&far);
        assert_eq!(conn.dest_signature.as_deref(), Some("XYZ-789"));
        assert_eq!(conn.dest_wh_type.as_deref(), Some("K162"));
        assert_eq!(conn.signature.as_deref(), Some("ABC-123"));
        assert_eq!(conn.system_id, 30000142);
    }

    #[test]
    fn wh_catalogue_lookup() {
        let k = lookup_type("k162").expect("K162 present");
        assert_eq!(k.dest(), DestClass::Unknown);
        assert!(k.size().is_none());
        let j = lookup_type("J377").unwrap();
        assert_eq!(j.dest(), DestClass::Turnur);
        assert_eq!(j.size(), Some(ShipSize::Medium));
        assert!(lookup_type("B735").unwrap().is_drifter());
        assert!(!lookup_type("N968").unwrap().is_drifter());
        assert!(!is_wh_code("hello"));
        assert!(!is_wh_code("1DQ1"));
        assert!(is_wh_code("e587"));
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
            assert_eq!(DestClass::from_code(d.code()), d);
        }
    }
}
