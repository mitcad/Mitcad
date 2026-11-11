// SPDX-License-Identifier: MIT
//! Ordinary cuts across bends (mitcad#1628).
//!
//! An ordinary feature (an extrusion, a hole) that only removes material and
//! whose removed region crosses one or more bends is applied to the sheet as
//! `sheet_cut` would apply it:
//!
//! - every chart's footprint is read from the result's faces on the chart's
//!   front surface: a panel's (or an open bend's) plane, a folded bend's
//!   cylinder (the faces' own parameter curves mapped into the bend's
//!   developed coordinates by the kernel, so nothing is sampled);
//! - the material each chart lost there, in its own coordinates, is mapped
//!   into the developed plane of a panel with the bends it touches opened,
//!   and removed from every chart developed into that plane in one trim
//!   whose seams are the registered ports; the bends are then folded again;
//! - in a bend the cut is then through the thickness, normal to the
//!   material, not along the feature's own walls: where the rebuilt sheet
//!   differs from the feature's result (only inside the bends it cut, and
//!   only where the feature cut through the material there), the body
//!   becomes the rebuilt sheet and the feature gets a warning.
//!
//! Anything else (material added, a cut in a bend that does not go through
//! the material, a panel's walls not normal to it) is refused with a reason,
//! and the sheet meaning is lost as before.

use std::collections::BTreeMap;

use super::super::folded::develop::{chart_coordinates, planar_map, reconfigure, region_area};
use super::super::folded::{cut::developed_cut, mapped_regions};
use super::*;
use crate::kernel::BooleanOp;

/// The sheet `state` of `before` with the cut of `shape` across its bends
/// applied, and the body's shape where it is not the feature's result; None
/// where the change touches no folded bend (the planar adapter decides).
pub(super) fn across_bends<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    state: SheetState,
    rules: &crate::features::sheet::SheetRulesDef<f64>,
    before: &K::Shape,
    shape: &K::Shape,
) -> Result<Option<Derived<K::Shape>>, Failure> {
    let thickness = rules.thickness;
    let mut slot = 2_000_000;
    // The material each folded bend lost on its front surface.
    let mut lost: Vec<(SheetDomainRef, Vec<MaterialRegion>)> = Vec::new();
    for patch in &state.charts {
        let SheetEmbedding::Cylinder(c) = &patch.embedding else {
            continue;
        };
        let footprint = match cylinder_footprint(ctx, shape, c, thickness, false, &mut slot) {
            Ok(footprint) => footprint,
            Err(Failure::Unsupported) => return Ok(None),
            Err(other) => return Err(other),
        };
        let base = if state.overlays.is_empty() {
            patch.regions.clone()
        } else {
            cylinder_footprint(ctx, before, c, thickness, false, &mut slot)?
        };
        let regions = difference(ctx, patch.domain, base, &footprint, &mut slot)?;
        if !regions.is_empty() {
            lost.push((patch.domain, regions));
        }
    }
    if lost.is_empty() {
        return Ok(None);
    }
    let name = ctx.feature_name(ctx.uid);
    // It only removes material.
    let volume = |ctx: &EvalContext<'_, K>, s: &K::Shape| -> Result<f64, Failure> {
        Ok(kernel(ctx.kernel.mass_properties(s))?.volume)
    };
    let least = 1e-9 * volume(ctx, before)?.abs().max(1.0);
    let added = kernel(ctx.kernel.boolean(BooleanOp::Cut, &[shape], before))?;
    for piece in &added.pieces {
        if volume(ctx, &piece.shape)? > least {
            return Err("it adds material where it crosses a bend".into());
        }
    }
    // In a bend it cuts through the material: what it removed on the front
    // surface it also removed on the back.
    let touched: Vec<BendId> = lost
        .iter()
        .filter_map(|(d, _)| match d {
            SheetDomainRef::Bend(b) => Some(*b),
            SheetDomainRef::Panel(_) => None,
        })
        .collect();
    for (domain, front) in &lost {
        let patch = state
            .charts
            .iter()
            .find(|p| p.domain == *domain)
            .ok_or("a bend lost its chart")?;
        let SheetEmbedding::Cylinder(c) = &patch.embedding else {
            continue;
        };
        let footprint = cylinder_footprint(ctx, shape, c, thickness, true, &mut slot)?;
        let base = if state.overlays.is_empty() {
            patch.regions.clone()
        } else {
            cylinder_footprint(ctx, before, c, thickness, true, &mut slot)?
        };
        let back = difference(ctx, *domain, base, &footprint, &mut slot)?;
        for region in front {
            let common = kernel(ctx.kernel.sheet_trim_domain(&SheetTrimSpec {
                operation: SheetTrimOperation::Intersection,
                domains: &[TrimDomain {
                    domain: *domain,
                    regions: vec![region.clone()],
                    to_operation: Isometry2::IDENTITY,
                }],
                tools: &back,
                seams: &[],
                tolerance: TOLERANCE,
            }))?;
            let mut area = 0.0;
            for r in settled_regions(ctx.uid, &mut slot, &common.domains[0].regions)? {
                area += region_area(&r)?;
            }
            if area <= MINIMUM_AREA {
                return Err(super::super::folded::name_walls(
                    ctx,
                    &format!(
                        "it does not cut through the material in {}",
                        super::super::folded::develop::domain_label(*domain)
                    ),
                )
                .into());
            }
        }
    }
    // The material the planar charts lost on their front surfaces.
    let faces = kernel(ctx.kernel.face_geometries(shape))?;
    let before_faces = if state.overlays.is_empty() {
        Vec::new()
    } else {
        kernel(ctx.kernel.face_geometries(before))?
    };
    for patch in &state.charts {
        let SheetEmbedding::Plane {
            front_frame,
            front_to_back_sign,
        } = &patch.embedding
        else {
            continue;
        };
        let f = front_frame.normal().map(|v| v * front_to_back_sign.sign());
        let footprint = front_outline(ctx, &faces, shape, front_frame, f, &mut slot)?;
        let base = if state.overlays.is_empty() {
            patch.regions.clone()
        } else {
            front_outline(ctx, &before_faces, before, front_frame, f, &mut slot)?
        };
        let regions = difference(ctx, patch.domain, base, &footprint, &mut slot)?;
        if !regions.is_empty() {
            lost.push((patch.domain, regions));
        }
    }
    // The touched bends as they are folded, where the rebuilt sheet may
    // differ from the result.
    let folded: Vec<SheetEmbedding> = state
        .charts
        .iter()
        .filter(|p| matches!(p.domain, SheetDomainRef::Bend(b) if touched.contains(&b)))
        .map(|p| p.embedding.clone())
        .collect();
    // Each group of charts that develops into one plane with the touched
    // bends open is cut in one operation, as `sheet_cut` cuts.
    let mut remaining = lost;
    let mut state = state;
    while !remaining.is_empty() {
        let stationary = stationary_panel(&state, &remaining)
            .ok_or("the material it removed is on no panel's side of a bend")?;
        let open: Vec<BendId> = touched
            .iter()
            .copied()
            .filter(|b| state.bends.iter().any(|r| r.id == *b))
            .collect();
        let mut work = state.clone();
        let targets: BTreeMap<BendId, bool> = open.iter().map(|b| (*b, true)).collect();
        reconfigure(&mut work, thickness, stationary, &targets)?;
        let fixed = work
            .charts
            .iter()
            .find(|p| p.domain == SheetDomainRef::Panel(stationary))
            .ok_or("the stationary panel has no chart")?
            .embedding
            .clone();
        let mut tools = Vec::new();
        let mut rest = Vec::new();
        for (domain, regions) in remaining {
            let map = work
                .charts
                .iter()
                .find(|p| p.domain == domain)
                .and_then(|p| planar_map(&p.embedding, &fixed).ok());
            match map {
                Some(map) => tools.extend(mapped_regions(&regions, map)),
                None => rest.push((domain, regions)),
            }
        }
        if tools.is_empty() {
            return Err("the material it removed does not develop into one plane".into());
        }
        let tools = merged(ctx, SheetDomainRef::Panel(stationary), tools, &mut slot)?;
        let outcome = developed_cut(
            ctx,
            &state,
            thickness,
            stationary,
            &open,
            &tools,
            &format!("{name}'s cut"),
        )?;
        state = outcome.state;
        remaining = rest;
    }
    // The rebuilt sheet, and how it differs from the feature's result.
    let geometry = kernel(ctx.kernel.sheet_shape(
        &SheetShapeSpec {
            thickness,
            patches: &state.charts,
            ports: &state.joins,
            tolerance: TOLERANCE,
        },
        &ctx.uid.to_string(),
    ))?;
    let [solid] = geometry.solids.as_slice() else {
        return Err(format!(
            "its re-derived material makes {} solids",
            geometry.solids.len()
        )
        .into());
    };
    let rebuilt = super::super::folded::cut::with_overlays(ctx, &state, solid.shape.clone())?;
    let a = kernel(ctx.kernel.mass_properties(&rebuilt))?;
    let b = kernel(ctx.kernel.mass_properties(shape))?;
    let size = b.volume.abs().cbrt().max(1.0);
    let off = |x: f64, y: f64, scale: f64| (x - y).abs() > 1e-6 * scale.max(1.0);
    let same = !(off(a.volume, b.volume, b.volume.abs())
        || off(a.area, b.area, b.area.abs())
        || (0..3).any(|i| off(a.center[i], b.center[i], size)));
    let mut derived = Derived {
        state: finished(state, rules)?,
        shape: None,
        warning: None,
    };
    if same {
        return Ok(Some(derived));
    }
    // Where they differ: only inside the bends it cut.
    for (from, less) in [(shape, &rebuilt), (&rebuilt, shape)] {
        let difference = kernel(ctx.kernel.boolean(BooleanOp::Cut, &[from], less))?;
        for piece in &difference.pieces {
            let mass = kernel(ctx.kernel.mass_properties(&piece.shape))?;
            if mass.volume <= least {
                continue;
            }
            let inside = folded.iter().any(|c| {
                chart_coordinates(c, mass.center, thickness)
                    .is_some_and(|(_, depth)| (-TOLERANCE..=thickness + TOLERANCE).contains(&depth))
            });
            if !inside {
                return Err(format!(
                    "the result is not the sheet's material changed through its thickness ({:.6} \
                     mm3 differ outside the bends it crosses): material added, a bend changed or \
                     walls not normal to the material",
                    mass.volume
                )
                .into());
            }
        }
    }
    derived.shape = Some(rebuilt);
    derived.warning = Some(format!(
        "{name} cuts across a bend: cut normal to the sheet there, as Sheet Cut does"
    ));
    Ok(Some(derived))
}

/// Material regions smaller than this (mm2) are rounding, not material.
const MINIMUM_AREA: f64 = 1e-9;

/// A panel to hold while the touched bends open: the first panel that lost
/// material, else a panel joined to the first bend that did.
fn stationary_panel(
    state: &SheetState,
    remaining: &[(SheetDomainRef, Vec<MaterialRegion>)],
) -> Option<PanelId> {
    if let Some(panel) = remaining.iter().find_map(|(d, _)| match d {
        SheetDomainRef::Panel(p) => Some(*p),
        SheetDomainRef::Bend(_) => None,
    }) {
        return Some(panel);
    }
    let (first, _) = remaining.first()?;
    state.joins.iter().find_map(|port| {
        for (own, other) in [(&port.first, &port.second), (&port.second, &port.first)] {
            if own.boundary.domain == *first
                && let SheetDomainRef::Panel(p) = other.boundary.domain
            {
                return Some(p);
            }
        }
        None
    })
}

/// The faces of `shape` on a folded bend's front (or back) cylinder, as
/// material regions in the bend's chart.
fn cylinder_footprint<K: Kernel>(
    ctx: &EvalContext<'_, K>,
    shape: &K::Shape,
    c: &CylinderEmbedding,
    thickness: f64,
    back: bool,
    slot: &mut u32,
) -> Result<Vec<MaterialRegion>, Failure> {
    let radius = c.inside_radius
        + if back == c.inner_is_front {
            thickness
        } else {
            0.0
        };
    let mut out = Vec::new();
    for face in kernel(
        ctx.kernel
            .sheet_cylinder_footprint(shape, c, radius, RECOGNITION),
    )? {
        let curves = face.into_iter().map(|e| e.geometry).collect();
        out.extend(regions(ctx.uid, slot, curves)?);
    }
    Ok(out)
}

/// `base` less `footprint` in one chart's coordinates, without slivers of
/// rounding.
fn difference<K: Kernel>(
    ctx: &EvalContext<'_, K>,
    domain: SheetDomainRef,
    base: Vec<MaterialRegion>,
    footprint: &[MaterialRegion],
    slot: &mut u32,
) -> Result<Vec<MaterialRegion>, Failure> {
    let result = kernel(ctx.kernel.sheet_trim_domain(&SheetTrimSpec {
        operation: SheetTrimOperation::Difference,
        domains: &[TrimDomain {
            domain,
            regions: base,
            to_operation: Isometry2::IDENTITY,
        }],
        tools: footprint,
        seams: &[],
        tolerance: TOLERANCE,
    }))?;
    let mut out = Vec::new();
    for domain in result.domains {
        for region in settled_regions(ctx.uid, slot, &domain.regions)? {
            if region_area(&region)? > MINIMUM_AREA {
                out.push(region);
            }
        }
    }
    Ok(out)
}

/// The lost regions of several charts as one set of tools: regions that
/// meet at a seam become one region crossing it, as a Sheet Cut profile
/// does.
fn merged<K: Kernel>(
    ctx: &EvalContext<'_, K>,
    domain: SheetDomainRef,
    tools: Vec<MaterialRegion>,
    slot: &mut u32,
) -> Result<Vec<MaterialRegion>, Failure> {
    let Some((first, others)) = tools.split_first() else {
        return Ok(tools);
    };
    if others.is_empty() {
        return Ok(tools);
    }
    let result = kernel(ctx.kernel.sheet_trim_domain(&SheetTrimSpec {
        operation: SheetTrimOperation::Union,
        domains: &[TrimDomain {
            domain,
            regions: vec![first.clone()],
            to_operation: Isometry2::IDENTITY,
        }],
        tools: others,
        seams: &[],
        tolerance: TOLERANCE,
    }))?;
    let mut out = Vec::new();
    for domain in result.domains {
        for r in &domain.regions {
            let mut loops = Vec::new();
            for l in &r.loops {
                let mut edges = Vec::new();
                for e in &l.edges {
                    let piece = e
                        .lineage
                        .iter()
                        .map(|s| match &s.source {
                            TrimSource::Domain { boundary } => boundary.piece.clone(),
                            TrimSource::Tool { piece } => piece.clone(),
                        })
                        .next()
                        .unwrap_or_else(|| {
                            *slot += 1;
                            CurvePieceRef {
                                curve: SheetCurveId::Generated {
                                    feature: ctx.uid,
                                    slot: *slot,
                                },
                                piece: 0,
                            }
                        });
                    edges.push(MaterialEdge {
                        piece,
                        curve: e.curve.clone(),
                    });
                }
                loops.push(MaterialLoop { edges });
            }
            let mut region = MaterialRegion { loops };
            super::super::folded::develop::orient(&mut region)?;
            out.push(region);
        }
    }
    Ok(out)
}
