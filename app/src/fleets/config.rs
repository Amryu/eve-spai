//! Moving the doctrine configuration between machines.
//!
//! What a doctrine is made of lives in `Settings` as several flat lists keyed by setup id, which
//! is convenient to edit and awkward to hand to somebody else. This folds them into one document
//! per doctrine and back again.
//!
//! Setup ids are the dashboard's and mean the same thing on every machine, so a bundle carries
//! them. Names travel too, for the doctrines added by hand and to name what is being imported.

use serde::{Deserialize, Serialize};

use crate::settings::{FleetBoostRequirement, FleetHull, Settings};

/// Bumped when the shape changes in a way an older build cannot read.
pub const VERSION: u32 = 1;

#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Bundle {
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub doctrines: Vec<DoctrineConfig>,
    /// Hulls welcome in any fleet, which belong to no doctrine.
    #[serde(default)]
    pub always_allowed: Vec<Hull>,
}

#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DoctrineConfig {
    pub setup_id: i32,
    pub name: String,
    /// True for a doctrine the user added rather than one the dashboard lists.
    #[serde(default)]
    pub custom: bool,
    #[serde(default)]
    pub tank: String,
    #[serde(default)]
    pub url: String,
    /// Takes its own hulls and nothing else.
    #[serde(default)]
    pub strict: bool,
    #[serde(default)]
    pub hulls: Vec<Hull>,
    #[serde(default)]
    pub boosts: Vec<Boost>,
}

#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hull {
    #[serde(default)]
    pub type_id: i64,
    pub name: String,
    #[serde(default)]
    pub main: bool,
}

#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Boost {
    pub charge: String,
    pub priority: String,
}

/// Everything configured about doctrines, as one document.
///
/// `names` supplies the setup names, which live in the seed rather than in settings, so the file
/// says what it is about rather than listing bare numbers.
pub fn export(s: &Settings, names: &dyn Fn(i32) -> Option<String>) -> Bundle {
    let mut ids: Vec<i32> = Vec::new();
    let mut note = |id: i32| {
        if id != 0 && !ids.contains(&id) {
            ids.push(id);
        }
    };
    for h in &s.fleet_hulls {
        note(h.setup_id);
    }
    for b in &s.fleet_boost_requirements {
        note(b.setup_id);
    }
    for (id, _) in &s.fleet_doctrine_tanks {
        note(*id);
    }
    for (id, _) in &s.fleet_doctrine_urls {
        note(*id);
    }
    for (id, _) in &s.fleet_custom_doctrines {
        note(*id);
    }
    for id in &s.fleet_doctrine_strict {
        note(*id);
    }
    ids.sort();

    let doctrines = ids
        .into_iter()
        .map(|id| DoctrineConfig {
            setup_id: id,
            name: s
                .fleet_custom_doctrines
                .iter()
                .find(|(c, _)| *c == id)
                .map(|(_, n)| n.clone())
                .or_else(|| names(id))
                .unwrap_or_default(),
            custom: s.fleet_custom_doctrines.iter().any(|(c, _)| *c == id),
            tank: s
                .fleet_doctrine_tanks
                .iter()
                .find(|(t, _)| *t == id)
                .map(|(_, v)| v.clone())
                .unwrap_or_default(),
            url: s
                .fleet_doctrine_urls
                .iter()
                .find(|(t, _)| *t == id)
                .map(|(_, v)| v.clone())
                .unwrap_or_default(),
            strict: s.fleet_doctrine_strict.contains(&id),
            hulls: s
                .fleet_hulls
                .iter()
                .filter(|h| h.setup_id == id)
                .map(|h| Hull { type_id: h.type_id, name: h.name.clone(), main: h.main })
                .collect(),
            boosts: s
                .fleet_boost_requirements
                .iter()
                .filter(|b| b.setup_id == id)
                .map(|b| Boost { charge: b.charge.clone(), priority: b.priority.clone() })
                .collect(),
        })
        .collect();

    Bundle {
        version: VERSION,
        doctrines,
        always_allowed: s
            .fleet_hulls
            .iter()
            .filter(|h| h.setup_id == 0)
            .map(|h| Hull { type_id: h.type_id, name: h.name.clone(), main: h.main })
            .collect(),
    }
}

/// What an import did, so the dialog can say so rather than looking like nothing happened.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Imported {
    pub doctrines: usize,
    pub hulls: usize,
    pub boosts: usize,
}

/// Writes a bundle into the settings.
///
/// `replace` clears what is there first. Without it a doctrine in the bundle overwrites that
/// doctrine and leaves every other one alone, which is how one doctrine gets shared without
/// taking the rest of somebody's configuration with it.
pub fn import(s: &mut Settings, bundle: &Bundle, replace: bool) -> Imported {
    if replace {
        s.fleet_hulls.clear();
        s.fleet_boost_requirements.clear();
        s.fleet_doctrine_tanks.clear();
        s.fleet_doctrine_urls.clear();
        s.fleet_doctrine_strict.clear();
        s.fleet_custom_doctrines.clear();
    }
    let mut out = Imported::default();

    if !bundle.always_allowed.is_empty() {
        s.fleet_hulls.retain(|h| h.setup_id != 0);
        for h in &bundle.always_allowed {
            s.fleet_hulls.push(FleetHull {
                setup_id: 0,
                type_id: h.type_id,
                name: h.name.trim().to_owned(),
                main: h.main,
            });
            out.hulls += 1;
        }
    }

    for d in &bundle.doctrines {
        let id = d.setup_id;
        if id == 0 {
            continue;
        }
        out.doctrines += 1;

        s.fleet_hulls.retain(|h| h.setup_id != id);
        for h in &d.hulls {
            s.fleet_hulls.push(FleetHull {
                setup_id: id,
                type_id: h.type_id,
                name: h.name.trim().to_owned(),
                main: h.main,
            });
            out.hulls += 1;
        }

        s.fleet_boost_requirements.retain(|b| b.setup_id != id);
        for b in &d.boosts {
            s.fleet_boost_requirements.push(FleetBoostRequirement {
                setup_id: id,
                charge: b.charge.trim().to_owned(),
                priority: b.priority.trim().to_lowercase(),
            });
            out.boosts += 1;
        }

        s.fleet_doctrine_tanks.retain(|(t, _)| *t != id);
        if !d.tank.trim().is_empty() {
            s.fleet_doctrine_tanks.push((id, d.tank.trim().to_lowercase()));
        }

        s.fleet_doctrine_urls.retain(|(t, _)| *t != id);
        if !d.url.trim().is_empty() {
            s.fleet_doctrine_urls.push((id, d.url.trim().to_owned()));
        }

        s.fleet_doctrine_strict.retain(|t| *t != id);
        if d.strict {
            s.fleet_doctrine_strict.push(id);
        }

        // A hand-added doctrine has to exist on this machine too, or its configuration has
        // nothing to hang off.
        if d.custom && !s.fleet_custom_doctrines.iter().any(|(c, _)| *c == id) {
            s.fleet_custom_doctrines.push((id, d.name.trim().to_owned()));
        }
    }
    out
}

pub fn to_json(b: &Bundle) -> String {
    serde_json::to_string_pretty(b).unwrap_or_default()
}

/// Reads a bundle, refusing one written by a later version rather than importing half of it.
pub fn from_json(text: &str) -> Result<Bundle, String> {
    let b: Bundle = serde_json::from_str(text.trim()).map_err(|e| e.to_string())?;
    if b.version > VERSION {
        return Err(format!("written by a newer version ({}, this build reads {VERSION})", b.version));
    }
    Ok(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> Settings {
        let mut s = Settings::default();
        s.fleet_custom_doctrines = vec![(-1, "Shield Cruisers".to_owned())];
        s.fleet_doctrine_tanks = vec![(46, "shield".to_owned()), (-1, "shield".to_owned())];
        s.fleet_doctrine_urls = vec![(46, "https://example.invalid/fast".to_owned())];
        s.fleet_doctrine_strict = vec![19];
        s.fleet_hulls = vec![
            FleetHull { setup_id: 46, type_id: 22_464, name: "Flycatcher".to_owned(), main: false },
            FleetHull { setup_id: 46, type_id: 37_458, name: "Kirin".to_owned(), main: false },
            FleetHull { setup_id: 19, type_id: 0, name: "Hecate".to_owned(), main: false },
            FleetHull { setup_id: 0, type_id: 11_957, name: "Falcon".to_owned(), main: false },
        ];
        s.fleet_boost_requirements = vec![
            FleetBoostRequirement {
                setup_id: 46,
                charge: "Shield Extension".to_owned(),
                priority: "high".to_owned(),
            },
            FleetBoostRequirement {
                setup_id: 19,
                charge: "Sensor Optimization".to_owned(),
                priority: "low".to_owned(),
            },
        ];
        s
    }

    fn names(id: i32) -> Option<String> {
        match id {
            46 => Some("Fast Tackle".to_owned()),
            19 => Some("Entosis".to_owned()),
            _ => None,
        }
    }

    /// Out and back in leaves the same configuration, which is the whole promise of the file.
    #[test]
    fn a_bundle_round_trips() {
        let before = settings();
        let bundle = export(&before, &names);
        let text = to_json(&bundle);
        let back = from_json(&text).expect("valid json");
        assert_eq!(back, bundle);

        let mut after = Settings::default();
        let n = import(&mut after, &back, true);
        assert_eq!(n.doctrines, 3);

        let sort = |s: &mut Settings| {
            s.fleet_hulls.sort_by(|a, b| (a.setup_id, &a.name).cmp(&(b.setup_id, &b.name)));
            s.fleet_boost_requirements
                .sort_by(|a, b| (a.setup_id, &a.charge).cmp(&(b.setup_id, &b.charge)));
            s.fleet_doctrine_tanks.sort();
            s.fleet_doctrine_urls.sort();
            s.fleet_doctrine_strict.sort();
            s.fleet_custom_doctrines.sort();
        };
        let mut before = before;
        sort(&mut before);
        sort(&mut after);
        assert_eq!(after.fleet_hulls, before.fleet_hulls);
        assert_eq!(after.fleet_boost_requirements, before.fleet_boost_requirements);
        assert_eq!(after.fleet_doctrine_tanks, before.fleet_doctrine_tanks);
        assert_eq!(after.fleet_doctrine_urls, before.fleet_doctrine_urls);
        assert_eq!(after.fleet_doctrine_strict, before.fleet_doctrine_strict);
        assert_eq!(after.fleet_custom_doctrines, before.fleet_custom_doctrines);
    }

    /// The file says what it is about rather than listing bare numbers.
    #[test]
    fn a_bundle_names_its_doctrines() {
        let bundle = export(&settings(), &names);
        let named = |id: i32| {
            bundle.doctrines.iter().find(|d| d.setup_id == id).map(|d| d.name.as_str())
        };
        assert_eq!(named(46), Some("Fast Tackle"));
        assert_eq!(named(19), Some("Entosis"));
        // A hand-added one carries its own name, and says it is one.
        assert_eq!(named(-1), Some("Shield Cruisers"));
        assert!(bundle.doctrines.iter().find(|d| d.setup_id == -1).expect("custom").custom);
        assert_eq!(bundle.always_allowed, vec![Hull { type_id: 11_957, name: "Falcon".to_owned(), main: false }]);
        // The always-allowed hulls belong to no doctrine.
        assert!(!bundle.doctrines.iter().any(|d| d.setup_id == 0));
    }

    /// Merging replaces the doctrines the bundle names and leaves the rest alone.
    #[test]
    fn a_merge_leaves_other_doctrines_alone() {
        let mut mine = Settings::default();
        mine.fleet_hulls = vec![
            FleetHull { setup_id: 60, type_id: 0, name: "Hound".to_owned(), main: false },
            FleetHull { setup_id: 46, type_id: 0, name: "Wrong".to_owned(), main: false },
        ];
        let bundle = Bundle {
            version: VERSION,
            doctrines: vec![DoctrineConfig {
                setup_id: 46,
                name: "Fast Tackle".to_owned(),
                hulls: vec![Hull { type_id: 22_464, name: "Flycatcher".to_owned(), main: false }],
                ..DoctrineConfig::default()
            }],
            always_allowed: Vec::new(),
        };
        import(&mut mine, &bundle, false);

        // Mine survived, theirs replaced the doctrine it names rather than adding to it.
        assert!(mine.fleet_hulls.iter().any(|h| h.setup_id == 60 && h.name == "Hound"));
        let theirs: Vec<&FleetHull> =
            mine.fleet_hulls.iter().filter(|h| h.setup_id == 46).collect();
        assert_eq!(theirs.len(), 1);
        assert_eq!(theirs[0].name, "Flycatcher");

        // An empty always-allowed list does not wipe one that is already there.
        mine.fleet_hulls.push(FleetHull { setup_id: 0, type_id: 0, name: "Falcon".to_owned(), main: false });
        import(&mut mine, &bundle, false);
        assert!(mine.fleet_hulls.iter().any(|h| h.setup_id == 0 && h.name == "Falcon"));
    }

    /// A file from a later build is refused rather than half read.
    #[test]
    fn a_newer_bundle_is_refused() {
        let text = to_json(&Bundle { version: VERSION + 1, ..Bundle::default() });
        let err = from_json(&text).expect_err("refused");
        assert!(err.contains("newer version"), "{err}");

        assert!(from_json("not json").is_err());
        // An empty document is a valid, empty bundle.
        assert_eq!(from_json("{}").expect("empty").doctrines.len(), 0);
    }
}
