// SPDX-License-Identifier: MIT
//! The `.rvt` import (mitcad#274, mitcad#275, mitcad#333): a project's
//! elements, one component per level, each level's elevation a
//! construction plane of its component and each grid a construction axis
//! (an arc grid a sketched arc).
//!
//! - **Walls, floors, roofs, ceilings, columns and beams** are rebuilt
//!   from their parameters as features ([`plan`], [`footprint`]): a sketch
//!   and an extrusion each, with the cuts of the inserts they host and of
//!   the elements whose geometry cuts them, in one command per element.
//!   Each element's result is checked: against the IFC export of the same
//!   project when one is given ([`Reference`], by the element's GlobalId:
//!   its volume within 1 % and its box within 5 mm), else against its
//!   stored box. An element whose features fail or disagree is undone and
//!   comes in as below, with the reason on its report line. A wall's
//!   openings (mitcad#345) are counted: cut, held by its fallback body (the
//!   geometry the file keeps, the reference's body), or missing and why.
//! - **Every other element** comes in as a body named `Family:Type:Id`
//!   (or by its class and id) of its level's base feature, from the first
//!   source that gives one (mitcad#333): a mesh of the geometry the file
//!   keeps of it ([`geometry`]: its own B-reps and the types it places),
//!   checked like the features; the reference's bodies of its GlobalId;
//!   the triangle mesh its record saves; else its bounding box (a family
//!   instance's type box at the instance's transform, any other element's
//!   box in model coordinates), hidden unless the element is structural
//!   (of a kind [`plan::kind_of`] names).
//! - **Groups**: the members a placed group places where they are come in
//!   as any element.
//! - **Multistory stairs** (mitcad#391): the runs, landings, supports,
//!   rails' supports and balusters of a stair repeated on further storeys
//!   come in again on each, moved up (`… (on L4)`); a rail builds its
//!   copies with itself, and a stair's own mesh leaves out its runs,
//!   landings and supports (elements of their own).
//! - **Materials** become appearances (colour, transparency) of the bodies
//!   built as features: a layered type's exterior layer's material, a
//!   family type's first.
//! - **Links**: each linked model found ([`RvtOptions::links`]) comes in
//!   the same way into a component of its own, placed by its link
//!   instance's transform; a link not found leaves its component empty and
//!   the report names it.
//!
//! Types, view-specific elements (annotations), members of groups'
//! definitions, elements without a box and elements of categories that are
//! no building elements (levels, grids, rooms, sketch lines, views) stay
//! out; the report counts them by reason. The reader is `mitcad_rvt`; the
//! geometry goes through the model's [`Kernel`].

pub mod footprint;
pub mod geometry;
pub mod plan;
// Elements' properties (mitcad#496).
pub mod properties;
// The elements' outcomes and the import's notices (mitcad#1051).
pub mod outcome;

pub use outcome::Outcome;

use std::collections::{BTreeMap, HashMap, HashSet};

use serde::Serialize;
use serde_json::json;

use mitcad_model::properties::{Property, PropertyOwner};
use mitcad_model::{
    BaseInput, ComponentUid, Document, FeatureUid, ImportBody, Kernel, ModelError, PrimitiveShape,
    PrimitiveSpec, SketchFrame, Transform,
};
use mitcad_rvt::building::{Curve, Link};
use mitcad_rvt::categories;
use mitcad_rvt::elements::{Element, FOOT_MM, Model};

use mitcad_import_base::Progress;
use mitcad_import_ifc::build::{Builder, Node, Op};
use plan::Plan;

/// Extents (feet) below which a box has no volume.
const THIN: f64 = 1e-6;

/// How far (feet) a family type's box at the instance's transform may lie
/// from the element's own box and still be used.
const BOX_TOLERANCE: f64 = 0.01;

/// How deep links of linked models are followed (the project's links only).
pub const LINK_DEPTH: usize = 1;

/// A level's component in the report.
#[derive(Debug, Clone, Default, Serialize)]
pub struct LevelReport {
    /// The level element; None for the component of elements without one.
    pub id: Option<i64>,
    /// The other level elements of the same name and elevation, whose
    /// elements it holds too (mitcad#310).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub merged: Vec<i64>,
    /// The component's name: the level's, numbered when another component
    /// has it.
    pub name: String,
    /// Millimetres.
    pub elevation: Option<f64>,
    pub component: Option<String>,
    /// Its base feature of the elements not built as features.
    pub feature: Option<String>,
    /// The link (an index into the report's `links`) whose model it is of.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link: Option<usize>,
    /// Elements built as features.
    pub features: usize,
    /// Bodies of its base feature: saved meshes and boxes.
    pub bodies: usize,
    pub meshes: usize,
    pub boxes: usize,
    /// The elements it holds (as features or bodies).
    #[serde(skip)]
    pub elements: Vec<i64>,
}

/// One element's line of the report.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ElementLine {
    pub id: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link: Option<usize>,
    pub global_id: Option<String>,
    /// `wall`, `floor`, `roof`, `ceiling`, `stair`, `column`, `beam`, or
    /// `other`.
    pub kind: String,
    pub category: String,
    pub name: String,
    pub level: Option<String>,
    /// `features`, `parts`, `mesh` (the geometry the file keeps, or a saved
    /// mesh), `reference body`, `box` or `not built` ([`Outcome`]'s words).
    pub outcome: String,
    pub features: usize,
    pub bodies: usize,
    /// The built solid's volume (mm³) and what it was checked against
    /// (`reference` or `stored box`).
    pub volume: Option<f64>,
    pub checked_against: Option<String>,
    pub reference_volume: Option<f64>,
    pub volume_difference: Option<f64>,
    /// mm.
    pub box_difference: Option<f64>,
    /// The volume (mm³) of the geometry the file keeps, where it was
    /// checked against that (`volume_difference` then its relative
    /// difference).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kept_volume: Option<f64>,
    pub notes: Vec<String>,
    /// Its body is hidden (a box of an element that is not structural).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub hidden: bool,
}

/// A link's line of the report.
#[derive(Debug, Clone, Default, Serialize)]
pub struct LinkReport {
    /// The link instance, its type and the linked file's name.
    pub id: i64,
    pub file: String,
    /// The file read, when found.
    pub found: Option<String>,
    pub component: Option<String>,
    pub elements: usize,
    pub note: Option<String>,
}

/// What the import did.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Report {
    pub levels: Vec<LevelReport>,
    /// Elements built as features, by kind.
    pub features: usize,
    pub built: BTreeMap<String, usize>,
    /// Elements of those kinds that came in as meshes or boxes instead.
    pub fallbacks: BTreeMap<String, usize>,
    /// The reference the features were checked against (its file name).
    pub reference: Option<String>,
    /// Elements built as features that agree with the reference, by its
    /// kind of solid (`IFCWALL`, ...).
    pub reference_agree: BTreeMap<String, usize>,
    /// The reference's walls, slabs, roofs, columns, beams and members
    /// that no element built as features agrees with: kind, name,
    /// GlobalId and why (at most 100).
    pub reference_unmatched: Vec<String>,
    /// Bodies made of the other elements: meshes of the geometry the
    /// file keeps, the reference's bodies, saved meshes, boxes at a
    /// transform, boxes in model coordinates (and of these the boxes
    /// hidden).
    pub bodies: usize,
    /// The properties the elements brought: identity, instance and type
    /// parameters (mitcad#496).
    pub properties: usize,
    pub geometry_meshes: usize,
    /// Their triangles.
    pub geometry_triangles: usize,
    pub reference_bodies: usize,
    pub meshes: usize,
    pub hidden_boxes: usize,
    /// Curtain walls whose panels and mullions are their shape (no body of
    /// their own).
    pub curtain_walls: usize,
    /// Railings whose rails and balusters are their shape.
    pub railings: usize,
    /// Stairs whose runs, landings and supports are their shape
    /// (mitcad#391).
    pub stairs: usize,
    /// Meshes of group members placed away from their definitions.
    pub group_copies: usize,
    /// Copies of stairs' runs, landings, supports and balusters a
    /// multistory stair places on further storeys (mitcad#391; a rail's
    /// copies are its own).
    pub multistory_copies: usize,
    pub oriented_boxes: usize,
    pub boxes: usize,
    /// Construction planes of the levels, axes and sketches of the grids,
    /// appearances of the materials.
    pub level_planes: usize,
    pub grid_axes: usize,
    pub grid_arcs: usize,
    pub appearances: usize,
    pub links: Vec<LinkReport>,
    /// Imported elements by category.
    pub categories: BTreeMap<String, usize>,
    /// Elements left out, by reason.
    pub left_out: BTreeMap<String, usize>,
    /// Elements whose records name no level, placed on the highest level
    /// at or below their box (or the lowest one).
    pub by_elevation: usize,
    /// Elements whose body could not be built: (id, why), at most 50.
    pub not_built: Vec<(i64, String)>,
    /// Elements left out because the process ran low on memory.
    pub memory_low: usize,
    pub warnings: Vec<String>,
    /// The walls' openings (mitcad#345): the inserts and opening elements
    /// cut from their walls built as features, or whose walls' bodies hold
    /// them (the geometry the file keeps, the reference's body); those
    /// without their opening, by reason, and each of them (`insert 47 in
    /// wall 31: why`, at most 200).
    pub openings_cut: usize,
    pub openings_missing: BTreeMap<String, usize>,
    pub openings_missing_lines: Vec<String>,
    /// A line for every element imported.
    pub elements: Vec<ElementLine>,
}

/// Where the placed groups place their members (mitcad#333): each placed
/// group's (`ElementGroup`) geometry places its members' by their element
/// ids (as a `GInstance` each), at their place (identity) or moved.
pub type GroupPlacements = HashMap<i64, Vec<(i64, [f64; 12])>>;

/// The identity transform.
const IDENTITY: [f64; 12] = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0];

fn is_identity(t: &[f64; 12]) -> bool {
    t.iter().zip(IDENTITY).all(|(a, b)| (a - b).abs() < 1e-9)
}

pub fn group_placements(model: &Model) -> GroupPlacements {
    let mut out: GroupPlacements = HashMap::new();
    for e in &model.elements {
        if e.class.as_deref() != Some("ElementGroup") || e.owner_view.is_some() {
            continue;
        }
        let Some(g) = model.geometry.get(&e.id) else {
            continue;
        };
        for p in &g.instances {
            if let Some(member) = p.symbol {
                out.entry(member).or_default().push((e.id, p.transform));
            }
        }
    }
    out
}

/// Why an element is not imported, or None. A member of a group's
/// definition comes in when a placed group places it where it is.
fn left_out(model: &Model, e: &Element, groups: &GroupPlacements) -> Option<String> {
    if e.partition.is_none() {
        return Some("no records".into());
    }
    if e.is_type {
        return Some("types".into());
    }
    if e.owner_view.is_some() {
        return Some("view-specific (annotations)".into());
    }
    if e.class.as_deref() == Some("ElementGroup") {
        return Some("groups (their members come in)".into());
    }
    if e.unplaced_owner.is_some()
        && !groups
            .get(&e.id)
            .is_some_and(|ts| ts.iter().any(|(_, t)| is_identity(t)))
    {
        return Some("in a group's definition (not placed where it is)".into());
    }
    // (mitcad#391) A family instance naming a rail as its owner that the
    // rail does not list among its supports: no support of it (an earlier
    // layout's, which nothing places).
    if let Some(rail) = model
        .building
        .owners
        .get(&e.id)
        .and_then(|o| model.building.sweeps.get(o))
        .filter(|r| r.path3d.is_some())
        && !rail.supports.contains(&e.id)
    {
        return Some("a rail's support the rail does not list (nothing places it)".into());
    }
    let has_mesh = model.meshes.contains_key(&e.id);
    let solid_box = e
        .bbox
        .is_some_and(|b| (0..3).all(|k| (b[k + 3] - b[k]).is_finite() && b[k + 3] - b[k] > THIN));
    if !has_mesh && !solid_box {
        return Some("no box".into());
    }
    match e.category {
        Some(c) if categories::is_model(c) => None,
        Some(c) => Some(format!(
            "not a building element ({})",
            categories::name(c).map_or(c.to_string(), str::to_string)
        )),
        None => Some("no category".into()),
    }
}

/// A box (feet, min and max corner) placed by a transform (rows of the
/// matrix, then the origin): the frame of its first corner and its sizes
/// (mm).
fn placed_box(local: [f64; 6], t: &[f64; 12]) -> (SketchFrame, [f64; 3]) {
    let apply = |p: [f64; 3]| -> [f64; 3] {
        std::array::from_fn(|r| {
            t[3 * r] * p[0] + t[3 * r + 1] * p[1] + t[3 * r + 2] * p[2] + t[9 + r]
        })
    };
    let column = |c: usize| [t[c], t[3 + c], t[6 + c]];
    let (x, mut y, z) = (column(0), column(1), column(2));
    let cross = [
        x[1] * y[2] - x[2] * y[1],
        x[2] * y[0] - x[0] * y[2],
        x[0] * y[1] - x[1] * y[0],
    ];
    let mirrored = cross[0] * z[0] + cross[1] * z[1] + cross[2] * z[2] < 0.0;
    // A mirrored transform: the box spans the other way along y.
    let corner = if mirrored {
        y = y.map(|v| -v);
        [local[0], local[4], local[2]]
    } else {
        [local[0], local[1], local[2]]
    };
    let origin = apply(corner).map(|v| v * FOOT_MM);
    (
        SketchFrame {
            origin,
            x_axis: x,
            y_axis: y,
        },
        [
            (local[3] - local[0]) * FOOT_MM,
            (local[4] - local[1]) * FOOT_MM,
            (local[5] - local[2]) * FOOT_MM,
        ],
    )
}

/// The axis-aligned box of a box at a transform (feet).
fn placed_bounds(local: [f64; 6], t: &[f64; 12]) -> [f64; 6] {
    let mut out = [
        f64::INFINITY,
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    for i in 0..8 {
        let p = [
            if i & 1 == 0 { local[0] } else { local[3] },
            if i & 2 == 0 { local[1] } else { local[4] },
            if i & 4 == 0 { local[2] } else { local[5] },
        ];
        for r in 0..3 {
            let v = t[3 * r] * p[0] + t[3 * r + 1] * p[1] + t[3 * r + 2] * p[2] + t[9 + r];
            out[r] = out[r].min(v);
            out[r + 3] = out[r + 3].max(v);
        }
    }
    out
}

/// How an element's body is made.
enum Shape {
    Mesh,
    /// A type's box at the instance's transform.
    Oriented(SketchFrame, [f64; 3]),
    /// The element's box in model coordinates.
    Aligned(SketchFrame, [f64; 3]),
}

fn shape_of(model: &Model, e: &Element) -> Option<Shape> {
    if model.meshes.contains_key(&e.id) {
        return Some(Shape::Mesh);
    }
    let bbox = e.bbox?;
    // The instance's own symbol first (its geometry as placed), then its
    // type; used when it agrees with the element's box.
    if let Some(t) = &e.transform {
        for symbol in [e.symbol, e.type_id].into_iter().flatten() {
            let Some(local) = model.outlines.get(&symbol).copied() else {
                continue;
            };
            if (0..3).any(|k| local[k + 3] - local[k] <= THIN) {
                continue;
            }
            let placed = placed_bounds(local, t);
            if (0..6).all(|k| (placed[k] - bbox[k]).abs() <= BOX_TOLERANCE) {
                let (frame, size) = placed_box(local, t);
                return Some(Shape::Oriented(frame, size));
            }
        }
    }
    Some(Shape::Aligned(
        SketchFrame {
            origin: [bbox[0] * FOOT_MM, bbox[1] * FOOT_MM, bbox[2] * FOOT_MM],
            x_axis: [1.0, 0.0, 0.0],
            y_axis: [0.0, 1.0, 0.0],
        },
        [
            (bbox[3] - bbox[0]) * FOOT_MM,
            (bbox[4] - bbox[1]) * FOOT_MM,
            (bbox[5] - bbox[2]) * FOOT_MM,
        ],
    ))
}

/// A box (mm) on a frame through the kernel.
fn make_box<K: Kernel>(
    document: &Document<K>,
    frame: SketchFrame,
    size: [f64; 3],
) -> Result<K::Shape, mitcad_model::KernelError> {
    document.kernel().primitive(&PrimitiveSpec {
        feature: FeatureUid(0),
        frame,
        shape: PrimitiveShape::Box {
            length: size[0],
            width: size[1],
            height: size[2],
        },
    })
}

/// The level an element is placed on: its own, else the highest level at
/// or below its box, else the lowest; whether it was by elevation.
fn level_of(model: &Model, e: &Element) -> Option<(i64, bool)> {
    if let Some(l) = e.level.filter(|l| model.level(*l).is_some()) {
        return Some((l, false));
    }
    let base = e.bbox.map(|b| b[2]).or_else(|| {
        let mesh = model.meshes.get(&e.id)?;
        mesh.iter()
            .filter_map(|m| m.bounds())
            .map(|b| b[2])
            .reduce(f64::min)
    })?;
    let below = model
        .levels
        .iter()
        .filter(|l| l.elevation <= base + 1e-6)
        .max_by(|a, b| a.elevation.total_cmp(&b.elevation));
    below.or(model.levels.first()).map(|l| (l.id, true))
}

/// An element's solid in the reference (the IFC export of the same
/// project): its volume (mm³) and its box (mm), by GlobalId.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ReferenceSolid {
    pub kind: String,
    pub name: String,
    pub volume: Option<f64>,
    pub bbox: Option<([f64; 3], [f64; 3])>,
    /// Its bodies' B-rep data ([`Kernel::brep_data`], mm, model
    /// coordinates): an element's body where the project file keeps none
    /// (mitcad#333).
    #[serde(skip)]
    pub bodies: Vec<Vec<u8>>,
}

/// The reference the elements built as features are checked against.
#[derive(Debug, Clone, Default)]
pub struct Reference {
    /// The reference file's name.
    pub file: String,
    pub solids: HashMap<String, ReferenceSolid>,
}

/// A linked model found and read, for its link instance.
pub struct LinkedModel {
    pub link: Link,
    /// The file it was read from (its name).
    pub file: String,
    pub model: Model,
}

/// An IFC model found for a link instance (a project can link IFC files
/// as well as projects, mitcad#333).
pub struct IfcLink {
    pub link: Link,
    /// The file's name.
    pub file: String,
    pub ifc: mitcad_ifc::IfcFile,
}

/// The options of [`import_rvt`].
#[derive(Default)]
pub struct RvtOptions<'a> {
    /// The memory guard (mitcad#80).
    pub progress: Option<&'a Progress>,
    /// The IFC export to check the features against (else each element's
    /// stored box).
    pub reference: Option<&'a Reference>,
    /// The linked models found, by their link instances.
    pub links: Vec<LinkedModel>,
    /// The links not found: the link and why.
    pub missing_links: Vec<(Link, String)>,
    /// The IFC models linked into the project that were found (imported
    /// into their links' components by the IFC import).
    pub ifc_links: Vec<IfcLink>,
    /// Every element as its mesh or box (phase 1), no features.
    pub mesh_only: bool,
    /// Only these elements of the project (mitcad#385): the others are
    /// left out (`not among the elements asked for`).
    pub only: Option<HashSet<i64>>,
    /// The IFC export's openings (mitcad#345, [`reference_openings`]): an
    /// insert whose type gives no cut tool is cut by the opening of its
    /// GlobalId.
    pub openings: HashMap<String, Vec<mitcad_ifc::geometry::Item>>,
}

/// The openings of an IFC export (mitcad#345): each `IfcOpeningElement`'s
/// body items in the project's coordinates (mm), by its own GlobalId and
/// by those of the elements that fill it (`IfcRelFillsElement`), for
/// [`RvtOptions::openings`].
pub fn reference_openings(
    file: &mitcad_ifc::IfcFile,
) -> HashMap<String, Vec<mitcad_ifc::geometry::Item>> {
    use mitcad_ifc::geometry::{Decoder, Item};
    use mitcad_ifc::math;
    let model = mitcad_ifc::model::Model::read(file);
    let decoder = Decoder::new(file, &model.units);
    let place =
        |item: &Item, m: &math::Matrix| mitcad_import_ifc::transform::transformed(item, m).ok();
    let mut out = HashMap::new();
    for p in model.products.values() {
        if &*p.kind != "IFCOPENINGELEMENT" {
            continue;
        }
        let Some(rep) = model.body(file, p) else {
            continue;
        };
        let mut items = Vec::new();
        for i in rep.items {
            match decoder.item(i) {
                Item::Mapped { map, transform } => {
                    let m = math::mul(&p.placement, &transform);
                    if let Some(r) = model.map_representation(file, map) {
                        items.extend(
                            r.items
                                .into_iter()
                                .filter_map(|j| place(&decoder.item(j), &m)),
                        );
                    }
                }
                other => items.extend(place(&other, &p.placement)),
            }
        }
        if items.is_empty() {
            continue;
        }
        for f in &p.fillings {
            if let Some(filling) = model.products.get(f) {
                out.insert(filling.global_id.clone(), items.clone());
            }
        }
        out.insert(p.global_id.clone(), items);
    }
    out
}

/// What an element built as features was checked against without a
/// reference: the geometry the file keeps of it.
pub const KEPT: &str = "the geometry the file keeps";
/// ... or its box, both ways (a rail's, mitcad#391: its thin profile
/// meshes within a chord that takes up to a tenth of its volume).
pub const KEPT_BOX: &str = "the box of the geometry the file keeps";
/// Fraction of the volume of the geometry the file keeps the built volume
/// may differ by (a little more than the reference's: the file's walls
/// are cleaned up at their joins in ways the features follow only
/// nearly).
pub const KEPT_VOLUME_TOLERANCE: f64 = 0.02;

/// Fraction of the reference volume the built volume may differ by.
pub const VOLUME_TOLERANCE: f64 = 0.01;
/// How far (mm) the built box's sides may lie from the reference's.
pub const BOX_TOLERANCE_MM: f64 = 5.0;
/// How far a mesh of the geometry the file keeps may reach out of the
/// element's stored box: this fraction of the box's largest extent (at
/// least [`BOX_TOLERANCE_MM`]).
pub const MESH_BOX_FRACTION: f64 = 0.1;

type Bbox = ([f64; 3], [f64; 3]);

/// An element waiting for its batch: its plan's items prepared by the
/// kernel, its body's name and its report line.
struct Pending<'m> {
    e: &'m Element,
    kind: &'static str,
    line: ElementLine,
    slot: usize,
    component: ComponentUid,
    label: String,
    appearance: Option<String>,
    body_name: String,
    solid: Node,
    more: Vec<Node>,
    cuts: Vec<(String, Node)>,
    /// A wall's attachments (mitcad#348): its heights are checked both
    /// ways.
    attached: Vec<String>,
    /// A wall's openings (mitcad#345).
    openings: Vec<plan::WallOpening>,
    /// The volume (mm³) and box of the geometry the file keeps of it,
    /// where that meshes closed: what it is checked against without a
    /// reference.
    kept: Option<(f64, Bbox)>,
    /// A rail's: the box of that geometry where it does not mesh closed
    /// (mitcad#391).
    kept_box: Option<Bbox>,
    /// How far up (feet) a copy of the element lies (mitcad#391: a
    /// multistory stair's), 0 for the element itself.
    shift: f64,
}

/// The copies of an element of a stair that a multistory stair repeats
/// (mitcad#391), each its level and how far up (feet), that come in as
/// elements of their own: a run's, landing's, stair support's, rail
/// support's or baluster's, which the stair's, railing's or rail's
/// geometry places again on each storey (its own geometry holds it once).
/// None for the stair itself, and for a railing or a rail, whose own
/// geometry places its symbol on every storey (a rail builds its copies
/// with itself, [`plan`]).
fn multistory_copies(model: &Model, e: &Element) -> Option<(i64, Vec<(i64, f64)>)> {
    if matches!(
        e.class.as_deref(),
        Some("StairsElement" | "BaseRailing" | "MultistoryStairs")
    ) || model
        .building
        .sweeps
        .get(&e.id)
        .is_some_and(|s| s.path3d.is_some())
    {
        return None;
    }
    model
        .multistory_copies(e.id)
        .filter(|(stair, copies)| *stair != e.id && !copies.is_empty())
}

/// Where an element comes in (mitcad#391): its level's slot, how far up
/// (feet) and, for a copy up its multistory stair, the stair and the
/// copy's level.
type Place = (usize, f64, Option<(i64, i64)>);

/// A box (feet) moved up by `dz` (feet).
fn moved_box(b: Option<[f64; 6]>, dz: f64) -> Option<[f64; 6]> {
    b.map(|b| [b[0], b[1], b[2] + dz, b[3], b[4], b[5] + dz])
}

/// An element's features and notes in a batch, or why they did not build;
/// and its cuts: what (`insert 47`), their features, and whether they were
/// made.
type Built = Result<(Vec<FeatureUid>, Vec<String>, Vec<CutMade>), String>;

/// A cut of an element in a batch: what, its features, whether it was
/// made (mitcad#345).
type CutMade = (String, Vec<FeatureUid>, bool);

/// Elements built in one command.
const BATCH: usize = 64;

/// An element of a batch: what it was checked against, its features, its
/// cuts that remove nothing and its cuts (mitcad#345), or why it is not
/// built.
type Verdict = Result<(&'static str, Vec<FeatureUid>, Vec<FeatureUid>, Vec<CutMade>), String>;

/// A level's component, its report and the bodies of its base feature.
struct LevelGroup<S> {
    entry: LevelReport,
    component: ComponentUid,
    bodies: Vec<ImportBody<S>>,
    /// Its bodies (indices) hidden: boxes of elements that are not
    /// structural.
    hidden: Vec<usize>,
    /// The element of each of its bodies, in their order (mitcad#496:
    /// their properties).
    body_elements: Vec<i64>,
}

/// The volume and box (mm, the component's coordinates) of the bodies of
/// features, and how many bodies.
fn measure<K: Kernel>(
    document: &Document<K>,
    features: &[FeatureUid],
) -> (Option<f64>, Option<Bbox>, usize) {
    let kernel = document.kernel();
    let mut volume = 0.0;
    let mut measured = false;
    let mut bbox: Option<Bbox> = None;
    let mut count = 0;
    // The features' bodies by their ids (not the document's every body:
    // an import of many elements would measure in quadratic time).
    let shapes = features.iter().flat_map(|f| {
        let mut misses = 0;
        (0u32..)
            .map_while(move |i| {
                let shape = document.body_shape(mitcad_model::BodyUid::new(*f, i));
                misses = if shape.is_some() { 0 } else { misses + 1 };
                (misses < 8).then_some(shape)
            })
            .flatten()
    });
    for shape in shapes {
        count += 1;
        if let Ok(m) = kernel.mass_properties(shape) {
            volume += m.volume;
            measured = true;
        }
        if let Ok(Some(b)) = kernel.bounding_box(shape) {
            bbox = Some(match bbox {
                None => (b.min, b.max),
                Some((lo, hi)) => (
                    std::array::from_fn(|k| lo[k].min(b.min[k])),
                    std::array::from_fn(|k| hi[k].max(b.max[k])),
                ),
            });
        }
    }
    (measured.then_some(volume), bbox, count)
}

/// The largest distance (mm) between the sides of two boxes.
pub fn box_difference(a: &Bbox, b: &Bbox) -> f64 {
    (0..3)
        .map(|k| (a.0[k] - b.0[k]).abs().max((a.1[k] - b.1[k]).abs()))
        .fold(0.0, f64::max)
}

/// Whether the built solid agrees with the reference: what it was checked
/// against, or why it disagrees.
fn check(
    line: &mut ElementLine,
    stored: Option<[f64; 6]>,
    reference: Option<&Reference>,
    volume: Option<f64>,
    bbox: Option<Bbox>,
    kept: Option<(f64, Bbox)>,
    kept_box: Option<Bbox>,
) -> Result<&'static str, String> {
    let bbox = bbox.ok_or("its features made no body")?;
    let volume = volume
        .filter(|v| *v > 0.0)
        .ok_or("its body encloses no volume")?;
    let solid = reference.and_then(|r| line.global_id.as_ref().and_then(|g| r.solids.get(g)));
    if let Some(solid) = solid {
        if let Some(rv) = solid.volume.filter(|v| *v > 0.0) {
            let d = (volume - rv).abs() / rv;
            line.reference_volume = Some(rv);
            line.volume_difference = Some(d);
            if d > VOLUME_TOLERANCE {
                return Err(format!(
                    "its volume {:.4} m³ differs from the reference's {:.4} m³ by {:.2} %",
                    volume * 1e-9,
                    rv * 1e-9,
                    d * 100.0
                ));
            }
        }
        if let Some(rb) = &solid.bbox {
            let d = box_difference(&bbox, rb);
            line.box_difference = Some(d);
            if d > BOX_TOLERANCE_MM {
                return Err(format!("its box lies {d:.1} mm from the reference's"));
            }
        }
        return Ok("reference");
    }
    // No reference solid: the box of the geometry the file keeps must
    // hold it (a wall's stored box also holds its layers' faces, which
    // reach its unconnected height).
    if let Some((kv, kb)) = kept {
        line.kept_volume = Some(kv);
        let d = (volume - kv).abs() / kv;
        line.volume_difference = Some(d);
        if d > KEPT_VOLUME_TOLERANCE {
            return Err(format!(
                "its volume {:.4} m³ differs from the geometry the file keeps ({:.4} m³) by {:.2} %",
                volume * 1e-9,
                kv * 1e-9,
                d * 100.0
            ));
        }
        // (A stair run's box both ways, mitcad#397: one too short would
        // pass inside the box of the geometry the file keeps.)
        if let Some(kb2) = kept_box {
            let d = box_difference(&bbox, &kb2);
            line.box_difference = Some(d);
            if d > BOX_TOLERANCE_MM {
                return Err(format!(
                    "its box lies {d:.1} mm from the box of the geometry the file keeps"
                ));
            }
            return Ok(KEPT);
        }
        let out = (0..3)
            .map(|k| (kb.0[k] - bbox.0[k]).max(bbox.1[k] - kb.1[k]))
            .fold(f64::NEG_INFINITY, f64::max);
        line.box_difference = Some(out.max(0.0));
        if out > BOX_TOLERANCE_MM {
            return Err(format!(
                "its body reaches {out:.1} mm outside the geometry the file keeps"
            ));
        }
        return Ok(KEPT);
    }
    // A rail's geometry as the file keeps it (its thin profile meshed
    // within a chord that takes up to a tenth of its volume), within its box
    // both ways (mitcad#391: a rail missing its copy up a multistory stair
    // lies inside the stored box, which holds the copy too).
    if let Some(kb) = kept_box {
        let d = box_difference(&bbox, &kb);
        line.box_difference = Some(d);
        if d > BOX_TOLERANCE_MM {
            return Err(format!(
                "its box lies {d:.1} mm from the box of the geometry the file keeps"
            ));
        }
        return Ok(KEPT_BOX);
    }
    // Else the element's stored box must hold it.
    let stored = stored.ok_or("no reference solid and no stored box to check against")?;
    let s: Bbox = (
        [stored[0], stored[1], stored[2]].map(|v| v * FOOT_MM),
        [stored[3], stored[4], stored[5]].map(|v| v * FOOT_MM),
    );
    let out = (0..3)
        .map(|k| (s.0[k] - bbox.0[k]).max(bbox.1[k] - s.1[k]))
        .fold(f64::NEG_INFINITY, f64::max);
    line.box_difference = Some(out.max(0.0));
    if out > BOX_TOLERANCE_MM {
        return Err(format!(
            "its body reaches {out:.1} mm outside the element's stored box"
        ));
    }
    Ok("stored box")
}

/// The openings of a wall whose plan failed (mitcad#345): what would cut
/// it, for its fallback body to hold or to miss (none for a curtain wall,
/// whose parts are its shape).
fn expected_openings(
    model: &Model,
    e: &Element,
    curtain_hosts: &HashSet<i64>,
) -> Vec<plan::WallOpening> {
    let Some(w) = model.building.walls.get(&e.id) else {
        return Vec::new();
    };
    if curtain_hosts.contains(&e.id) {
        return Vec::new();
    }
    plan::wall_openings(model, w)
        .0
        .into_iter()
        .map(|c| plan::WallOpening {
            id: c.id,
            label: c.label,
            missing: c.tools.err(),
        })
        .collect()
}

/// An attached wall's heights against its stored box, both ways
/// (mitcad#348): checked against the stored box alone, a wall too low
/// (its top not reaching its roof) would pass.
fn check_heights(
    stored_box: Option<[f64; 6]>,
    bbox: Option<Bbox>,
    attached: &[String],
    kept: Option<(f64, Bbox)>,
) -> Result<(), String> {
    let stored = kept
        .map(|(_, b)| (b.0[2], b.1[2]))
        .or_else(|| stored_box.map(|s| (s[2] * FOOT_MM, s[5] * FOOT_MM)));
    let (Some(bbox), Some((lo, hi))) = (bbox, stored) else {
        return Ok(());
    };
    let box_of = if kept.is_some() {
        "the geometry the file keeps"
    } else {
        "its stored box"
    };
    let d = (bbox.0[2] - lo).abs().max((bbox.1[2] - hi).abs());
    if d > BOX_TOLERANCE_MM {
        return Err(format!(
            "its heights {:.0} to {:.0} mm lie {d:.1} mm from {box_of}'s ({:.0} to {:.0} mm; {})",
            bbox.0[2],
            bbox.1[2],
            lo,
            hi,
            attached.join(", ")
        ));
    }
    Ok(())
}

/// What the import does, over the project and its linked models.
struct Importer<'r, 'o, K: Kernel> {
    document: &'r mut Document<K>,
    report: Report,
    options: &'r RvtOptions<'o>,
    /// The document's appearances by material id, per model.
    appearances: HashMap<(usize, i64), Option<String>>,
    feature_names: HashSet<String>,
    body_names: HashSet<String>,
    stopped: bool,
    /// The import's undo depth and step label.
    depth: usize,
    label: String,
    /// Import notes of the base features.
    notes: Vec<(FeatureUid, String)>,
    /// The triangulated geometries of the models (types placed many times
    /// triangulated once).
    tess: geometry::Cache,
    /// The model's railing parts: balusters, rails and the rails' types,
    /// left out of their railings' meshes.
    parts: HashSet<i64>,
    /// The model's rails' symbols (`ContinuousRailSymbol`), and its
    /// elements that are not types (mitcad#391: what a stair's geometry
    /// places of them is left out of its mesh).
    rail_symbols: HashSet<i64>,
    own_elements: HashSet<i64>,
    /// The model's inserts without their openings (mitcad#345): insert,
    /// wall, why; noted on the inserts' lines at the end.
    missing: Vec<(i64, i64, String)>,
    /// The elements' properties, added at the end (mitcad#496).
    properties: Vec<(PropertyOwner, String, Property)>,
    /// The model's GlobalIds and its elements' places in its list, for
    /// their properties (the model in hand: the project, then each link).
    global_ids: HashMap<i64, String>,
    element_index: HashMap<i64, usize>,
}

/// The bodies features made, by their ids (as [`measure`] finds them).
fn feature_bodies<K: Kernel>(
    document: &Document<K>,
    features: &[FeatureUid],
) -> Vec<mitcad_model::BodyUid> {
    let mut bodies = Vec::new();
    for f in features {
        let mut misses = 0;
        for i in 0u32.. {
            let uid = mitcad_model::BodyUid::new(*f, i);
            if document.body_shape(uid).is_some() {
                bodies.push(uid);
                misses = 0;
            } else {
                misses += 1;
                if misses >= 8 {
                    break;
                }
            }
        }
    }
    bodies
}

impl<K: Kernel> Importer<'_, '_, K> {
    /// An element's properties on its bodies (mitcad#496), added at the
    /// end of the import.
    fn element_properties(
        &mut self,
        model: &Model,
        e: &Element,
        level: &str,
        bodies: &[mitcad_model::BodyUid],
    ) {
        if bodies.is_empty() {
            return;
        }
        let global_id = self.global_ids.get(&e.id).map(String::as_str);
        let items = properties::element_properties(model, e, global_id, Some(level));
        for body in bodies {
            for (set, property) in &items {
                self.properties
                    .push((PropertyOwner::Body(*body), set.clone(), property.clone()));
            }
        }
    }

    fn memory_low(&mut self) -> bool {
        if !self.stopped && self.options.progress.is_some_and(Progress::is_memory_low) {
            self.stopped = true;
        }
        self.stopped
    }

    /// The appearance of a material (made once).
    fn appearance(&mut self, model_key: usize, model: &Model, material: i64) -> Option<String> {
        if let Some(a) = self.appearances.get(&(model_key, material)) {
            return a.clone();
        }
        let made = model
            .materials
            .iter()
            .find(|m| m.id == material)
            .and_then(|m| {
                let color = m.color?;
                let base = if m.name.trim().is_empty() {
                    format!("Material {}", m.id)
                } else {
                    m.name.clone()
                };
                let mut change = mitcad_model::appearance::AppearanceChange {
                    name: Some(base.clone()),
                    base_color: Some(color.map(|c| f64::from(c) / 255.0)),
                    opacity: Some((1.0 - m.transparency.unwrap_or(0.0) / 100.0).clamp(0.0, 1.0)),
                    ..Default::default()
                };
                // A name taken (a linked model's material, an earlier import)
                // gets a number.
                let mut made = self.document.create_appearance(&change);
                let mut n = 2;
                while made
                    .as_ref()
                    .is_err_and(|e| e.to_string().contains("an appearance is named"))
                    && n < 100
                {
                    change.name = Some(format!("{base} ({n})"));
                    made = self.document.create_appearance(&change);
                    n += 1;
                }
                match made {
                    Ok(id) => {
                        self.report.appearances += 1;
                        Some(id)
                    }
                    Err(e) => {
                        self.report
                            .warnings
                            .push(format!("appearance of material {}: {e}", m.id));
                        None
                    }
                }
            });
        self.appearances.insert((model_key, material), made.clone());
        made
    }

    fn unique_body(&mut self, name: &str) -> String {
        let mut out = name.to_string();
        let mut n = 2;
        while self.body_names.contains(&out) {
            out = format!("{name} ({n})");
            n += 1;
        }
        self.body_names.insert(out.clone());
        out
    }

    /// The volume (mm³) and box of the geometry the file keeps of an
    /// element, where it meshes closed with every face; for a rail, the
    /// box of that mesh where it has every face (mitcad#391).
    fn kept(&mut self, model: &Model, e: &Element) -> (Option<(f64, Bbox)>, Option<Bbox>) {
        let skip = self.skip_of(model, e).unwrap_or_else(|| self.parts.clone());
        let Some(Ok(mesh)) = geometry::element_mesh(model, e.id, &mut self.tess, &skip) else {
            return (None, None);
        };
        if mesh.faces_left_out > 0 {
            return (None, None);
        }
        // (A rail's thin profile meshes within a chord of its curves that
        // takes up to a tenth of its volume: its box is what is compared.)
        let rail = model
            .building
            .sweeps
            .get(&e.id)
            .is_some_and(|s| s.path3d.is_some());
        if rail {
            return (None, mesh.bounds());
        }
        // A stair run's treads and risers meet edge to edge (mitcad#397):
        // its mesh may touch itself along those edges. It is checked
        // against the volume that encloses and its box, both ways.
        if model.building.stair_runs.contains_key(&e.id) {
            let volume = mesh.volume.or_else(|| {
                mitcad_import_ifc::brep::mesh_balanced(&mesh.points, &mesh.triangles).then(|| {
                    mitcad_import_ifc::brep::mesh_measures(&mesh.points, &mesh.triangles)
                        .0
                        .abs()
                })
            });
            if let Some(volume) = volume.filter(|v| *v > 0.0) {
                return (mesh.bounds().map(|b| (volume, b)), mesh.bounds());
            }
        }
        if let (true, Some(volume)) = (mesh.closed, mesh.volume.filter(|v| *v > 0.0)) {
            return (mesh.bounds().map(|b| (volume, b)), None);
        }
        (None, None)
    }

    /// What the geometry the file keeps of an element places that are
    /// elements of their own, to leave out of its mesh (None: nothing
    /// particular): a railing's rails and balusters; a stair's runs,
    /// landings and supports (mitcad#391); a rail's supports, but not its
    /// own symbol (mitcad#391: placed once on every storey of a
    /// multistory stair).
    fn skip_of(&self, model: &Model, e: &Element) -> Option<HashSet<i64>> {
        match e.class.as_deref() {
            Some("BaseRailing") => Some(self.parts.clone()),
            Some("StairsElement") => Some(self.own_elements.clone()),
            _ if model
                .building
                .sweeps
                .get(&e.id)
                .is_some_and(|s| s.path3d.is_some()) =>
            {
                let own: HashSet<i64> = model
                    .geometry
                    .get(&e.id)
                    .map(|g| g.instances.iter().filter_map(|p| p.symbol).collect())
                    .unwrap_or_default();
                Some(
                    self.parts
                        .iter()
                        .copied()
                        .filter(|p| !(own.contains(p) && self.rail_symbols.contains(p)))
                        .collect(),
                )
            }
            _ => None,
        }
    }

    /// An element's plan made ready for a batch: its label, appearance,
    /// body name (taken) and the kernel's prepared items.
    #[allow(clippy::too_many_arguments)]
    fn prepare<'m>(
        &mut self,
        model_key: usize,
        model: &Model,
        e: &'m Element,
        kind: &'static str,
        plan: Plan,
        component: ComponentUid,
        slot: usize,
        line: ElementLine,
        shift: f64,
    ) -> Pending<'m> {
        let label = line.name.clone();
        let appearance = plan
            .material
            .and_then(|m| self.appearance(model_key, model, m));
        let body_name = self.unique_body(&label);
        let up = shift * FOOT_MM;
        let moved =
            |(lo, hi): Bbox| -> Bbox { ([lo[0], lo[1], lo[2] + up], [hi[0], hi[1], hi[2] + up]) };
        let (kept, kept_box) = self.kept(model, e);
        let kept = kept.map(|(v, b)| (v, moved(b)));
        let kept_box = kept_box.map(moved);
        let kernel = self.document.kernel();
        Pending {
            e,
            kept,
            kept_box,
            shift,
            kind,
            line,
            slot,
            component,
            label,
            appearance,
            body_name,
            attached: plan.attached,
            openings: plan.openings,
            solid: mitcad_import_ifc::build::prepare(kernel, plan.solid),
            more: plan
                .more
                .into_iter()
                .map(|t| mitcad_import_ifc::build::prepare(kernel, t))
                .collect(),
            cuts: plan
                .cuts
                .into_iter()
                .map(|(what, t)| (what, mitcad_import_ifc::build::prepare(kernel, t)))
                .collect(),
        }
    }

    /// Adds the features of the pending elements (of those `only` keeps)
    /// in one command: per element its features and notes, or why its
    /// items did not build.
    fn add_batch(
        &mut self,
        items: &[Pending<'_>],
        only: Option<&[bool]>,
    ) -> Result<Vec<Built>, String> {
        let names = &mut self.feature_names;
        let label = format!("Import {} elements", items.len());
        self.document
            .add_features_with(&label, |batch| {
                let mut out = Vec::with_capacity(items.len());
                for (k, p) in items.iter().enumerate() {
                    if only.is_some_and(|o| !o[k]) {
                        out.push(Err(String::new()));
                        continue;
                    }
                    let mut b = Builder::new(batch, names, p.component, &p.label);
                    let mut bodies = match b.node(&p.solid, Op::New) {
                        Ok(bodies) => bodies,
                        Err(why) => {
                            out.push(Err(why));
                            continue;
                        }
                    };
                    let mut failed = None;
                    for extra in &p.more {
                        match b.node(extra, Op::New) {
                            Ok(more) => bodies.extend(more),
                            Err(why) => {
                                failed = Some(why);
                                break;
                            }
                        }
                    }
                    if let Some(why) = failed {
                        out.push(Err(why));
                        continue;
                    }
                    let mut notes = Vec::new();
                    let mut cuts = Vec::new();
                    for (what, tool) in &p.cuts {
                        b.label = format!("{} cut", p.label);
                        let before = b.features.len();
                        let made = b.node(tool, Op::With("cut", bodies.clone()));
                        if let Err(why) = &made {
                            notes.push(format!("the cut of {what} is not made: {why}"));
                        }
                        cuts.push((what.clone(), b.features[before..].to_vec(), made.is_ok()));
                    }
                    for (k, body) in bodies.iter().enumerate() {
                        let name = if k == 0 {
                            p.body_name.clone()
                        } else {
                            format!("{} ({})", p.body_name, k + 1)
                        };
                        let _ = b.batch.name_body(*body, &name);
                        if let Some(a) = &p.appearance {
                            let _ = b.batch.set_body_appearance(*body, a);
                        }
                    }
                    notes.extend(b.notes.iter().cloned());
                    out.push(Ok((b.features.clone(), notes, cuts)));
                }
                Ok(out)
            })
            .map_err(|e| format!("its features were refused: {e}"))
    }

    /// Whether an element's features built and agree with the reference
    /// (else its stored box): what it was checked against, the cuts that
    /// remove nothing (to suppress), or why not.
    fn judge(
        &self,
        stored: Option<[f64; 6]>,
        features: &[FeatureUid],
        line: &mut ElementLine,
        attached: &[String],
        kept: Option<(f64, Bbox)>,
        kept_box: Option<Bbox>,
    ) -> Result<(&'static str, Vec<FeatureUid>), String> {
        // A cut that removes nothing stays, suppressed; another failing
        // feature fails the element.
        let mut misses = Vec::new();
        for uid in features {
            if let Some(error) = self.document.status(*uid).and_then(|s| s.error()) {
                // (An intersection whose tool holds the whole body changes
                // nothing either: an edited wall within its footprint.)
                if mitcad_import_ifc::is_miss(error)
                    || error.contains("does not intersect any participant body")
                {
                    misses.push(*uid);
                } else {
                    return Err(format!("a feature failed: {error}"));
                }
            }
        }
        let (volume, bbox, bodies) = measure(self.document, features);
        line.volume = volume;
        line.bodies = bodies;
        let against = check(
            line,
            stored,
            self.options.reference,
            volume,
            bbox,
            kept,
            kept_box,
        )?;
        if against != "reference" && !attached.is_empty() {
            check_heights(stored, bbox, attached, kept)?;
        }
        Ok((against, misses))
    }

    /// Builds the pending elements' plans in one command (mitcad#333: one
    /// command per element made an import of many elements take quadratic
    /// time): the elements whose features fail or disagree with the
    /// reference (else their stored boxes) are left out of it, built again
    /// without them, and come in as their meshes or boxes with the reason.
    fn flush(
        &mut self,
        model: &Model,
        pending: &mut Vec<Pending<'_>>,
        levels: &mut [LevelGroup<K::Shape>],
        curtain_hosts: &HashSet<i64>,
    ) {
        if pending.is_empty() {
            return;
        }
        let mut items = std::mem::take(pending);
        let mut lines: Vec<ElementLine> = items
            .iter_mut()
            .map(|p| std::mem::take(&mut p.line))
            .collect();
        let mut verdicts: Vec<Verdict> = match self.add_batch(&items, None) {
            Ok(built) => built
                .into_iter()
                .zip(items.iter())
                .zip(lines.iter_mut())
                .map(|((b, p), line)| {
                    let (features, notes, cuts) = b?;
                    line.notes.extend(notes);
                    let (against, misses) = self.judge(
                        moved_box(p.e.bbox, p.shift),
                        &features,
                        line,
                        &p.attached,
                        p.kept,
                        p.kept_box,
                    )?;
                    Ok((against, features, misses, cuts))
                })
                .collect(),
            Err(why) => items.iter().map(|_| Err(why.clone())).collect(),
        };
        let keep: Vec<bool> = verdicts.iter().map(Result::is_ok).collect();
        if keep.iter().any(|k| !k) {
            // The batch again without the elements that failed.
            while self.document.undo_depth() > self.depth + 1 {
                if self.document.undo().is_none() {
                    break;
                }
            }
            if keep.iter().any(|k| *k) {
                match self.add_batch(&items, Some(&keep)) {
                    Ok(built) => {
                        for (k, b) in built.into_iter().enumerate() {
                            if !keep[k] {
                                continue;
                            }
                            match b {
                                Ok((features, _, cuts)) => {
                                    let misses = features
                                        .iter()
                                        .copied()
                                        .filter(|u| {
                                            self.document
                                                .status(*u)
                                                .and_then(|s| s.error())
                                                .is_some()
                                        })
                                        .collect();
                                    if let Ok((against, _, _, _)) = &verdicts[k] {
                                        verdicts[k] = Ok((against, features, misses, cuts));
                                    }
                                }
                                Err(why) => verdicts[k] = Err(why),
                            }
                        }
                    }
                    Err(why) => {
                        for v in verdicts.iter_mut() {
                            *v = Err(why.clone());
                        }
                    }
                }
            }
        }
        for (verdict, line) in verdicts.iter().zip(lines.iter_mut()) {
            if let Ok((_, _, misses, _)) = verdict {
                for uid in misses {
                    if self.document.set_suppressed(*uid, true).is_ok() {
                        line.notes
                            .push("a cut that removes nothing is suppressed".into());
                    }
                }
            }
        }
        self.document.merge_undo(self.depth, &self.label);
        for ((p, verdict), mut line) in items.iter().zip(verdicts).zip(lines) {
            match verdict {
                Ok((against, features, misses, cuts)) => {
                    self.openings_built(p, &line, &misses, &cuts);
                    line.checked_against = Some(against.to_string());
                    line.features = features.len();
                    line.outcome = "features".into();
                    self.report.features += 1;
                    *self.report.built.entry(p.kind.to_string()).or_default() += 1;
                    levels[p.slot].entry.features += 1;
                    levels[p.slot].entry.elements.push(p.e.id);
                    // Its properties on the bodies its features made
                    // (mitcad#496).
                    let bodies = feature_bodies(self.document, &features);
                    let level = levels[p.slot].entry.name.clone();
                    self.element_properties(model, p.e, &level, &bodies);
                    self.report.elements.push(line);
                }
                Err(why) => {
                    line.notes.push(format!("not built as features: {why}"));
                    self.body_names.remove(&p.body_name);
                    self.fail(
                        model,
                        p.e,
                        p.kind,
                        &mut levels[p.slot],
                        line,
                        curtain_hosts,
                        &p.openings,
                        p.shift,
                    );
                }
            }
        }
    }

    /// The openings of a plan without a cut tool of their own, cut by the
    /// IFC export's opening of the insert's GlobalId when the options give
    /// one (mitcad#345, [`RvtOptions::openings`]).
    fn reference_cuts(&self, p: &mut Plan, global_ids: &HashMap<i64, String>) {
        if self.options.openings.is_empty() {
            return;
        }
        for o in &mut p.openings {
            let Some(why) = o.missing.clone() else {
                continue;
            };
            let items = global_ids
                .get(&o.id)
                .and_then(|g| self.options.openings.get(g))
                .filter(|items| !items.is_empty());
            let Some(items) = items else {
                o.missing = Some(format!(
                    "{why}, and the reference has no opening of its GlobalId"
                ));
                continue;
            };
            p.cuts
                .extend(items.iter().cloned().map(|i| (o.label.clone(), i)));
            p.notes.push(format!(
                "{} is cut by the reference's opening of its GlobalId ({why})",
                o.label
            ));
            o.missing = None;
        }
    }

    /// A wall built as features (mitcad#345): each of its openings is cut
    /// when one of its cuts was made and removes something; else it is
    /// missing, with why.
    fn openings_built(
        &mut self,
        p: &Pending<'_>,
        line: &ElementLine,
        misses: &[FeatureUid],
        cuts: &[CutMade],
    ) {
        for o in &p.openings {
            if let Some(why) = &o.missing {
                self.opening_missing(p.e.id, &o.label, why.clone(), why.clone());
                continue;
            }
            let mine: Vec<&CutMade> = cuts.iter().filter(|(w, _, _)| *w == o.label).collect();
            let cut = mine.iter().any(|(_, uids, made)| {
                *made && !uids.is_empty() && !uids.iter().any(|u| misses.contains(u))
            });
            if cut {
                self.report.openings_cut += 1;
            } else if mine.iter().any(|(_, _, made)| !made) {
                let prefix = format!("the cut of {} is not made: ", o.label);
                let why = line
                    .notes
                    .iter()
                    .find_map(|n| n.strip_prefix(&prefix))
                    .unwrap_or("?");
                self.opening_missing(
                    p.e.id,
                    &o.label,
                    "its cut failed".into(),
                    format!("its cut failed: {why}"),
                );
            } else if mine.is_empty() {
                self.opening_missing(
                    p.e.id,
                    &o.label,
                    "its cut is not made".into(),
                    "its cut is not made".into(),
                );
            } else {
                self.opening_missing(
                    p.e.id,
                    &o.label,
                    "its cut removes nothing (suppressed)".into(),
                    "its cut removes nothing (suppressed)".into(),
                );
            }
        }
    }

    /// An opening that its wall does not have (mitcad#345): counted by
    /// reason, listed, and noted on the insert's line at the end of the
    /// model's import.
    fn opening_missing(&mut self, wall: i64, label: &str, reason: String, why: String) {
        *self.report.openings_missing.entry(reason).or_default() += 1;
        if self.report.openings_missing_lines.len() < 200 {
            self.report
                .openings_missing_lines
                .push(format!("{label} in wall {wall}: {why}"));
        }
        if let Some(id) = label.rsplit(' ').next().and_then(|s| s.parse::<i64>().ok()) {
            self.missing.push((id, wall, why));
        }
    }

    /// The openings of a wall that is not built as features (mitcad#345):
    /// a body of the geometry the file keeps, or the reference's, holds
    /// them; a saved mesh or a box does not.
    fn openings_fallen_back(
        &mut self,
        e: &Element,
        line: &mut ElementLine,
        openings: &[plan::WallOpening],
    ) {
        if openings.is_empty() {
            return;
        }
        let kept = match line.outcome.as_str() {
            "reference body" => Some("the reference's body"),
            "mesh"
                if line
                    .notes
                    .iter()
                    .any(|n| n.starts_with("its geometry as the file keeps it")) =>
            {
                Some("the geometry the file keeps")
            }
            _ => None,
        };
        match kept {
            Some(body) => {
                self.report.openings_cut += openings.len();
                line.notes
                    .push(format!("its {} openings are in {body}", openings.len()));
            }
            None => {
                let what = match line.outcome.as_str() {
                    "mesh" => "its saved mesh",
                    "box" => "its box",
                    "parts" => "its parts",
                    _ => "no body",
                };
                for o in openings {
                    self.opening_missing(
                        e.id,
                        &o.label,
                        format!("its wall came in as {what}"),
                        format!("its wall came in as {what}"),
                    );
                }
            }
        }
    }

    /// An element of a kind built as features that is not: a curtain wall
    /// has its panels and mullions; another comes in as its mesh or box.
    #[allow(clippy::too_many_arguments)]
    fn fail(
        &mut self,
        model: &Model,
        e: &Element,
        kind: &str,
        group: &mut LevelGroup<K::Shape>,
        mut line: ElementLine,
        curtain_hosts: &HashSet<i64>,
        openings: &[plan::WallOpening],
        shift: f64,
    ) {
        // A curtain wall: its panels and mullions are its shape, elements
        // of their own (mitcad#333).
        if curtain_hosts.contains(&e.id) {
            line.outcome = "parts".into();
            line.notes.push(
                "a curtain wall: its panels and mullions come in as elements of their own".into(),
            );
            self.report.curtain_walls += 1;
            self.report.elements.push(line);
            return;
        }
        // Cut away whole by the elements joined to it, in the file too: its
        // bodies there bound nothing (faces without edges).
        if line
            .notes
            .iter()
            .any(|n| n.ends_with("its features made no body"))
            && model.geometry.get(&e.id).is_some_and(|g| {
                !g.solids.is_empty()
                    && g.instances.is_empty()
                    && g.solids
                        .iter()
                        .all(|s| s.faces.iter().all(|f| f.loops.is_empty()))
            })
        {
            line.outcome = "not built".into();
            line.notes.push(
                "the elements that cut it remove all of it, as in the geometry the file keeps \
                 (its faces have no edges): no body"
                    .into(),
            );
            self.report.elements.push(line);
            return;
        }
        *self.report.fallbacks.entry(kind.to_string()).or_default() += 1;
        self.fallback(model, e, plan::structural(kind), group, &mut line, shift);
        self.openings_fallen_back(e, &mut line, openings);
        self.report.elements.push(line);
    }

    /// Whether an element's mesh (from the geometry the file keeps) agrees
    /// with the reference's solid (its box within [`BOX_TOLERANCE_MM`],
    /// a closed mesh's volume within [`VOLUME_TOLERANCE`]), else lies
    /// within the element's stored box (but for [`MESH_BOX_FRACTION`] of
    /// it): what it was checked against.
    fn check_mesh(
        &self,
        stored: Option<[f64; 6]>,
        line: &mut ElementLine,
        mesh: &geometry::ElementMesh,
    ) -> Result<&'static str, String> {
        let bbox = mesh.bounds().ok_or("has no points")?;
        line.volume = mesh.volume;
        let solid = self
            .options
            .reference
            .and_then(|r| line.global_id.as_ref().and_then(|g| r.solids.get(g)));
        if let Some(solid) = solid.filter(|s| s.bbox.is_some()) {
            if let (Some(v), Some(rv)) = (mesh.volume, solid.volume.filter(|v| *v > 0.0)) {
                let d = (v - rv).abs() / rv;
                line.reference_volume = Some(rv);
                line.volume_difference = Some(d);
                if d > VOLUME_TOLERANCE {
                    return Err(format!(
                        "encloses {:.4} m³, the reference's solid {:.4} m³ ({:.2} % apart)",
                        v * 1e-9,
                        rv * 1e-9,
                        d * 100.0
                    ));
                }
            }
            let rb = solid.bbox.as_ref().expect("filtered");
            let d = box_difference(&bbox, rb);
            line.box_difference = Some(d);
            if d > BOX_TOLERANCE_MM {
                return Err(format!("lies {d:.1} mm from the reference's box"));
            }
            return Ok("reference");
        }
        let Some(stored) = stored else {
            return Ok("nothing (no stored box)");
        };
        let s: Bbox = (
            [stored[0], stored[1], stored[2]].map(|v| v * FOOT_MM),
            [stored[3], stored[4], stored[5]].map(|v| v * FOOT_MM),
        );
        let out = (0..3)
            .map(|k| (s.0[k] - bbox.0[k]).max(bbox.1[k] - s.1[k]))
            .fold(f64::NEG_INFINITY, f64::max);
        line.box_difference = Some(out.max(0.0));
        // (The stored box need not hold every part of a type's geometry:
        // a door's hardware reaches 19 mm out of it. A mesh misplaced lies
        // much further.)
        let extent = (0..3).map(|k| s.1[k] - s.0[k]).fold(0.0, f64::max);
        let allowed = BOX_TOLERANCE_MM.max(MESH_BOX_FRACTION * extent);
        if out > allowed {
            return Err(format!(
                "reaches {out:.1} mm outside the element's stored box (at most {allowed:.1} mm)"
            ));
        }
        Ok("stored box")
    }

    /// Adds an element's body to its level's base feature.
    fn add_body(
        &mut self,
        e: &Element,
        shape: K::Shape,
        group: &mut LevelGroup<K::Shape>,
        line: &mut ElementLine,
    ) {
        line.bodies = 1;
        group.entry.elements.push(e.id);
        group.body_elements.push(e.id);
        let name = self.unique_body(&line.name.clone());
        group.bodies.push(ImportBody {
            name: Some(name),
            color: None,
            shape,
        });
    }

    /// An element not built as features, in its level's base feature: the
    /// first of its sources that gives a body (mitcad#333):
    ///
    /// 1. the geometry the file keeps of it (its own bodies and the types
    ///    it places), as a mesh that agrees with the reference or lies in
    ///    its stored box;
    /// 2. the reference's (the IFC export's) bodies of its GlobalId;
    /// 3. the triangle mesh its record saves;
    /// 4. its box: a family instance's type box at its transform, else its
    ///    stored box; hidden unless the element is structural.
    ///
    /// The line says which, and for a box why the others gave nothing.
    fn fallback(
        &mut self,
        model: &Model,
        e: &Element,
        structural: bool,
        group: &mut LevelGroup<K::Shape>,
        line: &mut ElementLine,
        shift: f64,
    ) {
        let mut why: Vec<String> = Vec::new();
        // 1. The geometry the file keeps.
        // A railing's rails and balusters, a stair's runs, landings and
        // supports, a rail's supports are elements of their own.
        let railing = e.class.as_deref() == Some("BaseRailing");
        let stair = e.class.as_deref() == Some("StairsElement");
        let skip = self.skip_of(model, e).unwrap_or_default();
        let mut mesh = geometry::element_mesh(model, e.id, &mut self.tess, &skip);
        if railing && !matches!(mesh, Some(Ok(_))) {
            line.outcome = "parts".into();
            line.notes
                .push("a railing: its rails and balusters come in as elements of their own".into());
            self.report.railings += 1;
            return;
        }
        if stair && !matches!(mesh, Some(Ok(_))) {
            line.outcome = "parts".into();
            line.notes.push(
                "a stair: its runs, landings and supports come in as elements of their own".into(),
            );
            self.report.stairs += 1;
            return;
        }
        // (A copy up a multistory stair, mitcad#391: moved up.)
        if let Some(Ok(m)) = &mut mesh {
            for p in &mut m.points {
                p[2] += shift * FOOT_MM;
            }
        }
        match mesh {
            Some(Ok(mesh)) => match self.check_mesh(moved_box(e.bbox, shift), line, &mesh) {
                Ok(against) => match self
                    .document
                    .kernel()
                    .triangle_body(&mesh.points, &mesh.triangles)
                {
                    Ok(shape) => {
                        line.outcome = "mesh".into();
                        line.checked_against = Some(against.to_string());
                        line.notes.push(format!(
                            "its geometry as the file keeps it, as a mesh: {} bodies, {} faces, {} triangles{}",
                            mesh.bodies,
                            mesh.faces,
                            mesh.triangles.len(),
                            if mesh.faces_left_out > 0 {
                                format!(
                                    " ({} left out: {})",
                                    mesh.faces_left_out,
                                    mesh.notes.join("; ")
                                )
                            } else {
                                String::new()
                            }
                        ));
                        group.entry.meshes += 1;
                        self.report.geometry_meshes += 1;
                        self.report.geometry_triangles += mesh.triangles.len();
                        self.add_body(e, shape, group, line);
                        return;
                    }
                    Err(err) => why.push(format!("its geometry's mesh was not built: {err}")),
                },
                Err(w) => why.push(format!("its geometry as the file keeps it {w}")),
            },
            Some(Err(w)) => why.push(format!("its geometry did not mesh: {w}")),
            None => why.push("the file keeps no geometry of it".into()),
        }
        // 2. The reference's bodies.
        let solid = self
            .options
            .reference
            .and_then(|r| line.global_id.as_ref().and_then(|g| r.solids.get(g)));
        match solid {
            Some(s) if !s.bodies.is_empty() => {
                let kernel = self.document.kernel();
                let shapes: Result<Vec<K::Shape>, _> = s
                    .bodies
                    .iter()
                    .map(|d| kernel.import_brep(FeatureUid(0), d, 0).map(|(s, _)| s))
                    .collect();
                let shape = shapes.and_then(|mut v| {
                    if v.len() == 1 {
                        Ok(v.remove(0))
                    } else {
                        kernel.compound(&v)
                    }
                });
                match shape {
                    Ok(shape) => {
                        line.outcome = "reference body".into();
                        line.checked_against = Some("reference".into());
                        line.notes.push(format!(
                            "the reference's body of its GlobalId ({} {}): {}",
                            s.kind,
                            s.name,
                            why.join("; ")
                        ));
                        self.report.reference_bodies += 1;
                        group.entry.meshes += 1;
                        self.add_body(e, shape, group, line);
                        return;
                    }
                    Err(err) => why.push(format!("the reference's body was not built: {err}")),
                }
            }
            Some(_) => why.push("the reference's solid of its GlobalId has no body".into()),
            None if self.options.reference.is_some() => {
                why.push("the reference has no solid of its GlobalId".into())
            }
            None => {}
        }
        // 3. and 4. The saved mesh, the box.
        let shape = match shape_of(model, e) {
            Some(Shape::Mesh) => {
                let meshes = &model.meshes[&e.id];
                let mut vertices = Vec::new();
                let mut triangles = Vec::new();
                for m in meshes {
                    let first = vertices.len() as u32;
                    vertices.extend(
                        m.points
                            .iter()
                            .map(|p| [p[0] * FOOT_MM, p[1] * FOOT_MM, (p[2] + shift) * FOOT_MM]),
                    );
                    triangles.extend(m.triangles.iter().map(|t| t.map(|i| i + first)));
                }
                self.document
                    .kernel()
                    .triangle_body(&vertices, &triangles)
                    .map(|s| (s, 0))
            }
            Some(Shape::Oriented(mut frame, size)) => {
                frame.origin[2] += shift * FOOT_MM;
                make_box(self.document, frame, size).map(|s| (s, 1))
            }
            Some(Shape::Aligned(mut frame, size)) => {
                frame.origin[2] += shift * FOOT_MM;
                make_box(self.document, frame, size).map(|s| (s, 2))
            }
            None => {
                line.outcome = "not built".into();
                line.notes.push(format!("no body: {}", why.join("; ")));
                return;
            }
        };
        match shape {
            Ok((shape, kind)) => {
                if kind == 0 {
                    group.entry.meshes += 1;
                    self.report.meshes += 1;
                    line.outcome = "mesh".into();
                    line.notes
                        .push(format!("its saved mesh: {}", why.join("; ")));
                } else {
                    group.entry.boxes += 1;
                    if kind == 1 {
                        self.report.oriented_boxes += 1;
                    } else {
                        self.report.boxes += 1;
                    }
                    line.outcome = "box".into();
                    line.notes.push(format!(
                        "{}: {}",
                        if kind == 1 {
                            "its type's box at its transform"
                        } else {
                            "its stored box"
                        },
                        why.join("; ")
                    ));
                    if !structural {
                        group.hidden.push(group.bodies.len());
                        self.report.hidden_boxes += 1;
                        line.hidden = true;
                        line.notes.push(
                            "its box is hidden (not a structural element); showing its body \
                             shows it"
                                .into(),
                        );
                    }
                }
                self.add_body(e, shape, group, line);
            }
            Err(err) => {
                line.outcome = "not built".into();
                line.notes.push(format!("its body was not built: {err}"));
                if self.report.not_built.len() < 50 {
                    self.report.not_built.push((e.id, err.to_string()));
                }
            }
        }
    }

    /// The levels' construction planes and the grids' axes (an arc grid's
    /// sketched arc), in one command.
    fn levels_and_grids(
        &mut self,
        model: &Model,
        parent: ComponentUid,
        levels: &[LevelGroup<K::Shape>],
    ) {
        let planes: Vec<(ComponentUid, f64, String)> = levels
            .iter()
            .filter_map(|g| {
                Some((
                    g.component,
                    g.entry.elevation?,
                    format!("{} level", g.entry.name),
                ))
            })
            .collect();
        let grids = &model.building.grids;
        if planes.is_empty() && grids.is_empty() {
            return;
        }
        let names = &mut self.feature_names;
        let mut unique = |name: &str| {
            let mut out = name.to_string();
            let mut n = 2;
            while names.contains(&out) {
                out = format!("{name} ({n})");
                n += 1;
            }
            names.insert(out.clone());
            out
        };
        let plane_names: Vec<String> = planes.iter().map(|(_, _, n)| unique(n)).collect();
        let grid_names: Vec<String> = grids
            .iter()
            .map(|g| {
                unique(&format!(
                    "Grid {}",
                    if g.name.is_empty() {
                        g.id.to_string()
                    } else {
                        g.name.clone()
                    }
                ))
            })
            .collect();
        let mut warnings = Vec::new();
        let result = self.document.add_features_with("Import levels and grids", |batch| {
            let mut made = (0, 0, 0);
            for ((component, z, _), name) in planes.iter().zip(&plane_names) {
                let def = json!({"type": "construction_plane", "definition": {"type": "fixed",
                    "origin": [0.0, 0.0, z], "x_axis": [1.0, 0.0, 0.0], "y_axis": [0.0, 1.0, 0.0]}});
                let def = serde_json::from_value(def).expect("a plane definition");
                if batch.add(&def, Some(name), *component).is_ok() {
                    made.0 += 1;
                }
            }
            for (grid, name) in grids.iter().zip(&grid_names) {
                match &grid.curve {
                    Some(Curve::Line { start, end }) => {
                        let s = start.map(|v| v * FOOT_MM);
                        let d = [end[0] - start[0], end[1] - start[1], end[2] - start[2]].map(|v| v * FOOT_MM);
                        let def = json!({"type": "construction_axis", "definition": {"type": "fixed",
                            "origin": s, "direction": d}});
                        let def = serde_json::from_value(def).expect("an axis definition");
                        match batch.add(&def, Some(name), parent) {
                            Ok(_) => made.1 += 1,
                            Err(e) => warnings.push(format!("grid {}: {e}", grid.name)),
                        }
                    }
                    Some(arc) => {
                        let z = arc.start()[2] * FOOT_MM;
                        let frame = mitcad_ifc::math::from_axes(
                            [0.0, 0.0, z],
                            [1.0, 0.0, 0.0],
                            [0.0, 1.0, 0.0],
                            [0.0, 0.0, 1.0],
                        );
                        let path = footprint::to_mm(&mitcad_ifc::geometry::Path {
                            segments: vec![footprint::seg(arc)],
                        });
                        match mitcad_import_ifc::sketch::path_sketch(&frame, &path) {
                            Ok(sketch) => {
                                let def = serde_json::from_value(sketch.def)
                                    .map_err(|e| e.to_string());
                                match def.map(|d| batch.add(&d, Some(name), parent)) {
                                    Ok(Ok(_)) => made.2 += 1,
                                    Ok(Err(e)) => warnings.push(format!("grid {}: {e}", grid.name)),
                                    Err(e) => warnings.push(format!("grid {}: {e}", grid.name)),
                                }
                            }
                            Err(e) => warnings.push(format!("grid {}: {e}", grid.name)),
                        }
                    }
                    None => warnings.push(format!(
                        "grid {}: {}",
                        grid.name,
                        grid.note.as_deref().unwrap_or("no curve")
                    )),
                }
            }
            Ok(made)
        });
        match result {
            Ok((p, a, s)) => {
                self.report.level_planes += p;
                self.report.grid_axes += a;
                self.report.grid_arcs += s;
            }
            Err(e) => warnings.push(format!("levels and grids not added: {e}")),
        }
        self.report.warnings.extend(warnings);
    }

    /// Imports a model's elements into `parent`, one component per level.
    fn import_model(
        &mut self,
        model_key: usize,
        file: &str,
        model: &Model,
        parent: ComponentUid,
        link: Option<usize>,
    ) -> Result<usize, ModelError> {
        // Levels of one name and elevation are one group, under the first of
        // them (a project can hold several, mitcad#310); unnamed levels stay
        // apart.
        let mut first_of: HashMap<(&str, u64), i64> = HashMap::new();
        let mut merged: BTreeMap<i64, Vec<i64>> = BTreeMap::new();
        let mut group_of: HashMap<i64, i64> = HashMap::new();
        for l in &model.levels {
            let first = if l.name.trim().is_empty() {
                l.id
            } else {
                *first_of
                    .entry((l.name.as_str(), l.elevation.to_bits()))
                    .or_insert(l.id)
            };
            if first != l.id {
                merged.entry(first).or_default().push(l.id);
            }
            group_of.insert(l.id, first);
        }

        // The elements by level, in the table's order; every level has its
        // group.
        let placed = group_placements(model);
        // A railing's parts: its balusters, rails and rails' types.
        self.parts = model
            .elements
            .iter()
            .filter(|e| {
                matches!(e.category, Some(-2_000_948..=-2_000_946))
                    || e.class.as_deref() == Some("ContinuousRailSymbol")
            })
            .map(|e| e.id)
            .collect();
        self.rail_symbols = model
            .elements
            .iter()
            .filter(|e| e.class.as_deref() == Some("ContinuousRailSymbol"))
            .map(|e| e.id)
            .collect();
        self.own_elements = model
            .elements
            .iter()
            .filter(|e| !e.is_type)
            .map(|e| e.id)
            .collect();
        // The curtain walls: the hosts of panels and mullions.
        let curtain_hosts: HashSet<i64> = model
            .elements
            .iter()
            .filter(|e| matches!(e.category, Some(-2_000_170 | -2_000_171)))
            .filter_map(|e| model.building.instances.get(&e.id)?.host)
            .collect();
        let mut groups: BTreeMap<Option<i64>, Vec<&Element>> = BTreeMap::new();
        for l in &model.levels {
            if group_of.get(&l.id) == Some(&l.id) {
                groups.entry(Some(l.id)).or_default();
            }
        }
        for e in &model.elements {
            if let Some(why) = left_out(model, e, &placed) {
                *self.report.left_out.entry(why).or_default() += 1;
                continue;
            }
            if link.is_none()
                && let Some(only) = &self.options.only
                && !only.contains(&e.id)
            {
                *self
                    .report
                    .left_out
                    .entry("not among the elements asked for".into())
                    .or_default() += 1;
                continue;
            }
            let level = level_of(model, e);
            if level.is_some_and(|(_, by_elevation)| by_elevation) {
                self.report.by_elevation += 1;
            }
            let level = level.map(|(l, _)| group_of.get(&l).copied().unwrap_or(l));
            groups.entry(level).or_default().push(e);
        }
        let mut order: Vec<Option<i64>> = groups.keys().copied().collect();
        let elevation = |l: &Option<i64>| {
            l.and_then(|l| model.level(l))
                .map_or(f64::NEG_INFINITY, |l| l.elevation)
        };
        order.sort_by(|a, b| elevation(a).total_cmp(&elevation(b)));
        order.retain(|l| l.is_some() || !groups[l].is_empty());

        // One component per level.
        let mut levels: Vec<LevelGroup<K::Shape>> = Vec::new();
        for level in &order {
            let (name, level_elevation) = match level.and_then(|l| model.level(l)) {
                Some(l) => (
                    if l.name.trim().is_empty() {
                        format!("Level {}", l.id)
                    } else {
                        l.name.clone()
                    },
                    Some(l.elevation * FOOT_MM),
                ),
                None => ("Elements".to_string(), None),
            };
            let (component, _) =
                self.document
                    .add_component(Some(&name), parent, Transform::IDENTITY)?;
            levels.push(LevelGroup {
                entry: LevelReport {
                    id: *level,
                    merged: level
                        .and_then(|l| merged.get(&l))
                        .cloned()
                        .unwrap_or_default(),
                    name: self.document.assembly().name(component),
                    elevation: level_elevation,
                    component: Some(component.to_string()),
                    link,
                    ..LevelReport::default()
                },
                component,
                bodies: Vec::new(),
                hidden: Vec::new(),
                body_elements: Vec::new(),
            });
        }
        self.levels_and_grids(model, parent, &levels);
        self.document.merge_undo(self.depth, &self.label);

        let global_ids = model.global_ids();
        // For the elements' properties (mitcad#496).
        self.global_ids = global_ids.clone();
        self.element_index = model
            .elements
            .iter()
            .enumerate()
            .map(|(i, e)| (e.id, i))
            .collect();
        let mut count = 0;
        let mut pending: Vec<Pending<'_>> = Vec::new();
        for (own_slot, level) in order.iter().enumerate() {
            for e in &groups[level] {
                if self.memory_low() {
                    self.report.memory_low += 1;
                    continue;
                }
                count += 1;
                let kind = if self.options.mesh_only {
                    None
                } else {
                    plan::kind_of(model, e)
                };
                // The element where it is, then its copies up its
                // multistory stair (mitcad#391), each on its level: (slot,
                // how far up in feet, the copy's stair and level).
                let mut places: Vec<Place> = vec![(own_slot, 0.0, None)];
                if let Some((stair, copies)) = multistory_copies(model, e) {
                    for (l, dz) in copies {
                        let at = group_of.get(&l).copied().unwrap_or(l);
                        let slot = order
                            .iter()
                            .position(|o| *o == Some(at))
                            .unwrap_or(own_slot);
                        places.push((slot, dz, Some((stair, l))));
                    }
                }
                for (slot, shift, copy) in places {
                    let component = levels[slot].component;
                    let mut line = ElementLine {
                        id: e.id,
                        link,
                        global_id: global_ids.get(&e.id).cloned().filter(|_| copy.is_none()),
                        kind: kind.unwrap_or("other").to_string(),
                        category: e
                            .category_name
                            .map(str::to_string)
                            .or_else(|| e.category.map(|c| format!("category {c}")))
                            .unwrap_or_default(),
                        name: e.display_name(),
                        level: Some(levels[slot].entry.name.clone()),
                        ..ElementLine::default()
                    };
                    if let Some((stair, l)) = copy {
                        let name = model.level(l).map_or(l.to_string(), |l| l.name.clone());
                        line.name = format!("{} (on {name})", e.display_name());
                        line.notes.push(format!(
                            "a copy its multistory stair places on level {name} (stair {stair}): \
                             it moved {:.0} mm up",
                            shift * FOOT_MM
                        ));
                        self.report.multistory_copies += 1;
                    } else {
                        *self
                            .report
                            .categories
                            .entry(line.category.clone())
                            .or_default() += 1;
                    }
                    if let Some(kind) = kind {
                        let made = plan::plan(model, e).and_then(|p| {
                            if shift == 0.0 {
                                Ok(p)
                            } else {
                                plan::moved_plan(p, shift)
                            }
                        });
                        match made {
                            Ok(mut p) => {
                                if link.is_none() && copy.is_none() {
                                    self.reference_cuts(&mut p, &global_ids);
                                }
                                line.notes.extend(p.notes.iter().cloned());
                                let ready = self.prepare(
                                    model_key, model, e, kind, p, component, slot, line, shift,
                                );
                                pending.push(ready);
                                if pending.len() >= BATCH {
                                    self.flush(model, &mut pending, &mut levels, &curtain_hosts);
                                }
                            }
                            Err(why) => {
                                line.notes.push(format!("not built as features: {why}"));
                                let openings = expected_openings(model, e, &curtain_hosts);
                                self.fail(
                                    model,
                                    e,
                                    kind,
                                    &mut levels[slot],
                                    line,
                                    &curtain_hosts,
                                    &openings,
                                    shift,
                                );
                            }
                        }
                        continue;
                    } else {
                        line.notes.push(if self.options.mesh_only {
                            "not built as features (none asked for)".to_string()
                        } else {
                            "not an element of a kind built as features".to_string()
                        });
                    }
                    let structural = plan::kind_of(model, e).is_some_and(plan::structural);
                    self.fallback(model, e, structural, &mut levels[slot], &mut line, shift);
                    self.report.elements.push(line);
                }
            }
        }

        self.flush(model, &mut pending, &mut levels, &curtain_hosts);

        // The members a placed group places away from where its definition
        // keeps them: their meshes moved there, on the group's level
        // (mitcad#333).
        let mut moved: Vec<(i64, i64, [f64; 12])> = placed
            .iter()
            .flat_map(|(m, ts)| {
                ts.iter()
                    .filter(|(_, t)| !is_identity(t))
                    .map(move |(g, t)| (*m, *g, *t))
            })
            .collect();
        moved.sort_by_key(|(m, g, _)| (*g, *m));
        for (member, group, t) in moved {
            let Some(e) = model.element(member) else {
                continue;
            };
            if left_out(model, e, &HashMap::new()).is_some_and(|w| !w.starts_with("in a group")) {
                continue;
            }
            let slot = model
                .element(group)
                .and_then(|g| level_of(model, g))
                .map(|(l, _)| group_of.get(&l).copied().unwrap_or(l))
                .and_then(|l| order.iter().position(|o| *o == Some(l)))
                .unwrap_or(0);
            let mut line = ElementLine {
                id: e.id,
                link,
                global_id: None,
                kind: "other".into(),
                category: e
                    .category_name
                    .map(str::to_string)
                    .or_else(|| e.category.map(|c| format!("category {c}")))
                    .unwrap_or_default(),
                name: format!("{} (group {group})", e.display_name()),
                level: Some(levels[slot].entry.name.clone()),
                ..ElementLine::default()
            };
            let mesh = geometry::element_mesh(model, e.id, &mut self.tess, &HashSet::new());
            let built = match mesh {
                Some(Ok(mut m)) => {
                    // The transform's translation is in feet, the mesh in mm.
                    for p in &mut m.points {
                        let q: [f64; 3] = std::array::from_fn(|r| {
                            t[3 * r] * p[0]
                                + t[3 * r + 1] * p[1]
                                + t[3 * r + 2] * p[2]
                                + t[9 + r] * FOOT_MM
                        });
                        *p = q;
                    }
                    let det = t[0] * (t[4] * t[8] - t[5] * t[7])
                        - t[1] * (t[3] * t[8] - t[5] * t[6])
                        + t[2] * (t[3] * t[7] - t[4] * t[6]);
                    if det < 0.0 {
                        for tr in &mut m.triangles {
                            tr.swap(1, 2);
                        }
                    }
                    self.document
                        .kernel()
                        .triangle_body(&m.points, &m.triangles)
                        .map_err(|e| e.to_string())
                }
                Some(Err(w)) => Err(w),
                None => Err("the file keeps no geometry of it".into()),
            };
            match built {
                Ok(shape) => {
                    line.outcome = "mesh".into();
                    line.notes.push(format!(
                        "a member of group {group} placed away from its definition: its \
                         geometry as the file keeps it, moved there"
                    ));
                    self.report.group_copies += 1;
                    self.report.geometry_meshes += 1;
                    levels[slot].entry.meshes += 1;
                    line.bodies = 1;
                    levels[slot].entry.elements.push(e.id);
                    levels[slot].body_elements.push(e.id);
                    let name = self.unique_body(&line.name);
                    levels[slot].bodies.push(ImportBody {
                        name: Some(name),
                        color: None,
                        shape,
                    });
                }
                Err(why) => {
                    line.outcome = "not built".into();
                    line.notes.push(format!(
                        "a member of group {group} placed away from its definition is not \
                         built: {why}"
                    ));
                }
            }
            self.report.elements.push(line);
        }

        // The other elements' bodies: one base feature per level.
        let mut inputs: Vec<BaseInput<K::Shape>> = Vec::new();
        let mut notes_of: Vec<(usize, String)> = Vec::new();
        let mut hidden_of: Vec<(usize, Vec<usize>)> = Vec::new();
        // The element of each body (mitcad#496).
        let mut elements_of: Vec<Vec<i64>> = Vec::new();
        for (slot, group) in levels.iter_mut().enumerate() {
            if group.bodies.is_empty() {
                continue;
            }
            let entry = &mut group.entry;
            let feature_names = &mut self.feature_names;
            let feature_name = std::iter::once(format!("{} elements", entry.name))
                .chain((1..).map(|k| format!("{} elements ({k})", entry.name)))
                .find(|n| !feature_names.contains(n))
                .expect("a free name exists");
            feature_names.insert(feature_name.clone());
            entry.bodies = group.bodies.len();
            notes_of.push((
                slot,
                format!(
                    "The {} elements of {} not built as features: {} meshes (of the geometry the \
                     file keeps, the reference's bodies or saved meshes) and {} boxes (a family \
                     type's box at its instance's transform, or the element's box; {} hidden); \
                     the import report's element lines say why",
                    entry.bodies,
                    entry.name,
                    entry.meshes,
                    entry.boxes,
                    group.hidden.len()
                ),
            ));
            hidden_of.push((slot, std::mem::take(&mut group.hidden)));
            elements_of.push(std::mem::take(&mut group.body_elements));
            inputs.push(BaseInput {
                name: Some(feature_name),
                source: Some(format!("{file}: {}", entry.name)),
                component: Some(group.component),
                ..BaseInput::new(std::mem::take(&mut group.bodies))
            });
        }
        if !inputs.is_empty() {
            let imported = self.document.add_base_features(&self.label, inputs)?;
            for (((imported, (slot, note)), (_, hidden)), elements) in imported
                .iter()
                .zip(notes_of)
                .zip(hidden_of)
                .zip(elements_of)
            {
                levels[slot].entry.feature = Some(imported.feature.uid.to_string());
                // Each element's properties on its body (mitcad#496).
                let level = levels[slot].entry.name.clone();
                for (index, id) in elements.into_iter().enumerate() {
                    let Some(e) = self.element_index.get(&id).map(|i| &model.elements[*i]) else {
                        continue;
                    };
                    let body = mitcad_model::BodyUid::new(imported.feature.uid, index as u32);
                    self.element_properties(model, e, &level, &[body]);
                }
                self.notes.push((imported.feature.uid, note));
                // The boxes of elements that are not structural, hidden.
                for index in hidden {
                    let uid = mitcad_model::BodyUid::new(imported.feature.uid, index as u32);
                    if let Err(e) = self.document.set_body_visible(uid, false) {
                        self.report
                            .warnings
                            .push(format!("a box was not hidden: {e}"));
                    }
                }
            }
            self.document.merge_undo(self.depth, &self.label);
        }
        // The inserts without their openings, on their own lines
        // (mitcad#345).
        for (insert, wall, why) in std::mem::take(&mut self.missing) {
            if let Some(l) = self
                .report
                .elements
                .iter_mut()
                .find(|l| l.id == insert && l.link == link)
            {
                l.notes
                    .push(format!("its opening in wall {wall} is not cut: {why}"));
            }
        }
        self.report.bodies = self.report.meshes
            + self.report.geometry_meshes
            + self.report.reference_bodies
            + self.report.oriented_boxes
            + self.report.boxes;
        self.report
            .levels
            .extend(levels.into_iter().map(|g| g.entry));
        Ok(count)
    }
}

/// A link instance's transform (feet) as a placement (mm).
fn link_transform(t: &[f64; 12]) -> Transform {
    Transform {
        linear: [[t[0], t[1], t[2]], [t[3], t[4], t[5]], [t[6], t[7], t[8]]],
        translation: [t[9] * FOOT_MM, t[10] * FOOT_MM, t[11] * FOOT_MM],
    }
}

/// Imports the elements of `model` (read from `file`) into the document,
/// with the linked models the options give: one undo step. The process's
/// memory guard leaves the elements not yet built out when it runs low,
/// and the report says so.
pub fn import_rvt<K: Kernel>(
    document: &mut Document<K>,
    file: &str,
    model: &Model,
    options: &RvtOptions<'_>,
) -> Result<Report, ModelError> {
    let label = format!("Import {file}");
    let depth = document.undo_depth();
    // The joins and cuts fuse within the import's geometric tolerance where
    // they fail exactly (mitcad#298, mitcad#397: a rail's pieces meet in the
    // faces the file keeps of it, which lie on each other only within a
    // micrometre). The design keeps it; the import's undo step sets it.
    let _ = document.set_join_tolerance(Some(mitcad_import_base::DEFAULT_GEOMETRIC_TOLERANCE));
    let feature_names: HashSet<String> = document.features().map(|f| f.name.clone()).collect();
    let body_names: HashSet<String> = document.bodies().iter().map(|b| b.name.clone()).collect();
    let mut importer = Importer {
        document,
        report: Report {
            reference: options.reference.map(|r| r.file.clone()),
            ..Report::default()
        },
        options,
        appearances: HashMap::new(),
        feature_names,
        body_names,
        stopped: false,
        depth,
        label: label.clone(),
        notes: Vec::new(),
        tess: geometry::Cache::new(),
        parts: HashSet::new(),
        rail_symbols: HashSet::new(),
        own_elements: HashSet::new(),
        missing: Vec::new(),
        properties: Vec::new(),
        global_ids: HashMap::new(),
        element_index: HashMap::new(),
    };
    importer.import_model(0, file, model, ComponentUid::ROOT, None)?;

    // The links: each one's component, filled with its model when found.
    let mut links: Vec<(&Link, Option<&LinkedModel>, Option<String>)> = Vec::new();
    for l in &model.building.links {
        let found = options.links.iter().find(|m| m.link.id == l.id);
        let missing = options
            .missing_links
            .iter()
            .find(|(m, _)| m.id == l.id)
            .map(|(_, why)| why.clone());
        links.push((l, found, missing));
    }
    for (index, (link, found, missing)) in links.into_iter().enumerate() {
        let name = link
            .file
            .clone()
            .unwrap_or_else(|| format!("Link {}", link.id));
        let mut line = LinkReport {
            id: link.id,
            file: name.clone(),
            ..LinkReport::default()
        };
        let transform = link
            .transform
            .map(|t| link_transform(&t))
            .unwrap_or(Transform::IDENTITY);
        let component_name = if link.instance_name.is_empty() {
            name.clone()
        } else {
            format!("{name} : {}", link.instance_name)
        };
        match importer
            .document
            .add_component(Some(&component_name), ComponentUid::ROOT, transform)
        {
            Ok((component, _)) => {
                line.component = Some(component.to_string());
                importer.document.merge_undo(depth, &label);
                if let Some(linked) = found {
                    line.found = Some(linked.file.clone());
                    importer.report.links.push(line);
                    match importer.import_model(
                        index + 1,
                        &linked.file,
                        &linked.model,
                        component,
                        Some(index),
                    ) {
                        Ok(n) => importer.report.links[index].elements = n,
                        Err(e) => {
                            importer.report.links[index].note = Some(format!("not imported: {e}"))
                        }
                    }
                    if !linked.model.building.links.is_empty() {
                        importer.report.links[index].note = Some(format!(
                            "its own {} links are not followed",
                            linked.model.building.links.len()
                        ));
                    }
                    continue;
                }
                // An IFC model linked into the project (mitcad#333).
                if let Some(ifc) = options.ifc_links.iter().find(|l| l.link.id == link.id) {
                    line.found = Some(ifc.file.clone());
                    let ifc_options = mitcad_import_ifc::IfcOptions {
                        parent: component,
                        progress: None,
                        ..Default::default()
                    };
                    let made = mitcad_import_ifc::import_ifc(
                        importer.document,
                        &ifc.ifc,
                        &ifc.file,
                        &ifc_options,
                    );
                    line.elements = made.products.len();
                    line.note = Some(format!(
                        "an IFC model: {} products, {} bodies",
                        made.products.len(),
                        made.bodies
                    ));
                    importer.document.merge_undo(depth, &label);
                    importer.report.links.push(line);
                    continue;
                }
                line.note =
                    Some(missing.unwrap_or_else(|| {
                        format!("{name} was not found: its component is empty")
                    }));
            }
            Err(e) => line.note = Some(format!("no component: {e}")),
        }
        importer.report.links.push(line);
    }
    if importer.stopped {
        let n = importer.report.memory_low;
        importer.report.warnings.push(format!(
            "the process ran low on memory: {n} elements were left out"
        ));
    }
    // The elements' properties, one step merged into the import's
    // (mitcad#496).
    let properties = std::mem::take(&mut importer.properties);
    importer.report.properties = importer.document.import_properties(&label, properties);
    if !importer.notes.is_empty()
        && let Err(e) = importer.document.set_import_notes(&importer.notes)
    {
        importer.report.warnings.push(format!("import notes: {e}"));
    }
    let mut report = importer.report;
    if let Some(reference) = options.reference {
        reference_summary(&mut report, reference);
    }
    document.merge_undo(depth, &label);
    Ok(report)
}

/// The reference's kinds of solids the features are built for.
const REFERENCE_KINDS: [&str; 9] = [
    "IFCCOVERING",
    "IFCWALL",
    "IFCWALLSTANDARDCASE",
    "IFCSLAB",
    "IFCROOF",
    "IFCCOLUMN",
    "IFCBEAM",
    "IFCMEMBER",
    "IFCSTAIRFLIGHT",
];

/// How the reference's solids fared: those agreed with by kind, and the
/// walls, slabs, roofs, columns and beams no feature agrees with.
fn reference_summary(report: &mut Report, reference: &Reference) {
    let lines: HashMap<&str, &ElementLine> = report
        .elements
        .iter()
        .filter(|l| l.link.is_none())
        .filter_map(|l| Some((l.global_id.as_deref()?, l)))
        .collect();
    let mut solids: Vec<(&String, &ReferenceSolid)> = reference.solids.iter().collect();
    solids.sort_by(|a, b| a.0.cmp(b.0));
    for (gid, solid) in solids {
        let line = lines.get(gid.as_str());
        if line.is_some_and(|l| l.checked_against.as_deref() == Some("reference")) {
            *report
                .reference_agree
                .entry(solid.kind.clone())
                .or_default() += 1;
        } else if REFERENCE_KINDS.contains(&solid.kind.as_str())
            && report.reference_unmatched.len() < 100
        {
            let why = match line {
                None => "no element of the project has its GlobalId".to_string(),
                Some(l) => l
                    .notes
                    .iter()
                    .rev()
                    .find(|n| n.starts_with("not built"))
                    .cloned()
                    .unwrap_or_else(|| format!("{} ({})", l.outcome, l.kind)),
            };
            report
                .reference_unmatched
                .push(format!("{} {} {gid}: {why}", solid.kind, solid.name));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mitcad_model::testing::MockKernel;
    use mitcad_rvt::{RvtFile, testfile};

    #[test]
    fn the_test_project_comes_in_by_level() {
        let file = RvtFile::parse(testfile::project()).unwrap();
        let schema = file.schema().unwrap();
        let model = Model::read(&file, &schema).unwrap();
        let mut document = Document::new(MockKernel::default());
        let report =
            import_rvt(&mut document, "project.rvt", &model, &RvtOptions::default()).unwrap();
        // The column, the wall and the surface (a saved mesh); the levels,
        // the family, the types and the material stay out.
        assert_eq!(report.bodies, 3, "{report:?}");
        assert_eq!(report.meshes, 1);
        assert_eq!(report.oriented_boxes, 1);
        assert_eq!(report.boxes, 1);
        assert_eq!(report.left_out["types"], 2);
        assert!(report.left_out.contains_key("no records"));
        // One component per level that has elements, lowest first.
        let named: Vec<(&str, usize)> = report
            .levels
            .iter()
            .map(|l| (l.name.as_str(), l.bodies))
            .collect();
        assert_eq!(named, vec![("Ground", 2), ("Upper", 1)]);
        assert_eq!(report.levels[1].elevation, Some(10.0 * FOOT_MM));
        let bodies: Vec<String> = document.bodies().iter().map(|b| b.name.clone()).collect();
        assert!(
            bodies.contains(&format!("Square Column:24 x 24:{}", testfile::COLUMN)),
            "{bodies:?}"
        );
        assert!(bodies.contains(&format!("Basic Wall:Generic 8:{}", testfile::WALL)));
        // One undo step, and the timeline's notes.
        assert_eq!(document.undo_depth(), 1);
        assert_eq!(document.import_notes().count(), 2);
    }

    #[test]
    fn elements_bring_their_identity_and_parameters() {
        use mitcad_model::properties::{PropertyOwner, PropertyValue, Source};
        let file = RvtFile::parse(testfile::project()).unwrap();
        let schema = file.schema().unwrap();
        let model = Model::read(&file, &schema).unwrap();
        let mut document = Document::new(MockKernel::default());
        let report =
            import_rvt(&mut document, "project.rvt", &model, &RvtOptions::default()).unwrap();
        assert!(report.properties > 0, "{report:?}");
        // Still one undo step.
        assert_eq!(document.undo_depth(), 1);
        let body = |name: String| {
            document
                .bodies()
                .iter()
                .find(|b| b.name == name)
                .map(|b| PropertyOwner::Body(b.uid))
                .unwrap_or_else(|| panic!("no body {name}"))
        };
        let store = document.property_store();
        let value = |owner, key: &str| {
            let p = store
                .get(owner, properties::IDENTITY_SET, key)
                .unwrap_or_else(|| panic!("no {key}"));
            assert_eq!(p.source, Source::Rvt);
            p.value.clone()
        };
        let wall = body(format!("Basic Wall:Generic 8:{}", testfile::WALL));
        assert_eq!(
            value(wall, "ElementId"),
            PropertyValue::Integer(testfile::WALL)
        );
        assert_eq!(
            value(wall, "Family"),
            PropertyValue::Text("Basic Wall".into())
        );
        assert_eq!(value(wall, "Type"), PropertyValue::Text("Generic 8".into()));
        assert!(matches!(value(wall, "Level"), PropertyValue::Text(_)));
        let column = body(format!("Square Column:24 x 24:{}", testfile::COLUMN));
        assert_eq!(
            value(column, "ElementId"),
            PropertyValue::Integer(testfile::COLUMN)
        );
        assert_eq!(value(column, "Type"), PropertyValue::Text("24 x 24".into()));
        // The wall's record's number parameters, as its instance's.
        let element = model.element(testfile::WALL).unwrap();
        let numbers = element
            .params
            .iter()
            .filter(|(_, v)| matches!(v, mitcad_rvt::elements::ParamValue::Number(_)))
            .count();
        let instance = store
            .sets(wall)
            .iter()
            .find(|s| s.name == properties::INSTANCE_SET)
            .map_or(0, |s| s.properties.len());
        assert!(instance >= numbers.min(1), "{:?}", element.params);
    }

    #[test]
    fn levels_of_one_name_come_in_once_per_elevation() {
        // Two levels named alike at one elevation, a third at another, and
        // a level named like the first one's numbered component (mitcad#310).
        let level = |id, name: &str, elevation| mitcad_rvt::elements::Level {
            id,
            name: name.into(),
            elevation,
            building_story: None,
        };
        let wall = |id, level, z: f64| Element {
            id,
            partition: Some(1),
            category: Some(categories::OST_WALLS),
            level: Some(level),
            bbox: Some([0.0, 0.0, z, 1.0, 1.0, z + 1.0]),
            ..Element::default()
        };
        let model = Model {
            levels: vec![
                level(10, "Story", 0.0),
                level(11, "Story", 0.0),
                level(12, "Story (1)", 5.0),
                level(13, "Story", 10.0),
            ],
            elements: vec![
                wall(1, 10, 0.0),
                wall(2, 11, 0.0),
                wall(3, 12, 5.0),
                wall(4, 13, 10.0),
            ],
            ..Model::default()
        };
        let mut document = Document::new(MockKernel::default());
        let report =
            import_rvt(&mut document, "project.rvt", &model, &RvtOptions::default()).unwrap();
        let named: Vec<(&str, usize, &[i64])> = report
            .levels
            .iter()
            .map(|l| (l.name.as_str(), l.bodies, l.merged.as_slice()))
            .collect();
        assert_eq!(
            named,
            vec![
                ("Story", 2, &[11][..]),
                ("Story (1)", 1, &[][..]),
                ("Story (2)", 1, &[][..]),
            ]
        );
        let features: Vec<&str> = document.features().map(|f| f.name.as_str()).collect();
        assert_eq!(
            features,
            vec![
                "Story level",
                "Story (1) level",
                "Story (2) level",
                "Story elements",
                "Story (1) elements",
                "Story (2) elements"
            ]
        );
        // Imported again: new components and features, numbered.
        let again =
            import_rvt(&mut document, "project.rvt", &model, &RvtOptions::default()).unwrap();
        assert_eq!(again.levels[0].name, "Story (3)");
        assert_eq!(document.features().count(), 12);
    }

    #[test]
    fn the_house_wall_attached_to_its_roof_names_it() {
        // mitcad#348: the gable wall's line says what its top follows; the
        // walls that are not attached keep their heights.
        use mitcad_rvt::testfile::building as t;
        let file = RvtFile::parse(t::house()).unwrap();
        let schema = file.schema().unwrap();
        let model = Model::read(&file, &schema).unwrap();
        let kernel = MockKernel::default();
        kernel.measures.set(true);
        let mut document = Document::new(kernel);
        let report =
            import_rvt(&mut document, "house.rvt", &model, &RvtOptions::default()).unwrap();
        let line = |id| {
            report
                .elements
                .iter()
                .find(|l| l.id == id && l.link.is_none())
                .unwrap()
        };
        let c = line(t::WALL_C);
        assert_eq!(c.kind, "wall");
        assert!(
            c.notes
                .iter()
                .any(|n| n == "its top follows roof 44's lower surface (2 slopes)"),
            "{c:?}"
        );
        assert_eq!(line(t::ROOF).kind, "roof");
        for id in [t::WALL_A, t::WALL_B] {
            let l = line(id);
            assert_eq!(l.outcome, "features", "{l:?}");
        }
        let bodies = document.bodies();
        let a = bodies
            .iter()
            .find(|b| b.name.starts_with("Basic Wall:Brick 6:30"))
            .unwrap()
            .shape;
        let a = document.kernel().bounding_box(a).unwrap().unwrap();
        assert!((a.max[2] - t::HEIGHT * FOOT_MM).abs() < 1e-6, "{a:?}");

        // mitcad#345: the openings the report counts. The window's type
        // cannot cut wall B (built as features), the reason on the window's
        // line; the gable wall comes in as its box (the mock kernel's boxes
        // of cuts hold their tools), which holds no opening of its door.
        let why = "its type has neither cut loops nor void geometry";
        assert_eq!(report.openings_missing.get(why), Some(&1), "{report:?}");
        assert_eq!(
            report.openings_missing.get("its wall came in as its box"),
            Some(&1),
            "{:?}",
            report.openings_missing
        );
        assert_eq!(report.openings_cut, 0);
        assert!(
            report.openings_missing_lines.contains(&format!(
                "insert {} in wall {}: {why}",
                t::WINDOW,
                t::WALL_B
            )),
            "{:?}",
            report.openings_missing_lines
        );
        let window = line(t::WINDOW);
        assert!(
            window.notes.contains(&format!(
                "its opening in wall {} is not cut: {why}",
                t::WALL_B
            )),
            "{window:?}"
        );
        assert_eq!(line(t::WALL_C).outcome, "box", "{:?}", line(t::WALL_C));
        // mitcad#1051: the gable wall's box and the two missing openings
        // are warnings of the import, the box among the elements that came
        // in as boxes, and as a wall's fallback.
        use mitcad_model::notice::{Notice, NoticeLevel};
        let notices = report.notices();
        assert!(
            notices.contains(&Notice::warning("2 openings of the walls are missing")),
            "{notices:?}"
        );
        assert!(
            notices.iter().any(|n| n.level == NoticeLevel::Warning
                && n.message.contains("came in as")
                && n.message.contains("box")),
            "{notices:?}"
        );
        assert!(
            notices
                .iter()
                .any(|n| n.message.contains("of the kinds built as features")),
            "{notices:?}"
        );
        let door = line(t::DOOR);
        assert!(
            door.notes.contains(&format!(
                "its opening in wall {} is not cut: its wall came in as its box",
                t::WALL_C
            )),
            "{door:?}"
        );

        // With the IFC export's openings given, an insert without a tool of
        // its own is looked up by its GlobalId (the test project has none).
        let kernel = MockKernel::default();
        kernel.measures.set(true);
        let mut document = Document::new(kernel);
        let options = RvtOptions {
            openings: reference_openings(
                &mitcad_ifc::IfcFile::parse(mitcad_ifc::testdata::BUILDING).unwrap(),
            ),
            ..RvtOptions::default()
        };
        let report = import_rvt(&mut document, "house.rvt", &model, &options).unwrap();
        let reason = format!("{why}, and the reference has no opening of its GlobalId");
        assert!(
            report.openings_missing.contains_key(&reason),
            "{:?}",
            report.openings_missing
        );
    }

    #[test]
    fn an_ifc_export_s_openings_by_their_fillings() {
        // mitcad#345: the IFC test building's opening, by its own GlobalId
        // and its window's, in the project's coordinates.
        let file = mitcad_ifc::IfcFile::parse(mitcad_ifc::testdata::BUILDING).unwrap();
        let openings = reference_openings(&file);
        for gid in ["1OPEN00000000000000000", "1WIN000000000000000000"] {
            let items = &openings[gid];
            assert_eq!(items.len(), 1, "{gid}");
            let (lo, hi) = mitcad_import_ifc::transform::bounds(&items[0]).unwrap();
            let near = |a: [f64; 3], b: [f64; 3]| (0..3).all(|k| (a[k] - b[k]).abs() < 1e-6);
            assert!(
                near(lo, [2000.0, -200.0, 1000.0]) && near(hi, [3000.0, 200.0, 2000.0]),
                "{lo:?} {hi:?}"
            );
        }
    }

    #[test]
    fn a_family_box_at_its_transform() {
        // A 2 x 2 x 9 box turned a quarter about z and moved.
        let t = [
            0.0, -1.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 10.0, 20.0, 5.0,
        ];
        let local = [-1.0, -1.0, 0.0, 1.0, 1.0, 9.0];
        assert_eq!(placed_bounds(local, &t), [9.0, 19.0, 5.0, 11.0, 21.0, 14.0]);
        let (frame, size) = placed_box(local, &t);
        assert_eq!(
            frame.origin,
            [11.0 * FOOT_MM, 19.0 * FOOT_MM, 5.0 * FOOT_MM]
        );
        assert_eq!(frame.x_axis, [0.0, 1.0, 0.0]);
        assert_eq!(size, [2.0 * FOOT_MM, 2.0 * FOOT_MM, 9.0 * FOOT_MM]);
        // Mirrored: the box spans the other way along y.
        let m = [1.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0];
        let (frame, _) = placed_box([0.0, 0.0, 0.0, 1.0, 2.0, 3.0], &m);
        assert_eq!(frame.y_axis, [0.0, 1.0, 0.0]);
        assert_eq!(frame.origin, [0.0, -2.0 * FOOT_MM, 0.0]);
    }

    #[test]
    fn the_building_project_comes_in_as_features() {
        use mitcad_rvt::testfile::building as t;
        let read = || {
            let file = RvtFile::parse(t::project()).unwrap();
            let schema = file.schema().unwrap();
            Model::read(&file, &schema).unwrap()
        };
        let model = read();
        let kernel = MockKernel::default();
        kernel.measures.set(true);
        let mut document = Document::new(kernel);
        // The linked model: the same project, read again.
        let linked = LinkedModel {
            link: model.building.links[0].clone(),
            file: t::LINKED.into(),
            model: read(),
        };
        let options = RvtOptions {
            links: vec![linked],
            ..RvtOptions::default()
        };
        let report = import_rvt(&mut document, "building.rvt", &model, &options).unwrap();
        // The walls and the floor as features, checked against their stored
        // boxes (the mock kernel measures boxes).
        let line = |id| {
            report
                .elements
                .iter()
                .find(|l| l.id == id && l.link.is_none())
                .unwrap()
        };
        for id in [t::WALL_A, t::WALL_B, t::FLOOR] {
            let l = line(id);
            assert_eq!(l.outcome, "features", "{l:?}");
            assert_eq!(l.checked_against.as_deref(), Some("stored box"));
        }
        assert_eq!(line(t::WALL_A).notes, vec!["its start extends"]);
        assert_eq!(line(t::WALL_B).notes, vec!["its start stops"]);
        // Wall A runs on to wall B's far face, wall B stops at A's near face.
        let bodies = document.bodies();
        let body = |name: &str| {
            bodies
                .iter()
                .find(|b| b.name.starts_with(name))
                .unwrap()
                .shape
        };
        let kernel = document.kernel();
        let a = kernel
            .bounding_box(body("Basic Wall:Brick 6:30"))
            .unwrap()
            .unwrap();
        assert!((a.min[0] + 0.25 * FOOT_MM).abs() < 1e-6, "{a:?}");
        let b = kernel
            .bounding_box(body("Basic Wall:Brick 6:31"))
            .unwrap()
            .unwrap();
        assert!((b.min[1] - 0.25 * FOOT_MM).abs() < 1e-6, "{b:?}");
        assert_eq!(report.built["wall"], 4, "the project's and the link's");
        // The levels' planes, the grid's axis, the material's appearance.
        assert_eq!(report.level_planes, 4);
        assert_eq!(report.grid_axes, 2);
        assert_eq!(report.appearances, 2);
        // The link's component, at its transform, holds its model's levels.
        assert_eq!(report.links.len(), 1);
        assert_eq!(report.links[0].found.as_deref(), Some(t::LINKED));
        assert!(report.links[0].elements >= 3, "{:?}", report.links[0]);
        assert!(
            report.links[0]
                .note
                .as_deref()
                .unwrap()
                .contains("not followed")
        );
        assert!(report.levels.iter().any(|l| l.link == Some(0)));
        // A link not found: its component stays empty, the report names it.
        let mut document = Document::new(MockKernel::default());
        let missing = RvtOptions {
            missing_links: vec![(
                model.building.links[0].clone(),
                "linked.rvt was not found".into(),
            )],
            ..RvtOptions::default()
        };
        let report = import_rvt(&mut document, "building.rvt", &model, &missing).unwrap();
        assert_eq!(report.links[0].elements, 0);
        assert!(report.links[0].component.is_some());
        assert_eq!(
            report.links[0].note.as_deref(),
            Some("linked.rvt was not found")
        );
        // Without measures the features cannot be checked: the walls come in
        // as boxes, with the reason.
        assert_eq!(report.fallbacks["wall"], 2);
        let l = report.elements.iter().find(|l| l.id == t::WALL_A).unwrap();
        assert_eq!(l.outcome, "box");
        assert!(
            l.notes.iter().any(|n| n.contains("encloses no volume")),
            "{l:?}"
        );
        // Boxes are hidden but for structural elements' (mitcad#333): the
        // wall's box shows, the chair's (furniture) does not, and showing
        // it makes it visible.
        let body_of = |name: &str| {
            document
                .bodies()
                .into_iter()
                .find(|b| b.name.starts_with(name))
                .unwrap()
                .uid
        };
        let wall = body_of("Basic Wall:Brick 6:30");
        assert!(document.body_attributes(wall).visible);
        assert!(!l.hidden);
        let chair = report.elements.iter().find(|l| l.id == t::CHAIR).unwrap();
        assert_eq!(chair.outcome, "box");
        assert!(chair.hidden, "{chair:?}");
        assert!(
            chair
                .notes
                .iter()
                .any(|n| n.contains("the file keeps no geometry")),
            "{chair:?}"
        );
        assert_eq!(report.hidden_boxes, 1);
        let chair = body_of(&format!("FamilyInstance-{}", t::CHAIR));
        assert!(!document.body_attributes(chair).visible);
        document.set_body_visible(chair, true).unwrap();
        assert!(document.body_attributes(chair).visible);
    }

    #[test]
    fn a_multistory_stair_s_supports_come_in_on_each_storey() {
        // (mitcad#391) A stair (1) on level 10 repeated on level 11, 10 ft
        // up; its railing (3), the railing's rail (4, a path of its own)
        // and the rail's support (5) and an earlier support (6) the rail
        // does not list. The stair's geometry places the support (an
        // element of its own) on both storeys: the stair has no body of its
        // own, the support comes in where it is and 10 ft up, the earlier
        // support not at all.
        use mitcad_rvt::brep::{GRep, Placed};
        let level = |id, elevation| mitcad_rvt::elements::Level {
            id,
            name: format!("L{id}"),
            elevation,
            building_story: None,
        };
        let support = |id| Element {
            id,
            partition: Some(1),
            class: Some("FamilyInstance".into()),
            category: Some(-2_000_948),
            level: Some(10),
            bbox: Some([0.0, 0.0, 0.0, 0.1, 0.1, 3.0]),
            ..Element::default()
        };
        let mut model = Model {
            levels: vec![level(10, 0.0), level(11, 10.0)],
            elements: vec![
                Element {
                    id: 1,
                    partition: Some(1),
                    class: Some("StairsElement".into()),
                    category: Some(-2_000_120),
                    level: Some(10),
                    bbox: Some([0.0, 0.0, 0.0, 1.0, 1.0, 13.0]),
                    ..Element::default()
                },
                support(5),
                support(6),
            ],
            ..Model::default()
        };
        let mut t = IDENTITY;
        let placed = |transform| Placed {
            symbol: Some(5),
            transform,
            embedded: None,
        };
        let here = placed(t);
        t[11] = 10.0;
        model.geometry.insert(
            1,
            GRep {
                instances: vec![here, placed(t)],
                ..GRep::default()
            },
        );
        let b = &mut model.building;
        b.multistory.insert(
            1,
            mitcad_rvt::building::Multistory {
                base_level: 10,
                levels: vec![11],
            },
        );
        for (e, owner) in [(3, 1), (4, 3), (5, 4), (6, 4)] {
            b.owners.insert(e, owner);
        }
        b.sweeps.insert(
            4,
            mitcad_rvt::building::Sweep {
                id: 4,
                class: "HandRail".into(),
                paths: Vec::new(),
                sides: Vec::new(),
                path3d: Some(7),
                profile: None,
                type_id: None,
                up: None,
                offsets: [0.0, 0.0],
                flipped: false,
                host: None,
                cut_by: Vec::new(),
                returns: Vec::new(),
                joined: Vec::new(),
                supports: vec![5],
            },
        );
        let mut document = Document::new(MockKernel::default());
        let report =
            import_rvt(&mut document, "project.rvt", &model, &RvtOptions::default()).unwrap();
        assert_eq!((report.stairs, report.multistory_copies), (1, 1));
        assert_eq!(
            report
                .left_out
                .get("a rail's support the rail does not list (nothing places it)"),
            Some(&1)
        );
        let lines: Vec<(i64, &str, &str, Option<&str>)> = report
            .elements
            .iter()
            .map(|l| {
                (
                    l.id,
                    l.name.as_str(),
                    l.outcome.as_str(),
                    l.level.as_deref(),
                )
            })
            .collect();
        assert_eq!(
            lines,
            vec![
                (1, "StairsElement-1", "parts", Some("L10")),
                (5, "FamilyInstance-5", "box", Some("L10")),
                (5, "FamilyInstance-5 (on L11)", "box", Some("L11")),
            ]
        );
    }
}
