// SPDX-License-Identifier: MIT
//! How large construction planes and axes are drawn (mitcad#1827, GitHub
//! mitcad/Mitcad#321): at the size and shape of what they are made from,
//! worked out with the feature, so that the drawing follows the model.
//! Display only: the datum, what sketches and features use, is the same.
//!
//! - A plane made on or from a face (offset, at an angle, tangent,
//!   midplane): a rectangle in the plane covering the faces' outlines seen
//!   along the plane's normal, its long side along the longest straight
//!   edge of the outlines, else along the plane's own x axis.
//! - A plane through edges, axes or points (or made on a face whose
//!   outline the kernel cannot give): a square covering where the plane
//!   cuts the boxes of the bodies they are on, else the referenced
//!   geometry.
//! - An axis: the piece of it along its edges', faces', planes' and
//!   points' extent, else where it passes through the boxes of the bodies
//!   they are on.
//! - Each with a margin all round of 5 % of its larger side, at least
//!   [`MIN_MARGIN`]. Nothing to measure (a fixed plane, an origin plane's
//!   offset): none, and the application draws its default size.
//!
//! A construction plane or axis made from another one covers the other's
//! extent (without its margin, so that margins do not pile up).
//!
//! The resize handles' size (`display_size`, mitcad#1748) overrides this:
//! see [`DrawnExtent::sized`].

use super::{GeomRef, PathRef};
use crate::datum::{CurveGeometry, Datum, DatumAxis, DatumPlane, Vec3};
use crate::features::EvalContext;
use crate::features::geom_ref::Resolver;
use crate::ids::{BodyUid, FeatureUid};
use crate::kernel::{BoundingBox, Curve3, Kernel};
use crate::recompute::Read;
use mitcad_vec3::{add, cross, dot, norm, scale, sub, unit};

/// How large a construction plane (its square's side) or axis (its line's
/// length) is drawn with nothing to measure and no size of its own, mm
/// (mitcad#1748).
pub const DEFAULT_SIZE: f64 = 100.0;

/// The smallest margin around what a plane or axis covers, mm.
pub const MIN_MARGIN: f64 = 3.0;

/// The margin around what a plane or axis covers, a share of its larger
/// side.
const MARGIN: f64 = 0.05;

/// Extents shorter than this (mm) measure nothing.
const LINEAR: f64 = 1e-6;

/// Where a construction plane or axis is drawn, in its component's
/// coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DrawnExtent {
    /// A rectangle in the plane: its centre, the unit directions of its
    /// width and height (in the plane, `y_axis` = normal × `x_axis`) and
    /// its sides, mm, the margin all round included.
    Rect {
        center: Vec3,
        x_axis: Vec3,
        y_axis: Vec3,
        width: f64,
        height: f64,
        margin: f64,
    },
    /// A piece of the axis from `start` to `end` (along its direction),
    /// the margin at each end included.
    Segment { start: Vec3, end: Vec3, margin: f64 },
}

/// What sets where a construction plane or axis is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrawnSource {
    /// The size its resize handles set (`display_size`, mitcad#1748).
    DisplaySize,
    /// What it is made from (mitcad#1827).
    Fitted,
    /// Nothing to measure: [`DEFAULT_SIZE`] about its origin.
    Default,
}

impl DrawnSource {
    /// `display_size`, `fitted` or `default`, as the `datums` query
    /// gives it.
    pub fn name(self) -> &'static str {
        match self {
            Self::DisplaySize => "display_size",
            Self::Fitted => "fitted",
            Self::Default => "default",
        }
    }
}

impl DrawnExtent {
    /// A datum drawn at a size centred on its origin along its own axes:
    /// a plane's square of side `size`, an axis's line `size` long; none
    /// for a point.
    pub fn centred(datum: &Datum, size: f64) -> Option<Self> {
        match datum {
            Datum::Plane(plane) => Some(Self::Rect {
                center: plane.origin,
                x_axis: plane.x_axis,
                y_axis: plane.y_axis,
                width: size,
                height: size,
                margin: 0.0,
            }),
            Datum::Axis(axis) => Some(Self::Segment {
                start: add(axis.origin, scale(axis.direction, -size / 2.0)),
                end: add(axis.origin, scale(axis.direction, size / 2.0)),
                margin: 0.0,
            }),
            Datum::Point(_) => None,
        }
    }

    /// The same at the size its resize handles set (mitcad#1748): a square
    /// of side `size` about the rectangle's centre and along its sides, a
    /// line `size` long about the segment's middle.
    pub fn sized(self, size: f64) -> Self {
        match self {
            Self::Rect {
                center,
                x_axis,
                y_axis,
                ..
            } => Self::Rect {
                center,
                x_axis,
                y_axis,
                width: size,
                height: size,
                margin: 0.0,
            },
            Self::Segment { start, end, .. } => {
                let middle = scale(add(start, end), 0.5);
                let along = unit(sub(end, start)).unwrap_or([1.0, 0.0, 0.0]);
                Self::Segment {
                    start: add(middle, scale(along, -size / 2.0)),
                    end: add(middle, scale(along, size / 2.0)),
                    margin: 0.0,
                }
            }
        }
    }

    /// The rectangle's corners or the segment's ends, without the margin:
    /// what it covers.
    fn covered(&self) -> Vec<Vec3> {
        match *self {
            Self::Rect {
                center,
                x_axis,
                y_axis,
                width,
                height,
                margin,
            } => {
                let x = scale(x_axis, ((width - 2.0 * margin) / 2.0).max(0.0));
                let y = scale(y_axis, ((height - 2.0 * margin) / 2.0).max(0.0));
                vec![
                    add(center, add(x, y)),
                    add(center, sub(y, x)),
                    sub(center, add(x, y)),
                    add(center, sub(x, y)),
                ]
            }
            Self::Segment { start, end, margin } => {
                let along = unit(sub(end, start)).unwrap_or([0.0; 3]);
                let shrink = margin.min(norm(sub(end, start)) / 2.0);
                vec![
                    add(start, scale(along, shrink)),
                    sub(end, scale(along, shrink)),
                ]
            }
        }
    }
}

/// What references show: points and straight edges.
#[derive(Default)]
struct Seen {
    points: Vec<Vec3>,
    lines: Vec<(Vec3, Vec3)>,
}

impl Seen {
    fn add_curve(&mut self, curve: &Curve3) {
        if let Curve3::Line { start, end } = curve {
            self.lines.push((*start, *end));
        }
        self.points.extend(crate::features::path::samples(curve));
    }

    fn add_geometry(&mut self, curve: &CurveGeometry) {
        match *curve {
            CurveGeometry::Line { start, end } => {
                self.lines.push((start, end));
                self.points.extend([start, end]);
            }
            CurveGeometry::Circle {
                center,
                normal,
                radius,
                start,
                end,
            } => {
                // The circle's box in its plane, which holds every arc.
                self.points.extend([start, end]);
                if let Some(normal) = unit(normal) {
                    let (a, b) = across(normal);
                    for v in [a, b] {
                        self.points.push(add(center, scale(v, radius)));
                        self.points.push(sub(center, scale(v, radius)));
                    }
                }
            }
            CurveGeometry::Other { start, end, .. } => self.points.extend([start, end]),
        }
    }

    fn add_extent(&mut self, extent: &DrawnExtent) {
        let corners = extent.covered();
        for (i, corner) in corners.iter().enumerate() {
            if corners.len() > 2 || i == 0 {
                self.lines.push((*corner, corners[(i + 1) % corners.len()]));
            }
        }
        self.points.extend(corners);
    }
}

/// Two unit vectors at right angles to each other and to `normal`.
fn across(normal: Vec3) -> (Vec3, Vec3) {
    let helper = if normal[0].abs() < 0.9 {
        [1.0, 0.0, 0.0]
    } else {
        [0.0, 1.0, 0.0]
    };
    let a = unit(cross(normal, helper)).unwrap_or([0.0, 1.0, 0.0]);
    (a, cross(normal, a))
}

fn margin(larger: f64) -> f64 {
    (MARGIN * larger).max(MIN_MARGIN)
}

/// Where a construction plane is drawn: covering the outlines of `faces`
/// (planar faces and construction planes it is made on or from), else a
/// square over where it cuts the bodies of all its references (`nearby`,
/// `path` too), else over the referenced geometry. None when nothing
/// measures.
pub(crate) fn plane_extent<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    plane: &DatumPlane,
    faces: &[&GeomRef],
    nearby: &[&GeomRef],
    path: Option<&PathRef>,
) -> Option<DrawnExtent> {
    let normal = plane.normal();
    let mut seen = Seen::default();
    for reference in faces {
        outline(ctx, reference, normal, &mut seen);
    }
    if let Some(rect) = rectangle(plane, &seen) {
        return Some(rect);
    }
    let references: Vec<&GeomRef> = faces.iter().chain(nearby).copied().collect();
    let mut cut = Vec::new();
    for body in bodies(&references, path) {
        if let Some(bounds) = body_box(ctx, body) {
            cut.extend(box_cut(&bounds, plane));
        }
    }
    let (x, y) = (plane.x_axis, cross(normal, plane.x_axis));
    if let Some(square) = framed(plane.origin, x, y, &cut, true) {
        return Some(square);
    }
    let mut seen = Seen::default();
    for reference in &references {
        geometry(ctx, reference, normal, &mut seen);
    }
    framed(plane.origin, x, y, &seen.points, true)
}

/// Where a construction axis is drawn: along the extent of `geometry`
/// (edges, faces, planes, points), else where it passes through the
/// boxes of the bodies of `bodies`. None when nothing measures.
pub(crate) fn axis_extent<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    axis: &DatumAxis,
    geometry_of: &[&GeomRef],
    bodies_of: &[&GeomRef],
) -> Option<DrawnExtent> {
    let direction = unit(axis.direction)?;
    let (view, _) = across(direction);
    let mut seen = Seen::default();
    for reference in geometry_of {
        geometry(ctx, reference, view, &mut seen);
    }
    let along = |p: &Vec3| dot(sub(*p, axis.origin), direction);
    let mut range = seen.points.iter().map(along).fold(None, widen);
    if !range.is_some_and(|(t0, t1)| t1 - t0 > LINEAR) {
        range = None;
        for body in bodies(bodies_of, None) {
            if let Some(bounds) = body_box(ctx, body)
                && let Some((t0, t1)) = box_through(&bounds, axis.origin, direction)
            {
                range = widen(widen(range, t0), t1);
            }
        }
    }
    let (t0, t1) = range.filter(|(t0, t1)| t1 - t0 > LINEAR)?;
    let margin = margin(t1 - t0);
    Some(DrawnExtent::Segment {
        start: add(axis.origin, scale(direction, t0 - margin)),
        end: add(axis.origin, scale(direction, t1 + margin)),
        margin,
    })
}

fn widen(range: Option<(f64, f64)>, t: f64) -> Option<(f64, f64)> {
    if !t.is_finite() {
        return range;
    }
    Some(match range {
        Some((t0, t1)) => (t0.min(t), t1.max(t)),
        None => (t, t),
    })
}

/// The outline of a face seen along `view` (its boundary, else its
/// outline as [`Kernel::face_outline`] gives it), or a construction
/// plane's or axis's extent.
fn outline<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    reference: &GeomRef,
    view: Vec3,
    seen: &mut Seen,
) {
    match reference {
        GeomRef::Face { body, face } => {
            let Ok(shape) = Resolver::shape(ctx, *body) else {
                return;
            };
            let Ok(found) = crate::features::renamed::face_for(ctx, &shape, face) else {
                return;
            };
            let curves: Vec<Curve3> = match ctx.kernel.face_boundary(&shape, &found) {
                Ok(boundary) if !boundary.is_empty() => {
                    boundary.into_iter().map(|(_, curve)| curve).collect()
                }
                _ => ctx
                    .kernel
                    .face_outline(&shape, &found, view)
                    .unwrap_or_default(),
            };
            for curve in &curves {
                seen.add_curve(curve);
            }
        }
        GeomRef::Datum(uid) => {
            if let Some(extent) = drawn_of(ctx, *uid) {
                seen.add_extent(&extent);
            }
        }
        _ => {}
    }
}

/// What a reference shows: a face's outline, an edge's curve, a point, a
/// construction plane's or axis's extent (a construction point's point).
fn geometry<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    reference: &GeomRef,
    view: Vec3,
    seen: &mut Seen,
) {
    match reference {
        GeomRef::Face { .. } => outline(ctx, reference, view, seen),
        GeomRef::Edge { .. } => {
            if let Ok(curve) = ctx.curve(reference) {
                seen.add_geometry(&curve);
            }
        }
        GeomRef::Vertex { .. } | GeomRef::SketchPoint { .. } | GeomRef::FixedPoint(_) => {
            if let Ok(point) = ctx.point(reference) {
                seen.points.push(point);
            }
        }
        GeomRef::Datum(uid) => match drawn_of(ctx, *uid) {
            Some(extent) => seen.add_extent(&extent),
            None => {
                if let Ok(Datum::Point(point)) = Resolver::datum(ctx, *uid) {
                    seen.points.push(point.point);
                }
            }
        },
        _ => {}
    }
}

/// The drawn extent of a construction plane or axis before this feature,
/// in its own component (one of another component by a link is left out).
fn drawn_of<K: Kernel>(ctx: &mut EvalContext<'_, K>, uid: FeatureUid) -> Option<DrawnExtent> {
    if ctx.datum_links.contains_key(&uid) {
        return None;
    }
    let result = ctx.env.result_of(uid);
    let output = result.and_then(|r| r.output.clone());
    ctx.reads
        .push(Read::Output(uid, output.as_ref().map(|o| o.version)));
    if result.is_some_and(|r| !r.status.is_ok()) {
        return None;
    }
    output.and_then(|o| o.drawn)
}

/// The bodies that references are on, each once.
fn bodies(references: &[&GeomRef], path: Option<&PathRef>) -> Vec<BodyUid> {
    let mut bodies = Vec::new();
    let path_body = match path {
        Some(PathRef::Edges { body, .. }) => Some(*body),
        _ => None,
    };
    for body in references
        .iter()
        .filter_map(|reference| match reference {
            GeomRef::Face { body, .. }
            | GeomRef::Edge { body, .. }
            | GeomRef::Vertex { body, .. }
            | GeomRef::Body(body) => Some(*body),
            _ => None,
        })
        .chain(path_body)
    {
        if !bodies.contains(&body) {
            bodies.push(body);
        }
    }
    bodies
}

fn body_box<K: Kernel>(ctx: &mut EvalContext<'_, K>, body: BodyUid) -> Option<BoundingBox> {
    let shape = Resolver::shape(ctx, body).ok()?;
    ctx.kernel.bounding_box(&shape).ok().flatten()
}

/// Where a plane cuts a box: the corners on it and the points where the
/// box's edges pass through it.
fn box_cut(bounds: &BoundingBox, plane: &DatumPlane) -> Vec<Vec3> {
    let normal = plane.normal();
    let corner = |i: usize| -> Vec3 {
        std::array::from_fn(|k| {
            if i & (1 << k) == 0 {
                bounds.min[k]
            } else {
                bounds.max[k]
            }
        })
    };
    let side = |p: Vec3| dot(sub(p, plane.origin), normal);
    let mut points = Vec::new();
    for i in 0..8 {
        let a = corner(i);
        let da = side(a);
        if da.abs() <= LINEAR {
            points.push(a);
        }
        for bit in [1, 2, 4] {
            if i & bit != 0 {
                continue;
            }
            let b = corner(i | bit);
            let db = side(b);
            if da * db < 0.0 && da.abs() > LINEAR && db.abs() > LINEAR {
                points.push(add(a, scale(sub(b, a), da / (da - db))));
            }
        }
    }
    points
}

/// Where the line through `origin` along `direction` (a unit vector) is
/// inside a box: its parameters along the line.
fn box_through(bounds: &BoundingBox, origin: Vec3, direction: Vec3) -> Option<(f64, f64)> {
    let (mut t0, mut t1) = (f64::NEG_INFINITY, f64::INFINITY);
    for k in 0..3 {
        let (o, d) = (origin[k], direction[k]);
        if d.abs() < 1e-12 {
            if o < bounds.min[k] - LINEAR || o > bounds.max[k] + LINEAR {
                return None;
            }
        } else {
            let (a, b) = ((bounds.min[k] - o) / d, (bounds.max[k] - o) / d);
            t0 = t0.max(a.min(b));
            t1 = t1.min(a.max(b));
        }
    }
    (t0.is_finite() && t1.is_finite() && t1 - t0 > LINEAR).then_some((t0, t1))
}

/// A rectangle in the plane over what was seen, along its longest
/// straight edge where it has one (seen in the plane), else along the
/// plane's x axis.
fn rectangle(plane: &DatumPlane, seen: &Seen) -> Option<DrawnExtent> {
    let normal = plane.normal();
    let in_plane = |v: Vec3| sub(v, scale(normal, dot(v, normal)));
    let longest = seen
        .lines
        .iter()
        .map(|(a, b)| in_plane(sub(*b, *a)))
        .filter(|d| norm(*d) > LINEAR)
        .max_by(|a, b| norm(*a).total_cmp(&norm(*b)))
        .and_then(unit);
    // Of the two ways along the edge, the one nearer the plane's x axis
    // (its y axis when the edge is square to x).
    let x = match longest {
        Some(d) => {
            let along_x = dot(d, plane.x_axis);
            if along_x < -LINEAR || (along_x.abs() <= LINEAR && dot(d, plane.y_axis) < 0.0) {
                scale(d, -1.0)
            } else {
                d
            }
        }
        None => plane.x_axis,
    };
    framed(plane.origin, x, cross(normal, x), &seen.points, false)
}

/// The rectangle along `x` and `y` (unit, at right angles) over the
/// points seen in their plane, with the margin; a square of its larger
/// side with `square`. None when the points span nothing.
fn framed(origin: Vec3, x: Vec3, y: Vec3, points: &[Vec3], square: bool) -> Option<DrawnExtent> {
    let mut low = [f64::INFINITY; 2];
    let mut high = [f64::NEG_INFINITY; 2];
    for point in points {
        let d = sub(*point, origin);
        for (k, value) in [dot(d, x), dot(d, y)].into_iter().enumerate() {
            if value.is_finite() {
                low[k] = low[k].min(value);
                high[k] = high[k].max(value);
            }
        }
    }
    let size = [high[0] - low[0], high[1] - low[1]];
    if !size.iter().all(|s| s.is_finite()) {
        return None;
    }
    let larger = size[0].max(size[1]);
    if larger <= LINEAR {
        return None;
    }
    let margin = margin(larger);
    let (width, height) = if square {
        (larger, larger)
    } else {
        (size[0], size[1])
    };
    let center = add(
        origin,
        add(
            scale(x, (low[0] + high[0]) / 2.0),
            scale(y, (low[1] + high[1]) / 2.0),
        ),
    );
    Some(DrawnExtent::Rect {
        center,
        x_axis: x,
        y_axis: y,
        width: width + 2.0 * margin,
        height: height + 2.0 * margin,
        margin,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plane_xy(z: f64) -> DatumPlane {
        DatumPlane {
            origin: [0.0, 0.0, z],
            x_axis: [1.0, 0.0, 0.0],
            y_axis: [0.0, 1.0, 0.0],
        }
    }

    #[test]
    fn a_rectangle_runs_along_the_longest_straight_edge_with_its_margin() {
        // A 100 × 10 face turned 90° in XY, seen from 5 mm above.
        let mut seen = Seen::default();
        let corners = [
            [0.0, 0.0, 0.0],
            [0.0, 100.0, 0.0],
            [-10.0, 100.0, 0.0],
            [-10.0, 0.0, 0.0],
        ];
        for (i, start) in corners.iter().enumerate() {
            seen.add_curve(&Curve3::Line {
                start: *start,
                end: corners[(i + 1) % corners.len()],
            });
        }
        let Some(DrawnExtent::Rect {
            center,
            x_axis,
            width,
            height,
            margin,
            ..
        }) = rectangle(&plane_xy(5.0), &seen)
        else {
            panic!("no rectangle");
        };
        // Along Y (square to the plane's x axis: its y axis's way).
        assert_eq!(x_axis, [0.0, 1.0, 0.0]);
        assert!((margin - 5.0).abs() < 1e-12, "{margin}");
        assert!((width - 110.0).abs() < 1e-9, "{width}");
        assert!((height - 20.0).abs() < 1e-9, "{height}");
        assert!(norm(sub(center, [-5.0, 50.0, 5.0])) < 1e-9, "{center:?}");
    }

    #[test]
    fn a_small_extent_takes_the_smallest_margin_and_nothing_spans_nothing() {
        let points = [[0.0, 0.0, 0.0], [2.0, 1.0, 0.0]];
        let Some(DrawnExtent::Rect { width, height, .. }) =
            framed([0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], &points, true)
        else {
            panic!("no square");
        };
        assert!((width - (2.0 + 2.0 * MIN_MARGIN)).abs() < 1e-12, "{width}");
        assert!((height - width).abs() < 1e-12, "{height}");
        assert_eq!(
            framed(
                [0.0; 3],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                &[[1.0; 3]],
                true
            ),
            None
        );
    }

    #[test]
    fn a_plane_cuts_a_box_where_its_edges_cross_it() {
        let bounds = BoundingBox {
            min: [0.0, 0.0, 0.0],
            max: [40.0, 20.0, 10.0],
        };
        let cut = box_cut(&bounds, &plane_xy(5.0));
        assert_eq!(cut.len(), 4, "{cut:?}");
        assert!(cut.iter().all(|p| (p[2] - 5.0).abs() < 1e-12));
        assert!(box_cut(&bounds, &plane_xy(15.0)).is_empty());
        // A line through the box along X.
        let through = box_through(&bounds, [-5.0, 10.0, 5.0], [1.0, 0.0, 0.0]);
        assert_eq!(through, Some((5.0, 45.0)));
        assert_eq!(
            box_through(&bounds, [-5.0, 30.0, 5.0], [1.0, 0.0, 0.0]),
            None
        );
    }

    #[test]
    fn a_stored_size_keeps_the_centre_and_sides_and_an_extent_covers_without_its_margin() {
        let rect = DrawnExtent::Rect {
            center: [1.0, 2.0, 3.0],
            x_axis: [0.0, 1.0, 0.0],
            y_axis: [-1.0, 0.0, 0.0],
            width: 110.0,
            height: 20.0,
            margin: 5.0,
        };
        assert_eq!(
            rect.sized(40.0),
            DrawnExtent::Rect {
                center: [1.0, 2.0, 3.0],
                x_axis: [0.0, 1.0, 0.0],
                y_axis: [-1.0, 0.0, 0.0],
                width: 40.0,
                height: 40.0,
                margin: 0.0,
            }
        );
        let covered = rect.covered();
        assert!(
            norm(sub(covered[0], [-4.0, 52.0, 3.0])) < 1e-12,
            "{covered:?}"
        );
        let segment = DrawnExtent::Segment {
            start: [0.0, 0.0, 0.0],
            end: [0.0, 0.0, 20.0],
            margin: 3.0,
        };
        assert_eq!(
            segment.sized(10.0),
            DrawnExtent::Segment {
                start: [0.0, 0.0, 5.0],
                end: [0.0, 0.0, 15.0],
                margin: 0.0,
            }
        );
        assert_eq!(segment.covered(), vec![[0.0, 0.0, 3.0], [0.0, 0.0, 17.0]]);
    }
}
