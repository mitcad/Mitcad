// SPDX-License-Identifier: MIT
//! Newton / Levenberg-Marquardt iteration on the residual equations of one
//! component, the drag iteration, and the solve driver of [`System`].
//!
//! The step is the minimum-norm (in the weighted metric `W = D^-1`)
//! Levenberg-Marquardt step in its dual form
//! `dx = -D J^T (J D J^T + mu I)^-1 f`, which equals
//! `-(J^T J + mu W)^-1 J^T f`. For under-constrained sketches (the usual
//! case) it moves the geometry as little as possible; with `mu -> 0` it is
//! the Gauss-Newton step of minimal norm. `mu` follows the gain ratio
//! (Nielsen's update), so far-off starting geometry still converges.
//!
//! The iterations work on any residual equations ([`Equations`]): the
//! sketch's and the rigid bodies' of `rigid.rs`.

use crate::analysis;
use crate::direction;
use crate::prepare::{Component, Prepared, prepare};
use crate::scalar::{Dual, LANES};
use crate::sparse::{Factor, Rows};
use crate::system::{EntityData, ParamKind, System};
use crate::types::*;

/// Residual equations of components of unknowns, as the iterations and
/// the rank analysis see them.
pub(crate) trait Equations {
    /// The residuals of `comp`'s rows at `params` (global unknowns) and,
    /// with `jac`, the Jacobian in `comp.jac`'s pattern.
    fn evaluate(
        &self,
        comp: &Component,
        params: &[f64],
        scale: f64,
        f: &mut [f64],
        jac: Option<&mut Rows>,
    );

    /// The range a global unknown is kept in, if it has one.
    fn bounds(&self, var: usize) -> Option<(f64, f64)>;
}

/// The sketch's equations.
pub(crate) struct SketchEquations<'a> {
    pub prep: &'a Prepared,
    pub pkind: &'a [ParamKind],
}

impl Equations for SketchEquations<'_> {
    fn evaluate(
        &self,
        comp: &Component,
        params: &[f64],
        scale: f64,
        f: &mut [f64],
        jac: Option<&mut Rows>,
    ) {
        evaluate(self.prep, comp, params, scale, f, jac);
    }

    fn bounds(&self, var: usize) -> Option<(f64, f64)> {
        match self.pkind[var] {
            ParamKind::SplineT { lo, hi } => Some((lo, hi)),
            _ => None,
        }
    }
}

/// Residuals (and optionally the Jacobian) of a component.
pub(crate) fn evaluate(
    prep: &Prepared,
    comp: &Component,
    params: &[f64],
    scale: f64,
    f: &mut [f64],
    jac: Option<&mut Rows>,
) {
    match jac {
        None => {
            let mut x = Vec::new();
            for (r, &ei) in comp.eqs.iter().enumerate() {
                let eq = &prep.eqs[ei];
                x.clear();
                x.extend(eq.vars.iter().map(|&g| params[g]));
                f[r] = eq.kind.residual(&x, eq.value, scale);
            }
        }
        Some(jac) => {
            let mut x: Vec<Dual> = Vec::new();
            for (r, &ei) in comp.eqs.iter().enumerate() {
                let eq = &prep.eqs[ei];
                let n = eq.vars.len();
                let base = jac.ptr[r];
                let mut start = 0;
                loop {
                    x.clear();
                    x.extend(eq.vars.iter().enumerate().map(|(i, &g)| {
                        let lane = (i >= start && i < start + LANES).then(|| i - start);
                        Dual::var(params[g], lane)
                    }));
                    let v = eq.kind.residual(&x, eq.value, scale);
                    f[r] = v.v;
                    for i in start..(start + LANES).min(n) {
                        jac.val[base + i] = v.d[i - start];
                    }
                    start += LANES;
                    if start >= n {
                        break;
                    }
                }
            }
        }
    }
}

fn max_abs(v: &[f64]) -> f64 {
    v.iter().fold(0.0, |m, x| m.max(x.abs()))
}

fn norm2(v: &[f64]) -> f64 {
    v.iter().map(|x| x * x).sum()
}

/// Keeps bounded unknowns (curve parameters, limited motions) in range.
fn clamp(eqs: &dyn Equations, params: &mut [f64], vars: &[usize]) {
    for &g in vars {
        if let Some((lo, hi)) = eqs.bounds(g) {
            params[g] = params[g].clamp(lo, hi);
        }
    }
}

pub(crate) struct Run {
    pub converged: bool,
    pub iterations: usize,
    pub residual: f64,
}

/// Workspace for one component.
pub(crate) struct Work {
    pub f: Vec<f64>,
    fnew: Vec<f64>,
    pub jac: Rows,
    factor: Factor,
    lam: Vec<f64>,
    dx: Vec<f64>,
    w: Vec<f64>,
    saved: Vec<f64>,
}

impl Work {
    pub fn new(comp: &Component) -> Work {
        let m = comp.eqs.len();
        Work {
            f: vec![0.0; m],
            fnew: vec![0.0; m],
            jac: comp.jac.clone(),
            factor: Factor::new(&comp.plan.sym),
            lam: vec![0.0; m],
            dx: vec![0.0; comp.vars.len()],
            w: Vec::new(),
            saved: vec![0.0; comp.vars.len()],
        }
    }

    /// `dx = g - D J^T (J D J^T + mu I)^-1 (J g + f)` with the factor of the
    /// current Jacobian (`g = 0` for a plain Newton/LM step).
    fn step(&mut self, comp: &Component, dcol: &[f64], g: Option<&[f64]>) {
        let sym = &comp.plan.sym;
        let m = comp.eqs.len();
        for i in 0..m {
            let mut rhs = self.f[i];
            if let Some(g) = g {
                let (cols, vals) = self.jac.row(i);
                rhs += cols.iter().zip(vals).map(|(&c, &v)| v * g[c]).sum::<f64>();
            }
            self.lam[sym.pos[i]] = rhs;
        }
        self.factor.solve(sym, &mut self.lam);
        match g {
            Some(g) => self.dx.copy_from_slice(g),
            None => self.dx.iter_mut().for_each(|x| *x = 0.0),
        }
        for i in 0..m {
            let l = self.lam[sym.pos[i]];
            if l != 0.0 {
                let (cols, vals) = self.jac.row(i);
                for (&c, &v) in cols.iter().zip(vals) {
                    self.dx[c] -= dcol[c] * v * l;
                }
            }
        }
    }

    fn jac_times(&self, i: usize, dx: &[f64]) -> f64 {
        let (cols, vals) = self.jac.row(i);
        cols.iter().zip(vals).map(|(&c, &v)| v * dx[c]).sum()
    }
}

pub(crate) struct Ctx<'a> {
    pub eqs: &'a dyn Equations,
    pub scale: f64,
    pub tol: f64,
    pub max_iterations: usize,
}

/// Levenberg-Marquardt on one component until every residual is below the
/// tolerance.
pub(crate) fn lm(ctx: &Ctx, comp: &Component, params: &mut [f64], work: &mut Work) -> Run {
    let sym = &comp.plan.sym;
    ctx.eqs.evaluate(comp, params, ctx.scale, &mut work.f, None);
    let mut fn2 = norm2(&work.f);
    let mut mu = -1.0;
    let mut nu = 2.0;
    let mut max_diag: f64 = 0.0;
    let mut need_jac = true;
    let mut slow = 0;
    // Iterate a little past the tolerance (Newton converges quadratically,
    // so this usually costs one step) to leave fixed geometry and exact
    // dimensions exact to rounding; a stall below the tolerance is fine.
    let target = (ctx.tol * 1e-3).max(1e-14 * ctx.scale);
    // Bounded unknowns (curve parameters, limited motions) at the end of
    // their range that a step would push outward are left out of that step
    // (an active set for the bounds).
    let bounded: Vec<(usize, f64, f64)> = comp
        .vars
        .iter()
        .enumerate()
        .filter_map(|(c, &g)| ctx.eqs.bounds(g).map(|(lo, hi)| (c, lo, hi)))
        .collect();
    let mut dcol = comp.dcol.clone();
    let mut iterations = 0;
    for it in 0..ctx.max_iterations {
        let res = max_abs(&work.f);
        if res <= target {
            return Run {
                converged: true,
                iterations: it,
                residual: res,
            };
        }
        if need_jac {
            let mut f = std::mem::take(&mut work.f);
            ctx.eqs
                .evaluate(comp, params, ctx.scale, &mut f, Some(&mut work.jac));
            work.f = f;
            dcol.copy_from_slice(&comp.dcol);
            comp.plan
                .fill(&work.jac, &dcol, &mut work.factor, &mut work.w, false);
            max_diag = work
                .factor
                .adiag
                .iter()
                .fold(0.0, |m: f64, &d| m.max(d))
                .max(1e-300);
            if mu < 0.0 {
                mu = 1e-10 * max_diag;
            }
            need_jac = false;
        }
        work.factor.factor(sym, mu, 1e-14);
        work.step(comp, &dcol, None);
        loop {
            let mut changed = false;
            for &(c, lo, hi) in &bounded {
                let x = params[comp.vars[c]];
                let outward = (x <= lo && work.dx[c] < 0.0) || (x >= hi && work.dx[c] > 0.0);
                if dcol[c] != 0.0 && outward {
                    dcol[c] = 0.0;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
            comp.plan
                .fill(&work.jac, &dcol, &mut work.factor, &mut work.w, false);
            work.factor.factor(sym, mu, 1e-14);
            work.step(comp, &dcol, None);
        }
        let pred = fn2
            - (0..work.f.len())
                .map(|i| {
                    let v = work.f[i] + work.jac_times(i, &work.dx);
                    v * v
                })
                .sum::<f64>();
        for (k, &g) in comp.vars.iter().enumerate() {
            work.saved[k] = params[g];
            params[g] += work.dx[k];
        }
        clamp(ctx.eqs, params, &comp.vars);
        ctx.eqs
            .evaluate(comp, params, ctx.scale, &mut work.fnew, None);
        let new2 = norm2(&work.fnew);
        let actual = fn2 - new2;
        let rho = if pred > 0.0 { actual / pred } else { -1.0 };
        if new2.is_finite() && actual > 0.0 && rho > 1e-4 {
            if actual < 1e-12 * fn2 {
                slow += 1;
            } else {
                slow = 0;
            }
            std::mem::swap(&mut work.f, &mut work.fnew);
            fn2 = new2;
            need_jac = true;
            mu *= (1.0 - (2.0 * rho - 1.0).powi(3)).max(1.0 / 3.0);
            mu = mu.max(1e-15 * max_diag);
            nu = 2.0;
            if slow >= 3 {
                iterations = it + 1;
                break;
            }
        } else {
            for (k, &g) in comp.vars.iter().enumerate() {
                params[g] = work.saved[k];
            }
            if res <= ctx.tol {
                // Rounding limits further progress; good enough.
                iterations = it + 1;
                break;
            }
            mu *= nu;
            nu *= 2.0;
            if mu > 1e12 * max_diag {
                iterations = it + 1;
                break;
            }
        }
        iterations = it + 1;
    }
    let res = max_abs(&work.f);
    Run {
        converged: res <= ctx.tol,
        iterations,
        residual: res,
    }
}

/// Pulls the goal unknowns (`(column, target)`) toward their targets while
/// the constraints hold. Each step moves the goals as far as the linearized
/// constraints allow (a projection in the weighted metric, with the goals
/// weighted heavily so that the rest of the geometry moves as little as
/// possible), then [`lm`] restores the constraints. A step that does not
/// bring the goals closer is halved (the constraint curvature can make full
/// steps overshoot).
pub(crate) fn drag(
    ctx: &Ctx,
    comp: &Component,
    params: &mut [f64],
    work: &mut Work,
    goals: &[(usize, f64)],
) -> Run {
    let sym = &comp.plan.sym;
    let first = lm(ctx, comp, params, work);
    if !first.converged {
        return first;
    }
    let mut iterations = first.iterations;
    let n = comp.vars.len();
    let mut dcol = comp.dcol.clone();
    for &(c, _) in goals {
        dcol[c] *= 1e-6;
    }
    let goal_dist = |params: &[f64]| -> f64 {
        goals
            .iter()
            .map(|&(c, t)| (t - params[comp.vars[c]]).powi(2))
            .sum::<f64>()
            .sqrt()
    };
    let mut g = vec![0.0; n];
    let mut good: Vec<f64> = comp.vars.iter().map(|&v| params[v]).collect();
    let mut good_dist = goal_dist(params);
    let mut alpha: f64 = 1.0;
    for _ in 0..60 {
        if good_dist <= ctx.tol {
            break;
        }
        g.iter_mut().for_each(|x| *x = 0.0);
        for &(c, t) in goals {
            g[c] = t - params[comp.vars[c]];
        }
        let mut f = std::mem::take(&mut work.f);
        ctx.eqs
            .evaluate(comp, params, ctx.scale, &mut f, Some(&mut work.jac));
        work.f = f;
        comp.plan
            .fill(&work.jac, &dcol, &mut work.factor, &mut work.w, false);
        let max_diag = work
            .factor
            .adiag
            .iter()
            .fold(0.0, |m: f64, &d| m.max(d))
            .max(1e-300);
        // A tiny shift: the goals' weights make parts of the Gram matrix
        // small, and a larger shift would bend the projection.
        work.factor.factor(sym, 1e-15 * max_diag, 1e-14);
        work.step(comp, &dcol, Some(&g));
        // Limit the step so that the linearization stays meaningful.
        let len = max_abs(&work.dx);
        let limit = 0.25 * ctx.scale;
        let k = alpha * if len > limit { limit / len } else { 1.0 };
        if k * len <= ctx.tol {
            break;
        }
        // Reduction of the squared goal distance predicted for a projected
        // gradient step `s = k P g`: `2 g.s - |s|^2 = (2k - k^2) |P g|^2`.
        // (Written with `|P g|^2` only, it does not depend on the part of
        // `g` that the constraints cannot follow.)
        let pg2: f64 = goals.iter().map(|&(c, _)| work.dx[c] * work.dx[c]).sum();
        let pred = (2.0 * k - k * k) * pg2;
        for (i, &v) in comp.vars.iter().enumerate() {
            params[v] += k * work.dx[i];
        }
        clamp(ctx.eqs, params, &comp.vars);
        let run = lm(ctx, comp, params, work);
        iterations += run.iterations + 1;
        let d = goal_dist(params);
        // Sufficient decrease; an overshooting (oscillating) step is halved.
        if run.converged && good_dist * good_dist - d * d > 0.25 * pred {
            for (i, &v) in comp.vars.iter().enumerate() {
                good[i] = params[v];
            }
            good_dist = d;
            alpha = (2.0 * alpha).min(1.0);
        } else {
            for (i, &v) in comp.vars.iter().enumerate() {
                params[v] = good[i];
            }
            alpha *= 0.5;
            if alpha < 1e-3 {
                break;
            }
        }
    }
    ctx.eqs.evaluate(comp, params, ctx.scale, &mut work.f, None);
    let res = max_abs(&work.f);
    Run {
        converged: res <= ctx.tol,
        iterations,
        residual: res,
    }
}

impl System {
    pub(crate) fn ensure_prepared(&mut self) {
        if self.prepared.is_none() {
            self.prepared = Some(prepare(self));
        }
        let params = std::mem::take(&mut self.params);
        if let Some(p) = &mut self.prepared {
            p.refresh_fixed(&params);
        }
        self.params = params;
    }

    /// Solves all constraints, moving the geometry as little as possible.
    ///
    /// Parts of the sketch that are independent of each other are solved
    /// separately; a part that fails (contradiction or no solution) is left
    /// where it was, the others are solved.
    pub fn solve(&mut self, opts: &SolveOptions) -> SolveResult {
        self.run(opts, &[])
    }

    /// Moves the goals toward their targets as far as the constraints allow
    /// (constraints are hard, goals soft), moving other geometry as little
    /// as possible. Used while the user drags geometry: call it for each
    /// pointer position.
    pub fn drag(&mut self, goals: &[DragGoal], opts: &SolveOptions) -> SolveResult {
        self.run(opts, goals)
    }

    fn run(&mut self, opts: &SolveOptions, goals: &[DragGoal]) -> SolveResult {
        let classes = self.point_classes();
        let (dir_conflicts, _) = direction::check(self, &classes);
        if !dir_conflicts.is_empty() {
            return SolveResult {
                status: SolveStatus::Conflicting,
                iterations: 0,
                residual: self.residual(),
                conflicts: dir_conflicts,
                degenerate: Vec::new(),
            };
        }
        self.ensure_prepared();
        let mut prep = self.prepared.take().expect("prepared");
        let result = self.run_prepared(&mut prep, opts, goals);
        self.prepared = Some(prep);
        result
    }

    /// Goal unknowns and targets.
    fn goal_params(&self, goals: &[DragGoal]) -> Vec<(usize, f64)> {
        let mut out = Vec::new();
        for g in goals {
            match *g {
                DragGoal::Point { point, target } => {
                    if let Ok(p) = self.pidx(point) {
                        let d = self.pd(p);
                        out.push((d.x, target[0]));
                        out.push((d.y, target[1]));
                    }
                }
                DragGoal::Radius { entity, target } => {
                    if let Ok(EntityData::Circle { r, .. }) = self.ent(entity) {
                        out.push((*r, target));
                    }
                }
            }
        }
        out
    }

    fn run_prepared(
        &mut self,
        prep: &mut Prepared,
        opts: &SolveOptions,
        goals: &[DragGoal],
    ) -> SolveResult {
        let scale = self.scale();
        let tol = opts.tolerance * scale;
        let goal_params = self.goal_params(goals);
        // Goals on unknowns without equations move freely.
        for &(g, t) in &goal_params {
            if prep.free.contains(&g) {
                self.params[g] = t;
            }
        }
        // Continuation: large dimension changes are applied in steps.
        let mut changed: Vec<(usize, f64, f64)> = Vec::new();
        let mut steps = 1usize;
        for (i, rec) in self.live_constraints() {
            let Some(v) = rec.def.value() else { continue };
            let last = rec.last_value;
            if (v - last).abs() <= tol {
                continue;
            }
            changed.push((i, last, v));
            let n = if rec.def.is_angular() {
                ((v - last).abs() / (std::f64::consts::PI / 8.0)).ceil()
            } else {
                ((v - last).abs() / (0.3 * v.abs().max(last.abs()).max(1e-6 * scale))).ceil()
            };
            steps = steps.max(n as usize);
        }
        if !opts.continuation {
            steps = 1;
        }
        let steps = steps.min(16);
        let start = self.params.clone();
        let mut result = self.run_steps(prep, opts, scale, tol, &goal_params, &changed, steps);
        if result.status != SolveStatus::Converged && steps > 1 {
            // Retry in one step from the original geometry.
            self.params.clone_from(&start);
            result = self.run_steps(prep, opts, scale, tol, &goal_params, &changed, 1);
        }
        // Dimensions that now hold (also in the parts that did solve when
        // another part failed) are the start of the next continuation.
        for (i, _, v) in changed {
            let Some(rec) = self.constraints[i].as_ref() else {
                continue;
            };
            let limit = if rec.def.is_angular() {
                10.0 * opts.tolerance
            } else {
                10.0 * tol
            };
            let holds = self
                .measure_rec(rec)
                .is_some_and(|m| (m - v).abs() <= limit);
            if holds && let Some(rec) = self.constraints[i].as_mut() {
                rec.last_value = v;
            }
        }
        if result.status == SolveStatus::Converged {
            result.degenerate = self.degenerate(scale);
        }
        result
    }

    #[allow(clippy::too_many_arguments)]
    fn run_steps(
        &mut self,
        prep: &mut Prepared,
        opts: &SolveOptions,
        scale: f64,
        tol: f64,
        goal_params: &[(usize, f64)],
        changed: &[(usize, f64, f64)],
        steps: usize,
    ) -> SolveResult {
        let mut result = SolveResult {
            status: SolveStatus::Converged,
            iterations: 0,
            residual: 0.0,
            conflicts: Vec::new(),
            degenerate: Vec::new(),
        };
        let start = self.params.clone();
        for k in 1..=steps {
            let t = k as f64 / steps as f64;
            for &(c, from, to) in changed {
                prep.update_values(c, from + (to - from) * t);
            }
            result = self.solve_components(prep, opts, scale, tol, goal_params, &start);
            if result.status != SolveStatus::Converged {
                break;
            }
        }
        // Leave the prepared values at the targets.
        for &(c, _, to) in changed {
            prep.update_values(c, to);
        }
        result
    }

    fn solve_components(
        &mut self,
        prep: &Prepared,
        opts: &SolveOptions,
        scale: f64,
        tol: f64,
        goal_params: &[(usize, f64)],
        rollback: &[f64],
    ) -> SolveResult {
        let eqs = SketchEquations {
            prep,
            pkind: &self.pkind,
        };
        let ctx = Ctx {
            eqs: &eqs,
            scale,
            tol,
            max_iterations: opts.max_iterations,
        };
        let mut status = SolveStatus::Converged;
        let mut iterations = 0;
        let mut residual: f64 = 0.0;
        let mut conflicts = Vec::new();
        let mut params = std::mem::take(&mut self.params);
        for comp in &prep.comps {
            let goals: Vec<(usize, f64)> = goal_params
                .iter()
                .filter_map(|&(g, t)| comp.vars.binary_search(&g).ok().map(|c| (c, t)))
                .collect();
            let mut work = Work::new(comp);
            let run = if goals.is_empty() {
                lm(&ctx, comp, &mut params, &mut work)
            } else {
                drag(&ctx, comp, &mut params, &mut work, &goals)
            };
            iterations = iterations.max(run.iterations);
            residual = residual.max(run.residual);
            if !run.converged {
                // Explain the failure at the least-squares point, then put the
                // component back where it was.
                let deps =
                    analysis::component_dependencies(&eqs, comp, &params, scale, &mut work, false);
                let found: Vec<Dependency> =
                    analysis::to_dependencies(prep, comp, &deps.dependent, scale, true);
                if found.is_empty() {
                    if status == SolveStatus::Converged {
                        status = SolveStatus::NotConverged;
                    }
                } else {
                    status = SolveStatus::Conflicting;
                    conflicts.extend(found);
                }
                for &g in &comp.vars {
                    params[g] = rollback[g];
                }
            }
        }
        self.params = params;
        conflicts.sort_by_key(|d: &Dependency| d.constraint);
        SolveResult {
            status,
            iterations,
            residual,
            conflicts,
            degenerate: Vec::new(),
        }
    }

    /// Lines of (nearly) zero length and circles or arcs of zero radius.
    fn degenerate(&self, scale: f64) -> Vec<EntityId> {
        let eps = 1e-7 * scale;
        let mut out = Vec::new();
        for (i, r) in self.entities.iter().enumerate() {
            let Some(r) = r else { continue };
            let small = match r.data {
                EntityData::Line { p1, p2 } => self.xy(p2).sub(self.xy(p1)).norm() < eps,
                EntityData::Circle { r, .. } => self.params[r].abs() < eps,
                EntityData::Arc { c, s, .. } => self.xy(s).sub(self.xy(c)).norm() < eps,
                _ => false,
            };
            if small {
                out.push(EntityId(i as u32));
            }
        }
        out
    }

    /// Largest residual of the current geometry (millimetres).
    pub fn residual(&mut self) -> f64 {
        self.ensure_prepared();
        let prep = self.prepared.as_ref().expect("prepared");
        let scale = self.scale();
        let mut res: f64 = 0.0;
        for comp in &prep.comps {
            let mut f = vec![0.0; comp.eqs.len()];
            evaluate(prep, comp, &self.params, scale, &mut f, None);
            res = res.max(max_abs(&f));
        }
        res
    }
}
