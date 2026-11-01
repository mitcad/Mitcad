// SPDX-License-Identifier: MIT
//! A view's lines cut in its own plane (mitcad#707): a detail view keeps
//! what lies inside its circle, a break takes bands out and moves the rest
//! together. Both work on what the kernel projected, so they need no
//! geometry library: lines and polylines are cut where they cross, arcs
//! where they cross the circle or, across a break, as polylines; the loops
//! of cut faces are clipped as polygons (Sutherland and Hodgman).
//!
//! [`ViewMap`] puts view coordinates on the sheet: at its scale, its
//! anchor at its place, with its breaks closed up.

use std::f64::consts::TAU;

use super::{CutFace, P2, ViewCurve, ViewEdge, ViewProjection, ViewSpline};

/// A band taken out of a view: from `from` to `to` along `axis` (0: x,
/// 1: y) of the view, in view millimetres, closed up to `gap` millimetres
/// of paper.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Band {
    pub axis: usize,
    pub from: f64,
    pub to: f64,
    pub gap: f64,
}

/// Where a view's coordinates go on the sheet.
#[derive(Debug, Clone, PartialEq)]
pub struct ViewMap {
    /// Sheet millimetres per view millimetre.
    pub scale: f64,
    /// The view point that lies at `place`.
    pub anchor: P2,
    pub place: P2,
    pub breaks: Vec<Band>,
}

impl ViewMap {
    pub fn new(scale: f64, anchor: P2, place: P2) -> Self {
        Self {
            scale,
            anchor,
            place,
            breaks: Vec::new(),
        }
    }

    /// A coordinate along an axis with the breaks closed: what lies beyond
    /// a band moves back by its length less its gap; inside a band it is
    /// squeezed into the gap.
    fn closed(&self, axis: usize, v: f64) -> f64 {
        let mut out = v;
        for b in self.breaks.iter().filter(|b| b.axis == axis) {
            let length = b.to - b.from;
            let gap = b.gap / self.scale;
            if v >= b.to {
                out -= length - gap;
            } else if v > b.from {
                out -= (v - b.from) * (1.0 - gap / length);
            }
        }
        out
    }

    /// The inverse of [`ViewMap::closed`].
    fn opened(&self, axis: usize, v: f64) -> f64 {
        let mut bands: Vec<&Band> = self.breaks.iter().filter(|b| b.axis == axis).collect();
        bands.sort_by(|a, b| a.from.total_cmp(&b.from));
        let mut shift = 0.0;
        for b in bands {
            let length = b.to - b.from;
            let gap = b.gap / self.scale;
            // Where the band starts and ends with the breaks before it
            // closed.
            let start = b.from - shift;
            if v <= start {
                break;
            }
            if v < start + gap {
                return b.from + (v - start) * if gap > 0.0 { length / gap } else { 0.0 };
            }
            shift += length - gap;
        }
        v + shift
    }

    /// Whether a view point lies inside a band a break takes out.
    pub fn in_break(&self, p: P2) -> bool {
        self.breaks
            .iter()
            .any(|b| p[b.axis] > b.from && p[b.axis] < b.to)
    }

    /// The sheet point of a view point.
    pub fn at(&self, p: P2) -> P2 {
        if self.breaks.is_empty() {
            return [
                self.place[0] + self.scale * (p[0] - self.anchor[0]),
                self.place[1] + self.scale * (p[1] - self.anchor[1]),
            ];
        }
        std::array::from_fn(|k| {
            self.place[k] + self.scale * (self.closed(k, p[k]) - self.closed(k, self.anchor[k]))
        })
    }

    /// The view point of a sheet point.
    pub fn view_point(&self, s: P2) -> P2 {
        std::array::from_fn(|k| {
            let closed = (s[k] - self.place[k]) / self.scale + self.closed(k, self.anchor[k]);
            self.opened(k, closed)
        })
    }
}

/// The largest distance of a polyline from an arc it stands for, mm.
const DEFLECTION: f64 = 0.01;

/// An arc as a polyline.
fn arc_points(center: P2, radius: f64, start: f64, end: f64) -> Vec<P2> {
    let sweep = end - start;
    let step = if radius > DEFLECTION {
        2.0 * (1.0 - DEFLECTION / radius).acos()
    } else {
        sweep
    };
    let n = ((sweep / step.max(1e-3)).ceil() as usize).clamp(2, 720);
    (0..=n)
        .map(|i| {
            let a = start + sweep * i as f64 / n as f64;
            [center[0] + radius * a.cos(), center[1] + radius * a.sin()]
        })
        .collect()
}

/// The runs of a polyline that `kept` keeps: each segment is split where
/// it crosses the boundary (`crossings` gives the parameters in 0..1 along
/// it), and a piece stays when its middle is kept.
fn cut_polyline(
    points: &[P2],
    crossings: &dyn Fn(P2, P2) -> Vec<f64>,
    kept: &dyn Fn(P2) -> bool,
) -> Vec<Vec<P2>> {
    let mut out = Vec::new();
    let mut run: Vec<P2> = Vec::new();
    for pair in points.windows(2) {
        let (p, q) = (pair[0], pair[1]);
        let mut ts: Vec<f64> = crossings(p, q)
            .into_iter()
            .filter(|t| *t > 1e-12 && *t < 1.0 - 1e-12)
            .collect();
        ts.sort_by(f64::total_cmp);
        let mut cuts = vec![p];
        cuts.extend(
            ts.iter()
                .map(|t| [p[0] + (q[0] - p[0]) * t, p[1] + (q[1] - p[1]) * t]),
        );
        cuts.push(q);
        for piece in cuts.windows(2) {
            let (a, b) = (piece[0], piece[1]);
            if kept([(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0]) {
                if run.is_empty() {
                    run.push(a);
                }
                run.push(b);
            } else if !run.is_empty() {
                out.push(std::mem::take(&mut run));
            }
        }
    }
    if run.len() > 1 {
        out.push(run);
    }
    out
}

/// Where the segment from `p` to `q` crosses a circle (parameters along
/// it).
fn circle_crossings(center: P2, radius: f64, p: P2, q: P2) -> Vec<f64> {
    let d = [q[0] - p[0], q[1] - p[1]];
    let f = [p[0] - center[0], p[1] - center[1]];
    let a = d[0] * d[0] + d[1] * d[1];
    let b = 2.0 * (f[0] * d[0] + f[1] * d[1]);
    let c = f[0] * f[0] + f[1] * f[1] - radius * radius;
    let disc = b * b - 4.0 * a * c;
    if a <= 0.0 || disc < 0.0 {
        return Vec::new();
    }
    let root = disc.sqrt();
    vec![(-b - root) / (2.0 * a), (-b + root) / (2.0 * a)]
}

/// The pieces of a projection inside a circle (a detail view): edges cut
/// where they leave it, arcs split where they cross it, cut faces clipped.
pub fn inside_circle(projection: &ViewProjection, center: P2, radius: f64) -> ViewProjection {
    let inside = |p: P2| {
        let (dx, dy) = (p[0] - center[0], p[1] - center[1]);
        dx * dx + dy * dy <= radius * radius
    };
    let crossings = |p: P2, q: P2| circle_crossings(center, radius, p, q);
    let mut edges = Vec::new();
    for edge in &projection.edges {
        let piece = |curve: ViewCurve| ViewEdge {
            visible: edge.visible,
            kind: edge.kind,
            curve,
            source: edge.source,
        };
        match &edge.curve {
            ViewCurve::Line(a, b) => {
                for p in cut_polyline(&[*a, *b], &crossings, &inside) {
                    edges.push(piece(ViewCurve::Line(p[0], p[p.len() - 1])));
                }
            }
            // A B-spline is cut as its polyline (mitcad#853).
            ViewCurve::Polyline(points) | ViewCurve::Spline(ViewSpline { points, .. }) => {
                for p in cut_polyline(points, &crossings, &inside) {
                    edges.push(piece(ViewCurve::Polyline(p)));
                }
            }
            ViewCurve::Arc {
                center: c,
                radius: r,
                start,
                end,
            } => {
                for (s, e) in arc_inside_circle(*c, *r, *start, *end, center, radius) {
                    edges.push(piece(ViewCurve::Arc {
                        center: *c,
                        radius: *r,
                        start: s,
                        end: e,
                    }));
                }
            }
        }
    }
    let boundary: Vec<P2> = (0..96)
        .map(|i| {
            let a = TAU * f64::from(i) / 96.0;
            [center[0] + radius * a.cos(), center[1] + radius * a.sin()]
        })
        .collect();
    let cuts = projection
        .cuts
        .iter()
        .map(|face| {
            face.iter()
                .map(|lp| clip_convex(lp, &boundary))
                .filter(|lp| lp.len() > 2)
                .collect::<Vec<_>>()
        })
        .filter(|face: &CutFace| !face.is_empty())
        .collect();
    ViewProjection {
        edges,
        cuts,
        polygonal: projection.polygonal,
    }
}

/// The ranges of an arc (counter-clockwise from `start` to `end`) inside a
/// circle.
fn arc_inside_circle(
    c: P2,
    r: f64,
    start: f64,
    end: f64,
    center: P2,
    radius: f64,
) -> Vec<(f64, f64)> {
    let dx = center[0] - c[0];
    let dy = center[1] - c[1];
    let d = (dx * dx + dy * dy).sqrt();
    let point = |a: f64| [c[0] + r * a.cos(), c[1] + r * a.sin()];
    let inside = |a: f64| {
        let p = point(a);
        let (ex, ey) = (p[0] - center[0], p[1] - center[1]);
        ex * ex + ey * ey <= radius * radius
    };
    // The angles where the arc's circle crosses the detail's.
    let mut cuts = vec![start, end];
    if d > 1e-12 && d < r + radius && d > (r - radius).abs() {
        let a = (r * r + d * d - radius * radius) / (2.0 * r * d);
        let base = dy.atan2(dx);
        let half = a.clamp(-1.0, 1.0).acos();
        for angle in [base - half, base + half] {
            let t = start + (angle - start).rem_euclid(TAU);
            if t > start && t < end {
                cuts.push(t);
            }
        }
    }
    cuts.sort_by(f64::total_cmp);
    let mut out: Vec<(f64, f64)> = Vec::new();
    for pair in cuts.windows(2) {
        let (s, e) = (pair[0], pair[1]);
        if e - s < 1e-12 || !inside((s + e) / 2.0) {
            continue;
        }
        match out.last_mut() {
            Some(last) if (last.1 - s).abs() < 1e-12 => last.1 = e,
            _ => out.push((s, e)),
        }
    }
    out
}

/// A polygon clipped to a convex one (counter-clockwise), Sutherland and
/// Hodgman.
fn clip_convex(polygon: &[P2], convex: &[P2]) -> Vec<P2> {
    let mut out: Vec<P2> = polygon.to_vec();
    for i in 0..convex.len() {
        if out.is_empty() {
            break;
        }
        let a = convex[i];
        let b = convex[(i + 1) % convex.len()];
        let side = |p: P2| (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0]);
        out = clip_half(&out, &side);
    }
    out
}

/// A polygon clipped to where `side` is not negative.
fn clip_half(polygon: &[P2], side: &dyn Fn(P2) -> f64) -> Vec<P2> {
    let mut out = Vec::new();
    for i in 0..polygon.len() {
        let p = polygon[i];
        let q = polygon[(i + 1) % polygon.len()];
        let (sp, sq) = (side(p), side(q));
        if sp >= 0.0 {
            out.push(p);
        }
        if (sp >= 0.0) != (sq >= 0.0) {
            let t = sp / (sp - sq);
            out.push([p[0] + (q[0] - p[0]) * t, p[1] + (q[1] - p[1]) * t]);
        }
    }
    out
}

/// Filled polygons (a shaded view's faces, with their greys) inside a
/// detail's circle.
pub fn polygons_inside_circle(
    polygons: &[(Vec<P2>, f64)],
    center: P2,
    radius: f64,
) -> Vec<(Vec<P2>, f64)> {
    let boundary: Vec<P2> = (0..96)
        .map(|i| {
            let a = TAU * f64::from(i) / 96.0;
            [center[0] + radius * a.cos(), center[1] + radius * a.sin()]
        })
        .collect();
    polygons
        .iter()
        .map(|(p, grey)| (clip_convex(p, &boundary), *grey))
        .filter(|(p, _)| p.len() > 2)
        .collect()
}

/// Filled polygons with the breaks' bands taken out: each clipped to each
/// part of the view.
pub fn polygons_without_bands(polygons: &[(Vec<P2>, f64)], breaks: &[Band]) -> Vec<(Vec<P2>, f64)> {
    if breaks.is_empty() {
        return polygons.to_vec();
    }
    let mut out = Vec::new();
    for cell in cells(breaks) {
        for (p, grey) in polygons {
            let clipped = clip_cell(p, cell);
            if clipped.len() > 2 {
                out.push((clipped, *grey));
            }
        }
    }
    out
}

/// A rectangle of the view's plane between breaks: its range of x and of
/// y (infinite at the open sides).
type Cell = ((f64, f64), (f64, f64));

fn cells(breaks: &[Band]) -> Vec<Cell> {
    let xs = kept_ranges(breaks, 0);
    let ys = kept_ranges(breaks, 1);
    xs.iter()
        .flat_map(|x| ys.iter().map(move |y| (*x, *y)))
        .collect()
}

/// A polygon clipped to a cell.
fn clip_cell(polygon: &[P2], cell: Cell) -> Vec<P2> {
    let mut out = polygon.to_vec();
    let ((x0, x1), (y0, y1)) = cell;
    if x0.is_finite() {
        out = clip_half(&out, &|p: P2| p[0] - x0);
    }
    if x1.is_finite() {
        out = clip_half(&out, &|p: P2| x1 - p[0]);
    }
    if y0.is_finite() {
        out = clip_half(&out, &|p: P2| p[1] - y0);
    }
    if y1.is_finite() {
        out = clip_half(&out, &|p: P2| y1 - p[1]);
    }
    out
}

/// The parts of a view that stay beside its breaks: each kept band of
/// the view's plane (between the breaks along an axis) as a range of x and
/// one of y.
fn kept_ranges(breaks: &[Band], axis: usize) -> Vec<(f64, f64)> {
    let mut bands: Vec<&Band> = breaks.iter().filter(|b| b.axis == axis).collect();
    bands.sort_by(|a, b| a.from.total_cmp(&b.from));
    let mut out = Vec::new();
    let mut low = f64::NEG_INFINITY;
    for b in bands {
        out.push((low, b.from));
        low = b.to;
    }
    out.push((low, f64::INFINITY));
    out
}

/// A projection with the breaks' bands taken out: edges cut at the
/// bands, arcs that would lie in two parts (or whose centre lies in
/// another part) as polylines, the cut faces' loops clipped to each part.
/// The parts stay where they are; [`ViewMap::at`] moves them together.
pub fn without_bands(projection: &ViewProjection, breaks: &[Band]) -> ViewProjection {
    if breaks.is_empty() {
        return projection.clone();
    }
    let cells = cells(breaks);
    let in_cell = |cell: &Cell, p: P2| {
        p[0] >= cell.0.0 && p[0] <= cell.0.1 && p[1] >= cell.1.0 && p[1] <= cell.1.1
    };
    let kept = |p: P2| cells.iter().any(|c| in_cell(c, p));
    // Where a segment crosses the bands' edges.
    let crossings = |p: P2, q: P2| -> Vec<f64> {
        let mut ts = Vec::new();
        for band in breaks {
            let k = band.axis;
            let d = q[k] - p[k];
            if d.abs() > 1e-12 {
                ts.push((band.from - p[k]) / d);
                ts.push((band.to - p[k]) / d);
            }
        }
        ts
    };
    let mut edges = Vec::new();
    for edge in &projection.edges {
        let piece = |curve: ViewCurve| ViewEdge {
            visible: edge.visible,
            kind: edge.kind,
            curve,
            source: edge.source,
        };
        let points = match &edge.curve {
            ViewCurve::Line(a, b) => {
                // A line through several parts is cut at each band; the
                // parts are apart, so each run is one straight piece.
                for run in cut_polyline(&[*a, *b], &crossings, &kept) {
                    edges.push(piece(ViewCurve::Line(run[0], run[run.len() - 1])));
                }
                continue;
            }
            ViewCurve::Arc {
                center,
                radius,
                start,
                end,
            } => {
                let points = arc_points(*center, *radius, *start, *end);
                let cell = cells.iter().find(|c| in_cell(c, *center));
                if let Some(cell) = cell
                    && points.iter().all(|p| in_cell(cell, *p))
                {
                    edges.push(edge.clone());
                    continue;
                }
                points
            }
            ViewCurve::Polyline(points) | ViewCurve::Spline(ViewSpline { points, .. }) => {
                points.clone()
            }
        };
        // Every way from one part to another crosses a band, so the runs
        // each lie in one part.
        for run in cut_polyline(&points, &crossings, &kept) {
            edges.push(piece(ViewCurve::Polyline(run)));
        }
    }
    let mut cuts: Vec<CutFace> = Vec::new();
    for face in &projection.cuts {
        for cell in &cells {
            let loops: Vec<Vec<P2>> = face
                .iter()
                .map(|lp| clip_cell(lp, *cell))
                .filter(|lp| lp.len() > 2)
                .collect();
            if !loops.is_empty() {
                cuts.push(loops);
            }
        }
    }
    ViewProjection {
        edges,
        cuts,
        polygonal: projection.polygonal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ViewEdgeKind;

    fn line(a: P2, b: P2) -> ViewEdge {
        ViewEdge {
            visible: true,
            kind: ViewEdgeKind::Sharp,
            curve: ViewCurve::Line(a, b),
            source: None,
        }
    }

    #[test]
    fn a_map_puts_the_anchor_at_its_place_and_closes_breaks() {
        let mut map = ViewMap::new(2.0, [0.0, 0.0], [100.0, 50.0]);
        assert_eq!(map.at([0.0, 0.0]), [100.0, 50.0]);
        assert_eq!(map.at([10.0, -5.0]), [120.0, 40.0]);
        map.breaks.push(Band {
            axis: 0,
            from: 20.0,
            to: 80.0,
            gap: 6.0,
        });
        // Before the band nothing moves; after it, 60 mm close to 3 mm.
        assert_eq!(map.at([10.0, 0.0]), [120.0, 50.0]);
        let beyond = map.at([100.0, 0.0]);
        assert!(
            (beyond[0] - (100.0 + 2.0 * 43.0)).abs() < 1e-9,
            "{beyond:?}"
        );
        for p in [[10.0, 3.0], [50.0, 0.0], [100.0, -2.0]] {
            let back = map.view_point(map.at(p));
            assert!((back[0] - p[0]).abs() < 1e-9 && (back[1] - p[1]).abs() < 1e-9);
        }
    }

    #[test]
    fn a_detail_keeps_what_lies_in_its_circle() {
        let projection = ViewProjection {
            edges: vec![
                line([-10.0, 0.0], [10.0, 0.0]),
                line([20.0, 20.0], [30.0, 20.0]),
                ViewEdge {
                    visible: true,
                    kind: ViewEdgeKind::Sharp,
                    curve: ViewCurve::Arc {
                        center: [0.0, 0.0],
                        radius: 3.0,
                        start: 0.0,
                        end: TAU,
                    },
                    source: None,
                },
            ],
            cuts: vec![vec![vec![
                [-10.0, -10.0],
                [10.0, -10.0],
                [10.0, 10.0],
                [-10.0, 10.0],
            ]]],
            polygonal: false,
        };
        let detail = inside_circle(&projection, [0.0, 0.0], 5.0);
        assert_eq!(detail.edges.len(), 2);
        match &detail.edges[0].curve {
            ViewCurve::Line(a, b) => {
                assert!((a[0] + 5.0).abs() < 1e-9 && (b[0] - 5.0).abs() < 1e-9);
            }
            other => panic!("{other:?}"),
        }
        assert!(matches!(detail.edges[1].curve, ViewCurve::Arc { radius, .. } if radius == 3.0));
        // The square's cut face is the circle's polygon.
        assert_eq!(detail.cuts.len(), 1);
        assert!(
            detail.cuts[0][0]
                .iter()
                .all(|p| (p[0].hypot(p[1]) - 5.0).abs() < 1e-6)
        );
        // An arc crossing the circle keeps its inner part.
        let ranges = arc_inside_circle([5.0, 0.0], 5.0, 0.0, TAU, [0.0, 0.0], 5.0);
        assert_eq!(ranges.len(), 1);
        let (s, e) = ranges[0];
        assert!((e - s - TAU / 3.0).abs() < 1e-9, "{s} {e}");
    }

    #[test]
    fn a_break_takes_its_band_out() {
        let projection = ViewProjection {
            edges: vec![
                line([0.0, 0.0], [100.0, 0.0]),
                line([50.0, -5.0], [50.0, 5.0]),
                ViewEdge {
                    visible: true,
                    kind: ViewEdgeKind::Sharp,
                    curve: ViewCurve::Arc {
                        center: [90.0, 0.0],
                        radius: 2.0,
                        start: 0.0,
                        end: TAU,
                    },
                    source: None,
                },
            ],
            cuts: vec![vec![vec![
                [0.0, -5.0],
                [100.0, -5.0],
                [100.0, 5.0],
                [0.0, 5.0],
            ]]],
            polygonal: false,
        };
        let broken = without_bands(
            &projection,
            &[Band {
                axis: 0,
                from: 20.0,
                to: 80.0,
                gap: 6.0,
            }],
        );
        // The long line in two, the line inside the band gone, the circle
        // beyond it whole.
        let lines: Vec<_> = broken
            .edges
            .iter()
            .filter_map(|e| match e.curve {
                ViewCurve::Line(a, b) => Some((a, b)),
                _ => None,
            })
            .collect();
        assert_eq!(lines.len(), 2, "{lines:?}");
        for ((a, b), (ea, eb)) in lines.iter().zip([(0.0, 20.0), (80.0, 100.0)]) {
            assert!(
                (a[0] - ea).abs() < 1e-9 && (b[0] - eb).abs() < 1e-9,
                "{lines:?}"
            );
        }
        assert!(
            broken
                .edges
                .iter()
                .any(|e| matches!(e.curve, ViewCurve::Arc { center, .. } if center == [90.0, 0.0]))
        );
        assert_eq!(broken.cuts.len(), 2);
    }
}
