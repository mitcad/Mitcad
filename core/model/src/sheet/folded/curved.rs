// SPDX-License-Identifier: MIT
//! Convert to Sheet Metal of a cylindrical or conical face, and its Flat
//! Pattern (mitcad#2435).
//!
//! A curved face picked as the Face becomes the one panel of a sheet whose
//! chart is the face developed at the neutral surface (the rules'
//! K-factor): a cylinder unrolled (`u` the angle about the axis times the
//! neutral radius, `v` along the axis), a cone unrolled into a fan about
//! its apex (lengths and angles of the neutral cone kept). Every face of
//! the body on the picked face's surface belongs to the panel (a cylinder
//! split into pieces by a loft's edges is one sheet); the surface may be
//! stored as a spline or another type within 1e-6 mm of the cylinder or
//! cone. The faces' boundaries and holes are read from their edges and
//! developed by the kernel ([`Kernel::sheet_face_development`]): a
//! generator is a line, a circle round the axis a line (a cylinder's) or an
//! arc about the apex (a cone's), from their exact ends; other edges are
//! B-splines through their exact development within 1e-8 mm. The pieces'
//! developments are united where they share edges.
//!
//! - The material lies behind the face, the rules' thickness deep: the
//!   body must be a plate of that thickness behind the face. The sheet of
//!   the whole face (a closed face in two halves) must have the body's
//!   volume and centre of mass; where it does not, the body must still be
//!   a plate (its other faces all on the surface the thickness behind the
//!   face, or walls from the face to that surface): then holes or ends
//!   whose walls are not normal to the material are cut normal to it, as
//!   the cutting pattern cuts them, with a warning that says by how much
//!   the sheet differs from the body.
//! - A closed face (once round its axis, or pieces that meet again round
//!   it) cannot lie flat in one piece: it is opened at its seam edge (or
//!   where the pieces meet again), the corner gap wide (half of it each side
//!   of the seam, a strip of constant width along the generator).
//! - A face that bends two ways (a sphere, a torus, most spline faces) is
//!   refused naming it: no development lays it flat without stretching.
//!
//! The Flat Pattern of such a sheet is its chart, laid in the plane that
//! touches the face's front at the chart's first boundary point.
//!
//! The face's curves are generated curves `g<1000000+>` of the feature, the
//! cuts at the seam `g<960000+>`, the halves of the plate check
//! `g<970000+>`, edges of the pieces' union of no piece `g<975000+>`.

use mitcad_vec3::{add, cross, dot, norm, scale, sub};

use super::develop::{orient, region_area, station};
use super::*;
use crate::datum::SurfaceGeometry;
use crate::features::face_refs::kernel_error;
use crate::features::sheet::{Allowance, SheetRulesDef};
use crate::features::sheet_convert::SheetConvertDef;
use crate::notice::NoticePlace;
use crate::topo::{FaceName, TopoName};

use mitcad_vec3::Vec3 as V3;

const FACE_CURVES: u32 = 1_000_000;
/// The cuts that open a closed face at its seam.
const SEAM: u32 = 960_000;
/// The cuts of the plate check's halves.
const HALVES: u32 = 970_000;
/// Edges of the faces' union that come from none of them.
const UNITED: u32 = 975_000;

/// A face that bends two ways at least this much (its Gaussian curvature
/// times the square of its size) cannot be laid flat; below it, it is flat
/// as far as the kernel can tell but no surface the sheet develops.
const BENDS_TWO_WAYS: f64 = 1e-6;

/// Surfaces of the body and their axes agree within this (mm, and the sine
/// of an angle): faces stored as splines are each recognised as a cylinder
/// or cone on their own (within 1e-6 mm over the face), so the axes and
/// radii read from two pieces of one surface differ by more than that. The
/// kernel's development then checks every piece's points on the picked
/// face's surface within 1e-5 mm.
const RECOGNITION: f64 = 1e-4;

/// The curved panel of a sheet (a cylindrical or conical panel), if any.
pub(crate) fn curved_panel(state: &SheetState) -> Option<PanelId> {
    state
        .charts
        .iter()
        .find_map(|p| match (p.domain, &p.embedding) {
            (SheetDomainRef::Panel(id), SheetEmbedding::Cylinder(_) | SheetEmbedding::Cone(_)) => {
                Some(id)
            }
            _ => None,
        })
}

/// A refusal about faces of the body: `said` in plain words for a person,
/// the places painted, `named` with their names for agents (mitcad#1612).
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

/// A surface type as a person reads it.
fn surface_words(kind: &str) -> String {
    match kind {
        "sphere" => "a spherical surface".into(),
        "torus" => "a toroidal surface".into(),
        "bspline" | "bezier" => "a freeform (spline) surface".into(),
        "revolution" => "a surface of revolution".into(),
        "extrusion" => "an extruded surface".into(),
        "offset" => "an offset surface".into(),
        other => format!("a {other} surface"),
    }
}

/// Why the face `name` is no panel of the sheet: a cylindrical or conical
/// face reached across a bend (it converts on its own), or a face that no
/// development lays flat without stretching, with how much it bends.
pub(super) fn not_flat<K: Kernel>(
    ctx: &EvalContext<'_, K>,
    shape: &K::Shape,
    name: &FaceName,
    surface: &SurfaceGeometry,
) -> String {
    let places = vec![NoticePlace::face(name.to_string())];
    if matches!(
        surface,
        SurfaceGeometry::Cylinder { .. } | SurfaceGeometry::Cone { .. }
    ) {
        return placed_at(
            places,
            "a cylindrical or conical face across a bend: only flat faces are joined by bends; \
             a curved face is converted on its own (pick it as the Face, without bend edges)",
            format!(
                "face {name} is cylindrical or conical: only flat faces are joined by bends; it \
                 is converted on its own (pick it as the face, without bend edges)"
            ),
        );
    }
    let kind = surface_words(surface.kind());
    let bending = ctx.kernel.face_bending(shape, name).ok();
    let two_ways = match bending {
        Some(b) => b.gaussian * b.size * b.size > BENDS_TWO_WAYS,
        None => matches!(
            surface,
            SurfaceGeometry::Sphere { .. } | SurfaceGeometry::Torus { .. }
        ),
    };
    if two_ways {
        let how = bending
            .filter(|b| b.gaussian > 0.0)
            .map(|b| {
                format!(
                    " (its Gaussian curvature reaches 1/({:.1} mm)\u{b2})",
                    1.0 / b.gaussian.sqrt()
                )
            })
            .unwrap_or_default();
        let said = format!(
            "the face cannot be laid flat: it is {kind} curved in two directions{how}, so its \
             material would have to stretch and no cutting pattern is made for it; form the part \
             another way (pressing, spinning or wheeling), or design it as separate developable \
             pieces: flat, cylindrical or conical faces"
        );
        return placed_at(places, said.clone(), format!("face {name}: {said}"));
    }
    let said = format!(
        "the face cannot be laid flat yet: it is {kind}, and only flat, cylindrical and conical \
         faces are developed into cutting patterns; model it of such faces"
    );
    placed_at(places, said.clone(), format!("face {name}: {said}"))
}

/// The K-factor a rolled face develops by: the rules' own, the middle for a
/// kerf bend's rules. Measured fold allowances do not apply to rolling.
fn rolled_k(rules: &SheetRulesDef<f64>) -> Result<f64, String> {
    match &rules.allowance {
        Allowance::KFactor { k } => Ok(*k),
        Allowance::KerfBend { .. } => Ok(0.5),
        _ => Err(
            "a curved face develops at the neutral surface of the rules' K-factor; these rules \
             give measured fold allowances, which do not apply to rolled material: use a \
             K-factor"
                .into(),
        ),
    }
}

fn rotated(v: V3, about: V3, angle: f64) -> V3 {
    // Rodrigues' rotation of v (across `about`, a unit vector).
    let (s, c) = angle.sin_cos();
    add(
        add(scale(v, c), scale(cross(about, v), s)),
        scale(about, dot(about, v) * (1.0 - c)),
    )
}

fn translation(dx: f64) -> Isometry2 {
    Isometry2 {
        origin: [dx, 0.0],
        ..Isometry2::IDENTITY
    }
}

fn rotation(angle: f64) -> Isometry2 {
    let (s, c) = angle.sin_cos();
    Isometry2 {
        origin: [0.0, 0.0],
        x_axis: [c, s],
        y_axis: [-s, c],
    }
}

/// The chart moved round its axis: the material that was at chart angle
/// `shift` (a cylinder's `u`, a cone's developed angle) is at 0 after it,
/// the embedding's start turned to it, its angle `extent`.
fn rebased(patch: &SheetPatch, shift: f64, extent: f64) -> SheetPatch {
    let (embedding, map) = match &patch.embedding {
        SheetEmbedding::Cylinder(c) => {
            // The angle runs from radial_start towards angular_direction:
            // about the axis by the right-hand rule, `sign` times it.
            let sign = dot(cross(c.axis, c.radial_start), c.angular_direction).signum();
            let turn = sign * shift / c.neutral_radius;
            let start = rotated(c.radial_start, c.axis, turn);
            let across = rotated(c.angular_direction, c.axis, turn);
            (
                SheetEmbedding::Cylinder(CylinderEmbedding {
                    radial_start: start,
                    angular_direction: across,
                    angle_extent: extent / c.neutral_radius,
                    ..c.clone()
                }),
                translation(-shift),
            )
        }
        SheetEmbedding::Cone(c) => {
            // The angle about the axis runs from radial_start towards
            // angular_direction; turned about the axis by the right-hand
            // rule, that is a turn of `sign` times it.
            let sign = dot(cross(c.axis, c.radial_start), c.angular_direction).signum();
            let turn = sign * shift / c.half_angle.sin();
            (
                SheetEmbedding::Cone(ConeEmbedding {
                    radial_start: rotated(c.radial_start, c.axis, turn),
                    angular_direction: rotated(c.angular_direction, c.axis, turn),
                    angle_extent: extent,
                    ..c.clone()
                }),
                rotation(-shift),
            )
        }
        plane => (plane.clone(), Isometry2::IDENTITY),
    };
    SheetPatch {
        domain: patch.domain,
        regions: mapped_regions(&patch.regions, map),
        embedding,
    }
}

/// Where the chart's material lies: the least and greatest `v` (a
/// cylinder's) or distance from the apex (a cone's).
fn reach(patch: &SheetPatch) -> Result<[f64; 2], String> {
    let mut out = [f64::INFINITY, f64::NEG_INFINITY];
    for edge in patch
        .regions
        .iter()
        .flat_map(|r| &r.loops)
        .flat_map(|l| &l.edges)
    {
        let (t0, t1) = (edge.curve.range.first, edge.curve.range.last);
        for i in 0..=32 {
            let p = edge.curve.point(t0 + (t1 - t0) * f64::from(i) / 32.0)?;
            let value = match patch.embedding {
                SheetEmbedding::Cone(_) => p[0].hypot(p[1]),
                _ => p[1],
            };
            out = [out[0].min(value), out[1].max(value)];
        }
    }
    if !out[0].is_finite() || !out[1].is_finite() {
        return Err("the face's development has no material".into());
    }
    Ok(out)
}

/// A wedge about the chart's origin from angle `from` to `to` (radians,
/// less than half a turn apart), `radius` long, counter-clockwise.
fn wedge(
    feature: FeatureUid,
    slot: u32,
    from: f64,
    to: f64,
    radius: f64,
) -> Result<MaterialRegion, String> {
    let at = |a: f64| [radius * a.cos(), radius * a.sin()];
    polygon(
        feature,
        slot,
        &[[0.0, 0.0], at(from), at((from + to) / 2.0), at(to)],
    )
}

/// The chart `patch` (in a state of its own) less `tools` (in coordinates
/// `map` takes the chart to).
fn trimmed<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    base: &SheetState,
    patch: &SheetPatch,
    tools: &[(MaterialRegion, Isometry2)],
) -> Result<SheetPatch, String> {
    let mut state = SheetState {
        charts: vec![patch.clone()],
        lineage: Vec::new(),
        ..base.clone()
    };
    for (tool, map) in tools {
        trim(
            ctx,
            &mut state,
            &[patch.domain],
            SheetTrimOperation::Difference,
            std::slice::from_ref(tool),
            &[*map],
        )?;
    }
    let out = state
        .charts
        .into_iter()
        .find(|p| p.domain == patch.domain)
        .ok_or("the face's development lost all its material")?;
    if out.regions.is_empty() {
        return Err("the face's development lost all its material".into());
    }
    Ok(out)
}

/// The closed face's chart (a full turn, from angle 0 to `full`) in two
/// halves, each rebased to start at 0: charts of less than a turn that the
/// kernel builds.
fn halves<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    base: &SheetState,
    patch: &SheetPatch,
    full: f64,
) -> Result<Vec<SheetPatch>, String> {
    let middle = full / 2.0;
    let [low, high] = reach(patch)?;
    let uid = ctx.uid;
    let (first, second) = match &patch.embedding {
        SheetEmbedding::Cone(_) => {
            // The development leaves `full` to a whole turn free; a quarter
            // of that past each end of the material.
            let past = (std::f64::consts::TAU - full) / 4.0;
            let radius = 2.0 * high + 1.0;
            (
                wedge(uid, HALVES, middle, full + past, radius)?,
                wedge(uid, HALVES + 4, -past, middle, radius)?,
            )
        }
        _ => (
            rectangle(uid, HALVES, [middle, low - 1.0], [full + 1.0, high + 1.0])?,
            rectangle(uid, HALVES + 4, [-1.0, low - 1.0], [middle, high + 1.0])?,
        ),
    };
    let one = trimmed(ctx, base, patch, &[(first, Isometry2::IDENTITY)])?;
    let other = trimmed(ctx, base, patch, &[(second, Isometry2::IDENTITY)])?;
    Ok(vec![
        rebased(&one, 0.0, middle),
        rebased(&other, middle, middle),
    ])
}

/// The closed face's chart opened at its seam (angle 0 and `full`): a strip
/// `gap` wide, half of it each side of the seam, taken away; the material
/// rebased to start at 0.
fn opened<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    base: &SheetState,
    patch: &SheetPatch,
    full: f64,
    gap: f64,
) -> Result<SheetPatch, String> {
    let [low, high] = reach(patch)?;
    let uid = ctx.uid;
    match &patch.embedding {
        SheetEmbedding::Cone(_) => {
            // Near the seam the material lies at distances `low..high` from
            // the apex; the strip runs along the seam's generator from just
            // short of `low` to past `high`, and a little across the seam
            // (less than the material on its other side comes near it).
            let near = (low * low - gap * gap / 4.0).max(0.0).sqrt() / 2.0;
            if gap >= low {
                return Err(
                    "the corner gap is wider than the cone's face is near its apex: make it \
                     smaller"
                        .into(),
                );
            }
            let free = std::f64::consts::TAU - full;
            let across = (gap / 2.0).min(near * free.min(std::f64::consts::FRAC_PI_2).sin() / 4.0);
            let start = polygon(
                uid,
                SEAM,
                &[
                    [near, -across],
                    [high + 1.0, -across],
                    [high + 1.0, gap / 2.0],
                    [near, gap / 2.0],
                ],
            )?;
            let end = polygon(
                uid,
                SEAM + 4,
                &[
                    [near, -gap / 2.0],
                    [high + 1.0, -gap / 2.0],
                    [high + 1.0, across],
                    [near, across],
                ],
            )?;
            let open = trimmed(
                ctx,
                base,
                patch,
                &[(start, Isometry2::IDENTITY), (end, rotation(-full))],
            )?;
            // The strips take at least this developed angle at each end
            // (where the material is farthest from the apex): the chart
            // turned by it, its angle less than a turn of the cone.
            let taken = (gap / (2.0 * (high + 1.0))).asin();
            Ok(rebased(&open, taken, full - 2.0 * taken))
        }
        _ => {
            let start = rectangle(uid, SEAM, [-1.0, low - 1.0], [gap / 2.0, high + 1.0])?;
            let end = rectangle(
                uid,
                SEAM + 4,
                [full - gap / 2.0, low - 1.0],
                [full + 1.0, high + 1.0],
            )?;
            let open = trimmed(
                ctx,
                base,
                patch,
                &[(start, Isometry2::IDENTITY), (end, Isometry2::IDENTITY)],
            )?;
            Ok(rebased(&open, gap / 2.0, full - gap))
        }
    }
}

/// The developed faces as one material: their regions united where they
/// meet at shared edges (the pieces of a split face). One region is given
/// back as it is.
fn united<K: Kernel>(
    ctx: &EvalContext<'_, K>,
    domain: SheetDomainRef,
    regions: Vec<MaterialRegion>,
) -> Result<Vec<MaterialRegion>, String> {
    let Some((first, others)) = regions.split_first() else {
        return Ok(regions);
    };
    if others.is_empty() {
        return Ok(regions);
    }
    let union = ctx
        .kernel
        .sheet_trim_domain(&SheetTrimSpec {
            operation: SheetTrimOperation::Union,
            domains: &[TrimDomain {
                domain,
                regions: vec![first.clone()],
                to_operation: Isometry2::IDENTITY,
            }],
            tools: others,
            seams: &[],
            tolerance: TOLERANCE,
        })
        .map_err(|e| format!("the faces' developments do not join: {}", kernel_error(e)))?;
    let mut slot = 0;
    Ok(union
        .domains
        .iter()
        .flat_map(|d| &d.regions)
        .map(|r| MaterialRegion {
            loops: r
                .loops
                .iter()
                .map(|l| MaterialLoop {
                    edges: l
                        .edges
                        .iter()
                        .map(|e| MaterialEdge {
                            piece: e
                                .lineage
                                .first()
                                .map(|s| match &s.source {
                                    TrimSource::Domain { boundary } => boundary.piece.clone(),
                                    TrimSource::Tool { piece } => piece.clone(),
                                })
                                .unwrap_or_else(|| {
                                    slot += 1;
                                    generated(ctx.uid, UNITED + slot)
                                }),
                            curve: e.curve.clone(),
                        })
                        .collect(),
                })
                .collect(),
        })
        .collect())
}

/// Every face's names and surface, faces stored as splines (or other types)
/// within 1e-6 mm of a cylinder or cone read as that one, as the faces the
/// panel is made of are picked (`face_geometry` reads them so; the list of
/// `face_geometries` gives them as `Other`).
fn face_surfaces<K: Kernel>(
    ctx: &EvalContext<'_, K>,
    shape: &K::Shape,
) -> Result<Vec<(Vec<String>, SurfaceGeometry)>, String> {
    let mut faces = ctx.kernel.face_geometries(shape).map_err(kernel_error)?;
    let indices: Vec<usize> = (0..faces.len()).collect();
    if let Ok(recognised) = ctx.kernel.recognised_face_geometries(shape, &indices)
        && recognised.len() == faces.len()
    {
        for ((_, geometry), read) in faces.iter_mut().zip(recognised) {
            *geometry = read;
        }
    }
    Ok(faces)
}

/// The surface `thickness` behind the face: another face of the body on it
/// lies on the sheet's back.
fn on_back(front: &SurfaceGeometry, back: &SurfaceGeometry, offset: f64) -> bool {
    let parallel = |a: V3, b: V3| norm(cross(a, b)) <= RECOGNITION;
    let on_axis = |origin: V3, point: V3, axis: V3| {
        let d = sub(point, origin);
        norm(sub(d, scale(axis, dot(d, axis)))) <= RECOGNITION
    };
    match (front, back) {
        (
            SurfaceGeometry::Cylinder {
                origin: o,
                axis: a,
                radius: r,
            },
            SurfaceGeometry::Cylinder {
                origin: p,
                axis: b,
                radius: s,
            },
        ) => parallel(*a, *b) && on_axis(*o, *p, *a) && (s - (r + offset)).abs() <= RECOGNITION,
        (
            SurfaceGeometry::Cone {
                origin: o,
                axis: a,
                radius: r,
                half_angle: h,
            },
            SurfaceGeometry::Cone {
                origin: p,
                axis: b,
                radius: s,
                half_angle: k,
            },
        ) => {
            if !parallel(*a, *b) || !on_axis(*o, *p, *a) {
                return false;
            }
            // Both radii at the front's origin, growing along `a`.
            let same = dot(*a, *b).signum();
            let grows = (h.tan() - same * k.tan()).abs() <= RECOGNITION;
            let there = s + dot(sub(*o, *p), *b) * k.tan();
            grows && (there - (r + offset / h.cos().abs())).abs() <= RECOGNITION
        }
        _ => false,
    }
}

/// The body is a plate behind its faces `front`: its other faces lie on the
/// surface `offset` (radially) from theirs, or are walls meeting both.
fn plate_faces<K: Kernel>(
    ctx: &EvalContext<'_, K>,
    shape: &K::Shape,
    front: &[FaceName],
    surface: &SurfaceGeometry,
    offset: f64,
) -> Result<bool, String> {
    let faces = face_surfaces(ctx, shape)?;
    let front_names: Vec<String> = front.iter().map(ToString::to_string).collect();
    let mut backs: Vec<&Vec<String>> = Vec::new();
    let mut walls: Vec<&Vec<String>> = Vec::new();
    for (names, geometry) in &faces {
        if names.iter().any(|n| front_names.contains(n)) {
            continue;
        }
        if on_back(surface, geometry, offset) {
            backs.push(names);
        } else {
            walls.push(names);
        }
    }
    if backs.is_empty() {
        return Ok(false);
    }
    let mut meets = vec![[false; 2]; walls.len()];
    for info in ctx.kernel.edges(shape).map_err(kernel_error)? {
        let Some(name) = info.name else { continue };
        let Ok(TopoName::Edge(edge)) = name.parse::<TopoName>() else {
            continue;
        };
        let [a, b] = edge.faces().clone().map(|f| f.to_string());
        for (k, wall) in walls.iter().enumerate() {
            for (own, other) in [(&a, &b), (&b, &a)] {
                if !wall.contains(own) {
                    continue;
                }
                if front_names.contains(other) {
                    meets[k][0] = true;
                }
                if backs.iter().any(|n| n.contains(other)) {
                    meets[k][1] = true;
                }
            }
        }
    }
    Ok(meets.iter().all(|m| m[0] && m[1]))
}

pub(super) fn convert<K: Kernel>(
    d: &SheetConvertDef<f64>,
    ctx: &mut EvalContext<'_, K>,
    shape: &K::Shape,
    name: &FaceName,
    surface: &SurfaceGeometry,
    base: SheetState,
    rules: &SheetRulesDef<f64>,
) -> Result<FeatureOutput<K::Shape>, String> {
    let places = vec![NoticePlace::face(name.to_string())];
    let cone = match surface {
        SurfaceGeometry::Cylinder { .. } => false,
        SurfaceGeometry::Cone { .. } => true,
        other => return Err(not_flat(ctx, shape, name, other)),
    };
    if let Some(edge) = d.bends.first().or(d.rips.first()) {
        return Err(placed_at(
            vec![NoticePlace::edge(edge.to_string())],
            "a curved face is converted on its own: bend and rip edges join flat faces only",
            format!(
                "edge {edge}: a curved face is converted on its own; bend and rip edges join \
                 flat faces only"
            ),
        ));
    }
    let t = rules.thickness;
    let k = rolled_k(rules)?;
    // Every face of the body on the picked face's surface is part of the
    // panel: a cylinder or cone split into pieces (the edges a loft's
    // sections leave on a neck extruded from its end) is one sheet.
    let picked = name.to_string();
    let mut fronts: Vec<FaceName> = vec![name.clone()];
    for (names, geometry) in face_surfaces(ctx, shape)? {
        if names.contains(&picked) || !on_back(surface, &geometry, 0.0) {
            continue;
        }
        if let Some(Ok(face)) = names.first().map(|n| n.parse::<FaceName>()) {
            fronts.push(face);
        }
    }
    let developed = ctx
        .kernel
        .sheet_face_development(shape, &fronts, k * t, t)
        .map_err(|e| {
            let e = kernel_error(e);
            placed_at(
                places.clone(),
                format!(
                    "the face could not be developed: {}",
                    crate::notice::user_message(&e)
                ),
                format!("face {name} could not be developed: {e}"),
            )
        })?;
    if cone != (developed.half_angle > 0.0) {
        return Err("the face's development is of another surface than the face's".into());
    }
    let full_turn = std::f64::consts::TAU;
    let closed = developed.closed;
    let inner_is_front = developed.inner_is_front;
    let embedding = if cone {
        SheetEmbedding::Cone(ConeEmbedding {
            apex: developed.origin,
            axis: developed.axis,
            radial_start: developed.radial_start,
            angular_direction: developed.angular_direction,
            half_angle: developed.half_angle,
            neutral_depth: k * t,
            angle_extent: developed.turn * developed.half_angle.sin(),
            inner_is_front,
        })
    } else {
        SheetEmbedding::Cylinder(CylinderEmbedding {
            centre: developed.origin,
            axis: developed.axis,
            radial_start: developed.radial_start,
            angular_direction: developed.angular_direction,
            inside_radius: developed.neutral_radius - k * t,
            neutral_radius: developed.neutral_radius,
            angle_extent: developed.turn,
            inner_is_front,
        })
    };
    // The chart's angle a whole turn spans: a cylinder's `u`, a cone's
    // developed angle.
    let full = if cone {
        full_turn * developed.half_angle.sin()
    } else {
        full_turn * developed.neutral_radius
    };
    let uid = ctx.uid;
    let mut slot = FACE_CURVES;
    let mut pieces = Vec::new();
    for face in developed.faces {
        let curves = face.into_iter().map(|e| e.geometry).collect();
        pieces.extend(
            crate::sheet::rederive::regions(uid, &mut slot, curves).map_err(|e| {
                placed_at(
                    places.clone(),
                    format!("the face's development: {e}"),
                    format!("face {name}'s development: {e}"),
                )
            })?,
        );
    }
    let panel = PanelId {
        feature: uid,
        slot: 0,
    };
    let mut regions = united(ctx, SheetDomainRef::Panel(panel), pieces)?;
    if regions.len() != 1 {
        return Err(placed_at(
            places,
            format!(
                "the face's development is not one region ({} found)",
                regions.len()
            ),
            format!(
                "the development of face {name} is not one region ({} found)",
                regions.len()
            ),
        ));
    }
    for region in &mut regions {
        orient(region)?;
    }
    let whole = SheetPatch {
        domain: SheetDomainRef::Panel(panel),
        regions,
        embedding,
    };
    // The plate check: the sheet of the whole face against the body.
    let pieces = if closed {
        halves(ctx, &base, &whole, full)?
    } else {
        vec![whole.clone()]
    };
    let body = ctx.kernel.mass_properties(shape).map_err(kernel_error)?;
    let (mut volume, mut moment) = (0.0, [0.0; 3]);
    for piece in &pieces {
        let geometry = ctx
            .kernel
            .sheet_shape(
                &SheetShapeSpec {
                    thickness: t,
                    patches: std::slice::from_ref(piece),
                    ports: &[],
                    tolerance: TOLERANCE,
                },
                &uid.to_string(),
            )
            .map_err(|e| {
                placed_at(
                    places.clone(),
                    format!("the sheet of the face could not be built: {e}"),
                    format!("the sheet of face {name} could not be built: {e}"),
                )
            })?;
        for solid in &geometry.solids {
            let m = ctx
                .kernel
                .mass_properties(&solid.shape)
                .map_err(kernel_error)?;
            volume += m.volume;
            moment = add(moment, scale(m.center, m.volume));
        }
    }
    let center = scale(moment, 1.0 / volume.max(f64::MIN_POSITIVE));
    let size = body.volume.abs().cbrt().max(1.0);
    let same = (volume - body.volume).abs() <= 1e-6 * body.volume.abs().max(1.0)
        && (0..3).all(|i| (center[i] - body.center[i]).abs() <= 1e-6 * size);
    if !same {
        // The surface the thickness behind the face, away from the axis
        // for an inner face and towards it for an outer one.
        let back = if inner_is_front { t } else { -t };
        if !plate_faces(ctx, shape, &fronts, surface, back)? {
            return Err(placed_at(
                places,
                format!(
                    "the body is not a plate of its rules' thickness {t} mm behind the face: a \
                     curved face converts with the body made of it alone, its back the \
                     thickness behind it and walls between them (a body of several curved \
                     faces, such as a transition, is converted face by face, split first)"
                ),
                format!(
                    "the body is not a plate of its rules' thickness {t} mm behind face {name} \
                     (its sheet has volume {volume:.6} mm\u{b3}, the body {:.6} mm\u{b3})",
                    body.volume
                ),
            ));
        }
        ctx.warn(format!(
            "walls of the face's ends or holes are not normal to the material: the sheet cuts \
             them normal to it, as its cutting pattern does, so it differs from the body by \
             {:.3} mm\u{b3}",
            (volume - body.volume).abs()
        ));
    }
    let sheet = if closed {
        let gap = d.corner_gap.unwrap_or(t);
        if !gap.is_finite() || gap <= TOLERANCE {
            return Err(placed_at(
                places,
                "a closed face is opened at its seam to lie flat: the corner gap there must be \
                 above 0",
                format!(
                    "face {name} is closed: it is opened at its seam to lie flat, and the corner \
                     gap there must be above 0"
                ),
            ));
        }
        opened(ctx, &base, &whole, full, gap)?
    } else {
        rebased(&whole, 0.0, full * developed.turn / full_turn)
    };
    let mut state = base;
    state.charts = vec![sheet];
    publish(ctx, state, rules, None)
}

/// The front's model point and the chart's unit directions there, with the
/// direction from the front to the back.
fn tangent_frame(embedding: &SheetEmbedding, p: [f64; 2], thickness: f64) -> Option<[V3; 4]> {
    match embedding {
        SheetEmbedding::Cylinder(_) => {
            let s = station(embedding, p[0], thickness);
            Some([add(s.origin, scale(s.y, p[1])), s.x, s.y, s.f])
        }
        SheetEmbedding::Cone(c) => {
            let ([radial, angular], _) = c.frame(p, 0.0, thickness);
            let (s, k) = c.half_angle.sin_cos();
            let generator: V3 = std::array::from_fn(|i| s * radial[i] + k * c.axis[i]);
            let normal: V3 = std::array::from_fn(|i| k * radial[i] - s * c.axis[i]);
            let phi = c.developed_angle(p);
            let (sp, cp) = phi.sin_cos();
            let x: V3 = std::array::from_fn(|i| cp * generator[i] - sp * angular[i]);
            let y: V3 = std::array::from_fn(|i| sp * generator[i] + cp * angular[i]);
            let f = if c.inner_is_front {
                normal
            } else {
                scale(normal, -1.0)
            };
            Some([c.point(p, 0.0, thickness), x, y, f])
        }
        SheetEmbedding::Plane { .. } => None,
    }
}

/// The developed sheet of a body whose fixed panel is curved, for its Flat
/// Pattern: the panel's chart (already its development) laid in the plane
/// touching its front at the chart's first boundary point. None when the
/// panel is flat; an error for a curved panel among other walls.
pub(crate) fn developed(
    state: &SheetState,
    panel: PanelId,
    thickness: f64,
) -> Option<Result<SheetState, String>> {
    let domain = SheetDomainRef::Panel(panel);
    let patch = state.charts.iter().find(|p| p.domain == domain)?;
    if matches!(patch.embedding, SheetEmbedding::Plane { .. }) {
        return None;
    }
    Some((|| {
        if state.charts.len() != 1 || !state.bends.is_empty() {
            return Err("a curved panel is developed only as a sheet of its own".to_owned());
        }
        let edge = patch
            .regions
            .iter()
            .flat_map(|r| &r.loops)
            .flat_map(|l| &l.edges)
            .next()
            .ok_or("the curved panel has no material")?;
        let at = edge.curve.point(edge.curve.range.first)?;
        let [origin, x, y, f] =
            tangent_frame(&patch.embedding, at, thickness).ok_or("the panel is flat")?;
        let front_frame = SketchFrame {
            origin: sub(origin, add(scale(x, at[0]), scale(y, at[1]))),
            x_axis: x,
            y_axis: y,
        };
        let sign = if dot(f, cross(x, y)) > 0.0 {
            MaterialParity::Positive
        } else {
            MaterialParity::Negative
        };
        let mut out = state.clone();
        out.charts = vec![SheetPatch {
            domain,
            regions: patch.regions.clone(),
            embedding: SheetEmbedding::Plane {
                front_frame,
                front_to_back_sign: sign,
            },
        }];
        // The developed area must be the chart's.
        let area: f64 = patch
            .regions
            .iter()
            .map(region_area)
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .sum();
        if area.is_nan() || area <= 0.0 {
            return Err("the curved panel has no area".to_owned());
        }
        Ok(out)
    })())
}

#[cfg(test)]
#[path = "curved_tests.rs"]
mod tests;
