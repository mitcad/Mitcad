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
    BodyId, Freedom, JointId, JointMotion, Pose, RigidJoint, RigidOptions, RigidResult,
    RigidSystem, SolveStatus,
};
use serde::{Deserialize, Serialize};

use crate::assembly::{Assembly, inverse};
use crate::datum::{Datum, Vec3, canonical_axis, default_x_axis, unit};
use crate::document::DocState;
use crate::features::geom_ref::{self, GeomRef, Resolver};
use crate::features::joint::{FrameOverride, JointOrigin};
use crate::features::{FeatureDef, FeatureEntry, PlacementChange, SketchOutput};
use crate::ids::{BodyUid, ComponentUid, FeatureUid, OccurrenceUid};
use crate::kernel::Kernel;
use crate::parameters::{ParamId, Parameters};
use crate::profile::{cross, dot};
use crate::recompute::{BodyState, Recomputed, Versioned, place};
use crate::sketch::geometry::Curve2;
use crate::transform::Transform;

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
pub(crate) fn frame(origin: Vec3, x_axis: Vec3, z_axis: Vec3) -> Transform {
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
pub(crate) fn frame_parts(t: &Transform) -> [Vec3; 4] {
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
pub(crate) fn alignment(angle: f64, offset: f64, flip: bool) -> Transform {
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
pub(crate) fn apply_override(frame_in: &Transform, o: &FrameOverride) -> Result<Transform, String> {
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
    Ok(frame(origin, x, z))
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
pub(crate) fn origin_frame(r: &mut impl Resolver, geometry: &GeomRef) -> Result<Transform, String> {
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
            let kernel = r.kernel();
            let surface = kernel
                .face_geometry(&shape, face)
                .map_err(|e| format!("face {face}: {e}"))?;
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
                    "face {face} ({kind}) has no plane, axis or centre for a joint"
                )),
            }
        }
        GeomRef::Edge { body, edge } => {
            let shape = r.shape(*body)?;
            use crate::datum::CurveGeometry as C;
            match r
                .kernel()
                .edge_geometry(&shape, edge)
                .map_err(|e| format!("edge {edge}: {e}"))?
            {
                C::Circle { center, normal, .. } => {
                    let axis = canonical_axis(unit(normal).ok_or("the circle has no normal")?);
                    Ok(frame_along(center, axis))
                }
                C::Line { start, end } => {
                    let along = unit(std::array::from_fn(|i| end[i] - start[i]))
                        .ok_or_else(|| format!("edge {edge} has no length"))?;
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
        GeomRef::Body(_) | GeomRef::SketchCurves { .. } => Err(format!(
            "{geometry} gives no frame for a joint (a face, an edge, a point, a plane or an axis)"
        )),
    }
}

// Occurrence paths.

/// The component an occurrence path from `component` ends in: the last
/// occurrence's, or `component` itself for the empty path. Fails for an
/// occurrence that does not exist or is not placed where the path says.
pub(crate) fn path_component(
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
pub(crate) fn find_path_from(
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
    )
}

/// The occurrences a joint feature names (in paths, as members).
pub(crate) fn occurrences_used(def: &FeatureDef) -> Vec<OccurrenceUid> {
    match def {
        FeatureDef::Joint(j) => [&j.a.occurrence.0[..], &j.b.occurrence.0[..]].concat(),
        FeatureDef::AsBuiltJoint(j) => {
            let mut used = [&j.a.0[..], &j.b.0[..]].concat();
            if let Some(origin) = &j.origin {
                used.extend(&origin.occurrence.0);
            }
            used
        }
        FeatureDef::RigidGroup(g) => g.occurrences.clone(),
        _ => Vec::new(),
    }
}

/// Renames the occurrences a joint feature names (copies of components);
/// true when it is one.
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
pub(crate) struct MotionSpec {
    pub motion: Motion,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub target: Option<f64>,
}

/// A joint in effect, as the joint solver takes it: each side's frame is
/// `placement(path) · local`, with `local` in the coordinates of the
/// component the path ends in (resolved at the joint's point of the
/// timeline), and the joint holds when
/// `frame_a = frame_b · motion(values) · alignment`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ActiveJoint {
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
}

impl ActiveJoint {
    /// The two frames with these placements.
    fn frames(&self, placements: &BTreeMap<OccurrenceUid, Transform>) -> (Transform, Transform) {
        (
            path_placement(placements, &self.a).after(&self.local_a),
            path_placement(placements, &self.b).after(&self.local_b),
        )
    }

    /// The free motions' values with these placements (turns unwrapped
    /// near their targets or limits), None when the joint does not hold.
    fn values(
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
pub(crate) struct JointsState {
    pub joints: BTreeMap<FeatureUid, JointResult>,
    /// Rigid groups in effect, in timeline order: the feature, its
    /// component and the occurrences.
    pub groups: Vec<(FeatureUid, ComponentUid, Vec<OccurrenceUid>)>,
    /// Joints in effect, in timeline order.
    pub active: Vec<ActiveJoint>,
}

/// Whether recompute applies the feature with [`apply`].
pub(crate) fn applies(def: &FeatureDef) -> bool {
    matches!(
        def,
        FeatureDef::Joint(_) | FeatureDef::AsBuiltJoint(_) | FeatureDef::RigidGroup(_)
    )
}

/// Resolves references in one component at a point of a recompute.
struct StepResolver<'a, K: Kernel> {
    kernel: &'a K,
    assembly: &'a Assembly,
    component: ComponentUid,
    bodies: Option<&'a BodyState<K::Shape>>,
    datums: &'a BTreeMap<FeatureUid, Versioned<Datum>>,
    sketches: &'a BTreeMap<FeatureUid, Versioned<Arc<SketchOutput>>>,
    features: &'a [Arc<FeatureEntry>],
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
) -> Result<(), String> {
    let assembly = &state.assembly;
    let params = &state.parameters;
    let component = entry.component;
    if let FeatureDef::RigidGroup(group) = &entry.def {
        for o in &group.occurrences {
            path_component(assembly, component, &[*o])?;
        }
        done.joints
            .groups
            .push((entry.uid, component, group.occurrences.clone()));
        return Ok(());
    }
    // An origin's frame in the component its path ends in.
    let local = |origin: &JointOrigin| -> Result<Transform, String> {
        let at = path_component(assembly, component, &origin.occurrence.0)?;
        let mut resolver = StepResolver {
            kernel,
            assembly,
            component: at,
            bodies: components.get(&at).map(|b| &**b),
            datums: &done.datums,
            sketches: &done.sketches,
            features: &state.features,
        };
        let mut local = origin_frame(&mut resolver, &origin.geometry)?;
        if let Some(o) = &origin.frame_override {
            local = apply_override(&local, o)?;
        }
        Ok(local)
    };

    let (kind, slide, limits, position, a_path, b_path) = match &entry.def {
        FeatureDef::Joint(j) => (
            j.kind,
            j.slide_axis,
            &j.limits,
            &j.position,
            &j.a.occurrence.0,
            &j.b.occurrence.0,
        ),
        FeatureDef::AsBuiltJoint(j) => {
            (j.kind, j.slide_axis, &j.limits, &j.position, &j.a.0, &j.b.0)
        }
        _ => return Ok(()),
    };
    path_component(assembly, component, a_path).map_err(|e| format!("a: {e}"))?;
    path_component(assembly, component, b_path).map_err(|e| format!("b: {e}"))?;
    let motions = motion_specs(params, kind, slide, limits, position)?;

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
    };
    let (state_now, changes) = solve_joint(state, entry, &joint, done)?;
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
    Ok(())
}

/// After recompute: each joint's frames and values where the occurrences
/// are at the marker.
pub(crate) fn finish(joints: &mut JointsState, placements: &BTreeMap<OccurrenceUid, Transform>) {
    for joint in &joints.active {
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
}

/// Solves a new joint with the joints in effect that share moving
/// occurrences with it (through each other) and places the occurrences
/// that move, as placement changes of the joint feature applied like
/// other features' (`recompute::place`).
fn solve_joint<S>(
    state: &DocState,
    entry: &FeatureEntry,
    joint: &ActiveJoint,
    done: &mut Recomputed<S>,
) -> Result<(JointState, Vec<PlacementChange>), String> {
    let assembly = &state.assembly;
    let component = joint.component;
    let groups = &done.joints.groups;
    let unit = |path: &[OccurrenceUid]| path.first().map(|o| group_of(groups, component, *o));
    let fixed = |u: &Option<BTreeSet<OccurrenceUid>>| {
        u.as_ref().is_none_or(|members| {
            members
                .iter()
                .any(|o| assembly.occurrence(*o).is_some_and(|o| o.grounded))
        })
    };
    let (ua, ub) = (unit(&joint.a), unit(&joint.b));
    // Nothing the solver can move: the joint holds or fails.
    let still = match (&ua, &ub) {
        (Some(x), Some(y)) if x == y => {
            Some(format!("both sides move with {}", names(assembly, x)))
        }
        _ if fixed(&ua) && fixed(&ub) => Some(format!(
            "both sides are fixed ({} and {})",
            side_name(assembly, component, &joint.a),
            side_name(assembly, component, &joint.b)
        )),
        _ => None,
    };
    if let Some(why) = still {
        let Some(values) = joint.values(&done.placements) else {
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
        return Ok((JointState::Satisfied, Vec::new()));
    }

    // The joints in effect that share moving units with this one, through
    // each other.
    let mine: Vec<&ActiveJoint> = done
        .joints
        .active
        .iter()
        .filter(|j| j.component == component)
        .chain(std::iter::once(joint))
        .collect();
    let mut moving: Vec<BTreeSet<OccurrenceUid>> = [ua, ub]
        .into_iter()
        .filter(|u| !fixed(u))
        .flatten()
        .collect();
    let mut taken = vec![false; mine.len()];
    loop {
        let mut grown = false;
        for (i, j) in mine.iter().enumerate() {
            if taken[i] {
                continue;
            }
            let sides = [unit(&j.a), unit(&j.b)];
            if !sides.iter().flatten().any(|u| moving.contains(u)) {
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
    let mut mechanism = Mechanism::new(
        assembly,
        groups,
        component,
        &cluster,
        &done.placements,
        false,
    )?;
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

/// Why a solve failed, by the joints' names.
fn solve_error(state: &DocState, mechanism: &Mechanism, result: &RigidResult) -> String {
    let name = |uid: FeatureUid| {
        state
            .features
            .iter()
            .find(|f| f.uid == uid)
            .map_or_else(|| uid.to_string(), |f| f.name.clone())
    };
    let list = |names: Vec<String>| match names.as_slice() {
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
        let involved = joints
            .iter()
            .map(|j| {
                let text = name(mechanism.feature(*j));
                let driven: Vec<&str> = targets
                    .iter()
                    .filter(|(t, _)| t == j)
                    .map(|(_, f)| motion_of(*f).as_str())
                    .collect();
                if driven.is_empty() {
                    text
                } else {
                    format!("{text} (at its {} position)", driven.join(", "))
                }
            })
            .collect();
        format!("over-constrained: {} cannot all hold", list(involved))
    } else {
        let all = mechanism.joints.iter().map(|(uid, _)| name(*uid)).collect();
        format!(
            "the joints cannot all hold ({}) from where the occurrences are: their limits or \
             sizes keep them apart",
            list(all)
        )
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
    /// The solver's joints and their features.
    pub joints: Vec<(FeatureUid, JointId)>,
}

impl Mechanism {
    /// With `all_units`, every top-level occurrence of the component is a
    /// body, also those without joints.
    pub(crate) fn new(
        assembly: &Assembly,
        groups: &[(FeatureUid, ComponentUid, Vec<OccurrenceUid>)],
        component: ComponentUid,
        joints: &[&ActiveJoint],
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
            mechanism.joints.push((joint.uid, id));
        }
        Ok(mechanism)
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
        self.joints
            .iter()
            .find(|(_, j)| *j == joint)
            .map(|(uid, _)| *uid)
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
                changes.push(PlacementChange::Set(*o, motion.after(&current)));
            }
        }
        changes
    }

    /// Each joint's values after a solve or drag.
    pub(crate) fn values(
        &self,
        active: &[&ActiveJoint],
    ) -> BTreeMap<FeatureUid, BTreeMap<Motion, f64>> {
        self.joints
            .iter()
            .filter_map(|(uid, id)| {
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

fn names(assembly: &Assembly, occurrences: &BTreeSet<OccurrenceUid>) -> String {
    occurrences
        .iter()
        .map(|o| assembly.occurrence_name(*o))
        .collect::<Vec<_>>()
        .join(", ")
}

fn side_name(assembly: &Assembly, component: ComponentUid, path: &[OccurrenceUid]) -> String {
    if path.is_empty() {
        assembly.name(component)
    } else {
        assembly.path_name(path)
    }
}

// Dragging.

/// Where a drag puts the occurrences and the joints' values.
#[derive(Debug, Clone, PartialEq)]
pub struct Dragged {
    /// The occurrences that moved and their placements.
    pub placements: Vec<(OccurrenceUid, Transform)>,
    pub values: BTreeMap<FeatureUid, BTreeMap<Motion, f64>>,
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
    })
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
            let unit_joints = active
                .iter()
                .filter(|j| mine(&j.a) != mine(&j.b))
                .map(|j| j.uid)
                .collect();
            DofUnit {
                occurrences: members.iter().copied().collect(),
                grounded: *grounded,
                dof: analysis.body_dof[b] as u32,
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
            occurrences_used(&f.def)
                .iter()
                .any(|o| state.assembly.occurrence(*o).is_none())
        })
        .map(|f| f.uid)
        .collect()
}

/// Every free motion's limits as values: min, max and rest.
pub(crate) fn limit_values(
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
