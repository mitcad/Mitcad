// SPDX-License-Identifier: MIT
//! Draft: tilts faces of one body about a fixed plane. The plane is a plane
//! reference (an origin or construction plane, a fixed plane, moved by
//! `offset`) or a planar face; the faces turn about their intersection with
//! it by `angle`.
//!
//! The pull direction is the plane's normal, or for a face from the face
//! into the material; `flip` reverses it. A positive angle removes material
//! on the pull side (a negative one adds), so a side face drafted from the
//! bottom face narrows towards the top. `symmetric` drafts both sides of
//! the plane by `angle`, `angle2` sets the other side's angle; the faces are
//! split at the plane and each side narrows away from it. With
//! `tangent_chain` off, a face tangent to a drafted one is unsupported (the
//! geometry kernel drafts the whole chain). Parting line drafts are not
//! supported.
//!
//! The faces keep their names; split pieces get `#k`.

use serde::{Deserialize, Serialize};

use super::face_refs::{
    add_faces, check_faces, check_tool, kernel_error, map_offset, require_faces, resolve_tool,
};
use super::geom_ref::{GeomRef, Want};
use super::{
    BodyChange, CheckContext, EvalContext, Evaluate, FeatureInfo, FeatureOutput, References,
    is_false,
};
use crate::ids::BodyUid;
use crate::kernel::{DraftSpec, Kernel};
use crate::parameters::ParamId;
use crate::topo::FaceName;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(bound(deserialize = "P: Deserialize<'de>"))]
pub struct DraftDef<P = ParamId> {
    pub body: BodyUid,
    pub faces: Vec<FaceName>,
    /// The fixed plane: a plane or a planar face.
    pub plane: GeomRef,
    /// Moves a plane (not a face) along its normal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset: Option<P>,
    /// Radians.
    pub angle: P,
    #[serde(default, skip_serializing_if = "is_false")]
    pub symmetric: bool,
    /// The angle on the other side of the plane (two angles).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub angle2: Option<P>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub flip: bool,
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub tangent_chain: bool,
}

fn yes() -> bool {
    true
}

fn is_true(value: &bool) -> bool {
    *value
}

impl<P> DraftDef<P> {
    pub const TYPE: &'static str = "draft";
    pub const BASE_NAME: &'static str = "Draft";

    pub fn map_params<Q, E>(
        &self,
        f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
    ) -> Result<DraftDef<Q>, E> {
        Ok(DraftDef {
            body: self.body,
            faces: self.faces.clone(),
            plane: self.plane.clone(),
            offset: map_offset(&self.offset, f)?,
            angle: f("angle", &self.angle)?,
            symmetric: self.symmetric,
            angle2: self.angle2.as_ref().map(|a| f("angle2", a)).transpose()?,
            flip: self.flip,
            tangent_chain: self.tangent_chain,
        })
    }
}

fn check_angle(angle: f64) -> Result<(), String> {
    if angle != 0.0 && angle.abs() < std::f64::consts::FRAC_PI_2 {
        Ok(())
    } else {
        Err(format!(
            "the draft angle must be between -90 and 90 degrees and not zero, got {angle} rad"
        ))
    }
}

impl FeatureInfo for DraftDef {
    fn references(&self) -> References {
        let mut references = References::default();
        references.body(self.body);
        add_faces(&mut references, &self.faces);
        self.plane.add_to(&mut references);
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        ctx.body(self.body)?;
        check_faces(ctx, &self.faces, "faces to draft")?;
        check_tool(
            ctx,
            &self.plane,
            Want::Plane,
            self.offset.is_some(),
            None,
            "the fixed plane",
        )?;
        if self.symmetric && self.angle2.is_some() {
            return Err("a symmetric draft has one angle".to_owned());
        }
        Ok(())
    }

    fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
        check_angle(value(self.angle))?;
        self.angle2.map_or(Ok(()), |a| check_angle(value(a)))
    }
}

impl<K: Kernel> Evaluate<K> for DraftDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let angle = ctx.param(self.angle)?;
        check_angle(angle).map_err(|e| format!("{}: {e}", ctx.param_name(self.angle)))?;
        let angle2 = match self.angle2 {
            Some(id) => {
                let value = ctx.param(id)?;
                check_angle(value).map_err(|e| format!("{}: {e}", ctx.param_name(id)))?;
                Some(value)
            }
            None if self.symmetric => Some(angle),
            None => None,
        };
        let shape = ctx.body(self.body)?;
        require_faces(ctx, self.body, &shape, &self.faces)?;
        let plane = resolve_tool(ctx, &self.plane, self.offset, None)?;
        let spec = DraftSpec {
            faces: &self.faces,
            plane: plane.input(),
            angle,
            angle2,
            flip: self.flip,
            tangent_chain: self.tangent_chain,
        };
        let drafted = ctx
            .kernel
            .draft(ctx.uid, &shape, &spec)
            .map_err(kernel_error)?;
        Ok(FeatureOutput {
            changes: vec![BodyChange::Set(self.body, drafted)],
            ..FeatureOutput::default()
        })
    }
}
