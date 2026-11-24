// SPDX-License-Identifier: MIT
//! Move face: faces of one body moved by a rigid motion, the motion given as
//! a move's (a matrix, distances, a rotation about an axis, point to point;
//! [`MoveSpec`]). Each face keeps its surface, moved; the neighbouring
//! faces stay on their surfaces and extend or shorten up to the moved ones.
//! The faces keep their names. A motion that would change the body's faces
//! (a face that would vanish or split) fails (mitcad#343: the `.f3d`
//! import's moved faces).

use serde::{Deserialize, Serialize};

use super::face_refs::{add_faces, check_faces, kernel_error, require_faces};
use super::mapping::map_params;
use super::moves::MoveSpec;
use super::{
    BodyChange, CheckContext, EvalContext, Evaluate, FeatureInfo, FeatureOutput, References,
};
use crate::ids::BodyUid;
use crate::kernel::Kernel;
use crate::parameters::ParamId;
use crate::topo::FaceName;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MoveFaceDef<P = ParamId> {
    pub body: BodyUid,
    pub faces: Vec<FaceName>,
    pub transform: MoveSpec<P>,
}

impl<P> MoveFaceDef<P> {
    pub const TYPE: &'static str = "move_face";
    pub const BASE_NAME: &'static str = "MoveFace";

    map_params! {
        MoveFaceDef {
            body: copy,
            faces: clone,
            transform: map,
        }
    }
}

impl FeatureInfo for MoveFaceDef {
    fn references(&self) -> References {
        let mut references = References::default();
        references.body(self.body);
        add_faces(&mut references, &self.faces);
        self.transform.add_references(&mut references);
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        ctx.body(self.body)?;
        check_faces(ctx, &self.faces, "faces to move")?;
        self.transform.check(ctx)
    }
}

impl<K: Kernel> Evaluate<K> for MoveFaceDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let motion = self.transform.resolve(ctx, None)?;
        if !motion.is_rigid() {
            return Err("the faces' motion must be a rotation and a translation".to_owned());
        }
        if motion.is_identity() {
            return Err("the faces' motion moves nothing".to_owned());
        }
        let shape = ctx.surface_body(self.body)?;
        let faces = require_faces(ctx, self.body, &shape, &self.faces)?;
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
