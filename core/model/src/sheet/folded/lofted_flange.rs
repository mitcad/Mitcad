// SPDX-License-Identifier: MIT
//! Flange and Hem on a Lofted Flange's sheet (mitcad#2575): on a straight
//! edge of its chart that lies on one flat facet.
//!
//! - **The facet.** The pair of rulings whose chart outline holds the
//!   edge, flat (its four corners in one plane, both triangles mapped into
//!   the chart by one frame). The lofted chart is laid in that facet's
//!   plane (a planar panel whose front is the facet's front), and Flange
//!   and Hem run on that as on any panel: their setback and reliefs trim
//!   the chart, their bends and walls are added with their ports.
//! - **The body.** Material the flange took from the chart (a setback,
//!   reliefs) must lie on the facet: it is cut through the thickness at
//!   right angles to it. The bends and walls are built by the sheet
//!   construction on a strip of the facet along the edge (the strip within
//!   the facet, the thickness wide), and what the body does not hold of
//!   them is joined to it: the bends and walls, and where the transition's
//!   wall along its profile slants off the square of the edge (the mitres
//!   at the folds set its back edge off), the material between that wall
//!   and the bend. They may overlap the body within the strip only: a wall
//!   running into the transition is refused in words.
//! - **The sheet.** The chart keeps its lofted embedding (the trimmed
//!   development); the bends and walls are charts of their own, joined to
//!   the lofted chart's edge. Flat Pattern lays each flange flat beside the
//!   facet it stands on ([`developed`](super::developed)).
//!
//! A flange on several edges, along a run of edges, with a half open
//! corner, or on a flange of the lofted flange is not taken yet (refused
//! in words).

use mitcad_vec3::{dot, sub};

use super::*;
use crate::features::sheet::SheetRulesDef;
use crate::features::sheet_folded::SheetFlangeDef;
use crate::sheet::BoundedCurve2Ext;

/// A lofted flange's sheet as the planar panel a flange runs on.
pub(crate) struct FlangeBase {
    /// The sheet with the lofted chart laid in the facet's plane.
    pub proxy: SheetState,
    lofted: LoftedEmbedding,
    panel: PanelId,
    /// The facet's front, mapping chart points onto it.
    facet: SketchFrame,
    /// The material direction of the facet.
    material: V3,
    /// The facet's outline in the chart, anticlockwise.
    outline: Vec<[f64; 2]>,
}

/// How far (mm) a chart point may lie outside the facet's outline.
const ON_FACET: f64 = 1e-6;

/// The sheet of a flange on a lofted flange's body (see the module
/// documentation) and its rules, None for another sheet.
pub(crate) fn flange_base<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    d: &SheetFlangeDef<f64>,
) -> Result<Option<(FlangeBase, SheetRulesDef<f64>)>, String> {
    if !super::is_lofted(ctx, d.body) {
        return Ok(None);
    }
    let (state, rules) = super::super::read_any(ctx, d.body)?;
    let panel = lofted_panel(&state).ok_or("the sheet has no lofted flange's chart")?;
    if d.boundary.panel != panel {
        return Err(
            "a flange on a lofted flange's sheet goes on an edge of its transition: flanges on its \
             flanges are not taken yet"
                .into(),
        );
    }
    let index = state
        .charts
        .iter()
        .position(|p| p.domain == SheetDomainRef::Panel(panel))
        .ok_or("the sheet has no lofted flange's chart")?;
    let SheetEmbedding::Lofted(l) = &state.charts[index].embedding else {
        return Err("the sheet has no lofted flange's chart".into());
    };
    let l = l.clone();
    let edge = super::super::boundary(&state, &d.boundary)?;
    let (a, b) = super::super::line(edge).map_err(|_| {
        "a flange on a lofted flange's sheet goes on a straight edge of a flat facet".to_owned()
    })?;
    let (facet, material, outline) = facet_of(&l, a, b).ok_or(
        "the edge does not lie on one flat facet of the lofted flange: a flange goes on a \
         straight edge of a flat facet",
    )?;
    let parity = if dot(facet.normal(), material) > 0.0 {
        MaterialParity::Positive
    } else {
        MaterialParity::Negative
    };
    let mut proxy = state.clone();
    proxy.charts[index].embedding = SheetEmbedding::Plane {
        front_frame: facet,
        front_to_back_sign: parity,
    };
    Ok(Some((
        FlangeBase {
            proxy,
            lofted: l,
            panel,
            facet,
            material,
            outline,
        },
        rules,
    )))
}

/// Whether a chart point lies in a convex outline (anticlockwise), within
/// `slack`.
fn in_outline(outline: &[[f64; 2]], p: [f64; 2], slack: f64) -> bool {
    outline
        .iter()
        .zip(outline.iter().cycle().skip(1))
        .all(|(a, b)| {
            let d = [b[0] - a[0], b[1] - a[1]];
            let length = d[0].hypot(d[1]);
            length <= 1e-12 || (d[0] * (p[1] - a[1]) - d[1] * (p[0] - a[0])) / length >= -slack
        })
}

/// The flat facet holding the chart segment `a b`: its front as a frame
/// mapping the chart onto it, its material direction, and its outline in
/// the chart (anticlockwise). None where no one flat facet holds it.
pub(super) fn facet_of(
    l: &LoftedEmbedding,
    a: [f64; 2],
    b: [f64; 2],
) -> Option<(SketchFrame, V3, Vec<[f64; 2]>)> {
    let middle = [0.5 * (a[0] + b[0]), 0.5 * (a[1] + b[1])];
    for pair in l.rulings.windows(2) {
        let (r, s) = (&pair[0], &pair[1]);
        let [r0, r1, s0, s1] = super::pair_at(l, r, s, 0.0);
        // The outline: corners that coincide are one.
        let mut outline: Vec<[f64; 2]> = Vec::new();
        for c in [r.chart[0], s.chart[0], s.chart[1], r.chart[1]] {
            if outline
                .last()
                .is_none_or(|q: &[f64; 2]| (q[0] - c[0]).hypot(q[1] - c[1]) > 1e-9)
            {
                outline.push(c);
            }
        }
        if outline.len() > 3 {
            let (f, l2) = (outline[0], outline[outline.len() - 1]);
            if (f[0] - l2[0]).hypot(f[1] - l2[1]) <= 1e-9 {
                outline.pop();
            }
        }
        if outline.len() < 3 {
            continue;
        }
        let area: f64 = outline
            .iter()
            .zip(outline.iter().cycle().skip(1))
            .map(|(p, q)| p[0] * q[1] - q[0] * p[1])
            .sum();
        if area < 0.0 {
            outline.reverse();
        }
        if ![a, b, middle]
            .iter()
            .all(|p| in_outline(&outline, *p, ON_FACET))
        {
            continue;
        }
        // Flat: one frame maps all four corners into the chart.
        let triangles = [
            ([r0, r1, s0], [r.chart[0], r.chart[1], s.chart[0]]),
            ([r1, s1, s0], [r.chart[1], s.chart[1], s.chart[0]]),
        ];
        let Some((frame, normal)) = triangles
            .iter()
            .filter_map(|(m, c)| super::triangle_frame(*m, *c, r.front_to_back))
            .next()
        else {
            continue;
        };
        let flat = [
            (r0, r.chart[0]),
            (r1, r.chart[1]),
            (s0, s.chart[0]),
            (s1, s.chart[1]),
        ]
        .iter()
        .all(|(m, c)| {
            let d = sub(*m, frame.origin);
            let at = [dot(d, frame.x_axis), dot(d, frame.y_axis)];
            dot(d, normal).abs() <= 1e-7 && (at[0] - c[0]).hypot(at[1] - c[1]) <= 1e-7
        });
        if !flat {
            return None;
        }
        return Some((frame, normal, outline));
    }
    None
}

/// The convex polygon `subject` clipped to the convex `clip` (both
/// anticlockwise).
fn clipped(subject: Vec<[f64; 2]>, clip: &[[f64; 2]]) -> Vec<[f64; 2]> {
    let mut out = subject;
    for (a, b) in clip.iter().zip(clip.iter().cycle().skip(1)) {
        let d = [b[0] - a[0], b[1] - a[1]];
        let length = d[0].hypot(d[1]);
        if length <= 1e-12 || out.is_empty() {
            continue;
        }
        let side = |p: [f64; 2]| (d[0] * (p[1] - a[1]) - d[1] * (p[0] - a[0])) / length;
        let input = std::mem::take(&mut out);
        for (k, p) in input.iter().enumerate() {
            let q = input[(k + 1) % input.len()];
            let (sp, sq) = (side(*p), side(q));
            if sp >= -1e-9 {
                out.push(*p);
            }
            if (sp >= -1e-9) != (sq >= -1e-9) && (sp - sq).abs() > 1e-15 {
                let t = sp / (sp - sq);
                out.push([p[0] + t * (q[0] - p[0]), p[1] + t * (q[1] - p[1])]);
            }
        }
    }
    // Points that coincide are one.
    let mut points: Vec<[f64; 2]> = Vec::new();
    for p in out {
        if points
            .last()
            .is_none_or(|q| (q[0] - p[0]).hypot(q[1] - p[1]) > 1e-9)
        {
            points.push(p);
        }
    }
    while points.len() > 1 {
        let (f, l) = (points[0], points[points.len() - 1]);
        if (f[0] - l[0]).hypot(f[1] - l[1]) > 1e-9 {
            break;
        }
        points.pop();
    }
    points
}

/// The strip of the facet along the flange's edge (`edge`, `a b` in the
/// chart): the thickness wide into the facet, within it; the edge keeps
/// its piece, the other sides are generated curves of the feature.
fn strip(
    feature: crate::ids::FeatureUid,
    edge: &MaterialEdge,
    (a, b): ([f64; 2], [f64; 2]),
    outline: &[[f64; 2]],
    width: f64,
) -> Result<MaterialRegion, String> {
    let d = [b[0] - a[0], b[1] - a[1]];
    let length = d[0].hypot(d[1]);
    let centre = outline.iter().fold([0.0, 0.0], |acc, p| {
        [
            acc[0] + p[0] / outline.len() as f64,
            acc[1] + p[1] / outline.len() as f64,
        ]
    });
    let mut n = [-d[1] / length, d[0] / length];
    if n[0] * (centre[0] - a[0]) + n[1] * (centre[1] - a[1]) < 0.0 {
        n = [-n[0], -n[1]];
    }
    let mut band = vec![
        a,
        b,
        [b[0] + width * n[0], b[1] + width * n[1]],
        [a[0] + width * n[0], a[1] + width * n[1]],
    ];
    let area: f64 = band
        .iter()
        .zip(band.iter().cycle().skip(1))
        .map(|(p, q)| p[0] * q[1] - q[0] * p[1])
        .sum();
    if area < 0.0 {
        band.reverse();
    }
    let points = clipped(band, outline);
    if points.len() < 3 {
        return Err("the facet leaves no room for the flange's edge".into());
    }
    let is = |p: [f64; 2], q: [f64; 2]| (p[0] - q[0]).hypot(p[1] - q[1]) <= 1e-7;
    let mut edges = Vec::new();
    let mut found = false;
    for (k, p) in points.iter().enumerate() {
        let q = points[(k + 1) % points.len()];
        if (is(*p, a) && is(q, b)) || (is(*p, b) && is(q, a)) {
            edges.push(edge.clone());
            found = true;
            continue;
        }
        edges.push(MaterialEdge {
            piece: super::super::generated(feature, 1_500_000 + k as u32),
            curve: BoundedCurve2::from_geometry(SegmentGeometry::Line { start: *p, end: q })?,
        });
    }
    if !found {
        return Err("the flange's edge is not whole on its facet".into());
    }
    let mut region = MaterialRegion {
        loops: vec![MaterialLoop { edges }],
    };
    super::super::develop::orient(&mut region)?;
    Ok(region)
}

/// Regions of the chart made a prism on the facet: from `start` to `end`
/// along its material direction from its front.
fn prism<K: Kernel>(
    ctx: &EvalContext<'_, K>,
    base: &FlangeBase,
    regions: &[MaterialRegion],
    (start, end): (f64, f64),
) -> Result<K::Shape, String> {
    let mut profiles = Vec::new();
    for region in regions {
        let mut loops = Vec::new();
        let mut key = 0;
        for l in &region.loops {
            let mut segments = Vec::new();
            for e in &l.edges {
                key += 1;
                segments.push(crate::profile::ProfileSegment {
                    key: format!("c{key}").parse().expect("segment"),
                    geometry: e.curve.geometry_piece()?,
                });
            }
            loops.push(crate::profile::ProfileLoop { segments });
        }
        profiles.push(crate::profile::ProfileRegion {
            key: "r{c1}".parse().expect("region"),
            loops,
        });
    }
    ctx.kernel
        .extrude(&crate::kernel::ExtrudeSpec {
            feature: ctx.uid,
            frame: base.facet,
            regions: &profiles,
            direction: base.material,
            start,
            end,
        })
        .map_err(|e| e.to_string())
}

fn volume<K: Kernel>(ctx: &EvalContext<'_, K>, shape: &K::Shape) -> Result<f64, String> {
    Ok(ctx
        .kernel
        .mass_properties(shape)
        .map_err(|e| e.to_string())?
        .volume)
}

/// One solid of a boolean, or `refused`.
fn one<K: Kernel>(
    result: crate::kernel::BooleanOutput<K::Shape>,
    refused: &str,
) -> Result<K::Shape, String> {
    let pieces: Vec<_> = result
        .pieces
        .into_iter()
        .filter(|p| !p.sources.is_empty())
        .collect();
    match pieces.as_slice() {
        [piece] => Ok(piece.shape.clone()),
        _ => Err(refused.to_owned()),
    }
}

/// The flange made on the lofted chart laid in the facet's plane
/// (`after`, from `base.proxy`) published on the lofted flange's body (see
/// the module documentation).
pub(crate) fn published_flange<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    base: FlangeBase,
    mut after: SheetState,
    rules: &SheetRulesDef<f64>,
) -> Result<FeatureOutput<K::Shape>, String> {
    let t = rules.thickness;
    let domain = SheetDomainRef::Panel(base.panel);
    let before = super::super::chart(&base.proxy, base.panel)?
        .regions
        .clone();
    let index = after
        .charts
        .iter()
        .position(|p| p.domain == domain)
        .ok_or("the flange takes all of the lofted flange's sheet")?;
    let regions = after.charts[index].regions.clone();
    let mut body = ctx.body(after.body)?;
    let size = volume(ctx, &body)?.abs().max(1.0);
    // Material the flange took from the facet: through the thickness.
    if before != regions {
        let result = ctx
            .kernel
            .sheet_trim_domain(&SheetTrimSpec {
                operation: SheetTrimOperation::Difference,
                domains: &[TrimDomain {
                    domain,
                    regions: before,
                    to_operation: Isometry2::IDENTITY,
                }],
                tools: &regions,
                seams: &[],
                tolerance: TOLERANCE,
            })
            .map_err(|e| e.to_string())?;
        let mut removed = Vec::new();
        let mut slot = 1_600_000;
        for output in result.domains {
            for r in output.regions {
                let mut region = MaterialRegion {
                    loops: r
                        .loops
                        .iter()
                        .map(|l| MaterialLoop {
                            edges: l
                                .edges
                                .iter()
                                .map(|e| {
                                    slot += 1;
                                    MaterialEdge {
                                        piece: super::super::generated(ctx.uid, slot),
                                        curve: e.curve.clone(),
                                    }
                                })
                                .collect(),
                        })
                        .collect(),
                };
                super::super::develop::orient(&mut region)?;
                if super::super::develop::region_area(&region)? > 1e-9 {
                    removed.push(region);
                }
            }
        }
        if !removed.is_empty() {
            for region in &removed {
                for e in region.loops.iter().flat_map(|l| &l.edges) {
                    let (first, last) = (e.curve.range.first, e.curve.range.last);
                    for k in 0..=8 {
                        let p = e.curve.point(first + (last - first) * f64::from(k) / 8.0)?;
                        if !in_outline(&base.outline, p, ON_FACET) {
                            return Err(
                                "the flange's setback or reliefs reach past the folds beside its \
                                 edge: on a lofted flange's sheet a flange takes material from \
                                 its own facet only; place it at the edge, or give it offsets"
                                    .into(),
                            );
                        }
                    }
                }
            }
            let over = (0.01 * t).min(0.01);
            let tool = prism(ctx, &base, &removed, (-over, t + over))?;
            let cut = ctx
                .kernel
                .boolean(crate::kernel::BooleanOp::Cut, &[&body], &tool)
                .map_err(|e| e.to_string())?;
            body = one::<K>(
                cut,
                "the flange's setback parts the lofted flange's sheet in pieces",
            )?;
        }
    }
    // The flange's bends and walls, and their ports.
    let old: std::collections::BTreeSet<SheetDomainRef> =
        base.proxy.charts.iter().map(|p| p.domain).collect();
    let added: Vec<SheetPatch> = after
        .charts
        .iter()
        .filter(|p| !old.contains(&p.domain))
        .cloned()
        .collect();
    if added.is_empty() {
        return Err("the flange adds no material to the lofted flange's sheet".into());
    }
    let new: std::collections::BTreeSet<SheetDomainRef> = added.iter().map(|p| p.domain).collect();
    let ports: Vec<ShapePort> = after
        .joins
        .iter()
        .filter(|p| {
            new.contains(&p.first.boundary.domain) || new.contains(&p.second.boundary.domain)
        })
        .cloned()
        .collect();
    let mut on_lofted = Vec::new();
    for port in &ports {
        for side in [&port.first, &port.second] {
            if side.boundary.domain == domain {
                on_lofted.push(side.boundary.piece.clone());
            } else if !new.contains(&side.boundary.domain) {
                return Err(
                    "the flange meets other walls of the sheet: on a lofted flange's sheet a \
                     flange stands on its own edge"
                        .into(),
                );
            }
        }
    }
    let [piece] = on_lofted.as_slice() else {
        return Err("a flange on a lofted flange's sheet stands on one edge".into());
    };
    let edge = regions
        .iter()
        .flat_map(|r| &r.loops)
        .flat_map(|l| &l.edges)
        .find(|e| &e.piece == piece)
        .ok_or("the flange's edge is not on the lofted flange's sheet")?;
    let ends = super::super::line(edge)?;
    let strip = strip(ctx.uid, edge, ends, &base.outline, t)?;
    let mut patches = vec![SheetPatch {
        domain,
        regions: vec![strip.clone()],
        embedding: super::super::chart(&base.proxy, base.panel)?
            .embedding
            .clone(),
    }];
    patches.extend(added);
    let geometry = ctx
        .kernel
        .sheet_shape(
            &SheetShapeSpec {
                thickness: t,
                patches: &patches,
                ports: &ports,
                tolerance: TOLERANCE,
            },
            &ctx.uid.to_string(),
        )
        .map_err(|e| super::super::name_walls(ctx, &e.to_string()))?;
    let [solid] = geometry.solids.as_slice() else {
        return Err(format!(
            "the flange's bends and walls make {} solids",
            geometry.solids.len()
        ));
    };
    let built = solid.shape.clone();
    // What the body does not hold of them: the bends and walls, and where
    // the transition's wall along the profile slants off the square of the
    // facet's edge (its mitres at the folds set its back edge off), the
    // material between that wall and the bend's start. They overlap the
    // body within the strip only.
    let cut = ctx
        .kernel
        .boolean(crate::kernel::BooleanOp::Cut, &[&built], &body)
        .map_err(|e| e.to_string())?;
    let mut pieces = Vec::new();
    for piece in cut.pieces {
        if volume(ctx, &piece.shape)? > 1e-6 {
            pieces.push(piece.shape);
        }
    }
    let [walls] = <[K::Shape; 1]>::try_from(pieces).map_err(|_| {
        "the flange's bend or wall runs into the lofted flange beside its edge: shorten it, give \
         it offsets, or fold it the other way"
            .to_owned()
    })?;
    let strip_volume = super::super::develop::region_area(&strip)? * t;
    let (v_built, v_walls) = (volume(ctx, &built)?, volume(ctx, &walls)?);
    let common = v_built - v_walls;
    let slack = 1e-6 * v_built.abs().max(1.0);
    if common < -slack || common > strip_volume + slack {
        return Err(
            "the flange's bend or wall runs into the lofted flange beside its edge: shorten it, \
             give it offsets, or fold it the other way"
                .into(),
        );
    }
    // Joined at the edge.
    let v_body = volume(ctx, &body)?;
    let joined = ctx
        .kernel
        .boolean(crate::kernel::BooleanOp::Join, &[&body], &walls)
        .map_err(|e| e.to_string())?;
    let joined = one::<K>(
        joined,
        "the flange's bend does not meet the lofted flange at its edge",
    )?;
    let v_joined = volume(ctx, &joined)?;
    if (v_joined - v_body - v_walls).abs() > 1e-6 * size.max(v_walls.abs()) {
        return Err(
            "the flange's bend or wall runs into the lofted flange beside its edge: shorten it, \
             give it offsets, or fold it the other way"
                .into(),
        );
    }
    for diagnostic in geometry.diagnostics.clone() {
        ctx.warn(diagnostic);
    }
    // The lofted chart back in place.
    after.charts[index].embedding = SheetEmbedding::Lofted(base.lofted);
    let mut output = FeatureOutput::default();
    super::super::finish(&mut output, after, rules, joined)?;
    Ok(output)
}

#[cfg(test)]
#[path = "lofted_flange_tests.rs"]
mod tests;
