// SPDX-License-Identifier: MIT
//! Dogbone (mitcad#1011): relief cuts at a body's inside corners, so that a
//! router bit of the tool's diameter clears the sharp corner a square part
//! fits into. For each inside (concave) corner edge a cylinder of the
//! tool's diameter is cut along the edge, placed so that its circle passes
//! through the sharp corner (plus the extra clearance):
//!
//! - **Diagonal**: along the corner's bisector;
//! - **Along a side** (a T-bone): along one face of the corner, the face
//!   picked or the longer of the two.
//!
//! The picks: inside corner edges, faces (their inside corner edges
//! parallel to the tool axis, and of a face across the axis, a pocket's
//! floor, the corner edges that end on it) or, with neither, the whole
//! body (every inside corner edge parallel to the tool axis). The tool
//! axis is `axis`, the component's Z axis by default; picked edges are
//! taken whatever their direction. The kernel finds the edges and makes
//! the cylinders (`Kernel::dogbone_tool`), refusing a picked edge that is
//! not straight or not an inside corner with the reason; the dogbone cuts
//! them from the body, which keeps its id and its faces' names.

use serde::{Deserialize, Serialize};

use super::base::apply_tool;
use super::face_refs::{add_faces, check_faces, kernel_error, require_faces};
use super::mapping::map_params;
use super::{CheckContext, EvalContext, Evaluate, FeatureInfo, FeatureOutput, References};
use crate::ids::BodyUid;
// The relief's placement is the kernel's input.
pub use crate::kernel::DogboneKind;
use crate::kernel::{BooleanOp, DogboneSpec, Kernel};
use crate::parameters::ParamId;
use crate::topo::{EdgeName, FaceName};
use mitcad_vec3::unit;

/// The tool diameter without one in the definition (a quarter-inch bit).
pub const DEFAULT_DIAMETER: f64 = 6.35;
/// The extra clearance without one in the definition.
pub const DEFAULT_EXTRA: f64 = 0.1;
/// The tool axis without one in the definition: the component's Z axis.
pub const DEFAULT_AXIS: [f64; 3] = [0.0, 0.0, 1.0];

/// Lengths in millimetres.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, bound(deserialize = "P: Deserialize<'de>"))]
pub struct DogboneDef<P = ParamId> {
    pub body: BodyUid,
    /// Inside corner edges, whatever their direction.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub edges: Vec<EdgeName>,
    /// Faces whose inside corners along the tool axis are taken.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub faces: Vec<FaceName>,
    /// The tool's diameter; [`DEFAULT_DIAMETER`] without one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diameter: Option<P>,
    /// How far beyond the sharp corner the relief reaches; [`DEFAULT_EXTRA`]
    /// without one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extra: Option<P>,
    #[serde(default, rename = "dogbone_type", skip_serializing_if = "is_diagonal")]
    pub kind: DogboneKind,
    /// The router bit's axis in the component's coordinates (need not be a
    /// unit vector); [`DEFAULT_AXIS`] without one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub axis: Option<[f64; 3]>,
}

fn is_diagonal(kind: &DogboneKind) -> bool {
    *kind == DogboneKind::Diagonal
}

impl<P> DogboneDef<P> {
    pub const TYPE: &'static str = "dogbone";
    pub const BASE_NAME: &'static str = "Dogbone";

    map_params! {
        DogboneDef {
            body: copy,
            edges: clone,
            faces: clone,
            diameter: option,
            extra: option,
            kind: copy,
            axis: copy,
        }
    }
}

fn check_diameter(value: f64) -> Result<(), String> {
    if value > 0.0 {
        Ok(())
    } else {
        Err(format!(
            "the dogbone's tool diameter must be greater than zero, got {value}"
        ))
    }
}

fn check_extra(value: f64) -> Result<(), String> {
    if value >= 0.0 {
        Ok(())
    } else {
        Err(format!(
            "the dogbone's extra clearance must not be negative, got {value}"
        ))
    }
}

impl DogboneDef {
    /// The tool axis as a unit vector.
    fn tool_axis(&self) -> Result<[f64; 3], String> {
        let axis = self.axis.unwrap_or(DEFAULT_AXIS);
        unit(axis).ok_or_else(|| {
            format!(
                "the dogbone's tool axis must be a direction, got [{}, {}, {}]",
                axis[0], axis[1], axis[2]
            )
        })
    }
}

impl FeatureInfo for DogboneDef {
    fn references(&self) -> References {
        let mut references = References::default();
        references.body(self.body);
        references.edges(&self.edges);
        add_faces(&mut references, &self.faces);
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        ctx.body(self.body)?;
        if !self.edges.is_empty() {
            ctx.edges(&self.edges)?;
        }
        if !self.faces.is_empty() {
            check_faces(ctx, &self.faces, "faces")?;
        }
        self.tool_axis().map(|_| ())
    }

    fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
        if let Some(diameter) = self.diameter {
            check_diameter(value(diameter))?;
        }
        if let Some(extra) = self.extra {
            check_extra(value(extra))?;
        }
        Ok(())
    }
}

impl<K: Kernel> Evaluate<K> for DogboneDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let diameter = match self.diameter {
            Some(id) => {
                let value = ctx.param(id)?;
                check_diameter(value).map_err(|e| format!("{}: {e}", ctx.param_name(id)))?;
                value
            }
            None => DEFAULT_DIAMETER,
        };
        let extra = match self.extra {
            Some(id) => {
                let value = ctx.param(id)?;
                check_extra(value).map_err(|e| format!("{}: {e}", ctx.param_name(id)))?;
                value
            }
            None => DEFAULT_EXTRA,
        };
        let axis = self.tool_axis()?;
        let shape = ctx.surface_body(self.body)?;
        // The names the kernel gets: an edge or face a sketch change
        // renamed is found again (mitcad#272); the definition keeps its
        // names.
        let edges = if self.edges.is_empty() {
            Vec::new()
        } else {
            ctx.require_edges(self.body, &shape, &self.edges)?
        };
        let faces = if self.faces.is_empty() {
            Vec::new()
        } else {
            require_faces(ctx, self.body, &shape, &self.faces)?
        };
        let spec = DogboneSpec {
            feature: ctx.uid,
            body: &shape,
            edges: &edges,
            faces: &faces,
            axis,
            diameter,
            extra,
            kind: self.kind,
        };
        let tool = ctx.kernel.dogbone_tool(&spec).map_err(kernel_error)?;
        let changes = apply_tool(
            ctx,
            BooleanOp::Cut,
            &[(self.body, shape.clone())],
            &tool,
            "dogbone",
        )?;
        // The picked edges and faces where they were, for the timeline
        // (mitcad#957).
        let inputs = super::inputs::picked_edges(
            ctx.kernel,
            &shape,
            std::iter::once((edges.as_slice(), faces.as_slice())),
        );
        Ok(FeatureOutput {
            changes,
            tool: Some(tool),
            inputs,
            ..FeatureOutput::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::features::{FeatureDef, ValueInput};

    fn parse(json: &str) -> FeatureDef<ValueInput> {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn the_definition_round_trips_with_its_slots() {
        let short = r#"{"type":"dogbone","body":"F2.b0"}"#;
        assert_eq!(serde_json::to_string(&parse(short)).unwrap(), short);
        let full = r#"{"type":"dogbone","body":"F2.b0","edges":["E{F2:a|F2:b}"],"faces":["F3:bottom"],"diameter":6.35,"extra":0.1,"dogbone_type":"along_side","axis":[0.0,1.0,0.0]}"#;
        let def = parse(full);
        assert_eq!(serde_json::to_string(&def).unwrap(), full);
        let mut slots = Vec::new();
        def.map_params(&mut |slot, _| {
            slots.push(slot.to_owned());
            Ok::<_, ()>(())
        })
        .unwrap();
        assert_eq!(slots, ["diameter", "extra"]);
        let error = serde_json::from_str::<FeatureDef<ValueInput>>(
            r#"{"type":"dogbone","body":"F2.b0","dogbone_type":"t_bone"}"#,
        )
        .unwrap_err();
        assert!(error.to_string().contains("unknown variant"), "{error}");
    }
}
