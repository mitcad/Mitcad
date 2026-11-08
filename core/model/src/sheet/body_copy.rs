// SPDX-License-Identifier: MIT
//! Copies of sheet metal bodies (mitcad#1450, mitcad#1462).
//!
//! A mirror or pattern of bodies whose source is a valid sheet keeps its
//! copies sheets:
//!
//! - a copy that is a new body takes the source's sheet state carried by
//!   the copy's transform ([`image`]): every chart, bend, port and overlay
//!   moved with the material, a mirror turning each planar chart's
//!   material side with its plane (a reflected frame's normal is the other
//!   way), so the copy's charts are the image of the original's material.
//!   Its domains keep their identities, a sheet's domains being named per
//!   body; its faces keep the copy's names (`F6:inst1(F3:sheet_back(0))`),
//!   which the sheet's picks read as the domain inside them. A fillet's or
//!   chamfer's change overlay is read from the body it was made on, moved
//!   with the copy (mitcad#1462);
//! - a copy joined to a sheet ([`joined`], a mirror's or pattern's
//!   `combine`, or `combine_any` where the copy touches one sheet) merges
//!   the two sheets: the copy's domains and curves are renamed as the
//!   pattern's, each slot derived from the copy and the identity it
//!   replaces ([`slot`]), so an upstream edit that adds or removes domains
//!   leaves the others' names alone (mitcad#1462); every copy panel in the
//!   plane of a panel of the sheet that it touches along a seam is united
//!   with it into one panel (the seam's walls gone, its curves the trim's
//!   lineage), and the joined sheet is built again from the merged charts.
//!   It must be one solid with the volume, area and centre of mass of the
//!   two halves joined; otherwise the halves overlap or meet where no
//!   coplanar panels share a seam (a bend or a slanted wall across the
//!   plane), and the join is refused with that reason.
//!
//! A copy the sheet cannot carry (a scaled one: scaling changes the sheet's
//! thickness) and a join of several bodies at once leave ordinary solids,
//! with a warning that says so ([`scaled`], [`lost`]).

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use super::folded::develop::{loop_area, planar_map};
use super::folded::{mapped_regions, read, remap_lineage, trim_with_lineage};
use super::*;
use crate::features::EvalContext;
use crate::kernel::Kernel;
use crate::profile::SegmentGeometry;
use crate::transform::Transform;

/// The lowest slot a joined copy's domains and curves take: the pattern's
/// own identities below it (a united seam's curves) are never met.
const FIRST_SLOT: u32 = 100_000;

/// The sheet of `body` when its copies can be sheets: a valid sheet whose
/// material domains match its shape; None otherwise (the copies stay
/// ordinary solids, as before).
pub(crate) fn source<K: Kernel>(ctx: &mut EvalContext<'_, K>, body: BodyUid) -> Option<SheetState> {
    let state = ctx.sheet(body)?;
    if state.value.status() != "valid" {
        return None;
    }
    let (state, _) = read(ctx, body).ok()?;
    Some(state)
}

/// The sheet `body` had just before the feature `before` (a pattern of
/// bodies a pattern of patterns places, mitcad#1462), as [`source`] takes
/// it: the sheet the last feature before it that set the body gave it,
/// carried by the rigid moves since (a Move keeps a sheet by moving it);
/// None when that feature gave no sheet of its own (ordinary modelling)
/// or it is not valid. (The caller reads that feature's result for the
/// body's shape, `body_before`, which records the read.)
pub(crate) fn source_before<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    before: FeatureUid,
    body: BodyUid,
) -> Option<SheetState> {
    let history = ctx.env.history;
    let at = history.iter().position(|r| r.uid == before)?;
    // The moves met walking back, the latest first: the whole motion is
    // the latest after the earlier ones.
    let mut moved = Transform::IDENTITY;
    let mut state = None;
    for result in history[..at]
        .iter()
        .rev()
        .filter(|r| r.status.is_ok() && r.shape_set(body).is_some())
    {
        match result.output.as_ref()?.sheets.get(&body)? {
            SheetChange::Transform(t) if t.is_rigid() => moved = moved.after(t),
            SheetChange::Set(set) => {
                state = Some(set.transformed(&moved));
                break;
            }
            _ => return None,
        }
    }
    let mut state = state?;
    state.body = body;
    if state.status() != "valid" || state.algorithm != ALGORITHM_VERSION || state.charts.is_empty()
    {
        return None;
    }
    let rules = ctx.sheet_rules(state.rules).ok()?;
    (state.rules_version == format!("{:032x}", rules.version)).then_some(state)
}

/// Warns that the copies of sheet `body` that a scale made are ordinary
/// solids, not sheets (mitcad#1462).
pub(crate) fn scaled<K: Kernel>(ctx: &mut EvalContext<'_, K>, body: BodyUid) {
    ctx.warn(format!(
        "the scaled copies of sheet {body} are not sheet metal bodies: scaling changes the \
         sheet's thickness"
    ));
}

/// Warns that the sheets `bodies` lost their sheet meaning to a join, and
/// why (mitcad#1462).
pub(crate) fn lost<K: Kernel>(ctx: &mut EvalContext<'_, K>, bodies: &[BodyUid], why: &str) {
    if bodies.is_empty() {
        return;
    }
    let names: Vec<String> = bodies.iter().map(ToString::to_string).collect();
    let what = if bodies.len() == 1 {
        "is no longer a sheet metal body"
    } else {
        "are no longer sheet metal bodies"
    };
    ctx.warn(format!("{} {what}: {why}", names.join(", ")));
}

/// Sheets a join took the sheet meaning from, with why ([`lost`] takes
/// them): settled once the bodies the copies joined are final.
pub(crate) type Lost = Vec<(Vec<BodyUid>, String)>;

/// The sheets `pending` lost to joins, settled with the joined bodies'
/// final shapes (`results`): a joined body the sheet-domain adapter
/// re-derives (flat material joined in a panel's plane) is a sheet after
/// all, and its state is returned; the others are warned about ([`lost`]).
pub(crate) fn settle<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    pending: Lost,
    results: &[(BodyUid, &K::Shape)],
) -> BTreeMap<BodyUid, SheetState> {
    let mut derived = BTreeMap::new();
    let mut failed = BTreeSet::new();
    for (bodies, why) in pending {
        // Sheets again only when a body of them is still there and every
        // one still there is re-derived.
        let mut there = false;
        let mut all = true;
        for body in &bodies {
            let Some((_, shape)) = results.iter().find(|(b, _)| b == body) else {
                continue;
            };
            there = true;
            if derived.contains_key(body) {
                continue;
            }
            if failed.contains(body) {
                all = false;
                continue;
            }
            match super::rederive::rederive_joined(ctx, *body, shape) {
                Some(state) => {
                    derived.insert(*body, state);
                }
                None => {
                    failed.insert(*body);
                    all = false;
                }
            }
        }
        if !(there && all) {
            lost(ctx, &bodies, &why);
        }
    }
    derived
}

/// The name of a copy of `body` among a joined pattern's copies: its
/// element's index after the indices of the inner patterns' elements that
/// placed it (`steps`, patterns of patterns), `F3.b0/1` or `F3.b0/F6:1/1`.
pub(crate) fn copy_key(body: BodyUid, steps: &[(FeatureUid, u32)], index: u32) -> String {
    let mut key = body.to_string();
    for (feature, step) in steps {
        key.push_str(&format!("/{feature}:{step}"));
    }
    key.push_str(&format!("/{index}"));
    key
}

/// The sheet of a copy of `state`'s body moved by `transform`, a rigid
/// motion or a mirror: its charts, bends, ports and overlays carried with
/// the material. A reflected frame (x and y moved) has its normal the
/// other way, so a mirrored planar chart's material side is turned to stay
/// the image of the original's; a cylinder keeps its basis as given, its
/// handedness read by the construction. A change overlay keeps reading
/// its change from the body it was made on.
pub(crate) fn image(state: &SheetState, transform: &Transform, body: BodyUid) -> SheetState {
    let mut copy = state.transformed(transform);
    for overlay in &mut copy.overlays {
        if let SheetOverlayKind::Change { source, .. } = &mut overlay.kind
            && source.is_none()
        {
            *source = Some(state.body);
        }
    }
    if transform.determinant() < 0.0 {
        for chart in &mut copy.charts {
            if let SheetEmbedding::Plane {
                front_to_back_sign, ..
            } = &mut chart.embedding
            {
                *front_to_back_sign = match front_to_back_sign {
                    MaterialParity::Positive => MaterialParity::Negative,
                    MaterialParity::Negative => MaterialParity::Positive,
                };
            }
        }
    }
    copy.body = body;
    copy.shape_generation.clear();
    copy
}

/// The slot of the identity `identity` (`F5.p0`, `F2/c1[c4,c2]`) of the
/// copy `key` ([`copy_key`]) among a joined pattern's identities: their
/// 64-bit FNV-1a hash (of `<key>/<identity>`) folded to 32 bits, at
/// [`FIRST_SLOT`] or above; on a slot `taken` already, the next free one.
/// Derived from the identity and not from its rank among the copy's, so an
/// upstream edit that adds or removes domains renames no other (mitcad#1462).
fn slot(key: &str, identity: &str, taken: &mut BTreeSet<u32>) -> u32 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in key.bytes().chain(*b"/").chain(identity.bytes()) {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    let folded = ((hash >> 32) ^ hash) as u32;
    let span = u32::MAX - FIRST_SLOT + 1;
    let mut slot = FIRST_SLOT + folded % span;
    while !taken.insert(slot) {
        slot = if slot == u32::MAX {
            FIRST_SLOT
        } else {
            slot + 1
        };
    }
    slot
}

/// The domain identity a key of [`collect`] stands for, as text (`F5.p0`).
fn domain_text(key: &str) -> String {
    serde_json::from_str::<SheetDomainId>(key).map_or_else(|_| key.to_owned(), |id| id.to_string())
}

/// The curve identity a key of [`collect`] stands for, as text
/// (`F2/c1[c4,c2]`, `F5/g13`).
fn curve_text(key: &str) -> String {
    serde_json::from_str::<SheetCurveId>(key).map_or_else(|_| key.to_owned(), |id| id.to_string())
}

/// The copy's state with every domain and curve renamed as the pattern's
/// (`uid`): each slot derived from the copy `key` and the identity it
/// replaces ([`slot`]), clear of the pattern's identities `current`
/// already holds, so the copy's names never meet the sheet's it joins and
/// stay where upstream edits change other domains. Port tokens follow the
/// sheet's (`first_token` on).
fn renamed(
    copy: &SheetState,
    current: &SheetState,
    uid: FeatureUid,
    key: &str,
    first_token: u32,
) -> Result<SheetState, String> {
    let mut value = serde_json::to_value(copy).map_err(|e| e.to_string())?;
    let mut domains = BTreeSet::new();
    let mut curves = BTreeSet::new();
    collect(&value, &mut domains, &mut curves);
    // The slots the pattern's identities have in the sheet already (the
    // copies joined before this one).
    let (mut taken_domains, mut taken_curves) = (BTreeSet::new(), BTreeSet::new());
    {
        let (mut held_domains, mut held_curves) = (BTreeSet::new(), BTreeSet::new());
        let held = serde_json::to_value(current).map_err(|e| e.to_string())?;
        collect(&held, &mut held_domains, &mut held_curves);
        for text in held_domains {
            if let Ok(id) = serde_json::from_str::<SheetDomainId>(&text)
                && id.feature == uid
            {
                taken_domains.insert(id.slot);
            }
        }
        for text in held_curves {
            if let Ok(SheetCurveId::Generated { feature, slot }) = serde_json::from_str(&text)
                && feature == uid
            {
                taken_curves.insert(slot);
            }
        }
    }
    let feature = serde_json::to_value(uid).map_err(|e| e.to_string())?;
    let mut new_domains: BTreeMap<String, Value> = BTreeMap::new();
    for old in domains {
        let number = slot(key, &domain_text(&old), &mut taken_domains);
        let mut id = serde_json::Map::new();
        id.insert("feature".into(), feature.clone());
        id.insert("slot".into(), Value::from(number));
        new_domains.insert(old, Value::Object(id));
    }
    let mut new_curves: BTreeMap<String, Value> = BTreeMap::new();
    for old in curves {
        let number = slot(key, &curve_text(&old), &mut taken_curves);
        let mut id = serde_json::Map::new();
        id.insert("type".into(), Value::from("generated"));
        id.insert("feature".into(), feature.clone());
        id.insert("slot".into(), Value::from(number));
        new_curves.insert(old, Value::Object(id));
    }
    replace(&mut value, &new_domains, &new_curves);
    let mut state: SheetState = serde_json::from_value(value).map_err(|e| e.to_string())?;
    for port in &mut state.joins {
        port.token += first_token;
    }
    Ok(state)
}

/// A curve identity (`{type: sketch|generated, ...}`).
fn is_curve(m: &serde_json::Map<String, Value>) -> bool {
    match m.get("type").and_then(Value::as_str) {
        Some("sketch") => m.contains_key("sketch") && m.contains_key("segment"),
        Some("generated") => m.contains_key("feature") && m.contains_key("slot"),
        _ => false,
    }
}

/// A panel or bend identity (`{feature, slot}`).
fn is_domain(m: &serde_json::Map<String, Value>) -> bool {
    m.len() == 2 && m.contains_key("feature") && m.get("slot").is_some_and(Value::is_u64)
}

fn collect(
    v: &Value,
    domains: &mut std::collections::BTreeSet<String>,
    curves: &mut std::collections::BTreeSet<String>,
) {
    match v {
        Value::Object(m) if is_curve(m) => {
            curves.insert(v.to_string());
        }
        Value::Object(m) if is_domain(m) => {
            domains.insert(v.to_string());
        }
        Value::Object(m) => m.values().for_each(|c| collect(c, domains, curves)),
        Value::Array(a) => a.iter().for_each(|c| collect(c, domains, curves)),
        _ => {}
    }
}

fn replace(v: &mut Value, domains: &BTreeMap<String, Value>, curves: &BTreeMap<String, Value>) {
    let key = v.to_string();
    match v {
        Value::Object(m) if is_curve(m) => {
            if let Some(to) = curves.get(&key) {
                *v = to.clone();
            }
        }
        Value::Object(m) if is_domain(m) => {
            if let Some(to) = domains.get(&key) {
                *v = to.clone();
            }
        }
        Value::Object(m) => m.values_mut().for_each(|c| replace(c, domains, curves)),
        Value::Array(a) => a.iter_mut().for_each(|c| replace(c, domains, curves)),
        _ => {}
    }
}

/// The area of a chart's material.
fn area(regions: &[MaterialRegion]) -> Result<f64, String> {
    let mut total = 0.0;
    for region in regions {
        for l in &region.loops {
            total += loop_area(l)?;
        }
    }
    Ok(total)
}

/// A port side's span moved with its boundary curve by `map` (as
/// [`mapped_regions`] moves the curve: lines and splines keep their
/// parameters, a circle's angle turns and a reflection runs it the other
/// way).
fn mapped_span(regions: &[MaterialRegion], side: &ShapePortSide, map: Isometry2) -> ParamSpan {
    let sign = map.x_axis[0] * map.y_axis[1] - map.x_axis[1] * map.y_axis[0];
    let rotation = map.x_axis[1].atan2(map.x_axis[0]);
    let geometry = regions
        .iter()
        .flat_map(|r| &r.loops)
        .flat_map(|l| &l.edges)
        .find(|e| e.piece == side.boundary.piece)
        .map(|e| &e.curve.geometry);
    let span = side.span;
    match geometry {
        Some(SegmentGeometry::Circle { .. } | SegmentGeometry::Arc { .. }) => ParamSpan {
            first: rotation + sign * span.first,
            last: rotation + sign * span.last,
        },
        Some(SegmentGeometry::Ellipse { .. } | SegmentGeometry::EllipseArc { .. }) => ParamSpan {
            first: sign * span.first,
            last: sign * span.last,
        },
        _ => span,
    }
}

/// Every reference to the copy panel `from` made a reference to `to`,
/// whose chart now holds `from`'s regions moved by `map`: ports (their
/// spans moved with the curves), bends' boundaries, opened bends' fixed
/// panels and the lineage of `from`'s curves (their generations those of
/// the moved curves).
fn redirect(state: &mut SheetState, from: PanelId, to: PanelId, map: Isometry2) {
    let (old, new) = (SheetDomainRef::Panel(from), SheetDomainRef::Panel(to));
    let regions = state
        .charts
        .iter()
        .find(|p| p.domain == old)
        .map(|p| p.regions.clone())
        .unwrap_or_default();
    for port in &mut state.joins {
        for side in [&mut port.first, &mut port.second] {
            if side.boundary.domain == old {
                side.span = mapped_span(&regions, side, map);
                side.boundary.domain = new;
            }
        }
    }
    for bend in &mut state.bends {
        for boundary in [&mut bend.stationary, &mut bend.moving] {
            if boundary.domain == old {
                boundary.domain = new;
            }
        }
    }
    for opened in &mut state.opened {
        if opened.stationary == Some(from) {
            opened.stationary = Some(to);
        }
    }
    remap_lineage(&mut state.lineage, old, &regions, map);
    for record in &mut state.lineage {
        if record.boundary.domain == old {
            record.boundary.domain = new;
        }
    }
}

/// Why two halves cannot be one sheet.
const NO_SEAM: &str = "the halves meet where no coplanar panels share a seam (a bend or a wall \
                       that is not in a panel's plane crosses where they meet)";

/// The sheet `current` of a body joined with a copy
/// whose sheet is `copy` (from [`image`]) and whose material joined to
/// the body is `result` (the generic join): the merged sheet and the solid
/// built from it, or the reason the halves are not one sheet. `key` names
/// the copy ([`copy_key`]), from which its identities are derived.
pub(crate) fn joined<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    current: &SheetState,
    copy: &SheetState,
    key: &str,
    result: &K::Shape,
) -> Result<(SheetState, K::Shape), String> {
    let rules = ctx.sheet_rules(current.rules)?.value;
    if copy.rules != current.rules {
        return Err("the halves use different sheet metal rules".into());
    }
    let first_token = current.joins.iter().map(|p| p.token + 1).max().unwrap_or(0);
    let copy = renamed(copy, current, ctx.uid, key, first_token)?;
    // The copy's own panels, which are joined to the sheet's and not to
    // each other.
    let own: BTreeSet<SheetDomainRef> = copy.charts.iter().map(|p| p.domain).collect();
    let mut state = current.clone();
    state.charts.extend(copy.charts.iter().cloned());
    state.bends.extend(copy.bends.iter().cloned());
    state.joins.extend(copy.joins.iter().cloned());
    state.opened.extend(copy.opened.iter().cloned());
    state.lineage.extend(copy.lineage.iter().cloned());
    state.overlays.extend(copy.overlays.iter().cloned());
    // Each copy panel united with a panel of the sheet in its plane that it
    // touches along a seam.
    let mut seams = 0;
    for patch in &copy.charts {
        let SheetDomainRef::Panel(from) = patch.domain else {
            continue;
        };
        let candidates: Vec<SheetPatch> = state
            .charts
            .iter()
            .filter(|p| matches!(p.domain, SheetDomainRef::Panel(_)) && !own.contains(&p.domain))
            .filter(|p| planar_map(&patch.embedding, &p.embedding).is_ok())
            .cloned()
            .collect();
        for target in candidates {
            let SheetDomainRef::Panel(to) = target.domain else {
                continue;
            };
            let map = planar_map(&patch.embedding, &target.embedding)?;
            let moved = mapped_regions(&patch.regions, map);
            // Their union first: apart, touching along a seam, or
            // overlapping.
            let union = ctx
                .kernel
                .sheet_trim_domain(&SheetTrimSpec {
                    operation: SheetTrimOperation::Union,
                    domains: &[TrimDomain {
                        domain: target.domain,
                        regions: target.regions.clone(),
                        to_operation: Isometry2::IDENTITY,
                    }],
                    tools: &moved,
                    seams: &[],
                    tolerance: TOLERANCE,
                })
                .map_err(|e| e.to_string())?;
            let united: Vec<MaterialRegion> = union
                .domains
                .iter()
                .flat_map(|d| &d.regions)
                .map(|r| MaterialRegion {
                    loops: r
                        .loops
                        .iter()
                        .map(|l| MaterialLoop {
                            edges: l
                                .edges
                                .iter()
                                .map(|e| MaterialEdge {
                                    piece: super::folded::generated(ctx.uid, 0),
                                    curve: e.curve.clone(),
                                })
                                .collect(),
                        })
                        .collect(),
                })
                .collect();
            if united.len() >= target.regions.len() + patch.regions.len() {
                // Apart in the plane.
                continue;
            }
            let expected = area(&target.regions)? + area(&patch.regions)?;
            let got = area(&united)?;
            if got < expected - 1e-6 * expected.abs().max(1.0) {
                return Err(format!(
                    "the halves overlap: {:.3} mm2 of the panel of {} lie in both",
                    expected - got,
                    ctx.feature_name(to.feature)
                ));
            }
            let mut trial = state.clone();
            redirect(&mut trial, from, to, map);
            trial.charts.retain(|p| p.domain != patch.domain);
            let chart = trial
                .charts
                .iter_mut()
                .find(|p| p.domain == target.domain)
                .ok_or("a joined panel is unavailable")?;
            chart.regions.extend(moved.iter().cloned());
            trim_with_lineage(
                ctx,
                &mut trial,
                &[target.domain],
                SheetTrimOperation::Union,
                &moved,
                &[Isometry2::IDENTITY],
                None,
            )?;
            state = trial;
            seams += 1;
            break;
        }
    }
    if seams == 0 {
        return Err(NO_SEAM.into());
    }
    let geometry = ctx
        .kernel
        .sheet_shape(
            &SheetShapeSpec {
                thickness: rules.thickness,
                patches: &state.charts,
                ports: &state.joins,
                tolerance: TOLERANCE,
            },
            &ctx.uid.to_string(),
        )
        .map_err(|e| super::folded::name_walls(ctx, &e.to_string()))?;
    if geometry.solids.len() != 1 {
        return Err(NO_SEAM.into());
    }
    let solid = geometry.solids.into_iter().next().expect("one solid");
    let shape = super::folded::cut::with_overlays(ctx, &state, solid.shape)?;
    // The joined sheet must be the two halves joined.
    let mass = |shape: &K::Shape| ctx.kernel.mass_properties(shape).map_err(|e| e.to_string());
    let (built, actual) = (mass(&shape)?, mass(result)?);
    let size = actual.volume.abs().cbrt().max(1.0);
    let off = |a: f64, b: f64, scale: f64| (a - b).abs() > 1e-6 * scale.max(1.0);
    if built.volume > actual.volume + 1e-6 * actual.volume.abs().max(1.0) {
        return Err(format!(
            "the halves overlap: {:.3} mm3 of material lies in both",
            built.volume - actual.volume
        ));
    }
    if off(built.volume, actual.volume, actual.volume.abs())
        || off(built.area, actual.area, actual.area.abs())
        || (0..3).any(|i| off(built.center[i], actual.center[i], size))
    {
        return Err(NO_SEAM.into());
    }
    state.panels = super::folded::legacy_panels(&state.charts, &rules)?;
    state.shape_generation.clear();
    Ok((state, shape))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copies_are_keyed_by_body_inner_places_and_element() {
        let body = BodyUid::new(FeatureUid(3), 0);
        assert_eq!(copy_key(body, &[], 1), "F3.b0/1");
        assert_eq!(copy_key(body, &[(FeatureUid(6), 1)], 2), "F3.b0/F6:1/2");
    }

    #[test]
    fn slots_come_from_the_identity_not_its_rank() {
        // The slot the CLI test sheet_copy_stable_slots names: FNV-1a of
        // "F3.b0/1/F5.p0" folded to 32 bits, past FIRST_SLOT.
        let mut taken = BTreeSet::new();
        assert_eq!(slot("F3.b0/1", "F5.p0", &mut taken), 1_108_818_023);
        // An identity that sorts before it, added upstream, leaves it.
        let mut taken = BTreeSet::new();
        let added = slot("F3.b0/1", "F14.p0", &mut taken);
        assert_ne!(added, 1_108_818_023);
        assert!(added >= FIRST_SLOT);
        assert_eq!(slot("F3.b0/1", "F5.p0", &mut taken), 1_108_818_023);
        // Another copy of it is named elsewhere.
        assert_ne!(
            slot("F3.b0/2", "F5.p0", &mut BTreeSet::new()),
            1_108_818_023
        );
        // A slot taken already gives the next free one.
        let mut taken = BTreeSet::from([1_108_818_023, 1_108_818_024]);
        assert_eq!(slot("F3.b0/1", "F5.p0", &mut taken), 1_108_818_025);
    }

    #[test]
    fn identities_are_named_as_the_model_writes_them() {
        assert_eq!(domain_text(r#"{"feature":"F5","slot":0}"#), "F5.p0");
        assert_eq!(
            curve_text(r#"{"type":"generated","feature":"F5","slot":13}"#),
            "F5/g13"
        );
    }

    #[test]
    fn a_change_overlay_names_its_source_only_on_copies() {
        let own = SheetOverlay {
            feature: FeatureUid(4),
            kind: SheetOverlayKind::Change {
                placement: Transform::IDENTITY,
                source: None,
            },
            removed: 5.0,
        };
        let value = serde_json::to_value(&own).unwrap();
        assert!(value.get("source").is_none(), "{value}");
        assert_eq!(serde_json::from_value::<SheetOverlay>(value).unwrap(), own);
        let copied = SheetOverlay {
            kind: SheetOverlayKind::Change {
                placement: Transform::IDENTITY,
                source: Some(BodyUid::new(FeatureUid(3), 0)),
            },
            ..own
        };
        let value = serde_json::to_value(&copied).unwrap();
        assert_eq!(value["source"], "F3.b0");
        assert_eq!(
            serde_json::from_value::<SheetOverlay>(value).unwrap(),
            copied
        );
    }
}
