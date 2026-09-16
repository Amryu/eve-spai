/// The waypoints a planned route puts into the game, mirroring `web::route::ingame_waypoints`.
///
/// The autopilot flies a gate route whole, so every system is a waypoint. A leg the pilot flies by
/// hand gets only its two ends, plus the waypoints the user named and the branches they picked.
/// Its own module so the rule can be run on its own against the Rust one: the wrong set of waypoints
/// looks like nothing in the browser and wipes the route in the game.
export function ingameWaypoints(o, player) {
  if (!o?.path?.length) return [];
  const out = [];
  const start = o.path[0];
  if (player !== start) out.push(start);
  if ((o.hops ?? []).every((h) => !h.kind)) {
    out.push(...o.path.slice(1));
  } else {
    o.hops.forEach((h, i) => {
      // A system the user named is a waypoint in the game too, or the route arrives there by
      // whatever way the game likes, or not at all.
      if (h.anchor && i > 0 && out[out.length - 1] !== h.id) out.push(h.id);
      // A fork the autopilot would take the other way round needs the branch pinned.
      if (h.fork?.length && o.path[i + 1] != null) out.push(o.path[i + 1]);
      if (!h.kind) return;
      if (i > 0 && out[out.length - 1] !== o.hops[i - 1].id) out.push(o.hops[i - 1].id);
      out.push(h.id);
    });
    out.push(o.path[o.path.length - 1]);
  }
  const uniq = out.filter((id, i) => i === 0 || id !== out[i - 1]);
  return uniq[0] === player ? uniq.slice(1) : uniq;
}

