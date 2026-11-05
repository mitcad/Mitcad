// SPDX-License-Identifier: MIT
//! An element's items as features of one batch
//! ([`mitcad_model::FeatureBatch`]), in its component's coordinates.
//!
//! - Extrusions are a sketch of the profile and an extrusion (a sweep
//!   along a fixed line, the profile keeping its direction, when the file
//!   extrudes at a slant); revolutions a sketch and a revolve; tapered
//!   extrusions two sketches and ruled lofts (an area's outline one way,
//!   its holes cut); sweeps along a curve a sketch of the curve, one of the
//!   profile at its start and a sweep; swept disks along a planar curve a
//!   sketch and a pipe.
//! - Booleans: the first operand's bodies, then the second operand as the
//!   cut, join or intersection of a feature with them as participants, or
//!   built on its own and combined. Half spaces cut the bodies with an
//!   extruded rectangle on their plane through all on the side of the
//!   material, or with the extruded polygon of a polygonal boundary from
//!   the plane on.
//! - Breps, face sets and swept disks along a curve that is not planar are
//!   built by the kernel beforehand ([`prepare`]) and come in as base
//!   features.

use std::f64::consts::TAU;

use mitcad_ifc::geometry::{BoolOp, Item, Path, Profile, Seg};
use mitcad_ifc::math::{self, Matrix};
use mitcad_model::features::FeatureDef;
use mitcad_model::features::base::{BaseBody, BaseDef, Brep};
use mitcad_model::features::extrude::Operation;
use mitcad_model::{
    BodyUid, ComponentUid, FeatureBatch, FeatureUid, Kernel, KernelError, ModelError, ValueInput,
};
use serde_json::{Value, json};

use super::brep;
use super::sketch::{SketchBuild, path_sketch, profile_sketch};

/// A body the kernel built for an item, as base feature data.
#[derive(Debug, Clone)]
pub struct Prebuilt {
    pub(crate) data: Vec<u8>,
    pub(crate) mesh: bool,
    pub(crate) solid: bool,
    pub(crate) volume: Option<f64>,
    /// A mesh's area (the kernel measures the other bodies).
    pub(crate) area: Option<f64>,
    pub(crate) bounds: Option<([f64; 3], [f64; 3])>,
    /// What the item was (`faceted brep`, ...).
    pub(crate) kind: &'static str,
    pub(crate) notes: Vec<String>,
}

/// An item ready for the batch.
#[derive(Debug, Clone)]
pub enum Node {
    Feature(Item),
    Body(Prebuilt),
    Boolean {
        op: BoolOp,
        first: Box<Node>,
        second: Box<Node>,
    },
    Failed {
        kind: String,
        why: String,
    },
}

fn is_planar_disk(item: &Item) -> bool {
    match item {
        Item::SweptDisk { directrix, .. } => directrix.plane(plane_tolerance(directrix)).is_some(),
        _ => false,
    }
}

fn plane_tolerance(path: &Path) -> f64 {
    let size = path
        .segments
        .iter()
        .flat_map(|s| s.sample(2))
        .map(math::length)
        .fold(1.0, f64::max);
    (size * 1e-9).max(1e-6)
}

/// Builds what the kernel builds (breps, face sets, swept disks along a
/// curve that is not planar) and leaves the rest to the features.
pub fn prepare<K: Kernel>(kernel: &K, item: Item) -> Node {
    let failed = |kind: &str, why: String| Node::Failed {
        kind: kind.to_owned(),
        why,
    };
    let built = |shape: Result<(K::Shape, Vec<String>, bool), KernelError>,
                 kind: &'static str,
                 mesh: bool,
                 volume: Option<f64>| match shape {
        Ok((shape, notes, solid)) => match kernel.brep_data(&shape) {
            Ok(data) => Node::Body(Prebuilt {
                data,
                mesh,
                solid,
                volume,
                area: None,
                bounds: kernel
                    .bounding_box(&shape)
                    .ok()
                    .flatten()
                    .map(|b| (b.min, b.max)),
                kind,
                notes,
            }),
            Err(e) => failed(kind, e.to_string()),
        },
        Err(e) => failed(kind, e.to_string()),
    };
    match item {
        Item::Faceted { faces, closed } => match brep::faceted(&faces) {
            Ok(mut body) => {
                if !closed {
                    // A surface model: its faces as a sheet, whatever closes.
                    body.shells = vec![body.shells.concat()];
                }
                built(
                    kernel.boundary_body(&body).map(|(s, r)| {
                        let mut notes = r.messages;
                        if r.left_out > 0 {
                            notes.insert(0, format!("{} faces or edges left out", r.left_out));
                        }
                        (s, notes, r.solid)
                    }),
                    "faceted brep",
                    false,
                    None,
                )
            }
            Err(why) => failed("faceted brep", why),
        },
        Item::Advanced { faces, .. } => match brep::advanced(&faces) {
            Ok(body) => built(
                kernel.boundary_body(&body).map(|(s, r)| {
                    let mut notes = r.messages;
                    if r.left_out > 0 {
                        notes.insert(0, format!("{} faces or edges left out", r.left_out));
                    }
                    (s, notes, r.solid)
                }),
                "advanced brep",
                false,
                None,
            ),
            Err(why) => failed("advanced brep", why),
        },
        Item::Mesh {
            points,
            triangles,
            closed,
        } => {
            if triangles.is_empty() {
                return failed("face set", "a face set without faces".to_owned());
            }
            let (volume, area) = brep::mesh_measures(&points, &triangles);
            // Closed when the file says so or when every edge joins two
            // triangles.
            let closed = closed || brep::mesh_closed(&points, &triangles);
            let mut node = built(
                kernel
                    .triangle_body(&points, &triangles)
                    .map(|s| (s, Vec::new(), false)),
                "face set",
                true,
                closed.then_some(volume.abs()),
            );
            if let Node::Body(p) = &mut node {
                p.area = Some(area);
            }
            node
        }
        Item::SweptDisk {
            directrix,
            radius,
            inner_radius,
        } if directrix.plane(plane_tolerance(&directrix)).is_none() => {
            let path: Vec<mitcad_model::sweeps::PathCurve> = directrix
                .segments
                .iter()
                .enumerate()
                .map(|(i, s)| mitcad_model::sweeps::PathCurve {
                    name: format!("c{}", i + 1),
                    curve: curve3(s),
                })
                .collect();
            let closed = directrix.is_closed(plane_tolerance(&directrix));
            let spec = mitcad_model::sweeps::PipeSpec {
                feature: FeatureUid(0),
                path: &path,
                extent1: 1.0,
                extent2: if closed { 0.0 } else { 1.0 },
                section: mitcad_model::sweeps::PipeSection::Circular,
                size: 2.0 * radius,
                thickness: inner_radius.map(|r| radius - r),
            };
            built(
                kernel.pipe(&spec).map(|s| {
                    (
                        s,
                        vec![
                            "along a curve that is not planar: built as the file's body".to_owned(),
                        ],
                        true,
                    )
                }),
                "swept disk",
                false,
                None,
            )
        }
        Item::Boolean { op, first, second } => Node::Boolean {
            op,
            first: Box::new(prepare(kernel, *first)),
            second: Box::new(prepare(kernel, *second)),
        },
        Item::Unsupported { kind, why, id } => failed(&kind, format!("#{id}: {why}")),
        Item::Mapped { map, .. } => failed(
            "IFCMAPPEDITEM",
            format!("the mapped item of map #{map} is not placed here"),
        ),
        other => Node::Feature(other),
    }
}

/// A piece as the model's 3D curve.
fn curve3(seg: &Seg) -> mitcad_model::Curve3 {
    match seg {
        Seg::Line { start, end } => mitcad_model::Curve3::Line {
            start: *start,
            end: *end,
        },
        Seg::Arc {
            frame,
            radius,
            start,
            sweep,
        } => conic3(frame, *radius, *radius, *start, *sweep),
        Seg::Ellipse {
            frame,
            a,
            b,
            start,
            sweep,
        } => conic3(frame, *a, *b, *start, *sweep),
        Seg::Spline {
            degree,
            poles,
            weights,
            knots,
            multiplicities,
        } => mitcad_model::Curve3::BSpline {
            degree: *degree,
            poles: poles.clone(),
            weights: weights.clone(),
            knots: mitcad_ifc::geometry::full_knots(knots, multiplicities),
        },
        Seg::Helix {
            frame,
            radius,
            pitch,
            start,
            sweep,
        } => helix3(frame, *radius, *pitch, *start, *sweep),
    }
}

/// A helix as the model's curve (mitcad#385), its parameter increasing
/// along it: run backwards, about the reversed axis from the negated
/// angles.
fn helix3(frame: &Matrix, radius: f64, pitch: f64, start: f64, sweep: f64) -> mitcad_model::Curve3 {
    let axis = math::column(frame, 2);
    let (axis, s, e) = if sweep < 0.0 {
        (math::scale(axis, -1.0), -start, -(start + sweep))
    } else {
        (axis, start, start + sweep)
    };
    mitcad_model::Curve3::Helix {
        center: math::origin(frame),
        axis,
        x_axis: math::column(frame, 0),
        radius,
        pitch,
        start: s,
        end: e,
    }
}

fn conic3(frame: &Matrix, a: f64, b: f64, start: f64, sweep: f64) -> mitcad_model::Curve3 {
    let mut normal = math::column(frame, 2);
    let (mut s, mut e) = (start, start + sweep);
    if sweep < 0.0 {
        normal = math::scale(normal, -1.0);
        s = -s;
        e = -e;
    }
    mitcad_model::Curve3::Conic {
        center: math::origin(frame),
        normal,
        x_axis: math::column(frame, 0),
        major: a,
        minor: b,
        start: s,
        end: e,
        closed: sweep.abs() >= TAU - 1e-9,
    }
}

/// The box of what a node makes (a boolean's: its first operand's).
pub(crate) fn node_bounds(node: &Node) -> Option<([f64; 3], [f64; 3])> {
    match node {
        Node::Feature(item) => super::transform::bounds(item),
        Node::Body(p) => p.bounds,
        Node::Boolean { first, .. } => node_bounds(first),
        Node::Failed { .. } => None,
    }
}

/// What a feature does with bodies.
#[derive(Debug, Clone)]
pub enum Op {
    New,
    /// `cut`, `join` or `intersect` with the participants.
    With(&'static str, Vec<BodyUid>),
}

impl Op {
    fn of(op: BoolOp) -> &'static str {
        match op {
            BoolOp::Union => "join",
            BoolOp::Difference => "cut",
            BoolOp::Intersection => "intersect",
        }
    }

    fn fields(&self, def: &mut Value) {
        match self {
            Op::New => def["operation"] = json!("new_body"),
            Op::With(op, participants) => {
                def["operation"] = json!(op);
                def["participants"] = json!(
                    participants
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                );
            }
        }
    }
}

/// The features of one element in a batch.
pub struct Builder<'b, 'a> {
    pub batch: &'b mut FeatureBatch<'a>,
    /// The feature names taken (the document's and the import's).
    pub(crate) names: &'b mut std::collections::HashSet<String>,
    pub(crate) component: ComponentUid,
    pub label: String,
    /// Features made, in order.
    pub features: Vec<FeatureUid>,
    /// Item kinds that came in as features, and as the kernel's bodies.
    pub(crate) parametric: Vec<&'static str>,
    pub(crate) stored: Vec<&'static str>,
    pub notes: Vec<String>,
}

fn model_error(e: ModelError) -> String {
    e.to_string()
}

impl<'b, 'a> Builder<'b, 'a> {
    pub fn new(
        batch: &'b mut FeatureBatch<'a>,
        names: &'b mut std::collections::HashSet<String>,
        component: ComponentUid,
        label: &str,
    ) -> Self {
        Self {
            batch,
            names,
            component,
            label: label.to_owned(),
            features: Vec::new(),
            parametric: Vec::new(),
            stored: Vec::new(),
            notes: Vec::new(),
        }
    }

    fn add(&mut self, def: Value, name: &str) -> Result<FeatureUid, String> {
        let def: FeatureDef<ValueInput> =
            serde_json::from_value(def).map_err(|e| format!("definition: {e}"))?;
        let name = self.unique(name);
        let added = self
            .batch
            .add(&def, Some(&name), self.component)
            .map_err(model_error)?;
        self.features.push(added.uid);
        Ok(added.uid)
    }

    /// `name`, or `name (2)`, `name (3)`, ... the first not taken; taken
    /// from now on.
    fn unique(&mut self, name: &str) -> String {
        let name = name.trim();
        let name = if name.is_empty() { "IFC element" } else { name };
        let mut out = name.to_owned();
        let mut n = 2;
        while self.names.contains(&out) {
            out = format!("{name} ({n})");
            n += 1;
        }
        self.names.insert(out.clone());
        out
    }

    fn sketch(&mut self, sketch: SketchBuild, what: &str) -> Result<FeatureUid, String> {
        self.notes.extend(sketch.notes.iter().cloned());
        let name = format!("{} {what}", self.label);
        self.add(sketch.def, &name)
    }

    fn profiles(uid: FeatureUid, regions: &[String]) -> Value {
        json!(
            regions
                .iter()
                .map(|r| json!({"sketch": uid.to_string(), "region": r}))
                .collect::<Vec<_>>()
        )
    }

    /// The bodies a feature that makes new bodies makes: its first (the
    /// model numbers them; a profile of several separate areas may make
    /// more, which the importer finds after the batch).
    fn made(uid: FeatureUid, op: &Op) -> Vec<BodyUid> {
        match op {
            Op::New => vec![BodyUid::new(uid, 0)],
            Op::With(_, participants) => participants.clone(),
        }
    }

    /// Builds a node; the bodies it makes or works on.
    pub fn node(&mut self, node: &Node, op: Op) -> Result<Vec<BodyUid>, String> {
        match node {
            Node::Failed { kind, why } => Err(format!("{kind}: {why}")),
            Node::Body(p) => {
                if p.mesh && !matches!(op, Op::New) {
                    return Err("a face set (a mesh body) cannot be cut or joined".to_owned());
                }
                let mut def = BaseDef::new(vec![BaseBody {
                    name: None,
                    color: None,
                    mesh: p.mesh,
                    brep: Brep::new(p.data.clone()),
                    number: None,
                    first_face: None,
                }]);
                if let Op::With(o, participants) = &op {
                    def.operation = match *o {
                        "cut" => Operation::Cut,
                        "join" => Operation::Join,
                        _ => Operation::Intersect,
                    };
                    def.participants = participants.clone();
                }
                def.source = Some(format!("IFC {}", p.kind));
                let name = self.unique(&self.label.clone());
                let added = self
                    .batch
                    .add(&FeatureDef::Base(def), Some(&name), self.component)
                    .map_err(model_error)?;
                self.features.push(added.uid);
                self.stored.push(p.kind);
                self.notes.extend(p.notes.iter().cloned());
                if !p.solid && !p.mesh {
                    self.notes
                        .push(format!("the {} bounds no solid: a surface body", p.kind));
                }
                Ok(Self::made(added.uid, &op))
            }
            Node::Boolean {
                op: bop,
                first,
                second,
            } => {
                let targets = match &op {
                    Op::New => None,
                    Op::With(..) => Some(op.clone()),
                };
                let bodies = self.node(first, Op::New)?;
                match self.boolean(*bop, &bodies, first, second) {
                    Ok(()) => {}
                    Err(why) => self
                        .notes
                        .push(format!("the boolean's second operand is left out: {why}")),
                }
                match targets {
                    None => Ok(bodies),
                    Some(Op::With(o, participants)) => {
                        self.combine(o, &participants, &bodies)?;
                        Ok(participants)
                    }
                    Some(Op::New) => unreachable!("handled"),
                }
            }
            Node::Feature(item) => self.item(item, op),
        }
    }

    /// The second operand of a boolean applied to the first's bodies.
    fn boolean(
        &mut self,
        op: BoolOp,
        bodies: &[BodyUid],
        first: &Node,
        second: &Node,
    ) -> Result<(), String> {
        if bodies.is_empty() {
            return Err("the first operand made no body".to_owned());
        }
        match second {
            Node::Feature(Item::HalfSpace {
                plane,
                below,
                boundary,
            }) => {
                let bound = node_bounds(first);
                if op == BoolOp::Union {
                    return Err("a union with a half space".to_owned());
                }
                // An intersection keeps the half space's side: cut the
                // other side.
                let below = if op == BoolOp::Intersection {
                    !below
                } else {
                    *below
                };
                self.half_space(plane, below, boundary.as_ref(), bound, bodies)
            }
            Node::Feature(item) if !matches!(item, Item::HalfSpace { .. }) => {
                self.item(item, Op::With(Op::of(op), bodies.to_vec()))?;
                Ok(())
            }
            Node::Body(p) if !p.mesh => {
                self.node(second, Op::With(Op::of(op), bodies.to_vec()))?;
                Ok(())
            }
            Node::Failed { kind, why } => Err(format!("{kind}: {why}")),
            _ => {
                let tools = self.node(second, Op::New)?;
                self.combine(Op::of(op), bodies, &tools)
            }
        }
    }

    /// Each target combined with the tools (kept until the last).
    fn combine(&mut self, op: &str, targets: &[BodyUid], tools: &[BodyUid]) -> Result<(), String> {
        for (i, target) in targets.iter().enumerate() {
            let def = json!({"type": "combine", "target": target.to_string(),
                "tools": tools.iter().map(ToString::to_string).collect::<Vec<_>>(),
                "operation": op, "keep_tools": i + 1 < targets.len()});
            let name = format!("{} {op}", self.label);
            self.add(def, &name)?;
        }
        Ok(())
    }

    fn half_space(
        &mut self,
        plane: &Matrix,
        below: bool,
        boundary: Option<&(Matrix, Path)>,
        bounds: Option<([f64; 3], [f64; 3])>,
        bodies: &[BodyUid],
    ) -> Result<(), String> {
        let normal = math::column(plane, 2);
        let participants: Vec<String> = bodies.iter().map(ToString::to_string).collect();
        match boundary {
            None => {
                let (lo, hi) = bounds.ok_or("the clipped body's extent is not known")?;
                let center: [f64; 3] = std::array::from_fn(|k| (lo[k] + hi[k]) / 2.0);
                let diagonal = math::length(math::sub(hi, lo)).max(1.0);
                // The centre projected onto the plane.
                let o = math::origin(plane);
                let c = math::sub(
                    center,
                    math::scale(normal, math::dot(math::sub(center, o), normal)),
                );
                let frame =
                    math::from_axes(c, math::column(plane, 0), math::column(plane, 1), normal);
                let h = 2.0 * diagonal;
                let rect =
                    mitcad_ifc::geometry::rounded(&[[-h, -h], [h, -h], [h, h], [-h, h]], &[0.0; 4]);
                let sketch = profile_sketch(&frame, &area_profile(rect))?;
                let regions = sketch.regions.clone();
                let s = self.sketch(sketch, "clipping plane")?;
                let def = json!({"type": "extrude", "profiles": Self::profiles(s, &regions),
                    "extent": {"type": "through_all"}, "flip": below,
                    "operation": "cut", "participants": participants});
                let name = format!("{} clipping", self.label);
                self.add(def, &name)?;
            }
            Some((frame, polygon)) => {
                let axis = math::column(frame, 2);
                let material = if below {
                    math::scale(normal, -1.0)
                } else {
                    normal
                };
                let along = math::dot(axis, material);
                if along.abs() < 1e-6 {
                    return Err("the boundary's prism runs along the half space's plane".to_owned());
                }
                let sketch = profile_sketch(frame, &area_profile(polygon.clone()))?;
                let regions = sketch.regions.clone();
                let s = self.sketch(sketch, "clipping boundary")?;
                let def = json!({"type": "extrude", "profiles": Self::profiles(s, &regions),
                    "start": {"type": "object", "object": {"type": "plane", "plane": {
                        "origin": math::origin(plane), "normal": normal,
                        "x_axis": math::column(plane, 0)}}},
                    "extent": {"type": "through_all"}, "flip": along < 0.0,
                    "operation": "cut", "participants": participants});
                let name = format!("{} clipping", self.label);
                self.add(def, &name)?;
            }
        }
        self.parametric.push("half space");
        Ok(())
    }

    /// An item built as features.
    fn item(&mut self, item: &Item, op: Op) -> Result<Vec<BodyUid>, String> {
        match item {
            Item::Extrusion {
                position,
                profile,
                direction,
                depth,
            } => {
                let sketch = profile_sketch(position, profile)?;
                let regions = sketch.regions.clone();
                let s = self.sketch(sketch, "profile")?;
                let straight = direction[0].abs() + direction[1].abs() <= 1e-9;
                let mut def = if straight {
                    json!({"type": "extrude", "profiles": Self::profiles(s, &regions),
                        "extent": {"type": "distance", "distance": depth},
                        "flip": direction[2] < 0.0})
                } else {
                    let o = math::origin(position);
                    let d = math::scale(math::apply_vector(position, *direction), *depth);
                    json!({"type": "sweep", "profiles": Self::profiles(s, &regions),
                        "path": {"start": o, "end": math::add(o, d)},
                        "orientation": "parallel"})
                };
                op.fields(&mut def);
                let label = self.label.clone();
                let uid = self.add(def, &label)?;
                self.parametric.push(if straight {
                    "extrusion"
                } else {
                    "slanted extrusion"
                });
                Ok(Self::made(uid, &op))
            }
            Item::TaperedExtrusion {
                position,
                profile,
                end_profile,
                direction,
                depth,
            } => {
                if direction[0].abs() + direction[1].abs() > 1e-9 {
                    return Err("a slanted tapered extrusion".to_owned());
                }
                if profile.areas.len() != end_profile.areas.len()
                    || profile
                        .areas
                        .iter()
                        .zip(&end_profile.areas)
                        .any(|(a, b)| a.holes.len() != b.holes.len())
                {
                    return Err("the end profile's areas do not match the start's".to_owned());
                }
                let shift = math::scale(math::column(position, 2), direction[2] * depth);
                let mut end_frame = *position;
                for (r, s) in end_frame.iter_mut().zip(shift) {
                    r[3] += s;
                }
                let start = profile_sketch(position, profile)?;
                let end = profile_sketch(&end_frame, end_profile)?;
                let (start_holes, end_holes) = (start.holes.clone(), end.holes.clone());
                let (start_regions, end_regions) = (start.regions.clone(), end.regions.clone());
                let s0 = self.sketch(start, "start profile")?;
                let s1 = self.sketch(end, "end profile")?;
                let mut made = Vec::new();
                for (i, (a, b)) in start_regions.iter().zip(&end_regions).enumerate() {
                    let mut def = json!({"type": "loft", "sections": [
                        {"type": "profile", "sketch": s0.to_string(), "region": a},
                        {"type": "profile", "sketch": s1.to_string(), "region": b}],
                        "ruled": true});
                    op.fields(&mut def);
                    let label = self.label.clone();
                    let uid = self.add(def, &label)?;
                    let bodies = Self::made(uid, &op);
                    for (h0, h1) in start_holes[i].iter().zip(&end_holes[i]) {
                        let cut = json!({"type": "loft", "sections": [
                            {"type": "profile", "sketch": s0.to_string(), "region": h0},
                            {"type": "profile", "sketch": s1.to_string(), "region": h1}],
                            "ruled": true, "operation": "cut",
                            "participants": bodies.iter().map(ToString::to_string).collect::<Vec<_>>()});
                        let name = format!("{} hole", self.label);
                        self.add(cut, &name)?;
                    }
                    made.extend(bodies);
                }
                self.parametric.push("tapered extrusion");
                made.dedup();
                Ok(made)
            }
            Item::Revolution {
                position,
                profile,
                axis_origin,
                axis,
                angle,
            } => {
                let sketch = profile_sketch(position, profile)?;
                let regions = sketch.regions.clone();
                let s = self.sketch(sketch, "profile")?;
                let extent = if angle.abs() >= TAU - 1e-9 {
                    json!({"type": "full"})
                } else {
                    json!({"type": "angle", "angle": angle})
                };
                let mut def = json!({"type": "revolve", "profiles": Self::profiles(s, &regions),
                    "axis": {"origin": math::apply(position, *axis_origin),
                             "direction": math::apply_vector(position, *axis)},
                    "extent": extent});
                op.fields(&mut def);
                let label = self.label.clone();
                let uid = self.add(def, &label)?;
                self.parametric.push("revolution");
                Ok(Self::made(uid, &op))
            }
            Item::Sweep {
                profile,
                directrix,
                reference,
                binormal,
                ..
            } => {
                // A helix (the `.rvt` import's spiral rails, mitcad#385) is a
                // helical path of its own; another directrix a path sketch.
                let path = match directrix.segments.as_slice() {
                    [
                        Seg::Helix {
                            frame,
                            radius,
                            pitch,
                            start,
                            sweep,
                        },
                    ] => json!({"helix": {
                        "center": math::origin(frame), "axis": math::column(frame, 2),
                        "x_axis": math::column(frame, 0), "radius": radius, "pitch": pitch,
                        "start_angle": start, "end_angle": start + sweep}}),
                    _ => {
                        let (path_uid, curves) = self.path(directrix)?;
                        json!({"sketch": path_uid.to_string(), "curves": curves})
                    }
                };
                let start = directrix.start().ok_or("an empty directrix")?;
                let tangent = start_tangent(directrix).ok_or("the directrix has no tangent")?;
                let plane_normal = directrix
                    .plane(plane_tolerance(directrix))
                    .map(|p| math::column(&p, 2));
                let x = reference
                    .or(plane_normal)
                    .and_then(|r| {
                        math::normalize(math::sub(r, math::scale(tangent, math::dot(r, tangent))))
                    })
                    .ok_or("no direction for the profile's x axis")?;
                let y = math::cross(tangent, x);
                // The profile's x axis along the reference (the reference
                // surface's normal), its y axis in the surface, z along the
                // directrix.
                let frame = math::from_axes(start, x, y, tangent);
                let sketch = profile_sketch(&frame, profile)?;
                let regions = sketch.regions.clone();
                let s = self.sketch(sketch, "profile")?;
                let mut def = json!({"type": "sweep", "profiles": Self::profiles(s, &regions),
                    "path": path, "orientation": "perpendicular"});
                // (mitcad#397) The profile kept at its angles to a fixed
                // binormal.
                if let Some(b) = binormal {
                    def["binormal"] = json!(b);
                }
                op.fields(&mut def);
                let label = self.label.clone();
                let uid = self.add(def, &label)?;
                self.parametric.push("sweep");
                Ok(Self::made(uid, &op))
            }
            Item::SweptDisk {
                directrix,
                radius,
                inner_radius,
            } if is_planar_disk(item) => {
                let (path_uid, curves) = self.path(directrix)?;
                let mut def = json!({"type": "pipe",
                    "path": {"sketch": path_uid.to_string(), "curves": curves},
                    "section": "circular", "size": 2.0 * radius});
                if let Some(inner) = inner_radius {
                    def["thickness"] = json!(radius - inner);
                }
                op.fields(&mut def);
                let label = self.label.clone();
                let uid = self.add(def, &label)?;
                self.parametric.push("swept disk");
                Ok(Self::made(uid, &op))
            }
            Item::HalfSpace { .. } => Err("a half space alone bounds no body".to_owned()),
            Item::Boolean { .. } => Err("a boolean not prepared".to_owned()),
            other => Err(format!("{} items are not built as features", other.name())),
        }
    }

    /// A sketch of a planar path: its uid and curves.
    fn path(&mut self, path: &Path) -> Result<(FeatureUid, Vec<String>), String> {
        let plane = path
            .plane(plane_tolerance(path))
            .ok_or("the curve is not planar")?;
        let inverse = math::inverse(&plane).ok_or("a degenerate plane")?;
        let mut local = path.transformed(&inverse);
        for s in &mut local.segments {
            match s {
                Seg::Line { start, end } => {
                    start[2] = 0.0;
                    end[2] = 0.0;
                }
                Seg::Spline { poles, .. } => poles.iter_mut().for_each(|p| p[2] = 0.0),
                Seg::Arc { frame, .. } | Seg::Ellipse { frame, .. } => frame[2][3] = 0.0,
                Seg::Helix { .. } => return Err("a helix is not planar".to_owned()),
            }
        }
        let sketch = path_sketch(&plane, &local)?;
        let curves = sketch.curves.clone();
        let uid = self.sketch(sketch, "path")?;
        Ok((uid, curves))
    }
}

/// A profile of one area bounded by `outer`.
fn area_profile(outer: Path) -> Profile {
    Profile {
        kind: String::new(),
        name: String::new(),
        areas: vec![mitcad_ifc::geometry::Area {
            outer,
            holes: Vec::new(),
        }],
    }
}

/// The direction a path starts in.
fn start_tangent(path: &Path) -> Option<[f64; 3]> {
    let first = path.segments.first()?;
    match first {
        Seg::Line { start, end } => math::normalize(math::sub(*end, *start)),
        Seg::Spline { poles, .. } => {
            let p0 = poles[0];
            poles
                .iter()
                .skip(1)
                .find_map(|p| math::normalize(math::sub(*p, p0)))
        }
        Seg::Helix {
            frame,
            radius,
            pitch,
            start,
            sweep,
        } => {
            let d = [
                -radius * start.sin(),
                radius * start.cos(),
                pitch / std::f64::consts::TAU,
            ];
            math::normalize(math::scale(math::apply_vector(frame, d), sweep.signum()))
        }
        // (mitcad#397) Exactly: a chord's direction is half its angle off,
        // which turned a sweep's profile out of the plane square to the
        // path (and its start cap off the piece before it).
        Seg::Arc {
            frame,
            radius,
            start,
            sweep,
        } => {
            let d = [-radius * start.sin(), radius * start.cos(), 0.0];
            math::normalize(math::scale(math::apply_vector(frame, d), sweep.signum()))
        }
        Seg::Ellipse {
            frame,
            a,
            b,
            start,
            sweep,
        } => {
            let d = [-a * start.sin(), b * start.cos(), 0.0];
            math::normalize(math::scale(math::apply_vector(frame, d), sweep.signum()))
        }
    }
}
