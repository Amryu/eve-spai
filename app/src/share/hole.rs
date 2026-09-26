//! A hole as the group log carries it: each field with the time and author of its last change.
//! Merging keeps, per field, the change with the later (time, author): the same result in any
//! order, and applying one twice changes nothing.

use serde_json::Value;
use std::collections::HashMap;

use super::ops::{Field, HoleState};
use crate::wormholes::{DestClass, Life, Mass, ShipSize, Source, Wormhole};

/// Every shared field of a hole. Identity (`uid`, `system_id`) and origin (`source`,
/// `reported_at`) travel beside them.
pub const FIELDS: [&str; 15] = [
    "signature",
    "wh_type",
    "dest",
    "dest_system_id",
    "dest_signature",
    "dest_wh_type",
    "size",
    "is_drifter",
    "explicit_expiry",
    "life",
    "mass",
    "observed_at",
    "note",
    "detected_by",
    "jumped_at",
];

pub fn get(w: &Wormhole, field: &str) -> Value {
    let s = |v: &Option<String>| v.clone().map_or(Value::Null, Value::String);
    let n = |v: Option<i64>| v.map_or(Value::Null, Value::from);
    match field {
        "signature" => s(&w.signature),
        "wh_type" => s(&w.wh_type),
        "dest" => Value::String(w.dest.code().to_owned()),
        "dest_system_id" => n(w.dest_system_id),
        "dest_signature" => s(&w.dest_signature),
        "dest_wh_type" => s(&w.dest_wh_type),
        "size" => w.size.map_or(Value::Null, |x| Value::String(x.code().to_owned())),
        "is_drifter" => Value::Bool(w.is_drifter),
        "explicit_expiry" => n(w.explicit_expiry),
        "life" => w.life.map_or(Value::Null, |x| Value::String(x.code().to_owned())),
        "mass" => w.mass.map_or(Value::Null, |x| Value::String(x.code().to_owned())),
        "observed_at" => n(w.observed_at),
        "note" => s(&w.note),
        "detected_by" => s(&w.detected_by),
        "jumped_at" => n(w.jumped_at),
        _ => Value::Null,
    }
}

pub fn set(w: &mut Wormhole, field: &str, v: &Value) {
    let s = || v.as_str().map(str::to_owned);
    match field {
        "signature" => w.signature = s(),
        "wh_type" => w.wh_type = s(),
        "dest" => w.dest = v.as_str().map_or(DestClass::Unknown, DestClass::from_code),
        "dest_system_id" => w.dest_system_id = v.as_i64(),
        "dest_signature" => w.dest_signature = s(),
        "dest_wh_type" => w.dest_wh_type = s(),
        "size" => w.size = v.as_str().and_then(ShipSize::from_code),
        "is_drifter" => w.is_drifter = v.as_bool().unwrap_or(false),
        "explicit_expiry" => w.explicit_expiry = v.as_i64(),
        "life" => w.life = v.as_str().and_then(Life::from_code),
        "mass" => w.mass = v.as_str().and_then(Mass::from_code),
        "observed_at" => w.observed_at = v.as_i64(),
        "note" => w.note = s(),
        "detected_by" => w.detected_by = s(),
        "jumped_at" => w.jumped_at = v.as_i64(),
        _ => {}
    }
}

/// The fields that differ between two versions of a hole.
pub fn changed(before: Option<&Wormhole>, after: &Wormhole) -> Vec<&'static str> {
    FIELDS
        .iter()
        .copied()
        .filter(|f| before.is_none_or(|b| get(b, f) != get(after, f)))
        .filter(|f| before.is_some() || get(after, f) != Value::Null)
        .collect()
}

/// A field's last change: (time, author). An author of 0 is a local change not yet sent.
pub type Clock = (i64, i64);

pub fn state(w: &Wormhole, clocks: &HashMap<String, Clock>, me: i64) -> HoleState {
    let fields = FIELDS
        .iter()
        .filter_map(|f| {
            let (at, by) = *clocks.get(*f)?;
            Some((f.to_string(), Field { v: get(w, f), at, by: if by == 0 { me } else { by } }))
        })
        .collect();
    HoleState { uid: w.uid.clone(), system_id: w.system_id, source: w.source.code().to_owned(), reported_at: w.reported_at, fields }
}

/// Folds `remote` into `local`, field by field. Returns the fields taken, with their clocks.
pub fn merge(local: &mut Wormhole, clocks: &mut HashMap<String, Clock>, remote: &HoleState) -> Vec<(String, Clock)> {
    let mut taken = Vec::new();
    for (name, f) in &remote.fields {
        if !FIELDS.contains(&name.as_str()) {
            continue;
        }
        let mine = clocks.get(name).copied().unwrap_or((i64::MIN, 0));
        if (f.at, f.by) > mine {
            set(local, name, &f.v);
            clocks.insert(name.clone(), (f.at, f.by));
            taken.push((name.clone(), (f.at, f.by)));
        }
    }
    local.reported_at = if local.reported_at == 0 { remote.reported_at } else { local.reported_at.min(remote.reported_at) };
    taken
}

/// A new local row for a hole first heard of from the group.
pub fn fresh(remote: &HoleState) -> Wormhole {
    let mut w = Wormhole {
        uid: remote.uid.clone(),
        system_id: remote.system_id,
        source: Source::from_code(&remote.source),
        reported_at: remote.reported_at,
        updated_at: remote.fields.values().map(|f| f.at).max().unwrap_or(remote.reported_at),
        ..Default::default()
    };
    for (name, f) in &remote.fields {
        set(&mut w, name, &f.v);
    }
    w
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hole(fields: &[(&str, Value, i64, i64)]) -> HoleState {
        HoleState {
            uid: "u".into(),
            system_id: 1,
            source: "manual".into(),
            reported_at: 100,
            fields: fields.iter().map(|(n, v, at, by)| (n.to_string(), Field { v: v.clone(), at: *at, by: *by })).collect(),
        }
    }

    #[test]
    fn merging_in_any_order_gives_the_same_hole() {
        let a = hole(&[("mass", "reduced".into(), 10, 1), ("life", "lt4h".into(), 20, 1)]);
        let b = hole(&[("mass", "critical".into(), 15, 2), ("signature", "ABC".into(), 5, 2)]);
        let c = hole(&[("life", "lt1h".into(), 20, 3), ("signature", "ABD".into(), 5, 1)]);
        let orders = [[&a, &b, &c], [&c, &b, &a], [&b, &a, &c], [&c, &a, &b]];
        let results: Vec<(Option<Mass>, Option<Life>, Option<String>)> = orders
            .iter()
            .map(|order| {
                let mut w = Wormhole::default();
                let mut clocks = HashMap::new();
                for op in order {
                    merge(&mut w, &mut clocks, op);
                }
                (w.mass, w.life, w.signature)
            })
            .collect();
        assert!(results.windows(2).all(|p| p[0] == p[1]), "{results:?}");
        assert_eq!(results[0], (Some(Mass::Critical), Some(Life::Under1h), Some("ABC".into())));
    }

    #[test]
    fn applying_the_same_change_twice_changes_nothing() {
        let a = hole(&[("mass", "reduced".into(), 10, 1)]);
        let mut w = Wormhole::default();
        let mut clocks = HashMap::new();
        assert_eq!(merge(&mut w, &mut clocks, &a).len(), 1);
        assert!(merge(&mut w, &mut clocks, &a).is_empty());
    }

    #[test]
    fn a_state_round_trips_through_a_fresh_row() {
        let mut w = Wormhole { uid: "u".into(), system_id: 1, signature: Some("ABC".into()), mass: Some(Mass::Reduced), reported_at: 100, ..Default::default() };
        w.dest = DestClass::Highsec;
        let clocks: HashMap<String, Clock> = changed(None, &w).into_iter().map(|f| (f.to_owned(), (50, 0))).collect();
        let s = state(&w, &clocks, 7);
        assert!(s.fields.values().all(|f| f.by == 7), "unsent local changes go out under our name");
        let back = fresh(&s);
        assert_eq!((back.signature, back.mass, back.dest), (w.signature, w.mass, w.dest));
    }
}
