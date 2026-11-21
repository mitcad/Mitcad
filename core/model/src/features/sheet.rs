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
use crate::sheet::process::SheetProcess;
use crate::sheet::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::marker::PhantomData;
use std::sync::Arc;

/// How a fold's developed length is worked out (mitcad#1939, commands.md
/// "Sheet process profiles"). Every method gives the fold an effective
/// K-factor, the neutral surface's place in the thickness, so that the
/// fold engine develops each the same way; a measured allowance or
/// deduction can put it outside the material (beyond K = 1), as a nearly
/// sharp crease needs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "snake_case",
    deny_unknown_fields,
    bound(deserialize = "P: Deserialize<'de>")
)]
pub enum Allowance<P> {
    /// The neutral surface `k` of the thickness from the inside (0 to 1).
    KFactor { k: P },
    /// A measured developed length of a 90 degree fold; other angles in
    /// proportion (the same neutral radius).
    BendAllowance { allowance: P },
    /// A measured bend deduction of a 90 degree fold: the allowance is
    /// twice the outside setback `r + t` less it.
    BendDeduction { deduction: P },
    /// A board's crease table: the row of the sheet's thickness (the
    /// nearest), flute, fold direction and side gives the fold's measured
    /// allowance or deduction. As a fold's own allowance with no rows, it
    /// picks the rules' rows in its `direction` (a fold along the flutes).
    CreaseTable {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        flute: Option<String>,
        #[serde(default, skip_serializing_if = "CreaseDirection::is_any")]
        direction: CreaseDirection,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        rows: Vec<CreaseRow<P>>,
    },
    /// A kerf bend (a living hinge in laser sheet): the neutral surface
    /// in the middle (K = 0.5), and a slit lattice across the bend in the
    /// flat pattern instead of reliefs; the radius at least the lattice's
    /// minimum ([`SlitLattice::min_radius`]). The slit width is the rules'
    /// kerf unless given.
    KerfBend {
        slit_length: P,
        gap: P,
        pitch: P,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        slit_width: Option<P>,
    },
}

/// Which way a crease runs to the board's flutes (or grain).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CreaseDirection {
    /// Any direction (a row that matches every fold).
    #[default]
    Any,
    /// Along the flutes.
    Along,
    /// Across the flutes.
    Across,
}

impl CreaseDirection {
    pub fn is_any(&self) -> bool {
        *self == CreaseDirection::Any
    }
    fn words(self) -> &'static str {
        match self {
            CreaseDirection::Any => "in any direction",
            CreaseDirection::Along => "along the flutes",
            CreaseDirection::Across => "across the flutes",
        }
    }
}

/// Which side of the fold the sheet's reference (front) surface ends up
/// on: `inner` when the fold closes towards it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CreaseSide {
    #[default]
    Any,
    Inner,
    Outer,
}

impl CreaseSide {
    pub fn is_any(&self) -> bool {
        *self == CreaseSide::Any
    }
}

/// A row of a crease table: a measured allowance or deduction of a 90
/// degree fold of a board `thickness` thick, of a flute (any when left
/// out), in a direction and on a side. Exactly one of `allowance` and
/// `deduction`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, bound(deserialize = "P: Deserialize<'de>"))]
pub struct CreaseRow<P> {
    pub thickness: P,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flute: Option<String>,
    #[serde(default, skip_serializing_if = "CreaseDirection::is_any")]
    pub direction: CreaseDirection,
    #[serde(default, skip_serializing_if = "CreaseSide::is_any")]
    pub side: CreaseSide,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowance: Option<P>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deduction: Option<P>,
}

impl<P> Allowance<P> {
    /// The allowance with its values mapped, slots under `prefix`
    /// (`allowance.k`, `allowance.rows[0].deduction`, ...).
    pub fn map<Q, E>(
        &self,
        prefix: &str,
        f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
    ) -> Result<Allowance<Q>, E> {
        let slot = |name: &str| format!("{prefix}.{name}");
        Ok(match self {
            Allowance::KFactor { k } => Allowance::KFactor {
                k: f(&slot("k"), k)?,
            },
            Allowance::BendAllowance { allowance } => Allowance::BendAllowance {
                allowance: f(&slot("allowance"), allowance)?,
            },
            Allowance::BendDeduction { deduction } => Allowance::BendDeduction {
                deduction: f(&slot("deduction"), deduction)?,
            },
            Allowance::CreaseTable {
                flute,
                direction,
                rows,
            } => Allowance::CreaseTable {
                flute: flute.clone(),
                direction: *direction,
                rows: rows
                    .iter()
                    .enumerate()
                    .map(|(i, row)| {
                        let at = |name: &str| format!("{prefix}.rows[{i}].{name}");
                        Ok(CreaseRow {
                            thickness: f(&at("thickness"), &row.thickness)?,
                            flute: row.flute.clone(),
                            direction: row.direction,
                            side: row.side,
                            allowance: row
                                .allowance
                                .as_ref()
                                .map(|v| f(&at("allowance"), v))
                                .transpose()?,
                            deduction: row
                                .deduction
                                .as_ref()
                                .map(|v| f(&at("deduction"), v))
                                .transpose()?,
                        })
                    })
                    .collect::<Result<_, E>>()?,
            },
            Allowance::KerfBend {
                slit_length,
                gap,
                pitch,
                slit_width,
            } => Allowance::KerfBend {
                slit_length: f(&slot("slit_length"), slit_length)?,
                gap: f(&slot("gap"), gap)?,
                pitch: f(&slot("pitch"), pitch)?,
                slit_width: slit_width
                    .as_ref()
                    .map(|v| f(&slot("slit_width"), v))
                    .transpose()?,
            },
        })
    }

    /// Its type as commands write it.
    pub fn kind(&self) -> &'static str {
        match self {
            Allowance::KFactor { .. } => "k_factor",
            Allowance::BendAllowance { .. } => "bend_allowance",
            Allowance::BendDeduction { .. } => "bend_deduction",
            Allowance::CreaseTable { .. } => "crease_table",
            Allowance::KerfBend { .. } => "kerf_bend",
        }
    }
}

/// The slot a value of an allowance may hold: `allowance.k` in `[0,1]`,
/// a deduction any finite value (a measured deduction may be 0 or below),
/// every other value positive.
fn allowance_value_ok(slot: &str, value: f64) -> bool {
    value.is_finite()
        && if slot.ends_with(".k") {
            (0.0..=1.0).contains(&value)
        } else if slot.ends_with(".deduction") {
            true
        } else {
            value > TOLERANCE
        }
}

/// A fold's effective allowance: the K-factor that places its neutral
/// surface and, for a kerf bend, its slit lattice.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FoldAllowance {
    pub k: f64,
    /// The inside radius, raised to a kerf bend's lattice minimum when
    /// the fold takes the rules' radius.
    pub radius: f64,
    pub lattice: Option<SlitLattice>,
}

impl SheetRulesDef<f64> {
    /// The rules with a fold's own allowance in place of theirs (its
    /// per-bend override): a crease table without rows takes the rules'
    /// rows in its direction (and flute, when it names one).
    pub fn with_fold_allowance(&self, own: Option<&Allowance<f64>>) -> SheetRulesDef<f64> {
        let mut rules = self.clone();
        match (own, &self.allowance) {
            (None, _) => {}
            (
                Some(Allowance::CreaseTable {
                    flute,
                    direction,
                    rows,
                }),
                Allowance::CreaseTable {
                    flute: rules_flute,
                    rows: rules_rows,
                    ..
                },
            ) if rows.is_empty() => {
                rules.allowance = Allowance::CreaseTable {
                    flute: flute.clone().or_else(|| rules_flute.clone()),
                    direction: *direction,
                    rows: rules_rows.clone(),
                };
            }
            (Some(own), _) => rules.allowance = own.clone(),
        }
        rules
    }

    /// The effective allowance of a fold of `angle` (signed: a negative
    /// angle closes towards the front) with the inside radius `radius`
    /// (the rules' when None). A measured value is of a 90 degree fold:
    /// the neutral radius `allowance / (pi / 2)` gives the K-factor
    /// `(neutral - r) / t`, used for every angle. Refused, in words, where
    /// no neutral surface results (a deduction too large for the radius and
    /// thickness), where a crease table has no row for the fold, and a kerf
    /// bend tighter than its lattice allows.
    pub fn fold_allowance(&self, angle: f64, radius: Option<f64>) -> Result<FoldAllowance, String> {
        let t = self.thickness;
        let given = radius.unwrap_or(self.bend_radius);
        let quarter = std::f64::consts::FRAC_PI_2;
        let from_measured = |allowance: Option<f64>,
                             deduction: Option<f64>,
                             r: f64|
         -> Result<f64, String> {
            let developed = match (allowance, deduction) {
                (Some(a), None) => a,
                (None, Some(d)) => 2.0 * (r + t) - d,
                _ => return Err("a crease row needs an allowance or a deduction, not both".into()),
            };
            let neutral = developed / quarter;
            if neutral.is_nan() || neutral <= TOLERANCE || !developed.is_finite() {
                return Err(format!(
                    "the measured fold gives no bend allowance: a 90 degree fold of {} mm \
                     with an inside radius of {} mm must develop to more than 0 mm",
                    short(t),
                    short(r)
                ));
            }
            Ok((neutral - r) / t)
        };
        match &self.allowance {
            Allowance::KFactor { k } => Ok(FoldAllowance {
                k: *k,
                radius: given,
                lattice: None,
            }),
            Allowance::BendAllowance { allowance } => Ok(FoldAllowance {
                k: from_measured(Some(*allowance), None, given)?,
                radius: given,
                lattice: None,
            }),
            Allowance::BendDeduction { deduction } => Ok(FoldAllowance {
                k: from_measured(None, Some(*deduction), given)?,
                radius: given,
                lattice: None,
            }),
            Allowance::CreaseTable {
                flute,
                direction,
                rows,
            } => {
                let side = if angle < 0.0 {
                    CreaseSide::Inner
                } else {
                    CreaseSide::Outer
                };
                let flute = flute.as_deref().or(self.flute.as_deref());
                let row = crease_row(rows, t, flute, *direction, side).ok_or_else(|| {
                    format!(
                        "the crease table has no row for a fold of {} mm board{} {} with the \
                         front {} the fold",
                        short(t),
                        flute.map(|f| format!(" of flute {f}")).unwrap_or_default(),
                        direction.words(),
                        if side == CreaseSide::Inner {
                            "inside"
                        } else {
                            "outside"
                        }
                    )
                })?;
                Ok(FoldAllowance {
                    k: from_measured(row.allowance, row.deduction, given)?,
                    radius: given,
                    lattice: None,
                })
            }
            Allowance::KerfBend {
                slit_length,
                gap,
                pitch,
                slit_width,
            } => {
                let width = slit_width.or(self.kerf).unwrap_or(0.0);
                if width.is_nan() || width <= TOLERANCE {
                    return Err(
                        "a kerf bend needs a slit width: set the rules' kerf or the lattice's \
                         slit width"
                            .into(),
                    );
                }
                if *pitch <= width {
                    return Err(format!(
                        "the kerf bend's pitch ({} mm) must be larger than its slit width ({} mm)",
                        short(*pitch),
                        short(width)
                    ));
                }
                let lattice = SlitLattice {
                    slit_length: *slit_length,
                    gap: *gap,
                    pitch: *pitch,
                    slit_width: width,
                };
                let least = lattice.min_radius(t);
                let radius = match radius {
                    Some(r) if r + 1e-9 < least => {
                        return Err(format!(
                            "the kerf bend's radius ({} mm) is smaller than its slit lattice \
                             bends to ({} mm): space the slits closer or widen them",
                            short(r),
                            short(least)
                        ));
                    }
                    Some(r) => r,
                    None => given.max(least),
                };
                Ok(FoldAllowance {
                    k: 0.5,
                    radius,
                    lattice: Some(lattice),
                })
            }
        }
    }
}

/// The row of a crease table for a fold: among the rows whose flute,
/// direction and side match (a row's `any` and missing flute match
/// everything), the one of the nearest thickness, an exact match of more
/// fields first.
fn crease_row<'a>(
    rows: &'a [CreaseRow<f64>],
    thickness: f64,
    flute: Option<&str>,
    direction: CreaseDirection,
    side: CreaseSide,
) -> Option<&'a CreaseRow<f64>> {
    rows.iter()
        .filter(|row| {
            row.flute
                .as_deref()
                .is_none_or(|f| flute.is_some_and(|g| g.eq_ignore_ascii_case(f)))
                && (row.direction.is_any() || row.direction == direction)
                && (row.side.is_any() || row.side == side)
        })
        .min_by(|a, b| {
            let general = |row: &CreaseRow<f64>| {
                usize::from(row.flute.is_none())
                    + usize::from(row.direction.is_any())
                    + usize::from(row.side.is_any())
            };
            (a.thickness - thickness)
                .abs()
                .total_cmp(&(b.thickness - thickness).abs())
                .then(general(a).cmp(&general(b)))
        })
}

fn short(value: f64) -> String {
    super::bearing_sizes::short_number(value)
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
#[serde(deny_unknown_fields, bound(deserialize = "P: Deserialize<'de>"))]
pub struct SheetRulesDef<P = ParamId> {
    pub thickness: P,
    pub bend_radius: P,
    pub allowance: Allowance<P>,
    pub reference_side: SheetReferenceSide,
    pub bend_relief: Relief<P>,
    pub corner_relief: Relief<P>,
    /// The part's process profile (mitcad#1939): metal when left out,
    /// unless the material of the part says otherwise
    /// ([`crate::sheet::process`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process: Option<SheetProcess>,
    /// The width the cutting tool (a laser's beam) takes away, mm: kerf
    /// compensation of cut sheets, a kerf bend's slit width; none: 0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kerf: Option<P>,
    /// The board's flute (`A`, `B`, `C`, `E`, `F`, `BC`, ...) that crease
    /// tables match; none for a sheet without flutes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flute: Option<String>,
    /// The options of the standard box these rules were made for
    /// (mitcad#1939, `add_standard_box`): its style, sizes, board and the
    /// features it made, which `edit_standard_box` changes together.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub standard_box: Option<serde_json::Value>,
}

impl<P> SheetRulesDef<P> {
    pub const TYPE: &'static str = "sheet_rules";
    pub const BASE_NAME: &'static str = "SheetRules";
    // Written by hand: the allowance and the reliefs map under nested slots.
    pub fn map_params<Q, E>(
        &self,
        f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
    ) -> Result<SheetRulesDef<Q>, E> {
        Ok(SheetRulesDef {
            thickness: f("thickness", &self.thickness)?,
            bend_radius: f("bend_radius", &self.bend_radius)?,
            allowance: self.allowance.map("allowance", f)?,
            process: self.process,
            kerf: self.kerf.as_ref().map(|k| f("kerf", k)).transpose()?,
            flute: self.flute.clone(),
            standard_box: self.standard_box.clone(),
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
            let good = if slot.starts_with("allowance.") {
                allowance_value_ok(slot, *value)
            } else if slot == "kerf" {
                value.is_finite() && *value >= 0.0
            } else {
                value.is_finite() && *value > TOLERANCE
            };
            if good {
                Ok(*value)
            } else {
                Err(format!("invalid sheet rule {slot}: {value}"))
            }
        })?;
        if let Allowance::CreaseTable { rows, .. } = &self.allowance {
            if rows.is_empty() {
                return Err("the rules' crease table has no rows".into());
            }
            if rows
                .iter()
                .any(|row| row.allowance.is_some() == row.deduction.is_some())
            {
                return Err(
                    "each row of a crease table needs an allowance or a deduction, not both".into(),
                );
            }
        }
        Ok(())
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
                    gaps: Vec::new(),
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

#[cfg(test)]
#[path = "sheet_allowance_tests.rs"]
mod allowance_tests;
