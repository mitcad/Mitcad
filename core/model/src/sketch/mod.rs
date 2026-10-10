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
//!   one only measures.
//! - Stored positions are the last solved ones and the start of every
//!   solve, so the same definition and values always give the same
//!   geometry (`solve.rs`).
//! - Profile regions (`regions.rs`): the non-construction curves are cut at
//!   their intersections into segments, and the faces of that planar
//!   arrangement, with their holes, are the regions. Segment and region
//!   keys (see [`crate::topo`]) name them from curve ids only.

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

#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::fmt;

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Value};

use crate::ids::BodyUid;
use crate::ids::{EntityUid, IdError, parse_number};
use crate::topo::TopoName;

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

crate::ids::string_serde!(ConstraintUid, "a constraint id like \"k3\"");

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

crate::ids::string_serde!(Ref, "an entity like \"p3\" or \"c4\"");

/// Point ids in files and commands: `"p3"`.
pub(crate) mod point_serde {
    use crate::ids::{EntityUid, IdError, parse_number};

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

use crate::ids::curve_serde;

/// Lists of curve ids: `["c3", "c4"]`.
pub(crate) mod curves_serde {
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
    FittedSpline {
        #[serde(with = "points_serde")]
        points: Vec<EntityUid>,
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
            Self::FittedSpline { points } => points.clone(),
        }
    }

    /// The start and end points of an open curve.
    pub fn ends(&self) -> Option<(EntityUid, EntityUid)> {
        match self {
            Self::Line { start, end, .. }
            | Self::Arc { start, end, .. }
            | Self::EllipticalArc { start, end, .. } => Some((*start, *end)),
            Self::Spline { control: p, .. } | Self::FittedSpline { points: p } => {
                Some((*p.first()?, *p.last()?))
            }
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
            | Self::FittedSpline { points } => {
                points.iter_mut().for_each(swap);
            }
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
}

impl Entity {
    pub fn new(id: EntityUid, kind: EntityKind) -> Self {
        Self {
            id,
            kind,
            construction: false,
            fixed: false,
            reference: false,
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
}

impl Serialize for Entity {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        EntityOut {
            id: self.as_ref(),
            kind: &self.kind,
            construction: self.construction,
            fixed: self.fixed,
            reference: self.reference,
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
    Tangent {
        #[serde(with = "curve_serde")]
        a: EntityUid,
        #[serde(with = "curve_serde")]
        b: EntityUid,
    },
    /// Curvature continuity (G2) where a spline joins another curve.
    Smooth {
        #[serde(with = "curve_serde")]
        a: EntityUid,
        #[serde(with = "curve_serde")]
        b: EntityUid,
    },
    /// Equal length (lines) or radius (circles, arcs).
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
    Symmetric {
        a: Ref,
        b: Ref,
        #[serde(with = "curve_serde")]
        axis: EntityUid,
    },
}

impl ConstraintKind {
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
            | Tangent { a, b }
            | Smooth { a, b }
            | Equal { a, b }
            | Concentric { a, b } => vec![c(*a), c(*b)],
            Midpoint { point, curve } => vec![p(*point), c(*curve)],
            Symmetric { a, b, axis } => vec![*a, *b, c(*axis)],
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
}

#[derive(Serialize)]
struct ConstraintOut<'a> {
    id: ConstraintUid,
    #[serde(flatten)]
    kind: &'a ConstraintKind,
}

impl Serialize for Constraint {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        ConstraintOut {
            id: self.id,
            kind: &self.kind,
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

impl<'de> Deserialize<'de> for Constraint {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let mut map = Map::deserialize(deserializer)?;
        let id = take_id(&mut map)?;
        let kind = serde_json::from_value(Value::Object(map)).map_err(D::Error::custom)?;
        Ok(Self { id, kind })
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
    /// `|Δx|` between two points in sketch axes.
    HorizontalDistance {
        #[serde(with = "point_serde")]
        a: EntityUid,
        #[serde(with = "point_serde")]
        b: EntityUid,
    },
    /// `|Δy|` between two points in sketch axes.
    VerticalDistance {
        #[serde(with = "point_serde")]
        a: EntityUid,
        #[serde(with = "point_serde")]
        b: EntityUid,
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
    /// Angle between two lines, 0 to 180 degrees, in the quadrant where the
    /// lines are when it is added.
    Angle {
        #[serde(with = "curve_serde")]
        a: EntityUid,
        #[serde(with = "curve_serde")]
        b: EntityUid,
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
}

/// How a dimension value must be: positive, not negative, or an angle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueRange {
    Positive,
    NotNegative,
    Angle,
}

impl DimensionKind {
    pub fn refs(&self) -> Vec<Ref> {
        use DimensionKind::*;
        let p = Ref::Point;
        let c = Ref::Curve;
        match self {
            Distance { a, b } | HorizontalDistance { a, b } | VerticalDistance { a, b } => {
                vec![p(*a), p(*b)]
            }
            PointLineDistance { point, line } => vec![p(*point), c(*line)],
            LineDistance { a, b } | Angle { a, b } => vec![c(*a), c(*b)],
            Length { line } => vec![c(*line)],
            Radius { curve } | Diameter { curve } => vec![c(*curve)],
            ArcLength { arc } => vec![c(*arc)],
            MajorRadius { ellipse } | MinorRadius { ellipse } => vec![c(*ellipse)],
            LinearDiameter { axis, entity } => vec![c(*axis), *entity],
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
            Length { .. } => "length",
            Angle { .. } => "angle",
            Radius { .. } => "radius",
            Diameter { .. } => "diameter",
            ArcLength { .. } => "arc_length",
            MajorRadius { .. } => "major_radius",
            MinorRadius { .. } => "minor_radius",
            LinearDiameter { .. } => "linear_diameter",
        }
    }

    pub fn range(&self) -> ValueRange {
        use DimensionKind::*;
        match self {
            HorizontalDistance { .. }
            | VerticalDistance { .. }
            | PointLineDistance { .. }
            | LineDistance { .. } => ValueRange::NotNegative,
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
    /// Where the value is shown, in sketch coordinates.
    pub text: Option<[f64; 2]>,
}

impl<P> Dimension<P> {
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
            text: self.text,
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
    text: Option<[f64; 2]>,
}

impl<P: Serialize> Serialize for Dimension<P> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        DimensionOut {
            id: self.id,
            kind: &self.kind,
            value: self.value.as_ref(),
            driven: self.value.is_none(),
            text: self.text,
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
        let text = take_field(&mut map, "text")
            .map(|v| serde_json::from_value::<[f64; 2]>(v).map_err(D::Error::custom))
            .transpose()?;
        let kind = serde_json::from_value(Value::Object(map)).map_err(D::Error::custom)?;
        match (driven, &value) {
            (true, Some(_)) => Err(D::Error::custom(format!(
                "{id}: a driven dimension has no \"value\""
            ))),
            (false, None) => Err(D::Error::custom(format!(
                "{id}: a driving dimension needs a \"value\" (or \"driven\": true)"
            ))),
            _ => Ok(Self {
                id,
                kind,
                value,
                text,
            }),
        }
    }
}

/// Model geometry projected into the sketch whose entities follow the
/// source (a linked projection). Unlinked projections are plain
/// fixed reference entities and need no record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Projection {
    /// An edge, a face (its boundary edges) or a vertex.
    pub source: TopoName,
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
    pub link: Option<crate::links::OccurrenceLink>,
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
    use crate::ids::{EntityUid, IdError, parse_number};

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
