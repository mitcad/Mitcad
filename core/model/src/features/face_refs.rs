// SPDX-License-Identifier: MIT
//! What the features that work on faces share (F2: fillet, chamfer, shell,
//! draft, offset, delete and replace face, split body and split face):
//! named faces, and the tool a feature works against. A tool is a
//! [`GeomRef`] (a plane, a face, a body or sketch curves, [`Want::Tool`]);
//! a plane tool can be moved along its normal by the feature's `offset`,
//! and sketch curves are swept along the feature's `direction` (the sketch
//! normal by default).
//!
//! A feature fails with a message starting [`UNSUPPORTED`] when the input
//! asks for something the geometry kernel cannot do (an .f3d option without
//! an OCCT counterpart); the .f3d importer then falls back to the body
//! the file stores.

use serde_json::{Map, Value};

use super::geom_ref::{GeomRef, Want};
use super::{CheckContext, EvalContext, References};
use crate::datum::{Vec3, add, scale, unit};
use crate::ids::{BodyUid, EntityUid};
use crate::kernel::{Kernel, KernelError, ToolInput};
use crate::parameters::ParamId;
use crate::profile::{ProfileRegion, SketchFrame};
use crate::recompute::FeatureStatus;
use crate::topo::FaceName;

/// The start of the message of a feature that failed because the geometry
/// kernel lacks an option.
pub const UNSUPPORTED: &str = "unsupported: ";

/// A kernel error as a feature error; unsupported operations keep the
/// [`UNSUPPORTED`] mark.
pub(crate) fn kernel_error(error: KernelError) -> String {
    match error {
        KernelError::Unsupported(what) => {
            format!("{UNSUPPORTED}the geometry kernel does not support {what}")
        }
        KernelError::Failed(message) => message,
    }
}

/// Checks a face feature's tool; `what` names its role and `want` what it
/// may be ([`Want::Plane`] for planes and planar faces, [`Want::Tool`]).
pub(crate) fn check_tool(
    ctx: &CheckContext<'_>,
    tool: &GeomRef,
    want: Want,
    offset: bool,
    direction: Option<Vec3>,
    what: &str,
) -> Result<(), String> {
    tool.check(ctx, want).map_err(|e| format!("{what}: {e}"))?;
    let planar = !matches!(
        tool,
        GeomRef::Face { .. } | GeomRef::Body(_) | GeomRef::SketchCurves { .. }
    );
    if offset && !planar {
        return Err(format!("{what}: only a plane can be offset, not {tool}"));
    }
    match direction {
        Some(_) if !matches!(tool, GeomRef::SketchCurves { .. }) => Err(format!(
            "{what}: a direction applies to sketch curves, not {tool}"
        )),
        Some(d) if unit(d).is_none() => Err(format!("{what}: the direction is zero")),
        _ => Ok(()),
    }
}

/// A tool evaluated: the shapes and sketch data it stands for.
pub(crate) enum Resolved<S> {
    Plane {
        origin: [f64; 3],
        normal: [f64; 3],
    },
    Face {
        shape: S,
        face: FaceName,
    },
    Body {
        shape: S,
    },
    Curves {
        frame: SketchFrame,
        regions: Vec<ProfileRegion>,
        curves: Vec<EntityUid>,
        direction: [f64; 3],
    },
}

impl<S> Resolved<S> {
    pub fn input(&self) -> ToolInput<'_, S> {
        match self {
            Self::Plane { origin, normal } => ToolInput::Plane {
                origin: *origin,
                normal: *normal,
            },
            Self::Face { shape, face } => ToolInput::Face { shape, face },
            Self::Body { shape } => ToolInput::Body { shape },
            Self::Curves {
                frame,
                regions,
                curves,
                direction,
            } => ToolInput::Curves {
                frame: *frame,
                regions,
                curves,
                direction: *direction,
            },
        }
    }
}

/// Evaluates a tool: a face stays a face (the kernel needs its shape and
/// side), other planes become a plane moved by `offset` along its normal.
pub(crate) fn resolve_tool<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    tool: &GeomRef,
    offset: Option<ParamId>,
    direction: Option<Vec3>,
) -> Result<Resolved<K::Shape>, String> {
    Ok(match tool {
        GeomRef::Face { body, face } => {
            let shape = ctx.body(*body)?;
            require_faces(ctx, *body, &shape, std::slice::from_ref(face))?;
            Resolved::Face {
                shape,
                face: face.clone(),
            }
        }
        GeomRef::Body(body) => Resolved::Body {
            shape: ctx.body(*body)?,
        },
        GeomRef::SketchCurves { sketch, curves } => {
            let output = ctx.sketch(*sketch)?;
            let regions: Vec<ProfileRegion> = output
                .regions
                .iter()
                .filter(|region| {
                    curves.is_empty()
                        || region
                            .loops
                            .iter()
                            .flat_map(|l| &l.segments)
                            .any(|s| s.key.curve.entity().is_some_and(|c| curves.contains(&c)))
                })
                .cloned()
                .collect();
            if regions.is_empty() {
                return Err(format!(
                    "{} has none of the curves",
                    ctx.feature_name(*sketch)
                ));
            }
            let direction = match direction {
                Some(d) => unit(d).ok_or("the direction is zero")?,
                None => output.frame.normal(),
            };
            Resolved::Curves {
                frame: output.frame,
                regions,
                curves: curves.clone(),
                direction,
            }
        }
        plane => {
            let plane = ctx.plane(plane)?;
            let normal = plane.normal();
            let offset = match offset {
                Some(id) => ctx.param(id)?,
                None => 0.0,
            };
            Resolved::Plane {
                // `+ 0.0` turns -0 into 0.
                origin: add(plane.origin, scale(normal, offset)).map(|v| v + 0.0),
                normal: normal.map(|v| v + 0.0),
            }
        }
    })
}

/// Reads F2's earlier tool forms, which kept the offset of an origin plane
/// and the direction of sketch curves inside the reference and left out the
/// body of a face on the feature's own body: the offset and the direction
/// move to the feature, the face gets the feature's body. Runs on a feature
/// definition before it is read.
pub(crate) fn migrate_tool(def: &mut Map<String, Value>) {
    let field = match def.get("type").and_then(Value::as_str) {
        Some("draft") => "plane",
        Some("replace_face") => "target",
        Some("split_body" | "split_face") => "tool",
        _ => return,
    };
    let own_body = def.get("body").cloned();
    let Some(Value::Object(tool)) = def.get_mut(field) else {
        return;
    };
    let (offset, direction) = match tool.get("type").and_then(Value::as_str) {
        Some("origin_plane") => (tool.remove("offset"), None),
        Some("sketch") => (None, tool.remove("direction")),
        Some("face") => {
            if !tool.contains_key("body")
                && let Some(body) = own_body
            {
                tool.insert("body".to_owned(), body);
            }
            (None, None)
        }
        _ => (None, None),
    };
    for (key, value) in [("offset", offset), ("direction", direction)] {
        if let Some(value) = value.filter(|v| !v.is_null()) {
            def.entry(key).or_insert(value);
        }
    }
}

/// The offset parameter of a tool, slot `offset`.
pub(crate) fn map_offset<P, Q, E>(
    offset: &Option<P>,
    f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
) -> Result<Option<Q>, E> {
    offset.as_ref().map(|o| f("offset", o)).transpose()
}

/// Adds the features named in the faces.
pub(crate) fn add_faces<'a>(
    references: &mut References,
    faces: impl IntoIterator<Item = &'a FaceName>,
) {
    for face in faces {
        references.features.extend(face.features());
    }
}

/// The faces are listed once and their features come before the feature.
pub(crate) fn check_faces(
    ctx: &CheckContext<'_>,
    faces: &[FaceName],
    what: &str,
) -> Result<(), String> {
    if faces.is_empty() {
        return Err(format!("no {what} selected"));
    }
    for (i, face) in faces.iter().enumerate() {
        if faces[..i].contains(face) {
            return Err(format!("face {face} is listed more than once"));
        }
        for feature in face.features() {
            ctx.feature(feature)
                .map_err(|e| format!("face {face}: {e}"))?;
        }
    }
    Ok(())
}

/// Checks that every face resolves on the shape, explaining a missing one
/// as [`EvalContext::missing_edge`] does for edges. Kernels without face
/// lookup skip the check.
pub(crate) fn require_faces<K: Kernel>(
    ctx: &EvalContext<'_, K>,
    body: BodyUid,
    shape: &K::Shape,
    faces: &[FaceName],
) -> Result<(), String> {
    for face in faces {
        match ctx.kernel.count_faces(shape, face) {
            Ok(0) => return Err(missing_face(ctx, body, face)),
            Ok(_) | Err(KernelError::Unsupported(_)) => {}
            Err(error) => return Err(error.to_string()),
        }
    }
    Ok(())
}

fn missing_face<K: Kernel>(ctx: &EvalContext<'_, K>, body: BodyUid, face: &FaceName) -> String {
    let count = |shape: &K::Shape| ctx.kernel.count_faces(shape, face).unwrap_or(0);
    let mut existed = false;
    let mut lost_after = None;
    for result in ctx.env.history {
        let Some(shape) = result.shape_set(body) else {
            continue;
        };
        let present = count(shape) > 0;
        if existed && !present {
            lost_after = Some(result.uid);
        }
        existed = present;
    }
    if let Some(feature) = lost_after {
        return format!(
            "face {face} no longer exists after {}",
            ctx.feature_name(feature)
        );
    }
    for feature in face.features() {
        let state = match ctx.env.history.iter().find(|r| r.uid == feature) {
            Some(r) => match r.status {
                FeatureStatus::Ok => continue,
                FeatureStatus::Failed(_) => "failed",
                FeatureStatus::Suppressed => "is suppressed",
                FeatureStatus::RolledBack => "is rolled back",
            },
            None => continue,
        };
        return format!(
            "face {face} does not exist because {} {state}",
            ctx.feature_name(feature)
        );
    }
    format!("the body has no face {face}")
}
