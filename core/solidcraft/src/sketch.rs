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
    /// 3D curves.
    #[serde(default)]
    pub wires: Vec<Value>,
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
