// SPDX-License-Identifier: MIT
//! The `.3dm` import (mitcad#1715): `mitcad_3dm` reads the model and
//! converts its B-reps, extrusions and surfaces to the neutral B-rep model,
//! which OCCT builds (`build_brep_body`); its meshes become mesh bodies.
//! Bodies only: a `.3dm` model keeps no feature history.
//!
//! - The bodies of each layer are one base feature named after the layer
//!   (its full path, `Parent::Child`); a body is named after its object
//!   where the object has a name. Its colour is the object's display
//!   colour (its own or its layer's); black, the format's default, leaves
//!   MitCad's.
//! - A block definition is a component, placed by an occurrence for each
//!   instance that is a rotation and a translation; an instance that scales
//!   or mirrors its block is copied in place into the component that holds
//!   it. Nested blocks keep their nesting.
//! - A B-rep that cannot be built comes in as its render mesh where the
//!   file has one.
//! - The model's unit becomes the document's (an empty document only, and
//!   for mm, cm, m, in and ft); the bodies are millimetres either way.
//!
//! Hidden objects (and those on hidden layers), curves, points,
//! annotations, SubD objects and other classes are left out; the report
//! accounts for every object. The `import_3dm` document command.
//!
//! The file is untrusted: the reader checks every length and count
//! (`core/3dm`), a block that holds an instance of itself (directly or
//! through others) is left out at that instance, copies of blocks stop at
//! [`MAX_COPIES`], and a panic in the reader or in an object's conversion
//! is taken as that file or object not being read, not as a crash of the
//! import.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::panic::{AssertUnwindSafe, catch_unwind};

use serde::Deserialize;
use serde_json::{Value, json};

use mitcad_3dm::File3dm;
use mitcad_3dm::convert::{self, Converted};
use mitcad_3dm::geom::{Geometry, InstanceRef};
use mitcad_3dm::model::Object;
use mitcad_asm::convert::ConvertedBody;
use mitcad_import::outcomes::EntityOutcome;
use mitcad_model::notice::Notice;
use mitcad_model::{BaseInput, ComponentUid, ImportBody, Kernel, Transform};
use mitcad_model_api::ApiError;

use crate::Document;
use crate::brep_import::{Source, to_ffi};
use crate::kernel::exchange::ffi::{build_brep_body, catch_occt_crashes};
use crate::kernel::shape::ffi::new_shape_list;
use crate::kernel::{OcctKernel, Shape};

use cxx::SharedPtr;

/// The most copies of blocks (instances that scale or mirror, each copied
/// with its bodies) one import makes: nested blocks of such instances
/// multiply, and a file could ask for billions.
const MAX_COPIES: usize = 10_000;

/// The most bodies copied for such instances in one import.
const MAX_COPIED_BODIES: usize = 100_000;

/// The value of `f`, or `None` when it panics (a damaged object that the
/// reader's checks let through is then left out, not the import ended).
fn without_panic<T>(f: impl FnOnce() -> T) -> Option<T> {
    catch_unwind(AssertUnwindSafe(f)).ok()
}

/// The options of `import_3dm` (JSON).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Options {
    /// A readable report instead of JSON.
    #[serde(default)]
    text: bool,
    /// Also write the JSON report to this file.
    #[serde(default)]
    report_path: Option<String>,
}

fn file_name(path: &str) -> &str {
    std::path::Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(path)
}

/// A built body of an object.
#[derive(Clone)]
struct Built {
    shape: SharedPtr<Shape>,
    solid: bool,
    mesh: bool,
    valid: bool,
    volume: f64,
    /// Its kind, for its name: Solid, Surface, Mesh.
    kind: &'static str,
}

/// sRGB in [0, 1] from the format's `0xAABBGGRR`; None for black.
fn rgb(c: u32) -> Option<[f64; 3]> {
    let (r, g, b) = (c & 0xFF, (c >> 8) & 0xFF, (c >> 16) & 0xFF);
    (r | g | b != 0).then(|| [r, g, b].map(|v| f64::from(v) / 255.0))
}

/// The placement of an instance in millimetres; None for a projection.
fn placement(x: &[[f64; 4]; 4], mm: f64) -> Option<Transform> {
    let last = x[3];
    if last[0] != 0.0 || last[1] != 0.0 || last[2] != 0.0 || (last[3] - 1.0).abs() > 1e-12 {
        return None;
    }
    let t = Transform {
        linear: [
            [x[0][0], x[0][1], x[0][2]],
            [x[1][0], x[1][1], x[1][2]],
            [x[2][0], x[2][1], x[2][2]],
        ],
        translation: [x[0][3] * mm, x[1][3] * mm, x[2][3] * mm],
    };
    t.is_finite().then_some(t)
}

fn mesh_volume(vertices: &[[f64; 3]], triangles: &[[u32; 3]]) -> f64 {
    triangles
        .iter()
        .map(|t| {
            let [a, b, c] = t.map(|i| vertices[i as usize]);
            (a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
                + a[2] * (b[0] * c[1] - b[1] * c[0]))
                / 6.0
        })
        .sum()
}

/// What the import does with the objects, as it goes.
#[derive(Default)]
struct Account {
    /// Every object by kind.
    objects: BTreeMap<String, usize>,
    taken: BTreeMap<String, usize>,
    /// (kind, why, left out by design) -> count. By design: curves,
    /// points, annotations and the like, and hidden objects; not by
    /// design: SubD objects and objects that could not be read.
    left_out: BTreeMap<(String, String, bool), usize>,
    /// `source` (the object in words), `error`, and for agents `object`
    /// (its index in the object table).
    not_built: Vec<Value>,
    /// What a person reads: in words, without indexes, offsets or ids.
    warnings: Vec<String>,
    /// Each warning with its detail for agents (`warning`, `detail`:
    /// indexes, offsets, typecodes), also written to the log.
    details: Vec<Value>,
    fallbacks: usize,
}

impl Account {
    /// A warning in words, and its detail for agents and the log.
    fn warn(&mut self, words: String, detail: String) {
        eprintln!("import: .3dm: {detail}");
        self.details
            .push(json!({"warning": words, "detail": detail}));
        self.warnings.push(words);
    }

    fn leave(&mut self, kind: &str, why: &str, design: bool) {
        *self
            .left_out
            .entry((kind.to_owned(), why.to_owned(), design))
            .or_insert(0) += 1;
    }
    fn take(&mut self, kind: &str) {
        *self.taken.entry(kind.to_owned()).or_insert(0) += 1;
    }
}

/// The bodies of the objects, each object built once.
struct Builder<'a> {
    file: &'a File3dm,
    built: HashMap<usize, Vec<Built>>,
    /// The name of the block an object's id belongs to.
    block_of: HashMap<mitcad_3dm::Uuid, String>,
}

/// "a" or "an" before a kind.
fn article(kind: &str) -> &'static str {
    if kind.starts_with(['a', 'e', 'i', 'o', 'u', 'A', 'E', 'I', 'O', 'U']) {
        "an"
    } else {
        "a"
    }
}

impl Builder<'_> {
    /// An object in words for a person: its kind, its name, its block and
    /// its layer where it has them ("the B-rep "Bracket" in the block Bolt
    /// on the layer Parts"); never its index in the file, which goes to
    /// the detail for agents.
    fn describe(&self, kind: &str, attributes: Option<&mitcad_3dm::model::Attributes>) -> String {
        let name = attributes.map(|a| a.name.trim()).filter(|n| !n.is_empty());
        let mut text = match name {
            Some(name) => format!("the {kind} \"{name}\""),
            None => format!("{} {kind}", article(kind)),
        };
        if let Some(a) = attributes {
            if let Some(block) = self.block_of.get(&a.id) {
                text.push_str(&format!(" in the block {block}"));
            }
            if self.file.layer(a.layer).is_some() {
                text.push_str(&format!(" on the layer {}", self.file.layer_path(a.layer)));
            }
        }
        text
    }

    /// The object `i` in words ([`Self::describe`]).
    fn source(&self, i: usize) -> String {
        let o = &self.file.objects[i];
        self.describe(&mitcad_3dm::kind_name(&o.geometry), Some(&o.attributes))
    }

    /// A neutral body built with OCCT; None (and why) when it is not.
    fn build_brep(&self, place: &str, c: Converted) -> Result<Built, String> {
        let faces = c.body.faces.len();
        if faces == 0 {
            return Err("none of its faces could be converted".to_owned());
        }
        let check = c.body.check(1e-6);
        let body = ConvertedBody {
            record: 0,
            top_level: true,
            body: c.body,
            issues: c.issues,
            skipped_faces: c.skipped_faces,
            unknown_edges: c.unknown_edges,
            degenerate_edges: c.degenerate_edges,
            tidied: c.tidied,
            check,
            stored_blends: Vec::new(),
        };
        let source = Source {
            document: "",
            blob: place,
            asm_version: "",
            history: false,
            history_step: 0,
        };
        let data = to_ffi(&source, &body);
        let mut list = new_shape_list();
        let r = build_brep_body(&data, list.pin_mut());
        if !r.built || r.faces == 0 {
            return Err(if r.built {
                "none of its faces could be built".to_owned()
            } else {
                r.error.to_string()
            });
        }
        Ok(Built {
            shape: list.at(list.size() - 1),
            solid: r.solid,
            mesh: false,
            valid: r.valid,
            volume: r.volume,
            kind: if r.solid { "Solid" } else { "Surface" },
        })
    }

    fn build_mesh(&self, mesh: &mitcad_3dm::geom::Mesh) -> Result<Built, String> {
        let s = self.file.units.mm;
        let vertices: Vec<[f64; 3]> = mesh.vertices.iter().map(|p| p.map(|v| v * s)).collect();
        let triangles = mesh.triangles();
        if triangles.is_empty() {
            return Err("a mesh without faces".to_owned());
        }
        let shape = crate::kernel::mesh::ffi::mesh_from_triangles(
            vertices.as_flattened(),
            triangles.as_flattened(),
        )
        .map_err(|e| e.to_string())?;
        Ok(Built {
            shape,
            solid: false,
            mesh: true,
            valid: true,
            volume: mesh_volume(&vertices, &triangles).abs(),
            kind: "Mesh",
        })
    }

    /// The object's bodies (empty when it has none or is left out; the
    /// account says why).
    fn bodies(&mut self, i: usize, account: &mut Account) -> Vec<Built> {
        if let Some(b) = self.built.get(&i) {
            return b.clone();
        }
        let o = &self.file.objects[i];
        let s = self.file.units.mm;
        let place = self.source(i);
        let kind = mitcad_3dm::kind_name(&o.geometry);
        let mut out = Vec::new();
        // The conversions are this crate's Rust: a panic there (a damage
        // the reader's checks let through) leaves the object out.
        let panicked = || "the object is damaged (its conversion failed)".to_owned();
        let made = match &o.geometry {
            Geometry::Brep(b) => {
                let built = without_panic(|| convert::brep_body(b, s))
                    .ok_or_else(panicked)
                    .and_then(|c| self.build_brep(&place, c));
                match built {
                    Ok(built) => Ok(built),
                    Err(why) => match without_panic(|| convert::render_mesh(b)).flatten() {
                        Some(mesh) => self.build_mesh(&mesh).inspect(|_| {
                            account.fallbacks += 1;
                            account.warn(
                                format!("{place}: its render mesh came in, the B-rep could not be built"),
                                format!("object {i}: its render mesh came in, the B-rep could not be built ({why})"),
                            );
                        }),
                        None => Err(why),
                    },
                }
            }
            Geometry::Extrusion(e) => without_panic(|| convert::extrusion_body(e, s))
                .unwrap_or_else(|| Err(panicked()))
                .and_then(|c| self.build_brep(&place, c)),
            Geometry::Surface(srf) => without_panic(|| convert::surface_body(srf, s))
                .unwrap_or_else(|| Err(panicked()))
                .and_then(|c| self.build_brep(&place, c)),
            Geometry::Mesh(m) => self.build_mesh(m),
            // Left out, each object once (a block's however often it is
            // placed).
            Geometry::Curve(_) => {
                account.leave(&kind, "MitCad imports solids, surfaces and meshes", true);
                self.built.insert(i, out.clone());
                return out;
            }
            Geometry::Other(name) => {
                if name == "SubD" {
                    account.leave(&kind, "SubD objects are not imported yet", false);
                } else {
                    account.leave(&kind, "not a solid, a surface or a mesh", true);
                }
                self.built.insert(i, out.clone());
                return out;
            }
            Geometry::Instance(_) => return out,
        };
        match made {
            Ok(b) => {
                account.take(&kind);
                out.push(b);
            }
            Err(error) => {
                eprintln!("import: .3dm: object {i} ({place}) not built: {error}");
                account
                    .not_built
                    .push(json!({"source": place, "error": error, "object": i}));
            }
        }
        self.built.insert(i, out.clone());
        out
    }
}

/// The bodies placed so far: per component and layer, in the order they
/// came.
#[derive(Default)]
struct Placed {
    order: Vec<(ComponentUid, String)>,
    bodies: HashMap<(ComponentUid, String), Vec<ImportBody<SharedPtr<Shape>>>>,
    /// Per component, layer and kind: the bodies named so far by number.
    numbers: HashMap<(ComponentUid, String, &'static str), usize>,
    /// Per component: its bodies, its occurrences.
    counts: HashMap<ComponentUid, (usize, usize)>,
    solids: usize,
    meshes: usize,
    valid: usize,
    total: usize,
    volume: f64,
    copied: usize,
    /// The bodies those copies placed.
    copied_bodies: usize,
}

impl Placed {
    fn push(
        &mut self,
        component: ComponentUid,
        layer: String,
        name: Option<String>,
        color: Option<[f64; 3]>,
        b: &Built,
        shape: SharedPtr<Shape>,
    ) {
        // A body without a name of its own: its kind and number.
        let name = name.or_else(|| {
            let n = self
                .numbers
                .entry((component, layer.clone(), b.kind))
                .or_insert(0);
            *n += 1;
            Some(format!("{} {n}", b.kind))
        });
        let key = (component, layer);
        if !self.bodies.contains_key(&key) {
            self.order.push(key.clone());
        }
        self.bodies
            .entry(key)
            .or_default()
            .push(ImportBody { name, color, shape });
        self.counts.entry(component).or_insert((0, 0)).0 += 1;
        self.total += 1;
        self.solids += usize::from(b.solid);
        self.meshes += usize::from(b.mesh);
        self.valid += usize::from(b.valid);
        if b.solid {
            self.volume += b.volume;
        }
    }
}

/// Walks the model: the top-level objects into the root component, the
/// blocks into components.
struct Walk<'a> {
    file: &'a File3dm,
    builder: Builder<'a>,
    account: Account,
    placed: Placed,
    by_id: HashMap<mitcad_3dm::Uuid, usize>,
    /// Block definition id -> its index in the file (the first of an id).
    definitions: HashMap<mitcad_3dm::Uuid, usize>,
    /// Block definition -> its component.
    components: HashMap<mitcad_3dm::Uuid, ComponentUid>,
    /// Components in the order they were made: (definition name, uid).
    made: Vec<(String, ComponentUid)>,
    /// The blocks being placed, outermost first: an instance of one of
    /// them inside them is left out.
    expanding: Vec<mitcad_3dm::Uuid>,
}

impl Walk<'_> {
    fn visible(&self, o: &Object) -> bool {
        o.attributes.visible && self.file.layer_visible(o.attributes.layer)
    }

    /// An object (not an instance) placed in `component`, moved by
    /// `copy` when it is a copy of a block's geometry.
    fn place_object(
        &mut self,
        i: usize,
        component: ComponentUid,
        copy: Option<&Transform>,
        parent_color: Option<u32>,
    ) {
        let o = &self.file.objects[i];
        let bodies = self.builder.bodies(i, &mut self.account);
        let layer = self.file.layer_path(o.attributes.layer);
        let color = self
            .file
            .object_color(&o.attributes, parent_color)
            .and_then(rgb);
        let name =
            (!o.attributes.name.trim().is_empty()).then(|| o.attributes.name.trim().to_owned());
        for b in &bodies {
            if copy.is_some() && self.placed.copied_bodies >= MAX_COPIED_BODIES {
                self.account.leave(
                    "block instance",
                    "more bodies in copies of blocks than the import makes",
                    false,
                );
                continue;
            }
            let shape = match copy {
                None => b.shape.clone(),
                Some(t) => match OcctKernel.transform_shape(&b.shape, t, None) {
                    Ok(s) => {
                        self.placed.copied_bodies += 1;
                        s
                    }
                    Err(e) => {
                        self.account
                            .not_built
                            .push(json!({"source": format!("a copy of {}", self.builder.source(i)), "error": e.to_string(), "object": i}));
                        continue;
                    }
                },
            };
            self.placed
                .push(component, layer.clone(), name.clone(), color, b, shape);
        }
    }

    /// An instance of a block in `component`, within copies moved by
    /// `copy` (None outside copies).
    fn place_instance(
        &mut self,
        inst: &InstanceRef,
        source: usize,
        component: ComponentUid,
        copy: Option<Transform>,
        doc: &mut mitcad_model::Document<OcctKernel>,
        depth: usize,
    ) -> Result<(), ApiError> {
        if depth > 32 {
            self.account
                .leave("block instance", "blocks nested more than 32 deep", false);
            return Ok(());
        }
        let Some(&d) = self.definitions.get(&inst.definition) else {
            self.account.leave(
                "block instance",
                "its block definition is not in the file",
                false,
            );
            return Ok(());
        };
        let file = self.file;
        let def = &file.definitions[d];
        // A block placed inside itself (directly or through other blocks)
        // would be placed without end.
        if self.expanding.contains(&def.id) {
            self.account
                .leave("block instance", "a block placed inside itself", false);
            return Ok(());
        }
        let Some(t) = placement(&inst.xform, self.file.units.mm) else {
            self.account
                .leave("block instance", "a projection, not a placement", false);
            return Ok(());
        };
        let rigid = copy.is_none() && t.is_rigid();
        if !rigid && self.placed.copied >= MAX_COPIES {
            self.account.leave(
                "block instance",
                "more copies of blocks than the import makes",
                false,
            );
            return Ok(());
        }
        let parent_color = self
            .file
            .object_color(&self.file.objects[source].attributes, None);
        self.account.take("block instance");
        let objects: Vec<usize> = def
            .objects
            .iter()
            .filter_map(|id| self.by_id.get(id).copied())
            .collect();
        if rigid {
            if let Some(&c) = self.components.get(&def.id) {
                doc.add_occurrence(c, component, t)
                    .map_err(|e| ApiError(e.to_string()))?;
                self.placed.counts.entry(c).or_insert((0, 0)).1 += 1;
                return Ok(());
            }
            let name = if def.name.trim().is_empty() {
                "Block"
            } else {
                def.name.trim()
            };
            let (c, _) = doc
                .add_component(Some(name), component, t)
                .map_err(|e| ApiError(e.to_string()))?;
            self.components.insert(def.id, c);
            self.made.push((name.to_owned(), c));
            self.placed.counts.entry(c).or_insert((0, 0)).1 += 1;
            self.expanding.push(def.id);
            let placed = self.place_block(&objects, c, None, parent_color, doc, depth);
            self.expanding.pop();
            return placed;
        }
        // A copy in place: the block's geometry moved by the instance (and
        // by the copies it is in).
        let at = match copy {
            Some(outer) => outer.after(&t),
            None => t,
        };
        self.placed.copied += 1;
        self.expanding.push(def.id);
        let placed = self.place_block(&objects, component, Some(at), parent_color, doc, depth);
        self.expanding.pop();
        placed
    }

    /// A block's visible objects placed in `component` (moved by `copy`
    /// when they are a copy), its instances one level deeper.
    fn place_block(
        &mut self,
        objects: &[usize],
        component: ComponentUid,
        copy: Option<Transform>,
        parent_color: Option<u32>,
        doc: &mut mitcad_model::Document<OcctKernel>,
        depth: usize,
    ) -> Result<(), ApiError> {
        let file = self.file;
        for &i in objects {
            if !file.objects[i].attributes.visible {
                continue;
            }
            match &file.objects[i].geometry {
                Geometry::Instance(nested) => {
                    self.place_instance(nested, i, component, copy, doc, depth + 1)?;
                }
                _ => self.place_object(i, component, copy.as_ref(), parent_color),
            }
        }
        Ok(())
    }
}

impl Document {
    fn import_3dm_report(&mut self, path: &str, options: &Options) -> Result<Value, ApiError> {
        let file_label = file_name(path);
        let started = std::time::Instant::now();
        let data = std::fs::read(path).map_err(|e| ApiError(format!("{file_label}: {e}")))?;
        // The error a person reads is in words; where it is (offsets,
        // typecodes) goes to the log.
        let file = without_panic(|| File3dm::read(&data))
            .unwrap_or_else(|| Err(mitcad_3dm::Error::new("the file is damaged")))
            .map_err(|e| {
                eprintln!("import: .3dm: {file_label}: {}", e.detail());
                ApiError(format!("{file_label}: {}", e.0))
            })?;
        drop(data);
        let read_seconds = started.elapsed().as_secs_f64();
        catch_occt_crashes();

        let mut walk = Walk {
            file: &file,
            builder: Builder {
                file: &file,
                built: HashMap::new(),
                block_of: file
                    .definitions
                    .iter()
                    .flat_map(|d| {
                        let name = d.name.trim().to_owned();
                        d.objects.iter().map(move |id| (*id, name.clone()))
                    })
                    .filter(|(_, name)| !name.is_empty())
                    .collect(),
            },
            account: Account::default(),
            placed: Placed::default(),
            by_id: file
                .objects
                .iter()
                .enumerate()
                .filter(|(_, o)| !o.attributes.id.is_nil())
                .map(|(i, o)| (o.attributes.id, i))
                .collect(),
            definitions: HashMap::new(),
            components: HashMap::new(),
            made: Vec::new(),
            expanding: Vec::new(),
        };
        for (k, d) in file.definitions.iter().enumerate() {
            walk.definitions.entry(d.id).or_insert(k);
        }
        for w in &file.warnings {
            walk.account.warn(w.0.clone(), w.detail());
        }
        // An object that could not be read, named by what is known of it
        // (its kind, name, block and layer); its index only in the detail.
        for f in &file.failed {
            let kind = f.kind.as_deref().unwrap_or("object");
            walk.account.leave(kind, "it could not be read", false);
            let what = walk.builder.describe(kind, f.attributes.as_ref());
            walk.account.warn(
                format!("{what} cannot be read: {}", f.error.0),
                format!("object {}: {}", f.index, f.error.detail()),
            );
        }

        // The document's unit (an empty document only).
        let empty = self.0.features().next().is_none();
        let depth = self.0.undo_depth();
        let label = format!("Import {file_label}");
        let mut units = None;
        if let Some(sym) = file.units.symbol()
            && let Some((length, 1)) = mitcad_model::expr::Unit::parse(sym)
                .ok()
                .and_then(|u| u.length())
        {
            if empty {
                self.0.set_units(length)?;
                units = Some(sym);
            } else if sym != "mm" {
                let text = format!(
                    "the model's length unit ({sym}) is not applied to a document that has features"
                );
                walk.account.warn(text.clone(), text);
            }
        }

        // Every object once: the top-level ones placed, block geometry
        // through its instances.
        for (i, o) in file.objects.iter().enumerate() {
            let kind = mitcad_3dm::kind_name(&o.geometry);
            *walk.account.objects.entry(kind.clone()).or_insert(0) += 1;
            if o.attributes.mode == 3 {
                continue;
            }
            if !walk.visible(o) {
                walk.account.leave(&kind, "hidden in the file", true);
                continue;
            }
            match &o.geometry {
                Geometry::Instance(inst) => {
                    let inst = inst.clone();
                    walk.place_instance(&inst, i, ComponentUid::ROOT, None, &mut self.0, 0)?;
                }
                _ => walk.place_object(i, ComponentUid::ROOT, None, None),
            }
        }
        // Blocks without an instance: their geometry is not placed.
        let used: HashSet<mitcad_3dm::Uuid> = file
            .objects
            .iter()
            .filter_map(|o| match &o.geometry {
                Geometry::Instance(i) => Some(i.definition),
                _ => None,
            })
            .collect();
        for d in &file.definitions {
            if !used.contains(&d.id) {
                let text = format!("the block {} has no instance and is left out", d.name);
                walk.account.warn(text.clone(), text);
            }
        }
        let build_seconds = started.elapsed().as_secs_f64() - read_seconds;

        // The base features: one per component and layer.
        let mut inputs = Vec::new();
        let mut feature_names: std::collections::BTreeSet<String> =
            self.0.features().map(|f| f.name.clone()).collect();
        let names: HashMap<ComponentUid, String> =
            walk.made.iter().map(|(n, c)| (*c, n.clone())).collect();
        // Per name, the number its next copy tries (layers of one name
        // need not be counted from 2 each time).
        let mut next: HashMap<String, usize> = HashMap::new();
        for key in std::mem::take(&mut walk.placed.order) {
            let Some(bodies) = walk.placed.bodies.remove(&key) else {
                continue;
            };
            let (component, layer) = key;
            let base = match names.get(&component) {
                Some(c) => format!("{c} layer {layer}"),
                None => format!("Layer {layer}"),
            };
            let from = next.get(&base).copied().unwrap_or(2);
            let name = std::iter::once(base.clone())
                .filter(|_| from == 2)
                .chain((from..).map(|n| format!("{base} ({n})")))
                .find(|n| !feature_names.contains(n))
                .expect("an unused name exists");
            let used = name
                .strip_prefix(base.as_str())
                .and_then(|rest| rest.strip_prefix(" ("))
                .and_then(|rest| rest.strip_suffix(')'))
                .and_then(|n| n.parse::<usize>().ok())
                .unwrap_or(1);
            next.insert(base, used + 1);
            feature_names.insert(name.clone());
            inputs.push(BaseInput {
                name: Some(name),
                source: Some(format!("{file_label}, layer {layer}")),
                component: Some(component),
                ..BaseInput::new(bodies)
            });
        }
        if !inputs.is_empty() {
            self.0.add_base_features(&label, inputs)?;
        }
        self.0.merge_undo(depth, &label);

        let a = &walk.account;
        let p = &walk.placed;
        let left_out: Vec<Value> = a
            .left_out
            .iter()
            .map(|((t, why, design), n)| json!({"type": t, "count": n, "why": why, "design": design}))
            .collect();
        let components: Vec<Value> = walk
            .made
            .iter()
            .map(|(name, c)| {
                let (bodies, occurrences) = p.counts.get(c).copied().unwrap_or((0, 0));
                json!({"name": name, "bodies": bodies, "occurrences": occurrences, "placed": occurrences, "component": c.to_string()})
            })
            .collect();
        let missed: usize = a
            .left_out
            .iter()
            .filter(|((_, _, design), _)| !design)
            .map(|(_, n)| n)
            .sum();
        let checks = vec![
            json!({"check": "every body built", "expected": 0, "found": a.not_built.len(), "pass": a.not_built.is_empty()}),
            json!({"check": "every body valid", "expected": p.total, "found": p.valid, "pass": p.valid == p.total}),
            json!({"check": "every solid, surface and mesh taken", "expected": 0, "found": missed, "pass": missed == 0}),
        ];
        let pass = checks.iter().all(|c| c["pass"] == true);
        let mut report = json!({
            "checks": checks,
            "pass": pass,
            "file": file_label,
            "format": "3dm",
            "version": file.version.to_string(),
            "application": file.application,
            "units": units,
            "file_units": file.units.name,
            "unit_mm": file.units.mm,
            "units_assumed": file.units.assumed,
            "tolerance": file.tolerance,
            "layers": file.layers.len(),
            "objects": a.objects,
            "taken": a.taken,
            "left_out": left_out,
            "components": components,
            "copied_instances": p.copied,
            "bodies": p.total,
            "solids": p.solids,
            "meshes": p.meshes,
            "valid": p.valid,
            "solid_volume": p.volume,
            "render_mesh_fallbacks": a.fallbacks,
            "not_built": a.not_built,
            "warnings": a.warnings,
            "warning_details": a.details,
            "seconds": {"read": read_seconds, "build": build_seconds},
        });
        report["notices"] = mitcad_model::notice::to_json(&notices(&report));
        if let Some(path) = &options.report_path {
            let text = serde_json::to_string_pretty(&report).expect("the report serializes");
            std::fs::write(path, format!("{text}\n"))
                .map_err(|e| ApiError(format!("{path}: {e}")))?;
        }
        Ok(report)
    }

    /// The bridge's `import_3dm`: the report as JSON, or text.
    pub(crate) fn import_3dm(&mut self, path: &str, json: &str) -> Result<String, ApiError> {
        let json = if json.trim().is_empty() { "{}" } else { json };
        let options: Options = serde_json::from_str(json)
            .map_err(|e| ApiError(format!("invalid import options: {e}")))?;
        let report = self.without_job(|document| document.import_3dm_report(path, &options))?;
        Ok(if options.text {
            summary(&report)
        } else {
            serde_json::to_string_pretty(&report).expect("the report serializes")
        })
    }

    /// `{"cmd": "import_3dm", "path": ..., ...}`.
    pub(crate) fn import_3dm_command(&mut self, mut command: Value) -> Result<Value, ApiError> {
        let path = command
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(|| ApiError("import_3dm needs a \"path\"".to_owned()))?
            .to_owned();
        if let Some(map) = command.as_object_mut() {
            map.remove("cmd");
            map.remove("path");
        }
        let options: Options = serde_json::from_value(command)
            .map_err(|e| ApiError(format!("invalid import options: {e}")))?;
        let report = self.without_job(|document| document.import_3dm_report(&path, &options))?;
        Ok(json!({
            "file": report["file"],
            "bodies": report["bodies"],
            "components": report["components"].as_array().map_or(0, Vec::len),
            "notices": report["notices"],
            "report": report,
        }))
    }
}

/// The import's notices (mitcad#1051): its objects' outcomes at their
/// levels, the worst first, then its warnings.
fn notices(report: &Value) -> Vec<Notice> {
    let n = |v: &Value| usize::try_from(v.as_u64().unwrap_or(0)).unwrap_or(usize::MAX);
    let mut counts: BTreeMap<EntityOutcome, usize> = BTreeMap::new();
    counts.insert(
        EntityOutcome::Taken,
        report["taken"]
            .as_object()
            .map_or(0, |t| t.values().map(n).sum()),
    );
    // Left out by design (curves, annotations, hidden objects): in the
    // report, not a notice.
    counts.insert(
        EntityOutcome::LeftOut,
        report["left_out"].as_array().map_or(0, |l| {
            l.iter()
                .filter(|x| x["design"] != true)
                .map(|x| n(&x["count"]))
                .sum()
        }),
    );
    counts.insert(
        EntityOutcome::NotValid,
        n(&report["bodies"]).saturating_sub(n(&report["valid"])),
    );
    counts.insert(
        EntityOutcome::NotBuilt,
        report["not_built"].as_array().map_or(0, Vec::len),
    );
    let warnings: Vec<String> = report["warnings"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect();
    EntityOutcome::notices(counts, &warnings)
}

/// The import report as text.
fn summary(report: &Value) -> String {
    let n = |v: &Value| v.as_u64().unwrap_or(0);
    let mut text = format!(
        "Imported {} (.3dm version {}, {}): {} bodies ({} solids, {} meshes; {} valid), {} components\n",
        report["file"].as_str().unwrap_or_default(),
        report["version"].as_str().unwrap_or_default(),
        report["application"].as_str().unwrap_or_default(),
        n(&report["bodies"]),
        n(&report["solids"]),
        n(&report["meshes"]),
        n(&report["valid"]),
        report["components"].as_array().map_or(0, Vec::len),
    );
    text.push_str(&format!(
        "Units: {} ({} mm per unit{})\n",
        report["file_units"].as_str().unwrap_or_default(),
        report["unit_mm"],
        if report["units_assumed"] == true {
            ", assumed"
        } else {
            ""
        }
    ));
    for c in report["components"].as_array().into_iter().flatten() {
        text.push_str(&format!(
            "Component {}: {} bodies, {} occurrences\n",
            c["name"].as_str().unwrap_or_default(),
            n(&c["bodies"]),
            n(&c["occurrences"]),
        ));
    }
    let taken: Vec<String> = report["taken"]
        .as_object()
        .into_iter()
        .flatten()
        .map(|(k, v)| format!("{k} {v}"))
        .collect();
    text.push_str(&format!("Objects taken: {}\n", taken.join(", ")));
    for l in report["left_out"].as_array().into_iter().flatten() {
        text.push_str(&format!(
            "Left out: {} {} ({})\n",
            n(&l["count"]),
            l["type"].as_str().unwrap_or_default(),
            l["why"].as_str().unwrap_or_default()
        ));
    }
    text.push_str(&format!(
        "Volume of the solids: {:.1} mm3\n",
        report["solid_volume"].as_f64().unwrap_or(0.0)
    ));
    for b in report["not_built"].as_array().into_iter().flatten() {
        text.push_str(&format!(
            "Not built: {}: {}\n",
            b["source"].as_str().unwrap_or_default(),
            b["error"].as_str().unwrap_or_default()
        ));
    }
    for w in report["warnings"].as_array().into_iter().flatten() {
        text.push_str(&format!("Warning: {}\n", w.as_str().unwrap_or_default()));
    }
    let checks = report["checks"].as_array().map_or(&[][..], Vec::as_slice);
    text.push_str(&format!(
        "Checks: {} of {} pass\n",
        checks.iter().filter(|c| c["pass"] == true).count(),
        checks.len()
    ));
    text
}

/// The text form of an `import_3dm` report given as JSON (`mitcad-cli
/// import-3dm`).
pub(crate) fn import_3dm_report_text(json: &str) -> String {
    serde_json::from_str::<Value>(json)
        .map_or_else(|e| format!("not a report: {e}\n"), |r| summary(&r))
}

/// Whether an `import_3dm` report given as JSON passes all its checks.
pub(crate) fn import_3dm_report_passes(json: &str) -> bool {
    serde_json::from_str::<Value>(json).is_ok_and(|r| r["pass"] == true)
}

/// Writes a model of the reader's test writer for the import's tests: the
/// sample model (`mitcad_3dm::testfile::sample`, `test_case` empty) or a
/// case of `mitcad_3dm::testfile::test_case` (`block-in-itself`,
/// `plane-surfaces`).
pub(crate) fn import_3dm_write_test_file(path: &str, test_case: &str) -> Result<(), ApiError> {
    let bytes = mitcad_3dm::testfile::test_case(test_case)
        .ok_or_else(|| ApiError(format!("unknown test case {test_case}")))?;
    std::fs::write(path, bytes).map_err(|e| ApiError(format!("{path}: {e}")))
}
