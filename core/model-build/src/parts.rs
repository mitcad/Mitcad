// SPDX-License-Identifier: MIT
//! What the BUILD rules read of placed parts: every part, its bodies' box,
//! the BUILD host and the paths from it, library source updates and the
//! appearance a part inherits. A single part's record, derived part,
//! source, metadata, mount frames and length (`PartInfo`) and its
//! designation and part number are read in the model
//! (`mitcad_model::internal::build`) and re-exported here.

use std::collections::{BTreeMap, BTreeSet};

use crate::{LibrarySourceDef, SourceId};
use mitcad_model::expr::{Expr, Unit};
use mitcad_model::features::{FeatureDef, ValueInput};
use mitcad_model::ids::{ComponentUid, FeatureUid, OccurrenceUid};
use mitcad_model::kernel::{BoundingBox, Kernel};
use mitcad_model::parameters::{ParamId, Parameters};
use mitcad_model::part_options::{ConfiguredSource, PartValue};
use mitcad_model::topo::FaceName;
use mitcad_model::transform::Transform;
use mitcad_model::{DocState, Document, ModelError};

// What BUILD reads of a placed part (`Document::part_info`) and what a
// list gives it are in the model, which the parts list and the
// properties use (mitcad#777).
pub use mitcad_model::internal::build::{
    PartInfo, designation, part_number, part_values, typed_section,
};

/// The document's BUILD methods of this block, as an extension trait
/// (mitcad#777): BUILD planning lives in a crate above the model.
pub trait PartsBuild<K: Kernel> {
    /// Every BUILD part, in the order the components were made.
    fn build_parts_info(&self) -> Vec<PartInfo>;

    /// The box of a part's bodies at the marker, in its coordinates.
    fn part_bounds(&self, component: ComponentUid) -> Option<BoundingBox>;

    /// The component BUILD places parts in (mitcad#394): the active one
    /// when its policy takes components, else the nearest one up from it
    /// that takes them and can be edited, as the design policy puts a new
    /// component beside a part ("goes beside it", mitcad#292). In a new
    /// design that is its active Component1 itself, which takes components
    /// (mitcad#421). The active component when
    /// none up takes components.
    fn build_host(&self) -> ComponentUid;

    /// The path of a component from the BUILD host
    /// ([`Document::build_host`]): its placement there is the path's.
    fn placed_in_host(&self, component: ComponentUid) -> Option<Vec<OccurrenceUid>>;

    /// The BUILD host's placement in the design.
    fn host_placement(&self) -> Transform;
}

impl<K: Kernel> PartsBuild<K> for Document<K> {
    fn build_parts_info(&self) -> Vec<PartInfo> {
        self.state()
            .assembly
            .components
            .iter()
            .filter_map(|c| self.part_info(c.uid))
            .collect()
    }

    fn part_bounds(&self, component: ComponentUid) -> Option<BoundingBox> {
        let mut all: Option<BoundingBox> = None;
        for body in self.component_bodies(component) {
            let shape = self.body_shape(body)?;
            let Ok(Some(b)) = self.kernel().bounding_box(shape) else {
                continue;
            };
            all = Some(match all {
                Some(a) => a.union(&b),
                None => b,
            });
        }
        all
    }

    fn build_host(&self) -> ComponentUid {
        // A timber frame's own component while it drives its fasteners
        // (mitcad#522).
        if let Some(host) = self
            .build_host_override()
            .filter(|c| self.assembly().exists(*c))
        {
            return host;
        }
        let a = self.assembly();
        let active = a.active;
        if a.policy(active).components {
            return active;
        }
        let Ok(path) = mitcad_model::internal::links::first_path(a, active, None) else {
            return active;
        };
        (0..path.len())
            .rev()
            .find_map(|i| {
                let parent = a.occurrence(path[i])?.parent;
                (a.policy(parent).components && self.state().editable(parent).is_ok())
                    .then_some(parent)
            })
            .unwrap_or(active)
    }

    fn placed_in_host(&self, component: ComponentUid) -> Option<Vec<OccurrenceUid>> {
        let a = self.assembly();
        let host = self.build_host();
        let mut found = None;
        let mut todo: Vec<(ComponentUid, Vec<OccurrenceUid>)> = vec![(host, Vec::new())];
        while let Some((c, path)) = todo.pop() {
            if c == component && !path.is_empty() {
                found = Some(path);
                break;
            }
            if path.len() > 32 {
                continue;
            }
            for o in a.children(c) {
                let mut next = path.clone();
                next.push(o.uid);
                todo.push((o.component, next));
            }
        }
        found
    }

    fn host_placement(&self) -> Transform {
        let a = self.assembly();
        let host = self.build_host();
        if host.is_root() {
            return Transform::IDENTITY;
        }
        a.paths_to(host)
            .first()
            .map_or(Transform::IDENTITY, |path| self.path_transform(path))
    }
}

/// A definition in command form, its values by their parameters' names.
pub(crate) fn as_input(def: &FeatureDef, params: &Parameters) -> FeatureDef<ValueInput> {
    def.map_params(&mut |_, id: &ParamId| Ok::<_, ()>(ValueInput::Name(params.name(*id))))
        .expect("names never fail")
}

/// The source's state in a row with values set, as the derived part makes
/// it (for frames and sizes before the part exists).
pub fn source_state(
    source: &LibrarySourceDef,
    config: Option<&str>,
    values: &[(String, f64)],
) -> Result<DocState, String> {
    mitcad_model::internal::features::configured(source, config, values)
}

/// The chosen options evaluated in the parent document. Text and
/// non-length numeric units follow the source, as derived-part slots do;
/// lengths retain BUILD's millimetre fallback for bare expressions.
#[derive(Debug, Clone, PartialEq)]
pub struct SourceOptionValues {
    pub values: Vec<(String, PartValue)>,
    pub pinned: BTreeSet<String>,
}

impl SourceOptionValues {
    /// The values after the shared cascade, including options it moved.
    /// Validate before caching so the same numbers from a bound request
    /// cannot reuse a constant request that the cascade repaired.
    pub fn normalized(
        &self,
        configured: &ConfiguredSource,
    ) -> Result<Vec<(String, PartValue)>, String> {
        let names: BTreeSet<_> = self
            .values
            .iter()
            .map(|(name, _)| name)
            .chain(configured.moves.iter().map(|m| &m.option))
            .collect();
        let params = &configured.state.parameters;
        names
            .into_iter()
            .map(|name| {
                let id = params
                    .find(name)
                    .ok_or_else(|| format!("there is no parameter {name}"))?;
                let value = match params.text(id) {
                    Some(text) => PartValue::Text(text.to_owned()),
                    None => PartValue::Number(
                        params
                            .value(id)
                            .filter(|n| n.is_finite())
                            .ok_or_else(|| format!("{name}: not a finite value"))?,
                    ),
                };
                Ok((name.clone(), value))
            })
            .collect()
    }
}

/// Reads expressions and records only parent-bound values as pinned,
/// matching a committed derived part's constant/bound distinction.
pub fn source_option_values<K: Kernel>(
    doc: &Document<K>,
    source: &LibrarySourceDef,
    values: &BTreeMap<String, ValueInput>,
) -> Result<SourceOptionValues, ModelError> {
    let design = source.design.state().map_err(ModelError::Invalid)?;
    let params = doc.parameters();
    let mut pinned = BTreeSet::new();
    let values = values
        .iter()
        .map(|(name, value)| {
            let parameter = design
                .parameters
                .find(name)
                .and_then(|id| design.parameters.get(id))
                .ok_or_else(|| ModelError::Invalid(format!("there is no parameter {name}")))?;
            let source_unit = parameter.unit();
            let unit = if !source_unit.is_text() && source_unit.dims() == Unit::MM.dims() {
                Unit::MM
            } else {
                source_unit
            };
            if let ValueInput::Name(text) = value {
                let expression = mitcad_model::internal::values::split_named(text)
                    .map_or(text.as_str(), |(_, e)| e);
                let parsed = Expr::parse(expression)
                    .map_err(|e| ModelError::Invalid(format!("“{text}”: {e}")))?;
                if !parsed.is_constant() {
                    pinned.insert(name.clone());
                }
            }
            let evaluated = match value {
                ValueInput::Name(text) if unit.is_text() => {
                    let expression = mitcad_model::internal::values::split_named(text)
                        .map_or(text.as_str(), |(_, e)| e);
                    let parsed = Expr::parse(expression)
                        .map_err(|e| ModelError::Invalid(format!("“{text}”: {e}")))?;
                    let text = parsed
                        .eval_text(params.table(), &params.context())
                        .map_err(|e| ModelError::Invalid(format!("“{text}”: {e}")))?;
                    PartValue::Text(text)
                }
                _ => PartValue::Number(mitcad_model::internal::build::value_in(doc, value, unit)?),
            };
            Ok((name.clone(), evaluated))
        })
        .collect::<Result<_, ModelError>>()?;
    Ok(SourceOptionValues { values, pinned })
}

/// The source for planning or preview, using the same cascade and bound
/// value refusal as a committed derived part. Constants may be repaired.
pub fn source_state_with_options(
    source: &LibrarySourceDef,
    config: Option<&str>,
    values: &SourceOptionValues,
) -> Result<ConfiguredSource, String> {
    let configured =
        mitcad_model::part_options::configure(source, config, &values.values, &values.pinned)?;
    if let Some(broken) = &configured.broken {
        return Err(broken.text.clone());
    }
    Ok(configured)
}

/// The source's default value of a parameter, in its row.
pub(crate) fn default_value(
    source: &LibrarySourceDef,
    config: Option<&str>,
    name: &str,
) -> Option<f64> {
    let state = source_state(source, config, &[]).ok()?;
    let id = state.parameters.find(name)?;
    state.parameters.value(id)
}

/// The source of an id, or why there is none.
pub(crate) fn source_of(state: &DocState, id: SourceId) -> Result<LibrarySourceDef, String> {
    state
        .library_sources
        .get(id)
        .cloned()
        .ok_or_else(|| format!("library source {id} is not in the design"))
}

/// The document's BUILD methods of this block, as an extension trait
/// (mitcad#777): BUILD planning lives in a crate above the model.
pub trait LibraryPartsBuild<K: Kernel> {
    /// Moves library sources to other versions (`update_library_parts` with
    /// `source`), as one undo step: each takes that version's design and
    /// `build`, and every derived part made from it recomputes. Returns one
    /// line per source.
    fn update_library_sources(
        &mut self,
        changes: &[(SourceId, Option<String>)],
    ) -> Result<Vec<String>, mitcad_model::ModelError>;
}

impl<K: Kernel> LibraryPartsBuild<K> for Document<K> {
    fn update_library_sources(
        &mut self,
        changes: &[(SourceId, Option<String>)],
    ) -> Result<Vec<String>, mitcad_model::ModelError> {
        use crate::place::refuse;
        if changes.is_empty() {
            return Err(refuse("no library sources to change"));
        }
        let resolver = self
            .link_resolver()
            .ok_or_else(|| refuse("no component libraries are set up here"))?;
        let mut planned = Vec::new();
        for (id, rev) in changes {
            let old = source_of(self.state(), *id).map_err(refuse)?;
            let read = resolver
                .library_source(&mitcad_model::library::LibraryRequest {
                    library: old.library.library.clone(),
                    url: old.library.url.clone(),
                    rev: rev.clone().unwrap_or_else(|| old.library.rev.clone()),
                    component: old.library.component.clone(),
                })
                .map_err(|e| refuse(format!("{}: {e}", old.label())))?;
            let new = LibrarySourceDef::from_library(*id, &read).map_err(refuse)?;
            let line = if new.library.rev == old.library.rev && new.design == old.design {
                format!("{}: unchanged", old.label())
            } else {
                format!(
                    "{}: version {} -> {}",
                    old.label(),
                    old.library.version_text(),
                    new.library.version_text()
                )
            };
            planned.push((new, line));
        }
        let label = match planned.as_slice() {
            [(new, _)] => {
                let version = if new.library.label.is_empty() {
                    new.library.rev.chars().take(7).collect()
                } else {
                    new.library.label.clone()
                };
                format!("Update {} to {version}", new.label())
            }
            _ => "Update Library Components".to_owned(),
        };
        let lines: Vec<String> = planned.iter().map(|(_, l)| l.clone()).collect();
        self.apply(|state| {
            for (new, _) in planned {
                if let Some(source) = state.library_sources.get_mut(new.id) {
                    *source = new;
                }
            }
            Ok((label, lines))
        })
    }
}

/// Gives a derived part's bodies their source's appearance and material
/// (mitcad#478: a timber member shows its wood), for a part whose library
/// block names a `material`: when the source's bodies
/// that set one all set the same, every body the part makes takes it (the
/// first `count`, at least one), and a document appearance of the source
/// is copied into the design when it has none of that id.
pub(crate) fn inherit_appearance(
    state: &mut DocState,
    part: FeatureUid,
    source: &LibrarySourceDef,
) {
    // Only parts whose library block names a material (timber, sheets,
    // their metal parts): slot profiles look as they did.
    if source
        .build
        .as_ref()
        .is_none_or(|b| b.get("material").is_none())
    {
        return;
    }
    let Ok(design) = source.design.state() else {
        return;
    };
    let shown: Vec<&mitcad_model::BodyAttributes> = design
        .body_attributes
        .values()
        .filter(|a| a.visible)
        .collect();
    let same = |pick: &dyn Fn(&mitcad_model::BodyAttributes) -> Option<String>| {
        let values: Vec<String> = shown.iter().filter_map(|a| pick(a)).collect();
        match values.first() {
            Some(first) if values.iter().all(|v| v == first) => Some(first.clone()),
            _ => None,
        }
    };
    let appearance = same(&|a| a.appearance.clone());
    let material = same(&|a| a.material.clone());
    // The ends' look (mitcad#484: a timber member's end grain): the
    // appearance the source's bodies give the caps of their extrusion
    // (`<F>:start(...)`, `<F>:end(...)`, the faces at z = 0 and L), when
    // they give one, goes to the part's end faces, cut or not.
    let caps: BTreeSet<&String> = shown
        .iter()
        .flat_map(|a| a.face_appearances.iter())
        .filter(|(face, _)| {
            face.parse::<FaceName>()
                .is_ok_and(|name| name.role == "start" || name.role == "end")
        })
        .map(|(_, id)| id)
        .collect();
    let ends = match (caps.len(), source.build.as_ref()) {
        (1, Some(build)) if build.get("length").is_some() => caps.first().map(|id| (*id).clone()),
        _ => None,
    };
    if appearance.is_none() && material.is_none() && ends.is_none() {
        return;
    }
    for id in appearance.iter().chain(ends.iter()) {
        if mitcad_model::appearance::library_appearance(id).is_none()
            && !state.appearances.iter().any(|a| &a.id == id)
            && let Some(own) = design.appearances.iter().find(|a| &a.id == id)
        {
            state.appearances.push(own.clone());
        }
    }
    // A material of the library design's own comes with it (mitcad#640).
    let material = match material {
        Some(id) if design.materials.iter().any(|m| m.id == id) => {
            let mapped = state.copy_materials(&design, &BTreeSet::from([id.clone()]));
            Some(mapped.get(&id).cloned().unwrap_or(id))
        }
        other => other,
    };
    let count = shown.len().max(1);
    for i in 0..count {
        let entry = state
            .body_attributes
            .entry(mitcad_model::ids::BodyUid::new(part, i as u32))
            .or_default();
        if entry.appearance.is_none() {
            entry.appearance = appearance.clone();
        }
        if entry.material.is_none() {
            entry.material = material.clone();
        }
        if let Some(id) = &ends {
            for end in ["end_a", "end_b"] {
                entry
                    .face_appearances
                    .entry(FaceName::new(part, end, None).to_string())
                    .or_insert_with(|| id.clone());
            }
        }
    }
}
