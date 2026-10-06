// SPDX-License-Identifier: MIT
//! Profile features: extrude with its options, revolve, hole and thread,
//! and the face and edge geometry they refer to
//! (`geometry/include/mitcad/geometry/{extrude,revolve,hole,thread,reference}.hpp`).

#[cxx::bridge(namespace = "mitcad::bridge")]
pub mod ffi {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum TargetKind {
        /// No target: a distance (an offset for a start).
        Unset,
        Plane,
        Face,
        Body,
    }

    /// `geometry::Target`; `body` indexes the bodies passed along.
    #[derive(Clone, Debug)]
    struct TargetInput {
        kind: TargetKind,
        plane_origin: [f64; 3],
        plane_normal: [f64; 3],
        body: usize,
        face: String,
        extend: bool,
        through: bool,
        offset: f64,
    }

    /// `geometry::ExtrudeSide`; `wall_location` 0 side 1, 1 centre, 2 side 2.
    #[derive(Clone, Debug)]
    struct ExtrudeSideInput {
        target: TargetInput,
        distance: f64,
        taper: f64,
        thin: bool,
        wall_location: u8,
        wall_thickness: f64,
    }

    #[derive(Clone, Debug)]
    struct ExtrudeFeatureInput {
        direction: [f64; 3],
        start_offset: f64,
        start: TargetInput,
        side1: ExtrudeSideInput,
        two_sides: bool,
        side2: ExtrudeSideInput,
    }

    #[derive(Clone, Debug)]
    struct RevolveInput {
        axis_origin: [f64; 3],
        axis_direction: [f64; 3],
        angle1: f64,
        two_sides: bool,
        angle2: f64,
        target: TargetInput,
    }

    /// `geometry::HoleSpec`: positions as x, y, z triples; `shape` 0
    /// simple, 1 counterbore, 2 countersink, 3 counterdrill (the
    /// counterbore's diameter and depth, the countersink's angle); `extent`
    /// 0 distance, 1 through all (the first `through` bodies), 2 target.
    #[derive(Clone, Debug)]
    struct HoleInput {
        origins: Vec<f64>,
        directions: Vec<f64>,
        diameter: f64,
        shape: u8,
        counterbore_diameter: f64,
        counterbore_depth: f64,
        countersink_diameter: f64,
        countersink_angle: f64,
        flat: bool,
        tip_angle: f64,
        extent: u8,
        depth: f64,
        through: usize,
        target: TargetInput,
        /// The wall's angle to the axis, positive narrowing (mitcad#4).
        taper: f64,
    }

    #[derive(Clone, Debug)]
    struct ThreadInput {
        faces: Vec<String>,
        pitch: f64,
        depth: f64,
        right_handed: bool,
        full_length: bool,
        length: f64,
        offset: f64,
        high_end: bool,
    }

    struct PlaneOutput {
        origin: [f64; 3],
        normal: [f64; 3],
    }

    struct AxisOutput {
        origin: [f64; 3],
        direction: [f64; 3],
    }

    struct CylinderOutput {
        axis: AxisOutput,
        radius: f64,
        length: f64,
        internal: bool,
    }

    unsafe extern "C++" {
        include!("bridge/profile_feature.hpp");

        #[namespace = "mitcad::geometry"]
        type Shape = crate::kernel::shape::ffi::Shape;
        type ShapeList = crate::kernel::shape::ffi::ShapeList;
        type Frame = crate::kernel::profile::ffi::Frame;
        type Region = crate::kernel::profile::ffi::Region;

        fn extrude_feature(
            feature: &str,
            frame: &Frame,
            regions: &[Region],
            input: &ExtrudeFeatureInput,
            bodies: &ShapeList,
        ) -> Result<SharedPtr<Shape>>;
        fn revolve(
            feature: &str,
            frame: &Frame,
            regions: &[Region],
            input: &RevolveInput,
            bodies: &ShapeList,
        ) -> Result<SharedPtr<Shape>>;
        fn hole_tool(
            feature: &str,
            input: &HoleInput,
            bodies: &ShapeList,
        ) -> Result<SharedPtr<Shape>>;
        fn modeled_thread(
            feature: &str,
            body: &Shape,
            input: &ThreadInput,
        ) -> Result<SharedPtr<Shape>>;
        fn face_plane(shape: &Shape, face: &str) -> Result<PlaneOutput>;
        fn face_cylinder(shape: &Shape, face: &str) -> Result<CylinderOutput>;
    }
}

use cxx::{SharedPtr, UniquePtr};
use mitcad_model::{
    Axis, Bound, Cylinder, ExtrudeFeatureSpec, ExtrudeSide, ExtrudeStart, HoleEnd, HoleShape,
    HoleSpec, KernelError, Plane, RevolveSpec, SideEnd, Target, ThreadSpec, WallLocation,
};

use super::shape::ffi::{ShapeList, new_shape_list};
use super::{Shape, occt};

/// Shapes the targets refer to, by index.
pub struct Bodies(UniquePtr<ShapeList>);

impl Default for Bodies {
    fn default() -> Self {
        Self(new_shape_list())
    }
}

impl Bodies {
    fn add(&mut self, shape: &SharedPtr<Shape>) -> Result<usize, KernelError> {
        occt(shape)?;
        let index = self.0.size();
        self.0.pin_mut().push(shape.clone());
        Ok(index)
    }

    pub fn list(&self) -> &ShapeList {
        &self.0
    }
}

fn unset() -> ffi::TargetInput {
    ffi::TargetInput {
        kind: ffi::TargetKind::Unset,
        plane_origin: [0.0; 3],
        plane_normal: [0.0, 0.0, 1.0],
        body: 0,
        face: String::new(),
        extend: true,
        through: false,
        offset: 0.0,
    }
}

fn target(
    target: &Target<SharedPtr<Shape>>,
    offset: f64,
    bodies: &mut Bodies,
) -> Result<ffi::TargetInput, KernelError> {
    let mut input = unset();
    input.offset = offset;
    match target {
        Target::Plane(Plane { origin, normal }) => {
            input.kind = ffi::TargetKind::Plane;
            input.plane_origin = *origin;
            input.plane_normal = *normal;
        }
        Target::Face { body, face, extend } => {
            input.kind = ffi::TargetKind::Face;
            input.body = bodies.add(body)?;
            input.face = face.to_string();
            input.extend = *extend;
        }
        Target::Body { body, through } => {
            input.kind = ffi::TargetKind::Body;
            input.body = bodies.add(body)?;
            input.through = *through;
        }
    }
    Ok(input)
}

fn bound(
    bound: &Bound<SharedPtr<Shape>>,
    bodies: &mut Bodies,
) -> Result<ffi::TargetInput, KernelError> {
    target(&bound.target, bound.offset, bodies)
}

fn side(
    side: &ExtrudeSide<SharedPtr<Shape>>,
    bodies: &mut Bodies,
) -> Result<ffi::ExtrudeSideInput, KernelError> {
    let (target, distance) = match &side.end {
        SideEnd::Distance(d) => (unset(), *d),
        SideEnd::Target(b) => (bound(b, bodies)?, 0.0),
    };
    let (thin, wall_location, wall_thickness) = match side.thin {
        None => (false, 1, 0.0),
        Some(wall) => (
            true,
            match wall.location {
                WallLocation::Side1 => 0,
                WallLocation::Center => 1,
                WallLocation::Side2 => 2,
            },
            wall.thickness,
        ),
    };
    Ok(ffi::ExtrudeSideInput {
        target,
        distance,
        taper: side.taper,
        thin,
        wall_location,
        wall_thickness,
    })
}

pub fn extrude_input(
    spec: &ExtrudeFeatureSpec<'_, SharedPtr<Shape>>,
    bodies: &mut Bodies,
) -> Result<ffi::ExtrudeFeatureInput, KernelError> {
    let (start_offset, start) = match &spec.start {
        ExtrudeStart::Offset(offset) => (*offset, unset()),
        ExtrudeStart::Object(b) => (0.0, bound(b, bodies)?),
    };
    let side1 = side(&spec.side1, bodies)?;
    let side2 = match &spec.side2 {
        Some(two) => Some(side(two, bodies)?),
        None => None,
    };
    Ok(ffi::ExtrudeFeatureInput {
        direction: spec.direction,
        start_offset,
        start,
        two_sides: side2.is_some(),
        side2: side2.unwrap_or_else(|| side1.clone()),
        side1,
    })
}

pub fn revolve_input(
    spec: &RevolveSpec<'_, SharedPtr<Shape>>,
    bodies: &mut Bodies,
) -> Result<ffi::RevolveInput, KernelError> {
    Ok(ffi::RevolveInput {
        axis_origin: spec.axis.origin,
        axis_direction: spec.axis.direction,
        angle1: spec.angle1,
        two_sides: spec.angle2.is_some(),
        angle2: spec.angle2.unwrap_or(0.0),
        target: match &spec.target {
            Some(t) => target(t, 0.0, bodies)?,
            None => unset(),
        },
    })
}

pub fn hole_input(
    spec: &HoleSpec<'_, SharedPtr<Shape>>,
    bodies: &mut Bodies,
) -> Result<ffi::HoleInput, KernelError> {
    let mut input = ffi::HoleInput {
        origins: spec.positions.iter().flat_map(|p| p.origin).collect(),
        directions: spec.positions.iter().flat_map(|p| p.direction).collect(),
        diameter: spec.diameter,
        shape: 0,
        counterbore_diameter: 0.0,
        counterbore_depth: 0.0,
        countersink_diameter: 0.0,
        countersink_angle: 0.0,
        flat: spec.tip_angle.is_none(),
        tip_angle: spec.tip_angle.unwrap_or(0.0),
        extent: 0,
        depth: 0.0,
        through: 0,
        target: unset(),
        taper: spec.taper,
    };
    match spec.shape {
        HoleShape::Simple => {}
        HoleShape::Counterbore { diameter, depth } => {
            input.shape = 1;
            input.counterbore_diameter = diameter;
            input.counterbore_depth = depth;
        }
        HoleShape::Countersink { diameter, angle } => {
            input.shape = 2;
            input.countersink_diameter = diameter;
            input.countersink_angle = angle;
        }
        HoleShape::Counterdrill {
            diameter,
            depth,
            angle,
        } => {
            input.shape = 3;
            input.counterbore_diameter = diameter;
            input.counterbore_depth = depth;
            input.countersink_angle = angle;
        }
    }
    match &spec.end {
        HoleEnd::Distance(depth) => input.depth = *depth,
        HoleEnd::ThroughAll => {
            input.extent = 1;
            for body in &spec.bodies {
                bodies.add(body)?;
            }
            input.through = spec.bodies.len();
        }
        HoleEnd::Target(b) => {
            input.extent = 2;
            input.target = bound(b, bodies)?;
        }
    }
    Ok(input)
}

pub fn thread_input(spec: &ThreadSpec<'_>) -> ffi::ThreadInput {
    let (length, offset, high_end) = spec.part.unwrap_or((0.0, 0.0, true));
    ffi::ThreadInput {
        faces: spec.faces.iter().map(ToString::to_string).collect(),
        pitch: spec.pitch,
        depth: spec.depth,
        right_handed: spec.right_handed,
        full_length: spec.part.is_none(),
        length,
        offset,
        high_end,
    }
}

pub fn plane(output: ffi::PlaneOutput) -> Plane {
    Plane {
        origin: output.origin,
        normal: output.normal,
    }
}

pub fn axis(output: &ffi::AxisOutput) -> Axis {
    Axis {
        origin: output.origin,
        direction: output.direction,
    }
}

pub fn cylinder(output: ffi::CylinderOutput) -> Cylinder {
    Cylinder {
        axis: axis(&output.axis),
        radius: output.radius,
        length: output.length,
        internal: output.internal,
    }
}
