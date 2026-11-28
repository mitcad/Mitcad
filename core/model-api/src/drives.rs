// SPDX-License-Identifier: MIT
//! Placement by picked geometry, Gear Pair and Belt Drive (mitcad#656):
//! the `place_frame`, `gear_pair`, `belt_drive` and `belt_check` queries
//! and the `add_gear_pair` and `add_belt_drive` commands. The arithmetic is
//! `features/drives.rs`, the placement `features/place_at.rs`. See
//! `commands.md`, "Placement by picked geometry".

use serde::Deserialize;
use serde_json::{Value, json};

use super::ApiError;
use super::dispatch::named_enum;
use super::prelude::*;
use mitcad_model::Document;
use mitcad_model::expr::Expr;
use mitcad_model::features::drives::{self, BeltDriveOptions, Catalogue, Fit, GearPairOptions};
use mitcad_model::features::gear::{GearDef, GearKind, MeshWith, ToothForm};
use mitcad_model::features::pulley::{self, PulleyDef};
use mitcad_model::features::pulley_profile::BeltProfile;
use mitcad_model::features::{FeatureDef, PlaceAt, Placed, Seat, ValueInput};
use mitcad_model::ids::FeatureUid;
use mitcad_model::kernel::Kernel;
use mitcad_model::parameters::{slot_unit, unit_for};
use mitcad_model::profile::SketchFrame;
use mitcad_vec3::{cross, dot};
use mitcad_vec3::{norm, scale, sub};

/// Where one part of a pair goes (mitcad#754): a placement as a feature's
/// `at` takes it, or `{"seat": …}`, a seat as a feature's `seat` takes it,
/// for a shaft in another component's occurrence (with its `link`).
#[derive(Debug)]
pub(super) enum PairPlace {
    At(Box<PlaceAt<ValueInput>>),
    Seat(Box<Seat<ValueInput>>),
}

/// The seat form of a [`PairPlace`].
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SeatPlace {
    seat: Seat<ValueInput>,
}

impl<'de> Deserialize<'de> for PairPlace {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        // By its `seat` key, so that each form reports its own errors.
        let value = Value::deserialize(deserializer)?;
        if value.get("seat").is_some() {
            SeatPlace::deserialize(value)
                .map(|place| PairPlace::Seat(Box::new(place.seat)))
                .map_err(D::Error::custom)
        } else {
            PlaceAt::<ValueInput>::deserialize(value)
                .map(|at| PairPlace::At(Box::new(at)))
                .map_err(D::Error::custom)
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PlaceFrameQuery {
    /// A placement as a feature's `at` takes it.
    at: PlaceAt<ValueInput>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct GearPairQuery {
    first: PairPlace,
    second: PairPlace,
    /// z2 / z1.
    ratio: f64,
    #[serde(default)]
    kind: Option<GearKind>,
    #[serde(default)]
    pressure_angle: Option<f64>,
    #[serde(default)]
    modules: Option<[f64; 2]>,
    #[serde(default)]
    second_series: bool,
    #[serde(default)]
    min_teeth: Option<f64>,
    #[serde(default)]
    max_teeth: Option<f64>,
    #[serde(default)]
    ratio_tolerance: Option<f64>,
    #[serde(default)]
    max_profile_shift: Option<f64>,
    #[serde(default)]
    max_helix_angle: Option<f64>,
    #[serde(default)]
    limit: Option<usize>,
    /// Cycloidal teeth (mitcad#2607); involute without it.
    #[serde(default)]
    tooth_form: ToothForm,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct BeltDriveQuery {
    first: PairPlace,
    second: PairPlace,
    /// z2 / z1.
    ratio: f64,
    profile: BeltProfile,
    #[serde(default)]
    belt_lengths: Option<Vec<f64>>,
    /// Whole teeth even where the parts library has belts.
    #[serde(default)]
    whole_teeth: bool,
    #[serde(default)]
    min_teeth_in_mesh: Option<f64>,
    #[serde(default)]
    min_teeth: Option<f64>,
    #[serde(default)]
    max_teeth: Option<f64>,
    #[serde(default)]
    ratio_tolerance: Option<f64>,
    #[serde(default)]
    belts: Option<usize>,
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct BeltCheckQuery {
    /// Two pulley features of one component.
    pulleys: [FeatureUid; 2],
    #[serde(default)]
    belt_lengths: Option<Vec<f64>>,
    #[serde(default)]
    whole_teeth: bool,
    #[serde(default)]
    belts: Option<usize>,
}

/// Two parts' definitions made in one undo step.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PairCommand<T> {
    first: T,
    second: T,
    /// The component (uid or name); the active one when left out.
    #[serde(default)]
    component: Option<String>,
    #[serde(default)]
    names: Option<[String; 2]>,
}

named_enum! {
    #[derive(Debug, Deserialize)]
    #[serde(tag = "cmd", deny_unknown_fields)]
    pub(crate) enum DriveCommand {
        "add_gear_pair" => AddGearPair(Box<PairCommand<GearDef<ValueInput>>>),
        "add_belt_drive" => AddBeltDrive(Box<PairCommand<PulleyDef<ValueInput>>>),
    }
}

/// The number for a candidate's fields: an option or its default.
fn or(value: Option<f64>, default: f64) -> f64 {
    value.unwrap_or(default)
}

/// The document's methods of this module used across the api, as an
/// extension trait (mitcad#777): the api can then live in a crate above
/// the model.
pub(super) trait DrivesOps<K: Kernel> {
    fn place_frame_json(&self, query: &PlaceFrameQuery) -> Result<Value, ApiError>;

    fn gear_pair_json(&self, query: &GearPairQuery) -> Result<Value, ApiError>;

    fn belt_drive_json(&self, query: &BeltDriveQuery) -> Result<Value, ApiError>;

    fn belt_check_json(&self, query: &BeltCheckQuery) -> Result<Value, ApiError>;

    /// The belt round two built pulleys of one component: its length at
    /// their axes' distance and the catalogue belts with their travel (the
    /// `belt_check` query, and a pulley's own `belt_check` in
    /// `pulley_values`).
    fn belt_check_value(
        &self,
        a: FeatureUid,
        b: FeatureUid,
        lengths: Option<&[f64]>,
        whole_teeth: bool,
        count: usize,
    ) -> Result<Value, ApiError>;

    fn run_drive_command(&mut self, command: DriveCommand) -> Result<Value, ApiError>;
}

impl<K: Kernel> DrivesOps<K> for Document<K> {
    fn place_frame_json(&self, query: &PlaceFrameQuery) -> Result<Value, ApiError> {
        Ok(self.placed(&query.at, "at")?.to_json())
    }

    fn gear_pair_json(&self, query: &GearPairQuery) -> Result<Value, ApiError> {
        let apart = self.axes_apart(&query.first, &query.second)?;
        let distance = apart.distance;
        let defaults = GearPairOptions::default();
        let (spur, helical) = match query.kind {
            None => (true, true),
            Some(GearKind::Spur) => (true, false),
            Some(GearKind::Helical) => (false, true),
            Some(other) => {
                return Err(ApiError(format!(
                    "a gear pair is spur or helical, not {}",
                    other.name()
                )));
            }
        };
        // Cycloidal pairs (mitcad#2607): their rolling circles shape the
        // teeth, not a pressure angle.
        let cycloid = query.tooth_form == ToothForm::Cycloid;
        if cycloid && query.pressure_angle.is_some() {
            return Err(ApiError(
                "a pair with cycloidal teeth has no pressure angle".to_owned(),
            ));
        }
        if cycloid && query.max_profile_shift.is_some() {
            return Err(ApiError(
                "a pair with cycloidal teeth has no profile shift".to_owned(),
            ));
        }
        let options = GearPairOptions {
            ratio: query.ratio,
            spur,
            helical,
            pressure_angle: or(query.pressure_angle, defaults.pressure_angle),
            modules: query
                .modules
                .map_or(defaults.modules, |[low, high]| (low, high)),
            second_series: query.second_series,
            min_teeth: or(query.min_teeth, defaults.min_teeth),
            max_teeth: or(query.max_teeth, defaults.max_teeth),
            ratio_tolerance: or(query.ratio_tolerance, defaults.ratio_tolerance),
            max_profile_shift: or(query.max_profile_shift, defaults.max_profile_shift),
            max_helix_angle: or(query.max_helix_angle, defaults.max_helix_angle),
            limit: query.limit.unwrap_or(defaults.limit),
            cycloid,
        };
        let pairs = drives::gear_pairs(distance, &options).map_err(ApiError)?;
        let candidates: Vec<Value> = pairs
            .iter()
            .map(|pair| {
                let kind = if pair.fit == Fit::Helix {
                    GearKind::Helical
                } else {
                    GearKind::Spur
                };
                let gear = |i: usize| {
                    let mut gear = json!({
                        "kind": kind,
                        "module": pair.module,
                        "teeth": pair.teeth[i],
                    });
                    if cycloid {
                        gear["tooth_form"] = json!(ToothForm::Cycloid);
                    } else {
                        gear["pressure_angle"] = json!(options.pressure_angle);
                    }
                    // A shift of a fit's own or one that frees an undercut.
                    if pair.profile_shift[i] != 0.0 {
                        gear["profile_shift"] = json!(pair.profile_shift[i]);
                    }
                    if pair.fit == Fit::Helix {
                        gear["helix_angle"] = json!(pair.helix_angle);
                        if i == 1 {
                            gear["left_handed"] = json!(true);
                        }
                    }
                    gear
                };
                let mut candidate = json!({
                    "module": pair.module,
                    "teeth": pair.teeth,
                    "ratio": pair.ratio,
                    "ratio_deviation": pair.ratio_deviation,
                    "fit": pair.fit,
                    "undercut": pair.undercut,
                    "corrected": pair.corrected,
                    "first": gear(0),
                    "second": gear(1),
                });
                if pair.profile_shift != [0.0, 0.0] {
                    candidate["profile_shift"] = json!(pair.profile_shift);
                }
                if pair.fit == Fit::Helix {
                    candidate["helix_angle"] = json!(pair.helix_angle);
                }
                candidate
            })
            .collect();
        let mut result = json!({
            "center_distance": distance,
            "axial_offset": apart.axial,
            "candidates": candidates,
        });
        if let Some(warning) = pair_warning(&apart, "gears", "mesh across their face width") {
            result["warning"] = json!(warning);
        }
        Ok(result)
    }

    fn belt_drive_json(&self, query: &BeltDriveQuery) -> Result<Value, ApiError> {
        let defaults = BeltDriveOptions::new(query.ratio, query.profile);
        // An absurd bound is refused rather than searched up to
        // (mitcad#748).
        let max_teeth = or(query.max_teeth, defaults.max_teeth);
        if !(max_teeth.is_finite() && max_teeth <= drives::MAX_TEETH) {
            return Err(ApiError(format!(
                "max_teeth must be at most {} teeth, got {max_teeth}",
                drives::MAX_TEETH
            )));
        }
        let apart = self.axes_apart(&query.first, &query.second)?;
        let distance = apart.distance;
        let options = BeltDriveOptions {
            catalogue: query.belt_lengths.clone(),
            whole_teeth: query.whole_teeth,
            min_teeth: query.min_teeth,
            max_teeth,
            min_teeth_in_mesh: or(query.min_teeth_in_mesh, defaults.min_teeth_in_mesh),
            ratio_tolerance: or(query.ratio_tolerance, defaults.ratio_tolerance),
            belts: query.belts.unwrap_or(defaults.belts),
            limit: query.limit.unwrap_or(defaults.limit),
            ..defaults
        };
        let drives = drives::belt_drives(distance, &options).map_err(ApiError)?;
        let candidates: Vec<Value> = drives
            .iter()
            .map(|drive| {
                let pulley = |i: usize| {
                    json!({"kind": "toothed", "profile": query.profile, "teeth": drive.teeth[i]})
                };
                json!({
                    "teeth": drive.teeth,
                    "ratio": drive.ratio,
                    "ratio_deviation": drive.ratio_deviation,
                    "pitch_diameters": drive.pitch_diameters,
                    "belt_length": drive.belt_length,
                    "teeth_in_mesh": drive.teeth_in_mesh,
                    "belts": belts_json(&drive.belts),
                    "travel": drive.belts[0].travel,
                    "fit": drive.belts[0].fit,
                    "first": pulley(0),
                    "second": pulley(1),
                })
            })
            .collect();
        let mut result = json!({
            "center_distance": distance,
            "axial_offset": apart.axial,
            "catalogue": options.catalogue().name(),
            "candidates": candidates,
        });
        if let Some(warning) = pair_warning(&apart, "pulleys", "run in one belt's plane") {
            result["warning"] = json!(warning);
        }
        Ok(result)
    }

    fn belt_check_json(&self, query: &BeltCheckQuery) -> Result<Value, ApiError> {
        let [a, b] = query.pulleys;
        self.belt_check_value(
            a,
            b,
            query.belt_lengths.as_deref(),
            query.whole_teeth,
            query.belts.unwrap_or(3),
        )
    }

    fn belt_check_value(
        &self,
        a: FeatureUid,
        b: FeatureUid,
        lengths: Option<&[f64]>,
        whole_teeth: bool,
        count: usize,
    ) -> Result<Value, ApiError> {
        if a == b {
            return Err(ApiError("a belt goes round two pulleys".to_owned()));
        }
        let (pa, pb) = (self.pulley_values_of(a)?, self.pulley_values_of(b)?);
        let components = [a, b].map(|uid| self.feature(uid).map(|f| f.component));
        if components[0] != components[1] {
            return Err(ApiError(
                "the pulleys are in different components: check a belt on pulleys of one component"
                    .to_owned(),
            ));
        }
        let (oa, na) = self.pulley_axis(a)?;
        let (ob, nb) = self.pulley_axis(b)?;
        if norm(cross(na, nb)) > 1.0e-6 {
            return Err(ApiError("the pulleys' axes are not parallel".to_owned()));
        }
        let d = sub(ob, oa);
        let along = dot(d, na);
        let distance = norm(sub(d, scale(na, along)));
        let drive = pulley::belt_drive(&pa, &pb, distance).map_err(ApiError)?;
        let (ra, rb) = (pa.belt_radius(), pb.belt_radius());
        let catalogue = Catalogue::of(lengths, pa.profile.or(pb.profile), whole_teeth);
        // Without a pitch only lengths given make belts.
        let belts = match (drive.pitch, catalogue) {
            (Some(pitch), catalogue) => {
                drives::belt_fits(ra, rb, distance, pitch, catalogue, count)
            }
            (None, catalogue @ Catalogue::Given(_)) => {
                drives::belt_fits(ra, rb, distance, f64::NAN, catalogue, count)
            }
            (None, _) => Vec::new(),
        };
        let mut result = json!({
            "center_distance": distance,
            "axial_offset": along,
            "belt_length": drive.belt_length,
            "wrap_angles": [drive.wrap.0, drive.wrap.1],
            "catalogue": catalogue.name(),
            "belts": belts_json(&belts),
        });
        if let Some(belt) = belts.first() {
            result["fit"] = json!(belt.fit);
        }
        if let Some(teeth) = drive.belt_teeth {
            result["belt_teeth"] = json!(teeth);
        }
        if let Some(teeth) = drive.teeth_in_mesh {
            result["teeth_in_mesh"] = json!(teeth);
        }
        let apart = Apart {
            distance,
            axial: along,
            opposite: dot(na, nb) < 0.0,
        };
        if let Some(warning) = pair_warning(&apart, "pulleys", "run in one belt's plane") {
            result["warning"] = json!(warning);
        }
        Ok(result)
    }

    fn run_drive_command(&mut self, command: DriveCommand) -> Result<Value, ApiError> {
        match command {
            DriveCommand::AddGearPair(pair) => {
                let PairCommand {
                    first,
                    mut second,
                    component,
                    names,
                } = *pair;
                if second.mesh_with.is_some() {
                    return Err(ApiError(
                        "the second gear of a pair meshes with the first: leave out its mesh_with"
                            .to_owned(),
                    ));
                }
                self.add_pair(
                    "Add Gear Pair",
                    FeatureDef::Gear(first),
                    move |uid| {
                        second.mesh_with = Some(MeshWith {
                            gear: uid,
                            angle: None,
                            position: None,
                            toward: None,
                        });
                        FeatureDef::Gear(second)
                    },
                    component,
                    names,
                )
            }
            DriveCommand::AddBeltDrive(pair) => {
                let PairCommand {
                    first,
                    second,
                    component,
                    names,
                } = *pair;
                self.add_pair(
                    "Add Belt Drive",
                    FeatureDef::Pulley(first),
                    move |_| FeatureDef::Pulley(second),
                    component,
                    names,
                )
            }
        }
    }
}

/// The document's methods used only in this module, as an
/// extension trait (mitcad#777): the api can then live in a crate above
/// the model.
trait DrivesLocal<K: Kernel> {
    /// A placement's values: numbers as they are (mm, radians), names and
    /// expressions evaluated in the slot's unit.
    fn place_values(&self, at: &PlaceAt<ValueInput>) -> Result<PlaceAt<f64>, ApiError>;

    /// A placement resolved at the timeline marker.
    fn placed(&self, at: &PlaceAt<ValueInput>, what: &str) -> Result<Placed, ApiError>;

    /// Where one part of a pair goes, at the timeline marker in the active
    /// component's coordinates: a placement's frame, or a seat's on a
    /// shaft of another component's occurrence (mitcad#754).
    fn pair_frame(&self, place: &PairPlace, what: &str) -> Result<SketchFrame, ApiError>;

    /// How two places' parallel axes lie (see [`Apart`]).
    fn axes_apart(&self, first: &PairPlace, second: &PairPlace) -> Result<Apart, ApiError>;

    /// A built pulley's axis: its mount frame `bore` (origin and axis).
    fn pulley_axis(&self, uid: FeatureUid) -> Result<([f64; 3], [f64; 3]), ApiError>;

    /// Adds two features in one undo step named `label`; the second's
    /// definition may name the first. A refused one leaves nothing of the
    /// pair and the undo and redo steps as they were (mitcad#758).
    fn add_pair(
        &mut self,
        label: &str,
        first: FeatureDef<ValueInput>,
        second: impl FnOnce(FeatureUid) -> FeatureDef<ValueInput>,
        component: Option<String>,
        names: Option<[String; 2]>,
    ) -> Result<Value, ApiError>;
}

impl<K: Kernel> DrivesLocal<K> for Document<K> {
    fn place_values(&self, at: &PlaceAt<ValueInput>) -> Result<PlaceAt<f64>, ApiError> {
        let params = self.parameters();
        let context = params.context();
        at.map_params("at.", &mut |slot, value| match value {
            ValueInput::Number(n) => Ok(*n),
            ValueInput::Name(text) => {
                let unit = unit_for(slot_unit(slot).dims(), &context)
                    .unwrap_or(mitcad_model::expr::Unit::NONE);
                let parsed = Expr::parse(text).map_err(|e| ApiError(format!("{slot}: {e}")))?;
                parsed
                    .eval_as(params.table(), &context, unit)
                    .map(|v| v.value)
                    .map_err(|e| ApiError(format!("{slot}: {e}")))
            }
        })
    }

    fn placed(&self, at: &PlaceAt<ValueInput>, what: &str) -> Result<Placed, ApiError> {
        let at = self.place_values(at)?;
        at.check_values()
            .map_err(|e| ApiError(format!("{what}: {e}")))?;
        self.place_frame(&at)
            .map_err(|e| ApiError(format!("{what}: {e}")))
    }

    fn pair_frame(&self, place: &PairPlace, what: &str) -> Result<SketchFrame, ApiError> {
        match place {
            PairPlace::At(at) => Ok(self.placed(at, what)?.frame),
            PairPlace::Seat(seat) => {
                let seat = self
                    .seat_values(seat)
                    .map_err(|e| ApiError(format!("{what}: {}", e.0)))?;
                seat.check_values()
                    .map_err(|e| ApiError(format!("{what}: {e}")))?;
                self.seat_frame(&seat, self.active_component())
                    .map(|frame| frame.sketch_frame())
                    .map_err(|e| ApiError(format!("{what}: seat: {e}")))
            }
        }
    }

    fn axes_apart(&self, first: &PairPlace, second: &PairPlace) -> Result<Apart, ApiError> {
        let a = self.pair_frame(first, "first")?;
        let b = self.pair_frame(second, "second")?;
        axes_distance(&a, &b)
    }

    fn pulley_axis(&self, uid: FeatureUid) -> Result<([f64; 3], [f64; 3]), ApiError> {
        let frame = self
            .last_result()
            .result(uid)
            .filter(|r| r.status.is_ok())
            .and_then(|r| r.output.as_ref())
            .and_then(|o| o.mounts.as_ref())
            .and_then(|m| m.get(pulley::MOUNT).copied())
            .ok_or_else(|| {
                ApiError(format!(
                    "{} is not built: it has no axis to check a belt on",
                    self.feature(uid)
                        .map_or(uid.to_string(), |f| f.name.clone())
                ))
            })?;
        Ok((
            frame.translation,
            std::array::from_fn(|r| frame.linear[r][2]),
        ))
    }

    fn add_pair(
        &mut self,
        label: &str,
        first: FeatureDef<ValueInput>,
        second: impl FnOnce(FeatureUid) -> FeatureDef<ValueInput>,
        component: Option<String>,
        names: Option<[String; 2]>,
    ) -> Result<Value, ApiError> {
        let component = component
            .as_deref()
            .map(|c| self.component_ref(c))
            .transpose()?;
        let (first_name, second_name) = match &names {
            Some([a, b]) => (Some(a.as_str()), Some(b.as_str())),
            None => (None, None),
        };
        let (a, b) =
            self.add_feature_pair(label, &first, second, [first_name, second_name], component)?;
        Ok(json!({"uids": [a.uid, b.uid], "names": [a.name, b.name]}))
    }
}

/// How two parts' parallel axes lie.
struct Apart {
    /// Between the axes.
    distance: f64,
    /// The second's origin along the first's axis.
    axial: f64,
    /// The second axis points the opposite way from the first
    /// (mitcad#749).
    opposite: bool,
}

/// How two frames' axes (parallel) lie.
fn axes_distance(a: &SketchFrame, b: &SketchFrame) -> Result<Apart, ApiError> {
    let n = a.normal();
    let m = b.normal();
    if norm(cross(n, m)) > 1.0e-6 {
        return Err(ApiError(
            "the axes are not parallel: a gear pair or a belt drive needs parallel axes".to_owned(),
        ));
    }
    let d = sub(b.origin, a.origin);
    let along = dot(d, n);
    let distance = norm(sub(d, scale(n, along)));
    if distance < 1.0e-9 {
        return Err(ApiError("the axes meet: pick two axes apart".to_owned()));
    }
    Ok(Apart {
        distance,
        axial: along,
        opposite: dot(n, m) < 0.0,
    })
}

/// The warning of a pair: the second axis pointing the opposite way
/// (mitcad#749: each part is built from its placement along its own axis,
/// so the offset along the axes would be measured from the wrong end and
/// parts growing apart would pass), else the second placement out of the
/// first's plane.
fn pair_warning(apart: &Apart, parts: &str, so_that: &str) -> Option<String> {
    if apart.opposite {
        return Some(format!(
            "the second axis points the opposite way from the first: the {parts} would be built from their placements in opposite directions; flip one, or pick its shaft nearer its other end, so that both point the same way"
        ));
    }
    axial_warning(apart.axial, parts, so_that)
}

/// A warning when the second placement is not in the first's plane
/// along their axes (`axial_offset` not 0).
fn axial_warning(axial: f64, parts: &str, so_that: &str) -> Option<String> {
    (axial.abs() >= 5.0e-4).then(|| {
        let text = format!("{axial:.3}");
        let text = text.trim_end_matches('0').trim_end_matches('.');
        format!(
            "the second placement lies {text} mm along the axis from the first: put the {parts} in one plane so that they {so_that}"
        )
    })
}

fn belts_json(belts: &[drives::BeltFit]) -> Value {
    belts
        .iter()
        .map(|belt| {
            let mut value = json!({
                "length": belt.length,
                "teeth": belt.teeth,
                "center_distance": belt.center_distance,
                "travel": belt.travel,
                "fit": belt.fit,
            });
            // A belt of the parts library: its part, its name and width.
            if let Some(part) = belt.library {
                value["library"] = json!(part.id);
                value["name"] = json!(part.name);
                value["width"] = json!(part.width);
            }
            value
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use crate::DocumentApi;
    use mitcad_model::Document;
    use mitcad_model::testing::MockKernel;

    fn query(doc: &Document<MockKernel>, value: Value) -> Result<Value, String> {
        doc.query(&value.to_string())
            .map(|text| serde_json::from_str(&text).unwrap())
            .map_err(|e| e.to_string())
    }

    fn command(doc: &mut Document<MockKernel>, value: Value) -> Value {
        serde_json::from_str(&doc.command(&value.to_string()).unwrap()).unwrap()
    }

    #[test]
    fn cycloid_gear_pairs_fit_exactly_or_by_a_helix() {
        let doc = Document::new(MockKernel::default());
        let result = query(
            &doc,
            json!({"query": "gear_pair", "ratio": 3, "tooth_form": "cycloid",
                   "first": {"face": "xy", "offset1": 0, "offset2": 0},
                   "second": {"face": "xy", "offset1": 40, "offset2": 0}}),
        )
        .unwrap();
        let candidates = result["candidates"].as_array().unwrap();
        assert!(!candidates.is_empty(), "{result}");
        // No profile shifts: the exact pairs, the fewest teeth first (16
        // and 48 of 1.25 mm), then helix fits.
        for c in candidates {
            assert_ne!(c["fit"], json!("profile_shift"), "{c}");
            assert!(c.get("profile_shift").is_none(), "{c}");
            assert_eq!(c["first"]["tooth_form"], json!("cycloid"), "{c}");
            assert!(c["first"].get("pressure_angle").is_none(), "{c}");
        }
        let first = &candidates[0];
        assert_eq!(first["fit"], json!("exact"), "{result}");
        assert_eq!(first["teeth"], json!([16.0, 48.0]), "{result}");
        assert_eq!(first["module"], json!(1.25), "{result}");
        // A pressure angle means nothing to them.
        let error = query(
            &doc,
            json!({"query": "gear_pair", "ratio": 3, "tooth_form": "cycloid", "pressure_angle": 0.3,
                   "first": {"axis": "z"},
                   "second": {"face": "xy", "offset1": 40, "offset2": 0}}),
        )
        .unwrap_err();
        assert!(error.contains("no pressure angle"), "{error}");
    }

    #[test]
    fn gear_pairs_on_two_origin_plane_points() {
        let doc = Document::new(MockKernel::default());
        // Two points on XY 40 mm apart along x: axes along z.
        let result = query(
            &doc,
            json!({"query": "gear_pair", "ratio": 3,
                   "first": {"face": "xy", "offset1": 0, "offset2": 0},
                   "second": {"face": "xy", "offset1": 40, "offset2": 0}}),
        )
        .unwrap();
        assert_eq!(result["center_distance"], json!(40.0), "{result}");
        assert_eq!(result["axial_offset"], json!(0.0), "{result}");
        assert!(result.get("warning").is_none(), "{result}");
        let candidates = result["candidates"].as_array().unwrap();
        // Pairs needing no correction against undercut first, the fewest
        // teeth first: 20 and 60 of 1 mm.
        let first = &candidates[0];
        assert_eq!(first["fit"], json!("exact"), "{result}");
        assert_eq!(first["module"], json!(1.0), "{result}");
        assert_eq!(first["teeth"], json!([20.0, 60.0]), "{result}");
        assert_eq!(first["undercut"], json!([false, false]), "{result}");
        assert_eq!(first["corrected"], json!(false), "{result}");
        assert!(first.get("profile_shift").is_none(), "{result}");
        assert_eq!(first["first"]["kind"], json!("spur"), "{result}");
        assert_eq!(first["second"]["teeth"], json!(60.0), "{result}");
        // 16 teeth undercut at 20 degrees: shifted out of it.
        let small = candidates
            .iter()
            .find(|c| c["teeth"] == json!([16.0, 48.0]))
            .expect("the 16 and 48 teeth pair");
        assert_eq!(small["fit"], json!("exact"), "{small}");
        assert_eq!(small["undercut"], json!([false, false]), "{small}");
        assert_eq!(small["corrected"], json!(true), "{small}");
        assert!(
            small["first"]["profile_shift"].as_f64().unwrap() > 0.0,
            "{small}"
        );
        assert!(
            small["second"]["profile_shift"].as_f64().unwrap() < 0.0,
            "{small}"
        );
        // Helical only: the second gear left-handed.
        let result = query(
            &doc,
            json!({"query": "gear_pair", "ratio": 2, "kind": "helical", "modules": [2, 2],
                   "first": {"axis": "z"},
                   "second": {"face": "xy", "offset1": 42, "offset2": 0}}),
        )
        .unwrap();
        let first = &result["candidates"][0];
        assert_eq!(first["fit"], json!("helix"), "{result}");
        assert_eq!(first["second"]["left_handed"], json!(true), "{result}");
        assert!(first["first"].get("left_handed").is_none(), "{result}");
        // Planes apart along the axes: a warning.
        let result = query(
            &doc,
            json!({"query": "gear_pair", "ratio": 3,
                   "first": {"axis": "z"},
                   "second": {"point": {"point": [40, 0, 5]}, "plane": "xy"}}),
        )
        .unwrap();
        assert_eq!(result["axial_offset"], json!(5.0), "{result}");
        let warning = result["warning"].as_str().unwrap();
        assert!(warning.contains("lies 5 mm along the axis"), "{warning}");
        // Crossed axes are refused.
        let error = query(
            &doc,
            json!({"query": "gear_pair", "ratio": 2,
                   "first": {"axis": "z"}, "second": {"axis": "x"}}),
        )
        .unwrap_err();
        assert!(error.contains("not parallel"), "{error}");
    }

    #[test]
    fn belt_drives_on_two_axes() {
        let doc = Document::new(MockKernel::default());
        let result = query(
            &doc,
            json!({"query": "belt_drive", "ratio": 2, "profile": "gt2_2mm",
                   "first": {"face": "xy", "offset1": 0, "offset2": 0},
                   "second": {"face": "xy", "offset1": 0, "offset2": 100}}),
        )
        .unwrap();
        assert_eq!(result["center_distance"], json!(100.0), "{result}");
        // The parts library's GT2 belts.
        assert_eq!(result["catalogue"], json!("library"), "{result}");
        let candidates = result["candidates"].as_array().unwrap();
        assert!(!candidates.is_empty(), "{result}");
        for candidate in candidates {
            assert!(candidate["belts"][0]["library"].is_string(), "{candidate}");
            assert!(candidate["belts"][0]["name"].is_string(), "{candidate}");
            assert_eq!(candidate["belts"][0]["width"], json!(6.0), "{candidate}");
            let fit = candidate["fit"].as_str().unwrap();
            assert!(["ok", "tensioner", "short"].contains(&fit), "{candidate}");
            assert!(
                candidate["teeth"][0].as_f64().unwrap() >= 16.0,
                "{candidate}"
            );
            assert!(
                candidate["teeth_in_mesh"].as_f64().unwrap() >= 6.0,
                "{candidate}"
            );
        }
        let step = |c: &Value| (c["travel"].as_f64().unwrap().abs() / 0.5).floor();
        assert!(candidates.windows(2).all(|w| step(&w[0]) <= step(&w[1])));
        assert_eq!(candidates[0]["first"]["profile"], json!("gt2_2mm"));
        let teeth = candidates[0]["teeth"].as_array().unwrap();
        assert_eq!(
            teeth[1].as_f64().unwrap(),
            2.0 * teeth[0].as_f64().unwrap(),
            "{result}"
        );
        // Whole teeth when asked.
        let result = query(
            &doc,
            json!({"query": "belt_drive", "ratio": 2, "profile": "gt2_2mm", "whole_teeth": true,
                   "first": {"face": "xy", "offset1": 0, "offset2": 0},
                   "second": {"face": "xy", "offset1": 0, "offset2": 100}}),
        )
        .unwrap();
        assert_eq!(result["catalogue"], json!("whole_teeth"), "{result}");
        assert_eq!(
            result["candidates"][0]["teeth"],
            json!([16.0, 32.0]),
            "{result}"
        );
    }

    /// An absurd `max_teeth` is refused with the bound (mitcad#748); a
    /// large one within it is searched only as far as the pulleys fit.
    #[test]
    fn a_belt_drive_refuses_an_absurd_max_teeth() {
        let doc = Document::new(MockKernel::default());
        let drive = |max_teeth: f64| {
            query(
                &doc,
                json!({"query": "belt_drive", "ratio": 2, "profile": "gt2_2mm",
                       "max_teeth": max_teeth,
                       "first": {"face": "xy", "offset1": 0, "offset2": 0},
                       "second": {"face": "xy", "offset1": 0, "offset2": 100}}),
            )
        };
        for absurd in [1.0e9, 1.0e16] {
            let error = drive(absurd).unwrap_err();
            assert!(error.contains("max_teeth must be at most 1000"), "{error}");
        }
        let result = drive(1000.0).unwrap();
        assert!(
            !result["candidates"].as_array().unwrap().is_empty(),
            "{result}"
        );
    }

    #[test]
    fn a_belt_is_checked_on_two_built_pulleys() {
        let mut doc = Document::new(MockKernel::default());
        let made = command(
            &mut doc,
            json!({"cmd": "add_belt_drive",
                   "first": {"kind": "toothed", "profile": "gt2_2mm", "teeth": 20,
                             "belt_width": 6, "operation": "new_body",
                             "at": {"face": "xy", "offset1": 0, "offset2": 0}},
                   "second": {"kind": "toothed", "profile": "gt2_2mm", "teeth": 40,
                              "belt_width": 6, "operation": "new_body",
                              "at": {"face": "xy", "offset1": 0, "offset2": 100}}}),
        );
        assert_eq!(doc.undo_depth(), 1, "{made}");
        let result = query(
            &doc,
            json!({"query": "belt_check", "pulleys": made["uids"].clone(), "whole_teeth": true}),
        )
        .unwrap();
        assert_eq!(result["center_distance"], json!(100.0), "{result}");
        assert_eq!(result["catalogue"], json!("whole_teeth"), "{result}");
        let belts = result["belts"].as_array().unwrap();
        assert_eq!(result["fit"], belts[0]["fit"], "{result}");
        assert!(!belts.is_empty(), "{result}");
        assert!(belts[0]["travel"].as_f64().unwrap().abs() < 1.0, "{result}");
        assert!(result["belt_teeth"].as_f64().unwrap() > 0.0, "{result}");
        // The library's belts by default.
        let result = query(
            &doc,
            json!({"query": "belt_check", "pulleys": made["uids"].clone()}),
        )
        .unwrap();
        assert_eq!(result["catalogue"], json!("library"), "{result}");
        assert!(result["belts"][0]["library"].is_string(), "{result}");

        // A third pulley keeps its belt with the first checked: the
        // pulley's values carry it, by uid through a rename.
        let first = made["uids"][0].clone();
        let kept = command(
            &mut doc,
            json!({"cmd": "add_feature",
                   "def": {"type": "pulley", "kind": "toothed", "profile": "gt2_2mm",
                           "teeth": 20, "belt_width": 6, "operation": "new_body",
                           "at": {"face": "xy", "offset1": 0, "offset2": -150},
                           "belt_check": {"pulley": first}}}),
        );
        command(
            &mut doc,
            json!({"cmd": "rename_feature", "uid": first, "name": "Driver"}),
        );
        let values = query(&doc, json!({"query": "pulley_values", "uid": kept["uid"]})).unwrap();
        let check = &values["belt_check"];
        assert_eq!(check["pulley"], first, "{values}");
        assert_eq!(check["center_distance"], json!(150.0), "{values}");
        assert!(check["belt_length"].as_f64().unwrap() > 300.0, "{values}");
        assert!(check.get("error").is_none(), "{values}");
        // Saved with the design.
        assert!(doc.to_json().contains("\"belt_check\""));
        // Its mate is not deleted under it.
        let error = doc
            .command(&json!({"cmd": "delete_feature", "uid": first}).to_string())
            .unwrap_err()
            .to_string();
        assert!(error.contains("used by"), "{error}");
    }

    #[test]
    fn flanges_and_hubs_are_worked_out_unless_given() {
        let doc = Document::new(MockKernel::default());
        let pulley = |extra: Value| {
            let mut def = json!({"kind": "toothed", "profile": "gt2_2mm", "teeth": 40,
                                 "belt_width": 6, "plane": "xy", "operation": "new_body",
                                 "flanges": {"sides": "both", "thickness": 1},
                                 "hub": {"length": 7},
                                 "bore": {"type": "plain", "diameter": 5}});
            for (k, v) in extra.as_object().unwrap() {
                def[k] = v.clone();
            }
            query(&doc, json!({"query": "pulley_values", "def": def})).unwrap()
        };
        // 40 teeth: the flanges the groove's 0.764 mm and 1 mm above the
        // outside circle all round, the hub 2 mm round the 5 mm bore.
        let values = pulley(json!({}));
        assert!(values.get("error").is_none(), "{values}");
        let outside = values["outside_diameter"].as_f64().unwrap();
        let flange = values["flange_diameter"].as_f64().unwrap();
        assert!(
            (flange - (outside + 2.0 * 1.764)).abs() < 1.0e-9,
            "{values}"
        );
        assert_eq!(values["hub_diameter"], json!(9.0), "{values}");
        assert_eq!(
            values["computed"],
            json!(["flange_diameter", "hub_diameter"]),
            "{values}"
        );
        // Set screws' tap drills (M3: about 2.46 mm) thicken the hub's wall.
        let values =
            pulley(json!({"set_screws": {"count": 2, "thread": {"designation": "M3x0.5"}}}));
        assert!(values.get("error").is_none(), "{values}");
        let hub = values["hub_diameter"].as_f64().unwrap();
        assert!(hub > 9.5 && hub < 10.0, "{values}");
        // Given values are kept and checked as before: 18 mm flanges are
        // too small for 40 teeth.
        let values = pulley(
            json!({"flanges": {"sides": "both", "diameter": 18, "thickness": 1},
                                   "hub": {"diameter": 16, "length": 7}}),
        );
        assert_eq!(values["hub_diameter"], json!(16.0), "{values}");
        assert!(values.get("computed").is_none(), "{values}");
        let error = values["error"].as_str().unwrap();
        assert!(error.contains("the flanges must be larger"), "{error}");
        // A gear's hub likewise.
        let values = query(
            &doc,
            json!({"query": "gear_values",
                   "def": {"kind": "spur", "module": 2, "teeth": 20, "face_width": 10,
                           "plane": "xy", "operation": "new_body", "hub": {"length": 5},
                           "bore": {"type": "plain", "diameter": 8}}}),
        )
        .unwrap();
        assert!(values.get("error").is_none(), "{values}");
        assert_eq!(values["hub_diameter"], json!(12.0), "{values}");
        assert_eq!(values["computed"], json!(["hub_diameter"]), "{values}");
    }

    /// A bearing's element gap, count and diameter are automatic only when
    /// left out: 0 is refused with the reason (mitcad#656).
    #[test]
    fn a_bearing_takes_no_zero_for_automatic() {
        let doc = Document::new(MockKernel::default());
        let error = |field: &str| {
            let mut def = json!({"size": "608", "plane": "xy", "operation": "new_body"});
            def[field] = json!(0);
            let values = query(&doc, json!({"query": "bearing_values", "def": def})).unwrap();
            values["error"].as_str().unwrap_or_default().to_owned()
        };
        assert!(
            error("element_gap").contains("below"),
            "{}",
            error("element_gap")
        );
        assert!(
            error("element_count").contains("at least"),
            "{}",
            error("element_count")
        );
        assert!(
            error("element_diameter").contains("must be greater than zero"),
            "{}",
            error("element_diameter")
        );
        // Left out: the bearing's own choice.
        let values = query(
            &doc,
            json!({"query": "bearing_values",
                   "def": {"size": "608", "plane": "xy", "operation": "new_body"}}),
        )
        .unwrap();
        assert!(values.get("error").is_none(), "{values}");
    }
}
