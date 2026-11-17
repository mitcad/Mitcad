// SPDX-License-Identifier: MIT
//! Flex drape (mitcad#1713): the chain of a Flex item let hang and the
//! nodes that keep its shape (`geometry/include/mitcad/geometry/flex_drape.hpp`;
//! the model's side is `mitcad_model::flex_drape`).

#[cxx::bridge(namespace = "mitcad::bridge")]
pub mod ffi {
    /// `mitcad_model::flex_drape::DrapeContact`: `obstacle` -1 for a point
    /// in the air, `face` empty for a face without a name.
    struct DrapeContactData {
        obstacle: i32,
        face: String,
        normal: [f64; 3],
    }

    /// `mitcad_model::flex_drape::DrapeStepSpec` without the obstacles:
    /// points and velocities as x0, y0, z0, x1, ...; `pinned` 1 for a
    /// pinned point; a bend radius that is not above zero is none; `grab`
    /// only with `has_grab`; `carry` what the step before left of a substep.
    struct DrapeStepInput {
        points: Vec<f64>,
        velocities: Vec<f64>,
        pinned: Vec<u8>,
        segment: f64,
        resting: f64,
        bend_radius: f64,
        gravity: [f64; 3],
        seconds: f64,
        has_grab: bool,
        grab: u32,
        target: [f64; 3],
        still: f64,
        carry: f64,
        damping: f64,
    }

    /// `mitcad_model::flex_drape::DrapeStep`, its vectors as x0, y0, z0,
    /// x1, ...
    struct DrapeStepData {
        points: Vec<f64>,
        velocities: Vec<f64>,
        normals: Vec<f64>,
        contacts: Vec<DrapeContactData>,
        still: f64,
        at_rest: bool,
        time: f64,
        carry: f64,
    }

    /// `mitcad_model::flex_drape::DrapeNodesSpec` without the obstacles;
    /// a limit that is not above zero is none, a port's fields only with
    /// their flags.
    struct DrapeNodesInput {
        points: Vec<f64>,
        contacts: Vec<DrapeContactData>,
        keep: Vec<u32>,
        has_start_port: bool,
        start_direction: [f64; 3],
        has_start_x_axis: bool,
        start_x_axis: [f64; 3],
        has_end_port: bool,
        end_direction: [f64; 3],
        has_end_x_axis: bool,
        end_x_axis: [f64; 3],
        resting: f64,
        half_width: f64,
        reach: f64,
        bend_radius: f64,
        bend_radius_edge: f64,
        max_twist: f64,
        tolerance: f64,
    }

    /// `mitcad_model::flex_drape::DrapeNodes`: `deviation` below zero when
    /// no route could be made through the nodes.
    struct DrapeNodesData {
        nodes: Vec<u32>,
        deviation: f64,
    }

    unsafe extern "C++" {
        include!("bridge/flex_drape.hpp");
        include!("bridge/trycatch.hpp");

        type ShapeList = crate::kernel::shape::ffi::ShapeList;

        /// One step of the chain among the obstacles
        /// (`geometry::flex_drape_step`).
        fn flex_drape_step(obstacles: &ShapeList, input: &DrapeStepInput) -> Result<DrapeStepData>;

        /// The nodes of the resting chain (`geometry::flex_drape_nodes`).
        fn flex_drape_nodes(
            obstacles: &ShapeList,
            input: &DrapeNodesInput,
        ) -> Result<DrapeNodesData>;
    }
}

use cxx::SharedPtr;
use mitcad_model::flex_drape::{
    DrapeContact, DrapeNodes, DrapeNodesSpec, DrapePort, DrapeStep, DrapeStepSpec,
};
use mitcad_model::{KernelError, TopoName};

use super::{Shape, failed, shape_list};

fn flat(points: &[[f64; 3]]) -> Vec<f64> {
    points.iter().flatten().copied().collect()
}

fn triples(values: &[f64]) -> Vec<[f64; 3]> {
    values.chunks(3).map(|p| [p[0], p[1], p[2]]).collect()
}

fn contact_data(contact: Option<&DrapeContact>) -> ffi::DrapeContactData {
    match contact {
        Some(c) => ffi::DrapeContactData {
            obstacle: i32::try_from(c.obstacle).unwrap_or(-1),
            face: c.face.as_ref().map(ToString::to_string).unwrap_or_default(),
            normal: c.normal,
        },
        None => ffi::DrapeContactData {
            obstacle: -1,
            face: String::new(),
            normal: [0.0, 0.0, 1.0],
        },
    }
}

fn contact(c: &ffi::DrapeContactData) -> Option<DrapeContact> {
    let obstacle = usize::try_from(c.obstacle).ok()?;
    let face = match c.face.parse::<TopoName>() {
        Ok(TopoName::Face(face)) if !c.face.is_empty() => Some(face),
        _ => None,
    };
    Some(DrapeContact {
        obstacle,
        face,
        normal: c.normal,
    })
}

/// `Kernel::flex_drape_step`.
pub fn step(spec: &DrapeStepSpec<'_, SharedPtr<Shape>>) -> Result<DrapeStep, KernelError> {
    let obstacles = shape_list(spec.obstacles)?;
    let (has_grab, grab, target) = match spec.grab {
        Some((index, target)) => (true, u32::try_from(index).unwrap_or(u32::MAX), target),
        None => (false, 0, [0.0; 3]),
    };
    let input = ffi::DrapeStepInput {
        points: flat(spec.points),
        velocities: flat(spec.velocities),
        pinned: spec.pinned.iter().map(|p| u8::from(*p)).collect(),
        segment: spec.segment,
        resting: spec.resting,
        bend_radius: spec.bend_radius.unwrap_or(0.0),
        gravity: spec.gravity,
        seconds: spec.seconds,
        has_grab,
        grab,
        target,
        still: spec.still,
        carry: spec.carry,
        damping: spec.damping,
    };
    let data = ffi::flex_drape_step(&obstacles, &input).map_err(failed)?;
    Ok(DrapeStep {
        points: triples(&data.points),
        velocities: triples(&data.velocities),
        normals: triples(&data.normals),
        contacts: data.contacts.iter().map(contact).collect(),
        still: data.still,
        at_rest: data.at_rest,
        time: data.time,
        carry: data.carry,
    })
}

/// `Kernel::flex_drape_nodes`.
pub fn nodes(spec: &DrapeNodesSpec<'_, SharedPtr<Shape>>) -> Result<DrapeNodes, KernelError> {
    let obstacles = shape_list(spec.obstacles)?;
    let port = |port: Option<DrapePort>| match port {
        Some(p) => (
            true,
            p.direction,
            p.x_axis.is_some(),
            p.x_axis.unwrap_or([0.0; 3]),
        ),
        None => (false, [0.0; 3], false, [0.0; 3]),
    };
    let (has_start_port, start_direction, has_start_x_axis, start_x_axis) = port(spec.start_port);
    let (has_end_port, end_direction, has_end_x_axis, end_x_axis) = port(spec.end_port);
    let input = ffi::DrapeNodesInput {
        points: flat(spec.points),
        contacts: spec
            .contacts
            .iter()
            .map(|c| contact_data(c.as_ref()))
            .collect(),
        keep: spec
            .keep
            .iter()
            .filter_map(|k| u32::try_from(*k).ok())
            .collect(),
        has_start_port,
        start_direction,
        has_start_x_axis,
        start_x_axis,
        has_end_port,
        end_direction,
        has_end_x_axis,
        end_x_axis,
        resting: spec.resting,
        half_width: spec.half_width,
        reach: spec.reach,
        bend_radius: spec.bend_radius.unwrap_or(0.0),
        bend_radius_edge: spec.bend_radius_edge.unwrap_or(0.0),
        max_twist: spec.max_twist.unwrap_or(0.0),
        tolerance: spec.tolerance,
    };
    let data = ffi::flex_drape_nodes(&obstacles, &input).map_err(failed)?;
    Ok(DrapeNodes {
        nodes: data.nodes.iter().map(|k| *k as usize).collect(),
        deviation: (data.deviation >= 0.0).then_some(data.deviation),
    })
}
