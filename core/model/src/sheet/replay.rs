// SPDX-License-Identifier: MIT
//! Patterns and mirrors of sheet features (mitcad#242).
//!
//! A pattern or mirror of features that change a sheet body repeats them
//! on the sheet, one copy after another, keeping it a sheet:
//!
//! - a flange is built again on the copy of its boundary: the panel
//!   boundary of the current sheet that the element's transform carries
//!   the original's boundary onto, with the bend turning to the side the
//!   transform carries the original's bend side to (its offsets and width
//!   turned with the boundary's direction);
//! - every other feature (a joined face, a cut, a hole, a corner round or
//!   chamfer, a relief) repeats the material it changed: the material it
//!   added joined and the material it removed cut, moved by the transform,
//!   and the sheet's domains re-derived from the result (the sheet-domain
//!   adapter, [`super::rederive`]).
//!
//! Domains and curves each copy makes are the pattern's own (its feature
//! identity, numbered per copy).

use mitcad_vec3::{add, distance as dist, sub};

use super::folded::cut::{straight_cut_on, straight_cuts_at_once, straight_profiles};
use super::folded::{boundary, flange, line, plane, read};
use super::*;
use crate::features::sheet_cut::SheetCutMode;
use crate::features::sheet_folded::SheetFlangeWidth;
use crate::features::{BodyChange, EvalContext, FeatureDef, FeatureOutput};
use crate::kernel::{BooleanOp, Kernel};
use crate::recompute::Versioned;
use crate::transform::{Instance, Transform};
use mitcad_vec3::dot;

/// One copy: the feature, the transform from its place to the copy's, and
/// the instance that names the copy.
pub(crate) struct Copy {
    pub feature: FeatureUid,
    pub transform: Transform,
    pub instance: Instance,
}

/// The pattern's result when every copy changes one valid sheet body;
/// None when the copies do not (the generic copies apply then).
pub(crate) fn repeat<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    copies: &[Copy],
) -> Result<Option<FeatureOutput<K::Shape>>, String> {
    let mut target = None;
    for copy in copies {
        let changed = ctx.feature_changed_bodies(copy.feature)?;
        let [body] = changed.as_slice() else {
            return Ok(None);
        };
        if target.is_some_and(|t| t != *body) {
            return Ok(None);
        }
        target = Some(*body);
    }
    let Some(body) = target else {
        return Ok(None);
    };
    if ctx.sheet(body).is_none_or(|s| s.value.status() != "valid") {
        return Ok(None);
    }
    let (mut state, rules) = read(ctx, body)?;
    let mut shape = ctx.body(body)?;
    let mut done = std::collections::BTreeSet::new();
    if let Some((next_shape, next_state, left)) =
        straight_copies_at_once(ctx, copies, body, &state)?
    {
        shape = next_shape;
        state = next_state;
        done = (0..copies.len()).filter(|k| !left.contains(k)).collect();
    }
    for (k, copy) in copies.iter().enumerate() {
        if done.contains(&k) {
            continue;
        }
        let entry = ctx.feature_entry(copy.feature)?;
        let before = state.clone();
        let offset = 100_000 * (k as u32 + 1);
        let (next_shape, next_state) = match &entry.def {
            FeatureDef::SheetFlange(def) => {
                let d = def.map_params(&mut |_, p| ctx.param(*p))?;
                let original = state_before(ctx, copy.feature, body)
                    .ok_or_else(|| format!("{} has no sheet before it to copy from", entry.name))?;
                let d = moved_flange(&d, &original, &state, rules.thickness, &copy.transform)
                    .map_err(|e| format!("the copy of {}: {e}", entry.name))?;
                let output = with_sheet(ctx, body, &shape, &state, |child| flange(&d, child))
                    .map_err(|e| format!("the copy of {}: {e}", entry.name))?;
                published(output, body)?
            }
            FeatureDef::SheetCut(def) if def.mode == SheetCutMode::Straight => {
                // The cut again from the copy of its sketch's plane along
                // the copy of its direction: re-derived where it stays in
                // panels, an overlay of the copy where it crosses a bend.
                let d = def.map_params(&mut |_, p| ctx.param(*p))?;
                let (frame, profiles) = straight_profiles(&d, ctx)?;
                let direction = frame.normal().map(|v| if d.flip { -v } else { v });
                let frame = copy.transform.apply_frame(&frame);
                let direction = copy.transform.apply_vector(direction);
                let output = with_sheet(ctx, body, &shape, &state, |child| {
                    straight_cut_on(child, body, frame, profiles, direction, d.extent.as_ref())
                })
                .map_err(|e| format!("the copy of {}: {e}", entry.name))?;
                published(output, body)?
            }
            _ if kept_as_change(ctx, copy.feature, Some(body)) => {
                // A feature the sheet keeps as a change overlay (an ordinary
                // fillet or chamfer the domains do not hold): its copy is
                // kept the same way, the change moved by the transform.
                let changed = repeat_change(ctx, copy, body, &shape)?;
                let volume = |shape: &K::Shape| -> Result<f64, String> {
                    Ok(ctx
                        .kernel
                        .mass_properties(shape)
                        .map_err(|e| e.to_string())?
                        .volume)
                };
                let mut next = state.clone();
                next.overlays.push(SheetOverlay {
                    feature: copy.feature,
                    kind: SheetOverlayKind::Change {
                        placement: copy.transform,
                    },
                    removed: volume(&shape)? - volume(&changed)?,
                });
                (changed, next)
            }
            _ => {
                let changed = repeat_change(ctx, copy, body, &shape)?;
                let derived =
                    super::rederive::rederive_from(ctx, state.clone(), &rules, &shape, &changed)
                        .map_err(|e| {
                            format!("the copy of {} is not a sheet change: {e}", entry.name)
                        })?;
                (changed, derived)
            }
        };
        shape = next_shape;
        state = renumber(&before, next_state, ctx.uid, offset)?;
    }
    state.shape_generation.clear();
    let mut output = FeatureOutput::default();
    output.changes.push(BodyChange::Set(body, shape));
    output.sheets.insert(body, SheetChange::Set(state));
    Ok(Some(output))
}

/// Copies that are all of one straight sheet cut (mitcad#288: patterns of
/// hundreds of vent holes), cut at once: each copy's tool from the copy of
/// the sketch's plane along the copy of its direction, one boolean and one
/// re-derivation of the domains for the copies clear of the bends' seams
/// ([`straight_cuts_at_once`]), the result the same as cutting them one
/// after another. Returns the shape, the sheet and the indices of the
/// copies left to cut one by one; None when they are not such copies or
/// cannot be taken together (then they are all cut one by one).
#[allow(clippy::type_complexity)]
fn straight_copies_at_once<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    copies: &[Copy],
    body: BodyUid,
    before: &SheetState,
) -> Result<Option<(K::Shape, SheetState, Vec<usize>)>, String> {
    let Some(first) = copies.first() else {
        return Ok(None);
    };
    if copies.len() < 2 || copies.iter().any(|c| c.feature != first.feature) {
        return Ok(None);
    }
    let entry = ctx.feature_entry(first.feature)?;
    let FeatureDef::SheetCut(def) = &entry.def else {
        return Ok(None);
    };
    if def.mode != SheetCutMode::Straight {
        return Ok(None);
    }
    let name = entry.name.clone();
    let d = def.map_params(&mut |_, p| ctx.param(*p))?;
    let (frame, profiles) = straight_profiles(&d, ctx)?;
    let direction = frame.normal().map(|v| if d.flip { -v } else { v });
    let cuts = copies
        .iter()
        .map(|copy| {
            (
                copy.transform.apply_frame(&frame),
                profiles.clone(),
                copy.transform.apply_vector(direction),
            )
        })
        .collect();
    let Some((shape, next, left)) = with_sheet_result(ctx, body, before, |child| {
        straight_cuts_at_once(child, body, cuts, d.extent.as_ref())
    })
    .map_err(|e| format!("the copies of {name}: {e}"))?
    else {
        return Ok(None);
    };
    // Numbered apart from the copies cut one by one (offsets 100000 k).
    let offset = 100_000 * (copies.len() as u32 + 1);
    let state = renumber(before, next, ctx.uid, offset)?;
    Ok(Some((shape, state, left)))
}

/// Runs `run` in a child context of the pattern on the current body and
/// sheet (as [`with_sheet`] does for one copy).
fn with_sheet_result<K: Kernel, T>(
    ctx: &mut EvalContext<'_, K>,
    body: BodyUid,
    state: &SheetState,
    run: impl FnOnce(&mut EvalContext<'_, K>) -> Result<T, String>,
) -> Result<T, String> {
    let shape = ctx.body(body)?;
    let mut result = None;
    with_sheet(ctx, body, &shape, state, |child| {
        result = Some(run(child));
        Ok(FeatureOutput::default())
    })?;
    result.expect("ran")
}

/// The copies of faces that made new sheet bodies (mitcad#280), when every
/// copy is one: each body such a face made is copied as a new sheet body
/// of the pattern, its charts moved by the copy's transform (a mirror
/// turns the material's side with the planes: the copy is the image of the
/// original's material) and its domains renamed as the pattern's,
/// numbered per copy; the solid is built again from the moved charts. None
/// when a copy is of another feature (the generic copies apply then).
pub(crate) fn copy_faces<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    copies: &[Copy],
) -> Result<Option<FeatureOutput<K::Shape>>, String> {
    if copies.is_empty()
        || !copies.iter().all(|c| {
            ctx.feature_entry(c.feature)
                .is_ok_and(|e| matches!(&e.def, FeatureDef::SheetFace(d) if d.body.is_none()))
        })
    {
        return Ok(None);
    }
    let owner = ctx
        .env
        .features
        .iter()
        .find(|f| f.uid == ctx.uid)
        .ok_or("the pattern is not in the timeline")?
        .component;
    let mut output = FeatureOutput::default();
    let mut next = 0;
    for (k, copy) in copies.iter().enumerate() {
        let name = ctx.feature_name(copy.feature);
        let made: Vec<SheetState> = ctx
            .env
            .history
            .iter()
            .find(|r| r.uid == copy.feature && r.status.is_ok())
            .and_then(|r| r.output.as_ref())
            .ok_or_else(|| format!("{name} has no result to copy"))?
            .sheets
            .values()
            .filter_map(|change| match change {
                SheetChange::Set(state) => Some(state.clone()),
                _ => None,
            })
            .collect();
        if made.is_empty() {
            return Err(format!("{name} made no sheet to copy"));
        }
        let reflected = copy.transform.determinant() < 0.0;
        let offset = 100_000 * (k as u32 + 1);
        for original in made {
            let rules = ctx.sheet_rules(original.rules)?;
            if !original.bends.is_empty() || !original.overlays.is_empty() {
                return Err(format!("{name}'s sheet has bends or overlays to copy"));
            }
            let mut state = original.clone();
            for chart in &mut state.charts {
                let SheetEmbedding::Plane {
                    front_frame,
                    front_to_back_sign,
                } = &chart.embedding
                else {
                    return Err(format!("{name}'s sheet has a domain that is not planar"));
                };
                // The image of the material: a mirror reverses the frame's
                // handedness, so the material's side turns with it.
                let parity = match (reflected, front_to_back_sign) {
                    (false, p) => *p,
                    (true, MaterialParity::Positive) => MaterialParity::Negative,
                    (true, MaterialParity::Negative) => MaterialParity::Positive,
                };
                chart.embedding = SheetEmbedding::Plane {
                    front_frame: copy.transform.apply_frame(front_frame),
                    front_to_back_sign: parity,
                };
            }
            let mut state = renamed(state, copy.feature, ctx.uid, offset)?;
            let body = BodyUid::new(ctx.uid, next);
            next += 1;
            state.body = body;
            state.owner = owner;
            let geometry = ctx
                .kernel
                .sheet_shape(
                    &SheetShapeSpec {
                        thickness: rules.value.thickness,
                        patches: &state.charts,
                        ports: &state.joins,
                        tolerance: TOLERANCE,
                    },
                    &ctx.uid.to_string(),
                )
                .map_err(|e| super::folded::name_walls(ctx, &e.to_string()))?;
            let [solid] = <[_; 1]>::try_from(geometry.solids).map_err(|s: Vec<_>| {
                format!("the copy of {name} came apart into {} solids", s.len())
            })?;
            super::folded::finish(&mut output, state, &rules.value, solid.shape)?;
        }
    }
    Ok(Some(output))
}

/// A copied sheet state's domains and curves of `feature` renamed as the
/// pattern's (`uid`), their slots shifted by `offset`.
fn renamed(
    state: SheetState,
    feature: FeatureUid,
    uid: FeatureUid,
    offset: u32,
) -> Result<SheetState, String> {
    use serde_json::Value;
    let from = serde_json::to_value(feature).map_err(|e| e.to_string())?;
    let to = serde_json::to_value(uid).map_err(|e| e.to_string())?;
    fn walk(v: &mut Value, from: &Value, to: &Value, offset: u32) {
        match v {
            Value::Object(m) => {
                if m.get("feature") == Some(from)
                    && let Some(slot) = m.get("slot").and_then(Value::as_u64)
                {
                    m.insert("feature".into(), to.clone());
                    m.insert("slot".into(), Value::from(slot + u64::from(offset)));
                }
                m.values_mut().for_each(|c| walk(c, from, to, offset));
            }
            Value::Array(a) => a.iter_mut().for_each(|c| walk(c, from, to, offset)),
            _ => {}
        }
    }
    let mut value = serde_json::to_value(&state).map_err(|e| e.to_string())?;
    walk(&mut value, &from, &to, offset);
    serde_json::from_value(value).map_err(|e| e.to_string())
}

/// The sheet state of `body` just before `feature` set it.
fn state_before<K: Kernel>(
    ctx: &EvalContext<'_, K>,
    feature: FeatureUid,
    body: BodyUid,
) -> Option<SheetState> {
    let at = ctx.env.history.iter().position(|r| r.uid == feature)?;
    ctx.env.history[..at].iter().rev().find_map(|r| {
        let output = r.output.as_ref()?;
        if !r.status.is_ok() {
            return None;
        }
        match output.sheets.get(&body)? {
            SheetChange::Set(state) => Some(state.clone()),
            _ => None,
        }
    })
}

/// Whether the sheet keeps `feature`'s own change to `body` (to any sheet
/// body with None) as a change overlay (the state its result set has one
/// of it): an ordinary fillet or chamfer on a sheet, whose patterns and
/// mirrors are repeated on the sheet as well.
pub(crate) fn kept_as_change<K: Kernel>(
    ctx: &EvalContext<'_, K>,
    feature: FeatureUid,
    body: Option<BodyUid>,
) -> bool {
    let Some(output) = ctx
        .env
        .history
        .iter()
        .find(|r| r.uid == feature)
        .and_then(|r| r.output.as_ref())
    else {
        return false;
    };
    output.sheets.iter().any(|(uid, change)| {
        body.is_none_or(|b| b == *uid)
            && matches!(change, SheetChange::Set(state) if state.overlays.iter().any(|o| {
                o.feature == feature && matches!(o.kind, SheetOverlayKind::Change { .. })
            }))
    })
}

/// A flange's definition on the copy of its boundary.
fn moved_flange(
    d: &crate::features::sheet_folded::SheetFlangeDef<f64>,
    original: &SheetState,
    current: &SheetState,
    thickness: f64,
    transform: &Transform,
) -> Result<crate::features::sheet_folded::SheetFlangeDef<f64>, String> {
    // The original boundary on its panel's front surface, the bend's side
    // and the canonical axial direction.
    let edge = boundary(original, &d.boundary)?;
    let (a, b) = line(edge)?;
    let patch = super::folded::chart(original, d.boundary.panel)?;
    let (frame, parity) = plane(patch)?;
    let n = frame.normal().map(|v| v * parity.sign());
    let along = |frame: &crate::profile::SketchFrame, a: [f64; 2], b: [f64; 2]| {
        let p = frame.point(a);
        let q = frame.point(b);
        let d = sub(q, p);
        let l = dot(d, d).sqrt();
        d.map(|v| v / l)
    };
    let e = along(&frame, a, b).map(|v| -v * parity.sign());
    let ends = [
        transform.apply_point(frame.point(a)),
        transform.apply_point(frame.point(b)),
    ];
    let tn = transform.apply_vector(n);
    let te = transform.apply_vector(e);
    // The current panel boundary the transform carries it onto (front or
    // back surface).
    let mut found = None;
    for patch in &current.charts {
        let SheetDomainRef::Panel(panel) = patch.domain else {
            continue;
        };
        let Ok((frame, parity)) = plane(patch) else {
            continue;
        };
        let f = frame.normal().map(|v| v * parity.sign());
        let edges: Vec<&MaterialEdge> = patch
            .regions
            .iter()
            .flat_map(|r| &r.loops)
            .flat_map(|l| &l.edges)
            .collect();
        for candidate in &edges {
            let Ok((p, q)) = line(candidate) else {
                continue;
            };
            for depth in [0.0, thickness] {
                let p3 = add(frame.point(p), f.map(|v| v * depth));
                let q3 = add(frame.point(q), f.map(|v| v * depth));
                let same = (dist(p3, ends[0]) <= 1e-6 && dist(q3, ends[1]) <= 1e-6)
                    || (dist(p3, ends[1]) <= 1e-6 && dist(q3, ends[0]) <= 1e-6);
                if !same {
                    continue;
                }
                if found.is_some() {
                    return Err("the copy of its boundary is ambiguous".into());
                }
                let pieces = edges
                    .iter()
                    .filter(|e| e.piece.curve == candidate.piece.curve)
                    .count();
                let target_e = along(&frame, p, q).map(|v| -v * parity.sign());
                found = Some((
                    BoundaryRef {
                        panel,
                        curve: candidate.piece.curve.clone(),
                        piece: (pieces > 1).then_some(candidate.piece.piece),
                        range: None,
                    },
                    dot(tn, f),
                    dot(te, target_e),
                ));
            }
        }
    }
    let (reference, side, axial) =
        found.ok_or("no panel boundary of the sheet lies where the copy of its boundary does")?;
    let mut moved = d.clone();
    moved.boundary = reference;
    if side < 0.0 {
        moved.angle = -moved.angle;
    }
    if axial < 0.0 {
        if let Some(o) = &mut moved.offsets {
            std::mem::swap(&mut o.start, &mut o.end);
        }
        if let Some(SheetFlangeWidth::Middle { before, after, .. }) = &mut moved.width {
            std::mem::swap(before, after);
        }
    }
    Ok(moved)
}

/// Runs a sheet operation as if `body` were `shape` with sheet `state`.
fn with_sheet<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    body: BodyUid,
    shape: &K::Shape,
    state: &SheetState,
    run: impl FnOnce(&mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String>,
) -> Result<FeatureOutput<K::Shape>, String> {
    let mut sheets = ctx.env.sheets.clone();
    let mut working = state.clone();
    // (The working state stands for the body as its component holds it:
    // the generation check of the operation compares with that version.)
    working.shape_generation = ctx
        .env
        .owners
        .get(&body)
        .and_then(|c| ctx.env.components.get(c))
        .and_then(|b| b.get(&body))
        .map_or_else(String::new, |b| format!("{:032x}", b.version));
    sheets.insert(
        body,
        Versioned {
            version: version(&working),
            value: working,
        },
    );
    let mut bodies = ctx.env.bodies.clone();
    let shape_version = bodies.get(&body).map_or(0, |b| b.version);
    bodies.insert(
        body,
        Versioned {
            version: shape_version,
            value: shape.clone(),
        },
    );
    // (The body holds `shape` now, not a stand-in of a restored result,
    // mitcad#886.)
    let mut pending = ctx.env.pending.clone();
    pending.remove(&body);
    let env = crate::features::Env {
        sheets: &sheets,
        bodies: &bodies,
        pending: &pending,
        ..ctx.env
    };
    let mut child = EvalContext::new(ctx.kernel, ctx.uid, env);
    let result = run(&mut child);
    ctx.unpack_time += child.unpack_time;
    result
}

/// The shape and sheet state a sheet operation published for `body`.
fn published<S>(output: FeatureOutput<S>, body: BodyUid) -> Result<(S, SheetState), String> {
    if output.changes.len() != 1 {
        return Err("the copy came apart into several bodies".into());
    }
    let mut changes = output.changes;
    let Some(BodyChange::Set(uid, shape)) = changes.pop() else {
        return Err("the copy removed its body".into());
    };
    if uid != body {
        return Err("the copy changed another body".into());
    }
    match output.sheets.get(&body) {
        Some(SheetChange::Set(state)) => Ok((shape, state.clone())),
        _ => Err("the copy left no sheet".into()),
    }
}

/// The material a feature changed in `body`, moved by the copy's
/// transform and changed in `shape` the same way.
fn repeat_change<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    copy: &Copy,
    body: BodyUid,
    shape: &K::Shape,
) -> Result<K::Shape, String> {
    let name = ctx.feature_name(copy.feature);
    let before = ctx
        .body_before(copy.feature, body)
        .ok_or_else(|| format!("{name} has no body before it"))?;
    let after = ctx
        .env
        .history
        .iter()
        .find(|r| r.uid == copy.feature)
        .and_then(|r| r.shape_set(body))
        .cloned()
        .ok_or_else(|| format!("{name} has no result"))?;
    let volume = |shape: &K::Shape| -> Result<f64, String> {
        Ok(ctx
            .kernel
            .mass_properties(shape)
            .map_err(|e| e.to_string())?
            .volume)
    };
    let least = 1e-9 * volume(&before)?.abs().max(1.0);
    let mut shape = shape.clone();
    for (operation, from, less) in [
        (BooleanOp::Join, &after, &before),
        (BooleanOp::Cut, &before, &after),
    ] {
        let difference = ctx
            .kernel
            .boolean(BooleanOp::Cut, &[from], less)
            .map_err(|e| e.to_string())?;
        for piece in difference.pieces {
            if volume(&piece.shape)? <= least {
                continue;
            }
            let moved = ctx
                .kernel
                .transform_shape(&piece.shape, &copy.transform, Some(copy.instance))
                .map_err(|e| e.to_string())?;
            let result = ctx
                .kernel
                .boolean(operation, &[&shape], &moved)
                .map_err(|e| e.to_string())?;
            if !result.touched.first().copied().unwrap_or(false) {
                return Err(format!("the copy of {name} does not reach the sheet"));
            }
            let pieces: Vec<_> = result
                .pieces
                .into_iter()
                .filter(|p| !p.sources.is_empty())
                .collect();
            let [piece] = pieces.as_slice() else {
                return Err(format!(
                    "the copy of {name} leaves {} pieces of the sheet",
                    pieces.len()
                ));
            };
            shape = piece.shape.clone();
        }
    }
    Ok(shape)
}

/// Domain and curve identities a copy made, renumbered by `offset`: the
/// copies of one pattern all carry its identity, so each copy's slots are
/// kept apart.
fn renumber(
    before: &SheetState,
    after: SheetState,
    uid: FeatureUid,
    offset: u32,
) -> Result<SheetState, String> {
    use serde_json::Value;
    let feature = serde_json::to_value(uid).map_err(|e| e.to_string())?;
    let mut old = std::collections::BTreeSet::new();
    fn collect(v: &Value, feature: &Value, old: &mut std::collections::BTreeSet<String>) {
        match v {
            Value::Object(m) => {
                if m.get("feature") == Some(feature) && m.get("slot").is_some() {
                    old.insert(v.to_string());
                }
                m.values().for_each(|c| collect(c, feature, old));
            }
            Value::Array(a) => a.iter().for_each(|c| collect(c, feature, old)),
            _ => {}
        }
    }
    fn shift(
        v: &mut Value,
        feature: &Value,
        old: &std::collections::BTreeSet<String>,
        offset: u32,
    ) {
        match v {
            Value::Object(m) => {
                let ours = m.get("feature") == Some(feature)
                    && m.get("slot").is_some_and(Value::is_u64)
                    && !old.contains(&Value::Object(m.clone()).to_string());
                if ours {
                    let slot = m["slot"].as_u64().unwrap_or(0) + u64::from(offset);
                    m.insert("slot".into(), Value::from(slot));
                } else {
                    m.values_mut().for_each(|c| shift(c, feature, old, offset));
                }
            }
            Value::Array(a) => a.iter_mut().for_each(|c| shift(c, feature, old, offset)),
            _ => {}
        }
    }
    let before = serde_json::to_value(before).map_err(|e| e.to_string())?;
    collect(&before, &feature, &mut old);
    let mut value = serde_json::to_value(&after).map_err(|e| e.to_string())?;
    shift(&mut value, &feature, &old, offset);
    let mut state: SheetState = serde_json::from_value(value).map_err(|e| e.to_string())?;
    // Curve generations hash the curves only; identities moved with them.
    state.shape_generation = after.shape_generation;
    Ok(state)
}
