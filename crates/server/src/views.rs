//! Server-rendered, mobile-first HTML: the public report directory (`GET /br`) and
//! the single-report viewer (`GET /br/{id}`).
//!
//! Built with [`maud`] — compile-time templates that auto-escape every interpolation.
//! Every user- or EVE-supplied string (titles, uploader names, side/alliance/pilot
//! names) is rendered through a normal `(value)` interpolation, never via
//! [`PreEscaped`], so none of them can inject markup. `PreEscaped` is used *only* for
//! the static, developer-authored CSS/JS blobs in [`layout`].

use br_core::battle::{Battle, BattleReportDoc, Party, PartyKind, Side};
use maud::{html, Markup, PreEscaped, DOCTYPE};

/// EVE ship/structure icon for a type id (64 px). Empty markup for type id 0.
fn icon_url(type_id: i64) -> String {
    format!("https://images.evetech.net/types/{type_id}/icon?size=64")
}

/// zKillboard permalink for a killmail.
fn zkill_url(kill_id: i64) -> String {
    format!("https://zkillboard.com/kill/{kill_id}/")
}

/// Alliance/corporation logo URL for a party (32 px). `None` for characters, factions,
/// unknowns, or a 0 id — those have no entity logo.
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

/// Inline party logo (alliance/corp) with the party name as accessible label. Empty for
/// parties with no logo.
fn party_logo(p: &Party) -> Markup {
    html! {
        @if let Some(url) = party_logo_url(p) {
            img .party-logo src=(url) width="20" height="20" loading="lazy" alt=(p.name) title=(p.name);
        }
    }
}

/// A side's composition by entity: every participating Alliance/Corporation identity with
/// its participant count and share of the side. Returns `(total_roster, entries)` where
/// `entries` is `(party, count, ratio)` sorted by count desc (stable tie-break by id). The
/// denominator is the *full* roster, so shares sum to ≤ 1 (characters/factions, which have
/// no entity logo, are excluded from the entries but still counted in the total).
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

/// Parties that make up more than 10% of a side's roster, as `(party, ratio)`, biggest
/// first, at most three — the logos shown inline in the side header.
fn dominant_parties(battle: &Battle, side_idx: usize) -> Vec<(Party, f64)> {
    let (_, entries) = side_breakdown(battle, side_idx);
    entries.into_iter().filter(|(_, _, r)| *r > 0.10).map(|(p, _, r)| (p, r)).take(3).collect()
}

/// One report row's render inputs: its public id, the stored doc, uploader, view count.
pub struct CardData {
    pub id: String,
    pub doc: BattleReportDoc,
    pub uploader: String,
    pub views: i64,
}

/// Current directory filter/sort state, used both to pre-fill the form and to build
/// shareable pagination links. All fields are the raw query-string values.
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
    /// `/br?...` for a given page, carrying the current filters. Values are
    /// percent-encoded so a name with spaces/`&` stays a single, valid parameter.
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

/// Minimal percent-encoding for a query-string value: keep the RFC 3986 unreserved
/// set, encode everything else. Avoids pulling in a URL crate for one job.
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

/// Security-status colour band, echoing EVE's map palette: high-sec blue/green,
/// low-sec amber, null/anom red.
fn sec_color(sec: f64) -> &'static str {
    if sec >= 0.5 {
        "#4fc3a1"
    } else if sec > 0.0 {
        "#e8a13a"
    } else {
        "#e04c4c"
    }
}

/// `1.23B`, `345.0M`, `12k`, `900` — compact ISK with a unit suffix.
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

/// `1h 23m`, `7m`, `<1m`.
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

/// `2024-01-02 14:05 UTC`, or `—` for an out-of-range timestamp.
fn fmt_time(unix: i64) -> String {
    chrono::DateTime::from_timestamp(unix, 0)
        .map(|t| t.format("%Y-%m-%d %H:%M UTC").to_string())
        .unwrap_or_else(|| "—".to_string())
}

/// ISK-efficiency percentage as `73%`, or `—` when no ISK was exchanged.
fn fmt_eff(side: &Side) -> String {
    side.isk_efficiency().map(|e| format!("{e:.0}%")).unwrap_or_else(|| "—".to_string())
}

/// A side's display name: its coalition, else its most-involved party, else `Unknown`.
/// Mirrors the `side_names` extraction in `pipeline::extract_columns`.
fn side_label(side: &Side) -> String {
    side.coalition
        .clone()
        .or_else(|| side.parties.first().map(|p| p.name.clone()))
        .unwrap_or_else(|| "Unknown".to_string())
}

/// Title to show for a battle: the human title if present, else a derived
/// `System — date` (first system + start date).
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
        format!("{system} — {date}")
    }
}

/// Inline systems chip list with per-system security colour.
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

/// Two-side ISK-efficiency bar pair (top two sides), used on directory cards.
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

/// One directory card, linking to `/br/{id}`.
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

/// The directory page: filter form (pre-filled, GET, shareable) + cards + pagination.
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
    layout("Battle reports — EVE Spai", body, Some(DIRECTORY_JS))
}

/// Per-ship-type aggregation of a side's roster.
struct ShipGroup {
    ship: i64,
    lost: usize,
    survived: usize,
    final_blows: usize,
}

/// Group a side's roster by hull type (mirrors `Battle::roster(i)`, which already folds
/// pods and dedups survivors): lost vs survived counts, plus final-blow tallies derived
/// from the battle's engagements for that side. Heaviest-hit hulls first.
fn roster_groups(battle: &Battle, side_idx: usize) -> Vec<ShipGroup> {
    use std::collections::HashMap;
    let roster = battle.roster(side_idx);

    // Final blows landed by this side, counted per attacker ship type.
    let mut fb_by_ship: HashMap<i64, usize> = HashMap::new();
    for e in &battle.engagements {
        for a in &e.attackers {
            if a.final_blow && battle.side_of(&a.party) == Some(side_idx) {
                *fb_by_ship.entry(a.ship).or_default() += 1;
            }
        }
    }

    let mut by_ship: HashMap<i64, (usize, usize)> = HashMap::new();
    for p in &roster {
        let entry = by_ship.entry(p.ship).or_insert((0, 0));
        if p.lost.is_some() {
            entry.0 += 1;
        } else {
            entry.1 += 1;
        }
    }
    let mut groups: Vec<ShipGroup> = by_ship
        .into_iter()
        .map(|(ship, (lost, survived))| ShipGroup {
            ship,
            lost,
            survived,
            final_blows: fb_by_ship.get(&ship).copied().unwrap_or(0),
        })
        .collect();
    // Most losses first, then most ships, then a stable type-id order.
    groups.sort_by(|a, b| {
        b.lost
            .cmp(&a.lost)
            .then((b.lost + b.survived).cmp(&(a.lost + a.survived)))
            .then(a.ship.cmp(&b.ship))
    });
    groups
}

/// One participant row for the Details layout: hull, pilot, party (with logo), and — for a
/// destroyed ship — a red marker, ISK value, and zKill link. `data-*` attributes drive the
/// hover-highlight and hull-filter JS (`data-char`, `data-ship`, and `data-kill` on losses).
fn detail_cell(p: &br_core::battle::Participant) -> Markup {
    let lost = p.lost.as_ref();
    html! {
        div .dcell .lost[lost.is_some()]
            data-char=(p.char_id)
            data-ship=(p.ship)
            data-kill=[lost.map(|l| l.kill_id)]
        {
            img .dhull src=(icon_url(p.ship)) width="40" height="40" loading="lazy" alt="ship";
            div .dinfo {
                span .dpilot title=(p.pilot) { (p.pilot) }
                span .dparty {
                    (party_logo(&p.party))
                    span .dparty-name title=(p.party.name) { (p.party.name) }
                }
            }
            @if let Some(l) = lost {
                div .dloss {
                    span .destroyed { "destroyed" }
                    span .dval { (fmt_isk(l.value + l.pod_value)) }
                    a .zk href=(zkill_url(l.kill_id)) target="_blank" rel="noopener" { "zKill" }
                }
            }
        }
    }
}

/// One side's full panel: header (name + dominant logos + overflow chip), stats, and both
/// the Tiles (grouped hulls) and Details (per-pilot) layouts. Only one layout is visible at
/// a time, switched by the report-level `view-tiles`/`view-details` class.
fn side_panel(battle: &Battle, side_idx: usize) -> Markup {
    let side = &battle.sides[side_idx];
    let groups = roster_groups(battle, side_idx);
    let pilots: usize = groups.iter().map(|g| g.lost + g.survived).sum();
    let doms = dominant_parties(battle, side_idx);
    let (_, breakdown) = side_breakdown(battle, side_idx);
    // Distinct alliance/corp entities beyond the (≤3) shown inline.
    let overflow = breakdown.len().saturating_sub(doms.len());
    let roster = battle.roster(side_idx);
    html! {
        div .side-panel {
            div .side-head {
                div .side-title {
                    h3 title=(side_label(side)) { (side_label(side)) }
                    @if !doms.is_empty() {
                        span .dom-logos {
                            @for (p, r) in &doms {
                                @if let Some(url) = party_logo_url(p) {
                                    @let label = format!("{} — {:.0}%", p.name, r * 100.0);
                                    img .dom-logo src=(url) width="26" height="26" loading="lazy"
                                        alt=(label) title=(label);
                                }
                            }
                            @if overflow > 0 {
                                button type="button" .more-chip data-side=(side_idx)
                                    title="Show full breakdown" { "+" (overflow) }
                            }
                        }
                    }
                }
                @if let Some(coalition) = &side.coalition {
                    @if !side.parties.is_empty() {
                        @let cm = format!("{} — {} parties", coalition, side.parties.len());
                        div .coalition-members title=(cm) { (cm) }
                    }
                }
            }
            div .side-stats {
                div .stat { span .stat-num { (pilots) } span .stat-label { "pilots" } }
                div .stat { span .stat-num { (side.kills) } span .stat-label { "kills" } }
                div .stat { span .stat-num { (side.losses) } span .stat-label { "losses" } }
                div .stat { span .stat-num { (fmt_isk(side.isk_destroyed)) } span .stat-label { "ISK destroyed" } }
                div .stat { span .stat-num { (fmt_isk(side.isk_lost)) } span .stat-label { "ISK lost" } }
                div .stat { span .stat-num { (fmt_eff(side)) } span .stat-label { "efficiency" } }
            }
            div .bar { div .bar-fill style=(format!("width:{:.0}%", side.isk_efficiency().unwrap_or(0.0))) {} }
            div .roster {
                @for g in &groups {
                    div .ship data-ship=(g.ship) title="Show these pilots" {
                        img .ship-icon src=(icon_url(g.ship)) width="48" height="48" loading="lazy" alt="ship";
                        div .ship-counts {
                            @if g.lost > 0 { span .lost { (g.lost) " lost" } }
                            @if g.survived > 0 { span .survived { (g.survived) " survived" } }
                            @if g.final_blows > 0 { span .fb title="final blows" { "★ " (g.final_blows) } }
                        }
                    }
                }
            }
            div .details {
                @for p in &roster {
                    (detail_cell(p))
                }
            }
        }
    }
}

/// One side's entry in the breakdown modal: heading + every alliance/corp with logo, name,
/// participant count, and share, biggest first.
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
                        li .bd-row {
                            (party_logo(p))
                            span .bd-name title=(p.name) { (p.name) }
                            span .bd-count { (count) }
                            span .bd-pct { (format!("{:.0}%", r * 100.0)) }
                        }
                    }
                }
            }
        }
    }
}

/// The "emblem A vs B" strip at the top of a report: each side's single most-represented
/// alliance/corp logo, clickable to open that side's breakdown. Skipped if no side has a
/// logo-bearing entity.
fn versus_strip(battle: &Battle) -> Markup {
    let emblems: Vec<(usize, Party)> = (0..battle.sides.len())
        .filter_map(|i| side_breakdown(battle, i).1.into_iter().next().map(|(p, _, _)| (i, p)))
        .collect();
    html! {
        @if !emblems.is_empty() {
            div .versus {
                @for (n, (side_idx, p)) in emblems.iter().enumerate() {
                    @if n > 0 { span .vs-sep { "vs" } }
                    button type="button" .vs-emblem data-side=(side_idx) title=(p.name) {
                        @if let Some(url) = party_logo_url(p) {
                            img src=(url) width="40" height="40" loading="lazy" alt=(p.name);
                        }
                        span .vs-name { (side_label(&battle.sides[*side_idx])) }
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

/// The single-report viewer page.
pub fn viewer_page(data: &CardData) -> Markup {
    let b = &data.doc.battle;
    let json_href = format!("/api/br/{}.json", data.id);
    let inv = involvement_json(b);
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
                    span { (fmt_time(b.start)) " – " (fmt_time(b.end)) }
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
            div .view-toggle role="group" aria-label="Layout" {
                button type="button" .seg data-mode="tiles" aria-pressed="true" { "Tiles" }
                button type="button" .seg data-mode="details" aria-pressed="false" { "Details" }
            }
            div .report .view-tiles {
                div .filter-chip hidden {
                    img width="24" height="24" alt="hull";
                    span .fc-label { "Showing only this hull" }
                    button type="button" .btn .fc-clear { "Clear filter" }
                }
                div .panels {
                    @for i in 0..b.sides.len() {
                        (side_panel(b, i))
                    }
                }
            }
            // Hover-highlight maps (integer ids only — safe to emit verbatim).
            script type="application/json" #inv-data { (PreEscaped(inv)) }
            // Per-side alliance/corp breakdown, hidden until opened from a "+N" chip or emblem.
            div .modal #breakdown-modal hidden {
                div .modal-overlay {}
                div .modal-panel role="dialog" aria-modal="true" aria-label="Side breakdown" {
                    button type="button" .modal-close aria-label="Close" { "×" }
                    h3 { "Composition" }
                    div .bd-sides {
                        @for i in 0..b.sides.len() {
                            (breakdown_section(b, i))
                        }
                    }
                }
            }
        }
    };
    layout(&format!("{} — EVE Spai", display_title(&data.doc)), body, Some(VIEWER_JS))
}

/// Themed 404, used when a report is missing or an unlisted one is requested without it.
pub fn not_found_page() -> Markup {
    let body = html! {
        section .notfound {
            h2 { "Report not found" }
            p { "This battle report does not exist, or it is unlisted and only reachable by its direct link." }
            a .btn .primary href="/br" { "Browse battle reports" }
        }
    };
    layout("Not found — EVE Spai", body, None)
}

/// Shared base template: viewport, inlined theme CSS, branded header, centred column.
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
                footer { p { "EVE Spai — battle reports. EVE Online and all related assets are property of Fenris Creations." } }
                @if let Some(js) = script {
                    script { (PreEscaped(js)) }
                }
            }
        }
    }
}

/// Tiny, data-free auto-submit helper for the filter form (no SPA, no framework).
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

/// Viewer interactions, all reading data already in the DOM (no API round-trips):
/// copy-link, the Tiles/Details toggle, the hull-filter chip, the involvement hover
/// highlight (driven by the embedded #inv-data integer maps), and the breakdown modal.
const VIEWER_JS: &str = r#"
(function(){
  // Copy current URL to clipboard.
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

  // --- Hull filter: click a tile -> Details, only that hull ---
  Array.prototype.slice.call(document.querySelectorAll('.ship[data-ship]')).forEach(function(tile){
    tile.addEventListener('click',function(){
      var ship=tile.getAttribute('data-ship');
      setMode('details');
      cells.forEach(function(c){
        c.classList.toggle('filtered-out', c.getAttribute('data-ship')!==ship);
      });
      if(chip){
        chip.hidden=false;
        var img=chip.querySelector('img');
        if(img){ img.src='https://images.evetech.net/types/'+ship+'/icon?size=64'; }
      }
    });
  });
  if(chip){ var cb=chip.querySelector('.fc-clear'); if(cb){ cb.addEventListener('click',clearFilter); } }

  // --- Breakdown modal ---
  var modal=document.getElementById('breakdown-modal');
  function openModal(){ if(modal){ modal.hidden=false; } }
  function closeModal(){ if(modal){ modal.hidden=true; } }
  Array.prototype.slice.call(document.querySelectorAll('.more-chip,.vs-emblem')).forEach(function(b){
    b.addEventListener('click',openModal);
  });
  if(modal){
    var ov=modal.querySelector('.modal-overlay'); if(ov){ ov.addEventListener('click',closeModal); }
    var x=modal.querySelector('.modal-close'); if(x){ x.addEventListener('click',closeModal); }
    document.addEventListener('keydown',function(e){ if(e.key==='Escape'){ closeModal(); } });
  }
})();
"#;

/// The full stylesheet, inlined into every page. Colour tokens are taken verbatim from
/// `site/index.html`; the rest are mobile-first layout rules for cards and rosters.
const CSS: &str = r#"
:root{
  --bg:#0c1117; --panel:#141a22; --panel-2:#171f29; --line:#243040;
  --blue:#4fc3f7; --blue-dim:#2b6c8c; --red:#e04c4c; --text:#e7eef5; --muted:#8ea0b2;
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
.coalition-members{color:var(--muted); font-size:12.5px; overflow:hidden; text-overflow:ellipsis; white-space:nowrap;}
.side-stats{display:grid; grid-template-columns:repeat(3,1fr); gap:10px; margin:12px 0;}
.stat{display:flex; flex-direction:column;}
.stat-num{font-size:16px; font-weight:600; font-family:var(--mono);}
.stat-label{color:var(--muted); font-size:11.5px; text-transform:uppercase; letter-spacing:0.4px;}
.roster{display:grid; grid-template-columns:repeat(auto-fit,minmax(78px,1fr)); gap:10px; margin-top:14px;}
.ship{display:flex; flex-direction:column; align-items:center; gap:4px; padding:4px; border-radius:8px; cursor:pointer;}
.ship:hover{background:var(--panel-2);}
.ship-icon{width:48px; height:48px; border-radius:8px; border:1px solid var(--line); background:var(--panel-2);}
.ship-counts{display:flex; flex-direction:column; align-items:center; font-size:11px; line-height:1.3;}
.ship-counts .lost{color:var(--red);}
.ship-counts .survived{color:var(--muted);}
.ship-counts .fb{color:var(--blue);}

/* Tiles / Details toggle and layout switching */
.view-toggle{display:inline-flex; margin-bottom:14px; border:1px solid var(--line); border-radius:10px; overflow:hidden;}
.view-toggle .seg{padding:8px 18px; font-size:14px; font-weight:600; background:var(--panel); color:var(--muted); border:none; border-right:1px solid var(--line); cursor:pointer; font-family:inherit;}
.view-toggle .seg:last-child{border-right:none;}
.view-toggle .seg[aria-pressed="true"]{background:var(--blue-dim); color:var(--text);}
.report.view-tiles .details{display:none;}
.report.view-details .roster{display:none;}

/* Versus strip — each side's top emblem */
.versus{display:flex; flex-wrap:wrap; align-items:center; gap:12px; margin:12px 0 4px;}
.vs-emblem{display:inline-flex; align-items:center; gap:8px; min-width:0; max-width:100%; padding:6px 10px; background:var(--panel-2); border:1px solid var(--line); border-radius:10px; color:var(--text); cursor:pointer; font-family:inherit; font-size:14px; font-weight:600;}
.vs-emblem:hover{border-color:var(--blue-dim);}
.vs-emblem img{flex:0 0 auto; border-radius:6px;}
.vs-name{min-width:0; max-width:160px; overflow:hidden; text-overflow:ellipsis; white-space:nowrap;}
.vs-sep{color:var(--muted); font-style:italic; font-size:13px;}

/* Dominant-party logos + overflow chip in side headers */
.side-title{display:flex; align-items:center; gap:8px; flex-wrap:wrap;}
.dom-logos{display:inline-flex; align-items:center; gap:5px;}
.dom-logo{border-radius:5px; border:1px solid var(--line); background:var(--panel-2);}
.more-chip{padding:3px 8px; font-size:12px; font-weight:600; color:var(--muted); background:var(--panel-2); border:1px solid var(--line); border-radius:8px; cursor:pointer; font-family:inherit;}
.more-chip:hover{border-color:var(--blue-dim); color:var(--text);}

/* Hull filter chip. The explicit [hidden] rule beats the author `display:flex` above —
   without it the boolean `hidden` attribute would not hide the chip. */
.filter-chip{display:flex; align-items:center; gap:10px; margin-bottom:14px; padding:8px 12px; background:var(--panel); border:1px solid var(--blue-dim); border-radius:10px; font-size:13px;}
.filter-chip[hidden]{display:none;}
.filter-chip img{flex:0 0 auto; border-radius:5px; border:1px solid var(--line);}
.filter-chip .fc-label{flex:1; min-width:0; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; color:var(--muted);}
.filter-chip .btn{flex:0 0 auto; padding:5px 11px; font-size:13px;}

/* Details layout — per-pilot cells */
.details{display:flex; flex-direction:column; gap:6px; margin-top:14px;}
.dcell{display:flex; align-items:center; gap:10px; padding:6px 8px; border:1px solid var(--line); border-radius:8px; background:var(--panel-2);}
.dcell.filtered-out{display:none;}
.dcell.lost{border-color:#5a2a2a;}
.dhull{flex:0 0 auto; border-radius:6px; border:1px solid var(--line); background:var(--panel);}
.dinfo{display:flex; flex-direction:column; min-width:0; flex:1;}
.dpilot{min-width:0; font-size:14px; overflow:hidden; text-overflow:ellipsis; white-space:nowrap;}
.dparty{display:flex; align-items:center; gap:5px; min-width:0; color:var(--muted); font-size:12.5px;}
.dparty-name{min-width:0; overflow:hidden; text-overflow:ellipsis; white-space:nowrap;}
.party-logo{flex:0 0 auto; border-radius:4px;}
.dloss{display:flex; align-items:center; gap:8px; flex:0 0 auto; font-size:12.5px;}
.destroyed{color:var(--red); font-size:11px; font-weight:600; text-transform:uppercase; letter-spacing:0.4px;}
.dval{font-family:var(--mono); color:var(--red);}
.zk{font-weight:600;}
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
.bd-list{list-style:none; margin:0; padding:0; display:flex; flex-direction:column; gap:5px;}
.bd-row{display:flex; align-items:center; gap:8px; min-width:0; font-size:13px;}
.bd-name{flex:1; min-width:0; overflow:hidden; text-overflow:ellipsis; white-space:nowrap;}
.bd-count{font-family:var(--mono);}
.bd-pct{font-family:var(--mono); color:var(--muted); min-width:38px; text-align:right;}
.bd-empty{color:var(--muted); font-size:13px;}
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
    use br_core::battle::{Attacker, Engagement, Overrides, Party, PartyKind, BATTLE_BREAK_SECS};

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
        BattleReportDoc::new(battle, engs, Overrides::default(), title.map(|t| t.into()), 1_700_000_000)
    }

    fn card_data(title: Option<&str>, uploader: &str) -> CardData {
        CardData { id: "AbCd123456".into(), doc: doc(title), uploader: uploader.into(), views: 42 }
    }

    #[test]
    fn card_shows_key_facts_and_links() {
        let html = card(&card_data(Some("Jita Brawl"), "Scout One")).into_string();
        assert!(html.contains("Jita Brawl"));
        assert!(html.contains("Jita")); // system
        assert!(html.contains("Red Alliance") && html.contains("Blue Alliance"));
        assert!(html.contains("ISK"));
        assert!(html.contains("Scout One"));
        assert!(html.contains("42 views"));
        assert!(html.contains("/br/AbCd123456")); // links to the viewer
    }

    #[test]
    fn viewer_shows_facts_icon_and_download() {
        let html = viewer_page(&card_data(Some("Big Fight"), "Uploader X")).into_string();
        assert!(html.contains("Big Fight"));
        assert!(html.contains("Jita"));
        assert!(html.contains("Red Alliance") && html.contains("Blue Alliance"));
        assert!(html.contains("ISK destroyed"));
        // Ship icon URL for the hulls in the fight.
        assert!(html.contains("https://images.evetech.net/types/587/icon?size=64"));
        assert!(html.contains("loading=\"lazy\""));
        // Download-JSON link points at the M3 JSON route.
        assert!(html.contains("/api/br/AbCd123456.json"));
        assert!(html.contains("Copy link"));
        // No delete control on the public page.
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
        // A side/alliance name with markup must be escaped wherever it is shown.
        let engs = vec![
            eng(1, 0, (100, "<img src=x onerror=alert(1)>", "V", 587), (200, "Blue", "K", 588), true),
            eng(2, 20, (200, "Blue", "V2", 588), (100, "<img src=x onerror=alert(1)>", "K2", 587), true),
        ];
        let battle = br_core::battle::preview_battle(engs.clone(), BATTLE_BREAK_SECS);
        let d = BattleReportDoc::new(battle, engs, Overrides::default(), None, 1_700_000_000);
        let data = CardData { id: "Yy11111111".into(), doc: d, uploader: "u".into(), views: 1 };
        let html = viewer_page(&data).into_string();
        assert!(!html.contains("<img src=x onerror=alert(1)>"));
        assert!(html.contains("&lt;img src=x onerror=alert(1)&gt;"));
    }

    #[test]
    fn derived_title_when_absent() {
        let html = card(&card_data(None, "u")).into_string();
        // Derives "System — date" from the battle.
        assert!(html.contains("Jita — "));
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
        // Form pre-filled from the query.
        assert!(html.contains("value=\"Jita\""));
        assert!(html.contains("value=\"Red Alliance\""));
        // Pagination keeps the filters (percent-encoded) and moves the page.
        assert!(html.contains("system=Jita"));
        assert!(html.contains("participant=Red%20Alliance"));
        assert!(html.contains("page=3")); // next
        assert!(html.contains("page=1")); // prev
    }

    #[test]
    fn not_found_is_themed() {
        let html = not_found_page().into_string();
        assert!(html.contains("Report not found"));
        assert!(html.contains("EVE")); // branded layout
        assert!(html.contains("/br"));
    }

    #[test]
    fn viewer_renders_both_layouts_and_toggle() {
        let html = viewer_page(&card_data(Some("Layouts"), "u")).into_string();
        // The Tiles/Details toggle, both layouts, and the report wrapper all render server-side.
        assert!(html.contains("data-mode=\"tiles\""));
        assert!(html.contains("data-mode=\"details\""));
        assert!(html.contains("class=\"report view-tiles\"")); // default = Tiles
        assert!(html.contains("class=\"roster\"")); // Tiles layout
        assert!(html.contains("class=\"details\"")); // Details layout
        // Tiles are clickable (carry their hull type id) and details cells carry hover ids.
        assert!(html.contains("data-ship=\"587\""));
        assert!(html.contains("data-char="));
        // Back-to-directory affordance.
        assert!(html.contains("All battle reports"));
        assert!(html.contains("href=\"/br\""));
    }

    #[test]
    fn details_view_has_zkill_links_and_involvement_maps() {
        let html = viewer_page(&card_data(Some("Kills"), "u")).into_string();
        // Each loss links to its zKill killmail (kill ids 1 and 3 are losses in `doc`).
        assert!(html.contains("https://zkillboard.com/kill/1/"));
        assert!(html.contains("rel=\"noopener\""));
        assert!(html.contains("data-kill=\"1\""));
        // The embedded involvement maps (integer-only) are present for the hover JS.
        assert!(html.contains("id=\"inv-data\""));
        assert!(html.contains("\"ka\"") && html.contains("\"ck\""));
        // The JSON keeps its quotes (not HTML-escaped) so JSON.parse works.
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
        let d = BattleReportDoc::new(battle, engs, Overrides::default(), None, 1_700_000_000);
        let data = CardData { id: "Zz22222222".into(), doc: d, uploader: "u".into(), views: 1 };
        let html = viewer_page(&data).into_string();
        // The malicious pilot name (shown in the Details cell) is escaped.
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
        // The viewer renders the dominant logos (alliance logo URL) in side headers.
        let html = viewer_page(&card_data(Some("Dom"), "u")).into_string();
        assert!(html.contains("images.evetech.net/alliances/100/logo"));
        assert!(html.contains("class=\"dom-logo\""));
    }

    #[test]
    fn hull_filter_ids_match_between_tiles_and_details() {
        // The hull filter only works if the grouped-tile `data-ship` equals the per-
        // participant `data-ship` it filters against. Both must carry the same hull type id.
        let html = viewer_page(&card_data(Some("Filter"), "u")).into_string();
        // The Tiles tile and the Details cells share hull id 587 (and 588).
        for id in ["587", "588"] {
            let tile = format!("class=\"ship\" data-ship=\"{id}\"");
            let cell = format!("data-ship=\"{id}\"");
            assert!(html.contains(&tile), "tile carries data-ship={id}");
            // A Details cell carries the same id (appears beyond the single tile occurrence).
            assert!(html.matches(&cell).count() >= 2, "a detail cell also has data-ship={id}");
        }
        // The filter hides via a class (so author `.dcell` display can't override `[hidden]`),
        // and the chip's [hidden] is force-hidden — both required for apply/clear to work.
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
        let d = BattleReportDoc::new(battle, engs, Overrides::default(), None, 1_700_000_000);
        let data = CardData { id: "Ll44444444".into(), doc: d, uploader: "u".into(), views: 1 };
        let html = viewer_page(&data).into_string();
        // The long party name is truncated with an ellipsis class and shown in full via title.
        assert!(html.contains("class=\"dparty-name\""));
        assert!(html.contains(&format!("title=\"{long}\"")));
        // The ellipsis CSS and the shrink-enabling min-width:0 are present.
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
        let d = BattleReportDoc::new(battle, engs, Overrides::default(), None, 1_700_000_000);
        let data = CardData { id: "Mm33333333".into(), doc: d, uploader: "u".into(), views: 1 };
        let html = viewer_page(&data).into_string();
        // The breakdown modal is rendered (hidden) with per-side composition lists.
        assert!(html.contains("id=\"breakdown-modal\""));
        assert!(html.contains("class=\"bd-list\""));
        assert!(html.contains("class=\"bd-count\""));
        // The malicious party name is escaped in the breakdown.
        assert!(!html.contains("<b>Sneaky</b> Alliance"));
        assert!(html.contains("&lt;b&gt;Sneaky&lt;/b&gt; Alliance"));
    }
}
