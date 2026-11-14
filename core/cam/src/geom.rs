// SPDX-License-Identifier: MIT
//! Planar outlines of lines and arcs, the input of the 2.5-axis
//! operations: a vertex's `bulge` makes the segment from it to the next
//! vertex an arc (`tan(sweep / 4)`, positive counter-clockwise; zero is a
//! line), the form the offsets work in (cavalier_contours), so that arcs
//! stay arcs through the tool radius compensation and come out of the
//! post-processors as G2 and G3.

use cavalier_contours::core::math::Vector2;
use cavalier_contours::polyline::{
    PlineOrientation, PlineSource, PlineSourceMut, PlineVertex, Polyline, seg_arc_radius_and_center,
};
use serde::{Deserialize, Serialize};

/// Positions closer than this are the same (mm).
pub const POS_EPS: f64 = 1e-6;

/// A vertex of an outline: its position and the bulge of the segment that
/// starts at it.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Vertex {
    pub x: f64,
    pub y: f64,
    /// `tan(sweep / 4)` of the arc to the next vertex: positive
    /// counter-clockwise, zero for a line; at most 1 in size (a half circle).
    #[serde(default)]
    pub bulge: f64,
}

impl Vertex {
    pub fn new(x: f64, y: f64, bulge: f64) -> Self {
        Vertex { x, y, bulge }
    }

    pub fn line(x: f64, y: f64) -> Self {
        Vertex { x, y, bulge: 0.0 }
    }

    pub fn pos(&self) -> [f64; 2] {
        [self.x, self.y]
    }
}

fn closed_by_default() -> bool {
    true
}

/// An outline in the plane of the operation, in the work coordinates (mm).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Contour {
    pub vertices: Vec<Vertex>,
    /// A closed outline joins its last vertex to the first (by the last
    /// vertex's bulge).
    #[serde(default = "closed_by_default")]
    pub closed: bool,
}

impl Contour {
    pub fn closed(vertices: Vec<Vertex>) -> Self {
        Contour {
            vertices,
            closed: true,
        }
    }

    pub fn open(vertices: Vec<Vertex>) -> Self {
        Contour {
            vertices,
            closed: false,
        }
    }

    /// A closed polygon through `points`.
    pub fn polygon(points: &[[f64; 2]]) -> Self {
        Contour::closed(points.iter().map(|p| Vertex::line(p[0], p[1])).collect())
    }

    /// The rectangle between two corners, counter-clockwise.
    pub fn rectangle(min: [f64; 2], max: [f64; 2]) -> Self {
        Contour::polygon(&[
            [min[0], min[1]],
            [max[0], min[1]],
            [max[0], max[1]],
            [min[0], max[1]],
        ])
    }

    /// A circle, counter-clockwise, as two half circles starting on its
    /// right (+X) side.
    pub fn circle(center: [f64; 2], radius: f64) -> Self {
        Contour::closed(vec![
            Vertex::new(center[0] + radius, center[1], 1.0),
            Vertex::new(center[0] - radius, center[1], 1.0),
        ])
    }

    /// The signed area: positive for a counter-clockwise outline, zero for
    /// an open one.
    pub fn area(&self) -> f64 {
        if !self.closed {
            return 0.0;
        }
        self.to_pline().area()
    }

    pub fn is_ccw(&self) -> bool {
        self.closed && self.to_pline().orientation() == PlineOrientation::CounterClockwise
    }

    /// The same outline run the other way.
    pub fn reversed(&self) -> Contour {
        let mut p = self.to_pline();
        p.invert_direction_mut();
        Contour::from_pline(&p)
    }

    /// The outline counter-clockwise (`ccw`) or clockwise.
    pub fn oriented(&self, ccw: bool) -> Contour {
        if self.is_ccw() == ccw {
            self.clone()
        } else {
            self.reversed()
        }
    }

    /// The length of the outline's path.
    pub fn length(&self) -> f64 {
        self.to_pline().path_length()
    }

    /// Checks what the operations need: at least two vertices (three for a
    /// line-only closed outline), finite numbers, bulges within a half
    /// circle, no zero-length segments. The message says what is wrong in
    /// words.
    pub fn validate(&self) -> Result<(), String> {
        let n = self.vertices.len();
        if n < 2 {
            return Err("An outline needs at least two points.".into());
        }
        if self
            .vertices
            .iter()
            .any(|v| !(v.x.is_finite() && v.y.is_finite() && v.bulge.is_finite()))
        {
            return Err("An outline has a point that is not a number.".into());
        }
        if self.vertices.iter().any(|v| v.bulge.abs() > 1.0 + 1e-9) {
            return Err(
                "An outline has an arc of more than half a circle; split it in two.".into(),
            );
        }
        let segments = if self.closed { n } else { n - 1 };
        for i in 0..segments {
            let a = self.vertices[i];
            let b = self.vertices[(i + 1) % n];
            if (a.x - b.x).hypot(a.y - b.y) <= POS_EPS {
                return Err("An outline has two points on top of each other.".into());
            }
        }
        if self.closed && self.area().abs() <= POS_EPS {
            return Err("A closed outline encloses no area.".into());
        }
        Ok(())
    }

    pub(crate) fn to_pline(&self) -> Polyline<f64> {
        let mut p = if self.closed {
            Polyline::new_closed()
        } else {
            Polyline::new()
        };
        for v in &self.vertices {
            p.add(v.x, v.y, v.bulge);
        }
        p
    }

    pub(crate) fn from_pline(p: &Polyline<f64>) -> Contour {
        Contour {
            vertices: p
                .iter_vertexes()
                .map(|v| Vertex::new(v.x, v.y, v.bulge))
                .collect(),
            closed: p.is_closed(),
        }
    }

    /// The closed outline started again at its point closest to `at`
    /// (a new vertex there when it is inside a segment).
    pub fn started_near(&self, at: [f64; 2]) -> Contour {
        if !self.closed || self.vertices.len() < 2 {
            return self.clone();
        }
        let p = self.to_pline();
        let Some(closest) = p.closest_point(Vector2::new(at[0], at[1]), POS_EPS) else {
            return self.clone();
        };
        match p.rotate_start(closest.seg_start_index, closest.seg_point, POS_EPS) {
            Some(r) => Contour::from_pline(&r),
            None => self.clone(),
        }
    }

    /// Whether `at` is inside the closed outline (by its winding number).
    pub fn contains(&self, at: [f64; 2]) -> bool {
        self.closed && self.to_pline().winding_number(Vector2::new(at[0], at[1])) != 0
    }

    /// The segments in order: `(start, end, arc)`, `arc` the center and
    /// whether it runs counter-clockwise.
    pub fn segments(&self) -> Vec<Segment> {
        let n = self.vertices.len();
        let count = if self.closed { n } else { n.saturating_sub(1) };
        (0..count)
            .map(|i| segment(self.vertices[i], self.vertices[(i + 1) % n]))
            .collect()
    }
}

/// A segment of an outline: a line, or an arc about `center`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Segment {
    pub start: [f64; 2],
    pub end: [f64; 2],
    /// The arc's center and direction (`true`: counter-clockwise).
    pub arc: Option<([f64; 2], bool)>,
}

/// The segment from `a` to `b` by `a`'s bulge.
pub fn segment(a: Vertex, b: Vertex) -> Segment {
    let start = a.pos();
    let end = b.pos();
    if a.bulge.abs() < 1e-12 || (a.x - b.x).hypot(a.y - b.y) <= POS_EPS {
        return Segment {
            start,
            end,
            arc: None,
        };
    }
    let (_, c) = seg_arc_radius_and_center(
        PlineVertex::new(a.x, a.y, a.bulge),
        PlineVertex::new(b.x, b.y, b.bulge),
    );
    Segment {
        start,
        end,
        arc: Some(([c.x, c.y], a.bulge > 0.0)),
    }
}

/// A planar region: an outer outline with islands (holes) inside it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Region {
    pub outer: Contour,
    #[serde(default)]
    pub islands: Vec<Contour>,
}

impl Region {
    pub fn new(outer: Contour, islands: Vec<Contour>) -> Self {
        Region { outer, islands }
    }

    pub fn validate(&self) -> Result<(), String> {
        if !self.outer.closed || self.islands.iter().any(|i| !i.closed) {
            return Err("A region's outlines must be closed.".into());
        }
        self.outer.validate()?;
        for island in &self.islands {
            island.validate()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rectangle_is_counter_clockwise_with_its_area() {
        let r = Contour::rectangle([0.0, 0.0], [20.0, 10.0]);
        assert!(r.is_ccw());
        assert!((r.area() - 200.0).abs() < 1e-9);
        assert!((r.reversed().area() + 200.0).abs() < 1e-9);
        assert!(!r.reversed().is_ccw());
    }

    #[test]
    fn circle_has_two_counter_clockwise_half_arcs() {
        let c = Contour::circle([5.0, 5.0], 2.0);
        let segs = c.segments();
        assert_eq!(segs.len(), 2);
        for s in &segs {
            let (center, ccw) = s.arc.expect("an arc");
            assert!(ccw);
            assert!((center[0] - 5.0).abs() < 1e-9 && (center[1] - 5.0).abs() < 1e-9);
        }
        assert!((c.area() - std::f64::consts::PI * 4.0).abs() < 1e-9);
    }

    #[test]
    fn started_near_rotates_to_the_closest_point() {
        let r = Contour::rectangle([0.0, 0.0], [10.0, 10.0]);
        let s = r.started_near([12.0, 5.0]);
        assert_eq!(s.vertices[0].pos(), [10.0, 5.0]);
        assert_eq!(s.vertices.len(), 5);
        assert!((s.area() - 100.0).abs() < 1e-9);
    }

    #[test]
    fn validation_says_what_is_wrong() {
        assert!(Contour::polygon(&[[0.0, 0.0]]).validate().is_err());
        assert!(
            Contour::polygon(&[[0.0, 0.0], [1.0, 0.0], [2.0, 0.0]])
                .validate()
                .is_err()
        );
        let big_arc = Contour::closed(vec![Vertex::new(0.0, 0.0, 2.0), Vertex::new(1.0, 0.0, 0.0)]);
        assert!(big_arc.validate().unwrap_err().contains("half a circle"));
        assert!(Contour::circle([0.0, 0.0], 1.0).validate().is_ok());
        let region = Region::new(
            Contour::rectangle([0.0, 0.0], [1.0, 1.0]),
            vec![Contour::open(vec![])],
        );
        assert!(region.validate().is_err());
    }

    #[test]
    fn contains_uses_the_winding_number() {
        let r = Contour::rectangle([0.0, 0.0], [10.0, 10.0]);
        assert!(r.contains([5.0, 5.0]));
        assert!(r.reversed().contains([5.0, 5.0]));
        assert!(!r.contains([15.0, 5.0]));
    }
}
