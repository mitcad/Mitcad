// SPDX-License-Identifier: MIT
//! Exact check of the direction constraints (horizontal, vertical, parallel,
//! perpendicular, collinear, angle) on a graph of line directions.
//!
//! These constraints relate line angles modulo pi: `theta_a - theta_b = c`.
//! A cycle whose offsets do not add up to zero contradicts itself even when
//! the polynomial equations have a degenerate solution (a horizontal and
//! vertical line can collapse to a point), so contradictions are found here
//! and not only by the rank analysis. Union-find with offsets detects the
//! cycles; the cycle's constraints are reported.

use std::collections::{HashMap, VecDeque};
use std::f64::consts::{FRAC_PI_2, PI};

use crate::system::{EntityData, System};
use crate::types::{Constraint, ConstraintId, Dependency, EntityId};

const ANGLE_TOL: f64 = 1e-8;

struct Graph {
    parent: Vec<usize>,
    /// `theta_i - theta_parent`.
    off: Vec<f64>,
    /// Spanning forest: neighbour, `theta_self - theta_neighbour`, constraint.
    adj: Vec<Vec<(usize, usize)>>,
}

impl Graph {
    fn node(&mut self) -> usize {
        self.parent.push(self.parent.len());
        self.off.push(0.0);
        self.adj.push(Vec::new());
        self.parent.len() - 1
    }

    fn find(&self, mut i: usize) -> (usize, f64) {
        let mut phi = 0.0;
        while self.parent[i] != i {
            phi += self.off[i];
            i = self.parent[i];
        }
        (i, phi)
    }

    /// Constraints on the tree path between `a` and `b`.
    fn path(&self, a: usize, b: usize) -> Vec<usize> {
        let mut prev: HashMap<usize, (usize, usize)> = HashMap::new();
        let mut queue = VecDeque::from([a]);
        prev.insert(a, (a, usize::MAX));
        while let Some(n) = queue.pop_front() {
            if n == b {
                break;
            }
            for &(m, c) in &self.adj[n] {
                if let std::collections::hash_map::Entry::Vacant(e) = prev.entry(m) {
                    e.insert((n, c));
                    queue.push_back(m);
                }
            }
        }
        let mut out = Vec::new();
        let mut n = b;
        while n != a {
            let Some(&(p, c)) = prev.get(&n) else { break };
            out.push(c);
            n = p;
        }
        out
    }
}

/// Reduces an angle modulo pi to (-pi/2, pi/2].
fn wrap_pi(x: f64) -> f64 {
    x - PI * (x / PI).round()
}

/// Returns the contradicting and the redundant direction constraints.
pub(crate) fn check(sys: &System, classes: &[usize]) -> (Vec<Dependency>, Vec<Dependency>) {
    let mut g = Graph {
        parent: Vec::new(),
        off: Vec::new(),
        adj: Vec::new(),
    };
    let x_axis = g.node();
    let mut nodes: HashMap<(usize, usize), usize> = HashMap::new();
    let mut node_of = |g: &mut Graph, p: usize, q: usize| -> Option<usize> {
        let (a, b) = (classes[p], classes[q]);
        if a == b {
            return None;
        }
        let key = (a.min(b), a.max(b));
        Some(*nodes.entry(key).or_insert_with(|| g.node()))
    };
    let line = |e: EntityId| match sys.entities[e.index()].as_ref().map(|r| &r.data) {
        Some(EntityData::Line { p1, p2 }) => Some((*p1, *p2)),
        _ => None,
    };
    let mut conflicts = Vec::new();
    let mut redundant = Vec::new();
    for (cid, rec) in sys.live_constraints() {
        use Constraint::*;
        // Edge theta_a - theta_b = c.
        let (pa, pb, c) = match rec.def {
            Horizontal(l) | Vertical(l) => {
                let Some((p, q)) = line(l) else { continue };
                let c = if matches!(rec.def, Horizontal(_)) {
                    0.0
                } else {
                    FRAC_PI_2
                };
                ((p, q), None, c)
            }
            HorizontalPoints(p, q) => ((p.index(), q.index()), None, 0.0),
            VerticalPoints(p, q) => ((p.index(), q.index()), None, FRAC_PI_2),
            Parallel(a, b) | Perpendicular(a, b) | Collinear(a, b) => {
                let (Some(la), Some(lb)) = (line(a), line(b)) else {
                    continue;
                };
                let c = if matches!(rec.def, Perpendicular(..)) {
                    FRAC_PI_2
                } else {
                    0.0
                };
                (la, Some(lb), c)
            }
            Angle { a, b, value } => {
                let (Some(la), Some(lb)) = (line(a), line(b)) else {
                    continue;
                };
                (lb, Some(la), rec.side * value)
            }
            _ => continue,
        };
        let Some(na) = node_of(&mut g, pa.0, pa.1) else {
            continue;
        };
        let nb = match pb {
            Some(l) => match node_of(&mut g, l.0, l.1) {
                Some(n) => n,
                None => continue,
            },
            None => x_axis,
        };
        let ((ra, fa), (rb, fb)) = (g.find(na), g.find(nb));
        if ra != rb {
            g.parent[ra] = rb;
            g.off[ra] = c + fb - fa;
            g.adj[na].push((nb, cid));
            g.adj[nb].push((na, cid));
            continue;
        }
        let mut involved: Vec<ConstraintId> = g
            .path(na, nb)
            .into_iter()
            .map(|c| ConstraintId(c as u32))
            .collect();
        involved.push(ConstraintId(cid as u32));
        involved.sort_unstable();
        involved.dedup();
        let dep = Dependency {
            constraint: ConstraintId(cid as u32),
            involved,
        };
        if wrap_pi(fa - fb - c).abs() < ANGLE_TOL {
            redundant.push(dep);
        } else {
            conflicts.push(dep);
        }
    }
    (conflicts, redundant)
}
