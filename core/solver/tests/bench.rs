// SPDX-License-Identifier: MIT
//! Timing of larger sketches. Ignored by default; run in release:
//! `cargo test --release -p mitcad-solver --test bench -- --ignored --nocapture`

mod common;

use common::*;
use mitcad_solver::Constraint::*;
use mitcad_solver::{ConstraintId, PointId, System};
use std::time::Instant;

/// `n` rectangles in a chain (one connected component): `4 n` lines and
/// points and `8 n - 1` constraints, fully constrained.
fn rectangles(s: &mut System, rng: &mut Rng, n: usize) -> (Vec<ConstraintId>, Vec<PointId>) {
    let mut dims = Vec::new();
    let mut pts = Vec::new();
    let mut prev: Option<Rect> = None;
    for i in 0..n {
        let r = rect(s, i as f64 * 30.0, 0.0, 20.0, 10.0);
        dims.push(
            s.add_constraint(HorizontalDistance {
                a: r.p[0],
                b: r.p[1],
                value: 20.0,
            })
            .unwrap(),
        );
        dims.push(
            s.add_constraint(VerticalDistance {
                a: r.p[1],
                b: r.p[2],
                value: 10.0,
            })
            .unwrap(),
        );
        match &prev {
            None => {
                s.add_constraint(FixPoint(r.p[0])).unwrap();
            }
            Some(p) => {
                dims.push(
                    s.add_constraint(HorizontalDistance {
                        a: p.p[1],
                        b: r.p[0],
                        value: 10.0,
                    })
                    .unwrap(),
                );
                dims.push(
                    s.add_constraint(VerticalDistance {
                        a: p.p[0],
                        b: r.p[0],
                        value: 0.0,
                    })
                    .unwrap(),
                );
            }
        }
        pts.extend_from_slice(&r.p);
        prev = Some(r);
    }
    for &p in &pts {
        let v = pt(s, p);
        s.set_point(p, [v[0] + rng.sym(1.0), v[1] + rng.sym(1.0)])
            .unwrap();
    }
    (dims, pts)
}

/// 25 slots with a tangent circle each, chained: 150 entities (50 arcs, 50
/// lines, 25 circles) and about 300 constraints.
fn slots(s: &mut System, rng: &mut Rng) -> Vec<ConstraintId> {
    let mut dims = Vec::new();
    let mut prev: Option<PointId> = None;
    for i in 0..25 {
        let x = i as f64 * 40.0;
        let c1 = s.add_point(x, 0.0);
        let c2 = s.add_point(x + 20.0, 0.0);
        let tl = s.add_point(x, 5.0);
        let tr = s.add_point(x + 20.0, 5.0);
        let br = s.add_point(x + 20.0, -5.0);
        let bl = s.add_point(x, -5.0);
        let left = s.add_arc(c1, tl, bl).unwrap();
        let right = s.add_arc(c2, br, tr).unwrap();
        let top = s.add_line(tr, tl).unwrap();
        let bottom = s.add_line(bl, br).unwrap();
        for (l, a) in [(top, left), (top, right), (bottom, left), (bottom, right)] {
            s.add_constraint(Tangent(l, a)).unwrap();
        }
        s.add_constraint(Equal(left, right)).unwrap();
        dims.push(
            s.add_constraint(Radius {
                entity: left,
                value: 5.0,
            })
            .unwrap(),
        );
        dims.push(
            s.add_constraint(Distance {
                a: c1,
                b: c2,
                value: 20.0,
            })
            .unwrap(),
        );
        s.add_constraint(HorizontalPoints(c1, c2)).unwrap();
        let c3 = s.add_point(x + 10.0, 9.0);
        let circle = s.add_circle(c3, 4.0).unwrap();
        s.add_constraint(Tangent(top, circle)).unwrap();
        dims.push(
            s.add_constraint(Radius {
                entity: circle,
                value: 4.0,
            })
            .unwrap(),
        );
        dims.push(
            s.add_constraint(HorizontalDistance {
                a: c1,
                b: c3,
                value: 10.0,
            })
            .unwrap(),
        );
        match prev {
            None => {
                s.add_constraint(FixPoint(c1)).unwrap();
            }
            Some(p) => {
                dims.push(
                    s.add_constraint(HorizontalDistance {
                        a: p,
                        b: c1,
                        value: 20.0,
                    })
                    .unwrap(),
                );
                s.add_constraint(HorizontalPoints(p, c1)).unwrap();
            }
        }
        prev = Some(c2);
        for p in [c1, c2, tl, tr, br, bl, c3] {
            let v = pt(s, p);
            s.set_point(p, [v[0] + rng.sym(0.5), v[1] + rng.sym(0.5)])
                .unwrap();
        }
    }
    dims
}

fn report(name: &str, s: &mut System, dims: &[ConstraintId]) {
    report_dof(name, s, dims, 0);
}

fn report_dof(name: &str, s: &mut System, dims: &[ConstraintId], dof: usize) {
    let n_constraints = s.constraints().count();
    let n_entities = s.entities().count();
    let t = Instant::now();
    let r = s.solve(&opts());
    let first = t.elapsed();
    assert!(r.is_ok(), "{r:?}");
    let first_iterations = r.iterations;
    // Incremental: change one dimension at a time and solve again.
    let rounds = 20;
    let t = Instant::now();
    let mut iterations = 0;
    for k in 0..rounds {
        let id = dims[(k * 7) % dims.len()];
        let v = s.dimension_value(id).unwrap();
        s.set_dimension_value(id, v + 1.0).unwrap();
        let r = s.solve(&opts());
        assert!(r.is_ok(), "{r:?}");
        iterations += r.iterations;
    }
    let incremental = t.elapsed() / rounds as u32;
    let t = Instant::now();
    let r = s.solve(&opts());
    let unchanged = t.elapsed();
    assert!(r.is_ok());
    let t = Instant::now();
    let an = s.analyze();
    let analysis = t.elapsed();
    eprintln!(
        "{name}: {n_entities} entities, {n_constraints} constraints; \
         first solve {first:?} ({first_iterations} iterations), \
         re-solve after a dimension change {incremental:?} ({:.1} iterations), \
         re-solve unchanged {unchanged:?}, analysis {analysis:?} (dof {})",
        iterations as f64 / rounds as f64,
        an.dof
    );
    assert_eq!(an.dof, dof);
    assert!(an.conflicts.is_empty());
}

#[test]
#[ignore]
fn bench_rectangles_200_lines_400_constraints() {
    let mut s = System::new();
    let mut rng = Rng(99);
    let (dims, _) = rectangles(&mut s, &mut rng, 50);
    report("50 rectangles", &mut s, &dims);
}

#[test]
#[ignore]
fn bench_rectangles_under_constrained() {
    // Without the chain dimensions: every rectangle but the first can move
    // (2 degrees of freedom each), so the analysis checks every unknown.
    let mut s = System::new();
    let mut rng = Rng(98);
    let (dims, _) = rectangles(&mut s, &mut rng, 50);
    let chain: Vec<ConstraintId> = s
        .constraints()
        .filter(|(_, c)| matches!(c, VerticalDistance { value, .. } if *value == 0.0))
        .map(|(id, _)| id)
        .collect();
    for id in &chain {
        s.remove_constraint(*id).unwrap();
    }
    let dims: Vec<ConstraintId> = dims.into_iter().filter(|d| !chain.contains(d)).collect();
    report_dof("50 rectangles, 49 dof", &mut s, &dims, 49);
}

#[test]
#[ignore]
fn bench_rectangles_2000_lines() {
    let mut s = System::new();
    let mut rng = Rng(97);
    let (dims, _) = rectangles(&mut s, &mut rng, 500);
    report("500 rectangles", &mut s, &dims);
}

#[test]
#[ignore]
fn bench_slots_with_arcs_and_tangents() {
    let mut s = System::new();
    let mut rng = Rng(5);
    let dims = slots(&mut s, &mut rng);
    report("25 slots + circles", &mut s, &dims);
}
