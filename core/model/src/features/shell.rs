// SPDX-License-Identifier: MIT
//! Shell: hollows one body. The removed faces become openings; without any
//! the body gets a closed void. The wall runs `inside` into the body and
//! `outside` out of it from its surface (one may be left out). With
//! `tangent_chain` (the default) the faces tangent to a removed face go
//! too; `rounded` rounds the outer corners (a rounded offset), else they
//! stay sharp.
//!
//! The outside of the wall keeps the body's face names; the inside is
//! `<shell>:offset(<face>)` and the end of the wall where a removed face was
//! `<shell>:offset_cap(<face>)`.

use serde::{Deserialize, Serialize};

use super::face_refs::{add_faces, check_faces, kernel_error, require_faces};
use super::{
    BodyChange, CheckContext, EvalContext, Evaluate, FeatureInfo, FeatureOutput, References,
    is_false,
};
use crate::ids::BodyUid;
use crate::kernel::{Kernel, ShellSpec};
use crate::parameters::ParamId;
use crate::topo::FaceName;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(bound(deserialize = "P: Deserialize<'de>"))]
pub struct ShellDef<P = ParamId> {
    pub body: BodyUid,
    /// The faces to remove; none for a closed void.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub faces: Vec<FaceName>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inside: Option<P>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outside: Option<P>,
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub tangent_chain: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub rounded: bool,
}

fn yes() -> bool {
    true
}

fn is_true(value: &bool) -> bool {
    *value
}

impl<P> ShellDef<P> {
    pub const TYPE: &'static str = "shell";
    pub const BASE_NAME: &'static str = "Shell";

    pub fn map_params<Q, E>(
        &self,
        f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
    ) -> Result<ShellDef<Q>, E> {
        Ok(ShellDef {
            body: self.body,
            faces: self.faces.clone(),
            inside: self.inside.as_ref().map(|v| f("inside", v)).transpose()?,
            outside: self.outside.as_ref().map(|v| f("outside", v)).transpose()?,
            tangent_chain: self.tangent_chain,
            rounded: self.rounded,
        })
    }
}

impl FeatureInfo for ShellDef {
    fn references(&self) -> References {
        let mut references = References::default();
        references.body(self.body);
        add_faces(&mut references, &self.faces);
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        ctx.body(self.body)?;
        if !self.faces.is_empty() {
            check_faces(ctx, &self.faces, "faces to remove")?;
        }
        if self.inside.is_none() && self.outside.is_none() {
            return Err("the shell needs an inside or an outside thickness".to_owned());
        }
        Ok(())
    }

    fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
        let inside = self.inside.map_or(0.0, value);
        let outside = self.outside.map_or(0.0, value);
        thicknesses(inside, outside)
    }
}

fn thicknesses(inside: f64, outside: f64) -> Result<(), String> {
    if !(inside >= 0.0 && outside >= 0.0) {
        Err(format!(
            "shell thicknesses must not be negative, got {inside} inside and {outside} outside"
        ))
    } else if inside == 0.0 && outside == 0.0 {
        Err("the shell needs an inside or an outside thickness greater than zero".to_owned())
    } else {
        Ok(())
    }
}

impl<K: Kernel> Evaluate<K> for ShellDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let inside = match self.inside {
            Some(id) => ctx.param(id)?,
            None => 0.0,
        };
        let outside = match self.outside {
            Some(id) => ctx.param(id)?,
            None => 0.0,
        };
        thicknesses(inside, outside)?;
        let shape = ctx.body(self.body)?;
        require_faces(ctx, self.body, &shape, &self.faces)?;
        let spec = ShellSpec {
            faces: &self.faces,
            inside,
            outside,
            tangent_chain: self.tangent_chain,
            rounded: self.rounded,
        };
        let hollow = ctx
            .kernel
            .shell(ctx.uid, &shape, &spec)
            .map_err(kernel_error)?;
        Ok(FeatureOutput {
            changes: vec![BodyChange::Set(self.body, hollow)],
            ..FeatureOutput::default()
        })
    }
}
