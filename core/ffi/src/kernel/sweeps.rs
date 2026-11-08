// SPDX-License-Identifier: MIT
//! Sweeps, lofts, pipes, coils, ribs and webs (F3)
//! (`geometry/include/mitcad/geometry/{path_sweep,loft,rib}.hpp`).

#[cxx::bridge(namespace = "mitcad::bridge")]
pub mod ffi {
    /// A path curve with its name: `kind` 0 line, 1 conic, 2 B-spline, 3
    /// helix (its radius as `major`, its pitch as `minor`, its axis as
    /// `normal`); only the fields of its kind are used (see
    /// `mitcad_model::Curve3`).
    #[derive(Clone, Debug)]
    struct SweepCurve {
        name: String,
        kind: u8,
        start: [f64; 3],
        end: [f64; 3],
        center: [f64; 3],
        normal: [f64; 3],
        x_axis: [f64; 3],
        major: f64,
        minor: f64,
        first: f64,
        last: f64,
        closed: bool,
        degree: u32,
        /// x0, y0, z0, x1, ...
        poles: Vec<f64>,
        weights: Vec<f64>,
        knots: Vec<f64>,
    }

    /// `geometry::SweepSpec` without the profiles; `scaling` 0 scale, 1
    /// stretch, 2 none. Guide surfaces (mitcad#157): face names and the
    /// index of each one's body in the bodies passed along.
    struct SweepInput {
        path: Vec<SweepCurve>,
        extent1: f64,
        extent2: f64,
        parallel: bool,
        twist: f64,
        taper: f64,
        rail: Vec<SweepCurve>,
        scaling: u8,
        guide_faces: Vec<String>,
        guide_bodies: Vec<usize>,
        /// A fixed binormal (mitcad#397) when `fixed_binormal`.
        fixed_binormal: bool,
        binormal: [f64; 3],
        /// A second guide rail (empty for none) and the end scale (1 for
        /// none; mitcad#464).
        rail2: Vec<SweepCurve>,
        end_scale: f64,
        /// The rails' points where they cross the plane square to the
        /// path, else at the path's fractions of their lengths
        /// (mitcad#469).
        rail_plane: bool,
        /// The path's Frenet frame, or (non-empty) towards an auxiliary
        /// curve, its point at the same fraction of its length with
        /// `curvilinear` (mitcad#539, `mitcad_model::sweeps::PathFrame`).
        frenet: bool,
        auxiliary: Vec<SweepCurve>,
        curvilinear: bool,
        /// About a constant binormal when `constant_binormal`.
        constant_binormal: bool,
        constant_direction: [f64; 3],
        /// A planar body face as the profile (mitcad#543): the face of the
        /// body at index `profile_body` of the bodies passed along, its
        /// caps keyed by `profile_key`.
        has_profile_face: bool,
        profile_body: usize,
        profile_face: String,
        profile_key: String,
    }

    /// A loft section: `kind` 0 a region (`index` into the frames and
    /// regions), 1 a face (`index` into the bodies) or 2 a point.
    struct LoftSectionInput {
        kind: u8,
        index: usize,
        face: String,
        point: [f64; 3],
    }

    /// `geometry::LoftSpec` with the sections' frames, regions and bodies
    /// passed along. The rails' curves one after another, `rails` holding
    /// how many each has; end kinds 0 free, 1 direction, 2 tangent, 3
    /// smooth, 4 point sharp, 5 point tangent.
    struct LoftInput {
        sections: Vec<LoftSectionInput>,
        centerline: Vec<SweepCurve>,
        ruled: bool,
        closed: bool,
        solid: bool,
        rail_curves: Vec<SweepCurve>,
        rails: Vec<usize>,
        start_kind: u8,
        start_angle: f64,
        start_weight: f64,
        end_kind: u8,
        end_angle: f64,
        end_weight: f64,
        /// A frame along the centre line (mitcad#543), as `SweepInput`'s.
        frenet: bool,
        auxiliary: Vec<SweepCurve>,
        curvilinear: bool,
        constant_binormal: bool,
        constant_direction: [f64; 3],
        /// The sections only move along the centre line (mitcad#547).
        parallel: bool,
    }

    /// `section` 0 circular, 1 square, 2 triangular; `thickness` 0 for a
    /// solid pipe; `corners` 0 mitered, 1 rounded (mitcad#1448).
    struct PipeInput {
        path: Vec<SweepCurve>,
        extent1: f64,
        extent2: f64,
        section: u8,
        size: f64,
        thickness: f64,
        corners: u8,
    }

    /// `section` 0 circular, 1 square, 2 triangular external, 3 triangular
    /// internal; `position` 0 inside, 1 on centre, 2 outside.
    struct CoilInput {
        diameter: f64,
        revolutions: f64,
        pitch: f64,
        angle: f64,
        spiral: bool,
        clockwise: bool,
        section: u8,
        position: u8,
        size: f64,
    }

    /// `geometry::HelixSweepSpec` without the profiles (FreeCAD's
    /// helices, mitcad#4).
    struct HelixInput {
        origin: [f64; 3],
        direction: [f64; 3],
        pitch: f64,
        revolutions: f64,
        left_handed: bool,
        growth: f64,
        flip: bool,
        freecad: bool,
        /// The start's flat and transition, the end's transition and flat
        /// (radians; mitcad#235).
        ends: [f64; 4],
        /// The pieces' seams shifted by this many turns (0 for none;
        /// mitcad#235, `geometry::HelixSweepSpec::piece_shift`).
        piece_shift: f64,
    }

    /// The chains' curves one after another, `chains` holding how many each
    /// has; `location` 0 symmetric, 1 side 1, 2 side 2.
    struct RibInput {
        web: bool,
        curves: Vec<SweepCurve>,
        chains: Vec<usize>,
        thickness: f64,
        location: u8,
        has_depth: bool,
        depth: f64,
        flip: bool,
    }

    unsafe extern "C++" {
        include!("bridge/sweeps.hpp");
        include!("bridge/trycatch.hpp");

        #[namespace = "mitcad::geometry"]
        type Shape = crate::kernel::shape::ffi::Shape;
        type ShapeList = crate::kernel::shape::ffi::ShapeList;
        type Frame = crate::kernel::profile::ffi::Frame;
        type Region = crate::kernel::profile::ffi::Region;

        fn sweep_solid(
            feature: &str,
            frame: &Frame,
            regions: &[Region],
            input: &SweepInput,
            bodies: &ShapeList,
        ) -> Result<SharedPtr<Shape>>;
        fn loft_solid(
            feature: &str,
            input: &LoftInput,
            frames: &[Frame],
            regions: &[Region],
            bodies: &ShapeList,
        ) -> Result<SharedPtr<Shape>>;
        fn pipe_solid(feature: &str, input: &PipeInput) -> Result<SharedPtr<Shape>>;
        fn coil_solid(feature: &str, frame: &Frame, input: &CoilInput) -> Result<SharedPtr<Shape>>;
        fn helix_solid(
            feature: &str,
            frame: &Frame,
            regions: &[Region],
            input: &HelixInput,
            bodies: &ShapeList,
        ) -> Result<SharedPtr<Shape>>;
        fn rib_solid(
            feature: &str,
            frame: &Frame,
            input: &RibInput,
            bodies: &ShapeList,
        ) -> Result<SharedPtr<Shape>>;
    }
}

use cxx::SharedPtr;
use mitcad_model::{
    CoilPosition, CoilSection, CoilSpec, Curve3, KernelError, LoftEnd, LoftEndKind, LoftSection,
    LoftSpec, PathCurve, PipeSection, PipeSpec, ProfileScaling, RibSpec, SweepOrientation,
    SweepSpec, ThicknessLocation,
};
// Sweeps along guide surfaces (mitcad#157).
use mitcad_model::GuideSurface;
// Sweeps' rail points (mitcad#469).
use mitcad_model::RailPoint;
// Pipes' corners (mitcad#1448).
use mitcad_model::PipeCorners;
// Frenet and auxiliary frames (mitcad#539).
use mitcad_model::sweeps::PathFrame;
// Body faces as profiles (mitcad#543).
use mitcad_model::sweeps::ProfileFace;

use super::shape::ffi::new_shape_list;
use super::{Shape, failed, occt, profile};

pub(crate) fn curve(piece: &PathCurve) -> ffi::SweepCurve {
    let mut input = ffi::SweepCurve {
        name: piece.name.clone(),
        kind: 0,
        start: [0.0; 3],
        end: [0.0; 3],
        center: [0.0; 3],
        normal: [0.0, 0.0, 1.0],
        x_axis: [1.0, 0.0, 0.0],
        major: 0.0,
        minor: 0.0,
        first: 0.0,
        last: 0.0,
        closed: false,
        degree: 0,
        poles: Vec::new(),
        weights: Vec::new(),
        knots: Vec::new(),
    };
    match &piece.curve {
        Curve3::Point(p) => {
            input.start = *p;
            input.end = *p;
        }
        Curve3::Line { start, end } => {
            input.start = *start;
            input.end = *end;
        }
        Curve3::Conic {
            center,
            normal,
            x_axis,
            major,
            minor,
            start,
            end,
            closed,
        } => {
            input.kind = 1;
            input.center = *center;
            input.normal = *normal;
            input.x_axis = *x_axis;
            input.major = *major;
            input.minor = *minor;
            input.first = *start;
            input.last = *end;
            input.closed = *closed;
        }
        Curve3::BSpline {
            degree,
            poles,
            weights,
            knots,
        } => {
            input.kind = 2;
            input.degree = *degree;
            input.poles = poles.iter().flatten().copied().collect();
            input.weights = weights.clone();
            input.knots = knots.clone();
        }
        // A helix (mitcad#385): the radius as `major`, the pitch as `minor`.
        Curve3::Helix {
            center,
            axis,
            x_axis,
            radius,
            pitch,
            start,
            end,
        } => {
            input.kind = 3;
            input.center = *center;
            input.normal = *axis;
            input.x_axis = *x_axis;
            input.major = *radius;
            input.minor = *pitch;
            input.first = *start;
            input.last = *end;
        }
    }
    input
}

fn curves(path: &[PathCurve]) -> Vec<ffi::SweepCurve> {
    path.iter().map(curve).collect()
}

pub fn sweep(spec: &SweepSpec<'_>) -> Result<SharedPtr<Shape>, KernelError> {
    surface_sweep(spec, &[])
}

/// A sweep, along guide surfaces when there are some (mitcad#157).
pub fn surface_sweep(
    spec: &SweepSpec<'_>,
    surfaces: &[GuideSurface<'_, SharedPtr<Shape>>],
) -> Result<SharedPtr<Shape>, KernelError> {
    sweep_with(spec, surfaces, None)
}

/// A sweep of a planar body face (mitcad#543).
pub fn face_sweep(
    spec: &SweepSpec<'_>,
    face: &ProfileFace<'_, SharedPtr<Shape>>,
) -> Result<SharedPtr<Shape>, KernelError> {
    sweep_with(spec, &[], Some(face))
}

fn sweep_with(
    spec: &SweepSpec<'_>,
    surfaces: &[GuideSurface<'_, SharedPtr<Shape>>],
    face: Option<&ProfileFace<'_, SharedPtr<Shape>>>,
) -> Result<SharedPtr<Shape>, KernelError> {
    let mut bodies = new_shape_list();
    let mut guide_faces = Vec::new();
    let mut guide_bodies = Vec::new();
    for surface in surfaces {
        occt(surface.body)?;
        guide_faces.push(surface.face.to_string());
        guide_bodies.push(bodies.size());
        bodies.pin_mut().push(surface.body.clone());
    }
    // A body face as the profile (mitcad#543): its body after the guide
    // surfaces'.
    let profile_body = bodies.size();
    if let Some(face) = face {
        occt(face.body)?;
        bodies.pin_mut().push(face.body.clone());
    }
    let regions: Vec<profile::ffi::Region> = spec.regions.iter().map(profile::region).collect();
    let input = ffi::SweepInput {
        has_profile_face: face.is_some(),
        profile_body,
        profile_face: face.map(|f| f.face.to_string()).unwrap_or_default(),
        profile_key: face.map(|f| f.key.to_string()).unwrap_or_default(),
        guide_faces,
        guide_bodies,
        path: curves(spec.path),
        extent1: spec.extent1,
        extent2: spec.extent2,
        parallel: spec.orientation == SweepOrientation::Parallel,
        twist: spec.twist,
        taper: spec.taper,
        rail: spec.guide.map(|g| curves(g.rail)).unwrap_or_default(),
        scaling: match spec.guide.map(|g| g.scaling) {
            Some(ProfileScaling::Stretch) => 1,
            Some(ProfileScaling::None) => 2,
            _ => 0,
        },
        fixed_binormal: spec.binormal.is_some(),
        binormal: spec.binormal.unwrap_or([0.0, 0.0, 1.0]),
        rail2: spec
            .guide
            .and_then(|g| g.rail2)
            .map(curves)
            .unwrap_or_default(),
        end_scale: spec.end_scale,
        rail_plane: spec.guide.is_some_and(|g| g.point == RailPoint::Plane),
        frenet: spec.path_frame == Some(PathFrame::Frenet),
        auxiliary: match spec.path_frame {
            Some(PathFrame::Auxiliary { curve, .. }) => curves(curve),
            _ => Vec::new(),
        },
        curvilinear: matches!(
            spec.path_frame,
            Some(PathFrame::Auxiliary {
                curvilinear: true,
                ..
            })
        ),
        constant_binormal: matches!(spec.path_frame, Some(PathFrame::ConstantBinormal(_))),
        constant_direction: match spec.path_frame {
            Some(PathFrame::ConstantBinormal(b)) => b,
            _ => [0.0, 0.0, 1.0],
        },
    };
    ffi::sweep_solid(
        &spec.feature.to_string(),
        &profile::frame(&spec.frame),
        &regions,
        &input,
        &bodies,
    )
    .map_err(failed)
}

pub fn loft(spec: &LoftSpec<'_, SharedPtr<Shape>>) -> Result<SharedPtr<Shape>, KernelError> {
    let mut frames = Vec::new();
    let mut regions = Vec::new();
    let mut bodies = new_shape_list();
    let mut sections = Vec::new();
    for section in &spec.sections {
        let mut input = ffi::LoftSectionInput {
            kind: 0,
            index: 0,
            face: String::new(),
            point: [0.0; 3],
        };
        match section {
            LoftSection::Region { frame, region } => {
                input.index = regions.len();
                frames.push(profile::frame(frame));
                regions.push(profile::region(region));
            }
            LoftSection::Face { body, face } => {
                occt(body)?;
                input.kind = 1;
                input.index = bodies.size();
                input.face = face.to_string();
                bodies.pin_mut().push((*body).clone());
            }
            LoftSection::Point(point) => {
                input.kind = 2;
                input.point = *point;
            }
        }
        sections.push(input);
    }
    let kind = |end: LoftEnd| match end.kind {
        LoftEndKind::Free => 0,
        LoftEndKind::Direction => 1,
        LoftEndKind::Tangent => 2,
        LoftEndKind::Smooth => 3,
        LoftEndKind::PointSharp => 4,
        LoftEndKind::PointTangent => 5,
    };
    let input = ffi::LoftInput {
        sections,
        centerline: spec.centerline.map(curves).unwrap_or_default(),
        ruled: spec.ruled,
        closed: spec.closed,
        solid: spec.solid,
        rail_curves: spec
            .rails
            .iter()
            .flat_map(|r| r.iter().map(curve))
            .collect(),
        rails: spec.rails.iter().map(|r| r.len()).collect(),
        start_kind: kind(spec.start),
        start_angle: spec.start.angle,
        start_weight: spec.start.weight,
        end_kind: kind(spec.end),
        end_angle: spec.end.angle,
        end_weight: spec.end.weight,
        frenet: spec.centerline_frame == Some(PathFrame::Frenet),
        auxiliary: match spec.centerline_frame {
            Some(PathFrame::Auxiliary { curve, .. }) => curves(curve),
            _ => Vec::new(),
        },
        curvilinear: matches!(
            spec.centerline_frame,
            Some(PathFrame::Auxiliary {
                curvilinear: true,
                ..
            })
        ),
        constant_binormal: matches!(spec.centerline_frame, Some(PathFrame::ConstantBinormal(_))),
        constant_direction: match spec.centerline_frame {
            Some(PathFrame::ConstantBinormal(b)) => b,
            _ => [0.0, 0.0, 1.0],
        },
        parallel: spec.centerline_parallel,
    };
    ffi::loft_solid(
        &spec.feature.to_string(),
        &input,
        &frames,
        &regions,
        &bodies,
    )
    .map_err(failed)
}

pub fn pipe(spec: &PipeSpec<'_>) -> Result<SharedPtr<Shape>, KernelError> {
    let input = ffi::PipeInput {
        path: curves(spec.path),
        extent1: spec.extent1,
        extent2: spec.extent2,
        section: match spec.section {
            PipeSection::Circular => 0,
            PipeSection::Square => 1,
            PipeSection::Triangular => 2,
        },
        size: spec.size,
        thickness: spec.thickness.unwrap_or(0.0),
        corners: match spec.corners {
            PipeCorners::Mitered => 0,
            PipeCorners::Rounded => 1,
        },
    };
    ffi::pipe_solid(&spec.feature.to_string(), &input).map_err(failed)
}

pub fn coil(spec: &CoilSpec) -> Result<SharedPtr<Shape>, KernelError> {
    let input = ffi::CoilInput {
        diameter: spec.diameter,
        revolutions: spec.revolutions,
        pitch: spec.pitch,
        angle: spec.angle,
        spiral: spec.spiral,
        clockwise: spec.clockwise,
        section: match spec.section {
            CoilSection::Circular => 0,
            CoilSection::Square => 1,
            CoilSection::TriangularExternal => 2,
            CoilSection::TriangularInternal => 3,
        },
        position: match spec.position {
            CoilPosition::Inside => 0,
            CoilPosition::OnCenter => 1,
            CoilPosition::Outside => 2,
        },
        size: spec.size,
    };
    ffi::coil_solid(
        &spec.feature.to_string(),
        &profile::frame(&spec.frame),
        &input,
    )
    .map_err(failed)
}

/// FreeCAD's helices (mitcad#4): the profiles turned about the axis as they
/// move along it.
pub fn helix(
    spec: &mitcad_model::HelixSpec<'_, SharedPtr<Shape>>,
) -> Result<SharedPtr<Shape>, KernelError> {
    let mut bodies = new_shape_list();
    for body in spec.bodies {
        occt(body)?;
        bodies.pin_mut().push(body.clone());
    }
    let regions: Vec<profile::ffi::Region> = spec.regions.iter().map(profile::region).collect();
    let input = ffi::HelixInput {
        origin: spec.origin,
        direction: spec.direction,
        pitch: spec.pitch,
        revolutions: spec.revolutions,
        left_handed: spec.left_handed,
        growth: spec.growth,
        flip: spec.flip,
        freecad: spec.freecad,
        ends: spec.ends,
        piece_shift: spec.piece_shift,
    };
    ffi::helix_solid(
        &spec.feature.to_string(),
        &profile::frame(&spec.frame),
        &regions,
        &input,
        &bodies,
    )
    .map_err(failed)
}

pub fn rib(spec: &RibSpec<'_, SharedPtr<Shape>>) -> Result<SharedPtr<Shape>, KernelError> {
    let mut bodies = new_shape_list();
    for body in &spec.bodies {
        occt(body)?;
        bodies.pin_mut().push(body.clone());
    }
    let input = ffi::RibInput {
        web: spec.web,
        curves: spec
            .chains
            .iter()
            .flat_map(|c| c.iter().map(curve))
            .collect(),
        chains: spec.chains.iter().map(Vec::len).collect(),
        thickness: spec.thickness,
        location: match spec.location {
            ThicknessLocation::Symmetric => 0,
            ThicknessLocation::Side1 => 1,
            ThicknessLocation::Side2 => 2,
        },
        has_depth: spec.depth.is_some(),
        depth: spec.depth.unwrap_or(0.0),
        flip: spec.flip,
    };
    ffi::rib_solid(
        &spec.feature.to_string(),
        &profile::frame(&spec.frame),
        &input,
        &bodies,
    )
    .map_err(failed)
}
