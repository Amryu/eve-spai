use std::time::Instant;
fn main() {
    let mut a = std::env::args().skip(1);
    let d = spai_fit::dogma::blob::decode(&std::fs::read(a.next().unwrap()).unwrap()).unwrap();
    let docs: Vec<spai_fit::doctrine::Doctrine> = serde_json::from_str(&std::fs::read_to_string(a.next().unwrap()).unwrap()).unwrap();
    let skills = spai_fit::skills::SkillSet::all_v();
    let doc = docs.iter().find(|x| x.name.starts_with("Alpha")).unwrap();
    let fits: std::collections::HashMap<String, spai_fit::fit::Fit> = doc.inline_fits().map(|(i, f)| (i.to_owned(), f.clone())).collect();
    let mut fleet = doc.fleet(&d, &|_| None, &skills);
    fleet.size = 250;
    let lookup = |id: &str| fits.get(id).cloned();
    let t = Instant::now();
    let st = spai_fit::fleet::stats(&d, &fleet, &lookup, &|_| skills.clone());
    println!("stats: {:.0} ms ({} slots)", t.elapsed().as_secs_f64() * 1e3, st.slots.len());
    let t = Instant::now();
    let ammo: Vec<Vec<_>> = st.slots.iter().map(|x| { let t1 = Instant::now(); let c = spai_fit::fleet::ammo::choices(&d, &x.fit, &x.skills, &st.env); println!("   ammo slot {}: {:.0} ms", x.name, t1.elapsed().as_secs_f64() * 1e3); c }).collect();
    println!("ammo: {:.0} ms", t.elapsed().as_secs_f64() * 1e3);
    let side = spai_fit::fleet::sweep::Side { fleet: &fleet, stats: &st, ammo: &ammo };
    let mut ranges = spai_fit::fleet::sweep::ranges(&st, &st);
    ranges.push(f64::from(fleet.range)); ranges.sort_by(|x, y| x.total_cmp(y)); ranges.dedup();
    let t = Instant::now();
    let sw = spai_fit::fleet::sweep::sweep(&side, &side, &ranges);
    println!("sweep: {:.0} ms ({} ranges)", t.elapsed().as_secs_f64() * 1e3, ranges.len());
    let t = Instant::now();
    let _ = spai_fit::fleet::race::race(&d, &side, &side, f64::from(fleet.range), 1800.0);
    println!("race: {:.0} ms", t.elapsed().as_secs_f64() * 1e3);
    let t = Instant::now();
    let rs: Vec<f64> = sw.points.iter().map(|p| p.distance).collect();
    if let Some(tg) = spai_fit::fleet::sweep::primary(&fleet, &st) { for k in 0..st.slots.len() { let _ = spai_fit::fleet::sweep::ammo_bands(&side, &side, k, tg, &rs); } }
    println!("bands: {:.0} ms", t.elapsed().as_secs_f64() * 1e3);
    let t = Instant::now();
    let _ = spai_fit::fleet::against(&d, &fleet, &st, &fleet, &st);
    println!("against: {:.0} ms", t.elapsed().as_secs_f64() * 1e3);
}
