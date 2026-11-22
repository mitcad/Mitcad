// SPDX-License-Identifier: MIT
//! Insert Points (mitcad#2232): x y coordinates of a `.csv`, `.txt` or
//! `.dat` file into a sketch, as one smooth spline or as points, with an
//! airfoil file's chord length and angle of attack.
//!
//! - [`read`]: the file's points. Each line that starts with two numbers
//!   (separated by spaces, tabs, commas or semicolons, with a decimal point)
//!   is a point; other lines (a header, an airfoil's name, `#` comments)
//!   are skipped, the first one before any point kept as the file's name.
//!   A third value on a line is ignored with a warning.
//! - Airfoils: a file whose x values lie in about 0 to 1 is a normalised
//!   airfoil when it runs from the trailing edge over the upper surface
//!   round the leading edge back to the trailing edge (the Selig layout),
//!   or when its first line of numbers holds the two surfaces' point
//!   counts (`61. 61.`) followed by the upper and the lower surface, each
//!   from the leading edge to the trailing edge (the Lednicer layout). The
//!   counts line is not a point; the surfaces are split by the counts and
//!   reordered to the Selig order, the repeated leading-edge point dropped.
//! - [`plan`]: what goes into the sketch: the points placed (scaled and
//!   moved; an airfoil's chord along +X from its leading edge, turned by the
//!   angle of attack), and up to [`THROUGH_MAX`] points one Fit Point
//!   Spline through every point, more a least-squares fit
//!   ([`crate::point_fit`]) within the tolerance.
//! - [`insert`]: the plan as sketch entities.

use crate::EntityKind;
use crate::edit::SketchEdit;
use crate::geometry::P2;
use crate::ids::EntityUid;
use crate::point_fit::{self, CurveFit, Ends};

/// The most points a spline is drawn through exactly (a Fit Point
/// Spline); more are fitted.
pub const THROUGH_MAX: usize = 30;

/// Ends closer than this (mm) are one point: an airfoil's sharp trailing
/// edge, a closed list's first and last point.
pub const SAME_POINT: f64 = 1e-6;

/// The smallest default tolerance of a fit, mm.
pub const MIN_TOLERANCE: f64 = 0.01;

/// How an airfoil file lists its surfaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AirfoilLayout {
    /// From the trailing edge over the upper surface round the leading
    /// edge to the trailing edge on the lower surface.
    Selig,
    /// The two surfaces' point counts, then each surface from the leading
    /// edge to the trailing edge, the upper first.
    Lednicer,
}

impl AirfoilLayout {
    pub fn name(self) -> &'static str {
        match self {
            Self::Selig => "selig",
            Self::Lednicer => "lednicer",
        }
    }
}

/// An airfoil found in a file.
#[derive(Debug, Clone, PartialEq)]
pub struct Airfoil {
    pub layout: AirfoilLayout,
    /// The profile in the Selig order, in the file's numbers.
    pub profile: Vec<P2>,
}

/// A point file read.
#[derive(Debug, Clone, PartialEq)]
pub struct PointFile {
    /// The points in the file's order (an airfoil's counts line left out,
    /// a point repeated on the next line once).
    pub points: Vec<P2>,
    /// The first line when it is not a point: an airfoil's name, a header.
    pub name: Option<String>,
    /// The airfoil the file holds, if it is one.
    pub airfoil: Option<Airfoil>,
    /// Why the file is not an airfoil (empty when it is).
    pub not_airfoil: String,
    /// The most decimals a coordinate is written with (the file's last
    /// decimal place).
    pub decimals: u32,
    /// What the reader skipped or ignored.
    pub warnings: Vec<String>,
}

impl PointFile {
    /// Half the file's last decimal place: how far its rounding may have
    /// moved a point, in the file's numbers.
    pub fn rounding(&self) -> f64 {
        0.5 * 10f64.powi(-(self.decimals as i32))
    }
}

/// A number as files of coordinates write it: digits with a decimal point,
/// a sign and an exponent (`-.00041`, `61.`, `1.5e-3`); None for anything
/// else (`......`, `nan`, `1,5` is two numbers).
fn number(token: &str) -> Option<f64> {
    if !token.chars().any(|c| c.is_ascii_digit())
        || !token
            .chars()
            .all(|c| c.is_ascii_digit() || matches!(c, '+' | '-' | '.' | 'e' | 'E'))
    {
        return None;
    }
    token.parse::<f64>().ok().filter(|v| v.is_finite())
}

/// The decimals a number is written with: its digits after the point, less
/// its exponent (`0.00125` 5, `1.25e-3` 5, `12` 0).
fn decimals_of(token: &str) -> u32 {
    let (mantissa, exponent) = match token.find(['e', 'E']) {
        Some(at) => (&token[..at], token[at + 1..].parse::<i32>().unwrap_or(0)),
        None => (token, 0),
    };
    let after = mantissa.find('.').map_or(0, |at| {
        mantissa[at + 1..]
            .chars()
            .filter(char::is_ascii_digit)
            .count() as i32
    });
    (after - exponent).clamp(0, 15) as u32
}

fn same(a: P2, b: P2, within: f64) -> bool {
    (a[0] - b[0]).hypot(a[1] - b[1]) <= within
}

/// Reads a point file's text.
pub fn read(text: &str) -> Result<PointFile, String> {
    let mut numeric: Vec<P2> = Vec::new();
    let mut name: Option<String> = None;
    let mut decimals = 0u32;
    let mut third: Vec<usize> = Vec::new();
    let mut skipped: Vec<usize> = Vec::new();
    for (index, raw) in text.lines().enumerate() {
        let line = raw.trim().trim_start_matches('\u{feff}').trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let tokens: Vec<&str> = line
            .split(|c: char| c.is_whitespace() || c == ',' || c == ';')
            .filter(|t| !t.is_empty())
            .collect();
        let values = (
            tokens.first().and_then(|t| number(t)),
            tokens.get(1).and_then(|t| number(t)),
        );
        if let (Some(x), Some(y)) = values {
            numeric.push([x, y]);
            decimals = decimals.max(decimals_of(tokens[0]).max(decimals_of(tokens[1])));
            if tokens.len() > 2 {
                third.push(index + 1);
            }
        } else if numeric.is_empty() && name.is_none() {
            name = Some(line.to_owned());
        } else {
            skipped.push(index + 1);
        }
    }
    let mut warnings = Vec::new();
    if let Some(first) = third.first() {
        warnings.push(format!(
            "lines with more than two values, whose third and later values are ignored: {} (the first: line {first})",
            third.len()
        ));
    }
    if let Some(first) = skipped.first() {
        warnings.push(format!(
            "lines without two numbers, skipped: {} (the first: line {first})",
            skipped.len()
        ));
    }
    // The airfoil, Lednicer's counts line first.
    let (airfoil, points, not_airfoil) = match lednicer(&numeric) {
        Ok(airfoil) => (Some(airfoil), numeric[1..].to_vec(), String::new()),
        Err(_) => match selig(&numeric) {
            Ok(airfoil) => (Some(airfoil), numeric.clone(), String::new()),
            Err(why) => (None, numeric.clone(), why),
        },
    };
    let (points, repeated) = without_repeats(points);
    if repeated > 0 {
        warnings.push(format!(
            "points repeated on the next line, read once: {repeated}"
        ));
    }
    let airfoil = airfoil.map(|a| Airfoil {
        profile: without_repeats(a.profile).0,
        ..a
    });
    match points.len() {
        0 => {
            return Err(
                "the file has no points: a point is a line that starts with two numbers, x and y"
                    .to_owned(),
            );
        }
        1 => {
            return Err(
                "the file has only one point: at least two lines that start with two numbers, x and y, are needed"
                    .to_owned(),
            );
        }
        _ => {}
    }
    Ok(PointFile {
        points,
        name,
        airfoil,
        not_airfoil,
        decimals,
        warnings,
    })
}

/// The points with a point equal to the one before it left out, and how
/// many were.
fn without_repeats(points: Vec<P2>) -> (Vec<P2>, usize) {
    let mut out: Vec<P2> = Vec::with_capacity(points.len());
    let mut repeated = 0;
    for p in points {
        if out.last().is_some_and(|q| *q == p) {
            repeated += 1;
        } else {
            out.push(p);
        }
    }
    (out, repeated)
}

/// The x values of a normalised airfoil lie in about 0 to 1.
fn normalised(points: &[P2]) -> Result<(f64, f64), String> {
    let lo = points.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min);
    let hi = points
        .iter()
        .map(|p| p[0])
        .fold(f64::NEG_INFINITY, f64::max);
    if lo >= -0.05 && hi <= 1.05 && hi - lo >= 0.9 {
        Ok((lo, hi))
    } else {
        Err(format!(
            "its x values run from {} to {}, not from about 0 to 1",
            short(lo),
            short(hi)
        ))
    }
}

/// The Selig layout: from the trailing edge over the upper surface round
/// the leading edge (the smallest x) back to the trailing edge.
fn selig(points: &[P2]) -> Result<Airfoil, String> {
    if points.len() < 5 {
        return Err("it has fewer than five points".to_owned());
    }
    let (_, hi) = normalised(points)?;
    let le = leading_edge(points);
    let last = points.len() - 1;
    let near_te = |p: &P2| p[0] >= hi - 0.05;
    let slack = 1e-6;
    let runs = le > 0
        && le < last
        && near_te(&points[0])
        && near_te(&points[last])
        && points[..=le].windows(2).all(|w| w[1][0] <= w[0][0] + slack)
        && points[le..].windows(2).all(|w| w[1][0] >= w[0][0] - slack);
    if !runs {
        return Err(
            "it does not run from the trailing edge round the leading edge back to the trailing edge"
                .to_owned(),
        );
    }
    Ok(Airfoil {
        layout: AirfoilLayout::Selig,
        profile: points.to_vec(),
    })
}

/// The Lednicer layout: the counts line, then the upper and the lower
/// surface from the leading edge to the trailing edge; the profile in the
/// Selig order.
fn lednicer(numeric: &[P2]) -> Result<Airfoil, String> {
    let Some(&[upper, lower]) = numeric.first() else {
        return Err("it has no points".to_owned());
    };
    let count = |v: f64| (v >= 2.0 && v.fract() == 0.0).then_some(v as usize);
    let (Some(upper), Some(lower)) = (count(upper), count(lower)) else {
        return Err("its first line of numbers is not the surfaces' point counts".to_owned());
    };
    if upper + lower != numeric.len() - 1 {
        return Err("its surfaces' point counts do not match its lines".to_owned());
    }
    let surfaces = &numeric[1..];
    normalised(surfaces)?;
    let (up, low) = surfaces.split_at(upper);
    let rising = |s: &[P2]| s[0][0] < s[s.len() - 1][0];
    if !(rising(up) && rising(low)) {
        return Err(
            "its surfaces do not run from the leading edge to the trailing edge".to_owned(),
        );
    }
    let mut profile: Vec<P2> = up.iter().rev().copied().collect();
    let skip = usize::from(same(low[0], up[0], 1e-9));
    profile.extend_from_slice(&low[skip..]);
    Ok(Airfoil {
        layout: AirfoilLayout::Lednicer,
        profile,
    })
}

/// The index of the leading edge: the first point of the smallest x.
fn leading_edge(points: &[P2]) -> usize {
    let mut best = 0;
    for (i, p) in points.iter().enumerate() {
        if p[0] < points[best][0] {
            best = i;
        }
    }
    best
}

/// A number with up to six significant digits, for messages.
fn short(v: f64) -> String {
    if v == 0.0 || !v.is_finite() {
        return format!("{v}");
    }
    let digits = (5 - v.abs().log10().floor() as i32).clamp(0, 12) as usize;
    let text = format!("{v:.digits$}");
    if text.contains('.') {
        text.trim_end_matches('0').trim_end_matches('.').to_owned()
    } else {
        text
    }
}

/// How the points go into the sketch.
#[derive(Debug, Clone, PartialEq)]
pub struct PointOptions {
    /// One sketch point per file point instead of a spline.
    pub as_points: bool,
    /// Sketch millimetres per number of the file (not for an airfoil).
    pub scale: f64,
    /// Where the file's origin goes (an airfoil's leading edge, or with
    /// `pivot` that point of the chord).
    pub at: P2,
    /// The spline runs on from the last point back to the first; None:
    /// when the first and last points coincide. Never for an airfoil.
    pub closed: Option<bool>,
    /// The largest distance of a point from a fitted spline, mm; None: the
    /// larger of [`MIN_TOLERANCE`] and half the file's last decimal place
    /// times the scale (an airfoil's chord).
    pub tolerance: Option<f64>,
    /// Read the file as an airfoil; None: when it is one.
    pub airfoil: Option<bool>,
    /// An airfoil's chord length, mm.
    pub chord: f64,
    /// An airfoil's angle of attack, radians: positive raises the leading
    /// edge, turned about the pivot.
    pub angle: f64,
    /// The point of the chord `at` places and the angle turns about, as a
    /// share of the chord from the leading edge (0 the leading edge, 0.25
    /// the quarter chord, 1 the trailing edge).
    pub pivot: f64,
    /// An airfoil mirrored across its chord.
    pub flip: bool,
    /// A line between a blunt trailing edge's two ends.
    pub close_trailing_edge: bool,
}

impl Default for PointOptions {
    fn default() -> Self {
        Self {
            as_points: false,
            scale: 1.0,
            at: [0.0, 0.0],
            closed: None,
            tolerance: None,
            airfoil: None,
            chord: 100.0,
            angle: 0.0,
            pivot: 0.0,
            flip: false,
            close_trailing_edge: true,
        }
    }
}

/// The curve a plan draws.
#[derive(Debug, Clone, PartialEq)]
pub enum PlannedCurve {
    /// Only points.
    Points,
    /// A Fit Point Spline through every point.
    Through,
    /// A spline by control points fitted to the points.
    Fitted(CurveFit),
}

/// What goes into the sketch.
#[derive(Debug, Clone, PartialEq)]
pub struct PointPlan {
    /// The file was read as an airfoil.
    pub airfoil: bool,
    /// The points where they go in the sketch, mm, in the order the curve
    /// runs (an airfoil's in the Selig order). A closed spline's last
    /// point is not its first again.
    pub points: Vec<P2>,
    pub curve: PlannedCurve,
    /// The spline is closed.
    pub closed: bool,
    /// The spline's two ends are one point (an airfoil's sharp trailing
    /// edge, an open list that ends where it starts, a closed fit).
    pub shared_ends: bool,
    /// A line between the spline's two ends (a blunt trailing edge).
    pub closing_line: bool,
    /// A sketch point at an airfoil's leading edge.
    pub leading_edge: Option<P2>,
    /// The tolerance of a fit, mm.
    pub tolerance: f64,
    /// The largest distance of a point from the spline, mm (0 through the
    /// points).
    pub deviation: f64,
    pub warnings: Vec<String>,
}

fn rotate(p: P2, angle: f64) -> P2 {
    let (s, c) = angle.sin_cos();
    [c * p[0] - s * p[1], s * p[0] + c * p[1]]
}

/// Plans the insert of `file` with `options`.
pub fn plan(file: &PointFile, options: &PointOptions) -> Result<PointPlan, String> {
    let airfoil = match (options.airfoil, &file.airfoil) {
        (Some(false), _) => None,
        (_, Some(airfoil)) => Some(airfoil),
        (Some(true), None) => {
            return Err(format!("the file is not an airfoil: {}", file.not_airfoil));
        }
        (None, None) => None,
    };
    let mut warnings = file.warnings.clone();
    let (points, scale, leading) = match airfoil {
        Some(airfoil) => {
            if options.closed == Some(true) {
                return Err("an airfoil is one open spline from the trailing edge round the leading edge: leave closed out, or read the file with airfoil false".to_owned());
            }
            if !(options.chord.is_finite() && options.chord > 0.0) {
                return Err(format!(
                    "the chord length must be greater than zero, got {}",
                    options.chord
                ));
            }
            if !options.angle.is_finite() || !options.pivot.is_finite() {
                return Err("the angle of attack and the pivot must be numbers".to_owned());
            }
            let profile = &airfoil.profile;
            let le = profile[leading_edge(profile)];
            let hi = profile
                .iter()
                .map(|p| p[0])
                .fold(f64::NEG_INFINITY, f64::max);
            let scale = options.chord / (hi - le[0]);
            let flip = if options.flip { -1.0 } else { 1.0 };
            // The chord along +X from the leading edge; positive angles of
            // attack raise the leading edge: the profile turns clockwise.
            let place = |p: P2| {
                let local = [
                    (p[0] - le[0]) * scale - options.pivot * options.chord,
                    (p[1] - le[1]) * scale * flip,
                ];
                let turned = rotate(local, -options.angle);
                [turned[0] + options.at[0], turned[1] + options.at[1]]
            };
            let points: Vec<P2> = profile.iter().map(|p| place(*p)).collect();
            (points, scale, Some(place(le)))
        }
        None => {
            if !(options.scale.is_finite() && options.scale > 0.0) {
                return Err(format!(
                    "the scale must be greater than zero, got {}",
                    options.scale
                ));
            }
            let points = file
                .points
                .iter()
                .map(|p| {
                    [
                        p[0] * options.scale + options.at[0],
                        p[1] * options.scale + options.at[1],
                    ]
                })
                .collect();
            (points, options.scale, None)
        }
    };
    if points.iter().flatten().any(|v| !v.is_finite()) {
        return Err("the points do not fit in the sketch: the scale is too large".to_owned());
    }
    let tolerance = match options.tolerance {
        Some(t) if !(t.is_finite() && t > 0.0) => {
            return Err(format!("the tolerance must be greater than zero, got {t}"));
        }
        Some(t) => t,
        None => MIN_TOLERANCE.max(file.rounding() * scale),
    };
    let is_airfoil = airfoil.is_some();
    let ends_meet = same(points[0], points[points.len() - 1], SAME_POINT);
    let closed = !is_airfoil && options.closed.unwrap_or(ends_meet);
    let mut plan = PointPlan {
        airfoil: is_airfoil,
        points,
        curve: PlannedCurve::Points,
        closed: false,
        shared_ends: false,
        closing_line: false,
        leading_edge: None,
        tolerance,
        deviation: 0.0,
        warnings: Vec::new(),
    };
    if options.as_points {
        plan.warnings = warnings;
        return Ok(plan);
    }
    let last = plan.points.len() - 1;
    if closed {
        // Listed once: the last point is not the first again.
        if ends_meet {
            plan.points.pop();
        }
        if plan.points.len() < 3 {
            return Err("a closed spline needs three points".to_owned());
        }
        plan.closed = true;
        if plan.points.len() <= THROUGH_MAX {
            plan.curve = PlannedCurve::Through;
        } else {
            let mut ring = plan.points.clone();
            ring.push(ring[0]);
            let fit = point_fit::fit(&ring, tolerance, point_fit::MAX_CONTROL, Ends::Closed)?;
            plan.shared_ends = true;
            plan.deviation = fit.deviation;
            if !fit.within {
                warnings.push(cap_warning(&fit, tolerance));
            }
            plan.curve = PlannedCurve::Fitted(fit);
        }
    } else {
        if ends_meet {
            // One point at both ends (an airfoil's sharp trailing edge).
            plan.points[last] = plan.points[0];
            plan.shared_ends = true;
        }
        if plan.points.len() <= THROUGH_MAX {
            plan.curve = PlannedCurve::Through;
        } else {
            let fit = point_fit::fit(&plan.points, tolerance, point_fit::MAX_CONTROL, Ends::Open)?;
            plan.deviation = fit.deviation;
            if !fit.within {
                warnings.push(cap_warning(&fit, tolerance));
            }
            plan.curve = PlannedCurve::Fitted(fit);
        }
        if is_airfoil {
            plan.closing_line = !plan.shared_ends && options.close_trailing_edge;
            plan.leading_edge = leading;
        }
    }
    plan.warnings = warnings;
    Ok(plan)
}

fn cap_warning(fit: &CurveFit, tolerance: f64) -> String {
    format!(
        "the fitted spline stops at {} control points {} mm from the farthest point, more than the tolerance of {} mm",
        fit.control.len(),
        short(fit.deviation),
        short(tolerance)
    )
}

/// What [`insert`] made.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PointInsert {
    /// The spline.
    pub spline: Option<EntityUid>,
    /// The line across a blunt trailing edge.
    pub closing_line: Option<EntityUid>,
    /// The point at an airfoil's leading edge.
    pub leading_edge: Option<EntityUid>,
    /// Curves and sketch points made.
    pub curves: usize,
    pub points: usize,
    /// The spline's control points (a fit) or fit points (through the
    /// points).
    pub spline_points: usize,
}

/// Sketch points at `at`, the last one the first again when `shared`.
fn add_points(
    edit: &mut SketchEdit<'_>,
    made: &mut PointInsert,
    at: &[P2],
    shared: bool,
) -> Vec<EntityUid> {
    let mut ids: Vec<EntityUid> = Vec::with_capacity(at.len());
    for (i, p) in at.iter().enumerate() {
        if shared && i + 1 == at.len() && !ids.is_empty() {
            ids.push(ids[0]);
        } else {
            ids.push(edit.add_point(*p));
            made.points += 1;
        }
    }
    ids
}

/// Adds the plan to the sketch being edited.
pub fn insert(edit: &mut SketchEdit<'_>, plan: &PointPlan) -> Result<PointInsert, String> {
    let mut made = PointInsert::default();
    let ends = match &plan.curve {
        PlannedCurve::Points => {
            add_points(edit, &mut made, &plan.points, false);
            None
        }
        PlannedCurve::Through => {
            let ids = add_points(
                edit,
                &mut made,
                &plan.points,
                plan.shared_ends && !plan.closed,
            );
            made.spline_points = ids.len();
            let ends = (ids[0], ids[ids.len() - 1]);
            made.spline = Some(edit.add_curve(EntityKind::FittedSpline {
                points: ids,
                closed: plan.closed,
            }));
            Some(ends)
        }
        PlannedCurve::Fitted(fit) => {
            let ids = add_points(edit, &mut made, &fit.control, plan.shared_ends);
            made.spline_points = ids.len();
            let ends = (ids[0], ids[ids.len() - 1]);
            made.spline = Some(edit.add_curve(EntityKind::Spline {
                degree: crate::bspline::DEGREE as u32,
                control: ids,
                weights: Vec::new(),
                knots: fit.knots.clone(),
            }));
            Some(ends)
        }
    };
    if made.spline.is_some() {
        made.curves += 1;
    }
    if plan.closing_line
        && let Some((first, last)) = ends
        && first != last
    {
        made.closing_line = Some(edit.add_curve(EntityKind::Line {
            start: last,
            end: first,
            centerline: false,
        }));
        made.curves += 1;
    }
    if let Some(at) = plan.leading_edge {
        made.leading_edge = Some(edit.add_point(at));
        made.points += 1;
    }
    Ok(made)
}
