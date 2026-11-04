//! Puts written skill plans on a doctrine bundle's fits as milestones.
//!
//! `spai-doctrine-milestones --blob <dogma> --doctrines <in.json> --plans <plans.json> --out <out.json>`
//!
//! `plans.json`: `{ "<fit id>": { "required": [["<skill>", level], ..], "recommended": [..] } }`.
//! Every fit's milestones are replaced: one without a plan ends up with none.

use std::collections::BTreeMap;

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use spai_fit::ids::TypeId;

#[derive(Deserialize)]
struct Plan {
    #[serde(default)]
    required: Vec<(String, u8)>,
    #[serde(default)]
    recommended: Vec<(String, u8)>,
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let arg = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).with_context(|| format!("{name} is required"));
    let dogma = spai_fit::dogma::blob::decode(&std::fs::read(arg("--blob")?)?)?;
    let mut bundle: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(arg("--doctrines")?)?)?;
    let plans: BTreeMap<String, Plan> = serde_json::from_str(&std::fs::read_to_string(arg("--plans")?)?)?;
    let mut unknown: BTreeMap<String, usize> = BTreeMap::new();
    let mut ids = |list: &[(String, u8)]| -> Vec<(TypeId, u8)> {
        let mut out: Vec<(TypeId, u8)> = Vec::new();
        for (name, level) in list {
            match dogma.type_named(name).filter(|t| t.category == spai_fit::ids::category::SKILL) {
                Some(t) => match out.iter_mut().find(|(s, _)| *s == t.id) {
                    Some(e) => e.1 = e.1.max(*level),
                    None => out.push((t.id, *level)),
                },
                None => *unknown.entry(name.clone()).or_default() += 1,
            }
        }
        out
    };
    let (mut fits, mut placed, mut used) = (0, 0, 0);
    let Some(doctrines) = bundle.as_array_mut() else { bail!("the bundle is no list of doctrines") };
    for doctrine in doctrines {
        let name = doctrine["name"].as_str().unwrap_or_default().to_owned();
        for f in doctrine["fits"].as_array_mut().into_iter().flatten() {
            fits += 1;
            let id = f["fit_id"].as_str().unwrap_or_default().to_owned();
            let Some(body) = f["fit"].as_object_mut() else { continue };
            body.remove("milestones");
            let Some(plan) = plans.get(&id) else { continue };
            used += 1;
            let fit: spai_fit::fit::Fit = serde_json::from_value(serde_json::Value::Object(body.clone())).with_context(|| id.clone())?;
            let resolved = spai_fit::variations::resolve(&dogma, &fit);
            let m = spai_fit::skills::milestones_over_minimum(&dogma, &resolved, &ids(&plan.required), &ids(&plan.recommended), ["Required", "Recommended"]);
            println!("{name} / {id}: {}", m.iter().map(|x| format!("{} {}", x.name, x.skills.len())).collect::<Vec<_>>().join(", "));
            if !m.is_empty() {
                placed += 1;
                body.insert("milestones".to_owned(), serde_json::to_value(&m)?);
            }
        }
    }
    println!("{placed} of {fits} fits have milestones; {used} of {} plans matched a fit", plans.len());
    for (name, n) in &unknown {
        println!("unknown skill: {name} ({n})");
    }
    std::fs::write(arg("--out")?, serde_json::to_string(&bundle)?)?;
    Ok(())
}
