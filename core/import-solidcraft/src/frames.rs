// SPDX-License-Identifier: MIT
//! Frame members (mitcad#2157): one body per straight line of a sketch, a
//! profile laid square to the line at its start and run the line's length
//! (SolidCraft's `eval_frame.rs` and `frame.rs`), as MitCad features that
//! follow the line:
//!
//! - a round bar or round tube whose line runs through its centre: one
//!   `pipe` per line, its diameter and wall the file's inputs (their
//!   parameters);
//! - every other section (a bar, rectangular tube, angle, channel or I
//!   beam, a sketch's profiles, a round one off its centre): per line a
//!   construction plane along the line at its start, a sketch on it with
//!   the section in the sketch's frame (the profile point the file puts on
//!   the line at the plane's origin, the file's turn and mirror), and a
//!   `sweep` of it along the line, square to it.
//!
//! The profile's axes are SolidCraft's (`member_axes`: its y up towards
//! world +Z as far as the line allows, x across), kept as the sketch's
//! frame on MitCad's plane; after an edit of the line the section keeps
//! that turn from the plane's own x axis. Bodies are named as the file
//! names them (`names` by line, else `body_name` or the profile's
//! description, made unique). End caps stay placeholders.

use std::collections::{BTreeMap, HashMap};

use mitcad_model::{ComponentUid, Datum, Kernel};
use mitcad_solidcraft::Feature;
use mitcad_solidcraft::inputs::{self, Kind};
use mitcad_solidcraft::kinds::{FrameMember, MemberProfile, ProfileSel};
use mitcad_solidcraft::sketch::{CurveKind, Sketch};
use serde_json::{Value, json};

use super::planes::Frame;
use super::sketch::inside;
use super::{Importer, MapResult, Mapped, NotMapped};
use mitcad_import_base::geom::{add, cross, dot, norm, scale, sub, unit};

/// Shortest member (mm).
const MIN_LENGTH: f64 = 1e-6;
/// How far (radians) a line may lean from vertical and still be vertical.
const VERTICAL_TOLERANCE: f64 = 1e-6;
/// The smallest profile radius that is not a sharp corner (mm).
const MIN_PROFILE_DIMENSION: f64 = 0.01;

type V2 = [f64; 2];

/// A member's line: its id, its ends in component coordinates, the path
/// along it.
struct Line {
    id: String,
    a: [f64; 3],
    b: [f64; 3],
    path: Value,
}

/// A piece of a profile's outline, in its own coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Seg {
    Line {
        a: V2,
        b: V2,
    },
    /// From `start` (radians) through `sweep` (signed) about `center`.
    Arc {
        center: V2,
        radius: f64,
        start: f64,
        sweep: f64,
    },
    Circle {
        center: V2,
        radius: f64,
    },
}

impl Seg {
    fn point(&self, t: f64) -> V2 {
        match *self {
            Seg::Line { a, b } => [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t],
            Seg::Arc {
                center,
                radius,
                start,
                sweep,
            } => {
                let s = start + sweep * t;
                [center[0] + radius * s.cos(), center[1] + radius * s.sin()]
            }
            Seg::Circle { center, radius } => {
                let s = std::f64::consts::TAU * t;
                [center[0] + radius * s.cos(), center[1] + radius * s.sin()]
            }
        }
    }

    fn derivative(&self, t: f64) -> V2 {
        match *self {
            Seg::Line { a, b } => [b[0] - a[0], b[1] - a[1]],
            Seg::Arc {
                radius,
                start,
                sweep,
                ..
            } => {
                let s = start + sweep * t;
                [-radius * sweep * s.sin(), radius * sweep * s.cos()]
            }
            Seg::Circle { radius, .. } => {
                let w = std::f64::consts::TAU;
                let s = w * t;
                [-radius * w * s.sin(), radius * w * s.cos()]
            }
        }
    }

    /// Mirrored across the y axis (x to -x).
    fn mirrored(&self) -> Seg {
        let m = |p: V2| [-p[0], p[1]];
        match *self {
            Seg::Line { a, b } => Seg::Line { a: m(a), b: m(b) },
            Seg::Arc {
                center,
                radius,
                start,
                sweep,
            } => Seg::Arc {
                center: m(center),
                radius,
                start: std::f64::consts::PI - start,
                sweep: -sweep,
            },
            Seg::Circle { center, radius } => Seg::Circle {
                center: m(center),
                radius,
            },
        }
    }

    /// Points that include its extremes along x and y.
    fn extremes(&self) -> Vec<V2> {
        match *self {
            Seg::Line { a, b } => vec![a, b],
            Seg::Arc { start, sweep, .. } => {
                let mut out = vec![self.point(0.0), self.point(1.0)];
                let (lo, hi) = if sweep >= 0.0 {
                    (start, start + sweep)
                } else {
                    (start + sweep, start)
                };
                let quarter = std::f64::consts::FRAC_PI_2;
                let mut k = (lo / quarter).ceil();
                while k * quarter <= hi {
                    let t = (k * quarter - start) / sweep;
                    out.push(self.point(t));
                    k += 1.0;
                }
                out
            }
            Seg::Circle { center, radius } => vec![
                [center[0] - radius, center[1]],
                [center[0] + radius, center[1]],
                [center[0], center[1] - radius],
                [center[0], center[1] + radius],
            ],
        }
    }
}

/// A built-in profile: its outline and holes (each a loop of pieces), its
/// own origin and what its members are named after.
#[derive(Debug, Clone)]
struct Library {
    kind: String,
    loops: Vec<Vec<Seg>>,
    origin: V2,
    description: String,
}

/// A number for a name: up to four decimals, without trailing zeros.
fn short(x: f64) -> String {
    let t = format!("{x:.4}");
    let t = if t.contains('.') {
        t.trim_end_matches('0').trim_end_matches('.')
    } else {
        &t
    };
    if t == "-0" { "0".into() } else { t.to_string() }
}

fn room(x: f64) -> f64 {
    if x.is_finite() { x.max(0.0) } else { 0.0 }
}

/// A tapered flange's thickness at its toe and at the web.
fn tapered_flange_ends(inside: f64, thickness: f64, slope_deg: f64) -> (f64, f64) {
    let change = inside / 2.0 * slope_deg.to_radians().tan();
    (thickness - change, thickness + change)
}

impl Library {
    /// The profile of `kind` with its dimensions (mm; a flange slope in
    /// degrees), defaults worked out as SolidCraft's `Profile::filled`.
    fn new(kind: &str, d: &BTreeMap<String, f64>) -> Result<Library, String> {
        let need = |n: &str| -> Result<f64, String> {
            d.get(n)
                .copied()
                .filter(|v| v.is_finite() && *v > 0.0)
                .ok_or_else(|| format!("a {kind} profile without its {n}"))
        };
        let opt = |n: &str| d.get(n).copied();
        let d2 = |v: &[f64]| v.iter().map(|x| short(*x)).collect::<Vec<_>>().join("x");
        let shifted = |pts: &[(f64, f64)], w: f64, h: f64| -> Vec<V2> {
            pts.iter()
                .map(|&(x, y)| [x - w / 2.0, y - h / 2.0])
                .collect()
        };
        let (loops, origin, description) = match kind {
            "bar" => {
                let (w, h) = (need("width")?, need("height")?);
                let r = opt("corner_radius").unwrap_or(0.0);
                (
                    vec![rounded_rect(w, h, r)?],
                    [-w / 2.0, -h / 2.0],
                    format!("Bar {}", d2(&[w, h])),
                )
            }
            "round_bar" => {
                let dia = need("diameter")?;
                (
                    vec![vec![Seg::Circle {
                        center: [0.0; 2],
                        radius: dia / 2.0,
                    }]],
                    [0.0; 2],
                    format!("Round Bar {}", short(dia)),
                )
            }
            "rect_tube" => {
                let (w, h, t) = (need("width")?, need("height")?, need("wall")?);
                let ro = opt("outer_radius").unwrap_or_else(|| (2.0 * t).min(w.min(h) / 2.0));
                let ri = opt("inner_radius").unwrap_or_else(|| {
                    let ri = room(ro - t);
                    if ri < MIN_PROFILE_DIMENSION { 0.0 } else { ri }
                });
                (
                    vec![
                        rounded_rect(w, h, ro)?,
                        rounded_rect(w - 2.0 * t, h - 2.0 * t, ri)?,
                    ],
                    [0.0; 2],
                    format!("Rect Tube {}", d2(&[w, h, t])),
                )
            }
            "round_tube" => {
                let (dia, t) = (need("diameter")?, need("wall")?);
                if 2.0 * t >= dia {
                    return Err("a round tube whose wall fills it".to_owned());
                }
                (
                    vec![
                        vec![Seg::Circle {
                            center: [0.0; 2],
                            radius: dia / 2.0,
                        }],
                        vec![Seg::Circle {
                            center: [0.0; 2],
                            radius: dia / 2.0 - t,
                        }],
                    ],
                    [0.0; 2],
                    format!("Round Tube {}", d2(&[dia, t])),
                )
            }
            "angle" => {
                let (a, b, t) = (need("width")?, need("height")?, need("thickness")?);
                let leg = room(a.min(b) - t);
                let (r1, r2) = match (opt("root_radius"), opt("toe_radius")) {
                    (Some(r1), Some(r2)) => (r1, r2),
                    (Some(r1), None) => (r1, (t / 2.0).min(room(leg - r1))),
                    (None, Some(r2)) => (t.min(room(leg - r2)), r2),
                    (None, None) => {
                        let k = if 1.5 * t > leg { leg / (1.5 * t) } else { 1.0 };
                        (t * k, t / 2.0 * k)
                    }
                };
                let pts = [(0.0, 0.0), (a, 0.0), (a, t), (t, t), (t, b), (0.0, b)];
                let radii = [0.0, 0.0, r2, r1, r2, 0.0];
                (
                    vec![rounded_polygon(&shifted(&pts, a, b), &radii)?],
                    [-a / 2.0, -b / 2.0],
                    format!("Angle {}", d2(&[a, b, t])),
                )
            }
            "channel" => {
                let (h, b, tw, tf) = (
                    need("height")?,
                    need("width")?,
                    need("web")?,
                    need("flange")?,
                );
                let r2 = opt("toe_radius").unwrap_or(0.0);
                let slope = opt("flange_slope").unwrap_or(0.0);
                let (at_toe, at_web) = tapered_flange_ends(b - tw, tf, slope);
                let r1 = opt("root_radius")
                    .unwrap_or_else(|| tw.min(room(b - tw - r2)).min(room(h - 2.0 * at_web) / 2.0));
                let pts = [
                    (0.0, 0.0),
                    (b, 0.0),
                    (b, at_toe),
                    (tw, at_web),
                    (tw, h - at_web),
                    (b, h - at_toe),
                    (b, h),
                    (0.0, h),
                ];
                let radii = [0.0, 0.0, r2, r1, r1, r2, 0.0, 0.0];
                (
                    vec![rounded_polygon(&shifted(&pts, b, h), &radii)?],
                    [-b / 2.0, 0.0],
                    format!("Channel {}", d2(&[h, b, tw])),
                )
            }
            "i_beam" => {
                let (h, b, tw, tf) = (
                    need("height")?,
                    need("width")?,
                    need("web")?,
                    need("flange")?,
                );
                let r = opt("root_radius")
                    .unwrap_or_else(|| tw.min(room(b - tw) / 2.0).min(room(h - 2.0 * tf) / 2.0));
                let (x, y, w, f) = (b / 2.0, h / 2.0, tw / 2.0, h / 2.0 - tf);
                let pts = [
                    [-x, -y],
                    [x, -y],
                    [x, -f],
                    [w, -f],
                    [w, f],
                    [x, f],
                    [x, y],
                    [-x, y],
                    [-x, f],
                    [-w, f],
                    [-w, -f],
                    [-x, -f],
                ];
                let radii = [0.0, 0.0, 0.0, r, r, 0.0, 0.0, 0.0, 0.0, r, r, 0.0];
                (
                    vec![rounded_polygon(&pts, &radii)?],
                    [0.0; 2],
                    format!("I-Beam {}", d2(&[h, b, tw])),
                )
            }
            other => {
                return Err(format!(
                    "a profile of a kind this import does not know (`{other}`)"
                ));
            }
        };
        Ok(Library {
            kind: kind.to_owned(),
            loops,
            origin,
            description,
        })
    }

    fn round(&self) -> bool {
        matches!(self.kind.as_str(), "round_bar" | "round_tube")
    }
}

fn rounded_rect(w: f64, h: f64, r: f64) -> Result<Vec<Seg>, String> {
    let (x, y) = (w / 2.0, h / 2.0);
    rounded_polygon(&[[-x, -y], [x, -y], [x, y], [-x, y]], &[r; 4])
}

fn unit2(v: V2) -> Option<V2> {
    let n = (v[0] * v[0] + v[1] * v[1]).sqrt();
    (n > 1e-300 && n.is_finite()).then(|| [v[0] / n, v[1] / n])
}

/// The counter-clockwise polygon `pts` with each corner rounded by its
/// radius (0: sharp), the arc tangent to both edges (SolidCraft's
/// `rounded_polygon`).
fn rounded_polygon(pts: &[V2], radii: &[f64]) -> Result<Vec<Seg>, String> {
    let n = pts.len();
    if n < 3 || radii.len() != n {
        return Err("a profile polygon needs 3 or more corners".to_owned());
    }
    let mut corners: Vec<(V2, V2, Option<Seg>)> = Vec::with_capacity(n);
    for i in 0..n {
        let (prev, v, next, r) = (pts[(i + n - 1) % n], pts[i], pts[(i + 1) % n], radii[i]);
        let (Some(d1), Some(d2)) = (
            unit2([v[0] - prev[0], v[1] - prev[1]]),
            unit2([next[0] - v[0], next[1] - v[1]]),
        ) else {
            return Err("a profile polygon has two corners in one place".to_owned());
        };
        let turn = (d1[0] * d2[1] - d1[1] * d2[0]).atan2(d1[0] * d2[0] + d1[1] * d2[1]);
        if r <= 0.0 || turn.abs() < 1e-12 {
            corners.push((v, v, None));
            continue;
        }
        let l = r * (turn.abs() / 2.0).tan();
        let t1 = [v[0] - d1[0] * l, v[1] - d1[1] * l];
        let t2 = [v[0] + d2[0] * l, v[1] + d2[1] * l];
        let left = [-d1[1], d1[0]];
        let side = if turn > 0.0 {
            left
        } else {
            [-left[0], -left[1]]
        };
        let center = [t1[0] + side[0] * r, t1[1] + side[1] * r];
        corners.push((
            t1,
            t2,
            Some(Seg::Arc {
                center,
                radius: r,
                start: (t1[1] - center[1]).atan2(t1[0] - center[0]),
                sweep: turn,
            }),
        ));
    }
    let mut segs = Vec::with_capacity(2 * n);
    for i in 0..n {
        let (_, out, arc) = corners[i];
        let (inn, _, _) = corners[(i + 1) % n];
        let (a, b) = (pts[i], pts[(i + 1) % n]);
        if let Some(arc) = arc {
            segs.push(arc);
        }
        let along = (inn[0] - out[0]) * (b[0] - a[0]) + (inn[1] - out[1]) * (b[1] - a[1]);
        if along < -1e-9 * (1.0 + ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt()) {
            return Err("profile corner radii that do not fit along an edge".to_owned());
        }
        if ((inn[0] - out[0]).powi(2) + (inn[1] - out[1]).powi(2)).sqrt() > 1e-9 {
            segs.push(Seg::Line { a: out, b: inn });
        }
    }
    Ok(segs)
}

/// Gauss-Legendre nodes and weights on [0, 1].
const GL5: [(f64, f64); 5] = [
    (0.046_910_077_030_668, 0.118_463_442_528_094_5),
    (0.230_765_344_947_158_5, 0.239_314_335_249_683_2),
    (0.5, 0.284_444_444_444_444_4),
    (0.769_234_655_052_841_5, 0.239_314_335_249_683_2),
    (0.953_089_922_969_332, 0.118_463_442_528_094_5),
];

/// A loop's area and first moments (∫x dA, ∫y dA), counter-clockwise
/// positive, by Green's theorem.
fn loop_moments(l: &[Seg]) -> [f64; 3] {
    let mut m = [0.0; 3];
    for seg in l {
        let pieces = if matches!(seg, Seg::Line { .. }) {
            1
        } else {
            16
        };
        for k in 0..pieces {
            let h = 1.0 / f64::from(pieces);
            for (x, wt) in GL5 {
                let t = (f64::from(k) + x) * h;
                let (p, dp) = (seg.point(t), seg.derivative(t));
                let w = wt * h;
                m[0] += w * p[0] * dp[1];
                m[1] += w * 0.5 * p[0] * p[0] * dp[1];
                m[2] += w * -0.5 * p[1] * p[1] * dp[0];
            }
        }
    }
    m
}

/// The centroid of an outline and its holes (each loop either way round).
fn centroid(loops: &[Vec<Seg>]) -> V2 {
    let mut total = [0.0; 3];
    for (i, l) in loops.iter().enumerate() {
        let mut m = loop_moments(l);
        if m[0] < 0.0 {
            m = m.map(|v| -v);
        }
        let sign = if i == 0 { 1.0 } else { -1.0 };
        for (t, v) in total.iter_mut().zip(m) {
            *t += sign * v;
        }
    }
    if total[0].abs() > 1e-300 {
        [total[1] / total[0], total[2] / total[0]]
    } else {
        [0.0; 2]
    }
}

/// A bounding box's corners from points.
fn bounds(points: impl Iterator<Item = V2>) -> Option<(V2, V2)> {
    let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
    for p in points {
        lo = [lo[0].min(p[0]), lo[1].min(p[1])];
        hi = [hi[0].max(p[0]), hi[1].max(p[1])];
    }
    (lo[0] <= hi[0] && lo[1] <= hi[1]).then_some((lo, hi))
}

/// Where an anchor is on a profile of bounding box `lo`..`hi` and
/// centroid `c` (SolidCraft's `anchor_point`).
fn anchor_point(anchor: &str, lo: V2, hi: V2, c: V2) -> Result<V2, String> {
    let mid = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0];
    Ok(match anchor {
        "origin" => [0.0; 2],
        "center" => mid,
        "centroid" => c,
        "top_left" => [lo[0], hi[1]],
        "top" => [mid[0], hi[1]],
        "top_right" => hi,
        "left" => [lo[0], mid[1]],
        "right" => [hi[0], mid[1]],
        "bottom_left" => lo,
        "bottom" => [mid[0], lo[1]],
        "bottom_right" => [hi[0], lo[1]],
        other => return Err(format!("an anchor this import does not know (`{other}`)")),
    })
}

/// The directions of a member's profile x and y for a line along `d`, its
/// y up towards +Z as far as the line allows, turned by `angle` about the
/// line (SolidCraft's `member_axes`).
fn member_axes(d: [f64; 3], angle: f64) -> Option<([f64; 3], [f64; 3])> {
    let d = unit(d)?;
    let z = [0.0, 0.0, 1.0];
    let up = sub(z, scale(d, dot(z, d)));
    let (x, y) = match unit(up).filter(|_| norm(up) > VERTICAL_TOLERANCE) {
        Some(y) => (cross(y, d), y),
        None => ([1.0, 0.0, 0.0], cross(d, [1.0, 0.0, 0.0])),
    };
    let (c, s) = (angle.cos(), angle.sin());
    Some((add(scale(x, c), scale(y, s)), sub(scale(y, c), scale(x, s))))
}

/// The body name of the member on each line: its own from `pinned` when
/// free, else the first free one of `base`, `base (2)`, … that no line has
/// pinned (SolidCraft's `member_names`).
fn member_names(
    taken: &dyn Fn(&str) -> bool,
    lines: &[String],
    pinned: &BTreeMap<String, String>,
    base: &str,
) -> Vec<String> {
    let reserved: Vec<&str> = pinned.values().map(String::as_str).collect();
    let mut used: Vec<String> = Vec::new();
    let mut out: Vec<Option<String>> = lines
        .iter()
        .map(|line| {
            let n = pinned
                .get(line)
                .map(|n| n.trim())
                .filter(|n| !n.is_empty() && !taken(n))?;
            (!used.iter().any(|u| u == n)).then(|| {
                used.push(n.to_owned());
                n.to_owned()
            })
        })
        .collect();
    let base = if base.trim().is_empty() {
        "Member"
    } else {
        base.trim()
    };
    let mut next = 1usize;
    let limit = lines.len() + reserved.len() + used.len() + 10_000;
    for slot in out.iter_mut().filter(|n| n.is_none()) {
        while next <= limit {
            let candidate = if next == 1 {
                base.to_owned()
            } else {
                format!("{base} ({next})")
            };
            next += 1;
            if !taken(&candidate)
                && !used.contains(&candidate)
                && !reserved.contains(&candidate.as_str())
            {
                used.push(candidate.clone());
                *slot = Some(candidate);
                break;
            }
        }
    }
    out.into_iter()
        .map(|n| n.unwrap_or_else(|| base.to_owned()))
        .collect()
}

/// A sketch's profile as entities: fixed points, lines, arcs and circles.
fn profile_entities(loops: &[Vec<Seg>]) -> Vec<Value> {
    let mut points: Vec<(V2, String)> = Vec::new();
    let mut entities = Vec::new();
    let mut next = 1usize;
    let mut point = |p: V2, entities: &mut Vec<Value>, next: &mut usize| -> String {
        if let Some((_, id)) = points
            .iter()
            .find(|(q, _)| (q[0] - p[0]).abs() < 1e-9 && (q[1] - p[1]).abs() < 1e-9)
        {
            return id.clone();
        }
        let id = format!("p{next}");
        *next += 1;
        entities.push(json!({"id": id, "type": "point", "at": p, "fixed": true}));
        points.push((p, id.clone()));
        id
    };
    for l in loops {
        for seg in l {
            let e = match *seg {
                Seg::Line { a, b } => {
                    let (pa, pb) = (
                        point(a, &mut entities, &mut next),
                        point(b, &mut entities, &mut next),
                    );
                    json!({"type": "line", "start": pa, "end": pb})
                }
                Seg::Arc { center, sweep, .. } => {
                    // Counter-clockwise from start to end.
                    let (s, t) = if sweep >= 0.0 {
                        (seg.point(0.0), seg.point(1.0))
                    } else {
                        (seg.point(1.0), seg.point(0.0))
                    };
                    let c = point(center, &mut entities, &mut next);
                    let (ps, pt) = (
                        point(s, &mut entities, &mut next),
                        point(t, &mut entities, &mut next),
                    );
                    json!({"type": "arc", "center": c, "start": ps, "end": pt})
                }
                Seg::Circle { center, radius } => {
                    let c = point(center, &mut entities, &mut next);
                    json!({"type": "circle", "center": c, "radius": radius})
                }
            };
            let mut e = e;
            e["id"] = json!(format!("c{next}"));
            e["fixed"] = json!(true);
            next += 1;
            entities.push(e);
        }
    }
    entities
}

/// A sketch mirrored across its y axis (x to -x), arcs kept
/// counter-clockwise; a point placed by an expression of x has its sign
/// turned.
fn mirrored_sketch(s: &Sketch) -> Sketch {
    let mut m = s.clone();
    for p in &mut m.points {
        p.pos[0] = -p.pos[0];
        if let Some(e) = &mut p.exprs
            && let Some(x) = &mut e.x
        {
            *x = format!("-({x})");
        }
    }
    for c in &mut m.curves {
        if let CurveKind::Arc { a, b, .. } = &mut c.kind {
            std::mem::swap(a, b);
        }
    }
    m
}

/// The regions of a sketch that are material: each inside an even number
/// of the others' outlines (SolidCraft's `material`: a loop inside an
/// outline is a hole, one inside that material again).
fn material(regions: &[mitcad_model::sketch::regions::Region]) -> Vec<usize> {
    let probe = |r: &mitcad_model::sketch::regions::Region| -> Option<V2> {
        // Just inside the outline (counter-clockwise: the inside is left).
        let (a, b) = (r.outline.first()?, r.outline.get(1)?);
        let d = unit2([b[0] - a[0], b[1] - a[1]])?;
        let size = bounds(r.outline.iter().copied())
            .map_or(1.0, |(lo, hi)| (hi[0] - lo[0]).max(hi[1] - lo[1]));
        let eps = 1e-6 * size.max(1e-3);
        let m = [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0];
        Some([m[0] - d[1] * eps, m[1] + d[0] * eps])
    };
    (0..regions.len())
        .filter(|&i| {
            let Some(p) = probe(&regions[i]) else {
                return false;
            };
            let depth = regions
                .iter()
                .enumerate()
                .filter(|(j, o)| *j != i && inside(&o.outline, p))
                .count();
            depth % 2 == 0
        })
        .collect()
}

/// Where a member's section comes from, worked out.
enum Section {
    Library(Library),
    /// A sketch's profiles: the file's sketch (mirrored when the member
    /// is), the regions of it, and what members are named after.
    Sketch {
        id: u64,
        sketch: Box<Sketch>,
        profiles: ProfileSel,
        name: String,
    },
}

impl<K: Kernel> Importer<'_, '_, K> {
    /// A frame member: one Pipe or Sweep per line (mitcad#2157).
    pub(super) fn frame_member(&mut self, f: &Feature, component: ComponentUid) -> MapResult {
        let m: FrameMember = f.typed()?;
        if self
            .design
            .feature(m.path_sketch)
            .is_some_and(|s| s.component != f.component)
        {
            return Err(NotMapped::Failed(
                "its lines' sketch is in another component".to_owned(),
            ));
        }
        let lines = self.member_lines(&m)?;
        let mut notes = Vec::new();
        // The turn about the line.
        let turn = match &m.angle {
            Some(_) => {
                let (_, expr) = self.input(f, "/angle");
                let expr = expr.ok_or("its turn has no value")?;
                if mitcad_solidcraft::expr::literal(&expr, std::f64::consts::PI / 180.0).is_none() {
                    notes.push(
                        "the members' turn is fixed at its value now (it follows no parameter)"
                            .to_owned(),
                    );
                }
                self.eval(&expr, Kind::Angle)?
            }
            None => 0.0,
        };
        let section = match &m.profile {
            MemberProfile::Library { profile } => {
                let kind = profile
                    .get("kind")
                    .and_then(Value::as_str)
                    .ok_or("a profile without its kind")?
                    .to_owned();
                let mut dims = BTreeMap::new();
                for (name, v) in profile.as_object().into_iter().flatten() {
                    if matches!(name.as_str(), "kind" | "size") {
                        continue;
                    }
                    let text = inputs::expr_text(v)
                        .ok_or_else(|| format!("profile `{name}` is not an expression"))?;
                    let value = if name == "flange_slope" {
                        self.eval(&text, Kind::Angle)?.to_degrees()
                    } else {
                        self.eval(&text, Kind::Length)?
                    };
                    dims.insert(name.clone(), value);
                }
                if dims.is_empty() {
                    return Err(NotMapped::Failed(
                        "a standard size saved without its dimensions".to_owned(),
                    ));
                }
                let mut lib = Library::new(&kind, &dims)?;
                if let Some(size) = profile
                    .get("size")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                {
                    lib.description = size.to_owned();
                }
                Section::Library(lib)
            }
            MemberProfile::Sketch { sketch, profiles } => {
                let file = self
                    .design
                    .feature(*sketch)
                    .ok_or("its profile sketch is not in the file")?;
                let s: mitcad_solidcraft::kinds::SketchFeature = file.typed()?;
                Section::Sketch {
                    id: *sketch,
                    sketch: Box::new(if m.mirror {
                        mirrored_sketch(&s.sketch)
                    } else {
                        s.sketch
                    }),
                    profiles: profiles.clone(),
                    name: file.name.clone(),
                }
            }
        };
        // The profile point on the line, in the (mirrored) profile's
        // coordinates.
        let at = self.member_at(&m, &section, &mut notes)?;
        let base = m
            .body_name
            .as_deref()
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .map_or_else(
                || match &section {
                    Section::Library(l) => l.description.clone(),
                    Section::Sketch { name, .. } => name.clone(),
                },
                str::to_owned,
            );
        let ids: Vec<String> = lines.iter().map(|l| l.id.clone()).collect();
        let bodies = &self.bodies;
        let names = member_names(&|n| bodies.contains_key(n), &ids, &m.names, &base);
        let pipe = matches!(&section, Section::Library(l) if l.round())
            && at[0].abs() < 1e-9
            && at[1].abs() < 1e-9;
        if !pipe && matches!(section, Section::Library(_)) {
            notes.push(
                "the section is drawn at the profile's dimensions now (they follow no parameter)"
                    .to_owned(),
            );
        }
        let mut slots: HashMap<&'static str, String> = HashMap::new();
        let mut failed = Vec::new();
        let mut made = 0;
        for (k, (line, body)) in lines.iter().zip(&names).enumerate() {
            let name = if k == 0 {
                f.name.clone()
            } else {
                format!("{} ({})", f.name, k + 1)
            };
            let depth = self.doc.undo_depth();
            let kept = slots.clone();
            let result = if pipe {
                self.member_pipe(f, &section, line, &name, body, &mut slots, component)
            } else {
                self.member_sweep(&m, &section, line, at, turn, &name, body, component)
            };
            self.member_body = None;
            if let Err(e) = result {
                // What the member added (its plane, its section, their
                // visibility) is taken back.
                self.params.forget_pending();
                while self.doc.undo_depth() > depth {
                    if self.doc.undo().is_none() {
                        break;
                    }
                }
                slots = kept;
                failed.push(format!(
                    "the member on line {} did not come in: {e}",
                    line.id
                ));
            } else {
                made += 1;
            }
        }
        if made == 0 {
            return Err(NotMapped::Failed(failed.join("; ")));
        }
        notes.extend(failed);
        Ok(Mapped::partial(notes))
    }

    /// The member lines of the sketch: those asked for, or every straight
    /// line drawn in it (not construction, not linked, with a length).
    fn member_lines(&self, m: &FrameMember) -> Result<Vec<Line>, String> {
        let ps = self
            .sketches
            .get(&m.path_sketch)
            .ok_or("the lines' sketch did not come in")?;
        let planar = |i: usize| -> Option<([f64; 3], [f64; 3])> {
            let CurveKind::Line { a, b } = ps.sketch.curves.get(i)?.kind else {
                return None;
            };
            let (pa, pb) = (ps.sketch.points.get(a)?.pos, ps.sketch.points.get(b)?.pos);
            Some((ps.frame.point(pa), ps.frame.point(pb)))
        };
        let planar_path = |id: &str| -> Result<Value, String> {
            Ok(json!({"sketch": ps.uid.to_string(), "curves": [ps.curve(id)?]}))
        };
        let mut out = Vec::new();
        if m.lines.is_empty() {
            for (i, c) in ps.sketch.curves.iter().enumerate() {
                if !c.construction
                    && !c.centerline
                    && c.link.is_none()
                    && let Some((a, b)) = planar(i)
                    && norm(sub(b, a)) > MIN_LENGTH
                {
                    out.push(Line {
                        id: c.id.clone(),
                        a,
                        b,
                        path: planar_path(&c.id)?,
                    });
                }
            }
            if let Some(sp) = &ps.spatial {
                for w in sp.wires.iter().filter(|w| !w.linked) {
                    if let Some((a, b)) = w.straight()
                        && norm(sub(b, a)) > MIN_LENGTH
                    {
                        out.push(Line {
                            id: w.id.clone(),
                            a,
                            b,
                            path: sp.path(std::slice::from_ref(&w.id))?,
                        });
                    }
                }
            }
            if out.is_empty() {
                return Err("its sketch has no lines for members".to_owned());
            }
            return Ok(out);
        }
        for id in &m.lines {
            let (a, b, path) = match ps
                .spatial
                .as_ref()
                .and_then(|sp| sp.wire(id).map(|w| (sp, w)))
            {
                Some((sp, w)) => {
                    let (a, b) = w.straight().ok_or_else(|| {
                        format!("`{id}` is not a straight line (members run along lines)")
                    })?;
                    (a, b, sp.path(std::slice::from_ref(id))?)
                }
                None => {
                    let i = ps
                        .sketch
                        .curve_index(id)
                        .ok_or_else(|| format!("its sketch has no line `{id}`"))?;
                    let (a, b) = planar(i).ok_or_else(|| {
                        format!("`{id}` is not a straight line (members run along lines)")
                    })?;
                    (a, b, planar_path(id)?)
                }
            };
            if norm(sub(b, a)) <= MIN_LENGTH {
                return Err(format!("line `{id}` has no length"));
            }
            out.push(Line {
                id: id.clone(),
                a,
                b,
                path,
            });
        }
        Ok(out)
    }

    /// The profile point the file puts on the line: `locate` from the
    /// profile's own origin, else its anchor.
    fn member_at(
        &self,
        m: &FrameMember,
        section: &Section,
        notes: &mut Vec<String>,
    ) -> Result<V2, String> {
        let flip = |p: V2| if m.mirror { [-p[0], p[1]] } else { p };
        if let Some([x, y]) = m.locate {
            if !(x.is_finite() && y.is_finite()) {
                return Err("a profile point that is not a number".to_owned());
            }
            let origin = match section {
                Section::Library(l) => l.origin,
                Section::Sketch { .. } => [0.0; 2],
            };
            return Ok(flip([origin[0] + x, origin[1] + y]));
        }
        let anchor = m.anchor.as_deref().unwrap_or("origin");
        if anchor == "origin" {
            return Ok([0.0; 2]);
        }
        let (lo, hi, c) = match section {
            Section::Library(l) => {
                let loops: Vec<Vec<Seg>> = l
                    .loops
                    .iter()
                    .map(|lp| {
                        lp.iter()
                            .map(|s| if m.mirror { s.mirrored() } else { *s })
                            .collect()
                    })
                    .collect();
                let (lo, hi) = bounds(loops[0].iter().flat_map(Seg::extremes))
                    .ok_or("a profile without an outline")?;
                (lo, hi, centroid(&loops))
            }
            Section::Sketch { id, .. } => {
                // The profile sketch's material, as MitCad samples it.
                let made = self
                    .sketches
                    .get(id)
                    .ok_or("its profile sketch did not come in")?;
                let output = self
                    .doc
                    .sketch_output(made.uid)
                    .ok_or("its profile sketch does not evaluate")?;
                let regions = &output.region_info;
                let picked = material(regions);
                let (mut lo, mut hi) = bounds(
                    picked
                        .iter()
                        .flat_map(|&i| regions[i].outline.iter().copied()),
                )
                .ok_or("its profile sketch has no closed profile")?;
                let area: f64 = picked.iter().map(|&i| regions[i].area).sum();
                let mut c = [0.0; 2];
                for &i in &picked {
                    c[0] += regions[i].centroid[0] * regions[i].area / area;
                    c[1] += regions[i].centroid[1] * regions[i].area / area;
                }
                if m.mirror {
                    (lo[0], hi[0]) = (-hi[0], -lo[0]);
                    c[0] = -c[0];
                }
                notes.push(
                    "the profile point is placed on the profile as MitCad samples its curves"
                        .to_owned(),
                );
                (lo, hi, c)
            }
        };
        anchor_point(anchor, lo, hi, c)
    }

    /// A round member along its centre line: a pipe of the file's diameter
    /// and wall, their parameters shared by the feature's pipes.
    #[allow(clippy::too_many_arguments)]
    fn member_pipe(
        &mut self,
        f: &Feature,
        section: &Section,
        line: &Line,
        name: &str,
        body: &str,
        slots: &mut HashMap<&'static str, String>,
        component: ComponentUid,
    ) -> Result<(), String> {
        let Section::Library(l) = section else {
            return Err("a pipe of a sketch's profile".to_owned());
        };
        let mut slot = |im: &mut Self, key: &'static str| -> Result<String, String> {
            if let Some(s) = slots.get(key) {
                return Ok(s.clone());
            }
            let s = im.slot(f, &format!("/profile/profile/{key}"))?;
            // `d4 = 42.4 mm`: the later pipes use d4.
            let name = s.split_once(" = ").map_or(s.clone(), |(n, _)| n.to_owned());
            slots.insert(key, name);
            Ok(s)
        };
        let mut def = json!({"type": "pipe", "path": line.path, "section": "circular",
            "size": slot(self, "diameter")?, "operation": "new_body", "participants": []});
        if l.kind == "round_tube" {
            def["thickness"] = json!(slot(self, "wall")?);
        }
        self.member_body = Some(body.to_owned());
        self.add(&def, Some(name), component)?;
        Ok(())
    }

    /// Any other member: a plane square to the line at its start, the
    /// section sketched on it in the member's frame, swept along the line.
    #[allow(clippy::too_many_arguments)]
    fn member_sweep(
        &mut self,
        m: &FrameMember,
        section: &Section,
        line: &Line,
        at: V2,
        turn: f64,
        name: &str,
        body: &str,
        component: ComponentUid,
    ) -> Result<(), String> {
        let distance = self.params.unnamed(self.doc, "0");
        // A plane's path of one planar curve is that curve
        // (`{"sketch", "curve"}`): its several-curve form needs two.
        let plane_path = match (line.path.get("sketch"), line.path["curves"].as_array()) {
            (Some(sketch), Some(curves)) if curves.len() == 1 => {
                json!({"sketch": sketch, "curve": curves[0]})
            }
            _ => line.path.clone(),
        };
        let plane = self.add_helper(
            &json!({"type": "construction_plane", "definition":
                {"type": "along_path", "path": plane_path, "distance": distance, "physical": false}}),
            component,
        )?;
        let Some(Datum::Plane(p)) = self.doc.datum(plane) else {
            return Err("the plane along the line was not found".to_owned());
        };
        let on = Frame {
            o: p.origin,
            x: p.x_axis,
            y: p.y_axis,
        };
        // The member's frame: the profile point on the line's start.
        let (x, y) = member_axes(sub(line.b, line.a), turn)
            .ok_or_else(|| format!("line `{}` has no length", line.id))?;
        let member = Frame {
            o: sub(line.a, add(scale(x, at[0]), scale(y, at[1]))),
            x,
            y,
        };
        let sketch_name = format!("{name} Section");
        let (mut def, all) = match section {
            Section::Library(l) => {
                let loops: Vec<Vec<Seg>> = l
                    .loops
                    .iter()
                    .map(|lp| {
                        lp.iter()
                            .map(|s| if m.mirror { s.mirrored() } else { *s })
                            .collect()
                    })
                    .collect();
                (
                    json!({"type": "sketch", "plane": plane.to_string(),
                        "entities": profile_entities(&loops), "constraints": [], "dimensions": []}),
                    true,
                )
            }
            Section::Sketch {
                sketch, profiles, ..
            } => {
                let t = self.translate_sketch(sketch).map_err(|e| match e {
                    NotMapped::Failed(e) | NotMapped::Unmapped(e) => e,
                })?;
                (
                    json!({"type": "sketch", "plane": plane.to_string(),
                        "entities": t.entities, "constraints": t.constraints,
                        "dimensions": t.dimensions}),
                    matches!(profiles, ProfileSel::All),
                )
            }
        };
        if let Some((o, xa, ya)) = on.relative(&member) {
            def["frame"] = json!({"origin": o, "x_axis": xa, "y_axis": ya});
        }
        let sketch = match self.add(&def, Some(&sketch_name), component) {
            Ok(uid) => uid,
            Err(e) => {
                // Without its relations, its geometry where the file has it.
                self.params.forget_pending();
                let mut bare = def.clone();
                bare["constraints"] = json!([]);
                bare["dimensions"] = json!([]);
                self.add(&bare, Some(&sketch_name), component)
                    .map_err(|e2| format!("its section: {e}; without its relations: {e2}"))?
            }
        };
        let _ = self.doc.set_feature_visible(sketch, false);
        let output = self
            .doc
            .sketch_output(sketch)
            .ok_or("its section does not evaluate")?;
        let regions = &output.region_info;
        let reference = |key: String| json!({"sketch": sketch.to_string(), "region": key});
        let profiles: Vec<Value> = if all {
            material(regions)
                .into_iter()
                .map(|i| reference(regions[i].profile.key.to_string()))
                .collect()
        } else {
            let Section::Sketch { id, profiles, .. } = section else {
                return Err("a selection of a built-in profile".to_owned());
            };
            // The same regions of the copy: its curves have the same ids.
            let keys: Vec<String> = regions.iter().map(|r| r.profile.key.to_string()).collect();
            self.profiles(*id, profiles)?
                .into_iter()
                .map(|p| {
                    let key = p["region"].as_str().unwrap_or_default().to_owned();
                    if keys.contains(&key) {
                        Ok(reference(key))
                    } else {
                        Err("its profile is not found in the section drawn on the line".to_owned())
                    }
                })
                .collect::<Result<_, String>>()?
        };
        if profiles.is_empty() {
            return Err("its section has no closed profile".to_owned());
        }
        self.member_body = Some(body.to_owned());
        self.add(
            &json!({"type": "sweep", "profiles": profiles, "path": line.path,
                "orientation": "perpendicular", "operation": "new_body", "participants": []}),
            Some(name),
            component,
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area(loops: &[Vec<Seg>]) -> f64 {
        loops
            .iter()
            .enumerate()
            .map(|(i, l)| {
                let a = loop_moments(l)[0].abs();
                if i == 0 { a } else { -a }
            })
            .sum()
    }

    /// A rectangular tube's section: 40 x 30 with a 3 wall, the default
    /// radii (6 outside, 3 inside): the outline less the hole.
    #[test]
    fn library_sections_have_the_files_area() {
        let dims: BTreeMap<String, f64> = [("width", 40.0), ("height", 30.0), ("wall", 3.0)]
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v))
            .collect();
        let l = Library::new("rect_tube", &dims).unwrap();
        let corner = |w: f64, h: f64, r: f64| w * h - (4.0 - std::f64::consts::PI) * r * r;
        let want = corner(40.0, 30.0, 6.0) - corner(34.0, 24.0, 3.0);
        assert!(
            (area(&l.loops) - want).abs() < 1e-9,
            "{} {want}",
            area(&l.loops)
        );
        assert_eq!(l.description, "Rect Tube 40x30x3");
        let c = centroid(&l.loops);
        assert!(c[0].abs() < 1e-9 && c[1].abs() < 1e-9);
        // An L angle 50 x 30 x 5 without radii: its centroid off the box's
        // centre.
        let dims: BTreeMap<String, f64> = [
            ("width", 50.0),
            ("height", 30.0),
            ("thickness", 5.0),
            ("root_radius", 0.0),
            ("toe_radius", 0.0),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v))
        .collect();
        let l = Library::new("angle", &dims).unwrap();
        assert!((area(&l.loops) - (50.0 * 5.0 + 25.0 * 5.0)).abs() < 1e-9);
        let c = centroid(&l.loops);
        // Legs 250 at (25, 2.5) and 125 at (2.5, 17.5), heel at (-25, -15).
        let want = [
            (250.0 * 25.0 + 125.0 * 2.5) / 375.0 - 25.0,
            (250.0 * 2.5 + 125.0 * 17.5) / 375.0 - 15.0,
        ];
        assert!(
            (c[0] - want[0]).abs() < 1e-9 && (c[1] - want[1]).abs() < 1e-9,
            "{c:?}"
        );
    }

    /// A member's axes: a level line's profile y is up, a post's x is +X,
    /// and a turn turns them about the line.
    #[test]
    fn member_axes_are_the_files() {
        // x = y x line: (0, 0, 1) x (1, 0, 0) = (0, 1, 0), so x x y runs
        // along the line.
        let (x, y) = member_axes([10.0, 0.0, 0.0], 0.0).unwrap();
        assert!(norm(sub(y, [0.0, 0.0, 1.0])) < 1e-12 && norm(sub(x, [0.0, 1.0, 0.0])) < 1e-12);
        let (x, y) = member_axes([0.0, 0.0, 5.0], 0.0).unwrap();
        assert!(norm(sub(x, [1.0, 0.0, 0.0])) < 1e-12 && norm(sub(y, [0.0, 1.0, 0.0])) < 1e-12);
        let (x, _) = member_axes([10.0, 0.0, 0.0], std::f64::consts::FRAC_PI_2).unwrap();
        assert!(norm(sub(x, [0.0, 0.0, 1.0])) < 1e-12, "{x:?}");
    }

    #[test]
    fn members_are_named_as_the_file_names_them() {
        let pinned: BTreeMap<String, String> = [("l2".to_owned(), "Rail".to_owned())].into();
        let lines = ["l1".to_owned(), "l2".to_owned(), "l3".to_owned()];
        let names = member_names(&|n| n == "Tube", &lines, &pinned, "Tube");
        assert_eq!(names, ["Tube (2)", "Rail", "Tube (3)"]);
    }
}
