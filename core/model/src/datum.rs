// SPDX-License-Identifier: MIT
//! Datums: the planes, axes and points that construction features produce
//! (construction geometry) and the origin datums every document
//! has, with the analytic descriptions of faces and edges the kernel
//! reports for them. Millimetres and radians, model coordinates.
//!
//! Datums are not bodies. A construction feature's result is one datum,
//! kept in the recompute state by the feature's uid, so later features
//! refer to it as `"F7"` (see `features::geom_ref::GeomRef`).

use std::fmt;

use crate::profile::{SketchFrame, cross, dot};

pub type Vec3 = [f64; 3];

/// Directions closer than this (as the sine of their angle) are parallel.
pub(crate) const ANGULAR: f64 = 1e-9;
/// Points closer than this are the same point, mm.
pub(crate) const LINEAR: f64 = 1e-6;

pub(crate) fn add(a: Vec3, b: Vec3) -> Vec3 {
    std::array::from_fn(|i| a[i] + b[i])
}

pub(crate) fn sub(a: Vec3, b: Vec3) -> Vec3 {
    std::array::from_fn(|i| a[i] - b[i])
}

pub(crate) fn scale(a: Vec3, s: f64) -> Vec3 {
    a.map(|v| v * s)
}

pub(crate) fn norm(a: Vec3) -> f64 {
    dot(a, a).sqrt()
}

/// The unit vector along `a`, None for a (nearly) zero vector.
pub(crate) fn unit(a: Vec3) -> Option<Vec3> {
    let length = norm(a);
    (length > 1e-12 && length.is_finite()).then(|| scale(a, 1.0 / length))
}

/// `v` rotated by `angle` about the unit vector `axis`, right-handed.
pub(crate) fn rotate(v: Vec3, axis: Vec3, angle: f64) -> Vec3 {
    let (sin, cos) = angle.sin_cos();
    let along = scale(axis, dot(axis, v) * (1.0 - cos));
    add(add(scale(v, cos), scale(cross(axis, v), sin)), along)
}

/// The smallest rotation taking the unit vector `from` to `to`, applied to
/// `v`. Opposite vectors turn about any perpendicular axis.
pub(crate) fn rotate_onto(v: Vec3, from: Vec3, to: Vec3) -> Vec3 {
    let axis = cross(from, to);
    let sin = norm(axis);
    let cos = dot(from, to);
    if sin < ANGULAR {
        if cos > 0.0 {
            return v;
        }
        return rotate(v, perpendicular(from), std::f64::consts::PI);
    }
    rotate(v, scale(axis, 1.0 / sin), sin.atan2(cos))
}

/// A unit vector at right angles to the unit vector `n`, chosen as OCCT's
/// `gp_Ax2` does, so frames without other guidance are reproducible.
pub(crate) fn perpendicular(n: Vec3) -> Vec3 {
    let [a, b, c] = n;
    let (aa, ab, ac) = (a.abs(), b.abs(), c.abs());
    let v = if ab <= aa && ab <= ac {
        if aa > ac { [-c, 0.0, a] } else { [c, 0.0, -a] }
    } else if aa <= ab && aa <= ac {
        if ab > ac { [0.0, -c, b] } else { [0.0, c, -b] }
    } else if aa > ab {
        [-b, a, 0.0]
    } else {
        [b, -a, 0.0]
    };
    unit(v).unwrap_or([1.0, 0.0, 0.0])
}

/// The direction of an axis that has no sense of its own (the axis of a
/// cylinder, cone, torus or circle): the unit vector `v` or its opposite,
/// whichever points along its largest component, so that it does not
/// depend on how the kernel built the surface or curve. The kernel's face
/// descriptions follow the same rule (`geometry/src/datum.cpp`).
pub(crate) fn canonical_axis(v: Vec3) -> Vec3 {
    let mut largest = v[0];
    for c in [v[1], v[2]] {
        if c.abs() > largest.abs() + 1e-12 {
            largest = c;
        }
    }
    if largest < 0.0 { scale(v, -1.0) } else { v }
}

/// The in-plane x axis for a plane with unit normal `n` and no other
/// guidance: the model X axis projected onto the plane, or the Y axis when
/// X is (nearly) along the normal.
pub(crate) fn default_x_axis(n: Vec3) -> Vec3 {
    let project = |axis: Vec3| unit(sub(axis, scale(n, dot(n, axis))));
    if dot(n, [1.0, 0.0, 0.0]).abs() < 1.0 - 1e-3 {
        project([1.0, 0.0, 0.0])
    } else {
        project([0.0, 1.0, 0.0])
    }
    .unwrap_or_else(|| perpendicular(n))
}

/// A plane with an in-plane frame. The frame places sketches on the plane
/// (see [`DatumPlane::frame`]); the normal is `x_axis × y_axis`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DatumPlane {
    pub origin: Vec3,
    /// Unit vectors at right angles.
    pub x_axis: Vec3,
    pub y_axis: Vec3,
}

impl DatumPlane {
    /// The plane through `origin` with the unit normal `normal`; the x axis
    /// is `x_hint` projected onto the plane, or the default (see
    /// [`default_x_axis`]) when there is no hint or it is along the normal.
    pub fn from_normal(origin: Vec3, normal: Vec3, x_hint: Option<Vec3>) -> Self {
        let x_axis = x_hint
            .and_then(|x| unit(sub(x, scale(normal, dot(normal, x)))))
            .unwrap_or_else(|| default_x_axis(normal));
        Self {
            origin,
            x_axis,
            y_axis: cross(normal, x_axis),
        }
    }

    /// The frame of a plane known by a point and its unit normal, such as a
    /// planar face: the model origin projected onto the plane is the origin
    /// and the x axis is the default (see [`default_x_axis`]). This is the
    /// one rule for faces: sketches on faces, mirror planes, primitives and
    /// extents to faces all use it.
    pub fn on_plane(point: Vec3, normal: Vec3) -> Self {
        Self::from_normal(scale(normal, dot(normal, point)), normal, None)
    }

    pub fn normal(&self) -> Vec3 {
        cross(self.x_axis, self.y_axis)
    }

    /// The sketch frame of the plane: sketch x and y along the plane's axes,
    /// the sketch normal along the plane's normal.
    pub fn frame(&self) -> SketchFrame {
        SketchFrame {
            origin: self.origin,
            x_axis: self.x_axis,
            y_axis: self.y_axis,
        }
    }

    /// Signed distance of a point along the normal.
    pub fn distance_to(&self, point: Vec3) -> f64 {
        dot(sub(point, self.origin), self.normal())
    }

    /// The point of the plane nearest to `point`.
    pub fn project(&self, point: Vec3) -> Vec3 {
        sub(point, scale(self.normal(), self.distance_to(point)))
    }

    /// The plane moved by `offset`.
    pub fn translated(&self, offset: Vec3) -> Self {
        Self {
            origin: add(self.origin, offset),
            ..*self
        }
    }

    /// The line where the planes meet, along `self.normal() × other.normal()`
    /// through the point nearest to this plane's origin; None for parallel
    /// planes.
    pub fn intersection(&self, other: &DatumPlane) -> Option<DatumAxis> {
        let (n1, n2) = (self.normal(), other.normal());
        let u = cross(n1, n2);
        let u2 = dot(u, u);
        if u2.sqrt() < ANGULAR {
            return None;
        }
        let (h1, h2) = (dot(n1, self.origin), dot(n2, other.origin));
        let on_both = scale(
            add(scale(cross(n2, u), h1), scale(cross(u, n1), h2)),
            1.0 / u2,
        );
        let line = DatumAxis {
            origin: on_both,
            direction: scale(u, 1.0 / u2.sqrt()),
        };
        Some(DatumAxis {
            origin: line.project(self.origin),
            ..line
        })
    }
}

/// An infinite line through `origin` along the unit vector `direction`.
/// The direction matters: it sets the sense of rotations about the axis.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DatumAxis {
    pub origin: Vec3,
    pub direction: Vec3,
}

impl DatumAxis {
    /// The axis from `a` towards `b`; None when they coincide.
    pub fn through(a: Vec3, b: Vec3) -> Option<Self> {
        (norm(sub(b, a)) > LINEAR).then(|| Self {
            origin: a,
            direction: unit(sub(b, a)).expect("a and b differ"),
        })
    }

    /// The point of the line nearest to `point`.
    pub fn project(&self, point: Vec3) -> Vec3 {
        add(
            self.origin,
            scale(self.direction, dot(sub(point, self.origin), self.direction)),
        )
    }

    pub fn distance_to(&self, point: Vec3) -> f64 {
        norm(sub(point, self.project(point)))
    }
}

/// A point.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DatumPoint {
    pub point: Vec3,
}

/// What a construction feature produces.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Datum {
    Plane(DatumPlane),
    Axis(DatumAxis),
    Point(DatumPoint),
}

impl Datum {
    pub fn kind(&self) -> DatumKind {
        match self {
            Self::Plane(_) => DatumKind::Plane,
            Self::Axis(_) => DatumKind::Axis,
            Self::Point(_) => DatumKind::Point,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatumKind {
    Plane,
    Axis,
    Point,
}

impl DatumKind {
    /// `plane`, `axis` or `point`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Plane => "plane",
            Self::Axis => "axis",
            Self::Point => "point",
        }
    }
}

impl fmt::Display for DatumKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The origin datums of a document (the browser's Origin folder): the planes
/// XY, XZ and YZ, the axes X, Y and Z and the origin point, written `xy`,
/// `xz`, `yz`, `x`, `y`, `z` and `origin`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum OriginDatum {
    Xy,
    Xz,
    Yz,
    X,
    Y,
    Z,
    Origin,
}

impl OriginDatum {
    pub const ALL: [Self; 7] = [
        Self::Xy,
        Self::Xz,
        Self::Yz,
        Self::X,
        Self::Y,
        Self::Z,
        Self::Origin,
    ];

    /// The id in files and commands.
    pub fn id(self) -> &'static str {
        match self {
            Self::Xy => "xy",
            Self::Xz => "xz",
            Self::Yz => "yz",
            Self::X => "x",
            Self::Y => "y",
            Self::Z => "z",
            Self::Origin => "origin",
        }
    }

    /// The name in the browser.
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Xy => "XY",
            Self::Xz => "XZ",
            Self::Yz => "YZ",
            Self::X => "X",
            Self::Y => "Y",
            Self::Z => "Z",
            Self::Origin => "Origin",
        }
    }

    pub fn parse(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|o| o.id() == id)
    }

    pub fn kind(self) -> DatumKind {
        self.datum().kind()
    }

    /// The datum. The plane frames are those of sketches on the origin
    /// planes in .f3d designs: XY has x = +X, y = +Y (normal +Z); XZ has
    /// x = +X, y = -Z (normal +Y); YZ has x = -Z, y = +Y (normal +X). XY is
    /// confirmed by T0 models and XZ by imported designs; YZ follows the
    /// same rule (the view from the positive normal with the original Y-up
    /// orientation) and awaits experiment K1.
    pub fn datum(self) -> Datum {
        let plane = |x_axis, y_axis| {
            Datum::Plane(DatumPlane {
                origin: [0.0; 3],
                x_axis,
                y_axis,
            })
        };
        let axis = |direction| {
            Datum::Axis(DatumAxis {
                origin: [0.0; 3],
                direction,
            })
        };
        match self {
            Self::Xy => plane([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
            Self::Xz => plane([1.0, 0.0, 0.0], [0.0, 0.0, -1.0]),
            Self::Yz => plane([0.0, 0.0, -1.0], [0.0, 1.0, 0.0]),
            Self::X => axis([1.0, 0.0, 0.0]),
            Self::Y => axis([0.0, 1.0, 0.0]),
            Self::Z => axis([0.0, 0.0, 1.0]),
            Self::Origin => Datum::Point(DatumPoint { point: [0.0; 3] }),
        }
    }
}

/// The surface of a face as the kernel describes it. Normals are outward
/// (the face orientation applied). The axes of cylinders, cones and tori
/// point along their largest component (see [`canonical_axis`]), so they do
/// not depend on how the kernel built the surface; for a hole along Z the
/// axis is +Z.
#[derive(Debug, Clone, PartialEq)]
pub enum SurfaceGeometry {
    /// `origin` is the point of the plane nearest to the model origin. The
    /// plane's frame is the model's ([`DatumPlane::on_plane`]).
    Plane {
        origin: Vec3,
        normal: Vec3,
    },
    /// `origin` is the axis point nearest to the face's centre.
    Cylinder {
        origin: Vec3,
        axis: Vec3,
        radius: f64,
    },
    /// The radius at `origin`; it grows along `axis` by tan(half_angle).
    Cone {
        origin: Vec3,
        axis: Vec3,
        radius: f64,
        half_angle: f64,
    },
    Sphere {
        center: Vec3,
        radius: f64,
    },
    Torus {
        center: Vec3,
        axis: Vec3,
        major_radius: f64,
        minor_radius: f64,
    },
    /// Another surface type (`bspline`, `revolution`, ...).
    Other {
        kind: String,
    },
}

impl SurfaceGeometry {
    /// `plane`, `cylinder`, ... as the face query names surface types.
    pub fn kind(&self) -> &str {
        match self {
            Self::Plane { .. } => "plane",
            Self::Cylinder { .. } => "cylinder",
            Self::Cone { .. } => "cone",
            Self::Sphere { .. } => "sphere",
            Self::Torus { .. } => "torus",
            Self::Other { kind } => kind,
        }
    }
}

/// The curve of an edge as the kernel describes it, in the direction of
/// the edge's curve (its parameter).
#[derive(Debug, Clone, PartialEq)]
pub enum CurveGeometry {
    Line {
        start: Vec3,
        end: Vec3,
    },
    /// A circle or an arc, counter-clockwise about `normal` from `start` to
    /// `end` (the same point for a full circle).
    Circle {
        center: Vec3,
        normal: Vec3,
        radius: f64,
        start: Vec3,
        end: Vec3,
    },
    /// Another curve type (`ellipse`, `bspline`, ...).
    Other {
        kind: String,
        start: Vec3,
        end: Vec3,
    },
}

impl CurveGeometry {
    /// `line`, `circle`, ... as the edge query names curve types.
    pub fn kind(&self) -> &str {
        match self {
            Self::Line { .. } => "line",
            Self::Circle { .. } => "circle",
            Self::Other { kind, .. } => kind,
        }
    }
}

/// Where on a path (a chain of edges) to evaluate it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PathParameter {
    /// A fraction of the length from the start, 0 to 1; values outside
    /// continue straight along the tangent at the end.
    Fraction(f64),
    /// A length from the start, mm; beyond the ends as for fractions.
    Length(f64),
    /// The point of the path nearest to a point.
    Near(Vec3),
}

/// A point of a path and the unit tangent in the path's direction.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PathPoint {
    pub point: Vec3,
    pub tangent: Vec3,
}

/// A point of a face and the unit outward normal there.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SurfacePoint {
    pub point: Vec3,
    pub normal: Vec3,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: Vec3, b: Vec3) -> bool {
        norm(sub(a, b)) < 1e-12
    }

    #[test]
    fn origin_planes_have_right_handed_frames() {
        let normals = [
            (OriginDatum::Xy, [0.0, 0.0, 1.0]),
            (OriginDatum::Xz, [0.0, 1.0, 0.0]),
            (OriginDatum::Yz, [1.0, 0.0, 0.0]),
        ];
        for (origin, normal) in normals {
            let Datum::Plane(plane) = origin.datum() else {
                panic!("{origin:?} is a plane");
            };
            assert!(near(plane.normal(), normal), "{origin:?}");
            assert_eq!(dot(plane.x_axis, plane.y_axis), 0.0);
            assert_eq!(OriginDatum::parse(origin.id()), Some(origin));
        }
        assert_eq!(OriginDatum::X.kind(), DatumKind::Axis);
        assert_eq!(OriginDatum::Origin.kind(), DatumKind::Point);
        assert_eq!(OriginDatum::parse("XY"), None);
    }

    #[test]
    fn rotations_are_right_handed() {
        let z = [0.0, 0.0, 1.0];
        let quarter = std::f64::consts::FRAC_PI_2;
        assert!(near(rotate([1.0, 0.0, 0.0], z, quarter), [0.0, 1.0, 0.0]));
        assert!(near(
            rotate_onto([0.0, 1.0, 0.0], z, [1.0, 0.0, 0.0]),
            [0.0, 1.0, 0.0]
        ));
        assert!(near(rotate_onto(z, z, [0.0, -1.0, 0.0]), [0.0, -1.0, 0.0]));
        assert!(near(rotate_onto(z, z, [0.0, 0.0, -1.0]), [0.0, 0.0, -1.0]));
    }

    #[test]
    fn default_frames_follow_occt_and_the_model_axes() {
        assert_eq!(perpendicular([0.0, 0.0, 1.0]), [1.0, 0.0, 0.0]);
        assert_eq!(default_x_axis([0.0, 0.0, -1.0]), [1.0, 0.0, 0.0]);
        assert_eq!(default_x_axis([1.0, 0.0, 0.0]), [0.0, 1.0, 0.0]);
        let plane = DatumPlane::from_normal([0.0, 0.0, 5.0], [0.0, -1.0, 0.0], None);
        assert!(near(plane.x_axis, [1.0, 0.0, 0.0]));
        assert!(near(plane.y_axis, [0.0, 0.0, 1.0]));
        assert!((plane.distance_to([3.0, -2.0, 1.0]) - 2.0).abs() < 1e-12);
        // A face's frame: the origin projected, x along +X (+Y facing ±X).
        let face = DatumPlane::on_plane([7.0, 3.0, 10.0], [0.0, 0.0, 1.0]);
        assert_eq!(
            face.frame(),
            SketchFrame {
                origin: [0.0, 0.0, 10.0],
                ..SketchFrame::XY
            }
        );
        let side = DatumPlane::on_plane([60.0, 3.0, 1.0], [-1.0, 0.0, 0.0]);
        assert!(near(side.origin, [60.0, 0.0, 0.0]));
        assert!(near(side.x_axis, [0.0, 1.0, 0.0]));
        assert!(near(side.y_axis, [0.0, 0.0, -1.0]));
        assert_eq!(canonical_axis([0.0, 0.0, -1.0]), [0.0, 0.0, 1.0]);
        assert_eq!(canonical_axis([-0.6, 0.8, 0.0]), [-0.6, 0.8, 0.0]);
        let axis = DatumAxis::through([0.0; 3], [0.0, 0.0, 2.0]).unwrap();
        assert!(near(axis.project([1.0, 1.0, 1.0]), [0.0, 0.0, 1.0]));
        assert!(DatumAxis::through([1.0; 3], [1.0; 3]).is_none());
    }
}
