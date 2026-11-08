// SPDX-License-Identifier: MIT
//! Thickness-normal sheet Cut across bends (mitcad#208, production form
//! mitcad#209).
//!
//! The named bends are developed while the fixed panel stays in place; every
//! chart in that panel's plane is trimmed by the tool regions in one
//! operation whose seams are the registered ports, so a contour crossing a
//! tangent line splits both sides of the seam at the same axial position.
//! Charts keep their coordinates, so restoring the input configuration only
//! restores the embeddings: the cut is folded with the material. A domain the
//! tools cover completely is removed with its ports and bend record; material
//! the cut disconnects becomes several bodies ([`super::publish_pieces`]).
//! Corner reliefs ([`super::relief`]) are the same operation with generated
//! tool contours.

use mitcad_vec3::{add, sub};

use super::develop::{bend_record, domain_label, loop_area, planar_map, reconfigure};
use super::*;
use crate::features::sheet_cut::SheetCutDef;
use mitcad_vec3::dot;
use std::collections::BTreeMap;

/// A trimmed edge before its identity is settled.
struct Pending {
    token: u32,
    edge: MaterialEdge,
    record: CurveLineage,
}

/// What [`settle`] installed.
pub(crate) struct Settled {
    /// The piece of each result edge token, for the trim's seam ports.
    pub tokens: BTreeMap<(SheetDomainRef, u32), CurvePieceRef>,
    /// Domains the tools covered completely; their charts are removed.
    pub removed: BTreeSet<SheetDomainRef>,
    /// Whether any boundary of the result comes from a tool.
    pub used_tools: bool,
}

/// Replaces the regions of every trimmed domain, keeps each fragment's proven
/// aliases, numbers fragments of one curve so every boundary piece of a chart
/// is unique, and keeps exteriors counter-clockwise. A domain without
/// material left is removed from the charts and reported.
pub(crate) fn settle(
    state: &mut SheetState,
    inputs: &[TrimDomain],
    result: &SheetTrimResult,
) -> Result<Settled, String> {
    if result.domains.len() != inputs.len() {
        return Err("sheet trim omitted a requested material domain".into());
    }
    let input_lineage = state.lineage.clone();
    let mut settled = Settled {
        tokens: BTreeMap::new(),
        removed: BTreeSet::new(),
        used_tools: false,
    };
    for output in &result.domains {
        let input = inputs
            .iter()
            .find(|d| d.domain == output.domain)
            .ok_or("sheet trim returned ambiguous domain membership")?;
        if output.regions.is_empty() {
            settled.removed.insert(output.domain);
            continue;
        }
        let mut regions: Vec<Vec<Vec<Pending>>> = Vec::new();
        for r in &output.regions {
            let mut loops = Vec::new();
            for l in &r.loops {
                let mut edges = Vec::new();
                for edge in &l.edges {
                    if edge.lineage.is_empty() {
                        return Err("sheet trim returned a boundary without provenance".into());
                    }
                    let mut aliases: Vec<_> = edge
                        .lineage
                        .iter()
                        .map(|s| match &s.source {
                            TrimSource::Domain { boundary } => boundary.piece.clone(),
                            TrimSource::Tool { piece } => {
                                settled.used_tools = true;
                                piece.clone()
                            }
                        })
                        .collect();
                    canonical_aliases(&mut aliases);
                    let piece = aliases[0].clone();
                    for source in &edge.lineage {
                        if let TrimSource::Domain { boundary } = &source.source {
                            aliases.extend(inherited_aliases(
                                &input_lineage,
                                boundary.domain,
                                &input.regions,
                                &boundary.piece,
                                source.span,
                            )?);
                        }
                    }
                    canonical_aliases(&mut aliases);
                    edges.push(Pending {
                        token: edge.token,
                        edge: MaterialEdge {
                            piece: piece.clone(),
                            curve: edge.curve.clone(),
                        },
                        record: CurveLineage {
                            boundary: BoundaryPieceRef {
                                domain: output.domain,
                                piece,
                            },
                            curve_generation: String::new(),
                            aliases,
                            sources: edge.lineage.clone(),
                        },
                    });
                }
                loops.push(edges);
            }
            regions.push(loops);
        }
        // Fragments of one curve get distinct piece numbers, ordered by their
        // midpoints: a reference to the curve alone is then ambiguous, as it
        // must be. (Lines come back with their own normalized parameters.)
        #[allow(clippy::type_complexity)]
        let mut by_curve: BTreeMap<String, Vec<(f64, f64, usize, usize, usize)>> = BTreeMap::new();
        for (r, loops) in regions.iter().enumerate() {
            for (l, edges) in loops.iter().enumerate() {
                for (e, p) in edges.iter().enumerate() {
                    let range = p.edge.curve.range;
                    let mid = p.edge.curve.point((range.first + range.last) / 2.0)?;
                    by_curve
                        .entry(serde_json::to_string(&p.edge.piece.curve).expect("curve id"))
                        .or_default()
                        .push((mid[0], mid[1], r, l, e));
                }
            }
        }
        for fragments in by_curve.values_mut() {
            let pieces: BTreeSet<u32> = fragments
                .iter()
                .map(|(_, _, r, l, e)| regions[*r][*l][*e].edge.piece.piece)
                .collect();
            if pieces.len() == fragments.len() {
                continue;
            }
            fragments.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
            for (k, (_, _, r, l, e)) in fragments.iter().enumerate() {
                let p = &mut regions[*r][*l][*e];
                p.edge.piece.piece = k as u32;
                p.record.boundary.piece.piece = k as u32;
                if !p.record.aliases.contains(&p.edge.piece) {
                    p.record.aliases.push(p.edge.piece.clone());
                    canonical_aliases(&mut p.record.aliases);
                }
            }
        }
        let mut settled_regions = Vec::new();
        for loops in regions {
            let mut settled_loops = Vec::new();
            for (index, mut edges) in loops.into_iter().enumerate() {
                let mut material = MaterialLoop {
                    edges: edges.iter().map(|p| p.edge.clone()).collect(),
                };
                if (loop_area(&material)? > 0.0) != (index == 0) {
                    material.edges.reverse();
                    edges.reverse();
                    for e in &mut material.edges {
                        e.curve.range = e.curve.range.reversed();
                    }
                }
                for (p, edge) in edges.into_iter().zip(&material.edges) {
                    let mut record = p.record;
                    record.curve_generation = CurveLineage::generation(&edge.curve);
                    state.lineage.push(record);
                    settled
                        .tokens
                        .insert((output.domain, p.token), edge.piece.clone());
                }
                settled_loops.push(material);
            }
            settled_regions.push(MaterialRegion {
                loops: settled_loops,
            });
        }
        state
            .charts
            .iter_mut()
            .find(|p| p.domain == output.domain)
            .ok_or("sheet trim returned an unknown material domain")?
            .regions = settled_regions;
    }
    let removed = &settled.removed;
    state.charts.retain(|p| !removed.contains(&p.domain));
    state
        .lineage
        .retain(|l| !removed.contains(&l.boundary.domain));
    Ok(settled)
}

fn port_side(
    tokens: &BTreeMap<(SheetDomainRef, u32), CurvePieceRef>,
    axial: ParamSpan,
    side: &TrimEndpointSpan,
) -> Result<ShapePortSide, String> {
    let piece = tokens
        .get(&(side.domain, side.edge_token))
        .ok_or("a trimmed seam refers to an unknown boundary")?
        .clone();
    let scale = (axial.last - axial.first) / (side.range.last - side.range.first);
    if !scale.is_finite() || scale == 0.0 {
        return Err("a trimmed seam has a degenerate parameter map".into());
    }
    Ok(ShapePortSide {
        boundary: BoundaryPieceRef {
            domain: side.domain,
            piece,
        },
        span: side.range,
        axial: AxialMap {
            offset: axial.first - scale * side.range.first,
            scale,
        },
    })
}

/// A seam outside the cut must survive whole. The trim returns lines with
/// their own normalized parameters, so the seam's ends are found again on
/// the settled boundary of the same piece and its span and axial map are
/// rebuilt for them; a boundary the cut shortened or split is refused.
pub(super) fn rebind(
    before: &SheetState,
    after: &SheetState,
    side: &ShapePortSide,
) -> Option<ShapePortSide> {
    let edges = |state: &SheetState| -> Vec<MaterialEdge> {
        state
            .charts
            .iter()
            .filter(|p| p.domain == side.boundary.domain)
            .flat_map(|p| &p.regions)
            .flat_map(|r| &r.loops)
            .flat_map(|l| &l.edges)
            .filter(|e| e.piece.curve == side.boundary.piece.curve)
            .cloned()
            .collect()
    };
    let old: Vec<_> = edges(before)
        .into_iter()
        .filter(|e| {
            let (lo, hi) = (
                e.curve.range.first.min(e.curve.range.last),
                e.curve.range.first.max(e.curve.range.last),
            );
            e.piece == side.boundary.piece
                && side.span.first.min(side.span.last) >= lo - 1e-9
                && side.span.first.max(side.span.last) <= hi + 1e-9
        })
        .collect();
    let [old] = old.as_slice() else {
        return None;
    };
    let (p0, p1) = (
        old.curve.point(side.span.first).ok()?,
        old.curve.point(side.span.last).ok()?,
    );
    // Fragments of the same curve elsewhere in the chart may have been
    // renumbered; the seam is found again by position on any of them.
    for edge in edges(after) {
        let SegmentGeometry::Line { start, end } = edge.curve.geometry else {
            continue;
        };
        let d = crate::sketch::geometry::sub(end, start);
        let len = crate::sketch::geometry::dot(d, d);
        let at = |p: [f64; 2]| {
            crate::sketch::geometry::dot(crate::sketch::geometry::sub(p, start), d) / len
        };
        let (t0, t1) = (at(p0), at(p1));
        let (lo, hi) = (
            edge.curve.range.first.min(edge.curve.range.last),
            edge.curve.range.first.max(edge.curve.range.last),
        );
        let on = |t: f64, p: [f64; 2]| {
            t >= lo - 1e-9
                && t <= hi + 1e-9
                && edge
                    .curve
                    .point(t)
                    .is_ok_and(|q| crate::sketch::geometry::dist(p, q) <= TOLERANCE)
        };
        if len > 0.0 && on(t0, p0) && on(t1, p1) {
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

/// A seam to material outside the cut plane whose side in the plane the
/// cut shortened (mitcad#280: a cut ending on the seam's line, removing
/// material on the cut side only): the seam over the one stretch of its
/// line that boundary still has, the other side's boundary keeping the rest
/// as a free edge. None when the boundary no longer lies along the seam in
/// one stretch.
fn shortened(
    before: &SheetState,
    after: &SheetState,
    port: &ShapePort,
    cut_first: bool,
) -> Option<ShapePort> {
    use crate::sketch::geometry::{dist, dot, sub};
    let (cut, other) = if cut_first {
        (&port.first, &port.second)
    } else {
        (&port.second, &port.first)
    };
    // The other side lies outside the cut plane: it must be unchanged.
    let regions = |s: &SheetState| {
        s.charts
            .iter()
            .find(|p| p.domain == other.boundary.domain)
            .map(|p| p.regions.clone())
    };
    if regions(before).is_none() || regions(before) != regions(after) {
        return None;
    }
    let old = before
        .charts
        .iter()
        .filter(|p| p.domain == cut.boundary.domain)
        .flat_map(|p| &p.regions)
        .flat_map(|r| &r.loops)
        .flat_map(|l| &l.edges)
        .find(|e| e.piece == cut.boundary.piece)?;
    let (p0, p1) = (
        old.curve.point(cut.span.first).ok()?,
        old.curve.point(cut.span.last).ok()?,
    );
    let d = sub(p1, p0);
    let len = dot(d, d);
    if len <= 0.0 {
        return None;
    }
    let s = |q: [f64; 2]| dot(sub(q, p0), d) / len;
    let off = |q: [f64; 2]| {
        let t = s(q);
        dist(q, [p0[0] + t * d[0], p0[1] + t * d[1]])
    };
    let axial = |side: &ShapePortSide, t: f64| side.axial.offset + side.axial.scale * t;
    let (a0, a1) = (axial(cut, cut.span.first), axial(cut, cut.span.last));
    let mut found = None;
    for edge in after
        .charts
        .iter()
        .filter(|p| p.domain == cut.boundary.domain)
        .flat_map(|p| &p.regions)
        .flat_map(|r| &r.loops)
        .flat_map(|l| &l.edges)
        .filter(|e| e.piece.curve == cut.boundary.piece.curve)
    {
        let SegmentGeometry::Line { start, end } = edge.curve.geometry else {
            continue;
        };
        let (q0, q1) = (
            edge.curve.point(edge.curve.range.first).ok()?,
            edge.curve.point(edge.curve.range.last).ok()?,
        );
        if off(q0) > TOLERANCE || off(q1) > TOLERANCE {
            continue;
        }
        let (lo, hi) = (s(q0).min(s(q1)).max(0.0), s(q0).max(s(q1)).min(1.0));
        if hi - lo <= 1e-9 {
            continue;
        }
        if found.is_some() {
            return None;
        }
        // The edge's own parameters at the stretch's ends.
        let e = sub(end, start);
        let t = |s: f64| {
            let q = [p0[0] + s * d[0], p0[1] + s * d[1]];
            dot(sub(q, start), e) / dot(e, e)
        };
        let (t_lo, t_hi) = (t(lo), t(hi));
        let (x_lo, x_hi) = (a0 + (a1 - a0) * lo, a0 + (a1 - a0) * hi);
        let scale = (x_hi - x_lo) / (t_hi - t_lo);
        let cut_side = ShapePortSide {
            boundary: BoundaryPieceRef {
                domain: cut.boundary.domain,
                piece: edge.piece.clone(),
            },
            span: ParamSpan {
                first: t_lo,
                last: t_hi,
            },
            axial: AxialMap {
                offset: x_lo - scale * t_lo,
                scale,
            },
        };
        let param = |x: f64| (x - other.axial.offset) / other.axial.scale;
        let other_side = ShapePortSide {
            span: ParamSpan {
                first: param(x_lo),
                last: param(x_hi),
            },
            ..other.clone()
        };
        let interval = if port.axial_interval.first <= port.axial_interval.last {
            ParamSpan {
                first: x_lo.min(x_hi),
                last: x_lo.max(x_hi),
            }
        } else {
            ParamSpan {
                first: x_lo.max(x_hi),
                last: x_lo.min(x_hi),
            }
        };
        let (first, second) = if cut_first {
            (cut_side, other_side)
        } else {
            (other_side, cut_side)
        };
        found = Some(ShapePort {
            token: port.token,
            axial_interval: interval,
            first,
            second,
        });
    }
    found
}

/// The result of [`developed_cut`]: the input configuration with the cut
/// material, possibly disconnected.
pub(crate) struct CutOutcome {
    pub state: SheetState,
    pub removed: BTreeSet<SheetDomainRef>,
    pub used_tools: bool,
}

/// Removes `tools` (regions in the chart coordinates of `stationary`) from
/// every chart in that panel's plane once `open` bends are developed, then
/// restores the input configuration. `what` names the operation in errors.
pub(crate) fn developed_cut<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    state: &SheetState,
    thickness: f64,
    stationary: PanelId,
    open: &[BendId],
    tools: &[MaterialRegion],
    what: &str,
) -> Result<CutOutcome, String> {
    let mut targets = BTreeMap::new();
    for bend in open {
        bend_record(state, *bend, thickness)?;
        targets.insert(*bend, true);
    }
    let mut work = state.clone();
    reconfigure(&mut work, thickness, stationary, &targets)?;
    let fixed_embedding = chart(&work, stationary)?.embedding.clone();
    // Every chart developed into the fixed plane is cut in one operation.
    let domains: Vec<TrimDomain> = work
        .charts
        .iter()
        .filter_map(|p| {
            planar_map(&p.embedding, &fixed_embedding)
                .ok()
                .map(|to_operation| TrimDomain {
                    domain: p.domain,
                    regions: p.regions.clone(),
                    to_operation,
                })
        })
        .collect();
    let inside = |domain: SheetDomainRef| domains.iter().any(|d| d.domain == domain);
    let seams: Vec<TrimSeam> = work
        .joins
        .iter()
        .filter(|p| inside(p.first.boundary.domain) && inside(p.second.boundary.domain))
        .map(|p| {
            let side = |s: &ShapePortSide| TrimSeamSide {
                boundary: s.boundary.clone(),
                span: s.span,
                axial: s.axial,
            };
            TrimSeam {
                token: p.token,
                first: side(&p.first),
                second: side(&p.second),
                axial_interval: p.axial_interval,
            }
        })
        .collect();
    let result = ctx
        .kernel
        .sheet_trim_domain(&SheetTrimSpec {
            operation: SheetTrimOperation::Difference,
            domains: &domains,
            tools,
            seams: &seams,
            tolerance: TOLERANCE,
        })
        .map_err(|e| e.to_string())?;
    let before = work.clone();
    let settled = settle(&mut work, &domains, &result)?;
    if work.charts.is_empty() {
        return Err(format!("{what} removes all material of the sheet"));
    }
    // A seam to material outside the cut plane must survive whole: cutting it
    // needs that bend opened as well.
    let mut joins = Vec::new();
    for port in work
        .joins
        .iter()
        .filter(|p| !(inside(p.first.boundary.domain) && inside(p.second.boundary.domain)))
    {
        let rebound = match (
            rebind(&before, &work, &port.first),
            rebind(&before, &work, &port.second),
        ) {
            (Some(first), Some(second)) => Some(ShapePort {
                first,
                second,
                ..port.clone()
            }),
            // The cut ends on the seam and shortened it on the cut side
            // only (mitcad#280): the rest of the other side's boundary
            // becomes a free edge.
            _ => shortened(&before, &work, port, inside(port.first.boundary.domain)),
        };
        let Some(rebound) = rebound else {
            let (cut, folded) = if inside(port.first.boundary.domain) {
                (port.first.boundary.domain, port.second.boundary.domain)
            } else {
                (port.second.boundary.domain, port.first.boundary.domain)
            };
            return Err(format!(
                "{what} reaches the seam between {} and {}; open {} for it",
                domain_label(cut),
                domain_label(folded),
                match folded {
                    SheetDomainRef::Bend(_) => "that bend",
                    SheetDomainRef::Panel(_) => "the bend between them",
                }
            ));
        };
        joins.push(rebound);
    }
    let first_token = joins.iter().map(|p| p.token).max().map_or(0, |v| v + 1);
    for (next, port) in (first_token..).zip(&result.ports) {
        joins.push(ShapePort {
            token: next,
            axial_interval: port.axial_interval,
            first: port_side(&settled.tokens, port.axial_interval, &port.first)?,
            second: port_side(&settled.tokens, port.axial_interval, &port.second)?,
        });
    }
    work.joins = joins;
    for message in result.diagnostics {
        ctx.warn(message);
    }
    // Restore the input configuration: only embeddings change. A bend whose
    // material is gone loses its record and any open state with it.
    for patch in &mut work.charts {
        patch.embedding = state
            .charts
            .iter()
            .find(|p| p.domain == patch.domain)
            .ok_or("the cut changed the material domains")?
            .embedding
            .clone();
    }
    let removed = &settled.removed;
    work.bends = state
        .bends
        .iter()
        .filter(|b| !removed.contains(&SheetDomainRef::Bend(b.id)))
        .cloned()
        .collect();
    work.opened
        .retain(|o| !removed.contains(&SheetDomainRef::Bend(o.bend)));
    Ok(CutOutcome {
        state: work,
        removed: settled.removed,
        used_tools: settled.used_tools,
    })
}

/// The cut profiles in the chart coordinates of the fixed panel. Each
/// sketch lies on that panel's front or back surface (with the bends
/// developed the panel does not move, so its input chart serves).
fn sketch_tools<K: Kernel>(
    d: &SheetCutDef<f64>,
    stationary: PanelId,
    ctx: &mut EvalContext<'_, K>,
    state: &SheetState,
    thickness: f64,
) -> Result<Vec<MaterialRegion>, String> {
    let fixed = SheetDomainRef::Panel(stationary);
    let (frame, parity) = plane(chart(state, stationary)?)?;
    let f = frame.normal().map(|v| v * parity.sign());
    let mut tools = Vec::new();
    let mut sketches: Vec<FeatureUid> = d.profiles.iter().map(|p| p.sketch).collect();
    sketches.sort();
    sketches.dedup();
    for sketch_uid in sketches {
        let sketch = ctx.sketch(sketch_uid)?;
        let s = sketch.frame;
        let offset = dot(sub(s.origin, frame.origin), f);
        if dot(s.normal(), frame.normal()).abs() < 1.0 - 1e-9
            || (offset.abs() > 1e-6 && (offset - thickness).abs() > 1e-6)
        {
            return Err(format!(
                "the cut sketch {sketch_uid} is not on a surface of the developed {}",
                domain_label(fixed)
            ));
        }
        let o = sub(s.origin, frame.origin);
        let to_chart = Isometry2 {
            origin: [dot(o, frame.x_axis), dot(o, frame.y_axis)],
            x_axis: [dot(s.x_axis, frame.x_axis), dot(s.x_axis, frame.y_axis)],
            y_axis: [dot(s.y_axis, frame.x_axis), dot(s.y_axis, frame.y_axis)],
        };
        for profile in d.profiles.iter().filter(|p| p.sketch == sketch_uid) {
            // A changed sketch keeps resolving the region as extrusions do
            // (all pieces of a split one, with a warning; mitcad#407), so a
            // dimension edit that changes adjacency keeps the cut.
            for region in found_profiles(ctx, &sketch, sketch_uid, &profile.region)? {
                let material = MaterialRegion::from_legacy(&sheet_region(sketch_uid, &region))?;
                tools.extend(mapped_regions(&[material], to_chart));
            }
        }
    }
    Ok(tools)
}

/// A simple hole through a sheet as a cut of its material domain
/// (mitcad#211): when `body` is a valid sheet and every axis starts on the
/// front or back surface of one planar panel, normal to it and into the
/// material, as deep as the sheet at least (`depth` None: through all),
/// its circles are removed from the panel's plane as `sheet_cut` removes
/// regions, so the body stays a sheet whose later features use its
/// domains. `None` where it does not apply (the hole is then an ordinary
/// cut, which leaves the body's sheet meaning invalid).
pub(crate) fn hole_cut<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    body: BodyUid,
    positions: &[crate::kernel::Axis],
    diameter: f64,
    depth: Option<f64>,
) -> Result<Option<FeatureOutput<K::Shape>>, String> {
    if ctx.sheet(body).is_none_or(|s| s.value.status() != "valid") {
        return Ok(None);
    }
    let Ok((state, rules)) = read(ctx, body) else {
        return Ok(None);
    };
    let thickness = rules.thickness;
    if depth.is_some_and(|d| d < thickness - TOLERANCE) || positions.is_empty() {
        return Ok(None);
    }
    let mut found = None;
    for patch in &state.charts {
        let SheetDomainRef::Panel(id) = patch.domain else {
            continue;
        };
        let Ok((frame, parity)) = plane(patch) else {
            continue;
        };
        let n = frame.normal().map(|c| c * parity.sign());
        let on = positions.iter().all(|a| {
            let q = dot(sub(a.origin, frame.origin), n);
            let into = dot(a.direction, n);
            (q.abs() <= 1e-6 && into > 1.0 - 1e-9)
                || ((q - thickness).abs() <= 1e-6 && into < -1.0 + 1e-9)
        });
        if on {
            found = Some((id, frame));
            break;
        }
    }
    let Some((panel, frame)) = found else {
        return Ok(None);
    };
    let mut tools = Vec::new();
    for (k, a) in positions.iter().enumerate() {
        let o = sub(a.origin, frame.origin);
        let center = [dot(o, frame.x_axis), dot(o, frame.y_axis)];
        tools.push(MaterialRegion::from_legacy(&SheetRegion {
            loops: vec![SheetLoop {
                segments: vec![SheetSegment {
                    id: SheetCurveId::Generated {
                        feature: ctx.uid,
                        slot: k as u32,
                    },
                    geometry: SegmentGeometry::Circle {
                        center,
                        radius: diameter / 2.0,
                        start: 0.0,
                    },
                }],
            }],
        })?);
    }
    // A hole the sheet's domains cannot take (one reaching a bend) is an
    // ordinary cut, as before.
    let Ok(outcome) = developed_cut(ctx, &state, thickness, panel, &[], &tools, "the hole") else {
        return Ok(None);
    };
    if !outcome.used_tools {
        return Ok(None);
    }
    Ok(publish_pieces(ctx, outcome.state, &rules, None).ok())
}

pub(crate) fn cut<K: Kernel>(
    d: &SheetCutDef<f64>,
    ctx: &mut EvalContext<'_, K>,
) -> Result<FeatureOutput<K::Shape>, String> {
    let (state, rules) = read(ctx, d.body)?;
    if d.profiles.is_empty() {
        return Err("no profiles selected".into());
    }
    let stationary = d
        .stationary
        .ok_or("a developed sheet cut names its stationary panel")?;
    let tools = sketch_tools(d, stationary, ctx, &state, rules.thickness)?;
    let outcome = developed_cut(
        ctx,
        &state,
        rules.thickness,
        stationary,
        &d.open_bends,
        &tools,
        "the cut",
    )?;
    if !outcome.used_tools {
        ctx.warn("the cut profiles do not reach the sheet material");
    }
    publish_pieces(ctx, outcome.state, &rules, None)
}

/// A straight sheet cut (mitcad#242): the profiles swept along the
/// sketch's normal (against it with `flip`) from the sketch's plane, as
/// far as the extent, are cut from the folded sheet. Where the result is
/// still the sheet's material changed through its thickness (the cut stays
/// in panels normal to its direction), the domains are re-derived from it
/// as for an ordinary feature, so the cut is an ordinary domain edit. Where
/// it crosses a bend or an inclined panel its walls follow the sweep, which
/// the domains cannot hold: the cut is kept as an overlay of the sheet,
/// cut from the solid every later sheet feature rebuilds.
pub(crate) fn straight_cut<K: Kernel>(
    d: &SheetCutDef<f64>,
    ctx: &mut EvalContext<'_, K>,
) -> Result<FeatureOutput<K::Shape>, String> {
    let (frame, profiles) = straight_profiles(d, ctx)?;
    let direction = frame.normal().map(|v| if d.flip { -v } else { v });
    straight_cut_on(ctx, d.body, frame, profiles, direction, d.extent.as_ref())
}

/// A straight cut's sketch frame and its profiles in it.
pub(crate) fn straight_profiles<K: Kernel>(
    d: &SheetCutDef<f64>,
    ctx: &mut EvalContext<'_, K>,
) -> Result<
    (
        crate::profile::SketchFrame,
        Vec<(crate::topo::RegionKey, SheetRegion)>,
    ),
    String,
> {
    let sketch_uid = d.profiles.first().ok_or("no profiles selected")?.sketch;
    let sketch = ctx.sketch(sketch_uid)?;
    let frame = sketch.frame;
    let mut profiles = Vec::new();
    for profile in &d.profiles {
        for region in found_profiles(ctx, &sketch, sketch_uid, &profile.region)? {
            profiles.push((region.key.clone(), sheet_region(sketch_uid, &region)));
        }
    }
    Ok((frame, profiles))
}

/// The regions a cut profile means in its evaluated sketch, as extrusions
/// find them (the main one under the referenced key, the other pieces of a
/// split one under their own), warning when the key changed (mitcad#407).
fn found_profiles<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    sketch: &crate::features::SketchOutput,
    sketch_uid: FeatureUid,
    key: &crate::topo::RegionKey,
) -> Result<Vec<crate::profile::ProfileRegion>, String> {
    let found = sketch
        .found(key)
        .ok_or_else(|| format!("cut profile {key} is unavailable"))?;
    if let Some(warning) = found.warning(key, &ctx.feature_name(sketch_uid)) {
        ctx.warn(warning);
    }
    Ok(found.profiles(key))
}

/// A sketch region as a sheet region, its segments named in the sketch.
fn sheet_region(sketch: FeatureUid, region: &crate::profile::ProfileRegion) -> SheetRegion {
    SheetRegion {
        loops: region
            .loops
            .iter()
            .map(|l| SheetLoop {
                segments: l
                    .segments
                    .iter()
                    .map(|s| SheetSegment {
                        id: SheetCurveId::Sketch {
                            sketch,
                            segment: s.key.clone(),
                        },
                        geometry: s.geometry.clone(),
                    })
                    .collect(),
            })
            .collect(),
    }
}

/// The straight cut of `profiles` in `frame` along `direction` from the
/// frame's plane on the sheet `body` (also the copies a pattern or mirror
/// makes, [`crate::sheet::replay`]).
pub(crate) fn straight_cut_on<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    body: BodyUid,
    frame: crate::profile::SketchFrame,
    profiles: Vec<(crate::topo::RegionKey, SheetRegion)>,
    direction: [f64; 3],
    extent: Option<&crate::features::sheet_cut::SheetCutExtent<f64>>,
) -> Result<FeatureOutput<K::Shape>, String> {
    use crate::features::sheet_cut::SheetCutExtent;
    let (mut state, rules) = read(ctx, body)?;
    let current = ctx.body(body)?;
    let end = match extent {
        Some(SheetCutExtent::Distance { distance }) => {
            if !distance.is_finite() || *distance <= TOLERANCE {
                return Err("a straight sheet cut's distance must be positive".into());
            }
            *distance
        }
        Some(SheetCutExtent::ThroughAll) => {
            let bounds = ctx
                .kernel
                .bounding_box(&current)
                .map_err(|e| e.to_string())?
                .ok_or("the sheet has no extent")?;
            let reach = bounds
                .corners()
                .map(|c| dot(sub(c, frame.origin), direction))
                .fold(f64::NEG_INFINITY, f64::max);
            if reach <= 0.0 {
                return Err("the sheet lies behind the straight cut's sketch".into());
            }
            reach * 1.25 + 1.0
        }
        None => return Err("a straight sheet cut needs an extent".into()),
    };
    let mut overlay = SheetOverlay {
        feature: ctx.uid,
        kind: SheetOverlayKind::Straight {
            frame,
            profiles,
            direction,
            start: 0.0,
            end,
        },
        removed: 0.0,
    };
    let tool = straight_tool(ctx, &overlay)?;
    let cut = ctx
        .kernel
        .boolean(crate::kernel::BooleanOp::Cut, &[&current], &tool)
        .map_err(|e| e.to_string())?;
    let mut output = FeatureOutput::default();
    if !cut.touched.first().copied().unwrap_or(false) {
        ctx.warn("the cut profiles do not reach the sheet material");
        output.changes.push(BodyChange::Set(body, current));
        state.shape_generation.clear();
        output.sheets.insert(body, SheetChange::Set(state));
        return Ok(output);
    }
    let [piece] = cut.pieces.as_slice() else {
        return Err(format!(
            "the straight cut leaves {} pieces of the sheet",
            cut.pieces.len()
        ));
    };
    let result = piece.shape.clone();
    let volume = |shape: &K::Shape| -> Result<f64, String> {
        Ok(ctx
            .kernel
            .mass_properties(shape)
            .map_err(|e| e.to_string())?
            .volume)
    };
    overlay.removed = volume(&current)? - volume(&result)?;
    let warnings = ctx.warnings.len();
    let derived =
        crate::sheet::rederive::rederive_from(ctx, state.clone(), &rules, &current, &result);
    ctx.warnings.truncate(warnings);
    let mut next = match derived {
        Ok(derived) => derived,
        Err(_) => {
            state.overlays.push(overlay);
            state
        }
    };
    next.shape_generation.clear();
    output.changes.push(BodyChange::Set(body, result));
    output.sheets.insert(body, SheetChange::Set(next));
    Ok(output)
}

/// The copies of one straight cut cut at once (a pattern of a straight
/// sheet cut, mitcad#288): the tools of the copies that keep clear of every
/// seam between a panel and a bend (their boxes do not reach the seam's line
/// on the front or the back surface) cut from the sheet in one boolean, and the domains
/// re-derived once from the result. Returns the result and the indices of
/// the copies left out, which are cut one by one after it (a copy reaching
/// a bend stays an overlay of its own, as it does alone). None when the
/// copies cannot be taken together: no copy clear of the seams, a copy that
/// does not reach the material, a result in several pieces, or a result the
/// re-derivation does not take; the copies are then all cut one by one.
#[allow(clippy::type_complexity)]
pub(crate) fn straight_cuts_at_once<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    body: BodyUid,
    cuts: Vec<(
        crate::profile::SketchFrame,
        Vec<(crate::topo::RegionKey, SheetRegion)>,
        [f64; 3],
    )>,
    extent: Option<&crate::features::sheet_cut::SheetCutExtent<f64>>,
) -> Result<Option<(K::Shape, SheetState, Vec<usize>)>, String> {
    use crate::features::sheet_cut::SheetCutExtent;
    let (state, rules) = read(ctx, body)?;
    let current = ctx.body(body)?;
    let bounds = ctx
        .kernel
        .bounding_box(&current)
        .map_err(|e| e.to_string())?
        .ok_or("the sheet has no extent")?;
    // The seams between panels and bends, as lines in model space, on the
    // front and the back surface.
    let mut seams: Vec<([f64; 3], [f64; 3])> = Vec::new();
    for port in &state.joins {
        for (here, there) in [(&port.first, &port.second), (&port.second, &port.first)] {
            let (SheetDomainRef::Panel(panel), SheetDomainRef::Bend(_)) =
                (here.boundary.domain, there.boundary.domain)
            else {
                continue;
            };
            let Ok((frame, parity)) = plane(chart(&state, panel)?) else {
                continue;
            };
            let Some((a, b)) = port_ends(&state, here) else {
                return Ok(None);
            };
            let back = frame.normal().map(|v| v * parity.sign() * rules.thickness);
            seams.push((frame.point(a), frame.point(b)));
            seams.push((add(frame.point(a), back), add(frame.point(b), back)));
        }
    }
    let grow = 1.0e-4;
    let mut tools = Vec::new();
    let mut left = Vec::new();
    for (index, (frame, profiles, direction)) in cuts.into_iter().enumerate() {
        let end = match extent {
            Some(SheetCutExtent::Distance { distance }) => *distance,
            Some(SheetCutExtent::ThroughAll) => {
                let reach = bounds
                    .corners()
                    .map(|c| dot(sub(c, frame.origin), direction))
                    .fold(f64::NEG_INFINITY, f64::max);
                if reach <= 0.0 {
                    return Ok(None);
                }
                reach * 1.25 + 1.0
            }
            None => return Err("a straight sheet cut needs an extent".into()),
        };
        if !end.is_finite() || end <= TOLERANCE {
            return Err("a straight sheet cut's distance must be positive".into());
        }
        let overlay = SheetOverlay {
            feature: ctx.uid,
            kind: SheetOverlayKind::Straight {
                frame,
                profiles,
                direction,
                start: 0.0,
                end,
            },
            removed: 0.0,
        };
        let tool = straight_tool(ctx, &overlay)?;
        let Some(boxed) = ctx.kernel.bounding_box(&tool).map_err(|e| e.to_string())? else {
            return Ok(None);
        };
        let min = boxed.min.map(|v| v - grow);
        let max = boxed.max.map(|v| v + grow);
        if seams
            .iter()
            .any(|(a, b)| segment_meets_box(*a, *b, min, max))
        {
            left.push(index);
        } else {
            tools.push(tool);
        }
    }
    if tools.is_empty() {
        return Ok(None);
    }
    let Ok(tool) = ctx.kernel.compound(&tools) else {
        return Ok(None);
    };
    let cut = ctx
        .kernel
        .boolean(crate::kernel::BooleanOp::Cut, &[&current], &tool)
        .map_err(|e| e.to_string())?;
    let [piece] = cut.pieces.as_slice() else {
        return Ok(None);
    };
    if !cut.touched.first().copied().unwrap_or(false) {
        return Ok(None);
    }
    let result = piece.shape.clone();
    let warnings = ctx.warnings.len();
    let derived =
        crate::sheet::rederive::rederive_from(ctx, state.clone(), &rules, &current, &result);
    ctx.warnings.truncate(warnings);
    let Ok(mut next) = derived else {
        return Ok(None);
    };
    next.shape_generation.clear();
    Ok(Some((result, next, left)))
}

/// Whether the segment `a`-`b` meets the box `min`-`max` (slabs).
fn segment_meets_box(a: [f64; 3], b: [f64; 3], min: [f64; 3], max: [f64; 3]) -> bool {
    let (mut t0, mut t1) = (0.0f64, 1.0f64);
    for k in 0..3 {
        let d = b[k] - a[k];
        if d.abs() <= f64::EPSILON {
            if a[k] < min[k] || a[k] > max[k] {
                return false;
            }
            continue;
        }
        let (mut lo, mut hi) = ((min[k] - a[k]) / d, (max[k] - a[k]) / d);
        if lo > hi {
            std::mem::swap(&mut lo, &mut hi);
        }
        t0 = t0.max(lo);
        t1 = t1.min(hi);
        if t0 > t1 {
            return false;
        }
    }
    true
}

/// The tool of a straight cut kept as an overlay.
fn straight_tool<K: Kernel>(
    ctx: &EvalContext<'_, K>,
    overlay: &SheetOverlay,
) -> Result<K::Shape, String> {
    let SheetOverlayKind::Straight {
        frame,
        profiles,
        direction,
        start,
        end,
    } = &overlay.kind
    else {
        return Err("not a straight cut".into());
    };
    let regions: Vec<crate::profile::ProfileRegion> = profiles
        .iter()
        .map(|(key, region)| crate::profile::ProfileRegion {
            key: key.clone(),
            loops: region
                .loops
                .iter()
                .map(|l| crate::profile::ProfileLoop {
                    segments: l
                        .segments
                        .iter()
                        .map(|s| crate::profile::ProfileSegment {
                            key: match &s.id {
                                SheetCurveId::Sketch { segment, .. } => segment.clone(),
                                SheetCurveId::Generated { .. } => {
                                    "c1".parse().expect("a segment key")
                                }
                            },
                            geometry: s.geometry.clone(),
                        })
                        .collect(),
                })
                .collect(),
        })
        .collect();
    ctx.kernel
        .extrude(&crate::kernel::ExtrudeSpec {
            feature: overlay.feature,
            frame: *frame,
            regions: &regions,
            direction: *direction,
            start: *start,
            end: *end,
        })
        .map_err(|e| e.to_string())
}

/// The pieces of material a feature removed and added.
pub(crate) type Change<S> = (Vec<S>, Vec<S>);

/// The material an earlier feature changed in `body`: the pieces it
/// removed and those it added (exact booleans of the body before it and
/// its result), moved by `placement`.
pub(crate) fn feature_change<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    feature: FeatureUid,
    body: BodyUid,
    placement: &crate::transform::Transform,
) -> Result<Change<K::Shape>, String> {
    let name = ctx.feature_name(feature);
    let before = ctx
        .body_before(feature, body)
        .ok_or_else(|| format!("{name} has no body before it"))?;
    let after = ctx
        .env
        .history
        .iter()
        .find(|r| r.uid == feature)
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
    let mut removed = Vec::new();
    let mut added = Vec::new();
    for (into, from, less) in [
        (&mut removed, &before, &after),
        (&mut added, &after, &before),
    ] {
        let difference = ctx
            .kernel
            .boolean(crate::kernel::BooleanOp::Cut, &[from], less)
            .map_err(|e| e.to_string())?;
        for piece in difference.pieces {
            if volume(&piece.shape)? > least {
                into.push(if placement.is_identity() {
                    piece.shape
                } else {
                    ctx.kernel
                        .transform_shape(&piece.shape, placement, None)
                        .map_err(|e| e.to_string())?
                });
            }
        }
    }
    Ok((removed, added))
}

/// A rebuilt sheet with its overlays made on it again, in order. An
/// overlay that would remove more than it did when it was made (material
/// added after it, in its way) is refused.
pub(crate) fn with_overlays<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    state: &SheetState,
    mut shape: K::Shape,
) -> Result<K::Shape, String> {
    use crate::kernel::BooleanOp;
    let volume = |ctx: &EvalContext<'_, K>, shape: &K::Shape| -> Result<f64, String> {
        Ok(ctx
            .kernel
            .mass_properties(shape)
            .map_err(|e| e.to_string())?
            .volume)
    };
    for overlay in &state.overlays {
        let tools: Vec<(BooleanOp, K::Shape)> = match &overlay.kind {
            SheetOverlayKind::Straight { .. } => {
                vec![(BooleanOp::Cut, straight_tool(ctx, overlay)?)]
            }
            SheetOverlayKind::Change { placement, source } => {
                // A copy's change is read from the body it was made on
                // (mitcad#1462).
                let body = source.unwrap_or(state.body);
                let (removed, added) = feature_change(ctx, overlay.feature, body, placement)?;
                removed
                    .into_iter()
                    .map(|s| (BooleanOp::Cut, s))
                    .chain(added.into_iter().map(|s| (BooleanOp::Join, s)))
                    .collect()
            }
        };
        let before = volume(ctx, &shape)?;
        for (operation, tool) in tools {
            let result = ctx
                .kernel
                .boolean(operation, &[&shape], &tool)
                .map_err(|e| e.to_string())?;
            if !result.touched.first().copied().unwrap_or(false) {
                continue;
            }
            let pieces: Vec<_> = result
                .pieces
                .into_iter()
                .filter(|p| !p.sources.is_empty())
                .collect();
            let [piece] = pieces.as_slice() else {
                return Err(format!(
                    "the overlay of {} leaves {} pieces of the rebuilt sheet",
                    overlay.feature,
                    pieces.len()
                ));
            };
            shape = piece.shape.clone();
        }
        let after = volume(ctx, &shape)?;
        if before - after > overlay.removed + 1e-6 * overlay.removed.abs().max(1.0) {
            return Err(format!(
                "{} would also change material added after it ({:.6} mm3 more)",
                overlay.feature,
                before - after - overlay.removed
            ));
        }
    }
    Ok(shape)
}
