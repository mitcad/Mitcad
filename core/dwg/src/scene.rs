// SPDX-License-Identifier: MIT
//! A drawing's 3D content for the import: its blocks (model space too)
//! with their bodies (B-rep records), meshes (3D faces, polyface and
//! polygon meshes, subdivision meshes) and inserts, the account of every
//! entity, and the [`Plan`] of the components the import makes.
//!
//! - The 3D faces of a block on one layer in one colour make one mesh
//!   body; each polyface, polygon and subdivision mesh is one.
//! - A named block becomes a component, placed by each rigid insert of it
//!   (an array insert places it once per cell); its nested blocks keep
//!   their nesting. Anonymous blocks, and blocks placed by an insert that
//!   scales or mirrors (an occurrence is a rigid placement), are copied
//!   into the component that inserts them, moved by the insert.
//! - Coordinates are drawing units; the plan's translations and the
//!   import's bodies are millimetres (`unit_mm`).

use std::collections::{BTreeMap, BTreeSet};

use mitcad_dxf::Units;
use mitcad_vec3::{cross, unit_div};

use crate::objects::{Color, EntityData, Insert, Kind, types};
use crate::solid::{self, Form};
use crate::{DwgFile, Version};

/// An affine map `p' = m p + t` (rows of `m`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Affine {
    pub m: [[f64; 3]; 3],
    pub t: [f64; 3],
}

impl Affine {
    pub const IDENTITY: Affine = Affine {
        m: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        t: [0.0; 3],
    };

    pub fn apply(&self, p: [f64; 3]) -> [f64; 3] {
        let m = &self.m;
        [
            m[0][0] * p[0] + m[0][1] * p[1] + m[0][2] * p[2] + self.t[0],
            m[1][0] * p[0] + m[1][1] * p[1] + m[1][2] * p[2] + self.t[1],
            m[2][0] * p[0] + m[2][1] * p[1] + m[2][2] * p[2] + self.t[2],
        ]
    }

    /// `self` after `first`: p -> self(first(p)).
    pub fn after(&self, first: &Affine) -> Affine {
        let mut m = [[0.0; 3]; 3];
        for (i, row) in m.iter_mut().enumerate() {
            for (j, v) in row.iter_mut().enumerate() {
                *v = (0..3).map(|k| self.m[i][k] * first.m[k][j]).sum();
            }
        }
        let t0 = self.apply(first.t);
        Affine { m, t: t0 }
    }

    pub fn determinant(&self) -> f64 {
        let m = &self.m;
        m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
            - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
            + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
    }

    /// A rotation and a translation (no scale, no mirror).
    pub fn is_rigid(&self) -> bool {
        for i in 0..3 {
            for j in 0..3 {
                let d: f64 = (0..3).map(|k| self.m[k][i] * self.m[k][j]).sum();
                let want = if i == j { 1.0 } else { 0.0 };
                if (d - want).abs() > 1e-9 {
                    return false;
                }
            }
        }
        self.determinant() > 0.0
    }

    pub fn is_identity(&self) -> bool {
        self.m == Affine::IDENTITY.m && self.t == [0.0; 3]
    }

    /// The same map in other length units: the translation scaled.
    pub fn scaled(&self, factor: f64) -> Affine {
        Affine {
            m: self.m,
            t: [self.t[0] * factor, self.t[1] * factor, self.t[2] * factor],
        }
    }

    fn translation(t: [f64; 3]) -> Affine {
        Affine {
            t,
            ..Affine::IDENTITY
        }
    }
}

/// The object coordinate system of an extrusion direction (the arbitrary
/// axis algorithm): columns are its x, y and z axes in world coordinates.
pub fn ocs(extrusion: [f64; 3]) -> Affine {
    let Some(z) = unit_div(extrusion) else {
        return Affine::IDENTITY;
    };
    if (z[0] - 0.0).abs() < 1e-12 && (z[1] - 0.0).abs() < 1e-12 && z[2] > 0.0 {
        return Affine::IDENTITY;
    }
    let limit = 1.0 / 64.0;
    let x = if z[0].abs() < limit && z[1].abs() < limit {
        cross([0.0, 1.0, 0.0], z)
    } else {
        cross([0.0, 0.0, 1.0], z)
    };
    let x = unit_div(x).unwrap_or([1.0, 0.0, 0.0]);
    let y = cross(z, x);
    Affine {
        m: [[x[0], y[0], z[0]], [x[1], y[1], z[1]], [x[2], y[2], z[2]]],
        t: [0.0; 3],
    }
}

/// The transforms an insert places its block with: one, or one per cell
/// of an array, row by row. `base` is the block's base point. Made as they
/// are taken: an array may have 32767 × 32767 cells (mitcad#931).
pub fn insert_transforms(insert: &Insert, base: [f64; 3]) -> impl Iterator<Item = Affine> {
    let (s, c) = insert.rotation.sin_cos();
    let rotate = Affine {
        m: [[c, -s, 0.0], [s, c, 0.0], [0.0, 0.0, 1.0]],
        t: [0.0; 3],
    };
    let scale = Affine {
        m: [
            [insert.scale[0], 0.0, 0.0],
            [0.0, insert.scale[1], 0.0],
            [0.0, 0.0, insert.scale[2]],
        ],
        t: [0.0; 3],
    };
    let to_base = Affine::translation([-base[0], -base[1], -base[2]]);
    let local = rotate.after(&scale).after(&to_base);
    let ocs = ocs(insert.extrusion);
    let p = insert.point;
    let (column_spacing, row_spacing) = (insert.column_spacing, insert.row_spacing);
    let columns = insert.columns.max(1);
    (0..insert.rows.max(1)).flat_map(move |row| {
        (0..columns).map(move |col| {
            let cell = rotate.apply([
                f64::from(col) * column_spacing,
                f64::from(row) * row_spacing,
                0.0,
            ]);
            let at = Affine::translation([p[0] + cell[0], p[1] + cell[1], p[2] + cell[2]]);
            ocs.after(&at.after(&local))
        })
    })
}

/// A layer as the import uses it.
#[derive(Clone, Debug, PartialEq)]
pub struct LayerInfo {
    pub name: String,
    pub color: Option<[u8; 3]>,
    pub frozen: bool,
    pub off: bool,
}

/// What a block holds.
#[derive(Clone, Debug, PartialEq)]
pub enum ItemKind {
    /// A B-rep record and the entity type that holds it.
    Brep {
        form: Form,
        bytes: Vec<u8>,
        entity: String,
    },
    /// Triangles (indices into `vertices`), in drawing units.
    Mesh {
        vertices: Vec<[f64; 3]>,
        triangles: Vec<[u32; 3]>,
        entity: String,
        /// The entities it was made of (3D faces merge).
        entities: usize,
    },
    Insert {
        insert: Insert,
        entity: String,
    },
}

/// An item of a block.
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    /// The entity's handle (the first one for merged 3D faces).
    pub handle: u64,
    pub layer: Option<u64>,
    pub color: Color,
    pub kind: ItemKind,
}

/// A block (model space too).
#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    pub handle: u64,
    pub name: String,
    pub base: [f64; 3],
    pub anonymous: bool,
    pub items: Vec<Item>,
}

/// The account of a drawing's entities.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
pub struct Account {
    /// Entities by type, of model space and the blocks.
    pub entities: BTreeMap<String, usize>,
    /// Entities taken as bodies, meshes or inserts, by type.
    pub taken: BTreeMap<String, usize>,
    /// Entities left out, by type, with why.
    pub left_out: BTreeMap<String, (usize, String)>,
    /// Entities of paper space (not imported).
    pub paper_space: usize,
    pub warnings: Vec<String>,
}

impl Account {
    fn left_out(&mut self, entity: &str, why: &str) {
        let e = self
            .left_out
            .entry(entity.to_owned())
            .or_insert((0, why.to_owned()));
        e.0 += 1;
    }

    fn take(&mut self, entity: &str) {
        *self.taken.entry(entity.to_owned()).or_insert(0) += 1;
    }
}

/// The drawing's 3D content.
#[derive(Clone, Debug)]
pub struct Scene {
    pub version: Version,
    pub units: Units,
    /// Millimetres per drawing unit (1 when the drawing names no unit).
    pub unit_mm: f64,
    /// The drawing names no unit: millimetres were assumed.
    pub units_assumed: bool,
    /// The model space's `$EXTMIN` and `$EXTMAX`, drawing units.
    pub extents: Option<([f64; 3], [f64; 3])>,
    pub layers: BTreeMap<u64, LayerInfo>,
    pub blocks: BTreeMap<u64, Block>,
    pub model_space: u64,
    pub account: Account,
    /// The blocks the paper space's own inserts insert (paper space is not
    /// imported; the plan's warnings say so of their blocks).
    pub paper_inserts: BTreeSet<u64>,
}

/// 2D entities: drawn, not modelled.
const DRAWN: &[&str] = &[
    "LINE",
    "ARC",
    "CIRCLE",
    "ELLIPSE",
    "SPLINE",
    "LWPOLYLINE",
    "POINT",
    "TEXT",
    "MTEXT",
    "ATTRIB",
    "ATTDEF",
    "DIMENSION",
    "LEADER",
    "MLEADER",
    "HATCH",
    "SOLID",
    "TRACE",
    "SHAPE",
    "RAY",
    "XLINE",
    "TOLERANCE",
    "MLINE",
    "VIEWPORT",
    "IMAGE",
    "WIPEOUT",
];

fn triangulate(polygon: &[u32], out: &mut Vec<[u32; 3]>) {
    for k in 1..polygon.len().saturating_sub(1) {
        out.push([polygon[0], polygon[k], polygon[k + 1]]);
    }
}

impl Scene {
    pub fn of(file: &DwgFile) -> Scene {
        let mut account = Account {
            warnings: file.warnings.clone(),
            ..Default::default()
        };
        let header = file.header.clone().unwrap_or_default();
        let units = header
            .insunits
            .and_then(|c| Units::from_code(i32::from(c)))
            .unwrap_or_default();
        let (unit_mm, units_assumed) = match units.millimeters() {
            Some(mm) => (mm, false),
            None => (1.0, true),
        };
        if units_assumed {
            account
                .warnings
                .push("the drawing names no length unit: millimetres assumed".into());
        }
        // Extents never computed are stored empty (minimum 1e20, maximum
        // -1e20).
        let extents = header
            .extmin
            .zip(header.extmax)
            .filter(|(a, b)| (0..3).all(|k| a[k] <= b[k]));
        if header.extmin.is_some() && extents.is_none() {
            account
                .warnings
                .push("the drawing's extents ($EXTMIN, $EXTMAX) are not set".into());
        }

        let mut layers = BTreeMap::new();
        let mut blocks = BTreeMap::new();
        let mut model_space = None;
        for o in file.objects.values() {
            match &o.kind {
                Kind::Layer(l) => {
                    layers.insert(
                        o.handle,
                        LayerInfo {
                            name: l.name.clone(),
                            color: l.color.rgb(),
                            frozen: l.frozen,
                            off: l.off,
                        },
                    );
                }
                Kind::BlockHeader(b) => {
                    if b.name.eq_ignore_ascii_case("*Model_Space") {
                        model_space = Some(o.handle);
                    }
                    blocks.insert(
                        o.handle,
                        Block {
                            handle: o.handle,
                            name: b.name.clone(),
                            base: b.base,
                            anonymous: b.name.starts_with('*'),
                            items: Vec::new(),
                        },
                    );
                }
                _ => {}
            }
        }
        let model_space = model_space.unwrap_or_else(|| {
            account.warnings.push(
                "the drawing has no model space block: its entities are taken as model space"
                    .into(),
            );
            0
        });
        blocks.entry(model_space).or_insert_with(|| Block {
            handle: model_space,
            name: "*Model_Space".into(),
            base: [0.0; 3],
            anonymous: true,
            items: Vec::new(),
        });

        // The vertices of each polyline, by owner.
        let mut owned: BTreeMap<u64, Vec<u64>> = BTreeMap::new();
        for o in file.objects.values() {
            if let Kind::Entity(c, _) = &o.kind
                && c.entmode == 0
                && let Some(owner) = c.owner
                && matches!(
                    o.type_code,
                    types::VERTEX_MESH
                        | types::VERTEX_PFACE
                        | types::VERTEX_PFACE_FACE
                        | types::SEQEND
                        | types::VERTEX_2D
                        | types::VERTEX_3D
                )
            {
                owned.entry(owner).or_default().push(o.handle);
            }
        }
        // 3D faces merge by block, layer and colour.
        let mut faces: BTreeMap<(u64, Option<u64>, String), MergedFaces> = BTreeMap::new();
        let mut paper_inserts = BTreeSet::new();

        for o in file.objects.values() {
            let Kind::Entity(common, data) = &o.kind else {
                continue;
            };
            let name = o.type_name.as_str();
            // Parts of polylines and inserts are accounted with them.
            if matches!(
                o.type_code,
                types::VERTEX_2D
                    | types::VERTEX_3D
                    | types::VERTEX_MESH
                    | types::VERTEX_PFACE
                    | types::VERTEX_PFACE_FACE
                    | types::SEQEND
                    | types::BLOCK
                    | types::ENDBLK
            ) || (name == "ATTRIB" && common.entmode == 0)
            {
                continue;
            }
            let block = match common.entmode {
                2 => model_space,
                1 => {
                    account.paper_space += 1;
                    if let EntityData::Insert(insert) = data {
                        paper_inserts.insert(insert.block);
                    }
                    continue;
                }
                _ => match common.owner {
                    Some(owner) if blocks.contains_key(&owner) => owner,
                    _ => {
                        account.left_out(name, "its owner is not a block");
                        continue;
                    }
                },
            };
            *account.entities.entry(name.to_owned()).or_insert(0) += 1;
            let item = |kind| Item {
                handle: o.handle,
                layer: common.layer,
                color: common.color,
                kind,
            };
            let new = match data {
                EntityData::Brep(b) => match solid::record(b, file.stored_breps.get(&o.handle)) {
                    Some((form, bytes)) => Some(item(ItemKind::Brep {
                        form,
                        bytes,
                        entity: name.to_owned(),
                    })),
                    None => {
                        if let crate::objects::BrepData::Unreadable(why) = b {
                            account.warnings.push(format!(
                                "{name} {:X}: its B-rep data could not be read ({why})",
                                o.handle
                            ));
                        }
                        account.left_out(name, "no B-rep data");
                        None
                    }
                },
                EntityData::Face3d { corners, .. } => {
                    let key = (block, common.layer, format!("{:?}", common.color));
                    let e = faces
                        .entry(key)
                        .or_insert((o.handle, Vec::new(), Vec::new(), 0));
                    let first = e.1.len() as u32;
                    let quad = corners[3] != corners[2];
                    e.1.extend_from_slice(&corners[..if quad { 4 } else { 3 }]);
                    e.2.push([first, first + 1, first + 2]);
                    if quad {
                        e.2.push([first, first + 2, first + 3]);
                    }
                    e.3 += 1;
                    account.take(name);
                    None
                }
                EntityData::PolyfaceMesh {
                    owned: list,
                    first_last,
                    ..
                } => match polyface(file, o.handle, list, *first_last, &owned) {
                    Ok((vertices, triangles)) => Some(item(ItemKind::Mesh {
                        vertices,
                        triangles,
                        entity: "POLYLINE (polyface mesh)".into(),
                        entities: 1,
                    })),
                    Err(e) => {
                        account.left_out("POLYLINE (polyface mesh)", &e);
                        None
                    }
                },
                EntityData::PolygonMesh {
                    flags,
                    m,
                    n,
                    owned: list,
                    first_last,
                } => {
                    match polygon_mesh(file, o.handle, *flags, *m, *n, list, *first_last, &owned) {
                        Ok((vertices, triangles)) => Some(item(ItemKind::Mesh {
                            vertices,
                            triangles,
                            entity: "POLYLINE (polygon mesh)".into(),
                            entities: 1,
                        })),
                        Err(e) => {
                            account.left_out("POLYLINE (polygon mesh)", &e);
                            None
                        }
                    }
                }
                EntityData::Mesh {
                    vertices, faces, ..
                } => {
                    let mut triangles = Vec::new();
                    for f in faces {
                        triangulate(f, &mut triangles);
                    }
                    Some(item(ItemKind::Mesh {
                        vertices: vertices.clone(),
                        triangles,
                        entity: name.to_owned(),
                        entities: 1,
                    }))
                }
                EntityData::Insert(insert) => {
                    if blocks.contains_key(&insert.block) {
                        Some(item(ItemKind::Insert {
                            insert: insert.clone(),
                            entity: name.to_owned(),
                        }))
                    } else {
                        account.left_out(name, "its block is not in the drawing");
                        None
                    }
                }
                _ => {
                    let why = if DRAWN.contains(&name) {
                        "a 2D entity (drawn, not modelled)"
                    } else if name.starts_with("POLYLINE") {
                        "a 2D or 3D polyline (a wire)"
                    } else if o.type_name.ends_with("SURFACE") {
                        "a surface without B-rep data"
                    } else {
                        "not converted"
                    };
                    let label = match o.type_code {
                        types::POLYLINE_2D => "POLYLINE (2D)",
                        types::POLYLINE_3D => "POLYLINE (3D)",
                        _ => name,
                    };
                    account.left_out(label, why);
                    None
                }
            };
            if let Some(new) = new {
                match &new.kind {
                    ItemKind::Brep { .. } | ItemKind::Insert { .. } => account.take(name),
                    ItemKind::Mesh { entity, .. } => account.take(entity),
                }
                if let Some(b) = blocks.get_mut(&block) {
                    b.items.push(new);
                }
            }
        }
        for ((block, layer, _), (handle, vertices, triangles, count)) in faces {
            let color = file
                .objects
                .get(&handle)
                .and_then(|o| match &o.kind {
                    Kind::Entity(c, _) => Some(c.color),
                    _ => None,
                })
                .unwrap_or_default();
            if let Some(b) = blocks.get_mut(&block) {
                b.items.push(Item {
                    handle,
                    layer,
                    color,
                    kind: ItemKind::Mesh {
                        vertices,
                        triangles,
                        entity: "3DFACE".into(),
                        entities: count,
                    },
                });
            }
        }
        for b in blocks.values_mut() {
            b.items.sort_by_key(|i| i.handle);
        }
        for (handle, kind, why) in &file.failed {
            let kind = if kind.is_empty() { "an object" } else { kind };
            account
                .warnings
                .push(format!("{kind} {handle:X} could not be read: {why}"));
        }
        Scene {
            version: file.version,
            units,
            unit_mm,
            units_assumed,
            extents,
            layers,
            blocks,
            model_space,
            account,
            paper_inserts,
        }
    }

    /// The layer name of an item ("0" when unknown).
    pub fn layer_name(&self, layer: Option<u64>) -> &str {
        layer
            .and_then(|l| self.layers.get(&l))
            .map_or("0", |l| l.name.as_str())
    }

    /// The colour an item shows in: its own, else its layer's; None for
    /// "by block" (the inserting entity's), resolved by the plan.
    pub fn item_color(&self, item: &Item) -> Option<[u8; 3]> {
        match item.color {
            Color::ByLayer => item
                .layer
                .and_then(|l| self.layers.get(&l))
                .and_then(|l| l.color),
            c => c.rgb(),
        }
    }

    /// Whether a block holds bodies or meshes, itself or through its
    /// inserts.
    pub fn has_content(&self, block: u64) -> bool {
        let mut seen = BTreeSet::new();
        self.content(block, &mut seen)
    }

    fn content(&self, block: u64, seen: &mut BTreeSet<u64>) -> bool {
        if !seen.insert(block) {
            return false;
        }
        let Some(b) = self.blocks.get(&block) else {
            return false;
        };
        b.items.iter().any(|i| match &i.kind {
            ItemKind::Insert { insert, .. } => self.content(insert.block, seen),
            _ => true,
        })
    }

    /// The plan of components, within [`PLAN_BUDGET`].
    pub fn plan(&self) -> Plan {
        self.plan_within(PLAN_BUDGET)
    }

    /// The plan of components, placing at most `limit` bodies and
    /// occurrences in the world (an occurrence's contents count once per
    /// occurrence of it, at every level); what is left over is not planned,
    /// with a warning.
    pub fn plan_within(&self, limit: u64) -> Plan {
        let mut plan = Plan {
            nodes: vec![Node {
                name: String::new(),
                block: Some(self.model_space),
                bodies: Vec::new(),
                children: Vec::new(),
            }],
            flattened: 0,
            warnings: Vec::new(),
        };
        let mut by_block: BTreeMap<u64, usize> = BTreeMap::new();
        let mut stack = vec![self.model_space];
        let mut budget = Budget {
            total: limit,
            left: limit,
            per_occurrence: BTreeMap::new(),
            exhausted: false,
        };
        self.fill(
            &mut plan,
            0,
            self.model_space,
            Affine::IDENTITY,
            None,
            &mut by_block,
            &mut stack,
            &mut budget,
            1,
        );
        if budget.exhausted {
            plan.warnings.push(format!(
                "the drawing places more than {} bodies and occurrences (counting each occurrence's contents): the rest are not imported",
                budget.total
            ));
        }
        // Blocks with bodies that model space does not insert: definitions
        // only, or a layout's content (paper space is not imported).
        let paper = |b: &Block| b.name.to_ascii_lowercase().starts_with("*paper_space");
        let reached = self.inserted_from(vec![self.model_space]);
        let in_paper = self.inserted_from(
            self.blocks
                .values()
                .filter(|b| paper(b))
                .map(|b| b.handle)
                .chain(self.paper_inserts.iter().copied())
                .collect(),
        );
        for b in self.blocks.values() {
            let bodies = b
                .items
                .iter()
                .filter(|i| !matches!(i.kind, ItemKind::Insert { .. }))
                .count();
            if bodies > 0 && !reached.contains(&b.handle) && !paper(b) {
                let where_ = if in_paper.contains(&b.handle) {
                    "inserted only in paper space (a layout)"
                } else {
                    "inserted nowhere"
                };
                plan.warnings.push(format!(
                    "the block {} ({bodies} bodies or meshes) is {where_}: not imported",
                    b.name
                ));
            }
        }
        plan
    }

    /// The blocks `from` and those their inserts insert, at any depth.
    fn inserted_from(&self, mut todo: Vec<u64>) -> BTreeSet<u64> {
        let mut reached = BTreeSet::new();
        while let Some(b) = todo.pop() {
            if !reached.insert(b) {
                continue;
            }
            for item in self.blocks.get(&b).map_or(&[][..], |b| &b.items) {
                if let ItemKind::Insert { insert, .. } = &item.kind {
                    todo.push(insert.block);
                }
            }
        }
        reached
    }

    /// Adds a block's items to node `node`, moved by `at` (identity unless
    /// the block is copied into the node). `copies` is how many times the
    /// node is placed in the world so far (what each item added here costs
    /// of the budget). Returns the instances the items place per
    /// occurrence of the node.
    #[allow(clippy::too_many_arguments)]
    fn fill(
        &self,
        plan: &mut Plan,
        node: usize,
        block: u64,
        at: Affine,
        by_block_color: Option<[u8; 3]>,
        by_block: &mut BTreeMap<u64, usize>,
        stack: &mut Vec<u64>,
        budget: &mut Budget,
        copies: u64,
    ) -> u64 {
        let Some(b) = self.blocks.get(&block) else {
            return 0;
        };
        let mut placed_here = 0u64;
        for (index, item) in b.items.iter().enumerate() {
            if budget.exhausted {
                break;
            }
            let color = match item.color {
                Color::ByBlock => by_block_color,
                _ => self.item_color(item),
            };
            let ItemKind::Insert { insert, .. } = &item.kind else {
                if !budget.take(copies) {
                    break;
                }
                plan.nodes[node].bodies.push(PlacedItem {
                    block,
                    item: index,
                    transform: at,
                    color,
                    layer: item.layer,
                });
                placed_here = placed_here.saturating_add(1);
                continue;
            };
            let Some(inner) = self.blocks.get(&insert.block) else {
                continue;
            };
            if !self.has_content(insert.block) {
                continue;
            }
            if stack.contains(&insert.block) {
                plan.warnings.push(format!(
                    "the block {} inserts itself: left out there",
                    inner.name
                ));
                continue;
            }
            stack.push(insert.block);
            let mut copied_warned = false;
            for t in insert_transforms(insert, inner.base) {
                if budget.exhausted {
                    break;
                }
                let placed = at.after(&t);
                if !inner.anonymous && placed.is_rigid() {
                    let child = match by_block.get(&insert.block) {
                        Some(&c) => {
                            // Another occurrence: it places the node's
                            // contents again.
                            let inside = budget.per_occurrence.get(&c).copied().unwrap_or(0);
                            if !budget.take(copies.saturating_mul(inside.saturating_add(1))) {
                                break;
                            }
                            c
                        }
                        None => {
                            if !budget.take(copies) {
                                break;
                            }
                            let c = plan.nodes.len();
                            plan.nodes.push(Node {
                                name: inner.name.clone(),
                                block: Some(insert.block),
                                bodies: Vec::new(),
                                children: Vec::new(),
                            });
                            by_block.insert(insert.block, c);
                            let inside = self.fill(
                                plan,
                                c,
                                insert.block,
                                Affine::IDENTITY,
                                color,
                                by_block,
                                stack,
                                budget,
                                copies,
                            );
                            budget.per_occurrence.insert(c, inside);
                            c
                        }
                    };
                    plan.nodes[node]
                        .children
                        .push((child, placed.scaled(self.unit_mm)));
                    let inside = budget.per_occurrence.get(&child).copied().unwrap_or(0);
                    placed_here = placed_here.saturating_add(inside.saturating_add(1));
                } else {
                    // Copied into this node, moved by the insert.
                    plan.flattened += 1;
                    // Once per insert, not per cell of an array.
                    if !inner.anonymous && !copied_warned {
                        copied_warned = true;
                        plan.warnings.push(format!(
                            "an insert of {} scales or mirrors it: its contents are copied in place, not an occurrence",
                            inner.name
                        ));
                    }
                    let inside = self.fill(
                        plan,
                        node,
                        insert.block,
                        placed,
                        color,
                        by_block,
                        stack,
                        budget,
                        copies,
                    );
                    placed_here = placed_here.saturating_add(inside);
                }
            }
            stack.pop();
        }
        placed_here
    }
}

/// The most bodies and occurrences a plan places in the world (counting an
/// occurrence's contents once per occurrence): an insert array may have
/// 32767 × 32767 cells, and nested arrays multiply (mitcad#931).
pub const PLAN_BUDGET: u64 = 1_000_000;

/// What a plan may still place.
struct Budget {
    total: u64,
    left: u64,
    /// Per node: the instances one occurrence of it places (its bodies,
    /// its occurrences and their contents).
    per_occurrence: BTreeMap<usize, u64>,
    /// Something did not fit: nothing more is planned.
    exhausted: bool,
}

impl Budget {
    /// Takes `n` instances; false, taking nothing, when they do not fit.
    fn take(&mut self, n: u64) -> bool {
        if self.exhausted || n > self.left {
            self.exhausted = true;
            return false;
        }
        self.left -= n;
        true
    }
}

/// A body of a node: a block's item, moved by `transform` (drawing
/// units) within the node.
#[derive(Clone, Debug, PartialEq)]
pub struct PlacedItem {
    pub block: u64,
    pub item: usize,
    pub transform: Affine,
    pub color: Option<[u8; 3]>,
    pub layer: Option<u64>,
}

/// A component of the plan (node 0 is the root, the model space).
#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    pub name: String,
    pub block: Option<u64>,
    pub bodies: Vec<PlacedItem>,
    /// Occurrences of other nodes, with their placements in millimetres.
    pub children: Vec<(usize, Affine)>,
}

/// The components the import makes.
#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    pub nodes: Vec<Node>,
    /// Inserts whose blocks were copied in place.
    pub flattened: usize,
    pub warnings: Vec<String>,
}

impl Plan {
    /// How many times each node is placed (the root once).
    pub fn occurrences(&self) -> Vec<usize> {
        let mut count = vec![0usize; self.nodes.len()];
        count[0] = 1;
        for n in &self.nodes {
            for (c, _) in &n.children {
                count[*c] += 1;
            }
        }
        count
    }
}

/// The vertices of a polyline, in order: its list, or its chain from the
/// first to the last, or those it owns by handle.
fn vertex_handles(
    file: &DwgFile,
    polyline: u64,
    list: &[u64],
    first_last: Option<(u64, u64)>,
    owned: &BTreeMap<u64, Vec<u64>>,
) -> Vec<u64> {
    if !list.is_empty() {
        return list.to_vec();
    }
    if let Some((first, last)) = first_last {
        let mut out = vec![first];
        let mut at = first;
        while at != last && out.len() < 1_000_000 {
            let next = match file.objects.get(&at).map(|o| &o.kind) {
                Some(Kind::Entity(c, _)) => c.next,
                _ => None,
            };
            match next {
                Some(n) if file.objects.contains_key(&n) => {
                    out.push(n);
                    at = n;
                }
                _ => break,
            }
        }
        if out.last() == Some(&last) {
            return out;
        }
    }
    let mut v = owned.get(&polyline).cloned().unwrap_or_default();
    v.sort_unstable();
    v
}

type MeshData = (Vec<[f64; 3]>, Vec<[u32; 3]>);

/// 3D faces merged: the first one's handle, the vertices, the triangles
/// and how many faces.
type MergedFaces = (u64, Vec<[f64; 3]>, Vec<[u32; 3]>, usize);

fn polyface(
    file: &DwgFile,
    handle: u64,
    list: &[u64],
    first_last: Option<(u64, u64)>,
    owned: &BTreeMap<u64, Vec<u64>>,
) -> Result<MeshData, String> {
    let mut vertices = Vec::new();
    let mut triangles = Vec::new();
    for h in vertex_handles(file, handle, list, first_last, owned) {
        match file.objects.get(&h).map(|o| &o.kind) {
            Some(Kind::Entity(_, EntityData::MeshVertex { point, .. })) => vertices.push(*point),
            Some(Kind::Entity(_, EntityData::FaceRecord { indices })) => {
                let polygon: Vec<u32> = indices
                    .iter()
                    .filter(|&&i| i != 0)
                    .map(|&i| u32::from(i.unsigned_abs()) - 1)
                    .collect();
                triangulate(&polygon, &mut triangles);
            }
            _ => {}
        }
    }
    if triangles
        .iter()
        .flatten()
        .any(|&i| i as usize >= vertices.len())
    {
        return Err("a face refers to a vertex the mesh does not have".into());
    }
    if triangles.is_empty() {
        return Err("no faces".into());
    }
    Ok((vertices, triangles))
}

#[allow(clippy::too_many_arguments)]
fn polygon_mesh(
    file: &DwgFile,
    handle: u64,
    flags: i16,
    m: i16,
    n: i16,
    list: &[u64],
    first_last: Option<(u64, u64)>,
    owned: &BTreeMap<u64, Vec<u64>>,
) -> Result<MeshData, String> {
    let mut vertices = Vec::new();
    for h in vertex_handles(file, handle, list, first_last, owned) {
        if let Some(Kind::Entity(_, EntityData::MeshVertex { point, .. })) =
            file.objects.get(&h).map(|o| &o.kind)
        {
            vertices.push(*point);
        }
    }
    let (m, n) = (m.max(0) as usize, n.max(0) as usize);
    if m < 2 || n < 2 || vertices.len() < m * n {
        return Err(format!("a {m} x {n} mesh of {} vertices", vertices.len()));
    }
    let closed_m = flags & 1 != 0;
    let closed_n = flags & 32 != 0;
    let mut triangles = Vec::new();
    let rows = if closed_m { m } else { m - 1 };
    let cols = if closed_n { n } else { n - 1 };
    for i in 0..rows {
        for j in 0..cols {
            let a = (i * n + j) as u32;
            let b = (i * n + (j + 1) % n) as u32;
            let c = (((i + 1) % m) * n + (j + 1) % n) as u32;
            let d = (((i + 1) % m) * n + j) as u32;
            triangles.push([a, b, c]);
            triangles.push([a, c, d]);
        }
    }
    Ok((vertices, triangles))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testfile;

    #[test]
    fn the_test_drawing_as_a_scene_and_a_plan() {
        for v in Version::ALL {
            let file = crate::read(&testfile::write(&testfile::test_drawing(), v)).unwrap();
            let scene = Scene::of(&file);
            assert_eq!(scene.unit_mm, 1.0, "{v}");
            assert_eq!(scene.units_assumed, v < Version::R2000, "{v}");
            let a = &scene.account;
            assert_eq!(a.taken["3DSOLID"], 2, "{v}");
            assert_eq!(a.taken["3DFACE"], 2, "{v}");
            assert_eq!(a.taken["POLYLINE (polyface mesh)"], 1, "{v}");
            assert_eq!(a.taken["MESH"], 1, "{v}");
            assert_eq!(a.left_out["LINE"].0, 1, "{v}");
            let plan = scene.plan();
            // The root, Segment and Brace.
            assert_eq!(plan.nodes.len(), 3, "{v}");
            let names: Vec<&str> = plan.nodes.iter().map(|n| n.name.as_str()).collect();
            assert_eq!(names, ["", "Segment", "Brace"]);
            // Segment twice, Brace twice in each Segment and six times in
            // the array (rigid); the scaled insert is copied.
            assert_eq!(plan.occurrences(), [1, 2, 8], "{v}");
            assert_eq!(plan.flattened, 1);
            // The root: the 3D faces (one mesh), the polyface mesh, the
            // subdivision mesh and the scaled brace's cube.
            assert_eq!(plan.nodes[0].bodies.len(), 4, "{v}");
            assert_eq!(plan.nodes[1].bodies.len(), 1);
            assert_eq!(plan.nodes[2].bodies.len(), 1);
            let scaled = plan.nodes[0]
                .bodies
                .iter()
                .find(|b| !b.transform.is_identity())
                .unwrap();
            assert_eq!(scaled.transform.apply([1.0, 1.0, 1.0]), [72.0, 2.0, 2.0]);
            // The second Segment turned a quarter.
            let (_, t) = plan.nodes[0].children[1];
            let p = t.apply([1.0, 0.0, 0.0]);
            assert!(
                (p[0] - 0.0).abs() < 1e-12 && (p[1] - 21.0).abs() < 1e-12,
                "{p:?}"
            );
            // The brace's colour comes from its layer (red), the chord's
            // is its own (green).
            assert_eq!(plan.nodes[2].bodies[0].color, Some([255, 0, 0]));
            assert_eq!(plan.nodes[1].bodies[0].color, Some([0, 255, 0]));
        }
    }

    #[test]
    fn the_solids_parse_and_convert_in_every_form() {
        for v in Version::ALL {
            let file = crate::read(&testfile::write(&testfile::test_drawing(), v)).unwrap();
            let scene = Scene::of(&file);
            let mut faces = Vec::new();
            let mut forms = Vec::new();
            for b in scene.blocks.values() {
                for item in &b.items {
                    if let ItemKind::Brep { form, bytes, .. } = &item.kind {
                        let asm = solid::parse(*form, bytes).unwrap_or_else(|e| panic!("{v}: {e}"));
                        let options = mitcad_asm::convert::Options {
                            scale: scene.unit_mm,
                            ..Default::default()
                        };
                        let bodies = mitcad_asm::convert::convert_file(&asm, &options);
                        assert_eq!(bodies.len(), 1, "{v}");
                        assert!(bodies[0].issues.is_empty(), "{v}: {:?}", bodies[0].issues);
                        faces.push(bodies[0].body.faces.len());
                        forms.push(*form);
                    }
                }
            }
            faces.sort_unstable();
            assert_eq!(faces, [3, 6], "{v}");
            if v >= Version::R2013 {
                assert_eq!(forms, [Form::Stored, Form::Stored]);
            } else {
                assert!(
                    forms.contains(&Form::Text) && forms.contains(&Form::Binary),
                    "{v}"
                );
            }
        }
    }

    #[test]
    fn blocks_model_space_does_not_insert_are_named_in_the_warnings() {
        // Two copies of the Brace block: one inserted by paper space's own
        // insert, one by nothing (a public sample drawing's 3D content was
        // inserted only from its layouts, which the warning called
        // "inserted nowhere").
        let file =
            crate::read(&testfile::write(&testfile::test_drawing(), Version::R2018)).unwrap();
        let mut scene = Scene::of(&file);
        let brace = scene.blocks[&testfile::BRACE].clone();
        for (handle, name) in [(0x501, "Sheet"), (0x502, "Spare")] {
            scene.blocks.insert(
                handle,
                Block {
                    handle,
                    name: name.into(),
                    ..brace.clone()
                },
            );
        }
        scene.paper_inserts.insert(0x501);
        let warnings = scene.plan().warnings;
        assert!(
            warnings.contains(&"the block Sheet (1 bodies or meshes) is inserted only in paper space (a layout): not imported".to_owned()),
            "{warnings:?}"
        );
        assert!(
            warnings.contains(
                &"the block Spare (1 bodies or meshes) is inserted nowhere: not imported"
                    .to_owned()
            ),
            "{warnings:?}"
        );
    }

    /// The bodies and occurrences a plan places in the world: each node's
    /// contents once per occurrence of it.
    fn world_instances(plan: &Plan, node: usize) -> u64 {
        let n = &plan.nodes[node];
        n.bodies.len() as u64
            + n.children
                .iter()
                .map(|(c, _)| 1 + world_instances(plan, *c))
                .sum::<u64>()
    }

    /// The test drawing's scene, each insert of `block` in `owner` made an
    /// array of `columns` × `rows` cells.
    fn with_arrays(scene: &mut Scene, owner: u64, block: u64, columns: i16, rows: i16) {
        for item in &mut scene.blocks.get_mut(&owner).unwrap().items {
            if let ItemKind::Insert { insert, .. } = &mut item.kind
                && insert.block == block
            {
                insert.columns = columns;
                insert.rows = rows;
            }
        }
    }

    #[test]
    fn the_budget_counts_each_occurrence_s_contents() {
        let file =
            crate::read(&testfile::write(&testfile::test_drawing(), Version::R2018)).unwrap();
        let scene = Scene::of(&file);
        // Root: 4 bodies; Segment twice (each its chord and two braces);
        // Brace six times in the array: 4 + 2 × (1 + 5) + 6 × (1 + 1).
        let plan = scene.plan();
        assert_eq!(world_instances(&plan, 0), 28);
        assert_eq!(scene.plan_within(28), plan);
        let short = scene.plan_within(27);
        assert!(
            short
                .warnings
                .iter()
                .any(|w| w.contains("more than 27 bodies and occurrences")),
            "{:?}",
            short.warnings
        );
        assert!(world_instances(&short, 0) <= 27);
    }

    #[test]
    fn an_insert_of_10000_by_10000_cells_plans_within_the_budget() {
        // Each cell's transform was made up front and planned (mitcad#931):
        // 10^8 placements.
        let file =
            crate::read(&testfile::write(&testfile::test_drawing(), Version::R2018)).unwrap();
        let mut scene = Scene::of(&file);
        let model = scene.model_space;
        with_arrays(&mut scene, model, testfile::BRACE, 10_000, 10_000);
        let plan = scene.plan();
        let entries: usize = plan
            .nodes
            .iter()
            .map(|n| n.bodies.len() + n.children.len())
            .sum();
        assert!(entries as u64 <= PLAN_BUDGET, "{entries}");
        assert!(world_instances(&plan, 0) <= PLAN_BUDGET);
        assert!(
            plan.warnings
                .iter()
                .any(|w| w.contains("bodies and occurrences")),
            "{:?}",
            plan.warnings
        );
    }

    #[test]
    fn nested_arrays_multiply_within_the_budget() {
        // Segment's braces in arrays of 100 × 100 and the Segments too: the
        // contents multiply (2 × 10^4 × 2 × 10^4 braces); the plan's own
        // entries stay few, its placements in the world within the budget.
        let file =
            crate::read(&testfile::write(&testfile::test_drawing(), Version::R2018)).unwrap();
        let mut scene = Scene::of(&file);
        let model = scene.model_space;
        with_arrays(&mut scene, testfile::SEGMENT, testfile::BRACE, 100, 100);
        with_arrays(&mut scene, model, testfile::SEGMENT, 100, 100);
        let plan = scene.plan_within(100_000);
        assert!(world_instances(&plan, 0) <= 100_000);
        assert!(
            plan.warnings
                .iter()
                .any(|w| w.contains("more than 100000 bodies and occurrences")),
            "{:?}",
            plan.warnings
        );
        // Scaled arrays are copied in place, cell by cell: bounded too.
        let mut scene = Scene::of(&file);
        for item in &mut scene.blocks.get_mut(&model).unwrap().items {
            if let ItemKind::Insert { insert, .. } = &mut item.kind {
                insert.scale = [2.0; 3];
                insert.columns = 1000;
                insert.rows = 1000;
            }
        }
        let plan = scene.plan_within(50_000);
        assert!(world_instances(&plan, 0) <= 50_000);
        assert!(plan.nodes[0].bodies.len() <= 50_000);
        assert!(!plan.warnings.is_empty());
    }

    #[test]
    fn object_coordinate_systems() {
        let t = ocs([0.0, 0.0, -1.0]);
        let x = t.apply([1.0, 0.0, 0.0]);
        assert!((x[0] + 1.0).abs() < 1e-12, "{x:?}");
        assert!(t.is_rigid());
        let t = ocs([1.0, 0.0, 0.0]);
        assert_eq!(t.apply([0.0, 0.0, 1.0]), [1.0, 0.0, 0.0]);
        assert!(t.is_rigid());
    }
}
