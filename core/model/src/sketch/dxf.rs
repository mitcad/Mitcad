// SPDX-License-Identifier: MIT
//! A DXF drawing into a sketch (U6, `sketch.import_dxf`): lines, arcs,
//! circles, ellipses and elliptical arcs, splines, points and text, in
//! millimetres, moved by an offset. End points and centres closer than a
//! micrometre's thousandth become one sketch point, so that the curves are
//! joined end to end; nothing is constrained or
//! dimensioned. Export is in `exchange.rs` (`sketch_drawing`).

use std::collections::HashMap;
use std::f64::consts::TAU;

use mitcad_dxf::{Drawing, Geometry, Point2, Spline};

use super::edit::SketchEdit;
use super::{Entity, EntityKind, SketchText};
use crate::ids::EntityUid;

/// What a drawing added to a sketch.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DxfInsert {
    pub curves: usize,
    pub points: usize,
    pub texts: usize,
    /// Entities left out, with the reason.
    pub skipped: Vec<String>,
}

/// Points closer than this (mm) are one.
const TOLERANCE: f64 = 1e-6;
/// The side of a cell of the point index; larger than the tolerance, so
/// that a match lies in the point's cell or a neighbour.
const CELL: f64 = 1e-4;

/// The points in a cell of the index, with their positions.
type Cell = Vec<(EntityUid, [f64; 2])>;

/// The shared points made so far, by position.
#[derive(Default)]
struct SharedPoints {
    cells: HashMap<(i64, i64), Cell>,
    made: usize,
}

impl SharedPoints {
    fn cell(p: [f64; 2]) -> (i64, i64) {
        ((p[0] / CELL).floor() as i64, (p[1] / CELL).floor() as i64)
    }

    /// The point at a position: an earlier one within the tolerance, or a
    /// new one.
    fn at(&mut self, edit: &mut SketchEdit<'_>, p: [f64; 2]) -> EntityUid {
        let (cx, cy) = Self::cell(p);
        for dx in -1..=1 {
            for dy in -1..=1 {
                if let Some(found) = self.cells.get(&(cx + dx, cy + dy)).and_then(|points| {
                    points
                        .iter()
                        .find(|(_, q)| (q[0] - p[0]).hypot(q[1] - p[1]) <= TOLERANCE)
                }) {
                    return found.0;
                }
            }
        }
        let id = edit.add_point(p);
        self.cells.entry((cx, cy)).or_default().push((id, p));
        self.made += 1;
        id
    }
}

fn finite(points: &[[f64; 2]]) -> bool {
    points.iter().flatten().all(|v| v.is_finite())
}

/// Adds the drawing's entities to the sketch being edited, moved by `at`.
pub fn insert_drawing(
    edit: &mut SketchEdit<'_>,
    drawing: &Drawing,
    at: [f64; 2],
) -> Result<DxfInsert, String> {
    let mut shared = SharedPoints::default();
    let mut result = DxfInsert::default();
    let place = |p: Point2| [p.x + at[0], p.y + at[1]];
    for (index, entity) in drawing.entities.iter().enumerate() {
        let skip = |result: &mut DxfInsert, why: &str| {
            result
                .skipped
                .push(format!("entity {index} (layer {}): {why}", entity.layer));
        };
        match &entity.geometry {
            Geometry::Point(p) => {
                let p = place(*p);
                if finite(&[p]) {
                    shared.at(edit, p);
                } else {
                    skip(&mut result, "a point that is not finite");
                }
            }
            Geometry::Line { start, end } => {
                let (a, b) = (place(*start), place(*end));
                if !finite(&[a, b]) || (a[0] - b[0]).hypot(a[1] - b[1]) <= TOLERANCE {
                    skip(&mut result, "a line without length");
                    continue;
                }
                let start = shared.at(edit, a);
                let end = shared.at(edit, b);
                add_curve(
                    edit,
                    &mut result,
                    EntityKind::Line {
                        start,
                        end,
                        centerline: false,
                    },
                );
            }
            Geometry::Circle { center, radius } => {
                let c = place(*center);
                if !finite(&[c]) || !(radius.is_finite() && *radius > TOLERANCE) {
                    skip(&mut result, "a circle without radius");
                    continue;
                }
                let center = shared.at(edit, c);
                add_curve(
                    edit,
                    &mut result,
                    EntityKind::Circle {
                        center,
                        radius: *radius,
                    },
                );
            }
            Geometry::Arc {
                center,
                radius,
                start_angle,
                end_angle,
            } => {
                let c = place(*center);
                if !finite(&[c]) || !(radius.is_finite() && *radius > TOLERANCE) {
                    skip(&mut result, "an arc without radius");
                    continue;
                }
                let center = shared.at(edit, c);
                if end_angle - start_angle >= TAU - 1e-12 {
                    add_curve(
                        edit,
                        &mut result,
                        EntityKind::Circle {
                            center,
                            radius: *radius,
                        },
                    );
                    continue;
                }
                let on = |angle: f64| [c[0] + radius * angle.cos(), c[1] + radius * angle.sin()];
                let start = shared.at(edit, on(*start_angle));
                let end = shared.at(edit, on(*end_angle));
                if start == end {
                    skip(&mut result, "an arc too short to keep");
                    continue;
                }
                add_curve(edit, &mut result, EntityKind::Arc { center, start, end });
            }
            Geometry::Ellipse {
                center,
                major_axis,
                ratio,
                start_param,
                end_param,
            } => {
                let c = place(*center);
                let major = major_axis.length();
                let minor_radius = major * ratio;
                if !finite(&[c]) || !(major > TOLERANCE && minor_radius > TOLERANCE) {
                    skip(&mut result, "an ellipse without size");
                    continue;
                }
                let center = shared.at(edit, c);
                // The end of the major axis is the ellipse's own handle.
                let major_point = edit.add_point([c[0] + major_axis.x, c[1] + major_axis.y]);
                if entity.geometry.is_full_ellipse() {
                    add_curve(
                        edit,
                        &mut result,
                        EntityKind::Ellipse {
                            center,
                            major: major_point,
                            minor_radius,
                        },
                    );
                    continue;
                }
                let minor_axis = major_axis.perp() * *ratio;
                let on = |t: f64| {
                    let p = *major_axis * t.cos() + minor_axis * t.sin();
                    [c[0] + p.x, c[1] + p.y]
                };
                let start = shared.at(edit, on(*start_param));
                let end = shared.at(edit, on(*end_param));
                add_curve(
                    edit,
                    &mut result,
                    EntityKind::EllipticalArc {
                        center,
                        major: major_point,
                        minor_radius,
                        start,
                        end,
                    },
                );
            }
            Geometry::Spline(spline) => match spline_kind(edit, &mut shared, spline, &place) {
                Ok(kind) => add_curve(edit, &mut result, kind),
                Err(why) => skip(&mut result, &why),
            },
            Geometry::Text(text) => {
                let content = text.content.trim();
                let p = place(text.position);
                if content.is_empty()
                    || !finite(&[p])
                    || !(text.height.is_finite() && text.height > 0.0)
                {
                    skip(&mut result, "an empty text");
                    continue;
                }
                edit.text(SketchText {
                    angle: text.rotation,
                    ..SketchText::new(EntityUid(0), content, p, text.height)
                })?;
                result.texts += 1;
            }
        }
    }
    result.points = shared.made;
    if result.curves == 0 && result.points == 0 && result.texts == 0 {
        return Err("the drawing has nothing a sketch can hold".to_owned());
    }
    Ok(result)
}

fn add_curve(edit: &mut SketchEdit<'_>, result: &mut DxfInsert, kind: EntityKind) {
    let id = edit.id();
    edit.push(Entity::new(id, kind));
    result.curves += 1;
}

/// A spline by its control points, knots and weights, or through its fit
/// points; the ends of a clamped spline join other curves.
fn spline_kind(
    edit: &mut SketchEdit<'_>,
    shared: &mut SharedPoints,
    spline: &Spline,
    place: &dyn Fn(Point2) -> [f64; 2],
) -> Result<EntityKind, String> {
    let degree = spline.degree as usize;
    let n = spline.control_points.len();
    if degree >= 1 && n > degree && spline.knots.len() == n + degree + 1 {
        let control: Vec<[f64; 2]> = spline.control_points.iter().map(|p| place(*p)).collect();
        if !finite(&control) || !spline.knots.iter().all(|k| k.is_finite()) {
            return Err("a spline that is not finite".to_owned());
        }
        let knots = &spline.knots;
        let clamped = knots[..=degree].iter().all(|k| *k == knots[0])
            && knots[n..].iter().all(|k| *k == knots[n + degree]);
        let mut ids = Vec::with_capacity(n);
        for (i, p) in control.iter().enumerate() {
            let end = i == 0 || i == n - 1;
            ids.push(if clamped && end {
                shared.at(edit, *p)
            } else {
                edit.add_point(*p)
            });
        }
        let weights = if spline.is_rational() {
            (0..n).map(|i| spline.weight(i)).collect()
        } else {
            Vec::new()
        };
        return Ok(EntityKind::Spline {
            degree: spline.degree,
            control: ids,
            weights,
            knots: knots.clone(),
        });
    }
    if spline.fit_points.len() >= 2 {
        let fit: Vec<[f64; 2]> = spline.fit_points.iter().map(|p| place(*p)).collect();
        if !finite(&fit) {
            return Err("a spline that is not finite".to_owned());
        }
        let last = fit.len() - 1;
        let points = fit
            .iter()
            .enumerate()
            .map(|(i, p)| {
                if i == 0 || i == last {
                    shared.at(edit, *p)
                } else {
                    edit.add_point(*p)
                }
            })
            .collect();
        return Ok(EntityKind::FittedSpline { points });
    }
    Err("a spline without a valid control polygon or fit points".to_owned())
}
