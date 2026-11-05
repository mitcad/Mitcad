// SPDX-License-Identifier: MIT
//! Commands and queries of components and occurrences (F6, see
//! `commands.md`).

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::Deserialize;
use serde_json::{Value, json};

use super::ApiError;
use super::dispatch::named_enum;
use super::prelude::*;
use mitcad_model::assembly::{from_rows, matrix_rows, matrix4};
use mitcad_model::features::OccurrencePath;
use mitcad_model::features::ValueInput;
use mitcad_model::ids::{BodyUid, ComponentUid, FeatureUid, OccurrenceUid};
use mitcad_model::kernel::Kernel;
use mitcad_model::transform::Transform;
use mitcad_model::{Document, InsertOptions};

/// A placement in a command: rows of a rotation and a translation (3 or 4
/// rows of 4 numbers), or a translation with an optional rotation about an
/// axis through the origin (applied first).
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(crate) enum TransformInput {
    Rows(Vec<Vec<f64>>),
    Parts(Parts),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Parts {
    #[serde(default)]
    translation: [f64; 3],
    #[serde(default)]
    rotation: Option<Rotation>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Rotation {
    axis: [f64; 3],
    /// Radians, right-handed.
    angle: f64,
}

impl TransformInput {
    pub(crate) fn transform(&self) -> Result<Transform, ApiError> {
        match self {
            Self::Rows(rows) => from_rows(rows).map_err(ApiError),
            Self::Parts(parts) => {
                let turn = match &parts.rotation {
                    Some(r) => Transform::rotation([0.0; 3], r.axis, r.angle)
                        .ok_or_else(|| ApiError("the rotation axis is zero".to_owned()))?,
                    None => Transform::IDENTITY,
                };
                Ok(Transform::translation(parts.translation).after(&turn))
            }
        }
    }
}

fn optional(input: Option<&TransformInput>) -> Result<Option<Transform>, ApiError> {
    input.map(TransformInput::transform).transpose()
}

named_enum! {
    #[derive(Debug, Deserialize)]
    #[serde(tag = "cmd", rename_all = "snake_case", deny_unknown_fields)]
    pub(crate) enum ComponentCommand {
        "create_component" => CreateComponent {
            #[serde(default)]
            name: Option<String>,
            #[serde(default)]
            transform: Option<TransformInput>,
            #[serde(default = "yes")]
            activate: bool,
            /// Its design policy (mitcad#220): a preset name or settings over
            /// the general defaults; the general defaults when left out.
            #[serde(default)]
            policy: Option<Value>,
        },
        "components_from_bodies" => ComponentsFromBodies {
            bodies: Vec<BodyUid>,
        },
        "activate_component" => ActivateComponent {
            component: String,
            /// As the document opens (mitcad#426): no undo step, no new revision.
            #[serde(default)]
            opening: bool,
        },
        "rename_component" => RenameComponent {
            component: String,
            name: String,
        },
        "ground_occurrence" => GroundOccurrence {
            occurrence: String,
            #[serde(default = "yes")]
            grounded: bool,
        },
        "set_occurrence_visible" => SetOccurrenceVisible {
            occurrence: String,
            #[serde(default)]
            visible: Option<bool>,
            /// A condition the visibility follows instead (mitcad#800); ""
            /// removes it.
            #[serde(default)]
            expression: Option<String>,
        },
        "set_occurrence_transform" => SetOccurrenceTransform {
            occurrence: String,
            transform: TransformInput,
            #[serde(default)]
            capture: bool,
        },
        "copy_occurrence" => CopyOccurrence {
            occurrence: String,
            #[serde(default)]
            transform: Option<TransformInput>,
        },
        "paste_new" => PasteNew {
            occurrence: String,
            #[serde(default)]
            transform: Option<TransformInput>,
        },
        "delete_occurrence" => DeleteOccurrence {
            occurrence: String,
        },
        "insert_component" => InsertComponent {
            /// A design file; or `library` (mitcad#64).
            #[serde(default)]
            path: Option<String>,
            #[serde(default)]
            library: Option<super::libraries::LibraryInput>,
            #[serde(default = "yes")]
            link: bool,
            #[serde(default)]
            transform: Option<TransformInput>,
            #[serde(default)]
            name: Option<String>,
            #[serde(default)]
            base: Option<String>,
            /// Placed by a mount frame (mitcad#525).
            #[serde(default)]
            mount: Option<Box<mitcad_model::MountInput>>,
            /// Linked with options (mitcad#801): values of the source's
            /// user parameters, expressions of this design's parameters.
            #[serde(default)]
            parameters: Option<BTreeMap<String, ValueInput>>,
        },
        "update_links" => UpdateLinks {
            #[serde(default)]
            base: Option<String>,
        },
        // Moving an occurrence to another parent (mitcad#219).
        "reparent_occurrence" => ReparentOccurrence {
            /// Its path from the root.
            occurrence: String,
            /// The path of the new parent's occurrence; empty: the root.
            #[serde(default)]
            parent: String,
        },
        // Options of an inserted component (mitcad#801).
        "set_component_parameters" => SetComponentParameters {
            component: String,
            /// {option: expression or number}; null removes the
            /// component's own value.
            #[serde(default)]
            parameters: BTreeMap<String, Option<ValueInput>>,
            #[serde(default)]
            config: Option<String>,
        },
        // Save Component as Design (mitcad#1057): the component written as
        // a design of its own, optionally replaced by a link to it.
        "save_component_as_design" => SaveComponentAsDesign {
            component: String,
            /// The design file to write.
            path: String,
            /// The component becomes a linked component of the file.
            #[serde(default)]
            replace: bool,
            /// The folder of this design's file (relative link paths).
            #[serde(default)]
            base: Option<String>,
        },
        // An occurrence drawn see-through with what it contains (mitcad#1093).
        "set_occurrence_transparent" => SetOccurrenceTransparent {
            occurrence: String,
            transparent: bool,
        },
        // A linked component shown by its envelope (mitcad#1271).
        "set_link_envelope" => SetLinkEnvelope {
            occurrence: String,
            /// The envelope, or the linked design's bodies again.
            #[serde(default = "yes")]
            envelope: bool,
            /// mm; left out: the size it, or another component of the
            /// file, is shown at, else 1 % of the design's diagonal, at
            /// least 5 mm.
            #[serde(default)]
            detail: Option<f64>,
            /// The folder of relative link paths; left out: the design
            /// file's.
            #[serde(default)]
            base: Option<String>,
            /// Every occurrence of its component (mitcad#1271); left out: only
            /// this occurrence (mitcad#1276).
            #[serde(default)]
            all: bool,
        },
        // Find Repeated Parts (mitcad#1277): a base feature's copies of one
        // body as occurrences of one component.
        "find_repeated_parts" => FindRepeatedParts {
            feature: FeatureUid,
            /// mm; the design's join tolerance, else 0.01 mm.
            #[serde(default)]
            tolerance: Option<f64>,
        },
    }
}

/// What Find Repeated Parts made (mitcad#1277), as `find_repeated_parts`
/// and `import_file`'s `repeated_parts` answer it.
pub(crate) fn repeated_parts_json(found: &mitcad_model::RepeatedParts) -> Value {
    let parts: Vec<Value> = found
        .parts
        .iter()
        .map(|p| {
            json!({"component": p.component, "name": p.name, "feature": p.feature,
                   "body": p.body, "occurrences": p.occurrences, "copies": p.copies})
        })
        .collect();
    json!({"components": parts, "kept": found.kept, "combines": found.combines})
}

fn yes() -> bool {
    true
}

/// The `reparent_occurrence` query (mitcad#219): where an occurrence can
/// go and, with `parent`, what moving it there does.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReparentQuery {
    occurrence: String,
    #[serde(default)]
    parent: Option<String>,
}

/// The `save_component_as_design` query (mitcad#1057): what saving a
/// component as a design of its own would freeze or leave out.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SaveComponentQuery {
    component: String,
}

/// The references a saved component could not carry, as the command and
/// the query list them.
fn cut_references_json(references: &[mitcad_model::CutReference]) -> Value {
    Value::Array(
        references
            .iter()
            .map(|r| {
                json!({"feature": r.feature, "feature_name": r.feature_name,
                       "kind": r.kind.as_str(), "target": r.target, "frozen": r.frozen,
                       "message": r.message()})
            })
            .collect(),
    )
}

/// The `instances` query.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct InstancesQuery {
    /// World volume, area, centre of mass and bounding box of each.
    #[serde(default)]
    properties: bool,
    /// Hidden ones too.
    #[serde(default)]
    hidden: bool,
}

impl InstancesQuery {
    /// Every instance with its properties (the `report` query).
    pub(crate) fn everything() -> Self {
        Self {
            properties: true,
            hidden: true,
        }
    }
}

/// The document's methods of this module used across the model, as an
/// extension trait (mitcad#777): the api can then live in a crate above
/// the model.
pub(crate) trait ComponentsCrateOps<K: Kernel> {
    /// A component by uid (`C2`, `C0` the root) or name.
    fn component_ref(&self, text: &str) -> Result<ComponentUid, ApiError>;

    /// An occurrence by uid (`O3`), name (`Component1:2`) or path from the
    /// root (`Arm:1/Pin:2`, `O1/O4`: its last occurrence).
    fn occurrence_ref(&self, text: &str) -> Result<OccurrenceUid, ApiError>;

    fn run_component_command(&mut self, command: ComponentCommand) -> Result<Value, ApiError>;

    /// The `reparent_occurrence` query (mitcad#219): the occurrence, what
    /// moves with it, every parent it could go into with the reason it
    /// cannot, and with `parent` what the move there does.
    fn reparent_json(&self, query: &ReparentQuery) -> Result<Value, ApiError>;

    /// The `components` query: the components, the occurrence tree and
    /// the active component.
    fn components_json(&self) -> Value;

    /// The `instances` query: the bodies as placed in the design.
    fn instances_json(&self, query: &InstancesQuery) -> Result<Value, ApiError>;

    /// The `save_component_as_design` query (mitcad#1057): the references
    /// of the component to things outside it, or why it cannot be saved.
    fn save_component_json(&self, query: &SaveComponentQuery) -> Result<Value, ApiError>;
}

impl<K: Kernel> ComponentsCrateOps<K> for Document<K> {
    fn component_ref(&self, text: &str) -> Result<ComponentUid, ApiError> {
        let a = self.assembly();
        if let Ok(uid) = text.parse::<ComponentUid>()
            && a.exists(uid)
        {
            return Ok(uid);
        }
        if text == a.root_name {
            return Ok(ComponentUid::ROOT);
        }
        a.components
            .iter()
            .find(|c| c.name == text)
            .map(|c| c.uid)
            .ok_or_else(|| ApiError(format!("no component {text}")))
    }

    fn occurrence_ref(&self, text: &str) -> Result<OccurrenceUid, ApiError> {
        let a = self.assembly();
        if let Ok(uid) = text.parse::<OccurrenceUid>()
            && a.occurrence(uid).is_some()
        {
            return Ok(uid);
        }
        if let Some(o) = a
            .occurrences
            .iter()
            .find(|o| a.occurrence_name(o.uid) == text)
        {
            return Ok(o.uid);
        }
        a.find_path(text)
            .and_then(|path| path.last().copied())
            .ok_or_else(|| ApiError(format!("no occurrence {text}")))
    }

    fn run_component_command(&mut self, command: ComponentCommand) -> Result<Value, ApiError> {
        Ok(match command {
            ComponentCommand::CreateComponent {
                name,
                transform,
                activate,
                policy,
            } => {
                let transform = optional(transform.as_ref())?.unwrap_or(Transform::IDENTITY);
                let base = mitcad_model::policy::Policy::GENERAL;
                let policy = match &policy {
                    None => base,
                    Some(Value::String(preset)) => {
                        super::policy::changed_policy(base, Some(preset.as_str()), None)?
                    }
                    Some(Value::Object(settings)) => {
                        super::policy::changed_policy(base, None, Some(settings))?
                    }
                    Some(_) => {
                        return Err(ApiError(
                            "policy: a preset name or an object of settings".to_owned(),
                        ));
                    }
                };
                let requested = self.active_component();
                let (component, occurrence) =
                    self.create_component_with(name.as_deref(), transform, activate, policy)?;
                let mut reply = json!({"component": component, "occurrence": occurrence,
                                       "name": self.assembly().name(component)});
                self.note_placed_beside(&mut reply, requested, component, "New Component");
                reply
            }
            ComponentCommand::ComponentsFromBodies { bodies } => {
                let made = self.components_from_bodies(&bodies)?;
                let items: Vec<Value> = made
                    .iter()
                    .map(|(added, component)| {
                        json!({"feature": added.uid, "component": component,
                               "name": self.assembly().name(*component)})
                    })
                    .collect();
                json!({"components": items})
            }
            ComponentCommand::ActivateComponent { component, opening } => {
                let uid = self.component_ref(&component)?;
                if opening {
                    let previous = self.active_component();
                    self.activate_component_on_open(uid)?;
                    json!({"component": uid, "previous": previous})
                } else {
                    self.activate_component(uid)?;
                    json!({"component": uid})
                }
            }
            ComponentCommand::RenameComponent { component, name } => {
                let uid = self.component_ref(&component)?;
                // Its only body, default-named, is renamed with it (mitcad#291).
                let body = self.rename_component(uid, &name)?;
                let renamed = body.map(|b| json!({"uid": b, "name": self.body_name(b)}));
                json!({"renamed_body": renamed})
            }
            ComponentCommand::GroundOccurrence {
                occurrence,
                grounded,
            } => {
                let uid = self.occurrence_ref(&occurrence)?;
                self.set_occurrence_grounded(uid, grounded)?;
                json!({})
            }
            ComponentCommand::SetOccurrenceVisible {
                occurrence,
                visible,
                expression,
            } => {
                let uid = self.occurrence_ref(&occurrence)?;
                match (visible, expression) {
                    (Some(_), Some(_)) => {
                        return Err(ApiError(
                            "`visible` and `expression` are both given: give the value, or the \
                             expression it follows"
                                .to_owned(),
                        ));
                    }
                    (None, None) => {
                        return Err(ApiError(
                            "set_occurrence_visible needs `visible` or `expression`".to_owned(),
                        ));
                    }
                    (Some(visible), None) => self.set_occurrence_visible(uid, visible)?,
                    (None, Some(expression)) => self.bind_occurrence_visible(uid, &expression)?,
                }
                json!({})
            }
            ComponentCommand::SetOccurrenceTransform {
                occurrence,
                transform,
                capture,
            } => {
                let uid = self.occurrence_ref(&occurrence)?;
                let added = self.set_occurrence_transform(uid, transform.transform()?, capture)?;
                json!({"feature": added.map(|a| a.uid)})
            }
            ComponentCommand::CopyOccurrence {
                occurrence,
                transform,
            } => {
                let uid = self.occurrence_ref(&occurrence)?;
                let copy = self.copy_occurrence(uid, optional(transform.as_ref())?)?;
                json!({"occurrence": copy, "name": self.assembly().occurrence_name(copy)})
            }
            ComponentCommand::PasteNew {
                occurrence,
                transform,
            } => {
                let uid = self.occurrence_ref(&occurrence)?;
                let (component, copy) = self.paste_new(uid, optional(transform.as_ref())?)?;
                json!({"component": component, "occurrence": copy,
                       "name": self.assembly().name(component)})
            }
            ComponentCommand::DeleteOccurrence { occurrence } => {
                let uid = self.occurrence_ref(&occurrence)?;
                let before = self.state().clone();
                let deleted = self.delete_occurrence(uid)?;
                let mut reply = json!({"deleted": deleted});
                // The cut-scope exclusions it dropped (mitcad#1236).
                let dropped = self.state().dropped_exclusions(&before);
                if !dropped.is_empty() {
                    reply["notices"] = mitcad_model::notice::to_json(&dropped);
                }
                reply
            }
            ComponentCommand::InsertComponent {
                path,
                library,
                link,
                transform,
                name,
                base,
                mount,
                parameters,
            } => {
                let options = InsertOptions {
                    link,
                    transform: optional(transform.as_ref())?.unwrap_or(Transform::IDENTITY),
                    name,
                    base: base.map(PathBuf::from),
                    mount: mount.map(|m| *m),
                    parameters,
                };
                // With its mount frames and the joint a mount placement made
                // (mitcad#525).
                let inserted = match (path, library) {
                    (Some(path), None) => self.insert_file(&path, &options)?,
                    (None, Some(library)) => self.insert_library(&library.part(), &options)?,
                    _ => {
                        return Err(ApiError(
                            "insert_component needs a path or a library part, not both".to_owned(),
                        ));
                    }
                };
                super::mounts::inserted_json(self, &inserted)
            }
            ComponentCommand::SetComponentParameters {
                component,
                parameters,
                config,
            } => {
                let change = mitcad_model::OptionsChange {
                    component: self.component_ref(&component)?,
                    parameters,
                    config,
                };
                let changed = self.set_component_parameters(&change)?;
                super::component_options::changed_json(self, &changed)
            }
            ComponentCommand::SaveComponentAsDesign {
                component,
                path,
                replace,
                base,
            } => {
                let uid = self.component_ref(&component)?;
                let options = mitcad_model::SaveComponentOptions {
                    replace,
                    base: base.map(PathBuf::from),
                };
                let saved =
                    self.save_component_as_design(uid, std::path::Path::new(&path), &options)?;
                json!({"component": uid, "path": saved.path.to_string_lossy(),
                       "references": cut_references_json(&saved.references),
                       "replaced": saved.link.is_some(), "link": saved.link})
            }
            ComponentCommand::UpdateLinks { base } => {
                let base = base.map(PathBuf::from);
                let messages = self.update_links(base.as_deref())?;
                json!({"messages": messages})
            }
            ComponentCommand::ReparentOccurrence { occurrence, parent } => {
                let source = self.path_ref(&occurrence)?;
                let destination = self.path_ref(&parent)?;
                let plan = self.reparent_occurrence(&source, &destination)?;
                let a = self.assembly();
                let path = plan.path();
                let features = self.carried_json(&plan);
                json!({"occurrence": plan.occurrence, "component": plan.component,
                       "path": OccurrencePath(path.clone()), "path_name": a.path_name(&path),
                       "parent": plan.destination, "updated": plan.updates,
                       "features": features})
            }
            ComponentCommand::SetOccurrenceTransparent {
                occurrence,
                transparent,
            } => {
                let uid = self.occurrence_ref(&occurrence)?;
                self.set_occurrence_transparent(uid, transparent)?;
                json!({})
            }
            ComponentCommand::SetLinkEnvelope {
                occurrence,
                envelope,
                detail,
                base,
                all,
            } => {
                let uid = self.occurrence_ref(&occurrence)?;
                let base = base.map(PathBuf::from);
                let switched =
                    self.set_link_envelope(uid, envelope, detail, base.as_deref(), all)?;
                let bodies: Vec<BodyUid> = self
                    .bodies()
                    .iter()
                    .filter(|b| self.body_component(b.uid) == Some(switched.component))
                    .map(|b| b.uid)
                    .collect();
                json!({"component": switched.component, "envelope": switched.detail.is_some(),
                       "detail": switched.detail, "changed": switched.changed,
                       "bodies": bodies})
            }
            ComponentCommand::FindRepeatedParts { feature, tolerance } => {
                let found = self.find_repeated_parts(feature, tolerance)?;
                repeated_parts_json(&found)
            }
        })
    }

    fn reparent_json(&self, query: &ReparentQuery) -> Result<Value, ApiError> {
        let a = self.assembly();
        let source = self.path_ref(&query.occurrence)?;
        let label = |path: &[OccurrenceUid]| {
            if path.is_empty() {
                a.root_name.clone()
            } else {
                format!("{}/{}", a.root_name, a.path_name(path))
            }
        };
        let problems: Vec<String> = self
            .reparent_source(&source)
            .err()
            .map(|e| e.to_string())
            .into_iter()
            .collect();
        let Some((&uid, parents)) = source.split_last() else {
            let root = json!({"occurrence": null, "path": "", "path_name": a.root_name,
                              "problems": problems, "destinations": []});
            return Ok(root);
        };
        let occurrence = a
            .occurrence(uid)
            .ok_or_else(|| ApiError(format!("occurrence {uid} does not exist")))?;
        // What moves with it: its component and everything placed in it.
        let mut inside = Vec::new();
        let mut components = vec![occurrence.component];
        let mut todo = vec![(occurrence.component, String::new())];
        while let Some((component, prefix)) = todo.pop() {
            if inside.len() > 4096 {
                break; // a guard against cycles in loaded data
            }
            for child in a.children(component) {
                let name = format!("{prefix}{}", a.occurrence_name(child.uid));
                inside.push(name.clone());
                if !components.contains(&child.component) {
                    components.push(child.component);
                }
                todo.push((child.component, format!("{name}/")));
            }
        }
        inside.sort();
        let bodies: usize = components
            .iter()
            .map(|c| self.component_bodies(*c).len())
            .sum();
        let features = self
            .features()
            .filter(|f| components.contains(&f.component))
            .count();
        let mut destinations = Vec::new();
        for component in
            std::iter::once(ComponentUid::ROOT).chain(a.components.iter().map(|c| c.uid))
        {
            for path in a.paths_to(component) {
                let reason = self
                    .plan_reparent(&source, &path)
                    .err()
                    .map(|blockers| blockers.join("; "));
                let parent_occurrence =
                    OccurrencePath(path[..path.len().saturating_sub(1)].to_vec());
                destinations.push(json!({
                    "component": component, "name": a.name(component),
                    "occurrence": OccurrencePath(path.clone()), "path": label(&path),
                    "parent_occurrence": parent_occurrence,
                    "current": path == parents,
                    "eligible": reason.is_none(), "reason": reason.unwrap_or_default(),
                }));
            }
        }
        let mut value = json!({
            "occurrence": uid,
            "name": a.occurrence_name(uid),
            "path": OccurrencePath(source.clone()),
            "path_name": label(&source),
            "component": occurrence.component,
            "component_name": a.name(occurrence.component),
            "parent": occurrence.parent,
            "parent_path": label(parents),
            "grounded": occurrence.grounded,
            "contents": {"occurrences": inside, "components": components.len(),
                         "bodies": bodies, "features": features},
            "problems": problems,
            "destinations": destinations,
        });
        if let Some(parent) = &query.parent {
            let destination = self.path_ref(parent)?;
            value["plan"] = match self.plan_reparent(&source, &destination) {
                Ok(plan) => json!({
                    "eligible": true, "parent": plan.destination,
                    "parent_path": label(&plan.destination_path),
                    "from": plan.parent, "from_path": label(&plan.source[..plan.source.len() - 1]),
                    "path": OccurrencePath(plan.path()),
                    "transform": matrix_rows(&plan.transform),
                    "updates": plan.updates, "features": self.carried_json(&plan),
                    "problems": [],
                }),
                Err(blockers) => json!({"eligible": false, "problems": blockers, "updates": [],
                                        "features": []}),
            };
        }
        Ok(value)
    }

    fn components_json(&self) -> Value {
        let a = self.assembly();
        let component = |uid: ComponentUid| {
            let def = a.component(uid);
            let features: Vec<_> = self
                .features()
                .filter(|f| f.component == uid)
                .map(|f| f.uid)
                .collect();
            let bodies = self.component_bodies(uid);
            let mut value = json!({
                "uid": uid,
                "name": a.name(uid),
                "created_by": def.and_then(|d| d.created_by),
                "link": def.and_then(|d| d.link.as_ref()).map(|l| l.path.clone()),
                "library": def.and_then(|d| d.library.as_ref()),
                "features": features,
                "bodies": bodies,
                "occurrences": a.occurrences_of(uid).filter(|o| self.placement(o.uid).is_some()).count(),
                // Design policy (mitcad#220).
                "policy": super::policy::policy_json(&a.policy(uid)),
            });
            // A linked component shown by its envelope: its detail size
            // (mitcad#1271).
            if let Some(detail) = def.and_then(|d| d.link.as_ref()).and_then(|l| l.envelope) {
                value["envelope"] = json!(detail);
            }
            value
        };
        let mut components = vec![component(ComponentUid::ROOT)];
        components.extend(a.components.iter().map(|c| component(c.uid)));
        json!({
            "active": a.active,
            "components": components,
            "occurrences": self.occurrence_tree(ComponentUid::ROOT, &[], Transform::IDENTITY),
        })
    }

    fn instances_json(&self, query: &InstancesQuery) -> Result<Value, ApiError> {
        let a = self.assembly();
        let kernel = self.kernel();
        let mut out = Vec::new();
        for i in self.instances() {
            if !(i.visible || query.hidden) {
                continue;
            }
            let mut value = json!({
                "path": i.path,
                "occurrence": i.path_name,
                "component": i.component,
                "component_name": a.name(i.component),
                "body": i.body,
                "name": i.name,
                "visible": i.visible,
                "transform": matrix4(&i.transform),
            });
            // The body or an occurrence on its path (mitcad#1093).
            if i.transparent {
                value["transparent"] = json!(true);
            }
            if query.properties {
                let error =
                    |e: mitcad_model::kernel::KernelError| ApiError(format!("{}: {e}", i.name));
                let shape = self
                    .body_shape(i.body)
                    .ok_or_else(|| ApiError(format!("body {} has no shape", i.body)))?;
                let placed = if i.transform.is_identity() {
                    shape.clone()
                } else {
                    kernel
                        .transform_shape(shape, &i.transform, None)
                        .map_err(error)?
                };
                let mass = kernel.mass_properties(&placed).map_err(error)?;
                value["volume"] = json!(mass.volume);
                value["area"] = json!(mass.area);
                value["center"] = json!(mass.center);
                if let Some(bounds) = kernel.bounding_box(&placed).map_err(error)? {
                    value["bbox"] = json!({"min": bounds.min, "max": bounds.max});
                }
            }
            out.push(value);
        }
        Ok(Value::Array(out))
    }

    fn save_component_json(&self, query: &SaveComponentQuery) -> Result<Value, ApiError> {
        let uid = self.component_ref(&query.component)?;
        let name = self.assembly().name(uid);
        Ok(match self.component_design_references(uid) {
            Ok(references) => json!({"component": uid, "name": name,
                                     "references": cut_references_json(&references),
                                     "blocked": null}),
            Err(e) => json!({"component": uid, "name": name, "references": [],
                             "blocked": e.to_string()}),
        })
    }
}

/// The document's methods used only in this module, as an
/// extension trait (mitcad#777): the api can then live in a crate above
/// the model.
trait ComponentsLocal<K: Kernel> {
    /// An occurrence path from the root by uids (`O1/O4`) or names
    /// (`Arm:1/Pin:2`); the empty text is the root.
    fn path_ref(&self, text: &str) -> Result<Vec<OccurrenceUid>, ApiError>;

    /// The features that place a moved occurrence and go with it
    /// (mitcad#301): `name`, `from`/`from_name` (its component), `to`/
    /// `to_name` (the new parent) and `split` (the new feature that takes
    /// the occurrence's part of it, or null when it goes whole).
    fn carried_json(&self, plan: &mitcad_model::internal::document::ReparentPlan) -> Vec<Value>;

    fn occurrence_tree(
        &self,
        parent: ComponentUid,
        path: &[OccurrenceUid],
        at: Transform,
    ) -> Vec<Value>;
}

impl<K: Kernel> ComponentsLocal<K> for Document<K> {
    fn path_ref(&self, text: &str) -> Result<Vec<OccurrenceUid>, ApiError> {
        self.assembly()
            .find_path(text)
            .ok_or_else(|| ApiError(format!("no occurrence path {text}")))
    }

    fn carried_json(&self, plan: &mitcad_model::internal::document::ReparentPlan) -> Vec<Value> {
        let a = self.assembly();
        plan.carried
            .iter()
            .map(|c| {
                json!({"name": c.name, "from": c.from, "from_name": a.name(c.from),
                       "to": c.to, "to_name": a.name(c.to), "split": c.split})
            })
            .collect()
    }

    fn occurrence_tree(
        &self,
        parent: ComponentUid,
        path: &[OccurrenceUid],
        at: Transform,
    ) -> Vec<Value> {
        let a = self.assembly();
        if path.len() > 64 {
            return Vec::new();
        }
        a.children(parent)
            .filter(|o| self.placement(o.uid).is_some())
            .map(|o| {
                let mut path = path.to_vec();
                path.push(o.uid);
                let placement = self.placement(o.uid).unwrap_or(o.transform);
                let world = at.after(&placement);
                let mut value = json!({
                    "uid": o.uid,
                    "name": a.occurrence_name(o.uid),
                    "path": a.path_name(&path),
                    "component": o.component,
                    "transform": matrix_rows(&placement),
                    "world": matrix4(&world),
                    "grounded": o.grounded,
                    "visible": o.visible,
                    "children": self.occurrence_tree(o.component, &path, world),
                });
                // Drawn see-through with its contents (mitcad#1093).
                if o.transparent {
                    value["transparent"] = json!(true);
                }
                // The condition it is shown under (mitcad#800), the
                // parameters it uses, and why it does not hold.
                if let Some(text) = self.occurrence_visible_when(o.uid) {
                    value["visible_when"] = json!(text);
                    value["dependencies"] =
                        json!(mitcad_model::DocState::expression_parameters([text]));
                    if let Some(warning) = self.state().occurrence_binding_warning(o.uid) {
                        value["warnings"] = json!([warning]);
                    }
                }
                value
            })
            .collect()
    }
}
