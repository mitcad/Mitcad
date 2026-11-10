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
//!   lineage), every copy bend on the cylinder of a bend of the sheet that
//!   it touches likewise into one bend (a bend across the mirror plane,
//!   mitcad#1558: its record's axial interval covering both, the seams of
//!   the two side by side along each tangent line), and the joined sheet
//!   is built again from the merged charts. It must be one solid with the
//!   volume, area and centre of mass of the two halves joined; otherwise
//!   the halves overlap or meet where no coplanar panels or coaxial bends
//!   share a seam (a slanted wall across the plane), and the join is
//!   refused with that reason;
//! - a Combine's join of a sheet with sheet tools ([`combined`],
//!   mitcad#1558) merges them as a joined copy, each tool keyed by its
//!   body; where they are not one sheet, the target is re-derived by the
//!   sheet-domain adapter or left invalid with the reason ([`not_combined`]).
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

/// The isometry from the chart of the copy's domain `from` to the chart of
/// the sheet's domain `to` where both are one material surface: two panels
/// in one plane with one material side ([`planar_map`]), or two bends of
/// the same radius, allowance and angle on one cylinder over the same
/// angle (a bend and its image in a mirror plane across its axis, or a
/// copy moved along it, mitcad#1558: [`cylinder_map`]); bends opened flat
/// as panels are. Err where they are not.
fn chart_map(state: &SheetState, from: &SheetPatch, to: &SheetPatch) -> Result<Isometry2, String> {
    match (from.domain, to.domain) {
        (SheetDomainRef::Panel(_), SheetDomainRef::Panel(_)) => {
            planar_map(&from.embedding, &to.embedding)
        }
        (SheetDomainRef::Bend(a), SheetDomainRef::Bend(b)) => {
            let record = |id: BendId| {
                state
                    .bends
                    .iter()
                    .find(|r| r.id == id)
                    .ok_or_else(|| "a joined bend has no record".to_owned())
            };
            let (a, b) = (record(a)?, record(b)?);
            let near = |x: f64, y: f64| (x - y).abs() <= 1e-9 * x.abs().max(y.abs()).max(1.0);
            if !near(a.signed_angle, b.signed_angle)
                || !near(a.radius, b.radius)
                || !near(a.k_factor, b.k_factor)
                || !near(a.allowance, b.allowance)
            {
                return Err("the bends differ in angle, radius or allowance".into());
            }
            match (&from.embedding, &to.embedding) {
                (SheetEmbedding::Cylinder(x), SheetEmbedding::Cylinder(y)) => cylinder_map(x, y),
                (SheetEmbedding::Plane { .. }, SheetEmbedding::Plane { .. }) => {
                    planar_map(&from.embedding, &to.embedding)
                }
                _ => Err("one bend is open and the other folded".into()),
            }
        }
        _ => Err("a panel and a bend are not one material domain".into()),
    }
}

/// The isometry from the chart of the bend cylinder `from` to that of `to`
/// (mitcad#1558): both on one axis with the same radii, angle and material
/// side, `from`'s arc either `to`'s (`u` kept) or `to`'s run the other way
/// (`u` to `BA - u`), `v` moved along the common axis or reversed with it
/// (a mirror across the axis turns it). Err where the cylinders differ.
fn cylinder_map(from: &CylinderEmbedding, to: &CylinderEmbedding) -> Result<Isometry2, String> {
    use mitcad_vec3::{add, distance, dot, scale, sub};
    const DIRECTION: f64 = 1.0e-9;
    let near = |x: f64, y: f64| (x - y).abs() <= 1e-9 * x.abs().max(y.abs()).max(1.0);
    if !near(from.inside_radius, to.inside_radius)
        || !near(from.neutral_radius, to.neutral_radius)
        || !near(from.angle_extent, to.angle_extent)
        || from.inner_is_front != to.inner_is_front
    {
        return Err("the bends differ in radius or angle".into());
    }
    let s = dot(from.axis, to.axis);
    if (s.abs() - 1.0).abs() > DIRECTION {
        return Err("the bends are not coaxial".into());
    }
    let d = sub(from.centre, to.centre);
    let along = dot(d, to.axis);
    if distance(d, scale(to.axis, along)) > TOLERANCE {
        return Err("the bends are not coaxial".into());
    }
    let radial = |c: &CylinderEmbedding, phi: f64| {
        add(
            scale(c.radial_start, phi.cos()),
            scale(c.angular_direction, phi.sin()),
        )
    };
    let theta = to.angle_extent;
    let same = |phis: [(f64, f64); 3]| {
        phis.iter()
            .all(|(a, b)| distance(radial(from, *a), radial(to, *b)) <= DIRECTION)
    };
    let middle = theta / 2.0;
    let (start, sigma) = if same([(0.0, 0.0), (middle, middle), (theta, theta)]) {
        (0.0, 1.0)
    } else if same([(0.0, theta), (middle, middle), (theta, 0.0)]) {
        (to.neutral_radius * theta, -1.0)
    } else {
        return Err("the bends turn over different angles about their axis".into());
    };
    Ok(Isometry2 {
        origin: [start, along],
        x_axis: [sigma, 0.0],
        y_axis: [0.0, s.signum()],
    })
}

/// Every reference to the copy's domain `from` made a reference to `to`,
/// whose chart now holds `from`'s regions moved by `map`: ports (their
/// spans moved with the curves), bends' boundaries, opened bends' fixed
/// panels and the lineage of `from`'s curves (their generations those of
/// the moved curves). A bend's seams take `to`'s axial coordinate (each
/// seam's whole axial map moved: the seams of the two bends lie side by
/// side along each united tangent line, all in `to`'s `v`); its record and
/// open state go into `to`'s, whose axial interval covers both
/// (mitcad#1558).
fn redirect(
    state: &mut SheetState,
    old: SheetDomainRef,
    new: SheetDomainRef,
    map: Isometry2,
) -> Result<(), String> {
    let regions = state
        .charts
        .iter()
        .find(|p| p.domain == old)
        .map(|p| p.regions.clone())
        .unwrap_or_default();
    if let (SheetDomainRef::Bend(from), SheetDomainRef::Bend(to)) = (old, new) {
        // Each seam's axial coordinate `a` becomes `alpha + beta a`, read
        // from its side on the bend: the bend's `v` at its ends, mapped.
        let mut moves = Vec::new();
        for (index, port) in state.joins.iter().enumerate() {
            let Some(side) = [&port.first, &port.second]
                .into_iter()
                .find(|s| s.boundary.domain == old)
            else {
                continue;
            };
            let (p0, p1) = super::folded::port_ends(state, side)
                .ok_or("a seam of a joined bend has no material boundary")?;
            let axial = |t: f64| side.axial.offset + side.axial.scale * t;
            let (a0, a1) = (axial(side.span.first), axial(side.span.last));
            let (b0, b1) = (map.point(p0)[1], map.point(p1)[1]);
            if (a1 - a0).abs() <= TOLERANCE {
                return Err("a seam of a joined bend has no length".into());
            }
            let beta = (b1 - b0) / (a1 - a0);
            moves.push((index, b0 - beta * a0, beta));
        }
        for (index, alpha, beta) in moves {
            let port = &mut state.joins[index];
            for side in [&mut port.first, &mut port.second] {
                side.axial = AxialMap {
                    offset: alpha + beta * side.axial.offset,
                    scale: beta * side.axial.scale,
                };
            }
            let (x, y) = (
                alpha + beta * port.axial_interval.first,
                alpha + beta * port.axial_interval.last,
            );
            port.axial_interval = ParamSpan {
                first: x.min(y),
                last: x.max(y),
            };
        }
        let interval = state
            .bends
            .iter()
            .find(|r| r.id == from)
            .map(|r| r.axial_interval);
        state.bends.retain(|r| r.id != from);
        if let (Some(i), Some(record)) = (interval, state.bends.iter_mut().find(|r| r.id == to)) {
            let (x, y) = (map.point([0.0, i.first])[1], map.point([0.0, i.last])[1]);
            let own = record.axial_interval;
            record.axial_interval = ParamSpan {
                first: own.first.min(own.last).min(x).min(y),
                last: own.first.max(own.last).max(x).max(y),
            };
        }
        if state.opened.iter().any(|o| o.bend == to) {
            state.opened.retain(|o| o.bend != from);
        } else {
            for opened in &mut state.opened {
                if opened.bend == from {
                    opened.bend = to;
                }
            }
        }
    }
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
    if let (SheetDomainRef::Panel(from), SheetDomainRef::Panel(to)) = (old, new) {
        for opened in &mut state.opened {
            if opened.stationary == Some(from) {
                opened.stationary = Some(to);
            }
        }
    }
    remap_lineage(&mut state.lineage, old, &regions, map);
    for record in &mut state.lineage {
        if record.boundary.domain == old {
            record.boundary.domain = new;
        }
    }
    Ok(())
}

/// The port side `side` bound by position to the straight boundary of its
/// domain's chart in `state` that holds both its ends `p0` and `p1`, its
/// axial coordinates at the ends kept (as `cut::rebind`, on any boundary
/// curve). None where no boundary holds them.
fn seat(
    state: &SheetState,
    side: &ShapePortSide,
    p0: [f64; 2],
    p1: [f64; 2],
) -> Option<ShapePortSide> {
    use crate::sketch::geometry::{dist, dot, sub};
    let patch = state
        .charts
        .iter()
        .find(|p| p.domain == side.boundary.domain)?;
    for edge in patch
        .regions
        .iter()
        .flat_map(|r| &r.loops)
        .flat_map(|l| &l.edges)
    {
        let SegmentGeometry::Line { start, end } = edge.curve.geometry else {
            continue;
        };
        let d = sub(end, start);
        let len = dot(d, d);
        if len <= 0.0 {
            continue;
        }
        let at = |p: [f64; 2]| dot(sub(p, start), d) / len;
        let (t0, t1) = (at(p0), at(p1));
        let (lo, hi) = (
            edge.curve.range.first.min(edge.curve.range.last),
            edge.curve.range.first.max(edge.curve.range.last),
        );
        let on = |t: f64, p: [f64; 2]| {
            t >= lo - 1e-9
                && t <= hi + 1e-9
                && edge.curve.point(t).is_ok_and(|q| dist(p, q) <= TOLERANCE)
        };
        if (t1 - t0).abs() > 0.0 && on(t0, p0) && on(t1, p1) {
            let a0 = side.axial.offset + side.axial.scale * side.span.first;
            let a1 = side.axial.offset + side.axial.scale * side.span.last;
            let scale = (a1 - a0) / (t1 - t0);
            return Some(ShapePortSide {
                boundary: BoundaryPieceRef {
                    domain: side.boundary.domain,
                    piece: edge.piece.clone(),
                },
                span: ParamSpan {
                    first: t0,
                    last: t1,
                },
                axial: AxialMap {
                    offset: a0 - scale * t0,
                    scale,
                },
            });
        }
    }
    None
}

/// The port sides on `domain` that its union left on a boundary piece it
/// no longer has, bound again by position (mitcad#1558). Where the union
/// returns the tangent line of the sheet and that of the copy as one
/// boundary, its piece is the first of their identities: the seam on the
/// other one is found again where its ends lie (`before`: the state the
/// union was made on), which `trim_with_lineage`'s rebinding, keeping to
/// one curve, does not do.
fn reseat(before: &SheetState, after: &mut SheetState, domain: SheetDomainRef) {
    let mut joins = after.joins.clone();
    for port in &mut joins {
        for side in [&mut port.first, &mut port.second] {
            if side.boundary.domain != domain || super::folded::port_ends(after, side).is_some() {
                continue;
            }
            let Some((p0, p1)) = super::folded::port_ends(before, side) else {
                continue;
            };
            if let Some(found) = seat(after, side, p0, p1) {
                *side = found;
            }
        }
    }
    after.joins = joins;
}

/// Why two halves cannot be one sheet.
const NO_SEAM: &str = "the halves meet where no coplanar panels or coaxial bends share a seam \
                       (a wall that is not in a panel's plane, or a bend that is not on one \
                       cylinder with the other half's, crosses where they meet)";

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
    // touches along a seam, and each copy bend with a bend of the sheet on
    // its cylinder (mitcad#1558: a bend across the mirror plane).
    let mut seams = 0;
    for patch in &copy.charts {
        let from = patch.domain;
        let candidates: Vec<(SheetPatch, Isometry2)> = state
            .charts
            .iter()
            .filter(|p| !own.contains(&p.domain))
            .filter_map(|p| chart_map(&state, patch, p).ok().map(|map| (p.clone(), map)))
            .collect();
        for (target, map) in candidates {
            let to = target.domain;
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
                let what = match to {
                    SheetDomainRef::Panel(_) => "panel",
                    SheetDomainRef::Bend(_) => "bend",
                };
                return Err(format!(
                    "the halves overlap: {:.3} mm2 of the {what} of {} lie in both",
                    expected - got,
                    ctx.feature_name(to.feature())
                ));
            }
            let mut trial = state.clone();
            redirect(&mut trial, from, to, map)?;
            trial.charts.retain(|p| p.domain != from);
            let chart = trial
                .charts
                .iter_mut()
                .find(|p| p.domain == to)
                .ok_or("a joined material domain is unavailable")?;
            chart.regions.extend(moved.iter().cloned());
            let before = trial.clone();
            trim_with_lineage(
                ctx,
                &mut trial,
                &[to],
                SheetTrimOperation::Union,
                &moved,
                &[Isometry2::IDENTITY],
                None,
            )?;
            reseat(&before, &mut trial, to);
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

/// The sheet of a Combine's join (mitcad#1558): where the target and every
/// tool are valid sheets (`tools`: their bodies and shapes), each tool is
/// merged into the target's sheet as a joined copy is ([`joined`], keyed by
/// the tool's body), in order, each against the generic join of the target
/// so far and that tool (`whole`, the generic join of them all, for one
/// tool). The merged sheet and the solid built from it; None where a body
/// is not a valid sheet (the sheet-domain adapter takes the join, as for
/// ordinary modelling); Err with the reason the sheets are not one sheet.
/// A fillet's or chamfer's overlay on a tool keeps reading its change from
/// the tool.
pub(crate) fn combined<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    target: (BodyUid, &K::Shape),
    tools: &[(BodyUid, &K::Shape)],
    whole: &K::Shape,
) -> Option<Result<(SheetState, K::Shape), String>> {
    let mut current = source(ctx, target.0)?;
    let mut sheets = Vec::new();
    for (body, shape) in tools {
        let mut state = source(ctx, *body)?;
        for overlay in &mut state.overlays {
            if let SheetOverlayKind::Change { source, .. } = &mut overlay.kind
                && source.is_none()
            {
                *source = Some(*body);
            }
        }
        sheets.push((*body, state, *shape));
    }
    let mut shape = target.1.clone();
    for (body, sheet, tool) in sheets {
        let result = if tools.len() == 1 {
            whole.clone()
        } else {
            match ctx.unite(&[&shape, tool]) {
                Ok(result) => result,
                Err(e) => return Some(Err(e.to_string())),
            }
        };
        if matches!(ctx.kernel.solid_count(&result), Ok(n) if n > 1) {
            return Some(Err(format!(
                "{body} does not touch it: sheets are joined where coplanar panels or coaxial \
                 bends meet at a seam"
            )));
        }
        match joined(ctx, &current, &sheet, &body.to_string(), &result) {
            Ok((state, rebuilt)) => {
                current = state;
                shape = rebuilt;
            }
            Err(e) => return Some(Err(format!("{body} cannot be joined to it: {e}"))),
        }
    }
    Some(Ok((current, shape)))
}

/// The sheet of a Combine's target whose join with sheet tools is not one
/// sheet (`why`, from [`combined`]): re-derived where the sheet-domain
/// adapter can (flat material joined in a panel's plane), else invalid
/// with that reason (mitcad#1558).
pub(crate) fn not_combined<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    target: BodyUid,
    shape: &K::Shape,
    why: &str,
) -> SheetChange {
    match super::rederive::rederive_joined(ctx, target, shape) {
        Some(state) => SheetChange::Set(state),
        None => SheetChange::Invalidate(format!("the joined bodies are not one sheet: {why}")),
    }
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

    /// A 90-degree bend along y from y = 50, r = 3, neutral radius 3.6,
    /// turning from -z towards +x.
    fn bend_at(centre: [f64; 3], axis: [f64; 3]) -> CylinderEmbedding {
        CylinderEmbedding {
            centre,
            axis,
            radial_start: [0.0, 0.0, -1.0],
            angular_direction: [1.0, 0.0, 0.0],
            inside_radius: 3.0,
            neutral_radius: 3.6,
            angle_extent: std::f64::consts::FRAC_PI_2,
            inner_is_front: false,
        }
    }

    fn assert_map(map: Isometry2, origin: [f64; 2], x_axis: [f64; 2], y_axis: [f64; 2]) {
        for (got, want) in [
            (map.origin, origin),
            (map.x_axis, x_axis),
            (map.y_axis, y_axis),
        ] {
            assert!(
                (got[0] - want[0]).abs() <= 1e-9 && (got[1] - want[1]).abs() <= 1e-9,
                "{map:?}"
            );
        }
    }

    #[test]
    fn a_bend_mirrored_across_its_axis_maps_onto_the_bend() {
        // mitcad#1558: the bend at y = 50..70 mirrored about y = 70 lies on
        // y = 70..90, its axis reversed: v runs back from 40.
        let bend = bend_at([0.0, 50.0, 5.0], [0.0, 1.0, 0.0]);
        let image = bend_at([0.0, 90.0, 5.0], [0.0, -1.0, 0.0]);
        let map = cylinder_map(&image, &bend).unwrap();
        assert_map(map, [0.0, 40.0], [1.0, 0.0], [0.0, -1.0]);
        // Its strip [0, BA] x [0, 20] lands on [0, BA] x [20, 40].
        let corner = map.point([0.0, 0.0]);
        assert!(corner[0].abs() <= 1e-12 && (corner[1] - 40.0).abs() <= 1e-12);
        assert!((map.point([1.0, 20.0])[1] - 20.0).abs() <= 1e-12);
        // A copy moved along the axis keeps v's direction.
        let moved = bend_at([0.0, 70.0, 5.0], [0.0, 1.0, 0.0]);
        assert_map(
            cylinder_map(&moved, &bend).unwrap(),
            [0.0, 20.0],
            [1.0, 0.0],
            [0.0, 1.0],
        );
    }

    #[test]
    fn a_bend_turning_the_other_way_over_the_same_arc_runs_u_back() {
        // The arc from +x back to -z: u becomes BA - u.
        let bend = bend_at([0.0, 50.0, 5.0], [0.0, 1.0, 0.0]);
        let reversed = CylinderEmbedding {
            radial_start: [1.0, 0.0, 0.0],
            angular_direction: [0.0, 0.0, -1.0],
            ..bend_at([0.0, 30.0, 5.0], [0.0, 1.0, 0.0])
        };
        let allowance = 3.6 * std::f64::consts::FRAC_PI_2;
        assert_map(
            cylinder_map(&reversed, &bend).unwrap(),
            [allowance, -20.0],
            [-1.0, 0.0],
            [0.0, 1.0],
        );
    }

    #[test]
    fn bends_off_the_axis_or_of_another_radius_or_arc_are_not_one_bend() {
        let bend = bend_at([0.0, 50.0, 5.0], [0.0, 1.0, 0.0]);
        // Parallel but 1 mm aside.
        assert!(cylinder_map(&bend_at([1.0, 90.0, 5.0], [0.0, -1.0, 0.0]), &bend).is_err());
        // Crossing it.
        assert!(cylinder_map(&bend_at([0.0, 50.0, 5.0], [1.0, 0.0, 0.0]), &bend).is_err());
        // Another radius.
        let wider = CylinderEmbedding {
            inside_radius: 4.0,
            neutral_radius: 4.6,
            ..bend_at([0.0, 90.0, 5.0], [0.0, -1.0, 0.0])
        };
        assert!(cylinder_map(&wider, &bend).is_err());
        // The other quarter of the cylinder.
        let other = CylinderEmbedding {
            radial_start: [0.0, 0.0, 1.0],
            angular_direction: [-1.0, 0.0, 0.0],
            ..bend_at([0.0, 90.0, 5.0], [0.0, -1.0, 0.0])
        };
        assert!(cylinder_map(&other, &bend).is_err());
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
