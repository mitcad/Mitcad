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
//!   plane of the body's face. A bend closing a loop is refused.
//! - Every other edge between two panels is a rip (mitcad#1949): the
//!   panels are cut apart there as the walls of a flange on several edges
//!   are at a corner (mitcad#1872): on the plane bisecting the two faces,
//!   half the corner gap off it each way, or one face running on to the
//!   other's far surface (the earlier or the later face in the order they
//!   are reached), the other ending the gap short of it. A sheet is cut
//!   normal to its surfaces, on the line where that plane meets the
//!   surface that reaches further. The bends at a rip's ends end square
//!   where all their material clears the same plane, and the panel before
//!   such a bend is cut back to its tangent line beside it to the edge's
//!   end (a corner relief). Each rip is recorded as a corner gap of the
//!   sheet ([`crate::sheet::SheetGap`], mitcad#2098), across which a later
//!   flange on several edges mitres its walls.
//! - A bend runs along its edge only as far as both its faces reach its
//!   tangent lines: a face whose ends are not square to its bend (a
//!   vertical side wall on an inclined base) runs past the bend at one end
//!   and ends short of the edge's end at the other, and is cut back to its
//!   tangent line beside the bend.
//!
//! - A cylindrical or conical picked face is a curved panel of its own
//!   (mitcad#2435, [`super::curved`]); other curved faces are refused
//!   saying why they cannot be laid flat.
//!
//! The panels are all placed first, from the faces as the body has them;
//! then each is cut at its rips, then back to its bends' tangent lines,
//! and the bends are made last, between the cut panels.
//!
//! Panel `k` is the k-th face reached (slot 0 the picked one), bend `k`
//! the bend that reached panel `k + 1`. The faces' curves are generated
//! curves `g<1000000+>` of the feature, the setback lines `g<900000+20k+3>`
//! (on the panel before the bend) and `g<900010+20k+1>` (after it); the
//! cuts beside bend `k` `g<950000+20k>` on, the cuts at the rips
//! `g<980000+>`.

use crate::notice::NoticePlace;
use mitcad_vec3::{norm, scale, sub};

use super::bends::{BendMetrics, PortFrame, add_bend_strip, construction, port_frame, side};
use super::develop::{orient, region_area};
use super::flange_edges::{Keep, Limit, bend_limit};
use super::*;
use crate::datum::{CurveGeometry, SurfaceGeometry};
use crate::features::face_refs::kernel_error;
use crate::features::sheet_contour::rules_of;
use crate::features::sheet_convert::SheetConvertDef;
use crate::features::sheet_folded::SheetMiter;
use crate::kernel::Curve3;
use crate::topo::{EdgeName, FaceName, TopoName};
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
/// The cuts beside a bend that a rip or a skewed end shortens (mitcad#1949).
const CORNERS: u32 = 950_000;
/// The cuts of the panels at the rips (mitcad#1949).
const CLIPS: u32 = 980_000;

/// Ends of a bend edge and a rip edge this close (mm) meet.
const MEET: f64 = 1e-3;

/// A bend's span this close (mm) to its edge's ends runs to them: no
/// sliver of a face is left beside it.
const SNAP: f64 = 1e-6;

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
        // A curved face across a bend, or one that bends two ways
        // (mitcad#2435).
        return Err(super::curved::not_flat(ctx, shape, name, &surface));
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

/// The in-plane direction from the straight edge `start`..`end` of `face`
/// into the face (probed beside the edge's middle), or none when the face
/// does not run from the edge.
fn into_face(face: &Face, start: V3, end: V3, feature: FeatureUid) -> Result<Option<V3>, String> {
    let along = unit(sub(end, start))?;
    let across = unit(cross(face.normal, along))?;
    let probe = frame_on(start, face.normal, along)?;
    let mut probe_slot = 0;
    let region = outline(face, &probe, feature, &mut probe_slot)?;
    let length = dot(sub(end, start), along);
    let middle: V3 = std::array::from_fn(|i| start[i] + along[i] * length / 2.0);
    Ok([1e-3, -1e-3]
        .into_iter()
        .find(|s| {
            let p: V3 = std::array::from_fn(|i| middle[i] + across[i] * s);
            crate::sheet::flat::contains(&region, chart_point(&probe, p))
        })
        .map(|s| scale(across, s.signum())))
}

/// A bend as the faces place it, before any panel is cut.
struct Placed {
    edge: EdgeName,
    ends: [V3; 2],
    from: usize,
    to: usize,
    port: PortFrame,
    metrics: BendMetrics,
    setback: f64,
}

/// A rip (mitcad#1949): an edge between two converted faces that is no
/// bend, and what the material of each face (and of the bends at the
/// rip's ends on its side) keeps to.
struct Rip {
    edge: EdgeName,
    ends: [V3; 2],
    panels: [usize; 2],
    wall: [Vec<Keep>; 2],
    bend: [Vec<Keep>; 2],
}

/// The slab of a face's panel along `s`: its least and greatest `s . p`
/// (the front on the face, the back a thickness behind it).
fn slab(face: &Face, thickness: f64, s: V3) -> [f64; 2] {
    let front = dot(s, face.origin);
    let back = front - thickness * dot(s, face.normal);
    [front.min(back), front.max(back)]
}

/// The keep of a panel that ends `gap` short of the panel `other`'s
/// surface it comes to, moving `along` (towards the rip), or none when the
/// panel runs beside the other.
fn short_of(other: &Face, thickness: f64, along: V3, gap: f64) -> Option<Keep> {
    let n = other.normal;
    let s = dot(n, along);
    if s.abs() < 1e-6 {
        return None;
    }
    let s = if s > 0.0 { n } else { scale(n, -1.0) };
    let near = slab(other, thickness, s)[0];
    Some(Keep {
        normal: scale(s, -1.0),
        offset: -(near - gap),
    })
}

/// What the two faces of a rip keep to (the corners of a flange on several
/// edges, mitcad#1872), each face given by the in-plane direction from the
/// rip into it (`inward`): the plane bisecting the faces, half the gap off
/// it each way; or the earlier or the later face (in the order the faces
/// are reached) running on to the far surface of the other, which ends the
/// gap short of the first. The walls' keeps, then the bends'.
type RipKeeps = ([Vec<Keep>; 2], [Vec<Keep>; 2]);

fn rip_keeps(
    faces: [&Face; 2],
    inward: [V3; 2],
    point: V3,
    (thickness, gap): (f64, f64),
    miter: SheetMiter,
    places: &[NoticePlace],
) -> Result<RipKeeps, String> {
    let refused = |said: &str| placed_at(places.to_vec(), said, said);
    let difference = sub(inward[0], inward[1]);
    let length = norm(difference);
    if length < 1e-9 {
        return Err(refused("two faces at a rip lie over each other"));
    }
    let n = scale(difference, 1.0 / length);
    let keeps = [
        Keep {
            normal: n,
            offset: dot(n, point) + gap / 2.0,
        },
        Keep {
            normal: scale(n, -1.0),
            offset: -dot(n, point) + gap / 2.0,
        },
    ];
    let flat = dot(inward[0], inward[1]) < -1.0 + 1e-9;
    let mut wall = [vec![keeps[0]], vec![keeps[1]]];
    let mut bend = [vec![keeps[0]], vec![keeps[1]]];
    if miter == SheetMiter::Bisect {
        if flat && gap <= TOLERANCE {
            return Err(refused(
                "faces that lie in one plane need a corner gap above 0 at a rip between them",
            ));
        }
        return Ok((wall, bend));
    }
    if flat {
        return Err(refused(
            "faces that lie in one plane meet at a rip only by a bisecting miter; choose Bisect",
        ));
    }
    if gap <= TOLERANCE {
        return Err(refused(
            "a rip where one face runs past the other needs a corner gap above 0",
        ));
    }
    // The passing face `p` runs on to the far surface of the other face
    // `q`; `q` ends the gap short of `p`'s near surface.
    let (p, q) = if miter == SheetMiter::EarlierRunsPast {
        (0, 1)
    } else {
        (1, 0)
    };
    let beside = || {
        refused(
            "the faces at a rip run side by side, so neither can run past the other; choose Bisect",
        )
    };
    let far = short_of(faces[q], thickness, scale(inward[p], -1.0), 0.0).ok_or_else(beside)?;
    let far = Keep {
        normal: far.normal,
        offset: far.offset - thickness,
    };
    let short = short_of(faces[p], thickness, scale(inward[q], -1.0), gap).ok_or_else(beside)?;
    wall[p] = vec![far];
    wall[q] = vec![short];
    bend[q].push(short);
    Ok((wall, bend))
}

/// Cuts the panel `panel` to the material that keeps to `keep` on both its
/// surfaces (a cut normal to the panel, on the line where the keep's plane
/// meets the surface that reaches further; curves `slot` on).
fn clip<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    state: &mut SheetState,
    panel: PanelId,
    (keep, thickness): (&Keep, f64),
    slot: u32,
    places: &[NoticePlace],
) -> Result<(), String> {
    let patch = chart(state, panel)?;
    let (frame, parity) = plane(patch)?;
    let (min, max) = bounds(patch)?;
    let n = keep.normal;
    let (a, b) = (dot(n, frame.x_axis), dot(n, frame.y_axis));
    // The back lies a thickness along `sign * normal` from the front.
    let c = keep.offset - dot(n, frame.origin)
        + (-thickness * parity.sign() * dot(n, frame.normal())).max(0.0);
    let size = a.hypot(b);
    let said = "at a rip a face would be cut away whole; make the corner gap smaller";
    if size < 1e-12 {
        if c > TOLERANCE {
            return Err(placed_at(places.to_vec(), said, said));
        }
        return Ok(());
    }
    let (a, b, c) = (a / size, b / size, c / size);
    let reach = 1.0 + 10.0 * (min[0].abs() + min[1].abs() + max[0].abs() + max[1].abs() + c.abs());
    // The half-plane `a x + b y < c` the cut takes away, counter-clockwise.
    let q = [a * c, b * c];
    let at = |s: f64, u: f64| [q[0] + s * a + u * b, q[1] + s * b - u * a];
    let outside = polygon(
        ctx.uid,
        slot,
        &[
            at(-reach, -reach),
            at(-reach, reach),
            at(0.0, reach),
            at(0.0, -reach),
        ],
    )?;
    trim(
        ctx,
        state,
        &[SheetDomainRef::Panel(panel)],
        SheetTrimOperation::Difference,
        &[outside],
        &[Isometry2::IDENTITY],
    )?;
    if chart(state, panel)?.regions.is_empty() {
        return Err(placed_at(places.to_vec(), said, said));
    }
    Ok(())
}

/// The interval (in `map`'s second coordinate) that the feature's
/// generated boundary curve `slot` covers on the panel, or none.
fn covered(state: &SheetState, panel: PanelId, slot: u32, map: Isometry2) -> Option<[f64; 2]> {
    let curve = generated(panel.feature, slot).curve;
    let mut hull: Option<[f64; 2]> = None;
    for e in chart(state, panel)
        .ok()?
        .regions
        .iter()
        .flat_map(|r| &r.loops)
        .flat_map(|l| &l.edges)
        .filter(|e| e.piece.curve == curve)
    {
        let (p, q) = e.curve.ends().ok()?;
        let (v, w) = (map.point(p)[1], map.point(q)[1]);
        let h = hull.get_or_insert([f64::INFINITY, f64::NEG_INFINITY]);
        *h = [h[0].min(v.min(w)), h[1].max(v.max(w))];
    }
    hull
}

/// Where the panel's material reaches the line of the tool's curve `slot`
/// (its interval in `map`'s second coordinate) once `tool` (in the
/// coordinates `map` takes the panel's chart to) cuts the panel. The state
/// is left as it was.
fn reach_along<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    state: &SheetState,
    panel: PanelId,
    (tool, map): (MaterialRegion, Isometry2),
    slot: u32,
) -> Result<Option<[f64; 2]>, String> {
    let mut trial = state.clone();
    trim(
        ctx,
        &mut trial,
        &[SheetDomainRef::Panel(panel)],
        SheetTrimOperation::Difference,
        &[tool],
        &[map],
    )?;
    Ok(covered(&trial, panel, slot, map))
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
    // Faces laid flat approximately, by triangulation (mitcad#2442).
    if let Some(output) =
        super::triangulated::convert(d, ctx, &shape, &fixed_name, owner, &rules_version, &rules)?
    {
        return Ok(output);
    }
    // A cylindrical or conical face is a curved panel of its own; other
    // curved faces are refused saying why (mitcad#2435).
    let surface = ctx
        .kernel
        .face_geometry(&shape, &fixed_name)
        .map_err(kernel_error)?;
    if !matches!(surface, SurfaceGeometry::Plane { .. }) {
        let base = SheetState {
            schema: 2,
            algorithm: ALGORITHM_VERSION,
            body: d.body,
            owner,
            shape_generation: String::new(),
            rules: d.rules,
            rules_version: rules_version.clone(),
            panels: Vec::new(),
            charts: Vec::new(),
            bends: Vec::new(),
            joins: Vec::new(),
            opened: Vec::new(),
            lineage: Vec::new(),
            overlays: Vec::new(),
            gaps: Vec::new(),
            invalidated: None,
            blockers: Vec::new(),
            approximation: None,
        };
        return super::curved::convert(d, ctx, &shape, &fixed_name, &surface, base, &rules);
    }
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
        gaps: Vec::new(),
        invalidated: None,
        blockers: Vec::new(),
        approximation: None,
    };
    if d.bends.is_empty() {
        if let Some(rip) = d.rips.first() {
            return Err(placed_at(
                vec![NoticePlace::edge(rip.to_string())],
                "a rip edge must lie between two of the converted faces; without bends only the \
                 picked face is converted",
                format!(
                    "rip edge {rip} must lie between two of the converted faces; without bends \
                     only the picked face is converted"
                ),
            ));
        }
        return plate(ctx, d, &shape, state, &rules, region_area(&base)?);
    }
    let thickness = rules.thickness;
    let gap = d.corner_gap.unwrap_or(thickness);
    if !gap.is_finite() || gap < 0.0 {
        return Err("the corner gap must not be negative".into());
    }
    let miter = d.miter.unwrap_or_default();
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
                        "a bend edge closes a loop of bends: a flat sheet cannot fold into it; \
                         leave it out of the bends to rip it open",
                        format!(
                            "bend edge {} closes a loop of bends: a flat sheet cannot fold into \
                             it; leave it out of the bends to rip it open",
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
    let mut faces = vec![fixed];
    for name in &panels[1..] {
        faces.push(face(ctx, &shape, name)?);
    }
    // The rips (mitcad#1949): every other edge between two panels.
    let mut rips: Vec<Rip> = Vec::new();
    if let Ok(all) = ctx.kernel.edges(&shape) {
        for info in all {
            let Some(name) = info.name else { continue };
            let Ok(TopoName::Edge(edge)) = name.parse::<TopoName>() else {
                continue;
            };
            let [a, b] = edge.faces();
            let (Some(ia), Some(ib)) = (
                panels.iter().position(|p| p == a),
                panels.iter().position(|p| p == b),
            ) else {
                continue;
            };
            if ia == ib || edges.iter().any(|e| e.faces() == edge.faces()) {
                continue;
            }
            let places = vec![
                NoticePlace::edge(edge.to_string()),
                NoticePlace::face(a.to_string()),
                NoticePlace::face(b.to_string()),
            ];
            let CurveGeometry::Line { start, end } = ctx
                .kernel
                .edge_geometry(&shape, &edge)
                .map_err(kernel_error)?
            else {
                return Err(placed_at(
                    places,
                    "two converted faces meet at an edge that is not straight: it cannot be a rip",
                    format!(
                        "faces {a} and {b} meet at edge {edge}, which is not straight: it cannot \
                         be a rip"
                    ),
                ));
            };
            let (ia, ib) = (ia.min(ib), ia.max(ib));
            let mut inward = [[0.0; 3]; 2];
            for (k, i) in [ia, ib].into_iter().enumerate() {
                inward[k] = into_face(&faces[i], start, end, uid)?.ok_or_else(|| {
                    placed_at(
                        places.clone(),
                        "a rip edge: a face beside it does not run from it",
                        format!("rip edge {edge}: face {} does not run from it", panels[i]),
                    )
                })?;
            }
            let (wall, bend) = rip_keeps(
                [&faces[ia], &faces[ib]],
                inward,
                start,
                (thickness, gap),
                miter,
                &places,
            )?;
            rips.push(Rip {
                edge,
                ends: [start, end],
                panels: [ia, ib],
                wall,
                bend,
            });
        }
    }
    for rip in &d.rips {
        if !rips.iter().any(|r| r.edge.faces() == rip.faces()) {
            return Err(placed_at(
                vec![NoticePlace::edge(rip.to_string())],
                "a rip edge must lie between two of the converted faces, and not be a bend",
                format!(
                    "rip edge {rip} must lie between two of the converted faces, and not be a bend"
                ),
            ));
        }
    }
    // Each bend placed from its faces as they are, before any is cut.
    let mut placed: Vec<Placed> = Vec::new();
    for (k, from, to) in &order {
        let (edge, start, end) = &lines[*k];
        let from_id = PanelId {
            feature: uid,
            slot: *from as u32,
        };
        let to_face = &faces[*to];
        let boundary = edge_on(&state, from_id, *start, *end).map_err(|e| {
            crate::notice::about_place(
                NoticePlace::edge(edge.to_string()),
                &format!("a bend edge: {e}"),
            )
        })?;
        let port = port_frame(&state, &boundary)?;
        // Which way the face runs from the edge.
        let away = into_face(to_face, *start, *end, uid)?.ok_or_else(|| {
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
        let constructed = construction(&port, -setback, 0.0, &metrics);
        let SheetEmbedding::Plane { front_frame, .. } = &constructed.moving else {
            return Err("a bend's moving panel is not planar".into());
        };
        let region = outline(to_face, front_frame, uid, &mut slot)?;
        state.charts.push(SheetPatch {
            domain: SheetDomainRef::Panel(PanelId {
                feature: uid,
                slot: *to as u32,
            }),
            regions: vec![region],
            embedding: constructed.moving.clone(),
        });
        placed.push(Placed {
            edge: edge.clone(),
            ends: [*start, *end],
            from: *from,
            to: *to,
            port,
            metrics,
            setback,
        });
    }
    // Each panel cut at its rips.
    let mut clips = CLIPS;
    for rip in &rips {
        let places = vec![
            NoticePlace::edge(rip.edge.to_string()),
            NoticePlace::face(panels[rip.panels[0]].to_string()),
            NoticePlace::face(panels[rip.panels[1]].to_string()),
        ];
        for (side, panel) in rip.panels.iter().enumerate() {
            let id = PanelId {
                feature: uid,
                slot: *panel as u32,
            };
            for keep in &rip.wall[side] {
                clip(ctx, &mut state, id, (keep, thickness), clips, &places)?;
                clips += 4;
            }
        }
        // The gap recorded (mitcad#2098): a later flange on several edges
        // takes edge ends facing each other across it as one corner.
        state.gaps.push(crate::sheet::SheetGap {
            panels: rip.panels.map(|i| PanelId {
                feature: uid,
                slot: i as u32,
            }),
            line: rip.ends,
            width: gap,
        });
    }
    // Each bend's span along its edge, its panels cut back to its tangent
    // lines.
    let mut spans = Vec::new();
    for (n, b) in placed.iter().enumerate() {
        let places = vec![NoticePlace::edge(b.edge.to_string())];
        let from_id = PanelId {
            feature: uid,
            slot: b.from as u32,
        };
        let to_id = PanelId {
            feature: uid,
            slot: b.to as u32,
        };
        let (port, setback, w) = (&b.port, b.setback, b.port.width);
        let tool_a = TOOLS + 20 * n as u32;
        let tool_b = tool_a + 10;
        let corners = CORNERS + 20 * n as u32;
        // As far as the rips at the edge's ends let the bend run.
        let at_start = construction(port, -setback, 0.0, &b.metrics).cylinder;
        let (mut lo, mut hi) = (f64::NEG_INFINITY, f64::INFINITY);
        for rip in &rips {
            let touches = rip
                .ends
                .iter()
                .any(|p| b.ends.iter().any(|q| norm(sub(*p, *q)) <= MEET));
            if !touches {
                continue;
            }
            for (side, panel) in rip.panels.iter().enumerate() {
                if *panel != b.from && *panel != b.to {
                    continue;
                }
                for keep in &rip.bend[side] {
                    match bend_limit(&at_start, thickness, keep) {
                        Limit::Lower(v) => lo = lo.max(v),
                        Limit::Upper(v) => hi = hi.min(v),
                        Limit::Free => {}
                        Limit::Never => {
                            return Err(placed_at(
                                places.clone(),
                                "a bend would lie wholly past a rip at its end; make the corner \
                                 gap smaller",
                                format!(
                                    "bend edge {} would lie wholly past a rip at its end; make \
                                     the corner gap smaller",
                                    b.edge
                                ),
                            ));
                        }
                    }
                }
            }
        }
        // And as far as both faces reach its tangent lines: a wall whose
        // ends are not square to its bend runs past the bend at one end and
        // ends short of the edge's end at the other.
        let stationary = reach_along(
            ctx,
            &state,
            from_id,
            (
                rectangle(uid, tool_a, [-setback, 0.0], [1.0, w])?,
                port.to_operation,
            ),
            tool_a + 3,
        )?;
        let (min, max) = bounds(chart(&state, to_id)?)?;
        let big = 1.0 + 10.0 * (min[0].abs() + min[1].abs() + max[0].abs() + max[1].abs() + w);
        let moving = reach_along(
            ctx,
            &state,
            to_id,
            (
                rectangle(uid, tool_b, [-big, -big], [0.0, big])?,
                Isometry2::IDENTITY,
            ),
            tool_b + 1,
        )?;
        let no_room = || {
            placed_at(
                places.clone(),
                "the faces at a bend edge leave no room for its bend: they do not both reach the \
                 bend along the edge, or the rips at its ends take all of it",
                format!(
                    "bend edge {}: the faces on its two sides leave no room for its bend: they do \
                     not both reach the bend along the edge, or the rips at its ends take all of \
                     it",
                    b.edge
                ),
            )
        };
        let (Some(stationary), Some(moving)) = (stationary, moving) else {
            return Err(no_room());
        };
        lo = lo.max(stationary[0]).max(moving[0]);
        hi = hi.min(stationary[1]).min(moving[1]);
        if lo.abs() <= SNAP {
            lo = 0.0;
        }
        if (hi - w).abs() <= SNAP {
            hi = w;
        }
        if hi - lo <= TOLERANCE {
            return Err(no_room());
        }
        // The panel before the bend, cut back to its tangent line over the
        // bend and beside it to the edge's ends (a corner relief).
        let mut before = vec![rectangle(uid, tool_a, [-setback, lo], [1.0, hi])?];
        if lo > 0.0 {
            before.push(rectangle(uid, corners, [-setback, 0.0], [1.0, lo])?);
        }
        if hi < w {
            before.push(rectangle(uid, corners + 4, [-setback, hi], [1.0, w])?);
        }
        for tool in before {
            trim(
                ctx,
                &mut state,
                &[SheetDomainRef::Panel(from_id)],
                SheetTrimOperation::Difference,
                &[tool],
                &[port.to_operation],
            )?;
        }
        // The panel after it, cut back to its tangent line, beside the bend
        // too (past the edge's ends or the bend's ends at the rips).
        trim(
            ctx,
            &mut state,
            &[SheetDomainRef::Panel(to_id)],
            SheetTrimOperation::Difference,
            &[rectangle(uid, tool_b, [-setback - 1.0, lo], [0.0, hi])?],
            &[Isometry2::IDENTITY],
        )?;
        if bounds(chart(&state, to_id)?)?.0[0] < -TOLERANCE {
            for tool in [
                rectangle(uid, corners + 8, [-big, -big], [0.0, lo])?,
                rectangle(uid, corners + 12, [-big, hi], [0.0, big])?,
            ] {
                trim(
                    ctx,
                    &mut state,
                    &[SheetDomainRef::Panel(to_id)],
                    SheetTrimOperation::Difference,
                    &[tool],
                    &[Isometry2::IDENTITY],
                )?;
            }
        }
        spans.push((lo, hi));
    }
    // The bends between the cut panels.
    for (n, (b, (lo, hi))) in placed.iter().zip(spans).enumerate() {
        let edge = &b.edge;
        let from_id = PanelId {
            feature: uid,
            slot: b.from as u32,
        };
        let to_id = PanelId {
            feature: uid,
            slot: b.to as u32,
        };
        let tool_a = TOOLS + 20 * n as u32;
        let tool_b = tool_a + 10;
        let strip = |e: String| {
            placed_at(
                vec![NoticePlace::edge(edge.to_string())],
                format!(
                    "a bend edge: the faces on its two sides do not run the whole bend after \
                     their setbacks ({})",
                    crate::notice::user_message(&e)
                ),
                format!(
                    "bend edge {edge}: the faces on its two sides do not run the whole bend after \
                     their setbacks ({e})"
                ),
            )
        };
        let piece = piece_of(
            &state,
            from_id,
            SheetCurveId::Generated {
                feature: uid,
                slot: tool_a + 3,
            },
        )?;
        let mut axial = b.port.to_operation;
        axial.origin[1] -= lo;
        let stationary = side(chart(&state, from_id)?, &piece, axial).map_err(strip)?;
        let piece = piece_of(
            &state,
            to_id,
            SheetCurveId::Generated {
                feature: uid,
                slot: tool_b + 1,
            },
        )?;
        let shifted = Isometry2 {
            origin: [0.0, -lo],
            ..Isometry2::IDENTITY
        };
        let moving_side = side(chart(&state, to_id)?, &piece, shifted).map_err(strip)?;
        let cylinder = construction(&b.port, -b.setback, lo, &b.metrics).cylinder;
        add_bend_strip(
            ctx,
            &mut state,
            &b.metrics,
            cylinder,
            vec![stationary],
            vec![moving_side],
            hi - lo,
            None,
            n as u32,
        )
        .map_err(strip)?;
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
