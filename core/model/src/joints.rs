// SPDX-License-Identifier: MIT
//! Joints between occurrences (mitcad#55, phases 1 and 2): the kinematic
//! vocabulary (joint kinds and their free motions), joint frames on
//! geometry of an occurrence's component, and what recompute does with the
//! joint features of `features/joint.rs`.
//!
//! A joint connects origin `a` to origin `b`. Each origin is geometry of an
//! occurrence's component (an occurrence path relative to the joint's own
//! component; the empty path is that component's own geometry), resolved
//! there to a frame and moved by the occurrences' placements into the
//! joint's component. The joint holds when
//!
//! `frame_a = frame_b · motion(values) · Rz(angle) · Tz(offset) · flip`
//!
//! where `motion` is the kind's free motions at some values, along and
//! about frame `b`'s axes (see [`JointKind::motions`],
//! [`motion_transform`]), and `flip` turns frame `a` over (half a turn
//! about its x axis): without it the frames' z axes point the same way.
//!
//! Each free motion is driven to a value (held there) when the joint has a
//! position for it, else its rest value; without either it is free. Free
//! motions stay within their limits.
//!
//! Recompute solves the joints with the joint solver
//! (`mitcad_solver::RigidSystem`): at a joint's point of the timeline, the
//! joints in effect in its component that share moving occurrences with
//! it (through each other) are solved together from where the occurrences
//! are, moving them as little as possible. A body of the solver is a unit:
//! a top-level occurrence of the component with those its rigid groups
//! join to it; grounded units and the component's own geometry stay. What
//! needs no iteration is placed first: a joint between a fixed side (or
//! one already joined to something fixed) and a free one moves the free
//! side, with what is joined to it so far, onto the other at the driven
//! values (else at the free values nearest to where it is); between two
//! free sides `a`'s side moves. Joints that close loops are iterated. The
//! occurrences that moved get placement changes of the joint feature
//! (`PlacementChange::Set`, applied as other features' are); their own
//! transforms stay the starting placements. Joints that contradict each
//! other, or cannot hold from where the occurrences are, fail the joint
//! feature with a message naming them.
//!
//! A path below a top-level occurrence only locates geometry: a joint
//! moves the top-level occurrence of its path (the whole sub-assembly),
//! never an occurrence inside it; joints inside a sub-assembly are the
//! sub-component's own features.
//!
//! Degrees of freedom are the rank of the joint equations where the
//! occurrences are ([`dof_report`]); [`drag`] pulls an occurrence toward a
//! point while the joints hold.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::Arc;

use mitcad_solver::{
    BodyId, ConstraintPart, Freedom, JointId, JointMotion, Pose, RigidJoint, RigidOptions,
    RigidResult, RigidSystem, SolveStatus,
};
use serde::{Deserialize, Serialize};

use crate::assembly::{Assembly, inverse};
use crate::constraints::{
    self, ConstraintSetResult, Geo, RowResult, RowValues, resolve_geo, row_spec,
};
use crate::datum::{Datum, Vec3};
use crate::document::DocState;
use crate::features::constraint_set::row_label;
use crate::features::geom_ref::{self, GeomRef, Resolver};
use crate::features::joint::{FrameOverride, JointOrigin};
use crate::features::{FeatureDef, FeatureEntry, PlacementChange, SketchOutput};
use crate::ids::{BodyUid, ComponentUid, FeatureUid, OccurrenceUid};
use crate::kernel::Kernel;
use crate::parameters::{ParamId, Parameters};
use crate::recompute::{BodyState, Recomputed, Versioned, place};
use crate::sketch::geometry::Curve2;
use crate::transform::Transform;
use mitcad_model_base::internal::datum::{canonical_axis, default_x_axis};
use mitcad_vec3::unit;
use mitcad_vec3::{cross, dot};

/// Distances below this hold a joint, mm.
const LINEAR: f64 = 1e-6;
/// Rotation matrix entries within this hold a joint.
const ANGULAR: f64 = 1e-7;

/// How a joint lets its two sides move against each other.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JointKind {
    /// No motion.
    Rigid,
    /// Turns about z.
    Revolute,
    /// Slides along x, y or z ([`SlideAxis`]).
    Slider,
    /// Turns about z and slides along it.
    Cylindrical,
    /// Turns about z and slides along x.
    PinSlot,
    /// Slides along x and y and turns about z.
    Planar,
    /// Turns about x, y and z.
    Ball,
}

impl JointKind {
    pub const ALL: [Self; 7] = [
        Self::Rigid,
        Self::Revolute,
        Self::Slider,
        Self::Cylindrical,
        Self::PinSlot,
        Self::Planar,
        Self::Ball,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Rigid => "rigid",
            Self::Revolute => "revolute",
            Self::Slider => "slider",
            Self::Cylindrical => "cylindrical",
            Self::PinSlot => "pin_slot",
            Self::Planar => "planar",
            Self::Ball => "ball",
        }
    }

    /// The free motions in the order their values compose
    /// ([`motion_transform`]).
    pub fn motions(self, slide: SlideAxis) -> Vec<Motion> {
        use Motion as M;
        match self {
            Self::Rigid => vec![],
            Self::Revolute => vec![M::Rz],
            Self::Slider => vec![slide.motion()],
            Self::Cylindrical => vec![M::Tz, M::Rz],
            Self::PinSlot => vec![M::Tx, M::Rz],
            Self::Planar => vec![M::Tx, M::Ty, M::Rz],
            Self::Ball => vec![M::Rz, M::Ry, M::Rx],
        }
    }

    /// The motions it takes away from the side it connects (6 less its
    /// free motions).
    pub fn constraints(self) -> u32 {
        6 - self.motions(SlideAxis::Z).len() as u32
    }
}

impl fmt::Display for JointKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One free motion of a joint, in the joint's frame: a slide along an axis
/// (mm) or a turn about it (radians, right-handed).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Motion {
    Tx,
    Ty,
    Tz,
    Rx,
    Ry,
    Rz,
}

impl Motion {
    pub const ALL: [Self; 6] = [Self::Tx, Self::Ty, Self::Tz, Self::Rx, Self::Ry, Self::Rz];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Tx => "tx",
            Self::Ty => "ty",
            Self::Tz => "tz",
            Self::Rx => "rx",
            Self::Ry => "ry",
            Self::Rz => "rz",
        }
    }

    pub fn is_rotation(self) -> bool {
        matches!(self, Self::Rx | Self::Ry | Self::Rz)
    }

    fn axis(self) -> Vec3 {
        match self {
            Self::Tx | Self::Rx => [1.0, 0.0, 0.0],
            Self::Ty | Self::Ry => [0.0, 1.0, 0.0],
            Self::Tz | Self::Rz => [0.0, 0.0, 1.0],
        }
    }

    /// The transform of this motion alone at `value`.
    fn transform(self, value: f64) -> Transform {
        if self.is_rotation() {
            Transform::rotation([0.0; 3], self.axis(), value).expect("unit axis")
        } else {
            Transform::translation(self.axis().map(|a| a * value))
        }
    }
}

impl fmt::Display for Motion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The axis a slider slides along, in the joint's frame.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SlideAxis {
    X,
    Y,
    #[default]
    Z,
}

impl SlideAxis {
    pub fn is_z(&self) -> bool {
        *self == Self::Z
    }

    pub fn motion(self) -> Motion {
        match self {
            Self::X => Motion::Tx,
            Self::Y => Motion::Ty,
            Self::Z => Motion::Tz,
        }
    }
}

// Frames.

/// A frame as the rigid transform from its own coordinates to the
/// coordinates it is given in: x, y = z × x and z as the columns.
#[doc(hidden)]
pub fn frame(origin: Vec3, x_axis: Vec3, z_axis: Vec3) -> Transform {
    let y_axis = cross(z_axis, x_axis);
    Transform {
        linear: std::array::from_fn(|r| [x_axis[r], y_axis[r], z_axis[r]]),
        translation: origin,
    }
}

/// A frame through `origin` with the unit z axis `z` and its default x
/// axis (the model X projected, Y when z is along X).
fn frame_along(origin: Vec3, z: Vec3) -> Transform {
    frame(origin, default_x_axis(z), z)
}

/// The columns of a frame: origin, x, y and z axes.
#[doc(hidden)]
pub fn frame_parts(t: &Transform) -> [Vec3; 4] {
    let column = |c: usize| -> Vec3 { std::array::from_fn(|r| t.linear[r][c]) };
    [t.translation, column(0), column(1), column(2)]
}

/// The transform of the motions at their values, composed in the kind's
/// order (`Tz · Rz` for a cylindrical joint, `Tx · Ty · Rz` for a planar
/// one, `Rz · Ry · Rx` for a ball); missing values are zero.
pub fn motion_transform(
    kind: JointKind,
    slide: SlideAxis,
    values: &BTreeMap<Motion, f64>,
) -> Transform {
    kind.motions(slide)
        .into_iter()
        .fold(Transform::IDENTITY, |t, m| {
            t.after(&m.transform(values.get(&m).copied().unwrap_or(0.0)))
        })
}

/// The fixed part of a joint between its frames: the angle about z, the
/// offset along z and the flip (half a turn about x).
#[doc(hidden)]
pub fn alignment(angle: f64, offset: f64, flip: bool) -> Transform {
    let turn = Motion::Rz.transform(angle);
    let lift = Motion::Tz.transform(offset);
    let over = if flip {
        Motion::Rx.transform(std::f64::consts::PI)
    } else {
        Transform::IDENTITY
    };
    turn.after(&lift).after(&over)
}

/// The values of the free motions that give `relative` (the motion part
/// of frame `a` in frame `b`: `frame_b⁻¹ · frame_a · alignment⁻¹`), or
/// None when the motions cannot give it: the joint does not hold.
pub fn motion_values(
    kind: JointKind,
    slide: SlideAxis,
    relative: &Transform,
) -> Option<BTreeMap<Motion, f64>> {
    let r = &relative.linear;
    let t = relative.translation;
    let scale = 1.0 + t.iter().fold(0.0_f64, |m, v| m.max(v.abs()));
    let zero = |v: f64| v.abs() <= LINEAR * scale;
    let no_turn =
        (0..3).all(|i| (0..3).all(|j| (r[i][j] - f64::from(u8::from(i == j))).abs() <= ANGULAR));
    // Turns about z only: z stays z.
    let about_z = r[0][2].abs() <= ANGULAR && r[1][2].abs() <= ANGULAR && r[2][2] > 0.0;
    let rz = || r[1][0].atan2(r[0][0]);
    let values: Vec<(Motion, f64)> = match kind {
        JointKind::Rigid => (no_turn && t.iter().all(|v| zero(*v))).then(Vec::new)?,
        JointKind::Revolute => {
            (about_z && t.iter().all(|v| zero(*v))).then(|| vec![(Motion::Rz, rz())])?
        }
        JointKind::Slider => {
            let along = slide.motion();
            let index = match slide {
                SlideAxis::X => 0,
                SlideAxis::Y => 1,
                SlideAxis::Z => 2,
            };
            let off = (0..3).filter(|i| *i != index).all(|i| zero(t[i]));
            (no_turn && off).then(|| vec![(along, t[index])])?
        }
        JointKind::Cylindrical => (about_z && zero(t[0]) && zero(t[1]))
            .then(|| vec![(Motion::Tz, t[2]), (Motion::Rz, rz())])?,
        JointKind::PinSlot => (about_z && zero(t[1]) && zero(t[2]))
            .then(|| vec![(Motion::Tx, t[0]), (Motion::Rz, rz())])?,
        JointKind::Planar => (about_z && zero(t[2]))
            .then(|| vec![(Motion::Tx, t[0]), (Motion::Ty, t[1]), (Motion::Rz, rz())])?,
        JointKind::Ball => t.iter().all(|v| zero(*v)).then(|| {
            // R = Rz(c) · Ry(b) · Rx(a).
            let ry = (-r[2][0]).clamp(-1.0, 1.0).asin();
            let (rx, rz) = if r[2][0].abs() < 1.0 - 1e-12 {
                (r[2][1].atan2(r[2][2]), r[1][0].atan2(r[0][0]))
            } else {
                // Gimbal lock: the x turn is taken by z.
                (0.0, (-r[0][1]).atan2(r[1][1]))
            };
            vec![(Motion::Rz, rz), (Motion::Ry, ry), (Motion::Rx, rx)]
        })?,
    };
    // Rounding noise of a joint at rest reads as zero.
    Some(
        values
            .into_iter()
            .map(|(m, v)| (m, if v.abs() < 1e-12 { 0.0 } else { v }))
            .collect(),
    )
}

/// A frame's override: given parts replace the resolved ones (in the
/// component's coordinates); the x axis is projected onto the plane across
/// z, or the default when it is along z.
#[doc(hidden)]
pub fn apply_override(frame_in: &Transform, o: &FrameOverride) -> Result<Transform, String> {
    let [origin, x, _, z] = frame_parts(frame_in);
    let origin = o.origin.unwrap_or(origin);
    let z = match o.z_axis {
        Some(z) => unit(z).ok_or("the frame's z axis is zero")?,
        None => z,
    };
    let hint = o.x_axis.unwrap_or(x);
    let projected = unit(std::array::from_fn(|i| hint[i] - z[i] * dot(z, hint)));
    let x = match (projected, o.x_axis) {
        (Some(x), _) => x,
        (None, Some(_)) => return Err("the frame's x axis must not be along its z axis".to_owned()),
        (None, None) => default_x_axis(z),
    };
    let turned = frame(origin, x, z);
    // The shift along the frame's own axes as they are now.
    Ok(match o.shift {
        Some(shift) => frame(turned.apply_point(shift), x, z),
        None => turned,
    })
}

/// The frame of a joint origin's geometry in its component's coordinates:
///
/// - a planar face: its middle, z its outward normal;
/// - a cylindrical, conical or toroidal face: its axis (z along the axis's
///   largest component), at the axis point nearest to the face's middle
///   or the torus' centre; a sphere: its centre, the component's axes;
/// - a circular edge: its centre and axis; a straight edge: its middle, z
///   along it;
/// - a vertex, a sketch point and any point: the point, the component's
///   axes; a sketch circle or arc: its centre, z the sketch's normal; a
///   sketch line: its start, z along it;
/// - a plane (origin, construction, joint origin, fixed): its frame; an
///   axis: its origin, z along it.
///
/// Other x axes are the default for z (the model X projected, Y when z is
/// along X).
#[doc(hidden)]
pub fn origin_frame(r: &mut impl Resolver, geometry: &GeomRef) -> Result<Transform, String> {
    let identity_at = Transform::translation;
    let datum_frame = |datum: Datum| match datum {
        Datum::Plane(p) => frame(p.origin, p.x_axis, p.normal()),
        Datum::Axis(a) => frame_along(a.origin, a.direction),
        Datum::Point(p) => identity_at(p.point),
    };
    match geometry {
        GeomRef::Origin(origin) => Ok(datum_frame(origin.datum())),
        GeomRef::Datum(uid) => Ok(datum_frame(r.datum(*uid)?)),
        GeomRef::Face { body, face } => {
            let shape = r.shape(*body)?;
            let face = &crate::features::renamed::face_for(r, &shape, face)?;
            let kernel = r.kernel();
            let surface = kernel.face_geometry(&shape, face).map_err(|e| {
                crate::notice::about_place(
                    crate::notice::NoticePlace::face(face.to_string()),
                    &e.to_string(),
                )
            })?;
            use crate::datum::SurfaceGeometry as S;
            match surface {
                S::Plane { origin, normal } => {
                    // The middle of the face, else the plane's origin.
                    let middle = kernel.face_plane(&shape, face).map_or(origin, |p| p.origin);
                    Ok(frame_along(middle, normal))
                }
                S::Cylinder { origin, axis, .. } | S::Cone { origin, axis, .. } => {
                    Ok(frame_along(origin, axis))
                }
                S::Torus { center, axis, .. } => Ok(frame_along(center, axis)),
                S::Sphere { center, .. } => Ok(identity_at(center)),
                S::Other { kind } => Err(format!(
                    "face {face} ({kind}) has no plane, axis or center for a joint"
                )),
            }
        }
        GeomRef::Edge { body, edge } => {
            let shape = r.shape(*body)?;
            let edge = &crate::features::renamed::edge_for(r, &shape, edge)?;
            use crate::datum::CurveGeometry as C;
            match r.kernel().edge_geometry(&shape, edge).map_err(|e| {
                crate::notice::about_place(
                    crate::notice::NoticePlace::edge(edge.to_string()),
                    &e.to_string(),
                )
            })? {
                C::Circle { center, normal, .. } => {
                    let axis = canonical_axis(unit(normal).ok_or("the circle has no normal")?);
                    Ok(frame_along(center, axis))
                }
                C::Line { start, end } => {
                    let along =
                        unit(std::array::from_fn(|i| end[i] - start[i])).ok_or_else(|| {
                            crate::notice::placed(
                                "the edge has no length",
                                crate::notice::NoticePlace::edge(edge.to_string()),
                                format!("edge {edge} has no length"),
                            )
                        })?;
                    Ok(frame_along(
                        std::array::from_fn(|i| (start[i] + end[i]) / 2.0),
                        along,
                    ))
                }
                C::Other { kind, .. } => Err(format!(
                    "edge {edge} ({kind}) is neither straight nor circular"
                )),
            }
        }
        GeomRef::SketchCurves { sketch, curves } if curves.len() == 1 => {
            let output = r.sketch(*sketch)?;
            let sketch_frame = output.frame;
            match output.solved.curves.get(&curves[0]) {
                Some(Curve2::Circle { center, .. } | Curve2::Arc { center, .. }) => Ok(frame(
                    sketch_frame.point(*center),
                    sketch_frame.x_axis,
                    sketch_frame.normal(),
                )),
                _ => {
                    let axis = geom_ref::resolve_axis(r, geometry)?;
                    Ok(frame_along(axis.origin, axis.direction))
                }
            }
        }
        GeomRef::FixedPlane { .. } => {
            let p = geom_ref::resolve_plane(r, geometry)?;
            Ok(frame(p.origin, p.x_axis, p.normal()))
        }
        GeomRef::FixedAxis { .. } => {
            let a = geom_ref::resolve_axis(r, geometry)?;
            Ok(frame_along(a.origin, a.direction))
        }
        GeomRef::Vertex { .. } | GeomRef::SketchPoint { .. } | GeomRef::FixedPoint(_) => {
            Ok(identity_at(geom_ref::resolve_point(r, geometry)?))
        }
        GeomRef::Mount { feature, name } => r.mount(*feature, name),
        GeomRef::Body(_) | GeomRef::SketchCurves { .. } => Err(format!(
            "{geometry} gives no frame for a joint (a face, an edge, a point, a plane or an axis)"
        )),
    }
}

// Occurrence paths.

/// The component an occurrence path from `component` ends in: the last
/// occurrence's, or `component` itself for the empty path. Fails for an
/// occurrence that does not exist or is not placed where the path says.
#[doc(hidden)]
pub fn path_component(
    assembly: &Assembly,
    component: ComponentUid,
    path: &[OccurrenceUid],
) -> Result<ComponentUid, String> {
    let mut parent = component;
    for o in path {
        let occurrence = assembly
            .occurrence(*o)
            .ok_or_else(|| format!("occurrence {o} does not exist"))?;
        if occurrence.parent != parent {
            return Err(format!(
                "{} is placed in {}, not in {}",
                assembly.occurrence_name(*o),
                assembly.name(occurrence.parent),
                assembly.name(parent)
            ));
        }
        parent = occurrence.component;
    }
    Ok(parent)
}

/// Finds an occurrence path from `component` by uids (`O1/O4`) or names
/// (`Arm:1/Pin:2`); the empty text is the component itself.
#[doc(hidden)]
pub fn find_path_from(
    assembly: &Assembly,
    component: ComponentUid,
    text: &str,
) -> Option<Vec<OccurrenceUid>> {
    let text = text.trim();
    if text.is_empty() {
        return Some(Vec::new());
    }
    let mut parent = component;
    let mut path = Vec::new();
    for part in text.split('/') {
        let o = assembly.children(parent).find(|o| {
            o.uid.to_string() == part.trim() || assembly.occurrence_name(o.uid) == part.trim()
        })?;
        path.push(o.uid);
        parent = o.component;
    }
    Some(path)
}

// Checks.

/// The joint features' check of components (instead of the general one in
/// `document.rs`, which keeps a feature to its own component's sketches and
/// construction geometry): each origin's sketches and construction
/// geometry must be in the component its occurrence path ends in. Paths
/// that do not lead from the feature's component (occurrences deleted, or
/// a copy whose occurrences are made after its features) are left to
/// recompute, which reports them; None for other features.
pub(crate) fn check_components(
    assembly: &Assembly,
    features: &[Arc<FeatureEntry>],
    entry: &FeatureEntry,
) -> Option<Result<(), String>> {
    let check_origin = |label: &str, origin: &JointOrigin| -> Result<(), String> {
        let Ok(component) = path_component(assembly, entry.component, &origin.occurrence.0) else {
            return Ok(());
        };
        let mut references = crate::features::References::default();
        origin.geometry.add_to(&mut references);
        for uid in references.features {
            let Some(other) = features.iter().find(|f| f.uid == uid) else {
                continue;
            };
            if placed_geometry(&other.def) && other.component != component {
                return Err(format!(
                    "{label}: {} is in {}, not in {}",
                    other.name,
                    assembly.name(other.component),
                    assembly.name(component)
                ));
            }
        }
        Ok(())
    };
    Some(match &entry.def {
        FeatureDef::Joint(joint) => {
            check_origin("a", &joint.a).and_then(|()| check_origin("b", &joint.b))
        }
        FeatureDef::AsBuiltJoint(joint) => match &joint.origin {
            Some(origin) => check_origin("origin", origin),
            None => Ok(()),
        },
        FeatureDef::RigidGroup(_) => Ok(()),
        FeatureDef::BoundLength(b) => {
            let [from, to] = b.origins();
            check_origin("from", &from).and_then(|()| check_origin("to", &to))
        }
        FeatureDef::ConstraintSet(set) => set.rows.iter().enumerate().try_for_each(|(i, row)| {
            row.origins().try_for_each(|(key, origin)| {
                check_origin(&format!("{}: {key}", row_label(i, row.kind)), origin)
            })
        }),
        // Motion links (mitcad#466): joints of their own component, and
        // origins as joints' are.
        FeatureDef::MotionLink(link) => [("a", link.a.joint), ("b", link.b.joint)]
            .into_iter()
            .try_for_each(|(key, uid)| match features.iter().find(|f| f.uid == uid) {
                Some(joint) if joint.component != entry.component => Err(format!(
                    "{key}: {} is in {}, not in {}",
                    joint.name,
                    assembly.name(joint.component),
                    assembly.name(entry.component)
                )),
                _ => Ok(()),
            })
            .and_then(|()| {
                link.coupling
                    .origins()
                    .into_iter()
                    .try_for_each(|origin| check_origin("coupling", origin))
            }),
        _ => return None,
    })
}

/// Sketches and construction geometry: they are in their component's
/// coordinates.
pub(crate) fn placed_geometry(def: &FeatureDef) -> bool {
    matches!(
        def,
        FeatureDef::Sketch(_)
            | FeatureDef::ConstructionPlane(_)
            | FeatureDef::ConstructionAxis(_)
            | FeatureDef::ConstructionPoint(_)
            | FeatureDef::JointOrigin(_)
            | FeatureDef::CoordinateSystem(_)
    )
}

/// The occurrences a joint feature names (in paths, as members).
pub(crate) fn occurrences_used(def: &FeatureDef) -> Vec<OccurrenceUid> {
    match def {
        FeatureDef::PatternAtPoints(p) => p
            .paths()
            .into_iter()
            .flat_map(|p| p.0.iter().copied())
            .collect(),
        // Not the scope's exclusions: they use nothing (mitcad#1236).
        FeatureDef::Extrude(e) => e.active_cut_scope().map_or_else(Vec::new, |scope| {
            scope
                .reference_paths()
                .into_iter()
                .flat_map(|path| path.0.iter().copied())
                .collect()
        }),
        FeatureDef::Joint(j) => [&j.a.occurrence.0[..], &j.b.occurrence.0[..]].concat(),
        FeatureDef::AsBuiltJoint(j) => {
            let mut used = [&j.a.0[..], &j.b.0[..]].concat();
            if let Some(origin) = &j.origin {
                used.extend(&origin.occurrence.0);
            }
            used
        }
        FeatureDef::RigidGroup(g) => g.occurrences.clone(),
        FeatureDef::ConstraintSet(set) => set
            .rows
            .iter()
            .flat_map(|row| row.origins().map(|(_, o)| o.occurrence.0.clone()))
            .flatten()
            .collect(),
        // BUILD's bound lengths (mitcad#375).
        FeatureDef::BoundLength(b) => [&b.from.occurrence.0[..], &b.to.occurrence.0[..]].concat(),
        // Motion links' picked geometry (mitcad#466).
        FeatureDef::MotionLink(link) => link
            .coupling
            .origins()
            .into_iter()
            .flat_map(|o| o.occurrence.0.iter().copied())
            .collect(),
        _ => Vec::new(),
    }
}

/// Explicit occurrence references, including geometry links and timeline moves.
#[doc(hidden)]
pub fn referenced_occurrences(def: &FeatureDef) -> Vec<OccurrenceUid> {
    let mut out = occurrences_used(def);
    // A move's points and axes picked in occurrences (mitcad#699).
    if let Some(spec) = def.move_spec() {
        for path in spec.occurrence_paths() {
            out.extend(&path.0);
        }
    }
    let links: Vec<&crate::links::OccurrenceLink> = match def {
        FeatureDef::Sketch(s) => s
            .plane_link
            .iter()
            .chain(s.projections.iter().filter_map(|p| p.link.as_ref()))
            .collect(),
        FeatureDef::Combine(c) => c.links().map(|(_, link)| link).collect(),
        FeatureDef::Sweep(s) => s.body_links.values().collect(),
        FeatureDef::SplitBody(s) => s.tool_links.values().collect(),
        FeatureDef::Extrude(x) => x.object_links.values().collect(),
        FeatureDef::Revolve(r) => r.object_links.values().collect(),
        FeatureDef::ConstructionPlane(p) => {
            p.links.values().chain(p.datum_links.values()).collect()
        }
        FeatureDef::ConstructionAxis(a) => a.links.values().chain(a.datum_links.values()).collect(),
        FeatureDef::ConstructionPoint(p) => {
            p.links.values().chain(p.datum_links.values()).collect()
        }
        FeatureDef::Derive(d) => d.links.values().collect(),
        FeatureDef::BodyTransfer(t) => vec![&t.link],
        // A pulley's or gear's seat on another component's shaft (mitcad#578).
        FeatureDef::Pulley(_) | FeatureDef::Gear(_) | FeatureDef::PrintableBearing(_) => {
            def.seat_link().map(|(_, link)| link).into_iter().collect()
        }
        FeatureDef::MoveOccurrence(m) => {
            out.extend(&m.occurrences);
            Vec::new()
        }
        FeatureDef::CapturePosition(c) => {
            out.extend(c.positions.iter().map(|p| p.occurrence));
            Vec::new()
        }
        // An Envelope's source (mitcad#1276), relative to its component.
        FeatureDef::Envelope(e) => {
            out.extend(&e.occurrence.0);
            Vec::new()
        }
        _ => Vec::new(),
    };
    for link in links {
        out.extend(&link.source.0);
        out.extend(&link.target.0);
    }
    out
}

pub(crate) fn check_generated_references(
    assembly: &Assembly,
    ctx: &crate::features::CheckContext<'_>,
    def: &FeatureDef,
) -> Result<(), String> {
    for uid in referenced_occurrences(def) {
        if let Some(instance) = assembly.occurrence(uid).and_then(|o| o.pattern.as_ref()) {
            ctx.feature(instance.feature)?;
        }
    }
    Ok(())
}

/// Renames the occurrences a joint or pattern feature names.
pub(crate) fn remap_occurrences(
    def: &mut FeatureDef,
    map: &dyn Fn(OccurrenceUid) -> OccurrenceUid,
) -> bool {
    let path = |p: &mut Vec<OccurrenceUid>| {
        for o in p.iter_mut() {
            *o = map(*o);
        }
    };
    match def {
        FeatureDef::PatternAtPoints(p) => {
            for point in p.targets.iter_mut().chain(p.anchor.iter_mut()) {
                if let Some(occurrence) = &mut point.occurrence {
                    path(&mut occurrence.0);
                }
            }
            if let Some(context) = &mut p.context {
                path(&mut context.0);
            }
            if let crate::features::PatternObjects::Components { occurrences } = &mut p.objects {
                for occurrence in occurrences {
                    path(&mut occurrence.0);
                }
            }
        }
        FeatureDef::Joint(j) => {
            path(&mut j.a.occurrence.0);
            path(&mut j.b.occurrence.0);
        }
        FeatureDef::AsBuiltJoint(j) => {
            path(&mut j.a.0);
            path(&mut j.b.0);
            if let Some(origin) = &mut j.origin {
                path(&mut origin.occurrence.0);
            }
        }
        FeatureDef::RigidGroup(g) => path(&mut g.occurrences),
        FeatureDef::BoundLength(b) => {
            path(&mut b.from.occurrence.0);
            path(&mut b.to.occurrence.0);
        }
        FeatureDef::ConstraintSet(set) => {
            for row in &mut set.rows {
                for origin in [&mut row.a, &mut row.b]
                    .into_iter()
                    .chain(row.a2.as_mut())
                    .chain(row.b2.as_mut())
                {
                    path(&mut origin.occurrence.0);
                }
            }
        }
        FeatureDef::MotionLink(link) => {
            for origin in link.coupling.origins_mut() {
                path(&mut origin.occurrence.0);
            }
        }
        // An Envelope's source (mitcad#1276).
        FeatureDef::Envelope(e) => path(&mut e.occurrence.0),
        _ => return false,
    }
    true
}

// Recompute.

/// What a joint feature did in the last recompute.
#[derive(Debug, Clone, PartialEq)]
pub enum JointState {
    /// The joint solver moved these occurrences so that the joints hold.
    Placed(Vec<OccurrenceUid>),
    /// The joint held where the occurrences were.
    Satisfied,
}

impl JointState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Placed(_) => "placed",
            Self::Satisfied => "satisfied",
        }
    }
}

/// A joint or as-built joint after recompute.
#[derive(Debug, Clone, PartialEq)]
pub struct JointResult {
    /// The joint feature's component, whose coordinates the frames are in.
    pub component: ComponentUid,
    pub kind: JointKind,
    pub slide: SlideAxis,
    /// The occurrence paths of the two sides.
    pub a: Vec<OccurrenceUid>,
    pub b: Vec<OccurrenceUid>,
    /// The joint's frames at the marker (an as-built joint's motion frame
    /// as it moves with each side).
    pub frame_a: Transform,
    pub frame_b: Transform,
    pub state: JointState,
    /// The free motions' values at the marker, None when the joint does not
    /// hold there (a later feature moved a side).
    pub values: Option<BTreeMap<Motion, f64>>,
    /// The motions with limits whose values are outside them.
    pub beyond_limits: Vec<Motion>,
    /// The placement changes the joint made: where its solve put the
    /// occurrences.
    pub placements: Vec<PlacementChange>,
}

/// One free motion of a joint in effect: its limits and the value it is
/// driven to (its position, else its rest value), at the joint's point of
/// the timeline.
#[derive(Debug, Clone, Copy, PartialEq)]
#[doc(hidden)]
pub struct MotionSpec {
    pub motion: Motion,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub target: Option<f64>,
    /// Where the free motion is when the joint places its moving side
    /// (the joint's `start`, mitcad#1745); None: nearest to where it is.
    pub start: Option<f64>,
}

/// A joint in effect, as the joint solver takes it: each side's frame is
/// `placement(path) · local`, with `local` in the coordinates of the
/// component the path ends in (resolved at the joint's point of the
/// timeline), and the joint holds when
/// `frame_a = frame_b · motion(values) · alignment`.
#[derive(Debug, Clone, PartialEq)]
#[doc(hidden)]
pub struct ActiveJoint {
    pub uid: FeatureUid,
    pub component: ComponentUid,
    pub kind: JointKind,
    pub slide: SlideAxis,
    pub a: Vec<OccurrenceUid>,
    pub b: Vec<OccurrenceUid>,
    pub local_a: Transform,
    pub local_b: Transform,
    pub alignment: Transform,
    pub motions: Vec<MotionSpec>,
    /// A row of a constraint set (mitcad#148): its index and equations
    /// (`motions` is then its value, `kind` and `alignment` unused).
    pub row: Option<usize>,
    pub parts: Option<Vec<ConstraintPart>>,
}

impl ActiveJoint {
    /// The two frames with these placements.
    pub(crate) fn frames(
        &self,
        placements: &BTreeMap<OccurrenceUid, Transform>,
    ) -> (Transform, Transform) {
        (
            path_placement(placements, &self.a).after(&self.local_a),
            path_placement(placements, &self.b).after(&self.local_b),
        )
    }

    /// A constraint row's: whether it holds with these placements, and its
    /// value there.
    pub(crate) fn holds(
        &self,
        placements: &BTreeMap<OccurrenceUid, Transform>,
    ) -> (bool, Option<f64>) {
        let (fa, fb) = self.frames(placements);
        let parts = self.parts.as_deref().unwrap_or_default();
        constraints::check(&fa, &fb, parts, self.motions.first())
    }

    /// The free motions' values with these placements (turns unwrapped
    /// near their targets or limits), None when the joint does not hold.
    pub(crate) fn values(
        &self,
        placements: &BTreeMap<OccurrenceUid, Transform>,
    ) -> Option<BTreeMap<Motion, f64>> {
        let (fa, fb) = self.frames(placements);
        let relative = inverse(&fb).after(&fa).after(&inverse(&self.alignment));
        let mut values = motion_values(self.kind, self.slide, &relative)?;
        for spec in &self.motions {
            let Some(v) = values.get_mut(&spec.motion) else {
                continue;
            };
            if !spec.motion.is_rotation() {
                continue;
            }
            let reference = match (spec.target, spec.min, spec.max) {
                (Some(t), _, _) => t,
                (None, Some(lo), Some(hi)) => 0.5 * (lo + hi),
                (None, Some(lo), None) => lo,
                (None, None, Some(hi)) => hi,
                (None, None, None) => continue,
            };
            let tau = std::f64::consts::TAU;
            *v += ((reference - *v) / tau).round() * tau;
        }
        Some(values)
    }
}

/// The joints' part of a recompute's results.
#[derive(Debug, Clone, Default, PartialEq)]
#[doc(hidden)]
pub struct JointsState {
    pub joints: BTreeMap<FeatureUid, JointResult>,
    /// Rigid groups in effect, in timeline order: the feature, its
    /// component and the occurrences.
    pub groups: Vec<(FeatureUid, ComponentUid, Vec<OccurrenceUid>)>,
    /// Joints and constraint rows in effect, in timeline order.
    pub active: Vec<ActiveJoint>,
    /// Constraint sets (mitcad#148).
    pub constraints: BTreeMap<FeatureUid, ConstraintSetResult>,
    /// Motion links in effect, in timeline order, and what each did
    /// (mitcad#466).
    pub links: Vec<crate::motion_links::ActiveLink>,
    pub link_results: BTreeMap<FeatureUid, crate::motion_links::LinkResult>,
}

/// Whether recompute applies the feature with [`apply`].
pub(crate) fn applies(def: &FeatureDef) -> bool {
    matches!(
        def,
        FeatureDef::Joint(_)
            | FeatureDef::AsBuiltJoint(_)
            | FeatureDef::RigidGroup(_)
            | FeatureDef::ConstraintSet(_)
            | FeatureDef::MotionLink(_)
    )
}

/// Resolves references in one component at a point of a recompute.
pub(crate) struct StepResolver<'a, K: Kernel> {
    pub kernel: &'a K,
    pub assembly: &'a Assembly,
    pub component: ComponentUid,
    pub bodies: Option<&'a BodyState<K::Shape>>,
    pub datums: &'a BTreeMap<FeatureUid, Versioned<Datum>>,
    pub sketches: &'a BTreeMap<FeatureUid, Versioned<Arc<SketchOutput>>>,
    pub features: &'a [Arc<FeatureEntry>],
    /// The results so far, for derived parts' mount frames (mitcad#375).
    pub history: &'a [crate::recompute::FeatureResult<K::Shape>],
    /// Warnings of names found after a sketch change (mitcad#272).
    pub renamed: Vec<String>,
}

impl<K: Kernel> Resolver for StepResolver<'_, K> {
    type Kernel = K;

    fn kernel(&self) -> &K {
        self.kernel
    }

    fn name(&self, uid: FeatureUid) -> String {
        self.features
            .iter()
            .find(|f| f.uid == uid)
            .map_or_else(|| uid.to_string(), |f| f.name.clone())
    }

    fn datum(&mut self, uid: FeatureUid) -> Result<Datum, String> {
        self.datums
            .get(&uid)
            .map(|d| d.value)
            .ok_or_else(|| format!("{} has no result", self.name(uid)))
    }

    fn shape(&mut self, body: BodyUid) -> Result<K::Shape, String> {
        self.bodies
            .and_then(|b| b.get(&body))
            .map(|b| b.value.clone())
            .ok_or_else(|| {
                format!(
                    "body {body} is not in {} at this point of the timeline",
                    self.assembly.name(self.component)
                )
            })
    }

    fn sketch(&mut self, uid: FeatureUid) -> Result<Arc<SketchOutput>, String> {
        self.sketches
            .get(&uid)
            .map(|s| s.value.clone())
            .ok_or_else(|| format!("{} has no result", self.name(uid)))
    }

    fn renamed(&mut self, what: &str, stored: &str, found: &str, maker: FeatureUid) {
        let message =
            crate::features::renamed::warning(self.features, what, &stored, &found, maker);
        if !self.renamed.contains(&message) {
            self.renamed.push(message);
        }
    }

    fn mount(&mut self, feature: FeatureUid, name: &str) -> Result<Transform, String> {
        let label = self.name(feature);
        crate::features::derived_part::mount_frame(self.history, feature, name, &label)
    }
}

// Joints to a linked design shown by its envelope (mitcad#1276).

/// How an origin on a linked design's face, edge or vertex resolves while
/// the design is shown by its envelope: the component that shows it
/// holds the envelope instead of the design's bodies.
pub(crate) enum EnvelopeSide {
    /// The frame it had when the design was last shown in full
    /// (`ExternalLink::frames`).
    Kept(Transform),
    /// In this component instead: another component linking the same
    /// file, whose bodies the origin names, shows the design in full (an
    /// occurrence shown by its envelope on its own is placed by a
    /// component of its own, `document/envelope.rs`).
    Full(ComponentUid),
    /// It cannot be resolved: why.
    Missing(String),
}

/// The body whose face, edge or vertex the geometry is.
fn geometry_body(geometry: &GeomRef) -> Option<BodyUid> {
    match geometry {
        GeomRef::Face { body, .. } | GeomRef::Edge { body, .. } | GeomRef::Vertex { body, .. } => {
            Some(*body)
        }
        _ => None,
    }
}

/// See [`EnvelopeSide`]: None when the geometry resolves in `at` as
/// usual (also an origin picked on the envelope itself, which has no kept
/// frame).
pub(crate) fn envelope_side(
    state: &DocState,
    at: ComponentUid,
    geometry: &GeomRef,
) -> Option<EnvelopeSide> {
    let body = geometry_body(geometry)?;
    let owner = state.entry(body.feature)?.component;
    let assembly = &state.assembly;
    let here = assembly.component(at)?.link.as_ref()?;
    let link = assembly.component(owner)?.link.as_ref()?;
    if link.feature != body.feature || here.path != link.path {
        return None;
    }
    if link.envelope.is_none() {
        return (owner != at).then_some(EnvelopeSide::Full(owner));
    }
    match link.frames.iter().find(|f| f.geometry == *geometry) {
        Some(kept) => Some(EnvelopeSide::Kept(kept.frame)),
        None if owner == at => None,
        None => Some(EnvelopeSide::Missing(format!(
            "{geometry} is on {}, which is shown by its envelope: show it in full to join to \
             its faces",
            assembly.name(owner)
        ))),
    }
}

/// The frames of the joint origins on `component`'s bodies (a linked
/// design about to be shown by its envelope) where the last recompute
/// `result` has them, for [`EnvelopeSide::Kept`]; origins that do not
/// resolve there are left out.
pub(crate) fn envelope_frames<K: Kernel>(
    kernel: &K,
    state: &DocState,
    result: &Recomputed<K::Shape>,
    component: ComponentUid,
) -> Vec<crate::assembly::EnvelopeFrame> {
    let mut frames: Vec<crate::assembly::EnvelopeFrame> = Vec::new();
    for entry in &state.features {
        let origins: Vec<&JointOrigin> = match &entry.def {
            FeatureDef::Joint(j) => vec![&j.a, &j.b],
            FeatureDef::AsBuiltJoint(j) => j.origin.iter().collect(),
            _ => continue,
        };
        for origin in origins {
            let Some(body) = geometry_body(&origin.geometry) else {
                continue;
            };
            if state.entry(body.feature).map(|f| f.component) != Some(component)
                || frames.iter().any(|f| f.geometry == origin.geometry)
            {
                continue;
            }
            let mut resolver = StepResolver {
                kernel,
                assembly: &state.assembly,
                component,
                bodies: Some(&*result.bodies),
                datums: &result.datums,
                sketches: &result.sketches,
                features: &state.features,
                history: &result.results,
                renamed: Vec::new(),
            };
            if let Ok(frame) = origin_frame(&mut resolver, &origin.geometry) {
                frames.push(crate::assembly::EnvelopeFrame {
                    geometry: origin.geometry.clone(),
                    frame,
                });
            }
        }
    }
    frames
}

/// The value of an optional parameter, zero without one.
fn value_or_zero(params: &Parameters, id: Option<&ParamId>) -> Result<f64, String> {
    match id {
        None => Ok(0.0),
        Some(id) => params
            .value(*id)
            .ok_or_else(|| format!("parameter {} does not exist", params.name(*id))),
    }
}

/// The placement of an occurrence path: the product of its occurrences'.
pub(crate) fn path_placement(
    placements: &BTreeMap<OccurrenceUid, Transform>,
    path: &[OccurrenceUid],
) -> Transform {
    path.iter().fold(Transform::IDENTITY, |t, o| {
        t.after(&placements.get(o).copied().unwrap_or(Transform::IDENTITY))
    })
}

/// A joint's free motions with their limits and driven values: the
/// position, else the rest value.
fn motion_specs(
    params: &Parameters,
    kind: JointKind,
    slide: SlideAxis,
    limits: &crate::features::Limits<ParamId>,
    position: &crate::features::JointPosition<ParamId>,
    start: &BTreeMap<Motion, f64>,
) -> Result<Vec<MotionSpec>, String> {
    let value = |id: Option<&ParamId>| id.map(|id| value_or_zero(params, Some(id))).transpose();
    kind.motions(slide)
        .into_iter()
        .map(|motion| {
            let limit = limits.get(&motion);
            let rest = value(limit.and_then(|l| l.rest.as_ref()))?;
            Ok(MotionSpec {
                motion,
                min: value(limit.and_then(|l| l.min.as_ref()))?,
                max: value(limit.and_then(|l| l.max.as_ref()))?,
                target: value(position.get(&motion))?.or(rest),
                start: start.get(&motion).copied(),
            })
        })
        .collect()
}

/// Applies a joint, an as-built joint or a rigid group at its point of the
/// recompute (after its evaluation, which checks its values): records the
/// group, or resolves the joint's frames and solves it with the joints in
/// effect before it (see the module documentation). Placements are not
/// cached: they depend on the assembly's flags and the placements before
/// the feature.
pub(crate) fn apply<K: Kernel>(
    kernel: &K,
    state: &DocState,
    entry: &FeatureEntry,
    components: &BTreeMap<ComponentUid, Arc<BodyState<K::Shape>>>,
    done: &mut Recomputed<K::Shape>,
) -> Result<Vec<String>, String> {
    let assembly = &state.assembly;
    let params = &state.parameters;
    let component = entry.component;
    for uid in occurrences_used(&entry.def) {
        if assembly
            .occurrence(uid)
            .is_some_and(|o| o.pattern.is_some())
            && !done.placements.contains_key(&uid)
        {
            return Err(format!(
                "{} is unavailable: its Pattern at Points feature has no active result",
                assembly.occurrence_name(uid)
            ));
        }
    }
    if let FeatureDef::MotionLink(link) = &entry.def {
        return crate::motion_links::apply(kernel, state, entry, link, components, done);
    }
    if let FeatureDef::RigidGroup(group) = &entry.def {
        for o in &group.occurrences {
            path_component(assembly, component, &[*o])?;
        }
        done.joints
            .groups
            .push((entry.uid, component, group.occurrences.clone()));
        return Ok(Vec::new());
    }
    // What the geometry found under other names after a sketch change
    // (mitcad#272): the feature's warnings.
    let renamed = std::cell::RefCell::new(Vec::<String>::new());
    // An origin's frame in the component its path ends in; on a linked
    // design shown by its envelope, as it is in full (mitcad#1276).
    let local = |origin: &JointOrigin| -> Result<Transform, String> {
        let mut at = path_component(assembly, component, &origin.occurrence.0)?;
        let mut kept = None;
        match envelope_side(state, at, &origin.geometry) {
            None => {}
            Some(EnvelopeSide::Kept(frame)) => kept = Some(frame),
            Some(EnvelopeSide::Full(owner)) => at = owner,
            Some(EnvelopeSide::Missing(why)) => return Err(why),
        }
        let mut local = match kept {
            Some(frame) => frame,
            None => {
                let mut resolver = StepResolver {
                    kernel,
                    assembly,
                    component: at,
                    bodies: components.get(&at).map(|b| &**b),
                    datums: &done.datums,
                    sketches: &done.sketches,
                    features: &state.features,
                    history: &done.results,
                    renamed: Vec::new(),
                };
                let frame = origin_frame(&mut resolver, &origin.geometry);
                renamed.borrow_mut().append(&mut resolver.renamed);
                frame?
            }
        };
        if let Some(o) = &origin.frame_override {
            local = apply_override(&local, o)?;
        }
        Ok(local)
    };
    if let FeatureDef::ConstraintSet(set) = &entry.def {
        // Each row's references in the components their paths end in.
        let mut rows = Vec::with_capacity(set.rows.len());
        for (i, row) in set.rows.iter().enumerate() {
            let label = row_label(i, row.kind);
            let geo = |key: &str, origin: &JointOrigin| -> Result<Geo, String> {
                let mut at = path_component(assembly, component, &origin.occurrence.0)
                    .map_err(|e| format!("{label}: {key}: {e}"))?;
                // A linked design's geometry where it is shown in full
                // (mitcad#1276).
                match envelope_side(state, at, &origin.geometry) {
                    Some(EnvelopeSide::Full(owner)) => at = owner,
                    Some(EnvelopeSide::Missing(why)) => {
                        return Err(format!("{label}: {key}: {why}"));
                    }
                    _ => {}
                }
                let mut resolver = StepResolver {
                    kernel,
                    assembly,
                    component: at,
                    bodies: components.get(&at).map(|b| &**b),
                    datums: &done.datums,
                    sketches: &done.sketches,
                    features: &state.features,
                    history: &done.results,
                    renamed: Vec::new(),
                };
                let resolved = resolve_geo(&mut resolver, origin);
                renamed.borrow_mut().append(&mut resolver.renamed);
                resolved.map_err(|e| format!("{label}: {key}: {e}"))
            };
            let a = geo("a", &row.a)?;
            let a2 = row.a2.as_ref().map(|o| geo("a2", o)).transpose()?;
            let b = geo("b", &row.b)?;
            let b2 = row.b2.as_ref().map(|o| geo("b2", o)).transpose()?;
            let values = RowValues::of(row, params).map_err(|e| format!("{label}: {e}"))?;
            let spec = row_spec(row.kind, &a, a2.as_ref(), &b, b2.as_ref(), &values)
                .map_err(|e| format!("{label}: {e}"))?;
            let (pa, pb) = (&row.a.occurrence.0, &row.b.occurrence.0);
            let (a_path, b_path) = if spec.swap { (pb, pa) } else { (pa, pb) };
            rows.push(ActiveJoint {
                uid: entry.uid,
                component,
                kind: JointKind::Rigid,
                slide: SlideAxis::Z,
                a: a_path.clone(),
                b: b_path.clone(),
                local_a: spec.local_a,
                local_b: spec.local_b,
                alignment: Transform::IDENTITY,
                motions: spec.value.into_iter().collect(),
                row: Some(i),
                parts: Some(spec.parts),
            });
        }
        let (state_now, changes) = solve_new(state, entry, &rows, &[], done)?;
        let results = set
            .rows
            .iter()
            .zip(&rows)
            .map(|(row, active)| {
                let (holds, value) = active.holds(&done.placements);
                RowResult {
                    kind: row.kind,
                    a: row.a.occurrence.0.clone(),
                    b: row.b.occurrence.0.clone(),
                    holds,
                    value: if active.motions.is_empty() {
                        None
                    } else {
                        value
                    },
                }
            })
            .collect();
        done.joints.active.extend(rows);
        done.joints.constraints.insert(
            entry.uid,
            ConstraintSetResult {
                component,
                state: state_now,
                rows: results,
                placements: changes,
            },
        );
        return Ok(renamed.take());
    }

    let no_start = BTreeMap::new();
    let (kind, slide, limits, position, start, a_path, b_path) = match &entry.def {
        FeatureDef::Joint(j) => (
            j.kind,
            j.slide_axis,
            &j.limits,
            &j.position,
            &j.start,
            &j.a.occurrence.0,
            &j.b.occurrence.0,
        ),
        FeatureDef::AsBuiltJoint(j) => (
            j.kind,
            j.slide_axis,
            &j.limits,
            &j.position,
            &no_start,
            &j.a.0,
            &j.b.0,
        ),
        _ => return Ok(renamed.take()),
    };
    path_component(assembly, component, a_path).map_err(|e| format!("a: {e}"))?;
    path_component(assembly, component, b_path).map_err(|e| format!("b: {e}"))?;
    let motions = motion_specs(params, kind, slide, limits, position, start)?;

    let (local_a, local_b, alignment) = match &entry.def {
        FeatureDef::Joint(j) => (
            local(&j.a).map_err(|e| format!("a: {e}"))?,
            local(&j.b).map_err(|e| format!("b: {e}"))?,
            self::alignment(
                value_or_zero(params, j.angle.as_ref())?,
                value_or_zero(params, j.offset.as_ref())?,
                j.flip,
            ),
        ),
        FeatureDef::AsBuiltJoint(j) => {
            // The sides' own frames; the relation is the recorded one.
            let pa = path_placement(&done.placements, a_path);
            let pb = path_placement(&done.placements, b_path);
            let relative = j.relative.unwrap_or_else(|| inverse(&pb).after(&pa));
            // The motion frame in b's coordinates, with a where the
            // recorded relation puts it: an origin that moves with a is
            // taken there.
            let motion_frame = match &j.origin {
                None => Transform::IDENTITY,
                Some(origin) => {
                    let world = path_placement(&done.placements, &origin.occurrence.0)
                        .after(&local(origin).map_err(|e| format!("origin: {e}"))?);
                    let groups = &done.joints.groups;
                    let side = |path: &[OccurrenceUid]| {
                        path.first().map(|o| group_of(groups, component, *o))
                    };
                    if side(&origin.occurrence.0) == side(a_path) {
                        relative.after(&inverse(&pa)).after(&world)
                    } else {
                        inverse(&pb).after(&world)
                    }
                }
            };
            (
                inverse(&relative).after(&motion_frame),
                motion_frame,
                Transform::IDENTITY,
            )
        }
        _ => unreachable!(),
    };
    let joint = ActiveJoint {
        uid: entry.uid,
        component,
        kind,
        slide,
        a: a_path.clone(),
        b: b_path.clone(),
        local_a,
        local_b,
        alignment,
        motions,
        row: None,
        parts: None,
    };
    let (state_now, changes) = solve_new(state, entry, std::slice::from_ref(&joint), &[], done)?;
    if let JointState::Placed(moved) = &state_now {
        renamed
            .borrow_mut()
            .extend(placed_again(state, entry, moved));
    }
    let (frame_a, frame_b) = joint.frames(&done.placements);
    let values = joint.values(&done.placements);
    done.joints.active.push(joint);
    done.joints.joints.insert(
        entry.uid,
        JointResult {
            component,
            kind,
            slide,
            a: a_path.clone(),
            b: b_path.clone(),
            frame_a,
            frame_b,
            state: state_now,
            values,
            beyond_limits: Vec::new(),
            placements: changes,
        },
    );
    Ok(renamed.take())
}

/// After recompute: each joint's frames and values where the occurrences
/// are at the marker.
pub(crate) fn finish(joints: &mut JointsState, placements: &BTreeMap<OccurrenceUid, Transform>) {
    for joint in &joints.active {
        if let Some(i) = joint.row {
            // A constraint row: whether it holds there, and its value.
            if let Some(row) = joints
                .constraints
                .get_mut(&joint.uid)
                .and_then(|set| set.rows.get_mut(i))
            {
                let (holds, value) = joint.holds(placements);
                row.holds = holds;
                if !joint.motions.is_empty() {
                    row.value = value;
                }
            }
            continue;
        }
        let Some(result) = joints.joints.get_mut(&joint.uid) else {
            continue;
        };
        (result.frame_a, result.frame_b) = joint.frames(placements);
        result.values = joint.values(placements);
        result.beyond_limits = match &result.values {
            None => Vec::new(),
            Some(values) => joint
                .motions
                .iter()
                .filter(|s| {
                    let v = values.get(&s.motion).copied().unwrap_or(0.0);
                    s.min.is_some_and(|min| v < min - 1e-9)
                        || s.max.is_some_and(|max| v > max + 1e-9)
                })
                .map(|s| s.motion)
                .collect(),
        };
    }
    crate::motion_links::finish(joints);
}

/// The name of a joint, or of a constraint set's row
/// (`Constraint1 row 2 (insert)`), in messages.
pub(crate) fn relation_name(state: &DocState, uid: FeatureUid, row: Option<usize>) -> String {
    let Some(entry) = state.features.iter().find(|f| f.uid == uid) else {
        return uid.to_string();
    };
    match (row, &entry.def) {
        (Some(i), FeatureDef::ConstraintSet(set)) => match set.rows.get(i) {
            Some(r) => format!("{} {}", entry.name, row_label(i, r.kind)),
            None => entry.name.clone(),
        },
        _ => entry.name.clone(),
    }
}

/// Solves new joints or constraint rows (a feature's) with the joints and
/// constraints in effect that share moving occurrences with them (through
/// each other) and places the occurrences that move, as placement changes
/// of the feature applied like other features' (`recompute::place`).
/// New motion links (mitcad#466) bring the joints they couple, and links
/// bring the joints they couple to those taken.
pub(crate) fn solve_new<S>(
    state: &DocState,
    entry: &FeatureEntry,
    new: &[ActiveJoint],
    new_links: &[crate::motion_links::ActiveLink],
    done: &mut Recomputed<S>,
) -> Result<(JointState, Vec<PlacementChange>), String> {
    let assembly = &state.assembly;
    let component = entry.component;
    let groups = &done.joints.groups;
    let unit = |path: &[OccurrenceUid]| path.first().map(|o| group_of(groups, component, *o));
    let fixed = |u: &Option<BTreeSet<OccurrenceUid>>| {
        u.as_ref().is_none_or(|members| {
            members
                .iter()
                .any(|o| assembly.occurrence(*o).is_some_and(|o| o.grounded))
        })
    };
    let mut moving: Vec<BTreeSet<OccurrenceUid>> = Vec::new();
    let mut solving: Vec<&ActiveJoint> = Vec::new();
    for joint in new {
        let (ua, ub) = (unit(&joint.a), unit(&joint.b));
        // Nothing the solver can move: it holds or fails, saying in words
        // what holds each side, with its places (mitcad#1745).
        let still = match (&ua, &ub) {
            (Some(x), Some(y)) if x == y => Some(one_unit(assembly, component, &joint.a, x)),
            _ => match (
                held_side(state, &done.joints, component, &joint.a, joint.uid),
                held_side(state, &done.joints, component, &joint.b, joint.uid),
            ) {
                (Some((why_a, mut places)), Some((why_b, more))) => {
                    places.extend(more);
                    Some((
                        format!("neither side can move ({why_a}, and {why_b})"),
                        places,
                    ))
                }
                _ => None,
            },
        };
        if let Some((why, places)) = still {
            check_still(state, joint, &why, &done.placements).map_err(|e| {
                crate::notice::FeatureError::from_text(&e)
                    .with_places(places)
                    .text()
            })?;
            continue;
        }
        for u in [ua, ub].into_iter().filter(|u| !fixed(u)).flatten() {
            if !moving.contains(&u) {
                moving.push(u);
            }
        }
        solving.push(joint);
    }
    if solving.is_empty() && new_links.is_empty() {
        return Ok((JointState::Satisfied, Vec::new()));
    }

    // The joints and constraints in effect that share moving units with
    // these, through each other and through motion links.
    let mine: Vec<&ActiveJoint> = done
        .joints
        .active
        .iter()
        .filter(|j| j.component == component)
        .chain(solving)
        .collect();
    let links: Vec<&crate::motion_links::ActiveLink> = done
        .joints
        .links
        .iter()
        .filter(|l| l.component == component)
        .chain(new_links)
        .collect();
    let first_new_link = links.len() - new_links.len();
    let mut taken = vec![false; mine.len()];
    let mut taken_links = vec![false; links.len()];
    // Joints a taken link couples.
    let mut linked: BTreeSet<FeatureUid> = BTreeSet::new();
    loop {
        let mut grown = false;
        for (k, l) in links.iter().enumerate() {
            let joint_taken = |uid: FeatureUid| {
                mine.iter()
                    .zip(&taken)
                    .any(|(j, t)| *t && j.row.is_none() && j.uid == uid)
            };
            if taken_links[k] || !(k >= first_new_link || joint_taken(l.a.0) || joint_taken(l.b.0))
            {
                continue;
            }
            taken_links[k] = true;
            linked.extend([l.a.0, l.b.0]);
            grown = true;
        }
        for (i, j) in mine.iter().enumerate() {
            if taken[i] {
                continue;
            }
            let sides = [unit(&j.a), unit(&j.b)];
            let coupled = j.row.is_none() && linked.contains(&j.uid);
            if !coupled && !sides.iter().flatten().any(|u| moving.contains(u)) {
                continue;
            }
            taken[i] = true;
            grown = true;
            for side in sides {
                if !fixed(&side)
                    && let Some(u) = side
                    && !moving.contains(&u)
                {
                    moving.push(u);
                }
            }
        }
        if !grown {
            break;
        }
    }
    let cluster: Vec<&ActiveJoint> = mine
        .iter()
        .zip(&taken)
        .filter(|(_, t)| **t)
        .map(|(j, _)| *j)
        .collect();
    let cluster_links: Vec<&crate::motion_links::ActiveLink> = links
        .iter()
        .zip(&taken_links)
        .filter(|(_, t)| **t)
        .map(|(l, _)| *l)
        .collect();
    let mut mechanism = Mechanism::new(
        assembly,
        groups,
        component,
        &cluster,
        &cluster_links,
        &done.placements,
        false,
    )?;
    // A new link's joints must both move in the solve.
    for l in new_links {
        for uid in [l.a.0, l.b.0] {
            if mechanism.joint_of(uid).is_none() {
                return Err(format!(
                    "{} cannot move: both its sides move together",
                    relation_name(state, uid, None)
                ));
            }
        }
    }
    let result = mechanism.system.solve(&RigidOptions::default());
    if !result.is_ok() {
        return Err(solve_error(state, &mechanism, &result));
    }
    let changes = mechanism.changes(&done.placements);
    let moved: Vec<OccurrenceUid> = changes
        .iter()
        .map(|c| match c {
            PlacementChange::Move(o, _) | PlacementChange::Set(o, _) => *o,
        })
        .collect();
    place(state, entry, &changes, &mut done.placements)?;
    let joint_state = if moved.is_empty() {
        JointState::Satisfied
    } else {
        JointState::Placed(moved)
    };
    Ok((joint_state, changes))
}

/// A joint or constraint row whose sides cannot move: it holds (a joint
/// at its driven values) or fails with `why`.
fn check_still(
    state: &DocState,
    joint: &ActiveJoint,
    why: &str,
    placements: &BTreeMap<OccurrenceUid, Transform>,
) -> Result<(), String> {
    if let Some(i) = joint.row {
        if joint.holds(placements).0 {
            return Ok(());
        }
        let label = state
            .features
            .iter()
            .find(|f| f.uid == joint.uid)
            .and_then(|f| match &f.def {
                FeatureDef::ConstraintSet(set) => set.rows.get(i).map(|r| row_label(i, r.kind)),
                _ => None,
            })
            .unwrap_or_else(|| format!("row {}", i + 1));
        return Err(format!("{label}: {why} and the constraint does not hold"));
    }
    let Some(values) = joint.values(placements) else {
        return Err(format!("{why} and the joint does not hold"));
    };
    for spec in &joint.motions {
        if let (Some(target), Some(v)) = (spec.target, values.get(&spec.motion))
            && (target - v).abs() > 1e-9 * (1.0 + target.abs())
        {
            return Err(format!(
                "{why} and the joint is not at its position ({} {v} instead of {target})",
                spec.motion
            ));
        }
    }
    Ok(())
}

/// The names of these joints and constraint rows of the solve, each once
/// in their order, as the timeline shows them, and their features as
/// places (mitcad#1753).
fn fighting(
    state: &DocState,
    mechanism: &Mechanism,
    ids: impl IntoIterator<Item = JointId>,
) -> (Vec<String>, Vec<crate::notice::NoticePlace>) {
    let mut names: Vec<String> = Vec::new();
    let mut places: Vec<crate::notice::NoticePlace> = Vec::new();
    for j in ids {
        let (uid, row) = mechanism.relation(j);
        let text = relation_name(state, uid, row);
        if !names.contains(&text) {
            names.push(text);
        }
        let place = crate::notice::NoticePlace::joint(uid);
        if !places.contains(&place) {
            places.push(place);
        }
    }
    (names, places)
}

/// Why a solve failed (mitcad#1753): in words, which joints and constraint
/// rows work against each other, by the names the timeline shows, and what
/// to do (free a motion, remove one of them, drive only one of them), the
/// joints painted as its places; the detail for agents names the driven
/// motions the contradiction holds.
fn solve_error(state: &DocState, mechanism: &Mechanism, result: &RigidResult) -> String {
    let name = |j: JointId| {
        let (uid, row) = mechanism.relation(j);
        relation_name(state, uid, row)
    };
    let list = |names: &[String]| match names {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    };
    if result.status == SolveStatus::Conflicting {
        let mut joints: BTreeSet<JointId> = BTreeSet::new();
        let mut targets: BTreeSet<(JointId, Freedom)> = BTreeSet::new();
        for c in &result.conflicts {
            joints.extend(&c.involved);
            targets.extend(&c.targets);
        }
        let involved: Vec<String> = joints
            .iter()
            .map(|j| {
                let text = name(*j);
                let driven: Vec<Freedom> = targets
                    .iter()
                    .filter(|(t, _)| t == j)
                    .map(|(_, f)| *f)
                    .collect();
                match (driven.as_slice(), mechanism.relation(*j).1) {
                    ([], _) => text,
                    // A constraint's value held at its rest value.
                    ([f, ..], Some(_)) => format!(
                        "{text} (at its {})",
                        if f.is_rotation() { "angle" } else { "offset" }
                    ),
                    (_, None) => {
                        let motions: Vec<&str> =
                            driven.iter().map(|f| motion_of(*f).as_str()).collect();
                        format!("{text} (at its {} position)", motions.join(", "))
                    }
                }
            })
            .collect();
        let detail = format!("over-constrained: {} cannot all hold", list(&involved));
        let (names, places) = fighting(state, mechanism, joints.iter().copied());
        // A joint held at a driven value among them: driving only one of
        // them may let the others hold.
        let driven = targets
            .iter()
            .any(|(j, _)| mechanism.relation(*j).1.is_none());
        let message = match (names.as_slice(), driven) {
            ([one], _) => format!("{one} cannot hold: free one of its motions, or remove it"),
            (_, false) => format!(
                "{} work against each other and cannot all hold: free a motion of one of \
                 them, or remove one of them",
                list(&names)
            ),
            (_, true) => format!(
                "{} cannot all hold at their positions: free a motion of one of them, remove \
                 one of them, or drive only one of them",
                list(&names)
            ),
        };
        crate::notice::said_at(message, detail, places)
    } else {
        let (names, places) = fighting(
            state,
            mechanism,
            mechanism.joints.iter().map(|(_, _, id)| *id),
        );
        let what = if mechanism.joints.iter().any(|(_, row, _)| row.is_some()) {
            "the joints and constraints"
        } else {
            "the joints"
        };
        let detail = format!(
            "{what} cannot all hold ({}) from where the occurrences are: their limits or \
             sizes keep them apart",
            list(&names)
        );
        let message = match names.as_slice() {
            [one] => format!(
                "{one} cannot hold from where the occurrences are, as limits or sizes keep it \
                 apart: change a limit or a size, free one of its motions, or remove it"
            ),
            _ => format!(
                "{} cannot all hold from where the occurrences are, as limits or sizes keep \
                 them apart: change a limit or a size, free a motion of one of them, or \
                 remove one of them",
                list(&names)
            ),
        };
        crate::notice::said_at(message, detail, places)
    }
}

fn freedom(m: Motion) -> Freedom {
    match m {
        Motion::Tx => Freedom::Tx,
        Motion::Ty => Freedom::Ty,
        Motion::Tz => Freedom::Tz,
        Motion::Rx => Freedom::Rx,
        Motion::Ry => Freedom::Ry,
        Motion::Rz => Freedom::Rz,
    }
}

fn motion_of(f: Freedom) -> Motion {
    match f {
        Freedom::Tx => Motion::Tx,
        Freedom::Ty => Motion::Ty,
        Freedom::Tz => Motion::Tz,
        Freedom::Rx => Motion::Rx,
        Freedom::Ry => Motion::Ry,
        Freedom::Rz => Motion::Rz,
    }
}

fn pose(t: &Transform) -> Pose {
    Pose {
        rotation: t.linear,
        translation: t.translation,
    }
}

fn transform(p: &Pose) -> Transform {
    Transform {
        linear: p.rotation,
        translation: p.translation,
    }
}

/// Joints of a component as the joint solver takes them: a body per unit
/// (an occurrence with those its rigid groups join to it), fixed when a
/// member is grounded; a side that is the component's own geometry is the
/// fixed world. Joints within one unit are left out.
pub(crate) struct Mechanism {
    pub system: RigidSystem,
    /// Each body's occurrences and whether it is grounded.
    pub units: Vec<(BTreeSet<OccurrenceUid>, bool)>,
    /// The solver's joints and constraint rows: their features and rows.
    pub joints: Vec<(FeatureUid, Option<usize>, JointId)>,
}

impl Mechanism {
    /// With `all_units`, every top-level occurrence of the component is a
    /// body, also those without joints.
    pub(crate) fn new(
        assembly: &Assembly,
        groups: &[(FeatureUid, ComponentUid, Vec<OccurrenceUid>)],
        component: ComponentUid,
        joints: &[&ActiveJoint],
        links: &[&crate::motion_links::ActiveLink],
        placements: &BTreeMap<OccurrenceUid, Transform>,
        all_units: bool,
    ) -> Result<Self, String> {
        let mut mechanism = Mechanism {
            system: RigidSystem::new(),
            units: Vec::new(),
            joints: Vec::new(),
        };
        if all_units {
            for o in assembly.children(component) {
                if o.pattern.is_some() && !placements.contains_key(&o.uid) {
                    continue;
                }
                mechanism.body(assembly, groups, component, o.uid);
            }
        }
        for joint in joints {
            let side = |m: &mut Mechanism, path: &[OccurrenceUid]| {
                path.first()
                    .map(|o| m.body(assembly, groups, component, *o))
            };
            let (a, b) = (
                side(&mut mechanism, &joint.a),
                side(&mut mechanism, &joint.b),
            );
            if a == b {
                continue;
            }
            let (fa, fb) = joint.frames(placements);
            if let Some(parts) = &joint.parts {
                let mut c = constraints::rigid_constraint(&fa, &fb, parts, joint.motions.first());
                (c.a, c.b) = (a, b);
                let id = mechanism
                    .system
                    .add_constraint(c)
                    .map_err(|e| e.to_string())?;
                mechanism.joints.push((joint.uid, joint.row, id));
                continue;
            }
            let id = mechanism
                .system
                .add_joint(RigidJoint {
                    a,
                    frame_a: pose(&fa),
                    b,
                    frame_b: pose(&fb),
                    motions: joint
                        .motions
                        .iter()
                        .map(|s| JointMotion {
                            freedom: freedom(s.motion),
                            min: s.min,
                            max: s.max,
                            target: s.target,
                        })
                        .collect(),
                    alignment: pose(&joint.alignment),
                })
                .map_err(|e| e.to_string())?;
            // Where a new or dragged joint's free motions are (mitcad#1745).
            for s in &joint.motions {
                if let Some(v) = s.start {
                    mechanism
                        .system
                        .set_start(id, freedom(s.motion), v)
                        .map_err(|e| e.to_string())?;
                }
            }
            mechanism.joints.push((joint.uid, None, id));
        }
        // Motion links (mitcad#466) between joints of the mechanism; a link
        // whose joint is left out (its sides move together) couples
        // nothing here.
        for link in links {
            let (Some(a), Some(b)) = (mechanism.joint_of(link.a.0), mechanism.joint_of(link.b.0))
            else {
                continue;
            };
            let id = mechanism
                .system
                .add_coupling(mitcad_solver::RigidCoupling {
                    driver: (a, freedom(link.a.1)),
                    follower: (b, freedom(link.b.1)),
                    relation: link.relation.clone(),
                })
                .map_err(|e| e.to_string())?;
            mechanism.joints.push((link.uid, None, id));
        }
        Ok(mechanism)
    }

    /// The solver's joint of a joint feature, when it is in the mechanism.
    pub(crate) fn joint_of(&self, joint: FeatureUid) -> Option<JointId> {
        self.joints
            .iter()
            .find(|(uid, row, _)| *uid == joint && row.is_none())
            .map(|(_, _, id)| *id)
    }

    /// The body of an occurrence's unit, added when new.
    fn body(
        &mut self,
        assembly: &Assembly,
        groups: &[(FeatureUid, ComponentUid, Vec<OccurrenceUid>)],
        component: ComponentUid,
        occurrence: OccurrenceUid,
    ) -> BodyId {
        if let Some(i) = self.units.iter().position(|(u, _)| u.contains(&occurrence)) {
            return BodyId(i as u32);
        }
        let members = group_of(groups, component, occurrence);
        let grounded = members
            .iter()
            .any(|o| assembly.occurrence(*o).is_some_and(|o| o.grounded));
        self.units.push((members, grounded));
        self.system.add_body(grounded)
    }

    pub(crate) fn body_of(&self, occurrence: OccurrenceUid) -> Option<BodyId> {
        self.units
            .iter()
            .position(|(u, _)| u.contains(&occurrence))
            .map(|i| BodyId(i as u32))
    }

    pub(crate) fn feature(&self, joint: JointId) -> FeatureUid {
        self.relation(joint).0
    }

    /// The feature of a solver's joint, and its row for a constraint row.
    pub(crate) fn relation(&self, joint: JointId) -> (FeatureUid, Option<usize>) {
        self.joints
            .iter()
            .find(|(_, _, j)| *j == joint)
            .map(|(uid, row, _)| (*uid, *row))
            .expect("a joint of the mechanism")
    }

    /// The placements of the occurrences the solve moved.
    pub(crate) fn changes(
        &self,
        placements: &BTreeMap<OccurrenceUid, Transform>,
    ) -> Vec<PlacementChange> {
        let mut changes = Vec::new();
        for (b, (members, _)) in self.units.iter().enumerate() {
            let motion = self.system.body_pose(BodyId(b as u32));
            let (distance, turn) = motion.distance(&Pose::IDENTITY);
            if distance <= 1e-12 && turn <= 1e-12 {
                continue;
            }
            let motion = transform(&motion);
            for o in members {
                let current = placements.get(o).copied().unwrap_or(Transform::IDENTITY);
                // Rotations and translations again, to rounding
                // (mitcad#1664, mitcad#1666), so that rounding does not pile
                // up solve after solve; one far off is left to the command
                // that would keep it, which refuses.
                let moved = motion.after(&current);
                let moved = crate::assembly::solved_placement(&moved).unwrap_or(moved);
                changes.push(PlacementChange::Set(*o, moved));
            }
        }
        changes
    }

    /// Each joint's values after a solve or drag (constraint rows have
    /// none to drive).
    pub(crate) fn values(
        &self,
        active: &[&ActiveJoint],
    ) -> BTreeMap<FeatureUid, BTreeMap<Motion, f64>> {
        self.joints
            .iter()
            .filter(|(_, row, _)| row.is_none())
            .filter_map(|(uid, _, id)| {
                let joint = active.iter().find(|j| j.uid == *uid)?;
                let values = joint
                    .motions
                    .iter()
                    .zip(self.system.joint_values(*id))
                    .map(|(s, v)| (s.motion, *v))
                    .collect();
                Some((*uid, values))
            })
            .collect()
    }
}

/// The motion links in effect in a component (mitcad#466).
pub(crate) fn links_of(
    joints: &JointsState,
    component: ComponentUid,
) -> Vec<&crate::motion_links::ActiveLink> {
    joints
        .links
        .iter()
        .filter(|l| l.component == component)
        .collect()
}

/// The occurrence and those rigid groups of `component` join it to.
pub(crate) fn group_of(
    groups: &[(FeatureUid, ComponentUid, Vec<OccurrenceUid>)],
    component: ComponentUid,
    occurrence: OccurrenceUid,
) -> BTreeSet<OccurrenceUid> {
    let mut members = BTreeSet::from([occurrence]);
    loop {
        let before = members.len();
        for (_, c, group) in groups {
            if *c == component && group.iter().any(|o| members.contains(o)) {
                members.extend(group.iter().copied());
            }
        }
        if members.len() == before {
            return members;
        }
    }
}

/// A feature's placement changes with its rigid groups: a move of an
/// occurrence moves the occurrences its groups join to it by the same
/// motion (once each).
pub(crate) fn with_groups(
    groups: &[(FeatureUid, ComponentUid, Vec<OccurrenceUid>)],
    component: ComponentUid,
    changes: &[PlacementChange],
) -> Vec<PlacementChange> {
    let listed: BTreeSet<OccurrenceUid> = changes
        .iter()
        .map(|c| match c {
            PlacementChange::Move(o, _) | PlacementChange::Set(o, _) => *o,
        })
        .collect();
    let mut seen = listed.clone();
    let mut out = changes.to_vec();
    for change in changes {
        if let PlacementChange::Move(o, by) = change {
            for member in group_of(groups, component, *o) {
                if seen.insert(member) {
                    out.push(PlacementChange::Move(member, *by));
                }
            }
        }
    }
    out
}

/// Where an occurrence of `component` (a path in it) is in the design, as
/// a notice's place: its path of uids from the root, through the
/// component's first placement.
fn occurrence_place(
    assembly: &Assembly,
    component: ComponentUid,
    path: &[OccurrenceUid],
) -> Option<crate::notice::NoticePlace> {
    let mut full = assembly.paths_to(component).into_iter().next()?;
    full.extend_from_slice(path);
    (!full.is_empty()).then(|| {
        crate::notice::NoticePlace::occurrence(
            full.iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("/"),
        )
    })
}

/// Warnings of a joint that placed `moved` (mitcad#1745): an occurrence a
/// move or a captured position later in the timeline places again does
/// not stay where the joint puts it. In words with component names (the
/// feature's name for agents), the occurrence painted.
fn placed_again(state: &DocState, entry: &FeatureEntry, moved: &[OccurrenceUid]) -> Vec<String> {
    let assembly = &state.assembly;
    let Some(at) = state.features.iter().position(|f| f.uid == entry.uid) else {
        return Vec::new();
    };
    let mut warnings = Vec::new();
    for o in moved {
        let later = state.features[at + 1..]
            .iter()
            .filter(|f| !f.suppressed)
            .find_map(|f| match &f.def {
                FeatureDef::MoveOccurrence(m) if m.occurrences.contains(o) => Some((f, true)),
                FeatureDef::CapturePosition(c)
                    if c.positions.iter().any(|p| p.occurrence == *o) =>
                {
                    Some((f, false))
                }
                _ => None,
            });
        let Some((feature, by_move)) = later else {
            continue;
        };
        let name = assembly.occurrence_name(*o);
        let message = if by_move {
            format!(
                "{name} is placed again by a move later in the timeline, so it does not stay \
                 where this joint puts it"
            )
        } else {
            format!(
                "{name} is placed again by a captured position later in the timeline, so it \
                 does not stay where this joint puts it"
            )
        };
        warnings.push(crate::notice::said_at(
            message,
            format!(
                "{name} is placed again by {} later in the timeline, so it does not stay \
                 where this joint puts it",
                feature.name
            ),
            occurrence_place(assembly, entry.component, &[*o]),
        ));
    }
    warnings
}

/// Names in a sentence: `A`, `A and B`, `A, B and C`.
fn listed(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// Why both sides of a joint move as one unit (mitcad#1745), in words, and
/// the places of the unit's occurrences: a rigid group joins them, or both
/// are in one occurrence.
fn one_unit(
    assembly: &Assembly,
    component: ComponentUid,
    path: &[OccurrenceUid],
    unit: &BTreeSet<OccurrenceUid>,
) -> (String, Vec<crate::notice::NoticePlace>) {
    let places = unit
        .iter()
        .filter_map(|o| occurrence_place(assembly, component, &[*o]))
        .collect();
    let why = if unit.len() > 1 {
        let members: Vec<String> = unit.iter().map(|o| assembly.occurrence_name(*o)).collect();
        format!(
            "both sides move as one (a rigid group joins {})",
            listed(&members)
        )
    } else {
        let at = path
            .first()
            .map(|o| assembly.occurrence_name(*o))
            .unwrap_or_default();
        format!("both sides move as one (both are in {at})")
    };
    (why, places)
}

/// Why a side of joint `joint` cannot move (mitcad#1745), in words with
/// component names only, and the places of what holds it; None when it can
/// move. A side is held when it is the component's own geometry, when its
/// unit has a grounded member (itself, or one a rigid group joins to it),
/// or when the joints before it that let nothing move (rigid ones, or all
/// of whose motions are driven) join it to such a unit.
fn held_side(
    state: &DocState,
    joints: &JointsState,
    component: ComponentUid,
    path: &[OccurrenceUid],
    joint: FeatureUid,
) -> Option<(String, Vec<crate::notice::NoticePlace>)> {
    let assembly = &state.assembly;
    let groups = &joints.groups;
    let place = |o: OccurrenceUid| occurrence_place(assembly, component, &[o]);
    let Some(first) = path.first().copied() else {
        return Some((
            format!(
                "an origin on {}'s own geometry does not move",
                assembly.name(component)
            ),
            Vec::new(),
        ));
    };
    let name = assembly.occurrence_name(first);
    let grounded = |o: &OccurrenceUid| assembly.occurrence(*o).is_some_and(|o| o.grounded);
    // A unit's reason, when one of its members is grounded.
    let unit_held = |o: OccurrenceUid| -> Option<(String, Vec<crate::notice::NoticePlace>)> {
        let unit = group_of(groups, component, o);
        let held: Vec<OccurrenceUid> = unit.iter().copied().filter(grounded).collect();
        if held.is_empty() {
            return None;
        }
        let who = assembly.occurrence_name(o);
        if held.contains(&o) {
            return Some((format!("{who} is grounded"), place(o).into_iter().collect()));
        }
        let names: Vec<String> = held.iter().map(|h| assembly.occurrence_name(*h)).collect();
        let places = std::iter::once(o)
            .chain(held.iter().copied())
            .filter_map(place)
            .collect();
        let why = if held.len() == 1 {
            format!(
                "{who} is in a rigid group with {}, which is grounded",
                listed(&names)
            )
        } else {
            format!(
                "{who} is in a rigid group with {}, which are grounded",
                listed(&names)
            )
        };
        Some((why, places))
    };
    if let Some(found) = unit_held(first) {
        return Some(found);
    }
    // Joined to a held unit by joints before this one that let nothing
    // move: a search outward from this side's unit.
    let still: Vec<&ActiveJoint> = joints
        .active
        .iter()
        .filter(|j| j.component == component && j.uid != joint && j.row.is_none())
        .filter(|j| j.motions.iter().all(|m| m.target.is_some()))
        // A motion link may let a driven motion go.
        .filter(|j| {
            !joints
                .links
                .iter()
                .any(|l| l.a.0 == j.uid || l.b.0 == j.uid)
        })
        .collect();
    let mut seen: Vec<OccurrenceUid> = vec![first];
    let mut queue: Vec<OccurrenceUid> = vec![first];
    while let Some(at) = queue.pop() {
        let unit = group_of(groups, component, at);
        for j in &still {
            let (a, b) = (j.a.first().copied(), j.b.first().copied());
            let other = match (a, b) {
                (Some(x), y) if unit.contains(&x) => y,
                (x, Some(y)) if unit.contains(&y) => x,
                _ => continue,
            };
            let Some(other) = other else {
                // Joined rigidly to the component's own geometry.
                return Some((
                    format!(
                        "{name} is joined rigidly to {}'s own geometry",
                        assembly.name(component)
                    ),
                    place(first).into_iter().collect(),
                ));
            };
            if seen.contains(&other) {
                continue;
            }
            if let Some((_, places)) = unit_held(other) {
                let to = assembly.occurrence_name(other);
                let mut all: Vec<_> = place(first).into_iter().collect();
                all.extend(places);
                return Some((
                    format!("{name} is joined rigidly to {to}, which cannot move"),
                    all,
                ));
            }
            seen.push(other);
            queue.push(other);
        }
    }
    None
}

fn names(assembly: &Assembly, occurrences: &BTreeSet<OccurrenceUid>) -> String {
    occurrences
        .iter()
        .map(|o| assembly.occurrence_name(*o))
        .collect::<Vec<_>>()
        .join(", ")
}

// Dragging.

/// Where a drag puts the occurrences and the joints' values.
#[derive(Debug, Clone, PartialEq)]
pub struct Dragged {
    /// The occurrences that moved and their placements.
    pub placements: Vec<(OccurrenceUid, Transform)>,
    pub values: BTreeMap<FeatureUid, BTreeMap<Motion, f64>>,
    /// The contact of a contact set that stopped the drag short of the
    /// target (mitcad#454).
    pub contact: Option<crate::contact::ContactHit>,
}

/// Pulls `grab` (a point in the parent component's coordinates, moving
/// with `occurrence`) toward `target` while the joints of the parent
/// component hold, moving the other occurrences as little as possible.
/// Driven motions follow like free ones.
pub(crate) fn drag(
    assembly: &Assembly,
    joints: &JointsState,
    placements: &BTreeMap<OccurrenceUid, Transform>,
    occurrence: OccurrenceUid,
    grab: Vec3,
    target: Vec3,
) -> Result<Dragged, String> {
    let o = assembly
        .occurrence(occurrence)
        .ok_or_else(|| format!("occurrence {occurrence} does not exist"))?;
    let component = o.parent;
    let active: Vec<&ActiveJoint> = joints
        .active
        .iter()
        .filter(|j| j.component == component)
        .collect();
    let mut mechanism = Mechanism::new(
        assembly,
        &joints.groups,
        component,
        &active,
        &links_of(joints, component),
        placements,
        true,
    )?;
    let body = mechanism
        .body_of(occurrence)
        .expect("every child is a body");
    if mechanism.units[body.0 as usize].1 {
        return Err(format!(
            "{} is grounded",
            names(assembly, &mechanism.units[body.0 as usize].0)
        ));
    }
    let result = mechanism
        .system
        .drag(body, grab, target, &RigidOptions::default());
    if !result.is_ok() {
        return Err("the joints do not hold where the occurrences are".to_owned());
    }
    let placements = mechanism
        .changes(placements)
        .into_iter()
        .map(|c| match c {
            PlacementChange::Move(o, t) | PlacementChange::Set(o, t) => (o, t),
        })
        .collect();
    Ok(Dragged {
        placements,
        values: mechanism.values(&active),
        contact: None,
    })
}

/// Where a joint's motion driven to each of `values` in turn puts the
/// occurrences of the joint's component (Animate Joint's frames,
/// mitcad#1701): the joints of the component solved as a drag solves them,
/// with the motion driven there (held within its limits) and the motions
/// motion links couple to it following, each frame from the one before.
/// Per frame the placements of every occurrence of `placements` (those
/// that moved changed) and the joints' values. Nothing is recomputed: as a
/// drag, the joints of other components and the shapes stay as they are.
pub(crate) fn poses(
    assembly: &Assembly,
    joints: &JointsState,
    placements: &BTreeMap<OccurrenceUid, Transform>,
    joint: FeatureUid,
    motion: Motion,
    values: &[f64],
) -> Result<Vec<Dragged>, String> {
    let driven = joints
        .active
        .iter()
        .find(|j| j.uid == joint && j.row.is_none())
        .ok_or_else(|| "the joint is not in effect at the marker".to_owned())?;
    let spec = driven
        .motions
        .iter()
        .find(|s| s.motion == motion)
        .copied()
        .ok_or_else(|| format!("the joint has no free motion {motion}"))?;
    let component = driven.component;
    let links = links_of(joints, component);
    // The motions the links couple to the driven one follow it (as
    // `drive_joint` releases them).
    let mut released: BTreeSet<(FeatureUid, Motion)> = BTreeSet::from([(joint, motion)]);
    let mut queue = vec![(joint, motion)];
    while let Some(at) = queue.pop() {
        for link in &links {
            let other = if link.a == at {
                link.b
            } else if link.b == at {
                link.a
            } else {
                continue;
            };
            if released.insert(other) {
                queue.push(other);
            }
        }
    }
    let mut active: Vec<ActiveJoint> = joints
        .active
        .iter()
        .filter(|j| j.component == component)
        .cloned()
        .collect();
    for j in active.iter_mut().filter(|j| j.row.is_none()) {
        for s in &mut j.motions {
            if released.contains(&(j.uid, s.motion)) {
                s.target = None;
            }
        }
    }
    let mut current = placements.clone();
    let mut out = Vec::with_capacity(values.len());
    for &value in values {
        if !value.is_finite() {
            return Err("the values must be finite".to_owned());
        }
        let mut v = value;
        if let Some(min) = spec.min {
            v = v.max(min);
        }
        if let Some(max) = spec.max {
            v = v.min(max);
        }
        for j in active
            .iter_mut()
            .filter(|j| j.uid == joint && j.row.is_none())
        {
            for s in j.motions.iter_mut().filter(|s| s.motion == motion) {
                s.target = Some(v);
            }
        }
        let refs: Vec<&ActiveJoint> = active.iter().collect();
        let mut mechanism = Mechanism::new(
            assembly,
            &joints.groups,
            component,
            &refs,
            &links,
            &current,
            false,
        )?;
        let result = mechanism.system.solve(&RigidOptions::default());
        if !result.is_ok() {
            return Err(format!(
                "the joints do not hold with {motion} at {}",
                if motion.is_rotation() {
                    format!("{:.3} deg", v.to_degrees())
                } else {
                    format!("{v:.3} mm")
                }
            ));
        }
        let moved: Vec<(OccurrenceUid, Transform)> = mechanism
            .changes(&current)
            .into_iter()
            .map(|c| match c {
                PlacementChange::Move(o, t) | PlacementChange::Set(o, t) => (o, t),
            })
            .collect();
        let frame_values = mechanism.values(&refs);
        current.extend(moved.iter().copied());
        out.push(Dragged {
            placements: current.iter().map(|(o, t)| (*o, *t)).collect(),
            values: frame_values,
            contact: None,
        });
    }
    Ok(out)
}

// Degrees of freedom.

/// A set of top-level occurrences of a component that move as one (an
/// occurrence alone or a rigid group) and its remaining degrees of
/// freedom.
#[derive(Debug, Clone, PartialEq)]
pub struct DofUnit {
    pub occurrences: Vec<OccurrenceUid>,
    pub grounded: bool,
    /// The motions it can make with the other units held where they are
    /// (the rank of the joint equations); 0 when grounded.
    pub dof: u32,
    /// Whether it moves when the other units may move too (a link of a
    /// four-bar linkage: `dof` 0, but it moves); false when grounded or
    /// when the joints hold it fixed.
    pub mobile: bool,
    /// The joints between it and other units or the component.
    pub joints: Vec<FeatureUid>,
}

/// The degrees of freedom of a component's top-level occurrences.
#[derive(Debug, Clone, PartialEq)]
pub struct DofReport {
    pub component: ComponentUid,
    pub units: Vec<DofUnit>,
    /// The motions all units can make together: six per unit that is not
    /// grounded less the rank of the joint equations (driven values left
    /// out: they say where a motion is, not whether it can move).
    pub total: u32,
    /// Some joints take away motions that others already took
    /// (`redundant`) or contradict them where the occurrences are.
    pub overconstrained: bool,
    /// Joints whose equations repeat earlier joints' (they hold).
    pub redundant: Vec<FeatureUid>,
    /// Joints that contradict earlier ones where the occurrences are.
    pub conflicting: Vec<FeatureUid>,
}

/// The degrees of freedom of `component`'s top-level occurrences from the
/// joints in effect at the marker (those that recomputed; suppressed,
/// failed and rolled back ones do not count), grounding and rigid groups,
/// where the occurrences are.
pub(crate) fn dof_report(
    assembly: &Assembly,
    joints: &JointsState,
    placements: &BTreeMap<OccurrenceUid, Transform>,
    component: ComponentUid,
) -> DofReport {
    let active: Vec<&ActiveJoint> = joints
        .active
        .iter()
        .filter(|j| j.component == component)
        .collect();
    let mechanism = match Mechanism::new(
        assembly,
        &joints.groups,
        component,
        &active,
        &links_of(joints, component),
        placements,
        true,
    ) {
        Ok(m) => m,
        Err(_) => {
            return DofReport {
                component,
                units: Vec::new(),
                total: 0,
                overconstrained: false,
                redundant: Vec::new(),
                conflicting: Vec::new(),
            };
        }
    };
    let analysis = mechanism.system.analyze();
    let units = mechanism
        .units
        .iter()
        .enumerate()
        .map(|(b, (members, grounded))| {
            let mine = |path: &[OccurrenceUid]| path.first().is_some_and(|o| members.contains(o));
            let mut unit_joints: Vec<FeatureUid> = active
                .iter()
                .filter(|j| mine(&j.a) != mine(&j.b))
                .map(|j| j.uid)
                .collect();
            // A constraint set's rows once.
            unit_joints.dedup();
            DofUnit {
                occurrences: members.iter().copied().collect(),
                grounded: *grounded,
                dof: analysis.body_dof[b] as u32,
                mobile: analysis.body_mobile[b],
                joints: unit_joints,
            }
        })
        .collect();
    let features = |deps: &[mitcad_solver::RigidDependency]| -> Vec<FeatureUid> {
        let mut uids: Vec<FeatureUid> = deps.iter().map(|d| mechanism.feature(d.joint)).collect();
        uids.dedup();
        uids
    };
    let redundant = features(&analysis.redundant);
    let conflicting = features(&analysis.conflicts);
    DofReport {
        component,
        units,
        total: analysis.dof as u32,
        overconstrained: !redundant.is_empty() || !conflicting.is_empty(),
        redundant,
        conflicting,
    }
}

/// The joint features whose occurrences no longer all exist.
pub(crate) fn orphaned(state: &DocState) -> BTreeSet<FeatureUid> {
    state
        .features
        .iter()
        .filter(|f| {
            if matches!(f.def, FeatureDef::PatternAtPoints(_)) {
                return false;
            }
            // Missing occurrence paths stay repairable failures. Deleting a
            // component definition still uses normal dependent-feature removal.
            if matches!(&f.def, FeatureDef::Extrude(e) if e.active_cut_scope().is_some()) {
                return false;
            }
            occurrences_used(&f.def)
                .iter()
                .any(|o| state.assembly.occurrence(*o).is_none())
        })
        .map(|f| f.uid)
        .collect()
}

/// Every free motion's limits as values: min, max and rest.
#[doc(hidden)]
pub fn limit_values(
    params: &Parameters,
    limits: &BTreeMap<Motion, crate::features::joint::Limit<ParamId>>,
) -> BTreeMap<Motion, [Option<f64>; 3]> {
    limits
        .iter()
        .map(|(m, l)| {
            let v = |id: &Option<ParamId>| id.and_then(|id| params.value(id));
            (*m, [v(&l.min), v(&l.max), v(&l.rest)])
        })
        .collect()
}

// The frame of a joint origin before the joint exists (mitcad#778).

/// The frame a joint origin gives at the marker
/// ([`Document::joint_frame`](crate::document::Document::joint_frame); the
/// api's `joint_frame` query and the import's joints ask for it).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OriginFrame {
    /// The component the occurrence path ends in, whose geometry the
    /// origin is.
    pub component: ComponentUid,
    /// The frame in the coordinates of the component the path starts in.
    pub frame: Transform,
    /// The frame in the coordinates of its own component.
    pub local: Transform,
}
