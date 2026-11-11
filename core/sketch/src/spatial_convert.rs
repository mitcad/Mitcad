// SPDX-License-Identifier: MIT
//! Turning a planar sketch into a 3D sketch in place (mitcad#1544): the
//! definition the 3D sketch gets. The document
//! (`mitcad_model::Document::convert_sketch_to_spatial`) puts it in the
//! planar sketch's place, rewrites the features that use the sketch's
//! curves as a path and takes the whole change as one undo step, or
//! nothing.
//!
//! What is kept: the support and the sketch's own frame (its normal is the
//! 3D sketch's local +Z), every point, curve, text, constraint and
//! dimension id, construction flags, the driving dimensions' parameters
//! (with their expressions), driven dimensions and the texts.
//!
//! | Planar | 3D sketch |
//! |---|---|
//! | point `(x, y)` | point `(x, y, 0)` (where the last solve left it) |
//! | line | line; a centre line becomes a construction line |
//! | arc, circle | arc or circle bound to the sketch plane (`Release Plane` frees it) |
//! | ellipse, elliptical arc, text | plane-bound ellipse, elliptical arc, text |
//! | spline | spline of the same kind, degree, weights and knots |
//! | coincidence of two points | 3D coincidence |
//! | fixed point or curve | in-plane fix (its x and y) |
//! | any other constraint or dimension | in-plane relation (on the x and y of its points) |
//!
//! One `On Sketch Plane` relation keeps the geometry in the plane: the
//! representative (lowest id) of each elevation group, so a fully
//! constrained planar sketch stays fully constrained and nothing becomes
//! redundant. A group whose elevation something else holds (an attached
//! point, a point on a projected curve) has none.
//!
//! Linked projections keep following their sources as Project-mode
//! reference records: a projected point (of a vertex, another sketch's
//! point) is attached to its source; a projected line stays a construction
//! line whose ends are attached to the source curve's start and end; any
//! other projected curve becomes a reference curve of its source, with its
//! ends attached where something uses them, a point on it (a planar
//! coincidence) put on the reference curve and a midpoint of it kept as
//! one. The document checks each attached pair of ends against where the
//! source's start and end are and swaps them where the projection runs the
//! other way ([`swap_ends`]). Unlinked projected geometry (no record) is
//! plain fixed geometry and stays so, as construction geometry.
//!
//! Nothing is refused (mitcad#1550): what 3D sketches have no own form for
//! is kept in the 3D sketch's planar records
//! ([`crate::spatial_planar::PlanarRecords`]) and acts in the sketch plane
//! as it acted in the planar sketch: patterns and offsets (bound in the
//! plane; the curves of derived offsets computed there), the placement on
//! the plane, a plane of another component, texts bound to text
//! parameters, inactive constraints and dimensions (kept, holding
//! nothing), and every linked projection a reference record cannot stand
//! for (a face's edges, section curves, a chain of edges, a source as an
//! earlier feature left it, several curves of one source, a projected
//! curve other than a line whose centre or other points the sketch uses or
//! that a relation names): its entities stay geometry of the sketch held
//! in the plane, following the source at every evaluation. The planar
//! flags (reference, centre line, the sketch's axes, where the dimensions'
//! values are shown) are kept too, so that the sketch converts back to the
//! same planar sketch ([`crate::spatial_planar::planar_from_spatial`]).

use std::collections::{BTreeMap, BTreeSet};

use crate::def::SketchDef;
use crate::ids::EntityUid;
use crate::solve::Solved;
use crate::spatial::{
    SpatialAxis, SpatialConstraintDef, SpatialConstraintKind, SpatialCurve, SpatialCurveKind,
    SpatialCurvePoint, SpatialDimension, SpatialDimensionKind, SpatialDirection, SpatialPoint,
    SpatialSketchDef, SpatialSource, SpatialSplineEnd, SpatialText,
};
use crate::spatial_planar::{DimensionLabel, InactiveRelation};
use crate::topo::TopoName;
use crate::{
    ConstraintKind, ConstraintUid, Entity, EntityIndex, EntityKind, Projection, ProjectionSource,
    Ref, SketchAxis,
};

/// What a conversion made of a planar sketch.
#[derive(Debug, Clone, PartialEq)]
pub struct Converted {
    pub def: SpatialSketchDef,
    /// The planar constraints and dimensions kept as in-plane ones (the
    /// in-plane fixes of fixed entities have new ids, in `fixes`).
    pub in_plane: Vec<ConstraintUid>,
    /// The planar coincidences of two points kept as 3D coincidences.
    pub coincident: Vec<ConstraintUid>,
    /// The in-plane fixes made for the planar sketch's fixed entities.
    pub fixes: Vec<ConstraintUid>,
    /// The On Sketch Plane relation and the points it lists.
    pub on_sketch_plane: Option<(ConstraintUid, Vec<EntityUid>)>,
    /// The attachments made for linked projections.
    pub attachments: Vec<ConstraintUid>,
    /// The reference curves made of projected curves (their planar ids).
    pub references: Vec<EntityUid>,
    /// The attachments of one projected curve's start and end, checked by
    /// the document against where the source's ends are ([`swap_ends`]).
    pub ends: Vec<[ConstraintUid; 2]>,
    /// Points of projected curves that nothing else used, left out with
    /// their curves turned into reference curves.
    pub dropped: Vec<EntityUid>,
    /// What the 3D sketch keeps in its planar records (mitcad#1550), named:
    /// `k7 (circular pattern)`, `the placement on the plane`, ...
    pub kept: Vec<String>,
}

/// One projected curve or point of a linked projection, and its source.
struct Linked {
    source: SourceKind,
    curve: Option<EntityUid>,
    points: Vec<EntityUid>,
}

/// A linked projection's source, as a 3D sketch's source.
enum SourceKind {
    Vertex(SpatialSource),
    /// Made with `at` for a point of the curve.
    Curve(Box<dyn Fn(Option<SpatialCurvePoint>) -> SpatialSource>),
}

fn curve_name(id: EntityUid) -> String {
    format!("c{}", id.0)
}

/// The source of a linked projection as a 3D sketch's source; an error
/// naming the projection when a 3D sketch cannot follow it.
fn linked(projection: &Projection) -> Result<Linked, String> {
    let named = || format!("the projection of {}", projection.source);
    if projection.section {
        return Err(format!(
            "{} gives section curves, which a 3D sketch does not keep linked",
            named()
        ));
    }
    if !projection.chain.is_empty() || projection.outline || projection.after.is_some() {
        return Err(format!(
            "{} is not one edge, vertex or sketch entity, which is what a 3D sketch keeps linked",
            named()
        ));
    }
    let link = projection.link.clone();
    let curves: Vec<EntityUid> = projection
        .entities
        .iter()
        .filter(|r| !r.is_point())
        .map(|r| r.uid())
        .collect();
    let points: Vec<EntityUid> = projection
        .entities
        .iter()
        .filter(|r| r.is_point())
        .map(|r| r.uid())
        .collect();
    let source = match (&projection.source, projection.body) {
        (ProjectionSource::Topo(TopoName::Vertex(vertex)), Some(body)) => {
            SourceKind::Vertex(SpatialSource::Vertex {
                body,
                vertex: vertex.clone(),
                link,
            })
        }
        (ProjectionSource::Topo(TopoName::Edge(edge)), Some(body)) => {
            let edge = edge.clone();
            SourceKind::Curve(Box::new(move |at| SpatialSource::Edge {
                body,
                edge: Box::new(edge.clone()),
                at,
                link: link.clone(),
            }))
        }
        (ProjectionSource::Topo(TopoName::Face(_)), _) => {
            return Err(format!(
                "{} is a face's edges, which a 3D sketch does not keep linked (it keeps edges, \
                 vertices and sketch entities one by one)",
                named()
            ));
        }
        (ProjectionSource::Topo(_), None) => {
            return Err(format!("{} names no body", named()));
        }
        (ProjectionSource::Sketch(entity), _) if entity.entity.is_point() => {
            SourceKind::Vertex(SpatialSource::Sketch {
                entity: *entity,
                at: None,
                link,
            })
        }
        (ProjectionSource::Sketch(entity), _) => {
            let entity = *entity;
            SourceKind::Curve(Box::new(move |at| SpatialSource::Sketch {
                entity,
                at,
                link: link.clone(),
            }))
        }
        (ProjectionSource::Curve(feature), _) => {
            let feature = *feature;
            SourceKind::Curve(Box::new(move |at| SpatialSource::Curve {
                feature,
                at,
                link: link.clone(),
            }))
        }
    };
    let curve = match (&source, curves.as_slice()) {
        (SourceKind::Vertex(_), []) => None,
        (SourceKind::Curve(_), [curve]) => Some(*curve),
        _ => {
            return Err(format!(
                "{} gives {} curves and {} points; a 3D sketch keeps one projected curve or point \
                 of a source linked",
                named(),
                curves.len(),
                points.len()
            ));
        }
    };
    Ok(Linked {
        source,
        curve,
        points,
    })
}

/// Whether a linked projection's curve can be a reference curve of the 3D
/// sketch (one projected curve other than a line): only points on it and
/// at its middle relate to it, the sketch uses only its ends, and no
/// pattern or offset names it. Otherwise its projection is kept as a
/// planar record (mitcad#1550).
fn reference_fits(def: &SketchDef, curve: EntityUid) -> bool {
    let Some(e) = def.entities.iter().find(|e| e.id == curve) else {
        return false;
    };
    let ends = e.kind.ends();
    let named = |refs: Vec<Ref>| refs.contains(&Ref::Curve(curve));
    let relations = def.constraints.iter().all(|c| match &c.kind {
        ConstraintKind::Coincident {
            entity: Ref::Curve(x),
            ..
        }
        | ConstraintKind::Midpoint { curve: x, .. }
            if *x == curve =>
        {
            c.active
        }
        kind => !named(kind.refs()),
    }) && def.dimensions.iter().all(|d| !named(d.kind.refs()));
    let points = e.kind.points().into_iter().all(|p| {
        ends.is_some_and(|(s, t)| p == s || p == t)
            || !(def
                .entities
                .iter()
                .any(|o| o.id != curve && o.kind.points().contains(&p))
                || def
                    .constraints
                    .iter()
                    .any(|c| c.kind.refs().contains(&Ref::Point(p)))
                || def
                    .dimensions
                    .iter()
                    .any(|d| d.kind.refs().contains(&Ref::Point(p)))
                || def.texts.iter().any(|t| t.frame.contains(&p)))
    });
    let records = def.patterns.iter().all(|p| {
        !p.entities.iter().any(|r| r.uid() == curve)
            && !p
                .copies
                .iter()
                .any(|c| c.entities.values().any(|r| r.uid() == curve))
    }) && def
        .offsets
        .iter()
        .all(|o| !o.curves.contains(&curve) && !o.results.contains(&curve));
    relations && points && records
}

/// The 3D sketch a planar sketch `def` becomes, its points where `solved`
/// (the last evaluation) has them, else where the definition keeps them.
pub fn spatial_from_planar(def: &SketchDef, solved: Option<&Solved>) -> Result<Converted, String> {
    let entity = |id: EntityUid| def.entities.iter().find(|e| e.id == id);
    // The curves and points derived offsets compute: kept planar, made in
    // the plane after every solve.
    let derived = crate::offset::derived_entities(&def.offsets, &EntityIndex::new(&def.entities));
    // The linked projections, by their curves and points; those a
    // reference record cannot stand for are kept as planar records.
    let mut projected_curve: BTreeMap<EntityUid, usize> = BTreeMap::new();
    let mut projected_point: BTreeMap<EntityUid, usize> = BTreeMap::new();
    let mut sources = Vec::new();
    let mut linked_projections: Vec<&Projection> = Vec::new();
    let mut kept_projections: Vec<Projection> = Vec::new();
    for projection in &def.projections {
        let found = match linked(projection) {
            Ok(found) => found,
            Err(_) => {
                kept_projections.push(projection.clone());
                continue;
            }
        };
        let fits = match found.curve {
            Some(curve) => {
                matches!(
                    entity(curve).map(|e| &e.kind),
                    Some(EntityKind::Line { .. })
                ) || reference_fits(def, curve)
            }
            None => true,
        };
        let in_records = projection
            .entities
            .iter()
            .any(|r| derived.contains(&r.uid()));
        if !fits || in_records {
            kept_projections.push(projection.clone());
            continue;
        }
        let index = sources.len();
        if let Some(curve) = found.curve {
            projected_curve.insert(curve, index);
        } else {
            for p in &found.points {
                projected_point.insert(*p, index);
            }
        }
        sources.push(found);
        linked_projections.push(projection);
    }
    // Projected curves other than lines become reference curves.
    let references: BTreeSet<EntityUid> = projected_curve
        .keys()
        .copied()
        .filter(|id| !matches!(entity(*id).map(|e| &e.kind), Some(EntityKind::Line { .. })))
        .collect();

    // What uses each point, apart from the curves that it defines and that
    // become reference curves.
    let mut used: BTreeSet<EntityUid> = BTreeSet::new();
    for e in &def.entities {
        if !references.contains(&e.id) {
            used.extend(e.kind.points());
        }
    }
    for c in &def.constraints {
        used.extend(
            c.kind
                .refs()
                .into_iter()
                .filter(|r| r.is_point())
                .map(Ref::uid),
        );
    }
    for d in &def.dimensions {
        used.extend(
            d.kind
                .refs()
                .into_iter()
                .filter(|r| r.is_point())
                .map(Ref::uid),
        );
    }
    for t in &def.texts {
        used.extend(t.frame.iter().copied());
    }

    let at = |e: &Entity| -> [f64; 3] {
        let xy = solved
            .and_then(|s| s.points.get(&e.id).copied())
            .or(match e.kind {
                EntityKind::Point { at } => Some(at),
                _ => None,
            })
            .unwrap_or([0.0, 0.0]);
        [xy[0], xy[1], 0.0]
    };
    let radius = |id: EntityUid, stored: f64| -> f64 {
        solved
            .and_then(|s| s.radii.get(&id).copied())
            .unwrap_or(stored)
    };

    let mut out = SpatialSketchDef {
        support: def.plane.clone(),
        frame: def.frame.clone(),
        ..SpatialSketchDef::default()
    };
    let mut dropped = Vec::new();
    // Points of reference curves that nothing else uses go with them.
    let reference_points: BTreeSet<EntityUid> = references
        .iter()
        .filter_map(|id| entity(*id))
        .flat_map(|e| e.kind.points())
        .collect();
    for e in def
        .entities
        .iter()
        .filter(|e| e.is_point() && !derived.contains(&e.id))
    {
        if reference_points.contains(&e.id) && !used.contains(&e.id) {
            dropped.push(e.id);
            continue;
        }
        out.points.push(SpatialPoint {
            id: e.id,
            at: at(e),
        });
    }
    let plane = Some(SpatialDirection::SKETCH_PLANE);
    for e in def
        .entities
        .iter()
        .filter(|e| !e.is_point() && !derived.contains(&e.id))
    {
        let construction = e.construction || e.reference;
        if let Some(index) = projected_curve.get(&e.id)
            && references.contains(&e.id)
        {
            let SourceKind::Curve(source) = &sources[*index].source else {
                unreachable!("a projected curve has a curve source");
            };
            out.curves.push(SpatialCurve::new(
                e.id,
                SpatialCurveKind::Reference {
                    source: source(None),
                    projected: true,
                },
                construction,
            ));
            continue;
        }
        let kind = match &e.kind {
            EntityKind::Point { .. } => unreachable!("points are taken above"),
            EntityKind::Line {
                start,
                end,
                centerline,
            } => {
                let curve = SpatialCurve::new(
                    e.id,
                    SpatialCurveKind::Line {
                        start: *start,
                        end: *end,
                    },
                    construction || *centerline,
                );
                out.curves.push(curve);
                continue;
            }
            EntityKind::Circle {
                center, radius: r, ..
            } => SpatialCurveKind::Circle {
                center: *center,
                radius: radius(e.id, *r),
                normal: [0.0, 0.0, 1.0],
                plane,
            },
            EntityKind::Arc { center, start, end } => SpatialCurveKind::Arc {
                center: *center,
                start: *start,
                end: *end,
                normal: [0.0, 0.0, 1.0],
                plane,
            },
            EntityKind::Ellipse {
                center,
                major,
                minor_radius,
            } => SpatialCurveKind::Ellipse {
                center: *center,
                major: *major,
                minor_radius: radius(e.id, *minor_radius),
                normal: SpatialAxis::Z,
                frame: None,
            },
            EntityKind::EllipticalArc {
                center,
                major,
                minor_radius,
                start,
                end,
            } => SpatialCurveKind::EllipticalArc {
                center: *center,
                major: *major,
                minor_radius: radius(e.id, *minor_radius),
                normal: SpatialAxis::Z,
                start: *start,
                end: *end,
            },
            EntityKind::Spline {
                degree,
                control,
                weights,
                knots,
            } => SpatialCurveKind::Spline {
                degree: *degree,
                control: control.clone(),
                weights: weights.clone(),
                knots: knots.clone(),
            },
            EntityKind::FittedSpline { points, closed } => SpatialCurveKind::FittedSpline {
                points: points.clone(),
                closed: *closed,
                ends: [SpatialSplineEnd::Natural; 2],
                tangents: [None, None],
            },
        };
        out.curves.push(SpatialCurve::new(e.id, kind, construction));
    }
    for t in &def.texts {
        out.texts.push(SpatialText {
            planar: t.clone(),
            z: 0.0,
            normal: SpatialAxis::Z,
            frame: None,
        });
    }

    // New relation ids after the planar ones.
    let mut next = def
        .constraints
        .iter()
        .map(|c| c.id.0)
        .chain(def.dimensions.iter().map(|d| d.id.0))
        .chain(def.patterns.iter().map(|p| p.id.0))
        .chain(def.offsets.iter().map(|o| o.id.0))
        .max()
        .unwrap_or(0);
    let mut new_id = || {
        next += 1;
        ConstraintUid(next)
    };
    let mut converted = Converted {
        def: SpatialSketchDef::default(),
        in_plane: Vec::new(),
        coincident: Vec::new(),
        fixes: Vec::new(),
        on_sketch_plane: None,
        attachments: Vec::new(),
        references: references.iter().copied().collect(),
        ends: Vec::new(),
        dropped,
        kept: Vec::new(),
    };
    // Points whose elevation something other than the plane holds.
    let mut held: BTreeSet<EntityUid> = BTreeSet::new();

    // The planar relations, in their order and with their ids.
    let on_reference = |r: &Ref| !r.is_point() && references.contains(&r.uid());
    // In plain words for a person, the ids for agents (mitcad#1644).
    let what = |id: ConstraintUid, kind: &str, curve: EntityUid| {
        crate::said(
            "a relation or dimension relates to a projected curve: a 3D sketch keeps projected \
             curves other than lines as reference curves, which take only points on them and \
             midpoints; Convert Projected makes it the sketch's own geometry first",
            format!(
                "{id} ({kind}) relates to {}, a projected curve: a 3D sketch keeps projected \
                 curves other than lines as reference curves, which take only points on them and \
                 midpoints; Convert Projected makes it the sketch's own geometry first",
                curve_name(curve)
            ),
        )
    };
    for c in &def.constraints {
        // Inactive: kept in the plane, holding nothing until activated.
        if !c.active {
            converted.in_plane.push(c.id);
            out.planar.inactive.push(InactiveRelation {
                id: c.id,
                reason: c.inactive_reason.clone(),
            });
            converted.kept.push(format!("{} (inactive)", c.id));
            out.constraints.push(SpatialConstraintDef {
                id: c.id,
                kind: SpatialConstraintKind::InPlane {
                    relation: c.kind.clone(),
                    normal: SpatialAxis::Z,
                },
            });
            continue;
        }
        let kind = match &c.kind {
            ConstraintKind::Coincident {
                point,
                entity: Ref::Point(b),
            } => {
                converted.coincident.push(c.id);
                SpatialConstraintKind::Coincident { a: *point, b: *b }
            }
            ConstraintKind::Coincident {
                point,
                entity: Ref::Curve(curve),
            } if references.contains(curve) => {
                held.insert(*point);
                SpatialConstraintKind::PointOn {
                    point: *point,
                    curve: *curve,
                }
            }
            ConstraintKind::Midpoint { point, curve } if references.contains(curve) => {
                held.insert(*point);
                SpatialConstraintKind::Midpoint {
                    point: *point,
                    curve: *curve,
                }
            }
            relation => {
                if let Some(r) = relation.refs().iter().find(|r| on_reference(r)) {
                    return Err(what(c.id, &relation.type_name(), r.uid()));
                }
                converted.in_plane.push(c.id);
                SpatialConstraintKind::InPlane {
                    relation: relation.clone(),
                    normal: SpatialAxis::Z,
                }
            }
        };
        out.constraints
            .push(SpatialConstraintDef { id: c.id, kind });
    }
    for d in &def.dimensions {
        if let Some(r) = d.kind.refs().iter().find(|r| on_reference(r)) {
            return Err(what(d.id, d.kind.type_name(), r.uid()));
        }
        converted.in_plane.push(d.id);
        if !d.active {
            out.planar.inactive.push(InactiveRelation {
                id: d.id,
                reason: d.inactive_reason.clone(),
            });
            converted.kept.push(format!("{} (inactive)", d.id));
        }
        if let Some(at) = d.text {
            out.planar.labels.push(DimensionLabel { id: d.id, at });
        }
        let mut dimension = SpatialDimension::new(
            d.id,
            SpatialDimensionKind::InPlane {
                dimension: d.kind.clone(),
                normal: SpatialAxis::Z,
            },
            d.value,
        );
        // A driven dimension keeps its reference parameter (mitcad#1661).
        dimension.reference = d.reference;
        out.dimensions.push(dimension);
    }

    // Linked projections: attachments to their sources.
    let mut attached: BTreeSet<EntityUid> = BTreeSet::new();
    let mut attach = |out: &mut SpatialSketchDef,
                      point: EntityUid,
                      source: SpatialSource,
                      converted: &mut Converted|
     -> Option<ConstraintUid> {
        if !attached.insert(point) {
            // Held by an attachment already (an end two projected curves
            // share).
            return None;
        }
        let id = new_id();
        out.constraints.push(SpatialConstraintDef {
            id,
            kind: SpatialConstraintKind::Attach {
                point,
                source,
                projected: true,
            },
        });
        converted.attachments.push(id);
        Some(id)
    };
    for (index, found) in sources.iter().enumerate() {
        match (&found.source, found.curve) {
            (SourceKind::Vertex(source), _) => {
                for p in &found.points {
                    if projected_point.get(p) == Some(&index) {
                        attach(&mut out, *p, source.clone(), &mut converted);
                    }
                }
            }
            (SourceKind::Curve(source), Some(curve)) => {
                let Some(e) = entity(curve) else {
                    return Err(crate::said(
                        "a projection lists an entity the sketch does not have",
                        format!(
                            "the projection of {} lists {}, which the sketch does not have",
                            linked_projections[index].source,
                            curve_name(curve)
                        ),
                    ));
                };
                let ends = e.kind.ends();
                // The points of a reference curve that something uses: its
                // ends are attached; another one cannot be kept.
                if references.contains(&curve) {
                    for p in e.kind.points() {
                        let is_end = ends.is_some_and(|(s, t)| p == s || p == t);
                        if used.contains(&p) && !is_end {
                            return Err(crate::said(
                                "a point of a projected curve is used by the sketch: a 3D sketch \
                                 keeps projected curves other than lines as reference curves, \
                                 whose centre or other points it cannot keep; Convert Projected \
                                 makes it the sketch's own geometry first",
                                format!(
                                    "p{} of {}, a projected curve, is used by the sketch: a 3D \
                                     sketch keeps projected curves other than lines as reference \
                                     curves, whose centre or other points it cannot keep; Convert \
                                     Projected makes it the sketch's own geometry first",
                                    p.0,
                                    curve_name(curve)
                                ),
                            ));
                        }
                    }
                }
                if let Some((s, t)) = ends {
                    let keep = |p: EntityUid| !references.contains(&curve) || used.contains(&p);
                    let a = keep(s)
                        .then(|| {
                            attach(
                                &mut out,
                                s,
                                source(Some(SpatialCurvePoint::Start)),
                                &mut converted,
                            )
                        })
                        .flatten();
                    let b = keep(t)
                        .then(|| {
                            attach(
                                &mut out,
                                t,
                                source(Some(SpatialCurvePoint::End)),
                                &mut converted,
                            )
                        })
                        .flatten();
                    if let (Some(a), Some(b)) = (a, b) {
                        converted.ends.push([a, b]);
                    }
                }
            }
            (SourceKind::Curve(_), None) => {}
        }
    }
    held.extend(attached.iter().copied());

    // Fixed entities: in-plane fixes, except what an attachment holds or a
    // reference curve is, and a line or spline held by its fixed points
    // (as the planar solve leaves them out, mitcad#877).
    let fixed_points: BTreeSet<EntityUid> = def
        .entities
        .iter()
        .filter(|e| e.fixed && e.is_point())
        .map(|e| e.id)
        .collect();
    for e in def.entities.iter().filter(|e| e.fixed) {
        if attached.contains(&e.id)
            || references.contains(&e.id)
            || converted.dropped.contains(&e.id)
            || derived.contains(&e.id)
        {
            continue;
        }
        let all = |points: &[EntityUid]| points.iter().all(|p| fixed_points.contains(p));
        let held_by_points = match &e.kind {
            EntityKind::Line { start, end, .. } => all(&[*start, *end]),
            EntityKind::Spline { control, .. } => all(control),
            EntityKind::FittedSpline { points, .. } => all(points),
            _ => false,
        };
        if held_by_points {
            continue;
        }
        let id = new_id();
        out.constraints.push(SpatialConstraintDef {
            id,
            kind: SpatialConstraintKind::InPlaneFixed { entity: e.as_ref() },
        });
        converted.fixes.push(id);
    }

    // One On Sketch Plane row per elevation group that nothing else holds.
    let groups = out.elevation_groups();
    let held_groups: BTreeSet<EntityUid> =
        held.iter().filter_map(|p| groups.get(p)).copied().collect();
    let representatives: Vec<EntityUid> = groups
        .values()
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .filter(|g| !held_groups.contains(g))
        .collect();
    if !representatives.is_empty() {
        let id = new_id();
        out.constraints.push(SpatialConstraintDef {
            id,
            kind: SpatialConstraintKind::OnSketchPlane {
                points: representatives.clone(),
            },
        });
        converted.on_sketch_plane = Some((id, representatives));
    }

    // What 3D sketches have no own form for, kept to act in the plane and
    // to convert back (mitcad#1550).
    let present: BTreeSet<EntityUid> = out
        .points
        .iter()
        .map(|p| p.id)
        .chain(out.curves.iter().map(|c| c.id))
        .chain(derived.iter().copied())
        .collect();
    let records = &mut out.planar;
    records.plane_link = def.plane_link.clone();
    if def.plane_link.is_some() {
        converted
            .kept
            .push("its plane, another component's".to_owned());
    }
    records.placement = def.placement.clone();
    if def.placement.is_some() {
        converted.kept.push("its placement on the plane".to_owned());
    }
    records.patterns = def.patterns.clone();
    for p in &def.patterns {
        converted.kept.push(format!("{} (pattern)", p.id));
    }
    records.offsets = def.offsets.clone();
    for o in &def.offsets {
        converted.kept.push(format!("{} (offset)", o.id));
    }
    records.text_values = def.text_values.clone();
    for t in &def.text_values {
        converted
            .kept
            .push(format!("t{} (text parameter)", t.text.0));
    }
    for p in &kept_projections {
        converted
            .kept
            .push(format!("the projection of {}", p.source));
    }
    records.projections = kept_projections;
    records.derived = def
        .entities
        .iter()
        .filter(|e| derived.contains(&e.id))
        .cloned()
        .collect();
    for e in def.entities.iter().filter(|e| present.contains(&e.id)) {
        let r = e.as_ref();
        if e.reference {
            records.references.push(r);
            if e.construction {
                records.construction_references.push(r);
            }
        }
        if let EntityKind::Line {
            centerline: true, ..
        } = e.kind
        {
            records.centerlines.push(e.id);
        }
        match e.axis {
            Some(SketchAxis::X) => records.x_axis.push(r),
            Some(SketchAxis::Y) => records.y_axis.push(r),
            None => {}
        }
    }
    out.check_definition()?;
    converted.def = out;
    Ok(converted)
}

/// Swaps the source points (start and end) of two attachments of one
/// projected curve's ends: the projection runs the other way along its
/// source.
pub fn swap_ends(def: &mut SpatialSketchDef, pair: [ConstraintUid; 2]) {
    for c in def.constraints.iter_mut().filter(|c| pair.contains(&c.id)) {
        if let SpatialConstraintKind::Attach {
            source:
                SpatialSource::Edge { at, .. }
                | SpatialSource::Sketch { at, .. }
                | SpatialSource::Curve { at, .. },
            ..
        } = &mut c.kind
        {
            *at = match *at {
                Some(SpatialCurvePoint::Start) => Some(SpatialCurvePoint::End),
                Some(SpatialCurvePoint::End) => Some(SpatialCurvePoint::Start),
                other => other,
            };
        }
    }
}
