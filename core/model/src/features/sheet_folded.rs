// SPDX-License-Identifier: MIT
//! Editable folded sheet operations; geometry is shared with material trimming.

use super::mapping::map_params;
use super::sheet::Relief;
use super::{
    CheckContext, EvalContext, Evaluate, FeatureDef, FeatureInfo, FeatureOutput, References,
};
use crate::ids::{BodyUid, EntityUid, FeatureUid};
use crate::kernel::Kernel;
use crate::parameters::ParamId;
use crate::sheet::{BendPortMode, BoundaryRef, DoubleBendFixed, PanelId, SheetCurveId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SheetPosition {
    Inside,
    Outside,
    #[default]
    BendLine,
    /// The stationary tangent line on the selected boundary: the bend
    /// starts at the edge and the panel keeps all its material (mitcad#211,
    /// the imports' flanges at the edge).
    Edge,
    /// The bend's outer surface touches the side plane through the
    /// selected boundary (normal to the panel) and does not pass it: the
    /// stationary tangent at minus the outer extent `(r+t)
    /// sin(min(|angle|, 90 degrees))` (mitcad#242, the `.f3d` flanges
    /// "tangent to the side face", hems among them).
    SideTangent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SheetLengthMode {
    Tangent,
    Inner,
    Outer,
    /// From the plane that touches the bend's outer surface and is normal
    /// to the wall: the straight length is the entered length minus the
    /// outer extent `(r+t) sin(min(|angle|, 90 degrees))`; defined up to a
    /// 180-degree hem, where the virtual sharps are not (mitcad#242).
    OuterTangent,
}

/// The part of the selected boundary a flange covers, measured from the
/// boundary's midpoint (mitcad#242): `before` towards the start of the
/// canonical axial direction `e` (where `offsets.start` is measured from),
/// `after` towards its end. A flange may be wider than its boundary: past
/// the boundary's ends the wall continues beside the bend and is cut back
/// at its root by the relief depth, or by `root` where given (0: the
/// overhang starts at the bend's moving tangent line, as the `.ipt`
/// flanges' do, mitcad#242). The whole boundary when left out; `offsets`
/// measures from the ends instead and cannot be combined with it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum SheetFlangeWidth<P> {
    Middle {
        before: P,
        after: P,
        #[serde(default = "Option::default", skip_serializing_if = "Option::is_none")]
        root: Option<P>,
    },
}

/// A hem whose bends a flange works out from the hem's sizes
/// (mitcad#1378). The flange's `length` is the hem's length, measured
/// from the plane that touches its first bend's outer surface and is
/// normal to the panel (length mode `outer_tangent`), to the hem's far
/// end; its `radius` is the inside radius of every bend of the hem; its
/// `angle` gives only the side the hem folds to (its sign), the bends'
/// angles following from the sizes. Panel, gap and walls are measured on
/// the sheet's thickness `t`, with `a = r + t`:
///
/// - `teardrop`: one bend of `180 + alpha` degrees and a straight wall
///   coming back towards the panel, the inner corner of its end `gap`
///   above the panel and its outer corner at the hem's length; the radius
///   must be more than half the gap;
/// - `rope`: a bend of `180 + alpha` degrees, `cos(alpha) = (gap + t) /
///   (2 r + t)`, at once a bend of `alpha` the other way, then a wall
///   parallel to the panel `gap` above it, ending at the hem's length
///   (the radius more than half the gap);
/// - `double`: the wall folded back over itself: two bends of 90 degrees
///   with a wall of `gap + t` between them, a wall of `length - 2 a` over
///   the panel, a bend of 180 degrees and a wall back under it, ending
///   `setback` short of the first wall (`length - a - t - setback`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "form", rename_all = "snake_case", deny_unknown_fields)]
pub enum SheetHem<P> {
    Teardrop { gap: P },
    Rope { gap: P },
    Double { gap: P, setback: P },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SheetLength<P> {
    pub mode: SheetLengthMode,
    pub value: P,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SheetOffsets<P> {
    pub start: P,
    pub end: P,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, bound(deserialize = "P: Deserialize<'de>"))]
pub struct SheetFlangeDef<P = ParamId> {
    pub body: BodyUid,
    pub boundary: BoundaryRef,
    pub angle: P,
    pub length: SheetLength<P>,
    #[serde(default)]
    pub position: SheetPosition,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offsets: Option<SheetOffsets<P>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<SheetFlangeWidth<P>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub radius: Option<P>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relief: Option<Relief<P>>,
    /// A hem form whose bends follow from its sizes (mitcad#1378).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hem: Option<SheetHem<P>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, bound(deserialize = "P: Deserialize<'de>"))]
pub struct SheetBendDef<P = ParamId> {
    pub body: BodyUid,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub moving_body: Option<BodyUid>,
    pub stationary: BoundaryRef,
    pub moving: BoundaryRef,
    pub angle: P,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port_mode: Option<BendPortMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub radius: Option<P>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relief: Option<Relief<P>>,
    /// A double bend's boundary whose panel stays whole (mitcad#280):
    /// `stationary` (the default) or `moving`. Only with port mode
    /// `double`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fixed: Option<DoubleBendFixed>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StationarySide {
    Left,
    Right,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FoldLine {
    pub sketch: FeatureUid,
    #[serde(with = "mitcad_model_base::internal::ids::curve_serde")]
    pub curve: EntityUid,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, bound(deserialize = "P: Deserialize<'de>"))]
pub struct SheetFoldDef<P = ParamId> {
    pub body: BodyUid,
    pub panel: PanelId,
    pub line: FoldLine,
    pub stationary_side: StationarySide,
    pub angle: P,
    #[serde(default)]
    pub position: SheetPosition,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub radius: Option<P>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relief: Option<Relief<P>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SheetCorner {
    pub panel: PanelId,
    pub incoming: SheetCurveId,
    pub outgoing: SheetCurveId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub incoming_piece: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outgoing_piece: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CornerSlot {
    pub corner: SheetCorner,
    pub slot: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, bound(deserialize = "P: Deserialize<'de>"))]
pub struct SheetCornerRoundDef<P = ParamId> {
    pub body: BodyUid,
    pub corners: Vec<SheetCorner>,
    pub radius: P,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub corner_slots: Vec<CornerSlot>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CornerDistances<P> {
    pub first: P,
    pub second: P,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, bound(deserialize = "P: Deserialize<'de>"))]
pub struct SheetCornerChamferDef<P = ParamId> {
    pub body: BodyUid,
    pub corners: Vec<SheetCorner>,
    pub distances: CornerDistances<P>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub corner_slots: Vec<CornerSlot>,
}

fn map_relief<P, Q, E>(
    r: &Option<Relief<P>>,
    f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
) -> Result<Option<Relief<Q>>, E> {
    r.as_ref()
        .map(|r| {
            Ok(Relief {
                kind: r.kind,
                width: f("relief.width", &r.width)?,
                depth: f("relief.depth", &r.depth)?,
            })
        })
        .transpose()
}

impl<P> SheetFlangeDef<P> {
    pub const TYPE: &'static str = "sheet_flange";
    pub const BASE_NAME: &'static str = "SheetFlange";
    // Written by hand: length, offsets and width map under nested slots.
    pub fn map_params<Q, E>(
        &self,
        f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
    ) -> Result<SheetFlangeDef<Q>, E> {
        Ok(SheetFlangeDef {
            body: self.body,
            boundary: self.boundary.clone(),
            angle: f("angle", &self.angle)?,
            length: SheetLength {
                mode: self.length.mode,
                value: f("length.value", &self.length.value)?,
            },
            position: self.position,
            offsets: self
                .offsets
                .as_ref()
                .map(|o| {
                    Ok(SheetOffsets {
                        start: f("offsets.start", &o.start)?,
                        end: f("offsets.end", &o.end)?,
                    })
                })
                .transpose()?,
            width: self
                .width
                .as_ref()
                .map(|w| {
                    let SheetFlangeWidth::Middle {
                        before,
                        after,
                        root,
                    } = w;
                    Ok(SheetFlangeWidth::Middle {
                        before: f("width.before", before)?,
                        after: f("width.after", after)?,
                        root: root.as_ref().map(|r| f("width.root", r)).transpose()?,
                    })
                })
                .transpose()?,
            radius: self.radius.as_ref().map(|p| f("radius", p)).transpose()?,
            relief: map_relief(&self.relief, f)?,
            hem: self
                .hem
                .as_ref()
                .map(|h| {
                    Ok(match h {
                        SheetHem::Teardrop { gap } => SheetHem::Teardrop {
                            gap: f("hem.gap", gap)?,
                        },
                        SheetHem::Rope { gap } => SheetHem::Rope {
                            gap: f("hem.gap", gap)?,
                        },
                        SheetHem::Double { gap, setback } => SheetHem::Double {
                            gap: f("hem.gap", gap)?,
                            setback: f("hem.setback", setback)?,
                        },
                    })
                })
                .transpose()?,
        })
    }
}
impl<P> SheetBendDef<P> {
    pub const TYPE: &'static str = "sheet_bend";
    pub const BASE_NAME: &'static str = "SheetBend";
    pub fn resolved_mode(&self) -> BendPortMode {
        self.port_mode
            .unwrap_or(if self.moving_body.is_some_and(|b| b != self.body) {
                BendPortMode::TangentPorts
            } else {
                BendPortMode::VirtualSharp
            })
    }
    // Written by hand: the port mode is the resolved one, not a copy.
    pub fn map_params<Q, E>(
        &self,
        f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
    ) -> Result<SheetBendDef<Q>, E> {
        Ok(SheetBendDef {
            body: self.body,
            moving_body: self.moving_body,
            stationary: self.stationary.clone(),
            moving: self.moving.clone(),
            angle: f("angle", &self.angle)?,
            port_mode: Some(self.resolved_mode()),
            radius: self.radius.as_ref().map(|p| f("radius", p)).transpose()?,
            relief: map_relief(&self.relief, f)?,
            fixed: self.fixed,
        })
    }
}
impl<P> SheetFoldDef<P> {
    pub const TYPE: &'static str = "sheet_fold";
    pub const BASE_NAME: &'static str = "SheetFold";
    map_params! {
        SheetFoldDef {
            body: copy,
            panel: copy,
            line: clone,
            stationary_side: copy,
            angle: param,
            position: copy,
            radius: option,
            relief: with(map_relief),
        }
    }
}
impl<P> SheetCornerRoundDef<P> {
    pub const TYPE: &'static str = "sheet_corner_round";
    pub const BASE_NAME: &'static str = "SheetCornerRound";
    map_params! {
        SheetCornerRoundDef {
            body: copy,
            corners: clone,
            radius: param,
            corner_slots: clone,
        }
    }
}
impl<P> SheetCornerChamferDef<P> {
    pub const TYPE: &'static str = "sheet_corner_chamfer";
    pub const BASE_NAME: &'static str = "SheetCornerChamfer";
    // Written by hand: the distances map under nested slots (`distances.first`).
    pub fn map_params<Q, E>(
        &self,
        f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
    ) -> Result<SheetCornerChamferDef<Q>, E> {
        Ok(SheetCornerChamferDef {
            body: self.body,
            corners: self.corners.clone(),
            distances: CornerDistances {
                first: f("distances.first", &self.distances.first)?,
                second: f("distances.second", &self.distances.second)?,
            },
            corner_slots: self.corner_slots.clone(),
        })
    }
}

fn curve_ref(refs: &mut References, c: &SheetCurveId) {
    refs.features.insert(match c {
        SheetCurveId::Sketch { sketch, .. } => *sketch,
        SheetCurveId::Generated { feature, .. } => *feature,
    });
}
fn boundary_ref(refs: &mut References, b: &BoundaryRef) {
    refs.features.insert(b.panel.feature);
    curve_ref(refs, &b.curve);
}
fn corner_refs(body: BodyUid, corners: &[SheetCorner]) -> References {
    let mut refs = References::default();
    refs.body(body);
    for c in corners {
        refs.features.insert(c.panel.feature);
        curve_ref(&mut refs, &c.incoming);
        curve_ref(&mut refs, &c.outgoing);
    }
    refs
}
fn checked(ctx: &CheckContext<'_>, body: BodyUid, refs: References) -> Result<(), String> {
    ctx.body(body)?;
    for f in refs.features {
        ctx.feature(f)?;
    }
    Ok(())
}

pub(crate) fn settle(def: &mut FeatureDef, old: Option<&FeatureDef>) {
    let (corners, slots, previous) = match def {
        FeatureDef::SheetCornerRound(d) => (
            &d.corners,
            &mut d.corner_slots,
            match old {
                Some(FeatureDef::SheetCornerRound(o)) => Some(&o.corner_slots),
                _ => None,
            },
        ),
        FeatureDef::SheetCornerChamfer(d) => (
            &d.corners,
            &mut d.corner_slots,
            match old {
                Some(FeatureDef::SheetCornerChamfer(o)) => Some(&o.corner_slots),
                _ => None,
            },
        ),
        _ => return,
    };
    if let Some(previous) = previous {
        *slots = previous.clone();
    }
    let mut missing: Vec<_> = corners
        .iter()
        .filter(|c| !slots.iter().any(|s| &s.corner == *c))
        .cloned()
        .collect();
    missing.sort_by_key(|c| serde_json::to_string(c).expect("corner reference"));
    missing.dedup();
    let mut next = slots
        .iter()
        .map(|s| s.slot)
        .max()
        .map_or(0, |v| v.saturating_add(1));
    for corner in missing {
        slots.push(CornerSlot { corner, slot: next });
        next = next.saturating_add(1);
    }
}

fn check_corners(corners: &[SheetCorner], slots: &[CornerSlot]) -> Result<(), String> {
    if corners.is_empty() {
        return Err("no sheet corners selected".into());
    }
    for (i, c) in corners.iter().enumerate() {
        if corners[..i].contains(c) {
            return Err("sheet corner is selected more than once".into());
        }
    }
    for (i, s) in slots.iter().enumerate() {
        if s.slot > u32::MAX / 4
            || slots[..i]
                .iter()
                .any(|p| p.slot == s.slot || p.corner == s.corner)
        {
            return Err("sheet corner allocation contains duplicate or invalid identities".into());
        }
    }
    Ok(())
}

impl FeatureInfo for SheetFlangeDef {
    fn references(&self) -> References {
        let mut r = References::default();
        r.body(self.body);
        boundary_ref(&mut r, &self.boundary);
        r
    }
    fn check(&self, c: &CheckContext<'_>) -> Result<(), String> {
        checked(c, self.body, self.references())
    }
    fn creates_bodies(&self) -> bool {
        true
    }
}
impl FeatureInfo for SheetBendDef {
    fn references(&self) -> References {
        let mut r = References::default();
        r.body(self.body);
        r.body(self.moving_body.unwrap_or(self.body));
        boundary_ref(&mut r, &self.stationary);
        boundary_ref(&mut r, &self.moving);
        r
    }
    fn check(&self, c: &CheckContext<'_>) -> Result<(), String> {
        c.body(self.moving_body.unwrap_or(self.body))?;
        if self.fixed.is_some() && self.resolved_mode() != BendPortMode::Double {
            return Err("a sheet bend's fixed boundary is chosen only for a double bend".into());
        }
        checked(c, self.body, self.references())
    }
    fn creates_bodies(&self) -> bool {
        true
    }
}
impl FeatureInfo for SheetFoldDef {
    fn references(&self) -> References {
        let mut r = References::default();
        r.body(self.body);
        r.features.extend([self.panel.feature, self.line.sketch]);
        r
    }
    fn check(&self, c: &CheckContext<'_>) -> Result<(), String> {
        c.sketch(self.line.sketch)?;
        checked(c, self.body, self.references())
    }
    fn creates_bodies(&self) -> bool {
        true
    }
}
impl FeatureInfo for SheetCornerRoundDef {
    fn references(&self) -> References {
        corner_refs(self.body, &self.corners)
    }
    fn check(&self, c: &CheckContext<'_>) -> Result<(), String> {
        check_corners(&self.corners, &self.corner_slots)?;
        checked(c, self.body, self.references())
    }
    fn creates_bodies(&self) -> bool {
        true
    }
}
impl FeatureInfo for SheetCornerChamferDef {
    fn references(&self) -> References {
        corner_refs(self.body, &self.corners)
    }
    fn check(&self, c: &CheckContext<'_>) -> Result<(), String> {
        check_corners(&self.corners, &self.corner_slots)?;
        checked(c, self.body, self.references())
    }
    fn creates_bodies(&self) -> bool {
        true
    }
}

impl<K: Kernel> Evaluate<K> for SheetFlangeDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let d = self.map_params(&mut |_, p| ctx.param(*p))?;
        crate::sheet::folded::flange(&d, ctx)
    }
}
impl<K: Kernel> Evaluate<K> for SheetBendDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let d = self.map_params(&mut |_, p| ctx.param(*p))?;
        crate::sheet::folded::bend(&d, ctx)
    }
}
impl<K: Kernel> Evaluate<K> for SheetFoldDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let d = self.map_params(&mut |_, p| ctx.param(*p))?;
        crate::sheet::folded::fold(&d, ctx)
    }
}
impl<K: Kernel> Evaluate<K> for SheetCornerRoundDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let d = self.map_params(&mut |_, p| ctx.param(*p))?;
        crate::sheet::folded::corner_round(&d, ctx)
    }
}
impl<K: Kernel> Evaluate<K> for SheetCornerChamferDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let d = self.map_params(&mut |_, p| ctx.param(*p))?;
        crate::sheet::folded::corner_chamfer(&d, ctx)
    }
}
