// SPDX-License-Identifier: MIT
//! Copies of components with their history (F6): Paste New, and inserting
//! a copy of another design file. The copied features get new uids and
//! default names; references among them follow (feature and body ids,
//! topological names, occurrences), their own parameters are copied with
//! their expressions, and other parameters are shared (the same document)
//! or brought along by name (another file).

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;
use std::sync::Arc;

use serde_json::Value;

use super::{ComponentCopy, DocState, ModelError, invalid};
use crate::appearance::{self, ImageData, Texture};
use crate::assembly::{Assembly, Occurrence};
use crate::expr::ParamSpec;
use crate::features::{
    ChamferSizeDef, CombineTool, FeatureDef, FeatureEntry, FilletSizeDef, OccurrencePath,
};
use crate::ids::{BodyUid, ComponentUid, FeatureUid, OccurrenceUid};
use crate::library::{LibraryRequest, LinkResolver};
use crate::links::OccurrenceLink;
use crate::parameters::ParamId;
use crate::topo::{EdgeName, TopoName};
use crate::transform::Transform;

/// Marks a parameter in the JSON form of a copied definition.
const PARAM: char = '\u{1}';

/// Appearance definitions are shared within a document. Copies from
/// elsewhere keep image paths tied to the source, never the target file.
pub(crate) enum CopySource<'a> {
    SameDocument,
    Document(Option<&'a Path>),
    Library {
        resolver: &'a dyn LinkResolver,
        request: &'a LibraryRequest,
    },
}

impl CopySource<'_> {
    /// Each image of the set (the base colour and the other maps,
    /// mitcad#313) that is a file: an absolute path from another design,
    /// embedded from a library.
    fn texture(&self, texture: &mut Texture) -> Result<(), ModelError> {
        for slot in texture.slots_mut() {
            if slot.data.is_none() {
                self.image(slot.path, slot.data)?;
            }
        }
        Ok(())
    }

    fn image(
        &self,
        image_path: &mut String,
        data: &mut Option<ImageData>,
    ) -> Result<(), ModelError> {
        match self {
            Self::SameDocument => {}
            Self::Document(directory) => {
                let path = Path::new(image_path.as_str());
                if path.is_relative() {
                    let directory = directory.ok_or_else(|| {
                        invalid(format!(
                            "cannot copy relative texture '{image_path}': save the source design \
                             first, or use an absolute path or an embedded image"
                        ))
                    })?;
                    let path = directory.join(path);
                    *image_path = std::path::absolute(&path)
                        .map_err(|e| {
                            invalid(format!("cannot resolve texture {}: {e}", path.display()))
                        })?
                        .to_string_lossy()
                        .into_owned();
                }
            }
            Self::Library { resolver, request } => {
                let bytes = resolver
                    .library_texture(request, image_path)
                    .map_err(invalid)?;
                if bytes.len() > appearance::MAX_EMBEDDED_IMAGE {
                    return Err(invalid("the library texture image is too large to embed"));
                }
                if appearance::image_format(&bytes).is_none() {
                    return Err(invalid("the library texture image is no PNG or JPEG file"));
                }
                *data = Some(ImageData::from_bytes(&bytes));
            }
        }
        Ok(())
    }
}

impl DocState {
    /// Adds a feature with resolved values at the marker: checked, and a
    /// new-component operation makes its component, placed in the
    /// feature's own. Returns the component it made.
    pub(crate) fn insert_resolved(
        &mut self,
        entry: FeatureEntry,
    ) -> Result<Option<ComponentUid>, ModelError> {
        let position = self.marker;
        self.check_feature(position, &entry)
            .map_err(ModelError::Invalid)?;
        let made = entry.def.info().new_component().then(|| {
            // Named after what it makes where the feature says (mitcad#577).
            let name = self.made_component_name(&entry.def);
            let made = self.assembly.create(name.as_deref(), Some(entry.uid));
            self.assembly
                .place(made, entry.component, Transform::IDENTITY);
            made
        });
        let uid = entry.uid;
        self.features.insert(position, Arc::new(entry));
        self.marker += 1;
        self.sync_point_pattern(uid)?;
        Ok(made)
    }

    /// Copies the component at `source_path` of `source` (this document's
    /// state before the copy with `SameDocument`, else another file's)
    /// with its features before the marker, the components placed in it and their
    /// occurrences, at the marker. `into` is already placed in the
    /// destination assembly. With `maker` (the same document only), the
    /// feature that made the source component is copied too and owns
    /// `into`. An empty source path copies another file's root component.
    pub(crate) fn copy_component(
        &mut self,
        source: &DocState,
        source_path: &[OccurrenceUid],
        into: ComponentUid,
        maker: Option<FeatureUid>,
        origin: CopySource<'_>,
    ) -> Result<ComponentUid, ModelError> {
        self.copy_component_mapped(source, source_path, into, maker, origin)
            .map(|(into, _)| into)
    }

    /// [`DocState::copy_component`], also returning the copied occurrences and features
    /// (the source's, each with its copy).
    pub(crate) fn copy_component_mapped(
        &mut self,
        source: &DocState,
        source_path: &[OccurrenceUid],
        into: ComponentUid,
        maker: Option<FeatureUid>,
        origin: CopySource<'_>,
    ) -> Result<(ComponentUid, ComponentCopy), ModelError> {
        let same_document = matches!(origin, CopySource::SameDocument);
        let a = &source.assembly;
        let from =
            crate::joints::path_component(a, ComponentUid::ROOT, source_path).map_err(invalid)?;
        let before_marker: BTreeSet<_> = source.features[..source.marker]
            .iter()
            .map(|f| f.uid)
            .collect();
        let copy_occurrence = |o: &Occurrence| {
            o.pattern
                .as_ref()
                .is_none_or(|p| before_marker.contains(&p.feature))
        };
        // The components to copy: `from` and everything placed in it.
        let mut inside = BTreeSet::from([from]);
        let mut todo = vec![from];
        while let Some(c) = todo.pop() {
            for o in a.children(c) {
                if !copy_occurrence(o) {
                    continue;
                }
                if inside.insert(o.component) {
                    todo.push(o.component);
                }
            }
        }
        let rule_dependencies: BTreeSet<_> = source.features[..source.marker]
            .iter()
            .filter(|f| inside.contains(&f.component) || Some(f.uid) == maker)
            .filter_map(|f| match &f.def {
                FeatureDef::SheetFace(d) => Some(d.rules),
                FeatureDef::SheetContour(d) => Some(d.rules),
                FeatureDef::SheetConvert(d) => Some(d.rules),
                _ => None,
            })
            .collect();
        let features: Vec<&FeatureEntry> = source.features[..source.marker]
            .iter()
            .filter(|f| {
                inside.contains(&f.component)
                    || Some(f.uid) == maker
                    || rule_dependencies.contains(&f.uid)
            })
            .map(|f| &**f)
            .collect();
        let mut uids: HashMap<FeatureUid, FeatureUid> = HashMap::new();
        for f in &features {
            uids.insert(f.uid, self.new_uid());
        }
        let params = self.copy_parameters(source, &features, &uids, same_document)?;

        let mut components: BTreeMap<ComponentUid, ComponentUid> = BTreeMap::new();
        components.insert(from, into);
        if let Some(maker) = maker.and_then(|m| source.entry(m)) {
            components.insert(maker.component, maker.component);
        }
        // Make the whole assembly before validating features: links and
        // joints must already be able to resolve their copied occurrences.
        for c in &inside {
            if !components.contains_key(c) {
                let name = self.free_component_name(&a.name(*c));
                components.insert(*c, self.assembly.create(Some(&name), None));
            }
            if let Some(copy) = self.assembly.component_mut(components[c]) {
                copy.created_by = a
                    .component(*c)
                    .and_then(|d| d.created_by)
                    .and_then(|f| uids.get(&f).copied());
                // A copy keeps the policy; another design's root gives its
                // own (mitcad#220).
                copy.policy = a.policy(*c);
            }
        }
        let mut occurrences: HashMap<OccurrenceUid, OccurrenceUid> = HashMap::new();
        for o in a
            .occurrences
            .iter()
            .filter(|o| inside.contains(&o.parent) && copy_occurrence(o))
        {
            let target =
                self.assembly
                    .place(components[&o.component], components[&o.parent], o.transform);
            set_flags(self.assembly.occurrence_mut(target).expect("placed"), o);
            occurrences.insert(o.uid, target);
        }
        // Their properties, and the bodies' of the copied features
        // (mitcad#496): another design's root gives its own to `into`.
        self.copy_properties(source, &components, &occurrences, &uids);
        // The copied components' drawings come along (mitcad#707).
        let copied: BTreeMap<ComponentUid, ComponentUid> = components
            .iter()
            .filter(|(c, _)| inside.contains(*c))
            .map(|(c, copy)| (*c, *copy))
            .collect();
        self.copy_drawings(source, &copied, &occurrences, &uids);
        let prefix = crate::links::first_path(&self.assembly, into, None).map_err(invalid)?;

        // The features, in timeline order, checked only after all their
        // reference names and occurrence paths have been mapped.
        for f in &features {
            // Rule dependencies can be outside the copied component subtree.
            // Their independent copies precede consumers in this component.
            let component = components.get(&f.component).copied().unwrap_or(into);
            let mut def = copy_def(&f.def, &params, &uids)?;
            let references = CopyReferences {
                source: a,
                destination: &self.assembly,
                source_path,
                prefix: &prefix,
                occurrences: &occurrences,
                uids: &uids,
                same_document,
            };
            references.links(&mut def, &f.def, component)?;
            match &mut def {
                // Root-relative paths were already mapped by links().
                FeatureDef::PatternAtPoints(_) => {}
                FeatureDef::MoveOccurrence(m) => {
                    for o in &mut m.occurrences {
                        *o = occurrences.get(o).copied().unwrap_or(*o);
                    }
                }
                FeatureDef::CapturePosition(c) => {
                    for p in &mut c.positions {
                        p.occurrence = occurrences
                            .get(&p.occurrence)
                            .copied()
                            .unwrap_or(p.occurrence);
                    }
                }
                // Joints between occurrences (mitcad#55).
                def => {
                    crate::joints::remap_occurrences(def, &|o| {
                        occurrences.get(&o).copied().unwrap_or(o)
                    });
                }
            }
            let entry = FeatureEntry {
                uid: uids[&f.uid],
                name: self.default_name(def.base_name()),
                suppressed: f.suppressed,
                component,
                def,
            };
            self.check_feature(self.marker, &entry)
                .map_err(|e| invalid(format!("copy of {}: {e}", f.name)))?;
            // Components and placements, including ones made by these
            // features, were created above; insert_resolved would make
            // them a second time.
            if let (FeatureDef::PatternAtPoints(original), FeatureDef::PatternAtPoints(copy)) =
                (&f.def, &entry.def)
                && let (
                    crate::features::PatternObjects::Components {
                        occurrences: sources,
                    },
                    crate::features::PatternObjects::Components {
                        occurrences: mapped,
                    },
                ) = (&original.objects, &copy.objects)
            {
                for o in &a.occurrences {
                    let Some(instance) = o.pattern.as_ref().filter(|p| p.feature == f.uid) else {
                        continue;
                    };
                    let Some(target) = occurrences.get(&o.uid) else {
                        continue;
                    };
                    let source_index = sources
                        .iter()
                        .position(|p| *p == instance.source)
                        .ok_or_else(|| invalid("pattern source ownership is inconsistent"))?;
                    let target_index = original
                        .targets
                        .iter()
                        .position(|p| *p == instance.target)
                        .ok_or_else(|| invalid("pattern target ownership is inconsistent"))?;
                    self.assembly
                        .occurrence_mut(*target)
                        .expect("copied")
                        .pattern = Some(crate::features::point_pattern::PatternInstance {
                        feature: entry.uid,
                        source: mapped[source_index].clone(),
                        target: copy.targets[target_index].clone(),
                    });
                }
            }
            self.features.insert(self.marker, Arc::new(entry));
            self.marker += 1;
        }
        // Import only appearances used by copied bodies/faces. Existing
        // target definitions stay unchanged even when ids or names collide.
        let appearances = if same_document {
            BTreeMap::new()
        } else {
            self.copy_appearances(source, &uids, &origin)?
        };
        // Materials the copied bodies and components name (mitcad#640),
        // brought from another document; a target's own of the same id
        // and other fields stays, the copy gets another id.
        let materials = if same_document {
            BTreeMap::new()
        } else {
            let needed: BTreeSet<String> = source
                .body_attributes
                .iter()
                .filter(|(body, _)| uids.contains_key(&body.feature))
                .filter_map(|(_, a)| a.material.clone())
                .chain(
                    inside
                        .iter()
                        .filter_map(|c| source.component_materials.get(c).cloned()),
                )
                .collect();
            self.copy_materials(source, &needed)
        };
        for c in &inside {
            if let Some(id) = source.component_materials.get(c) {
                let id = materials.get(id).cloned().unwrap_or_else(|| id.clone());
                self.component_materials.insert(components[c], id);
            }
        }
        // Body attributes follow the bodies, face appearances their faces.
        let text: HashMap<String, String> = uids
            .iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect();
        let attributes: Vec<_> = source
            .body_attributes
            .iter()
            .filter_map(|(body, attributes)| {
                let feature = uids.get(&body.feature)?;
                let mut attributes = attributes.clone();
                if let Some(id) = attributes.appearance.as_mut()
                    && let Some(mapped) = appearances.get(id)
                {
                    *id = mapped.clone();
                }
                if let Some(id) = attributes.material.as_mut()
                    && let Some(mapped) = materials.get(id)
                {
                    *id = mapped.clone();
                }
                attributes.face_appearances = attributes
                    .face_appearances
                    .into_iter()
                    .map(|(face, appearance)| {
                        let mapped = appearances.get(&appearance).cloned().unwrap_or(appearance);
                        (remap_reference(&face, &text), mapped)
                    })
                    .collect();
                attributes.print_bed_face = attributes
                    .print_bed_face
                    .map(|face| remap_reference(&face, &text));
                Some((BodyUid::new(*feature, body.index), attributes))
            })
            .collect();
        self.body_attributes.extend(attributes);
        // Flat Pattern items follow their sheet bodies, split pieces
        // (`F<cut>.b<k>`) included, with the fixed panel's feature mapped
        // (mitcad#125).
        let flat_patterns: Vec<_> = source
            .flat_patterns
            .iter()
            .filter_map(|(body, def)| {
                let feature = uids.get(&body.feature)?;
                let mut def = def.clone();
                if let Some(panel) = uids.get(&def.stationary.feature) {
                    def.stationary.feature = *panel;
                }
                Some((BodyUid::new(*feature, body.index), def))
            })
            .collect();
        self.flat_patterns.extend(flat_patterns);
        Ok((
            into,
            ComponentCopy {
                occurrences,
                features: uids,
            },
        ))
    }

    fn copy_appearances(
        &mut self,
        source: &DocState,
        uids: &HashMap<FeatureUid, FeatureUid>,
        origin: &CopySource<'_>,
    ) -> Result<BTreeMap<String, String>, ModelError> {
        let needed: BTreeSet<&str> = source
            .body_attributes
            .iter()
            .filter(|(body, _)| uids.contains_key(&body.feature))
            .flat_map(|(_, attributes)| {
                attributes
                    .appearance
                    .iter()
                    .chain(attributes.face_appearances.values())
                    .map(String::as_str)
            })
            .collect();
        let mut ids: BTreeSet<String> = appearance::library()
            .iter()
            .chain(&self.appearances)
            .map(|a| a.id.clone())
            .collect();
        // Undefined target ids must remain undefined too: importing a
        // definition with that id must not recolour an existing body.
        ids.extend(self.body_attributes.values().flat_map(|attributes| {
            attributes
                .appearance
                .iter()
                .chain(attributes.face_appearances.values())
                .cloned()
        }));
        let mut names: BTreeSet<String> = appearance::library()
            .iter()
            .chain(&self.appearances)
            .map(|a| a.name.clone())
            .collect();
        let mut mapped = BTreeMap::new();
        // Images the target embeds already are shared, not copied (mitcad#313).
        let mut images = appearance::embedded_images(&self.appearances);
        for id in needed {
            if appearance::library_appearance(id).is_some() {
                continue;
            }
            let new_id = std::iter::once(id.to_owned())
                .chain((1..).map(|n| format!("{id}_{n}")))
                .find(|id| ids.insert(id.clone()))
                .expect("an unused appearance id exists");
            // Older/imported designs may keep an unknown appearance id.
            // Keep it unresolved instead of binding to a target definition.
            if let Some(appearance) = source.appearances.iter().find(|a| a.id == id) {
                let mut copy = appearance.clone();
                copy.id = new_id.clone();
                copy.name = std::iter::once(copy.name.clone())
                    .chain((1..).map(|n| format!("{} ({n})", appearance.name)))
                    .find(|name| names.insert(name.clone()))
                    .expect("an unused appearance name exists");
                if let Some(texture) = copy.texture.as_mut() {
                    origin.texture(texture)?;
                    appearance::share_images(texture, &mut images);
                }
                self.appearances.push(copy);
            }
            mapped.insert(id.to_owned(), new_id);
        }
        Ok(mapped)
    }

    /// The parameters the copied features use: their own ones copied with
    /// their expressions (the names they refer to follow), others shared
    /// in the same document or brought from another by name.
    fn copy_parameters(
        &mut self,
        source: &DocState,
        features: &[&FeatureEntry],
        uids: &HashMap<FeatureUid, FeatureUid>,
        same_document: bool,
    ) -> Result<HashMap<ParamId, ParamId>, ModelError> {
        let table = source.parameters.table();
        let mut independent: BTreeSet<ParamId> = features
            .iter()
            .filter(|f| matches!(f.def, FeatureDef::SheetRules(_)))
            .flat_map(|f| f.def.params())
            .collect();
        let mut rule_params: Vec<_> = independent.iter().copied().collect();
        while let Some(id) = rule_params.pop() {
            if let Some(p) = table.get(id) {
                for dependency in p.dependencies() {
                    if independent.insert(*dependency) {
                        rule_params.push(*dependency);
                    }
                }
            }
        }
        let mut needed: BTreeSet<ParamId> = features.iter().flat_map(|f| f.def.params()).collect();
        let mut todo: Vec<ParamId> = needed.iter().copied().collect();
        while let Some(id) = todo.pop() {
            if let Some(p) = table.get(id) {
                for d in p.dependencies() {
                    if needed.insert(*d) {
                        todo.push(*d);
                    }
                }
            }
        }
        // The tables the copied expressions look values up in come along
        // from another design (mitcad#667); in the same one they are shared.
        let tables = if same_document {
            HashMap::new()
        } else {
            let used: BTreeSet<&str> = needed
                .iter()
                .filter_map(|id| table.get(*id))
                .flat_map(|p| p.table_references())
                .map(|(t, _)| t)
                .collect();
            self.bring_tables(source, used)?
        };
        let mut map: HashMap<ParamId, ParamId> = HashMap::new();
        let mut names: HashMap<String, String> = HashMap::new();
        for id in table.evaluation_order() {
            if !needed.contains(&id) {
                continue;
            }
            let p = table.get(id).expect("in the table");
            let owner = source
                .parameters
                .owner(id)
                .and_then(|o| uids.get(&o))
                .copied();
            if owner.is_none() && same_document && !independent.contains(&id) {
                map.insert(id, id);
                continue;
            }
            let name = if owner.is_some() && is_dimension_name(p.name()) {
                self.parameters.table().unused_name("d")
            } else {
                free_parameter_name(&self.parameters, p.name())
            };
            let expression = super::tables::rename_tables_in(
                &rename_references(p.expression(), p.expr(), &names),
                &tables,
            );
            // Dimensionless parameters can still use implicit lengths or
            // angles, and rounding uses default units too. Keep the source
            // context instead of reinterpreting the same expression here.
            let context = p
                .expression_context()
                .or_else(|| (!same_document).then_some(source.parameters.context()));
            // Its limits come along, their references renamed as the
            // expression's (mitcad#665).
            let mut limits = p.limits().clone();
            for text in [&mut limits.min, &mut limits.max, &mut limits.step]
                .into_iter()
                .flatten()
            {
                if let Ok(expr) = crate::expr::Expr::parse(text) {
                    *text = super::tables::rename_tables_in(
                        &rename_references(text, &expr, &names),
                        &tables,
                    );
                }
            }
            let mut spec = ParamSpec::user(&name, &expression, p.unit())
                .with_comment(p.comment())
                .with_expression_context(context)
                .with_limits(limits);
            // A copied sketch's driven dimensions keep read-only references
            // (mitcad#231).
            if owner.is_some() && p.kind() == crate::expr::ParamKind::Reference {
                spec.kind = crate::expr::ParamKind::Reference;
            }
            let new = self
                .parameters
                .add(spec, owner)
                .map_err(|e| invalid(format!("parameter {}: {e}", p.name())))?;
            names.insert(p.name().to_owned(), name);
            map.insert(id, new);
        }
        Ok(map)
    }
}

fn set_flags(target: &mut Occurrence, source: &Occurrence) {
    target.transform = source.transform;
    target.grounded = source.grounded;
    target.visible = source.visible;
}

/// `d12` and the like: dimension names, which copies number afresh.
fn is_dimension_name(name: &str) -> bool {
    name.strip_prefix('d')
        .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

/// `name`, or `name_1`, `name_2`, ... the first one not in use.
fn free_parameter_name(parameters: &crate::parameters::Parameters, name: &str) -> String {
    std::iter::once(name.to_owned())
        .chain((1..).map(|k| format!("{name}_{k}")))
        .find(|n| {
            parameters.find(n).is_none()
                && !crate::expr::is_reserved_name(n)
                && !parameters.tables().is_some_and(|t| t.has_table(n))
        })
        .expect("a free name exists")
}

/// Reference mapping for geometry picked in assembly context. The same
/// component may also be placed outside the copied subtree: a link picked
/// there keeps both that occurrence and its original geometry ids.
struct CopyReferences<'a> {
    source: &'a Assembly,
    destination: &'a Assembly,
    source_path: &'a [OccurrenceUid],
    prefix: &'a [OccurrenceUid],
    occurrences: &'a HashMap<OccurrenceUid, OccurrenceUid>,
    uids: &'a HashMap<FeatureUid, FeatureUid>,
    same_document: bool,
}

impl CopyReferences<'_> {
    fn body(&self, body: BodyUid) -> BodyUid {
        BodyUid::new(
            self.uids
                .get(&body.feature)
                .copied()
                .unwrap_or(body.feature),
            body.index,
        )
    }

    /// A root-relative path through the selected source occurrence, with
    /// its prefix replaced by the new copy's and the occurrences below
    /// it mapped. Paths outside that subtree remain external, even if
    /// they end in a copied component.
    fn path(&self, path: &OccurrencePath) -> Option<OccurrencePath> {
        let mut component = ComponentUid::ROOT;
        if !path.0.starts_with(self.source_path) {
            return None;
        }
        for uid in &path.0 {
            let occurrence = self.source.occurrence(*uid)?;
            if occurrence.parent != component {
                return None;
            }
            component = occurrence.component;
        }
        let suffix: Option<Vec<_>> = path.0[self.source_path.len()..]
            .iter()
            .map(|o| self.occurrences.get(o).copied())
            .collect();
        Some(OccurrencePath(
            self.prefix.iter().copied().chain(suffix?).collect(),
        ))
    }

    fn link(
        &self,
        link: &OccurrenceLink,
        component: ComponentUid,
    ) -> Result<(OccurrenceLink, bool), ModelError> {
        let source = self.path(&link.source);
        let internal = source.is_some();
        let target = self.path(&link.target);
        let target = crate::links::first_path(
            self.destination,
            component,
            target.as_ref().map(|p| p.0.as_slice()),
        )
        .map_err(invalid)?;
        Ok((
            OccurrenceLink {
                source: source.unwrap_or_else(|| link.source.clone()),
                target: OccurrencePath(target),
            },
            internal,
        ))
    }

    /// A seat's link to another component's shaft (mitcad#578): the copy
    /// keeps the original shaft unless it is copied too.
    fn seat(
        &self,
        copy: &mut Option<crate::features::Seat>,
        original: &Option<crate::features::Seat>,
        component: ComponentUid,
    ) -> Result<(), ModelError> {
        if let (Some(copy), Some(original)) = (copy.as_mut(), original.as_ref())
            && let Some(link) = &original.link
        {
            let (link, internal) = self.link(link, component)?;
            copy.link = Some(link);
            if !internal {
                copy.shaft = original.shaft.clone();
            }
        }
        Ok(())
    }

    fn links(
        &self,
        copy: &mut FeatureDef,
        original: &FeatureDef,
        component: ComponentUid,
    ) -> Result<(), ModelError> {
        // A move's points and axes picked in occurrences (mitcad#699): root
        // paths into the copied subtree follow it, others stay as they were,
        // as Pattern at Points' points do.
        if let (Some(to), Some(from)) = (copy.move_spec_mut(), original.move_spec()) {
            for ((to_ref, to_path), (from_ref, from_path)) in to
                .placed_references_mut()
                .into_iter()
                .zip(from.placed_references())
            {
                if let Some(path) = from_path {
                    match self.path(path) {
                        Some(mapped) => *to_path = Some(mapped),
                        None => {
                            *to_ref = from_ref.clone();
                            *to_path = Some(path.clone());
                        }
                    }
                }
            }
            return Ok(());
        }
        match (copy, original) {
            (FeatureDef::PatternAtPoints(copy), FeatureDef::PatternAtPoints(original)) => {
                for (to, from) in copy
                    .targets
                    .iter_mut()
                    .chain(copy.anchor.iter_mut())
                    .zip(original.targets.iter().chain(original.anchor.iter()))
                {
                    if let Some(path) = &from.occurrence {
                        if let Some(mapped) = self.path(path) {
                            to.occurrence = Some(mapped);
                        } else {
                            *to = from.clone();
                        }
                    }
                }
                copy.context = Some(OccurrencePath(
                    crate::links::first_path(self.destination, component, None).map_err(invalid)?,
                ));
                if let (
                    crate::features::PatternObjects::Components { occurrences: to },
                    crate::features::PatternObjects::Components { occurrences: from },
                ) = (&mut copy.objects, &original.objects)
                {
                    *to = from
                        .iter()
                        .map(|p| self.path(p).unwrap_or_else(|| p.clone()))
                        .collect();
                }
            }
            (FeatureDef::Extrude(copy), FeatureDef::Extrude(original)) => {
                if let Some(original) = &original.cut_scope {
                    use crate::features::extrude_scope::{
                        BodyTarget, ComponentTarget, TargetScope,
                    };
                    let target = |t: &ComponentTarget| -> Result<ComponentTarget, ModelError> {
                        match self.path(&t.occurrence) {
                            Some(path) => Ok(ComponentTarget {
                                component: crate::joints::path_component(
                                    self.destination,
                                    ComponentUid::ROOT,
                                    &path.0,
                                )
                                .map_err(invalid)?,
                                occurrence: path,
                            }),
                            None if self.same_document => Ok(t.clone()),
                            None => Err(invalid(
                                "cannot copy an assembly cut target outside the inserted component",
                            )),
                        }
                    };
                    let mut scope = original.clone();
                    let source_component = crate::joints::path_component(
                        self.source,
                        ComponentUid::ROOT,
                        &original.context.occurrence.0,
                    )
                    .map_err(invalid)?;
                    if source_component != original.context.component {
                        return Err(invalid(
                            "cannot copy an assembly cut whose source context no longer resolves",
                        ));
                    }
                    let preferred = self.path(&original.context.occurrence);
                    scope.context = ComponentTarget {
                        component,
                        occurrence: OccurrencePath(
                            crate::links::first_path(
                                self.destination,
                                component,
                                preferred.as_ref().map(|p| p.0.as_slice()),
                            )
                            .map_err(invalid)?,
                        ),
                    };
                    let bodies = original
                        .excluded_bodies()
                        .iter()
                        .map(|b| {
                            let mapped = target(&ComponentTarget {
                                component: b.component,
                                occurrence: b.occurrence.clone(),
                            })?;
                            let body = if self.path(&b.occurrence).is_some() {
                                self.body(b.body)
                            } else {
                                b.body
                            };
                            Ok(BodyTarget {
                                component: mapped.component,
                                occurrence: mapped.occurrence,
                                body,
                            })
                        })
                        .collect::<Result<Vec<_>, ModelError>>()?;
                    let targets = original
                        .stored_targets()
                        .iter()
                        .map(target)
                        .collect::<Result<Vec<_>, _>>()?;
                    scope.targets = match &original.targets {
                        TargetScope::Automatic { .. } => TargetScope::Automatic {
                            excluded: targets,
                            excluded_bodies: bodies,
                        },
                        TargetScope::Explicit { .. } => TargetScope::Explicit {
                            included: targets,
                            excluded_bodies: bodies,
                        },
                    };
                    copy.cut_scope = Some(scope);
                }
                // The objects on other components' bodies (mitcad#104):
                // as a sweep's.
                copy.object_links.clear();
                for (body, link) in &original.object_links {
                    let (link, internal) = self.link(link, component)?;
                    let mapped = if internal { self.body(*body) } else { *body };
                    copy.object_links.insert(mapped, link);
                }
            }
            // A revolution's object on another component's body (mitcad#762):
            // as an extrusion's.
            (FeatureDef::Revolve(copy), FeatureDef::Revolve(original)) => {
                copy.object_links.clear();
                for (body, link) in &original.object_links {
                    let (link, internal) = self.link(link, component)?;
                    let mapped = if internal { self.body(*body) } else { *body };
                    copy.object_links.insert(mapped, link);
                }
            }
            (FeatureDef::Combine(copy), FeatureDef::Combine(original)) => {
                let mut tools = HashMap::new();
                copy.tool_links.clear();
                for (tool, link) in &original.tool_links {
                    let (link, internal) = self.link(link, component)?;
                    let mapped = if internal { self.body(*tool) } else { *tool };
                    tools.insert(*tool, mapped);
                    copy.tool_links.insert(mapped, link);
                }
                // Tools in occurrences (mitcad#544): each by its own link.
                copy.tools = original
                    .tools
                    .iter()
                    .map(|tool| match &tool.link {
                        Some(link) => {
                            let (link, internal) = self.link(link, component)?;
                            let body = if internal {
                                self.body(tool.body)
                            } else {
                                tool.body
                            };
                            Ok(CombineTool::linked(body, link))
                        }
                        None => Ok(CombineTool::own(
                            tools
                                .get(&tool.body)
                                .copied()
                                .unwrap_or_else(|| self.body(tool.body)),
                        )),
                    })
                    .collect::<Result<Vec<_>, ModelError>>()?;
            }
            // A sweep's path and guide surfaces on other components' bodies
            // (mitcad#161): the bodies' references are copied as other
            // references are (renamed when the copy has them).
            (FeatureDef::Sweep(copy), FeatureDef::Sweep(original)) => {
                copy.body_links.clear();
                for (body, link) in &original.body_links {
                    let (link, internal) = self.link(link, component)?;
                    let mapped = if internal { self.body(*body) } else { *body };
                    copy.body_links.insert(mapped, link);
                }
            }
            // A construction plane's faces on other components' bodies
            // (mitcad#196): as a sweep's.
            (FeatureDef::ConstructionPlane(copy), FeatureDef::ConstructionPlane(original)) => {
                copy.links.clear();
                for (body, link) in &original.links {
                    let (link, internal) = self.link(link, component)?;
                    let mapped = if internal { self.body(*body) } else { *body };
                    copy.links.insert(mapped, link);
                }
            }
            // A derive's bodies of other components (mitcad#324): as a
            // combine's tools.
            (FeatureDef::Derive(copy), FeatureDef::Derive(original)) => {
                let mut bodies = HashMap::new();
                copy.links.clear();
                for (body, link) in &original.links {
                    let (link, internal) = self.link(link, component)?;
                    let mapped = if internal { self.body(*body) } else { *body };
                    bodies.insert(*body, mapped);
                    copy.links.insert(mapped, link);
                }
                copy.bodies = original
                    .bodies
                    .iter()
                    .map(|b| bodies.get(b).copied().unwrap_or_else(|| self.body(*b)))
                    .collect();
            }
            // A body transfer between components of the copy (mitcad#465):
            // its bodies are the copies'.
            (FeatureDef::BodyTransfer(copy), FeatureDef::BodyTransfer(original)) => {
                let (link, internal) = self.link(&original.link, component)?;
                copy.link = link;
                if internal {
                    copy.bodies = original.bodies.iter().map(|b| self.body(*b)).collect();
                }
            }
            // A seat on another component's shaft (mitcad#578): as a
            // sketch's plane.
            (FeatureDef::Pulley(copy), FeatureDef::Pulley(original)) => {
                self.seat(&mut copy.seat, &original.seat, component)?;
            }
            (FeatureDef::Gear(copy), FeatureDef::Gear(original)) => {
                self.seat(&mut copy.seat, &original.seat, component)?;
            }
            (FeatureDef::PrintableBearing(copy), FeatureDef::PrintableBearing(original)) => {
                self.seat(&mut copy.seat, &original.seat, component)?;
            }
            // A split's tool bodies of other components: as a sweep's.
            (FeatureDef::SplitBody(copy), FeatureDef::SplitBody(original)) => {
                copy.tool_links.clear();
                for (body, link) in &original.tool_links {
                    let (link, internal) = self.link(link, component)?;
                    let mapped = if internal { self.body(*body) } else { *body };
                    copy.tool_links.insert(mapped, link);
                }
            }
            (FeatureDef::Sketch(copy), FeatureDef::Sketch(original)) => {
                if let Some(link) = &original.plane_link {
                    let (link, internal) = self.link(link, component)?;
                    copy.plane_link = Some(link);
                    if !internal {
                        copy.plane = original.plane.clone();
                    }
                }
                let text = self
                    .uids
                    .iter()
                    .map(|(a, b)| (a.to_string(), b.to_string()))
                    .collect();
                for (copy, original) in copy.projections.iter_mut().zip(&original.projections) {
                    let internal = if let Some(link) = &original.link {
                        let (link, internal) = self.link(link, component)?;
                        copy.link = Some(link);
                        internal
                    } else {
                        true
                    };
                    copy.source = if internal {
                        // Another sketch's entity (mitcad#904): its sketch's
                        // copy when that is copied too.
                        let remapped = match &original.source {
                            crate::sketch::ProjectionSource::Sketch(_) => {
                                remap_text(&original.source.to_string(), &text)
                            }
                            crate::sketch::ProjectionSource::Topo(_) => {
                                remap_reference(&original.source.to_string(), &text)
                            }
                        };
                        remapped
                            .parse()
                            .map_err(|e| invalid(format!("copy projection: {e}")))?
                    } else {
                        original.source.clone()
                    };
                    copy.body = original
                        .body
                        .map(|b| if internal { self.body(b) } else { b });
                    // The earlier feature's state it is read in (mitcad#998):
                    // that feature's copy when it is copied too.
                    copy.after = original.after.map(|f| {
                        if internal {
                            self.uids.get(&f).copied().unwrap_or(f)
                        } else {
                            f
                        }
                    });
                }
            }
            _ => {}
        }
        Ok(())
    }
}

/// An expression with the references renamed by `names` all at once.
fn rename_references(
    text: &str,
    expr: &crate::expr::Expr,
    names: &HashMap<String, String>,
) -> String {
    let mut out = String::with_capacity(text.len());
    let mut copied = 0;
    for (name, span) in expr.reference_spans() {
        if let Some(new) = names.get(name) {
            out.push_str(&text[copied..span.start]);
            out.push_str(new);
            copied = span.end;
        }
    }
    out.push_str(&text[copied..]);
    out
}

/// A definition with its parameters and feature references mapped. The
/// references are rewritten in the definition's JSON form, accepting only
/// feature ids, body ids and parsed topological names. Body-keyed links
/// and projection sources are handled in their typed context above.
fn copy_def(
    def: &FeatureDef,
    params: &HashMap<ParamId, ParamId>,
    uids: &HashMap<FeatureUid, FeatureUid>,
) -> Result<FeatureDef, ModelError> {
    let mut slots = Vec::new();
    let marked = def
        .map_params(&mut |_, id: &ParamId| {
            slots.push(*params.get(id).unwrap_or(id));
            Ok::<_, ()>(format!("{PARAM}{}", slots.len() - 1))
        })
        .expect("never fails");
    let mut value = serde_json::to_value(&marked).map_err(|e| invalid(e.to_string()))?;
    let text: HashMap<String, String> = uids
        .iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect();
    remap_value(&mut value, &text);
    let copied: FeatureDef<String> =
        serde_json::from_value(value).map_err(|e| invalid(format!("copy: {e}")))?;
    let mut copied = copied.map_params(&mut |slot, marker: &String| {
        marker
            .strip_prefix(PARAM)
            .and_then(|i| i.parse::<usize>().ok())
            .and_then(|i| slots.get(i).copied())
            .ok_or_else(|| invalid(format!("copy: {slot} lost its parameter")))
    })?;
    copy_measurement_sides(&mut copied, def, &text);
    Ok(copied)
}

/// Renumbering may reverse the canonical faces of an edge. Distances
/// measured on its first face must still refer to that same copied face.
fn copy_measurement_sides(
    copy: &mut FeatureDef,
    original: &FeatureDef,
    uids: &HashMap<String, String>,
) {
    let edges = |old: &[EdgeName], new: &[EdgeName]| -> (Vec<EdgeName>, Vec<EdgeName>) {
        let (mut kept, mut swapped) = (Vec::new(), Vec::new());
        for (old, new) in old.iter().zip(new) {
            if remap_reference(&old.faces()[0].to_string(), uids) == new.faces()[0].to_string() {
                kept.push(new.clone());
            } else {
                swapped.push(new.clone());
            }
        }
        (kept, swapped)
    };
    match (copy, original) {
        (FeatureDef::Fillet(copy), FeatureDef::Fillet(original)) => {
            for (mut set, old) in std::mem::take(&mut copy.sets)
                .into_iter()
                .zip(&original.sets)
            {
                let mut split = false;
                if set.reference_face.is_none()
                    && matches!(set.size, FilletSizeDef::Asymmetric { .. })
                {
                    let (kept, swapped) = edges(&old.edges, &set.edges);
                    if !swapped.is_empty() {
                        split = true;
                        let mut other = set.clone();
                        other.edges = swapped;
                        other.faces.clear();
                        if let FilletSizeDef::Asymmetric { flip, .. } = &mut other.size {
                            *flip = !*flip;
                        }
                        copy.sets.push(other);
                        set.edges = kept;
                    }
                }
                if !split || !set.edges.is_empty() || !set.faces.is_empty() {
                    copy.sets.push(set);
                }
            }
        }
        (FeatureDef::Chamfer(copy), FeatureDef::Chamfer(original)) => {
            for (mut set, old) in std::mem::take(&mut copy.sets)
                .into_iter()
                .zip(&original.sets)
            {
                let mut split = false;
                if set.reference_face.is_none()
                    && !matches!(set.size, ChamferSizeDef::EqualDistance { .. })
                {
                    let (kept, swapped) = edges(&old.edges, &set.edges);
                    if !swapped.is_empty() {
                        split = true;
                        let mut other = set.clone();
                        other.edges = swapped;
                        other.faces.clear();
                        other.flip = !other.flip;
                        copy.sets.push(other);
                        set.edges = kept;
                    }
                }
                if !split || !set.edges.is_empty() || !set.faces.is_empty() {
                    copy.sets.push(set);
                }
            }
        }
        _ => {}
    }
}

/// Fields of definitions that are texts, not references.
const TEXT_FIELDS: [&str; 5] = ["name", "source", "text", "font", "brep"];

fn remap_value(value: &mut Value, uids: &HashMap<String, String>) {
    match value {
        Value::String(s) if !s.starts_with(PARAM) => *s = remap_reference(s, uids),
        Value::Array(items) => {
            for item in items {
                remap_value(item, uids);
            }
        }
        Value::Object(fields) => {
            for (key, item) in fields.iter_mut() {
                if !TEXT_FIELDS.contains(&key.as_str()) {
                    remap_value(item, uids);
                }
            }
        }
        _ => {}
    }
}

pub(super) fn remap_reference(text: &str, uids: &HashMap<String, String>) -> String {
    if text.parse::<FeatureUid>().is_ok() || text.parse::<BodyUid>().is_ok() {
        return remap_text(text, uids);
    }
    // A derived part's mount frame, `F7:mount(slot_x+_1)` (mitcad#375).
    if crate::features::geom_ref::parse_mount(text).is_ok() {
        return remap_text(text, uids);
    }
    if text.parse::<TopoName>().is_ok() {
        // Parsing again canonicalises nested edge/vertex face order after
        // renumbering (F9 sorts differently from F10).
        return remap_text(text, uids)
            .parse::<TopoName>()
            .expect("mapping feature ids preserves a topological name")
            .to_string();
    }
    text.to_owned()
}

/// Replaces the feature ids `F<n>` in a reference text: an `F` that does
/// not follow a letter, digit or `_`, then digits that no letter, digit or
/// `_` follows.
fn remap_text(text: &str, uids: &HashMap<String, String>) -> String {
    let bytes = text.as_bytes();
    let word = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    let mut copied = 0;
    while i < bytes.len() {
        // A derived part's face names its source's face in the source's
        // own ids (mitcad#375): `:part(...)` is copied as it is.
        if bytes[i..].starts_with(b":part(") {
            let mut depth = 0usize;
            let mut j = i + ":part".len();
            while j < bytes.len() {
                match bytes[j] {
                    b'(' => depth += 1,
                    b')' => {
                        depth -= 1;
                        if depth == 0 {
                            j += 1;
                            break;
                        }
                    }
                    _ => {}
                }
                j += 1;
            }
            i = j;
            continue;
        }
        if bytes[i] == b'F' && (i == 0 || !word(bytes[i - 1])) {
            let mut j = i + 1;
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                j += 1;
            }
            if j > i + 1
                && (j == bytes.len() || !word(bytes[j]))
                && let Some(new) = uids.get(&text[i..j])
            {
                out.push_str(&text[copied..i]);
                out.push_str(new);
                copied = j;
            }
            i = j.max(i + 1);
        } else {
            i += 1;
        }
    }
    out.push_str(&text[copied..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feature_ids_in_references_are_renamed() {
        let uids: HashMap<String, String> = [("F2", "F9"), ("F3", "F10")]
            .into_iter()
            .map(|(a, b)| (a.to_owned(), b.to_owned()))
            .collect();
        assert_eq!(remap_text("F2", &uids), "F9");
        assert_eq!(remap_text("F2.b0", &uids), "F9.b0");
        assert_eq!(
            remap_text("E{F2:side(c1[c4,c2])|F3:inst1(F2:end(r{c5})#1)}", &uids),
            "E{F9:side(c1[c4,c2])|F10:inst1(F9:end(r{c5})#1)}"
        );
        assert_eq!(remap_text("F23", &uids), "F23");
        assert_eq!(remap_text("XF2", &uids), "XF2");
        assert_eq!(remap_text("F2a", &uids), "F2a");
        assert_eq!(remap_text("r{c1[c4,c2]}", &uids), "r{c1[c4,c2]}");
    }

    #[test]
    fn only_typed_references_are_mapped_and_nested_names_are_canonical() {
        let uids = HashMap::from([
            ("F2".to_owned(), "F9".to_owned()),
            ("F3".to_owned(), "F10".to_owned()),
        ]);
        assert_eq!(remap_reference("F2.b0", &uids), "F9.b0");
        assert_eq!(
            remap_reference("F3:fillet(E{F2:side(c1)|F3:end(r{c1})})", &uids),
            "F10:fillet(E{F10:end(r{c1})|F9:side(c1)})"
        );
        assert_eq!(remap_reference("caption F2", &uids), "caption F2");
        let mut value = serde_json::json!({
            "name": "F2", "source": "F2", "text": "F2", "font": "F2", "brep": "F2",
            "body": "F2.b0", "reference_face": "F2:end(r{c1})",
        });
        remap_value(&mut value, &uids);
        for field in TEXT_FIELDS {
            assert_eq!(value[field], "F2");
        }
        assert_eq!(value["body"], "F9.b0");
        assert_eq!(value["reference_face"], "F9:end(r{c1})");
    }

    #[test]
    fn whole_design_links_to_root_geometry_receive_the_copy_prefix() {
        let source = Assembly::default();
        let mut destination = Assembly::default();
        let copy = destination.create(Some("Copy"), None);
        let occurrence = destination.place(copy, ComponentUid::ROOT, Transform::IDENTITY);
        let prefix = [occurrence];
        let references = CopyReferences {
            source: &source,
            destination: &destination,
            source_path: &[],
            prefix: &prefix,
            occurrences: &HashMap::new(),
            uids: &HashMap::new(),
            same_document: false,
        };
        let (link, internal) = references.link(&OccurrenceLink::default(), copy).unwrap();
        assert!(internal);
        assert_eq!(link.source.0, prefix);
        assert_eq!(link.target.0, prefix);
    }

    #[test]
    fn copied_chamfers_keep_distances_on_the_original_faces() {
        use crate::features::{ChamferCorner, ChamferDef, ChamferSetDef};
        let old = FeatureDef::Chamfer(ChamferDef {
            body: "F2.b0".parse().unwrap(),
            sets: vec![ChamferSetDef {
                edges: vec![
                    "E{F2:a|F3:b}".parse().unwrap(),
                    "E{F2:a|F2:b}".parse().unwrap(),
                ],
                faces: Vec::new(),
                all_edges: false,
                size: ChamferSizeDef::TwoDistances {
                    distance1: ParamId::from_raw(1),
                    distance2: ParamId::from_raw(2),
                },
                reference_face: None,
                flip: false,
                tangent_chain: true,
            }],
            corner: ChamferCorner::Chamfer,
        });
        let uids = HashMap::from([
            (FeatureUid(2), FeatureUid(9)),
            (FeatureUid(3), FeatureUid(10)),
        ]);
        let FeatureDef::Chamfer(copy) = copy_def(&old, &HashMap::new(), &uids).unwrap() else {
            panic!("copied a chamfer");
        };
        assert_eq!(copy.sets.len(), 2);
        assert!(copy.sets[0].flip);
        assert_eq!(copy.sets[0].edges[0].to_string(), "E{F10:b|F9:a}");
        assert!(!copy.sets[1].flip);
        assert_eq!(copy.sets[1].edges[0].to_string(), "E{F9:a|F9:b}");
        assert_eq!(copy.sets[0].size, copy.sets[1].size);
    }

    #[test]
    fn copied_fillets_map_reference_faces_vertices_and_asymmetric_sides() {
        let old: FeatureDef<String> = serde_json::from_value(serde_json::json!({
            "type": "fillet", "body": "F2.b0", "sets": [
                {"edges": ["E{F2:a|F3:b}"],
                 "size": {"type": "asymmetric", "distance1": "d1", "distance2": "d2"}},
                {"edges": ["E{F2:a|F3:b}"], "faces": ["F2:a"],
                 "reference_face": "F3:b",
                 "size": {"type": "variable", "start": "d1", "end": "d2",
                          "start_vertex": "V{F2:a|F2:b|F3:c}"}}
            ]
        }))
        .unwrap();
        let old = old
            .map_params(&mut |_, _| Ok::<_, ()>(ParamId::from_raw(1)))
            .unwrap();
        let uids = HashMap::from([
            (FeatureUid(2), FeatureUid(9)),
            (FeatureUid(3), FeatureUid(10)),
        ]);
        let FeatureDef::Fillet(copy) = copy_def(&old, &HashMap::new(), &uids).unwrap() else {
            panic!("copied a fillet");
        };
        assert_eq!(copy.sets.len(), 2);
        assert!(matches!(
            copy.sets[0].size,
            FilletSizeDef::Asymmetric { flip: true, .. }
        ));
        assert_eq!(copy.sets[0].edges[0].to_string(), "E{F10:b|F9:a}");
        assert_eq!(
            copy.sets[1].reference_face.as_ref().unwrap().to_string(),
            "F10:b"
        );
        assert_eq!(copy.sets[1].faces[0].to_string(), "F9:a");
        let FilletSizeDef::Variable { start_vertex, .. } = &copy.sets[1].size else {
            panic!("kept a variable fillet");
        };
        assert_eq!(
            start_vertex.as_ref().unwrap().to_string(),
            "V{F10:c|F9:a|F9:b}"
        );
    }

    #[test]
    fn expressions_rename_all_references_at_once() {
        let mut table = crate::expr::ParameterTable::new(crate::parameters::default_context());
        let unit = crate::expr::Unit::MM;
        table.add(ParamSpec::user("d1", "10 mm", unit)).unwrap();
        table.add(ParamSpec::user("d2", "20 mm", unit)).unwrap();
        let id = table
            .add(ParamSpec::user("d3", "d1 + d2 * 2", unit))
            .unwrap();
        let p = table.get(id).unwrap();
        let names: HashMap<String, String> = [("d1", "d2"), ("d2", "d7")]
            .into_iter()
            .map(|(a, b)| (a.to_owned(), b.to_owned()))
            .collect();
        assert_eq!(
            rename_references(p.expression(), p.expr(), &names),
            "d2 + d7 * 2"
        );
    }
}
