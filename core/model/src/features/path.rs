// SPDX-License-Identifier: MIT
//! Paths of the sweep family (F3): what a sweep or a pipe runs along, a
//! sweep's guide rail, a loft's centre line and rails, and the curves of
//! ribs and webs. A path is curves of a sketch (S1's entity ids, the
//! sketch-curves form of [`GeomRef`](super::GeomRef)) or a [`PathRef`]:
//!
//! | JSON | Meaning |
//! |---|---|
//! | `{"sketch": "F1", "curves": ["c1", "c2"]}` | the curves, in any order, joined end to end |
//! | `{"sketch": "F1", "curves": ["c1"], "chain": true}` | also every curve joined to them end to end (chain selection; construction curves only when listed) |
//! | `{"sketch": "F1", "curve": "c1"}` | one curve ([`PathRef`]) |
//! | `{"body": "F2.b0", "edges": ["E{…}", …]}` | connected edges of a body ([`PathRef`]) |
//! | `{"start": [x, y, z], "end": [x, y, z]}` | a fixed line ([`PathRef`]) |
//!
//! Evaluation gives the curves in model space in order along the path,
//! each running along it ([`PathCurve`]). The path starts at the free end
//! of the first listed curve when that curve ends the chain; otherwise it
//! runs along the first listed curve's own direction (a closed chain from
//! that curve's start). A path may not branch.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::geom_ref::PathRef;
use super::{CheckContext, EvalContext, FeatureDef, References, is_false};
use crate::ids::{BodyUid, EntityUid, FeatureUid};
use crate::kernel::{Curve3, Kernel};
use crate::profile::SketchFrame;
use crate::sketch::geometry::Curve2;
use crate::sweeps::PathCurve;
use crate::topo::{EdgeName, TopoName};

const TAU: f64 = std::f64::consts::TAU;

/// Curves of a sketch (see the module documentation).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SketchCurves {
    pub sketch: FeatureUid,
    #[serde(with = "curves_serde")]
    pub curves: Vec<EntityUid>,
    /// Adds the curves joined to them end to end.
    #[serde(default, skip_serializing_if = "is_false")]
    pub chain: bool,
}

/// A path: curves of a sketch, or a [`PathRef`] (edges of a body, one
/// sketch curve, a fixed line).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum CurvePath {
    Sketch(SketchCurves),
    Path(PathRef),
}

impl<'de> Deserialize<'de> for CurvePath {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        let value = Value::deserialize(deserializer)?;
        if value.get("curves").is_some() || value.get("chain").is_some() {
            serde_json::from_value(value)
                .map(Self::Sketch)
                .map_err(D::Error::custom)
        } else {
            PathRef::from_value(value)
                .map(Self::Path)
                .map_err(D::Error::custom)
        }
    }
}

/// Curve ids in files and commands: `["c1", "c2"]`.
mod curves_serde {
    use serde::Deserialize;

    use crate::ids::EntityUid;

    pub fn serialize<S: serde::Serializer>(ids: &[EntityUid], s: S) -> Result<S::Ok, S::Error> {
        s.collect_seq(ids.iter().map(|id| id.curve_name()))
    }

    pub fn deserialize<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<EntityUid>, D::Error> {
        Vec::<String>::deserialize(d)?
            .iter()
            .map(|n| EntityUid::parse_curve(n).map_err(serde::de::Error::custom))
            .collect()
    }
}

impl SketchCurves {
    pub fn add_references(&self, references: &mut References) {
        references.features.insert(self.sketch);
    }

    pub fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        if self.curves.is_empty() {
            return Err("no curves selected".to_owned());
        }
        let (entry, def) = ctx.sketch(self.sketch)?;
        for (i, curve) in self.curves.iter().enumerate() {
            if self.curves[..i].contains(curve) {
                return Err(format!(
                    "curve {} is listed more than once",
                    curve.curve_name()
                ));
            }
            if !def.entity(*curve).is_some_and(|e| !e.is_point()) {
                return Err(format!(
                    "{} has no curve {}",
                    entry.name,
                    curve.curve_name()
                ));
            }
        }
        Ok(())
    }

    /// The sketch's frame and the curves in model space, the listed ones
    /// first in their order, then those `chain` adds.
    pub fn curves<K: Kernel>(
        &self,
        ctx: &mut EvalContext<'_, K>,
    ) -> Result<(SketchFrame, Vec<PathCurve>), String> {
        let output = ctx.sketch(self.sketch)?;
        let name = ctx.feature_name(self.sketch);
        let solved = &output.solved;
        for id in &self.curves {
            if !solved.curves.contains_key(id) {
                return Err(format!("{name} has no curve {}", id.curve_name()));
            }
        }
        let mut ids = self.curves.clone();
        if self.chain {
            let construction = construction_curves(ctx, self.sketch);
            let candidates: Vec<(EntityUid, &Curve2)> = solved
                .curves
                .iter()
                .filter(|(id, _)| !construction.contains(id) && !ids.contains(id))
                .map(|(id, c)| (*id, c))
                .collect();
            ids.extend(joined(&ids, solved, &candidates));
        }
        let pieces = ids
            .iter()
            .map(|id| PathCurve {
                name: id.curve_name(),
                curve: curve3(&output.frame, &solved.curves[id]),
            })
            .collect();
        Ok((output.frame, pieces))
    }
}

/// The construction curves of a sketch, which chained selection skips.
fn construction_curves<K: Kernel>(
    ctx: &EvalContext<'_, K>,
    sketch: FeatureUid,
) -> BTreeSet<EntityUid> {
    ctx.env
        .features
        .iter()
        .find(|f| f.uid == sketch)
        .and_then(|f| match &f.def {
            FeatureDef::Sketch(def) => Some(def),
            _ => None,
        })
        .map(|def| {
            def.entities
                .iter()
                .filter(|e| e.construction)
                .map(|e| e.id)
                .collect()
        })
        .unwrap_or_default()
}

/// The candidates joined to the curves end to end, directly or through
/// other candidates, in the order they are reached.
fn joined(
    seeds: &[EntityUid],
    solved: &crate::sketch::solve::Solved,
    candidates: &[(EntityUid, &Curve2)],
) -> Vec<EntityUid> {
    let mut ends: Vec<[f64; 2]> = Vec::new();
    let mut size: f64 = 1.0;
    for id in seeds {
        if let Some((a, b)) = solved.curves.get(id).and_then(Curve2::ends) {
            ends.extend([a, b]);
        }
    }
    for (_, curve) in candidates {
        if let Some((a, b)) = curve.ends() {
            size = size
                .max(a[0].abs())
                .max(a[1].abs())
                .max(b[0].abs())
                .max(b[1].abs());
        }
    }
    let tolerance = 1e-6 * size;
    let near = |p: [f64; 2], q: [f64; 2]| (p[0] - q[0]).hypot(p[1] - q[1]) <= tolerance;
    let mut added = Vec::new();
    let mut taken = vec![false; candidates.len()];
    loop {
        let mut grew = false;
        for (i, (id, curve)) in candidates.iter().enumerate() {
            let Some((a, b)) = curve.ends() else {
                continue;
            };
            if taken[i] || !ends.iter().any(|e| near(*e, a) || near(*e, b)) {
                continue;
            }
            taken[i] = true;
            grew = true;
            ends.extend([a, b]);
            added.push(*id);
        }
        if !grew {
            return added;
        }
    }
}

impl CurvePath {
    pub fn add_references(&self, references: &mut References) {
        match self {
            Self::Sketch(curves) => curves.add_references(references),
            Self::Path(path) => path.add_to(references),
        }
    }

    pub fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        match self {
            Self::Sketch(curves) => curves.check(ctx),
            Self::Path(path) => path.check(ctx),
        }
    }

    /// The path's curves in order, each running along it.
    pub fn resolve<K: Kernel>(
        &self,
        ctx: &mut EvalContext<'_, K>,
    ) -> Result<Vec<PathCurve>, String> {
        let pieces = match self {
            Self::Sketch(curves) => curves.curves(ctx)?.1,
            Self::Path(PathRef::Edges { body, edges }) => edge_curves(ctx, *body, edges)?,
            Self::Path(PathRef::SketchCurve { sketch, curve }) => {
                let curves = SketchCurves {
                    sketch: *sketch,
                    curves: vec![*curve],
                    chain: false,
                };
                curves.curves(ctx)?.1
            }
            Self::Path(PathRef::Line { start, end }) => vec![PathCurve {
                name: "line".to_owned(),
                curve: Curve3::Line {
                    start: *start,
                    end: *end,
                },
            }],
        };
        chain(pieces)
    }
}

/// The curves of a body's edges, in the order listed.
fn edge_curves<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    body: BodyUid,
    edges: &[EdgeName],
) -> Result<Vec<PathCurve>, String> {
    let shape = ctx.body(body)?;
    let mut pieces = Vec::new();
    for edge in edges {
        let curves = ctx
            .kernel
            .curves_of(&shape, &TopoName::Edge(edge.clone()))
            .map_err(|e| format!("path: {e}"))?;
        let mut curves = curves.into_iter();
        match (curves.next(), curves.next()) {
            (None, _) => return Err(format!("path: {}", ctx.missing_edge(body, edge))),
            (Some(curve), None) => pieces.push(PathCurve {
                name: edge.to_string(),
                curve,
            }),
            (Some(_), Some(_)) => {
                return Err(format!(
                    "path: {edge} names several edges; name one with #k"
                ));
            }
        }
    }
    Ok(pieces)
}

/// A sketch curve in model space.
pub(crate) fn curve3(frame: &SketchFrame, curve: &Curve2) -> Curve3 {
    let normal = frame.normal();
    let direction = |angle: f64| -> [f64; 3] {
        std::array::from_fn(|i| angle.cos() * frame.x_axis[i] + angle.sin() * frame.y_axis[i])
    };
    let after = |start: f64, end: f64| if end <= start { end + TAU } else { end };
    let conic = |center, x_axis, major, minor, start, end, closed| Curve3::Conic {
        center: frame.point(center),
        normal,
        x_axis,
        major,
        minor,
        start,
        end,
        closed,
    };
    match curve {
        Curve2::Line { a, b } => Curve3::Line {
            start: frame.point(*a),
            end: frame.point(*b),
        },
        Curve2::Circle { center, radius } => {
            conic(*center, frame.x_axis, *radius, *radius, 0.0, TAU, true)
        }
        Curve2::Arc {
            center,
            radius,
            start,
            end,
        } => conic(
            *center,
            frame.x_axis,
            *radius,
            *radius,
            *start,
            after(*start, *end),
            false,
        ),
        Curve2::Ellipse {
            center,
            major,
            minor,
            rotation,
        } => conic(
            *center,
            direction(*rotation),
            *major,
            *minor,
            0.0,
            TAU,
            true,
        ),
        Curve2::EllipticalArc {
            center,
            major,
            minor,
            rotation,
            start,
            end,
        } => conic(
            *center,
            direction(*rotation),
            *major,
            *minor,
            *start,
            after(*start, *end),
            false,
        ),
        Curve2::Nurbs(n) => Curve3::BSpline {
            degree: n.degree as u32,
            poles: n.control.iter().map(|p| frame.point(*p)).collect(),
            weights: n.weights.clone(),
            knots: n.knots.clone(),
        },
    }
}

fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|i| a[i] + b[i])
}

fn scaled(a: [f64; 3], s: f64) -> [f64; 3] {
    a.map(|v| v * s)
}

fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    (0..3).map(|i| (a[i] - b[i]).powi(2)).sum::<f64>().sqrt()
}

/// A point of a conic at parameter `t`.
fn conic_point(
    center: [f64; 3],
    normal: [f64; 3],
    x_axis: [f64; 3],
    (major, minor): (f64, f64),
    t: f64,
) -> [f64; 3] {
    let y_axis = crate::profile::cross(normal, x_axis);
    add(
        center,
        add(
            scaled(x_axis, major * t.cos()),
            scaled(y_axis, minor * t.sin()),
        ),
    )
}

/// A point of a B-spline with its full knot vector (de Boor, rational in
/// homogeneous coordinates).
fn bspline_point(
    degree: usize,
    poles: &[[f64; 3]],
    weights: &[f64],
    knots: &[f64],
    t: f64,
) -> [f64; 3] {
    let n = poles.len();
    let p = degree;
    let mut k = p;
    while k + 1 < n && knots[k + 1] <= t {
        k += 1;
    }
    let mut d: Vec<[f64; 4]> = (0..=p)
        .map(|j| {
            let i = k + j - p;
            let w = weights.get(i).copied().unwrap_or(1.0);
            let q = poles[i];
            [q[0] * w, q[1] * w, q[2] * w, w]
        })
        .collect();
    for r in 1..=p {
        for j in (r..=p).rev() {
            let i = k + j - p;
            let span = knots[i + p + 1 - r] - knots[i];
            let alpha = if span == 0.0 {
                0.0
            } else {
                (t - knots[i]) / span
            };
            d[j] = std::array::from_fn(|c| (1.0 - alpha) * d[j - 1][c] + alpha * d[j][c]);
        }
    }
    let h = d[p];
    [h[0] / h[3], h[1] / h[3], h[2] / h[3]]
}

/// The start and end of an open curve; None for a closed one.
pub(crate) fn curve_ends(curve: &Curve3) -> Option<([f64; 3], [f64; 3])> {
    match curve {
        Curve3::Point(p) => Some((*p, *p)),
        Curve3::Line { start, end } => Some((*start, *end)),
        Curve3::Conic { closed: true, .. } => None,
        Curve3::Conic {
            center,
            normal,
            x_axis,
            major,
            minor,
            start,
            end,
            ..
        } => {
            let at = |t| conic_point(*center, *normal, *x_axis, (*major, *minor), t);
            Some((at(*start), at(*end)))
        }
        Curve3::BSpline {
            degree,
            poles,
            weights,
            knots,
        } => {
            let p = *degree as usize;
            if p == 0 || poles.len() <= p || knots.len() != poles.len() + p + 1 {
                return poles.first().zip(poles.last()).map(|(a, b)| (*a, *b));
            }
            let (t0, t1) = (knots[p], knots[poles.len()]);
            let a = bspline_point(p, poles, weights, knots, t0);
            let b = bspline_point(p, poles, weights, knots, t1);
            let size = poles.iter().flatten().fold(1.0_f64, |m, v| m.max(v.abs()));
            (distance(a, b) > 1e-9 * size).then_some((a, b))
        }
    }
}

/// The curve run the other way.
pub(crate) fn reversed(curve: Curve3) -> Curve3 {
    match curve {
        Curve3::Point(p) => Curve3::Point(p),
        Curve3::Line { start, end } => Curve3::Line {
            start: end,
            end: start,
        },
        // With the normal reversed, the point at t is the old point at -t.
        Curve3::Conic {
            center,
            normal,
            x_axis,
            major,
            minor,
            start,
            end,
            closed,
        } => Curve3::Conic {
            center,
            normal: normal.map(|v| -v),
            x_axis,
            major,
            minor,
            start: -end,
            end: -start,
            closed,
        },
        Curve3::BSpline {
            degree,
            mut poles,
            mut weights,
            knots,
        } => {
            poles.reverse();
            weights.reverse();
            let (first, last) = (knots[0], knots[knots.len() - 1]);
            let knots = knots.iter().rev().map(|k| first + last - k).collect();
            Curve3::BSpline {
                degree,
                poles,
                weights,
                knots,
            }
        }
    }
}

/// True when the path's last curve ends where its first starts (or it is
/// one closed curve).
pub(crate) fn is_closed(path: &[PathCurve]) -> bool {
    let (Some(first), Some(last)) = (path.first(), path.last()) else {
        return false;
    };
    match (curve_ends(&first.curve), curve_ends(&last.curve)) {
        (None, _) => true,
        (Some((start, _)), Some((_, end))) => distance(start, end) <= tolerance(path),
        _ => false,
    }
}

/// The path run the other way.
pub(crate) fn reversed_path(path: Vec<PathCurve>) -> Vec<PathCurve> {
    path.into_iter()
        .rev()
        .map(|p| PathCurve {
            name: p.name,
            curve: reversed(p.curve),
        })
        .collect()
}

/// Ends closer than this join.
fn tolerance(path: &[PathCurve]) -> f64 {
    let mut size: f64 = 1.0;
    for piece in path {
        if let Some((a, b)) = curve_ends(&piece.curve) {
            size = a.iter().chain(&b).fold(size, |m, v| m.max(v.abs()));
        }
    }
    1e-6 * size
}

/// Orders the curves into one chain along the path (see the module
/// documentation for its direction).
pub(crate) fn chain(pieces: Vec<PathCurve>) -> Result<Vec<PathCurve>, String> {
    if pieces.is_empty() {
        return Err("the path has no curves".to_owned());
    }
    let ends: Vec<Option<([f64; 3], [f64; 3])>> =
        pieces.iter().map(|p| curve_ends(&p.curve)).collect();
    if let Some(i) = ends.iter().position(Option::is_none) {
        if pieces.len() == 1 {
            return Ok(pieces);
        }
        return Err(format!(
            "{} is a closed curve; it can only be a path by itself",
            pieces[i].name
        ));
    }
    let tolerance = tolerance(&pieces);
    let mut nodes: Vec<[f64; 3]> = Vec::new();
    let mut node_of = |p: [f64; 3]| -> usize {
        if let Some(i) = nodes.iter().position(|q| distance(*q, p) <= tolerance) {
            return i;
        }
        nodes.push(p);
        nodes.len() - 1
    };
    let links: Vec<(usize, usize)> = ends
        .iter()
        .map(|e| {
            let (a, b) = e.expect("open");
            (node_of(a), node_of(b))
        })
        .collect();
    let mut degree = vec![0usize; nodes.len()];
    for (i, (a, b)) in links.iter().enumerate() {
        if a == b {
            return Err(format!("{} closes on itself", pieces[i].name));
        }
        degree[*a] += 1;
        degree[*b] += 1;
    }
    if let Some(node) = degree.iter().position(|d| *d > 2) {
        let p = nodes[node];
        return Err(format!(
            "the path branches at ({:.6}, {:.6}, {:.6})",
            p[0], p[1], p[2]
        ));
    }
    let free = |node: usize| degree[node] == 1;
    let (a0, b0) = links[0];
    let start = if pieces.len() > 1 && free(a0) {
        a0
    } else if pieces.len() > 1 && free(b0) {
        b0
    } else if degree.contains(&1) {
        // The first curve lies inside the chain: back from its start to the
        // free end.
        let mut used = vec![false; pieces.len()];
        used[0] = true;
        let mut node = a0;
        while let Some(i) =
            (0..links.len()).find(|i| !used[*i] && (links[*i].0 == node || links[*i].1 == node))
        {
            used[i] = true;
            node = if links[i].0 == node {
                links[i].1
            } else {
                links[i].0
            };
        }
        node
    } else {
        a0
    };
    let mut used = vec![false; pieces.len()];
    let mut order: Vec<(usize, bool)> = Vec::new();
    let mut node = start;
    loop {
        let incident = |i: usize| !used[i] && (links[i].0 == node || links[i].1 == node);
        let next = if incident(0) && (links[0].0 == node || order.is_empty()) {
            Some(0)
        } else {
            (0..links.len()).find(|i| incident(*i))
        };
        let Some(i) = next else {
            break;
        };
        used[i] = true;
        let forward = links[i].0 == node;
        node = if forward { links[i].1 } else { links[i].0 };
        order.push((i, forward));
    }
    if let Some(i) = used.iter().position(|u| !u) {
        return Err(format!(
            "the path's curves are not connected: {} does not join the others",
            pieces[i].name
        ));
    }
    let mut pieces: Vec<Option<PathCurve>> = pieces.into_iter().map(Some).collect();
    Ok(order
        .into_iter()
        .map(|(i, forward)| {
            let piece = pieces[i].take().expect("each piece once");
            if forward {
                piece
            } else {
                PathCurve {
                    name: piece.name,
                    curve: reversed(piece.curve),
                }
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(name: &str, a: [f64; 3], b: [f64; 3]) -> PathCurve {
        PathCurve {
            name: name.to_owned(),
            curve: Curve3::Line { start: a, end: b },
        }
    }

    fn names(path: &[PathCurve]) -> Vec<String> {
        path.iter()
            .map(|p| {
                let Some((a, b)) = curve_ends(&p.curve) else {
                    return p.name.clone();
                };
                format!("{}:{:?}->{:?}", p.name, a, b)
            })
            .collect()
    }

    #[test]
    fn chains_start_at_the_free_end_of_the_first_curve() {
        let c1 = line("c1", [0.0; 3], [40.0, 0.0, 0.0]);
        let c2 = line("c2", [60.0, 20.0, 0.0], [40.0, 0.0, 0.0]);
        let c3 = line("c3", [60.0, 20.0, 0.0], [60.0, 60.0, 0.0]);
        let path = chain(vec![c1.clone(), c3.clone(), c2.clone()]).unwrap();
        assert_eq!(
            names(&path),
            [
                "c1:[0.0, 0.0, 0.0]->[40.0, 0.0, 0.0]",
                "c2:[40.0, 0.0, 0.0]->[60.0, 20.0, 0.0]",
                "c3:[60.0, 20.0, 0.0]->[60.0, 60.0, 0.0]"
            ]
        );
        // The last curve first: from its free end backwards.
        let path = chain(vec![c3.clone(), c1.clone(), c2.clone()]).unwrap();
        assert_eq!(path[0].name, "c3");
        assert_eq!(curve_ends(&path[0].curve).unwrap().0, [60.0, 60.0, 0.0]);
        // A curve inside the chain: along its own direction (c2 runs from
        // (60, 20) to (40, 0), so the path starts at c3's free end).
        let path = chain(vec![c2, c1, c3]).unwrap();
        assert_eq!(
            path.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(),
            ["c3", "c2", "c1"]
        );
        assert_eq!(curve_ends(&path[2].curve).unwrap().1, [0.0; 3]);
    }

    #[test]
    fn branches_gaps_and_closed_curves_are_errors() {
        let a = line("c1", [0.0; 3], [10.0, 0.0, 0.0]);
        let b = line("c2", [10.0, 0.0, 0.0], [20.0, 0.0, 0.0]);
        let c = line("c3", [10.0, 0.0, 0.0], [10.0, 10.0, 0.0]);
        let far = line("c4", [50.0, 0.0, 0.0], [60.0, 0.0, 0.0]);
        let error = chain(vec![a.clone(), b.clone(), c]).unwrap_err();
        assert!(error.contains("branches at (10.000000"), "{error}");
        let error = chain(vec![a.clone(), far]).unwrap_err();
        assert!(error.contains("c4 does not join the others"), "{error}");
        let circle = PathCurve {
            name: "c5".to_owned(),
            curve: Curve3::Conic {
                center: [0.0; 3],
                normal: [0.0, 0.0, 1.0],
                x_axis: [1.0, 0.0, 0.0],
                major: 5.0,
                minor: 5.0,
                start: 0.0,
                end: TAU,
                closed: true,
            },
        };
        assert!(is_closed(&chain(vec![circle.clone()]).unwrap()));
        let error = chain(vec![a, circle]).unwrap_err();
        assert!(error.contains("c5 is a closed curve"), "{error}");
    }

    #[test]
    fn a_closed_chain_runs_along_its_first_curve() {
        let square = [
            line("c1", [0.0; 3], [10.0, 0.0, 0.0]),
            line("c2", [10.0, 10.0, 0.0], [10.0, 0.0, 0.0]),
            line("c3", [10.0, 10.0, 0.0], [0.0, 10.0, 0.0]),
            line("c4", [0.0, 10.0, 0.0], [0.0; 3]),
        ];
        let path = chain(square.to_vec()).unwrap();
        assert!(is_closed(&path));
        assert_eq!(
            path.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(),
            ["c1", "c2", "c3", "c4"]
        );
        assert_eq!(curve_ends(&path[1].curve).unwrap().1, [10.0, 10.0, 0.0]);
    }

    #[test]
    fn reversed_curves_run_between_the_same_points() {
        let arc = Curve3::Conic {
            center: [1.0, 2.0, 3.0],
            normal: [0.0, 0.0, 1.0],
            x_axis: [1.0, 0.0, 0.0],
            major: 4.0,
            minor: 2.0,
            start: 0.3,
            end: 2.0,
            closed: false,
        };
        let spline = Curve3::BSpline {
            degree: 2,
            poles: vec![[0.0; 3], [1.0, 2.0, 0.0], [3.0, 2.0, 1.0], [4.0, 0.0, 0.0]],
            weights: vec![1.0, 2.0, 1.0, 1.0],
            knots: vec![0.0, 0.0, 0.0, 0.4, 1.0, 1.0, 1.0],
        };
        for curve in [arc, spline] {
            let (a, b) = curve_ends(&curve).unwrap();
            let (c, d) = curve_ends(&reversed(curve.clone())).unwrap();
            assert!(
                distance(a, d) < 1e-12 && distance(b, c) < 1e-12,
                "{curve:?}"
            );
        }
        let ends = curve_ends(&Curve3::BSpline {
            degree: 1,
            poles: vec![[0.0; 3], [2.0, 0.0, 0.0], [4.0, 0.0, 0.0]],
            weights: Vec::new(),
            knots: vec![0.0, 0.0, 0.5, 1.0, 1.0],
        })
        .unwrap();
        assert_eq!(ends, ([0.0; 3], [4.0, 0.0, 0.0]));
    }
}
