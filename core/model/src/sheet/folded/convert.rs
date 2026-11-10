// SPDX-License-Identifier: MIT
//! Convert to Sheet Metal (mitcad#461): the planar faces of a body become
//! the panels of a sheet of its rules.
//!
//! The picked face is the first panel's front surface, the material behind
//! it (into the body). Its outline is read exactly from the body (lines,
//! arcs and splines of its edges, its holes too).
//!
//! - Without bends the body must be a plate of the rules' thickness behind
//!   the face: the sheet rebuilt from the face must have the body's volume
//!   and centre of mass.
//! - With bends, the planar faces reached from the picked face across the
//!   bend edges are panels, each on the sheet's front with the rules'
//!   thickness behind it: a shell of that wall thickness keeps its walls, a
//!   solid becomes the sheet of its faces. A bend edge is the virtual sharp
//!   of its bend on the front: the panels on both sides are set back from
//!   it by `(r+t) tan(a/2)` where the front is the bend's outside and by
//!   `r tan(a/2)` where it is its inside (`a` the angle between the
//!   faces). Each face across a bend is built where the bend puts it (a
//!   flange's construction from the setback line), so it must lie in the
//!   plane of the body's face, and its edge along the bend must run the
//!   whole bend edge, as the other side's does. Two panels may meet only at
//!   a bend: an edge between two panels that is no bend (a corner that
//!   would have to be cut open) is refused, as is a bend closing a loop.
//!
//! Panel `k` is the k-th face reached (slot 0 the picked one), bend `k`
//! the bend that reached panel `k + 1`. The faces' curves are generated
//! curves `g<1000000+>` of the feature, the setback lines `g<900000+20k+3>`
//! (on the panel before the bend) and `g<900010+20k+1>` (after it).

use crate::notice::NoticePlace;
use mitcad_vec3::{norm, scale, sub};

use super::bends::{BendMetrics, add_bend_strip, construction, port_frame, side};
use super::develop::{orient, region_area};
use super::*;
use crate::datum::{CurveGeometry, SurfaceGeometry};
use crate::features::face_refs::kernel_error;
use crate::features::sheet_contour::rules_of;
use crate::features::sheet_convert::SheetConvertDef;
use crate::kernel::Curve3;
use crate::topo::{FaceName, TopoName};
use mitcad_vec3::{cross, dot};

use mitcad_vec3::Vec3 as V3;

fn unit(a: V3) -> Result<V3, String> {
    let l = norm(a);
    if l <= 1e-12 {
        return Err("a direction of zero length".into());
    }
    Ok(scale(a, 1.0 / l))
}

const FACE_CURVES: u32 = 1_000_000;
const TOOLS: u32 = 900_000;

/// A planar face of the body: its plane (outward normal) and edges.
struct Face {
    name: FaceName,
    origin: V3,
    normal: V3,
    curves: Vec<Curve3>,
}

fn face<K: Kernel>(
    ctx: &EvalContext<'_, K>,
    shape: &K::Shape,
    name: &FaceName,
) -> Result<Face, String> {
    let surface = ctx
        .kernel
        .face_geometry(shape, name)
        .map_err(kernel_error)?;
    let SurfaceGeometry::Plane { origin, normal } = surface else {
        return Err(placed_at(
            vec![NoticePlace::face(name.to_string())],
            "the face is not planar: only planar faces become panels of a sheet",
            format!("face {name} is not planar: only planar faces become panels of a sheet"),
        ));
    };
    let curves = ctx
        .kernel
        .curves_of(shape, &TopoName::Face(name.clone()))
        .map_err(kernel_error)?;
    if curves.is_empty() {
        return Err(placed_at(
            vec![NoticePlace::face(name.to_string())],
            "the body has no such face",
            format!("the body has no face {name}"),
        ));
    }
    Ok(Face {
        name: name.clone(),
        origin,
        normal: unit(normal)?,
        curves,
    })
}

/// A frame on a plane whose normal is `into` (x along `hint` where it can).
fn frame_on(origin: V3, into: V3, hint: V3) -> Result<SketchFrame, String> {
    let mut x = sub(hint, scale(into, dot(hint, into)));
    if dot(x, x) <= 1e-12 {
        x = if into[0].abs() < 0.9 {
            [1.0, 0.0, 0.0]
        } else {
            [0.0, 1.0, 0.0]
        };
        x = sub(x, scale(into, dot(x, into)));
    }
    let x = unit(x)?;
    Ok(SketchFrame {
        origin,
        x_axis: x,
        y_axis: cross(into, x),
    })
}

/// The face's outline in `frame` (on its plane): one region, exterior
/// counter-clockwise.
fn outline(
    face: &Face,
    frame: &SketchFrame,
    feature: FeatureUid,
    slot: &mut u32,
) -> Result<MaterialRegion, String> {
    let n = frame.normal();
    if dot(sub(face.origin, frame.origin), n).abs() > 1e-6 || dot(face.normal, n).abs() < 1.0 - 1e-9
    {
        return Err(placed_at(
            vec![NoticePlace::face(face.name.to_string())],
            "a face is not where the bend before it puts it: it must lie in the plane of the \
             panel the bend makes",
            format!(
                "face {} is not where the bend before it puts it: it must lie in the plane of the \
                 panel the bend makes",
                face.name
            ),
        ));
    }
    let curves = face
        .curves
        .iter()
        .map(|c| crate::sheet::rederive::planar_curve(c, frame))
        .collect::<Result<Vec<_>, _>>()?;
    let mut regions = crate::sheet::rederive::regions(feature, slot, curves)?;
    if regions.len() != 1 {
        return Err(placed_at(
            vec![NoticePlace::face(face.name.to_string())],
            format!(
                "the outline of a face is not one region ({} found)",
                regions.len()
            ),
            format!(
                "the outline of face {} is not one region ({} found)",
                face.name,
                regions.len()
            ),
        ));
    }
    let mut region = regions.remove(0);
    orient(&mut region)?;
    Ok(region)
}

fn chart_point(frame: &SketchFrame, p: V3) -> [f64; 2] {
    let d = sub(p, frame.origin);
    [dot(d, frame.x_axis), dot(d, frame.y_axis)]
}

/// The straight boundary of a panel along the line `a`..`b` (both ends
/// matching, in the panel's chart).
fn edge_on(state: &SheetState, panel: PanelId, a: V3, b: V3) -> Result<BoundaryRef, String> {
    let patch = chart(state, panel)?;
    let (frame, _) = plane(patch)?;
    let (a, b) = (chart_point(&frame, a), chart_point(&frame, b));
    let near = |p: [f64; 2], q: [f64; 2]| crate::sketch::geometry::dist(p, q) <= 1e-6;
    for e in patch
        .regions
        .iter()
        .flat_map(|r| &r.loops)
        .flat_map(|l| &l.edges)
    {
        if !matches!(e.curve.geometry, SegmentGeometry::Line { .. }) {
            continue;
        }
        let (p, q) = e.curve.ends()?;
        if (near(p, a) && near(q, b)) || (near(p, b) && near(q, a)) {
            return Ok(BoundaryRef {
                panel,
                curve: e.piece.curve.clone(),
                piece: Some(e.piece.piece),
                range: None,
            });
        }
    }
    Err(format!(
        "the bend edge is not one whole straight edge of {}",
        develop::domain_label(SheetDomainRef::Panel(panel))
    ))
}

fn piece_of(
    state: &SheetState,
    panel: PanelId,
    curve: SheetCurveId,
) -> Result<CurvePieceRef, String> {
    chart(state, panel)?
        .regions
        .iter()
        .flat_map(|r| &r.loops)
        .flat_map(|l| &l.edges)
        .find(|e| e.piece.curve == curve)
        .map(|e| e.piece.clone())
        .ok_or_else(|| {
            format!(
                "the bend's setback leaves no edge on {}",
                develop::domain_label(SheetDomainRef::Panel(panel))
            )
        })
}

pub(crate) fn convert<K: Kernel>(
    d: &SheetConvertDef<f64>,
    ctx: &mut EvalContext<'_, K>,
) -> Result<FeatureOutput<K::Shape>, String> {
    let (rules, rules_version) = rules_of(ctx, d.rules)?;
    let shape = ctx.body(d.body)?;
    let fixed_name = ctx
        .faces_on(d.body, &shape, std::slice::from_ref(&d.face))?
        .remove(0);
    let owner = ctx
        .env
        .owners
        .get(&d.body)
        .copied()
        .ok_or_else(|| format!("body {} has no owner", d.body))?;
    let uid = ctx.uid;
    let fixed = face(ctx, &shape, &fixed_name)?;
    let into = scale(fixed.normal, -1.0);
    let hint = fixed
        .curves
        .iter()
        .find_map(|c| match c {
            Curve3::Line { start, end } => Some(sub(*end, *start)),
            _ => None,
        })
        .unwrap_or([1.0, 0.0, 0.0]);
    let frame = frame_on(fixed.origin, into, hint)?;
    let mut slot = FACE_CURVES;
    let base = outline(&fixed, &frame, uid, &mut slot)?;
    let base_id = PanelId {
        feature: uid,
        slot: 0,
    };
    let mut state = SheetState {
        schema: 2,
        algorithm: ALGORITHM_VERSION,
        body: d.body,
        owner,
        shape_generation: String::new(),
        rules: d.rules,
        rules_version,
        panels: Vec::new(),
        charts: vec![SheetPatch {
            domain: SheetDomainRef::Panel(base_id),
            regions: vec![base.clone()],
            embedding: SheetEmbedding::Plane {
                front_frame: frame,
                front_to_back_sign: MaterialParity::Positive,
            },
        }],
        bends: Vec::new(),
        joins: Vec::new(),
        opened: Vec::new(),
        lineage: Vec::new(),
        overlays: Vec::new(),
        invalidated: None,
        blockers: Vec::new(),
    };
    if d.bends.is_empty() {
        return plate(ctx, d, &shape, state, &rules, region_area(&base)?);
    }
    // The bend edges: straight, between two planar faces.
    let edges = ctx.edges_on(d.body, &shape, &d.bends)?;
    let mut lines = Vec::new();
    for edge in &edges {
        match ctx
            .kernel
            .edge_geometry(&shape, edge)
            .map_err(kernel_error)?
        {
            CurveGeometry::Line { start, end } => lines.push((edge.clone(), start, end)),
            _ => {
                return Err(placed_at(
                    vec![NoticePlace::edge(edge.to_string())],
                    "a bend edge is not straight",
                    format!("bend edge {edge} is not straight"),
                ));
            }
        }
    }
    // The panels in the order they are reached from the picked face.
    let mut panels: Vec<FaceName> = vec![fixed_name.clone()];
    let mut order: Vec<(usize, usize, usize)> = Vec::new(); // (edge, from, to)
    let mut todo: Vec<usize> = (0..lines.len()).collect();
    while !todo.is_empty() {
        let before = todo.len();
        let mut rest = Vec::new();
        for k in todo {
            let [a, b] = lines[k].0.faces().clone();
            let (ia, ib) = (
                panels.iter().position(|p| *p == a),
                panels.iter().position(|p| *p == b),
            );
            match (ia, ib) {
                (Some(_), Some(_)) => {
                    return Err(placed_at(
                        vec![NoticePlace::edge(lines[k].0.to_string())],
                        "a bend edge closes a loop of bends: a flat sheet cannot fold into it",
                        format!(
                            "bend edge {} closes a loop of bends: a flat sheet cannot fold into it",
                            lines[k].0
                        ),
                    ));
                }
                (Some(i), None) => {
                    panels.push(b);
                    order.push((k, i, panels.len() - 1));
                }
                (None, Some(i)) => {
                    panels.push(a);
                    order.push((k, i, panels.len() - 1));
                }
                (None, None) => rest.push(k),
            }
        }
        if rest.len() == before {
            return Err(placed_at(
                vec![NoticePlace::edge(lines[rest[0]].0.to_string())],
                "a bend edge does not join the picked face through the other bends",
                format!(
                    "bend edge {} does not join the picked face through the other bends",
                    lines[rest[0]].0
                ),
            ));
        }
        todo = rest;
    }
    // Panels may meet only at bends.
    if let Ok(all) = ctx.kernel.edges(&shape) {
        for info in all {
            let Some(name) = info.name else { continue };
            let Ok(TopoName::Edge(edge)) = name.parse::<TopoName>() else {
                continue;
            };
            let [a, b] = edge.faces();
            if a != b
                && panels.contains(a)
                && panels.contains(b)
                && !edges.iter().any(|e| e.faces() == edge.faces())
            {
                return Err(placed_at(
                    vec![
                        NoticePlace::edge(edge.to_string()),
                        NoticePlace::face(a.to_string()),
                        NoticePlace::face(b.to_string()),
                    ],
                    "two faces meet at an edge that is no bend: a corner that would have to be \
                     cut open is not supported",
                    format!(
                        "faces {a} and {b} meet at an edge that is no bend: a corner that would \
                         have to be cut open is not supported"
                    ),
                ));
            }
        }
    }
    for (n, (k, from, to)) in order.iter().enumerate() {
        let (edge, start, end) = &lines[*k];
        let from_id = PanelId {
            feature: uid,
            slot: *from as u32,
        };
        let to_face = face(ctx, &shape, &panels[*to])?;
        let boundary = edge_on(&state, from_id, *start, *end).map_err(|e| {
            crate::notice::about_place(
                NoticePlace::edge(edge.to_string()),
                &format!("a bend edge: {e}"),
            )
        })?;
        let port = port_frame(&state, &boundary)?;
        // Which way the face runs from the edge: probe both sides of the
        // edge's middle in its plane.
        let along = unit(sub(*end, *start))?;
        let across = unit(cross(to_face.normal, along))?;
        let probe = frame_on(*start, to_face.normal, along)?;
        let mut probe_slot = 0;
        let region = outline(&to_face, &probe, uid, &mut probe_slot)?;
        let length = dot(sub(*end, *start), along);
        let middle: V3 = std::array::from_fn(|i| start[i] + along[i] * length / 2.0);
        let away = [1e-3, -1e-3]
            .into_iter()
            .find(|s| {
                let p: V3 = std::array::from_fn(|i| middle[i] + across[i] * s);
                crate::sheet::flat::contains(&region, chart_point(&probe, p))
            })
            .map(|s| scale(across, s.signum()))
            .ok_or_else(|| {
                placed_at(
                    vec![
                        NoticePlace::edge(edge.to_string()),
                        NoticePlace::face(to_face.name.to_string()),
                    ],
                    "a bend edge: the face beside it does not run from it",
                    format!(
                        "bend edge {edge}: face {} does not run from it",
                        to_face.name
                    ),
                )
            })?;
        let angle = dot(away, port.n).atan2(dot(away, port.m));
        if angle.abs() <= 1e-6 || angle.abs() >= std::f64::consts::PI - 1e-6 {
            return Err(placed_at(
                vec![NoticePlace::edge(edge.to_string())],
                "the faces at a bend edge must meet at an angle between 0 and 180 degrees",
                format!(
                    "the faces at bend edge {edge} must meet at an angle between 0 and 180 degrees"
                ),
            ));
        }
        let metrics = BendMetrics::new(angle, d.radius, &rules)?;
        // The front is the bend's outside where it turns towards the back.
        let setback = if angle > 0.0 {
            metrics.setback(metrics.outer, "outer")?
        } else {
            metrics.setback(metrics.inner, "inner")?
        };
        let tool_a = TOOLS + 20 * n as u32;
        let tool_b = tool_a + 10;
        trim(
            ctx,
            &mut state,
            &[SheetDomainRef::Panel(from_id)],
            SheetTrimOperation::Difference,
            &[rectangle(uid, tool_a, [-setback, 0.0], [1.0, port.width])?],
            &[port.to_operation],
        )?;
        let piece = piece_of(
            &state,
            from_id,
            SheetCurveId::Generated {
                feature: uid,
                slot: tool_a + 3,
            },
        )?;
        let stationary = side(chart(&state, from_id)?, &piece, port.to_operation)?;
        let constructed = construction(&port, -setback, 0.0, &metrics);
        let SheetEmbedding::Plane { front_frame, .. } = &constructed.moving else {
            return Err("a bend's moving panel is not planar".into());
        };
        let region = outline(&to_face, front_frame, uid, &mut slot)?;
        let to_id = PanelId {
            feature: uid,
            slot: *to as u32,
        };
        state.charts.push(SheetPatch {
            domain: SheetDomainRef::Panel(to_id),
            regions: vec![region],
            embedding: constructed.moving.clone(),
        });
        trim(
            ctx,
            &mut state,
            &[SheetDomainRef::Panel(to_id)],
            SheetTrimOperation::Difference,
            &[rectangle(
                uid,
                tool_b,
                [-setback - 1.0, 0.0],
                [0.0, port.width],
            )?],
            &[Isometry2::IDENTITY],
        )?;
        let piece = piece_of(
            &state,
            to_id,
            SheetCurveId::Generated {
                feature: uid,
                slot: tool_b + 1,
            },
        )?;
        let moving_side = side(chart(&state, to_id)?, &piece, Isometry2::IDENTITY)?;
        add_bend_strip(
            ctx,
            &mut state,
            &metrics,
            constructed.cylinder,
            vec![stationary],
            vec![moving_side],
            port.width,
            None,
            n as u32,
        )
        .map_err(|e| {
            placed_at(
                vec![NoticePlace::edge(edge.to_string())],
                format!(
                    "a bend edge: the faces on its two sides do not run the whole edge after \
                     their setbacks ({})",
                    crate::notice::user_message(&e)
                ),
                format!(
                    "bend edge {edge}: the faces on its two sides do not run the whole edge after \
                     their setbacks ({e})"
                ),
            )
        })?;
    }
    publish(ctx, state, &rules, None)
}

/// A plate: the sheet of the picked face must be the body.
fn plate<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    d: &SheetConvertDef<f64>,
    shape: &K::Shape,
    state: SheetState,
    rules: &crate::features::sheet::SheetRulesDef<f64>,
    area: f64,
) -> Result<FeatureOutput<K::Shape>, String> {
    let body = ctx.kernel.mass_properties(shape).map_err(kernel_error)?;
    let t = rules.thickness;
    let measured = body.volume / area;
    if (measured - t).abs() > 1e-6 * t.max(1.0) {
        return Err(placed_at(
            vec![NoticePlace::face(d.face.to_string())],
            format!(
                "the body is not a plate of its rules' thickness {t} mm behind the face: its \
                 volume over the face's area is {measured:.6} mm"
            ),
            format!(
                "the body is not a plate of its rules' thickness {t} mm behind face {}: its volume \
                 over the face's area is {measured:.6} mm",
                d.face
            ),
        ));
    }
    let output = publish(ctx, state, rules, None)?;
    let Some(crate::features::BodyChange::Set(_, sheet)) = output.changes.first() else {
        return Err("the converted sheet has no shape".into());
    };
    let rebuilt = ctx.kernel.mass_properties(sheet).map_err(kernel_error)?;
    let size = body.volume.abs().cbrt().max(1.0);
    if (rebuilt.volume - body.volume).abs() > 1e-6 * body.volume.abs().max(1.0)
        || (0..3).any(|i| (rebuilt.center[i] - body.center[i]).abs() > 1e-6 * size)
    {
        return Err(placed_at(
            vec![NoticePlace::face(d.face.to_string())],
            "the body is not a plate of even thickness behind the face: the sheet of that face \
             differs from it",
            format!(
                "the body is not a plate of even thickness behind face {}: the sheet of that face \
                 differs from it",
                d.face
            ),
        ));
    }
    Ok(output)
}

/// A refusal about faces and edges of the body (mitcad#1612): `said` in
/// plain words for a person, the places painted, `named` with their names
/// for agents.
fn placed_at(
    places: Vec<NoticePlace>,
    said: impl Into<String>,
    named: impl Into<String>,
) -> String {
    crate::notice::FeatureError::new(said)
        .with_places(places)
        .with_detail(named)
        .text()
}
