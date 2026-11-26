// SPDX-License-Identifier: MIT
//! Corner relief at the junction of two bends (mitcad#209; junctions without
//! a common panel and non-perpendicular rectangular reliefs mitcad#125).
//!
//! The junction is found in a development: a stationary panel stays fixed
//! while the two bends, and every bend on the seam paths from that panel to
//! them, are developed into its plane. The panel is the one both bends leave.
//! When they leave none in common (their panels meet through planar seams,
//! other panels or other bends), it is the panel next to the first bend on
//! the shortest seam path to the second, or the selected panel, which must
//! join one of the two bends. Each bend is then a strip between the tangent
//! line through which the development reaches it (`s = 0`, the line on the
//! common panel when there is one) and its far tangent line (`s = BA`). The
//! junction is where the two strips would cross: `J0` is the meeting point of
//! the two `s = 0` lines and `C` that of the two bend centrelines
//! (`s = BA/2`). `C` and the crossing zones do not depend on which side of a
//! strip is `s = 0`. The relief is removed from all material in the panel's
//! developed plane with the same operation as a sheet Cut, then the input
//! configuration returns:
//!
//! - `round`: a disc of diameter `width` centred at `C`;
//! - `rectangular`: the crossing of two bands of width `width` centred on the
//!   two centrelines (`|s1 - BA1/2| <= width/2`, `|s2 - BA2/2| <= width/2`):
//!   for perpendicular bends a square of side `width` with sides along both
//!   strips, otherwise the parallelogram with sides parallel to both bend
//!   lines and `width` across each strip (area `width^2 / sin(angle)`);
//! - `tear`: exactly the crossing zone `0 <= s1 <= BA1, 0 <= s2 <= BA2`, the
//!   minimum separation of the two bend zones; it needs no size.
//!
//! `depth` belongs to bend reliefs and is not used here. A relief that would
//! remove a whole domain or split the sheet is refused (the flange is too
//! short for that relief), as is one reaching a seam of a third, folded bend;
//! parallel bends have no junction.
//!
//! A Flange whose bend meets another bend at a corner cuts the rules' corner
//! relief there by itself (mitcad#2342, [`flange_corner_reliefs`]). Its
//! corner leaves the crossing zone open already: a flange on the next edge
//! cuts the earlier bend back to its own tangent line and runs its bend to
//! the earlier one's; a flange on several edges runs each bend of a corner
//! to the other's tangent line where the rules' relief opens the corner
//! (mitcad#2411, `flange_edges.rs`). So the relief there is measured from
//! `J0`, the inner corner of the two tangent lines, outward, rather than
//! centred at `C` inside a zone that holds no material (mitcad#2411):
//!
//! - `tear`: nothing more; the two bends meet at `J0`;
//! - `rectangular`: the crossing zone grown by `width` toward the panel
//!   (`-width <= s1 <= BA1`, `-width <= s2 <= BA2`): each bend ends `width`
//!   short of the other's tangent line and the panel loses a `width` square
//!   (a parallelogram at an oblique corner) at its corner;
//! - `round`: a disc of diameter `width` centred at `J0`, which notches
//!   the panel's corner and each bend's end beside it.

use super::cut::{CutOutcome, developed_cut};
use super::develop::{bend_record, bend_side, domain_label, planar_map, reconfigure};
use super::*;
use crate::features::sheet::{Relief, ReliefKind};
use crate::features::sheet_relief::SheetCornerReliefDef;
use crate::features::{FeatureDef, FeatureEntry};
use crate::fingerprint::Fingerprint;
use crate::recompute::Read;
use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;

type V2 = [f64; 2];

fn vector(map: Isometry2, v: V2) -> V2 {
    [
        v[0] * map.x_axis[0] + v[1] * map.y_axis[0],
        v[0] * map.x_axis[1] + v[1] * map.y_axis[1],
    ]
}

/// A developed bend in the panel's chart: its `s = 0` tangent line passes
/// through `origin`; `into` is the unit direction of increasing `s`,
/// `allowance` its developed width.
#[derive(Debug, Clone, Copy)]
struct Strip {
    origin: V2,
    into: V2,
    allowance: f64,
}

/// The point at offset `s` into the first strip and `t` into the second.
fn junction(a: &Strip, b: &Strip, s: f64, t: f64) -> Result<V2, String> {
    let det = a.into[0] * b.into[1] - a.into[1] * b.into[0];
    if det.abs() <= 1e-9 {
        return Err("the bends are parallel and have no corner junction".into());
    }
    // Solve into_a . p = ra, into_b . p = rb: the offsets measured from each
    // tangent line.
    let ra = s + a.origin[0] * a.into[0] + a.origin[1] * a.into[1];
    let rb = t + b.origin[0] * b.into[0] + b.origin[1] * b.into[1];
    Ok([
        (ra * b.into[1] - rb * a.into[1]) / det,
        (a.into[0] * rb - b.into[0] * ra) / det,
    ])
}

/// The corners of the rectangular relief of `width` centred at `centre`:
/// the crossing of the bands `|s1 - BA1/2| <= width/2` and
/// `|s2 - BA2/2| <= width/2`. For perpendicular strips this is the square
/// `centre + s*h*into_a + t*h*into_b`, `s, t = +-1`.
fn rectangular(a: &Strip, b: &Strip, centre: V2, width: f64) -> Vec<V2> {
    let h = width / 2.0;
    let d = a.into[0] * b.into[0] + a.into[1] * b.into[1];
    let sine2 = 1.0 - d * d;
    // p = centre + alpha*into_a + beta*into_b with into_a.(p - centre) = s*h
    // and into_b.(p - centre) = t*h.
    let at = |s: f64, t: f64| {
        let alpha = (s * h - d * t * h) / sine2;
        let beta = (t * h - d * s * h) / sine2;
        [
            centre[0] + alpha * a.into[0] + beta * b.into[0],
            centre[1] + alpha * a.into[1] + beta * b.into[1],
        ]
    };
    vec![at(-1.0, -1.0), at(1.0, -1.0), at(1.0, 1.0), at(-1.0, 1.0)]
}

/// The rectangular relief of `width` at a flange's corner (mitcad#2411):
/// the crossing zone grown by `width` toward the panel,
/// `-width <= s1 <= BA1` and `-width <= s2 <= BA2`.
fn grown_zone(a: &Strip, b: &Strip, width: f64) -> Result<Vec<V2>, String> {
    Ok(vec![
        junction(a, b, -width, -width)?,
        junction(a, b, a.allowance, -width)?,
        junction(a, b, a.allowance, b.allowance)?,
        junction(a, b, -width, b.allowance)?,
    ])
}

fn ccw(mut points: Vec<V2>) -> Vec<V2> {
    let area: f64 = points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .map(|(p, q)| p[0] * q[1] - q[0] * p[1])
        .sum();
    if area < 0.0 {
        points.reverse();
    }
    points
}

/// The panels joined to `bend` through registered seams.
fn neighbours(state: &SheetState, bend: BendId) -> BTreeSet<PanelId> {
    let domain = SheetDomainRef::Bend(bend);
    state
        .joins
        .iter()
        .filter_map(|p| {
            if p.first.boundary.domain == domain {
                Some(p.second.boundary.domain)
            } else if p.second.boundary.domain == domain {
                Some(p.first.boundary.domain)
            } else {
                None
            }
        })
        .filter_map(|d| match d {
            SheetDomainRef::Panel(p) => Some(p),
            SheetDomainRef::Bend(_) => None,
        })
        .collect()
}

/// Breadth-first search over the registered seams from `start`, the seams in
/// their stored order: every reached domain with the domain it was first
/// reached from.
fn reach(
    state: &SheetState,
    start: SheetDomainRef,
) -> BTreeMap<SheetDomainRef, Option<SheetDomainRef>> {
    let mut parents = BTreeMap::from([(start, None)]);
    let mut queue = VecDeque::from([start]);
    while let Some(domain) = queue.pop_front() {
        for port in &state.joins {
            for (own, other) in [(&port.first, &port.second), (&port.second, &port.first)] {
                let next = other.boundary.domain;
                if own.boundary.domain == domain && !parents.contains_key(&next) {
                    parents.insert(next, Some(domain));
                    queue.push_back(next);
                }
            }
        }
    }
    parents
}

/// The seam path from the start of `parents` to `to`, both included.
fn path(
    parents: &BTreeMap<SheetDomainRef, Option<SheetDomainRef>>,
    to: SheetDomainRef,
) -> Option<Vec<SheetDomainRef>> {
    let mut path = vec![to];
    let mut at = to;
    while let Some(parent) = *parents.get(&at)? {
        path.push(parent);
        at = parent;
    }
    path.reverse();
    Some(path)
}

/// The stationary panel of the development (see the module notes).
fn stationary_panel(
    state: &SheetState,
    first: BendId,
    second: BendId,
    selected: Option<PanelId>,
) -> Result<PanelId, String> {
    let label = |b: BendId| domain_label(SheetDomainRef::Bend(b));
    let (near_first, near_second) = (neighbours(state, first), neighbours(state, second));
    let common: Vec<PanelId> = near_first.intersection(&near_second).copied().collect();
    if !common.is_empty() {
        return match selected {
            Some(p) if common.contains(&p) => Ok(p),
            Some(p) => Err(format!(
                "{} does not join both {} and {}",
                domain_label(SheetDomainRef::Panel(p)),
                label(first),
                label(second)
            )),
            None => match common.as_slice() {
                [p] => Ok(*p),
                _ => Err(format!(
                    "{} and {} leave several common panels; select one",
                    label(first),
                    label(second)
                )),
            },
        };
    }
    // No common panel: the junction is taken in the development of the seam
    // path between the two bends.
    if let Some(p) = selected {
        if near_first.contains(&p) || near_second.contains(&p) {
            return Ok(p);
        }
        return Err(format!(
            "{} joins neither {} nor {}",
            domain_label(SheetDomainRef::Panel(p)),
            label(first),
            label(second)
        ));
    }
    let parents = reach(state, SheetDomainRef::Bend(first));
    let route = path(&parents, SheetDomainRef::Bend(second)).ok_or_else(|| {
        format!(
            "{} and {} are not connected through the sheet's bends and seams",
            label(first),
            label(second)
        )
    })?;
    route
        .iter()
        .find_map(|d| match d {
            SheetDomainRef::Panel(p) => Some(*p),
            SheetDomainRef::Bend(_) => None,
        })
        .ok_or_else(|| {
            format!(
                "{} and {} are joined by no panel",
                label(first),
                label(second)
            )
        })
}

/// A bend developed in the fixed chart, measured from its tangent line on
/// `from`, the domain through which the development reaches it.
fn strip(
    state: &SheetState,
    bend: BendId,
    from: SheetDomainRef,
    fixed: &SheetEmbedding,
    thickness: f64,
) -> Result<Strip, String> {
    let record = bend_record(state, bend, thickness)?;
    let domain = SheetDomainRef::Bend(bend);
    let patch = state
        .charts
        .iter()
        .find(|p| p.domain == domain)
        .ok_or_else(|| format!("{} has no material chart", domain_label(domain)))?;
    let map = planar_map(&patch.embedding, fixed)?;
    let mut side: Option<f64> = None;
    for port in &state.joins {
        let own = if port.first.boundary.domain == domain && port.second.boundary.domain == from {
            &port.first
        } else if port.second.boundary.domain == domain && port.first.boundary.domain == from {
            &port.second
        } else {
            continue;
        };
        let u = bend_side(patch, own, record.allowance)?;
        if side.is_some_and(|old| (old - u).abs() > TOLERANCE) {
            return Err(format!(
                "{} joins {} along both tangent lines",
                domain_label(domain),
                domain_label(from)
            ));
        }
        side = Some(u);
    }
    let u = side.ok_or_else(|| {
        format!(
            "{} does not join {}",
            domain_label(domain),
            domain_label(from)
        )
    })?;
    let into = vector(map, [if u <= TOLERANCE { 1.0 } else { -1.0 }, 0.0]);
    Ok(Strip {
        origin: map.point([u, 0.0]),
        into,
        allowance: record.allowance,
    })
}

pub(crate) fn corner_relief<K: Kernel>(
    d: &SheetCornerReliefDef<f64>,
    ctx: &mut EvalContext<'_, K>,
) -> Result<FeatureOutput<K::Shape>, String> {
    let (state, rules) = read(ctx, d.body)?;
    let relief = d.relief.clone().unwrap_or(rules.corner_relief.clone());
    let [first, second] = d.bends[..] else {
        return Err("a corner relief names exactly two bends".into());
    };
    if relief.kind != ReliefKind::Tear && (!relief.width.is_finite() || relief.width <= TOLERANCE) {
        return Err("a corner relief requires a positive width".into());
    }
    let outcome = relieve(
        ctx,
        &state,
        rules.thickness,
        [first, second],
        d.panel,
        (&relief, Placement::Centre),
        0,
        "the corner relief",
    )?;
    if let Some(domain) = outcome.removed.first() {
        return Err(format!(
            "the corner relief removes all material of {}; the flange is too short for it",
            domain_label(*domain)
        ));
    }
    if !outcome.used_tools {
        ctx.warn("the corner relief removes no material at the junction");
    }
    publish_pieces(ctx, outcome.state, &rules, Some("the corner relief"))
}

/// Where a relief's shape lies at a junction (see the module notes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Placement {
    /// The Corner Relief feature's: centred at `C`, Tear the crossing zone.
    Centre,
    /// The rules' relief at a flange's corner (mitcad#2411): from `J0`
    /// outward, the crossing zone open already.
    Corner,
}

/// The relief `relief` cut at the junction of the bends `first` and
/// `second` (see the module notes), placed by `placement`, `selected` the
/// panel held while it is developed (none: the one [`stationary_panel`]
/// finds). The relief's outline is the feature's generated curves from
/// `slot`; `what` names the cut in errors.
#[allow(clippy::too_many_arguments)]
fn relieve<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    state: &SheetState,
    thickness: f64,
    [first, second]: [BendId; 2],
    selected: Option<PanelId>,
    (relief, placement): (&Relief<f64>, Placement),
    slot: u32,
    what: &str,
) -> Result<CutOutcome, String> {
    for bend in [first, second] {
        bend_record(state, bend, thickness)?;
    }
    let panel = stationary_panel(state, first, second, selected)?;
    // The bends developed: both, and every bend on the seam paths from the
    // panel to them (none when they leave it directly).
    let parents = reach(state, SheetDomainRef::Panel(panel));
    let mut open: Vec<BendId> = Vec::new();
    let mut entries = Vec::new();
    for bend in [first, second] {
        let route = path(&parents, SheetDomainRef::Bend(bend)).ok_or_else(|| {
            format!(
                "{} is not connected to {} through the sheet's bends and seams",
                domain_label(SheetDomainRef::Bend(bend)),
                domain_label(SheetDomainRef::Panel(panel))
            )
        })?;
        for d in &route {
            if let SheetDomainRef::Bend(b) = d
                && !open.contains(b)
            {
                open.push(*b);
            }
        }
        entries.push(route[route.len() - 2]);
    }
    // The junction is found with those bends developed from the panel.
    let mut work = state.clone();
    let targets: BTreeMap<BendId, bool> = open.iter().map(|b| (*b, true)).collect();
    reconfigure(&mut work, thickness, panel, &targets)?;
    let fixed = chart(&work, panel)?.embedding.clone();
    let a = strip(&work, first, entries[0], &fixed, thickness)?;
    let b = strip(&work, second, entries[1], &fixed, thickness)?;
    if placement == Placement::Corner {
        let untouched = || CutOutcome {
            state: state.clone(),
            removed: BTreeSet::new(),
            used_tools: false,
        };
        // At a reflex corner of the panel the bends run on past each
        // other's tangent lines (their middles on the far side): no corner
        // to relieve.
        let middle = |bend: BendId| -> Result<V2, String> {
            let domain = SheetDomainRef::Bend(bend);
            let patch = work
                .charts
                .iter()
                .find(|p| p.domain == domain)
                .ok_or_else(|| format!("{} has no material chart", domain_label(domain)))?;
            let (min, max) = bounds(patch)?;
            Ok(planar_map(&patch.embedding, &fixed)?
                .point([(min[0] + max[0]) / 2.0, (min[1] + max[1]) / 2.0]))
        };
        let across =
            |s: &Strip, p: V2| (p[0] - s.origin[0]) * s.into[0] + (p[1] - s.origin[1]) * s.into[1];
        if across(&b, middle(first)?) >= 0.0 || across(&a, middle(second)?) >= 0.0 {
            return Ok(untouched());
        }
        let w = relief.width;
        let tool = match relief.kind {
            // The two bends meet at J0 already.
            ReliefKind::Tear => return Ok(untouched()),
            ReliefKind::Round => region(
                ctx.uid,
                slot,
                vec![SegmentGeometry::Circle {
                    center: junction(&a, &b, 0.0, 0.0)?,
                    radius: w / 2.0,
                    start: 0.0,
                }],
            )?,
            ReliefKind::Rectangular => polygon(ctx.uid, slot, &ccw(grown_zone(&a, &b, w)?))?,
        };
        return developed_cut(ctx, state, thickness, panel, &open, &[tool], what);
    }
    let centre = junction(&a, &b, a.allowance / 2.0, b.allowance / 2.0)?;
    let tool = match relief.kind {
        ReliefKind::Round => region(
            ctx.uid,
            slot,
            vec![SegmentGeometry::Circle {
                center: centre,
                radius: relief.width / 2.0,
                start: 0.0,
            }],
        )?,
        ReliefKind::Rectangular => polygon(
            ctx.uid,
            slot,
            &ccw(rectangular(&a, &b, centre, relief.width)),
        )?,
        ReliefKind::Tear => polygon(
            ctx.uid,
            slot,
            &ccw(vec![
                junction(&a, &b, 0.0, 0.0)?,
                junction(&a, &b, a.allowance, 0.0)?,
                junction(&a, &b, a.allowance, b.allowance)?,
                junction(&a, &b, 0.0, b.allowance)?,
            ]),
        )?,
    };
    developed_cut(ctx, state, thickness, panel, &open, &[tool], what)
}

/// The generated curves of the rules' corner relief at a flange's `n`-th
/// corner start at `CORNER_SLOT + 100 n` (mitcad#2342): free in every
/// hundred of a flange's curves (its trim, bends, walls and corner cuts
/// use `0` to `89` of each).
const CORNER_SLOT: u32 = 90;

/// The bends that meet `bend` at a corner of `panel` (mitcad#2342): each
/// other bend seamed to the panel whose seam ends where a seam of `bend` on
/// the panel ends, the two not parallel. A Flange on the next edge of a
/// corner meets an earlier one so: its trim cuts the earlier bend back to
/// its own tangent line, and its bend runs to the earlier one's.
pub(super) fn corner_bends(state: &SheetState, panel: PanelId, bend: BendId) -> Vec<BendId> {
    let seams = |b: BendId| -> Vec<(V2, V2)> {
        state
            .joins
            .iter()
            .flat_map(|p| [(&p.first, &p.second), (&p.second, &p.first)])
            .filter(|(here, there)| {
                here.boundary.domain == SheetDomainRef::Panel(panel)
                    && there.boundary.domain == SheetDomainRef::Bend(b)
            })
            .filter_map(|(here, _)| port_ends(state, here))
            .collect()
    };
    let own = seams(bend);
    let near = |p: V2, q: V2| (p[0] - q[0]).hypot(p[1] - q[1]) <= 1e-6;
    let meet = |(a, b): (V2, V2), (c, d): (V2, V2)| {
        let u = [b[0] - a[0], b[1] - a[1]];
        let v = [d[0] - c[0], d[1] - c[1]];
        let length = u[0].hypot(u[1]) * v[0].hypot(v[1]);
        length > 1e-12
            && (u[0] * v[1] - u[1] * v[0]).abs() / length > 1e-6
            && [a, b].iter().any(|p| [c, d].iter().any(|q| near(*p, *q)))
    };
    let mut out: Vec<BendId> = Vec::new();
    for other in state.bends.iter().map(|b| b.id) {
        if other == bend || out.contains(&other) {
            continue;
        }
        if seams(other)
            .iter()
            .any(|theirs| own.iter().any(|ours| meet(*ours, *theirs)))
        {
            out.push(other);
        }
    }
    out
}

/// The Corner Relief features of the design that name a bend of `flange`
/// and are not suppressed: each one's uid and its two bends in order.
fn corner_relief_features(
    features: &[Arc<FeatureEntry>],
    flange: FeatureUid,
) -> Vec<(FeatureUid, [BendId; 2])> {
    features
        .iter()
        .filter(|entry| !entry.suppressed)
        .filter_map(|entry| match &entry.def {
            FeatureDef::SheetCornerRelief(relief) => match relief.bends[..] {
                [a, b] if a.feature == flange || b.feature == flange => {
                    Some((entry.uid, [a.min(b), a.max(b)]))
                }
                _ => None,
            },
            _ => None,
        })
        .collect()
}

/// What a flange read of the Corner Relief features on its corners, for
/// [`Read::CornerReliefs`] (mitcad#2342): their uids and bends, None
/// without one. They come after the flange in the timeline, so the flange
/// reads their definitions, not their results.
pub(crate) fn corner_reliefs_key(
    features: &[Arc<FeatureEntry>],
    flange: FeatureUid,
) -> Option<u128> {
    let found = corner_relief_features(features, flange);
    if found.is_empty() {
        return None;
    }
    let mut key = Fingerprint::new("flange corner reliefs");
    for (uid, bends) in &found {
        key.u64(uid.0);
        for bend in bends {
            key.u64(bend.feature.0).u32(bend.slot);
        }
    }
    Some(key.finish())
}

/// Whether the feature that made `bend` has a relief of its own (a
/// Flange's, a Bend's or a Fold's `relief`): its corners keep it.
fn own_relief<K: Kernel>(ctx: &EvalContext<'_, K>, bend: BendId) -> bool {
    ctx.feature_entry(bend.feature)
        .is_ok_and(|entry| match &entry.def {
            FeatureDef::SheetFlange(f) => f.relief.is_some(),
            FeatureDef::SheetBend(b) => b.relief.is_some(),
            FeatureDef::SheetFold(f) => f.relief.is_some(),
            _ => false,
        })
}

/// The corners of the flange being evaluated that Corner Relief features
/// relieve instead of the rules (each its two bends in order), recording
/// that the flange read them ([`Read::CornerReliefs`]; they come after the
/// flange, so it reads their definitions).
pub(super) fn named_corners<K: Kernel>(ctx: &mut EvalContext<'_, K>) -> Vec<[BendId; 2]> {
    let features = ctx.env.features;
    let named = corner_relief_features(features, ctx.uid)
        .into_iter()
        .map(|(_, bends)| bends)
        .collect();
    ctx.reads.push(Read::CornerReliefs(
        ctx.uid,
        corner_reliefs_key(features, ctx.uid),
    ));
    named
}

/// The rules' corner relief cut at the corners where the bends of the
/// flange being evaluated meet other bends (mitcad#2342, its placement
/// mitcad#2411; commands.md, "Corner relief at flange corners"): `corners`
/// the pairs, a bend of the flange first. Each is cut in the panel both
/// bends leave, with the rules' relief measured from the inner corner of
/// their tangent lines ([`Placement::Corner`]), when
///
/// - the rules relieve flange corners ([`SheetRulesDef::relieves_flange_corners`]);
/// - no Corner Relief feature names the two bends (it relieves the corner
///   instead, [`named_corners`]);
/// - neither bend's feature has a relief of its own (the flange being
///   evaluated is checked by its caller);
/// - the two bends leave one common panel;
/// - the relief is Rectangular or Round: a Tear adds nothing to a corner
///   whose bends meet at that inner corner.
pub(super) fn flange_corner_reliefs<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    mut state: SheetState,
    rules: &SheetRulesDef<f64>,
    corners: &[[BendId; 2]],
) -> Result<SheetState, String> {
    if corners.is_empty() || !rules.relieves_flange_corners() {
        return Ok(state);
    }
    let named = named_corners(ctx);
    let relief = &rules.corner_relief;
    if relief.kind == ReliefKind::Tear {
        return Ok(state);
    }
    let thickness = rules.thickness;
    for (n, &[own, other]) in corners.iter().enumerate() {
        if named.contains(&[own.min(other), own.max(other)]) || own_relief(ctx, other) {
            continue;
        }
        let common: Vec<PanelId> = neighbours(&state, own)
            .intersection(&neighbours(&state, other))
            .copied()
            .collect();
        let [panel] = common[..] else {
            continue;
        };
        // (A bend whose development is stale is left as it is; a later
        // feature that develops it says so.)
        if bend_record(&state, own, thickness).is_err()
            || bend_record(&state, other, thickness).is_err()
        {
            continue;
        }
        let outcome = relieve(
            ctx,
            &state,
            thickness,
            [own, other],
            Some(panel),
            (relief, Placement::Corner),
            CORNER_SLOT + 100 * n as u32,
            "the rules' corner relief",
        )
        .map_err(|e| too_wide(&e))?;
        if let Some(domain) = outcome.removed.first() {
            return Err(too_wide(&format!(
                "the rules' corner relief removes all material of {}",
                domain_label(*domain)
            )));
        }
        if outcome.used_tools {
            state = outcome.state;
        }
    }
    Ok(state)
}

/// A refusal of the rules' corner relief at a flange's corner, with what
/// the user can do about it.
fn too_wide(message: &str) -> String {
    format!(
        "{message}. The Corner Relief of the sheet rules does not fit at this flange's corner: \
         make Corner Relief Width smaller, or give the flange a relief of its own"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(p: V2, q: V2) -> bool {
        (p[0] - q[0]).abs() < 1e-12 && (p[1] - q[1]).abs() < 1e-12
    }

    #[test]
    fn junction_is_measured_from_both_tangent_lines() {
        let ba = 1.2 * std::f64::consts::PI;
        // Bend 1 leaves the panel at x = 37 towards +x, bend 2 at y = 27
        // towards +y: the tangent lines meet at (37, 27), the centrelines at
        // (37 + BA/2, 27 + BA/2).
        let a = Strip {
            origin: [37.0, -5.0],
            into: [1.0, 0.0],
            allowance: ba,
        };
        let b = Strip {
            origin: [4.0, 27.0],
            into: [0.0, 1.0],
            allowance: ba,
        };
        assert!(near(junction(&a, &b, 0.0, 0.0).unwrap(), [37.0, 27.0]));
        assert!(near(
            junction(&a, &b, ba / 2.0, ba / 2.0).unwrap(),
            [37.0 + ba / 2.0, 27.0 + ba / 2.0]
        ));
        // Measured into each strip, whichever way the strip runs.
        let c = Strip {
            origin: [0.0, 27.0],
            into: [0.0, -1.0],
            allowance: ba,
        };
        assert!(near(junction(&a, &c, 1.0, 2.0).unwrap(), [38.0, 25.0]));
        let parallel = Strip {
            origin: [50.0, 0.0],
            into: [-1.0, 0.0],
            allowance: ba,
        };
        assert!(junction(&a, &parallel, 0.0, 0.0).is_err());
        // Outlines are counter-clockwise whatever the bends' handedness.
        let square = ccw(vec![[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]]);
        assert_eq!(square[1], [1.0, 1.0]);
    }

    #[test]
    fn the_centre_and_crossing_zone_do_not_depend_on_the_strip_side() {
        // The same strip measured from its other tangent line.
        let ba = 3.0;
        let a = Strip {
            origin: [10.0, 0.0],
            into: [1.0, 0.0],
            allowance: ba,
        };
        let a_far = Strip {
            origin: [10.0 + ba, 7.0],
            into: [-1.0, 0.0],
            allowance: ba,
        };
        let b = Strip {
            origin: [0.0, 5.0],
            into: [0.6, 0.8],
            allowance: 2.0,
        };
        let c = junction(&a, &b, ba / 2.0, 1.0).unwrap();
        assert!(near(junction(&a_far, &b, ba / 2.0, 1.0).unwrap(), c));
        let zone = |s: &Strip| -> Vec<V2> {
            let mut corners: Vec<V2> = [(0.0, 0.0), (ba, 0.0), (ba, 2.0), (0.0, 2.0)]
                .iter()
                .map(|(p, q)| junction(s, &b, *p, *q).unwrap())
                .collect();
            corners.sort_by(|p, q| p[0].total_cmp(&q[0]).then(p[1].total_cmp(&q[1])));
            corners
        };
        for (p, q) in zone(&a).iter().zip(zone(&a_far)) {
            assert!((p[0] - q[0]).abs() < 1e-12 && (p[1] - q[1]).abs() < 1e-12);
        }
    }

    #[test]
    fn rectangular_relief_is_the_crossing_of_two_bands() {
        let a = Strip {
            origin: [0.0, 0.0],
            into: [1.0, 0.0],
            allowance: 4.0,
        };
        // Perpendicular: the square of side `width` along both strips.
        let b = Strip {
            origin: [0.0, 0.0],
            into: [0.0, 1.0],
            allowance: 4.0,
        };
        let centre = junction(&a, &b, 2.0, 2.0).unwrap();
        let square = rectangular(&a, &b, centre, 2.0);
        assert_eq!(square, vec![[1.0, 1.0], [3.0, 1.0], [3.0, 3.0], [1.0, 3.0]]);
        // At 60 degrees between the strips' directions: a parallelogram whose
        // sides are parallel to both bend lines, `width` across each strip,
        // of area width^2 / sin(60 degrees).
        let (sin, cos) = (3f64.sqrt() / 2.0, 0.5);
        let b = Strip {
            origin: [0.0, 0.0],
            into: [cos, sin],
            allowance: 4.0,
        };
        let centre = junction(&a, &b, 2.0, 2.0).unwrap();
        let corners = ccw(rectangular(&a, &b, centre, 2.0));
        let area: f64 = corners
            .iter()
            .zip(corners.iter().cycle().skip(1))
            .map(|(p, q)| (p[0] * q[1] - q[0] * p[1]) / 2.0)
            .sum();
        assert!((area - 4.0 / sin).abs() < 1e-12, "{area}");
        for p in &corners {
            let s1 = p[0];
            let s2 = p[0] * cos + p[1] * sin;
            assert!(((s1 - 2.0).abs() - 1.0).abs() < 1e-12, "{p:?}");
            assert!(((s2 - 2.0).abs() - 1.0).abs() < 1e-12, "{p:?}");
        }
    }

    #[test]
    fn a_flange_corner_relief_grows_the_crossing_zone_toward_the_panel() {
        // The adjacent flanges' corner (mitcad#2411): bend 2 leaves the panel
        // at y = 28 towards +y, bend 1 at x = 38 towards +x, BA = 0.72 pi;
        // the panel lies at x <= 38, y <= 28.
        let ba = 0.72 * std::f64::consts::PI;
        let a = Strip {
            origin: [0.0, 28.0],
            into: [0.0, 1.0],
            allowance: ba,
        };
        let b = Strip {
            origin: [38.0, 0.0],
            into: [1.0, 0.0],
            allowance: ba,
        };
        // Rectangular, width 1: from 1 mm inside the panel's corner J0 to
        // both bends' far tangent lines, (BA + 1)^2.
        let zone = ccw(grown_zone(&a, &b, 1.0).unwrap());
        let mut sorted = zone.clone();
        sorted.sort_by(|p, q| p[0].total_cmp(&q[0]).then(p[1].total_cmp(&q[1])));
        let expected = [
            [37.0, 27.0],
            [37.0, 28.0 + ba],
            [38.0 + ba, 27.0],
            [38.0 + ba, 28.0 + ba],
        ];
        for (p, q) in sorted.iter().zip(expected) {
            assert!(
                (p[0] - q[0]).abs() < 1e-12 && (p[1] - q[1]).abs() < 1e-12,
                "{p:?}"
            );
        }
        let area: f64 = zone
            .iter()
            .zip(zone.iter().cycle().skip(1))
            .map(|(p, q)| (p[0] * q[1] - q[0] * p[1]) / 2.0)
            .sum();
        assert!((area - (ba + 1.0) * (ba + 1.0)).abs() < 1e-9, "{area}");
        // The round relief's centre: J0 itself.
        assert!(near(junction(&a, &b, 0.0, 0.0).unwrap(), [38.0, 28.0]));
    }

    fn port(token: u32, a: SheetDomainRef, b: SheetDomainRef) -> ShapePort {
        let side = |domain| ShapePortSide {
            boundary: BoundaryPieceRef {
                domain,
                piece: generated(FeatureUid(99), token),
            },
            span: ParamSpan {
                first: 0.0,
                last: 1.0,
            },
            axial: AxialMap {
                offset: 0.0,
                scale: 1.0,
            },
        };
        ShapePort {
            token,
            axial_interval: ParamSpan {
                first: 0.0,
                last: 1.0,
            },
            first: side(a),
            second: side(b),
        }
    }

    #[test]
    fn the_panel_of_a_junction_without_a_common_panel_is_on_the_seam_path() {
        let panel = |f: u64, slot: u32| PanelId {
            feature: FeatureUid(f),
            slot,
        };
        let bend = |f: u64| BendId {
            feature: FeatureUid(f),
            slot: 0,
        };
        let (p_left, p_right, p_mid) = (panel(3, 0), panel(3, 1), panel(3, 2));
        let (b1, b2, b3) = (bend(4), bend(5), bend(6));
        let (w1, w2) = (panel(4, 0), panel(5, 0));
        // Wall 1 -- b1 -- right panel == left panel -- b2 -- wall 2, the two
        // base panels joined by a planar seam; b3 joins the right panel and
        // a third panel that neither b1 nor b2 joins.
        let state = SheetState {
            schema: 2,
            algorithm: ALGORITHM_VERSION,
            body: BodyUid::new(FeatureUid(3), 0),
            owner: crate::ids::ComponentUid::ROOT,
            shape_generation: String::new(),
            rules: FeatureUid(1),
            rules_version: String::new(),
            panels: Vec::new(),
            charts: Vec::new(),
            bends: Vec::new(),
            joins: vec![
                port(0, SheetDomainRef::Panel(p_right), SheetDomainRef::Bend(b1)),
                port(1, SheetDomainRef::Bend(b1), SheetDomainRef::Panel(w1)),
                port(
                    2,
                    SheetDomainRef::Panel(p_left),
                    SheetDomainRef::Panel(p_right),
                ),
                port(3, SheetDomainRef::Panel(p_left), SheetDomainRef::Bend(b2)),
                port(4, SheetDomainRef::Bend(b2), SheetDomainRef::Panel(w2)),
                port(5, SheetDomainRef::Panel(p_right), SheetDomainRef::Bend(b3)),
                port(6, SheetDomainRef::Bend(b3), SheetDomainRef::Panel(p_mid)),
            ],
            opened: Vec::new(),
            lineage: Vec::new(),
            overlays: Vec::new(),
            gaps: Vec::new(),
            invalidated: None,
            blockers: Vec::new(),
        };
        // The panel next to the first bend on the path to the second.
        assert_eq!(stationary_panel(&state, b1, b2, None), Ok(p_right));
        assert_eq!(stationary_panel(&state, b2, b1, None), Ok(p_left));
        // A selected panel must join one of them.
        assert_eq!(stationary_panel(&state, b1, b2, Some(w2)), Ok(w2));
        assert_eq!(stationary_panel(&state, b1, b2, Some(w1)), Ok(w1));
        let refused = stationary_panel(&state, b1, b2, Some(p_mid)).unwrap_err();
        assert!(refused.contains("joins neither"), "{refused}");
        // With a common panel the selection keeps its earlier rule.
        assert_eq!(stationary_panel(&state, b1, b3, None), Ok(p_right));
        let refused = stationary_panel(&state, b1, b3, Some(w1)).unwrap_err();
        assert!(refused.contains("does not join both"), "{refused}");
        // The paths developed from the right panel: b1 directly, b2 through
        // the planar seam.
        let parents = reach(&state, SheetDomainRef::Panel(p_right));
        assert_eq!(
            path(&parents, SheetDomainRef::Bend(b2)).unwrap(),
            vec![
                SheetDomainRef::Panel(p_right),
                SheetDomainRef::Panel(p_left),
                SheetDomainRef::Bend(b2)
            ]
        );
        // From wall 2 the path to b1 crosses b2 as well.
        let parents = reach(&state, SheetDomainRef::Panel(w2));
        assert_eq!(
            path(&parents, SheetDomainRef::Bend(b1)).unwrap(),
            vec![
                SheetDomainRef::Panel(w2),
                SheetDomainRef::Bend(b2),
                SheetDomainRef::Panel(p_left),
                SheetDomainRef::Panel(p_right),
                SheetDomainRef::Bend(b1)
            ]
        );
    }
}
