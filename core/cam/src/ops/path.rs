// SPDX-License-Identifier: MIT
//! Runs of lines and arcs walked by length: the points, tangents and
//! pieces the entries, tabs, leads, adaptive clearing and the rest
//! clean-up cut along, and the distances the gouge checks measure.

use std::f64::consts::TAU;

use cavalier_contours::core::math::Vector2;
use cavalier_contours::polyline::{PlineSource, Polyline};
use cavalier_contours::shape_algorithms::Shape;

use crate::geom::{Contour, Region, Segment};
use crate::toolpath::{PathBuilder, arc_sweep};

pub(crate) fn dist2(a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

fn lerp(a: [f64; 2], b: [f64; 2], t: f64) -> [f64; 2] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
}

/// The length of a segment of an outline (an arc of at most a half
/// circle, or a line).
pub(crate) fn seg_length(s: &Segment) -> f64 {
    match s.arc {
        None => dist2(s.start, s.end),
        Some((c, ccw)) => dist2(s.start, c) * arc_sweep(s.start, s.end, c, ccw).abs(),
    }
}

fn reversed_seg(s: &Segment) -> Segment {
    Segment {
        start: s.end,
        end: s.start,
        arc: s.arc.map(|(c, ccw)| (c, !ccw)),
    }
}

/// The point `t` along a segment of length `len`.
fn seg_point(s: &Segment, len: f64, t: f64) -> [f64; 2] {
    if t <= 0.0 {
        return s.start;
    }
    if t >= len {
        return s.end;
    }
    match s.arc {
        None => lerp(s.start, s.end, t / len),
        Some((c, ccw)) => {
            let r = dist2(s.start, c);
            if r <= 0.0 {
                return s.start;
            }
            let a0 = (s.start[1] - c[1]).atan2(s.start[0] - c[0]);
            let a = if ccw { a0 + t / r } else { a0 - t / r };
            [c[0] + r * a.cos(), c[1] + r * a.sin()]
        }
    }
}

/// The unit tangent `t` along a segment, in its direction.
fn seg_tangent(s: &Segment, len: f64, t: f64) -> [f64; 2] {
    match s.arc {
        None => {
            if len <= 0.0 {
                return [1.0, 0.0];
            }
            [(s.end[0] - s.start[0]) / len, (s.end[1] - s.start[1]) / len]
        }
        Some((c, ccw)) => {
            let p = seg_point(s, len, t);
            let r = dist2(p, c);
            if r <= 0.0 {
                return [1.0, 0.0];
            }
            let u = [(p[0] - c[0]) / r, (p[1] - c[1]) / r];
            if ccw { [-u[1], u[0]] } else { [u[1], -u[0]] }
        }
    }
}

/// The distance from `p` to a segment.
pub(crate) fn seg_distance(s: &Segment, p: [f64; 2]) -> f64 {
    match s.arc {
        None => {
            let d = [s.end[0] - s.start[0], s.end[1] - s.start[1]];
            let len2 = d[0] * d[0] + d[1] * d[1];
            let t = if len2 > 0.0 {
                (((p[0] - s.start[0]) * d[0] + (p[1] - s.start[1]) * d[1]) / len2).clamp(0.0, 1.0)
            } else {
                0.0
            };
            dist2(p, lerp(s.start, s.end, t))
        }
        Some((c, ccw)) => {
            let r = dist2(s.start, c);
            let dp = dist2(p, c);
            if dp <= 1e-12 {
                return r;
            }
            let total = arc_sweep(s.start, s.end, c, ccw).abs();
            let a0 = (s.start[1] - c[1]).atan2(s.start[0] - c[0]);
            let ap = (p[1] - c[1]).atan2(p[0] - c[0]);
            let along = if ccw { ap - a0 } else { a0 - ap }.rem_euclid(TAU);
            if along <= total + 1e-12 {
                (dp - r).abs()
            } else {
                dist2(p, s.start).min(dist2(p, s.end))
            }
        }
    }
}

/// The distance from `p` to the nearest of the outlines.
pub(crate) fn distance_to(contours: &[Contour], p: [f64; 2]) -> f64 {
    contours
        .iter()
        .flat_map(|c| c.segments())
        .map(|s| seg_distance(&s, p))
        .fold(f64::INFINITY, f64::min)
}

/// The distance from `p` to the nearest outline of a region.
pub(crate) fn region_distance(region: &Region, p: [f64; 2]) -> f64 {
    distance_to(std::slice::from_ref(&region.outer), p).min(distance_to(&region.islands, p))
}

/// Whether `p` is inside a region's outer outline and outside its islands.
pub(crate) fn inside_region(region: &Region, p: [f64; 2]) -> bool {
    region.outer.contains(p) && !region.islands.iter().any(|i| i.contains(p))
}

/// An area bounded by polylines: inside one of the outer ones and outside
/// every hole. Containment by winding numbers, without converting the
/// outlines again for every point.
pub(crate) struct Area {
    outers: Vec<Polyline<f64>>,
    holes: Vec<Polyline<f64>>,
}

impl Area {
    pub(crate) fn of_shape(s: &Shape<f64>) -> Area {
        Area {
            outers: s
                .ccw_plines
                .iter()
                .filter(|p| p.polyline.vertex_count() > 1)
                .map(|p| p.polyline.clone())
                .collect(),
            holes: s
                .cw_plines
                .iter()
                .filter(|p| p.polyline.vertex_count() > 1)
                .map(|p| p.polyline.clone())
                .collect(),
        }
    }

    pub(crate) fn of_region(r: &Region) -> Area {
        Area {
            outers: vec![r.outer.to_pline()],
            holes: r.islands.iter().map(|i| i.to_pline()).collect(),
        }
    }

    pub(crate) fn contains(&self, p: [f64; 2]) -> bool {
        let v = Vector2::new(p[0], p[1]);
        self.outers.iter().any(|o| o.winding_number(v) != 0)
            && !self.holes.iter().any(|h| h.winding_number(v) != 0)
    }

    /// The corners of the box round the outer outlines.
    pub(crate) fn bounds(&self) -> Option<([f64; 2], [f64; 2])> {
        let mut min = [f64::INFINITY; 2];
        let mut max = [f64::NEG_INFINITY; 2];
        for o in &self.outers {
            if let Some(e) = o.extents() {
                min = [min[0].min(e.min_x), min[1].min(e.min_y)];
                max = [max[0].max(e.max_x), max[1].max(e.max_y)];
            }
        }
        min[0].is_finite().then_some((min, max))
    }
}

/// A run of lines and arcs with the length of each.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Path {
    pub segs: Vec<Segment>,
    pub lens: Vec<f64>,
}

impl Path {
    /// The segments of an outline (closed ones round to the start again).
    pub(crate) fn of(c: &Contour) -> Path {
        let mut p = Path::default();
        for s in c.segments() {
            p.push(s, seg_length(&s));
        }
        p
    }

    /// Straight lines through the points.
    pub(crate) fn polyline(points: &[[f64; 2]]) -> Path {
        let mut p = Path::default();
        for w in points.windows(2) {
            p.line(w[0], w[1]);
        }
        p
    }

    pub(crate) fn line(&mut self, a: [f64; 2], b: [f64; 2]) {
        self.push(
            Segment {
                start: a,
                end: b,
                arc: None,
            },
            dist2(a, b),
        );
    }

    /// An arc from `a` to `b` about `c` (no more than a whole turn less a
    /// hair).
    pub(crate) fn arc(&mut self, a: [f64; 2], b: [f64; 2], c: [f64; 2], ccw: bool) {
        let s = Segment {
            start: a,
            end: b,
            arc: Some((c, ccw)),
        };
        self.push(s, seg_length(&s));
    }

    /// Adds a segment, leaving out ones too short to matter.
    pub(crate) fn push(&mut self, s: Segment, len: f64) {
        if len > 1e-9 {
            self.segs.push(s);
            self.lens.push(len);
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.segs.is_empty()
    }

    pub(crate) fn length(&self) -> f64 {
        self.lens.iter().sum()
    }

    pub(crate) fn start(&self) -> Option<[f64; 2]> {
        self.segs.first().map(|s| s.start)
    }

    pub(crate) fn end(&self) -> Option<[f64; 2]> {
        self.segs.last().map(|s| s.end)
    }

    /// The segment at `t` along the path and `t` within it.
    fn locate(&self, t: f64) -> Option<(usize, f64)> {
        let mut acc = 0.0;
        for (i, &l) in self.lens.iter().enumerate() {
            if t <= acc + l || i + 1 == self.lens.len() {
                return Some((i, (t - acc).clamp(0.0, l)));
            }
            acc += l;
        }
        None
    }

    pub(crate) fn point_at(&self, t: f64) -> Option<[f64; 2]> {
        self.locate(t)
            .map(|(i, u)| seg_point(&self.segs[i], self.lens[i], u))
    }

    /// The unit tangent at `t` along the path, in its direction.
    pub(crate) fn tangent_at(&self, t: f64) -> [f64; 2] {
        match self.locate(t) {
            Some((i, u)) => seg_tangent(&self.segs[i], self.lens[i], u),
            None => [1.0, 0.0],
        }
    }

    /// The piece from `t0` to `t1` along the path.
    pub(crate) fn slice(&self, t0: f64, t1: f64) -> Path {
        let mut out = Path::default();
        let mut acc = 0.0;
        for (s, &l) in self.segs.iter().zip(&self.lens) {
            let a = (t0 - acc).max(0.0);
            let b = (t1 - acc).min(l);
            if b - a > 1e-9 {
                if a <= 0.0 && b >= l {
                    out.push(*s, l);
                } else {
                    let piece = Segment {
                        start: seg_point(s, l, a),
                        end: seg_point(s, l, b),
                        arc: s.arc,
                    };
                    out.push(piece, b - a);
                }
            }
            acc += l;
            if acc >= t1 {
                break;
            }
        }
        out
    }

    pub(crate) fn reversed(&self) -> Path {
        Path {
            segs: self.segs.iter().rev().map(reversed_seg).collect(),
            lens: self.lens.iter().rev().copied().collect(),
        }
    }

    /// This path followed by `other`.
    pub(crate) fn joined(&self, other: &Path) -> Path {
        let mut p = self.clone();
        p.segs.extend_from_slice(&other.segs);
        p.lens.extend_from_slice(&other.lens);
        p
    }

    /// Points along the path no farther apart than `step`, its start and
    /// end included.
    pub(crate) fn samples(&self, step: f64) -> Vec<[f64; 2]> {
        let total = self.length();
        let Some(start) = self.start() else {
            return Vec::new();
        };
        let n = ((total / step.max(1e-6)).ceil() as usize).clamp(1, 100_000);
        let mut out = vec![start];
        for i in 1..=n {
            if let Some(p) = self.point_at(total * i as f64 / n as f64) {
                out.push(p);
            }
        }
        out
    }

    /// Cuts along the path from its start (where the tool is), Z going
    /// from `z0` to `z1` in proportion to the length. An arc too short to
    /// write as one (its end the same as its start to the program's
    /// digits) is cut as a line.
    pub(crate) fn cut(&self, b: &mut PathBuilder, z0: f64, z1: f64, feed: f64) {
        let total = self.length();
        let mut acc = 0.0;
        let n = self.segs.len();
        for (i, (s, &l)) in self.segs.iter().zip(&self.lens).enumerate() {
            acc += l;
            let z = if i + 1 == n || total <= 0.0 {
                z1
            } else {
                z0 + (z1 - z0) * acc / total
            };
            let to = [s.end[0], s.end[1], z];
            match s.arc {
                Some((c, ccw)) if dist2(s.start, s.end) >= 0.01 => b.arc(to, c, ccw, feed),
                _ => b.line(to, feed),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_slice_of_a_circle_keeps_its_arcs_and_length() {
        let c = Contour::circle([0.0, 0.0], 10.0);
        let p = Path::of(&c);
        assert!((p.length() - TAU * 10.0).abs() < 1e-9);
        let quarter = p.slice(0.0, TAU * 10.0 / 4.0);
        assert_eq!(quarter.segs.len(), 1);
        let end = quarter.end().unwrap();
        assert!(end[0].abs() < 1e-9 && (end[1] - 10.0).abs() < 1e-9);
        let t = p.tangent_at(0.0);
        assert!(t[0].abs() < 1e-12 && (t[1] - 1.0).abs() < 1e-12);
        let back = quarter.reversed();
        assert_eq!(back.start(), quarter.end());
        assert!((back.length() - quarter.length()).abs() < 1e-12);
    }

    #[test]
    fn distances_to_lines_and_arcs() {
        let c = Contour::circle([0.0, 0.0], 10.0);
        assert!((distance_to(std::slice::from_ref(&c), [3.0, 4.0]) - 5.0).abs() < 1e-12);
        let r = Contour::rectangle([0.0, 0.0], [10.0, 10.0]);
        assert!((distance_to(std::slice::from_ref(&r), [15.0, 5.0]) - 5.0).abs() < 1e-12);
        assert!((distance_to(std::slice::from_ref(&r), [13.0, 14.0]) - 5.0).abs() < 1e-12);
        let half = Segment {
            start: [10.0, 0.0],
            end: [-10.0, 0.0],
            arc: Some(([0.0, 0.0], true)),
        };
        // Below the upper half circle its ends are nearest.
        assert!((seg_distance(&half, [0.0, -10.0]) - 200f64.sqrt()).abs() < 1e-12);
    }
}
