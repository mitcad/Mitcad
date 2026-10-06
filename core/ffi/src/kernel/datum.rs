// SPDX-License-Identifier: MIT
//! Construction geometry: surfaces, curves and points of named sub-shapes,
//! paths, and display shapes of datums
//! (`geometry/include/mitcad/geometry/datum.hpp`).

#[cxx::bridge(namespace = "mitcad::bridge")]
pub mod ffi {
    struct DatumVec {
        xyz: [f64; 3],
    }

    /// A face's surface; the fields of its kind are set (see
    /// `mitcad::geometry::SurfaceDescription`).
    struct DatumSurface {
        kind: String,
        origin: [f64; 3],
        axis: [f64; 3],
        radius: f64,
        minor_radius: f64,
        half_angle: f64,
    }

    struct DatumCurve {
        kind: String,
        start: [f64; 3],
        end: [f64; 3],
        center: [f64; 3],
        normal: [f64; 3],
        radius: f64,
    }

    /// A point with a direction: a path's tangent or a face's normal.
    struct DatumPointing {
        point: [f64; 3],
        direction: [f64; 3],
    }

    unsafe extern "C++" {
        include!("bridge/datum.hpp");

        #[namespace = "mitcad::geometry"]
        type Shape = crate::kernel::shape::ffi::Shape;

        fn datum_face_geometry(shape: &Shape, face: &str) -> Result<DatumSurface>;
        fn datum_edge_geometry(shape: &Shape, edge: &str) -> Result<DatumCurve>;
        fn datum_vertex_point(shape: &Shape, vertex: &str) -> Result<DatumVec>;
        /// `mode` 0: `value` is a fraction of the length, 1: a length, 2:
        /// the point nearest to `near`.
        fn datum_path_point(
            shape: &Shape,
            edges: &[String],
            mode: u8,
            value: f64,
            near: &DatumVec,
        ) -> Result<DatumPointing>;
        fn datum_face_point(shape: &Shape, face: &str, near: &DatumVec) -> Result<DatumPointing>;
        fn datum_plane_shape(
            origin: &DatumVec,
            normal: &DatumVec,
            x_axis: &DatumVec,
            size: f64,
        ) -> Result<SharedPtr<Shape>>;
        fn datum_axis_shape(
            origin: &DatumVec,
            direction: &DatumVec,
            size: f64,
        ) -> Result<SharedPtr<Shape>>;
        fn datum_point_shape(point: &DatumVec) -> Result<SharedPtr<Shape>>;
    }
}

use mitcad_model::datum::{CurveGeometry, PathParameter, SurfaceGeometry};

pub fn vec(xyz: [f64; 3]) -> ffi::DatumVec {
    ffi::DatumVec { xyz }
}

pub fn surface(s: ffi::DatumSurface) -> SurfaceGeometry {
    match s.kind.as_str() {
        "plane" => SurfaceGeometry::Plane {
            origin: s.origin,
            normal: s.axis,
        },
        "cylinder" => SurfaceGeometry::Cylinder {
            origin: s.origin,
            axis: s.axis,
            radius: s.radius,
        },
        "cone" => SurfaceGeometry::Cone {
            origin: s.origin,
            axis: s.axis,
            radius: s.radius,
            half_angle: s.half_angle,
        },
        "sphere" => SurfaceGeometry::Sphere {
            center: s.origin,
            radius: s.radius,
        },
        "torus" => SurfaceGeometry::Torus {
            center: s.origin,
            axis: s.axis,
            major_radius: s.radius,
            minor_radius: s.minor_radius,
        },
        _ => SurfaceGeometry::Other { kind: s.kind },
    }
}

pub fn curve(c: ffi::DatumCurve) -> CurveGeometry {
    match c.kind.as_str() {
        "line" => CurveGeometry::Line {
            start: c.start,
            end: c.end,
        },
        "circle" => CurveGeometry::Circle {
            center: c.center,
            normal: c.normal,
            radius: c.radius,
            start: c.start,
            end: c.end,
        },
        _ => CurveGeometry::Other {
            kind: c.kind,
            start: c.start,
            end: c.end,
        },
    }
}

/// The mode, value and point of [`ffi::datum_path_point`].
pub fn path_parameter(at: PathParameter) -> (u8, f64, [f64; 3]) {
    match at {
        PathParameter::Fraction(f) => (0, f, [0.0; 3]),
        PathParameter::Length(l) => (1, l, [0.0; 3]),
        PathParameter::Near(p) => (2, 0.0, p),
    }
}
