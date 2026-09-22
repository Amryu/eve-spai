//! ESI's id-to-name and name-to-id lookups, shared so every caller deduplicates, batches and
//! survives an unresolvable id the same way.

use std::collections::HashMap;

const NAMES_URL: &str = "https://esi.evetech.net/latest/universe/names/";
const IDS_URL: &str = "https://esi.evetech.net/latest/universe/ids/";
/// ESI accepts 1000 ids per call. A smaller batch keeps bisecting around one bad id cheap.
const BATCH: usize = 200;

#[derive(serde::Deserialize)]
struct NameEntry {
    id: i64,
    name: String,
}

pub fn names(client: &reqwest::blocking::Client, ids: &[i64]) -> HashMap<i64, String> {
    let mut out = HashMap::new();
    names_into(client, ids, &mut out);
    out
}

/// Resolves only the ids `out` does not already name, so it doubles as a cache fill.
pub fn names_into(client: &reqwest::blocking::Client, ids: &[i64], out: &mut HashMap<i64, String>) {
    let mut wanted: Vec<i64> = ids.iter().copied().filter(|id| *id != 0 && !out.contains_key(id)).collect();
    wanted.sort_unstable();
    // ESI answers 400 to a batch with a duplicate id.
    wanted.dedup();
    for chunk in wanted.chunks(BATCH) {
        batch(client, chunk, out);
    }
}

/// With its own client, for callers that have none to hand.
pub fn lookup_names(ids: &[i64]) -> HashMap<i64, String> {
    match crate::http::client(20) {
        Ok(client) => names(&client, ids),
        Err(_) => HashMap::new(),
    }
}

/// ESI fails the entire request with 404 when even one id is unresolvable (a deleted character, say),
/// so a 404 bisects to isolate the bad id instead of losing the whole batch.
fn batch(client: &reqwest::blocking::Client, ids: &[i64], out: &mut HashMap<i64, String>) {
    if ids.is_empty() {
        return;
    }
    match client.post(NAMES_URL).json(&ids).send() {
        Ok(resp) if resp.status().is_success() => {
            if let Ok(entries) = resp.json::<Vec<NameEntry>>() {
                out.extend(entries.into_iter().map(|e| (e.id, e.name)));
            }
        }
        Ok(resp) if resp.status() == reqwest::StatusCode::NOT_FOUND && ids.len() > 1 => {
            let mid = ids.len() / 2;
            batch(client, &ids[..mid], out);
            batch(client, &ids[mid..], out);
        }
        Ok(resp) if !resp.status().is_success() => {
            let status = resp.status();
            let body = resp.text().unwrap_or_default();
            crate::esilog::record(
                "universe/names non-2xx",
                &format!("status: {status}\nbatch size: {}\nbody:\n{body}", ids.len()),
            );
        }
        _ => {}
    }
}

/// A character's id and canonical name. ESI can answer several characters for one query, so an
/// exact case-insensitive match wins over its first hit.
pub fn character(client: &reqwest::blocking::Client, name: &str) -> Result<Option<(i64, String)>, reqwest::Error> {
    #[derive(serde::Deserialize)]
    struct Ids {
        characters: Option<Vec<NameEntry>>,
    }
    let name = name.trim();
    let ids: Ids = client.post(IDS_URL).json(&[name]).send()?.error_for_status()?.json()?;
    Ok(best_match(ids.characters.unwrap_or_default(), name))
}

/// Ids for many character names at once, keyed by the lowercased name asked for. `None` when the
/// call itself failed, so the caller can tell "no such pilot" from "ask again later". Keep `names`
/// under ~200: ESI answers 400 or 504 to much larger batches.
pub fn character_ids(client: &reqwest::blocking::Client, names: &[String]) -> Option<HashMap<String, (i64, String)>> {
    #[derive(serde::Deserialize)]
    struct Ids {
        characters: Option<Vec<NameEntry>>,
    }
    let ids: Ids = client.post(IDS_URL).json(names).send().ok()?.error_for_status().ok()?.json().ok()?;
    let mut out = HashMap::new();
    for c in ids.characters.unwrap_or_default() {
        out.insert(c.name.to_lowercase(), (c.id, c.name));
    }
    Some(out)
}

fn best_match(chars: Vec<NameEntry>, name: &str) -> Option<(i64, String)> {
    let exact = chars.iter().position(|c| c.name.eq_ignore_ascii_case(name));
    let pick = exact.or((!chars.is_empty()).then_some(0))?;
    let c = chars.into_iter().nth(pick)?;
    Some((c.id, c.name))
}

#[cfg(test)]
mod tests {
    use super::{best_match, NameEntry};

    fn e(id: i64, name: &str) -> NameEntry {
        NameEntry { id, name: name.to_owned() }
    }

    #[test]
    fn an_exact_name_beats_the_first_hit() {
        let got = best_match(vec![e(1, "Bob Prime"), e(2, "bob")], "Bob");
        assert_eq!(got, Some((2, "bob".to_owned())));
        assert_eq!(best_match(vec![e(1, "Bob Prime")], "Bob"), Some((1, "Bob Prime".to_owned())));
        assert_eq!(best_match(vec![], "Bob"), None);
    }
}
