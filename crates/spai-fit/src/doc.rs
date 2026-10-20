//! The persisted fit: the `Fit` as JSON with a version, the same bytes in the desktop database,
//! the browser's storage and the server.

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

use crate::fit::Fit;

pub const VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
struct FitDoc {
    #[serde(default)]
    v: u32,
    #[serde(flatten)]
    fit: Fit,
}

pub fn to_json(fit: &Fit) -> String {
    serde_json::to_string(&FitDoc { v: VERSION, fit: fit.clone() }).expect("a fit serialises")
}

pub fn from_json(json: &str) -> Result<Fit> {
    let doc: FitDoc = serde_json::from_str(json)?;
    if doc.v > VERSION {
        bail!("fit saved by a newer version ({} > {VERSION})", doc.v);
    }
    Ok(doc.fit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fit::Slot;

    #[test]
    fn a_fit_round_trips_and_a_newer_document_is_refused() {
        let mut fit = Fit::new(587, "Rifter");
        fit.add_module(2048, Slot::Low);
        let json = to_json(&fit);
        assert!(json.starts_with("{\"v\":1,"));
        assert_eq!(from_json(&json).unwrap(), fit);
        assert!(from_json(&json.replacen("\"v\":1", "\"v\":9", 1)).is_err());
        assert!(from_json("{\"ship\": 587}").is_ok(), "an unversioned document reads as version 0");
    }
}
