// SPDX-License-Identifier: MIT
//! Corner relief at the junction of two bends (mitcad#209). The relief is a
//! thickness-normal cut of the generated contour in the plane of the panel
//! both bends leave (or, without a common panel, of a panel next to one of
//! them; mitcad#125), with the bends between developed; its size defaults to
//! the rules' corner relief.

use super::sheet::Relief;
use super::{CheckContext, EvalContext, Evaluate, FeatureInfo, FeatureOutput, References};
use crate::ids::BodyUid;
use crate::kernel::Kernel;
use crate::parameters::ParamId;
use crate::sheet::{BendId, PanelId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, bound(deserialize = "P: Deserialize<'de>"))]
pub struct SheetCornerReliefDef<P = ParamId> {
    pub body: BodyUid,
    /// The two bends whose junction is relieved.
    pub bends: Vec<BendId>,
    /// The panel held while the junction is developed: one both bends leave,
    /// needed only when they share several; without a common panel, one
    /// joining either bend (default: the panel next to the first bend on the
    /// seam path to the second).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub panel: Option<PanelId>,
    /// Overrides the rules' corner relief.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relief: Option<Relief<P>>,
}

impl<P> SheetCornerReliefDef<P> {
    pub const TYPE: &'static str = "sheet_corner_relief";
    pub const BASE_NAME: &'static str = "SheetCornerRelief";
    // Written by hand: the relief maps under nested slots (`relief.width`).
    pub fn map_params<Q, E>(
        &self,
        f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
    ) -> Result<SheetCornerReliefDef<Q>, E> {
        Ok(SheetCornerReliefDef {
            body: self.body,
            bends: self.bends.clone(),
            panel: self.panel,
            relief: self
                .relief
                .as_ref()
                .map(|r| {
                    Ok(Relief {
                        kind: r.kind,
                        width: f("relief.width", &r.width)?,
                        depth: f("relief.depth", &r.depth)?,
                    })
                })
                .transpose()?,
        })
    }
}

impl FeatureInfo for SheetCornerReliefDef {
    fn references(&self) -> References {
        let mut r = References::default();
        r.body(self.body);
        r.features.extend(self.bends.iter().map(|b| b.feature));
        r.features.extend(self.panel.map(|p| p.feature));
        r
    }
    fn check(&self, c: &CheckContext<'_>) -> Result<(), String> {
        if self.bends.len() != 2 {
            return Err("a corner relief names exactly two bends".into());
        }
        if self.bends[0] == self.bends[1] {
            return Err(
                crate::notice::FeatureError::new("a bend is selected more than once")
                    .with_detail(format!(
                        "bend {} slot {} is selected more than once",
                        self.bends[0].feature, self.bends[0].slot
                    ))
                    .text(),
            );
        }
        c.body(self.body)?;
        for f in self.references().features {
            c.feature(f)?;
        }
        Ok(())
    }
    fn creates_bodies(&self) -> bool {
        true
    }
}

impl<K: Kernel> Evaluate<K> for SheetCornerReliefDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let d = self.map_params(&mut |_, p| ctx.param(*p))?;
        crate::sheet::folded::corner_relief(&d, ctx)
    }
}
