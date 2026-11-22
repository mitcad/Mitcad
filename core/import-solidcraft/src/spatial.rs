// SPDX-License-Identifier: MIT
//! A sketch's 3D curves as a MitCad 3D sketch (`spatial_sketch`,
//! mitcad#2157) on its component's XY plane, the points in component
//! coordinates (a curve in world coordinates through the component's
//! placement).
//!
//! - Drawn curves: a point, a line (two fit points), an exact arc (centre,
//!   ends and normal), a fit spline (`fitted_spline` through the fit
//!   points; MitCad fits its own cubic through them where the file's runs a
//!   Catmull-Rom curve). A linked curve's exact pieces are fixed curves:
//!   lines, arcs (their normals held), circles, ellipses and splines as
//!   B-splines (rational where the file's are, a circle's and an
//!   ellipse's exact quadratic ones, a piece of a longer spline cut out by
//!   knot insertion). A linked curve known only by its sampled points
//!   stays left out with a note.
//! - Ends that meet share one point; in a design whose 3D curves were
//!   joined by relations (format 58's `wires_joined`) only those relations
//!   join them.
//! - Format 58's relations and dimensions map to their `sketch3d`
//!   counterparts (see [`Importer::spatial_constraint`]); a design without
//!   3D relations gets every drawn point fixed where the file has it.

use std::collections::{BTreeMap, HashMap};

use mitcad_model::{ComponentUid, FeatureUid, Kernel};
use mitcad_solidcraft::document::{Mat, apply_point, apply_vector};
use mitcad_solidcraft::sketch::{
    RecordCurve, Sketch, Wire, WireConstraint, WireConstraintKind, WirePoint, WireSpace,
};
use serde_json::{Value, json};

use super::planes::{is_identity, rigid_inverse};
use super::report::SketchReport;
use super::{Importer, NotMapped};
use mitcad_import_base::geom::{add, cross, dot, norm, scale, sub, unit};

/// Two points closer than this (mm) meet.
const MEET: f64 = 1e-6;
/// How far (mm) a 3D curve may stray from the line between its ends and
/// still be a line (SolidCraft's `eval_frame.rs`).
const STRAIGHT_TOL: f64 = 1e-3;

/// A 3D curve of the file as it came in.
#[derive(Debug, Clone)]
pub(crate) struct WireMade {
    pub id: String,
    /// A linked curve (projected, included, intersected).
    pub linked: bool,
    /// Its points in component coordinates: the fit points of a drawn
    /// curve, else its samples.
    pub pts: Vec<[f64; 3]>,
    /// Its MitCad curves (a linked curve's pieces, in order).
    pub curves: Vec<String>,
}

impl WireMade {
    /// Its ends when it runs straight from its first point to its last
    /// (SolidCraft's `straight`).
    pub fn straight(&self) -> Option<([f64; 3], [f64; 3])> {
        straight(&self.pts)
    }
}

/// A 3D sketch made from a file sketch's 3D curves.
#[derive(Debug, Clone)]
pub(crate) struct SpatialMade {
    pub uid: FeatureUid,
    /// The file's 3D curves that came in, in the file's order.
    pub wires: Vec<WireMade>,
    /// Each fit point's MitCad point, by the curve's id and index.
    pub points: HashMap<(String, usize), String>,
}

impl SpatialMade {
    pub fn wire(&self, id: &str) -> Option<&WireMade> {
        self.wires.iter().find(|w| w.id == id)
    }

    /// The typed path along these 3D curves.
    pub fn path(&self, ids: &[String]) -> Result<Value, String> {
        let mut curves = Vec::new();
        for id in ids {
            let w = self
                .wire(id)
                .ok_or_else(|| format!("the 3D curve {id} did not come in"))?;
            curves.extend(w.curves.iter().cloned());
        }
        Ok(json!({"spatial_sketch": self.uid.to_string(), "curves": curves}))
    }

    /// A 3D curve's point (`w3:1`, its fit point 1), as SolidCraft's
    /// references name it (format 52).
    pub fn node(&self, key: &str) -> Option<String> {
        let (wire, index) = key.rsplit_once(':')?;
        self.points
            .get(&(wire.to_owned(), index.parse().ok()?))
            .cloned()
    }
}

/// The translated definition.
struct Translated {
    def: Value,
    /// The same without relations and dimensions: every point fixed.
    bare: Value,
    wires: Vec<WireMade>,
    points: HashMap<(String, usize), String>,
    dropped: Vec<String>,
    notes: Vec<String>,
    relations: usize,
    dimensions: usize,
    relations_mapped: usize,
    dimensions_mapped: usize,
}

/// A point slot: a fit point, an arc's centre or a piece's end.
#[derive(Debug, Clone)]
struct Slot {
    at: [f64; 3],
    /// Its MitCad point once emitted.
    id: Option<String>,
}

/// Union-find over slots.
fn find(parent: &mut [usize], mut i: usize) -> usize {
    while parent[i] != i {
        parent[i] = parent[parent[i]];
        i = parent[i];
    }
    i
}

fn union(parent: &mut [usize], a: usize, b: usize) {
    let (ra, rb) = (find(parent, a), find(parent, b));
    if ra != rb {
        parent[rb.max(ra)] = rb.min(ra);
    }
}

/// A slot's point (the point of its group), emitted when first used;
/// `fixed`: held where it is (always in the bare definition).
fn slot_point(
    e: &mut Emit,
    slots: &mut [Slot],
    parent: &mut [usize],
    i: usize,
    fixed: bool,
) -> String {
    let r = find(parent, i);
    if let Some(id) = &slots[r].id {
        return id.clone();
    }
    let at = slots[r].at;
    let id = e.point(at);
    e.fix(&id, at, fixed);
    slots[r].id = Some(id.clone());
    id
}

/// The ends of a polyline that runs straight (every point within
/// [`STRAIGHT_TOL`] of the line between its ends, in order).
pub(crate) fn straight(pts: &[[f64; 3]]) -> Option<([f64; 3], [f64; 3])> {
    let (a, b) = (*pts.first()?, *pts.last()?);
    let len = norm(sub(b, a));
    let d = unit(sub(b, a))?;
    let mut reached = 0.0_f64;
    for p in pts {
        let q = sub(*p, a);
        let t = dot(q, d);
        if norm(sub(q, scale(d, t))) > STRAIGHT_TOL
            || t < reached - STRAIGHT_TOL
            || t > len + STRAIGHT_TOL
        {
            return None;
        }
        reached = reached.max(t);
    }
    Some((a, b))
}

/// The emitting state: the definition's lists and the id counters.
struct Emit {
    points: Vec<Value>,
    curves: Vec<Value>,
    constraints: Vec<Value>,
    dimensions: Vec<Value>,
    /// Fixes of every point (the bare definition's relations).
    fixes: Vec<Value>,
    next: usize,
    next_k: usize,
    origin: Option<String>,
}

impl Emit {
    fn id(&mut self, prefix: char) -> String {
        let id = format!("{prefix}{}", self.next);
        self.next += 1;
        id
    }

    fn k(&mut self) -> String {
        let id = format!("k{}", self.next_k);
        self.next_k += 1;
        id
    }

    fn point(&mut self, at: [f64; 3]) -> String {
        let id = self.id('p');
        self.points.push(json!({"id": id, "at": at}));
        id
    }

    fn relation(&mut self, mut r: Value) {
        r["id"] = json!(self.k());
        self.constraints.push(r);
    }

    /// Holds a point where it is: a relation when `keep`, and always in
    /// the bare definition.
    fn fix(&mut self, point: &str, at: [f64; 3], keep: bool) {
        let r = json!({"type": "fix", "point": point, "at": at});
        if keep {
            self.relation(r.clone());
        }
        self.fixes.push(r);
    }

    /// The point attached to the sketch's origin (mitcad#1912's).
    fn origin(&mut self) -> String {
        if let Some(o) = &self.origin {
            return o.clone();
        }
        let o = self.point([0.0; 3]);
        self.relation(json!({"type": "attach", "point": o, "source": "origin"}));
        self.origin = Some(o.clone());
        o
    }
}

/// An origin plane's normal axis in the sketch's frame (`xy`: `z`).
fn plane_axis(plane: &str) -> Option<&'static str> {
    match plane.to_ascii_lowercase().as_str() {
        "xy" | "yx" => Some("z"),
        "yz" | "zy" => Some("x"),
        "xz" | "zx" => Some("y"),
        _ => None,
    }
}

fn axis_index(axis: &str) -> Option<usize> {
    match axis.to_ascii_lowercase().as_str() {
        "x" => Some(0),
        "y" => Some(1),
        "z" => Some(2),
        _ => None,
    }
}

impl<K: Kernel> Importer<'_, '_, K> {
    /// Adds the 3D sketch of a file sketch's 3D curves, named `name`, in
    /// `component`; the sketch report and the notes it leaves.
    pub(super) fn spatial_sketch(
        &mut self,
        id: u64,
        name: &str,
        s: &Sketch,
        file_component: u64,
        component: ComponentUid,
    ) -> Result<(SpatialMade, Vec<String>), NotMapped> {
        let t = self.translate_spatial(s, file_component)?;
        let mut notes = t.notes.clone();
        let (uid, mapped) = match self.add(&t.def, Some(name), component) {
            Ok(uid) => (uid, true),
            Err(e) => {
                self.params.forget_pending();
                let uid = self.add(&t.bare, Some(name), component).map_err(|e2| {
                    NotMapped::Failed(format!("its 3D curves: {e}; without their relations: {e2}"))
                })?;
                notes.push(format!(
                    "the 3D curves came in fixed where the file has them, without their relations and dimensions: {e}"
                ));
                (uid, false)
            }
        };
        if !t.dropped.is_empty() {
            notes.push(format!(
                "left out of the 3D sketch: {}",
                t.dropped.join(", ")
            ));
        }
        self.report.sketches.push(SketchReport {
            id,
            name: name.to_owned(),
            points: t.points.len(),
            curves: t.wires.len(),
            constraints: t.relations,
            dimensions: t.dimensions,
            constraints_mapped: if mapped { t.relations_mapped } else { 0 },
            dimensions_mapped: if mapped { t.dimensions_mapped } else { 0 },
            dropped: t.dropped,
        });
        Ok((
            SpatialMade {
                uid,
                wires: t.wires,
                points: t.points,
            },
            notes,
        ))
    }

    fn translate_spatial(
        &mut self,
        s: &Sketch,
        file_component: u64,
    ) -> Result<Translated, NotMapped> {
        let mut dropped = Vec::new();
        let mut notes = Vec::new();
        let wires: Vec<Wire> = s
            .wires
            .iter()
            .filter_map(|v| match Wire::read(v) {
                Ok(w) => Some(w),
                Err(e) => {
                    dropped.push(e);
                    None
                }
            })
            .collect();
        // World coordinates through the component's placement.
        let world = self.design.component_world(file_component);
        let inverse = rigid_inverse(&world);
        let identity = is_identity(&inverse);
        let turned = !is_identity(&{
            let mut m = inverse;
            m[3] = [0.0, 0.0, 0.0, 1.0];
            m
        });
        if !identity
            && wires.iter().any(|w| w.space == WireSpace::World)
            && self
                .design
                .occurrences
                .iter()
                .filter(|o| o.component == file_component)
                .count()
                > 1
        {
            notes.push(
                "its 3D curves in world coordinates are placed in the component through its first placement"
                    .to_owned(),
            );
        }
        let to_local = |space: WireSpace, p: [f64; 3]| -> [f64; 3] {
            match space {
                WireSpace::World if !identity => apply_point(&inverse, p),
                _ => p,
            }
        };
        let dir_local = |space: WireSpace, d: [f64; 3]| -> [f64; 3] {
            match space {
                WireSpace::World if !identity => apply_vector(&inverse, d),
                _ => d,
            }
        };
        let constraints: Vec<WireConstraint> = s
            .wire_constraints
            .iter()
            .filter_map(|v| match WireConstraint::read(v) {
                Ok(c) => Some(c),
                Err(e) => {
                    dropped.push(e);
                    None
                }
            })
            .collect();
        let relational = !constraints.is_empty();

        // Slots: every fit point of a drawn curve, an arc's centre, the
        // ends of a linked curve's pieces.
        let mut slots: Vec<Slot> = Vec::new();
        let mut slot_of: HashMap<(String, usize), usize> = HashMap::new();
        let mut centers: HashMap<String, usize> = HashMap::new();
        // A linked curve's pieces: (start slot, end slot, the piece).
        let mut pieces: HashMap<String, Vec<(usize, usize, Piece)>> = HashMap::new();
        // The slots at the ends of each curve (for joining).
        let mut ends: Vec<usize> = Vec::new();
        let mut made: Vec<(Wire, Vec<[f64; 3]>)> = Vec::new();
        for w in wires {
            if w.link.is_none() && !w.fit.is_empty() {
                let pts: Vec<[f64; 3]> = w.fit.iter().map(|p| to_local(w.space, *p)).collect();
                for (i, p) in pts.iter().enumerate() {
                    slot_of.insert((w.id.clone(), i), slots.len());
                    slots.push(Slot { at: *p, id: None });
                }
                if let Some(arc) = &w.arc
                    && pts.len() == 3
                {
                    centers.insert(w.id.clone(), slots.len());
                    slots.push(Slot {
                        at: to_local(w.space, arc.center),
                        id: None,
                    });
                }
                ends.push(slot_of[&(w.id.clone(), 0)]);
                ends.push(slot_of[&(w.id.clone(), pts.len() - 1)]);
                made.push((w, pts));
                continue;
            }
            if w.link.is_some() && !w.exact.is_empty() {
                let mut list = Vec::new();
                let mut failed = None;
                for piece in &w.exact {
                    match Piece::of(&piece.curve, piece.interval) {
                        Ok(ps) => {
                            list.extend(ps.into_iter().map(|p| p.mapped(&|p| to_local(w.space, p))))
                        }
                        Err(e) => {
                            failed = Some(e);
                            break;
                        }
                    }
                }
                if let Some(e) = failed {
                    dropped.push(format!("{}: {e}", w.id));
                    continue;
                }
                let mut out = Vec::new();
                let mut prev: Option<usize> = None;
                for p in list {
                    let start = match prev {
                        Some(e) if norm(sub(slots[e].at, p.start())) < MEET => e,
                        _ => {
                            slots.push(Slot {
                                at: p.start(),
                                id: None,
                            });
                            slots.len() - 1
                        }
                    };
                    let end = if p.closed() {
                        start
                    } else {
                        slots.push(Slot {
                            at: p.end(),
                            id: None,
                        });
                        slots.len() - 1
                    };
                    prev = Some(end);
                    out.push((start, end, p));
                }
                if let (Some(first), Some(last)) = (out.first(), out.last()) {
                    ends.push(first.0);
                    ends.push(last.1);
                }
                let pts: Vec<[f64; 3]> = w.pts.iter().map(|p| to_local(w.space, *p)).collect();
                pieces.insert(w.id.clone(), out);
                made.push((w, pts));
                continue;
            }
            if w.link.is_some() {
                dropped.push(format!(
                    "{}: a linked curve known only by its points (projected onto a face)",
                    w.id
                ));
            } else {
                dropped.push(format!("{}: a 3D curve without points", w.id));
            }
        }

        // Points that are one.
        let mut parent: Vec<usize> = (0..slots.len()).collect();
        for c in &constraints {
            if let WireConstraintKind::Coincident { a, b } = &c.kind
                && let (Some(&i), Some(&j)) = (slot_of.get(&key(a)), slot_of.get(&key(b)))
            {
                union(&mut parent, i, j);
            }
        }
        if !s.wires_joined {
            for (n, &i) in ends.iter().enumerate() {
                for &j in &ends[n + 1..] {
                    if norm(sub(slots[i].at, slots[j].at)) < MEET {
                        union(&mut parent, i, j);
                    }
                }
            }
        }

        let mut e = Emit {
            points: Vec::new(),
            curves: Vec::new(),
            constraints: Vec::new(),
            dimensions: Vec::new(),
            fixes: Vec::new(),
            next: 1,
            next_k: 1,
            origin: None,
        };

        let mut out_wires = Vec::new();
        let mut points_of: HashMap<(String, usize), String> = HashMap::new();
        for (w, pts) in &made {
            let mut curves = Vec::new();
            if let Some(list) = pieces.get(&w.id) {
                for (start, end, p) in list {
                    let a = slot_point(&mut e, &mut slots, &mut parent, *start, true);
                    let b = slot_point(&mut e, &mut slots, &mut parent, *end, true);
                    let id = e.id('c');
                    match p {
                        Piece::Line { .. } => {
                            e.curves
                                .push(json!({"id": id, "type": "line", "start": a, "end": b}));
                        }
                        Piece::Arc { center, normal, .. } => {
                            let c = e.point(*center);
                            e.fix(&c, *center, true);
                            e.curves.push(json!({"id": id, "type": "arc", "center": c,
                                "start": a, "end": b, "normal": normal}));
                            e.relation(
                                json!({"type": "normal_along", "curve": id, "direction": normal}),
                            );
                        }
                        Piece::Spline {
                            degree,
                            poles,
                            weights,
                            knots,
                        } => {
                            let mut control = vec![a.clone()];
                            for pole in &poles[1..poles.len() - 1] {
                                let q = e.point(*pole);
                                e.fix(&q, *pole, true);
                                control.push(q);
                            }
                            control.push(b.clone());
                            let mut c = json!({"id": id, "type": "spline", "degree": degree,
                                "control": control, "knots": knots});
                            if weights.iter().any(|w| (w - 1.0).abs() > 1e-15) {
                                c["weights"] = json!(weights);
                            }
                            e.curves.push(c);
                        }
                    }
                    curves.push(id);
                }
                out_wires.push(WireMade {
                    id: w.id.clone(),
                    linked: true,
                    pts: pts.clone(),
                    curves,
                });
                continue;
            }
            let fixed = !relational;
            let n = pts.len();
            let slot = |i: usize| slot_of[&(w.id.clone(), i)];
            if n == 1 {
                let p = slot_point(&mut e, &mut slots, &mut parent, slot(0), fixed);
                points_of.insert((w.id.clone(), 0), p);
            } else if let (Some(arc), 3, Some(&c)) = (&w.arc, n, centers.get(&w.id)) {
                let a = slot_point(&mut e, &mut slots, &mut parent, slot(0), fixed);
                let b = slot_point(&mut e, &mut slots, &mut parent, slot(2), fixed);
                let center = slot_point(&mut e, &mut slots, &mut parent, c, fixed);
                let id = e.id('c');
                e.curves
                    .push(json!({"id": id, "type": "arc", "center": center,
                    "start": a, "end": b,
                    "normal": unit(dir_local(w.space, arc.normal)).unwrap_or([0.0, 0.0, 1.0])}));
                points_of.insert((w.id.clone(), 0), a);
                points_of.insert((w.id.clone(), 2), b);
                curves.push(id);
            } else if n == 2 {
                let a = slot_point(&mut e, &mut slots, &mut parent, slot(0), fixed);
                let b = slot_point(&mut e, &mut slots, &mut parent, slot(1), fixed);
                let id = e.id('c');
                e.curves
                    .push(json!({"id": id, "type": "line", "start": a, "end": b}));
                points_of.insert((w.id.clone(), 0), a);
                points_of.insert((w.id.clone(), 1), b);
                curves.push(id);
            } else {
                // Through its points; one that ends where it starts is
                // closed.
                let closed = n >= 4 && norm(sub(pts[0], pts[n - 1])) < 1e-9;
                let count = if closed { n - 1 } else { n };
                let ids: Vec<String> = (0..count)
                    .map(|i| slot_point(&mut e, &mut slots, &mut parent, slot(i), fixed))
                    .collect();
                for (i, p) in ids.iter().enumerate() {
                    points_of.insert((w.id.clone(), i), p.clone());
                }
                if closed {
                    points_of.insert((w.id.clone(), n - 1), ids[0].clone());
                }
                let id = e.id('c');
                let mut c = json!({"id": id, "type": "fitted_spline", "points": ids});
                if closed {
                    c["closed"] = json!(true);
                }
                e.curves.push(c);
                curves.push(id);
            }
            out_wires.push(WireMade {
                id: w.id.clone(),
                linked: false,
                pts: pts.clone(),
                curves,
            });
        }
        if made
            .iter()
            .any(|(w, pts)| w.link.is_none() && w.arc.is_none() && pts.len() > 2)
        {
            notes.push(
                "a fit spline runs through its points as MitCad fits them (the file's spline is a Catmull-Rom curve through them)"
                    .to_owned(),
            );
        }

        // Relations and dimensions.
        let space_of: HashMap<String, WireSpace> =
            made.iter().map(|(w, _)| (w.id.clone(), w.space)).collect();
        let positions: HashMap<(String, usize), [f64; 3]> = slot_of
            .iter()
            .map(|(k, &i)| (k.clone(), slots[i].at))
            .collect();
        let ctx = Ctx {
            curve_of: out_wires
                .iter()
                .filter(|w| w.curves.len() == 1)
                .map(|w| (w.id.clone(), w.curves[0].clone()))
                .collect(),
            arcs: made
                .iter()
                .filter(|(w, pts)| w.arc.is_some() && pts.len() == 3)
                .map(|(w, _)| w.id.clone())
                .collect(),
            positions,
            space_of,
            inverse,
            identity,
            turned,
        };
        let (mut relations, mut dimensions, mut relations_mapped, mut dimensions_mapped) =
            (0, 0, 0, 0);
        for c in &constraints {
            let is_dim = c.kind.value().is_some();
            if is_dim {
                dimensions += 1;
            } else {
                relations += 1;
            }
            match self.spatial_constraint(c, &mut e, &ctx, &mut points_of, &mut |e, key| {
                let i = *slot_of
                    .get(key)
                    .ok_or_else(|| format!("{} has no point {}", key.0, key.1))?;
                Ok(slot_point(e, &mut slots, &mut parent, i, false))
            }) {
                Ok(()) if is_dim => dimensions_mapped += 1,
                Ok(()) => relations_mapped += 1,
                Err(why) => dropped.push(format!("{} ({}): {why}", c.id, c.kind.name())),
            }
        }

        let def = json!({"type": "spatial_sketch", "support": "xy",
            "points": e.points, "curves": e.curves,
            "constraints": e.constraints, "dimensions": e.dimensions});
        // Every point fixed, the arcs' normals held, the origin point on
        // the origin; numbered again.
        let mut bare_relations: Vec<Value> = e.fixes.clone();
        bare_relations.extend(
            e.constraints
                .iter()
                .filter(|r| matches!(r["type"].as_str(), Some("normal_along" | "attach")))
                .cloned(),
        );
        for (k, r) in bare_relations.iter_mut().enumerate() {
            r["id"] = json!(format!("k{}", k + 1));
        }
        let bare = json!({"type": "spatial_sketch", "support": "xy",
            "points": e.points, "curves": e.curves, "constraints": bare_relations});
        Ok(Translated {
            def,
            bare,
            wires: out_wires,
            points: points_of,
            dropped,
            notes,
            relations,
            dimensions,
            relations_mapped,
            dimensions_mapped,
        })
    }

    /// A dimension's value: the file's parameter (`d3 = expression`) or
    /// its number; through `g` where MitCad's value is `g` of the file's.
    fn spatial_value(
        &mut self,
        c: &WireConstraint,
        angle: bool,
        g: Option<&dyn Fn(&str) -> String>,
    ) -> Result<Option<String>, String> {
        if c.driven {
            return Ok(None);
        }
        let v = c.kind.value().unwrap_or(0.0);
        let expr = c
            .param
            .as_ref()
            .and_then(|n| self.design.params.iter().find(|p| &p.name == n))
            .map(|p| p.expr.clone())
            .unwrap_or_else(|| {
                if angle {
                    format!("{} deg", v.to_degrees())
                } else {
                    format!("{v} mm")
                }
            });
        Ok(Some(match g {
            Some(g) => self
                .params
                .slot_of(self.doc, c.param.as_deref(), &expr, g)?,
            None => self.params.slot(self.doc, c.param.as_deref(), &expr)?,
        }))
    }

    /// One relation or dimension of the file as the 3D sketch's.
    fn spatial_constraint(
        &mut self,
        c: &WireConstraint,
        e: &mut Emit,
        ctx: &Ctx,
        points_of: &mut HashMap<(String, usize), String>,
        point: &mut PointOf<'_>,
    ) -> Result<(), String> {
        use WireConstraintKind as W;
        // A fit point's MitCad point; an arc's through point is made on
        // the arc when a relation names it.
        let mut pt = |e: &mut Emit, ctx: &Ctx, p: &WirePoint| -> Result<String, String> {
            let k = key(p);
            if let Some(id) = points_of.get(&k) {
                return Ok(id.clone());
            }
            if ctx.arcs.contains(&p.wire) && p.index == 1 {
                let id = point(e, &k)?;
                let arc = ctx.curve(&p.wire)?;
                e.relation(json!({"type": "point_on", "point": id, "curve": arc}));
                points_of.insert(k, id.clone());
                return Ok(id);
            }
            let id = point(e, &k)?;
            points_of.insert(k, id.clone());
            Ok(id)
        };
        let dim = |e: &mut Emit, mut d: Value, value: Option<String>| {
            d["id"] = json!(e.k());
            if let Some(v) = value {
                d["value"] = json!(v);
            }
            e.dimensions.push(d);
        };
        match &c.kind {
            W::Coincident { a, b } => {
                let (pa, pb) = (pt(e, ctx, a)?, pt(e, ctx, b)?);
                if pa != pb {
                    e.relation(json!({"type": "coincident", "a": pa, "b": pb}));
                }
            }
            W::PointOnLine { p, line } => {
                let id = pt(e, ctx, p)?;
                let start = pt(
                    e,
                    ctx,
                    &WirePoint {
                        wire: line.clone(),
                        index: 0,
                    },
                )?;
                if start != id {
                    self.point_on_line(e, ctx, &id, ctx.at(p)?, line, &start)?;
                }
            }
            W::Fix { p, at, exprs } => {
                let id = pt(e, ctx, p)?;
                let space = ctx.space(&p.wire);
                let at_local = ctx.local(space, *at);
                match exprs {
                    Some(x) if [&x.x, &x.y, &x.z].iter().any(|v| v.is_some()) => {
                        if space == WireSpace::World && !ctx.identity {
                            return Err("its coordinates follow parameters in world coordinates, and the component is placed elsewhere".to_owned());
                        }
                        let o = e.origin();
                        for (i, (axis, ex)) in
                            ["x", "y", "z"].iter().zip([&x.x, &x.y, &x.z]).enumerate()
                        {
                            let value = match ex {
                                Some(text) => self.params.expression(self.doc, text)?,
                                None => format!("{} mm", at_local[i]),
                            };
                            let value = self.params.unnamed(self.doc, &value);
                            dim(
                                e,
                                json!({"type": "offset", "a": o, "b": id, "axis": axis}),
                                Some(value),
                            );
                        }
                    }
                    _ => e.relation(json!({"type": "fix", "point": id, "at": at_local})),
                }
            }
            W::AlongAxis { line, axis } => {
                let i = axis_index(axis).ok_or("an axis it does not name")?;
                ctx.axes(line)?;
                let (a, b) = ctx.line_ends(line)?;
                let sense = if sub(b, a)[i] >= 0.0 {
                    "positive"
                } else {
                    "negative"
                };
                e.relation(json!({"type": "align", "line": ctx.curve(line)?, "axis": axis.to_ascii_lowercase(), "sense": sense}));
            }
            W::Distance { a, b, .. } => {
                let (pa, pb) = (pt(e, ctx, a)?, pt(e, ctx, b)?);
                if pa == pb {
                    return Err("between one point".to_owned());
                }
                // A line's length, else that of a construction line between
                // the points.
                let line = (a.wire == b.wire && ctx.is_line(&a.wire) && a.index != b.index)
                    .then(|| ctx.curve(&a.wire))
                    .transpose()?;
                let line = match line {
                    Some(l) => l,
                    None => {
                        let l = e.id('c');
                        e.curves.push(json!({"id": l, "type": "line", "start": pa, "end": pb, "construction": true}));
                        l
                    }
                };
                let value = self.spatial_value(c, false, None)?;
                dim(e, json!({"type": "length", "line": line}), value);
            }
            W::AxisDistance {
                a,
                b,
                axis,
                reversed,
                ..
            } => {
                axis_index(axis).ok_or("an axis it does not name")?;
                ctx.axes(&a.wire)?;
                let (pa, pb) = (pt(e, ctx, a)?, pt(e, ctx, b)?);
                let neg = |n: &str| format!("-({n})");
                let value = self.spatial_value(
                    c,
                    false,
                    reversed.then_some(&neg as &dyn Fn(&str) -> String),
                )?;
                dim(
                    e,
                    json!({"type": "offset", "a": pa, "b": pb, "axis": axis.to_ascii_lowercase()}),
                    value,
                );
            }
            W::Angle {
                a,
                b,
                a_reversed,
                b_reversed,
                ..
            } => {
                let (ca, cb) = (ctx.curve(a)?, ctx.curve(b)?);
                let other = |n: &str| format!("180 deg - {n}");
                let g = (a_reversed != b_reversed).then_some(&other as &dyn Fn(&str) -> String);
                let value = self.spatial_value(c, true, g)?;
                dim(e, json!({"type": "angle", "a": ca, "b": cb}), value);
            }
            W::Radius { arc, .. } => {
                let curve = ctx.curve(arc)?;
                let value = self.spatial_value(c, false, None)?;
                dim(e, json!({"type": "radius", "curve": curve}), value);
            }
            W::Parallel { a, b } => {
                e.relation(json!({"type": "parallel", "a": ctx.curve(a)?, "b": ctx.curve(b)?}));
            }
            W::Perpendicular { a, b } => {
                e.relation(
                    json!({"type": "perpendicular", "a": ctx.curve(a)?, "b": ctx.curve(b)?}),
                );
            }
            W::Collinear { a, b } => {
                // Parallel, and b's start on a's line.
                let (ca, cb) = (ctx.curve(a)?, ctx.curve(b)?);
                let start = WirePoint {
                    wire: b.clone(),
                    index: 0,
                };
                let id = pt(e, ctx, &start)?;
                let a_start = pt(
                    e,
                    ctx,
                    &WirePoint {
                        wire: a.clone(),
                        index: 0,
                    },
                )?;
                e.relation(json!({"type": "parallel", "a": ca, "b": cb}));
                if a_start != id {
                    self.point_on_line(e, ctx, &id, ctx.at(&start)?, a, &a_start)?;
                }
            }
            W::Equal { a, b, .. } => {
                e.relation(json!({"type": "equal", "a": ctx.curve(a)?, "b": ctx.curve(b)?}));
            }
            W::Tangent {
                line, arc, arc_end, ..
            } => {
                let at = pt(
                    e,
                    ctx,
                    &WirePoint {
                        wire: arc.clone(),
                        index: *arc_end,
                    },
                )?;
                e.relation(json!({"type": "tangent", "a": ctx.curve(line)?, "b": ctx.curve(arc)?, "at": at}));
            }
            W::OnPlane { p, plane } => {
                let axis = plane_axis(plane).ok_or("a plane it does not name")?;
                ctx.planes(&p.wire)?;
                let id = pt(e, ctx, p)?;
                e.relation(json!({"type": "on_plane", "points": [id], "plane": {"frame": axis}}));
            }
            W::PlaneDistance {
                p, plane, reversed, ..
            } => {
                let axis = plane_axis(plane).ok_or("a plane it does not name")?;
                ctx.planes(&p.wire)?;
                let id = pt(e, ctx, p)?;
                let o = e.origin();
                let neg = |n: &str| format!("-({n})");
                let value = self.spatial_value(
                    c,
                    false,
                    reversed.then_some(&neg as &dyn Fn(&str) -> String),
                )?;
                dim(
                    e,
                    json!({"type": "offset", "a": o, "b": id, "axis": axis}),
                    value,
                );
            }
            W::ParallelToPlane { .. } | W::PlaneAngle { .. } => {
                return Err("MitCad's 3D sketches relate lines to frame axes and to each other, not to a plane's direction (the curves stay where the file solved them)".to_owned());
            }
            W::Other => return Err("a kind this import does not know".to_owned()),
        }
        Ok(())
    }

    /// A point on the infinite line through a line: on the line where it
    /// lies between the ends, else at the end of a construction line from
    /// the line's start held parallel to it.
    fn point_on_line(
        &mut self,
        e: &mut Emit,
        ctx: &Ctx,
        point: &str,
        at: [f64; 3],
        line: &str,
        start: &str,
    ) -> Result<(), String> {
        let curve = ctx.curve(line)?;
        let (a, b) = ctx.line_ends(line)?;
        let d = sub(b, a);
        let t = dot(sub(at, a), d) / dot(d, d).max(1e-300);
        if (-1e-9..=1.0 + 1e-9).contains(&t) {
            e.relation(json!({"type": "point_on", "point": point, "curve": curve}));
            return Ok(());
        }
        let l = e.id('c');
        e.curves.push(
            json!({"id": l, "type": "line", "start": start, "end": point, "construction": true}),
        );
        e.relation(json!({"type": "parallel", "a": curve, "b": l}));
        Ok(())
    }
}

/// A fit point's MitCad point, made when first named.
type PointOf<'a> = dyn FnMut(&mut Emit, &(String, usize)) -> Result<String, String> + 'a;

/// What the relations need of the curves made.
struct Ctx {
    curve_of: BTreeMap<String, String>,
    arcs: Vec<String>,
    positions: HashMap<(String, usize), [f64; 3]>,
    space_of: HashMap<String, WireSpace>,
    /// World coordinates into the component's.
    inverse: Mat,
    /// The component's placement moves nothing.
    identity: bool,
    /// It turns (world axes are not the component's).
    turned: bool,
}

impl Ctx {
    fn curve(&self, wire: &str) -> Result<String, String> {
        self.curve_of
            .get(wire)
            .cloned()
            .ok_or_else(|| format!("the 3D curve {wire} did not come in"))
    }

    fn is_line(&self, wire: &str) -> bool {
        self.positions.contains_key(&(wire.to_owned(), 1))
            && !self.positions.contains_key(&(wire.to_owned(), 2))
            && !self.arcs.iter().any(|a| a == wire)
    }

    fn line_ends(&self, wire: &str) -> Result<([f64; 3], [f64; 3]), String> {
        if !self.is_line(wire) {
            return Err(format!("{wire} is not a drawn line"));
        }
        Ok((
            self.positions[&(wire.to_owned(), 0)],
            self.positions[&(wire.to_owned(), 1)],
        ))
    }

    fn at(&self, p: &WirePoint) -> Result<[f64; 3], String> {
        self.positions
            .get(&key(p))
            .copied()
            .ok_or_else(|| format!("{} has no point {}", p.wire, p.index))
    }

    fn space(&self, wire: &str) -> WireSpace {
        self.space_of.get(wire).copied().unwrap_or_default()
    }

    /// A point of the space in component coordinates.
    fn local(&self, space: WireSpace, p: [f64; 3]) -> [f64; 3] {
        match space {
            WireSpace::World if !self.identity => apply_point(&self.inverse, p),
            _ => p,
        }
    }

    /// Axes of the curve's space are the sketch's.
    fn axes(&self, wire: &str) -> Result<(), String> {
        if self.space(wire) == WireSpace::World && self.turned {
            return Err("it is along a world axis, and the component is turned".to_owned());
        }
        Ok(())
    }

    /// Origin planes of the curve's space are the sketch's.
    fn planes(&self, wire: &str) -> Result<(), String> {
        if self.space(wire) == WireSpace::World && !self.identity {
            return Err("it is to a world plane, and the component is placed elsewhere".to_owned());
        }
        Ok(())
    }
}

fn key(p: &WirePoint) -> (String, usize) {
    (p.wire.clone(), p.index)
}

/// One exact piece of a linked curve, in component coordinates.
#[derive(Debug, Clone)]
enum Piece {
    Line {
        start: [f64; 3],
        end: [f64; 3],
    },
    /// Counter-clockwise about `normal` from `start` to `end`.
    Arc {
        center: [f64; 3],
        normal: [f64; 3],
        start: [f64; 3],
        end: [f64; 3],
    },
    /// A clamped B-spline on [0, 1] (weights all 1 when not rational).
    Spline {
        degree: usize,
        poles: Vec<[f64; 3]>,
        weights: Vec<f64>,
        knots: Vec<f64>,
    },
}

impl Piece {
    fn start(&self) -> [f64; 3] {
        match self {
            Piece::Line { start, .. } | Piece::Arc { start, .. } => *start,
            Piece::Spline { poles, .. } => poles[0],
        }
    }

    fn end(&self) -> [f64; 3] {
        match self {
            Piece::Line { end, .. } | Piece::Arc { end, .. } => *end,
            Piece::Spline { poles, .. } => poles[poles.len() - 1],
        }
    }

    /// It ends where it starts (a full circle or ellipse, a closed spline).
    fn closed(&self) -> bool {
        matches!(self, Piece::Spline { .. }) && norm(sub(self.start(), self.end())) < 1e-9
    }

    fn mapped(self, f: &dyn Fn([f64; 3]) -> [f64; 3]) -> Piece {
        match self {
            Piece::Line { start, end } => Piece::Line {
                start: f(start),
                end: f(end),
            },
            Piece::Arc {
                center,
                normal,
                start,
                end,
            } => {
                let c = f(center);
                let tip = f(add(center, normal));
                Piece::Arc {
                    center: c,
                    normal: unit(sub(tip, c)).unwrap_or(normal),
                    start: f(start),
                    end: f(end),
                }
            }
            Piece::Spline {
                degree,
                poles,
                weights,
                knots,
            } => Piece::Spline {
                degree,
                poles: poles.into_iter().map(f).collect(),
                weights,
                knots,
            },
        }
    }

    /// The pieces of a record curve on `[t0, t1]` (a polyline's one per
    /// segment).
    fn of(curve: &RecordCurve, [t0, t1]: [f64; 2]) -> Result<Vec<Piece>, String> {
        if !(t0.is_finite() && t1.is_finite() && t1 > t0) {
            return Err("a piece of no length".to_owned());
        }
        Ok(vec![match curve {
            RecordCurve::Line { start, end } => {
                let d = sub(*end, *start);
                Piece::Line {
                    start: add(*start, scale(d, t0)),
                    end: add(*start, scale(d, t1)),
                }
            }
            RecordCurve::Circle { frame, radius } => {
                let (x, y) = axes(frame.x_axis, frame.y_axis)?;
                if t1 - t0 >= std::f64::consts::TAU - 1e-9 {
                    conic_spline(
                        frame.origin,
                        x,
                        y,
                        [*radius; 2],
                        t0,
                        t0 + std::f64::consts::TAU,
                    )
                } else {
                    let at = |t: f64| {
                        add(
                            frame.origin,
                            add(scale(x, radius * t.cos()), scale(y, radius * t.sin())),
                        )
                    };
                    Piece::Arc {
                        center: frame.origin,
                        normal: cross(x, y),
                        start: at(t0),
                        end: at(t1),
                    }
                }
            }
            RecordCurve::Ellipse { frame, radii } => {
                let (x, y) = axes(frame.x_axis, frame.y_axis)?;
                let t1 = t1.min(t0 + std::f64::consts::TAU);
                conic_spline(frame.origin, x, y, *radii, t0, t1)
            }
            RecordCurve::Polyline { points } => {
                // Segment i spans [i, i + 1]: a line from each corner the
                // piece passes to the next.
                let n = points.len();
                if n < 2 {
                    return Err("a polyline of one point".to_owned());
                }
                let at = |t: f64| {
                    let i = (t.floor().max(0.0) as usize).min(n - 2);
                    let f = t - i as f64;
                    add(points[i], scale(sub(points[i + 1], points[i]), f))
                };
                let (t0, t1) = (t0.max(0.0), t1.min((n - 1) as f64));
                let mut cuts = vec![t0];
                let mut k = t0.floor() + 1.0;
                while k < t1 - 1e-9 {
                    cuts.push(k);
                    k += 1.0;
                }
                cuts.push(t1);
                return Ok(cuts
                    .windows(2)
                    .filter(|c| norm(sub(at(c[1]), at(c[0]))) > 1e-9)
                    .map(|c| Piece::Line {
                        start: at(c[0]),
                        end: at(c[1]),
                    })
                    .collect());
            }
            RecordCurve::Spline { knots, points } => {
                let w = vec![1.0; points.len()];
                bspline_piece(knots, points, &w, t0, t1)?
            }
            RecordCurve::RationalSpline { knots, homogeneous } => {
                let mut pts = Vec::with_capacity(homogeneous.len());
                let mut w = Vec::with_capacity(homogeneous.len());
                for h in homogeneous {
                    if h[3].is_nan() || h[3] <= 0.0 {
                        return Err("a rational spline with a weight not above 0".to_owned());
                    }
                    pts.push([h[0] / h[3], h[1] / h[3], h[2] / h[3]]);
                    w.push(h[3]);
                }
                bspline_piece(knots, &pts, &w, t0, t1)?
            }
            RecordCurve::Other => {
                return Err("an intersection curve known here only near its leader".to_owned());
            }
        }])
    }
}

/// A frame's x and y made unit and square.
fn axes(x: [f64; 3], y: [f64; 3]) -> Result<([f64; 3], [f64; 3]), String> {
    let xu = unit(x).ok_or("a frame without an x axis")?;
    let n = unit(cross(xu, y)).ok_or("a frame whose y runs along its x")?;
    Ok((xu, cross(n, xu)))
}

/// The exact rational quadratic B-spline of `centre + a cos t x + b sin t
/// y` for t from `t0` to `t1` (a circle when the radii are equal): one
/// span of at most a quarter turn per piece.
fn conic_spline(
    center: [f64; 3],
    x: [f64; 3],
    y: [f64; 3],
    [a, b]: [f64; 2],
    t0: f64,
    t1: f64,
) -> Piece {
    let at = |t: f64| add(center, add(scale(x, a * t.cos()), scale(y, b * t.sin())));
    let spans = ((t1 - t0) / std::f64::consts::FRAC_PI_2).ceil().max(1.0) as usize;
    let step = (t1 - t0) / spans as f64;
    let w = (step / 2.0).cos();
    let mut poles = vec![at(t0)];
    let mut weights = vec![1.0];
    let mut knots = vec![0.0; 3];
    for k in 0..spans {
        let m = t0 + step * (k as f64 + 0.5);
        // The middle pole: where the tangents at the span's ends meet.
        let mid = add(
            center,
            add(scale(x, a * m.cos() / w), scale(y, b * m.sin() / w)),
        );
        poles.push(mid);
        weights.push(w);
        poles.push(at(t0 + step * (k + 1) as f64));
        weights.push(1.0);
        let u = (k + 1) as f64 / spans as f64;
        if k + 1 < spans {
            knots.extend([u, u]);
        }
    }
    knots.extend([1.0; 3]);
    Piece::Spline {
        degree: 2,
        poles,
        weights,
        knots,
    }
}

/// The piece on `[t0, t1]` of a B-spline (`weights` all 1 when not
/// rational), cut out by inserting both ends as knots of full
/// multiplicity, its knots put on [0, 1].
fn bspline_piece(
    knots: &[f64],
    points: &[[f64; 3]],
    weights: &[f64],
    t0: f64,
    t1: f64,
) -> Result<Piece, String> {
    let n = points.len();
    if n < 2 || knots.len() <= n + 1 || weights.len() != n {
        return Err("a spline whose knots and points do not agree".to_owned());
    }
    let p = knots.len() - n - 1;
    if knots.windows(2).any(|w| w[1] < w[0]) {
        return Err("a spline whose knots run backwards".to_owned());
    }
    let (lo, hi) = (knots[p], knots[n]);
    let (t0, t1) = (t0.max(lo), t1.min(hi));
    if t1 <= t0 {
        return Err("a piece outside its spline".to_owned());
    }
    // Homogeneous poles.
    let mut h: Vec<[f64; 4]> = points
        .iter()
        .zip(weights)
        .map(|(q, w)| [q[0] * w, q[1] * w, q[2] * w, *w])
        .collect();
    let mut u = knots.to_vec();
    for t in [t0, t1] {
        let have = u.iter().filter(|k| (**k - t).abs() < 1e-12).count();
        for _ in have..p + 1 {
            insert_knot(&mut u, &mut h, p, t);
        }
    }
    // The poles of [t0, t1]: those whose knots lie in it.
    let first = u
        .iter()
        .position(|k| (*k - t0).abs() < 1e-12)
        .ok_or("a knot that did not go in")?;
    let last = u
        .iter()
        .rposition(|k| (*k - t1).abs() < 1e-12)
        .ok_or("a knot that did not go in")?;
    let knots: Vec<f64> = u[first..=last]
        .iter()
        .map(|k| ((k - t0) / (t1 - t0)).clamp(0.0, 1.0))
        .collect();
    let count = knots.len() - p - 1;
    let poles_h = &h[first..first + count];
    let poles = poles_h
        .iter()
        .map(|q| [q[0] / q[3], q[1] / q[3], q[2] / q[3]])
        .collect();
    let weights = poles_h.iter().map(|q| q[3]).collect();
    Ok(Piece::Spline {
        degree: p,
        poles,
        weights,
        knots,
    })
}

/// Boehm's insertion of the knot `t` once.
fn insert_knot(u: &mut Vec<f64>, h: &mut Vec<[f64; 4]>, p: usize, t: f64) {
    // The span: u[k] <= t < u[k + 1] (the last span at the end).
    let n = h.len();
    let mut k = p;
    while k + 1 < n && u[k + 1] <= t {
        k += 1;
    }
    let mut q = Vec::with_capacity(n + 1);
    for i in 0..=n {
        if i + p <= k {
            q.push(h[i]);
        } else if i > k {
            q.push(h[i - 1]);
        } else {
            let d = u[i + p] - u[i];
            let a = if d.abs() < 1e-300 {
                0.0
            } else {
                (t - u[i]) / d
            };
            q.push(std::array::from_fn(|j| {
                (1.0 - a) * h[i - 1][j] + a * h[i][j]
            }));
        }
    }
    u.insert(k + 1, t);
    *h = q;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eval(p: &Piece, t: f64) -> [f64; 3] {
        let Piece::Spline {
            degree,
            poles,
            weights,
            knots,
        } = p
        else {
            panic!("not a spline")
        };
        // De Boor on homogeneous poles.
        let n = poles.len();
        let mut k = *degree;
        while k + 1 < n && knots[k + 1] <= t {
            k += 1;
        }
        let mut d: Vec<[f64; 4]> = (0..=*degree)
            .map(|j| {
                let q = poles[j + k - degree];
                let w = weights[j + k - degree];
                [q[0] * w, q[1] * w, q[2] * w, w]
            })
            .collect();
        for r in 1..=*degree {
            for j in (r..=*degree).rev() {
                let i = j + k - degree;
                let a = (t - knots[i]) / (knots[i + degree + 1 - r] - knots[i]);
                d[j] = std::array::from_fn(|m| (1.0 - a) * d[j - 1][m] + a * d[j][m]);
            }
        }
        let q = d[*degree];
        [q[0] / q[3], q[1] / q[3], q[2] / q[3]]
    }

    /// A quarter of an ellipse and a whole circle as exact quadratic
    /// splines: every point on the conic.
    #[test]
    fn conics_are_exact_splines() {
        let e = conic_spline(
            [1.0, 2.0, 3.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [4.0, 2.0],
            0.3,
            1.9,
        );
        for k in 0..=20 {
            let p = eval(&e, f64::from(k) / 20.0);
            let (x, y) = ((p[0] - 1.0) / 4.0, (p[1] - 2.0) / 2.0);
            assert!((x * x + y * y - 1.0).abs() < 1e-12, "{p:?}");
            assert!((p[2] - 3.0).abs() < 1e-12);
        }
        assert!(
            norm(sub(
                e.start(),
                [1.0 + 4.0 * 0.3f64.cos(), 2.0 + 2.0 * 0.3f64.sin(), 3.0]
            )) < 1e-12
        );
        let c = conic_spline(
            [0.0; 3],
            [1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [5.0; 2],
            0.0,
            std::f64::consts::TAU,
        );
        assert!(c.closed());
        for k in 0..=40 {
            let p = eval(&c, f64::from(k) / 40.0);
            assert!((norm(p) - 5.0).abs() < 1e-12 && p[1].abs() < 1e-12, "{p:?}");
        }
    }

    /// A piece of a cubic B-spline cut out by knot insertion runs on the
    /// spline itself between the two parameters.
    #[test]
    fn a_spline_piece_is_cut_out_exactly() {
        let knots = [0.0, 0.0, 0.0, 0.0, 1.0, 2.0, 3.0, 3.0, 3.0, 3.0];
        let points = [
            [0.0, 0.0, 0.0],
            [1.0, 2.0, 0.0],
            [2.0, -1.0, 1.0],
            [3.0, 1.0, 2.0],
            [4.0, 0.0, 0.0],
            [5.0, 3.0, 1.0],
        ];
        let whole = bspline_piece(&knots, &points, &[1.0; 6], 0.0, 3.0).unwrap();
        let piece = bspline_piece(&knots, &points, &[1.0; 6], 0.5, 2.25).unwrap();
        for k in 0..=10 {
            let s = f64::from(k) / 10.0;
            let t = 0.5 + s * 1.75;
            let a = eval(&piece, s);
            let b = eval(&whole, t / 3.0);
            assert!(norm(sub(a, b)) < 1e-12, "{a:?} {b:?} at {t}");
        }
    }

    #[test]
    fn straight_polylines_are_lines() {
        assert!(straight(&[[0.0; 3], [1.0, 0.0, 0.0], [2.0, 0.0005, 0.0]]).is_some());
        assert!(straight(&[[0.0; 3], [1.0, 0.1, 0.0], [2.0, 0.0, 0.0]]).is_none());
        assert!(straight(&[[0.0; 3], [2.0, 0.0, 0.0], [1.0, 0.0, 0.0]]).is_none());
    }
}
