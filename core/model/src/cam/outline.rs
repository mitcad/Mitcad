// SPDX-License-Identifier: MIT
//! The model's geometry as planar outlines in the work coordinates, the
//! input of the CAM operations (mitcad#1013): the kernel's curves (a
//! face's boundary, edges) and a sketch's segments and curves become
//! pieces in model space ([`Piece`]), which the work frame turns into
//! segments of the work plane ([`Seg`]): arcs about the tool axis stay
//! arcs (a bulge), other curves become short lines within a tolerance.
//! Segments are chained end to end into outlines ([`chains`]), and closed
//! outlines nested into regions with islands ([`regions`]).

use std::collections::VecDeque;
use std::f64::consts::{PI, TAU};

use mitcad_cam::geom::{Contour, Region, Vertex};
use mitcad_model_base::curves::{bspline_point, conic_point, helix_point};
use mitcad_vec3::{add, cross, dot, norm, scale, sub};

use crate::kernel::Curve3;
use crate::profile::{SegmentGeometry, SketchFrame};
use crate::sketch::geometry::Curve2;
use crate::transform::Transform;

type P3 = [f64; 3];

/// Ends closer than this (mm, in the work plane) join.
pub const JOIN: f64 = 1e-3;
/// How far lines that stand for a curve may stray from it (mm).
pub const TOLERANCE: f64 = 0.005;

/// The work coordinate system in the model: an origin and three unit axes
/// at right angles (Z the tool axis, up).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorkFrame {
    pub origin: P3,
    pub x: P3,
    pub y: P3,
    pub z: P3,
}

impl WorkFrame {
    /// The design's own axes at the model origin.
    pub const MODEL: WorkFrame = WorkFrame {
        origin: [0.0, 0.0, 0.0],
        x: [1.0, 0.0, 0.0],
        y: [0.0, 1.0, 0.0],
        z: [0.0, 0.0, 1.0],
    };

    /// A model point in work coordinates.
    pub fn to_work(&self, p: P3) -> P3 {
        let d = sub(p, self.origin);
        [dot(d, self.x), dot(d, self.y), dot(d, self.z)]
    }

    /// A work point in the model.
    pub fn to_model(&self, w: P3) -> P3 {
        add(
            self.origin,
            add(
                scale(self.x, w[0]),
                add(scale(self.y, w[1]), scale(self.z, w[2])),
            ),
        )
    }

    /// A model direction in work coordinates.
    pub fn direction(&self, v: P3) -> P3 {
        [dot(v, self.x), dot(v, self.y), dot(v, self.z)]
    }
}

/// A curve piece in model space.
#[derive(Debug, Clone, PartialEq)]
pub enum Piece {
    Line {
        a: P3,
        b: P3,
    },
    /// Counter-clockwise about the unit `normal`, from `a` through `sweep`
    /// radians (2π: a whole circle).
    Arc {
        center: P3,
        normal: P3,
        a: P3,
        sweep: f64,
    },
    /// Points along a curve, in order.
    Points(Vec<P3>),
}

/// Points on a circle of `radius` within `tolerance`: how many segments a
/// sweep takes.
fn steps(radius: f64, sweep: f64, tolerance: f64) -> usize {
    let radius = radius.abs().max(1e-9);
    let half = (1.0 - (tolerance / radius).min(1.0)).acos().max(1e-3);
    ((sweep.abs() / (2.0 * half)).ceil() as usize).clamp(4, 4096)
}

/// The sweep from `start` to `end` counter-clockwise, in (0, 2π].
fn ccw_sweep(start: f64, end: f64) -> f64 {
    let mut sweep = end - start;
    while sweep <= 1e-12 {
        sweep += TAU;
    }
    while sweep > TAU + 1e-12 {
        sweep -= TAU;
    }
    sweep
}

fn unit3(v: P3) -> Option<P3> {
    let n = norm(v);
    (n > 1e-12).then(|| scale(v, 1.0 / n))
}

impl Piece {
    /// The piece where an occurrence places it: `placement` takes its
    /// component's coordinates into the design's (a rigid motion; a
    /// mirror turns an arc's sense, so its normal turns over).
    pub fn placed(&self, placement: &Transform) -> Piece {
        match self {
            Piece::Line { a, b } => Piece::Line {
                a: placement.apply_point(*a),
                b: placement.apply_point(*b),
            },
            Piece::Arc {
                center,
                normal,
                a,
                sweep,
            } => {
                let turned = placement.apply_vector(*normal);
                let turned = if placement.determinant() < 0.0 {
                    scale(turned, -1.0)
                } else {
                    turned
                };
                Piece::Arc {
                    center: placement.apply_point(*center),
                    normal: unit3(turned).unwrap_or(*normal),
                    a: placement.apply_point(*a),
                    sweep: *sweep,
                }
            }
            Piece::Points(points) => {
                Piece::Points(points.iter().map(|p| placement.apply_point(*p)).collect())
            }
        }
    }
}

/// The kernel's curve as a piece; None for a point.
pub fn piece_of_curve(curve: &Curve3) -> Option<Piece> {
    match curve {
        Curve3::Point(_) => None,
        Curve3::Line { start, end } => Some(Piece::Line { a: *start, b: *end }),
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
            let sweep = if *closed {
                TAU
            } else {
                ccw_sweep(*start, *end)
            };
            let at = |t: f64| conic_point(*center, *normal, *x_axis, (*major, *minor), t);
            let normal = unit3(*normal)?;
            if (major - minor).abs() <= 1e-9 * major.abs().max(1.0) {
                Some(Piece::Arc {
                    center: *center,
                    normal,
                    a: at(*start),
                    sweep,
                })
            } else {
                let n = steps(minor.abs().min(major.abs()), sweep, TOLERANCE);
                Some(Piece::Points(
                    (0..=n)
                        .map(|i| at(start + sweep * i as f64 / n as f64))
                        .collect(),
                ))
            }
        }
        Curve3::BSpline {
            degree,
            poles,
            weights,
            knots,
        } => {
            let p = *degree as usize;
            if p == 0 || poles.len() <= p || knots.len() != poles.len() + p + 1 {
                return (poles.len() >= 2).then(|| Piece::Points(poles.clone()));
            }
            let (t0, t1) = (knots[p], knots[poles.len()]);
            let n = (poles.len() * 16).clamp(16, 4096);
            Some(Piece::Points(
                (0..=n)
                    .map(|i| {
                        let t = t0 + (t1 - t0) * i as f64 / n as f64;
                        bspline_point(p, poles, weights, knots, t.min(t1))
                    })
                    .collect(),
            ))
        }
        Curve3::Helix {
            center,
            axis,
            x_axis,
            radius,
            pitch,
            start,
            end,
        } => {
            let n = steps(*radius, end - start, TOLERANCE);
            Some(Piece::Points(
                (0..=n)
                    .map(|i| {
                        let t = start + (end - start) * i as f64 / n as f64;
                        helix_point(*center, *axis, *x_axis, (*radius, *pitch), t)
                    })
                    .collect(),
            ))
        }
    }
}

/// A point of an ellipse in its plane.
fn ellipse_point(center: [f64; 2], a: f64, b: f64, rotation: f64, t: f64) -> [f64; 2] {
    let (x, y) = (a * t.cos(), b * t.sin());
    let (s, c) = rotation.sin_cos();
    [center[0] + x * c - y * s, center[1] + x * s + y * c]
}

/// An arc of a sketch in model space.
fn sketch_arc(frame: &SketchFrame, center: [f64; 2], radius: f64, start: f64, sweep: f64) -> Piece {
    Piece::Arc {
        center: frame.point(center),
        normal: frame.normal(),
        a: frame.point([
            center[0] + radius * start.cos(),
            center[1] + radius * start.sin(),
        ]),
        sweep,
    }
}

/// A profile segment of a sketch region in model space.
pub fn piece_of_segment(geometry: &SegmentGeometry, frame: &SketchFrame) -> Result<Piece, String> {
    Ok(match geometry {
        SegmentGeometry::Line { start, end } => Piece::Line {
            a: frame.point(*start),
            b: frame.point(*end),
        },
        SegmentGeometry::Arc {
            center,
            radius,
            start_angle,
            end_angle,
        } => sketch_arc(
            frame,
            *center,
            *radius,
            *start_angle,
            ccw_sweep(*start_angle, *end_angle),
        ),
        SegmentGeometry::Circle {
            center,
            radius,
            start,
        } => sketch_arc(frame, *center, *radius, *start, TAU),
        SegmentGeometry::Ellipse {
            center,
            major_radius,
            minor_radius,
            rotation,
        } => {
            let n = steps(*minor_radius, TAU, TOLERANCE);
            Piece::Points(
                (0..=n)
                    .map(|i| {
                        let t = TAU * i as f64 / n as f64;
                        frame.point(ellipse_point(
                            *center,
                            *major_radius,
                            *minor_radius,
                            *rotation,
                            t,
                        ))
                    })
                    .collect(),
            )
        }
        SegmentGeometry::EllipseArc {
            center,
            major_radius,
            minor_radius,
            rotation,
            start_angle,
            end_angle,
        } => {
            let sweep = ccw_sweep(*start_angle, *end_angle);
            let n = steps(*minor_radius, sweep, TOLERANCE);
            Piece::Points(
                (0..=n)
                    .map(|i| {
                        let t = start_angle + sweep * i as f64 / n as f64;
                        frame.point(ellipse_point(
                            *center,
                            *major_radius,
                            *minor_radius,
                            *rotation,
                            t,
                        ))
                    })
                    .collect(),
            )
        }
        SegmentGeometry::BSpline {
            degree,
            poles,
            weights,
            knots,
            multiplicities,
            periodic,
        } => {
            if *periodic {
                return Err(
                    "A closed spline of a sketch cannot be machined yet: split it into open pieces."
                        .to_owned(),
                );
            }
            let flat: Vec<f64> = knots
                .iter()
                .zip(multiplicities)
                .flat_map(|(k, m)| std::iter::repeat_n(*k, *m as usize))
                .collect();
            let poles3: Vec<P3> = poles.iter().map(|p| [p[0], p[1], 0.0]).collect();
            let curve = Curve3::BSpline {
                degree: *degree,
                poles: poles3,
                weights: weights.clone(),
                knots: flat,
            };
            match piece_of_curve(&curve) {
                Some(Piece::Points(points)) => {
                    Piece::Points(points.iter().map(|p| frame.point([p[0], p[1]])).collect())
                }
                _ => return Err("A spline of a sketch cannot be read.".to_owned()),
            }
        }
    })
}

/// A solved sketch curve in model space.
pub fn piece_of_sketch_curve(curve: &Curve2, frame: &SketchFrame) -> Piece {
    match curve {
        Curve2::Line { a, b } => Piece::Line {
            a: frame.point(*a),
            b: frame.point(*b),
        },
        Curve2::Circle {
            center,
            radius,
            start,
        } => sketch_arc(frame, *center, *radius, *start, TAU),
        Curve2::Arc {
            center,
            radius,
            start,
            end,
        } => sketch_arc(frame, *center, *radius, *start, ccw_sweep(*start, *end)),
        other => {
            let (t0, t1) = other.domain();
            let n = 256;
            Piece::Points(
                (0..=n)
                    .map(|i| frame.point(other.point(t0 + (t1 - t0) * i as f64 / n as f64)))
                    .collect(),
            )
        }
    }
}

/// A segment of the work plane: from `a` to `b` (work coordinates, their
/// Z kept for the height), an arc when `bulge` is not zero
/// (`tan(sweep / 4)`, positive counter-clockwise).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Seg {
    pub a: P3,
    pub b: P3,
    pub bulge: f64,
}

impl Seg {
    fn reversed(self) -> Seg {
        Seg {
            a: self.b,
            b: self.a,
            bulge: -self.bulge,
        }
    }
}

fn line_segs(points: &[P3], out: &mut Vec<Seg>) {
    for pair in points.windows(2) {
        out.push(Seg {
            a: pair[0],
            b: pair[1],
            bulge: 0.0,
        });
    }
}

/// The piece's segments in the work plane.
pub fn work_segments(piece: &Piece, frame: &WorkFrame) -> Vec<Seg> {
    let mut out = Vec::new();
    match piece {
        Piece::Line { a, b } => out.push(Seg {
            a: frame.to_work(*a),
            b: frame.to_work(*b),
            bulge: 0.0,
        }),
        Piece::Points(points) => {
            let points: Vec<P3> = points.iter().map(|p| frame.to_work(*p)).collect();
            line_segs(&points, &mut out);
        }
        Piece::Arc {
            center,
            normal,
            a,
            sweep,
        } => {
            let Some(u) = unit3(sub(*a, *center)) else {
                return out;
            };
            let radius = norm(sub(*a, *center));
            let v = cross(*normal, u);
            let at = |theta: f64| {
                add(
                    *center,
                    add(
                        scale(u, radius * theta.cos()),
                        scale(v, radius * theta.sin()),
                    ),
                )
            };
            let up = frame.direction(*normal)[2];
            if up.abs() >= 1.0 - 1e-9 {
                // About the tool axis: arcs of at most half a turn.
                let parts = ((sweep / PI) - 1e-9).ceil().max(1.0) as usize;
                let part = sweep / parts as f64;
                let bulge = (part / 4.0).tan() * up.signum();
                for i in 0..parts {
                    out.push(Seg {
                        a: frame.to_work(at(part * i as f64)),
                        b: frame.to_work(at(part * (i + 1) as f64)),
                        bulge,
                    });
                }
            } else {
                let n = steps(radius, *sweep, TOLERANCE);
                let points: Vec<P3> = (0..=n)
                    .map(|i| frame.to_work(at(sweep * i as f64 / n as f64)))
                    .collect();
                line_segs(&points, &mut out);
            }
        }
    }
    out.retain(|s| s.bulge != 0.0 || plane_distance(s.a, s.b) > 1e-9);
    out
}

fn plane_distance(a: P3, b: P3) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

/// An outline made of segments, with its height.
#[derive(Debug, Clone, PartialEq)]
pub struct Chain {
    pub contour: Contour,
    /// The mean work Z of its segments' ends.
    pub z: f64,
    /// All ends within [`JOIN`] of that height: the outline is level with
    /// the work plane.
    pub level: bool,
}

/// Takes the segment that starts (or, reversed, ends) at `at`.
fn take_from(left: &mut [Option<Seg>], at: P3) -> Option<Seg> {
    for slot in left.iter_mut() {
        let Some(seg) = *slot else { continue };
        if plane_distance(seg.a, at) <= JOIN {
            *slot = None;
            return Some(seg);
        }
        if plane_distance(seg.b, at) <= JOIN {
            *slot = None;
            return Some(seg.reversed());
        }
    }
    None
}

/// Takes the segment that ends (or, reversed, starts) at `at`.
fn take_to(left: &mut [Option<Seg>], at: P3) -> Option<Seg> {
    take_from(left, at).map(Seg::reversed)
}

/// Segments chained end to end into outlines: closed where a chain comes
/// back to its start, open otherwise.
pub fn chains(segs: Vec<Seg>) -> Vec<Chain> {
    let mut left: Vec<Option<Seg>> = segs.into_iter().map(Some).collect();
    let mut out = Vec::new();
    while let Some(first) = left.iter_mut().find_map(Option::take) {
        let mut chain: VecDeque<Seg> = VecDeque::from([first]);
        let closes = |chain: &VecDeque<Seg>| {
            let (Some(front), Some(back)) = (chain.front(), chain.back()) else {
                return false;
            };
            (chain.len() > 1 || front.bulge != 0.0) && plane_distance(back.b, front.a) <= JOIN
        };
        while !closes(&chain) {
            let end = chain.back().map(|s| s.b).unwrap_or_default();
            match take_from(&mut left, end) {
                Some(seg) => chain.push_back(seg),
                None => break,
            }
        }
        let closed = closes(&chain);
        if !closed {
            loop {
                let start = chain.front().map(|s| s.a).unwrap_or_default();
                match take_to(&mut left, start) {
                    Some(seg) => chain.push_front(seg),
                    None => break,
                }
                if closes(&chain) {
                    break;
                }
            }
        }
        let closed = closes(&chain);
        let zs: Vec<f64> = chain.iter().flat_map(|s| [s.a[2], s.b[2]]).collect();
        let z = zs.iter().sum::<f64>() / zs.len().max(1) as f64;
        let level = zs.iter().all(|v| (v - z).abs() <= JOIN);
        let mut vertices: Vec<Vertex> = chain
            .iter()
            .map(|s| Vertex::new(s.a[0], s.a[1], s.bulge))
            .collect();
        if !closed && let Some(last) = chain.back() {
            vertices.push(Vertex::line(last.b[0], last.b[1]));
        }
        let contour = if closed {
            Contour::closed(vertices)
        } else {
            Contour::open(vertices)
        };
        out.push(Chain { contour, z, level });
    }
    out
}

/// Closed outlines nested into regions: an outline inside an even number
/// of others is a region's outer outline, one inside an odd number an
/// island of the smallest outer outline around it.
pub fn regions(loops: &[Contour]) -> Vec<Region> {
    let inside = |inner: &Contour, outer: &Contour| {
        inner
            .vertices
            .first()
            .is_some_and(|v| outer.contains([v.x, v.y]))
    };
    let depth: Vec<usize> = loops
        .iter()
        .enumerate()
        .map(|(i, c)| {
            loops
                .iter()
                .enumerate()
                .filter(|(j, other)| *j != i && inside(c, other))
                .count()
        })
        .collect();
    let mut out: Vec<(usize, Region)> = Vec::new();
    for (i, c) in loops.iter().enumerate() {
        if depth[i].is_multiple_of(2) {
            out.push((i, Region::new(c.clone(), Vec::new())));
        }
    }
    for (i, c) in loops.iter().enumerate() {
        if !depth[i].is_multiple_of(2) {
            let owner = out
                .iter_mut()
                .filter(|(o, _)| depth[*o] + 1 == depth[i] && inside(c, &loops[*o]))
                .min_by(|a, b| loops[a.0].area().abs().total_cmp(&loops[b.0].area().abs()));
            if let Some((_, region)) = owner {
                region.islands.push(c.clone());
            }
        }
    }
    out.into_iter().map(|(_, r)| r).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(x0: f64, y0: f64, size: f64, z: f64) -> Vec<Seg> {
        let p = [
            [x0, y0, z],
            [x0 + size, y0, z],
            [x0 + size, y0 + size, z],
            [x0, y0 + size, z],
        ];
        // Out of order and one reversed, as a face's boundary may come.
        vec![
            Seg {
                a: p[2],
                b: p[3],
                bulge: 0.0,
            },
            Seg {
                a: p[1],
                b: p[0],
                bulge: 0.0,
            },
            Seg {
                a: p[3],
                b: p[0],
                bulge: 0.0,
            },
            Seg {
                a: p[1],
                b: p[2],
                bulge: 0.0,
            },
        ]
    }

    #[test]
    fn segments_out_of_order_chain_into_one_closed_level_outline() {
        let found = chains(square(0.0, 0.0, 10.0, -3.0));
        assert_eq!(found.len(), 1);
        assert!(found[0].contour.closed);
        assert_eq!(found[0].contour.vertices.len(), 4);
        assert!(found[0].level);
        assert!((found[0].z + 3.0).abs() < 1e-12);
        assert!((found[0].contour.area().abs() - 100.0).abs() < 1e-9);
    }

    #[test]
    fn open_edges_chain_from_either_end() {
        let segs = vec![
            Seg {
                a: [5.0, 0.0, 0.0],
                b: [10.0, 0.0, 0.0],
                bulge: 0.0,
            },
            Seg {
                a: [0.0, 0.0, 0.0],
                b: [5.0, 0.0, 0.0],
                bulge: 0.0,
            },
        ];
        let found = chains(segs);
        assert_eq!(found.len(), 1);
        assert!(!found[0].contour.closed);
        let xs: Vec<f64> = found[0].contour.vertices.iter().map(|v| v.x).collect();
        assert_eq!(xs, vec![0.0, 5.0, 10.0]);
    }

    #[test]
    fn a_circle_about_the_tool_axis_stays_two_half_arcs() {
        let piece = piece_of_curve(&Curve3::Conic {
            center: [0.0, 0.0, 5.0],
            normal: [0.0, 0.0, 1.0],
            x_axis: [1.0, 0.0, 0.0],
            major: 4.0,
            minor: 4.0,
            start: 0.0,
            end: TAU,
            closed: true,
        })
        .unwrap();
        let segs = work_segments(&piece, &WorkFrame::MODEL);
        assert_eq!(segs.len(), 2);
        assert!(segs.iter().all(|s| (s.bulge - 1.0).abs() < 1e-12));
        let found = chains(segs);
        assert_eq!(found.len(), 1);
        assert!(found[0].contour.closed);
        assert!((found[0].contour.area() - PI * 16.0).abs() < 1e-6);
        // Seen from below (the work Z down), the same circle runs
        // clockwise.
        let below = WorkFrame {
            origin: [0.0; 3],
            x: [1.0, 0.0, 0.0],
            y: [0.0, -1.0, 0.0],
            z: [0.0, 0.0, -1.0],
        };
        let segs = work_segments(&piece, &below);
        assert!(segs.iter().all(|s| (s.bulge + 1.0).abs() < 1e-12));
    }

    #[test]
    fn a_square_with_a_square_hole_is_one_region_with_an_island() {
        let mut segs = square(0.0, 0.0, 30.0, 0.0);
        segs.extend(square(10.0, 10.0, 5.0, 0.0));
        segs.extend(square(50.0, 0.0, 10.0, 0.0));
        let loops: Vec<Contour> = chains(segs).into_iter().map(|c| c.contour).collect();
        assert_eq!(loops.len(), 3);
        let found = regions(&loops);
        assert_eq!(found.len(), 2);
        let with_island = found.iter().find(|r| !r.islands.is_empty()).unwrap();
        assert!((with_island.outer.area().abs() - 900.0).abs() < 1e-9);
        assert!((with_island.islands[0].area().abs() - 25.0).abs() < 1e-9);
    }

    #[test]
    fn work_frames_turn_points_both_ways() {
        let frame = WorkFrame {
            origin: [10.0, 20.0, 30.0],
            x: [0.0, 1.0, 0.0],
            y: [-1.0, 0.0, 0.0],
            z: [0.0, 0.0, 1.0],
        };
        let w = frame.to_work([11.0, 23.0, 25.0]);
        assert_eq!(w, [3.0, -1.0, -5.0]);
        assert_eq!(frame.to_model(w), [11.0, 23.0, 25.0]);
    }

    #[test]
    fn pieces_go_where_their_occurrence_places_them() {
        let circle = piece_of_curve(&Curve3::Conic {
            center: [0.0, 0.0, 5.0],
            normal: [0.0, 0.0, 1.0],
            x_axis: [1.0, 0.0, 0.0],
            major: 4.0,
            minor: 4.0,
            start: 0.0,
            end: TAU,
            closed: true,
        })
        .unwrap();
        // Moved and turned a quarter about Z: the centre moves, the arcs
        // stay counter-clockwise about the tool axis.
        let placement = Transform::translation([100.0, 20.0, 0.0])
            .after(&Transform::rotation([0.0; 3], [0.0, 0.0, 1.0], PI / 2.0).unwrap());
        let placed = circle.placed(&placement);
        let Piece::Arc { center, normal, .. } = &placed else {
            panic!("{placed:?}");
        };
        assert!((center[0] - 100.0).abs() < 1e-9 && (center[1] - 20.0).abs() < 1e-9);
        assert!((normal[2] - 1.0).abs() < 1e-12);
        let segs = work_segments(&placed, &WorkFrame::MODEL);
        assert!(segs.iter().all(|s| (s.bulge - 1.0).abs() < 1e-12));
        // Mirrored across the XZ plane: the same circle, clockwise.
        let mirror = Transform::mirror([0.0; 3], [0.0, 1.0, 0.0]).unwrap();
        let segs = work_segments(&circle.placed(&mirror), &WorkFrame::MODEL);
        assert!(segs.iter().all(|s| (s.bulge + 1.0).abs() < 1e-12));
        // A line's ends move.
        let line = Piece::Line {
            a: [0.0; 3],
            b: [1.0, 0.0, 0.0],
        };
        assert_eq!(
            line.placed(&Transform::translation([5.0, 6.0, 7.0])),
            Piece::Line {
                a: [5.0, 6.0, 7.0],
                b: [6.0, 6.0, 7.0],
            }
        );
    }
}
