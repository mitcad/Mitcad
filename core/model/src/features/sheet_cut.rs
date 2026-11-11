// SPDX-License-Identifier: MIT
//! Thickness-normal sheet Cut across bends (mitcad#208, mitcad#209): profiles
//! of one or several sketches on the fixed panel cut every chart developed into
//! its plane; material the cut disconnects becomes several bodies. Mode
//! `straight` (mitcad#242) cuts the folded sheet straight along the sketch's
//! normal instead.

use super::extrude::ProfileRef;
use super::{CheckContext, EvalContext, Evaluate, FeatureInfo, FeatureOutput, References};
use crate::ids::BodyUid;
use crate::kernel::Kernel;
use crate::parameters::ParamId;
use crate::sheet::{BendId, PanelId};
use serde::{Deserialize, Serialize};

/// How the cut meets the material (mitcad#242).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SheetCutMode {
    /// The sketch cuts the developed material: through a bend the cut
    /// follows the bend, its walls normal to the material.
    #[default]
    Developed,
    /// The profiles swept along the sketch's normal cut the folded sheet,
    /// as an extrusion would: through a bend or an inclined panel the walls
    /// follow the sweep.
    Straight,
}

/// How far a straight cut goes along its direction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum SheetCutExtent<P> {
    Distance { distance: P },
    ThroughAll,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, bound(deserialize = "P: Deserialize<'de>"))]
pub struct SheetCutDef<P = ParamId> {
    pub body: BodyUid,
    /// The panel whose developed plane the sketch lies in; it stays fixed.
    /// A straight cut has none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stationary: Option<PanelId>,
    /// Bends developed to interpret the sketch; the input configuration is
    /// restored afterwards. Bends already open stay open.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub open_bends: Vec<BendId>,
    #[serde(deserialize_with = "super::extrude::profile_set")]
    pub profiles: Vec<ProfileRef>,
    #[serde(default, skip_serializing_if = "is_developed")]
    pub mode: SheetCutMode,
    /// A straight cut's reach along the sketch's normal (`flip`: against
    /// it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extent: Option<SheetCutExtent<P>>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub flip: bool,
}

fn is_developed(mode: &SheetCutMode) -> bool {
    *mode == SheetCutMode::Developed
}

impl<P> SheetCutDef<P> {
    pub const TYPE: &'static str = "sheet_cut";
    pub const BASE_NAME: &'static str = "SheetCut";
    // Written by hand: the extent maps by its kind (`extent.distance`).
    pub fn map_params<Q, E>(
        &self,
        f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
    ) -> Result<SheetCutDef<Q>, E> {
        Ok(SheetCutDef {
            body: self.body,
            stationary: self.stationary,
            open_bends: self.open_bends.clone(),
            profiles: self.profiles.clone(),
            mode: self.mode,
            extent: match &self.extent {
                Some(SheetCutExtent::Distance { distance }) => Some(SheetCutExtent::Distance {
                    distance: f("extent.distance", distance)?,
                }),
                Some(SheetCutExtent::ThroughAll) => Some(SheetCutExtent::ThroughAll),
                None => None,
            },
            flip: self.flip,
        })
    }
}

impl FeatureInfo for SheetCutDef {
    fn references(&self) -> References {
        let mut r = References::default();
        r.body(self.body);
        r.features.extend(self.stationary.map(|s| s.feature));
        r.features.extend(self.open_bends.iter().map(|b| b.feature));
        r.features.extend(self.profiles.iter().map(|p| p.sketch));
        r
    }
    fn check(&self, c: &CheckContext<'_>) -> Result<(), String> {
        if self.profiles.is_empty() {
            return Err("no profiles selected".into());
        }
        match self.mode {
            SheetCutMode::Developed => {
                if self.stationary.is_none() {
                    return Err("a developed sheet cut names its stationary panel".into());
                }
                if self.extent.is_some() || self.flip {
                    return Err(
                        "a developed sheet cut goes through the material; it has no \
                                extent or flip"
                            .into(),
                    );
                }
            }
            SheetCutMode::Straight => {
                if self.extent.is_none() {
                    return Err("a straight sheet cut needs an extent".into());
                }
                if self.stationary.is_some() || !self.open_bends.is_empty() {
                    return Err("a straight sheet cut opens no bends and holds no panel".into());
                }
                if self
                    .profiles
                    .iter()
                    .any(|p| p.sketch != self.profiles[0].sketch)
                {
                    return Err("a straight sheet cut's profiles lie in one sketch".into());
                }
            }
        }
        // Several sketches may contribute; each must lie on the fixed panel.
        for profile in &self.profiles {
            c.profile_sketch(profile.sketch)?;
        }
        for (i, bend) in self.open_bends.iter().enumerate() {
            if self.open_bends[..i].contains(bend) {
                return Err(format!(
                    "bend {} slot {} is selected more than once",
                    bend.feature, bend.slot
                ));
            }
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
    fn profiles(&self) -> &[ProfileRef] {
        &self.profiles
    }
}

impl<K: Kernel> Evaluate<K> for SheetCutDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let d = self.map_params(&mut |_, p| ctx.param(*p))?;
        match d.mode {
            SheetCutMode::Developed => crate::sheet::folded::cut(&d, ctx),
            SheetCutMode::Straight => crate::sheet::folded::cut::straight_cut(&d, ctx),
        }
    }
}
