// SPDX-License-Identifier: MIT
//! The sketch plane of a 3D sketch in planar form (mitcad#1550).
//!
//! - [`PlanarRecords`]: what a 3D sketch converted from a planar one keeps
//!   of the planar sketch that 3D sketches have no own form for, so that it
//!   converts without refusals and back again: its plane of another
//!   component, its placement on the plane, its patterns and offsets, the
//!   texts that follow text parameters, the inactive constraints and
//!   dimensions, its linked projections of a face, section curves and
//!   chains of edges, the curves derived offsets compute, and the planar
//!   flags (reference, centre line, the sketch's own axes, the places of
//!   the dimensions' values). They act in the sketch plane as they act in
//!   the planar sketch: the patterns' and offsets' bindings are in-plane
//!   rows of the solve ([`crate::spatial::SpatialSketchDef::build_with`]),
//!   the kept projections follow their sources at the evaluation, the
//!   placement moves the frame.
//! - [`planar_view`]: what lies in the sketch plane (`|z|` at most
//!   [`PLANE_TOLERANCE`]) as a planar sketch with its solved geometry, from
//!   which the model finds the profile regions of a 3D sketch: its closed
//!   loops in the sketch plane bound regions as a planar sketch's do, with
//!   the same keys for the same curves, so the profiles of a converted
//!   sketch keep their names.
//! - [`planar_from_spatial`]: a 3D sketch whose geometry all lies in the
//!   sketch plane as the planar sketch it becomes in place (3D Sketch
//!   unchecked): every id kept, in-plane relations and dimensions back as
//!   planar ones, 3D relations as their planar equivalents, attachments
//!   and reference curves as linked projections; refused naming what lies
//!   out of the plane.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::def::SketchDef;
use crate::geometry::Curve2;
use crate::ids::EntityUid;
use crate::kernel::Curve3;
use crate::occurrence::OccurrenceLink;
use crate::offset::SketchOffset;
use crate::parameters::ParamId;
use crate::pattern::{Binding, SketchPattern};
use crate::placement::SketchPlacement;
use crate::profile::SketchFrame;
use crate::project::{self, Projected};
use crate::solve::Solved;
use crate::spatial::{
    SolvedCurve, SpatialAxis, SpatialConstraintKind, SpatialCurve, SpatialCurveKind,
    SpatialCurvePoint, SpatialDimensionKind, SpatialPlaneRef, SpatialReferenceCurve,
    SpatialSketchDef, SpatialSketchOutput, SpatialSource,
};
use crate::topo::TopoName;
use crate::{
    Constraint, ConstraintKind, ConstraintUid, Dimension, DimensionKind, Entity, EntityIndex,
    EntityKind, Projection, ProjectionSource, Ref, SketchAxis, TextValue,
};
// Refusals in words, the ids for agents (mitcad#1644).
use mitcad_model_base::notice::{said_after, said_at};

/// How far from the sketch plane (local `z`) a point or a curve may be and
/// still lie in it, millimetres: the project's geometric baseline.
pub const PLANE_TOLERANCE: f64 = 1e-6;

/// A planar constraint or dimension kept inactive (mitcad#721): it holds
/// nothing until it is activated, as in the planar sketch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InactiveRelation {
    pub id: ConstraintUid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Where a planar dimension's value was shown, in sketch coordinates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DimensionLabel {
    pub id: ConstraintUid,
    pub at: [f64; 2],
}

/// What a 3D sketch converted from a planar one keeps of it that 3D
/// sketches have no own form for (see the module).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanarRecords<P = ParamId> {
    /// The plane is another component's (mitcad#100): the support is found
    /// there and moved into the sketch's component.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plane_link: Option<OccurrenceLink>,
    /// Where the sketch lies on its plane (mitcad#787).
    #[serde(default = "no_placement", skip_serializing_if = "Option::is_none")]
    pub placement: Option<Box<SketchPlacement<P>>>,
    /// Circular and rectangular patterns, bound in the plane.
    #[serde(default = "Vec::new", skip_serializing_if = "Vec::is_empty")]
    pub patterns: Vec<SketchPattern<P>>,
    /// Offsets with their distances, bound and computed in the plane.
    #[serde(default = "Vec::new", skip_serializing_if = "Vec::is_empty")]
    pub offsets: Vec<SketchOffset<P>>,
    /// Texts whose text is a text parameter's value (mitcad#664).
    #[serde(default = "Vec::new", skip_serializing_if = "Vec::is_empty")]
    pub text_values: Vec<TextValue<P>>,
    /// Linked projections whose entities follow their sources in the plane
    /// as a whole (a face's edges, section curves, a chain of edges, a
    /// source as an earlier feature left it); their entities are geometry
    /// of the sketch held in the plane, moved to their sources at every
    /// evaluation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub projections: Vec<Projection>,
    /// The curves and points derived offsets compute after the solve
    /// (offsets of ellipses and splines): planar only, no unknowns.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub derived: Vec<Entity>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inactive: Vec<InactiveRelation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<DimensionLabel>,
    /// Projected entities of the planar sketch (its reference flag).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub references: Vec<Ref>,
    /// Of those, the construction ones (the others bound profiles).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub construction_references: Vec<Ref>,
    /// Lines that were centre lines.
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        with = "crate::curves_serde"
    )]
    pub centerlines: Vec<EntityUid>,
    /// The entities of the sketch's own x and y axes (mitcad#500).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub x_axis: Vec<Ref>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub y_axis: Vec<Ref>,
}

fn no_placement<P>() -> Option<Box<SketchPlacement<P>>> {
    None
}

impl<P> Default for PlanarRecords<P> {
    fn default() -> Self {
        Self {
            plane_link: None,
            placement: None,
            patterns: Vec::new(),
            offsets: Vec::new(),
            text_values: Vec::new(),
            projections: Vec::new(),
            derived: Vec::new(),
            inactive: Vec::new(),
            labels: Vec::new(),
            references: Vec::new(),
            construction_references: Vec::new(),
            centerlines: Vec::new(),
            x_axis: Vec::new(),
            y_axis: Vec::new(),
        }
    }
}

impl<P> PlanarRecords<P> {
    /// Nothing kept: a sketch made as a 3D sketch.
    pub fn is_empty(&self) -> bool {
        self.plane_link.is_none()
            && self.placement.is_none()
            && self.patterns.is_empty()
            && self.offsets.is_empty()
            && self.text_values.is_empty()
            && self.projections.is_empty()
            && self.derived.is_empty()
            && self.inactive.is_empty()
            && self.labels.is_empty()
            && self.references.is_empty()
            && self.construction_references.is_empty()
            && self.centerlines.is_empty()
            && self.x_axis.is_empty()
            && self.y_axis.is_empty()
    }

    pub fn map_params<Q, E>(
        &self,
        f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
    ) -> Result<PlanarRecords<Q>, E> {
        Ok(PlanarRecords {
            plane_link: self.plane_link.clone(),
            placement: match &self.placement {
                Some(p) => Some(Box::new(p.map_value(f)?)),
                None => None,
            },
            patterns: self
                .patterns
                .iter()
                .map(|p| p.map_value(f))
                .collect::<Result<_, _>>()?,
            offsets: self
                .offsets
                .iter()
                .map(|o| o.map_value(f))
                .collect::<Result<_, _>>()?,
            text_values: self
                .text_values
                .iter()
                .map(|v| v.map_value(f))
                .collect::<Result<_, _>>()?,
            projections: self.projections.clone(),
            derived: self.derived.clone(),
            inactive: self.inactive.clone(),
            labels: self.labels.clone(),
            references: self.references.clone(),
            construction_references: self.construction_references.clone(),
            centerlines: self.centerlines.clone(),
            x_axis: self.x_axis.clone(),
            y_axis: self.y_axis.clone(),
        })
    }

    /// The ids of the inactive relations.
    pub fn inactive_ids(&self) -> BTreeSet<ConstraintUid> {
        self.inactive.iter().map(|i| i.id).collect()
    }

    /// Whether `id` is inactive.
    pub fn is_inactive(&self, id: ConstraintUid) -> bool {
        self.inactive.iter().any(|i| i.id == id)
    }

    /// The constraint ids its patterns and offsets take (they share the
    /// numbers with constraints and dimensions, as in a planar sketch).
    pub fn constraint_ids(&self) -> impl Iterator<Item = ConstraintUid> + '_ {
        self.patterns
            .iter()
            .map(|p| p.id)
            .chain(self.offsets.iter().map(|o| o.id))
    }

    /// The entity ids of the derived offsets' curves and points.
    pub fn derived_ids(&self) -> impl Iterator<Item = EntityUid> + '_ {
        self.derived.iter().map(|e| e.id)
    }

    /// The entities its kept projections list.
    pub fn projected(&self) -> BTreeSet<EntityUid> {
        self.projections
            .iter()
            .flat_map(|p| p.entities.iter().map(|r| r.uid()))
            .collect()
    }
}

/// The planar entity of a curve of a 3D sketch that lies in the sketch
/// plane: a line, an arc or circle about the frame's z axis (`flipped`:
/// about -z, whose start and end change places), an ellipse, a spline.
/// None for a reference curve.
fn planar_kind(curve: &SpatialCurve, flipped: bool, centerline: bool) -> Option<EntityKind> {
    Some(match &curve.kind {
        SpatialCurveKind::Line { start, end } => EntityKind::Line {
            start: *start,
            end: *end,
            centerline,
        },
        SpatialCurveKind::Arc {
            center, start, end, ..
        } => {
            let (start, end) = if flipped {
                (*end, *start)
            } else {
                (*start, *end)
            };
            EntityKind::Arc {
                center: *center,
                start,
                end,
            }
        }
        SpatialCurveKind::Circle { center, radius, .. } => EntityKind::Circle {
            center: *center,
            radius: *radius,
            start: 0.0,
        },
        SpatialCurveKind::Ellipse {
            center,
            major,
            minor_radius,
            ..
        } => EntityKind::Ellipse {
            center: *center,
            major: *major,
            minor_radius: *minor_radius,
        },
        SpatialCurveKind::EllipticalArc {
            center,
            major,
            minor_radius,
            start,
            end,
            ..
        } => EntityKind::EllipticalArc {
            center: *center,
            major: *major,
            minor_radius: *minor_radius,
            start: *start,
            end: *end,
        },
        SpatialCurveKind::Spline {
            degree,
            control,
            weights,
            knots,
        } => EntityKind::Spline {
            degree: *degree,
            control: control.clone(),
            weights: weights.clone(),
            knots: knots.clone(),
        },
        SpatialCurveKind::FittedSpline { points, closed, .. } => EntityKind::FittedSpline {
            points: points.clone(),
            closed: *closed,
        },
        SpatialCurveKind::Reference { .. } => return None,
    })
}

impl<P> SpatialSketchDef<P> {
    /// Every point and curve (but reference curves) as a planar entity at
    /// the x and y of its stored position, with the planar flags kept, and
    /// the derived offsets' entities: what the patterns' and offsets'
    /// bindings are made from.
    pub fn planar_entities(&self) -> Vec<Entity> {
        let records = &self.planar;
        let mut out = Vec::new();
        for p in &self.points {
            let mut e = Entity::new(
                p.id,
                EntityKind::Point {
                    at: [p.at[0], p.at[1]],
                },
            );
            self.flag(&mut e, false);
            out.push(e);
        }
        for c in &self.curves {
            let flipped = self.stored_round(c.id).is_some_and(|(n, _)| n[2] < 0.0);
            let centerline = records.centerlines.contains(&c.id);
            if let Some(kind) = planar_kind(c, flipped, centerline) {
                let mut e = Entity::new(c.id, kind);
                self.flag(&mut e, c.construction);
                out.push(e);
            }
        }
        out.extend(records.derived.iter().cloned());
        out
    }

    /// Takes the x and y of `ids`' points, and circles' radii and ellipses'
    /// minor radii, from planar `entities`: where the kept linked
    /// projections followed their sources (the elevations stay).
    pub fn take_planar_positions(&mut self, entities: &[Entity], ids: &BTreeSet<EntityUid>) {
        for e in entities.iter().filter(|e| ids.contains(&e.id)) {
            match &e.kind {
                EntityKind::Point { at } => {
                    if let Some(p) = self.points.iter_mut().find(|p| p.id == e.id) {
                        p.at[0] = at[0];
                        p.at[1] = at[1];
                    }
                }
                EntityKind::Circle { radius, .. } => {
                    if let Some(SpatialCurveKind::Circle { radius: r, .. }) = self
                        .curves
                        .iter_mut()
                        .find(|c| c.id == e.id)
                        .map(|c| &mut c.kind)
                    {
                        *r = *radius;
                    }
                }
                EntityKind::Ellipse { minor_radius, .. }
                | EntityKind::EllipticalArc { minor_radius, .. } => {
                    if let Some(
                        SpatialCurveKind::Ellipse {
                            minor_radius: r, ..
                        }
                        | SpatialCurveKind::EllipticalArc {
                            minor_radius: r, ..
                        },
                    ) = self
                        .curves
                        .iter_mut()
                        .find(|c| c.id == e.id)
                        .map(|c| &mut c.kind)
                    {
                        *r = *minor_radius;
                    }
                }
                _ => {}
            }
        }
    }

    /// Sets an entity's planar flags: fixed by an in-plane fix, the kept
    /// reference, construction and axis flags.
    fn flag(&self, e: &mut Entity, construction: bool) {
        let records = &self.planar;
        let r = e.as_ref();
        e.reference = records.references.contains(&r);
        e.construction = if e.reference {
            records.construction_references.contains(&r)
        } else {
            construction && !records.centerlines.contains(&e.id)
        };
        e.fixed = self.constraints.iter().any(
            |c| matches!(&c.kind, SpatialConstraintKind::InPlaneFixed { entity } if *entity == r),
        );
        e.axis = if records.x_axis.contains(&r) {
            Some(SketchAxis::X)
        } else if records.y_axis.contains(&r) {
            Some(SketchAxis::Y)
        } else {
            None
        };
    }

    /// The in-plane relations as planar constraints, active or not.
    fn planar_constraints(&self) -> Vec<Constraint> {
        self.constraints
            .iter()
            .filter_map(|c| match &c.kind {
                SpatialConstraintKind::InPlane {
                    relation,
                    normal: SpatialAxis::Z,
                } => {
                    let mut planar = Constraint::new(c.id, relation.clone());
                    if let Some(i) = self.planar.inactive.iter().find(|i| i.id == c.id) {
                        planar.active = false;
                        planar.inactive_reason = i.reason.clone();
                    }
                    Some(planar)
                }
                _ => None,
            })
            .collect()
    }
}

impl<P: Clone> SpatialSketchDef<P> {
    /// The in-plane dimensions as planar ones, active or not.
    fn planar_dimensions(&self) -> Vec<Dimension<P>> {
        self.dimensions
            .iter()
            .filter_map(|d| match &d.kind {
                SpatialDimensionKind::InPlane {
                    dimension,
                    normal: SpatialAxis::Z,
                } => {
                    let mut planar = Dimension::new(d.id, dimension.clone(), d.value.clone());
                    planar.text = self
                        .planar
                        .labels
                        .iter()
                        .find(|l| l.id == d.id)
                        .map(|l| l.at);
                    if let Some(i) = self.planar.inactive.iter().find(|i| i.id == d.id) {
                        planar.active = false;
                        planar.inactive_reason = i.reason.clone();
                    }
                    Some(planar)
                }
                _ => None,
            })
            .collect()
    }
}

impl SpatialSketchDef {
    /// The in-plane bindings of the kept patterns (each copy bound to its
    /// original by the pattern's transform) and offsets (their tangent
    /// corners and the helpers that hold them), with the relations the
    /// offsets' corners imply, which the solve leaves out (mitcad#1171);
    /// as a planar sketch's solve makes them (`SketchDef::solve_with`).
    pub fn in_plane_bindings(
        &self,
        value: &mut dyn FnMut(ParamId) -> Result<f64, String>,
    ) -> Result<(Vec<Binding>, BTreeSet<ConstraintUid>), String> {
        let records = &self.planar;
        if records.patterns.is_empty() && records.offsets.is_empty() {
            return Ok((Vec::new(), BTreeSet::new()));
        }
        let entities = self.planar_entities();
        let index = EntityIndex::new(&entities);
        let mut bindings = Vec::new();
        for pattern in &records.patterns {
            let mut values = Vec::new();
            for (slot, id) in pattern.slots().iter().zip(pattern.values()) {
                let v = value(*id)?;
                crate::pattern::check_value(slot, v)
                    .map_err(|e| said_after("", &format!("{} ", pattern.id), &e))?;
                values.push(v);
            }
            bindings.extend(pattern.bindings(&values, &index));
        }
        let patterns = bindings.len();
        let constraints = self.planar_constraints();
        bindings.extend(crate::offset::tangent_corners(
            &records.offsets,
            &entities,
            &constraints,
        ));
        let dimensions = self.planar_dimensions();
        let (anchors, left_out) = crate::offset::independent(
            &records.offsets,
            &entities,
            &constraints,
            &dimensions,
            &bindings[patterns..],
        );
        bindings.extend(anchors);
        Ok((bindings, left_out))
    }
}

/// A curve in local coordinates as a model curve (a reference curve's).
pub fn reference_curve3(curve: &SpatialReferenceCurve) -> Option<Curve3> {
    Some(match curve {
        SpatialReferenceCurve::Line { start, end } => Curve3::Line {
            start: *start,
            end: *end,
        },
        SpatialReferenceCurve::Conic {
            center,
            normal,
            x_axis,
            major,
            minor,
            start,
            end,
            closed,
        } => Curve3::Conic {
            center: *center,
            normal: *normal,
            x_axis: *x_axis,
            major: *major,
            minor: *minor,
            start: *start,
            end: *end,
            closed: *closed,
        },
        SpatialReferenceCurve::BSpline {
            degree,
            poles,
            weights,
            knots,
        } => Curve3::BSpline {
            degree: u32::try_from(*degree).ok()?,
            poles: poles.clone(),
            weights: weights.clone(),
            knots: knots.clone(),
        },
        _ => return None,
    })
}

/// Whether a curve in local coordinates lies in the sketch plane.
fn curve_in_plane(curve: &Curve3) -> bool {
    let flat = |p: &[f64; 3]| p[2].abs() <= PLANE_TOLERANCE;
    match curve {
        Curve3::Point(p) => flat(p),
        Curve3::Line { start, end } => flat(start) && flat(end),
        Curve3::Conic { center, normal, .. } => {
            flat(center) && normal[0].abs() <= 1e-9 && normal[1].abs() <= 1e-9
        }
        Curve3::BSpline { poles, .. } => poles.iter().all(flat),
        Curve3::Helix { .. } => false,
    }
}

/// A curve in local coordinates in the sketch plane as a planar curve.
fn planar_curve(curve: &Curve3) -> Option<Curve2> {
    let mut found = project::project(std::slice::from_ref(curve), &SketchFrame::XY);
    match (found.len(), found.pop()) {
        (1, Some(Projected::Curve(c))) => Some(c),
        _ => None,
    }
}

/// What a 3D sketch's sketch plane gives as a planar sketch (see
/// [`planar_view`] and [`planar_from_spatial`]).
#[derive(Debug, Clone)]
pub struct Flat<P = ParamId> {
    /// The planar sketch: the support and frame, the points, curves and
    /// texts in the plane with their ids, the relations and dimensions as
    /// planar ones, the kept records.
    pub def: SketchDef<P>,
    /// The solved points and curves of what lies in the plane, the x and y
    /// of their solved positions.
    pub solved: Solved,
    /// What lies out of the plane, named (`p5 is 12 mm off the sketch
    /// plane`).
    pub off_plane: Vec<String>,
    /// The same in plain words for a person, one for each (mitcad#1644:
    /// `a point is 12 mm off the sketch plane`).
    pub off_plane_said: Vec<String>,
    /// 3D relations and dimensions that the plane holds by itself (On
    /// Sketch Plane, normals along z, offsets along z) or that have no
    /// planar form (a spline's driven length), left out; and the fixes and
    /// in-plane fixes that became fixed entities.
    pub dropped: Vec<ConstraintUid>,
    /// Attachments and reference curves that became linked projections.
    pub projected: Vec<String>,
    /// What changes on the way: a fit-point spline's free end becomes a
    /// natural one.
    pub notes: Vec<String>,
    /// Reference curves in the plane that bound profiles (the planar
    /// view's, not its entities), with their planar curves.
    pub reference_curves: Vec<(EntityUid, Curve2)>,
}

/// The planar view of an evaluated 3D sketch: what lies in its sketch
/// plane, relations of what does not left out. The model finds the
/// profile regions in it.
pub fn planar_view(def: &SpatialSketchDef, output: &SpatialSketchOutput) -> Flat {
    flatten(def, output, &BTreeMap::new(), false).unwrap_or_else(|_| Flat {
        def: SketchDef::default(),
        solved: Solved::default(),
        off_plane: Vec::new(),
        off_plane_said: Vec::new(),
        dropped: Vec::new(),
        projected: Vec::new(),
        notes: Vec::new(),
        reference_curves: Vec::new(),
    })
}

/// The planar sketch a 3D sketch becomes in place (3D Sketch unchecked):
/// refused naming what lies out of the sketch plane. `attached` gives, for
/// each attachment to a curve's start, end or middle, that curve in local
/// coordinates, projected onto the plane (the projection it becomes).
pub fn planar_from_spatial(
    def: &SpatialSketchDef,
    output: &SpatialSketchOutput,
    attached: &BTreeMap<ConstraintUid, SpatialReferenceCurve>,
) -> Result<Flat, String> {
    let flat = flatten(def, output, attached, true)?;
    if !flat.off_plane.is_empty() {
        // The first in words, and how many more (mitcad#1644); agents read
        // each with its ids.
        let first = flat.off_plane_said.first().cloned().unwrap_or_default();
        let said = match flat.off_plane.len() {
            1 => format!("it is not all in its sketch plane: {first}"),
            n => format!(
                "it is not all in its sketch plane: {first} (and {} more)",
                n - 1
            ),
        };
        return Err(crate::said(
            said,
            format!(
                "it is not all in its sketch plane: {}",
                flat.off_plane.join("; ")
            ),
        ));
    }
    // A point on a face (mitcad#1549): on a planar face in the sketch plane
    // it is the plane's own (left out); on any other face a 2D sketch has
    // no such relation.
    let records: Vec<_> = def
        .face_records()
        .filter(|(id, _, _)| !coplanar_face(output, *id))
        .collect();
    let on_faces: Vec<String> = records
        .iter()
        .map(|(id, point, source)| format!("{id} keeps p{} on {source}", point.0))
        .collect();
    if !on_faces.is_empty() {
        // The faces are its places (mitcad#1644).
        let places: Vec<_> = records.iter().filter_map(|(_, _, s)| s.place()).collect();
        return Err(said_at(
            "a 2D sketch keeps a point on a face only where the face is a plane in its sketch \
             plane; Detach the point first",
            format!(
                "{}: a 2D sketch keeps a point on a face only where the face is a plane in its \
                 sketch plane; Detach it first",
                on_faces.join("; ")
            ),
            places,
        ));
    }
    Ok(flat)
}

/// Whether the surface of the On Face relation `id` is a plane that is the
/// sketch plane (its normal along z, through z = 0).
fn coplanar_face(output: &SpatialSketchOutput, id: ConstraintUid) -> bool {
    matches!(
        output.surfaces.get(&id),
        Some(crate::spatial::SpatialSurface::Plane { origin, normal })
            if origin[2].abs() <= PLANE_TOLERANCE
                && normal[0].abs() <= 1e-9
                && normal[1].abs() <= 1e-9
    )
}

/// A projection record of a 3D sketch's source (an attachment's or a
/// reference curve's) with `entities`; None for the origin.
fn projection_of(source: &SpatialSource, entities: Vec<Ref>) -> Option<Projection> {
    let (source, body, link) = match source {
        SpatialSource::Origin => return None,
        SpatialSource::Vertex { body, vertex, link } => (
            ProjectionSource::Topo(TopoName::Vertex(vertex.clone())),
            Some(*body),
            link.clone(),
        ),
        SpatialSource::Edge {
            body, edge, link, ..
        } => (
            ProjectionSource::Topo(TopoName::Edge((**edge).clone())),
            Some(*body),
            link.clone(),
        ),
        SpatialSource::Sketch { entity, link, .. } => {
            (ProjectionSource::Sketch(*entity), None, link.clone())
        }
        SpatialSource::Curve { feature, link, .. } => {
            (ProjectionSource::Curve(*feature), None, link.clone())
        }
        SpatialSource::Face { body, face, link } => (
            ProjectionSource::Topo(TopoName::Face((**face).clone())),
            Some(*body),
            link.clone(),
        ),
    };
    Some(Projection {
        source,
        body,
        entities,
        link,
        by_geometry: true,
        section: false,
        chain: Vec::new(),
        after: None,
        outline: false,
    })
}

/// The flattening shared by the view (`strict` false: what is out of the
/// plane left out) and the conversion (`strict`: it is named, and
/// attachments and reference curves become projections).
fn flatten<P: Clone>(
    def: &SpatialSketchDef<P>,
    output: &SpatialSketchOutput,
    attached: &BTreeMap<ConstraintUid, SpatialReferenceCurve>,
    strict: bool,
) -> Result<Flat<P>, String> {
    let solved = output
        .solved
        .as_ref()
        .ok_or("it has not solved: fix it first")?;
    let records = &def.planar;
    let at = |id: EntityUid| -> Option<[f64; 3]> {
        solved.points.get(&id).copied().or_else(|| def.position(id))
    };
    let mut flat = Flat {
        def: SketchDef {
            plane: def.support.clone(),
            plane_link: records.plane_link.clone(),
            frame: def.frame.clone(),
            texts: Vec::new(),
            patterns: records.patterns.clone(),
            offsets: records.offsets.clone(),
            text_values: records.text_values.clone(),
            placement: records.placement.clone(),
            projections: records.projections.clone(),
            ..SketchDef::default()
        },
        solved: Solved::default(),
        off_plane: Vec::new(),
        off_plane_said: Vec::new(),
        dropped: Vec::new(),
        projected: Vec::new(),
        notes: Vec::new(),
        reference_curves: Vec::new(),
    };

    // Points.
    let mut in_plane: BTreeSet<EntityUid> = BTreeSet::new();
    for p in &def.points {
        let Some(xyz) = at(p.id) else {
            continue;
        };
        if xyz[2].abs() > PLANE_TOLERANCE {
            off_plane(
                &mut flat,
                format!("a point is {} mm off the sketch plane", round_mm(xyz[2])),
                format!(
                    "p{} is {} mm off the sketch plane",
                    p.id.0,
                    round_mm(xyz[2])
                ),
            );
            continue;
        }
        let mut e = Entity::new(
            p.id,
            EntityKind::Point {
                at: [xyz[0], xyz[1]],
            },
        );
        def.flag(&mut e, false);
        flat.solved.points.insert(p.id, [xyz[0], xyz[1]]);
        flat.def.entities.push(e);
        in_plane.insert(p.id);
    }

    // Curves.
    let mut references: BTreeMap<EntityUid, Curve2> = BTreeMap::new();
    for c in &def.curves {
        let name = format!("c{}", c.id.0);
        if let SpatialCurveKind::Reference { .. } = c.kind {
            let geometry = output.references.get(&c.id).and_then(reference_curve3);
            match geometry {
                Some(curve) if curve_in_plane(&curve) => {
                    if let Some(planar) = planar_curve(&curve) {
                        references.insert(c.id, planar);
                    }
                }
                _ => off_plane(
                    &mut flat,
                    "a reference curve is not in the sketch plane",
                    format!("{name}, a reference curve, is not in the sketch plane"),
                ),
            }
            continue;
        }
        // A curve of a point out of the plane: the point is named.
        if c.points().iter().any(|p| !in_plane.contains(p)) {
            continue;
        }
        let round = match solved.curves.get(&c.id) {
            Some(SolvedCurve::Round { normal, radius }) => Some((*normal, *radius)),
            _ => def.stored_round(c.id),
        };
        // An ellipse whose plane is not the sketch plane (another frame
        // plane's, a rotated plane's: mitcad#1661), its points in it or not.
        if let Some(n) = c.ellipse_normal()
            && (n[0].abs() > 1e-9 || n[1].abs() > 1e-9)
        {
            off_plane(
                &mut flat,
                "an ellipse does not lie in the sketch plane (its normal is not the plane's)",
                format!("{name}, an ellipse, does not lie in the sketch plane"),
            );
            continue;
        }
        let mut flipped = false;
        if let Some((normal, _)) = round {
            if normal[0].abs() > 1e-9 || normal[1].abs() > 1e-9 {
                off_plane(
                    &mut flat,
                    tilted(c),
                    format!(
                        "{name}, {} {}, does not lie in the sketch plane (its normal is not the \
                         plane's)",
                        article(c.type_name()),
                        c.type_name()
                    ),
                );
                continue;
            }
            flipped = normal[2] < 0.0;
        }
        if let SpatialCurveKind::FittedSpline { tangents, ends, .. } = &c.kind {
            if tangents
                .iter()
                .flatten()
                .any(|t| t[2].abs() > PLANE_TOLERANCE * (1.0 + t[0].hypot(t[1])))
            {
                off_plane(
                    &mut flat,
                    "a spline's end tangent leaves the sketch plane",
                    format!("{name}'s end tangent leaves the sketch plane"),
                );
                continue;
            }
            if ends.contains(&crate::spatial::SpatialSplineEnd::Free) && strict {
                flat.notes.push(crate::said(
                    "a spline's free end becomes a natural one (a 2D fit-point spline's ends are \
                     natural unless a tangency holds them)",
                    format!(
                        "{name}'s free end becomes a natural one (a 2D fit-point spline's ends \
                         are natural unless a tangency holds them)"
                    ),
                ));
            }
        }
        let geometry = output.local_curve(c.id);
        if let Some(curve) = &geometry
            && !curve_in_plane(curve)
        {
            off_plane(
                &mut flat,
                lies_off(c),
                format!(
                    "{name}, {} {}, does not lie in the sketch plane",
                    article(c.type_name()),
                    c.type_name()
                ),
            );
            continue;
        }
        let centerline = records.centerlines.contains(&c.id);
        let Some(mut kind) = planar_kind(c, flipped, centerline) else {
            continue;
        };
        if let (EntityKind::Circle { radius, .. }, Some((_, r))) = (&mut kind, round) {
            *radius = r;
        }
        if let Some(curve) = geometry.as_ref().and_then(planar_curve) {
            flat.solved.curves.insert(c.id, curve);
        }
        match (&kind, round) {
            (EntityKind::Circle { .. }, Some((_, r))) => {
                flat.solved.radii.insert(c.id, r);
            }
            (
                EntityKind::Ellipse { minor_radius, .. }
                | EntityKind::EllipticalArc { minor_radius, .. },
                _,
            ) => {
                flat.solved.radii.insert(c.id, *minor_radius);
            }
            _ => {}
        }
        let mut e = Entity::new(c.id, kind);
        def.flag(&mut e, c.construction);
        flat.def.entities.push(e);
        in_plane.insert(c.id);
    }
    // The derived offsets' entities: computed in the plane after the solve.
    for e in &records.derived {
        flat.def.entities.push(e.clone());
        in_plane.insert(e.id);
    }

    // Texts.
    for t in &def.texts {
        // A text on a rotated plane (mitcad#1661) stands across it.
        if t.frame.is_some() {
            off_plane(
                &mut flat,
                "a text stands on a rotated plane, across the sketch plane",
                format!(
                    "t{} is on a rotated plane, across the sketch plane",
                    t.planar.id.0
                ),
            );
            continue;
        }
        // A text on another frame plane (mitcad#1550) stands across it.
        if t.normal != SpatialAxis::Z {
            off_plane(
                &mut flat,
                "a text stands on another frame plane, across the sketch plane",
                format!(
                    "t{} is on the {} plane, across the sketch plane",
                    t.planar.id.0,
                    t.normal.plane_name()
                ),
            );
            continue;
        }
        if t.z.abs() > PLANE_TOLERANCE {
            off_plane(
                &mut flat,
                format!("a text is {} mm off the sketch plane", round_mm(t.z)),
                format!(
                    "t{} is {} mm off the sketch plane",
                    t.planar.id.0,
                    round_mm(t.z)
                ),
            );
            continue;
        }
        if t.planar.frame.iter().all(|p| in_plane.contains(p)) {
            flat.def.texts.push(t.planar.clone());
            in_plane.insert(t.planar.id);
        }
    }

    // The fixes as planar fixed entities.
    let mut fixed: BTreeSet<EntityUid> = BTreeSet::new();
    // New ids for what projections need, after every id the sketch uses.
    let mut next_entity = def
        .next_entity()
        .0
        .max(records.derived_ids().map(|id| id.0 + 1).max().unwrap_or(1));
    let mut fresh = || {
        let id = EntityUid(next_entity);
        next_entity += 1;
        id
    };
    let all_in = |refs: &[Ref], in_plane: &BTreeSet<EntityUid>| {
        refs.iter().all(|r| in_plane.contains(&r.uid()))
    };

    // Reference curves: in the view they bound profiles where the planar
    // sketch's did; converted they become linked projections of their
    // sources, keeping their ids.
    for (id, curve) in &references {
        let Some(c) = def.curve(*id) else {
            continue;
        };
        let SpatialCurveKind::Reference { source, .. } = &c.kind else {
            continue;
        };
        if !strict {
            let construction = if records.references.contains(&Ref::Curve(*id)) {
                records.construction_references.contains(&Ref::Curve(*id))
            } else {
                c.construction
            };
            if !construction {
                flat.reference_curves.push((*id, curve.clone()));
            }
            continue;
        }
        let mut made = project::entities(&[Projected::Curve(curve.clone())], &mut fresh);
        // The curve keeps the reference curve's id.
        let made_curve = made.iter().find(|e| !e.is_point()).map(|e| e.id);
        if let Some(made_curve) = made_curve {
            for e in made.iter_mut().filter(|e| e.id == made_curve) {
                e.id = *id;
            }
        }
        for e in &mut made {
            e.fixed = true;
            e.reference = true;
            e.construction = c.construction;
        }
        let refs: Vec<Ref> = made.iter().map(Entity::as_ref).collect();
        for e in made {
            in_plane.insert(e.id);
            flat.def.entities.push(e);
        }
        if let Some(projection) = projection_of(source, refs) {
            flat.def.projections.push(projection);
        }
        flat.projected.push(format!("c{}", id.0));
    }

    // Relations (a point on a reference curve that became a projection
    // keeps its relation by its id, on the projected curve).
    let inactive = records.inactive_ids();
    for c in &def.constraints {
        let refs: Vec<Ref> = c
            .kind
            .points()
            .into_iter()
            .map(Ref::Point)
            .chain(c.kind.curves().into_iter().map(Ref::Curve))
            .collect();
        if !all_in(&refs, &in_plane) {
            continue;
        }
        let kind = match &c.kind {
            SpatialConstraintKind::Coincident { a, b } => ConstraintKind::Coincident {
                point: *a,
                entity: Ref::Point(*b),
            },
            SpatialConstraintKind::Fix { point, .. } => {
                fixed.insert(*point);
                flat.dropped.push(c.id);
                continue;
            }
            SpatialConstraintKind::InPlaneFixed { entity } => {
                fixed.insert(entity.uid());
                flat.dropped.push(c.id);
                continue;
            }
            SpatialConstraintKind::Attach { point, source, .. } => {
                if !strict {
                    continue;
                }
                let target = output.targets.get(&c.id).copied().unwrap_or([0.0; 3]);
                if target[2].abs() > PLANE_TOLERANCE {
                    off_plane(
                        &mut flat,
                        format!(
                            "an attachment holds a point {} mm off the sketch plane",
                            round_mm(target[2])
                        ),
                        format!(
                            "{} attaches p{} to {source}, {} mm off the sketch plane",
                            c.id,
                            point.0,
                            round_mm(target[2])
                        ),
                    );
                    continue;
                }
                // A curve's start, end or middle: the curve as the sketch's
                // normal sees it (a point when it runs along the normal).
                let seen: Option<Vec<Projected>> = match source.at() {
                    Some(_) => attached
                        .get(&c.id)
                        .and_then(reference_curve3)
                        .map(|curve| project::project(&[curve], &SketchFrame::XY)),
                    None => None,
                };
                let mut whole = source.clone();
                if let SpatialSource::Edge { at, .. }
                | SpatialSource::Sketch { at, .. }
                | SpatialSource::Curve { at, .. } = &mut whole
                {
                    *at = None;
                }
                let seen_point = match seen.as_deref() {
                    Some([Projected::Point(p)]) => Some(*p),
                    _ => None,
                };
                match (source, source.at(), seen_point) {
                    (SpatialSource::Origin, _, _) => {
                        fixed.insert(*point);
                        flat.dropped.push(c.id);
                    }
                    (_, Some(which), None) => {
                        // The curve projected, the point on its end or at
                        // its middle.
                        let curve = match seen.as_deref() {
                            Some([Projected::Curve(curve)]) => Some(curve.clone()),
                            _ => None,
                        };
                        let Some(curve) = curve else {
                            off_plane(
                                &mut flat,
                                "an attachment's source curve was not found",
                                format!(
                                    "{} attaches p{} to {source}, whose curve was not found",
                                    c.id, point.0
                                ),
                            );
                            continue;
                        };
                        let mut made = project::entities(&[Projected::Curve(curve)], &mut fresh);
                        for e in &mut made {
                            e.fixed = true;
                            e.reference = true;
                            e.construction = true;
                        }
                        let Some(made_curve) = made.iter().find(|e| !e.is_point()).cloned() else {
                            continue;
                        };
                        let refs: Vec<Ref> = made.iter().map(Entity::as_ref).collect();
                        for e in made {
                            in_plane.insert(e.id);
                            flat.def.entities.push(e);
                        }
                        if let Some(projection) = projection_of(&whole, refs) {
                            flat.def.projections.push(projection);
                        }
                        let kind = match (which, made_curve.kind.ends()) {
                            (SpatialCurvePoint::Middle, _) | (_, None) => {
                                ConstraintKind::Midpoint {
                                    point: *point,
                                    curve: made_curve.id,
                                }
                            }
                            (SpatialCurvePoint::Start, Some((s, _))) => {
                                ConstraintKind::Coincident {
                                    point: *point,
                                    entity: Ref::Point(s),
                                }
                            }
                            (SpatialCurvePoint::End, Some((_, e))) => ConstraintKind::Coincident {
                                point: *point,
                                entity: Ref::Point(e),
                            },
                        };
                        flat.def.constraints.push(Constraint::new(c.id, kind));
                        flat.projected.push(c.id.to_string());
                    }
                    (_, _, _) => {
                        // A vertex, another sketch's point, or a curve seen
                        // as a point along the normal: projected as a point,
                        // this one on it.
                        let made = fresh();
                        let mut e = Entity::new(
                            made,
                            EntityKind::Point {
                                at: [target[0], target[1]],
                            },
                        );
                        e.fixed = true;
                        e.reference = true;
                        flat.def.entities.push(e);
                        in_plane.insert(made);
                        if let Some(projection) = projection_of(&whole, vec![Ref::Point(made)]) {
                            flat.def.projections.push(projection);
                        }
                        flat.def.constraints.push(Constraint::new(
                            c.id,
                            ConstraintKind::Coincident {
                                point: *point,
                                entity: Ref::Point(made),
                            },
                        ));
                        flat.projected.push(c.id.to_string());
                    }
                }
                continue;
            }
            SpatialConstraintKind::Align { line, axis, .. } => match axis {
                SpatialAxis::X => ConstraintKind::Horizontal { line: *line },
                SpatialAxis::Y => ConstraintKind::Vertical { line: *line },
                SpatialAxis::Z => {
                    off_plane(
                        &mut flat,
                        "an alignment keeps a line along the sketch's normal",
                        format!("{} keeps c{} along z", c.id, line.0),
                    );
                    continue;
                }
            },
            SpatialConstraintKind::PointOn { point, curve } => ConstraintKind::Coincident {
                point: *point,
                entity: Ref::Curve(*curve),
            },
            SpatialConstraintKind::Midpoint { point, curve } => ConstraintKind::Midpoint {
                point: *point,
                curve: *curve,
            },
            SpatialConstraintKind::Tangent { a, b, at, .. } => ConstraintKind::Tangent {
                a: *a,
                b: *b,
                at: Some(*at),
            },
            SpatialConstraintKind::Equal { a, b } => ConstraintKind::Equal { a: *a, b: *b },
            SpatialConstraintKind::NormalAlign { curve, axis, .. } => {
                if *axis != SpatialAxis::Z {
                    off_plane(
                        &mut flat,
                        "a normal alignment keeps an arc's or circle's normal across the sketch's \
                         normal",
                        format!("{} keeps c{}'s normal along {}", c.id, curve.0, axis.name()),
                    );
                } else {
                    flat.dropped.push(c.id);
                }
                continue;
            }
            SpatialConstraintKind::OnSketchPlane { .. } => {
                flat.dropped.push(c.id);
                continue;
            }
            // On a planar face in the sketch plane: the plane's own; on
            // another, named by `planar_from_spatial`.
            SpatialConstraintKind::OnFace { .. } => {
                if coplanar_face(output, c.id) {
                    flat.dropped.push(c.id);
                }
                continue;
            }
            SpatialConstraintKind::InPlane {
                relation,
                normal: SpatialAxis::Z,
            } => relation.clone(),
            // A plane-bound shape's plane (mitcad#1550) is the sketch's own
            // in a planar sketch; another plane's relations go with it.
            SpatialConstraintKind::InPlane { .. } | SpatialConstraintKind::PlaneBound { .. } => {
                flat.dropped.push(c.id);
                continue;
            }
            // A rotated plane's (mitcad#1661) go with it as another frame
            // plane's do; whether its shape lies in the sketch plane is
            // its geometry's to say.
            SpatialConstraintKind::InFrame { .. }
            | SpatialConstraintKind::FrameBound { .. }
            | SpatialConstraintKind::AlignAlong { .. } => {
                flat.dropped.push(c.id);
                continue;
            }
            SpatialConstraintKind::NormalAlong { curve, direction } => {
                if direction[0].abs() > 1e-9 || direction[1].abs() > 1e-9 {
                    off_plane(
                        &mut flat,
                        "a circle's normal is kept across the sketch's normal",
                        format!(
                            "{} keeps c{}'s normal across the sketch's normal",
                            c.id, curve.0
                        ),
                    );
                } else {
                    flat.dropped.push(c.id);
                }
                continue;
            }
            // Directions in the plane (mitcad#1661): two lines keep their
            // planar relation; a normal of an arc or circle in the plane is
            // the sketch's normal, so the relation goes with the plane.
            SpatialConstraintKind::Parallel { a, b } => {
                if lines(def, &[*a, *b]) {
                    ConstraintKind::Parallel { a: *a, b: *b }
                } else {
                    flat.dropped.push(c.id);
                    continue;
                }
            }
            SpatialConstraintKind::Perpendicular { a, b } => {
                if lines(def, &[*a, *b]) {
                    ConstraintKind::Perpendicular { a: *a, b: *b }
                } else {
                    flat.dropped.push(c.id);
                    continue;
                }
            }
            SpatialConstraintKind::Concentric { a, b, .. } => {
                ConstraintKind::Concentric { a: *a, b: *b }
            }
            SpatialConstraintKind::Curvature { a, b, at, .. } => ConstraintKind::Smooth {
                a: *a,
                b: *b,
                at: Some(*at),
            },
            // On the sketch plane itself: the plane's own.
            SpatialConstraintKind::OnPlane {
                plane: SpatialPlaneRef::Frame(SpatialAxis::Z),
                ..
            } => {
                flat.dropped.push(c.id);
                continue;
            }
            SpatialConstraintKind::OnPlane { .. } => {
                off_plane(
                    &mut flat,
                    "points are kept on another plane than the sketch plane",
                    format!("{} keeps points on another plane", c.id),
                );
                continue;
            }
            SpatialConstraintKind::Symmetric { .. } => {
                off_plane(
                    &mut flat,
                    "a symmetry about a plane has no form in a 2D sketch",
                    format!("{} is a symmetry about a plane", c.id),
                );
                continue;
            }
        };
        let mut planar = Constraint::new(c.id, kind);
        if inactive.contains(&c.id) {
            planar.active = false;
            planar.inactive_reason = records
                .inactive
                .iter()
                .find(|i| i.id == c.id)
                .and_then(|i| i.reason.clone());
        }
        flat.def.constraints.push(planar);
    }

    // Dimensions.
    for d in &def.dimensions {
        let refs: Vec<Ref> = d
            .kind
            .points()
            .into_iter()
            .map(Ref::Point)
            .chain(d.kind.curves().into_iter().map(Ref::Curve))
            .collect();
        if !all_in(&refs, &in_plane) {
            continue;
        }
        let kind = match &d.kind {
            SpatialDimensionKind::Length { line } => DimensionKind::Length { line: *line },
            SpatialDimensionKind::Offset { a, b, axis } => match axis {
                SpatialAxis::X => DimensionKind::HorizontalDistance {
                    a: *a,
                    b: *b,
                    signed: true,
                },
                SpatialAxis::Y => DimensionKind::VerticalDistance {
                    a: *a,
                    b: *b,
                    signed: true,
                },
                SpatialAxis::Z => {
                    flat.dropped.push(d.id);
                    continue;
                }
            },
            SpatialDimensionKind::Radius { curve } => DimensionKind::Radius { curve: *curve },
            SpatialDimensionKind::Diameter { curve } => DimensionKind::Diameter { curve: *curve },
            SpatialDimensionKind::ArcLength { curve } => DimensionKind::ArcLength { arc: *curve },
            SpatialDimensionKind::SplineLength { .. }
            | SpatialDimensionKind::TangentMagnitude { .. }
            | SpatialDimensionKind::OffsetAlong { .. } => {
                flat.dropped.push(d.id);
                continue;
            }
            SpatialDimensionKind::Angle { a, b } => {
                if lines(def, &[*a, *b]) {
                    DimensionKind::Angle {
                        a: *a,
                        b: *b,
                        reverse_a: None,
                        reverse_b: None,
                    }
                } else {
                    flat.dropped.push(d.id);
                    continue;
                }
            }
            SpatialDimensionKind::InPlane {
                dimension,
                normal: SpatialAxis::Z,
            } => dimension.clone(),
            SpatialDimensionKind::InPlane { .. } => {
                flat.dropped.push(d.id);
                continue;
            }
            SpatialDimensionKind::MajorRadius { curve } => {
                DimensionKind::MajorRadius { ellipse: *curve }
            }
            SpatialDimensionKind::MinorRadius { curve } => {
                DimensionKind::MinorRadius { ellipse: *curve }
            }
        };
        let mut planar = Dimension::new(d.id, kind, d.value.clone());
        // A driven dimension keeps its reference parameter (mitcad#1661).
        planar.reference = d.reference.clone();
        planar.text = records.labels.iter().find(|l| l.id == d.id).map(|l| l.at);
        if let Some(i) = records.inactive.iter().find(|i| i.id == d.id) {
            planar.active = false;
            planar.inactive_reason = i.reason.clone();
        }
        flat.def.dimensions.push(planar);
    }

    for e in flat.def.entities.iter_mut() {
        if fixed.contains(&e.id) {
            e.fixed = true;
        }
    }
    // Kept projections and patterns name only what is in the plane.
    if !strict {
        flat.def
            .projections
            .retain(|p| p.entities.iter().all(|r| in_plane.contains(&r.uid())));
        flat.def.patterns.retain(|p| {
            p.entities.iter().all(|r| in_plane.contains(&r.uid()))
                && p.copies
                    .iter()
                    .all(|c| c.entities.values().all(|r| in_plane.contains(&r.uid())))
        });
        flat.def.offsets.retain(|o| {
            o.curves
                .iter()
                .chain(o.results.iter())
                .all(|id| in_plane.contains(id))
        });
    }
    Ok(flat)
}

/// Notes what lies out of the plane: `said` in plain words for a person,
/// `named` with its ids for agents (mitcad#1644).
/// Whether every curve of `ids` is a line of `def` (mitcad#1661).
fn lines<P>(def: &SpatialSketchDef<P>, ids: &[EntityUid]) -> bool {
    ids.iter()
        .all(|id| def.curve(*id).is_some_and(|c| c.as_line().is_some()))
}

fn off_plane<P>(flat: &mut Flat<P>, said: impl Into<String>, named: String) {
    flat.off_plane_said.push(said.into());
    flat.off_plane.push(named);
}

/// That a curve does not lie in the sketch plane, one whole message per
/// kind (mitcad#1644).
fn lies_off(c: &SpatialCurve) -> &'static str {
    match c.kind {
        SpatialCurveKind::Line { .. } => "a line does not lie in the sketch plane",
        SpatialCurveKind::Arc { .. } => "an arc does not lie in the sketch plane",
        SpatialCurveKind::Circle { .. } => "a circle does not lie in the sketch plane",
        SpatialCurveKind::Spline { .. } | SpatialCurveKind::FittedSpline { .. } => {
            "a spline does not lie in the sketch plane"
        }
        SpatialCurveKind::Ellipse { .. } => "an ellipse does not lie in the sketch plane",
        SpatialCurveKind::EllipticalArc { .. } => {
            "an elliptical arc does not lie in the sketch plane"
        }
        SpatialCurveKind::Reference { .. } => "a reference curve is not in the sketch plane",
    }
}

/// That an arc's or circle's plane is not the sketch plane (mitcad#1644).
fn tilted(c: &SpatialCurve) -> &'static str {
    match c.kind {
        SpatialCurveKind::Circle { .. } => {
            "a circle does not lie in the sketch plane (its normal is not the plane's)"
        }
        _ => "an arc does not lie in the sketch plane (its normal is not the plane's)",
    }
}

/// `a` or `an` before a curve's type name.
fn article(name: &str) -> &'static str {
    if name.starts_with(['a', 'e', 'i', 'o', 'u']) {
        "an"
    } else {
        "a"
    }
}

/// A length for messages: to a thousandth of a millimetre.
fn round_mm(v: f64) -> String {
    let rounded = (v * 1000.0).round() / 1000.0;
    if rounded.abs() < 1e-12 {
        format!("{v:.1e}")
    } else {
        format!("{rounded}")
    }
}
