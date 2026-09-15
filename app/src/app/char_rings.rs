//! Jump distances from every character to a card's systems, and whether a jump bridge is behind the number shown.

/// How far a BFS walks before it gives up and reports "unreachable".
pub(crate) const JUMP_SCAN_CAP: u32 = 50;

pub(crate) fn jumps_from_you(
    systems: &Option<std::sync::Arc<crate::geo::Systems>>,
    player_sys: Option<i64>,
    target: Option<i64>,
    use_bridges: bool,
) -> Option<u32> {
    let (sys, p, t) = (systems.as_ref()?, player_sys?, target?);
    if use_bridges {
        sys.jumps(t, p, JUMP_SCAN_CAP)
    } else {
        sys.jumps_gates_only(t, p, JUMP_SCAN_CAP)
    }
}

/// What a card's jump distance rests on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub(crate) enum JumpVia {
    /// Gates alone reach the target in the number shown, so it is also what a hostile faces.
    #[default]
    Gates,
    /// A jump bridge cut the trip; the gate answer is this much longer.
    BridgeShorter(u32),
    /// Gates do not reach the target at all, so the number only exists because of a bridge.
    BridgeOnly,
}

/// One character's distance to a card's system.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub(crate) struct CharHop {
    pub(crate) name: String,
    /// EVE character id, for the portrait. 0 when the name is not in the store, which renders a
    /// glyph instead of a broken image.
    pub(crate) id: i64,
    /// None when nothing reaches the target inside [`JUMP_SCAN_CAP`].
    pub(crate) jumps: Option<u32>,
    #[serde(default)]
    pub(crate) via: JumpVia,
}

/// Which characters a card's numbers belong to, nearest first. Empty where there is nothing to
/// disambiguate, which is the single-character case, and the card then draws the plain number.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub(crate) struct CardChars {
    pub(crate) hops: Vec<CharHop>,
    /// Index into `hops` of the active character.
    pub(crate) selected: Option<usize>,
    #[serde(default)]
    pub(crate) ly: CardLy,
}

/// Straight-line distance from staging and from the active character to each system on a card.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub(crate) struct CardLy {
    pub(crate) staging: String,
    pub(crate) you: String,
    /// (system, from staging, from you) in hundredths of a light-year, integers so the card stays
    /// hashable.
    pub(crate) systems: Vec<(i64, Option<u32>, Option<u32>)>,
}

impl CardLy {
    pub(crate) fn of(&self, system: i64) -> Option<(Option<u32>, Option<u32>)> {
        self.systems.iter().find(|(id, ..)| *id == system).map(|&(_, st, you)| (st, you))
    }
}

impl CardChars {
    pub(crate) fn nearest(&self) -> Option<&CharHop> {
        self.hops.first().filter(|h| h.jumps.is_some())
    }

    /// The selected character's slot, only when it is not already the nearest one.
    pub(crate) fn second(&self) -> Option<&CharHop> {
        match self.selected {
            Some(i) if i != 0 => self.hops.get(i),
            _ => None,
        }
    }
}

/// Whether a character's distance may stand behind an alert. Extracted so the card's candidate set
/// and `AlertEngine::evaluate`'s cannot drift apart.
pub(crate) fn alert_candidate(
    name: &str,
    docked: bool,
    disabled: &[String],
    only_undocked: bool,
) -> bool {
    let alerts_on = !disabled.iter().any(|d| d.eq_ignore_ascii_case(name));
    let in_space = !only_undocked || !docked;
    alerts_on && in_space
}

pub(crate) struct Ring {
    pub(crate) name: String,
    pub(crate) id: i64,
    /// The ball the shown number comes from.
    pub(crate) shown: std::sync::Arc<std::collections::HashMap<i64, u32>>,
    /// Gate-only ball, kept only while bridges count, to derive the verdict per character.
    pub(crate) gates: Option<std::sync::Arc<std::collections::HashMap<i64, u32>>>,
    pub(crate) active: bool,
    pub(crate) from: i64,
}

/// Every character a card may attribute a number to this frame, each with the whole distance ball
/// around wherever it is sitting. Built once per feed, read per card: one walk per card at 250 cards
/// already fills the frame budget.
#[derive(Default)]
pub(crate) struct CharRings {
    pub(crate) rings: Vec<Ring>,
    pub(crate) systems: Option<std::sync::Arc<crate::geo::Systems>>,
    pub(crate) staging: Option<(String, i64)>,
}

impl CharRings {
    /// Resolved against the graph, so an unknown or empty name drops the staging line.
    pub(crate) fn with_staging(mut self, name: Option<&str>) -> Self {
        self.staging = name
            .and_then(|n| self.systems.as_ref()?.lookup(n.trim()))
            .map(|i| (i.name.clone(), i.id));
        self
    }

    pub(crate) fn card_for(&self, r: &crate::intel::IntelReport) -> CardChars {
        let mut c = self.card(r.primary_system().map(|s| s.id));
        let Some(sys) = self.systems.as_ref() else { return c };
        let you = self.rings.iter().find(|r| r.active);
        let centi = |from: i64, to: i64| sys.ly_between(from, to).map(|ly| (ly * 100.0).round() as u32);
        let mut systems: Vec<(i64, Option<u32>, Option<u32>)> = Vec::new();
        for s in &r.systems {
            if systems.iter().any(|(id, ..)| *id == s.id) {
                continue;
            }
            let st = self.staging.as_ref().and_then(|(_, id)| centi(*id, s.id));
            let yo = you.and_then(|y| centi(y.from, s.id));
            if st.is_some() || yo.is_some() {
                systems.push((s.id, st, yo));
            }
        }
        if !systems.is_empty() {
            c.ly = CardLy {
                staging: self.staging.as_ref().map(|(n, _)| n.clone()).unwrap_or_default(),
                you: you.map(|y| y.name.clone()).unwrap_or_default(),
                systems,
            };
        }
        c
    }

    /// N hash lookups. Nearest first, unreachable last, ties broken toward the active character so
    /// an alt sitting beside you does not take the badge off you.
    pub(crate) fn card(&self, target: Option<i64>) -> CardChars {
        let (Some(t), true) = (target, self.rings.len() > 1) else {
            return CardChars::default();
        };
        let mut hops: Vec<(CharHop, bool)> = self
            .rings
            .iter()
            .map(|r| {
                let jumps = r.shown.get(&t).copied();
                let via = match (&r.gates, jumps) {
                    (Some(g), Some(j)) => match g.get(&t).copied() {
                        Some(gate) if gate == j => JumpVia::Gates,
                        Some(gate) => JumpVia::BridgeShorter(gate),
                        None => JumpVia::BridgeOnly,
                    },
                    _ => JumpVia::Gates,
                };
                (CharHop { name: r.name.clone(), id: r.id, jumps, via }, r.active)
            })
            .collect();
        hops.sort_by(|(a, a_act), (b, b_act)| {
            let by_dist = match (a.jumps, b.jumps) {
                (Some(x), Some(y)) => x.cmp(&y),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            };
            // Name last so the order is stable: `locations` is a HashMap and iterates arbitrarily.
            by_dist.then(b_act.cmp(a_act)).then_with(|| a.name.cmp(&b.name))
        });
        let selected = hops.iter().position(|(_, active)| *active);
        CardChars { hops: hops.into_iter().map(|(h, _)| h).collect(), selected, ly: CardLy::default() }
    }
}

/// Distances from one origin, cached per graph. Same invalidation story as [`ViaMemo`]: `Systems`
/// is only ever mutated before it is wrapped in its `Arc`, so a bridge edit replaces the `Arc`.
pub(crate) struct BallMemo {
    pub(crate) systems: std::sync::Arc<crate::geo::Systems>,
    pub(crate) balls: std::collections::HashMap<(i64, bool), std::sync::Arc<std::collections::HashMap<i64, u32>>>,
}

pub(crate) fn distance_ball(
    systems: &std::sync::Arc<crate::geo::Systems>,
    from: i64,
    gate_only: bool,
) -> std::sync::Arc<std::collections::HashMap<i64, u32>> {
    thread_local! {
        static MEMO: std::cell::RefCell<Option<BallMemo>> = const { std::cell::RefCell::new(None) };
    }
    MEMO.with(|cell| {
        let mut slot = cell.borrow_mut();
        let hit = matches!(slot.as_ref(), Some(m) if std::sync::Arc::ptr_eq(&m.systems, systems));
        if !hit {
            *slot = Some(BallMemo {
                systems: systems.clone(),
                balls: std::collections::HashMap::new(),
            });
        }
        let memo = slot.as_mut().expect("set above");
        // A ball is ~90KB, so this is a memory bound, not a hit-rate one: characters do not move
        // often enough for the eviction to cost a rebuild in practice.
        if memo.balls.len() >= 16 {
            memo.balls.clear();
        }
        memo.balls
            .entry((from, gate_only))
            .or_insert_with(|| {
                std::sync::Arc::new(if gate_only {
                    systems.gate_distances_from(from, JUMP_SCAN_CAP)
                } else {
                    systems.distances_from(from, JUMP_SCAN_CAP)
                })
            })
            .clone()
    })
}

/// The active character is always a ring, whatever its alert setting, because the card quotes it
/// by name and dropping it would leave the user unable to find their own number.
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_char_rings(
    systems: &Option<std::sync::Arc<crate::geo::Systems>>,
    chars: &[(String, i64)],
    locations: &std::collections::HashMap<String, (i64, bool)>,
    active: &str,
    active_fallback_sys: Option<i64>,
    disabled: &[String],
    only_undocked: bool,
    use_bridges: bool,
) -> CharRings {
    let Some(sys) = systems.as_ref() else {
        return CharRings::default();
    };
    let id_of = |name: &str| {
        chars.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map_or(0, |(_, id)| *id)
    };
    let ring = |name: &str, from: i64, is_active: bool| Ring {
        name: name.to_owned(),
        id: id_of(name),
        shown: distance_ball(sys, from, !use_bridges),
        gates: use_bridges.then(|| distance_ball(sys, from, true)),
        active: is_active,
        from,
    };
    let has_active = !active.is_empty() && active != "No character";
    let mut rings: Vec<Ring> = locations
        .iter()
        .filter(|(name, (_, docked))| {
            name.eq_ignore_ascii_case(active)
                || alert_candidate(name, *docked, disabled, only_undocked)
        })
        .map(|(name, (from, _))| ring(name, *from, name.eq_ignore_ascii_case(active)))
        .collect();
    if has_active && !rings.iter().any(|r| r.active) {
        // Offline, or ESI has no location for it, but `player_system` still answers from its
        // fallback and the feed is already quoting that number.
        if let Some(from) = active_fallback_sys {
            rings.push(ring(active, from, true));
        }
    }
    CharRings { rings, systems: Some(sys.clone()), staging: None }
}

/// Every answer for one graph, one player position and one setting. The graph is held by `Arc`
/// rather than by address, so a replaced graph cannot be mistaken for the one it reused memory
/// from.
pub(crate) struct ViaMemo {
    pub(crate) systems: std::sync::Arc<crate::geo::Systems>,
    pub(crate) player: i64,
    pub(crate) use_bridges: bool,
    pub(crate) by_target: std::collections::HashMap<i64, (u32, JumpVia)>,
}

/// Asked only about a number already computed with bridges on, since the gate-only answer is
/// honest as it stands: a bridge that would merely shorten the player's own trip says nothing
/// about the contact and would mark most of a home-region feed.
///
/// Memoized because a feed redraws up to 250 cards per frame and a gate walk over a map-sized
/// graph runs into hundreds of microseconds each. Nothing here changes between frames unless the
/// player moves, the setting flips or the bridge list is edited, and each of those replaces the
/// key.
pub(crate) fn jump_via(
    systems: &Option<std::sync::Arc<crate::geo::Systems>>,
    player_sys: Option<i64>,
    target: Option<i64>,
    use_bridges: bool,
    shown: Option<u32>,
) -> JumpVia {
    if !use_bridges {
        return JumpVia::Gates;
    }
    let (Some(sys), Some(p), Some(t), Some(j)) = (systems.as_ref(), player_sys, target, shown)
    else {
        return JumpVia::Gates;
    };

    thread_local! {
        static MEMO: std::cell::RefCell<Option<ViaMemo>> =
            const { std::cell::RefCell::new(None) };
    }
    MEMO.with(|cell| {
        let mut cell = cell.borrow_mut();
        let hit = cell.as_ref().is_some_and(|m| {
            std::sync::Arc::ptr_eq(&m.systems, sys) && m.player == p && m.use_bridges == use_bridges
        });
        if !hit {
            *cell = Some(ViaMemo {
                systems: sys.clone(),
                player: p,
                use_bridges,
                by_target: std::collections::HashMap::new(),
            });
        }
        let memo = cell.as_mut().expect("just seeded");
        if let Some(&(had, via)) = memo.by_target.get(&t) {
            if had == j {
                return via;
            }
        }
        // Gates can never beat the bridged graph, so a gate walk capped at the shown number either
        // matches it or proves a bridge is load-bearing. That keeps the common answer inside the
        // ball the first walk already covered, instead of a second full-cap scan on every card.
        let via = if sys.jumps_gates_only(t, p, j).is_some() {
            JumpVia::Gates
        } else {
            match sys.jumps_gates_only(t, p, JUMP_SCAN_CAP) {
                Some(gate) => JumpVia::BridgeShorter(gate),
                None => JumpVia::BridgeOnly,
            }
        };
        memo.by_target.insert(t, (j, via));
        via
    })
}

/// Colour for a card's jump text, and the marker that follows it when a bridge is in the route.
/// Alliance purple because jump bridges are alliance infrastructure, and because the row already
/// spends green on cleared reports and amber and red on threat.
pub(crate) fn jump_chip_style(via: JumpVia) -> (egui::Color32, Option<String>) {
    match via {
        JumpVia::Gates => (crate::theme::standing::CORP, None),
        // Purple is the whole mark. A glyph beside every bridged number is noise in a home region
        // where most of them are, and the tooltip still says which kind of bridge it is.
        JumpVia::BridgeShorter(_) => (crate::theme::standing::ALLIANCE, None),
        // Colour cannot say "there is no gate route at all", so this one keeps its words.
        JumpVia::BridgeOnly => (crate::theme::standing::ALLIANCE, Some("bridge only".to_owned())),
    }
}

pub(crate) fn jump_chip_tip(via: JumpVia, shown: u32) -> Option<String> {
    match via {
        JumpVia::Gates => None,
        JumpVia::BridgeShorter(gate) => Some(format!(
            "{shown}j counts your jump bridges, {gate}j by gate. A hostile can't use your \
             bridges, so {gate}j is how far away they really are."
        )),
        JumpVia::BridgeOnly => Some(format!(
            "Only your jump bridges reach this system, there is no gate route within \
             {JUMP_SCAN_CAP} jumps. A hostile can't get here the way you would."
        )),
    }
}

pub(crate) fn min_jumps_from(
    systems: &Option<std::sync::Arc<crate::geo::Systems>>,
    srcs: &[i64],
    target: Option<i64>,
    use_bridges: bool,
) -> Option<u32> {
    let (sys, t) = (systems.as_ref()?, target?);
    srcs.iter()
        .filter_map(|&s| {
            if use_bridges {
                sys.jumps(t, s, 50)
            } else {
                sys.jumps_gates_only(t, s, 50)
            }
        })
        .min()
}
