// SPDX-License-Identifier: MIT
//! Cycloidal teeth (mitcad#2607): the outlines of a Gear feature whose
//! `tooth_form` is `cycloid`, worked out without a geometry kernel.
//!
//! A cycloidal flank is what a point of a rolling circle traces as the
//! circle rolls on the pitch circle: outside it the epicycloid of a circle
//! rolling outside, inside it the hypocycloid of a circle rolling inside.
//! An external gear's addendum flank is the epicycloid of its addendum
//! rolling circle, its dedendum flank the hypocycloid of its dedendum
//! rolling circle; a ring gear's addendum flank (inside its pitch circle)
//! is the hypocycloid of its addendum circle, its dedendum flank the
//! epicycloid of its dedendum circle; a rack's flanks are the cycloids of
//! its circles rolling on its reference line, the addendum circle on the
//! tips' side. Two gears mesh when each one's addendum circle is the
//! other's dedendum circle: the circle rolls on both pitch circles at
//! once and its point traces both flanks, which stay in contact along the
//! circle.
//!
//! The heights are the involute teeth's (addendum 1 m, dedendum 1.25 m in
//! the normal module, a helical gear's transverse section in the
//! transverse module); there is no pressure angle and no profile shift.
//! A tooth is half the pitch thick at the pitch circle less half the
//! backlash (a ring's space wider by half of it). A circle of half the
//! pitch diameter makes a radial dedendum flank; smaller circles make
//! flanks that spread towards the root, larger ones flanks that narrow.
//! The roots and tips are arcs (a rack's lines); the flanks are fitted as
//! B-splines within [`FIT_TOLERANCE`], by their radius (a rack's by the
//! height), which keeps them free of the cycloid's cusp at the pitch
//! circle.
//!
//! Profiles are laid out as `gear_profile`'s: a gear's centre at the
//! origin, tooth 1 centred at `phase`; a rack along x from 0, its
//! reference line on y = 0 and its teeth towards +y, tooth k centred at
//! x = (k - 1/2) p. Segment names: `tip(k)`, `flank_a(k)` (the addendum
//! flank) and `dedendum_a(k)` (the dedendum flank) on the clockwise side
//! of tooth k, `flank_b(k)` and `dedendum_b(k)` on its other side,
//! `root(k)` after it.

use std::f64::consts::TAU;

use super::{
    ADDENDUM, DEDENDUM, FIT_TOLERANCE, MIN_TEETH, NamedSegment, Rack, Tooth, fit, moved, named,
    placed,
};
use crate::profile::SegmentGeometry;

/// The rolling circles' diameter without one given, in modules: the pitch
/// radius of a 15-tooth gear, whose dedendum flanks it makes radial.
pub const ROLLING_DIAMETER: f64 = 7.5;

/// A cycloidal gear's values: the teeth's (module, teeth, helix angle and
/// backlash; the pressure angle and the profile shift are not used) and
/// the diameters of its two rolling circles in the transverse section.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cycloid {
    pub tooth: Tooth,
    /// The circle of the addendum flanks (mm).
    pub addendum: f64,
    /// The circle of the dedendum flanks (mm).
    pub dedendum: f64,
}

/// Why a side of a tooth or a space cannot be made.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Fault {
    /// The outer circle's epicycloid does not reach the outer radius.
    OuterShort,
    /// The inner circle's hypocycloid does not reach the inner radius (or
    /// the circle does not fit inside the pitch circle).
    InnerReach,
    /// The outer part reaches the middle: the tooth (space) closes.
    OuterCloses,
    /// The inner part reaches the middle.
    InnerCloses,
    /// The inner part reaches the middle of the next space (tooth).
    InnerOpens,
}

/// The polar angle of the point at radius `r` of the epicycloid that a
/// circle of `diameter` rolling outside the circle of radius `pitch`
/// traces from (pitch, 0), rolling counter-clockwise (pitch <= r <=
/// pitch + diameter).
fn epicycloid_angle(pitch: f64, diameter: f64, r: f64) -> f64 {
    let rho = diameter / 2.0;
    let c = pitch + rho;
    // r^2 = c^2 + rho^2 - 2 c rho cos(pitch t / rho).
    let cos = ((c * c + rho * rho - r * r) / (2.0 * c * rho)).clamp(-1.0, 1.0);
    let t = rho / pitch * cos.acos();
    let k = c / rho;
    let x = c * t.cos() - rho * (k * t).cos();
    let y = c * t.sin() - rho * (k * t).sin();
    y.atan2(x)
}

/// The polar angle of the point at radius `r` of the hypocycloid that a
/// circle of `diameter` (less than twice `pitch`) rolling inside the circle
/// of radius `pitch` traces from (pitch, 0), rolling counter-clockwise
/// (|pitch - diameter| <= r <= pitch).
fn hypocycloid_angle(pitch: f64, diameter: f64, r: f64) -> f64 {
    let rho = diameter / 2.0;
    let c = pitch - rho;
    // r^2 = c^2 + rho^2 + 2 c rho cos(pitch t / rho).
    let cos = ((r * r - c * c - rho * rho) / (2.0 * c * rho)).clamp(-1.0, 1.0);
    let t = rho / pitch * cos.acos();
    let k = c / rho;
    let x = c * t.cos() + rho * (k * t).cos();
    let y = c * t.sin() - rho * (k * t).sin();
    y.atan2(x)
}

/// One side of a tooth (an external gear's) or of a space (a ring's) in
/// the coordinates where it lies along +x, this side on -y: `half` the
/// width at the pitch circle (an angle), the epicycloid of the `outer`
/// circle from the pitch circle out to `r_out` (narrowing), the
/// hypocycloid of the `inner` circle in to `r_in`.
#[derive(Debug, Clone, Copy)]
struct Side {
    pitch: f64,
    half: f64,
    outer: f64,
    inner: f64,
    r_out: f64,
    r_in: f64,
    /// From one tooth (space) to the next.
    step: f64,
}

impl Side {
    /// The half-angle at radius `r` (from the middle).
    fn angle(&self, r: f64) -> f64 {
        if r >= self.pitch {
            self.half - epicycloid_angle(self.pitch, self.outer, r)
        } else {
            self.half + hypocycloid_angle(self.pitch, self.inner, r)
        }
    }

    fn point(&self, r: f64) -> [f64; 2] {
        let h = self.angle(r);
        [r * h.cos(), -r * h.sin()]
    }

    fn check(&self) -> Result<(), Fault> {
        if self.pitch + self.outer <= self.r_out + 1.0e-9 {
            return Err(Fault::OuterShort);
        }
        if (self.pitch - self.inner).abs() >= self.r_in - 1.0e-9 {
            return Err(Fault::InnerReach);
        }
        const STEPS: usize = 64;
        for i in 0..=STEPS {
            let f = i as f64 / STEPS as f64;
            if self.angle(self.pitch + (self.r_out - self.pitch) * f) <= 1.0e-9 {
                return Err(Fault::OuterCloses);
            }
            let inner = self.angle(self.pitch - (self.pitch - self.r_in) * f);
            if inner <= 1.0e-9 {
                return Err(Fault::InnerCloses);
            }
            if inner >= self.step / 2.0 - 1.0e-9 {
                return Err(Fault::InnerOpens);
            }
        }
        Ok(())
    }

    /// The outer part (from the pitch circle out) and the inner part (from
    /// `r_in` to the pitch circle).
    fn segments(&self) -> (SegmentGeometry, SegmentGeometry) {
        let f = |r: f64| self.point(r);
        (
            fit(&f, self.pitch, self.r_out, FIT_TOLERANCE),
            fit(&f, self.r_in, self.pitch, FIT_TOLERANCE),
        )
    }
}

/// A length for a message: at most four decimals, no trailing zeros.
fn mm(value: f64) -> String {
    let text = format!("{value:.4}");
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// One side of a rack's tooth centred at x = 0: its left side x(y), half
/// the width `half` at the reference line, the cycloid of the `addendum`
/// circle up to `tip`, of the `dedendum` circle down to `-root`.
#[derive(Debug, Clone, Copy)]
struct RackSide {
    half: f64,
    addendum: f64,
    dedendum: f64,
    tip: f64,
    root: f64,
}

impl RackSide {
    fn x(&self, y: f64) -> f64 {
        if y >= 0.0 {
            let rho = self.addendum / 2.0;
            let t = (1.0 - y / rho).clamp(-1.0, 1.0).acos();
            -self.half + rho * (t - t.sin())
        } else {
            let rho = self.dedendum / 2.0;
            let t = (1.0 + y / rho).clamp(-1.0, 1.0).acos();
            -self.half - rho * (t - t.sin())
        }
    }
}

impl Cycloid {
    /// The teeth's values and the circles'.
    pub fn check(&self) -> Result<(), String> {
        self.tooth.check()?;
        self.check_circles()
    }

    fn check_circles(&self) -> Result<(), String> {
        for (what, d) in [("addendum", self.addendum), ("dedendum", self.dedendum)] {
            if !(d > 0.0 && d.is_finite()) {
                return Err(format!(
                    "the {what} rolling circle's diameter must be greater than zero, got {d}"
                ));
            }
        }
        Ok(())
    }

    /// An external gear's tip and root radii.
    pub fn tip_radius(&self) -> f64 {
        self.tooth.pitch_radius() + ADDENDUM * self.tooth.module
    }

    pub fn root_radius(&self) -> f64 {
        self.tooth.pitch_radius() - DEDENDUM * self.tooth.module
    }

    /// A ring gear's tip radius (inside) and root radius.
    pub fn internal_tip_radius(&self) -> f64 {
        self.tooth.pitch_radius() - ADDENDUM * self.tooth.module
    }

    pub fn internal_root_radius(&self) -> f64 {
        self.tooth.pitch_radius() + DEDENDUM * self.tooth.module
    }

    fn external_side(&self) -> Side {
        let t = &self.tooth;
        let r = t.pitch_radius();
        Side {
            pitch: r,
            half: (t.pitch() / 2.0 - t.backlash / 2.0) / (2.0 * r),
            outer: self.addendum,
            inner: self.dedendum,
            r_out: self.tip_radius(),
            r_in: self.root_radius(),
            step: TAU / t.teeth.round(),
        }
    }

    /// A ring gear's space: its outer part the ring's dedendum flank, its
    /// inner part the addendum flank.
    fn internal_side(&self) -> Side {
        let t = &self.tooth;
        let r = t.pitch_radius();
        Side {
            pitch: r,
            half: (t.pitch() / 2.0 + t.backlash / 2.0) / (2.0 * r),
            outer: self.dedendum,
            inner: self.addendum,
            r_out: self.internal_root_radius(),
            r_in: self.internal_tip_radius(),
            step: TAU / t.teeth.round(),
        }
    }

    /// The values an external gear can be made with.
    pub fn check_external(&self) -> Result<(), String> {
        self.check()?;
        let side = self.external_side();
        let m = self.tooth.module;
        side.check().map_err(|fault| match fault {
            Fault::OuterShort => format!(
                "the addendum rolling circle is too small to reach the tips: its diameter must be more than {} mm, got {}",
                mm(ADDENDUM * m),
                mm(self.addendum)
            ),
            Fault::InnerReach => format!(
                "the dedendum rolling circle's diameter must be more than {} mm and less than {} mm for these teeth, got {}",
                mm(DEDENDUM * m),
                mm(2.0 * side.pitch - DEDENDUM * m),
                mm(self.dedendum)
            ),
            Fault::OuterCloses => {
                "the teeth come to a point below their outside diameter: make the addendum rolling circle larger"
                    .to_owned()
            }
            Fault::InnerCloses => {
                "the teeth narrow to nothing above their roots: make the dedendum rolling circle smaller"
                    .to_owned()
            }
            Fault::InnerOpens => {
                "the spaces between the teeth close above their roots: make the dedendum rolling circle larger"
                    .to_owned()
            }
        })
    }

    /// The values a ring gear can be made with.
    pub fn check_internal(&self) -> Result<(), String> {
        self.check()?;
        let side = self.internal_side();
        let m = self.tooth.module;
        side.check().map_err(|fault| match fault {
            Fault::OuterShort => format!(
                "the ring gear's dedendum rolling circle is too small to reach its roots: its diameter must be more than {} mm, got {}",
                mm(DEDENDUM * m),
                mm(self.dedendum)
            ),
            Fault::InnerReach => format!(
                "the ring gear's addendum rolling circle's diameter must be more than {} mm and less than {} mm, got {}",
                mm(ADDENDUM * m),
                mm(2.0 * side.pitch - ADDENDUM * m),
                mm(self.addendum)
            ),
            Fault::OuterCloses => {
                "the ring gear's spaces close before its root diameter: make the dedendum rolling circle larger"
                    .to_owned()
            }
            Fault::InnerCloses => {
                "the ring gear's spaces close towards its tips: make the addendum rolling circle smaller"
                    .to_owned()
            }
            Fault::InnerOpens => {
                "the ring gear's teeth come to a point inside their tip diameter: make the addendum rolling circle larger"
                    .to_owned()
            }
        })
    }

    fn rack_side(&self) -> RackSide {
        let t = &self.tooth;
        RackSide {
            half: t.pitch() / 4.0 - t.backlash / 4.0,
            addendum: self.addendum,
            dedendum: self.dedendum,
            tip: ADDENDUM * t.module,
            root: DEDENDUM * t.module,
        }
    }

    /// The values a rack can be made with (any count of teeth from one).
    pub fn check_rack(&self) -> Result<(), String> {
        Tooth {
            teeth: self.tooth.teeth.max(MIN_TEETH),
            ..self.tooth
        }
        .check()?;
        self.check_circles()?;
        let side = self.rack_side();
        if self.addendum <= side.tip + 1.0e-9 {
            return Err(format!(
                "the rack's addendum rolling circle is too small to reach its tips: its diameter must be more than {} mm, got {}",
                mm(side.tip),
                mm(self.addendum)
            ));
        }
        if self.dedendum <= side.root + 1.0e-9 {
            return Err(format!(
                "the rack's dedendum rolling circle is too small to reach its roots: its diameter must be more than {} mm, got {}",
                mm(side.root),
                mm(self.dedendum)
            ));
        }
        if side.x(side.tip) >= -1.0e-9 {
            return Err(
                "the rack's teeth come to a point below their tips: make the addendum rolling circle larger"
                    .to_owned(),
            );
        }
        if self.tooth.pitch() / 2.0 + side.x(-side.root) <= 1.0e-9 {
            return Err(
                "the rack's spaces close above their roots: make the dedendum rolling circle larger"
                    .to_owned(),
            );
        }
        Ok(())
    }
}

/// The closed outline of an external cycloidal gear, tooth 1 centred at
/// `phase`.
pub fn external_outline(c: &Cycloid, phase: f64) -> Result<Vec<NamedSegment>, String> {
    c.check_external()?;
    let side = c.external_side();
    let (outer, inner) = side.segments();
    let (tip_angle, root_angle) = (side.angle(side.r_out), side.angle(side.r_in));
    let z = c.tooth.teeth.round() as usize;
    let step = side.step;
    let mut segments = Vec::with_capacity(6 * z);
    for i in 0..z {
        let k = i + 1;
        let a = phase + step * i as f64;
        segments.push(named("dedendum_a", k, placed(&inner, a, false)));
        segments.push(named("flank_a", k, placed(&outer, a, false)));
        segments.push(named(
            "tip",
            k,
            SegmentGeometry::Arc {
                center: [0.0, 0.0],
                radius: side.r_out,
                start_angle: a - tip_angle,
                end_angle: a + tip_angle,
            },
        ));
        segments.push(named("flank_b", k, placed(&outer, a, true)));
        segments.push(named("dedendum_b", k, placed(&inner, a, true)));
        let (from, to) = (a + root_angle, a + step - root_angle);
        if to - from > 1.0e-9 {
            segments.push(named(
                "root",
                k,
                SegmentGeometry::Arc {
                    center: [0.0, 0.0],
                    radius: side.r_in,
                    start_angle: from,
                    end_angle: to,
                },
            ));
        }
    }
    Ok(segments)
}

/// The toothed loop of a cycloidal ring gear (a hole of its rim), tooth 1
/// centred at `phase`.
pub fn internal_outline(c: &Cycloid, phase: f64) -> Result<Vec<NamedSegment>, String> {
    c.check_internal()?;
    let side = c.internal_side();
    let (outer, inner) = side.segments();
    let (root_angle, tip_angle) = (side.angle(side.r_out), side.angle(side.r_in));
    let z = c.tooth.teeth.round() as usize;
    let step = side.step;
    let tooth_half = step / 2.0 - tip_angle;
    let mut segments = Vec::with_capacity(6 * z);
    for i in 0..z {
        let k = i + 1;
        let next = if k == z { 1 } else { k + 1 };
        let a = phase + step * i as f64;
        let space = a + step / 2.0;
        segments.push(named(
            "tip",
            k,
            SegmentGeometry::Arc {
                center: [0.0, 0.0],
                radius: side.r_in,
                start_angle: a - tooth_half,
                end_angle: a + tooth_half,
            },
        ));
        segments.push(named("flank_b", k, placed(&inner, space, false)));
        segments.push(named("dedendum_b", k, placed(&outer, space, false)));
        if root_angle > 1.0e-9 {
            segments.push(named(
                "root",
                k,
                SegmentGeometry::Arc {
                    center: [0.0, 0.0],
                    radius: side.r_out,
                    start_angle: space - root_angle,
                    end_angle: space + root_angle,
                },
            ));
        }
        segments.push(named("dedendum_a", next, placed(&outer, space, true)));
        segments.push(named("flank_a", next, placed(&inner, space, true)));
    }
    Ok(segments)
}

/// The closed outline of a cycloidal rack, laid out as
/// `gear_profile::rack_outline`'s: its back on y = height below the tips,
/// tooth k centred at x = (k - 1/2) p, the ends at x = 0 and x = length;
/// roles `back`, `end_a` (x = 0), `end_b` and the teeth's.
pub fn rack_outline(c: &Cycloid, rack: &Rack) -> Result<Vec<NamedSegment>, String> {
    rack.check()?;
    c.check_rack()?;
    let t = &rack.tooth;
    let p = t.pitch();
    let n = t.teeth.round() as usize;
    let side = c.rack_side();
    let tip_y = side.tip;
    let root_y = -side.root;
    let bottom = tip_y - rack.height;
    // The left foot of the tooth centred at 0 (on the root line) and half
    // the tip's width.
    let foot = side.x(root_y);
    let tip_half = -side.x(tip_y);
    let left = |y: f64| [side.x(y), y];
    let right = |y: f64| [-side.x(y), y];
    let left_dedendum = fit(&left, root_y, 0.0, FIT_TOLERANCE);
    let left_flank = fit(&left, 0.0, tip_y, FIT_TOLERANCE);
    let right_dedendum = fit(&right, root_y, 0.0, FIT_TOLERANCE);
    let right_flank = fit(&right, 0.0, tip_y, FIT_TOLERANCE);
    let mut top = Vec::new();
    let mut x = 0.0;
    for i in 0..n {
        let k = i + 1;
        let middle = p * (i as f64 + 0.5);
        let start = middle + foot;
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
        top.push(named("dedendum_a", k, moved(&left_dedendum, [middle, 0.0])));
        top.push(named("flank_a", k, moved(&left_flank, [middle, 0.0])));
        top.push(named(
            "tip",
            k,
            SegmentGeometry::Line {
                start: [middle - tip_half, tip_y],
                end: [middle + tip_half, tip_y],
            },
        ));
        top.push(named("flank_b", k, moved(&right_flank, [middle, 0.0])));
        top.push(named(
            "dedendum_b",
            k,
            moved(&right_dedendum, [middle, 0.0]),
        ));
        x = middle - foot;
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

#[cfg(test)]
mod tests {
    use std::f64::consts::PI;

    use super::super::{bspline_point, bspline_range, ends, sample};
    use super::*;

    fn tooth(module: f64, teeth: f64, backlash: f64) -> Tooth {
        Tooth {
            module,
            teeth,
            pressure_angle: 20f64.to_radians(),
            helix_angle: 0.0,
            shift: 0.0,
            backlash,
        }
    }

    /// Module 2, the circles 7.5 modules across.
    fn standard(teeth: f64) -> Cycloid {
        Cycloid {
            tooth: tooth(2.0, teeth, 0.0),
            addendum: 15.0,
            dedendum: 15.0,
        }
    }

    fn assert_closed(segments: &[NamedSegment]) {
        let n = segments.len();
        for i in 0..n {
            let a = ends(&segments[i].geometry);
            let b = ends(&segments[(i + 1) % n].geometry);
            let gap = a
                .iter()
                .flat_map(|p| b.map(|q| (p[0] - q[0]).hypot(p[1] - q[1])))
                .fold(f64::INFINITY, f64::min);
            assert!(
                gap < 1.0e-6,
                "{} and {} are {gap} apart",
                segments[i].name,
                segments[(i + 1) % n].name
            );
        }
    }

    #[test]
    fn the_curves_have_their_known_points() {
        let (pitch, diameter) = (20.0, 15.0);
        // The epicycloid's farthest point, a diameter out, lies at
        // rho pi / pitch; the hypocycloid's nearest, a diameter in, too.
        let farthest = diameter / 2.0 * PI / pitch;
        let far = epicycloid_angle(pitch, diameter, pitch + diameter);
        assert!((far - farthest).abs() < 1.0e-9, "{far}");
        let near = hypocycloid_angle(pitch, diameter, pitch - diameter);
        assert!((near - farthest).abs() < 1.0e-9, "{near}");
        // A circle of half the pitch diameter rolls a straight diameter.
        for r in [1.0, 5.0, 12.0, 19.9] {
            assert!(hypocycloid_angle(pitch, pitch, r).abs() < 1.0e-12);
        }
        // Both start at the pitch circle.
        assert!(epicycloid_angle(pitch, diameter, pitch).abs() < 1.0e-12);
        assert!(hypocycloid_angle(pitch, diameter, pitch).abs() < 1.0e-12);
    }

    #[test]
    fn dedendum_flanks_are_radial_at_half_the_pitch_diameter() {
        // 15 teeth of module 2: pitch radius 15, the circles 15 across.
        let gear = standard(15.0);
        let side = gear.external_side();
        for i in 0..=20 {
            let r = side.r_in + (side.pitch - side.r_in) * i as f64 / 20.0;
            assert!((side.angle(r) - side.half).abs() < 1.0e-12);
        }
        // Fewer teeth narrow towards the root, more spread.
        let few = standard(12.0).external_side();
        assert!(few.angle(few.r_in) < few.half);
        let many = standard(30.0).external_side();
        assert!(many.angle(many.r_in) > many.half);
        // The addendum narrows towards the tip.
        assert!(many.angle(many.r_out) < many.half);
    }

    #[test]
    fn outlines_close_and_follow_their_curves() {
        for teeth in [12.0, 20.0, 47.0] {
            let gear = Cycloid {
                tooth: tooth(2.0, teeth, 0.1),
                ..standard(teeth)
            };
            let outline = external_outline(&gear, 0.3).unwrap();
            assert_closed(&outline);
            assert_eq!(
                outline.iter().filter(|s| s.name.starts_with("tip")).count(),
                teeth as usize
            );
            // The fitted flanks lie on the cycloids.
            let side = gear.external_side();
            for name in ["flank_a(1)", "dedendum_a(1)"] {
                let segment = &outline.iter().find(|s| s.name == name).unwrap().geometry;
                let (a, b) = bspline_range(segment).unwrap();
                for i in 0..=100 {
                    let p = bspline_point(segment, a + (b - a) * i as f64 / 100.0).unwrap();
                    let r = p[0].hypot(p[1]);
                    let h = -(p[1].atan2(p[0]) - 0.3);
                    let off = r * (h - side.angle(r)).abs();
                    assert!(off < 1.0e-5, "{teeth} teeth, {name}: {off} mm off");
                }
            }
        }
        let ring = Cycloid {
            tooth: tooth(2.0, 60.0, 0.1),
            ..standard(60.0)
        };
        let outline = internal_outline(&ring, 0.0).unwrap();
        assert_closed(&outline);
        let tip = outline.iter().find(|s| s.name == "tip(1)").unwrap();
        let p = sample(&tip.geometry, 2)[1];
        assert!((p[0].hypot(p[1]) - 58.0).abs() < 1.0e-9);
        let rack = Rack {
            tooth: tooth(2.0, 10.0, 0.1),
            height: 20.0,
            length: 10.0 * PI * 2.0,
        };
        let cycloid = Cycloid {
            tooth: rack.tooth,
            ..standard(10.0)
        };
        let outline = rack_outline(&cycloid, &rack).unwrap();
        assert_closed(&outline);
        assert_eq!(
            outline.iter().filter(|s| s.name.starts_with("tip")).count(),
            10
        );
        // The teeth are as wide at the reference line as their spaces less
        // the backlash.
        let side = cycloid.rack_side();
        let width = -2.0 * side.x(0.0);
        assert!((width - (PI - 0.05)).abs() < 1.0e-12, "{width}");
    }

    #[test]
    fn values_that_make_no_teeth_are_refused() {
        // Eight teeth: the default circle does not fit to the roots.
        let message = standard(8.0).check_external().unwrap_err();
        assert!(message.contains("dedendum rolling circle"), "{message}");
        let short = Cycloid {
            addendum: 1.5,
            ..standard(20.0)
        };
        let message = short.check_external().unwrap_err();
        assert!(message.contains("too small to reach the tips"), "{message}");
        let small = Cycloid {
            dedendum: 3.0,
            ..standard(20.0)
        };
        let message = small.check_external().unwrap_err();
        assert!(message.contains("close above their roots"), "{message}");
        let message = Cycloid {
            addendum: 0.0,
            ..standard(20.0)
        }
        .check()
        .unwrap_err();
        assert!(message.contains("greater than zero"), "{message}");
    }

    // -----------------------------------------------------------------
    // Meshing: the gears' boundaries, worked out from the curves, placed
    // as the Gear feature places a mesh, turned through a pitch; no point
    // of one lies inside the other.

    const SAMPLES: usize = 120;

    /// How deep `p` lies in an external gear turned by `turn` (its tooth 1
    /// there): the distance across the tooth at that radius, or inside the
    /// root circle; negative outside.
    fn external_depth(c: &Cycloid, turn: f64, p: [f64; 2]) -> f64 {
        let side = c.external_side();
        let r = p[0].hypot(p[1]);
        if r >= side.r_out {
            return side.r_out - r;
        }
        if r <= side.r_in {
            return side.r_in - r;
        }
        let phi = p[1].atan2(p[0]) - turn;
        let delta = (phi - (phi / side.step).round() * side.step).abs();
        r * (side.angle(r) - delta)
    }

    /// How deep `p` lies in a ring gear's teeth or rim.
    fn internal_depth(c: &Cycloid, turn: f64, p: [f64; 2]) -> f64 {
        let side = c.internal_side();
        let r = p[0].hypot(p[1]);
        if r >= side.r_out {
            return r - side.r_out;
        }
        if r <= side.r_in {
            return r - side.r_in;
        }
        let phi = p[1].atan2(p[0]) - turn - side.step / 2.0;
        let delta = (phi - (phi / side.step).round() * side.step).abs();
        r * (delta - side.angle(r))
    }

    /// How deep `p` lies in a rack whose tooth is centred at x = `shift`.
    fn rack_depth(c: &Cycloid, shift: f64, p: [f64; 2]) -> f64 {
        let side = c.rack_side();
        let pitch = c.tooth.pitch();
        if p[1] >= side.tip {
            return side.tip - p[1];
        }
        if p[1] <= -side.root {
            return -side.root - p[1];
        }
        let x = p[0] - shift;
        let dx = (x - (x / pitch).round() * pitch).abs();
        -side.x(p[1]) - dx
    }

    /// Points along a gear's boundary (a side's both flanks round every
    /// tooth or space, and the arcs between), the side's middle k at
    /// `turn + offset + k step`.
    fn boundary(side: &Side, turn: f64, offset: f64, teeth: usize) -> Vec<[f64; 2]> {
        let mut points = Vec::new();
        let at = |r: f64, a: f64| [r * a.cos(), r * a.sin()];
        for k in 0..teeth {
            let middle = turn + offset + side.step * k as f64;
            for i in 0..=SAMPLES {
                let r = side.r_in + (side.r_out - side.r_in) * i as f64 / SAMPLES as f64;
                let h = side.angle(r);
                points.push(at(r, middle - h));
                points.push(at(r, middle + h));
            }
            let (h_out, h_in) = (side.angle(side.r_out), side.angle(side.r_in));
            for i in 0..=8 {
                let f = i as f64 / 8.0;
                points.push(at(side.r_out, middle - h_out + 2.0 * h_out * f));
                points.push(at(side.r_in, middle + h_in + (side.step - 2.0 * h_in) * f));
            }
        }
        points
    }

    fn external_points(c: &Cycloid, turn: f64, centre: [f64; 2]) -> Vec<[f64; 2]> {
        let z = c.tooth.teeth.round() as usize;
        boundary(&c.external_side(), turn, 0.0, z)
            .into_iter()
            .map(|p| [p[0] + centre[0], p[1] + centre[1]])
            .collect()
    }

    /// The deepest point of either gear in the other, external gears `a`
    /// at the origin and `b` at their centre distance along x, turned
    /// through a pitch of `a`; `b` turned by `offset` from its mesh.
    fn external_mesh(a: &Cycloid, b: &Cycloid, offset: f64) -> f64 {
        let distance = a.tooth.pitch_radius() + b.tooth.pitch_radius();
        let (za, zb) = (a.tooth.teeth, b.tooth.teeth);
        let mut deepest = f64::NEG_INFINITY;
        for i in 0..40 {
            let turn_a = TAU / za * i as f64 / 40.0;
            // As `gear::placement` meshes them: b's space faces a's tooth 1.
            let turn_b = PI - PI / zb - turn_a * za / zb + offset;
            for p in external_points(b, turn_b, [distance, 0.0]) {
                deepest = deepest.max(external_depth(a, turn_a, p));
            }
            for p in external_points(a, turn_a, [-distance, 0.0]) {
                deepest = deepest.max(external_depth(b, turn_b, p));
            }
        }
        deepest
    }

    #[test]
    fn external_gears_mesh_without_interference() {
        for (za, zb) in [(20.0, 30.0), (12.0, 40.0), (15.0, 15.0)] {
            let deepest = external_mesh(&standard(za), &standard(zb), 0.0);
            assert!(deepest < 1.0e-6, "{za} and {zb} teeth: {deepest} mm deep");
            // And they touch: no backlash.
            assert!(deepest > -1.0e-2, "{za} and {zb} teeth: {deepest} mm apart");
        }
        // Circles of their own, each one's addendum circle the other's
        // dedendum circle.
        let a = Cycloid {
            addendum: 10.0,
            dedendum: 13.0,
            ..standard(18.0)
        };
        let b = Cycloid {
            addendum: 13.0,
            dedendum: 10.0,
            ..standard(25.0)
        };
        let deepest = external_mesh(&a, &b, 0.0);
        assert!(deepest < 1.0e-6 && deepest > -1.0e-2, "{deepest}");
        // The check finds interference: turned by a quarter pitch, the
        // teeth run into each other.
        let wrong = external_mesh(&standard(20.0), &standard(30.0), PI / 60.0);
        assert!(wrong > 0.1, "{wrong}");
    }

    #[test]
    fn a_ring_gear_and_its_pinion_mesh_without_interference() {
        let ring = standard(60.0);
        let pinion = standard(20.0);
        let distance = ring.tooth.pitch_radius() - pinion.tooth.pitch_radius();
        let (zr, zp) = (60.0, 20.0);
        let ring_side = ring.internal_side();
        let mut deepest = f64::NEG_INFINITY;
        for i in 0..40 {
            let turn_r = TAU / zr * i as f64 / 40.0;
            // As `gear::placement` meshes a pinion in a ring: its space
            // faces the ring's tooth 1; both turn the same way.
            let turn_p = -PI / zp + turn_r * zr / zp;
            for p in external_points(&pinion, turn_p, [distance, 0.0]) {
                deepest = deepest.max(internal_depth(&ring, turn_r, p));
            }
            let ring_points = boundary(&ring_side, turn_r, ring_side.step / 2.0, 60);
            for p in ring_points {
                deepest = deepest.max(external_depth(&pinion, turn_p, [p[0] - distance, p[1]]));
            }
        }
        assert!(deepest < 1.0e-6 && deepest > -1.0e-2, "{deepest}");
    }

    #[test]
    fn a_rack_and_its_pinion_mesh_without_interference() {
        let pinion = standard(20.0);
        let rack = Cycloid {
            tooth: tooth(2.0, 10.0, 0.0),
            ..standard(10.0)
        };
        let r = pinion.tooth.pitch_radius();
        let pitch = rack.tooth.pitch();
        let side = rack.rack_side();
        let mut deepest = f64::NEG_INFINITY;
        for i in 0..40 {
            let shift = pitch * i as f64 / 40.0;
            // The pinion above the rack, its space facing the rack's tooth
            // at x = shift; it rolls along with the rack.
            let turn = -PI / 2.0 + PI / 20.0 + shift / r;
            for p in external_points(&pinion, turn, [0.0, r]) {
                deepest = deepest.max(rack_depth(&rack, shift, p));
            }
            for k in -3..=3 {
                let middle = shift + pitch * k as f64;
                for j in 0..=SAMPLES {
                    let y = -side.root + (side.tip + side.root) * j as f64 / SAMPLES as f64;
                    let x = side.x(y);
                    for p in [[middle + x, y], [middle - x, y]] {
                        deepest = deepest.max(external_depth(&pinion, turn, [p[0], p[1] - r]));
                    }
                }
            }
        }
        assert!(deepest < 1.0e-6 && deepest > -1.0e-2, "{deepest}");
    }
}
