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
//! is consistent (redundant) or contradictory (conflict). With fixed
//! geometry the factorization leaves known columns out and factors implicit
//! equations later, with the same answers ([`crate::analysis_plan`]).
//!
//! An unknown is determined when its unit vector lies in the row space of the
//! Jacobian, i.e. when the Schur complement `1 - b^T G^-1 b` of its column
//! vanishes. This is a first-order (linearized) analysis at the current
//! geometry. Where the geometry sits at a singular point of its solution
//! set (an arc touching a line it ends on, mitcad#1774) and the rank drops
//! there, the analysis is that of a regular point beside it
//! ([`regular_nearby`]).

use std::collections::BTreeMap;

use crate::analysis_plan::{AnalysisPlan, Variant};
use crate::direction;
use crate::prepare::{Component, Prepared};
use crate::solve::{Ctx, Equations, NUDGE, SketchEquations, Work, lm, nudge};
use crate::sparse::{Factor, Rows, Symbolic};
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
    component_dependencies_within(eqs, comp, params, scale, work, want_determined, DEP_TOL)
}

/// [`component_dependencies`] with a row dependent when its pivot is below
/// `dep_tol`.
fn component_dependencies_within(
    eqs: &dyn Equations,
    comp: &Component,
    params: &[f64],
    scale: f64,
    work: &mut Work,
    want_determined: bool,
    dep_tol: f64,
) -> CompAnalysis {
    let mut f = std::mem::take(&mut work.f);
    eqs.evaluate(comp, params, scale, &mut f, Some(&mut work.jac));
    work.f = f;
    let jac = &work.jac;
    let nrm = unit_norms(comp, jac);
    let n = comp.vars.len();
    let plan_of = |v: Variant| comp.analysis_plans.get(jac, n, &comp.implicit, v);
    // With rows that fix unknowns, known columns left out and implicit
    // rows later where that is exact (mitcad#883); else creation order
    // with every column.
    let mut plan = None;
    if (0..comp.eqs.len()).any(|r| jac.row(r).0.len() == 1) {
        let p = plan_of(if comp.implicit.is_empty() {
            Variant::Reduced
        } else {
            Variant::Ordered
        });
        if p.pins_hold(jac) {
            plan = Some(p);
        }
    }
    let plan = plan.unwrap_or_else(|| plan_of(Variant::Plain));
    let ranked =
        analyse_in(&plan, comp, jac, &nrm, want_determined, dep_tol).unwrap_or_else(|| {
            // A row factored later depended on others: creation order.
            let reduced = plan_of(Variant::Reduced);
            let creation = if reduced.pins_hold(jac) {
                reduced
            } else {
                plan_of(Variant::Plain)
            };
            analyse_in(&creation, comp, jac, &nrm, want_determined, dep_tol)
                .expect("creation order moves no row")
        });
    let f = &work.f;
    let dependent: Vec<DepRow> = ranked
        .dependent
        .into_iter()
        .map(|(row, coeffs)| {
            let inconsistency = if nrm[row] > 0.0 {
                f[row] / nrm[row] - coeffs.iter().map(|&(j, v)| v * f[j] / nrm[j]).sum::<f64>()
            } else {
                f[row]
            };
            DepRow {
                row,
                coeffs,
                inconsistency,
            }
        })
        .collect();
    CompAnalysis {
        rank: ranked.rank,
        dependent,
        determined: ranked.determined,
    }
}

/// The length of each row in the weighted metric.
fn unit_norms(comp: &Component, jac: &Rows) -> Vec<f64> {
    (0..comp.eqs.len())
        .map(|i| {
            let (c, v) = jac.row(i);
            c.iter()
                .zip(v)
                .map(|(&c, &v)| v * v * comp.dcol[c])
                .sum::<f64>()
                .sqrt()
        })
        .collect()
}

/// The rank analysis by a plan, before the residuals.
struct Ranked {
    rank: usize,
    /// Dependent component rows (ascending), each with its combination of
    /// earlier rows (unit rows in the weighted metric, ascending).
    dependent: Dependent,
    determined: Option<Vec<bool>>,
}

/// The rank, the dependent rows and (`want_determined`) the determined
/// unknowns by `plan`; `None` when a row the plan factors later than
/// rows created after it is dependent there, so that the plan does not
/// give creation order's answers.
fn analyse_in(
    plan: &AnalysisPlan,
    comp: &Component,
    jac: &Rows,
    nrm: &[f64],
    want_determined: bool,
    dep_tol: f64,
) -> Option<Ranked> {
    let m = comp.eqs.len();
    let n = comp.vars.len();
    let (factor, dependent) = rank_rows(plan, comp, jac, nrm, n, dep_tol)?;
    let rank = m - factor.skipped.iter().filter(|&&s| s).count();
    let determined = if !want_determined {
        None
    } else if rank == n {
        Some(vec![true; n])
    } else if plan.has_pins() {
        let mut accepted = vec![false; m];
        for (k, &r) in plan.order.iter().enumerate() {
            accepted[r] = !factor.skipped[k];
        }
        Some(determined_free(plan, comp, jac, nrm, &accepted))
    } else {
        // Creation order, every column: positions are rows.
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
        Some(determined_in(
            &plan.gram.sym,
            &factor,
            &col_rows,
            &comp.dcol,
        ))
    };
    Some(Ranked {
        rank,
        dependent,
        determined,
    })
}

/// Dependent component rows, each with its combination of earlier rows.
type Dependent = Vec<(usize, Vec<(usize, f64)>)>;

/// Factors the rows in the plan's order and finds the dependent ones with
/// their combinations of earlier rows (component rows, ascending, of unit
/// rows in the weighted metric). `None` when a moved row is dependent:
/// the order does not hold.
fn rank_rows(
    plan: &AnalysisPlan,
    comp: &Component,
    jac: &Rows,
    nrm: &[f64],
    n: usize,
    dep_tol: f64,
) -> Option<(Factor, Dependent)> {
    let m = plan.order.len();
    let sym = &plan.gram.sym;
    let mut factor = Factor::new(sym);
    let mut w = Vec::new();
    let kept = plan.kept_rows(jac);
    plan.gram
        .fill(&kept, &comp.dcol, &mut factor, &mut w, false);
    // Unit rows of their full length, scaled as `GramPlan::fill` with
    // `normalize` scales whole rows.
    let full: Vec<f64> = plan
        .order
        .iter()
        .map(|&r| {
            let (c, v) = jac.row(r);
            c.iter().zip(v).map(|(&c, &v)| v * (v * comp.dcol[c])).sum()
        })
        .collect();
    for j in 0..m {
        for p in sym.lp[j]..sym.lp[j + 1] {
            let den = (full[j] * full[sym.li[p]]).sqrt();
            factor.ax[p] = if den > 0.0 { factor.ax[p] / den } else { 0.0 };
        }
    }
    let reference: Vec<f64> = full
        .iter()
        .map(|&d| if d > 0.0 { 1.0 } else { 0.0 })
        .collect();
    for (a, &d) in factor.adiag.iter_mut().zip(&full) {
        *a = if d > 0.0 { *a / d } else { 0.0 };
    }
    // The Jacobian has n columns, so its rank is at most n. With nearly
    // dependent rows the Gram matrix is so ill-conditioned (its condition is
    // the square of the Jacobian's) that the pivot of a row that lies in the
    // span of earlier rows can come out above `DEP_TOL`; once n rows are
    // accepted, every later row is dependent.
    factor.factor_against(sym, 0.0, Some(&reference), dep_tol, n);
    if (0..m).any(|k| plan.moved[k] && factor.skipped[k]) {
        return None;
    }
    let pins = plan.has_pins();
    // A row's entry (as a unit row) at an index of the Jacobian's values.
    let unit = |r: usize, at: usize| jac.val[at] / nrm[r];
    let mut c = vec![0.0; m];
    let mut mark = vec![false; m];
    let mut reach: Vec<usize> = Vec::new();
    let mut stack: Vec<usize> = Vec::new();
    let mut acc = vec![0.0; m];
    let mut in_acc = vec![false; m];
    let mut acc_at: Vec<usize> = Vec::new();
    let mut beta = vec![0.0; n];
    let mut in_beta = vec![false; n];
    let mut beta_at: Vec<usize> = Vec::new();
    // The combinations of the dependent rows that fix a column (by
    // position), to put in for that column.
    let mut pin_combination: BTreeMap<usize, Vec<(usize, f64)>> = BTreeMap::new();
    let mut dependent = Vec::new();
    for k in 0..m {
        if !factor.skipped[k] {
            continue;
        }
        // The combination of the kept rows: `L^T c = l` over the rows
        // before k, from the entries of row k of L; only the columns those
        // entries reach can be non-zero.
        reach.clear();
        for &(j, _) in &sym.rows[k] {
            if !mark[j] {
                mark[j] = true;
                stack.push(j);
            }
        }
        while let Some(j) = stack.pop() {
            reach.push(j);
            for &(i, _) in &sym.rows[j] {
                if !mark[i] {
                    mark[i] = true;
                    stack.push(i);
                }
            }
        }
        reach.sort_unstable();
        for &(j, p) in &sym.rows[k] {
            c[j] = factor.lx[p];
        }
        for &j in reach.iter().rev() {
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
        let mut add = |j: usize, v: f64| {
            if !in_acc[j] {
                in_acc[j] = true;
                acc_at.push(j);
            }
            acc[j] += v;
        };
        for &j in &reach {
            if c[j] != 0.0 {
                add(j, c[j]);
            }
        }
        let row_k = plan.order[k];
        if pins {
            // The combination of the full rows: also, for every left-out
            // column, its unit vector as much as row k has of it minus
            // what the combined rows have.
            let mut put = |col: usize, v: f64| {
                if !in_beta[col] {
                    in_beta[col] = true;
                    beta_at.push(col);
                }
                beta[col] += v;
            };
            if nrm[row_k] > 0.0 {
                for t in jac.ptr[row_k]..jac.ptr[row_k + 1] {
                    if plan.pin[jac.col[t]] < k {
                        put(jac.col[t], unit(row_k, t));
                    }
                }
            }
            for &j in &reach {
                let row_j = plan.order[j];
                if c[j] == 0.0 || nrm[row_j] == 0.0 {
                    continue;
                }
                for t in jac.ptr[row_j]..jac.ptr[row_j + 1] {
                    if plan.pin[jac.col[t]] < j {
                        put(jac.col[t], -c[j] * unit(row_j, t));
                    }
                }
            }
            for &col in &beta_at {
                in_beta[col] = false;
                let b = std::mem::replace(&mut beta[col], 0.0);
                if b == 0.0 {
                    continue;
                }
                // The unit vector of the column is |r_p| / r_pv times the
                // unit row of the row p that fixed it.
                let p = plan.pin[col];
                let s = b * nrm[plan.order[p]] / jac.val[plan.kept_at[plan.kept_ptr[p]]];
                if factor.skipped[p] {
                    for &(j, v) in &pin_combination[&p] {
                        add(j, s * v);
                    }
                } else {
                    add(p, s);
                }
            }
            beta_at.clear();
        }
        let combination: Vec<(usize, f64)> = acc_at
            .iter()
            .map(|&j| {
                in_acc[j] = false;
                (j, std::mem::replace(&mut acc[j], 0.0))
            })
            .collect();
        acc_at.clear();
        for &j in &reach {
            c[j] = 0.0;
            mark[j] = false;
        }
        dependent.push((row_k, coefficients(&plan.order, &combination)));
        if pins && matches!(jac.row(row_k).0, [col] if plan.pin[*col] == k) {
            pin_combination.insert(k, combination);
        }
    }
    dependent.sort_unstable_by_key(|d| d.0);
    Some((factor, dependent))
}

/// A combination by position as component rows, ascending, without the
/// negligible coefficients.
fn coefficients(order: &[usize], combination: &[(usize, f64)]) -> Vec<(usize, f64)> {
    let mut out: Vec<(usize, f64)> = combination
        .iter()
        .filter(|(_, v)| v.abs() > COEFF_TOL)
        .map(|&(j, v)| (order[j], v))
        .collect();
    out.sort_unstable_by_key(|&(j, _)| j);
    out
}

/// Which unknowns the accepted rows determine when rows fix unknowns: a
/// fixed one is; another one when its unit vector lies in the span of the
/// accepted rows without the fixed columns (which are orthogonal to it).
fn determined_free(
    plan: &AnalysisPlan,
    comp: &Component,
    jac: &Rows,
    nrm: &[f64],
    accepted: &[bool],
) -> Vec<bool> {
    let m = accepted.len();
    let n = comp.vars.len();
    let free = |c: usize| plan.pin[c] == usize::MAX;
    let gram = plan.free_gram(jac, n);
    let sym = &gram.sym;
    // Accepted rows' free entries as unit rows of their full length.
    let scale: Vec<f64> = (0..m)
        .map(|r| {
            if accepted[r] && nrm[r] > 0.0 {
                1.0 / nrm[r]
            } else {
                0.0
            }
        })
        .collect();
    let mut rows = Rows {
        ptr: vec![0],
        ..Rows::default()
    };
    for (r, &s) in scale.iter().enumerate() {
        let (cols, vals) = jac.row(r);
        for (&c, &v) in cols.iter().zip(vals) {
            if free(c) {
                rows.col.push(c);
                rows.val.push(v * s);
            }
        }
        rows.ptr.push(rows.col.len());
    }
    let mut factor = Factor::new(sym);
    let mut w = Vec::new();
    gram.fill(&rows, &comp.dcol, &mut factor, &mut w, false);
    let mut reference = vec![0.0; m];
    for (r, &s) in scale.iter().enumerate() {
        if s > 0.0 {
            reference[sym.pos[r]] = 1.0;
        }
    }
    factor.factor_against(sym, 0.0, Some(&reference), DEP_TOL, n);
    let mut col_rows: Vec<Vec<(usize, f64)>> = vec![Vec::new(); n];
    for r in 0..m {
        if factor.skipped[sym.pos[r]] {
            continue;
        }
        for t in rows.ptr[r]..rows.ptr[r + 1] {
            if rows.val[t] != 0.0 {
                col_rows[rows.col[t]].push((r, rows.val[t]));
            }
        }
    }
    let mut det = determined_in(sym, &factor, &col_rows, &comp.dcol);
    for (c, d) in det.iter_mut().enumerate() {
        if !free(c) {
            *d = true;
        }
    }
    det
}

/// `1 - b^T G^-1 b < DET_TOL` for the column `b` of each unknown (`col_rows`:
/// its entries in the unit rows, by row) with the factor of the rows' Gram
/// matrix.
fn determined_in(
    sym: &Symbolic,
    factor: &Factor,
    col_rows: &[Vec<(usize, f64)>],
    dcol: &[f64],
) -> Vec<bool> {
    let m = sym.n();
    let n = col_rows.len();
    // `b^T G^-1 b` for the column `b` of each unknown: from the entries
    // of `G^-1` between rows that share a column (a selected inverse,
    // mitcad#677) when the factor is sparse (its cost is the sum of
    // the squared column counts with a lookup each, linear for
    // chains), else by a forward solve per unknown (a full pass each).
    let selected: usize = (0..m).map(|j| (sym.lp[j + 1] - sym.lp[j] + 1).pow(2)).sum();
    if 4 * selected < n * (sym.nnz() + m) {
        let (zdiag, zx) = factor.selected_inverse(sym);
        let z = |a: usize, b: usize| -> f64 {
            if a == b {
                zdiag[sym.pos[a]]
            } else {
                sym.slot(a, b).map_or(0.0, |p| zx[p])
            }
        };
        (0..n)
            .map(|v| {
                let rows = &col_rows[v];
                if rows.is_empty() {
                    return false;
                }
                let mut q = 0.0;
                for (k, &(i, vi)) in rows.iter().enumerate() {
                    q += vi * vi * z(i, i);
                    for &(j, vj) in &rows[..k] {
                        q += 2.0 * vi * vj * z(i, j);
                    }
                }
                1.0 - q * dcol[v] < DET_TOL
            })
            .collect()
    } else {
        let mut b = vec![0.0; m];
        (0..n)
            .map(|v| {
                if col_rows[v].is_empty() {
                    return false;
                }
                b.iter_mut().for_each(|x| *x = 0.0);
                let s = dcol[v].sqrt();
                for &(i, val) in &col_rows[v] {
                    b[sym.pos[i]] = val * s;
                }
                factor.forward(sym, &mut b);
                let q: f64 = (0..m)
                    .filter(|&i| !factor.skipped[i])
                    .map(|i| b[i] * b[i] / factor.d[i])
                    .sum();
                1.0 - q < DET_TOL
            })
            .collect()
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

/// The pivot below which a row depends on earlier rows at the point beside
/// a singular one: stricter than [`DEP_TOL`], so that a point the solve
/// left within its tolerance of the singular one (a double point is met
/// only to the square root of the tolerance) is not taken for a regular
/// one, while a point [`NUDGE`] away is.
const NUDGED_DEP_TOL: f64 = 1e-8;
/// The most unknowns of a component [`regular_nearby`] solves again.
const MAX_NUDGED: usize = 4000;

/// The analysis of a component at a regular point of its solution set
/// beside `params` (mitcad#1774), when its rank there is higher than
/// `rank`. At a singular point the linearized equations lose rank although
/// the constraints take away as much freedom as anywhere beside it: an arc
/// whose ends lie on two lines and whose centre is held has its radius free,
/// and where it touches both lines (the circle meets each in a double
/// point) the rows of its end equations along the lines vanish, so that its
/// ends seemed free to slide (two degrees of freedom for one) and the lines'
/// dimensions followed from the rest. Every unknown is moved by up to
/// [`NUDGE`] of the sketch size (the same pseudo-random amounts every time),
/// the equations are solved again from there, and the rank found there with
/// a stricter tolerance is the rank of the solution set through `params`
/// where it is higher: a rank only drops at special points. None when the
/// solve does not converge, settles back where it was (an isolated
/// solution) or far away, or finds no more rank.
fn regular_nearby(
    eqs: &dyn Equations,
    comp: &Component,
    params: &[f64],
    scale: f64,
    rank: usize,
) -> Option<CompAnalysis> {
    if comp.vars.len() > MAX_NUDGED {
        return None;
    }
    let delta = NUDGE * scale;
    let mut moved = params.to_vec();
    nudge(comp, &mut moved, delta);
    let options = SolveOptions::default();
    let ctx = Ctx {
        eqs,
        scale,
        tol: options.tolerance * scale,
        max_iterations: options.max_iterations,
    };
    let mut work = Work::new(comp);
    if !lm(&ctx, comp, &mut moved, &mut work).converged {
        return None;
    }
    let away = comp
        .vars
        .iter()
        .map(|&g| (moved[g] - params[g]).powi(2))
        .sum::<f64>()
        .sqrt();
    if !(away > 0.01 * delta && away < 100.0 * delta) {
        return None;
    }
    let there =
        component_dependencies_within(eqs, comp, &moved, scale, &mut work, true, NUDGED_DEP_TOL);
    (there.rank > rank).then_some(there)
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
            let mut a = component_dependencies(&eqs, comp, &self.params, scale, &mut work, true);
            // A singular point of the solution set (an arc that touches a
            // line it ends on): told as at a regular point beside it
            // (mitcad#1774).
            let consistent = a
                .dependent
                .iter()
                .any(|d| d.inconsistency.abs() <= CONFLICT_TOL * scale);
            if consistent
                && a.rank < comp.vars.len()
                && let Some(regular) = regular_nearby(&eqs, comp, &self.params, scale, a.rank)
            {
                a = regular;
            }
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

/// What differs between two analyses of a component (empty: the same rank,
/// dependent rows, sets they depend on, coefficients within 1e-8 and
/// determined unknowns).
#[cfg(test)]
fn differences(a: &Ranked, b: &Ranked) -> String {
    let mut out = Vec::new();
    if a.rank != b.rank {
        out.push(format!("rank {} / {}", a.rank, b.rank));
    }
    let rows = |r: &Ranked| r.dependent.iter().map(|d| d.0).collect::<Vec<_>>();
    if rows(a) != rows(b) {
        out.push(format!("dependent rows {:?} / {:?}", rows(a), rows(b)));
    } else {
        for (x, y) in a.dependent.iter().zip(&b.dependent) {
            let set = |d: &[(usize, f64)]| d.iter().map(|c| c.0).collect::<Vec<_>>();
            if set(&x.1) != set(&y.1) {
                out.push(format!("row {}: {:?} / {:?}", x.0, x.1, y.1));
            } else if x.1.iter().zip(&y.1).any(|(p, q)| (p.1 - q.1).abs() > 1e-8) {
                out.push(format!("row {} coefficients: {:?} / {:?}", x.0, x.1, y.1));
            }
        }
    }
    if a.determined != b.determined {
        out.push(format!(
            "determined {:?} / {:?}",
            a.determined, b.determined
        ));
    }
    out.join("; ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Constraint::*;
    use std::f64::consts::PI;

    /// What a component's analysis is by each plan: the plain one, and
    /// whether the ordered one held. Asserts that every plan that holds
    /// gives the plain plan's answers.
    struct Compared {
        /// Per component with fixing rows: the ordered plan held, and the
        /// entries of L by the plain and the ordered plan.
        components: Vec<(bool, usize, usize)>,
    }

    fn compare(s: &mut System) -> Compared {
        s.ensure_prepared();
        let prep = s.prepared.take().expect("prepared");
        let scale = s.scale();
        let eqs = SketchEquations {
            prep: &prep,
            pkind: &s.pkind,
        };
        let mut components = Vec::new();
        for comp in &prep.comps {
            let mut work = Work::new(comp);
            let mut f = vec![0.0; comp.eqs.len()];
            eqs.evaluate(comp, &s.params, scale, &mut f, Some(&mut work.jac));
            let jac = &work.jac;
            let nrm = unit_norms(comp, jac);
            let n = comp.vars.len();
            let plan = |v| comp.analysis_plans.get(jac, n, &comp.implicit, v);
            let plain_plan = plan(Variant::Plain);
            let plain = analyse_in(&plain_plan, comp, jac, &nrm, true, DEP_TOL).expect("plain");
            let reduced = analyse_in(&plan(Variant::Reduced), comp, jac, &nrm, true, DEP_TOL)
                .expect("creation order holds");
            assert_eq!(differences(&plain, &reduced), "", "known columns left out");
            let ordered_plan = plan(Variant::Ordered);
            assert!(ordered_plan.pins_hold(jac));
            let ordered = analyse_in(&ordered_plan, comp, jac, &nrm, true, DEP_TOL);
            if let Some(ordered) = &ordered {
                assert_eq!(differences(&plain, ordered), "", "implicit rows later");
            }
            if ordered_plan.has_pins() {
                components.push((
                    ordered.is_some(),
                    plain_plan.gram.sym.nnz(),
                    ordered_plan.gram.sym.nnz(),
                ));
            }
        }
        s.prepared = Some(prep);
        Compared { components }
    }

    fn polar(s: &mut System, r: f64, a: f64) -> PointId {
        s.add_point(r * a.cos(), r * a.sin())
    }

    /// The outline of a castellated ring as projected edges: arcs of two
    /// radii about one shared centre, joined by radial lines, all fixed
    /// (their rows made the analysis's Gram matrix dense, mitcad#883).
    fn castellated(s: &mut System, teeth: usize) -> (PointId, Vec<EntityId>) {
        let c = s.add_point(0.0, 0.0);
        let step = 2.0 * PI / teeth as f64;
        let mut curves = Vec::new();
        let first = polar(s, 10.0, 0.0);
        let mut at = first;
        for i in 0..teeth {
            let a = i as f64 * step;
            let inner_end = polar(s, 10.0, a + 0.5 * step);
            curves.push(s.add_arc(c, at, inner_end).unwrap());
            let outer_start = polar(s, 12.0, a + 0.5 * step);
            curves.push(s.add_line(inner_end, outer_start).unwrap());
            let outer_end = polar(s, 12.0, a + step);
            curves.push(s.add_arc(c, outer_start, outer_end).unwrap());
            let next = if i + 1 == teeth {
                first
            } else {
                polar(s, 10.0, a + step)
            };
            curves.push(s.add_line(outer_end, next).unwrap());
            at = next;
        }
        for &e in &curves {
            s.add_constraint(FixEntity(e)).unwrap();
        }
        (c, curves)
    }

    #[test]
    fn fixed_geometry_keeps_the_gram_matrix_sparse() {
        let mut s = System::new();
        let (c, curves) = castellated(&mut s, 40);
        // Free geometry on it: a circle on the centre with its radius, a
        // horizontal line from the centre to an arc, a point on an arc.
        let rim = s.add_circle(c, 5.0).unwrap();
        s.add_constraint(Radius {
            entity: rim,
            value: 5.0,
        })
        .unwrap();
        let a = s.add_point(0.0, 0.0);
        let b = s.add_point(1.0, 0.0);
        let line = s.add_line(a, b).unwrap();
        s.add_constraint(Coincident(a, c)).unwrap();
        s.add_constraint(PointOnCurve(b, curves[0])).unwrap();
        s.add_constraint(Horizontal(line)).unwrap();
        let loose = s.add_point(11.0, 0.0);
        s.add_constraint(PointOnCurve(loose, curves[2])).unwrap();
        assert!(s.solve(&SolveOptions::default()).is_ok());
        let compared = compare(&mut s);
        let main = compared
            .components
            .iter()
            .max_by_key(|c| c.1)
            .expect("fixed geometry");
        assert!(main.0, "the ordered plan holds");
        assert!(
            main.2 * 20 < main.1,
            "L has {} entries in creation order, {} in the ordered plan",
            main.1,
            main.2
        );
        let a = s.analyze();
        // The point on an arc slides along it; the rest is fixed.
        assert_eq!(a.dof, 1);
        assert!(!a.redundant.is_empty());
        assert!(a.conflicts.is_empty());
    }

    #[test]
    fn fixed_points_that_follow_from_others() {
        // A point fixed after it is already known through a coincidence,
        // and constraints between known points: redundant, then in
        // conflict.
        for value in [5.0, 6.0] {
            let mut s = System::new();
            let p = s.add_point(0.0, 0.0);
            let q = s.add_point(0.0, 0.0);
            let r = s.add_point(3.0, 4.0);
            let t = s.add_point(3.0, 0.0);
            s.add_constraint(Coincident(p, q)).unwrap();
            s.add_constraint(FixPoint(q)).unwrap();
            s.add_constraint(FixPoint(p)).unwrap();
            s.add_constraint(FixPoint(r)).unwrap();
            s.add_constraint(FixPoint(r)).unwrap();
            s.add_constraint(Distance { a: p, b: r, value }).unwrap();
            let l = s.add_line(t, r).unwrap();
            s.add_constraint(Vertical(l)).unwrap();
            s.add_constraint(HorizontalPoints(p, t)).unwrap();
            s.add_constraint(FixPoint(t)).unwrap();
            s.solve(&SolveOptions::default());
            compare(&mut s);
            let a = s.analyze();
            if value == 5.0 {
                assert!(a.conflicts.is_empty());
                assert!(!a.redundant.is_empty());
            } else {
                assert!(!a.conflicts.is_empty());
            }
        }
    }

    #[test]
    fn fixed_ellipse_arcs_and_splines() {
        let mut s = System::new();
        // Elliptical arcs end to end with lines, all fixed. An arc that
        // ends at the end of its major axis (every other one) has an
        // equation that does not change with that end's parameter there:
        // factored later it depends on the rest, and that component is
        // analysed in creation order.
        let closing_at = |i: usize| if i.is_multiple_of(2) { PI } else { 2.5 };
        let closing: Vec<PointId> = (0..6)
            .map(|i| {
                let t = closing_at(i);
                s.add_point(20.0 * i as f64 + 6.0 * t.cos(), 3.0 * t.sin())
            })
            .collect();
        let mut curves = Vec::new();
        for i in 0..6 {
            let x = 20.0 * i as f64;
            let c = s.add_point(x, 0.0);
            let major = s.add_point(x + 6.0, 0.0);
            let end = s.add_point(x + 6.0 * (0.5f64).cos(), 3.0 * (0.5f64).sin());
            curves.push(
                s.add_elliptical_arc(c, major, 3.0, end, closing[i])
                    .unwrap(),
            );
            if i + 1 < 6 {
                curves.push(s.add_line(end, closing[i + 1]).unwrap());
            }
        }
        let fit: Vec<PointId> = (0..8)
            .map(|i| s.add_point(i as f64 * 5.0, 30.0 + (i as f64).sin()))
            .collect();
        let spline = s
            .add_fitted_spline(&fit, [SplineEnd::Natural, SplineEnd::Free])
            .unwrap();
        curves.push(spline);
        for &e in &curves {
            s.add_constraint(FixEntity(e)).unwrap();
        }
        // A free line from the spline's start to the first arc's start.
        let a = s.add_point(0.0, 29.0);
        let b = s.add_point(-6.0, 1.0);
        s.add_line(a, b).unwrap();
        s.add_constraint(Coincident(a, fit[0])).unwrap();
        s.add_constraint(Coincident(b, closing[0])).unwrap();
        assert!(s.solve(&SolveOptions::default()).is_ok());
        let compared = compare(&mut s);
        let held = compared.components.iter().filter(|c| c.0).count();
        assert!(held > 0 && held < compared.components.len());
    }

    /// A quarter arc about a fixed centre from a horizontal line's end to a
    /// vertical line's (their other ends fixed), touching both lines: its
    /// radius is free (mitcad#1774).
    fn touching_arc(s: &mut System) -> (PointId, PointId, EntityId) {
        let d = 10.0;
        let c = s.add_point(0.0, 0.0);
        let a = s.add_point(0.0, -d);
        let b = s.add_point(d, 0.0);
        let q1 = s.add_point(-40.0, -d);
        let q2 = s.add_point(d, 40.0);
        let arc = s.add_arc(c, a, b).unwrap();
        let h = s.add_line(q1, a).unwrap();
        let v = s.add_line(b, q2).unwrap();
        s.add_constraint(FixPoint(c)).unwrap();
        s.add_constraint(FixPoint(q1)).unwrap();
        s.add_constraint(FixPoint(q2)).unwrap();
        s.add_constraint(Horizontal(h)).unwrap();
        s.add_constraint(Vertical(v)).unwrap();
        (a, b, arc)
    }

    #[test]
    fn an_arc_touching_the_lines_it_ends_on_has_its_radius_free() {
        // Where the arc touches both lines its ends' equations lose their
        // rows along the lines: the ends seemed free to slide (2 degrees of
        // freedom) and the Vertical followed from the rest. Beside it, one
        // degree of freedom (the radius) and nothing redundant.
        let mut s = System::new();
        touching_arc(&mut s);
        assert!(s.solve(&SolveOptions::default()).is_ok());
        let a = s.analyze();
        assert_eq!(a.dof, 1);
        assert!(a.redundant.is_empty(), "{:?}", a.redundant);
        assert!(a.conflicts.is_empty());
    }

    #[test]
    fn an_arc_touching_the_lines_it_ends_on_can_be_dragged() {
        let mut s = System::new();
        let (a, b, arc) = touching_arc(&mut s);
        let r = s.drag(
            &[DragGoal::Point {
                point: a,
                target: [-5.0, -15.0],
            }],
            &SolveOptions::default(),
        );
        assert!(r.is_ok());
        // Its start slid along the horizontal line, its end along the
        // vertical one as far, and the radius grew.
        let (pa, pb) = (s.point(a).unwrap(), s.point(b).unwrap());
        assert!(pa[0] < -1.0, "{pa:?}");
        assert!((pa[1] + 10.0).abs() < 1e-9, "{pa:?}");
        assert!((pb[0] - 10.0).abs() < 1e-9, "{pb:?}");
        assert!((pa[0].abs() - pb[1].abs()).abs() < 1e-6, "{pa:?} {pb:?}");
        let radius = pa[0].hypot(pa[1]);
        assert!(radius > 10.05, "{radius}");
        assert!(s.entity_kind(arc).is_some());
    }

    #[test]
    fn a_held_point_on_a_touching_arc_stays_isolated() {
        // With its start held where it touches (a fixed point there), the
        // arc's end is held only to the second order: no point beside it
        // solves, so the analysis stays where it is.
        let mut s = System::new();
        let (a, _, _) = touching_arc(&mut s);
        s.add_constraint(FixPoint(a)).unwrap();
        assert!(s.solve(&SolveOptions::default()).is_ok());
        let a = s.analyze();
        assert!(a.conflicts.is_empty());
        assert!(a.dof <= 1, "{}", a.dof);
    }
}
