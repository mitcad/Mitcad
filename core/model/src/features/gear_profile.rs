// SPDX-License-Identifier: MIT
//! The tooth geometry of gears (mitcad#472): the outlines a Gear feature
//! sweeps, worked out here without a geometry kernel.
//!
//! Teeth follow the ISO 53 basic rack (profile A): addendum 1 m, dedendum
//! 1.25 m, root fillet 0.38 m, the pressure angle given (20 degrees by
//! default). An external gear's tooth is what the generating rack leaves
//! as it rolls on the pitch circle: the flank is the involute of the base
//! circle, fitted as a B-spline within [`FIT_TOLERANCE`], and the root is
//! the envelope of the rack's tip fillet (the trochoid's offset), which
//! also cuts the undercut of small tooth counts into the flank. A helical
//! gear's transverse section is the transverse rack's; heights stay in the
//! normal module. An internal (ring) gear's space has the shape of an
//! external tooth, with a circular fillet at the root. A rack has the basic
//! rack's straight flanks. A straight bevel gear's teeth follow Tredgold's
//! approximation: the virtual spur gear on the back cone, wrapped onto it
//! and projected from the apex.
//!
//! Profiles are closed loops of named segments in a gear's own plane
//! coordinates: a gear's centre at the origin, tooth 1 centred at `phase`
//! from the x axis; a rack along x from 0, its reference (pitch) line on
//! y = 0 and its teeth pointing to +y. Segment names are the faces' roles:
//! `tip(k)`, `flank_a(k)` and `fillet_a(k)` on the clockwise side of tooth
//! k (seen along the axis), `flank_b(k)` and `fillet_b(k)` on its other side,
//! `root(k)` the root after it (k from 1).

use std::f64::consts::{PI, TAU};

use crate::profile::SegmentGeometry;

/// Cycloidal teeth (mitcad#2607).
pub mod cycloid;
// Named segments and points along segments, below the model with the
// kernel (mitcad#777).
pub use mitcad_model_base::segments::NamedSegment;
pub(crate) use mitcad_model_base::segments::{bspline_point, bspline_range, sample};

/// The basic rack's addendum, dedendum and root fillet radius, in modules.
pub const ADDENDUM: f64 = 1.0;
pub const DEDENDUM: f64 = 1.25;
pub const ROOT_FILLET: f64 = 0.38;

/// How closely the fitted B-splines follow their curves (mm).
pub const FIT_TOLERANCE: f64 = 1.0e-7;

/// The fewest teeth a gear may have.
pub const MIN_TEETH: f64 = 5.0;

/// inv(a) = tan(a) - a.
pub fn involute(angle: f64) -> f64 {
    angle.tan() - angle
}

/// The angle whose involute is `value` (>= 0).
pub fn involute_inverse(value: f64) -> f64 {
    if value <= 0.0 {
        return 0.0;
    }
    // inv(a) is about a^3 / 3 for small angles.
    let mut a = (3.0 * value).cbrt().min(1.5);
    for _ in 0..100 {
        let slope = a.tan().powi(2);
        if slope <= 0.0 {
            break;
        }
        let step = (involute(a) - value) / slope;
        a = (a - step).clamp(1.0e-12, 1.5);
        if step.abs() < 1.0e-15 {
            break;
        }
    }
    a
}

fn named(role: &str, index: usize, geometry: SegmentGeometry) -> NamedSegment {
    NamedSegment {
        name: format!("{role}({index})"),
        geometry,
    }
}

/// The values of a gear's teeth.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tooth {
    /// The normal module m (mm).
    pub module: f64,
    /// The number of teeth z (a virtual gear's need not be whole).
    pub teeth: f64,
    /// The normal pressure angle (radians).
    pub pressure_angle: f64,
    /// The helix angle at the pitch circle (radians); 0 for straight teeth.
    pub helix_angle: f64,
    /// The profile shift coefficient x: positive moves the profile away
    /// from the gear's axis (an internal gear's teeth outwards too).
    pub shift: f64,
    /// The backlash at the pitch circle (mm): a tooth is thinner (a space
    /// wider) by half of it, so two gears with the same value mesh with it.
    pub backlash: f64,
}

impl Tooth {
    pub fn transverse_module(&self) -> f64 {
        self.module / self.helix_angle.cos()
    }

    pub fn transverse_pressure_angle(&self) -> f64 {
        (self.pressure_angle.tan() / self.helix_angle.cos()).atan()
    }

    pub fn pitch_radius(&self) -> f64 {
        self.transverse_module() * self.teeth / 2.0
    }

    pub fn base_radius(&self) -> f64 {
        self.pitch_radius() * self.transverse_pressure_angle().cos()
    }

    /// The transverse circular pitch.
    pub fn pitch(&self) -> f64 {
        PI * self.transverse_module()
    }

    /// An external gear's tip and root radii.
    pub fn tip_radius(&self) -> f64 {
        self.pitch_radius() + self.module * (ADDENDUM + self.shift)
    }

    pub fn root_radius(&self) -> f64 {
        self.pitch_radius() - self.module * (DEDENDUM - self.shift)
    }

    /// An internal gear's tip radius (inside), at the base circle at least
    /// (no involute lies inside it).
    pub fn internal_tip_radius(&self) -> f64 {
        (self.pitch_radius() - self.module * (ADDENDUM - self.shift)).max(self.base_radius())
    }

    pub fn internal_root_radius(&self) -> f64 {
        self.pitch_radius() + self.module * (DEDENDUM + self.shift)
    }

    /// An external tooth's thickness at the pitch circle (transverse arc).
    pub fn thickness(&self) -> f64 {
        self.pitch() / 2.0 + 2.0 * self.shift * self.module * self.transverse_pressure_angle().tan()
            - self.backlash / 2.0
    }

    /// An internal gear's space width at the pitch circle.
    pub fn space_width(&self) -> f64 {
        self.pitch() / 2.0
            + 2.0 * self.shift * self.module * self.transverse_pressure_angle().tan()
            + self.backlash / 2.0
    }

    /// The lead of a helical gear's teeth (mm per turn); infinite for
    /// straight teeth.
    pub fn lead(&self) -> f64 {
        if self.helix_angle == 0.0 {
            f64::INFINITY
        } else {
            TAU * self.pitch_radius() / self.helix_angle.tan().abs()
        }
    }

    /// The values a gear can be made with.
    pub fn check(&self) -> Result<(), String> {
        let finite = [
            self.module,
            self.teeth,
            self.pressure_angle,
            self.helix_angle,
            self.shift,
            self.backlash,
        ]
        .iter()
        .all(|v| v.is_finite());
        if !finite {
            return Err("the gear's values must be finite numbers".to_owned());
        }
        if self.module <= 0.0 {
            return Err(format!(
                "the module must be greater than zero, got {}",
                self.module
            ));
        }
        if self.teeth < MIN_TEETH {
            return Err(format!(
                "a gear needs at least {MIN_TEETH} teeth, got {}",
                self.teeth
            ));
        }
        let degrees = self.pressure_angle.to_degrees();
        if !(10.0..=35.0).contains(&degrees) {
            return Err(format!(
                "the pressure angle must be between 10 and 35 degrees, got {degrees:.4} degrees"
            ));
        }
        if self.helix_angle.abs().to_degrees() > 45.0 {
            return Err(format!(
                "the helix angle must be at most 45 degrees, got {:.4} degrees",
                self.helix_angle.abs().to_degrees()
            ));
        }
        if !(-1.0..=0.8).contains(&self.shift) {
            return Err(format!(
                "the profile shift must be between -1 and 0.8, got {}",
                self.shift
            ));
        }
        if self.backlash < 0.0 || self.backlash > self.pitch() / 4.0 {
            return Err(format!(
                "the backlash must be between 0 and a quarter of the pitch ({:.4} mm), got {}",
                self.pitch() / 4.0,
                self.backlash
            ));
        }
        if self.root_radius() <= 0.0 {
            return Err("the gear has too few teeth for its profile shift".to_owned());
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Curves

pub(crate) fn rotate(p: [f64; 2], angle: f64) -> [f64; 2] {
    let (s, c) = angle.sin_cos();
    [p[0] * c - p[1] * s, p[0] * s + p[1] * c]
}

/// A segment mapped by a rotation about the origin, mirrored first in the
/// x axis when `mirror`.
pub(crate) fn placed(segment: &SegmentGeometry, angle: f64, mirror: bool) -> SegmentGeometry {
    let map = |p: [f64; 2]| rotate(if mirror { [p[0], -p[1]] } else { p }, angle);
    match segment {
        SegmentGeometry::Line { start, end } => SegmentGeometry::Line {
            start: map(*start),
            end: map(*end),
        },
        SegmentGeometry::Arc {
            center,
            radius,
            start_angle,
            end_angle,
        } => {
            let (start_angle, end_angle) = if mirror {
                (-end_angle, -start_angle)
            } else {
                (*start_angle, *end_angle)
            };
            SegmentGeometry::Arc {
                center: map(*center),
                radius: *radius,
                start_angle: start_angle + angle,
                end_angle: end_angle + angle,
            }
        }
        SegmentGeometry::BSpline {
            degree,
            poles,
            weights,
            knots,
            multiplicities,
            periodic,
        } => SegmentGeometry::BSpline {
            degree: *degree,
            poles: poles.iter().map(|p| map(*p)).collect(),
            weights: weights.clone(),
            knots: knots.clone(),
            multiplicities: multiplicities.clone(),
            periodic: *periodic,
        },
        other => other.clone(),
    }
}

/// A segment moved by `offset`.
pub(crate) fn moved(segment: &SegmentGeometry, offset: [f64; 2]) -> SegmentGeometry {
    let map = |p: [f64; 2]| [p[0] + offset[0], p[1] + offset[1]];
    match segment {
        SegmentGeometry::Line { start, end } => SegmentGeometry::Line {
            start: map(*start),
            end: map(*end),
        },
        SegmentGeometry::Arc {
            center,
            radius,
            start_angle,
            end_angle,
        } => SegmentGeometry::Arc {
            center: map(*center),
            radius: *radius,
            start_angle: *start_angle,
            end_angle: *end_angle,
        },
        SegmentGeometry::BSpline {
            degree,
            poles,
            weights,
            knots,
            multiplicities,
            periodic,
        } => SegmentGeometry::BSpline {
            degree: *degree,
            poles: poles.iter().map(|p| map(*p)).collect(),
            weights: weights.clone(),
            knots: knots.clone(),
            multiplicities: multiplicities.clone(),
            periodic: *periodic,
        },
        other => other.clone(),
    }
}

/// A cubic B-spline through `f` on [a, b] within `tolerance`: a C1 chain
/// of cubic Hermite pieces (double interior knots), split until every
/// piece is within the tolerance at its quarter points. Tangents are
/// central differences of `f`, which must be defined a little beyond the
/// ends.
pub(crate) fn fit(f: &dyn Fn(f64) -> [f64; 2], a: f64, b: f64, tolerance: f64) -> SegmentGeometry {
    let at = |s: f64| f(a + (b - a) * s);
    const H: f64 = 1.0e-5;
    // Central differences inside, one-sided ones of the same order at the
    // ends (a curve need not go on past them).
    let tangent = |s: f64| {
        let d = |a: [f64; 2], b: [f64; 2], c: [f64; 2], sign: f64| {
            std::array::from_fn::<f64, 2, _>(|i| {
                sign * (-3.0 * a[i] + 4.0 * b[i] - c[i]) / (2.0 * H)
            })
        };
        if s < H {
            d(at(s), at(s + H), at(s + 2.0 * H), 1.0)
        } else if s > 1.0 - H {
            d(at(s), at(s - H), at(s - 2.0 * H), -1.0)
        } else {
            let p = at(s + H);
            let q = at(s - H);
            [(p[0] - q[0]) / (2.0 * H), (p[1] - q[1]) / (2.0 * H)]
        }
    };
    let bezier = |s0: f64, s1: f64| {
        let (p0, p3) = (at(s0), at(s1));
        let (d0, d1) = (tangent(s0), tangent(s1));
        let h = (s1 - s0) / 3.0;
        [
            p0,
            [p0[0] + d0[0] * h, p0[1] + d0[1] * h],
            [p3[0] - d1[0] * h, p3[1] - d1[1] * h],
            p3,
        ]
    };
    let eval = |c: &[[f64; 2]; 4], t: f64| {
        let u = 1.0 - t;
        let w = [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t];
        [
            w[0] * c[0][0] + w[1] * c[1][0] + w[2] * c[2][0] + w[3] * c[3][0],
            w[0] * c[0][1] + w[1] * c[1][1] + w[2] * c[2][1] + w[3] * c[3][1],
        ]
    };
    let mut knots = vec![0.0, 0.5, 1.0];
    for _ in 0..16 {
        let mut refined = vec![knots[0]];
        let mut split = false;
        for pair in knots.windows(2) {
            let (s0, s1) = (pair[0], pair[1]);
            let c = bezier(s0, s1);
            let error = [0.25, 0.5, 0.75]
                .iter()
                .map(|t| {
                    let p = eval(&c, *t);
                    let q = at(s0 + (s1 - s0) * t);
                    (p[0] - q[0]).hypot(p[1] - q[1])
                })
                .fold(0.0, f64::max);
            if error > tolerance {
                refined.push((s0 + s1) / 2.0);
                split = true;
            }
            refined.push(s1);
        }
        knots = refined;
        if !split {
            break;
        }
    }
    let mut poles = Vec::with_capacity(2 * knots.len());
    for (i, pair) in knots.windows(2).enumerate() {
        let c = bezier(pair[0], pair[1]);
        if i == 0 {
            poles.push(c[0]);
        }
        poles.push(c[1]);
        poles.push(c[2]);
    }
    poles.push(at(1.0));
    let mut multiplicities = vec![2; knots.len()];
    multiplicities[0] = 4;
    *multiplicities.last_mut().unwrap() = 4;
    SegmentGeometry::BSpline {
        degree: 3,
        poles,
        weights: Vec::new(),
        knots,
        multiplicities,
        periodic: false,
    }
}

/// The ends of a segment.
#[cfg(test)]
pub(crate) fn ends(segment: &SegmentGeometry) -> [[f64; 2]; 2] {
    let points = sample(segment, 1);
    [points[0], points[points.len() - 1]]
}

// ---------------------------------------------------------------------------
// External teeth

/// Half of an external tooth in tooth coordinates: the tooth along +x, this
/// half on the -y side (the clockwise flank), from the root to the tip.
#[derive(Debug, Clone)]
pub struct HalfTooth {
    /// The root fillet the rack's tip rounds, from the root circle up.
    pub fillet: SegmentGeometry,
    /// The involute flank up to the tip circle.
    pub flank: SegmentGeometry,
    /// The angle from the tooth's middle to the fillet's start on the root
    /// circle, and to the flank's end on the tip circle.
    pub root_angle: f64,
    pub tip_angle: f64,
    /// Where the involute starts (the form radius).
    pub form_radius: f64,
    /// The rack's fillet cuts into the involute (an undercut tooth).
    pub undercut: bool,
}

/// The generating rack in its own coordinates: u along it (the gear's
/// tooth in the middle of its space at u = 0), w out from the gear (w = 0
/// its reference line); the tip fillet of its tooth right of the space.
struct GeneratingRack {
    radius: f64,
    shift: f64,
    center: [f64; 2],
    fillet: f64,
    alpha: f64,
}

impl GeneratingRack {
    fn new(t: &Tooth) -> Result<Self, String> {
        let m = t.module;
        let alpha = t.transverse_pressure_angle();
        let (sin, cos) = alpha.sin_cos();
        // The flank of the rack's tooth meets its reference line at u0: the
        // space is the gear tooth's thickness there.
        let u0 = t.pitch() / 4.0 - t.backlash / 4.0;
        let tip = -DEDENDUM * m;
        // The largest fillet that fits the tooth's tip (two fillets meeting
        // in its middle).
        let largest = ((t.pitch() / 2.0 - u0) * cos - DEDENDUM * m * sin) / (1.0 - sin);
        let fillet = (ROOT_FILLET * m).min(largest);
        if fillet <= 0.0 {
            return Err("the rack's teeth have no room for their root fillet".to_owned());
        }
        let wc = tip + fillet;
        let uc = u0 + (fillet - wc * sin) / cos;
        Ok(Self {
            radius: t.pitch_radius(),
            shift: t.shift * m,
            center: [uc, wc],
            fillet,
            alpha,
        })
    }

    /// The rack's position (the gear turned by `phi`) where the fillet
    /// starts cutting the root, and where it meets the flank.
    fn range(&self) -> (f64, f64) {
        let [uc, wc] = self.center;
        let start = -uc / self.radius;
        let end = -(uc + (-self.shift - wc) / self.alpha.tan()) / self.radius;
        (start, end)
    }

    /// The point the fillet cuts at rack position `phi`, in tooth
    /// coordinates.
    fn envelope(&self, phi: f64) -> [f64; 2] {
        let [uc, wc] = self.center;
        let r = self.radius;
        // The normal at the contact passes through the pitch point.
        let d = [-r * phi - uc, -self.shift - wc];
        let length = d[0].hypot(d[1]);
        let q = [
            uc - self.fillet * d[0] / length,
            wc - self.fillet * d[1] / length,
        ];
        let x = q[0] + r * phi;
        let y = r + self.shift + q[1];
        let (s, c) = phi.sin_cos();
        let g = [x * c - y * s, x * s + y * c];
        [g[1], -g[0]]
    }

    /// Whether the straight flank starts generating below the base circle.
    fn undercuts(&self) -> bool {
        let w_flank = self.center[1] - self.fillet * self.alpha.sin();
        -(self.shift + w_flank) > self.radius * self.alpha.sin().powi(2) + 1.0e-12
    }
}

/// The half-angle of an involute tooth (or space) at radius `r`: `half`
/// at the pitch circle.
fn involute_half(half: f64, alpha: f64, base: f64, r: f64) -> f64 {
    half + involute(alpha) - involute((base / r).min(1.0).acos())
}

/// A point of an involute half, in tooth coordinates, by its roll t
/// (r = base sqrt(1 + t^2)).
fn involute_point(half: f64, alpha: f64, base: f64, t: f64) -> [f64; 2] {
    let r = base * (1.0 + t * t).sqrt();
    let h = half + involute(alpha) - (t - t.atan());
    [r * h.cos(), -r * h.sin()]
}

fn roll(base: f64, r: f64) -> f64 {
    ((r / base).powi(2) - 1.0).max(0.0).sqrt()
}

fn half_angle_of(p: [f64; 2]) -> f64 {
    (-p[1]).atan2(p[0])
}

fn radius_of(p: [f64; 2]) -> f64 {
    p[0].hypot(p[1])
}

/// Half of an external tooth.
pub fn external_half(t: &Tooth) -> Result<HalfTooth, String> {
    let rack = GeneratingRack::new(t)?;
    if -rack.shift - rack.center[1] <= 0.0 {
        return Err("the profile shift is too large for the root fillet".to_owned());
    }
    let r = t.pitch_radius();
    let base = t.base_radius();
    let alpha = t.transverse_pressure_angle();
    let tip = t.tip_radius();
    let half = t.thickness() / (2.0 * r);
    let (start, end) = rack.range();
    let undercut = rack.undercuts();
    let gap = |phi: f64| {
        let p = rack.envelope(phi);
        let radius = radius_of(p);
        if radius < base {
            -1.0
        } else {
            half_angle_of(p) - involute_half(half, alpha, base, radius)
        }
    };
    let crossing = if undercut {
        // The fillet's envelope cuts the involute: where it reaches it
        // first from the root.
        const STEPS: usize = 2000;
        let mut previous = start;
        let mut found = None;
        for i in 1..=STEPS {
            let phi = start + (end - start) * i as f64 / STEPS as f64;
            if gap(phi) >= 0.0 {
                found = Some((previous, phi));
                break;
            }
            previous = phi;
        }
        let (mut lo, mut hi) =
            found.ok_or("the undercut of so few teeth leaves no involute flank")?;
        for _ in 0..200 {
            let mid = (lo + hi) / 2.0;
            if gap(mid) >= 0.0 {
                hi = mid;
            } else {
                lo = mid;
            }
        }
        hi
    } else {
        end
    };
    let joint = rack.envelope(crossing);
    let form_radius = radius_of(joint);
    if form_radius >= tip {
        return Err("the teeth have no involute flank below their tip".to_owned());
    }
    // The fillet's points climb from the root to the joint.
    let mut last = 0.0;
    for i in 0..=64 {
        let p = rack.envelope(start + (crossing - start) * i as f64 / 64.0);
        let radius = radius_of(p);
        if radius < last - 1.0e-9 {
            return Err("the root fillet folds over: the gear has too few teeth".to_owned());
        }
        last = radius;
    }
    let tip_angle = involute_half(half, alpha, base, tip);
    if tip_angle <= 0.0 {
        return Err(format!(
            "the teeth are pointed below the outside diameter ({:.4} mm): reduce the profile shift",
            2.0 * tip
        ));
    }
    let fillet = fit(&|phi| rack.envelope(phi), start, crossing, FIT_TOLERANCE);
    let (t0, t1) = (roll(base, form_radius), roll(base, tip));
    let flank = fit(
        &|s| involute_point(half, alpha, base, s),
        t0,
        t1,
        FIT_TOLERANCE,
    );
    Ok(HalfTooth {
        fillet,
        flank,
        root_angle: -start,
        tip_angle,
        form_radius,
        undercut,
    })
}

/// The closed outline of an external gear, tooth 1 centred at `phase`.
pub fn external_outline(t: &Tooth, phase: f64) -> Result<Vec<NamedSegment>, String> {
    let half = external_half(t)?;
    let z = t.teeth.round() as usize;
    let step = TAU / z as f64;
    let (tip, root) = (t.tip_radius(), t.root_radius());
    let mut segments = Vec::with_capacity(6 * z);
    for i in 0..z {
        let k = i + 1;
        let c = phase + step * i as f64;
        segments.push(named("fillet_a", k, placed(&half.fillet, c, false)));
        segments.push(named("flank_a", k, placed(&half.flank, c, false)));
        segments.push(named(
            "tip",
            k,
            SegmentGeometry::Arc {
                center: [0.0, 0.0],
                radius: tip,
                start_angle: c - half.tip_angle,
                end_angle: c + half.tip_angle,
            },
        ));
        segments.push(named("flank_b", k, placed(&half.flank, c, true)));
        segments.push(named("fillet_b", k, placed(&half.fillet, c, true)));
        let (from, to) = (c + half.root_angle, c + step - half.root_angle);
        if to - from > 1.0e-9 {
            segments.push(named(
                "root",
                k,
                SegmentGeometry::Arc {
                    center: [0.0, 0.0],
                    radius: root,
                    start_angle: from,
                    end_angle: to,
                },
            ));
        }
    }
    Ok(segments)
}

// ---------------------------------------------------------------------------
// Internal teeth

/// The space of an internal gear in space coordinates (the space along
/// +x), the -y side from the tip circle out: the flank, the root fillet,
/// and the angles of the flank's start at the tip circle and of the
/// fillet's end on the root circle.
struct HalfSpace {
    flank: SegmentGeometry,
    fillet: SegmentGeometry,
    tip_angle: f64,
    root_angle: f64,
}

fn internal_half(t: &Tooth) -> Result<HalfSpace, String> {
    let r = t.pitch_radius();
    let base = t.base_radius();
    let alpha = t.transverse_pressure_angle();
    let half = t.space_width() / (2.0 * r);
    let (tip, root) = (t.internal_tip_radius(), t.internal_root_radius());
    let flank_at = |radius: f64| {
        let h = involute_half(half, alpha, base, radius);
        [radius * h.cos(), -radius * h.sin()]
    };
    let root_half = involute_half(half, alpha, base, root);
    if root_half <= 0.0 {
        return Err("the ring gear's spaces close before its root diameter".to_owned());
    }
    // The nearest point of the flank to `c` (the flank is far from
    // straight only near the base circle).
    let nearest = |c: [f64; 2]| {
        let (mut lo, mut hi) = (tip, root);
        let distance = |radius: f64| {
            let p = flank_at(radius);
            (p[0] - c[0]).hypot(p[1] - c[1])
        };
        for _ in 0..200 {
            let a = lo + (hi - lo) * 0.381_966_011_250_105;
            let b = hi - (hi - lo) * 0.381_966_011_250_105;
            if distance(a) < distance(b) {
                hi = b;
            } else {
                lo = a;
            }
        }
        let radius = (lo + hi) / 2.0;
        (radius, distance(radius))
    };
    // A fillet of radius rho in the corner: its centre on the circle of
    // root - rho at the angle where it is rho from the flank.
    let place = |rho: f64| {
        let rc = root - rho;
        let center = |gamma: f64| [rc * gamma.cos(), -rc * gamma.sin()];
        let gap = |gamma: f64| nearest(center(gamma)).1 - rho;
        let (mut lo, mut hi) = (0.0, involute_half(half, alpha, base, rc).max(0.0));
        if gap(lo) < 0.0 {
            return None;
        }
        for _ in 0..200 {
            let mid = (lo + hi) / 2.0;
            if gap(mid) > 0.0 {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        Some((lo, center(lo)))
    };
    let mut rho = ROOT_FILLET * t.module;
    let (gamma, center) = match place(rho) {
        Some(found) => found,
        None => {
            // The fillets of both sides meet in the space's middle.
            let (mut lo, mut hi) = (0.0, rho);
            for _ in 0..100 {
                let mid = (lo + hi) / 2.0;
                if place(mid).is_some() {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            rho = lo;
            place(rho).ok_or("the ring gear's spaces have no room for a root fillet")?
        }
    };
    let (contact, _) = nearest(center);
    if contact <= tip {
        return Err("the ring gear's root fillet reaches its tip".to_owned());
    }
    let q = flank_at(contact);
    let a = (q[1] - center[1]).atan2(q[0] - center[0]);
    let b = -gamma;
    // The short way round from the flank to the root circle.
    let (mut from, mut to) = (a.min(b), a.max(b));
    if to - from > PI {
        std::mem::swap(&mut from, &mut to);
        to += TAU;
    }
    let fillet = SegmentGeometry::Arc {
        center,
        radius: rho,
        start_angle: from,
        end_angle: to,
    };
    let flank = fit(
        &|s| involute_point(half, alpha, base, s),
        roll(base, tip),
        roll(base, contact),
        FIT_TOLERANCE,
    );
    Ok(HalfSpace {
        flank,
        fillet,
        tip_angle: involute_half(half, alpha, base, tip),
        root_angle: gamma,
    })
}

/// The toothed loop of an internal gear (a hole of its rim), tooth 1
/// centred at `phase`.
pub fn internal_outline(t: &Tooth, phase: f64) -> Result<Vec<NamedSegment>, String> {
    let half = internal_half(t)?;
    let z = t.teeth.round() as usize;
    let step = TAU / z as f64;
    let tooth_half = step / 2.0 - half.tip_angle;
    if tooth_half <= 0.0 {
        return Err("the ring gear's teeth are pointed at their tips".to_owned());
    }
    let (tip, root) = (t.internal_tip_radius(), t.internal_root_radius());
    let mut segments = Vec::with_capacity(6 * z);
    for i in 0..z {
        let k = i + 1;
        let next = if k == z { 1 } else { k + 1 };
        let c = phase + step * i as f64;
        let space = c + step / 2.0;
        segments.push(named(
            "tip",
            k,
            SegmentGeometry::Arc {
                center: [0.0, 0.0],
                radius: tip,
                start_angle: c - tooth_half,
                end_angle: c + tooth_half,
            },
        ));
        segments.push(named("flank_b", k, placed(&half.flank, space, false)));
        segments.push(named("fillet_b", k, placed(&half.fillet, space, false)));
        if half.root_angle > 1.0e-9 {
            segments.push(named(
                "root",
                k,
                SegmentGeometry::Arc {
                    center: [0.0, 0.0],
                    radius: root,
                    start_angle: space - half.root_angle,
                    end_angle: space + half.root_angle,
                },
            ));
        }
        segments.push(named("fillet_a", next, placed(&half.fillet, space, true)));
        segments.push(named("flank_a", next, placed(&half.flank, space, true)));
    }
    Ok(segments)
}

// ---------------------------------------------------------------------------
// Racks

/// A rack's values: the teeth's (its `teeth` count and transverse pitch),
/// its height from its back to its tips, and its length (at least the
/// teeth's pitch line).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rack {
    pub tooth: Tooth,
    pub height: f64,
    pub length: f64,
}

impl Rack {
    /// The length of the toothed pitch line: teeth times the pitch.
    pub fn pitch_length(&self) -> f64 {
        self.tooth.teeth.round() * self.tooth.pitch()
    }

    pub fn check(&self) -> Result<(), String> {
        let m = self.tooth.module;
        if self.tooth.teeth < 1.0 {
            return Err("a rack needs at least one tooth".to_owned());
        }
        if self.height.is_nan() || self.height <= (ADDENDUM + DEDENDUM) * m {
            return Err(format!(
                "the rack's height must be more than its teeth's depth ({} mm), got {}",
                (ADDENDUM + DEDENDUM) * m,
                self.height
            ));
        }
        if self.length < self.pitch_length() - 1.0e-9 {
            return Err(format!(
                "the rack's length must be at least its teeth's pitch line ({:.4} mm), got {}",
                self.pitch_length(),
                self.length
            ));
        }
        Ok(())
    }
}

/// The closed outline of a rack: its back on y = height above the tips
/// below them, tooth k centred at x = (k - 1/2) p, the ends at x = 0 and
/// x = length. Roles: `back`, `end_a` (x = 0), `end_b`, and the teeth's.
pub fn rack_outline(rack: &Rack) -> Result<Vec<NamedSegment>, String> {
    rack.check()?;
    let t = &rack.tooth;
    let m = t.module;
    let p = t.pitch();
    let alpha = t.transverse_pressure_angle();
    let (sin, cos) = alpha.sin_cos();
    let tan = alpha.tan();
    let n = t.teeth.round() as usize;
    let tip_y = ADDENDUM * m;
    let root_y = -DEDENDUM * m;
    let bottom = tip_y - rack.height;
    // Half a tooth's width at the reference line, and at height y.
    let hw0 = p / 4.0 - t.backlash / 4.0;
    let hw = |y: f64| hw0 - y * tan;
    if hw(tip_y) <= 0.0 {
        return Err("the rack's teeth are pointed".to_owned());
    }
    // The root fillet left of tooth centred at 0: tangent to the root line
    // and to the flank x = -hw(y).
    let fillet_for = |rho: f64| {
        let yc = root_y + rho;
        let yl = yc - rho * sin;
        let xc = -hw0 + yl * tan - rho * cos;
        (xc, [-hw0 + yl * tan, yl], [xc, yc])
    };
    let mut rho = ROOT_FILLET * m;
    if fillet_for(rho).0 < -p / 2.0 {
        // The fillets of a space meet in its middle.
        let (mut lo, mut hi) = (0.0, ROOT_FILLET * m);
        for _ in 0..100 {
            let mid = (lo + hi) / 2.0;
            if fillet_for(mid).0 >= -p / 2.0 {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        rho = lo;
    }
    let (xc, flank_foot, center) = fillet_for(rho);
    let foot_angle = (flank_foot[1] - center[1]).atan2(flank_foot[0] - center[0]);
    // Left of a tooth centred at 0: the fillet from the root line (angle
    // -90 degrees about its centre) to the flank's foot.
    let left_fillet = SegmentGeometry::Arc {
        center,
        radius: rho,
        start_angle: -PI / 2.0,
        end_angle: foot_angle,
    };
    let left_flank = SegmentGeometry::Line {
        start: flank_foot,
        end: [-hw(tip_y), tip_y],
    };
    let mirror = |s: &SegmentGeometry| match s {
        SegmentGeometry::Line { start, end } => SegmentGeometry::Line {
            start: [-end[0], end[1]],
            end: [-start[0], start[1]],
        },
        SegmentGeometry::Arc {
            center,
            radius,
            start_angle,
            end_angle,
        } => SegmentGeometry::Arc {
            center: [-center[0], center[1]],
            radius: *radius,
            start_angle: PI - end_angle,
            end_angle: PI - start_angle,
        },
        other => other.clone(),
    };
    let right_fillet = mirror(&left_fillet);
    let right_flank = mirror(&left_flank);
    // Along the top from x = 0 to the length; the roots between teeth.
    let mut top = Vec::new();
    let mut x = 0.0;
    for i in 0..n {
        let k = i + 1;
        let c = p * (i as f64 + 0.5);
        let start = c + xc;
        if start - x > 1.0e-9 {
            top.push(named(
                "root",
                i,
                SegmentGeometry::Line {
                    start: [x, root_y],
                    end: [start, root_y],
                },
            ));
        }
        top.push(named("fillet_a", k, moved(&left_fillet, [c, 0.0])));
        top.push(named("flank_a", k, moved(&left_flank, [c, 0.0])));
        top.push(named(
            "tip",
            k,
            SegmentGeometry::Line {
                start: [c - hw(tip_y), tip_y],
                end: [c + hw(tip_y), tip_y],
            },
        ));
        top.push(named("flank_b", k, moved(&right_flank, [c, 0.0])));
        top.push(named("fillet_b", k, moved(&right_fillet, [c, 0.0])));
        x = c - xc;
    }
    if rack.length - x > 1.0e-9 {
        top.push(named(
            "root",
            n,
            SegmentGeometry::Line {
                start: [x, root_y],
                end: [rack.length, root_y],
            },
        ));
    }
    let mut segments = vec![
        NamedSegment {
            name: "back".to_owned(),
            geometry: SegmentGeometry::Line {
                start: [0.0, bottom],
                end: [rack.length, bottom],
            },
        },
        NamedSegment {
            name: "end_b".to_owned(),
            geometry: SegmentGeometry::Line {
                start: [rack.length, bottom],
                end: [rack.length, root_y],
            },
        },
    ];
    segments.extend(top.into_iter().rev());
    segments.push(NamedSegment {
        name: "end_a".to_owned(),
        geometry: SegmentGeometry::Line {
            start: [0.0, root_y],
            end: [0.0, bottom],
        },
    });
    Ok(segments)
}

// ---------------------------------------------------------------------------
// Bevel gears

/// A straight bevel gear's values (ISO 23509 nomenclature): the module and
/// teeth at the heel (the outer end), the pitch angle delta, the face width
/// along the pitch cone.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bevel {
    pub tooth: Tooth,
    pub pitch_angle: f64,
    pub face_width: f64,
}

/// What a bevel gear is made of: its heel section seen from the apex side,
/// in a plane square to the axis, and the solid's other values. Heights
/// along the axis are measured from that plane towards the apex.
#[derive(Debug, Clone)]
pub struct BevelSection {
    pub outline: Vec<NamedSegment>,
    /// The apex's height above the section's plane.
    pub apex: f64,
    /// The toe section: the heel's scaled about the apex.
    pub scale: f64,
    /// The blank turned about the axis, in (radius, height) coordinates:
    /// `back_cone`, `back`, `front`, `front_cone` and the rest outside the
    /// teeth.
    pub blank: Vec<NamedSegment>,
}

impl Bevel {
    /// The outer cone distance R_e.
    pub fn cone_distance(&self) -> f64 {
        self.tooth.pitch_radius() / self.pitch_angle.sin()
    }

    /// The pitch angle of a gear of `teeth` meeting one of `mate` teeth at
    /// `shaft_angle`.
    pub fn pitch_angle_of(teeth: f64, mate: f64, shaft_angle: f64) -> f64 {
        shaft_angle.sin().atan2(mate / teeth + shaft_angle.cos())
    }

    pub fn check(&self) -> Result<(), String> {
        self.tooth.check()?;
        if self.tooth.helix_angle != 0.0 {
            return Err("a straight bevel gear has no helix angle".to_owned());
        }
        if !(self.pitch_angle > 0.0 && self.pitch_angle < PI / 2.0 + 1.0e-12) {
            return Err(format!(
                "the bevel gear's pitch angle must be between 0 and 90 degrees, got {:.4} degrees",
                self.pitch_angle.to_degrees()
            ));
        }
        if self.face_width <= 0.0 || self.face_width >= self.cone_distance() / 2.0 {
            return Err(format!(
                "the bevel gear's face width must be less than half its cone distance ({:.4} mm), got {}",
                self.cone_distance() / 2.0,
                self.face_width
            ));
        }
        Ok(())
    }

    /// The heel section, tooth 1 centred at `phase`.
    pub fn section(&self, phase: f64) -> Result<BevelSection, String> {
        self.check()?;
        let delta = self.pitch_angle;
        let (sin_d, cos_d) = delta.sin_cos();
        let t = &self.tooth;
        let m = t.module;
        let re = self.cone_distance();
        // Tredgold: the virtual spur gear on the back cone, of radius
        // r / cos(delta).
        let virtual_tooth = Tooth {
            teeth: t.teeth / cos_d,
            ..*t
        };
        let half = external_half(&virtual_tooth)?;
        let rv = virtual_tooth.pitch_radius();
        let (rho_tip, rho_root) = (virtual_tooth.tip_radius(), virtual_tooth.root_radius());
        // Heights along the axis from the apex towards the heel: the back
        // cone's apex, and the virtual gear's points on the back cone.
        let back_apex = re * cos_d + rv * sin_d;
        let height = |rho: f64| back_apex - rho * sin_d;
        let toe = (re - self.face_width) / re;
        // The section planes: past every point of the heel and the toe.
        let margin = 0.05 * m;
        let section = height(rho_root) + margin;
        let toe_section = toe * height(rho_tip) - margin;
        if toe_section <= 0.0 {
            return Err("the bevel gear's teeth reach its apex".to_owned());
        }
        // A point of the virtual gear (its centre at the origin, the tooth
        // along +x) on the section plane.
        let map = |p: [f64; 2], c: f64| {
            let rho = radius_of(p);
            let psi = p[1].atan2(p[0]);
            let radius = rho * cos_d * section / height(rho);
            let a = c + psi / cos_d;
            [radius * a.cos(), radius * a.sin()]
        };
        let wrap = |segment: &SegmentGeometry, c: f64, mirror: bool| {
            let (a, b) = bspline_range(segment).unwrap_or((0.0, 1.0));
            fit(
                &|s| {
                    let p = bspline_point(segment, s).unwrap_or([0.0, 0.0]);
                    map(if mirror { [p[0], -p[1]] } else { p }, c)
                },
                a,
                b,
                FIT_TOLERANCE,
            )
        };
        let circle = |rho: f64, from: f64, to: f64, c: f64| SegmentGeometry::Arc {
            center: [0.0, 0.0],
            radius: rho * cos_d * section / height(rho),
            start_angle: c + from / cos_d,
            end_angle: c + to / cos_d,
        };
        let z = t.teeth.round() as usize;
        let step = TAU / z as f64;
        let mut outline = Vec::with_capacity(6 * z);
        let virtual_step = TAU / virtual_tooth.teeth;
        for i in 0..z {
            let k = i + 1;
            let c = phase + step * i as f64;
            outline.push(named("fillet_a", k, wrap(&half.fillet, c, false)));
            outline.push(named("flank_a", k, wrap(&half.flank, c, false)));
            outline.push(named(
                "tip",
                k,
                circle(rho_tip, -half.tip_angle, half.tip_angle, c),
            ));
            outline.push(named("flank_b", k, wrap(&half.flank, c, true)));
            outline.push(named("fillet_b", k, wrap(&half.fillet, c, true)));
            let (from, to) = (half.root_angle, virtual_step - half.root_angle);
            if to - from > 1.0e-9 {
                outline.push(named("root", k, circle(rho_root, from, to, c)));
            }
        }
        // The blank about the axis: between the back cone through the heel
        // and the front cone through the toe (the back cone scaled about
        // the apex), out past the tips. (radius, height above the section
        // plane towards the apex.)
        let out = rho_tip + 2.0 * m;
        let point = |scale: f64, rho: f64| [scale * rho * cos_d, section - scale * height(rho)];
        let back_axis = [0.0, section - back_apex];
        let back_out = point(1.0, out);
        let front_out = point(toe, out);
        let front_axis = [0.0, section - toe * back_apex];
        let line = |name: &str, start: [f64; 2], end: [f64; 2]| NamedSegment {
            name: name.to_owned(),
            geometry: SegmentGeometry::Line { start, end },
        };
        let blank = vec![
            line("back_cone", back_axis, back_out),
            line("outside", back_out, front_out),
            line("front_cone", front_out, front_axis),
            line("axis", front_axis, back_axis),
        ];
        Ok(BevelSection {
            outline,
            apex: section,
            scale: toe_section / section,
            blank,
        })
    }
}

// ---------------------------------------------------------------------------
// Bores and keyways

/// A keyway's width and its depth into the hub (DIN 6885-1, form A) for a
/// shaft of `diameter`, or None outside the table (6 to 230 mm).
pub fn keyway(diameter: f64) -> Option<(f64, f64)> {
    // (largest shaft diameter, key width b, hub depth t2)
    const TABLE: [(f64, f64, f64); 20] = [
        (8.0, 2.0, 1.0),
        (10.0, 3.0, 1.4),
        (12.0, 4.0, 1.8),
        (17.0, 5.0, 2.3),
        (22.0, 6.0, 2.8),
        (30.0, 8.0, 3.3),
        (38.0, 10.0, 3.3),
        (44.0, 12.0, 3.3),
        (50.0, 14.0, 3.8),
        (58.0, 16.0, 4.3),
        (65.0, 18.0, 4.4),
        (75.0, 20.0, 4.9),
        (85.0, 22.0, 5.4),
        (95.0, 25.0, 5.4),
        (110.0, 28.0, 6.4),
        (130.0, 32.0, 7.4),
        (150.0, 36.0, 8.4),
        (170.0, 40.0, 9.4),
        (200.0, 45.0, 10.4),
        (230.0, 50.0, 11.4),
    ];
    if diameter.is_nan() || diameter <= 6.0 {
        return None;
    }
    TABLE
        .iter()
        .find(|(largest, _, _)| diameter <= *largest + 1.0e-9)
        .map(|(_, width, depth)| (*width, *depth))
}

/// A bore's loop: a circle of `diameter` (`bore`), with the keyway cut
/// into it at `angle` from the x axis when given (`keyway_a`, `keyway_top`,
/// `keyway_b`).
pub fn bore_outline(diameter: f64, keyway_at: Option<(f64, f64, f64)>) -> Vec<NamedSegment> {
    let r = diameter / 2.0;
    let Some((angle, width, depth)) = keyway_at else {
        return vec![NamedSegment {
            name: "bore".to_owned(),
            geometry: SegmentGeometry::Circle {
                center: [0.0, 0.0],
                radius: r,
                start: 0.0,
            },
        }];
    };
    let w = width / 2.0;
    let x0 = (r * r - w * w).max(0.0).sqrt();
    let top = r + depth;
    let a = w.atan2(x0);
    let at = |p: [f64; 2]| rotate(p, angle);
    let line = |name: &str, start: [f64; 2], end: [f64; 2]| NamedSegment {
        name: name.to_owned(),
        geometry: SegmentGeometry::Line {
            start: at(start),
            end: at(end),
        },
    };
    vec![
        NamedSegment {
            name: "bore".to_owned(),
            geometry: SegmentGeometry::Arc {
                center: [0.0, 0.0],
                radius: r,
                start_angle: angle + a,
                end_angle: angle + TAU - a,
            },
        },
        line("keyway_a", [x0, -w], [top, -w]),
        line("keyway_top", [top, -w], [top, w]),
        line("keyway_b", [top, w], [x0, w]),
    ]
}

/// A circle loop named `name`.
pub fn circle(name: &str, radius: f64) -> Vec<NamedSegment> {
    vec![NamedSegment {
        name: name.to_owned(),
        geometry: SegmentGeometry::Circle {
            center: [0.0, 0.0],
            radius,
            start: 0.0,
        },
    }]
}

/// The area a closed loop bounds (its segments in order, each either
/// way), from points along it.
pub fn loop_area(segments: &[NamedSegment]) -> f64 {
    const SAMPLES: usize = 400;
    let mut points: Vec<[f64; 2]> = Vec::new();
    let n = segments.len();
    for (i, segment) in segments.iter().enumerate() {
        let mut p = sample(&segment.geometry, SAMPLES);
        let reverse = match points.last() {
            Some(last) => {
                let d = |q: [f64; 2]| (q[0] - last[0]).hypot(q[1] - last[1]);
                d(p[0]) > d(p[p.len() - 1])
            }
            None => {
                // The first runs towards the second.
                let next = sample(&segments[(i + 1) % n].geometry, 1);
                let d = |q: [f64; 2]| {
                    next.iter()
                        .map(|r| (q[0] - r[0]).hypot(q[1] - r[1]))
                        .fold(f64::INFINITY, f64::min)
                };
                n > 1 && d(p[0]) < d(p[p.len() - 1])
            }
        };
        if reverse {
            p.reverse();
        }
        points.extend(p);
    }
    let m = points.len();
    let twice: f64 = (0..m)
        .map(|i| {
            let (a, b) = (points[i], points[(i + 1) % m]);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum();
    twice.abs() / 2.0
}

// ---------------------------------------------------------------------------
// Meshing

/// The working pressure angle and centre distance of two gears' mesh:
/// external (`internal` false) or a pinion (`a`) in a ring (`b`).
pub fn mesh_distance(a: &Tooth, b: &Tooth, internal: bool) -> (f64, f64) {
    let alpha = a.transverse_pressure_angle();
    let tan = a.pressure_angle.tan();
    let (ra, rb) = (a.pitch_radius(), b.pitch_radius());
    let (sum, teeth, shift) = if internal {
        (rb - ra, b.teeth - a.teeth, b.shift - a.shift)
    } else {
        (ra + rb, a.teeth + b.teeth, a.shift + b.shift)
    };
    let working = involute_inverse(involute(alpha) + 2.0 * tan * shift / teeth);
    (working, sum * alpha.cos() / working.cos())
}

#[cfg(test)]
mod tests {
    use mitcad_vec3::vec2::distance;

    use super::*;

    fn spur(teeth: f64) -> Tooth {
        Tooth {
            module: 2.0,
            teeth,
            pressure_angle: 20f64.to_radians(),
            helix_angle: 0.0,
            shift: 0.0,
            backlash: 0.0,
        }
    }

    /// The loop's segments join end to end.
    fn assert_closed(segments: &[NamedSegment]) {
        let n = segments.len();
        for i in 0..n {
            let a = ends(&segments[i].geometry);
            let b = ends(&segments[(i + 1) % n].geometry);
            let gap = [a[0], a[1]]
                .iter()
                .flat_map(|p| [b[0], b[1]].map(|q| distance(*p, q)))
                .fold(f64::INFINITY, f64::min);
            assert!(
                gap < 1.0e-6,
                "{} and {} are {gap} apart",
                segments[i].name,
                segments[(i + 1) % n].name
            );
        }
    }

    /// The polygon of the samples has no crossing segments.
    fn assert_simple(segments: &[NamedSegment]) {
        let mut points: Vec<[f64; 2]> = Vec::new();
        let n = segments.len();
        // Orient each segment to start where the previous ended.
        let mut last: Option<[f64; 2]> = None;
        for (i, s) in segments.iter().enumerate() {
            let mut p = sample(&s.geometry, 24);
            let next = ends(&segments[(i + 1) % n].geometry);
            let reverse = match last {
                Some(l) => distance(p[0], l) > distance(p[p.len() - 1], l),
                None => {
                    let end = p[p.len() - 1];
                    distance(end, next[0]).min(distance(end, next[1]))
                        > distance(p[0], next[0]).min(distance(p[0], next[1]))
                }
            };
            if reverse {
                p.reverse();
            }
            last = Some(p[p.len() - 1]);
            points.extend(&p[..p.len() - 1]);
        }
        let cross = |a: [f64; 2], b: [f64; 2], c: [f64; 2], d: [f64; 2]| {
            let o = |p: [f64; 2], q: [f64; 2], r: [f64; 2]| {
                (q[0] - p[0]) * (r[1] - p[1]) - (q[1] - p[1]) * (r[0] - p[0])
            };
            o(a, b, c) * o(a, b, d) < 0.0 && o(c, d, a) * o(c, d, b) < 0.0
        };
        let m = points.len();
        for i in 0..m {
            for j in i + 2..m {
                if i == 0 && j == m - 1 {
                    continue;
                }
                assert!(
                    !cross(
                        points[i],
                        points[(i + 1) % m],
                        points[j],
                        points[(j + 1) % m]
                    ),
                    "the outline crosses itself near {:?}",
                    points[i]
                );
            }
        }
    }

    #[test]
    fn involutes_invert() {
        for a in [0.1, 0.3, 0.5, 0.9] {
            assert!((involute_inverse(involute(a)) - a).abs() < 1.0e-12);
        }
    }

    #[test]
    fn spur_gear_sizes_and_thickness() {
        let t = spur(20.0);
        assert!((2.0 * t.tip_radius() - 44.0).abs() < 1.0e-12);
        assert!((2.0 * t.pitch_radius() - 40.0).abs() < 1.0e-12);
        assert!((2.0 * t.root_radius() - 35.0).abs() < 1.0e-12);
        // The fitted flank at the pitch circle: half the tooth's
        // thickness from its middle, pi m / 2 in all.
        let half = external_half(&t).unwrap();
        let (a, b) = bspline_range(&half.flank).unwrap();
        let (mut lo, mut hi) = (a, b);
        for _ in 0..200 {
            let mid = (lo + hi) / 2.0;
            if radius_of(bspline_point(&half.flank, mid).unwrap()) < 20.0 {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        let p = bspline_point(&half.flank, lo).unwrap();
        let thickness = 2.0 * 20.0 * half_angle_of(p);
        assert!((thickness - PI).abs() < 1.0e-3, "{thickness}");
        assert!(!half.undercut);
    }

    #[test]
    fn flanks_lie_on_the_involute() {
        for teeth in [12.0, 20.0, 57.0] {
            let t = spur(teeth);
            let half = external_half(&t).unwrap();
            let (a, b) = bspline_range(&half.flank).unwrap();
            let base = t.base_radius();
            let alpha = t.transverse_pressure_angle();
            let h = t.thickness() / (2.0 * t.pitch_radius());
            for i in 0..=100 {
                let p = bspline_point(&half.flank, a + (b - a) * i as f64 / 100.0).unwrap();
                let r = radius_of(p);
                let off = r * (half_angle_of(p) - involute_half(h, alpha, base, r)).abs();
                assert!(off < 1.0e-4, "{teeth} teeth: {off} mm off the involute");
            }
        }
    }

    #[test]
    fn few_teeth_are_undercut_without_crossings() {
        let t = spur(12.0);
        let half = external_half(&t).unwrap();
        assert!(half.undercut);
        assert!(half.form_radius > t.base_radius());
        let outline = external_outline(&t, 0.0).unwrap();
        assert_closed(&outline);
        assert_simple(&outline);
        assert!(!external_half(&spur(18.0)).unwrap().undercut);
        assert!(external_half(&spur(16.0)).unwrap().undercut);
    }

    #[test]
    fn outlines_close() {
        for (teeth, shift, angle, helix) in [
            (20.0, 0.0, 20.0, 0.0),
            (31.0, 0.3, 14.5, 0.0),
            (25.0, -0.2, 25.0, 0.0),
            (15.0, 0.4, 20.0, 20.0),
        ] {
            let t = Tooth {
                shift,
                pressure_angle: f64::to_radians(angle),
                helix_angle: f64::to_radians(helix),
                backlash: 0.1,
                ..spur(teeth)
            };
            let outline = external_outline(&t, 0.3).unwrap();
            assert_eq!(
                outline.iter().filter(|s| s.name.starts_with("tip")).count(),
                teeth as usize
            );
            assert_closed(&outline);
        }
    }

    #[test]
    fn ring_gears_close() {
        let t = spur(40.0);
        let outline = internal_outline(&t, 0.0).unwrap();
        assert_closed(&outline);
        assert_simple(&outline);
        // The tips lie inside the pitch circle, the roots outside.
        let tip = outline.iter().find(|s| s.name == "tip(1)").unwrap();
        let p = sample(&tip.geometry, 2)[1];
        assert!((radius_of(p) - 38.0).abs() < 1.0e-9);
    }

    #[test]
    fn racks_have_their_pitch_length() {
        let rack = Rack {
            tooth: spur(10.0),
            height: 20.0,
            length: 10.0 * PI * 2.0,
        };
        assert!((rack.pitch_length() - 20.0 * PI).abs() < 1.0e-12);
        let outline = rack_outline(&rack).unwrap();
        assert_closed(&outline);
        assert_simple(&outline);
        let tips = outline.iter().filter(|s| s.name.starts_with("tip")).count();
        assert_eq!(tips, 10);
        let back = &outline[0].geometry;
        let [a, b] = ends(back);
        assert!((distance(a, b) - 20.0 * PI).abs() < 1.0e-12);
    }

    #[test]
    fn keyways_follow_the_table() {
        assert_eq!(keyway(8.0), Some((2.0, 1.0)));
        assert_eq!(keyway(20.0), Some((6.0, 2.8)));
        assert_eq!(keyway(5.0), None);
        let bore = bore_outline(20.0, keyway(20.0).map(|(w, d)| (0.5, w, d)));
        assert_closed(&bore);
    }

    #[test]
    fn mesh_distances() {
        let (a, b) = (spur(20.0), spur(30.0));
        let (_, distance) = mesh_distance(&a, &b, false);
        assert!((distance - 50.0).abs() < 1.0e-9);
        let (_, inside) = mesh_distance(&a, &spur(40.0), true);
        assert!((inside - 20.0).abs() < 1.0e-9);
        let shifted = Tooth { shift: 0.5, ..a };
        let (working, wider) = mesh_distance(&shifted, &b, false);
        assert!(wider > 50.0 && working > a.pressure_angle);
    }

    #[test]
    fn bevel_sections_close() {
        let bevel = Bevel {
            tooth: spur(20.0),
            pitch_angle: Bevel::pitch_angle_of(20.0, 20.0, PI / 2.0),
            face_width: 8.0,
        };
        assert!((bevel.pitch_angle - PI / 4.0).abs() < 1.0e-12);
        let section = bevel.section(0.0).unwrap();
        assert_closed(&section.outline);
        assert!(section.scale > 0.0 && section.scale < 1.0);
        assert!(section.apex > 0.0);
    }
}
