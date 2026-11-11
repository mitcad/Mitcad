// SPDX-License-Identifier: MIT
//! The sheet-domain adapter of ordinary features (mitcad#242).
//!
//! An ordinary feature (an extrusion, a hole, a mirror or pattern, a
//! fillet, a library feature, an import's stored body) replaces a sheet
//! body's shape without sheet semantics. Where its result is still the
//! sheet's material changed through the thickness of planar charts (a cut,
//! a hole, a notch, a rounded or bevelled contour corner), the sheet's
//! domains are re-derived from the result, so later sheet features keep
//! working on it:
//!
//! - every planar chart's new material outline is read from the result's
//!   faces on the chart's front surface (exact edge curves mapped into the
//!   chart's coordinates);
//! - the material it lost is removed from the chart exactly as `sheet_cut`
//!   removes regions, in one trim per plane whose seams are the registered
//!   ports, so the curves it keeps keep their identities and the new ones
//!   are generated curves of the feature (`F<feature>/g<k>`);
//! - the re-derived sheet is rebuilt and must be the result: the same
//!   volume, area and centre of mass. Bends are not re-derived: a change
//!   of a bend, added material, a change of thickness or walls that are
//!   not normal to the material make the rebuild differ, and the sheet
//!   meaning is lost with that reason.

use mitcad_vec3::sub;

use super::folded::{cut::developed_cut, develop::planar_map, mapped_regions, read};
use super::*;
use crate::features::{BodyChange, EvalContext, FeatureOutput};
use crate::kernel::{Curve3, Kernel, KernelError};
use crate::profile::SegmentGeometry;
use mitcad_vec3::dot;

use mitcad_vec3::Vec3 as V3;

/// How close an edge's end must be to the next edge's start in a face's
/// loop, and a face to a chart's surface (mm).
const RECOGNITION: f64 = 1.0e-6;

pub(super) const NOT_DERIVED: &str = "ordinary modeling replaced this body and the sheet-domain adapter \
                           could not re-derive its material";

/// Re-derives the sheet of every valid sheet body the feature replaced
/// without a sheet change of its own (see the module documentation).
pub(crate) fn adapt<K: Kernel>(ctx: &mut EvalContext<'_, K>, output: &mut FeatureOutput<K::Shape>) {
    let bodies: Vec<BodyUid> = output
        .changes
        .iter()
        .filter_map(|c| match c {
            BodyChange::Set(body, _) => Some(*body),
            BodyChange::Remove(_) => None,
        })
        .filter(|body| !output.sheets.contains_key(body))
        .filter(|body| {
            ctx.env
                .sheets
                .get(body)
                .is_some_and(|s| s.value.status() == "valid")
        })
        .collect();
    for body in bodies {
        let Some(shape) = output.changes.iter().find_map(|c| match c {
            BodyChange::Set(b, shape) if *b == body => Some(shape.clone()),
            _ => None,
        }) else {
            continue;
        };
        let warnings = ctx.warnings.len();
        let result = adapted(ctx, body, &shape);
        ctx.warnings.truncate(warnings);
        match result {
            Ok(derived) => {
                output.sheets.insert(body, SheetChange::Set(derived.state));
                // A cut across a bend is the sheet's material: through the
                // thickness, normal to it (mitcad#1628).
                if let Some(rebuilt) = derived.shape {
                    for change in &mut output.changes {
                        if let BodyChange::Set(b, s) = change
                            && *b == body
                        {
                            *s = rebuilt.clone();
                        }
                    }
                }
                if let Some(warning) = derived.warning {
                    ctx.warn(warning);
                }
            }
            // Without the kernel's face queries the body stays an
            // ordinary solid, as before.
            Err(Failure::Unsupported) => {}
            Err(Failure::Reason(reason)) => {
                let change = match dressup_overlay(ctx, body, &shape) {
                    Ok(Some(state)) => SheetChange::Set(state),
                    _ => SheetChange::Invalidate(format!("{NOT_DERIVED}: {reason}")),
                };
                output.sheets.insert(body, change);
            }
        }
    }
}

/// An ordinary fillet or chamfer whose result the domains cannot hold (a
/// countersink at a hole's edge, a rounded surface edge) is kept as an
/// overlay of the sheet (mitcad#242): the material it changed is made
/// again on every rebuilt solid, and the developed form is refused. None
/// for other features.
fn dressup_overlay<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    body: BodyUid,
    shape: &K::Shape,
) -> Result<Option<SheetState>, String> {
    let def = ctx.feature_entry(ctx.uid)?.def.clone();
    if !matches!(
        def,
        crate::features::FeatureDef::Fillet(_) | crate::features::FeatureDef::Chamfer(_)
    ) {
        return Ok(None);
    }
    let (mut state, rules) = read(ctx, body)?;
    let before = ctx.body(body)?;
    let volume = |shape: &K::Shape| -> Result<f64, String> {
        Ok(ctx
            .kernel
            .mass_properties(shape)
            .map_err(|e| e.to_string())?
            .volume)
    };
    // The domains and earlier overlays must give the body it changed.
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
        .map_err(|e| super::folded::name_walls(ctx, &e.to_string()))?;
    let [solid] = geometry.solids.as_slice() else {
        return Ok(None);
    };
    let rebuilt = super::folded::cut::with_overlays(ctx, &state, solid.shape.clone())?;
    let (v0, v1) = (volume(&before)?, volume(&rebuilt)?);
    if (v0 - v1).abs() > 1e-6 * v0.abs().max(1.0) {
        return Ok(None);
    }
    state.overlays.push(SheetOverlay {
        feature: ctx.uid,
        kind: SheetOverlayKind::Change {
            placement: crate::transform::Transform::IDENTITY,
            source: None,
        },
        removed: v0 - volume(shape)?,
    });
    state.shape_generation.clear();
    Ok(Some(state))
}

enum Failure {
    Unsupported,
    Reason(String),
}

impl From<String> for Failure {
    fn from(reason: String) -> Self {
        Failure::Reason(reason)
    }
}

impl From<&str> for Failure {
    fn from(reason: &str) -> Self {
        Failure::Reason(reason.into())
    }
}

fn kernel<T>(result: Result<T, KernelError>) -> Result<T, Failure> {
    result.map_err(|e| match e {
        KernelError::Unsupported(_) => Failure::Unsupported,
        other => Failure::Reason(other.to_string()),
    })
}

fn rederive<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    body: BodyUid,
    shape: &K::Shape,
) -> Result<SheetState, Failure> {
    let (state, rules) = read(ctx, body)?;
    let before = ctx.body(body)?;
    derive(ctx, state, &rules, &before, shape)
}

#[path = "rederive_bends.rs"]
mod bends;

/// A sheet re-derived for an ordinary feature: its state, the body's shape
/// where it is not the feature's result (a cut across a bend, made normal
/// to the material), and the warning that says so.
pub(super) struct Derived<S> {
    state: SheetState,
    shape: Option<S>,
    warning: Option<String>,
}

/// The sheet of `body` re-derived from the feature's result `shape`: a cut
/// across bends as Sheet Cut makes it (mitcad#1628), otherwise a change of
/// planar charts.
fn adapted<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    body: BodyUid,
    shape: &K::Shape,
) -> Result<Derived<K::Shape>, Failure> {
    let (state, rules) = read(ctx, body)?;
    let before = ctx.body(body)?;
    if let Some(derived) = bends::across_bends(ctx, state.clone(), &rules, &before, shape)? {
        return Ok(derived);
    }
    Ok(Derived {
        state: derive(ctx, state, &rules, &before, shape)?,
        shape: None,
        warning: None,
    })
}

/// The sheet of `body` re-derived from `shape`, as [`adapt`] would, for a
/// sheet a copy of a pattern joined without carrying it (mitcad#1462);
/// None where the adapter cannot.
pub(super) fn rederive_joined<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    body: BodyUid,
    shape: &K::Shape,
) -> Option<SheetState> {
    let warnings = ctx.warnings.len();
    let result = rederive(ctx, body, shape).ok();
    ctx.warnings.truncate(warnings);
    result
}

/// The sheet `state` of `before` re-derived from `shape`, a change of its
/// material (the straight sheet cut's first choice, mitcad#242).
pub(crate) fn rederive_from<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    state: SheetState,
    rules: &crate::features::sheet::SheetRulesDef<f64>,
    before: &K::Shape,
    shape: &K::Shape,
) -> Result<SheetState, String> {
    derive(ctx, state, rules, before, shape).map_err(|f| match f {
        Failure::Unsupported => "the geometry kernel cannot read the result's faces".into(),
        Failure::Reason(reason) => reason,
    })
}

fn derive<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    mut state: SheetState,
    rules: &crate::features::sheet::SheetRulesDef<f64>,
    before: &K::Shape,
    shape: &K::Shape,
) -> Result<SheetState, Failure> {
    let thickness = rules.thickness;
    let faces = kernel(ctx.kernel.face_geometries(shape))?;
    // Planar charts grouped by plane and material direction: one trim each.
    let mut groups: Vec<Vec<usize>> = Vec::new();
    for (i, patch) in state.charts.iter().enumerate() {
        if !matches!(patch.embedding, SheetEmbedding::Plane { .. }) {
            continue;
        }
        match groups
            .iter_mut()
            .find(|g| planar_map(&patch.embedding, &state.charts[g[0]].embedding).is_ok())
        {
            Some(group) => group.push(i),
            None => groups.push(vec![i]),
        }
    }
    let mut slot = 0;
    for group in groups {
        // A panel of the plane holds it (a developed bend alone is not
        // re-derived).
        let Some(&lead) = group
            .iter()
            .find(|&&i| matches!(state.charts[i].domain, SheetDomainRef::Panel(_)))
        else {
            continue;
        };
        let stationary = state.charts[lead].clone();
        let SheetEmbedding::Plane {
            front_frame,
            front_to_back_sign,
        } = stationary.embedding
        else {
            continue;
        };
        let SheetDomainRef::Panel(panel) = stationary.domain else {
            continue;
        };
        let f = front_frame.normal().map(|v| v * front_to_back_sign.sign());
        // The new outline: the result's faces on the front surface.
        let outline = front_outline(ctx, &faces, shape, &front_frame, f, &mut slot)?;
        // The material each chart of the plane lost: against its domains,
        // or, where straight cuts the domains do not hold are on the sheet,
        // against the input's own front outline (so that only this
        // feature's change is taken).
        let mut removed = Vec::new();
        if !state.overlays.is_empty() {
            let before_faces = kernel(ctx.kernel.face_geometries(before))?;
            let old = front_outline(ctx, &before_faces, before, &front_frame, f, &mut slot)?;
            let result = kernel(ctx.kernel.sheet_trim_domain(&SheetTrimSpec {
                operation: SheetTrimOperation::Difference,
                domains: &[TrimDomain {
                    domain: stationary.domain,
                    regions: old,
                    to_operation: Isometry2::IDENTITY,
                }],
                tools: &outline,
                seams: &[],
                tolerance: TOLERANCE,
            }))?;
            for domain in result.domains {
                removed.extend(settled_regions(ctx.uid, &mut slot, &domain.regions)?);
            }
        }
        for &i in group.iter().filter(|_| state.overlays.is_empty()) {
            let patch = &state.charts[i];
            let map = planar_map(&patch.embedding, &stationary.embedding)?;
            let result = kernel(ctx.kernel.sheet_trim_domain(&SheetTrimSpec {
                operation: SheetTrimOperation::Difference,
                domains: &[TrimDomain {
                    domain: patch.domain,
                    regions: patch.regions.clone(),
                    to_operation: map,
                }],
                tools: &outline,
                seams: &[],
                tolerance: TOLERANCE,
            }))?;
            for domain in result.domains {
                let lost = settled_regions(ctx.uid, &mut slot, &domain.regions)?;
                removed.extend(mapped_regions(&lost, map));
            }
        }
        if !removed.is_empty() {
            let outcome = developed_cut(
                ctx,
                &state,
                thickness,
                panel,
                &[],
                &removed,
                "the ordinary feature's change",
            )?;
            state = outcome.state;
        }
        // The material it added in the plane: united with the chart it
        // touches (a joined face, the copy of one, mitcad#242).
        let mut existing = Vec::new();
        for patch in state.charts.iter() {
            if let Ok(map) = planar_map(&patch.embedding, &stationary.embedding) {
                existing.extend(mapped_regions(&patch.regions, map));
            }
        }
        let result = kernel(ctx.kernel.sheet_trim_domain(&SheetTrimSpec {
            operation: SheetTrimOperation::Difference,
            domains: &[TrimDomain {
                domain: stationary.domain,
                regions: outline.clone(),
                to_operation: Isometry2::IDENTITY,
            }],
            tools: &existing,
            seams: &[],
            tolerance: TOLERANCE,
        }))?;
        let mut added = Vec::new();
        for domain in result.domains {
            added.extend(added_regions(ctx.uid, &mut slot, &domain.regions)?);
        }
        for region in added {
            let mut joined = None;
            for patch in &state.charts {
                let Ok(map) = planar_map(&patch.embedding, &stationary.embedding) else {
                    continue;
                };
                let SheetDomainRef::Panel(_) = patch.domain else {
                    continue;
                };
                let tool = mapped_regions(
                    std::slice::from_ref(&region),
                    super::folded::develop::inverse(map),
                );
                let mut trial = state.clone();
                let before = patch.regions.len();
                super::folded::trim_with_lineage(
                    ctx,
                    &mut trial,
                    &[patch.domain],
                    SheetTrimOperation::Union,
                    &tool,
                    &[Isometry2::IDENTITY],
                    Some(patch.domain),
                )?;
                let after = trial
                    .charts
                    .iter()
                    .find(|p| p.domain == patch.domain)
                    .map_or(usize::MAX, |p| p.regions.len());
                if after <= before {
                    joined = Some(trial);
                    break;
                }
            }
            state = joined.ok_or("it added material that touches no panel in its plane")?;
        }
    }
    // The re-derived sheet must be the result.
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
    let rebuilt = super::folded::cut::with_overlays(ctx, &state, solid.shape.clone())?;
    let rebuilt = kernel(ctx.kernel.mass_properties(&rebuilt))?;
    let actual = kernel(ctx.kernel.mass_properties(shape))?;
    let size = actual.volume.abs().cbrt().max(1.0);
    let off = |a: f64, b: f64, scale: f64| (a - b).abs() > 1e-6 * scale.max(1.0);
    if off(rebuilt.volume, actual.volume, actual.volume.abs())
        || off(rebuilt.area, actual.area, actual.area.abs())
        || (0..3).any(|i| off(rebuilt.center[i], actual.center[i], size))
    {
        return Err(format!(
            "the result is not the sheet's material changed through its thickness (the \
             re-derived sheet has volume {:.6} and area {:.6}, the result {:.6} and {:.6}): \
             material added, a bend changed or walls not normal to the material",
            rebuilt.volume, rebuilt.area, actual.volume, actual.area
        )
        .into());
    }
    finished(state, rules)
}

/// The re-derived `state` with its panels projected again from its charts,
/// ready to pair with a new shape.
fn finished(
    mut state: SheetState,
    rules: &crate::features::sheet::SheetRulesDef<f64>,
) -> Result<SheetState, Failure> {
    state.panels = Vec::new();
    for patch in &state.charts {
        let SheetDomainRef::Panel(id) = patch.domain else {
            continue;
        };
        let SheetEmbedding::Plane {
            front_frame,
            front_to_back_sign,
        } = &patch.embedding
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
        for region in &patch.regions {
            let loops = region
                .loops
                .iter()
                .map(|l| {
                    Ok(SheetLoop {
                        segments: l
                            .edges
                            .iter()
                            .map(|e| {
                                Ok(SheetSegment {
                                    id: e.piece.curve.clone(),
                                    geometry: e.curve.geometry_piece()?,
                                })
                            })
                            .collect::<Result<_, String>>()?,
                    })
                })
                .collect::<Result<_, String>>()?;
            state.panels.push(SheetPlanarPatch {
                id,
                frame,
                region: SheetRegion { loops },
            });
        }
    }
    state.shape_generation.clear();
    Ok(state)
}

/// The material regions of a shape's faces on the front plane of a chart
/// (`front` its frame, `f` its front-to-back direction), in the chart's
/// coordinates; each face's loops on their own (faces of one plane may
/// share edges).
fn front_outline<K: Kernel>(
    ctx: &EvalContext<'_, K>,
    faces: &[(Vec<String>, crate::datum::SurfaceGeometry)],
    shape: &K::Shape,
    front: &crate::profile::SketchFrame,
    f: V3,
    slot: &mut u32,
) -> Result<Vec<MaterialRegion>, Failure> {
    let mut outline = Vec::new();
    for (names, surface) in faces {
        let crate::datum::SurfaceGeometry::Plane { origin, normal } = surface else {
            continue;
        };
        if dot(*normal, f).abs() < 1.0 - 1e-9
            || dot(sub(*origin, front.origin), f).abs() > RECOGNITION
        {
            continue;
        }
        let Some(name) = names.first() else {
            continue;
        };
        // A thickness wall of another domain lying in this plane (the end
        // faces of a box's side flanges in the plane of its end wall's
        // surface, mitcad#280) is no material of the plane's surface.
        if matches!(
            super::pick::parse_face(name),
            Some(super::pick::PickedFace::Wall(..))
        ) {
            continue;
        }
        let name: crate::topo::TopoName = name.parse().map_err(|_| {
            crate::notice::FeatureError::new("a face of the result has no usable name")
                .with_detail(format!("the result's face {name} has no usable name"))
                .text()
        })?;
        let mut curves = Vec::new();
        for curve in kernel(ctx.kernel.curves_of(shape, &name))? {
            curves.push(planar_curve(&curve, front)?);
        }
        outline.extend(regions(ctx.uid, slot, curves)?);
    }
    Ok(outline)
}

/// A model-space edge on a chart's plane in the chart's coordinates.
pub(crate) fn planar_curve(
    curve: &Curve3,
    frame: &crate::profile::SketchFrame,
) -> Result<SegmentGeometry, String> {
    let at = |p: V3| {
        let d = sub(p, frame.origin);
        [dot(d, frame.x_axis), dot(d, frame.y_axis)]
    };
    let vector = |v: V3| [dot(v, frame.x_axis), dot(v, frame.y_axis)];
    Ok(match curve {
        Curve3::Point(_) => return Err("a face's boundary has a degenerate edge".into()),
        Curve3::Helix { .. } => return Err("a face's boundary has a helical edge".into()),
        Curve3::Line { start, end } => SegmentGeometry::Line {
            start: at(*start),
            end: at(*end),
        },
        Curve3::Conic {
            center,
            normal,
            x_axis,
            major,
            minor,
            start,
            end,
            closed,
        } => {
            let u = vector(*x_axis);
            let rotation = u[1].atan2(u[0]);
            // A conic seen from the other side runs the other way.
            let same = dot(*normal, frame.normal()) > 0.0;
            let (a, b) = if same {
                (*start, *end)
            } else {
                (-*end, -*start)
            };
            let c = at(*center);
            if (major - minor).abs() <= RECOGNITION {
                if *closed {
                    SegmentGeometry::Circle {
                        center: c,
                        radius: *major,
                        start: 0.0,
                    }
                } else {
                    SegmentGeometry::Arc {
                        center: c,
                        radius: *major,
                        start_angle: rotation + a,
                        end_angle: rotation + b,
                    }
                }
            } else if *closed {
                SegmentGeometry::Ellipse {
                    center: c,
                    major_radius: *major,
                    minor_radius: *minor,
                    rotation,
                }
            } else {
                SegmentGeometry::EllipseArc {
                    center: c,
                    major_radius: *major,
                    minor_radius: *minor,
                    rotation,
                    start_angle: a,
                    end_angle: b,
                }
            }
        }
        Curve3::BSpline {
            degree,
            poles,
            weights,
            knots,
        } => {
            let mut distinct: Vec<f64> = Vec::new();
            let mut multiplicities: Vec<u32> = Vec::new();
            for k in knots {
                match distinct.last() {
                    Some(last) if (k - last).abs() <= 1e-12 => {
                        *multiplicities.last_mut().expect("paired") += 1;
                    }
                    _ => {
                        distinct.push(*k);
                        multiplicities.push(1);
                    }
                }
            }
            let spline = SegmentGeometry::BSpline {
                degree: *degree,
                poles: poles.iter().map(|p| at(*p)).collect(),
                weights: weights.clone(),
                knots: distinct,
                multiplicities,
                periodic: false,
            };
            // The sheet construction gives its straight and circular walls
            // spline edges: they are lines and arcs again in the chart.
            exact_form(spline)?
        }
    })
}

/// A spline that is a straight segment or a circular arc (within 1e-9 mm
/// at every sample) as that line or arc; otherwise the spline.
fn exact_form(spline: SegmentGeometry) -> Result<SegmentGeometry, String> {
    let curve = BoundedCurve2::from_geometry(spline.clone())?;
    let samples: Vec<[f64; 2]> = (0..=32)
        .map(|k| {
            curve.point(
                curve.range.first + (curve.range.last - curve.range.first) * f64::from(k) / 32.0,
            )
        })
        .collect::<Result<_, _>>()?;
    let (a, b) = (samples[0], samples[32]);
    let d = crate::sketch::geometry::dist(a, b);
    let off_line =
        |p: [f64; 2]| ((b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0])).abs() / d;
    if d > RECOGNITION && samples.iter().all(|p| off_line(*p) <= 1e-9) {
        return Ok(SegmentGeometry::Line { start: a, end: b });
    }
    // The circle through the ends and the middle.
    let m = samples[16];
    let (ax, ay, bx, by, cx, cy) = (a[0], a[1], m[0], m[1], b[0], b[1]);
    let det = 2.0 * (ax * (by - cy) + bx * (cy - ay) + cx * (ay - by));
    if det.abs() > 1e-12 {
        let sq = |x: f64, y: f64| x * x + y * y;
        let center = [
            (sq(ax, ay) * (by - cy) + sq(bx, by) * (cy - ay) + sq(cx, cy) * (ay - by)) / det,
            (sq(ax, ay) * (cx - bx) + sq(bx, by) * (ax - cx) + sq(cx, cy) * (bx - ax)) / det,
        ];
        let radius = crate::sketch::geometry::dist(center, a);
        if samples
            .iter()
            .all(|p| (crate::sketch::geometry::dist(center, *p) - radius).abs() <= 1e-9)
        {
            let angle = |p: [f64; 2]| (p[1] - center[1]).atan2(p[0] - center[0]);
            // Counter-clockwise from start to end through the middle.
            let ccw = (m[0] - a[0]) * (b[1] - a[1]) - (m[1] - a[1]) * (b[0] - a[0]) > 0.0;
            let (s, e) = if ccw { (a, b) } else { (b, a) };
            let start_angle = angle(s);
            let mut end_angle = angle(e);
            while end_angle <= start_angle {
                end_angle += std::f64::consts::TAU;
            }
            return Ok(SegmentGeometry::Arc {
                center,
                radius,
                start_angle,
                end_angle,
            });
        }
    }
    Ok(spline)
}

/// The material regions bounded by a plane's face edges: the edges chained
/// into closed loops, each loop an exterior or a hole by how many others
/// contain it. Every edge gets a generated identity of the feature.
pub(crate) fn regions(
    feature: FeatureUid,
    slot: &mut u32,
    curves: Vec<SegmentGeometry>,
) -> Result<Vec<MaterialRegion>, String> {
    let mut edges = Vec::new();
    for geometry in curves {
        let bounded = BoundedCurve2::from_geometry(geometry.clone())?;
        let ends = bounded.ends()?;
        edges.push((geometry, ends));
    }
    // Chain the edges by their ends (closed curves are loops of their own).
    let mut used = vec![false; edges.len()];
    let mut loops: Vec<Vec<SegmentGeometry>> = Vec::new();
    for first in 0..edges.len() {
        if used[first] {
            continue;
        }
        used[first] = true;
        let (start, mut end) = edges[first].1;
        let mut chain = vec![edges[first].0.clone()];
        while crate::sketch::geometry::dist(start, end) > RECOGNITION {
            let next = (0..edges.len()).find(|&k| {
                !used[k]
                    && (crate::sketch::geometry::dist(edges[k].1.0, end) <= RECOGNITION
                        || crate::sketch::geometry::dist(edges[k].1.1, end) <= RECOGNITION)
            });
            let Some(k) = next else {
                return Err("a face's boundary on the sheet's surface is not closed".into());
            };
            used[k] = true;
            end = if crate::sketch::geometry::dist(edges[k].1.0, end) <= RECOGNITION {
                edges[k].1.1
            } else {
                edges[k].1.0
            };
            chain.push(edges[k].0.clone());
        }
        loops.push(chain);
    }
    // Nesting: a loop inside an even number of others is an exterior.
    // (Each edge sampled in the direction the chain runs it.)
    let polygon = |l: &[SegmentGeometry]| -> Result<Vec<[f64; 2]>, String> {
        let mut points: Vec<[f64; 2]> = Vec::new();
        let mut end: Option<[f64; 2]> = None;
        for g in l {
            let c = BoundedCurve2::from_geometry(g.clone())?;
            let mut samples = (0..=16)
                .map(|k| {
                    c.point(c.range.first + (c.range.last - c.range.first) * f64::from(k) / 16.0)
                })
                .collect::<Result<Vec<_>, String>>()?;
            if let Some(last) = end
                && crate::sketch::geometry::dist(last, samples[0])
                    > crate::sketch::geometry::dist(last, samples[16])
            {
                samples.reverse();
            }
            end = Some(samples[16]);
            points.extend(&samples[..16]);
        }
        Ok(points)
    };
    let polygons: Vec<Vec<[f64; 2]>> =
        loops.iter().map(|l| polygon(l)).collect::<Result<_, _>>()?;
    let inside = |p: [f64; 2], poly: &[[f64; 2]]| {
        let mut odd = false;
        for i in 0..poly.len() {
            let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
            if (a[1] > p[1]) != (b[1] > p[1]) {
                let x = a[0] + (p[1] - a[1]) * (b[0] - a[0]) / (b[1] - a[1]);
                if p[0] < x {
                    odd = !odd;
                }
            }
        }
        odd
    };
    // A point of a loop that lies on no other loop's boundary: the middle
    // of its first edge.
    let probe = |l: &[SegmentGeometry]| -> Result<[f64; 2], String> {
        let c = BoundedCurve2::from_geometry(l[0].clone())?;
        c.point((c.range.first + c.range.last) / 2.0)
    };
    let containers: Vec<Vec<usize>> = (0..loops.len())
        .map(|i| {
            let p = probe(&loops[i])?;
            Ok((0..loops.len())
                .filter(|&j| j != i && inside(p, &polygons[j]))
                .collect())
        })
        .collect::<Result<_, String>>()?;
    let mut out = Vec::new();
    for (i, l) in loops.iter().enumerate() {
        if containers[i].len() % 2 == 1 {
            continue;
        }
        let depth = containers[i].len();
        let mut region = vec![l.clone()];
        for (j, hole) in loops.iter().enumerate() {
            if containers[j].len() == depth + 1 && containers[j].contains(&i) {
                region.push(hole.clone());
            }
        }
        let mut sheet_loops = Vec::new();
        for l in region {
            let mut segments = Vec::new();
            for geometry in l {
                segments.push(SheetSegment {
                    id: SheetCurveId::Generated {
                        feature,
                        slot: *slot,
                    },
                    geometry,
                });
                *slot += 1;
            }
            sheet_loops.push(SheetLoop { segments });
        }
        out.push(MaterialRegion::from_legacy(&SheetRegion {
            loops: sheet_loops,
        })?);
    }
    Ok(out)
}

/// A trim's result regions as tool regions: edges from the tools keep the
/// tools' identities; those from the chart get generated ones (they lie on
/// the chart's boundary and go with the removed material).
/// The regions of the result's outline that no chart holds, as tool
/// regions: their edges keep the outline's identities (generated curves of
/// the feature); those along a chart get new generated ones (they become
/// interior to the union).
fn added_regions(
    feature: FeatureUid,
    slot: &mut u32,
    regions: &[TrimRegion],
) -> Result<Vec<MaterialRegion>, String> {
    let mut out = Vec::new();
    for r in regions {
        let mut loops = Vec::new();
        for l in &r.loops {
            let mut edges = Vec::new();
            for e in &l.edges {
                let piece = e
                    .lineage
                    .iter()
                    .find_map(|s| match &s.source {
                        TrimSource::Domain { boundary } => Some(boundary.piece.clone()),
                        TrimSource::Tool { .. } => None,
                    })
                    .unwrap_or_else(|| {
                        *slot += 1;
                        CurvePieceRef {
                            curve: SheetCurveId::Generated {
                                feature,
                                slot: 1_000_000 + *slot,
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
        super::folded::develop::orient(&mut region)?;
        out.push(region);
    }
    Ok(out)
}

fn settled_regions(
    feature: FeatureUid,
    slot: &mut u32,
    regions: &[TrimRegion],
) -> Result<Vec<MaterialRegion>, String> {
    let mut out = Vec::new();
    for r in regions {
        let mut loops = Vec::new();
        for l in &r.loops {
            let mut edges = Vec::new();
            for e in &l.edges {
                let piece = e
                    .lineage
                    .iter()
                    .find_map(|s| match &s.source {
                        TrimSource::Tool { piece } => Some(piece.clone()),
                        TrimSource::Domain { .. } => None,
                    })
                    .unwrap_or_else(|| {
                        *slot += 1;
                        CurvePieceRef {
                            curve: SheetCurveId::Generated {
                                feature,
                                slot: 1_000_000 + *slot,
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
        super::folded::develop::orient(&mut region)?;
        out.push(region);
    }
    Ok(out)
}
