// SPDX-License-Identifier: MIT
//! Sketch patterns of many copies (mitcad#677): copies bound to their grid
//! neighbour (a chain) solve, drag and analyse as copies bound to the
//! original (a star) do, and their cost grows about linearly. The timing
//! test is ignored by default; run it in release:
//! `cargo test --release -p mitcad-solver --test planar -- pattern:: --ignored --nocapture`

use crate::common::*;
use mitcad_solver::Constraint::*;
use mitcad_solver::{DragGoal, EntityId, PointId, SolveOptions, System};
use std::time::{Duration, Instant};

/// How the copies are bound.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Bind {
    /// Every copy to the original.
    Star,
    /// Every copy to its neighbour one step toward the original.
    Chain,
}

/// An `n` x `n` rectangular pattern of a circle at the origin, 10 mm
/// apart, as the model binds it: the copies' centres `Translated`, their
/// radii `EqualSize`. The copies start `jitter` off their places.
fn grid(s: &mut System, n: usize, bind: Bind, jitter: f64) -> Vec<(PointId, EntityId)> {
    let mut rng = Rng(677);
    let mut out: Vec<(PointId, EntityId)> = Vec::new();
    for i in 0..n {
        for j in 0..n {
            let (x, y) = (10.0 * i as f64, 10.0 * j as f64);
            let (dx, dy) = if i + j == 0 {
                (0.0, 0.0)
            } else {
                (rng.sym(jitter), rng.sym(jitter))
            };
            let p = s.add_point(x + dx, y + dy);
            let c = s.add_circle(p, 3.0 + rng.sym(jitter)).unwrap();
            out.push((p, c));
        }
    }
    for i in 0..n {
        for j in 0..n {
            if i + j == 0 {
                continue;
            }
            // The model's order (`SketchPattern::neighbour`): toward the
            // original along the second direction, then along the first.
            let (fi, fj) = match bind {
                Bind::Star => (0, 0),
                Bind::Chain if j > 0 => (i, j - 1),
                Bind::Chain => (i - 1, j),
            };
            let (a, ca) = out[fi * n + fj];
            let (b, cb) = out[i * n + j];
            let by = [10.0 * (i - fi) as f64, 10.0 * (j - fj) as f64];
            s.add_constraint(Translated { a, b, by }).unwrap();
            s.add_constraint(EqualSize(ca, cb)).unwrap();
        }
    }
    out
}

/// The positions and radii of every circle.
fn shape(s: &System, circles: &[(PointId, EntityId)]) -> Vec<[f64; 3]> {
    circles
        .iter()
        .map(|&(p, c)| {
            let v = pt(s, p);
            [v[0], v[1], s.radius(c).unwrap()]
        })
        .collect()
}

fn assert_same(a: &[[f64; 3]], b: &[[f64; 3]], what: &str) {
    for (k, (x, y)) in a.iter().zip(b).enumerate() {
        for d in 0..3 {
            assert!(
                (x[d] - y[d]).abs() < EPS,
                "{what}: circle {k}: {x:?} vs {y:?}"
            );
        }
    }
}

/// One drag step as the model runs it (`sketch.drag`): a new system, a
/// solve and a tight solve, the drag, the solves again, the analysis.
/// Returns the shape, the degrees of freedom and the time.
fn drag_step(n: usize, bind: Bind, goal: DragGoalOf) -> (Vec<[f64; 3]>, usize, Duration) {
    let t = Instant::now();
    let mut s = System::new();
    let circles = grid(&mut s, n, bind, 0.0);
    let tight = SolveOptions {
        tolerance: 1e-13,
        ..SolveOptions::default()
    };
    solve_ok(&mut s);
    s.solve(&tight);
    let (p, c) = circles[0];
    let goal = match goal {
        DragGoalOf::Centre(target) => DragGoal::Point { point: p, target },
        DragGoalOf::Radius(target) => DragGoal::Radius { entity: c, target },
    };
    s.drag(&[goal], &opts());
    solve_ok(&mut s);
    s.solve(&tight);
    let a = s.analyze();
    assert!(a.conflicts.is_empty() && a.redundant.is_empty(), "{a:?}");
    (shape(&s, &circles), a.dof, t.elapsed())
}

#[derive(Clone, Copy)]
enum DragGoalOf {
    Centre([f64; 2]),
    Radius(f64),
}

#[test]
fn chained_copies_solve_as_copies_bound_to_the_original() {
    for n in [2, 3, 6] {
        let mut shapes = Vec::new();
        for bind in [Bind::Star, Bind::Chain] {
            let mut s = System::new();
            let circles = grid(&mut s, n, bind, 0.5);
            solve_ok(&mut s);
            let a = s.analyze();
            // The original's centre and radius stay free; the copies add
            // no freedom and no redundancy.
            assert_eq!(a.dof, 3, "{bind:?} {n}");
            assert!(a.redundant.is_empty() && a.conflicts.is_empty());
            let got = shape(&s, &circles);
            for (k, v) in got.iter().enumerate() {
                let (i, j) = (k / n, k % n);
                assert_near(v[0] - got[0][0], 10.0 * i as f64, "x");
                assert_near(v[1] - got[0][1], 10.0 * j as f64, "y");
                assert_near(v[2], got[0][2], "radius");
            }
            shapes.push(got);
        }
        assert_same(&shapes[0], &shapes[1], "start");
    }
}

#[test]
fn chained_copies_drag_as_copies_bound_to_the_original() {
    for goal in [DragGoalOf::Centre([4.0, 2.5]), DragGoalOf::Radius(4.5)] {
        let (star, star_dof, _) = drag_step(5, Bind::Star, goal);
        let (chain, chain_dof, _) = drag_step(5, Bind::Chain, goal);
        assert_eq!((star_dof, chain_dof), (3, 3));
        assert_same(&star, &chain, "drag");
    }
    let (chain, _, _) = drag_step(4, Bind::Chain, DragGoalOf::Centre([4.0, 2.5]));
    assert_near(chain[15][0], 34.0, "last copy x");
    assert_near(chain[15][1], 32.5, "last copy y");
}

#[test]
fn a_chained_copy_fixed_elsewhere_conflicts_with_its_pattern() {
    for bind in [Bind::Star, Bind::Chain] {
        let mut s = System::new();
        let circles = grid(&mut s, 3, bind, 0.0);
        let bindings = s.constraints().count();
        s.add_constraint(FixPoint(circles[0].0)).unwrap();
        let (p, _) = circles[8];
        s.set_point(p, [21.0, 20.0]).unwrap();
        let fix = s.add_constraint(FixPoint(p)).unwrap();
        let r = s.solve(&opts());
        assert!(!r.is_ok(), "{bind:?}");
        // The conflict names the fixed copy and bindings of the pattern.
        assert!(!r.conflicts.is_empty(), "{bind:?}: {r:?}");
        let involved = &r.conflicts[0].involved;
        assert!(involved.contains(&fix), "{bind:?}: {involved:?}");
        assert!(involved.iter().any(|c| c.index() < bindings), "{bind:?}");
    }
}

/// Times of a drag step of the model's fixture
/// (`tools/cli/tests/perf_sketch_pattern_{5,10,20}.json`) in the solver
/// alone, medians of 5, both bindings (`MITCAD_BENCH_CHAIN_ONLY=1`: the
/// chain only, e.g. under a profiler).
#[test]
#[ignore]
fn bench_pattern_drag_step() {
    let binds = if std::env::var_os("MITCAD_BENCH_CHAIN_ONLY").is_some() {
        vec![Bind::Chain]
    } else {
        vec![Bind::Star, Bind::Chain]
    };
    for n in [5, 10, 20] {
        for &bind in &binds {
            let mut times: Vec<Duration> = (0..5)
                .map(|_| drag_step(n, bind, DragGoalOf::Centre([1.0, 0.5])).2)
                .collect();
            times.sort();
            eprintln!("{n} x {n} {bind:?}: drag step {:?}", times[2]);
        }
    }
}
