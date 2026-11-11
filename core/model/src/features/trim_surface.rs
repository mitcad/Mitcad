// SPDX-License-Identifier: MIT
//! Trim (mitcad#455): a surface body cut by a tool, keeping the pieces the
//! keep points pick (the piece nearest each). The tool is a face
//! operation's: a plane (moved by `offset`), a face (of any body, extended
//! along its surface with `extend`, the default), a body, or sketch curves
//! (open or closed) swept both ways along `direction` (the sketch normal by
//! default). The body keeps its id; its faces keep their names (`#k` for
//! pieces).

use serde::{Deserialize, Serialize};

use super::face_refs::{check_tool, extended_tools, kernel_error, map_offset, resolve_tool};
use super::geom_ref::{GeomRef, Want};
use super::mapping::map_params;
use super::path::{self, SketchCurves};
use super::pattern::none;
use super::{
    BodyChange, CheckContext, EvalContext, Evaluate, FeatureInfo, FeatureOutput, References,
};
use crate::datum::Vec3;
use crate::exchange::BodyKind;
use crate::ids::BodyUid;
use crate::kernel::Kernel;
use crate::parameters::ParamId;
use crate::surface_modelling::{TrimSpec, TrimTool};
use mitcad_vec3::unit;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrimSurfaceDef<P = ParamId> {
    /// A surface body.
    pub body: BodyUid,
    pub tool: GeomRef,
    /// Moves a plane tool along its normal.
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub offset: Option<P>,
    /// The direction sketch curves are swept along.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direction: Option<Vec3>,
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub extend: bool,
    /// Points (mm, the component's coordinates) that pick the pieces kept.
    pub keep: Vec<[f64; 3]>,
}

fn yes() -> bool {
    true
}

fn is_true(value: &bool) -> bool {
    *value
}

impl<P> TrimSurfaceDef<P> {
    pub const TYPE: &'static str = "trim_surface";
    pub const BASE_NAME: &'static str = "Trim";

    map_params! {
        TrimSurfaceDef {
            body: copy,
            tool: clone,
            offset: with(map_offset),
            direction: copy,
            extend: copy,
            keep: clone,
        }
    }
}

impl FeatureInfo for TrimSurfaceDef {
    fn references(&self) -> References {
        let mut references = References::default();
        references.body(self.body);
        self.tool.add_to(&mut references);
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        ctx.body(self.body)?;
        check_tool(
            ctx,
            &self.tool,
            Want::Tool,
            self.offset.is_some(),
            self.direction,
            "the trimming tool",
        )?;
        if matches!(self.tool, GeomRef::Body(b) if b == self.body) {
            return Err(format!("body {} cannot trim itself", self.body));
        }
        if self.keep.is_empty() {
            return Err("no point picks the pieces to keep".to_owned());
        }
        if self.keep.iter().flatten().any(|v| !v.is_finite()) {
            return Err("a keep point is not finite".to_owned());
        }
        Ok(())
    }
}

impl<K: Kernel> Evaluate<K> for TrimSurfaceDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let shape = ctx.surface_body(self.body)?;
        if matches!(ctx.kernel.body_kind(&shape), Ok(BodyKind::Solid)) {
            return Err(format!(
                "body {} (from {}) is a solid; Trim trims surface bodies (Split Body divides solids)",
                self.body,
                ctx.feature_name(self.body.feature)
            ));
        }
        // Sketch curves are swept as chains, open ones too; other tools
        // are a face operation's.
        let trimmed = if let GeomRef::SketchCurves { sketch, curves } = &self.tool {
            let curves = SketchCurves {
                sketch: *sketch,
                curves: curves.clone(),
                chain: false,
                reversed: false,
            };
            let output = ctx.sketch(*sketch)?;
            let frame = output.frame;
            let chains = path::wall_chains(curves.curves_on(ctx, &frame)?, &frame)?;
            let direction = match self.direction {
                Some(d) => unit(d).ok_or("the direction is zero")?,
                None => frame.normal(),
            };
            let spec = TrimSpec {
                body: &shape,
                tool: TrimTool::Curves {
                    chains: &chains,
                    direction,
                },
                extend: self.extend,
                keep: &self.keep,
            };
            ctx.kernel.trim_surface(&spec).map_err(kernel_error)?
        } else {
            let tool = resolve_tool(ctx, &self.tool, self.offset, self.direction)?;
            let spec = TrimSpec {
                body: &shape,
                tool: TrimTool::Tool(tool.input()),
                extend: self.extend,
                keep: &self.keep,
            };
            ctx.kernel.trim_surface(&spec).map_err(kernel_error)?
        };
        Ok(FeatureOutput {
            changes: vec![BodyChange::Set(self.body, trimmed)],
            ..FeatureOutput::default()
        })
    }

    /// The tool as the trim extends it, for the open command's preview
    /// only (mitcad#712); sketch curves are swept, not extended.
    fn preview_tool(&self, ctx: &mut EvalContext<'_, K>) -> Option<K::Shape> {
        if !self.extend || matches!(self.tool, GeomRef::SketchCurves { .. }) {
            return None;
        }
        let shape = ctx.surface_body(self.body).ok()?;
        let tool = resolve_tool(ctx, &self.tool, self.offset, self.direction).ok()?;
        extended_tools(ctx.kernel, &[&shape], &[&tool], self.extend)
    }
}
