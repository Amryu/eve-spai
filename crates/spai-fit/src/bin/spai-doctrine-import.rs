//! Builds supplied doctrines out of forum topics saved from the browser, for an admin to publish.
//!
//! ```text
//! spai-doctrine-import --blob dogma.bin.gz [--curation curation.json] --out doctrines.json page.html...
//! ```
//!
//! Each topic becomes one doctrine of its opening poster's fits. The curation file, kept with the
//! data and out of the repository, names what a title cannot say: the mainline hull where the topic
//! has no "ship of the line", hulls to leave out, refits of a spare module. The report on stderr
//! lists what needs a look: items the data does not know, fits that do not fit, spares left in the
//! cargo.

use std::collections::BTreeMap;

use serde::Deserialize;
use spai_fit::doctrine::{forum, supplied_fit_id, Doctrine, DoctrineFit, FitRole};
use spai_fit::fit::Fit;
use spai_fit::warnings::{Severity, WarningKind};
use spai_fit::Dogma;

#[derive(Default, Deserialize)]
#[serde(default)]
struct Curation {
    alliance: String,
    id_prefix: String,
    /// Roles by hull for every page, where the hull's group says otherwise.
    roles: BTreeMap<String, FitRole>,
    pages: BTreeMap<String, PageCuration>,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct PageCuration {
    skip: bool,
    id: Option<String>,
    name: Option<String>,
    category: Option<String>,
    tags: Option<Vec<String>>,
    /// Hulls (or fit names) that are the mainline damage.
    main: Vec<String>,
    /// Hulls (or fit names) to leave out.
    drop: Vec<String>,
    /// Keep interdictors and entosis fits, which otherwise live in their own doctrines.
    keep_utility: bool,
    roles: BTreeMap<String, FitRole>,
    refits: Vec<Refit>,
    /// Fits that are another fit refitted: kept as a variation of it instead.
    fold: Vec<Fold>,
    notes: Option<String>,
}

#[derive(Deserialize)]
struct Fold {
    /// `Hull: fit name` of the folded fit, and of the one it folds into.
    fit: String,
    into: String,
    name: String,
    key: Option<String>,
}

#[derive(Deserialize)]
struct Refit {
    /// The hull or a part of the fit's name.
    fit: String,
    name: String,
    key: Option<String>,
    replace: String,
    with: String,
    #[serde(default)]
    all: bool,
}

const INTERDICTOR_GROUP: spai_fit::ids::GroupId = 541;

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let (mut blob, mut curation, mut out, mut pages) = (None, None, None, Vec::new());
    while let Some(a) = args.next() {
        match a.as_str() {
            "--blob" => blob = args.next(),
            "--curation" => curation = args.next(),
            "--out" => out = args.next(),
            _ => pages.push(a),
        }
    }
    let blob = blob.ok_or_else(|| anyhow::anyhow!("--blob <dogma.bin.gz> is required"))?;
    let dogma = spai_fit::dogma::blob::decode(&std::fs::read(blob)?)?;
    let cur: Curation = match curation {
        Some(p) => serde_json::from_str(&std::fs::read_to_string(p)?)?,
        None => Curation::default(),
    };
    let alliance = if cur.alliance.is_empty() { "Goonswarm Federation".to_owned() } else { cur.alliance.clone() };
    let prefix = if cur.id_prefix.is_empty() { "gsf".to_owned() } else { cur.id_prefix.clone() };
    let mut doctrines = Vec::new();
    for path in pages {
        let html = std::fs::read_to_string(&path)?;
        let page = forum::page(&html);
        let topic = page.url.as_deref().and_then(topic_number).unwrap_or_default();
        let pc = cur.pages.get(&topic);
        if pc.is_some_and(|p| p.skip) {
            eprintln!("skip {}", page.title);
            continue;
        }
        let empty = PageCuration::default();
        let pc = pc.unwrap_or(&empty);
        match build(&dogma, &page, pc, &cur.roles, &alliance, &prefix, &topic) {
            Some(d) => doctrines.push(d),
            None => eprintln!("\n## {}\n   no fits found ({path})", page.title),
        }
    }
    doctrines.sort_by(|a, b| (spai_fit::doctrine::category_rank(&a.category), &a.name).cmp(&(spai_fit::doctrine::category_rank(&b.category), &b.name)));
    let json = serde_json::to_string_pretty(&doctrines)?;
    match out {
        Some(p) => std::fs::write(p, json)?,
        None => println!("{json}"),
    }
    Ok(())
}

fn topic_number(url: &str) -> Option<String> {
    let t = url.split("/topic/").nth(1)?;
    Some(t.chars().take_while(char::is_ascii_digit).collect())
}

/// Status, kind and name out of `ACTIVE - STRAT - HammerFleet (FNI)`.
fn title_parts(title: &str) -> (bool, &'static str, Vec<String>, String) {
    let mut parts: Vec<&str> = title.split(" - ").map(str::trim).collect();
    let mut active = true;
    let mut category = "Capitals";
    let mut tags: Vec<String> = Vec::new();
    while parts.len() > 1 {
        let p = parts[0].to_ascii_uppercase();
        let (cat, tag) = match p.as_str() {
            "ACTIVE" => {
                tags.push("Active".into());
                parts.remove(0);
                continue;
            }
            "RETIRED" => {
                active = false;
                tags.push("Retired".into());
                parts.remove(0);
                continue;
            }
            "STRAT" => ("Strategic", "Strategic"),
            "PEACETIME" => ("Peacetime", "Peacetime"),
            "SUGGESTED" => ("Utility", "Suggested"),
            "CYNO" => ("SIGs", "Cyno"),
            "BEEHIVE" => ("SIGs", "Beehive"),
            "CAPITALS" => ("Capitals", "Capitals"),
            _ => break,
        };
        category = cat;
        tags.push(tag.into());
        parts.remove(0);
    }
    if !active {
        category = "Retired";
    }
    (active, category, tags, parts.join(" - "))
}

fn hull_name(dogma: &Dogma, fit: &Fit) -> String {
    dogma.type_(fit.ship).map_or_else(String::new, |t| t.name.clone())
}

/// A hull's name matches the hull alone, anything else a part of the fit's name.
fn names(dogma: &Dogma, fit: &Fit, wanted: &str) -> bool {
    if dogma.type_named(wanted).is_some_and(|t| spai_fit::formats::place_of(dogma, t) == spai_fit::formats::Place::Ship) {
        return hull_name(dogma, fit).eq_ignore_ascii_case(wanted);
    }
    fit.name.to_lowercase().contains(&wanted.to_lowercase())
}

fn slug(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    out.trim_end_matches('-').to_owned()
}

fn build(dogma: &Dogma, page: &forum::Page, pc: &PageCuration, roles: &BTreeMap<String, FitRole>, alliance: &str, prefix: &str, topic: &str) -> Option<Doctrine> {
    let op = page.posts.first()?.author.clone();
    let (_, category, mut tags, name) = title_parts(&page.title);
    let utility_page = category == "Utility" || name.to_lowercase().contains("entosis");
    let mut found: Vec<forum::Found> = Vec::new();
    let mut updated: Option<i64> = None;
    // Lists in the posts before the first fit are what every fit of the page carries.
    let mut common: Vec<forum::Loose> = Vec::new();
    for post in page.posts.iter().filter(|p| p.author == op) {
        let (fits, orphans) = forum::read(dogma, &post.id, &post.lines);
        if found.is_empty() && fits.is_empty() {
            common.extend(orphans);
        } else {
            common.extend(orphans.into_iter().filter(|l| l.heading == forum::FUEL));
        }
        if !fits.is_empty() {
            updated = updated.max(post.changed());
        }
        found.extend(fits);
    }
    for f in &mut found {
        for list in &common {
            let unknown = forum::add_items(dogma, &mut f.fit, list);
            f.unknown.extend(unknown);
        }
    }
    if found.is_empty() {
        return None;
    }
    let name = pc.name.clone().unwrap_or(name);
    let id = pc.id.clone().unwrap_or_else(|| format!("{prefix}-{topic}"));
    let category = pc.category.clone().unwrap_or_else(|| category.to_owned());
    if let Some(t) = &pc.tags {
        tags = t.clone();
    }
    if !tags.iter().any(|t| t == alliance) {
        tags.insert(0, alliance.to_owned());
    }
    eprintln!("\n## {name}  [{category}]  {}  updated {}", page.url.as_deref().unwrap_or("-"), updated.map_or("-".into(), date));
    let mut doctrine = Doctrine { id: id.clone(), alliance: alliance.to_owned(), category, name, tags, url: page.url.clone(), updated, notes: pc.notes.clone().unwrap_or_default(), ..Doctrine::default() };
    let mut seen: Vec<String> = Vec::new();
    let mut keys: Vec<String> = Vec::new();
    let mut line_heading_seen = false;
    for f in found {
        let mut fit = f.fit;
        let hull = hull_name(dogma, &fit);
        let eft = spai_fit::formats::eft::export(dogma, &fit);
        if seen.contains(&eft) {
            continue;
        }
        seen.push(eft);
        let heading_main = f.context.iter().any(|c| c.to_lowercase().contains("ship of the line") || c.to_lowercase().contains("ships of the line"));
        let group = dogma.type_(fit.ship).map(|t| t.group);
        let entosis = fit.modules.iter().any(|m| dogma.type_(m.type_id).is_some_and(|t| t.name.starts_with("Entosis Link")));
        if pc.drop.iter().any(|d| names(dogma, &fit, d)) {
            eprintln!("   - dropped  {hull}: {}", fit.name);
            continue;
        }
        let main = if pc.main.is_empty() { heading_main && !line_heading_seen } else { pc.main.iter().any(|m| names(dogma, &fit, m)) };
        if heading_main {
            line_heading_seen = true;
        }
        if !utility_page && !pc.keep_utility && !main && (group == Some(INTERDICTOR_GROUP) || entosis) {
            eprintln!("   - utility  {hull}: {} (lives in its own doctrine)", fit.name);
            continue;
        }
        forum::fuel_to_bay(dogma, &mut fit);
        forum::launch_drones(dogma, &mut fit);
        forum::launch_fighters(dogma, &mut fit);
        let made = forum::refits(dogma, &mut fit);
        let left = made.left;
        for name in made.unfit {
            eprintln!("   ! {} on {}: the refit would not fit", name, fit.name);
        }
        let refits: Vec<&Refit> = pc.refits.iter().filter(|r| names(dogma, &fit, &r.fit)).collect();
        for r in refits {
            let (Some(a), Some(b)) = (dogma.type_named(&r.replace), dogma.type_named(&r.with)) else {
                eprintln!("   ! refit {} names an unknown type", r.name);
                continue;
            };
            if !forum::module_refit(&mut fit, &r.name, r.key.as_deref(), a.id, b.id, r.all) {
                eprintln!("   ! refit {} found no {} on {}", r.name, r.replace, fit.name);
            }
        }
        let role = if main {
            FitRole::Line
        } else {
            pc.roles.iter().chain(roles.iter()).find(|(k, _)| names(dogma, &fit, k)).map(|(_, r)| *r).unwrap_or_else(|| FitRole::of_hull(dogma, fit.ship))
        };
        let note = f.context.iter().rev().find(|c| c.to_lowercase().starts_with(&hull.to_lowercase()) && c.contains(" - ")).and_then(|c| c.split_once(" - ").map(|(_, n)| n.trim().to_owned())).unwrap_or_default();
        let mut key = slug(&hull);
        let mut n = 2;
        while keys.contains(&key) {
            key = format!("{}-{n}", slug(&hull));
            n += 1;
        }
        keys.push(key.clone());
        fit.id = supplied_fit_id(&id, &key);
        let said: Vec<&str> = [prefix, alliance, doctrine.name.as_str(), page.title.as_str()].into_iter().chain(doctrine.tags.iter().map(String::as_str)).collect();
        let forum_name = std::mem::take(&mut fit.name);
        fit.name = spai_fit::doctrine::short_fit_name(&forum_name, &hull, &said);
        fit.notes = format!("Forum name: {forum_name}");
        report(dogma, &fit, &f.unknown, &left, main, role, &note);
        doctrine.fits.push(DoctrineFit { fit_id: fit.id.clone(), fit: Some(fit), role, main, note });
    }
    for f in &pc.fold {
        let pick = |d: &Doctrine, which: &str| d.fits.iter().position(|x| x.fit.as_ref().is_some_and(|fit| format!("{}: {}", hull_name(dogma, fit), fit.notes.trim_start_matches("Forum name: ")).eq_ignore_ascii_case(which.trim())));
        let (Some(from), Some(into)) = (pick(&doctrine, &f.fit), pick(&doctrine, &f.into)) else {
            eprintln!("   ! fold {} into {}: not both found", f.fit, f.into);
            continue;
        };
        let folded = doctrine.fits.remove(from);
        let into = if from < into { into - 1 } else { into };
        let (Some(src), Some(dst)) = (folded.fit, doctrine.fits[into].fit.as_mut()) else { continue };
        if src.ship != dst.ship {
            eprintln!("   ! fold {}: another hull", f.fit);
            continue;
        }
        let ops = spai_fit::variations::diff(dst, &src);
        let id = dst.variations.iter().map(|v| v.id).max().unwrap_or(0) + 1;
        dst.variations.push(spai_fit::variations::Variation { id, name: f.name.clone(), kind: spai_fit::variations::VariationKind::Refit, refit_key: f.key.clone(), ops, group: None });
        eprintln!("   ~ folded {} into {} as {}", src.name, dst.name, f.name);
    }
    Some(doctrine)
}

fn date(t: i64) -> String {
    let days = t.div_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y}-{m:02}-{d:02}")
}

#[allow(clippy::too_many_arguments)]
fn report(dogma: &Dogma, fit: &Fit, unknown: &[String], left: &[u32], main: bool, role: FitRole, note: &str) {
    let hull = hull_name(dogma, fit);
    eprintln!("   {} {hull}: {}  [{role:?}]{}", if main { "*" } else { " " }, fit.name, if note.is_empty() { String::new() } else { format!("  ({note})") });
    if !unknown.is_empty() {
        eprintln!("       UNKNOWN: {}", unknown.join(", "));
    }
    let skills = spai_fit::skills::SkillSet::all_v();
    let computed = spai_fit::stats::compute(dogma, fit, &skills, &Default::default());
    for w in spai_fit::warnings::check(dogma, fit, &computed, &[]) {
        let shown = match &w.kind {
            WarningKind::CpuOver { .. } | WarningKind::PowerOver { .. } | WarningKind::CalibrationOver { .. } | WarningKind::SlotsOver { .. } | WarningKind::TurretsOver { .. } | WarningKind::LaunchersOver { .. } | WarningKind::DroneBandwidthOver { .. } | WarningKind::DroneBayOver { .. } | WarningKind::GroupLimit { .. } | WarningKind::WrongCharge { .. } | WarningKind::UnknownTypes(_) => true,
            _ => w.severity == Severity::Error,
        };
        if shown {
            eprintln!("       FIT: {:?}", w.kind);
        }
    }
    for v in &fit.variations {
        eprintln!("       refit: {} ({})", v.name, v.refit_key.as_deref().unwrap_or("-"));
    }
    if !left.is_empty() {
        let names: Vec<String> = left.iter().map(|t| spai_fit::formats::name_of(dogma, *t)).collect();
        eprintln!("       cargo modules: {}", names.join(", "));
    }
}
