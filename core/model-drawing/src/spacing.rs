// SPDX-License-Identifier: MIT
//! Dimension spacing (mitcad#1173): a dimension placed or dragged on a
//! view snaps to the spacing of the dimension placed before it in its
//! drawing, and to multiples of it, so that stacked dimensions lie evenly
//! apart without a grid or a setting.
//!
//! What is spaced depends on the dimension ([`Family`]), in paper mm:
//! - a linear or an ordinate dimension's line, and the line of a diameter
//!   or a radius of a round face seen from the side: its distance beyond
//!   its view's box, on the side it lies, counted from the box (one,
//!   two, three spacings out);
//! - an angle's arc: its radius from the corner;
//! - a radius' or a diameter's text on a leader: its reach beyond the
//!   circle.
//!
//! A dimension's spacing ([`spacing`]) is how far it lies from the one
//! stacked inside it on the same side (or from the box, the corner or the
//! circle when there is none): the third of three lines placed evenly
//! says the step between them, not its whole distance.
//!
//! A place snaps when it lies within [`SPACING_REACH`] (and half a
//! spacing) of a multiple; otherwise it stays where it was clicked. The
//! caller finds the dimension placed before and its spacing
//! (`model-api/src/drawing_spacing.rs`, the `drawing_snap` query).

use mitcad_vec3::vec2::perp;
use mitcad_vec3::vec2::{add as add2, dot as dot2, norm as len2, scale as mul2, sub as sub2};

use super::P2;
use super::annotate::{
    AnnotationType, Frame, Seen, ViewAnnotation, corner, linear_axes, mid, moved, ordinate_axes,
    placed_at, unit2,
};
use super::compose::Bounds;

/// How near a multiple of the spacing a place snaps to it, paper mm (as
/// near as a pick, [`super::annotate::PICK_REACH`], and a little more).
pub const SPACING_REACH: f64 = 4.0;
/// The most spacings out a place snaps to.
const MAX_MULTIPLE: f64 = 1000.0;
/// Distances this near count as the same, paper mm.
const SAME: f64 = 1e-6;

/// What a dimension's spacing measures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    /// A dimension line beyond its view's box.
    Line,
    /// An angle's arc about its corner.
    Arc,
    /// A radius' or a diameter's text on a leader beyond its circle.
    Leader,
}

impl Family {
    pub fn key(self) -> &'static str {
        match self {
            Self::Line => "line",
            Self::Arc => "arc",
            Self::Leader => "leader",
        }
    }
}

/// Where a placed dimension lies, as its spacing measures it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Gauge {
    pub family: Family,
    /// Its distance beyond the box, its arc's radius or its leader's reach
    /// beyond the circle, paper mm.
    pub value: f64,
    /// The sheet direction a drag moves it along to grow `value` one for
    /// one: away from the box, along the angle's middle, along the leader.
    pub grow: P2,
    /// The corner of an angle or the centre of a circle on the sheet.
    pub center: Option<P2>,
}

/// Where a dimension lies, as its spacing measures it; None for what is no
/// dimension, one not placed by hand (no `line_offset`, a leader without a
/// `text_offset`), a line inside its view's box, or picks that make none.
pub fn gauge(annotation: &ViewAnnotation, seen: &[Seen], frame: &Frame<'_>) -> Option<Gauge> {
    let at = |p: P2| frame.map.at(p);
    match annotation.kind {
        AnnotationType::Linear => {
            let offset = annotation.line_offset?;
            if seen.len() < 2 {
                return None;
            }
            let (_, d) = linear_axes(annotation.along, annotation.turn, seen).ok()?;
            let n = perp(d);
            let middle = mid(at(seen[0].point()), at(seen[1].point()));
            beyond(dot2(middle, n) + offset, n, &frame.view_box)
        }
        AnnotationType::Ordinate => {
            let level = annotation.line_offset?;
            let origin = at(seen.first()?.point());
            let (_, n) = ordinate_axes(annotation);
            beyond(dot2(origin, n) + level, n, &frame.view_box)
        }
        AnnotationType::Angle => {
            let radius = annotation.line_offset?;
            if seen.len() < 2 {
                return None;
            }
            let (c, r1, r2) = corner(seen).ok()?;
            Some(Gauge {
                family: Family::Arc,
                value: radius,
                grow: unit2(add2(r1, r2)),
                center: Some(at(c)),
            })
        }
        AnnotationType::Radius | AnnotationType::Diameter => match *seen.first()? {
            Seen::Circle {
                center,
                radius,
                end_on: true,
                ..
            } => {
                let text = annotation.text_offset?;
                let reach = len2(text) - radius * frame.map.scale;
                (reach > SAME).then_some(Gauge {
                    family: Family::Leader,
                    value: reach,
                    grow: unit2(text),
                    center: Some(at(center)),
                })
            }
            Seen::Circle {
                center,
                radius,
                across,
                ..
            } => side_line(annotation, frame, center, radius, across),
            Seen::Axis { a, b, radius } => side_line(
                annotation,
                frame,
                mid(a, b),
                radius,
                perp(unit2(sub2(b, a))),
            ),
            _ => None,
        },
        _ => None,
    }
}

/// A diameter's or a radius' line across a round face seen from the side,
/// as `annotate::placed_at` places it.
fn side_line(
    annotation: &ViewAnnotation,
    frame: &Frame<'_>,
    center: P2,
    radius: f64,
    across: P2,
) -> Option<Gauge> {
    let offset = annotation.line_offset?;
    let n = perp(across);
    let middle = mid(
        frame.map.at(sub2(center, mul2(across, radius))),
        frame.map.at(add2(center, mul2(across, radius))),
    );
    beyond(dot2(middle, n) + offset, n, &frame.view_box)
}

/// A line at `position` across the sheet (along `n`) beyond a view's box:
/// how far beyond its nearer side, and the way out of the box.
fn beyond(position: f64, n: P2, view_box: &Bounds) -> Option<Gauge> {
    if view_box.is_empty() {
        return None;
    }
    let corners = [
        view_box.min,
        [view_box.max[0], view_box.min[1]],
        view_box.max,
        [view_box.min[0], view_box.max[1]],
    ];
    let (low, high) = corners
        .iter()
        .map(|c| dot2(*c, n))
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| {
            (lo.min(v), hi.max(v))
        });
    let (value, grow) = if position > high {
        (position - high, n)
    } else if position < low {
        (low - position, mul2(n, -1.0))
    } else {
        return None;
    };
    (value > SAME).then_some(Gauge {
        family: Family::Line,
        value,
        grow,
        center: None,
    })
}

/// Whether two dimensions are stacked: lines beyond the same side of
/// their view's box (the caller gives them of one view), arcs and leaders
/// about the same point.
fn stacked(a: &Gauge, b: &Gauge) -> bool {
    a.family == b.family
        && match (a.center, b.center) {
            (Some(p), Some(q)) => len2(sub2(p, q)) < SAME,
            (None, None) => dot2(a.grow, b.grow) > 1.0 - 1e-9,
            _ => false,
        }
}

/// The spacing a placed dimension shows: its distance from the nearest of
/// `others` (of its view) stacked inside it, else its whole distance.
pub fn spacing(own: &Gauge, others: &[Gauge]) -> f64 {
    let inside = others
        .iter()
        .filter(|o| stacked(own, o) && o.value < own.value - SAME)
        .map(|o| o.value)
        .fold(0.0, f64::max);
    own.value - inside
}

/// The multiple of `spacing` a distance snaps to and how many spacings
/// that is; None when it lies farther than [`SPACING_REACH`] (or half a
/// spacing) from every multiple, or there is no spacing.
pub fn snapped_value(value: f64, spacing: f64) -> Option<(f64, u32)> {
    if spacing.is_nan() || spacing <= SAME || value.is_nan() || value <= 0.0 {
        return None;
    }
    let k = (value / spacing).round().max(1.0);
    if k > MAX_MULTIPLE {
        return None;
    }
    let target = k * spacing;
    let reach = SPACING_REACH.min(spacing / 2.0);
    ((value - target).abs() <= reach).then_some((target, k as u32))
}

/// A place or a drag snapped to a spacing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Snap {
    /// The placing point (sheet mm) or the drag that puts the dimension
    /// there.
    pub to: P2,
    pub family: Family,
    /// The spacing snapped to, paper mm.
    pub spacing: f64,
    /// How many spacings out it lies.
    pub multiple: u32,
    /// Its distance as its spacing measures it: `multiple` × `spacing`.
    pub value: f64,
}

/// The placing point a new dimension (its picks and options, not yet
/// placed) snaps to near `at`; `spacing_of` gives the spacing of its
/// family, None when there is none (the first dimension of a drawing).
/// None: it goes where it is clicked.
pub fn snap_place(
    annotation: &ViewAnnotation,
    seen: &[Seen],
    frame: &Frame<'_>,
    at: P2,
    spacing_of: impl FnMut(Family) -> Option<f64>,
) -> Option<Snap> {
    let mut placed = annotation.clone();
    placed_at(&mut placed, seen, frame, at);
    let g = gauge(&placed, seen, frame)?;
    // A click moves an arc's radius and a leader's reach along the line
    // from their centre.
    let grow = match g.center {
        Some(c) => unit2(sub2(at, c)),
        None => g.grow,
    };
    snap(g, grow, at, spacing_of)
}

/// The drag `delta` (sheet mm) of a placed dimension snaps to near it, as
/// [`snap_place`] has it.
pub fn snap_move(
    annotation: &ViewAnnotation,
    seen: &[Seen],
    frame: &Frame<'_>,
    delta: P2,
    spacing_of: impl FnMut(Family) -> Option<f64>,
) -> Option<Snap> {
    let mut placed = annotation.clone();
    moved(&mut placed, seen, frame, delta);
    let g = gauge(&placed, seen, frame)?;
    snap(g, g.grow, delta, spacing_of)
}

fn snap(
    g: Gauge,
    grow: P2,
    from: P2,
    mut spacing_of: impl FnMut(Family) -> Option<f64>,
) -> Option<Snap> {
    let spacing = spacing_of(g.family)?;
    let (value, multiple) = snapped_value(g.value, spacing)?;
    Some(Snap {
        to: add2(from, mul2(grow, value - g.value)),
        family: g.family,
        spacing,
        multiple,
        value,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clip::ViewMap;

    fn near(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    fn near2(a: P2, b: P2) -> bool {
        near(a[0], b[0]) && near(a[1], b[1])
    }

    /// A view at 1:1 whose origin lies at (100, 100) on the sheet, its box
    /// 100..140 x 100..120 (a 40 x 20 box at its origin).
    fn map() -> ViewMap {
        ViewMap::new(1.0, [0.0, 0.0], [100.0, 100.0])
    }

    fn frame(map: &ViewMap) -> Frame<'_> {
        let mut view_box = Bounds::empty();
        view_box.add([100.0, 100.0]);
        view_box.add([140.0, 120.0]);
        Frame {
            map,
            view_box,
            text: 3.5,
        }
    }

    /// The box's upright sides, measured across: a horizontal dimension
    /// whose picks' middle is (120, 110) on the sheet.
    fn sides() -> [Seen; 2] {
        [
            Seen::Line([0.0, 0.0], [0.0, 20.0]),
            Seen::Line([40.0, 0.0], [40.0, 20.0]),
        ]
    }

    fn linear() -> ViewAnnotation {
        let mut d = ViewAnnotation::new(1, AnnotationType::Linear, Vec::new());
        d.along = Some(crate::annotate::Along::Horizontal);
        d
    }

    #[test]
    fn a_linear_dimension_is_spaced_beyond_its_view_box() {
        let map = map();
        let frame = frame(&map);
        let seen = sides();
        let mut d = linear();
        // Not placed by hand: nothing to measure.
        assert_eq!(gauge(&d, &seen, &frame), None);
        // 12 above the box (its top at 120).
        placed_at(&mut d, &seen, &frame, [120.0, 132.0]);
        let g = gauge(&d, &seen, &frame).unwrap();
        assert_eq!(g.family, Family::Line);
        assert!(near(g.value, 12.0), "{g:?}");
        assert!(near2(g.grow, [0.0, 1.0]), "{g:?}");
        // 13 below it (its bottom at 100): the way out is down.
        placed_at(&mut d, &seen, &frame, [120.0, 87.0]);
        let g = gauge(&d, &seen, &frame).unwrap();
        assert!(near(g.value, 13.0), "{g:?}");
        assert!(near2(g.grow, [0.0, -1.0]), "{g:?}");
        // Inside the box: no spacing.
        placed_at(&mut d, &seen, &frame, [120.0, 110.0]);
        assert_eq!(gauge(&d, &seen, &frame), None);
    }

    #[test]
    fn a_new_dimension_snaps_to_the_spacing_and_its_multiples() {
        let map = map();
        let frame = frame(&map);
        let seen = sides();
        let d = linear();
        let ten = |_: Family| Some(10.0);
        // 11.5 above: one spacing, 10.
        let s = snap_place(&d, &seen, &frame, [125.0, 131.5], ten).unwrap();
        assert!(near2(s.to, [125.0, 130.0]), "{s:?}");
        assert_eq!((s.multiple, s.family), (1, Family::Line));
        assert!(near(s.value, 10.0) && near(s.spacing, 10.0), "{s:?}");
        let mut placed = d.clone();
        placed_at(&mut placed, &seen, &frame, s.to);
        assert_eq!(placed.line_offset, Some(20.0));
        // 21 above: two spacings.
        let s = snap_place(&d, &seen, &frame, [120.0, 141.0], ten).unwrap();
        assert!(near2(s.to, [120.0, 140.0]), "{s:?}");
        assert_eq!(s.multiple, 2);
        // 13 below: one spacing below.
        let s = snap_place(&d, &seen, &frame, [120.0, 87.0], ten).unwrap();
        assert!(near2(s.to, [120.0, 90.0]), "{s:?}");
        // 15.5 above: 4.5 from 20, farther than the reach: where clicked.
        assert_eq!(snap_place(&d, &seen, &frame, [120.0, 135.5], ten), None);
        // Inside the box, or no spacing yet: where clicked.
        assert_eq!(snap_place(&d, &seen, &frame, [120.0, 110.0], ten), None);
        assert_eq!(
            snap_place(&d, &seen, &frame, [120.0, 131.5], |_| None),
            None
        );
        // A small spacing snaps within half of it only.
        let three = |_: Family| Some(3.0);
        let s = snap_place(&d, &seen, &frame, [120.0, 129.4], three).unwrap();
        assert_eq!(s.multiple, 3);
        assert!(near2(s.to, [120.0, 129.0]), "{s:?}");
    }

    #[test]
    fn a_dragged_dimension_snaps_the_same_way() {
        let map = map();
        let frame = frame(&map);
        let seen = sides();
        let mut d = linear();
        placed_at(&mut d, &seen, &frame, [120.0, 130.0]);
        // Dragged up 9 and along 3: 19 out, snapped to 20; along it stays.
        let s = snap_move(&d, &seen, &frame, [3.0, 9.0], |_| Some(10.0)).unwrap();
        assert!(near2(s.to, [3.0, 10.0]), "{s:?}");
        assert_eq!(s.multiple, 2);
        moved(&mut d, &seen, &frame, s.to);
        assert_eq!(d.line_offset, Some(30.0));
        assert_eq!(d.text_offset, Some([3.0, 0.0]));
    }

    #[test]
    fn stacked_dimensions_space_from_the_one_inside() {
        let line = |value: f64, grow: P2| Gauge {
            family: Family::Line,
            value,
            grow,
            center: None,
        };
        let first = line(12.0, [0.0, 1.0]);
        let second = line(24.0, [0.0, 1.0]);
        let below = line(5.0, [0.0, -1.0]);
        // The second above the first: their step.
        assert!(near(spacing(&second, &[first, below]), 12.0));
        // The first: its distance from the box.
        assert!(near(spacing(&first, &[second, below]), 12.0));
        // One alone on its side: its distance.
        assert!(near(spacing(&below, &[first, second]), 5.0));
        // In a row (the same distance): its distance.
        assert!(near(spacing(&first, &[first]), 12.0));
    }

    #[test]
    fn an_angle_snaps_its_radius_and_a_leader_its_reach() {
        let map = map();
        let frame = frame(&map);
        // Two edges from the view's origin: the corner at (100, 100).
        let edges = [
            Seen::Line([0.0, 0.0], [30.0, 0.0]),
            Seen::Line([0.0, 0.0], [0.0, 30.0]),
        ];
        let angle = ViewAnnotation::new(1, AnnotationType::Angle, Vec::new());
        // Clicked 12.73 from the corner: two spacings of 6.
        let s = snap_place(&angle, &edges, &frame, [109.0, 109.0], |f| {
            (f == Family::Arc).then_some(6.0)
        })
        .unwrap();
        assert_eq!((s.family, s.multiple), (Family::Arc, 2));
        let r = len2(sub2(s.to, [100.0, 100.0]));
        assert!(near(r, 12.0), "{s:?}");
        let mut placed = angle.clone();
        placed_at(&mut placed, &edges, &frame, s.to);
        assert!(near(placed.line_offset.unwrap(), 12.0));
        // Its arc dragged out along its middle by 5: 17, snapped to 18.
        let half = std::f64::consts::FRAC_1_SQRT_2;
        let s = snap_move(&placed, &edges, &frame, [5.0 * half, 5.0 * half], |_| {
            Some(6.0)
        })
        .unwrap();
        moved(&mut placed, &edges, &frame, s.to);
        assert!(near(placed.line_offset.unwrap(), 18.0), "{placed:?}");

        // A radius of a circle of 5 seen end on, its centre (120, 110):
        // its text 9.5 beyond the circle snaps to two spacings of 4.
        let circle = [Seen::Circle {
            center: [20.0, 10.0],
            radius: 5.0,
            end_on: true,
            across: [1.0, 0.0],
            arc: None,
        }];
        let radius = ViewAnnotation::new(2, AnnotationType::Radius, Vec::new());
        let s = snap_place(&radius, &circle, &frame, [134.5, 110.0], |f| {
            (f == Family::Leader).then_some(4.0)
        })
        .unwrap();
        assert_eq!((s.family, s.multiple), (Family::Leader, 2));
        assert!(near2(s.to, [133.0, 110.0]), "{s:?}");
        // An angle's spacing is no leader's.
        assert_eq!(
            snap_place(&radius, &circle, &frame, [134.5, 110.0], |f| {
                (f == Family::Arc).then_some(4.0)
            }),
            None
        );
    }

    #[test]
    fn a_value_snaps_only_near_a_multiple() {
        assert_eq!(snapped_value(11.0, 10.0), Some((10.0, 1)));
        assert_eq!(snapped_value(2.0, 10.0), None);
        assert_eq!(snapped_value(29.0, 10.0), Some((30.0, 3)));
        assert_eq!(snapped_value(25.0, 10.0), None);
        assert_eq!(snapped_value(10.0, 0.0), None);
        assert_eq!(snapped_value(-3.0, 10.0), None);
    }
}
