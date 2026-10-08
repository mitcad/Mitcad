// SPDX-License-Identifier: MIT
//! Rigid bodies joined by joints: the joint solver of assemblies
//! (mitcad#55).
//!
//! A body is what moves as one (an occurrence with those rigid groups join
//! to it). Its unknowns are a small motion about where it is: a
//! translation `t` and a rotation vector `w` turning about the body's
//! centre `c` (the mean of its joint frames' origins),
//! `X = T(c + t) · R(w) · T(-c) · X0`. Fixed bodies have none. A joint
//! relates a frame on body `a` to a frame on body `b` (either side may be
//! the fixed world) and holds when
//!
//! `frame_a = frame_b · M(values) · alignment`
//!
//! where `M` composes the joint's free motions (slides along and turns
//! about frame `b`'s axes, in the order given) at their values. The values
//! are unknowns too, kept within their limits. Each joint gives six rows:
//! the difference of the frames' origins (mm) and the skew part of the
//! rotation between them (scaled by the system's size). A motion with a
//! target value adds the row `value - target`.
//!
//! The rows are solved with the sketch solver's Levenberg-Marquardt
//! iteration (`solve.rs`: minimum-norm steps in a weighted metric, so the
//! bodies move as little as possible and the values take what they can),
//! its sparse LDLᵀ factor of the Gram matrix and its forward-mode
//! derivatives (`scalar.rs`). Its rank analysis (`analysis.rs`) gives the
//! degrees of freedom and the joints that over-constrain: the rows are in
//! the order the joints were added, so the newest joint of a dependent set
//! is the one reported.
//!
//! Before iterating, [`RigidSystem::solve`] places what needs no
//! iteration: in joint order, a joint between a side that is fixed (or
//! already joined to something fixed) and one that is not moves the free
//! side's bodies, with every body joined to it so far, so that the joint
//! holds at its start values (its targets, else the values nearest to
//! where the bodies are); between two free sides `a`'s side moves. Only
//! joints that close a loop are left to the iteration.
//!
//! The skew part of a rotation also vanishes at half a turn; a solve that
//! ends there turns the joint's body over and iterates again.

use std::sync::OnceLock;

use crate::analysis::{self, CONFLICT_TOL, DepRow};
use crate::prepare::Component;
use crate::scalar::{Dual, LANES, Scalar};
use crate::solve::{self, Ctx, Equations, Run, Work};
use crate::sparse::{GramPlan, Rows};
use crate::types::SolveStatus;

pub type Vec3 = [f64; 3];
type M3<T> = [[T; 3]; 3];

const I3: M3<f64> = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
/// Joint values cost this much less than moving a body by as much: a step
/// changes values before it moves bodies.
const VALUE_WEIGHT: f64 = 100.0;
/// Attempts of a solve that ends half a turn off.
const ATTEMPTS: usize = 4;

fn add(a: Vec3, b: Vec3) -> Vec3 {
    std::array::from_fn(|i| a[i] + b[i])
}

fn sub(a: Vec3, b: Vec3) -> Vec3 {
    std::array::from_fn(|i| a[i] - b[i])
}

fn norm(a: Vec3) -> f64 {
    a.iter().map(|v| v * v).sum::<f64>().sqrt()
}

fn mat_vec<T: Scalar>(m: &M3<T>, v: [T; 3]) -> [T; 3] {
    std::array::from_fn(|r| m[r][0] * v[0] + m[r][1] * v[1] + m[r][2] * v[2])
}

fn mat_mul<T: Scalar>(a: &M3<T>, b: &M3<T>) -> M3<T> {
    std::array::from_fn(|r| {
        std::array::from_fn(|c| a[r][0] * b[0][c] + a[r][1] * b[1][c] + a[r][2] * b[2][c])
    })
}

/// `a · bᵀ`.
fn mat_mul_t<T: Scalar>(a: &M3<T>, b: &M3<T>) -> M3<T> {
    std::array::from_fn(|r| {
        std::array::from_fn(|c| a[r][0] * b[c][0] + a[r][1] * b[c][1] + a[r][2] * b[c][2])
    })
}

fn transpose(m: &M3<f64>) -> M3<f64> {
    std::array::from_fn(|r| std::array::from_fn(|c| m[c][r]))
}

/// The rotation by the rotation vector `w` (Rodrigues' formula; its
/// series near zero keeps the derivatives exact there).
fn rodrigues<T: Scalar>(w: [T; 3]) -> M3<T> {
    let th2 = w[0] * w[0] + w[1] * w[1] + w[2] * w[2];
    let one = T::cst(1.0);
    let (a, b) = if th2.val() < 1e-6 {
        let th4 = th2 * th2;
        (
            one - th2.mulf(1.0 / 6.0) + th4.mulf(1.0 / 120.0),
            T::cst(0.5) - th2.mulf(1.0 / 24.0) + th4.mulf(1.0 / 720.0),
        )
    } else {
        let th = th2.sqrt();
        (th.sin() / th, (one - th.cos()) / th2)
    };
    let k: M3<T> = [
        [T::cst(0.0), -w[2], w[1]],
        [w[2], T::cst(0.0), -w[0]],
        [-w[1], w[0], T::cst(0.0)],
    ];
    std::array::from_fn(|r| {
        std::array::from_fn(|c| {
            let diagonal = if r == c { one - b * th2 } else { T::cst(0.0) };
            diagonal + a * k[r][c] + b * w[r] * w[c]
        })
    })
}

/// A rigid placement: a point `p` goes to `rotation · p + translation`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    pub rotation: [[f64; 3]; 3],
    pub translation: Vec3,
}

impl Pose {
    pub const IDENTITY: Pose = Pose {
        rotation: I3,
        translation: [0.0; 3],
    };

    pub fn translation(t: Vec3) -> Pose {
        Pose {
            rotation: I3,
            translation: t,
        }
    }

    /// A turn by `angle` (radians, right-handed) about `axis` through
    /// `centre`; the identity for a zero axis.
    pub fn rotation(centre: Vec3, axis: Vec3, angle: f64) -> Pose {
        let n = norm(axis);
        if n == 0.0 {
            return Self::IDENTITY;
        }
        let r = rodrigues(axis.map(|a| a * angle / n));
        Pose {
            rotation: r,
            translation: sub(centre, mat_vec(&r, centre)),
        }
    }

    /// `self · other`: `other` first.
    pub fn after(&self, other: &Pose) -> Pose {
        Pose {
            rotation: mat_mul(&self.rotation, &other.rotation),
            translation: add(mat_vec(&self.rotation, other.translation), self.translation),
        }
    }

    pub fn inverse(&self) -> Pose {
        let r = transpose(&self.rotation);
        Pose {
            rotation: r,
            translation: mat_vec(&r, self.translation).map(|v| -v),
        }
    }

    pub fn apply_point(&self, p: Vec3) -> Vec3 {
        add(mat_vec(&self.rotation, p), self.translation)
    }

    pub fn apply_vector(&self, v: Vec3) -> Vec3 {
        mat_vec(&self.rotation, v)
    }

    /// The largest difference of the rotations' entries and of the
    /// translations.
    pub fn distance(&self, other: &Pose) -> (f64, f64) {
        let mut rotation: f64 = 0.0;
        for r in 0..3 {
            for c in 0..3 {
                rotation = rotation.max((self.rotation[r][c] - other.rotation[r][c]).abs());
            }
        }
        (norm(sub(self.translation, other.translation)), rotation)
    }
}

/// One free motion of a joint in frame `b`: a slide along an axis (mm) or a
/// turn about it (radians, right-handed).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Freedom {
    Tx,
    Ty,
    Tz,
    Rx,
    Ry,
    Rz,
}

impl Freedom {
    pub const ALL: [Freedom; 6] = [
        Freedom::Tx,
        Freedom::Ty,
        Freedom::Tz,
        Freedom::Rx,
        Freedom::Ry,
        Freedom::Rz,
    ];

    pub fn is_rotation(self) -> bool {
        matches!(self, Freedom::Rx | Freedom::Ry | Freedom::Rz)
    }

    /// 0, 1 or 2 for x, y or z.
    pub fn axis(self) -> usize {
        match self {
            Freedom::Tx | Freedom::Rx => 0,
            Freedom::Ty | Freedom::Ry => 1,
            Freedom::Tz | Freedom::Rz => 2,
        }
    }

    /// This motion alone at `value`.
    pub fn pose(self, value: f64) -> Pose {
        PoseT::<f64>::cst(&Pose::IDENTITY)
            .motion(self, value)
            .pose()
    }
}

/// The transform of the motions at their values, composed in order.
pub fn motion_pose(motions: &[Freedom], values: &[f64]) -> Pose {
    motions
        .iter()
        .zip(values)
        .fold(PoseT::<f64>::cst(&Pose::IDENTITY), |p, (m, v)| {
            p.motion(*m, *v)
        })
        .pose()
}

/// A pose of scalars (values with derivatives).
#[derive(Clone, Copy)]
struct PoseT<T> {
    r: M3<T>,
    t: [T; 3],
}

impl<T: Scalar> PoseT<T> {
    fn cst(p: &Pose) -> Self {
        PoseT {
            r: p.rotation.map(|row| row.map(T::cst)),
            t: p.translation.map(T::cst),
        }
    }

    fn after(&self, o: &Self) -> Self {
        let moved = mat_vec(&self.r, o.t);
        PoseT {
            r: mat_mul(&self.r, &o.r),
            t: std::array::from_fn(|i| moved[i] + self.t[i]),
        }
    }

    /// `self · motion(value)`.
    fn motion(mut self, m: Freedom, v: T) -> Self {
        let a = m.axis();
        if m.is_rotation() {
            let (c, s) = (v.cos(), v.sin());
            let (zero, one) = (T::cst(0.0), T::cst(1.0));
            let turn: M3<T> = match a {
                0 => [[one, zero, zero], [zero, c, -s], [zero, s, c]],
                1 => [[c, zero, s], [zero, one, zero], [-s, zero, c]],
                _ => [[c, -s, zero], [s, c, zero], [zero, zero, one]],
            };
            self.r = mat_mul(&self.r, &turn);
        } else {
            for i in 0..3 {
                self.t[i] = self.t[i] + self.r[i][a] * v;
            }
        }
        self
    }
}

impl PoseT<f64> {
    fn pose(&self) -> Pose {
        Pose {
            rotation: self.r,
            translation: self.t,
        }
    }
}

/// A body of a [`RigidSystem`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BodyId(pub u32);

/// A joint of a [`RigidSystem`], numbered in the order they were added.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct JointId(pub u32);

/// One free motion of a joint: its limits and, when it is driven, the
/// value it is held at.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct JointMotion {
    pub freedom: Freedom,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub target: Option<f64>,
}

impl JointMotion {
    pub fn free(freedom: Freedom) -> Self {
        JointMotion {
            freedom,
            min: None,
            max: None,
            target: None,
        }
    }
}

/// A joint: `frame_a = frame_b · M(values) · alignment`. Frames are given
/// where the bodies are when the joint is added (in the fixed world's
/// coordinates); a side without a body is fixed.
#[derive(Clone, Debug, PartialEq)]
pub struct RigidJoint {
    pub a: Option<BodyId>,
    pub frame_a: Pose,
    pub b: Option<BodyId>,
    pub frame_b: Pose,
    /// The free motions in the order they compose.
    pub motions: Vec<JointMotion>,
    pub alignment: Pose,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RigidError {
    NoBody(BodyId),
    NoJoint(JointId),
    /// The joint has no such free motion.
    NoMotion(JointId, Freedom),
    /// Both sides are the same body (or both fixed).
    SameSides,
    /// A free motion is listed twice.
    RepeatedMotion(Freedom),
    /// Limits in the wrong order, or a number that is not finite.
    BadValue(Freedom),
}

impl std::fmt::Display for RigidError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RigidError::NoBody(b) => write!(f, "no body {}", b.0),
            RigidError::NoJoint(j) => write!(f, "no joint {}", j.0),
            RigidError::NoMotion(j, m) => write!(f, "joint {} has no free motion {m:?}", j.0),
            RigidError::SameSides => f.write_str("both sides of the joint are the same body"),
            RigidError::RepeatedMotion(m) => write!(f, "the motion {m:?} is listed twice"),
            RigidError::BadValue(m) => write!(f, "the limits or target of {m:?} are invalid"),
        }
    }
}

impl std::error::Error for RigidError {}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RigidOptions {
    /// Every residual (mm, turns scaled by the system's size) must be below
    /// `tolerance` times the size.
    pub tolerance: f64,
    /// Iteration limit of one Levenberg-Marquardt run.
    pub max_iterations: usize,
    /// Place what needs no iteration first (see the module documentation).
    pub place: bool,
}

impl Default for RigidOptions {
    fn default() -> Self {
        RigidOptions {
            tolerance: 1e-10,
            max_iterations: 100,
            place: true,
        }
    }
}

/// A row that depends on earlier ones: a joint's frame rows, or the
/// target row of one of its motions, with the joints of the dependent set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RigidDependency {
    pub joint: JointId,
    /// The motion whose target is the dependent row; None for the joint's
    /// frames.
    pub target: Option<Freedom>,
    /// Every joint of the set, `joint` included, ascending.
    pub involved: Vec<JointId>,
    /// The targets in the set (driven motions), ascending.
    pub targets: Vec<(JointId, Freedom)>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RigidResult {
    pub status: SolveStatus,
    pub iterations: usize,
    /// The largest residual (mm).
    pub residual: f64,
    /// Over-constraining rows when the joints contradict each other.
    pub conflicts: Vec<RigidDependency>,
    /// Free motions held at one of their limits.
    pub at_limits: Vec<(JointId, Freedom)>,
}

impl RigidResult {
    pub fn is_ok(&self) -> bool {
        self.status == SolveStatus::Converged
    }
}

/// Degrees of freedom of the joints (their targets left out: the
/// motions the joints allow) where the bodies are.
#[derive(Clone, Debug, PartialEq)]
pub struct RigidAnalysis {
    /// Of all bodies together.
    pub dof: usize,
    /// Of each body with the others held where they are; 0 when fixed.
    pub body_dof: Vec<usize>,
    /// Joints that take away motions earlier joints already took
    /// (consistent: they hold).
    pub redundant: Vec<RigidDependency>,
    /// Joints that contradict earlier ones where the bodies are.
    pub conflicts: Vec<RigidDependency>,
}

#[derive(Clone, Debug)]
struct BodyRec {
    fixed: bool,
    pose: Pose,
}

#[derive(Clone, Debug)]
struct JointRec {
    def: RigidJoint,
    values: Vec<f64>,
}

/// Rigid bodies and the joints between them.
#[derive(Clone, Debug, Default)]
pub struct RigidSystem {
    bodies: Vec<BodyRec>,
    joints: Vec<JointRec>,
}

/// Which rows a run solves.
#[derive(Clone, Copy, PartialEq)]
enum Mode {
    /// Joints and targets.
    Solve,
    /// Joints only (drags, degrees of freedom).
    Joints,
}

/// The unknowns of a run: each free body's six, then each joint's values.
struct Layout {
    body_col: Vec<Option<usize>>,
    value_col: Vec<usize>,
    n: usize,
}

/// A joint's frame rows.
struct FrameRows {
    /// Frame a where the run starts and the centre of a's body when it
    /// moves.
    ga: Pose,
    ca: Option<Vec3>,
    gb: Pose,
    cb: Option<Vec3>,
    motions: Vec<Freedom>,
    alignment: Pose,
}

enum BlockKind {
    Frames(Box<FrameRows>),
    Target { value: f64, rotation: bool },
}

/// Rows of one joint (its frames, or one target), evaluated together.
struct Block {
    joint: usize,
    target: Option<Freedom>,
    row: usize,
    rows: usize,
    vars: Vec<usize>,
    kind: BlockKind,
}

struct RigidEquations {
    blocks: Vec<Block>,
    bounds: Vec<Option<(f64, f64)>>,
    rows: usize,
}

/// A side's frame at the run's unknowns: `x` starts at the side's body
/// unknowns when it moves; returns the unknowns used.
fn side_frame<T: Scalar>(g: &Pose, c: Option<Vec3>, x: &[T]) -> (PoseT<T>, usize) {
    let Some(c) = c else {
        return (PoseT::cst(g), 0);
    };
    let r = rodrigues([x[3], x[4], x[5]]);
    let arm = mat_vec(&r, sub(g.translation, c).map(T::cst));
    let frame = PoseT {
        r: mat_mul(&r, &g.rotation.map(|row| row.map(T::cst))),
        t: std::array::from_fn(|i| arm[i] + T::cst(c[i]) + x[i]),
    };
    (frame, 6)
}

/// A joint's six residuals: frame `a` less where frame `b`, the motions
/// and the alignment put it (origins; the rotation's skew part times
/// `scale`).
fn frame_rows<T: Scalar>(rows: &FrameRows, x: &[T], scale: f64) -> [T; 6] {
    let FrameRows {
        ga,
        ca,
        gb,
        cb,
        motions,
        alignment,
    } = rows;
    let (fa, used_a) = side_frame(ga, *ca, x);
    let (fb, used_b) = side_frame(gb, *cb, &x[used_a..]);
    let values = &x[used_a + used_b..];
    let q = motions
        .iter()
        .zip(values)
        .fold(fb, |p, (m, v)| p.motion(*m, *v))
        .after(&PoseT::cst(alignment));
    let e = mat_mul_t(&fa.r, &q.r);
    let half = 0.5 * scale;
    [
        fa.t[0] - q.t[0],
        fa.t[1] - q.t[1],
        fa.t[2] - q.t[2],
        (e[2][1] - e[1][2]).mulf(half),
        (e[0][2] - e[2][0]).mulf(half),
        (e[1][0] - e[0][1]).mulf(half),
    ]
}

impl Equations for RigidEquations {
    fn evaluate(
        &self,
        _comp: &Component,
        params: &[f64],
        scale: f64,
        f: &mut [f64],
        mut jac: Option<&mut Rows>,
    ) {
        for block in &self.blocks {
            match &block.kind {
                BlockKind::Target { value, rotation } => {
                    let k = if *rotation { scale } else { 1.0 };
                    f[block.row] = (params[block.vars[0]] - value) * k;
                    if let Some(jac) = jac.as_deref_mut() {
                        jac.val[jac.ptr[block.row]] = k;
                    }
                }
                BlockKind::Frames(rows) => match jac.as_deref_mut() {
                    None => {
                        let x: Vec<f64> = block.vars.iter().map(|&g| params[g]).collect();
                        let r = frame_rows(rows, &x, scale);
                        f[block.row..block.row + 6].copy_from_slice(&r);
                    }
                    Some(jac) => {
                        let n = block.vars.len();
                        let mut start = 0;
                        loop {
                            let x: Vec<Dual> = block
                                .vars
                                .iter()
                                .enumerate()
                                .map(|(i, &g)| {
                                    let lane = (i >= start && i < start + LANES).then(|| i - start);
                                    Dual::var(params[g], lane)
                                })
                                .collect();
                            let r = frame_rows(rows, &x, scale);
                            for (k, v) in r.iter().enumerate() {
                                let row = block.row + k;
                                f[row] = v.v;
                                let base = jac.ptr[row];
                                for i in start..(start + LANES).min(n) {
                                    jac.val[base + i] = v.d[i - start];
                                }
                            }
                            start += LANES;
                            if start >= n {
                                break;
                            }
                        }
                    }
                },
            }
        }
    }

    fn bounds(&self, var: usize) -> Option<(f64, f64)> {
        self.bounds[var]
    }
}

impl RigidEquations {
    /// The rows' pattern and Gram matrix plans, with the unknowns'
    /// inverse weights.
    fn component(&self, n: usize, dcol: Vec<f64>) -> Component {
        let mut jac = Rows {
            ptr: vec![0],
            ..Rows::default()
        };
        let mut row_cols = Vec::with_capacity(self.rows);
        for block in &self.blocks {
            for _ in 0..block.rows {
                jac.col.extend(&block.vars);
                jac.val.extend(block.vars.iter().map(|_| 0.0));
                jac.ptr.push(jac.col.len());
                row_cols.push(block.vars.clone());
            }
        }
        Component {
            eqs: (0..self.rows).collect(),
            vars: (0..n).collect(),
            plan: GramPlan::new(&row_cols, n, None),
            jac,
            dcol,
            analysis_plan: OnceLock::new(),
        }
    }

    fn block_of(&self, row: usize) -> &Block {
        self.blocks
            .iter()
            .find(|b| row >= b.row && row < b.row + b.rows)
            .expect("every row is in a block")
    }

    /// Dependent rows as joints' dependencies: inconsistent ones with
    /// `conflicting`, else the consistent (redundant) ones.
    fn dependencies(&self, rows: &[DepRow], scale: f64, conflicting: bool) -> Vec<RigidDependency> {
        let mut out: Vec<RigidDependency> = Vec::new();
        for d in rows {
            if (d.inconsistency.abs() > CONFLICT_TOL * scale) != conflicting {
                continue;
            }
            let block = self.block_of(d.row);
            let blocks: Vec<&Block> = d
                .coeffs
                .iter()
                .map(|&(r, _)| self.block_of(r))
                .chain(std::iter::once(block))
                .collect();
            let involved: Vec<JointId> = blocks.iter().map(|b| JointId(b.joint as u32)).collect();
            let targets: Vec<(JointId, Freedom)> = blocks
                .iter()
                .filter_map(|b| Some((JointId(b.joint as u32), b.target?)))
                .collect();
            let joint = JointId(block.joint as u32);
            match out
                .iter_mut()
                .find(|o| o.joint == joint && o.target == block.target)
            {
                Some(o) => {
                    o.involved.extend(involved);
                    o.targets.extend(targets);
                }
                None => out.push(RigidDependency {
                    joint,
                    target: block.target,
                    involved,
                    targets,
                }),
            }
        }
        for o in &mut out {
            o.involved.sort_unstable();
            o.involved.dedup();
            o.targets.sort_unstable();
            o.targets.dedup();
        }
        out.sort_by_key(|o| (o.joint, o.target));
        out
    }
}

/// The values of `motions` nearest to `relative` (closed forms for slides
/// before turns, one turn, or turns about z, y and x; other turns start at
/// zero).
fn project(motions: &[Freedom], relative: &Pose) -> Vec<f64> {
    let r = &relative.rotation;
    let turns: Vec<Freedom> = motions
        .iter()
        .copied()
        .filter(|m| m.is_rotation())
        .collect();
    let euler = turns == [Freedom::Rz, Freedom::Ry, Freedom::Rx];
    let (ex, ey, ez) = if euler {
        // R = Rz(c) · Ry(b) · Rx(a).
        let ry = (-r[2][0]).clamp(-1.0, 1.0).asin();
        if r[2][0].abs() < 1.0 - 1e-12 {
            (r[2][1].atan2(r[2][2]), ry, r[1][0].atan2(r[0][0]))
        } else {
            (0.0, ry, (-r[0][1]).atan2(r[1][1]))
        }
    } else {
        (0.0, 0.0, 0.0)
    };
    motions
        .iter()
        .map(|m| match m {
            Freedom::Tx | Freedom::Ty | Freedom::Tz => relative.translation[m.axis()],
            _ if euler => match m {
                Freedom::Rx => ex,
                Freedom::Ry => ey,
                _ => ez,
            },
            _ if turns.len() == 1 => match m {
                Freedom::Rx => r[2][1].atan2(r[1][1]),
                Freedom::Ry => r[0][2].atan2(r[2][2]),
                _ => r[1][0].atan2(r[0][0]),
            },
            _ => 0.0,
        })
        .collect()
}

/// `angle` plus whole turns, nearest to `reference`.
fn unwrap_turn(angle: f64, reference: f64) -> f64 {
    let tau = std::f64::consts::TAU;
    angle + ((reference - angle) / tau).round() * tau
}

impl RigidSystem {
    pub fn new() -> Self {
        Self::default()
    }

    /// A body where it is (its pose is the identity: the change from
    /// here); a fixed body never moves.
    pub fn add_body(&mut self, fixed: bool) -> BodyId {
        self.bodies.push(BodyRec {
            fixed,
            pose: Pose::IDENTITY,
        });
        BodyId(self.bodies.len() as u32 - 1)
    }

    pub fn add_joint(&mut self, joint: RigidJoint) -> Result<JointId, RigidError> {
        for b in [joint.a, joint.b].into_iter().flatten() {
            if b.0 as usize >= self.bodies.len() {
                return Err(RigidError::NoBody(b));
            }
        }
        if joint.a == joint.b {
            return Err(RigidError::SameSides);
        }
        for (i, m) in joint.motions.iter().enumerate() {
            if joint.motions[..i].iter().any(|o| o.freedom == m.freedom) {
                return Err(RigidError::RepeatedMotion(m.freedom));
            }
            let numbers = [m.min, m.max, m.target];
            let finite = numbers.iter().flatten().all(|v| v.is_finite());
            let ordered = match (m.min, m.max) {
                (Some(lo), Some(hi)) => lo <= hi,
                _ => true,
            };
            if !finite || !ordered {
                return Err(RigidError::BadValue(m.freedom));
            }
        }
        let values = vec![0.0; joint.motions.len()];
        self.joints.push(JointRec { def: joint, values });
        let id = JointId(self.joints.len() as u32 - 1);
        self.joints[id.0 as usize].values = self.start_values(id.0 as usize);
        Ok(id)
    }

    /// Replaces the limits and target of one of a joint's motions (the one
    /// with `motion.freedom`).
    pub fn set_motion(&mut self, joint: JointId, motion: JointMotion) -> Result<(), RigidError> {
        let Some(rec) = self.joints.get_mut(joint.0 as usize) else {
            return Err(RigidError::NoJoint(joint));
        };
        let finite = [motion.min, motion.max, motion.target]
            .iter()
            .flatten()
            .all(|v| v.is_finite());
        let ordered = match (motion.min, motion.max) {
            (Some(lo), Some(hi)) => lo <= hi,
            _ => true,
        };
        if !finite || !ordered {
            return Err(RigidError::BadValue(motion.freedom));
        }
        match rec
            .def
            .motions
            .iter_mut()
            .find(|m| m.freedom == motion.freedom)
        {
            Some(m) => {
                *m = motion;
                Ok(())
            }
            None => Err(RigidError::NoMotion(joint, motion.freedom)),
        }
    }

    /// The body's motion since it was added (`pose · placement` is where it
    /// is now).
    pub fn body_pose(&self, body: BodyId) -> Pose {
        self.bodies[body.0 as usize].pose
    }

    /// The joint's motion values, in its motions' order.
    pub fn joint_values(&self, joint: JointId) -> &[f64] {
        &self.joints[joint.0 as usize].values
    }

    /// Frame `a` and where frame `b`, the values and the alignment put it.
    fn frames(&self, j: usize) -> (Pose, Pose) {
        let joint = &self.joints[j];
        let (fa, fb) = self.side_frames(j);
        let motions: Vec<Freedom> = joint.def.motions.iter().map(|m| m.freedom).collect();
        let q = fb
            .after(&motion_pose(&motions, &joint.values))
            .after(&joint.def.alignment);
        (fa, q)
    }

    /// The two frames where the bodies are now.
    fn side_frames(&self, j: usize) -> (Pose, Pose) {
        let def = &self.joints[j].def;
        let at = |body: Option<BodyId>, frame: &Pose| match body {
            Some(b) => self.bodies[b.0 as usize].pose.after(frame),
            None => *frame,
        };
        (at(def.a, &def.frame_a), at(def.b, &def.frame_b))
    }

    /// How far the joint is from holding at its values: the distance of
    /// the frames' origins (mm) and the angle between them (radians).
    pub fn joint_error(&self, joint: JointId) -> (f64, f64) {
        let (fa, q) = self.frames(joint.0 as usize);
        let e = mat_mul_t(&fa.rotation, &q.rotation);
        let cos = (e[0][0] + e[1][1] + e[2][2] - 1.0) / 2.0;
        let sin = 0.5 * norm([e[2][1] - e[1][2], e[0][2] - e[2][0], e[1][0] - e[0][1]]);
        (norm(sub(fa.translation, q.translation)), sin.atan2(cos))
    }

    /// The values a joint starts from: targets, else the values nearest to
    /// where the bodies are, turns unwrapped near the middle of their
    /// limits (or their previous value) and kept within the limits.
    fn start_values(&self, j: usize) -> Vec<f64> {
        let joint = &self.joints[j];
        let (fa, fb) = self.side_frames(j);
        let relative = fb
            .inverse()
            .after(&fa)
            .after(&joint.def.alignment.inverse());
        let freedoms: Vec<Freedom> = joint.def.motions.iter().map(|m| m.freedom).collect();
        let raw = project(&freedoms, &relative);
        joint
            .def
            .motions
            .iter()
            .enumerate()
            .map(|(k, m)| {
                if let Some(target) = m.target {
                    return target;
                }
                let mut v = raw[k];
                if m.freedom.is_rotation() {
                    let reference = match (m.min, m.max) {
                        (Some(lo), Some(hi)) => 0.5 * (lo + hi),
                        (Some(lo), None) => lo,
                        (None, Some(hi)) => hi,
                        (None, None) => joint.values[k],
                    };
                    v = unwrap_turn(v, reference);
                }
                if let Some(lo) = m.min {
                    v = v.max(lo);
                }
                if let Some(hi) = m.max {
                    v = v.min(hi);
                }
                v
            })
            .collect()
    }

    /// The system's size: the diagonal of the box around the joints'
    /// frames, at least 1 mm.
    fn size(&self, extra: &[Vec3]) -> f64 {
        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        let mut points = extra.to_vec();
        for j in 0..self.joints.len() {
            let (fa, fb) = self.side_frames(j);
            points.extend([fa.translation, fb.translation]);
        }
        for p in &points {
            for i in 0..3 {
                lo[i] = lo[i].min(p[i]);
                hi[i] = hi[i].max(p[i]);
            }
        }
        if points.is_empty() {
            return 1.0;
        }
        norm(sub(hi, lo)).max(1.0)
    }

    fn layout(&self) -> Layout {
        let mut n = 0;
        let body_col = self
            .bodies
            .iter()
            .map(|b| {
                (!b.fixed).then(|| {
                    n += 6;
                    n - 6
                })
            })
            .collect();
        let value_col = self
            .joints
            .iter()
            .map(|j| {
                n += j.values.len();
                n - j.values.len()
            })
            .collect();
        Layout {
            body_col,
            value_col,
            n,
        }
    }

    /// Each free body's centre: the mean of its joint frames' origins.
    fn centres(&self) -> Vec<Vec3> {
        let mut sum = vec![[0.0; 3]; self.bodies.len()];
        let mut count = vec![0usize; self.bodies.len()];
        for j in 0..self.joints.len() {
            let def = &self.joints[j].def;
            let (fa, fb) = self.side_frames(j);
            for (body, frame) in [(def.a, fa), (def.b, fb)] {
                if let Some(b) = body {
                    sum[b.0 as usize] = add(sum[b.0 as usize], frame.translation);
                    count[b.0 as usize] += 1;
                }
            }
        }
        sum.iter()
            .zip(&count)
            .map(|(s, &c)| {
                if c == 0 {
                    [0.0; 3]
                } else {
                    s.map(|v| v / c as f64)
                }
            })
            .collect()
    }

    /// The rows of a run from where the bodies are.
    fn equations(&self, layout: &Layout, centres: &[Vec3], mode: Mode) -> RigidEquations {
        let mut blocks = Vec::new();
        let mut bounds = vec![None; layout.n];
        let mut rows = 0;
        for (j, joint) in self.joints.iter().enumerate() {
            let def = &joint.def;
            let (ga, gb) = self.side_frames(j);
            let mut vars = Vec::new();
            let mut centre = |body: Option<BodyId>| -> Option<Vec3> {
                let b = body?.0 as usize;
                let col = layout.body_col[b]?;
                vars.extend(col..col + 6);
                Some(centres[b])
            };
            let ca = centre(def.a);
            let cb = centre(def.b);
            let first = layout.value_col[j];
            vars.extend(first..first + def.motions.len());
            blocks.push(Block {
                joint: j,
                target: None,
                row: rows,
                rows: 6,
                vars,
                kind: BlockKind::Frames(Box::new(FrameRows {
                    ga,
                    ca,
                    gb,
                    cb,
                    motions: def.motions.iter().map(|m| m.freedom).collect(),
                    alignment: def.alignment,
                })),
            });
            rows += 6;
            for (k, m) in def.motions.iter().enumerate() {
                let col = first + k;
                if m.min.is_some() || m.max.is_some() {
                    bounds[col] = Some((
                        m.min.unwrap_or(f64::NEG_INFINITY),
                        m.max.unwrap_or(f64::INFINITY),
                    ));
                }
                if mode == Mode::Solve
                    && let Some(value) = m.target
                {
                    blocks.push(Block {
                        joint: j,
                        target: Some(m.freedom),
                        row: rows,
                        rows: 1,
                        vars: vec![col],
                        kind: BlockKind::Target {
                            value,
                            rotation: m.freedom.is_rotation(),
                        },
                    });
                    rows += 1;
                }
            }
        }
        RigidEquations {
            blocks,
            bounds,
            rows,
        }
    }

    /// Inverse weights: a body's turn counts as moving points at the
    /// system's size; values are cheaper than bodies.
    fn weights(&self, layout: &Layout, scale: f64) -> Vec<f64> {
        let mut dcol = vec![1.0; layout.n];
        let turn = 1.0 / (scale * scale);
        for col in layout.body_col.iter().flatten() {
            dcol[col + 3..col + 6].iter_mut().for_each(|w| *w = turn);
        }
        for (j, joint) in self.joints.iter().enumerate() {
            for (k, m) in joint.def.motions.iter().enumerate() {
                let w = if m.freedom.is_rotation() { turn } else { 1.0 };
                dcol[layout.value_col[j] + k] = VALUE_WEIGHT * w;
            }
        }
        dcol
    }

    fn params(&self, layout: &Layout) -> Vec<f64> {
        let mut params = vec![0.0; layout.n];
        for (j, joint) in self.joints.iter().enumerate() {
            let first = layout.value_col[j];
            params[first..first + joint.values.len()].copy_from_slice(&joint.values);
        }
        params
    }

    /// Takes a run's unknowns: the bodies' motions into their poses, the
    /// values into the joints.
    fn fold(&mut self, layout: &Layout, centres: &[Vec3], params: &[f64]) {
        for (b, body) in self.bodies.iter_mut().enumerate() {
            let Some(col) = layout.body_col[b] else {
                continue;
            };
            let x = &params[col..col + 6];
            let c = centres[b];
            let turn = Pose {
                rotation: rodrigues([x[3], x[4], x[5]]),
                translation: [0.0; 3],
            };
            let motion = Pose::translation(add(c, [x[0], x[1], x[2]]))
                .after(&turn)
                .after(&Pose::translation(c.map(|v| -v)));
            body.pose = motion.after(&body.pose);
        }
        for (j, joint) in self.joints.iter_mut().enumerate() {
            let first = layout.value_col[j];
            let n = joint.values.len();
            joint.values.copy_from_slice(&params[first..first + n]);
        }
    }

    /// One Levenberg-Marquardt run (or a drag with `goal`: a body, a point
    /// on it and where the point should go) from where the bodies are.
    fn run(&mut self, mode: Mode, goal: Option<(BodyId, Vec3, Vec3)>, opts: &RigidOptions) -> Run {
        let points: Vec<Vec3> = goal.iter().flat_map(|(_, g, t)| [*g, *t]).collect();
        let scale = self.size(&points);
        let layout = self.layout();
        let mut centres = self.centres();
        let mut goals = Vec::new();
        if let Some((body, grab, target)) = goal
            && let Some(col) = layout.body_col[body.0 as usize]
        {
            // Turning about the grabbed point, the body's translation is
            // the point's.
            centres[body.0 as usize] = grab;
            goals.extend((0..3).map(|i| (col + i, target[i] - grab[i])));
        }
        let eqs = self.equations(&layout, &centres, mode);
        let comp = eqs.component(layout.n, self.weights(&layout, scale));
        let mut params = self.params(&layout);
        let ctx = Ctx {
            eqs: &eqs,
            scale,
            tol: opts.tolerance * scale,
            max_iterations: opts.max_iterations,
        };
        let mut work = Work::new(&comp);
        let run = if goals.is_empty() {
            solve::lm(&ctx, &comp, &mut params, &mut work)
        } else {
            solve::drag(&ctx, &comp, &mut params, &mut work, &goals)
        };
        self.fold(&layout, &centres, &params);
        run
    }

    /// The rank analysis of the rows where the bodies are: rank, and the
    /// redundant and conflicting rows. `mask` holds every body but one
    /// where it is (its unknowns weigh nothing).
    fn rank(
        &self,
        mode: Mode,
        mask: Option<usize>,
    ) -> (usize, usize, Vec<RigidDependency>, Vec<RigidDependency>) {
        let scale = self.size(&[]);
        let layout = self.layout();
        let centres = self.centres();
        let eqs = self.equations(&layout, &centres, mode);
        let mut dcol = self.weights(&layout, scale);
        let mut n = layout.n;
        if let Some(keep) = mask {
            for (b, col) in layout.body_col.iter().enumerate() {
                if let Some(col) = col
                    && b != keep
                {
                    dcol[*col..col + 6].iter_mut().for_each(|w| *w = 0.0);
                    n -= 6;
                }
            }
        }
        let comp = eqs.component(layout.n, dcol);
        let params = self.params(&layout);
        let mut work = Work::new(&comp);
        let a = analysis::component_dependencies(&eqs, &comp, &params, scale, &mut work, false);
        let redundant = eqs.dependencies(&a.dependent, scale, false);
        let conflicts = eqs.dependencies(&a.dependent, scale, true);
        (n, a.rank, redundant, conflicts)
    }

    /// Places what needs no iteration (see the module documentation) and
    /// sets every joint's start values.
    fn place(&mut self, place: bool) {
        let n = self.bodies.len();
        let mut cluster: Vec<usize> = (0..n).collect();
        let fixed: Vec<bool> = self.bodies.iter().map(|b| b.fixed).collect();
        let find = |cluster: &Vec<usize>, mut b: usize| {
            while cluster[b] != b {
                b = cluster[b];
            }
            b
        };
        let is_fixed = |cluster: &Vec<usize>, root: Option<usize>| match root {
            None => true,
            Some(r) => (0..n).any(|b| fixed[b] && find(cluster, b) == r),
        };
        for j in 0..self.joints.len() {
            self.joints[j].values = self.start_values(j);
            if !place {
                continue;
            }
            let def = &self.joints[j].def;
            let ra = def.a.map(|b| find(&cluster, b.0 as usize));
            let rb = def.b.map(|b| find(&cluster, b.0 as usize));
            if ra == rb {
                continue;
            }
            let (fixed_a, fixed_b) = (is_fixed(&cluster, ra), is_fixed(&cluster, rb));
            if !(fixed_a && fixed_b) {
                let joint = &self.joints[j];
                let freedoms: Vec<Freedom> = joint.def.motions.iter().map(|m| m.freedom).collect();
                let m = motion_pose(&freedoms, &joint.values);
                let (fa, fb) = self.side_frames(j);
                let (root, delta) = if !fixed_a {
                    let wanted = fb.after(&m).after(&joint.def.alignment);
                    (ra, wanted.after(&fa.inverse()))
                } else {
                    let wanted = fa.after(&joint.def.alignment.inverse()).after(&m.inverse());
                    (rb, wanted.after(&fb.inverse()))
                };
                let root = root.expect("a free side is a body");
                for b in 0..n {
                    if find(&cluster, b) == root {
                        self.bodies[b].pose = delta.after(&self.bodies[b].pose);
                    }
                }
            }
            if let (Some(x), Some(y)) = (ra, rb) {
                cluster[x] = y;
            }
        }
    }

    /// A joint left about half a turn off, turned over: the body of side
    /// `a` (else `b`) turned so that the frames' rotations agree. False
    /// when every joint is less than a quarter turn off.
    fn turn_over(&mut self) -> bool {
        for j in 0..self.joints.len() {
            let (fa, q) = self.frames(j);
            let e = mat_mul_t(&fa.rotation, &q.rotation);
            let cos = (e[0][0] + e[1][1] + e[2][2] - 1.0) / 2.0;
            if cos >= 0.0 {
                continue;
            }
            // The axis: the symmetric part less cos θ is (1 - cos θ) n nᵀ;
            // its sign from the skew part (sin θ n).
            let k = (0..3)
                .max_by(|&a, &b| e[a][a].total_cmp(&e[b][b]))
                .expect("three");
            let mut axis: Vec3 =
                std::array::from_fn(|i| 0.5 * (e[i][k] + e[k][i]) - if i == k { cos } else { 0.0 });
            let skew = [e[2][1] - e[1][2], e[0][2] - e[2][0], e[1][0] - e[0][1]];
            if axis.iter().zip(&skew).map(|(a, s)| a * s).sum::<f64>() < 0.0 {
                axis = axis.map(|v| -v);
            }
            let angle = cos.clamp(-1.0, 1.0).acos();
            let def = &self.joints[j].def;
            let free = |b: Option<BodyId>| b.filter(|b| !self.bodies[b.0 as usize].fixed);
            let (body, turn) = match (free(def.a), free(def.b)) {
                (Some(a), _) => (a, Pose::rotation(fa.translation, axis, -angle)),
                (None, Some(b)) => (b, Pose::rotation(fa.translation, axis, angle)),
                (None, None) => continue,
            };
            let pose = &mut self.bodies[body.0 as usize].pose;
            *pose = turn.after(pose);
            return true;
        }
        false
    }

    fn at_limits(&self, scale: f64, opts: &RigidOptions) -> Vec<(JointId, Freedom)> {
        let mut out = Vec::new();
        for (j, joint) in self.joints.iter().enumerate() {
            for (m, v) in joint.def.motions.iter().zip(&joint.values) {
                if m.target.is_some() {
                    continue;
                }
                let tol = opts.tolerance * if m.freedom.is_rotation() { 1.0 } else { scale };
                let at = |l: Option<f64>| l.is_some_and(|l| (v - l).abs() <= tol.max(1e-12));
                if at(m.min) || at(m.max) {
                    out.push((JointId(j as u32), m.freedom));
                }
            }
        }
        out
    }

    /// Moves the bodies as little as possible until every joint holds and
    /// every driven motion is at its target, free motions within their
    /// limits. When that fails, the bodies and values are left where they
    /// were and the result tells why ([`RigidResult::conflicts`] when the
    /// joints contradict each other).
    pub fn solve(&mut self, opts: &RigidOptions) -> RigidResult {
        let saved = (self.bodies.clone(), self.joints.clone());
        self.place(opts.place);
        self.finish(Mode::Solve, None, opts, saved)
    }

    /// Pulls `grab` (a point of `body` where it is now) toward `target`
    /// while the joints hold, moving the other bodies as little as
    /// possible. Targets are let go: driven motions follow like free ones
    /// (read their values to drive them there).
    pub fn drag(
        &mut self,
        body: BodyId,
        grab: Vec3,
        target: Vec3,
        opts: &RigidOptions,
    ) -> RigidResult {
        let saved = (self.bodies.clone(), self.joints.clone());
        self.place(false);
        self.finish(Mode::Joints, Some((body, grab, target)), opts, saved)
    }

    fn finish(
        &mut self,
        mode: Mode,
        goal: Option<(BodyId, Vec3, Vec3)>,
        opts: &RigidOptions,
        saved: (Vec<BodyRec>, Vec<JointRec>),
    ) -> RigidResult {
        let mut iterations = 0;
        let mut attempt = 0;
        let run = loop {
            let r = self.run(mode, goal, opts);
            iterations += r.iterations;
            attempt += 1;
            if attempt < ATTEMPTS && self.turn_over() {
                continue;
            }
            break r;
        };
        let mut residual = run.residual;
        let mut converged = run.converged;
        // A drag's goal may stay out of reach; the joints must hold.
        if goal.is_some() && !converged {
            let r = self.run(mode, None, opts);
            iterations += r.iterations;
            residual = r.residual;
            converged = r.converged;
        }
        let scale = self.size(&[]);
        if converged {
            return RigidResult {
                status: SolveStatus::Converged,
                iterations,
                residual,
                conflicts: Vec::new(),
                at_limits: self.at_limits(scale, opts),
            };
        }
        let (_, _, _, conflicts) = self.rank(mode, None);
        (self.bodies, self.joints) = saved;
        RigidResult {
            status: if conflicts.is_empty() {
                SolveStatus::NotConverged
            } else {
                SolveStatus::Conflicting
            },
            iterations,
            residual,
            conflicts,
            at_limits: Vec::new(),
        }
    }

    /// Degrees of freedom of the joints where the bodies are (targets left
    /// out), and redundant or conflicting joints.
    pub fn analyze(&self) -> RigidAnalysis {
        let (n, rank, redundant, conflicts) = self.rank(Mode::Joints, None);
        let body_dof = (0..self.bodies.len())
            .map(|b| {
                if self.bodies[b].fixed {
                    0
                } else {
                    let (n, rank, _, _) = self.rank(Mode::Joints, Some(b));
                    n - rank
                }
            })
            .collect();
        RigidAnalysis {
            dof: n - rank,
            body_dof,
            redundant,
            conflicts,
        }
    }
}
