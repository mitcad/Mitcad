// SPDX-License-Identifier: MIT
//! Library components and configuration tables (mitcad#64, mitcad#63):
//! inserting a library's component in a configuration row, linked (its
//! bodies read again from the commit chosen for it) or as a copy; changing
//! a linked part's version or row; the configuration table of a design and
//! applying a row; the parts list of the design (a bill of materials).

use std::collections::BTreeMap;
use std::sync::Arc;

use super::components::{LookedParts, design_digest, file_label};
use super::copy::CopySource;
use super::linked_looks::SourceLook;
use super::mounts::{Inserted, SourceMount};
use super::{DocState, Document, ModelError, invalid};
use crate::assembly::ExternalLink;
use crate::configurations::{ConfigurationRow, Configurations};
use crate::features::{BaseBody, BaseDef, FeatureDef};
use crate::ids::{ComponentUid, OccurrenceUid};
use crate::kernel::Kernel;
use crate::library::{
    LibraryRef, LibraryRequest, LibrarySource, SupplierPart, designation, row_standard,
    row_supplier_parts,
};
use mitcad_model_base::internal::configurations::apply_values;

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
    /// The standard the part is named by when its row names several
    /// (mitcad#949: `DIN 912`, the Standard filter's); empty: the row's
    /// first.
    pub standard: String,
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
    /// Who sells it, the article number and the product page (mitcad#375).
    pub supplier: String,
    pub part_number: String,
    pub supplier_url: String,
    /// The suppliers' articles of its row (mitcad#949).
    pub supplier_parts: Vec<SupplierPart>,
}

/// New bodies of a linked component: the component, its link, the new
/// digest, the bodies, the mount frames (mitcad#525) and what the design
/// gives each body (mitcad#1921; none for an envelope).
pub(super) type LinkUpdate = (
    ComponentUid,
    ExternalLink,
    String,
    Vec<BaseBody>,
    Vec<SourceMount>,
    Vec<SourceLook>,
);

/// A library component's design in a row: the row applied to a copy of
/// its parameters.
struct Configured {
    state: DocState,
    row: Option<ConfigurationRow>,
}

impl<K: Kernel> Document<K> {
    /// Reads a library component through the resolver.
    pub(super) fn library_source(&self, request: &LibraryRequest) -> Result<LibrarySource, String> {
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

    /// A library component's design in its row and the name its component
    /// takes (mitcad#525: a mount placement's preview).
    pub(super) fn library_design(
        &self,
        part: &LibraryPart,
    ) -> Result<(DocState, String), ModelError> {
        let request = LibraryRequest {
            library: part.library.clone(),
            url: part.url.clone(),
            rev: part.rev.clone(),
            component: part.component.clone(),
        };
        let source = self.library_source(&request).map_err(invalid)?;
        let configured = Self::configured(&source, part.config.as_deref())?;
        let designation = designation(
            &source,
            &configured.state,
            configured.row.as_ref(),
            &part.standard,
        );
        let name = if designation.is_empty() {
            source.name.clone()
        } else {
            designation
        };
        Ok((configured.state, name))
    }

    /// The bodies of a configured library component, named after its
    /// designation when it has one body, its mount frames (mitcad#525),
    /// and what it gives each body (mitcad#1921: its material and
    /// appearance, the images of its own appearances read from the
    /// library at the commit `source` was read at, `request`'s library).
    fn library_bodies(
        &self,
        configured: &Configured,
        designation: &str,
        source: &LibrarySource,
        request: &LibraryRequest,
    ) -> Result<LookedParts, ModelError> {
        let texture_request = LibraryRequest {
            rev: source.rev.clone(),
            ..request.clone()
        };
        let resolver = self.link_resolver();
        let (mut bodies, mounts, looks) = match &resolver {
            Some(resolver) => {
                let origin = CopySource::Library {
                    resolver: &**resolver,
                    request: &texture_request,
                };
                self.design_parts_looked(&configured.state, &source.path, &origin)?
            }
            None => {
                let (bodies, mounts) = self.design_parts(&configured.state, &source.path)?;
                (bodies, mounts, Vec::new())
            }
        };
        if bodies.len() == 1 && !designation.is_empty() {
            bodies[0].name = Some(designation.to_owned());
        }
        Ok((bodies, mounts, looks))
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
        self.insert_library(part, options)
            .map(|inserted| (inserted.component, inserted.occurrence))
    }

    /// [`Document::insert_library_component`], with the mount frames the
    /// new component has and the joint a mount placement made
    /// (mitcad#525).
    pub fn insert_library(
        &mut self,
        part: &LibraryPart,
        options: &super::InsertOptions,
    ) -> Result<Inserted, ModelError> {
        if let Some(parameters) = &options.parameters {
            return self.insert_library_with_options(part, options, parameters);
        }
        super::components::check_transform(&options.transform)?;
        let parent = self.state.assembly.active;
        self.state.editable(parent)?;
        let request = LibraryRequest {
            library: part.library.clone(),
            url: part.url.clone(),
            rev: part.rev.clone(),
            component: part.component.clone(),
        };
        let resolver = self
            .link_resolver()
            .ok_or_else(|| invalid("no component libraries are set up here"))?;
        let source = resolver.library_source(&request).map_err(invalid)?;
        // Textures come from the same commit even if a tag or HEAD moves.
        let texture_request = LibraryRequest {
            rev: source.rev.clone(),
            ..request
        };
        let configured = Self::configured(&source, part.config.as_deref())?;
        let designation = designation(
            &source,
            &configured.state,
            configured.row.as_ref(),
            &part.standard,
        );
        let reference = library_ref(
            &source,
            configured.row.as_ref(),
            &designation,
            &part.standard,
        );
        let wanted = options.name.clone().unwrap_or_else(|| {
            if designation.is_empty() {
                source.name.clone()
            } else {
                designation.clone()
            }
        });
        // Its mount frames (mitcad#525): a linked part's are joint origins
        // of its own where the design has them; a copy has the design's.
        let (bodies, mounts) = if options.link {
            // With the materials and appearances the design gives its
            // bodies (mitcad#1921).
            let (bodies, mounts, looks) =
                self.library_bodies(&configured, &designation, &source, &texture_request)?;
            (Some((bodies, looks)), mounts)
        } else if options.mount.is_some() {
            (None, self.design_mounts(&configured.state)?)
        } else {
            (None, Vec::new())
        };
        let listed = super::mounts::listed_mounts(&configured.state);
        let plan = match &options.mount {
            Some(mount) => Some(self.plan_mount(&mounts, mount, &wanted)?),
            None => None,
        };
        let transform = plan.as_ref().map_or(options.transform, |p| p.placement);
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
            let occurrence = state.assembly.place(made, parent, transform);
            let frames = match bodies {
                Some((bodies, looks)) => {
                    let base = BaseDef {
                        source: Some(format!("{} {}", source.library, file_label(&source.path))),
                        ..BaseDef::new(bodies)
                    };
                    let single = base.bodies.len() == 1;
                    let added = state.add_feature_in(&FeatureDef::Base(base), None, made)?;
                    state.give_new_link_looks(added.uid, &looks)?;
                    if single && !designation.is_empty() {
                        let mut taken = state.body_names.values().cloned().collect();
                        let name = crate::exchange::unique_name(&designation, &mut taken);
                        state
                            .body_names
                            .insert(crate::ids::BodyUid::new(added.uid, 0), name);
                    }
                    let frames = super::mounts::add_linked_mounts(state, made, &mounts)?;
                    let component = state.assembly.component_mut(made).expect("made");
                    component.link = Some(ExternalLink {
                        path: source.path.clone(),
                        digest,
                        feature: added.uid,
                        envelope: None,
                        frames: Vec::new(),
                        // Its bodies took what the design gives them
                        // (mitcad#1921).
                        looks: true,
                        given: Vec::new(),
                        parts: None,
                        unlinked: Vec::new(),
                        tree: None,
                    });
                    frames
                }
                None => {
                    let (_, copy) = state.copy_component_mapped(
                        &configured.state,
                        &[],
                        made,
                        None,
                        CopySource::Library {
                            resolver: &*resolver,
                            request: &texture_request,
                        },
                    )?;
                    super::mounts::copied_mounts(state, made, &listed, &copy)
                }
            };
            state.assembly.component_mut(made).expect("made").library = Some(reference);
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
                moves: Vec::new(),
                broken: None,
            };
            Ok((label, inserted))
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
        // Parts with options follow their sources (mitcad#801).
        if changes
            .iter()
            .any(|c| self.state.options_part(c.component).is_some())
        {
            return self.update_options_parts(changes);
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
            // The standard it was named by stays where the new row has it
            // (mitcad#949).
            let designation = designation(
                &source,
                &configured.state,
                configured.row.as_ref(),
                &old.standard,
            );
            let reference = library_ref(
                &source,
                configured.row.as_ref(),
                &designation,
                &old.standard,
            );
            let (bodies, mounts, looks) =
                self.library_bodies(&configured, &designation, &source, &request)?;
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
                mounts,
                looks,
            ));
        }
        let label = match planned.as_slice() {
            [(uid, _, reference, ..)] => {
                let name = self.state.assembly.name(*uid);
                format!("Update {name} to {}", part_label(reference))
            }
            _ => "Update Library Components".to_owned(),
        };
        let lines: Vec<String> = planned.iter().map(|p| p.6.clone()).collect();
        self.apply(|state| {
            for (uid, link, reference, mut bodies, digest, rename, _, mounts, looks) in planned {
                // Its mount frames follow the version and row (mitcad#525).
                super::mounts::update_linked_mounts(state, uid, &mounts)?;
                let position = state.require(link.feature)?;
                let mut entry = (*state.features[position]).clone();
                if let FeatureDef::Base(base) = &mut entry.def {
                    // Materials and appearances follow the source (mitcad#1921).
                    state.give_linked_looks(link.feature, &base.bodies, &mut bodies, &looks);
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
                    // Its bodies took the source's materials (mitcad#1921).
                    l.looks = true;
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
            // Parts with options are read from their sources (mitcad#801).
            .filter(|c| self.state.options_part(c.uid).is_none())
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
            // Unchanged: read only for the materials of a link made before
            // them (mitcad#1923); a link shown by its envelope has none.
            let once = digest == link.digest;
            if once && (link.looks || link.envelope.is_some()) {
                continue;
            }
            match self.library_bodies(&configured, &library.designation, &source, &request) {
                Ok((bodies, mounts, looks)) => {
                    if !once {
                        messages.push(format!(
                            "{name} is updated from {} {}",
                            library.library,
                            library.version_text()
                        ));
                    } else if super::linked_looks::gives_any(&looks) {
                        messages.push(format!(
                            "{name} takes the materials of {} {}",
                            library.library,
                            library.version_text()
                        ));
                    }
                    updates.push((uid, link, digest, bodies, mounts, looks));
                }
                Err(ModelError::Cancelled) => return Err(ModelError::Cancelled),
                Err(e) => messages.push(format!(
                    "{name}: {e}; it keeps the bodies saved with the design"
                )),
            }
        }
        Ok(updates)
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
            .map(|c| {
                // A BUILD part (mitcad#375): its source's record; a part
                // with options (mitcad#801): its combination's.
                let build = self
                    .build_list_entry(c.uid)
                    .or_else(|| self.options_list_entry(c.uid));
                let library = build
                    .as_ref()
                    .map(|b| b.library.clone())
                    .or_else(|| c.library.clone());
                PartsListRow {
                    component: c.uid,
                    name: c.name.clone(),
                    designation: build
                        .as_ref()
                        .map(|b| b.designation.clone())
                        .or_else(|| c.library.as_ref().map(|l| l.designation.clone()))
                        .filter(|d| !d.is_empty())
                        // A BUILD end cap's plate (mitcad#2243).
                        .or_else(|| self.end_cap_designation(c.uid))
                        // A part converted to features keeps it (mitcad#1815).
                        .or_else(|| self.stored_text(c.uid, "Designation"))
                        .unwrap_or_else(|| c.name.clone()),
                    quantity: placed(a, c.uid, &mut memo, 0),
                    linked: c.link.is_some(),
                    supplier: library
                        .as_ref()
                        .map(|l| l.supplier.clone())
                        .or_else(|| self.stored_text(c.uid, "Supplier"))
                        .unwrap_or_default(),
                    // The component's own number first, then its entry's
                    // (mitcad#2138); an empty one counts as none.
                    part_number: self
                        .stored_text(c.uid, "Part number")
                        .or_else(|| entry_part_number(build.as_ref(), library.as_ref()))
                        .unwrap_or_default(),
                    supplier_url: library
                        .as_ref()
                        .map(|l| l.supplier_url.clone())
                        .unwrap_or_default(),
                    supplier_parts: library
                        .as_ref()
                        .map(|l| l.supplier_parts.clone())
                        .unwrap_or_default(),
                    library,
                }
            })
            .collect()
    }

    /// The part number a library or BUILD part's entry gives it (its BUILD
    /// or options entry's, else its library's), whatever number of its
    /// own the component has: what the read-only `Library/Part number`
    /// row shows (mitcad#2138). None for another component or an empty
    /// number.
    pub(crate) fn library_part_number(&self, component: ComponentUid) -> Option<String> {
        let build = self
            .build_list_entry(component)
            .or_else(|| self.options_list_entry(component));
        let library = build.as_ref().map(|b| b.library.clone()).or_else(|| {
            self.state
                .assembly
                .component(component)
                .and_then(|c| c.library.clone())
        });
        entry_part_number(build.as_ref(), library.as_ref())
    }

    /// A component's own stored text `key` (of any set): its `Part
    /// number`, which comes first in the parts list (mitcad#2138): one
    /// brought in from the web gets its file name's first part
    /// (mitcad#1579), and a user may give any component one; its
    /// `Designation` and `Supplier`, which a part converted to features
    /// keeps from its library (mitcad#1815).
    fn stored_text(&self, component: ComponentUid, key: &str) -> Option<String> {
        use crate::properties::{PropertyOwner, PropertyValue, same_name};
        self.state
            .properties
            .sets(PropertyOwner::Component(component))
            .iter()
            .flat_map(|group| group.properties.iter())
            .find(|p| same_name(&p.key, key))
            .and_then(|p| match &p.value {
                PropertyValue::Text(text) => Some(text.trim().to_owned()),
                PropertyValue::Integer(number) => Some(number.to_string()),
                _ => None,
            })
            .filter(|text| !text.is_empty())
    }
}

/// The part number of a library or BUILD part's entry: its BUILD or
/// options entry's, else its library's; an empty one counts as none at
/// every step (mitcad#2138).
fn entry_part_number(
    build: Option<&crate::build::part_info::ListEntry>,
    library: Option<&LibraryRef>,
) -> Option<String> {
    let given = |number: &str| Some(number.trim().to_owned()).filter(|n| !n.is_empty());
    build
        .and_then(|b| given(&b.part_number))
        .or_else(|| library.and_then(|l| given(&l.part_number)))
}

/// The names of a table's rows, for messages.
pub(super) fn row_names(table: &Configurations) -> String {
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
    standard: &str,
) -> LibraryRef {
    let row_name = row.map(|r| r.name.as_str());
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
        supplier: source.supplier.clone().unwrap_or_default(),
        part_number: row
            .and_then(|r| r.part_number.clone())
            .or_else(|| source.part_number.clone())
            .unwrap_or_default(),
        supplier_url: row
            .and_then(|r| r.supplier_url.clone())
            .or_else(|| source.supplier_url.clone())
            .unwrap_or_default(),
        standard: row_standard(source, row_name, standard),
        supplier_parts: row_supplier_parts(source, row_name),
        // Not for print services (mitcad#2214): copied at insert and
        // update, so that the design keeps it without the library.
        non_printable: source.non_printable,
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
