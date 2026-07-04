const DOCTRINES: &[(&str, &[&str])] = &[
    (
        "https://goonfleet.com/index.php/topic/380930-active-strat-typhoons/",
        &["typhoon"],
    ),
    (
        "https://goonfleet.com/index.php/topic/369029-active-strat-vultures/",
        &["vulture"],
    ),
    (
        "https://goonfleet.com/index.php/topic/355156-active-strat-tomahawks-ravens/",
        &["tomahawk", "raven"],
    ),
    (
        "https://goonfleet.com/index.php/topic/326958-active-strat-flycatchers/",
        &["flycatcher"],
    ),
    (
        "https://goonfleet.com/index.php/topic/366187-active-strat-snail-fleet/",
        &["snail"],
    ),
    (
        "https://goonfleet.com/index.php/topic/376533-active-strat-tigers-claw-carriers/",
        &["tiger", "carrier"],
    ),
    (
        "https://goonfleet.com/index.php/topic/379775-active-beehive-stupid-idiot-rorquals-sir/",
        &["rorqual", "beehive"],
    ),
    (
        "https://goonfleet.com/index.php/topic/349468-goonswarm-federation-unified-strategic-doctrine-mk-xiii/",
        &["unified", "mk xiii", "strategic doctrine"],
    ),
];

pub fn link_for(doctrine: &str) -> Option<&'static str> {
    let d = doctrine.to_lowercase();
    DOCTRINES
        .iter()
        .find(|(_, keys)| keys.iter().any(|k| d.contains(k)))
        .map(|(url, _)| *url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_doctrine_names() {
        assert!(link_for("Typhoons").unwrap().contains("typhoons"));
        assert!(link_for("vulture fleet").unwrap().contains("vultures"));
        assert!(link_for("Tiger's Claw (Carriers)").unwrap().contains("tigers-claw"));
        assert!(link_for("Rorqual mining op").unwrap().contains("rorquals"));
        assert!(link_for("Some Unknown Doctrine").is_none());
    }
}
