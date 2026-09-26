#!/usr/bin/env python3
"""Builds app/assets/whdata.json.gz, the wormhole reference data the app bundles.

Sources, credited in the output:
  - anoik.is static.json: hole types (source/destination class, mass, lifetime, static) and
    every J-space system's class, effect and statics, plus the effect modifier tables.
  - Jambeeno (jambeeno.com, CC BY 4.0): which k-space systems can spawn which Pochven system's
    C729 (https://jambeeno.com/c729_counts).
  - The types anoik lacks (Pochven, Turnur, Tabbetzur) by hand, with CCP SDE dogma numbers.

Run from the repo root: python3 tools/gen_whdata.py
"""
import gzip
import html
import json
import re
import urllib.request

UA = {"User-Agent": "eve-spai whdata generator"}


def fetch(url):
    with urllib.request.urlopen(urllib.request.Request(url, headers=UA), timeout=60) as r:
        return r.read().decode("utf-8", "replace")


# Types anoik does not carry. Mass in kg, lifetime in hours. The Pochven lifetimes are the
# patch 23.02 ones: C729, F216, R081, U372, X450 at 12 hours; I078, L687, O546 at 4.5.
# "pochven" as a class means the Pochven region; "ks" means any k-space security band.
HAND_TYPES = [
    # C729 has been a normal static in Pochven since the March 2026 patch; its K162 is in k-space.
    dict(code="C729", type_id=56546, src=["pochven"], dest="ks", static=True, lifetime_h=12,
         total_mass=1_000_000_000, jump_mass=410_000_000, regen=0),
    dict(code="U372", type_id=56547, src=["ns"], dest="pochven", static=False, lifetime_h=12,
         total_mass=1_000_000_000, jump_mass=375_000_000, regen=0),
    dict(code="X450", type_id=56548, src=["pochven"], dest="ns", static=False, lifetime_h=12,
         total_mass=1_000_000_000, jump_mass=375_000_000, regen=0),
    dict(code="F216", type_id=56549, src=["c2", "c3", "c4", "c5", "c6"], dest="pochven", static=False,
         lifetime_h=12, total_mass=1_000_000_000, jump_mass=375_000_000, regen=0),
    dict(code="R081", type_id=56550, src=["pochven"], dest="c4", static=False, lifetime_h=12,
         total_mass=1_000_000_000, jump_mass=375_000_000, regen=0),
    dict(code="J377", type_id=73749, src=["c1", "c2", "c3", "c4"], dest="turnur", static=False,
         lifetime_h=24, total_mass=1_000_000_000, jump_mass=62_000_000, regen=0),
    dict(code="J492", type_id=0, src=["c1", "c2", "c3", "c4"], dest="tabbetzur", static=False,
         lifetime_h=24, total_mass=1_000_000_000, jump_mass=62_000_000, regen=0),
    # Pochven's internal holes: medium hulls, short-lived.
    dict(code="I078", type_id=0, src=["pochven"], dest="pochven", static=False, lifetime_h=4.5,
         total_mass=100_000_000, jump_mass=62_000_000, regen=0),
    dict(code="L687", type_id=0, src=["pochven"], dest="pochven", static=False, lifetime_h=4.5,
         total_mass=100_000_000, jump_mass=62_000_000, regen=0),
    dict(code="O546", type_id=0, src=["pochven"], dest="pochven", static=False, lifetime_h=4.5,
         total_mass=100_000_000, jump_mass=62_000_000, regen=0),
]


def types_from(anoik):
    out = []
    for code, t in sorted(anoik["wormholes"].items()):
        out.append(dict(
            code=code,
            type_id=t.get("typeID") or 0,
            src=t.get("src") or [],
            dest=t.get("dest") or "",
            static=bool(t.get("static")),
            lifetime_h=t.get("lifetime") or 0,
            total_mass=t.get("total_mass") or 0,
            jump_mass=t.get("max_mass_per_jump") or 0,
            regen=t.get("mass_regen") or 0,
        ))
    known = {t["code"] for t in out}
    out.extend(t for t in HAND_TYPES if t["code"] not in known)
    return out


def systems_from(anoik):
    # [id, class, effect or null, [statics], sun, [planet kinds], moons], compact on purpose:
    # 2600 of them. Celestials are [group, type, ...]: 6 sun, 7 planet, 8 moon.
    kinds = anoik.get("celestialtypes", {})
    name = lambda t: kinds.get(str(t), {}).get("typeName", "")
    rows = []
    for s in anoik["systems"].values():
        cels = s.get("cels") or []
        sun = next((name(c[1]).removeprefix("Sun ") for c in cels if c[0] == 6), "")
        planets = [re.sub(r"^Planet \((.*)\)$", r"\1", name(c[1])) for c in cels if c[0] == 7]
        moons = sum(1 for c in cels if c[0] == 8)
        rows.append([s["solarSystemID"], s.get("wormholeClass") or "", s.get("effectName"), s.get("statics") or [],
                     sun, planets, moons])
    rows.sort()
    return rows


def c729_from(page):
    # Table rows: Region | System | # | Pochven C729(s)
    rows = []
    for tr in re.findall(r"<tr.*?</tr>", page, flags=re.S):
        cells = [html.unescape(re.sub(r"<[^>]+>", "", c)).strip() for c in re.findall(r"<t[dh][^>]*>(.*?)</t[dh]>", tr, flags=re.S)]
        if len(cells) >= 4 and cells[2].isdigit():
            rows.append([cells[1], [p.strip() for p in cells[3].split(",") if p.strip()]])
    rows.sort()
    return rows


def main():
    anoik = json.loads(fetch("https://anoik.is/static/static.json"))
    c729 = c729_from(fetch("https://jambeeno.com/c729_counts"))
    assert len(c729) > 250, f"only {len(c729)} C729 spawn systems parsed"
    data = dict(
        attribution=(
            "Wormhole types, J-space classes, effects and statics: anoik.is. "
            "Pochven C729 spawn zones: Jambeeno (jambeeno.com), CC BY 4.0. "
            "Masses and lifetimes of the types anoik lacks: CCP SDE."
        ),
        anoik_version=anoik.get("version"),
        types=types_from(anoik),
        systems=systems_from(anoik),
        effects=anoik.get("effects", {}),
        c729=c729,
    )
    raw = json.dumps(data, separators=(",", ":"), sort_keys=True).encode()
    # mtime=0 so an unchanged dataset produces an identical file.
    with open("app/assets/whdata.json.gz", "wb") as f:
        f.write(gzip.compress(raw, compresslevel=9, mtime=0))
    print(f"{len(data['types'])} types, {len(data['systems'])} systems, {len(c729)} C729 spawn systems, {len(raw)} bytes raw")


if __name__ == "__main__":
    main()
