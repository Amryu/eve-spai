//! Where the wormhole map puts systems nobody has placed. Tree: a tidy tree, each subtree packed
//! against its neighbours by its outline. Layered: Sugiyama's method via `rust-sugiyama`, for
//! chains whose holes close loops.

use super::wh_graph::node_size;
use std::collections::{HashMap, HashSet, VecDeque};

pub(crate) const COL: f32 = 330.0;
const GAP: f32 = 10.0;
pub(crate) const CHAIN_GAP: f32 = 40.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) enum Style {
    #[default]
    Tree,
    Layered,
}

impl Style {
    pub(crate) fn code(self) -> &'static str {
        match self {
            Style::Tree => "tree",
            Style::Layered => "layered",
        }
    }

    pub(crate) fn from_code(s: &str) -> Self {
        match s {
            "layered" => Style::Layered,
            _ => Style::Tree,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Opts {
    pub style: Style,
    /// Chains packed in rows to this width:height, instead of one after another.
    pub aspect: Option<f32>,
}

impl Opts {
    /// A step to the next level, and a step to the next box beside one.
    pub(crate) fn steps(&self) -> (egui::Vec2, egui::Vec2) {
        (egui::vec2(COL, 0.0), egui::vec2(0.0, super::wh_graph::NODE.y + GAP))
    }
}

pub(crate) fn layout(edges: &[(i64, i64)], alone: &[i64], score: impl Fn(i64) -> i64, opts: Opts) -> Vec<(i64, Option<i64>, egui::Pos2)> {
    let mut adj: HashMap<i64, Vec<i64>> = HashMap::new();
    for n in alone {
        adj.entry(*n).or_default();
    }
    for &(a, b) in edges {
        if a != b {
            adj.entry(a).or_default().push(b);
            adj.entry(b).or_default().push(a);
        }
    }
    for v in adj.values_mut() {
        v.sort_unstable();
        v.dedup();
    }
    let mut nodes: Vec<i64> = adj.keys().copied().collect();
    nodes.sort_unstable();
    let mut seen = HashSet::new();
    let mut chains: Vec<Vec<i64>> = Vec::new();
    for &n in &nodes {
        if seen.insert(n) {
            let mut comp = vec![n];
            let mut q = VecDeque::from([n]);
            while let Some(u) = q.pop_front() {
                for &v in &adj[&u] {
                    if seen.insert(v) {
                        comp.push(v);
                        q.push_back(v);
                    }
                }
            }
            chains.push(comp);
        }
    }
    // A system on its own (a drifter hub nothing reaches yet) after every chain.
    let rank = |c: &Vec<i64>| (c.len() > 1, c.iter().map(|n| score(*n)).max().unwrap_or(0), c.len());
    chains.sort_by_key(|c| std::cmp::Reverse(rank(c)));

    let placed: Vec<Chain> = chains.iter().map(|comp| chain(comp, &adj, &score, opts)).collect();
    let offsets = arrange(&placed, opts);
    let mut out = Vec::new();
    for (c, off) in placed.into_iter().zip(offsets) {
        out.extend(c.nodes.into_iter().map(|(n, parent, p)| (n, parent, p + off)));
    }
    out
}

struct Chain {
    nodes: Vec<(i64, Option<i64>, egui::Pos2)>,
    size: egui::Vec2,
}

/// A box's size across the direction the chain grows.
fn breadth(n: i64) -> f32 {
    node_size(n).y
}

fn chain(comp: &[i64], adj: &HashMap<i64, Vec<i64>>, score: &impl Fn(i64) -> i64, opts: Opts) -> Chain {
    let root = *comp.iter().max_by_key(|n| (score(**n), adj[*n].len(), -**n)).unwrap();
    let mut parent: HashMap<i64, Option<i64>> = HashMap::from([(root, None)]);
    let mut children: HashMap<i64, Vec<i64>> = HashMap::new();
    let mut depth: HashMap<i64, usize> = HashMap::from([(root, 0)]);
    let mut order = vec![root];
    let mut q = VecDeque::from([root]);
    while let Some(u) = q.pop_front() {
        for &v in &adj[&u] {
            if let std::collections::hash_map::Entry::Vacant(e) = parent.entry(v) {
                e.insert(Some(u));
                children.entry(u).or_default().push(v);
                depth.insert(v, depth[&u] + 1);
                order.push(v);
                q.push_back(v);
            }
        }
    }
    let (across, depth): (HashMap<i64, f32>, HashMap<i64, usize>) = match opts.style {
        Style::Layered => layered(comp, adj, &depth),
        Style::Tree => None,
    }
    .unwrap_or_else(|| (tidy(root, &children).0.into_iter().collect(), depth));
    let lo = across.values().copied().fold(f32::INFINITY, f32::min);
    let (level, _) = opts.steps();
    let at = |n: i64| {
        let b = across[&n] - lo;
        let d = depth[&n] as f32;
        egui::pos2(d * level.x, b)
    };
    let nodes: Vec<(i64, Option<i64>, egui::Pos2)> = order.iter().map(|&n| (n, parent[&n], at(n))).collect();
    let size = nodes
        .iter()
        .fold(egui::Vec2::ZERO, |s, (n, _, p)| s.max(p.to_vec2() + node_size(*n)));
    Chain { nodes, size }
}

/// The tidy tree under `n`: each box's near edge along the breadth, and the subtree's outline as
/// the extent it takes at each level down from `n`.
fn tidy(n: i64, children: &HashMap<i64, Vec<i64>>) -> (Vec<(i64, f32)>, Vec<(f32, f32)>) {
    let size = breadth(n);
    let kids = children.get(&n).map(Vec::as_slice).unwrap_or_default();
    if kids.is_empty() {
        return (vec![(n, 0.0)], vec![(0.0, size)]);
    }
    let mut at: Vec<(i64, f32)> = Vec::new();
    let mut outline: Vec<(f32, f32)> = Vec::new();
    let mut kid_mid: Vec<f32> = Vec::new();
    for &k in kids {
        let (sub, line) = tidy(k, children);
        // As close to the siblings before it as their outlines allow, at every level they share.
        let shift = outline
            .iter()
            .zip(&line)
            .map(|(have, next)| have.1 + GAP - next.0)
            .fold(f32::NEG_INFINITY, f32::max);
        let shift = if shift.is_finite() { shift } else { 0.0 };
        kid_mid.push(sub[0].1 + shift + breadth(k) / 2.0);
        at.extend(sub.into_iter().map(|(id, b)| (id, b + shift)));
        for (i, (lo, hi)) in line.into_iter().enumerate() {
            let (lo, hi) = (lo + shift, hi + shift);
            match outline.get_mut(i) {
                Some(o) => *o = (o.0.min(lo), o.1.max(hi)),
                None => outline.push((lo, hi)),
            }
        }
    }
    let mid = (kid_mid[0] + kid_mid[kid_mid.len() - 1]) / 2.0;
    let me = mid - size / 2.0;
    at.insert(0, (n, me));
    outline.insert(0, (me, me + size));
    (at, outline)
}

/// Sugiyama's layered layout of one chain: each box's near edge along the breadth, and its level.
/// `None` for a chain of one, which it has nothing to say about.
fn layered(comp: &[i64], adj: &HashMap<i64, Vec<i64>>, depth: &HashMap<i64, usize>) -> Option<(HashMap<i64, f32>, HashMap<i64, usize>)> {
    if comp.len() < 2 {
        return None;
    }
    let index: HashMap<i64, u32> = comp.iter().enumerate().map(|(i, n)| (*n, i as u32)).collect();
    // The levels are spaced here, not by the library.
    let vertices: Vec<(u32, (f64, f64))> = comp.iter().map(|n| (index[n], (breadth(*n) as f64, 1.0))).collect();
    // Pointing away from the root, so the ranks follow the tree's levels.
    let mut edges: Vec<(u32, u32)> = Vec::new();
    for &a in comp {
        for &b in &adj[&a] {
            if (depth[&a], a) < (depth[&b], b) {
                edges.push((index[&a], index[&b]));
            }
        }
    }
    let config = rust_sugiyama::configure::Config { vertex_spacing: GAP as f64, ..Default::default() };
    let layouts = rust_sugiyama::from_vertices_and_edges(&vertices, &edges, &config);
    let mut out = HashMap::new();
    let mut ys: Vec<(i64, f64)> = Vec::new();
    for (coords, _, _) in layouts {
        for (i, (x, y)) in coords {
            let n = *comp.get(i)?;
            out.insert(n, x as f32 - breadth(n) / 2.0);
            ys.push((n, y));
        }
    }
    let mut levels: Vec<i64> = ys.iter().map(|(_, y)| y.round() as i64).collect();
    levels.sort_unstable();
    levels.dedup();
    let rank: HashMap<i64, usize> = ys.iter().map(|(n, y)| (*n, levels.binary_search(&(y.round() as i64)).unwrap_or(0))).collect();
    (out.len() == comp.len()).then_some((out, rank))
}

fn arrange(chains: &[Chain], opts: Opts) -> Vec<egui::Vec2> {
    let mut out = Vec::with_capacity(chains.len());
    match opts.aspect {
        None => {
            let mut next = 0.0;
            for c in chains {
                out.push(egui::vec2(0.0, next));
                next += c.size.y + CHAIN_GAP;
            }
        }
        Some(aspect) => {
            // Every row width a greedy fill can stop at, keeping the one closest to the window's
            // shape. The order stays, so the most important chain is top left.
            let mut best: Option<(f32, Vec<egui::Vec2>)> = None;
            let mut width = 0.0;
            for c in chains {
                width += c.size.x + CHAIN_GAP;
                let (at, extent) = rows(chains, width);
                let miss = (extent.x / extent.y.max(1.0) / aspect.max(0.1)).ln().abs();
                if best.as_ref().is_none_or(|(m, _)| miss < *m) {
                    best = Some((miss, at));
                }
            }
            out = best.map(|(_, at)| at).unwrap_or_default();
        }
    }
    out
}

/// Chains left to right in rows no wider than `width`: each one's offset, and the whole extent.
fn rows(chains: &[Chain], width: f32) -> (Vec<egui::Vec2>, egui::Vec2) {
    let (mut x, mut y, mut row) = (0.0f32, 0.0f32, 0.0f32);
    let mut extent = egui::Vec2::ZERO;
    let mut at = Vec::with_capacity(chains.len());
    for c in chains {
        if x > 0.0 && x + c.size.x > width {
            x = 0.0;
            y += row + CHAIN_GAP;
            row = 0.0;
        }
        at.push(egui::vec2(x, y));
        extent = extent.max(egui::vec2(x, y) + c.size);
        x += c.size.x + CHAIN_GAP;
        row = row.max(c.size.y);
    }
    (at, extent)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(auto: &[(i64, Option<i64>, egui::Pos2)]) -> HashMap<i64, egui::Rect> {
        auto.iter().map(|(n, _, p)| (*n, egui::Rect::from_min_size(*p, node_size(*n)))).collect()
    }

    fn no_overlap(r: &HashMap<i64, egui::Rect>) {
        let v: Vec<_> = r.iter().collect();
        for i in 0..v.len() {
            for j in i + 1..v.len() {
                assert!(!v[i].1.intersects(*v[j].1), "{} and {} overlap: {:?} {:?}", v[i].0, v[j].0, v[i].1, v[j].1);
            }
        }
    }

    /// A broad tree: 1 with three branches, one long and bushy.
    const TREE: [(i64, i64); 9] = [(1, 2), (1, 3), (1, 4), (2, 5), (2, 6), (5, 7), (5, 8), (3, 9), (4, 10)];

    #[test]
    fn a_short_branch_tucks_in_beside_a_long_one() {
        let opts = Opts::default();
        let auto = layout(&TREE, &[], |n| if n == 1 { 100 } else { 0 }, opts);
        let r = at(&auto);
        no_overlap(&r);
        // Five leaves: five rows, one each, would take 5 * 70 - 20.
        let extent = r.values().fold(egui::Rect::NOTHING, |a, b| a.union(*b));
        assert!(extent.height() < 330.0, "{extent:?}");
    }

    #[test]
    fn every_style_keeps_boxes_apart_and_parents_before_children() {
        let edges: Vec<(i64, i64)> = TREE.iter().copied().chain([(9, 10), (20, 21), (21, 22)]).collect();
        for style in [Style::Tree, Style::Layered] {
            {
                for aspect in [None, Some(1.6)] {
                    let opts = Opts { style, aspect };
                    let auto = layout(&edges, &[30], |n| if n == 1 { 100 } else { 0 }, opts);
                    let r = at(&auto);
                    assert_eq!(r.len(), 14, "{opts:?}");
                    no_overlap(&r);
                    for (n, p, _) in &auto {
                        if let Some(p) = p {
                            let (a, b) = (r[p].min, r[n].min);
                            assert!(a.x < b.x, "{opts:?}: {p} before {n}");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn packed_chains_fill_rows_instead_of_one_long_column() {
        // Tall chains: a hub with four holes out of it.
        let edges: Vec<(i64, i64)> = (0..8).flat_map(|i| (1..=4).map(move |k| (i * 10, i * 10 + k))).collect();
        let stacked = at(&layout(&edges, &[], |_| 0, Opts::default()));
        let packed = at(&layout(&edges, &[], |_| 0, Opts { aspect: Some(1.6), ..Default::default() }));
        no_overlap(&packed);
        let extent = |r: &HashMap<i64, egui::Rect>| r.values().fold(egui::Rect::NOTHING, |a, b| a.union(*b));
        let (s, p) = (extent(&stacked), extent(&packed));
        assert!(p.height() < s.height() / 2.0, "{s:?} {p:?}");
        assert!(p.aspect_ratio() > s.aspect_ratio());
    }
}
