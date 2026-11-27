// SPDX-License-Identifier: MIT
//! Lofted Flange (mitcad#2512): a transition made directly as sheet
//! between two sketch profiles, both open or both closed (a rectangle to a
//! circle, a rectangle to another, a circle to another), for ducts, hoods
//! and hoppers. The profiles lie on the sheet's front; the rules'
//! thickness lies behind it, outwards of a closed transition (Flip: the
//! other side). Made on a press brake, the regions about corners and arcs
//! are flat facets folded at bend lines; die formed, they are formed
//! smooth. A closed transition is opened at a seam, a gap wide. The
//! geometry is in `sheet/folded/lofted.rs`.
//!
//! mitcad#2560: press brake folds may be bends of the rules' radius
//! (`bends`); die formed profiles without a developable surface between
//! them may be laid flat by triangulation (`development: triangulated`).

use super::extrude::ProfileRef;
use super::mapping::map_params;
use super::sheet_convert::{MAX_TRIANGLES, SheetDevelopment};
use super::{
    CheckContext, EvalContext, Evaluate, FeatureDef, FeatureInfo, FeatureOutput, References,
};
use crate::ids::{EntityUid, FeatureUid};
use crate::kernel::Kernel;
use crate::parameters::ParamId;
use crate::topo::RegionKey;
use serde::{Deserialize, Serialize};

/// How the transition is made.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoftedMethod {
    /// Flat facets folded at bend lines: each facet exact, so the blank is
    /// exact, with the bend lines and their angles.
    #[default]
    PressBrake,
    /// The smooth surface, formed in a die: planes and cones between
    /// profiles in parallel planes that turn one way (convex).
    DieFormed,
}

/// A profile of a Lofted Flange: a region of a sketch (its outer boundary)
/// or curves of a sketch joined end to end, open or closed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoftedProfile {
    pub sketch: FeatureUid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<RegionKey>,
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        with = "mitcad_sketch::internal::curves_serde"
    )]
    pub curves: Vec<EntityUid>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, bound(deserialize = "P: Deserialize<'de>"))]
pub struct SheetLoftedFlangeDef<P = ParamId> {
    pub rules: FeatureUid,
    /// The first and the second profile.
    pub profiles: Vec<LoftedProfile>,
    #[serde(default, skip_serializing_if = "is_press_brake")]
    pub method: LoftedMethod,
    /// Press brake: the facets of each region about a corner or an arc;
    /// none (and no chord tolerance): 4.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub facets: Option<P>,
    /// Press brake: the facets of each such region as few as keep the
    /// facets within this distance of the arcs (mm), in place of a count.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chord_tolerance: Option<P>,
    /// A closed transition's seam: where along the first profile (a
    /// fraction of its length from its start, 0 to 1); none: the shortest
    /// line between the profiles.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seam: Option<P>,
    /// The gap at the seam; none: the rules' thickness.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gap: Option<P>,
    /// The material on the other side of the profiles: inwards of a closed
    /// transition.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub flip: bool,
    /// Press brake: the folds are bends of the rules' bend radius, each a
    /// cylinder between the facets beside it (mitcad#2560); otherwise they
    /// are sharp.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub bends: bool,
    /// Die formed: how the transition is laid flat (mitcad#2560); none:
    /// `exact`, the profiles' developable surface. `triangulated` joins
    /// profiles that have none by a smooth ruled surface and lays it flat
    /// approximately by triangles, saying how far its lengths are off.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub development: Option<SheetDevelopment>,
    /// The triangles of a `triangulated` development; none: 64.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub triangles: Option<P>,
}

fn is_press_brake(method: &LoftedMethod) -> bool {
    *method == LoftedMethod::PressBrake
}

impl<P> SheetLoftedFlangeDef<P> {
    pub const TYPE: &'static str = "sheet_lofted_flange";
    pub const BASE_NAME: &'static str = "SheetLoftedFlange";
    map_params! {
        SheetLoftedFlangeDef {
            rules: copy,
            profiles: clone,
            method: copy,
            facets: option,
            chord_tolerance: option,
            seam: option,
            gap: option,
            flip: copy,
            bends: copy,
            development: copy,
            triangles: option,
        }
    }
}

impl LoftedProfile {
    /// The region reference, for the profile of a region.
    pub(crate) fn region_ref(&self) -> Option<ProfileRef> {
        self.region.clone().map(|region| ProfileRef {
            sketch: self.sketch,
            region,
        })
    }
}

impl FeatureInfo for SheetLoftedFlangeDef {
    fn references(&self) -> References {
        let mut features: std::collections::BTreeSet<FeatureUid> =
            self.profiles.iter().map(|p| p.sketch).collect();
        features.insert(self.rules);
        References {
            features,
            ..References::default()
        }
    }
    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        if !matches!(ctx.feature(self.rules)?.def, FeatureDef::SheetRules(_)) {
            return Err(format!("{} is not a sheet rules feature", self.rules));
        }
        if self.profiles.len() != 2 {
            return Err(format!(
                "a lofted flange runs between two profiles, got {}",
                self.profiles.len()
            ));
        }
        for (i, p) in self.profiles.iter().enumerate() {
            let which = if i == 0 { "first" } else { "second" };
            ctx.sketch(p.sketch)?;
            match (&p.region, p.curves.is_empty()) {
                (Some(_), false) => {
                    return Err(format!(
                        "the {which} profile is a region or curves of a sketch, not both"
                    ));
                }
                (None, true) => {
                    return Err(format!(
                        "the {which} profile needs a region or curves of a sketch"
                    ));
                }
                _ => {}
            }
            for (k, c) in p.curves.iter().enumerate() {
                if p.curves[..k].contains(c) {
                    return Err(format!(
                        "curve {} is listed more than once in the {which} profile",
                        c.curve_name()
                    ));
                }
            }
        }
        if self.facets.is_some() && self.chord_tolerance.is_some() {
            return Err(
                "a lofted flange's facets are given by a count or by a chord tolerance, not both"
                    .into(),
            );
        }
        if self.method == LoftedMethod::DieFormed
            && (self.facets.is_some() || self.chord_tolerance.is_some())
        {
            return Err(
                "a die formed lofted flange has no facets: its regions are formed smooth".into(),
            );
        }
        if self.method == LoftedMethod::DieFormed && self.bends {
            return Err(
                "a die formed lofted flange has no press brake bends: its regions are formed \
                 smooth"
                    .into(),
            );
        }
        let triangulated = self.development == Some(SheetDevelopment::Triangulated);
        if triangulated && self.method != LoftedMethod::DieFormed {
            return Err(
                "a press brake lofted flange is made of flat facets and lies flat exactly: the \
                 triangulated development is for a die formed one"
                    .into(),
            );
        }
        if self.triangles.is_some() && !triangulated {
            return Err(
                "the number of triangles is for a die formed lofted flange laid flat by \
                 triangulation"
                    .into(),
            );
        }
        Ok(())
    }
    fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
        if let Some(n) = self.facets.map(value)
            && !(n.is_finite() && (1.0..=1000.0).contains(&n) && n.fract() == 0.0)
        {
            return Err(format!(
                "the facets of a region must be a whole number from 1 to 1000, got {n}"
            ));
        }
        if let Some(c) = self.chord_tolerance.map(value)
            && !(c.is_finite() && c > crate::sheet::TOLERANCE)
        {
            return Err(format!("the chord tolerance must be positive, got {c}"));
        }
        if let Some(s) = self.seam.map(value)
            && !(s.is_finite() && (0.0..=1.0).contains(&s))
        {
            return Err(format!(
                "the seam is a fraction of the first profile's length, 0 to 1, got {s}"
            ));
        }
        if let Some(g) = self.gap.map(value)
            && !(g.is_finite() && g > crate::sheet::TOLERANCE)
        {
            return Err(format!("the seam's gap must be positive, got {g}"));
        }
        if let Some(n) = self.triangles.map(value)
            && !(n.fract() == 0.0 && (2.0..=MAX_TRIANGLES).contains(&n))
        {
            return Err(format!(
                "the number of triangles must be a whole number from 2 to {MAX_TRIANGLES}, got {n}"
            ));
        }
        Ok(())
    }
    fn creates_bodies(&self) -> bool {
        true
    }
}

impl<K: Kernel> Evaluate<K> for SheetLoftedFlangeDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let d = self.map_params(&mut |_, p| ctx.param(*p))?;
        crate::sheet::folded::lofted_flange(&d, ctx)
            .map_err(|e| crate::sheet::folded::worded(ctx, &e))
    }
}

#[cfg(test)]
mod tests {
    use crate::Document;
    use crate::test_support::def;
    use crate::testing::MockKernel;
    use serde_json::json;

    /// The triangulated development is refused in words for a press brake
    /// lofted flange, added or edited so (mitcad#2560; a script cannot
    /// expect a refused command, mitcad#2584).
    #[test]
    fn a_press_brake_transition_is_not_triangulated() {
        let mut doc = Document::new(MockKernel::default());
        let rules = doc
            .add_feature(
                &def(
                    json!({"type": "sheet_rules", "thickness": 2, "bend_radius": 3,
                    "allowance": {"type": "k_factor", "k": 0}, "reference_side": "front",
                    "bend_relief": {"type": "rectangular", "width": 2, "depth": 3},
                    "corner_relief": {"type": "round", "width": 2, "depth": 3}}),
                ),
                None,
            )
            .unwrap();
        let circle = |radius: f64| {
            json!({"type": "sketch", "plane": "xy",
                   "entities": [{"id": "p1", "type": "point", "at": [0, 0]},
                                {"id": "c2", "type": "circle", "center": "p1", "radius": radius}]})
        };
        let first = doc.add_feature(&def(circle(30.0)), None).unwrap();
        let second = doc.add_feature(&def(circle(20.0)), None).unwrap();
        let flange = |method: &str| {
            json!({"type": "sheet_lofted_flange", "rules": rules.uid.to_string(),
                   "method": method, "development": "triangulated",
                   "profiles": [{"sketch": first.uid.to_string(), "region": "r{c2}"},
                                {"sketch": second.uid.to_string(), "region": "r{c2}"}]})
        };
        let error = doc
            .add_feature(&def(flange("press_brake")), None)
            .unwrap_err();
        assert!(
            error.to_string().contains("for a die formed one"),
            "{error}"
        );
        let die_formed = doc.add_feature(&def(flange("die_formed")), None).unwrap();
        let error = doc
            .edit_feature(die_formed.uid, &def(flange("press_brake")))
            .unwrap_err();
        assert!(
            error.to_string().contains("for a die formed one"),
            "{error}"
        );
    }
}
