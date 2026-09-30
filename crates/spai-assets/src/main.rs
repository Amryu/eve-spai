//! `spai-assets <eve-spai.db> <universe.json.gz>`: New Eden for the web app, from a database the
//! desktop app has imported EVE's static data into. The database is opened read-only.

use anyhow::{bail, Context as _, Result};
use rusqlite::{Connection, OpenFlags};
use spai_core::universe::{System, Universe};

fn read(db: &Connection) -> Result<Universe> {
    let regions = db
        .prepare("SELECT id, name FROM sde_regions ORDER BY id")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    let systems = db
        .prepare(
            "SELECT s.id, s.name, s.security, COALESCE(s.region_id, 0), COALESCE(c.name, ''), COALESCE(s.faction_id, 0),
                    s.x, s.y, s.z, COALESCE(s.x2d, s.x), COALESCE(s.z2d, s.z)
             FROM sde_systems s LEFT JOIN sde_constellations c ON c.id = s.constellation_id ORDER BY s.id",
        )?
        .query_map([], |r| {
            Ok(System {
                id: r.get(0)?,
                name: r.get(1)?,
                security: r.get(2)?,
                region_id: r.get(3)?,
                constellation: r.get(4)?,
                faction: spai_core::factions::name(r.get(5)?).to_owned(),
                pos: [r.get(6)?, r.get(7)?, r.get(8)?],
                pos2d: [r.get(9)?, r.get(10)?],
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let jumps = db
        .prepare("SELECT from_id, to_id FROM sde_jumps ORDER BY from_id, to_id")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Universe { regions, systems, jumps })
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let [_, db, out] = args.as_slice() else { bail!("usage: spai-assets <eve-spai.db> <universe.json.gz>") };
    let db = Connection::open_with_flags(db, OpenFlags::SQLITE_OPEN_READ_ONLY).with_context(|| format!("opening {db}"))?;
    let u = read(&db)?;
    if u.systems.len() < 8000 || u.jumps.is_empty() {
        bail!("only {} systems and {} jumps: import the static data in EVE Spai first", u.systems.len(), u.jumps.len());
    }
    let gz = u.to_gz();
    std::fs::write(out, &gz)?;
    println!("{} regions, {} systems, {} jumps: {} KB", u.regions.len(), u.systems.len(), u.jumps.len(), gz.len() / 1024);
    Ok(())
}
