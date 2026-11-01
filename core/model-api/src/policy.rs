// SPDX-License-Identifier: MIT
//! Commands and queries of the design policy (mitcad#220, see
//! `commands.md`, "Design policy").

use serde::Deserialize;
use serde_json::{Map, Value, json};

use super::ApiError;
use super::dispatch::named_enum;
use super::prelude::*;
use mitcad_model::ids::ComponentUid;
use mitcad_model::kernel::Kernel;
use mitcad_model::policy::{Policy, Setting};
use mitcad_model::{Document, PolicyReport};

named_enum! {
    #[derive(Debug, Deserialize)]
    #[serde(tag = "cmd", rename_all = "snake_case", deny_unknown_fields)]
    pub(crate) enum PolicyCommand {
        "set_policy" => SetPolicy {
            /// Uid or name; the active component when left out.
            #[serde(default)]
            component: Option<String>,
            /// `hybrid` (alias `defaults`), `part` or `assembly`, applied first.
            #[serde(default)]
            preset: Option<String>,
            /// Settings to change (the others stay).
            #[serde(default)]
            policy: Option<Map<String, Value>>,
        },
    }
}

/// The `policy` query.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PolicyQuery {
    /// Uid or name; the active component when left out.
    #[serde(default)]
    component: Option<String>,
    /// Settings to report as if they were set (the others stay).
    #[serde(default)]
    policy: Option<Map<String, Value>>,
    #[serde(default)]
    preset: Option<String>,
}

/// The `sketch_supports` query.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SketchSupportsQuery {
    #[serde(default)]
    component: Option<String>,
}

/// A policy changed by a preset and then by the settings given.
pub(crate) fn changed_policy(
    base: Policy,
    preset: Option<&str>,
    settings: Option<&Map<String, Value>>,
) -> Result<Policy, ApiError> {
    let mut policy = match preset {
        Some(name) => Policy::preset(name)
            .ok_or_else(|| ApiError(format!("unknown preset {name} (hybrid, part or assembly)")))?,
        None => base,
    };
    for (key, value) in settings.into_iter().flatten() {
        let setting = Setting::ALL
            .into_iter()
            .find(|s| s.key() == key.as_str())
            .ok_or_else(|| ApiError(format!("policy: unknown setting `{key}`")))?;
        let Value::Bool(allowed) = value else {
            return Err(ApiError(format!("policy: `{key}` must be true or false")));
        };
        let field = match setting {
            Setting::Sketches => &mut policy.sketches,
            Setting::Features => &mut policy.features,
            Setting::Components => &mut policy.components,
            Setting::AttachToOthers => &mut policy.attach_to_others,
            Setting::AttachmentsFromOthers => &mut policy.attachments_from_others,
            Setting::FirstSketchAttaches => &mut policy.first_sketch_attaches,
            Setting::FirstSketchAttachmentsFromOthers => {
                &mut policy.first_sketch_attachments_from_others
            }
            Setting::ReferenceOthers => &mut policy.reference_others,
            Setting::ReferencesFromOthers => &mut policy.references_from_others,
        };
        *field = *allowed;
    }
    Ok(policy)
}

/// A policy as JSON: its settings and the preset it is, if any.
pub(crate) fn policy_json(policy: &Policy) -> Value {
    let mut value = json!(policy);
    value["preset"] = json!(policy.preset_name());
    value
}

/// The document's methods of this module used across the model, as an
/// extension trait (mitcad#777): the api can then live in a crate above
/// the model.
pub(crate) trait PolicyCrateOps<K: Kernel> {
    fn run_policy_command(&mut self, command: PolicyCommand) -> Result<Value, ApiError>;

    /// The `policy` query: a component's settings, what breaks each and why
    /// the component refuses new objects.
    fn policy_query_json(&self, query: &PolicyQuery) -> Result<Value, ApiError>;

    /// The `policies` query: every owner's policy and how many objects
    /// break it.
    fn policies_json(&self) -> Result<Value, ApiError>;

    /// A reply's note that a new component went beside the owner that was
    /// asked for, which takes no components (mitcad#292): `placed_in` (the
    /// owner it went into), `instead_of` and `placement_warning`, the text
    /// the application shows. Nothing when it went where asked.
    fn note_placed_beside(
        &self,
        reply: &mut Value,
        requested: ComponentUid,
        made: ComponentUid,
        what: &str,
    );

    /// The `sketch_supports` query: whose faces and planes a component's
    /// new sketches may be attached to. `denied` lists the other owners
    /// they may not, with the reason, and `denied_paths` every occurrence
    /// path (`O1/O4`, empty for the root) that shows such an owner.
    fn sketch_supports_json(&self, query: &SketchSupportsQuery) -> Result<Value, ApiError>;
}

impl<K: Kernel> PolicyCrateOps<K> for Document<K> {
    fn run_policy_command(&mut self, command: PolicyCommand) -> Result<Value, ApiError> {
        Ok(match command {
            PolicyCommand::SetPolicy {
                component,
                preset,
                policy,
            } => {
                let uid = self.policy_component(component.as_deref())?;
                let next = changed_policy(self.policy(uid), preset.as_deref(), policy.as_ref())?;
                self.set_policy(uid, next)?;
                json!({"component": uid, "policy": policy_json(&next)})
            }
        })
    }

    fn policy_query_json(&self, query: &PolicyQuery) -> Result<Value, ApiError> {
        let uid = self.policy_component(query.component.as_deref())?;
        let asked = (query.policy.is_some() || query.preset.is_some())
            .then(|| {
                changed_policy(
                    self.policy(uid),
                    query.preset.as_deref(),
                    query.policy.as_ref(),
                )
            })
            .transpose()?;
        let report = self.policy_report(uid, asked)?;
        Ok(self.report_json(&report))
    }

    fn policies_json(&self) -> Result<Value, ApiError> {
        let a = self.assembly();
        let owners = std::iter::once(ComponentUid::ROOT).chain(a.components.iter().map(|c| c.uid));
        let mut out = Vec::new();
        for uid in owners {
            let report = self.policy_report(uid, None)?;
            let violations: usize = report.settings.iter().map(|(_, items)| items.len()).sum();
            out.push(json!({"component": uid, "name": a.name(uid),
                            "policy": policy_json(&report.policy),
                            "violations": violations}));
        }
        Ok(Value::Array(out))
    }

    fn note_placed_beside(
        &self,
        reply: &mut Value,
        requested: ComponentUid,
        made: ComponentUid,
        what: &str,
    ) {
        let a = self.assembly();
        let Some(placed_in) = a.occurrences_of(made).next().map(|o| o.parent) else {
            return;
        };
        if placed_in == requested {
            return;
        }
        reply["placed_in"] = json!(placed_in);
        reply["instead_of"] = json!(requested);
        reply["placement_warning"] = json!(format!(
            "{} does not take components (its policy): {what} made in {} instead",
            a.name(requested),
            a.name(placed_in)
        ));
    }

    fn sketch_supports_json(&self, query: &SketchSupportsQuery) -> Result<Value, ApiError> {
        let uid = self.policy_component(query.component.as_deref())?;
        let a = self.assembly();
        let denials = self.attachment_denials(uid);
        let mut paths = Map::new();
        let denied: Vec<Value> = denials
            .iter()
            .map(|(other, reason)| {
                for path in a.paths_to(*other) {
                    let text = path
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join("/");
                    paths.insert(text, json!(reason));
                }
                json!({"component": other, "name": a.name(*other), "reason": reason})
            })
            .collect();
        Ok(
            json!({"component": uid, "denied": denied, "denied_paths": paths,
                  "sketches": self.creation_refusal(uid, Setting::Sketches)}),
        )
    }
}

/// The document's methods used only in this module, as an
/// extension trait (mitcad#777): the api can then live in a crate above
/// the model.
trait PolicyLocal<K: Kernel> {
    fn policy_component(&self, text: Option<&str>) -> Result<ComponentUid, ApiError>;

    fn report_json(&self, report: &PolicyReport) -> Value;
}

impl<K: Kernel> PolicyLocal<K> for Document<K> {
    fn policy_component(&self, text: Option<&str>) -> Result<ComponentUid, ApiError> {
        match text {
            Some(text) => self.component_ref(text),
            None => Ok(self.active_component()),
        }
    }

    fn report_json(&self, report: &PolicyReport) -> Value {
        let a = self.assembly();
        let settings: Vec<Value> = report
            .settings
            .iter()
            .map(|(setting, items)| {
                let items: Vec<Value> = items
                    .iter()
                    .map(|item| {
                        let others: Vec<Value> = item
                            .others
                            .iter()
                            .map(|o| json!({"uid": o, "name": a.name(*o)}))
                            .collect();
                        json!({"type": item.kind, "uid": item.uid, "name": item.name,
                               "component": item.component,
                               "component_name": a.name(item.component),
                               "others": others})
                    })
                    .collect();
                json!({"setting": setting.key(), "allowed": report.policy.allows(*setting),
                       "count": items.len(), "items": items})
            })
            .collect();
        let mut refusals = Map::new();
        for setting in [Setting::Sketches, Setting::Features, Setting::Components] {
            if let Some(why) = self.creation_refusal(report.component, setting) {
                refusals.insert(setting.key().to_owned(), json!(why));
            }
        }
        json!({
            "component": report.component,
            "name": a.name(report.component),
            "active": a.active == report.component,
            "policy": policy_json(&report.policy),
            "settings": settings,
            "refusals": refusals,
        })
    }
}
