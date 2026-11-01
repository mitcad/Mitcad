// SPDX-License-Identifier: MIT
//! Pulley (mitcad#562): timing belt pulleys (GT2 2 mm and 3 mm, HTD 3M and
//! 5M, T5, XL; `pulley_profile.rs` works out the grooves), smooth idlers
//! and toothed idlers, made from parameters on a plane as a gear is: the
//! pulley's axis is the plane's normal through `center`, and the pulley
//! runs along the normal from the plane. `angle` turns it about its axis
//! (tooth 1 is centred on the plane's x axis without it; the keyway and the
//! first set screw lie in that direction).
//!
//! Along the axis from the plane: the first flange (`flanges.sides` `one`
//! or `both`), the toothed part `belt_width + clearance` wide, the second
//! flange (`both`), then the hub. So a single flange lies on the side away
//! from the hub. A bore (plain with an optional DIN 6885 keyway, or a
//! thread size's tap drill) goes through all of it; set screws are radial
//! tap drills through the hub.
//!
//! `belt_with` places the pulley instead: in the plane of an earlier
//! pulley feature of the same component, at `center_distance` from its
//! centre in the direction `belt_with.angle`, for one belt round both.
//!
//! `seat` places it on a shaft instead of `plane` and `center` (mitcad#578,
//! `seat.rs`): its plane at the seat's offset along the shaft, its axis
//! the shaft's; without a `bore` its bore is plain, the shaft's diameter
//! plus `bore_clearance`, and follows the shaft.
//!
//! `at` places it by picked geometry (mitcad#656, `place_at.rs`): a shaft
//! or an axis, a point, or a point on a face; on a shaft its bore follows
//! the shaft as a seat's does, elsewhere it keeps its own.

use std::f64::consts::{FRAC_PI_2, PI, TAU};

use serde::{Deserialize, Serialize};

use mitcad_vec3::{add_scaled, combine};

use super::extrude::{Operation, apply_operation, check_participants, participant_bodies};
use super::gear::{Bore, Hub};
use super::gear_profile::{self, NamedSegment, bore_outline, circle};
use super::geom_ref::{GeomRef, Want};
use super::place_at::{PlaceAt, map_at};
use super::pulley_profile::{self, BeltProfile};
use super::seat::Seat;
use super::thread::ThreadSize;
use super::{
    CheckContext, EvalContext, Evaluate, FeatureDef, FeatureInfo, FeatureOutput, References,
    ToolUse, is_false,
};
use crate::ids::{BodyUid, FeatureUid};
use crate::kernel::{BooleanOp, GearStep, GearSweepSpec, Kernel};
use crate::parameters::ParamId;
use crate::profile::SketchFrame;
use crate::recompute::Read;
use mitcad_vec3::cross;

/// The clearance added to the belt's width without one (mm).
pub const DEFAULT_CLEARANCE: f64 = 1.0;

/// The least wall between a bore (with its keyway) and the groove
/// bottoms (an idler's running face), mm.
pub const MIN_WALL: f64 = 0.5;

/// How far a worked-out flange stands above the groove bottoms' depth
/// beyond the outside circle, mm (mitcad#656): Mitcad's default.
pub const FLANGE_MARGIN: f64 = 1.0;

/// The most set screws, 90 degrees apart.
pub const MAX_SET_SCREWS: f64 = 4.0;

/// What kind of pulley.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PulleyKind {
    /// A timing belt pulley.
    Toothed,
    /// A smooth idler of a running diameter.
    Idler,
    /// Teeth on a bearing bore.
    ToothedIdler,
}

impl PulleyKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Toothed => "pulley",
            Self::Idler => "idler",
            Self::ToothedIdler => "toothed idler",
        }
    }

    pub fn toothed(self) -> bool {
        matches!(self, Self::Toothed | Self::ToothedIdler)
    }
}

/// Which flanges a pulley has.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FlangeSides {
    None,
    /// On the plane's side, away from the hub.
    One,
    #[default]
    Both,
}

fn none<P>() -> Option<P> {
    None
}

/// The flanges on the sides of the toothed part.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Flanges<P = ParamId> {
    #[serde(default)]
    pub sides: FlangeSides,
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub diameter: Option<P>,
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub thickness: Option<P>,
}

/// Radial set screws through the hub: the tap drills of a thread size.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetScrews<P = ParamId> {
    /// 1 to 4, 90 degrees apart from the pulley's `angle`.
    pub count: P,
    pub thread: ThreadSize,
    /// From the hub's outer end along the axis; the hub's middle without
    /// it.
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub position: Option<P>,
}

/// A belt kept checked round a pulley and an earlier pulley feature of
/// its component (mitcad#656): a relation that places nothing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BeltCheck {
    pub pulley: FeatureUid,
}

/// Where a pulley runs on one belt with an earlier pulley feature.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BeltWith<P = ParamId> {
    pub pulley: FeatureUid,
    pub center_distance: P,
    /// The direction from that pulley's centre, from its plane's x axis.
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub angle: Option<P>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PulleyDef<P = ParamId> {
    pub kind: PulleyKind,
    /// The belt profile: needed with teeth, optional for a smooth idler
    /// (its belt's pitch and the check that two pulleys share a belt).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<BeltProfile>,
    /// An origin plane, a construction plane or a planar face; none with
    /// `belt_with`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plane: Option<GeomRef>,
    /// The axis, in the plane's coordinates.
    #[serde(default, skip_serializing_if = "is_origin")]
    pub center: [f64; 2],
    /// Turns the pulley about its axis; not with `belt_with`.
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub angle: Option<P>,
    /// The toothed kinds' number of teeth.
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub teeth: Option<P>,
    /// A smooth idler's running diameter.
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub diameter: Option<P>,
    pub belt_width: P,
    /// Added to the belt's width: the toothed (running) width.
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub clearance: Option<P>,
    /// Added to the outside diameter (the pitch diameter's correction).
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub pitch_correction: Option<P>,
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub flanges: Option<Flanges<P>>,
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub hub: Option<Hub<P>>,
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub bore: Option<Bore<P>>,
    /// A DIN 6885 keyway in a plain bore.
    #[serde(default, skip_serializing_if = "is_false")]
    pub keyway: bool,
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub set_screws: Option<SetScrews<P>>,
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub belt_with: Option<BeltWith<P>>,
    /// On a shaft instead of `plane` and `center` (mitcad#578).
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub seat: Option<Seat<P>>,
    /// Added to the shaft's diameter: the plain bore of a seated pulley
    /// (0 without it).
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub bore_clearance: Option<P>,
    pub operation: Operation,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub participants: Vec<BodyUid>,
    /// Placed by picked geometry instead of `plane` and `center`
    /// (mitcad#656, `place_at.rs`).
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub at: Option<PlaceAt<P>>,
    /// A belt checked round this pulley and an earlier one (mitcad#656):
    /// no geometry; `pulley_values` gives the check.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub belt_check: Option<BeltCheck>,
}

fn is_origin(center: &[f64; 2]) -> bool {
    *center == [0.0, 0.0]
}

impl<P> PulleyDef<P> {
    pub const TYPE: &'static str = "pulley";
    pub const BASE_NAME: &'static str = "Pulley";

    // Written by hand: the optional values come first, nested parts under prefixes.
    pub fn map_params<Q, E>(
        &self,
        f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
    ) -> Result<PulleyDef<Q>, E> {
        let mut option = |slot: &str, v: &Option<P>| v.as_ref().map(|v| f(slot, v)).transpose();
        let angle = option("angle", &self.angle)?;
        let teeth = option("teeth", &self.teeth)?;
        let diameter = option("diameter", &self.diameter)?;
        let clearance = option("clearance", &self.clearance)?;
        let pitch_correction = option("pitch_correction", &self.pitch_correction)?;
        let bore_clearance = option("bore_clearance", &self.bore_clearance)?;
        let flanges = match &self.flanges {
            None => None,
            Some(flanges) => Some(Flanges {
                sides: flanges.sides,
                diameter: option("flanges.diameter", &flanges.diameter)?,
                thickness: option("flanges.thickness", &flanges.thickness)?,
            }),
        };
        let set_screws = match &self.set_screws {
            None => None,
            Some(screws) => Some(SetScrews {
                count: f("set_screws.count", &screws.count)?,
                thread: screws.thread.clone(),
                position: screws
                    .position
                    .as_ref()
                    .map(|v| f("set_screws.position", v))
                    .transpose()?,
            }),
        };
        let seat = match &self.seat {
            None => None,
            Some(seat) => Some(seat.map_params("seat.", &mut *f)?),
        };
        Ok(PulleyDef {
            kind: self.kind,
            profile: self.profile,
            plane: self.plane.clone(),
            center: self.center,
            angle,
            teeth,
            diameter,
            belt_width: f("belt_width", &self.belt_width)?,
            clearance,
            pitch_correction,
            flanges,
            hub: match &self.hub {
                None => None,
                Some(hub) => Some(Hub {
                    diameter: hub
                        .diameter
                        .as_ref()
                        .map(|v| f("hub.diameter", v))
                        .transpose()?,
                    length: f("hub.length", &hub.length)?,
                }),
            },
            bore: match &self.bore {
                None => None,
                Some(Bore::Plain { diameter }) => Some(Bore::Plain {
                    diameter: f("bore.diameter", diameter)?,
                }),
                Some(Bore::Thread { thread }) => Some(Bore::Thread {
                    thread: thread.clone(),
                }),
            },
            keyway: self.keyway,
            set_screws,
            belt_with: match &self.belt_with {
                None => None,
                Some(belt) => Some(BeltWith {
                    pulley: belt.pulley,
                    center_distance: f("belt_with.center_distance", &belt.center_distance)?,
                    angle: belt
                        .angle
                        .as_ref()
                        .map(|v| f("belt_with.angle", v))
                        .transpose()?,
                }),
            },
            seat,
            bore_clearance,
            operation: self.operation,
            participants: self.participants.clone(),
            at: map_at(&self.at, &mut *f)?,
            belt_check: self.belt_check.clone(),
        })
    }
}

/// The definition with its values.
pub fn evaluated(def: &PulleyDef, value: &dyn Fn(ParamId) -> f64) -> PulleyDef<f64> {
    def.map_params(&mut |_, id: &ParamId| Ok::<_, ()>(value(*id)))
        .expect("no errors")
}

/// Where the parts of a pulley lie along its axis, from its plane.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Layout {
    /// The flanges' starts.
    pub flange_a: Option<f64>,
    pub flange_b: Option<f64>,
    /// The toothed (running) part's start and end.
    pub teeth: (f64, f64),
    /// The hub's start and end.
    pub hub: Option<(f64, f64)>,
    /// The whole length.
    pub length: f64,
}

impl PulleyDef<f64> {
    /// The profile's dimensions, when there is a profile.
    pub fn data(&self) -> Option<pulley_profile::ProfileData> {
        self.profile.map(BeltProfile::data)
    }

    /// The toothed (running) width: the belt's width and the clearance.
    pub fn width(&self) -> f64 {
        self.belt_width + self.clearance.unwrap_or(DEFAULT_CLEARANCE)
    }

    /// The whole number of teeth of a toothed kind.
    pub fn tooth_count(&self) -> Option<f64> {
        self.teeth
            .filter(|_| self.kind.toothed())
            .map(|t| t.round())
    }

    /// The pitch diameter of a toothed kind: z p / pi.
    pub fn pitch_diameter(&self) -> Option<f64> {
        Some(pulley_profile::pitch_diameter(
            self.profile?,
            self.tooth_count()?,
        ))
    }

    /// The outside diameter: a toothed kind's pitch diameter, corrected,
    /// less twice the pitch line offset; an idler's running diameter.
    pub fn outside_diameter(&self) -> f64 {
        match (self.kind.toothed(), self.profile, self.tooth_count()) {
            (true, Some(profile), Some(teeth)) => pulley_profile::outside_diameter(
                profile,
                teeth,
                self.pitch_correction.unwrap_or(0.0),
            ),
            (true, _, _) => f64::NAN,
            (false, _, _) => self.diameter.unwrap_or(f64::NAN),
        }
    }

    /// The diameter of the groove bottoms (an idler's running diameter).
    pub fn root_diameter(&self) -> f64 {
        match self.data() {
            Some(data) if self.kind.toothed() => self.outside_diameter() - 2.0 * data.groove.depth,
            _ => self.outside_diameter(),
        }
    }

    /// The radius the belt's pitch line wraps: a toothed kind's pitch
    /// radius, an idler's running radius.
    pub fn belt_radius(&self) -> f64 {
        if self.kind.toothed() {
            self.pitch_diameter().unwrap_or(f64::NAN) / 2.0
        } else {
            self.outside_diameter() / 2.0
        }
    }

    /// The bore's diameter.
    pub fn bore_diameter(&self) -> Result<Option<f64>, String> {
        Ok(match &self.bore {
            None => None,
            Some(Bore::Plain { diameter }) => Some(*diameter),
            Some(Bore::Thread { thread }) => Some(thread.data()?.minor),
        })
    }

    /// A seated pulley without a bore gets the plain bore of its shaft of
    /// diameter `shaft`: the shaft's diameter and the bore clearance.
    pub fn seat_bore(&mut self, shaft: f64) {
        // A placement on a shaft too (mitcad#656); an axis of its own has no
        // diameter.
        if (self.seat.is_some() || self.at.is_some()) && self.bore.is_none() && shaft.is_finite() {
            self.bore = Some(Bore::Plain {
                diameter: shaft + self.bore_clearance.unwrap_or(0.0),
            });
        }
    }

    /// Whether the bore is plain: given so, or a seat's.
    fn plain_bore(&self) -> bool {
        matches!(self.bore, Some(Bore::Plain { .. }))
            || ((self.seat.is_some() || self.at.as_ref().is_some_and(PlaceAt::on_shaft))
                && self.bore.is_none())
    }

    /// The flanges' diameter worked out when none is given (mitcad#656):
    /// the outside diameter and, all round, the groove's depth (none
    /// without a profile) and [`FLANGE_MARGIN`].
    pub fn default_flange_diameter(&self) -> f64 {
        let depth = self.data().map_or(0.0, |data| data.groove.depth);
        self.outside_diameter() + 2.0 * (depth + FLANGE_MARGIN)
    }

    /// The hub's diameter: given, or worked out from the bore (a seat's
    /// too), a set screw's tap drill and the roots (mitcad#656,
    /// [`super::gear::hub_diameter`]).
    pub fn hub_diameter(&self) -> f64 {
        let given = self.hub.as_ref().and_then(|hub| hub.diameter);
        given.unwrap_or_else(|| {
            let screw = self
                .set_screws
                .as_ref()
                .and_then(|screws| screws.thread.data().ok())
                .map(|data| data.minor);
            super::gear::hub_diameter(
                self.bore_diameter().ok().flatten(),
                self.root_diameter(),
                screw,
            )
        })
    }

    /// The flanges' sides, diameter (given or worked out) and thickness.
    fn flange_values(&self) -> (FlangeSides, f64, f64) {
        match &self.flanges {
            None => (FlangeSides::None, 0.0, 0.0),
            Some(f) => (
                f.sides,
                f.diameter.unwrap_or_else(|| self.default_flange_diameter()),
                f.thickness.unwrap_or(f64::NAN),
            ),
        }
    }

    /// Where the parts lie along the axis.
    pub fn layout(&self) -> Layout {
        let (sides, _, t) = self.flange_values();
        let mut at = 0.0;
        let flange_a = (sides != FlangeSides::None).then(|| {
            at += t;
            0.0
        });
        let teeth = (at, at + self.width());
        at = teeth.1;
        let flange_b = (sides == FlangeSides::Both).then(|| {
            at += t;
            teeth.1
        });
        let hub = self.hub.as_ref().map(|hub| {
            let start = at;
            at += hub.length;
            (start, at)
        });
        Layout {
            flange_a,
            flange_b,
            teeth,
            hub,
            length: at,
        }
    }

    /// The set screws' tap drill diameter, axial position (from the plane)
    /// and count.
    pub fn set_screw_values(&self) -> Result<Option<(f64, f64, usize)>, String> {
        let Some(screws) = &self.set_screws else {
            return Ok(None);
        };
        let Some((start, end)) = self.layout().hub else {
            return Err("set screws need a hub".to_owned());
        };
        let d = screws.thread.data()?.minor;
        let from_end = screws.position.unwrap_or((end - start) / 2.0);
        Ok(Some((d, end - from_end, screws.count.round() as usize)))
    }

    /// Checks the values as the kind needs them.
    pub fn check(&self) -> Result<(), String> {
        let kind = self.kind;
        let name = kind.name();
        let positive = |v: f64, what: &str| {
            if v > 0.0 && v.is_finite() {
                Ok(v)
            } else {
                Err(format!(
                    "the {name}'s {what} must be greater than zero, got {v}"
                ))
            }
        };
        let finite = |v: Option<f64>, what: &str| match v {
            Some(v) if !v.is_finite() => Err(format!("the {name}'s {what} must be a number")),
            _ => Ok(()),
        };
        for (v, what) in [
            (self.angle, "angle"),
            (self.clearance, "clearance"),
            (self.pitch_correction, "pitch correction"),
            (self.bore_clearance, "bore clearance"),
        ] {
            finite(v, what)?;
        }
        if let Some(at) = &self.at {
            at.check_values()?;
        }
        if let Some(seat) = &self.seat {
            seat.check_values()?;
        }
        positive(self.belt_width, "belt width")?;
        if self.clearance.is_some_and(|c| c < 0.0) {
            return Err(format!("the {name}'s clearance must not be negative"));
        }
        if kind.toothed() {
            let profile = self
                .profile
                .ok_or_else(|| format!("a {name} needs a belt profile"))?;
            let data = profile.data();
            let teeth = self
                .teeth
                .ok_or_else(|| format!("a {name} needs a number of teeth"))?;
            if self.diameter.is_some() {
                return Err(format!(
                    "a {name} takes its size from its teeth: it has no diameter"
                ));
            }
            if !teeth.is_finite() || (teeth - teeth.round()).abs() > 1.0e-9 {
                return Err(mitcad_model_base::internal::parameters::count_error(
                    format!("the {name}'s number of teeth must be whole, got {teeth}"),
                    teeth,
                ));
            }
            if teeth < data.min_teeth || teeth > 1000.0 {
                return Err(format!(
                    "a {} {name} needs {} to 1000 teeth, got {teeth}",
                    data.name, data.min_teeth
                ));
            }
            let correction = self.pitch_correction.unwrap_or(0.0);
            if correction.abs() > data.pitch / 4.0 {
                return Err(format!(
                    "the pitch correction must be within {} mm (a quarter of the pitch), got {correction}",
                    data.pitch / 4.0
                ));
            }
            pulley_profile::check_teeth(profile, teeth, self.outside_diameter() / 2.0)?;
        } else {
            if self.teeth.is_some() {
                return Err("an idler has no teeth (a toothed idler has)".to_owned());
            }
            if self.pitch_correction.is_some() {
                return Err("an idler has no pitch correction: its diameter is its own".to_owned());
            }
            let d = self
                .diameter
                .ok_or_else(|| "an idler needs a running diameter".to_owned())?;
            positive(d, "diameter")?;
        }
        let outside = self.outside_diameter();
        // Flanges and hub.
        let (sides, fd, ft) = self.flange_values();
        if sides != FlangeSides::None {
            positive(fd, "flange diameter")?;
            positive(ft, "flange thickness")?;
            if fd <= outside {
                return Err(format!(
                    "the flanges must be larger than the {name}'s outside diameter ({outside:.4} mm), got {fd}"
                ));
            }
        }
        if let Some(hub) = &self.hub {
            positive(self.hub_diameter(), "hub diameter")?;
            positive(hub.length, "hub length")?;
        }
        // The bore.
        let bore = self.bore_diameter()?;
        if self.keyway && !self.plain_bore() {
            return Err("a keyway needs a plain bore".to_owned());
        }
        if let Some(d) = bore {
            positive(d, "bore diameter")?;
            let mut reach = d / 2.0;
            if self.keyway {
                let (_, depth) = gear_profile::keyway(d).ok_or_else(|| {
                    format!("DIN 6885 has no keyway for a bore of {d} mm (6 to 230 mm)")
                })?;
                reach += depth;
            }
            let limit = self.root_diameter() / 2.0 - MIN_WALL;
            if reach >= limit {
                return Err(format!(
                    "the bore reaches the {}: it must stay within {limit:.4} mm of the axis",
                    if kind.toothed() {
                        "grooves"
                    } else {
                        "running face"
                    }
                ));
            }
            if self.hub.is_some() && d >= self.hub_diameter() {
                return Err("the hub must be wider than the bore".to_owned());
            }
        }
        // Set screws.
        if let Some(screws) = &self.set_screws {
            let count = screws.count;
            if !count.is_finite()
                || (count - count.round()).abs() > 1.0e-9
                || !(1.0..=MAX_SET_SCREWS).contains(&count)
            {
                return Err(mitcad_model_base::internal::parameters::count_error(
                    format!("1 to {MAX_SET_SCREWS} set screws fit a hub, got {count}"),
                    count,
                ));
            }
            finite(screws.position, "set screws' position")?;
            if let (Some((d, _, _)), Some(hub)) = (self.set_screw_values()?, &self.hub) {
                let position = screws.position.unwrap_or(hub.length / 2.0);
                if position < d / 2.0 || position > hub.length - d / 2.0 {
                    return Err(format!(
                        "the set screws' tap drills ({d:.4} mm) must lie within the hub: their position from its end between {:.4} and {:.4} mm, got {position}",
                        d / 2.0,
                        hub.length - d / 2.0
                    ));
                }
                if bore.is_some_and(|b| d >= self.hub_diameter() - b) {
                    return Err("the set screws' tap drills are too large for the hub".to_owned());
                }
            }
        }
        if let Some(belt) = &self.belt_with {
            positive(belt.center_distance, "center distance")?;
            finite(belt.angle, "belt angle")?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Belts

/// The values of an open belt round two pulleys.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BeltDrive {
    pub center_distance: f64,
    /// The belt's pitch length.
    pub belt_length: f64,
    /// The belt profile's pitch and the belt's teeth (length / pitch),
    /// when a pulley has a profile.
    pub pitch: Option<f64>,
    pub belt_teeth: Option<f64>,
    /// The whole teeth in mesh on the smaller toothed pulley.
    pub teeth_in_mesh: Option<f64>,
    /// The belt's wrap on the smaller and the larger pulley (radians).
    pub wrap: (f64, f64),
}

/// The open belt round `a` and `b`, `distance` apart: its pitch length
/// L = 2 C cos(t) + pi (R + r) + 2 t (R - r), sin(t) = (R - r) / C, from
/// the radii R and r its pitch line wraps (a toothed pulley's pitch radius,
/// an idler's running radius); the belt wraps pi - 2 t of the smaller
/// pulley and pi + 2 t of the larger.
pub fn belt_drive(
    a: &PulleyDef<f64>,
    b: &PulleyDef<f64>,
    distance: f64,
) -> Result<BeltDrive, String> {
    let profile = match (a.profile, b.profile) {
        (Some(p), Some(q)) if p != q => {
            return Err(format!(
                "pulleys of different belt profiles ({} and {}) do not share a belt",
                p.data().name,
                q.data().name
            ));
        }
        (p, q) => p.or(q),
    };
    let (ra, rb) = (a.belt_radius(), b.belt_radius());
    if !(ra > 0.0 && rb > 0.0) {
        return Err("the pulleys need their sizes for a belt".to_owned());
    }
    if distance.is_nan() || distance <= ra + rb {
        return Err(format!(
            "the pulleys' pitch circles overlap: the center distance must be more than {:.4} mm, got {distance}",
            ra + rb
        ));
    }
    let (big, small) = (ra.max(rb), ra.min(rb));
    let t = ((big - small) / distance).asin();
    let belt_length = 2.0 * distance * t.cos() + PI * (big + small) + 2.0 * t * (big - small);
    let wrap = (PI - 2.0 * t, PI + 2.0 * t);
    let wrap_of = |r: f64, other: f64| if r <= other { wrap.0 } else { wrap.1 };
    // The smaller toothed pulley's teeth in mesh.
    let toothed: Vec<(f64, f64, f64)> = [(a, ra, rb), (b, rb, ra)]
        .iter()
        .filter_map(|(p, r, other)| Some((p.tooth_count()?, *r, *other)))
        .collect();
    let teeth_in_mesh = toothed
        .iter()
        .min_by(|x, y| x.1.total_cmp(&y.1))
        .map(|(teeth, r, other)| (teeth * wrap_of(*r, *other) / TAU + 1.0e-9).floor());
    let pitch = profile.map(|p| p.data().pitch);
    Ok(BeltDrive {
        center_distance: distance,
        belt_length,
        pitch,
        belt_teeth: pitch.map(|p| belt_length / p),
        teeth_in_mesh,
        wrap,
    })
}

// ---------------------------------------------------------------------------
// Placement

/// Where a pulley lies: its frame (origin on the axis on its plane) and its
/// turn about the axis (tooth 1's middle, the keyway).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    pub frame: SketchFrame,
    pub phase: f64,
}

impl PulleyDef {
    fn params_list(&self) -> Vec<ParamId> {
        let mut ids = Vec::new();
        let _ = self.map_params(&mut |_, id: &ParamId| {
            ids.push(*id);
            Ok::<_, ()>(())
        });
        ids
    }
}

/// The pulley's placement: its plane, or its belt mate's; with the mate's
/// values when it has one.
#[allow(clippy::type_complexity)]
pub(crate) fn placement<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    def: &PulleyDef,
    depth: usize,
) -> Result<(PulleyDef<f64>, Placement, Option<PulleyDef<f64>>), String> {
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
    let mut pulley = evaluated(def, &value);
    let Some(belt) = pulley.belt_with.clone() else {
        let phase = pulley.angle.unwrap_or(0.0);
        // Placed by picked geometry (mitcad#656): on a shaft its bore
        // follows the shaft.
        if let Some(at) = pulley.at.clone() {
            let placed = ctx.place(&at)?;
            if let Some(seat) = placed.round() {
                pulley.seat_bore(seat.diameter);
            }
            let frame = placed.frame;
            return Ok((pulley, Placement { frame, phase }, None));
        }
        // On a shaft (mitcad#578): its bore follows the shaft.
        if let Some(seat) = pulley.seat.clone() {
            let seat = ctx.seat(&seat).map_err(|e| format!("seat: {e}"))?;
            pulley.seat_bore(seat.diameter);
            let frame = seat.sketch_frame();
            return Ok((pulley, Placement { frame, phase }, None));
        }
        let plane = def
            .plane
            .as_ref()
            .ok_or("a pulley needs a plane, a shaft, or a pulley to run with")?;
        let plane = ctx.plane(plane)?.frame();
        let frame = SketchFrame {
            origin: plane.point(pulley.center),
            ..plane
        };
        return Ok((pulley, Placement { frame, phase }, None));
    };
    if depth > 64 {
        return Err("the pulleys run with each other in a circle".to_owned());
    }
    let uid = belt.pulley;
    let entry = ctx
        .env
        .features
        .iter()
        .find(|f| f.uid == uid)
        .cloned()
        .ok_or_else(|| format!("feature {uid} does not exist"))?;
    let FeatureDef::Pulley(mate_def) = &entry.def else {
        return Err(format!("{} is not a pulley", entry.name));
    };
    if entry.component != ctx.component() {
        return Err(format!(
            "{} is in another component: a pulley runs with a pulley feature of its own component",
            entry.name
        ));
    }
    let version = ctx
        .env
        .history
        .iter()
        .find(|r| r.uid == uid)
        .and_then(|r| r.output.as_ref())
        .map(|o| o.version);
    ctx.reads.push(Read::Output(uid, version));
    let (mate, at, _) = placement(ctx, mate_def, depth + 1)?;
    let theta = belt.angle.unwrap_or(0.0);
    let frame = at.frame;
    let (s, c) = theta.sin_cos();
    let u = combine(frame.x_axis, c, frame.y_axis, s);
    let placed = Placement {
        frame: SketchFrame {
            origin: add_scaled(frame.origin, u, belt.center_distance),
            ..frame
        },
        phase: 0.0,
    };
    Ok((pulley, placed, Some(mate)))
}

// ---------------------------------------------------------------------------
// Bodies

fn sweep<K: Kernel>(
    ctx: &EvalContext<'_, K>,
    frame: SketchFrame,
    loops: &[Vec<NamedSegment>],
    start: f64,
    length: f64,
    names: (&str, &str),
) -> Result<K::Shape, String> {
    ctx.kernel
        .gear_sweep(&GearSweepSpec {
            feature: ctx.uid,
            frame,
            loops,
            turn: 0.0,
            start,
            steps: &[GearStep {
                length,
                twist: 0.0,
                shift: 0.0,
            }],
            start_name: names.0,
            end_name: names.1,
        })
        .map_err(|e| e.to_string())
}

fn cut<K: Kernel>(
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
        (None, _) => Err(format!("the {what} removes the whole pulley")),
        (Some(_), Some(_)) => Err(format!("the {what} cuts the pulley apart")),
    }
}

/// A set screw's frame: its normal along the radius at `angle`, its origin
/// on the axis `height` from the pulley's plane.
fn screw_frame(frame: &SketchFrame, angle: f64, height: f64) -> SketchFrame {
    let n = frame.normal();
    let (s, c) = angle.sin_cos();
    let u = combine(frame.x_axis, c, frame.y_axis, s);
    SketchFrame {
        origin: add_scaled(frame.origin, n, height),
        x_axis: n,
        y_axis: cross(u, n),
    }
}

/// The pulley's solid in the component's coordinates.
fn pulley_solid<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    pulley: &PulleyDef<f64>,
    at: Placement,
) -> Result<K::Shape, String> {
    let frame = at.frame;
    let layout = pulley.layout();
    let outside = pulley.outside_diameter() / 2.0;
    let outline = match (pulley.kind.toothed(), pulley.profile, pulley.tooth_count()) {
        (true, Some(profile), Some(teeth)) => {
            pulley_profile::toothed_outline(profile, teeth, outside, at.phase)?
        }
        (true, _, _) => return Err("a toothed pulley needs a profile and teeth".to_owned()),
        (false, _, _) => circle("running", outside),
    };
    let (start, end) = layout.teeth;
    let teeth = sweep(ctx, frame, &[outline], start, end - start, ("start", "end"))?;
    let mut parts = vec![teeth];
    let (_, fd, ft) = pulley.flange_values();
    if let Some(z) = layout.flange_a {
        parts.push(sweep(
            ctx,
            frame,
            &[circle("flange_a", fd / 2.0)],
            z,
            ft,
            ("flange_a_end", "flange_a_inner"),
        )?);
    }
    if let Some(z) = layout.flange_b {
        parts.push(sweep(
            ctx,
            frame,
            &[circle("flange_b", fd / 2.0)],
            z,
            ft,
            ("flange_b_inner", "flange_b_end"),
        )?);
    }
    if let (Some((z0, z1)), Some(_)) = (layout.hub, &pulley.hub) {
        parts.push(sweep(
            ctx,
            frame,
            &[circle("hub", pulley.hub_diameter() / 2.0)],
            z0,
            z1 - z0,
            ("hub_start", "hub_end"),
        )?);
    }
    let mut solid = if parts.len() == 1 {
        parts.pop().expect("the teeth")
    } else {
        let refs: Vec<&K::Shape> = parts.iter().collect();
        ctx.kernel.unite(&refs).map_err(|e| e.to_string())?
    };
    if let Some(d) = pulley.bore_diameter()? {
        let keyway = if pulley.keyway {
            gear_profile::keyway(d).map(|(w, depth)| (at.phase, w, depth))
        } else {
            None
        };
        let tool = sweep(
            ctx,
            frame,
            &[bore_outline(d, keyway)],
            -1.0,
            layout.length + 2.0,
            ("bore_start", "bore_end"),
        )?;
        solid = cut(ctx, &solid, &tool, "bore")?;
    }
    if let (Some((d, height, count)), Some(_)) = (pulley.set_screw_values()?, &pulley.hub) {
        for i in 0..count {
            let k = i + 1;
            let screw = screw_frame(&frame, at.phase + FRAC_PI_2 * i as f64, height);
            // The caps (plain roles: a cap's name takes no index).
            let (bottom, top) = (
                format!("set_screw_bottom_{k}"),
                format!("set_screw_top_{k}"),
            );
            let tool = sweep(
                ctx,
                screw,
                &[circle(&format!("set_screw({k})"), d / 2.0)],
                0.0,
                pulley.hub_diameter() / 2.0 + 1.0,
                (bottom.as_str(), top.as_str()),
            )?;
            solid = cut(ctx, &solid, &tool, "set screw")?;
        }
    }
    Ok(solid)
}

impl FeatureInfo for PulleyDef {
    fn references(&self) -> References {
        let mut references = References::default();
        if let Some(plane) = &self.plane {
            plane.add_to(&mut references);
        }
        if let Some(belt) = &self.belt_with {
            references.features.insert(belt.pulley);
        }
        if let Some(check) = &self.belt_check {
            references.features.insert(check.pulley);
        }
        if let Some(seat) = &self.seat {
            seat.add_to(&mut references);
        }
        if let Some(at) = &self.at {
            at.add_to(&mut references);
        }
        for body in &self.participants {
            references.body(*body);
        }
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        check_participants(ctx, self.operation, &self.participants)?;
        // A belt kept checked (mitcad#656): with another pulley.
        if let Some(check) = &self.belt_check {
            let entry = ctx.feature(check.pulley)?;
            if !matches!(entry.def, FeatureDef::Pulley(_)) {
                return Err(format!(
                    "{} is not a pulley: a belt is checked round two",
                    entry.name
                ));
            }
        }
        // Placed by picked geometry (mitcad#656): a bore given is kept.
        if let Some(at) = &self.at {
            if self.plane.is_some() || !is_origin(&self.center) || self.seat.is_some() {
                return Err(
                    "a pulley placed by at takes no plane, center or seat of its own".to_owned(),
                );
            }
            if self.belt_with.is_some() {
                return Err(
                    "a pulley placed by at is not placed by a belt too (the belt is checked with belt_check)"
                        .to_owned(),
                );
            }
            if self.bore_clearance.is_some() && self.bore.is_some() {
                return Err("a bore of its own takes no bore clearance".to_owned());
            }
            return at.check(ctx);
        }
        // On a shaft (mitcad#578).
        if let Some(seat) = &self.seat {
            if self.plane.is_some() || !is_origin(&self.center) {
                return Err(
                    "a pulley on a shaft takes its plane and center from the shaft".to_owned(),
                );
            }
            if self.belt_with.is_some() {
                return Err(
                    "a pulley on a shaft is not placed by a belt too (seat the first pulley of the belt)"
                        .to_owned(),
                );
            }
            // An axis of its own has no diameter: the bore is the pulley's
            // own (mitcad#656).
            if seat.body().is_some() && matches!(self.bore, Some(Bore::Plain { .. })) {
                return Err(
                    "a pulley on a shaft takes its plain bore from the shaft: leave out the bore (bore_clearance widens it)"
                        .to_owned(),
                );
            }
            if self.bore_clearance.is_some() && self.bore.is_some() {
                return Err("a threaded bore takes no bore clearance".to_owned());
            }
            return seat.check(ctx);
        }
        if self.bore_clearance.is_some() {
            return Err("a bore clearance needs a seat on a shaft".to_owned());
        }
        match (&self.plane, &self.belt_with) {
            (None, None) => {
                Err("a pulley needs a plane, a shaft, or a pulley to run with".to_owned())
            }
            (Some(_), Some(_)) => {
                Err("a pulley on a belt with another takes its plane from it".to_owned())
            }
            (Some(plane), None) => plane.check(ctx, Want::Plane),
            (None, Some(belt)) => {
                let entry = ctx.feature(belt.pulley)?;
                if !matches!(entry.def, FeatureDef::Pulley(_)) {
                    return Err(format!("{} is not a pulley", entry.name));
                }
                if self.angle.is_some() {
                    return Err(
                        "a pulley on a belt with another takes no angle (belt_with.angle places it)"
                            .to_owned(),
                    );
                }
                if !is_origin(&self.center) {
                    return Err(
                        "a pulley on a belt with another takes its center from it".to_owned()
                    );
                }
                Ok(())
            }
        }
    }

    fn check_values(&self, value: &dyn Fn(ParamId) -> f64) -> Result<(), String> {
        evaluated(self, value).check()
    }

    fn creates_bodies(&self) -> bool {
        true
    }

    fn new_component(&self) -> bool {
        self.operation == Operation::NewComponent
    }

    fn tool_use(&self) -> Option<ToolUse> {
        Some(ToolUse {
            operation: self.operation,
            participants: self.participants.clone(),
        })
    }
}

impl<K: Kernel> Evaluate<K> for PulleyDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let participants = participant_bodies(ctx, self.operation, &self.participants)?;
        let (pulley, at, mate) = placement(ctx, self, 0)?;
        pulley.check()?;
        if let (Some(mate), Some(belt)) = (&mate, &pulley.belt_with) {
            belt_drive(&pulley, mate, belt.center_distance)?;
        }
        let tool = pulley_solid(ctx, &pulley, at)?;
        let mut output =
            apply_operation(ctx, self.operation, participants, tool, pulley.kind.name())?;
        // The frame on the axis, as a library pulley's mount `bore`.
        output
            .mounts
            .insert(MOUNT.to_owned(), frame_transform(&at.frame));
        Ok(output)
    }
}

// ---------------------------------------------------------------------------
// Derived values

/// What the panel and the `pulley_values` query show of a pulley: mm,
/// `null` where a value does not apply.
pub fn derived_values(pulley: &PulleyDef<f64>) -> serde_json::Value {
    use serde_json::json;
    let layout = pulley.layout();
    let mut values = json!({
        "kind": pulley.kind,
        "tooth_width": pulley.width(),
        "length": layout.length,
    });
    let map = values.as_object_mut().expect("an object");
    // The bore: given, or a seat's (mitcad#578).
    if let Ok(Some(d)) = pulley.bore_diameter() {
        map.insert("bore_diameter".into(), json!(d));
    }
    // Flanges and hub: given, or worked out (mitcad#656; `computed`
    // lists those worked out).
    let mut computed = Vec::new();
    if let Some(flanges) = &pulley.flanges
        && flanges.sides != FlangeSides::None
    {
        map.insert("flange_diameter".into(), json!(pulley.flange_values().1));
        if flanges.diameter.is_none() {
            computed.push("flange_diameter");
        }
    }
    if let Some(hub) = &pulley.hub {
        map.insert("hub_diameter".into(), json!(pulley.hub_diameter()));
        if hub.diameter.is_none() {
            computed.push("hub_diameter");
        }
    }
    if !computed.is_empty() {
        map.insert("computed".into(), json!(computed));
    }
    if let Some(profile) = pulley.profile {
        let data = profile.data();
        map.insert("profile".into(), json!(profile));
        map.insert("pitch".into(), json!(data.pitch));
        map.insert("pitch_line_offset".into(), json!(data.pitch_line_offset));
        map.insert("min_teeth".into(), json!(data.min_teeth));
    }
    if pulley.kind.toothed() {
        if let Some(teeth) = pulley.tooth_count() {
            map.insert("teeth".into(), json!(teeth));
        }
        if let Some(pd) = pulley.pitch_diameter() {
            map.insert("pitch_diameter".into(), json!(pd));
        }
        let od = pulley.outside_diameter();
        if od.is_finite() {
            map.insert("outside_diameter".into(), json!(od));
            map.insert("root_diameter".into(), json!(pulley.root_diameter()));
        }
        if let (Some(profile), Some(teeth)) = (pulley.profile, pulley.tooth_count())
            && let Ok(outline) = pulley_profile::toothed_outline(profile, teeth, od / 2.0, 0.0)
        {
            map.insert(
                "section_area".into(),
                json!(gear_profile::loop_area(&outline)),
            );
        }
    } else if let Some(d) = pulley.diameter {
        map.insert("diameter".into(), json!(d));
        map.insert("outside_diameter".into(), json!(d));
        map.insert("section_area".into(), json!(PI * d * d / 4.0));
    }
    values
}

/// The mount frame a pulley's result keeps, as a library pulley's: its
/// placement's frame (x, y and the axis as the columns, the origin on the
/// axis on its plane as the translation).
pub const MOUNT: &str = "bore";

fn frame_transform(frame: &SketchFrame) -> crate::transform::Transform {
    let n = frame.normal();
    crate::transform::Transform {
        linear: std::array::from_fn(|r| [frame.x_axis[r], frame.y_axis[r], n[r]]),
        translation: frame.origin,
    }
}
