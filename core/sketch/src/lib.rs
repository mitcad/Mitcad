// SPDX-License-Identifier: MIT
//! The general sketch (`docs/architecture.md`): entities, geometric
//! constraints and dimensions in sketch plane coordinates (millimetres,
//! radians), solved by `mitcad-solver`, and the profile regions their curves
//! bound.
//!
//! - Entities ([`Entity`]): points, and curves that refer to points (lines,
//!   circles, arcs, ellipses, elliptical arcs, B-splines by control points,
//!   splines through fit points). Curves sharing a point are joined there.
//!   Points and curves share one id space per sketch: `p<n>` and `c<n>`.
//!   Flags: `construction` (left out of profiles), `fixed` (the solver does
//!   not move it), `reference` (projected model geometry), and for lines
//!   `centerline` (in profiles, unlike construction lines).
//! - Constraints ([`Constraint`]) and dimensions ([`Dimension`]) share the
//!   id space `k<n>`. A driving dimension's value is a parameter; a driven
//!   one only measures. Either can be inactive (mitcad#721): kept and shown,
//!   but left out of the solve until it is activated.
//! - Stored positions are the last solved ones and the start of every
//!   solve, so the same definition and values always give the same
//!   geometry (`solve.rs`).
//! - Profile regions (`regions.rs`): the non-construction curves are cut at
//!   their intersections into segments, and the faces of that planar
//!   arrangement, with their holes, are the regions. Segment and region
//!   keys (see [`mitcad_model_base::topo`]) name them from curve ids only.
//!
//! This crate sits below the model's core (mitcad#780), which re-exports it
//! as `mitcad_model::sketch`; the sketch feature's definition is [`def`]
//! (`mitcad_model::features::sketch`). What needs the document (the
//! feature's references and checks against the timeline, its evaluation,
//! the sources of linked projections) stays in the model.

// The base modules under the names the sketch code has always used.
use mitcad_expr as expr;
use mitcad_model_base::{
    datum, ids, kernel, occurrence, parameters, profile, topo, transform, values,
};

// The sketch feature's definition and evaluated output (mitcad#780).
pub mod def;
pub mod edit;
pub mod geometry;
pub mod intersect;
pub mod legacy;
pub mod modify;
pub mod project;
pub mod regions;
pub mod solve;
pub mod tools;
// A DXF drawing into a sketch (U6).
pub mod dxf;
// Patterns, offsets and text (P3, P4).
pub mod offset;
pub mod pattern;
pub mod text;
// Font files, shaping and both directions of text, SVG drawings
// (mitcad#473).
pub mod fonts;
pub mod svg;
// QR codes (mitcad#473).
pub mod code;
// Equal spacing (mitcad#221).
pub mod spacing;
// Duplicate and overlapping projected curves (mitcad#232).
pub mod cleanup;
// Projected curves that no longer match their source (mitcad#252).
pub mod stale;
// Points that become one (mitcad#259, mitcad#260).
pub mod points;
// Projected geometry into ordinary sketch geometry (mitcad#257).
pub mod convert;
// The sketch's own axes as entities for constraints and dimensions
// (mitcad#500).
pub mod axes;
// Copy, Cut and Paste of sketch geometry (mitcad#788).
pub mod clipboard;
// A sketch's place on its plane: offset, move, rotation and flip
// (mitcad#787, Redefine Sketch Plane).
pub mod placement;
// Items other Mitcad crates use; not part of this crate's API (mitcad#811).
pub mod internal;
// Insert Point on a spline: a fit point, or a control point by knot
// insertion (mitcad#836).
pub mod spline_points;
// Scaling a sketch, and the first dimension that scales it (mitcad#860).
pub mod scale;

#[cfg(test)]
mod tests;
// Equal spacing (mitcad#221).
#[cfg(test)]
mod spacing_tests;
// Regions of many loops (mitcad#678).
#[cfg(test)]
mod regions_tests;

use std::collections::BTreeMap;
use std::fmt;

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Value};

use crate::ids::BodyUid;
use crate::ids::{EntityUid, IdError};
use crate::topo::TopoName;
use mitcad_model_base::internal::ids::parse_number;

/// A constraint or dimension of a sketch, written `k<n>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ConstraintUid(pub u32);

impl fmt::Display for ConstraintUid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "k{}", self.0)
    }
}

impl std::str::FromStr for ConstraintUid {
    type Err = IdError;

    fn from_str(text: &str) -> Result<Self, IdError> {
        parse_number(text, "k")
            .map(ConstraintUid)
            .ok_or_else(|| IdError::new("constraint", text, "k<number>"))
    }
}

mitcad_model_base::internal::ids::string_serde!(ConstraintUid, "a constraint id like \"k3\"");

/// A reference to a point (`p<n>`) or a curve (`c<n>`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Ref {
    Point(EntityUid),
    Curve(EntityUid),
}

impl Ref {
    pub fn uid(self) -> EntityUid {
        match self {
            Self::Point(uid) | Self::Curve(uid) => uid,
        }
    }

    pub fn is_point(self) -> bool {
        matches!(self, Self::Point(_))
    }
}

impl fmt::Display for Ref {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Point(uid) => write!(f, "p{}", uid.0),
            Self::Curve(uid) => write!(f, "c{}", uid.0),
        }
    }
}

impl std::str::FromStr for Ref {
    type Err = IdError;

    fn from_str(text: &str) -> Result<Self, IdError> {
        if let Some(n) = parse_number(text, "p") {
            Ok(Self::Point(EntityUid(n)))
        } else if let Some(n) = parse_number(text, "c") {
            Ok(Self::Curve(EntityUid(n)))
        } else {
            Err(IdError::new("entity", text, "p<number> or c<number>"))
        }
    }
}

mitcad_model_base::internal::ids::string_serde!(Ref, "an entity like \"p3\" or \"c4\"");

/// Point ids in files and commands: `"p3"`.
#[doc(hidden)]
pub mod point_serde {
    use crate::ids::{EntityUid, IdError};
    use mitcad_model_base::internal::ids::parse_number;

    pub fn serialize<S: serde::Serializer>(id: &EntityUid, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&format!("p{}", id.0))
    }

    pub fn parse(text: &str) -> Result<EntityUid, IdError> {
        parse_number(text, "p")
            .map(EntityUid)
            .ok_or_else(|| IdError::new("point", text, "p<number>"))
    }

    pub fn deserialize<'de, D: serde::Deserializer<'de>>(d: D) -> Result<EntityUid, D::Error> {
        let text: String = serde::Deserialize::deserialize(d)?;
        parse(&text).map_err(serde::de::Error::custom)
    }
}

/// Lists of point ids: `["p3", "p4"]`.
pub(crate) mod points_serde {
    use serde::Deserialize;

    use crate::ids::EntityUid;

    pub fn serialize<S: serde::Serializer>(ids: &[EntityUid], s: S) -> Result<S::Ok, S::Error> {
        s.collect_seq(ids.iter().map(|id| format!("p{}", id.0)))
    }

    pub fn deserialize<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<EntityUid>, D::Error> {
        let names = Vec::<String>::deserialize(d)?;
        names
            .iter()
            .map(|n| super::point_serde::parse(n).map_err(serde::de::Error::custom))
            .collect()
    }
}

/// Lists of entity ids: `["c3", "p4"]`.
pub(crate) mod uids_serde {
    use serde::Deserialize;

    use super::Ref;
    use crate::ids::EntityUid;

    pub fn serialize<S: serde::Serializer>(ids: &[Ref], s: S) -> Result<S::Ok, S::Error> {
        s.collect_seq(ids.iter().map(ToString::to_string))
    }

    pub fn deserialize<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<Ref>, D::Error> {
        let names = Vec::<String>::deserialize(d)?;
        names
            .iter()
            .map(|n| n.parse().map_err(serde::de::Error::custom))
            .collect()
    }

    #[allow(dead_code)]
    pub fn uids(refs: &[Ref]) -> Vec<EntityUid> {
        refs.iter().map(|r| r.uid()).collect()
    }
}

use mitcad_model_base::internal::ids::curve_serde;

/// Lists of curve ids: `["c3", "c4"]`.
#[doc(hidden)]
pub mod curves_serde {
    use serde::Deserialize;

    use crate::ids::EntityUid;

    pub fn serialize<S: serde::Serializer>(ids: &[EntityUid], s: S) -> Result<S::Ok, S::Error> {
        s.collect_seq(ids.iter().map(|id| format!("c{}", id.0)))
    }

    pub fn deserialize<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<EntityUid>, D::Error> {
        Vec::<String>::deserialize(d)?
            .iter()
            .map(|n| EntityUid::parse_curve(n).map_err(serde::de::Error::custom))
            .collect()
    }
}

fn is_false(value: &bool) -> bool {
    !*value
}

/// The geometry of an entity. Positions are the last solved ones.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum EntityKind {
    Point {
        at: [f64; 2],
    },
    Line {
        #[serde(with = "point_serde")]
        start: EntityUid,
        #[serde(with = "point_serde")]
        end: EntityUid,
        /// A centre line: in profiles (unlike construction lines), the
        /// default axis of a revolve.
        #[serde(default, skip_serializing_if = "is_false")]
        centerline: bool,
    },
    Circle {
        #[serde(with = "point_serde")]
        center: EntityUid,
        radius: f64,
        /// Where the circle starts, counter-clockwise from the sketch x
        /// axis (radians): the seam of the cylinder it makes. Zero unless
        /// an import set it (a circle drawn here starts on the x axis).
        #[serde(default, rename = "start_angle", skip_serializing_if = "is_zero")]
        start: f64,
    },
    /// Counter-clockwise from `start` to `end` around `center`.
    Arc {
        #[serde(with = "point_serde")]
        center: EntityUid,
        #[serde(with = "point_serde")]
        start: EntityUid,
        #[serde(with = "point_serde")]
        end: EntityUid,
    },
    /// `major` is the end point of the major axis.
    Ellipse {
        #[serde(with = "point_serde")]
        center: EntityUid,
        #[serde(with = "point_serde")]
        major: EntityUid,
        minor_radius: f64,
    },
    /// Counter-clockwise in the ellipse parameter from `start` to `end`.
    EllipticalArc {
        #[serde(with = "point_serde")]
        center: EntityUid,
        #[serde(with = "point_serde")]
        major: EntityUid,
        minor_radius: f64,
        #[serde(with = "point_serde")]
        start: EntityUid,
        #[serde(with = "point_serde")]
        end: EntityUid,
    },
    /// A B-spline by its control points. Empty `weights` is non-rational,
    /// empty `knots` a clamped uniform knot vector on [0, 1].
    Spline {
        degree: u32,
        #[serde(with = "points_serde")]
        control: Vec<EntityUid>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        weights: Vec<f64>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        knots: Vec<f64>,
    },
    /// A cubic spline through fit points (chord-length parameters).
    /// `closed` (mitcad#783): it runs on from the last point back to the
    /// first (listed once, at least three) and is smooth (C2) through it:
    /// one closed curve without ends, like a circle.
    FittedSpline {
        #[serde(with = "points_serde")]
        points: Vec<EntityUid>,
        #[serde(default, skip_serializing_if = "is_false")]
        closed: bool,
    },
}

impl EntityKind {
    pub fn is_point(&self) -> bool {
        matches!(self, Self::Point { .. })
    }

    /// The type name in files and commands.
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Point { .. } => "point",
            Self::Line { .. } => "line",
            Self::Circle { .. } => "circle",
            Self::Arc { .. } => "arc",
            Self::Ellipse { .. } => "ellipse",
            Self::EllipticalArc { .. } => "elliptical_arc",
            Self::Spline { .. } => "spline",
            Self::FittedSpline { .. } => "fitted_spline",
        }
    }

    /// The points a curve refers to: line `[start, end]`, circle
    /// `[center]`, arc `[center, start, end]`, ellipse `[center, major]`,
    /// elliptical arc `[center, major, start, end]`, splines their control
    /// or fit points.
    pub fn points(&self) -> Vec<EntityUid> {
        match self {
            Self::Point { .. } => Vec::new(),
            Self::Line { start, end, .. } => vec![*start, *end],
            Self::Circle { center, .. } => vec![*center],
            Self::Arc { center, start, end } => vec![*center, *start, *end],
            Self::Ellipse { center, major, .. } => vec![*center, *major],
            Self::EllipticalArc {
                center,
                major,
                start,
                end,
                ..
            } => vec![*center, *major, *start, *end],
            Self::Spline { control, .. } => control.clone(),
            Self::FittedSpline { points, .. } => points.clone(),
        }
    }

    /// The start and end points of an open curve (a closed fitted spline
    /// has none).
    pub fn ends(&self) -> Option<(EntityUid, EntityUid)> {
        match self {
            Self::Line { start, end, .. }
            | Self::Arc { start, end, .. }
            | Self::EllipticalArc { start, end, .. } => Some((*start, *end)),
            Self::Spline { control: p, .. }
            | Self::FittedSpline {
                points: p,
                closed: false,
            } => Some((*p.first()?, *p.last()?)),
            _ => None,
        }
    }

    /// Replaces a point reference.
    pub fn replace_point(&mut self, old: EntityUid, new: EntityUid) {
        let swap = |p: &mut EntityUid| {
            if *p == old {
                *p = new;
            }
        };
        match self {
            Self::Point { .. } => {}
            Self::Line { start, end, .. } => {
                swap(start);
                swap(end);
            }
            Self::Circle { center, .. } => swap(center),
            Self::Arc { center, start, end } => {
                swap(center);
                swap(start);
                swap(end);
            }
            Self::Ellipse { center, major, .. } => {
                swap(center);
                swap(major);
            }
            Self::EllipticalArc {
                center,
                major,
                start,
                end,
                ..
            } => {
                swap(center);
                swap(major);
                swap(start);
                swap(end);
            }
            Self::Spline {
                control: points, ..
            }
            | Self::FittedSpline { points, .. } => {
                points.iter_mut().for_each(swap);
            }
        }
    }
}

/// One of the sketch's own axes (mitcad#500): `"x"` or `"y"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SketchAxis {
    X,
    Y,
}

impl SketchAxis {
    /// The axis's unit direction in the sketch plane.
    pub fn direction(self) -> [f64; 2] {
        match self {
            Self::X => [1.0, 0.0],
            Self::Y => [0.0, 1.0],
        }
    }
}

/// A sketch entity.
#[derive(Debug, Clone, PartialEq)]
pub struct Entity {
    pub id: EntityUid,
    pub kind: EntityKind,
    /// Left out of profile regions.
    pub construction: bool,
    /// Not moved by the solver.
    pub fixed: bool,
    /// Projected or included model geometry.
    pub reference: bool,
    /// The sketch's own x or y axis (mitcad#500): the fixed construction
    /// line along it that constraints and dimensions refer to, and that
    /// line's far end point (see [`axes`]). The app draws the axis across
    /// the view instead of the line.
    pub axis: Option<SketchAxis>,
}

impl Entity {
    pub fn new(id: EntityUid, kind: EntityKind) -> Self {
        Self {
            id,
            kind,
            construction: false,
            fixed: false,
            reference: false,
            axis: None,
        }
    }

    pub fn point(id: EntityUid, at: [f64; 2]) -> Self {
        Self::new(id, EntityKind::Point { at })
    }

    pub fn is_point(&self) -> bool {
        self.kind.is_point()
    }

    /// The entity as a reference: `p<n>` or `c<n>`.
    pub fn as_ref(&self) -> Ref {
        if self.is_point() {
            Ref::Point(self.id)
        } else {
            Ref::Curve(self.id)
        }
    }
}

/// The file and command form: `{"id": "c3", "type": "line", "start": "p1",
/// "end": "p2", "construction": true}`.
#[derive(Serialize)]
struct EntityOut<'a> {
    id: Ref,
    #[serde(flatten)]
    kind: &'a EntityKind,
    #[serde(skip_serializing_if = "is_false")]
    construction: bool,
    #[serde(skip_serializing_if = "is_false")]
    fixed: bool,
    #[serde(skip_serializing_if = "is_false")]
    reference: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    axis: Option<SketchAxis>,
}

impl Serialize for Entity {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        EntityOut {
            id: self.as_ref(),
            kind: &self.kind,
            construction: self.construction,
            fixed: self.fixed,
            reference: self.reference,
            axis: self.axis,
        }
        .serialize(serializer)
    }
}

/// Takes the common fields out of an object; the rest is the item's kind.
fn take_field(map: &mut Map<String, Value>, field: &str) -> Option<Value> {
    map.remove(field)
}

fn take_flag<E: serde::de::Error>(map: &mut Map<String, Value>, field: &str) -> Result<bool, E> {
    match take_field(map, field) {
        None => Ok(false),
        Some(Value::Bool(b)) => Ok(b),
        Some(_) => Err(E::custom(format!("\"{field}\" must be true or false"))),
    }
}

impl<'de> Deserialize<'de> for Entity {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let mut map = Map::deserialize(deserializer)?;
        let id: Ref = match take_field(&mut map, "id") {
            Some(Value::String(text)) => text.parse().map_err(D::Error::custom)?,
            Some(_) => return Err(D::Error::custom("\"id\" must be a string")),
            None => return Err(D::Error::missing_field("id")),
        };
        let construction = take_flag(&mut map, "construction")?;
        let fixed = take_flag(&mut map, "fixed")?;
        let reference = take_flag(&mut map, "reference")?;
        let axis: Option<SketchAxis> = match take_field(&mut map, "axis") {
            None => None,
            Some(value) => Some(
                serde_json::from_value::<SketchAxis>(value)
                    .map_err(|_| D::Error::custom("\"axis\" must be \"x\" or \"y\""))?,
            ),
        };
        let kind: EntityKind =
            serde_json::from_value(Value::Object(map)).map_err(D::Error::custom)?;
        if id.is_point() != kind.is_point() {
            return Err(D::Error::custom(format!(
                "{id}: a {} has an id like \"{}{}\"",
                kind.type_name(),
                if kind.is_point() { "p" } else { "c" },
                id.uid().0
            )));
        }
        Ok(Self {
            id: id.uid(),
            kind,
            construction,
            fixed,
            reference,
            axis,
        })
    }
}

/// A geometric constraint (`GeometricConstraints` in .f3d designs).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ConstraintKind {
    /// A point at another point, or on a curve (the infinite line, the full
    /// circle or ellipse of an arc, or the spline).
    Coincident {
        #[serde(with = "point_serde")]
        point: EntityUid,
        entity: Ref,
    },
    Horizontal {
        #[serde(with = "curve_serde")]
        line: EntityUid,
    },
    Vertical {
        #[serde(with = "curve_serde")]
        line: EntityUid,
    },
    HorizontalPoints {
        #[serde(with = "point_serde")]
        a: EntityUid,
        #[serde(with = "point_serde")]
        b: EntityUid,
    },
    VerticalPoints {
        #[serde(with = "point_serde")]
        a: EntityUid,
        #[serde(with = "point_serde")]
        b: EntityUid,
    },
    Parallel {
        #[serde(with = "curve_serde")]
        a: EntityUid,
        #[serde(with = "curve_serde")]
        b: EntityUid,
    },
    Perpendicular {
        #[serde(with = "curve_serde")]
        a: EntityUid,
        #[serde(with = "curve_serde")]
        b: EntityUid,
    },
    Collinear {
        #[serde(with = "curve_serde")]
        a: EntityUid,
        #[serde(with = "curve_serde")]
        b: EntityUid,
    },
    /// `at` (mitcad#783): the point it applies at, an end of the curves
    /// (each curve's end there, else its nearer end), so that a curve can
    /// be tangent at both ends; `a` and `b` are then the same spline when
    /// both its ends are at `at` (smooth where it closes). Without it (all
    /// older designs) at the point the curves share, or the nearest ends.
    Tangent {
        #[serde(with = "curve_serde")]
        a: EntityUid,
        #[serde(with = "curve_serde")]
        b: EntityUid,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            with = "point_opt_serde"
        )]
        at: Option<EntityUid>,
    },
    /// Curvature continuity (G2) where a spline joins another curve; `at`
    /// as a tangent's.
    Smooth {
        #[serde(with = "curve_serde")]
        a: EntityUid,
        #[serde(with = "curve_serde")]
        b: EntityUid,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            with = "point_opt_serde"
        )]
        at: Option<EntityUid>,
    },
    /// Equal length (lines), radius (circles, arcs) or minor radius
    /// (ellipses and elliptical arcs).
    Equal {
        #[serde(with = "curve_serde")]
        a: EntityUid,
        #[serde(with = "curve_serde")]
        b: EntityUid,
    },
    Concentric {
        #[serde(with = "curve_serde")]
        a: EntityUid,
        #[serde(with = "curve_serde")]
        b: EntityUid,
    },
    Midpoint {
        #[serde(with = "point_serde")]
        point: EntityUid,
        #[serde(with = "curve_serde")]
        curve: EntityUid,
    },
    /// Two points, or two curves of one type, mirrored about a line.
    /// `radii` (two circles or arcs; default true): their radii are made
    /// equal too. False when other constraints or dimensions already fix
    /// both radii (mitcad#865): `sketch.add_constraint` then mirrors only
    /// the positions (the centres, an arc's end directions), so that it
    /// does not over-constrain the sketch.
    Symmetric {
        a: Ref,
        b: Ref,
        #[serde(with = "curve_serde")]
        axis: EntityUid,
        #[serde(default = "yes", skip_serializing_if = "is_true")]
        radii: bool,
    },
    /// Equal spacing (mitcad#221): the successive intervals between the
    /// anchors, in their stored order, are equal, measured as `mode` says.
    /// An anchor is a point, the centre of a circle or an arc, or the
    /// midpoint of a line (`p3`, `c4`); at least three. `direction` is the
    /// line an `along` spacing measures along. The common spacing is free
    /// unless a `spacing` dimension drives it.
    EqualSpacing {
        anchors: Vec<Ref>,
        mode: SpacingMode,
        #[serde(
            default,
            skip_serializing_if = "Option::is_none",
            with = "curve_opt_serde"
        )]
        direction: Option<EntityUid>,
    },
}

/// What the intervals of an equal spacing measure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpacingMode {
    /// Successive x intervals.
    Horizontal,
    /// Successive y intervals.
    Vertical,
    /// Successive intervals projected on the direction of a line.
    Along,
    /// The distances between successive anchors.
    PointToPoint,
}

impl SpacingMode {
    /// Whether the intervals are measured along a direction (signed),
    /// not as distances.
    pub fn directional(self) -> bool {
        self != Self::PointToPoint
    }
}

/// An optional curve id: `"c4"`.
#[doc(hidden)]
pub mod curve_opt_serde {
    use crate::ids::EntityUid;

    pub fn serialize<S: serde::Serializer>(
        id: &Option<EntityUid>,
        s: S,
    ) -> Result<S::Ok, S::Error> {
        match id {
            Some(id) => s.serialize_str(&id.curve_name()),
            None => s.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: serde::Deserializer<'de>>(
        d: D,
    ) -> Result<Option<EntityUid>, D::Error> {
        let text: Option<String> = serde::Deserialize::deserialize(d)?;
        text.map(|t| EntityUid::parse_curve(&t).map_err(serde::de::Error::custom))
            .transpose()
    }
}

/// An optional point id: `"p3"` (a tangent's point, mitcad#783).
pub(crate) mod point_opt_serde {
    use super::point_serde;

    use crate::ids::EntityUid;

    pub fn serialize<S: serde::Serializer>(
        id: &Option<EntityUid>,
        s: S,
    ) -> Result<S::Ok, S::Error> {
        match id {
            Some(id) => point_serde::serialize(id, s),
            None => s.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: serde::Deserializer<'de>>(
        d: D,
    ) -> Result<Option<EntityUid>, D::Error> {
        let text: Option<String> = serde::Deserialize::deserialize(d)?;
        text.map(|t| point_serde::parse(&t).map_err(serde::de::Error::custom))
            .transpose()
    }
}

impl ConstraintKind {
    /// A tangent where the curves meet, without a point of its own.
    pub fn tangent(a: EntityUid, b: EntityUid) -> Self {
        Self::Tangent { a, b, at: None }
    }

    /// The entities it refers to.
    pub fn refs(&self) -> Vec<Ref> {
        use ConstraintKind::*;
        let p = Ref::Point;
        let c = Ref::Curve;
        match self {
            Coincident { point, entity } => vec![p(*point), *entity],
            Horizontal { line } | Vertical { line } => vec![c(*line)],
            HorizontalPoints { a, b } | VerticalPoints { a, b } => vec![p(*a), p(*b)],
            Parallel { a, b }
            | Perpendicular { a, b }
            | Collinear { a, b }
            | Equal { a, b }
            | Concentric { a, b } => vec![c(*a), c(*b)],
            // A spline smooth where it closes names it once (mitcad#783).
            Tangent { a, b, at } | Smooth { a, b, at } => {
                let mut refs = vec![c(*a)];
                if a != b {
                    refs.push(c(*b));
                }
                refs.extend(at.map(p));
                refs
            }
            Midpoint { point, curve } => vec![p(*point), c(*curve)],
            Symmetric { a, b, axis, .. } => vec![*a, *b, c(*axis)],
            EqualSpacing {
                anchors, direction, ..
            } => anchors
                .iter()
                .copied()
                .chain(direction.map(Ref::Curve))
                .collect(),
        }
    }

    pub fn type_name(&self) -> String {
        serde_json::to_value(self).expect("serializes")["type"]
            .as_str()
            .unwrap_or_default()
            .to_owned()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Constraint {
    pub id: ConstraintUid,
    pub kind: ConstraintKind,
    /// Whether the solver applies it (mitcad#721): an inactive constraint
    /// is kept and shown, but constrains nothing until it is activated.
    pub active: bool,
    /// Why it is inactive, when something says (an import: "not met in the
    /// imported file"); None for an active one.
    pub inactive_reason: Option<String>,
}

impl Constraint {
    /// An active constraint.
    pub fn new(id: ConstraintUid, kind: ConstraintKind) -> Self {
        Self {
            id,
            kind,
            active: true,
            inactive_reason: None,
        }
    }
}

#[derive(Serialize)]
struct ConstraintOut<'a> {
    id: ConstraintUid,
    #[serde(flatten)]
    kind: &'a ConstraintKind,
    #[serde(skip_serializing_if = "is_true")]
    active: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    inactive_reason: Option<&'a str>,
}

impl Serialize for Constraint {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        ConstraintOut {
            id: self.id,
            kind: &self.kind,
            active: self.active,
            inactive_reason: inactive_reason(self.active, &self.inactive_reason),
        }
        .serialize(serializer)
    }
}

fn take_id<E: serde::de::Error>(map: &mut Map<String, Value>) -> Result<ConstraintUid, E> {
    match take_field(map, "id") {
        Some(Value::String(text)) => text.parse().map_err(E::custom),
        Some(_) => Err(E::custom("\"id\" must be a string")),
        None => Err(E::missing_field("id")),
    }
}

/// The `active` flag and the `inactive_reason` of a constraint or dimension
/// (mitcad#721): active unless `"active": false`; a reason only goes with
/// an inactive one.
fn take_active<E: serde::de::Error>(
    map: &mut Map<String, Value>,
) -> Result<(bool, Option<String>), E> {
    let active = match take_field(map, "active") {
        None => true,
        Some(Value::Bool(b)) => b,
        Some(_) => return Err(E::custom("\"active\" must be true or false")),
    };
    let reason = match take_field(map, "inactive_reason") {
        None | Some(Value::Null) => None,
        Some(Value::String(text)) => Some(text),
        Some(_) => return Err(E::custom("\"inactive_reason\" must be a string")),
    };
    Ok((active, reason.filter(|r| !active && !r.is_empty())))
}

/// The reason written with an item: only an inactive one's.
fn inactive_reason(active: bool, reason: &Option<String>) -> Option<&str> {
    reason.as_deref().filter(|_| !active)
}

impl<'de> Deserialize<'de> for Constraint {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let mut map = Map::deserialize(deserializer)?;
        let id = take_id(&mut map)?;
        let (active, inactive_reason) = take_active(&mut map)?;
        let kind = serde_json::from_value(Value::Object(map)).map_err(D::Error::custom)?;
        Ok(Self {
            id,
            kind,
            active,
            inactive_reason,
        })
    }
}

/// What a dimension measures (`SketchDimensions` in .f3d designs).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum DimensionKind {
    /// Distance between two points (an aligned distance).
    Distance {
        #[serde(with = "point_serde")]
        a: EntityUid,
        #[serde(with = "point_serde")]
        b: EntityUid,
    },
    /// `|Δx|` between two points in sketch axes, on the side `b` is; with
    /// `signed` (mitcad#404), `b.x - a.x` itself, which may be negative.
    HorizontalDistance {
        #[serde(with = "point_serde")]
        a: EntityUid,
        #[serde(with = "point_serde")]
        b: EntityUid,
        #[serde(default, skip_serializing_if = "is_false")]
        signed: bool,
    },
    /// `|Δy|` between two points in sketch axes; with `signed`,
    /// `b.y - a.y`.
    VerticalDistance {
        #[serde(with = "point_serde")]
        a: EntityUid,
        #[serde(with = "point_serde")]
        b: EntityUid,
        #[serde(default, skip_serializing_if = "is_false")]
        signed: bool,
    },
    /// Distance of a point from a line (its infinite extension).
    PointLineDistance {
        #[serde(with = "point_serde")]
        point: EntityUid,
        #[serde(with = "curve_serde")]
        line: EntityUid,
    },
    /// Distance between parallel lines (an offset dimension).
    LineDistance {
        #[serde(with = "curve_serde")]
        a: EntityUid,
        #[serde(with = "curve_serde")]
        b: EntityUid,
    },
    Length {
        #[serde(with = "curve_serde")]
        line: EntityUid,
    },
    /// The gap between a circle or arc and a line it does not cross: the
    /// distance of its centre from the line (its infinite extension) minus
    /// its radius.
    TangentDistance {
        #[serde(with = "curve_serde")]
        circle: EntityUid,
        #[serde(with = "curve_serde")]
        line: EntityUid,
    },
    /// The gap between two concentric circles or arcs (mitcad#445): the
    /// difference of their radii, on the side `b` is when it is added.
    ConcentricDistance {
        #[serde(with = "curve_serde")]
        a: EntityUid,
        #[serde(with = "curve_serde")]
        b: EntityUid,
    },
    /// Angle between two lines, 0 to 180 degrees (mitcad#296): between the
    /// rays `±(a's end - start)` and `±(b's end - start)`, reversed where
    /// `reverse_a` or `reverse_b` is true, which bound the sector of the
    /// four the lines' extensions make that it measures. With neither
    /// given (sketches from before), the angle between the directions or
    /// its supplement, whichever is closer to the value where the lines are
    /// when the sketch is solved; see [`DimensionKind::angle_side`].
    Angle {
        #[serde(with = "curve_serde")]
        a: EntityUid,
        #[serde(with = "curve_serde")]
        b: EntityUid,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reverse_a: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reverse_b: Option<bool>,
    },
    Radius {
        #[serde(with = "curve_serde")]
        curve: EntityUid,
    },
    Diameter {
        #[serde(with = "curve_serde")]
        curve: EntityUid,
    },
    ArcLength {
        #[serde(with = "curve_serde")]
        arc: EntityUid,
    },
    MajorRadius {
        #[serde(with = "curve_serde")]
        ellipse: EntityUid,
    },
    MinorRadius {
        #[serde(with = "curve_serde")]
        ellipse: EntityUid,
    },
    /// Twice the distance of a point or a parallel line from a centre line
    /// (the diameter of a revolved body).
    LinearDiameter {
        #[serde(with = "curve_serde")]
        axis: EntityUid,
        entity: Ref,
    },
    /// The common spacing of an equal spacing constraint (mitcad#221): every
    /// interval of it, measured as the constraint measures them. One per
    /// constraint; it goes with the constraint.
    Spacing { constraint: ConstraintUid },
}

/// How a dimension value must be: positive, not negative, an angle, or any
/// finite number (a signed distance).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueRange {
    Positive,
    NotNegative,
    Angle,
    Signed,
}

impl DimensionKind {
    pub fn refs(&self) -> Vec<Ref> {
        use DimensionKind::*;
        let p = Ref::Point;
        let c = Ref::Curve;
        match self {
            Distance { a, b } | HorizontalDistance { a, b, .. } | VerticalDistance { a, b, .. } => {
                vec![p(*a), p(*b)]
            }
            PointLineDistance { point, line } => vec![p(*point), c(*line)],
            LineDistance { a, b } | Angle { a, b, .. } | ConcentricDistance { a, b } => {
                vec![c(*a), c(*b)]
            }
            TangentDistance { circle, line } => vec![c(*circle), c(*line)],
            Length { line } => vec![c(*line)],
            Radius { curve } | Diameter { curve } => vec![c(*curve)],
            ArcLength { arc } => vec![c(*arc)],
            MajorRadius { ellipse } | MinorRadius { ellipse } => vec![c(*ellipse)],
            LinearDiameter { axis, entity } => vec![c(*axis), *entity],
            // Its entities are its constraint's.
            Spacing { .. } => Vec::new(),
        }
    }

    /// The type name, also the last part of the dimension's value slot
    /// (`dimensions[k3].diameter`).
    pub fn type_name(&self) -> &'static str {
        use DimensionKind::*;
        match self {
            Distance { .. } => "distance",
            HorizontalDistance { .. } => "horizontal_distance",
            VerticalDistance { .. } => "vertical_distance",
            PointLineDistance { .. } => "point_line_distance",
            LineDistance { .. } => "line_distance",
            TangentDistance { .. } => "tangent_distance",
            ConcentricDistance { .. } => "concentric_distance",
            Length { .. } => "length",
            Angle { .. } => "angle",
            Radius { .. } => "radius",
            Diameter { .. } => "diameter",
            ArcLength { .. } => "arc_length",
            MajorRadius { .. } => "major_radius",
            MinorRadius { .. } => "minor_radius",
            LinearDiameter { .. } => "linear_diameter",
            Spacing { .. } => "spacing",
        }
    }

    /// An angle dimension's side (mitcad#296): whether it measures from
    /// `a`'s reversed direction and whether to `b`'s reversed one. None for
    /// one without a side (it takes the angle closer to its value) and for
    /// the other kinds.
    pub fn angle_side(&self) -> Option<[bool; 2]> {
        match self {
            Self::Angle {
                reverse_a,
                reverse_b,
                ..
            } if reverse_a.is_some() || reverse_b.is_some() => {
                Some([reverse_a.unwrap_or(false), reverse_b.unwrap_or(false)])
            }
            _ => None,
        }
    }

    pub fn range(&self) -> ValueRange {
        use DimensionKind::*;
        match self {
            HorizontalDistance { signed: true, .. } | VerticalDistance { signed: true, .. } => {
                ValueRange::Signed
            }
            HorizontalDistance { .. }
            | VerticalDistance { .. }
            | PointLineDistance { .. }
            | LineDistance { .. }
            | TangentDistance { .. }
            | ConcentricDistance { .. } => ValueRange::NotNegative,
            Angle { .. } => ValueRange::Angle,
            _ => ValueRange::Positive,
        }
    }
}

/// A dimension: driving with a parameter as its value, or driven.
#[derive(Debug, Clone, PartialEq)]
pub struct Dimension<P> {
    pub id: ConstraintUid,
    pub kind: DimensionKind,
    /// The parameter of a driving dimension; None for a driven one.
    pub value: Option<P>,
    /// The read-only reference parameter of a driven dimension: the value
    /// it measures (mitcad#231). None for a driving one, and for a driven
    /// one until the document gives it one.
    pub reference: Option<P>,
    /// Where the value is shown, in sketch coordinates.
    pub text: Option<[f64; 2]>,
    /// Whether the solver applies it (mitcad#721): an inactive driving
    /// dimension keeps its value but drives nothing, an inactive driven one
    /// measures nothing, until it is activated.
    pub active: bool,
    /// Why it is inactive, when something says; None for an active one.
    pub inactive_reason: Option<String>,
}

impl<P> Dimension<P> {
    /// An active dimension: driving with `value`, driven without.
    pub fn new(id: ConstraintUid, kind: DimensionKind, value: Option<P>) -> Self {
        Self {
            id,
            kind,
            value,
            reference: None,
            text: None,
            active: true,
            inactive_reason: None,
        }
    }

    pub fn is_driven(&self) -> bool {
        self.value.is_none()
    }

    /// The slot of its value: `dimensions[k3].diameter`.
    pub fn slot(&self) -> String {
        format!("dimensions[{}].{}", self.id, self.kind.type_name())
    }

    pub fn map_value<Q, E>(
        &self,
        f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
    ) -> Result<Dimension<Q>, E> {
        Ok(Dimension {
            id: self.id,
            kind: self.kind.clone(),
            value: match &self.value {
                Some(value) => Some(f(&self.slot(), value)?),
                None => None,
            },
            reference: match &self.reference {
                Some(reference) => Some(f(&self.slot(), reference)?),
                None => None,
            },
            text: self.text,
            active: self.active,
            inactive_reason: self.inactive_reason.clone(),
        })
    }
}

#[derive(Serialize)]
struct DimensionOut<'a, P> {
    id: ConstraintUid,
    #[serde(flatten)]
    kind: &'a DimensionKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<&'a P>,
    #[serde(skip_serializing_if = "is_false")]
    driven: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    reference: Option<&'a P>,
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<[f64; 2]>,
    #[serde(skip_serializing_if = "is_true")]
    active: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    inactive_reason: Option<&'a str>,
}

impl<P: Serialize> Serialize for Dimension<P> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        DimensionOut {
            id: self.id,
            kind: &self.kind,
            value: self.value.as_ref(),
            driven: self.value.is_none(),
            reference: self.reference.as_ref(),
            text: self.text,
            active: self.active,
            inactive_reason: inactive_reason(self.active, &self.inactive_reason),
        }
        .serialize(serializer)
    }
}

impl<'de, P: Deserialize<'de>> Deserialize<'de> for Dimension<P> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let mut map = Map::deserialize(deserializer)?;
        let id = take_id(&mut map)?;
        let driven = take_flag(&mut map, "driven")?;
        let value = take_field(&mut map, "value")
            .map(|v| P::deserialize(v).map_err(D::Error::custom))
            .transpose()?;
        let reference = take_field(&mut map, "reference")
            .map(|v| P::deserialize(v).map_err(D::Error::custom))
            .transpose()?;
        let text = take_field(&mut map, "text")
            .map(|v| serde_json::from_value::<[f64; 2]>(v).map_err(D::Error::custom))
            .transpose()?;
        let (active, inactive_reason) = take_active(&mut map)?;
        let kind = serde_json::from_value(Value::Object(map)).map_err(D::Error::custom)?;
        match (driven, &value) {
            (true, Some(_)) => Err(D::Error::custom(format!(
                "{id}: a driven dimension has no \"value\""
            ))),
            (false, Some(_)) if reference.is_some() => Err(D::Error::custom(format!(
                "{id}: a driving dimension has no \"reference\""
            ))),
            (false, None) => Err(D::Error::custom(format!(
                "{id}: a driving dimension needs a \"value\" (or \"driven\": true)"
            ))),
            _ => Ok(Self {
                id,
                kind,
                value,
                reference,
                text,
                active,
                inactive_reason,
            }),
        }
    }
}

/// A curve or point of a sketch, named by the sketch's uid and the
/// entity's id (mitcad#904): `F5/c3`, `F5/p2` in messages; in a
/// definition and in commands `{"sketch": "F5", "curve": "c3"}` or
/// `{"sketch": "F5", "point": "p2"}`, as the references to geometry write
/// them. The ids of a sketch's entities stay with them across its edits,
/// so the name keeps naming the same curve while the sketch changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SketchEntityRef {
    pub sketch: crate::ids::FeatureUid,
    pub entity: Ref,
}

impl fmt::Display for SketchEntityRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.sketch, self.entity)
    }
}

impl std::str::FromStr for SketchEntityRef {
    type Err = IdError;

    fn from_str(text: &str) -> Result<Self, IdError> {
        let wrong = || IdError::new("sketch entity", text, "F<number>/c<number> or /p<number>");
        let (sketch, entity) = text.split_once('/').ok_or_else(wrong)?;
        Ok(Self {
            sketch: sketch.parse().map_err(|_| wrong())?,
            entity: entity.parse().map_err(|_| wrong())?,
        })
    }
}

impl SketchEntityRef {
    /// `{"sketch": "F5", "curve": "c3"}` or `{"sketch": "F5", "point": "p2"}`.
    pub fn to_json(&self) -> Value {
        let key = if self.entity.is_point() {
            "point"
        } else {
            "curve"
        };
        let mut map = Map::new();
        map.insert("sketch".to_owned(), Value::String(self.sketch.to_string()));
        map.insert(key.to_owned(), Value::String(self.entity.to_string()));
        Value::Object(map)
    }

    /// From [`SketchEntityRef::to_json`]'s form: a sketch and one of
    /// `curve` and `point`, nothing else.
    pub fn from_json(value: &Value) -> Result<Self, String> {
        let Value::Object(map) = value else {
            return Err(format!(
                "a sketch entity is {{\"sketch\": \"F5\", \"curve\": \"c3\"}} or \
                 {{\"sketch\": \"F5\", \"point\": \"p2\"}}, not {value}"
            ));
        };
        if let Some(key) = map
            .keys()
            .find(|k| !matches!(k.as_str(), "sketch" | "curve" | "point"))
        {
            return Err(format!("a sketch entity has no field \"{key}\""));
        }
        let text = |key: &str| -> Result<Option<&str>, String> {
            match map.get(key) {
                None => Ok(None),
                Some(Value::String(s)) => Ok(Some(s.as_str())),
                Some(other) => Err(format!("\"{key}\" is a text, not {other}")),
            }
        };
        let sketch: crate::ids::FeatureUid = text("sketch")?
            .ok_or_else(|| "a sketch entity needs its \"sketch\"".to_owned())?
            .parse()
            .map_err(|e| format!("{e}"))?;
        let entity = match (text("curve")?, text("point")?) {
            (Some(curve), None) => Ref::Curve(
                parse_number(curve, "c")
                    .map(EntityUid)
                    .ok_or_else(|| format!("\"curve\" is c<number>, not {curve}"))?,
            ),
            (None, Some(point)) => Ref::Point(
                parse_number(point, "p")
                    .map(EntityUid)
                    .ok_or_else(|| format!("\"point\" is p<number>, not {point}"))?,
            ),
            _ => {
                return Err(
                    "a sketch entity names one \"curve\" or one \"point\" of its sketch".to_owned(),
                );
            }
        };
        Ok(Self { sketch, entity })
    }
}

impl Serialize for SketchEntityRef {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.to_json().serialize(s)
    }
}

impl<'de> Deserialize<'de> for SketchEntityRef {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(d)?;
        Self::from_json(&value).map_err(D::Error::custom)
    }
}

/// What a linked projection follows: model geometry by its topological
/// name (an edge, a face or a vertex), or a curve or point of an earlier
/// sketch (mitcad#904). In a definition the first is its name's text, as
/// in every earlier version, the second the object form of
/// [`SketchEntityRef`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ProjectionSource {
    Topo(TopoName),
    Sketch(SketchEntityRef),
}

impl ProjectionSource {
    /// The model geometry's name; None for a sketch's entity.
    pub fn topo(&self) -> Option<&TopoName> {
        match self {
            Self::Topo(name) => Some(name),
            Self::Sketch(_) => None,
        }
    }

    /// The sketch's entity; None for model geometry.
    pub fn sketch(&self) -> Option<&SketchEntityRef> {
        match self {
            Self::Topo(_) => None,
            Self::Sketch(entity) => Some(entity),
        }
    }
}

impl From<TopoName> for ProjectionSource {
    fn from(name: TopoName) -> Self {
        Self::Topo(name)
    }
}

impl From<SketchEntityRef> for ProjectionSource {
    fn from(entity: SketchEntityRef) -> Self {
        Self::Sketch(entity)
    }
}

impl PartialEq<TopoName> for ProjectionSource {
    fn eq(&self, other: &TopoName) -> bool {
        self.topo() == Some(other)
    }
}

impl fmt::Display for ProjectionSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Topo(name) => name.fmt(f),
            Self::Sketch(entity) => entity.fmt(f),
        }
    }
}

impl std::str::FromStr for ProjectionSource {
    type Err = String;

    /// `F5/c3` a sketch's entity, anything else a topological name.
    fn from_str(text: &str) -> Result<Self, String> {
        if let Ok(entity) = text.parse::<SketchEntityRef>() {
            return Ok(Self::Sketch(entity));
        }
        text.parse::<TopoName>()
            .map(Self::Topo)
            .map_err(|e| e.to_string())
    }
}

impl Serialize for ProjectionSource {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Topo(name) => name.serialize(s),
            Self::Sketch(entity) => entity.serialize(s),
        }
    }
}

impl<'de> Deserialize<'de> for ProjectionSource {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        match Value::deserialize(d)? {
            Value::String(text) => text
                .parse::<TopoName>()
                .map(Self::Topo)
                .map_err(D::Error::custom),
            other => SketchEntityRef::from_json(&other)
                .map(Self::Sketch)
                .map_err(D::Error::custom),
        }
    }
}

/// Model geometry projected into the sketch whose entities follow the
/// source (a linked projection). Unlinked projections are plain
/// fixed reference entities and need no record; their persistent
/// `Entity::reference` flag preserves their projected origin.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Projection {
    /// An edge, a face (its boundary edges) or a vertex; or a curve or
    /// point of an earlier sketch (mitcad#904).
    pub source: ProjectionSource,
    /// The body of the source; any body when left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<BodyUid>,
    /// The entities made from the source, in the order the kernel lists its
    /// curves.
    #[serde(with = "uids_serde")]
    pub entities: Vec<Ref>,
    /// Where the source is when it is geometry of another component
    /// (mitcad#100, `links.rs`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<crate::occurrence::OccurrenceLink>,
    /// Its entities follow the source by geometry, each the fresh curve of
    /// its kind most like it (mitcad#232): projections made with Project
    /// since 0.4.1. Without it (files of earlier versions, imports) they
    /// follow only a source whose curves come back of the same kinds, in
    /// the same order and joined the same way (mitcad#249).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub by_geometry: bool,
    /// Section curves (mitcad#389): the entities are where the sketch's
    /// plane cuts the source, a face (every face of a name without `#k`),
    /// not the source projected onto it ([`mitcad_model_base::Kernel::section_curves`]);
    /// of an edge, the points where the plane crosses it
    /// ([`mitcad_model_base::Kernel::section_points`], mitcad#736).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub section: bool,
    /// Edges the source continues into, in order (mitcad#389): a curve the
    /// model splits into pieces (a thread's helix, an edge per turn) is one
    /// curve of the sketch, the projection of the edges joined end to end
    /// ([`project::join_curves`]). Each name gives one curve.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub chain: Vec<TopoName>,
    /// The source as an earlier feature left its body (mitcad#998): the
    /// body's shape right after that feature, not as the features after
    /// it changed it before the sketch (an import's: a FreeCAD sketch's
    /// external geometry on an earlier feature's face). Only with a body
    /// element (`source` a face, an edge or a vertex) and `body`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<crate::ids::FeatureUid>,
}

/// A sketch text whose text is a text parameter's value (mitcad#664):
/// `{"text": "t5", "value": "Label"}` in a sketch's `text_values`. The
/// parameter is a text parameter of the document or the sketch's own one
/// for an expression (its slot is `texts[t5].text`); the text follows it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextValue<P> {
    #[serde(with = "text_serde")]
    pub text: EntityUid,
    pub value: P,
}

impl<P> TextValue<P> {
    /// The slot of its value: `texts[t5].text`.
    pub fn slot(&self) -> String {
        format!("texts[t{}].text", self.text.0)
    }

    pub fn map_value<Q, E>(
        &self,
        f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
    ) -> Result<TextValue<Q>, E> {
        Ok(TextValue {
            text: self.text,
            value: f(&self.slot(), &self.value)?,
        })
    }
}

/// Text: the outlines of its characters' glyphs
/// bound profiles (`text.rs`). Lines are separated by `\n`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SketchText {
    #[serde(with = "text_serde")]
    pub id: EntityUid,
    pub text: String,
    /// The anchor: the start of the first line's baseline by default (see
    /// `align`, `valign`). Unused by a text in a frame or on a path.
    pub at: [f64; 2],
    /// The font size (its em) in millimetres.
    pub height: f64,
    /// Rotation from the sketch x axis, radians.
    #[serde(default)]
    pub angle: f64,
    /// The font family; empty for the bundled default font.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub font: String,
    #[serde(default, skip_serializing_if = "is_false")]
    pub bold: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub italic: bool,
    /// Horizontal alignment: of the lines about the anchor, in the frame,
    /// or along the path.
    #[serde(default, skip_serializing_if = "TextAlign::is_default")]
    pub align: TextAlign,
    /// Vertical alignment about the anchor or in the frame.
    #[serde(default, skip_serializing_if = "TextVAlign::is_default")]
    pub valign: TextVAlign,
    /// Additional character spacing, percent of the font's.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub spacing: f64,
    /// A text in a frame (multi-line text): its corner, the corner
    /// along its x side and the corner along its y side, points of the
    /// sketch (the frame's construction rectangle), so the text follows
    /// the frame.
    #[serde(default, skip_serializing_if = "Vec::is_empty", with = "points_serde")]
    pub frame: Vec<EntityUid>,
    /// A text along a curve.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<TextPath>,
    /// Mirrored left to right, or upside down (the flips).
    #[serde(default, skip_serializing_if = "is_false")]
    pub flip_x: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub flip_y: bool,
    /// `height` is the height of the capital letters (the font's
    /// em is that height over the font's capital height) and a text in a
    /// frame aligns vertically by them: its top is the capitals' top, its
    /// bottom the last baseline. The `.f3d` import sets it.
    #[serde(default, skip_serializing_if = "is_false")]
    pub cap_height: bool,
    /// A text in a frame breaks its lines at spaces to fit the frame's
    /// width (a word wider than the frame stays whole). The `.f3d` import
    /// sets it.
    #[serde(default, skip_serializing_if = "is_false")]
    pub wrap: bool,
    /// The distance from one line's baseline to the next, mm; 0 for the
    /// font's (see `cap_height`). The `.ipt` import sets it from the
    /// baselines the file keeps (mitcad#414).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub line_spacing: f64,
    /// Lines align by their pens (from the first character's pen position
    /// to the end of the last one's advance) also with `cap_height`, where
    /// they align by their outlines otherwise. The `.ipt` import sets it.
    #[serde(default, skip_serializing_if = "is_false")]
    pub pen_align: bool,
    /// The top of the lines (a frame's top, `valign` top or middle) is the
    /// first line's highest outline point, not the capitals' top (with
    /// `cap_height`) or the font's ascender. The `.ipt` import sets it.
    #[serde(default, skip_serializing_if = "is_false")]
    pub ink_top: bool,
    /// The outlines a file stores for the text's characters (mitcad#416),
    /// used as their shapes while the font is not installed and the text
    /// is still the one they are the outlines of (`text.rs`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub glyphs: Option<StoredGlyphs>,
    /// The font file the text's font comes from (mitcad#473), instead of
    /// an installed font: `font` is the file's family, and the outlines
    /// are stored in `glyphs`, so the design looks the same without it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_file: Option<FontFile>,
    /// A code of the text's content instead of its characters (mitcad#473,
    /// `code.rs`): `height` is a module's size, `at` the lower left corner
    /// of the modules, turned by `angle`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<SketchCode>,
}

/// A code a sketch text draws (mitcad#473).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SketchCode {
    #[serde(default)]
    pub kind: CodeKind,
    /// The error correction level.
    #[serde(default)]
    pub ecc: CodeEcc,
}

/// The kinds of codes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodeKind {
    #[default]
    Qr,
}

/// A QR code's error correction: about 7, 15, 25 or 30 % of it may be
/// lost.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum CodeEcc {
    L,
    #[default]
    M,
    Q,
    H,
}

/// A font file a text uses (mitcad#473; `fonts.rs`): where it was read and
/// what its licence says, kept with the design.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FontFile {
    /// The file's path when it was read (`.ttf`, `.otf`, `.ttc`, `.woff`
    /// or `.woff2`).
    pub path: String,
    /// The face in a collection.
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub index: u32,
    /// The font's copyright and licence notes (its name table).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub licence: String,
    /// The file was chosen in this session (`text::use_font_file`), so it
    /// may be read again for a new string or style. A path read from a
    /// design file never is (mitcad#929): such a text needs its stored
    /// outlines, or the font file chosen again. Not saved, and not part of
    /// the equality: the same file whether chosen now or before.
    #[serde(skip)]
    pub chosen: bool,
}

impl PartialEq for FontFile {
    fn eq(&self, other: &Self) -> bool {
        self.path == other.path && self.index == other.index && self.licence == other.licence
    }
}

impl FontFile {
    /// The file's name, without its folder.
    pub fn name(&self) -> &str {
        self.path
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(self.path.as_str())
    }
}

fn is_zero_u32(value: &u32) -> bool {
    *value == 0
}

/// The glyph outlines a file stores with a text (mitcad#416): what the
/// characters of `text` look like in `font`, so a text keeps its shape on
/// a computer without that font. Lengths are in units of the text's
/// `height` (a glyph 0.7 high is 0.7 × `height` mm high), with the pen at
/// the origin on the baseline and y up.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoredGlyphs {
    /// The string, family and style they are the outlines of.
    pub text: String,
    pub font: String,
    #[serde(default, skip_serializing_if = "is_false")]
    pub bold: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub italic: bool,
    /// One per character of `text`, newlines included.
    pub glyphs: Vec<StoredGlyph>,
    /// The font's vertical metrics in the same units, when they were
    /// stored (a font file's, mitcad#473): above and below the baseline
    /// and from one baseline to the next. Zero: not stored.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub ascender: f64,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub descender: f64,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub line_spacing: f64,
}

/// One character's stored outline (see [`StoredGlyphs`]).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoredGlyph {
    /// From its pen to the next character's.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub advance: f64,
    /// Closed contours of Bézier pieces (two control points for a line,
    /// three or four for a quadratic or cubic curve), each piece starting
    /// where the one before it ends.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub contours: Vec<Vec<Vec<[f64; 2]>>>,
}

impl StoredGlyphs {
    /// Whether they are the outlines of a text as it is now.
    pub fn fit(&self, text: &SketchText) -> bool {
        self.text == text.text
            && self.font == text.font
            && self.bold == text.bold
            && self.italic == text.italic
            && self.glyphs.len() == text.text.chars().count()
    }
}

impl SketchText {
    /// A one-line text at a point with the default options.
    pub fn new(id: EntityUid, text: &str, at: [f64; 2], height: f64) -> Self {
        Self {
            id,
            text: text.to_owned(),
            at,
            height,
            angle: 0.0,
            font: String::new(),
            bold: false,
            italic: false,
            align: TextAlign::default(),
            valign: TextVAlign::default(),
            spacing: 0.0,
            frame: Vec::new(),
            path: None,
            flip_x: false,
            flip_y: false,
            cap_height: false,
            wrap: false,
            line_spacing: 0.0,
            pen_align: false,
            ink_top: false,
            glyphs: None,
            font_file: None,
            code: None,
        }
    }
}

fn is_zero(value: &f64) -> bool {
    *value == 0.0
}

/// Horizontal text alignment.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
}

impl TextAlign {
    fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// Vertical text alignment: the first line's baseline at the anchor, or
/// the top, middle or bottom of the lines (their ascender and descender).
/// In a frame, `baseline` is `top`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextVAlign {
    #[default]
    Baseline,
    Bottom,
    Middle,
    Top,
}

impl TextVAlign {
    fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// A text along a curve: on its left side (`above`, in the curve's
/// direction) or hanging below it; `fit` spreads the characters over the
/// whole curve.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextPath {
    #[serde(with = "curve_serde")]
    pub curve: EntityUid,
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub above: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub fit: bool,
}

fn yes() -> bool {
    true
}

fn is_true(value: &bool) -> bool {
    *value
}

/// Text ids: `"t7"`.
pub(crate) mod text_serde {
    use crate::ids::{EntityUid, IdError};
    use mitcad_model_base::internal::ids::parse_number;

    pub fn serialize<S: serde::Serializer>(id: &EntityUid, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&format!("t{}", id.0))
    }

    pub fn deserialize<'de, D: serde::Deserializer<'de>>(d: D) -> Result<EntityUid, D::Error> {
        let text: String = serde::Deserialize::deserialize(d)?;
        parse_number(&text, "t")
            .map(EntityUid)
            .ok_or_else(|| IdError::new("text", &text, "t<number>"))
            .map_err(serde::de::Error::custom)
    }
}

/// Looks up entities of a sketch by id.
pub struct EntityIndex<'a> {
    by_id: BTreeMap<EntityUid, &'a Entity>,
}

impl<'a> EntityIndex<'a> {
    pub fn new(entities: &'a [Entity]) -> Self {
        Self {
            by_id: entities.iter().map(|e| (e.id, e)).collect(),
        }
    }

    pub fn get(&self, id: EntityUid) -> Option<&'a Entity> {
        self.by_id.get(&id).copied()
    }

    /// The position of a point.
    pub fn point(&self, id: EntityUid) -> Option<[f64; 2]> {
        match self.get(id)?.kind {
            EntityKind::Point { at } => Some(at),
            _ => None,
        }
    }
}
