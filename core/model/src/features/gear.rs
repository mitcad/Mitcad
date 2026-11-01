// SPDX-License-Identifier: MIT
//! Gear (mitcad#472): spur, helical, herringbone, internal (ring), rack and
//! straight bevel gears with involute teeth (`gear_profile.rs` works out
//! the teeth), made from parameters on a plane: the gear's axis is the
//! plane's normal through `center`, its teeth start on the plane and run
//! `face_width` along the normal (a bevel gear's back lies on the plane,
//! its apex along the normal; a rack runs along the plane's x axis from
//! `center`, its teeth towards y). `angle` turns the gear about its axis
//! (tooth 1 is centred on the plane's x axis without it).
//!
//! `mesh_with` places the gear instead: in the plane of another gear
//! feature of the same component, at the centre distance of their mesh in
//! the direction `mesh_with.angle` from that gear's centre (or
//! `mesh_with.position` along a rack from its middle), turned so that the
//! teeth engage. Bores (plain with an optional DIN 6885 keyway, or a thread
//! size's tap drill) and hubs go through or on spur, helical, herringbone
//! and bevel gears. A bevel gear's `pair` also makes its mate.
//!
//! `seat` places a gear with a bore (spur, helical, herringbone, a single
//! bevel gear) on a shaft instead of `plane` and `center` (mitcad#578,
//! `seat.rs`): its plane at the seat's offset along the shaft, its axis
//! the shaft's; without a `bore` its bore is plain, the shaft's diameter
//! plus `bore_clearance`, and follows the shaft.
//!
//! `at` places a gear (not a rack) by picked geometry (mitcad#656,
//! `place_at.rs`): a shaft or an axis, a point, a point on a face. With
//! `mesh_with` too the gear stays where `at` puts it and the mesh only
//! turns it to engage. `mesh_with.toward` picks the direction from the
//! mate's centre instead of typing `mesh_with.angle`.

use std::f64::consts::{PI, TAU};

use serde::{Deserialize, Serialize};

use mitcad_vec3::{add_scaled, combine};

use super::extrude::{Operation, apply_operation, check_participants, participant_bodies};
use super::gear_profile::{
    self, Bevel, NamedSegment, Rack, Tooth, bore_outline, circle, mesh_distance,
};
use super::geom_ref::{GeomRef, Want};
use super::place_at::{PlaceAt, map_at};
use super::seat::Seat;
use super::thread::ThreadSize;
use super::{
    CheckContext, EvalContext, Evaluate, FeatureDef, FeatureInfo, FeatureOutput, References,
    ToolUse, is_false,
};
use crate::ids::{BodyUid, FeatureUid};
use crate::kernel::{BooleanOp, GearConeSpec, GearStep, GearSweepSpec, Kernel};
use crate::parameters::ParamId;
use crate::profile::SketchFrame;
use crate::recompute::Read;
use mitcad_vec3::{add, norm, scale, sub};
use mitcad_vec3::{cross, dot};

/// What kind of gear.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GearKind {
    Spur,
    Helical,
    Herringbone,
    Ring,
    Rack,
    Bevel,
}

impl GearKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Spur => "spur gear",
            Self::Helical => "helical gear",
            Self::Herringbone => "herringbone gear",
            Self::Ring => "ring gear",
            Self::Rack => "rack",
            Self::Bevel => "bevel gear",
        }
    }

    /// An external gear on a parallel axis.
    fn external(self) -> bool {
        matches!(self, Self::Spur | Self::Helical | Self::Herringbone)
    }
}

/// A bore through the gear (and its hub).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Bore<P = ParamId> {
    Plain {
        diameter: P,
    },
    /// The tap drill of a thread size (its minor diameter), listed as a
    /// cosmetic thread by the `threads` query.
    Thread {
        thread: ThreadSize,
    },
}

/// A hub on the gear's end face (a bevel gear's back), round its axis.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hub<P = ParamId> {
    /// Worked out without it (mitcad#656, [`hub_diameter`]).
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub diameter: Option<P>,
    pub length: P,
}

/// The least wall a worked-out hub keeps round its bore (mm).
pub const HUB_WALL: f64 = 2.0;

/// A hub's diameter worked out when none is given (mitcad#656): the bore
/// with a wall of [`HUB_WALL`] round it, thicker for a set screw's tap
/// drill (`screw`, its diameter); without a bore half the teeth's root
/// diameter (`root`), at least two walls.
pub fn hub_diameter(bore: Option<f64>, root: f64, screw: Option<f64>) -> f64 {
    let wall = HUB_WALL.max(screw.unwrap_or(0.0));
    match bore {
        Some(d) => d + 2.0 * wall,
        None => (root / 2.0).max(2.0 * wall),
    }
}

/// Where a gear meshes with an earlier gear feature.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeshWith<P = ParamId> {
    pub gear: FeatureUid,
    /// The direction from that gear's centre, from its plane's x axis.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub angle: Option<P>,
    /// Along a rack from the middle of its teeth.
    #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
    pub position: Option<P>,
    /// The direction from that gear's centre towards a picked point or
    /// axis instead of `angle` (mitcad#656).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub toward: Option<GeomRef>,
}

fn none<P>() -> Option<P> {
    None
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GearDef<P = ParamId> {
    pub kind: GearKind,
    /// An origin plane, a construction plane or a planar face; none with
    /// `mesh_with`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plane: Option<GeomRef>,
    /// The axis (a rack's start), in the plane's coordinates.
    #[serde(default, skip_serializing_if = "is_origin")]
    pub center: [f64; 2],
    pub module: P,
    pub teeth: P,
    /// 20 degrees without it.
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub pressure_angle: Option<P>,
    pub face_width: P,
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub helix_angle: Option<P>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub left_handed: bool,
    /// A herringbone gear's gap between its halves.
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub gap: Option<P>,
    /// A ring gear's rim outside its roots.
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub rim: Option<P>,
    /// A rack's height from its back to its tips, and its length (the
    /// teeth's pitch line without it).
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub height: Option<P>,
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub length: Option<P>,
    /// A bevel gear's shaft angle (90 degrees without it) and its mate's
    /// teeth (its own without them).
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub shaft_angle: Option<P>,
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub mate_teeth: Option<P>,
    /// A bevel gear also makes its mate.
    #[serde(default, skip_serializing_if = "is_false")]
    pub pair: bool,
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub backlash: Option<P>,
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub profile_shift: Option<P>,
    /// Turns the gear about its axis (a rack about `center`).
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub angle: Option<P>,
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub bore: Option<Bore<P>>,
    /// A DIN 6885 keyway in a plain bore.
    #[serde(default, skip_serializing_if = "is_false")]
    pub keyway: bool,
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub hub: Option<Hub<P>>,
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub mesh_with: Option<MeshWith<P>>,
    /// On a shaft instead of `plane` and `center` (mitcad#578).
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub seat: Option<Seat<P>>,
    /// Added to the shaft's diameter: the plain bore of a seated gear (0
    /// without it).
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub bore_clearance: Option<P>,
    pub operation: Operation,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub participants: Vec<BodyUid>,
    /// Placed by picked geometry instead of `plane` and `center`
    /// (mitcad#656, `place_at.rs`); with `mesh_with` the mesh only turns it.
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub at: Option<PlaceAt<P>>,
}

fn is_origin(center: &[f64; 2]) -> bool {
    *center == [0.0, 0.0]
}

impl<P> GearDef<P> {
    pub const TYPE: &'static str = "gear";
    pub const BASE_NAME: &'static str = "Gear";

    // Written by hand: the optional values come first, nested parts under prefixes.
    pub fn map_params<Q, E>(
        &self,
        f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
    ) -> Result<GearDef<Q>, E> {
        let mut option = |slot: &str, v: &Option<P>| v.as_ref().map(|v| f(slot, v)).transpose();
        let pressure_angle = option("pressure_angle", &self.pressure_angle)?;
        let helix_angle = option("helix_angle", &self.helix_angle)?;
        let gap = option("gap", &self.gap)?;
        let rim = option("rim", &self.rim)?;
        let height = option("height", &self.height)?;
        let length = option("length", &self.length)?;
        let shaft_angle = option("shaft_angle", &self.shaft_angle)?;
        let mate_teeth = option("mate_teeth", &self.mate_teeth)?;
        let backlash = option("backlash", &self.backlash)?;
        let profile_shift = option("profile_shift", &self.profile_shift)?;
        let angle = option("angle", &self.angle)?;
        let bore_clearance = option("bore_clearance", &self.bore_clearance)?;
        let seat = match &self.seat {
            None => None,
            Some(seat) => Some(seat.map_params("seat.", &mut *f)?),
        };
        Ok(GearDef {
            kind: self.kind,
            plane: self.plane.clone(),
            center: self.center,
            module: f("module", &self.module)?,
            teeth: f("teeth", &self.teeth)?,
            pressure_angle,
            face_width: f("face_width", &self.face_width)?,
            helix_angle,
            left_handed: self.left_handed,
            gap,
            rim,
            height,
            length,
            shaft_angle,
            mate_teeth,
            pair: self.pair,
            backlash,
            profile_shift,
            angle,
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
            mesh_with: match &self.mesh_with {
                None => None,
                Some(mesh) => Some(MeshWith {
                    gear: mesh.gear,
                    angle: mesh
                        .angle
                        .as_ref()
                        .map(|v| f("mesh_with.angle", v))
                        .transpose()?,
                    position: mesh
                        .position
                        .as_ref()
                        .map(|v| f("mesh_with.position", v))
                        .transpose()?,
                    toward: mesh.toward.clone(),
                }),
            },
            seat,
            bore_clearance,
            operation: self.operation,
            participants: self.participants.clone(),
            at: map_at(&self.at, &mut *f)?,
        })
    }
}

/// The definition with its values.
pub fn evaluated(def: &GearDef, value: &dyn Fn(ParamId) -> f64) -> GearDef<f64> {
    def.map_params(&mut |_, id: &ParamId| Ok::<_, ()>(value(*id)))
        .expect("no errors")
}

impl GearDef<f64> {
    /// The teeth's values.
    pub fn tooth(&self) -> Tooth {
        let helix = self.helix_angle.unwrap_or(0.0);
        Tooth {
            module: self.module,
            teeth: self.teeth,
            pressure_angle: self.pressure_angle.unwrap_or(20f64.to_radians()),
            helix_angle: if self.left_handed { -helix } else { helix },
            shift: self.profile_shift.unwrap_or(0.0),
            backlash: self.backlash.unwrap_or(0.0),
        }
    }

    /// A bevel gear's pitch angle, its mate's teeth and the shaft angle.
    pub fn bevel(&self) -> (Bevel, f64, f64) {
        let shaft = self.shaft_angle.unwrap_or(PI / 2.0);
        let mate = self.mate_teeth.unwrap_or(self.teeth);
        let mut tooth = self.tooth();
        tooth.helix_angle = 0.0;
        (
            Bevel {
                tooth,
                pitch_angle: Bevel::pitch_angle_of(self.teeth, mate, shaft),
                face_width: self.face_width,
            },
            mate,
            shaft,
        )
    }

    /// A rack's values.
    pub fn rack(&self) -> Rack {
        let tooth = self.tooth();
        let pitch_length = tooth.teeth.round() * tooth.pitch();
        Rack {
            tooth,
            height: self.height.unwrap_or(f64::NAN),
            length: self.length.unwrap_or(pitch_length),
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

    /// The hub's diameter: given, or worked out from the bore and the
    /// roots (mitcad#656, [`hub_diameter`]).
    pub fn hub_diameter(&self) -> f64 {
        let given = self.hub.as_ref().and_then(|hub| hub.diameter);
        given.unwrap_or_else(|| {
            hub_diameter(
                self.bore_diameter().ok().flatten(),
                2.0 * self.tooth().root_radius(),
                None,
            )
        })
    }

    /// A seated gear without a bore gets the plain bore of its shaft of
    /// diameter `shaft`: the shaft's diameter and the bore clearance.
    pub fn seat_bore(&mut self, shaft: f64) {
        // A placement on a shaft too (mitcad#656); an axis of its own has no
        // diameter, and a ring or a rack no bore.
        let bored = self.kind.external() || self.kind == GearKind::Bevel;
        if (self.seat.is_some() || self.at.is_some())
            && self.bore.is_none()
            && shaft.is_finite()
            && bored
        {
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

    /// The radius a bore and a hub must stay inside: the roots less a
    /// module.
    fn inner_limit(&self) -> f64 {
        let tooth = self.tooth();
        match self.kind {
            GearKind::Bevel => {
                let (bevel, _, _) = self.bevel();
                // The toe's root circle, less a module.
                let toe = (bevel.cone_distance() - self.face_width) / bevel.cone_distance();
                toe * tooth.root_radius() - tooth.module
            }
            _ => tooth.root_radius() - tooth.module,
        }
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
        positive(self.module, "module")?;
        positive(self.face_width, "face width")?;
        if (self.teeth - self.teeth.round()).abs() > 1.0e-9 {
            return Err(mitcad_model_base::internal::parameters::count_error(
                format!(
                    "the {name}'s number of teeth must be whole, got {}",
                    self.teeth
                ),
                self.teeth,
            ));
        }
        let not = |present: bool, what: &str| {
            if present {
                Err(format!("a {name} has no {what}"))
            } else {
                Ok(())
            }
        };
        let helical = matches!(kind, GearKind::Helical | GearKind::Herringbone);
        match self.helix_angle {
            None if helical => return Err(format!("a {name} needs a helix angle")),
            Some(a) if helical && a <= 0.0 => {
                return Err(format!(
                    "a {name}'s helix angle must be greater than zero (left_handed turns it), got {a}"
                ));
            }
            Some(_) if matches!(kind, GearKind::Spur | GearKind::Bevel) => {
                return Err(format!("a {name} has no helix angle (a helical gear has)"));
            }
            Some(a) if a < 0.0 => {
                return Err(format!(
                    "the helix angle must not be negative (left_handed turns it), got {a}"
                ));
            }
            _ => {}
        }
        not(
            self.left_handed && self.helix_angle.is_none_or(|a| a == 0.0),
            "hand without a helix angle",
        )?;
        not(self.gap.is_some() && kind != GearKind::Herringbone, "gap")?;
        if let Some(gap) = self.gap
            && !(gap >= 0.0 && gap < self.face_width)
        {
            return Err(format!(
                "the gap must be at least zero and less than the face width, got {gap}"
            ));
        }
        match (kind, self.rim) {
            (GearKind::Ring, None) => return Err("a ring gear needs a rim thickness".to_owned()),
            (GearKind::Ring, Some(rim)) => {
                positive(rim, "rim thickness")?;
            }
            (_, Some(_)) => not(true, "rim")?,
            _ => {}
        }
        match (kind, self.height) {
            (GearKind::Rack, None) => return Err("a rack needs a height".to_owned()),
            (GearKind::Rack, Some(_)) => {}
            (_, Some(_)) => not(true, "height")?,
            _ => {}
        }
        not(self.length.is_some() && kind != GearKind::Rack, "length")?;
        let bevel = kind == GearKind::Bevel;
        not(!bevel && self.shaft_angle.is_some(), "shaft angle")?;
        not(!bevel && self.mate_teeth.is_some(), "mate")?;
        not(!bevel && self.pair, "pair")?;
        if matches!(kind, GearKind::Ring | GearKind::Rack) {
            not(self.bore.is_some(), "bore")?;
            not(self.hub.is_some(), "hub")?;
        }
        if self.pair && (self.bore.is_some() || self.hub.is_some()) {
            return Err(
                "a bevel pair takes no bore or hub (make the two gears one by one)".to_owned(),
            );
        }
        if self.keyway && !self.plain_bore() {
            return Err("a keyway needs a plain bore".to_owned());
        }
        if let Some(seat) = &self.seat {
            seat.check_values()?;
        }
        if let Some(at) = &self.at {
            at.check_values()?;
        }
        if self.bore_clearance.is_some_and(|c| !c.is_finite()) {
            return Err(format!("the {name}'s bore clearance must be a number"));
        }
        // The teeth.
        match kind {
            GearKind::Rack => {
                let rack = self.rack();
                rack.tooth.check_rack()?;
                rack.check()?;
            }
            GearKind::Bevel => {
                let (bevel, mate, shaft) = self.bevel();
                if !(shaft > 0.0 && shaft < PI) {
                    return Err(format!(
                        "the shaft angle must be between 0 and 180 degrees, got {:.4} degrees",
                        shaft.to_degrees()
                    ));
                }
                if (mate - mate.round()).abs() > 1.0e-9 || mate < gear_profile::MIN_TEETH {
                    return Err(mitcad_model_base::internal::parameters::count_error(
                        format!(
                            "the mate's number of teeth must be whole and at least {}, got {mate}",
                            gear_profile::MIN_TEETH
                        ),
                        mate,
                    ));
                }
                bevel.check()?;
            }
            _ => self.tooth().check()?,
        }
        // Bores and hubs.
        let limit = self.inner_limit();
        let bore = self.bore_diameter()?;
        if let Some(d) = bore {
            positive(d, "bore diameter")?;
            let mut reach = d / 2.0;
            if self.keyway {
                let (_, depth) = gear_profile::keyway(d).ok_or_else(|| {
                    format!("DIN 6885 has no keyway for a bore of {d} mm (6 to 230 mm)")
                })?;
                reach += depth;
            }
            if reach >= limit {
                return Err(format!(
                    "the bore reaches the teeth: it must stay within {:.4} mm of the axis",
                    limit
                ));
            }
        }
        if let Some(hub) = &self.hub {
            let diameter = self.hub_diameter();
            positive(diameter, "hub diameter")?;
            positive(hub.length, "hub length")?;
            if bore.is_some_and(|d| d >= diameter) {
                return Err("the hub must be wider than the bore".to_owned());
            }
        }
        Ok(())
    }
}

impl Tooth {
    /// A rack's teeth: any count from one, the values as a gear's.
    fn check_rack(&self) -> Result<(), String> {
        Tooth {
            teeth: self.teeth.max(gear_profile::MIN_TEETH),
            ..*self
        }
        .check()
    }
}

// ---------------------------------------------------------------------------
// Placement

/// Where a gear lies: its frame (origin on the axis where the teeth start,
/// or a rack's start on its reference line, x along it and y towards its
/// tips) and the angle of tooth 1 in it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    pub frame: SketchFrame,
    pub phase: f64,
}

/// The values of two gears' mesh: the centre distance (a rack: from the
/// pinion's axis to the rack's reference line) and the radii a motion link
/// couples (the pitch radii; a rack's none).
pub fn mesh_values(new: &GearDef<f64>, mate: &GearDef<f64>) -> Result<(f64, f64, f64), String> {
    let (a, b) = (new.tooth(), mate.tooth());
    if (a.module - b.module).abs() > 1.0e-9 * a.module {
        return Err(format!(
            "gears of different modules do not mesh ({} and {} mm)",
            a.module, b.module
        ));
    }
    if (a.pressure_angle - b.pressure_angle).abs() > 1.0e-9 {
        return Err("gears of different pressure angles do not mesh".to_owned());
    }
    if (a.helix_angle.abs() - b.helix_angle.abs()).abs() > 1.0e-9 {
        return Err("gears of different helix angles do not mesh".to_owned());
    }
    let opposite = a.helix_angle * b.helix_angle < 0.0;
    let same = (a.helix_angle - b.helix_angle).abs() <= 1.0e-9;
    use GearKind::*;
    let distance = match (new.kind, mate.kind) {
        (n, m) if n.external() && m.external() => {
            if a.helix_angle != 0.0 && !opposite {
                return Err("helical gears in an external mesh need opposite hands".to_owned());
            }
            mesh_distance(&a, &b, false).1
        }
        (Ring, m) if m.external() => {
            if !same {
                return Err("a ring gear and its pinion need the same hand".to_owned());
            }
            mesh_distance(&b, &a, true).1
        }
        (n, Ring) if n.external() => {
            if !same {
                return Err("a ring gear and its pinion need the same hand".to_owned());
            }
            mesh_distance(&a, &b, true).1
        }
        (Rack, m) if m.external() => {
            if !same {
                return Err("a rack and its pinion need the same hand".to_owned());
            }
            b.pitch_radius() + b.shift * b.module
        }
        (n, Rack) if n.external() => {
            if !same {
                return Err("a rack and its pinion need the same hand".to_owned());
            }
            a.pitch_radius() + a.shift * a.module
        }
        (Bevel, _) | (_, Bevel) => {
            return Err("a bevel gear meshes with its mate through its pair option".to_owned());
        }
        (n, m) => {
            return Err(format!("a {} does not mesh with a {}", n.name(), m.name()));
        }
    };
    Ok((distance, a.pitch_radius(), b.pitch_radius()))
}

/// The gear's placement: its plane, or its mate's mesh.
pub(crate) fn placement<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    def: &GearDef,
    depth: usize,
) -> Result<(GearDef<f64>, Placement), String> {
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
    let mut gear = evaluated(def, &value);
    // Placed by picked geometry (mitcad#656): on a shaft its bore follows
    // the shaft; a mesh only turns it to engage.
    if let Some(at) = gear.at.clone() {
        let placed = ctx.place(&at)?;
        if let Some(seat) = placed.round() {
            gear.seat_bore(seat.diameter);
        }
        let frame = placed.frame;
        let phase = match gear.mesh_with.clone() {
            None => gear.angle.unwrap_or(0.0),
            Some(mesh) => {
                if depth > 64 {
                    return Err("the gears mesh with each other in a circle".to_owned());
                }
                let (mate, at) = mate_placement(ctx, mesh.gear, depth)?;
                mesh_phase(ctx, &gear, &mate, at, &frame)?
            }
        };
        return Ok((gear, Placement { frame, phase }));
    }
    let Some(mesh) = gear.mesh_with.clone() else {
        // On a shaft (mitcad#578): its bore follows the shaft.
        if let Some(seat) = gear.seat.clone() {
            let seat = ctx.seat(&seat).map_err(|e| format!("seat: {e}"))?;
            gear.seat_bore(seat.diameter);
            let phase = gear.angle.unwrap_or(0.0);
            let frame = seat.sketch_frame();
            return Ok((gear, Placement { frame, phase }));
        }
        let plane = def
            .plane
            .as_ref()
            .ok_or("a gear needs a plane, a shaft, or a gear to mesh with")?;
        let plane = ctx.plane(plane)?.frame();
        let turn = gear.angle.unwrap_or(0.0);
        let frame = SketchFrame {
            origin: plane.point(gear.center),
            ..plane
        };
        return Ok((
            gear,
            if def.kind == GearKind::Rack {
                // A rack turns about its start.
                let (s, c) = turn.sin_cos();
                Placement {
                    frame: SketchFrame {
                        origin: frame.origin,
                        x_axis: combine(frame.x_axis, c, frame.y_axis, s),
                        y_axis: combine(frame.x_axis, -s, frame.y_axis, c),
                    },
                    phase: 0.0,
                }
            } else {
                Placement { frame, phase: turn }
            },
        ));
    };
    if depth > 64 {
        return Err("the gears mesh with each other in a circle".to_owned());
    }
    let (mate, at) = mate_placement(ctx, mesh.gear, depth)?;
    let (distance, _, _) = mesh_values(&gear, &mate)?;
    // A picked direction (mitcad#656), or the angle.
    let theta = match &mesh.toward {
        Some(toward) => toward_angle(ctx, toward, &at.frame)?,
        None => mesh.angle.unwrap_or(0.0),
    };
    let frame = at.frame;
    let (s, c) = theta.sin_cos();
    let u = combine(frame.x_axis, c, frame.y_axis, s);
    let (new_teeth, mate_teeth) = (gear.teeth.round(), mate.teeth.round());
    // Where the pitch point lies from a gear's centre, as a fraction of its
    // pitch from tooth 1's middle (both counter-clockwise about the
    // normal).
    let fraction = |angle: f64, phase: f64, teeth: f64| (angle - phase) * teeth / TAU;
    use GearKind::*;
    let placed = match (gear.kind, mate.kind) {
        (Rack, _) => {
            // The rack along the mate's tangent, its tips towards it.
            let tooth = mate.tooth();
            let rack = gear.rack();
            let p = rack.tooth.pitch();
            let f = fraction(theta, at.phase, mate_teeth);
            let middle = rack.pitch_length() / 2.0;
            let s = p * (f + (middle / p - f).round());
            let d = combine(frame.x_axis, -theta.sin(), frame.y_axis, theta.cos());
            let datum = add_scaled(
                frame.origin,
                u,
                tooth.pitch_radius() + tooth.shift * tooth.module,
            );
            Placement {
                frame: SketchFrame {
                    origin: add_scaled(datum, d, -s),
                    x_axis: d,
                    y_axis: u.map(|v| -v),
                },
                phase: 0.0,
            }
        }
        (_, Rack) => {
            // Along the rack from its teeth's middle, on its tips' side.
            let rack = mate.rack();
            let p = rack.tooth.pitch();
            let x = rack.pitch_length() / 2.0 + mesh.position.unwrap_or(0.0);
            let tooth = gear.tooth();
            let origin = add_scaled(
                add_scaled(frame.origin, frame.x_axis, x),
                frame.y_axis,
                tooth.pitch_radius() + tooth.shift * tooth.module,
            );
            let f = x / p;
            Placement {
                frame: SketchFrame { origin, ..frame },
                phase: -PI / 2.0 - f * TAU / new_teeth,
            }
        }
        (Ring, _) => {
            let f = fraction(theta + PI, at.phase, mate_teeth) - 0.5;
            Placement {
                frame: SketchFrame {
                    origin: add_scaled(frame.origin, u, distance),
                    ..frame
                },
                phase: theta + PI - f * TAU / new_teeth,
            }
        }
        (_, Ring) => {
            let f = fraction(theta, at.phase, mate_teeth) + 0.5;
            Placement {
                frame: SketchFrame {
                    origin: add_scaled(frame.origin, u, distance),
                    ..frame
                },
                phase: theta - f * TAU / new_teeth,
            }
        }
        _ => {
            let f = fraction(theta, at.phase, mate_teeth);
            Placement {
                frame: SketchFrame {
                    origin: add_scaled(frame.origin, u, distance),
                    ..frame
                },
                phase: theta + PI - (0.5 - f) * TAU / new_teeth,
            }
        }
    };
    Ok((gear, placed))
}

/// The gear `uid` meshes with: its values and its placement (its read
/// recorded for the recompute cache).
fn mate_placement<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    uid: FeatureUid,
    depth: usize,
) -> Result<(GearDef<f64>, Placement), String> {
    let entry = ctx
        .env
        .features
        .iter()
        .find(|f| f.uid == uid)
        .cloned()
        .ok_or_else(|| format!("feature {uid} does not exist"))?;
    let FeatureDef::Gear(mate_def) = &entry.def else {
        return Err(format!("{} is not a gear", entry.name));
    };
    if entry.component != ctx.component() {
        return Err(format!(
            "{} is in another component: a gear meshes with a gear feature of its own component",
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
    placement(ctx, mate_def, depth + 1)
}

/// A length for a message: at most four decimals, no trailing zeros.
fn mm(value: f64) -> String {
    let text = format!("{value:.4}");
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// The turn of a gear placed on its own axis (mitcad#656) that engages its
/// mate's teeth: the direction between the axes takes the place of
/// `mesh_with.angle`, worked out in the mate's frame as a mesh placed by
/// its mate is, then given in the gear's own frame. The axes must be
/// parallel; a distance other than the mesh's warns.
fn mesh_phase<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    gear: &GearDef<f64>,
    mate: &GearDef<f64>,
    at: Placement,
    frame: &SketchFrame,
) -> Result<f64, String> {
    use GearKind::*;
    if gear.kind == Rack || mate.kind == Rack {
        return Err(
            "a gear on its own axis does not mesh with a rack: place it by mesh_with's position"
                .to_owned(),
        );
    }
    let (distance, _, _) = mesh_values(gear, mate)?;
    let m = at.frame;
    let n = m.normal();
    if norm(cross(n, frame.normal())) > 1.0e-6 {
        return Err("the gear's axis is not parallel to its mate's".to_owned());
    }
    let d = sub(frame.origin, m.origin);
    let across = sub(d, scale(n, dot(d, n)));
    let apart = norm(across);
    if apart < 1.0e-9 {
        return Err("the gear's axis is its mate's: they cannot mesh".to_owned());
    }
    if (apart - distance).abs() > 1.0e-3 {
        ctx.warn(format!(
            "the axes are {} mm apart, the mesh needs {} mm",
            mm(apart),
            mm(distance)
        ));
    }
    // Each gear is built from its placement along its own axis: on axes
    // that point opposite ways they grow apart (mitcad#749).
    if dot(n, frame.normal()) < 0.0 {
        ctx.warn(
            "the gear's axis points the opposite way from its mate's: the gears are built from their placements in opposite directions; flip one so that both point the same way",
        );
    }
    let theta = dot(across, m.y_axis).atan2(dot(across, m.x_axis));
    let (new_teeth, mate_teeth) = (gear.teeth.round(), mate.teeth.round());
    let fraction = |angle: f64, phase: f64, teeth: f64| (angle - phase) * teeth / TAU;
    let phase = match (gear.kind, mate.kind) {
        (Ring, _) => {
            let f = fraction(theta + PI, at.phase, mate_teeth) - 0.5;
            theta + PI - f * TAU / new_teeth
        }
        (_, Ring) => {
            let f = fraction(theta, at.phase, mate_teeth) + 0.5;
            theta - f * TAU / new_teeth
        }
        _ => {
            let f = fraction(theta, at.phase, mate_teeth);
            theta + PI - (0.5 - f) * TAU / new_teeth
        }
    };
    // From the mate's frame into the gear's own.
    let v = add(scale(m.x_axis, phase.cos()), scale(m.y_axis, phase.sin()));
    Ok(dot(v, frame.y_axis).atan2(dot(v, frame.x_axis)))
}

/// The direction (from the plane's x axis) from the frame's origin towards
/// a picked point, or towards where a picked axis meets the frame's plane
/// (`mesh_with.toward`, mitcad#656).
fn toward_angle<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    toward: &GeomRef,
    frame: &SketchFrame,
) -> Result<f64, String> {
    let n = frame.normal();
    let point = match ctx.point(toward) {
        Ok(point) => point,
        Err(_) => {
            let axis = ctx
                .axis(toward)
                .map_err(|e| format!("mesh_with.toward: {e}"))?;
            let along = dot(axis.direction, n);
            if along.abs() < 1.0e-9 {
                axis.origin
            } else {
                add(
                    axis.origin,
                    scale(
                        axis.direction,
                        dot(sub(frame.origin, axis.origin), n) / along,
                    ),
                )
            }
        }
    };
    let d = sub(point, frame.origin);
    let across = sub(d, scale(n, dot(d, n)));
    if norm(across) < 1.0e-9 {
        return Err(
            "mesh_with.toward lies on the gear's axis: pick a point or an axis beside it"
                .to_owned(),
        );
    }
    Ok(dot(across, frame.y_axis).atan2(dot(across, frame.x_axis)))
}

/// What `mesh_with.toward` may be: a point, or an axis (mitcad#656).
fn check_toward(toward: &GeomRef, ctx: &CheckContext<'_>) -> Result<(), String> {
    let result = match toward {
        GeomRef::Vertex { .. } | GeomRef::SketchPoint { .. } | GeomRef::FixedPoint(_) => {
            toward.check(ctx, Want::Point)
        }
        GeomRef::Origin(_) | GeomRef::Datum(_) | GeomRef::Mount { .. } => toward
            .check(ctx, Want::Point)
            .or_else(|_| toward.check(ctx, Want::Axis)),
        _ => toward.check(ctx, Want::Axis),
    };
    result.map_err(|e| format!("mesh_with.toward: {e}"))
}

impl GearDef {
    fn params_list(&self) -> Vec<ParamId> {
        let mut ids = Vec::new();
        let _ = self.map_params(&mut |_, id: &ParamId| {
            ids.push(*id);
            Ok::<_, ()>(())
        });
        ids
    }
}

// ---------------------------------------------------------------------------
// Bodies

/// The steps of a gear's teeth along its axis: the twist of a helical
/// gear (right-handed for a positive helix angle), the shift of a helical
/// rack.
fn helix_steps(gear: &GearDef<f64>, length: f64) -> GearStep {
    let tooth = gear.tooth();
    let tan = tooth.helix_angle.tan();
    if gear.kind == GearKind::Rack {
        GearStep {
            length,
            twist: 0.0,
            shift: length * tan,
        }
    } else {
        GearStep {
            length,
            twist: length * tan / tooth.pitch_radius(),
            shift: 0.0,
        }
    }
}

fn sweep<K: Kernel>(
    ctx: &EvalContext<'_, K>,
    frame: SketchFrame,
    loops: &[Vec<NamedSegment>],
    turn: f64,
    start: f64,
    steps: &[GearStep],
    names: (&str, &str),
) -> Result<K::Shape, String> {
    ctx.kernel
        .gear_sweep(&GearSweepSpec {
            feature: ctx.uid,
            frame,
            loops,
            turn,
            start,
            steps,
            start_name: names.0,
            end_name: names.1,
        })
        .map_err(|e| e.to_string())
}

fn unite<K: Kernel>(ctx: &EvalContext<'_, K>, shapes: &[&K::Shape]) -> Result<K::Shape, String> {
    ctx.kernel.unite(shapes).map_err(|e| e.to_string())
}

fn cut<K: Kernel>(
    ctx: &EvalContext<'_, K>,
    target: &K::Shape,
    tool: &K::Shape,
) -> Result<K::Shape, String> {
    let output = ctx
        .kernel
        .boolean(BooleanOp::Cut, &[target], tool)
        .map_err(|e| e.to_string())?;
    let mut pieces = output.pieces.into_iter();
    match (pieces.next(), pieces.next()) {
        (Some(piece), None) => Ok(piece.shape),
        (None, _) => Err("the bore removes the whole gear".to_owned()),
        (Some(_), Some(_)) => Err("the bore cuts the gear apart".to_owned()),
    }
}

/// The gear's solid (a bevel pair's two) in the component's coordinates.
fn gear_solid<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    gear: &GearDef<f64>,
    at: Placement,
) -> Result<K::Shape, String> {
    let tooth = gear.tooth();
    let frame = at.frame;
    let b = gear.face_width;
    // The teeth, and the extent of the body along the axis (for the bore).
    let (mut solid, low, high): (K::Shape, f64, f64) = match gear.kind {
        GearKind::Spur | GearKind::Helical => {
            let outline = gear_profile::external_outline(&tooth, at.phase)?;
            let step = helix_steps(gear, b);
            let solid = sweep(ctx, frame, &[outline], 0.0, 0.0, &[step], ("start", "end"))?;
            (solid, 0.0, b)
        }
        GearKind::Herringbone => {
            let outline = gear_profile::external_outline(&tooth, at.phase)?;
            let gap = gear.gap.unwrap_or(0.0);
            let half = (b - gap) / 2.0;
            let first = helix_steps(gear, half);
            let back = GearStep {
                twist: -first.twist,
                ..first
            };
            let loops = [outline];
            let solid = if gap == 0.0 {
                sweep(
                    ctx,
                    frame,
                    &loops,
                    0.0,
                    0.0,
                    &[first, back],
                    ("start", "end"),
                )?
            } else {
                let a = sweep(ctx, frame, &loops, 0.0, 0.0, &[first], ("start", "middle"))?;
                let c = sweep(
                    ctx,
                    frame,
                    &loops,
                    first.twist,
                    half + gap,
                    &[back],
                    ("middle", "end"),
                )?;
                // The gap: a groove a quarter module below the roots.
                let groove = circle("groove", tooth.root_radius() - tooth.module / 4.0);
                let g = sweep(
                    ctx,
                    frame,
                    &[groove],
                    0.0,
                    half,
                    &[GearStep {
                        length: gap,
                        twist: 0.0,
                        shift: 0.0,
                    }],
                    ("middle", "middle"),
                )?;
                unite(ctx, &[&a, &g, &c])?
            };
            (solid, 0.0, b)
        }
        GearKind::Ring => {
            let toothed = gear_profile::internal_outline(&tooth, at.phase)?;
            let rim = circle(
                "rim",
                tooth.internal_root_radius() + gear.rim.unwrap_or(0.0),
            );
            let step = helix_steps(gear, b);
            let solid = sweep(
                ctx,
                frame,
                &[rim, toothed],
                0.0,
                0.0,
                &[step],
                ("start", "end"),
            )?;
            (solid, 0.0, b)
        }
        GearKind::Rack => {
            let outline = gear_profile::rack_outline(&gear.rack())?;
            let step = helix_steps(gear, b);
            let solid = sweep(ctx, frame, &[outline], 0.0, 0.0, &[step], ("start", "end"))?;
            (solid, 0.0, b)
        }
        GearKind::Bevel => {
            let (bevel, mate_teeth, shaft) = gear.bevel();
            let section = bevel.section(at.phase)?;
            let solid = cone(ctx, frame, &section)?;
            let solid = if gear.pair {
                let mate = Bevel {
                    tooth: Tooth {
                        teeth: mate_teeth,
                        ..bevel.tooth
                    },
                    pitch_angle: shaft - bevel.pitch_angle,
                    face_width: bevel.face_width,
                };
                let mate_solid = bevel_mate(ctx, frame, &bevel, &section, &mate, shaft, at.phase)?;
                ctx.kernel
                    .compound(&[solid, mate_solid])
                    .map_err(|e| e.to_string())?
            } else {
                solid
            };
            let apex = section.apex;
            (solid, 0.0, apex)
        }
    };
    let mut low = low;
    let mut high = high;
    if let Some(hub) = &gear.hub {
        let (start, names) = if gear.kind == GearKind::Bevel {
            (-hub.length, ("hub_end", "hub_start"))
        } else {
            (b, ("hub_start", "hub_end"))
        };
        let tool = sweep(
            ctx,
            frame,
            &[circle("hub", gear.hub_diameter() / 2.0)],
            0.0,
            start,
            &[GearStep {
                length: hub.length,
                twist: 0.0,
                shift: 0.0,
            }],
            names,
        )?;
        solid = unite(ctx, &[&solid, &tool])?;
        low = low.min(start);
        high = high.max(start + hub.length);
    }
    if let Some(d) = gear.bore_diameter()? {
        let keyway = if gear.keyway {
            gear_profile::keyway(d).map(|(w, depth)| (gear.angle.unwrap_or(0.0), w, depth))
        } else {
            None
        };
        let loops = [bore_outline(d, keyway)];
        let tool = sweep(
            ctx,
            frame,
            &loops,
            0.0,
            low - 1.0,
            &[GearStep {
                length: high - low + 2.0,
                twist: 0.0,
                shift: 0.0,
            }],
            ("bore_start", "bore_end"),
        )?;
        solid = cut(ctx, &solid, &tool)?;
    }
    Ok(solid)
}

fn cone<K: Kernel>(
    ctx: &EvalContext<'_, K>,
    frame: SketchFrame,
    section: &gear_profile::BevelSection,
) -> Result<K::Shape, String> {
    ctx.kernel
        .gear_cone(&GearConeSpec {
            feature: ctx.uid,
            frame,
            loops: std::slice::from_ref(&section.outline),
            apex: section.apex,
            scale: section.scale,
            blank: &section.blank,
        })
        .map_err(|e| e.to_string())
}

/// A bevel gear's mate: its axis at the shaft angle through the apex,
/// towards the plane's x axis turned by the gear's angle, its teeth turned
/// to engage.
fn bevel_mate<K: Kernel>(
    ctx: &EvalContext<'_, K>,
    frame: SketchFrame,
    gear: &Bevel,
    section: &gear_profile::BevelSection,
    mate: &Bevel,
    shaft: f64,
    phase: f64,
) -> Result<K::Shape, String> {
    let mate_section = mate.section(0.0)?;
    let normal = frame.normal();
    let apex = add_scaled(frame.origin, normal, section.apex);
    // From the apex: the gear's axis towards its heel is -normal; the
    // contact along the plane's x axis.
    let u = frame.x_axis;
    let axis = combine(u, shaft.sin(), normal, -shaft.cos());
    let delta = gear.pitch_angle;
    let contact = combine(u, delta.sin(), normal, -delta.cos());
    let along = contact.iter().zip(&axis).map(|(a, b)| a * b).sum::<f64>();
    let x = add_scaled(contact, axis, -along);
    let length = (x[0] * x[0] + x[1] * x[1] + x[2] * x[2]).sqrt();
    let x = x.map(|v| v / length);
    // The mate's frame: its normal towards its apex (-axis), its origin on
    // its own section plane.
    let mate_normal = axis.map(|v| -v);
    let y = cross(mate_normal, x);
    let origin = add_scaled(apex, axis, mate_section.apex);
    let mate_frame = SketchFrame {
        origin,
        x_axis: x,
        y_axis: y,
    };
    // Tooth 1 of the gear is at `phase`; the contact at 0 in both frames.
    let f = (0.0 - phase) * gear.tooth.teeth.round() / TAU;
    let mate_phase = -(0.5 - f) * TAU / mate.tooth.teeth.round();
    let mate_section = mate.section(mate_phase)?;
    cone(ctx, mate_frame, &mate_section)
}

impl FeatureInfo for GearDef {
    fn references(&self) -> References {
        let mut references = References::default();
        if let Some(plane) = &self.plane {
            plane.add_to(&mut references);
        }
        if let Some(mesh) = &self.mesh_with {
            references.features.insert(mesh.gear);
            if let Some(toward) = &mesh.toward {
                toward.add_to(&mut references);
            }
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
        // Placed by picked geometry (mitcad#656): a mesh only turns it.
        if let Some(at) = &self.at {
            let name = self.kind.name();
            if self.kind == GearKind::Rack {
                return Err("a rack keeps its plane and center: it is not placed by at".to_owned());
            }
            if self.pair {
                return Err(
                    "a bevel pair is not placed by at (make the two gears one by one)".to_owned(),
                );
            }
            if self.plane.is_some() || !is_origin(&self.center) || self.seat.is_some() {
                return Err(format!(
                    "a {name} placed by at takes no plane, center or seat of its own"
                ));
            }
            if self.bore_clearance.is_some() && self.bore.is_some() {
                return Err("a bore of its own takes no bore clearance".to_owned());
            }
            if let Some(mesh) = &self.mesh_with {
                let entry = ctx.feature(mesh.gear)?;
                match &entry.def {
                    FeatureDef::Gear(mate) if mate.kind == GearKind::Rack => {
                        return Err(format!(
                            "{} is a rack: a gear meshing with a rack is placed by mesh_with's position",
                            entry.name
                        ));
                    }
                    FeatureDef::Gear(_) => {}
                    _ => return Err(format!("{} is not a gear", entry.name)),
                }
                if mesh.angle.is_some() || mesh.position.is_some() || mesh.toward.is_some() {
                    return Err(format!(
                        "a {name} placed by at meshes towards its mate's axis: mesh_with takes no angle, position or toward"
                    ));
                }
                if self.angle.is_some() {
                    return Err(format!(
                        "a {name} placed by at and meshed takes its turn from the mesh"
                    ));
                }
            }
            return at.check(ctx);
        }
        // On a shaft (mitcad#578): the kinds with a bore.
        if let Some(seat) = &self.seat {
            let name = self.kind.name();
            if matches!(self.kind, GearKind::Ring | GearKind::Rack) {
                return Err(format!(
                    "a {name} has no bore: it is placed by a plane, not on a shaft"
                ));
            }
            if self.pair {
                return Err(
                    "a bevel pair is not placed on a shaft (make the two gears one by one)"
                        .to_owned(),
                );
            }
            if self.plane.is_some() || !is_origin(&self.center) {
                return Err(format!(
                    "a {name} on a shaft takes its plane and center from the shaft"
                ));
            }
            if self.mesh_with.is_some() {
                return Err(format!(
                    "a {name} on a shaft is not placed by a mesh too (seat the first gear of the mesh)"
                ));
            }
            // An axis of its own has no diameter: the bore is the gear's own
            // (mitcad#656).
            if seat.body().is_some() && matches!(self.bore, Some(Bore::Plain { .. })) {
                return Err(format!(
                    "a {name} on a shaft takes its plain bore from the shaft: leave out the bore (bore_clearance widens it)"
                ));
            }
            if self.bore_clearance.is_some() && self.bore.is_some() {
                return Err("a threaded bore takes no bore clearance".to_owned());
            }
            return seat.check(ctx);
        }
        if self.bore_clearance.is_some() {
            return Err("a bore clearance needs a seat on a shaft".to_owned());
        }
        match (&self.plane, &self.mesh_with) {
            (None, None) => Err("a gear needs a plane, a shaft, or a gear to mesh with".to_owned()),
            (Some(_), Some(_)) => {
                Err("a gear meshed with another takes its plane from it".to_owned())
            }
            (Some(plane), None) => plane.check(ctx, Want::Plane),
            (None, Some(mesh)) => {
                let entry = ctx.feature(mesh.gear)?;
                if !matches!(entry.def, FeatureDef::Gear(_)) {
                    return Err(format!("{} is not a gear", entry.name));
                }
                if self.angle.is_some() {
                    return Err(
                        "a gear meshed with another takes its turn from the mesh".to_owned()
                    );
                }
                // A picked direction (mitcad#656).
                if let Some(toward) = &mesh.toward {
                    if mesh.angle.is_some() || mesh.position.is_some() {
                        return Err(
                            "mesh_with takes a direction by its angle or toward a pick, not both"
                                .to_owned(),
                        );
                    }
                    if let FeatureDef::Gear(mate) = &entry.def
                        && mate.kind == GearKind::Rack
                    {
                        return Err(
                            "a gear on a rack is placed by mesh_with's position, not toward a pick"
                                .to_owned(),
                        );
                    }
                    check_toward(toward, ctx)?;
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

impl<K: Kernel> Evaluate<K> for GearDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let participants = participant_bodies(ctx, self.operation, &self.participants)?;
        let (gear, at) = placement(ctx, self, 0)?;
        gear.check()?;
        let tool = gear_solid(ctx, &gear, at)?;
        let mut output =
            apply_operation(ctx, self.operation, participants, tool, gear.kind.name())?;
        // The gear's frame (`link_gears` reads its axes).
        output
            .mounts
            .insert(MOUNT.to_owned(), frame_transform(&at.frame));
        Ok(output)
    }
}

// ---------------------------------------------------------------------------
// Derived values

/// What the panel and the `gear_values` query show of a gear: mm and
/// radians, `null` where a value does not apply.
pub fn derived_values(gear: &GearDef<f64>) -> serde_json::Value {
    use serde_json::json;
    let tooth = gear.tooth();
    let mut values = json!({
        "kind": gear.kind,
        "module": tooth.module,
        "teeth": tooth.teeth,
        "transverse_module": tooth.transverse_module(),
        "pressure_angle": tooth.pressure_angle,
        "circular_pitch": tooth.pitch(),
    });
    let map = values.as_object_mut().expect("an object");
    // The bore: given, or a seat's (mitcad#578).
    if let Ok(Some(d)) = gear.bore_diameter() {
        map.insert("bore_diameter".into(), json!(d));
    }
    // The hub: given, or worked out (mitcad#656; `computed` lists it).
    if let Some(hub) = &gear.hub {
        map.insert("hub_diameter".into(), json!(gear.hub_diameter()));
        if hub.diameter.is_none() {
            map.insert("computed".into(), json!(["hub_diameter"]));
        }
    }
    match gear.kind {
        GearKind::Rack => {
            let rack = gear.rack();
            map.insert("pitch_line_length".into(), json!(rack.pitch_length()));
            map.insert("length".into(), json!(rack.length));
            map.insert(
                "tooth_thickness".into(),
                json!(tooth.pitch() / 2.0 - tooth.backlash / 2.0),
            );
        }
        GearKind::Ring => {
            map.insert("pitch_diameter".into(), json!(2.0 * tooth.pitch_radius()));
            map.insert("base_diameter".into(), json!(2.0 * tooth.base_radius()));
            map.insert(
                "inside_diameter".into(),
                json!(2.0 * tooth.internal_tip_radius()),
            );
            map.insert(
                "root_diameter".into(),
                json!(2.0 * tooth.internal_root_radius()),
            );
            map.insert(
                "outside_diameter".into(),
                json!(2.0 * (tooth.internal_root_radius() + gear.rim.unwrap_or(0.0))),
            );
            map.insert("space_width".into(), json!(tooth.space_width()));
        }
        GearKind::Bevel => {
            let (bevel, mate, shaft) = gear.bevel();
            let t = bevel.tooth;
            let delta = bevel.pitch_angle;
            map.insert("pitch_diameter".into(), json!(2.0 * t.pitch_radius()));
            map.insert(
                "outside_diameter".into(),
                json!(2.0 * (t.pitch_radius() + t.module * delta.cos())),
            );
            map.insert("pitch_angle".into(), json!(delta));
            map.insert("cone_distance".into(), json!(bevel.cone_distance()));
            map.insert("virtual_teeth".into(), json!(t.teeth / delta.cos()));
            map.insert("mate_teeth".into(), json!(mate));
            map.insert("mate_pitch_angle".into(), json!(shaft - delta));
            map.insert("shaft_angle".into(), json!(shaft));
        }
        _ => {
            map.insert("pitch_diameter".into(), json!(2.0 * tooth.pitch_radius()));
            map.insert("base_diameter".into(), json!(2.0 * tooth.base_radius()));
            map.insert("outside_diameter".into(), json!(2.0 * tooth.tip_radius()));
            map.insert("root_diameter".into(), json!(2.0 * tooth.root_radius()));
            map.insert("tooth_thickness".into(), json!(tooth.thickness()));
            if let Ok(half) = gear_profile::external_half(&tooth) {
                map.insert("undercut".into(), json!(half.undercut));
                map.insert("form_diameter".into(), json!(2.0 * half.form_radius));
            }
            if tooth.helix_angle != 0.0 {
                map.insert("helix_angle".into(), json!(tooth.helix_angle.abs()));
                map.insert("lead".into(), json!(tooth.lead()));
            }
        }
    }
    // The transverse section's area (the teeth's, without bore and hub):
    // the volume per length of a straight or twisted gear.
    let area = match gear.kind {
        GearKind::Rack => gear_profile::rack_outline(&gear.rack())
            .ok()
            .map(|o| gear_profile::loop_area(&o)),
        GearKind::Ring => gear_profile::internal_outline(&tooth, 0.0).ok().map(|o| {
            let outside = tooth.internal_root_radius() + gear.rim.unwrap_or(0.0);
            PI * outside * outside - gear_profile::loop_area(&o)
        }),
        GearKind::Bevel => None,
        _ => gear_profile::external_outline(&tooth, 0.0)
            .ok()
            .map(|o| gear_profile::loop_area(&o)),
    };
    if let Some(area) = area {
        map.insert("section_area".into(), json!(area));
    }
    values
}

/// The mount frame a gear's result keeps: its placement's frame (x, y and
/// the axis as the columns, the origin as the translation), a rack's x
/// along it.
pub const MOUNT: &str = "gear";

fn frame_transform(frame: &SketchFrame) -> crate::transform::Transform {
    let n = frame.normal();
    crate::transform::Transform {
        linear: std::array::from_fn(|r| [frame.x_axis[r], frame.y_axis[r], n[r]]),
        translation: frame.origin,
    }
}
