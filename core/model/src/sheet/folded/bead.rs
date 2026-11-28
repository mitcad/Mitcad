// SPDX-License-Identifier: MIT
//! Bead (mitcad#2645): ribs pressed into a flat panel of a sheet along
//! sketch lines and arcs, built on the sheet and kept in its state.
//!
//! The sketch must lie on a panel's front or back surface; its lines and
//! arcs, mapped into the panel's chart, are the center lines
//! ([`crate::sheet::bead::chains`]). A bead is kept clear of the panel's
//! outline, its holes and its bends (they bound the panel's chart) and of
//! other beads by how far it reaches on its face; arcs must be wider than
//! half its width, corners only join two lines, and a run-out end needs a
//! straight line that holds its ramp.
//!
//! **The solid** (`sheet/bead.rs` has the section). Two tools are swept
//! along the center lines in the panel's current place: the region between
//! the face's plane and the face's curve, and the region between the other
//! surface's plane and its curve, each reaching half a thickness past its
//! plane on the side away from the press direction. Lines sweep a prism,
//! arcs turn about their centres, round ends and corners turn the half
//! section about the end or the corner (a corner on its outer side, by the
//! angle the line turns); a run-out's prism is cut down by its ramp (the
//! region under the ramp's curve, swept across). The pieces of each tool
//! are united. A bead standing out joins the face's tool and cuts the
//! other's; one pressed in joins the other's and cuts the face's: in both
//! the material left lies between the two curves, a thickness apart.
//!
//! The faces are named by the feature and section curve: element `k` of the
//! bead (its lines and arcs in order, then its ends and corners) sweeps the
//! face's curve pieces as `c<1000(k+1)+j>` and the other's as
//! `c<1000(k+1)+500+j>` (`j` from 1, the lips first), so a round bead's
//! crown on its first line is `<feature>:side(c1002)` and the surface
//! under it `<feature>:side(c1502)`.
//!
//! The bead is a [`SheetOverlayKind::Bead`] of the sheet, made again on
//! every rebuilt solid where its panel lies then ([`remade`]): Unfold and
//! Refold move it with the panel, and the Flat Pattern draws its outline
//! and center lines (`flat.rs`). A rebuild that no longer leaves the bead
//! clear inside its face, or changes another volume than the bead did, is
//! refused in words.

use std::f64::consts::{PI, TAU};

use super::*;
use crate::features::sheet_bead::{DEFAULT_ANGLE, SheetBeadDef};
use crate::kernel::{Axis, BooleanOp, BooleanOutput, ExtrudeSpec, RevolveSpec};
use crate::profile::{ProfileLoop, ProfileRegion, ProfileSegment};
use crate::sheet::bead::{
    BeadChain, BeadElement, BeadEnd, BeadSection, CORNER_ARCS, Piece, SMOOTH, Section, SheetBead,
    UNMADE, add2, chains, scale2, segment_distance, segments, sub2, turn,
};
use crate::sketch::geometry::Curve2;
use crate::topo::{RegionKey, SegmentKey};
use mitcad_vec3::{add, cross, dot, scale, sub};

/// The bead feature: the bead built on the sheet's current body and kept
/// in its state.
pub(crate) fn bead<K: Kernel>(
    d: &SheetBeadDef<f64>,
    ctx: &mut EvalContext<'_, K>,
) -> Result<FeatureOutput<K::Shape>, String> {
    let (mut state, rules) = read(ctx, d.body)?;
    let t = rules.thickness;
    let values = BeadSection {
        profile: d.profile,
        width: d.width,
        depth: d.depth,
        radius: d.radius.unwrap_or(rules.bend_radius),
        angle: d.angle.unwrap_or(DEFAULT_ANGLE),
        direction: d.direction,
        ends: d.ends,
        run_out: d.run_out.unwrap_or(d.width),
        thickness: t,
    };
    let section = values.section()?;
    let sketch = ctx.sketch(d.sketch)?;
    let mut pieces = Vec::with_capacity(d.curves.len());
    for id in &d.curves {
        let curve = crate::features::sketch::curve_of(&sketch.solved, *id)
            .ok_or("a line or arc of the bead is no longer in its sketch")?;
        pieces.push(match *curve {
            Curve2::Line { a, b } => BeadElement::Line { start: a, end: b },
            Curve2::Arc {
                center,
                radius,
                start,
                end,
            } => BeadElement::Arc {
                center,
                radius,
                start,
                sweep: if end > start {
                    end - start
                } else {
                    (end - start).rem_euclid(TAU)
                },
            },
            Curve2::Circle {
                center,
                radius,
                start,
            } => BeadElement::Arc {
                center,
                radius,
                start,
                sweep: TAU,
            },
            _ => return Err("a bead's center lines are made of sketch lines and arcs".into()),
        });
    }
    let (panel, side, mapped) = on_panel(&state, &sketch.frame, t, &pieces)?;
    let bead = SheetBead {
        panel,
        side,
        chains: chains(&mapped)?,
        section: values,
    };
    check_path(&bead, &section)?;
    check_clear(&state, &bead, &section, None)?;
    let before = ctx.body(d.body)?;
    let (shape, change, _) = pressed(ctx, &state, &bead, &section, ctx.uid, before)?;
    state.overlays.push(SheetOverlay {
        feature: ctx.uid,
        kind: SheetOverlayKind::Bead(bead),
        removed: change,
    });
    let mut output = FeatureOutput::default();
    finish(&mut output, state, &rules, shape)?;
    Ok(output)
}

/// The panel whose front or back surface holds the sketch and whose
/// material holds the middle of its first line or arc: the panel, the
/// surface, and the lines and arcs in its chart.
fn on_panel(
    state: &SheetState,
    sketch: &SketchFrame,
    thickness: f64,
    pieces: &[BeadElement],
) -> Result<(PanelId, SheetReferenceSide, Vec<BeadElement>), String> {
    let first = pieces
        .first()
        .ok_or("a bead needs at least one line or arc of a sketch")?;
    for patch in &state.charts {
        let SheetDomainRef::Panel(panel) = patch.domain else {
            continue;
        };
        let Ok((frame, parity)) = plane(patch) else {
            continue;
        };
        let Some(map) = super::cut::sketch_surface_map(*sketch, frame, parity, thickness) else {
            continue;
        };
        let offset = dot(sub(sketch.origin, frame.origin), frame.normal()) * parity.sign();
        let side = if offset.abs() <= 1e-6 {
            SheetReferenceSide::Front
        } else {
            SheetReferenceSide::Back
        };
        let middle = match mapped(first, map) {
            BeadElement::Line { start, end } => scale2(add2(start, end), 0.5),
            BeadElement::Arc {
                center,
                radius,
                start,
                sweep,
            } => {
                let at = start + sweep / 2.0;
                [center[0] + radius * at.cos(), center[1] + radius * at.sin()]
            }
        };
        if !inside(&patch.regions, middle)? {
            continue;
        }
        return Ok((panel, side, pieces.iter().map(|e| mapped(e, map)).collect()));
    }
    Err(
        "a bead's sketch must lie on a flat face of the sheet, with its lines and arcs on that \
         face"
            .into(),
    )
}

/// A line or arc moved by an isometry of the plane (a reflection runs an
/// arc the other way).
fn mapped(e: &BeadElement, map: Isometry2) -> BeadElement {
    match *e {
        BeadElement::Line { start, end } => BeadElement::Line {
            start: map.point(start),
            end: map.point(end),
        },
        BeadElement::Arc {
            center,
            radius,
            start,
            sweep,
        } => {
            let sign = map.x_axis[0] * map.y_axis[1] - map.x_axis[1] * map.y_axis[0];
            let rotation = map.x_axis[1].atan2(map.x_axis[0]);
            BeadElement::Arc {
                center: map.point(center),
                radius,
                start: rotation + sign.signum() * start,
                sweep: sign.signum() * sweep,
            }
        }
    }
}

/// The center lines' own checks: corners only between lines and not
/// doubling back, arcs wider than the bead's half, run-outs on lines long
/// enough for their ramps, and an outline that closes at every corner.
fn check_path(bead: &SheetBead, section: &Section) -> Result<(), String> {
    for chain in &bead.chains {
        let n = chain.elements.len();
        let joints = if chain.closed { n } else { n - 1 };
        for j in 0..joints {
            let (a, b) = (&chain.elements[j], &chain.elements[(j + 1) % n]);
            let bend = turn(a, b);
            if bend.abs() < SMOOTH {
                continue;
            }
            if !matches!((a, b), (BeadElement::Line { .. }, BeadElement::Line { .. })) {
                return Err(CORNER_ARCS.into());
            }
            if bend.abs() > PI - 1e-3 {
                return Err("a bead's center line doubles back on itself at a corner".into());
            }
        }
        for e in &chain.elements {
            if let BeadElement::Arc { radius, .. } = e
                && *radius <= section.half + 1e-6
            {
                return Err(
                    "an arc of the bead's center line is tighter than half the bead's width: \
                     make the arc larger or the bead narrower"
                        .into(),
                );
            }
        }
        if !chain.closed && bead.section.ends == BeadEnd::RunOut {
            let ramp = section.crown();
            let (first, last) = (&chain.elements[0], &chain.elements[n - 1]);
            let lines = matches!(first, BeadElement::Line { .. })
                && matches!(last, BeadElement::Line { .. });
            let room = if n == 1 { 2.0 * ramp } else { ramp };
            if !lines || first.length() < room - 1e-9 || last.length() < room - 1e-9 {
                return Err(
                    "a bead's run-out ends on a straight line that holds its ramp: make the \
                     lines at its ends longer or the run-out shorter"
                        .into(),
                );
            }
        }
    }
    bead.outline(section).map(|_| ())
}

/// A region's loops as polylines (points along each edge).
fn loops(regions: &[MaterialRegion]) -> Result<Vec<Vec<Vec<[f64; 2]>>>, String> {
    let mut out = Vec::new();
    for region in regions {
        let mut polylines = Vec::new();
        for l in &region.loops {
            let mut points = Vec::new();
            for edge in &l.edges {
                let curve = &edge.curve;
                let (t0, t1) = (curve.range.first, curve.range.last);
                let n = match &curve.geometry {
                    SegmentGeometry::Line { .. } => 1,
                    SegmentGeometry::Circle { radius, .. }
                    | SegmentGeometry::Arc { radius, .. } => {
                        let step = 2.0 * (1.0 - (1e-4 / radius).min(1.0)).acos();
                        ((t1 - t0).abs() / step.max(1e-6)).ceil().clamp(8.0, 4096.0) as usize
                    }
                    _ => 512,
                };
                for k in 0..=n {
                    points.push(curve.point(t0 + (t1 - t0) * k as f64 / n as f64)?);
                }
            }
            polylines.push(points);
        }
        out.push(polylines);
    }
    Ok(out)
}

/// Whether a point lies in the material of a chart's regions.
fn inside(regions: &[MaterialRegion], p: [f64; 2]) -> Result<bool, String> {
    Ok(loops(regions)?.iter().any(|region| {
        region
            .iter()
            .filter(|l| crate::sketch::geometry::polygon_contains(l, p))
            .count()
            % 2
            == 1
    }))
}

/// The bead must lie in its panel's material, clear of the panel's edges
/// (its outline, holes and bends) by how far it reaches on its face, and
/// clear of the panel's other beads (`except`: the bead itself, when it is
/// made again).
fn check_clear(
    state: &SheetState,
    bead: &SheetBead,
    section: &Section,
    except: Option<FeatureUid>,
) -> Result<(), String> {
    let patch = state
        .charts
        .iter()
        .find(|p| p.domain == SheetDomainRef::Panel(bead.panel))
        .ok_or("a bead's flat face is no longer a face of the sheet")?;
    let reach = section.reach();
    let step = (reach / 4.0).clamp(0.05, 1.0);
    let paths = bead.polylines(step);
    let first = paths
        .first()
        .and_then(|p| p.first())
        .copied()
        .ok_or("a bead needs at least one line or arc of a sketch")?;
    let not_clear = "the bead does not fit on its face: it keeps clear of the face's edges, \
                     holes and bends by half its width (and by its run-out's foot at a run-out \
                     end), and lies inside the face";
    if !inside(&patch.regions, first)? {
        return Err(not_clear.into());
    }
    for region in loops(&patch.regions)? {
        for l in &region {
            for (a, b) in segments(l) {
                for path in &paths {
                    for (c, e) in segments(path) {
                        if segment_distance(a, b, c, e) < reach + 1e-6 {
                            return Err(not_clear.into());
                        }
                    }
                }
            }
        }
    }
    // Its own center lines apart, and the other beads of the panel.
    let overlap = "beads overlap: keep the center lines of beads on a face apart by their widths";
    for (i, a) in bead.chains.iter().enumerate() {
        for b in &bead.chains[i + 1..] {
            if chains_closer(a, b, 2.0 * reach, step) {
                return Err(overlap.into());
            }
        }
    }
    for overlay in &state.overlays {
        let SheetOverlayKind::Bead(other) = &overlay.kind else {
            continue;
        };
        if other.panel != bead.panel || Some(overlay.feature) == except {
            continue;
        }
        let Ok(other_section) = other.section.section() else {
            continue;
        };
        let apart = reach + other_section.reach();
        for a in &bead.chains {
            for b in &other.chains {
                if chains_closer(a, b, apart, step) {
                    return Err(overlap.into());
                }
            }
        }
    }
    Ok(())
}

fn chains_closer(a: &BeadChain, b: &BeadChain, apart: f64, step: f64) -> bool {
    let polylines = |c: &BeadChain| -> Vec<Vec<[f64; 2]>> {
        c.elements.iter().map(|e| e.points(step)).collect()
    };
    let (pa, pb) = (polylines(a), polylines(b));
    pa.iter().any(|p| {
        pb.iter().any(|q| {
            segments(p).any(|(a0, a1)| {
                segments(q).any(|(b0, b1)| segment_distance(a0, a1, b0, b1) < apart + 1e-6)
            })
        })
    })
}

/// The bead's place in 3D: the panel's chart on its face, the press
/// direction.
struct Placed {
    frame: SketchFrame,
    /// From the chart's front plane to the bead's face.
    lift: [f64; 3],
    /// The press direction (`z` of the section).
    press: [f64; 3],
}

impl Placed {
    fn new(patch: &SheetPatch, bead: &SheetBead) -> Result<Self, String> {
        let (frame, parity) = plane(patch)?;
        let t = bead.section.thickness;
        let material = scale(frame.normal(), parity.sign());
        let back = bead.side == SheetReferenceSide::Back;
        let lift = if back { scale(material, t) } else { [0.0; 3] };
        let into = bead.section.direction == crate::sheet::bead::BeadDirection::In;
        // Pressed in from the front: towards the back; from the back:
        // towards the front; standing out, the other way.
        let press = scale(material, if into != back { 1.0 } else { -1.0 });
        Ok(Self { frame, lift, press })
    }

    fn point(&self, p: [f64; 2]) -> [f64; 3] {
        add(self.frame.point(p), self.lift)
    }

    fn vector(&self, v: [f64; 2]) -> [f64; 3] {
        add(
            scale(self.frame.x_axis, v[0]),
            scale(self.frame.y_axis, v[1]),
        )
    }
}

fn unit3(v: [f64; 3]) -> Result<[f64; 3], String> {
    mitcad_vec3::unit(v).ok_or_else(|| UNMADE.to_owned())
}

/// A closed region of section curve pieces and the straight lines through
/// `closing` back to the first piece's start, its segments keyed from
/// `base + 1`.
fn region(pieces: &[Piece], closing: &[[f64; 2]], base: u32) -> Result<ProfileRegion, String> {
    let mut geometry: Vec<SegmentGeometry> = pieces.iter().map(Piece::geometry).collect();
    let mut at = pieces.last().ok_or(UNMADE)?.end();
    let first = pieces[0].start();
    for &p in closing.iter().chain(std::iter::once(&first)) {
        geometry.push(SegmentGeometry::Line { start: at, end: p });
        at = p;
    }
    let segments: Vec<ProfileSegment> = geometry
        .into_iter()
        .enumerate()
        .map(|(j, geometry)| {
            let key: SegmentKey = format!("c{}", base + 1 + j as u32)
                .parse()
                .map_err(|_| UNMADE.to_owned())?;
            Ok(ProfileSegment { key, geometry })
        })
        .collect::<Result<_, String>>()?;
    let key = RegionKey::new(segments.iter().map(|s| s.key.clone())).ok_or(UNMADE)?;
    Ok(ProfileRegion {
        key,
        loops: vec![ProfileLoop { segments }],
    })
}

/// The two tools of a bead in its panel's current place: the one joined
/// and the one cut.
fn tools<K: Kernel>(
    ctx: &EvalContext<'_, K>,
    patch: &SheetPatch,
    bead: &SheetBead,
    section: &Section,
    feature: FeatureUid,
) -> Result<(K::Shape, K::Shape), String> {
    let k = ctx.kernel;
    let kernel = |e: crate::kernel::KernelError| e.to_string();
    let placed = Placed::new(patch, bead)?;
    let t = bead.section.thickness;
    let e = t / 2.0;
    let (half, level) = (section.half, section.other_level);
    let face_full = |base| region(&section.face, &[[half, -e], [-half, -e]], base);
    let other_full = |base| {
        region(
            &section.other,
            &[[half, level - e], [-half, level - e]],
            base,
        )
    };
    let face_half = |base| region(&section.face_half, &[[half, -e], [0.0, -e]], base);
    let other_half = |base| {
        region(
            &section.other_half,
            &[[half, level - e], [0.0, level - e]],
            base,
        )
    };
    let mut face_tools = Vec::new();
    let mut other_tools = Vec::new();
    let mut element = 0u32;
    let mut next = || {
        element += 1;
        1000 * element
    };
    let run_out = bead.section.ends == BeadEnd::RunOut;
    let foot = section.foot();
    for chain in &bead.chains {
        let n = chain.elements.len();
        for (i, piece) in chain.elements.iter().enumerate() {
            let base = next();
            let (face, other) = (face_full(base)?, other_full(base + 500)?);
            let tool = |region: &ProfileRegion,
                        start_ext: f64,
                        end_ext: f64|
             -> Result<K::Shape, String> {
                match *piece {
                    BeadElement::Line { start, end } => {
                        let along = unit3(placed.vector(sub2(end, start)))?;
                        let across = cross(placed.press, along);
                        k.extrude(&ExtrudeSpec {
                            feature,
                            frame: SketchFrame {
                                origin: sub(placed.point(start), scale(along, start_ext)),
                                x_axis: across,
                                y_axis: placed.press,
                            },
                            regions: std::slice::from_ref(region),
                            direction: along,
                            start: 0.0,
                            end: piece.length() + start_ext + end_ext,
                        })
                        .map_err(kernel)
                    }
                    BeadElement::Arc {
                        center,
                        radius,
                        start,
                        sweep,
                    } => {
                        let from = [
                            center[0] + radius * start.cos(),
                            center[1] + radius * start.sin(),
                        ];
                        let radial = unit3(placed.vector(sub2(from, center)))?;
                        let along = unit3(placed.vector(piece.start_tangent()))?;
                        k.revolve(&RevolveSpec {
                            feature,
                            frame: SketchFrame {
                                origin: placed.point(from),
                                x_axis: radial,
                                y_axis: placed.press,
                            },
                            regions: std::slice::from_ref(region),
                            axis: Axis {
                                origin: placed.point(center),
                                direction: cross(radial, along),
                            },
                            angle1: sweep.abs().min(TAU),
                            angle2: None,
                            target: None,
                        })
                        .map_err(kernel)
                    }
                }
            };
            // A run-out's line reaches past its end to the ramp's foot.
            let open_end = |at: usize| run_out && !chain.closed && at == i;
            let start_ext = if open_end(0) { foot + e } else { 0.0 };
            let end_ext = if open_end(n - 1) { foot + e } else { 0.0 };
            let mut face_tool = tool(&face, start_ext, end_ext)?;
            let mut other_tool = tool(&other, start_ext, end_ext)?;
            for (end, (at, point, into_bead)) in [
                (0, piece.start(), piece.start_tangent()),
                (n - 1, piece.end(), scale2(piece.end_tangent(), -1.0)),
            ]
            .into_iter()
            .enumerate()
            {
                if !open_end(at) {
                    continue;
                }
                let length = piece.length() + foot + 2.0 * t;
                // The ramps at the two ends of one line are named apart.
                let keys = base + 100 * end as u32;
                let (lf, lo) = ramps(
                    ctx, &placed, section, feature, point, into_bead, length, keys,
                )?;
                face_tool = common(ctx, &face_tool, &lf)?;
                other_tool = common(ctx, &other_tool, &lo)?;
            }
            face_tools.push(face_tool);
            other_tools.push(other_tool);
        }
        // Corners and round ends: the half section turned about the point.
        let mut dish =
            |point: [f64; 2], from: [f64; 3], axis: [f64; 3], angle: f64| -> Result<(), String> {
                let base = next();
                for (region, into) in [
                    (face_half(base)?, &mut face_tools),
                    (other_half(base + 500)?, &mut other_tools),
                ] {
                    into.push(
                        k.revolve(&RevolveSpec {
                            feature,
                            frame: SketchFrame {
                                origin: placed.point(point),
                                x_axis: from,
                                y_axis: placed.press,
                            },
                            regions: std::slice::from_ref(&region),
                            axis: Axis {
                                origin: placed.point(point),
                                direction: axis,
                            },
                            angle1: angle,
                            angle2: None,
                            target: None,
                        })
                        .map_err(kernel)?,
                    );
                }
                Ok(())
            };
        let joints = if chain.closed { n } else { n - 1 };
        for j in 0..joints {
            let (a, b) = (&chain.elements[j], &chain.elements[(j + 1) % n]);
            let bend = turn(a, b);
            if bend.abs() < SMOOTH {
                continue;
            }
            let (t1, t2) = (
                unit3(placed.vector(a.end_tangent()))?,
                unit3(placed.vector(b.start_tangent()))?,
            );
            let left = dot(cross(t1, t2), placed.press).signum();
            // From the end of `a` on the corner's outer side, round to the
            // start of `b`.
            let outer = scale(cross(placed.press, t1), -left);
            dish(a.end(), outer, scale(placed.press, left), bend.abs())?;
        }
        if !chain.closed && !run_out {
            let (first, last) = (&chain.elements[0], &chain.elements[n - 1]);
            for (point, outward) in [
                (last.end(), last.end_tangent()),
                (first.start(), scale2(first.start_tangent(), -1.0)),
            ] {
                let outward = unit3(placed.vector(outward))?;
                let side = cross(placed.press, outward);
                dish(point, side, cross(side, outward), PI)?;
            }
        }
    }
    let unite = |tools: &[K::Shape]| -> Result<K::Shape, String> {
        let refs: Vec<&K::Shape> = tools.iter().collect();
        k.unite(&refs).map_err(kernel)
    };
    let (face, other) = (unite(&face_tools)?, unite(&other_tools)?);
    Ok(match bead.section.direction {
        crate::sheet::bead::BeadDirection::Out => (face, other),
        crate::sheet::bead::BeadDirection::In => (other, face),
    })
}

/// The regions under a run-out's ramp, for the face's tool and the
/// other's, swept across the bead at the end `point` (`into_bead` the
/// center line's direction from it into the bead), `length` along it.
#[allow(clippy::too_many_arguments)]
fn ramps<K: Kernel>(
    ctx: &EvalContext<'_, K>,
    placed: &Placed,
    section: &Section,
    feature: FeatureUid,
    point: [f64; 2],
    into_bead: [f64; 2],
    length: f64,
    base: u32,
) -> Result<(K::Shape, K::Shape), String> {
    let t = section.other_level.abs();
    let x0 = -section.foot();
    let depth = section.ramp.last().ok_or(UNMADE)?.end()[1];
    let level = section.other_level;
    let far = length.max(section.crown() + t);
    let face = region(
        &section.ramp,
        &[[far, depth], [far, -t], [x0 - t, -t], [x0 - t, 0.0]],
        base + 200,
    )?;
    let other = region(
        &section.ramp_other,
        &[
            [far, depth + level],
            [far, level - t],
            [x0 - t, level - t],
            [x0 - t, level],
        ],
        base + 700,
    )?;
    let along = unit3(placed.vector(into_bead))?;
    let across = cross(along, placed.press);
    let wide = section.half + t;
    let mut out = Vec::new();
    for region in [face, other] {
        out.push(
            ctx.kernel
                .extrude(&ExtrudeSpec {
                    feature,
                    frame: SketchFrame {
                        origin: placed.point(point),
                        x_axis: along,
                        y_axis: placed.press,
                    },
                    regions: std::slice::from_ref(&region),
                    direction: across,
                    start: -wide,
                    end: wide,
                })
                .map_err(|e| e.to_string())?,
        );
    }
    let other = out.pop().expect("two ramps");
    let face = out.pop().expect("two ramps");
    Ok((face, other))
}

/// The common part of a tool and a ramp.
fn common<K: Kernel>(
    ctx: &EvalContext<'_, K>,
    tool: &K::Shape,
    ramp: &K::Shape,
) -> Result<K::Shape, String> {
    let result = ctx
        .kernel
        .boolean(BooleanOp::Intersect, &[tool], ramp)
        .map_err(|e| e.to_string())?;
    single(result, "the bead's run-out could not be made")
}

/// The one solid a boolean leaves of one target.
fn single<S>(result: BooleanOutput<S>, failed: &str) -> Result<S, String> {
    if !result.touched.first().copied().unwrap_or(false) {
        return Err(failed.into());
    }
    let mut pieces = result.pieces.into_iter();
    match (pieces.next(), pieces.next()) {
        (Some(piece), None) if !piece.sources.is_empty() => Ok(piece.shape),
        _ => Err(failed.into()),
    }
}

/// The bead pressed into `shape` (the sheet in its current place): the new
/// solid, the volume it removed less the volume it added, and the volume
/// of `shape`.
fn pressed<K: Kernel>(
    ctx: &EvalContext<'_, K>,
    state: &SheetState,
    bead: &SheetBead,
    section: &Section,
    feature: FeatureUid,
    shape: K::Shape,
) -> Result<(K::Shape, f64, f64), String> {
    let patch = state
        .charts
        .iter()
        .find(|p| p.domain == SheetDomainRef::Panel(bead.panel))
        .ok_or("a bead's flat face is no longer a face of the sheet")?;
    let (join, cut) = tools(ctx, patch, bead, section, feature)?;
    let missed = "the bead does not meet its face: the sheet is not where its sketch is";
    let joined = single(
        ctx.kernel
            .boolean(BooleanOp::Join, &[&shape], &join)
            .map_err(|e| e.to_string())?,
        missed,
    )?;
    let result = single(
        ctx.kernel
            .boolean(BooleanOp::Cut, &[&joined], &cut)
            .map_err(|e| e.to_string())?,
        "the bead would cut the sheet apart",
    )?;
    let volume = |s: &K::Shape| -> Result<f64, String> {
        Ok(ctx
            .kernel
            .mass_properties(s)
            .map_err(|e| e.to_string())?
            .volume)
    };
    let before = volume(&shape)?;
    let change = before - volume(&result)?;
    Ok((result, change, before))
}

/// A bead of the sheet made again on a rebuilt solid (the sheet's
/// [`super::cut::with_overlays`]): in its panel's place now, still clear
/// inside its face, changing the volume it changed when it was made.
pub(crate) fn remade<K: Kernel>(
    ctx: &EvalContext<'_, K>,
    state: &SheetState,
    overlay: &SheetOverlay,
    bead: &SheetBead,
    shape: K::Shape,
) -> Result<K::Shape, String> {
    let section = bead.section.section()?;
    let moved = "a later change reaches a bead of the sheet: keep cuts, holes and other \
                 changes of the face clear of its beads";
    check_clear(state, bead, &section, Some(overlay.feature)).map_err(|_| moved.to_owned())?;
    let (result, change, before) = pressed(ctx, state, bead, &section, overlay.feature, shape)?;
    // The volumes are integrals of the whole body: within a millionth of it.
    let scale = before.abs().max(overlay.removed.abs()).max(1.0);
    if (change - overlay.removed).abs() > 1e-6 * scale {
        return Err(moved.into());
    }
    Ok(result)
}
