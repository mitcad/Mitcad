// SPDX-License-Identifier: MIT
//! Perforate (mitcad#461): a field of round holes laid out on the developed
//! sheet and cut through it.
//!
//! Every bend is developed while the stationary panel stays fixed; in that
//! panel's chart the grid starts at the lower left corner of the developed
//! material's box, `margin + diameter/2` in from it, rows along the chart's
//! x axis (`pitch` apart, staggered rows `pitch sqrt(3)/2` apart with every
//! other row shifted by half a pitch). A hole is kept where its centre lies
//! on a panel, on no bend's strip, and `margin + diameter/2` or more from
//! every boundary of the material (its outline, holes and cut-outs) and of
//! every bend's strip. The kept circles are cut as a developed cut with all
//! bends open (`g<k>` the k-th hole), and the input configuration restored.

use super::cut::{developed_cut, found_profiles, sheet_region, sketch_surface_map};
use super::develop::{bend_record, planar_map, reconfigure};
use super::*;
use crate::features::sheet_perforate::{
    PerforationOutline, PerforationPattern, SheetPerforateDef, check_sizes,
};
use std::collections::BTreeMap;

/// The most holes one perforation cuts.
const MAX_HOLES: usize = 100_000;

type Edge = ([f64; 2], [f64; 2]);

/// A loop as a polyline, with its physical boundary chords for clearance.
fn polyline<'a>(
    edges: impl IntoIterator<Item = (&'a BoundedCurve2, bool)>,
    map: Isometry2,
    clearance_edges: &mut Vec<Edge>,
) -> Result<Vec<[f64; 2]>, String> {
    let mut points = Vec::new();
    for (curve, clearance) in edges {
        let (t0, t1) = (curve.range.first, curve.range.last);
        let n = match &curve.geometry {
            SegmentGeometry::Line { .. } => 1,
            SegmentGeometry::Circle { radius, .. } | SegmentGeometry::Arc { radius, .. } => {
                let step = 2.0 * (1.0 - (1e-4 / radius).min(1.0)).acos();
                ((t1 - t0).abs() / step.max(1e-6)).ceil().clamp(8.0, 4096.0) as usize
            }
            _ => 512,
        };
        let start = points.len();
        for k in 0..n {
            points.push(map.point(curve.point(t0 + (t1 - t0) * k as f64 / n as f64)?));
        }
        if clearance {
            for pair in points[start..].windows(2) {
                clearance_edges.push((pair[0], pair[1]));
            }
            clearance_edges.push((points[start + n - 1], map.point(curve.point(t1)?)));
        }
    }
    Ok(points)
}

fn segment_distance(a: [f64; 2], b: [f64; 2], p: [f64; 2]) -> f64 {
    use crate::sketch::geometry as g;
    let d = g::sub(b, a);
    let len = g::dot(d, d);
    let t = if len > 0.0 {
        (g::dot(g::sub(p, a), d) / len).clamp(0.0, 1.0)
    } else {
        0.0
    };
    g::dist(g::add(a, g::scale(d, t)), p)
}

/// A region's loops as polylines, and whether a point lies inside it.
struct Area {
    loops: Vec<Vec<[f64; 2]>>,
}

struct Grid {
    diameter: f64,
    pitch: f64,
    margin: f64,
    pattern: PerforationPattern,
}

impl Area {
    fn contains(&self, p: [f64; 2]) -> bool {
        let mut inside = false;
        for l in &self.loops {
            if crate::sketch::geometry::polygon_contains(l, p) {
                inside = !inside;
            }
        }
        inside
    }
}

/// The hole centres of a field, in the developed chart of the fixed panel.
fn hole_centres(
    panels: &[Area],
    strips: &[Area],
    edges: &[Edge],
    grid: Grid,
    bounded: bool,
) -> Result<Vec<[f64; 2]>, String> {
    let Grid {
        diameter,
        pitch,
        margin,
        pattern,
    } = grid;
    let clear = diameter / 2.0 + margin;
    let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
    let bound_strips = if bounded { &[] } else { strips };
    for p in panels
        .iter()
        .chain(bound_strips)
        .flat_map(|a| &a.loops)
        .flatten()
    {
        for k in 0..2 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    if !(lo[0] <= hi[0] && lo[1] <= hi[1]) {
        return Ok(Vec::new());
    }
    // The edges bucketed in a grid of cells, so each hole checks only the
    // edges near it.
    let cell = (2.0 * clear).max(pitch).max(1e-3);
    let nx = ((hi[0] - lo[0]) / cell) as usize + 1;
    let ny = ((hi[1] - lo[1]) / cell) as usize + 1;
    if nx.saturating_mul(ny) > 16 * MAX_HOLES {
        return Err("the perforation's pitch is too small for the sheet's size".into());
    }
    let index = |p: [f64; 2]| -> (usize, usize) {
        (
            ((p[0] - lo[0]) / cell).floor().clamp(0.0, (nx - 1) as f64) as usize,
            ((p[1] - lo[1]) / cell).floor().clamp(0.0, (ny - 1) as f64) as usize,
        )
    };
    let mut grid: Vec<Vec<usize>> = vec![Vec::new(); nx * ny];
    for (k, (a, b)) in edges.iter().enumerate() {
        let (i0, j0) = index([a[0].min(b[0]) - clear, a[1].min(b[1]) - clear]);
        let (i1, j1) = index([a[0].max(b[0]) + clear, a[1].max(b[1]) + clear]);
        for j in j0..=j1 {
            for i in i0..=i1 {
                grid[j * nx + i].push(k);
            }
        }
    }
    let (dy, shift) = match pattern {
        PerforationPattern::Square => (pitch, 0.0),
        PerforationPattern::Staggered => (pitch * 3f64.sqrt() / 2.0, pitch / 2.0),
    };
    let mut out = Vec::new();
    let mut row = 0usize;
    loop {
        let y = lo[1] + clear + row as f64 * dy;
        if y > hi[1] - clear + 1e-9 {
            break;
        }
        let mut x = lo[0] + clear + if row % 2 == 1 { shift } else { 0.0 };
        while x <= hi[0] - clear + 1e-9 {
            let c = [x, y];
            let (i, j) = index(c);
            if panels.iter().any(|a| a.contains(c))
                && !strips.iter().any(|a| a.contains(c))
                && grid[j * nx + i]
                    .iter()
                    .all(|&k| segment_distance(edges[k].0, edges[k].1, c) >= clear - 1e-9)
            {
                out.push(c);
                if out.len() > MAX_HOLES {
                    return Err(format!(
                        "the perforation would cut more than {MAX_HOLES} holes"
                    ));
                }
            }
            x += pitch;
        }
        row += 1;
    }
    Ok(out)
}

/// Clips the sketch to the material of the panels it lies on before
/// developing it. A profile on one folded face cannot leak onto another
/// face that merely lines up with it after unfolding.
fn outline_in_development<K: Kernel>(
    outline: &PerforationOutline,
    ctx: &mut EvalContext<'_, K>,
    state: &SheetState,
    developed: &SheetState,
    fixed: &SheetEmbedding,
    thickness: f64,
    edges: &mut Vec<Edge>,
) -> Result<Vec<Area>, String> {
    let sketch_uid = outline
        .profiles
        .first()
        .ok_or("select closed sketch profiles for the perforation outline")?
        .sketch;
    let sketch = ctx.sketch(sketch_uid)?;
    let mut regions = Vec::new();
    for profile in &outline.profiles {
        let found = found_profiles(ctx, &sketch, sketch_uid, &profile.region)
            .map_err(|_| "the perforation outline needs closed sketch profiles")?;
        for region in found {
            regions.push(MaterialRegion::from_legacy(&sheet_region(
                sketch_uid, &region,
            ))?);
        }
    }
    let mut areas = Vec::new();
    for patch in &state.charts {
        let SheetDomainRef::Panel(id) = patch.domain else {
            continue;
        };
        let (frame, parity) = plane(patch)?;
        let Some(to_chart) = sketch_surface_map(sketch.frame, frame, parity, thickness) else {
            continue;
        };
        let domains = [TrimDomain {
            domain: patch.domain,
            regions: patch.regions.clone(),
            to_operation: Isometry2::IDENTITY,
        }];
        let tools = mapped_regions(&regions, to_chart);
        let intersection = ctx
            .kernel
            .sheet_trim_domain(&SheetTrimSpec {
                operation: SheetTrimOperation::Intersection,
                domains: &domains,
                tools: &tools,
                seams: &[],
                tolerance: TOLERANCE,
            })
            .map_err(|error| error.to_string())?;
        let to_flat = planar_map(&chart(developed, id)?.embedding, fixed)?;
        for region in intersection.domains.into_iter().flat_map(|d| d.regions) {
            let mut loops = Vec::new();
            for l in region.loops {
                // The outline adds clearance only along its own edges.
                // Existing exterior and bend edges are already included.
                // A coplanar panel seam within the outline adds no gap.
                loops.push(polyline(
                    l.edges.iter().map(|e| {
                        let outline_edge = e
                            .lineage
                            .iter()
                            .any(|span| matches!(&span.source, TrimSource::Tool { .. }));
                        (&e.curve, outline_edge)
                    }),
                    to_flat,
                    edges,
                )?);
            }
            areas.push(Area { loops });
        }
    }
    if areas.is_empty() {
        return Err("the perforation outline must overlap a flat face of the sheet".into());
    }
    Ok(areas)
}

pub(crate) fn perforate<K: Kernel>(
    d: &SheetPerforateDef<f64>,
    ctx: &mut EvalContext<'_, K>,
) -> Result<FeatureOutput<K::Shape>, String> {
    let margin = d.margin.unwrap_or(d.diameter);
    check_sizes(d.diameter, d.pitch, margin)?;
    let (state, rules) = read(ctx, d.body)?;
    super::overlay_refusal(ctx, &state, "Perforate")?;
    let thickness = rules.thickness;
    let stationary = match d.stationary {
        Some(panel) => panel,
        None => state
            .charts
            .iter()
            .filter_map(|p| match p.domain {
                SheetDomainRef::Panel(id) => Some(id),
                SheetDomainRef::Bend(_) => None,
            })
            .min()
            .ok_or("the sheet has no panel")?,
    };
    let bends: Vec<BendId> = state.bends.iter().map(|b| b.id).collect();
    let mut targets = BTreeMap::new();
    for bend in &bends {
        bend_record(&state, *bend, thickness)?;
        targets.insert(*bend, true);
    }
    let mut work = state.clone();
    reconfigure(&mut work, thickness, stationary, &targets)?;
    let fixed = chart(&work, stationary)?.embedding.clone();
    // Seams between two panels of one plane are inside the material.
    let inner_seam = |domain: SheetDomainRef, piece: &CurvePieceRef| {
        work.joins.iter().any(|p| {
            let panels = matches!(p.first.boundary.domain, SheetDomainRef::Panel(_))
                && matches!(p.second.boundary.domain, SheetDomainRef::Panel(_));
            panels
                && [&p.first, &p.second]
                    .iter()
                    .any(|s| s.boundary.domain == domain && &s.boundary.piece == piece)
        })
    };
    let (mut panels, mut strips, mut edges) = (Vec::new(), Vec::new(), Vec::new());
    for patch in &work.charts {
        let map = planar_map(&patch.embedding, &fixed).map_err(|_| {
            format!(
                "{} does not lie in the developed sheet",
                develop::domain_label(patch.domain)
            )
        })?;
        for region in &patch.regions {
            let mut loops = Vec::new();
            for l in &region.loops {
                loops.push(polyline(
                    l.edges
                        .iter()
                        .map(|e| (&e.curve, !inner_seam(patch.domain, &e.piece))),
                    map,
                    &mut edges,
                )?);
            }
            let area = Area { loops };
            match patch.domain {
                SheetDomainRef::Panel(_) => panels.push(area),
                SheetDomainRef::Bend(_) => strips.push(area),
            }
        }
    }
    if let Some(outline) = &d.outline {
        panels =
            outline_in_development(outline, ctx, &state, &work, &fixed, thickness, &mut edges)?;
    }
    let centres = hole_centres(
        &panels,
        &strips,
        &edges,
        Grid {
            diameter: d.diameter,
            pitch: d.pitch,
            margin,
            pattern: d.pattern,
        },
        d.outline.is_some(),
    )?;
    if centres.is_empty() {
        ctx.warn("no hole of the perforation fits on the sheet");
        return publish_pieces(ctx, state, &rules, Some("the perforation"));
    }
    ctx.inform(format!("The perforation cuts {} holes.", centres.len()));
    let tools = centres
        .iter()
        .enumerate()
        .map(|(k, c)| {
            MaterialRegion::from_legacy(&SheetRegion {
                loops: vec![SheetLoop {
                    segments: vec![SheetSegment {
                        id: SheetCurveId::Generated {
                            feature: ctx.uid,
                            slot: k as u32,
                        },
                        geometry: SegmentGeometry::Circle {
                            center: *c,
                            radius: d.diameter / 2.0,
                            start: 0.0,
                        },
                    }],
                }],
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let outcome = developed_cut(
        ctx,
        &state,
        thickness,
        stationary,
        &bends,
        &tools,
        "the perforation",
    )?;
    publish_pieces(ctx, outcome.state, &rules, Some("the perforation"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(min: [f64; 2], max: [f64; 2]) -> (Area, Vec<Edge>) {
        let points = vec![min, [max[0], min[1]], max, [min[0], max[1]]];
        let edges = (0..4).map(|k| (points[k], points[(k + 1) % 4])).collect();
        (
            Area {
                loops: vec![points],
            },
            edges,
        )
    }

    fn grid(pattern: PerforationPattern) -> Grid {
        Grid {
            diameter: 5.0,
            pitch: 10.0,
            margin: 5.0,
            pattern,
        }
    }

    #[test]
    fn grids_start_clear_of_the_box_and_keep_clear_of_edges_and_bends() {
        // 100 x 60, holes 5 every 10, margin 5: centres 7.5 from the edges.
        let (panel, edges) = rect([0.0, 0.0], [100.0, 60.0]);
        let square = hole_centres(
            std::slice::from_ref(&panel),
            &[],
            &edges,
            grid(PerforationPattern::Square),
            false,
        )
        .unwrap();
        assert_eq!(square.len(), 9 * 5);
        assert_eq!(square[0], [7.5, 7.5]);
        let staggered = hole_centres(
            std::slice::from_ref(&panel),
            &[],
            &edges,
            grid(PerforationPattern::Staggered),
            false,
        )
        .unwrap();
        // Six rows 5 sqrt(3) apart, nine holes each (odd rows from 12.5).
        assert_eq!(staggered.len(), 6 * 9);
        assert_eq!(staggered[9], [12.5, 7.5 + 5.0 * 3f64.sqrt()]);
        // A bend strip across x = 45..55 takes the columns within 7.5 of it.
        let (strip, strip_edges) = rect([45.0, 0.0], [55.0, 60.0]);
        let all: Vec<_> = edges.iter().chain(&strip_edges).copied().collect();
        let split = hole_centres(
            std::slice::from_ref(&panel),
            std::slice::from_ref(&strip),
            &all,
            grid(PerforationPattern::Square),
            false,
        )
        .unwrap();
        assert!(
            split
                .iter()
                .all(|c| c[0] <= 37.5 + 1e-9 || c[0] >= 62.5 - 1e-9)
        );
        assert_eq!(split.len(), 7 * 5);
    }

    #[test]
    fn the_outline_anchors_the_grid_and_keeps_its_inner_loop_clear() {
        let (mut outline, mut edges) = rect([20.0, 10.0], [80.0, 60.0]);
        let loop_points: Vec<_> = (0..256)
            .map(|i| {
                let angle = i as f64 * std::f64::consts::TAU / 256.0;
                [47.5 + 3.0 * angle.cos(), 37.5 + 3.0 * angle.sin()]
            })
            .collect();
        edges.extend((0..256).map(|i| (loop_points[i], loop_points[(i + 1) % 256])));
        outline.loops.push(loop_points);
        let (strip, strip_edges) = rect([70.0, 0.0], [75.0, 80.0]);
        edges.extend(strip_edges);
        let centres = hole_centres(
            &[outline],
            &[strip],
            &edges,
            grid(PerforationPattern::Square),
            true,
        )
        .unwrap();
        assert_eq!(centres.first(), Some(&[27.5, 17.5]));
        assert!(centres.iter().all(|p| p[0] <= 62.5));
        assert!(
            centres
                .iter()
                .all(|p| (p[0] - 47.5).hypot(p[1] - 37.5) >= 10.5)
        );
    }
}
