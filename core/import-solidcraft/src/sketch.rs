// SPDX-License-Identifier: MIT
//! Sketches: the file's points, curves, constraints and dimensions as a
//! Mitcad sketch definition, and the profiles features select from it.
//!
//! - Points become `p<n>`, curves `c<n>` (one id space); the fixed sketch
//!   origin becomes a fixed point at (0, 0) when something uses it. Points
//!   of linked (projected) geometry are fixed where the file has them.
//! - Lines, circles, arcs (counter-clockwise), ellipses, splines (control
//!   points, or fit points: `fitted_spline`) and conics (a rational
//!   quadratic spline through the conic's ends with its apex as the middle
//!   pole, weight ρ / (1 − ρ): the same curve). Construction lines,
//!   centerlines and infinite lines are construction geometry.
//! - Constraints and dimensions of the same meaning; a dimension's value
//!   is its parameter (`d1 = width`); an angle from line a to line b is the
//!   sector between the rays (the second reversed with `flip`); a point
//!   placed by parameters (`exprs`) gets signed horizontal and vertical
//!   distances from the origin. Left out, with the reason in the report:
//!   pattern relations (the copies stay where they are), the angle of a
//!   line from the x axis and an arc's sweep (other than level or upright
//!   lines), a slot's overall length and offset distances.

use std::collections::HashMap;

use mitcad_model::{ComponentUid, FeatureUid, Kernel};
use mitcad_solidcraft::Feature;
use mitcad_solidcraft::kinds::{ProfileSel, SketchFeature};
use mitcad_solidcraft::sketch::{ConstraintKind, CurveKind, Sketch};
use serde_json::{Value, json};

use super::planes::Frame;
use super::report::SketchReport;
use super::{Importer, MapResult, Mapped, NotMapped};

/// A sketch made.
#[derive(Debug, Clone)]
pub(crate) struct SketchMade {
    pub uid: FeatureUid,
    /// The file's point and curve ids and the Mitcad ones.
    pub points: HashMap<String, String>,
    pub curves: HashMap<String, String>,
    /// The sketch's frame in its component's coordinates.
    pub frame: Frame,
    /// The file's sketch (for profiles by curves).
    pub sketch: Sketch,
}

impl SketchMade {
    pub fn curve(&self, id: &str) -> Result<String, String> {
        self.curves
            .get(id)
            .cloned()
            .ok_or_else(|| format!("the sketch has no curve {id}"))
    }

    pub fn point(&self, id: &str) -> Result<String, String> {
        self.points
            .get(id)
            .cloned()
            .ok_or_else(|| format!("the sketch has no point {id}"))
    }
}

/// The translated definition and what was left out.
struct Translated {
    entities: Vec<Value>,
    constraints: Vec<Value>,
    dimensions: Vec<Value>,
    points: HashMap<String, String>,
    curves: HashMap<String, String>,
    dropped: Vec<String>,
    constraints_mapped: usize,
    dimensions_mapped: usize,
}

impl<K: Kernel> Importer<'_, '_, K> {
    pub(super) fn sketch(&mut self, f: &Feature, component: ComponentUid) -> MapResult {
        let s: SketchFeature = f.typed()?;
        let mut names = f.param_names.iter().map(String::as_str);
        let plane = self.plane_for_sketch(&s.plane, &mut names, component)?;
        let t = self.translate_sketch(&s.sketch)?;
        let mut def = json!({
            "type": "sketch",
            "plane": plane.geom,
            "entities": t.entities,
            "constraints": t.constraints,
            "dimensions": t.dimensions,
        });
        if let Some(frame) = &plane.frame {
            def["frame"] = json!({"origin": frame.0, "x_axis": frame.1, "y_axis": frame.2});
        }
        let uid = match self.add(&def, Some(&f.name), component) {
            Ok(uid) => uid,
            Err(e) => {
                self.params.forget_pending();
                // A sketch the solver refuses with its constraints comes in
                // without them, its geometry where the file has it.
                let mut bare = def.clone();
                bare["constraints"] = json!([]);
                bare["dimensions"] = json!([]);
                let uid = self.add(&bare, Some(&f.name), component).map_err(|e2| {
                    NotMapped::Failed(format!("{e}; without its constraints: {e2}"))
                })?;
                self.report.sketches.push(SketchReport {
                    id: f.id,
                    name: f.name.clone(),
                    points: s.sketch.points.len(),
                    curves: s.sketch.curves.len(),
                    constraints: s.sketch.counts().0,
                    dimensions: s.sketch.counts().1,
                    constraints_mapped: 0,
                    dimensions_mapped: 0,
                    dropped: vec![format!("every constraint: {e}")],
                });
                self.record_sketch(f, uid, component, &s, t.points, t.curves, plane.world);
                return Ok(Mapped::partial(vec![format!(
                    "the sketch came in without its constraints and dimensions: {e}"
                )]));
            }
        };
        let (constraints, dimensions) = s.sketch.counts();
        let mut notes = plane.notes;
        if !t.dropped.is_empty() {
            notes.push(format!("left out of the sketch: {}", t.dropped.join(", ")));
        }
        self.report.sketches.push(SketchReport {
            id: f.id,
            name: f.name.clone(),
            points: s.sketch.points.len(),
            curves: s.sketch.curves.len(),
            constraints,
            dimensions,
            constraints_mapped: t.constraints_mapped,
            dimensions_mapped: t.dimensions_mapped,
            dropped: t.dropped,
        });
        self.record_sketch(f, uid, component, &s, t.points, t.curves, plane.world);
        Ok(Mapped::partial(notes))
    }

    #[allow(clippy::too_many_arguments)]
    fn record_sketch(
        &mut self,
        f: &Feature,
        uid: FeatureUid,
        _component: ComponentUid,
        s: &SketchFeature,
        points: HashMap<String, String>,
        curves: HashMap<String, String>,
        world: Option<Frame>,
    ) {
        let frame = self
            .doc
            .sketch_output(uid)
            .map(|o| Frame {
                o: o.frame.origin,
                x: o.frame.x_axis,
                y: o.frame.y_axis,
            })
            .or(world)
            .unwrap_or(Frame::XY);
        self.sketches.insert(
            f.id,
            SketchMade {
                uid,
                points,
                curves,
                frame,
                sketch: s.sketch.clone(),
            },
        );
    }

    fn translate_sketch(&mut self, s: &Sketch) -> Result<Translated, NotMapped> {
        let mut t = Translated {
            entities: Vec::new(),
            constraints: Vec::new(),
            dimensions: Vec::new(),
            points: HashMap::new(),
            curves: HashMap::new(),
            dropped: Vec::new(),
            constraints_mapped: 0,
            dimensions_mapped: 0,
        };
        // Which points something uses (the origin only then).
        let mut used = vec![false; s.points.len()];
        for c in &s.curves {
            for p in point_ids(&c.kind) {
                if let Some(u) = used.get_mut(p) {
                    *u = true;
                }
            }
        }
        for c in &s.constraints {
            for p in constraint_points(&c.kind) {
                if let Some(u) = used.get_mut(p) {
                    *u = true;
                }
            }
        }
        let mut next = 1usize;
        let mut point_ids_of: Vec<Option<String>> = vec![None; s.points.len()];
        let mut fixed_by_constraint = vec![false; s.points.len()];
        for c in &s.constraints {
            if let ConstraintKind::Fix { p } = c.kind
                && let Some(f) = fixed_by_constraint.get_mut(p)
            {
                *f = true;
            }
        }
        let mut origin: Option<String> = None;
        for (i, p) in s.points.iter().enumerate() {
            if i == 0 && p.id == "origin" && !used[0] && !s.points.iter().any(|q| q.exprs.is_some())
            {
                continue;
            }
            let id = format!("p{next}");
            next += 1;
            let fixed = p.fixed || p.link.is_some() || fixed_by_constraint[i];
            let mut e = json!({"id": id, "type": "point", "at": p.pos});
            if fixed {
                e["fixed"] = json!(true);
            }
            t.entities.push(e);
            if i == 0 && p.id == "origin" {
                origin = Some(id.clone());
            }
            t.points.insert(p.id.clone(), id.clone());
            point_ids_of[i] = Some(id);
        }
        let pid =
            |i: usize| -> Result<String, String> {
                point_ids_of.get(i).cloned().flatten().ok_or_else(|| {
                    format!("a curve uses point {i}, which the sketch does not have")
                })
            };
        let mut curve_ids_of: Vec<Option<String>> = vec![None; s.curves.len()];
        for (i, c) in s.curves.iter().enumerate() {
            let id = format!("c{next}");
            next += 1;
            let mut e = match &c.kind {
                CurveKind::Line { a, b } => {
                    json!({"id": id, "type": "line", "start": pid(*a)?, "end": pid(*b)?})
                }
                CurveKind::Circle { c: center, r } => {
                    json!({"id": id, "type": "circle", "center": pid(*center)?, "radius": r})
                }
                CurveKind::Arc { c: center, a, b } => {
                    json!({"id": id, "type": "arc", "center": pid(*center)?,
                    "start": pid(*a)?, "end": pid(*b)?})
                }
                CurveKind::Ellipse { c: center, m, r } => json!({"id": id, "type": "ellipse",
                    "center": pid(*center)?, "major": pid(*m)?, "minor_radius": r}),
                CurveKind::Spline {
                    pts,
                    control,
                    degree,
                    periodic,
                    nurbs,
                } => {
                    let ids: Vec<String> = pts.iter().map(|p| pid(*p)).collect::<Result<_, _>>()?;
                    if *periodic {
                        t.dropped
                            .push(format!("{}: periodic (taken as an open spline)", c.id));
                    }
                    if *control {
                        let mut e =
                            json!({"id": id, "type": "spline", "degree": degree, "control": ids});
                        // Its own knots and weights (format 49), as MitCad's
                        // spline takes them.
                        if let Some(n) = nurbs {
                            e["knots"] = json!(n.knots);
                            if !n.weights.is_empty() {
                                e["weights"] = json!(n.weights);
                            }
                        }
                        e
                    } else {
                        json!({"id": id, "type": "fitted_spline", "points": ids})
                    }
                }
                CurveKind::Conic { a, b, apex, rho } => {
                    let w = (rho / (1.0 - rho)).clamp(1e-6, 1e6);
                    json!({"id": id, "type": "spline", "degree": 2,
                        "control": [pid(*a)?, pid(*apex)?, pid(*b)?], "weights": [1.0, w, 1.0]})
                }
            };
            if c.centerline {
                e["centerline"] = json!(true);
            } else if c.is_construction() {
                e["construction"] = json!(true);
            }
            if c.fixed || c.link.is_some() {
                e["fixed"] = json!(true);
            }
            t.entities.push(e);
            t.curves.insert(c.id.clone(), id.clone());
            curve_ids_of[i] = Some(id);
        }
        let cid = |i: usize| -> Result<String, String> {
            curve_ids_of.get(i).cloned().flatten().ok_or_else(|| {
                format!("a constraint uses curve {i}, which the sketch does not have")
            })
        };
        let mut next_k = 1usize;
        for c in &s.constraints {
            let k = format!("k{next_k}");
            next_k += 1;
            use ConstraintKind as C;
            let constraint = match &c.kind {
                C::Coincident { p, q } => {
                    Some(json!({"type": "coincident", "point": pid(*p)?, "entity": pid(*q)?}))
                }
                C::PointOnCurve { p, c } => {
                    Some(json!({"type": "coincident", "point": pid(*p)?, "entity": cid(*c)?}))
                }
                C::Horizontal { l } => Some(json!({"type": "horizontal", "line": cid(*l)?})),
                C::Vertical { l } => Some(json!({"type": "vertical", "line": cid(*l)?})),
                C::HorizontalPoints { p, q } => {
                    Some(json!({"type": "horizontal_points", "a": pid(*p)?, "b": pid(*q)?}))
                }
                C::VerticalPoints { p, q } => {
                    Some(json!({"type": "vertical_points", "a": pid(*p)?, "b": pid(*q)?}))
                }
                C::Parallel { a, b } => {
                    Some(json!({"type": "parallel", "a": cid(*a)?, "b": cid(*b)?}))
                }
                C::Perpendicular { a, b } => {
                    Some(json!({"type": "perpendicular", "a": cid(*a)?, "b": cid(*b)?}))
                }
                C::Collinear { a, b } => {
                    Some(json!({"type": "collinear", "a": cid(*a)?, "b": cid(*b)?}))
                }
                C::Tangent { a, b } => {
                    Some(json!({"type": "tangent", "a": cid(*a)?, "b": cid(*b)?}))
                }
                C::Equal { a, b } => Some(json!({"type": "equal", "a": cid(*a)?, "b": cid(*b)?})),
                C::Concentric { a, b } => {
                    Some(json!({"type": "concentric", "a": cid(*a)?, "b": cid(*b)?}))
                }
                C::Smooth { a, b } => Some(json!({"type": "smooth", "a": cid(*a)?, "b": cid(*b)?})),
                C::Midpoint { p, l } => {
                    Some(json!({"type": "midpoint", "point": pid(*p)?, "curve": cid(*l)?}))
                }
                C::Symmetric { p, q, l } => Some(
                    json!({"type": "symmetric", "a": pid(*p)?, "b": pid(*q)?, "axis": cid(*l)?}),
                ),
                C::Fix { .. } => {
                    // The point is fixed where the file has it.
                    t.constraints_mapped += 1;
                    continue;
                }
                _ => None,
            };
            if let Some(mut con) = constraint {
                con["id"] = json!(k);
                t.constraints.push(con);
                t.constraints_mapped += 1;
                continue;
            }
            // Dimensions.
            let dim = match &c.kind {
                C::Distance { p, q, .. } => {
                    Some(json!({"type": "distance", "a": pid(*p)?, "b": pid(*q)?}))
                }
                C::DistanceX { p, q, .. } => {
                    Some(json!({"type": "horizontal_distance", "a": pid(*p)?, "b": pid(*q)?}))
                }
                C::DistanceY { p, q, .. } => {
                    Some(json!({"type": "vertical_distance", "a": pid(*p)?, "b": pid(*q)?}))
                }
                C::PointLineDistance { p, l, .. } => Some(
                    json!({"type": "point_line_distance", "point": pid(*p)?, "line": cid(*l)?}),
                ),
                C::Length { l, .. } => Some(json!({"type": "length", "line": cid(*l)?})),
                C::Radius { c, .. } => Some(json!({"type": "radius", "curve": cid(*c)?})),
                C::Diameter { c, .. } => Some(json!({"type": "diameter", "curve": cid(*c)?})),
                C::ArcLength { c, .. } => Some(json!({"type": "arc_length", "arc": cid(*c)?})),
                C::LinearDiameter { p, l, .. } => {
                    Some(json!({"type": "linear_diameter", "axis": cid(*l)?, "entity": pid(*p)?}))
                }
                C::Angle { a, b, value, flip } => {
                    let mut d = json!({"type": "angle", "a": cid(*a)?, "b": cid(*b)?});
                    if *flip {
                        d["reverse_b"] = json!(true);
                    }
                    if *value < 0.0 {
                        // The sector's angle: the size of the signed one.
                        d["negative"] = json!(true);
                    }
                    Some(d)
                }
                C::LineAngle { l, value } if !c.driven => {
                    // Level or upright lines only: Mitcad dimensions no line
                    // against the sketch's axes.
                    let turns = value / std::f64::consts::FRAC_PI_2;
                    if (turns - turns.round()).abs() < 1e-9
                        && c.param.as_deref().is_none_or(|n| {
                            self.design
                                .params
                                .iter()
                                .find(|p| p.name == n)
                                .is_none_or(|p| {
                                    mitcad_solidcraft::expr::literal(
                                        &p.expr,
                                        std::f64::consts::PI / 180.0,
                                    )
                                    .is_some()
                                })
                        })
                    {
                        let kind = if turns.round() as i64 % 2 == 0 {
                            "horizontal"
                        } else {
                            "vertical"
                        };
                        t.constraints
                            .push(json!({"id": k, "type": kind, "line": cid(*l)?}));
                        t.constraints_mapped += 1;
                        continue;
                    }
                    None
                }
                _ => None,
            };
            let Some(mut d) = dim else {
                t.dropped.push(format!("{} ({})", c.id, c.kind.name()));
                continue;
            };
            let negative = d.get("negative").is_some();
            if let Some(o) = d.as_object_mut() {
                o.remove("negative");
            }
            d["id"] = json!(k);
            if c.driven {
                d["driven"] = json!(true);
                if let Some(n) = &c.param {
                    d["reference"] = json!(self.params.mitcad_name(n));
                }
            } else {
                let expr = c
                    .param
                    .as_ref()
                    .and_then(|n| self.design.params.iter().find(|p| &p.name == n))
                    .map(|p| p.expr.clone())
                    .unwrap_or_else(|| {
                        let v = c.kind.value().unwrap_or(0.0);
                        if c.kind.is_angle() {
                            format!("{} deg", v.to_degrees())
                        } else {
                            format!("{v}")
                        }
                    });
                let value = if negative {
                    self.params
                        .slot_of(self.doc, c.param.as_deref(), &expr, |n| format!("abs({n})"))?
                } else {
                    self.params.slot(self.doc, c.param.as_deref(), &expr)?
                };
                d["value"] = json!(value);
            }
            t.dimensions.push(d);
            t.dimensions_mapped += 1;
        }
        // Points placed by parameters: signed distances from the origin.
        for p in &s.points {
            let Some(exprs) = &p.exprs else { continue };
            let (Some(o), Ok(id)) = (origin.clone(), t.points.get(&p.id).cloned().ok_or(())) else {
                continue;
            };
            for (kind, e) in [
                ("horizontal_distance", &exprs.x),
                ("vertical_distance", &exprs.y),
            ] {
                let Some(e) = e else { continue };
                let value = self.params.expression(self.doc, e)?;
                let k = format!("k{next_k}");
                next_k += 1;
                t.dimensions.push(
                    json!({"id": k, "type": kind, "a": o, "b": id, "signed": true, "value": value}),
                );
            }
            // The point moves with them.
            if let Some(e) = t.entities.iter_mut().find(|e| e["id"] == json!(id))
                && let Some(o) = e.as_object_mut()
            {
                o.remove("fixed");
            }
        }
        for (what, n) in [
            ("sketch pattern", s.patterns.len()),
            ("offset", s.offsets.len() + s.freeform_offsets.len()),
            ("3D curve", s.wires.len()),
        ] {
            if n > 0 {
                t.dropped
                    .push(format!("{n} {what}{}", if n > 1 { "s" } else { "" }));
            }
        }
        if !s.equations.is_empty() {
            t.dropped.push(format!(
                "the formulas of {} equation curve{} (each stays the spline it placed)",
                s.equations.len(),
                if s.equations.len() > 1 { "s" } else { "" }
            ));
        }
        for l in &s.links {
            let kind = l.get("type").and_then(Value::as_str).unwrap_or("link");
            let what = match kind {
                "text" => "a text",
                _ => "a link to geometry (its curves are fixed where they are)",
            };
            t.dropped.push(what.to_owned());
        }
        Ok(t)
    }

    /// The profile regions a selection means, as Mitcad profile references.
    pub(super) fn profiles(&self, sketch: u64, sel: &ProfileSel) -> Result<Vec<Value>, String> {
        let made = self
            .sketches
            .get(&sketch)
            .ok_or_else(|| format!("sketch {sketch} did not come in"))?;
        let output = self
            .doc
            .sketch_output(made.uid)
            .ok_or_else(|| "the sketch does not evaluate".to_owned())?;
        let reference = |key: String| json!({"sketch": made.uid.to_string(), "region": key});
        let all: Vec<&mitcad_model::sketch::regions::Region> = output.region_info.iter().collect();
        if all.is_empty() {
            return Err("the sketch has no closed profile".to_owned());
        }
        let outer_curves = |r: &mitcad_model::sketch::regions::Region| -> Vec<String> {
            let mut c: Vec<String> = r
                .profile
                .key
                .segments()
                .map(|s| s.curve.to_string())
                .collect();
            c.sort();
            c.dedup();
            c
        };
        let picked: Vec<String> = match sel {
            ProfileSel::All => all.iter().map(|r| r.profile.key.to_string()).collect(),
            ProfileSel::Indices { indices } => {
                let mut regions: Vec<&&mitcad_model::sketch::regions::Region> =
                    all.iter().collect();
                // SolidCraft's profile list: larger outer loops first is
                // its usual order; the selection is checked by the result.
                regions.sort_by(|a, b| b.area.total_cmp(&a.area));
                indices
                    .iter()
                    .map(|i| {
                        regions
                            .get(*i)
                            .map(|r| r.profile.key.to_string())
                            .ok_or_else(|| format!("the sketch has no profile {i}"))
                    })
                    .collect::<Result<_, _>>()?
            }
            ProfileSel::Curves { loops } => loops
                .iter()
                .map(|l| {
                    let mut want: Vec<String> =
                        l.iter().map(|c| made.curve(c)).collect::<Result<_, _>>()?;
                    want.sort();
                    want.dedup();
                    let found: Vec<_> = all.iter().filter(|r| outer_curves(r) == want).collect();
                    match found.as_slice() {
                        [r] => Ok(r.profile.key.to_string()),
                        [] => Err(format!("no profile is bounded by the curves {l:?}")),
                        _ => Err(format!("several profiles are bounded by the curves {l:?}")),
                    }
                })
                .collect::<Result<_, _>>()?,
            ProfileSel::WholeBoundaries { profiles } => profiles
                .iter()
                .map(|p| {
                    let mut want: Vec<String> = p
                        .outer
                        .iter()
                        .map(|c| made.curve(c))
                        .collect::<Result<_, _>>()?;
                    want.sort();
                    want.dedup();
                    all.iter()
                        .find(|r| outer_curves(r) == want)
                        .map(|r| r.profile.key.to_string())
                        .ok_or_else(|| format!("no profile is bounded by the curves {:?}", p.outer))
                })
                .collect::<Result<_, _>>()?,
            ProfileSel::Points { points } => points
                .iter()
                .map(|p| {
                    all.iter()
                        .filter(|r| {
                            inside(&r.outline, *p) && !r.holes.iter().any(|h| inside(h, *p))
                        })
                        .min_by(|a, b| a.area.total_cmp(&b.area))
                        .map(|r| r.profile.key.to_string())
                        .ok_or_else(|| format!("no profile holds the point {p:?}"))
                })
                .collect::<Result<_, _>>()?,
            ProfileSel::OffsetBoundaries { .. } => {
                return Err("profiles of offset boundaries are not translated".to_owned());
            }
        };
        let mut seen = Vec::new();
        Ok(picked
            .into_iter()
            .filter(|k| {
                let new = !seen.contains(k);
                seen.push(k.clone());
                new
            })
            .map(reference)
            .collect())
    }
}

fn point_ids(c: &CurveKind) -> Vec<usize> {
    match c {
        CurveKind::Line { a, b } => vec![*a, *b],
        CurveKind::Circle { c, .. } => vec![*c],
        CurveKind::Arc { c, a, b } => vec![*c, *a, *b],
        CurveKind::Ellipse { c, m, .. } => vec![*c, *m],
        CurveKind::Spline { pts, .. } => pts.clone(),
        CurveKind::Conic { a, b, apex, .. } => vec![*a, *b, *apex],
    }
}

fn constraint_points(c: &ConstraintKind) -> Vec<usize> {
    use ConstraintKind as C;
    match *c {
        C::PatternPoint { p, q }
        | C::Coincident { p, q }
        | C::HorizontalPoints { p, q }
        | C::VerticalPoints { p, q } => {
            vec![p, q]
        }
        C::Distance { p, q, .. } | C::DistanceX { p, q, .. } | C::DistanceY { p, q, .. } => {
            vec![p, q]
        }
        C::Symmetric { p, q, .. } => vec![p, q],
        C::PointOnCurve { p, .. }
        | C::Midpoint { p, .. }
        | C::Fix { p }
        | C::PointLineDistance { p, .. }
        | C::PointCircleDistance { p, .. }
        | C::LinearDiameter { p, .. } => vec![p],
        _ => Vec::new(),
    }
}

/// Whether a point lies inside a closed polygon (even-odd).
fn inside(poly: &[[f64; 2]], p: [f64; 2]) -> bool {
    let mut c = false;
    let n = poly.len();
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + n - 1) % n]);
        if (a[1] > p[1]) != (b[1] > p[1])
            && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0]
        {
            c = !c;
        }
    }
    c
}
