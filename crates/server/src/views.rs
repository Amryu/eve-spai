//! Built with [`maud`] - compile-time templates that auto-escape every interpolation.
//! Every user- or EVE-supplied string (titles, uploader names, side/alliance/pilot
//! names) is rendered through a normal `(value)` interpolation, never via
//! [`PreEscaped`], so none of them can inject markup. `PreEscaped` is used *only* for
//! the static, developer-authored CSS/JS blobs in [`layout`].

use br_core::battle::{Affil, Battle, BattleReportDoc, Party, PartyKind, Side};
use maud::{html, Markup, PreEscaped, DOCTYPE};

/// A folded-in pod loss is only shown when its value exceeds this floor; `br_core` stores
/// `10000` as the default/empty pod value (no real capsule kill), so anything at or below
/// it is not a pod worth surfacing.
const POD_VALUE_MIN: f64 = 10_000.0;

fn icon_url(type_id: i64) -> String {
    format!("https://images.evetech.net/types/{type_id}/icon?size=64")
}

fn zkill_url(kill_id: i64) -> String {
    format!("https://zkillboard.com/kill/{kill_id}/")
}

fn party_logo_url(p: &Party) -> Option<String> {
    if p.id == 0 {
        return None;
    }
    match p.kind {
        PartyKind::Alliance => {
            Some(format!("https://images.evetech.net/alliances/{}/logo?size=32", p.id))
        }
        PartyKind::Corporation => {
            Some(format!("https://images.evetech.net/corporations/{}/logo?size=32", p.id))
        }
        _ => None,
    }
}

fn party_zkill_url(p: &Party) -> Option<String> {
    if p.id == 0 {
        return None;
    }
    match p.kind {
        PartyKind::Alliance => Some(format!("https://zkillboard.com/alliance/{}/", p.id)),
        PartyKind::Corporation => Some(format!("https://zkillboard.com/corporation/{}/", p.id)),
        _ => None,
    }
}

fn affil_icons(affil: Option<&Affil>) -> Markup {
    html! {
        @if let Some(a) = affil {
            @if a.corp_id != 0 {
                a .affil-logo href=(format!("https://zkillboard.com/corporation/{}/", a.corp_id))
                    target="_blank" rel="noopener" title=(a.corp_name) {
                    img src=(format!("https://images.evetech.net/corporations/{}/logo?size=32", a.corp_id))
                        width="18" height="18" loading="lazy" alt=(a.corp_name);
                }
            }
            @if a.alliance_id != 0 {
                a .affil-logo href=(format!("https://zkillboard.com/alliance/{}/", a.alliance_id))
                    target="_blank" rel="noopener" title=(a.alliance_name) {
                    img src=(format!("https://images.evetech.net/alliances/{}/logo?size=32", a.alliance_id))
                        width="18" height="18" loading="lazy" alt=(a.alliance_name);
                }
            }
        }
    }
}

fn party_logo(p: &Party) -> Markup {
    html! {
        @if let Some(url) = party_logo_url(p) {
            img .party-logo src=(url) width="20" height="20" loading="lazy" alt=(p.name) title=(p.name);
        }
    }
}

fn side_breakdown(battle: &Battle, side_idx: usize) -> (usize, Vec<(Party, usize, f64)>) {
    use std::collections::HashMap;
    let roster = battle.roster(side_idx);
    let total = roster.len();
    if total == 0 {
        return (0, Vec::new());
    }
    let mut counts: HashMap<i64, (Party, usize)> = HashMap::new();
    for p in &roster {
        if matches!(p.party.kind, PartyKind::Alliance | PartyKind::Corporation) && p.party.id != 0 {
            let e = counts.entry(p.party.id).or_insert_with(|| (p.party.clone(), 0));
            e.1 += 1;
        }
    }
    let mut v: Vec<(Party, usize, f64)> =
        counts.into_values().map(|(p, c)| (p, c, c as f64 / total as f64)).collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.id.cmp(&b.0.id)));
    (total, v)
}

fn dominant_parties(battle: &Battle, side_idx: usize) -> Vec<(Party, f64)> {
    let (_, entries) = side_breakdown(battle, side_idx);
    entries.into_iter().filter(|(_, _, r)| *r > 0.10).map(|(p, _, r)| (p, r)).take(3).collect()
}

pub struct CardData {
    pub id: String,
    pub doc: BattleReportDoc,
    pub uploader: String,
    pub views: i64,
}

#[derive(Default, Clone)]
pub struct DirQuery {
    pub system: String,
    pub from: String,
    pub to: String,
    pub participant: String,
    pub min_isk: String,
    pub sort: String,
}

impl DirQuery {
    fn link(&self, page: i64) -> String {
        let mut q: Vec<(&str, &str)> = Vec::new();
        if !self.system.is_empty() {
            q.push(("system", &self.system));
        }
        if !self.from.is_empty() {
            q.push(("from", &self.from));
        }
        if !self.to.is_empty() {
            q.push(("to", &self.to));
        }
        if !self.participant.is_empty() {
            q.push(("participant", &self.participant));
        }
        if !self.min_isk.is_empty() {
            q.push(("min_isk", &self.min_isk));
        }
        if !self.sort.is_empty() {
            q.push(("sort", &self.sort));
        }
        let page = page.to_string();
        q.push(("page", &page));
        let qs: Vec<String> = q.iter().map(|(k, v)| format!("{k}={}", enc(v))).collect();
        format!("/br?{}", qs.join("&"))
    }
}

fn enc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn sec_color(sec: f64) -> &'static str {
    if sec >= 0.5 {
        "#4fc3a1"
    } else if sec > 0.0 {
        "#e8a13a"
    } else {
        "#e04c4c"
    }
}

fn fmt_isk(v: f64) -> String {
    let a = v.abs();
    if a >= 1e12 {
        format!("{:.2}T", v / 1e12)
    } else if a >= 1e9 {
        format!("{:.2}B", v / 1e9)
    } else if a >= 1e6 {
        format!("{:.1}M", v / 1e6)
    } else if a >= 1e3 {
        format!("{:.0}k", v / 1e3)
    } else {
        format!("{v:.0}")
    }
}

fn fmt_duration(secs: i64) -> String {
    if secs < 60 {
        return "<1m".to_string();
    }
    let mins = secs / 60;
    if mins < 60 {
        format!("{mins}m")
    } else {
        format!("{}h {}m", mins / 60, mins % 60)
    }
}

fn fmt_time(unix: i64) -> String {
    chrono::DateTime::from_timestamp(unix, 0)
        .map(|t| t.format("%Y-%m-%d %H:%M UTC").to_string())
        .unwrap_or_else(|| "-".to_string())
}

fn fmt_eff(side: &Side) -> String {
    side.isk_efficiency().map(|e| format!("{e:.0}%")).unwrap_or_else(|| "-".to_string())
}

/// A side's display name: its coalition, else its most-involved party, else `Unknown`.
/// Mirrors the `side_names` extraction in `pipeline::extract_columns`.
fn side_label(side: &Side) -> String {
    side.coalition
        .clone()
        .or_else(|| side.parties.first().map(|p| p.name.clone()))
        .unwrap_or_else(|| "Unknown".to_string())
}

fn display_title(doc: &BattleReportDoc) -> String {
    if let Some(t) = doc.title.as_deref().filter(|t| !t.trim().is_empty()) {
        return t.to_string();
    }
    let system = doc.battle.systems.first().map(|(_, n, _)| n.as_str()).unwrap_or("Battle");
    let date = chrono::DateTime::from_timestamp(doc.battle.start, 0)
        .map(|t| t.format("%Y-%m-%d").to_string())
        .unwrap_or_default();
    if date.is_empty() {
        system.to_string()
    } else {
        format!("{system} - {date}")
    }
}

fn systems_chips(battle: &Battle) -> Markup {
    html! {
        span .systems {
            @for (i, (_, name, sec)) in battle.systems.iter().enumerate() {
                @if i > 0 { span .sep { ", " } }
                span .sys {
                    span .sec style=(format!("color:{}", sec_color(*sec))) { (format!("{sec:.1}")) }
                    " "
                    (name)
                }
            }
        }
    }
}

fn side_bars(battle: &Battle) -> Markup {
    html! {
        div .sides {
            @for side in battle.sides.iter().take(2) {
                @let eff = side.isk_efficiency().unwrap_or(0.0);
                div .side-row {
                    span .side-name { (side_label(side)) }
                    div .bar { div .bar-fill style=(format!("width:{eff:.0}%")) {} }
                    span .side-eff { (fmt_eff(side)) }
                }
            }
        }
    }
}

pub fn card(data: &CardData) -> Markup {
    let b = &data.doc.battle;
    let href = format!("/br/{}", data.id);
    html! {
        a .card href=(href) {
            div .card-head {
                span .card-title { (display_title(&data.doc)) }
            }
            (systems_chips(b))
            div .card-meta {
                span { (fmt_time(b.start)) }
                span .dot-sep { "·" }
                span { (fmt_duration(b.end - b.start)) }
                span .dot-sep { "·" }
                span { (b.kills) " kills" }
                span .dot-sep { "·" }
                span .isk { (fmt_isk(b.isk)) " ISK" }
            }
            (side_bars(b))
            div .card-foot {
                span { "by " (data.uploader) }
                span .views { (data.views) " views" }
            }
        }
    }
}

pub fn directory_page(cards: &[CardData], q: &DirQuery, page: i64, has_next: bool) -> Markup {
    let body = html! {
        section .toolbar {
            h2 { "Battle reports" }
            form .filters method="get" action="/br" {
                input type="text" name="system" placeholder="System" value=(q.system) list="system-list";
                input type="text" name="participant" placeholder="Alliance / pilot" value=(q.participant);
                input type="date" name="from" value=(q.from) title="From date";
                input type="date" name="to" value=(q.to) title="To date";
                input type="number" name="min_isk" placeholder="Min ISK" value=(q.min_isk) step="any" min="0";
                select name="sort" {
                    option value="newest" selected[q.sort.is_empty() || q.sort == "newest"] { "Newest" }
                    option value="oldest" selected[q.sort == "oldest"] { "Oldest" }
                    option value="isk" selected[q.sort == "isk"] { "Most ISK" }
                    option value="kills" selected[q.sort == "kills"] { "Most kills" }
                }
                datalist #system-list {
                    @for c in cards {
                        @for (_, name, _) in c.doc.battle.systems.iter() {
                            option value=(name) {}
                        }
                    }
                }
                button type="submit" .btn .primary { "Filter" }
                a .btn href="/br" { "Reset" }
            }
        }
        @if cards.is_empty() {
            div .empty { "No battle reports match these filters." }
        } @else {
            div .cards {
                @for c in cards { (card(c)) }
            }
        }
        nav .pager {
            @if page > 1 {
                a .btn href=(q.link(page - 1)) { "← Newer" }
            } @else {
                span .btn .disabled { "← Newer" }
            }
            span .page-num { "Page " (page) }
            @if has_next {
                a .btn href=(q.link(page + 1)) { "Older →" }
            } @else {
                span .btn .disabled { "Older →" }
            }
        }
    };
    layout("Battle reports - EVE Spai", body, Some(DIRECTORY_JS))
}

struct ShipGroup {
    ship: i64,
    lost: usize,
    survived: usize,
}

/// Group a side's roster by hull type (mirrors `Battle::roster(i)`, which already folds
/// pods and dedups survivors): lost vs survived counts. Heaviest-hit hulls first. Capsules
/// (`POD_TYPES`) are excluded so pods never appear as their own tile.
fn roster_groups(battle: &Battle, side_idx: usize) -> Vec<ShipGroup> {
    use br_core::battle::POD_TYPES;
    use std::collections::HashMap;
    let roster = battle.roster(side_idx);

    let mut by_ship: HashMap<i64, (usize, usize)> = HashMap::new();
    for p in &roster {
        if POD_TYPES.contains(&p.ship) {
            continue;
        }
        let entry = by_ship.entry(p.ship).or_insert((0, 0));
        if p.lost.is_some() {
            entry.0 += 1;
        } else {
            entry.1 += 1;
        }
    }
    let mut groups: Vec<ShipGroup> = by_ship
        .into_iter()
        .map(|(ship, (lost, survived))| ShipGroup { ship, lost, survived })
        .collect();
    groups.sort_by(|a, b| {
        b.lost
            .cmp(&a.lost)
            .then((b.lost + b.survived).cmp(&(a.lost + a.survived)))
            .then(a.ship.cmp(&b.ship))
    });
    groups
}

fn pod_kill_ids(battle: &Battle) -> std::collections::HashMap<i64, i64> {
    use br_core::battle::POD_TYPES;
    let mut m = std::collections::HashMap::new();
    for e in &battle.engagements {
        if e.victim_char != 0 && POD_TYPES.contains(&e.victim_ship) {
            m.insert(e.victim_char, e.kill_id);
        }
    }
    m
}

fn detail_cell(
    p: &br_core::battle::Participant,
    pod_kills: &std::collections::HashMap<i64, i64>,
    affiliations: &std::collections::BTreeMap<i64, Affil>,
) -> Markup {
    let lost = p.lost.as_ref();
    let pod = lost.filter(|l| l.pod_value > POD_VALUE_MIN);
    let pod_kill = pod.and_then(|_| (p.char_id != 0).then(|| pod_kills.get(&p.char_id).copied()).flatten());
    let affil = (p.char_id != 0).then(|| affiliations.get(&p.char_id)).flatten();
    html! {
        div .dcell .lost[lost.is_some()]
            data-char=(p.char_id)
            data-party=(p.party.id)
            data-ship=(p.ship)
            data-kill=[lost.map(|l| l.kill_id)]
        {
            img .dhull src=(icon_url(p.ship)) width="40" height="40" loading="lazy" alt="ship";
            div .dinfo {
                span .dpilot {
                    (affil_icons(affil))
                    @if p.char_id != 0 {
                        a .dpilot-name href=(format!("https://zkillboard.com/character/{}/", p.char_id))
                            target="_blank" rel="noopener" title=(p.pilot) { (p.pilot) }
                    } @else {
                        span .dpilot-name title=(p.pilot) { (p.pilot) }
                    }
                }
                span .dparty {
                    (party_logo(&p.party))
                    span .dparty-name title=(p.party.name) { (p.party.name) }
                }
            }
            @if let Some(l) = lost {
                div .dloss {
                    span .destroyed { "destroyed" }
                    span .dval { (fmt_isk(l.value)) }
                    a .zk href=(zkill_url(l.kill_id)) target="_blank" rel="noopener" { "zKill" }
                }
            }
        }
        @if let Some(l) = pod {
            div .dcell .lost .dpod-row data-party=(p.party.id) data-ship=(p.ship) {
                img .dhull .dpod-hull src=(icon_url(l.pod_ship)) width="32" height="32" loading="lazy" alt="pod";
                div .dinfo {
                    span .dpilot title=(p.pilot) { (p.pilot) }
                    span .dparty { span .dparty-name { "Capsule" } }
                }
                div .dloss {
                    span .destroyed { "pod" }
                    span .dval { (fmt_isk(l.pod_value)) }
                    @if let Some(k) = pod_kill {
                        a .zk href=(zkill_url(k)) target="_blank" rel="noopener" { "zKill" }
                    }
                }
            }
        }
    }
}

fn side_panel(
    battle: &Battle,
    side_idx: usize,
    ship_names: &std::collections::BTreeMap<i64, String>,
    pod_kills: &std::collections::HashMap<i64, i64>,
    affiliations: &std::collections::BTreeMap<i64, Affil>,
) -> Markup {
    let side = &battle.sides[side_idx];
    let groups = roster_groups(battle, side_idx);
    let pilots: usize = groups.iter().map(|g| g.lost + g.survived).sum();
    let doms = dominant_parties(battle, side_idx);
    let (_, breakdown) = side_breakdown(battle, side_idx);
    let overflow = breakdown.len().saturating_sub(doms.len());
    let roster = battle.roster(side_idx);
    html! {
        div .side-panel data-side=(side_idx) {
            div .side-head {
                div .side-title {
                    h3 title=(side_label(side)) { (side_label(side)) }
                    @if !doms.is_empty() {
                        span .dom-logos {
                            @for (p, r) in &doms {
                                @if let (Some(url), Some(zk)) = (party_logo_url(p), party_zkill_url(p)) {
                                    @let label = format!("{} - {:.0}%", p.name, r * 100.0);
                                    a .dom-logo-link href=(zk) target="_blank" rel="noopener"
                                        title=(label) data-party=(p.id) {
                                        img .dom-logo src=(url) width="26" height="26" loading="lazy" alt=(label);
                                    }
                                }
                            }
                            @if overflow > 0 {
                                button type="button" .more-chip data-side=(side_idx)
                                    title="Show full breakdown" { "+" (overflow) }
                            }
                        }
                    }
                }
                // Always render this subtitle row so every side header is the same height
                // and the stats/rosters below line up; a side with no coalition gets a
                // non-breaking placeholder of identical height.
                @let coalition_line = side
                    .coalition
                    .as_ref()
                    .filter(|_| !side.parties.is_empty())
                    .map(|c| format!("{} - {} parties", c, side.parties.len()));
                @if let Some(cm) = coalition_line {
                    div .coalition-members title=(cm) { (cm) }
                } @else {
                    div .coalition-members aria-hidden="true" { (PreEscaped("&nbsp;")) }
                }
            }
            div .side-stats {
                div .stat { span .stat-num { (pilots) } span .stat-label { "pilots" } }
                div .stat { span .stat-num { (side.kills) } span .stat-label { "kills" } }
                div .stat { span .stat-num { (side.losses) } span .stat-label { "losses" } }
                div .stat { span .stat-num .isk-destroyed { (fmt_isk(side.isk_destroyed)) } span .stat-label .isk-destroyed { "ISK destroyed" } }
                div .stat { span .stat-num .isk-lost { (fmt_isk(side.isk_lost)) } span .stat-label .isk-lost { "ISK lost" } }
                div .stat { span .stat-num { (fmt_eff(side)) } span .stat-label { "efficiency" } }
            }
            div .bar { div .bar-fill style=(format!("width:{:.0}%", side.isk_efficiency().unwrap_or(0.0))) {} }
            div .roster {
                @for g in &groups {
                    @let name = ship_names.get(&g.ship);
                    div .ship data-ship=(g.ship) title="Show these pilots" {
                        img .ship-icon src=(icon_url(g.ship)) width="48" height="48" loading="lazy" alt="ship";
                        @if let Some(n) = name {
                            span .ship-name title=(n) { (n) }
                        }
                        div .ship-counts {
                            @let total = g.lost + g.survived;
                            @if total > 0 {
                                @let pct = ((g.lost as f64 / total as f64) * 100.0).round() as u32;
                                div .lossbar title=(format!("{} of {} destroyed ({}%)", g.lost, total, pct)) {
                                    div .lossbar-fill style=(format!("width:{pct}%")) {}
                                    span .lossbar-label { (g.lost) "/" (total) " (" (pct) "%)" }
                                }
                            }
                        }
                    }
                }
            }
            div .details {
                @for p in &roster {
                    (detail_cell(p, pod_kills, affiliations))
                }
            }
        }
    }
}

fn breakdown_section(battle: &Battle, side_idx: usize) -> Markup {
    let side = &battle.sides[side_idx];
    let (_, entries) = side_breakdown(battle, side_idx);
    html! {
        div .bd-side {
            h4 title=(side_label(side)) { (side_label(side)) }
            @if entries.is_empty() {
                div .bd-empty { "No alliance/corp breakdown available." }
            } @else {
                ul .bd-list {
                    @for (p, count, r) in &entries {
                        li .bd-row data-party=(p.id) {
                            @if let Some(zk) = party_zkill_url(p) {
                                a .bd-entity href=(zk) target="_blank" rel="noopener" {
                                    (party_logo(p))
                                    span .bd-name title=(p.name) { (p.name) }
                                }
                            } @else {
                                span .bd-entity {
                                    (party_logo(p))
                                    span .bd-name title=(p.name) { (p.name) }
                                }
                            }
                            span .bd-count { (count) }
                            span .bd-pct { (format!("{:.0}%", r * 100.0)) }
                            select .bd-move data-party=(p.id) aria-label="Move party to side" {}
                        }
                    }
                }
            }
        }
    }
}

fn versus_strip(battle: &Battle) -> Markup {
    let emblems: Vec<(usize, Party)> = (0..battle.sides.len())
        .filter_map(|i| side_breakdown(battle, i).1.into_iter().next().map(|(p, _, _)| (i, p)))
        .collect();
    html! {
        @if !emblems.is_empty() {
            div .versus #versus {
                @for (n, (side_idx, p)) in emblems.iter().enumerate() {
                    @if n > 0 { span .vs-sep { "vs" } }
                    @if let Some(zk) = party_zkill_url(p) {
                        a .vs-emblem href=(zk) target="_blank" rel="noopener"
                            data-side=(side_idx) title=(p.name) {
                            @if let Some(url) = party_logo_url(p) {
                                img src=(url) width="40" height="40" loading="lazy" alt=(p.name);
                            }
                            span .vs-name { (side_label(&battle.sides[*side_idx])) }
                        }
                    } @else {
                        span .vs-emblem data-side=(side_idx) title=(p.name) {
                            span .vs-name { (side_label(&battle.sides[*side_idx])) }
                        }
                    }
                }
            }
        }
    }
}

/// Hover-highlight lookup maps, embedded as JSON for the viewer JS. Both maps contain only
/// integer ids, so the blob is safe to emit verbatim. `ka`: kill_id → attacker char_ids
/// (who scored each loss). `ck`: char_id → kill_ids that pilot was an attacker on.
fn involvement_json(battle: &Battle) -> String {
    use std::collections::BTreeMap;
    let mut ka: BTreeMap<i64, Vec<i64>> = BTreeMap::new();
    let mut ck: BTreeMap<i64, Vec<i64>> = BTreeMap::new();
    for e in &battle.engagements {
        let mut atk: Vec<i64> = Vec::new();
        for a in &e.attackers {
            if a.char_id == 0 {
                continue;
            }
            if !atk.contains(&a.char_id) {
                atk.push(a.char_id);
            }
            let kills = ck.entry(a.char_id).or_default();
            if !kills.contains(&e.kill_id) {
                kills.push(e.kill_id);
            }
        }
        ka.insert(e.kill_id, atk);
    }
    serde_json::json!({ "ka": ka, "ck": ck }).to_string()
}

/// Escape a JSON string for safe embedding inside an HTML `<script>` element: `<`, `>`, and
/// `&` become `\uXXXX` escapes. This keeps any `</script>` or HTML entity in EVE-supplied
/// names from breaking out of the script context, while remaining valid JSON.
fn js_safe_json(s: &str) -> String {
    s.replace('<', "\\u003c").replace('>', "\\u003e").replace('&', "\\u0026")
}

/// party->side grouping, with no server round-trip. Integers and short kind strings only,
/// except party/ship names, which are escaped by [`js_safe_json`]. Schema:
fn sides_data_json(doc: &BattleReportDoc) -> String {
    use serde_json::json;
    let b = &doc.battle;
    let kind_str = |k: PartyKind| match k {
        PartyKind::Alliance => "alliance",
        PartyKind::Corporation => "corp",
        _ => "other",
    };

    let mut parties = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut participants = Vec::new();
    for i in 0..b.sides.len() {
        for p in b.roster(i) {
            let kind = kind_str(p.party.kind);
            if p.party.id != 0 && seen.insert(p.party.id) {
                parties.push(json!({"id": p.party.id, "name": p.party.name, "kind": kind, "side": i}));
            }
            let lost_value = p.lost.as_ref().map(|l| l.value + l.pod_value).unwrap_or(0.0);
            participants.push(json!({
                "char": p.char_id, "party_id": p.party.id, "party_name": p.party.name,
                "party_kind": kind, "ship": p.ship, "lost_value": lost_value,
                "is_lost": p.lost.is_some(), "side": i
            }));
        }
    }

    let mut engagements = Vec::new();
    for e in &b.engagements {
        let mut apids: Vec<i64> = Vec::new();
        let mut fb = Vec::new();
        for a in &e.attackers {
            if a.party.id != 0 && !apids.contains(&a.party.id) {
                apids.push(a.party.id);
            }
            if a.final_blow && a.party.id != 0 {
                fb.push(json!({"p": a.party.id, "s": a.ship}));
            }
        }
        engagements.push(json!({
            "kill_id": e.kill_id, "victim_char": e.victim_char, "victim_value": e.isk,
            "attacker_party_ids": apids, "fb": fb
        }));
    }

    let ship_names: std::collections::BTreeMap<String, &String> =
        doc.ship_names.iter().map(|(k, v)| (k.to_string(), v)).collect();
    let v = json!({
        "sides_count": b.sides.len(),
        "parties": parties,
        "participants": participants,
        "engagements": engagements,
        "ship_names": ship_names,
    });
    js_safe_json(&v.to_string())
}

pub fn viewer_page(data: &CardData) -> Markup {
    let b = &data.doc.battle;
    let json_href = format!("/api/br/{}.json", data.id);
    let inv = involvement_json(b);
    let pod_kills = pod_kill_ids(b);
    let body = html! {
        section .viewer {
            div .v-head {
                div .v-tools {
                    a .icon-btn href=(json_href) download title="Download JSON" aria-label="Download JSON" {
                        (PreEscaped(ICON_DOWNLOAD))
                    }
                    button type="button" .icon-btn #copy-link title="Copy link" aria-label="Copy link" {
                        (PreEscaped(ICON_LINK))
                    }
                }
                a .back-link href="/br" { "← All battle reports" }
                h2 { (display_title(&data.doc)) }
                (systems_chips(b))
                div .v-meta {
                    span { (fmt_time(b.start)) " to " (fmt_time(b.end)) }
                    span .dot-sep { "·" }
                    span { (fmt_duration(b.end - b.start)) }
                    span .dot-sep { "·" }
                    span .isk { (fmt_isk(b.isk)) " ISK destroyed" }
                    span .dot-sep { "·" }
                    span { (b.kills) " kills" }
                }
                (versus_strip(b))
                div .v-actions {
                    span .meta-by { "uploaded by " (data.uploader) " · " (data.views) " views" }
                }
            }
            div .report-tools {
                div .view-toggle role="group" aria-label="Layout" {
                    button type="button" .seg data-mode="tiles" aria-pressed="true" { "Tiles" }
                    button type="button" .seg data-mode="details" aria-pressed="false" { "Details" }
                }
                button type="button" .btn #edit-sides { "Edit sides" }
            }
            div .report .view-tiles {
                div .filter-chip hidden {
                    img width="24" height="24" alt="hull";
                    span .fc-label { "Showing only this hull" }
                    button type="button" .btn .fc-clear { "Clear filter" }
                }
                div .panels {
                    @for i in 0..b.sides.len() {
                        (side_panel(b, i, &data.doc.ship_names, &pod_kills, &data.doc.affiliations))
                    }
                }
            }
            script type="application/json" #inv-data { (PreEscaped(inv)) }
            script type="application/json" #sides-data { (PreEscaped(sides_data_json(&data.doc))) }
            div .modal #breakdown-modal hidden {
                div .modal-overlay {}
                div .modal-panel role="dialog" aria-modal="true" aria-label="Edit sides" {
                    button type="button" .modal-close aria-label="Close" { "×" }
                    h3 { "Sides" }
                    p .modal-hint .editor-only {
                        "Move an alliance or corp to another side, or to a new side, then Apply. "
                        "This only changes your view. Nothing is saved or uploaded."
                    }
                    div .bd-sides {
                        @for i in 0..b.sides.len() {
                            (breakdown_section(b, i))
                        }
                    }
                    div .modal-actions .editor-only {
                        button type="button" .btn .primary #sides-apply { "Apply" }
                        button type="button" .btn #sides-reset { "Reset" }
                    }
                }
            }
        }
    };
    layout(&format!("{} - EVE Spai", display_title(&data.doc)), body, Some(VIEWER_JS))
}

pub fn not_found_page() -> Markup {
    let body = html! {
        section .notfound {
            h2 { "Report not found" }
            p { "This battle report does not exist, or it is unlisted and only reachable by its direct link." }
            a .btn .primary href="/br" { "Browse battle reports" }
        }
    };
    layout("Not found - EVE Spai", body, None)
}

fn layout(title: &str, body: Markup, script: Option<&str>) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="UTF-8";
                meta name="viewport" content="width=device-width, initial-scale=1.0";
                title { (title) }
                style { (PreEscaped(CSS)) }
            }
            body {
                header .site {
                    a .brand href="/br" {
                        "EVE" (PreEscaped("&nbsp;")) "Spai" span .dot { "." }
                    }
                }
                main .wrap {
                    (body)
                }
                footer { p { "EVE Spai battle reports. EVE Online and all related assets are property of Fenris Creations." } }
                @if let Some(js) = script {
                    script { (PreEscaped(js)) }
                }
            }
        }
    }
}

const DIRECTORY_JS: &str = r#"
(function(){
  var f=document.querySelector('form.filters'); if(!f) return;
  var KEY='br-filter-focus';
  // Restore focus + caret after the debounced reload so the user can keep typing.
  try{
    var s=JSON.parse(sessionStorage.getItem(KEY)||'null');
    if(s){ sessionStorage.removeItem(KEY);
      var el=f.querySelector('[name="'+s.name+'"]');
      if(el){ el.focus();
        if(s.pos!=null){ try{ el.setSelectionRange(s.pos,s.pos); }catch(e){} } }
    }
  }catch(e){}
  function save(el){ var pos=null; try{ pos=el.selectionStart; }catch(e){}
    try{ sessionStorage.setItem(KEY,JSON.stringify({name:el.name,pos:pos})); }catch(e){} }
  var t;
  f.querySelector('[name=sort]').addEventListener('change',function(){ f.submit(); });
  f.querySelectorAll('input[type=text],input[type=number],input[type=date]').forEach(function(el){
    el.addEventListener('input',function(){ clearTimeout(t); save(el); t=setTimeout(function(){ f.submit(); },350); });
  });
})();
"#;

/// Inline glyphs for the header icon buttons (developer-authored, so emitted via
/// `PreEscaped`). `currentColor` lets them inherit the button's muted/hover colour.
const ICON_DOWNLOAD: &str = r#"<svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 3v12"/><path d="M7 11l5 5 5-5"/><path d="M5 21h14"/></svg>"#;
const ICON_LINK: &str = r#"<svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M10 13a5 5 0 0 0 7 0l3-3a5 5 0 0 0-7-7l-1 1"/><path d="M14 11a5 5 0 0 0-7 0l-3 3a5 5 0 0 0 7 7l1-1"/></svg>"#;

const VIEWER_JS: &str = r#"
(function(){
  var cp=document.getElementById('copy-link');
  if(cp){ cp.addEventListener('click',function(){
    navigator.clipboard.writeText(window.location.href).then(function(){
      var o=cp.innerHTML; cp.classList.add('copied'); cp.textContent='Copied!';
      setTimeout(function(){ cp.innerHTML=o; cp.classList.remove('copied'); },1500);
    });
  }); }

  var report=document.querySelector('.report');
  if(!report) return;

  var cells=Array.prototype.slice.call(document.querySelectorAll('.dcell'));
  var chip=document.querySelector('.filter-chip');

  // Remove any hull filter: reveal every Details cell and hide the chip. The cells use a
  // 'filtered-out' class (not the `hidden` attribute) because the author `.dcell` display
  // rule would otherwise win over the UA `[hidden]` rule and the cell would stay visible.
  function clearFilter(){
    cells.forEach(function(c){ c.classList.remove('filtered-out'); });
    if(chip){ chip.hidden=true; }
  }

  // --- Tiles / Details toggle (resets any active hull filter) ---
  var tabs=Array.prototype.slice.call(document.querySelectorAll('.view-toggle .seg'));
  function setMode(mode){
    report.classList.remove('view-tiles','view-details');
    report.classList.add('view-'+mode);
    tabs.forEach(function(t){ t.setAttribute('aria-pressed', t.dataset.mode===mode?'true':'false'); });
  }
  tabs.forEach(function(t){ t.addEventListener('click',function(){ clearFilter(); setMode(t.dataset.mode); }); });

  // --- Involvement hover highlight ---
  var maps={ka:{},ck:{}};
  var el=document.getElementById('inv-data');
  if(el){ try{ maps=JSON.parse(el.textContent)||maps; }catch(e){} }
  var ka=maps.ka||{}, ck=maps.ck||{};
  var byChar={}, byKill={};
  cells.forEach(function(c){
    var ch=c.getAttribute('data-char'); if(ch){ (byChar[ch]=byChar[ch]||[]).push(c); }
    var k=c.getAttribute('data-kill'); if(k){ (byKill[k]=byKill[k]||[]).push(c); }
  });
  function clearHi(){ cells.forEach(function(c){ c.classList.remove('hi-killer','hi-victim'); }); }
  cells.forEach(function(c){
    c.addEventListener('mouseenter',function(){
      clearHi();
      var kill=c.getAttribute('data-kill');
      if(kill && ka[kill]){ ka[kill].forEach(function(id){
        (byChar[id]||[]).forEach(function(x){ x.classList.add('hi-killer'); }); }); }
      var ch=c.getAttribute('data-char');
      if(ch && ck[ch]){ ck[ch].forEach(function(k){
        (byKill[k]||[]).forEach(function(x){ x.classList.add('hi-victim'); }); }); }
    });
    c.addEventListener('mouseleave',clearHi);
  });

  // --- Hull filter: click a tile -> Details, only that hull. Delegated, so tiles the side
  // editor rebuilds keep working. `cells` is refreshed live so moved nodes are included. ---
  report.addEventListener('click',function(e){
    var tile=e.target.closest ? e.target.closest('.ship[data-ship]') : null;
    if(!tile || !report.contains(tile)) return;
    var ship=tile.getAttribute('data-ship');
    setMode('details');
    Array.prototype.slice.call(report.querySelectorAll('.dcell')).forEach(function(c){
      c.classList.toggle('filtered-out', c.getAttribute('data-ship')!==ship);
    });
    if(chip){
      chip.hidden=false;
      var img=chip.querySelector('img');
      if(img){ img.src='https://images.evetech.net/types/'+ship+'/icon?size=64'; }
    }
  });
  if(chip){ var cb=chip.querySelector('.fc-clear'); if(cb){ cb.addEventListener('click',clearFilter); } }

  // --- Breakdown / side-editor modal open/close ---
  var modal=document.getElementById('breakdown-modal');
  function openModal(){ if(modal){ modal.hidden=false; } }
  function closeModal(){ if(modal){ modal.hidden=true; } }
  var openers=[].slice.call(document.querySelectorAll('.more-chip'));
  var es=document.getElementById('edit-sides'); if(es){ openers.push(es); }
  openers.forEach(function(b){ b.addEventListener('click',openModal); });
  if(modal){
    var ov=modal.querySelector('.modal-overlay'); if(ov){ ov.addEventListener('click',closeModal); }
    var x=modal.querySelector('.modal-close'); if(x){ x.addEventListener('click',closeModal); }
    document.addEventListener('keydown',function(e){ if(e.key==='Escape'){ closeModal(); } });
  }
})();

// --- Side editor: ephemeral, client-only regrouping of parties into sides. Reads #sides-data
// (integer/short-string inputs). If absent/unparseable, the controls stay hidden and the
// static report still works. Re-bins the server-rendered Details cells by party between side
// containers, rebuilds Tiles/stats/headers/emblems, and supports new sides. Never persisted. ---
(function(){
  var dataEl=document.getElementById('sides-data');
  var report=document.querySelector('.report');
  var modal=document.getElementById('breakdown-modal');
  if(!dataEl || !report || !modal) return;
  var SD; try{ SD=JSON.parse(dataEl.textContent); }catch(e){ return; }
  if(!SD || !SD.participants || !SD.parties) return;

  var panels=report.querySelector('.panels');
  if(!panels) return;
  modal.classList.add('editor-ready'); // reveals the move selects + Apply/Reset

  var POD=[670,33328];
  var participants=SD.participants, engagements=SD.engagements||[], shipNames=SD.ship_names||{};
  var partyById={}; SD.parties.forEach(function(p){ partyById[p.id]=p; });
  var charParty={}; participants.forEach(function(p){ charParty[p.char]=p.party_id; });

  // party_id (string) -> side index. Start from the server grouping.
  function origMap(){ var m={}; participants.forEach(function(p){ m[''+p.party_id]=p.side; }); return m; }
  var curMap=origMap();

  // Cache the server-rendered Details cells (refs survive being moved/detached).
  var allCells=[].slice.call(panels.querySelectorAll('.dcell'));

  function fmtIsk(v){
    var a=Math.abs(v);
    if(a>=1e12) return (v/1e12).toFixed(2)+'T';
    if(a>=1e9) return (v/1e9).toFixed(2)+'B';
    if(a>=1e6) return (v/1e6).toFixed(1)+'M';
    if(a>=1e3) return (v/1e3).toFixed(0)+'k';
    return v.toFixed(0);
  }
  function logoUrl(p){
    if(!p||!p.id) return null;
    if(p.kind==='alliance') return 'https://images.evetech.net/alliances/'+p.id+'/logo?size=32';
    if(p.kind==='corp') return 'https://images.evetech.net/corporations/'+p.id+'/logo?size=32';
    return null;
  }
  function zkUrl(p){
    if(!p||!p.id) return null;
    if(p.kind==='alliance') return 'https://zkillboard.com/alliance/'+p.id+'/';
    if(p.kind==='corp') return 'https://zkillboard.com/corporation/'+p.id+'/';
    return null;
  }
  function sideCount(){ return panels.querySelectorAll('.side-panel').length; }

  function makeTile(ship,lost,survived,kb,name){
    var d=document.createElement('div'); d.className='ship'; d.setAttribute('data-ship',ship);
    d.title='Show these pilots';
    var img=document.createElement('img'); img.className='ship-icon';
    img.width=48; img.height=48; img.loading='lazy'; img.alt='ship';
    img.src='https://images.evetech.net/types/'+ship+'/icon?size=64'; d.appendChild(img);
    if(name){ var ns=document.createElement('span'); ns.className='ship-name'; ns.title=name; ns.textContent=name; d.appendChild(ns); }
    var c=document.createElement('div'); c.className='ship-counts';
    var total=lost+survived;
    if(total>0){
      var pct=Math.round(lost/total*100);
      var bar=document.createElement('div'); bar.className='lossbar';
      bar.title=lost+' of '+total+' destroyed ('+pct+'%)';
      var fill=document.createElement('div'); fill.className='lossbar-fill'; fill.style.width=pct+'%'; bar.appendChild(fill);
      var lab=document.createElement('span'); lab.className='lossbar-label'; lab.textContent=lost+'/'+total+' ('+pct+'%)'; bar.appendChild(lab);
      c.appendChild(bar);
    }
    d.appendChild(c); return d;
  }

  function computeSide(i){
    var parts=participants.filter(function(p){ return curMap[''+p.party_id]===i; });
    var pilots=parts.length, losses=0, iskLost=0;
    parts.forEach(function(p){ if(p.is_lost){ losses++; iskLost+=p.lost_value||0; } });
    var kills=0, destroyed=0;
    engagements.forEach(function(e){
      var vside=charParty[e.victim_char]!=null ? curMap[''+charParty[e.victim_char]] : null;
      var onThis=(e.attacker_party_ids||[]).some(function(pid){ return curMap[''+pid]===i; });
      if(onThis && vside!==i){ kills++; destroyed+=e.victim_value||0; }
    });
    var eff=(destroyed+iskLost)>0 ? destroyed/(destroyed+iskLost)*100 : null;
    return {parts:parts,pilots:pilots,losses:losses,iskLost:iskLost,kills:kills,destroyed:destroyed,eff:eff};
  }

  function rebuildHeader(sp,i,parts){
    var total=parts.length, counts={};
    parts.forEach(function(p){
      if((p.party_kind==='alliance'||p.party_kind==='corp') && p.party_id){
        var e=counts[p.party_id]||(counts[p.party_id]={id:p.party_id,name:p.party_name,kind:p.party_kind,c:0});
        e.c++;
      }
    });
    var arr=Object.keys(counts).map(function(k){ return counts[k]; });
    arr.sort(function(a,b){ return b.c-a.c || a.id-b.id; });
    var doms=arr.filter(function(e){ return total>0 && e.c/total>0.10; }).slice(0,3);
    var h3=sp.querySelector('.side-title h3');
    if(h3){ var lbl=doms.length?doms[0].name:('Side '+(i+1)); h3.textContent=lbl; h3.title=lbl; }
    // A regrouped side has no coalition concept: keep the subtitle row (so headers stay the
    // same height and panels align) but replace its text with a non-breaking placeholder.
    var cm=sp.querySelector('.coalition-members');
    if(!cm){ cm=document.createElement('div'); cm.className='coalition-members'; sp.querySelector('.side-head').appendChild(cm); }
    cm.textContent=' '; cm.removeAttribute('title'); cm.setAttribute('aria-hidden','true');
    var logos=sp.querySelector('.dom-logos');
    if(!logos){ logos=document.createElement('span'); logos.className='dom-logos'; sp.querySelector('.side-title').appendChild(logos); }
    logos.innerHTML='';
    doms.forEach(function(e){
      var url=logoUrl(e), zk=zkUrl(e); if(!url||!zk) return;
      var pct=Math.round(e.c/total*100), label=e.name+' - '+pct+'%';
      var a=document.createElement('a'); a.className='dom-logo-link'; a.href=zk; a.target='_blank'; a.rel='noopener';
      a.title=label; a.setAttribute('data-party',e.id);
      var img=document.createElement('img'); img.className='dom-logo'; img.width=26; img.height=26;
      img.loading='lazy'; img.alt=label; img.src=url; a.appendChild(img); logos.appendChild(a);
    });
    var overflow=arr.length-doms.length;
    if(overflow>0){ var b=document.createElement('button'); b.type='button'; b.className='more-chip';
      b.setAttribute('data-side',i); b.title='Show full breakdown'; b.textContent='+'+overflow;
      b.addEventListener('click',function(){ modal.hidden=false; }); logos.appendChild(b); }
  }

  function setStats(sp,c){
    var nums=sp.querySelectorAll('.side-stats .stat .stat-num');
    if(nums.length<6) return;
    nums[0].textContent=c.pilots;
    nums[1].textContent=c.kills;
    nums[2].textContent=c.losses;
    nums[3].textContent=fmtIsk(c.destroyed);
    nums[4].textContent=fmtIsk(c.iskLost);
    nums[5].textContent=(c.eff==null?'-':Math.round(c.eff)+'%');
    var bar=sp.querySelector('.bar .bar-fill'); if(bar){ bar.style.width=(c.eff==null?0:Math.round(c.eff))+'%'; }
  }

  function rebuildTiles(sp,i,parts){
    var roster=sp.querySelector('.roster'); if(!roster) return; roster.innerHTML='';
    var g={};
    parts.forEach(function(p){ if(POD.indexOf(p.ship)>=0) return;
      var e=g[p.ship]||(g[p.ship]={lost:0,survived:0}); if(p.is_lost) e.lost++; else e.survived++; });
    var kb={};
    engagements.forEach(function(e){ (e.fb||[]).forEach(function(f){
      if(curMap[''+f.p]===i && POD.indexOf(f.s)<0){ kb[f.s]=(kb[f.s]||0)+1; } }); });
    var ships=Object.keys(g).map(Number);
    ships.sort(function(a,b){ return g[b].lost-g[a].lost || (g[b].lost+g[b].survived)-(g[a].lost+g[a].survived) || a-b; });
    ships.forEach(function(s){ roster.appendChild(makeTile(s,g[s].lost,g[s].survived,kb[s]||0,shipNames[''+s])); });
  }

  function rebuildVersus(n){
    var v=document.getElementById('versus'); if(!v) return; v.innerHTML='';
    for(var i=0;i<n;i++){
      var c=computeSide(i);
      var counts={};
      c.parts.forEach(function(p){ if((p.party_kind==='alliance'||p.party_kind==='corp')&&p.party_id){
        var e=counts[p.party_id]||(counts[p.party_id]={id:p.party_id,name:p.party_name,kind:p.party_kind,c:0}); e.c++; } });
      var arr=Object.keys(counts).map(function(k){return counts[k];});
      arr.sort(function(a,b){ return b.c-a.c || a.id-b.id; });
      if(!arr.length) continue;
      var top=arr[0], url=logoUrl(top), zk=zkUrl(top);
      if(i>0 && v.children.length){ var sep=document.createElement('span'); sep.className='vs-sep'; sep.textContent='vs'; v.appendChild(sep); }
      var sp=panels.querySelectorAll('.side-panel')[i];
      var label=sp?sp.querySelector('.side-title h3').textContent:top.name;
      var node; if(zk){ node=document.createElement('a'); node.href=zk; node.target='_blank'; node.rel='noopener'; }
      else { node=document.createElement('span'); }
      node.className='vs-emblem'; node.setAttribute('data-side',i); node.title=top.name;
      if(url){ var img=document.createElement('img'); img.width=40; img.height=40; img.loading='lazy'; img.alt=top.name; img.src=url; node.appendChild(img); }
      var ns=document.createElement('span'); ns.className='vs-name'; ns.textContent=label; node.appendChild(ns);
      v.appendChild(node);
    }
  }

  function ensurePanels(n){
    var sps=[].slice.call(panels.querySelectorAll('.side-panel'));
    var tmpl=sps[0];
    while(sps.length<n){ var c=tmpl.cloneNode(true); panels.appendChild(c); sps.push(c); }
    while(sps.length>n){ panels.removeChild(sps.pop()); }
    sps=[].slice.call(panels.querySelectorAll('.side-panel'));
    sps.forEach(function(sp,i){ sp.setAttribute('data-side',i); });
    return sps;
  }

  function renderControls(n){
    [].slice.call(document.querySelectorAll('.bd-move')).forEach(function(sel){
      var pid=sel.getAttribute('data-party'); var cur=curMap[pid];
      sel.innerHTML='';
      for(var i=0;i<n;i++){ var o=document.createElement('option'); o.value=i; o.textContent='Side '+(i+1); sel.appendChild(o); }
      var on=document.createElement('option'); on.value='new'; on.textContent='New side'; sel.appendChild(on);
      sel.value=(cur==null?0:cur);
    });
  }

  function applyMap(){
    var n=0; Object.keys(curMap).forEach(function(k){ if(curMap[k]!=null) n=Math.max(n,curMap[k]+1); });
    n=Math.max(n,1);
    var sps=ensurePanels(n);
    var dconts=sps.map(function(sp){ var d=sp.querySelector('.details'); d.innerHTML=''; return d; });
    allCells.forEach(function(cell){
      var s=curMap[cell.getAttribute('data-party')]; if(s==null||s>=n) s=0;
      dconts[s].appendChild(cell);
    });
    for(var i=0;i<n;i++){ var c=computeSide(i); rebuildTiles(sps[i],i,c.parts); setStats(sps[i],c); rebuildHeader(sps[i],i,c.parts); }
    rebuildVersus(n);
    renderControls(n);
  }

  // Read the selects into curMap; "new" collapses to a single fresh side index.
  function readControls(){
    var newIdx=sideCount();
    [].slice.call(document.querySelectorAll('.bd-move')).forEach(function(sel){
      var pid=sel.getAttribute('data-party');
      curMap[pid]=(sel.value==='new')?newIdx:parseInt(sel.value,10);
    });
  }

  var applyBtn=document.getElementById('sides-apply');
  var resetBtn=document.getElementById('sides-reset');
  if(applyBtn){ applyBtn.addEventListener('click',function(){ readControls(); applyMap(); }); }
  if(resetBtn){ resetBtn.addEventListener('click',function(){ curMap=origMap(); applyMap(); }); }

  renderControls(SD.sides_count||sideCount());
})();
"#;

const CSS: &str = r#"
:root{
  --bg:#0c1117; --panel:#141a22; --panel-2:#171f29; --line:#243040;
  --blue:#4fc3f7; --blue-dim:#2b6c8c; --red:#e04c4c; --green:#5ac86a; --text:#e7eef5; --muted:#8ea0b2;
  --mono: ui-monospace,"SF Mono","JetBrains Mono",Menlo,Consolas,monospace;
}
*{box-sizing:border-box;}
body{
  margin:0; color:var(--text); background:var(--bg); line-height:1.6;
  font-family:-apple-system,BlinkMacSystemFont,"Segoe UI",Roboto,Helvetica,Arial,sans-serif;
  -webkit-font-smoothing:antialiased;
}
body::before{
  content:""; position:fixed; inset:0; z-index:-1;
  background:
    radial-gradient(900px 600px at 50% -10%, rgba(79,195,247,0.10), transparent 60%),
    radial-gradient(600px 500px at 85% 0%, rgba(224,76,76,0.06), transparent 55%);
}
a{color:var(--blue); text-decoration:none;}
a:hover{text-decoration:underline;}
.wrap{max-width:960px; margin:0 auto; padding:0 20px 60px;}
header.site{text-align:center; padding:28px 20px 8px;}
.brand{
  font-size:30px; font-weight:700; letter-spacing:-0.5px;
  background:linear-gradient(180deg,#ffffff,#9fd8f3);
  -webkit-background-clip:text; background-clip:text; color:transparent;
}
.brand:hover{text-decoration:none;}
.dot{color:var(--red); -webkit-text-fill-color:var(--red);}
h2{font-size:20px; margin:18px 0 14px;}
h3{font-size:17px; margin:0;}
footer{margin-top:40px; padding:24px 0; border-top:1px solid var(--line); color:var(--muted); font-size:13px; text-align:center;}

.btn{
  display:inline-flex; align-items:center; gap:8px; padding:9px 16px; border-radius:10px;
  font-size:14px; font-weight:600; border:1px solid var(--line); color:var(--text);
  background:var(--panel); cursor:pointer;
}
.btn:hover{text-decoration:none; border-color:var(--blue-dim);}
.btn.primary{color:var(--bg); border-color:transparent; background:linear-gradient(180deg,#6fd0fb,var(--blue));}
.btn.disabled{opacity:0.4; cursor:default;}

.toolbar h2{margin-bottom:10px;}
form.filters{display:flex; flex-wrap:wrap; gap:8px; margin-bottom:20px;}
form.filters input, form.filters select{
  background:var(--panel); border:1px solid var(--line); border-radius:9px;
  color:var(--text); padding:8px 11px; font-size:14px; font-family:inherit;
}
form.filters input:focus, form.filters select:focus{outline:none; border-color:var(--blue-dim);}

.cards{display:grid; grid-template-columns:repeat(2,1fr); gap:14px;}
.card{
  display:block; min-width:0; background:var(--panel); border:1px solid var(--line); border-radius:12px;
  padding:16px; color:var(--text);
}
.card:hover{text-decoration:none; border-color:var(--blue-dim);}
.card-title{font-size:16px; font-weight:600; overflow-wrap:anywhere;}
.systems{display:block; margin:6px 0; color:var(--muted); font-size:13px;}
.systems .sec{font-family:var(--mono); font-weight:600;}
.systems .sep{color:var(--line);}
.card-meta, .v-meta{display:flex; flex-wrap:wrap; gap:6px; color:var(--muted); font-size:13px; margin:6px 0;}
.dot-sep{color:var(--line);}
.isk{color:var(--blue); font-family:var(--mono);}
.sides{margin:10px 0 6px;}
.side-row{display:flex; align-items:center; gap:8px; margin:4px 0; font-size:13px;}
.side-name{flex:0 0 38%; overflow:hidden; text-overflow:ellipsis; white-space:nowrap;}
.bar{flex:1; height:8px; background:var(--panel-2); border:1px solid var(--line); border-radius:6px; overflow:hidden;}
.bar-fill{height:100%; background:linear-gradient(90deg,var(--blue-dim),var(--blue));}
.side-eff{flex:0 0 auto; font-family:var(--mono); color:var(--muted);}
.card-foot{display:flex; justify-content:space-between; color:var(--muted); font-size:12.5px; margin-top:10px;}
.empty{padding:40px; text-align:center; color:var(--muted); background:var(--panel); border:1px solid var(--line); border-radius:12px;}

.pager{display:flex; align-items:center; justify-content:space-between; margin-top:22px;}
.page-num{color:var(--muted); font-size:13px;}

.viewer .v-head{position:relative; background:var(--panel); border:1px solid var(--line); border-radius:12px; padding:18px 20px; margin-bottom:18px;}
.viewer .v-head h2{padding-right:84px; overflow-wrap:anywhere;}
.back-link{display:inline-block; margin-bottom:6px; color:var(--blue); font-size:13px; font-weight:600;}
.v-tools{position:absolute; top:14px; right:16px; display:flex; gap:6px;}
.icon-btn{display:inline-flex; align-items:center; justify-content:center; min-width:34px; height:34px; padding:0 8px; border-radius:9px; border:1px solid var(--line); background:var(--panel); color:var(--muted); cursor:pointer; font-size:13px; font-weight:600;}
.icon-btn:hover{color:var(--blue); border-color:var(--blue-dim); text-decoration:none;}
.icon-btn.copied{color:var(--blue);}
.v-actions{display:flex; flex-wrap:wrap; align-items:center; gap:10px; margin-top:12px;}
.meta-by{color:var(--muted); font-size:13px;}
.panels{display:grid; grid-template-columns:repeat(2,1fr); gap:16px;}
/* min-width:0 lets a grid item shrink below its content width, so a long unbreakable name
   inside cannot widen the column and force horizontal scrolling. */
.side-panel{min-width:0; background:var(--panel); border:1px solid var(--line); border-radius:12px; padding:16px;}
.side-head h3{margin-bottom:2px;}
.side-title h3{min-width:0; overflow:hidden; text-overflow:ellipsis; white-space:nowrap;}
/* Always occupies one line (an nbsp placeholder fills it when there is no coalition) so
   both side headers are the same height and the stats/rosters below line up. */
.coalition-members{color:var(--muted); font-size:12.5px; min-height:1.6em; overflow:hidden; text-overflow:ellipsis; white-space:nowrap;}
.side-stats{display:grid; grid-template-columns:repeat(3,1fr); gap:10px; margin:12px 0;}
.stat{display:flex; flex-direction:column;}
.stat-num{font-size:16px; font-weight:600; font-family:var(--mono);}
.stat-label{color:var(--muted); font-size:11.5px; text-transform:uppercase; letter-spacing:0.4px;}
/* ISK destroyed reads green, ISK lost reads red (number + label). */
.stat-num.isk-destroyed{color:var(--green);}
.stat-label.isk-destroyed{color:var(--green);}
.stat-num.isk-lost{color:var(--red);}
.stat-label.isk-lost{color:var(--red);}
.roster{display:grid; grid-template-columns:repeat(auto-fit,minmax(78px,1fr)); gap:10px; margin-top:14px;}
.ship{display:flex; flex-direction:column; align-items:center; gap:4px; min-width:0; padding:4px; border-radius:8px; cursor:pointer;}
.ship:hover{background:var(--panel-2);}
.ship-icon{width:48px; height:48px; border-radius:8px; border:1px solid var(--line); background:var(--panel-2);}
/* Hull name under the tile; clipped with ellipsis so a long name can't widen the grid cell. */
.ship-name{max-width:100%; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; font-size:11px; color:var(--text); text-align:center;}
.ship-counts{display:flex; flex-direction:column; align-items:center; gap:3px; width:100%; font-size:11px; line-height:1.3;}
/* Loss bar: full width = all pilots of this hull, red fill = destroyed, centred X/Y (NN%) overlay. */
.lossbar{position:relative; align-self:stretch; height:15px; background:var(--panel-2); border:1px solid var(--line); border-radius:3px; overflow:hidden;}
.lossbar-fill{position:absolute; left:0; top:0; bottom:0; background:var(--red);}
.lossbar-label{position:absolute; inset:0; display:flex; align-items:center; justify-content:center; font-size:10px; font-weight:600; color:var(--text); text-shadow:0 1px 2px rgba(0,0,0,0.8); white-space:nowrap;}

/* Tiles / Details toggle and layout switching */
.report-tools{display:flex; flex-wrap:wrap; align-items:center; gap:10px; margin-bottom:14px;}
#edit-sides{padding:8px 14px; font-size:14px;}
.view-toggle{display:inline-flex; border:1px solid var(--line); border-radius:10px; overflow:hidden;}
.view-toggle .seg{padding:8px 18px; font-size:14px; font-weight:600; background:var(--panel); color:var(--muted); border:none; border-right:1px solid var(--line); cursor:pointer; font-family:inherit;}
.view-toggle .seg:last-child{border-right:none;}
.view-toggle .seg[aria-pressed="true"]{background:var(--blue-dim); color:var(--text);}
.report.view-tiles .details{display:none;}
.report.view-details .roster{display:none;}

/* Versus strip - each side's top emblem */
.versus{display:flex; flex-wrap:wrap; align-items:center; gap:12px; margin:12px 0 4px;}
.vs-emblem{display:inline-flex; align-items:center; gap:8px; min-width:0; max-width:100%; padding:6px 10px; background:var(--panel-2); border:1px solid var(--line); border-radius:10px; color:var(--text); cursor:pointer; font-family:inherit; font-size:14px; font-weight:600;}
.vs-emblem:hover{border-color:var(--blue-dim); text-decoration:none;}
.vs-emblem img{flex:0 0 auto; border-radius:6px;}
.vs-name{min-width:0; max-width:160px; overflow:hidden; text-overflow:ellipsis; white-space:nowrap;}
.vs-sep{color:var(--muted); font-style:italic; font-size:13px;}

/* Dominant-party logos + overflow chip in side headers */
.side-title{display:flex; align-items:center; gap:8px; flex-wrap:wrap;}
.dom-logos{display:inline-flex; align-items:center; gap:5px;}
.dom-logo-link{display:inline-flex; line-height:0;}
.dom-logo-link:hover{text-decoration:none;}
.dom-logo{border-radius:5px; border:1px solid var(--line); background:var(--panel-2);}
.more-chip{padding:3px 8px; font-size:12px; font-weight:600; color:var(--muted); background:var(--panel-2); border:1px solid var(--line); border-radius:8px; cursor:pointer; font-family:inherit;}
.more-chip:hover{border-color:var(--blue-dim); color:var(--text);}

/* Hull filter chip. The explicit [hidden] rule beats the author `display:flex` above -
   without it the boolean `hidden` attribute would not hide the chip. */
.filter-chip{display:flex; align-items:center; gap:10px; margin-bottom:14px; padding:8px 12px; background:var(--panel); border:1px solid var(--blue-dim); border-radius:10px; font-size:13px;}
.filter-chip[hidden]{display:none;}
.filter-chip img{flex:0 0 auto; border-radius:5px; border:1px solid var(--line);}
.filter-chip .fc-label{flex:1; min-width:0; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; color:var(--muted);}
.filter-chip .btn{flex:0 0 auto; padding:5px 11px; font-size:13px;}

/* Details layout - per-pilot cells */
.details{display:flex; flex-direction:column; gap:6px; margin-top:14px;}
.dcell{display:flex; align-items:center; gap:10px; padding:6px 8px; border:1px solid var(--line); border-radius:8px; background:var(--panel-2);}
.dcell.filtered-out{display:none;}
.dcell.lost{border-color:#5a2a2a;}
.dhull{flex:0 0 auto; border-radius:6px; border:1px solid var(--line); background:var(--panel);}
.dinfo{display:flex; flex-direction:column; min-width:0; flex:1;}
.dpilot{display:flex; align-items:center; gap:5px; min-width:0; font-size:14px;}
.dpilot-name{min-width:0; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; color:var(--text);}
.dpilot-name:hover{color:var(--blue);}
.affil-logo{flex:0 0 auto; display:inline-flex; line-height:0;}
.affil-logo:hover{text-decoration:none;}
.affil-logo img{border-radius:3px; border:1px solid var(--line); background:var(--panel);}
.dparty{display:flex; align-items:center; gap:5px; min-width:0; color:var(--muted); font-size:12.5px;}
.dparty-name{min-width:0; overflow:hidden; text-overflow:ellipsis; white-space:nowrap;}
.party-logo{flex:0 0 auto; border-radius:4px;}
.dloss{display:flex; align-items:center; gap:8px; flex:0 0 auto; font-size:12.5px;}
.destroyed{color:var(--red); font-size:11px; font-weight:600; text-transform:uppercase; letter-spacing:0.4px;}
.dval{font-family:var(--mono); color:var(--red);}
.zk{font-weight:600;}
/* Pod kill rendered as a secondary, indented sub-row beneath its ship row. */
.dpod-row{margin-left:24px; padding:4px 8px; background:transparent; border-style:dashed; opacity:0.85;}
.dpod-hull{width:32px; height:32px;}
.dpod-row .dpilot{font-size:13px; color:var(--muted);}
.dcell.hi-killer{outline:2px solid var(--blue); background:rgba(79,195,247,0.12);}
.dcell.hi-victim{outline:2px solid var(--red); background:rgba(224,76,76,0.12);}

/* Breakdown modal */
.modal[hidden]{display:none;}
.modal{position:fixed; inset:0; z-index:50; display:flex; align-items:center; justify-content:center; padding:20px;}
.modal-overlay{position:fixed; inset:0; background:rgba(4,8,12,0.72);}
.modal-panel{position:relative; z-index:1; width:100%; max-width:560px; max-height:80vh; overflow-x:hidden; overflow-y:auto; background:var(--panel); border:1px solid var(--line); border-radius:14px; padding:20px;}
.modal-close{position:absolute; top:10px; right:12px; background:none; border:none; color:var(--muted); font-size:24px; line-height:1; cursor:pointer;}
.modal-close:hover{color:var(--text);}
.modal-panel h3{margin-bottom:12px; padding-right:24px;}
.bd-sides{display:grid; grid-template-columns:repeat(2,1fr); gap:16px;}
/* min-width:0 so a long alliance name can't widen its column past the panel. */
.bd-side{min-width:0;}
.bd-side h4{min-width:0; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; font-size:14px; margin:0 0 8px;}
.bd-list{list-style:none; margin:0; padding:0; display:flex; flex-direction:column; gap:6px;}
.bd-row{display:flex; align-items:center; gap:8px; min-width:0; font-size:13px;}
.bd-entity{display:flex; align-items:center; gap:6px; flex:1; min-width:0; color:var(--text);}
.bd-entity:hover{text-decoration:none; color:var(--blue);}
.bd-name{flex:1; min-width:0; overflow:hidden; text-overflow:ellipsis; white-space:nowrap;}
.bd-count{font-family:var(--mono); flex:0 0 auto;}
.bd-pct{font-family:var(--mono); color:var(--muted); min-width:38px; text-align:right; flex:0 0 auto;}
.bd-empty{color:var(--muted); font-size:13px;}
/* Side-editor controls: hidden until the JS confirms the data parsed (editor-ready). */
.editor-only{display:none;}
.modal.editor-ready .editor-only{display:block;}
.modal.editor-ready .modal-actions{display:flex; gap:10px; margin-top:16px;}
.bd-move{display:none;}
.modal.editor-ready .bd-move{display:inline-block; flex:0 0 auto; max-width:120px; background:var(--panel-2); border:1px solid var(--line); border-radius:7px; color:var(--text); padding:3px 6px; font-size:12px; font-family:inherit;}
.modal-hint{color:var(--muted); font-size:12.5px; margin:0 0 12px;}
.notfound{text-align:center; padding:60px 20px;}
.notfound p{color:var(--muted); max-width:480px; margin:10px auto 24px;}

@media (max-width:700px){
  .cards{grid-template-columns:1fr;}
  .panels{grid-template-columns:1fr;}
  .bd-sides{grid-template-columns:1fr;}
  .side-name{flex-basis:42%;}
  form.filters input, form.filters select{flex:1 1 100%;}
}
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use br_core::battle::{
        Affil, Attacker, Engagement, Overrides, Party, PartyKind, BATTLE_BREAK_SECS,
    };

    fn party(id: i64, name: &str) -> Party {
        Party { id, name: name.to_string(), kind: PartyKind::Alliance }
    }

    fn eng(
        kill_id: i64,
        time: i64,
        victim: (i64, &str, &str, i64),
        killer: (i64, &str, &str, i64),
        fb: bool,
    ) -> Engagement {
        Engagement {
            kill_id,
            time,
            system_id: 30000142,
            system_name: "Jita".to_string(),
            security: 0.9,
            victim: party(victim.0, victim.1),
            victim_char: 1000 + kill_id,
            victim_pilot: victim.2.to_string(),
            victim_ship: victim.3,
            attackers: vec![Attacker {
                party: party(killer.0, killer.1),
                char_id: 2000 + kill_id,
                ship: killer.3,
                pilot: killer.2.to_string(),
                final_blow: fb,
            }],
            isk: 100_000_000.0,
            anchored: true,
        }
    }

    fn doc(title: Option<&str>) -> BattleReportDoc {
        let red = (100, "Red Alliance");
        let blue = (200, "Blue Alliance");
        let engs = vec![
            eng(1, 0, (red.0, red.1, "RedPilot", 587), (blue.0, blue.1, "BluePilot", 588), true),
            eng(2, 30, (blue.0, blue.1, "BlueGuy", 588), (red.0, red.1, "RedGuy", 587), true),
            eng(3, 90, (red.0, red.1, "RedTwo", 587), (blue.0, blue.1, "BlueTwo", 588), false),
        ];
        let battle = br_core::battle::preview_battle(engs.clone(), BATTLE_BREAK_SECS);
        let ship_names = [(587, "Rifter".to_string()), (588, "Rupture".to_string())].into();
        let affiliations = [
            (
                1001i64,
                Affil {
                    corp_id: 5001,
                    corp_name: "Red Corp".into(),
                    alliance_id: 100,
                    alliance_name: "Red Alliance".into(),
                },
            ),
            (
                2002i64,
                Affil {
                    corp_id: 5002,
                    corp_name: "Blue Corp".into(),
                    alliance_id: 0,
                    alliance_name: String::new(),
                },
            ),
        ]
        .into();
        BattleReportDoc::new(
            battle,
            engs,
            Overrides::default(),
            title.map(|t| t.into()),
            1_700_000_000,
            ship_names,
            affiliations,
        )
    }

    fn card_data(title: Option<&str>, uploader: &str) -> CardData {
        CardData { id: "AbCd123456".into(), doc: doc(title), uploader: uploader.into(), views: 42 }
    }

    #[test]
    fn card_shows_key_facts_and_links() {
        let html = card(&card_data(Some("Jita Brawl"), "Scout One")).into_string();
        assert!(html.contains("Jita Brawl"));
        assert!(html.contains("Jita"));
        assert!(html.contains("Red Alliance") && html.contains("Blue Alliance"));
        assert!(html.contains("ISK"));
        assert!(html.contains("Scout One"));
        assert!(html.contains("42 views"));
        assert!(html.contains("/br/AbCd123456"));
    }

    #[test]
    fn viewer_shows_facts_icon_and_download() {
        let html = viewer_page(&card_data(Some("Big Fight"), "Uploader X")).into_string();
        assert!(html.contains("Big Fight"));
        assert!(html.contains("Jita"));
        assert!(html.contains("Red Alliance") && html.contains("Blue Alliance"));
        assert!(html.contains("ISK destroyed"));
        assert!(html.contains("https://images.evetech.net/types/587/icon?size=64"));
        assert!(html.contains("loading=\"lazy\""));
        assert!(html.contains("/api/br/AbCd123456.json"));
        assert!(html.contains("Copy link"));
        assert!(!html.to_lowercase().contains(">delete<"));
    }

    #[test]
    fn malicious_title_is_escaped() {
        let evil = "<script>alert(1)</script>";
        let data = CardData {
            id: "Xx00000000".into(),
            doc: doc(Some(evil)),
            uploader: "<b>hax</b>".into(),
            views: 0,
        };
        let card_html = card(&data).into_string();
        let view_html = viewer_page(&data).into_string();
        for html in [&card_html, &view_html] {
            assert!(!html.contains("<script>alert(1)</script>"));
            assert!(html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
            assert!(!html.contains("<b>hax</b>"));
            assert!(html.contains("&lt;b&gt;hax&lt;/b&gt;"));
        }
    }

    #[test]
    fn malicious_side_name_is_escaped() {
        let engs = vec![
            eng(1, 0, (100, "<img src=x onerror=alert(1)>", "V", 587), (200, "Blue", "K", 588), true),
            eng(2, 20, (200, "Blue", "V2", 588), (100, "<img src=x onerror=alert(1)>", "K2", 587), true),
        ];
        let battle = br_core::battle::preview_battle(engs.clone(), BATTLE_BREAK_SECS);
        let d = BattleReportDoc::new(battle, engs, Overrides::default(), None, 1_700_000_000, Default::default(), Default::default());
        let data = CardData { id: "Yy11111111".into(), doc: d, uploader: "u".into(), views: 1 };
        let html = viewer_page(&data).into_string();
        assert!(!html.contains("<img src=x onerror=alert(1)>"));
        assert!(html.contains("&lt;img src=x onerror=alert(1)&gt;"));
    }

    #[test]
    fn derived_title_when_absent() {
        let html = card(&card_data(None, "u")).into_string();
        assert!(html.contains("Jita - "));
    }

    #[test]
    fn directory_filters_prefilled_and_pagination_links() {
        let q = DirQuery {
            system: "Jita".into(),
            participant: "Red Alliance".into(),
            sort: "isk".into(),
            ..Default::default()
        };
        let cards = vec![card_data(Some("A"), "u")];
        let html = directory_page(&cards, &q, 2, true).into_string();
        assert!(html.contains("value=\"Jita\""));
        assert!(html.contains("value=\"Red Alliance\""));
        assert!(html.contains("system=Jita"));
        assert!(html.contains("participant=Red%20Alliance"));
        assert!(html.contains("page=3"));
        assert!(html.contains("page=1"));
    }

    #[test]
    fn not_found_is_themed() {
        let html = not_found_page().into_string();
        assert!(html.contains("Report not found"));
        assert!(html.contains("EVE"));
        assert!(html.contains("/br"));
    }

    #[test]
    fn viewer_renders_both_layouts_and_toggle() {
        let html = viewer_page(&card_data(Some("Layouts"), "u")).into_string();
        assert!(html.contains("data-mode=\"tiles\""));
        assert!(html.contains("data-mode=\"details\""));
        assert!(html.contains("class=\"report view-tiles\""));
        assert!(html.contains("class=\"roster\""));
        assert!(html.contains("class=\"details\""));
        assert!(html.contains("data-ship=\"587\""));
        assert!(html.contains("data-char="));
        assert!(html.contains("All battle reports"));
        assert!(html.contains("href=\"/br\""));
    }

    #[test]
    fn details_view_has_zkill_links_and_involvement_maps() {
        let html = viewer_page(&card_data(Some("Kills"), "u")).into_string();
        assert!(html.contains("https://zkillboard.com/kill/1/"));
        assert!(html.contains("rel=\"noopener\""));
        assert!(html.contains("data-kill=\"1\""));
        assert!(html.contains("id=\"inv-data\""));
        assert!(html.contains("\"ka\"") && html.contains("\"ck\""));
        assert!(!html.contains("&quot;ka&quot;"));
    }

    #[test]
    fn malicious_pilot_and_party_escaped_in_details() {
        let evil = "<script>alert(1)</script>";
        let engs = vec![
            eng(1, 0, (100, "Red", evil, 587), (200, "Blue", "K", 588), true),
            eng(2, 20, (200, "Blue", "V2", 588), (100, "Red", "K2", 587), true),
        ];
        let battle = br_core::battle::preview_battle(engs.clone(), BATTLE_BREAK_SECS);
        let d = BattleReportDoc::new(battle, engs, Overrides::default(), None, 1_700_000_000, Default::default(), Default::default());
        let data = CardData { id: "Zz22222222".into(), doc: d, uploader: "u".into(), views: 1 };
        let html = viewer_page(&data).into_string();
        assert!(!html.contains("<script>alert(1)</script>"));
        assert!(html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
    }

    #[test]
    fn dominant_icons_capped_and_thresholded() {
        let battle = &doc(Some("Dom")).battle;
        for i in 0..battle.sides.len() {
            let doms = dominant_parties(battle, i);
            assert!(doms.len() <= 3, "at most three logos");
            for (_, r) in &doms {
                assert!(*r > 0.10, "only parties above 10%");
            }
        }
        let html = viewer_page(&card_data(Some("Dom"), "u")).into_string();
        assert!(html.contains("images.evetech.net/alliances/100/logo"));
        assert!(html.contains("class=\"dom-logo\""));
    }

    #[test]
    fn hull_filter_ids_match_between_tiles_and_details() {
        let html = viewer_page(&card_data(Some("Filter"), "u")).into_string();
        for id in ["587", "588"] {
            let tile = format!("class=\"ship\" data-ship=\"{id}\"");
            let cell = format!("data-ship=\"{id}\"");
            assert!(html.contains(&tile), "tile carries data-ship={id}");
            assert!(html.matches(&cell).count() >= 2, "a detail cell also has data-ship={id}");
        }
        assert!(CSS.contains(".dcell.filtered-out{display:none;}"));
        assert!(CSS.contains(".filter-chip[hidden]{display:none;}"));
    }

    #[test]
    fn long_names_get_title_and_ellipsis() {
        let long = "Extremely Long Alliance Name That Would Otherwise Stretch The Container Far Too Wide";
        let engs = vec![
            eng(1, 0, (100, long, "PilotWithAVeryLongPilotNameIndeedYesQuiteLong", 587),
                (200, "Blue", "K", 588), true),
            eng(2, 20, (200, "Blue", "V2", 588), (100, long, "K2", 587), true),
        ];
        let battle = br_core::battle::preview_battle(engs.clone(), BATTLE_BREAK_SECS);
        let d = BattleReportDoc::new(battle, engs, Overrides::default(), None, 1_700_000_000, Default::default(), Default::default());
        let data = CardData { id: "Ll44444444".into(), doc: d, uploader: "u".into(), views: 1 };
        let html = viewer_page(&data).into_string();
        assert!(html.contains("class=\"dparty-name\""));
        assert!(html.contains(&format!("title=\"{long}\"")));
        assert!(CSS.contains(".dparty-name{min-width:0; overflow:hidden; text-overflow:ellipsis"));
        assert!(CSS.contains(".side-panel{min-width:0;"));
        assert!(CSS.contains(".bd-side{min-width:0;}"));
    }

    #[test]
    fn breakdown_modal_lists_parties_and_escapes_names() {
        let evil = "<b>Sneaky</b> Alliance";
        let engs = vec![
            eng(1, 0, (100, evil, "RedA", 587), (200, "Blue", "BlueA", 588), true),
            eng(2, 20, (100, evil, "RedB", 587), (200, "Blue", "BlueB", 588), true),
            eng(3, 40, (200, "Blue", "BlueC", 588), (100, evil, "RedC", 587), true),
        ];
        let battle = br_core::battle::preview_battle(engs.clone(), BATTLE_BREAK_SECS);
        let d = BattleReportDoc::new(battle, engs, Overrides::default(), None, 1_700_000_000, Default::default(), Default::default());
        let data = CardData { id: "Mm33333333".into(), doc: d, uploader: "u".into(), views: 1 };
        let html = viewer_page(&data).into_string();
        assert!(html.contains("id=\"breakdown-modal\""));
        assert!(html.contains("class=\"bd-list\""));
        assert!(html.contains("class=\"bd-count\""));
        assert!(!html.contains("<b>Sneaky</b> Alliance"));
        assert!(html.contains("&lt;b&gt;Sneaky&lt;/b&gt; Alliance"));
    }

    #[test]
    fn tiles_show_ship_names_lossbar_and_isk_colors() {
        let html = viewer_page(&card_data(Some("Hulls"), "u")).into_string();
        assert!(html.contains("class=\"ship-name\""));
        assert!(html.contains("Rifter") && html.contains("Rupture"));
        // Loss bar instead of "x lost / y survived" text and no killing-blow marker.
        assert!(html.contains("class=\"lossbar\""));
        assert!(html.contains("lossbar-fill"));
        assert!(!html.contains("KB "));
        assert!(!html.contains("killing blows"));
        assert!(!html.contains('★'));
        assert!(html.contains("isk-destroyed"));
        assert!(html.contains("isk-lost"));
        assert!(CSS.contains(".stat-num.isk-destroyed{color:var(--green);}"));
        assert!(CSS.contains(".stat-num.isk-lost{color:var(--red);}"));
        assert!(!html.contains("dcell lost dpod-row"));
    }

    fn eng_full(
        kill_id: i64,
        victim_party: (i64, &str),
        vchar: i64,
        vpilot: &str,
        vship: i64,
        killer_party: (i64, &str),
        isk: f64,
    ) -> Engagement {
        Engagement {
            kill_id,
            time: kill_id * 10,
            system_id: 30000142,
            system_name: "Jita".to_string(),
            security: 0.9,
            victim: party(victim_party.0, victim_party.1),
            victim_char: vchar,
            victim_pilot: vpilot.to_string(),
            victim_ship: vship,
            attackers: vec![Attacker {
                party: party(killer_party.0, killer_party.1),
                char_id: 9000 + kill_id,
                ship: 588,
                pilot: "K".to_string(),
                final_blow: true,
            }],
            isk,
            anchored: true,
        }
    }

    #[test]
    fn pods_excluded_from_tiles_but_pod_kill_shown_in_details() {
        use br_core::battle::POD_TYPES;
        let engs = vec![
            eng_full(1, (100, "Red"), 5000, "RedVictim", 587, (200, "Blue"), 100_000_000.0),
            eng_full(2, (100, "Red"), 5000, "RedVictim", 670, (200, "Blue"), 50_000_000.0),
            eng_full(3, (200, "Blue"), 6000, "BlueVictim", 588, (100, "Red"), 80_000_000.0),
        ];
        let battle = br_core::battle::preview_battle(engs.clone(), BATTLE_BREAK_SECS);
        let ship_names = [(587, "Rifter".to_string()), (588, "Rupture".to_string())].into();
        let d = BattleReportDoc::new(battle, engs, Overrides::default(), None, 1_700_000_000, ship_names, Default::default());
        let data = CardData { id: "Pp55555555".into(), doc: d, uploader: "u".into(), views: 1 };
        let html = viewer_page(&data).into_string();
        for pod in POD_TYPES {
            assert!(!html.contains(&format!("data-ship=\"{pod}\"")), "pod {pod} must not be a tile");
        }
        assert!(html.contains("dcell lost dpod-row"));
        assert!(html.contains(&icon_url(670)));
        assert!(html.contains("https://zkillboard.com/kill/1/"));
        assert!(html.contains("https://zkillboard.com/kill/2/"));
    }

    #[test]
    fn pod_row_without_known_kill_id_omits_zkill_link() {
        let engs = vec![
            eng_full(1, (100, "Red"), 5000, "RedVictim", 587, (200, "Blue"), 100_000_000.0),
            eng_full(2, (100, "Red"), 0, "RedVictim", 670, (200, "Blue"), 50_000_000.0),
            eng_full(3, (200, "Blue"), 6000, "BlueVictim", 588, (100, "Red"), 80_000_000.0),
        ];
        let battle = br_core::battle::preview_battle(engs.clone(), BATTLE_BREAK_SECS);
        let d = BattleReportDoc::new(battle, engs, Overrides::default(), None, 1_700_000_000, Default::default(), Default::default());
        let data = CardData { id: "Rr77777777".into(), doc: d, uploader: "u".into(), views: 1 };
        let html = viewer_page(&data).into_string();
        assert!(html.contains("dcell lost dpod-row"));
        assert!(html.contains(&icon_url(670)));
        assert!(!html.contains("https://zkillboard.com/kill/2/"));
    }

    #[test]
    fn small_pod_value_shows_nothing_pod_related() {
        let engs = vec![
            eng_full(1, (100, "Red"), 5000, "RedVictim", 587, (200, "Blue"), 100_000_000.0),
            eng_full(2, (100, "Red"), 5000, "RedVictim", 670, (200, "Blue"), 10_000.0),
            eng_full(3, (200, "Blue"), 6000, "BlueVictim", 588, (100, "Red"), 80_000_000.0),
        ];
        let battle = br_core::battle::preview_battle(engs.clone(), BATTLE_BREAK_SECS);
        let d = BattleReportDoc::new(battle, engs, Overrides::default(), None, 1_700_000_000, Default::default(), Default::default());
        let data = CardData { id: "Qq66666666".into(), doc: d, uploader: "u".into(), views: 1 };
        let html = viewer_page(&data).into_string();
        assert!(!html.contains("dcell lost dpod-row"));
    }

    #[test]
    fn no_em_dashes_in_rendered_output() {
        let view = viewer_page(&card_data(None, "u")).into_string();
        let card_html = card(&card_data(Some("T"), "u")).into_string();
        let dir = directory_page(&[card_data(None, "u")], &DirQuery::default(), 1, false).into_string();
        let nf = not_found_page().into_string();
        for html in [&view, &card_html, &dir, &nf] {
            assert!(!html.contains('\u{2014}'), "no em dash in rendered output");
        }
    }

    #[test]
    fn no_coalition_side_still_renders_subtitle_placeholder() {
        let html = viewer_page(&card_data(Some("Align"), "u")).into_string();
        assert!(html.contains("class=\"coalition-members\" aria-hidden=\"true\""));
        assert!(html.matches("class=\"coalition-members\"").count() >= 2);
        assert!(CSS.contains(".coalition-members{") && CSS.contains("min-height:1.6em"));
    }

    #[test]
    fn sides_data_json_is_present_and_well_formed() {
        let html = viewer_page(&card_data(None, "u")).into_string();
        assert!(html.contains("id=\"sides-data\""));
        let j = sides_data_json(&doc(None));
        let v: serde_json::Value = serde_json::from_str(&j).unwrap();
        assert!(v["sides_count"].as_u64().unwrap() >= 2);
        let parties = v["parties"].as_array().unwrap();
        let red = parties.iter().find(|p| p["id"] == 100).expect("party 100 present");
        assert_eq!(red["kind"], "alliance");
        assert!(red["side"].as_u64().is_some());
        let parts = v["participants"].as_array().unwrap();
        assert!(parts.iter().all(|p| p["char"].as_i64().is_some()));
        assert!(parts.iter().any(|p| p["party_id"] == 100));
        let engs = v["engagements"].as_array().unwrap();
        assert!(engs.iter().all(|e| e["kill_id"].as_i64().is_some()
            && e["attacker_party_ids"].is_array()));
    }

    #[test]
    fn sides_data_escapes_markup_in_names() {
        let engs = vec![
            eng(1, 0, (100, "</script><b>x", "V", 587), (200, "Blue", "K", 588), true),
            eng(2, 20, (200, "Blue", "V2", 588), (100, "</script><b>x", "K2", 587), true),
        ];
        let battle = br_core::battle::preview_battle(engs.clone(), BATTLE_BREAK_SECS);
        let d = BattleReportDoc::new(battle, engs, Overrides::default(), None, 1_700_000_000, Default::default(), Default::default());
        let data = CardData { id: "Ss88888888".into(), doc: d, uploader: "u".into(), views: 1 };
        let html = viewer_page(&data).into_string();
        let blob = sides_data_json(&data.doc);
        assert!(!blob.contains("</script>"));
        assert!(blob.contains("\\u003c"));
        assert!(!html.contains("</script><b>x"));
    }

    #[test]
    fn party_logos_and_names_link_to_zkill() {
        let html = viewer_page(&card_data(Some("Z"), "u")).into_string();
        assert!(html.contains("https://zkillboard.com/alliance/100/"));
        assert!(html.contains("class=\"dom-logo-link\""));
        assert!(html.contains("class=\"bd-entity\""));
        assert!(html.contains("class=\"vs-emblem\""));

        let corp = Party { id: 300, name: "Corp X".to_string(), kind: PartyKind::Corporation };
        let ally = party(100, "Red Alliance");
        let mk = |kill_id: i64, victim: &Party, vchar: i64, vship: i64, killer: &Party| Engagement {
            kill_id,
            time: kill_id * 10,
            system_id: 30000142,
            system_name: "Jita".to_string(),
            security: 0.9,
            victim: victim.clone(),
            victim_char: vchar,
            victim_pilot: "P".to_string(),
            victim_ship: vship,
            attackers: vec![Attacker {
                party: killer.clone(),
                char_id: 9000 + kill_id,
                ship: 588,
                pilot: "K".to_string(),
                final_blow: true,
            }],
            isk: 100_000_000.0,
            anchored: true,
        };
        let engs = vec![mk(1, &ally, 1001, 587, &corp), mk(2, &corp, 1002, 588, &ally)];
        let battle = br_core::battle::preview_battle(engs.clone(), BATTLE_BREAK_SECS);
        let d = BattleReportDoc::new(battle, engs, Overrides::default(), None, 1_700_000_000, Default::default(), Default::default());
        let data = CardData { id: "Cc99999999".into(), doc: d, uploader: "u".into(), views: 1 };
        let html = viewer_page(&data).into_string();
        assert!(html.contains("https://zkillboard.com/corporation/300/"));
        let v: serde_json::Value = serde_json::from_str(&sides_data_json(&data.doc)).unwrap();
        assert!(v["parties"].as_array().unwrap().iter().any(|p| p["id"] == 300 && p["kind"] == "corp"));
    }

    #[test]
    fn details_render_pilot_corp_and_alliance_icons() {
        let engs = vec![
            eng(1, 0, (100, "Red Alliance", "Pilot A", 587), (200, "Blue Alliance", "Killer A", 588), true),
            eng(2, 30, (100, "Red Alliance", "Pilot B", 587), (200, "Blue Alliance", "Killer B", 588), true),
            eng(3, 60, (200, "Blue Alliance", "BlueV", 588), (100, "Red Alliance", "RedK", 587), true),
        ];
        let battle = br_core::battle::preview_battle(engs.clone(), BATTLE_BREAK_SECS);
        let affiliations = [
            (
                1001i64,
                Affil {
                    corp_id: 98001,
                    corp_name: "Alpha Corp".into(),
                    alliance_id: 99001,
                    alliance_name: "Alpha Alliance".into(),
                },
            ),
            (
                1002i64,
                Affil {
                    corp_id: 98002,
                    corp_name: "Bravo Corp".into(),
                    alliance_id: 0,
                    alliance_name: String::new(),
                },
            ),
        ]
        .into();
        let d = BattleReportDoc::new(
            battle,
            engs,
            Overrides::default(),
            None,
            1_700_000_000,
            Default::default(),
            affiliations,
        );
        let data = CardData { id: "Aa10101010".into(), doc: d, uploader: "u".into(), views: 1 };
        let html = viewer_page(&data).into_string();

        assert!(html.contains("https://images.evetech.net/corporations/98001/logo?size=32"));
        assert!(html.contains("https://zkillboard.com/corporation/98001/"));
        assert!(html.contains("https://images.evetech.net/alliances/99001/logo?size=32"));
        assert!(html.contains("https://zkillboard.com/alliance/99001/"));
        let corp_at = html.find("corporations/98001").unwrap();
        let ally_at = html.find("alliances/99001").unwrap();
        assert!(corp_at < ally_at, "corp logo must precede alliance logo");
        assert!(html.contains("https://zkillboard.com/character/1001/"));

        assert!(html.contains("https://images.evetech.net/corporations/98002/logo?size=32"));
        assert!(!html.contains("alliances/99002"));
        assert!(!html.contains("alliances/0/"));
        assert!(html.contains("class=\"affil-logo\""));
    }
}
