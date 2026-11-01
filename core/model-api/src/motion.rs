// SPDX-License-Identifier: MIT
//! Commands, queries and previews of motion studies and contact sets
//! (mitcad#454, see `commands.md`, "Motion studies and contact sets").

use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;
use serde_json::{Map, Value, json};

use super::ApiError;
use super::dispatch::named_enum;
use super::prelude::*;
use mitcad_model::contact::{ContactBody, ContactHit};
use mitcad_model::features::{FeatureDef, OccurrencePath, ValueInput};
use mitcad_model::ids::FeatureUid;
use mitcad_model::internal::contact::ContactPairs;
use mitcad_model::joints::Motion;
use mitcad_model::kernel::Kernel;
use mitcad_model::motion::{ContactSet, DEFAULT_STEPS, MotionStudy, StudyKey};
use mitcad_model::{Document, JointValues, MotionOutcome};

/// A study's key in a command: the joint by uid or name, the motion and
/// `[step, value]` points (values: numbers in mm or radians, or
/// expressions such as `"30 deg"`).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct KeyInput {
    joint: String,
    motion: Motion,
    points: Vec<(f64, ValueInput)>,
}

named_enum! {
    /// The variants are the commands' names (`add_motion_study`, ...).
    #[derive(Debug, Deserialize)]
    #[serde(tag = "cmd", rename_all = "snake_case", deny_unknown_fields)]
    pub(crate) enum MotionCommand {
        "add_motion_study" => AddMotionStudy {
            #[serde(default)]
            name: Option<String>,
            #[serde(default)]
            steps: Option<u32>,
            #[serde(default)]
            keys: Vec<KeyInput>,
        },
        "edit_motion_study" => EditMotionStudy {
            name: String,
            #[serde(default)]
            new_name: Option<String>,
            #[serde(default)]
            steps: Option<u32>,
            #[serde(default)]
            keys: Option<Vec<KeyInput>>,
        },
        "delete_motion_study" => DeleteMotionStudy {
            name: String,
        },
        /// Add Key: a joint's motion at a step, its value where the joint is
        /// when none is given.
        "set_study_key" => SetStudyKey {
            study: String,
            joint: String,
            motion: Motion,
            step: f64,
            #[serde(default)]
            value: Option<ValueInput>,
        },
        "remove_study_key" => RemoveStudyKey {
            study: String,
            joint: String,
            motion: Motion,
            #[serde(default)]
            step: Option<f64>,
        },
        "play_study" => PlayStudy {
            study: String,
            step: f64,
        },
        "add_contact_set" => AddContactSet {
            #[serde(default)]
            name: Option<String>,
            occurrences: Vec<String>,
            #[serde(default)]
            suppressed: bool,
        },
        "edit_contact_set" => EditContactSet {
            name: String,
            #[serde(default)]
            new_name: Option<String>,
            #[serde(default)]
            occurrences: Option<Vec<String>>,
            #[serde(default)]
            suppressed: Option<bool>,
        },
        "delete_contact_set" => DeleteContactSet {
            name: String,
        },
    }
}

/// The `study_values` query: the keyed values of a study at a step.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct StudyValuesQuery {
    study: String,
    step: f64,
}

/// The previews of motions (the application's playback and Animate
/// Joint): `{"cmd": "play_study", ...}` and `{"cmd": "drive_joint", ...}`.
#[derive(Debug, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case", deny_unknown_fields)]
enum MotionPreview {
    PlayStudy {
        study: String,
        step: f64,
        #[serde(default)]
        from: Option<f64>,
    },
    DriveJoint {
        joint: String,
        values: BTreeMap<Motion, ValueInput>,
        #[serde(default)]
        from: Option<BTreeMap<Motion, f64>>,
    },
}

/// Values per joint uid and motion.
pub(crate) fn joint_values_json(values: &JointValues) -> Value {
    Value::Object(
        values
            .iter()
            .map(|(uid, motions)| {
                let motions: Map<String, Value> = motions
                    .iter()
                    .map(|(m, v)| (m.as_str().to_owned(), json!(v)))
                    .collect();
                (uid.to_string(), Value::Object(motions))
            })
            .collect(),
    )
}

/// The document's methods of this module used across the model, as an
/// extension trait (mitcad#777): the api can then live in a crate above
/// the model.
pub(crate) trait MotionCrateOps<K: Kernel> {
    fn run_motion_command(&mut self, command: MotionCommand) -> Result<Value, ApiError>;

    /// A contact that stopped a motion: the set, the two bodies and how far
    /// the motion went (0 to 1).
    fn contact_json(&self, hit: &ContactHit, fraction: f64) -> Value;

    /// The previews of motions, None for another preview command.
    fn motion_preview(
        &mut self,
        value: &Value,
    ) -> Result<Option<(mitcad_model::PreviewReport, Value)>, ApiError>;

    /// The `motion_studies` query: every study in its file form, each key
    /// with its joint's name.
    fn motion_studies_json(&self) -> Value;

    /// The `study_values` query.
    fn study_values_json(&self, query: &StudyValuesQuery) -> Result<Value, ApiError>;

    /// The `contact_sets` query: every set with its occurrences' paths and
    /// names.
    fn contact_sets_json(&self) -> Value;

    /// The `contact_check` query: the pairs of the active contact sets
    /// that pass through each other at the marker.
    fn contact_check_json(&self) -> Result<Value, ApiError>;
}

impl<K: Kernel> MotionCrateOps<K> for Document<K> {
    fn run_motion_command(&mut self, command: MotionCommand) -> Result<Value, ApiError> {
        Ok(match command {
            MotionCommand::AddMotionStudy { name, steps, keys } => {
                let keys = self.study_keys(keys)?;
                let study = MotionStudy {
                    name: name.unwrap_or_default(),
                    steps: steps.unwrap_or(DEFAULT_STEPS),
                    keys,
                };
                let name = self.add_motion_study(study)?;
                json!({"name": name})
            }
            MotionCommand::EditMotionStudy {
                name,
                new_name,
                steps,
                keys,
            } => {
                let keys = keys.map(|k| self.study_keys(k)).transpose()?;
                self.edit_motion_study(&name, new_name.as_deref(), steps, keys)?;
                json!({"name": new_name.unwrap_or(name)})
            }
            MotionCommand::DeleteMotionStudy { name } => {
                self.delete_motion_study(&name)?;
                json!({})
            }
            MotionCommand::SetStudyKey {
                study,
                joint,
                motion,
                step,
                value,
            } => {
                let uid = self.study_joint(&joint)?;
                let value = match value {
                    Some(v) => self.motion_value(&v, motion)?,
                    None => self.joint_value(uid, motion),
                };
                self.set_study_key(&study, uid, motion, step, value)?;
                json!({"joint": uid, "motion": motion.as_str(), "step": step, "value": value})
            }
            MotionCommand::RemoveStudyKey {
                study,
                joint,
                motion,
                step,
            } => {
                let uid = self.study_joint(&joint)?;
                self.remove_study_key(&study, uid, motion, step)?;
                json!({})
            }
            MotionCommand::PlayStudy { study, step } => {
                let outcome = self.play_study(&study, step)?;
                let mut value = self.outcome_json(&outcome);
                value["study"] = json!(study);
                value["step"] = json!(step);
                value
            }
            MotionCommand::AddContactSet {
                name,
                occurrences,
                suppressed,
            } => {
                let set = ContactSet {
                    name: name.unwrap_or_default(),
                    occurrences: self.contact_paths(&occurrences)?,
                    suppressed,
                };
                let name = self.add_contact_set(set)?;
                json!({"name": name})
            }
            MotionCommand::EditContactSet {
                name,
                new_name,
                occurrences,
                suppressed,
            } => {
                let occurrences = occurrences.map(|o| self.contact_paths(&o)).transpose()?;
                self.edit_contact_set(&name, new_name.as_deref(), occurrences, suppressed)?;
                json!({"name": new_name.unwrap_or(name)})
            }
            MotionCommand::DeleteContactSet { name } => {
                self.delete_contact_set(&name)?;
                json!({})
            }
        })
    }

    fn contact_json(&self, hit: &ContactHit, fraction: f64) -> Value {
        let a = self.contact_body_json(&hit.a);
        let b = self.contact_body_json(&hit.b);
        let message = format!(
            "stopped at contact: {} ({}) and {} ({}) in {}",
            a["name"].as_str().unwrap_or_default(),
            a["occurrence"].as_str().unwrap_or_default(),
            b["name"].as_str().unwrap_or_default(),
            b["occurrence"].as_str().unwrap_or_default(),
            hit.set
        );
        json!({"set": hit.set, "bodies": [a, b], "fraction": fraction, "message": message})
    }

    fn motion_preview(
        &mut self,
        value: &Value,
    ) -> Result<Option<(mitcad_model::PreviewReport, Value)>, ApiError> {
        let cmd = value.get("cmd").and_then(Value::as_str).unwrap_or("");
        if cmd != "play_study" && cmd != "drive_joint" {
            return Ok(None);
        }
        let preview: MotionPreview = super::parse(value.clone(), "preview command")?;
        let (report, outcome) = match preview {
            MotionPreview::PlayStudy { study, step, from } => {
                self.preview_study(&study, step, from)?
            }
            MotionPreview::DriveJoint {
                joint,
                values,
                from,
            } => {
                let uid = self.study_joint(&joint)?;
                let mut to = BTreeMap::new();
                for (m, v) in &values {
                    to.insert(*m, self.motion_value(v, *m)?);
                }
                self.preview_drive(uid, &to, from.as_ref())?
            }
        };
        Ok(Some((report, self.outcome_json(&outcome))))
    }

    fn motion_studies_json(&self) -> Value {
        Value::Array(
            self.motion_studies()
                .iter()
                .map(|study| {
                    let keys: Vec<Value> = study
                        .keys
                        .iter()
                        .map(|k| {
                            json!({
                                "joint": k.joint,
                                "joint_name": self.feature(k.joint).map(|f| f.name.clone()),
                                "motion": k.motion.as_str(),
                                "points": k.points,
                            })
                        })
                        .collect();
                    json!({"name": study.name, "steps": study.steps, "keys": keys})
                })
                .collect(),
        )
    }

    fn study_values_json(&self, query: &StudyValuesQuery) -> Result<Value, ApiError> {
        let study = self.motion_study(&query.study)?;
        if !query.step.is_finite() {
            return Err(ApiError("the step must be finite".to_owned()));
        }
        Ok(json!({
            "study": study.name,
            "step": query.step,
            "values": joint_values_json(&study.values_at(query.step)),
        }))
    }

    fn contact_sets_json(&self) -> Value {
        let a = self.assembly();
        Value::Array(
            self.contact_sets()
                .iter()
                .map(|set| {
                    let occurrences: Vec<Value> = set
                        .occurrences
                        .iter()
                        .map(|p| json!({"path": p, "name": a.path_name(&p.0)}))
                        .collect();
                    json!({"name": set.name, "occurrences": occurrences,
                           "suppressed": set.suppressed})
                })
                .collect(),
        )
    }

    fn contact_check_json(&self) -> Result<Value, ApiError> {
        let pairs = ContactPairs::new(self.kernel(), self.state(), self.last_result());
        let found = pairs
            .overlapping(
                self.kernel(),
                &self.last_result().placements,
                &BTreeSet::new(),
            )
            .map_err(|e| ApiError(format!("contact check: {e}")))?;
        let overlaps: Vec<Value> = found
            .into_iter()
            .map(|(p, volume)| {
                let hit = pairs.hit(p, volume);
                json!({"set": hit.set, "volume": volume,
                       "bodies": [self.contact_body_json(&hit.a), self.contact_body_json(&hit.b)]})
            })
            .collect();
        Ok(json!({"overlaps": overlaps}))
    }
}

/// The document's methods used only in this module, as an
/// extension trait (mitcad#777): the api can then live in a crate above
/// the model.
trait MotionLocal<K: Kernel> {
    /// An occurrence path from the root: uids or names (`Arm:1/Pin:2`), or
    /// one occurrence (uid or name) whose component is placed once.
    fn root_path(&self, text: &str) -> Result<OccurrencePath, ApiError>;

    /// A joint or as-built joint by uid or name.
    fn study_joint(&self, text: &str) -> Result<FeatureUid, ApiError>;

    fn study_keys(&self, keys: Vec<KeyInput>) -> Result<Vec<StudyKey>, ApiError>;

    fn contact_paths(&self, occurrences: &[String]) -> Result<Vec<OccurrencePath>, ApiError>;

    fn contact_body_json(&self, body: &ContactBody) -> Value;

    fn outcome_json(&self, outcome: &MotionOutcome) -> Value;
}

impl<K: Kernel> MotionLocal<K> for Document<K> {
    fn root_path(&self, text: &str) -> Result<OccurrencePath, ApiError> {
        let a = self.assembly();
        if let Some(path) = a.find_path(text).filter(|p| !p.is_empty()) {
            return Ok(OccurrencePath(path));
        }
        let uid = self.occurrence_ref(text)?;
        let parent = a.occurrence(uid).expect("found").parent;
        match a.paths_to(parent).as_slice() {
            [one] => {
                let mut path = one.clone();
                path.push(uid);
                Ok(OccurrencePath(path))
            }
            _ => Err(ApiError(format!(
                "{text} is placed in several places: give its path from the root"
            ))),
        }
    }

    fn study_joint(&self, text: &str) -> Result<FeatureUid, ApiError> {
        let is_joint =
            |def: &FeatureDef| matches!(def, FeatureDef::Joint(_) | FeatureDef::AsBuiltJoint(_));
        // The joint of a name it shares with other features (mitcad#1107).
        let found = self
            .feature_by_text(text, Some(&is_joint))?
            .ok_or_else(|| ApiError(format!("no joint {text}")))?;
        match found.def {
            FeatureDef::Joint(_) | FeatureDef::AsBuiltJoint(_) => Ok(found.uid),
            _ => Err(ApiError(format!("{} is not a joint", found.name))),
        }
    }

    fn study_keys(&self, keys: Vec<KeyInput>) -> Result<Vec<StudyKey>, ApiError> {
        keys.into_iter()
            .enumerate()
            .map(|(i, key)| {
                let at = |e: String| ApiError(format!("keys[{i}]: {e}"));
                let joint = self.study_joint(&key.joint).map_err(|e| at(e.0))?;
                let mut points = Vec::with_capacity(key.points.len());
                for (step, value) in &key.points {
                    let v = self
                        .motion_value(value, key.motion)
                        .map_err(|e| at(e.to_string()))?;
                    points.push((*step, v));
                }
                points.sort_by(|a, b| a.0.total_cmp(&b.0));
                Ok(StudyKey {
                    joint,
                    motion: key.motion,
                    points,
                })
            })
            .collect()
    }

    fn contact_paths(&self, occurrences: &[String]) -> Result<Vec<OccurrencePath>, ApiError> {
        occurrences.iter().map(|o| self.root_path(o)).collect()
    }

    fn contact_body_json(&self, body: &ContactBody) -> Value {
        json!({
            "body": body.body,
            "name": self.body_name(body.body),
            "occurrence": self.assembly().path_name(&body.path),
            "path": OccurrencePath(body.path.clone()),
        })
    }

    fn outcome_json(&self, outcome: &MotionOutcome) -> Value {
        let mut value = json!({
            "values": joint_values_json(&outcome.values),
            "fraction": outcome.fraction,
        });
        if let Some(hit) = &outcome.contact {
            value["contact"] = self.contact_json(hit, outcome.fraction);
        }
        value
    }
}
