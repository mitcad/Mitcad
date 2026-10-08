// SPDX-License-Identifier: MIT
//! Degrees of freedom, fully constrained geometry and dependent constraints.
//!
//! The rows of the Jacobian (one per equation, scaled to unit length in the
//! weighted metric) are processed in creation order: implicit equations of
//! the entities first, then the constraints in the order they were added.
//! A Cholesky factorization of their Gram matrix in that order gives each
//! row's squared distance from the span of the rows before it as its pivot.
//! A pivot below `DEP_TOL` marks a row that depends on earlier rows: the
//! newest constraint of a dependent set is the one reported, as in an
//! interactive sketcher that refuses the constraint just added. The
//! dependency coefficients (from the factor) give the other constraints of
//! the set, and the same combination of the residuals tells whether the set
//! is consistent (redundant) or contradictory (conflict).
//!
//! An unknown is determined when its unit vector lies in the row space of the
//! Jacobian, i.e. when the Schur complement `1 - b^T G^-1 b` of its column
//! vanishes. This is a first-order (linearized) analysis at the current
//! geometry.

use std::collections::BTreeMap;

use crate::direction;
use crate::prepare::{Component, Prepared};
use crate::solve::{Equations, SketchEquations, Work};
use crate::sparse::{Factor, GramPlan};
use crate::system::System;
use crate::types::*;

/// Pivot tolerance (squared relative distance of a row from earlier rows).
const DEP_TOL: f64 = 1e-10;
/// Tolerance of the squared distance of an unknown's unit vector from the
/// row space.
const DET_TOL: f64 = 1e-8;
/// A dependency coefficient below this is not part of the dependent set.
const COEFF_TOL: f64 = 1e-7;
/// Inconsistency (millimetres, relative to the sketch size) above which a
/// dependent set is a conflict.
pub(crate) const CONFLICT_TOL: f64 = 1e-6;

pub(crate) struct DepRow {
    /// Component-local row.
    pub row: usize,
    /// Earlier rows and their coefficients in the combination.
    pub coeffs: Vec<(usize, f64)>,
    /// Residual of the row minus the same combination of the earlier
    /// residuals (distance units).
    pub inconsistency: f64,
}

pub(crate) struct CompAnalysis {
    pub rank: usize,
    pub dependent: Vec<DepRow>,
    pub determined: Option<Vec<bool>>,
}

pub(crate) fn component_dependencies(
    eqs: &dyn Equations,
    comp: &Component,
    params: &[f64],
    scale: f64,
    work: &mut Work,
    want_determined: bool,
) -> CompAnalysis {
    let m = comp.eqs.len();
    let n = comp.vars.len();
    let mut f = std::mem::take(&mut work.f);
    eqs.evaluate(comp, params, scale, &mut f, Some(&mut work.jac));
    work.f = f;
    let jac = &work.jac;
    let plan = comp.analysis_plan.get_or_init(|| {
        let row_cols: Vec<Vec<usize>> = (0..m).map(|i| jac.row(i).0.to_vec()).collect();
        let order: Vec<usize> = (0..m).collect();
        GramPlan::new(&row_cols, n, Some(&order))
    });
    let sym = &plan.sym;
    let mut factor = Factor::new(sym);
    let mut w = Vec::new();
    plan.fill(jac, &comp.dcol, &mut factor, &mut w, true);
    let nrm: Vec<f64> = (0..m)
        .map(|i| {
            let (c, v) = jac.row(i);
            c.iter()
                .zip(v)
                .map(|(&c, &v)| v * v * comp.dcol[c])
                .sum::<f64>()
                .sqrt()
        })
        .collect();
    let skipped = factor.factor(sym, 0.0, DEP_TOL);
    let rank = m - skipped;
    let f = &work.f;
    let mut dependent = Vec::new();
    for k in 0..m {
        if !factor.skipped[k] {
            continue;
        }
        let mut c = vec![0.0; k];
        for &(j, p) in &sym.rows[k] {
            c[j] = factor.lx[p];
        }
        for j in (0..k).rev() {
            if factor.skipped[j] {
                c[j] = 0.0;
                continue;
            }
            let mut s = c[j];
            for p in sym.lp[j]..sym.lp[j + 1] {
                let r = sym.li[p];
                if r >= k {
                    break;
                }
                s -= factor.lx[p] * c[r];
            }
            c[j] = s;
        }
        let coeffs: Vec<(usize, f64)> = c
            .iter()
            .enumerate()
            .filter(|(_, v)| v.abs() > COEFF_TOL)
            .map(|(j, &v)| (j, v))
            .collect();
        let inconsistency = if nrm[k] > 0.0 {
            f[k] / nrm[k] - coeffs.iter().map(|&(j, v)| v * f[j] / nrm[j]).sum::<f64>()
        } else {
            f[k]
        };
        dependent.push(DepRow {
            row: k,
            coeffs,
            inconsistency,
        });
    }
    let determined = if !want_determined {
        None
    } else if rank == n {
        Some(vec![true; n])
    } else {
        let mut col_rows: Vec<Vec<(usize, f64)>> = vec![Vec::new(); n];
        for (i, &norm) in nrm.iter().enumerate() {
            if factor.skipped[i] || norm == 0.0 {
                continue;
            }
            let (cols, vals) = jac.row(i);
            for (&c, &v) in cols.iter().zip(vals) {
                col_rows[c].push((i, v / norm));
            }
        }
        let mut b = vec![0.0; m];
        let det = (0..n)
            .map(|v| {
                if col_rows[v].is_empty() {
                    return false;
                }
                b.iter_mut().for_each(|x| *x = 0.0);
                let s = comp.dcol[v].sqrt();
                for &(i, val) in &col_rows[v] {
                    b[i] = val * s;
                }
                factor.forward(sym, &mut b);
                let q: f64 = (0..m)
                    .filter(|&i| !factor.skipped[i])
                    .map(|i| b[i] * b[i] / factor.d[i])
                    .sum();
                1.0 - q < DET_TOL
            })
            .collect();
        Some(det)
    };
    CompAnalysis {
        rank,
        dependent,
        determined,
    }
}

/// Groups dependent rows by constraint. `conflicting` selects the
/// inconsistent sets, otherwise the consistent (redundant) ones.
pub(crate) fn to_dependencies(
    prep: &Prepared,
    comp: &Component,
    rows: &[DepRow],
    scale: f64,
    conflicting: bool,
) -> Vec<Dependency> {
    let mut by: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for d in rows {
        if (d.inconsistency.abs() > CONFLICT_TOL * scale) != conflicting {
            continue;
        }
        let owner = |r: usize| prep.eqs[comp.eqs[r]].constraint();
        let mut involved: Vec<usize> = d.coeffs.iter().filter_map(|&(r, _)| owner(r)).collect();
        let main = match owner(d.row) {
            Some(c) => c,
            None => match involved.iter().max() {
                Some(&c) => c,
                None => continue,
            },
        };
        involved.push(main);
        by.entry(main).or_default().extend(involved);
    }
    by.into_iter()
        .map(|(c, mut inv)| {
            inv.sort_unstable();
            inv.dedup();
            Dependency {
                constraint: ConstraintId(c as u32),
                involved: inv.into_iter().map(|i| ConstraintId(i as u32)).collect(),
            }
        })
        .collect()
}

/// Merges dependencies of the same constraint.
fn merge(mut deps: Vec<Dependency>) -> Vec<Dependency> {
    deps.sort_by_key(|d| d.constraint);
    let mut out: Vec<Dependency> = Vec::new();
    for d in deps {
        match out.last_mut() {
            Some(last) if last.constraint == d.constraint => {
                last.involved.extend(d.involved);
                last.involved.sort_unstable();
                last.involved.dedup();
            }
            _ => out.push(d),
        }
    }
    out
}

impl System {
    /// Degrees of freedom, fully constrained points and entities, and
    /// redundant or conflicting constraints at the current geometry (call it
    /// after a successful [`System::solve`]).
    pub fn analyze(&mut self) -> Analysis {
        let classes = self.point_classes();
        let (dir_conflicts, dir_redundant) = direction::check(self, &classes);
        self.ensure_prepared();
        let prep = self.prepared.take().expect("prepared");
        let scale = self.scale();
        let mut dof = prep.free.len();
        let mut determined = vec![false; self.params.len()];
        let mut conflicts = dir_conflicts;
        let mut redundant = dir_redundant;
        let eqs = SketchEquations {
            prep: &prep,
            pkind: &self.pkind,
        };
        for comp in &prep.comps {
            let mut work = Work::new(comp);
            let a = component_dependencies(&eqs, comp, &self.params, scale, &mut work, true);
            dof += comp.vars.len() - a.rank;
            if let Some(det) = &a.determined {
                for (k, &g) in comp.vars.iter().enumerate() {
                    determined[g] = det[k];
                }
            }
            conflicts.extend(to_dependencies(&prep, comp, &a.dependent, scale, true));
            redundant.extend(to_dependencies(&prep, comp, &a.dependent, scale, false));
        }
        self.prepared = Some(prep);
        let conflicts = merge(conflicts);
        let redundant: Vec<Dependency> = merge(redundant)
            .into_iter()
            .filter(|d| !conflicts.iter().any(|c| c.constraint == d.constraint))
            .collect();
        let mut fully_constrained_points = Vec::new();
        for (i, p) in self.points.iter().enumerate() {
            if let Some(p) = p
                && !p.hidden
                && determined[p.x]
                && determined[p.y]
            {
                fully_constrained_points.push(PointId(i as u32));
            }
        }
        let mut fully_constrained_entities = Vec::new();
        for (i, r) in self.entities.iter().enumerate() {
            let Some(r) = r else { continue };
            let points_ok = r.data.points().iter().all(|&p| {
                let d = self.pd(p);
                determined[d.x] && determined[d.y]
            });
            if points_ok && r.data.scalars().iter().all(|&s| determined[s]) {
                fully_constrained_entities.push(EntityId(i as u32));
            }
        }
        Analysis {
            dof,
            fully_constrained_points,
            fully_constrained_entities,
            redundant,
            conflicts,
        }
    }
}
