// SPDX-License-Identifier: MIT
//! Inserted components with options (mitcad#801; `commands.md`, "Options
//! of inserted components"; the rules' side is `crate::part_options`).
//!
//! A library part or a linked design inserted with `parameters` is made
//! by a derived part of a source the design keeps (`library_sources`, as
//! BUILD keeps its parts' sources), so its values, expressions of the
//! design's parameters, are evaluated at recompute: a change of a
//! parameter it uses recomputes that part only. Here: inserting such a
//! component, changing its options afterwards (a BUILD part's too), what
//! the queries say of them, and their updates (another version or row of
//! a library part, a linked file that changed).

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde_json::Value;

use super::components::{check_transform, design_digest, file_label, resolve_path};
use super::mounts::{Inserted, MountFrame, listed_mounts};
use super::{DocState, Document, InsertOptions, LibraryChange, LibraryPart, ModelError, invalid};
use crate::assembly::ExternalLink;
use crate::build::{LibrarySourceDef, SourceDesign, SourceId};
use crate::expr::{Expr, ParamKind, Unit};
use crate::features::derived_part::{DerivedPartDef, Ends};
use crate::features::geom_ref::{GeomRef, valid_mount_name};
use crate::features::joint::{JointOriginDef, MOUNT_PREFIX, MountSpec};
use crate::features::{FeatureDef, Operation, ValueInput};
use crate::ids::{ComponentUid, FeatureUid};
use crate::kernel::Kernel;
use crate::library::{LibraryRef, LibraryRequest};
use crate::parameters::ParamId;
use crate::part_options::{
    self, BrokenRule, Combination, ConfiguredSource, Naming, OptionMove, PartValue,
};
use crate::policy::Policy;
use mitcad_model_base::internal::values::split_named;

/// A change of an inserted component's options (`set_component_parameters`).
#[derive(Debug, Clone, PartialEq)]
pub struct OptionsChange {
    pub component: ComponentUid,
    /// New values by option: an expression of the design's parameters or a
    /// number; None removes the component's own value (the row's holds).
    pub parameters: BTreeMap<String, Option<ValueInput>>,
    /// Another configuration row to start from (not for a BUILD part, whose
    /// row is `build_edit`'s).
    pub config: Option<String>,
}

/// What a change of options did.
#[derive(Debug, Clone, PartialEq)]
pub struct OptionsChanged {
    pub component: ComponentUid,
    /// The derived part that makes it.
    pub part: FeatureUid,
    /// The constants the source's rules moved (and that the part now keeps).
    pub moves: Vec<OptionMove>,
    /// The rule a bound value breaks: the part fails with it.
    pub broken: Option<BrokenRule>,
}

/// An inserted component's options as the queries show them.
#[derive(Debug, Clone)]
pub struct ComponentOptions {
    pub component: ComponentUid,
    /// The derived part that makes it, and its source.
    pub part: FeatureUid,
    pub source: SourceId,
    /// The row it starts from (the source's default row when None).
    pub config: Option<String>,
    /// Its values as written, by option: the part's own expression, or the
    /// name of the design's parameter it uses.
    pub parameters: BTreeMap<String, String>,
    /// The options bound to the design's parameters.
    pub bound: BTreeSet<String>,
    /// The source's design in its combination (None when it cannot be
    /// made: then `error` says why).
    pub configured: Option<ConfiguredSource>,
    pub combination: Combination,
    pub error: Option<String>,
}

impl DocState {
    /// The derived part that makes a component with options: a BUILD
    /// part's, or the part of a component inserted with `parameters`.
    pub(crate) fn options_part(&self, component: ComponentUid) -> Option<FeatureUid> {
        let c = self.assembly.component(component)?;
        let uid = match (&c.build, &c.link) {
            (Some(record), _) => record.part?,
            (None, Some(link)) => link.feature,
            (None, None) => return None,
        };
        matches!(self.entry(uid)?.def, FeatureDef::DerivedPart(_)).then_some(uid)
    }
}

/// How a source names its parts: a library's pattern and standard; a BUILD
/// part's default is the name and the row, as BUILD names them.
fn naming(source: &LibrarySourceDef, build: bool) -> Naming<'_> {
    Naming {
        pattern: &source.library.designation,
        standard: &source.standard,
        name: &source.name,
        fallback: if build || source.standard.trim().is_empty() {
            "{name} {config}"
        } else {
            "{standard} {config}"
        },
        // A source in the design keeps no rows' standards (mitcad#949):
        // its pattern's `{standard}` is the component's.
        rows: None,
        chosen: "",
    }
}

/// A library part's record for its combination: the source's library, the
/// row, the designation, and the row's part number and page when it has
/// its own.
fn record_of(source: &LibrarySourceDef, combination: &Combination) -> LibraryRef {
    let mut reference = source.library.clone();
    reference.config = combination.row.clone();
    reference.designation = combination.designation.clone();
    if let Some(number) = &combination.part_number {
        reference.part_number = number.clone();
    }
    if let Some(url) = &combination.supplier_url {
        reference.supplier_url = url.clone();
    }
    reference
}

/// A derived part in command form: its values by their parameters' names.
fn part_input(state: &DocState, def: &DerivedPartDef) -> DerivedPartDef<ValueInput> {
    def.map_params(&mut |_, id: &ParamId| Ok::<_, ()>(ValueInput::Name(state.parameters.name(*id))))
        .expect("names never fail")
}

/// A derived part's definition.
fn part_def(state: &DocState, part: FeatureUid) -> Result<DerivedPartDef, ModelError> {
    match state.entry(part).map(|e| &e.def) {
        Some(FeatureDef::DerivedPart(def)) => Ok(def.clone()),
        Some(_) => Err(invalid(format!("{part} is not a derived part"))),
        None => Err(ModelError::NoSuchFeature(part)),
    }
}

/// Runs the source's rules on a part's values as the design gives them
/// now: the constants the cascade moves are written into the part (so that
/// it keeps them), a bound value that breaks a rule is left for the part to
/// fail with. Returns the source in its combination.
fn settle_options(state: &mut DocState, part: FeatureUid) -> Result<ConfiguredSource, ModelError> {
    let def = part_def(state, part)?;
    let source = state
        .library_sources
        .get(def.source)
        .cloned()
        .ok_or_else(|| {
            invalid(format!(
                "library source {} is not in the design",
                def.source
            ))
        })?;
    let (values, pinned) = part_options::part_values(&def, &state.parameters, part);
    let configured = part_options::configure(&source, def.config.as_deref(), &values, &pinned)
        .map_err(|e| invalid(format!("{}: {e}", source.label())))?;
    if !configured.moves.is_empty() {
        let mut input = part_input(state, &def);
        for moved in &configured.moves {
            input.parameters.insert(
                moved.option.clone(),
                ValueInput::Name(moved.to_expression.clone()),
            );
        }
        state.edit_feature(part, &FeatureDef::DerivedPart(input))?;
    }
    Ok(configured)
}

/// A library part's record after its options, row or version changed:
/// the combination's row and designation; a part whose name was its old
/// designation takes the new one.
fn refresh_record(state: &mut DocState, component: ComponentUid, configured: &DocState) {
    let Some(c) = state.assembly.component(component) else {
        return;
    };
    let (Some(old), None) = (c.library.clone(), &c.build) else {
        return;
    };
    let Some(part) = state.options_part(component) else {
        return;
    };
    let Ok(def) = part_def(state, part) else {
        return;
    };
    let Some(source) = state.library_sources.get(def.source) else {
        return;
    };
    let combination = part_options::combination(
        configured,
        def.config.as_deref(),
        &naming(source, false),
        &BTreeMap::new(),
    );
    let reference = record_of(source, &combination);
    let rename = !old.designation.is_empty()
        && !reference.designation.is_empty()
        && c.name == old.designation
        && reference.designation != old.designation;
    let new_name = rename.then(|| state.free_component_name(&reference.designation));
    let c = state.assembly.component_mut(component).expect("checked");
    if let Some(name) = new_name {
        c.name = name;
    }
    c.library = Some(reference);
}

/// A linked design as a source the design keeps: written as one file, so
/// that its text holds everything (as a library's source is), named by
/// its path.
fn file_source(path: &str, state: &DocState, stem: &str) -> Result<LibrarySourceDef, ModelError> {
    if crate::file::has_references(state) {
        return Err(invalid(format!(
            "{path}: B-rep data it refers to is missing"
        )));
    }
    Ok(LibrarySourceDef {
        id: SourceId(0),
        library: LibraryRef {
            path: path.to_owned(),
            component: stem.to_owned(),
            ..LibraryRef::default()
        },
        name: stem.to_owned(),
        standard: String::new(),
        design: SourceDesign::new(crate::file::single_file(state)),
        build: None,
        supplier: String::new(),
        part_number: String::new(),
        supplier_url: String::new(),
    })
}

/// The source of the same design the design keeps already, else the next
/// free id.
fn source_id(state: &DocState, source: &LibrarySourceDef) -> SourceId {
    state
        .library_sources
        .iter()
        .find(|s| s.library == source.library && s.design.sha256 == source.design.sha256)
        .map_or_else(|| state.library_sources.next_id(), |s| s.id)
}

/// Adds a part's mount frames to its component: a joint origin on each
/// frame of its derived part (`F7:mount(name)`), so that the frames follow
/// its options.
fn add_part_mounts(
    state: &mut DocState,
    component: ComponentUid,
    part: FeatureUid,
    specs: &[MountSpec],
) -> Result<Vec<MountFrame>, ModelError> {
    let mut taken: BTreeSet<String> = state.features.iter().map(|f| f.name.clone()).collect();
    let mut frames = Vec::with_capacity(specs.len());
    for spec in specs {
        let name =
            crate::exchange::unique_name(&format!("{MOUNT_PREFIX}{}", spec.name), &mut taken);
        let def = FeatureDef::JointOrigin(JointOriginDef::<ValueInput> {
            geometry: GeomRef::Mount {
                feature: part,
                name: spec.name.clone(),
            },
            frame_override: None,
            offset: None,
            angle: None,
            flip: false,
            mount: Some(spec.clone()),
        });
        let added = state.add_feature_in(&def, Some(&name), component)?;
        frames.push(MountFrame {
            spec: spec.clone(),
            path: Vec::new(),
            feature: added.uid,
        });
    }
    Ok(frames)
}

/// What an insert with options reads first: the source, the row it starts
/// from, whether it is a library's, the root policy of a linked design,
/// the link's path and digest, and the undo label's name.
struct OptionsInsert {
    source: LibrarySourceDef,
    config: Option<String>,
    library: bool,
    policy: Option<Policy>,
    path: String,
    digest: String,
    label: Option<String>,
}

impl<K: Kernel> Document<K> {
    /// Inserts a library's component with options (`insert_component` with
    /// `library` and `parameters`).
    pub(super) fn insert_library_with_options(
        &mut self,
        part: &LibraryPart,
        options: &InsertOptions,
        parameters: &BTreeMap<String, ValueInput>,
    ) -> Result<Inserted, ModelError> {
        let request = LibraryRequest {
            library: part.library.clone(),
            url: part.url.clone(),
            rev: part.rev.clone(),
            component: part.component.clone(),
        };
        let read = self.library_source(&request).map_err(invalid)?;
        let source = LibrarySourceDef::from_library(SourceId(0), &read).map_err(invalid)?;
        let digest = design_digest(&read.text);
        let insert = OptionsInsert {
            source,
            config: part.config.clone(),
            library: true,
            policy: None,
            path: read.path.clone(),
            digest,
            label: None,
        };
        self.insert_with_options(insert, options, parameters)
    }

    /// Inserts a design file linked with options (`insert_component` with
    /// `path` and `parameters`).
    pub(super) fn insert_file_with_options(
        &mut self,
        path: &str,
        options: &InsertOptions,
        parameters: &BTreeMap<String, ValueInput>,
    ) -> Result<Inserted, ModelError> {
        let resolved = std::path::absolute(resolve_path(path, options.base.as_deref()))
            .map_err(|e| invalid(format!("cannot resolve {path}: {e}")))?;
        let text = std::fs::read_to_string(&resolved)
            .map_err(|e| invalid(format!("cannot read {path}: {e}")))?;
        let (design, _) = crate::file::load_state_at(&text, &resolved)
            .map_err(|e| invalid(format!("{path}: {e}")))?;
        let stem = file_stem(path);
        let insert = OptionsInsert {
            source: file_source(path, &design, &stem)?,
            config: None,
            library: false,
            policy: Some(design.assembly.root_policy),
            path: path.to_owned(),
            digest: design_digest(&text),
            label: Some(file_label(path)),
        };
        self.insert_with_options(insert, options, parameters)
    }

    fn insert_with_options(
        &mut self,
        insert: OptionsInsert,
        options: &InsertOptions,
        parameters: &BTreeMap<String, ValueInput>,
    ) -> Result<Inserted, ModelError> {
        check_transform(&options.transform)?;
        let parent = self.state.assembly.active;
        self.state.editable(parent)?;
        if !options.link {
            return Err(invalid(
                "a copy has the design's own parameters: insert it linked to give it options, \
                 or change the copy's parameters",
            ));
        }
        let OptionsInsert {
            mut source,
            config,
            library,
            policy,
            path,
            digest,
            label,
        } = insert;
        source.id = source_id(&self.state, &source);
        let design = source
            .design
            .state()
            .map_err(|e| invalid(format!("{}: {e}", source.label())))?;
        let (values, pinned) = self.trial_values(&design, &source.label(), parameters)?;
        let configured = part_options::configure_state(
            &design,
            &source.label(),
            config.as_deref(),
            &values,
            &pinned,
        )
        .map_err(invalid)?;
        let combination = part_options::combination(
            &configured.state,
            config.as_deref(),
            &naming(&source, false),
            &BTreeMap::new(),
        );
        let wanted = options.name.clone().unwrap_or_else(|| {
            if library && !combination.designation.is_empty() {
                combination.designation.clone()
            } else {
                source.name.clone()
            }
        });
        let specs: Vec<MountSpec> = listed_mounts(&configured.state)
            .into_iter()
            .map(|(_, spec)| spec)
            .filter(|spec| valid_mount_name(&spec.name))
            .collect();
        let plan = match &options.mount {
            Some(mount) => {
                let mounts = self.design_mounts(&configured.state)?;
                Some(self.plan_mount(&mounts, mount, &wanted)?)
            }
            None => None,
        };
        let transform = plan.as_ref().map_or(options.transform, |p| p.placement);
        let reference = library.then(|| record_of(&source, &combination));
        let label = format!("Insert {}", label.unwrap_or_else(|| wanted.clone()));
        let parameters = parameters.clone();
        self.apply(|state| {
            if state.library_sources.get(source.id).is_none() {
                state.library_sources.0.push(source.clone());
            }
            let name = match &options.name {
                Some(name) => {
                    state.check_component_name(name, ComponentUid(u32::MAX))?;
                    name.clone()
                }
                None => state.free_component_name(&wanted),
            };
            let made = state.assembly.create(Some(&name), None);
            if let Some(policy) = policy {
                state.assembly.set_policy(made, policy);
            }
            let occurrence = state.assembly.place(made, parent, transform);
            let def = DerivedPartDef::<ValueInput> {
                source: source.id,
                config,
                parameters,
                ends: Ends { a: None, b: None },
                operation: Operation::NewBody,
                cuts: Vec::new(),
            };
            let added = state.add_feature_in(&FeatureDef::DerivedPart(def), None, made)?;
            let settled = settle_options(state, added.uid)?;
            let frames = add_part_mounts(state, made, added.uid, &specs)?;
            let component = state.assembly.component_mut(made).expect("made");
            component.link = Some(ExternalLink {
                path,
                digest,
                feature: added.uid,
            });
            component.library = reference;
            let joint = match &plan {
                Some(plan) => Some(super::mounts::attach_mount(
                    state, parent, occurrence, &frames, plan,
                )?),
                None => None,
            };
            let inserted = Inserted {
                component: made,
                occurrence,
                joint,
                frames,
                plan,
                moves: settled.moves,
                broken: settled.broken,
            };
            Ok((label, inserted))
        })
    }

    /// The values an insert gives its source, evaluated in the design's
    /// parameters now, and the names of those bound to them.
    fn trial_values(
        &self,
        design: &DocState,
        label: &str,
        parameters: &BTreeMap<String, ValueInput>,
    ) -> Result<TrialValues, ModelError> {
        let params = &self.state.parameters;
        let mut values = Vec::with_capacity(parameters.len());
        let mut pinned = BTreeSet::new();
        for (name, input) in parameters {
            let param = design
                .parameters
                .find(name)
                .and_then(|id| design.parameters.get(id))
                .ok_or_else(|| invalid(format!("{label} has no parameter {name}")))?;
            if param.kind() != ParamKind::User {
                return Err(invalid(format!(
                    "{name} of {label} is not a user parameter: a component's options are its \
                     source's user parameters"
                )));
            }
            let (value, bound) = match input {
                ValueInput::Number(number) => (PartValue::Number(*number), false),
                ValueInput::Name(text) => {
                    let expression = split_named(text)
                        .map_or(text.as_str(), |(_, rest)| rest)
                        .trim();
                    let mut trial = params.clone();
                    let id = match params.find(expression) {
                        Some(id) => id,
                        None => {
                            let unit = param.unit();
                            let unit = if !unit.is_text() && unit.dims() == Unit::MM.dims() {
                                Unit::of_length(params.context().default_length_unit)
                            } else {
                                unit
                            };
                            trial
                                .add_dimension(expression, unit, "", None)
                                .map_err(|e| invalid(format!("{name}: {e}")))?
                        }
                    };
                    let value = match trial.text(id) {
                        Some(text) => PartValue::Text(text.to_owned()),
                        None => PartValue::Number(trial.value(id).unwrap_or(f64::NAN)),
                    };
                    (
                        value,
                        !Expr::parse(expression).is_ok_and(|e| e.is_constant()),
                    )
                }
            };
            if bound {
                pinned.insert(name.clone());
            }
            values.push((name.clone(), value));
        }
        Ok((values, pinned))
    }

    /// Changes an inserted component's options (`set_component_parameters`):
    /// its values (expressions of the design's parameters) and its row, as
    /// one undo step (`Change Options of Shelf board`). The source's rules
    /// move its constants (`moves`); a bound value that breaks a rule is
    /// kept, and the part fails with the rule (`broken`).
    pub fn set_component_parameters(
        &mut self,
        change: &OptionsChange,
    ) -> Result<OptionsChanged, ModelError> {
        let component = change.component;
        let c = self
            .state
            .assembly
            .component(component)
            .ok_or_else(|| invalid(format!("component {component} does not exist")))?;
        let name = c.name.clone();
        let build = c.build.is_some();
        let part = self.state.options_part(component).ok_or_else(|| {
            invalid(format!(
                "{name} has no options: insert a library part or a linked design with \
                 `parameters`, or place a BUILD part"
            ))
        })?;
        if change.parameters.is_empty() && change.config.is_none() {
            return Err(invalid("set_component_parameters: nothing to change"));
        }
        if build && change.config.is_some() {
            return Err(invalid(format!(
                "{name} is a BUILD part: its row is build_edit's config"
            )));
        }
        let def = part_def(&self.state, part)?;
        let source = self.state.library_sources.get(def.source).ok_or_else(|| {
            invalid(format!(
                "library source {} is not in the design",
                def.source
            ))
        })?;
        let label = source.label();
        let design = source
            .design
            .state()
            .map_err(|e| invalid(format!("{label}: {e}")))?;
        for (option, value) in &change.parameters {
            let param = design
                .parameters
                .find(option)
                .and_then(|id| design.parameters.get(id))
                .ok_or_else(|| invalid(format!("{label} has no parameter {option}")))?;
            if value.is_some() && param.kind() != ParamKind::User {
                return Err(invalid(format!(
                    "{option} of {label} is not a user parameter"
                )));
            }
        }
        if let Some(row) = &change.config
            && design.configurations.row(row).is_none()
        {
            return Err(invalid(format!(
                "{label} has no configuration {row} (its rows: {})",
                super::library_parts::row_names(&design.configurations)
            )));
        }
        let undo = format!("Change Options of {name}");
        self.apply(|state| {
            let def = part_def(state, part)?;
            let mut input = part_input(state, &def);
            for (option, value) in &change.parameters {
                match value {
                    Some(value) => {
                        input.parameters.insert(option.clone(), value.clone());
                    }
                    None => {
                        input.parameters.remove(option);
                    }
                }
            }
            if let Some(row) = &change.config {
                input.config = Some(row.clone());
            }
            state.edit_feature(part, &FeatureDef::DerivedPart(input))?;
            let settled = settle_options(state, part)?;
            refresh_record(state, component, &settled.state);
            let changed = OptionsChanged {
                component,
                part,
                moves: settled.moves,
                broken: settled.broken,
            };
            Ok((undo, changed))
        })
    }

    /// An inserted component's options (one with a derived part: inserted
    /// with `parameters`, or a BUILD part), None for another component.
    pub fn component_options(&self, component: ComponentUid) -> Option<ComponentOptions> {
        let state = &self.state;
        let part = state.options_part(component)?;
        let def = part_def(state, part).ok()?;
        let source = state.library_sources.get(def.source)?;
        let params = &state.parameters;
        let parameters = def
            .parameters
            .iter()
            .map(|(name, id)| {
                let text = if params.owner(*id) == Some(part) {
                    params
                        .get(*id)
                        .map(|p| p.expression().to_owned())
                        .unwrap_or_default()
                } else {
                    params.name(*id)
                };
                (name.clone(), text)
            })
            .collect();
        let (values, bound) = part_options::part_values(&def, params, part);
        let numbers: BTreeMap<String, f64> = values
            .iter()
            .filter_map(|(name, value)| match value {
                PartValue::Number(n) => Some((name.clone(), *n)),
                PartValue::Text(_) => None,
            })
            .collect();
        let build = state
            .assembly
            .component(component)
            .is_some_and(|c| c.build.is_some());
        let (configured, error) =
            match part_options::configure(source, def.config.as_deref(), &values, &bound) {
                Ok(configured) => {
                    let error = configured.broken.as_ref().map(|b| b.text.clone());
                    (Some(configured), error)
                }
                Err(e) => (None, Some(e)),
            };
        let combination = configured
            .as_ref()
            .map(|c| {
                part_options::combination(
                    &c.state,
                    def.config.as_deref(),
                    &naming(source, build),
                    &numbers,
                )
            })
            .unwrap_or_default();
        Some(ComponentOptions {
            component,
            part,
            source: def.source,
            config: def.config.clone(),
            parameters,
            bound,
            configured,
            combination,
            error,
        })
    }

    /// The `configure` query of an inserted component (mitcad#801): the
    /// choosers' view of its options with its values, `overrides` (by
    /// option, expressions of the design's parameters) tried instead of
    /// its own. Its bound options are shown driven; nothing changes.
    pub fn component_configure_json(
        &self,
        component: ComponentUid,
        overrides: &BTreeMap<String, ValueInput>,
    ) -> Result<Value, ModelError> {
        let name = self.state.assembly.name(component);
        let part = self
            .state
            .options_part(component)
            .ok_or_else(|| invalid(format!("{name} has no options")))?;
        let def = part_def(&self.state, part)?;
        let source = self.state.library_sources.get(def.source).ok_or_else(|| {
            invalid(format!(
                "library source {} is not in the design",
                def.source
            ))
        })?;
        let label = source.label();
        let design = source
            .design
            .state()
            .map_err(|e| invalid(format!("{label}: {e}")))?;
        let (mut values, mut pinned) =
            part_options::part_values(&def, &self.state.parameters, part);
        let (tried, tried_bound) = self.trial_values(&design, &label, overrides)?;
        values.retain(|(name, _)| !overrides.contains_key(name));
        values.extend(tried);
        pinned.retain(|name| !overrides.contains_key(name));
        pinned.extend(tried_bound);
        let mut expressions: BTreeMap<String, String> = self
            .component_options(component)
            .map(|o| o.parameters)
            .unwrap_or_default();
        for (option, value) in overrides {
            let text = match value {
                ValueInput::Number(n) => crate::expr::format_number_exact(*n),
                ValueInput::Name(text) => text.clone(),
            };
            expressions.insert(option.clone(), text);
        }
        let build = self
            .state
            .assembly
            .component(component)
            .is_some_and(|c| c.build.is_some());
        part_options::configure_json(
            &design,
            &label,
            def.config.as_deref(),
            &values,
            &pinned,
            &expressions,
            Some(&naming(source, build)),
        )
        .map_err(invalid)
    }

    /// A component with options in the parts list (not a BUILD part's,
    /// which `build_list_entry` gives): its library record for the
    /// combination of its options.
    pub(crate) fn options_list_entry(
        &self,
        component: ComponentUid,
    ) -> Option<crate::build::part_info::ListEntry> {
        let c = self.state.assembly.component(component)?;
        if c.build.is_some() {
            return None;
        }
        let library = c.library.as_ref()?;
        let options = self.component_options(component)?;
        let source = self.state.library_sources.get(options.source)?;
        let mut reference = record_of(source, &options.combination);
        // Its version as recorded on the component.
        reference.rev.clone_from(&library.rev);
        reference.label.clone_from(&library.label);
        Some(crate::build::part_info::ListEntry {
            designation: reference.designation.clone(),
            part_number: reference.part_number.clone(),
            library: reference,
        })
    }

    /// Linked library parts with options to another version or row
    /// (`update_library_parts`): each takes the source at the version, its
    /// derived part the row; one undo step, one line per part. Parts
    /// without options are refused here: change them apart.
    pub(super) fn update_options_parts(
        &mut self,
        changes: &[LibraryChange],
    ) -> Result<Vec<String>, ModelError> {
        let mut planned = Vec::with_capacity(changes.len());
        for change in changes {
            let c = self
                .state
                .assembly
                .component(change.component)
                .ok_or_else(|| invalid(format!("component {} does not exist", change.component)))?;
            let part = self.state.options_part(change.component).ok_or_else(|| {
                invalid(format!(
                    "{} has no options: change the parts with options and the others apart",
                    c.name
                ))
            })?;
            let old = c.library.clone().ok_or_else(|| {
                invalid(format!(
                    "{} is not a part of a library (update_links reads a linked file again)",
                    c.name
                ))
            })?;
            let request = LibraryRequest {
                library: old.library.clone(),
                url: old.url.clone(),
                rev: change.rev.clone().unwrap_or_else(|| old.rev.clone()),
                component: old.component.clone(),
            };
            let read = self
                .library_source(&request)
                .map_err(|e| invalid(format!("{}: {e}", c.name)))?;
            let source = LibrarySourceDef::from_library(SourceId(0), &read)
                .map_err(|e| invalid(format!("{}: {e}", c.name)))?;
            if let Some(row) = &change.config {
                let design = source
                    .design
                    .state()
                    .map_err(|e| invalid(format!("{}: {e}", c.name)))?;
                if design.configurations.row(row).is_none() {
                    return Err(invalid(format!(
                        "{}: {} has no configuration {row} (its rows: {})",
                        c.name,
                        source.label(),
                        super::library_parts::row_names(&design.configurations)
                    )));
                }
            }
            let mut line = format!("{}:", c.name);
            if read.rev != old.rev {
                let mut new = old.clone();
                new.rev.clone_from(&read.rev);
                new.label.clone_from(&read.label);
                line.push_str(&format!(
                    " version {} -> {}",
                    old.version_text(),
                    new.version_text()
                ));
            }
            if let Some(row) = &change.config {
                line.push_str(&format!(
                    " size {} -> {row}",
                    old.config.as_deref().unwrap_or("-")
                ));
            }
            if line.ends_with(':') {
                line.push_str(" unchanged");
            }
            planned.push((change.component, part, source, change.config.clone(), line));
        }
        let label = match planned.as_slice() {
            [(uid, ..)] => format!("Update {}", self.state.assembly.name(*uid)),
            _ => "Update Library Parts".to_owned(),
        };
        self.apply(|state| {
            let mut lines = Vec::with_capacity(planned.len());
            for (component, part, mut source, config, line) in planned {
                source.id = source_id(state, &source);
                if state.library_sources.get(source.id).is_none() {
                    state.library_sources.0.push(source.clone());
                }
                let def = part_def(state, part)?;
                let mut input = part_input(state, &def);
                input.source = source.id;
                if let Some(row) = config {
                    input.config = Some(row);
                }
                state.edit_feature(part, &FeatureDef::DerivedPart(input))?;
                let settled = settle_options(state, part)?;
                refresh_record(state, component, &settled.state);
                lines.push(line);
            }
            Ok((label, lines))
        })
    }

    /// Linked designs with options whose files changed (part of
    /// `update_links`): each component, its part, the file read again as a
    /// source and the new digest; a line per file that changed or could not
    /// be read.
    pub(super) fn options_file_updates(
        &self,
        base: Option<&Path>,
        messages: &mut Vec<String>,
    ) -> Vec<(ComponentUid, FeatureUid, LibrarySourceDef, String)> {
        let mut updates = Vec::new();
        for c in &self.state.assembly.components {
            let (Some(link), None, None) = (&c.link, &c.library, &c.build) else {
                continue;
            };
            let Some(part) = self.state.options_part(c.uid) else {
                continue;
            };
            let resolved = resolve_path(&link.path, base);
            let text = match std::fs::read_to_string(&resolved) {
                Ok(text) => text,
                Err(e) => {
                    messages.push(format!(
                        "{}: cannot read {} ({e}); it keeps the design saved with the project",
                        c.name, link.path
                    ));
                    continue;
                }
            };
            let digest = design_digest(&text);
            if digest == link.digest {
                continue;
            }
            let read = crate::file::load_state_at(&text, &resolved)
                .map_err(|e| invalid(e.to_string()))
                .and_then(|(design, _)| file_source(&link.path, &design, &file_stem(&link.path)));
            match read {
                Ok(source) => {
                    messages.push(format!("{} is updated from {}", c.name, link.path));
                    updates.push((c.uid, part, source, digest));
                }
                Err(e) => messages.push(format!(
                    "{}: {e}; it keeps the design saved with the project",
                    c.name
                )),
            }
        }
        updates
    }
}

/// Puts the files read again into a state (`update_links`): each part
/// takes its new source, and its link the new digest.
pub(super) fn apply_file_updates(
    state: &mut DocState,
    updates: Vec<(ComponentUid, FeatureUid, LibrarySourceDef, String)>,
) -> Result<(), ModelError> {
    for (component, part, mut source, digest) in updates {
        source.id = source_id(state, &source);
        if state.library_sources.get(source.id).is_none() {
            state.library_sources.0.push(source.clone());
        }
        let def = part_def(state, part)?;
        let mut input = part_input(state, &def);
        input.source = source.id;
        state.edit_feature(part, &FeatureDef::DerivedPart(input))?;
        settle_options(state, part)?;
        if let Some(link) = state
            .assembly
            .component_mut(component)
            .and_then(|c| c.link.as_mut())
        {
            link.digest = digest;
        }
    }
    Ok(())
}

/// A file's name without its extension, for a component's name.
fn file_stem(path: &str) -> String {
    Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| !s.trim().is_empty())
        .unwrap_or("Inserted")
        .to_owned()
}

/// The values a trial gives a component's parameters, and the names of
/// those bound to the design's parameters.
type TrialValues = (Vec<(String, PartValue)>, BTreeSet<String>);
