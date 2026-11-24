// SPDX-License-Identifier: MIT
//! Insert Point on a spline (mitcad#836): a point where the spline is
//! nearest to a place picked near it.
//!
//! - A fitted spline gets a fit point there, between its two neighbours in
//!   the curve's order (a closed one between its last point and its first
//!   where the pick is on that stretch). The curve still passes through
//!   every fit point; between the new point's neighbours its shape may
//!   change a little, as the interpolation now has one more point.
//! - A spline by control points gets a control point by knot insertion
//!   ([`Nurbs::insert_knot`]): the same curve, exactly, with one more
//!   control point. The control points next to the new one move to keep
//!   the shape; their ids, and so their constraints and dimensions, stay.
//!
//! Closed splines work alike. Every existing point keeps its id, its
//! constraints and its dimensions.
//!
//! Remove Point (mitcad#2326) is its pair: a spline's own point leaves the
//! spline, which stays.
//!
//! - A fitted spline is fitted again through the rest of its points; an
//!   end point of an open one makes the next point the end.
//! - A spline by control points loses the control point and one interior
//!   knot ([`Nurbs::remove_control_point`]): the change stays within that
//!   knot's spans, and a point just inserted goes again exactly.
//!
//! Constraints and dimensions on the removed point go with it, as does a
//! tangent or smooth constraint at a removed end (its `at` names the
//! point); the others stay.

use std::collections::BTreeSet;

use super::EntityKind;
use super::edit::SketchEdit;
use super::geometry::{Curve2, Nurbs, P2, dist};
use super::solve::Solved;
use crate::ids::EntityUid;

impl SketchEdit<'_> {
    /// Inserts a point on the spline `id` where it is nearest to `at` and
    /// returns the new point (see the module documentation). Refused for a
    /// curve that is no spline, a projected spline or one a derived offset
    /// computes, a place on an existing fit point or at the ends of a
    /// spline by control points, and where the knot is there as often as
    /// the degree (the curve passes through a control point there already).
    pub fn insert_spline_point(&mut self, id: EntityUid, at: P2) -> Result<EntityUid, String> {
        if !at.iter().all(|v| v.is_finite()) {
            return Err(format!("the place must be finite, got {at:?}"));
        }
        let kind = self.curve(id)?.kind.clone();
        if !matches!(
            kind,
            EntityKind::Spline { .. } | EntityKind::FittedSpline { .. }
        ) {
            return Err(format!(
                "c{} is a {}: points are inserted into splines only",
                id.0,
                kind.type_name()
            ));
        }
        self.shape_editable(id)?;
        let solved = self.solved()?;
        let curve = solved
            .curves
            .get(&id)
            .cloned()
            .ok_or_else(|| format!("curve c{} is not solved", id.0))?;
        let Curve2::Nurbs(spline) = &curve else {
            return Err(format!("c{} is not solved as a spline", id.0));
        };
        let (t, _) = curve.closest(at);
        match kind {
            EntityKind::FittedSpline { points, closed } => {
                self.insert_fit_point(id, &solved, spline, &points, closed, t)
            }
            EntityKind::Spline { control, .. } => {
                self.insert_control_point(id, spline, &control, t)
            }
            _ => unreachable!("a spline"),
        }
    }

    /// A fit point at the parameter `t` of the solved fitted spline.
    fn insert_fit_point(
        &mut self,
        id: EntityUid,
        solved: &Solved,
        spline: &Nurbs,
        points: &[EntityUid],
        closed: bool,
        t: f64,
    ) -> Result<EntityUid, String> {
        if points.len() < 2 {
            return Err(format!("c{} has fewer than two fit points", id.0));
        }
        let mut fit = points
            .iter()
            .map(|p| {
                solved
                    .points
                    .get(p)
                    .copied()
                    .ok_or_else(|| format!("point p{} is not solved", p.0))
            })
            .collect::<Result<Vec<P2>, String>>()?;
        if closed {
            // On from the last point back to the first.
            fit.push(fit[0]);
        }
        // The fit points' parameters: chord lengths over the domain, as the
        // solver interpolates them (`mitcad_solver`'s `interpolate`).
        let mut total = 0.0;
        let mut lengths = vec![0.0];
        for w in fit.windows(2) {
            total += dist(w[0], w[1]);
            lengths.push(total);
        }
        if total <= 0.0 {
            return Err(format!("c{} has no length", id.0));
        }
        let (lo, hi) = spline.domain();
        let params: Vec<f64> = lengths.iter().map(|s| lo + (hi - lo) * s / total).collect();
        // The stretch between the fit points `i` and `i + 1` that holds `t`.
        let i = params[1..]
            .iter()
            .position(|end| t < *end)
            .unwrap_or(params.len() - 2);
        let on = spline.eval(t)[0];
        let tolerance = 1e-9 * total.max(1.0);
        if dist(on, fit[i]) <= tolerance || dist(on, fit[i + 1]) <= tolerance {
            return Err("a fit point is there already".to_owned());
        }
        let point = self.add_point(on);
        if let Some(EntityKind::FittedSpline { points, .. }) =
            self.def.entity_mut(id).map(|e| &mut e.kind)
        {
            // After the last point of a closed spline is before its first.
            points.insert(i + 1, point);
        }
        Ok(point)
    }

    /// A control point by knot insertion at the parameter `t` of the solved
    /// spline.
    fn insert_control_point(
        &mut self,
        id: EntityUid,
        spline: &Nurbs,
        control: &[EntityUid],
        t: f64,
    ) -> Result<EntityUid, String> {
        if control.len() != spline.control.len() {
            return Err(format!("c{} is not solved with its control points", id.0));
        }
        let (lo, hi) = spline.domain();
        if (t - lo).abs() <= 1e-9 * (hi - lo) || (hi - t).abs() <= 1e-9 * (hi - lo) {
            return Err(
                "the place is at an end of the spline (where a closed one closes): \
                 pick a place between its ends"
                    .to_owned(),
            );
        }
        let refined = spline
            .insert_knot(t)
            .ok_or_else(|| "the spline passes through a control point there already".to_owned())?;
        // The old points that move: one the spline uses twice cannot be in
        // two places.
        let moved = &control[refined.first..refined.last];
        if let Some(twice) = moved
            .iter()
            .find(|p| control.iter().filter(|q| q == p).count() > 1)
        {
            return Err(format!(
                "p{} is twice a control point of c{} and cannot move to keep its shape",
                twice.0, id.0
            ));
        }
        let new = &refined.spline;
        // Of the new places, the one nearest to the curve at the knot is
        // the new point's; the moved points keep their order around it.
        let on = new.eval(refined.knot)[0];
        let fresh = (refined.first..=refined.last)
            .min_by(|a, b| dist(new.control[*a], on).total_cmp(&dist(new.control[*b], on)))
            .expect("one new place at least");
        let point = self.add_point(new.control[fresh]);
        let mut ids = Vec::with_capacity(control.len() + 1);
        ids.extend_from_slice(&control[..fresh]);
        ids.push(point);
        ids.extend_from_slice(&control[fresh..]);
        for (uid, at) in ids
            .iter()
            .zip(&new.control)
            .take(refined.last + 1)
            .skip(refined.first)
        {
            if let Some(EntityKind::Point { at: place }) =
                self.def.entity_mut(*uid).map(|e| &mut e.kind)
            {
                *place = *at;
            }
        }
        let e = self.def.entity_mut(id).expect("a curve");
        if let EntityKind::Spline {
            control,
            weights,
            knots,
            ..
        } = &mut e.kind
        {
            *control = ids;
            *weights = new.weights.clone();
            *knots = new.knots.clone();
        }
        Ok(point)
    }
}

// Remove Point on a spline (mitcad#2326).
impl SketchEdit<'_> {
    /// Removes the point `point` from the spline `id`, which stays (see the
    /// module documentation). Refused, with the reason in words, for a
    /// curve that is no spline, a point it does not list, a point another
    /// curve uses too, a projected spline or one a derived offset computes,
    /// the one point where both ends of a spline by control points are, and
    /// where fewer points would remain than the spline needs (an open
    /// fitted spline 2, a closed one 3, a spline by control points its
    /// degree + 1). A pattern's spline is first freed into plain sketch
    /// geometry, as Trim frees it.
    pub fn remove_spline_point(&mut self, id: EntityUid, point: EntityUid) -> Result<(), String> {
        let kind = self.curve(id)?.kind.clone();
        let listed = match &kind {
            EntityKind::FittedSpline { points, .. } => points.clone(),
            EntityKind::Spline { control, .. } => control.clone(),
            _ => {
                return Err(format!(
                    "c{} is a {}: points are removed from splines only",
                    id.0,
                    kind.type_name()
                ));
            }
        };
        self.shape_editable(id)?;
        let times = listed.iter().filter(|p| **p == point).count();
        if times == 0 {
            return Err(format!("p{} is not a point of c{}", point.0, id.0));
        }
        if let Some(other) = self
            .def
            .entities
            .iter()
            .find(|e| e.id != id && e.kind.points().contains(&point))
        {
            return Err(format!(
                "p{} is also a point of {}: it can only be deleted with the curves that use it",
                point.0,
                other.as_ref()
            ));
        }
        if times > 1 {
            let closes = listed.first() == Some(&point) && listed.last() == Some(&point);
            return Err(if closes {
                format!(
                    "p{} is where c{} closes, both its ends: the spline cannot lose it",
                    point.0, id.0
                )
            } else {
                format!(
                    "p{} is listed more than once in c{}: the spline cannot lose it",
                    point.0, id.0
                )
            });
        }
        let (least, what) = match &kind {
            EntityKind::FittedSpline { closed: false, .. } => (2, "an open spline".to_owned()),
            EntityKind::FittedSpline { closed: true, .. } => (3, "a closed spline".to_owned()),
            EntityKind::Spline { degree, .. } => (
                *degree as usize + 1,
                format!("a spline of degree {degree} by control points"),
            ),
            _ => unreachable!("a spline"),
        };
        if listed.len() <= least {
            return Err(format!(
                "c{} would keep {} points, and {what} needs {least}: delete the spline instead",
                id.0,
                listed.len() - 1
            ));
        }
        match kind {
            EntityKind::FittedSpline { .. } => {
                if let Some(EntityKind::FittedSpline { points, .. }) =
                    self.def.entity_mut(id).map(|e| &mut e.kind)
                {
                    points.retain(|p| *p != point);
                }
            }
            EntityKind::Spline { control, .. } => {
                let solved = self.solved()?;
                let curve = solved
                    .curves
                    .get(&id)
                    .ok_or_else(|| format!("curve c{} is not solved", id.0))?;
                let Curve2::Nurbs(spline) = curve else {
                    return Err(format!("c{} is not solved as a spline", id.0));
                };
                self.remove_control_point(id, spline, &control, point)?;
            }
            _ => unreachable!("a spline"),
        }
        // A pattern makes its curves again from their originals: its
        // spline is plain sketch geometry from now on (mitcad#2007).
        self.free_patterns_of(&[id]);
        // Its constraints and dimensions with it: a tangent or smooth at a
        // removed end names it (`at`). One without `at` touches the spline
        // anywhere or holds an end it shares with the other curve, which
        // was refused above, so it stays.
        self.drop_entities(&BTreeSet::from([point]));
        Ok(())
    }

    /// The control point `point` and one knot out of the spline by control
    /// points `id`, solved as `spline`.
    fn remove_control_point(
        &mut self,
        id: EntityUid,
        spline: &Nurbs,
        control: &[EntityUid],
        point: EntityUid,
    ) -> Result<(), String> {
        if control.len() != spline.control.len() {
            return Err(format!("c{} is not solved with its control points", id.0));
        }
        let m = control
            .iter()
            .position(|p| *p == point)
            .expect("a listed point");
        let removal = spline.remove_control_point(m).ok_or_else(|| {
            format!(
                "c{} has no knot between its ends to remove with p{}",
                id.0, point.0
            )
        })?;
        // A point the spline uses twice cannot move to two places.
        if let Some(twice) = removal
            .moved
            .iter()
            .map(|i| control[*i])
            .find(|p| control.iter().filter(|q| *q == p).count() > 1)
        {
            return Err(format!(
                "p{} is twice a control point of c{} and cannot move to keep its shape",
                twice.0, id.0
            ));
        }
        let new = &removal.spline;
        for i in &removal.moved {
            let at = new.control[if *i < m { *i } else { *i - 1 }];
            if let Some(EntityKind::Point { at: place }) =
                self.def.entity_mut(control[*i]).map(|e| &mut e.kind)
            {
                *place = at;
            }
        }
        let e = self.def.entity_mut(id).expect("a curve");
        if let EntityKind::Spline {
            control: ids,
            weights,
            knots,
            ..
        } = &mut e.kind
        {
            ids.remove(m);
            *weights = new.weights.clone();
            *knots = new.knots.clone();
        }
        Ok(())
    }
}
