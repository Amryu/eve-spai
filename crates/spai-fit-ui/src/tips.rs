//! What an item's tooltip says, by what kind of item it is: a few lines that matter for its kind
//! (fitting and activation for modules, damage and range for charges and drones, bonuses for
//! implants and boosters), each value as fitted where the fit changes it.

use std::collections::HashMap;

use spai_fit::dogma::{Op, TypeDef};
use spai_fit::ids::{AttrId, GroupId};
use spai_fit::Dogma;
use spai_ui::{tr, trf};

use crate::details::{label_of, value_text};
use crate::text;

/// Whether the fit made a value better or worse than the item's own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Plain,
    Better,
    Worse,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub label: String,
    pub value: String,
    pub tone: Tone,
    /// A lesser line: restrictions, side notes.
    pub weak: bool,
}

/// What kind of tooltip an item gets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Module,
    TurretAmmo,
    Missile,
    Bomb,
    Breacher,
    CapBooster,
    Probe,
    BurstCharge,
    Charge,
    Drone,
    Fighter,
    Implant,
    Booster,
    Other,
}

const CPU: AttrId = 50;
const POWER: AttrId = 30;
const CAP_NEED: AttrId = 6;
const DURATION: AttrId = 73;
const SPEED: AttrId = 51;
const DURATION_HIGH: AttrId = 3115;
const MAX_RANGE: AttrId = 54;
const FALLOFF: AttrId = 158;
const FALLOFF_EFFECTIVE: AttrId = 2044;
const RELOAD: AttrId = 1795;
const CHARGE_GROUP_1: AttrId = 604;
const MAX_GROUP_FITTED: AttrId = 1544;
const REACTIVATION: AttrId = 669;
const FUEL_QTY: AttrId = 714;
const FUEL_TYPE: AttrId = 713;
const CALIBRATION: AttrId = 1153;
const RIG_SIZE: AttrId = 1547;
const DRAWBACK: AttrId = 1138;
const DAMAGE: [AttrId; 4] = [114, 118, 117, 116];
const DAMAGE_MULT: AttrId = 64;
const RESIST_BONUS: [AttrId; 4] = [984, 987, 986, 985];
/// Armour, shield and hull resonances, EM, thermal, kinetic, explosive.
const RESONANCES: [(&str, [AttrId; 4]); 3] = [("Armor", [267, 270, 269, 268]), ("Shield", [271, 274, 273, 272]), ("Hull", [974, 977, 976, 975])];
/// Per-cycle amounts that read better as a rate too.
const PER_CYCLE: [AttrId; 6] = [68, 84, 83, 90, 97, 77];
/// Never in a tooltip: mass, volume, capacity, hit points, skills, fitting restrictions, heat.
const HIDDEN: [AttrId; 32] = [4, 38, 161, 9, 162, 422, 182, 183, 184, 277, 278, 279, 1211, 1212, 1180, 1205, 1206, 1208, 1210, 604, 605, 606, 609, 610, 128, 633, 1544, 669, 714, 713, 1795, 1138];

pub fn kind(dogma: &Dogma, t: &TypeDef) -> Kind {
    let g = t.group;
    match t.category {
        7 | 32 => Kind::Module,
        18 => Kind::Drone,
        87 => Kind::Fighter,
        20 if g == 303 => Kind::Booster,
        20 => Kind::Implant,
        8 => match g {
            83 | 85 | 86 | 372..=377 | 1987 | 1989 | 4061 | 4062 | 4186 => Kind::TurretAmmo,
            384..=387 | 89 | 772 | 476 | 1019 | 648 | 653..=657 | 1677 | 1678 | 394..=396 | 88 | 1546 | 1547 => Kind::Missile,
            90 | 863 | 864 | 1548 => Kind::Bomb,
            4808 => Kind::Breacher,
            87 => Kind::CapBooster,
            479 | 492 | 548 | 4088 => Kind::Probe,
            1769 | 1771..=1774 | 4905 => Kind::BurstCharge,
            _ => Kind::Charge,
        },
        _ => {
            let _ = dogma;
            Kind::Other
        }
    }
}

struct Ctx<'a> {
    dogma: &'a Dogma,
    t: &'a TypeDef,
    fitted: Option<&'a HashMap<AttrId, f64>>,
}

impl Ctx<'_> {
    fn base(&self, a: AttrId) -> Option<f64> {
        self.t.attr(a).map(f64::from)
    }

    /// The value as fitted where the fit has one, else the item's own; none when it has none.
    fn get(&self, a: AttrId) -> Option<f64> {
        let base = self.base(a);
        self.fitted.and_then(|f| f.get(&a)).copied().or(base)
    }

    fn nz(&self, a: AttrId) -> Option<f64> {
        self.get(a).filter(|v| v.abs() > 1e-9)
    }

    fn tone(&self, a: AttrId) -> Tone {
        let (Some(v), Some(b)) = (self.get(a), self.base(a)) else { return Tone::Plain };
        if (v - b).abs() <= 1e-9 * b.abs().max(1.0) {
            return Tone::Plain;
        }
        let good = self.dogma.attr(a).is_none_or(|d| d.high_is_good);
        if (v > b) == good { Tone::Better } else { Tone::Worse }
    }

    fn unit(&self, a: AttrId) -> Option<u8> {
        self.dogma.attr(a).and_then(|d| d.unit)
    }

    /// A value in its unit, bonuses signed.
    fn fmt(&self, a: AttrId, v: f64) -> String {
        match self.unit(a) {
            Some(105) => format!("{}%", signed(v)),
            Some(111) => format!("{}%", signed((1.0 - v) * 100.0)),
            None if (v - v.round()).abs() < 1e-9 => format!("{v:.0}"),
            Some(10 | 11) => format!("{} m/s", text::num(v)),
            Some(117) => size_class(v).to_owned(),
            _ if a == 160 => format!("{} rad/s", sig3(v)),
            u => value_text(self.dogma, v, u),
        }
    }

    fn line(&self, a: AttrId) -> Option<Line> {
        let v = self.nz(a)?;
        Some(Line { label: label_of(self.dogma, a), value: self.fmt(a, v), tone: self.tone(a), weak: false })
    }

    /// A display-named, non-zero attribute that is no noise.
    fn shown(&self, a: AttrId) -> bool {
        !HIDDEN.contains(&a) && self.dogma.attr(a).is_some_and(|d| !d.display.is_empty()) && self.nz(a).is_some()
    }

    /// The cycle in seconds: duration, else rate of fire, else the active time.
    fn cycle_s(&self) -> Option<f64> {
        [DURATION, SPEED, DURATION_HIGH].iter().find_map(|a| self.nz(*a)).map(|ms| ms / 1000.0)
    }

    /// The item's own attributes its own effects modify something with, each with the operation
    /// that uses it, in the order the effects list them.
    fn drivers(&self) -> Vec<(AttrId, Op, Option<String>)> {
        let mut out: Vec<(AttrId, Op, Option<String>)> = Vec::new();
        for (e, _) in &self.t.effects {
            let Some(def) = self.dogma.effect(*e) else { continue };
            if def.usage_chance_attr.is_some() {
                continue;
            }
            for m in &def.modifiers {
                if let Some(seen) = out.iter_mut().find(|(a, ..)| *a == m.modifying) {
                    // The same bonus onto several skills' items says no one skill.
                    let scope = match m.skill {
                        Some(spai_fit::dogma::SkillRef::Of(s)) => self.dogma.type_(s).map(|t| t.name.clone()),
                        _ => None,
                    };
                    if seen.2 != scope {
                        seen.2 = None;
                    }
                    continue;
                }
                if self.nz(m.modifying).is_none() || HIDDEN.contains(&m.modifying) || (175..=179).contains(&m.modifying) || m.modifying == 447 {
                    continue;
                }
                if matches!(m.op, Op::PreMul | Op::PostMul) && (self.get(m.modifying).unwrap_or(1.0) - 1.0).abs() < 1e-9 {
                    continue;
                }
                if self.dogma.attr(m.modifying).is_none_or(|d| d.display.is_empty()) || m.modifying == spai_fit::dogma::attr::SKILL_LEVEL {
                    continue;
                }
                let scope = match m.skill {
                    Some(spai_fit::dogma::SkillRef::Of(s)) => self.dogma.type_(s).map(|t| t.name.clone()),
                    _ => None,
                };
                out.push((m.modifying, m.op, scope));
            }
        }
        out
    }

    fn driver_lines(&self, max: usize) -> Vec<Line> {
        self.drivers()
            .into_iter()
            .take(max)
            .map(|(a, op, scope)| {
                let v = self.get(a).unwrap_or(0.0);
                let value = match op {
                    Op::PostPercent => format!("{}%", signed(v)),
                    Op::PreMul | Op::PostMul => format!("{}%", signed((v - 1.0) * 100.0)),
                    Op::ModAdd => format!("+{}", whole(v)),
                    Op::ModSub => format!("\u{2212}{}", text::num(v)),
                    Op::PostAssign | Op::PreAssign => format!("= {}", text::num(v)),
                    _ => self.fmt(a, v),
                };
                let label = short_label(&label_of(self.dogma, a));
                let label = match scope {
                    Some(s) if generic(&label) => format!("{label} ({s})"),
                    _ => label,
                };
                Line { label, value, tone: self.tone(a), weak: false }
            })
            .collect()
    }

    /// Total damage and its split: `24 HP · Kin 58% Th 42%`.
    fn damage(&self, ids: [AttrId; 4], times: f64) -> Option<(f64, String)> {
        let parts: Vec<f64> = ids.iter().map(|a| self.get(*a).unwrap_or(0.0).max(0.0)).collect();
        let total: f64 = parts.iter().sum();
        if total <= 0.0 {
            return None;
        }
        let names = [tr!("EM"), tr!("Th"), tr!("Kin"), tr!("Exp")];
        let shares: Vec<String> = parts.iter().zip(names).filter(|(p, _)| **p > 0.0).map(|(p, n)| if *p >= total { n.to_owned() } else { format!("{n} {:.0}%", p / total * 100.0) }).collect();
        Some((total * times, format!("{} HP · {}", text::num(total * times), shares.join(" "))))
    }

    /// A missile's reach: speed × flight time.
    fn flight(&self) -> Option<Line> {
        let (v, t) = (self.nz(37)?, self.nz(281)? / 1000.0);
        Some(Line { label: tr!("Range").to_owned(), value: format!("{} ({} m/s · {})", text::metres(v * t), text::num(v), text::secs(t)), tone: self.tone(37), weak: false })
    }
}

fn signed(v: f64) -> String {
    let s = if (v - v.round()).abs() < 0.05 { format!("{:+.0}", v) } else { format!("{v:+.1}") };
    s.replace('-', "\u{2212}")
}

fn whole(v: f64) -> String {
    if (v - v.round()).abs() < 1e-9 { format!("{v:.0}") } else { text::num(v) }
}

fn sig3(v: f64) -> String {
    if v >= 100.0 { format!("{v:.0}") } else if v >= 1.0 { format!("{v:.2}") } else { format!("{v:.3}") }
}

fn size_class(v: f64) -> &'static str {
    match v.round() as i64 {
        1 => tr!("Small"),
        2 => tr!("Medium"),
        3 => tr!("Large"),
        4 => tr!("Capital"),
        _ => "",
    }
}

/// "Modification of Optimal Range Bonus" reads "Optimal Range".
fn short_label(s: &str) -> String {
    s.trim_start_matches("Modification of ").trim_end_matches(" Bonus").to_owned()
}

/// A bonus name that says nothing without what it applies to.
fn generic(label: &str) -> bool {
    ["Duration", "Capacitor Need", "Damage Multiplier", "Optimal Range Modifier", "Rate of Fire", "CPU Need", "Repair", "Shield Boost", "Velocity Modifier"].iter().any(|g| label.starts_with(g))
}

/// The lines of an item's tooltip, at most a handful, as fitted where `fitted` has values.
pub fn lines(dogma: &Dogma, t: &TypeDef, fitted: Option<&HashMap<AttrId, f64>>) -> Vec<Line> {
    let c = Ctx { dogma, t, fitted };
    let mut out = match kind(dogma, t) {
        Kind::Module => module(&c),
        Kind::TurretAmmo => {
            let mut v: Vec<Line> = c.damage(DAMAGE, 1.0).map(|(_, s)| Line { label: tr!("Damage").to_owned(), value: s, tone: c.tone(117), weak: false }).into_iter().collect();
            v.extend(c.driver_lines(5));
            v
        }
        Kind::Missile | Kind::Bomb | Kind::Breacher => missile(&c),
        Kind::CapBooster => {
            let mut v: Vec<Line> = c.line(67).into_iter().collect();
            if let (Some(gj), Some(m3)) = (c.nz(67), c.nz(161)) {
                v.push(Line { label: tr!("Per m³").to_owned(), value: format!("{} GJ", text::num(gj / m3)), tone: Tone::Plain, weak: true });
            }
            v
        }
        Kind::Probe => [1371, 1370, 1372, 2697, 103, 20, 2279, 2280, 281, 600].iter().filter_map(|a| c.line(*a)).take(5).collect(),
        Kind::BurstCharge => burst_charge(&c),
        Kind::Charge => {
            let mut v = c.driver_lines(5);
            if v.is_empty() {
                v = generic_lines(&c, 4);
            }
            v
        }
        Kind::Drone => drone(&c),
        Kind::Fighter => fighter(&c),
        Kind::Implant => {
            let mut v = attribute_bonuses(&c);
            let more: Vec<Line> = c.driver_lines(4).into_iter().filter(|l| !v.iter().any(|x| x.label == l.label)).collect();
            v.extend(more);
            v
        }
        Kind::Booster => booster(&c),
        Kind::Other => generic_lines(&c, 6),
    };
    out.truncate(9);
    out
}

/// The subtitle's extra: slot of an implant or booster, size of a charge or rig, squadron size.
pub fn subtitle(dogma: &Dogma, t: &TypeDef) -> Option<String> {
    let c = Ctx { dogma, t, fitted: None };
    match kind(dogma, t) {
        Kind::Implant => c.nz(331).map(|s| trf!("Slot {n}", n = s.round() as i64)),
        Kind::Booster => c.nz(1087).map(|s| trf!("Booster slot {n}", n = s.round() as i64)),
        Kind::Fighter => c.nz(2215).map(|s| trf!("{n} a squadron", n = s.round() as i64)),
        Kind::Module => c.nz(RIG_SIZE).or_else(|| c.nz(128).filter(|_| [53, 55, 74, 1986, 4060].contains(&t.group))).map(|v| size_class(v).to_owned()).filter(|s| !s.is_empty()),
        Kind::TurretAmmo => c.nz(128).map(|v| size_class(v).to_owned()).filter(|s| !s.is_empty()),
        _ => None,
    }
}

/// What the common block of a module already says.
const COMMON: [AttrId; 12] = [CPU, POWER, CAP_NEED, DURATION, SPEED, DURATION_HIGH, MAX_RANGE, FALLOFF, FALLOFF_EFFECTIVE, CALIBRATION, RIG_SIZE, 331];

fn generic_lines(c: &Ctx, max: usize) -> Vec<Line> {
    c.t.attrs.iter().map(|(a, _)| *a).filter(|a| c.shown(*a) && !COMMON.contains(a)).take(max).filter_map(|a| c.line(a)).collect()
}

/// Which family a module's rows come from, by group: the rows after the common block.
fn module_rows(group: GroupId) -> Option<&'static [AttrId]> {
    Some(match group {
        53 | 55 | 74 | 1986 | 4060 => &[DAMAGE_MULT, 160, 2733, 2734, 653, 654],
        46 | 1189 | 1533 | 4769 => &[20, 796, 554, 147, 973, 2066],
        40 | 62 | 63 | 1156 | 1199 => &[68, 84, 83, 1886],
        41 | 67 | 325 | 585 | 1697 | 1698 | 2018 => &[68, 84, 83, 90, 2796, 2797, 1886],
        38 | 329 | 78 => &[72, 1159, 983, 796, 150, 149, 169],
        59 | 205 | 302 | 367 | 1988 | 4067 | 645 | 407 => &[DAMAGE_MULT, 213, 1255, 204],
        213 | 211 | 1396 | 1395 | 646 | 1292 | 209 => &[767, 351, 349, 847, 848, 547, 596],
        212 | 210 | 290 | 96 => &[309, 566, 235, 1027, 143],
        514 => &[1130, 1536],
        65 | 1672 => &[20],
        52 => &[105],
        899 => &[103, 554],
        2008 => &[105, 2603, 1131],
        71 => &[97, 2451],
        68 => &[90],
        201 | 80 => &[238, 239, 240, 241, 142],
        208 => &[309, 566],
        379 => &[554],
        291 => &[767, 351, 349, 847, 848, 547, 596],
        72 => &[99, 114, 116, 117, 118],
        76 => &[],
        61 => &[67, 2267],
        39 | 43 | 57 | 285 | 339 | 766 | 767 | 768 | 769 | 770 => &[144, 147, 134, 146, 145, 202, 549, 548],
        330 => &[560, 306, 3118],
        658 | 590 | 4127 | 4184 => &[1334],
        1299 => &[1296],
        1289 => &[1950],
        1770 | 316 => &[2535, 833],
        644 => &[20],
        647 => &[459],
        954 | 956 | 957 | 958 => &[1374, 1375, 1376, 1368, 1369, 11, 48, 263, 1159, 2688, 482, 1271, 283],
        54 | 464 | 483 | 2004 | 737 | 4138 => &[77, 789, 3154, 3153, 5967, 5969],
        546 => &[434, 1082],
        481 => &[846],
        1223 | 1238 => &[1907, 1905, 1906],
        538 => &[1910, 1909, 1911],
        1122 => &[902],
        650 => &[1045],
        47 => &[126],
        48 => &[125],
        315 => &[105, 309, 565, 3124],
        4117 => &[309, 565, 3124],
        1154 => &[3113, 3114],
        762 => &[169, 554],
        763 => &[1076, 169, 150],
        764 => &[1076, 149],
        765 => &[149, 306, 150],
        // Covered by the common block and the resist line.
        501 | 506..=512 | 524 | 771 | 862 | 1245 | 1673 | 1674 | 4807 | 60 | 77 | 98 | 295 | 326 | 328 | 1699 | 1700 => &[],
        1150 => &[],
        _ => return None,
    })
}

fn module(c: &Ctx) -> Vec<Line> {
    let mut out = Vec::new();
    let rig = c.nz(CALIBRATION).is_some() && c.t.category == 7 && c.base(CPU).is_none_or(|v| v == 0.0);
    // Fitting.
    if rig {
        out.push(Line { label: tr!("Calibration").to_owned(), value: text::num(c.get(CALIBRATION).unwrap_or(0.0)), tone: Tone::Plain, weak: false });
    } else if c.nz(CPU).is_some() || c.nz(POWER).is_some() {
        let value = format!("{} tf · {} MW", text::num(c.get(CPU).unwrap_or(0.0)), text::num(c.get(POWER).unwrap_or(0.0)));
        let tone = [c.tone(CPU), c.tone(POWER)].into_iter().find(|t| *t != Tone::Plain).unwrap_or(Tone::Plain);
        out.push(Line { label: tr!("CPU · Powergrid").to_owned(), value, tone, weak: false });
    }
    // Activation.
    let cycle = c.cycle_s();
    match (c.nz(CAP_NEED), cycle) {
        (Some(cap), Some(s)) => out.push(Line { label: tr!("Activation").to_owned(), value: format!("{} GJ / {} ({} GJ/s)", text::num(cap), text::secs(s), text::num(cap / s)), tone: c.tone(CAP_NEED), weak: false }),
        (None, Some(s)) => out.push(Line { label: tr!("Cycle").to_owned(), value: text::secs(s), tone: [c.tone(DURATION), c.tone(SPEED)].into_iter().find(|t| *t != Tone::Plain).unwrap_or(Tone::Plain), weak: false }),
        _ => {}
    }
    // Reach.
    if let Some(r) = c.nz(MAX_RANGE) {
        let fall = c.nz(FALLOFF_EFFECTIVE).or_else(|| c.nz(FALLOFF));
        let value = match fall {
            Some(f) => format!("{} + {}", text::metres(r), text::metres(f)),
            None => text::metres(r),
        };
        out.push(Line { label: tr!("Range").to_owned(), value, tone: c.tone(MAX_RANGE), weak: false });
    } else if let Some(r) = [99, 142, 103].iter().find_map(|a| c.nz(*a)) {
        out.push(Line { label: tr!("Radius").to_owned(), value: text::metres(r), tone: Tone::Plain, weak: false });
    }
    // The family's own rows; resists folded to one line.
    let rows = module_rows(c.t.group);
    out.extend(resist_lines(c));
    for a in rows.unwrap_or(&[]) {
        if [99, 142, 103].contains(a) && out.iter().any(|l| l.label == tr!("Radius")) {
            continue;
        }
        let Some(mut l) = c.line(*a) else { continue };
        if (*a == 147 || *a == 148 || *a == 150 || *a == 149 || *a == 145 || *a == 146 || *a == 202 || *a == 306) && (c.get(*a).unwrap_or(1.0) - 1.0).abs() < 1e-9 {
            continue;
        }
        if a == &DAMAGE_MULT && c.t.category == 7 && ![53, 55, 74, 1986, 4060].contains(&c.t.group) {
            l.value = format!("{}%", signed((c.get(*a).unwrap_or(1.0) - 1.0) * 100.0));
        }
        if PER_CYCLE.contains(a) {
            if let Some(s) = cycle {
                l.value = format!("{} ({}/s)", l.value, l.value.split_whitespace().last().map_or_else(String::new, |u| format!("{} {u}", text::num(c.get(*a).unwrap_or(0.0) / s))).trim());
            }
        }
        out.push(l);
    }
    if rows.is_none() {
        // A family without rows of its own: what its own effects use, then display-named values.
        let mut extra = c.driver_lines(3);
        if extra.is_empty() {
            extra = generic_lines(c, 3).into_iter().filter(|l| !out.iter().any(|o| o.label == l.label)).collect();
        }
        out.extend(extra);
    }
    if let Some(d) = drawback(c) {
        out.push(d);
    }
    if let Some(r) = c.nz(RELOAD).filter(|ms| *ms >= 1000.0 && c.base(CHARGE_GROUP_1).is_some()) {
        out.push(Line { label: tr!("Reload").to_owned(), value: text::secs(r / 1000.0), tone: Tone::Plain, weak: true });
    }
    let mut limits = Vec::new();
    if c.get(MAX_GROUP_FITTED).is_some_and(|v| (v - 1.0).abs() < 1e-9) {
        limits.push(tr!("1 per ship").to_owned());
    }
    if let Some(ms) = c.nz(REACTIVATION) {
        limits.push(trf!("reactivates after {t}", t = text::secs(ms / 1000.0)));
    }
    if let (Some(q), Some(ty)) = (c.nz(FUEL_QTY), c.nz(FUEL_TYPE)) {
        limits.push(format!("{} × {}", text::num(q), c.dogma.type_(ty as u32).map_or_else(String::new, |t| t.name.clone())));
    }
    if !limits.is_empty() {
        out.push(Line { label: String::new(), value: limits.join(" · "), tone: Tone::Plain, weak: true });
    }
    out
}

/// Resist bonuses as one line in game order, or "All x%" when they match; resonances per layer.
fn resist_lines(c: &Ctx) -> Vec<Line> {
    let mut out = Vec::new();
    let bonus: Vec<f64> = RESIST_BONUS.iter().map(|a| -c.get(*a).unwrap_or(0.0)).collect();
    if bonus.iter().any(|v| *v > 1e-9) {
        let tone = RESIST_BONUS.iter().map(|a| c.tone(*a)).find(|t| *t != Tone::Plain).unwrap_or(Tone::Plain);
        out.push(Line { label: tr!("Resists").to_owned(), value: resist_text(&bonus), tone, weak: false });
    }
    for (layer, ids) in RESONANCES {
        let res: Vec<f64> = ids.iter().map(|a| c.get(*a).map_or(0.0, |v| (1.0 - v) * 100.0)).collect();
        if res.iter().any(|v| *v > 1e-6) {
            let label = match layer {
                "Armor" => tr!("Armor resists"),
                "Shield" => tr!("Shield resists"),
                _ => tr!("Hull resists"),
            };
            out.push(Line { label: label.to_owned(), value: resist_text(&res), tone: Tone::Plain, weak: false });
        }
    }
    out
}

fn resist_text(v: &[f64]) -> String {
    let pct = |x: f64| if (x - x.round()).abs() < 0.05 { format!("{x:.0}%") } else { format!("{x:.1}%") };
    if v.iter().all(|x| (x - v[0]).abs() < 1e-6) {
        return trf!("all {v}", v = pct(v[0]));
    }
    let names = [tr!("EM"), tr!("Th"), tr!("Kin"), tr!("Exp")];
    v.iter().zip(names).filter(|(x, _)| **x > 1e-6).map(|(x, n)| format!("{n} {}", pct(*x))).collect::<Vec<_>>().join(" ")
}

/// A rig's drawback, named after what it penalises.
fn drawback(c: &Ctx) -> Option<Line> {
    let v = c.nz(DRAWBACK)?;
    let hit = c.t.effects.iter().filter_map(|(e, _)| c.dogma.effect(*e)).flat_map(|e| e.modifiers.iter()).find(|m| m.modifying == DRAWBACK).map(|m| label_of(c.dogma, m.modified));
    Some(Line { label: trf!("Drawback: {what}", what = hit.unwrap_or_default().to_lowercase()), value: format!("{}%", signed(v)), tone: c.tone(DRAWBACK), weak: false })
}

fn missile(c: &Ctx) -> Vec<Line> {
    let mut out = Vec::new();
    if let Some((_, s)) = c.damage(DAMAGE, 1.0) {
        out.push(Line { label: tr!("Damage").to_owned(), value: s, tone: c.tone(117).max_by(c.tone(114)), weak: false });
    }
    for a in [97, 238, 5736, 5737, 5735, 99] {
        if let Some(l) = c.line(a) {
            out.push(l);
        }
    }
    out.extend(c.flight());
    for a in [654, 653, 2700, 265] {
        if a == 265 && c.t.group != 90 && c.t.group != 863 && c.t.group != 864 {
            continue;
        }
        if let Some(l) = c.line(a) {
            out.push(l);
        }
    }
    out
}

trait Pick {
    fn max_by(self, other: Tone) -> Tone;
}

impl Pick for Tone {
    fn max_by(self, other: Tone) -> Tone {
        if self == Tone::Plain { other } else { self }
    }
}

fn burst_charge(c: &Ctx) -> Vec<Line> {
    [(2468, 2596), (2470, 2597), (2472, 2598), (2536, 2599)]
        .iter()
        .filter_map(|(id, mult)| {
            let buff = c.nz(*id)? as u16;
            let name = c.dogma.buff(buff).map(|b| b.name.clone())?;
            let v = c.get(*mult).unwrap_or(0.0);
            Some(Line { label: name, value: crate::details::buff_value(c.dogma, buff, v), tone: Tone::Plain, weak: false })
        })
        .collect()
}

fn drone(c: &Ctx) -> Vec<Line> {
    let mut out = Vec::new();
    let mult = c.get(DAMAGE_MULT).unwrap_or(1.0);
    if let Some((shot, s)) = c.damage(DAMAGE, mult) {
        out.push(Line { label: tr!("Damage").to_owned(), value: s, tone: c.tone(DAMAGE_MULT), weak: false });
        if let Some(rof) = c.nz(SPEED) {
            out.push(Line { label: tr!("DPS").to_owned(), value: format!("{} · {}", text::num(shot / (rof / 1000.0)), trf!("every {t}", t = text::secs(rof / 1000.0))), tone: c.tone(SPEED), weak: false });
        }
    }
    // What a utility drone does, each with its rate where it has one.
    for a in [238, 554, 309, 566, 351, 767, 97, 20, 84, 68, 83, 77, 902] {
        if out.len() >= 3 {
            break;
        }
        if let Some(mut l) = c.line(a) {
            if let (true, Some(s)) = (PER_CYCLE.contains(&a), c.nz(942).or_else(|| c.nz(DURATION)).map(|ms| ms / 1000.0)) {
                l.value = format!("{} · {}/s", l.value, text::num(c.get(a).unwrap_or(0.0) / s));
            }
            out.push(l);
        }
    }
    if let Some(r) = c.nz(MAX_RANGE).or_else(|| c.nz(936)).or_else(|| c.nz(98)) {
        let value = match c.nz(FALLOFF) {
            Some(f) => format!("{} + {}", text::metres(r), text::metres(f)),
            None => text::metres(r),
        };
        out.push(Line { label: tr!("Range").to_owned(), value, tone: c.tone(MAX_RANGE), weak: false });
    }
    if c.damage(DAMAGE, 1.0).is_some() {
        out.extend(c.line(160));
    }
    if let Some(v) = c.nz(37) {
        let orbit = c.nz(508).map(|o| format!(" · {}", trf!("orbit {v}", v = text::num(o)))).unwrap_or_default();
        out.push(Line { label: tr!("Speed").to_owned(), value: format!("{} m/s{orbit}", text::num(v)), tone: c.tone(37), weak: false });
    }
    if let Some(bw) = c.nz(1272) {
        out.push(Line { label: tr!("Bandwidth · volume").to_owned(), value: format!("{} Mbit/s · {} m³", text::num(bw), text::num(c.get(161).unwrap_or(0.0))), tone: Tone::Plain, weak: true });
    }
    out
}

fn fighter(c: &Ctx) -> Vec<Line> {
    let mut out = Vec::new();
    let size = c.nz(2215).unwrap_or(1.0);
    if let Some((volley, s)) = c.damage([2227, 2228, 2229, 2230], c.get(2226).unwrap_or(1.0) * size) {
        let cycle = c.nz(2233).map(|ms| ms / 1000.0);
        let value = match cycle {
            Some(t) => format!("{s} · {} DPS", text::num(volley / t)),
            None => s,
        };
        out.push(Line { label: tr!("Attack").to_owned(), value, tone: c.tone(2226), weak: false });
        if let Some(r) = c.nz(2236) {
            out.push(Line { label: tr!("Attack range").to_owned(), value: format!("{} + {}", text::metres(r), text::metres(c.get(2237).unwrap_or(0.0))), tone: Tone::Plain, weak: false });
        }
    }
    if let Some((volley, s)) = c.damage([2131, 2132, 2133, 2134], c.get(2130).unwrap_or(1.0) * size) {
        let value = match c.nz(2182) {
            Some(ms) => format!("{s} / {} · {}", text::secs(ms / 1000.0), text::metres(c.get(2149).unwrap_or(0.0))),
            None => s,
        };
        let _ = volley;
        out.push(Line { label: tr!("Missiles").to_owned(), value, tone: Tone::Plain, weak: false });
    }
    for a in [2211, 2184, 2205, 2246] {
        if let Some(l) = c.line(a) {
            out.push(l);
        }
    }
    if let Some(v) = c.nz(37) {
        let mwd = c.nz(2152).or_else(|| c.nz(2151)).map(|b| format!(" · {}", trf!("boosted {v} m/s", v = text::num(v * (1.0 + b / 100.0))))).unwrap_or_default();
        out.push(Line { label: tr!("Speed").to_owned(), value: format!("{} m/s{mwd}", text::num(v)), tone: c.tone(37), weak: false });
    }
    if let Some(hp) = c.nz(263) {
        out.push(Line { label: tr!("Shield").to_owned(), value: trf!("{hp} HP each, {all} a squadron", hp = text::num(hp), all = text::num(hp * size)), tone: c.tone(263), weak: true });
    }
    out
}

/// Learning implants and accelerators: their attribute points, one line when all five match.
fn attribute_bonuses(c: &Ctx) -> Vec<Line> {
    let names = [tr!("Charisma"), tr!("Intelligence"), tr!("Memory"), tr!("Perception"), tr!("Willpower")];
    let v: Vec<f64> = (175..=179).map(|a| c.get(a).unwrap_or(0.0)).collect();
    if v.iter().all(|x| *x > 0.0 && (x - v[0]).abs() < 1e-9) {
        return vec![Line { label: tr!("All attributes").to_owned(), value: format!("+{}", whole(v[0])), tone: Tone::Plain, weak: false }];
    }
    v.iter().zip(names).filter(|(x, _)| **x > 0.0).map(|(x, n)| Line { label: n.to_owned(), value: format!("+{}", whole(*x)), tone: Tone::Plain, weak: false }).collect()
}

fn booster(c: &Ctx) -> Vec<Line> {
    let mut out = attribute_bonuses(c);
    out.extend(c.driver_lines(3));
    if let Some(ms) = c.nz(330) {
        out.push(Line { label: tr!("Duration").to_owned(), value: duration_text(ms / 1000.0), tone: Tone::Plain, weak: false });
    }
    // Side effects: each chance, with what it would cost.
    let mut sides: Vec<(f64, String)> = Vec::new();
    for (e, _) in &c.t.effects {
        let Some(def) = c.dogma.effect(*e) else { continue };
        let Some(chance_attr) = def.usage_chance_attr else { continue };
        let chance = c.get(chance_attr).unwrap_or(0.0);
        for m in &def.modifiers {
            if let Some(v) = c.nz(m.modifying) {
                sides.push((chance, format!("{} {}%", label_of(c.dogma, m.modified).to_lowercase(), signed(v))));
                break;
            }
        }
    }
    if !sides.is_empty() {
        let same = sides.iter().all(|(ch, _)| (ch - sides[0].0).abs() < 1e-9);
        let value = if same {
            sides.iter().map(|(_, s)| s.clone()).collect::<Vec<_>>().join(", ")
        } else {
            sides.iter().map(|(ch, s)| format!("{s} ({:.0}%)", ch * 100.0)).collect::<Vec<_>>().join(", ")
        };
        let label = if same { trf!("Side effects, {p}% each", p = format!("{:.0}", sides[0].0 * 100.0)) } else { tr!("Side effects").to_owned() };
        out.push(Line { label, value, tone: Tone::Plain, weak: true });
    }
    out
}

fn duration_text(s: f64) -> String {
    if s >= 86_400.0 {
        format!("{:.0} d", s / 86_400.0)
    } else if s >= 3600.0 {
        format!("{} h", text::num(s / 3600.0))
    } else if s >= 60.0 {
        format!("{:.0} min", s / 60.0)
    } else {
        text::secs(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spai_fit::test_support::{dataset, ids};

    fn find<'a>(lines: &'a [Line], label: &str) -> Option<&'a Line> {
        lines.iter().find(|l| l.label == label)
    }

    #[test]
    fn a_turret_shows_its_fitting_its_cycle_and_damage_modifier_and_no_noise() {
        let d = dataset();
        let t = d.type_(ids::GUN).unwrap();
        let l = lines(&d, t, None);
        assert!(find(&l, "CPU · Powergrid").is_some(), "{l:?}");
        assert!(find(&l, "Cycle").is_some() || find(&l, "Activation").is_some(), "{l:?}");
        assert!(!l.iter().any(|x| x.label.contains("Mass") || x.label.contains("Volume") || x.label.contains("Capacity")), "{l:?}");
        assert!(l.len() <= 9);
    }

    #[test]
    fn a_fitted_value_is_used_and_marked() {
        let d = dataset();
        let t = d.type_(ids::GUN).unwrap();
        let mut f = HashMap::new();
        f.insert(SPEED, f64::from(t.attr(SPEED).unwrap()) * 0.5);
        let l = lines(&d, t, Some(&f));
        let cycle = l.iter().find(|x| x.label == "Cycle" || x.label == "Activation").unwrap();
        assert_ne!(cycle.tone, Tone::Plain, "a changed value is marked");
    }

    #[test]
    fn ammo_and_drones_lead_with_their_damage() {
        let d = dataset();
        let ammo = lines(&d, d.type_(ids::AMMO).unwrap(), None);
        assert_eq!(ammo[0].label, "Damage", "{ammo:?}");
        assert!(ammo[0].value.contains("HP"));
        let drone = lines(&d, d.type_(ids::DRONE).unwrap(), None);
        assert_eq!(drone[0].label, "Damage", "{drone:?}");
        assert!(find(&drone, "DPS").is_some_and(|x| x.value.contains("every")), "{drone:?}");
        let missile = lines(&d, d.type_(ids::MISSILE).unwrap(), None);
        assert!(find(&missile, "Range").is_some_and(|x| x.value.contains("m/s")), "speed × flight time: {missile:?}");
    }

    #[test]
    fn resists_read_as_one_line() {
        assert_eq!(resist_text(&[25.0, 25.0, 25.0, 25.0]), "all 25%");
        assert_eq!(resist_text(&[0.0, 32.5, 0.0, 0.0]), "Th 32.5%");
        assert_eq!(signed(-12.0), "\u{2212}12");
        assert_eq!(short_label("Modification of Optimal Range Bonus"), "Optimal Range");
    }

    /// The tooltips of a spread of real items, printed to read. `SPAI_DOGMA_BLOB=<path> cargo test
    /// -p spai-fit-ui real_item_tips -- --ignored --nocapture`
    #[test]
    #[ignore = "needs SPAI_DOGMA_BLOB"]
    fn real_item_tips() {
        let path = std::env::var("SPAI_DOGMA_BLOB").expect("SPAI_DOGMA_BLOB");
        let d = spai_fit::dogma::blob::decode(&std::fs::read(path).unwrap()).unwrap();
        for name in [
            "425mm Railgun II", "Heavy Missile Launcher II", "10MN Afterburner II", "50MN Microwarpdrive II", "Medium Armor Repairer II", "Large Shield Extender II",
            "Multispectrum Energized Membrane II", "Reactive Armor Hardener", "Damage Control II", "Gyrostabilizer II", "Ballistic Control System II", "Tracking Computer II",
            "Stasis Webifier II", "Warp Scrambler II", "Heavy Energy Neutralizer II", "Large Remote Armor Repairer II", "Covert Ops Cloaking Device II", "Shield Command Burst II",
            "Cap Recharger II", "Medium Core Defense Field Extender I", "Medium Trimark Armor Pump I", "Large Micro Jump Drive", "Inertial Stabilizers II",
            "Antimatter Charge M", "Scourge Heavy Missile", "Hammerhead II", "Templar I", "Cap Booster 800", "Optimal Range Script", "High-grade Snake Alpha",
            "Strong Blue Pill Booster", "Shield Harmonizing Charge", "'Noble' RP-905", "Ocular Filter - Basic", "Warp Disrupt Probe", "Medium Armor Maintenance Bot II",
        ] {
            let Some(t) = d.type_named(name) else {
                println!("-- {name}: not found");
                continue;
            };
            println!("-- {} [{}]", t.name, subtitle(&d, t).unwrap_or_default());
            for l in lines(&d, t, None) {
                println!("   {:<34} {}{}", l.label, l.value, if l.weak { "  (weak)" } else { "" });
            }
        }
    }
}
