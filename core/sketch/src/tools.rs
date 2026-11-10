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

/// Rectangle tools. A corner or centre given as an existing point is that
/// point (mitcad#1269); `c` of three points is only a position.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RectangleMode {
    /// Opposite corners, sides along the sketch axes.
    TwoPoint { a: PointInput, b: PointInput },
    /// One side from `a` to `b`, the other through `c`'s side distance.
    ThreePoint {
        a: PointInput,
        b: PointInput,
        c: [f64; 2],
    },
    /// Centre and a corner, sides along the sketch axes.
    Center {
        center: PointInput,
        corner: PointInput,
    },
}

/// Circle tools. A point the circle passes through given as an existing
/// point is held on it (mitcad#1269).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CircleMode {
    CenterRadius {
        center: PointInput,
        radius: f64,
    },
    /// The ends of a diameter.
    TwoPoint {
        a: PointInput,
        b: PointInput,
    },
    ThreePoint {
        a: PointInput,
        b: PointInput,
        c: PointInput,
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
    /// From `start` through `through` to `end` (an existing point as
    /// `through` is held on the arc, mitcad#1269).
    ThreePoint {
        start: PointInput,
        through: PointInput,
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
    /// From the end point `from` of a line or a circular arc, square to it
    /// (normal, mitcad#1393), to `end`: the arc's centre on the line, or on
    /// the arc's tangent at `from`. It leaves toward the side of the curve
    /// `toward` is on (a position), else the side `end` is on: with
    /// `toward`, an arc that sweeps 180 degrees or more, whose end lies
    /// behind the curve, keeps the side it was drawn to (mitcad#1608).
    Normal {
        from: EntityUid,
        end: PointInput,
        toward: Option<P2>,
    },
}

/// Slot tools; `width` is the slot's width. A centre given as an existing
/// point is that point (mitcad#1269); an arc slot's `end` is a direction.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SlotMode {
    /// Between the centres of its ends.
    CenterToCenter { a: PointInput, b: PointInput },
    /// The centre of the slot and the centre of one end.
    CenterPoint { center: PointInput, end: PointInput },
    /// Along an arc around `center` from `start` to the direction of `end`.
    Arc {
        center: PointInput,
        start: PointInput,
        end: [f64; 2],
    },
}

/// The corner (0 lower left, then counter-clockwise) of an axis-aligned
/// rectangle that `p` is, `other` lying across the rectangle from it.
fn corner_index(p: P2, other: P2) -> usize {
    match (p[0] > other[0], p[1] > other[1]) {
        (false, false) => 0,
        (true, false) => 1,
        (true, true) => 2,
        (false, true) => 3,
    }
}

/// The existing point a point input names, if any.
fn existing(input: PointInput) -> Option<EntityUid> {
    match input {
        PointInput::Existing(id) => Some(id),
        PointInput::At(_) => None,
    }
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
            return Err("a line is construction geometry or a center line, not both".to_owned());
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

    /// A line through its middle `center` to `end` and as far on the other
    /// side (mitcad#1393): `center` (a new point, or the existing point
    /// given) held at the line's middle by a midpoint constraint. Returns
    /// the line and its middle point.
    pub fn midpoint_line(
        &mut self,
        center: PointInput,
        end: PointInput,
        construction: bool,
        centerline: bool,
    ) -> Result<(EntityUid, EntityUid), String> {
        let (c, e) = (self.input_position(center)?, self.input_position(end)?);
        if dist(c, e) == 0.0 {
            return Err("a midpoint line needs its middle and an end apart".to_owned());
        }
        let middle = self.point(center)?;
        let start = self.new_point(sub(scale(c, 2.0), e));
        let line = self.add_line(PointInput::Existing(start), end, construction, centerline)?;
        self.add_constraint(ConstraintKind::Midpoint {
            point: middle,
            curve: line,
        });
        Ok((line, middle))
    }

    /// Four lines with shared corners; returns the lines in order around
    /// the rectangle (counter-clockwise).
    pub fn rectangle(
        &mut self,
        mode: RectangleMode,
        construction: bool,
    ) -> Result<Vec<EntityUid>, String> {
        // The existing points among the inputs, at the corners they are
        // (mitcad#1269): the rectangle's corner there is that point.
        let mut joined: [Option<EntityUid>; 4] = [None; 4];
        let (corners, axis_aligned) = match mode {
            RectangleMode::TwoPoint { a: pa, b: pb } => {
                let (a, b) = (self.input_position(pa)?, self.input_position(pb)?);
                let (x0, x1) = (a[0].min(b[0]), a[0].max(b[0]));
                let (y0, y1) = (a[1].min(b[1]), a[1].max(b[1]));
                joined[corner_index(a, b)] = existing(pa);
                if let Some(p) = existing(pb) {
                    joined[corner_index(b, a)] = Some(p);
                }
                ([[x0, y0], [x1, y0], [x1, y1], [x0, y1]], true)
            }
            RectangleMode::Center {
                center: pc,
                corner: pk,
            } => {
                let (center, corner) = (self.input_position(pc)?, self.input_position(pk)?);
                joined[corner_index(corner, center)] = existing(pk);
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
            RectangleMode::ThreePoint { a: pa, b: pb, c } => {
                let (a, b) = (self.input_position(pa)?, self.input_position(pb)?);
                finite(&[c])?;
                let side = sub(b, a);
                if norm(side) == 0.0 {
                    return Err("the first side has no length".to_owned());
                }
                let n = perp(unit(side));
                let h = scale(n, dot(sub(c, b), n));
                let corners = [a, b, add(b, h), add(a, h)];
                joined[0] = existing(pa);
                if cross(side, h) < 0.0 {
                    joined[3] = existing(pb);
                    ([corners[0], corners[3], corners[2], corners[1]], false)
                } else {
                    joined[1] = existing(pb);
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
        let mut points = Vec::with_capacity(4);
        for corner in joined {
            points.push(match corner {
                Some(p) => p,
                None => self.id(),
            });
        }
        for i in 0..4 {
            let mut e = Entity::new(lines[i], line(points[i], points[(i + 1) % 4]));
            e.construction = construction;
            self.push(e);
        }
        for ((point, at), corner) in points.iter().zip(corners).zip(joined) {
            if corner.is_none() {
                self.push(Entity::point(*point, at));
            }
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
            // A construction diagonal with the centre at its middle (an
            // existing point given as the centre).
            let c = self.point(center)?;
            let diagonal = self.curve_entity(line(points[0], points[2]), true);
            self.add_constraint(ConstraintKind::Midpoint {
                point: c,
                curve: diagonal,
            });
        }
        Ok(lines)
    }

    pub fn circle(&mut self, mode: CircleMode, construction: bool) -> Result<EntityUid, String> {
        // Existing points the circle passes through (mitcad#1269).
        let mut on: Vec<EntityUid> = Vec::new();
        let (center, radius, tangents): (PointInput, f64, Vec<EntityUid>) = match mode {
            CircleMode::CenterRadius { center, radius } => (center, radius, Vec::new()),
            CircleMode::TwoPoint { a: pa, b: pb } => {
                let (a, b) = (self.input_position(pa)?, self.input_position(pb)?);
                on.extend([pa, pb].into_iter().filter_map(existing));
                (
                    PointInput::At(scale(add(a, b), 0.5)),
                    dist(a, b) / 2.0,
                    Vec::new(),
                )
            }
            CircleMode::ThreePoint {
                a: pa,
                b: pb,
                c: pc,
            } => {
                let (a, b, c) = (
                    self.input_position(pa)?,
                    self.input_position(pb)?,
                    self.input_position(pc)?,
                );
                on.extend([pa, pb, pc].into_iter().filter_map(existing));
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
        let mut e = Entity::new(
            id,
            EntityKind::Circle {
                center,
                radius,
                start: 0.0,
            },
        );
        e.construction = construction;
        self.push(e);
        for t in tangents {
            self.curve(t)?;
            self.add_constraint(ConstraintKind::tangent(t, id));
        }
        self.hold_on(&on, id);
        Ok(id)
    }

    /// Holds existing points on a new curve through them (mitcad#1269: a
    /// circle's or an arc's point given as an existing point); a point
    /// given twice is held once.
    fn hold_on(&mut self, points: &[EntityUid], curve: EntityUid) {
        let mut held: Vec<EntityUid> = Vec::new();
        for p in points {
            if !held.contains(p) {
                held.push(*p);
                self.add_constraint(ConstraintKind::Coincident {
                    point: *p,
                    entity: super::Ref::Curve(curve),
                });
            }
        }
    }

    /// The curve that ends at `from` and the direction it leaves `from`
    /// with (outwards); construction curves only for a construction arc.
    fn leaving(&self, from: EntityUid, construction: bool) -> Result<(EntityUid, P2), String> {
        let solved = self.solved()?;
        self.def
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
            .ok_or_else(|| format!("no curve ends at p{}", from.0))
    }

    /// The arc from the point `from` at `p` leaving along the unit
    /// direction `t` to the point `end` at `pe` (not straight ahead): a new
    /// centre point, and the arc counter-clockwise either way round.
    fn leaving_arc(
        &mut self,
        from: EntityUid,
        end: EntityUid,
        t: P2,
        p: P2,
        pe: P2,
    ) -> (EntityUid, EntityKind) {
        let n = perp(t);
        let chord = sub(pe, p);
        let r = dot(chord, chord) / (2.0 * dot(chord, n));
        let center = self.new_point(add(p, scale(n, r)));
        let kind = if r > 0.0 {
            EntityKind::Arc {
                center,
                start: from,
                end,
            }
        } else {
            EntityKind::Arc {
                center,
                start: end,
                end: from,
            }
        };
        (center, kind)
    }

    pub fn arc(&mut self, mode: ArcMode, construction: bool) -> Result<EntityUid, String> {
        let id = self.id();
        // An existing point the arc passes through (mitcad#1269).
        let mut on: Vec<EntityUid> = Vec::new();
        let entity = match mode {
            ArcMode::ThreePoint {
                start,
                through: pt,
                end,
            } => {
                let through = self.input_position(pt)?;
                on.extend(existing(pt));
                let (s, e) = (self.point(start)?, self.point(end)?);
                let (ps, pe) = (self.at(s)?, self.at(e)?);
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
                let (curve, direction) = self.leaving(from, construction)?;
                let e = self.point(end)?;
                let pe = self.at(e)?;
                let d = unit(direction);
                let chord = sub(pe, p);
                if dot(chord, perp(d)).abs() <= 1e-12 * norm(chord).max(1e-300) {
                    return Err("the end is straight ahead; draw a line".to_owned());
                }
                let (_, kind) = self.leaving_arc(from, e, d, p, pe);
                self.add_constraint(ConstraintKind::tangent(curve, id));
                kind
            }
            ArcMode::Normal { from, end, toward } => {
                let p = self.at(from)?;
                let (curve, direction) = self.leaving(from, construction)?;
                // What holds the arc square to the curve: its centre on the
                // line, or a construction radius of the arc it leaves that
                // the new arc is tangent to at `from`.
                let previous_center = match self.def.entity(curve).map(|c| &c.kind) {
                    Some(EntityKind::Line { .. }) => None,
                    Some(EntityKind::Arc { center, .. }) => Some(*center),
                    _ => {
                        return Err(format!(
                            "a normal arc starts from a line or a circular arc; c{} is neither",
                            curve.0
                        ));
                    }
                };
                let e = self.point(end)?;
                let pe = self.at(e)?;
                let d = unit(direction);
                let chord = sub(pe, p);
                if dot(chord, d).abs() <= 1e-12 * norm(chord).max(1e-300) {
                    return Err("the end is straight across; draw a line".to_owned());
                }
                // It leaves `from` across the curve, toward the side
                // `toward` is on, else the end's side (mitcad#1608: past
                // 180 degrees the end is behind the curve).
                if let Some(toward) = toward {
                    finite(&[toward])?;
                }
                let side = toward.map_or(chord, |toward| sub(toward, p));
                let across = dot(side, perp(d));
                let t = scale(perp(d), if across < 0.0 { -1.0 } else { 1.0 });
                let (center, kind) = self.leaving_arc(from, e, t, p, pe);
                let hold = match previous_center {
                    None => ConstraintKind::Coincident {
                        point: center,
                        entity: super::Ref::Curve(curve),
                    },
                    Some(c) => {
                        let radius = self.curve_entity(line(c, from), true);
                        ConstraintKind::tangent(radius, id)
                    }
                };
                self.add_constraint(hold);
                kind
            }
        };
        let mut e = Entity::new(id, entity);
        e.construction = construction;
        self.push(e);
        self.hold_on(&on, id);
        Ok(id)
    }

    /// A regular polygon around `center`: `inscribed` puts the corners on a
    /// construction circle through `vertex`, with equal sides; otherwise
    /// `vertex` is the middle of a side, the sides touch a construction
    /// circle through it and the corners lie on a second one around the
    /// same centre point.
    ///
    /// Either set leaves exactly the placement, the orientation and the
    /// size free (four degrees of freedom), so dragging or turning keeps
    /// the polygon regular (mitcad#303). Equal sides tangent to one circle
    /// alone are not enough: with an even number of sides the touching
    /// points can slide off the sides' middles in turn (alternate tangent
    /// lengths `a`, `L - a`), which skews the polygon.
    ///
    /// An existing point as `center` is the polygon's centre point, and as
    /// the `vertex` of an inscribed polygon its first corner (mitcad#1269);
    /// a circumscribed polygon's `vertex`, the middle of a side, is a
    /// position only.
    pub fn polygon(
        &mut self,
        center_input: PointInput,
        vertex_input: PointInput,
        sides: usize,
        inscribed: bool,
        construction: bool,
    ) -> Result<Vec<EntityUid>, String> {
        let center = self.input_position(center_input)?;
        let vertex = self.input_position(vertex_input)?;
        let first_corner = existing(vertex_input);
        if !inscribed && first_corner.is_some() {
            return Err(
                "the vertex of a circumscribed polygon is the middle of a side, a position \
                 [x, y]: a point id is taken as the vertex of an inscribed polygon (its first \
                 corner)"
                    .to_owned(),
            );
        }
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
        let mut corners = Vec::with_capacity(sides);
        for i in 0..sides {
            corners.push(match first_corner {
                Some(p) if i == 0 => p,
                _ => self.id(),
            });
        }
        for i in 0..sides {
            let mut e = Entity::new(lines[i], line(corners[i], corners[(i + 1) % sides]));
            e.construction = construction;
            self.push(e);
        }
        for (i, id) in corners.iter().enumerate() {
            if i == 0 && first_corner.is_some() {
                continue; // the existing point
            }
            let a = first + TAU * i as f64 / n;
            self.push(Entity::point(
                *id,
                [center[0] + big * a.cos(), center[1] + big * a.sin()],
            ));
        }
        // The centre point: the existing one given, or a new one.
        let c = self.point(center_input)?;
        let circle = self.curve_entity(
            EntityKind::Circle {
                center: c,
                radius: if inscribed { big } else { r },
                start: 0.0,
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
            for w in lines.windows(2) {
                self.add_constraint(ConstraintKind::Equal { a: w[0], b: w[1] });
            }
        } else {
            // The corners' circle shares the centre point; each side is a
            // chord of it touching the inner circle, so all sides are
            // equally long and turn by the same angle.
            let around = self.curve_entity(
                EntityKind::Circle {
                    center: c,
                    radius: big,
                    start: 0.0,
                },
                true,
            );
            for l in &lines {
                self.add_constraint(ConstraintKind::tangent(*l, circle));
            }
            for p in &corners {
                self.add_constraint(ConstraintKind::Coincident {
                    point: *p,
                    entity: super::Ref::Curve(around),
                });
            }
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
                let (pc, pe) = (self.input_position(center)?, self.input_position(end)?);
                let a = sub(scale(pc, 2.0), pe);
                self.straight_slot(PointInput::At(a), end, r, Some(center))
            }
            SlotMode::Arc { center, start, end } => self.arc_slot(center, start, end, r),
        }
    }

    /// A straight slot between the end arcs' centres `a` and `b` (existing
    /// points or new ones), with `middle` held at the middle of its centre
    /// line.
    fn straight_slot(
        &mut self,
        a_input: PointInput,
        b_input: PointInput,
        r: f64,
        middle: Option<PointInput>,
    ) -> Result<Vec<EntityUid>, String> {
        let (a, b) = (self.input_position(a_input)?, self.input_position(b_input)?);
        finite(&[a, b])?;
        let length = dist(a, b);
        if length == 0.0 {
            return Err("the slot has no length".to_owned());
        }
        let d = unit(sub(b, a));
        let n = perp(d);
        let curves = self.ids(4); // top, arc at b, bottom, arc at a
        let ca = self.point(a_input)?;
        let cb = self.point(b_input)?;
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
            self.add_constraint(ConstraintKind::tangent(curves[l], curves[arc]));
        }
        self.add_constraint(ConstraintKind::Equal {
            a: curves[1],
            b: curves[3],
        });
        if let Some(m) = middle {
            let m = self.point(m)?;
            self.add_constraint(ConstraintKind::Midpoint {
                point: m,
                curve: axis,
            });
        }
        Ok(curves)
    }

    /// An arc slot around `center` from `start` (existing points or new
    /// ones: the centre and the start's end arc centre) to the direction of
    /// `end`.
    fn arc_slot(
        &mut self,
        center_input: PointInput,
        start_input: PointInput,
        end: [f64; 2],
        r: f64,
    ) -> Result<Vec<EntityUid>, String> {
        let center = self.input_position(center_input)?;
        let start = self.input_position(start_input)?;
        finite(&[end])?;
        let big = dist(center, start);
        if big <= r {
            return Err("the slot's arc must be larger than half the width".to_owned());
        }
        let a0 = (start[1] - center[1]).atan2(start[0] - center[0]);
        let a1 = angle_after(a0, (end[1] - center[1]).atan2(end[0] - center[0]));
        let at = |radius: f64, a: f64| [center[0] + radius * a.cos(), center[1] + radius * a.sin()];
        let curves = self.ids(4); // outer, cap at end, inner, cap at start
        let c = self.point(center_input)?;
        let es = match existing(start_input) {
            Some(p) => p,
            None => self.new_point(at(big, a0)),
        };
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
            self.add_constraint(ConstraintKind::tangent(curves[side], curves[cap]));
        }
        self.add_constraint(ConstraintKind::Equal {
            a: curves[1],
            b: curves[3],
        });
        Ok(curves)
    }

    /// An ellipse or an elliptical arc; `major`, the end of the major axis,
    /// may be an existing point as `center` may (mitcad#1269).
    pub fn ellipse(
        &mut self,
        center: PointInput,
        major: PointInput,
        minor_radius: f64,
        arc: Option<(PointInput, PointInput)>,
        construction: bool,
    ) -> Result<EntityUid, String> {
        if !(minor_radius.is_finite() && minor_radius > 0.0) {
            return Err(format!(
                "the minor radius must be positive, got {minor_radius}"
            ));
        }
        self.input_position(major)?;
        let id = self.id();
        let c = self.point(center)?;
        let m = self.point(major)?;
        if c == m {
            return Err("the major axis has no length".to_owned());
        }
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
    /// `closed` (mitcad#783): a fit point spline that runs on from its last
    /// point back to the first, smooth through it (at least three points).
    pub fn spline(
        &mut self,
        points: &[PointInput],
        degree: Option<u32>,
        closed: bool,
        construction: bool,
    ) -> Result<EntityUid, String> {
        if closed && degree.is_some() {
            return Err("only a fit point spline is closed".to_owned());
        }
        if closed && points.len() < 3 {
            return Err(format!(
                "a closed spline needs at least three points, got {}",
                points.len()
            ));
        }
        let id = self.id();
        let points = points
            .iter()
            .map(|p| self.point(*p))
            .collect::<Result<Vec<_>, _>>()?;
        let kind = match degree {
            None => EntityKind::FittedSpline { points, closed },
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
