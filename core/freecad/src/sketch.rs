// SPDX-License-Identifier: MIT
//! Sketcher sketches (`Sketcher::SketchObject`): their geometry,
//! constraints and external geometry as the file keeps them.
//!
//! - `Geometry` (`Part::PropertyGeometryList`): `<GeometryList><Geometry
//!   type="Part::GeomLineSegment" [id]>` with the sketcher's extension
//!   (`<GeoExtension type="Sketcher::SketchGeometryExtension"
//!   internalGeometryType geometryModeFlags/>`: the flags' bit 0 blocked,
//!   bit 1 construction, written as a bit string, bit 0 last), the curve
//!   (`<LineSegment StartX…/>`, `<ArcOfCircle Center… Normal… AngleXU
//!   Radius StartAngle EndAngle/>`, `<BSplineCurve><Pole/>…<Knot/>…`) and,
//!   from 1.0, `<Construction value/>` as well. Coordinates are the
//!   sketch's (z = 0), millimetres and radians.
//! - A conic's frame is OCCT's default one for its normal (the X direction
//!   OCCT picks for a plane through it: +X for +Z, −X for −Z) turned by
//!   `AngleXU` about the normal; a curve whose normal is −Z runs
//!   clockwise in the sketch, and the sketcher names its ends as if it ran
//!   counter-clockwise (its start is the end of its parameter range).
//! - GeoIds: the geometry's index (from 0); −1 and −2 are the sketch's H
//!   and V axes; −3, −4, … the external geometry. A point is a GeoId with
//!   a position: 1 start, 2 end, 3 centre (0: the whole curve); the root
//!   point is (−1, 1). [`GEO_UNDEF`] is an unused reference.
//! - `Constraints` (`Sketcher::PropertyConstraintList`): `<Constrain Name
//!   Type Value First FirstPos Second SecondPos Third ThirdPos IsDriving
//!   IsActive [InternalAlignmentType InternalAlignmentIndex]/>`. Values are
//!   millimetres, angles radians. 1.1 also writes the references as
//!   `ElementIds` and `ElementPositions`, read when the attributes are
//!   missing.
//! - External geometry: `ExternalGeometry` links to the elements of other
//!   objects (`Edge3`, `Vertex2`); from 1.0 `ExternalGeo` keeps the
//!   projected geometry in sketch coordinates, the two axes first, each
//!   entry with the reference it was made from (`ref`, the link's mapped
//!   name) and flags (bit 0: defining, part of the sketch's profiles).
//!   0.21 keeps only the links: one geometry each, projected when FreeCAD
//!   opens the file.

use serde::Serialize;

use crate::document::Object;
use crate::value::{LinkRef, Value};
use crate::xml::Element;

pub const H_AXIS: i32 = -1;
pub const V_AXIS: i32 = -2;
/// The first external geometry; the k-th (from 0) is `FIRST_EXTERNAL - k`.
pub const FIRST_EXTERNAL: i32 = -3;
/// No geometry: an unused reference of a constraint.
pub const GEO_UNDEF: i32 = -2000;

/// A point of a geometry, or the whole curve.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PointPos {
    None,
    Start,
    End,
    Mid,
}

impl PointPos {
    pub fn from_index(value: i64) -> PointPos {
        match value {
            1 => PointPos::Start,
            2 => PointPos::End,
            3 => PointPos::Mid,
            _ => PointPos::None,
        }
    }

    pub fn index(self) -> u8 {
        match self {
            PointPos::None => 0,
            PointPos::Start => 1,
            PointPos::End => 2,
            PointPos::Mid => 3,
        }
    }
}

/// A geometry or one of its points, as a constraint refers to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
pub struct GeoRef {
    pub geo: i32,
    pub pos: PointPos,
}

impl GeoRef {
    pub const UNDEF: GeoRef = GeoRef {
        geo: GEO_UNDEF,
        pos: PointPos::None,
    };
    pub const ROOT: GeoRef = GeoRef {
        geo: H_AXIS,
        pos: PointPos::Start,
    };

    pub fn new(geo: i32, pos: PointPos) -> GeoRef {
        GeoRef { geo, pos }
    }

    pub fn is_undef(&self) -> bool {
        self.geo == GEO_UNDEF
    }

    /// A point (rather than a whole curve or nothing).
    pub fn is_point(&self) -> bool {
        !self.is_undef() && self.pos != PointPos::None
    }

    /// The index of an external geometry (−3 → 0), None for others.
    pub fn external(&self) -> Option<usize> {
        (self.geo <= FIRST_EXTERNAL && !self.is_undef())
            .then(|| (FIRST_EXTERNAL - self.geo) as usize)
    }
}

/// What kind of constraint (the `Type` attribute: FreeCAD's numbering, the
/// same in 0.19 to 1.1; new kinds are appended).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConstraintType {
    None,
    Coincident,
    Horizontal,
    Vertical,
    Parallel,
    Tangent,
    Distance,
    DistanceX,
    DistanceY,
    Angle,
    Perpendicular,
    Radius,
    Equal,
    PointOnObject,
    Symmetric,
    InternalAlignment,
    SnellsLaw,
    Block,
    Diameter,
    Weight,
    Group,
    Text,
    Other(i64),
}

impl ConstraintType {
    const ALL: [ConstraintType; 22] = [
        Self::None,
        Self::Coincident,
        Self::Horizontal,
        Self::Vertical,
        Self::Parallel,
        Self::Tangent,
        Self::Distance,
        Self::DistanceX,
        Self::DistanceY,
        Self::Angle,
        Self::Perpendicular,
        Self::Radius,
        Self::Equal,
        Self::PointOnObject,
        Self::Symmetric,
        Self::InternalAlignment,
        Self::SnellsLaw,
        Self::Block,
        Self::Diameter,
        Self::Weight,
        Self::Group,
        Self::Text,
    ];

    pub fn from_index(value: i64) -> ConstraintType {
        usize::try_from(value)
            .ok()
            .and_then(|i| Self::ALL.get(i).copied())
            .unwrap_or(Self::Other(value))
    }

    /// FreeCAD's name of the type (what its Python interface says).
    pub fn name(&self) -> String {
        match self {
            Self::Other(n) => format!("type {n}"),
            other => format!("{other:?}"),
        }
    }

    /// Whether the constraint has a value (a dimension).
    pub fn is_dimension(&self) -> bool {
        matches!(
            self,
            Self::Distance
                | Self::DistanceX
                | Self::DistanceY
                | Self::Angle
                | Self::Radius
                | Self::Diameter
        )
    }
}

/// Internal alignment: how internal geometry belongs to its curve (the
/// `internalGeometryType` of the geometry and the `InternalAlignmentType`
/// of its constraint).
pub mod internal {
    pub const NONE: i64 = 0;
    pub const ELLIPSE_MAJOR_DIAMETER: i64 = 1;
    pub const ELLIPSE_MINOR_DIAMETER: i64 = 2;
    pub const ELLIPSE_FOCUS1: i64 = 3;
    pub const ELLIPSE_FOCUS2: i64 = 4;
    pub const HYPERBOLA_MAJOR: i64 = 5;
    pub const HYPERBOLA_MINOR: i64 = 6;
    pub const HYPERBOLA_FOCUS: i64 = 7;
    pub const PARABOLA_FOCUS: i64 = 8;
    pub const BSPLINE_CONTROL_POINT: i64 = 9;
    pub const BSPLINE_KNOT_POINT: i64 = 10;
    pub const PARABOLA_FOCAL_AXIS: i64 = 11;
}

/// A curve or point in sketch coordinates. Conics carry their frame's x
/// axis (the parameter's zero, the major axis) and whether they run
/// clockwise (normal −Z); arcs their parameter range.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Curve {
    Point {
        at: [f64; 2],
    },
    Line {
        start: [f64; 2],
        end: [f64; 2],
    },
    Circle {
        center: [f64; 2],
        radius: f64,
    },
    Arc {
        center: [f64; 2],
        radius: f64,
        frame: ConicFrame,
        start: f64,
        end: f64,
    },
    Ellipse {
        center: [f64; 2],
        major: f64,
        minor: f64,
        frame: ConicFrame,
    },
    ArcOfEllipse {
        center: [f64; 2],
        major: f64,
        minor: f64,
        frame: ConicFrame,
        start: f64,
        end: f64,
    },
    /// `center + major cosh t x + minor sinh t y`.
    ArcOfHyperbola {
        center: [f64; 2],
        major: f64,
        minor: f64,
        frame: ConicFrame,
        start: f64,
        end: f64,
    },
    /// `vertex + t² / (4 focal) x + t y`.
    ArcOfParabola {
        vertex: [f64; 2],
        focal: f64,
        frame: ConicFrame,
        start: f64,
        end: f64,
    },
    /// Distinct knots with their multiplicities; a periodic one has the
    /// first knot again at the end (one period).
    BSpline(BSpline),
    /// A Bézier curve: its poles and weights (empty: all 1).
    Bezier {
        poles: Vec<[f64; 2]>,
        weights: Vec<f64>,
    },
    /// A kind this reader does not take (an infinite line, an offset
    /// curve): its type.
    Other {
        type_name: String,
    },
}

/// A conic's frame in the sketch: unit x (the major axis) and y axes; y is
/// x turned a quarter clockwise for a curve whose normal is −Z.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct ConicFrame {
    pub x: [f64; 2],
    pub y: [f64; 2],
}

impl ConicFrame {
    /// OCCT's frame for a normal turned by `angle_xu` about it, seen from
    /// +Z.
    pub fn new(normal: [f64; 3], angle_xu: f64) -> ConicFrame {
        let n = unit3(normal).unwrap_or([0.0, 0.0, 1.0]);
        let x0 = default_x(n);
        // Rodrigues: x0 turned by the angle about n (x0 ⟂ n).
        let (s, c) = angle_xu.sin_cos();
        let nx = cross3(n, x0);
        let x: [f64; 3] = std::array::from_fn(|i| x0[i] * c + nx[i] * s);
        let y = cross3(n, x);
        ConicFrame {
            x: [x[0], x[1]],
            y: [y[0], y[1]],
        }
    }

    /// Whether the frame turns clockwise (its normal is −Z).
    pub fn is_clockwise(&self) -> bool {
        self.x[0] * self.y[1] - self.x[1] * self.y[0] < 0.0
    }

    /// The sketch point at frame coordinates (u, v) from `origin`.
    pub fn place(&self, origin: [f64; 2], u: f64, v: f64) -> [f64; 2] {
        [
            origin[0] + u * self.x[0] + v * self.y[0],
            origin[1] + u * self.x[1] + v * self.y[1],
        ]
    }

    /// The x axis's angle from the sketch's +X, counter-clockwise.
    pub fn angle(&self) -> f64 {
        self.x[1].atan2(self.x[0])
    }
}

fn cross3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn unit3(v: [f64; 3]) -> Option<[f64; 3]> {
    let n = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    (n > 0.0 && n.is_finite()).then(|| v.map(|c| c / n))
}

/// The X direction OCCT gives a frame with main direction `n` (`gp_Ax2`
/// from a point and a direction): at right angles to it, with a zero
/// coordinate where `n` is smallest.
fn default_x(n: [f64; 3]) -> [f64; 3] {
    let [a, b, c] = n;
    let (aa, ba, ca) = (a.abs(), b.abs(), c.abs());
    let d = if ba <= aa && ba <= ca {
        if aa > ca { [-c, 0.0, a] } else { [c, 0.0, -a] }
    } else if aa <= ba && aa <= ca {
        if ba > ca { [0.0, -c, b] } else { [0.0, c, -b] }
    } else if aa > ba {
        [-b, a, 0.0]
    } else {
        [b, -a, 0.0]
    };
    unit3(d).unwrap_or([1.0, 0.0, 0.0])
}

/// A B-spline of the sketch.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BSpline {
    pub degree: usize,
    pub poles: Vec<[f64; 2]>,
    /// One per pole.
    pub weights: Vec<f64>,
    pub knots: Vec<f64>,
    pub mults: Vec<usize>,
    pub periodic: bool,
}

/// A B-spline in the usual non-periodic form: its poles, weights and full
/// knot vector (`poles + degree + 1` knots), the curve on
/// `knots[degree]..=knots[poles]`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FlatSpline {
    pub degree: usize,
    pub poles: Vec<[f64; 2]>,
    pub weights: Vec<f64>,
    pub knots: Vec<f64>,
}

impl BSpline {
    /// Clamped at both ends: the curve starts at its first pole and ends at
    /// its last.
    pub fn is_clamped(&self) -> bool {
        !self.periodic
            && self.mults.first() == Some(&(self.degree + 1))
            && self.mults.last() == Some(&(self.degree + 1))
    }

    /// The non-periodic form. A periodic spline's poles wrap around (the
    /// first `degree` again at the end) over its knots extended by the
    /// period, which gives the same curve (OCCT's periodic B-splines start
    /// their first pole's basis function `degree` knots before the first
    /// knot). None when the knots and poles do not fit.
    pub fn flat(&self) -> Option<FlatSpline> {
        let p = self.degree;
        let n = self.poles.len();
        if p == 0 || self.knots.len() != self.mults.len() || self.knots.len() < 2 {
            return None;
        }
        let weights = if self.weights.len() == n {
            self.weights.clone()
        } else {
            vec![1.0; n]
        };
        if !self.periodic {
            let knots: Vec<f64> = self
                .knots
                .iter()
                .zip(&self.mults)
                .flat_map(|(k, m)| std::iter::repeat_n(*k, *m))
                .collect();
            return (knots.len() == n + p + 1).then(|| FlatSpline {
                degree: p,
                poles: self.poles.clone(),
                weights,
                knots,
            });
        }
        // One period of flat knots (the last knot is the first again).
        let last = self.knots.len() - 1;
        let period = self.knots[last] - self.knots[0];
        let flat: Vec<f64> = self.knots[..last]
            .iter()
            .zip(&self.mults[..last])
            .flat_map(|(k, m)| std::iter::repeat_n(*k, *m))
            .collect();
        if flat.len() != n || n == 0 || period <= 0.0 {
            return None;
        }
        let at = |i: isize| -> f64 {
            let n = n as isize;
            let wraps = i.div_euclid(n);
            flat[i.rem_euclid(n) as usize] + wraps as f64 * period
        };
        let knots = (0..n + 2 * p + 1)
            .map(|j| at(j as isize - p as isize))
            .collect();
        let poles = (0..n + p).map(|j| self.poles[j % n]).collect();
        let weights = (0..n + p).map(|j| weights[j % n]).collect();
        Some(FlatSpline {
            degree: p,
            poles,
            weights,
            knots,
        })
    }
}

impl FlatSpline {
    pub fn domain(&self) -> (f64, f64) {
        (
            self.knots[self.degree],
            self.knots[self.knots.len() - self.degree - 1],
        )
    }

    /// The point at `t` (de Boor, in homogeneous coordinates).
    pub fn point(&self, t: f64) -> [f64; 2] {
        let p = self.degree;
        let n = self.poles.len();
        let (lo, hi) = self.domain();
        let t = t.clamp(lo, hi);
        // The span: knots[k] <= t < knots[k + 1], within p..n.
        let mut k = p;
        while k + 1 < n && self.knots[k + 1] <= t {
            k += 1;
        }
        let mut d: Vec<[f64; 3]> = (0..=p)
            .map(|j| {
                let i = j + k - p;
                let w = self.weights[i];
                [self.poles[i][0] * w, self.poles[i][1] * w, w]
            })
            .collect();
        for r in 1..=p {
            for j in (r..=p).rev() {
                let i = j + k - p;
                let span = self.knots[i + p + 1 - r] - self.knots[i];
                let alpha = if span == 0.0 {
                    0.0
                } else {
                    (t - self.knots[i]) / span
                };
                d[j] = std::array::from_fn(|c| (1.0 - alpha) * d[j - 1][c] + alpha * d[j][c]);
            }
        }
        [d[p][0] / d[p][2], d[p][1] / d[p][2]]
    }
}

impl Curve {
    /// The point at parameter `t` of a conic arc (or a circle, ellipse).
    pub fn at(&self, t: f64) -> Option<[f64; 2]> {
        Some(match self {
            Curve::Circle { center, radius } => {
                [center[0] + radius * t.cos(), center[1] + radius * t.sin()]
            }
            Curve::Arc {
                center,
                radius,
                frame,
                ..
            } => frame.place(*center, radius * t.cos(), radius * t.sin()),
            Curve::Ellipse {
                center,
                major,
                minor,
                frame,
            }
            | Curve::ArcOfEllipse {
                center,
                major,
                minor,
                frame,
                ..
            } => frame.place(*center, major * t.cos(), minor * t.sin()),
            Curve::ArcOfHyperbola {
                center,
                major,
                minor,
                frame,
                ..
            } => frame.place(*center, major * t.cosh(), minor * t.sinh()),
            Curve::ArcOfParabola {
                vertex,
                focal,
                frame,
                ..
            } => frame.place(*vertex, t * t / (4.0 * focal), t),
            _ => return None,
        })
    }

    /// An arc's parameter range and frame.
    fn range(&self) -> Option<(f64, f64, &ConicFrame)> {
        match self {
            Curve::Arc {
                frame, start, end, ..
            }
            | Curve::ArcOfEllipse {
                frame, start, end, ..
            }
            | Curve::ArcOfHyperbola {
                frame, start, end, ..
            }
            | Curve::ArcOfParabola {
                frame, start, end, ..
            } => Some((*start, *end, frame)),
            _ => None,
        }
    }

    /// A point of the curve as the sketcher names it: a line's or an arc's
    /// start and end (an arc's as if it ran counter-clockwise), a centre
    /// (an ellipse's, a hyperbola's; a parabola's vertex), a B-spline's
    /// ends; a point is all three.
    pub fn point(&self, pos: PointPos) -> Option<[f64; 2]> {
        use PointPos::*;
        match (self, pos) {
            (_, None) => Option::None,
            (Curve::Point { at }, _) => Some(*at),
            (Curve::Line { start, .. }, Start) => Some(*start),
            (Curve::Line { end, .. }, End) => Some(*end),
            (Curve::Circle { center, .. }, Mid)
            | (Curve::Arc { center, .. }, Mid)
            | (Curve::Ellipse { center, .. }, Mid)
            | (Curve::ArcOfEllipse { center, .. }, Mid)
            | (Curve::ArcOfHyperbola { center, .. }, Mid) => Some(*center),
            (Curve::ArcOfParabola { vertex, .. }, Mid) => Some(*vertex),
            (Curve::BSpline(b), Start | End) => {
                let flat = b.flat()?;
                let (lo, hi) = flat.domain();
                Some(flat.point(if pos == Start || b.periodic { lo } else { hi }))
            }
            (Curve::Bezier { poles, .. }, Start) => poles.first().copied(),
            (Curve::Bezier { poles, .. }, End) => poles.last().copied(),
            (_, Start | End) => {
                let (t0, t1, frame) = self.range()?;
                // The sketcher names the ends counter-clockwise.
                let (s, e) = if frame.is_clockwise() {
                    (t1, t0)
                } else {
                    (t0, t1)
                };
                self.at(if pos == Start { s } else { e })
            }
            _ => Option::None,
        }
    }

    /// The type name as FreeCAD writes it.
    pub fn type_name(&self) -> &str {
        match self {
            Curve::Point { .. } => "Part::GeomPoint",
            Curve::Line { .. } => "Part::GeomLineSegment",
            Curve::Circle { .. } => "Part::GeomCircle",
            Curve::Arc { .. } => "Part::GeomArcOfCircle",
            Curve::Ellipse { .. } => "Part::GeomEllipse",
            Curve::ArcOfEllipse { .. } => "Part::GeomArcOfEllipse",
            Curve::ArcOfHyperbola { .. } => "Part::GeomArcOfHyperbola",
            Curve::ArcOfParabola { .. } => "Part::GeomArcOfParabola",
            Curve::BSpline(_) => "Part::GeomBSplineCurve",
            Curve::Bezier { .. } => "Part::GeomBezierCurve",
            Curve::Other { type_name } => type_name,
        }
    }
}

/// A geometry of a sketch (or of its external geometry).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Geometry {
    pub curve: Curve,
    pub construction: bool,
    /// Blocked (a Block constraint's mark).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub blocked: bool,
    /// Internal geometry of another curve: its kind ([`internal`]).
    #[serde(skip_serializing_if = "is_zero")]
    pub internal: i64,
    /// The sketcher's geometry id (1.0).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<i64>,
    /// External geometry: the element it was projected from (the object's
    /// name and its mapped name), and whether it is defining (1.1: in the
    /// sketch's profiles).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub defining: bool,
}

fn is_zero(v: &i64) -> bool {
    *v == 0
}

/// A constraint as the file keeps it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Constraint {
    #[serde(skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(rename = "type")]
    pub kind: ConstraintType,
    /// Millimetres, or radians for angles.
    pub value: f64,
    pub first: GeoRef,
    pub second: GeoRef,
    pub third: GeoRef,
    /// False for a reference (driven) dimension.
    pub driving: bool,
    /// False when switched off.
    pub active: bool,
    /// Internal alignment: the kind ([`internal`]) and, for B-spline
    /// control and knot points, the index.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alignment: Option<(i64, i64)>,
}

impl Constraint {
    /// The references that are set, in order.
    pub fn refs(&self) -> Vec<GeoRef> {
        [self.first, self.second, self.third]
            .into_iter()
            .filter(|r| !r.is_undef())
            .collect()
    }
}

/// One link of `ExternalGeometry`: an element of another object.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExternalLink {
    pub object: String,
    /// `Edge3`, `Vertex2`, `Face1`.
    pub element: String,
    /// FreeCAD 1.0's mapped name of the element.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mapped: Option<String>,
}

/// A sketch's content.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Sketch {
    pub geometry: Vec<Geometry>,
    pub constraints: Vec<Constraint>,
    pub links: Vec<ExternalLink>,
    /// The external geometry, GeoId −3 first: from `ExternalGeo` (1.0), or
    /// None per link where the file keeps no geometry (0.21).
    pub external: Vec<Option<Geometry>>,
    /// For each external geometry, the link it was made from (an index
    /// into `links`).
    pub external_links: Vec<Option<usize>>,
    /// Expressions bound to constraints: constraint index and expression.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub expressions: Vec<(usize, String)>,
    /// What could not be read.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub problems: Vec<String>,
}

impl Sketch {
    /// The sketch of a `Sketcher::SketchObject`.
    pub fn of(object: &Object) -> Sketch {
        let mut sketch = Sketch::default();
        let mut problems = Vec::new();
        if let Some(Value::Xml(list)) = object.value("Geometry") {
            sketch.geometry = geometry_list(list, &mut problems);
        }
        if let Some(Value::Xml(list)) = object.value("Constraints") {
            sketch.constraints = list
                .elements("Constrain")
                .filter_map(|c| match constraint(c) {
                    Ok(c) => Some(c),
                    Err(e) => {
                        problems.push(format!("a constraint: {e}"));
                        None
                    }
                })
                .collect();
        }
        sketch.links = object
            .links("ExternalGeometry")
            .iter()
            .flat_map(links)
            .collect();
        match object.value("ExternalGeo") {
            Some(Value::Xml(list)) => {
                // The two axes first.
                let all = geometry_list(list, &mut problems);
                for g in all.into_iter().skip(2) {
                    let link = g.reference.as_deref().and_then(|r| {
                        sketch.links.iter().position(|l| {
                            l.mapped
                                .as_deref()
                                .is_some_and(|m| r == format!("{}.{}", l.object, mapped_element(m)))
                        })
                    });
                    sketch.external.push(Some(g));
                    sketch.external_links.push(link);
                }
                // Entries without a reference that matched: in order, when
                // there is one per link.
                if sketch.external.len() == sketch.links.len() {
                    for (i, link) in sketch.external_links.iter_mut().enumerate() {
                        link.get_or_insert(i);
                    }
                }
            }
            _ => {
                sketch.external = vec![None; sketch.links.len()];
                sketch.external_links = (0..sketch.links.len()).map(Some).collect();
            }
        }
        if let Some(Value::Expressions(list)) = object.value("ExpressionEngine") {
            for e in list {
                if let Some(i) = sketch.constraint_of_path(&e.path) {
                    sketch.expressions.push((i, e.expression.clone()));
                }
            }
        }
        sketch.problems = problems;
        sketch
    }

    /// The constraint an expression's path names: `.Constraints.width`,
    /// `Constraints.width` or `Constraints[3]`.
    fn constraint_of_path(&self, path: &str) -> Option<usize> {
        let rest = path.trim_start_matches('.').strip_prefix("Constraints")?;
        if let Some(index) = rest.strip_prefix('[') {
            return index.strip_suffix(']')?.trim().parse().ok();
        }
        let name = rest.strip_prefix('.')?;
        self.constraints.iter().position(|c| c.name == name)
    }

    /// The geometry a GeoId names: the sketch's own or external (None for
    /// the axes and unknown ids).
    pub fn geometry(&self, geo: i32) -> Option<&Geometry> {
        if geo >= 0 {
            return self.geometry.get(geo as usize);
        }
        let k = GeoRef::new(geo, PointPos::None).external()?;
        self.external.get(k)?.as_ref()
    }

    /// The expression bound to a constraint.
    pub fn expression(&self, constraint: usize) -> Option<&str> {
        self.expressions
            .iter()
            .find(|(i, _)| *i == constraint)
            .map(|(_, e)| e.as_str())
    }
}

/// The element of a mapped name `;#16:1;:U;XTR;:H12e7:7,E.Edge4` without
/// its trailing index name.
fn mapped_element(mapped: &str) -> &str {
    match mapped.rfind('.') {
        Some(at)
            if mapped[at + 1..]
                .trim_start_matches(char::is_alphabetic)
                .chars()
                .all(|c| c.is_ascii_digit()) =>
        {
            &mapped[..at]
        }
        _ => mapped,
    }
}

fn links(link: &LinkRef) -> Vec<ExternalLink> {
    link.subs
        .iter()
        .map(|s| ExternalLink {
            object: link.object.clone(),
            element: s.name.clone(),
            mapped: s.mapped.clone(),
        })
        .collect()
}

fn number(e: &Element, key: &str) -> Result<f64, String> {
    let text = e
        .attribute(key)
        .ok_or_else(|| format!("<{}> has no {key}", e.name))?;
    text.trim()
        .parse()
        .map_err(|_| format!("<{}> {key}=\"{text}\" is not a number", e.name))
}

fn integer(e: &Element, key: &str) -> Option<i64> {
    e.attribute(key)?.trim().parse().ok()
}

fn xy(e: &Element, x: &str, y: &str) -> Result<[f64; 2], String> {
    Ok([number(e, x)?, number(e, y)?])
}

fn normal(e: &Element) -> [f64; 3] {
    let n = |k: &str| number(e, k).unwrap_or(0.0);
    [n("NormalX"), n("NormalY"), n("NormalZ")]
}

/// The geometries of a `<GeometryList>`.
fn geometry_list(list: &Element, problems: &mut Vec<String>) -> Vec<Geometry> {
    list.elements("Geometry")
        .enumerate()
        .map(|(i, g)| {
            geometry(g).unwrap_or_else(|e| {
                problems.push(format!("geometry {i}: {e}"));
                Geometry {
                    curve: Curve::Other {
                        type_name: g.attribute("type").unwrap_or_default().to_owned(),
                    },
                    construction: false,
                    blocked: false,
                    internal: 0,
                    id: None,
                    reference: None,
                    defining: false,
                }
            })
        })
        .collect()
}

fn geometry(g: &Element) -> Result<Geometry, String> {
    let type_name = g.attribute("type").unwrap_or_default();
    let mut out = Geometry {
        curve: Curve::Other {
            type_name: type_name.to_owned(),
        },
        construction: false,
        blocked: false,
        internal: 0,
        id: integer(g, "id"),
        reference: g
            .attribute("ref")
            .filter(|r| !r.is_empty())
            .map(str::to_owned),
        defining: false,
    };
    let mut flags_seen = false;
    for extensions in g.elements("GeoExtensions") {
        for ext in extensions.elements("GeoExtension") {
            match ext.attribute("type") {
                Some("Sketcher::SketchGeometryExtension") => {
                    out.internal = integer(ext, "internalGeometryType").unwrap_or(0);
                    if let Some(bits) = ext.attribute("geometryModeFlags") {
                        // A bit string, bit 0 last.
                        let bit = |i: usize| bits.trim().chars().rev().nth(i) == Some('1');
                        out.blocked = bit(0);
                        out.construction = bit(1);
                        flags_seen = true;
                    }
                }
                Some("Sketcher::ExternalGeometryExtension") => {
                    out.defining = integer(ext, "Flags").unwrap_or(0) & 1 != 0;
                    if out.reference.is_none() {
                        out.reference = ext
                            .attribute("Ref")
                            .filter(|r| !r.is_empty())
                            .map(str::to_owned);
                    }
                }
                _ => {}
            }
        }
    }
    if !flags_seen && let Some(c) = g.elements("Construction").next() {
        out.construction = matches!(c.attribute("value"), Some("1" | "true"));
    }
    let Some(c) = g.children.iter().find(|c| {
        !matches!(
            c.name.as_str(),
            "GeoExtensions" | "Construction" | "GeometryExtensions"
        )
    }) else {
        return Ok(out);
    };
    let angle = |key: &str| number(c, key).unwrap_or(0.0);
    let frame = || ConicFrame::new(normal(c), angle("AngleXU"));
    out.curve = match c.name.as_str() {
        "GeomPoint" => Curve::Point {
            at: xy(c, "X", "Y")?,
        },
        "LineSegment" => Curve::Line {
            start: xy(c, "StartX", "StartY")?,
            end: xy(c, "EndX", "EndY")?,
        },
        "Circle" => Curve::Circle {
            center: xy(c, "CenterX", "CenterY")?,
            radius: number(c, "Radius")?,
        },
        "ArcOfCircle" => Curve::Arc {
            center: xy(c, "CenterX", "CenterY")?,
            radius: number(c, "Radius")?,
            frame: frame(),
            start: number(c, "StartAngle")?,
            end: number(c, "EndAngle")?,
        },
        "Ellipse" => Curve::Ellipse {
            center: xy(c, "CenterX", "CenterY")?,
            major: number(c, "MajorRadius")?,
            minor: number(c, "MinorRadius")?,
            frame: frame(),
        },
        "ArcOfEllipse" => Curve::ArcOfEllipse {
            center: xy(c, "CenterX", "CenterY")?,
            major: number(c, "MajorRadius")?,
            minor: number(c, "MinorRadius")?,
            frame: frame(),
            start: number(c, "StartAngle")?,
            end: number(c, "EndAngle")?,
        },
        "ArcOfHyperbola" => Curve::ArcOfHyperbola {
            center: xy(c, "CenterX", "CenterY")?,
            major: number(c, "MajorRadius")?,
            minor: number(c, "MinorRadius")?,
            frame: frame(),
            start: number(c, "StartAngle")?,
            end: number(c, "EndAngle")?,
        },
        "ArcOfParabola" => Curve::ArcOfParabola {
            vertex: xy(c, "CenterX", "CenterY")?,
            focal: number(c, "Focal")?,
            frame: frame(),
            start: number(c, "StartAngle")?,
            end: number(c, "EndAngle")?,
        },
        "BSplineCurve" => {
            let mut b = BSpline {
                degree: integer(c, "Degree").unwrap_or(0).max(0) as usize,
                poles: Vec::new(),
                weights: Vec::new(),
                knots: Vec::new(),
                mults: Vec::new(),
                periodic: matches!(c.attribute("IsPeriodic"), Some("1" | "true")),
            };
            for pole in c.elements("Pole") {
                b.poles.push(xy(pole, "X", "Y")?);
                b.weights.push(number(pole, "Weight").unwrap_or(1.0));
            }
            for knot in c.elements("Knot") {
                b.knots.push(number(knot, "Value")?);
                b.mults
                    .push(integer(knot, "Mult").unwrap_or(1).max(0) as usize);
            }
            if b.flat().is_none() {
                return Err(format!(
                    "a B-spline of degree {} with {} poles and knots {:?} of multiplicities {:?}",
                    b.degree,
                    b.poles.len(),
                    b.knots,
                    b.mults
                ));
            }
            Curve::BSpline(b)
        }
        "BezierCurve" => {
            let mut poles = Vec::new();
            let mut weights = Vec::new();
            for pole in c.elements("Pole") {
                poles.push(xy(pole, "X", "Y")?);
                weights.push(number(pole, "Weight").unwrap_or(1.0));
            }
            Curve::Bezier { poles, weights }
        }
        _ => Curve::Other {
            type_name: type_name.to_owned(),
        },
    };
    Ok(out)
}

fn constraint(c: &Element) -> Result<Constraint, String> {
    let kind = ConstraintType::from_index(integer(c, "Type").ok_or_else(|| "no Type".to_owned())?);
    // 1.1: the references also (or only) as lists.
    let list = |key: &str| -> Vec<i64> {
        c.attribute(key)
            .map(|t| {
                t.split_whitespace()
                    .filter_map(|v| v.parse().ok())
                    .collect()
            })
            .unwrap_or_default()
    };
    let (ids, positions) = (list("ElementIds"), list("ElementPositions"));
    let reference = |i: usize, geo: &str, pos: &str| -> GeoRef {
        let geo = integer(c, geo).or_else(|| ids.get(i).copied());
        let pos = integer(c, pos).or_else(|| positions.get(i).copied());
        match geo {
            Some(g) => GeoRef::new(
                i32::try_from(g).unwrap_or(GEO_UNDEF),
                PointPos::from_index(pos.unwrap_or(0)),
            ),
            None => GeoRef::UNDEF,
        }
    };
    let flag = |key: &str, default: bool| match c.attribute(key) {
        Some(v) => matches!(v.trim(), "1" | "true"),
        None => default,
    };
    let alignment = (kind == ConstraintType::InternalAlignment).then(|| {
        (
            integer(c, "InternalAlignmentType").unwrap_or(0),
            integer(c, "InternalAlignmentIndex").unwrap_or(-1),
        )
    });
    Ok(Constraint {
        name: c.attribute("Name").unwrap_or_default().to_owned(),
        kind,
        value: number(c, "Value").unwrap_or(0.0),
        first: reference(0, "First", "FirstPos"),
        second: reference(1, "Second", "SecondPos"),
        third: reference(2, "Third", "ThirdPos"),
        driving: flag("IsDriving", true),
        active: flag("IsActive", true),
        alignment,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: [f64; 2], b: [f64; 2]) -> bool {
        (a[0] - b[0]).abs() < 1e-12 && (a[1] - b[1]).abs() < 1e-12
    }

    #[test]
    fn conic_frames_follow_the_normal() {
        // +Z: the X axis turned by the angle.
        let f = ConicFrame::new([0.0, 0.0, 1.0], std::f64::consts::FRAC_PI_2);
        assert!(close(f.x, [0.0, 1.0]) && close(f.y, [-1.0, 0.0]));
        assert!(!f.is_clockwise());
        // −Z: OCCT's X is −X, and y is +Y; turning goes clockwise.
        let f = ConicFrame::new([0.0, 0.0, -1.0], 0.0);
        assert!(close(f.x, [-1.0, 0.0]) && close(f.y, [0.0, 1.0]));
        assert!(f.is_clockwise());
        // An arc about −Z from 0 to 1 rad: its sketcher start is the end of
        // its range.
        let arc = Curve::Arc {
            center: [120.0, 0.0],
            radius: 10.0,
            frame: f,
            start: 0.0,
            end: 1.0,
        };
        assert!(close(
            arc.point(PointPos::Start).unwrap(),
            [120.0 - 10.0 * 1f64.cos(), 10.0 * 1f64.sin()]
        ));
        assert!(close(arc.point(PointPos::End).unwrap(), [110.0, 0.0]));
    }

    #[test]
    fn periodic_splines_unwrap() {
        // Six poles on a circle, cubic, uniform: FreeCAD starts it near the
        // second pole (the first three poles' average 1:4:1).
        let poles: Vec<[f64; 2]> = (0..6)
            .map(|i| {
                let a = i as f64 * std::f64::consts::PI / 3.0;
                [10.0 * a.cos(), 10.0 * a.sin()]
            })
            .collect();
        let b = BSpline {
            degree: 3,
            poles: poles.clone(),
            weights: vec![1.0; 6],
            knots: (0..7).map(|i| i as f64 / 6.0).collect(),
            mults: vec![1; 7],
            periodic: true,
        };
        let flat = b.flat().unwrap();
        assert_eq!(flat.poles.len(), 9);
        assert_eq!(flat.knots.len(), 13);
        let start = b.flat().unwrap().point(0.0);
        let expected: [f64; 2] =
            std::array::from_fn(|k| (poles[0][k] + 4.0 * poles[1][k] + poles[2][k]) / 6.0);
        assert!(close(start, expected), "{start:?}");
        let (lo, hi) = flat.domain();
        assert!(close(flat.point(lo), flat.point(hi)));
        // A clamped one starts at its first pole.
        let clamped = BSpline {
            degree: 3,
            poles: poles[..4].to_vec(),
            weights: vec![1.0; 4],
            knots: vec![0.0, 1.0],
            mults: vec![4, 4],
            periodic: false,
        };
        assert!(clamped.is_clamped());
        assert!(close(
            Curve::BSpline(clamped.clone())
                .point(PointPos::End)
                .unwrap(),
            poles[3]
        ));
    }

    #[test]
    fn mapped_names_lose_their_index_names() {
        assert_eq!(
            mapped_element(";#16:1;:U;XTR;:H12e7:7,E.Edge4"),
            ";#16:1;:U;XTR;:H12e7:7,E"
        );
        assert_eq!(mapped_element("Edge4"), "Edge4");
    }
}
