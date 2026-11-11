// SPDX-License-Identifier: MIT
//! Editing a 3D sketch (mitcad#1521): the operations on its definition.
//! An edit works on a copy of the definition with a copy of the
//! document's parameters (a new driving dimension gets a parameter owned by
//! the sketch); the document then solves the result with the support and
//! the attachments' sources found, and takes the whole edit or nothing
//! (`mitcad_model::Document::edit_spatial_sketch`): points, curves,
//! relations, dimensions, parameters and ids alike.
//!
//! Points given by coordinates start there and are fixed by nothing. A
//! segment of a route ([`SegmentRequest`]) is one edit with its new end
//! point, its attachments, its axis and its dimensions; a continued route
//! shares the end point's id. So is an arc ([`ArcRequest`]: Three-Point,
//! Centre or Tangent, with its new centre and end, attachments, `Keep
//! Plane`, radius and arc length) and a spline ([`SplineRequest`]: through
//! fit points or by control points, with its new points). Unfix removes a
//! point's Fix only, Detach its attachment only. Deleting a curve keeps the
//! points other curves, relations or texts still use (a fit-point spline's
//! hidden control points are the solver's and go with it); deleting a
//! point that curves or relations use needs them deleted with it.

use std::collections::BTreeSet;
use std::f64::consts::{PI, TAU};

use super::ConstraintUid;
use crate::edit::{KeptParameter, resolve_dimension_value};
use crate::ids::{EntityUid, FeatureUid};
use crate::parameters::{ParamId, Parameters};
use crate::spatial::{
    SpatialAxis, SpatialConstraintDef, SpatialConstraintKind, SpatialCurve, SpatialCurveKind,
    SpatialDimension, SpatialDimensionKind, SpatialDirection, SpatialFrame, SpatialPlane,
    SpatialPoint, SpatialSense, SpatialSketchDef, SpatialSketchStatus, SpatialSource,
    SpatialSplineEnd, SpatialTangentPull, SpatialText, check_curve,
};
use crate::values::ValueInput;
use crate::{ConstraintKind, Ref, SketchText, said};
// Refusals in words with the source's place, the ids for agents
// (mitcad#1644).
use mitcad_model_base::notice::{said_after, said_at};
use mitcad_solver::{SPATIAL_MIN_SEGMENT, circle_through};
use mitcad_vec3::{add, add_scaled, cross, distance, dot, norm, sub, unit};

/// A point of an edit: an existing point or a new one at local
/// coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SpatialPointInput {
    At([f64; 3]),
    Existing(EntityUid),
}

/// What an edit created and removed, and the sketch's status after it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SpatialEditReport {
    pub points: Vec<EntityUid>,
    pub curves: Vec<EntityUid>,
    /// Texts placed (mitcad#1550).
    pub texts: Vec<EntityUid>,
    pub constraints: Vec<ConstraintUid>,
    pub dimensions: Vec<ConstraintUid>,
    /// Parameters created for new driving dimensions.
    pub parameters: Vec<String>,
    /// Removed points, curves, relations and dimensions: `p3`, `c4`, `k5`.
    pub removed: Vec<String>,
    /// The status after the edit, from the document's solve.
    pub status: SpatialSketchStatus,
    /// The lines each fillet of the edit rounded and its arc (mitcad#1548):
    /// a Pipe's or Sweep's path that lists both lines takes the arc too.
    pub rounded: Vec<([EntityUid; 2], EntityUid)>,
    /// What the user may like to know about the edit: a dimension's own
    /// parameter kept as a user parameter (mitcad#1535).
    pub notes: Vec<String>,
    /// Own parameters of dimensions kept as user parameters because the
    /// dimension's new value uses them (mitcad#1535), as a planar sketch's
    /// edit reports them.
    pub kept: Vec<KeptParameter>,
}

/// One segment of a route: a line from `start` to `end` with what is asked
/// of it, made or refused together.
#[derive(Debug, Clone, PartialEq)]
pub struct SegmentRequest {
    pub start: SpatialPointInput,
    pub end: SpatialPointInput,
    pub construction: bool,
    /// Keep the line along a frame axis, pointing this way.
    pub axis: Option<(SpatialAxis, SpatialSense)>,
    /// A driving true length.
    pub length: Option<ValueInput>,
    /// Driving signed offsets from the start to the end along X, Y and Z;
    /// None leaves that coordinate free (no hidden relation).
    pub offsets: [Option<ValueInput>; 3],
    /// Attach the start to a source.
    pub start_source: Option<SpatialSource>,
    /// Attach the end to a source.
    pub end_source: Option<SpatialSource>,
}

impl SegmentRequest {
    /// A plain segment from `start` to `end`.
    pub fn new(start: SpatialPointInput, end: SpatialPointInput) -> Self {
        Self {
            start,
            end,
            construction: false,
            axis: None,
            length: None,
            offsets: [None, None, None],
            start_source: None,
            end_source: None,
        }
    }
}

/// What a segment made.
#[derive(Debug, Clone, PartialEq)]
pub struct SegmentMade {
    pub line: EntityUid,
    pub start: EntityUid,
    pub end: EntityUid,
    pub constraints: Vec<ConstraintUid>,
    pub dimensions: Vec<ConstraintUid>,
}

/// How an arc's centre and plane are found.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ArcPlacement {
    /// Three-Point Arc: from the start through `through` (not stored) to
    /// the end; the centre and the normal follow from the three points,
    /// which must not be on one line.
    ThreePoint { through: [f64; 3] },
    /// Centre Arc: around `center`, counter-clockwise about `normal` (made
    /// at right angles to the radius to the start) from the start; a new
    /// end is placed on the arc's circle.
    Centre {
        center: SpatialPointInput,
        normal: [f64; 3],
    },
    /// Tangent Arc: from an end of `curve` (a line, an arc or a spline;
    /// the arc's start is that end), tangent to it there, in the plane of
    /// its tangent and the end; a Tangent relation is added.
    Tangent { curve: EntityUid },
}

/// An arc with what is asked of it, made or refused together.
#[derive(Debug, Clone, PartialEq)]
pub struct ArcRequest {
    pub start: SpatialPointInput,
    pub end: SpatialPointInput,
    pub placement: ArcPlacement,
    pub construction: bool,
    /// Keep the normal along a frame axis (`Keep Plane`): a normal
    /// alignment.
    pub normal_axis: Option<(SpatialAxis, SpatialSense)>,
    /// A driving radius.
    pub radius: Option<ValueInput>,
    /// A driving arc length.
    pub arc_length: Option<ValueInput>,
    /// Attach the start to a source.
    pub start_source: Option<SpatialSource>,
    /// Attach the end to a source.
    pub end_source: Option<SpatialSource>,
}

impl ArcRequest {
    /// A plain arc from `start` to `end`.
    pub fn new(start: SpatialPointInput, end: SpatialPointInput, placement: ArcPlacement) -> Self {
        Self {
            start,
            end,
            placement,
            construction: false,
            normal_axis: None,
            radius: None,
            arc_length: None,
            start_source: None,
            end_source: None,
        }
    }
}

/// What an arc made.
#[derive(Debug, Clone, PartialEq)]
pub struct ArcMade {
    pub arc: EntityUid,
    pub center: EntityUid,
    pub start: EntityUid,
    pub end: EntityUid,
    pub constraints: Vec<ConstraintUid>,
    pub dimensions: Vec<ConstraintUid>,
}

/// A spline through (fit points) or by (control points) its points.
#[derive(Debug, Clone, PartialEq)]
pub struct SplineRequest {
    pub points: Vec<SpatialPointInput>,
    /// None: a fit-point spline (a cubic through the points); a degree: a
    /// control-point spline (clamped uniform knots, weights 1).
    pub degree: Option<u32>,
    /// A closed fit-point spline (periodic through its first point).
    pub closed: bool,
    /// An open fit-point spline's end conditions.
    pub ends: [SpatialSplineEnd; 2],
    pub construction: bool,
}

impl SplineRequest {
    /// An open fit-point spline with natural ends through `points`.
    pub fn fit(points: Vec<SpatialPointInput>) -> Self {
        Self {
            points,
            degree: None,
            closed: false,
            ends: [SpatialSplineEnd::Natural; 2],
            construction: false,
        }
    }

    /// A control-point spline of `degree` by `points`.
    pub fn control(points: Vec<SpatialPointInput>, degree: u32) -> Self {
        Self {
            degree: Some(degree),
            ..Self::fit(points)
        }
    }
}

/// What a spline made: the curve and its points in order.
#[derive(Debug, Clone, PartialEq)]
pub struct SplineMade {
    pub spline: EntityUid,
    pub points: Vec<EntityUid>,
}

/// How a plane-bound rectangle's corners are given (mitcad#1550), as the
/// planar rectangle tools take them, in the plane square to the request's
/// `normal` through the first point given. An existing point given as a
/// corner (or the centre) is that corner (or the centre).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RectangleCorners {
    /// Opposite corners; the sides along the plane's two frame axes.
    TwoPoint {
        a: SpatialPointInput,
        b: SpatialPointInput,
    },
    /// The centre and a corner; the sides along the plane's frame axes.
    Center {
        center: SpatialPointInput,
        corner: SpatialPointInput,
    },
    /// One side from `a` to `b`, the other through `c`'s distance from it
    /// (a place only).
    ThreePoint {
        a: SpatialPointInput,
        b: SpatialPointInput,
        c: [f64; 3],
    },
}

/// A rectangle on a plane (mitcad#1550).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RectangleRequest {
    pub corners: RectangleCorners,
    /// The frame axis square to its plane.
    pub normal: SpatialAxis,
    pub construction: bool,
}

/// A regular polygon on a plane (mitcad#1550), as the planar polygon tool
/// makes it: around `center` with `sides` sides; `vertex` its first corner
/// (inscribed), or the middle of its first side (circumscribed, a place
/// only).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PolygonRequest {
    pub center: SpatialPointInput,
    pub vertex: SpatialPointInput,
    pub sides: usize,
    pub inscribed: bool,
    pub normal: SpatialAxis,
    pub construction: bool,
}

/// What a plane-bound shape made (mitcad#1550): its outline's curves in
/// order, its corners, its centre point, its construction curves, its
/// relations.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ShapeMade {
    pub curves: Vec<EntityUid>,
    pub points: Vec<EntityUid>,
    pub center: Option<EntityUid>,
    pub construction: Vec<EntityUid>,
    pub constraints: Vec<ConstraintUid>,
    /// A rectangle's first side (from its first corner given along the
    /// plane's first axis, or to its second corner given) and second side.
    pub sides: Option<[EntityUid; 2]>,
}

/// A text on a plane (mitcad#1550): at `at` (its anchor, the plane through
/// it), or in a frame drawn from `at` to the opposite corner `frame` (a
/// construction rectangle along `angle`).
#[derive(Debug, Clone, PartialEq)]
pub struct TextRequest {
    pub text: SketchText,
    pub at: [f64; 3],
    pub frame: Option<[f64; 3]>,
    pub normal: SpatialAxis,
}

/// A corner a fillet rounds (mitcad#1548): two lines that share an end, or
/// a point where two ordinary lines end (found when its turn comes, so a
/// line rounded at its other end before is taken as it is then).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FilletCorner {
    Lines(EntityUid, EntityUid),
    Point(EntityUid),
}

/// What a fillet made at one corner (mitcad#1548).
#[derive(Debug, Clone, PartialEq)]
pub struct FilletMade {
    /// The point the two lines met at: it stays where their extensions
    /// meet, with its own relations and dimensions.
    pub corner: EntityUid,
    /// The two lines, trimmed back to the tangent points, ids kept.
    pub lines: [EntityUid; 2],
    /// The tangent arc, from its tangent point on the first line to the
    /// one on the second.
    pub arc: EntityUid,
    pub center: EntityUid,
    /// The tangent points on the first and on the second line.
    pub tangent_points: [EntityUid; 2],
    /// The construction lines from the tangent points to the corner: the
    /// first in line with the first line (a tangency), both of one length
    /// (Equal), so the corner stays where the lines' extensions meet.
    pub corner_lines: [EntityUid; 2],
    /// Construction lines from a line's other end to the corner, which took
    /// the line's Length, Equal, Point on Curve and Midpoint relations so
    /// that they keep their meaning.
    pub full_lines: Vec<EntityUid>,
    /// The relations added: the corner's two, the arc's two tangencies and
    /// an Equal to the first arc of the edit.
    pub constraints: Vec<ConstraintUid>,
    /// The radius dimension: the first fillet's of an edit; the others are
    /// Equal to it.
    pub dimension: Option<ConstraintUid>,
    /// The radius it was placed with (mm).
    pub radius: f64,
}

/// How a fillet's radius is held: by a radius dimension of a value, or
/// Equal to another arc of that radius.
#[derive(Debug, Clone, Copy)]
enum FilletSize<'v> {
    Radius(&'v ValueInput),
    EqualTo(EntityUid, f64),
}

/// The sine of the angle between two lines at a corner at or below which
/// they are taken as collinear there (straight on, or folded back): no
/// corner to round, as the planar fillet has it.
const FILLET_STRAIGHT: f64 = 1e-7;

/// An edit in progress.
pub struct SpatialEdit<'a> {
    pub def: SpatialSketchDef,
    params: &'a mut Parameters,
    owner: FeatureUid,
    owner_name: String,
    pub report: SpatialEditReport,
    /// The temporary soft goals of a drag: where points and fit-point
    /// splines' end tangents are pulled to by the solve that finishes the
    /// edit; never stored.
    goals: SpatialGoals,
}

/// The temporary soft goals of a drag (never stored): points pulled toward
/// local targets, and fit-point splines' end tangents toward wanted
/// tangents (mitcad#1526: a tangent handle's drag).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SpatialGoals {
    pub points: Vec<(EntityUid, [f64; 3])>,
    pub tangents: Vec<SpatialTangentPull>,
}

impl SpatialGoals {
    pub fn is_empty(&self) -> bool {
        self.points.is_empty() && self.tangents.is_empty()
    }
}

fn finite(at: [f64; 3]) -> bool {
    at.iter().all(|v| v.is_finite())
}

/// The corner (0 lower left, then counter-clockwise) of a rectangle along
/// its plane's axes that the planar `p` is, `other` lying across the
/// rectangle from it (as the planar rectangle tools number them).
fn corner_index(p: [f64; 2], other: [f64; 2]) -> usize {
    match (p[0] > other[0], p[1] > other[1]) {
        (false, false) => 0,
        (true, false) => 1,
        (true, true) => 2,
        (false, true) => 3,
    }
}

fn point_name(id: EntityUid) -> String {
    format!("p{}", id.0)
}

/// The refusal of a point that does not exist: in words for a person, its
/// id for agents (mitcad#1644).
fn no_point(id: EntityUid) -> String {
    said(
        "the point does not exist",
        format!("point {} does not exist", point_name(id)),
    )
}

fn curve_name(id: EntityUid) -> String {
    format!("c{}", id.0)
}

/// Millimetres in a message: at most three decimals, no trailing zeros.
fn mm(value: f64) -> String {
    let text = format!("{value:.3}");
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

impl<'a> SpatialEdit<'a> {
    pub fn new(
        def: SpatialSketchDef,
        params: &'a mut Parameters,
        owner: FeatureUid,
        owner_name: &str,
    ) -> Self {
        Self {
            def,
            params,
            owner,
            owner_name: owner_name.to_owned(),
            report: SpatialEditReport::default(),
            goals: SpatialGoals::default(),
        }
    }

    pub fn owner(&self) -> FeatureUid {
        self.owner
    }

    /// The parameters as the edit has them.
    pub fn params(&self) -> &Parameters {
        self.params
    }

    /// Where a point is now (local coordinates).
    pub fn position(&self, id: EntityUid) -> Result<[f64; 3], String> {
        self.def.position(id).ok_or_else(|| no_point(id))
    }

    fn require_curve(&self, id: EntityUid) -> Result<&SpatialCurve, String> {
        self.def.curve(id).ok_or_else(|| {
            said(
                "the curve does not exist",
                format!("curve {} does not exist", curve_name(id)),
            )
        })
    }

    /// A curve that is a line: its start and end points.
    fn require_line(&self, id: EntityUid) -> Result<[EntityUid; 2], String> {
        let curve = self.require_curve(id)?;
        curve.as_line().ok_or_else(|| {
            said(
                "the curve is not a line",
                format!("{} is a {}, not a line", curve_name(id), curve.type_name()),
            )
        })
    }

    /// Adds a free point at local coordinates (three degrees of freedom; it
    /// is fixed by nothing).
    pub fn add_point(&mut self, at: [f64; 3]) -> Result<EntityUid, String> {
        if !finite(at) {
            return Err("a coordinate is not finite".to_owned());
        }
        let id = self.def.next_entity();
        self.def.points.push(SpatialPoint { id, at });
        self.report.points.push(id);
        Ok(id)
    }

    /// An existing point, or a new one.
    pub fn point(&mut self, input: SpatialPointInput) -> Result<EntityUid, String> {
        match input {
            SpatialPointInput::At(at) => self.add_point(at),
            SpatialPointInput::Existing(id) => {
                self.position(id)?;
                Ok(id)
            }
        }
    }

    /// Moves a point's start position (no relation: the solve keeps it
    /// only as far as the relations allow).
    pub fn move_point(&mut self, id: EntityUid, at: [f64; 3]) -> Result<(), String> {
        if !finite(at) {
            return Err("a coordinate is not finite".to_owned());
        }
        let point = self
            .def
            .points
            .iter_mut()
            .find(|p| p.id == id)
            .ok_or_else(|| no_point(id))?;
        point.at = at;
        Ok(())
    }

    /// Adds a line between two points; a new point given where the other
    /// is (within the shortest line) is refused as a collapsed line.
    pub fn add_line(
        &mut self,
        start: SpatialPointInput,
        end: SpatialPointInput,
        construction: bool,
    ) -> Result<EntityUid, String> {
        let a = self.point(start)?;
        let b = self.point(end)?;
        if a == b {
            return Err("a line needs two distinct points".to_owned());
        }
        let (pa, pb) = (self.position(a)?, self.position(b)?);
        if mitcad_vec3::distance(pa, pb) <= SPATIAL_MIN_SEGMENT
            && (matches!(start, SpatialPointInput::At(_))
                || matches!(end, SpatialPointInput::At(_)))
        {
            return Err(format!(
                "the line would have no length: its points are at most {SPATIAL_MIN_SEGMENT} mm \
                 apart"
            ));
        }
        let id = self.def.next_entity();
        self.def
            .curves
            .push(SpatialCurve::line(id, a, b, construction));
        self.report.curves.push(id);
        Ok(id)
    }

    /// Makes a curve a construction curve, or an ordinary one.
    pub fn set_construction(&mut self, curve: EntityUid, construction: bool) -> Result<(), String> {
        self.require_curve(curve)?;
        if let Some(c) = self.def.curves.iter_mut().find(|c| c.id == curve) {
            c.construction = construction;
        }
        Ok(())
    }

    /// Adds a relation.
    pub fn add_constraint(&mut self, kind: SpatialConstraintKind) -> Result<ConstraintUid, String> {
        for p in kind.points() {
            self.position(p)?;
        }
        for c in kind.curves() {
            self.require_curve(c)?;
        }
        if let Some(l) = kind.line() {
            self.require_line(l)?;
        }
        match &kind {
            SpatialConstraintKind::Coincident { a, b } if a == b => {
                return Err("a point cannot coincide with itself".to_owned());
            }
            SpatialConstraintKind::Fix { at, .. } if !finite(*at) => {
                return Err("a coordinate is not finite".to_owned());
            }
            _ => {}
        }
        let id = self.def.next_constraint();
        self.def.constraints.push(SpatialConstraintDef { id, kind });
        self.report.constraints.push(id);
        Ok(id)
    }

    fn relations_of_kind(&self, point: EntityUid, kind: &str) -> Vec<ConstraintUid> {
        self.def
            .constraints
            .iter()
            .filter(|c| c.kind.type_name() == kind && c.kind.points() == [point])
            .map(|c| c.id)
            .collect()
    }

    /// Two separately made points at one place.
    pub fn coincident(&mut self, a: EntityUid, b: EntityUid) -> Result<ConstraintUid, String> {
        self.add_constraint(SpatialConstraintKind::Coincident { a, b })
    }

    /// Fixes a point where it is now (a deliberate anchor).
    pub fn fix(&mut self, point: EntityUid) -> Result<ConstraintUid, String> {
        let at = self.position(point)?;
        if !self.relations_of_kind(point, "fix").is_empty() {
            return Err(said(
                "the point is fixed already",
                format!("{} is fixed already", point_name(point)),
            ));
        }
        self.add_constraint(SpatialConstraintKind::Fix { point, at })
    }

    /// Removes a point's Fix, and nothing else: an attachment stays.
    pub fn unfix(&mut self, point: EntityUid) -> Result<Vec<ConstraintUid>, String> {
        self.position(point)?;
        let fixes = self.relations_of_kind(point, "fix");
        if fixes.is_empty() {
            return Err(said(
                "the point is not fixed",
                format!("{} is not fixed", point_name(point)),
            ));
        }
        for id in &fixes {
            self.remove_relation(*id)?;
        }
        Ok(fixes)
    }

    /// Attaches a point to a source, included in true 3D (mitcad#1543):
    /// to a source point (the origin, a vertex, the start, end or middle
    /// of an edge or of a curve, another sketch's point) by an attachment;
    /// onto a source curve (an edge, another sketch's curve, an
    /// Intersection Curve) by a Point on Curve relation on that curve's
    /// reference curve, made or reused ([`SpatialEdit::include`]); onto a
    /// face of a body by an On Face relation (mitcad#1549). The relation's
    /// id.
    pub fn attach(
        &mut self,
        point: EntityUid,
        source: SpatialSource,
    ) -> Result<ConstraintUid, String> {
        self.attach_as(point, source, false)
    }

    /// [`SpatialEdit::attach`], the source projected along the sketch's
    /// normal onto its plane when `projected` (Project) instead of included
    /// in true 3D.
    pub fn attach_as(
        &mut self,
        point: EntityUid,
        source: SpatialSource,
        projected: bool,
    ) -> Result<ConstraintUid, String> {
        self.position(point)?;
        if !self.relations_of_kind(point, "attach").is_empty() {
            return Err(said(
                "the point is attached already; detach it first",
                format!("{} is attached already; detach it first", point_name(point)),
            ));
        }
        // The source's vertex, edge or face is painted with a refusal
        // (mitcad#1644).
        let places = source.places();
        // On a face (mitcad#1549): an On Face relation, the face's own
        // record; a point may lie on several faces (on two, it slides along
        // where they meet).
        if source.is_face() {
            if projected {
                return Err(said_at(
                    "a point lies on a face in true 3D, not projected",
                    format!("{source} is a face: a point lies on it in true 3D, not projected"),
                    places,
                ));
            }
            let already = self.def.constraints.iter().any(|c| {
                matches!(&c.kind, SpatialConstraintKind::OnFace { point: p, source: s }
                    if *p == point && *s == source)
            });
            if already {
                return Err(said_at(
                    "the point is on that face already",
                    format!("{} is on {source} already", point_name(point)),
                    places,
                ));
            }
            return self.add_constraint(SpatialConstraintKind::OnFace { point, source });
        }
        if source.is_curve() {
            let curve = self.include(source, projected)?;
            if self
                .def
                .constraints
                .iter()
                .any(|c| c.kind.points() == [point] && c.kind.curves() == [curve])
            {
                return Err(said_at(
                    "the point is on that curve already",
                    format!("{} is on {} already", point_name(point), curve_name(curve)),
                    places,
                ));
            }
            return self.point_on(point, curve);
        }
        if !self.links_of(point).is_empty() {
            return Err(said(
                "the point is on a reference curve or a face; detach it first",
                format!(
                    "{} is on a reference curve or a face; detach it first",
                    point_name(point)
                ),
            ));
        }
        self.add_constraint(SpatialConstraintKind::Attach {
            point,
            source,
            projected,
        })
    }

    /// The reference curve of a source curve (mitcad#1543): the sketch's
    /// record of the same source made the same way (`projected`) when it
    /// has one, one record per source element and occurrence; else a new
    /// one. A source point is refused (a point is attached to it).
    pub fn include(&mut self, source: SpatialSource, projected: bool) -> Result<EntityUid, String> {
        if !source.is_curve() {
            // One whole message per kind (mitcad#1644).
            let said = if source.is_face() {
                "the source is a face, not a curve: put a point on it"
            } else {
                "the source is a point, not a curve: attach a point to it"
            };
            return Err(said_at(
                said,
                format!(
                    "{source} is a {}, not a curve: attach a point to it",
                    source.kind_name()
                ),
                source.places(),
            ));
        }
        if let Some(id) = self.def.reference_of(&source, projected) {
            return Ok(id);
        }
        self.add_curve(SpatialCurveKind::Reference { source, projected }, false)
    }

    /// A point's links to sources: its attachments and its relations
    /// (Point on Curve, Midpoint) with reference curves.
    fn links_of(&self, point: EntityUid) -> Vec<ConstraintUid> {
        let reference =
            |id: &EntityUid| self.def.curve(*id).is_some_and(SpatialCurve::is_reference);
        self.def
            .constraints
            .iter()
            .filter(|c| match &c.kind {
                SpatialConstraintKind::Attach { point: p, .. }
                | SpatialConstraintKind::OnFace { point: p, .. } => *p == point,
                SpatialConstraintKind::PointOn { point: p, curve }
                | SpatialConstraintKind::Midpoint { point: p, curve } => {
                    *p == point && reference(curve)
                }
                _ => false,
            })
            .map(|c| c.id)
            .collect()
    }

    /// Removes a point's links to sources, and nothing else: its
    /// attachment, its Point on Curve (and Midpoint) relations with
    /// reference curves, its On Face relations (mitcad#1549); a reference
    /// curve that nothing uses any more goes with them (mitcad#1543). The
    /// point stays where it is now, free as far as the other relations
    /// allow.
    pub fn detach(&mut self, point: EntityUid) -> Result<Vec<ConstraintUid>, String> {
        self.position(point)?;
        let links = self.links_of(point);
        if links.is_empty() {
            return Err(said(
                "the point is not attached",
                format!("{} is not attached", point_name(point)),
            ));
        }
        for id in &links {
            self.remove_relation(*id)?;
        }
        Ok(links)
    }

    /// Points a reference record at another source of the same kind
    /// (`Replace Reference`, mitcad#1543): an attachment (`k3`) at another
    /// point, an On Face relation (`k4`, mitcad#1549) at another face, a
    /// reference curve (`c7`) at another curve; what uses it stays.
    /// The solve that finishes the edit refuses it, with nothing changed,
    /// where the new source does not fit; the document checks its
    /// eligibility (earlier geometry, no link cycle) likewise.
    pub fn replace_attachment(
        &mut self,
        id: ConstraintUid,
        source: SpatialSource,
    ) -> Result<(), String> {
        let relation = self
            .def
            .constraints
            .iter_mut()
            .find(|c| c.id == id)
            .ok_or_else(|| {
                said(
                    "the relation does not exist",
                    format!("{id} does not exist"),
                )
            })?;
        match &mut relation.kind {
            SpatialConstraintKind::Attach { source: kept, .. } => {
                if source.is_curve() || source.is_face() {
                    return Err(said_at(
                        "an attachment's new source is a point (Replace Reference keeps the kind)",
                        format!(
                            "{source} is a {}: an attachment's new source is a point (Replace \
                             Reference keeps the kind)",
                            source.kind_name()
                        ),
                        source.places(),
                    ));
                }
                *kept = source;
            }
            SpatialConstraintKind::OnFace { source: kept, .. } => {
                if !source.is_face() {
                    return Err(said_at(
                        "an On Face relation's new source is a face (Replace Reference keeps the \
                         kind)",
                        format!(
                            "{source} is a {}: an On Face relation's new source is a face (Replace \
                             Reference keeps the kind)",
                            source.kind_name()
                        ),
                        source.places(),
                    ));
                }
                *kept = source;
            }
            other => {
                return Err(said(
                    "Replace Reference takes an attachment or a point on a face",
                    format!(
                        "{id} is a {} relation, not an attachment or a point on a face",
                        other.type_name()
                    ),
                ));
            }
        }
        Ok(())
    }

    /// [`SpatialEdit::replace_attachment`] for a reference curve: its new
    /// source is a curve that the sketch has no reference curve of yet.
    pub fn replace_reference_curve(
        &mut self,
        curve: EntityUid,
        source: SpatialSource,
    ) -> Result<(), String> {
        let found = self.require_curve(curve)?;
        let SpatialCurveKind::Reference { projected, .. } = found.kind else {
            return Err(said(
                "the curve is not a reference curve",
                format!(
                    "{} is a {}, not a reference curve",
                    curve_name(curve),
                    found.type_name()
                ),
            ));
        };
        if !source.is_curve() {
            return Err(said_at(
                "a reference curve's new source is a curve (Replace Reference keeps the kind)",
                format!(
                    "{source} is a {}: a reference curve's new source is a curve (Replace \
                     Reference keeps the kind)",
                    source.kind_name()
                ),
                source.places(),
            ));
        }
        if let Some(other) = self.def.reference_of(&source, projected)
            && other != curve
        {
            return Err(said_at(
                "the sketch has a reference curve of that source already",
                format!(
                    "{} is the reference curve of {source} already",
                    curve_name(other)
                ),
                source.places(),
            ));
        }
        if let Some(c) = self.def.curves.iter_mut().find(|c| c.id == curve) {
            c.kind = SpatialCurveKind::Reference { source, projected };
        }
        Ok(())
    }

    /// Keeps a line along a frame axis in a sense.
    pub fn align(
        &mut self,
        line: EntityUid,
        axis: SpatialAxis,
        sense: SpatialSense,
    ) -> Result<ConstraintUid, String> {
        self.add_constraint(SpatialConstraintKind::Align { line, axis, sense })
    }

    /// Adds a dimension: driven, or driving with a value (a number, a
    /// parameter name or an expression), or driving with the measured
    /// value when `value` is None.
    pub fn add_dimension(
        &mut self,
        kind: SpatialDimensionKind,
        value: Option<&ValueInput>,
        driven: bool,
    ) -> Result<ConstraintUid, String> {
        for p in kind.points() {
            self.position(p)?;
        }
        for c in kind.curves() {
            self.require_curve(c)?;
        }
        if let Some(l) = kind.line() {
            self.require_line(l)?;
        }
        if let SpatialDimensionKind::Offset { a, b, .. } = &kind
            && a == b
        {
            return Err("an offset needs two points".to_owned());
        }
        let id = self.def.next_constraint();
        let mut dimension: SpatialDimension<ParamId> = SpatialDimension::new(id, kind, None);
        if !driven {
            dimension.value = Some(self.value_param(&dimension, value, None)?);
        }
        self.def.dimensions.push(dimension);
        self.report.dimensions.push(id);
        Ok(id)
    }

    /// The parameter of a driving dimension's value: `value`, or the
    /// measured value; `old` is the dimension's own parameter.
    fn value_param(
        &mut self,
        dimension: &SpatialDimension<ParamId>,
        value: Option<&ValueInput>,
        old: Option<ParamId>,
    ) -> Result<ParamId, String> {
        let measured;
        let value = match value {
            Some(value) => value,
            None => {
                measured = ValueInput::Number(
                    self.def
                        .measure(&dimension.kind)
                        .or_else(|| self.def.measure_spline(&dimension.kind))
                        .ok_or("the dimension cannot be measured")?,
                );
                &measured
            }
        };
        let slot = dimension.slot();
        let comment = format!("{} {}", self.owner_name, dimension.kind.type_name());
        // A value that uses the dimension's own parameter keeps that
        // parameter as a user parameter, as a planar sketch's dimension
        // does (mitcad#1535, the same rule and code).
        let dimensions = &self.def.dimensions;
        let resolved = resolve_dimension_value(
            self.params,
            self.owner,
            dimension.id,
            &slot,
            &comment,
            value,
            old,
            |p| {
                dimensions
                    .iter()
                    .any(|other| other.id != dimension.id && other.value == Some(p))
            },
        )?;
        self.report.parameters.extend(resolved.created);
        if let Some((kept, note)) = resolved.kept {
            self.report.notes.push(note);
            self.report.kept.push(kept);
        }
        Ok(resolved.param)
    }

    fn dimension_index(&self, id: ConstraintUid) -> Result<usize, String> {
        self.def
            .dimensions
            .iter()
            .position(|d| d.id == id)
            .ok_or_else(|| {
                said(
                    "the dimension does not exist",
                    format!("dimension {id} does not exist"),
                )
            })
    }

    /// Sets a dimension's value; a driven one becomes driving.
    pub fn set_dimension_value(
        &mut self,
        id: ConstraintUid,
        value: &ValueInput,
    ) -> Result<(), String> {
        let i = self.dimension_index(id)?;
        let dimension = self.def.dimensions[i].clone();
        let param = self.value_param(&dimension, Some(value), dimension.value)?;
        self.def.dimensions[i].value = Some(param);
        Ok(())
    }

    /// Makes a dimension driven (it only measures; its parameter goes when
    /// nothing else uses it) or driving at its measured value.
    pub fn set_driven(&mut self, id: ConstraintUid, driven: bool) -> Result<(), String> {
        let i = self.dimension_index(id)?;
        if driven == self.def.dimensions[i].is_driven() {
            return Ok(());
        }
        if driven {
            self.def.dimensions[i].value = None;
        } else {
            let dimension = self.def.dimensions[i].clone();
            let param = self.value_param(&dimension, None, None)?;
            self.def.dimensions[i].value = Some(param);
        }
        Ok(())
    }

    /// Removes a relation or a dimension by its id; a reference curve it
    /// was the last use of goes with it (mitcad#1543: a reference record
    /// without users is deleted with the last deletion that used it).
    pub fn remove_relation(&mut self, id: ConstraintUid) -> Result<(), String> {
        let curves = match (self.def.constraint(id), self.def.dimension(id)) {
            (Some(c), _) => c.kind.curves(),
            (None, Some(d)) => d.kind.curves(),
            (None, None) => Vec::new(),
        };
        self.drop_relation(id)?;
        for curve in curves {
            let unused = self
                .def
                .curve(curve)
                .is_some_and(|c| c.is_reference() && self.def.reference_users(curve).is_empty());
            if unused {
                self.delete_curve(curve)?;
            }
        }
        Ok(())
    }

    /// Removes a relation or a dimension by its id, and nothing else.
    fn drop_relation(&mut self, id: ConstraintUid) -> Result<(), String> {
        let before = self.def.constraints.len() + self.def.dimensions.len();
        self.def.constraints.retain(|c| c.id != id);
        self.def.dimensions.retain(|d| d.id != id);
        if self.def.constraints.len() + self.def.dimensions.len() == before {
            return Err(said(
                "the relation or dimension does not exist",
                format!("{id} does not exist"),
            ));
        }
        self.report.removed.push(id.to_string());
        Ok(())
    }

    /// Whether anything still uses a point: a curve, a relation or a
    /// dimension (a Plane-Bound relation, which only keeps a shape's points
    /// in its plane, does not hold a point on its own).
    fn used(&self, point: EntityUid) -> bool {
        !self.def.curves_at(point).is_empty()
            || self
                .def
                .relations_of(point)
                .iter()
                .any(|k| !self.plane_bound(*k))
            || !self.def.texts_at(point).is_empty()
    }

    /// Whether `id` is a Plane-Bound relation (mitcad#1550).
    fn plane_bound(&self, id: ConstraintUid) -> bool {
        matches!(
            self.def.constraint(id).map(|c| &c.kind),
            Some(
                SpatialConstraintKind::PlaneBound { .. } | SpatialConstraintKind::FrameBound { .. }
            )
        )
    }

    /// Takes a point out of the Plane-Bound relations that list it; one
    /// left with fewer than two points goes.
    fn unbind(&mut self, point: EntityUid) -> Result<(), String> {
        let mut emptied = Vec::new();
        for c in &mut self.def.constraints {
            if let SpatialConstraintKind::PlaneBound { points, .. }
            | SpatialConstraintKind::FrameBound { points, .. } = &mut c.kind
                && points.contains(&point)
            {
                points.retain(|p| *p != point);
                if points.len() < 2 {
                    emptied.push(c.id);
                }
            }
        }
        for id in emptied {
            self.drop_relation(id)?;
        }
        Ok(())
    }

    /// Deletes a curve with the relations and dimensions of the curve
    /// itself; its points stay where another curve, a relation or a
    /// dimension still uses them.
    pub fn delete_curve(&mut self, id: EntityUid) -> Result<(), String> {
        let curve = self.require_curve(id)?.clone();
        for relation in self.def.relations_of(id) {
            self.drop_relation(relation)?;
        }
        self.def.curves.retain(|c| c.id != id);
        self.report.removed.push(curve_name(id));
        for point in curve.points() {
            if self.def.point(point).is_some() && !self.used(point) {
                self.unbind(point)?;
                self.def.points.retain(|p| p.id != point);
                self.report.removed.push(point_name(point));
            }
        }
        Ok(())
    }

    /// Deletes a point. One that curves, relations or dimensions use is
    /// deleted only `with_dependents`, together with them (the curves'
    /// other points stay where something still uses them); otherwise the
    /// error names them.
    pub fn delete_point(&mut self, id: EntityUid, with_dependents: bool) -> Result<(), String> {
        self.position(id)?;
        let curves = self.def.curves_at(id);
        self.unbind(id)?;
        let relations = self.def.relations_of(id);
        if !with_dependents && (!curves.is_empty() || !relations.is_empty()) {
            let users: Vec<String> = curves
                .iter()
                .map(|c| curve_name(*c))
                .chain(relations.iter().map(ToString::to_string))
                .collect();
            return Err(said(
                format!(
                    "the point is used by {} curves, relations or dimensions; delete them with it",
                    users.len()
                ),
                format!(
                    "{} is used by {}; delete them with it",
                    point_name(id),
                    users.join(", ")
                ),
            ));
        }
        for relation in relations {
            self.remove_relation(relation)?;
        }
        for curve in curves {
            self.delete_curve(curve)?;
        }
        if self.def.point(id).is_some() {
            self.def.points.retain(|p| p.id != id);
            self.report.removed.push(point_name(id));
        }
        Ok(())
    }

    /// Pulls points toward local targets in the solve that finishes the
    /// edit, as far as the relations allow (temporary soft goals: no
    /// relation is added).
    pub fn drag(&mut self, goals: &[(EntityUid, [f64; 3])]) -> Result<(), String> {
        for (point, target) in goals {
            self.position(*point)?;
            if !finite(*target) {
                return Err("a drag target is not finite".to_owned());
            }
        }
        self.goals.points.extend_from_slice(goals);
        Ok(())
    }

    /// Pulls an open fit-point spline's end tangent (`end` 0 at its first
    /// point, 1 at its last) toward `tangent` in the solve that finishes
    /// the edit (a soft goal: its direction where a tangency holds it
    /// stays; mitcad#1526).
    pub fn drag_tangent(
        &mut self,
        curve: EntityUid,
        end: usize,
        tangent: [f64; 3],
    ) -> Result<(), String> {
        self.def.tangent_handle(curve, end)?;
        if !finite(tangent) || norm(tangent) <= SPATIAL_MIN_SEGMENT {
            return Err("a dragged tangent is finite and longer than nothing".to_owned());
        }
        self.goals.tangents.push(SpatialTangentPull {
            curve,
            end,
            tangent,
        });
        Ok(())
    }

    /// Adds one segment of a route with everything asked of it: its new
    /// points, their attachments, the axis and the dimensions. The new end
    /// point starts where the requested relations put it (along the axis
    /// at the length, or at the offsets), so the solve starts on the
    /// requested branch.
    pub fn add_segment(&mut self, request: &SegmentRequest) -> Result<SegmentMade, String> {
        self.add_segment_in(request, None)
    }

    /// [`SpatialEdit::add_segment`] with the request's axis and offsets
    /// along a rotated placement plane's axes (`frame`: its x, y and
    /// normal for X, Y and Z, mitcad#1661): an Align Along relation and
    /// Offset Along dimensions in place of the frame axes' ones.
    pub fn add_segment_in(
        &mut self,
        request: &SegmentRequest,
        frame: Option<SpatialFrame>,
    ) -> Result<SegmentMade, String> {
        let start = self.point(request.start)?;
        let mut made = SegmentMade {
            line: EntityUid(0),
            start,
            end: start,
            constraints: Vec::new(),
            dimensions: Vec::new(),
        };
        if let Some(source) = &request.start_source {
            made.constraints.push(self.attach(start, source.clone())?);
        }
        let end = match request.end {
            SpatialPointInput::Existing(id) => self.point(SpatialPointInput::Existing(id))?,
            SpatialPointInput::At(at) => {
                let at = self.requested_end(start, at, request, frame)?;
                self.add_point(at)?
            }
        };
        made.end = end;
        made.line = self.add_line(
            SpatialPointInput::Existing(start),
            SpatialPointInput::Existing(end),
            request.construction,
        )?;
        if matches!(request.end, SpatialPointInput::At(_))
            && mitcad_vec3::distance(self.position(start)?, self.position(end)?)
                <= SPATIAL_MIN_SEGMENT
        {
            return Err(format!(
                "the segment would have no length: its end is at most {SPATIAL_MIN_SEGMENT} mm \
                 from its start"
            ));
        }
        if let Some(source) = &request.end_source {
            made.constraints.push(self.attach(end, source.clone())?);
        }
        match (request.axis, frame) {
            (Some((axis, sense)), None) => {
                made.constraints.push(self.align(made.line, axis, sense)?);
            }
            (Some((axis, sense)), Some(frame)) => {
                made.constraints
                    .push(self.add_constraint(SpatialConstraintKind::AlignAlong {
                        line: made.line,
                        direction: frame.axis(axis),
                        sense,
                    })?);
            }
            (None, _) => {}
        }
        if let Some(length) = &request.length {
            made.dimensions.push(self.add_dimension(
                SpatialDimensionKind::Length { line: made.line },
                Some(length),
                false,
            )?);
        }
        for (k, offset) in request.offsets.iter().enumerate() {
            let (Some(offset), Some(axis)) = (offset, SpatialAxis::of_index(k)) else {
                continue;
            };
            let kind = match frame {
                None => SpatialDimensionKind::Offset {
                    a: start,
                    b: end,
                    axis,
                },
                Some(frame) => SpatialDimensionKind::OffsetAlong {
                    a: start,
                    b: end,
                    direction: frame.axis(axis),
                },
            };
            made.dimensions
                .push(self.add_dimension(kind, Some(offset), false)?);
        }
        Ok(made)
    }

    /// Where a segment's new end starts: `at`, moved onto the requested
    /// axis and length or offsets when their values are known.
    fn requested_end(
        &mut self,
        start: EntityUid,
        at: [f64; 3],
        request: &SegmentRequest,
        frame: Option<SpatialFrame>,
    ) -> Result<[f64; 3], String> {
        if !finite(at) {
            return Err("a coordinate is not finite".to_owned());
        }
        // In a rotated plane's frame (mitcad#1661): coordinates along its
        // x, y and normal, and back.
        let into = |p: [f64; 3]| match frame {
            Some(f) => [dot(p, f.x), dot(p, f.y()), dot(p, f.normal)],
            None => p,
        };
        let back = |q: [f64; 3]| match frame {
            Some(f) => f.place([q[0], q[1]], q[2]),
            None => q,
        };
        let from = into(self.position(start)?);
        let at = into(at);
        let mut end = at;
        let known = |params: &Parameters, value: &ValueInput| match value {
            ValueInput::Number(v) => Some(*v),
            ValueInput::Name(name) => params.find(name).and_then(|id| params.value(id)),
        };
        if let (Some((axis, sense)), Some(length)) = (request.axis, &request.length)
            && let Some(length) = known(self.params, length)
        {
            end = from;
            end[axis.index()] += sense.sign() * length;
        } else if let Some((axis, sense)) = request.axis {
            // Along the axis through the start, as far as `at` is along it.
            let along = (at[axis.index()] - from[axis.index()]).abs();
            end = from;
            end[axis.index()] += sense.sign() * along;
        }
        for (k, offset) in request.offsets.iter().enumerate() {
            if let Some(v) = offset.as_ref().and_then(|o| known(self.params, o)) {
                end[k] = from[k] + v;
            }
        }
        Ok(back(end))
    }

    /// Adds a curve of any kind through existing points (no new points, no
    /// relations): the building block of conversions (mitcad#1544) and of
    /// definitions made elsewhere. Checked like a stored curve.
    pub fn add_curve(
        &mut self,
        kind: SpatialCurveKind,
        construction: bool,
    ) -> Result<EntityUid, String> {
        let id = self.def.next_entity();
        let curve = SpatialCurve::new(id, kind, construction);
        for p in curve.points() {
            self.position(p)?;
        }
        check_curve(&curve).map_err(|e| {
            said_after(
                "",
                &format!("{} {}: ", curve.type_name(), curve_name(id)),
                &e,
            )
        })?;
        self.def.curves.push(curve);
        self.report.curves.push(id);
        Ok(id)
    }

    /// Adds an arc with everything asked of it: its new points, their
    /// attachments, its normal's alignment and its dimensions (see
    /// [`ArcRequest`]). The new points start on the requested arc, so the
    /// solve starts on its branch.
    pub fn add_arc(&mut self, request: &ArcRequest) -> Result<ArcMade, String> {
        let start = self.point(request.start)?;
        let mut constraints = Vec::new();
        if let Some(source) = &request.start_source {
            constraints.push(self.attach(start, source.clone())?);
        }
        let s = self.position(start)?;
        let end_at = match request.end {
            SpatialPointInput::At(at) if !finite(at) => {
                return Err("a coordinate is not finite".to_owned());
            }
            SpatialPointInput::At(at) => at,
            SpatialPointInput::Existing(id) => self.position(id)?,
        };
        let (center, normal, end_at) = match &request.placement {
            ArcPlacement::ThreePoint { through } => {
                if !finite(*through) {
                    return Err("a coordinate is not finite".to_owned());
                }
                let circle = circle_through(s, *through, end_at).ok_or(
                    "the three points are on one line (or two of them at one place): no arc \
                     goes through them",
                )?;
                (self.add_point(circle.center)?, circle.normal, end_at)
            }
            ArcPlacement::Centre { center, normal } => {
                let center = self.point(*center)?;
                let c = self.position(center)?;
                let radial = sub(s, c);
                let radius = norm(radial);
                if radius <= SPATIAL_MIN_SEGMENT {
                    return Err(format!(
                        "the start is at the centre: the radius must be longer than \
                         {SPATIAL_MIN_SEGMENT} mm"
                    ));
                }
                // The normal across the radius to the start.
                let n = unit(*normal).ok_or("the normal is not a direction")?;
                let u = radial.map(|v| v / radius);
                let normal = unit(add_scaled(n, u, -dot(n, u)))
                    .ok_or("the normal is along the radius to the start: it has no plane")?;
                // A new end goes onto the arc's circle.
                let end_at = match request.end {
                    SpatialPointInput::At(at) => {
                        let d = sub(at, c);
                        let d = add_scaled(d, normal, -dot(d, normal));
                        let d =
                            unit(d).ok_or("the end is on the arc's axis: it has no direction")?;
                        add_scaled(c, d, radius)
                    }
                    SpatialPointInput::Existing(_) => end_at,
                };
                (center, normal, end_at)
            }
            ArcPlacement::Tangent { curve } => {
                self.require_curve(*curve)?;
                let t = self.def.outward_tangent(*curve, start).ok_or_else(|| {
                    said(
                        "a tangent arc starts at an end of a line, an arc or a spline",
                        format!(
                            "a tangent arc starts at an end of a line, an arc or a spline: p{} is \
                             not an end of {}",
                            start.0,
                            curve_name(*curve)
                        ),
                    )
                })?;
                let chord = sub(end_at, s);
                let n = cross(t, chord);
                if norm(n) <= SPATIAL_MIN_SEGMENT * norm(chord).max(1.0) {
                    return Err(
                        "the end is on the tangent: a tangent arc needs an end off its line"
                            .to_owned(),
                    );
                }
                let normal = n.map(|v| v / norm(n));
                // Toward the centre from the start, in the arc's plane.
                let w = cross(normal, t);
                let radius = dot(chord, chord) / (2.0 * dot(chord, w));
                (self.add_point(add_scaled(s, w, radius))?, normal, end_at)
            }
        };
        let end = match request.end {
            SpatialPointInput::At(_) => self.add_point(end_at)?,
            SpatialPointInput::Existing(id) => id,
        };
        if end == start || end == center || start == center {
            return Err(
                "an arc needs a centre, a start and an end that are three points".to_owned(),
            );
        }
        let c = self.position(center)?;
        let (s, e) = (self.position(start)?, self.position(end)?);
        if distance(s, c) <= SPATIAL_MIN_SEGMENT {
            return Err(format!(
                "the arc would have no radius (at most {SPATIAL_MIN_SEGMENT} mm)"
            ));
        }
        if distance(s, e) <= SPATIAL_MIN_SEGMENT {
            return Err(
                "the arc's ends meet: its sweep would be nothing or a full turn".to_owned(),
            );
        }
        let arc = self.add_curve(
            SpatialCurveKind::Arc {
                center,
                start,
                end,
                normal,
                plane: None,
            },
            request.construction,
        )?;
        if let ArcPlacement::Tangent { curve } = request.placement {
            constraints.push(self.tangent(curve, arc, start)?);
        }
        if let Some(source) = &request.end_source {
            constraints.push(self.attach(end, source.clone())?);
        }
        if let Some((axis, sense)) = request.normal_axis {
            constraints.push(self.align_normal(arc, axis, sense)?);
        }
        let mut dimensions = Vec::new();
        if let Some(radius) = &request.radius {
            dimensions.push(self.add_dimension(
                SpatialDimensionKind::Radius { curve: arc },
                Some(radius),
                false,
            )?);
        }
        if let Some(length) = &request.arc_length {
            dimensions.push(self.add_dimension(
                SpatialDimensionKind::ArcLength { curve: arc },
                Some(length),
                false,
            )?);
        }
        Ok(ArcMade {
            arc,
            center,
            start,
            end,
            constraints,
            dimensions,
        })
    }

    /// Adds a circle around a centre (a new point or an existing one) with
    /// a radius and a normal (free, or bound to a frame axis by `plane`).
    pub fn add_circle(
        &mut self,
        center: SpatialPointInput,
        radius: f64,
        normal: [f64; 3],
        plane: Option<SpatialDirection>,
        construction: bool,
    ) -> Result<EntityUid, String> {
        let center = self.point(center)?;
        let normal = match plane {
            Some(p) => p.unit(),
            None => unit(normal).ok_or("the normal is not a direction")?,
        };
        self.add_curve(
            SpatialCurveKind::Circle {
                center,
                radius,
                normal,
                plane,
            },
            construction,
        )
    }

    // Plane-bound shapes (mitcad#1550): drawn on the plane square to a
    // frame axis through their first point, made of points, lines and
    // construction circles with the planar tools' relations as in-plane
    // relations of that plane, their points kept in it by one Plane-Bound
    // relation.

    /// Where a point input is: a new point's place, an existing point's
    /// position.
    fn input_position(&self, input: SpatialPointInput) -> Result<[f64; 3], String> {
        match input {
            SpatialPointInput::At(at) if !finite(at) => {
                Err("a coordinate is not finite".to_owned())
            }
            SpatialPointInput::At(at) => Ok(at),
            SpatialPointInput::Existing(id) => self.position(id),
        }
    }

    /// A point of a shape: the existing point given, or a new one at
    /// planar `uv` of the plane square to `normal` at `elevation`.
    fn shape_point(
        &mut self,
        existing: Option<EntityUid>,
        normal: SpatialPlane,
        uv: [f64; 2],
        elevation: f64,
    ) -> Result<EntityUid, String> {
        match existing {
            Some(id) => Ok(id),
            None => self.add_point(normal.place(uv, elevation)),
        }
    }

    /// An in-plane relation of the plane square to `normal`, or of a
    /// rotated plane (mitcad#1661).
    fn in_plane(
        &mut self,
        relation: ConstraintKind,
        normal: SpatialPlane,
    ) -> Result<ConstraintUid, String> {
        match normal {
            SpatialPlane::Axis(normal) => {
                self.add_constraint(SpatialConstraintKind::InPlane { relation, normal })
            }
            SpatialPlane::Frame(frame) => {
                self.add_constraint(SpatialConstraintKind::InFrame { relation, frame })
            }
        }
    }

    /// A construction circle of a shape around `center` on its plane:
    /// bound to a frame axis, or with its normal held along a rotated
    /// plane's (mitcad#1661; `held` gets that relation).
    fn shape_circle(
        &mut self,
        center: SpatialPointInput,
        radius: f64,
        normal: SpatialPlane,
        held: &mut Vec<ConstraintUid>,
    ) -> Result<EntityUid, String> {
        match normal {
            SpatialPlane::Axis(axis) => {
                let plane = Some(SpatialDirection {
                    axis,
                    sense: SpatialSense::Positive,
                });
                self.add_circle(center, radius, [0.0, 0.0, 1.0], plane, true)
            }
            SpatialPlane::Frame(frame) => {
                let circle = self.add_circle(center, radius, frame.normal, None, true)?;
                held.push(self.add_constraint(SpatialConstraintKind::NormalAlong {
                    curve: circle,
                    direction: frame.normal,
                })?);
                Ok(circle)
            }
        }
    }

    /// Keeps a shape's points (each once) in one plane square to `normal`.
    fn bind_to_plane(
        &mut self,
        points: &[EntityUid],
        normal: SpatialPlane,
    ) -> Result<ConstraintUid, String> {
        let mut unique: Vec<EntityUid> = Vec::with_capacity(points.len());
        for p in points {
            if !unique.contains(p) {
                unique.push(*p);
            }
        }
        match normal {
            SpatialPlane::Axis(normal) => self.add_constraint(SpatialConstraintKind::PlaneBound {
                points: unique,
                normal,
            }),
            SpatialPlane::Frame(frame) => self.add_constraint(SpatialConstraintKind::FrameBound {
                points: unique,
                frame,
            }),
        }
    }

    /// Adds a rectangle on a plane (mitcad#1550), as the planar rectangle
    /// tools make it: four lines through four corners counter-clockwise
    /// about `normal`; a 2-point or centre rectangle's sides Horizontal
    /// and Vertical in its plane (along the plane's first and second frame
    /// axis), a 3-point one's Perpendicular; a centre rectangle's
    /// construction diagonal with the centre at its Midpoint. The corners
    /// (and the centre) are kept in the plane through the first point
    /// given.
    pub fn add_rectangle(&mut self, request: &RectangleRequest) -> Result<ShapeMade, String> {
        self.add_rectangle_on(request, request.normal.into())
    }

    /// [`SpatialEdit::add_rectangle`] on `normal`, in place of the
    /// request's frame axis: a rotated plane's (mitcad#1661), whose x and
    /// y are the planar tools' horizontal and vertical.
    pub fn add_rectangle_on(
        &mut self,
        request: &RectangleRequest,
        normal: SpatialPlane,
    ) -> Result<ShapeMade, String> {
        let existing = |input: SpatialPointInput| match input {
            SpatialPointInput::Existing(id) => Some(id),
            SpatialPointInput::At(_) => None,
        };
        let first = match request.corners {
            RectangleCorners::TwoPoint { a, .. } | RectangleCorners::ThreePoint { a, .. } => a,
            RectangleCorners::Center { center, .. } => center,
        };
        let elevation = normal.elevation(self.input_position(first)?);
        let mut joined: [Option<EntityUid>; 4] = [None; 4];
        // A 3-point rectangle drawn clockwise is numbered the other way.
        let mut reversed = false;
        let (corners, axis_aligned) = match request.corners {
            RectangleCorners::TwoPoint { a: pa, b: pb } => {
                let a = normal.planar(self.input_position(pa)?);
                let b = normal.planar(self.input_position(pb)?);
                let (x0, x1) = (a[0].min(b[0]), a[0].max(b[0]));
                let (y0, y1) = (a[1].min(b[1]), a[1].max(b[1]));
                joined[corner_index(a, b)] = existing(pa);
                if let Some(p) = existing(pb) {
                    joined[corner_index(b, a)] = Some(p);
                }
                ([[x0, y0], [x1, y0], [x1, y1], [x0, y1]], true)
            }
            RectangleCorners::Center {
                center: pc,
                corner: pk,
            } => {
                let c = normal.planar(self.input_position(pc)?);
                let k = normal.planar(self.input_position(pk)?);
                joined[corner_index(k, c)] = existing(pk);
                let (dx, dy) = ((k[0] - c[0]).abs(), (k[1] - c[1]).abs());
                (
                    [
                        [c[0] - dx, c[1] - dy],
                        [c[0] + dx, c[1] - dy],
                        [c[0] + dx, c[1] + dy],
                        [c[0] - dx, c[1] + dy],
                    ],
                    true,
                )
            }
            RectangleCorners::ThreePoint { a: pa, b: pb, c } => {
                if !finite(c) {
                    return Err("a coordinate is not finite".to_owned());
                }
                let a = normal.planar(self.input_position(pa)?);
                let b = normal.planar(self.input_position(pb)?);
                let c = normal.planar(c);
                let side = [b[0] - a[0], b[1] - a[1]];
                let length = side[0].hypot(side[1]);
                if length <= SPATIAL_MIN_SEGMENT {
                    return Err("the first side has no length".to_owned());
                }
                // Across the first side, counter-clockwise.
                let n = [-side[1] / length, side[0] / length];
                let along = (c[0] - b[0]) * n[0] + (c[1] - b[1]) * n[1];
                let h = [n[0] * along, n[1] * along];
                let corners = [a, b, [b[0] + h[0], b[1] + h[1]], [a[0] + h[0], a[1] + h[1]]];
                joined[0] = existing(pa);
                if side[0] * h[1] - side[1] * h[0] < 0.0 {
                    joined[3] = existing(pb);
                    reversed = true;
                    ([corners[0], corners[3], corners[2], corners[1]], false)
                } else {
                    joined[1] = existing(pb);
                    (corners, false)
                }
            }
        };
        let apart = |p: [f64; 2], q: [f64; 2]| (p[0] - q[0]).hypot(p[1] - q[1]);
        if apart(corners[0], corners[1]) <= SPATIAL_MIN_SEGMENT
            || apart(corners[1], corners[2]) <= SPATIAL_MIN_SEGMENT
        {
            return Err("the rectangle has no area".to_owned());
        }
        let mut made = ShapeMade::default();
        for (corner, at) in joined.into_iter().zip(corners) {
            let point = self.shape_point(corner, normal, at, elevation)?;
            made.points.push(point);
        }
        for i in 0..4 {
            let line = self.add_line(
                SpatialPointInput::Existing(made.points[i]),
                SpatialPointInput::Existing(made.points[(i + 1) % 4]),
                request.construction,
            )?;
            made.curves.push(line);
        }
        let lines = made.curves.clone();
        // The first side (along the plane's first axis, or from the first
        // corner given to the second) and the second side, which a typed
        // width and height dimension.
        made.sides = Some(if reversed {
            [lines[3], lines[0]]
        } else {
            [lines[0], lines[1]]
        });
        if axis_aligned {
            for (i, line) in lines.iter().enumerate() {
                let relation = if i.is_multiple_of(2) {
                    ConstraintKind::Horizontal { line: *line }
                } else {
                    ConstraintKind::Vertical { line: *line }
                };
                made.constraints.push(self.in_plane(relation, normal)?);
            }
        } else {
            for pair in lines.windows(2) {
                let relation = ConstraintKind::Perpendicular {
                    a: pair[0],
                    b: pair[1],
                };
                made.constraints.push(self.in_plane(relation, normal)?);
            }
        }
        let mut bound = made.points.clone();
        if let RectangleCorners::Center { center, .. } = request.corners {
            // A construction diagonal with the centre at its middle.
            let c = match center {
                SpatialPointInput::Existing(id) => id,
                SpatialPointInput::At(at) => {
                    self.add_point(normal.place(normal.planar(at), elevation))?
                }
            };
            let diagonal = self.add_line(
                SpatialPointInput::Existing(made.points[0]),
                SpatialPointInput::Existing(made.points[2]),
                true,
            )?;
            made.construction.push(diagonal);
            let relation = ConstraintKind::Midpoint {
                point: c,
                curve: diagonal,
            };
            made.constraints.push(self.in_plane(relation, normal)?);
            made.center = Some(c);
            bound.push(c);
        }
        made.constraints.push(self.bind_to_plane(&bound, normal)?);
        Ok(made)
    }

    /// Adds a regular polygon on a plane (mitcad#1550), as the planar
    /// polygon tool makes it: its sides through its corners
    /// counter-clockwise about `normal`, a construction circle around the
    /// centre point with every corner on it and the sides Equal
    /// (inscribed), or the sides Tangent to it and the corners on a second
    /// construction circle (circumscribed); the corners and the centre kept
    /// in the plane through the centre.
    pub fn add_polygon(&mut self, request: &PolygonRequest) -> Result<ShapeMade, String> {
        self.add_polygon_on(request, request.normal.into())
    }

    /// [`SpatialEdit::add_polygon`] on `normal`, in place of the
    /// request's frame axis: a rotated plane's (mitcad#1661).
    pub fn add_polygon_on(
        &mut self,
        request: &PolygonRequest,
        normal: SpatialPlane,
    ) -> Result<ShapeMade, String> {
        let sides = request.sides;
        let centre_at = self.input_position(request.center)?;
        let elevation = normal.elevation(centre_at);
        let center = normal.planar(centre_at);
        let vertex = normal.planar(self.input_position(request.vertex)?);
        let first_corner = match request.vertex {
            SpatialPointInput::Existing(id) => Some(id),
            SpatialPointInput::At(_) => None,
        };
        if !request.inscribed && first_corner.is_some() {
            return Err(
                "the vertex of a circumscribed polygon is the middle of a side, a place [x, y, \
                 z]: a point id is taken as the vertex of an inscribed polygon (its first corner)"
                    .to_owned(),
            );
        }
        if !(3..=256).contains(&sides) {
            return Err(format!("a polygon has 3 to 256 sides, got {sides}"));
        }
        let r = (vertex[0] - center[0]).hypot(vertex[1] - center[1]);
        if r <= SPATIAL_MIN_SEGMENT {
            return Err("the polygon has no size".to_owned());
        }
        let a0 = (vertex[1] - center[1]).atan2(vertex[0] - center[0]);
        let n = sides as f64;
        let (big, first) = if request.inscribed {
            (r, a0)
        } else {
            (r / (PI / n).cos(), a0 - PI / n)
        };
        let mut made = ShapeMade::default();
        for i in 0..sides {
            let a = first + TAU * i as f64 / n;
            let at = [center[0] + big * a.cos(), center[1] + big * a.sin()];
            let existing = if i == 0 { first_corner } else { None };
            let point = self.shape_point(existing, normal, at, elevation)?;
            made.points.push(point);
        }
        for i in 0..sides {
            let line = self.add_line(
                SpatialPointInput::Existing(made.points[i]),
                SpatialPointInput::Existing(made.points[(i + 1) % sides]),
                request.construction,
            )?;
            made.curves.push(line);
        }
        let c = match request.center {
            SpatialPointInput::Existing(id) => id,
            SpatialPointInput::At(_) => self.add_point(normal.place(center, elevation))?,
        };
        made.center = Some(c);
        let at_centre = SpatialPointInput::Existing(c);
        let circle = self.shape_circle(
            at_centre,
            if request.inscribed { big } else { r },
            normal,
            &mut made.constraints,
        )?;
        made.construction.push(circle);
        let (corners, lines) = (made.points.clone(), made.curves.clone());
        if request.inscribed {
            for p in &corners {
                let relation = ConstraintKind::Coincident {
                    point: *p,
                    entity: Ref::Curve(circle),
                };
                made.constraints.push(self.in_plane(relation, normal)?);
            }
            for pair in lines.windows(2) {
                let relation = ConstraintKind::Equal {
                    a: pair[0],
                    b: pair[1],
                };
                made.constraints.push(self.in_plane(relation, normal)?);
            }
        } else {
            // The corners' circle shares the centre point; each side is a
            // chord of it touching the inner circle.
            let around = self.shape_circle(at_centre, big, normal, &mut made.constraints)?;
            made.construction.push(around);
            for l in &lines {
                let relation = ConstraintKind::tangent(*l, circle);
                made.constraints.push(self.in_plane(relation, normal)?);
            }
            for p in &corners {
                let relation = ConstraintKind::Coincident {
                    point: *p,
                    entity: Ref::Curve(around),
                };
                made.constraints.push(self.in_plane(relation, normal)?);
            }
        }
        let mut bound = corners;
        bound.push(c);
        made.constraints.push(self.bind_to_plane(&bound, normal)?);
        Ok(made)
    }

    /// Adds an ellipse on a plane (mitcad#1550): around `center`, its major
    /// axis to `major` (a new major point goes into the plane through the
    /// centre square to `normal`), its minor radius solved from
    /// `minor_radius` on.
    pub fn add_ellipse(
        &mut self,
        center: SpatialPointInput,
        major: SpatialPointInput,
        minor_radius: f64,
        normal: SpatialAxis,
        construction: bool,
    ) -> Result<EntityUid, String> {
        self.add_ellipse_on(center, major, minor_radius, normal.into(), construction)
    }

    /// [`SpatialEdit::add_ellipse`] on any plane: a rotated plane's
    /// (mitcad#1661) keeps it in the plane through its centre square to the
    /// frame's normal.
    pub fn add_ellipse_on(
        &mut self,
        center: SpatialPointInput,
        major: SpatialPointInput,
        minor_radius: f64,
        normal: SpatialPlane,
        construction: bool,
    ) -> Result<EntityUid, String> {
        if !(minor_radius.is_finite() && minor_radius > SPATIAL_MIN_SEGMENT) {
            return Err(format!(
                "the minor radius must be longer than {SPATIAL_MIN_SEGMENT} mm, got {minor_radius}"
            ));
        }
        let c = self.point(center)?;
        let at_centre = self.position(c)?;
        let m = match major {
            SpatialPointInput::At(at) if finite(at) => {
                self.add_point(normal.place(normal.planar(at), normal.elevation(at_centre)))?
            }
            other => self.point(other)?,
        };
        let (pc, pm) = (normal.planar(at_centre), normal.planar(self.position(m)?));
        if c == m || (pm[0] - pc[0]).hypot(pm[1] - pc[1]) <= SPATIAL_MIN_SEGMENT {
            return Err("the major axis has no length in the ellipse's plane".to_owned());
        }
        self.add_curve(
            SpatialCurveKind::Ellipse {
                center: c,
                major: m,
                minor_radius,
                normal: match normal {
                    SpatialPlane::Axis(axis) => axis,
                    SpatialPlane::Frame(_) => SpatialAxis::Z,
                },
                frame: normal.frame(),
            },
            construction,
        )
    }

    /// Adds a text on a plane (mitcad#1550): its planar definition at the
    /// anchor's place in the plane square to `normal` through it, or in a
    /// frame (a construction rectangle along its angle, from the anchor to
    /// the opposite corner, kept in that plane) whose corner, x corner and
    /// y corner it follows.
    pub fn add_text(&mut self, request: &TextRequest) -> Result<EntityUid, String> {
        self.add_text_on(request, request.normal.into())
    }

    /// [`SpatialEdit::add_text`] on `normal`, in place of the request's
    /// frame axis: a rotated plane's (mitcad#1661), the text's anchor,
    /// angle and frame in the frame's x and y.
    pub fn add_text_on(
        &mut self,
        request: &TextRequest,
        normal: SpatialPlane,
    ) -> Result<EntityUid, String> {
        let mut text = request.text.clone();
        if text.text.is_empty() {
            return Err("the text is empty".to_owned());
        }
        if !(text.height.is_finite() && text.height > 0.0) {
            return Err(format!(
                "the text height must be positive, got {}",
                text.height
            ));
        }
        if !finite(request.at) {
            return Err("a coordinate is not finite".to_owned());
        }
        let elevation = normal.elevation(request.at);
        text.at = normal.planar(request.at);
        if let Some(corner) = request.frame {
            if !finite(corner) {
                return Err("a coordinate is not finite".to_owned());
            }
            let (s, c) = text.angle.sin_cos();
            let (u, v) = ([c, s], [-s, c]);
            let far = normal.planar(corner);
            let d = [far[0] - text.at[0], far[1] - text.at[1]];
            let (dx, dy) = (d[0] * u[0] + d[1] * u[1], d[0] * v[0] + d[1] * v[1]);
            let shift = |p: [f64; 2], k: f64, w: [f64; 2]| [p[0] + k * w[0], p[1] + k * w[1]];
            let o = shift(shift(text.at, dx.min(0.0), u), dy.min(0.0), v);
            let (w, h) = (dx.abs(), dy.abs());
            if w <= SPATIAL_MIN_SEGMENT || h <= SPATIAL_MIN_SEGMENT {
                return Err("the text frame has no area".to_owned());
            }
            let place = |p: [f64; 2]| normal.place(p, elevation);
            let b = shift(o, w, u);
            let corners = if text.angle == 0.0 {
                RectangleCorners::TwoPoint {
                    a: SpatialPointInput::At(place(o)),
                    b: SpatialPointInput::At(place(shift(b, h, v))),
                }
            } else {
                RectangleCorners::ThreePoint {
                    a: SpatialPointInput::At(place(o)),
                    b: SpatialPointInput::At(place(b)),
                    c: place(shift(b, h, v)),
                }
            };
            let made = self.add_rectangle_on(
                &RectangleRequest {
                    corners,
                    normal: request.normal,
                    construction: true,
                },
                normal,
            )?;
            text.frame = vec![made.points[0], made.points[1], made.points[3]];
        }
        let id = self.def.next_entity();
        text.id = id;
        self.def.texts.push(SpatialText {
            planar: text,
            z: elevation,
            normal: match normal {
                SpatialPlane::Axis(axis) => axis,
                SpatialPlane::Frame(_) => SpatialAxis::Z,
            },
            frame: normal.frame(),
        });
        self.report.texts.push(id);
        Ok(id)
    }

    /// Deletes a text (a framed text's construction rectangle stays).
    pub fn delete_text(&mut self, id: EntityUid) -> Result<(), String> {
        let before = self.def.texts.len();
        self.def.texts.retain(|t| t.planar.id != id);
        if self.def.texts.len() == before {
            return Err(said(
                "the text does not exist",
                format!("text t{} does not exist", id.0),
            ));
        }
        self.report.removed.push(format!("t{}", id.0));
        Ok(())
    }

    /// Adds a spline with its new points (see [`SplineRequest`]); its
    /// points must be apart from one another in their order.
    pub fn add_spline(&mut self, request: &SplineRequest) -> Result<SplineMade, String> {
        let mut points = Vec::with_capacity(request.points.len());
        for input in &request.points {
            points.push(self.point(*input)?);
        }
        let mut positions = Vec::with_capacity(points.len());
        for p in &points {
            positions.push(self.position(*p)?);
        }
        let mut spaced: Vec<[[f64; 3]; 2]> = positions.windows(2).map(|w| [w[0], w[1]]).collect();
        if request.closed
            && let (Some(first), Some(last)) = (positions.first(), positions.last())
        {
            spaced.push([*last, *first]);
        }
        if spaced
            .iter()
            .any(|[a, b]| distance(*a, *b) <= SPATIAL_MIN_SEGMENT)
        {
            return Err(format!(
                "a spline's successive points must be more than {SPATIAL_MIN_SEGMENT} mm apart"
            ));
        }
        let kind = match request.degree {
            None => SpatialCurveKind::FittedSpline {
                points: points.clone(),
                closed: request.closed,
                ends: request.ends,
                tangents: [None; 2],
            },
            Some(_) if request.closed => {
                return Err(
                    "a control-point spline is open: close a fit-point spline instead".to_owned(),
                );
            }
            Some(degree) => SpatialCurveKind::Spline {
                degree,
                control: points.clone(),
                weights: Vec::new(),
                knots: Vec::new(),
            },
        };
        let spline = self.add_curve(kind, request.construction)?;
        Ok(SplineMade { spline, points })
    }

    /// A point on a curve it does not define.
    pub fn point_on(
        &mut self,
        point: EntityUid,
        curve: EntityUid,
    ) -> Result<ConstraintUid, String> {
        self.add_constraint(SpatialConstraintKind::PointOn { point, curve })
    }

    /// A point at the middle of a line, an arc or a reference curve.
    pub fn midpoint(
        &mut self,
        point: EntityUid,
        curve: EntityUid,
    ) -> Result<ConstraintUid, String> {
        self.add_constraint(SpatialConstraintKind::Midpoint { point, curve })
    }

    /// Two curves tangent at `at` (an end of each arc and spline, or a
    /// point on an arc or circle), in the sense their tangents have there
    /// now (taken by the solve that finishes the edit, then kept).
    pub fn tangent(
        &mut self,
        a: EntityUid,
        b: EntityUid,
        at: EntityUid,
    ) -> Result<ConstraintUid, String> {
        if a == b {
            return Err("a tangency needs two curves".to_owned());
        }
        for c in [a, b] {
            let curve = self.require_curve(c)?;
            if curve.is_reference() {
                return Err(said(
                    "tangency takes lines, arcs, circles and splines, not reference curves",
                    format!(
                        "{} is a reference curve: tangency takes lines, arcs, circles and splines",
                        curve_name(c)
                    ),
                ));
            }
        }
        self.add_constraint(SpatialConstraintKind::Tangent {
            a,
            b,
            at,
            sense: None,
        })
    }

    /// Equal lengths of two lines or radii of two arcs or circles.
    pub fn equal(&mut self, a: EntityUid, b: EntityUid) -> Result<ConstraintUid, String> {
        let (ca, cb) = (self.require_curve(a)?, self.require_curve(b)?);
        let lines = ca.as_line().is_some() && cb.as_line().is_some();
        if a == b || !(lines || (ca.is_round() && cb.is_round())) {
            return Err("equal takes two lines or two arcs or circles".to_owned());
        }
        self.add_constraint(SpatialConstraintKind::Equal { a, b })
    }

    /// Keeps an arc's or circle's free normal along a frame axis in a
    /// sense.
    pub fn align_normal(
        &mut self,
        curve: EntityUid,
        axis: SpatialAxis,
        sense: SpatialSense,
    ) -> Result<ConstraintUid, String> {
        let c = self.require_curve(curve)?;
        if !c.is_round() || c.is_plane_bound() {
            return Err(said(
                "the curve is not an arc or circle with a free normal",
                format!(
                    "{} is not an arc or circle with a free normal",
                    curve_name(curve)
                ),
            ));
        }
        self.add_constraint(SpatialConstraintKind::NormalAlign { curve, axis, sense })
    }

    /// Releases a plane-bound arc's or circle's normal from its frame axis
    /// (`Release Plane`, mitcad#1522): the normal starts where the axis
    /// points and is free from then on (two degrees of freedom), and the
    /// curve's points no longer share one elevation by it.
    pub fn release_plane(&mut self, curve: EntityUid) -> Result<(), String> {
        let c = self.require_curve(curve)?;
        if !c.is_round() || !c.is_plane_bound() {
            return Err(said(
                "the curve is not an arc or circle bound to a plane",
                format!(
                    "{} is not an arc or circle bound to a plane",
                    curve_name(curve)
                ),
            ));
        }
        if let Some(c) = self.def.curves.iter_mut().find(|c| c.id == curve)
            && let SpatialCurveKind::Arc { normal, plane, .. }
            | SpatialCurveKind::Circle { normal, plane, .. } = &mut c.kind
            && let Some(direction) = plane.take()
        {
            *normal = direction.unit();
        }
        Ok(())
    }

    /// Keeps points on the sketch plane: the representatives of their
    /// elevation groups join the sketch's On Sketch Plane relation (made
    /// when it has none). Returns the relation.
    pub fn keep_on_sketch_plane(&mut self, points: &[EntityUid]) -> Result<ConstraintUid, String> {
        for p in points {
            self.position(*p)?;
        }
        let wanted = self.def.elevation_representatives(points);
        match self.def.on_sketch_plane().map(|c| c.id) {
            Some(id) => {
                let groups = self.def.elevation_groups();
                if let Some(SpatialConstraintDef {
                    kind: SpatialConstraintKind::OnSketchPlane { points: listed },
                    ..
                }) = self.def.constraints.iter_mut().find(|c| c.id == id)
                {
                    let held: BTreeSet<EntityUid> = listed
                        .iter()
                        .filter_map(|p| groups.get(p))
                        .copied()
                        .collect();
                    listed.extend(wanted.into_iter().filter(|p| !held.contains(p)));
                    listed.sort_unstable();
                }
                Ok(id)
            }
            None => self.add_constraint(SpatialConstraintKind::OnSketchPlane { points: wanted }),
        }
    }

    /// Releases points from the sketch plane: their elevation groups leave
    /// the On Sketch Plane relation (coincident points and a plane-bound
    /// curve's points lift together), which goes when it is empty. Returns
    /// the points taken out of it.
    pub fn release_from_plane(&mut self, points: &[EntityUid]) -> Result<Vec<EntityUid>, String> {
        for p in points {
            self.position(*p)?;
        }
        let Some(id) = self.def.on_sketch_plane().map(|c| c.id) else {
            return Err("no point is kept on the sketch plane".to_owned());
        };
        let groups = self.def.elevation_groups();
        let released: BTreeSet<EntityUid> = points
            .iter()
            .filter_map(|p| groups.get(p))
            .copied()
            .collect();
        let mut removed = Vec::new();
        let mut empty = false;
        if let Some(SpatialConstraintDef {
            kind: SpatialConstraintKind::OnSketchPlane { points: listed },
            ..
        }) = self.def.constraints.iter_mut().find(|c| c.id == id)
        {
            listed.retain(|p| {
                let keep = groups.get(p).is_none_or(|g| !released.contains(g));
                if !keep {
                    removed.push(*p);
                }
                keep
            });
            empty = listed.is_empty();
        }
        if removed.is_empty() {
            return Err("those points are not kept on the sketch plane".to_owned());
        }
        if empty {
            self.remove_relation(id)?;
        }
        Ok(removed)
    }

    /// The two lines a fillet at `corner` rounds: the two ordinary (not
    /// construction) lines that end there (mitcad#1548). Two construction
    /// lines are rounded by naming them.
    pub fn corner_lines(&self, corner: EntityUid) -> Result<[EntityUid; 2], String> {
        self.position(corner)?;
        let lines: Vec<EntityUid> = self
            .def
            .curves
            .iter()
            .filter(|c| !c.construction && c.as_line().is_some_and(|ends| ends.contains(&corner)))
            .map(|c| c.id)
            .collect();
        match lines.as_slice() {
            [a, b] => Ok([*a, *b]),
            [] | [_] => Err(said(
                "the point is no corner of two lines: a fillet rounds the point where two lines \
                 meet",
                format!(
                    "{} is no corner of two lines: a fillet rounds the point where two lines meet",
                    point_name(corner)
                ),
            )),
            more => Err(said(
                "the point is an end of more than two lines: name the two lines to round",
                format!(
                    "{} is an end of the lines {}: name the two lines to round",
                    point_name(corner),
                    more.iter()
                        .map(|c| curve_name(*c))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            )),
        }
    }

    /// Rounds corners with tangent arcs (mitcad#1548), in the order given:
    /// the first arc gets a driving radius dimension of `radius`, the
    /// others are Equal to it. See [`Self::fillet`].
    pub fn fillets(
        &mut self,
        corners: &[FilletCorner],
        radius: &ValueInput,
    ) -> Result<Vec<FilletMade>, String> {
        if corners.is_empty() {
            return Err(
                "a fillet needs a corner: two lines, or a point where two lines meet".to_owned(),
            );
        }
        let mut made: Vec<FilletMade> = Vec::with_capacity(corners.len());
        for corner in corners {
            let [a, b] = match *corner {
                FilletCorner::Lines(a, b) => [a, b],
                FilletCorner::Point(p) => self.corner_lines(p)?,
            };
            let size = match made.first() {
                None => FilletSize::Radius(radius),
                Some(first) => FilletSize::EqualTo(first.arc, first.radius),
            };
            made.push(self.round_corner(a, b, size)?);
        }
        Ok(made)
    }

    /// Rounds the corner where the lines `a` and `b` meet with a tangent arc
    /// of `radius` in their plane (mitcad#1548). Both lines are trimmed back
    /// to the tangent points, keeping their ids and their other relations
    /// (a direction, a tangency at the far end); the arc is tangent to both
    /// (two Tangent relations) with a driving radius dimension. The corner
    /// point stays where the lines' extensions meet, with its own relations
    /// and dimensions: two construction lines run from the tangent points to
    /// it, the first in line with the first line and both of one length.
    /// What measured a line as a segment to the corner (its Length, an
    /// Equal of lines, a point on it or at its middle) moves to a
    /// construction line from its other end to the corner, so it keeps its
    /// meaning. Refused when the lines do not share an end, are collinear
    /// there, or the radius does not fit either line.
    pub fn fillet(
        &mut self,
        a: EntityUid,
        b: EntityUid,
        radius: &ValueInput,
    ) -> Result<FilletMade, String> {
        self.round_corner(a, b, FilletSize::Radius(radius))
    }

    fn round_corner(
        &mut self,
        a: EntityUid,
        b: EntityUid,
        size: FilletSize<'_>,
    ) -> Result<FilletMade, String> {
        if a == b {
            return Err("a fillet rounds the corner of two lines: name two".to_owned());
        }
        let [a0, a1] = self.require_line(a)?;
        let [b0, b1] = self.require_line(b)?;
        let corner = [a0, a1]
            .into_iter()
            .find(|p| *p == b0 || *p == b1)
            .ok_or_else(|| {
                said(
                    "the lines do not share an end: a fillet rounds the corner where two lines \
                     meet",
                    format!(
                        "{} and {} do not share an end: a fillet rounds the corner where two \
                         lines meet",
                        curve_name(a),
                        curve_name(b)
                    ),
                )
            })?;
        let far_a = if a0 == corner { a1 } else { a0 };
        let far_b = if b0 == corner { b1 } else { b0 };
        let p = self.position(corner)?;
        let (pa, pb) = (self.position(far_a)?, self.position(far_b)?);
        let collinear = || {
            said(
                "the lines are collinear at their corner: there is no corner to round",
                format!(
                    "{} and {} are collinear at {}: there is no corner to round",
                    curve_name(a),
                    curve_name(b),
                    point_name(corner)
                ),
            )
        };
        let ua = unit(sub(pa, p)).ok_or_else(collinear)?;
        let ub = unit(sub(pb, p)).ok_or_else(collinear)?;
        if far_a == far_b || norm(cross(ua, ub)) <= FILLET_STRAIGHT {
            return Err(collinear());
        }
        let construction =
            self.require_curve(a)?.construction && self.require_curve(b)?.construction;
        // The new points start at the corner and are placed once the radius
        // is known (its value may be a parameter or an expression).
        let ta = self.add_point(p)?;
        let tb = self.add_point(p)?;
        let center = self.add_point(p)?;
        let normal = unit(cross(ua, ub)).ok_or_else(collinear)?;
        let arc = self.add_curve(
            SpatialCurveKind::Arc {
                center,
                start: ta,
                end: tb,
                normal,
                plane: None,
            },
            construction,
        )?;
        // The lines' segment relations go to construction lines to the
        // corner, then the lines end at the tangent points.
        let mut full_lines = Vec::new();
        for (line, to) in [(a, ta), (b, tb)] {
            if let Some(full) = self.keep_segment_relations(line)? {
                full_lines.push(full);
            }
            self.replace_line_end(line, corner, to);
        }
        let corner_lines = [
            self.add_curve(
                SpatialCurveKind::Line {
                    start: ta,
                    end: corner,
                },
                true,
            )?,
            self.add_curve(
                SpatialCurveKind::Line {
                    start: corner,
                    end: tb,
                },
                true,
            )?,
        ];
        let mut constraints = vec![
            self.tangent(a, corner_lines[0], ta)?,
            self.equal(corner_lines[0], corner_lines[1])?,
            self.tangent(a, arc, ta)?,
            self.tangent(arc, b, tb)?,
        ];
        let (dimension, r) = match size {
            FilletSize::Radius(value) => {
                let k = self.add_dimension(
                    SpatialDimensionKind::Radius { curve: arc },
                    Some(value),
                    false,
                )?;
                let r = self
                    .def
                    .dimension(k)
                    .and_then(|d| d.value)
                    .and_then(|id| self.params.value(id))
                    .unwrap_or(f64::NAN);
                (Some(k), r)
            }
            FilletSize::EqualTo(first, r) => {
                constraints.push(self.equal(arc, first)?);
                (None, r)
            }
        };
        let positive = r.is_finite() && r > SPATIAL_MIN_SEGMENT;
        if !positive {
            return Err(format!(
                "a fillet's radius must be longer than {SPATIAL_MIN_SEGMENT} mm"
            ));
        }
        // The circle of the radius tangent to both lines, in their plane.
        let half = 0.5 * dot(ua, ub).clamp(-1.0, 1.0).acos();
        let setback = r / half.tan();
        for (line, far) in [(a, pa), (b, pb)] {
            let length = distance(p, far);
            if setback >= length - SPATIAL_MIN_SEGMENT {
                return Err(said(
                    format!(
                        "a radius of {} mm does not fit a line: the arc would touch it {} mm from \
                         the corner, and the line is {} mm long",
                        mm(r),
                        mm(setback),
                        mm(length)
                    ),
                    format!(
                        "a radius of {} mm does not fit {}: the arc would touch it {} mm from \
                         the corner {}, and it is {} mm long",
                        mm(r),
                        curve_name(line),
                        mm(setback),
                        point_name(corner),
                        mm(length)
                    ),
                ));
            }
        }
        let bisector = unit(add(ua, ub)).ok_or_else(collinear)?;
        let c = add_scaled(p, bisector, r / half.sin());
        let (at_a, at_b) = (add_scaled(p, ua, setback), add_scaled(p, ub, setback));
        // Counter-clockwise about its normal from the first tangent point to
        // the second, the short way.
        let normal = unit(cross(sub(at_a, c), sub(at_b, c))).ok_or_else(collinear)?;
        self.move_point(ta, at_a)?;
        self.move_point(tb, at_b)?;
        self.move_point(center, c)?;
        if let Some(SpatialCurve {
            kind: SpatialCurveKind::Arc { normal: n, .. },
            ..
        }) = self.def.curves.iter_mut().find(|c| c.id == arc)
        {
            *n = normal;
        }
        self.report.rounded.push(([a, b], arc));
        Ok(FilletMade {
            corner,
            lines: [a, b],
            arc,
            center,
            tangent_points: [ta, tb],
            corner_lines,
            full_lines,
            constraints,
            dimension,
            radius: r,
        })
    }

    /// Moves what takes `line` as a segment between its ends (its Length,
    /// an Equal of lines, a point on it or at its middle) to a new
    /// construction line between the same points, before the line is
    /// trimmed (mitcad#1548): the new line, or None when nothing used it so.
    fn keep_segment_relations(&mut self, line: EntityUid) -> Result<Option<EntityUid>, String> {
        let measures = |kind: &SpatialConstraintKind| match kind {
            SpatialConstraintKind::Equal { a, b } => *a == line || *b == line,
            SpatialConstraintKind::PointOn { curve, .. }
            | SpatialConstraintKind::Midpoint { curve, .. } => *curve == line,
            _ => false,
        };
        let used = self
            .def
            .dimensions
            .iter()
            .any(|d| d.kind.line() == Some(line))
            || self.def.constraints.iter().any(|c| measures(&c.kind));
        if !used {
            return Ok(None);
        }
        let [start, end] = self.require_line(line)?;
        let full = self.add_curve(SpatialCurveKind::Line { start, end }, true)?;
        for d in &mut self.def.dimensions {
            if let SpatialDimensionKind::Length { line: l } = &mut d.kind
                && *l == line
            {
                *l = full;
            }
        }
        for c in &mut self.def.constraints {
            match &mut c.kind {
                SpatialConstraintKind::Equal { a, b } => {
                    for side in [a, b] {
                        if *side == line {
                            *side = full;
                        }
                    }
                }
                SpatialConstraintKind::PointOn { curve, .. }
                | SpatialConstraintKind::Midpoint { curve, .. }
                    if *curve == line =>
                {
                    *curve = full;
                }
                _ => {}
            }
        }
        Ok(Some(full))
    }

    /// A line's end `old` becomes `new`; its direction stays.
    fn replace_line_end(&mut self, line: EntityUid, old: EntityUid, new: EntityUid) {
        if let Some(SpatialCurve {
            kind: SpatialCurveKind::Line { start, end },
            ..
        }) = self.def.curves.iter_mut().find(|c| c.id == line)
        {
            if *start == old {
                *start = new;
            } else if *end == old {
                *end = new;
            }
        }
    }

    /// Checks the definition and the values. Returns the definition, the
    /// report and the drag's goals.
    pub fn finish(mut self) -> Result<(SpatialSketchDef, SpatialEditReport, SpatialGoals), String> {
        self.def.check_definition()?;
        // Driven dimensions get their reference parameters (mitcad#1661).
        let created = settle_spatial_references(
            &mut self.def,
            self.params,
            self.owner,
            &self.owner_name,
            None,
        )?;
        self.report.parameters.extend(created);
        let params = &*self.params;
        self.def
            .check_values(&|id| params.value(id).unwrap_or(f64::NAN))?;
        Ok((self.def, self.report, self.goals))
    }
}

/// Gives every driven dimension of a 3D sketch its read-only reference
/// parameter (mitcad#1661, as a planar sketch's, mitcad#231) and takes it
/// from driving ones: a reference the sketch owns is kept, a user parameter
/// the definition names as the reference becomes the sketch's, else the
/// next free `d<n>` is made, commented `3DSketch1 length (driven)`. With
/// `measured` (a solve's status), references take the values their
/// dimensions measure. A driving dimension of the same sketch may use the
/// reference in its expression: the sketch is then solved in stages
/// ([`crate::spatial`]). Returns the names of new parameters.
pub fn settle_spatial_references(
    def: &mut SpatialSketchDef,
    params: &mut Parameters,
    owner: FeatureUid,
    owner_name: &str,
    measured: Option<&SpatialSketchStatus>,
) -> Result<Vec<String>, String> {
    let mut taken = BTreeSet::new();
    let mut created = Vec::new();
    for d in &mut def.dimensions {
        if d.value.is_some() {
            d.reference = None;
            continue;
        }
        let slot = d.slot();
        let comment = format!(
            "{owner_name} {}{}",
            d.kind.type_name(),
            crate::edit::DRIVEN_SUFFIX
        );
        if let Some(id) = d.reference.filter(|id| {
            params.get(*id).is_some() && params.owner(*id).is_none() && !taken.contains(id)
        }) {
            crate::edit::adopt_reference(params, id, owner, &comment)
                .map_err(|e| format!("{slot}: {e}"))?;
        }
        let usable = |id: ParamId| {
            params.get(id).is_some() && params.owner(id) == Some(owner) && !taken.contains(&id)
        };
        let id = match d.reference.filter(|id| usable(*id)) {
            Some(id) => {
                params.set_kind(id, crate::expr::ParamKind::Reference);
                id
            }
            None => {
                let unit = match crate::parameters::slot_unit(&slot) {
                    crate::expr::Unit::MM => {
                        crate::expr::Unit::of_length(params.context().default_length_unit)
                    }
                    unit => unit,
                };
                let value = measured
                    .and_then(|s| s.measured.get(&d.id).copied())
                    .unwrap_or(0.0);
                let id = params
                    .add_reference(value, unit, &comment, owner)
                    .map_err(|e| format!("{slot}: {e}"))?;
                created.push(params.name(id));
                id
            }
        };
        taken.insert(id);
        d.reference = Some(id);
        if let Some(value) = measured.and_then(|s| s.measured.get(&d.id)) {
            let differs = params
                .value(id)
                .is_none_or(|v| v.to_bits() != value.to_bits());
            if differs {
                params
                    .set_measured(id, *value)
                    .map_err(|e| format!("{slot}: {e}"))?;
            }
        }
    }
    Ok(created)
}
