// SPDX-License-Identifier: MIT
//! Turns the points, entities and constraints of a [`System`] into residual
//! equations, splits them into independent components and precomputes the
//! sparse structure. The result is cached until the structure changes.

use std::collections::HashSet;
use std::sync::OnceLock;

use crate::equations::{CurvTerm, EqKind, P, Rad, SplineEq, TParam};
use crate::sparse::{GramPlan, Rows};
use crate::system::{ConstraintRec, EntityData, System};
use crate::types::{Constraint, EntityId, SplineEnd};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Owner {
    /// Implicit equation of an entity (arc radius, curve points).
    Entity(usize),
    Constraint(usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ValueSrc {
    Zero,
    /// The value of a dimension.
    Dim(usize),
    /// The unknown's value when a solve starts (fixed geometry).
    Fixed,
}

#[derive(Clone, Debug)]
pub(crate) struct Equation {
    pub owner: Owner,
    /// Order for the rank analysis: implicit equations first, then
    /// constraints in creation order.
    pub key: (u8, u64, u32),
    /// Global unknowns used, in local order.
    pub vars: Vec<usize>,
    pub kind: EqKind,
    pub value: f64,
    pub src: ValueSrc,
}

impl Equation {
    pub fn constraint(&self) -> Option<usize> {
        match self.owner {
            Owner::Constraint(c) => Some(c),
            Owner::Entity(_) => None,
        }
    }
}

/// Equations that share unknowns (directly or through others).
#[derive(Clone, Debug)]
pub(crate) struct Component {
    /// Equation indices, in analysis order.
    pub eqs: Vec<usize>,
    /// Global unknowns, ascending.
    pub vars: Vec<usize>,
    /// Jacobian pattern (values are overwritten by each evaluation).
    pub jac: Rows,
    /// Inverse weights of the unknowns.
    pub dcol: Vec<f64>,
    /// Gram matrix assembly in a minimum degree order.
    pub plan: GramPlan,
    /// Gram matrix assembly in equation order (rank analysis), built on
    /// first use.
    pub analysis_plan: OnceLock<GramPlan>,
}

#[derive(Clone, Debug)]
pub(crate) struct Prepared {
    pub eqs: Vec<Equation>,
    pub comps: Vec<Component>,
    /// Unknowns that no equation uses (each is one degree of freedom).
    pub free: Vec<usize>,
}

impl Prepared {
    pub fn update_values(&mut self, constraint: usize, value: f64) {
        for eq in &mut self.eqs {
            if eq.src == ValueSrc::Dim(constraint) {
                eq.value = value;
            }
        }
    }

    /// Fixed geometry is fixed where it is when the solve starts.
    pub fn refresh_fixed(&mut self, params: &[f64]) {
        for eq in &mut self.eqs {
            if eq.src == ValueSrc::Fixed
                && let EqKind::Fixed { a } = eq.kind
            {
                eq.value = params[eq.vars[a]];
            }
        }
    }
}

/// Local variable numbering of one equation.
#[derive(Default)]
struct Vars {
    list: Vec<usize>,
}

impl Vars {
    fn v(&mut self, g: usize) -> usize {
        match self.list.iter().position(|&x| x == g) {
            Some(i) => i,
            None => {
                self.list.push(g);
                self.list.len() - 1
            }
        }
    }
}

/// How a tangent constraint is formulated.
enum TanPlan {
    /// Line and circle/arc touching at a shared point.
    LineRoundAt {
        line: usize,
        round: usize,
        x: usize,
    },
    LineRound {
        line: usize,
        round: usize,
    },
    RoundsAt {
        a: usize,
        b: usize,
        x: usize,
    },
    Rounds {
        a: usize,
        b: usize,
    },
    /// G1 join at a spline end (`other` is a line, circle/arc or spline).
    SplineEnd {
        spline: usize,
        end: usize,
        other: usize,
        other_end: Option<usize>,
    },
    /// Contact inside the spline (with a contact parameter).
    SplineInterior {
        spline: usize,
        other: usize,
    },
}

struct Builder<'a> {
    sys: &'a System,
    eqs: Vec<Equation>,
    owner: Owner,
    key: (u8, u64),
    n: u32,
}

impl Builder<'_> {
    fn push(&mut self, src: ValueSrc, f: impl FnOnce(&mut Vars, &Ctx) -> EqKind) {
        let mut vars = Vars::default();
        let ctx = Ctx { sys: self.sys };
        let kind = f(&mut vars, &ctx);
        let value = match src {
            ValueSrc::Dim(c) => self.sys.constraints[c]
                .as_ref()
                .and_then(|r| r.def.value())
                .unwrap_or(0.0),
            _ => 0.0,
        };
        self.eqs.push(Equation {
            owner: self.owner,
            key: (self.key.0, self.key.1, self.n),
            vars: vars.list,
            kind,
            value,
            src,
        });
        self.n += 1;
    }
}

/// Access to the system while building local variables.
struct Ctx<'a> {
    sys: &'a System,
}

impl Ctx<'_> {
    fn p(&self, v: &mut Vars, point: usize) -> P {
        let d = self.sys.pd(point);
        [v.v(d.x), v.v(d.y)]
    }

    fn ent(&self, e: usize) -> &EntityData {
        &self.sys.entities[e].as_ref().expect("live entity").data
    }

    /// Radius of a circle or arc.
    fn rad(&self, v: &mut Vars, e: usize) -> Rad {
        match *self.ent(e) {
            EntityData::Circle { r, .. } => Rad::Param(v.v(r)),
            EntityData::Arc { c, s, .. } => Rad::Dist(self.p(v, c), self.p(v, s)),
            _ => unreachable!("validated as round"),
        }
    }

    fn line(&self, e: usize) -> (usize, usize) {
        match *self.ent(e) {
            EntityData::Line { p1, p2 } => (p1, p2),
            _ => unreachable!("validated as line"),
        }
    }

    fn centre(&self, e: usize) -> usize {
        self.ent(e).centre().expect("validated as round")
    }
}

pub(crate) fn prepare(sys: &System) -> Prepared {
    let classes = sys.point_classes();
    // Points known to lie on each entity: its end points and the points of
    // point-on-curve constraints (by coincidence class).
    let mut on: Vec<Vec<(usize, usize)>> = vec![Vec::new(); sys.entities.len()];
    for (i, r) in sys.entities.iter().enumerate() {
        if let Some(r) = r
            && let Some(ends) = r.data.ends()
        {
            for p in ends {
                on[i].push((classes[p], p));
            }
        }
    }
    for (_, c) in sys.live_constraints() {
        if let Constraint::PointOnCurve(p, e) = c.def {
            on[e.index()].push((classes[p.index()], p.index()));
        }
    }

    // Tangent formulations, and fitted spline ends controlled by tangents.
    let mut tangent_plans: Vec<Option<TanPlan>> = Vec::new();
    let mut controlled: HashSet<(usize, usize)> = HashSet::new();
    for (i, c) in sys.constraints.iter().enumerate() {
        let plan = match c {
            Some(ConstraintRec {
                def: Constraint::Tangent(a, b),
                ..
            }) => Some(tangent_plan(sys, &classes, &on, *a, *b)),
            Some(ConstraintRec {
                def: Constraint::Smooth(a, b),
                ..
            }) => Some(smooth_plan(sys, &classes, *a, *b)),
            _ => None,
        };
        if let Some(TanPlan::SplineEnd {
            spline,
            end,
            other,
            other_end,
        }) = &plan
        {
            controlled.insert((*spline, *end));
            if let Some(oe) = other_end {
                controlled.insert((*other, *oe));
            }
        }
        tangent_plans.push(plan);
        debug_assert_eq!(tangent_plans.len(), i + 1);
    }

    let mut b = Builder {
        sys,
        eqs: Vec::new(),
        owner: Owner::Entity(0),
        key: (0, 0),
        n: 0,
    };
    for (i, r) in sys.entities.iter().enumerate() {
        let Some(r) = r else { continue };
        b.owner = Owner::Entity(i);
        b.key = (0, r.seq);
        b.n = 0;
        entity_equations(&mut b, &r.data, i, &controlled);
    }
    for (i, r) in sys.live_constraints() {
        b.owner = Owner::Constraint(i);
        b.key = (1, r.seq);
        b.n = 0;
        constraint_equations(&mut b, r, i, &classes, tangent_plans[i].as_ref());
    }
    let mut eqs = b.eqs;
    eqs.sort_by_key(|e| e.key);

    let (comps, free) = components(sys, &eqs);
    Prepared { eqs, comps, free }
}

fn entity_equations(
    b: &mut Builder,
    data: &EntityData,
    idx: usize,
    controlled: &HashSet<(usize, usize)>,
) {
    match data {
        EntityData::Arc { c, s, e } => {
            let (c, s, e) = (*c, *s, *e);
            b.push(ValueSrc::Zero, |v, x| EqKind::RadDiff {
                a: Rad::Dist(x.p(v, c), x.p(v, e)),
                b: Rad::Dist(x.p(v, c), x.p(v, s)),
            });
        }
        EntityData::EllipticalArc {
            c,
            a,
            b: minor,
            s,
            e,
            ts,
            te,
        } => {
            for (p, t) in [(*s, *ts), (*e, *te)] {
                for axis in 0..2 {
                    b.push(ValueSrc::Zero, |v, x| EqKind::Ellipse {
                        p: x.p(v, p),
                        c: x.p(v, *c),
                        a: x.p(v, *a),
                        b: v.v(*minor),
                        t: v.v(t),
                        axis,
                    });
                }
            }
        }
        EntityData::Fitted {
            basis,
            fit,
            ctrl,
            params,
            ends,
        } => {
            let k = fit.len() - 1;
            let support_eq =
                |b: &mut Builder, t: f64, what: &dyn Fn(&mut Vars, &Ctx) -> SplineEq| {
                    let range = basis.support(t);
                    let first = *range.start();
                    b.push(ValueSrc::Zero, |v, x| EqKind::Spline {
                        basis: basis.clone(),
                        first,
                        ctrl: range.clone().map(|i| x.p(v, ctrl[i])).collect(),
                        t: TParam::Const(t),
                        what: what(v, x),
                    });
                };
            for j in 1..k {
                for axis in 0..2 {
                    let p = fit[j];
                    support_eq(b, params[j], &|v, x| SplineEq::Coord { p: x.p(v, p), axis });
                }
            }
            let (lo, hi) = basis.domain();
            for (end, t) in [(0, lo), (1, hi)] {
                if ends[end] == SplineEnd::Natural && !controlled.contains(&(idx, end)) {
                    for axis in 0..2 {
                        support_eq(b, t, &|_, _| SplineEq::Second { axis });
                    }
                }
            }
        }
        _ => {}
    }
}

fn tangent_plan(
    sys: &System,
    classes: &[usize],
    on: &[Vec<(usize, usize)>],
    a: EntityId,
    b: EntityId,
) -> TanPlan {
    let kind = |e: usize| match sys.entities[e].as_ref().expect("live").data {
        EntityData::Line { .. } => 0,
        EntityData::Circle { .. } | EntityData::Arc { .. } => 1,
        _ => 2,
    };
    let (mut lo, mut hi) = (a.index(), b.index());
    if kind(lo) > kind(hi) {
        std::mem::swap(&mut lo, &mut hi);
    }
    let common = |x: &[(usize, usize)], y: &[(usize, usize)]| {
        x.iter()
            .find(|(cx, _)| y.iter().any(|(cy, _)| cx == cy))
            .map(|&(_, p)| p)
    };
    let spline_ends = |e: usize| {
        sys.entities[e]
            .as_ref()
            .expect("live")
            .data
            .ends()
            .expect("open")
    };
    match (kind(lo), kind(hi)) {
        (0, 1) => match common(&on[lo], &on[hi]) {
            Some(x) => TanPlan::LineRoundAt {
                line: lo,
                round: hi,
                x,
            },
            None => TanPlan::LineRound {
                line: lo,
                round: hi,
            },
        },
        (1, 1) => match common(&on[lo], &on[hi]) {
            Some(x) => TanPlan::RoundsAt { a: lo, b: hi, x },
            None => TanPlan::Rounds { a: lo, b: hi },
        },
        (k, _) => {
            let ends = spline_ends(hi);
            if k == 2 {
                let other = spline_ends(lo);
                let mut best = (f64::INFINITY, 0, 0);
                for (i, &p) in ends.iter().enumerate() {
                    for (j, &q) in other.iter().enumerate() {
                        let d = if classes[p] == classes[q] {
                            -1.0
                        } else {
                            sys.xy(p).sub(sys.xy(q)).norm()
                        };
                        if d < best.0 {
                            best = (d, i, j);
                        }
                    }
                }
                return TanPlan::SplineEnd {
                    spline: hi,
                    end: best.1,
                    other: lo,
                    other_end: Some(best.2),
                };
            }
            for (end, &p) in ends.iter().enumerate() {
                if on[lo].iter().any(|&(c, _)| c == classes[p]) {
                    return TanPlan::SplineEnd {
                        spline: hi,
                        end,
                        other: lo,
                        other_end: None,
                    };
                }
            }
            TanPlan::SplineInterior {
                spline: hi,
                other: lo,
            }
        }
    }
}

/// The joined ends of a smooth (G2) constraint: the spline's end and the
/// other curve's end (nearest ends if they are not joined).
fn smooth_plan(sys: &System, classes: &[usize], a: EntityId, b: EntityId) -> TanPlan {
    let data = |e: usize| &sys.entities[e].as_ref().expect("live").data;
    let (spline, other) = if data(a.index()).spline().is_some() {
        (a.index(), b.index())
    } else {
        (b.index(), a.index())
    };
    let (es, eo) = (
        data(spline).ends().expect("open"),
        data(other).ends().expect("open"),
    );
    let mut best = (f64::INFINITY, 0, 0);
    for (i, &p) in es.iter().enumerate() {
        for (j, &q) in eo.iter().enumerate() {
            let d = if classes[p] == classes[q] {
                -1.0
            } else {
                sys.xy(p).sub(sys.xy(q)).norm()
            };
            if d < best.0 {
                best = (d, i, j);
            }
        }
    }
    TanPlan::SplineEnd {
        spline,
        end: best.1,
        other,
        other_end: Some(best.2),
    }
}

/// Signed curvature at an end, parametrized from the end into the curve.
fn curvature_term(v: &mut Vars, x: &Ctx, e: usize, end: usize) -> CurvTerm {
    let sign = if end == 0 { 1.0 } else { -1.0 };
    match x.ent(e) {
        EntityData::Arc { c, s, .. } => CurvTerm::Arc {
            c: x.p(v, *c),
            s: x.p(v, *s),
            sign,
        },
        EntityData::BSpline { basis, ctrl } | EntityData::Fitted { basis, ctrl, .. } => {
            let (lo, hi) = basis.domain();
            CurvTerm::Spline {
                basis: basis.clone(),
                ctrl: ctrl.iter().map(|&p| x.p(v, p)).collect(),
                t: if end == 0 { lo } else { hi },
                sign,
            }
        }
        _ => CurvTerm::Zero,
    }
}

/// End point and the neighbouring control point of a spline end.
fn spline_leg(data: &EntityData, end: usize) -> (usize, usize) {
    let (_, ctrl) = data.spline().expect("spline");
    let n = ctrl.len() - 1;
    if end == 0 {
        (ctrl[0], ctrl[1])
    } else {
        (ctrl[n], ctrl[n - 1])
    }
}

fn tangent_equations(b: &mut Builder, rec: &ConstraintRec, plan: &TanPlan) {
    let z = ValueSrc::Zero;
    match *plan {
        TanPlan::LineRoundAt {
            line,
            round,
            x: contact,
        } => b.push(z, |v, x| {
            let (p1, p2) = x.line(line);
            EqKind::TanLineAt {
                a: x.p(v, p1),
                b: x.p(v, p2),
                e: x.p(v, contact),
                c: x.p(v, x.centre(round)),
            }
        }),
        TanPlan::LineRound { line, round } => b.push(z, |v, x| {
            let (p1, p2) = x.line(line);
            EqKind::TanLineCircle {
                a: x.p(v, p1),
                b: x.p(v, p2),
                c: x.p(v, x.centre(round)),
                r: x.rad(v, round),
                side: rec.side,
            }
        }),
        TanPlan::RoundsAt {
            a,
            b: bb,
            x: contact,
        } => b.push(z, |v, x| EqKind::TanCirclesAt {
            e: x.p(v, contact),
            c1: x.p(v, x.centre(a)),
            c2: x.p(v, x.centre(bb)),
        }),
        TanPlan::Rounds { a, b: bb } => b.push(z, |v, x| EqKind::TanCircles {
            c1: x.p(v, x.centre(a)),
            r1: x.rad(v, a),
            c2: x.p(v, x.centre(bb)),
            r2: x.rad(v, bb),
            internal: rec.side,
        }),
        TanPlan::SplineEnd {
            spline,
            end,
            other,
            other_end,
        } => b.push(z, |v, x| {
            // The leg points from the end into the curve; only its direction
            // up to sign matters for lines, and for circles the radius at the
            // end point is perpendicular to it.
            let (e0, e1) = spline_leg(x.ent(spline), end);
            let (l0, l1) = (x.p(v, e0), x.p(v, e1));
            match x.ent(other) {
                EntityData::Line { p1, p2 } => EqKind::DirParallel {
                    a1: x.p(v, *p1),
                    a2: x.p(v, *p2),
                    b1: l0,
                    b2: l1,
                },
                EntityData::Circle { .. } | EntityData::Arc { .. } => EqKind::DirPerp {
                    a1: x.p(v, x.centre(other)),
                    a2: l0,
                    b1: l0,
                    b2: l1,
                },
                _ => {
                    let (f0, f1) = spline_leg(x.ent(other), other_end.unwrap_or(0));
                    EqKind::DirParallel {
                        a1: x.p(v, f0),
                        a2: x.p(v, f1),
                        b1: l0,
                        b2: l1,
                    }
                }
            }
        }),
        TanPlan::SplineInterior { spline, other } => {
            let t = rec.params[0];
            for tangent in [false, true] {
                b.push(z, |v, x| {
                    let (basis, ctrl) = x.ent(spline).spline().expect("spline");
                    let ctrl_l: Vec<P> = ctrl.iter().map(|&p| x.p(v, p)).collect();
                    let what = match x.ent(other) {
                        EntityData::Line { p1, p2 } => {
                            let (a, bb) = (x.p(v, *p1), x.p(v, *p2));
                            if tangent {
                                SplineEq::TanLine { a, b: bb }
                            } else {
                                SplineEq::OnLine { a, b: bb }
                            }
                        }
                        _ => {
                            let c = x.p(v, x.centre(other));
                            if tangent {
                                SplineEq::TanCircle { c }
                            } else {
                                SplineEq::OnCircle {
                                    c,
                                    r: x.rad(v, other),
                                }
                            }
                        }
                    };
                    EqKind::Spline {
                        basis: basis.clone(),
                        first: 0,
                        ctrl: ctrl_l,
                        t: TParam::Var(v.v(t)),
                        what,
                    }
                });
            }
        }
    }
}

fn constraint_equations(
    b: &mut Builder,
    rec: &ConstraintRec,
    idx: usize,
    classes: &[usize],
    tangent: Option<&TanPlan>,
) {
    use Constraint::*;
    let z = ValueSrc::Zero;
    let dim = ValueSrc::Dim(idx);
    // `p1 - p2` per axis.
    let diff = |b: &mut Builder, p: usize, q: usize, axis: usize, src: ValueSrc, side: f64| {
        b.push(src, |v, x| {
            let (pp, qq) = (x.p(v, p), x.p(v, q));
            EqKind::Diff {
                a: pp[axis],
                b: qq[axis],
                side,
            }
        })
    };
    let sym = |b: &mut Builder, p: usize, q: usize, axis: usize| {
        b.push(z, |v, x| {
            let (a1, a2) = x.line(axis);
            EqKind::SymMid {
                p: x.p(v, p),
                q: x.p(v, q),
                a: x.p(v, a1),
                b: x.p(v, a2),
            }
        });
        b.push(z, |v, x| {
            let (a1, a2) = x.line(axis);
            EqKind::SymPerp {
                p: x.p(v, p),
                q: x.p(v, q),
                a: x.p(v, a1),
                b: x.p(v, a2),
            }
        });
    };
    let fixed = |b: &mut Builder, g: usize| {
        b.push(ValueSrc::Fixed, |v, _| EqKind::Fixed { a: v.v(g) });
    };
    let sys = b.sys;
    let ent = |e: EntityId| &sys.entities[e.index()].as_ref().expect("live").data;
    match rec.def {
        Coincident(p, q) => {
            for axis in 0..2 {
                diff(b, p.index(), q.index(), axis, z, 1.0);
            }
        }
        PointOnCurve(p, e) => {
            let p = p.index();
            match ent(e).clone() {
                EntityData::Line { p1, p2 } => b.push(z, |v, x| EqKind::PointLine {
                    p: x.p(v, p),
                    a: x.p(v, p1),
                    b: x.p(v, p2),
                    side: 1.0,
                }),
                EntityData::Circle { c, .. } | EntityData::Arc { c, .. } => {
                    b.push(z, |v, x| EqKind::OnCircle {
                        p: x.p(v, p),
                        c: x.p(v, c),
                        r: x.rad(v, e.index()),
                    })
                }
                EntityData::Ellipse { c, a, b: minor }
                | EntityData::EllipticalArc { c, a, b: minor, .. } => {
                    for axis in 0..2 {
                        b.push(z, |v, x| EqKind::Ellipse {
                            p: x.p(v, p),
                            c: x.p(v, c),
                            a: x.p(v, a),
                            b: v.v(minor),
                            t: v.v(rec.params[0]),
                            axis,
                        });
                    }
                }
                EntityData::BSpline { basis, ctrl } | EntityData::Fitted { basis, ctrl, .. } => {
                    for axis in 0..2 {
                        b.push(z, |v, x| EqKind::Spline {
                            basis: basis.clone(),
                            first: 0,
                            ctrl: ctrl.iter().map(|&q| x.p(v, q)).collect(),
                            t: TParam::Var(v.v(rec.params[0])),
                            what: SplineEq::Coord { p: x.p(v, p), axis },
                        });
                    }
                }
            }
        }
        Horizontal(l) | Vertical(l) => {
            let axis = if matches!(rec.def, Horizontal(_)) {
                1
            } else {
                0
            };
            let EntityData::Line { p1, p2 } = *ent(l) else {
                return;
            };
            diff(b, p2, p1, axis, z, 1.0);
        }
        HorizontalPoints(p, q) => diff(b, q.index(), p.index(), 1, z, 1.0),
        VerticalPoints(p, q) => diff(b, q.index(), p.index(), 0, z, 1.0),
        Parallel(l1, l2) | Perpendicular(l1, l2) => {
            let par = matches!(rec.def, Parallel(..));
            b.push(z, |v, x| {
                let (a1, a2) = x.line(l1.index());
                let (b1, b2) = x.line(l2.index());
                let (a1, a2, b1, b2) = (x.p(v, a1), x.p(v, a2), x.p(v, b1), x.p(v, b2));
                if par {
                    EqKind::Parallel { a1, a2, b1, b2 }
                } else {
                    EqKind::Perpendicular { a1, a2, b1, b2 }
                }
            });
        }
        Tangent(..) => {
            if let Some(plan) = tangent {
                tangent_equations(b, rec, plan);
            }
        }
        Smooth(..) => {
            if let Some(
                plan @ TanPlan::SplineEnd {
                    spline,
                    end,
                    other,
                    other_end,
                },
            ) = tangent
            {
                tangent_equations(b, rec, plan);
                let (spline, end, other) = (*spline, *end, *other);
                let other_end = other_end.unwrap_or(0);
                b.push(z, |v, x| EqKind::Curvature {
                    a: curvature_term(v, x, spline, end),
                    b: curvature_term(v, x, other, other_end),
                });
            }
        }
        Equal(e1, e2) => {
            if matches!(ent(e1), EntityData::Line { .. }) {
                b.push(z, |v, x| {
                    let (a1, a2) = x.line(e1.index());
                    let (b1, b2) = x.line(e2.index());
                    EqKind::EqualLength {
                        a1: x.p(v, a1),
                        a2: x.p(v, a2),
                        b1: x.p(v, b1),
                        b2: x.p(v, b2),
                    }
                });
            } else {
                b.push(z, |v, x| EqKind::RadDiff {
                    a: x.rad(v, e1.index()),
                    b: x.rad(v, e2.index()),
                });
            }
        }
        FixPoint(p) => {
            let d = b.sys.pd(p.index());
            fixed(b, d.x);
            fixed(b, d.y);
        }
        FixEntity(e) => {
            let data = ent(e).clone();
            // Only as many equations as the entity has degrees of freedom, so
            // that fixing it is never reported as redundant.
            match data {
                EntityData::Arc { c, s, e: end } => {
                    for p in [c, s] {
                        let d = sys.pd(p);
                        fixed(b, d.x);
                        fixed(b, d.y);
                    }
                    // The end point is on the circle: fix the coordinate that
                    // moves most along it.
                    let r = sys.xy(end).sub(sys.xy(c));
                    let d = sys.pd(end);
                    fixed(b, if r.y.abs() >= r.x.abs() { d.x } else { d.y });
                    return;
                }
                EntityData::EllipticalArc {
                    c,
                    a,
                    b: minor,
                    ts,
                    te,
                    ..
                } => {
                    for p in [c, a] {
                        let d = sys.pd(p);
                        fixed(b, d.x);
                        fixed(b, d.y);
                    }
                    for g in [minor, ts, te] {
                        fixed(b, g);
                    }
                    return;
                }
                _ => {}
            }
            let mut pts = match &data {
                EntityData::Fitted { fit, .. } => fit.clone(),
                _ => data.points(),
            };
            if let EntityData::Fitted { ctrl, ends, .. } = &data {
                // Free ends: their tangent handles are fixed too.
                if ends[0] == SplineEnd::Free {
                    pts.push(ctrl[1]);
                }
                if ends[1] == SplineEnd::Free && ctrl.len() > 3 {
                    pts.push(ctrl[ctrl.len() - 2]);
                }
            }
            for p in pts {
                let d = b.sys.pd(p);
                fixed(b, d.x);
                fixed(b, d.y);
            }
            for s in data.scalars() {
                fixed(b, s);
            }
        }
        Midpoint(p, e) => {
            let p = p.index();
            match *ent(e) {
                EntityData::Line { p1, p2 } => {
                    for axis in 0..2 {
                        b.push(z, |v, x| EqKind::Mid {
                            m: x.p(v, p)[axis],
                            a: x.p(v, p1)[axis],
                            b: x.p(v, p2)[axis],
                        });
                    }
                }
                EntityData::Arc { c, s, e } => {
                    for axis in 0..2 {
                        b.push(z, |v, x| EqKind::ArcMid {
                            m: x.p(v, p),
                            c: x.p(v, c),
                            s: x.p(v, s),
                            e: x.p(v, e),
                            axis,
                        });
                    }
                }
                _ => {}
            }
        }
        Concentric(e1, e2) => {
            let (c1, c2) = (
                ent(e1).centre().expect("round"),
                ent(e2).centre().expect("round"),
            );
            for axis in 0..2 {
                diff(b, c1, c2, axis, z, 1.0);
            }
        }
        Collinear(l1, l2) => {
            let EntityData::Line { p1: a1, p2: a2 } = *ent(l1) else {
                return;
            };
            let EntityData::Line { p1: q1, p2: q2 } = *ent(l2) else {
                return;
            };
            for q in [q1, q2] {
                if classes[q] == classes[a1] || classes[q] == classes[a2] {
                    continue;
                }
                b.push(z, |v, x| EqKind::PointLine {
                    p: x.p(v, q),
                    a: x.p(v, a1),
                    b: x.p(v, a2),
                    side: 1.0,
                });
            }
        }
        SymmetricPoints { a, b: q, axis } => sym(b, a.index(), q.index(), axis.index()),
        SymmetricEntities { a, b: e2, axis } => {
            let ax = axis.index();
            match (ent(a).clone(), ent(e2).clone()) {
                (EntityData::Line { p1, p2 }, EntityData::Line { p1: q1, p2: q2 }) => {
                    let (m1, m2) = if rec.swap { (q2, q1) } else { (q1, q2) };
                    sym(b, p1, m1, ax);
                    sym(b, p2, m2, ax);
                }
                (EntityData::Circle { c: c1, .. }, EntityData::Circle { c: c2, .. }) => {
                    sym(b, c1, c2, ax);
                    b.push(z, |v, x| EqKind::RadDiff {
                        a: x.rad(v, a.index()),
                        b: x.rad(v, e2.index()),
                    });
                }
                (
                    EntityData::Arc {
                        c: c1,
                        s: s1,
                        e: f1,
                    },
                    EntityData::Arc {
                        c: c2,
                        s: s2,
                        e: f2,
                    },
                ) => {
                    // Mirroring reverses the direction: start <-> end.
                    sym(b, c1, c2, ax);
                    sym(b, f1, s2, ax);
                    // The end of `b` is on its circle already; its direction
                    // from the centre mirrors that of the start of `a`.
                    b.push(z, |v, x| {
                        let (a1, a2) = x.line(ax);
                        EqKind::MirrorDir {
                            c: x.p(v, c2),
                            e: x.p(v, f2),
                            rc: x.p(v, c1),
                            rp: x.p(v, s1),
                            a: x.p(v, a1),
                            b: x.p(v, a2),
                        }
                    });
                }
                _ => {}
            }
        }
        Distance { a, b: q, .. } => b.push(dim, |v, x| EqKind::Distance {
            p: x.p(v, a.index()),
            q: x.p(v, q.index()),
        }),
        PointLineDistance { point, line, .. } => b.push(dim, |v, x| {
            let (a1, a2) = x.line(line.index());
            EqKind::PointLine {
                p: x.p(v, point.index()),
                a: x.p(v, a1),
                b: x.p(v, a2),
                side: rec.side,
            }
        }),
        LineDistance { a, b: l2, .. } => b.push(dim, |v, x| {
            let (a1, a2) = x.line(a.index());
            let (b1, b2) = x.line(l2.index());
            EqKind::LineLine {
                a1: x.p(v, a1),
                a2: x.p(v, a2),
                b1: x.p(v, b1),
                b2: x.p(v, b2),
                side: rec.side,
            }
        }),
        HorizontalDistance { a, b: q, .. } => diff(b, q.index(), a.index(), 0, dim, rec.side),
        VerticalDistance { a, b: q, .. } => diff(b, q.index(), a.index(), 1, dim, rec.side),
        Length { line, .. } => b.push(dim, |v, x| {
            let (a1, a2) = x.line(line.index());
            EqKind::Distance {
                p: x.p(v, a1),
                q: x.p(v, a2),
            }
        }),
        Angle { a, b: l2, .. } => b.push(dim, |v, x| {
            let (a1, a2) = x.line(a.index());
            let (b1, b2) = x.line(l2.index());
            EqKind::Angle {
                a1: x.p(v, a1),
                a2: x.p(v, a2),
                b1: x.p(v, b1),
                b2: x.p(v, b2),
                flip: rec.flip,
                sigma: rec.side,
            }
        }),
        Radius { entity, .. } | Diameter { entity, .. } => {
            let factor = if matches!(rec.def, Diameter { .. }) {
                2.0
            } else {
                1.0
            };
            b.push(dim, |v, x| EqKind::Radius {
                r: x.rad(v, entity.index()),
                factor,
            });
        }
        ArcLength { arc, .. } => {
            let EntityData::Arc { c, s, e } = *ent(arc) else {
                return;
            };
            b.push(dim, |v, x| EqKind::ArcLen {
                c: x.p(v, c),
                s: x.p(v, s),
                e: x.p(v, e),
            });
        }
        MajorRadius { ellipse, .. } => {
            let (EntityData::Ellipse { c, a, .. } | EntityData::EllipticalArc { c, a, .. }) =
                *ent(ellipse)
            else {
                return;
            };
            b.push(dim, |v, x| EqKind::Distance {
                p: x.p(v, c),
                q: x.p(v, a),
            });
        }
        MinorRadius { ellipse, .. } => {
            let (EntityData::Ellipse { b: minor, .. } | EntityData::EllipticalArc { b: minor, .. }) =
                *ent(ellipse)
            else {
                return;
            };
            b.push(dim, |v, _| EqKind::Radius {
                r: Rad::Param(v.v(minor)),
                factor: 1.0,
            });
        }
        Translated { a, b: q, by } => {
            for axis in 0..2 {
                b.push(z, |v, x| {
                    let (pa, pq) = (x.p(v, a.index()), x.p(v, q.index()));
                    EqKind::Shift {
                        a: pa[axis],
                        b: pq[axis],
                        by: by[axis],
                    }
                });
            }
        }
        Rotated {
            center,
            a,
            b: q,
            angle,
        } => {
            let (sin, cos) = angle.sin_cos();
            for axis in 0..2 {
                b.push(z, |v, x| EqKind::Turned {
                    c: x.p(v, center.index()),
                    p: x.p(v, a.index()),
                    q: x.p(v, q.index()),
                    cos,
                    sin,
                    axis,
                });
            }
        }
        TurnedDirection {
            a_center,
            a,
            b_center,
            b: q,
            angle,
        } => {
            let (sin, cos) = angle.sin_cos();
            b.push(z, |v, x| EqKind::TurnedDir {
                ca: x.p(v, a_center.index()),
                a: x.p(v, a.index()),
                cb: x.p(v, b_center.index()),
                b: x.p(v, q.index()),
                cos,
                sin,
            });
        }
        EqualSize(e1, e2) => {
            let size = |e: EntityId| match *ent(e) {
                EntityData::Circle { r, .. } => r,
                EntityData::Ellipse { b: minor, .. }
                | EntityData::EllipticalArc { b: minor, .. } => minor,
                _ => unreachable!("validated as a circle or an ellipse"),
            };
            let (s1, s2) = (size(e1), size(e2));
            b.push(z, |v, _| EqKind::RadDiff {
                a: Rad::Param(v.v(s1)),
                b: Rad::Param(v.v(s2)),
            });
        }
    }
}

/// Splits the equations into components connected by shared unknowns.
fn components(sys: &System, eqs: &[Equation]) -> (Vec<Component>, Vec<usize>) {
    let n = sys.params.len();
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    let mut used = vec![false; n];
    for eq in eqs {
        for &g in &eq.vars {
            used[g] = true;
        }
        if let Some(&first) = eq.vars.first() {
            let r0 = find(&mut parent, first);
            for &g in &eq.vars[1..] {
                let r = find(&mut parent, g);
                if r != r0 {
                    parent[r] = r0;
                }
            }
        }
    }
    let mut comp_of_root = vec![usize::MAX; n];
    let mut comps_eqs: Vec<Vec<usize>> = Vec::new();
    for (i, eq) in eqs.iter().enumerate() {
        let Some(&first) = eq.vars.first() else {
            continue;
        };
        let r = find(&mut parent, first);
        if comp_of_root[r] == usize::MAX {
            comp_of_root[r] = comps_eqs.len();
            comps_eqs.push(Vec::new());
        }
        comps_eqs[comp_of_root[r]].push(i);
    }
    let mut comps_vars: Vec<Vec<usize>> = vec![Vec::new(); comps_eqs.len()];
    for (g, &u) in used.iter().enumerate() {
        if u {
            let r = find(&mut parent, g);
            comps_vars[comp_of_root[r]].push(g);
        }
    }
    let mut col = vec![usize::MAX; n];
    let mut comps = Vec::with_capacity(comps_eqs.len());
    for (ce, cv) in comps_eqs.into_iter().zip(comps_vars) {
        for (k, &g) in cv.iter().enumerate() {
            col[g] = k;
        }
        let row_cols: Vec<Vec<usize>> = ce
            .iter()
            .map(|&i| eqs[i].vars.iter().map(|&g| col[g]).collect())
            .collect();
        let mut jac = Rows {
            ptr: vec![0],
            ..Rows::default()
        };
        for cols in &row_cols {
            jac.col.extend(cols);
            jac.ptr.push(jac.col.len());
        }
        jac.val = vec![0.0; jac.col.len()];
        let plan = GramPlan::new(&row_cols, cv.len(), None);
        let dcol = cv.iter().map(|&g| 1.0 / sys.weights[g]).collect();
        comps.push(Component {
            eqs: ce,
            vars: cv,
            jac,
            dcol,
            plan,
            analysis_plan: OnceLock::new(),
        });
    }
    // Free unknowns: point coordinates and entity scalars without equations.
    let mut free = Vec::new();
    for p in sys.points.iter().flatten() {
        for g in [p.x, p.y] {
            if !used[g] {
                free.push(g);
            }
        }
    }
    for r in sys.entities.iter().flatten() {
        for g in r.data.scalars() {
            if !used[g] {
                free.push(g);
            }
        }
    }
    (comps, free)
}
