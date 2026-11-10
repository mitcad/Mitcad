// SPDX-License-Identifier: MIT
//! Mesh bodies in surface operations (mitcad#399). A mesh body has
//! triangles and no surfaces (STL and OBJ files, the meshes of imported
//! models; [`BodyKind::Mesh`]). The kernel's booleans, fillets, chamfers,
//! face operations and splits work on surfaces: given a face without one,
//! the boolean's face intersection reads a null surface and fails (an
//! access violation on Windows, mitcad#392). So the features refuse a mesh
//! body they are given by name, with a message naming it, before the kernel
//! sees it, and leave mesh bodies out where they take all bodies (an
//! extrusion's Join or Cut without participants: mesh bodies are not cut).

use super::EvalContext;
use crate::exchange::BodyKind;
use crate::ids::BodyUid;
use crate::kernel::Kernel;

/// Whether the shape is a mesh body; a kernel that cannot tell has none.
pub fn is_mesh<K: Kernel>(kernel: &K, shape: &K::Shape) -> bool {
    matches!(kernel.body_kind(shape), Ok(BodyKind::Mesh))
}

impl<K: Kernel> EvalContext<'_, K> {
    /// Whether the shape is a mesh body ([`is_mesh`]).
    pub fn is_mesh(&self, shape: &K::Shape) -> bool {
        is_mesh(self.kernel, shape)
    }

    /// Why this feature refuses `body`, a mesh body: "body F4.b0 (from
    /// scan) is a mesh body (triangles without surfaces), which Combine
    /// cannot use; it works on solid and surface bodies".
    pub fn mesh_refusal(&self, body: BodyUid) -> String {
        let operation = self
            .env
            .features
            .iter()
            .find(|f| f.uid == self.uid)
            .map_or("this feature", |f| f.def.base_name());
        format!(
            "body {body} (from {}) is a mesh body (triangles without surfaces), which \
             {operation} cannot use; it works on solid and surface bodies",
            self.feature_name(body.feature)
        )
    }

    /// Refuses `shape`, the shape of `body`, when it is a mesh body.
    pub fn require_surfaces(&self, body: BodyUid, shape: &K::Shape) -> Result<(), String> {
        if self.is_mesh(shape) {
            Err(self.mesh_refusal(body))
        } else {
            Ok(())
        }
    }

    /// The current shape of a body that the feature hands to a surface
    /// operation ([`EvalContext::body`]); a mesh body is refused.
    pub fn surface_body(&mut self, body: BodyUid) -> Result<K::Shape, String> {
        let shape = self.body(body)?;
        self.require_surfaces(body, &shape)?;
        Ok(shape)
    }

    /// All bodies of the feature's component but the mesh bodies, in order
    /// of their ids: what an operation on all bodies works on.
    pub fn surface_bodies(&mut self) -> Vec<(BodyUid, K::Shape)> {
        let bodies = self.bodies();
        bodies
            .into_iter()
            .filter(|(_, shape)| !self.is_mesh(shape))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use crate::exchange::{BaseInput, ImportBody};
    use crate::ids::{BodyUid, FeatureUid};
    use crate::test_support::{Block, block, def, extrude};
    use crate::testing::MockShape;
    use crate::{Document, FeatureStatus};

    /// The block (F2.b0) and a mesh body imported from an STL file
    /// (F3.b0, from the base feature "scan").
    fn block_and_mesh() -> (Block, BodyUid) {
        let mut b = block();
        let imported = b
            .doc
            .add_base_feature(BaseInput {
                name: Some("scan".to_owned()),
                ..BaseInput::new(vec![ImportBody {
                    name: Some("Scan".to_owned()),
                    color: None,
                    shape: MockShape::imported("mesh", 1),
                }])
            })
            .unwrap();
        let mesh = imported.bodies[0].uid;
        assert_eq!(mesh.to_string(), "F3.b0");
        (b, mesh)
    }

    fn add(doc: &mut Document<crate::testing::MockKernel>, value: Value) -> FeatureUid {
        doc.add_feature(&def(value), None).unwrap().uid
    }

    fn failure(doc: &Document<crate::testing::MockKernel>, uid: FeatureUid) -> String {
        match doc.status(uid).cloned().expect("recomputed") {
            FeatureStatus::Failed(message) => message.agent_text().to_owned(),
            other => panic!("{uid} did not fail: {other:?}"),
        }
    }

    fn history(doc: &Document<crate::testing::MockKernel>, body: BodyUid) -> String {
        doc.body_shape(body).expect("the body").history.clone()
    }

    #[test]
    fn combine_refuses_a_mesh_body_by_name_before_the_kernel() {
        let (mut b, mesh) = block_and_mesh();
        for (target, tool) in [("F2.b0", "F3.b0"), ("F3.b0", "F2.b0")] {
            for operation in ["join", "cut", "intersect"] {
                let uid = add(
                    &mut b.doc,
                    json!({"type": "combine", "target": target, "tools": [tool],
                           "operation": operation}),
                );
                assert_eq!(
                    failure(&b.doc, uid),
                    "body F3.b0 (from scan) is a mesh body (triangles without surfaces), which \
                     Combine cannot use; it works on solid and surface bodies"
                );
                b.doc.delete_feature(uid, false).unwrap();
            }
        }
        for op in ["unite", "join", "cut", "intersect"] {
            assert_eq!(b.doc.kernel().count(op), 0, "{op}");
        }
        assert!(history(&b.doc, mesh).contains(",mesh)"));
    }

    #[test]
    fn operations_on_all_bodies_leave_mesh_bodies_out() {
        let (mut b, mesh) = block_and_mesh();
        // A cut and a join without participants work on the block only.
        for operation in ["cut", "join"] {
            let uid = b
                .doc
                .add_feature(&extrude(b.sketch, &[&b.region], 5.0, operation, &[]), None)
                .unwrap()
                .uid;
            assert!(b.doc.status(uid).unwrap().is_ok(), "{operation}");
            assert!(history(&b.doc, mesh).contains(",mesh)"), "{operation}");
            assert!(
                history(&b.doc, b.body).starts_with(operation),
                "{operation}"
            );
            b.doc.delete_feature(uid, false).unwrap();
        }
        // Listed, the mesh body is refused.
        let uid = b
            .doc
            .add_feature(&extrude(b.sketch, &[&b.region], 5.0, "cut", &[mesh]), None)
            .unwrap()
            .uid;
        let message = failure(&b.doc, uid);
        assert!(
            message.starts_with("body F3.b0 (from scan) is a mesh body")
                && message.contains("Extrude"),
            "{message}"
        );
    }

    #[test]
    fn dressups_face_operations_and_splits_refuse_mesh_bodies() {
        let (mut b, _) = block_and_mesh();
        let face = "F3:import(0)";
        for (value, operation) in [
            (
                json!({"type": "fillet", "body": "F3.b0", "edges": [format!("E{{{face}|{face}}}")],
                       "radius": 1.0}),
                "Fillet",
            ),
            (
                json!({"type": "shell", "body": "F3.b0", "faces": [face], "inside": 1.0}),
                "Shell",
            ),
            (
                json!({"type": "offset_face", "body": "F3.b0", "faces": [face], "distance": 1.0}),
                "OffsetFace",
            ),
            (
                json!({"type": "split_body", "bodies": ["F3.b0"],
                       "tool": {"type": "origin_plane", "plane": "xy"}}),
                "SplitBody",
            ),
            (
                json!({"type": "split_body", "bodies": ["F2.b0"], "tool": {"type": "body", "body": "F3.b0"}}),
                "SplitBody",
            ),
        ] {
            let uid = add(&mut b.doc, value.clone());
            let message = failure(&b.doc, uid);
            assert!(
                message.starts_with("body F3.b0 (from scan) is a mesh body")
                    && message.contains(&format!("which {operation} cannot use")),
                "{value}: {message}"
            );
            b.doc.delete_feature(uid, false).unwrap();
        }
        for op in ["fillet", "shell", "offset_faces", "split_body"] {
            assert_eq!(b.doc.kernel().count(op), 0, "{op}");
        }
    }

    #[test]
    fn a_mirror_that_joins_refuses_a_mesh_body_and_copies_it_otherwise() {
        let (mut b, _) = block_and_mesh();
        let mirror = |combine: bool| {
            json!({"type": "mirror", "objects": {"type": "bodies", "bodies": ["F3.b0"]},
                   "plane": {"type": "origin", "plane": "yz"}, "combine": combine})
        };
        let uid = add(&mut b.doc, mirror(true));
        assert!(failure(&b.doc, uid).contains("which Mirror cannot use"));
        b.doc.delete_feature(uid, false).unwrap();
        let uid = add(&mut b.doc, mirror(false));
        assert!(b.doc.status(uid).unwrap().is_ok());
    }
}
