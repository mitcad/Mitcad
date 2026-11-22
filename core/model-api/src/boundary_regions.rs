// SPDX-License-Identifier: MIT
//! The `boundary_regions` query (mitcad#2149, see `commands.md`, "Boundary
//! Fill"): the closed regions that tool bodies (solids and surface bodies)
//! and planes bound together, each with a point inside it, for Boundary
//! Fill's panel; with `points`, the region each point lies in.

use serde::Deserialize;
use serde_json::{Value, json};

use super::ApiError;
use super::prelude::*;
use mitcad_model::datum::Vec3;
use mitcad_model::exchange::BodyKind;
use mitcad_model::ids::{BodyUid, FeatureUid};
use mitcad_model::kernel::Kernel;
use mitcad_model::{BoundaryPlace, Document, GeomRef};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct BoundaryRegionsQuery {
    /// The tool bodies, at the timeline marker.
    #[serde(default)]
    tools: Vec<BodyUid>,
    /// Plane references, as a `boundary_fill`'s `planes`.
    #[serde(default)]
    planes: Vec<GeomRef>,
    /// Points to place among the regions (a definition's `cells`).
    #[serde(default)]
    points: Vec<Vec3>,
}

/// Answers the query: `{"regions": [{"point", "min", "max"}], "places":
/// [{"region": i} | {"on_face": true} | {"open": true, "gap": [...]}]}`.
pub(super) fn query<K: Kernel>(
    doc: &Document<K>,
    query: &BoundaryRegionsQuery,
) -> Result<Value, ApiError> {
    if query.tools.is_empty() && query.planes.is_empty() {
        return Err(ApiError("give tool bodies or planes".to_owned()));
    }
    let kernel = doc.kernel();
    let mut tools = Vec::with_capacity(query.tools.len());
    for uid in &query.tools {
        let shape = doc.require_body(*uid)?;
        if matches!(kernel.body_kind(shape), Ok(BodyKind::Mesh)) {
            return Err(ApiError(format!(
                "body {uid} is a mesh: it has no surfaces to bound a region"
            )));
        }
        tools.push(shape);
    }
    let mut planes = Vec::with_capacity(query.planes.len());
    for (i, plane) in query.planes.iter().enumerate() {
        planes.push(
            doc.resolve_plane(plane)
                .map_err(|e| ApiError(format!("plane {}: {e}", i + 1)))?,
        );
    }
    // The names of the planes' pieces do not matter here.
    let found = kernel
        .boundary_regions(FeatureUid(0), &tools, &planes, &query.points)
        .map_err(|e| ApiError(e.to_string()))?;
    let regions: Vec<Value> = found
        .regions
        .iter()
        .map(|region| {
            let mut value = json!({"point": region.point});
            if let Ok(Some(b)) = kernel.bounding_box(&region.shape) {
                value["min"] = json!(b.min);
                value["max"] = json!(b.max);
            }
            value
        })
        .collect();
    let places: Vec<Value> = found
        .places
        .iter()
        .map(|place| match place {
            BoundaryPlace::Region(i) => json!({"region": i}),
            BoundaryPlace::OnFace => json!({"on_face": true}),
            BoundaryPlace::Open { gap } => json!({"open": true, "gap": gap}),
        })
        .collect();
    Ok(json!({"regions": regions, "places": places}))
}
