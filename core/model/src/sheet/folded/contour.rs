// SPDX-License-Identifier: MIT
//! Contour Flange (mitcad#461): panels along an open chain of sketch lines
//! joined by bends at its corners.
//!
//! The chain is the sheet's surface on the rules' reference side (its front
//! for `front`, its back for `back`); the material lies on the chosen side
//! of it. Each corner is the virtual sharp of a bend on that surface: a
//! corner turning towards the material has the surface outside the bend,
//! so the panels on both sides are set back from the corner by the outer
//! setback `(r+t) tan(a/2)`; one turning away has it inside, set back by
//! `r tan(a/2)` (`a` the turn between the two lines). Panel `i` (slot `i`)
//! is the rectangle of line `i`'s length less its setbacks by the width,
//! its curves `g<50+100i>` to `g<53+100i>` (`53` at the bend before it,
//! `51` at the bend after it); bend `i` (slot `i`) joins panels `i` and
//! `i+1` and is built as a flange's bend is, from the tangent line at the
//! end of panel `i`.

use super::bends::{BendMetrics, add_bend_strip, construction, port_frame, side};
use super::*;
use crate::features::sheet_contour::{ContourSide, SheetContourDef, rules_of};
use crate::sketch::geometry as g2;
use mitcad_vec3::cross;

fn world(frame: &SketchFrame, v: [f64; 2]) -> [f64; 3] {
    std::array::from_fn(|i| v[0] * frame.x_axis[i] + v[1] * frame.y_axis[i])
}

/// The chain's corner points in sketch coordinates, in the order it runs.
fn chain(
    solved: &crate::sketch::solve::Solved,
    curves: &[crate::ids::EntityUid],
) -> Result<Vec<[f64; 2]>, String> {
    let mut lines = Vec::new();
    for id in curves {
        match crate::features::sketch::curve_of(solved, *id) {
            Some(g2::Curve2::Line { a, b }) => lines.push((*a, *b)),
            Some(_) => {
                return Err(format!(
                    "{} is not a line: a contour flange's chain is made of lines (its \
                     corners become the bends)",
                    id.curve_name()
                ));
            }
            None => return Err(format!("sketch curve {} is unavailable", id.curve_name())),
        }
    }
    let near = |p: [f64; 2], q: [f64; 2]| g2::dist(p, q) <= 1e-6;
    let mut points = Vec::new();
    let (a, b) = lines[0];
    // The first line runs away from the second.
    let first = match lines.get(1) {
        Some(&(c, d)) if near(a, c) || near(a, d) => (b, a),
        _ => (a, b),
    };
    points.push(first.0);
    points.push(first.1);
    for (i, &(c, d)) in lines.iter().enumerate().skip(1) {
        let end = *points.last().expect("two points");
        let next = if near(c, end) {
            d
        } else if near(d, end) {
            c
        } else {
            return Err(format!(
                "the contour's lines are not joined end to end at {}",
                curves[i].curve_name()
            ));
        };
        points.push(next);
    }
    Ok(points)
}

pub(crate) fn contour<K: Kernel>(
    d: &SheetContourDef<f64>,
    ctx: &mut EvalContext<'_, K>,
) -> Result<FeatureOutput<K::Shape>, String> {
    let (rules, rules_version) = rules_of(ctx, d.rules)?;
    let sketch = ctx.sketch(d.sketch)?;
    if d.curves.is_empty() {
        return Err("a contour flange needs at least one sketch line".into());
    }
    let width = d.width;
    if !width.is_finite() || width <= TOLERANCE {
        return Err("the contour flange width must be positive".into());
    }
    let points = chain(&sketch.solved, &d.curves)?;
    let t = rules.thickness;
    let n = points.len() - 1;
    let mut dirs = Vec::with_capacity(n);
    let mut lengths = Vec::with_capacity(n);
    for i in 0..n {
        let length = g2::dist(points[i], points[i + 1]);
        if length <= TOLERANCE {
            return Err(format!("contour line {} has no length", i + 1));
        }
        dirs.push(g2::unit(g2::sub(points[i + 1], points[i])));
        lengths.push(length);
    }
    // Material to the right of travel (seen from the sketch's front), or
    // to the left.
    let hand = match d.side {
        ContourSide::Right => 1.0,
        ContourSide::Left => -1.0,
    };
    let front_on_chain = rules.reference_side == SheetReferenceSide::Front;
    let mut metrics = Vec::with_capacity(n.saturating_sub(1));
    let mut setbacks = Vec::with_capacity(n.saturating_sub(1));
    for i in 0..n.saturating_sub(1) {
        let (a, b) = (dirs[i], dirs[i + 1]);
        let turn = g2::dot(a, b).clamp(-1.0, 1.0).acos();
        if turn <= 1e-9 {
            return Err(format!(
                "contour lines {} and {} run on in one line: there is no bend between them",
                i + 1,
                i + 2
            ));
        }
        // Turning towards the material: the chain is the bend's outside.
        let up = g2::cross(a, b) * hand < 0.0;
        let towards_back = up == front_on_chain;
        let angle = if towards_back { turn } else { -turn };
        let m = BendMetrics::new(angle, d.radius, &rules)?;
        let setback = if up {
            m.setback(m.outer, "outer")?
        } else {
            m.setback(m.inner, "inner")?
        };
        metrics.push(m);
        setbacks.push(setback);
    }
    let mut straight = Vec::with_capacity(n);
    for i in 0..n {
        let before = if i > 0 { setbacks[i - 1] } else { 0.0 };
        let after = setbacks.get(i).copied().unwrap_or(0.0);
        let length = lengths[i] - before - after;
        if length <= TOLERANCE {
            return Err(format!(
                "contour line {} ({:.4} mm) is not longer than the setbacks of its bends \
                 ({:.4} mm)",
                i + 1,
                lengths[i],
                before + after
            ));
        }
        straight.push(length);
    }
    // The first panel: x along the first line, y along the width, the chain
    // on its reference-side surface.
    let frame = sketch.frame;
    let x = world(&frame, dirs[0]);
    let material = world(&frame, g2::scale([dirs[0][1], -dirs[0][0]], hand));
    let normal = frame.normal();
    let y = if d.reverse {
        normal.map(|v| -v)
    } else {
        normal
    };
    let start = frame.point(points[0]);
    let front_to_back = if front_on_chain {
        material
    } else {
        material.map(|v| -v)
    };
    let origin = if front_on_chain {
        start
    } else {
        std::array::from_fn(|i| start[i] + t * material[i])
    };
    let parity = if mitcad_vec3::dot(cross(x, y), front_to_back) > 0.0 {
        MaterialParity::Positive
    } else {
        MaterialParity::Negative
    };
    let uid = ctx.uid;
    let owner = ctx
        .env
        .features
        .iter()
        .find(|f| f.uid == uid)
        .ok_or("the contour flange is not in the timeline")?
        .component;
    let mut state = SheetState {
        schema: 2,
        algorithm: ALGORITHM_VERSION,
        body: BodyUid::new(uid, 0),
        owner,
        shape_generation: String::new(),
        rules: d.rules,
        rules_version,
        panels: Vec::new(),
        charts: vec![SheetPatch {
            domain: SheetDomainRef::Panel(PanelId {
                feature: uid,
                slot: 0,
            }),
            regions: vec![rectangle(uid, 50, [0.0, 0.0], [straight[0], width])?],
            embedding: SheetEmbedding::Plane {
                front_frame: SketchFrame {
                    origin,
                    x_axis: x,
                    y_axis: y,
                },
                front_to_back_sign: parity,
            },
        }],
        bends: Vec::new(),
        joins: Vec::new(),
        opened: Vec::new(),
        lineage: Vec::new(),
        overlays: Vec::new(),
        gaps: Vec::new(),
        invalidated: None,
        blockers: Vec::new(),
    };
    for (i, m) in metrics.iter().enumerate() {
        let slot = i as u32;
        let panel = PanelId { feature: uid, slot };
        let end = 51 + 100 * slot;
        let port = port_frame(
            &state,
            &BoundaryRef {
                panel,
                curve: SheetCurveId::Generated {
                    feature: uid,
                    slot: end,
                },
                piece: None,
                range: None,
            },
        )?;
        let stationary = side(
            chart(&state, panel)?,
            &generated(uid, end),
            port.to_operation,
        )?;
        let constructed = construction(&port, 0.0, 0.0, m);
        let next = slot + 1;
        let moving = SheetPatch {
            domain: SheetDomainRef::Panel(PanelId {
                feature: uid,
                slot: next,
            }),
            regions: vec![rectangle(
                uid,
                50 + 100 * next,
                [0.0, 0.0],
                [straight[i + 1], width],
            )?],
            embedding: constructed.moving,
        };
        let moving_side = side(
            &moving,
            &generated(uid, 53 + 100 * next),
            Isometry2::IDENTITY,
        )?;
        add_bend_strip(
            ctx,
            &mut state,
            m,
            constructed.cylinder,
            vec![stationary],
            vec![moving_side],
            port.width,
            None,
            slot,
        )?;
        state.charts.push(moving);
    }
    publish(ctx, state, &rules, None)
}
