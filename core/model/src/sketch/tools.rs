// SPDX-License-Identifier: MIT
//! Drawing tools (the Create menu of a sketch): each makes entities and
//! the geometric constraints the tool implies, never a separate "shape".
//! Positions are where the tool puts them; the solve that ends the edit
//! keeps them unless constraints move them. Dimensions are added
//! separately (`sketch.add_dimension`), as when a value is typed in.

use std::f64::consts::{PI, TAU};

use super::edit::{PointInput, SketchEdit};
use super::geometry::{P2, add, angle_after, cross, dist, dot, norm, perp, scale, sub, unit};
use super::{ConstraintKind, Entity, EntityKind, SketchText};
use crate::ids::EntityUid;

/// Rectangle tools.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RectangleMode {
    /// Opposite corners, sides along the sketch axes.
    TwoPoint { a: [f64; 2], b: [f64; 2] },
    /// One side from `a` to `b`, the other through `c`'s side distance.
    ThreePoint {
        a: [f64; 2],
        b: [f64; 2],
        c: [f64; 2],
    },
    /// Centre and a corner, sides along the sketch axes.
    Center { center: [f64; 2], corner: [f64; 2] },
}

/// Circle tools.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CircleMode {
    CenterRadius {
        center: PointInput,
        radius: f64,
    },
    /// The ends of a diameter.
    TwoPoint {
        a: [f64; 2],
        b: [f64; 2],
    },
    ThreePoint {
        a: [f64; 2],
        b: [f64; 2],
        c: [f64; 2],
    },
    /// Tangent to two lines, of a radius, near a point.
    TwoTangent {
        a: EntityUid,
        b: EntityUid,
        radius: f64,
        near: [f64; 2],
    },
    /// Tangent to three lines.
    ThreeTangent {
        a: EntityUid,
        b: EntityUid,
        c: EntityUid,
    },
}

/// Arc tools.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ArcMode {
    /// From `start` through `through` to `end`.
    ThreePoint {
        start: PointInput,
        through: [f64; 2],
        end: PointInput,
    },
    /// Counter-clockwise around `center` from `start` to the direction of
    /// `end` (clockwise with `clockwise`).
    Center {
        center: PointInput,
        start: PointInput,
        end: [f64; 2],
        clockwise: bool,
    },
    /// From the end point `from` of a curve, tangent to it, to `end`.
    Tangent { from: EntityUid, end: PointInput },
}

/// Slot tools; `width` is the slot's width.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SlotMode {
    /// Between the centres of its ends.
    CenterToCenter { a: [f64; 2], b: [f64; 2] },
    /// The centre of the slot and the centre of one end.
    CenterPoint { center: [f64; 2], end: [f64; 2] },
    /// Along an arc around `center` from `start` to the direction of `end`.
    Arc {
        center: [f64; 2],
        start: [f64; 2],
        end: [f64; 2],
    },
}

fn finite(points: &[[f64; 2]]) -> Result<(), String> {
    if points.iter().flatten().all(|v| v.is_finite()) {
        Ok(())
    } else {
        Err("the points must be finite".to_owned())
    }
}

fn line(start: EntityUid, end: EntityUid) -> EntityKind {
    EntityKind::Line {
        start,
        end,
        centerline: false,
    }
}

/// The centre of the circle through three points.
fn circumcenter(a: P2, b: P2, c: P2) -> Result<P2, String> {
    let d = 2.0 * cross(sub(b, a), sub(c, a));
    if d.abs() <= 1e-12 * (dist(a, b) * dist(a, c)).max(1e-300) {
        return Err("the three points are on a line".to_owned());
    }
    let (ab, ac) = (sub(b, a), sub(c, a));
    let (ab2, ac2) = (dot(ab, ab), dot(ac, ac));
    Ok(add(
        a,
        [
            (ac[1] * ab2 - ab[1] * ac2) / d,
            (ab[0] * ac2 - ac[0] * ab2) / d,
        ],
    ))
}

impl SketchEdit<'_> {
    /// Adds a curve with a fresh id.
    fn curve_entity(&mut self, kind: EntityKind, construction: bool) -> EntityUid {
        let id = self.id();
        let mut e = Entity::new(id, kind);
        e.construction = construction;
        self.push(e)
    }

    /// A point at a position, as an input.
    fn new_point(&mut self, at: P2) -> EntityUid {
        self.add_point(at)
    }

    pub fn add_line(
        &mut self,
        start: PointInput,
        end: PointInput,
        construction: bool,
        centerline: bool,
    ) -> Result<EntityUid, String> {
        if construction && centerline {
            return Err("a line is construction geometry or a centre line, not both".to_owned());
        }
        let id = self.id();
        let (s, e) = (self.point(start)?, self.point(end)?);
        if s == e || dist(self.at(s)?, self.at(e)?) == 0.0 {
            return Err("a line needs two different points".to_owned());
        }
        let mut entity = Entity::new(
            id,
            EntityKind::Line {
                start: s,
                end: e,
                centerline,
            },
        );
        entity.construction = construction;
        Ok(self.push(entity))
    }

    /// Four lines with shared corners; returns the lines in order around
    /// the rectangle (counter-clockwise).
    pub fn rectangle(
        &mut self,
        mode: RectangleMode,
        construction: bool,
    ) -> Result<Vec<EntityUid>, String> {
        let (corners, axis_aligned) = match mode {
            RectangleMode::TwoPoint { a, b } => {
                finite(&[a, b])?;
                let (x0, x1) = (a[0].min(b[0]), a[0].max(b[0]));
                let (y0, y1) = (a[1].min(b[1]), a[1].max(b[1]));
                ([[x0, y0], [x1, y0], [x1, y1], [x0, y1]], true)
            }
            RectangleMode::Center { center, corner } => {
                finite(&[center, corner])?;
                let (dx, dy) = ((corner[0] - center[0]).abs(), (corner[1] - center[1]).abs());
                let (cx, cy) = (center[0], center[1]);
                (
                    [
                        [cx - dx, cy - dy],
                        [cx + dx, cy - dy],
                        [cx + dx, cy + dy],
                        [cx - dx, cy + dy],
                    ],
                    true,
                )
            }
            RectangleMode::ThreePoint { a, b, c } => {
                finite(&[a, b, c])?;
                let side = sub(b, a);
                if norm(side) == 0.0 {
                    return Err("the first side has no length".to_owned());
                }
                let n = perp(unit(side));
                let h = scale(n, dot(sub(c, b), n));
                let corners = [a, b, add(b, h), add(a, h)];
                if cross(side, h) < 0.0 {
                    ([corners[0], corners[3], corners[2], corners[1]], false)
                } else {
                    (corners, false)
                }
            }
        };
        if corners[0] == corners[2]
            || dist(corners[0], corners[1]) == 0.0
            || dist(corners[1], corners[2]) == 0.0
        {
            return Err("the rectangle has no area".to_owned());
        }
        let lines = self.ids(4);
        let points = self.ids(4);
        for i in 0..4 {
            let mut e = Entity::new(lines[i], line(points[i], points[(i + 1) % 4]));
            e.construction = construction;
            self.push(e);
        }
        for i in 0..4 {
            self.push(Entity::point(points[i], corners[i]));
        }
        if axis_aligned {
            for (i, l) in lines.iter().enumerate() {
                self.add_constraint(if i.is_multiple_of(2) {
                    ConstraintKind::Horizontal { line: *l }
                } else {
                    ConstraintKind::Vertical { line: *l }
                });
            }
        } else {
            for i in 0..3 {
                self.add_constraint(ConstraintKind::Perpendicular {
                    a: lines[i],
                    b: lines[i + 1],
                });
            }
        }
        if let RectangleMode::Center { center, .. } = mode {
            // A construction diagonal with the centre at its middle.
            let c = self.new_point(center);
            let diagonal = self.curve_entity(line(points[0], points[2]), true);
            self.add_constraint(ConstraintKind::Midpoint {
                point: c,
                curve: diagonal,
            });
        }
        Ok(lines)
    }

    pub fn circle(&mut self, mode: CircleMode, construction: bool) -> Result<EntityUid, String> {
        let (center, radius, tangents): (PointInput, f64, Vec<EntityUid>) = match mode {
            CircleMode::CenterRadius { center, radius } => (center, radius, Vec::new()),
            CircleMode::TwoPoint { a, b } => {
                finite(&[a, b])?;
                (
                    PointInput::At(scale(add(a, b), 0.5)),
                    dist(a, b) / 2.0,
                    Vec::new(),
                )
            }
            CircleMode::ThreePoint { a, b, c } => {
                finite(&[a, b, c])?;
                let center = circumcenter(a, b, c)?;
                (PointInput::At(center), dist(center, a), Vec::new())
            }
            CircleMode::TwoTangent { a, b, radius, near } => {
                finite(&[near])?;
                (PointInput::At(near), radius, vec![a, b])
            }
            CircleMode::ThreeTangent { a, b, c } => {
                // Start from the incircle of the lines' middle points.
                let solved = self.solved()?;
                let mid = |l: EntityUid| -> Result<P2, String> {
                    match solved.curves.get(&l) {
                        Some(super::geometry::Curve2::Line { a, b }) => Ok(scale(add(*a, *b), 0.5)),
                        _ => Err(format!("c{} is not a line", l.0)),
                    }
                };
                let (ma, mb, mc) = (mid(a)?, mid(b)?, mid(c)?);
                let center = scale(add(add(ma, mb), mc), 1.0 / 3.0);
                let r = (dist(center, ma) + dist(center, mb) + dist(center, mc)) / 3.0;
                (PointInput::At(center), r.max(1e-6), vec![a, b, c])
            }
        };
        if !(radius.is_finite() && radius > 0.0) {
            return Err(format!("the radius must be positive, got {radius}"));
        }
        let id = self.id();
        let center = self.point(center)?;
        let mut e = Entity::new(id, EntityKind::Circle { center, radius });
        e.construction = construction;
        self.push(e);
        for t in tangents {
            self.curve(t)?;
            self.add_constraint(ConstraintKind::Tangent { a: t, b: id });
        }
        Ok(id)
    }

    pub fn arc(&mut self, mode: ArcMode, construction: bool) -> Result<EntityUid, String> {
        let id = self.id();
        let entity = match mode {
            ArcMode::ThreePoint {
                start,
                through,
                end,
            } => {
                let (s, e) = (self.point(start)?, self.point(end)?);
                let (ps, pe) = (self.at(s)?, self.at(e)?);
                finite(&[through])?;
                let c = circumcenter(ps, through, pe)?;
                let center = self.new_point(c);
                // Counter-clockwise from start to end must pass through the
                // middle point; else the other way round.
                let angle = |p: P2| (p[1] - c[1]).atan2(p[0] - c[0]);
                let (a0, am, a1) = (angle(ps), angle(through), angle(pe));
                let (start, end) = if angle_after(a0, am) < angle_after(a0, a1) {
                    (s, e)
                } else {
                    (e, s)
                };
                EntityKind::Arc { center, start, end }
            }
            ArcMode::Center {
                center,
                start,
                end,
                clockwise,
            } => {
                let (c, s) = (self.point(center)?, self.point(start)?);
                let (pc, ps) = (self.at(c)?, self.at(s)?);
                finite(&[end])?;
                let r = dist(pc, ps);
                let dir = unit(sub(end, pc));
                if r == 0.0 || norm(dir) == 0.0 {
                    return Err("the arc has no radius or no direction".to_owned());
                }
                let e = self.new_point(add(pc, scale(dir, r)));
                if clockwise {
                    EntityKind::Arc {
                        center: c,
                        start: e,
                        end: s,
                    }
                } else {
                    EntityKind::Arc {
                        center: c,
                        start: s,
                        end: e,
                    }
                }
            }
            ArcMode::Tangent { from, end } => {
                let p = self.at(from)?;
                // The curve that ends at `from` and its direction there.
                let solved = self.solved()?;
                let (curve, direction) = self
                    .def
                    .entities
                    .iter()
                    .filter(|e| !e.construction || construction)
                    .find_map(|e| {
                        let (s, t) = e.kind.ends()?;
                        let c = solved.curves.get(&e.id)?;
                        let (lo, hi) = c.domain();
                        if t == from {
                            Some((e.id, c.eval(hi)[1]))
                        } else if s == from {
                            Some((e.id, scale(c.eval(lo)[1], -1.0)))
                        } else {
                            None
                        }
                    })
                    .ok_or_else(|| format!("no curve ends at p{}", from.0))?;
                let e = self.point(end)?;
                let pe = self.at(e)?;
                let d = unit(direction);
                let n = perp(d);
                let chord = sub(pe, p);
                let along = dot(chord, n);
                if along.abs() <= 1e-12 * norm(chord).max(1e-300) {
                    return Err("the end is straight ahead; draw a line".to_owned());
                }
                let r = dot(chord, chord) / (2.0 * along);
                let center = self.new_point(add(p, scale(n, r)));
                self.add_constraint(ConstraintKind::Tangent { a: curve, b: id });
                if r > 0.0 {
                    EntityKind::Arc {
                        center,
                        start: from,
                        end: e,
                    }
                } else {
                    EntityKind::Arc {
                        center,
                        start: e,
                        end: from,
                    }
                }
            }
        };
        let mut e = Entity::new(id, entity);
        e.construction = construction;
        Ok(self.push(e))
    }

    /// A regular polygon around `center`: `inscribed` puts the corners on a
    /// construction circle through `vertex`, otherwise `vertex` is the
    /// middle of a side and the sides touch the circle. Equal sides.
    pub fn polygon(
        &mut self,
        center: [f64; 2],
        vertex: [f64; 2],
        sides: usize,
        inscribed: bool,
        construction: bool,
    ) -> Result<Vec<EntityUid>, String> {
        finite(&[center, vertex])?;
        if !(3..=256).contains(&sides) {
            return Err(format!("a polygon has 3 to 256 sides, got {sides}"));
        }
        let r = dist(center, vertex);
        if r == 0.0 {
            return Err("the polygon has no size".to_owned());
        }
        let a0 = (vertex[1] - center[1]).atan2(vertex[0] - center[0]);
        let n = sides as f64;
        let (big, first) = if inscribed {
            (r, a0)
        } else {
            (r / (PI / n).cos(), a0 - PI / n)
        };
        let lines = self.ids(sides);
        let corners = self.ids(sides);
        for i in 0..sides {
            let mut e = Entity::new(lines[i], line(corners[i], corners[(i + 1) % sides]));
            e.construction = construction;
            self.push(e);
        }
        for (i, id) in corners.iter().enumerate() {
            let a = first + TAU * i as f64 / n;
            self.push(Entity::point(
                *id,
                [center[0] + big * a.cos(), center[1] + big * a.sin()],
            ));
        }
        let c = self.new_point(center);
        let circle = self.curve_entity(
            EntityKind::Circle {
                center: c,
                radius: if inscribed { big } else { r },
            },
            true,
        );
        if inscribed {
            for p in &corners {
                self.add_constraint(ConstraintKind::Coincident {
                    point: *p,
                    entity: super::Ref::Curve(circle),
                });
            }
        } else {
            for l in &lines {
                self.add_constraint(ConstraintKind::Tangent { a: *l, b: circle });
            }
        }
        for w in lines.windows(2) {
            self.add_constraint(ConstraintKind::Equal { a: w[0], b: w[1] });
        }
        Ok(lines)
    }

    /// A slot: two end arcs, two sides tangent to them, and a construction
    /// centre line (an arc for an arc slot) with the end arcs' centres at
    /// its ends. Returns the outline curves.
    pub fn slot(&mut self, mode: SlotMode, width: f64) -> Result<Vec<EntityUid>, String> {
        if !(width.is_finite() && width > 0.0) {
            return Err(format!("the slot width must be positive, got {width}"));
        }
        let r = width / 2.0;
        match mode {
            SlotMode::CenterToCenter { a, b } => self.straight_slot(a, b, r, None),
            SlotMode::CenterPoint { center, end } => {
                finite(&[center, end])?;
                let a = sub(scale(center, 2.0), end);
                self.straight_slot(a, end, r, Some(center))
            }
            SlotMode::Arc { center, start, end } => self.arc_slot(center, start, end, r),
        }
    }

    fn straight_slot(
        &mut self,
        a: [f64; 2],
        b: [f64; 2],
        r: f64,
        middle: Option<[f64; 2]>,
    ) -> Result<Vec<EntityUid>, String> {
        finite(&[a, b])?;
        let length = dist(a, b);
        if length == 0.0 {
            return Err("the slot has no length".to_owned());
        }
        let d = unit(sub(b, a));
        let n = perp(d);
        let curves = self.ids(4); // top, arc at b, bottom, arc at a
        let ca = self.new_point(a);
        let cb = self.new_point(b);
        let p1 = self.new_point(add(a, scale(n, r)));
        let p2 = self.new_point(add(b, scale(n, r)));
        let p3 = self.new_point(sub(b, scale(n, r)));
        let p4 = self.new_point(sub(a, scale(n, r)));
        self.push(Entity::new(curves[0], line(p2, p1)));
        self.push(Entity::new(
            curves[1],
            EntityKind::Arc {
                center: cb,
                start: p3,
                end: p2,
            },
        ));
        self.push(Entity::new(curves[2], line(p4, p3)));
        self.push(Entity::new(
            curves[3],
            EntityKind::Arc {
                center: ca,
                start: p1,
                end: p4,
            },
        ));
        let axis = self.curve_entity(line(ca, cb), true);
        for (l, arc) in [(0, 1), (0, 3), (2, 1), (2, 3)] {
            self.add_constraint(ConstraintKind::Tangent {
                a: curves[l],
                b: curves[arc],
            });
        }
        self.add_constraint(ConstraintKind::Equal {
            a: curves[1],
            b: curves[3],
        });
        if let Some(m) = middle {
            let m = self.new_point(m);
            self.add_constraint(ConstraintKind::Midpoint {
                point: m,
                curve: axis,
            });
        }
        Ok(curves)
    }

    fn arc_slot(
        &mut self,
        center: [f64; 2],
        start: [f64; 2],
        end: [f64; 2],
        r: f64,
    ) -> Result<Vec<EntityUid>, String> {
        finite(&[center, start, end])?;
        let big = dist(center, start);
        if big <= r {
            return Err("the slot's arc must be larger than half the width".to_owned());
        }
        let a0 = (start[1] - center[1]).atan2(start[0] - center[0]);
        let a1 = angle_after(a0, (end[1] - center[1]).atan2(end[0] - center[0]));
        let at = |radius: f64, a: f64| [center[0] + radius * a.cos(), center[1] + radius * a.sin()];
        let curves = self.ids(4); // outer, cap at end, inner, cap at start
        let c = self.new_point(center);
        let es = self.new_point(at(big, a0));
        let ee = self.new_point(at(big, a1));
        let outer_s = self.new_point(at(big + r, a0));
        let outer_e = self.new_point(at(big + r, a1));
        let inner_s = self.new_point(at(big - r, a0));
        let inner_e = self.new_point(at(big - r, a1));
        let arc = |center, start, end| EntityKind::Arc { center, start, end };
        self.push(Entity::new(curves[0], arc(c, outer_s, outer_e)));
        self.push(Entity::new(curves[1], arc(ee, outer_e, inner_e)));
        self.push(Entity::new(curves[2], arc(c, inner_s, inner_e)));
        self.push(Entity::new(curves[3], arc(es, inner_s, outer_s)));
        self.curve_entity(arc(c, es, ee), true);
        for (side, cap) in [(0, 1), (0, 3), (2, 1), (2, 3)] {
            self.add_constraint(ConstraintKind::Tangent {
                a: curves[side],
                b: curves[cap],
            });
        }
        self.add_constraint(ConstraintKind::Equal {
            a: curves[1],
            b: curves[3],
        });
        Ok(curves)
    }

    pub fn ellipse(
        &mut self,
        center: PointInput,
        major: [f64; 2],
        minor_radius: f64,
        arc: Option<(PointInput, PointInput)>,
        construction: bool,
    ) -> Result<EntityUid, String> {
        if !(minor_radius.is_finite() && minor_radius > 0.0) {
            return Err(format!(
                "the minor radius must be positive, got {minor_radius}"
            ));
        }
        finite(&[major])?;
        let id = self.id();
        let c = self.point(center)?;
        let m = self.new_point(major);
        let kind = match arc {
            None => EntityKind::Ellipse {
                center: c,
                major: m,
                minor_radius,
            },
            Some((start, end)) => EntityKind::EllipticalArc {
                center: c,
                major: m,
                minor_radius,
                start: self.point(start)?,
                end: self.point(end)?,
            },
        };
        let mut e = Entity::new(id, kind);
        e.construction = construction;
        Ok(self.push(e))
    }

    /// A spline through fit points, or by control points with a degree.
    pub fn spline(
        &mut self,
        points: &[PointInput],
        degree: Option<u32>,
        construction: bool,
    ) -> Result<EntityUid, String> {
        let id = self.id();
        let points = points
            .iter()
            .map(|p| self.point(*p))
            .collect::<Result<Vec<_>, _>>()?;
        let kind = match degree {
            None => EntityKind::FittedSpline { points },
            Some(degree) => EntityKind::Spline {
                degree,
                control: points,
                weights: Vec::new(),
                knots: Vec::new(),
            },
        };
        let mut e = Entity::new(id, kind);
        e.construction = construction;
        Ok(self.push(e))
    }

    /// A text; its glyph outlines bound profiles (see `text.rs`).
    pub fn text(&mut self, mut text: SketchText) -> Result<EntityUid, String> {
        if text.text.is_empty() {
            return Err("the text is empty".to_owned());
        }
        text.id = self.id();
        let id = text.id;
        self.def.texts.push(text);
        self.report.texts.push(id);
        Ok(id)
    }
}
