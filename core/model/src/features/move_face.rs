// SPDX-License-Identifier: MIT
//! Move face: faces of one body moved by a rigid motion, the motion given as
//! a move's (a matrix, distances, a rotation about an axis, point to point;
//! [`MoveSpec`]), or along their normal up to an object (mitcad#2548:
//! `to_object`, a plane, a flat face, a point or a line, or the first flat
//! face of a body in front of them). Each face keeps its surface, moved; the
//! neighbouring faces stay on their surfaces and extend or shorten up to
//! the moved ones. The faces keep their names. Planar faces moved along
//! their normal may cross the body's other faces (a raised block's end
//! moved flush with the base under it, or past it); other motions that
//! would change the body's faces (a face that would vanish or split) fail
//! (mitcad#343: the `.f3d` import's moved faces).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::face_refs::{add_faces, check_faces, kernel_error, require_faces};
use super::mapping::map_params;
use super::moves::MoveSpec;
use super::reference::{Across, ExtentObject, LINE_ANGLE};
use super::{
    BodyChange, CheckContext, EvalContext, Evaluate, FeatureInfo, FeatureOutput, References,
};
use crate::datum::SurfaceGeometry;
use crate::ids::BodyUid;
use crate::kernel::{Kernel, Plane, Target};
use crate::parameters::ParamId;
use crate::topo::FaceName;
use crate::transform::Transform;
use mitcad_vec3::{cross, dot, norm, scale as scaled, unit};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MoveFaceDef<P = ParamId> {
    pub body: BodyUid,
    pub faces: Vec<FaceName>,
    /// The motion. Left out (no motion) with `to_object`.
    #[serde(
        default = "MoveSpec::still",
        skip_serializing_if = "MoveSpec::is_still"
    )]
    pub transform: MoveSpec<P>,
    /// Along the faces' normal up to this object instead (mitcad#2548):
    /// the plane of a plane, a flat face, a point or a line parallel to the
    /// faces, or of a body's first flat face parallel to them in front of
    /// them (out of the material). Found again on every recompute, so the
    /// stop follows an upstream edit of the object.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to_object: Option<Box<ExtentObject>>,
}

impl<P> MoveFaceDef<P> {
    pub const TYPE: &'static str = "move_face";
    pub const BASE_NAME: &'static str = "MoveFace";

    map_params! {
        MoveFaceDef {
            body: copy,
            faces: clone,
            transform: map,
            to_object: clone,
        }
    }
}

impl FeatureInfo for MoveFaceDef {
    fn references(&self) -> References {
        let mut references = References::default();
        references.body(self.body);
        add_faces(&mut references, &self.faces);
        self.transform.add_references(&mut references);
        if let Some(object) = &self.to_object {
            object.add_references(&mut references);
        }
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        ctx.body(self.body)?;
        check_faces(ctx, &self.faces, "faces to move")?;
        if let Some(object) = &self.to_object {
            if !self.transform.is_still() {
                return Err(BOTH_MOTIONS.to_owned());
            }
            if matches!(**object, ExtentObject::Body { through: true, .. }) {
                return Err(THROUGH_BODY.to_owned());
            }
            return object.check(ctx);
        }
        self.transform.check(ctx)
    }
}

impl<K: Kernel> Evaluate<K> for MoveFaceDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let given = match &self.to_object {
            Some(_) => None,
            None => {
                let motion = self.transform.resolve(ctx, None)?;
                if !motion.is_rigid() {
                    return Err("the faces' motion must be a rotation and a translation".to_owned());
                }
                if motion.is_identity() {
                    return Err("the faces' motion moves nothing".to_owned());
                }
                Some(motion)
            }
        };
        let shape = ctx.surface_body(self.body)?;
        let faces = require_faces(ctx, self.body, &shape, &self.faces)?;
        let motion = match (given, self.to_object.as_deref()) {
            (Some(motion), _) => motion,
            (None, Some(object)) => up_to_object(ctx, &shape, &faces, object)?,
            (None, None) => return Err("the faces' motion moves nothing".to_owned()),
        };
        let moved = ctx
            .kernel
            .move_faces(ctx.uid, &shape, &faces, &motion)
            .map_err(kernel_error)?;
        Ok(FeatureOutput {
            changes: vec![BodyChange::Set(self.body, moved)],
            ..FeatureOutput::default()
        })
    }
}

/// `to_object` with a motion of its own.
const BOTH_MOTIONS: &str = "a move up to an object takes no other motion";
/// `to_object` through a body.
const THROUGH_BODY: &str = "faces move up to a body, not through it";
/// A face to move up to an object that is not flat.
const NOT_FLAT: &str = "only flat faces move up to an object: a face to move is not flat";
/// Faces to move up to an object that do not lie in one plane.
const NOT_ONE_PLANE: &str =
    "the faces to move up to an object must lie in one plane, facing the same way";
/// An object whose plane is not parallel to the faces.
const NOT_PARALLEL: &str =
    "the object is not parallel to the faces to move, so it gives them no place to stop";
/// A face object that is not flat.
const TARGET_NOT_FLAT: &str =
    "the face picked as the object is not flat, so it gives the moved faces no place to stop";
/// A body without a stop in front of the faces.
const NO_FACE_AHEAD: &str = "the body picked as the object has no flat face parallel to the faces \
     to move in front of them; pick one of its faces, a vertex or a plane instead";
/// Faces already at the object.
const AT_OBJECT: &str = "the faces already lie at the object, so they do not move";

/// How far apart two parallel planes may lie and still be one level (mm),
/// as the kernel lands a moved face on another's level.
const LEVEL: f64 = 1e-6;

/// Whether two directions are parallel (either way), within
/// [`LINE_ANGLE`].
fn parallel(a: [f64; 3], b: [f64; 3]) -> bool {
    match (unit(a), unit(b)) {
        (Some(a), Some(b)) => norm(cross(a, b)) <= LINE_ANGLE.sin(),
        _ => false,
    }
}

/// The translation that takes the faces (all flat, in one plane) along
/// their normal to where `object` stops them (mitcad#2548): the level of a
/// plane, a flat face, a point or a line parallel to them, or a body's
/// nearest flat face parallel to them in front of them (out of the
/// material).
fn up_to_object<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    shape: &K::Shape,
    faces: &[FaceName],
    object: &ExtentObject,
) -> Result<Transform, String> {
    let mut first: Option<Plane> = None;
    for face in faces {
        let plane = ctx
            .kernel
            .face_plane(shape, face)
            .map_err(|_| NOT_FLAT.to_owned())?;
        match &first {
            None => first = Some(plane),
            Some(own) => {
                let normal = unit(own.normal).ok_or(NOT_FLAT)?;
                let other = unit(plane.normal).ok_or(NOT_FLAT)?;
                let level = dot(plane.origin, normal) - dot(own.origin, normal);
                if dot(normal, other) <= 0.0 || !parallel(normal, other) || level.abs() > LEVEL {
                    return Err(NOT_ONE_PLANE.to_owned());
                }
            }
        }
    }
    let own = first.ok_or("there are no faces to move")?;
    let normal = unit(own.normal).ok_or(NOT_FLAT)?;
    let start = dot(own.origin, normal);
    // A point or a line stands for the plane through it parallel to the
    // faces, as for an extrusion of a body face.
    let across = Across {
        normal,
        lies: "in a plane parallel to the faces to move",
    };
    let target = object.resolve_across(ctx, &BTreeMap::new(), Some(across))?;
    let stop = match target {
        Target::Plane(plane) => {
            if !parallel(plane.normal, normal) {
                return Err(NOT_PARALLEL.to_owned());
            }
            dot(plane.origin, normal)
        }
        Target::Face { body, face, .. } => {
            let plane = ctx
                .kernel
                .face_plane(&body, &face)
                .map_err(|_| TARGET_NOT_FLAT.to_owned())?;
            if !parallel(plane.normal, normal) {
                return Err(NOT_PARALLEL.to_owned());
            }
            dot(plane.origin, normal)
        }
        Target::Body { body, .. } => body_stop(ctx.kernel, &body, normal, start)?,
    };
    let distance = stop - start;
    if distance.abs() <= LEVEL {
        return Err(AT_OBJECT.to_owned());
    }
    Ok(Transform::translation(
        scaled(normal, distance).map(|v| v + 0.0),
    ))
}

/// The level along `normal` of the body's nearest flat face parallel to the
/// moved faces (at `start`) in front of them.
fn body_stop<K: Kernel>(
    kernel: &K,
    body: &K::Shape,
    normal: [f64; 3],
    start: f64,
) -> Result<f64, String> {
    let surfaces = kernel.face_geometries(body).map_err(kernel_error)?;
    let mut nearest: Option<f64> = None;
    for (_, surface) in surfaces {
        let SurfaceGeometry::Plane { origin, normal: n } = surface else {
            continue;
        };
        if !parallel(n, normal) {
            continue;
        }
        let ahead = dot(origin, normal) - start;
        if ahead > LEVEL && nearest.is_none_or(|best| ahead < best) {
            nearest = Some(ahead);
        }
    }
    nearest
        .map(|ahead| start + ahead)
        .ok_or_else(|| NO_FACE_AHEAD.to_owned())
}
