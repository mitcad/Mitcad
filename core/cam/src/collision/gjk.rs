// SPDX-License-Identifier: MIT
//! The distance between two convex shapes given by their support
//! functions: Gilbert, Johnson and Keerthi's algorithm, with the point of
//! the simplex nearest the origin found by its Voronoi regions.
//!
//! The answer is a lower bound of the distance that is never above it
//! (every support point gives one: no point of `A - B` is nearer the
//! origin than `v·w / |v|`), and an upper bound (the nearest point found);
//! they meet within a relative `REL` when the iteration ends. The shapes
//! overlap or touch when the lower bound is 0. A swept shape tells where on
//! its path the nearest point is through the `tag` of its support points.

use mitcad_vec3::{add, cross, dot, scale, sub};

use crate::toolpath::P3;

/// A convex shape for [`distance`]: its point farthest along `d` (any one
/// of them when several are), and a tag of that point that the nearest
/// point's tag is the same mix of (a swept shape's position on its path; 0
/// for a shape that does not sweep).
pub(crate) trait Convex {
    fn support(&self, d: P3) -> (P3, P3);
}

/// The bounds converge to within this part of the distance.
const REL: f64 = 1e-10;
/// Squared lengths below this are the origin.
const TINY2: f64 = 1e-26;
const MAX_ITERATIONS: usize = 200;

/// What [`distance`] found.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Distance {
    /// Never above the distance; 0 when the shapes meet.
    pub lower: f64,
    /// The tag of the nearest point of the first shape (meaningless when
    /// they meet).
    pub tag: P3,
}

#[derive(Clone, Copy, Debug)]
struct Vertex {
    /// A point of `A - B`.
    w: P3,
    /// The tag of its point of `A`.
    tag: P3,
}

fn neg(v: P3) -> P3 {
    [-v[0], -v[1], -v[2]]
}

fn support(a: &dyn Convex, b: &dyn Convex, d: P3) -> Vertex {
    let (pa, tag) = a.support(d);
    let (pb, _) = b.support(neg(d));
    Vertex {
        w: sub(pa, pb),
        tag,
    }
}

/// The distance between the convex shapes `a` and `b`.
pub(crate) fn distance(a: &dyn Convex, b: &dyn Convex) -> Distance {
    let meet = Distance {
        lower: 0.0,
        tag: [0.0; 3],
    };
    let mut simplex: Vec<(Vertex, f64)> = vec![(support(a, b, [1.0, 0.0, 0.0]), 1.0)];
    let mut v = simplex[0].0.w;
    let mut lower = 0.0_f64;
    for _ in 0..MAX_ITERATIONS {
        let vv = dot(v, v);
        if vv <= TINY2 {
            return meet;
        }
        let w = support(a, b, neg(v));
        let vw = dot(v, w.w);
        if vw > 0.0 {
            lower = lower.max(vw / vv.sqrt());
        }
        // No point of A - B is much nearer than v: done.
        if vv - vw <= REL * vv {
            break;
        }
        if simplex.iter().any(|(s, _)| s.w == w.w) {
            break;
        }
        let mut points: Vec<Vertex> = simplex.iter().map(|(s, _)| *s).collect();
        points.push(w);
        let Some(nearest) = nearest_on_simplex(&points) else {
            // The origin is inside the tetrahedron: they overlap.
            return meet;
        };
        let p = mix(&nearest);
        let pp = dot(p, p);
        if pp <= TINY2 {
            return meet;
        }
        if pp >= vv {
            // No progress left in the numbers.
            break;
        }
        v = p;
        simplex = nearest;
    }
    let upper = dot(v, v).sqrt();
    let tag = simplex
        .iter()
        .fold([0.0; 3], |acc, (s, l)| add(acc, scale(s.tag, *l)));
    Distance {
        lower: lower.min(upper),
        tag,
    }
}

/// The point the weights make of the vertices.
fn mix(weighted: &[(Vertex, f64)]) -> P3 {
    weighted
        .iter()
        .fold([0.0; 3], |acc, (s, l)| add(acc, scale(s.w, *l)))
}

/// The vertices (with their weights) of the smallest face of the simplex
/// that holds its point nearest the origin; `None` when the origin is
/// inside a tetrahedron.
fn nearest_on_simplex(s: &[Vertex]) -> Option<Vec<(Vertex, f64)>> {
    match s.len() {
        1 => Some(vec![(s[0], 1.0)]),
        2 => Some(nearest_on_segment(s[0], s[1])),
        3 => Some(nearest_on_triangle(s[0], s[1], s[2])),
        _ => nearest_on_tetrahedron(s[0], s[1], s[2], s[3]),
    }
}

fn nearest_on_segment(a: Vertex, b: Vertex) -> Vec<(Vertex, f64)> {
    let ab = sub(b.w, a.w);
    let len2 = dot(ab, ab);
    if len2 <= TINY2 {
        return vec![(a, 1.0)];
    }
    let t = -dot(a.w, ab) / len2;
    if t <= 0.0 {
        vec![(a, 1.0)]
    } else if t >= 1.0 {
        vec![(b, 1.0)]
    } else {
        vec![(a, 1.0 - t), (b, t)]
    }
}

fn squared(weighted: &[(Vertex, f64)]) -> f64 {
    let p = mix(weighted);
    dot(p, p)
}

/// The nearest point of the triangle to the origin by its Voronoi regions
/// (vertices, edges, inside).
fn nearest_on_triangle(a: Vertex, b: Vertex, c: Vertex) -> Vec<(Vertex, f64)> {
    let ab = sub(b.w, a.w);
    let ac = sub(c.w, a.w);
    let ap = neg(a.w);
    let d1 = dot(ab, ap);
    let d2 = dot(ac, ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return vec![(a, 1.0)];
    }
    let bp = neg(b.w);
    let d3 = dot(ab, bp);
    let d4 = dot(ac, bp);
    if d3 >= 0.0 && d4 <= d3 {
        return vec![(b, 1.0)];
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let t = d1 / (d1 - d3);
        return vec![(a, 1.0 - t), (b, t)];
    }
    let cp = neg(c.w);
    let d5 = dot(ab, cp);
    let d6 = dot(ac, cp);
    if d6 >= 0.0 && d5 <= d6 {
        return vec![(c, 1.0)];
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let t = d2 / (d2 - d6);
        return vec![(a, 1.0 - t), (c, t)];
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let t = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        return vec![(b, 1.0 - t), (c, t)];
    }
    let sum = va + vb + vc;
    let n = cross(ab, ac);
    // (The sum is |n|² for a triangle with an area.)
    if dot(n, n) <= 1e-20 * dot(ab, ab) * dot(ac, ac) || sum.abs() <= TINY2 {
        // A triangle with no area: its nearest edge.
        return [
            nearest_on_segment(a, b),
            nearest_on_segment(b, c),
            nearest_on_segment(a, c),
        ]
        .into_iter()
        .min_by(|x, y| squared(x).total_cmp(&squared(y)))
        .expect("three edges");
    }
    let v = vb / sum;
    let w = vc / sum;
    vec![(a, 1.0 - v - w), (b, v), (c, w)]
}

/// Whether the origin is on the other side of the plane through `a`, `b`
/// and `c` than `d` (or the four are nearly flat, which counts as outside
/// so that the face is searched).
fn origin_outside(a: P3, b: P3, c: P3, d: P3) -> bool {
    let n = cross(sub(b, a), sub(c, a));
    let sp = dot(neg(a), n);
    let sd = dot(sub(d, a), n);
    let scale_d = dot(n, n).sqrt() * dot(sub(d, a), sub(d, a)).sqrt();
    if sd.abs() <= 1e-12 * scale_d {
        return true;
    }
    sp * sd < 0.0
}

fn nearest_on_tetrahedron(
    a: Vertex,
    b: Vertex,
    c: Vertex,
    d: Vertex,
) -> Option<Vec<(Vertex, f64)>> {
    let faces = [(a, b, c, d), (a, c, d, b), (a, d, b, c), (b, d, c, a)];
    let mut best: Option<(f64, Vec<(Vertex, f64)>)> = None;
    for (p, q, r, opposite) in faces {
        if !origin_outside(p.w, q.w, r.w, opposite.w) {
            continue;
        }
        let found = nearest_on_triangle(p, q, r);
        let d2 = squared(&found);
        if best.as_ref().is_none_or(|(b2, _)| d2 < *b2) {
            best = Some((d2, found));
        }
    }
    best.map(|(_, found)| found)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Cube {
        min: P3,
        max: P3,
    }

    impl Convex for Cube {
        fn support(&self, d: P3) -> (P3, P3) {
            (
                std::array::from_fn(|k| {
                    if d[k] >= 0.0 {
                        self.max[k]
                    } else {
                        self.min[k]
                    }
                }),
                [0.0; 3],
            )
        }
    }

    struct Ball {
        center: P3,
        radius: f64,
    }

    impl Convex for Ball {
        fn support(&self, d: P3) -> (P3, P3) {
            let l = dot(d, d).sqrt();
            if l == 0.0 {
                return (self.center, [0.0; 3]);
            }
            (add(self.center, scale(d, self.radius / l)), [0.0; 3])
        }
    }

    #[test]
    fn boxes_apart_corner_to_corner() {
        let a = Cube {
            min: [0.0; 3],
            max: [1.0; 3],
        };
        let b = Cube {
            min: [4.0, 5.0, 1.0],
            max: [6.0, 7.0, 2.0],
        };
        let d = distance(&a, &b);
        // Apart by 3 in x and 4 in y, touching levels in z.
        assert!((d.lower - 5.0).abs() < 1e-9, "{d:?}");
    }

    #[test]
    fn overlapping_and_touching_shapes_meet() {
        let a = Cube {
            min: [0.0; 3],
            max: [2.0; 3],
        };
        let inside = Cube {
            min: [0.5; 3],
            max: [1.0; 3],
        };
        assert_eq!(distance(&a, &inside).lower, 0.0);
        let touching = Cube {
            min: [2.0, 0.0, 0.0],
            max: [3.0, 1.0, 1.0],
        };
        assert!(distance(&a, &touching).lower <= 1e-12);
    }

    #[test]
    fn a_ball_converges_to_its_true_distance() {
        let a = Cube {
            min: [-1.0; 3],
            max: [1.0; 3],
        };
        let b = Ball {
            center: [4.0, 3.0, 0.5],
            radius: 1.5,
        };
        // Nearest corner region: the edge x = 1, y = 1 (z within the box).
        let expected = (3.0_f64).hypot(2.0) - 1.5;
        let d = distance(&a, &b);
        assert!(d.lower <= expected + 1e-12, "{d:?}");
        assert!((d.lower - expected).abs() < 1e-6, "{d:?}");
    }
}
