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
//! Every motion folded into a body's pose leaves it a rotation to rounding
//! ([`Pose::rigid`]), and [`Pose::inverse`] inverts that rigid pose
//! (mitcad#1668): frames a little off a rotation would otherwise double
//! their error with each placing.
//!
//! The skew part of a rotation also vanishes at half a turn; a solve that
//! ends there turns the joint's body over and iterates again.
//!
//! An assembly constraint (mitcad#148, [`RigidConstraint`]) relates a frame
//! on `a` to one on `b` by some of these equations only
//! ([`ConstraintPart`]), written in frame `b`'s coordinates: coordinates
//! of a's origin, its distance from b's origin or z axis, the z axes along
//! or across each other or at an angle, and a's x axis turned about b's z.
//! A constraint may have one value (an offset or an angle) as an unknown
//! with limits and a target, like a joint's motion. Constraints are
//! numbered with the joints ([`JointId`]), their rows and targets are in
//! the same runs, rank analysis and dependencies. Before iterating, a
//! constraint moves its free side as little as it can so that each part
//! holds (directions first, about a's origin), joining nothing; a solve
//! that ends with z or x axes the wrong way turns the side over.

use std::f64::consts::{PI, TAU};

use mitcad_vec3::{add, cross, dot, norm, sub};

use crate::analysis::{self, CONFLICT_TOL, DepRow};
use crate::prepare::Component;
use crate::scalar::{Dual, LANES, Scalar};
use crate::solve::{self, Ctx, Equations, Run, Work};
use crate::sparse::{GramPlan, Rows};
use crate::types::SolveStatus;

pub use mitcad_vec3::Vec3;
type M3<T> = [[T; 3]; 3];

const I3: M3<f64> = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
/// Joint values cost this much less than moving a body by as much: a step
/// changes values before it moves bodies.
const VALUE_WEIGHT: f64 = 100.0;
/// Attempts of a solve that ends half a turn off.
const ATTEMPTS: usize = 4;

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

/// What [`nearest_rotation`] keeps as it is: the rounding of a few
/// products of rotations.
const ROUNDING: f64 = 1e-15;
/// The orthonormality error up to which [`nearest_rotation`] straightens a
/// matrix; one further off is not a rotation that rounding moved.
const STRAIGHTENED: f64 = 1e-3;

/// The largest entry of `|MᵀM − I|`: 0 for an exact rotation or mirror;
/// NaN or infinite for a matrix with a non-finite entry.
fn orthonormality_error(m: &M3<f64>) -> f64 {
    let mut worst = 0.0_f64;
    for r in 0..3 {
        for c in 0..3 {
            let product = m[0][r] * m[0][c] + m[1][r] * m[1][c] + m[2][r] * m[2][c];
            let deviation = (product - if r == c { 1.0 } else { 0.0 }).abs();
            if deviation.is_nan() {
                return f64::NAN;
            }
            worst = worst.max(deviation);
        }
    }
    worst
}

/// The rotation nearest to `m` (mitcad#1668): `m` itself when it is
/// orthonormal within [`ROUNDING`] or off by more than [`STRAIGHTENED`],
/// else the orthogonal factor of its polar decomposition. The iteration
/// `X ← X·(3I − XᵀX)/2` converges quadratically to it while `XᵀX` is near
/// the identity (from 1e-12 off one step reaches rounding, from 1e-3 four)
/// and keeps the sign of the determinant, so a rotation stays one.
fn nearest_rotation(m: &M3<f64>) -> M3<f64> {
    let error = orthonormality_error(m);
    // A matrix with a non-finite entry is kept as it is.
    if error.is_nan() || error <= ROUNDING || error > STRAIGHTENED {
        return *m;
    }
    let mut x = *m;
    for _ in 0..6 {
        let xtx: M3<f64> = std::array::from_fn(|r| {
            std::array::from_fn(|c| x[0][r] * x[0][c] + x[1][r] * x[1][c] + x[2][r] * x[2][c])
        });
        let half: M3<f64> = std::array::from_fn(|r| {
            std::array::from_fn(|c| {
                let three = if r == c { 3.0 } else { 0.0 };
                (three - xtx[r][c]) / 2.0
            })
        });
        x = mat_mul(&x, &half);
        if orthonormality_error(&x) <= ROUNDING {
            break;
        }
    }
    x
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

    /// The inverse of the rigid pose nearest to `self` ([`Pose::rigid`]):
    /// the transpose of its rotation made orthonormal (mitcad#1668). The
    /// plain transpose of a rotation a little off (`RᵀR = S`) would carry
    /// that error into every pose composed with it, so each placing would
    /// double it.
    pub fn inverse(&self) -> Pose {
        let r = transpose(&nearest_rotation(&self.rotation));
        Pose {
            rotation: r,
            translation: mat_vec(&r, self.translation).map(|v| -v),
        }
    }

    /// The rigid pose nearest to `self`, its translation kept: `self` when
    /// its rotation is orthonormal within rounding, else the nearest
    /// rotation (the orthogonal factor of its polar decomposition) when it
    /// is off by at most 1e-3 (the largest entry of `|RᵀR − I|`); a
    /// rotation further off is kept as it is.
    pub fn rigid(&self) -> Pose {
        Pose {
            rotation: nearest_rotation(&self.rotation),
            translation: self.translation,
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

/// Which way a constraint's z axes point against each other.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sense {
    /// The same way.
    Same,
    /// Opposite ways (a face against a face).
    Opposite,
    /// Either way.
    Either,
}

/// What a part of a constraint holds a measure at: a number, or the
/// constraint's value (an unknown within its limits, driven to its
/// target when it has one).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Amount {
    Fixed(f64),
    Value,
}

/// One equation of an assembly constraint (mitcad#148), in frame `b`'s
/// coordinates: `d` is frame `a`'s origin there, `z` and `x` frame `a`'s
/// axes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ConstraintPart {
    /// `d.x = 0`: a's origin on b's yz-plane.
    OriginX,
    /// `d.y = 0`.
    OriginY,
    /// `d.z` at the amount: a's origin that high above b's xy-plane.
    OriginZ(Amount),
    /// `|d|` at the amount (not zero: use the three origin parts).
    Distance(Amount),
    /// The distance of a's origin from b's z axis at the amount (not zero:
    /// use `OriginX` and `OriginY`).
    Radial(Amount),
    /// The z axes along each other (two equations).
    Parallel(Sense),
    /// The z axes across each other.
    Perpendicular,
    /// The angle between the z axes (0 to π) at the amount (neither 0 nor
    /// π: use `Parallel`).
    Angle(Amount),
    /// a's x axis turned this far (radians) about b's z axis from b's x
    /// axis; for z axes along each other.
    Twist(f64),
}

impl ConstraintPart {
    /// Its equations.
    pub fn rows(&self) -> usize {
        match self {
            Self::Parallel(_) => 2,
            _ => 1,
        }
    }

    fn amount(&self) -> Option<Amount> {
        match self {
            Self::OriginZ(a) | Self::Distance(a) | Self::Radial(a) | Self::Angle(a) => Some(*a),
            _ => None,
        }
    }

    /// Whether its equations are of directions (scaled by the system's size).
    fn angular(&self) -> bool {
        matches!(
            self,
            Self::Parallel(_) | Self::Perpendicular | Self::Angle(_) | Self::Twist(_)
        )
    }
}

/// An assembly constraint: some of a joint's equations between a frame on
/// body `a` and one on body `b` (either side may be the fixed world),
/// solved together with the joints. Frames are given where the bodies are
/// when it is added.
#[derive(Clone, Debug, PartialEq)]
pub struct RigidConstraint {
    pub a: Option<BodyId>,
    pub frame_a: Pose,
    pub b: Option<BodyId>,
    pub frame_b: Pose,
    pub parts: Vec<ConstraintPart>,
    /// The value of the parts with [`Amount::Value`]: its limits and target
    /// (`freedom` says whether it is a length, `Tz`, or an angle, `Rz`, and
    /// names it in dependencies).
    pub value: Option<JointMotion>,
}

/// How a coupling's follower follows its driver (mitcad#466).
#[derive(Clone, Debug, PartialEq)]
pub enum Relation {
    /// `follower = ratio · driver + offset` (gears, a rack and pinion, a
    /// screw).
    Linear { ratio: f64, offset: f64 },
    /// `follower = table(driver) + offset`: the table's values at the
    /// driver's values `2π k / n` (a cam's follower over one turn of the
    /// cam), between them a periodic monotone cubic (it never passes the
    /// two samples it lies between, so a dwell stays flat).
    Table { values: Vec<f64>, offset: f64 },
}

impl Relation {
    /// The follower's value for the driver's `x`.
    pub fn follow(&self, x: f64) -> f64 {
        self.eval(x)
    }

    fn eval<T: Scalar>(&self, x: T) -> T {
        match self {
            Relation::Linear { ratio, offset } => x.mulf(*ratio) + T::cst(*offset),
            Relation::Table { values, offset } => periodic_cubic(values, x) + T::cst(*offset),
        }
    }

    fn is_finite(&self) -> bool {
        match self {
            Relation::Linear { ratio, offset } => ratio.is_finite() && offset.is_finite(),
            Relation::Table { values, offset } => {
                !values.is_empty() && offset.is_finite() && values.iter().all(|v| v.is_finite())
            }
        }
    }
}

/// A periodic table over one turn (`values[k]` at `2π k / n`) at `x`:
/// cubic Hermite pieces whose slopes at the samples are limited
/// (Fritsch and Carlson) so that each piece stays between its two samples.
fn periodic_cubic<T: Scalar>(values: &[f64], x: T) -> T {
    let n = values.len();
    let per = n as f64 / std::f64::consts::TAU;
    let u = (x.val() * per).rem_euclid(n as f64);
    if !u.is_finite() {
        return T::cst(f64::NAN);
    }
    let i = (u.floor() as usize).min(n - 1);
    // The fraction of the piece, with the driver's derivative.
    let f = x.mulf(per) - T::cst(x.val() * per - (u - i as f64));
    let at = |k: usize| values[k % n];
    let (p0, p1, p2, p3) = (at(i + n - 1), at(i), at(i + 1), at(i + 2));
    let slope = |before: f64, after: f64| {
        if before * after <= 0.0 {
            0.0
        } else {
            let mean = 0.5 * (before + after);
            mean.signum() * mean.abs().min(3.0 * before.abs().min(after.abs()))
        }
    };
    let (m1, m2) = (slope(p1 - p0, p2 - p1), slope(p2 - p1, p3 - p2));
    let f2 = f * f;
    let f3 = f2 * f;
    let (h00, h10) = (
        f3.mulf(2.0) - f2.mulf(3.0) + T::cst(1.0),
        f3 - f2.mulf(2.0) + f,
    );
    let (h01, h11) = (f2.mulf(3.0) - f3.mulf(2.0), f3 - f2);
    h00.mulf(p1) + h10.mulf(m1) + h01.mulf(p2) + h11.mulf(m2)
}

/// A coupling of two joints' free motions (mitcad#466): the follower's
/// value follows the driver's by the relation. Numbered with the joints
/// ([`JointId`]); its row is in the solves, drags and analyses. A
/// follower whose driver is driven (or determined by other couplings),
/// and a table's follower always, lets go of its own target; a linear
/// follower driven while its driver is free drives the driver.
#[derive(Clone, Debug, PartialEq)]
pub struct RigidCoupling {
    pub driver: (JointId, Freedom),
    pub follower: (JointId, Freedom),
    pub relation: Relation,
}

#[derive(Clone, Debug, PartialEq)]
struct CouplingRec {
    /// Joint and index of the motion.
    driver: (usize, usize),
    follower: (usize, usize),
    relation: Relation,
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
    /// A constraint's part holds its value but the constraint has none, or
    /// one of its numbers is not finite.
    BadPart,
    /// A coupling names a constraint or a coupling instead of a joint, the
    /// same motion twice, or a relation that is not finite.
    BadCoupling,
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
            RigidError::BadPart => f.write_str("a part of the constraint is invalid"),
            RigidError::BadCoupling => f.write_str("the coupling is invalid"),
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
    /// Whether each body moves in some motion of all of them together
    /// (the others free too): a link of a four-bar linkage has no motion
    /// of its own but moves; false when fixed or held fixed by the joints.
    pub body_mobile: Vec<bool>,
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

impl BodyRec {
    /// Moves the body by `motion` (after where it is), its pose kept rigid
    /// (mitcad#1668): the motions come from frames that may be a little
    /// off a rotation, and each solve composes many of them.
    fn move_by(&mut self, motion: &Pose) {
        self.pose = motion.after(&self.pose).rigid();
    }
}

#[derive(Clone, Debug)]
struct JointRec {
    /// A constraint's sides and value (as its one motion; no alignment).
    def: RigidJoint,
    values: Vec<f64>,
    /// A constraint's equations; None for a joint.
    parts: Option<Vec<ConstraintPart>>,
    /// A coupling's motions and relation (mitcad#466; no sides, frames or
    /// values of its own).
    coupling: Option<CouplingRec>,
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

/// Which bodies a rank analysis holds where they are.
#[derive(Clone, Copy, PartialEq)]
enum Hold {
    None,
    /// Every body but this one.
    AllBut(usize),
    /// This body only.
    One(usize),
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

/// A constraint's rows.
struct PartRows {
    ga: Pose,
    ca: Option<Vec3>,
    gb: Pose,
    cb: Option<Vec3>,
    parts: Vec<ConstraintPart>,
    /// Whether the constraint has a value (the last unknown).
    value: bool,
}

enum BlockKind {
    Frames(Box<FrameRows>),
    Parts(Box<PartRows>),
    Target {
        value: f64,
        rotation: bool,
    },
    /// `follower - relation(driver)`: `vars` are the driver's and the
    /// follower's values; a turning follower's row is scaled as a turn.
    Coupling {
        relation: Relation,
        rotation: bool,
    },
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

/// Frame `a` seen from frame `b`: a's origin, z axis and x axis in b's
/// coordinates.
fn relative<T: Scalar>(fa: &PoseT<T>, fb: &PoseT<T>) -> [[T; 3]; 3] {
    let local = |v: [T; 3]| -> [T; 3] {
        std::array::from_fn(|i| fb.r[0][i] * v[0] + fb.r[1][i] * v[1] + fb.r[2][i] * v[2])
    };
    let column = |c: usize| -> [T; 3] { std::array::from_fn(|r| fa.r[r][c]) };
    [
        local(std::array::from_fn(|i| fa.t[i] - fb.t[i])),
        local(column(2)),
        local(column(0)),
    ]
}

/// A constraint's residuals between frames `fa` and `fb` (lengths in mm,
/// directions times `scale`); `value` is the constraint's value.
fn constraint_rows<T: Scalar>(
    parts: &[ConstraintPart],
    fa: &PoseT<T>,
    fb: &PoseT<T>,
    value: Option<T>,
    scale: f64,
    out: &mut Vec<T>,
) {
    let [d, z, x] = relative(fa, fb);
    let amount = |a: Amount| match a {
        Amount::Fixed(v) => T::cst(v),
        Amount::Value => value.unwrap_or(T::cst(0.0)),
    };
    for part in parts {
        match *part {
            ConstraintPart::OriginX => out.push(d[0]),
            ConstraintPart::OriginY => out.push(d[1]),
            ConstraintPart::OriginZ(a) => out.push(d[2] - amount(a)),
            ConstraintPart::Distance(a) => {
                out.push((d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt() - amount(a));
            }
            ConstraintPart::Radial(a) => out.push((d[0] * d[0] + d[1] * d[1]).sqrt() - amount(a)),
            ConstraintPart::Parallel(_) => {
                out.push(z[0].mulf(scale));
                out.push(z[1].mulf(scale));
            }
            ConstraintPart::Perpendicular => out.push(z[2].mulf(scale)),
            ConstraintPart::Angle(a) => {
                let across = (z[0] * z[0] + z[1] * z[1]).sqrt();
                out.push((across.atan2(z[2]) - amount(a)).mulf(scale));
            }
            ConstraintPart::Twist(t) => {
                out.push((x[1].mulf(t.cos()) - x[0].mulf(t.sin())).mulf(scale));
            }
        }
    }
}

/// A constraint's residuals at the run's unknowns.
fn part_rows<T: Scalar>(rows: &PartRows, x: &[T], scale: f64) -> Vec<T> {
    let (fa, used_a) = side_frame(&rows.ga, rows.ca, x);
    let (fb, used_b) = side_frame(&rows.gb, rows.cb, &x[used_a..]);
    let value = rows.value.then(|| x[used_a + used_b]);
    let mut out = Vec::new();
    constraint_rows(&rows.parts, &fa, &fb, value, scale, &mut out);
    out
}

/// A block's residuals and their derivatives by its unknowns, in passes of
/// [`LANES`] unknowns.
fn evaluate_block<R: AsRef<[f64]>, D: AsRef<[Dual]>>(
    block: &Block,
    params: &[f64],
    f: &mut [f64],
    jac: Option<&mut Rows>,
    eval_f64: impl Fn(&[f64]) -> R,
    eval_dual: impl Fn(&[Dual]) -> D,
) {
    let Some(jac) = jac else {
        let x: Vec<f64> = block.vars.iter().map(|&g| params[g]).collect();
        let r = eval_f64(&x);
        f[block.row..block.row + block.rows].copy_from_slice(r.as_ref());
        return;
    };
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
        let r = eval_dual(&x);
        for (k, v) in r.as_ref().iter().enumerate() {
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
                BlockKind::Frames(rows) => evaluate_block(
                    block,
                    params,
                    f,
                    jac.as_deref_mut(),
                    |x| frame_rows(rows, x, scale),
                    |x| frame_rows(rows, x, scale),
                ),
                BlockKind::Parts(rows) => evaluate_block(
                    block,
                    params,
                    f,
                    jac.as_deref_mut(),
                    |x| part_rows(rows, x, scale),
                    |x| part_rows(rows, x, scale),
                ),
                BlockKind::Coupling { relation, rotation } => {
                    let k = if *rotation { scale } else { 1.0 };
                    let row = |x: &[Dual]| [(x[1] - relation.eval(x[0])).mulf(k)];
                    evaluate_block(
                        block,
                        params,
                        f,
                        jac.as_deref_mut(),
                        |x: &[f64]| [(x[1] - relation.eval(x[0])) * k],
                        row,
                    );
                }
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
            plan: GramPlan::shared(&row_cols, n, None),
            jac,
            dcol,
            analysis_plans: Default::default(),
            implicit: Vec::new(),
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

/// The measure the parts with [`Amount::Value`] hold (the first one's):
/// a height, distance or angle between the frames.
fn parts_measure(parts: &[ConstraintPart], fa: &Pose, fb: &Pose) -> Option<f64> {
    let k = parts
        .iter()
        .position(|p| p.amount() == Some(Amount::Value))?;
    let mut rows = Vec::new();
    constraint_rows(
        &parts[k..=k],
        &PoseT::cst(fa),
        &PoseT::cst(fb),
        Some(0.0),
        1.0,
        &mut rows,
    );
    rows.first().copied()
}

/// How far parts are from holding between two frames with the value at
/// `value`: the largest residual of lengths (mm) and the largest error of
/// directions (radians; half a turn when z axes or x axes point the wrong
/// way).
fn parts_error(parts: &[ConstraintPart], fa: &Pose, fb: &Pose, value: Option<f64>) -> (f64, f64) {
    let (pa, pb) = (PoseT::cst(fa), PoseT::cst(fb));
    let [_, z, x] = relative(&pa, &pb);
    let mut length: f64 = 0.0;
    let mut angle: f64 = 0.0;
    for part in parts {
        let mut rows = Vec::new();
        constraint_rows(std::slice::from_ref(part), &pa, &pb, value, 1.0, &mut rows);
        let worst = rows.iter().fold(0.0_f64, |m, r| m.max(r.abs()));
        if !part.angular() {
            length = length.max(worst);
            continue;
        }
        let wrong_way = match part {
            ConstraintPart::Parallel(Sense::Same) => z[2] < 0.0,
            ConstraintPart::Parallel(Sense::Opposite) => z[2] > 0.0,
            ConstraintPart::Twist(t) => x[0] * t.cos() + x[1] * t.sin() < 0.0,
            _ => false,
        };
        let error = match part {
            ConstraintPart::Angle(_) => worst,
            _ => worst.clamp(0.0, 1.0).asin(),
        };
        angle = angle.max(if wrong_way {
            std::f64::consts::PI
        } else {
            error
        });
    }
    (length, angle)
}

fn column(p: &Pose, c: usize) -> Vec3 {
    std::array::from_fn(|r| p.rotation[r][c])
}

/// The turn about `pivot` that takes the unit direction `from` to `to`
/// (half a turn about `across`, a unit direction across both, when they
/// are opposite).
fn turn_between(pivot: Vec3, from: Vec3, to: Vec3, across: Vec3) -> Pose {
    let axis = cross(from, to);
    let sin = norm(axis);
    let cos = dot(from, to);
    if sin < 1e-12 {
        return if cos > 0.0 {
            Pose::IDENTITY
        } else {
            Pose::rotation(pivot, across, std::f64::consts::PI)
        };
    }
    Pose::rotation(pivot, axis, sin.atan2(cos))
}

/// The smallest motion of frame `a` that makes the parts hold against
/// frame `b` with the value at `value`, as far as each part alone settles
/// it: directions first (turning about a's origin), then the origin.
fn constraint_delta(parts: &[ConstraintPart], fa: &Pose, fb: &Pose, value: Option<f64>) -> Pose {
    let (xb, yb, zb) = (column(fb, 0), column(fb, 1), column(fb, 2));
    let pivot = fa.translation;
    let amount = |a: Amount| match a {
        Amount::Fixed(v) => Some(v),
        Amount::Value => value,
    };
    let mut delta = Pose::IDENTITY;
    let za = |d: &Pose| d.apply_vector(column(fa, 2));
    for part in parts {
        let z = za(&delta);
        let turn = match *part {
            ConstraintPart::Parallel(sense) => {
                let same = match sense {
                    Sense::Same => true,
                    Sense::Opposite => false,
                    Sense::Either => dot(z, zb) >= 0.0,
                };
                let to = if same { zb } else { zb.map(|v| -v) };
                turn_between(pivot, z, to, xb)
            }
            ConstraintPart::Perpendicular => {
                let c = dot(z, zb);
                let across = std::array::from_fn(|i| z[i] - c * zb[i]);
                let n = norm(across);
                let to = if n < 1e-12 { xb } else { across.map(|v| v / n) };
                turn_between(pivot, z, to, xb)
            }
            ConstraintPart::Angle(a) => match amount(a) {
                Some(v) => {
                    let now = norm(cross(zb, z)).atan2(dot(zb, z));
                    let k = cross(zb, z);
                    let axis = if norm(k) < 1e-9 { xb } else { k };
                    Pose::rotation(pivot, axis, v - now)
                }
                None => Pose::IDENTITY,
            },
            ConstraintPart::Twist(t) => {
                let x = delta.apply_vector(column(fa, 0));
                let now = dot(x, yb).atan2(dot(x, xb));
                Pose::rotation(pivot, zb, t - now)
            }
            _ => Pose::IDENTITY,
        };
        delta = turn.after(&delta);
    }
    // The origin: each part's coordinates of it in b's frame.
    let local = |p: Vec3| -> Vec3 {
        let w = sub(p, fb.translation);
        [dot(w, xb), dot(w, yb), dot(w, zb)]
    };
    let d = local(delta.apply_point(pivot));
    let mut want = d;
    for part in parts {
        match *part {
            ConstraintPart::OriginX => want[0] = 0.0,
            ConstraintPart::OriginY => want[1] = 0.0,
            ConstraintPart::OriginZ(a) => {
                if let Some(v) = amount(a) {
                    want[2] = v;
                }
            }
            ConstraintPart::Distance(a) => {
                let n = norm(want);
                if let (Some(v), true) = (amount(a), n > 1e-12) {
                    want = want.map(|c| c * v / n);
                }
            }
            ConstraintPart::Radial(a) => {
                let n = (want[0] * want[0] + want[1] * want[1]).sqrt();
                if let (Some(v), true) = (amount(a), n > 1e-12) {
                    want[0] *= v / n;
                    want[1] *= v / n;
                }
            }
            _ => {}
        }
    }
    let shift: Vec3 = std::array::from_fn(|i| {
        (want[0] - d[0]) * xb[i] + (want[1] - d[1]) * yb[i] + (want[2] - d[2]) * zb[i]
    });
    Pose::translation(shift).after(&delta)
}

impl RigidConstraint {
    /// The measure its value holds where the frames are (the height of a's
    /// origin, a distance or an angle); None without a value.
    pub fn measured(&self) -> Option<f64> {
        self.value.as_ref()?;
        parts_measure(&self.parts, &self.frame_a, &self.frame_b)
    }

    /// How far it is from holding where the frames are: the largest
    /// residual of lengths (mm) and error of directions (radians). Its
    /// value is its target, else the measure kept within its limits.
    pub fn error(&self) -> (f64, f64) {
        let value = self.value.as_ref().and_then(|m| {
            m.target.or_else(|| {
                let v = self.measured()?;
                Some(v.clamp(
                    m.min.unwrap_or(f64::NEG_INFINITY),
                    m.max.unwrap_or(f64::INFINITY),
                ))
            })
        });
        parts_error(&self.parts, &self.frame_a, &self.frame_b, value)
    }
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
        Ok(self.push(joint, None))
    }

    fn push(&mut self, def: RigidJoint, parts: Option<Vec<ConstraintPart>>) -> JointId {
        let values = vec![0.0; def.motions.len()];
        self.joints.push(JointRec {
            def,
            values,
            parts,
            coupling: None,
        });
        let id = JointId(self.joints.len() as u32 - 1);
        self.joints[id.0 as usize].values = self.start_values(id.0 as usize);
        id
    }

    /// Adds a coupling of two joints' motions (mitcad#466): numbered with
    /// the joints, solved and analysed with them.
    pub fn add_coupling(&mut self, c: RigidCoupling) -> Result<JointId, RigidError> {
        let motion = |(joint, freedom): (JointId, Freedom)| -> Result<(usize, usize), RigidError> {
            let rec = self
                .joints
                .get(joint.0 as usize)
                .ok_or(RigidError::NoJoint(joint))?;
            if rec.parts.is_some() || rec.coupling.is_some() {
                return Err(RigidError::BadCoupling);
            }
            let k = rec
                .def
                .motions
                .iter()
                .position(|m| m.freedom == freedom)
                .ok_or(RigidError::NoMotion(joint, freedom))?;
            Ok((joint.0 as usize, k))
        };
        let driver = motion(c.driver)?;
        let follower = motion(c.follower)?;
        if driver == follower || !c.relation.is_finite() {
            return Err(RigidError::BadCoupling);
        }
        self.joints.push(JointRec {
            def: RigidJoint {
                a: None,
                frame_a: Pose::IDENTITY,
                b: None,
                frame_b: Pose::IDENTITY,
                motions: Vec::new(),
                alignment: Pose::IDENTITY,
            },
            values: Vec::new(),
            parts: None,
            coupling: Some(CouplingRec {
                driver,
                follower,
                relation: c.relation,
            }),
        });
        Ok(JointId(self.joints.len() as u32 - 1))
    }

    /// The couplings' followers whose targets are let go: a follower whose
    /// driver is determined (driven, or by couplings from driven motions)
    /// and every table's follower. Per joint, per motion.
    fn released(&self) -> Vec<Vec<bool>> {
        let targets: Vec<Vec<bool>> = self
            .joints
            .iter()
            .map(|j| j.def.motions.iter().map(|m| m.target.is_some()).collect())
            .collect();
        let mut determined = targets.clone();
        let mut released: Vec<Vec<bool>> = targets.iter().map(|t| vec![false; t.len()]).collect();
        loop {
            let mut changed = false;
            for c in self.joints.iter().filter_map(|j| j.coupling.as_ref()) {
                let ((d, dk), (f, fk)) = (c.driver, c.follower);
                let table = matches!(c.relation, Relation::Table { .. });
                if targets[f][fk] && !released[f][fk] && (table || determined[d][dk]) {
                    released[f][fk] = true;
                    changed = true;
                }
                if determined[d][dk] && !determined[f][fk] {
                    determined[f][fk] = true;
                    changed = true;
                }
            }
            if !changed {
                return released;
            }
        }
    }

    /// Adds an assembly constraint (mitcad#148): numbered with the joints,
    /// solved and analysed with them.
    pub fn add_constraint(&mut self, c: RigidConstraint) -> Result<JointId, RigidError> {
        for b in [c.a, c.b].into_iter().flatten() {
            if b.0 as usize >= self.bodies.len() {
                return Err(RigidError::NoBody(b));
            }
        }
        if c.a == c.b {
            return Err(RigidError::SameSides);
        }
        if let Some(m) = &c.value {
            let finite = [m.min, m.max, m.target]
                .iter()
                .flatten()
                .all(|v| v.is_finite());
            let ordered = match (m.min, m.max) {
                (Some(lo), Some(hi)) => lo <= hi,
                _ => true,
            };
            if !finite || !ordered {
                return Err(RigidError::BadValue(m.freedom));
            }
        }
        for part in &c.parts {
            let number = match part {
                ConstraintPart::Twist(t) => Some(*t),
                _ => match part.amount() {
                    Some(Amount::Fixed(v)) => Some(v),
                    Some(Amount::Value) if c.value.is_none() => return Err(RigidError::BadPart),
                    _ => None,
                },
            };
            if number.is_some_and(|v| !v.is_finite()) {
                return Err(RigidError::BadPart);
            }
        }
        let def = RigidJoint {
            a: c.a,
            frame_a: c.frame_a,
            b: c.b,
            frame_b: c.frame_b,
            motions: c.value.into_iter().collect(),
            alignment: Pose::IDENTITY,
        };
        Ok(self.push(def, Some(c.parts)))
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
    /// A constraint's: its largest residual of lengths (mm) and of
    /// directions (radians) at its value.
    pub fn joint_error(&self, joint: JointId) -> (f64, f64) {
        let rec = &self.joints[joint.0 as usize];
        if let Some(c) = &rec.coupling {
            // A coupling's: how far the follower is from its relation's
            // value, as a length or a turn.
            let value = |(j, k): (usize, usize)| self.joints[j].values[k];
            let off = (value(c.follower) - c.relation.follow(value(c.driver))).abs();
            let (f, fk) = c.follower;
            return if self.joints[f].def.motions[fk].freedom.is_rotation() {
                (0.0, off)
            } else {
                (off, 0.0)
            };
        }
        if let Some(parts) = &rec.parts {
            let (fa, fb) = self.side_frames(joint.0 as usize);
            return parts_error(parts, &fa, &fb, rec.values.first().copied());
        }
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
        if joint.coupling.is_some() {
            return Vec::new();
        }
        let (fa, fb) = self.side_frames(j);
        if let Some(parts) = &joint.parts {
            // A constraint's value: its target, else its measure.
            return joint
                .def
                .motions
                .iter()
                .map(|m| {
                    m.target.unwrap_or_else(|| {
                        parts_measure(parts, &fa, &fb).unwrap_or(0.0).clamp(
                            m.min.unwrap_or(f64::NEG_INFINITY),
                            m.max.unwrap_or(f64::INFINITY),
                        )
                    })
                })
                .collect();
        }
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
            if self.joints[j].coupling.is_some() {
                continue;
            }
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
        let released = self.released();
        for (j, joint) in self.joints.iter().enumerate() {
            if let Some(c) = &joint.coupling {
                let col = |(i, k): (usize, usize)| layout.value_col[i] + k;
                let (f, fk) = c.follower;
                blocks.push(Block {
                    joint: j,
                    target: None,
                    row: rows,
                    rows: 1,
                    vars: vec![col(c.driver), col(c.follower)],
                    kind: BlockKind::Coupling {
                        relation: c.relation.clone(),
                        rotation: self.joints[f].def.motions[fk].freedom.is_rotation(),
                    },
                });
                rows += 1;
                continue;
            }
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
            let (count, kind) = match &joint.parts {
                Some(parts) => (
                    parts.iter().map(ConstraintPart::rows).sum(),
                    BlockKind::Parts(Box::new(PartRows {
                        ga,
                        ca,
                        gb,
                        cb,
                        parts: parts.clone(),
                        value: !def.motions.is_empty(),
                    })),
                ),
                None => (
                    6,
                    BlockKind::Frames(Box::new(FrameRows {
                        ga,
                        ca,
                        gb,
                        cb,
                        motions: def.motions.iter().map(|m| m.freedom).collect(),
                        alignment: def.alignment,
                    })),
                ),
            };
            blocks.push(Block {
                joint: j,
                target: None,
                row: rows,
                rows: count,
                vars,
                kind,
            });
            rows += count;
            for (k, m) in def.motions.iter().enumerate() {
                let col = first + k;
                if m.min.is_some() || m.max.is_some() {
                    bounds[col] = Some((
                        m.min.unwrap_or(f64::NEG_INFINITY),
                        m.max.unwrap_or(f64::INFINITY),
                    ));
                }
                if mode == Mode::Solve
                    && !released[j][k]
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
            body.move_by(&motion);
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

    /// The rank analysis of the rows where the bodies are: the unknowns,
    /// the rank, and the redundant and conflicting rows. `hold` holds
    /// bodies where they are (their unknowns weigh nothing).
    fn rank(
        &self,
        mode: Mode,
        hold: Hold,
    ) -> (usize, usize, Vec<RigidDependency>, Vec<RigidDependency>) {
        let scale = self.size(&[]);
        let layout = self.layout();
        let centres = self.centres();
        let eqs = self.equations(&layout, &centres, mode);
        let mut dcol = self.weights(&layout, scale);
        let mut n = layout.n;
        for (b, col) in layout.body_col.iter().enumerate() {
            let held = match hold {
                Hold::None => false,
                Hold::AllBut(keep) => b != keep,
                Hold::One(one) => b == one,
            };
            if let Some(col) = col
                && held
            {
                dcol[*col..col + 6].iter_mut().for_each(|w| *w = 0.0);
                n -= 6;
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
        let released = self.released();
        self.unwrap_drivers();
        for j in 0..self.joints.len() {
            self.joints[j].values = self.start_values(j);
            self.follow_couplings(j, &released);
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
            if let Some(parts) = &self.joints[j].parts {
                // A constraint moves the free side (a's between two free
                // ones) as little as it can so that it holds, and joins
                // nothing: what it leaves free is left to the iteration.
                if fixed_a && fixed_b {
                    continue;
                }
                let (fa, fb) = self.side_frames(j);
                let delta =
                    constraint_delta(parts, &fa, &fb, self.joints[j].values.first().copied());
                let (root, delta) = if !fixed_a {
                    (ra, delta)
                } else {
                    (rb, delta.inverse())
                };
                let root = root.expect("a free side is a body");
                for b in 0..n {
                    if find(&cluster, b) == root {
                        self.bodies[b].move_by(&delta);
                    }
                }
                continue;
            }
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
                        self.bodies[b].move_by(&delta);
                    }
                }
            }
            if let (Some(x), Some(y)) = (ra, rb) {
                cluster[x] = y;
            }
        }
    }

    /// A free turn without limits is only known where the bodies are up to
    /// whole turns; a coupling's follower tells which turn its driver is
    /// at (a gear's turns move its mate by a part of a turn, a pinion's
    /// move the rack). Each such driver's value becomes the turn whose
    /// relation is nearest to where its follower is, so that its start
    /// value is unwrapped near it.
    fn unwrap_drivers(&mut self) {
        let couplings: Vec<CouplingRec> = self
            .joints
            .iter()
            .filter_map(|j| j.coupling.clone())
            .collect();
        for c in couplings {
            let ((d, dk), (f, fk)) = (c.driver, c.follower);
            let m = self.joints[d].def.motions[dk];
            if !m.freedom.is_rotation() || m.target.is_some() || m.min.is_some() || m.max.is_some()
            {
                continue;
            }
            let (Some(x), Some(y)) = (
                self.start_values(d).get(dk).copied(),
                self.start_values(f).get(fk).copied(),
            ) else {
                continue;
            };
            let turning = self.joints[f].def.motions[fk].freedom.is_rotation();
            let off = |k: f64| {
                let e = y - c.relation.follow(x + k * TAU);
                if turning {
                    (e + PI).rem_euclid(TAU) - PI
                } else {
                    e
                }
                .abs()
            };
            // The nearest turn; of turns that place the follower alike
            // (within rounding), the least.
            let mut best = 0.0;
            for k in (1..=16).flat_map(|k| [k, -k]).map(f64::from) {
                if off(k) < off(best) - 1e-9 {
                    best = k;
                }
            }
            self.joints[d].values[dk] = x + best * TAU;
        }
    }

    /// Joint `j`'s start values where couplings make its motions follow
    /// (a follower without a target or whose target is let go): the
    /// relation at the driver's value (its start value when it comes
    /// later), within the follower's limits.
    fn follow_couplings(&mut self, j: usize, released: &[Vec<bool>]) {
        let followed: Vec<((usize, usize), usize, Relation)> = self
            .joints
            .iter()
            .filter_map(|rec| rec.coupling.as_ref())
            .filter(|c| c.follower.0 == j)
            .map(|c| (c.driver, c.follower.1, c.relation.clone()))
            .collect();
        for ((d, dk), k, relation) in followed {
            let m = self.joints[j].def.motions[k];
            if m.target.is_some() && !released[j][k] {
                continue;
            }
            let x = if d <= j {
                self.joints[d].values.get(dk).copied()
            } else {
                self.start_values(d).get(dk).copied()
            };
            let Some(x) = x else { continue };
            let mut v = relation.follow(x);
            if !v.is_finite() {
                continue;
            }
            if let Some(lo) = m.min {
                v = v.max(lo);
            }
            if let Some(hi) = m.max {
                v = v.min(hi);
            }
            self.joints[j].values[k] = v;
        }
    }

    /// A joint left about half a turn off, turned over: the body of side
    /// `a` (else `b`) turned so that the frames' rotations agree. False
    /// when every joint is less than a quarter turn off.
    fn turn_over(&mut self) -> bool {
        for j in 0..self.joints.len() {
            if self.joints[j].parts.is_some() {
                if self.turn_constraint_over(j) {
                    return true;
                }
                continue;
            }
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
            self.bodies[body.0 as usize].move_by(&turn);
            return true;
        }
        false
    }

    /// A constraint whose z axes (or x axes, for a twist) ended up pointing
    /// the wrong way, turned over: half a turn of side `a`'s body (else
    /// `b`'s) about b's x axis (z for a twist) through a's origin.
    fn turn_constraint_over(&mut self, j: usize) -> bool {
        let Some(parts) = &self.joints[j].parts else {
            return false;
        };
        let (fa, fb) = self.side_frames(j);
        let (za, xa) = (column(&fa, 2), column(&fa, 0));
        let (xb, yb, zb) = (column(&fb, 0), column(&fb, 1), column(&fb, 2));
        let mut axis = None;
        for part in parts {
            match part {
                ConstraintPart::Parallel(Sense::Same) if dot(za, zb) < -0.5 => axis = Some(xb),
                ConstraintPart::Parallel(Sense::Opposite) if dot(za, zb) > 0.5 => axis = Some(xb),
                ConstraintPart::Twist(t)
                    if dot(za, zb).abs() > 0.5
                        && dot(xa, xb) * t.cos() + dot(xa, yb) * t.sin() < -0.5 =>
                {
                    axis = Some(zb)
                }
                _ => continue,
            }
            break;
        }
        let Some(axis) = axis else {
            return false;
        };
        let def = &self.joints[j].def;
        let free = |b: Option<BodyId>| b.filter(|b| !self.bodies[b.0 as usize].fixed);
        let Some(body) = free(def.a).or_else(|| free(def.b)) else {
            return false;
        };
        let turn = Pose::rotation(fa.translation, axis, std::f64::consts::PI);
        self.bodies[body.0 as usize].move_by(&turn);
        true
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
        let (_, _, _, conflicts) = self.rank(mode, Hold::None);
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
        let (n, rank, redundant, conflicts) = self.rank(Mode::Joints, Hold::None);
        let body_dof: Vec<usize> = (0..self.bodies.len())
            .map(|b| {
                if self.bodies[b].fixed {
                    0
                } else {
                    let (n, rank, _, _) = self.rank(Mode::Joints, Hold::AllBut(b));
                    n - rank
                }
            })
            .collect();
        // A body without a motion of its own moves when holding it takes
        // a motion away from all of them.
        let body_mobile = (0..self.bodies.len())
            .map(|b| {
                if self.bodies[b].fixed {
                    false
                } else if body_dof[b] > 0 {
                    true
                } else {
                    let (held_n, held_rank, _, _) = self.rank(Mode::Joints, Hold::One(b));
                    held_n - held_rank < n - rank
                }
            })
            .collect();
        RigidAnalysis {
            dof: n - rank,
            body_dof,
            body_mobile,
            redundant,
            conflicts,
        }
    }
}
