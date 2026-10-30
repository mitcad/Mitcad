// SPDX-License-Identifier: MIT
//! The curve an edge input stores beside its recipe (mitcad#306).
//!
//! An edge input (`5662F619`, [`super::classes::FACE_REF`]) refers, besides
//! its recipe ([`super::recipe`]), to the curve of the edge as it was when
//! the input was made: after the two GUIDs of the reference a list `u32 n |
//! n refs` of curve objects (classes deriving from [`CURVE`]; one in every
//! edge input of the corpora). The curves are in the component's
//! coordinates, in centimetres and radians. After the root part (no
//! references, no attributes) *(read from the reference models, whose
//! edges the API dumps give, and checked on the corpora)*:
//!
//! ```text
//! line   (50AEE8B4): f64[3] start | f64[3] vector (start to end)
//! arc    (B0B3FA59): f64[3] centre | f64[3] axis | f64[3] reference
//!                    direction | f64 radius | f64 start | f64 end
//!                    (angles about the axis from the reference direction,
//!                    towards axis × reference)
//! ellipse (98E8921B): f64[3] centre | f64[3] major direction | f64[3]
//!                    minor direction | f64 major radius | f64 minor radius
//!                    | f64 start | f64 end (the parameter t of centre +
//!                    major cos t · major dir + minor sin t · minor dir)
//! spline (2319971D): u8 a | u8 b | u32 degree | f64 | knots | weights |
//!                    poles | f64 | u8 | u8 | f64 start | f64 end
//!                    (the edge's parameter range; a, b flags 0 or 1,
//!                    the two f64 small tolerances, not read)
//!   knots, weights: u32 n | u32 n | u32 8 | n × f64 (no weights: n = 0)
//!   poles:          u32 n | u32 n | u32 8 | n × f64[3]
//! every curve then: u8 0 | ref | u8 0 | u8 1 | ref
//!   (both references null, one byte 0, in an edge input's curves; a
//!   sketch profile's curves refer to the profile's objects there)
//! ```
//!
//! The curve is the edge as the input saw it: a later item may have cut or
//! split the edge since, so the edge found in the history lies on the curve
//! and within its extent, not always at its ends.

use mitcad_asm::brep::{self, P3};

use super::ir::Geometry;
use super::stream::{Segment, f64_at, u32_at};

/// The base class of the stored curves.
pub const CURVE: &str = "518AD6D6-763A-472C-9F19-9CFF03E8DC45";
/// A line segment.
pub const LINE: &str = "50AEE8B4-9456-4DD7-847D-6F6C909BBB9D";
/// A circular arc or circle.
pub const ARC: &str = "B0B3FA59-C374-444F-B424-24AA5DE41645";
/// A B-spline curve.
pub const SPLINE: &str = "2319971D-8E07-441A-8F16-11C8F6F862D4";
/// An elliptical arc or ellipse.
pub const ELLIPSE: &str = "98E8921B-84FC-4863-ABEF-82976C0E80A0";

/// The bytes every stored curve ends with where its base classes' two
/// references are null (`00 refA 00 01 refB`, a null reference one byte 0).
#[cfg(test)]
const TRAILER: [u8; 5] = [0, 0, 0, 1, 0];

/// A stored curve (cm, rad).
#[derive(Clone, Debug, PartialEq)]
pub enum StoredCurve {
    Line {
        start: P3,
        /// From the start to the end.
        vector: P3,
    },
    Arc {
        center: P3,
        /// Unit axis; angles turn from `ref_dir` towards `axis × ref_dir`.
        axis: P3,
        ref_dir: P3,
        radius: f64,
        /// Start and end angle.
        range: [f64; 2],
    },
    Ellipse {
        center: P3,
        /// The unit directions of its major and minor axes.
        major_dir: P3,
        minor_dir: P3,
        major: f64,
        minor: f64,
        /// Start and end of the parameter `t` of `center + major cos t ·
        /// major_dir + minor sin t · minor_dir`.
        range: [f64; 2],
    },
    Spline {
        degree: usize,
        /// Every knot, repeated by its multiplicity.
        knots: Vec<f64>,
        weights: Option<Vec<f64>>,
        poles: Vec<P3>,
        /// The edge's parameter range on the spline.
        range: [f64; 2],
    },
}

fn p3_at(d: &[u8], p: usize) -> Option<P3> {
    Some([f64_at(d, p)?, f64_at(d, p + 8)?, f64_at(d, p + 16)?])
}

/// The highest spline degree read: the geometry kernel's B-spline curves
/// go up to degree 25 (as the sketch splines' reader, `sketch::nurbs_at`),
/// so a stored curve above it could not be built anyway (mitcad#934).
const MAX_SPLINE_DEGREE: usize = 25;

/// The most poles a stored spline may have (mitcad#934): with the degree
/// capped, one evaluation (a span search over the poles, then de Boor's
/// degree² steps) stays near 10^5 steps; an edge's spline has far fewer.
const MAX_SPLINE_POLES: usize = 100_000;

/// `u32 n | u32 n | u32 8 | n × (k × f64)` at `p`, and where it ends.
fn array_at(d: &[u8], p: usize, k: usize) -> Option<(Vec<f64>, usize)> {
    let n = u32_at(d, p)? as usize;
    if u32_at(d, p + 4)? as usize != n || u32_at(d, p + 8)? != 8 || n > 1_000_000 {
        return None;
    }
    let start = p + 12;
    let end = start.checked_add(n.checked_mul(8 * k)?)?;
    if end > d.len() {
        return None;
    }
    let values = (0..n * k)
        .map(|i| f64_at(d, start + 8 * i))
        .collect::<Option<_>>()?;
    Some((values, end))
}

/// Decodes a curve object's data whose root part ends at `p`, by its class.
fn parse(guid: &str, d: &[u8], p: usize) -> Option<(StoredCurve, usize)> {
    let (curve, end) = match guid {
        LINE => (
            StoredCurve::Line {
                start: p3_at(d, p)?,
                vector: p3_at(d, p + 24)?,
            },
            p + 48,
        ),
        ARC => (
            StoredCurve::Arc {
                center: p3_at(d, p)?,
                axis: p3_at(d, p + 24)?,
                ref_dir: p3_at(d, p + 48)?,
                radius: f64_at(d, p + 72)?,
                range: [f64_at(d, p + 80)?, f64_at(d, p + 88)?],
            },
            p + 96,
        ),
        ELLIPSE => (
            StoredCurve::Ellipse {
                center: p3_at(d, p)?,
                major_dir: p3_at(d, p + 24)?,
                minor_dir: p3_at(d, p + 48)?,
                major: f64_at(d, p + 72)?,
                minor: f64_at(d, p + 80)?,
                range: [f64_at(d, p + 88)?, f64_at(d, p + 96)?],
            },
            p + 104,
        ),
        SPLINE => {
            let flag_a = *d.get(p)?;
            let flag_b = *d.get(p + 1)?;
            if flag_a > 1 || flag_b > 1 {
                return None;
            }
            let degree = u32_at(d, p + 2)? as usize;
            // Each evaluation costs O(degree²) (de Boor): a crafted degree of
            // 10^6 made one evaluation take minutes (mitcad#934).
            if !(1..=MAX_SPLINE_DEGREE).contains(&degree) {
                return None;
            }
            let (knots, q) = array_at(d, p + 14, 1)?;
            let (weights, q) = array_at(d, q, 1)?;
            if u32_at(d, q)? as usize > MAX_SPLINE_POLES {
                return None;
            }
            let (flat, q) = array_at(d, q, 3)?;
            let poles: Vec<P3> = flat.chunks(3).map(|c| [c[0], c[1], c[2]]).collect();
            let range = [f64_at(d, q + 10)?, f64_at(d, q + 18)?];
            let valid = degree >= 1
                && !poles.is_empty()
                && knots.len() == poles.len() + degree + 1
                && knots.windows(2).all(|w| w[0] <= w[1])
                && (weights.is_empty() || weights.len() == poles.len());
            if !valid {
                return None;
            }
            (
                StoredCurve::Spline {
                    degree,
                    knots,
                    weights: (!weights.is_empty()).then_some(weights),
                    poles,
                    range,
                },
                q + 26,
            )
        }
        _ => return None,
    };
    Some((curve, end))
}

/// The stored curve object `id`, if its class and layout are known.
pub fn decode(seg: &Segment, id: u64) -> Option<StoredCurve> {
    let d = seg.data_of(id);
    let guid = seg.guid_of(id)?;
    let (curve, end) = parse(guid, d, seg.root_part(d)?.end)?;
    (trailer_end(d, end, |p| seg.ref_at(d, p).map(|r| r.end)) == Some(d.len())).then_some(curve)
}

/// Where the base classes' data after a curve's own fields ends: `u8 0 |
/// ref | u8 0 | u8 1 | ref`, each reference null (one byte 0) or one that
/// `ref_end` reads (a curve of a sketch profile refers to the profile's
/// selection and its loop there).
fn trailer_end(d: &[u8], p: usize, ref_end: impl Fn(usize) -> Option<usize>) -> Option<usize> {
    let optional = |p: usize| match d.get(p)? {
        0 => Some(p + 1),
        _ => ref_end(p),
    };
    if *d.get(p)? != 0 {
        return None;
    }
    let q = optional(p + 1)?;
    if d.get(q..q + 2)? != [0, 1] {
        return None;
    }
    optional(q + 2)
}

/// The curve objects an edge input refers to: the first list `u32 n | n
/// refs` after its root part whose references are all curves.
pub fn ids_of_input(seg: &Segment, input: u64) -> Vec<u64> {
    let d = seg.data_of(input);
    let Some(start) = seg.root_part(d).map(|r| r.end) else {
        return Vec::new();
    };
    for p in start..d.len().saturating_sub(4) {
        let Some(n) = u32_at(d, p).filter(|n| (1..=64).contains(n)) else {
            continue;
        };
        if d.get(p + 4) != Some(&1) {
            continue;
        }
        if let Some((ids, _)) = seg.ref_list_at(d, p)
            && ids.len() == n as usize
            && ids.iter().all(|&id| seg.is_kind_of(id, CURVE))
        {
            return ids;
        }
    }
    Vec::new()
}

/// The curves an edge input stores, when it stores them and all decode.
pub fn of_input(seg: &Segment, input: u64) -> Option<Vec<StoredCurve>> {
    let ids = ids_of_input(seg, input);
    if ids.is_empty() {
        return None;
    }
    ids.iter().map(|&id| decode(seg, id)).collect()
}

impl StoredCurve {
    /// The curve as the dump IR's geometry (cm): `Line3D` with its ends,
    /// `Arc3D` (`normal` its axis, angles from `referenceVector`),
    /// `NurbsCurve3D` (`nurbs`: degree, knots, poles, weights and the
    /// edge's `range`).
    pub fn to_geometry(&self) -> Geometry {
        match self {
            StoredCurve::Line { start, vector } => Geometry {
                geometry_type: Some("Line3D".to_owned()),
                start_point: Some(*start),
                end_point: Some(brep::add(*start, *vector)),
                ..Geometry::default()
            },
            StoredCurve::Arc {
                center,
                axis,
                ref_dir,
                radius,
                range,
            } => Geometry {
                geometry_type: Some("Arc3D".to_owned()),
                center: Some(*center),
                normal: Some(*axis),
                reference_vector: Some(*ref_dir),
                radius: Some(*radius),
                start_angle: Some(range[0]),
                end_angle: Some(range[1]),
                ..Geometry::default()
            },
            StoredCurve::Ellipse {
                center,
                major_dir,
                minor_dir,
                major,
                minor,
                range,
            } => Geometry {
                geometry_type: Some("EllipticalArc3D".to_owned()),
                center: Some(*center),
                // The minor axis is normal × major axis.
                normal: Some(brep::cross(*major_dir, *minor_dir)),
                major_axis_direction: Some(*major_dir),
                major_radius: Some(*major),
                minor_radius: Some(*minor),
                start_angle: Some(range[0]),
                end_angle: Some(range[1]),
                ..Geometry::default()
            },
            StoredCurve::Spline {
                degree,
                knots,
                weights,
                poles,
                range,
            } => Geometry {
                geometry_type: Some("NurbsCurve3D".to_owned()),
                nurbs: Some(serde_json::json!({
                    "degree": degree,
                    "knots": knots,
                    "weights": weights,
                    "controlPoints": poles,
                    "range": range,
                })),
                ..Geometry::default()
            },
        }
    }

    /// The curve the dump IR's geometry gives ([`Self::to_geometry`]).
    pub fn from_geometry(g: &Geometry) -> Option<StoredCurve> {
        match g.geometry_type.as_deref()? {
            "Line3D" => {
                let start = g.start_point?;
                Some(StoredCurve::Line {
                    start,
                    vector: brep::sub(g.end_point?, start),
                })
            }
            "Arc3D" => Some(StoredCurve::Arc {
                center: g.center?,
                axis: g.normal?,
                ref_dir: g.reference_vector?,
                radius: g.radius?,
                range: [g.start_angle?, g.end_angle?],
            }),
            "EllipticalArc3D" => Some(StoredCurve::Ellipse {
                center: g.center?,
                major_dir: g.major_axis_direction?,
                minor_dir: brep::cross(g.normal?, g.major_axis_direction?),
                major: g.major_radius?,
                minor: g.minor_radius?,
                range: [g.start_angle?, g.end_angle?],
            }),
            "NurbsCurve3D" => {
                let n = g.nurbs.as_ref()?;
                let f = |v: &serde_json::Value| v.as_f64();
                let list =
                    |k: &str| -> Option<Vec<f64>> { n.get(k)?.as_array()?.iter().map(f).collect() };
                let poles = n
                    .get("controlPoints")?
                    .as_array()?
                    .iter()
                    .map(|p| {
                        let a = p.as_array()?;
                        Some([f(a.first()?)?, f(a.get(1)?)?, f(a.get(2)?)?])
                    })
                    .collect::<Option<Vec<P3>>>()?;
                let range = list("range")?;
                Some(StoredCurve::Spline {
                    degree: usize::try_from(n.get("degree")?.as_u64()?).ok()?,
                    knots: list("knots")?,
                    weights: list("weights"),
                    poles,
                    range: [*range.first()?, *range.get(1)?],
                })
            }
            _ => None,
        }
    }

    /// The same curve with its lengths times `k` (cm to mm: 10).
    pub fn scaled(&self, k: f64) -> StoredCurve {
        let s = |p: P3| brep::scale(p, k);
        match self {
            StoredCurve::Line { start, vector } => StoredCurve::Line {
                start: s(*start),
                vector: s(*vector),
            },
            StoredCurve::Arc {
                center,
                axis,
                ref_dir,
                radius,
                range,
            } => StoredCurve::Arc {
                center: s(*center),
                axis: *axis,
                ref_dir: *ref_dir,
                radius: radius * k,
                range: *range,
            },
            StoredCurve::Ellipse {
                center,
                major_dir,
                minor_dir,
                major,
                minor,
                range,
            } => StoredCurve::Ellipse {
                center: s(*center),
                major_dir: *major_dir,
                minor_dir: *minor_dir,
                major: major * k,
                minor: minor * k,
                range: *range,
            },
            StoredCurve::Spline {
                degree,
                knots,
                weights,
                poles,
                range,
            } => StoredCurve::Spline {
                degree: *degree,
                knots: knots.clone(),
                weights: weights.clone(),
                poles: poles.iter().map(|p| s(*p)).collect(),
                range: *range,
            },
        }
    }

    /// The spline as a B-spline curve (distinct knots and multiplicities).
    fn bspline(&self) -> Option<brep::BSplineCurve> {
        let StoredCurve::Spline {
            degree,
            knots,
            weights,
            poles,
            ..
        } = self
        else {
            return None;
        };
        let mut distinct: Vec<f64> = Vec::new();
        let mut mults: Vec<usize> = Vec::new();
        for &k in knots {
            match distinct.last() {
                Some(&last) if k - last <= 1e-12 * (1.0 + k.abs()) => {
                    *mults.last_mut().expect("with a knot") += 1;
                }
                _ => {
                    distinct.push(k);
                    mults.push(1);
                }
            }
        }
        let b = brep::BSplineCurve {
            degree: *degree,
            knots: distinct,
            mults,
            poles: poles.clone(),
            weights: weights.clone(),
            periodic: false,
        };
        b.is_valid().then_some(b)
    }

    /// The point at parameter `t` (an arc's angle, a line's fraction of its
    /// vector, a spline's parameter).
    pub fn eval(&self, t: f64) -> Option<P3> {
        match self {
            StoredCurve::Line { start, vector } => Some(brep::add(*start, brep::scale(*vector, t))),
            StoredCurve::Arc {
                center,
                axis,
                ref_dir,
                radius,
                ..
            } => {
                let y = brep::cross(*axis, *ref_dir);
                Some(brep::add(
                    *center,
                    brep::add(
                        brep::scale(*ref_dir, radius * t.cos()),
                        brep::scale(y, radius * t.sin()),
                    ),
                ))
            }
            StoredCurve::Ellipse {
                center,
                major_dir,
                minor_dir,
                major,
                minor,
                ..
            } => Some(brep::add(
                *center,
                brep::add(
                    brep::scale(*major_dir, major * t.cos()),
                    brep::scale(*minor_dir, minor * t.sin()),
                ),
            )),
            StoredCurve::Spline { .. } => Some(self.bspline()?.eval(t)),
        }
    }

    /// The ends of the edge the curve stores (its start and end).
    pub fn ends(&self) -> Option<[P3; 2]> {
        let [a, b] = match self {
            StoredCurve::Line { .. } => [0.0, 1.0],
            StoredCurve::Arc { range, .. }
            | StoredCurve::Ellipse { range, .. }
            | StoredCurve::Spline { range, .. } => *range,
        };
        Some([self.eval(a)?, self.eval(b)?])
    }

    /// The length of the edge the curve stores: exact for a line and an arc,
    /// the polyline through 1024 points of its range for an ellipse or a
    /// spline (within a few parts per million of the curve's).
    pub fn length(&self) -> Option<f64> {
        let range = match self {
            StoredCurve::Line { vector, .. } => return Some(brep::norm(*vector)),
            StoredCurve::Arc { radius, range, .. } => {
                return Some(radius.abs() * (range[1] - range[0]).abs());
            }
            StoredCurve::Ellipse { range, .. } | StoredCurve::Spline { range, .. } => *range,
        };
        const N: usize = 1024;
        let at = |i: usize| self.eval(range[0] + (range[1] - range[0]) * i as f64 / N as f64);
        let mut last = at(0)?;
        let mut sum = 0.0;
        for i in 1..=N {
            let p = at(i)?;
            sum += brep::dist(last, p);
            last = p;
        }
        Some(sum)
    }

    /// How a point lies to the curve: its distance from the curve's carrier
    /// (the whole line, circle or spline) and how far (along the curve,
    /// roughly) it lies beyond the stored extent (0 within it).
    pub fn place(&self, p: P3) -> Option<(f64, f64)> {
        match self {
            StoredCurve::Line { start, vector } => {
                let l2 = brep::dot(*vector, *vector);
                if l2 <= 0.0 {
                    return None;
                }
                let d = brep::sub(p, *start);
                let t = brep::dot(d, *vector) / l2;
                let off = brep::norm(brep::sub(d, brep::scale(*vector, t)));
                let beyond = (-t).max(t - 1.0).max(0.0) * l2.sqrt();
                Some((off, beyond))
            }
            StoredCurve::Arc {
                center,
                axis,
                ref_dir,
                radius,
                range,
            } => {
                let d = brep::sub(p, *center);
                let h = brep::dot(d, *axis);
                let radial = brep::sub(d, brep::scale(*axis, h));
                let rho = brep::norm(radial);
                let off = (rho - radius).hypot(h);
                let y = brep::cross(*axis, *ref_dir);
                let angle = brep::dot(radial, y).atan2(brep::dot(radial, *ref_dir));
                let tau = std::f64::consts::TAU;
                let (a, b) = (range[0].min(range[1]), range[0].max(range[1]));
                // The angle on the arc's turn from its start.
                let t = a + (angle - a).rem_euclid(tau);
                let beyond = if t <= b {
                    0.0
                } else {
                    (t - b).min(a + tau - t).max(0.0) * radius
                };
                Some((off, beyond))
            }
            StoredCurve::Ellipse { range, .. } => {
                let tau = std::f64::consts::TAU;
                let (a, b) = (range[0].min(range[1]), range[0].max(range[1]));
                let on = |t: f64| self.eval(t).unwrap_or([f64::NAN; 3]);
                // The closest point of the whole ellipse, its parameter on
                // the turn from the arc's start.
                let t = closest(&on, p, a, a + tau);
                let off = brep::dist(p, on(t));
                let t = a + (t - a).rem_euclid(tau);
                let beyond = if t <= b {
                    0.0
                } else {
                    brep::dist(on(t), on(b)).min(brep::dist(on(t), on(a)))
                };
                Some((off, beyond))
            }
            StoredCurve::Spline { range, .. } => {
                let b = self.bspline()?;
                let (lo, hi) = b.range();
                let t = closest(&|t| b.eval(t), p, lo, hi);
                let q = b.eval(t);
                let off = brep::dist(p, q);
                let (a, z) = (range[0].min(range[1]), range[0].max(range[1]));
                let beyond = if t < a {
                    brep::dist(q, b.eval(a))
                } else if t > z {
                    brep::dist(q, b.eval(z))
                } else {
                    0.0
                };
                Some((off, beyond))
            }
        }
    }

    /// How an edge with these ends and middle fits the curve: the largest
    /// distance of the points from the carrier and beyond the extent, and
    /// whether its ends are the curve's ends (either way round), all
    /// within `tol`.
    pub fn fit(&self, start: P3, mid: P3, end: P3, tol: f64) -> Option<CurveFit> {
        let mut off: f64 = 0.0;
        let mut beyond: f64 = 0.0;
        for p in [start, mid, end] {
            let (o, b) = self.place(p)?;
            off = off.max(o);
            beyond = beyond.max(b);
        }
        let [a, b] = self.ends()?;
        let near = |x: P3, y: P3| brep::dist(x, y) <= tol;
        // A closed edge on a closed curve is the whole curve, wherever the
        // two start (a circle's seam may lie elsewhere in the bodies).
        let closed = near(a, b) && near(start, end);
        let same_ends =
            closed || (near(start, a) && near(end, b)) || (near(start, b) && near(end, a));
        Some(CurveFit {
            off,
            beyond,
            same_ends: same_ends && off <= tol,
        })
    }
}

/// How an edge lies to the curve its input stores ([`StoredCurve::fit`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CurveFit {
    /// The largest distance of its ends and middle from the carrier.
    pub off: f64,
    /// The largest distance of its ends and middle beyond the extent.
    pub beyond: f64,
    /// Its ends are the curve's ends.
    pub same_ends: bool,
}

impl CurveFit {
    /// Ranks of fit, best first: 0 the stored edge itself (its ends),
    /// 1 a piece of it (on the carrier, within the extent), 2 on the
    /// carrier beyond the extent, 3 off the curve.
    pub fn rank(&self, tol: f64) -> u8 {
        if self.off > tol {
            3
        } else if self.same_ends {
            0
        } else if self.beyond <= tol {
            1
        } else {
            2
        }
    }

    /// A short word for the dump (`_f3d.curve_fit`).
    pub fn describe(&self, tol: f64) -> String {
        match self.rank(tol) {
            0 => "the stored edge".to_owned(),
            1 => "a piece of the stored edge".to_owned(),
            2 => format!("on the stored curve, {:.4} beyond its ends", self.beyond),
            _ => format!("off the stored curve by {:.4}", self.off),
        }
    }
}

/// The parameter of the point of a curve (`eval`) in `[lo, hi]` closest to
/// `p`: the best of 256 samples, refined by golden-section search around
/// it.
fn closest(eval: &dyn Fn(f64) -> P3, p: P3, lo: f64, hi: f64) -> f64 {
    const N: usize = 256;
    let at = |i: usize| lo + (hi - lo) * i as f64 / N as f64;
    let d = |t: f64| brep::dist(eval(t), p);
    let best = (0..=N)
        .min_by(|&i, &j| d(at(i)).total_cmp(&d(at(j))))
        .unwrap_or(0);
    let (mut a, mut z) = (at(best.saturating_sub(1)), at((best + 1).min(N)));
    let g = (5f64.sqrt() - 1.0) / 2.0;
    for _ in 0..80 {
        let (x, y) = (z - g * (z - a), a + g * (z - a));
        if d(x) <= d(y) {
            z = y;
        } else {
            a = x;
        }
    }
    0.5 * (a + z)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A curve's data from its own fields on (no references after them).
    fn whole(guid: &str, d: &[u8]) -> Option<StoredCurve> {
        let (curve, end) = parse(guid, d, 0)?;
        (trailer_end(d, end, |_| None) == Some(d.len())).then_some(curve)
    }

    #[test]
    fn reads_the_references_after_a_curve() {
        // A sketch profile's curve refers to two objects there (here an
        // 11-byte reference each).
        let r = |p: usize| Some(p + 11);
        let mut d = vec![0];
        d.extend([1; 11]);
        d.extend([0, 1]);
        d.extend([1; 11]);
        assert_eq!(trailer_end(&d, 0, r), Some(d.len()));
        assert_eq!(trailer_end(&TRAILER, 0, r), Some(5));
        assert_eq!(trailer_end(&[0, 0, 1, 1, 0], 0, r), None);
    }

    fn data(parts: &[&[u8]]) -> Vec<u8> {
        parts.concat()
    }

    fn f(xs: &[f64]) -> Vec<u8> {
        xs.iter().flat_map(|x| x.to_le_bytes()).collect()
    }

    fn u(xs: &[u32]) -> Vec<u8> {
        xs.iter().flat_map(|x| x.to_le_bytes()).collect()
    }

    #[test]
    fn reads_a_line() {
        // A vertical edge of a 6 × 4 × 2 cm block.
        let d = data(&[&f(&[6.0, 4.0, 0.0, 0.0, 0.0, 2.0]), &TRAILER]);
        let c = whole(LINE, &d).unwrap();
        assert_eq!(
            c,
            StoredCurve::Line {
                start: [6.0, 4.0, 0.0],
                vector: [0.0, 0.0, 2.0]
            }
        );
        assert_eq!(c.ends().unwrap(), [[6.0, 4.0, 0.0], [6.0, 4.0, 2.0]]);
        let g = c.to_geometry();
        assert_eq!(g.end_point, Some([6.0, 4.0, 2.0]));
        assert_eq!(StoredCurve::from_geometry(&g), Some(c.clone()));
        // A lower piece of it, and a point beyond its top.
        let fit = c
            .fit([6.0, 4.0, 0.0], [6.0, 4.0, 0.5], [6.0, 4.0, 1.0], 1e-6)
            .unwrap();
        assert_eq!(fit.rank(1e-6), 1);
        let fit = c
            .fit([6.0, 4.0, 0.0], [6.0, 4.0, 1.5], [6.0, 4.0, 3.0], 1e-6)
            .unwrap();
        assert_eq!(fit.rank(1e-6), 2);
        assert!((fit.beyond - 1.0).abs() < 1e-12);
        let fit = c
            .fit([6.0, 4.0, 2.0], [6.0, 4.0, 1.0], [6.0, 4.0, 0.0], 1e-6)
            .unwrap();
        assert_eq!(fit.rank(1e-6), 0);
        let fit = c
            .fit([0.0, 4.0, 0.0], [0.0, 4.0, 1.0], [0.0, 4.0, 2.0], 1e-6)
            .unwrap();
        assert_eq!(fit.rank(1e-6), 3);
        // A layout that does not end with the trailer is refused.
        let mut bad = d.clone();
        bad.push(0);
        assert_eq!(whole(LINE, &bad), None);
        assert_eq!(whole(ARC, &d), None);
    }

    #[test]
    fn reads_an_arc() {
        // A quarter round of 0.5 cm about (5.5, 0.5, 2), its axis down: it
        // turns from +x towards -y.
        let d = data(&[
            &f(&[5.5, 0.5, 2.0, 0.0, 0.0, -1.0, 1.0, 0.0, 0.0, 0.5]),
            &f(&[0.0, std::f64::consts::FRAC_PI_2]),
            &TRAILER,
        ]);
        let c = whole(ARC, &d).unwrap();
        let [a, b] = c.ends().unwrap();
        let close = |p: P3, q: P3| brep::dist(p, q) < 1e-12;
        assert!(close(a, [6.0, 0.5, 2.0]));
        assert!(close(b, [5.5, 0.0, 2.0]));
        assert_eq!(
            StoredCurve::from_geometry(&c.to_geometry()),
            Some(c.clone())
        );
        let s = std::f64::consts::FRAC_1_SQRT_2 * 0.5;
        let mid = [5.5 + s, 0.5 - s, 2.0];
        assert_eq!(c.fit(a, mid, b, 1e-6).unwrap().rank(1e-6), 0);
        // The same circle on the other side: beyond the extent.
        let other = [5.5 - s, 0.5 + s, 2.0];
        let fit = c
            .fit([5.0, 0.5, 2.0], other, [5.5, 1.0, 2.0], 1e-6)
            .unwrap();
        assert_eq!(fit.rank(1e-6), 2);
        // Another radius: off the curve.
        let fit = c.fit([6.1, 0.5, 2.0], mid, b, 1e-6).unwrap();
        assert_eq!(fit.rank(1e-6), 3);
        // A whole circle is a closed edge on it, wherever the edge starts.
        let circle = StoredCurve::Arc {
            center: [0.0; 3],
            axis: [0.0, 0.0, 1.0],
            ref_dir: [1.0, 0.0, 0.0],
            radius: 1.0,
            range: [0.0, std::f64::consts::TAU],
        };
        let fit = circle.fit([-1.0, 0.0, 0.0], [1.0, 0.0, 0.0], [-1.0, 0.0, 0.0], 1e-6);
        assert_eq!(fit.unwrap().rank(1e-6), 0);
        // In millimetres.
        let mm = c.scaled(10.0);
        let [a, _] = mm.ends().unwrap();
        assert!(close(a, [60.0, 5.0, 20.0]));
    }

    #[test]
    fn reads_an_elliptical_arc() {
        // A quarter of an ellipse of radii 2 and 1 about the origin, its
        // major axis along x and its minor axis along y, from -y to +x.
        let half_pi = std::f64::consts::FRAC_PI_2;
        let d = data(&[
            &f(&[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 2.0, 1.0]),
            &f(&[-half_pi, 0.0]),
            &TRAILER,
        ]);
        let c = whole(ELLIPSE, &d).unwrap();
        let [a, b] = c.ends().unwrap();
        let close = |p: P3, q: P3| brep::dist(p, q) < 1e-12;
        assert!(
            close(a, [0.0, -1.0, 0.0]) && close(b, [2.0, 0.0, 0.0]),
            "{a:?} {b:?}"
        );
        assert_eq!(
            StoredCurve::from_geometry(&c.to_geometry()),
            Some(c.clone())
        );
        let mid = c.eval(-half_pi / 2.0).unwrap();
        assert_eq!(c.fit(a, mid, b, 1e-6).unwrap().rank(1e-6), 0);
        // The other half of the ellipse lies beyond the arc; a point off it.
        let fit = c.fit([-2.0, 0.0, 0.0], [0.0, 1.0, 0.0], [2.0, 0.0, 0.0], 1e-6);
        assert_eq!(fit.unwrap().rank(1e-6), 2);
        assert!((c.place([0.0, 0.0, 0.0]).unwrap().0 - 1.0).abs() < 1e-9);
        let mm = c.scaled(10.0);
        assert!(close(mm.ends().unwrap()[1], [20.0, 0.0, 0.0]));
    }

    #[test]
    fn reads_a_spline() {
        // A cubic Bézier from (0,0,0) to (3,0,0) bulging in y, its edge the
        // middle third of the parameter range.
        let knots = [0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0];
        let poles = [0.0, 0.0, 0.0, 1.0, 1.0, 0.0, 2.0, 1.0, 0.0, 3.0, 0.0, 0.0];
        let d = data(&[
            &[0, 0],
            &u(&[3]),
            &f(&[1e-9]),
            &u(&[8, 8, 8]),
            &f(&knots),
            &u(&[0, 0, 8]),
            &u(&[4, 4, 8]),
            &f(&poles),
            &f(&[1e-12]),
            &[1, 1],
            &f(&[1.0 / 3.0, 2.0 / 3.0]),
            &TRAILER,
        ]);
        let c = whole(SPLINE, &d).unwrap();
        let StoredCurve::Spline {
            degree,
            ref weights,
            ..
        } = c
        else {
            panic!("a spline");
        };
        assert_eq!((degree, weights.is_none()), (3, true));
        let [a, b] = c.ends().unwrap();
        let mid = c.eval(0.5).unwrap();
        assert!((mid[0] - 1.5).abs() < 1e-12 && (mid[1] - 0.75).abs() < 1e-12);
        assert_eq!(c.fit(a, mid, b, 1e-6).unwrap().rank(1e-6), 0);
        assert_eq!(
            StoredCurve::from_geometry(&c.to_geometry()),
            Some(c.clone())
        );
        // The whole curve: beyond the stored extent at both ends.
        let fit = c.fit([0.0, 0.0, 0.0], mid, [3.0, 0.0, 0.0], 1e-6).unwrap();
        assert_eq!(fit.rank(1e-6), 2);
        // A point off it.
        assert!(c.place([1.5, 2.0, 0.0]).unwrap().0 > 1.0);
        // Counts that disagree are refused.
        let mut bad = d.clone();
        bad[14] = 9;
        assert_eq!(whole(SPLINE, &bad), None);
    }

    /// A spline of a degree no stored curve has (10 000, with 10 001 poles
    /// and consistent counts) is refused at once instead of being read and
    /// evaluated at O(degree²) per point (mitcad#934).
    #[test]
    fn refuses_a_spline_of_a_huge_degree() {
        // A Bézier curve of this degree along x: degree + 1 poles, the
        // knots 0 and 1 each degree + 1 times.
        let bezier = |degree: u32| {
            let n = degree + 1;
            let knots: Vec<f64> = (0..2 * n).map(|i| if i < n { 0.0 } else { 1.0 }).collect();
            let poles: Vec<f64> = (0..n).flat_map(|i| [f64::from(i), 0.0, 0.0]).collect();
            data(&[
                &[0, 0],
                &u(&[degree]),
                &f(&[1e-9]),
                &u(&[2 * n, 2 * n, 8]),
                &f(&knots),
                &u(&[0, 0, 8]),
                &u(&[n, n, 8]),
                &f(&poles),
                &f(&[1e-12]),
                &[1, 1],
                &f(&[0.0, 1.0]),
                &TRAILER,
            ])
        };
        let huge = bezier(10_000);
        let started = std::time::Instant::now();
        assert_eq!(whole(SPLINE, &huge), None);
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
        // The same layout at degree 25 is read.
        assert!(matches!(
            whole(SPLINE, &bezier(25)),
            Some(StoredCurve::Spline { degree: 25, .. })
        ));
    }
}
