// SPDX-License-Identifier: MIT
//! The constraint system: storage of points, entities and constraints, and
//! the editing operations.

use std::f64::consts::PI;
use std::sync::Arc;

use crate::prepare::Prepared;
use crate::scalar::{V2, wrap_angle};
use crate::spline::{SplineBasis, clamped_uniform_knots, interpolate};
use crate::types::*;

/// Kind of a scalar unknown; decides its weight and how it is kept in range.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum ParamKind {
    /// A coordinate or a radius (millimetres).
    Length,
    /// A spline parameter, kept inside the parameter range.
    SplineT { lo: f64, hi: f64 },
    /// An ellipse angle parameter.
    Angle,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct PointData {
    pub x: usize,
    pub y: usize,
    /// Internal control point of a fitted spline (not addressable by callers).
    pub hidden: bool,
}

#[derive(Clone, Debug)]
pub(crate) enum EntityData {
    Line {
        p1: usize,
        p2: usize,
    },
    Circle {
        c: usize,
        r: usize,
    },
    Arc {
        c: usize,
        s: usize,
        e: usize,
    },
    Ellipse {
        c: usize,
        a: usize,
        b: usize,
    },
    EllipticalArc {
        c: usize,
        a: usize,
        b: usize,
        s: usize,
        e: usize,
        ts: usize,
        te: usize,
    },
    BSpline {
        basis: Arc<SplineBasis>,
        ctrl: Vec<usize>,
    },
    Fitted {
        basis: Arc<SplineBasis>,
        fit: Vec<usize>,
        /// All control points: `fit[0]`, hidden points, `fit[last]`.
        ctrl: Vec<usize>,
        params: Vec<f64>,
        ends: [SplineEnd; 2],
    },
}

impl EntityData {
    pub fn kind(&self) -> EntityKind {
        match self {
            EntityData::Line { .. } => EntityKind::Line,
            EntityData::Circle { .. } => EntityKind::Circle,
            EntityData::Arc { .. } => EntityKind::Arc,
            EntityData::Ellipse { .. } => EntityKind::Ellipse,
            EntityData::EllipticalArc { .. } => EntityKind::EllipticalArc,
            EntityData::BSpline { .. } => EntityKind::BSpline,
            EntityData::Fitted { .. } => EntityKind::FittedSpline,
        }
    }

    /// Points that define the entity (for splines: the control points, for
    /// fitted splines: the fit points followed by the hidden control points).
    pub fn points(&self) -> Vec<usize> {
        match self {
            EntityData::Line { p1, p2 } => vec![*p1, *p2],
            EntityData::Circle { c, .. } => vec![*c],
            EntityData::Arc { c, s, e } => vec![*c, *s, *e],
            EntityData::Ellipse { c, a, .. } => vec![*c, *a],
            EntityData::EllipticalArc { c, a, s, e, .. } => vec![*c, *a, *s, *e],
            EntityData::BSpline { ctrl, .. } => ctrl.clone(),
            EntityData::Fitted { fit, ctrl, .. } => {
                let mut v = fit.clone();
                v.extend_from_slice(&ctrl[1..ctrl.len() - 1]);
                v
            }
        }
    }

    /// Scalar unknowns of the entity (radius, minor radius).
    pub fn scalars(&self) -> Vec<usize> {
        match self {
            EntityData::Circle { r, .. } => vec![*r],
            EntityData::Ellipse { b, .. } | EntityData::EllipticalArc { b, .. } => vec![*b],
            _ => Vec::new(),
        }
    }

    /// End points of an open curve.
    pub fn ends(&self) -> Option<[usize; 2]> {
        match self {
            EntityData::Line { p1, p2 } => Some([*p1, *p2]),
            EntityData::Arc { s, e, .. } | EntityData::EllipticalArc { s, e, .. } => Some([*s, *e]),
            EntityData::BSpline { ctrl, .. } | EntityData::Fitted { ctrl, .. } => {
                Some([ctrl[0], ctrl[ctrl.len() - 1]])
            }
            _ => None,
        }
    }

    pub fn centre(&self) -> Option<usize> {
        match self {
            EntityData::Circle { c, .. }
            | EntityData::Arc { c, .. }
            | EntityData::Ellipse { c, .. }
            | EntityData::EllipticalArc { c, .. } => Some(*c),
            _ => None,
        }
    }

    /// Basis and all control points of a spline.
    pub fn spline(&self) -> Option<(&Arc<SplineBasis>, &[usize])> {
        match self {
            EntityData::BSpline { basis, ctrl } | EntityData::Fitted { basis, ctrl, .. } => {
                Some((basis, ctrl))
            }
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct EntityRec {
    pub data: EntityData,
    pub seq: u64,
}

#[derive(Clone, Debug)]
pub(crate) struct ConstraintRec {
    pub def: Constraint,
    pub seq: u64,
    /// Orientation sign chosen from the geometry when added (side of a
    /// distance or tangency; for circle tangency 0 = external, otherwise the
    /// sign of `r_a - r_b` for internal).
    pub side: f64,
    /// Angle dimensions: +1, or -1 when the second line's direction is
    /// reversed to pick the quadrant.
    pub flip: f64,
    /// Symmetric lines: the first end point of `a` pairs with the second of `b`.
    pub swap: bool,
    /// Internal unknowns (curve parameters) owned by the constraint.
    pub params: Vec<usize>,
    /// Value at which the dimension was last satisfied (for continuation).
    pub last_value: f64,
}

/// Spline geometry for the caller (e.g. to build the kernel curve).
#[derive(Clone, Debug, PartialEq)]
pub struct SplineGeometry {
    pub degree: usize,
    pub knots: Vec<f64>,
    pub weights: Vec<f64>,
    pub control_points: Vec<[f64; 2]>,
}

/// A 2D sketch constraint system. See the crate documentation for an
/// example.
#[derive(Clone, Debug, Default)]
pub struct System {
    pub(crate) params: Vec<f64>,
    pub(crate) weights: Vec<f64>,
    pub(crate) pkind: Vec<ParamKind>,
    pub(crate) points: Vec<Option<PointData>>,
    pub(crate) entities: Vec<Option<EntityRec>>,
    pub(crate) constraints: Vec<Option<ConstraintRec>>,
    seq: u64,
    pub(crate) prepared: Option<Prepared>,
}

const HIDDEN_WEIGHT: f64 = 1e-2;

impl System {
    pub fn new() -> Self {
        Self::default()
    }

    fn next_seq(&mut self) -> u64 {
        self.seq += 1;
        self.seq
    }

    fn invalidate(&mut self) {
        self.prepared = None;
    }

    pub(crate) fn new_param(&mut self, v: f64, kind: ParamKind, weight: f64) -> usize {
        self.params.push(v);
        self.pkind.push(kind);
        self.weights.push(weight);
        self.params.len() - 1
    }

    fn new_point(&mut self, x: f64, y: f64, hidden: bool) -> usize {
        let w = if hidden { HIDDEN_WEIGHT } else { 1.0 };
        let px = self.new_param(x, ParamKind::Length, w);
        let py = self.new_param(y, ParamKind::Length, w);
        self.points.push(Some(PointData {
            x: px,
            y: py,
            hidden,
        }));
        self.invalidate();
        self.points.len() - 1
    }

    /// Adds a free point (2 degrees of freedom).
    pub fn add_point(&mut self, x: f64, y: f64) -> PointId {
        PointId(self.new_point(x, y, false) as u32)
    }

    pub(crate) fn pidx(&self, p: PointId) -> Result<usize, Error> {
        match self.points.get(p.index()) {
            Some(Some(d)) if !d.hidden => Ok(p.index()),
            _ => Err(Error::UnknownPoint(p)),
        }
    }

    pub(crate) fn pd(&self, p: usize) -> PointData {
        self.points[p].expect("live point")
    }

    pub(crate) fn xy(&self, p: usize) -> V2<f64> {
        let d = self.pd(p);
        V2::new(self.params[d.x], self.params[d.y])
    }

    pub(crate) fn ent(&self, e: EntityId) -> Result<&EntityData, Error> {
        match self.entities.get(e.index()) {
            Some(Some(r)) => Ok(&r.data),
            _ => Err(Error::UnknownEntity(e)),
        }
    }

    fn add_entity(&mut self, data: EntityData) -> EntityId {
        let seq = self.next_seq();
        self.entities.push(Some(EntityRec { data, seq }));
        self.invalidate();
        EntityId(self.entities.len() as u32 - 1)
    }

    /// Adds a line between two points (4 degrees of freedom when the points
    /// are free).
    pub fn add_line(&mut self, p1: PointId, p2: PointId) -> Result<EntityId, Error> {
        let (a, b) = (self.pidx(p1)?, self.pidx(p2)?);
        if a == b {
            return Err(Error::InvalidValue("a line needs two distinct points"));
        }
        Ok(self.add_entity(EntityData::Line { p1: a, p2: b }))
    }

    /// Adds a circle with a centre point and a radius (3 degrees of freedom).
    pub fn add_circle(&mut self, centre: PointId, radius: f64) -> Result<EntityId, Error> {
        let c = self.pidx(centre)?;
        if !positive(radius) {
            return Err(Error::InvalidValue("radius must be positive"));
        }
        let r = self.new_param(radius, ParamKind::Length, 1.0);
        Ok(self.add_entity(EntityData::Circle { c, r }))
    }

    /// Adds an arc counter-clockwise from `start` to `end` around `centre`.
    /// The arc keeps its end points at equal distance from the centre (5
    /// degrees of freedom).
    pub fn add_arc(
        &mut self,
        centre: PointId,
        start: PointId,
        end: PointId,
    ) -> Result<EntityId, Error> {
        let (c, s, e) = (self.pidx(centre)?, self.pidx(start)?, self.pidx(end)?);
        if c == s || c == e || s == e {
            return Err(Error::InvalidValue("an arc needs three distinct points"));
        }
        Ok(self.add_entity(EntityData::Arc { c, s, e }))
    }

    /// Adds an ellipse by its centre, the end point of its major axis
    /// (`centre + major_radius * direction`) and its minor radius (5 degrees
    /// of freedom).
    pub fn add_ellipse(
        &mut self,
        centre: PointId,
        major: PointId,
        minor_radius: f64,
    ) -> Result<EntityId, Error> {
        let (c, a) = (self.pidx(centre)?, self.pidx(major)?);
        if c == a || !positive(minor_radius) {
            return Err(Error::InvalidValue("ellipse axes must be positive"));
        }
        let b = self.new_param(minor_radius, ParamKind::Length, 1.0);
        Ok(self.add_entity(EntityData::Ellipse { c, a, b }))
    }

    /// Adds an elliptical arc (ellipse as in [`System::add_ellipse`]) from
    /// `start` to `end`, counter-clockwise in the ellipse parameter. The end
    /// points stay on the ellipse (7 degrees of freedom).
    pub fn add_elliptical_arc(
        &mut self,
        centre: PointId,
        major: PointId,
        minor_radius: f64,
        start: PointId,
        end: PointId,
    ) -> Result<EntityId, Error> {
        let (c, a) = (self.pidx(centre)?, self.pidx(major)?);
        let (s, e) = (self.pidx(start)?, self.pidx(end)?);
        if c == a || !positive(minor_radius) {
            return Err(Error::InvalidValue("ellipse axes must be positive"));
        }
        if s == e {
            return Err(Error::InvalidValue(
                "an elliptical arc needs distinct end points",
            ));
        }
        let b = self.new_param(minor_radius, ParamKind::Length, 1.0);
        let ts0 = self.ellipse_angle(c, a, b, s);
        let te0 = self.ellipse_angle(c, a, b, e);
        let w = self.ellipse_param_weight(c, a);
        let ts = self.new_param(ts0, ParamKind::Angle, w);
        let te = self.new_param(te0, ParamKind::Angle, w);
        Ok(self.add_entity(EntityData::EllipticalArc {
            c,
            a,
            b,
            s,
            e,
            ts,
            te,
        }))
    }

    fn ellipse_param_weight(&self, c: usize, a: usize) -> f64 {
        let size = self.xy(a).sub(self.xy(c)).norm();
        (0.1 * size).powi(2).max(1e-6)
    }

    /// Ellipse parameter of the point nearest to `p` (approximately).
    fn ellipse_angle(&self, c: usize, a: usize, b: usize, p: usize) -> f64 {
        let cv = self.xy(c);
        let av = self.xy(a).sub(cv);
        let big = av.norm();
        let u = av.unit();
        let d = self.xy(p).sub(cv);
        let minor = self.params[b];
        (d.dot(u.perp()) / minor).atan2(d.dot(u) / big)
    }

    /// Adds a B-spline by control points. `weights` default to 1 (a
    /// non-rational spline) and `knots` to a clamped uniform knot vector on
    /// [0, 1]. Degree, weights and knots stay fixed; the control points move.
    pub fn add_bspline(
        &mut self,
        degree: usize,
        control_points: &[PointId],
        weights: Option<&[f64]>,
        knots: Option<&[f64]>,
    ) -> Result<EntityId, Error> {
        let n = control_points.len();
        if degree == 0 || n < degree + 1 {
            return Err(Error::InvalidValue(
                "a spline needs degree >= 1 and degree + 1 control points",
            ));
        }
        let ctrl = control_points
            .iter()
            .map(|&p| self.pidx(p))
            .collect::<Result<Vec<_>, _>>()?;
        let weights = match weights {
            Some(w) if w.len() != n || w.iter().any(|&x| !positive(x)) => {
                return Err(Error::InvalidValue("one positive weight per control point"));
            }
            Some(w) => w.to_vec(),
            None => vec![1.0; n],
        };
        let knots = match knots {
            Some(k) if k.len() != n + degree + 1 || k.windows(2).any(|w| w[1] < w[0]) => {
                return Err(Error::InvalidValue(
                    "knots: n + degree + 1 non-decreasing values",
                ));
            }
            Some(k) => k.to_vec(),
            None => clamped_uniform_knots(n, degree),
        };
        if knots[n] <= knots[degree] {
            return Err(Error::InvalidValue("empty spline parameter range"));
        }
        let basis = Arc::new(SplineBasis {
            degree,
            knots,
            weights,
        });
        Ok(self.add_entity(EntityData::BSpline { basis, ctrl }))
    }

    /// Adds a cubic spline through the fit points (at least two). Its
    /// control points are internal unknowns that follow the fit points; the
    /// parameters of the fit points (chord length) are fixed when the
    /// spline is added.
    pub fn add_fitted_spline(
        &mut self,
        fit_points: &[PointId],
        ends: [SplineEnd; 2],
    ) -> Result<EntityId, Error> {
        let fit = fit_points
            .iter()
            .map(|&p| self.pidx(p))
            .collect::<Result<Vec<_>, _>>()?;
        let coords: Vec<[f64; 2]> = fit
            .iter()
            .map(|&p| {
                let v = self.xy(p);
                [v.x, v.y]
            })
            .collect();
        let it = interpolate(&coords).ok_or(Error::InvalidValue(
            "fit points: at least two, consecutive points distinct",
        ))?;
        let mut ctrl = vec![fit[0]];
        for c in &it.ctrl[1..it.ctrl.len() - 1] {
            ctrl.push(self.new_point(c[0], c[1], true));
        }
        ctrl.push(fit[fit.len() - 1]);
        Ok(self.add_entity(EntityData::Fitted {
            basis: Arc::new(it.basis),
            fit,
            ctrl,
            params: it.params,
            ends,
        }))
    }

    /// Removes an entity that no constraint refers to. Its points stay.
    pub fn remove_entity(&mut self, e: EntityId) -> Result<(), Error> {
        self.ent(e)?;
        if self
            .live_constraints()
            .any(|(_, c)| constraint_entities(&c.def).contains(&e))
        {
            return Err(Error::InUse);
        }
        if let Some(Some(rec)) = self.entities.get(e.index())
            && let EntityData::Fitted { ctrl, .. } = &rec.data
        {
            let hidden: Vec<usize> = ctrl[1..ctrl.len() - 1].to_vec();
            for h in hidden {
                self.points[h] = None;
            }
        }
        self.entities[e.index()] = None;
        self.invalidate();
        Ok(())
    }

    /// Removes a point that no entity or constraint refers to.
    pub fn remove_point(&mut self, p: PointId) -> Result<(), Error> {
        let i = self.pidx(p)?;
        let used_by_entity = self
            .entities
            .iter()
            .flatten()
            .any(|r| r.data.points().contains(&i));
        let used_by_constraint = self
            .live_constraints()
            .any(|(_, c)| constraint_points(&c.def).contains(&p));
        if used_by_entity || used_by_constraint {
            return Err(Error::InUse);
        }
        self.points[i] = None;
        self.invalidate();
        Ok(())
    }

    pub(crate) fn live_constraints(&self) -> impl Iterator<Item = (usize, &ConstraintRec)> {
        self.constraints
            .iter()
            .enumerate()
            .filter_map(|(i, c)| c.as_ref().map(|c| (i, c)))
    }

    /// Adds a constraint or dimension. Orientation choices are taken from
    /// the current geometry (see [`Constraint`]).
    pub fn add_constraint(&mut self, c: Constraint) -> Result<ConstraintId, Error> {
        let mut rec = ConstraintRec {
            def: c,
            seq: 0,
            side: 1.0,
            flip: 1.0,
            swap: false,
            params: Vec::new(),
            last_value: 0.0,
        };
        self.setup_constraint(&mut rec)?;
        rec.seq = self.next_seq();
        rec.last_value = self.measure_rec(&rec).unwrap_or(0.0);
        self.constraints.push(Some(rec));
        self.invalidate();
        Ok(ConstraintId(self.constraints.len() as u32 - 1))
    }

    /// Removes a constraint.
    pub fn remove_constraint(&mut self, id: ConstraintId) -> Result<(), Error> {
        match self.constraints.get_mut(id.index()) {
            Some(slot @ Some(_)) => {
                *slot = None;
                self.invalidate();
                Ok(())
            }
            _ => Err(Error::UnknownConstraint(id)),
        }
    }

    pub fn constraint(&self, id: ConstraintId) -> Option<&Constraint> {
        self.constraints.get(id.index())?.as_ref().map(|c| &c.def)
    }

    /// All constraints in creation order.
    pub fn constraints(&self) -> impl Iterator<Item = (ConstraintId, &Constraint)> {
        self.live_constraints()
            .map(|(i, c)| (ConstraintId(i as u32), &c.def))
    }

    /// All points (not the internal control points of fitted splines).
    pub fn points(&self) -> impl Iterator<Item = PointId> {
        self.points
            .iter()
            .enumerate()
            .filter(|(_, p)| matches!(p, Some(d) if !d.hidden))
            .map(|(i, _)| PointId(i as u32))
    }

    /// All entities in creation order.
    pub fn entities(&self) -> impl Iterator<Item = EntityId> {
        self.entities
            .iter()
            .enumerate()
            .filter(|(_, e)| e.is_some())
            .map(|(i, _)| EntityId(i as u32))
    }

    /// Changes the target value of a dimension. The next solve moves the
    /// geometry (in steps when the change is large).
    pub fn set_dimension_value(&mut self, id: ConstraintId, value: f64) -> Result<(), Error> {
        let rec = self
            .constraints
            .get(id.index())
            .and_then(|c| c.as_ref())
            .ok_or(Error::UnknownConstraint(id))?;
        if !rec.def.is_dimension() {
            return Err(Error::NotADimension(id));
        }
        let mut def = rec.def.clone();
        def.set_value(value);
        check_value(&def)?;
        let rec = self.constraints[id.index()]
            .as_mut()
            .expect("checked above");
        rec.def = def;
        if let Some(p) = &mut self.prepared {
            p.update_values(id.index(), value);
        }
        Ok(())
    }

    /// Target value of a dimension.
    pub fn dimension_value(&self, id: ConstraintId) -> Option<f64> {
        self.constraint(id)?.value()
    }

    /// Current geometric value of a dimension, measured the same way as it is
    /// constrained (for driven dimensions or to check a solve).
    pub fn measure(&self, id: ConstraintId) -> Option<f64> {
        let rec = self.constraints.get(id.index())?.as_ref()?;
        self.measure_rec(rec)
    }

    pub fn point(&self, p: PointId) -> Option<[f64; 2]> {
        let i = self.pidx(p).ok()?;
        let v = self.xy(i);
        Some([v.x, v.y])
    }

    /// Moves a point (for example to start from new imperfect geometry).
    /// Fixed points are fixed where they are when a solve starts.
    pub fn set_point(&mut self, p: PointId, xy: [f64; 2]) -> Result<(), Error> {
        let d = self.pd(self.pidx(p)?);
        self.params[d.x] = xy[0];
        self.params[d.y] = xy[1];
        Ok(())
    }

    /// Radius of a circle or arc.
    pub fn radius(&self, e: EntityId) -> Option<f64> {
        match self.ent(e).ok()? {
            EntityData::Circle { r, .. } => Some(self.params[*r]),
            EntityData::Arc { c, s, .. } => Some(self.xy(*s).sub(self.xy(*c)).norm()),
            _ => None,
        }
    }

    /// Sets the radius of a circle.
    pub fn set_radius(&mut self, e: EntityId, radius: f64) -> Result<(), Error> {
        match *self.ent(e)? {
            EntityData::Circle { r, .. } => {
                self.params[r] = radius;
                Ok(())
            }
            _ => Err(Error::WrongEntityKind {
                entity: e,
                expected: "a circle",
            }),
        }
    }

    /// Minor radius of an ellipse or elliptical arc.
    pub fn minor_radius(&self, e: EntityId) -> Option<f64> {
        match self.ent(e).ok()? {
            EntityData::Ellipse { b, .. } | EntityData::EllipticalArc { b, .. } => {
                Some(self.params[*b])
            }
            _ => None,
        }
    }

    pub fn entity_kind(&self, e: EntityId) -> Option<EntityKind> {
        self.ent(e).ok().map(|d| d.kind())
    }

    /// Defining points: line `[p1, p2]`, circle `[centre]`, arc
    /// `[centre, start, end]`, ellipse `[centre, major]`, elliptical arc
    /// `[centre, major, start, end]`, B-spline: control points, fitted
    /// spline: fit points.
    pub fn entity_points(&self, e: EntityId) -> Option<Vec<PointId>> {
        let d = self.ent(e).ok()?;
        let pts = match d {
            EntityData::Fitted { fit, .. } => fit.clone(),
            _ => d.points(),
        };
        Some(pts.into_iter().map(|p| PointId(p as u32)).collect())
    }

    /// Spline geometry (for fitted splines including the internal control
    /// points).
    pub fn spline(&self, e: EntityId) -> Option<SplineGeometry> {
        let (basis, ctrl) = self.ent(e).ok()?.spline()?;
        Some(SplineGeometry {
            degree: basis.degree,
            knots: basis.knots.clone(),
            weights: basis.weights.clone(),
            control_points: ctrl
                .iter()
                .map(|&p| {
                    let v = self.xy(p);
                    [v.x, v.y]
                })
                .collect(),
        })
    }

    /// Point of a spline at parameter `t`.
    pub fn spline_point(&self, e: EntityId, t: f64) -> Option<[f64; 2]> {
        let (basis, ctrl) = self.ent(e).ok()?.spline()?;
        let c: Vec<V2<f64>> = ctrl.iter().map(|&p| self.xy(p)).collect();
        let v = basis.eval(&c, t, 0).p;
        Some([v.x, v.y])
    }

    /// Characteristic size of the sketch: the diagonal of the bounding box of
    /// all points and circles, at least 1 mm.
    pub(crate) fn scale(&self) -> f64 {
        let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
        for p in self.points.iter().flatten() {
            let v = [self.params[p.x], self.params[p.y]];
            for k in 0..2 {
                lo[k] = lo[k].min(v[k]);
                hi[k] = hi[k].max(v[k]);
            }
        }
        let mut size = if lo[0].is_finite() {
            ((hi[0] - lo[0]).powi(2) + (hi[1] - lo[1]).powi(2)).sqrt()
        } else {
            0.0
        };
        for r in self.entities.iter().flatten() {
            for s in r.data.scalars() {
                size = size.max(2.0 * self.params[s].abs());
            }
        }
        size.max(1.0)
    }

    /// Union-find classes of points joined by coincident constraints.
    pub(crate) fn point_classes(&self) -> Vec<usize> {
        let mut parent: Vec<usize> = (0..self.points.len()).collect();
        fn find(parent: &mut [usize], mut i: usize) -> usize {
            while parent[i] != i {
                parent[i] = parent[parent[i]];
                i = parent[i];
            }
            i
        }
        for (_, c) in self.live_constraints() {
            if let Constraint::Coincident(a, b) = c.def {
                let (ra, rb) = (find(&mut parent, a.index()), find(&mut parent, b.index()));
                if ra != rb {
                    parent[ra.max(rb)] = ra.min(rb);
                }
            }
        }
        (0..parent.len()).map(|i| find(&mut parent, i)).collect()
    }
}

/// True for finite positive numbers (false for NaN).
fn positive(v: f64) -> bool {
    v > 0.0 && v.is_finite()
}

/// Checks the range of a dimension value.
fn check_value(c: &Constraint) -> Result<(), Error> {
    use Constraint::*;
    let ok = match *c {
        Distance { value, .. }
        | Length { value, .. }
        | Radius { value, .. }
        | Diameter { value, .. }
        | ArcLength { value, .. }
        | MajorRadius { value, .. }
        | MinorRadius { value, .. } => positive(value),
        PointLineDistance { value, .. }
        | LineDistance { value, .. }
        | HorizontalDistance { value, .. }
        | VerticalDistance { value, .. } => value == 0.0 || positive(value),
        Angle { value, .. } => (0.0..=PI).contains(&value),
        _ => true,
    };
    if ok {
        Ok(())
    } else {
        Err(Error::InvalidValue(match c {
            Angle { .. } => "the angle must be between 0 and pi",
            PointLineDistance { .. }
            | LineDistance { .. }
            | HorizontalDistance { .. }
            | VerticalDistance { .. } => "the distance must not be negative",
            _ => "the dimension must be positive",
        }))
    }
}

/// Entities a constraint refers to.
pub(crate) fn constraint_entities(c: &Constraint) -> Vec<EntityId> {
    use Constraint::*;
    match *c {
        PointOnCurve(_, e)
        | Horizontal(e)
        | Vertical(e)
        | FixEntity(e)
        | Midpoint(_, e)
        | PointLineDistance { line: e, .. }
        | Length { line: e, .. }
        | Radius { entity: e, .. }
        | Diameter { entity: e, .. }
        | ArcLength { arc: e, .. }
        | MajorRadius { ellipse: e, .. }
        | MinorRadius { ellipse: e, .. }
        | SymmetricPoints { axis: e, .. } => vec![e],
        Parallel(a, b)
        | Perpendicular(a, b)
        | Tangent(a, b)
        | Smooth(a, b)
        | Equal(a, b)
        | Concentric(a, b)
        | Collinear(a, b)
        | LineDistance { a, b, .. }
        | Angle { a, b, .. }
        | EqualSize(a, b) => vec![a, b],
        SymmetricEntities { a, b, axis } => vec![a, b, axis],
        _ => Vec::new(),
    }
}

/// Points a constraint refers to directly.
pub(crate) fn constraint_points(c: &Constraint) -> Vec<PointId> {
    use Constraint::*;
    match *c {
        Coincident(a, b)
        | HorizontalPoints(a, b)
        | VerticalPoints(a, b)
        | SymmetricPoints { a, b, .. }
        | Distance { a, b, .. }
        | HorizontalDistance { a, b, .. }
        | VerticalDistance { a, b, .. }
        | Translated { a, b, .. } => vec![a, b],
        PointOnCurve(p, _) | FixPoint(p) | Midpoint(p, _) | PointLineDistance { point: p, .. } => {
            vec![p]
        }
        Rotated { center, a, b, .. } => vec![center, a, b],
        TurnedDirection {
            a_center,
            a,
            b_center,
            b,
            ..
        } => vec![a_center, a, b_center, b],
        _ => Vec::new(),
    }
}

fn sign(v: f64) -> f64 {
    if v < 0.0 { -1.0 } else { 1.0 }
}

impl System {
    fn line(&self, e: EntityId) -> Result<(usize, usize), Error> {
        match *self.ent(e)? {
            EntityData::Line { p1, p2 } => Ok((p1, p2)),
            _ => Err(Error::WrongEntityKind {
                entity: e,
                expected: "a line",
            }),
        }
    }

    fn is_round(&self, e: EntityId) -> Result<bool, Error> {
        Ok(matches!(
            self.ent(e)?,
            EntityData::Circle { .. } | EntityData::Arc { .. }
        ))
    }

    fn round(&self, e: EntityId) -> Result<(usize, f64), Error> {
        match *self.ent(e)? {
            EntityData::Circle { c, r } => Ok((c, self.params[r])),
            EntityData::Arc { c, s, .. } => Ok((c, self.xy(s).sub(self.xy(c)).norm())),
            _ => Err(Error::WrongEntityKind {
                entity: e,
                expected: "a circle or an arc",
            }),
        }
    }

    fn signed_dist(&self, p: V2<f64>, a: usize, b: usize) -> f64 {
        let (a, b) = (self.xy(a), self.xy(b));
        b.sub(a).unit().cross(p.sub(a))
    }

    /// Validates a constraint and fills in the orientation choices and
    /// internal parameters from the current geometry.
    fn setup_constraint(&mut self, rec: &mut ConstraintRec) -> Result<(), Error> {
        use Constraint::*;
        check_value(&rec.def)?;
        for p in constraint_points(&rec.def) {
            self.pidx(p)?;
        }
        for e in constraint_entities(&rec.def) {
            self.ent(e)?;
        }
        match rec.def.clone() {
            Coincident(..)
            | FixPoint(_)
            | FixEntity(_)
            | HorizontalPoints(..)
            | VerticalPoints(..)
            | Distance { .. } => {}
            Translated { by, .. } => {
                if !by.iter().all(|v| v.is_finite()) {
                    return Err(Error::InvalidValue("the move must be finite"));
                }
            }
            Rotated { angle, .. } | TurnedDirection { angle, .. } => {
                if !angle.is_finite() {
                    return Err(Error::InvalidValue("the angle must be finite"));
                }
            }
            EqualSize(a, b) => {
                let size = |d: &EntityData| match d {
                    EntityData::Circle { .. } => Some(0),
                    EntityData::Ellipse { .. } | EntityData::EllipticalArc { .. } => Some(1),
                    _ => None,
                };
                let (sa, sb) = (size(self.ent(a)?), size(self.ent(b)?));
                if sa.is_none() || sa != sb {
                    return Err(Error::Unsupported(
                        "equal size needs two circles or two ellipses",
                    ));
                }
            }
            PointOnCurve(p, e) => {
                let p = self.pidx(p)?;
                match self.ent(e)?.clone() {
                    EntityData::Ellipse { c, a, b } | EntityData::EllipticalArc { c, a, b, .. } => {
                        let t = self.ellipse_angle(c, a, b, p);
                        let w = self.ellipse_param_weight(c, a);
                        rec.params.push(self.new_param(t, ParamKind::Angle, w));
                    }
                    EntityData::BSpline { basis, ctrl }
                    | EntityData::Fitted { basis, ctrl, .. } => {
                        let target = self.xy(p);
                        let t = self.spline_param(&basis, &ctrl, |pt, _| {
                            pt.sub(target).dot(pt.sub(target))
                        });
                        rec.params.push(self.spline_t_param(&basis, &ctrl, t));
                    }
                    _ => {}
                }
            }
            Horizontal(e) | Vertical(e) | Length { line: e, .. } => {
                self.line(e)?;
            }
            Parallel(a, b) | Perpendicular(a, b) | Collinear(a, b) => {
                self.line(a)?;
                self.line(b)?;
            }
            Angle { a, b, value } => {
                let (a1, a2) = self.line(a)?;
                let (b1, b2) = self.line(b)?;
                let da = self.xy(a2).sub(self.xy(a1));
                let db = self.xy(b2).sub(self.xy(b1));
                let phi = da.cross(db).atan2(da.dot(db));
                let alt = wrap_angle(phi + PI);
                let (flip, theta) = if (alt.abs() - value).abs() < (phi.abs() - value).abs() - 1e-12
                {
                    (-1.0, alt)
                } else {
                    (1.0, phi)
                };
                rec.flip = flip;
                rec.side = sign(theta);
            }
            Tangent(a, b) => self.setup_tangent(rec, a, b)?,
            Smooth(a, b) => self.setup_smooth(a, b)?,
            Equal(a, b) => {
                let la = matches!(self.ent(a)?, EntityData::Line { .. });
                let lb = matches!(self.ent(b)?, EntityData::Line { .. });
                if la != lb || (!la && !(self.is_round(a)? && self.is_round(b)?)) {
                    return Err(Error::Unsupported(
                        "equal needs two lines or two circles/arcs",
                    ));
                }
            }
            Midpoint(_, e) => {
                if !matches!(
                    self.ent(e)?,
                    EntityData::Line { .. } | EntityData::Arc { .. }
                ) {
                    return Err(Error::WrongEntityKind {
                        entity: e,
                        expected: "a line or an arc",
                    });
                }
            }
            Concentric(a, b) => {
                for e in [a, b] {
                    if self.ent(e)?.centre().is_none() {
                        return Err(Error::WrongEntityKind {
                            entity: e,
                            expected: "a circle, arc, ellipse or elliptical arc",
                        });
                    }
                }
            }
            SymmetricPoints { axis, .. } => {
                self.line(axis)?;
            }
            SymmetricEntities { a, b, axis } => {
                let (x1, x2) = self.line(axis)?;
                match (self.ent(a)?.clone(), self.ent(b)?.clone()) {
                    (EntityData::Line { p1, .. }, EntityData::Line { p1: q1, p2: q2 }) => {
                        let m = self.mirror(self.xy(p1), x1, x2);
                        rec.swap = m.sub(self.xy(q2)).norm() < m.sub(self.xy(q1)).norm();
                    }
                    (EntityData::Circle { .. }, EntityData::Circle { .. })
                    | (EntityData::Arc { .. }, EntityData::Arc { .. }) => {}
                    _ => {
                        return Err(Error::Unsupported(
                            "symmetry needs two lines, two circles or two arcs",
                        ));
                    }
                }
            }
            PointLineDistance { point, line, .. } => {
                let (a1, a2) = self.line(line)?;
                let p = self.xy(self.pidx(point)?);
                rec.side = sign(self.signed_dist(p, a1, a2));
            }
            LineDistance { a, b, .. } => {
                let (a1, a2) = self.line(a)?;
                let (b1, b2) = self.line(b)?;
                let mid = self.xy(b1).add(self.xy(b2)).mulf(0.5);
                rec.side = sign(self.signed_dist(mid, a1, a2));
            }
            HorizontalDistance { a, b, .. } => {
                let (a, b) = (self.xy(self.pidx(a)?), self.xy(self.pidx(b)?));
                rec.side = sign(b.x - a.x);
            }
            VerticalDistance { a, b, .. } => {
                let (a, b) = (self.xy(self.pidx(a)?), self.xy(self.pidx(b)?));
                rec.side = sign(b.y - a.y);
            }
            Radius { entity, .. } | Diameter { entity, .. } => {
                self.round(entity)?;
            }
            ArcLength { arc, .. } => {
                if !matches!(self.ent(arc)?, EntityData::Arc { .. }) {
                    return Err(Error::WrongEntityKind {
                        entity: arc,
                        expected: "an arc",
                    });
                }
            }
            MajorRadius { ellipse, .. } | MinorRadius { ellipse, .. } => {
                if !matches!(
                    self.ent(ellipse)?,
                    EntityData::Ellipse { .. } | EntityData::EllipticalArc { .. }
                ) {
                    return Err(Error::WrongEntityKind {
                        entity: ellipse,
                        expected: "an ellipse",
                    });
                }
            }
        }
        Ok(())
    }

    pub(crate) fn mirror(&self, p: V2<f64>, a: usize, b: usize) -> V2<f64> {
        let (a, b) = (self.xy(a), self.xy(b));
        let u = b.sub(a).unit();
        let d = p.sub(a);
        a.add(u.mulf(2.0 * d.dot(u))).sub(d)
    }

    fn setup_tangent(
        &mut self,
        rec: &mut ConstraintRec,
        a: EntityId,
        b: EntityId,
    ) -> Result<(), Error> {
        let (da, db) = (self.ent(a)?.clone(), self.ent(b)?.clone());
        let kind = |d: &EntityData| match d {
            EntityData::Line { .. } => 0,
            EntityData::Circle { .. } | EntityData::Arc { .. } => 1,
            EntityData::BSpline { .. } | EntityData::Fitted { .. } => 2,
            _ => 3,
        };
        let (ka, kb) = (kind(&da), kind(&db));
        if ka == 3 || kb == 3 || (ka == 0 && kb == 0) {
            return Err(Error::Unsupported(
                "tangency between lines, circles, arcs and splines only (not ellipses)",
            ));
        }
        // Order: line < round < spline.
        let (lo, hi, dlo, dhi) = if ka <= kb {
            (a, b, da, db)
        } else {
            (b, a, db, da)
        };
        match (kind(&dlo), kind(&dhi)) {
            (0, 1) => {
                let (p1, p2) = self.line(lo)?;
                let (c, _) = self.round(hi)?;
                rec.side = sign(self.signed_dist(self.xy(c), p1, p2));
            }
            (1, 1) => {
                let (c1, r1) = self.round(lo)?;
                let (c2, r2) = self.round(hi)?;
                let d = self.xy(c2).sub(self.xy(c1)).norm();
                let ext = (d - (r1 + r2)).abs();
                let int = (d - (r1 - r2).abs()).abs();
                rec.side = if int < ext { sign(r1 - r2) } else { 0.0 };
            }
            (_, 2) => {
                // Interior tangency needs a contact parameter; an end-point
                // join does not use it (unused parameters are not unknowns).
                let (basis, ctrl) = dhi
                    .spline()
                    .map(|(b, c)| (b.clone(), c.to_vec()))
                    .expect("spline");
                if kind(&dlo) == 2 {
                    let classes = self.point_classes();
                    let ea = dlo.ends().expect("spline ends");
                    let eb = dhi.ends().expect("spline ends");
                    let joined = ea
                        .iter()
                        .any(|x| eb.iter().any(|y| classes[*x] == classes[*y]));
                    if !joined {
                        return Err(Error::Unsupported(
                            "tangent splines must share an end point",
                        ));
                    }
                    if !basis.is_clamped() || !dlo.spline().expect("spline").0.is_clamped() {
                        return Err(Error::Unsupported(
                            "spline end tangency needs clamped knots",
                        ));
                    }
                    return Ok(());
                }
                let t = match dlo {
                    EntityData::Line { p1, p2 } => {
                        let (a, b) = (self.xy(p1), self.xy(p2));
                        let u = b.sub(a).unit();
                        let len = b.sub(a).norm();
                        self.spline_param(&basis, &ctrl, |pt, d1| {
                            let dist = u.cross(pt.sub(a));
                            let s = u.cross(d1.unit()) * len;
                            dist * dist + s * s
                        })
                    }
                    _ => {
                        let (c, r) = self.round(lo)?;
                        let cv = self.xy(c);
                        self.spline_param(&basis, &ctrl, |pt, d1| {
                            let dist = pt.sub(cv).norm() - r;
                            let s = pt.sub(cv).unit().dot(d1.unit()) * r;
                            dist * dist + s * s
                        })
                    }
                };
                rec.params.push(self.spline_t_param(&basis, &ctrl, t));
            }
            _ => unreachable!("ordered kinds"),
        }
        Ok(())
    }

    /// A spline joined at an end point to a line, arc or spline.
    fn setup_smooth(&self, a: EntityId, b: EntityId) -> Result<(), Error> {
        let (da, db) = (self.ent(a)?, self.ent(b)?);
        let spline = |d: &EntityData| d.spline().map(|(basis, _)| basis.is_clamped());
        let ok_other =
            |d: &EntityData| matches!(d, EntityData::Line { .. } | EntityData::Arc { .. });
        let valid = match (spline(da), spline(db)) {
            (Some(ca), Some(cb)) => ca && cb,
            (Some(c), None) => c && ok_other(db),
            (None, Some(c)) => c && ok_other(da),
            (None, None) => false,
        };
        if !valid {
            return Err(Error::Unsupported(
                "curvature continuity joins a spline (clamped knots) with a line, arc or spline",
            ));
        }
        let classes = self.point_classes();
        let (ea, eb) = (
            da.ends().expect("open curve"),
            db.ends().expect("open curve"),
        );
        if !ea
            .iter()
            .any(|x| eb.iter().any(|y| classes[*x] == classes[*y]))
        {
            return Err(Error::Unsupported("smooth curves must share an end point"));
        }
        Ok(())
    }

    fn spline_t_param(&mut self, basis: &SplineBasis, ctrl: &[usize], t: f64) -> usize {
        let (lo, hi) = basis.domain();
        let mut len = 0.0;
        for w in ctrl.windows(2) {
            len += self.xy(w[1]).sub(self.xy(w[0])).norm();
        }
        let w = (0.1 * len / (hi - lo)).powi(2).max(1e-6);
        self.new_param(t, ParamKind::SplineT { lo, hi }, w)
    }

    /// Parameter minimizing `cost(point, derivative)` over a dense sampling.
    fn spline_param(
        &self,
        basis: &SplineBasis,
        ctrl: &[usize],
        cost: impl Fn(V2<f64>, V2<f64>) -> f64,
    ) -> f64 {
        let c: Vec<V2<f64>> = ctrl.iter().map(|&p| self.xy(p)).collect();
        let (lo, hi) = basis.domain();
        let n = 32 * ctrl.len();
        let mut best = (f64::INFINITY, lo);
        for i in 0..=n {
            let t = lo + (hi - lo) * i as f64 / n as f64;
            let e = basis.eval(&c, t, 1);
            let v = cost(e.p, e.d1);
            if v < best.0 {
                best = (v, t);
            }
        }
        best.1
    }

    pub(crate) fn measure_rec(&self, rec: &ConstraintRec) -> Option<f64> {
        use Constraint::*;
        let pt = |p: PointId| self.xy(p.index());
        let line = |e: EntityId| self.line(e).ok();
        Some(match rec.def {
            Distance { a, b, .. } => pt(b).sub(pt(a)).norm(),
            Length { line: l, .. } => {
                let (a, b) = line(l)?;
                self.xy(b).sub(self.xy(a)).norm()
            }
            PointLineDistance { point, line: l, .. } => {
                let (a, b) = line(l)?;
                rec.side * self.signed_dist(pt(point), a, b)
            }
            LineDistance { a, b, .. } => {
                let (a1, a2) = line(a)?;
                let (b1, b2) = line(b)?;
                let mid = self.xy(b1).add(self.xy(b2)).mulf(0.5);
                rec.side * self.signed_dist(mid, a1, a2)
            }
            HorizontalDistance { a, b, .. } => rec.side * (pt(b).x - pt(a).x),
            VerticalDistance { a, b, .. } => rec.side * (pt(b).y - pt(a).y),
            Angle { a, b, .. } => {
                let (a1, a2) = line(a)?;
                let (b1, b2) = line(b)?;
                let da = self.xy(a2).sub(self.xy(a1));
                let db = self.xy(b2).sub(self.xy(b1)).mulf(rec.flip);
                rec.side * da.cross(db).atan2(da.dot(db))
            }
            Radius { entity, .. } => self.round(entity).ok()?.1,
            Diameter { entity, .. } => 2.0 * self.round(entity).ok()?.1,
            ArcLength { arc, .. } => match *self.ent(arc).ok()? {
                EntityData::Arc { c, s, e } => {
                    let (vs, ve) = (self.xy(s).sub(self.xy(c)), self.xy(e).sub(self.xy(c)));
                    let mut sweep = vs.cross(ve).atan2(vs.dot(ve));
                    if sweep <= 0.0 {
                        sweep += 2.0 * PI;
                    }
                    vs.norm() * sweep
                }
                _ => return None,
            },
            MajorRadius { ellipse, .. } => match *self.ent(ellipse).ok()? {
                EntityData::Ellipse { c, a, .. } | EntityData::EllipticalArc { c, a, .. } => {
                    self.xy(a).sub(self.xy(c)).norm()
                }
                _ => return None,
            },
            MinorRadius { ellipse, .. } => match *self.ent(ellipse).ok()? {
                EntityData::Ellipse { b, .. } | EntityData::EllipticalArc { b, .. } => {
                    self.params[b]
                }
                _ => return None,
            },
            _ => return None,
        })
    }
}
