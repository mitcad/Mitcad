// SPDX-License-Identifier: MIT
//! Gear Pair and Belt Drive (mitcad#656): the gears and the pulleys that
//! fit two axes a given distance apart at a given ratio.
//!
//! Gear Pair goes through the standard modules and the tooth counts near
//! the ratio, and keeps each pair that fits the centre distance `a`
//! exactly (`m (z1 + z2) / 2 = a`), by profile shifts within their limits
//! (the working pressure angle of the shifted mesh makes its centre
//! distance `a`: `gear_profile::mesh_distance`), or by helical teeth whose
//! helix angle makes it exact (`m (z1 + z2) / (2 cos beta) = a`).
//! Cycloidal pairs (mitcad#2607) take the exact and the helix fits only,
//! with the default rolling circles, and only tooth counts those make.
//!
//! Belt Drive goes through the tooth counts of the pulleys near the ratio
//! and gives for each the catalogue belts that go round them and the
//! tension travel each needs: how far the axes must move from their
//! distance for that belt to fit. The belt is the open belt of
//! `pulley::belt_drive`.

use std::f64::consts::{PI, TAU};

use serde::Serialize;

use super::gear_profile::{self, involute};
use super::pulley_profile::{self, BeltProfile};

/// ISO 54 series I from 1 mm up, and below 1 mm the DIN 780 series 1
/// values from 0.3 mm (mm).
pub const MODULES: &[f64] = &[
    0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0, 1.25, 1.5, 2.0, 2.5, 3.0, 4.0, 5.0, 6.0, 8.0, 10.0,
    12.0, 16.0, 20.0, 25.0, 32.0, 40.0, 50.0,
];

/// ISO 54 series II (mm).
pub const MODULES_II: &[f64] = &[
    1.125, 1.375, 1.75, 2.25, 2.75, 3.5, 4.5, 5.5, 7.0, 9.0, 11.0, 14.0, 18.0, 22.0, 28.0, 36.0,
    45.0,
];

/// Gear Pair's least module by default (mm): smaller gears are rarely
/// practical; `modules` asks for others (Mitcad's default).
pub const MIN_MODULE: f64 = 0.5;

/// Ratio deviations within the same step of this (relative) count as
/// equal in Gear Pair's order, so that the fewer teeth come first among
/// them.
pub const DEVIATION_STEP: f64 = 0.005;

/// The least helix angle a helix fit takes (radians): 5 degrees.
pub const MIN_HELIX_ANGLE: f64 = 5.0 * PI / 180.0;

/// Within how much a centre distance is exact (mm).
pub const EXACT: f64 = 1.0e-6;

/// How a pair of gears fits the axes' distance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Fit {
    Exact,
    ProfileShift,
    Helix,
}

/// What the Gear Pair search takes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GearPairOptions {
    /// z2 / z1.
    pub ratio: f64,
    /// Spur teeth (exact and profile shift fits), helical teeth (helix
    /// fits).
    pub spur: bool,
    pub helical: bool,
    /// The normal pressure angle (radians).
    pub pressure_angle: f64,
    /// The least and the largest module (mm).
    pub modules: (f64, f64),
    /// ISO 54 series II too.
    pub second_series: bool,
    pub min_teeth: f64,
    pub max_teeth: f64,
    /// The relative deviation of the ratio allowed.
    pub ratio_tolerance: f64,
    /// The largest profile shift of either gear (in modules, either way).
    pub max_profile_shift: f64,
    /// The largest helix angle (radians).
    pub max_helix_angle: f64,
    /// The most candidates.
    pub limit: usize,
    /// Cycloidal teeth (mitcad#2607): no profile shifts, none undercut.
    pub cycloid: bool,
}

impl Default for GearPairOptions {
    fn default() -> Self {
        Self {
            ratio: 1.0,
            spur: true,
            helical: true,
            pressure_angle: 20f64.to_radians(),
            modules: (MIN_MODULE, f64::INFINITY),
            second_series: false,
            min_teeth: 12.0,
            max_teeth: 200.0,
            ratio_tolerance: 0.02,
            max_profile_shift: 0.5,
            max_helix_angle: 30f64.to_radians(),
            limit: 20,
            cycloid: false,
        }
    }
}

/// A pair of gears that fits.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GearPairCandidate {
    /// The normal module (mm).
    pub module: f64,
    /// `[z1, z2]`.
    pub teeth: [f64; 2],
    pub fit: Fit,
    /// `[x1, x2]`: a profile shift fit's, or on any fit the shifts of sum
    /// zero that keep a gear free of undercut (0 without them).
    pub profile_shift: [f64; 2],
    /// The helix angle (0 unless a helix fit; the first gear's
    /// right-handed, the second's left-handed).
    pub helix_angle: f64,
    /// z2 / z1, and its deviation from the ratio asked (relative).
    pub ratio: f64,
    pub ratio_deviation: f64,
    /// Whether each gear's teeth are undercut with its shift
    /// ([`undercuts`]).
    pub undercut: [bool; 2],
    /// Whether shifts against undercut were added to the fit's own (an
    /// exact pair so corrected is `exact, corrected`).
    pub corrected: bool,
}

impl GearPairCandidate {
    /// What orders the candidates after the fit and the ratio's deviation:
    /// the shifts and the helix angle.
    fn cost(&self) -> f64 {
        self.profile_shift[0].abs() + self.profile_shift[1].abs() + self.helix_angle
    }
}

/// The least profile shift that keeps a gear of `teeth` free of undercut
/// by the basic rack: `x = 1 - zv sin²(alpha) / 2`, the virtual teeth `zv
/// = z / cos³(beta)` of a helical gear (`z < 2 (1 - x) / sin²(alpha)`
/// undercuts).
pub fn undercut_shift(teeth: f64, pressure_angle: f64, helix_angle: f64) -> f64 {
    let virtual_teeth = teeth / helix_angle.cos().powi(3);
    1.0 - virtual_teeth * pressure_angle.sin().powi(2) / 2.0
}

/// Whether a gear of `teeth` with profile shift `shift` is undercut.
pub fn undercuts(teeth: f64, pressure_angle: f64, helix_angle: f64, shift: f64) -> bool {
    shift < undercut_shift(teeth, pressure_angle, helix_angle) - 1.0e-9
}

/// A candidate with the shifts that free an undercut gear moved to it from
/// the other (their sum, so the centre distance, kept), where both stay
/// within `max_shift` and the other stays free of undercut.
#[allow(clippy::too_many_arguments)]
fn candidate(
    module: f64,
    teeth: [f64; 2],
    fit: Fit,
    shifts: [f64; 2],
    helix_angle: f64,
    ratio: f64,
    ratio_deviation: f64,
    options: &GearPairOptions,
) -> GearPairCandidate {
    // Cycloidal teeth (mitcad#2607) have no undercut to correct.
    if options.cycloid {
        return GearPairCandidate {
            module,
            teeth,
            fit,
            profile_shift: shifts,
            helix_angle,
            ratio,
            ratio_deviation,
            undercut: [false, false],
            corrected: false,
        };
    }
    let alpha = options.pressure_angle;
    let allowed = |x: f64| x.abs() <= options.max_profile_shift && (-1.0..=0.8).contains(&x);
    let total = shifts[0] + shifts[1];
    let mut x = shifts;
    for i in [0, 1] {
        let need = undercut_shift(teeth[i], alpha, helix_angle);
        if x[i] >= need {
            continue;
        }
        let mut moved = x;
        moved[i] = need;
        moved[1 - i] = total - need;
        if allowed(moved[0])
            && allowed(moved[1])
            && !undercuts(teeth[1 - i], alpha, helix_angle, moved[1 - i])
        {
            x = moved;
        }
    }
    GearPairCandidate {
        module,
        teeth,
        fit,
        profile_shift: x,
        helix_angle,
        ratio,
        ratio_deviation,
        undercut: [0, 1].map(|i| undercuts(teeth[i], alpha, helix_angle, x[i])),
        corrected: x != shifts,
    }
}

/// Whether a cycloidal gear of `teeth` with the default rolling circles
/// can be made (mitcad#2607).
fn cycloid_fits(module: f64, teeth: f64, helix_angle: f64) -> bool {
    let circle = gear_profile::cycloid::ROLLING_DIAMETER * module;
    gear_profile::cycloid::Cycloid {
        tooth: gear_profile::Tooth {
            module,
            teeth,
            pressure_angle: 20f64.to_radians(),
            helix_angle,
            shift: 0.0,
            backlash: 0.0,
        },
        addendum: circle,
        dedendum: circle,
    }
    .check_external()
    .is_ok()
}

/// The splits of `sum` teeth near the ratio, both counts within the limits
/// and the ratio within its tolerance: `(z1, z2, z2 / z1, deviation)`.
fn splits(
    sum: f64,
    ratio: f64,
    min_teeth: f64,
    max_teeth: f64,
    tolerance: f64,
) -> Vec<(f64, f64, f64, f64)> {
    let ideal = sum / (1.0 + ratio);
    let mut out: Vec<(f64, f64, f64, f64)> = Vec::new();
    for z1 in [ideal.floor(), ideal.ceil()] {
        let z2 = sum - z1;
        if z1 < min_teeth || z2 < min_teeth || z1 > max_teeth || z2 > max_teeth {
            continue;
        }
        let actual = z2 / z1;
        let deviation = actual / ratio - 1.0;
        if deviation.abs() > tolerance || out.iter().any(|s| s.0 == z1) {
            continue;
        }
        out.push((z1, z2, actual, deviation));
    }
    out
}

/// The profile shifts of a spur pair of `z1` and `z2` teeth of module `m`
/// that make the centre distance `a`, split as the module documentation
/// says; None when no working pressure angle gives it.
pub fn shifts_for(m: f64, z1: f64, z2: f64, a: f64, pressure_angle: f64) -> Option<[f64; 2]> {
    let sum = z1 + z2;
    let nominal = m * sum / 2.0;
    let c = nominal * pressure_angle.cos() / a;
    if !(c > 0.0 && c < 1.0) {
        return None;
    }
    let working = c.acos();
    let total = (involute(working) - involute(pressure_angle)) * sum / (2.0 * pressure_angle.tan());
    // More to the gear with fewer teeth when positive, less when negative.
    let share = if total >= 0.0 { z2 / sum } else { z1 / sum };
    let x1 = total * share;
    Some([x1, total - x1])
}

/// The gear pairs that fit axes `distance` apart, the best first (see the
/// module documentation).
pub fn gear_pairs(
    distance: f64,
    options: &GearPairOptions,
) -> Result<Vec<GearPairCandidate>, String> {
    let o = options;
    if !(distance > 0.0 && distance.is_finite()) {
        return Err(format!(
            "the axes must be apart to carry a pair of gears, got {distance} mm"
        ));
    }
    if !(o.ratio > 0.0 && o.ratio.is_finite()) {
        return Err(format!(
            "the ratio must be greater than zero, got {}",
            o.ratio
        ));
    }
    if !(o.spur || o.helical) {
        return Err("a gear pair is spur or helical".to_owned());
    }
    let max_helix = o.max_helix_angle.min(45f64.to_radians());
    let min_teeth = o.min_teeth.max(gear_profile::MIN_TEETH);
    let second: &[f64] = if o.second_series { MODULES_II } else { &[] };
    let mut modules: Vec<f64> = MODULES
        .iter()
        .chain(second)
        .copied()
        .filter(|m| *m >= o.modules.0 - 1.0e-12 && *m <= o.modules.1 + 1.0e-12)
        .collect();
    modules.sort_by(f64::total_cmp);
    let mut out = Vec::new();
    for m in modules {
        let exact_sum = 2.0 * distance / m;
        let split = |sum: f64| splits(sum, o.ratio, min_teeth, o.max_teeth, o.ratio_tolerance);
        let exact = |sum: f64| (m * sum / 2.0 - distance).abs() <= EXACT;
        if o.spur {
            let sum = exact_sum.round();
            if exact(sum) {
                for (z1, z2, ratio, ratio_deviation) in split(sum) {
                    out.push(candidate(
                        m,
                        [z1, z2],
                        Fit::Exact,
                        [0.0, 0.0],
                        0.0,
                        ratio,
                        ratio_deviation,
                        o,
                    ));
                }
            }
            let mut sums = vec![exact_sum.floor()];
            if exact_sum.ceil() != exact_sum.floor() {
                sums.push(exact_sum.ceil());
            }
            for sum in sums
                .into_iter()
                .filter(|s| *s > 0.0 && !exact(*s) && !o.cycloid)
            {
                for (z1, z2, ratio, ratio_deviation) in split(sum) {
                    let Some(x) = shifts_for(m, z1, z2, distance, o.pressure_angle) else {
                        continue;
                    };
                    let fits = x
                        .iter()
                        .all(|x| x.abs() <= o.max_profile_shift && (-1.0..=0.8).contains(x));
                    if fits {
                        out.push(candidate(
                            m,
                            [z1, z2],
                            Fit::ProfileShift,
                            x,
                            0.0,
                            ratio,
                            ratio_deviation,
                            o,
                        ));
                    }
                }
            }
        }
        if o.helical && max_helix >= MIN_HELIX_ANGLE {
            let low = (exact_sum * max_helix.cos()).ceil().max(1.0);
            let high = (exact_sum * MIN_HELIX_ANGLE.cos()).floor();
            let mut sum = low;
            while sum <= high {
                let beta = (m * sum / (2.0 * distance)).acos();
                if (MIN_HELIX_ANGLE - 1.0e-12..=max_helix + 1.0e-12).contains(&beta) {
                    for (z1, z2, ratio, ratio_deviation) in split(sum) {
                        out.push(candidate(
                            m,
                            [z1, z2],
                            Fit::Helix,
                            [0.0, 0.0],
                            beta,
                            ratio,
                            ratio_deviation,
                            o,
                        ));
                    }
                }
                sum += 1.0;
            }
        }
    }
    // Cycloidal gears whose default rolling circles make their teeth.
    if o.cycloid {
        out.retain(|c| {
            c.teeth
                .iter()
                .all(|z| cycloid_fits(c.module, *z, c.helix_angle))
        });
    }
    // Free of undercut first; then the ratio's deviation in steps of
    // [`DEVIATION_STEP`] (the ratio is what is asked), the fit, the pairs
    // that need no correction against undercut, the fewer teeth, the
    // smaller shifts and helix angle, the larger module.
    let step = |c: &GearPairCandidate| (c.ratio_deviation.abs() / DEVIATION_STEP).floor();
    out.sort_by(|a, b| {
        a.undercut
            .contains(&true)
            .cmp(&b.undercut.contains(&true))
            .then(step(a).total_cmp(&step(b)))
            .then(a.fit.cmp(&b.fit))
            .then(a.corrected.cmp(&b.corrected))
            .then((a.teeth[0] + a.teeth[1]).total_cmp(&(b.teeth[0] + b.teeth[1])))
            .then(a.cost().total_cmp(&b.cost()))
            .then(b.module.total_cmp(&a.module))
            .then(a.ratio_deviation.abs().total_cmp(&b.ratio_deviation.abs()))
    });
    out.truncate(o.limit);
    Ok(out)
}

// ---------------------------------------------------------------------------
// Belts

/// The pitch length of the open belt round pitch radii `r1` and `r2`, `c`
/// apart: L = 2 C cos(t) + pi (R + r) + 2 t (R - r), sin(t) = (R - r) / C.
pub fn open_belt_length(r1: f64, r2: f64, c: f64) -> f64 {
    let (big, small) = (r1.max(r2), r1.min(r2));
    let t = ((big - small) / c).clamp(-1.0, 1.0).asin();
    2.0 * c * t.cos() + PI * (big + small) + 2.0 * t * (big - small)
}

/// The centre distance at which an open belt of pitch length `length` goes
/// round pitch radii `r1` and `r2`; None when it is too short to clear
/// them.
pub fn center_for_length(r1: f64, r2: f64, length: f64) -> Option<f64> {
    let mut low = r1 + r2;
    if !(length.is_finite() && low > 0.0) || open_belt_length(r1, r2, low) >= length {
        return None;
    }
    let mut high = (length / 2.0).max(2.0 * low);
    let mut tries = 0;
    while open_belt_length(r1, r2, high) < length {
        high *= 2.0;
        tries += 1;
        if tries > 60 {
            return None;
        }
    }
    for _ in 0..200 {
        let middle = (low + high) / 2.0;
        if open_belt_length(r1, r2, middle) < length {
            low = middle;
        } else {
            high = middle;
        }
    }
    Some((low + high) / 2.0)
}

/// A closed belt of the parts library: its library id, pitch length, the
/// part's name and the belt's width (mm).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LibraryBelt {
    pub id: &'static str,
    pub length: f64,
    pub name: &'static str,
    pub width: f64,
}

/// The closed belts the parts library has of a profile
/// (`tools/libraries/profiles/linear-belts.json`, each with its supplier's
/// page there; the lengths are their pitch lengths, whole teeth); none for
/// the profiles it has no closed belts of.
pub fn library_belts(profile: BeltProfile) -> &'static [LibraryBelt] {
    const fn belt(id: &'static str, length: f64, name: &'static str, width: f64) -> LibraryBelt {
        LibraryBelt {
            id,
            length,
            name,
            width,
        }
    }
    const GT2: &[LibraryBelt] = &[
        belt(
            "belt-gt2-6-200",
            200.0,
            "Closed timing belt GT2 6 mm wide, 200 mm",
            6.0,
        ),
        belt(
            "belt-gt2-6-280",
            280.0,
            "Closed timing belt GT2 6 mm wide, 280 mm",
            6.0,
        ),
        belt(
            "belt-gt2-6-400",
            400.0,
            "Closed timing belt GT2 6 mm wide, 400 mm",
            6.0,
        ),
        belt(
            "belt-gt2-6-852",
            852.0,
            "Closed timing belt GT2 6 mm wide, 852 mm",
            6.0,
        ),
    ];
    const HTD3M: &[LibraryBelt] = &[
        belt(
            "belt-htd-3m-15-225",
            225.0,
            "Closed timing belt HTD 3M 15 mm wide, 225 mm",
            15.0,
        ),
        belt(
            "belt-htd-3m-9-255",
            255.0,
            "Closed timing belt HTD 3M 9 mm wide, 255 mm",
            9.0,
        ),
        belt(
            "belt-htd-3m-9-351",
            351.0,
            "Closed timing belt HTD 3M 9 mm wide, 351 mm",
            9.0,
        ),
    ];
    const HTD5M: &[LibraryBelt] = &[
        belt(
            "belt-htd-5m-15-255",
            255.0,
            "Closed timing belt HTD 5M 15 mm wide, 255 mm",
            15.0,
        ),
        belt(
            "belt-htd-5m-15-355",
            355.0,
            "Closed timing belt HTD 5M 15 mm wide, 355 mm",
            15.0,
        ),
        belt(
            "belt-htd-5m-15-575",
            575.0,
            "Closed timing belt HTD 5M 15 mm wide, 575 mm",
            15.0,
        ),
    ];
    match profile {
        BeltProfile::Gt2Pitch2 => GT2,
        BeltProfile::Htd3m => HTD3M,
        BeltProfile::Htd5m => HTD5M,
        BeltProfile::Gt2Pitch3 | BeltProfile::T5 | BeltProfile::Xl => &[],
    }
}

/// The fewest teeth Belt Drive gives a pulley of a profile by default:
/// Mitcad's defaults above the profile's least teeth (`min_teeth` of
/// `pulley_profile`), so that a belt is not bent round the smallest pulley
/// a groove allows; not a standard's values. `min_teeth` overrides them
/// down to the profile's least.
pub fn recommended_teeth(profile: BeltProfile) -> f64 {
    match profile {
        BeltProfile::Gt2Pitch2 | BeltProfile::Gt2Pitch3 => 16.0,
        BeltProfile::Htd3m | BeltProfile::Htd5m => 14.0,
        BeltProfile::T5 | BeltProfile::Xl => 12.0,
    }
}

/// The fewest teeth in mesh on the smaller pulley by default.
pub const MIN_TEETH_IN_MESH: f64 = 6.0;

/// The most teeth the `belt_drive` query takes as `max_teeth` (mitcad#748):
/// a pulley of 1000 GT2 teeth is 0.64 m across; a larger bound is taken
/// for "no limit" and refused.
pub const MAX_TEETH: f64 = 1000.0;

/// Within how much (mm) two travels count as the same in Belt Drive's
/// order, so that the smaller pulleys come first among them.
pub const TRAVEL_STEP: f64 = 0.5;

/// The belts a drive is fitted with.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Catalogue<'a> {
    /// Pitch lengths given (mm).
    Given(&'a [f64]),
    /// The parts library's closed belts.
    Library(&'static [LibraryBelt]),
    /// Every whole number of teeth: the nearest is at most one pitch of
    /// travel away (a pitch of belt is half a pitch of centre distance).
    WholeTeeth,
}

impl Catalogue<'_> {
    /// As the queries name it.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Given(_) => "given",
            Self::Library(_) => "library",
            Self::WholeTeeth => "whole_teeth",
        }
    }

    /// The catalogue of a profile: the lengths given, else the library's
    /// belts, else (or with `whole_teeth`) whole teeth.
    pub fn of(
        given: Option<&[f64]>,
        profile: Option<BeltProfile>,
        whole_teeth: bool,
    ) -> Catalogue<'_> {
        match (given, profile) {
            (Some(lengths), _) => Catalogue::Given(lengths),
            _ if whole_teeth => Catalogue::WholeTeeth,
            (None, Some(profile)) if !library_belts(profile).is_empty() => {
                Catalogue::Library(library_belts(profile))
            }
            _ => Catalogue::WholeTeeth,
        }
    }
}

/// A belt that goes round a pair of pulleys.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BeltFit {
    /// Its pitch length (mm) and teeth (length / pitch).
    pub length: f64,
    pub teeth: f64,
    /// The centre distance it fits at, and the travel to it from the axes'
    /// distance (positive: apart).
    pub center_distance: f64,
    pub travel: f64,
    /// The parts library's belt, from the library catalogue.
    pub library: Option<&'static LibraryBelt>,
    /// Whether it fits as it is, needs a tensioner or is short.
    pub fit: BeltVerdict,
}

/// Within how much travel (mm, either way) a belt fits as it is.
pub const FIT_TOLERANCE: f64 = 0.5;

/// How a belt fits the axes' distance, by its travel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BeltVerdict {
    /// Within [`FIT_TOLERANCE`]: it goes on as the axes are.
    Ok,
    /// Longer than the path: its slack is taken up by moving the axes
    /// apart by the travel or by a tensioner.
    Tensioner,
    /// Shorter than the path: the axes must move closer by the travel.
    Short,
}

impl BeltVerdict {
    /// The verdict of a travel (positive: the axes apart).
    pub fn of(travel: f64) -> Self {
        if travel.abs() <= FIT_TOLERANCE {
            Self::Ok
        } else if travel > 0.0 {
            Self::Tensioner
        } else {
            Self::Short
        }
    }
}

/// The belts of `catalogue` that go round pitch radii `r1` and `r2` whose
/// axes are `c` apart, the `count` with the smallest travel first.
pub fn belt_fits(
    r1: f64,
    r2: f64,
    c: f64,
    pitch: f64,
    catalogue: Catalogue<'_>,
    count: usize,
) -> Vec<BeltFit> {
    let length = open_belt_length(r1, r2, c);
    let lengths: Vec<(f64, Option<&'static LibraryBelt>)> = match catalogue {
        Catalogue::Given(lengths) => lengths.iter().map(|l| (*l, None)).collect(),
        Catalogue::Library(belts) => belts.iter().map(|b| (b.length, Some(b))).collect(),
        Catalogue::WholeTeeth => {
            let n = (length / pitch).floor();
            (-2..=3)
                .map(|k| ((n + f64::from(k)) * pitch, None))
                .collect()
        }
    };
    let mut fits: Vec<BeltFit> = Vec::new();
    for (l, library) in lengths {
        if l.is_nan() || l <= 0.0 || fits.iter().any(|f| (f.length - l).abs() < 1.0e-9) {
            continue;
        }
        if let Some(center) = center_for_length(r1, r2, l) {
            fits.push(BeltFit {
                length: l,
                teeth: l / pitch,
                center_distance: center,
                travel: center - c,
                library,
                fit: BeltVerdict::of(center - c),
            });
        }
    }
    fits.sort_by(|a, b| a.travel.abs().total_cmp(&b.travel.abs()));
    fits.truncate(count);
    fits
}

/// What the Belt Drive search takes.
#[derive(Debug, Clone, PartialEq)]
pub struct BeltDriveOptions {
    /// z2 / z1.
    pub ratio: f64,
    pub profile: BeltProfile,
    /// Catalogue pitch lengths (mm); the library's belts, else whole
    /// teeth, without them.
    pub catalogue: Option<Vec<f64>>,
    /// Whole teeth even where the library has belts.
    pub whole_teeth: bool,
    /// [`recommended_teeth`] without it; never below the profile's least.
    pub min_teeth: Option<f64>,
    pub max_teeth: f64,
    /// The fewest teeth in mesh on the smaller pulley.
    pub min_teeth_in_mesh: f64,
    pub ratio_tolerance: f64,
    /// The belts per candidate.
    pub belts: usize,
    pub limit: usize,
}

impl BeltDriveOptions {
    pub fn new(ratio: f64, profile: BeltProfile) -> Self {
        Self {
            ratio,
            profile,
            catalogue: None,
            whole_teeth: false,
            min_teeth: None,
            max_teeth: 150.0,
            min_teeth_in_mesh: MIN_TEETH_IN_MESH,
            ratio_tolerance: 0.02,
            belts: 3,
            limit: 20,
        }
    }

    /// The belts the drives are fitted with.
    pub fn catalogue(&self) -> Catalogue<'_> {
        Catalogue::of(
            self.catalogue.as_deref(),
            Some(self.profile),
            self.whole_teeth,
        )
    }
}

/// A pair of pulleys that fits.
#[derive(Debug, Clone, PartialEq)]
pub struct BeltCandidate {
    /// `[z1, z2]`.
    pub teeth: [f64; 2],
    pub ratio: f64,
    pub ratio_deviation: f64,
    pub pitch_diameters: [f64; 2],
    /// The pitch length at the axes' distance.
    pub belt_length: f64,
    /// The whole teeth in mesh on the smaller pulley.
    pub teeth_in_mesh: f64,
    /// The belts that fit, the smallest travel first.
    pub belts: Vec<BeltFit>,
}

/// The whole teeth of `teeth` in mesh with an open belt on the smaller of
/// pitch radii `r` and `other`, `c` apart.
pub fn teeth_in_mesh(teeth: f64, r: f64, other: f64, c: f64) -> f64 {
    let t = ((r - other).abs() / c).clamp(-1.0, 1.0).asin();
    let wrap = if r <= other {
        PI - 2.0 * t
    } else {
        PI + 2.0 * t
    };
    (teeth * wrap / TAU + 1.0e-9).floor()
}

/// The pulley pairs that fit axes `distance` apart (see the module
/// documentation): the smallest deviation of the ratio first, deviations
/// within [`DEVIATION_STEP`] of each other counting as one; then the
/// smallest travel, travels within [`TRAVEL_STEP`] counting as one; then
/// the smaller pulleys, the smaller deviation, the smaller travel.
pub fn belt_drives(
    distance: f64,
    options: &BeltDriveOptions,
) -> Result<Vec<BeltCandidate>, String> {
    let o = options;
    if !(distance > 0.0 && distance.is_finite()) {
        return Err(format!(
            "the axes must be apart to carry a belt drive, got {distance} mm"
        ));
    }
    if !(o.ratio > 0.0 && o.ratio.is_finite()) {
        return Err(format!(
            "the ratio must be greater than zero, got {}",
            o.ratio
        ));
    }
    let data = o.profile.data();
    let min_teeth = o
        .min_teeth
        .unwrap_or(recommended_teeth(o.profile))
        .max(data.min_teeth);
    let catalogue = o.catalogue();
    let fits_teeth = |z: f64| {
        pulley_profile::check_teeth(
            o.profile,
            z,
            pulley_profile::outside_diameter(o.profile, z, 0.0) / 2.0,
        )
        .is_ok()
    };
    let mut out = Vec::new();
    let mut z1 = min_teeth.ceil();
    while z1 <= o.max_teeth {
        let ideal = z1 * o.ratio;
        // Both pulleys grow with z1: once the smaller pair of this z1 no
        // longer fits between the axes, no larger one does (mitcad#748: a
        // huge `max_teeth` returns at once, and the loop never reaches the
        // counts where `z1 += 1.0` stops changing z1).
        let r1 = pulley_profile::pitch_diameter(o.profile, z1) / 2.0;
        let smallest = pulley_profile::pitch_diameter(o.profile, ideal.floor().max(0.0)) / 2.0;
        if r1 + smallest >= distance {
            break;
        }
        let mut seconds = vec![ideal.floor()];
        if ideal.ceil() != ideal.floor() {
            seconds.push(ideal.ceil());
        }
        for z2 in seconds {
            if z2 < min_teeth || z2 > o.max_teeth {
                continue;
            }
            let ratio = z2 / z1;
            let ratio_deviation = ratio / o.ratio - 1.0;
            if ratio_deviation.abs() > o.ratio_tolerance || !fits_teeth(z1) || !fits_teeth(z2) {
                continue;
            }
            let d1 = pulley_profile::pitch_diameter(o.profile, z1);
            let d2 = pulley_profile::pitch_diameter(o.profile, z2);
            let (r1, r2) = (d1 / 2.0, d2 / 2.0);
            if r1 + r2 >= distance {
                continue;
            }
            let (small_teeth, small, large) = if r1 <= r2 { (z1, r1, r2) } else { (z2, r2, r1) };
            let in_mesh = teeth_in_mesh(small_teeth, small, large, distance);
            if in_mesh < o.min_teeth_in_mesh {
                continue;
            }
            let belts = belt_fits(r1, r2, distance, data.pitch, catalogue, o.belts);
            if belts.is_empty() {
                continue;
            }
            out.push(BeltCandidate {
                teeth: [z1, z2],
                ratio,
                ratio_deviation,
                pitch_diameters: [d1, d2],
                belt_length: open_belt_length(r1, r2, distance),
                teeth_in_mesh: in_mesh,
                belts,
            });
        }
        z1 += 1.0;
    }
    // The ratio is what is asked: its deviation in steps first.
    let deviation = |c: &BeltCandidate| (c.ratio_deviation.abs() / DEVIATION_STEP).floor();
    let step = |c: &BeltCandidate| (c.belts[0].travel.abs() / TRAVEL_STEP).floor();
    out.sort_by(|a, b| {
        deviation(a)
            .total_cmp(&deviation(b))
            .then(step(a).total_cmp(&step(b)))
            .then(
                a.teeth[0]
                    .min(a.teeth[1])
                    .total_cmp(&b.teeth[0].min(b.teeth[1])),
            )
            .then(a.ratio_deviation.abs().total_cmp(&b.ratio_deviation.abs()))
            .then(a.belts[0].travel.abs().total_cmp(&b.belts[0].travel.abs()))
    });
    out.truncate(o.limit);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::gear_profile::{Tooth, mesh_distance};

    fn spur(module: f64, teeth: f64, shift: f64, helix: f64) -> Tooth {
        Tooth {
            module,
            teeth,
            pressure_angle: 20f64.to_radians(),
            helix_angle: helix,
            shift,
            backlash: 0.0,
        }
    }

    #[test]
    fn an_exact_pair_comes_first_free_of_undercut() {
        let options = GearPairOptions {
            ratio: 3.0,
            limit: 1000,
            ..GearPairOptions::default()
        };
        let pairs = gear_pairs(40.0, &options).unwrap();
        // Among the exact pairs those needing no correction first, the
        // fewest teeth first among them: 20 and 60 of 1 mm, ...; then 16
        // and 48 of 1.25 mm, corrected against undercut. Nothing below
        // 0.5 mm.
        assert!(pairs.iter().all(|p| p.module >= MIN_MODULE), "{pairs:?}");
        let exact: Vec<_> = pairs
            .iter()
            .filter(|p| p.fit == Fit::Exact && p.ratio_deviation == 0.0)
            .map(|p| (p.module, p.teeth))
            .collect();
        assert_eq!(
            exact,
            [
                (1.0, [20.0, 60.0]),
                (0.8, [25.0, 75.0]),
                (0.5, [40.0, 120.0]),
                (1.25, [16.0, 48.0])
            ],
            "{pairs:?}"
        );
        assert_eq!(pairs[0].teeth, [20.0, 60.0], "{pairs:?}");
        assert_eq!(pairs[0].profile_shift, [0.0, 0.0], "{pairs:?}");
        assert!(!pairs[0].corrected, "{pairs:?}");
        let small = *pairs
            .iter()
            .find(|p| p.teeth == [16.0, 48.0])
            .expect("the 16 and 48 teeth pair");
        assert_eq!(small.fit, Fit::Exact, "{small:?}");
        assert!(small.corrected, "{small:?}");
        // 16 teeth at 20 degrees undercut: shifted out of it, the sum (so
        // the distance) kept.
        let need = undercut_shift(16.0, 20f64.to_radians(), 0.0);
        assert!((need - (1.0 - 8.0 * 20f64.to_radians().sin().powi(2))).abs() < 1.0e-12);
        assert!(
            need > 0.0 && (small.profile_shift[0] - need).abs() < 1.0e-12,
            "{small:?}"
        );
        assert_eq!(small.profile_shift[0] + small.profile_shift[1], 0.0);
        assert_eq!(small.undercut, [false, false]);
        // Those free of undercut first.
        assert!(
            pairs
                .windows(2)
                .all(|w| !w[0].undercut.contains(&true) || w[1].undercut.contains(&true)),
            "{pairs:?}"
        );
        // No shift allowed: flagged, and after every pair free of it.
        let unshifted = GearPairOptions {
            max_profile_shift: 0.0,
            ..options
        };
        let pairs = gear_pairs(40.0, &unshifted).unwrap();
        let at = pairs
            .iter()
            .position(|p| p.teeth == [16.0, 48.0])
            .expect("the 16 and 48 teeth pair");
        assert_eq!(pairs[at].undercut, [true, false], "{pairs:?}");
        assert!(pairs[..at].iter().all(|p| !p.undercut.contains(&true)));
        assert!(pairs[at..].iter().all(|p| p.undercut.contains(&true)));
        // Smaller modules when asked.
        let fine = GearPairOptions {
            modules: (0.3, 0.5),
            ..options
        };
        let pairs = gear_pairs(40.0, &fine).unwrap();
        assert!(pairs.iter().any(|p| p.module == 0.4), "{pairs:?}");
        let pairs = gear_pairs(40.0, &unshifted).unwrap();
        for pair in pairs.iter().filter(|p| p.fit == Fit::Exact) {
            let sum = pair.teeth[0] + pair.teeth[1];
            assert!((pair.module * sum / 2.0 - 40.0).abs() <= EXACT, "{pair:?}");
            assert!(pair.teeth.iter().all(|z| *z >= 12.0), "{pair:?}");
        }
        // Among the pairs free of undercut and among the others: the
        // ratio's deviation in its steps first, the fits in their order
        // within a step.
        let step = |p: &GearPairCandidate| (p.ratio_deviation.abs() / DEVIATION_STEP).floor();
        assert!(
            pairs.windows(2).all(|w| {
                w[0].undercut.contains(&true) != w[1].undercut.contains(&true)
                    || step(&w[0]) < step(&w[1])
                    || (step(&w[0]) == step(&w[1]) && w[0].fit <= w[1].fit)
            }),
            "{pairs:?}"
        );
    }

    /// The ratio is what is asked: a pair that meets it with a profile
    /// shift comes before one that misses it unshifted.
    #[test]
    fn the_ratio_comes_before_the_fit() {
        let options = GearPairOptions {
            ratio: 2.0,
            helical: false,
            modules: (0.7, 1.5),
            limit: 1000,
            ..GearPairOptions::default()
        };
        let pairs = gear_pairs(60.0, &options).unwrap();
        let at = |module: f64, teeth: [f64; 2]| {
            pairs
                .iter()
                .position(|p| p.module == module && p.teeth == teeth)
                .unwrap_or_else(|| panic!("{module} {teeth:?} in {pairs:?}"))
        };
        let shifted = at(0.7, [57.0, 114.0]);
        let off_ratio = at(1.5, [27.0, 53.0]);
        assert_eq!(pairs[shifted].fit, Fit::ProfileShift);
        assert_eq!(pairs[shifted].ratio_deviation, 0.0);
        assert_eq!(pairs[off_ratio].fit, Fit::Exact);
        assert!(pairs[off_ratio].ratio_deviation < -0.018, "{pairs:?}");
        assert!(shifted < off_ratio, "{pairs:?}");
        // Among those meeting the ratio the exact ones first.
        assert!(at(1.0, [40.0, 80.0]) < shifted, "{pairs:?}");
    }

    #[test]
    fn a_small_difference_is_taken_up_by_profile_shifts() {
        let options = GearPairOptions {
            ratio: 1.0,
            helical: false,
            modules: (2.0, 2.0),
            ..GearPairOptions::default()
        };
        let pairs = gear_pairs(40.3, &options).unwrap();
        assert_eq!(pairs.len(), 1, "{pairs:?}");
        let pair = pairs[0];
        assert_eq!(pair.fit, Fit::ProfileShift);
        assert_eq!(pair.teeth, [20.0, 20.0]);
        let [x1, x2] = pair.profile_shift;
        assert!(x1 > 0.0 && (x1 - x2).abs() < 1.0e-12, "{pair:?}");
        // The mesh of the shifted gears is the axes' distance.
        let (_, a) = mesh_distance(&spur(2.0, 20.0, x1, 0.0), &spur(2.0, 20.0, x2, 0.0), false);
        assert!((a - 40.3).abs() < 1.0e-9, "{a}");
        // Uneven teeth: the pinion takes more of a positive sum.
        let [x1, x2] = shifts_for(2.0, 15.0, 30.0, 45.4, 20f64.to_radians()).unwrap();
        assert!(x1 > x2 && x2 > 0.0, "{x1} {x2}");
        let (_, a) = mesh_distance(&spur(2.0, 15.0, x1, 0.0), &spur(2.0, 30.0, x2, 0.0), false);
        assert!((a - 45.4).abs() < 1.0e-9, "{a}");
        // Beyond the shift limit: nothing.
        let tight = GearPairOptions {
            max_profile_shift: 0.05,
            ..options
        };
        let pairs = gear_pairs(40.3, &tight).unwrap();
        assert!(pairs.is_empty(), "{pairs:?}");
    }

    #[test]
    fn helical_teeth_make_the_distance_exact() {
        let options = GearPairOptions {
            ratio: 2.0,
            spur: false,
            modules: (2.0, 2.0),
            ..GearPairOptions::default()
        };
        let pairs = gear_pairs(42.0, &options).unwrap();
        let first = pairs[0];
        assert_eq!(first.fit, Fit::Helix, "{pairs:?}");
        assert_eq!(first.teeth, [13.0, 26.0], "{pairs:?}");
        assert_eq!(first.ratio_deviation, 0.0);
        let beta = first.helix_angle;
        // 13 helical teeth (about 16.2 virtual ones) undercut: shifted out
        // of it by the virtual teeth, the sum kept.
        let need = undercut_shift(13.0, 20f64.to_radians(), beta);
        assert!(need > 0.0, "{need}");
        assert!((first.profile_shift[0] - need).abs() < 1.0e-12, "{first:?}");
        assert_eq!(first.profile_shift[0] + first.profile_shift[1], 0.0);
        assert_eq!(first.undercut, [false, false]);
        assert!(
            beta > MIN_HELIX_ANGLE && beta < 30f64.to_radians(),
            "{beta}"
        );
        for pair in &pairs {
            let a = mesh_distance(
                &spur(2.0, pair.teeth[0], 0.0, pair.helix_angle),
                &spur(2.0, pair.teeth[1], 0.0, -pair.helix_angle),
                false,
            )
            .1;
            assert!((a - 42.0).abs() < 1.0e-9, "{pair:?}: {a}");
        }
        // No helix fits the exact sum (the spur pair's).
        assert!(pairs.iter().all(|p| p.teeth[0] + p.teeth[1] != 42.0));
    }

    #[test]
    fn a_belt_length_gives_back_its_centre_distance() {
        let (r1, r2) = (6.0, 15.0);
        for c in [30.0, 80.0, 250.0] {
            let length = open_belt_length(r1, r2, c);
            let back = center_for_length(r1, r2, length).unwrap();
            assert!((back - c).abs() < 1.0e-9, "{c}: {back}");
        }
        // Too short to go round them.
        assert!(center_for_length(r1, r2, 60.0).is_none());
    }

    #[test]
    fn belt_drives_come_with_the_smallest_travel_first() {
        let options = BeltDriveOptions {
            whole_teeth: true,
            ..BeltDriveOptions::new(2.0, BeltProfile::Gt2Pitch2)
        };
        assert_eq!(options.catalogue(), Catalogue::WholeTeeth);
        let drives = belt_drives(100.0, &options).unwrap();
        assert!(!drives.is_empty());
        for drive in &drives {
            assert!(
                (drive.teeth[1] / drive.teeth[0] / 2.0 - 1.0).abs() <= 0.02,
                "{drive:?}"
            );
            // The recommended teeth and teeth in mesh.
            assert!(drive.teeth.iter().all(|z| *z >= 16.0), "{drive:?}");
            assert!(drive.teeth_in_mesh >= MIN_TEETH_IN_MESH, "{drive:?}");
            let (r1, r2) = (
                drive.pitch_diameters[0] / 2.0,
                drive.pitch_diameters[1] / 2.0,
            );
            assert!((drive.belt_length - open_belt_length(r1, r2, 100.0)).abs() < 1.0e-9);
            for belt in &drive.belts {
                // Whole teeth, and the belt fits at the distance moved by
                // its travel.
                assert!((belt.teeth - belt.teeth.round()).abs() < 1.0e-9, "{belt:?}");
                assert!(belt.library.is_none(), "{belt:?}");
                let at = open_belt_length(r1, r2, 100.0 + belt.travel);
                assert!((at - belt.length).abs() < 1.0e-6, "{belt:?}: {at}");
            }
            assert!(
                drive
                    .belts
                    .windows(2)
                    .all(|w| w[0].travel.abs() <= w[1].travel.abs())
            );
            // A whole tooth is at most a pitch of travel away.
            assert!(drive.belts[0].travel.abs() <= 2.0, "{drive:?}");
        }
        // The travel's steps, then the smaller pulleys: the first drive is
        // the smallest that meets the defaults.
        let step = |d: &BeltCandidate| (d.belts[0].travel.abs() / TRAVEL_STEP).floor();
        assert!(drives.windows(2).all(|w| step(&w[0]) <= step(&w[1])));
        assert_eq!(drives[0].teeth, [16.0, 32.0], "{drives:?}");
        // Fewer teeth only when asked, never below the profile's least.
        let small = BeltDriveOptions {
            min_teeth: Some(4.0),
            min_teeth_in_mesh: 0.0,
            ..options.clone()
        };
        let drives = belt_drives(100.0, &small).unwrap();
        assert_eq!(drives[0].teeth[0], 10.0, "{drives:?}");
        // A catalogue: only its belts.
        let given = BeltDriveOptions {
            catalogue: Some(vec![300.0, 400.0]),
            ..options
        };
        let drives = belt_drives(100.0, &given).unwrap();
        assert!(!drives.is_empty());
        for drive in &drives {
            assert!(
                drive
                    .belts
                    .iter()
                    .all(|b| b.length == 300.0 || b.length == 400.0),
                "{drive:?}"
            );
        }
    }

    #[test]
    fn a_belt_s_verdict_comes_from_its_travel() {
        assert_eq!(BeltVerdict::of(0.0), BeltVerdict::Ok);
        assert_eq!(BeltVerdict::of(FIT_TOLERANCE), BeltVerdict::Ok);
        assert_eq!(BeltVerdict::of(-FIT_TOLERANCE), BeltVerdict::Ok);
        assert_eq!(BeltVerdict::of(0.6), BeltVerdict::Tensioner);
        assert_eq!(BeltVerdict::of(-0.6), BeltVerdict::Short);
        // A library belt at the distance it fits at: ok; 10 mm further
        // apart it is short, 10 mm closer it needs a tensioner.
        let (r1, r2) = (5.093, 10.186);
        let c = center_for_length(r1, r2, 280.0).unwrap();
        let catalogue = Catalogue::Library(library_belts(BeltProfile::Gt2Pitch2));
        let at = |c: f64| {
            belt_fits(r1, r2, c, 2.0, catalogue, 4)
                .into_iter()
                .find(|b| b.length == 280.0)
                .expect("the 280 mm belt")
        };
        assert_eq!(at(c).fit, BeltVerdict::Ok);
        assert_eq!(at(c + 10.0).fit, BeltVerdict::Short);
        assert_eq!(at(c - 10.0).fit, BeltVerdict::Tensioner);
        assert_eq!(at(c).library.map(|b| b.width), Some(6.0));
    }

    /// The ratio is what is asked: the pairs meeting it come before those
    /// that miss it, whatever their travel.
    #[test]
    fn belt_drives_meet_the_ratio_first() {
        let options = BeltDriveOptions {
            whole_teeth: true,
            limit: 1000,
            ..BeltDriveOptions::new(1.5, BeltProfile::Gt2Pitch2)
        };
        let drives = belt_drives(100.0, &options).unwrap();
        // Odd teeth on the first pulley miss 1.5 (17:25 by 1.96 %).
        assert!(
            drives.iter().any(|d| d.ratio_deviation != 0.0),
            "{drives:?}"
        );
        assert_eq!(drives[0].ratio_deviation, 0.0, "{drives:?}");
        assert_eq!(drives[0].teeth, [16.0, 24.0], "{drives:?}");
        let deviation = |d: &BeltCandidate| (d.ratio_deviation.abs() / DEVIATION_STEP).floor();
        let step = |d: &BeltCandidate| (d.belts[0].travel.abs() / TRAVEL_STEP).floor();
        assert!(
            drives
                .windows(2)
                .all(|w| deviation(&w[0]) < deviation(&w[1])
                    || (deviation(&w[0]) == deviation(&w[1]) && step(&w[0]) <= step(&w[1]))),
            "{drives:?}"
        );
    }

    #[test]
    fn the_library_belts_fit_by_default() {
        let options = BeltDriveOptions::new(2.0, BeltProfile::Gt2Pitch2);
        assert_eq!(options.catalogue().name(), "library");
        let drives = belt_drives(100.0, &options).unwrap();
        assert!(!drives.is_empty());
        let lengths: Vec<f64> = library_belts(BeltProfile::Gt2Pitch2)
            .iter()
            .map(|b| b.length)
            .collect();
        for drive in &drives {
            for belt in &drive.belts {
                assert!(lengths.contains(&belt.length), "{belt:?}");
                assert!(belt.library.is_some(), "{belt:?}");
            }
        }
        // Profiles without library belts: whole teeth.
        assert_eq!(
            BeltDriveOptions::new(2.0, BeltProfile::Xl).catalogue(),
            Catalogue::WholeTeeth
        );
    }

    /// The library catalogue is the parts library's closed belts: each id
    /// is there, named with its length.
    #[test]
    fn the_library_belts_are_the_parts_library_s() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tools/libraries/profiles/linear-belts.json");
        let text = std::fs::read_to_string(&path).expect("the library's belts");
        let parts: Vec<serde_json::Value> = serde_json::from_str(&text).unwrap();
        for profile in [
            BeltProfile::Gt2Pitch2,
            BeltProfile::Htd3m,
            BeltProfile::Htd5m,
        ] {
            for belt in library_belts(profile) {
                let part = parts
                    .iter()
                    .find(|p| p["id"] == belt.id)
                    .unwrap_or_else(|| panic!("{} is not in the library", belt.id));
                let name = part["name"].as_str().unwrap();
                assert!(name.starts_with("Closed timing belt"), "{name}");
                assert_eq!(name, belt.name);
                assert!(
                    name.contains(&format!(" {} mm wide,", belt.width)),
                    "{name}"
                );
                assert!(name.ends_with(&format!(", {} mm", belt.length)), "{name}");
                assert!(
                    (belt.length / profile.data().pitch).fract() == 0.0,
                    "{belt:?}"
                );
            }
        }
    }

    /// A huge `max_teeth` (an API client's "no limit") returns at once
    /// with the candidates of the default (mitcad#748): the search stops
    /// where the pulleys no longer fit between the axes, here at 63 teeth
    /// each, below the default's 150.
    #[test]
    fn a_huge_max_teeth_stops_where_the_pulleys_no_longer_fit() {
        let options = BeltDriveOptions {
            whole_teeth: true,
            limit: 1000,
            ..BeltDriveOptions::new(1.0, BeltProfile::Gt2Pitch2)
        };
        let default = belt_drives(40.0, &options).unwrap();
        assert!(!default.is_empty());
        for max_teeth in [1.0e9, 1.0e16, f64::INFINITY] {
            let huge = BeltDriveOptions {
                max_teeth,
                ..options.clone()
            };
            assert_eq!(belt_drives(40.0, &huge).unwrap(), default, "{max_teeth}");
        }
        // A ratio that makes the second pulley huge stops as soon.
        let steep = BeltDriveOptions {
            max_teeth: 1.0e16,
            ..BeltDriveOptions::new(1.0e6, BeltProfile::Gt2Pitch2)
        };
        assert!(belt_drives(40.0, &steep).unwrap().is_empty());
    }
}
