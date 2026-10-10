// SPDX-License-Identifier: MIT
//! The profile regions an extrusion or revolution selected, from the loops
//! the stream decoder reads (`_f3d_profile_loops` of its detail, mitcad#96;
//! the grammar in `mitcad_f3d`'s `design/build/profiles.rs`).
//!
//! [`mapped_regions`] maps them onto Mitcad's regions of the sketch, so
//! that the import tries those regions first and the history's guesses
//! after them (*What the history settles* in the README). It is a function
//! of its own so that a better mapping can replace it.
//!
//! The rule *(found on the learning dump of the private corpora: it gives
//! the regions the history accepted for 1473 of 1552 extrusions, most of
//! the others sets the history cannot tell from them)*: a listed profile
//! is a face, its outer loops less its hole loops; a region is selected
//! when its inside point lies inside a profile by the even-odd rule
//! against the listed curve pieces. A record names piece `piece` of
//! `pieces` of a curve cut at its crossings with the other curves of the
//! same loop, numbered along the curve's parameter (a line from its start,
//! an arc from its start angle, a circle from its x axis counter-clockwise,
//! the piece holding that point first). The geometry is the file's (sketch
//! space, cm), so that the pieces are numbered as the file numbers them;
//! curves the sketch no longer has (a profile older than the sketch's last
//! edit) are left out.
//!
//! Where a profile has curves without geometry the rule can use (ellipses,
//! splines) or selects nothing, its loops select the regions whose outer
//! loops share most of their curves (at least half of the two together),
//! the pieces of a profile split by curves inside it (its children) each
//! their own; a sketch of one region gives that one.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::f64::consts::TAU;

use mitcad_f3d::design::ir::Geometry;
use serde_json::Value;

/// Distances (cm) within which curves meet.
const TOL: f64 = 2e-5;

/// A direction for the even-odd ray, unlikely to pass a vertex.
const RAY_ANGLE: f64 = 0.813_771_9;

type P = [f64; 2];

fn sub(a: P, b: P) -> P {
    [a[0] - b[0], a[1] - b[1]]
}

fn dot(a: P, b: P) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}

fn cross(a: P, b: P) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}

fn dist(a: P, b: P) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

/// A curve of the file's sketch, parameterised over [0, 1].
#[derive(Debug, Clone, Copy, PartialEq)]
enum Curve {
    Line {
        s: P,
        e: P,
    },
    /// An arc or circle: the angle from `reference` towards `perp`, from
    /// `a0` to `a1`.
    Round {
        c: P,
        r: f64,
        reference: P,
        perp: P,
        a0: f64,
        a1: f64,
        circle: bool,
    },
}

impl Curve {
    /// A line, arc or circle of the file's geometry (sketch space, cm).
    fn of(g: &Geometry) -> Option<Curve> {
        let two = |v: Option<[f64; 3]>| v.map(|v| [v[0], v[1]]);
        match g.geometry_type.as_deref()? {
            "Line3D" => Some(Curve::Line {
                s: two(g.start_point)?,
                e: two(g.end_point)?,
            }),
            t @ ("Arc3D" | "Circle3D") => {
                let c = two(g.center)?;
                let r = g.radius.filter(|r| *r > 1e-12)?;
                let sign = if g.normal.map_or(1.0, |n| n[2]) >= 0.0 {
                    1.0
                } else {
                    -1.0
                };
                let circle = t == "Circle3D";
                let (reference, a0, a1) = if circle {
                    ([1.0, 0.0], 0.0, TAU)
                } else {
                    (two(g.reference_vector)?, g.start_angle?, g.end_angle?)
                };
                Some(Curve::Round {
                    c,
                    r,
                    reference,
                    perp: [-reference[1] * sign, reference[0] * sign],
                    a0,
                    a1,
                    circle,
                })
            }
            _ => None,
        }
    }

    fn point(&self, t: f64) -> P {
        match *self {
            Curve::Line { s, e } => [s[0] + t * (e[0] - s[0]), s[1] + t * (e[1] - s[1])],
            Curve::Round {
                c,
                r,
                reference,
                perp,
                a0,
                a1,
                ..
            } => {
                let a = a0 + t * (a1 - a0);
                let (cos, sin) = (a.cos(), a.sin());
                [
                    c[0] + r * (cos * reference[0] + sin * perp[0]),
                    c[1] + r * (cos * reference[1] + sin * perp[1]),
                ]
            }
        }
    }

    /// The parameter of a point on the curve.
    fn param(&self, p: P) -> f64 {
        match *self {
            Curve::Line { s, e } => {
                let d = sub(e, s);
                dot(sub(p, s), d) / dot(d, d)
            }
            Curve::Round {
                c,
                r,
                reference,
                perp,
                a0,
                a1,
                circle,
            } => {
                let v = sub(p, c);
                let mut a = (dot(v, perp).atan2(dot(v, reference)) - a0).rem_euclid(TAU);
                if !circle && a > TAU - TOL / r {
                    a -= TAU;
                }
                a / (a1 - a0)
            }
        }
    }

    fn length(&self) -> f64 {
        match *self {
            Curve::Line { s, e } => dist(s, e).max(1e-9),
            Curve::Round { r, a0, a1, .. } => (r * (a1 - a0).abs()).max(1e-9),
        }
    }

    fn is_circle(&self) -> bool {
        matches!(self, Curve::Round { circle: true, .. })
    }

    fn contains(&self, t: f64) -> bool {
        if self.is_circle() {
            return true;
        }
        let tol = TOL / self.length();
        (-tol..=1.0 + tol).contains(&t)
    }

    fn ends(&self) -> Vec<P> {
        match self {
            Curve::Line { s, e } => vec![*s, *e],
            Curve::Round { circle: true, .. } => Vec::new(),
            _ => vec![self.point(0.0), self.point(1.0)],
        }
    }
}

/// Where a line meets a circle (centre `c`, radius `r`), within the line.
fn line_circle(s: P, e: P, c: P, r: f64) -> Vec<P> {
    let d = sub(e, s);
    let a = dot(d, d);
    let len = a.sqrt();
    let t0 = -dot(sub(s, c), d) / a;
    let at = |t: f64| [s[0] + t * d[0], s[1] + t * d[1]];
    let foot = at(t0);
    let gap = dist(foot, c);
    if gap > r + TOL {
        return Vec::new();
    }
    let h2 = r * r - gap * gap;
    let h = if h2 > 0.0 { h2.sqrt() } else { 0.0 };
    let ts = if h < 3.0 * TOL || r - gap < TOL {
        vec![t0]
    } else {
        vec![t0 - h / len, t0 + h / len]
    };
    ts.into_iter()
        .filter(|t| (-TOL / len..=1.0 + TOL / len).contains(t))
        .map(at)
        .collect()
}

/// Where two curves meet; the ends of an overlap where they lie on each
/// other.
fn crossings(a: &Curve, b: &Curve) -> Vec<P> {
    match (*a, *b) {
        (Curve::Line { s: s1, e: e1 }, Curve::Line { s: s2, e: e2 }) => {
            let (d1, d2) = (sub(e1, s1), sub(e2, s2));
            let den = cross(d1, d2);
            let (l1, l2) = (d1[0].hypot(d1[1]), d2[0].hypot(d2[1]));
            if den.abs() < 1e-7 * l1 * l2 {
                if cross(d1, sub(s2, s1)).abs() > TOL * l1 {
                    return Vec::new();
                }
                let mut out: Vec<P> = [s2, e2]
                    .into_iter()
                    .filter(|p| a.contains(a.param(*p)))
                    .collect();
                out.extend([s1, e1].into_iter().filter(|p| b.contains(b.param(*p))));
                return out;
            }
            let t = cross(sub(s2, s1), d2) / den;
            let u = cross(sub(s2, s1), d1) / den;
            if (-TOL / l1..=1.0 + TOL / l1).contains(&t)
                && (-TOL / l2..=1.0 + TOL / l2).contains(&u)
            {
                vec![a.point(t)]
            } else {
                Vec::new()
            }
        }
        (Curve::Line { s, e }, Curve::Round { c, r, .. }) => line_circle(s, e, c, r)
            .into_iter()
            .filter(|p| b.contains(b.param(*p)))
            .collect(),
        (Curve::Round { .. }, Curve::Line { .. }) => crossings(b, a),
        (Curve::Round { c: c1, r: r1, .. }, Curve::Round { c: c2, r: r2, .. }) => {
            let d = dist(c1, c2);
            if d < 1e-9 {
                if (r1 - r2).abs() >= TOL {
                    return Vec::new();
                }
                let mut out: Vec<P> = a
                    .ends()
                    .into_iter()
                    .filter(|p| b.contains(b.param(*p)))
                    .collect();
                out.extend(b.ends().into_iter().filter(|p| a.contains(a.param(*p))));
                return out;
            }
            if d > r1 + r2 + TOL || d < (r1 - r2).abs() - TOL {
                return Vec::new();
            }
            let x = (d * d + r1 * r1 - r2 * r2) / (2.0 * d);
            let h = (r1 * r1 - x * x).max(0.0).sqrt();
            let ex = [(c2[0] - c1[0]) / d, (c2[1] - c1[1]) / d];
            let ey = [-ex[1], ex[0]];
            let mut pts = vec![[c1[0] + x * ex[0] + h * ey[0], c1[1] + x * ex[1] + h * ey[1]]];
            if h > TOL {
                pts.push([c1[0] + x * ex[0] - h * ey[0], c1[1] + x * ex[1] - h * ey[1]]);
            }
            pts.into_iter()
                .filter(|p| a.contains(a.param(*p)) && b.contains(b.param(*p)))
                .collect()
        }
    }
}

/// Sorted values with those within `tol` of the one before left out.
fn cluster(mut ts: Vec<f64>, tol: f64) -> Vec<f64> {
    ts.sort_by(f64::total_cmp);
    let mut out: Vec<f64> = Vec::new();
    for t in ts {
        if out.last().is_some_and(|l| t - l <= tol) {
            continue;
        }
        out.push(t);
    }
    out
}

/// The pieces (parameter ranges) of `c` cut where the `others` cross it,
/// in the file's order.
fn pieces(c: &Curve, others: &[&Curve]) -> Vec<(f64, f64)> {
    let tol = TOL / c.length();
    let cuts: Vec<f64> = others
        .iter()
        .flat_map(|o| crossings(c, o))
        .map(|p| c.param(p))
        .collect();
    if c.is_circle() {
        let mut cuts = cluster(cuts.into_iter().map(|t| t.rem_euclid(1.0)).collect(), tol);
        if cuts.len() > 1 && cuts[cuts.len() - 1] - cuts[0] > 1.0 - tol {
            cuts.pop();
        }
        if cuts.is_empty() {
            return vec![(0.0, 1.0)];
        }
        let mut out: Vec<(f64, f64)> = (0..cuts.len())
            .map(|i| (cuts[i], cuts.get(i + 1).copied().unwrap_or(cuts[0] + 1.0)))
            .collect();
        // The piece holding the circle's start first.
        if cuts[0] > tol {
            out.rotate_right(1);
        }
        return out;
    }
    let mut ts = vec![0.0];
    ts.extend(
        cluster(cuts, tol)
            .into_iter()
            .filter(|t| tol < *t && *t < 1.0 - tol),
    );
    ts.push(1.0);
    ts.windows(2).map(|w| (w[0], w[1])).collect()
}

/// Where the ray from `p` crosses a piece of a curve: the distances along
/// the ray.
fn ray_hits(c: &Curve, p: P, t0: f64, t1: f64) -> Vec<f64> {
    let ray = [RAY_ANGLE.cos(), RAY_ANGLE.sin()];
    let mut hits = Vec::new();
    match *c {
        Curve::Line { s, e } => {
            let d = sub(e, s);
            let den = cross(ray, d);
            if den.abs() < 1e-12 {
                return Vec::new();
            }
            let w = sub(s, p);
            let along = cross(w, d) / den;
            if along > 1e-9 {
                hits.push((along, cross(w, ray) / den));
            }
        }
        Curve::Round { c: centre, r, .. } => {
            let f = sub(p, centre);
            let b = dot(f, ray);
            let disc = b * b - (dot(f, f) - r * r);
            if disc < 0.0 {
                return Vec::new();
            }
            let q = disc.sqrt();
            for along in [-b - q, -b + q] {
                if along > 1e-9 {
                    hits.push((
                        along,
                        c.param([p[0] + along * ray[0], p[1] + along * ray[1]]),
                    ));
                }
            }
        }
    }
    hits.into_iter()
        .filter(|(_, u)| {
            if c.is_circle() {
                (u - t0).rem_euclid(1.0) <= t1 - t0 + 1e-9
            } else {
                t0 - 1e-9 <= *u && *u <= t1 + 1e-9
            }
        })
        .map(|(along, _)| along)
        .collect()
}

/// Whether `p` lies inside the pieces by the even-odd rule; crossings at
/// one place (coincident copies of a curve) count once.
fn inside(pieces: &[(Curve, f64, f64)], p: P) -> bool {
    let mut hits: Vec<f64> = pieces
        .iter()
        .flat_map(|(c, t0, t1)| ray_hits(c, p, *t0, *t1))
        .collect();
    hits.sort_by(f64::total_cmp);
    let mut count = 0;
    let mut last = f64::NEG_INFINITY;
    for h in hits {
        if h - last >= 1e-6 {
            count += 1;
            last = h;
        }
    }
    count % 2 == 1
}

/// One record of a loop: the curve's sketch-local id (where the sketch
/// has it) and its ids in the file, and which piece.
#[derive(Debug, Clone, PartialEq)]
struct Piece {
    id: Option<String>,
    key: (u64, u64),
    piece: usize,
}

#[derive(Debug, Clone, PartialEq)]
struct Loop {
    outer: bool,
    pieces: Vec<Piece>,
}

#[derive(Debug, Clone, PartialEq)]
struct Profile {
    loops: Vec<Loop>,
    children: Vec<Vec<Loop>>,
}

fn loops_of(v: &Value) -> Option<Vec<Loop>> {
    v.as_array()?
        .iter()
        .map(|l| {
            let pieces = l["curves"]
                .as_array()?
                .iter()
                .map(|c| {
                    Some(Piece {
                        id: c["id"].as_str().map(str::to_owned),
                        key: (c["primary"].as_u64()?, c["secondary"].as_u64()?),
                        piece: c["piece"].as_u64()? as usize,
                    })
                })
                .collect::<Option<Vec<_>>>()?;
            Some(Loop {
                outer: l["outer"].as_bool()?,
                pieces,
            })
        })
        .collect()
}

fn profiles_of(v: &Value) -> Option<Vec<Profile>> {
    v.as_array()?
        .iter()
        .map(|p| {
            Some(Profile {
                loops: loops_of(&p["loops"])?,
                children: p["children"]
                    .as_array()?
                    .iter()
                    .map(loops_of)
                    .collect::<Option<Vec<_>>>()?,
            })
        })
        .collect()
}

/// A region of Mitcad's sketch as [`mapped_regions`] sees it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RegionView {
    /// A point inside it (sketch mm).
    pub inside: Option<[f64; 2]>,
    /// The file's ids of the curves of its outer loop; None when one is
    /// Mitcad's own.
    pub outer: Option<Vec<String>>,
}

/// The regions (indices into `regions`) the decoded loops `loops`
/// (`_f3d_profile_loops`) select, by the module's rule; `curves` gives the
/// file's geometry of the sketch's curves by their ids. None when the loops
/// do not read or select nothing.
pub(crate) fn mapped_regions(
    loops: &Value,
    curves: &HashMap<String, Geometry>,
    regions: &[RegionView],
) -> Option<Vec<usize>> {
    let profiles = profiles_of(loops)?;
    let mut out = BTreeSet::new();
    for p in &profiles {
        let found = by_geometry(p, curves, regions)
            .filter(|f| !f.is_empty())
            .unwrap_or_else(|| by_curves(p, regions));
        out.extend(found);
    }
    (!out.is_empty()).then(|| out.into_iter().collect())
}

/// The regions inside a profile's loops; None when a curve it lists that
/// the sketch has is no line, arc or circle.
fn by_geometry(
    p: &Profile,
    curves: &HashMap<String, Geometry>,
    regions: &[RegionView],
) -> Option<Vec<usize>> {
    let mut listed = Vec::new();
    for l in &p.loops {
        let mut known: Vec<(&Piece, Curve)> = Vec::new();
        for r in &l.pieces {
            let Some(id) = &r.id else { continue };
            let Some(g) = curves.get(id) else { continue };
            known.push((r, Curve::of(g)?));
        }
        for (r, c) in &known {
            let others: Vec<&Curve> = known
                .iter()
                .filter(|(o, _)| o.id != r.id)
                .map(|(_, c)| c)
                .collect();
            let all = pieces(c, &others);
            let (t0, t1) = all
                .get(r.piece.wrapping_sub(1))
                .copied()
                .unwrap_or((0.0, 1.0));
            listed.push((*c, t0, t1));
        }
    }
    if listed.is_empty() {
        return Some(Vec::new());
    }
    Some(
        regions
            .iter()
            .enumerate()
            .filter(|(_, r)| {
                r.inside
                    .is_some_and(|q| inside(&listed, [q[0] / 10.0, q[1] / 10.0]))
            })
            .map(|(i, _)| i)
            .collect(),
    )
}

/// The regions whose outer loops share most curves with the profile's
/// outer loops (its children's, where it has any).
fn by_curves(p: &Profile, regions: &[RegionView]) -> Vec<usize> {
    let key = |r: &Piece| match &r.id {
        Some(id) => id.clone(),
        None => format!("{}:{}", r.key.0, r.key.1),
    };
    let sets: Vec<Option<HashSet<&str>>> = regions
        .iter()
        .map(|r| {
            r.outer
                .as_ref()
                .map(|o| o.iter().map(String::as_str).collect())
        })
        .collect();
    let groups: Vec<&Vec<Loop>> = if p.children.is_empty() {
        vec![&p.loops]
    } else {
        p.children.iter().collect()
    };
    let mut out = BTreeSet::new();
    for l in groups.into_iter().flatten().filter(|l| l.outer) {
        let listed: HashSet<String> = l.pieces.iter().map(key).collect();
        let mut best = 0.0;
        let mut chosen: Vec<usize> = Vec::new();
        for (i, set) in sets.iter().enumerate() {
            let Some(set) = set else { continue };
            let common = set.iter().filter(|c| listed.contains(**c)).count();
            let union = listed.len() + set.len() - common;
            let j = if union == 0 {
                0.0
            } else {
                common as f64 / union as f64
            };
            if j > best + 1e-9 {
                best = j;
                chosen = vec![i];
            } else if (j - best).abs() <= 1e-9 && j > 0.0 {
                chosen.push(i);
            }
        }
        if best >= 0.5 {
            out.insert(chosen[0]);
            // Tied regions of other curves too; of the same curves (the
            // halves of a disc cut by its diameter) the first.
            let distinct: HashSet<Vec<&str>> = chosen
                .iter()
                .filter_map(|&i| {
                    let mut v: Vec<&str> = sets[i].as_ref()?.iter().copied().collect();
                    v.sort_unstable();
                    Some(v)
                })
                .collect();
            if distinct.len() > 1 {
                out.extend(chosen);
            }
        } else if regions.len() == 1 {
            out.insert(0);
        }
    }
    out.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn line(s: [f64; 2], e: [f64; 2]) -> Geometry {
        Geometry {
            geometry_type: Some("Line3D".into()),
            start_point: Some([s[0], s[1], 0.0]),
            end_point: Some([e[0], e[1], 0.0]),
            ..Geometry::default()
        }
    }

    fn circle(c: [f64; 2], r: f64) -> Geometry {
        Geometry {
            geometry_type: Some("Circle3D".into()),
            center: Some([c[0], c[1], 0.0]),
            radius: Some(r),
            normal: Some([0.0, 0.0, 1.0]),
            ..Geometry::default()
        }
    }

    fn rec(id: &str, piece: u64) -> Value {
        json!({"id": id, "primary": 1, "secondary": 0, "piece": piece, "pieces": 1})
    }

    /// A square of 4 cm with a circle of radius 1 cm in its middle and a
    /// diagonal line through the circle: regions are the square's two
    /// triangles less the circle, and the circle's two halves.
    fn sketch() -> (HashMap<String, Geometry>, Vec<RegionView>) {
        let curves = HashMap::from([
            ("c0".to_owned(), line([0.0, 0.0], [4.0, 0.0])),
            ("c1".to_owned(), line([4.0, 0.0], [4.0, 4.0])),
            ("c2".to_owned(), line([4.0, 4.0], [0.0, 4.0])),
            ("c3".to_owned(), line([0.0, 4.0], [0.0, 0.0])),
            ("c4".to_owned(), circle([2.0, 2.0], 1.0)),
            ("c5".to_owned(), line([0.0, 0.0], [4.0, 4.0])),
        ]);
        let region = |x: f64, y: f64, outer: &[&str]| RegionView {
            inside: Some([x * 10.0, y * 10.0]),
            outer: Some(outer.iter().map(|s| s.to_string()).collect()),
        };
        let regions = vec![
            region(3.5, 0.5, &["c0", "c1", "c5", "c4"]),
            region(0.5, 3.5, &["c2", "c3", "c5", "c4"]),
            region(2.5, 1.8, &["c4", "c5"]),
            region(1.5, 2.2, &["c4", "c5"]),
        ];
        (curves, regions)
    }

    #[test]
    fn pieces_follow_the_curve() {
        let c = Curve::of(&circle([0.0, 0.0], 1.0)).unwrap();
        let l = Curve::of(&line([-2.0, 0.5], [2.0, 0.5])).unwrap();
        let p = pieces(&c, &[&l]);
        // Cut at 30° and 150°: the piece holding 0° (from 150° round) first.
        assert_eq!(p.len(), 2);
        assert!((p[0].0 - 5.0 / 12.0).abs() < 1e-9 && (p[0].1 - 13.0 / 12.0).abs() < 1e-9);
        assert!((p[1].0 - 1.0 / 12.0).abs() < 1e-9 && (p[1].1 - 5.0 / 12.0).abs() < 1e-9);
        let q = pieces(&l, &[&c]);
        assert_eq!(q.len(), 3);
        assert!((q[1].0 - (2.0 - 3f64.sqrt() / 2.0) / 4.0).abs() < 1e-9);
    }

    #[test]
    fn a_face_selects_the_regions_inside_it() {
        let (curves, regions) = sketch();
        // The square less the circle: the two triangles' material, cut by
        // nothing inside the square (the diagonal is not in the loop).
        let square = json!([{"loops": [
            {"outer": true, "curves": [rec("c0", 1), rec("c1", 1), rec("c2", 1), rec("c3", 1)]},
            {"outer": false, "curves": [rec("c4", 1)]}], "children": []}]);
        assert_eq!(mapped_regions(&square, &curves, &regions), Some(vec![0, 1]));
        // The circle's lower right half: piece 1 of the circle cut by the
        // diagonal (from 225° round to 45°) and the diagonal's middle piece.
        let half = json!([{"loops": [
            {"outer": true, "curves": [rec("c4", 1), rec("c5", 2)]}], "children": []}]);
        assert_eq!(mapped_regions(&half, &curves, &regions), Some(vec![2]));
        // The whole circle and the square without its hole: everything.
        let all = json!([{"loops": [{"outer": true, "curves": [rec("c4", 1)]}], "children": []},
            {"loops": [{"outer": true,
                        "curves": [rec("c0", 1), rec("c1", 1), rec("c2", 1), rec("c3", 1)]}],
             "children": []}]);
        assert_eq!(
            mapped_regions(&all, &curves, &regions),
            Some(vec![0, 1, 2, 3])
        );
    }

    #[test]
    fn without_geometry_the_curves_decide() {
        let (mut curves, regions) = sketch();
        // A spline stands in for the circle: the rule by geometry does not
        // apply, the outer loop's curves select the triangle.
        curves.insert(
            "c4".into(),
            Geometry {
                geometry_type: Some("NurbsCurve3D".into()),
                ..Geometry::default()
            },
        );
        let triangle = json!([{"loops": [
            {"outer": true, "curves": [rec("c0", 1), rec("c1", 1), rec("c5", 1), rec("c4", 1)]}],
            "children": []}]);
        assert_eq!(mapped_regions(&triangle, &curves, &regions), Some(vec![0]));
        // Children: each piece selects its own region.
        let split = json!([{"loops": [{"outer": true, "curves": [rec("c4", 1)]}], "children": [
            [{"outer": true, "curves": [rec("c4", 1), rec("c5", 1)]}]]}]);
        assert_eq!(mapped_regions(&split, &curves, &regions), Some(vec![2]));
        // Nothing that reads, or selects nothing: none.
        assert_eq!(mapped_regions(&json!({}), &curves, &regions), None);
        let stale = json!([{"loops": [{"outer": true, "curves": [
            {"primary": 9, "secondary": 0, "piece": 1, "pieces": 1}]}], "children": []}]);
        assert_eq!(mapped_regions(&stale, &curves, &regions), None);
    }
}
