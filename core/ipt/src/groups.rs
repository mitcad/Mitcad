// SPDX-License-Identifier: MIT
//! Sketch groups: polygons and the circular and rectangular patterns of
//! sketch entities, as the dump's `PolygonConstraint`,
//! `CircularPatternConstraint` and `RectangularPatternConstraint`.
//!
//! - A group (`38485352…`): the header, the prefix, a u32, the sketch, an
//!   entity (a polygon's or circular pattern's centre point, a rectangular
//!   pattern's direction line) and its pattern record (none for a polygon)
//!   *(seen in majors 21 to 27)*.
//! - A member (`1f89dbfb…`, a constraint of the sketch): its u32 after the
//!   parameter are two entities, the group's entity, the group twice, a u32
//!   and the pattern record. In a polygon the two entities are adjacent
//!   sides; in a pattern a copy and its original.
//! - A circular pattern (`e6ded4b5…`) and a rectangular one (`637dc972…`):
//!   the header, an i32 −1 (in every major seen), a map (kind 6) of the
//!   entities to their instances (`9b68a675…`), the sketch, and among the
//!   references after it the count (an integer parameter) and the angle (a
//!   parameter), or per direction its count and spacing (the first
//!   direction's first).

use std::collections::{HashMap, HashSet};

use serde_json::{Value, json};

use crate::dc::{self, Definitions, type_id};
use crate::params::{INTEGER, PARAMETER, Parameters};
use crate::sketch::{LINE, POINT, constraint_body, entity, point};

pub const GROUP: [u8; 16] = type_id("384853529f445255e1e1dab410eb06c4");
pub const MEMBER: [u8; 16] = type_id("1f89dbfbd84778b6b0967285c7ec8bb9");
pub const CIRCULAR: [u8; 16] = type_id("e6ded4b5394cb872ef2d81a89c35c763");
pub const RECTANGULAR: [u8; 16] = type_id("637dc972644f10ae9bfa398be861dd47");

/// A group record: its entity and pattern record.
fn group(dc: &Definitions, record: usize) -> Option<(usize, Option<usize>)> {
    let mut r = dc.fields(record);
    r.u32().ok()?;
    r.reference().ok()?;
    let entity = r.reference().ok()??;
    let pattern = r.reference().ok().flatten();
    Some((entity, pattern))
}

/// A member's two entities and its group.
fn member(dc: &Definitions, record: usize) -> Option<(usize, usize, usize)> {
    let (_, rest) = constraint_body(dc, record).ok()?;
    let r = |k: usize| rest.get(k).and_then(|&v| dc::reference(v));
    Some((r(0)?, r(1)?, r(3)?))
}

/// The references of a pattern record after its map and sketch, in order.
fn pattern_references(dc: &Definitions, record: usize) -> Vec<usize> {
    let bytes = dc.bytes(record);
    let mut r = dc.body(record);
    if r.i32().ok() != Some(-1) {
        return Vec::new();
    }
    // The map: u16 kind 6, u16 0x3000, u32 count, two u32, the pairs.
    let start = (|| -> dc::Result<usize> {
        let (kind, tag, n) = (r.u16()?, r.u16()?, r.u32()? as usize);
        if kind != 6 || tag != 0x3000 {
            return Err(dc::DcError("no map".to_owned()));
        }
        if n > 0 {
            r.skip(8)?;
            r.skip(8 * n)?;
        }
        Ok(r.pos)
    })();
    let Ok(mut at) = start else {
        return Vec::new();
    };
    let mut out = Vec::new();
    while at + 4 <= bytes.len() {
        let v = u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]);
        match dc::reference(v) {
            Some(p) if v & 0x8000_0000 != 0 && p < dc.len() => {
                out.push(p);
                at += 4;
            }
            _ => at += 1,
        }
    }
    out
}

/// A line's two end points.
fn line_ends(dc: &Definitions, line: usize) -> Option<(usize, usize)> {
    if !dc.is(line, &LINE) {
        return None;
    }
    let (_, mut r) = entity(dc, line).ok()?;
    let ends = r.references().ok()?;
    Some((*ends.first()?, *ends.get(1)?))
}

/// The groups of a sketch as the dump's constraints (without ids), the
/// records they account for (groups, members, patterns; the members of a
/// group not decoded too, which is reported once), and what could not be
/// decoded.
pub fn decode(
    dc: &Definitions,
    entities: &[usize],
    ids: &HashMap<usize, String>,
    params: &Parameters,
) -> (Vec<Value>, HashSet<usize>, Vec<String>) {
    let mut out = Vec::new();
    let mut done = HashSet::new();
    let mut dropped = Vec::new();
    // Members by group.
    let mut members: HashMap<usize, Vec<(usize, usize, usize)>> = HashMap::new();
    for &e in entities.iter().filter(|&&e| dc.is(e, &MEMBER)) {
        if let Some((a, b, g)) = member(dc, e) {
            members.entry(g).or_default().push((e, a, b));
        }
    }
    let id = |r: usize| ids.get(&r).cloned();
    let parameter = |r: usize| {
        params
            .by_record(r)
            .map(|p| json!({"kind": "parameter", "name": p.name, "value": p.value}))
    };
    // Rectangular patterns: their groups (one per direction) together.
    let mut rectangular: Vec<(usize, Vec<usize>)> = Vec::new();
    for &g in entities.iter().filter(|&&e| dc.is(e, &GROUP)) {
        let Some((anchor, pattern)) = group(dc, g) else {
            dropped.push(format!("group {g}: not read"));
            continue;
        };
        let mine = members.get(&g).cloned().unwrap_or_default();
        let result = match pattern {
            None if dc.is(anchor, &POINT) => polygon(dc, anchor, &mine, &id).map(|c| vec![c]),
            Some(p) if dc.is(p, &CIRCULAR) => circular(dc, p, anchor, &mine, &id, &parameter),
            Some(p) if dc.is(p, &RECTANGULAR) => {
                match rectangular.iter_mut().find(|(q, _)| *q == p) {
                    Some((_, groups)) => groups.push(g),
                    None => rectangular.push((p, vec![g])),
                }
                continue;
            }
            _ => Err("a group of another kind".to_owned()),
        };
        match result {
            Ok(c) => {
                out.extend(c);
                done.insert(g);
                done.extend(mine.iter().map(|m| m.0));
                done.extend(pattern);
            }
            Err(e) => {
                // Counted once, with the group.
                done.insert(g);
                done.extend(mine.iter().map(|m| m.0));
                dropped.push(format!("group {g}: {e}"));
            }
        }
    }
    for (p, groups) in rectangular {
        let result = rectangular_pattern(dc, p, &groups, &members, &id, &parameter);
        match result {
            Ok(c) => {
                out.push(c);
                done.insert(p);
                for g in &groups {
                    done.insert(*g);
                    done.extend(members.get(g).into_iter().flatten().map(|m| m.0));
                }
            }
            Err(e) => {
                for g in &groups {
                    done.insert(*g);
                    done.extend(members.get(g).into_iter().flatten().map(|m| m.0));
                }
                dropped.push(format!("rectangular pattern {p}: {e}"));
            }
        }
    }
    (out, done, dropped)
}

/// An offset (`bc144b4e…`, a constraint of the sketch): its u32 after the
/// parameter are two pairs of curves, each a curve and its offset, the
/// pairs next to each other along the offset chain *(seen)*.
pub const OFFSET: [u8; 16] = type_id("bc144b4ed211fea4600010b38932edb0");

/// The offsets of a sketch: the pairs of its offset records joined into
/// chains, each the dump's `OffsetConstraint` (without an id) with its
/// source and offset curves, its distance (cm) and the dimension between a
/// pair that drives it (one of `dimensions`, the dump's). What could not
/// be read is returned with why.
pub fn offsets(
    dc: &Definitions,
    records: &[usize],
    ids: &HashMap<usize, String>,
    dimensions: &[Value],
) -> (Vec<Value>, Vec<String>) {
    let mut pairs: Vec<(usize, usize)> = Vec::new();
    let mut links: Vec<(usize, usize)> = Vec::new();
    let mut dropped = Vec::new();
    for &r in records {
        let rest = constraint_body(dc, r)
            .map(|(_, rest)| rest)
            .unwrap_or_default();
        let curves: Vec<usize> = rest
            .iter()
            .take(4)
            .filter_map(|&v| dc::reference(v))
            .collect();
        if curves.len() != 4 {
            dropped.push(format!("offset {r}: not read"));
            continue;
        }
        let mut at = |p: (usize, usize)| match pairs.iter().position(|&q| q == p) {
            Some(k) => k,
            None => {
                pairs.push(p);
                pairs.len() - 1
            }
        };
        let (a, b) = (at((curves[0], curves[1])), at((curves[2], curves[3])));
        links.push((a, b));
    }
    // Chains: pairs joined by the records.
    let mut chain: Vec<usize> = (0..pairs.len()).collect();
    fn root(chain: &[usize], k: usize) -> usize {
        let mut k = k;
        while chain[k] != k {
            k = chain[k];
        }
        k
    }
    for &(a, b) in &links {
        let (ra, rb) = (root(&chain, a), root(&chain, b));
        chain[ra] = rb;
    }
    let mut out = Vec::new();
    let mut roots: Vec<usize> = Vec::new();
    for k in 0..pairs.len() {
        let r = root(&chain, k);
        if !roots.contains(&r) {
            roots.push(r);
        }
    }
    for r in roots {
        let members: Vec<(usize, usize)> = (0..pairs.len())
            .filter(|&k| root(&chain, k) == r)
            .map(|k| pairs[k])
            .collect();
        let result = (|| -> Result<Value, String> {
            let name = |c: usize| ids.get(&c).cloned().ok_or("a curve that was not read");
            let parents: Vec<String> = members
                .iter()
                .map(|&(p, _)| name(p))
                .collect::<Result<_, _>>()?;
            let children: Vec<String> = members
                .iter()
                .map(|&(_, c)| name(c))
                .collect::<Result<_, _>>()?;
            let distance = members
                .iter()
                .find_map(|&(p, c)| offset_distance(dc, p, c))
                .ok_or("its distance was not measured")?;
            let mut props = json!({"parentCurves": parents, "childCurves": children,
                                   "distance": distance});
            // The dimension between a curve and its offset drives it.
            let between = |d: &Value| {
                let refs: Vec<&str> = d["refs"]
                    .as_object()
                    .into_iter()
                    .flat_map(|o| o.values())
                    .filter_map(Value::as_str)
                    .collect();
                parents.iter().zip(&children).any(|(p, c)| {
                    refs.len() == 2 && refs.contains(&p.as_str()) && refs.contains(&c.as_str())
                })
            };
            if let Some(d) = dimensions.iter().find(|d| between(d)) {
                props["dimension"] = d["id"].clone();
            }
            Ok(json!({"type": "OffsetConstraint", "props": props}))
        })();
        match result {
            Ok(c) => out.push(c),
            Err(e) => dropped.push(format!("an offset of {} curves: {e}", members.len())),
        }
    }
    (out, dropped)
}

/// The distance between a curve and its offset (cm): parallel lines, or
/// concentric circles or arcs.
fn offset_distance(dc: &Definitions, a: usize, b: usize) -> Option<f64> {
    if dc.is(a, &LINE) && dc.is(b, &LINE) {
        let (p, q) = line_ends(dc, a)?;
        let (p, q) = (point(dc, p)?, point(dc, q)?);
        let (s, _) = line_ends(dc, b)?;
        let s = point(dc, s)?;
        let (dx, dy) = (q[0] - p[0], q[1] - p[1]);
        let n = dx.hypot(dy);
        return (n > 1e-12).then(|| ((s[0] - p[0]) * dy - (s[1] - p[1]) * dx).abs() / n);
    }
    let round = |c: usize| -> Option<([f64; 2], f64)> {
        if !dc.is(c, &crate::sketch::CIRCLE) {
            return None;
        }
        let bytes = dc.bytes(c);
        let mut tail = dc::Reader::at(bytes, bytes.len().checked_sub(13)?);
        let center = point(dc, tail.reference().ok()??)?;
        Some((center, tail.f64().ok()?))
    };
    let ((ca, ra), (cb, rb)) = (round(a)?, round(b)?);
    ((ca[0] - cb[0]).hypot(ca[1] - cb[1]) < 1e-9).then(|| (ra - rb).abs())
}

/// A polygon: its corners in order around it, then its centre.
fn polygon(
    dc: &Definitions,
    center: usize,
    members: &[(usize, usize, usize)],
    id: &dyn Fn(usize) -> Option<String>,
) -> Result<Value, String> {
    // Members relate adjacent sides, or adjacent corners.
    let mut sides: Vec<usize> = Vec::new();
    for &(_, a, b) in members {
        for s in [a, b] {
            if dc.is(s, &LINE) && !sides.contains(&s) {
                sides.push(s);
            }
        }
    }
    if sides.len() < 3 {
        return Err("fewer than three sides".to_owned());
    }
    let ends: Vec<(usize, usize)> = sides
        .iter()
        .map(|&s| line_ends(dc, s).ok_or("a side is not a line"))
        .collect::<Result<_, _>>()?;
    // Walk around: from the first side's end, the side that starts or ends
    // there.
    let (first, mut at) = ends[0];
    let mut corners = vec![first];
    let mut used = vec![0];
    while at != first {
        corners.push(at);
        let next = (0..ends.len())
            .find(|k| !used.contains(k) && (ends[*k].0 == at || ends[*k].1 == at))
            .ok_or("the sides do not close")?;
        used.push(next);
        at = if ends[next].0 == at {
            ends[next].1
        } else {
            ends[next].0
        };
    }
    if used.len() != sides.len() {
        return Err("the sides do not close".to_owned());
    }
    let mut list: Vec<String> = corners
        .iter()
        .map(|&p| id(p).ok_or("a corner that was not read"))
        .collect::<Result<_, _>>()?;
    list.push(id(center).ok_or("its centre was not read")?);
    Ok(json!({"type": "PolygonConstraint", "refs": {"entities": list}}))
}

/// Originals and copies of a pattern's members (copy, source), as ids: a
/// member's source can be an earlier copy (the pattern's instances one
/// after the other); the originals are the sources that are no copies.
fn originals_and_copies(
    members: &[(usize, usize, usize)],
    id: &dyn Fn(usize) -> Option<String>,
) -> Result<(Vec<String>, Vec<String>), String> {
    let (mut sources, mut copies) = (Vec::new(), Vec::new());
    for &(_, copy, source) in members {
        let (c, o) = (
            id(copy).ok_or("a copy that was not read")?,
            id(source).ok_or("an original that was not read")?,
        );
        if !copies.contains(&c) {
            copies.push(c);
        }
        if !sources.contains(&o) {
            sources.push(o);
        }
    }
    let originals: Vec<String> = sources
        .into_iter()
        .filter(|o| !copies.contains(o))
        .collect();
    if copies.is_empty() || originals.is_empty() {
        return Err("no originals or no copies".to_owned());
    }
    Ok((originals, copies))
}

/// A circular pattern: its count and angle among its references.
fn circular(
    dc: &Definitions,
    pattern: usize,
    center: usize,
    members: &[(usize, usize, usize)],
    id: &dyn Fn(usize) -> Option<String>,
    parameter: &dyn Fn(usize) -> Option<Value>,
) -> Result<Vec<Value>, String> {
    // Copies on their sources (a pattern about the original's own centre)
    // would make the same profile several times: each copy is held on its
    // source instead, a point coincident with its source point, a circle
    // or arc of its source's radius (their centres are members too)
    // *(seen)*.
    let shifts: Vec<f64> = members
        .iter()
        .filter_map(|&(_, copy, source)| {
            let (a, b) = (point(dc, copy)?, point(dc, source)?);
            Some((a[0] - b[0]).hypot(a[1] - b[1]))
        })
        .collect();
    if !shifts.is_empty() && shifts.iter().all(|&d| d < 1e-9) {
        let mut out = Vec::new();
        for &(_, copy, source) in members {
            let (Some(c), Some(s)) = (id(copy), id(source)) else {
                return Err("its copies lie on their originals, one not read".to_owned());
            };
            if dc.is(copy, &POINT) && dc.is(source, &POINT) {
                out.push(
                    json!({"type": "CoincidentConstraint", "refs": {"point": c, "entity": s}}),
                );
            } else if dc.is(copy, &crate::sketch::CIRCLE) && dc.is(source, &crate::sketch::CIRCLE) {
                out.push(
                    json!({"type": "EqualConstraint", "refs": {"curveOne": c, "curveTwo": s}}),
                );
            } else if !(dc.is(copy, &LINE) && dc.is(source, &LINE)) {
                // Lines follow their end points.
                return Err("its copies lie on their originals, not points or circles".to_owned());
            }
        }
        return Ok(out);
    }
    let refs = pattern_references(dc, pattern);
    let count = refs
        .iter()
        .find(|&&r| dc.is(r, &INTEGER))
        .and_then(|&r| parameter(r))
        .ok_or("its count was not read")?;
    let angle = refs
        .iter()
        .find(|&&r| dc.is(r, &PARAMETER))
        .and_then(|&r| parameter(r))
        .ok_or("its angle was not read")?;
    let (originals, copies) = originals_and_copies(members, id)?;
    let center = id(center).ok_or("its centre was not read")?;
    let mut all = originals;
    all.extend(copies.iter().cloned());
    all.push(center.clone());
    Ok(vec![
        json!({"type": "CircularPatternConstraint", "refs": {"entities": all},
              "props": {"centerPoint": center, "createdEntities": copies,
                        "quantity": count, "totalAngle": angle}}),
    ])
}

/// A rectangular pattern: per direction (a group with its line) the count
/// and spacing among the pattern's references, in the directions' order.
/// The direction is the line's, turned where the copies lie the other way.
fn rectangular_pattern(
    dc: &Definitions,
    pattern: usize,
    groups: &[usize],
    members: &HashMap<usize, Vec<(usize, usize, usize)>>,
    id: &dyn Fn(usize) -> Option<String>,
    parameter: &dyn Fn(usize) -> Option<Value>,
) -> Result<Value, String> {
    let refs = pattern_references(dc, pattern);
    let counts: Vec<usize> = refs
        .iter()
        .copied()
        .filter(|&r| dc.is(r, &INTEGER))
        .collect();
    let spacings: Vec<usize> = refs
        .iter()
        .copied()
        .filter(|&r| dc.is(r, &PARAMETER))
        .collect();
    let lines: Vec<usize> = refs.iter().copied().filter(|&r| dc.is(r, &LINE)).collect();
    let mut all_members = Vec::new();
    for g in groups {
        all_members.extend(members.get(g).into_iter().flatten().copied());
    }
    let (originals, copies) = originals_and_copies(&all_members, id)?;
    let mut props = serde_json::Map::new();
    props.insert("createdEntities".into(), json!(copies));
    let keys = [
        ("quantityOne", "distanceOne", "directionOne"),
        ("quantityTwo", "distanceTwo", "directionTwo"),
    ];
    for (k, (quantity, distance, direction)) in keys.into_iter().enumerate() {
        let (Some(&c), Some(&s), Some(&line)) = (counts.get(k), spacings.get(k), lines.get(k))
        else {
            if k == 0 {
                return Err("its first direction was not read".to_owned());
            }
            // One direction: the second has one instance.
            let first: Vec<f64> = serde_json::from_value(props["directionOne"].clone())
                .map_err(|_| "no first direction")?;
            props.insert(quantity.into(), json!({"kind": "parameter", "value": 1.0}));
            props.insert(distance.into(), json!({"kind": "parameter", "value": 0.0}));
            props.insert(direction.into(), json!([-first[1], first[0], 0.0]));
            break;
        };
        let (a, b) = line_ends(dc, line).ok_or("a direction that is not a line")?;
        let (Some(pa), Some(pb)) = (point(dc, a), point(dc, b)) else {
            return Err("a direction line's ends were not read".to_owned());
        };
        let d = [pb[0] - pa[0], pb[1] - pa[1]];
        let n = d[0].hypot(d[1]);
        if n < 1e-12 {
            return Err("a zero direction".to_owned());
        }
        let mut u = [d[0] / n, d[1] / n];
        // The copies of this direction's group: the side they lie on.
        let along: f64 = groups
            .iter()
            .filter(|&&g| group(dc, g).is_some_and(|(e, _)| e == line))
            .flat_map(|g| members.get(g).into_iter().flatten())
            .filter_map(|&(_, copy, original)| {
                let (c, o) = (point(dc, copy)?, point(dc, original)?);
                Some((c[0] - o[0]) * u[0] + (c[1] - o[1]) * u[1])
            })
            .sum();
        let spacing = parameter(s).ok_or("a spacing was not read")?;
        let positive = spacing["value"].as_f64().is_some_and(|v| v >= 0.0);
        if (along < 0.0) == positive {
            u = [-u[0], -u[1]];
        }
        props.insert(quantity.into(), parameter(c).ok_or("a count was not read")?);
        props.insert(distance.into(), spacing);
        props.insert(direction.into(), json!([u[0], u[1], 0.0]));
    }
    let mut all = originals;
    all.extend(copies);
    Ok(
        json!({"type": "RectangularPatternConstraint", "refs": {"entities": all},
              "props": props}),
    )
}
