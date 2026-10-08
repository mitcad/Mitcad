// SPDX-License-Identifier: MIT
//! Constraints over groups of entities (mitcad#36): polygons, circular
//! and rectangular sketch patterns, offsets, and the radius gap of a
//! concentric circle dimension. Each becomes what Mitcad's own tool makes
//! (`commands.md`, *sketch*):
//!
//! - a polygon: a construction circle about its centre through its
//!   corners, and equal sides (`sketch.polygon`);
//! - a pattern: a `patterns` record whose copies are the file's copies,
//!   matched to their originals by the stored geometry;
//! - an offset of lines and arcs: each offset curve parallel to (or
//!   concentric with) its source at the offset's distance, all with one
//!   parameter, and an `offsets` record (`sketch.offset`);
//! - a radius gap (concentric circle dimensions, offset arcs): a
//!   construction line from one circle to the other along a radius,
//!   horizontal or vertical, whose length is the dimension.
//!
//! Every group is checked against the stored geometry; a group that does
//! not match is left out whole (the sketch is then `partial`).

use std::collections::{BTreeMap, HashMap, HashSet};

use mitcad_f3d::design::ir::{Reference, SketchConstraint, SketchDimension};
use serde_json::{Value, json};

use super::{Ids, Kind, RefOf, refs, resolve};
use crate::geom::mm;
use crate::params::ParamMap;

/// Positions closer than this are the same (sketch mm).
const TOL: f64 = 1e-5;

/// What the groups add to a sketch.
#[derive(Debug, Default)]
pub(super) struct Group {
    pub entities: Vec<Value>,
    pub constraints: Vec<Value>,
    /// (driving, driven).
    pub dimensions: Vec<(Value, Value)>,
    pub patterns: Vec<Value>,
    pub offsets: Vec<Value>,
    /// Parameters to create before the sketch: (placeholder used in the
    /// values above, suggested name, expression).
    pub params: Vec<(String, String, String)>,
}

impl Group {
    pub fn append(&mut self, other: Group) {
        self.entities.extend(other.entities);
        self.constraints.extend(other.constraints);
        self.dimensions.extend(other.dimensions);
        self.patterns.extend(other.patterns);
        self.offsets.extend(other.offsets);
        self.params.extend(other.params);
    }
}

/// The translated sketch so far: entities by Mitcad name and point
/// positions.
pub(super) struct Sketch<'a> {
    pub entities: HashMap<String, &'a Value>,
    pub at: &'a HashMap<u32, [f64; 2]>,
    pub ids: &'a Ids,
    pub params: &'a ParamMap,
    /// The next free entity number.
    pub next: u32,
}

type P2 = [f64; 2];

fn sub(a: P2, b: P2) -> P2 {
    [a[0] - b[0], a[1] - b[1]]
}

fn add(a: P2, b: P2) -> P2 {
    [a[0] + b[0], a[1] + b[1]]
}

fn scale(a: P2, s: f64) -> P2 {
    [a[0] * s, a[1] * s]
}

fn dist(a: P2, b: P2) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

fn cross(a: P2, b: P2) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}

fn dot(a: P2, b: P2) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}

fn unit(a: P2) -> Option<P2> {
    let n = a[0].hypot(a[1]);
    (n > 1e-12).then(|| [a[0] / n, a[1] / n])
}

/// A quarter turn counter-clockwise.
fn perp(a: P2) -> P2 {
    [-a[1], a[0]]
}

fn number(name: &str) -> Option<u32> {
    name.get(1..)?.parse().ok()
}

impl Sketch<'_> {
    fn fresh(&mut self) -> u32 {
        self.next += 1;
        self.next - 1
    }

    fn entity(&self, name: &str) -> Result<&Value, String> {
        self.entities
            .get(name)
            .copied()
            .ok_or_else(|| format!("{name} was left out"))
    }

    fn point(&self, name: &str) -> Result<P2, String> {
        number(name)
            .and_then(|n| self.at.get(&n).copied())
            .ok_or_else(|| format!("{name} has no position"))
    }

    /// The points that define a curve, in a fixed order (a line's ends,
    /// a circle's centre, an arc's centre, start and end, ...).
    fn points_of(e: &Value) -> Vec<String> {
        let s = |k: &str| e[k].as_str().map(str::to_owned);
        let mut out: Vec<String> = match e["type"].as_str() {
            Some("point") => e["id"].as_str().map(str::to_owned).into_iter().collect(),
            Some("line") => [s("start"), s("end")].into_iter().flatten().collect(),
            Some("circle") => s("center").into_iter().collect(),
            Some("arc") => [s("center"), s("start"), s("end")]
                .into_iter()
                .flatten()
                .collect(),
            Some("ellipse") => [s("center"), s("major")].into_iter().flatten().collect(),
            Some("elliptical_arc") => [s("center"), s("major"), s("start"), s("end")]
                .into_iter()
                .flatten()
                .collect(),
            _ => Vec::new(),
        };
        if let Some(control) = e["control"].as_array() {
            out.extend(control.iter().filter_map(|p| p.as_str().map(str::to_owned)));
        }
        out
    }

    /// A circle's or an arc's centre point, centre and radius.
    fn round(&self, name: &str) -> Result<(String, P2, f64), String> {
        let e = self.entity(name)?;
        let center = e["center"].as_str().ok_or("not a circle or an arc")?;
        let c = self.point(center)?;
        let r = match e["type"].as_str() {
            Some("circle") => e["radius"].as_f64().ok_or("no radius")?,
            Some("arc") => dist(self.point(e["start"].as_str().ok_or("no start")?)?, c),
            _ => return Err(format!("{name} is not a circle or an arc")),
        };
        Ok((center.to_owned(), c, r))
    }

    /// The Mitcad names of sketch-local ids (all must be translated).
    fn names(&self, list: &[RefOf]) -> Result<Vec<(String, Kind)>, String> {
        resolve(list, self.ids)
    }

    /// The point pairs of `copy` as `f` of `original` (same type and size),
    /// or None.
    fn image(
        &self,
        original: &str,
        copy: &str,
        f: &dyn Fn(P2) -> P2,
    ) -> Option<Vec<(String, String)>> {
        let (a, b) = (self.entity(original).ok()?, self.entity(copy).ok()?);
        if a["type"] != b["type"] {
            return None;
        }
        for key in ["radius", "minor_radius"] {
            if let (Some(x), Some(y)) = (a[key].as_f64(), b[key].as_f64())
                && (x - y).abs() > TOL
            {
                return None;
            }
        }
        let (pa, pb) = (Self::points_of(a), Self::points_of(b));
        if pa.len() != pb.len() || pa.is_empty() {
            return None;
        }
        let fits = |order: &[String]| -> Option<bool> {
            let mut ok = true;
            for (p, q) in pa.iter().zip(order) {
                ok &= dist(f(self.point(p).ok()?), self.point(q).ok()?) < TOL;
            }
            Some(ok)
        };
        if fits(&pb)? {
            return Some(pa.into_iter().zip(pb).collect());
        }
        // A line may run the other way.
        if a["type"] == "line" {
            let reversed: Vec<String> = pb.iter().rev().cloned().collect();
            if fits(&reversed)? {
                return Some(pa.into_iter().zip(reversed).collect());
            }
        }
        None
    }
}

/// The ids a constraint's property lists (sketch-local ids).
fn id_list(props: Option<&serde_json::Map<String, Value>>, key: &str) -> Option<Vec<RefOf>> {
    props?.get(key)?.as_array().map(|list| {
        list.iter()
            .map(|v| {
                v.as_str()
                    .map_or(RefOf::Outside, |s| RefOf::Local(s.to_owned()))
            })
            .collect()
    })
}

/// A parameter reference in a constraint's properties: (Mitcad name if
/// the parameter was created, value in internal units).
fn param(
    props: Option<&serde_json::Map<String, Value>>,
    key: &str,
    params: &ParamMap,
) -> Option<(Option<String>, f64)> {
    let r: Reference = serde_json::from_value(props?.get(key)?.clone()).ok()?;
    let p = r.parameter()?;
    let name = p
        .name
        .as_deref()
        .and_then(|n| params.get(n))
        .map(str::to_owned);
    Some((name, p.value?))
}

/// A value: the parameter (negated when `negate`, divided by `divide`),
/// else the number.
fn value_of(name: Option<&str>, value: f64, negate: bool, divide: u32) -> Value {
    match name {
        Some(n) => {
            let mut e = if negate {
                format!("-({n})")
            } else {
                n.to_owned()
            };
            if divide > 1 {
                e = format!("({e}) / {divide}");
            }
            json!(e)
        }
        None => {
            let v = if negate { -value } else { value };
            json!(v / f64::from(divide.max(1)))
        }
    }
}

// Polygons.

/// A polygon: its corners (the file's order, around the polygon) and its
/// centre last.
pub(super) fn polygon(c: &SketchConstraint, s: &mut Sketch<'_>) -> Result<Group, String> {
    let list = refs(c.refs.as_ref(), &["entities"]);
    let names = s.names(&list)?;
    let (center, corners) = names
        .split_last()
        .filter(|(c, rest)| c.1 == Kind::Point && rest.len() >= 3)
        .ok_or("a polygon needs its corners and centre")?;
    if corners.iter().any(|(_, k)| *k != Kind::Point) {
        return Err("a polygon's corners are points".to_owned());
    }
    let c0 = s.point(&center.0)?;
    let r = dist(s.point(&corners[0].0)?, c0);
    for (p, _) in corners {
        if (dist(s.point(p)?, c0) - r).abs() > TOL {
            return Err("the polygon's corners are not on one circle".to_owned());
        }
    }
    // The side from each corner to the next.
    let n = corners.len();
    let mut sides = Vec::new();
    for i in 0..n {
        let (a, b) = (&corners[i].0, &corners[(i + 1) % n].0);
        let side = s
            .entities
            .iter()
            .filter(|(_, e)| {
                e["type"] == "line"
                    && ((e["start"] == a.as_str() && e["end"] == b.as_str())
                        || (e["start"] == b.as_str() && e["end"] == a.as_str()))
            })
            .map(|(name, _)| name.clone())
            .min_by_key(|name| number(name))
            .ok_or_else(|| format!("no side from {a} to {b}"))?;
        sides.push(side);
    }
    let mut g = Group::default();
    let circle = format!("c{}", s.fresh());
    g.entities
        .push(json!({"id": circle, "type": "circle", "center": center.0,
                           "radius": r, "construction": true}));
    for (p, _) in corners {
        g.constraints
            .push(json!({"type": "coincident", "point": p, "entity": circle}));
    }
    for w in sides.windows(2) {
        g.constraints
            .push(json!({"type": "equal", "a": w[0], "b": w[1]}));
    }
    Ok(g)
}

// Patterns.

/// Copies matched to originals: per instance, original → copy (curves and
/// their points).
type Copies = Vec<([u32; 2], BTreeMap<String, String>)>;

/// How an instance moves the originals.
type Move = Box<dyn Fn(P2) -> P2>;

/// An instance's place in the pattern and its move.
type Instance = ([u32; 2], Move);

/// Matches the copies to the originals moved by each instance's `f`.
fn match_copies(
    s: &Sketch<'_>,
    originals: &[String],
    copies: &[String],
    instances: &[Instance],
) -> Option<Copies> {
    let mut used: HashSet<&str> = HashSet::new();
    let mut out = Vec::new();
    for (index, f) in instances {
        let mut map: BTreeMap<String, String> = BTreeMap::new();
        // Curves first, so that points on them map with them.
        let mut order: Vec<&String> = originals.iter().filter(|o| o.starts_with('c')).collect();
        order.extend(originals.iter().filter(|o| o.starts_with('p')));
        for o in order {
            if map.contains_key(o) {
                continue;
            }
            let found = copies.iter().find_map(|c| {
                if used.contains(c.as_str()) {
                    return None;
                }
                let pairs = s.image(o, c, f.as_ref())?;
                // Points already mapped must map the same way.
                pairs
                    .iter()
                    .all(|(a, b)| map.get(a).is_none_or(|m| m == b))
                    .then_some((c, pairs))
            })?;
            let (c, pairs) = found;
            if o.starts_with('c') {
                map.insert(o.clone(), c.clone());
                used.insert(c.as_str());
            }
            for (a, b) in pairs {
                map.insert(a, b);
            }
        }
        for b in map.values() {
            if let Some(c) = copies.iter().find(|c| *c == b) {
                used.insert(c.as_str());
            }
        }
        out.push((*index, map));
    }
    (used.len() == copies.len()).then_some(out)
}

fn copies_json(copies: &Copies) -> Value {
    copies
        .iter()
        .map(|(index, map)| json!({"index": index, "entities": map}))
        .collect()
}

/// Originals and copies of a pattern: the entities less the copies (and
/// the centre).
fn pattern_entities(
    c: &SketchConstraint,
    s: &Sketch<'_>,
    center: Option<&str>,
) -> Result<(Vec<String>, Vec<String>), String> {
    let props = c.props.as_ref();
    let all = s.names(&refs(c.refs.as_ref(), &["entities"]))?;
    if all.is_empty() {
        return Err("its entities were not recorded".to_owned());
    }
    let created: Vec<String> = s
        .names(&id_list(props, "createdEntities").ok_or("its copies were not decoded")?)?
        .into_iter()
        .map(|(n, _)| n)
        .collect();
    let originals: Vec<String> = all
        .into_iter()
        .map(|(n, _)| n)
        .filter(|n| !created.contains(n) && Some(n.as_str()) != center)
        .collect();
    if originals.is_empty() || created.is_empty() {
        return Err("no originals or no copies".to_owned());
    }
    Ok((originals, created))
}

pub(super) fn circular_pattern(c: &SketchConstraint, s: &mut Sketch<'_>) -> Result<Group, String> {
    let props = c.props.as_ref();
    let center = props
        .and_then(|p| p.get("centerPoint"))
        .and_then(Value::as_str)
        .map(|id| RefOf::Local(id.to_owned()))
        .ok_or("no centre point")?;
    let center = s.names(&[center])?.remove(0).0;
    let (originals, created) = pattern_entities(c, s, Some(&center))?;
    let (_, quantity) = param(props, "quantity", s.params).ok_or("no quantity")?;
    let (angle_name, angle) = param(props, "totalAngle", s.params).ok_or("no angle")?;
    let count = quantity.round() as u32;
    if !(2..=1000).contains(&count) || angle == 0.0 {
        return Err(format!("{count} instances over {angle} rad"));
    }
    let full = (angle.abs() - std::f64::consts::TAU).abs() < 1e-9;
    let step = if full {
        angle / f64::from(count)
    } else {
        angle / f64::from(count - 1)
    };
    let c0 = s.point(&center)?;
    for sign in [1.0, -1.0] {
        let instances: Vec<Instance> = (1..count)
            .map(|k| {
                let a = sign * step * f64::from(k);
                let (sn, cs) = a.sin_cos();
                let f = move |p: P2| {
                    let d = sub(p, c0);
                    add(c0, [d[0] * cs - d[1] * sn, d[0] * sn + d[1] * cs])
                };
                ([k, 0], Box::new(f) as Move)
            })
            .collect();
        if let Some(copies) = match_copies(s, &originals, &created, &instances) {
            let mut g = Group::default();
            g.patterns
                .push(json!({"type": "circular", "center": center, "count": count,
                "angle": value_of(angle_name.as_deref(), angle, sign < 0.0, 1),
                "entities": originals, "copies": copies_json(&copies)}));
            return Ok(g);
        }
    }
    Err("the copies are not where the pattern puts them".to_owned())
}

pub(super) fn rectangular_pattern(
    c: &SketchConstraint,
    s: &mut Sketch<'_>,
) -> Result<Group, String> {
    let props = c.props.as_ref();
    let (originals, created) = pattern_entities(c, s, None)?;
    let direction = |key: &str| -> Option<P2> {
        let v = props?.get(key)?.as_array()?;
        unit([v.first()?.as_f64()?, v.get(1)?.as_f64()?])
    };
    let mut dirs = Vec::new();
    for (q, d, along) in [
        ("quantityOne", "distanceOne", "directionOne"),
        ("quantityTwo", "distanceTwo", "directionTwo"),
    ] {
        let (_, n) = param(props, q, s.params).ok_or("no quantity")?;
        let (name, dist) = param(props, d, s.params).ok_or("no distance")?;
        let u = direction(along).ok_or("no direction")?;
        dirs.push((n.round() as u32, name, mm(dist), u));
    }
    // The second direction alone is the first.
    if dirs[0].0 == 1 {
        dirs.swap(0, 1);
    }
    let (n1, n2) = (dirs[0].0, dirs[1].0);
    if n1 < 2 || n2 < 1 || u64::from(n1) * u64::from(n2) > 1000 {
        return Err(format!("{n1} × {n2} instances"));
    }
    // The distance is the spacing, or the whole extent.
    for extent in [false, true] {
        let step = |k: usize| -> (u32, f64) {
            let (n, _, d, _) = &dirs[k];
            let div = if extent && *n > 1 { n - 1 } else { 1 };
            (div, d / f64::from(div))
        };
        let (div1, s1) = step(0);
        let (div2, s2) = step(1);
        let (e1, e2) = (scale(dirs[0].3, s1), scale(dirs[1].3, s2));
        let instances: Vec<Instance> = (0..n1)
            .flat_map(|i| (0..n2).map(move |j| [i, j]))
            .filter(|ij| *ij != [0, 0])
            .map(|[i, j]| {
                let by = add(scale(e1, f64::from(i)), scale(e2, f64::from(j)));
                ([i, j], Box::new(move |p: P2| add(p, by)) as Move)
            })
            .collect();
        let Some(mut copies) = match_copies(s, &originals, &created, &instances) else {
            continue;
        };
        // Mitcad's second direction is its first turned a quarter
        // counter-clockwise, and its spacings are positive.
        let u1 = unit(e1).ok_or("a zero spacing")?;
        let ok_second = |a: P2, b: P2| unit(b).is_some_and(|b| dot(b, perp(a)) > 1.0 - 1e-9);
        let value = |k: usize, div: u32, sp: f64| {
            let (_, name, d, _) = &dirs[k];
            value_of(name.as_deref(), *d, sp < 0.0, div)
        };
        let (direction, count, spacing) = if n2 == 1 {
            let v1 = value(0, div1, s1);
            ([u1[0], u1[1]], [n1, 1], [v1.clone(), v1])
        } else if ok_second(u1, e2) {
            (
                [u1[0], u1[1]],
                [n1, n2],
                [value(0, div1, s1), value(1, div2, s2)],
            )
        } else if let Some(u2) = unit(e2).filter(|u2| ok_second(*u2, e1)) {
            for (index, _) in &mut copies {
                index.swap(0, 1);
            }
            (
                [u2[0], u2[1]],
                [n2, n1],
                [value(1, div2, s2), value(0, div1, s1)],
            )
        } else {
            return Err("the directions are not at right angles".to_owned());
        };
        let mut g = Group::default();
        g.patterns
            .push(json!({"type": "rectangular", "direction": direction,
            "count": count, "spacing": spacing, "entities": originals,
            "copies": copies_json(&copies)}));
        return Ok(g);
    }
    Err("the copies are not where the pattern puts them (a symmetric pattern?)".to_owned())
}

// Offsets and radius gaps.

/// A construction line along a radius from circle or arc `a` to `b`
/// (concentric), horizontal or vertical toward `toward`, with its length
/// dimension (`value`, or driven).
fn radius_gap(
    s: &mut Sketch<'_>,
    a: &str,
    b: &str,
    value: Option<Value>,
    toward: Option<P2>,
    g: &mut Group,
) -> Result<(), String> {
    let (ca, c, ra) = s.round(a)?;
    let (cb, cb_at, rb) = s.round(b)?;
    if dist(c, cb_at) > TOL {
        return Err(format!("{a} and {b} are not concentric"));
    }
    if (ra - rb).abs() < TOL {
        return Err(format!("{a} and {b} have the same radius"));
    }
    let d = toward.map_or([1.0, 0.0], |t| sub(t, c));
    let (u, horizontal) = if d[0].abs() >= d[1].abs() {
        ([if d[0] < 0.0 { -1.0 } else { 1.0 }, 0.0], true)
    } else {
        ([0.0, if d[1] < 0.0 { -1.0 } else { 1.0 }], false)
    };
    let (p1, p2, line) = (s.fresh(), s.fresh(), s.fresh());
    let (p1, p2, line) = (format!("p{p1}"), format!("p{p2}"), format!("c{line}"));
    g.entities.extend([
        json!({"id": p1, "type": "point", "at": add(c, scale(u, ra))}),
        json!({"id": p2, "type": "point", "at": add(c, scale(u, rb))}),
        json!({"id": line, "type": "line", "start": p1, "end": p2, "construction": true}),
    ]);
    if ca != cb {
        g.constraints
            .push(json!({"type": "concentric", "a": a, "b": b}));
    }
    g.constraints.extend([
        json!({"type": "coincident", "point": p1, "entity": a}),
        json!({"type": "coincident", "point": p2, "entity": b}),
        json!({"type": if horizontal { "horizontal" } else { "vertical" }, "line": line}),
        json!({"type": "coincident", "point": ca, "entity": line}),
    ]);
    let mut driven = json!({"type": "length", "line": line});
    if let Some(t) = toward {
        driven["text"] = json!(t);
    }
    let mut driving = driven.clone();
    driven["driven"] = json!(true);
    match value {
        Some(v) => driving["value"] = v,
        None => driving = driven.clone(),
    }
    g.dimensions.push((driving, driven));
    Ok(())
}

/// A concentric circle dimension: the radius gap of its two curves.
pub(super) fn concentric_dimension(
    d: &SketchDimension,
    s: &mut Sketch<'_>,
    value: Option<Value>,
) -> Result<Group, String> {
    let names = s.names(&refs(d.refs.as_ref(), &["circleOne", "circleTwo"]))?;
    let [(a, Kind::Curve), (b, Kind::Curve)] = &names[..] else {
        return Err("a concentric circle dimension needs two circles".to_owned());
    };
    let toward = d.text_position.map(|t| [mm(t[0]), mm(t[1])]);
    let mut g = Group::default();
    radius_gap(s, a, b, value, toward, &mut g)?;
    Ok(g)
}

/// The side Mitcad's offset puts `child` on, seen along the chain of
/// `curves` as `sketch.offset` orders it (its first curve with a free end,
/// else the first; forward when that end is its start or, in a closed
/// chain, when it meets the next curve at its end): true for the left.
fn offset_left(s: &Sketch<'_>, pairs: &[(String, String)]) -> Result<bool, String> {
    let ends: Vec<Option<(String, String)>> = pairs
        .iter()
        .map(|(p, _)| {
            let e = s.entity(p).ok()?;
            Some((
                e["start"].as_str()?.to_owned(),
                e["end"].as_str()?.to_owned(),
            ))
        })
        .collect();
    let degree = |q: &str| {
        ends.iter()
            .flatten()
            .filter(|(a, b)| a == q || b == q)
            .count()
    };
    let (first, forward) = if pairs.len() == 1 || ends.iter().any(Option::is_none) {
        (0, true)
    } else {
        let first = ends
            .iter()
            .flatten()
            .position(|(a, b)| degree(a) == 1 || degree(b) == 1)
            .unwrap_or(0);
        let (s0, t0) = ends[first].clone().expect("open");
        let (s1, t1) = ends[(first + 1) % ends.len()].clone().expect("open");
        (first, degree(&s0) == 1 || t0 == s1 || t0 == t1)
    };
    let (parent, child) = &pairs[first];
    let e = s.entity(parent)?;
    match e["type"].as_str() {
        Some("line") => {
            let (a, b) = (
                s.point(e["start"].as_str().ok_or("no start")?)?,
                s.point(e["end"].as_str().ok_or("no end")?)?,
            );
            let dir = if forward { sub(b, a) } else { sub(a, b) };
            let c = s.entity(child)?;
            let m = scale(
                add(
                    s.point(c["start"].as_str().ok_or("no start")?)?,
                    s.point(c["end"].as_str().ok_or("no end")?)?,
                ),
                0.5,
            );
            Ok(cross(dir, sub(m, scale(add(a, b), 0.5))) > 0.0)
        }
        _ => {
            // Counter-clockwise travel has the centre on the left.
            let (_, _, rp) = s.round(parent)?;
            let (_, _, rc) = s.round(child)?;
            Ok((rc < rp) == forward)
        }
    }
}

/// An offset: its curves at its distance from their sources. Returns the
/// group and the offset's dimension (sketch-local id), which the group
/// stands for.
pub(super) fn offset(
    c: &SketchConstraint,
    s: &mut Sketch<'_>,
    dimensions: &[SketchDimension],
) -> Result<(Group, Option<String>), String> {
    let props = c.props.as_ref();
    let parents = s.names(&id_list(props, "parentCurves").ok_or("its curves were not decoded")?)?;
    let children = s.names(&id_list(props, "childCurves").ok_or("its curves were not decoded")?)?;
    if children.is_empty() {
        return Err("no offset curves".to_owned());
    }
    let dimension = props
        .and_then(|p| p.get("dimension"))
        .and_then(Value::as_str)
        .map(str::to_owned);
    // The distance: the offset dimension's parameter, else the stored one.
    let parameter = dimension
        .as_deref()
        .and_then(|id| dimensions.iter().find(|d| d.id.as_deref() == Some(id)))
        .and_then(|d| d.parameter.clone().flatten())
        .and_then(|r| r.parameter().cloned());
    let stored = props
        .and_then(|p| p.get("distance"))
        .and_then(Value::as_f64);
    let signed = parameter
        .as_ref()
        .and_then(|p| p.value)
        .or(stored)
        .ok_or("no distance")?;
    let distance = mm(signed).abs();
    if distance < TOL {
        return Err("a zero offset".to_owned());
    }
    // Each offset curve with its source.
    let mut pairs: Vec<(String, String)> = Vec::new();
    let mut taken: HashSet<String> = HashSet::new();
    for (child, _) in &children {
        let ce = s.entity(child)?;
        let kind = ce["type"].as_str().unwrap_or_default();
        let mut best: Option<(f64, &String)> = None;
        for (parent, _) in &parents {
            if taken.contains(parent) {
                continue;
            }
            let pe = s.entity(parent)?;
            if pe["type"] != kind {
                continue;
            }
            let near = match kind {
                "line" => {
                    let ends = |e: &Value| -> Result<(P2, P2), String> {
                        Ok((
                            s.point(e["start"].as_str().ok_or("no start")?)?,
                            s.point(e["end"].as_str().ok_or("no end")?)?,
                        ))
                    };
                    let ((a, b), (p, q)) = (ends(pe)?, ends(ce)?);
                    let (Some(u), Some(v)) = (unit(sub(b, a)), unit(sub(q, p))) else {
                        continue;
                    };
                    let m = scale(add(p, q), 0.5);
                    if cross(u, v).abs() > 1e-7
                        || (cross(u, sub(m, a)).abs() - distance).abs() > TOL
                    {
                        continue;
                    }
                    dist(m, scale(add(a, b), 0.5))
                }
                "arc" | "circle" => {
                    let ((_, pc, pr), (_, cc, cr)) = (s.round(parent)?, s.round(child)?);
                    if dist(pc, cc) > TOL || ((pr - cr).abs() - distance).abs() > TOL {
                        continue;
                    }
                    let mid = |e: &Value| -> P2 {
                        let pts = Sketch::points_of(e);
                        let sum = pts
                            .iter()
                            .filter_map(|p| s.point(p).ok())
                            .fold([0.0, 0.0], add);
                        scale(sum, 1.0 / pts.len().max(1) as f64)
                    };
                    dist(mid(pe), mid(ce))
                }
                other => return Err(format!("offsets of {other}s are not supported")),
            };
            if best.is_none_or(|(d, _)| near < d) {
                best = Some((near, parent));
            }
        }
        let (_, parent) = best.ok_or_else(|| format!("{child} has no source at the distance"))?;
        taken.insert(parent.clone());
        pairs.push((parent.clone(), child.clone()));
    }
    // One parameter for all: the dimension's, negated where its sign
    // (the side) is negative.
    let mut g = Group::default();
    let name = parameter
        .as_ref()
        .and_then(|p| p.name.as_deref())
        .and_then(|n| s.params.get(n))
        .map(str::to_owned);
    let value = match name {
        Some(n) if signed < 0.0 => {
            let placeholder = format!("\u{1}offset{}", c.id.as_deref().unwrap_or("?"));
            g.params.push((
                placeholder.clone(),
                format!("{n}_offset"),
                format!("-({n})"),
            ));
            json!(placeholder)
        }
        Some(n) => json!(n),
        None => json!(distance),
    };
    for (parent, child) in &pairs {
        if s.entity(parent)?["type"] == "line" {
            g.constraints
                .push(json!({"type": "parallel", "a": parent, "b": child}));
            let driven = json!({"type": "line_distance", "a": parent, "b": child, "driven": true});
            let mut driving = driven.clone();
            driving.as_object_mut().expect("object").remove("driven");
            driving["value"] = value.clone();
            g.dimensions.push((driving, driven));
        } else {
            radius_gap(s, parent, child, Some(value.clone()), None, &mut g)?;
        }
    }
    let left = offset_left(s, &pairs)?;
    let (curves, results): (Vec<String>, Vec<String>) = pairs.into_iter().unzip();
    g.offsets
        .push(json!({"curves": curves, "distance": value, "left": left,
                          "results": results}));
    Ok((g, dimension))
}
