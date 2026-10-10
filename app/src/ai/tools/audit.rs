//! Token audit: every read tool run on fixture data, with what it returns and how big it is.
//! `cargo test --bin eve-spai ai_token_audit -- --ignored --nocapture`, then read
//! target/ai-tool-audit.txt.

use serde_json::json;

use super::testkit::facts;
use crate::ai::deps::AiDeps;

#[test]
#[ignore = "writes a report for reading, asserts nothing"]
fn ai_token_audit() {
    use crate::uitest::fixtures as fx;
    let mut f = facts(&["intel", "kills", "battles", "map", "wormholes", "jabber", "pilots", "notes", "characters.locations", "fleets", "rescue", "actions", "feeds", "internet"]);
    f.lookup_current = fx::lookup_rows().into_iter().map(|(n, _)| n).collect();
    f.notes_view = Some(std::sync::Arc::new(fx::notebook().view_with("", &Default::default())));
    f.fleet_backend = Some(std::sync::Arc::new(crate::fleets::spoof::SpoofBackend::with(crate::fleets::seed::invented(), std::time::Duration::ZERO)));
    f.alert_rules = vec![crate::settings::default_rule()];
    let mut deps = AiDeps::for_tests(f);
    deps.online = true;
    let now = crate::clock::utc().timestamp();
    {
        let mut st = deps.intel_state.lock().unwrap();
        for (i, mut r) in [fx::intel_typical(), fx::intel_across_the_bridge(), fx::intel_beyond_the_gates(), fx::intel_next_door(), fx::intel_clear(), fx::intel_torture(), fx::intel_two_celestials()].into_iter().enumerate() {
            r.received = now - 60 * i as i64;
            st.reports.push(r);
        }
    }
    *deps.jabber.lock().unwrap() = fx::jabber_state();
    deps.jabber.lock().unwrap().pings = vec![fx::ping_fleet(), fx::ping_fleet_multi(), fx::ping_plain(), fx::ping_plain_multiline()];
    {
        let mut t = deps.lookup_table.lock().unwrap();
        for (n, r) in fx::lookup_rows() {
            t.rows.insert(n.to_lowercase(), r);
        }
        t.orgs.extend(fx::lookup_orgs());
    }
    deps.alerts.lock().unwrap().push((now - 120, "Hostiles 2 jumps from 1DQ1-A: 14 in Muninns".into()));
    let store = crate::store::Store::mem();
    for i in 0..40 {
        store.log_kill(&crate::store::history::KillRow { kill_id: 1000 + i, time: now - i * 600, system_id: 30_004_759, ship_type_id: 12015, value: 2.5e8, attackers: 12, attacker_alliances: vec![1354830081], attacker_ships: vec![22456, 12015], on_gate: i % 3 == 0, ..Default::default() });
    }
    store.log_system_stats(now / 3600, &[crate::store::history::HourStats { system_id: 30_004_759, ship_kills: 4, pod_kills: 1, npc_kills: 300, jumps: 80 }]);
    let calls: &[(&str, serde_json::Value)] = &[
        ("search_intel", json!({})),
        ("search_intel", json!({"kinds": ["cyno", "bubble"]})),
        ("track_movement", json!({"entity": "frat"})),
        ("system_info", json!({"system": "1DQ1-A"})),
        ("map_query", json!({"limit": 15})),
        ("route", json!({"from": "1DQ1-A", "to": "7-K5EL"})),
        ("systems_within", json!({"system": "1DQ1-A", "jumps": 3})),
        ("jove_systems_near", json!({"system": "1DQ1-A"})),
        ("gate_camps", json!({})),
        ("wormholes_near", json!({})),
        ("kills_anywhere", json!({"days": 1})),
        ("system_history", json!({"system": "1DQ1-A", "days": 2})),
        ("jabber_pings", json!({})),
        ("jabber_rooms", json!({})),
        ("local_scan", json!({})),
        ("pilot_info", json!({"name": "Hostile Pilot"})),
        ("fleets_current", json!({})),
        ("fleets_history", json!({})),
        ("alerts", json!({})),
        ("alert_rules", json!({})),
        ("notes", json!({"search": "cyno"})),
        ("my_characters", json!({})),
        ("wormhole_info", json!({"type": "B274"})),
    ];
    let mut report = String::new();
    let mut total = 0;
    for (name, args) in calls {
        let facts = deps.facts();
        let mut actions = Vec::new();
        let mut ctx = super::Ctx { deps: &deps, facts: &facts, store: Some(&store), now, actions: &mut actions };
        let (out, err) = super::dispatch(&mut ctx, name, args);
        total += out.len();
        report.push_str(&format!("=== {name} {args} -> {} chars, ~{} tokens{}\n{out}\n\n", out.len(), out.len() / 4, if err { " (error)" } else { "" }));
        println!("{name:<20} {:>6} chars  ~{:>5} tokens{}", out.len(), out.len() / 4, if err { "  ERROR" } else { "" });
    }
    let defs: usize = super::registry().iter().map(|t| t.name.len() + t.description.len() + (t.schema)().to_string().len()).sum();
    println!("tool definitions: {defs} chars, ~{} tokens; outputs total {total} chars", defs / 4);
    std::fs::write(concat!(env!("CARGO_MANIFEST_DIR"), "/../target/ai-tool-audit.txt"), report).unwrap();
}
