//! Server-rendered, mobile-first HTML: the public report directory (`GET /br`) and
//! the single-report viewer (`GET /br/{id}`).
//!
//! Built with [`maud`] — compile-time templates that auto-escape every interpolation.
//! Every user- or EVE-supplied string (titles, uploader names, side/alliance/pilot
//! names) is rendered through a normal `(value)` interpolation, never via
//! [`PreEscaped`], so none of them can inject markup. `PreEscaped` is used *only* for
//! the static, developer-authored CSS/JS blobs in [`layout`].

use br_core::battle::{Battle, BattleReportDoc, Side};
use maud::{html, Markup, PreEscaped, DOCTYPE};

/// EVE ship/structure icon for a type id (64 px). Empty markup for type id 0.
fn icon_url(type_id: i64) -> String {
    format!("https://images.evetech.net/types/{type_id}/icon?size=64")
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

/// One side's full panel: name, pilots, ISK lost/destroyed, efficiency, ship roster.
fn side_panel(battle: &Battle, side_idx: usize) -> Markup {
    let side = &battle.sides[side_idx];
    let groups = roster_groups(battle, side_idx);
    let pilots: usize = groups.iter().map(|g| g.lost + g.survived).sum();
    html! {
        div .side-panel {
            div .side-head {
                h3 { (side_label(side)) }
                @if let Some(coalition) = &side.coalition {
                    @if !side.parties.is_empty() {
                        div .coalition-members {
                            (format!("{} — {} parties", coalition, side.parties.len()))
                        }
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
                    div .ship {
                        img .ship-icon src=(icon_url(g.ship)) width="48" height="48" loading="lazy" alt="ship";
                        div .ship-counts {
                            @if g.lost > 0 { span .lost { (g.lost) " lost" } }
                            @if g.survived > 0 { span .survived { (g.survived) " survived" } }
                            @if g.final_blows > 0 { span .fb title="final blows" { "★ " (g.final_blows) } }
                        }
                    }
                }
            }
        }
    }
}

/// The single-report viewer page.
pub fn viewer_page(data: &CardData) -> Markup {
    let b = &data.doc.battle;
    let json_href = format!("/api/br/{}.json", data.id);
    let body = html! {
        section .viewer {
            div .v-head {
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
                div .v-actions {
                    a .btn .primary href=(json_href) download { "Download JSON" }
                    button type="button" .btn #copy-link data-url="" { "Copy link" }
                    span .meta-by { "uploaded by " (data.uploader) " · " (data.views) " views" }
                }
            }
            div .panels {
                @for i in 0..b.sides.len() {
                    (side_panel(b, i))
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
  var t;
  f.querySelector('[name=sort]').addEventListener('change',function(){f.submit();});
  f.querySelectorAll('input[type=text],input[type=number]').forEach(function(el){
    el.addEventListener('input',function(){clearTimeout(t);t=setTimeout(function(){f.submit();},600);});
  });
})();
"#;

/// Copy-link button: writes the current page URL to the clipboard. No interpolated data.
const VIEWER_JS: &str = r#"
(function(){
  var b=document.getElementById('copy-link'); if(!b) return;
  b.addEventListener('click',function(){
    navigator.clipboard.writeText(window.location.href).then(function(){
      var o=b.textContent; b.textContent='Copied!'; setTimeout(function(){b.textContent=o;},1500);
    });
  });
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
  display:block; background:var(--panel); border:1px solid var(--line); border-radius:12px;
  padding:16px; color:var(--text);
}
.card:hover{text-decoration:none; border-color:var(--blue-dim);}
.card-title{font-size:16px; font-weight:600;}
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

.viewer .v-head{background:var(--panel); border:1px solid var(--line); border-radius:12px; padding:18px 20px; margin-bottom:18px;}
.v-actions{display:flex; flex-wrap:wrap; align-items:center; gap:10px; margin-top:12px;}
.meta-by{color:var(--muted); font-size:13px;}
.panels{display:grid; grid-template-columns:repeat(2,1fr); gap:16px;}
.side-panel{background:var(--panel); border:1px solid var(--line); border-radius:12px; padding:16px;}
.side-head h3{margin-bottom:2px;}
.coalition-members{color:var(--muted); font-size:12.5px;}
.side-stats{display:grid; grid-template-columns:repeat(3,1fr); gap:10px; margin:12px 0;}
.stat{display:flex; flex-direction:column;}
.stat-num{font-size:16px; font-weight:600; font-family:var(--mono);}
.stat-label{color:var(--muted); font-size:11.5px; text-transform:uppercase; letter-spacing:0.4px;}
.roster{display:flex; flex-wrap:wrap; gap:10px; margin-top:14px;}
.ship{display:flex; flex-direction:column; align-items:center; gap:4px; width:64px;}
.ship-icon{border-radius:8px; border:1px solid var(--line); background:var(--panel-2);}
.ship-counts{display:flex; flex-direction:column; align-items:center; font-size:11px; line-height:1.3;}
.ship-counts .lost{color:var(--red);}
.ship-counts .survived{color:var(--muted);}
.ship-counts .fb{color:var(--blue);}
.notfound{text-align:center; padding:60px 20px;}
.notfound p{color:var(--muted); max-width:480px; margin:10px auto 24px;}

@media (max-width:700px){
  .cards{grid-template-columns:1fr;}
  .panels{grid-template-columns:1fr;}
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
}
