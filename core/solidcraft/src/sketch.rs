// SPDX-License-Identifier: MIT
//! A sketch (SolidCraft's `crates/sketch/src/model.rs`): points, curves and
//! constraints. Curves and constraints refer to points and curves by their
//! index in the lists; point 0 is the fixed sketch origin (`origin`).
//! Coordinates are millimetres in the sketch plane, angles radians.

use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Sketch {
    #[serde(default)]
    pub points: Vec<Point>,
    #[serde(default)]
    pub curves: Vec<Curve>,
    #[serde(default)]
    pub constraints: Vec<Constraint>,
    /// Editable sketch patterns.
    #[serde(default)]
    pub patterns: Vec<Value>,
    /// Links to geometry outside the sketch (projections, intersections,
    /// texts).
    #[serde(default)]
    pub links: Vec<Value>,
    /// Associative offsets.
    #[serde(default)]
    pub offsets: Vec<Value>,
    #[serde(default)]
    pub freeform_offsets: Vec<Value>,
    /// 3D curves, each read by [`Wire::read`] (one that does not read is
    /// left out alone).
    #[serde(default)]
    pub wires: Vec<Value>,
    /// Relations and dimensions among the drawn 3D curves' fit points
    /// (format 58), each read by [`WireConstraint::read`].
    #[serde(default)]
    pub wire_constraints: Vec<Value>,
    /// The drawn 3D curves that met were joined once by coincident
    /// relations (format 58): only those relations join them.
    #[serde(default)]
    pub wires_joined: bool,
    /// Equation curves (format 44): the fit splines of `curves` they
    /// place, with their formulas.
    #[serde(default)]
    pub equations: Vec<Value>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Point {
    pub id: String,
    pub pos: [f64; 2],
    #[serde(default)]
    pub fixed: bool,
    /// The link (projection) that owns the point.
    #[serde(default)]
    pub link: Option<String>,
    /// A point placed by parameters.
    #[serde(default)]
    pub exprs: Option<PointExprs>,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct PointExprs {
    #[serde(default)]
    pub x: Option<String>,
    #[serde(default)]
    pub y: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CurveKind {
    Line {
        a: usize,
        b: usize,
    },
    Circle {
        c: usize,
        r: f64,
    },
    /// Counter-clockwise about `c` from `a` to `b`.
    Arc {
        c: usize,
        a: usize,
        b: usize,
    },
    /// Centre `c`, end of the major axis `m`, minor radius `r`.
    Ellipse {
        c: usize,
        m: usize,
        r: f64,
    },
    /// Through its points (fit points), or with `control` a clamped
    /// B-spline of `degree` over them.
    Spline {
        pts: Vec<usize>,
        #[serde(default)]
        control: bool,
        #[serde(default = "three")]
        degree: u8,
        #[serde(default)]
        periodic: bool,
        /// A control spline's own knots and weights (format 49); none: the
        /// clamped uniform B-spline.
        #[serde(default)]
        nurbs: Option<Nurbs>,
        /// A fit spline's dragged tangent handles (format 55), points by
        /// index; none: the tangents its fit gives.
        #[serde(default)]
        handles: Vec<usize>,
    },
    /// A conic from `a` to `b` shaped by `apex` and `rho`.
    Conic {
        a: usize,
        b: usize,
        apex: usize,
        rho: f64,
    },
}

fn three() -> u8 {
    3
}

/// A control spline's knots (clamped: the point count plus the degree plus
/// one) and weights (one per point; none: all 1).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Nurbs {
    pub knots: Vec<f64>,
    #[serde(default)]
    pub weights: Vec<f64>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Curve {
    pub id: String,
    #[serde(flatten)]
    pub kind: CurveKind,
    #[serde(default)]
    pub construction: bool,
    /// An arc drawn clockwise (stored counter-clockwise).
    #[serde(default)]
    pub reversed: bool,
    #[serde(default)]
    pub link: Option<String>,
    #[serde(default)]
    pub centerline: bool,
    #[serde(default)]
    pub fixed: bool,
    #[serde(default)]
    pub infinite: bool,
}

impl Curve {
    /// Not part of profiles.
    pub fn is_construction(&self) -> bool {
        self.construction || self.centerline || self.infinite
    }
}

/// Constraints and dimensions; `p`, `q` are points, `l`, `c`, `a`, `b`
/// curves (indices).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ConstraintKind {
    PatternPoint {
        p: usize,
        q: usize,
    },
    PatternRadius {
        a: usize,
        b: usize,
    },
    Coincident {
        p: usize,
        q: usize,
    },
    PointOnCurve {
        p: usize,
        c: usize,
    },
    Horizontal {
        l: usize,
    },
    Vertical {
        l: usize,
    },
    HorizontalPoints {
        p: usize,
        q: usize,
    },
    VerticalPoints {
        p: usize,
        q: usize,
    },
    Parallel {
        a: usize,
        b: usize,
    },
    Perpendicular {
        a: usize,
        b: usize,
    },
    Collinear {
        a: usize,
        b: usize,
    },
    Tangent {
        a: usize,
        b: usize,
    },
    Equal {
        a: usize,
        b: usize,
    },
    Concentric {
        a: usize,
        b: usize,
    },
    Midpoint {
        p: usize,
        l: usize,
    },
    Symmetric {
        p: usize,
        q: usize,
        l: usize,
    },
    Fix {
        p: usize,
    },
    Distance {
        p: usize,
        q: usize,
        value: f64,
    },
    DistanceX {
        p: usize,
        q: usize,
        value: f64,
    },
    DistanceY {
        p: usize,
        q: usize,
        value: f64,
    },
    PointLineDistance {
        p: usize,
        l: usize,
        value: f64,
    },
    Length {
        l: usize,
        value: f64,
    },
    /// The signed angle of a line's direction from the sketch's +X.
    LineAngle {
        l: usize,
        value: f64,
    },
    /// The signed sweep of an arc.
    ArcSweep {
        c: usize,
        value: f64,
    },
    SlotOverallLength {
        a: usize,
        b: usize,
        value: f64,
    },
    Radius {
        c: usize,
        value: f64,
    },
    Diameter {
        c: usize,
        value: f64,
    },
    /// From line `a` to line `b`, counter-clockwise; `flip` measures to one
    /// line reversed.
    Angle {
        a: usize,
        b: usize,
        value: f64,
        #[serde(default)]
        flip: bool,
    },
    Smooth {
        a: usize,
        b: usize,
    },
    ArcLength {
        c: usize,
        value: f64,
    },
    LinearDiameter {
        p: usize,
        l: usize,
        value: f64,
    },
    OffsetDistance {
        output: String,
        value: f64,
    },
    /// From line `l` to the edge of circle or arc `c` (format 28); with
    /// `inside`, how far the edge reaches past the line.
    LineCircleDistance {
        l: usize,
        c: usize,
        value: f64,
        #[serde(default)]
        inside: bool,
    },
    /// From point `p` to the edge of circle or arc `c` (format 36).
    PointCircleDistance {
        p: usize,
        c: usize,
        value: f64,
        #[serde(default)]
        inside: bool,
    },
    /// Between the edges of circles or arcs `a` and `b` (format 36):
    /// `circles` apart (none), `crossing` or `nested`.
    CircleCircleDistance {
        a: usize,
        b: usize,
        value: f64,
        #[serde(default)]
        circles: Option<String>,
    },
    /// Point `p` at the share `at` (0 to 1) of curve `c`'s length (format
    /// 51).
    PointAlong {
        p: usize,
        c: usize,
        at: f64,
    },
    /// A kind of a later format than this reader knows: left out of the
    /// sketch, which keeps the rest.
    #[serde(other)]
    Other,
}

impl ConstraintKind {
    pub fn is_dimension(&self) -> bool {
        self.value().is_some()
    }

    pub fn value(&self) -> Option<f64> {
        use ConstraintKind::*;
        match *self {
            Distance { value, .. }
            | DistanceX { value, .. }
            | DistanceY { value, .. }
            | PointLineDistance { value, .. }
            | Length { value, .. }
            | LineAngle { value, .. }
            | ArcSweep { value, .. }
            | SlotOverallLength { value, .. }
            | Radius { value, .. }
            | Diameter { value, .. }
            | ArcLength { value, .. }
            | LinearDiameter { value, .. }
            | OffsetDistance { value, .. }
            | LineCircleDistance { value, .. }
            | PointCircleDistance { value, .. }
            | CircleCircleDistance { value, .. }
            | Angle { value, .. } => Some(value),
            _ => None,
        }
    }

    pub fn is_angle(&self) -> bool {
        matches!(
            self,
            ConstraintKind::Angle { .. }
                | ConstraintKind::LineAngle { .. }
                | ConstraintKind::ArcSweep { .. }
        )
    }

    /// The kind's name in the file.
    pub fn name(&self) -> &'static str {
        use ConstraintKind::*;
        match self {
            PatternPoint { .. } => "pattern_point",
            PatternRadius { .. } => "pattern_radius",
            Coincident { .. } => "coincident",
            PointOnCurve { .. } => "point_on_curve",
            Horizontal { .. } => "horizontal",
            Vertical { .. } => "vertical",
            HorizontalPoints { .. } => "horizontal_points",
            VerticalPoints { .. } => "vertical_points",
            Parallel { .. } => "parallel",
            Perpendicular { .. } => "perpendicular",
            Collinear { .. } => "collinear",
            Tangent { .. } => "tangent",
            Equal { .. } => "equal",
            Concentric { .. } => "concentric",
            Midpoint { .. } => "midpoint",
            Symmetric { .. } => "symmetric",
            Fix { .. } => "fix",
            Distance { .. } => "distance",
            DistanceX { .. } => "distance_x",
            DistanceY { .. } => "distance_y",
            PointLineDistance { .. } => "point_line_distance",
            Length { .. } => "length",
            LineAngle { .. } => "line_angle",
            ArcSweep { .. } => "arc_sweep",
            SlotOverallLength { .. } => "slot_overall_length",
            Radius { .. } => "radius",
            Diameter { .. } => "diameter",
            Angle { .. } => "angle",
            Smooth { .. } => "smooth",
            ArcLength { .. } => "arc_length",
            LinearDiameter { .. } => "linear_diameter",
            OffsetDistance { .. } => "offset_distance",
            LineCircleDistance { .. } => "line_circle_distance",
            PointCircleDistance { .. } => "point_circle_distance",
            CircleCircleDistance { .. } => "circle_circle_distance",
            PointAlong { .. } => "point_along",
            Other => "a constraint of a later format",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Constraint {
    pub id: String,
    #[serde(flatten)]
    pub kind: ConstraintKind,
    /// A dimension's model parameter (`d1`).
    #[serde(default)]
    pub param: Option<String>,
    /// A driven (reference) dimension.
    #[serde(default)]
    pub driven: bool,
}

impl Sketch {
    /// The sketch's dimensions and constraints.
    pub fn counts(&self) -> (usize, usize) {
        let dims = self
            .constraints
            .iter()
            .filter(|c| c.kind.is_dimension())
            .count();
        (self.constraints.len() - dims, dims)
    }

    pub fn curve_index(&self, id: &str) -> Option<usize> {
        self.curves.iter().position(|c| c.id == id)
    }

    pub fn point_index(&self, id: &str) -> Option<usize> {
        self.points.iter().position(|p| p.id == id)
    }
}

/// The coordinates a 3D curve is in (SolidCraft's `WireSpace`): the
/// world's (the default, drawn or detached geometry), or the sketch's
/// component's.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WireSpace {
    #[default]
    World,
    Component,
}

/// A 3D curve of a sketch (SolidCraft's `crates/sketch/src/wire.rs`): a
/// drawn one through its fit points (one: a point, two: a line, more: a
/// spline; three with `arc`: an exact arc), or a linked one (`link`) with
/// its exact pieces or only its sampled points.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Wire {
    pub id: String,
    /// The sampled polyline.
    #[serde(default)]
    pub pts: Vec<[f64; 3]>,
    #[serde(default)]
    pub space: WireSpace,
    #[serde(default)]
    pub link: Option<String>,
    #[serde(default)]
    pub fit: Vec<[f64; 3]>,
    #[serde(default)]
    pub arc: Option<WireArc>,
    /// A linked curve's exact pieces, end to end.
    #[serde(default)]
    pub exact: Vec<WireCurve>,
}

impl Wire {
    /// A 3D curve as the file stores it.
    pub fn read(v: &Value) -> Result<Wire, String> {
        Wire::deserialize(v).map_err(|e| format!("a 3D curve that does not read: {e}"))
    }

    /// Its ends as the file has them: the first and last fit points of a
    /// drawn curve, else of its samples.
    pub fn ends(&self) -> Option<([f64; 3], [f64; 3])> {
        let pts = if self.fit.len() >= 2 {
            &self.fit
        } else {
            &self.pts
        };
        Some((*pts.first()?, *pts.last()?))
    }
}

/// An exact circular arc drawn in 3D: counter-clockwise about `normal`
/// from `start` to `end` round `center`.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct WireArc {
    pub center: [f64; 3],
    pub normal: [f64; 3],
    pub radius: f64,
    pub start: [f64; 3],
    pub end: [f64; 3],
}

/// One exact piece of a linked 3D curve: a curve on its interval.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct WireCurve {
    pub curve: RecordCurve,
    pub interval: [f64; 2],
}

/// A frame of a record curve: origin and axes.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct RecordFrame {
    pub origin: [f64; 3],
    pub x_axis: [f64; 3],
    pub y_axis: [f64; 3],
}

/// The curves of SolidCraft's records (`solidcore_record::brep::Curve`),
/// each with its parameter `t`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RecordCurve {
    /// `start + t (end - start)`.
    Line { start: [f64; 3], end: [f64; 3] },
    /// `origin + radius (cos t x + sin t y)`.
    Circle { frame: RecordFrame, radius: f64 },
    /// `origin + radii[0] cos t x + radii[1] sin t y`.
    Ellipse { frame: RecordFrame, radii: [f64; 2] },
    /// Straight segments, segment i on [i, i + 1].
    Polyline { points: Vec<[f64; 3]> },
    /// A B-spline on its knots (its degree: knots less points less one).
    Spline {
        knots: Vec<f64>,
        points: Vec<[f64; 3]>,
    },
    /// A NURBS curve by homogeneous `[x w, y w, z w, w]` coefficients.
    RationalSpline {
        knots: Vec<f64>,
        homogeneous: Vec<[f64; 4]>,
    },
    /// An intersection curve of two surfaces (known here only near its
    /// leader), or a kind of a later format.
    #[serde(other)]
    Other,
}

/// One fit point of a drawn 3D curve: `index` into its `fit`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Deserialize)]
pub struct WirePoint {
    pub wire: String,
    pub index: usize,
}

/// The expressions placing a 3D point, each coordinate of its curve's
/// space.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct PointExprs3 {
    #[serde(default)]
    pub x: Option<String>,
    #[serde(default)]
    pub y: Option<String>,
    #[serde(default)]
    pub z: Option<String>,
}

/// A relation or dimension among drawn 3D curves (format 58, SolidCraft's
/// `crates/sketch/src/wire_solve.rs`). Axes are `x`, `y`, `z`; origin
/// planes `xy`, `yz`, `xz`; lines and arcs by their 3D curve ids; values
/// in mm and radians.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WireConstraintKind {
    Coincident {
        a: WirePoint,
        b: WirePoint,
    },
    /// On the infinite line through a drawn line.
    PointOnLine {
        p: WirePoint,
        line: String,
    },
    /// Held at `at`; with `exprs` those coordinates follow parameters.
    Fix {
        p: WirePoint,
        at: [f64; 3],
        #[serde(default)]
        exprs: Option<PointExprs3>,
    },
    AlongAxis {
        line: String,
        axis: String,
    },
    Distance {
        a: WirePoint,
        b: WirePoint,
        value: f64,
    },
    /// From `a` to `b` along an axis; `reversed`: `b` before `a`.
    AxisDistance {
        a: WirePoint,
        b: WirePoint,
        axis: String,
        #[serde(default)]
        reversed: bool,
        value: f64,
    },
    /// Between two lines' directions, each start to end (end to start
    /// where reversed).
    Angle {
        a: String,
        b: String,
        #[serde(default)]
        a_reversed: bool,
        #[serde(default)]
        b_reversed: bool,
        value: f64,
    },
    Radius {
        arc: String,
        value: f64,
    },
    Parallel {
        a: String,
        b: String,
    },
    Perpendicular {
        a: String,
        b: String,
    },
    Collinear {
        a: String,
        b: String,
    },
    /// Equal lengths of lines, or with `arcs` equal radii.
    Equal {
        a: String,
        b: String,
        #[serde(default)]
        arcs: bool,
    },
    /// A line's end (0 start, 1 end) tangent to an arc's end (0 start, 2
    /// end) where they meet.
    Tangent {
        line: String,
        arc: String,
        line_end: usize,
        arc_end: usize,
    },
    ParallelToPlane {
        line: String,
        plane: String,
    },
    OnPlane {
        p: WirePoint,
        plane: String,
    },
    /// The signed distance from an origin plane along its normal
    /// (`reversed`: behind it).
    PlaneDistance {
        p: WirePoint,
        plane: String,
        #[serde(default)]
        reversed: bool,
        value: f64,
    },
    PlaneAngle {
        line: String,
        plane: String,
        #[serde(default)]
        reversed: bool,
        value: f64,
    },
    /// A kind of a later format.
    #[serde(other)]
    Other,
}

impl WireConstraintKind {
    /// The kind's name in the file.
    pub fn name(&self) -> &'static str {
        use WireConstraintKind::*;
        match self {
            Coincident { .. } => "coincident",
            PointOnLine { .. } => "point_on_line",
            Fix { .. } => "fix",
            AlongAxis { .. } => "along_axis",
            Distance { .. } => "distance",
            AxisDistance { .. } => "axis_distance",
            Angle { .. } => "angle",
            Radius { .. } => "radius",
            Parallel { .. } => "parallel",
            Perpendicular { .. } => "perpendicular",
            Collinear { .. } => "collinear",
            Equal { .. } => "equal",
            Tangent { .. } => "tangent",
            ParallelToPlane { .. } => "parallel_to_plane",
            OnPlane { .. } => "on_plane",
            PlaneDistance { .. } => "plane_distance",
            PlaneAngle { .. } => "plane_angle",
            Other => "a 3D relation of a later format",
        }
    }

    /// A dimension's value (mm or radians).
    pub fn value(&self) -> Option<f64> {
        use WireConstraintKind::*;
        match *self {
            Distance { value, .. }
            | AxisDistance { value, .. }
            | Angle { value, .. }
            | Radius { value, .. }
            | PlaneDistance { value, .. }
            | PlaneAngle { value, .. } => Some(value),
            _ => None,
        }
    }
}

/// A 3D relation or dimension with its id, its parameter (a driving
/// dimension's) and whether it is driven.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct WireConstraint {
    pub id: String,
    #[serde(flatten)]
    pub kind: WireConstraintKind,
    #[serde(default)]
    pub param: Option<String>,
    #[serde(default)]
    pub driven: bool,
}

impl WireConstraint {
    /// A 3D relation as the file stores it.
    pub fn read(v: &Value) -> Result<WireConstraint, String> {
        WireConstraint::deserialize(v).map_err(|e| format!("a 3D relation that does not read: {e}"))
    }
}
