// SPDX-License-Identifier: MIT
//! Solving a sketch with `mitcad-solver`: the system is built from the
//! stored positions every time (so the same definition and values always
//! give the same result), solved, and read back as point positions and
//! curves, with the status the solver's analysis gives: degrees of
//! freedom, fully constrained entities, redundant and conflicting
//! constraints by id.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use mitcad_solver::{
    Constraint as S, ConstraintId, DragGoal, EntityId, PointId, SolveOptions, SolveStatus,
    SplineEnd, System,
};

use super::geometry::{Curve2, Nurbs, P2, angle_after, cross, dist, dot, norm, sub, unit};
use super::pattern::{Binding, BindingKind};
use super::{Constraint, ConstraintKind, ConstraintUid, DimensionKind, Entity, EntityKind, Ref};
use crate::ids::EntityUid;

/// What a solver equation comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SolverItem {
    /// A constraint or dimension.
    Constraint(ConstraintUid),
    /// A fixed entity.
    Fixed(Ref),
}

impl fmt::Display for SolverItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Constraint(id) => id.fmt(f),
            Self::Fixed(entity) => write!(f, "{entity} (fixed)"),
        }
    }
}

/// Degrees of freedom and constraint status after a solve.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SketchStatus {
    /// Remaining degrees of freedom; 0 when fully constrained.
    pub dof: usize,
    /// Points and curves whose position is fully determined.
    pub fully_constrained: BTreeSet<EntityUid>,
    /// Constraints that follow from the others.
    pub redundant: Vec<SolverItem>,
    /// Sets of constraints that contradict each other.
    pub conflicts: Vec<Vec<SolverItem>>,
    /// Measured values of driven dimensions (mm, rad).
    pub driven: BTreeMap<ConstraintUid, f64>,
    /// Lines that collapsed to a point, circles of zero radius.
    pub degenerate: Vec<EntityUid>,
}

/// The solved sketch geometry.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Solved {
    pub points: BTreeMap<EntityUid, P2>,
    pub curves: BTreeMap<EntityUid, Curve2>,
    /// Circle radii and ellipse minor radii.
    pub radii: BTreeMap<EntityUid, f64>,
    pub status: SketchStatus,
}

impl Solved {
    /// Writes the solved positions into the definition, as the next
    /// solve's start.
    pub fn store(&self, entities: &mut [Entity]) {
        for entity in entities {
            match &mut entity.kind {
                EntityKind::Point { at } => {
                    if let Some(p) = self.points.get(&entity.id) {
                        *at = *p;
                    }
                }
                EntityKind::Circle { radius, .. } => {
                    if let Some(r) = self.radii.get(&entity.id) {
                        *radius = *r;
                    }
                }
                EntityKind::Ellipse { minor_radius, .. }
                | EntityKind::EllipticalArc { minor_radius, .. } => {
                    if let Some(r) = self.radii.get(&entity.id) {
                        *minor_radius = *r;
                    }
                }
                _ => {}
            }
        }
    }
}

/// A sketch that does not solve.
#[derive(Debug, Clone, PartialEq)]
pub struct SolveError {
    pub message: String,
    /// What the solver could tell: conflicts in particular.
    pub status: Box<SketchStatus>,
}

impl SolveError {
    pub fn new(message: String) -> Self {
        Self {
            message,
            status: Box::default(),
        }
    }
}

impl fmt::Display for SolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

/// A dimension to solve: driving with its value (mm, rad), or driven.
#[derive(Debug, Clone, Copy)]
pub struct DimensionInput<'a> {
    pub id: ConstraintUid,
    pub kind: &'a DimensionKind,
    pub value: Option<f64>,
}

/// A goal of a drag: a point toward a position, or a circle's radius.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Goal {
    Point(EntityUid, P2),
    Radius(EntityUid, f64),
}

/// What a solve takes besides the stored constraints: the bindings of
/// pattern copies, and the entities it leaves out (curves of derived
/// offsets, computed after it).
#[derive(Debug, Clone, Copy)]
pub struct Extra<'a> {
    pub bindings: &'a [Binding],
    pub skip: &'a BTreeSet<EntityUid>,
}

static NOTHING: BTreeSet<EntityUid> = BTreeSet::new();

impl Default for Extra<'_> {
    fn default() -> Self {
        Self {
            bindings: &[],
            skip: &NOTHING,
        }
    }
}

/// The solver system of a sketch with the mapping back to ids.
struct Built {
    system: System,
    points: BTreeMap<EntityUid, PointId>,
    curves: BTreeMap<EntityUid, EntityId>,
    items: BTreeMap<ConstraintId, SolverItem>,
}

fn list(items: &[SolverItem]) -> String {
    let names: Vec<String> = items.iter().map(ToString::to_string).collect();
    match names.as_slice() {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

impl Built {
    fn new(
        entities: &[Entity],
        constraints: &[Constraint],
        dimensions: &[DimensionInput<'_>],
        extra: &Extra<'_>,
    ) -> Result<Self, String> {
        let mut system = System::new();
        let mut points = BTreeMap::new();
        let entities: Vec<&Entity> = entities
            .iter()
            .filter(|e| !extra.skip.contains(&e.id))
            .collect();
        for e in &entities {
            if let EntityKind::Point { at } = e.kind {
                points.insert(e.id, system.add_point(at[0], at[1]));
            }
        }
        let p = |uid: EntityUid| {
            points
                .get(&uid)
                .copied()
                .ok_or_else(|| format!("point p{} does not exist", uid.0))
        };
        let mut curves = BTreeMap::new();
        for e in &entities {
            let what = |error: mitcad_solver::Error| format!("c{}: {error}", e.id.0);
            let id = match &e.kind {
                EntityKind::Point { .. } => continue,
                EntityKind::Line { start, end, .. } => {
                    system.add_line(p(*start)?, p(*end)?).map_err(what)?
                }
                EntityKind::Circle { center, radius } => {
                    system.add_circle(p(*center)?, *radius).map_err(what)?
                }
                EntityKind::Arc { center, start, end } => system
                    .add_arc(p(*center)?, p(*start)?, p(*end)?)
                    .map_err(what)?,
                EntityKind::Ellipse {
                    center,
                    major,
                    minor_radius,
                } => system
                    .add_ellipse(p(*center)?, p(*major)?, *minor_radius)
                    .map_err(what)?,
                EntityKind::EllipticalArc {
                    center,
                    major,
                    minor_radius,
                    start,
                    end,
                } => system
                    .add_elliptical_arc(
                        p(*center)?,
                        p(*major)?,
                        *minor_radius,
                        p(*start)?,
                        p(*end)?,
                    )
                    .map_err(what)?,
                EntityKind::Spline {
                    degree,
                    control,
                    weights,
                    knots,
                } => {
                    let control = control
                        .iter()
                        .map(|u| p(*u))
                        .collect::<Result<Vec<_>, _>>()?;
                    system
                        .add_bspline(
                            *degree as usize,
                            &control,
                            (!weights.is_empty()).then_some(weights.as_slice()),
                            (!knots.is_empty()).then_some(knots.as_slice()),
                        )
                        .map_err(what)?
                }
                EntityKind::FittedSpline { points: fit } => {
                    let fit = fit.iter().map(|u| p(*u)).collect::<Result<Vec<_>, _>>()?;
                    system
                        .add_fitted_spline(&fit, [SplineEnd::Natural, SplineEnd::Natural])
                        .map_err(what)?
                }
            };
            curves.insert(e.id, id);
        }
        let mut built = Self {
            system,
            points,
            curves,
            items: BTreeMap::new(),
        };
        for e in entities.iter().filter(|e| e.fixed) {
            let c = if e.is_point() {
                S::FixPoint(built.points[&e.id])
            } else {
                S::FixEntity(built.curves[&e.id])
            };
            built.add(c, SolverItem::Fixed(e.as_ref()))?;
        }
        for c in constraints {
            let item = SolverItem::Constraint(c.id);
            for s in built
                .constraint(&c.kind)
                .map_err(|e| format!("{}: {e}", c.id))?
            {
                built.add(s, item)?;
            }
        }
        for d in dimensions {
            if let Some(value) = d.value {
                let s = built
                    .dimension(d.kind, value)
                    .map_err(|e| format!("{}: {e}", d.id))?;
                built.add(s, SolverItem::Constraint(d.id))?;
            }
        }
        for b in extra.bindings {
            let s = built
                .binding(&b.kind)
                .map_err(|e| format!("{}: {e}", b.owner))?;
            built.add(s, SolverItem::Constraint(b.owner))?;
        }
        Ok(built)
    }

    fn binding(&self, kind: &BindingKind) -> Result<S, String> {
        let (p, c) = (|u| self.point(u), |u| self.curve(u));
        Ok(match *kind {
            BindingKind::Translated { a, b, by } => S::Translated {
                a: p(a)?,
                b: p(b)?,
                by,
            },
            BindingKind::Rotated {
                center,
                a,
                b,
                angle,
            } => S::Rotated {
                center: p(center)?,
                a: p(a)?,
                b: p(b)?,
                angle,
            },
            BindingKind::TurnedDirection {
                a_center,
                a,
                b_center,
                b,
                angle,
            } => S::TurnedDirection {
                a_center: p(a_center)?,
                a: p(a)?,
                b_center: p(b_center)?,
                b: p(b)?,
                angle,
            },
            BindingKind::EqualSize(a, b) => S::EqualSize(c(a)?, c(b)?),
        })
    }

    fn add(&mut self, c: S, item: SolverItem) -> Result<(), String> {
        let id = self
            .system
            .add_constraint(c)
            .map_err(|e| format!("{item}: {e}"))?;
        self.items.insert(id, item);
        Ok(())
    }

    fn point(&self, uid: EntityUid) -> Result<PointId, String> {
        self.points
            .get(&uid)
            .copied()
            .ok_or_else(|| format!("p{} is not a point", uid.0))
    }

    fn curve(&self, uid: EntityUid) -> Result<EntityId, String> {
        self.curves
            .get(&uid)
            .copied()
            .ok_or_else(|| format!("c{} is not a curve", uid.0))
    }

    fn constraint(&self, kind: &ConstraintKind) -> Result<Vec<S>, String> {
        use ConstraintKind as K;
        let (p, c) = (|u| self.point(u), |u| self.curve(u));
        Ok(match kind {
            K::Coincident { point, entity } => vec![match entity {
                Ref::Point(q) => S::Coincident(p(*point)?, p(*q)?),
                Ref::Curve(e) => S::PointOnCurve(p(*point)?, c(*e)?),
            }],
            K::Horizontal { line } => vec![S::Horizontal(c(*line)?)],
            K::Vertical { line } => vec![S::Vertical(c(*line)?)],
            K::HorizontalPoints { a, b } => vec![S::HorizontalPoints(p(*a)?, p(*b)?)],
            K::VerticalPoints { a, b } => vec![S::VerticalPoints(p(*a)?, p(*b)?)],
            K::Parallel { a, b } => vec![S::Parallel(c(*a)?, c(*b)?)],
            K::Perpendicular { a, b } => vec![S::Perpendicular(c(*a)?, c(*b)?)],
            K::Collinear { a, b } => vec![S::Collinear(c(*a)?, c(*b)?)],
            K::Tangent { a, b } => vec![S::Tangent(c(*a)?, c(*b)?)],
            K::Smooth { a, b } => vec![S::Smooth(c(*a)?, c(*b)?)],
            K::Equal { a, b } => vec![S::Equal(c(*a)?, c(*b)?)],
            K::Concentric { a, b } => vec![S::Concentric(c(*a)?, c(*b)?)],
            K::Midpoint { point, curve } => vec![S::Midpoint(p(*point)?, c(*curve)?)],
            K::Symmetric { a, b, axis } => vec![match (a, b) {
                (Ref::Point(a), Ref::Point(b)) => S::SymmetricPoints {
                    a: p(*a)?,
                    b: p(*b)?,
                    axis: c(*axis)?,
                },
                (Ref::Curve(a), Ref::Curve(b)) => S::SymmetricEntities {
                    a: c(*a)?,
                    b: c(*b)?,
                    axis: c(*axis)?,
                },
                _ => return Err("symmetric entities must be two points or two curves".to_owned()),
            }],
        })
    }

    fn dimension(&self, kind: &DimensionKind, value: f64) -> Result<S, String> {
        use DimensionKind as D;
        let (p, c) = (|u| self.point(u), |u| self.curve(u));
        Ok(match kind {
            D::Distance { a, b } => S::Distance {
                a: p(*a)?,
                b: p(*b)?,
                value,
            },
            D::HorizontalDistance { a, b } => S::HorizontalDistance {
                a: p(*a)?,
                b: p(*b)?,
                value,
            },
            D::VerticalDistance { a, b } => S::VerticalDistance {
                a: p(*a)?,
                b: p(*b)?,
                value,
            },
            D::PointLineDistance { point, line } => S::PointLineDistance {
                point: p(*point)?,
                line: c(*line)?,
                value,
            },
            D::LineDistance { a, b } => S::LineDistance {
                a: c(*a)?,
                b: c(*b)?,
                value,
            },
            D::Length { line } => S::Length {
                line: c(*line)?,
                value,
            },
            D::Angle { a, b } => S::Angle {
                a: c(*a)?,
                b: c(*b)?,
                value,
            },
            D::Radius { curve } => S::Radius {
                entity: c(*curve)?,
                value,
            },
            D::Diameter { curve } => S::Diameter {
                entity: c(*curve)?,
                value,
            },
            D::ArcLength { arc } => S::ArcLength {
                arc: c(*arc)?,
                value,
            },
            D::MajorRadius { ellipse } => S::MajorRadius {
                ellipse: c(*ellipse)?,
                value,
            },
            D::MinorRadius { ellipse } => S::MinorRadius {
                ellipse: c(*ellipse)?,
                value,
            },
            D::LinearDiameter { axis, entity } => match entity {
                Ref::Point(point) => S::PointLineDistance {
                    point: p(*point)?,
                    line: c(*axis)?,
                    value: value / 2.0,
                },
                Ref::Curve(line) => S::LineDistance {
                    a: c(*axis)?,
                    b: c(*line)?,
                    value: value / 2.0,
                },
            },
        })
    }

    fn items(&self, ids: &[ConstraintId]) -> Vec<SolverItem> {
        let mut items: Vec<SolverItem> = ids
            .iter()
            .filter_map(|id| self.items.get(id))
            .copied()
            .collect();
        items.sort();
        items.dedup();
        items
    }

    fn solve(&mut self) -> Result<(), SolveError> {
        let result = self.system.solve(&SolveOptions::default());
        match result.status {
            SolveStatus::Converged => {
                // A second, tighter pass so that curves meet their shared
                // points within the geometry kernel's tolerance; if it does
                // not converge, the first solution stays.
                let tight = SolveOptions {
                    tolerance: 1e-13,
                    ..SolveOptions::default()
                };
                self.system.solve(&tight);
                Ok(())
            }
            SolveStatus::Conflicting => {
                let conflicts: Vec<Vec<SolverItem>> = result
                    .conflicts
                    .iter()
                    .map(|d| self.items(&d.involved))
                    .collect();
                let mut all: Vec<SolverItem> = conflicts.iter().flatten().copied().collect();
                all.sort();
                all.dedup();
                Err(SolveError {
                    message: format!(
                        "the sketch is over-constrained: {} contradict each other",
                        list(&all)
                    ),
                    status: Box::new(SketchStatus {
                        conflicts,
                        ..SketchStatus::default()
                    }),
                })
            }
            SolveStatus::NotConverged => Err(SolveError::new(
                "the sketch cannot be solved: the dimensions do not fit the geometry".to_owned(),
            )),
        }
    }

    fn read(&self, entities: &[Entity], dimensions: &[DimensionInput<'_>]) -> Solved {
        let s = &self.system;
        let mut solved = Solved::default();
        for (uid, id) in &self.points {
            solved
                .points
                .insert(*uid, s.point(*id).expect("live point"));
        }
        let at = |u: &EntityUid| solved.points[u];
        let mut curves = BTreeMap::new();
        let mut radii = BTreeMap::new();
        for e in entities {
            let Some(&id) = self.curves.get(&e.id) else {
                continue;
            };
            let curve = match &e.kind {
                EntityKind::Point { .. } => continue,
                EntityKind::Line { start, end, .. } => Curve2::Line {
                    a: at(start),
                    b: at(end),
                },
                EntityKind::Circle { center, .. } => {
                    let radius = s.radius(id).expect("a circle");
                    radii.insert(e.id, radius);
                    Curve2::Circle {
                        center: at(center),
                        radius,
                    }
                }
                EntityKind::Arc { center, start, end } => {
                    let (c, a, b) = (at(center), at(start), at(end));
                    let (va, vb) = (sub(a, c), sub(b, c));
                    let start = va[1].atan2(va[0]);
                    Curve2::Arc {
                        center: c,
                        radius: norm(va),
                        start,
                        end: angle_after(start, vb[1].atan2(vb[0])),
                    }
                }
                EntityKind::Ellipse { center, major, .. } => {
                    let (c, m) = (at(center), at(major));
                    let minor = s.minor_radius(id).expect("an ellipse");
                    radii.insert(e.id, minor);
                    let axis = sub(m, c);
                    Curve2::Ellipse {
                        center: c,
                        major: norm(axis),
                        minor,
                        rotation: axis[1].atan2(axis[0]),
                    }
                }
                EntityKind::EllipticalArc {
                    center,
                    major,
                    start,
                    end,
                    ..
                } => {
                    let (c, m) = (at(center), at(major));
                    let minor = s.minor_radius(id).expect("an elliptical arc");
                    radii.insert(e.id, minor);
                    let axis = sub(m, c);
                    let a = norm(axis);
                    let u = unit(axis);
                    let param = |p: P2| {
                        let d = sub(p, c);
                        (dot(d, [-u[1], u[0]]) / minor).atan2(dot(d, u) / a)
                    };
                    let t0 = param(at(start));
                    Curve2::EllipticalArc {
                        center: c,
                        major: a,
                        minor,
                        rotation: axis[1].atan2(axis[0]),
                        start: t0,
                        end: angle_after(t0, param(at(end))),
                    }
                }
                EntityKind::Spline { .. } | EntityKind::FittedSpline { .. } => {
                    let g = s.spline(id).expect("a spline");
                    let rational = g.weights.iter().any(|w| *w != 1.0);
                    Curve2::Nurbs(Nurbs {
                        degree: g.degree,
                        control: g.control_points,
                        weights: if rational { g.weights } else { Vec::new() },
                        knots: g.knots,
                    })
                }
            };
            curves.insert(e.id, curve);
        }
        solved.curves = curves;
        solved.radii = radii;
        for d in dimensions.iter().filter(|d| d.value.is_none()) {
            if let Some(value) = measure(d.kind, &solved) {
                solved.status.driven.insert(d.id, value);
            }
        }
        solved
    }

    fn analyze(&mut self, solved: &mut Solved) {
        let a = self.system.analyze();
        let status = &mut solved.status;
        status.dof = a.dof;
        let by_point: BTreeMap<PointId, EntityUid> =
            self.points.iter().map(|(u, p)| (*p, *u)).collect();
        let by_curve: BTreeMap<EntityId, EntityUid> =
            self.curves.iter().map(|(u, c)| (*c, *u)).collect();
        status.fully_constrained = a
            .fully_constrained_points
            .iter()
            .filter_map(|p| by_point.get(p))
            .chain(
                a.fully_constrained_entities
                    .iter()
                    .filter_map(|e| by_curve.get(e)),
            )
            .copied()
            .collect();
        status.redundant = self.items(&a.redundant_constraints());
        status.conflicts = a
            .conflicts
            .iter()
            .map(|d| self.items(&d.involved))
            .collect();
    }
}

/// The value a dimension measures in solved geometry.
pub fn measure(kind: &DimensionKind, solved: &Solved) -> Option<f64> {
    use DimensionKind as D;
    let p = |u: &EntityUid| solved.points.get(u).copied();
    let line = |u: &EntityUid| match solved.curves.get(u)? {
        Curve2::Line { a, b } => Some((*a, *b)),
        _ => None,
    };
    let point_line = |q: P2, (a, b): (P2, P2)| cross(sub(q, a), unit(sub(b, a))).abs();
    let radius = |u: &EntityUid| match solved.curves.get(u)? {
        Curve2::Circle { radius, .. } | Curve2::Arc { radius, .. } => Some(*radius),
        _ => None,
    };
    Some(match kind {
        D::Distance { a, b } => dist(p(a)?, p(b)?),
        D::HorizontalDistance { a, b } => (p(b)?[0] - p(a)?[0]).abs(),
        D::VerticalDistance { a, b } => (p(b)?[1] - p(a)?[1]).abs(),
        D::PointLineDistance { point, line: l } => point_line(p(point)?, line(l)?),
        D::LineDistance { a, b } => {
            let (b0, b1) = line(b)?;
            point_line([(b0[0] + b1[0]) / 2.0, (b0[1] + b1[1]) / 2.0], line(a)?)
        }
        D::Length { line: l } => {
            let (a, b) = line(l)?;
            dist(a, b)
        }
        D::Angle { a, b } => {
            let (a0, a1) = line(a)?;
            let (b0, b1) = line(b)?;
            dot(unit(sub(a1, a0)), unit(sub(b1, b0)))
                .clamp(-1.0, 1.0)
                .acos()
        }
        D::Radius { curve } => radius(curve)?,
        D::Diameter { curve } => 2.0 * radius(curve)?,
        D::ArcLength { arc } => match solved.curves.get(arc)? {
            Curve2::Arc {
                radius, start, end, ..
            } => radius * (end - start),
            _ => return None,
        },
        D::MajorRadius { ellipse } => match solved.curves.get(ellipse)? {
            Curve2::Ellipse { major, .. } | Curve2::EllipticalArc { major, .. } => *major,
            _ => return None,
        },
        D::MinorRadius { ellipse } => match solved.curves.get(ellipse)? {
            Curve2::Ellipse { minor, .. } | Curve2::EllipticalArc { minor, .. } => *minor,
            _ => return None,
        },
        D::LinearDiameter { axis, entity } => {
            let axis = line(axis)?;
            2.0 * match entity {
                Ref::Point(q) => point_line(p(q)?, axis),
                Ref::Curve(l) => {
                    let (b0, b1) = line(l)?;
                    point_line([(b0[0] + b1[0]) / 2.0, (b0[1] + b1[1]) / 2.0], axis)
                }
            }
        }
    })
}

/// Solves the sketch from its stored positions.
pub fn solve(
    entities: &[Entity],
    constraints: &[Constraint],
    dimensions: &[DimensionInput<'_>],
) -> Result<Solved, SolveError> {
    solve_extra(entities, constraints, dimensions, &Extra::default())
}

/// [`solve`] with pattern bindings and entities left out.
pub fn solve_extra(
    entities: &[Entity],
    constraints: &[Constraint],
    dimensions: &[DimensionInput<'_>],
    extra: &Extra<'_>,
) -> Result<Solved, SolveError> {
    let mut built =
        Built::new(entities, constraints, dimensions, extra).map_err(SolveError::new)?;
    built.solve()?;
    let mut solved = built.read(entities, dimensions);
    built.analyze(&mut solved);
    Ok(solved)
}

/// Solves, then pulls the goals toward their targets as far as the
/// constraints allow (the solver's drag).
pub fn drag(
    entities: &[Entity],
    constraints: &[Constraint],
    dimensions: &[DimensionInput<'_>],
    goals: &[Goal],
) -> Result<Solved, SolveError> {
    drag_extra(entities, constraints, dimensions, &Extra::default(), goals)
}

/// [`drag`] with pattern bindings and entities left out.
pub fn drag_extra(
    entities: &[Entity],
    constraints: &[Constraint],
    dimensions: &[DimensionInput<'_>],
    extra: &Extra<'_>,
    goals: &[Goal],
) -> Result<Solved, SolveError> {
    let mut built =
        Built::new(entities, constraints, dimensions, extra).map_err(SolveError::new)?;
    built.solve()?;
    let mut solver_goals = Vec::new();
    for goal in goals {
        solver_goals.push(match *goal {
            Goal::Point(uid, target) => DragGoal::Point {
                point: built.point(uid).map_err(SolveError::new)?,
                target,
            },
            Goal::Radius(uid, target) => DragGoal::Radius {
                entity: built.curve(uid).map_err(SolveError::new)?,
                target,
            },
        });
    }
    built.system.drag(&solver_goals, &SolveOptions::default());
    // The drag leaves the constraints satisfied; tighten as after a solve.
    built.solve()?;
    let mut solved = built.read(entities, dimensions);
    built.analyze(&mut solved);
    Ok(solved)
}
