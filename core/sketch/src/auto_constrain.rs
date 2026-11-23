// SPDX-License-Identifier: MIT
//! Auto Constrain (mitcad#2019): a sketch that exists (an imported DXF, one
//! drawn without inference, one whose constraints were deleted) made fully
//! constrained in one edit, `sketch.auto_constrain`.
//!
//! **Phase 1, relations.** The relations the geometry already has within
//! the length and angle tolerances are found and added, the stronger ones
//! first: curve ends at one place joined (as `sketch.add_constraint` joins
//! them, mitcad#260), points at a line's middle, points on curves, tangent
//! joins of lines and arcs, equal radii, horizontal and vertical lines,
//! collinear, perpendicular (at a shared corner) and parallel lines, equal
//! lengths, concentric circles and arcs, points mirrored about a construction
//! line or a sketch axis, and points at the origin or on the sketch axes.
//!
//! **Phase 2, dimensions.** The degrees of freedom still free get driving
//! dimensions with the values the geometry has: the sizes first (line
//! lengths, radii and diameters, the angle at a corner of lines that are
//! neither perpendicular nor parallel), then the positions from the sketch's
//! origin (the horizontal and vertical distance of each connected part's
//! point nearest to the origin, the angle of one of its lines to the x
//! axis, then whatever point is still free). Their values are placed beside
//! their curves, apart from each other.
//!
//! Every candidate is tried on a copy of the sketch first and added only
//! when it takes a degree of freedom away, the redundant and conflicting
//! count does not rise (nothing is implied by the others, not even in
//! part), no curve collapses, and the solve leaves every point and radius
//! within the length tolerance of where it was when the command started.
//! A refused candidate is skipped silently.

use std::collections::{BTreeMap, BTreeSet};

use super::axes::{axis_line, origin_point};
use super::edit::SketchEdit;
use super::geometry::{Curve2, P2, add, cross, dist, dot, norm, perp, scale, sub, unit};
use super::solve::{SolveError, Solved, measure_in};
use super::{
    Constraint, ConstraintKind, ConstraintUid, Dimension, DimensionKind, EntityKind, Ref,
    SketchAxis,
};
use crate::def::SketchDef;
use crate::ids::EntityUid;
use crate::parameters::ParamId;

/// The length tolerance the application starts with (mm).
pub const DEFAULT_LENGTH_TOLERANCE: f64 = 0.01;

/// The angle tolerance the application starts with (degrees).
pub const DEFAULT_ANGLE_TOLERANCE: f64 = 0.5;

/// The id a candidate has while it is tried on a copy of the sketch.
const TRIAL: ConstraintUid = ConstraintUid(u32::MAX);

/// The parameter a trial dimension's value comes from.
const TRIAL_VALUE: ParamId = ParamId::from_raw(u32::MAX);

/// How many earlier lines or curves a line or curve is tried with for one
/// relation of a pair (parallel, equal, concentric): the first that holds
/// is enough, the others then follow from it.
const PARTNERS: usize = 4;

/// What `sketch.auto_constrain` is asked for.
#[derive(Debug, Clone, PartialEq)]
pub struct AutoConstrainOptions {
    /// How far apart (mm) two places may be and still count as one, and how
    /// far the solve may move a point or change a radius.
    pub length_tolerance: f64,
    /// How far (radians) from level, square or parallel a direction may be.
    pub angle_tolerance: f64,
    /// Phase 1: the relations the geometry has.
    pub relations: bool,
    /// Phase 2: dimensions for what is left free.
    pub dimensions: bool,
    /// The curves and points to constrain; the whole sketch when None.
    pub scope: Option<Vec<Ref>>,
}

impl Default for AutoConstrainOptions {
    fn default() -> Self {
        Self {
            length_tolerance: DEFAULT_LENGTH_TOLERANCE,
            angle_tolerance: DEFAULT_ANGLE_TOLERANCE.to_radians(),
            relations: true,
            dimensions: true,
            scope: None,
        }
    }
}

/// What `sketch.auto_constrain` added.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AutoConstrained {
    /// The constraints added (a join of two curve ends is none: the points
    /// became one).
    pub constraints: Vec<ConstraintUid>,
    /// The dimensions added.
    pub dimensions: Vec<ConstraintUid>,
    /// Curve ends joined into one point.
    pub joined: usize,
    /// How many of each type were added: constraint types
    /// (`coincident` counts the joins too) and dimension types.
    pub added: BTreeMap<String, usize>,
    /// The sketch's degrees of freedom afterwards.
    pub dof: usize,
}

impl AutoConstrained {
    /// The relations added: constraints and joins.
    pub fn relations(&self) -> usize {
        self.constraints.len() + self.joined
    }

    /// The summary for agents and the log: "12 constraints, 5 dimensions;
    /// the sketch is fully constrained".
    pub fn summary(&self) -> String {
        let count = |n: usize, one: &str, many: &str| {
            if n == 1 {
                format!("1 {one}")
            } else {
                format!("{n} {many}")
            }
        };
        let added = format!(
            "{}, {}",
            count(self.relations(), "constraint", "constraints"),
            count(self.dimensions.len(), "dimension", "dimensions")
        );
        let left = match self.dof {
            0 => "the sketch is fully constrained".to_owned(),
            n => count(n, "degree of freedom left", "degrees of freedom left"),
        };
        format!("{added}; {left}")
    }
}

/// What a point is to the curves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Role {
    /// An end of a curve.
    End,
    /// The centre of a circle, an arc or an ellipse.
    Center,
    /// A point of no curve.
    Lone,
}

/// A point a relation can hold.
#[derive(Debug, Clone)]
struct Spot {
    id: EntityUid,
    at: P2,
    role: Role,
    /// The curves it is an end of.
    ends_of: Vec<EntityUid>,
}

#[derive(Debug, Clone)]
struct LineView {
    id: EntityUid,
    a: EntityUid,
    b: EntityUid,
    pa: P2,
    pb: P2,
    construction: bool,
    /// A construction line or a centre line: an axis of symmetry.
    mirror: bool,
}

impl LineView {
    fn length(&self) -> f64 {
        dist(self.pa, self.pb)
    }

    fn direction(&self) -> P2 {
        unit(sub(self.pb, self.pa))
    }

    /// Its direction leaving `p`, an end of it.
    fn leaving(&self, p: EntityUid) -> P2 {
        if p == self.a {
            unit(sub(self.pb, self.pa))
        } else {
            unit(sub(self.pa, self.pb))
        }
    }
}

/// A circle or a circular arc.
#[derive(Debug, Clone)]
struct RoundView {
    id: EntityUid,
    center: EntityUid,
    c: P2,
    radius: f64,
    /// An arc's start and end points and their positions.
    ends: Option<[(EntityUid, P2); 2]>,
}

/// A curve a point can lie on.
#[derive(Debug, Clone)]
struct CurveView {
    id: EntityUid,
    curve: Curve2,
    /// Its own points.
    points: Vec<EntityUid>,
    /// The positions of its ends.
    ends: Vec<P2>,
}

/// The sketch as one pass sees it: the solved positions of what is in
/// scope.
#[derive(Debug, Clone, Default)]
struct View {
    spots: Vec<Spot>,
    lines: Vec<LineView>,
    rounds: Vec<RoundView>,
    curves: Vec<CurveView>,
    /// Ellipses and elliptical arcs.
    ellipses: Vec<EntityUid>,
    /// Every point in scope, also the inner points of splines and the major
    /// ends of ellipses.
    points: Vec<(EntityUid, P2)>,
    /// The lower left and upper right corners of the geometry.
    low: P2,
    high: P2,
}

impl View {
    /// Grows the bounds to hold `low` and `high`.
    fn cover(&mut self, low: P2, high: P2) {
        self.low = [self.low[0].min(low[0]), self.low[1].min(low[1])];
        self.high = [self.high[0].max(high[0]), self.high[1].max(high[1])];
    }

    fn line(&self, id: EntityUid) -> Option<&LineView> {
        self.lines.iter().find(|l| l.id == id)
    }

    fn center(&self) -> P2 {
        scale(add(self.low, self.high), 0.5)
    }
}

/// The state of one run.
struct Run {
    tol: f64,
    sin: f64,
    /// The entities a candidate may refer to besides the origin and the
    /// axes; every one when None.
    scope: Option<BTreeSet<EntityUid>>,
    original_points: BTreeMap<EntityUid, P2>,
    original_radii: BTreeMap<EntityUid, f64>,
    current: Solved,
    made: AutoConstrained,
    /// The origin point and axis entities this run made, in order.
    made_axes: Vec<EntityUid>,
    /// Where the values of the dimensions added are shown.
    placed: Vec<P2>,
    /// How far a value is shown from its curve (mm).
    gap: f64,
    /// How many horizontal and vertical positions were placed.
    rows: [usize; 2],
}

impl Run {
    fn in_scope(&self, refs: &[Ref], axes: &BTreeSet<EntityUid>) -> bool {
        refs.iter().all(|r| {
            axes.contains(&r.uid()) || self.scope.as_ref().is_none_or(|s| s.contains(&r.uid()))
        })
    }

    /// Whether everything `refs` names is fully constrained already (the
    /// origin and the axes are).
    fn settled(&self, refs: &[Ref], axes: &BTreeSet<EntityUid>) -> bool {
        refs.iter().all(|r| {
            axes.contains(&r.uid()) || self.current.status.fully_constrained.contains(&r.uid())
        })
    }

    /// Whether `after` is a solve the candidate may give: it takes freedom
    /// away, adds no redundancy, conflict or collapsed curve, and leaves
    /// every point and radius within the tolerance of where it started.
    fn holds(&self, after: &Solved) -> bool {
        let before = &self.current.status;
        let status = &after.status;
        if status.dof >= before.dof
            || status.redundant.len() > before.redundant.len()
            || status.conflicts.len() > before.conflicts.len()
            || status.degenerate.len() > before.degenerate.len()
        {
            return false;
        }
        let points = after.points.iter().all(|(p, at)| {
            self.original_points
                .get(p)
                .is_none_or(|was| dist(*was, *at) <= self.tol)
        });
        let radii = after.radii.iter().all(|(c, r)| {
            self.original_radii
                .get(c)
                .is_none_or(|was| (was - r).abs() <= self.tol)
        });
        points && radii
    }

    /// Where a dimension's value goes: `at`, moved on along `away` while it
    /// is on top of one placed before.
    fn place(&mut self, at: P2, away: P2) -> P2 {
        let step = scale(unit(away), self.gap * 0.8);
        let mut at = at;
        for _ in 0..24 {
            if self.placed.iter().all(|p| dist(*p, at) >= self.gap * 0.7) {
                break;
            }
            at = add(at, step);
        }
        self.placed.push(at);
        at
    }
}

/// A connected part of the sketch: its points with their positions, and
/// its lines.
type Part = (Vec<(EntityUid, P2)>, Vec<EntityUid>);

/// A line of symmetry: the line, or the sketch axis it is to be; a point on
/// it, its direction and its own points.
type Mirror = (Result<EntityUid, SketchAxis>, P2, P2, Vec<EntityUid>);

/// The point `p` mirrored about the line through `o` along the unit `u`.
fn mirrored(p: P2, o: P2, u: P2) -> P2 {
    let v = sub(p, o);
    let along = scale(u, dot(v, u));
    add(o, sub(scale(along, 2.0), v))
}

/// The distance of `p` from the line through `o` along the unit `u`.
fn off_line(p: P2, o: P2, u: P2) -> f64 {
    cross(u, sub(p, o)).abs()
}

impl SketchEdit<'_> {
    /// Auto Constrain (see the module documentation): adds the relations
    /// the geometry has, then the dimensions left, as `options` asks.
    pub fn auto_constrain(
        &mut self,
        options: &AutoConstrainOptions,
    ) -> Result<AutoConstrained, String> {
        if !(options.length_tolerance.is_finite() && options.length_tolerance > 0.0) {
            return Err("the length tolerance must be greater than zero".to_owned());
        }
        if !(options.angle_tolerance.is_finite()
            && options.angle_tolerance > 0.0
            && options.angle_tolerance < std::f64::consts::FRAC_PI_4)
        {
            return Err(
                "the angle tolerance must be greater than zero and less than 45 degrees".to_owned(),
            );
        }
        let current = self.solved()?;
        let scope = match &options.scope {
            Some(refs) => Some(self.scope_of(refs)?),
            None => None,
        };
        let mut run = Run {
            tol: options.length_tolerance,
            sin: options.angle_tolerance.sin(),
            scope,
            original_points: current.points.clone(),
            original_radii: current.radii.clone(),
            current,
            made: AutoConstrained::default(),
            made_axes: Vec::new(),
            placed: Vec::new(),
            gap: 1.0,
            rows: [0, 0],
        };
        run.current.store(&mut self.def.entities);
        if options.relations {
            self.join_ends(&mut run)?;
            self.midpoints(&mut run)?;
            self.points_on_curves(&mut run)?;
            self.tangent_joins(&mut run)?;
            self.equal_sizes(&mut run, false)?;
            self.level_lines(&mut run)?;
            self.collinear_lines(&mut run)?;
            self.square_corners(&mut run)?;
            self.parallel_lines(&mut run)?;
            self.equal_sizes(&mut run, true)?;
            self.concentric_curves(&mut run)?;
            self.symmetric_points(&mut run)?;
            self.origin_and_axes(&mut run)?;
        }
        if options.dimensions {
            let view = self.view(&run);
            let diagonal = dist(view.low, view.high);
            run.gap = if diagonal > 0.0 {
                (diagonal * 0.06).max(0.5)
            } else {
                5.0
            };
            run.placed = self.def.dimensions.iter().filter_map(|d| d.text).collect();
            self.sizes(&mut run)?;
            self.corner_angles(&mut run)?;
            self.positions(&mut run)?;
        }
        self.drop_unused_axes(&run);
        run.made.dof = run.current.status.dof;
        Ok(run.made)
    }

    /// The entities `refs` stand for: the curves with their points, the
    /// points.
    fn scope_of(&self, refs: &[Ref]) -> Result<BTreeSet<EntityUid>, String> {
        let mut scope = BTreeSet::new();
        for r in refs {
            let Some(e) = self.def.entity(r.uid()) else {
                return Err(crate::said(
                    "something selected is no longer in the sketch",
                    format!("{r} does not exist"),
                ));
            };
            scope.insert(e.id);
            scope.extend(e.kind.points());
        }
        Ok(scope)
    }

    /// The origin point and the axes' entities.
    fn axis_entities(&self) -> BTreeSet<EntityUid> {
        let mut out: BTreeSet<EntityUid> = self
            .def
            .entities
            .iter()
            .filter(|e| e.axis.is_some())
            .map(|e| e.id)
            .collect();
        out.extend(origin_point(&self.def));
        out
    }

    /// The sketch in scope as solved now.
    fn view(&self, run: &Run) -> View {
        let axes = self.axis_entities();
        let solved = &run.current;
        let wanted = |id: EntityUid| {
            !axes.contains(&id) && run.scope.as_ref().is_none_or(|s| s.contains(&id))
        };
        let at = |id: EntityUid| solved.points.get(&id).copied();
        let mut view = View {
            low: [f64::INFINITY; 2],
            high: [f64::NEG_INFINITY; 2],
            ..View::default()
        };
        let mut used: BTreeMap<EntityUid, (Role, Vec<EntityUid>)> = BTreeMap::new();
        for e in &self.def.entities {
            if e.is_point() || !wanted(e.id) {
                continue;
            }
            if let Some((s, t)) = e.kind.ends() {
                for p in [s, t] {
                    let entry = used.entry(p).or_insert((Role::End, Vec::new()));
                    entry.0 = Role::End;
                    entry.1.push(e.id);
                }
            }
            match &e.kind {
                EntityKind::Line {
                    start,
                    end,
                    centerline,
                } => {
                    if let (Some(pa), Some(pb)) = (at(*start), at(*end)) {
                        view.lines.push(LineView {
                            id: e.id,
                            a: *start,
                            b: *end,
                            pa,
                            pb,
                            construction: e.construction,
                            mirror: e.construction || *centerline,
                        });
                    }
                }
                EntityKind::Circle { center, .. } | EntityKind::Arc { center, .. } => {
                    used.entry(*center).or_insert((Role::Center, Vec::new()));
                    let ends = match &e.kind {
                        EntityKind::Arc { start, end, .. } => match (at(*start), at(*end)) {
                            (Some(s), Some(t)) => Some([(*start, s), (*end, t)]),
                            _ => None,
                        },
                        _ => None,
                    };
                    let radius = match solved.curves.get(&e.id) {
                        Some(Curve2::Circle { radius, .. } | Curve2::Arc { radius, .. }) => {
                            Some(*radius)
                        }
                        _ => None,
                    };
                    if let (Some(c), Some(radius)) = (at(*center), radius) {
                        view.rounds.push(RoundView {
                            id: e.id,
                            center: *center,
                            c,
                            radius,
                            ends,
                        });
                    }
                }
                EntityKind::Ellipse { center, .. } | EntityKind::EllipticalArc { center, .. } => {
                    used.entry(*center).or_insert((Role::Center, Vec::new()));
                    view.ellipses.push(e.id);
                }
                _ => {}
            }
            if let Some(curve) = solved.curves.get(&e.id) {
                let (low, high) = curve.bounds();
                view.cover(low, high);
                // A conic's apex is off the curve, as a spline's control
                // points are (mitcad#2236).
                let spline = matches!(
                    e.kind,
                    EntityKind::Spline { .. }
                        | EntityKind::FittedSpline { .. }
                        | EntityKind::Conic { .. }
                );
                if !spline {
                    view.curves.push(CurveView {
                        id: e.id,
                        curve: curve.clone(),
                        points: e.kind.points(),
                        ends: curve.ends().map(|(s, t)| vec![s, t]).unwrap_or_default(),
                    });
                }
            }
        }
        let on_curves: BTreeSet<EntityUid> = self
            .def
            .entities
            .iter()
            .flat_map(|e| e.kind.points())
            .collect();
        for e in &self.def.entities {
            if !e.is_point() || !wanted(e.id) {
                continue;
            }
            let Some(p) = at(e.id) else {
                continue;
            };
            view.points.push((e.id, p));
            view.cover(p, p);
            let spot = match used.remove(&e.id) {
                Some((role, ends_of)) => Some((role, ends_of)),
                None if !on_curves.contains(&e.id) => Some((Role::Lone, Vec::new())),
                None => None,
            };
            if let Some((role, ends_of)) = spot {
                view.spots.push(Spot {
                    id: e.id,
                    at: p,
                    role,
                    ends_of,
                });
            }
        }
        if view.low[0] > view.high[0] {
            view.low = [0.0; 2];
            view.high = [0.0; 2];
        }
        view
    }

    /// Solves `def` with the document's values, a trial dimension's value
    /// as `value`.
    fn trial(&self, def: &SketchDef, value: Option<f64>) -> Result<Solved, SolveError> {
        let params = &*self.params;
        def.solve_with(
            &mut |id| {
                if id == TRIAL_VALUE {
                    return value.ok_or_else(|| "no trial value".to_owned());
                }
                params
                    .value(id)
                    .ok_or_else(|| format!("parameter {} does not exist", params.name(id)))
            },
            &|id| {
                if id == TRIAL_VALUE {
                    "the new dimension".to_owned()
                } else {
                    params.name(id)
                }
            },
        )
    }

    /// Whether every entity a candidate refers to is still there (joins
    /// remove points).
    fn exists(&self, refs: &[Ref]) -> bool {
        refs.iter().all(|r| self.def.entity(r.uid()).is_some())
    }

    /// Adds the constraint `kind` when it holds (see the module
    /// documentation); whether it was added.
    fn try_constraint(&mut self, run: &mut Run, kind: ConstraintKind) -> Result<bool, String> {
        let refs = kind.refs();
        let axes = self.axis_entities();
        if run.current.status.dof == 0
            || !self.exists(&refs)
            || !run.in_scope(&refs, &axes)
            || run.settled(&refs, &axes)
        {
            return Ok(false);
        }
        let mut def = self.def.clone();
        def.constraints.push(Constraint::new(TRIAL, kind.clone()));
        match self.trial(&def, None) {
            Ok(after) if run.holds(&after) => {}
            _ => return Ok(false),
        }
        let type_name = kind.type_name();
        // Two curve ends at one place become one point (mitcad#260).
        let merge = self.coincident_merge(&kind);
        let id = self.add_constraint(kind);
        match merge {
            Some((keep, drop)) => {
                self.merge_coincident(id, keep, drop)?;
                run.made.joined += 1;
            }
            None => run.made.constraints.push(id),
        }
        *run.made.added.entry(type_name).or_default() += 1;
        run.current = self.solved()?;
        run.current.store(&mut self.def.entities);
        Ok(true)
    }

    /// Adds a driving dimension `kind` with the value it measures when it
    /// holds, its value shown at `text` (moved on along `away` past those
    /// placed before); whether it was added.
    fn try_dimension(
        &mut self,
        run: &mut Run,
        kind: DimensionKind,
        text: P2,
        away: P2,
    ) -> Result<bool, String> {
        let refs = kind.refs();
        let axes = self.axis_entities();
        if run.current.status.dof == 0
            || !self.exists(&refs)
            || !run.in_scope(&refs, &axes)
            || run.settled(&refs, &axes)
        {
            return Ok(false);
        }
        let Some(value) =
            measure_in(&kind, &run.current, &self.def.constraints).filter(|v| v.is_finite())
        else {
            return Ok(false);
        };
        let mut def = self.def.clone();
        def.dimensions
            .push(Dimension::new(TRIAL, kind.clone(), Some(TRIAL_VALUE)));
        match self.trial(&def, Some(value)) {
            Ok(after) if run.holds(&after) => {}
            _ => return Ok(false),
        }
        let type_name = kind.type_name().to_owned();
        let id = self.add_dimension(kind, None, false, None)?;
        let at = run.place(text, away);
        self.set_text_position(id, at)?;
        run.made.dimensions.push(id);
        *run.made.added.entry(type_name).or_default() += 1;
        run.current = self.solved()?;
        run.current.store(&mut self.def.entities);
        Ok(true)
    }

    /// The origin point, made (and remembered, to be taken away again when
    /// nothing comes to use it) when the sketch has none.
    fn run_origin(&mut self, run: &mut Run) -> EntityUid {
        if let Some(id) = origin_point(&self.def) {
            return id;
        }
        let id = self.origin();
        run.made_axes.push(id);
        self.solve_made(run);
        id
    }

    /// The current solve again after the origin or an axis was made: their
    /// fixed points take no freedom, but a dimension measures from them.
    fn solve_made(&mut self, run: &mut Run) {
        if let Ok(solved) = self.solved() {
            run.current = solved;
        }
    }

    /// The line of a sketch axis, made as [`Self::run_origin`] makes the
    /// origin.
    fn run_axis(&mut self, run: &mut Run, axis: SketchAxis) -> EntityUid {
        if let Some(id) = axis_line(&self.def, axis) {
            return id;
        }
        self.run_origin(run);
        let before = self.def.entities.len();
        let id = self.axis(axis);
        run.made_axes
            .extend(self.def.entities[before..].iter().map(|e| e.id));
        self.solve_made(run);
        id
    }

    // Phase 1: relations.

    /// Curve ends (and lone points, a centre with an end) at one place.
    fn join_ends(&mut self, run: &mut Run) -> Result<(), String> {
        let view = self.view(run);
        let spots = &view.spots;
        for i in 0..spots.len() {
            for j in i + 1..spots.len() {
                let (a, b) = (&spots[i], &spots[j]);
                if (a.role == Role::Center && b.role == Role::Center)
                    || a.ends_of.iter().any(|c| b.ends_of.contains(c))
                    || dist(a.at, b.at) > run.tol
                {
                    continue;
                }
                self.try_constraint(
                    run,
                    ConstraintKind::Coincident {
                        point: b.id,
                        entity: Ref::Point(a.id),
                    },
                )?;
            }
        }
        Ok(())
    }

    /// Points at the middle of a line.
    fn midpoints(&mut self, run: &mut Run) -> Result<(), String> {
        let view = self.view(run);
        for line in &view.lines {
            if line.length() <= run.tol {
                continue;
            }
            let middle = scale(add(line.pa, line.pb), 0.5);
            for spot in &view.spots {
                if spot.id == line.a || spot.id == line.b || dist(spot.at, middle) > run.tol {
                    continue;
                }
                self.try_constraint(
                    run,
                    ConstraintKind::Midpoint {
                        point: spot.id,
                        curve: line.id,
                    },
                )?;
            }
        }
        Ok(())
    }

    /// Points on a curve, away from its ends.
    fn points_on_curves(&mut self, run: &mut Run) -> Result<(), String> {
        let view = self.view(run);
        for spot in &view.spots {
            for curve in &view.curves {
                if curve.points.contains(&spot.id)
                    || curve.ends.iter().any(|e| dist(*e, spot.at) <= run.tol)
                    || curve.curve.closest(spot.at).1 > run.tol
                {
                    continue;
                }
                self.try_constraint(
                    run,
                    ConstraintKind::Coincident {
                        point: spot.id,
                        entity: Ref::Curve(curve.id),
                    },
                )?;
            }
        }
        Ok(())
    }

    /// The direction of a line or an arc at `p`, one of its ends.
    fn tangent_at(view: &View, curve: EntityUid, p: EntityUid) -> Option<P2> {
        if let Some(line) = view.line(curve) {
            return Some(line.leaving(p));
        }
        let round = view.rounds.iter().find(|r| r.id == curve)?;
        let (_, at) = round.ends?.into_iter().find(|(id, _)| *id == p)?;
        Some(perp(unit(sub(at, round.c))))
    }

    /// Lines and arcs that join smoothly.
    fn tangent_joins(&mut self, run: &mut Run) -> Result<(), String> {
        let view = self.view(run);
        for spot in &view.spots {
            let curves: Vec<EntityUid> = spot
                .ends_of
                .iter()
                .copied()
                .filter(|c| {
                    view.line(*c).is_some()
                        || view.rounds.iter().any(|r| r.id == *c && r.ends.is_some())
                })
                .collect();
            for i in 0..curves.len() {
                for j in i + 1..curves.len() {
                    let (a, b) = (curves[i], curves[j]);
                    if view.line(a).is_some() && view.line(b).is_some() {
                        continue;
                    }
                    let (Some(ta), Some(tb)) = (
                        Self::tangent_at(&view, a, spot.id),
                        Self::tangent_at(&view, b, spot.id),
                    ) else {
                        continue;
                    };
                    if cross(ta, tb).abs() > run.sin {
                        continue;
                    }
                    self.try_constraint(
                        run,
                        ConstraintKind::Tangent {
                            a,
                            b,
                            at: Some(spot.id),
                        },
                    )?;
                }
            }
        }
        Ok(())
    }

    /// Lines that are level or upright.
    fn level_lines(&mut self, run: &mut Run) -> Result<(), String> {
        let view = self.view(run);
        for line in &view.lines {
            if line.length() <= run.tol {
                continue;
            }
            let d = line.direction();
            if d[1].abs() <= run.sin {
                self.try_constraint(run, ConstraintKind::Horizontal { line: line.id })?;
            } else if d[0].abs() <= run.sin {
                self.try_constraint(run, ConstraintKind::Vertical { line: line.id })?;
            }
        }
        Ok(())
    }

    /// Whether a line's direction is held level or upright.
    fn direction_held(&self, line: EntityUid) -> bool {
        self.def.constraints.iter().any(|c| {
            matches!(c.kind, ConstraintKind::Horizontal { line: l } | ConstraintKind::Vertical { line: l } if l == line)
        })
    }

    /// Lines on one infinite line: collinear, or, where both directions are
    /// held already, an end of one on the other.
    fn collinear_lines(&mut self, run: &mut Run) -> Result<(), String> {
        let view = self.view(run);
        let lines = &view.lines;
        for j in 0..lines.len() {
            let b = &lines[j];
            if b.length() <= run.tol {
                continue;
            }
            for a in &lines[..j] {
                if a.length() <= run.tol {
                    continue;
                }
                let u = a.direction();
                if cross(u, b.direction()).abs() > run.sin
                    || off_line(b.pa, a.pa, u) > run.tol
                    || off_line(b.pb, a.pa, u) > run.tol
                {
                    continue;
                }
                if self.try_constraint(run, ConstraintKind::Collinear { a: a.id, b: b.id })? {
                    break;
                }
                let mut held = false;
                for end in [b.a, b.b] {
                    if end == a.a || end == a.b {
                        continue;
                    }
                    held |= self.try_constraint(
                        run,
                        ConstraintKind::Coincident {
                            point: end,
                            entity: Ref::Curve(a.id),
                        },
                    )?;
                }
                if held {
                    break;
                }
            }
        }
        Ok(())
    }

    /// Lines square to each other at a corner they share.
    fn square_corners(&mut self, run: &mut Run) -> Result<(), String> {
        let view = self.view(run);
        for spot in &view.spots {
            let lines: Vec<&LineView> = spot
                .ends_of
                .iter()
                .filter_map(|c| view.line(*c))
                .filter(|l| l.length() > run.tol)
                .collect();
            for i in 0..lines.len() {
                for j in i + 1..lines.len() {
                    let (a, b) = (lines[i], lines[j]);
                    if (self.direction_held(a.id) && self.direction_held(b.id))
                        || dot(a.direction(), b.direction()).abs() > run.sin
                    {
                        continue;
                    }
                    self.try_constraint(run, ConstraintKind::Perpendicular { a: a.id, b: b.id })?;
                }
            }
        }
        Ok(())
    }

    /// Parallel lines, each with an earlier one.
    fn parallel_lines(&mut self, run: &mut Run) -> Result<(), String> {
        let view = self.view(run);
        let lines = &view.lines;
        for j in 0..lines.len() {
            let b = &lines[j];
            if b.length() <= run.tol {
                continue;
            }
            let mut tried = 0;
            for a in &lines[..j] {
                if tried >= PARTNERS {
                    break;
                }
                if a.length() <= run.tol
                    || (self.direction_held(a.id) && self.direction_held(b.id))
                    || cross(a.direction(), b.direction()).abs() > run.sin
                {
                    continue;
                }
                tried += 1;
                if self.try_constraint(run, ConstraintKind::Parallel { a: a.id, b: b.id })? {
                    break;
                }
            }
        }
        Ok(())
    }

    /// Equal radii of circles and arcs (`lines` false) or equal lengths of
    /// lines (`lines` true), each with an earlier one. The radii come
    /// before the directions of lines: a slot's two lines held level or
    /// parallel would imply its arcs' equal radii.
    fn equal_sizes(&mut self, run: &mut Run, lines: bool) -> Result<(), String> {
        let view = self.view(run);
        let sizes: Vec<(EntityUid, f64)> = if lines {
            view.lines
                .iter()
                .filter(|l| !l.construction)
                .map(|l| (l.id, l.length()))
                .collect()
        } else {
            view.rounds.iter().map(|r| (r.id, r.radius)).collect()
        };
        for j in 0..sizes.len() {
            let (b, size) = sizes[j];
            if size <= run.tol {
                continue;
            }
            let mut tried = 0;
            for &(a, other) in &sizes[..j] {
                if tried >= PARTNERS {
                    break;
                }
                if (size - other).abs() > run.tol {
                    continue;
                }
                tried += 1;
                if self.try_constraint(run, ConstraintKind::Equal { a, b })? {
                    break;
                }
            }
        }
        Ok(())
    }

    /// Circles and arcs around one centre.
    fn concentric_curves(&mut self, run: &mut Run) -> Result<(), String> {
        let view = self.view(run);
        let rounds = &view.rounds;
        for j in 0..rounds.len() {
            let b = &rounds[j];
            let mut tried = 0;
            for a in &rounds[..j] {
                if tried >= PARTNERS {
                    break;
                }
                if a.center == b.center || dist(a.c, b.c) > run.tol {
                    continue;
                }
                tried += 1;
                if self.try_constraint(run, ConstraintKind::Concentric { a: a.id, b: b.id })? {
                    break;
                }
            }
        }
        Ok(())
    }

    /// Pairs of points mirrored about a construction line, a centre line or
    /// a sketch axis.
    fn symmetric_points(&mut self, run: &mut Run) -> Result<(), String> {
        let view = self.view(run);
        // (the line or the axis, a point on it, its direction, its points)
        let mut mirrors: Vec<Mirror> = view
            .lines
            .iter()
            .filter(|l| l.mirror && l.length() > run.tol)
            .map(|l| (Ok(l.id), l.pa, l.direction(), vec![l.a, l.b]))
            .collect();
        for axis in [SketchAxis::Y, SketchAxis::X] {
            mirrors.push((Err(axis), [0.0, 0.0], axis.direction(), Vec::new()));
        }
        for (mirror, o, u, own) in mirrors {
            let mut pairs = Vec::new();
            for (i, p) in view.spots.iter().enumerate() {
                if own.contains(&p.id) || off_line(p.at, o, u) <= run.tol {
                    continue;
                }
                let image = mirrored(p.at, o, u);
                for q in &view.spots[i + 1..] {
                    if !own.contains(&q.id) && dist(q.at, image) <= run.tol {
                        pairs.push((p.id, q.id));
                    }
                }
            }
            if pairs.is_empty() {
                continue;
            }
            let axis = match mirror {
                Ok(line) => line,
                Err(axis) => self.run_axis(run, axis),
            };
            for (p, q) in pairs {
                self.try_constraint(
                    run,
                    ConstraintKind::Symmetric {
                        a: Ref::Point(p),
                        b: Ref::Point(q),
                        axis,
                        radii: true,
                    },
                )?;
            }
        }
        Ok(())
    }

    /// Points at the origin and on the sketch axes.
    fn origin_and_axes(&mut self, run: &mut Run) -> Result<(), String> {
        let view = self.view(run);
        for spot in &view.spots {
            let [x, y] = spot.at;
            if norm(spot.at) <= run.tol {
                let origin = self.run_origin(run);
                self.try_constraint(
                    run,
                    ConstraintKind::Coincident {
                        point: spot.id,
                        entity: Ref::Point(origin),
                    },
                )?;
                continue;
            }
            for (near, axis) in [(y.abs(), SketchAxis::X), (x.abs(), SketchAxis::Y)] {
                if near > run.tol {
                    continue;
                }
                let line = self.run_axis(run, axis);
                self.try_constraint(
                    run,
                    ConstraintKind::Coincident {
                        point: spot.id,
                        entity: Ref::Curve(line),
                    },
                )?;
            }
        }
        Ok(())
    }

    // Phase 2: dimensions.

    /// The sizes: line lengths, radii of arcs, diameters of circles, the
    /// radii of ellipses.
    fn sizes(&mut self, run: &mut Run) -> Result<(), String> {
        let view = self.view(run);
        let center = view.center();
        let mut lines: Vec<&LineView> = view.lines.iter().collect();
        // Profile lines before construction lines.
        lines.sort_by_key(|l| l.construction);
        for line in lines {
            if line.length() <= run.tol {
                continue;
            }
            let middle = scale(add(line.pa, line.pb), 0.5);
            let mut out = perp(line.direction());
            if dot(out, sub(middle, center)) < 0.0 {
                out = scale(out, -1.0);
            }
            let text = add(middle, scale(out, run.gap));
            self.try_dimension(run, DimensionKind::Length { line: line.id }, text, out)?;
        }
        for round in &view.rounds {
            let (kind, out) = match round.ends {
                Some([(_, s), (_, t)]) => {
                    // Toward the middle of the arc.
                    let mid = add(unit(sub(s, round.c)), unit(sub(t, round.c)));
                    let mut out = unit(mid);
                    let curve = view.curves.iter().find(|c| c.id == round.id);
                    if let Some(curve) = curve {
                        let (t0, t1) = curve.curve.domain();
                        out = unit(sub(curve.curve.point(0.5 * (t0 + t1)), round.c));
                    }
                    (DimensionKind::Radius { curve: round.id }, out)
                }
                None => (
                    DimensionKind::Diameter { curve: round.id },
                    unit([1.0, 1.0]),
                ),
            };
            let text = add(round.c, scale(out, round.radius + run.gap));
            self.try_dimension(run, kind, text, out)?;
        }
        for ellipse in &view.ellipses {
            let Some(
                Curve2::Ellipse { center, major, .. } | Curve2::EllipticalArc { center, major, .. },
            ) = run.current.curves.get(ellipse).cloned()
            else {
                continue;
            };
            let out = unit([1.0, 1.0]);
            let text = add(center, scale(out, major + run.gap));
            self.try_dimension(
                run,
                DimensionKind::MajorRadius { ellipse: *ellipse },
                text,
                out,
            )?;
            let text = add(center, scale(out, major + 2.0 * run.gap));
            self.try_dimension(
                run,
                DimensionKind::MinorRadius { ellipse: *ellipse },
                text,
                out,
            )?;
        }
        Ok(())
    }

    /// The angle at a corner of two lines that are neither square nor
    /// parallel.
    fn corner_angles(&mut self, run: &mut Run) -> Result<(), String> {
        let view = self.view(run);
        for spot in &view.spots {
            let lines: Vec<&LineView> = spot
                .ends_of
                .iter()
                .filter_map(|c| view.line(*c))
                .filter(|l| l.length() > run.tol)
                .collect();
            for i in 0..lines.len() {
                for j in i + 1..lines.len() {
                    let (a, b) = (lines[i], lines[j]);
                    let (ra, rb) = (a.leaving(spot.id), b.leaving(spot.id));
                    if cross(ra, rb).abs() <= run.sin || dot(ra, rb).abs() <= run.sin {
                        continue;
                    }
                    let kind = DimensionKind::Angle {
                        a: a.id,
                        b: b.id,
                        reverse_a: Some(a.b == spot.id),
                        reverse_b: Some(b.b == spot.id),
                    };
                    let bisector = unit(add(ra, rb));
                    let text = add(spot.at, scale(bisector, run.gap * 1.5));
                    self.try_dimension(run, kind, text, bisector)?;
                }
            }
        }
        Ok(())
    }

    /// The connected parts of the view: groups of points the curves join,
    /// each with its lines.
    fn parts(&self, view: &View) -> Vec<Part> {
        let ids: Vec<EntityUid> = view.points.iter().map(|(p, _)| *p).collect();
        let index: BTreeMap<EntityUid, usize> =
            ids.iter().enumerate().map(|(i, p)| (*p, i)).collect();
        let mut parent: Vec<usize> = (0..ids.len()).collect();
        fn root(parent: &mut [usize], mut i: usize) -> usize {
            while parent[i] != i {
                parent[i] = parent[parent[i]];
                i = parent[i];
            }
            i
        }
        let mut link = |a: EntityUid, b: EntityUid| {
            if let (Some(&i), Some(&j)) = (index.get(&a), index.get(&b)) {
                let (ri, rj) = (root(&mut parent, i), root(&mut parent, j));
                parent[ri] = rj;
            }
        };
        for e in &self.def.entities {
            let points = e.kind.points();
            for pair in points.windows(2) {
                link(pair[0], pair[1]);
            }
        }
        for c in &self.def.constraints {
            let points: Vec<EntityUid> = c
                .kind
                .refs()
                .into_iter()
                .filter_map(|r| match r {
                    Ref::Point(p) => Some(p),
                    Ref::Curve(c) => self
                        .def
                        .entity(c)
                        .and_then(|e| e.kind.points().first().copied()),
                })
                .collect();
            for pair in points.windows(2) {
                link(pair[0], pair[1]);
            }
        }
        let mut groups: BTreeMap<usize, Part> = BTreeMap::new();
        for (i, (p, at)) in view.points.iter().enumerate() {
            let r = root(&mut parent, i);
            groups.entry(r).or_default().0.push((*p, *at));
        }
        for line in &view.lines {
            if let Some(&i) = index.get(&line.a) {
                let r = root(&mut parent, i);
                groups.entry(r).or_default().1.push(line.id);
            }
        }
        let mut out: Vec<_> = groups.into_values().collect();
        let nearest = |points: &[(EntityUid, P2)]| {
            points
                .iter()
                .map(|(_, at)| norm(*at))
                .fold(f64::INFINITY, f64::min)
        };
        out.sort_by(|a, b| nearest(&a.0).total_cmp(&nearest(&b.0)));
        out
    }

    /// The horizontal and vertical distance of `p` from the origin, as far
    /// as they take freedom away.
    fn try_position(
        &mut self,
        run: &mut Run,
        view: &View,
        p: EntityUid,
        at: P2,
    ) -> Result<(), String> {
        if run.current.status.fully_constrained.contains(&p) {
            return Ok(());
        }
        let origin = self.run_origin(run);
        let low = [view.low[0].min(0.0), view.low[1].min(0.0)];
        // Below the geometry, one row each, the vertical ones left of it.
        let text = [0.5 * at[0], low[1] - run.gap * (1.0 + run.rows[0] as f64)];
        let kind = DimensionKind::HorizontalDistance {
            a: origin,
            b: p,
            signed: at[0].abs() <= run.tol,
        };
        if self.try_dimension(run, kind, text, [0.0, -1.0])? {
            run.rows[0] += 1;
        }
        let text = [low[0] - run.gap * (1.0 + run.rows[1] as f64), 0.5 * at[1]];
        let kind = DimensionKind::VerticalDistance {
            a: origin,
            b: p,
            signed: at[1].abs() <= run.tol,
        };
        if self.try_dimension(run, kind, text, [-1.0, 0.0])? {
            run.rows[1] += 1;
        }
        Ok(())
    }

    /// The positions from the origin: of each connected part its point
    /// nearest to the origin, the angle of one of its lines to the x axis,
    /// then any point of it still free.
    fn positions(&mut self, run: &mut Run) -> Result<(), String> {
        let view = self.view(run);
        let spots: BTreeSet<EntityUid> = view.spots.iter().map(|s| s.id).collect();
        for (mut points, lines) in self.parts(&view) {
            if run.current.status.dof == 0 {
                break;
            }
            // The points a relation can hold first, the nearest first.
            points.sort_by(|(a, pa), (b, pb)| {
                (!spots.contains(a), norm(*pa))
                    .partial_cmp(&(!spots.contains(b), norm(*pb)))
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            if let Some(&(p, at)) = points.first() {
                self.try_position(run, &view, p, at)?;
            }
            let mut lines: Vec<&LineView> = lines
                .iter()
                .filter_map(|l| view.line(*l))
                .filter(|l| l.length() > run.tol && !self.direction_held(l.id))
                .collect();
            lines.sort_by(|a, b| b.length().total_cmp(&a.length()));
            for line in lines {
                if run.current.status.fully_constrained.contains(&line.id) {
                    continue;
                }
                let d = line.direction();
                // Level or upright lines are a relation's, when it is asked for.
                if d[0].abs() <= run.sin || d[1].abs() <= run.sin {
                    continue;
                }
                let axis = self.run_axis(run, SketchAxis::X);
                // The ray of the line that leans right.
                let reverse = d[0] < 0.0;
                let kind = DimensionKind::Angle {
                    a: axis,
                    b: line.id,
                    reverse_a: Some(false),
                    reverse_b: Some(reverse),
                };
                let ray = if reverse { scale(d, -1.0) } else { d };
                let start = if reverse { line.pb } else { line.pa };
                let text = add(start, scale(unit(add(ray, [1.0, 0.0])), run.gap * 1.5));
                if self.try_dimension(run, kind, text, [0.0, 1.0])? {
                    break;
                }
            }
            for &(p, at) in &points {
                self.try_position(run, &view, p, at)?;
            }
        }
        Ok(())
    }

    /// Takes away the origin point and axes this run made that nothing
    /// came to use.
    fn drop_unused_axes(&mut self, run: &Run) {
        if run.made_axes.is_empty() {
            return;
        }
        let made: BTreeSet<EntityUid> = run.made_axes.iter().copied().collect();
        let mut used: BTreeSet<EntityUid> = self
            .def
            .constraints
            .iter()
            .flat_map(|c| c.kind.refs())
            .chain(self.def.dimensions.iter().flat_map(|d| d.kind.refs()))
            .map(|r| r.uid())
            .collect();
        // An axis line in use keeps its points.
        for e in &self.def.entities {
            if made.contains(&e.id) && used.contains(&e.id) {
                used.extend(e.kind.points());
            }
        }
        // A point of an axis that was there keeps the origin.
        for e in &self.def.entities {
            if !made.contains(&e.id) {
                used.extend(e.kind.points().into_iter().filter(|p| made.contains(p)));
            }
        }
        let unused: BTreeSet<EntityUid> = made.difference(&used).copied().collect();
        if unused.is_empty() {
            return;
        }
        self.def.entities.retain(|e| !unused.contains(&e.id));
        self.report.entities.retain(|r| !unused.contains(&r.uid()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_summary_says_what_was_added_and_what_is_left() {
        let mut made = AutoConstrained {
            constraints: vec![ConstraintUid(1), ConstraintUid(2)],
            dimensions: vec![ConstraintUid(3)],
            joined: 4,
            ..AutoConstrained::default()
        };
        assert_eq!(
            made.summary(),
            "6 constraints, 1 dimension; the sketch is fully constrained"
        );
        made.dof = 2;
        assert_eq!(
            made.summary(),
            "6 constraints, 1 dimension; 2 degrees of freedom left"
        );
        made.dof = 1;
        made.dimensions.clear();
        assert!(
            made.summary()
                .ends_with("0 dimensions; 1 degree of freedom left")
        );
    }

    #[test]
    fn a_point_mirrored_about_a_line_lands_across_it() {
        let image = mirrored([3.0, 1.0], [0.0, 0.0], [0.0, 1.0]);
        assert!(dist(image, [-3.0, 1.0]) < 1e-12);
        assert!((off_line([3.0, 1.0], [0.0, 0.0], [0.0, 1.0]) - 3.0).abs() < 1e-12);
    }
}
