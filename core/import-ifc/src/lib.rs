// SPDX-License-Identifier: MIT
//! The IFC import (mitcad#273): an IFC file's building model
//! ([`mitcad_ifc`]) as a Mitcad design, generic over the [`Kernel`].
//!
//! - The spatial structure: each site, building, storey and facility (and
//!   its parts) is a component in its parent's, placed where the file
//!   places it; a storey's elevation is a construction plane of its
//!   component, a grid's axes are construction axes. Spaces are not
//!   imported.
//! - Each element's shape (its `Body` representation) comes into the
//!   component of the spatial element it is in, in one command of its own
//!   ([`mitcad_model::FeatureBatch`]): swept solids, CSG primitives and
//!   booleans as features ([`build`]: sketches, extrusions, revolutions,
//!   sweeps, pipes, lofts, cuts and combines), breps as base features of
//!   the bodies the kernel builds from their faces ([`brep`]), face sets
//!   as mesh bodies. Its openings (IfcRelVoidsElement) are cut from its
//!   bodies with their own shapes. Its bodies are named by its name and
//!   GlobalId and take the colours of its styles as appearances.
//! - A type's representation map that an element uses with a rigid
//!   transform (and no openings) is one component, built once, and each
//!   such use an occurrence of it; other uses copy the map's items into
//!   the element, moved (scaled, mirrored) by the transform
//!   ([`transform`]).
//! - The report ([`report`]) has a line for every product of the file,
//!   and checks the bodies' volumes against the file's element quantities.
//!   What could not be built stays on the timeline as import notes
//!   (mitcad#170): a note on the element's first feature, or a suppressed
//!   placeholder when nothing of it was built.
//! - Low on memory (the import process's guard, [`Progress`]), the
//!   import stops before the next element; the rest are reported as not
//!   imported.

pub mod brep;
pub mod build;
// Elements' properties (mitcad#496).
pub mod properties;
pub mod report;
pub mod sketch;
pub mod transform;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::Ordering;

use mitcad_ifc::IfcFile;
use mitcad_ifc::geometry::{Decoder, Item, TOO_MANY_ITEMS};
use mitcad_ifc::math::{self, Matrix};
use mitcad_ifc::model::{Model, Product, Style};
use mitcad_ifc::schema::ProductKind;
use mitcad_import_base::Progress;
use mitcad_model::properties::{Property, PropertyOwner};
use mitcad_model::{BodyKind, BodyUid, ComponentUid, Document, FeatureUid, Kernel, OccurrenceUid};
use serde_json::json;

use build::{Builder, Node, Op};
pub use report::{IfcReport, MapLine, Outcome, ProductLine, QUANTITY_TOLERANCE, QuantityCheck};

/// Settings of the IFC import.
#[derive(Debug, Clone)]
pub struct IfcOptions {
    /// Cut elements' openings from them.
    pub openings: bool,
    /// Uses of a type's representation map as occurrences of one
    /// component (else copies of its items).
    pub occurrences: bool,
    /// The import process's memory guard (mitcad#80).
    pub progress: Option<Arc<Progress>>,
    /// The component the spatial structure comes in under (the root; a
    /// project's IFC link's component, mitcad#333).
    pub parent: ComponentUid,
}

impl Default for IfcOptions {
    fn default() -> Self {
        Self {
            openings: true,
            occurrences: true,
            progress: None,
            parent: ComponentUid::ROOT,
        }
    }
}

/// What the items of an element or a map made.
#[derive(Debug, Default)]
struct Made {
    features: Vec<FeatureUid>,
    bodies: Vec<BodyUid>,
    parametric: Vec<&'static str>,
    stored: Vec<&'static str>,
    meshes: usize,
    /// Closed meshes' volumes (the kernel does not measure meshes).
    mesh_volume: f64,
    /// Meshes' areas.
    mesh_area: f64,
    volume: Option<f64>,
    /// The bodies' area (solids, sheets and meshes).
    area: Option<f64>,
    /// Items that did not build, and why.
    failed: Vec<String>,
    notes: Vec<String>,
    /// Openings cut.
    cuts: Vec<u64>,
}

struct MapState {
    component: Option<ComponentUid>,
    line: usize,
}

struct Importer<'a, K: Kernel> {
    doc: &'a mut Document<K>,
    file: &'a IfcFile,
    model: &'a Model,
    options: &'a IfcOptions,
    report: IfcReport,
    /// The components of spatial elements and their coordinates in the
    /// project's.
    spatial: HashMap<u64, (ComponentUid, Matrix)>,
    maps: HashMap<u64, MapState>,
    /// Document appearances by colour.
    appearances: HashMap<String, String>,
    /// Body names given.
    names: HashSet<String>,
    /// Feature names taken.
    feature_names: HashSet<String>,
    lines: BTreeMap<u64, ProductLine>,
    /// Import notes by feature.
    notes: Vec<(FeatureUid, String)>,
    stopped: Option<String>,
    /// The undo depth before the import and its step's label: the steps
    /// of each element are merged at once, so that the undo stack does
    /// not keep a state per element.
    depth: usize,
    label: String,
    /// The elements' properties, added at the end (mitcad#496).
    properties: Vec<(PropertyOwner, String, Property)>,
    /// The components that have their type's properties.
    typed: HashSet<ComponentUid>,
}

/// Imports an IFC file into the document (one undo step); `name` is the
/// file's name for labels and reports.
pub fn import_ifc<K: Kernel>(
    doc: &mut Document<K>,
    file: &IfcFile,
    name: &str,
    options: &IfcOptions,
) -> IfcReport {
    let started = std::time::Instant::now();
    let model = Model::read(file);
    let depth = doc.undo_depth();
    let empty = doc.features().next().is_none();
    let mut report = IfcReport {
        file: name.to_owned(),
        schema: file.schema.name().to_owned(),
        schema_name: file.header.schemas.join(", "),
        view_definition: file.header.view_definition(),
        originating_system: file.header.originating_system.clone(),
        project: model.project_name.clone(),
        length_unit: model.units.length_name.clone(),
        angle_unit: model.units.angle_name.clone(),
        ..IfcReport::default()
    };
    report.warnings.extend(file.warnings.iter().cloned());
    report.warnings.extend(model.units.notes.iter().cloned());
    report.warnings.extend(model.notes.iter().cloned());
    if empty
        && let Some(unit) = document_unit(&model.units.length_name)
        && let Some((length, 1)) = mitcad_model::expr::Unit::parse(unit)
            .ok()
            .and_then(|u| u.length())
        && doc.set_units(length).is_ok()
    {
        report.units = Some(unit.to_owned());
    }
    let feature_names: HashSet<String> = doc.features().map(|f| f.name.clone()).collect();
    let mut importer = Importer {
        doc,
        file,
        model: &model,
        options,
        report,
        spatial: HashMap::new(),
        maps: HashMap::new(),
        appearances: HashMap::new(),
        names: HashSet::new(),
        feature_names,
        lines: BTreeMap::new(),
        notes: Vec::new(),
        stopped: None,
        depth,
        label: format!("Import {name}"),
        properties: Vec::new(),
        typed: HashSet::new(),
    };
    importer.structure();
    importer.levels_and_grids();
    importer.elements();
    importer.others();
    let mut report = importer.finish();
    doc.merge_undo(depth, &format!("Import {name}"));
    report.seconds = started.elapsed().as_secs_f64();
    report
}

/// The model's name of a length unit the file names.
fn document_unit(name: &str) -> Option<&'static str> {
    let n = name.to_ascii_uppercase();
    Some(match n.as_str() {
        "MILLI METRE" => "mm",
        "CENTI METRE" => "cm",
        "METRE" => "m",
        "FOOT" | "FEET" => "ft",
        "INCH" => "in",
        _ => return None,
    })
}

fn rigid(m: &Matrix) -> bool {
    math::is_rigid(m, 1e-6)
}

fn transform_of(m: &Matrix) -> Result<mitcad_model::Transform, String> {
    mitcad_model::assembly::from_rows(&math::rows(m))
}

/// A product's line before it is built.
fn line_of(p: &Product) -> ProductLine {
    ProductLine {
        id: p.id,
        global_id: p.global_id.clone(),
        kind: p.kind.to_string(),
        class: p.class.name().to_owned(),
        name: p.name.clone(),
        outcome: Outcome::NotImported,
        component: None,
        bodies: Vec::new(),
        occurrences: Vec::new(),
        features: 0,
        parametric: Vec::new(),
        stored: Vec::new(),
        volume: None,
        area: None,
        quantity: None,
        quantity_difference: None,
        notes: Vec::new(),
    }
}

impl<K: Kernel> Importer<'_, K> {
    fn component_name(&self, component: ComponentUid) -> String {
        component.to_string()
    }

    // The spatial structure.

    fn structure(&mut self) {
        let roots = self.model.roots.clone();
        for root in roots {
            self.spatial_component(root, self.options.parent, math::IDENTITY, 0);
        }
    }

    fn spatial_component(
        &mut self,
        id: u64,
        parent: ComponentUid,
        parent_abs: Matrix,
        depth: usize,
    ) {
        let Some(p) = self.model.products.get(&id) else {
            return;
        };
        if p.class != ProductKind::Spatial || self.spatial.contains_key(&id) || depth > 64 {
            return;
        }
        let mut line = line_of(p);
        let relative = math::mul(
            &math::inverse(&parent_abs).unwrap_or(math::IDENTITY),
            &p.placement,
        );
        let (relative, placement) = if rigid(&relative) {
            (relative, p.placement)
        } else {
            line.notes
                .push("its placement is not rigid: placed at its parent's origin".to_owned());
            (math::IDENTITY, parent_abs)
        };
        if let Some(note) = &p.placement_note {
            line.notes.push(format!("placement: {note}"));
        }
        let made = transform_of(&relative)
            .map_err(|e| e.to_string())
            .and_then(|t| {
                self.doc
                    .add_component(Some(&p.label()), parent, t)
                    .map_err(|e| e.to_string())
            });
        match made {
            Ok((component, _)) => {
                self.spatial.insert(id, (component, placement));
                line.outcome = Outcome::Component;
                line.component = Some(self.component_name(component));
                self.report.components += 1;
                self.lines.insert(id, line);
                let mut children: Vec<u64> = p.parts.clone();
                children.sort_unstable();
                for c in children {
                    self.spatial_component(c, component, placement, depth + 1);
                }
            }
            Err(e) => {
                line.outcome = Outcome::Failed;
                line.notes.push(format!("no component: {e}"));
                self.lines.insert(id, line);
            }
        }
    }

    /// The component a product's shape goes into, and its coordinates in
    /// the project's.
    fn component_of(&self, id: u64) -> (ComponentUid, Matrix) {
        if let Some(c) = self.spatial.get(&id) {
            return *c;
        }
        self.model
            .spatial_parent(id)
            .and_then(|s| self.spatial.get(&s).copied())
            .unwrap_or((self.options.parent, math::IDENTITY))
    }

    /// Storeys' elevations as construction planes and grids' axes as
    /// construction axes, in one command.
    fn levels_and_grids(&mut self) {
        let mut planes: Vec<(ComponentUid, f64, String, u64)> = Vec::new();
        // (component, origin, direction, name, grid axis)
        type Axis = (ComponentUid, [f64; 3], [f64; 3], String, u64);
        let mut axes: Vec<Axis> = Vec::new();
        for p in self.model.products.values() {
            if &*p.kind == "IFCBUILDINGSTOREY"
                && let Some((component, abs)) = self.spatial.get(&p.id)
            {
                // The elevation above the storey's own origin, when the
                // file's elevation and its placement differ.
                let z = p
                    .elevation
                    .map(|e| e - math::origin(abs)[2])
                    .filter(|d| d.abs() > 1e-6)
                    .unwrap_or(0.0);
                planes.push((*component, z, format!("{} level", p.label()), p.id));
            }
            if p.class == ProductKind::Grid {
                let mut line = line_of(p);
                let (component, abs) = self.component_of(p.id);
                let inverse = math::inverse(&abs).unwrap_or(math::IDENTITY);
                let (grid_axes, notes) = self.model.grid_axes(self.file, p);
                line.notes.extend(notes);
                line.component = Some(self.component_name(component));
                if grid_axes.is_empty() {
                    line.notes.push("a grid without axes".to_owned());
                } else {
                    line.outcome = Outcome::Axes;
                }
                for a in grid_axes {
                    let s = math::apply(&inverse, a.start);
                    let e = math::apply(&inverse, a.end);
                    axes.push((
                        component,
                        s,
                        math::sub(e, s),
                        format!("{} {}", p.label(), a.tag),
                        p.id,
                    ));
                }
                self.lines.insert(p.id, line);
            }
        }
        if planes.is_empty() && axes.is_empty() {
            return;
        }
        let result = self.doc.add_features_with("Import levels and grids", |batch| {
            let mut made = (0, 0, Vec::new());
            for (component, z, name, _) in &planes {
                let def = json!({"type": "construction_plane", "definition": {"type": "fixed",
                    "origin": [0.0, 0.0, z], "x_axis": [1.0, 0.0, 0.0], "y_axis": [0.0, 1.0, 0.0]}});
                let def = serde_json::from_value(def).expect("a plane definition");
                if batch.add(&def, Some(name), *component).is_ok() {
                    made.0 += 1;
                }
            }
            for (component, origin, direction, name, grid) in &axes {
                if math::normalize(*direction).is_none() {
                    made.2.push((*grid, format!("axis {name} has no length")));
                    continue;
                }
                let def = json!({"type": "construction_axis", "definition": {"type": "fixed",
                    "origin": origin, "direction": direction}});
                let def = serde_json::from_value(def).expect("an axis definition");
                match batch.add(&def, Some(name), *component) {
                    Ok(_) => made.1 += 1,
                    Err(e) => made.2.push((*grid, format!("axis {name}: {e}"))),
                }
            }
            Ok(made)
        });
        match result {
            Ok((p, a, failed)) => {
                self.report.storey_planes = p;
                self.report.grid_axes = a;
                for (grid, note) in failed {
                    if let Some(line) = self.lines.get_mut(&grid) {
                        line.notes.push(note);
                    }
                }
            }
            Err(e) => self
                .report
                .warnings
                .push(format!("levels and grids not added: {e}")),
        }
    }

    // Elements.

    fn elements(&mut self) {
        let ids: Vec<u64> = self
            .model
            .products
            .values()
            .filter(|p| {
                p.class == ProductKind::Element
                    || (p.class == ProductKind::Spatial && p.shape.is_some())
            })
            .map(|p| p.id)
            .collect();
        for id in ids {
            if let Some(progress) = &self.options.progress {
                progress.step.fetch_add(1, Ordering::Relaxed);
                progress.item.store(id as i64, Ordering::Relaxed);
                if self.stopped.is_none() && progress.is_memory_low() {
                    self.stopped = Some(format!(
                        "{}; the import stopped before #{id}",
                        progress.memory()
                    ));
                }
                if progress.abandoned() {
                    self.stopped = Some("the import was given up".to_owned());
                }
            }
            if self.stopped.is_some() {
                let p = &self.model.products[&id];
                if p.class == ProductKind::Element {
                    let mut line = line_of(p);
                    line.notes
                        .push("not imported: the process ran low on memory".to_owned());
                    self.lines.insert(id, line);
                }
                continue;
            }
            self.element(id);
            self.doc.merge_undo(self.depth, &self.label);
        }
    }

    /// The colour of a style as a document appearance (made once).
    fn appearance(&mut self, style: &Style) -> Option<String> {
        let key = format!(
            "{:.4} {:.4} {:.4} {:.4}",
            style.color[0], style.color[1], style.color[2], style.transparency
        );
        if let Some(id) = self.appearances.get(&key) {
            return Some(id.clone());
        }
        let name = if style.name.trim().is_empty() {
            format!(
                "IFC colour {:.0} {:.0} {:.0}",
                style.color[0] * 255.0,
                style.color[1] * 255.0,
                style.color[2] * 255.0
            )
        } else {
            style.name.trim().to_owned()
        };
        let change = mitcad_model::appearance::AppearanceChange {
            name: Some(name),
            base_color: Some(style.color),
            opacity: Some((1.0 - style.transparency).clamp(0.0, 1.0)),
            ..Default::default()
        };
        match self.doc.create_appearance(&change) {
            Ok(id) => {
                self.appearances.insert(key, id.clone());
                self.report.appearances += 1;
                Some(id)
            }
            Err(e) => {
                self.report.warnings.push(format!(
                    "appearance {}: {e}",
                    change.name.unwrap_or_default()
                ));
                None
            }
        }
    }

    /// The items of a map moved by `m` (its mapped items' maps too).
    /// `visits` counts the items expanded for one product; past
    /// [`MAX_EXPANDED_ITEMS`] nothing more is expanded (see
    /// [`too_many_expanded`]).
    #[allow(clippy::too_many_arguments)]
    fn expand(
        &self,
        map: u64,
        m: &Matrix,
        style: Option<&Style>,
        product: Option<&Product>,
        depth: usize,
        visits: &mut usize,
        out: &mut Vec<(Item, Option<Style>)>,
    ) {
        if *visits > MAX_EXPANDED_ITEMS {
            return;
        }
        let decoder = Decoder::new(self.file, &self.model.units);
        let Some(rep) = self.model.map_representation(self.file, map) else {
            out.push((
                Item::Unsupported {
                    id: map,
                    kind: "IFCREPRESENTATIONMAP".to_owned(),
                    why: "it holds no representation".to_owned(),
                },
                None,
            ));
            return;
        };
        for i in rep.items {
            *visits += 1;
            if *visits > MAX_EXPANDED_ITEMS {
                return;
            }
            let item = decoder.item(i);
            let own = self.model.style(i, product).or(style).cloned();
            match item {
                Item::Mapped {
                    map: inner,
                    transform,
                } if depth < 16 => {
                    self.expand(
                        inner,
                        &math::mul(m, &transform),
                        own.as_ref(),
                        product,
                        depth + 1,
                        visits,
                        out,
                    );
                }
                other => out.push((moved(&other, m), own)),
            }
        }
    }

    fn element(&mut self, id: u64) {
        let model = self.model;
        let p = &model.products[&id];
        let mut line = line_of(p);
        let (component, abs) = self.component_of(id);
        line.component = Some(self.component_name(component));
        if let Some(note) = &p.placement_note {
            line.notes.push(format!("placement: {note}"));
        }
        let Some(rep) = model.body(self.file, p) else {
            let reps: Vec<String> = model
                .representations(self.file, p)
                .iter()
                .map(|r| format!("{}/{}", r.identifier, r.kind))
                .collect();
            line.notes.push(if reps.is_empty() && !p.parts.is_empty() {
                format!(
                    "not imported: it has no shape of its own; its {} parts are imported on their own",
                    p.parts.len()
                )
            } else if reps.is_empty() {
                "not imported: it has no shape".to_owned()
            } else {
                format!("not imported: no body representation ({})", reps.join(", "))
            });
            self.lines.insert(id, line);
            return;
        };
        let inverse = math::inverse(&abs).unwrap_or(math::IDENTITY);
        let to_component = math::mul(&inverse, &p.placement);
        let openings: Vec<u64> = if self.options.openings {
            p.openings.clone()
        } else {
            Vec::new()
        };
        let decoder = Decoder::new(self.file, &model.units);
        let mut items: Vec<(Item, Option<Style>)> = Vec::new();
        // The items expanded from maps for this product and its openings.
        let mut visits = 0;
        for i in &rep.items {
            let item = decoder.item(*i);
            let style = model.style(*i, Some(p)).cloned();
            match item {
                Item::Mapped { map, transform } => {
                    let m = math::mul(&to_component, &transform);
                    if openings.is_empty() && self.options.occurrences && rigid(&m) {
                        match self.occurrence(map, component, &m, p) {
                            Ok(o) => {
                                line.occurrences.push(o);
                                continue;
                            }
                            Err(e) => line
                                .notes
                                .push(format!("map #{map} as an occurrence: {e}; copied instead")),
                        }
                    }
                    self.expand(map, &m, style.as_ref(), Some(p), 0, &mut visits, &mut items);
                }
                other => items.push((moved(&other, &to_component), style)),
            }
        }
        // The openings' shapes, in the component.
        let mut tools: Vec<(u64, Vec<Item>)> = Vec::new();
        for o in &openings {
            let Some(op) = model.products.get(o) else {
                continue;
            };
            let mut shapes = Vec::new();
            if let Some(orep) = model.body(self.file, op) {
                let m = math::mul(&inverse, &op.placement);
                for i in orep.items {
                    match decoder.item(i) {
                        Item::Mapped { map, transform } => {
                            let mut inner = Vec::new();
                            self.expand(
                                map,
                                &math::mul(&m, &transform),
                                None,
                                Some(op),
                                0,
                                &mut visits,
                                &mut inner,
                            );
                            shapes.extend(inner.into_iter().map(|(i, _)| i));
                        }
                        other => shapes.push(moved(&other, &m)),
                    }
                }
            }
            tools.push((*o, shapes));
        }
        if visits > MAX_EXPANDED_ITEMS {
            // Not a part of the shape: none of it.
            items = vec![(too_many_expanded(p.id, &p.kind), None)];
            tools.clear();
        }
        let made = self.build(component, &p.label(), &p.global_id, items, tools);
        self.finish_element(p, line, made);
    }

    /// Builds items (and cuts openings' shapes from them) in a component in
    /// one command.
    fn build(
        &mut self,
        component: ComponentUid,
        label: &str,
        global_id: &str,
        items: Vec<(Item, Option<Style>)>,
        tools: Vec<(u64, Vec<Item>)>,
    ) -> Made {
        let mut made = Made::default();
        let kernel = self.doc.kernel();
        let nodes: Vec<(Node, Option<Style>)> = items
            .into_iter()
            .map(|(i, s)| (build::prepare(kernel, i), s))
            .collect();
        let tool_nodes: Vec<(u64, Vec<Node>)> = tools
            .into_iter()
            .map(|(o, items)| {
                (
                    o,
                    items
                        .into_iter()
                        .map(|i| build::prepare(kernel, i))
                        .collect(),
                )
            })
            .collect();
        for (n, _) in &nodes {
            if let Node::Body(p) = n
                && p.mesh
            {
                made.meshes += 1;
                if let Some(v) = p.volume {
                    made.mesh_volume += v;
                }
                made.mesh_area += p.area.unwrap_or(0.0);
            }
        }
        let mut appearances: Vec<Option<String>> = Vec::new();
        for (_, style) in &nodes {
            appearances.push(style.as_ref().and_then(|s| self.appearance(s)));
        }
        // Unique body names: "<label> <GlobalId>", then " (2)", ...
        let base = if global_id.is_empty() {
            label.to_owned()
        } else {
            format!("{label} {global_id}")
        };
        let names = &mut self.names;
        let mut name_of = |k: usize| -> String {
            let first = if k == 0 {
                base.clone()
            } else {
                format!("{base} ({})", k + 1)
            };
            let mut name = first.clone();
            let mut n = 2;
            while names.contains(&name) {
                name = format!("{first} ({n})");
                n += 1;
            }
            names.insert(name.clone());
            name
        };
        let feature_label = if label.trim().is_empty() {
            "IFC element".to_owned()
        } else {
            label.to_owned()
        };
        let feature_names = &mut self.feature_names;
        let result = self.doc.add_features_with(&format!("Import {feature_label}"), |batch| {
            let mut b = Builder::new(batch, feature_names, component, &feature_label);
            let mut out_bodies: Vec<(BodyUid, Option<String>, bool)> = Vec::new();
            let mut failed = Vec::new();
            for ((node, _), appearance) in nodes.iter().zip(&appearances) {
                match b.node(node, Op::New) {
                    Ok(bodies) => {
                        let mesh = matches!(node, Node::Body(p) if p.mesh);
                        for body in bodies {
                            if !out_bodies.iter().any(|(o, _, _)| *o == body) {
                                out_bodies.push((body, appearance.clone(), mesh));
                            }
                        }
                    }
                    Err(why) => failed.push(why),
                }
            }
            // Meshes cannot be cut.
            let solids: Vec<BodyUid> = out_bodies
                .iter()
                .filter(|(_, _, mesh)| !mesh)
                .map(|(b, _, _)| *b)
                .collect();
            let mut cuts = Vec::new();
            b.label = format!("{feature_label} opening");
            for (opening, shapes) in &tool_nodes {
                if shapes.is_empty() {
                    b.notes
                        .push(format!("opening #{opening} has no shape to cut"));
                    continue;
                }
                if solids.is_empty() {
                    b.notes.push(if out_bodies.is_empty() {
                        format!("opening #{opening}: no body to cut it from")
                    } else {
                        format!(
                            "opening #{opening} is not cut: meshes are not cut (a tessellated shape holds its openings)"
                        )
                    });
                    continue;
                }
                let mut ok = true;
                for tool in shapes {
                    if let Err(why) = b.node(tool, Op::With("cut", solids.clone())) {
                        b.notes.push(format!("opening #{opening}: {why}"));
                        ok = false;
                    }
                }
                if ok {
                    cuts.push(*opening);
                }
            }
            for (k, (body, appearance, _)) in out_bodies.iter().enumerate() {
                let _ = b.batch.name_body(*body, &name_of(k));
                if let Some(a) = appearance {
                    let _ = b.batch.set_body_appearance(*body, a);
                }
            }
            Ok((
                b.features.clone(),
                b.parametric.clone(),
                b.stored.clone(),
                b.notes.clone(),
                failed,
                cuts,
            ))
        });
        match result {
            Ok((features, parametric, stored, notes, failed, cuts)) => {
                made.features = features;
                made.parametric = parametric;
                made.stored = stored;
                made.notes = notes;
                made.failed = failed;
                made.cuts = cuts;
            }
            Err(e) => {
                made.failed.push(format!("its features were refused: {e}"));
                return made;
            }
        }
        // The features that failed when the batch computed. A clipping or an
        // opening that reaches none of the element's material removes
        // nothing: the file's shape is the same without it, so it stays on
        // the timeline suppressed, with a note.
        let mut misses = Vec::new();
        for uid in &made.features {
            if let Some(error) = self.doc.status(*uid).and_then(|s| s.error()) {
                let name = self
                    .doc
                    .features()
                    .find(|f| f.uid == *uid)
                    .map(|f| f.name.clone())
                    .unwrap_or_default();
                if is_miss(error) {
                    misses.push((*uid, name));
                } else {
                    made.failed.push(format!("{name}: {error}"));
                }
            }
        }
        for (uid, name) in misses {
            match self.doc.set_suppressed(uid, true) {
                Ok(()) => {
                    made.notes.push(format!(
                        "{name} removes nothing from the element: suppressed"
                    ));
                    self.notes.push((
                        uid,
                        "the IFC file's cut removes nothing from the element here".to_owned(),
                    ));
                }
                Err(e) => made.failed.push(format!("{name}: {e}")),
            }
        }
        let features: HashSet<FeatureUid> = made.features.iter().copied().collect();
        let kernel = self.doc.kernel();
        let mut volume = 0.0;
        let mut measured = false;
        let mut area = made.mesh_area;
        let mut unnamed = Vec::new();
        for body in self.doc.bodies() {
            if !features.contains(&body.uid.feature) {
                continue;
            }
            made.bodies.push(body.uid);
            if !self.names.contains(&body.name) {
                unnamed.push(body.uid);
            }
            // Solids only: a surface body encloses nothing, and meshes are
            // measured from their triangles.
            let kind = kernel.body_kind(body.shape).ok();
            if matches!(kind, Some(BodyKind::Solid | BodyKind::Sheet))
                && let Ok(m) = kernel.mass_properties(body.shape)
            {
                area += m.area;
                if kind == Some(BodyKind::Solid) {
                    volume += m.volume;
                    measured = true;
                }
            }
        }
        if made.meshes > 0 && made.mesh_volume > 0.0 {
            volume += made.mesh_volume;
            measured = true;
        }
        made.volume = measured.then_some(volume);
        made.area = (!made.bodies.is_empty()).then_some(area);
        // Bodies the batch did not foresee (pieces of a profile of several
        // areas, of a cut): named after the element too.
        for uid in unnamed {
            let n = made.bodies.len() + self.names.len();
            let name = format!("{base} ({n})");
            if self.doc.rename_body(uid, &name).is_ok() {
                self.names.insert(name);
            }
        }
        made
    }

    /// An occurrence of a map's component (made, with the map's items, at
    /// its first use).
    fn occurrence(
        &mut self,
        map: u64,
        parent: ComponentUid,
        m: &Matrix,
        product: &Product,
    ) -> Result<String, String> {
        let transform = transform_of(m)?;
        if let Some(state) = self.maps.get(&map) {
            let component = state.component.ok_or("its component was not made")?;
            let line = state.line;
            let occurrence = self
                .doc
                .add_occurrence(component, parent, transform)
                .map_err(|e| e.to_string())?;
            self.report.maps[line].occurrences += 1;
            return Ok(occurrence.to_string());
        }
        let name = self
            .model
            .map_types
            .get(&map)
            .and_then(|t| self.file.get(*t))
            .and_then(|t| t.arg(2).as_str().map(str::to_owned))
            .filter(|n| !n.trim().is_empty())
            .unwrap_or_else(|| format!("{} type", product.label()));
        let (component, occurrence) = self
            .doc
            .add_component(Some(&name), parent, transform)
            .map_err(|e| e.to_string())?;
        let mut items = Vec::new();
        let mut visits = 0;
        self.expand(
            map,
            &math::IDENTITY,
            None,
            Some(product),
            0,
            &mut visits,
            &mut items,
        );
        if visits > MAX_EXPANDED_ITEMS {
            items = vec![(too_many_expanded(map, "IFCREPRESENTATIONMAP"), None)];
        }
        let made = self.build(component, &name, "", items, Vec::new());
        let mut notes = made.notes.clone();
        notes.extend(made.failed.iter().map(|f| format!("not built: {f}")));
        if let Some(first) = made.features.first()
            && !made.failed.is_empty()
        {
            self.notes.push((
                *first,
                format!(
                    "partly imported from the IFC file: {}",
                    made.failed.join("; ")
                ),
            ));
        }
        self.report.features += made.features.len();
        self.report.bodies += made.bodies.len();
        self.report.maps.push(MapLine {
            map,
            name,
            component: Some(self.component_name(component)),
            occurrences: 1,
            bodies: made.bodies.len(),
            volume: made.volume,
            area: made.area,
            notes,
        });
        self.maps.insert(
            map,
            MapState {
                component: Some(component),
                line: self.report.maps.len() - 1,
            },
        );
        Ok(occurrence.to_string())
    }

    fn finish_element(&mut self, p: &Product, mut line: ProductLine, made: Made) {
        self.element_properties(p, &line, &made);
        line.features = made.features.len();
        line.bodies = made.bodies.iter().map(ToString::to_string).collect();
        line.parametric = made.parametric.iter().map(|s| (*s).to_owned()).collect();
        line.stored = made.stored.iter().map(|s| (*s).to_owned()).collect();
        line.volume = made.volume;
        line.area = made.area;
        line.notes.extend(made.notes.iter().cloned());
        line.notes
            .extend(made.failed.iter().map(|f| format!("not built: {f}")));
        self.report.features += made.features.len();
        self.report.bodies += made.bodies.len();
        let built = !made.bodies.is_empty();
        line.outcome = if !built && line.occurrences.is_empty() {
            Outcome::Failed
        } else if !built {
            Outcome::Occurrence
        } else if !made.failed.is_empty() {
            Outcome::Partial
        } else if made.stored.is_empty() {
            Outcome::Parametric
        } else if made.parametric.is_empty() && made.meshes == made.stored.len() {
            Outcome::Mesh
        } else if made.parametric.is_empty() && made.meshes == 0 {
            Outcome::Stored
        } else {
            Outcome::Mixed
        };
        // The openings cut from it.
        for o in &p.openings {
            let Some(op) = self.model.products.get(o) else {
                continue;
            };
            let mut ol = line_of(op);
            ol.component = line.component.clone();
            if made.cuts.contains(o) {
                ol.outcome = Outcome::Cut;
                ol.notes.push(format!("cut from #{}", p.id));
            } else if !self.options.openings {
                ol.notes
                    .push("not imported: openings are not cut".to_owned());
            } else {
                ol.notes.push(if made.meshes > 0 && made.parametric.is_empty() {
                    format!(
                        "not imported: its element #{} is a mesh, which is not cut (a tessellated shape holds its openings)",
                        p.id
                    )
                } else {
                    format!("not imported: not cut from #{} (see its notes)", p.id)
                });
            }
            self.lines.insert(*o, ol);
        }
        // What did not build stays on the timeline (mitcad#170).
        if !made.failed.is_empty() {
            let note = format!(
                "partly imported from the IFC file: {}",
                made.failed.join("; ")
            );
            match made.features.first() {
                Some(first) => self.notes.push((*first, note)),
                None if line.occurrences.is_empty() => {
                    let (component, _) = self.component_of(p.id);
                    let note = format!(
                        "{} {} '{}' ({}) not imported from the IFC file: {}",
                        p.kind,
                        p.global_id,
                        p.name,
                        p.id,
                        made.failed.join("; ")
                    );
                    if let Err(e) = self.doc.add_import_placeholder(
                        Some(&p.label()),
                        &format!("IFC {}", p.kind),
                        &note,
                        Some(component),
                    ) {
                        line.notes.push(format!("no placeholder: {e}"));
                    }
                }
                None => {}
            }
        }
        self.lines.insert(p.id, line);
    }

    /// Lines of the products that are not built.
    fn others(&mut self) {
        for p in self.model.products.values() {
            if self.lines.contains_key(&p.id) {
                continue;
            }
            let mut line = line_of(p);
            let (component, _) = self.component_of(p.id);
            line.component = Some(self.component_name(component));
            let why = match p.class {
                ProductKind::Space => "spaces are not imported".to_owned(),
                ProductKind::Annotation => "annotations are not imported".to_owned(),
                ProductKind::Port => "ports are not imported".to_owned(),
                ProductKind::Structural => {
                    "the structural analysis model is not imported".to_owned()
                }
                ProductKind::Virtual => "virtual elements are not imported".to_owned(),
                ProductKind::Opening => match p.voids {
                    Some(host) => format!("its element #{host} was not imported"),
                    None => "an opening of no element".to_owned(),
                },
                ProductKind::Spatial => "outside the project's spatial structure".to_owned(),
                ProductKind::Grid | ProductKind::Element => "not reached".to_owned(),
            };
            line.notes.push(format!("not imported: {why}"));
            self.lines.insert(p.id, line);
        }
    }

    /// An element's properties (mitcad#496): its identity and own sets on
    /// its bodies and occurrences; its type's sets on the component its
    /// type's shape became (once), and on its own bodies, which have no
    /// component of their type.
    fn element_properties(&mut self, p: &Product, line: &ProductLine, made: &Made) {
        if made.bodies.is_empty() && line.occurrences.is_empty() {
            return;
        }
        let own = properties::instance_properties(self.file, self.model, p);
        let typed = properties::type_properties(self.file, self.model, p);
        for body in &made.bodies {
            let owner = PropertyOwner::Body(*body);
            for (set, property) in typed.iter().chain(&own) {
                self.properties.push((owner, set.clone(), property.clone()));
            }
        }
        for occurrence in &line.occurrences {
            let Ok(uid) = occurrence.parse::<OccurrenceUid>() else {
                continue;
            };
            for (set, property) in &own {
                self.properties.push((
                    PropertyOwner::Occurrence(uid),
                    set.clone(),
                    property.clone(),
                ));
            }
            let Some(component) = self.doc.assembly().occurrence(uid).map(|o| o.component) else {
                continue;
            };
            if self.typed.insert(component) {
                for (set, property) in &typed {
                    self.properties.push((
                        PropertyOwner::Component(component),
                        set.clone(),
                        property.clone(),
                    ));
                }
            }
        }
    }

    fn finish(mut self) -> IfcReport {
        // The elements' properties (mitcad#496).
        let properties = std::mem::take(&mut self.properties);
        self.report.properties = self.doc.import_properties(&self.label, properties);
        if !self.notes.is_empty()
            && let Err(e) = self.doc.set_import_notes(&self.notes)
        {
            self.report.warnings.push(format!("import notes: {e}"));
        }
        // The volumes against the file's quantities.
        let mut check = QuantityCheck::default();
        for (id, line) in self.lines.iter_mut() {
            let Some(p) = self.model.products.get(id) else {
                continue;
            };
            let Some(volume) = line.volume else {
                continue;
            };
            if p.volumes.is_empty() {
                continue;
            }
            let cut = !p.openings.is_empty() && self.options.openings;
            let pick = |name: &str| p.volumes.iter().find(|(n, _)| n.eq_ignore_ascii_case(name));
            let Some((name, q)) = (if cut {
                pick("NetVolume").or(pick("GrossVolume"))
            } else {
                pick("GrossVolume").or(pick("NetVolume"))
            })
            .or(p.volumes.first())
            .cloned() else {
                continue;
            };
            if q <= 0.0 {
                continue;
            }
            let d = (volume - q).abs() / q;
            check.compared += 1;
            check.worst = check.worst.max(d);
            if d <= QUANTITY_TOLERANCE {
                check.agree += 1;
            } else if check.differ.len() < 50 {
                check.differ.push(format!(
                    "#{id} {} '{}': {volume:.1} mm3, the file's {name} {q:.1} mm3 ({d:.2e})",
                    p.kind, p.name
                ));
            }
            line.quantity = Some((name, q));
            line.quantity_difference = Some(d);
        }
        self.report.quantities = check;
        self.report.total_volume = self
            .lines
            .values()
            .filter(|l| l.outcome != Outcome::Component)
            .filter_map(|l| l.volume)
            .sum::<f64>()
            + self
                .report
                .maps
                .iter()
                .map(|m| m.volume.unwrap_or(0.0) * m.occurrences as f64)
                .sum::<f64>();
        self.report.total_area = self
            .lines
            .values()
            .filter(|l| l.outcome != Outcome::Component)
            .filter_map(|l| l.area)
            .sum::<f64>()
            + self
                .report
                .maps
                .iter()
                .map(|m| m.area.unwrap_or(0.0) * m.occurrences as f64)
                .sum::<f64>();
        self.report.low_memory = self.stopped.clone();
        let mut outcomes: BTreeMap<String, usize> = BTreeMap::new();
        for line in self.lines.values() {
            *outcomes.entry(line.outcome.name().to_owned()).or_default() += 1;
        }
        self.report.outcomes = outcomes;
        self.report.products = std::mem::take(&mut self.lines).into_values().collect();
        self.report
    }
}

/// A cut that failed only because it reaches none of its bodies.
pub fn is_miss(error: &str) -> bool {
    error.contains("does not cut into any participant body")
        || error.contains("there are no bodies in the extrude direction")
}

/// How many items one product (with its openings), or one map's
/// component, may expand from its mapped items, nested maps included
/// (mitcad#930: maps that each map the one below twice would expand 2^16
/// items at the nesting limit). A file's products expand a few dozen.
const MAX_EXPANDED_ITEMS: usize = 10_000;

/// What is left of a product's or a map's items when expanding its maps
/// passed [`MAX_EXPANDED_ITEMS`]: a note, not a part of the shape.
fn too_many_expanded(id: u64, kind: &str) -> Item {
    Item::Unsupported {
        id,
        kind: kind.to_owned(),
        why: format!("{TOO_MANY_ITEMS} (more than {MAX_EXPANDED_ITEMS} mapped items)"),
    }
}

/// An item moved into a component, or an unsupported one that says why
/// not.
fn moved(item: &Item, m: &Matrix) -> Item {
    match transform::transformed(item, m) {
        Ok(i) => i,
        Err(why) => Item::Unsupported {
            id: 0,
            kind: item.name().to_owned(),
            why,
        },
    }
}

#[cfg(test)]
mod tests;
