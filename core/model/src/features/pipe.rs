// SPDX-License-Identifier: MIT
//! Pipe: a circular, square or triangular section swept along a path,
//! solid or hollow, with the operations of an extrusion.

use serde::{Deserialize, Serialize};

use super::extrude::{Operation, apply_operation, check_participants, participant_bodies};
use super::geom_ref::PathRef;
use super::mapping::map_params;
use super::path::{self, CurvePath, SketchCurves};
use super::spatial_path::SpatialPath;
use super::sweep::{PathExtent, ProfileAt, SweepPlace};
use super::{CheckContext, EvalContext, Evaluate, FeatureInfo, FeatureOutput, References, ToolUse};
use crate::ids::BodyUid;
use crate::kernel::{Curve3, Kernel};
use crate::parameters::ParamId;
use crate::sweeps::{FOLDS_RESOLVED, PathCurve, PipeCorners, PipeSection, PipeSpec};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PipeDef<P = ParamId> {
    pub path: CurvePath,
    #[serde(default, skip_serializing_if = "is_circular")]
    pub section: PipeSection,
    /// The diameter of the circle, or of the circle round the square or
    /// the triangle (`sectionSize` in .f3d designs).
    pub size: P,
    /// Makes the pipe hollow with this wall inside the section (`isHollow`
    /// and `sectionThickness` in .f3d designs).
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub thickness: Option<P>,
    #[serde(
        default = "PathExtent::full",
        skip_serializing_if = "PathExtent::is_full"
    )]
    pub extent: PathExtent<P>,
    pub operation: Operation,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub participants: Vec<BodyUid>,
    /// The joints at the path's sharp corners (mitcad#1448): `mitered`
    /// (left out) or `rounded`.
    #[serde(default, skip_serializing_if = "is_mitered")]
    pub corners: PipeCorners,
}

fn is_circular(section: &PipeSection) -> bool {
    *section == PipeSection::Circular
}

fn is_mitered(corners: &PipeCorners) -> bool {
    *corners == PipeCorners::Mitered
}

impl<P> PipeDef<P> {
    pub const TYPE: &'static str = "pipe";
    pub const BASE_NAME: &'static str = "Pipe";

    map_params! {
        PipeDef {
            path: clone,
            section: copy,
            size: param,
            thickness: option,
            extent: map,
            operation: copy,
            participants: clone,
            corners: copy,
        }
    }
}

/// The largest wall a section takes: the radius of the circle inside it.
fn inner_radius(section: PipeSection, size: f64) -> f64 {
    let radius = size / 2.0;
    match section {
        PipeSection::Circular => radius,
        PipeSection::Square => radius * std::f64::consts::FRAC_1_SQRT_2,
        PipeSection::Triangular => radius / 2.0,
    }
}

fn check_sizes(section: PipeSection, size: f64, thickness: Option<f64>) -> Result<(), String> {
    if !(size > 0.0 && size.is_finite()) {
        return Err(format!(
            "the section size must be greater than zero, got {size}"
        ));
    }
    if let Some(t) = thickness {
        if !(t > 0.0 && t.is_finite()) {
            return Err(format!(
                "the wall thickness must be greater than zero, got {t}"
            ));
        }
        if t >= inner_radius(section, size) {
            return Err(format!(
                "the wall thickness {t} fills the section; it must be less than {}",
                inner_radius(section, size)
            ));
        }
    }
    Ok(())
}

/// Curves meeting at a smaller angle than this (radians) join smoothly:
/// the pipe shell takes no corner there either.
const CORNER_ANGLE: f64 = 1.0e-2;

/// The directions of a curve at its start and at its end, along its run;
/// none for a point or a closed curve.
fn end_tangents(curve: &Curve3) -> Option<([f64; 3], [f64; 3])> {
    use mitcad_vec3::{add, cross, scale, sub};
    match curve {
        Curve3::Point(_) | Curve3::Conic { closed: true, .. } => None,
        Curve3::Line { start, end } => {
            let d = sub(*end, *start);
            Some((d, d))
        }
        Curve3::Conic {
            normal,
            x_axis,
            major,
            minor,
            start,
            end,
            ..
        } => {
            let y_axis = cross(*normal, *x_axis);
            let at = |t: f64| {
                add(
                    scale(*x_axis, -major * t.sin()),
                    scale(y_axis, minor * t.cos()),
                )
            };
            Some((at(*start), at(*end)))
        }
        Curve3::Helix {
            axis,
            x_axis,
            radius,
            pitch,
            start,
            end,
            ..
        } => {
            let y_axis = cross(*axis, *x_axis);
            let rise = scale(*axis, pitch / std::f64::consts::TAU);
            let at = |t: f64| {
                add(
                    add(
                        scale(*x_axis, -radius * t.sin()),
                        scale(y_axis, radius * t.cos()),
                    ),
                    rise,
                )
            };
            Some((at(*start), at(*end)))
        }
        Curve3::BSpline {
            degree,
            poles,
            weights,
            knots,
        } => {
            let p = *degree as usize;
            if p == 0 || poles.len() <= p || knots.len() != poles.len() + p + 1 {
                let (first, last) = (poles.first()?, poles.last()?);
                let d = sub(*last, *first);
                return Some((d, d));
            }
            let (t0, t1) = (knots[p], knots[poles.len()]);
            let h = (t1 - t0) * 1.0e-6;
            let at = |t: f64| path::bspline_point(p, poles, weights, knots, t);
            Some((sub(at(t0 + h), at(t0)), sub(at(t1), at(t1 - h))))
        }
    }
}

/// Whether the directions `a` and `b` differ by more than [`CORNER_ANGLE`].
fn turns(a: [f64; 3], b: [f64; 3]) -> bool {
    match (mitcad_vec3::unit(a), mitcad_vec3::unit(b)) {
        (Some(a), Some(b)) => mitcad_vec3::dot(a, b) < CORNER_ANGLE.cos(),
        _ => false,
    }
}

/// The directions at the joint of `a` and `b`: the one arriving along `a`
/// and the one leaving along `b`, each curve taken the way the joint meets
/// it; none when either is a point or a closed curve.
fn joint_directions(a: &PathCurve, b: &PathCurve) -> Option<([f64; 3], [f64; 3])> {
    let (a0, a1) = path::curve_ends(&a.curve)?;
    let (b0, b1) = path::curve_ends(&b.curve)?;
    let (ta0, ta1) = end_tangents(&a.curve)?;
    let (tb0, tb1) = end_tangents(&b.curve)?;
    // The pair of ends that meet.
    let pairs = [
        (a1, b0, ta1, tb0),
        (a1, b1, ta1, mitcad_vec3::scale(tb1, -1.0)),
        (a0, b0, mitcad_vec3::scale(ta0, -1.0), tb0),
        (
            a0,
            b1,
            mitcad_vec3::scale(ta0, -1.0),
            mitcad_vec3::scale(tb1, -1.0),
        ),
    ];
    let mut best = pairs[0];
    for pair in pairs {
        if mitcad_vec3::distance(pair.0, pair.1) < mitcad_vec3::distance(best.0, best.1) {
            best = pair;
        }
    }
    Some((best.2, best.3))
}

/// The sharp corners of a path: the joints of its curves (the last to the
/// first on a closed path) where the path turns, each curve taken the way
/// the joint meets it.
fn corner_count(path: &[PathCurve], closed: bool) -> usize {
    let joint =
        |a: &PathCurve, b: &PathCurve| joint_directions(a, b).is_some_and(|(a, b)| turns(a, b));
    let mut count = path
        .windows(2)
        .filter(|pair| joint(&pair[0], &pair[1]))
        .count();
    if closed && path.len() > 1 && joint(&path[path.len() - 1], &path[0]) {
        count += 1;
    }
    count
}

/// How far a section of `size` reaches from the path towards the inside
/// of a bend (mitcad#1497). Along a path in one plane: half the circle's
/// diameter, half the square's side (its sides follow the plane) or half
/// the triangle's side (its base corners; its height lies along the
/// plane's normal). Along any other path the radius of the circle round
/// the section, as the section may turn any way.
fn section_reach(section: PipeSection, size: f64, planar: bool) -> f64 {
    match (section, planar) {
        (PipeSection::Circular, _) | (PipeSection::Square, true) => size / 2.0,
        (PipeSection::Square, false) => size * std::f64::consts::FRAC_1_SQRT_2,
        (PipeSection::Triangular, true) => size / 3.0_f64.sqrt(),
        (PipeSection::Triangular, false) => 2.0 * size / 3.0,
    }
}

/// Whether the path lies in one plane (a straight one too). A helix does
/// not; a spline does when its control points do.
fn is_planar(path: &[PathCurve]) -> bool {
    use mitcad_vec3::{cross, distance, dot, norm, sub, unit_above};
    let mut points: Vec<[f64; 3]> = Vec::new();
    for piece in path {
        match &piece.curve {
            Curve3::Point(p) => points.push(*p),
            Curve3::Line { start, end } => points.extend([*start, *end]),
            Curve3::Conic {
                center,
                normal,
                x_axis,
                major,
                minor,
                ..
            } => points.extend((0..4_i32).map(|k| {
                path::conic_point(
                    *center,
                    *normal,
                    *x_axis,
                    (*major, *minor),
                    f64::from(k) * std::f64::consts::FRAC_PI_2,
                )
            })),
            Curve3::Helix { .. } => return false,
            Curve3::BSpline { poles, .. } => points.extend(poles.iter().copied()),
        }
    }
    let Some(&origin) = points.first() else {
        return true;
    };
    let far = points
        .iter()
        .copied()
        .max_by(|a, b| distance(*a, origin).total_cmp(&distance(*b, origin)))
        .unwrap_or(origin);
    let size = distance(far, origin);
    let along = sub(far, origin);
    let normal = points
        .iter()
        .map(|p| cross(along, sub(*p, origin)))
        .max_by(|a, b| norm(*a).total_cmp(&norm(*b)))
        .and_then(|n| unit_above(n, 1.0e-9 * size * size));
    // All on one line: in any plane through it.
    let Some(normal) = normal else {
        return true;
    };
    points
        .iter()
        .all(|p| dot(sub(*p, origin), normal).abs() <= 1.0e-6 * size.max(1.0))
}

/// The smallest radius of curvature of a curve along its run; none for a
/// straight one (a point, a line, a spline of degree one: its kinks are
/// corners, not bends).
fn smallest_bend(curve: &Curve3) -> Option<f64> {
    match curve {
        Curve3::Point(_) | Curve3::Line { .. } => None,
        Curve3::Conic {
            major,
            minor,
            start,
            end,
            closed,
            ..
        } => {
            let (a, b) = (*major, *minor);
            let (t0, t1) = if *closed {
                (0.0, std::f64::consts::TAU)
            } else {
                (*start, *end)
            };
            if !(a > 0.0 && b > 0.0 && t0.is_finite() && t1.is_finite() && t0 <= t1) {
                return None;
            }
            // The radius is |r'|³ / (a b); |r'|² = a² sin² t + b² cos² t
            // runs one way between quarter turns, so its least value is at
            // an end of the arc or at a quarter turn inside it.
            let speed2 = |t: f64| (a * t.sin()).powi(2) + (b * t.cos()).powi(2);
            let quarter = std::f64::consts::FRAC_PI_2;
            let mut least = speed2(t0).min(speed2(t1));
            let mut k = (t0 / quarter).ceil();
            while k * quarter < t1 && k <= (t0 / quarter).ceil() + 8.0 {
                least = least.min(speed2(k * quarter));
                k += 1.0;
            }
            Some(least.powf(1.5) / (a * b))
        }
        Curve3::Helix { radius, pitch, .. } => {
            let (radius, rise) = (*radius, pitch / std::f64::consts::TAU);
            if radius > 0.0 {
                Some((radius * radius + rise * rise) / radius)
            } else {
                None
            }
        }
        Curve3::BSpline {
            degree,
            poles,
            weights,
            knots,
        } => spline_bend(*degree as usize, poles, weights, knots),
    }
}

/// The smallest radius of curvature of a B-spline, sampled inside each of
/// its knot spans (never across a knot, where a spline may kink).
fn spline_bend(p: usize, poles: &[[f64; 3]], weights: &[f64], knots: &[f64]) -> Option<f64> {
    use mitcad_vec3::{add, cross, norm, scale, sub};
    if p < 2 || poles.len() <= p || knots.len() != poles.len() + p + 1 {
        return None;
    }
    const SAMPLES: usize = 16;
    let at = |t: f64| path::bspline_point(p, poles, weights, knots, t);
    let mut least = f64::INFINITY;
    for span in knots[p..=poles.len()].windows(2) {
        let (u0, width) = (span[0], span[1] - span[0]);
        if !(width > 0.0 && width.is_finite()) {
            continue;
        }
        let h = width * 1.0e-3;
        for k in 0..SAMPLES {
            let t = u0 + width * (k as f64 + 0.5) / SAMPLES as f64;
            let (before, here, after) = (at(t - h), at(t), at(t + h));
            let d1 = scale(sub(after, before), 0.5 / h);
            let d2 = scale(add(sub(after, here), sub(before, here)), 1.0 / (h * h));
            let (speed, turn) = (norm(d1), norm(cross(d1, d2)));
            if speed > 0.0 && turn > 0.0 {
                least = least.min(speed.powi(3) / turn);
            }
        }
    }
    least.is_finite().then_some(least)
}

/// How long a straight piece of the path must be for the miters at its
/// ends (mitcad#1497): `reach` times the length of the difference of the
/// corners' leans. A corner turning by θ cuts the side of the pipe that
/// leans towards its other leg back from the corner by `x tan(θ/2)`, `x`
/// being how far the side lies from the path that way; the lean is that
/// leg's direction across the piece, `tan(θ/2)` long. None when the path
/// turns right back at an end.
fn miter_need(path: &[PathCurve], i: usize, closed: bool, reach: f64) -> Option<(f64, f64)> {
    use mitcad_vec3::{dot, norm, scale, sub, unit};
    let Curve3::Line { start, end } = &path[i].curve else {
        return None;
    };
    let along = unit(sub(*end, *start))?;
    let n = path.len();
    let before = if i > 0 {
        Some(&path[i - 1])
    } else if closed && n > 1 {
        Some(&path[n - 1])
    } else {
        None
    };
    let after = if i + 1 < n {
        Some(&path[i + 1])
    } else if closed && n > 1 {
        Some(&path[0])
    } else {
        None
    };
    // `other` across the piece over 1 + cos θ: tan(θ/2) long.
    let lean = |other: [f64; 3]| -> Option<[f64; 3]> {
        let other = unit(other)?;
        let cos = dot(other, along);
        if cos > CORNER_ANGLE.cos() {
            return Some([0.0; 3]);
        }
        if 1.0 + cos < 1.0e-9 {
            return None;
        }
        Some(scale(sub(other, scale(along, cos)), 1.0 / (1.0 + cos)))
    };
    let lean_before = match before.and_then(|b| joint_directions(b, &path[i])) {
        Some((arriving, _)) => lean(arriving)?,
        None => [0.0; 3],
    };
    let lean_after = match after.and_then(|a| joint_directions(&path[i], a)) {
        Some((_, leaving)) => lean(leaving)?,
        None => [0.0; 3],
    };
    let need = reach * norm(sub(lean_after, lean_before));
    Some((mitcad_vec3::distance(*start, *end), need))
}

/// A length for a message: at most three decimals, no trailing zeros.
fn mm(value: f64) -> String {
    let text = format!("{value:.3}");
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// A place where the pipe folds through itself (mitcad#1497): a curve of
/// the path that bends tighter than the section reaches from it, or a
/// straight piece too short for the miters at its ends; the curve named
/// (`place`) with the numbers.
#[derive(Debug, Clone, PartialEq)]
enum Fold {
    Bend {
        radius: f64,
        place: String,
        reach: f64,
    },
    Short {
        place: String,
        length: f64,
        need: f64,
    },
}

impl Fold {
    /// The warning where the pipe was built folded or not at all.
    fn warning(&self) -> String {
        match self {
            Fold::Bend {
                radius,
                place,
                reach,
            } => format!(
                "the path bends with radius {} mm at {}; the pipe needs at least {} mm",
                mm(*radius),
                place,
                mm(*reach)
            ),
            Fold::Short {
                place,
                length,
                need,
            } => format!(
                "{} is {} mm long; the pipe's mitered corners need at least {} mm there",
                place,
                mm(*length),
                mm(*need)
            ),
        }
    }

    /// The note where the geometry built the pipe in pieces there, fused
    /// into one solid (mitcad#1697).
    fn note(&self) -> String {
        match self {
            Fold::Bend {
                radius,
                place,
                reach,
            } => format!(
                "the path bends with radius {} mm at {}, tighter than the {} mm the pipe reaches; \
                 the pipe was built in pieces there, joined into one solid",
                mm(*radius),
                place,
                mm(*reach)
            ),
            Fold::Short {
                place,
                length,
                need,
            } => format!(
                "{} is {} mm long, shorter than the {} mm the pipe's mitered corners need; \
                 the pipe was built in pieces there, joined into one solid",
                place,
                mm(*length),
                mm(*need)
            ),
        }
    }
}

/// Where the path bends tighter than the section reaches from it, or a
/// straight piece is too short for the miters at its ends (mitcad#1497):
/// the pipe there folds through itself. One fold for each such curve of
/// the path, naming it (`place`) with the numbers.
fn tight_bends(
    path: &[PathCurve],
    closed: bool,
    section: PipeSection,
    size: f64,
    corners: PipeCorners,
    place: &dyn Fn(&PathCurve) -> String,
) -> Vec<Fold> {
    let reach = section_reach(section, size, is_planar(path));
    // Rounding errors of a bend or a piece drawn to the limit.
    let slack = 1.0e-9 * reach.max(1.0);
    let mut problems = Vec::new();
    for (i, piece) in path.iter().enumerate() {
        if let Some(radius) = smallest_bend(&piece.curve)
            && radius < reach - slack
        {
            problems.push(Fold::Bend {
                radius,
                place: place(piece),
                reach,
            });
        }
        // Rounded corners end the legs square at the corner: no miters.
        if corners == PipeCorners::Mitered
            && let Some((length, need)) = miter_need(path, i, closed, reach)
            && length < need - slack
        {
            problems.push(Fold::Short {
                place: place(piece),
                length,
                need,
            });
        }
    }
    problems
}

impl FeatureInfo for PipeDef {
    fn references(&self) -> References {
        let mut references = References::default();
        self.path.add_references(&mut references);
        for body in &self.participants {
            references.body(*body);
        }
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        check_participants(ctx, self.operation, &self.participants)?;
        self.path.check(ctx).map_err(|e| format!("path: {e}"))
    }

    fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
        self.extent.check_values(value)?;
        check_sizes(self.section, value(self.size), self.thickness.map(value))
    }

    fn creates_bodies(&self) -> bool {
        true
    }

    fn new_component(&self) -> bool {
        self.operation == Operation::NewComponent
    }

    fn tool_use(&self) -> Option<ToolUse> {
        Some(ToolUse {
            operation: self.operation,
            participants: self.participants.clone(),
        })
    }
}

impl<K: Kernel> Evaluate<K> for PipeDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let participants = participant_bodies(ctx, self.operation, &self.participants)?;
        let size = ctx.param(self.size)?;
        let thickness = self.thickness.map(|id| ctx.param(id)).transpose()?;
        check_sizes(self.section, size, thickness)?;
        let path = self.path.resolve(ctx).map_err(|e| format!("path: {e}"))?;
        let closed = path::is_closed(&path);
        // The preview tells the panel whether the path has corners
        // (mitcad#1448), also when the pipe then fails.
        ctx.sweep_place = Some(SweepPlace {
            closed,
            profile_at: if closed {
                ProfileAt::Inside
            } else {
                ProfileAt::Start
            },
            corners: corner_count(&path, closed),
        });
        // The section starts the path, so only a closed path has a part
        // before it.
        let (extent1, extent2) = match &self.extent {
            PathExtent::Partial {
                fraction2: Some(_), ..
            } if !closed => {
                return Err("only a pipe along a closed path has a second fraction".to_owned());
            }
            extent => extent.fractions(ctx, true)?,
        };
        // A bend tighter than the section, or miters that overlap
        // (mitcad#1497): checked along the whole path only, as a part of
        // it may leave the place out.
        let bends = if self.extent.is_full() {
            let names: &EvalContext<'_, K> = ctx;
            let place = |piece: &PathCurve| self.place(names, piece);
            tight_bends(&path, closed, self.section, size, self.corners, &place)
        } else {
            Vec::new()
        };
        let spec = PipeSpec {
            feature: ctx.uid,
            path: &path,
            extent1,
            extent2,
            section: self.section,
            size,
            thickness,
            corners: self.corners,
        };
        let tool = match ctx.kernel.pipe(&spec) {
            Ok(tool) => tool,
            Err(e) if bends.is_empty() => return Err(e.to_string()),
            // The likely cause with the failure.
            Err(e) => {
                let said: Vec<String> = bends.iter().map(Fold::warning).collect();
                return Err(format!("{e}: {}", said.join("; ")));
            }
        };
        // (mitcad#1697) Where the pipe passes through itself, the geometry
        // builds it in pieces fused into one solid and says so: the folds
        // are notes then. Where it could not, they stay warnings.
        let resolved = ctx
            .kernel
            .infos(&tool)
            .iter()
            .any(|info| info == FOLDS_RESOLVED);
        if resolved && bends.is_empty() {
            ctx.inform(FOLDS_RESOLVED);
        }
        for bend in bends {
            if resolved {
                ctx.inform(bend.note());
            } else {
                ctx.warn(bend.warning());
            }
        }
        apply_operation(ctx, self.operation, participants, tool, "pipe")
    }
}

impl PipeDef {
    /// A curve of the path for a message a person reads (mitcad#1612: no
    /// sketch ids or edge names): a curve of the sketch by the sketch's
    /// name (`a curve of Sketch2`), an edge of the path, or a piece of the
    /// path.
    fn place<K: Kernel>(&self, ctx: &EvalContext<'_, K>, _piece: &PathCurve) -> String {
        match &self.path {
            CurvePath::Sketch(SketchCurves { sketch, .. })
            | CurvePath::Spatial(SpatialPath { sketch, .. })
            | CurvePath::Path(
                PathRef::SketchCurve { sketch, .. } | PathRef::SketchCurves { sketch, .. },
            ) => format!("a curve of {}", ctx.feature_name(*sketch)),
            CurvePath::Path(PathRef::Edges { .. }) => "an edge of the path".to_owned(),
            CurvePath::Path(_) => "a piece of the path".to_owned(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(name: &str, a: [f64; 3], b: [f64; 3]) -> PathCurve {
        PathCurve {
            name: name.to_owned(),
            curve: Curve3::Line { start: a, end: b },
        }
    }

    fn conic(major: f64, minor: f64, start: f64, end: f64, closed: bool) -> Curve3 {
        Curve3::Conic {
            center: [0.0; 3],
            normal: [0.0, 0.0, 1.0],
            x_axis: [1.0, 0.0, 0.0],
            major,
            minor,
            start,
            end,
            closed,
        }
    }

    fn close(a: f64, b: f64, tolerance: f64) -> bool {
        (a - b).abs() <= tolerance
    }

    #[test]
    fn bends_of_arcs_ellipses_helices_and_splines() {
        // A circular arc: its radius.
        let arc = smallest_bend(&conic(3.0, 3.0, 0.2, 1.4, false)).unwrap();
        assert!(close(arc, 3.0, 1e-12), "{arc}");
        // An ellipse 10 by 4: b²/a = 1.6 at the ends of its major axis,
        // also for an arc across one of them; an arc between them bends
        // least tightly at its end nearer one.
        let full = smallest_bend(&conic(10.0, 4.0, 0.0, 0.0, true)).unwrap();
        assert!(close(full, 1.6, 1e-12), "{full}");
        let across = smallest_bend(&conic(10.0, 4.0, -0.5, 0.5, false)).unwrap();
        assert!(close(across, 1.6, 1e-12), "{across}");
        let between = smallest_bend(&conic(10.0, 4.0, 0.3, 1.0, false)).unwrap();
        let speed2 = 100.0 * 0.3_f64.sin().powi(2) + 16.0 * 0.3_f64.cos().powi(2);
        assert!(close(between, speed2.powf(1.5) / 40.0, 1e-12), "{between}");
        // A helix of radius 4 rising 3 a radian: (16 + 9) / 4.
        let helix = Curve3::Helix {
            center: [0.0; 3],
            axis: [0.0, 0.0, 1.0],
            x_axis: [1.0, 0.0, 0.0],
            radius: 4.0,
            pitch: 3.0 * std::f64::consts::TAU,
            start: 0.0,
            end: 20.0,
        };
        assert!(close(smallest_bend(&helix).unwrap(), 6.25, 1e-12));
        // A quarter circle of radius 7 as a rational quadratic spline.
        let quarter = Curve3::BSpline {
            degree: 2,
            poles: vec![[7.0, 0.0, 0.0], [7.0, 7.0, 0.0], [0.0, 7.0, 0.0]],
            weights: vec![1.0, std::f64::consts::FRAC_1_SQRT_2, 1.0],
            knots: vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
        };
        let bend = smallest_bend(&quarter).unwrap();
        assert!(close(bend, 7.0, 1e-4), "{bend}");
        // Straight: lines and splines of degree one.
        let polyline = Curve3::BSpline {
            degree: 1,
            poles: vec![[0.0; 3], [5.0, 0.0, 0.0], [5.0, 5.0, 0.0]],
            weights: Vec::new(),
            knots: vec![0.0, 0.0, 0.5, 1.0, 1.0],
        };
        assert_eq!(smallest_bend(&polyline), None);
        assert_eq!(
            smallest_bend(&line("c1", [0.0; 3], [1.0, 0.0, 0.0]).curve),
            None
        );
    }

    #[test]
    fn miters_need_room_on_a_short_piece() {
        // An L chamfered 2 mm back from its corner: the chamfer turns 45°
        // at each end, each miter cutting 5 tan(22.5°) back on the inside.
        let path = [
            line("c1", [0.0; 3], [38.0, 0.0, 0.0]),
            line("c3", [38.0, 0.0, 0.0], [40.0, 2.0, 0.0]),
            line("c2", [40.0, 2.0, 0.0], [40.0, 30.0, 0.0]),
        ];
        let (length, need) = miter_need(&path, 1, false, 5.0).unwrap();
        assert!(close(length, 8.0_f64.sqrt(), 1e-12), "{length}");
        let tan = (std::f64::consts::PI / 8.0).tan();
        assert!(close(need, 10.0 * tan, 1e-12), "{need}");
        // The first piece has only the corner at its end.
        let (_, need) = miter_need(&path, 0, false, 5.0).unwrap();
        assert!(close(need, 5.0 * tan, 1e-12), "{need}");
        let place = |piece: &PathCurve| piece.name.clone();
        let said: Vec<String> = tight_bends(
            &path,
            false,
            PipeSection::Circular,
            10.0,
            PipeCorners::Mitered,
            &place,
        )
        .iter()
        .map(Fold::warning)
        .collect();
        assert_eq!(
            said,
            ["c3 is 2.828 mm long; the pipe's mitered corners need at least 4.142 mm there"]
        );
        let said = tight_bends(
            &path,
            false,
            PipeSection::Circular,
            10.0,
            PipeCorners::Rounded,
            &place,
        );
        assert!(said.is_empty(), "{said:?}");
        // A jog (an S): the two miters lean the same way on the piece,
        // one cutting where the other adds; any length will do.
        let jog = [
            line("c1", [0.0; 3], [10.0, 0.0, 0.0]),
            line("c2", [10.0, 0.0, 0.0], [12.0, 2.0, 0.0]),
            line("c3", [12.0, 2.0, 0.0], [22.0, 2.0, 0.0]),
        ];
        let (_, need) = miter_need(&jog, 1, false, 5.0).unwrap();
        assert!(close(need, 0.0, 1e-12), "{need}");
    }

    #[test]
    fn a_closed_circle_tighter_than_the_section_warns() {
        let path = [PathCurve {
            name: "c1".to_owned(),
            curve: conic(3.0, 3.0, 0.0, std::f64::consts::TAU, true),
        }];
        let place = |piece: &PathCurve| format!("Sketch1's {}", piece.name);
        let said: Vec<String> = tight_bends(
            &path,
            true,
            PipeSection::Circular,
            10.0,
            PipeCorners::Mitered,
            &place,
        )
        .iter()
        .map(Fold::warning)
        .collect();
        assert_eq!(
            said,
            ["the path bends with radius 3 mm at Sketch1's c1; the pipe needs at least 5 mm"]
        );
        // Built in pieces by the geometry (mitcad#1697): a note.
        let fold = tight_bends(
            &path,
            true,
            PipeSection::Circular,
            10.0,
            PipeCorners::Mitered,
            &place,
        );
        assert_eq!(
            fold[0].note(),
            "the path bends with radius 3 mm at Sketch1's c1, tighter than the 5 mm the pipe \
             reaches; the pipe was built in pieces there, joined into one solid"
        );
        // A square of side 5 reaches 2.5 mm from a path in its plane.
        let said = tight_bends(
            &path,
            true,
            PipeSection::Square,
            5.0,
            PipeCorners::Mitered,
            &place,
        );
        assert!(said.is_empty(), "{said:?}");
    }

    #[test]
    fn reaches_of_sections_in_and_out_of_a_plane() {
        let flat = [
            line("c1", [0.0; 3], [10.0, 0.0, 0.0]),
            line("c2", [10.0, 0.0, 0.0], [10.0, 10.0, 0.0]),
        ];
        let bent = [
            line("c1", [0.0; 3], [10.0, 0.0, 0.0]),
            line("c2", [10.0, 0.0, 0.0], [10.0, 10.0, 0.0]),
            line("c3", [10.0, 10.0, 0.0], [10.0, 10.0, 10.0]),
        ];
        assert!(is_planar(&flat));
        assert!(!is_planar(&bent));
        assert!(is_planar(&[line("c1", [0.0; 3], [1.0, 2.0, 3.0])]));
        assert_eq!(section_reach(PipeSection::Circular, 10.0, false), 5.0);
        assert_eq!(section_reach(PipeSection::Square, 10.0, true), 5.0);
        assert!(close(
            section_reach(PipeSection::Square, 10.0, false),
            50.0_f64.sqrt(),
            1e-12
        ));
        // An equilateral triangle 9 high: sides of 6√3, the circle round
        // it of radius 6.
        assert!(close(
            section_reach(PipeSection::Triangular, 9.0, true),
            3.0 * 3.0_f64.sqrt(),
            1e-12
        ));
        assert_eq!(section_reach(PipeSection::Triangular, 9.0, false), 6.0);
    }
}
