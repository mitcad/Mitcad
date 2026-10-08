// SPDX-License-Identifier: MIT
//! Analysis: physical properties, measurement, interference, sections and
//! comparison with a STEP file (`geometry/analysis`, `geometry/io`).

#[cxx::bridge(namespace = "mitcad::bridge")]
pub mod ffi {
    struct AnalysisProperties {
        volume: f64,
        area: f64,
        mass: f64,
        center: [f64; 3],
        /// Row by row.
        inertia: [f64; 9],
        principal_moments: [f64; 3],
        /// One axis after another.
        principal_axes: [f64; 9],
    }

    /// The fields whose `has_*` flag is set apply.
    struct AnalysisMeasure {
        kind: String,
        has_volume: bool,
        volume: f64,
        has_area: bool,
        area: f64,
        has_length: bool,
        length: f64,
        has_point: bool,
        point: [f64; 3],
        has_circle: bool,
        center: [f64; 3],
        axis: [f64; 3],
        radius: f64,
        sweep: f64,
    }

    struct AnalysisSeparation {
        distance: f64,
        on_a: [f64; 3],
        on_b: [f64; 3],
        inside: bool,
        has_angle: bool,
        angle: f64,
    }

    struct AnalysisPlane {
        origin: [f64; 3],
        normal: [f64; 3],
    }

    struct AnalysisDeviation {
        max: f64,
        rms: f64,
        at: [f64; 3],
        samples: usize,
    }

    /// An edge's middle (T1d): half its length along it, with the unit
    /// tangent there.
    struct AnalysisEdgeMiddle {
        name: String,
        point: [f64; 3],
        tangent: [f64; 3],
        length: f64,
    }

    /// Points inside a named face (P5): x, y, z triples.
    struct AnalysisFacePoints {
        name: String,
        points: Vec<f64>,
    }

    struct AnalysisComparison {
        step_bodies: Vec<String>,
        volume_a: f64,
        volume_b: f64,
        /// The boolean differences succeeded.
        has_differences: bool,
        a_minus_b: f64,
        b_minus_a: f64,
        relative_difference: f64,
        a_to_b: AnalysisDeviation,
        b_to_a: AnalysisDeviation,
        max_deviation: f64,
        bounds_difference: f64,
    }

    unsafe extern "C++" {
        include!("bridge/analysis.hpp");

        #[namespace = "mitcad::geometry"]
        type Shape = crate::kernel::shape::ffi::Shape;
        type ShapeList = crate::kernel::shape::ffi::ShapeList;

        /// Overlapping pairs of bodies.
        type AnalysisInterferences;
        /// A body cut by a plane.
        type AnalysisSection;

        /// Density in g/cm³.
        fn analysis_properties(shape: &Shape, density: f64) -> Result<AnalysisProperties>;
        /// A body (empty name) or its faces, edges or vertices by name.
        fn analysis_measure(shape: &Shape, name: &str) -> Result<AnalysisMeasure>;
        fn analysis_between(
            a: &Shape,
            a_name: &str,
            b: &Shape,
            b_name: &str,
        ) -> Result<AnalysisSeparation>;

        fn analysis_interferences(
            bodies: &ShapeList,
            min_volume: f64,
        ) -> Result<UniquePtr<AnalysisInterferences>>;
        fn count(self: &AnalysisInterferences) -> usize;
        fn first(self: &AnalysisInterferences, index: usize) -> usize;
        fn second(self: &AnalysisInterferences, index: usize) -> usize;
        fn volume(self: &AnalysisInterferences, index: usize) -> f64;
        fn shape(self: &AnalysisInterferences, index: usize) -> SharedPtr<Shape>;

        fn analysis_section(
            shape: &Shape,
            plane: &AnalysisPlane,
        ) -> Result<UniquePtr<AnalysisSection>>;
        fn edge_count(self: &AnalysisSection) -> usize;
        fn face_count(self: &AnalysisSection) -> usize;
        fn length(self: &AnalysisSection) -> f64;
        fn area(self: &AnalysisSection) -> f64;
        fn curves(self: &AnalysisSection) -> SharedPtr<Shape>;
        fn faces(self: &AnalysisSection) -> SharedPtr<Shape>;

        fn analysis_clip(shape: &Shape, plane: &AnalysisPlane) -> Result<SharedPtr<Shape>>;

        /// The bodies together against a STEP file's bodies: all of them
        /// with an empty `step_body`, else the one with that name, or that
        /// 0-based index.
        fn analysis_compare_step(
            bodies: &ShapeList,
            path: &str,
            step_body: &str,
            samples: usize,
            fuzzy: f64,
        ) -> Result<AnalysisComparison>;

        // .f3d import (T1).
        /// The bodies `a` together against the bodies `b` together, for
        /// at most `seconds` (0: no limit), without the boolean differences
        /// when no sample lies farther than `booleans_above` (0: always
        /// with them).
        fn analysis_compare_shapes(
            a: &ShapeList,
            b: &ShapeList,
            samples: usize,
            fuzzy: f64,
            seconds: f64,
            booleans_above: f64,
        ) -> Result<AnalysisComparison>;
        /// Distances of points (x, y, z triples) from the shape's faces.
        fn analysis_boundary_distances(shape: &Shape, points: &[f64]) -> Result<Vec<f64>>;
        /// Whether points (x, y, z triples) lie inside the shape's solids.
        fn analysis_points_inside(shape: &Shape, points: &[f64]) -> Result<Vec<bool>>;
        /// Where a segment (two x, y, z triples) crosses the shape's
        /// faces: pairs of the fraction along it and -1 entering the
        /// material, +1 leaving it, in order along it.
        fn analysis_segment_crossings(shape: &Shape, segment: &[f64]) -> Result<Vec<f64>>;
        fn analysis_face_count(shape: &Shape) -> Result<usize>;
        /// Every named edge's middle, in the shape's edge order.
        fn analysis_edge_middles(shape: &Shape) -> Result<Vec<AnalysisEdgeMiddle>>;
        /// Points inside every named face, in the shape's face order (P5).
        fn analysis_face_points(shape: &Shape) -> Result<Vec<AnalysisFacePoints>>;
    }
}

use mitcad_model::analysis::{
    CircleMeasure, Comparison, Deviation, Measurement, PhysicalProperties, Separation,
};

fn rows(values: [f64; 9]) -> [[f64; 3]; 3] {
    std::array::from_fn(|i| std::array::from_fn(|j| values[3 * i + j]))
}

pub fn properties(p: ffi::AnalysisProperties) -> PhysicalProperties {
    PhysicalProperties {
        volume: p.volume,
        area: p.area,
        mass: p.mass,
        center_of_mass: p.center,
        inertia: rows(p.inertia),
        principal_moments: p.principal_moments,
        principal_axes: rows(p.principal_axes),
    }
}

pub fn measurement(m: ffi::AnalysisMeasure) -> Measurement {
    Measurement {
        kind: m.kind,
        volume: m.has_volume.then_some(m.volume),
        area: m.has_area.then_some(m.area),
        length: m.has_length.then_some(m.length),
        point: m.has_point.then_some(m.point),
        circle: m.has_circle.then_some(CircleMeasure {
            center: m.center,
            axis: m.axis,
            radius: m.radius,
            sweep: m.sweep,
        }),
    }
}

pub fn separation(s: ffi::AnalysisSeparation) -> Separation {
    Separation {
        distance: s.distance,
        on_a: s.on_a,
        on_b: s.on_b,
        inside: s.inside,
        angle: s.has_angle.then_some(s.angle),
    }
}

fn deviation(d: &ffi::AnalysisDeviation) -> Deviation {
    Deviation {
        max: d.max,
        rms: d.rms,
        at: d.at,
        samples: d.samples,
    }
}

pub fn comparison(c: ffi::AnalysisComparison) -> Comparison {
    let differences = c.has_differences;
    Comparison {
        a_to_b: deviation(&c.a_to_b),
        b_to_a: deviation(&c.b_to_a),
        step_bodies: c.step_bodies,
        volume_a: c.volume_a,
        volume_b: c.volume_b,
        a_minus_b: differences.then_some(c.a_minus_b),
        b_minus_a: differences.then_some(c.b_minus_a),
        relative_difference: differences.then_some(c.relative_difference),
        max_deviation: c.max_deviation,
        bounds_difference: c.bounds_difference,
    }
}
