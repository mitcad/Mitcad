// SPDX-License-Identifier: MIT
//! Native sheet edits operate on material charts, then reconstruct once.

mod bends;
mod corners;
pub(crate) use bends::{bend, flange, fold};
pub(crate) use corners::{corner_chamfer, corner_round};
// Unfold, Refold, graph development and the minimal Cut (mitcad#208).
pub(crate) mod cut;
pub(crate) mod develop;
pub(crate) use cut::cut;
pub(crate) use develop::{refold, unfold};
// Corner relief at bend junctions (mitcad#209).
mod relief;
pub(crate) use relief::corner_relief;
// A face joined coplanar to a sheet's panel (mitcad#242).
mod join;
pub(crate) use join::join_face;
// Contour Flange, Convert to Sheet Metal and Perforate (mitcad#461).
mod contour;
pub(crate) use contour::contour;
mod perforate;
pub(crate) use perforate::perforate;
mod convert;
pub(crate) use convert::convert;

use super::*;
use crate::features::sheet::SheetRulesDef;
use crate::features::{BodyChange, EvalContext, FeatureOutput};
use crate::ids::{BodyUid, FeatureUid};
use crate::kernel::Kernel;
use crate::profile::{SegmentGeometry, SketchFrame};
use std::collections::BTreeSet;

pub(super) fn read<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    body: BodyUid,
) -> Result<(SheetState, SheetRulesDef<f64>), String> {
    ctx.body(body)?;
    let state = ctx.require_sheet(body)?;
    let rules = ctx.sheet_rules(state.rules)?;
    if state.algorithm != ALGORITHM_VERSION || state.charts.is_empty() {
        return Err("sheet material charts require recomputation".into());
    }
    if state.rules_version != format!("{:032x}", rules.version) {
        return Err("sheet material rules do not match current rules".into());
    }
    Ok((state, rules.value))
}

/// A refusal of the kernel's sheet construction with the walls it names
/// (`panel F6 slot 0`, `bend F4 slot 0`: two walls that meet, mitcad#1409)
/// named by their features as the timeline shows them: `SheetFlange3`, a
/// further wall `SheetFlange3 (wall 2)`, a bend `SheetFlange1 (bend)`.
pub(crate) fn name_walls<K: Kernel>(ctx: &EvalContext<'_, K>, message: &str) -> String {
    let digits = |text: &str| text.bytes().take_while(u8::is_ascii_digit).count();
    let mut named = String::new();
    let mut rest = message;
    loop {
        let found = ["panel F", "bend F"]
            .iter()
            .filter_map(|key| rest.find(key).map(|at| (at, *key)))
            .min();
        let Some((at, key)) = found else {
            named.push_str(rest);
            return named;
        };
        named.push_str(&rest[..at]);
        let after = &rest[at + key.len()..];
        let feature_digits = digits(after);
        let slot_text = after[feature_digits..].strip_prefix(" slot ");
        let parsed = slot_text.and_then(|slot_text| {
            let slot_digits = digits(slot_text);
            let feature = after[..feature_digits].parse::<u64>().ok()?;
            let slot = slot_text[..slot_digits].parse::<u64>().ok()?;
            Some((feature, slot, &slot_text[slot_digits..]))
        });
        let Some((feature, slot, tail)) = parsed else {
            named.push_str(&rest[at..at + key.len()]);
            rest = after;
            continue;
        };
        let name = ctx.feature_name(FeatureUid(feature));
        let panel = key.starts_with("panel");
        named.push_str(&match (panel, slot) {
            (true, 0) => name,
            (true, k) => format!("{name} (wall {})", k + 1),
            (false, 0) => format!("{name} (bend)"),
            (false, k) => format!("{name} (bend {})", k + 1),
        });
        rest = tail;
    }
}

pub(super) fn publish<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    state: SheetState,
    rules: &SheetRulesDef<f64>,
    removed: Option<BodyUid>,
) -> Result<FeatureOutput<K::Shape>, String> {
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
        .map_err(|e| name_walls(ctx, &e.to_string()))?;
    // A folded feature has one connected sheet. Never silently keep only the
    // first piece when relief or an oversized corner disconnects material.
    if geometry.solids.len() != 1 {
        return Err(format!(
            "sheet operation produced {} disconnected solids",
            geometry.solids.len()
        ));
    }
    let mut solids = geometry.solids.into_iter();
    let solid = solids.next().expect("one solid");
    let received: BTreeSet<_> = solid.membership.iter().copied().collect();
    if received != memberships(&state) || received.len() != solid.membership.len() {
        return Err("sheet kernel returned incomplete material membership".into());
    }
    let mut output = FeatureOutput::default();
    // Straight cuts the domains cannot hold (mitcad#242).
    let shape = cut::with_overlays(ctx, &state, solid.shape)?;
    finish(&mut output, state, rules, shape)?;
    if let Some(body) = removed {
        output.changes.push(BodyChange::Remove(body));
        output.sheets.insert(body, SheetChange::Remove);
    }
    for diagnostic in geometry.diagnostics {
        ctx.warn(diagnostic);
    }
    Ok(output)
}

fn memberships(state: &SheetState) -> BTreeSet<RegionMembership> {
    state
        .charts
        .iter()
        .flat_map(|p| {
            (0..p.regions.len()).map(|region| RegionMembership {
                domain: p.domain,
                region,
            })
        })
        .collect()
}

pub(super) fn finish<S>(
    output: &mut FeatureOutput<S>,
    mut state: SheetState,
    rules: &SheetRulesDef<f64>,
    shape: S,
) -> Result<(), String> {
    state.panels = legacy_panels(&state.charts, rules)?;
    state.algorithm = ALGORITHM_VERSION;
    state.schema = 2;
    state.shape_generation.clear();
    output.changes.push(BodyChange::Set(state.body, shape));
    output.sheets.insert(state.body, SheetChange::Set(state));
    Ok(())
}

/// Publishes a sheet whose material may have come apart (mitcad#209): every
/// solid the kernel builds becomes a body with the domains of its regions.
/// The first piece in geometric order (centre of mass, then the lower corner
/// of the bounding box) keeps the body's identifier; the others become bodies
/// of the publishing feature, all owned by the input body's component. With
/// `single`, a disconnected result is refused naming that operation.
pub(super) fn publish_pieces<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    state: SheetState,
    rules: &SheetRulesDef<f64>,
    single: Option<&str>,
) -> Result<FeatureOutput<K::Shape>, String> {
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
        .map_err(|e| name_walls(ctx, &e.to_string()))?;
    if geometry.solids.is_empty() {
        return Err("sheet operation produced no material".into());
    }
    if geometry.solids.len() > 1
        && let Some(what) = single
    {
        return Err(format!(
            "{what} disconnects the sheet into {} pieces",
            geometry.solids.len()
        ));
    }
    let mut all = BTreeSet::new();
    let mut keyed = Vec::new();
    for solid in geometry.solids {
        for m in &solid.membership {
            if !all.insert(*m) {
                return Err("sheet material region belongs to several solids".into());
            }
        }
        // The geometric key is measured below, only when there are pieces.
        keyed.push(([0_i64; 6], solid));
    }
    if all != memberships(&state) {
        return Err("sheet kernel returned incomplete material membership".into());
    }
    if keyed.len() > 1 {
        for (key, solid) in &mut keyed {
            let mass = ctx
                .kernel
                .mass_properties(&solid.shape)
                .map_err(|e| e.to_string())?;
            let bounds = ctx
                .kernel
                .bounding_box(&solid.shape)
                .map_err(|e| e.to_string())?
                .ok_or("a sheet piece has no extent")?;
            let values = [
                mass.center[0],
                mass.center[1],
                mass.center[2],
                bounds.min[0],
                bounds.min[1],
                bounds.min[2],
            ];
            *key = values.map(|v| (v * 1e6).round() as i64);
        }
        keyed.sort_by_key(|(key, _)| *key);
    }
    let mut output = FeatureOutput::default();
    let whole = keyed.len() == 1;
    for (index, (_, solid)) in keyed.into_iter().enumerate() {
        let body = if index == 0 {
            state.body
        } else {
            BodyUid::new(ctx.uid, index as u32 - 1)
        };
        let mut piece = if whole {
            state.clone()
        } else {
            partition(&state, &solid.membership.iter().copied().collect())?
        };
        piece.body = body;
        if index > 0 {
            output.owners.insert(body, state.owner);
        }
        let shape = cut::with_overlays(ctx, &piece, solid.shape)?;
        finish(&mut output, piece, rules, shape)?;
    }
    for diagnostic in geometry.diagnostics {
        ctx.warn(diagnostic);
    }
    Ok(output)
}

/// The part of a sheet whose material regions are `members`: their charts
/// (regions renumbered in order), the ports between them, the records of
/// its bends and their open state, and the lineage of its domains. A port
/// joining two pieces is an inconsistent result.
pub(crate) fn partition(
    state: &SheetState,
    members: &BTreeSet<RegionMembership>,
) -> Result<SheetState, String> {
    let region_of = |side: &ShapePortSide| -> Option<usize> {
        let patch = state
            .charts
            .iter()
            .find(|p| p.domain == side.boundary.domain)?;
        patch.regions.iter().position(|r| {
            r.loops.iter().flat_map(|l| &l.edges).any(|e| {
                let (lo, hi) = (
                    e.curve.range.first.min(e.curve.range.last),
                    e.curve.range.first.max(e.curve.range.last),
                );
                e.piece == side.boundary.piece
                    && side.span.first.min(side.span.last) >= lo - 1e-9
                    && side.span.first.max(side.span.last) <= hi + 1e-9
            })
        })
    };
    let contains = |side: &ShapePortSide| -> Result<bool, String> {
        let region = region_of(side).ok_or("a sheet port has no material boundary")?;
        Ok(members.contains(&RegionMembership {
            domain: side.boundary.domain,
            region,
        }))
    };
    let mut piece = state.clone();
    piece.charts = state
        .charts
        .iter()
        .filter_map(|p| {
            let regions: Vec<_> = p
                .regions
                .iter()
                .enumerate()
                .filter(|(region, _)| {
                    members.contains(&RegionMembership {
                        domain: p.domain,
                        region: *region,
                    })
                })
                .map(|(_, r)| r.clone())
                .collect();
            (!regions.is_empty()).then(|| SheetPatch {
                regions,
                ..p.clone()
            })
        })
        .collect();
    piece.joins.clear();
    for port in &state.joins {
        match (contains(&port.first)?, contains(&port.second)?) {
            (true, true) => piece.joins.push(port.clone()),
            (false, false) => {}
            _ => return Err("a sheet port joins two separate solids".into()),
        }
    }
    let domains: BTreeSet<SheetDomainRef> = piece.charts.iter().map(|p| p.domain).collect();
    piece
        .bends
        .retain(|b| domains.contains(&SheetDomainRef::Bend(b.id)));
    piece
        .opened
        .retain(|o| domains.contains(&SheetDomainRef::Bend(o.bend)));
    piece
        .lineage
        .retain(|l| domains.contains(&l.boundary.domain));
    Ok(piece)
}

pub(super) fn legacy_panels(
    charts: &[SheetPatch],
    rules: &SheetRulesDef<f64>,
) -> Result<Vec<SheetPlanarPatch>, String> {
    let mut panels = Vec::new();
    for p in charts {
        let SheetDomainRef::Panel(id) = p.domain else {
            continue;
        };
        let SheetEmbedding::Plane {
            front_frame,
            front_to_back_sign,
        } = &p.embedding
        else {
            continue;
        };
        let mut frame = *front_frame;
        if rules.reference_side == SheetReferenceSide::Back {
            let n = frame.normal();
            for (i, n) in n.iter().enumerate() {
                frame.origin[i] += rules.thickness * front_to_back_sign.sign() * n;
            }
        }
        // Charts are authoritative and may contain several regions. The old
        // field remains a projection; richer callers consume `charts`.
        for region in &p.regions {
            let mut loops = Vec::new();
            for l in &region.loops {
                let segments = l
                    .edges
                    .iter()
                    .map(|e| {
                        Ok(SheetSegment {
                            id: e.piece.curve.clone(),
                            geometry: e.curve.geometry_piece()?,
                        })
                    })
                    .collect::<Result<_, String>>()?;
                loops.push(SheetLoop { segments });
            }
            panels.push(SheetPlanarPatch {
                id,
                frame,
                region: SheetRegion { loops },
            });
        }
    }
    Ok(panels)
}

pub(super) fn chart(state: &SheetState, panel: PanelId) -> Result<&SheetPatch, String> {
    state
        .charts
        .iter()
        .find(|p| p.domain == SheetDomainRef::Panel(panel))
        .ok_or_else(|| format!("sheet panel {panel} is unavailable"))
}

pub(super) fn plane(patch: &SheetPatch) -> Result<(SketchFrame, MaterialParity), String> {
    match &patch.embedding {
        SheetEmbedding::Plane {
            front_frame,
            front_to_back_sign,
        } => Ok((*front_frame, *front_to_back_sign)),
        _ => Err("selected sheet domain is not planar".into()),
    }
}

pub(super) fn boundary<'a>(
    state: &'a SheetState,
    reference: &BoundaryRef,
) -> Result<&'a MaterialEdge, String> {
    let patch = chart(state, reference.panel)?;
    let matches = |piece: &CurvePieceRef| {
        piece.curve == reference.curve && reference.piece.is_none_or(|p| p == piece.piece)
    };
    let candidates: Vec<(&MaterialEdge, bool)> = patch
        .regions
        .iter()
        .flat_map(|r| &r.loops)
        .flat_map(|l| &l.edges)
        .filter(|e| {
            reference
                .range
                .is_none_or(|span| span == e.curve.range || span == e.curve.range.reversed())
        })
        .filter_map(|e| {
            let direct = matches(&e.piece);
            let alias = state
                .lineage
                .iter()
                .filter(|l| l.applies_to(patch.domain, e))
                .any(|l| l.aliases.iter().any(matches));
            (direct || alias).then_some((e, direct))
        })
        .collect();
    // The piece itself before a piece that inherited its alias when it was
    // split off (mitcad#280).
    let direct: Vec<&MaterialEdge> = candidates
        .iter()
        .filter(|(_, d)| *d)
        .map(|(e, _)| *e)
        .collect();
    let mut edges: Box<dyn Iterator<Item = &MaterialEdge>> = if direct.len() == 1 {
        Box::new(direct.into_iter())
    } else {
        Box::new(candidates.into_iter().map(|(e, _)| e))
    };
    let found = edges
        .next()
        .ok_or("sheet boundary reference is unavailable")?;
    if edges.next().is_some() {
        return Err(
            "sheet boundary reference is ambiguous; select an explicit piece and parameter range"
                .into(),
        );
    }
    Ok(found)
}

pub(super) fn line(edge: &MaterialEdge) -> Result<([f64; 2], [f64; 2]), String> {
    if !matches!(edge.curve.geometry, SegmentGeometry::Line { .. }) {
        return Err("sheet operation requires a straight material boundary".into());
    }
    let (a, b) = edge.curve.ends()?;
    if crate::sketch::geometry::dist(a, b) <= TOLERANCE {
        return Err("sheet material boundary has zero length".into());
    }
    Ok((a, b))
}

pub(super) fn generated(feature: FeatureUid, slot: u32) -> CurvePieceRef {
    CurvePieceRef {
        curve: SheetCurveId::Generated { feature, slot },
        piece: 0,
    }
}

pub(super) fn region(
    feature: FeatureUid,
    slot: u32,
    curves: Vec<SegmentGeometry>,
) -> Result<MaterialRegion, String> {
    MaterialRegion::from_legacy(&SheetRegion {
        loops: vec![SheetLoop {
            segments: curves
                .into_iter()
                .enumerate()
                .map(|(i, geometry)| SheetSegment {
                    id: SheetCurveId::Generated {
                        feature,
                        slot: slot + i as u32,
                    },
                    geometry,
                })
                .collect(),
        }],
    })
}

pub(super) fn polygon(
    feature: FeatureUid,
    slot: u32,
    points: &[[f64; 2]],
) -> Result<MaterialRegion, String> {
    region(
        feature,
        slot,
        points
            .iter()
            .zip(points.iter().cycle().skip(1))
            .take(points.len())
            .map(|(a, b)| SegmentGeometry::Line { start: *a, end: *b })
            .collect(),
    )
}

pub(super) fn rectangle(
    feature: FeatureUid,
    slot: u32,
    min: [f64; 2],
    max: [f64; 2],
) -> Result<MaterialRegion, String> {
    polygon(
        feature,
        slot,
        &[min, [max[0], min[1]], max, [min[0], max[1]]],
    )
}

pub(super) fn trim<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    state: &mut SheetState,
    domains: &[SheetDomainRef],
    operation: SheetTrimOperation,
    tools: &[MaterialRegion],
    maps: &[Isometry2],
) -> Result<(), String> {
    trim_with_lineage(ctx, state, domains, operation, tools, maps, None)
}

pub(super) fn trim_with_lineage<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    state: &mut SheetState,
    domains: &[SheetDomainRef],
    operation: SheetTrimOperation,
    tools: &[MaterialRegion],
    maps: &[Isometry2],
    tool_domain: Option<SheetDomainRef>,
) -> Result<(), String> {
    let inputs: Vec<_> = domains
        .iter()
        .zip(maps)
        .map(|(domain, to_operation)| {
            let p = state
                .charts
                .iter()
                .find(|p| p.domain == *domain)
                .ok_or("trim material domain is unavailable")?;
            Ok(TrimDomain {
                domain: *domain,
                regions: p.regions.clone(),
                to_operation: *to_operation,
            })
        })
        .collect::<Result<_, String>>()?;
    let result = ctx
        .kernel
        .sheet_trim_domain(&SheetTrimSpec {
            operation,
            domains: &inputs,
            tools,
            seams: &[],
            tolerance: TOLERANCE,
        })
        .map_err(|e| e.to_string())?;
    if result.domains.len() != inputs.len() {
        return Err("sheet trim omitted a requested material domain".into());
    }
    let before = state.clone();
    let mut seen = BTreeSet::new();
    let input_lineage = state.lineage.clone();
    for output in result.domains {
        if !domains.contains(&output.domain) || !seen.insert(output.domain) {
            return Err("sheet trim returned ambiguous domain membership".into());
        }
        let mut regions = Vec::new();
        for r in output.regions {
            let mut loops = Vec::new();
            for (loop_index, l) in r.loops.into_iter().enumerate() {
                let mut edges = Vec::new();
                let mut records = Vec::new();
                for edge in l.edges {
                    if edge.lineage.is_empty() {
                        return Err("sheet trim returned a boundary without provenance".into());
                    }
                    let mut aliases: Vec<_> = edge
                        .lineage
                        .iter()
                        .map(|s| match &s.source {
                            TrimSource::Domain { boundary } => boundary.piece.clone(),
                            TrimSource::Tool { piece } => piece.clone(),
                        })
                        .collect();
                    aliases.sort_by_key(|p| serde_json::to_string(p).expect("curve identity"));
                    aliases.dedup();
                    let piece = aliases[0].clone();
                    for source in &edge.lineage {
                        let (domain, regions, piece) = match &source.source {
                            TrimSource::Domain { boundary } => {
                                let input =
                                    inputs.iter().find(|d| d.domain == boundary.domain).ok_or(
                                        "sheet trim lineage refers to an unknown input domain",
                                    )?;
                                (boundary.domain, input.regions.as_slice(), &boundary.piece)
                            }
                            TrimSource::Tool { piece } => {
                                let Some(domain) = tool_domain else {
                                    continue;
                                };
                                (domain, tools, piece)
                            }
                        };
                        aliases.extend(inherited_aliases(
                            &input_lineage,
                            domain,
                            regions,
                            piece,
                            source.span,
                        )?);
                    }
                    canonical_aliases(&mut aliases);
                    // One canonical representative is stored on the edge; all
                    // proven aliases remain resolvable through this lineage.
                    records.push(CurveLineage {
                        boundary: BoundaryPieceRef {
                            domain: output.domain,
                            piece: piece.clone(),
                        },
                        curve_generation: String::new(),
                        aliases,
                        sources: edge.lineage,
                    });
                    edges.push(MaterialEdge {
                        piece,
                        curve: edge.curve,
                    });
                }
                // A reflected operation frame returns loops traversed the other
                // way. Charts keep exteriors counter-clockwise and holes
                // clockwise: boundary normals and corners depend on it.
                let mut settled = MaterialLoop { edges };
                if (develop::loop_area(&settled)? > 0.0) != (loop_index == 0) {
                    settled.edges.reverse();
                    records.reverse();
                    for e in &mut settled.edges {
                        e.curve.range = e.curve.range.reversed();
                    }
                }
                for (record, edge) in records.iter_mut().zip(&settled.edges) {
                    record.curve_generation = CurveLineage::generation(&edge.curve);
                }
                state.lineage.extend(records);
                loops.push(settled);
            }
            regions.push(MaterialRegion { loops });
        }
        state
            .charts
            .iter_mut()
            .find(|p| p.domain == output.domain)
            .expect("checked")
            .regions = regions;
    }
    rebind_ports(&before, state, domains);
    for message in result.diagnostics {
        ctx.warn(message);
    }
    Ok(())
}

/// The chart points at the ends of a port side, on the boundary piece that
/// contains its span.
pub(super) fn port_ends(state: &SheetState, side: &ShapePortSide) -> Option<([f64; 2], [f64; 2])> {
    let edge = state
        .charts
        .iter()
        .filter(|p| p.domain == side.boundary.domain)
        .flat_map(|p| &p.regions)
        .flat_map(|r| &r.loops)
        .flat_map(|l| &l.edges)
        .find(|e| {
            let (lo, hi) = (
                e.curve.range.first.min(e.curve.range.last),
                e.curve.range.first.max(e.curve.range.last),
            );
            e.piece == side.boundary.piece
                && side.span.first.min(side.span.last) >= lo - 1e-9
                && side.span.first.max(side.span.last) <= hi + 1e-9
        })?;
    Some((
        edge.curve.point(side.span.first).ok()?,
        edge.curve.point(side.span.last).ok()?,
    ))
}

/// A trim returns lines with their own parameters, possibly traversed the
/// other way: registered seams of the trimmed domains whose boundary
/// survives whole but no longer has the same parameters are bound to it
/// again (a seam between two coplanar panels next to a new flange,
/// mitcad#125). Seams the trim shortened or removed stay as they were for
/// the reconstruction to refuse.
fn rebind_ports(before: &SheetState, state: &mut SheetState, domains: &[SheetDomainRef]) {
    let near = |a: [f64; 2], b: [f64; 2]| crate::sketch::geometry::dist(a, b) <= 1e-9;
    let joins: Vec<ShapePort> = state
        .joins
        .iter()
        .map(|port| {
            let mut port = port.clone();
            for side in [&mut port.first, &mut port.second] {
                if !domains.contains(&side.boundary.domain) {
                    continue;
                }
                let unchanged = match (port_ends(before, side), port_ends(state, side)) {
                    (Some((a0, a1)), Some((b0, b1))) => near(a0, b0) && near(a1, b1),
                    _ => false,
                };
                if !unchanged && let Some(bound) = cut::rebind(before, state, side) {
                    *side = bound;
                }
            }
            port
        })
        .collect();
    state.joins = joins;
}

fn canonical_aliases(aliases: &mut Vec<CurvePieceRef>) {
    aliases.sort_by_key(|p| serde_json::to_string(p).expect("curve identity"));
    aliases.dedup();
}

fn inherited_aliases(
    lineage: &[CurveLineage],
    domain: SheetDomainRef,
    regions: &[MaterialRegion],
    piece: &CurvePieceRef,
    span: ParamSpan,
) -> Result<Vec<CurvePieceRef>, String> {
    let mut resolved: Option<Vec<CurvePieceRef>> = None;
    for edge in regions
        .iter()
        .flat_map(|r| &r.loops)
        .flat_map(|l| &l.edges)
        .filter(|e| &e.piece == piece)
    {
        let range = edge.curve.range;
        if span.first.min(span.last) < range.first.min(range.last) - TOLERANCE
            || span.first.max(span.last) > range.first.max(range.last) + TOLERANCE
        {
            continue;
        }
        let mut aliases = vec![edge.piece.clone()];
        for record in lineage.iter().filter(|l| l.applies_to(domain, edge)) {
            aliases.extend(record.aliases.iter().cloned());
        }
        canonical_aliases(&mut aliases);
        if resolved.as_ref().is_some_and(|old| *old != aliases) {
            return Err("sheet trim source fragment has ambiguous semantic aliases".into());
        }
        resolved = Some(aliases);
    }
    resolved.ok_or_else(|| "sheet trim source interval does not belong to an input boundary".into())
}

pub(super) fn copy_lineage(
    lineage: &mut Vec<CurveLineage>,
    source: SheetDomainRef,
    target: SheetDomainRef,
    regions: &[MaterialRegion],
) {
    let records: Vec<_> = lineage
        .iter()
        .filter(|record| {
            regions
                .iter()
                .flat_map(|r| &r.loops)
                .flat_map(|l| &l.edges)
                .any(|edge| record.applies_to(source, edge))
        })
        .map(|record| {
            let mut copy = record.clone();
            copy.boundary.domain = target;
            copy
        })
        .collect();
    lineage.extend(records);
}

pub(super) fn remap_lineage(
    lineage: &mut [CurveLineage],
    domain: SheetDomainRef,
    regions: &[MaterialRegion],
    map: Isometry2,
) {
    let mut generations = Vec::new();
    for edge in regions.iter().flat_map(|r| &r.loops).flat_map(|l| &l.edges) {
        let mapped = mapped_regions(
            &[MaterialRegion {
                loops: vec![MaterialLoop {
                    edges: vec![edge.clone()],
                }],
            }],
            map,
        );
        let generation = CurveLineage::generation(&mapped[0].loops[0].edges[0].curve);
        for (index, _) in lineage
            .iter()
            .enumerate()
            .filter(|(_, record)| record.applies_to(domain, edge))
        {
            generations.push((index, generation.clone()));
        }
    }
    // Apply only after matching the original generations; a mapped curve can
    // have the same coordinates as another, unrelated input fragment.
    for (index, generation) in generations {
        lineage[index].curve_generation = generation;
    }
}

pub(super) fn bounds(patch: &SheetPatch) -> Result<([f64; 2], [f64; 2]), String> {
    let profile = crate::profile::ProfileRegion {
        key: "r{c1}".parse().expect("region"),
        loops: patch
            .regions
            .iter()
            .flat_map(|r| &r.loops)
            .map(|l| crate::profile::ProfileLoop {
                segments: l
                    .edges
                    .iter()
                    .map(|e| crate::profile::ProfileSegment {
                        key: "c1".parse().expect("segment"),
                        geometry: e.curve.geometry.clone(),
                    })
                    .collect(),
            })
            .collect(),
    };
    let (min, max) = profile.bounds();
    if min.iter().chain(&max).any(|x| !x.is_finite()) {
        return Err("empty sheet material domain".into());
    }
    Ok((min, max))
}

pub(super) fn mapped_regions(regions: &[MaterialRegion], map: Isometry2) -> Vec<MaterialRegion> {
    let sign = map.x_axis[0] * map.y_axis[1] - map.x_axis[1] * map.y_axis[0];
    let rotation = map.x_axis[1].atan2(map.x_axis[0]);
    let vector = |p: [f64; 2]| {
        [
            p[0] * map.x_axis[0] + p[1] * map.y_axis[0],
            p[0] * map.x_axis[1] + p[1] * map.y_axis[1],
        ]
    };
    regions
        .iter()
        .map(|r| MaterialRegion {
            loops: r
                .loops
                .iter()
                .map(|l| {
                    let mut edges: Vec<_> = l
                        .edges
                        .iter()
                        .map(|e| {
                            let mut range = e.curve.range;
                            let geometry = match &e.curve.geometry {
                                SegmentGeometry::Line { start, end } => SegmentGeometry::Line {
                                    start: map.point(*start),
                                    end: map.point(*end),
                                },
                                SegmentGeometry::Circle { center, radius, .. }
                                | SegmentGeometry::Arc { center, radius, .. } => {
                                    range = ParamSpan {
                                        first: rotation + sign * range.first,
                                        last: rotation + sign * range.last,
                                    };
                                    SegmentGeometry::Circle {
                                        center: map.point(*center),
                                        radius: *radius,
                                        start: 0.0,
                                    }
                                }
                                SegmentGeometry::Ellipse {
                                    center,
                                    major_radius,
                                    minor_radius,
                                    rotation: old,
                                }
                                | SegmentGeometry::EllipseArc {
                                    center,
                                    major_radius,
                                    minor_radius,
                                    rotation: old,
                                    ..
                                } => {
                                    let axis = vector([old.cos(), old.sin()]);
                                    range = ParamSpan {
                                        first: sign * range.first,
                                        last: sign * range.last,
                                    };
                                    SegmentGeometry::Ellipse {
                                        center: map.point(*center),
                                        major_radius: *major_radius,
                                        minor_radius: *minor_radius,
                                        rotation: axis[1].atan2(axis[0]),
                                    }
                                }
                                SegmentGeometry::BSpline {
                                    degree,
                                    poles,
                                    weights,
                                    knots,
                                    multiplicities,
                                    periodic,
                                } => SegmentGeometry::BSpline {
                                    degree: *degree,
                                    poles: poles.iter().map(|p| map.point(*p)).collect(),
                                    weights: weights.clone(),
                                    knots: knots.clone(),
                                    multiplicities: multiplicities.clone(),
                                    periodic: *periodic,
                                },
                            };
                            MaterialEdge {
                                piece: e.piece.clone(),
                                curve: BoundedCurve2 { geometry, range },
                            }
                        })
                        .collect();
                    if sign < 0.0 {
                        edges.reverse();
                        for e in &mut edges {
                            e.curve.range = e.curve.range.reversed();
                        }
                    }
                    MaterialLoop { edges }
                })
                .collect(),
        })
        .collect()
}
