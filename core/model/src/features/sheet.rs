// SPDX-License-Identifier: MIT
//! Fabrication rules and a base sheet built from named bounded profiles.

use super::mapping::map_params;
use super::{
    BodyChange, CheckContext, EvalContext, Evaluate, FeatureDef, FeatureInfo, FeatureOutput,
    ProfileRef, References, SketchOutput,
};
use crate::ids::{BodyUid, FeatureUid};
use crate::kernel::Kernel;
use crate::parameters::ParamId;
use crate::profile::ProfileRegion;
use crate::sheet::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::marker::PhantomData;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Allowance<P> {
    KFactor { k: P },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReliefKind {
    Rectangular,
    Round,
    Tear,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Relief<P> {
    #[serde(rename = "type")]
    pub kind: ReliefKind,
    pub width: P,
    pub depth: P,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SheetRulesDef<P = ParamId> {
    pub thickness: P,
    pub bend_radius: P,
    pub allowance: Allowance<P>,
    pub reference_side: SheetReferenceSide,
    pub bend_relief: Relief<P>,
    pub corner_relief: Relief<P>,
}

impl<P> SheetRulesDef<P> {
    pub const TYPE: &'static str = "sheet_rules";
    pub const BASE_NAME: &'static str = "SheetRules";
    // Written by hand: the allowance and the reliefs map under nested slots.
    pub fn map_params<Q, E>(
        &self,
        f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
    ) -> Result<SheetRulesDef<Q>, E> {
        let Allowance::KFactor { k } = &self.allowance;
        Ok(SheetRulesDef {
            thickness: f("thickness", &self.thickness)?,
            bend_radius: f("bend_radius", &self.bend_radius)?,
            allowance: Allowance::KFactor {
                k: f("allowance.k", k)?,
            },
            reference_side: self.reference_side,
            bend_relief: Relief {
                kind: self.bend_relief.kind,
                width: f("bend_relief.width", &self.bend_relief.width)?,
                depth: f("bend_relief.depth", &self.bend_relief.depth)?,
            },
            corner_relief: Relief {
                kind: self.corner_relief.kind,
                width: f("corner_relief.width", &self.corner_relief.width)?,
                depth: f("corner_relief.depth", &self.corner_relief.depth)?,
            },
        })
    }
}

impl SheetRulesDef<f64> {
    fn validate(&self) -> Result<(), String> {
        self.map_params(&mut |slot, value| {
            let good = value.is_finite()
                && if slot == "allowance.k" {
                    (0.0..=1.0).contains(value)
                } else {
                    *value > TOLERANCE
                };
            if good {
                Ok(*value)
            } else {
                Err(format!("invalid sheet rule {slot}: {value}"))
            }
        })
        .map(|_| ())
    }
}

impl FeatureInfo for SheetRulesDef {
    fn references(&self) -> References {
        References::default()
    }
    fn check(&self, _ctx: &CheckContext<'_>) -> Result<(), String> {
        Ok(())
    }
    fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
        self.map_params(&mut |_, id| Ok::<_, String>(value(*id)))?
            .validate()
    }
}

impl<K: Kernel> Evaluate<K> for SheetRulesDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let rules = self.map_params(&mut |_, id| ctx.param(*id))?;
        rules.validate()?;
        Ok(FeatureOutput {
            sheet_rules: Some(rules),
            ..FeatureOutput::default()
        })
    }
}

/// Settled semantic allocation. Removed references remain as tombstones so
/// that adding another region never reuses a previous panel's identity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PanelSlot {
    pub profile: ProfileRef,
    pub slot: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SheetFaceDef<P = ParamId> {
    pub rules: FeatureUid,
    #[serde(deserialize_with = "super::extrude::profile_set")]
    pub profiles: Vec<ProfileRef>,
    /// A face joined to this sheet body (mitcad#242): the regions lie on
    /// the front or back surface of one of its planar panels and are added
    /// to that panel's material, through its thickness. Without it the
    /// face makes new bodies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<BodyUid>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub panel_slots: Vec<PanelSlot>,
    #[serde(skip)]
    pub marker: PhantomData<P>,
}

impl<P> SheetFaceDef<P> {
    pub const TYPE: &'static str = "sheet_face";
    pub const BASE_NAME: &'static str = "SheetFace";
    map_params! {
        SheetFaceDef {
            rules: copy,
            profiles: clone,
            body: copy,
            panel_slots: clone,
            marker: phantom,
        }
    }

    pub(crate) fn settle(&mut self, old: Option<&Self>) {
        if let Some(old) = old {
            self.panel_slots = old.panel_slots.clone();
        }
        let mut missing: Vec<_> = self
            .profiles
            .iter()
            .filter(|p| !self.panel_slots.iter().any(|s| &s.profile == *p))
            .cloned()
            .collect();
        missing.sort_by_key(|p| (p.sketch, p.region.to_string()));
        let mut next = self
            .panel_slots
            .iter()
            .map(|s| s.slot)
            .max()
            .map_or(0, |s| s.saturating_add(1));
        for profile in missing {
            self.panel_slots.push(PanelSlot {
                profile,
                slot: next,
            });
            next = next.saturating_add(1);
        }
    }
}

impl FeatureInfo for SheetFaceDef {
    fn references(&self) -> References {
        let mut refs = References {
            features: self
                .profiles
                .iter()
                .map(|p| p.sketch)
                .chain([self.rules])
                .collect(),
            ..References::default()
        };
        if let Some(body) = self.body {
            refs.body(body);
        }
        refs
    }
    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        super::extrude::check_profiles(ctx, &self.profiles)?;
        if let Some(body) = self.body {
            ctx.body(body)?;
        }
        if !matches!(ctx.feature(self.rules)?.def, FeatureDef::SheetRules(_)) {
            return Err(format!("{} is not a sheet rules feature", self.rules));
        }
        let mut slots = BTreeSet::new();
        for (i, slot) in self.panel_slots.iter().enumerate() {
            if !slots.insert(slot.slot)
                || self.panel_slots[..i]
                    .iter()
                    .any(|s| s.profile == slot.profile)
            {
                return Err("sheet panel allocation contains duplicate identities".into());
            }
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

/// What a sheet profile reference does with a region a sketch change split
/// ([`sheet_profile_regions`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SplitProfile {
    /// Fail, naming the pieces, unless every piece besides the main one is
    /// selected by a reference of its own (a base Face: each panel's
    /// identity is the slot settled in the definition for its reference).
    Refuse,
    /// Use every piece (a joined Face: its material has no panel identity).
    Pieces,
}

/// The evaluated sketch of sheet profile references and the regions they
/// find, each with the reference whose slot it keeps.
pub(crate) type SheetProfilesOf<'a> = (Arc<SketchOutput>, Vec<(&'a ProfileRef, ProfileRegion)>);

/// The sketch of sheet profile references (all in one sketch).
fn sketch_uid_of(profiles: &[ProfileRef]) -> Result<FeatureUid, String> {
    Ok(profiles.first().ok_or("no profiles selected")?.sketch)
}

/// The regions sheet profile references find in their evaluated sketch
/// (mitcad#1444), as Extrude finds its profiles
/// ([`super::extrude::profile_regions`], [`super::SketchOutput::found`]):
/// a reference finds its region by its key; when a sketch change altered
/// the key (a corner rounded, a curve replaced or moved), the region inside
/// the outline its key traces now, else the one with the most curves in
/// common, with the warning `region r{…} found as r{…} after a change in
/// Sketch1`. The region found carries the reference's key, so each region
/// is returned with the reference whose panel slot and face names it keeps.
///
/// A reference whose region the change split into pieces: with `split`
/// [`SplitProfile::Pieces`] every piece is used, the main one under the
/// reference's key. With [`SplitProfile::Refuse`] the main piece keeps the
/// reference's slot only when each other piece is the region another
/// reference finds (the pieces selected since, each settled with a slot of
/// its own); else the Face fails, naming the pieces and asking for the
/// profiles to be selected again, rather than give a piece a slot at
/// evaluation that a later edit could hand to another region. A reference
/// that finds no region fails as Extrude does (`Sketch1 has no profile
/// r{…}`). Two references that find the same region (an old key and the
/// current one) use it once, under the one that names it exactly.
pub(crate) fn sheet_profile_regions<'a, K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    profiles: &'a [ProfileRef],
    split: SplitProfile,
) -> Result<SheetProfilesOf<'a>, String> {
    let sketch_uid = sketch_uid_of(profiles)?;
    let sketch = ctx.sketch(sketch_uid)?;
    let name = ctx.feature_name(sketch_uid);
    let mut all = Vec::with_capacity(profiles.len());
    for profile in profiles {
        let regions = sketch
            .found(&profile.region)
            .ok_or_else(|| format!("{name} has no profile {}", profile.region))?;
        all.push((profile, regions));
    }
    if split == SplitProfile::Refuse {
        for (profile, regions) in &all {
            let unselected = regions.regions.iter().skip(1).any(|piece| {
                !all.iter()
                    .any(|(_, other)| other.regions[0].profile.loops == piece.profile.loops)
            });
            if unselected {
                return Err(format!(
                    "region {} is split into {region_names} after a change in {name}; select \
                     the profiles again",
                    profile.region,
                    region_names = regions.keys()
                ));
            }
        }
    }
    // Each region with its reference and whether that reference names it
    // by the region's own key.
    let mut found: Vec<(&ProfileRef, ProfileRegion, bool)> = Vec::new();
    for (profile, regions) in all {
        if let Some(warning) = regions.warning(&profile.region, &name) {
            let place =
                crate::notice::NoticePlace::region_of(profile.sketch, profile.region.to_string());
            ctx.warn_at(warning, vec![place]);
        }
        let mut pieces = regions.profiles(&profile.region);
        if split == SplitProfile::Refuse {
            // The other pieces are the regions of references of their own
            // (checked above), whose slots they take.
            pieces.truncate(1);
        }
        for region in pieces {
            match found.iter_mut().find(|(_, r, _)| r.loops == region.loops) {
                Some(known) => {
                    if regions.exact && !known.2 {
                        *known = (profile, region, true);
                    }
                }
                None => found.push((profile, region, regions.exact)),
            }
        }
    }
    let found = found
        .into_iter()
        .map(|(profile, region, _)| (profile, region))
        .collect();
    Ok((sketch, found))
}

impl<K: Kernel> Evaluate<K> for SheetFaceDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        if let Some(body) = self.body {
            return crate::sheet::folded::join_face(self, body, ctx);
        }
        let rules = ctx.sheet_rules(self.rules)?;
        let (sketch, found) = sheet_profile_regions(ctx, &self.profiles, SplitProfile::Refuse)?;
        let sketch_uid = sketch_uid_of(&self.profiles)?;
        let mut settled = self.clone();
        settled.settle(None);
        let mut panels = Vec::new();
        for (profile, region) in &found {
            // A region found under another key keeps the panel slot of its
            // reference, so that the panel's identity and names stay.
            let slot = settled
                .panel_slots
                .iter()
                .find(|s| s.profile == **profile)
                .expect("settled")
                .slot;
            panels.push(SheetPlanarPatch {
                id: SheetDomainId {
                    feature: ctx.uid,
                    slot,
                },
                frame: sketch.frame,
                region: SheetRegion {
                    loops: region
                        .loops
                        .iter()
                        .map(|l| SheetLoop {
                            segments: l
                                .segments
                                .iter()
                                .map(|s| SheetSegment {
                                    id: SheetCurveId::Sketch {
                                        sketch: sketch_uid,
                                        segment: s.key.clone(),
                                    },
                                    geometry: s.geometry.clone(),
                                })
                                .collect(),
                        })
                        .collect(),
                },
            });
        }
        panels.sort_by_key(|p| p.id);
        let charts: Vec<_> = panels
            .iter()
            .map(|p| SheetPatch::from_legacy(p, rules.value.thickness, rules.value.reference_side))
            .collect::<Result<_, _>>()?;
        let joins = crate::sheet::planar_joins(&charts)?;
        let spec = SheetShapeSpec {
            thickness: rules.value.thickness,
            patches: &charts,
            ports: &[],
            tolerance: TOLERANCE,
        };
        let geometry = ctx
            .kernel
            .sheet_shape(&spec, &ctx.uid.to_string())
            .map_err(|e| e.to_string())?;
        if geometry.solids.is_empty() {
            return Err("sheet Face produced no solid".into());
        }
        let owner = ctx
            .env
            .features
            .iter()
            .find(|f| f.uid == ctx.uid)
            .expect("current feature")
            .component;
        let mut output = FeatureOutput::default();
        let mut assigned = BTreeSet::new();
        for solid in geometry.solids {
            if solid.membership.is_empty()
                || solid.membership.iter().any(|member| {
                    member.region != 0
                        || !charts.iter().any(|p| p.domain == member.domain)
                        || !assigned.insert(*member)
                })
            {
                return Err("sheet kernel returned ambiguous domain membership".into());
            }
            let slot = solid
                .membership
                .iter()
                .map(|member| member.domain.slot())
                .min()
                .expect("not empty");
            let body = BodyUid::new(ctx.uid, slot);
            output.changes.push(BodyChange::Set(body, solid.shape));
            output.sheets.insert(
                body,
                SheetChange::Set(SheetState {
                    schema: 2,
                    algorithm: ALGORITHM_VERSION,
                    body,
                    owner,
                    shape_generation: String::new(),
                    rules: self.rules,
                    rules_version: format!("{:032x}", rules.version),
                    panels: panels
                        .iter()
                        .filter(|p| {
                            solid
                                .membership
                                .iter()
                                .any(|m| m.domain == SheetDomainRef::Panel(p.id))
                        })
                        .cloned()
                        .collect(),
                    charts: charts
                        .iter()
                        .filter(|p| solid.membership.iter().any(|m| m.domain == p.domain))
                        .cloned()
                        .collect(),
                    bends: Vec::new(),
                    joins: joins
                        .iter()
                        .filter(|j| {
                            [j.first.boundary.domain, j.second.boundary.domain]
                                .iter()
                                .all(|domain| solid.membership.iter().any(|m| m.domain == *domain))
                        })
                        .cloned()
                        .collect(),
                    opened: Vec::new(),
                    lineage: Vec::new(),
                    overlays: Vec::new(),
                    invalidated: None,
                    blockers: Vec::new(),
                }),
            );
        }
        if assigned.len() != panels.len() {
            return Err("sheet kernel omitted a material domain".into());
        }
        for message in geometry.diagnostics {
            ctx.warn(message);
        }
        Ok(output)
    }
}
