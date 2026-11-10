// SPDX-License-Identifier: MIT
//! Printable bearing (mitcad#577): a deep groove bearing that a 3D printer
//! makes in one piece, its inner race, outer race and rolling elements
//! each a body of its own with clearances between them, or a variant that
//! takes bought 6 mm airsoft balls in a snap-in cage. The standard sizes
//! are in `bearing_sizes.rs`.
//!
//! **Section.** Everything is worked out in the bearing's section: `R` the
//! distance from its axis, `z` along it from the face on the plane (the
//! print bed: the bearing is printed with its axis vertical) to the far
//! face at the width `W`. The elements turn about axes parallel to the
//! bearing's on the pitch circle (radius `Rp`), so swept round the
//! bearing's axis an element of radius `rho(z)` at height `z` fills
//! `Rp - rho(z) .. Rp + rho(z)`; each race is bounded by the same function
//! `race(z)` on its side, `R = Rp - race(z)` for the inner race and
//! `Rp + race(z)` for the outer one, so the two races mirror each other
//! about the pitch circle.
//!
//! **Elements** (their largest diameter `D`, radius `r`; `t = tan(a)` the
//! slope of the flanks, `a` the flank angle, 45 degrees by default):
//!
//! - `dumbbell`: a waisted roller from face to face: radius `r`, a V waist
//!   `h` deep (`min(D/4, 0.8 t W/2)`) in the middle; the races carry a V
//!   ridge into the waist, so each element touches each race on two
//!   flanks, centres itself and is held along the axis;
//! - `roller`: a cylinder from face to face with its ends chamfered by
//!   `k` (`min(D/4, t W/4)`) at the flank angle; the races have lips over
//!   the chamfers;
//! - `ball`: a printed ball of diameter `D` centred at `W/2`, with conical
//!   caps tangent to it at the flank angle's latitude and cut by the faces,
//!   so that it stands on the bed and overhangs by the flank angle at most
//!   (it needs `D >= W sin(a)`); the races have V grooves whose flanks
//!   follow the caps and a flat bottom at the ball's equator;
//! - `airsoft`: bought airsoft balls (`D` 5.95 mm by default, 6 mm
//!   nominal) in the same grooves, the races' lands `0.55 r` from the pitch
//!   circle, and a snap-in cage: a ring above the balls (to the far face)
//!   with fingers between them down past their equators, the pockets
//!   spheres `r + radial_clearance` round the balls whose openings are
//!   0.2 mm narrower than a ball. The races are printed in place without
//!   balls; the balls go in by hand with the inner race pushed off centre
//!   (so at most the balls that fit the crescent, see [`Layout`]), and the
//!   cage, printed on its ring, snaps in from the far face.
//!
//! **Races.** With the radial clearance `cr` across the cylindrical faces
//! and the axial clearance `ca` along the axis at the flanks (a flank's
//! normal gap is `t ca / sqrt(1 + t^2)`): `race(z)` is
//! `min(r + cr, r - h + t ca + t |z - W/2|)` (dumbbell),
//! `min(r + cr, r - k + t ca + t z, r - k + t ca + t (W - z))` (roller) and
//! `max(land, min(r + cr, r / cos(a) + t ca - t |z - W/2|))` (balls). The
//! races' outer edges (the bore's and the outside's) have 45-degree
//! chamfers; a flange lies on the outer race at the face on the plane.
//!
//! **Placement.** On a plane with a centre, as Pulley and Gear take them:
//! the axis is the plane's normal through `center`, the bearing runs along
//! it from the plane, `angle` turns its elements about the axis. Or
//! `groove`: the raceway is cut into a cylindrical face of a picked body,
//! which becomes one race (a bore's body the outer race, a shaft's the
//! inner race), `offset` along the face's axis from its low end (the
//! middle of the face without it); the other race is a new body. Or
//! `seat` (mitcad#578, `seat.rs`): a shaft's or a hole's circular edge or
//! cylindrical face; the bearing lies on the plane through the seat's
//! origin (its offset from the end: how far along the shaft, how deep in
//! the hole) normal to its axis and runs along it into the shaft or the
//! hole. Without a size or custom dimensions it takes the standard size
//! whose bore fits the shaft (whose outside diameter fits the hole) within
//! `fit_clearance` (0.5 mm), the nearest.
//!
//! **Faces.** The inner race's faces are `side(c1…)` (its section's lines
//! from the bore's lower chamfer round), the outer race's `side(c101…)`,
//! an element's `side(c201…)` (the first; the others are copies
//! `inst<k>(…)` turned about the axis), a printed ball's arc `side(c204…)`,
//! an airsoft ball `side0`, the cage's `side(c301…)`; the groove cut into a
//! body `side(c401…)`.

use std::f64::consts::{FRAC_PI_4, PI, TAU};

use serde::{Deserialize, Serialize};

use mitcad_vec3::{add_scaled, combine, unit_div};

use super::bearing_sizes::{self, Fit, short_number};
use super::extrude::Operation;
use super::geom_ref::{GeomRef, Want};
use super::place_at::{PlaceAt, map_at};
use super::seat::{Seat, SeatFrame};
use super::{
    BodyChange, CheckContext, EvalContext, Evaluate, FeatureInfo, FeatureOutput, References,
};
use crate::ids::{BodyUid, EntityUid};
use crate::kernel::{Axis, BooleanOp, Kernel, RevolveSpec};
use crate::parameters::ParamId;
use crate::profile::{ProfileLoop, ProfileRegion, ProfileSegment, SegmentGeometry, SketchFrame};
use crate::topo::{RegionKey, SegmentKey};
use crate::transform::{Instance, PrimitiveShape, PrimitiveSpec, Transform};
use mitcad_vec3::cross;

/// FDM defaults (mm): the gap across the cylindrical faces, the gap along
/// the axis at the flanks, between neighbouring elements, the races'
/// chamfers and the smallest wall.
pub const DEFAULT_RADIAL_CLEARANCE: f64 = 0.4;
pub const DEFAULT_AXIAL_CLEARANCE: f64 = 0.4;
pub const DEFAULT_ELEMENT_GAP: f64 = 0.6;
pub const DEFAULT_CHAMFER: f64 = 0.4;
pub const DEFAULT_MIN_WALL: f64 = 1.2;
/// The flanks' angle from the axis: their overhang printed with the axis
/// vertical.
pub const DEFAULT_FLANK_ANGLE: f64 = FRAC_PI_4;
/// The steepest overhang an FDM printer makes without supports.
pub const MAX_OVERHANG: f64 = FRAC_PI_4;
/// The smallest clearance or gap that an FDM printer keeps open.
pub const MIN_CLEARANCE: f64 = 0.2;
/// How far an element reaches past the raceway at least, to be held.
pub const MIN_HOLD: f64 = 0.2;
/// The smallest ball that prints without sticking or rattling.
pub const MIN_PRINTED_BALL: f64 = 3.0;
/// An airsoft ball's diameter as bought (6 mm nominal).
pub const AIRSOFT_DIAMETER: f64 = 5.95;
/// How much narrower than a ball a cage pocket's opening is.
pub const CAGE_SNAP: f64 = 0.2;
/// The fewest elements.
pub const MIN_ELEMENTS: f64 = 3.0;
/// Within how much a standard size fits a shaft or a hole (the seat's
/// fit selection, mitcad#578).
pub const FIT_CLEARANCE: f64 = 0.5;

/// How far the groove's cutter reaches into the free side of the face.
const GROOVE_OVERLAP: f64 = 0.5;

/// The rolling elements.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Element {
    /// A waisted roller touching each raceway on two flanks.
    #[default]
    Dumbbell,
    /// A cylinder with chamfered ends: line contact.
    Roller,
    /// A printed ball with conical caps.
    Ball,
    /// Bought airsoft balls in a snap-in cage.
    Airsoft,
}

impl Element {
    pub fn name(self) -> &'static str {
        match self {
            Self::Dumbbell => "dumbbell",
            Self::Roller => "roller",
            Self::Ball => "ball",
            Self::Airsoft => "airsoft",
        }
    }

    /// The elements in the plural, a word of its own in the messages
    /// (mitcad#1284: no plural ending put on a value).
    pub fn plural(self) -> &'static str {
        match self {
            Self::Dumbbell => "dumbbells",
            Self::Roller => "rollers",
            Self::Ball => "balls",
            Self::Airsoft => "airsofts",
        }
    }
}

fn none<P>() -> Option<P> {
    None
}

fn new_component() -> Operation {
    Operation::NewComponent
}

fn is_origin(center: &[f64; 2]) -> bool {
    *center == [0.0, 0.0]
}

/// A flange on the outer race at the face on the plane.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BearingFlange<P = ParamId> {
    pub diameter: P,
    pub thickness: P,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrintableBearingDef<P = ParamId> {
    #[serde(default)]
    pub element: Element,
    /// A standard size (`608`, `6202`, `MR105`, `R8`, `F608`): its bore,
    /// outside diameter and width (and an F size's flange).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<String>,
    /// Custom dimensions, without `size`.
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub bore: Option<P>,
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub outside_diameter: Option<P>,
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub width: Option<P>,
    /// A flange (an F size's own without it).
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub flange: Option<BearingFlange<P>>,
    /// The elements' largest diameter (an airsoft ball's diameter);
    /// without it the largest that leaves the races their walls.
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub element_diameter: Option<P>,
    /// How many elements; the most that fit without it.
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub element_count: Option<P>,
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub radial_clearance: Option<P>,
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub axial_clearance: Option<P>,
    /// The least gap between neighbouring elements.
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub element_gap: Option<P>,
    /// The races' 45-degree chamfers at the bore and the outside.
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub chamfer: Option<P>,
    /// The thinnest wall a race (and an element's waist, a cage) may have.
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub min_wall: Option<P>,
    /// The flanks' angle from the axis.
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub flank_angle: Option<P>,
    /// An origin plane, a construction plane or a planar face.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plane: Option<GeomRef>,
    /// The axis, in the plane's coordinates.
    #[serde(default, skip_serializing_if = "is_origin")]
    pub center: [f64; 2],
    /// Turns the elements about the axis (element 1 on the plane's x axis
    /// without it).
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub angle: Option<P>,
    /// A cylindrical face of a body to cut the raceway into.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub groove: Option<GeomRef>,
    /// With `groove`: from the face's low end along its axis.
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub offset: Option<P>,
    /// On a shaft or in a hole (mitcad#578, `seat.rs`): a circular edge or
    /// a cylindrical face, the bearing from the seat's offset along it.
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub seat: Option<Seat<P>>,
    /// With `seat` and no size: the standard size whose bore (on a shaft)
    /// or outside diameter (in a hole) is within this of the seat's
    /// diameter, [`FIT_CLEARANCE`] without it.
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub fit_clearance: Option<P>,
    /// `new_component` (the default) or `new_body`; `new_body` with
    /// `groove`.
    #[serde(default = "new_component")]
    pub operation: Operation,
    /// Placed by picked geometry instead of `plane` and `center`
    /// (mitcad#656, `place_at.rs`): on a shaft or in a hole as a seat.
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub at: Option<PlaceAt<P>>,
}

impl<P> PrintableBearingDef<P> {
    pub const TYPE: &'static str = "printable_bearing";
    pub const BASE_NAME: &'static str = "Bearing";

    // Written by hand: the optional values come first, nested parts under prefixes.
    pub fn map_params<Q, E>(
        &self,
        f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
    ) -> Result<PrintableBearingDef<Q>, E> {
        let mut option = |slot: &str, v: &Option<P>| v.as_ref().map(|v| f(slot, v)).transpose();
        let bore = option("bore", &self.bore)?;
        let outside_diameter = option("outside_diameter", &self.outside_diameter)?;
        let width = option("width", &self.width)?;
        let element_diameter = option("element_diameter", &self.element_diameter)?;
        let element_count = option("element_count", &self.element_count)?;
        let radial_clearance = option("radial_clearance", &self.radial_clearance)?;
        let axial_clearance = option("axial_clearance", &self.axial_clearance)?;
        let element_gap = option("element_gap", &self.element_gap)?;
        let chamfer = option("chamfer", &self.chamfer)?;
        let min_wall = option("min_wall", &self.min_wall)?;
        let flank_angle = option("flank_angle", &self.flank_angle)?;
        let angle = option("angle", &self.angle)?;
        let offset = option("offset", &self.offset)?;
        let fit_clearance = option("fit_clearance", &self.fit_clearance)?;
        let seat = match &self.seat {
            None => None,
            Some(seat) => Some(seat.map_params("seat.", &mut *f)?),
        };
        let flange = match &self.flange {
            None => None,
            Some(flange) => Some(BearingFlange {
                diameter: f("flange.diameter", &flange.diameter)?,
                thickness: f("flange.thickness", &flange.thickness)?,
            }),
        };
        Ok(PrintableBearingDef {
            element: self.element,
            size: self.size.clone(),
            bore,
            outside_diameter,
            width,
            flange,
            element_diameter,
            element_count,
            radial_clearance,
            axial_clearance,
            element_gap,
            chamfer,
            min_wall,
            flank_angle,
            plane: self.plane.clone(),
            center: self.center,
            angle,
            groove: self.groove.clone(),
            offset,
            seat,
            fit_clearance,
            operation: self.operation,
            at: map_at(&self.at, &mut *f)?,
        })
    }

    fn params_list(&self) -> Vec<P>
    where
        P: Clone,
    {
        let mut ids = Vec::new();
        let _ = self.map_params(&mut |_, id: &P| {
            ids.push(id.clone());
            Ok::<_, ()>(())
        });
        ids
    }
}

/// The definition with its values.
pub fn evaluated(
    def: &PrintableBearingDef,
    value: &dyn Fn(ParamId) -> f64,
) -> PrintableBearingDef<f64> {
    def.map_params(&mut |_, id: &ParamId| Ok::<_, ()>(value(*id)))
        .expect("no errors")
}

// ---------------------------------------------------------------------------
// The section

/// A piecewise-linear function of `z`: `rho = a + b z` lines combined.
#[derive(Debug, Clone, Copy)]
struct Line {
    a: f64,
    b: f64,
}

impl Line {
    fn at(self, z: f64) -> f64 {
        self.a + self.b * z
    }
}

/// The points of `f` on `0..=w`: its value at the ends and wherever two of
/// `lines` cross, without the points between collinear neighbours.
fn polyline(lines: &[Line], w: f64, f: &dyn Fn(f64) -> f64) -> Vec<[f64; 2]> {
    let mut zs = vec![0.0, w];
    for (i, p) in lines.iter().enumerate() {
        for q in &lines[i + 1..] {
            if (p.b - q.b).abs() > 1.0e-12 {
                let z = (q.a - p.a) / (p.b - q.b);
                if z > 1.0e-9 && z < w - 1.0e-9 {
                    zs.push(z);
                }
            }
        }
    }
    zs.sort_by(f64::total_cmp);
    zs.dedup_by(|a, b| (*a - *b).abs() < 1.0e-9);
    let points: Vec<[f64; 2]> = zs.iter().map(|z| [f(*z), *z]).collect();
    simplify(points)
}

/// Drops the points that lie on the line through their neighbours.
fn simplify(points: Vec<[f64; 2]>) -> Vec<[f64; 2]> {
    let mut out: Vec<[f64; 2]> = Vec::with_capacity(points.len());
    for p in points {
        if out
            .last()
            .is_some_and(|q| (q[0] - p[0]).hypot(q[1] - p[1]) < 1.0e-9)
        {
            continue;
        }
        if out.len() >= 2 {
            let a = out[out.len() - 2];
            let b = out[out.len() - 1];
            let turn = (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0]);
            if turn.abs() < 1.0e-12 {
                out.pop();
            }
        }
        out.push(p);
    }
    out
}

/// A piece of an element's section: a line to a point or an arc about
/// `(0, zc)` to a point (counter-clockwise).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Piece {
    Line([f64; 2]),
    Arc { to: [f64; 2], zc: f64, radius: f64 },
}

/// An airsoft bearing's cage: its section `rho <= half_width` about the
/// pitch circle from `bottom` to `top`, minus spherical pockets.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cage {
    pub half_width: f64,
    pub bottom: f64,
    pub top: f64,
    pub pocket: f64,
    /// The ring above the pockets.
    pub ring: f64,
}

/// Where the raceway goes when it is cut into a body's face.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GrooveFace {
    pub radius: f64,
    /// A bore (the body is the outer race) rather than a shaft.
    pub internal: bool,
}

/// Everything the bearing's bodies and checks need, mm and radians.
#[derive(Debug, Clone, PartialEq)]
pub struct Layout {
    pub element: Element,
    /// The standard size's designation.
    pub designation: Option<String>,
    /// The inner race's bore (None: the body's own, groove on a shaft).
    pub bore: Option<f64>,
    /// The outer race's outside (None: the body's, groove in a bore).
    pub outside_diameter: Option<f64>,
    pub width: f64,
    pub flange: Option<(f64, f64)>,
    pub element_diameter: f64,
    pub element_count: usize,
    pub max_elements: usize,
    pub pitch_radius: f64,
    pub radial_clearance: f64,
    pub axial_clearance: f64,
    pub element_gap: f64,
    pub chamfer: f64,
    pub min_wall: f64,
    pub flank_angle: f64,
    /// The dumbbell's waist depth, or the roller's end chamfer.
    pub relief: f64,
    /// `race(z)` from `z = 0` to the width: `(rho, z)`.
    pub race: Vec<[f64; 2]>,
    /// An element's section from its axis at `z = 0` round (none for an
    /// airsoft ball, a sphere).
    pub section: Vec<Piece>,
    /// The thinnest walls of the races made (None: the body's).
    pub inner_wall: Option<f64>,
    pub outer_wall: Option<f64>,
    /// The thinnest part of an element (its waist, its end, a printed
    /// ball's flat ends).
    pub element_wall: f64,
    /// The least normal gap between an element and a race.
    pub min_gap: f64,
    /// How far an element reaches past the raceway's lip or ridge.
    pub hold: f64,
    pub max_overhang: f64,
    pub cage: Option<Cage>,
    pub groove: Option<GrooveFace>,
}

impl Layout {
    /// The smallest and largest `race(z)`.
    pub fn race_range(&self) -> (f64, f64) {
        race_range(&self.race)
    }

    /// Where a ball's centre lies along the axis: the middle of the width.
    pub fn middle(&self) -> f64 {
        self.width / 2.0
    }

    /// The diameter where the dumbbell's waist is.
    pub fn waist_diameter(&self) -> Option<f64> {
        (self.element == Element::Dumbbell).then_some(self.element_diameter - 2.0 * self.relief)
    }
}

fn race_range(race: &[[f64; 2]]) -> (f64, f64) {
    race.iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| {
            (lo.min(p[0]), hi.max(p[0]))
        })
}

/// The race function of an element of diameter `d`, with the element's
/// section and its relief.
struct Shape {
    race: Vec<[f64; 2]>,
    section: Vec<Piece>,
    relief: f64,
    /// The thinnest part of the element.
    element_wall: f64,
    hold: f64,
    land: f64,
}

struct Params {
    w: f64,
    cr: f64,
    ca: f64,
    t: f64,
    alpha: f64,
}

fn shape(element: Element, d: f64, p: &Params) -> Shape {
    let (w, cr, ca, t, alpha) = (p.w, p.cr, p.ca, p.t, p.alpha);
    let r = d / 2.0;
    let zm = w / 2.0;
    match element {
        Element::Dumbbell => {
            let h = (d / 4.0).min(0.8 * t * zm);
            let band = Line { a: r + cr, b: 0.0 };
            let low = Line {
                a: r - h + t * ca + t * zm,
                b: -t,
            };
            let high = Line {
                a: r - h + t * ca - t * zm,
                b: t,
            };
            let race = polyline(&[band, low, high], w, &|z| {
                band.at(z).min(r - h + t * ca + t * (z - zm).abs())
            });
            let section = vec![
                Piece::Line([r, 0.0]),
                Piece::Line([r, zm - h / t]),
                Piece::Line([r - h, zm]),
                Piece::Line([r, zm + h / t]),
                Piece::Line([r, w]),
                Piece::Line([0.0, w]),
                Piece::Line([0.0, 0.0]),
            ];
            Shape {
                race,
                section,
                relief: h,
                element_wall: d - 2.0 * h,
                hold: h - t * ca,
                land: 0.0,
            }
        }
        Element::Roller => {
            let k = (d / 4.0).min(t * w / 4.0);
            let band = Line { a: r + cr, b: 0.0 };
            let low = Line {
                a: r - k + t * ca,
                b: t,
            };
            let high = Line {
                a: r - k + t * ca + t * w,
                b: -t,
            };
            let race = polyline(&[band, low, high], w, &|z| {
                band.at(z).min(low.at(z)).min(high.at(z))
            });
            let section = vec![
                Piece::Line([r - k, 0.0]),
                Piece::Line([r, k / t]),
                Piece::Line([r, w - k / t]),
                Piece::Line([r - k, w]),
                Piece::Line([0.0, w]),
                Piece::Line([0.0, 0.0]),
            ];
            Shape {
                race,
                section,
                relief: k,
                element_wall: d - 2.0 * k,
                hold: k - t * ca,
                land: 0.0,
            }
        }
        Element::Ball | Element::Airsoft => {
            let land = if element == Element::Airsoft {
                0.55 * r
            } else {
                cr
            };
            let flat = Line { a: r + cr, b: 0.0 };
            let peak = r / alpha.cos() + t * ca;
            let low = Line {
                a: peak - t * zm,
                b: t,
            };
            let high = Line {
                a: peak + t * zm,
                b: -t,
            };
            let floor = Line { a: land, b: 0.0 };
            let race = polyline(&[flat, low, high, floor], w, &|z| {
                flat.at(z).min(peak - t * (z - zm).abs()).max(land)
            });
            // The printed ball: its caps' flat ends at the faces.
            let end = r / alpha.cos() - t * zm;
            let (sin, cos) = alpha.sin_cos();
            let section = if element == Element::Airsoft {
                Vec::new()
            } else if zm <= r * sin {
                // The faces cut the sphere before the caps begin.
                let rho = (r * r - zm * zm).max(0.0).sqrt();
                vec![
                    Piece::Line([rho, 0.0]),
                    Piece::Arc {
                        to: [rho, w],
                        zc: zm,
                        radius: r,
                    },
                    Piece::Line([0.0, w]),
                    Piece::Line([0.0, 0.0]),
                ]
            } else {
                vec![
                    Piece::Line([end, 0.0]),
                    Piece::Line([r * cos, zm - r * sin]),
                    Piece::Arc {
                        to: [r * cos, zm + r * sin],
                        zc: zm,
                        radius: r,
                    },
                    Piece::Line([end, w]),
                    Piece::Line([0.0, w]),
                    Piece::Line([0.0, 0.0]),
                ]
            };
            let (lo, _) = race_range(&race);
            Shape {
                race,
                section,
                relief: 0.0,
                element_wall: if element == Element::Airsoft {
                    d
                } else {
                    2.0 * end.min(r)
                },
                hold: r - lo,
                land,
            }
        }
    }
}

impl PrintableBearingDef<f64> {
    /// The standard size, when there is one.
    pub fn standard(&self) -> Result<Option<&'static bearing_sizes::BearingSize>, String> {
        self.size.as_deref().map(bearing_sizes::size_of).transpose()
    }

    /// The bore, the outside diameter, the width and the flange.
    #[allow(clippy::type_complexity)]
    pub fn dimensions(
        &self,
    ) -> Result<(Option<f64>, Option<f64>, Option<f64>, Option<(f64, f64)>), String> {
        let flange = self.flange.as_ref().map(|f| (f.diameter, f.thickness));
        Ok(match self.standard()? {
            Some(size) => (
                Some(size.bore),
                Some(size.outside_diameter),
                Some(size.width),
                flange.or(size.flange),
            ),
            None => (self.bore, self.outside_diameter, self.width, flange),
        })
    }

    /// The name of the component a new-component bearing makes:
    /// `Bearing 608 (8x22x7)`, `Bearing 8x22x7` with custom dimensions.
    pub fn component_name(&self) -> Option<String> {
        let on_shaft = self.seat.is_some() || self.at.as_ref().is_some_and(PlaceAt::on_shaft);
        if on_shaft && self.size.is_none() && self.width.is_none() {
            // The size fits the seat, known once the shaft is.
            return Some("Bearing".to_owned());
        }
        let (bore, outside, width, _) = self.dimensions().ok()?;
        let (d, od, w) = (bore?, outside?, width?);
        if ![d, od, w].iter().all(|v| v.is_finite()) {
            return None;
        }
        let dims = format!(
            "{}x{}x{}",
            short_number(d),
            short_number(od),
            short_number(w)
        );
        Some(match self.standard().ok().flatten() {
            Some(size) => format!("Bearing {} ({dims})", size.designation),
            None => format!("Bearing {dims}"),
        })
    }

    /// The standard sizes that fit a seat: by their bore on a shaft, by
    /// their outside diameter in a hole, within the fit clearance; the
    /// nearest first.
    pub fn fitting_sizes(&self, seat: &SeatFrame) -> Vec<&'static bearing_sizes::BearingSize> {
        let fit = if seat.hole { Fit::Outside } else { Fit::Bore };
        bearing_sizes::fitting(
            fit,
            seat.diameter,
            self.fit_clearance.unwrap_or(FIT_CLEARANCE),
        )
    }

    /// On a seat without a size or custom dimensions: the fitting standard
    /// size (the nearest; the user may give another).
    pub fn fit_to(&mut self, seat: &SeatFrame) -> Result<(), String> {
        if self.size.is_some()
            || self.bore.is_some()
            || self.outside_diameter.is_some()
            || self.width.is_some()
        {
            return Ok(());
        }
        // An axis of its own (mitcad#656).
        if !seat.diameter.is_finite() {
            return Err(
                "an axis of its own has no diameter: give the bearing a size or its dimensions"
                    .to_owned(),
            );
        }
        match self.fitting_sizes(seat).first() {
            Some(size) => {
                self.size = Some(size.designation.to_owned());
                Ok(())
            }
            None => Err(format!(
                "no standard bearing's {} is within {} mm of the {} mm {}: choose a size or give custom dimensions",
                if seat.hole {
                    "outside diameter"
                } else {
                    "bore"
                },
                short_number(self.fit_clearance.unwrap_or(FIT_CLEARANCE)),
                short_number(seat.diameter),
                if seat.hole { "hole" } else { "shaft" }
            )),
        }
    }

    /// Checks the values that do not need the bearing's section.
    fn check_inputs(&self) -> Result<(), String> {
        let positive = |v: Option<f64>, what: &str| match v {
            Some(v) if v > 0.0 && v.is_finite() => Ok(()),
            Some(v) => Err(format!(
                "the bearing's {what} must be greater than zero, got {v}"
            )),
            None => Ok(()),
        };
        let finite = |v: Option<f64>, what: &str| match v {
            Some(v) if !v.is_finite() => Err(format!("the bearing's {what} must be a number")),
            _ => Ok(()),
        };
        if self.size.is_some()
            && (self.bore.is_some() || self.outside_diameter.is_some() || self.width.is_some())
        {
            return Err(
                "a standard size gives the bore, the outside diameter and the width: give either"
                    .to_owned(),
            );
        }
        self.standard()?;
        for (v, what) in [
            (self.bore, "bore"),
            (self.outside_diameter, "outside diameter"),
            (self.width, "width"),
            (self.element_diameter, "element diameter"),
            (self.min_wall, "smallest wall"),
            (self.flank_angle, "flank angle"),
        ] {
            positive(v, what)?;
        }
        for (v, what) in [
            (self.angle, "angle"),
            (self.offset, "offset"),
            (self.chamfer, "chamfer"),
            (self.radial_clearance, "radial clearance"),
            (self.axial_clearance, "axial clearance"),
            (self.element_gap, "element gap"),
        ] {
            finite(v, what)?;
        }
        if self.chamfer.is_some_and(|c| c < 0.0) {
            return Err("the bearing's chamfer must not be negative".to_owned());
        }
        if let Some(c) = self.fit_clearance
            && (!c.is_finite() || c < 0.0)
        {
            return Err(format!(
                "the bearing's fit clearance must not be negative, got {c}"
            ));
        }
        if let Some(seat) = &self.seat {
            seat.check_values()?;
        }
        if let Some(at) = &self.at {
            at.check_values()?;
        }
        for (v, what) in [
            (self.radial_clearance, "radial clearance"),
            (self.axial_clearance, "axial clearance"),
            (self.element_gap, "gap between the elements"),
        ] {
            if let Some(v) = v
                && v + 1.0e-9 < MIN_CLEARANCE
            {
                return Err(format!(
                    "the {what} {v} mm is below the {MIN_CLEARANCE} mm that a printer keeps open"
                ));
            }
        }
        if let Some(a) = self.flank_angle
            && !(10.0_f64.to_radians()..=80.0_f64.to_radians()).contains(&a)
        {
            return Err(format!(
                "the flank angle must be 10 to 80 degrees, got {:.4} degrees",
                a.to_degrees()
            ));
        }
        if let Some(n) = self.element_count
            && (!n.is_finite() || (n - n.round()).abs() > 1.0e-9 || n < MIN_ELEMENTS)
        {
            return Err(mitcad_model_base::internal::parameters::count_error(
                format!(
                    "the number of elements must be a whole number of at least {MIN_ELEMENTS}, \
                     got {n}"
                ),
                n,
            ));
        }
        if let Some(flange) = &self.flange {
            positive(Some(flange.diameter), "flange diameter")?;
            positive(Some(flange.thickness), "flange thickness")?;
        }
        Ok(())
    }

    /// The bearing's section, element count and checks; `groove` is the
    /// face the raceway is cut into (its radius from the kernel).
    pub fn layout(&self, groove: Option<GrooveFace>) -> Result<Layout, String> {
        self.check_inputs()?;
        let element = self.element;
        let (bore, outside, width, flange) = self.dimensions()?;
        let w = width.ok_or("the bearing needs a width, or a standard size")?;
        let (bore, outside) = match groove {
            None => (
                Some(bore.ok_or("the bearing needs a bore, or a standard size")?),
                Some(outside.ok_or("the bearing needs an outside diameter, or a standard size")?),
            ),
            // The body is the outer race: the inner race's bore is needed.
            Some(GrooveFace { internal: true, .. }) => (
                Some(bore.ok_or("a raceway cut into a bore needs the inner race's bore")?),
                None,
            ),
            Some(GrooveFace {
                internal: false, ..
            }) => {
                (
                    None,
                    Some(outside.ok_or(
                        "a raceway cut into a shaft needs the outer race's outside diameter",
                    )?),
                )
            }
        };
        let cr = self.radial_clearance.unwrap_or(DEFAULT_RADIAL_CLEARANCE);
        let ca = self.axial_clearance.unwrap_or(DEFAULT_AXIAL_CLEARANCE);
        let min_wall = self.min_wall.unwrap_or(DEFAULT_MIN_WALL);
        let gap = self.element_gap.unwrap_or(if element == Element::Airsoft {
            // Room for the cage's fingers between the balls.
            2.0 * cr + min_wall
        } else {
            DEFAULT_ELEMENT_GAP
        });
        let chamfer = self.chamfer.unwrap_or(DEFAULT_CHAMFER);
        let alpha = self.flank_angle.unwrap_or(DEFAULT_FLANK_ANGLE);
        let p = Params {
            w,
            cr,
            ca,
            t: alpha.tan(),
            alpha,
        };
        let rb = bore.map(|d| d / 2.0);
        let ro = outside.map(|d| d / 2.0);
        if let (Some(rb), Some(ro)) = (rb, ro)
            && rb >= ro
        {
            return Err("the bearing's outside diameter must be larger than its bore".to_owned());
        }
        if let (Some(rb), Some(g)) = (rb, groove)
            && g.internal
            && rb >= g.radius
        {
            return Err(
                "the inner race's bore must be smaller than the bore it runs in".to_owned(),
            );
        }
        if let (Some(ro), Some(g)) = (ro, groove)
            && !g.internal
            && ro <= g.radius
        {
            return Err(
                "the outer race's outside diameter must be larger than the shaft it runs on"
                    .to_owned(),
            );
        }
        // The pitch radius and the walls for an element of diameter d.
        let fit = |d: f64| -> (f64, Option<f64>, Option<f64>, Shape) {
            let s = shape(element, d, &p);
            let (lo, hi) = race_range(&s.race);
            let rp = match groove {
                None => (rb.unwrap_or(0.0) + ro.unwrap_or(0.0)) / 2.0,
                Some(g) if g.internal => g.radius - lo,
                Some(g) => g.radius + lo,
            };
            let inner = rb.map(|rb| rp - hi - rb);
            let outer = ro.map(|ro| ro - rp - hi);
            (rp, inner, outer, s)
        };
        let room = match (groove, rb, ro) {
            (None, Some(rb), Some(ro)) => ro - rb,
            (Some(g), Some(rb), _) => g.radius - rb,
            (Some(g), _, Some(ro)) => ro - g.radius,
            _ => 0.0,
        };
        let d = match (self.element_diameter, element) {
            (Some(d), _) => d,
            (None, Element::Airsoft) => AIRSOFT_DIAMETER,
            (None, _) => {
                // The largest element that leaves each race made its wall.
                let target = if groove.is_none() {
                    min_wall.max(room / 6.0)
                } else {
                    min_wall.max(room / 4.0)
                };
                let thinnest = |d: f64| {
                    let (_, inner, outer, _) = fit(d);
                    inner
                        .unwrap_or(f64::INFINITY)
                        .min(outer.unwrap_or(f64::INFINITY))
                };
                let (mut lo, mut hi) = (0.0, room.max(0.0));
                for _ in 0..80 {
                    let mid = (lo + hi) / 2.0;
                    if thinnest(mid) >= target {
                        lo = mid;
                    } else {
                        hi = mid;
                    }
                }
                lo
            }
        };
        let (rp, inner_wall, outer_wall, s) = fit(d);
        let r = d / 2.0;
        let what = match element {
            Element::Dumbbell => "dumbbells",
            Element::Roller => "rollers",
            Element::Ball => "balls",
            Element::Airsoft => "airsoft balls",
        };
        if d.is_nan() || d <= 0.0 || rp.is_nan() || rp <= r {
            return Err(format!(
                "no {what} fit the bearing's section ({} mm across)",
                short_number(room.max(0.0))
            ));
        }
        // The count: the most that fit round the pitch circle with the gap
        // between neighbours; airsoft balls also into the crescent between
        // the races with the inner race pushed off centre against the
        // outer one (the groove bottoms' gap 2 (r + cr) - 2 land cos(phi)
        // holds a ball where cos(phi) <= cr / land).
        let spacing = |across: f64| {
            let s = across / (2.0 * rp);
            if s >= 1.0 {
                f64::INFINITY
            } else {
                2.0 * s.asin()
            }
        };
        let around = (TAU / spacing(d + gap) + 1.0e-9).floor().max(0.0) as usize;
        let max_elements = if element == Element::Airsoft {
            let arc = 2.0 * (PI - (cr / s.land).min(1.0).acos());
            let crescent = (arc / spacing(d) + 1.0e-9).floor() as usize + 1;
            around.min(crescent)
        } else {
            around
        };
        let element_count = match self.element_count {
            Some(n) => n.round() as usize,
            None => max_elements,
        };
        let t = p.t;
        let min_gap = cr.min(t * ca / (1.0 + t * t).sqrt());
        // The steepest overhang printed with the axis vertical: a race's
        // face growing towards the pitch circle upwards, an element's
        // growing outwards upwards; the chamfers' 45 degrees.
        let mut overhang = if chamfer > 0.0 { FRAC_PI_4 } else { 0.0 };
        for pair in s.race.windows(2) {
            let (dr, dz) = (pair[1][0] - pair[0][0], pair[1][1] - pair[0][1]);
            if dr < -1.0e-12 && dz > 1.0e-12 {
                overhang = overhang.max((-dr / dz).atan());
            }
        }
        if element != Element::Airsoft {
            overhang = overhang.max(alpha);
        }
        let cage = (element == Element::Airsoft).then(|| {
            let pocket = r + cr;
            let below = (pocket * pocket - (r - CAGE_SNAP).powi(2)).max(0.0).sqrt();
            Cage {
                half_width: s.land - cr,
                bottom: w / 2.0 - below,
                top: w,
                pocket,
                ring: w - (w / 2.0 + pocket),
            }
        });
        let layout = Layout {
            element,
            designation: self.standard()?.map(|s| s.designation.to_owned()),
            bore,
            outside_diameter: outside,
            width: w,
            flange,
            element_diameter: d,
            element_count,
            max_elements,
            pitch_radius: rp,
            radial_clearance: cr,
            axial_clearance: ca,
            element_gap: gap,
            chamfer,
            min_wall,
            flank_angle: alpha,
            relief: s.relief,
            race: s.race,
            section: s.section,
            inner_wall,
            outer_wall,
            element_wall: s.element_wall,
            min_gap,
            hold: s.hold,
            max_overhang: overhang,
            cage,
            groove,
        };
        layout.check()?;
        Ok(layout)
    }
}

impl Layout {
    /// The printability checks: nothing is resized, a value that does not
    /// print is refused with what it should be.
    pub fn check(&self) -> Result<(), String> {
        let d = self.element_diameter;
        let w = self.width;
        let mm = short_number;
        if self.element_count > self.max_elements {
            return Err(format!(
                "too many elements for the pitch circle: at most {} of {} mm with a gap of {} mm, got {}",
                self.max_elements,
                mm(d),
                mm(self.element_gap),
                self.element_count
            ));
        }
        if (self.element_count as f64) < MIN_ELEMENTS {
            return Err(format!(
                "the pitch circle holds fewer than {MIN_ELEMENTS} elements of {} mm",
                mm(d)
            ));
        }
        if self.element == Element::Airsoft {
            let cage = self.cage.expect("an airsoft bearing's cage");
            let wall = self
                .inner_wall
                .unwrap_or(f64::INFINITY)
                .min(self.outer_wall.unwrap_or(f64::INFINITY));
            if wall + 1.0e-9 < self.min_wall {
                return Err(format!(
                    "the {} mm airsoft balls do not fit the section: they leave a race {} mm thick, below the smallest wall of {} mm",
                    mm(d),
                    mm(wall.max(0.0)),
                    mm(self.min_wall)
                ));
            }
            if cage.ring + 1.0e-9 < self.min_wall {
                return Err(format!(
                    "the {} mm airsoft balls do not fit the section: their cage's ring above them is {} mm thick in a {} mm wide bearing, below the smallest wall of {} mm (a width of {} mm at least)",
                    mm(d),
                    mm(cage.ring.max(0.0)),
                    mm(w),
                    mm(self.min_wall),
                    mm(w + self.min_wall - cage.ring)
                ));
            }
            if 2.0 * cage.half_width + 1.0e-9 < self.min_wall {
                return Err(format!(
                    "the {} mm airsoft balls do not fit the section: their cage is {} mm across, below the smallest wall of {} mm",
                    mm(d),
                    mm((2.0 * cage.half_width).max(0.0)),
                    mm(self.min_wall)
                ));
            }
            let finger = self.element_gap - 2.0 * self.radial_clearance;
            if finger + 1.0e-9 < self.min_wall {
                return Err(format!(
                    "the cage's fingers between the airsoft balls are {} mm thick, below the smallest wall of {} mm: a gap between the balls of {} mm at least",
                    mm(finger.max(0.0)),
                    mm(self.min_wall),
                    mm(self.min_wall + 2.0 * self.radial_clearance)
                ));
            }
        }
        if self.element == Element::Ball {
            if d + 1.0e-9 < MIN_PRINTED_BALL {
                return Err(format!(
                    "a printed ball of {} mm is too small to print: below {MIN_PRINTED_BALL} mm balls stick or rattle",
                    mm(d)
                ));
            }
            let reach = w * self.flank_angle.sin();
            if d + 1.0e-9 < reach {
                return Err(format!(
                    "a printed ball of {} mm does not reach the print bed in a {} mm wide bearing: {} mm at least",
                    mm(d),
                    mm(w),
                    mm(reach)
                ));
            }
        }
        for (wall, what) in [
            (self.inner_wall, "inner race's"),
            (self.outer_wall, "outer race's"),
        ] {
            if let Some(wall) = wall
                && wall + 1.0e-9 < self.min_wall
            {
                return Err(format!(
                    "the {what} wall is {} mm, below the smallest wall of {} mm",
                    mm(wall.max(0.0)),
                    mm(self.min_wall)
                ));
            }
        }
        if self.element_wall + 1.0e-9 < self.min_wall {
            return Err(format!(
                "the {} are {} mm across at their thinnest, below the smallest wall of {} mm",
                self.element.plural(),
                mm(self.element_wall.max(0.0)),
                mm(self.min_wall)
            ));
        }
        if self.hold + 1.0e-9 < MIN_HOLD {
            return Err(format!(
                "the {} are not held: they reach {} mm past the raceway's {}, {MIN_HOLD} mm at least (a smaller axial clearance or a larger element)",
                self.element.plural(),
                mm(self.hold),
                match self.element {
                    Element::Dumbbell => "ridge",
                    Element::Roller => "lips",
                    _ => "lands",
                }
            ));
        }
        if self.max_overhang > MAX_OVERHANG + 1.0e-9 {
            return Err(format!(
                "an overhang of {} degrees, steeper than {} degrees, when printed with the axis vertical: a flank angle of 45 degrees at most",
                mm(self.max_overhang.to_degrees()),
                mm(MAX_OVERHANG.to_degrees())
            ));
        }
        if self.chamfer > 0.0 {
            let thinnest = self
                .inner_wall
                .unwrap_or(f64::INFINITY)
                .min(self.outer_wall.unwrap_or(f64::INFINITY))
                .min(w / 2.0);
            if self.chamfer >= thinnest {
                return Err(format!(
                    "the chamfer of {} mm must be smaller than the races' walls and half the width ({} mm)",
                    mm(self.chamfer),
                    mm(thinnest)
                ));
            }
        }
        // A flange on the outer race made (none on a body's own race).
        if let (Some((fd, ft)), Some(od)) = (self.flange, self.outside_diameter) {
            if fd.is_nan() || fd <= od {
                return Err(format!(
                    "the flange must be larger than the outside diameter ({} mm), got {}",
                    mm(od),
                    mm(fd)
                ));
            }
            if ft >= w {
                return Err(format!(
                    "the flange must be thinner than the bearing's width ({} mm), got {}",
                    mm(w),
                    mm(ft)
                ));
            }
        }
        Ok(())
    }

    /// The purchased parts: airsoft balls per bearing.
    pub fn purchased(&self) -> Option<(String, usize)> {
        (self.element == Element::Airsoft).then(|| {
            (
                airsoft_designation(self.element_diameter),
                self.element_count,
            )
        })
    }
}

/// The parts list's name of airsoft balls of a diameter: `Airsoft ball 6
/// mm` (the nominal size, the bought diameter rounded).
pub fn airsoft_designation(diameter: f64) -> String {
    format!("Airsoft ball {} mm", short_number(diameter.round()))
}

// ---------------------------------------------------------------------------
// Placement

/// Where the bearing lies: its frame (the origin on the axis at the face on
/// the plane, x along element 1's direction, the axis as the normal).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    pub frame: SketchFrame,
}

impl Placement {
    pub fn axis(&self) -> [f64; 3] {
        self.frame.normal()
    }
}

/// An x axis across `z`: z x X, or z x Y for z near X.
fn across(z: [f64; 3]) -> [f64; 3] {
    let other = if z[0].abs() < 0.9 {
        [1.0, 0.0, 0.0]
    } else {
        [0.0, 1.0, 0.0]
    };
    unit_div(cross(z, other)).unwrap_or([1.0, 0.0, 0.0])
}

/// The values, the placement and the groove's body, face and shape.
#[allow(clippy::type_complexity)]
fn placement<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    def: &PrintableBearingDef,
) -> Result<
    (
        PrintableBearingDef<f64>,
        Layout,
        Placement,
        Option<(BodyUid, K::Shape)>,
    ),
    String,
> {
    let mut values = Vec::new();
    for id in def.params_list() {
        values.push((id, ctx.param(id)?));
    }
    let value = |id: ParamId| {
        values
            .iter()
            .find(|(p, _)| *p == id)
            .map_or(f64::NAN, |(_, v)| *v)
    };
    let mut bearing = evaluated(def, &value);
    // Placed by picked geometry (mitcad#656): on a shaft or in a hole the
    // fitting size as a seat's; elsewhere the size given.
    if let Some(at) = bearing.at.clone() {
        let placed = ctx.place(&at)?;
        if let Some(seat) = placed.round() {
            bearing.fit_to(seat)?;
        }
        let layout = bearing.layout(None)?;
        let frame = turned(placed.frame, bearing.angle);
        return Ok((bearing, layout, Placement { frame }, None));
    }
    // On a shaft or in a hole (mitcad#578): the fitting size unless one is
    // given, the plane through the seat's origin normal to its axis.
    if let Some(seat) = bearing.seat.clone() {
        let seat = ctx.seat(&seat).map_err(|e| format!("seat: {e}"))?;
        bearing.fit_to(&seat)?;
        let layout = bearing.layout(None)?;
        let frame = turned(seat.sketch_frame(), bearing.angle);
        return Ok((bearing, layout, Placement { frame }, None));
    }
    if let Some(groove) = &def.groove {
        let GeomRef::Face { body, face } = groove else {
            return Err("the raceway is cut into a face of a body".to_owned());
        };
        let shape = ctx.surface_body(*body)?;
        let face = ctx.face_on(*body, &shape, face)?;
        let cylinder = ctx.kernel.face_cylinder(&shape, &face).map_err(|_| {
            "the raceway is cut into a cylindrical face: a bore or a shaft".to_owned()
        })?;
        let layout = bearing.layout(Some(GrooveFace {
            radius: cylinder.radius,
            internal: cylinder.internal,
        }))?;
        let n = unit_div(cylinder.axis.direction).ok_or("the face's axis has no direction")?;
        let start = bearing
            .offset
            .unwrap_or((cylinder.length - layout.width) / 2.0);
        let x = across(n);
        let frame = SketchFrame {
            origin: add_scaled(cylinder.axis.origin, n, start),
            x_axis: x,
            y_axis: cross(n, x),
        };
        return Ok((bearing, layout, Placement { frame }, Some((*body, shape))));
    }
    let plane = def
        .plane
        .as_ref()
        .ok_or("a bearing needs a plane, a shaft or a hole, or a face to cut its raceway into")?;
    let layout = bearing.layout(None)?;
    let plane = ctx.plane(plane)?.frame();
    let frame = turned(
        SketchFrame {
            origin: plane.point(bearing.center),
            ..plane
        },
        bearing.angle,
    );
    Ok((bearing, layout, Placement { frame }, None))
}

/// A frame turned about its normal by `angle`.
fn turned(frame: SketchFrame, angle: Option<f64>) -> SketchFrame {
    let (s, c) = angle.unwrap_or(0.0).sin_cos();
    SketchFrame {
        origin: frame.origin,
        x_axis: combine(frame.x_axis, c, frame.y_axis, s),
        y_axis: combine(frame.y_axis, c, frame.x_axis, -s),
    }
}

// ---------------------------------------------------------------------------
// Bodies

fn curve(n: u32) -> EntityUid {
    EntityUid(n)
}

/// A closed loop of `pieces` from `start`, curves `c<first>`, … in order.
fn loop_of(first: u32, start: [f64; 2], pieces: &[Piece]) -> ProfileLoop {
    let n = pieces.len() as u32;
    let id = |i: u32| curve(first + (i % n));
    let mut at = start;
    let segments = pieces
        .iter()
        .enumerate()
        .map(|(i, piece)| {
            let i = i as u32;
            let geometry = match *piece {
                Piece::Line(to) => {
                    let line = SegmentGeometry::Line { start: at, end: to };
                    at = to;
                    line
                }
                Piece::Arc { to, zc, radius } => {
                    let arc = SegmentGeometry::Arc {
                        center: [0.0, zc],
                        radius,
                        start_angle: (at[1] - zc).atan2(at[0]),
                        end_angle: (to[1] - zc).atan2(to[0]),
                    };
                    at = to;
                    arc
                }
            };
            ProfileSegment {
                key: SegmentKey::between(id(i), id(i + n - 1), id(i + 1)),
                geometry,
            }
        })
        .collect();
    ProfileLoop { segments }
}

/// A polygon `c<first>`, … from point i to i + 1 (repeated points left
/// out).
fn polygon(first: u32, points: &[[f64; 2]]) -> ProfileRegion {
    let mut clean: Vec<[f64; 2]> = Vec::new();
    for p in points {
        if clean
            .last()
            .is_none_or(|q| (q[0] - p[0]).hypot(q[1] - p[1]) > 1.0e-9)
        {
            clean.push(*p);
        }
    }
    if clean.len() > 1 {
        let (a, b) = (clean[0], clean[clean.len() - 1]);
        if (a[0] - b[0]).hypot(a[1] - b[1]) <= 1.0e-9 {
            clean.pop();
        }
    }
    let start = clean[0];
    let pieces: Vec<Piece> = clean[1..]
        .iter()
        .chain(std::iter::once(&start))
        .map(|p| Piece::Line(*p))
        .collect();
    region(loop_of(first, start, &pieces))
}

fn region(outer: ProfileLoop) -> ProfileRegion {
    let key =
        RegionKey::new(outer.segments.iter().map(|s| s.key.clone())).expect("a loop has segments");
    ProfileRegion {
        key,
        loops: vec![outer],
    }
}

/// The inner race's section in `(R, z)`: from the bore's lower chamfer
/// round to the raceway and back over the top.
pub fn inner_race_section(layout: &Layout, rb: f64) -> Vec<[f64; 2]> {
    let (w, ch, rp) = (layout.width, layout.chamfer, layout.pitch_radius);
    let mut points = vec![[rb, ch], [rb + ch, 0.0]];
    points.extend(layout.race.iter().map(|p| [rp - p[0], p[1]]));
    points.extend([[rb + ch, w], [rb, w - ch]]);
    points
}

/// The outer race's section in `(R, z)`: from the raceway's foot out
/// along the face on the plane (and the flange) round and back down the
/// raceway.
pub fn outer_race_section(layout: &Layout, ro: f64) -> Vec<[f64; 2]> {
    let (w, ch, rp) = (layout.width, layout.chamfer, layout.pitch_radius);
    let mut points = vec![[rp + layout.race[0][0], 0.0]];
    match layout.flange {
        Some((fd, ft)) => points.extend([[fd / 2.0, 0.0], [fd / 2.0, ft], [ro, ft]]),
        None => points.extend([[ro - ch, 0.0], [ro, ch]]),
    }
    points.extend([[ro, w - ch], [ro - ch, w]]);
    points.extend(layout.race.iter().rev().map(|p| [rp + p[0], p[1]]));
    points
}

/// The cutter of a raceway in a body's face of `radius`.
///
/// The body may go on past the bearing's faces, where its face lies
/// within the elements' reach: the cutter runs on `axial_clearance` past
/// each face at the raceway's radius there, so that the elements' ends
/// keep that gap from the groove's end walls instead of touching them.
pub fn groove_section(layout: &Layout, groove: GrooveFace) -> Vec<[f64; 2]> {
    let (w, rp, ca) = (layout.width, layout.pitch_radius, layout.axial_clearance);
    let first = layout.race[0][0];
    let last = layout.race[layout.race.len() - 1][0];
    // The raceway from below the first face to above the far one.
    let mut race = vec![[first, -ca]];
    race.extend(layout.race.iter().copied());
    race.push([last, w + ca]);
    let race = simplify(race);
    if groove.internal {
        let free = groove.radius - GROOVE_OVERLAP;
        let mut points = vec![[free, -ca]];
        points.extend(race.iter().map(|p| [rp + p[0], p[1]]));
        points.push([free, w + ca]);
        points
    } else {
        let free = groove.radius + GROOVE_OVERLAP;
        let mut points: Vec<[f64; 2]> = race.iter().map(|p| [rp - p[0], p[1]]).collect();
        points.extend([[free, w + ca], [free, -ca]]);
        // Counter-clockwise: up the free side, down the raceway.
        points.reverse();
        points
    }
}

fn revolve<K: Kernel>(
    ctx: &EvalContext<'_, K>,
    frame: SketchFrame,
    region: ProfileRegion,
    axis: Axis,
) -> Result<K::Shape, String> {
    ctx.kernel
        .revolve(&RevolveSpec {
            feature: ctx.uid,
            frame,
            regions: &[region],
            axis,
            angle1: TAU,
            angle2: None,
            target: None,
        })
        .map_err(|e| e.to_string())
}

fn one_piece<K: Kernel>(
    ctx: &EvalContext<'_, K>,
    target: &K::Shape,
    tool: &K::Shape,
    what: &str,
) -> Result<K::Shape, String> {
    let output = ctx
        .kernel
        .boolean(BooleanOp::Cut, &[target], tool)
        .map_err(|e| e.to_string())?;
    let mut pieces = output.pieces.into_iter();
    match (pieces.next(), pieces.next()) {
        (Some(piece), None) => Ok(piece.shape),
        (None, _) => Err(format!("the {what} removes the whole body")),
        (Some(_), Some(_)) => Err(format!("the {what} cuts the body apart")),
    }
}

/// The bodies, in order: the races made (inner, outer), the elements, the
/// cage; and the groove's body cut.
struct Bodies<S> {
    races: Vec<S>,
    elements: Vec<S>,
    cage: Option<S>,
    grooved: Option<(BodyUid, S)>,
}

fn bodies<K: Kernel>(
    ctx: &EvalContext<'_, K>,
    layout: &Layout,
    at: Placement,
    grooved: Option<(BodyUid, K::Shape)>,
) -> Result<Bodies<K::Shape>, String> {
    let n = at.axis();
    let o = at.frame.origin;
    let u = at.frame.x_axis;
    let v = cross(n, u);
    let axis = Axis {
        origin: o,
        direction: n,
    };
    let section = SketchFrame {
        origin: o,
        x_axis: u,
        y_axis: n,
    };
    let mut races = Vec::new();
    if let Some(d) = layout.bore {
        races.push(revolve(
            ctx,
            section,
            polygon(1, &inner_race_section(layout, d / 2.0)),
            axis,
        )?);
    }
    if let Some(d) = layout.outside_diameter {
        races.push(revolve(
            ctx,
            section,
            polygon(101, &outer_race_section(layout, d / 2.0)),
            axis,
        )?);
    }
    let rp = layout.pitch_radius;
    let first = match layout.element {
        Element::Airsoft => {
            let centre = add_scaled(add_scaled(o, u, rp), n, layout.middle());
            ctx.kernel
                .primitive(&PrimitiveSpec {
                    feature: ctx.uid,
                    frame: SketchFrame {
                        origin: centre,
                        x_axis: u,
                        y_axis: v,
                    },
                    shape: PrimitiveShape::Sphere {
                        radius: layout.element_diameter / 2.0,
                    },
                })
                .map_err(|e| e.to_string())?
        }
        _ => {
            let centre = add_scaled(o, u, rp);
            let start = layout
                .section
                .last()
                .map(|p| match *p {
                    Piece::Line(to) | Piece::Arc { to, .. } => to,
                })
                .unwrap_or([0.0, 0.0]);
            revolve(
                ctx,
                SketchFrame {
                    origin: centre,
                    x_axis: u,
                    y_axis: n,
                },
                region(loop_of(201, start, &layout.section)),
                Axis {
                    origin: centre,
                    direction: n,
                },
            )?
        }
    };
    let count = layout.element_count;
    let mut elements = vec![first.clone()];
    for k in 1..count {
        let turn = Transform::rotation(o, n, TAU * k as f64 / count as f64)
            .ok_or("the bearing's axis has no direction")?;
        elements.push(
            ctx.kernel
                .transform_shape(
                    &first,
                    &turn,
                    Some(Instance {
                        feature: ctx.uid,
                        index: k as u32,
                    }),
                )
                .map_err(|e| e.to_string())?,
        );
    }
    let cage = match layout.cage {
        None => None,
        Some(cage) => {
            let ring = revolve(
                ctx,
                section,
                polygon(
                    301,
                    &[
                        [rp - cage.half_width, cage.bottom],
                        [rp + cage.half_width, cage.bottom],
                        [rp + cage.half_width, cage.top],
                        [rp - cage.half_width, cage.top],
                    ],
                ),
                axis,
            )?;
            // The pockets round the balls, the copies named as the balls'.
            let pocket = ctx
                .kernel
                .primitive(&PrimitiveSpec {
                    feature: ctx.uid,
                    frame: SketchFrame {
                        origin: add_scaled(add_scaled(o, u, rp), n, layout.middle()),
                        x_axis: u,
                        y_axis: v,
                    },
                    shape: PrimitiveShape::Sphere {
                        radius: cage.pocket,
                    },
                })
                .map_err(|e| e.to_string())?;
            let mut pockets = vec![pocket.clone()];
            for k in 1..count {
                let turn = Transform::rotation(o, n, TAU * k as f64 / count as f64)
                    .ok_or("the bearing's axis has no direction")?;
                pockets.push(
                    ctx.kernel
                        .transform_shape(
                            &pocket,
                            &turn,
                            Some(Instance {
                                feature: ctx.uid,
                                index: k as u32,
                            }),
                        )
                        .map_err(|e| e.to_string())?,
                );
            }
            let refs: Vec<&K::Shape> = pockets.iter().collect();
            let tool = ctx.kernel.unite(&refs).map_err(|e| e.to_string())?;
            Some(one_piece(ctx, &ring, &tool, "cage's pockets")?)
        }
    };
    let grooved = match (grooved, layout.groove) {
        (Some((body, shape)), Some(face)) => {
            let tool = revolve(
                ctx,
                section,
                polygon(401, &groove_section(layout, face)),
                axis,
            )?;
            Some((body, one_piece(ctx, &shape, &tool, "raceway")?))
        }
        _ => None,
    };
    Ok(Bodies {
        races,
        elements,
        cage,
        grooved,
    })
}

impl FeatureInfo for PrintableBearingDef {
    fn references(&self) -> References {
        let mut references = References::default();
        if let Some(plane) = &self.plane {
            plane.add_to(&mut references);
        }
        if let Some(groove) = &self.groove {
            groove.add_to(&mut references);
        }
        if let Some(seat) = &self.seat {
            seat.add_to(&mut references);
        }
        if let Some(at) = &self.at {
            at.add_to(&mut references);
        }
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        if !self.operation.makes_bodies() {
            return Err(
                "a bearing makes bodies: its operation is new_component or new_body".to_owned(),
            );
        }
        // Placed by picked geometry (mitcad#656).
        if let Some(at) = &self.at {
            if self.plane.is_some() || !is_origin(&self.center) || self.seat.is_some() {
                return Err(
                    "a bearing placed by at takes no plane, center or seat of its own".to_owned(),
                );
            }
            if self.groove.is_some() || self.offset.is_some() {
                return Err(
                    "a bearing placed by at cuts no raceway into a face (the placement has its own offset)"
                        .to_owned(),
                );
            }
            return at.check(ctx);
        }
        // On a shaft or in a hole (mitcad#578).
        if let Some(seat) = &self.seat {
            if self.plane.is_some() || !is_origin(&self.center) {
                return Err(
                    "a bearing on a shaft or in a hole takes its plane and center from it"
                        .to_owned(),
                );
            }
            if self.groove.is_some() || self.offset.is_some() {
                return Err(
                    "a bearing on a shaft or in a hole cuts no raceway into a face (the seat has its own offset)"
                        .to_owned(),
                );
            }
            return seat.check(ctx);
        }
        if self.fit_clearance.is_some() {
            return Err("a fit clearance needs a seat on a shaft or in a hole".to_owned());
        }
        match (&self.plane, &self.groove) {
            (None, None) => Err(
                "a bearing needs a plane, a shaft or a hole, or a face to cut its raceway into"
                    .to_owned(),
            ),
            (Some(_), Some(_)) => Err(
                "a bearing cut into a face takes its place from the face, not a plane".to_owned(),
            ),
            (Some(plane), None) => {
                if self.offset.is_some() {
                    return Err("a bearing on a plane takes no offset".to_owned());
                }
                plane.check(ctx, Want::Plane)
            }
            (None, Some(groove)) => {
                if self.operation != Operation::NewBody {
                    return Err(
                        "a raceway cut into a body stays in the body's component: new_body"
                            .to_owned(),
                    );
                }
                if !matches!(groove, GeomRef::Face { .. }) {
                    return Err("the raceway is cut into a face of a body".to_owned());
                }
                if !is_origin(&self.center) || self.angle.is_some() {
                    return Err("a bearing cut into a face takes no center and no angle".to_owned());
                }
                groove.check(ctx, Want::Face)
            }
        }
    }

    fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
        let bearing = evaluated(self, value);
        if self.groove.is_some() || self.seat.is_some() || self.at.is_some() {
            // The section needs the face's radius or the seat's size:
            // checked when evaluated.
            bearing.check_inputs()
        } else {
            bearing.layout(None).map(|_| ())
        }
    }

    fn creates_bodies(&self) -> bool {
        true
    }

    fn new_component(&self) -> bool {
        self.operation == Operation::NewComponent
    }

    fn component_name(&self, value: &dyn Fn(ParamId) -> f64) -> Option<String> {
        evaluated(self, value).component_name()
    }
}

impl<K: Kernel> Evaluate<K> for PrintableBearingDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let (_, layout, at, grooved) = placement(ctx, self)?;
        let made = bodies(ctx, &layout, at, grooved)?;
        let mut output = FeatureOutput::default();
        let mut all = Vec::new();
        if let Some((body, shape)) = made.grooved {
            all.push(shape.clone());
            output.changes.push(BodyChange::Set(body, shape));
        }
        for shape in made.races.into_iter().chain(made.elements).chain(made.cage) {
            all.push(shape.clone());
            output.changes.push(BodyChange::Set(ctx.new_body(), shape));
        }
        output.tool = ctx.kernel.compound(&all).ok();
        for (name, frame) in mount_frames(&at, layout.width) {
            output
                .mounts
                .insert(name.to_owned(), frame_transform(&frame));
        }
        Ok(output)
    }
}

// ---------------------------------------------------------------------------
// Mount frames and derived values

/// The mount frames a bearing's result keeps: `bore` on the axis at the
/// face on the plane (the axis its z), `face_a` that face (z out of the
/// bearing), `face_b` the far face (z out of it).
pub const MOUNTS: [&str; 3] = ["bore", "face_a", "face_b"];

fn mount_frames(at: &Placement, width: f64) -> [(&'static str, SketchFrame); 3] {
    let f = at.frame;
    let n = f.normal();
    [
        (MOUNTS[0], f),
        (
            MOUNTS[1],
            SketchFrame {
                origin: f.origin,
                x_axis: f.x_axis,
                y_axis: f.y_axis.map(|c| -c),
            },
        ),
        (
            MOUNTS[2],
            SketchFrame {
                origin: add_scaled(f.origin, n, width),
                ..f
            },
        ),
    ]
}

fn frame_transform(frame: &SketchFrame) -> Transform {
    let n = frame.normal();
    Transform {
        linear: std::array::from_fn(|r| [frame.x_axis[r], frame.y_axis[r], n[r]]),
        translation: frame.origin,
    }
}

/// What the panel and the `bearing_values` query show of a bearing: mm and
/// radians, `null` where a value does not apply.
pub fn derived_values(layout: &Layout) -> serde_json::Value {
    use serde_json::json;
    let (lo, hi) = layout.race_range();
    let mut values = json!({
        "element": layout.element,
        "designation": layout.designation,
        "bore": layout.bore,
        "outside_diameter": layout.outside_diameter,
        "width": layout.width,
        "element_diameter": layout.element_diameter,
        "element_count": layout.element_count,
        "max_elements": layout.max_elements,
        "pitch_diameter": 2.0 * layout.pitch_radius,
        "radial_clearance": layout.radial_clearance,
        "axial_clearance": layout.axial_clearance,
        "element_gap": layout.element_gap,
        "chamfer": layout.chamfer,
        "min_wall": layout.min_wall,
        "flank_angle": layout.flank_angle,
        "min_gap": layout.min_gap,
        "inner_wall": layout.inner_wall,
        "outer_wall": layout.outer_wall,
        "element_wall": layout.element_wall,
        "hold": layout.hold,
        "max_overhang": layout.max_overhang,
        "raceway_depth": hi - lo,
    });
    let map = values.as_object_mut().expect("an object");
    if let Some(waist) = layout.waist_diameter() {
        map.insert("waist_diameter".into(), json!(waist));
    }
    if let Some((d, t)) = layout.flange {
        map.insert("flange".into(), json!({"diameter": d, "thickness": t}));
    }
    if let Some(cage) = layout.cage {
        map.insert(
            "cage".into(),
            json!({"ring": cage.ring, "width": 2.0 * cage.half_width,
                   "pocket_diameter": 2.0 * cage.pocket}),
        );
    }
    if let Some(groove) = layout.groove {
        map.insert(
            "groove".into(),
            json!({"diameter": 2.0 * groove.radius,
                   "race": if groove.internal { "outer" } else { "inner" }}),
        );
    }
    if let Some((designation, quantity)) = layout.purchased() {
        map.insert(
            "purchased".into(),
            json!([{"designation": designation, "quantity": quantity}]),
        );
    }
    values
}
