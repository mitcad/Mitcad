// SPDX-License-Identifier: MIT
//! Convert to Sheet Metal (mitcad#461): a body becomes a sheet of the
//! rules. Without bends, a plate of the rules' thickness whose picked face
//! is its front; with bends, the planar faces reached from the picked face
//! across the bend edges become panels on the sheet's front, with the
//! rules' thickness behind them and a bend at every bend edge, its virtual
//! sharp on that edge.

use super::face_refs::add_faces;
use super::mapping::map_params;
use super::{
    CheckContext, EvalContext, Evaluate, FeatureDef, FeatureInfo, FeatureOutput, References,
};
use crate::ids::{BodyUid, FeatureUid};
use crate::kernel::Kernel;
use crate::parameters::ParamId;
use crate::topo::{EdgeName, FaceName};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, bound(deserialize = "P: Deserialize<'de>"))]
pub struct SheetConvertDef<P = ParamId> {
    pub rules: FeatureUid,
    pub body: BodyUid,
    /// The planar face that becomes the first panel's front; it stays where
    /// it is.
    pub face: FaceName,
    /// Edges between two planar faces that become bends.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bends: Vec<EdgeName>,
    /// The bends' inside radius in place of the rules'.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub radius: Option<P>,
}

impl<P> SheetConvertDef<P> {
    pub const TYPE: &'static str = "sheet_convert";
    pub const BASE_NAME: &'static str = "SheetConvert";
    map_params! {
        SheetConvertDef {
            rules: copy,
            body: copy,
            face: clone,
            bends: clone,
            radius: option,
        }
    }
}

impl FeatureInfo for SheetConvertDef {
    fn references(&self) -> References {
        let mut r = References::default();
        r.body(self.body);
        r.features.insert(self.rules);
        add_faces(&mut r, [&self.face]);
        for edge in &self.bends {
            r.features.extend(edge.features());
        }
        r
    }
    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        ctx.body(self.body)?;
        if !matches!(ctx.feature(self.rules)?.def, FeatureDef::SheetRules(_)) {
            return Err(format!("{} is not a sheet rules feature", self.rules));
        }
        for (i, edge) in self.bends.iter().enumerate() {
            if self.bends[..i].contains(edge) {
                return Err(crate::notice::placed(
                    "an edge is listed more than once",
                    crate::notice::NoticePlace::edge(edge.to_string()),
                    format!("edge {edge} is listed more than once"),
                ));
            }
        }
        for f in self.references().features {
            ctx.feature(f)?;
        }
        Ok(())
    }
    fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
        if let Some(r) = self.radius.map(value)
            && !(r.is_finite() && r > crate::sheet::TOLERANCE)
        {
            return Err(format!("the bend radius must be positive, got {r}"));
        }
        Ok(())
    }
    fn creates_bodies(&self) -> bool {
        true
    }
}

impl<K: Kernel> Evaluate<K> for SheetConvertDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let d = self.map_params(&mut |_, p| ctx.param(*p))?;
        crate::sheet::folded::convert(&d, ctx)
    }
}
