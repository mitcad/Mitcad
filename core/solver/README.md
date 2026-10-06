# mitcad-solver

2D sketch constraint solver for Mitcad, without dependencies: residual
equations, Newton / Levenberg-Marquardt with minimum-norm steps, LDL^T
factorization of the Gram matrix in a minimum degree ordering, rank
detection by pivot thresholds, forward-mode automatic differentiation.
An independent MIT implementation of published methods; no code from
SolveSpace (GPLv3) or FreeCAD PlaneGCS (LGPL).

The sketch model keeps its own stable ids (`EntityUid`, see
`docs/architecture.md`) and maps them to solver ids.

## Usage

```rust
use mitcad_solver::{Analysis, Constraint::*, DragGoal, SolveOptions, System};

let mut s = System::new();
let p0 = s.add_point(0.0, 0.0);
let p1 = s.add_point(9.0, 0.4);          // imperfect geometry is fine
let line = s.add_line(p0, p1)?;
s.add_constraint(FixPoint(p0))?;
s.add_constraint(Horizontal(line))?;
let len = s.add_constraint(Length { line, value: 10.0 })?;

let r = s.solve(&SolveOptions::default()); // r.status, r.iterations, r.residual
assert!(r.is_ok());
let [x, y] = s.point(p1).unwrap();         // (10, 0)

let a: Analysis = s.analyze();             // a.dof == 0, p1 fully constrained
s.set_dimension_value(len, 25.0)?;         // incremental re-solve
s.solve(&SolveOptions::default());

// While the user drags: constraints hard, the pointer a soft goal.
s.drag(&[DragGoal::Point { point: p1, target: [30.0, 5.0] }], &SolveOptions::default());
```

`SolveResult::conflicts` and `Analysis::{redundant, conflicts}` list
`Dependency { constraint, involved }`: `constraint` is the most recently added
constraint of a dependent set (the one an interactive sketcher would refuse),
`involved` the whole set.

## Entities and unknowns

All values are in sketch plane coordinates (mm, radians). Points are shared
between entities: a shared point is a join; an explicit `Coincident` works too.

| Entity | Defined by | DOF |
|---|---|---|
| Point | x, y | 2 |
| Line | two points | 4 |
| Circle | centre point, radius | 3 |
| Arc | centre, start, end (CCW start -> end); implicit `|end-c| = |start-c|` | 5 |
| Ellipse | centre, major-axis end point, minor radius | 5 |
| Elliptical arc | ellipse + start/end points, each tied to the ellipse by an internal parameter | 7 |
| B-spline | control points; degree, knots, weights fixed (default: clamped uniform, non-rational) | 2 per control point |
| Fitted spline | fit points; cubic, internal control points, interpolation at fixed chord-length parameters; natural or free ends | 2 per fit point (+2 per free end) |

Internal unknowns (curve parameters of point-on-curve and spline tangency,
hidden control points of fitted splines) are balanced by their equations, so
they do not change the DOF count.

## Constraints and dimensions

| Constraint | Equations (residual = 0) |
|---|---|
| `Coincident(p, q)` | `p - q` (2) |
| `PointOnCurve(p, e)` | line: signed distance; circle/arc: `|p-c| - r`; ellipse, elliptical arc, spline: `p - C(t)` with internal `t` (2) |
| `Horizontal(l)`, `Vertical(l)`, `HorizontalPoints`, `VerticalPoints` | `dy`, `dx` |
| `Parallel(a, b)`, `Perpendicular(a, b)` | `unit(a) x b`, `unit(a) . b` |
| `Tangent(a, b)` | line-circle/arc: signed centre distance `= side * r`; circle/arc-circle/arc: `|c1-c2| = r1 + r2` or `side (r1 - r2)`; at a shared/coincident/point-on-curve contact point: line perpendicular to the radius, or centres collinear with the point (G1 join); line, arc or spline at a spline end: end leg parallel/perpendicular; line or circle/arc inside a spline: contact point `C(t)` on the curve and `C'(t)` tangent (2, internal `t`) |
| `Smooth(a, b)` (G2) | spline end joined to a line, arc or spline: G1 plus equal signed curvature |
| `Equal(a, b)` | lines: equal length; circles/arcs: equal radius |
| `FixPoint`, `FixEntity` | unknowns stay at their value when the solve starts (as many equations as the entity has DOF) |
| `Midpoint(p, e)` | line: `2p - a - b` (2); arc: `p = c + r unit(rot_cw(e - s))` (2) |
| `Concentric(a, b)` | centres coincide (2) |
| `Collinear(a, b)` | both end points of `b` on line `a` (shared points skipped) |
| `SymmetricPoints`, `SymmetricEntities` | midpoint on the axis and segment perpendicular to it (2 per point pair); circles: centres + equal radius; arcs: centres, start <-> end, mirrored end direction |
| `Translated`, `Rotated` (pattern copies) | `b - a - by`, `b - c - R(angle)(a - c)` (2) |
| `TurnedDirection` (arc ends of copies) | `unit(b - cb) x R(angle)(a - ca)` (1) |
| `EqualSize` (circle and ellipse copies) | equal radius / equal minor radius |
| `Distance`, `Length`, `MajorRadius` | `|q - p| - d` |
| `PointLineDistance`, `LineDistance` | signed distance `- side * d` |
| `HorizontalDistance`, `VerticalDistance` | `dx - side * d`, `dy - side * d` |
| `Angle(a, b)` | oriented angle from `a` to `flip * b` minus `sigma * value` (wrapped) |
| `Radius`, `Diameter`, `MinorRadius`, `ArcLength` | `r - d`, `2r - d`, `b - d`, `r * sweep - d` |

`side`, `flip` and `sigma` (and internal vs external tangency) are chosen from
the geometry when the constraint is added and stored, so later dimension
changes cannot mirror the geometry.

Residuals are lengths (mm). Direction-only residuals (unit vectors, angles)
are multiplied by the sketch size `scale` (bounding box diagonal, at least
1 mm), so one tolerance `tolerance * scale` (default `1e-9 * scale`) applies
to all equations.

## Solving

1. **Preparation** (cached until points, entities or constraints are added or
   removed): constraints become scalar equations, each over a few unknowns.
   Union-find splits them into independent components. For each component the
   pattern of the Gram matrix `G = J D J^T` and its symbolic LDL^T
   factorization in a minimum degree order are computed once. Dimension
   changes only update equation constants.
2. **Jacobian**: every residual is generic over a `Scalar` trait; evaluating
   it with dual numbers (8 derivative lanes, several passes for more
   unknowns) gives exact derivatives, including through B-spline evaluation
   (Cox-de Boor with first and second derivatives).
3. **Step**: minimum-norm Levenberg-Marquardt step in the weighted metric
   `W = D^-1`, `dx = -D J^T (J D J^T + mu I)^-1 f`
   (equal to `-(J^T J + mu W)^-1 J^T f`). For the usual under-constrained
   sketch this moves the geometry as little as possible; `mu` follows the gain
   ratio (Nielsen's update), so far-off geometry converges and
   rank-deficient (redundant) systems stay well defined. Coordinates and radii
   have weight 1, hidden control points 0.01 (they follow their fit points),
   curve parameters `(0.1 * curve size)^2`.
4. **Continuation**: a dimension that changes by more than about 30 % (or
   pi/8 for angles) is applied in up to 16 steps, each solved from the
   previous one, which keeps the geometry on its branch (no flips). If the
   stepped solve fails, a direct solve is tried.
5. **Failure**: a component that does not converge is analysed at its
   least-squares point (see below) to name the conflicting constraints, and
   then restored to its geometry before the solve. Other components keep
   their solution.
6. **Drag**: goals (point positions, circle radii) are pulled toward their
   targets: each iteration projects the goal step onto the linearized
   constraints (`dx = g - D' J^T (J D' J^T)^-1 (J g + f)`, goals weighted
   10^6 heavier so the rest moves minimally), restores the constraints with
   the LM iteration, and halves the step when it does not reduce the goal
   distance enough (the constraint curvature can make full steps
   oscillate). The result is the constrained point nearest to the target.

## Analysis

- **Rank in creation order.** The Jacobian rows, scaled to unit length, are
  processed in creation order (implicit entity equations first, then
  constraints as added). The LDL^T factorization of their Gram matrix in that
  order has as pivot each row's squared distance from the span of the rows
  before it; a pivot below `1e-10` marks a dependent row (relative distance
  below about `1e-5`). DOF = unknowns - rank.
- **Culprits.** For a dependent row the factor gives the coefficients of the
  combination of earlier rows that reproduces it (`L^T c = l`). The
  constraints with non-zero coefficients form the dependent set. The same
  combination of residuals tells whether the set is consistent (redundant,
  accepted) or contradictory (conflict).
- **Directions.** Horizontal, vertical, parallel, perpendicular, collinear and
  angle constraints are also checked exactly on a graph of line directions
  (angles modulo pi, union-find with offsets). This catches contradictions
  whose polynomial equations still have a degenerate solution (a line that is
  both horizontal and vertical collapses to a point).
- **Fully constrained.** An unknown is determined when its unit vector lies in
  the row space of `J`: the Schur complement `1 - b^T G^-1 b` of its column
  is zero (below `1e-8`). A point is fully constrained when both coordinates
  are, an entity when all its points and radii are (sketch mode shows
  these in black). Components without remaining DOF skip this test.

## Performance

Benchmark (release build):
`cargo test --release -p mitcad-solver --test bench -- --ignored --nocapture`.
Sketches of a few hundred constraints solve and analyse in well under a
millisecond; 2000 lines with 3999 constraints take a few milliseconds.
Re-solving after a dimension change is cheaper than the first solve,
which includes the preparation.

## Limitations

- The analysis is first order at the current geometry: at singular
  configurations (e.g. two tangent circles drawn exactly touching but without
  a tangent constraint) rank decisions use the tolerances above.
- Fitted splines keep the chord-length parameters of their fit points from
  creation; after large edits the caller may recreate the spline. A tangent or
  smooth constraint at a natural end replaces the natural condition, leaving
  the tangent magnitude free (like a tangent handle).
- Point on a line, circle, arc or ellipse means the infinite line or full
  curve; point on a spline stays within the parameter range.
- Ellipses support point-on-curve, concentric, axis dimensions and fix, but
  not tangency or equal.
- Spline tangency at ends and `Smooth` need clamped knot vectors; spline-spline
  tangency needs a shared end point.
- Mid-arc and symmetric-arc equations have a second (far) solution; Newton
  from nearby geometry stays on the near one.
- Dense fill can grow for unusual creation orders in the analysis
  factorization; sketches of a few thousand unknowns are fine.
