// SPDX-License-Identifier: MIT
//! Sparse linear algebra for the Gram matrix `G = J D J^T` of the residual
//! Jacobian `J` (rows: equations, columns: variables, `D`: inverse variable
//! weights).
//!
//! `G` is symmetric positive semidefinite. It is factored as `L diag(d) L^T`
//! with a left-looking sparse algorithm in an elimination order that is either
//! chosen by a minimum degree heuristic (to keep fill small, for solving) or
//! given (equation creation order, for the rank analysis). Pivots that fall
//! below a relative tolerance are skipped: the row is then linearly dependent
//! on the rows eliminated before it, and its solution component is zero.
//! These are textbook methods (Cholesky factorization of a Gram matrix,
//! minimum degree ordering by graph elimination).

use std::cmp::Reverse;
use std::collections::BinaryHeap;

/// Sparse matrix in compressed row form.
#[derive(Clone, Debug, Default)]
pub(crate) struct Rows {
    pub ptr: Vec<usize>,
    pub col: Vec<usize>,
    pub val: Vec<f64>,
}

impl Rows {
    pub fn row(&self, i: usize) -> (&[usize], &[f64]) {
        let (s, e) = (self.ptr[i], self.ptr[i + 1]);
        (&self.col[s..e], &self.val[s..e])
    }
    pub fn n_rows(&self) -> usize {
        self.ptr.len().saturating_sub(1)
    }
}

/// Elimination order and the nonzero pattern of `L`.
#[derive(Clone, Debug)]
pub(crate) struct Symbolic {
    /// Original index to elimination position.
    pub pos: Vec<usize>,
    /// Column pointers of `L` (position numbering).
    pub lp: Vec<usize>,
    /// Row positions of the below-diagonal entries of each column, ascending.
    pub li: Vec<usize>,
    /// For each row position: the columns `k` that have an entry in this row,
    /// with the slot of that entry, ascending in `k`.
    pub rows: Vec<Vec<(usize, usize)>>,
}

impl Symbolic {
    /// Symbolic factorization in a minimum degree order.
    pub fn min_degree(adj: &[Vec<usize>]) -> Symbolic {
        eliminate(adj, None)
    }

    /// Symbolic factorization in the given order (`order[k]` is eliminated
    /// `k`th).
    pub fn with_order(adj: &[Vec<usize>], order: &[usize]) -> Symbolic {
        eliminate(adj, Some(order))
    }

    pub fn n(&self) -> usize {
        self.pos.len()
    }

    /// Slot of the entry for the original indices `a != b`.
    pub fn slot(&self, a: usize, b: usize) -> Option<usize> {
        let (pa, pb) = (self.pos[a], self.pos[b]);
        let (c, r) = if pa < pb { (pa, pb) } else { (pb, pa) };
        let s = self.lp[c];
        self.li[s..self.lp[c + 1]]
            .binary_search(&r)
            .ok()
            .map(|k| s + k)
    }

    pub fn nnz(&self) -> usize {
        self.li.len()
    }
}

/// Simulates Gaussian elimination on the graph `adj` (symmetric, without
/// self loops). The neighbours of a node when it is eliminated are the
/// pattern of its column in `L`.
fn eliminate(adj: &[Vec<usize>], order: Option<&[usize]>) -> Symbolic {
    let n = adj.len();
    let mut g: Vec<Vec<usize>> = adj.to_vec();
    let mut alive = vec![true; n];
    let mut mark = vec![usize::MAX; n];
    let mut pos = vec![usize::MAX; n];
    let mut structs: Vec<Vec<usize>> = Vec::with_capacity(n);
    let mut heap = BinaryHeap::new();
    if order.is_none() {
        for (v, nb) in g.iter().enumerate() {
            heap.push(Reverse((nb.len(), v)));
        }
    }
    for step in 0..n {
        let v = match order {
            Some(o) => o[step],
            None => loop {
                let Reverse((d, v)) = heap.pop().expect("heap holds every live node");
                if alive[v] && d == g[v].len() {
                    break v;
                }
            },
        };
        alive[v] = false;
        pos[v] = step;
        let nb = std::mem::take(&mut g[v]);
        for &a in &nb {
            let list = &mut g[a];
            list.retain(|&x| x != v);
            for &x in list.iter() {
                mark[x] = a;
            }
            for &x in &nb {
                if x != a && mark[x] != a {
                    list.push(x);
                    mark[x] = a;
                }
            }
            if order.is_none() {
                heap.push(Reverse((list.len(), a)));
            }
        }
        structs.push(nb);
    }
    let mut lp = Vec::with_capacity(n + 1);
    let mut li = Vec::new();
    lp.push(0);
    for nb in &structs {
        let start = li.len();
        li.extend(nb.iter().map(|&x| pos[x]));
        li[start..].sort_unstable();
        lp.push(li.len());
    }
    let mut rows = vec![Vec::new(); n];
    for k in 0..n {
        for (p, &r) in li.iter().enumerate().take(lp[k + 1]).skip(lp[k]) {
            rows[r].push((k, p));
        }
    }
    Symbolic { pos, lp, li, rows }
}

/// Numeric `L diag(d) L^T` factor. `ax` and `adiag` hold the matrix to be
/// factored (lower entries at their slots, diagonal by position).
#[derive(Clone, Debug)]
pub(crate) struct Factor {
    pub ax: Vec<f64>,
    pub adiag: Vec<f64>,
    pub lx: Vec<f64>,
    pub d: Vec<f64>,
    pub skipped: Vec<bool>,
    work: Vec<f64>,
}

impl Factor {
    pub fn new(sym: &Symbolic) -> Factor {
        let n = sym.n();
        Factor {
            ax: vec![0.0; sym.nnz()],
            adiag: vec![0.0; n],
            lx: vec![0.0; sym.nnz()],
            d: vec![0.0; n],
            skipped: vec![false; n],
            work: vec![0.0; n],
        }
    }

    /// Factors `A + shift I`. A pivot below `tol` times its original diagonal
    /// is skipped. Returns the number of skipped pivots.
    pub fn factor(&mut self, sym: &Symbolic, shift: f64, tol: f64) -> usize {
        let n = sym.n();
        let y = &mut self.work;
        let mut skipped = 0;
        for j in 0..n {
            let (s, e) = (sym.lp[j], sym.lp[j + 1]);
            for p in s..e {
                y[sym.li[p]] = self.ax[p];
            }
            let reference = self.adiag[j] + shift;
            let mut dj = reference;
            for &(k, pk) in &sym.rows[j] {
                let ljk = self.lx[pk];
                if ljk == 0.0 {
                    continue;
                }
                let f = ljk * self.d[k];
                dj -= ljk * f;
                for p in pk + 1..sym.lp[k + 1] {
                    y[sym.li[p]] -= self.lx[p] * f;
                }
            }
            if reference > 0.0 && dj > tol * reference {
                self.skipped[j] = false;
                self.d[j] = dj;
                let inv = 1.0 / dj;
                for p in s..e {
                    let r = sym.li[p];
                    self.lx[p] = y[r] * inv;
                    y[r] = 0.0;
                }
            } else {
                skipped += 1;
                self.skipped[j] = true;
                self.d[j] = 0.0;
                for p in s..e {
                    self.lx[p] = 0.0;
                    y[sym.li[p]] = 0.0;
                }
            }
        }
        skipped
    }

    /// Solves `L z = b` in place (position numbering).
    pub fn forward(&self, sym: &Symbolic, b: &mut [f64]) {
        for j in 0..sym.n() {
            let z = b[j];
            if z != 0.0 {
                for p in sym.lp[j]..sym.lp[j + 1] {
                    b[sym.li[p]] -= self.lx[p] * z;
                }
            }
        }
    }

    /// Solves `(L diag(d) L^T) x = b` in place (position numbering); skipped
    /// pivots give zero components.
    pub fn solve(&self, sym: &Symbolic, b: &mut [f64]) {
        self.forward(sym, b);
        for (j, x) in b.iter_mut().enumerate().take(sym.n()) {
            *x = if self.skipped[j] { 0.0 } else { *x / self.d[j] };
        }
        for j in (0..sym.n()).rev() {
            if self.skipped[j] {
                b[j] = 0.0;
                continue;
            }
            let mut s = b[j];
            for p in sym.lp[j]..sym.lp[j + 1] {
                s -= self.lx[p] * b[sym.li[p]];
            }
            b[j] = s;
        }
    }
}

/// Precomputed assembly of `G = J D J^T` into a factor.
#[derive(Clone, Debug)]
pub(crate) struct GramPlan {
    pub sym: Symbolic,
    /// For each row `i`: the rows `j` eliminated after `i` that share a
    /// column with it, and the slot of `G[i][j]`.
    pairs: Vec<Vec<(usize, usize)>>,
}

impl GramPlan {
    /// `row_cols[i]` lists the columns of row `i` (no duplicates).
    /// `order: None` chooses a minimum degree order.
    pub fn new(row_cols: &[Vec<usize>], n_cols: usize, order: Option<&[usize]>) -> GramPlan {
        let m = row_cols.len();
        let mut col_rows = vec![Vec::new(); n_cols];
        for (i, cols) in row_cols.iter().enumerate() {
            for &c in cols {
                col_rows[c].push(i);
            }
        }
        let mut adj: Vec<Vec<usize>> = vec![Vec::new(); m];
        let mut mark = vec![usize::MAX; m];
        for (i, cols) in row_cols.iter().enumerate() {
            mark[i] = i;
            for &c in cols {
                for &j in &col_rows[c] {
                    if mark[j] != i {
                        mark[j] = i;
                        adj[i].push(j);
                    }
                }
            }
        }
        let sym = match order {
            Some(o) => Symbolic::with_order(&adj, o),
            None => Symbolic::min_degree(&adj),
        };
        let mut pairs = vec![Vec::new(); m];
        for (i, nb) in adj.iter().enumerate() {
            for &j in nb {
                if sym.pos[j] > sym.pos[i] {
                    let slot = sym.slot(i, j).expect("Gram entry lies in the pattern");
                    pairs[i].push((j, slot));
                }
            }
        }
        GramPlan { sym, pairs }
    }

    /// Writes `G = J diag(dcol) J^T` into `f.ax` and `f.adiag`. With
    /// `normalize`, rows of `J` are scaled to unit length first (zero rows
    /// stay zero).
    pub fn fill(
        &self,
        jac: &Rows,
        dcol: &[f64],
        f: &mut Factor,
        w: &mut Vec<f64>,
        normalize: bool,
    ) {
        w.clear();
        w.resize(dcol.len(), 0.0);
        f.ax.iter_mut().for_each(|x| *x = 0.0);
        for i in 0..jac.n_rows() {
            let (ci, vi) = jac.row(i);
            let mut diag = 0.0;
            for (&c, &v) in ci.iter().zip(vi) {
                let wv = v * dcol[c];
                w[c] = wv;
                diag += v * wv;
            }
            f.adiag[self.sym.pos[i]] = diag;
            for &(j, slot) in &self.pairs[i] {
                let (cj, vj) = jac.row(j);
                let mut s = 0.0;
                for (&c, &v) in cj.iter().zip(vj) {
                    s += v * w[c];
                }
                f.ax[slot] = s;
            }
            for &c in ci {
                w[c] = 0.0;
            }
        }
        if normalize {
            let sym = &self.sym;
            for j in 0..sym.n() {
                for p in sym.lp[j]..sym.lp[j + 1] {
                    let den = (f.adiag[j] * f.adiag[sym.li[p]]).sqrt();
                    f.ax[p] = if den > 0.0 { f.ax[p] / den } else { 0.0 };
                }
            }
            for d in &mut f.adiag {
                *d = if *d > 0.0 { 1.0 } else { 0.0 };
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dense_from(row_cols: &[Vec<usize>], vals: &[Vec<f64>], n: usize) -> Vec<Vec<f64>> {
        let mut j = vec![vec![0.0; n]; row_cols.len()];
        for (i, cols) in row_cols.iter().enumerate() {
            for (k, &c) in cols.iter().enumerate() {
                j[i][c] = vals[i][k];
            }
        }
        j
    }

    fn rows_of(row_cols: &[Vec<usize>], vals: &[Vec<f64>]) -> Rows {
        let mut r = Rows {
            ptr: vec![0],
            ..Rows::default()
        };
        for (cols, v) in row_cols.iter().zip(vals) {
            r.col.extend(cols);
            r.val.extend(v);
            r.ptr.push(r.col.len());
        }
        r
    }

    /// Pseudo-random sparse system: solving G x = b must match a dense check.
    #[test]
    fn solves_random_spd_systems() {
        let mut seed = 12345u64;
        let mut rnd = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed % 10_000) as f64 / 10_000.0
        };
        for &(m, n) in &[(5usize, 9usize), (30, 40), (60, 60)] {
            let mut row_cols = Vec::new();
            let mut vals = Vec::new();
            for i in 0..m {
                let mut cols = vec![i % n, (i * 7 + 3) % n, (i * 13 + 5) % n];
                cols.sort_unstable();
                cols.dedup();
                vals.push(cols.iter().map(|_| rnd() - 0.5).collect::<Vec<_>>());
                row_cols.push(cols);
            }
            let jd = dense_from(&row_cols, &vals, n);
            let jac = rows_of(&row_cols, &vals);
            let dcol: Vec<f64> = (0..n).map(|_| 0.5 + rnd()).collect();
            for order in [None, Some((0..m).collect::<Vec<_>>())] {
                let plan = GramPlan::new(&row_cols, n, order.as_deref());
                let mut f = Factor::new(&plan.sym);
                let mut w = Vec::new();
                plan.fill(&jac, &dcol, &mut f, &mut w, false);
                let shift = 1e-3;
                f.factor(&plan.sym, shift, 1e-14);
                let b: Vec<f64> = (0..m).map(|_| rnd()).collect();
                let mut x = vec![0.0; m];
                for i in 0..m {
                    x[plan.sym.pos[i]] = b[i];
                }
                f.solve(&plan.sym, &mut x);
                // Residual of (J D J^T + shift I) x - b with dense products.
                for i in 0..m {
                    let mut s = shift * x[plan.sym.pos[i]];
                    for k in 0..m {
                        let g: f64 = (0..n).map(|c| jd[i][c] * dcol[c] * jd[k][c]).sum();
                        s += g * x[plan.sym.pos[k]];
                    }
                    assert!((s - b[i]).abs() < 1e-9, "m={m} row {i}: {s} vs {}", b[i]);
                }
            }
        }
    }

    #[test]
    fn detects_dependent_rows_in_order() {
        // Row 2 = row 0 + row 1; row 3 independent.
        let row_cols = vec![vec![0, 1], vec![1, 2], vec![0, 1, 2], vec![3]];
        let vals = vec![
            vec![1.0, 1.0],
            vec![2.0, -1.0],
            vec![1.0, 3.0, -1.0],
            vec![1.0],
        ];
        let jac = rows_of(&row_cols, &vals);
        let order: Vec<usize> = (0..4).collect();
        let plan = GramPlan::new(&row_cols, 4, Some(&order));
        let mut f = Factor::new(&plan.sym);
        let mut w = Vec::new();
        plan.fill(&jac, &[1.0; 4], &mut f, &mut w, true);
        assert_eq!(f.factor(&plan.sym, 0.0, 1e-10), 1);
        assert_eq!(f.skipped, vec![false, false, true, false]);
    }
}
