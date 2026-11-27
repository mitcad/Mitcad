// SPDX-License-Identifier: MIT
//! Mesh tools for reverse engineering (mitcad#1871, see `commands.md`,
//! "Mesh tools for reverse engineering"): the `mesh_section` command (a
//! mesh body's sections into sketches, one or a parallel series) and the
//! `mesh_stats` query (a mesh body's counts: triangles, open and
//! non-manifold edges, holes). The mesh edits themselves are features
//! (`mesh_decimate`, `mesh_repair`, `mesh_remove`, `mesh_segment`,
//! `mesh_fit_surface`).

use serde::Deserialize;
use serde_json::{Value, json};

use super::ApiError;
use super::dispatch::named_enum;
use super::prelude::*;
use mitcad_model::Document;
use mitcad_model::exchange::{BodyKind, MeshTolerance};
use mitcad_model::features::SketchPlane;
use mitcad_model::ids::{BodyUid, FeatureUid};
use mitcad_model::kernel::Kernel;
use mitcad_model::{MeshSectionRequest, SectionTarget};

named_enum! {
    #[derive(Debug, Deserialize)]
    #[serde(tag = "cmd", rename_all = "snake_case", deny_unknown_fields)]
    pub(crate) enum MeshCommand {
        "mesh_section" => MeshSection {
            body: BodyUid,
            /// Into this sketch; or `plane`, a new sketch on it.
            #[serde(default)]
            sketch: Option<FeatureUid>,
            #[serde(default)]
            plane: Option<SketchPlane>,
            /// mm, 0.05 when left out.
            #[serde(default)]
            tolerance: Option<f64>,
            /// Sections in all, 1 when left out.
            #[serde(default)]
            count: Option<usize>,
            /// mm between neighbouring sections along the sketch's normal.
            #[serde(default)]
            spacing: Option<f64>,
        },
    }
}

/// The tolerance of a section's curves when the command gives none (mm).
pub(crate) const DEFAULT_SECTION_TOLERANCE: f64 = 0.05;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MeshStatsQuery {
    body: BodyUid,
}

/// The document's methods of this module used across the api, as an
/// extension trait (mitcad#777).
pub(crate) trait MeshToolsOps<K: Kernel> {
    fn run_mesh_command(&mut self, command: MeshCommand) -> Result<Value, ApiError>;
    fn mesh_stats_json(&self, query: &MeshStatsQuery) -> Result<Value, ApiError>;
}

impl<K: Kernel> MeshToolsOps<K> for Document<K> {
    fn run_mesh_command(&mut self, command: MeshCommand) -> Result<Value, ApiError> {
        match command {
            MeshCommand::MeshSection {
                body,
                sketch,
                plane,
                tolerance,
                count,
                spacing,
            } => {
                let target = match (sketch, plane) {
                    (Some(sketch), None) => SectionTarget::Sketch(sketch),
                    (None, Some(plane)) => SectionTarget::Plane(plane),
                    _ => {
                        return Err(ApiError(
                            "mesh_section takes a \"sketch\" to put the section into or a \
                             \"plane\" for a new sketch, not both"
                                .to_owned(),
                        ));
                    }
                };
                let made = self.mesh_sections(&MeshSectionRequest {
                    target,
                    body,
                    tolerance: tolerance.unwrap_or(DEFAULT_SECTION_TOLERANCE),
                    count: count.unwrap_or(1),
                    spacing: spacing.unwrap_or(0.0),
                })?;
                let names: Vec<String> = made
                    .sketches
                    .iter()
                    .map(|uid| {
                        self.feature(*uid)
                            .map_or_else(|| uid.to_string(), |f| f.name.clone())
                    })
                    .collect();
                Ok(json!({
                    "sketches": made.sketches.iter().map(ToString::to_string).collect::<Vec<_>>(),
                    "names": names,
                    "curves": made.lines + made.arcs + made.circles,
                    "lines": made.lines,
                    "arcs": made.arcs,
                    "circles": made.circles,
                    "missed": made.missed,
                }))
            }
        }
    }

    fn mesh_stats_json(&self, query: &MeshStatsQuery) -> Result<Value, ApiError> {
        let shape = self.require_body(query.body)?;
        if !matches!(self.kernel().body_kind(shape), Ok(BodyKind::Mesh)) {
            return Err(ApiError(format!(
                "body {} is not a mesh body; mesh_stats counts a mesh body's triangles",
                query.body
            )));
        }
        let mesh = self
            .kernel()
            .triangle_mesh(
                shape,
                &MeshTolerance {
                    deviation: 0.01,
                    angle: 0.1,
                    face_triangles: 0,
                    max_edge: 0.0,
                },
            )
            .map_err(|e| ApiError(format!("the mesh's triangles: {e}")))?;
        // Corners stored apart (an STL file's) joined first.
        let joined = mitcad_model::mesh_tools::joined(&mesh);
        let stats = mitcad_model::mesh_tools::mesh_stats(&joined);
        let closed = stats.open_edges == 0 && stats.non_manifold_edges == 0;
        let mut result = json!({
            "body": query.body.to_string(),
            "vertices": stats.vertices,
            "triangles": stats.triangles,
            "open_edges": stats.open_edges,
            "non_manifold_edges": stats.non_manifold_edges,
            "holes": stats.holes,
            "area": stats.area,
            "closed": closed,
        });
        // The enclosed volume of a closed mesh (mitcad#2515).
        if closed {
            result["volume"] = json!(mitcad_model::mesh_tools::volume(&joined));
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use crate::DocumentApi;
    use mitcad_model::Document;
    use mitcad_model::exchange::{BaseInput, ImportBody};
    use mitcad_model::testing::{MockKernel, MockShape};

    fn run(doc: &mut Document<MockKernel>, command: Value) -> Result<Value, String> {
        doc.command(&command.to_string())
            .map(|text| serde_json::from_str(&text).unwrap())
            .map_err(|e| e.to_string())
    }

    /// A mesh body (F1.b0; the mock kernel's mesh of it is a 10 mm box).
    fn mesh_document() -> Document<MockKernel> {
        let mut doc = Document::new(MockKernel::default());
        doc.add_base_feature(BaseInput::new(vec![ImportBody {
            name: Some("Scan".to_owned()),
            color: None,
            shape: MockShape::imported("mesh", 1),
            face_colors: Vec::new(),
        }]))
        .unwrap();
        doc
    }

    #[test]
    fn mesh_stats_counts_the_mesh() {
        let doc = mesh_document();
        let text = doc
            .query(&json!({"query": "mesh_stats", "body": "F1.b0"}).to_string())
            .unwrap();
        let stats: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(stats["triangles"], 12);
        assert_eq!(stats["vertices"], 8);
        assert_eq!(stats["holes"], 0);
        assert_eq!(stats["closed"], true);
        // The mock kernel's 10 mm box (mitcad#2515).
        assert!((stats["volume"].as_f64().unwrap() - 1000.0).abs() < 1e-9);
    }

    #[test]
    fn mesh_section_on_a_plane_and_its_refusals() {
        let mut doc = mesh_document();
        run(
            &mut doc,
            json!({"cmd": "add_feature", "def": {"type": "construction_plane",
                   "definition": {"type": "offset", "plane": "xy", "distance": 5}}}),
        )
        .unwrap();
        let made = run(
            &mut doc,
            json!({"cmd": "mesh_section", "body": "F1.b0", "plane": "F2", "count": 2,
                   "spacing": 2}),
        )
        .unwrap();
        assert_eq!(made["sketches"], json!(["F3", "F4"]));
        assert_eq!(made["names"], json!(["Sketch1", "Sketch2"]));
        assert_eq!(made["lines"], 8);
        assert_eq!(made["curves"], 8);
        let error = run(
            &mut doc,
            json!({"cmd": "mesh_section", "body": "F1.b0", "plane": "F2", "sketch": "F3"}),
        )
        .unwrap_err();
        assert!(error.contains("not both"), "{error}");
        let error = run(
            &mut doc,
            json!({"cmd": "mesh_section", "body": "F1.b0", "sketch": "F3", "tolerance": 0}),
        )
        .unwrap_err();
        assert!(
            error.contains("the tolerance must be more than 0"),
            "{error}"
        );
    }
}
