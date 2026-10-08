// SPDX-License-Identifier: MIT
//! Commands and queries of components and occurrences (F6, see
//! `commands.md`).

use std::path::PathBuf;

use serde::Deserialize;
use serde_json::{Value, json};

use super::ApiError;
use crate::assembly::{from_rows, matrix_rows, matrix4};
use crate::document::{Document, InsertOptions};
use crate::ids::{BodyUid, ComponentUid, OccurrenceUid};
use crate::kernel::Kernel;
use crate::transform::Transform;

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

#[derive(Debug, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum ComponentCommand {
    CreateComponent {
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        transform: Option<TransformInput>,
        #[serde(default = "yes")]
        activate: bool,
    },
    ComponentsFromBodies {
        bodies: Vec<BodyUid>,
    },
    ActivateComponent {
        component: String,
    },
    RenameComponent {
        component: String,
        name: String,
    },
    GroundOccurrence {
        occurrence: String,
        #[serde(default = "yes")]
        grounded: bool,
    },
    SetOccurrenceVisible {
        occurrence: String,
        visible: bool,
    },
    SetOccurrenceTransform {
        occurrence: String,
        transform: TransformInput,
        #[serde(default)]
        capture: bool,
    },
    CopyOccurrence {
        occurrence: String,
        #[serde(default)]
        transform: Option<TransformInput>,
    },
    PasteNew {
        occurrence: String,
        #[serde(default)]
        transform: Option<TransformInput>,
    },
    DeleteOccurrence {
        occurrence: String,
    },
    InsertComponent {
        /// A project file; or `library` (mitcad#64).
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
    },
    UpdateLinks {
        #[serde(default)]
        base: Option<String>,
    },
}

fn yes() -> bool {
    true
}

/// The commands' names, to send them here.
pub(crate) const COMMANDS: [&str; 12] = [
    "create_component",
    "components_from_bodies",
    "activate_component",
    "rename_component",
    "ground_occurrence",
    "set_occurrence_visible",
    "set_occurrence_transform",
    "copy_occurrence",
    "paste_new",
    "delete_occurrence",
    "insert_component",
    "update_links",
];

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

impl<K: Kernel> Document<K> {
    /// A component by uid (`C2`, `C0` the root) or name.
    pub(crate) fn component_ref(&self, text: &str) -> Result<ComponentUid, ApiError> {
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

    /// An occurrence by uid (`O3`), name (`Component1:2`) or path from the
    /// root (`Arm:1/Pin:2`, `O1/O4`: its last occurrence).
    pub(crate) fn occurrence_ref(&self, text: &str) -> Result<OccurrenceUid, ApiError> {
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

    pub(crate) fn run_component_command(
        &mut self,
        command: ComponentCommand,
    ) -> Result<Value, ApiError> {
        Ok(match command {
            ComponentCommand::CreateComponent {
                name,
                transform,
                activate,
            } => {
                let transform = optional(transform.as_ref())?.unwrap_or(Transform::IDENTITY);
                let (component, occurrence) =
                    self.create_component(name.as_deref(), transform, activate)?;
                json!({"component": component, "occurrence": occurrence,
                       "name": self.assembly().name(component)})
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
            ComponentCommand::ActivateComponent { component } => {
                let uid = self.component_ref(&component)?;
                self.activate_component(uid)?;
                json!({"component": uid})
            }
            ComponentCommand::RenameComponent { component, name } => {
                let uid = self.component_ref(&component)?;
                self.rename_component(uid, &name)?;
                json!({})
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
            } => {
                let uid = self.occurrence_ref(&occurrence)?;
                self.set_occurrence_visible(uid, visible)?;
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
                let deleted = self.delete_occurrence(uid)?;
                json!({"deleted": deleted})
            }
            ComponentCommand::InsertComponent {
                path,
                library,
                link,
                transform,
                name,
                base,
            } => {
                let options = InsertOptions {
                    link,
                    transform: optional(transform.as_ref())?.unwrap_or(Transform::IDENTITY),
                    name,
                    base: base.map(PathBuf::from),
                };
                let (component, occurrence) = match (path, library) {
                    (Some(path), None) => self.insert_component(&path, &options)?,
                    (None, Some(library)) => {
                        self.insert_library_component(&library.part(), &options)?
                    }
                    _ => {
                        return Err(ApiError(
                            "insert_component needs a path or a library part, not both".to_owned(),
                        ));
                    }
                };
                json!({"component": component, "occurrence": occurrence,
                       "name": self.assembly().name(component)})
            }
            ComponentCommand::UpdateLinks { base } => {
                let base = base.map(PathBuf::from);
                let messages = self.update_links(base.as_deref())?;
                json!({"messages": messages})
            }
        })
    }

    /// The `components` query: the components, the occurrence tree and
    /// the active component.
    pub(crate) fn components_json(&self) -> Value {
        let a = self.assembly();
        let component = |uid: ComponentUid| {
            let def = a.component(uid);
            let features: Vec<_> = self
                .features()
                .filter(|f| f.component == uid)
                .map(|f| f.uid)
                .collect();
            let bodies = self.component_bodies(uid);
            json!({
                "uid": uid,
                "name": a.name(uid),
                "created_by": def.and_then(|d| d.created_by),
                "link": def.and_then(|d| d.link.as_ref()).map(|l| l.path.clone()),
                "library": def.and_then(|d| d.library.as_ref()),
                "features": features,
                "bodies": bodies,
                "occurrences": a.occurrences_of(uid).count(),
            })
        };
        let mut components = vec![component(ComponentUid::ROOT)];
        components.extend(a.components.iter().map(|c| component(c.uid)));
        json!({
            "active": a.active,
            "components": components,
            "occurrences": self.occurrence_tree(ComponentUid::ROOT, &[], Transform::IDENTITY),
        })
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
            .map(|o| {
                let mut path = path.to_vec();
                path.push(o.uid);
                let placement = self.placement(o.uid).unwrap_or(o.transform);
                let world = at.after(&placement);
                json!({
                    "uid": o.uid,
                    "name": a.occurrence_name(o.uid),
                    "path": a.path_name(&path),
                    "component": o.component,
                    "transform": matrix_rows(&placement),
                    "world": matrix4(&world),
                    "grounded": o.grounded,
                    "visible": o.visible,
                    "children": self.occurrence_tree(o.component, &path, world),
                })
            })
            .collect()
    }

    /// The `instances` query: the bodies as placed in the design.
    pub(crate) fn instances_json(&self, query: &InstancesQuery) -> Result<Value, ApiError> {
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
            if query.properties {
                let error = |e: crate::kernel::KernelError| ApiError(format!("{}: {e}", i.name));
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
}
