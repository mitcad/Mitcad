// SPDX-License-Identifier: MIT
//! Editable folded sheet operations; geometry is shared with material trimming.

use super::mapping::map_params;
use super::sheet::{Allowance, Relief, warn_legacy_bend_relief};
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
///
/// The forms of mitcad#2112 fold in one bend and set its radius
/// themselves (the flange's `radius` is not used):
///
/// - `flat`: one bend of 180 degrees whose inside radius is half the gap
///   and a wall back over the panel, ending at the hem's length (`length -
///   gap / 2 - t`); the gap is the smallest the model builds, by default
///   0.1 times the thickness;
/// - `open`: the same with the gap typed, the wall `gap` above the panel;
/// - `rolled`: one bend of `angle` (more than 180 degrees, less than a
///   full turn) and no wall; its `radius` (else the flange's, else the
///   rule's) is its inside radius, and the flange's length is not used.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "form", rename_all = "snake_case", deny_unknown_fields)]
pub enum SheetHem<P> {
    Teardrop {
        gap: P,
    },
    Rope {
        gap: P,
    },
    Double {
        gap: P,
        setback: P,
    },
    Flat {
        #[serde(default = "Option::default", skip_serializing_if = "Option::is_none")]
        gap: Option<P>,
    },
    Open {
        gap: P,
    },
    Rolled {
        angle: P,
        #[serde(default = "Option::default", skip_serializing_if = "Option::is_none")]
        radius: Option<P>,
    },
}

impl<P> SheetHem<P> {
    /// The form's name as definitions write it (`flat`, `teardrop`, ...).
    pub fn form(&self) -> &'static str {
        match self {
            SheetHem::Teardrop { .. } => "teardrop",
            SheetHem::Rope { .. } => "rope",
            SheetHem::Double { .. } => "double",
            SheetHem::Flat { .. } => "flat",
            SheetHem::Open { .. } => "open",
            SheetHem::Rolled { .. } => "rolled",
        }
    }
}

/// A flat hem's gap when none is typed, as a factor of the sheet's
/// thickness (mitcad#2112).
pub const FLAT_HEM_GAP: f64 = 0.1;

/// The corner joint a flange makes where its wall closes against another
/// wall of the same sheet (mitcad#1410). The flange's length is then the
/// inside dimension of the closed section (length mode `inner`), and the
/// joint sets the wall's end from the sheet's thickness `t`:
///
/// - `open`: the wall ends on the other wall's inner surface; the two
///   walls meet only along the corner line (a full open corner, its outer
///   V welded);
/// - `half_open`: the wall reaches `t / 2` further, over half of the other
///   wall's end, and the other wall is cut back by
///   [`HALF_OPEN_SEAM`] under it, as walls that touch over a face cannot
///   make one sound body (mitcad#1409).
///
/// The list is open for further joints (flush, overlap).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SheetFlangeCorner {
    Open,
    HalfOpen,
}

/// The seam a half open corner leaves between the closing wall and the end
/// of the wall it lies over (mm, mitcad#1410).
pub const HALF_OPEN_SEAM: f64 = 1e-5;

/// How the walls of a flange on several edges meet where their edges do
/// (mitcad#1872). Walls are cut normal to their surfaces, as a laser cuts
/// sheet metal:
///
/// - `bisect`: each flange stays on its side of the plane that bisects the
///   corner, half the corner gap from it (walls, and the bends, which end
///   square where all their material clears that plane);
/// - `earlier_runs_past`: the wall of the edge earlier in the flange's list
///   runs on to the other wall's outer surface, and the other wall ends
///   the corner gap short of the passing wall's inner surface;
/// - `later_runs_past`: the same with the later edge's wall passing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SheetMiter {
    #[default]
    Bisect,
    EarlierRunsPast,
    LaterRunsPast,
}

/// The miter of one corner of a flange on several edges (mitcad#1872), in
/// place of the flange's own `miter` there: `edges` are the indices of the
/// two edges that meet (0 is `boundary`, 1 the first of `boundaries`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SheetMiterCorner {
    pub edges: [u32; 2],
    pub miter: SheetMiter,
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
    /// The corner joint where the wall closes against another wall of the
    /// sheet (mitcad#1410); none: the wall's end is free.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub corner: Option<SheetFlangeCorner>,
    /// A sketch before the flange whose closed region on the bend line is
    /// the wall's outline (mitcad#1648): the wall's sketch frame
    /// ([`crate::sheet::folded::flange_profile_frame`]) has its origin on
    /// the bend line at the middle of the boundary, `x` along the bend line
    /// and `y` along the wall. Its offsets and width then come from the
    /// profile; the length only places nothing. None: the rectangular wall
    /// of `length`, `offsets` and `width`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<FeatureUid>,
    /// Further straight boundaries after `boundary` (mitcad#1872): one
    /// wall on each, all of the flange's angle, length, position and
    /// radius. The edges are connected at their ends (at a common vertex,
    /// or across the side of a bend between their faces); where two of
    /// them meet, their walls are mitred ([`SheetMiter`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub boundaries: Vec<BoundaryRef>,
    /// The gap between the walls at a corner of a flange on several edges
    /// (or between its wall and an existing flange's wall it meets); none:
    /// the sheet's thickness.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub corner_gap: Option<P>,
    /// How the walls meet at the corners; none: `bisect`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub miter: Option<SheetMiter>,
    /// Corners whose miter differs from `miter`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub corner_miters: Vec<SheetMiterCorner>,
    /// The fold's own allowance in place of the rules' (mitcad#1939, a
    /// per-bend override): any allowance method; a crease table without
    /// rows picks the rules' rows in its direction.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowance: Option<Allowance<P>>,
    /// The boundary goes on along its panel's straight boundaries that
    /// continue it in a line and are joined to no other domain
    /// (mitcad#2117): one bend and one wall along the whole run, as on a
    /// body whose edge there is one (an imported design's base flange over
    /// collinear sketch lines). One edge only, no profile, not at position
    /// `edge`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub run: bool,
    /// An end of the whole boundary where the panel goes on along the bend
    /// line (its outline turns away from the material there: a concave
    /// corner) takes the bend relief inside the flange's span, as an offset
    /// end of the relief's width does (mitcad#2117: the `.f3d` edge
    /// flange's bend relief). Only without offsets, a width or a profile.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub relieve_ends: bool,
}

impl<P> SheetFlangeDef<P> {
    /// The flange's edges in order: `boundary`, then `boundaries`.
    pub fn edges(&self) -> Vec<&BoundaryRef> {
        std::iter::once(&self.boundary)
            .chain(&self.boundaries)
            .collect()
    }
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
    /// The fold's own allowance in place of the rules' (mitcad#1939, a
    /// per-bend override): any allowance method; a crease table without
    /// rows picks the rules' rows in its direction.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowance: Option<Allowance<P>>,
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
    /// The fold's own allowance in place of the rules' (mitcad#1939, a
    /// per-bend override): any allowance method; a crease table without
    /// rows picks the rules' rows in its direction.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowance: Option<Allowance<P>>,
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

/// A fold's own allowance, its slots under `allowance` (mitcad#1939).
fn map_allowance<P, Q, E>(
    a: &Option<Allowance<P>>,
    f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
) -> Result<Option<Allowance<Q>>, E> {
    a.as_ref().map(|a| a.map("allowance", f)).transpose()
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
                        SheetHem::Flat { gap } => SheetHem::Flat {
                            gap: gap.as_ref().map(|g| f("hem.gap", g)).transpose()?,
                        },
                        SheetHem::Open { gap } => SheetHem::Open {
                            gap: f("hem.gap", gap)?,
                        },
                        SheetHem::Rolled { angle, radius } => SheetHem::Rolled {
                            angle: f("hem.angle", angle)?,
                            radius: radius.as_ref().map(|r| f("hem.radius", r)).transpose()?,
                        },
                    })
                })
                .transpose()?,
            corner: self.corner,
            profile: self.profile,
            boundaries: self.boundaries.clone(),
            corner_gap: self
                .corner_gap
                .as_ref()
                .map(|p| f("corner_gap", p))
                .transpose()?,
            miter: self.miter,
            corner_miters: self.corner_miters.clone(),
            allowance: map_allowance(&self.allowance, f)?,
            run: self.run,
            relieve_ends: self.relieve_ends,
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
            allowance: map_allowance(&self.allowance, f)?,
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
            allowance: with(map_allowance),
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
        for b in &self.boundaries {
            boundary_ref(&mut r, b);
        }
        if let Some(profile) = self.profile {
            r.features.insert(profile);
        }
        r
    }
    fn check(&self, c: &CheckContext<'_>) -> Result<(), String> {
        if let Some(profile) = self.profile {
            c.sketch(profile)?;
        }
        let edges = self.edges();
        for (i, edge) in edges.iter().enumerate() {
            if edges[..i].contains(edge) {
                return Err("The flange's edges include the same edge twice.".into());
            }
        }
        for corner in &self.corner_miters {
            let [a, b] = corner.edges;
            if a == b || a as usize >= edges.len() || b as usize >= edges.len() {
                return Err(
                    "A corner's miter names edges the flange does not have; a corner is two \
                     different edges of the flange."
                        .into(),
                );
            }
        }
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
        warn_legacy_bend_relief(d.relief.as_ref(), ctx);
        // A profile sketch left where an earlier angle or position put the
        // wall says so (mitcad#1648); the wall follows its outline anyway.
        crate::sheet::folded::warn_moved_profile(&d, ctx)
            .and_then(|()| crate::sheet::folded::flange(&d, ctx))
            .map_err(|e| crate::sheet::folded::worded(ctx, &e))
    }
}
impl<K: Kernel> Evaluate<K> for SheetBendDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let d = self.map_params(&mut |_, p| ctx.param(*p))?;
        warn_legacy_bend_relief(d.relief.as_ref(), ctx);
        crate::sheet::folded::bend(&d, ctx).map_err(|e| crate::sheet::folded::worded(ctx, &e))
    }
}
impl<K: Kernel> Evaluate<K> for SheetFoldDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let d = self.map_params(&mut |_, p| ctx.param(*p))?;
        warn_legacy_bend_relief(d.relief.as_ref(), ctx);
        crate::sheet::folded::fold(&d, ctx).map_err(|e| crate::sheet::folded::worded(ctx, &e))
    }
}
impl<K: Kernel> Evaluate<K> for SheetCornerRoundDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let d = self.map_params(&mut |_, p| ctx.param(*p))?;
        crate::sheet::folded::corner_round(&d, ctx)
            .map_err(|e| crate::sheet::folded::worded(ctx, &e))
    }
}
impl<K: Kernel> Evaluate<K> for SheetCornerChamferDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let d = self.map_params(&mut |_, p| ctx.param(*p))?;
        crate::sheet::folded::corner_chamfer(&d, ctx)
            .map_err(|e| crate::sheet::folded::worded(ctx, &e))
    }
}
