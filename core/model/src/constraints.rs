// SPDX-License-Identifier: MIT
//! Assembly constraints (mitcad#148): what each row of a `constraint_set`
//! (`features/constraint_set.rs`) holds, and what recompute does with it.
//!
//! A row's references resolve like joint origins (`crate::joints`): a
//! frame in the component its path ends in, moved into the feature's
//! component by the path's placements. Each also has a kind: a point, a
//! plane (z its normal), an axis (z along it), a circle (z the normal of
//! the planar face it bounds, else of its plane), a cylinder (its axis and
//! radius) or a sphere (its centre and radius). The row's type turns the
//! two into some of the joint solver's equations
//! (`mitcad_solver::ConstraintPart`, in frame `b`'s coordinates):
//!
//! | Type | References | Equations |
//! |---|---|---|
//! | align | plane, plane | a's origin at `offset` above b's plane, normals facing (`flip`: the same way) |
//! | align | points | coincident (at a distance: `offset`) |
//! | align | axes (axes, cylinders, circles) | concentric (`offset` apart, parallel) |
//! | align | point and plane, point and axis, axis and plane | on it (`offset` off it) |
//! | angle | planes or axes | the angle between the normals or axes (`flip` turns a over) |
//! | center | a (and a2); b and b2 | a's point (the middle of a and a2) on the plane midway between b and b2; with b alone on b's normal through its middle (on its axis) |
//! | tangent | plane, cylinder or sphere pairs | touching outside (`flip`: inside) |
//! | insert | circle, circle | centres together, `offset` along b's normal, planes facing (`flip`: the same way); `lock_rotation`: a's x axis at `angle` about b's |
//! | insert_axis | circle, circle or cylinder | a's centre on b's axis, depth free; `lock_rotation` as insert |
//!
//! An align's or angle's value is free between its `limits` (held at the
//! rest value when given) instead of fixed: the solver's value of the
//! constraint. A set's rows are solved together with the joints and
//! constraints before them in the component that share moving units, as a
//! joint is (`joints::apply`); a row whose sides cannot move only holds or
//! fails. Conflicts name rows: `Constraint1 row 2 (insert)`.

use std::collections::BTreeMap;
use std::f64::consts::PI;

use mitcad_solver::{Amount, ConstraintPart, Freedom, JointMotion, Pose, RigidConstraint, Sense};

use crate::datum::{CurveGeometry, Datum, SurfaceGeometry, Vec3};
use crate::features::geom_ref::{GeomRef, Resolver};
use crate::features::{ConstraintRow, ConstraintType, JointOrigin};
use crate::ids::OccurrenceUid;
use crate::joints::{JointState, Motion, MotionSpec, apply_override, frame_parts, origin_frame};
use crate::kernel::Kernel;
use crate::parameters::{ParamId, Parameters};
use crate::sketch::geometry::Curve2;
use crate::transform::Transform;
use mitcad_vec3::dot;
use mitcad_vec3::unit;

/// What a reference of a constraint is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeoKind {
    Point,
    Plane,
    Axis,
    Circle,
    Cylinder,
    Sphere,
}

impl GeoKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Point => "point",
            Self::Plane => "plane",
            Self::Axis => "axis",
            Self::Circle => "circle",
            Self::Cylinder => "cylinder",
            Self::Sphere => "sphere",
        }
    }

    fn pointlike(self) -> bool {
        matches!(self, Self::Point | Self::Sphere)
    }

    fn axial(self) -> bool {
        matches!(self, Self::Axis | Self::Cylinder | Self::Circle)
    }

    /// A plane, or a circle's plane.
    fn planar(self) -> bool {
        matches!(self, Self::Plane | Self::Circle)
    }
}

/// A resolved reference: its kind, frame (in its component) and radius.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Geo {
    pub kind: GeoKind,
    pub frame: Transform,
    pub radius: f64,
}

/// Half a turn about the frame's x axis: z the other way.
fn turned_over(frame: &Transform) -> Transform {
    frame.after(&Transform::rotation([0.0; 3], [1.0, 0.0, 0.0], PI).expect("unit axis"))
}

/// A reference's kind and frame (the joint origin's frame, a circle's z
/// along the normal of a planar face it bounds), with its override.
pub(crate) fn resolve_geo(r: &mut impl Resolver, origin: &JointOrigin) -> Result<Geo, String> {
    let mut frame_now = origin_frame(r, &origin.geometry)?;
    let datum_kind = |d: Datum| match d {
        Datum::Plane(_) => GeoKind::Plane,
        Datum::Axis(_) => GeoKind::Axis,
        Datum::Point(_) => GeoKind::Point,
    };
    let (kind, radius) = match &origin.geometry {
        GeomRef::Origin(o) => (datum_kind(o.datum()), 0.0),
        GeomRef::Datum(uid) => (datum_kind(r.datum(*uid)?), 0.0),
        GeomRef::Face { body, face } => {
            let shape = r.shape(*body)?;
            let face = &crate::features::renamed::face_for(r, &shape, face)?;
            match r.kernel().face_geometry(&shape, face).map_err(|e| {
                crate::notice::about_place(
                    crate::notice::NoticePlace::face(face.to_string()),
                    &e.to_string(),
                )
            })? {
                SurfaceGeometry::Plane { .. } => (GeoKind::Plane, 0.0),
                SurfaceGeometry::Cylinder { radius, .. } => (GeoKind::Cylinder, radius),
                SurfaceGeometry::Sphere { radius, .. } => (GeoKind::Sphere, radius),
                SurfaceGeometry::Cone { .. } | SurfaceGeometry::Torus { .. } => {
                    (GeoKind::Axis, 0.0)
                }
                SurfaceGeometry::Other { kind } => {
                    return Err(crate::notice::placed(
                        format!("the face ({kind}) is no constraint's geometry"),
                        crate::notice::NoticePlace::face(face.to_string())
                            .in_body(&body.to_string()),
                        format!("face {face} ({kind}) is no constraint's geometry"),
                    ));
                }
            }
        }
        GeomRef::Edge { body, edge } => {
            let shape = r.shape(*body)?;
            let edge = &crate::features::renamed::edge_for(r, &shape, edge)?;
            match r.kernel().edge_geometry(&shape, edge).map_err(|e| {
                crate::notice::about_place(
                    crate::notice::NoticePlace::edge(edge.to_string()),
                    &e.to_string(),
                )
            })? {
                CurveGeometry::Circle { radius, .. } => {
                    // The circle's side: the outward normal of a planar face
                    // it bounds (a hole's rim on the top face looks up).
                    let z = frame_parts(&frame_now)[3];
                    for face in edge.faces() {
                        if let Ok(SurfaceGeometry::Plane { normal, .. }) =
                            r.kernel().face_geometry(&shape, face)
                            && let Some(n) = unit(normal)
                            && dot(n, z).abs() > 1.0 - 1e-9
                        {
                            if dot(n, z) < 0.0 {
                                frame_now = turned_over(&frame_now);
                            }
                            break;
                        }
                    }
                    (GeoKind::Circle, radius)
                }
                _ => (GeoKind::Axis, 0.0),
            }
        }
        GeomRef::SketchCurves { sketch, curves } => {
            let output = r.sketch(*sketch)?;
            match curves.first().and_then(|c| output.solved.curves.get(c)) {
                Some(Curve2::Circle { radius, .. } | Curve2::Arc { radius, .. }) => {
                    (GeoKind::Circle, *radius)
                }
                _ => (GeoKind::Axis, 0.0),
            }
        }
        GeomRef::FixedPlane { .. } => (GeoKind::Plane, 0.0),
        GeomRef::FixedAxis { .. } => (GeoKind::Axis, 0.0),
        GeomRef::Mount { .. } => (GeoKind::Plane, 0.0),
        GeomRef::Vertex { .. } | GeomRef::SketchPoint { .. } | GeomRef::FixedPoint(_) => {
            (GeoKind::Point, 0.0)
        }
        GeomRef::Body(_) => return Err(format!("{} is no constraint's geometry", origin.geometry)),
    };
    if let Some(o) = &origin.frame_override {
        frame_now = apply_override(&frame_now, o)?;
    }
    Ok(Geo {
        kind,
        frame: frame_now,
        radius,
    })
}

/// A row's values: its offset and angle, and its limits (min, max, rest)
/// when it has some.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct RowValues {
    pub offset: f64,
    pub angle: f64,
    pub flip: bool,
    pub lock_rotation: bool,
    pub limits: Option<[Option<f64>; 3]>,
}

impl RowValues {
    pub(crate) fn of(row: &ConstraintRow, params: &Parameters) -> Result<Self, String> {
        let value = |id: Option<&ParamId>| -> Result<Option<f64>, String> {
            id.map(|id| {
                params
                    .value(*id)
                    .ok_or_else(|| format!("parameter {} has no value", params.name(*id)))
            })
            .transpose()
        };
        let limits = if row.limited() || row.limits.as_ref().is_some_and(|l| l.rest.is_some()) {
            let l = row.limits.as_ref().expect("limits");
            Some([
                value(l.min.as_ref())?,
                value(l.max.as_ref())?,
                value(l.rest.as_ref())?,
            ])
        } else {
            None
        };
        Ok(Self {
            offset: value(row.offset.as_ref())?.unwrap_or(0.0),
            angle: value(row.angle.as_ref())?.unwrap_or(0.0),
            flip: row.flip,
            lock_rotation: row.lock_rotation,
            limits,
        })
    }
}

/// A row as the joint solver takes it: frames in the components of the
/// two sides (swapped when the row's b is the solver's a), the equations
/// and the value.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RowSpec {
    /// The solver's a is the row's b.
    pub swap: bool,
    pub local_a: Transform,
    pub local_b: Transform,
    pub parts: Vec<ConstraintPart>,
    pub value: Option<MotionSpec>,
}

/// A frame at the middle of two frames' origins, with the first's axes.
fn middle(a: &Transform, b: &Transform) -> Transform {
    Transform {
        linear: a.linear,
        translation: std::array::from_fn(|i| 0.5 * (a.translation[i] + b.translation[i])),
    }
}

/// The equations of a row of `kind` between `a` (with `a2`) and `b` (with
/// `b2`), or why the references do not fit the type.
pub(crate) fn row_spec(
    kind: ConstraintType,
    a: &Geo,
    a2: Option<&Geo>,
    b: &Geo,
    b2: Option<&Geo>,
    v: &RowValues,
) -> Result<RowSpec, String> {
    use ConstraintPart as P;
    use GeoKind as K;
    let (ka, kb) = (a.kind, b.kind);
    // The value: free between limits (held at rest), else fixed.
    let limited = v.limits.is_some();
    let value = |motion: Motion| {
        v.limits.map(|[min, max, rest]| MotionSpec {
            motion,
            min,
            max,
            target: rest,
            start: None,
        })
    };
    let amount = if limited {
        Amount::Value
    } else {
        Amount::Fixed(v.offset)
    };
    let zero = !limited && v.offset.abs() < 1e-12;
    let spec = |swap: bool, parts: Vec<ConstraintPart>, value: Option<MotionSpec>| {
        let (fa, fb) = if swap {
            (b.frame, a.frame)
        } else {
            (a.frame, b.frame)
        };
        RowSpec {
            swap,
            local_a: fa,
            local_b: fb,
            parts,
            value,
        }
    };
    let facing = if v.flip { Sense::Same } else { Sense::Opposite };
    let pair = format!("{} and {}", ka.as_str(), kb.as_str());
    match kind {
        ConstraintType::Align => {
            let value = value(Motion::Tz);
            if ka.planar() && kb.planar() && (ka == K::Plane || kb == K::Plane) {
                return Ok(spec(
                    false,
                    vec![P::OriginZ(amount), P::Parallel(facing)],
                    value,
                ));
            }
            if ka.pointlike() && kb.pointlike() {
                let parts = if zero {
                    vec![P::OriginX, P::OriginY, P::OriginZ(Amount::Fixed(0.0))]
                } else {
                    vec![P::Distance(amount)]
                };
                return Ok(spec(false, parts, value));
            }
            if ka.axial() && kb.axial() {
                let parts = if zero {
                    vec![P::OriginX, P::OriginY, P::Parallel(Sense::Either)]
                } else {
                    vec![P::Parallel(Sense::Either), P::Radial(amount)]
                };
                return Ok(spec(false, parts, value));
            }
            // A point on a plane or an axis, an axis on a plane: the plane
            // or axis is the solver's b.
            let on = |swap: bool, plane_or_axis: GeoKind, other: GeoKind| {
                let parts = match (other.pointlike(), plane_or_axis) {
                    (true, K::Plane) => vec![P::OriginZ(amount)],
                    (true, _) if zero => vec![P::OriginX, P::OriginY],
                    (true, _) => vec![P::Radial(amount)],
                    (false, _) => vec![P::Perpendicular, P::OriginZ(amount)],
                };
                spec(swap, parts, value)
            };
            match (ka, kb) {
                (p, K::Plane | K::Axis | K::Cylinder) if p.pointlike() => Ok(on(false, kb, ka)),
                (K::Plane | K::Axis | K::Cylinder, p) if p.pointlike() => Ok(on(true, ka, kb)),
                (p, K::Plane) if p.axial() => Ok(on(false, kb, ka)),
                (K::Plane, p) if p.axial() => Ok(on(true, ka, kb)),
                _ => Err(format!("an align row does not take a {pair}")),
            }
        }
        ConstraintType::Angle => {
            if !(ka == K::Plane || ka.axial()) || !(kb == K::Plane || kb.axial()) {
                return Err(format!("an angle row takes planes and axes, not a {pair}"));
            }
            let mut s = spec(false, Vec::new(), value(Motion::Rz));
            if v.flip {
                s.local_a = turned_over(&s.local_a);
            }
            s.parts = if limited {
                vec![P::Angle(Amount::Value)]
            } else if v.angle.abs() < 1e-9 {
                vec![P::Parallel(Sense::Same)]
            } else if (v.angle - PI).abs() < 1e-9 {
                vec![P::Parallel(Sense::Opposite)]
            } else {
                vec![P::Angle(Amount::Fixed(v.angle))]
            };
            Ok(s)
        }
        ConstraintType::Center => {
            let point = match a2 {
                Some(a2) => middle(&a.frame, &a2.frame),
                None => a.frame,
            };
            let (frame_b, parts) = match b2 {
                Some(b2) => {
                    if kb != K::Plane || b2.kind != K::Plane {
                        return Err("a center between two faces takes two planar faces".to_owned());
                    }
                    let (z1, z2) = (frame_parts(&b.frame)[3], frame_parts(&b2.frame)[3]);
                    if dot(z1, z2).abs() < 1.0 - 1e-6 {
                        return Err("the two faces of a center are not parallel".to_owned());
                    }
                    (
                        middle(&b.frame, &b2.frame),
                        vec![P::OriginZ(Amount::Fixed(0.0))],
                    )
                }
                None => (b.frame, vec![P::OriginX, P::OriginY]),
            };
            Ok(RowSpec {
                swap: false,
                local_a: point,
                local_b: frame_b,
                parts,
                value: None,
            })
        }
        ConstraintType::Tangent => {
            let side = if v.flip { -1.0 } else { 1.0 };
            // Outside: the sum of the radii; inside: their difference.
            let apart = |r1: f64, r2: f64| {
                if v.flip { (r1 - r2).abs() } else { r1 + r2 }
            };
            let fixed = Amount::Fixed;
            match (ka, kb) {
                (K::Plane, K::Plane) => Ok(spec(
                    false,
                    vec![P::OriginZ(fixed(0.0)), P::Parallel(facing)],
                    None,
                )),
                (K::Cylinder, K::Plane) | (K::Plane, K::Cylinder) => {
                    let (swap, r) = if kb == K::Plane {
                        (false, a.radius)
                    } else {
                        (true, b.radius)
                    };
                    Ok(spec(
                        swap,
                        vec![P::Perpendicular, P::OriginZ(fixed(side * r))],
                        None,
                    ))
                }
                (K::Sphere, K::Plane) | (K::Plane, K::Sphere) => {
                    let (swap, r) = if kb == K::Plane {
                        (false, a.radius)
                    } else {
                        (true, b.radius)
                    };
                    Ok(spec(swap, vec![P::OriginZ(fixed(side * r))], None))
                }
                (K::Cylinder, K::Cylinder) => {
                    let d = apart(a.radius, b.radius);
                    let parts = if d < 1e-9 {
                        vec![P::OriginX, P::OriginY, P::Parallel(Sense::Either)]
                    } else {
                        vec![P::Parallel(Sense::Either), P::Radial(fixed(d))]
                    };
                    Ok(spec(false, parts, None))
                }
                (K::Sphere, K::Sphere) | (K::Sphere, K::Cylinder) | (K::Cylinder, K::Sphere) => {
                    let d = apart(a.radius, b.radius);
                    // The cylinder (if any) is the solver's b.
                    let swap = ka == K::Cylinder;
                    let around_axis = ka == K::Cylinder || kb == K::Cylinder;
                    let parts = match (around_axis, d < 1e-9) {
                        (true, true) => vec![P::OriginX, P::OriginY],
                        (true, false) => vec![P::Radial(fixed(d))],
                        (false, true) => {
                            vec![P::OriginX, P::OriginY, P::OriginZ(fixed(0.0))]
                        }
                        (false, false) => vec![P::Distance(fixed(d))],
                    };
                    Ok(spec(swap, parts, None))
                }
                _ => Err(format!(
                    "a tangent row takes planar, cylindrical and spherical faces, not a {pair}"
                )),
            }
        }
        ConstraintType::Insert | ConstraintType::InsertAxis => {
            if ka != K::Circle {
                return Err(format!(
                    "an insert's a is a circle (a circular edge or a sketch circle), not a {}",
                    ka.as_str()
                ));
            }
            let lock = v.lock_rotation.then_some(P::Twist(v.angle));
            let mut parts = vec![P::OriginX, P::OriginY];
            if kind == ConstraintType::Insert {
                if kb != K::Circle {
                    return Err(format!(
                        "an insert's b is a circle, not a {} (insert_axis takes a hole's face)",
                        kb.as_str()
                    ));
                }
                parts.extend([P::OriginZ(Amount::Fixed(v.offset)), P::Parallel(facing)]);
            } else {
                if !kb.axial() {
                    return Err(format!(
                        "an axis insert's b is a cylindrical face or a circle, not a {}",
                        kb.as_str()
                    ));
                }
                parts.push(P::Parallel(Sense::Either));
            }
            parts.extend(lock);
            Ok(spec(false, parts, None))
        }
    }
}

/// Whether references fit a type (with its default values), else why not.
pub fn constraint_fits(
    kind: ConstraintType,
    a: &Geo,
    a2: Option<&Geo>,
    b: &Geo,
    b2: Option<&Geo>,
) -> Result<(), String> {
    row_spec(kind, a, a2, b, b2, &RowValues::default()).map(|_| ())
}

/// The types a pair of references fits, the likeliest first: two circles
/// an insert, a circle and a cylinder an axis insert, a cylinder or sphere
/// with a plane, or two spheres, a tangent; else an align.
pub fn fitting_types(a: &Geo, b: &Geo) -> Vec<ConstraintType> {
    use ConstraintType as T;
    use GeoKind as K;
    let first = match (a.kind, b.kind) {
        (K::Circle, K::Circle) => T::Insert,
        (K::Circle, K::Cylinder) => T::InsertAxis,
        (K::Cylinder | K::Sphere, K::Plane)
        | (K::Plane, K::Cylinder | K::Sphere)
        | (K::Sphere, K::Sphere | K::Cylinder)
        | (K::Cylinder, K::Sphere) => T::Tangent,
        _ => T::Align,
    };
    let values = RowValues::default();
    std::iter::once(first)
        .chain(T::ALL.into_iter().filter(|t| *t != first))
        .filter(|t| row_spec(*t, a, None, b, None, &values).is_ok())
        .collect()
}

/// What a row's references measure where they are (frames in one
/// component's coordinates): the offset of an align (the height above a
/// plane, a distance), the angle between normals or axes, and the turn
/// about b's z of an insert.
pub fn measures(a: &Geo, b: &Geo) -> BTreeMap<&'static str, f64> {
    let [oa, xa, _, za] = frame_parts(&a.frame);
    let [ob, xb, yb, zb] = frame_parts(&b.frame);
    let d: Vec3 = std::array::from_fn(|i| oa[i] - ob[i]);
    let mut out = BTreeMap::new();
    let along = |v: Vec3| dot(v, zb);
    if b.kind == GeoKind::Plane || b.kind == GeoKind::Circle {
        out.insert("offset", along(d));
    } else if a.kind.pointlike() && b.kind.pointlike() {
        out.insert("offset", dot(d, d).sqrt());
    }
    let c = dot(za, zb).clamp(-1.0, 1.0);
    out.insert("angle", c.acos());
    out.insert("twist", dot(xa, yb).atan2(dot(xa, xb)));
    out
}

// Recompute.

/// One row after recompute.
#[derive(Debug, Clone, PartialEq)]
pub struct RowResult {
    pub kind: ConstraintType,
    /// The paths of the row's a and b.
    pub a: Vec<OccurrenceUid>,
    pub b: Vec<OccurrenceUid>,
    /// It holds at the marker.
    pub holds: bool,
    /// Its value at the marker (an offset or angle between limits).
    pub value: Option<f64>,
}

/// A constraint set after recompute.
#[derive(Debug, Clone, PartialEq)]
pub struct ConstraintSetResult {
    pub component: crate::ids::ComponentUid,
    pub state: JointState,
    pub rows: Vec<RowResult>,
    pub placements: Vec<crate::features::PlacementChange>,
}

/// Lengths within this hold a constraint, mm.
pub(crate) const LINEAR: f64 = 1e-6;
/// Directions within this hold a constraint, radians.
#[doc(hidden)]
pub const ANGULAR: f64 = 1e-6;

/// The solver's constraint for frames where they are.
pub(crate) fn rigid_constraint(
    frame_a: &Transform,
    frame_b: &Transform,
    parts: &[ConstraintPart],
    value: Option<&MotionSpec>,
) -> RigidConstraint {
    let pose = |t: &Transform| Pose {
        rotation: t.linear,
        translation: t.translation,
    };
    RigidConstraint {
        a: None,
        frame_a: pose(frame_a),
        b: None,
        frame_b: pose(frame_b),
        parts: parts.to_vec(),
        value: value.map(|s| JointMotion {
            freedom: if s.motion.is_rotation() {
                Freedom::Rz
            } else {
                Freedom::Tz
            },
            min: s.min,
            max: s.max,
            target: s.target,
        }),
    }
}

/// Whether a constraint holds between frames, and its value there.
pub(crate) fn check(
    frame_a: &Transform,
    frame_b: &Transform,
    parts: &[ConstraintPart],
    value: Option<&MotionSpec>,
) -> (bool, Option<f64>) {
    let c = rigid_constraint(frame_a, frame_b, parts, value);
    let (d, angle) = c.error();
    let scale = 1.0
        + frame_a
            .translation
            .iter()
            .chain(&frame_b.translation)
            .fold(0.0_f64, |m, v| m.max(v.abs()));
    (d <= LINEAR * scale && angle <= ANGULAR, c.measured())
}

// A constraint row's references before the row is added (mitcad#778).

/// One reference of a constraint row before the row is added
/// ([`Document::constraint_row`](crate::document::Document::constraint_row)).
#[derive(Debug, Clone, Copy)]
pub struct RowReference<'a> {
    /// The occurrence path from the row's component: uids (`O1/O4`) or
    /// names (`Arm:1/Pin:2`); empty for the component's own geometry.
    pub occurrence: &'a str,
    /// The geometry picked on the path's last occurrence.
    pub geometry: &'a GeomRef,
    /// The frame's override, as a joint origin has it.
    pub frame_override: Option<crate::features::joint::FrameOverride>,
}

/// What a constraint row's references are before the row is added
/// ([`Document::constraint_row`](crate::document::Document::constraint_row);
/// the api's `constraint_row` query and the import's assembly
/// relationships ask for it).
#[derive(Debug, Clone)]
pub struct RowCheck {
    /// The references `a` and `b` resolved, their frames in the row's
    /// component.
    pub a: Geo,
    pub b: Geo,
    /// The types `a` and `b` fit, the likeliest first ([`fitting_types`]).
    pub types: Vec<ConstraintType>,
    /// What they measure where they are ([`measures`]).
    pub measures: BTreeMap<&'static str, f64>,
    /// With a type to check: whether the references fit it, else why not
    /// ([`constraint_fits`]).
    pub fits: Option<Result<(), String>>,
}
