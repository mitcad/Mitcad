// SPDX-License-Identifier: MIT
//! Corner reliefs where a Lofted Flange's bends meet in a point
//! (mitcad#2576), and the facets' outlines the bends and the reliefs cut.
//!
//! A press brake transition's facets fan into points of the profiles: a
//! rectangle's corner against an arc, a side's triangle against a point of
//! a circle, the triangles of a triangulated transition. Bends of one
//! radius would run into each other there, so each such point (a tip)
//! gets a relief:
//!
//! - every facet that meets the tip is cut straight across, between the
//!   points the relief's size from the tip along its two edges there (laid
//!   flat, a polygon inscribed in the circle of that radius about the tip);
//! - every bend through the tip ends on the plane through the points where
//!   the cuts of the facets beside it meet its two tangent lines, as square
//!   to the bend as a plane through both can be.
//!
//! The facets are cut as convex polygons ([`Outline`]) by the bends'
//! setbacks and the reliefs' cuts, and made strips again
//! ([`strips_between`]); the bends' ends lie on those cuts. The cut walls
//! are square to the sheet, as the profiles' are.

use mitcad_vec3::{add, cross, distance, dot, scale, sub};

use super::{NEAR, Plane, Side, Strip, V3, lerp3, near, polyline_length, triangulated, unit3};

/// What an edge of a facet's outline runs along.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Edge {
    /// The first profile's side, or a relief's cut about a tip on it.
    First,
    /// The facet's last ruling, or the next bend's tangent line.
    End,
    /// The second profile's side, or a relief's cut about a tip on it.
    Second,
    /// The facet's first ruling, or the previous bend's tangent line.
    Start,
}

/// A flat facet's outline: its corners in order, each with the edge from it
/// to the next one.
pub(super) type Outline = Vec<(V3, Edge)>;

/// A point of a profile where facets with bends beside them meet in a
/// point: the strips with a side ending there.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Tip {
    pub point: V3,
    /// On the second profile.
    pub second: bool,
    pub strips: Vec<usize>,
}

/// A facet's cut about a tip.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Cut {
    /// The tip's index.
    pub tip: usize,
    /// The plane square to the facet through the cut; the facet is kept on
    /// the side its normal points to, away from the tip.
    pub plane: Plane,
    /// The unit direction of one of the facet's edges from the tip.
    pub along: V3,
}

impl Cut {
    /// The relief's size that would put the cut through `p`.
    pub fn size_through(&self, tip: V3, p: V3) -> f64 {
        dot(sub(p, tip), self.plane.1) / dot(self.along, self.plane.1)
    }
}

/// The outline without corners on the next one (an edge without length).
fn tidy(mut o: Outline) -> Outline {
    let mut i = 0;
    while o.len() > 1 && i < o.len() {
        let j = (i + 1) % o.len();
        if distance(o[i].0, o[j].0) <= NEAR {
            o.remove(i);
        } else {
            i += 1;
        }
    }
    o
}

/// A flat strip's outline: along its first side, across its last ruling,
/// back along its second side and across its first ruling.
pub(super) fn outline_of(s: &Strip) -> Outline {
    tidy(vec![
        (s.a.point(0.0), Edge::First),
        (s.a.point(1.0), Edge::End),
        (s.b.point(1.0), Edge::Second),
        (s.b.point(0.0), Edge::Start),
    ])
}

/// The part of an outline on the side of `plane` its normal points to,
/// the plane's cut an edge of the kind `edge`.
pub(super) fn clipped(o: &Outline, (origin, normal): Plane, edge: Edge) -> Outline {
    let g = |x: V3| dot(sub(x, origin), normal);
    let n = o.len();
    let mut out = Vec::with_capacity(n + 1);
    for i in 0..n {
        let (here, kind) = o[i];
        let next = o[(i + 1) % n].0;
        let (gh, gn) = (g(here), g(next));
        if gh >= 0.0 {
            out.push((here, kind));
            if gn < 0.0 {
                out.push((lerp3(here, next, gh / (gh - gn)), edge));
            }
        } else if gn >= 0.0 {
            out.push((lerp3(here, next, gh / (gh - gn)), kind));
        }
    }
    tidy(out)
}

/// The end towards the first profile (or the second, with `second`) of an
/// outline's start or end edge.
pub(super) fn edge_end(o: &Outline, edge: Edge, second: bool) -> Option<V3> {
    let n = o.len();
    let i = (0..n).find(|&j| o[j].1 == edge && o[(j + 1) % n].1 != edge)?;
    let (from, to) = (o[i].0, o[(i + 1) % n].0);
    // A start edge runs from the second profile to the first, an end edge
    // from the first to the second.
    Some(match (edge, second) {
        (Edge::Start, false) | (Edge::End, true) => to,
        _ => from,
    })
}

/// The outline's corners along its first side, from its start edge to its
/// end edge, and along its second side the same way; None where its edges
/// are not in that order (a cut reaching past the facet).
pub(super) fn chains(o: &Outline) -> Option<(Vec<V3>, Vec<V3>)> {
    let n = o.len();
    if n < 3 {
        return None;
    }
    let start = (0..n).find(|&i| o[i].1 == Edge::Start && o[(i + 1) % n].1 != Edge::Start)?;
    let mut first = Vec::new();
    let mut second = Vec::new();
    let mut i = (start + 1) % n;
    let mut on_first = true;
    for _ in 0..n {
        let (p, kind) = o[i];
        match (on_first, kind) {
            (true, Edge::First) => first.push(p),
            (true, Edge::End) => {
                first.push(p);
                on_first = false;
            }
            (false, Edge::Second) => second.push(p),
            (false, Edge::Start) => {
                second.push(p);
                if i != start {
                    return None;
                }
                second.reverse();
                return Some((first, second));
            }
            _ => return None,
        }
        i = (i + 1) % n;
    }
    None
}

/// A facet's strips between its two sides' corners (each from its start
/// edge to its end edge): one strip of two points or segments, or
/// triangles across the shorter diagonals.
pub(super) fn strips_between(first: &[V3], second: &[V3]) -> Vec<Strip> {
    let side = |c: &[V3]| match c {
        [p] => Some(Side::Point(*p)),
        [p, q] => Some(Side::Segment(*p, *q)),
        _ => None,
    };
    match (side(first), side(second)) {
        (Some(a), Some(b)) => vec![Strip { a, b }],
        _ => triangulated(first, second),
    }
}

/// The length of a side's corners.
pub(super) fn length_of(chain: &[V3]) -> f64 {
    polyline_length(chain, false)
}

/// The tips of the strips: the points of a profile that are a whole side
/// of a strip with a bend beside it (`bent[i]`: a bend between strip `i`
/// and `i + 1`).
pub(super) fn tips(strips: &[Strip], bent: &[bool]) -> Vec<Tip> {
    let mut out: Vec<Tip> = Vec::new();
    for (k, s) in strips.iter().enumerate() {
        let beside = (k > 0 && bent[k - 1]) || bent.get(k).copied().unwrap_or(false);
        if !beside {
            continue;
        }
        for (second, side) in [(false, s.a), (true, s.b)] {
            let p = side.point(0.0);
            if side.is_point() && !out.iter().any(|t| t.second == second && near(t.point, p)) {
                out.push(Tip {
                    point: p,
                    second,
                    strips: Vec::new(),
                });
            }
        }
    }
    for tip in &mut out {
        tip.strips = strips
            .iter()
            .enumerate()
            .filter(|(_, s)| {
                let side = if tip.second { s.b } else { s.a };
                near(side.point(0.0), tip.point) || near(side.point(1.0), tip.point)
            })
            .map(|(k, _)| k)
            .collect();
    }
    out
}

/// A flat strip's cut about tip `index` at `point`, `size` from it along
/// both its edges there; None where an edge there is not longer than that.
pub(super) fn facet_cut(s: &Strip, index: usize, point: V3, size: f64) -> Option<Cut> {
    let o = outline_of(s);
    let n = o.len();
    let i = (0..n).find(|&j| near(o[j].0, point))?;
    let (prev, next) = (o[(i + n - 1) % n].0, o[(i + 1) % n].0);
    if distance(prev, point) <= size + NEAR || distance(next, point) <= size + NEAR {
        return None;
    }
    let (u1, u2) = (unit3(sub(prev, point))?, unit3(sub(next, point))?);
    let (x1, x2) = (add(point, scale(u1, size)), add(point, scale(u2, size)));
    let mut normal = unit3(cross(cross(u2, u1), sub(x2, x1)))?;
    if dot(sub(point, x1), normal) > 0.0 {
        normal = scale(normal, -1.0);
    }
    Some(Cut {
        tip: index,
        plane: (x1, normal),
        along: u1,
    })
}

/// The plane a bend through a tip ends on: through `a` and `b` (where the
/// facets' cuts meet its tangent lines), as square to its `axis` as a
/// plane through both can be.
pub(super) fn bend_end(a: V3, b: V3, axis: V3) -> Option<Plane> {
    let d = sub(b, a);
    let dd = dot(d, d);
    if dd <= NEAR * NEAR {
        return None;
    }
    let normal = unit3(sub(axis, scale(d, dot(axis, d) / dd)))?;
    Some((a, normal))
}
