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
// Equal spacing (mitcad#221).
use mitcad_solver::{Anchor as SolverAnchor, SpacingMode as SolverSpacing};

use super::geometry::{
    Curve2, Nurbs, P2, angle_after, angle_on_side, cross, dist, dot, norm, sub, unit,
};
use super::pattern::{Binding, BindingKind};
use super::{Constraint, ConstraintKind, ConstraintUid, DimensionKind, Entity, EntityKind, Ref};
use super::{SpacingMode, spacing};
use crate::ids::EntityUid;

/// The tolerance of a second solve where the first did not converge
/// (relative to the sketch size, as [`SolveOptions::tolerance`]; 1e-5 mm
/// on a sketch 100 mm across).
const LOOSE_TOLERANCE: f64 = 1e-7;

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
    /// The constraints and dimensions the stored positions did not
    /// satisfy before the solve (those of an imported sketch the file's
    /// own solution does not keep).
    pub stored_violations: Vec<SolverItem>,
    /// For each redundant constraint, the set it follows from (itself
    /// included), so that a refusal can name them (mitcad#221).
    pub redundant_sets: Vec<Vec<SolverItem>>,
    /// The inactive constraints and dimensions (mitcad#721) the solved
    /// geometry does not meet (1e-4 mm): those that would move it when
    /// activated. An import finds with it what of a sketch its file does not
    /// keep.
    pub unmet_inactive: Vec<ConstraintUid>,
}

impl SketchStatus {
    /// The sketch warning about collapsed curves (mitcad#365), `None` when
    /// none has collapsed. The palette shows it; [`degenerate`] names what
    /// it is about.
    ///
    /// [`degenerate`]: SketchStatus::degenerate
    pub fn degenerate_warning(&self) -> Option<String> {
        if self.degenerate.is_empty() {
            return None;
        }
        let names: Vec<String> = self
            .degenerate
            .iter()
            .map(|id| Ref::Curve(*id).to_string())
            .collect();
        Some(format!(
            "{} collapsed (a line of no length or a circle or arc of no radius) and makes no \
             region",
            names.join(", ")
        ))
    }
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
/// Inactive dimensions (mitcad#721) are not solver inputs; see
/// [`unmet`] for them.
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
/// pattern copies, the entities it leaves out (curves of derived offsets,
/// computed after it), and where its orientation choices come from.
#[derive(Debug, Clone, Copy)]
pub struct Extra<'a> {
    pub bindings: &'a [Binding],
    pub skip: &'a BTreeSet<EntityUid>,
    /// The positions the solver takes its orientation choices from (the
    /// side of a point-line, line-line or tangent distance, of a tangency,
    /// the quadrant of an angle) instead of the positions the solve starts
    /// from: the sketch as stored, when linked projections have moved to
    /// their sources' new places before the solve (mitcad#1047). Each
    /// point (by id) and each circle's radius and ellipse's minor radius is
    /// taken from here where it has one of the same kind.
    pub oriented: Option<&'a [Entity]>,
    /// Stored constraints and dimensions the solve leaves out: those of the
    /// offsets made by constraints that their tangent corners and helper
    /// anchors (in `bindings`) imply (mitcad#1171,
    /// [`crate::offset::independent`]). They stay in the definition.
    pub left_out: &'a BTreeSet<ConstraintUid>,
    /// The owners of bindings that only restate their own definition (the
    /// offsets made by constraints, mitcad#1171): a dependent set of their
    /// bindings and fixes alone is theirs, not redundant to the user (the
    /// closing corner of a closed chain of tangent arcs).
    pub restating: &'a BTreeSet<ConstraintUid>,
}

static NOTHING: BTreeSet<EntityUid> = BTreeSet::new();
static NONE_LEFT_OUT: BTreeSet<ConstraintUid> = BTreeSet::new();

impl Default for Extra<'_> {
    fn default() -> Self {
        Self {
            bindings: &[],
            skip: &NOTHING,
            oriented: None,
            left_out: &NONE_LEFT_OUT,
            restating: &NONE_LEFT_OUT,
        }
    }
}

/// Where the solver system is built: each entity of `entities` as
/// `oriented` has it (same id and kind), else as it is (mitcad#1047).
fn placed<'a>(oriented: &BTreeMap<EntityUid, &'a EntityKind>, e: &'a Entity) -> &'a EntityKind {
    match oriented.get(&e.id) {
        Some(&kind) if kind.type_name() == e.kind.type_name() => kind,
        _ => &e.kind,
    }
}

/// The solver system of a sketch with the mapping back to ids.
struct Built {
    system: System,
    points: BTreeMap<EntityUid, PointId>,
    curves: BTreeMap<EntityUid, EntityId>,
    items: BTreeMap<ConstraintId, SolverItem>,
    /// What equal spacings measure from (mitcad#221): points, circle and
    /// arc centres, line midpoints.
    anchors: BTreeMap<Ref, SolverAnchor>,
    /// The equal spacings, for their spacing dimensions.
    spacings: BTreeMap<ConstraintUid, (Vec<Ref>, SpacingMode, Option<EntityUid>)>,
    /// The curves the last converged solve left collapsed.
    degenerate: Vec<EntityId>,
    /// [`Extra::restating`].
    restating: BTreeSet<ConstraintUid>,
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
        // Built where `oriented` has the geometry, so that the constraints
        // take their orientation choices there, and moved to `entities`'
        // positions once they are in (mitcad#1047).
        let oriented: BTreeMap<EntityUid, &EntityKind> = extra
            .oriented
            .map(|o| o.iter().map(|e| (e.id, &e.kind)).collect())
            .unwrap_or_default();
        for e in &entities {
            if let EntityKind::Point { at } = placed(&oriented, e) {
                points.insert(e.id, system.add_point(at[0], at[1]));
            }
        }
        // A circle's radius or an ellipse's minor radius where it is built.
        let size = |e: &Entity, own: f64| match placed(&oriented, e) {
            EntityKind::Circle { radius, .. } => *radius,
            EntityKind::Ellipse { minor_radius, .. }
            | EntityKind::EllipticalArc { minor_radius, .. } => *minor_radius,
            _ => own,
        };
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
                EntityKind::Circle { center, radius, .. } => system
                    .add_circle(p(*center)?, size(e, *radius))
                    .map_err(what)?,
                EntityKind::Arc { center, start, end } => system
                    .add_arc(p(*center)?, p(*start)?, p(*end)?)
                    .map_err(what)?,
                EntityKind::Ellipse {
                    center,
                    major,
                    minor_radius,
                } => system
                    .add_ellipse(p(*center)?, p(*major)?, size(e, *minor_radius))
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
                        size(e, *minor_radius),
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
                EntityKind::FittedSpline {
                    points: fit,
                    closed,
                } => {
                    let fit = fit.iter().map(|u| p(*u)).collect::<Result<Vec<_>, _>>()?;
                    // A closed one is periodic (mitcad#783).
                    let end = if *closed {
                        SplineEnd::Periodic
                    } else {
                        SplineEnd::Natural
                    };
                    system.add_fitted_spline(&fit, [end, end]).map_err(what)?
                }
            };
            curves.insert(e.id, id);
        }
        let mut anchors = BTreeMap::new();
        for e in &entities {
            let anchor = match &e.kind {
                EntityKind::Point { .. } => points.get(&e.id).map(|p| SolverAnchor::Point(*p)),
                EntityKind::Circle { center, .. } | EntityKind::Arc { center, .. } => {
                    points.get(center).map(|p| SolverAnchor::Point(*p))
                }
                EntityKind::Line { start, end, .. } => points
                    .get(start)
                    .zip(points.get(end))
                    .map(|(a, b)| SolverAnchor::Mid(*a, *b)),
                _ => None,
            };
            if let Some(anchor) = anchor {
                anchors.insert(e.as_ref(), anchor);
            }
        }
        // Inactive constraints are left out (mitcad#721).
        let constraints: Vec<&Constraint> = constraints.iter().filter(|c| c.active).collect();
        let spacings = constraints
            .iter()
            .filter_map(|c| match &c.kind {
                ConstraintKind::EqualSpacing {
                    anchors,
                    mode,
                    direction,
                } => Some((c.id, (anchors.clone(), *mode, *direction))),
                _ => None,
            })
            .collect();
        let mut built = Self {
            system,
            points,
            curves,
            items: BTreeMap::new(),
            anchors,
            spacings,
            degenerate: Vec::new(),
            restating: extra.restating.clone(),
        };
        // A fixed line whose both end points are fixed (the sketch's axis
        // lines, mitcad#500) is held by its points' fixes; fixing the line
        // too would only repeat them and be reported redundant
        // (mitcad#877). So is a spline made only of fixed control or fit
        // points (mitcad#1171). A circle or an ellipse has a radius of its
        // own and keeps its fix. An arc keeps its fix too, even with its
        // centre and ends fixed (a projected one): its own equation puts its
        // end on the circle through its start, and without the fix the solve
        // takes another path through the fixed points' equations, which
        // moves a point that a sketch puts where two tangent circles meet
        // (where only the solve's last steps place it) by micrometres. A
        // linked profile of an imported design moved that much no longer
        // joined its body (mitcad#1171, found by a FreeCAD document). That
        // its fix only repeats the points' is not reported: a redundant set
        // of fixes alone is left out (`Built::analyze`). The curve stays fixed
        // in the definition.
        let fixed_points: BTreeSet<EntityUid> = entities
            .iter()
            .filter(|e| e.fixed && e.is_point())
            .map(|e| e.id)
            .collect();
        let held_by_points = |e: &Entity| {
            let all = |points: &[EntityUid]| points.iter().all(|p| fixed_points.contains(p));
            match &e.kind {
                EntityKind::Line { start, end, .. } => all(&[*start, *end]),
                EntityKind::Spline { control, .. } => all(control),
                EntityKind::FittedSpline { points, .. } => all(points),
                _ => false,
            }
        };
        for e in entities.iter().filter(|e| e.fixed && !held_by_points(e)) {
            let c = if e.is_point() {
                S::FixPoint(built.points[&e.id])
            } else {
                S::FixEntity(built.curves[&e.id])
            };
            built.add(c, SolverItem::Fixed(e.as_ref()))?;
        }
        for c in constraints
            .iter()
            .filter(|c| !extra.left_out.contains(&c.id))
        {
            let item = SolverItem::Constraint(c.id);
            for s in built
                .constraint(&c.kind)
                .map_err(|e| format!("{}: {e}", c.id))?
            {
                built.add(s, item)?;
            }
        }
        for d in dimensions
            .iter()
            .filter(|d| !extra.left_out.contains(&d.id))
        {
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
        if !oriented.is_empty() {
            built.start_at(&entities)?;
        }
        Ok(built)
    }

    /// Moves the system's points, circle radii and ellipse minor radii to
    /// where `entities` have them: the solve starts there, while the
    /// orientation choices the constraints took where the system was built
    /// stay (mitcad#1047).
    fn start_at(&mut self, entities: &[&Entity]) -> Result<(), String> {
        for e in entities {
            let what = |error: mitcad_solver::Error| format!("{}: {error}", e.as_ref());
            match &e.kind {
                EntityKind::Point { at } => {
                    if let Some(&id) = self.points.get(&e.id) {
                        self.system.set_point(id, *at).map_err(what)?;
                    }
                }
                EntityKind::Circle { radius, .. } => {
                    if let Some(&id) = self.curves.get(&e.id) {
                        self.system.set_radius(id, *radius).map_err(what)?;
                    }
                }
                EntityKind::Ellipse { minor_radius, .. }
                | EntityKind::EllipticalArc { minor_radius, .. } => {
                    if let Some(&id) = self.curves.get(&e.id) {
                        self.system
                            .set_minor_radius(id, *minor_radius)
                            .map_err(what)?;
                    }
                }
                _ => {}
            }
        }
        Ok(())
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
            BindingKind::TangentAt { a, b, at } => S::TangentAt {
                a: c(a)?,
                b: c(b)?,
                at: p(at)?,
            },
            BindingKind::Midpoint { point, curve } => S::Midpoint(p(point)?, c(curve)?),
            BindingKind::HorizontalPoints { a, b } => S::HorizontalPoints(p(a)?, p(b)?),
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
            K::Tangent { a, b, at: None } => vec![S::Tangent(c(*a)?, c(*b)?)],
            K::Smooth { a, b, at: None } => vec![S::Smooth(c(*a)?, c(*b)?)],
            // At a chosen end (mitcad#783).
            K::Tangent { a, b, at: Some(q) } => vec![S::TangentAt {
                a: c(*a)?,
                b: c(*b)?,
                at: p(*q)?,
            }],
            K::Smooth { a, b, at: Some(q) } => vec![S::SmoothAt {
                a: c(*a)?,
                b: c(*b)?,
                at: p(*q)?,
            }],
            K::Equal { a, b } => vec![S::Equal(c(*a)?, c(*b)?)],
            K::Concentric { a, b } => vec![S::Concentric(c(*a)?, c(*b)?)],
            K::Midpoint { point, curve } => vec![S::Midpoint(p(*point)?, c(*curve)?)],
            K::Symmetric { a, b, axis, radii } => vec![match (a, b) {
                (Ref::Point(a), Ref::Point(b)) => S::SymmetricPoints {
                    a: p(*a)?,
                    b: p(*b)?,
                    axis: c(*axis)?,
                },
                (Ref::Curve(a), Ref::Curve(b)) if *radii => S::SymmetricEntities {
                    a: c(*a)?,
                    b: c(*b)?,
                    axis: c(*axis)?,
                },
                // The positions only (mitcad#865).
                (Ref::Curve(a), Ref::Curve(b)) => S::SymmetricCentres {
                    a: c(*a)?,
                    b: c(*b)?,
                    axis: c(*axis)?,
                },
                _ => return Err("symmetric entities must be two points or two curves".to_owned()),
            }],
            K::EqualSpacing {
                anchors,
                mode,
                direction,
            } => vec![S::EqualSpacing {
                anchors: anchors
                    .iter()
                    .map(|r| self.anchor(*r))
                    .collect::<Result<Vec<_>, _>>()?,
                mode: self.spacing_mode(*mode, *direction)?,
            }],
        })
    }

    /// What an equal spacing measures from.
    fn anchor(&self, r: Ref) -> Result<SolverAnchor, String> {
        self.anchors
            .get(&r)
            .copied()
            .ok_or_else(|| format!("{r} is not a point, line, circle or arc to space"))
    }

    fn spacing_mode(
        &self,
        mode: SpacingMode,
        direction: Option<EntityUid>,
    ) -> Result<SolverSpacing, String> {
        Ok(match mode {
            SpacingMode::Horizontal => SolverSpacing::Horizontal,
            SpacingMode::Vertical => SolverSpacing::Vertical,
            SpacingMode::Along => SolverSpacing::Along(
                self.curve(direction.ok_or("an along spacing needs a direction line")?)?,
            ),
            SpacingMode::PointToPoint => SolverSpacing::PointToPoint,
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
            D::HorizontalDistance { a, b, signed } => S::HorizontalDistance {
                a: p(*a)?,
                b: p(*b)?,
                value,
                signed: *signed,
            },
            D::VerticalDistance { a, b, signed } => S::VerticalDistance {
                a: p(*a)?,
                b: p(*b)?,
                value,
                signed: *signed,
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
            D::TangentDistance { circle, line } => S::TangentDistance {
                circle: c(*circle)?,
                line: c(*line)?,
                value,
            },
            D::ConcentricDistance { a, b } => S::RadiusGap {
                a: c(*a)?,
                b: c(*b)?,
                value,
            },
            D::Length { line } => S::Length {
                line: c(*line)?,
                value,
            },
            D::Angle { a, b, .. } => S::Angle {
                a: c(*a)?,
                b: c(*b)?,
                value,
                // Reversing both rays keeps the angle.
                supplement: kind.angle_side().map(|[ra, rb]| ra != rb),
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
            // The first interval; the equal spacing passes it on.
            D::Spacing { constraint } => {
                let Some((anchors, mode, direction)) = self.spacings.get(constraint) else {
                    return Err(format!("{constraint} is not an equal spacing"));
                };
                let [a, b, ..] = anchors.as_slice() else {
                    return Err(format!("{constraint} has fewer than two anchors"));
                };
                S::Spacing {
                    a: self.anchor(*a)?,
                    b: self.anchor(*b)?,
                    mode: self.spacing_mode(*mode, *direction)?,
                    value,
                }
            }
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

    /// The items the stored positions do not satisfy (1e-4 mm), before
    /// the first solve.
    fn violated(&mut self) -> Vec<SolverItem> {
        let ids = self.system.violated(1e-4);
        self.items(&ids)
    }

    fn solve(&mut self) -> Result<(), SolveError> {
        let mut result = self.system.solve(&SolveOptions::default());
        if result.status == SolveStatus::NotConverged {
            // Geometry that is fixed but not exact (a linked curve whose
            // stored end lies a few millionths of a millimetre off its arc)
            // leaves a residual no solve can remove: a looser tolerance
            // takes it, one that still holds the constraints far below
            // anything a design means.
            let loose = SolveOptions {
                tolerance: LOOSE_TOLERANCE,
                ..SolveOptions::default()
            };
            let again = self.system.solve(&loose);
            if again.status == SolveStatus::Converged {
                result = again;
            }
        }
        match result.status {
            SolveStatus::Converged => {
                self.degenerate = result.degenerate.clone();
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

    fn read(
        &self,
        entities: &[Entity],
        constraints: &[Constraint],
        dimensions: &[DimensionInput<'_>],
    ) -> Solved {
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
                EntityKind::Circle { center, start, .. } => {
                    let radius = s.radius(id).expect("a circle");
                    radii.insert(e.id, radius);
                    Curve2::Circle {
                        center: at(center),
                        radius,
                        start: *start,
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
            if let Some(value) = measure_in(d.kind, &solved, constraints) {
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
        status.degenerate = self
            .degenerate
            .iter()
            .filter_map(|e| by_curve.get(e).copied())
            .collect();
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
        // Fixed geometry that follows from other fixed geometry is not
        // redundant to the user (mitcad#1171): the three fixed points of a
        // projected arc, whose end the arc's own equation puts on the
        // circle through its start; a projected circle's fixed centre and
        // its own fix. A set that only fixes take part in is left out, and
        // so is one of fixes and an offset's own bindings, which restate
        // what the offset holds (the closing corner of a closed chain of
        // tangent arcs, whose arcs' equations and corners close the loop).
        let restating = &self.restating;
        let internal = |s: &[SolverItem]| {
            s.iter().all(|i| match i {
                SolverItem::Fixed(_) => true,
                SolverItem::Constraint(k) => restating.contains(k),
            })
        };
        let dependent: Vec<_> = a
            .redundant
            .iter()
            .map(|d| (self.items(&[d.constraint]), self.items(&d.involved)))
            .filter(|(_, set)| !internal(set))
            .collect();
        let mut redundant: Vec<SolverItem> = dependent
            .iter()
            .flat_map(|(item, _)| item.iter().copied())
            .collect();
        redundant.sort();
        redundant.dedup();
        status.redundant = redundant;
        status.redundant_sets = dependent.into_iter().map(|(_, set)| set).collect();
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
        D::HorizontalDistance { a, b, signed } => signed_or_abs(p(b)?[0] - p(a)?[0], *signed),
        D::VerticalDistance { a, b, signed } => signed_or_abs(p(b)?[1] - p(a)?[1], *signed),
        D::PointLineDistance { point, line: l } => point_line(p(point)?, line(l)?),
        D::LineDistance { a, b } => {
            let (b0, b1) = line(b)?;
            point_line([(b0[0] + b1[0]) / 2.0, (b0[1] + b1[1]) / 2.0], line(a)?)
        }
        D::TangentDistance { circle, line: l } => {
            let centre = match solved.curves.get(circle)? {
                Curve2::Circle { center, .. } | Curve2::Arc { center, .. } => *center,
                _ => return None,
            };
            point_line(centre, line(l)?) - radius(circle)?
        }
        D::ConcentricDistance { a, b } => (radius(b)? - radius(a)?).abs(),
        D::Length { line: l } => {
            let (a, b) = line(l)?;
            dist(a, b)
        }
        D::Angle { a, b, .. } => angle_on_side(
            line(a)?,
            line(b)?,
            kind.angle_side().unwrap_or([false, false]),
        ),
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
        // Measured on its constraint: see `measure_in`.
        D::Spacing { .. } => return None,
    })
}

/// A signed dimension's offset as it is, another one's size.
fn signed_or_abs(offset: f64, signed: bool) -> f64 {
    if signed { offset } else { offset.abs() }
}

/// [`measure`], also of a spacing dimension: the size of its equal
/// spacing's first interval (mitcad#221).
pub fn measure_in(
    kind: &DimensionKind,
    solved: &Solved,
    constraints: &[Constraint],
) -> Option<f64> {
    if let DimensionKind::Spacing { constraint } = kind {
        let ConstraintKind::EqualSpacing {
            anchors,
            mode,
            direction,
        } = &constraints.iter().find(|c| c.id == *constraint)?.kind
        else {
            return None;
        };
        return spacing::intervals(solved, anchors, *mode, *direction)?
            .first()
            .map(|v| v.abs());
    }
    measure(kind, solved)
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
    let violated = built.violated();
    built.solve()?;
    let mut solved = built.read(entities, constraints, dimensions);
    built.analyze(&mut solved);
    solved.status.stored_violations = violated;
    Ok(solved)
}

/// The constraints and dimensions that `entities`' positions do not meet
/// (1e-4 mm), without solving: every one of `constraints` (active or not)
/// and the driving ones of `dimensions`. Empty when they cannot be built
/// into a system. The solve's [`SketchStatus::unmet_inactive`] (mitcad#721)
/// is this on the solved positions, for the inactive ones.
pub fn unmet(
    entities: &[Entity],
    constraints: &[Constraint],
    dimensions: &[DimensionInput<'_>],
    extra: &Extra<'_>,
) -> Vec<ConstraintUid> {
    let all: Vec<Constraint> = constraints
        .iter()
        .map(|c| Constraint {
            active: true,
            ..c.clone()
        })
        .collect();
    let Ok(mut built) = Built::new(entities, &all, dimensions, extra) else {
        return Vec::new();
    };
    built
        .violated()
        .into_iter()
        .filter_map(|item| match item {
            SolverItem::Constraint(k) => Some(k),
            SolverItem::Fixed(_) => None,
        })
        .collect()
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
    let mut solved = built.read(entities, constraints, dimensions);
    built.analyze(&mut solved);
    Ok(solved)
}
