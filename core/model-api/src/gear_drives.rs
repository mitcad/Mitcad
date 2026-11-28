// SPDX-License-Identifier: MIT
//! Gear Drive (mitcad#2608): a planetary set, a gear train, a rack and
//! pinion or a bevel pair made as one placed drive, in one undo step. The
//! drive is a component of its own (the frame its shafts sit in); each
//! moving member is a component of its own placed in it, its body made by
//! a `gear` feature (the carrier by a `cylinder`), turning on a revolute
//! joint (a rack sliding on a slider, a fixed member held by a rigid
//! joint), and motion links between the joints keep the teeth in mesh, so
//! that driving one joint turns the whole drive. The first gear keeps the
//! drive's definition and what it made (`GearDef::drive`), so that
//! `edit_gear_drive` makes the drive again from a changed definition and
//! `gear_drive` gives it back to a panel. See `commands.md`, "Gear Drive".

use std::f64::consts::{PI, TAU};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::ApiError;
use super::dispatch::named_enum;
use super::prelude::*;
use mitcad_model::features::gear::{self, GearDef};
use mitcad_model::features::{FeatureDef, ValueInput};
use mitcad_model::ids::{ComponentUid, FeatureUid, OccurrenceUid};
use mitcad_model::kernel::Kernel;
use mitcad_model::{Document, FeatureStatus};
use mitcad_vec3::{add_scaled, combine, cross, dot, norm, sub};

/// What kind of drive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum DriveKind {
    Planetary,
    GearTrain,
    RackPinion,
    BevelPair,
}

impl DriveKind {
    /// The name of the drive's component when none is given.
    fn component_name(self) -> &'static str {
        match self {
            Self::Planetary => "Planetary Drive",
            Self::GearTrain => "Gear Train",
            Self::RackPinion => "Rack and Pinion",
            Self::BevelPair => "Bevel Pair",
        }
    }

    /// The fields of the definition besides the common ones that this
    /// kind takes.
    fn takes(self) -> &'static [&'static str] {
        match self {
            Self::Planetary => &[
                "sun_teeth",
                "planet_teeth",
                "planets",
                "fixed",
                "sun_bore",
                "planet_bore",
                "rim",
                "carrier_thickness",
            ],
            Self::GearTrain => &["teeth", "compound", "stage_gap", "bore"],
            Self::RackPinion => &["pinion_teeth", "rack_teeth", "rack_height", "bore"],
            Self::BevelPair => &["teeth", "shaft_angle"],
        }
    }
}

/// The member of a planetary set held still.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Fixed {
    #[default]
    Ring,
    Sun,
    Carrier,
}

fn is_false(value: &bool) -> bool {
    !*value
}

/// A Gear Drive's definition: lengths in millimetres, angles in radians,
/// tooth counts whole numbers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DriveDef {
    #[serde(rename = "type")]
    kind: DriveKind,
    module: f64,
    /// 20 degrees without it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pressure_angle: Option<f64>,
    face_width: f64,
    /// Helical teeth (not a bevel pair's), the hands set so that the gears
    /// mesh; spur teeth without it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    helix_angle: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    backlash: Option<f64>,
    // A planetary set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    sun_teeth: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    planet_teeth: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    planets: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    fixed: Option<Fixed>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    sun_bore: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    planet_bore: Option<f64>,
    /// The ring's rim outside its roots, three modules without it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    rim: Option<f64>,
    /// Twice the module, at least 3 mm, without it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    carrier_thickness: Option<f64>,
    // A gear train's gears in order (a bevel pair's two).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    teeth: Option<Vec<f64>>,
    /// The teeth in pairs, a stage each: the driven gear of a stage and the
    /// driving gear of the next on one shaft.
    #[serde(default, skip_serializing_if = "is_false")]
    compound: bool,
    /// Between the stages of a compound train along the shafts, the module
    /// without it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    stage_gap: Option<f64>,
    /// A plain bore through every gear of a train, or the pinion.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    bore: Option<f64>,
    // A rack and pinion.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pinion_teeth: Option<f64>,
    /// The pinion's teeth without it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    rack_teeth: Option<f64>,
    /// From its back to its tips, four modules without it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    rack_height: Option<f64>,
    // A bevel pair: 90 degrees without it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    shaft_angle: Option<f64>,
    /// A contact set of the drive's members.
    #[serde(default, skip_serializing_if = "is_false")]
    contact_set: bool,
}

/// The most of a count.
const MOST_PLANETS: f64 = 24.0;
const MOST_GEARS: f64 = 24.0;

/// A whole number of at least `least`; else the refusal `message`, a whole
/// message (so that it is translated as one).
fn whole(value: Option<f64>, least: f64, message: &str) -> Result<f64, String> {
    match value {
        Some(v) if v.is_finite() && v.fract() == 0.0 && v >= least => Ok(v),
        _ => Err(message.to_owned()),
    }
}

/// A length greater than zero, when given; else the refusal `message`.
fn positive(value: Option<f64>, message: &str) -> Result<(), String> {
    match value {
        Some(v) if !(v.is_finite() && v > 0.0) => Err(message.to_owned()),
        _ => Ok(()),
    }
}

impl DriveDef {
    fn helix(&self) -> f64 {
        self.helix_angle.unwrap_or(0.0)
    }

    /// The transverse module: the module over the helix angle's cosine.
    fn transverse(&self) -> f64 {
        self.module / self.helix().cos()
    }

    fn pitch_radius(&self, teeth: f64) -> f64 {
        self.transverse() * teeth / 2.0
    }

    /// An external gear's root diameter (dedendum 1.25 modules).
    fn root_diameter(&self, teeth: f64) -> f64 {
        self.transverse() * teeth - 2.5 * self.module
    }

    /// An external gear's tip diameter (addendum one module).
    fn tip_diameter(&self, teeth: f64) -> f64 {
        self.transverse() * teeth + 2.0 * self.module
    }

    /// `helical` with a helix angle, else `spur`.
    fn external_kind(&self) -> &'static str {
        if self.helix() > 0.0 {
            "helical"
        } else {
            "spur"
        }
    }

    fn rim(&self) -> f64 {
        self.rim.unwrap_or(3.0 * self.module)
    }

    fn carrier_thickness(&self) -> f64 {
        self.carrier_thickness
            .unwrap_or((2.0 * self.module).max(3.0))
    }

    /// Between the carrier and the gears: a quarter module, at least
    /// 0.5 mm.
    fn carrier_gap(&self) -> f64 {
        (0.25 * self.module).max(0.5)
    }

    fn stage_gap(&self) -> f64 {
        self.stage_gap.unwrap_or(self.module)
    }

    fn rack_teeth(&self) -> f64 {
        self.rack_teeth.or(self.pinion_teeth).unwrap_or(0.0)
    }

    fn rack_height(&self) -> f64 {
        self.rack_height.unwrap_or(4.0 * self.module)
    }

    fn shaft_angle(&self) -> f64 {
        self.shaft_angle.unwrap_or(PI / 2.0)
    }

    /// The fields of a kind that are given.
    fn given(&self) -> Vec<&'static str> {
        [
            ("sun_teeth", self.sun_teeth.is_some()),
            ("planet_teeth", self.planet_teeth.is_some()),
            ("planets", self.planets.is_some()),
            ("fixed", self.fixed.is_some()),
            ("sun_bore", self.sun_bore.is_some()),
            ("planet_bore", self.planet_bore.is_some()),
            ("rim", self.rim.is_some()),
            ("carrier_thickness", self.carrier_thickness.is_some()),
            ("teeth", self.teeth.is_some()),
            ("compound", self.compound),
            ("stage_gap", self.stage_gap.is_some()),
            ("bore", self.bore.is_some()),
            ("pinion_teeth", self.pinion_teeth.is_some()),
            ("rack_teeth", self.rack_teeth.is_some()),
            ("rack_height", self.rack_height.is_some()),
            ("shaft_angle", self.shaft_angle.is_some()),
        ]
        .into_iter()
        .filter(|(_, given)| *given)
        .map(|(name, _)| name)
        .collect()
    }

    /// Refuses in words a definition that makes no drive: values out of
    /// range, and a set that cannot be assembled.
    fn check(&self) -> Result<(), String> {
        let degrees = PI / 180.0;
        positive(Some(self.module), "the module must be greater than zero")?;
        positive(
            Some(self.face_width),
            "the face width must be greater than zero",
        )?;
        if let Some(angle) = self.pressure_angle
            && !(10.0 * degrees - 1.0e-12..=35.0 * degrees + 1.0e-12).contains(&angle)
        {
            return Err("the pressure angle must be between 10 and 35 degrees".to_owned());
        }
        if let Some(angle) = self.helix_angle
            && !(0.0..=45.0 * degrees + 1.0e-12).contains(&angle)
        {
            return Err("the helix angle must be between 0 and 45 degrees".to_owned());
        }
        if let Some(backlash) = self.backlash
            && !(backlash.is_finite() && backlash >= 0.0)
        {
            return Err("the backlash must not be negative".to_owned());
        }
        if let Some(name) = self
            .given()
            .into_iter()
            .find(|name| !self.kind.takes().contains(name))
        {
            return Err(format!("the drive's type takes no {name}"));
        }
        match self.kind {
            DriveKind::Planetary => self.check_planetary(),
            DriveKind::GearTrain => self.check_train(),
            DriveKind::RackPinion => self.check_rack(),
            DriveKind::BevelPair => self.check_bevel(),
        }
    }

    fn check_planetary(&self) -> Result<(), String> {
        let zs = whole(
            self.sun_teeth,
            5.0,
            "the sun's teeth must be a whole number of at least 5",
        )?;
        let zp = whole(
            self.planet_teeth,
            5.0,
            "the planets' teeth must be a whole number of at least 5",
        )?;
        let n = whole(
            self.planets,
            1.0,
            "the number of planets must be a whole number of at least 1",
        )?;
        if n > MOST_PLANETS {
            return Err(format!(
                "a planetary drive has at most {MOST_PLANETS} planets"
            ));
        }
        let zr = zs + 2.0 * zp;
        if (zs + zr) % n != 0.0 {
            return Err(
                "the planets cannot be spaced evenly: the sun's and the ring's teeth together \
                 must divide by the number of planets"
                    .to_owned(),
            );
        }
        let distance = self.pitch_radius(zs) + self.pitch_radius(zp);
        if n >= 2.0 && 2.0 * distance * (PI / n).sin() <= self.tip_diameter(zp) {
            return Err(
                "the planets touch each other: use fewer planets, or smaller planets on a larger sun"
                    .to_owned(),
            );
        }
        positive(self.sun_bore, "the sun's bore must be greater than zero")?;
        positive(
            self.planet_bore,
            "the planets' bore must be greater than zero",
        )?;
        if self
            .sun_bore
            .is_some_and(|bore| bore >= self.root_diameter(zs))
        {
            return Err("the sun's bore reaches its tooth roots: make it smaller".to_owned());
        }
        if self
            .planet_bore
            .is_some_and(|bore| bore >= self.root_diameter(zp))
        {
            return Err("the planets' bore reaches their tooth roots: make it smaller".to_owned());
        }
        positive(self.rim, "the ring's rim must be greater than zero")?;
        positive(
            self.carrier_thickness,
            "the carrier's thickness must be greater than zero",
        )?;
        Ok(())
    }

    fn check_train(&self) -> Result<(), String> {
        let teeth = self
            .teeth
            .as_deref()
            .ok_or("a gear train needs the teeth of its gears")?;
        if teeth.len() < 2 || teeth.len() as f64 > MOST_GEARS {
            return Err(format!("a gear train has from 2 to {MOST_GEARS} gears"));
        }
        for &z in teeth {
            whole(
                Some(z),
                5.0,
                "every gear's teeth must be a whole number of at least 5",
            )?;
        }
        if self.compound && teeth.len() % 2 != 0 {
            return Err(
                "a compound train's gears come in pairs, a driving and a driven gear per stage"
                    .to_owned(),
            );
        }
        if self.stage_gap.is_some() && !self.compound {
            return Err("only a compound train has gaps between its stages".to_owned());
        }
        positive(
            self.stage_gap,
            "the gap between the stages must be greater than zero",
        )?;
        positive(self.bore, "the bore must be greater than zero")?;
        let least = teeth.iter().copied().fold(f64::INFINITY, f64::min);
        if self
            .bore
            .is_some_and(|bore| bore >= self.root_diameter(least))
        {
            return Err(
                "the bore reaches the tooth roots of the smallest gear: make it smaller".to_owned(),
            );
        }
        Ok(())
    }

    fn check_rack(&self) -> Result<(), String> {
        let zp = whole(
            self.pinion_teeth,
            5.0,
            "the pinion's teeth must be a whole number of at least 5",
        )?;
        whole(
            Some(self.rack_teeth()),
            1.0,
            "the rack's teeth must be a whole number of at least 1",
        )?;
        if self.rack_height() <= 2.25 * self.module {
            return Err("the rack's height must be more than the depth of its teeth".to_owned());
        }
        positive(self.bore, "the bore must be greater than zero")?;
        if self.bore.is_some_and(|bore| bore >= self.root_diameter(zp)) {
            return Err("the pinion's bore reaches its tooth roots: make it smaller".to_owned());
        }
        Ok(())
    }

    fn check_bevel(&self) -> Result<(), String> {
        if self.helix_angle.is_some_and(|angle| angle != 0.0) {
            return Err("a bevel pair has straight teeth: it takes no helix angle".to_owned());
        }
        let teeth = self
            .teeth
            .as_deref()
            .ok_or("a bevel pair needs the teeth of its two gears")?;
        let [z1, z2] = teeth else {
            return Err("a bevel pair has two gears".to_owned());
        };
        whole(
            Some(*z1),
            5.0,
            "the first gear's teeth must be a whole number of at least 5",
        )?;
        whole(
            Some(*z2),
            5.0,
            "the second gear's teeth must be a whole number of at least 5",
        )?;
        let shaft = self.shaft_angle();
        if !(shaft > 0.0 && shaft < PI) {
            return Err("the shaft angle must be between 0 and 180 degrees".to_owned());
        }
        let (delta, _) = bevel_angles(*z1, *z2, shaft);
        let cone = self.module * z1 / 2.0 / delta.sin();
        if self.face_width >= cone / 2.0 {
            return Err(
                "the face width must be less than half the gears' cone distance".to_owned(),
            );
        }
        Ok(())
    }
}

/// A bevel pair's pitch angles: the first gear's and its mate's.
fn bevel_angles(z1: f64, z2: f64, shaft: f64) -> (f64, f64) {
    let delta = shaft.sin().atan2(z2 / z1 + shaft.cos());
    (delta, shaft - delta)
}

/// What the panel shows of a drive (the `gear_drive_values` query): its
/// derived values, or `error` with the reason it makes no drive.
fn values(d: &DriveDef) -> Value {
    let mut result = json!({"type": d.kind});
    if let Err(error) = d.check() {
        result["error"] = json!(error);
        return result;
    }
    match d.kind {
        DriveKind::Planetary => {
            let (zs, zp) = (d.sun_teeth.unwrap_or(0.0), d.planet_teeth.unwrap_or(0.0));
            let zr = zs + 2.0 * zp;
            let fixed = d.fixed.unwrap_or_default();
            let (input, output, ratio) = match fixed {
                Fixed::Ring => ("sun", "carrier", 1.0 + zr / zs),
                Fixed::Sun => ("ring", "carrier", 1.0 + zs / zr),
                Fixed::Carrier => ("sun", "ring", -zr / zs),
            };
            result["ring_teeth"] = json!(zr);
            result["centre_distance"] = json!(d.pitch_radius(zs) + d.pitch_radius(zp));
            result["ratios"] = json!({
                "ring_fixed": 1.0 + zr / zs,
                "sun_fixed": 1.0 + zs / zr,
                "carrier_fixed": -zr / zs,
            });
            result["fixed"] = json!(fixed);
            result["input"] = json!(input);
            result["output"] = json!(output);
            result["ratio"] = json!(ratio);
            result["pitch_diameters"] = json!({
                "sun": 2.0 * d.pitch_radius(zs),
                "planet": 2.0 * d.pitch_radius(zp),
                "ring": 2.0 * d.pitch_radius(zr),
            });
            result["outside_diameter"] =
                json!(2.0 * (d.pitch_radius(zr) + 1.25 * d.module + d.rim()));
        }
        DriveKind::GearTrain => {
            let teeth = d.teeth.clone().unwrap_or_default();
            let pairs: Vec<(f64, f64)> = if d.compound {
                teeth.chunks(2).map(|p| (p[0], p[1])).collect()
            } else {
                teeth.windows(2).map(|p| (p[0], p[1])).collect()
            };
            let ratio = pairs.iter().fold(1.0, |r, (a, b)| -r * b / a);
            result["ratio"] = json!(ratio);
            result["centre_distances"] = json!(
                pairs
                    .iter()
                    .map(|(a, b)| d.pitch_radius(*a) + d.pitch_radius(*b))
                    .collect::<Vec<_>>()
            );
            result["shafts"] = json!(pairs.len() + 1);
            result["pitch_diameters"] = json!(
                teeth
                    .iter()
                    .map(|z| 2.0 * d.pitch_radius(*z))
                    .collect::<Vec<_>>()
            );
        }
        DriveKind::RackPinion => {
            let zp = d.pinion_teeth.unwrap_or(0.0);
            let radius = d.pitch_radius(zp);
            result["pitch_diameter"] = json!(2.0 * radius);
            result["travel_per_turn"] = json!(TAU * radius);
            result["rack_teeth"] = json!(d.rack_teeth());
            result["rack_length"] = json!(d.rack_teeth() * PI * d.transverse());
        }
        DriveKind::BevelPair => {
            let teeth = d.teeth.clone().unwrap_or_default();
            let (z1, z2) = (teeth[0], teeth[1]);
            let shaft = d.shaft_angle();
            let (delta, mate) = bevel_angles(z1, z2, shaft);
            result["ratio"] = json!(z2 / z1);
            result["pitch_angles"] = json!([delta, mate]);
            result["shaft_angle"] = json!(shaft);
            result["cone_distance"] = json!(d.module * z1 / 2.0 / delta.sin());
            result["pitch_diameters"] = json!([d.module * z1, d.module * z2]);
        }
    }
    result
}

// ---------------------------------------------------------------------------
// Commands and queries

/// `add_gear_drive`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AddDrive {
    drive: DriveDef,
    /// The drive's component (made free with ` (2)` when taken).
    #[serde(default)]
    name: Option<String>,
    /// Where the drive goes in the active component, as
    /// `create_component` takes it.
    #[serde(default)]
    transform: Option<Value>,
}

/// `edit_gear_drive`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct EditDrive {
    /// The drive's component, or one of its members (uid or name).
    component: String,
    drive: DriveDef,
}

named_enum! {
    #[derive(Debug, Deserialize)]
    #[serde(tag = "cmd", deny_unknown_fields)]
    pub(crate) enum GearDriveCommand {
        "add_gear_drive" => Add(Box<AddDrive>),
        "edit_gear_drive" => Edit(Box<EditDrive>),
    }
}

/// The `gear_drive_values` query.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ValuesQuery {
    drive: DriveDef,
}

/// The `gear_drive` query.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DriveQuery {
    component: String,
}

pub(super) fn run<K: Kernel>(
    doc: &mut Document<K>,
    command: GearDriveCommand,
) -> Result<Value, ApiError> {
    match command {
        GearDriveCommand::Add(c) => add(doc, *c),
        GearDriveCommand::Edit(c) => edit(doc, *c),
    }
}

pub(super) fn values_query(query: &ValuesQuery) -> Value {
    values(&query.drive)
}

/// The `gear_drive` query: the drive a component is (or is in), with its
/// definition, what it made and its values.
pub(super) fn drive_query<K: Kernel>(
    doc: &Document<K>,
    query: &DriveQuery,
) -> Result<Value, ApiError> {
    let component = doc.component_ref(&query.component)?;
    let found = find_drive(doc, component)?;
    let def: DriveDef = serde_json::from_value(found.record["def"].clone())
        .map_err(|e| ApiError(format!("the drive's definition: {e}")))?;
    let a = doc.assembly();
    Ok(json!({
        "component": found.frame.to_string(),
        "name": a.component(found.frame).map(|c| c.name.clone()),
        "occurrence": a.occurrences_of(found.frame).next().map(|o| o.uid.to_string()),
        "feature": found.lead.to_string(),
        "drive": def,
        "made": found.record["made"].clone(),
        "contact_set": found.record["contact_set"].clone(),
        "values": values(&def),
    }))
}

fn add<K: Kernel>(doc: &mut Document<K>, c: AddDrive) -> Result<Value, ApiError> {
    c.drive.check().map_err(ApiError)?;
    // Nothing of the drive stays when a step is refused (mitcad#2527).
    doc.all_or_nothing(|doc| {
        let depth = doc.undo_depth();
        let base = c
            .name
            .clone()
            .unwrap_or_else(|| c.drive.kind.component_name().to_owned());
        let mut create = json!({"cmd": "create_component", "name": free_name(doc, &base),
                                "activate": false});
        if let Some(transform) = &c.transform {
            create["transform"] = transform.clone();
        }
        let created = doc.run_command(create)?;
        let frame: ComponentUid = uid_of(&created["component"])?;
        let mut result = build(doc, frame, &c.drive)?;
        result["occurrence"] = created["occurrence"].clone();
        result["name"] = created["name"].clone();
        doc.merge_undo(depth, "Add Gear Drive");
        Ok(result)
    })
}

fn edit<K: Kernel>(doc: &mut Document<K>, c: EditDrive) -> Result<Value, ApiError> {
    c.drive.check().map_err(ApiError)?;
    let component = doc.component_ref(&c.component)?;
    let found = find_drive(doc, component)?;
    let made: Vec<FeatureUid> = serde_json::from_value(found.record["made"].clone())
        .map_err(|e| ApiError(format!("what the drive made: {e}")))?;
    // A copy of a drive keeps the original's record: it names features
    // that are not its own.
    if !made.contains(&found.lead) {
        return Err(ApiError(
            "this drive is a copy of another one and cannot be edited: make it again".to_owned(),
        ));
    }
    let a = doc.assembly();
    let in_drive = |entry: &mitcad_model::FeatureEntry| {
        entry.component == found.frame
            || a.occurrences_of(entry.component)
                .any(|o| o.parent == found.frame)
    };
    // The drive's own features, the latest first: links, joints, then the
    // members (a later gear meshes with an earlier one).
    let order: Vec<FeatureUid> = doc
        .features()
        .filter(|entry| made.contains(&entry.uid) && in_drive(entry))
        .map(|entry| entry.uid)
        .collect();
    let contact = found.record["contact_set"].as_str().map(str::to_owned);
    let frame = found.frame;
    doc.all_or_nothing(|doc| {
        let depth = doc.undo_depth();
        if let Some(name) = contact {
            // A set the user deleted is gone already.
            let _ = doc.run_command(json!({"cmd": "delete_contact_set", "name": name}));
        }
        for uid in order.into_iter().rev() {
            if doc.feature(uid).is_none() {
                continue;
            }
            doc.run_command(json!({"cmd": "delete_feature", "uid": uid.to_string()}))
                .map_err(|e| {
                    ApiError(format!(
                        "the drive cannot be made again while other features use its components: {}",
                        e.0
                    ))
                })?;
        }
        let mut result = build(doc, frame, &c.drive)?;
        let a = doc.assembly();
        result["occurrence"] = json!(a.occurrences_of(frame).next().map(|o| o.uid.to_string()));
        result["name"] = json!(a.component(frame).map(|def| def.name.clone()));
        doc.merge_undo(depth, "Edit Gear Drive");
        Ok(result)
    })
}

/// A drive found: its component, its first gear and the record that gear
/// keeps.
struct Found {
    frame: ComponentUid,
    lead: FeatureUid,
    record: Value,
}

/// The drive `component` is, or the drive it is placed in.
fn find_drive<K: Kernel>(doc: &Document<K>, component: ComponentUid) -> Result<Found, ApiError> {
    let a = doc.assembly();
    let mut at = Some(component);
    for _ in 0..64 {
        let Some(c) = at else {
            break;
        };
        for entry in doc.features() {
            if entry.component == c
                && let FeatureDef::Gear(def) = &entry.def
                && let Some(record) = &def.drive
            {
                return Ok(Found {
                    frame: c,
                    lead: entry.uid,
                    record: record.clone(),
                });
            }
        }
        if c == ComponentUid::ROOT {
            break;
        }
        at = a.occurrences_of(c).next().map(|o| o.parent);
    }
    Err(ApiError(
        "the component is not a gear drive, nor placed in one".to_owned(),
    ))
}

/// A component name not taken: `base`, else `base (2)`, `base (3)`, ….
fn free_name<K: Kernel>(doc: &Document<K>, base: &str) -> String {
    let a = doc.assembly();
    let taken = |name: &str| a.root_name == name || a.components.iter().any(|c| c.name == name);
    if !taken(base) {
        return base.to_owned();
    }
    (2..10_000)
        .map(|k| format!("{base} ({k})"))
        .find(|name| !taken(name))
        .unwrap_or_else(|| base.to_owned())
}

/// The uid a step of the drive answered with.
fn uid_of<T: std::str::FromStr>(value: &Value) -> Result<T, ApiError> {
    value
        .as_str()
        .and_then(|text| text.parse().ok())
        .ok_or_else(|| ApiError("a step of the drive made nothing".to_owned()))
}

/// A feature recomputed without failing; else its failure after its name.
fn ensure_ok<K: Kernel>(doc: &Document<K>, uid: FeatureUid) -> Result<(), ApiError> {
    let name = doc
        .feature(uid)
        .map_or_else(|| uid.to_string(), |entry| entry.name.clone());
    match doc.status(uid) {
        Some(FeatureStatus::Ok) => Ok(()),
        Some(FeatureStatus::Failed(error)) => {
            Err(ApiError(format!("{name}: {}", error.agent_text())))
        }
        _ => Err(ApiError(format!("{name} was not made"))),
    }
}

/// The path of uids from the root to an occurrence (`O1/O4`).
fn root_path<K: Kernel>(doc: &Document<K>, occurrence: OccurrenceUid) -> String {
    let a = doc.assembly();
    let mut path = vec![occurrence];
    let mut parent = a.occurrence(occurrence).map(|o| o.parent);
    for _ in 0..64 {
        let Some(p) = parent.filter(|p| *p != ComponentUid::ROOT) else {
            break;
        };
        let Some(up) = a.occurrences_of(p).next() else {
            break;
        };
        path.push(up.uid);
        parent = Some(up.parent);
    }
    path.iter()
        .rev()
        .map(|o| o.to_string())
        .collect::<Vec<_>>()
        .join("/")
}

/// A gear's frame where it is built (its mount `gear`), in its feature's
/// component: the origin on its axis where its teeth start (a rack's start
/// on its reference line), its x axis and its axis (z).
#[derive(Debug, Clone, Copy)]
struct Frame {
    origin: [f64; 3],
    x: [f64; 3],
    z: [f64; 3],
}

impl Frame {
    fn y(&self) -> [f64; 3] {
        cross(self.z, self.x)
    }
}

fn gear_frame<K: Kernel>(doc: &Document<K>, uid: FeatureUid) -> Result<Frame, ApiError> {
    let t = doc
        .last_result()
        .result(uid)
        .filter(|r| r.status.is_ok())
        .and_then(|r| r.output.as_ref())
        .and_then(|o| o.mounts.as_ref())
        .and_then(|m| m.get(gear::MOUNT).copied())
        .ok_or_else(|| ApiError("a gear of the drive was not made".to_owned()))?;
    let column = |c: usize| -> [f64; 3] { std::array::from_fn(|r| t.linear[r][c]) };
    Ok(Frame {
        origin: t.translation,
        x: column(0),
        z: column(2),
    })
}

/// A fixed plane: a joint's origin, a primitive's plane.
fn plane(origin: [f64; 3], normal: [f64; 3], x_axis: [f64; 3]) -> Value {
    json!({"origin": origin, "normal": normal, "x_axis": x_axis})
}

/// A joint's origin at a gear's frame.
fn at(frame: &Frame) -> Value {
    plane(frame.origin, frame.z, frame.x)
}

/// A gear's definition with the drive's tooth values, made as a component
/// of its own.
fn gear_def(d: &DriveDef, kind: &str, teeth: f64, left_handed: bool) -> Value {
    let mut def = json!({"type": "gear", "kind": kind, "module": d.module, "teeth": teeth,
                         "face_width": d.face_width, "operation": "new_component"});
    if let Some(angle) = d.pressure_angle {
        def["pressure_angle"] = json!(angle);
    }
    if d.helix() > 0.0 {
        def["helix_angle"] = json!(d.helix());
        if left_handed {
            def["left_handed"] = json!(true);
        }
    }
    if let Some(backlash) = d.backlash {
        def["backlash"] = json!(backlash);
    }
    def
}

fn put_bore(def: &mut Value, bore: Option<f64>) {
    if let Some(diameter) = bore {
        def["bore"] = json!({"type": "plain", "diameter": diameter});
    }
}

/// The gear coupling of two meshing gears' turns by their teeth.
fn mesh(ra: f64, rb: f64, internal: bool) -> Value {
    json!({"type": "gear", "ra": ra, "rb": rb, "internal": internal})
}

/// A member of the drive: the feature that made its body, its component
/// and its occurrence in the drive's component.
#[derive(Debug, Clone)]
struct Member {
    name: String,
    feature: FeatureUid,
    component: ComponentUid,
    occurrence: OccurrenceUid,
}

/// What a drive made, in timeline order.
#[derive(Default)]
struct Made {
    /// The first gear and its definition as added.
    lead: Option<(FeatureUid, Value)>,
    features: Vec<FeatureUid>,
    members: Vec<Member>,
    joints: Vec<(String, FeatureUid)>,
    links: Vec<(String, FeatureUid)>,
    /// The joint to drive: the input's.
    input: Option<FeatureUid>,
    contact_set: Option<String>,
}

struct Builder<'a, K: Kernel> {
    doc: &'a mut Document<K>,
    /// The drive's component.
    frame: ComponentUid,
    made: Made,
}

impl<K: Kernel> Builder<'_, K> {
    /// Adds a feature to `component`; refused when it fails.
    fn add(
        &mut self,
        def: Value,
        name: &str,
        component: ComponentUid,
    ) -> Result<FeatureUid, ApiError> {
        let added = self
            .doc
            .run_command(json!({"cmd": "add_feature", "def": def,
                                                "name": name,
                                                "component": component.to_string()}))?;
        let uid: FeatureUid = uid_of(&added["uid"])?;
        self.made.features.push(uid);
        ensure_ok(self.doc, uid)?;
        Ok(uid)
    }

    /// Adds a member made as a component of its own (a gear or the
    /// carrier) and names its component; the first gear leads.
    fn member(&mut self, def: Value, name: &str) -> Result<Member, ApiError> {
        let lead = self.made.lead.is_none() && def["type"] == "gear";
        let feature = self.add(def.clone(), name, self.frame)?;
        if lead {
            self.made.lead = Some((feature, def));
        }
        let component = self
            .doc
            .assembly()
            .created_by(feature)
            .ok_or_else(|| ApiError(format!("{name} was made in no component of its own")))?;
        self.placed(feature, component, name)
    }

    /// A member's component placed in the drive's, named `name`.
    fn placed(
        &mut self,
        feature: FeatureUid,
        component: ComponentUid,
        name: &str,
    ) -> Result<Member, ApiError> {
        let occurrence = self
            .doc
            .assembly()
            .occurrences_of(component)
            .find(|o| o.parent == self.frame)
            .map(|o| o.uid)
            .ok_or_else(|| {
                ApiError("the drive's component takes no components of its own".to_owned())
            })?;
        let name = free_name(self.doc, name);
        self.doc.run_command(json!({"cmd": "rename_component",
                                    "component": component.to_string(), "name": name}))?;
        let member = Member {
            name,
            feature,
            component,
            occurrence,
        };
        self.made.members.push(member.clone());
        Ok(member)
    }

    /// A joint of `a` on `b` (the drive's own geometry without it), both
    /// origins the plane `origin`; a slider slides along its x axis.
    fn joint(
        &mut self,
        name: &str,
        kind: &str,
        a: &Member,
        b: Option<&Member>,
        origin: Value,
    ) -> Result<FeatureUid, ApiError> {
        let mut side_b = json!({"geometry": origin.clone()});
        if let Some(b) = b {
            side_b["occurrence"] = json!(b.occurrence.to_string());
        }
        let mut command = json!({"cmd": "add_joint", "kind": kind,
                                 "a": {"occurrence": a.occurrence.to_string(), "geometry": origin},
                                 "b": side_b, "name": name,
                                 "component": self.frame.to_string()});
        if kind == "slider" {
            command["slide_axis"] = json!("x");
        }
        let added = self.doc.run_command(command)?;
        let uid: FeatureUid = uid_of(&added["uid"])?;
        self.made.features.push(uid);
        ensure_ok(self.doc, uid)?;
        self.made.joints.push((name.to_owned(), uid));
        Ok(uid)
    }

    /// A motion link: joint `b`'s motion follows joint `a`'s turn.
    fn link(
        &mut self,
        name: &str,
        a: FeatureUid,
        b: FeatureUid,
        b_motion: &str,
        coupling: Value,
        reverse: bool,
    ) -> Result<FeatureUid, ApiError> {
        let mut command = json!({"cmd": "add_motion_link",
                                 "a": {"joint": a.to_string(), "motion": "rz"},
                                 "b": {"joint": b.to_string(), "motion": b_motion},
                                 "coupling": coupling, "name": name,
                                 "component": self.frame.to_string()});
        if reverse {
            command["reverse"] = json!(true);
        }
        let added = self.doc.run_command(command)?;
        let uid: FeatureUid = uid_of(&added["uid"])?;
        self.made.features.push(uid);
        ensure_ok(self.doc, uid)?;
        self.made.links.push((name.to_owned(), uid));
        Ok(uid)
    }

    /// The contact set of the drive's members.
    fn contact_set(&mut self) -> Result<(), ApiError> {
        let paths: Vec<String> = self
            .made
            .members
            .iter()
            .map(|m| root_path(self.doc, m.occurrence))
            .collect();
        let added = self
            .doc
            .run_command(json!({"cmd": "add_contact_set", "occurrences": paths}))?;
        self.made.contact_set = added["name"].as_str().map(str::to_owned);
        Ok(())
    }

    /// Keeps the definition and what was made with the first gear.
    fn record(&mut self, d: &DriveDef) -> Result<(), ApiError> {
        let (lead, mut def) = self
            .made
            .lead
            .clone()
            .ok_or_else(|| ApiError("the drive made no gear".to_owned()))?;
        def["drive"] = json!({"def": d, "made": self.made.features,
                              "contact_set": self.made.contact_set});
        self.doc
            .run_command(json!({"cmd": "edit_feature", "uid": lead.to_string(), "def": def}))?;
        ensure_ok(self.doc, lead)
    }

    fn result(&self, d: &DriveDef) -> Value {
        let named = |list: &[(String, FeatureUid)]| -> Vec<Value> {
            list.iter()
                .map(|(name, uid)| json!({"name": name, "uid": uid.to_string()}))
                .collect()
        };
        json!({
            "component": self.frame.to_string(),
            "feature": self.made.lead.as_ref().map(|(uid, _)| uid.to_string()),
            "members": self.made.members.iter().map(|m| json!({
                "name": m.name, "component": m.component.to_string(),
                "occurrence": m.occurrence.to_string(), "feature": m.feature.to_string(),
            })).collect::<Vec<_>>(),
            "joints": named(&self.made.joints),
            "links": named(&self.made.links),
            "input": self.made.input.map(|uid| uid.to_string()),
            "contact_set": self.made.contact_set,
            "made": self.made.features,
            "values": values(d),
        })
    }
}

/// Makes the drive's members, joints and links in its component `frame`.
fn build<K: Kernel>(
    doc: &mut Document<K>,
    frame: ComponentUid,
    d: &DriveDef,
) -> Result<Value, ApiError> {
    let mut b = Builder {
        doc,
        frame,
        made: Made::default(),
    };
    match d.kind {
        DriveKind::Planetary => planetary(&mut b, d)?,
        DriveKind::GearTrain if d.compound => compound_train(&mut b, d)?,
        DriveKind::GearTrain => simple_train(&mut b, d)?,
        DriveKind::RackPinion => rack_pinion(&mut b, d)?,
        DriveKind::BevelPair => bevel_pair(&mut b, d)?,
    }
    if d.contact_set {
        b.contact_set()?;
    }
    b.record(d)?;
    Ok(b.result(d))
}

/// A planetary set: the sun at the drive's origin, the planets evenly
/// round it (planet k at k turns / n from the x axis) meshing with it, the
/// carrier a disc below the gears (with pins in the planets' bores when
/// they have bores), the ring meshing with the first planet. The fixed
/// member's joint is rigid; the links follow the carrier (the sun with
/// the carrier fixed):
///
/// - ring fixed: sun = (1 + zr/zs) carrier, planet = -(zr/zp) carrier;
/// - sun fixed: ring = (1 + zs/zr) carrier, planet = (zs/zp) carrier;
/// - carrier fixed: planet = -(zs/zp) sun, ring = (zp/zr) planet 1;
///
/// the planets' turns on the carrier.
fn planetary<K: Kernel>(b: &mut Builder<'_, K>, d: &DriveDef) -> Result<(), ApiError> {
    let (zs, zp) = (d.sun_teeth.unwrap_or(0.0), d.planet_teeth.unwrap_or(0.0));
    let n = d.planets.unwrap_or(1.0) as usize;
    let zr = zs + 2.0 * zp;
    let fixed = d.fixed.unwrap_or_default();
    let kind = d.external_kind();
    // Sun and planets of opposite hands, planets and ring of the same.
    let mut sun = gear_def(d, kind, zs, false);
    sun["plane"] = json!("xy");
    put_bore(&mut sun, d.sun_bore);
    let sun = b.member(sun, "Sun")?;
    let mut planets = Vec::with_capacity(n);
    for k in 0..n {
        let mut planet = gear_def(d, kind, zp, true);
        planet["mesh_with"] = json!({"gear": sun.feature.to_string(),
                                     "angle": TAU * k as f64 / n as f64});
        put_bore(&mut planet, d.planet_bore);
        planets.push(b.member(planet, &format!("Planet {}", k + 1))?);
    }
    let centre = gear_frame(b.doc, sun.feature)?;
    let planet_frames = planets
        .iter()
        .map(|p| gear_frame(b.doc, p.feature))
        .collect::<Result<Vec<_>, _>>()?;
    let distance = norm(sub(planet_frames[0].origin, centre.origin));
    // The carrier below the gears, reaching past the planets' axes.
    let gap = d.carrier_gap();
    let base = add_scaled(centre.origin, centre.z, -gap);
    let carrier_plane = plane(base, centre.z, centre.x);
    let reach = distance + d.root_diameter(zp) / 4.0;
    let carrier = b.member(
        json!({"type": "cylinder", "plane": carrier_plane, "center": [0.0, 0.0],
               "diameter": 2.0 * reach, "height": -d.carrier_thickness(),
               "operation": "new_component"}),
        "Carrier",
    )?;
    if let Some(bore) = d.planet_bore {
        let body = format!("{}.b0", carrier.feature);
        for (k, f) in planet_frames.iter().enumerate() {
            let r = sub(f.origin, base);
            let center = [dot(r, centre.x), dot(r, centre.y())];
            b.add(
                json!({"type": "cylinder", "plane": carrier_plane, "center": center,
                       "diameter": bore, "height": gap + d.face_width,
                       "operation": "join", "participants": [body]}),
                &format!("Pin {}", k + 1),
                carrier.component,
            )?;
        }
    }
    let mut ring = gear_def(d, "ring", zr, true);
    ring["rim"] = json!(d.rim());
    ring["mesh_with"] = json!({"gear": planets[0].feature.to_string(), "angle": PI});
    let ring = b.member(ring, "Ring")?;
    let ring_frame = gear_frame(b.doc, ring.feature)?;
    let turns = |member: Fixed| if member == fixed { "rigid" } else { "revolute" };
    let sun_joint = b.joint("Sun Joint", turns(Fixed::Sun), &sun, None, at(&centre))?;
    let carrier_joint = b.joint(
        "Carrier Joint",
        turns(Fixed::Carrier),
        &carrier,
        None,
        at(&centre),
    )?;
    let ring_joint = b.joint(
        "Ring Joint",
        turns(Fixed::Ring),
        &ring,
        None,
        at(&ring_frame),
    )?;
    let mut planet_joints = Vec::with_capacity(n);
    for (k, (planet, f)) in planets.iter().zip(&planet_frames).enumerate() {
        planet_joints.push(b.joint(
            &format!("Planet {} Joint", k + 1),
            "revolute",
            planet,
            Some(&carrier),
            at(f),
        )?);
    }
    match fixed {
        Fixed::Ring => {
            b.link(
                "Sun Link",
                carrier_joint,
                sun_joint,
                "rz",
                json!({"type": "ratio", "ratio": 1.0 + zr / zs}),
                false,
            )?;
            for (k, joint) in planet_joints.iter().enumerate() {
                // The ring turns back against the carrier.
                b.link(
                    &format!("Planet {} Mesh", k + 1),
                    carrier_joint,
                    *joint,
                    "rz",
                    mesh(zr, zp, true),
                    true,
                )?;
            }
            b.made.input = Some(sun_joint);
        }
        Fixed::Sun => {
            b.link(
                "Ring Link",
                carrier_joint,
                ring_joint,
                "rz",
                json!({"type": "ratio", "ratio": 1.0 + zs / zr}),
                false,
            )?;
            for (k, joint) in planet_joints.iter().enumerate() {
                // The sun turns back against the carrier.
                b.link(
                    &format!("Planet {} Mesh", k + 1),
                    carrier_joint,
                    *joint,
                    "rz",
                    mesh(zs, zp, false),
                    true,
                )?;
            }
            b.made.input = Some(ring_joint);
        }
        Fixed::Carrier => {
            for (k, joint) in planet_joints.iter().enumerate() {
                b.link(
                    &format!("Planet {} Mesh", k + 1),
                    sun_joint,
                    *joint,
                    "rz",
                    mesh(zs, zp, false),
                    false,
                )?;
            }
            b.link(
                "Ring Mesh",
                planet_joints[0],
                ring_joint,
                "rz",
                mesh(zp, zr, true),
                false,
            )?;
            b.made.input = Some(sun_joint);
        }
    }
    Ok(())
}

/// Gears in series in one plane along the x axis, each on a shaft of its
/// own, each meshing with the one before.
fn simple_train<K: Kernel>(b: &mut Builder<'_, K>, d: &DriveDef) -> Result<(), ApiError> {
    let teeth = d.teeth.clone().unwrap_or_default();
    let kind = d.external_kind();
    let mut gears: Vec<Member> = Vec::with_capacity(teeth.len());
    for (k, &z) in teeth.iter().enumerate() {
        let mut gear = gear_def(d, kind, z, k % 2 == 1);
        match gears.last() {
            None => gear["plane"] = json!("xy"),
            Some(before) => {
                gear["mesh_with"] = json!({"gear": before.feature.to_string(), "angle": 0.0});
            }
        }
        put_bore(&mut gear, d.bore);
        gears.push(b.member(gear, &format!("Gear {}", k + 1))?);
    }
    let mut joints = Vec::with_capacity(gears.len());
    for (k, gear) in gears.iter().enumerate() {
        let f = gear_frame(b.doc, gear.feature)?;
        joints.push(b.joint(
            &format!("Shaft {} Joint", k + 1),
            "revolute",
            gear,
            None,
            at(&f),
        )?);
    }
    for (k, pair) in joints.windows(2).enumerate() {
        b.link(
            &format!("Mesh {}", k + 1),
            pair[0],
            pair[1],
            "rz",
            mesh(teeth[k], teeth[k + 1], false),
            false,
        )?;
    }
    b.made.input = joints.first().copied();
    Ok(())
}

/// Stages of a driving and a driven gear, stacked along the shafts: stage
/// s at s times (face width + gap) along z, its driving gear on the shaft
/// of the stage before's driven gear (joined to it rigidly), its driven
/// gear on the next shaft along the x axis.
fn compound_train<K: Kernel>(b: &mut Builder<'_, K>, d: &DriveDef) -> Result<(), ApiError> {
    let teeth = d.teeth.clone().unwrap_or_default();
    let kind = d.external_kind();
    let step = d.face_width + d.stage_gap();
    // The shaft the next stage's driving gear goes on: its joint, the
    // driven gear on it and where it is.
    let mut shaft: Option<(FeatureUid, Member, [f64; 3])> = None;
    for (s, pair) in teeth.chunks(2).enumerate() {
        let (z1, z2) = (pair[0], pair[1]);
        let height = s as f64 * step;
        let mut driver = gear_def(d, kind, z1, false);
        match &shaft {
            None => driver["plane"] = json!("xy"),
            Some((_, _, axis)) => {
                driver["plane"] = plane([0.0, 0.0, height], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]);
                driver["center"] = json!([axis[0], axis[1]]);
            }
        }
        put_bore(&mut driver, d.bore);
        let driver = b.member(driver, &format!("Gear {}", 2 * s + 1))?;
        let mut driven = gear_def(d, kind, z2, true);
        driven["mesh_with"] = json!({"gear": driver.feature.to_string(), "angle": 0.0});
        put_bore(&mut driven, d.bore);
        let driven = b.member(driven, &format!("Gear {}", 2 * s + 2))?;
        let driver_frame = gear_frame(b.doc, driver.feature)?;
        let driven_frame = gear_frame(b.doc, driven.feature)?;
        let driver_joint = match &shaft {
            None => {
                let joint = b.joint(
                    "Shaft 1 Joint",
                    "revolute",
                    &driver,
                    None,
                    at(&driver_frame),
                )?;
                b.made.input = Some(joint);
                joint
            }
            Some((joint, on, _)) => {
                b.joint(
                    &format!("Gear {} on Shaft {}", 2 * s + 1, s + 1),
                    "rigid",
                    &driver,
                    Some(on),
                    at(&driver_frame),
                )?;
                *joint
            }
        };
        let driven_joint = b.joint(
            &format!("Shaft {} Joint", s + 2),
            "revolute",
            &driven,
            None,
            at(&driven_frame),
        )?;
        b.link(
            &format!("Stage {} Mesh", s + 1),
            driver_joint,
            driven_joint,
            "rz",
            mesh(z1, z2, false),
            false,
        )?;
        shaft = Some((driven_joint, driven, driven_frame.origin));
    }
    Ok(())
}

/// A rack along the x axis from the drive's origin, its teeth towards y,
/// the pinion meshing with the middle of its teeth. The rack slides along
/// its length by the pinion's pitch radius per radian.
fn rack_pinion<K: Kernel>(b: &mut Builder<'_, K>, d: &DriveDef) -> Result<(), ApiError> {
    let zp = d.pinion_teeth.unwrap_or(0.0);
    // A rack and its pinion of the same hand.
    let mut rack = gear_def(d, "rack", d.rack_teeth(), false);
    rack["plane"] = json!("xy");
    rack["height"] = json!(d.rack_height());
    let rack = b.member(rack, "Rack")?;
    let mut pinion = gear_def(d, d.external_kind(), zp, false);
    pinion["mesh_with"] = json!({"gear": rack.feature.to_string(), "position": 0.0});
    put_bore(&mut pinion, d.bore);
    let pinion = b.member(pinion, "Pinion")?;
    let rack_frame = gear_frame(b.doc, rack.feature)?;
    let pinion_frame = gear_frame(b.doc, pinion.feature)?;
    let rack_joint = b.joint("Rack Joint", "slider", &rack, None, at(&rack_frame))?;
    let pinion_joint = b.joint("Pinion Joint", "revolute", &pinion, None, at(&pinion_frame))?;
    b.link(
        "Rack Mesh",
        pinion_joint,
        rack_joint,
        "tx",
        json!({"type": "rack_pinion", "radius": d.pitch_radius(zp)}),
        false,
    )?;
    b.made.input = Some(pinion_joint);
    Ok(())
}

/// A bevel gear on the drive's xy plane (its back on it, its apex along
/// z) and its mate, made by one gear feature (`pair`) and each moved into
/// a component of its own. Both joints' axes point towards the apex, so
/// that the mate turns back: mate = -(z1/z2) gear.
fn bevel_pair<K: Kernel>(b: &mut Builder<'_, K>, d: &DriveDef) -> Result<(), ApiError> {
    let teeth = d.teeth.clone().unwrap_or_default();
    let (z1, z2) = (teeth[0], teeth[1]);
    let mut def = json!({"type": "gear", "kind": "bevel", "plane": "xy", "module": d.module,
                         "teeth": z1, "mate_teeth": z2, "face_width": d.face_width,
                         "pair": true, "operation": "new_body"});
    if let Some(angle) = d.pressure_angle {
        def["pressure_angle"] = json!(angle);
    }
    if let Some(angle) = d.shaft_angle {
        def["shaft_angle"] = json!(angle);
    }
    if let Some(backlash) = d.backlash {
        def["backlash"] = json!(backlash);
    }
    let apex = bevel_apex(&def)?;
    let frame = b.frame;
    let feature = b.add(def.clone(), "Bevel Pair", frame)?;
    b.made.lead = Some((feature, def));
    let split = b.doc.run_command(json!({"cmd": "components_from_bodies",
        "bodies": [format!("{feature}.b0"), format!("{feature}.b1")]}))?;
    let mut members = Vec::with_capacity(2);
    for (k, made) in split["components"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
    {
        let moved: FeatureUid = uid_of(&made["feature"])?;
        b.made.features.push(moved);
        let component: ComponentUid = uid_of(&made["component"])?;
        members.push(b.placed(moved, component, &format!("Bevel Gear {}", k + 1))?);
    }
    let [first, second] = members.as_slice() else {
        return Err(ApiError(
            "the bevel pair was not made as two gears".to_owned(),
        ));
    };
    let gear = gear_frame(b.doc, feature)?;
    let shaft = d.shaft_angle();
    // The mate's axis from the apex towards its heel is x sin + -z cos;
    // its joint's axis points back towards the apex.
    let mate_axis = combine(gear.z, shaft.cos(), gear.x, -shaft.sin());
    let apex_point = add_scaled(gear.origin, gear.z, apex);
    let first_joint = b.joint("Bevel Gear 1 Joint", "revolute", first, None, at(&gear))?;
    let second_joint = b.joint(
        "Bevel Gear 2 Joint",
        "revolute",
        second,
        None,
        plane(apex_point, mate_axis, gear.y()),
    )?;
    b.link(
        "Bevel Mesh",
        first_joint,
        second_joint,
        "rz",
        mesh(z1, z2, false),
        false,
    )?;
    b.made.input = Some(first_joint);
    Ok(())
}

/// The apex's height above a bevel gear's back, as the gear feature makes
/// it.
fn bevel_apex(def: &Value) -> Result<f64, ApiError> {
    let mut fields = def.clone();
    if let Some(object) = fields.as_object_mut() {
        object.remove("type");
    }
    let gear: GearDef<ValueInput> =
        serde_json::from_value(fields).map_err(|e| ApiError(format!("the bevel pair: {e}")))?;
    let gear: GearDef<f64> = gear.map_params(&mut |slot, value| match value {
        ValueInput::Number(n) => Ok(*n),
        ValueInput::Name(_) => Err(ApiError(format!("the bevel pair's {slot} is no number"))),
    })?;
    let (bevel, _, _) = gear.bevel();
    let section = bevel.section(0.0).map_err(ApiError)?;
    Ok(section.apex)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def(value: Value) -> DriveDef {
        serde_json::from_value(value).expect("a drive")
    }

    fn near(a: f64, b: f64) -> bool {
        (a - b).abs() < 1.0e-9
    }

    #[test]
    fn a_planetary_sets_its_ring_and_shows_its_ratios() {
        let v = values(&def(
            json!({"type": "planetary", "module": 1, "face_width": 8,
                                   "sun_teeth": 18, "planet_teeth": 30, "planets": 3}),
        ));
        assert!(v.get("error").is_none(), "{v}");
        assert_eq!(v["ring_teeth"], json!(78.0));
        assert!(near(v["centre_distance"].as_f64().unwrap(), 24.0));
        assert!(near(
            v["ratios"]["ring_fixed"].as_f64().unwrap(),
            1.0 + 78.0 / 18.0
        ));
        assert!(near(
            v["ratios"]["sun_fixed"].as_f64().unwrap(),
            1.0 + 18.0 / 78.0
        ));
        assert!(near(
            v["ratios"]["carrier_fixed"].as_f64().unwrap(),
            -78.0 / 18.0
        ));
        assert_eq!(v["input"], json!("sun"));
        assert_eq!(v["output"], json!("carrier"));
        assert!(near(v["ratio"].as_f64().unwrap(), 1.0 + 78.0 / 18.0));
    }

    #[test]
    fn a_planetary_that_cannot_be_assembled_is_refused_in_words() {
        let base = json!({"type": "planetary", "module": 1, "face_width": 8,
                          "sun_teeth": 18, "planet_teeth": 30, "planets": 3});
        let with = |key: &str, value: Value| {
            let mut v = base.clone();
            v[key] = value;
            values(&def(v))["error"]
                .as_str()
                .unwrap_or_default()
                .to_owned()
        };
        // 19 + 79 = 98 does not divide by 3.
        assert!(with("sun_teeth", json!(19)).contains("cannot be spaced evenly"));
        // Six planets of 30 teeth round a sun of 18 overlap.
        assert!(with("planets", json!(6)).contains("touch each other"));
        assert!(with("sun_bore", json!(16)).contains("sun's bore reaches"));
        assert!(with("planet_bore", json!(28)).contains("planets' bore reaches"));
        assert!(with("planets", json!(2.5)).contains("whole number"));
        assert!(with("teeth", json!([20, 30])).contains("takes no teeth"));
    }

    #[test]
    fn a_gear_train_gives_its_total_ratio() {
        let simple = values(&def(
            json!({"type": "gear_train", "module": 2, "face_width": 10,
                                        "teeth": [20, 40, 30]}),
        ));
        // Two meshes: the last gear turns as the first, 30 / 20 slower.
        assert!(near(simple["ratio"].as_f64().unwrap(), 1.5), "{simple}");
        assert_eq!(simple["shafts"], json!(3));
        let compound = values(&def(
            json!({"type": "gear_train", "module": 2, "face_width": 10,
                                          "teeth": [20, 40, 15, 45], "compound": true}),
        ));
        assert!(near(compound["ratio"].as_f64().unwrap(), 6.0), "{compound}");
        assert_eq!(compound["centre_distances"], json!([60.0, 60.0]));
        let odd = values(&def(
            json!({"type": "gear_train", "module": 2, "face_width": 10,
                                     "teeth": [20, 40, 15], "compound": true}),
        ));
        assert!(odd["error"].as_str().unwrap().contains("come in pairs"));
    }

    #[test]
    fn a_rack_and_a_bevel_pair_give_their_values() {
        let rack = values(&def(
            json!({"type": "rack_pinion", "module": 2, "face_width": 10,
                                      "pinion_teeth": 20}),
        ));
        assert!(
            near(rack["travel_per_turn"].as_f64().unwrap(), TAU * 20.0),
            "{rack}"
        );
        assert_eq!(rack["rack_teeth"], json!(20.0));
        let bevel = values(&def(
            json!({"type": "bevel_pair", "module": 2, "face_width": 8,
                                       "teeth": [20, 20]}),
        ));
        let angles = bevel["pitch_angles"].as_array().unwrap();
        assert!(near(angles[0].as_f64().unwrap(), PI / 4.0), "{bevel}");
        assert!(near(bevel["ratio"].as_f64().unwrap(), 1.0));
        let helical = values(&def(
            json!({"type": "bevel_pair", "module": 2, "face_width": 8,
                                         "teeth": [20, 20], "helix_angle": 0.3}),
        ));
        assert!(
            helical["error"]
                .as_str()
                .unwrap()
                .contains("straight teeth")
        );
    }
}
