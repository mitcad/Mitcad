// SPDX-License-Identifier: MIT
//! Flex (mitcad#1713): the routes of flexible items through their nodes,
//! the solids of their sections that are not round, and the end ports of
//! pipes and Flex items (`geometry/include/mitcad/geometry/flex.hpp`; the
//! model's side is `mitcad_model::flex`).

#[cxx::bridge(namespace = "mitcad::bridge")]
pub mod ffi {
    /// A node of a route (`mitcad_model::flex::RouteNode`). `obstacle` is
    /// the index of its face's body in the obstacles, -1 for a node that
    /// lies on no face (`face` and `normal` are then unused); `direction`
    /// and `x_axis` only with their flags.
    struct FlexNodeInput {
        point: [f64; 3],
        obstacle: i32,
        face: String,
        normal: [f64; 3],
        has_direction: bool,
        direction: [f64; 3],
        has_x_axis: bool,
        x_axis: [f64; 3],
        twist: f64,
    }

    /// `mitcad_model::flex::FlexRouteSpec` without the obstacles. A bend
    /// radius or a twist limit that is not above zero is none.
    struct FlexRouteInput {
        nodes: Vec<FlexNodeInput>,
        resting: f64,
        half_width: f64,
        reach: f64,
        bend_radius: f64,
        bend_radius_edge: f64,
        max_twist: f64,
        /// A quicker, rougher route (the route alone under the pointer).
        quick: bool,
    }

    /// A piece of a route's centre line (`mitcad_model::Curve3`): `kind` 0
    /// a point, 1 a line, 2 a conic, 3 a B-spline; only the fields of its
    /// kind are used.
    struct FlexCurve {
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

    /// A point of a route (`mitcad_model::flex::RouteSample`): `node` and
    /// `obstacle` are -1 for none, `face` is the name of the face the
    /// route lies on in that obstacle.
    struct FlexSampleData {
        along: f64,
        point: [f64; 3],
        tangent: [f64; 3],
        normal: [f64; 3],
        node: i32,
        obstacle: i32,
        face: String,
    }

    /// A stretch of a route with something wrong
    /// (`mitcad_model::flex::RouteProblem`): `kind` 0 passes through, 1 a
    /// bend too tight, 2 a twist over the limit, 3 no clean route, 4 a bend
    /// over the edge too tight;
    /// `obstacle` -1 for none; `points` x0, y0, z0, x1, ... and `boxes`
    /// six numbers each (minimum, maximum) are its places.
    struct FlexProblemData {
        kind: u8,
        from: f64,
        to: f64,
        obstacle: i32,
        points: Vec<f64>,
        boxes: Vec<f64>,
    }

    /// `mitcad_model::flex::FlexRoute`.
    struct FlexRouteData {
        curve: Vec<FlexCurve>,
        samples: Vec<FlexSampleData>,
        problems: Vec<FlexProblemData>,
    }

    /// `mitcad_model::flex::FlexSolidSpec` without the path and an
    /// outline's region: a circle of the diameter `width` when `round`, a
    /// rectangle's sizes when `rectangular`; a `wall` that is not above
    /// zero is none.
    struct FlexSolidInput {
        samples: Vec<FlexSampleData>,
        round: bool,
        rectangular: bool,
        width: f64,
        height: f64,
        corner_radius: f64,
        wall: f64,
    }

    /// `mitcad_model::flex::EndPort`.
    struct FlexPortData {
        position: [f64; 3],
        direction: [f64; 3],
        x_axis: [f64; 3],
        size: [f64; 2],
    }

    unsafe extern "C++" {
        include!("bridge/flex.hpp");
        include!("bridge/trycatch.hpp");

        #[namespace = "mitcad::geometry"]
        type Shape = crate::kernel::shape::ffi::Shape;
        type ShapeList = crate::kernel::shape::ffi::ShapeList;
        type SweepCurve = crate::kernel::sweeps::ffi::SweepCurve;
        type Region = crate::kernel::profile::ffi::Region;

        /// The route of the item through the nodes, around the obstacles
        /// (`geometry::flex_route`).
        fn flex_route(
            feature: &str,
            obstacles: &ShapeList,
            input: &FlexRouteInput,
        ) -> Result<FlexRouteData>;

        /// The solid of a rectangle, or of the one region of `outline`,
        /// along `path` (`geometry::flex_solid`).
        fn flex_solid(
            feature: &str,
            path: &[SweepCurve],
            outline: &[Region],
            input: &FlexSolidInput,
        ) -> Result<SharedPtr<Shape>>;

        /// The end port at the cap face `cap` (`geometry::end_port`).
        fn flex_end_port(shape: &Shape, cap: &str) -> Result<FlexPortData>;
    }
}

use cxx::SharedPtr;
use mitcad_model::flex::{
    EndPort, FlexRoute, FlexRouteSpec, FlexSection, FlexSolidSpec, RouteProblem, RouteProblemKind,
    RouteSample,
};
use mitcad_model::notice::NoticePlace;
use mitcad_model::{Curve3, FaceName, KernelError, TopoName};

use super::{Shape, failed, occt, profile, shape_list, sweeps};

/// An index for the bridge; -1 for none.
fn index(value: Option<usize>) -> i32 {
    value.and_then(|v| i32::try_from(v).ok()).unwrap_or(-1)
}

fn curve(c: &ffi::FlexCurve) -> Curve3 {
    match c.kind {
        0 => Curve3::Point(c.start),
        1 => Curve3::Line {
            start: c.start,
            end: c.end,
        },
        2 => Curve3::Conic {
            center: c.center,
            normal: c.normal,
            x_axis: c.x_axis,
            major: c.major,
            minor: c.minor,
            start: c.first,
            end: c.last,
            closed: c.closed,
        },
        _ => Curve3::BSpline {
            degree: c.degree,
            poles: c.poles.chunks(3).map(|p| [p[0], p[1], p[2]]).collect(),
            weights: c.weights.clone(),
            knots: c.knots.clone(),
        },
    }
}

fn sample(s: &ffi::FlexSampleData) -> Result<RouteSample, KernelError> {
    let on = match usize::try_from(s.obstacle) {
        Ok(obstacle) if !s.face.is_empty() => match s.face.parse::<TopoName>() {
            Ok(TopoName::Face(face)) => Some((obstacle, face)),
            _ => {
                return Err(KernelError::failed(format!(
                    "the route names a face that is none: {}",
                    s.face
                )));
            }
        },
        _ => None,
    };
    Ok(RouteSample {
        along: s.along,
        point: s.point,
        tangent: s.tangent,
        normal: s.normal,
        node: usize::try_from(s.node).ok(),
        on,
    })
}

fn sample_data(s: &RouteSample) -> ffi::FlexSampleData {
    ffi::FlexSampleData {
        along: s.along,
        point: s.point,
        tangent: s.tangent,
        normal: s.normal,
        node: index(s.node),
        obstacle: index(s.on.as_ref().map(|(obstacle, _)| *obstacle)),
        face: s
            .on
            .as_ref()
            .map(|(_, face)| face.to_string())
            .unwrap_or_default(),
    }
}

fn problem(p: &ffi::FlexProblemData) -> RouteProblem {
    let mut places: Vec<NoticePlace> = p
        .points
        .as_chunks::<3>()
        .0
        .iter()
        .map(|at| NoticePlace::point(*at))
        .collect();
    places.extend(p.boxes.as_chunks::<6>().0.iter().map(|b| NoticePlace::Box {
        min: [b[0], b[1], b[2]],
        max: [b[3], b[4], b[5]],
    }));
    RouteProblem {
        kind: match p.kind {
            0 => RouteProblemKind::PassesThrough,
            1 => RouteProblemKind::BendTooTight,
            2 => RouteProblemKind::TwistOverLimit,
            4 => RouteProblemKind::EdgeBendTooTight,
            _ => RouteProblemKind::NoCleanRoute,
        },
        from: p.from,
        to: p.to,
        obstacle: usize::try_from(p.obstacle).ok(),
        places,
    }
}

/// `Kernel::flex_route`.
pub fn route(spec: &FlexRouteSpec<'_, SharedPtr<Shape>>) -> Result<FlexRoute, KernelError> {
    let obstacles = shape_list(spec.obstacles)?;
    let nodes = spec
        .nodes
        .iter()
        .map(|node| ffi::FlexNodeInput {
            point: node.point,
            obstacle: index(node.face.map(|face| face.obstacle)),
            face: node
                .face
                .map(|face| face.face.to_string())
                .unwrap_or_default(),
            normal: node.face.map_or([0.0; 3], |face| face.normal),
            has_direction: node.direction.is_some(),
            direction: node.direction.unwrap_or([0.0; 3]),
            has_x_axis: node.x_axis.is_some(),
            x_axis: node.x_axis.unwrap_or([0.0; 3]),
            twist: node.twist,
        })
        .collect();
    let input = ffi::FlexRouteInput {
        nodes,
        resting: spec.resting,
        half_width: spec.half_width,
        reach: spec.reach,
        bend_radius: spec.bend_radius.unwrap_or(0.0),
        bend_radius_edge: spec.bend_radius_edge.unwrap_or(0.0),
        max_twist: spec.max_twist.unwrap_or(0.0),
        quick: spec.quick,
    };
    let data = ffi::flex_route(&spec.feature.to_string(), &obstacles, &input).map_err(failed)?;
    Ok(FlexRoute {
        curve: data.curve.iter().map(curve).collect(),
        samples: data.samples.iter().map(sample).collect::<Result<_, _>>()?,
        problems: data.problems.iter().map(problem).collect(),
    })
}

/// `Kernel::flex_solid`.
pub fn solid(spec: &FlexSolidSpec<'_>) -> Result<SharedPtr<Shape>, KernelError> {
    let path: Vec<sweeps::ffi::SweepCurve> = spec.path.iter().map(sweeps::curve).collect();
    let (round, rectangular, sizes, outline) = match spec.section {
        FlexSection::Round { diameter } => (true, false, [*diameter, 0.0, 0.0], Vec::new()),
        FlexSection::Rectangular {
            width,
            height,
            corner_radius,
        } => (false, true, [*width, *height, *corner_radius], Vec::new()),
        FlexSection::Outline { loops } => {
            let loops = loops
                .iter()
                .enumerate()
                .map(|(i, segments)| profile::ffi::Loop {
                    segments: segments
                        .iter()
                        .enumerate()
                        .map(|(k, segment)| profile::segment(format!("s{i}_{k}"), segment))
                        .collect(),
                })
                .collect();
            let region = profile::ffi::Region {
                name: "outline".to_owned(),
                loops,
            };
            (false, false, [0.0; 3], vec![region])
        }
    };
    let input = ffi::FlexSolidInput {
        samples: spec.samples.iter().map(sample_data).collect(),
        round,
        rectangular,
        width: sizes[0],
        height: sizes[1],
        corner_radius: sizes[2],
        wall: spec.wall.unwrap_or(0.0),
    };
    ffi::flex_solid(&spec.feature.to_string(), &path, &outline, &input).map_err(failed)
}

/// `Kernel::end_port`.
pub fn end_port(shape: &SharedPtr<Shape>, cap: &FaceName) -> Result<EndPort, KernelError> {
    let port = ffi::flex_end_port(occt(shape)?, &cap.to_string()).map_err(failed)?;
    Ok(EndPort {
        position: port.position,
        direction: port.direction,
        x_axis: port.x_axis,
        size: port.size,
    })
}
