//! The market's multibuy list: one `Name xN` line per type, the hull first, everything the fit
//! needs counted together.

use crate::dogma::Dogma;
use crate::fit::Fit;
use crate::ids::TypeId;

use super::name_of;

pub fn export(dogma: &Dogma, fit: &Fit) -> String {
    let mut items: Vec<(TypeId, u32)> = Vec::new();
    let mut push = |t: TypeId, n: u32| match items.iter_mut().find(|(x, _)| *x == t) {
        Some((_, q)) => *q += n,
        None => items.push((t, n)),
    };
    push(fit.ship, 1);
    for m in &fit.modules {
        push(m.type_id, 1);
    }
    for c in fit.modules.iter().filter_map(|m| m.charge) {
        push(c, 1);
    }
    for d in &fit.drones {
        push(d.type_id, d.count);
    }
    for f in &fit.fighters {
        push(f.type_id, f.count);
    }
    for &i in &fit.implants {
        push(i, 1);
    }
    for b in &fit.boosters {
        push(b.type_id, 1);
    }
    for c in &fit.cargo {
        push(c.type_id, c.quantity);
    }
    items.iter().map(|(t, n)| format!("{} x{n}", name_of(dogma, *t))).collect::<Vec<_>>().join("\n")
}
