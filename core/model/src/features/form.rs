// SPDX-License-Identifier: MIT
//! Form (mitcad#1757): a body whose surface is the Catmull-Clark limit
//! surface of an editable control cage, as an exact B-rep: a solid when the
//! cage is closed, a surface body when it is open. The cage is the
//! feature's stored data ([`Cage`], `mitcad_subdiv`); its edits are the
//! `form.*` commands, each one edit of the definition, and the features
//! after it recompute.
//!
//! The limit surface comes as bicubic patches (`mitcad_subdiv::patch`):
//! exact on regular quads of the once-subdivided cage, interpolated limit
//! points elsewhere, the patches meeting without gaps. The kernel sews them
//! ([`Kernel::patch_body`]). The faces are named per cage face and corner:
//! `<feature>:cage(<face id>)#<corner>`, on a symmetric cage's mirrored half
//! `<feature>:mirror(<face id>)#<corner>`, so a reference to a cage face
//! (`F7:cage(3)`) finds all its patches, and the names stay while the cage
//! is edited (a face keeps its id through moves, extrusions and the edge
//! loops that split it; a split's second half gets a new id).
//!
//! A closed Form joins, cuts or intersects bodies like a primitive, or
//! makes a new body; an open one makes a new surface body only.

use std::marker::PhantomData;

use serde::{Deserialize, Serialize};

use super::bodies::{BodySet, apply_tool, boolean_op, verb};
use super::mapping::map_params;
use super::{
    BodyChange, CheckContext, EvalContext, Evaluate, FeatureInfo, FeatureOutput, Operation,
    References, ToolUse,
};
use crate::ids::{BodyUid, FeatureUid};
use crate::kernel::{Kernel, PatchBodySpec, SurfacePatch};
use crate::parameters::ParamId;
use crate::topo::{FaceName, RoleKey};
use mitcad_subdiv::{Cage, Patch};

/// How far apart the patches' shared boundaries may be when the kernel sews
/// them, mm. They are the same curves to rounding (`mitcad_subdiv::patch`).
pub const SEWING_TOLERANCE: f64 = 1e-6;

fn new_body() -> Operation {
    Operation::NewBody
}

fn is_new_body(operation: &Operation) -> bool {
    *operation == Operation::NewBody
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormDef<P = ParamId> {
    /// The control cage, mm in the component's coordinates.
    pub cage: Cage,
    #[serde(default = "new_body", skip_serializing_if = "is_new_body")]
    pub operation: Operation,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub participants: Vec<BodyUid>,
    /// Has no values; `P` only matches the other feature types.
    #[serde(skip)]
    pub values: PhantomData<P>,
}

impl<P> FormDef<P> {
    pub const TYPE: &'static str = "form";
    pub const BASE_NAME: &'static str = "Form";

    pub fn new(cage: Cage, operation: Operation, participants: Vec<BodyUid>) -> Self {
        Self {
            cage,
            operation,
            participants,
            values: PhantomData,
        }
    }

    map_params! {
        FormDef {
            cage: clone,
            operation: copy,
            participants: clone,
            values: phantom,
        }
    }
}

/// The name of the face a patch becomes (see the module).
pub fn patch_face(feature: FeatureUid, face: i64, corner: u32) -> FaceName {
    let role = if face < 0 { "mirror" } else { "cage" };
    let id = u32::try_from(face.unsigned_abs()).unwrap_or(u32::MAX);
    let mut name = FaceName::new(feature, role, Some(RoleKey::Index(id)));
    name.split = vec![corner];
    name
}

/// The name of every face a cage face makes, its patches together.
pub fn cage_face(feature: FeatureUid, face: i64) -> FaceName {
    let role = if face < 0 { "mirror" } else { "cage" };
    let id = u32::try_from(face.unsigned_abs()).unwrap_or(u32::MAX);
    FaceName::new(feature, role, Some(RoleKey::Index(id)))
}

/// The kernel's form of a patch.
pub fn surface_patch(feature: FeatureUid, patch: &Patch) -> SurfacePatch {
    SurfacePatch {
        face: patch_face(feature, patch.face, patch.corner),
        u_degree: 3,
        v_degree: 3,
        nu: patch.nu,
        nv: patch.nv,
        poles: patch.poles.clone(),
        u_knots: patch.u_knots.clone(),
        u_mults: patch.u_mults.clone(),
        v_knots: patch.v_knots.clone(),
        v_mults: patch.v_mults.clone(),
    }
}

impl FeatureInfo for FormDef {
    fn references(&self) -> References {
        let mut references = References::default();
        for body in &self.participants {
            references.body(*body);
        }
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        self.cage.check()?;
        if self.operation.makes_bodies() && !self.participants.is_empty() {
            return Err("a new body form has no participant bodies".to_owned());
        }
        if !self.operation.makes_bodies() && !self.cage.full().closed() {
            return Err("an open cage makes a surface body: it can only be a new body".to_owned());
        }
        for (i, body) in self.participants.iter().enumerate() {
            if self.participants[..i].contains(body) {
                return Err(format!("body {body} is listed more than once"));
            }
            ctx.body(*body)?;
        }
        Ok(())
    }

    fn creates_bodies(&self) -> bool {
        true
    }

    fn new_component(&self) -> bool {
        self.operation == Operation::NewComponent
    }

    fn tool_use(&self) -> Option<ToolUse> {
        Some(ToolUse {
            operation: self.operation,
            participants: self.participants.clone(),
        })
    }
}

impl<K: Kernel> Evaluate<K> for FormDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let surface = mitcad_subdiv::limit_surface(&self.cage)?;
        let patches: Vec<SurfacePatch> = surface
            .patches
            .iter()
            .map(|patch| surface_patch(ctx.uid, patch))
            .collect();
        let tool = ctx
            .kernel
            .patch_body(&PatchBodySpec {
                patches: &patches,
                closed: surface.closed,
                tolerance: SEWING_TOLERANCE,
            })
            .map_err(|e| e.to_string())?;
        if !surface.closed {
            if !self.operation.makes_bodies() {
                return Err(
                    "an open cage makes a surface body: it can only be a new body".to_owned(),
                );
            }
            let body = ctx.new_body();
            return Ok(FeatureOutput {
                changes: vec![BodyChange::Set(body, tool.clone())],
                tool: Some(tool),
                ..FeatureOutput::default()
            });
        }
        let mut bodies = BodySet::new(if self.operation.makes_bodies() {
            Vec::new()
        } else {
            ctx.bodies()
        });
        if !apply_tool(ctx, &mut bodies, self.operation, &self.participants, &tool)? {
            let op = boolean_op(self.operation).expect("new bodies always apply");
            return Err(format!(
                "the form does not {} any participant body",
                match verb(op) {
                    "join" => "touch",
                    "cut" => "cut into",
                    other => other,
                }
            ));
        }
        Ok(FeatureOutput {
            changes: bodies.changes(),
            tool: Some(tool),
            ..FeatureOutput::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::Document;
    use crate::FeatureStatus;
    use crate::features::FeatureDef;
    use crate::test_support::def;
    use crate::testing::MockKernel;
    use mitcad_subdiv::primitive::box_cage;

    fn form(cage: &Cage) -> serde_json::Value {
        json!({"type": "form", "cage": cage})
    }

    /// A closed cage makes a solid whose faces are named per cage face and
    /// corner; the definition reads back as written.
    #[test]
    fn a_box_cage_makes_a_body_named_by_its_cage_faces() {
        let mut doc = Document::new(MockKernel::default());
        let cage = box_cage([0.0; 3], [40.0, 40.0, 40.0], [1, 1, 1]).unwrap();
        let added = doc.add_feature(&def(form(&cage)), None).unwrap();
        assert_eq!(added.name, "Form1");
        assert!(added.parameters.is_empty());
        assert_eq!(doc.status(added.uid), Some(&FeatureStatus::Ok));
        let body = BodyUid::new(added.uid, 0);
        let shape = doc.body_shape(body).expect("the form's body");
        // Six cage faces of four corners: 24 patches.
        assert_eq!(shape.faces.len(), 24);
        for face in 1..=6 {
            for corner in 0..4 {
                assert!(
                    shape.faces.contains(&patch_face(added.uid, face, corner)),
                    "{face} {corner}"
                );
            }
        }
        let entry = doc.feature(added.uid).unwrap();
        let FeatureDef::Form(stored) = &entry.def else {
            panic!("{:?}", entry.def);
        };
        assert_eq!(stored.cage, cage);
        // A new body is the default, left out of the file.
        let text = serde_json::to_value(stored).unwrap();
        assert_eq!(text, json!({"cage": cage}));
    }

    /// A broken cage is refused; an open one cannot cut.
    #[test]
    fn broken_and_open_cages_are_refused_where_they_cannot_be() {
        let mut doc = Document::new(MockKernel::default());
        let mut cage = box_cage([0.0; 3], [40.0, 40.0, 40.0], [1, 1, 1]).unwrap();
        cage.faces[0].vertices.reverse();
        let error = doc.add_feature(&def(form(&cage)), None).unwrap_err();
        assert!(error.to_string().contains("same way"), "{error}");
        let open = mitcad_subdiv::primitive::plane_cage([0.0; 3], [30.0, 20.0], [1, 1]).unwrap();
        let error = doc
            .add_feature(
                &def(json!({"type": "form", "cage": open, "operation": "cut"})),
                None,
            )
            .unwrap_err();
        assert!(error.to_string().contains("open cage"), "{error}");
        let sheet = doc.add_feature(&def(form(&open)), None).unwrap();
        assert_eq!(doc.status(sheet.uid), Some(&FeatureStatus::Ok));
        let shape = doc.body_shape(BodyUid::new(sheet.uid, 0)).unwrap();
        assert_eq!(shape.faces.len(), 4);
    }

    /// The patch names: a reference to the cage face finds its patches.
    #[test]
    fn patch_names_split_their_cage_face() {
        let f = FeatureUid(7);
        assert_eq!(patch_face(f, 3, 2).to_string(), "F7:cage(3)#2");
        assert_eq!(patch_face(f, -3, 0).to_string(), "F7:mirror(3)#0");
        assert!(patch_face(f, 3, 1).matches(&cage_face(f, 3)));
        assert!(!patch_face(f, -3, 1).matches(&cage_face(f, 3)));
    }
}
