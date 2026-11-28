// SPDX-License-Identifier: MIT
//! The spun target of a mill-turn setup (mitcad#2629, GitHub
//! mitcad/Mitcad#690): the bodies turned about the spindle axis, so that
//! turning leaves every feature whatever its spindle angle, not only what
//! the section through the axis cuts. Live tools mill the rest from the
//! bodies themselves.
//!
//! The section ([`TurnPart`](super::TurnPart)'s profile) is exact but sees
//! one plane; the bodies' triangle mesh sees them all. At a Z, a triangle
//! reaches farthest from the axis at an end of its slice there (the
//! distance from the axis is convex) and comes nearest at the slice's
//! point nearest the axis. Each triangle is read at its corners' Z, at the
//! section's corners' Z and every [`STEP`] between: the farthest it
//! reaches between two of them lies under the line joining them (convex
//! again), so the line bounds it from outside; the nearest lies within
//! the mesh's deviation of the line, which the mesh's lines are moved by,
//! away from the axis outside and towards it in the bore.
//!
//! What the mesh holds within twice its deviation of the section is the
//! section's own: when nothing lies beyond that (an axisymmetric part, or
//! features within the section's outline), the section's profile is kept
//! as it is, arcs and all. Otherwise the outside is, at every Z, the
//! farthest of the section and the mesh's lines beyond it, and the bore
//! the nearest, within the bore's own Z; the section's arcs then become
//! short lines within a tenth of the deviation.

use std::f64::consts::TAU;

use super::PXZ;
use super::profile::{PV, pieces};

/// A triangle of a body's mesh in work coordinates: Z the spindle axis,
/// the origin on it.
pub type Triangle = [[f64; 3]; 3];

/// The spun profile: the outside and the bore as
/// [`TurnPart`](super::TurnPart) has them, and whether the mesh changed
/// the section's.
#[derive(Clone, Debug, PartialEq)]
pub struct Spun {
    pub outside: Vec<PV>,
    pub inside: Vec<PV>,
    pub changed: bool,
}

/// The longest Z between two readings of a triangle (mm).
pub const STEP: f64 = 0.25;

const ZE: f64 = 1e-7;

/// A line in the profile's plane, `[x, z]` (x a radius).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Seg {
    a: PXZ,
    b: PXZ,
}

/// The spun profile of the section's `outside` and `inside` (as
/// [`TurnPart`](super::TurnPart) has them) with the bodies' `triangles`,
/// meshed within `deviation` (mm).
pub fn spun(outside: &[PV], inside: &[PV], triangles: &[Triangle], deviation: f64) -> Spun {
    let unchanged = Spun {
        outside: outside.to_vec(),
        inside: inside.to_vec(),
        changed: false,
    };
    if outside.len() < 2 || triangles.is_empty() {
        return unchanged;
    }
    let deviation = if deviation.is_finite() && deviation > 0.0 {
        deviation
    } else {
        1e-3
    };
    let tol = 2.0 * deviation + 1e-6;
    let sag = deviation / 10.0;
    let out_segs = chords(outside, sag);
    let in_segs = chords(inside, sag);
    // The section's corners' Z, where its outline turns.
    let mut corners: Vec<f64> = out_segs
        .iter()
        .chain(in_segs.iter())
        .flat_map(|s| [s.a[1], s.b[1]])
        .collect();
    corners.sort_by(f64::total_cmp);
    corners.dedup_by(|a, b| (*a - *b).abs() < ZE);
    let in_range = inside_range(inside);
    let out_lookup = Lookup::new(out_segs);
    let in_lookup = Lookup::new(in_segs);
    let mut out_added: Vec<Seg> = Vec::new();
    let mut in_added: Vec<Seg> = Vec::new();
    for t in triangles {
        if t.iter().flatten().any(|v| !v.is_finite()) {
            continue;
        }
        let read = slices(t, &corners);
        let beyond = read
            .iter()
            .any(|s| s.max > out_lookup.at(s.z, true).unwrap_or(0.0).max(0.0) + tol);
        if beyond {
            out_added.extend(polyline(&read, |s| s.max + deviation));
        }
        if let Some((lo, hi)) = in_range {
            let into = read.iter().any(|s| {
                s.z >= lo - ZE
                    && s.z <= hi + ZE
                    && in_lookup.at(s.z, false).is_some_and(|m| s.min < m - tol)
            });
            if into {
                in_added.extend(
                    polyline(&read, |s| (s.min - deviation).max(0.0))
                        .into_iter()
                        .filter_map(|g| clip_z(g, lo, hi)),
                );
            }
        }
    }
    if out_added.is_empty() && in_added.is_empty() {
        return unchanged;
    }
    let new_outside = if out_added.is_empty() {
        outside.to_vec()
    } else {
        let mut segs = out_lookup.segs;
        segs.extend(out_added);
        let pts = upper(&segs, 0.0);
        let mut chain = vec![[outside[0].x, pts.first().map_or(outside[0].z, |p| p[1])]];
        chain.extend(pts);
        simplify(chain)
    };
    let new_inside = if in_added.is_empty() {
        inside.to_vec()
    } else {
        let mut segs = in_lookup.segs;
        segs.extend(in_added);
        let flipped: Vec<Seg> = segs.iter().map(|s| flip(*s)).collect();
        let pts: Vec<PXZ> = upper(&flipped, 0.0)
            .into_iter()
            .map(|p| [(-p[0]).max(0.0), p[1]])
            .collect();
        simplify(pts)
    };
    Spun {
        outside: new_outside,
        inside: new_inside,
        changed: true,
    }
}

/// The bore's Z range, `(lowest, highest)`.
fn inside_range(inside: &[PV]) -> Option<(f64, f64)> {
    if inside.len() < 2 {
        return None;
    }
    let lo = inside.iter().map(|v| v.z).fold(f64::INFINITY, f64::min);
    let hi = inside.iter().map(|v| v.z).fold(f64::NEG_INFINITY, f64::max);
    Some((lo, hi))
}

fn flip(s: Seg) -> Seg {
    Seg {
        a: [-s.a[0], s.a[1]],
        b: [-s.b[0], s.b[1]],
    }
}

/// A chain as lines: arcs as chords no farther than `sag` from them.
fn chords(chain: &[PV], sag: f64) -> Vec<Seg> {
    let mut out = Vec::new();
    for p in pieces(chain) {
        let Some(c) = p.center else {
            out.push(Seg { a: p.a, b: p.b });
            continue;
        };
        let r = (p.a[0] - c[0]).hypot(p.a[1] - c[1]);
        let a0 = (p.a[0] - c[0]).atan2(p.a[1] - c[1]);
        let a1 = (p.b[0] - c[0]).atan2(p.b[1] - c[1]);
        let mut d = a1 - a0;
        if p.ccw {
            while d <= 0.0 {
                d += TAU;
            }
        } else {
            while d >= 0.0 {
                d -= TAU;
            }
        }
        let step = if sag < r {
            2.0 * (1.0 - sag / r).acos()
        } else {
            d.abs()
        };
        let n = ((d.abs() / step.max(1e-6)).ceil() as usize).clamp(1, 4096);
        let point = |k: usize| {
            if k == 0 {
                p.a
            } else if k == n {
                p.b
            } else {
                let t = a0 + d * k as f64 / n as f64;
                [c[0] + r * t.sin(), c[1] + r * t.cos()]
            }
        };
        for k in 0..n {
            out.push(Seg {
                a: point(k),
                b: point(k + 1),
            });
        }
    }
    out
}

/// The section's lines found by Z: each bucket of an even split of their Z
/// span lists the lines reaching into it.
struct Lookup {
    segs: Vec<Seg>,
    lo: f64,
    width: f64,
    buckets: Vec<Vec<usize>>,
}

impl Lookup {
    fn new(segs: Vec<Seg>) -> Self {
        let lo = segs
            .iter()
            .map(|s| s.a[1].min(s.b[1]))
            .fold(f64::INFINITY, f64::min);
        let hi = segs
            .iter()
            .map(|s| s.a[1].max(s.b[1]))
            .fold(f64::NEG_INFINITY, f64::max);
        let n = segs.len().clamp(1, 4096);
        let width = if hi > lo { (hi - lo) / n as f64 } else { 1.0 };
        let mut buckets = vec![Vec::new(); if hi >= lo { n } else { 0 }];
        for (i, s) in segs.iter().enumerate() {
            let (a, b) = (s.a[1].min(s.b[1]), s.a[1].max(s.b[1]));
            let first = ((a - ZE - lo) / width).floor().max(0.0) as usize;
            let last = (((b + ZE - lo) / width).floor().max(0.0) as usize).min(n - 1);
            for bucket in buckets.iter_mut().take(last + 1).skip(first) {
                bucket.push(i);
            }
        }
        Lookup {
            segs,
            lo,
            width,
            buckets,
        }
    }

    /// [`at`] of the lines.
    fn at(&self, z: f64, max: bool) -> Option<f64> {
        if self.buckets.is_empty() {
            return None;
        }
        let k = ((z - self.lo) / self.width).floor();
        if !k.is_finite() || k < -1.0 || k > self.buckets.len() as f64 {
            return None;
        }
        let k = (k.max(0.0) as usize).min(self.buckets.len() - 1);
        let near: Vec<Seg> = self.buckets[k].iter().map(|&i| self.segs[i]).collect();
        at(&near, z, max)
    }
}

/// The farthest (`max`) or nearest radius the lines have at `z`; none
/// where none reaches it. A line across at one Z counts with its ends.
fn at(segs: &[Seg], z: f64, max: bool) -> Option<f64> {
    let mut best: Option<f64> = None;
    for s in segs {
        let (lo, hi) = (s.a[1].min(s.b[1]), s.a[1].max(s.b[1]));
        if z < lo - ZE || z > hi + ZE {
            continue;
        }
        let r = if hi - lo < ZE {
            if max {
                s.a[0].max(s.b[0])
            } else {
                s.a[0].min(s.b[0])
            }
        } else {
            let zc = z.clamp(lo, hi);
            let t = (zc - s.a[1]) / (s.b[1] - s.a[1]);
            s.a[0] + t * (s.b[0] - s.a[0])
        };
        best = Some(match best {
            None => r,
            Some(b) if max => b.max(r),
            Some(b) => b.min(r),
        });
    }
    best
}

/// A triangle's slice at one Z: the farthest and nearest it is from the
/// axis there.
#[derive(Clone, Copy, Debug)]
struct Slice {
    z: f64,
    max: f64,
    min: f64,
}

fn radius(p: [f64; 3]) -> f64 {
    p[0].hypot(p[1])
}

/// The distance from the axis to a segment in the XY plane.
fn to_segment(a: [f64; 3], b: [f64; 3]) -> f64 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let l = dx * dx + dy * dy;
    let t = if l < 1e-24 {
        0.0
    } else {
        (-(a[0] * dx + a[1] * dy) / l).clamp(0.0, 1.0)
    };
    (a[0] + t * dx).hypot(a[1] + t * dy)
}

/// A triangle read at its corners' Z, the section's `corners` within its
/// span (just above and below each) and every [`STEP`] between.
fn slices(t: &Triangle, corners: &[f64]) -> Vec<Slice> {
    let lo = t.iter().map(|p| p[2]).fold(f64::INFINITY, f64::min);
    let hi = t.iter().map(|p| p[2]).fold(f64::NEG_INFINITY, f64::max);
    if hi - lo < ZE {
        // Across the axis at one Z: the whole triangle.
        let max = t.iter().map(|p| radius(*p)).fold(0.0, f64::max);
        let min = if holds_axis(t) {
            0.0
        } else {
            (0..3)
                .map(|i| to_segment(t[i], t[(i + 1) % 3]))
                .fold(f64::INFINITY, f64::min)
        };
        return vec![Slice {
            z: (lo + hi) / 2.0,
            max,
            min,
        }];
    }
    let mut zs: Vec<f64> = t.iter().map(|p| p[2]).collect();
    let n = ((hi - lo) / STEP).ceil() as usize;
    zs.extend((1..n.min(100_000)).map(|k| lo + (hi - lo) * k as f64 / n as f64));
    let first = corners.partition_point(|&z| z < lo);
    for &z in corners[first..].iter().take_while(|&&z| z <= hi) {
        zs.extend([z - 1e-6, z, z + 1e-6]);
    }
    zs.retain(|z| *z >= lo && *z <= hi);
    zs.sort_by(f64::total_cmp);
    zs.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
    zs.into_iter()
        .filter_map(|z| {
            let mut pts: Vec<[f64; 3]> = Vec::new();
            for i in 0..3 {
                let (p, q) = (t[i], t[(i + 1) % 3]);
                if (p[2] - q[2]).abs() < ZE {
                    if (p[2] - z).abs() < ZE {
                        pts.extend([p, q]);
                    }
                    continue;
                }
                let u = (z - p[2]) / (q[2] - p[2]);
                if (-1e-12..=1.0 + 1e-12).contains(&u) {
                    let u = u.clamp(0.0, 1.0);
                    pts.push(std::array::from_fn(|k| p[k] + u * (q[k] - p[k])));
                }
            }
            if pts.is_empty() {
                return None;
            }
            let max = pts.iter().map(|p| radius(*p)).fold(0.0, f64::max);
            let mut min = pts.iter().map(|p| radius(*p)).fold(f64::INFINITY, f64::min);
            for (i, &p) in pts.iter().enumerate() {
                for &q in &pts[i + 1..] {
                    min = min.min(to_segment(p, q));
                }
            }
            Some(Slice { z, max, min })
        })
        .collect()
}

/// Whether a triangle at one Z holds the axis.
fn holds_axis(t: &Triangle) -> bool {
    let cross = |a: [f64; 3], b: [f64; 3]| a[0] * b[1] - a[1] * b[0];
    let s: Vec<f64> = (0..3)
        .map(|i| {
            let (p, q) = (t[i], t[(i + 1) % 3]);
            cross([q[0] - p[0], q[1] - p[1], 0.0], [-p[0], -p[1], 0.0])
        })
        .collect();
    s.iter().all(|v| *v >= -1e-12) || s.iter().all(|v| *v <= 1e-12)
}

/// The lines joining a triangle's readings, each at the radius `r` gives.
fn polyline(read: &[Slice], r: impl Fn(&Slice) -> f64) -> Vec<Seg> {
    if let [one] = read {
        // Across at one Z: from its nearest to its farthest.
        return vec![Seg {
            a: [
                r(&Slice {
                    max: one.min,
                    ..*one
                }),
                one.z,
            ],
            b: [r(one), one.z],
        }];
    }
    read.windows(2)
        .map(|w| Seg {
            a: [r(&w[0]), w[0].z],
            b: [r(&w[1]), w[1].z],
        })
        .collect()
}

/// The part of a line within `[lo, hi]` along Z.
fn clip_z(s: Seg, lo: f64, hi: f64) -> Option<Seg> {
    let (zl, zh) = (s.a[1].min(s.b[1]), s.a[1].max(s.b[1]));
    if zh < lo - ZE || zl > hi + ZE {
        return None;
    }
    if zh - zl < ZE {
        return Some(s);
    }
    let point = |z: f64| {
        let t = (z - s.a[1]) / (s.b[1] - s.a[1]);
        [s.a[0] + t * (s.b[0] - s.a[0]), z]
    };
    let a = if s.a[1] < lo || s.a[1] > hi {
        point(s.a[1].clamp(lo, hi))
    } else {
        s.a
    };
    let b = if s.b[1] < lo || s.b[1] > hi {
        point(s.b[1].clamp(lo, hi))
    } else {
        s.b
    };
    Some(Seg { a, b })
}

/// The upper envelope of the lines, from the highest Z down: at every Z
/// the largest radius any line has there, going straight across where it
/// jumps, `floor` where no line is.
fn upper(segs: &[Seg], floor: f64) -> Vec<PXZ> {
    let mut zs: Vec<f64> = segs.iter().flat_map(|s| [s.a[1], s.b[1]]).collect();
    zs.sort_by(|a, b| b.total_cmp(a));
    zs.dedup_by(|a, b| (*a - *b).abs() < ZE);
    if zs.is_empty() {
        return Vec::new();
    }
    let index = |z: f64| zs.partition_point(|&v| v > z + ZE).min(zs.len() - 1);
    // (z high, z low, r there, r here): a line along Z.
    struct Line {
        hi: f64,
        lo: f64,
        r_hi: f64,
        slope: f64,
    }
    impl Line {
        fn r(&self, z: f64) -> f64 {
            self.r_hi + self.slope * (z - self.hi)
        }
    }
    let mut walls = vec![f64::NEG_INFINITY; zs.len()];
    let mut lines: Vec<Line> = Vec::new();
    for s in segs {
        let (h, l) = if s.a[1] >= s.b[1] {
            (s.a, s.b)
        } else {
            (s.b, s.a)
        };
        if h[1] - l[1] < ZE {
            let i = index(h[1]);
            walls[i] = walls[i].max(h[0].max(l[0]));
        } else {
            lines.push(Line {
                hi: h[1],
                lo: l[1],
                r_hi: h[0],
                slope: (h[0] - l[0]) / (h[1] - l[1]),
            });
        }
    }
    lines.sort_by(|a, b| b.hi.total_cmp(&a.hi));
    let mut out: Vec<PXZ> = Vec::new();
    let mut active: Vec<usize> = Vec::new();
    let mut next = 0;
    let mut above: Option<f64> = None;
    for k in 0..zs.len() {
        let z = zs[k];
        active.retain(|&i| lines[i].lo < z - ZE);
        while next < lines.len() && lines[next].hi >= z - ZE {
            if lines[next].lo < z - ZE {
                active.push(next);
            }
            next += 1;
        }
        let last = k + 1 == zs.len();
        // The line highest just below `z` (ties: the one falling least).
        let top = active.iter().copied().max_by(|&i, &j| {
            let (a, b) = (&lines[i], &lines[j]);
            a.r(z).total_cmp(&b.r(z)).then(b.slope.total_cmp(&a.slope))
        });
        let below = if last {
            None
        } else {
            Some(top.map_or(floor, |i| lines[i].r(z)))
        };
        let wall = walls[k];
        let mut peak = wall;
        for v in [above, below].into_iter().flatten() {
            peak = peak.max(v);
        }
        if let Some(v) = above {
            out.push([v, z]);
        }
        out.push([peak, z]);
        if let Some(v) = below {
            out.push([v, z]);
        }
        if last {
            break;
        }
        let lo = zs[k + 1];
        let Some(mut cur) = top else {
            above = Some(floor);
            continue;
        };
        let mut zc = z;
        loop {
            // Going down, the line that overtakes first.
            let mut best: Option<(f64, usize)> = None;
            for &j in &active {
                let (c, o) = (&lines[cur], &lines[j]);
                let dm = o.slope - c.slope;
                if dm > -1e-12 {
                    continue;
                }
                let d0 = o.r(zc) - c.r(zc);
                let zx = zc + (-d0) / dm;
                if zx < zc - ZE && zx > lo + ZE && best.is_none_or(|(b, _)| zx > b) {
                    best = Some((zx, j));
                }
            }
            match best {
                Some((zx, j)) => {
                    out.push([lines[cur].r(zx), zx]);
                    cur = j;
                    zc = zx;
                }
                None => break,
            }
        }
        above = Some(lines[cur].r(lo));
    }
    out
}

/// Points as a chain of lines, repeated points and points in the middle
/// of a straight run left out.
fn simplify(points: Vec<PXZ>) -> Vec<PV> {
    let mut out: Vec<PXZ> = Vec::with_capacity(points.len());
    for p in points {
        if out
            .last()
            .is_some_and(|q| (q[0] - p[0]).abs() < 1e-9 && (q[1] - p[1]).abs() < 1e-9)
        {
            continue;
        }
        if out.len() >= 2 {
            let (a, b) = (out[out.len() - 2], out[out.len() - 1]);
            let (u, v) = ([b[0] - a[0], b[1] - a[1]], [p[0] - b[0], p[1] - b[1]]);
            let cross = u[0] * v[1] - u[1] * v[0];
            let dot = u[0] * v[0] + u[1] * v[1];
            let length = u[0].hypot(u[1]) * v[0].hypot(v[1]);
            if cross.abs() <= 1e-9 * length.max(1e-12) && dot > 0.0 {
                let n = out.len();
                out[n - 1] = p;
                continue;
            }
        }
        out.push(p);
    }
    out.into_iter().map(|p| PV::new(p[0] + 0.0, p[1])).collect()
}

#[cfg(test)]
#[path = "spun_tests.rs"]
mod spun_tests;
