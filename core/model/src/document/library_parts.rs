// SPDX-License-Identifier: MIT
//! Library components and configuration tables (mitcad#64, mitcad#63):
//! inserting a library's component in a configuration row, linked (its
//! bodies read again from the commit chosen for it) or as a copy; changing
//! a linked part's version or row; the configuration table of a design and
//! applying a row; the parts list of the design (a bill of materials).

use std::collections::BTreeMap;
use std::sync::Arc;

use super::components::{design_digest, file_label};
use super::{DocState, Document, ModelError, invalid};
use crate::assembly::ExternalLink;
use crate::configurations::{ConfigurationRow, Configurations, apply_values};
use crate::features::{BaseBody, BaseDef, FeatureDef};
use crate::ids::{ComponentUid, OccurrenceUid};
use crate::kernel::Kernel;
use crate::library::{LibraryRef, LibraryRequest, LibrarySource, designation};

/// A library component to insert: the library, the version, the
/// component and its configuration row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryPart {
    pub library: String,
    pub url: String,
    /// A commit, or a tag the library's repository has; the commit is
    /// recorded.
    pub rev: String,
    pub component: String,
    /// The row; the table's default when None.
    pub config: Option<String>,
}

/// A change of a linked library part: another version, another row, or
/// both.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryChange {
    pub component: ComponentUid,
    pub rev: Option<String>,
    pub config: Option<String>,
}

/// A row of the parts list ([`Document::parts_list`]).
#[derive(Debug, Clone, PartialEq)]
pub struct PartsListRow {
    pub component: ComponentUid,
    pub name: String,
    /// The designation a library gives the part, else the component's
    /// name.
    pub designation: String,
    /// How many times the design places it (through every occurrence of
    /// the components it is in).
    pub quantity: usize,
    pub linked: bool,
    pub library: Option<LibraryRef>,
}

/// New bodies of a linked component: the component, its link, the new
/// digest and the bodies.
pub(super) type LinkUpdate = (ComponentUid, ExternalLink, String, Vec<BaseBody>);

/// A library component's design in a row: the row applied to a copy of
/// its parameters.
struct Configured {
    state: DocState,
    row: Option<ConfigurationRow>,
}

impl<K: Kernel> Document<K> {
    /// Reads a library component through the resolver.
    fn library_source(&self, request: &LibraryRequest) -> Result<LibrarySource, String> {
        let resolver = self.link_resolver().ok_or_else(|| {
            "no component libraries are set up here (libraries are read by the application \
             and mitcad-cli)"
                .to_owned()
        })?;
        resolver.library_source(request)
    }

    /// The design of a library component with the row `config` (else the
    /// table's default row) applied.
    fn configured(source: &LibrarySource, config: Option<&str>) -> Result<Configured, ModelError> {
        let (mut state, _) = crate::file::load_state_in(&source.text, &source.store)
            .map_err(|e| invalid(format!("{} in {}: {e}", source.path, source.library)))?;
        let table = state.configurations.clone();
        let row = match config {
            Some(name) => Some(table.row(name).cloned().ok_or_else(|| {
                invalid(format!(
                    "{} has no configuration {name} (its rows: {})",
                    source.name,
                    row_names(&table)
                ))
            })?),
            None => table.default_row().cloned(),
        };
        if let Some(row) = &row {
            apply_values(&mut state.parameters, row)
                .map_err(|e| invalid(format!("{} {}: {e}", source.name, row.name)))?;
        }
        Ok(Configured { state, row })
    }

    /// The bodies of a configured library component, named after its
    /// designation when it has one body.
    fn library_bodies(
        &self,
        configured: &Configured,
        designation: &str,
        path: &str,
    ) -> Result<Vec<BaseBody>, ModelError> {
        let mut bodies = self.design_bodies(&configured.state, path)?;
        if bodies.len() == 1 && !designation.is_empty() {
            bodies[0].name = Some(designation.to_owned());
        }
        Ok(bodies)
    }

    /// Inserts a library's component (Insert Component from a library):
    /// linked by default (read only; its bodies read from the commit
    /// recorded for it), or a copy of its design in the row with its
    /// history. The library, the commit, the row, the licence and the
    /// designation are recorded on the new component either way.
    pub fn insert_library_component(
        &mut self,
        part: &LibraryPart,
        options: &super::InsertOptions,
    ) -> Result<(ComponentUid, OccurrenceUid), ModelError> {
        super::components::check_transform(&options.transform)?;
        let parent = self.state.assembly.active;
        self.state.editable(parent)?;
        let request = LibraryRequest {
            library: part.library.clone(),
            url: part.url.clone(),
            rev: part.rev.clone(),
            component: part.component.clone(),
        };
        let source = self.library_source(&request).map_err(invalid)?;
        let configured = Self::configured(&source, part.config.as_deref())?;
        let designation = designation(&source, configured.row.as_ref());
        let reference = library_ref(&source, configured.row.as_ref(), &designation);
        let wanted = options.name.clone().unwrap_or_else(|| {
            if designation.is_empty() {
                source.name.clone()
            } else {
                designation.clone()
            }
        });
        let bodies = if options.link {
            Some(self.library_bodies(&configured, &designation, &source.path)?)
        } else {
            None
        };
        let digest = part_digest(&source, configured.row.as_ref());
        let label = format!("Insert {wanted}");
        self.apply(|state| {
            let name = match &options.name {
                Some(name) => {
                    state.check_component_name(name, ComponentUid(u32::MAX))?;
                    name.clone()
                }
                None => state.free_component_name(&wanted),
            };
            let made = state.assembly.create(Some(&name), None);
            match bodies {
                Some(bodies) => {
                    let base = BaseDef {
                        source: Some(format!("{} {}", source.library, file_label(&source.path))),
                        ..BaseDef::new(bodies)
                    };
                    let single = base.bodies.len() == 1;
                    let added = state.add_feature_in(&FeatureDef::Base(base), None, made)?;
                    if single && !designation.is_empty() {
                        let mut taken = state.body_names.values().cloned().collect();
                        let name = crate::exchange::unique_name(&designation, &mut taken);
                        state
                            .body_names
                            .insert(crate::ids::BodyUid::new(added.uid, 0), name);
                    }
                    let component = state.assembly.component_mut(made).expect("made");
                    component.link = Some(ExternalLink {
                        path: source.path.clone(),
                        digest,
                        feature: added.uid,
                    });
                }
                None => {
                    state.copy_component(
                        &configured.state,
                        ComponentUid::ROOT,
                        Some(made),
                        None,
                        false,
                    )?;
                }
            }
            state.assembly.component_mut(made).expect("made").library = Some(reference);
            let occurrence = state.assembly.place(made, parent, options.transform);
            Ok((label, (made, occurrence)))
        })
    }

    /// Changes linked library parts to other versions or rows, as one undo
    /// step; returns one line per part. A part whose name is its old
    /// designation is renamed after the new one. A copied part does not
    /// follow its library and is refused.
    pub fn update_library_parts(
        &mut self,
        changes: &[LibraryChange],
    ) -> Result<Vec<String>, ModelError> {
        if changes.is_empty() {
            return Err(invalid("no library parts to change"));
        }
        let mut planned = Vec::new();
        for change in changes {
            let component = self
                .state
                .assembly
                .component(change.component)
                .ok_or_else(|| invalid(format!("component {} does not exist", change.component)))?;
            let (Some(old), Some(link)) = (component.library.clone(), component.link.clone())
            else {
                return Err(invalid(match component.library {
                    Some(_) => format!(
                        "{} is a copy of a library part: it does not follow the library; insert \
                         the part again for another version or size",
                        component.name
                    ),
                    None => format!("{} is not a part of a library", component.name),
                }));
            };
            let request = LibraryRequest {
                library: old.library.clone(),
                url: old.url.clone(),
                rev: change.rev.clone().unwrap_or_else(|| old.rev.clone()),
                component: old.component.clone(),
            };
            let source = self
                .library_source(&request)
                .map_err(|e| invalid(format!("{}: {e}", component.name)))?;
            let config = change.config.clone().or_else(|| old.config.clone());
            let configured = Self::configured(&source, config.as_deref())
                .map_err(|e| invalid(format!("{}: {e}", component.name)))?;
            let designation = designation(&source, configured.row.as_ref());
            let reference = library_ref(&source, configured.row.as_ref(), &designation);
            let bodies = self.library_bodies(&configured, &designation, &source.path)?;
            let digest = part_digest(&source, configured.row.as_ref());
            let rename = !old.designation.is_empty()
                && component.name == old.designation
                && designation != old.designation;
            let mut text = format!("{}:", component.name);
            if reference.rev != old.rev {
                text.push_str(&format!(
                    " version {} -> {}",
                    old.version_text(),
                    reference.version_text()
                ));
            }
            if reference.config != old.config {
                text.push_str(&format!(
                    " size {} -> {}",
                    old.config.as_deref().unwrap_or("-"),
                    reference.config.as_deref().unwrap_or("-")
                ));
            }
            if text.ends_with(':') {
                text.push_str(" unchanged");
            }
            planned.push((
                change.component,
                link,
                reference,
                bodies,
                digest,
                rename,
                text,
            ));
        }
        let label = match planned.as_slice() {
            [(uid, _, reference, ..)] => {
                let name = self.state.assembly.name(*uid);
                format!("Update {name} to {}", part_label(reference))
            }
            _ => "Update Library Parts".to_owned(),
        };
        let lines: Vec<String> = planned.iter().map(|p| p.6.clone()).collect();
        self.apply(|state| {
            for (uid, link, reference, bodies, digest, rename, _) in planned {
                let position = state.require(link.feature)?;
                let mut entry = (*state.features[position]).clone();
                if let FeatureDef::Base(base) = &mut entry.def {
                    base.bodies = bodies;
                    base.source = Some(format!(
                        "{} {}",
                        reference.library,
                        file_label(&reference.path)
                    ));
                }
                state.features[position] = Arc::new(entry);
                let body = crate::ids::BodyUid::new(link.feature, 0);
                if rename
                    && state.body_names.get(&body) == Some(&old_designation(&state.assembly, uid))
                {
                    let mut taken = state.body_names.values().cloned().collect();
                    let name = crate::exchange::unique_name(&reference.designation, &mut taken);
                    state.body_names.insert(body, name);
                }
                let new_name = rename.then(|| state.free_component_name(&reference.designation));
                let component = state.assembly.component_mut(uid).expect("checked");
                if let Some(name) = new_name {
                    component.name = name;
                }
                if let Some(l) = component.link.as_mut() {
                    l.digest = digest;
                    l.path = reference.path.clone();
                }
                component.library = Some(reference);
            }
            Ok((label, lines))
        })
    }

    /// Reads the linked library parts again from the commits recorded for
    /// them (part of [`Document::update_links`]): the new bodies of those
    /// whose design or row differs from what was saved, and one message per
    /// part that changed or could not be read.
    pub(super) fn library_link_updates(
        &self,
        messages: &mut Vec<String>,
    ) -> Result<Vec<LinkUpdate>, ModelError> {
        let parts: Vec<(ComponentUid, String, ExternalLink, LibraryRef)> = self
            .state
            .assembly
            .components
            .iter()
            .filter_map(|c| Some((c.uid, c.name.clone(), c.link.clone()?, c.library.clone()?)))
            .collect();
        let mut updates = Vec::new();
        for (uid, name, link, library) in parts {
            let request = LibraryRequest {
                library: library.library.clone(),
                url: library.url.clone(),
                rev: library.rev.clone(),
                component: library.component.clone(),
            };
            let source = match self.library_source(&request) {
                Ok(source) => source,
                Err(e) => {
                    messages.push(format!(
                        "{name}: library {} {} ({}) cannot be read ({e}); it keeps the bodies \
                         saved with the design",
                        library.library,
                        library.version_text(),
                        crate::library::shown_url(&library.url)
                    ));
                    continue;
                }
            };
            let configured = match Self::configured(&source, library.config.as_deref()) {
                Ok(configured) => configured,
                Err(e) => {
                    messages.push(format!(
                        "{name}: {e}; it keeps the bodies saved with the design"
                    ));
                    continue;
                }
            };
            let digest = part_digest(&source, configured.row.as_ref());
            if digest == link.digest {
                continue;
            }
            match self.library_bodies(&configured, &library.designation, &source.path) {
                Ok(bodies) => {
                    messages.push(format!(
                        "{name} is updated from {} {}",
                        library.library,
                        library.version_text()
                    ));
                    updates.push((uid, link, digest, bodies));
                }
                Err(ModelError::Cancelled) => return Err(ModelError::Cancelled),
                Err(e) => messages.push(format!(
                    "{name}: {e}; it keeps the bodies saved with the design"
                )),
            }
        }
        Ok(updates)
    }

    /// The design's configuration table (empty when it has none).
    pub fn configurations(&self) -> &Configurations {
        &self.state.configurations
    }

    /// Sets the design's configuration table (empty: none), checked
    /// against its parameters; an undo step that recomputes nothing.
    pub fn set_configurations(&mut self, table: Configurations) -> Result<(), ModelError> {
        if let Some(problem) = table.problems(&self.state.parameters).first() {
            return Err(invalid(problem.clone()));
        }
        if table == self.state.configurations {
            return Ok(());
        }
        self.apply_with(|state| {
            state.configurations = table;
            Ok(("Edit Configurations".to_owned(), (), false))
        })
    }

    /// Sets the parameters to a row of the table; returns the parameters
    /// whose values changed.
    pub fn apply_configuration(&mut self, name: &str) -> Result<Vec<String>, ModelError> {
        let row = self
            .state
            .configurations
            .row(name)
            .cloned()
            .ok_or_else(|| {
                invalid(format!(
                    "the design has no configuration {name} (its rows: {})",
                    row_names(&self.state.configurations)
                ))
            })?;
        self.apply_with(|state| {
            let changed = apply_values(&mut state.parameters, &row).map_err(invalid)?;
            let recompute = !changed.is_empty();
            Ok((format!("Apply {name}"), changed, recompute))
        })
    }

    /// The parts of the design (its components other than the root) with
    /// how many times each is placed, a library part under its
    /// designation; in the order the components were made.
    pub fn parts_list(&self) -> Vec<PartsListRow> {
        let a = &self.state.assembly;
        // Placements of each component in the design: an occurrence counts
        // as many times as its parent is placed (the root once).
        fn placed(
            a: &crate::assembly::Assembly,
            c: ComponentUid,
            memo: &mut BTreeMap<ComponentUid, usize>,
            depth: usize,
        ) -> usize {
            if c.is_root() {
                return 1;
            }
            if let Some(n) = memo.get(&c) {
                return *n;
            }
            let parents: Vec<ComponentUid> = a.occurrences_of(c).map(|o| o.parent).collect();
            let n = if depth > 64 {
                0
            } else {
                parents
                    .into_iter()
                    .map(|p| placed(a, p, memo, depth + 1))
                    .sum()
            };
            memo.insert(c, n);
            n
        }
        let mut memo = BTreeMap::new();
        a.components
            .iter()
            .map(|c| PartsListRow {
                component: c.uid,
                name: c.name.clone(),
                designation: c
                    .library
                    .as_ref()
                    .map(|l| l.designation.clone())
                    .filter(|d| !d.is_empty())
                    .unwrap_or_else(|| c.name.clone()),
                quantity: placed(a, c.uid, &mut memo, 0),
                linked: c.link.is_some(),
                library: c.library.clone(),
            })
            .collect()
    }
}

/// The names of a table's rows, for messages.
fn row_names(table: &Configurations) -> String {
    if table.rows.is_empty() {
        return "none".to_owned();
    }
    let names: Vec<&str> = table.rows.iter().map(|r| r.name.as_str()).collect();
    if names.len() > 12 {
        format!("{}, ... ({} in all)", names[..12].join(", "), names.len())
    } else {
        names.join(", ")
    }
}

/// What a part's link records: the source's text and the row, so that a
/// row whose values changed in another version counts as a change.
fn part_digest(source: &LibrarySource, row: Option<&ConfigurationRow>) -> String {
    let row = row.map_or_else(String::new, |r| {
        serde_json::to_string(r).expect("a row serializes")
    });
    design_digest(&format!("{}\n{row}", source.text))
}

fn library_ref(
    source: &LibrarySource,
    row: Option<&ConfigurationRow>,
    designation: &str,
) -> LibraryRef {
    LibraryRef {
        library: source.library.clone(),
        url: source.url.clone(),
        rev: source.rev.clone(),
        label: source.label.clone(),
        component: source.component.clone(),
        path: source.path.clone(),
        config: row.map(|r| r.name.clone()),
        license: source.license.clone(),
        authors: source.authors.clone(),
        designation: designation.to_owned(),
    }
}

/// A part's version and row for an undo label: `v1.1.0, M5x20`.
fn part_label(reference: &LibraryRef) -> String {
    let version = if reference.label.is_empty() {
        reference.rev.chars().take(7).collect()
    } else {
        reference.label.clone()
    };
    match &reference.config {
        Some(config) => format!("{version}, {config}"),
        None => version,
    }
}

/// The designation a component's library reference has.
fn old_designation(assembly: &crate::assembly::Assembly, uid: ComponentUid) -> String {
    assembly
        .component(uid)
        .and_then(|c| c.library.as_ref())
        .map(|l| l.designation.clone())
        .unwrap_or_default()
}
