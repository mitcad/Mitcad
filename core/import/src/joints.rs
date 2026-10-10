// SPDX-License-Identifier: MIT
//! The file's joints, as-built joints, joint origins and ground items as
//! Mitcad's (mitcad#55, phase 4; what the decoder gives: SCHEMA.md §5.3
//! `Joint`, `AsBuiltJoint`, `JointOrigin`, `GroundOccurrence`).
//!
//! Occurrences start where the file places them at the end of its
//! timeline (their last captured positions, else the transforms the file
//! stores: `components.rs`, `captures.rs`), and nothing the import adds
//! may move them elsewhere than there: a joint comes in as a joint only
//! where it holds there (after it is added, every occurrence is where it
//! was or where the file places it), else it keeps the placements as an
//! as-built joint, with a warning. Captured positions come in as
//! `capture_position` features at their points of the timeline.
//!
//! - **Joints.** Side one is origin `a`, side two `b` (side one moves onto
//!   side two, as `a` does onto `b`). The decoder's relation `world(one) ·
//!   frame one · Rz(angle) · Rx(π if opposed) · T(−offset) = world(two) ·
//!   frame two` is Mitcad's `frame_a = frame_b · Rz(±angle) · Tz(offset) ·
//!   flip` with `flip` where the file's z axes are opposed, the angle as
//!   it is when flipped and the other way round when not, and the x and y
//!   offsets moving `b`'s origin within its plane (numbers). The sense of
//!   the file's angle is not settled (only 0 and 180° were read), so an
//!   angle that is neither is tried both ways; the placements pick.
//! - **Sides.** The frames are the file's stored ones, in the components
//!   the occurrence paths end in. Geometry of that component at the joint's
//!   point of the timeline that gives the frame comes first (a joint origin
//!   the side names, construction geometry its key point or direction
//!   names, circular edges centred on the frame's origin along its z axis,
//!   planar faces through it across z, faces of revolution about z), with
//!   a `frame_override` for the parts it does not give; else a fixed plane
//!   with the frame (the report counts those sides). A side on a component
//!   of another document (a fastener's joint origin, mitcad#75) goes on
//!   the origin of the empty component standing for it, with the file's
//!   frame there (the identity in the designs read).
//! - **Motions.** The kind from the motion; rotation limits are `rz`'s,
//!   slide limits the slide's (a slider's axis by the axis code of its
//!   value, `z` unless 0 or 1 say `x` or `y` [assumed]); planar and ball
//!   joints' limits are left out (which motion they bound is not known).
//!   The sense of the file's values is not settled either: the limits are
//!   tried as they are, turned round, then left out. A rest value other
//!   than the current value adds the current value as the position.
//! - **As-built joints.** Two occurrences joined where they are: `relative`
//!   from the placements the file stores (the frames the file records for
//!   each occurrence agree with them in the designs read; a warning where
//!   they do not), the motion frame (kinds with motions) the recorded
//!   frame on occurrence two. Limits only where the current value is 0
//!   (Mitcad's values are 0 at the recorded placement).
//! - **Joint origins.** A `joint_origin` of the owning component with the
//!   file's angle and z offset (parameters) on geometry that gives the
//!   frame before them (the component's origin point, a face or edge, else
//!   a fixed plane), checked against the file's frame; else the file's
//!   frame fixed.
//! - **Ground items.** The occurrence's ground flag (the occurrence tree
//!   has it too).
//!
//! Occurrences of components of other documents (fasteners, inserted
//! parts) are occurrences of empty components; joints of occurrences inside
//! them (a level of a path in another document) are left out.

use std::collections::{BTreeMap, HashMap};

use mitcad_f3d::design::ir::{Reference, TimelineItem};
use mitcad_model::assembly::{inverse, matrix_rows};
use mitcad_model::datum::{CurveGeometry, Datum, SurfaceGeometry};
use mitcad_model::joints::{JointKind, Motion, SlideAxis, motion_values};
use mitcad_model::{
    BodyUid, ComponentUid, EdgeName, FaceName, FeatureUid, Kernel, OccurrenceUid, Transform,
};
use serde_json::{Map, Value, json};

use crate::geom::{self, mm, mm3};
use crate::report::Outcome;
use crate::{Candidate, Importer, components};

type Vec3 = [f64; 3];

/// A trace line with the time (`MITCAD_IMPORT_TRACE`).
macro_rules! trace {
    ($($arg:tt)*) => {
        if crate::tracing() {
            eprintln!("import: [{:.2}] {}", crate::trace_clock(), format!($($arg)*));
        }
    };
}

/// A face or edge that can give a joint's side its frame: a circular
/// edge or a face of the kinds `frame_entities` looks for.
#[derive(Clone)]
pub(crate) enum FrameEntity {
    Edge(EdgeName, CurveGeometry),
    Face(FaceName, SurfaceGeometry),
}

/// The [`FrameEntity`]s of each body with the body's version: joints on
/// the same large part find them once (mitcad#87).
pub(crate) type FrameEntities = HashMap<BodyUid, (u128, std::sync::Arc<Vec<FrameEntity>>)>;

/// A joint's limits of one motion, each with its value (mm or rad).
struct Limit {
    motion: &'static str,
    min: Option<(Value, f64)>,
    max: Option<(Value, f64)>,
    rest: Option<(Value, f64)>,
}

/// Distances below this hold a joint where the file places the
/// occurrences, mm (relative to the size of the placements).
const LINEAR: f64 = 1e-6;
/// Rotation matrix entries within this hold a joint.
const ANGULAR: f64 = 1e-7;
/// The most faces and edges tried as a side's geometry.
const MAX_ENTITIES: usize = 8;

/// The item's detail as JSON.
fn detail(item: &TimelineItem) -> Value {
    item.detail
        .as_ref()
        .and_then(|d| serde_json::to_value(d).ok())
        .unwrap_or(Value::Null)
}

fn vec3(v: &Value) -> Option<Vec3> {
    serde_json::from_value(v.clone()).ok()
}

fn reference(v: &Value) -> Option<Reference> {
    (!v.is_null())
        .then(|| serde_json::from_value(v.clone()).ok())
        .flatten()
}

/// A parameter reference's value (cm or rad, as the dump has it).
fn raw_value(v: &Value) -> Option<f64> {
    v.get("value").and_then(Value::as_f64)
}

/// A frame through `origin` with axes `x` and `z` (unit, at right angles);
/// None when they are not.
fn frame(origin: Vec3, x: Vec3, z: Vec3) -> Option<Transform> {
    let (x, z) = (geom::unit(x)?, geom::unit(z)?);
    if geom::dot(x, z).abs() > 1e-6 || !origin.iter().all(|v| v.is_finite()) {
        return None;
    }
    let y = geom::cross(z, x);
    Some(Transform {
        linear: std::array::from_fn(|r| [x[r], y[r], z[r]]),
        translation: origin,
    })
}

/// A frame's origin and x, y, z axes.
fn parts(t: &Transform) -> [Vec3; 4] {
    let column = |c: usize| -> Vec3 { std::array::from_fn(|r| t.linear[r][c]) };
    [t.translation, column(0), column(1), column(2)]
}

/// A 4x4 matrix of the dump (cm) as a rigid transform in millimetres.
fn matrix(v: &Value) -> Option<Transform> {
    components::transform(&serde_json::from_value(v.clone()).ok()?)
}

/// A `JointGeometry`'s frame (mm): its stored matrix, else its origin and
/// axes.
fn geometry_frame(g: &Value) -> Option<Transform> {
    if let Some(t) = g.pointer("/_f3d/frame").and_then(matrix) {
        return Some(t);
    }
    frame(
        mm3(vec3(&g["origin"])?),
        vec3(&g["primaryAxisVector"])?,
        vec3(&g["thirdAxisVector"])?,
    )
}

/// Whether two placements or frames agree within [`LINEAR`] and
/// [`ANGULAR`].
fn same(a: &Transform, b: &Transform) -> bool {
    let size = a
        .translation
        .iter()
        .chain(&b.translation)
        .fold(0.0_f64, |m, v| m.max(v.abs()));
    (0..3).all(|i| (a.translation[i] - b.translation[i]).abs() <= LINEAR * (1.0 + size))
        && (0..3).all(|i| (0..3).all(|j| (a.linear[i][j] - b.linear[i][j]).abs() <= ANGULAR))
}

/// How far two placements are apart, for messages: the distance of their
/// origins (mm) and the angle of the turn between them.
fn gap(a: &Transform, b: &Transform) -> String {
    let d = geom::distance(a.translation, b.translation);
    // The trace of aᵀ·b is 1 + 2 cos(angle).
    let trace: f64 = (0..3)
        .flat_map(|i| (0..3).map(move |j| a.linear[i][j] * b.linear[i][j]))
        .sum();
    let angle = ((trace - 1.0) / 2.0).clamp(-1.0, 1.0).acos();
    if (0..3).any(|i| (0..3).any(|j| (a.linear[i][j] - b.linear[i][j]).abs() > ANGULAR)) {
        format!("{d:.3e} mm and turned by {:.4} rad", angle)
    } else {
        format!("{d:.3e} mm")
    }
}

/// Mitcad's fixed part of a joint between its frames (`joints.rs` of the
/// model): `Rz(angle) · Tz(offset) · flip`.
fn alignment(angle: f64, offset: f64, flip: bool) -> Transform {
    let z = [0.0, 0.0, 1.0];
    let turn = Transform::rotation([0.0; 3], z, angle).expect("unit axis");
    let lift = Transform::translation([0.0, 0.0, offset]);
    let over = if flip {
        Transform::rotation([0.0; 3], [1.0, 0.0, 0.0], std::f64::consts::PI).expect("unit axis")
    } else {
        Transform::IDENTITY
    };
    turn.after(&lift).after(&over)
}

/// A fixed plane with a frame's origin, normal and x axis.
fn fixed_plane(t: &Transform) -> Value {
    let [o, x, _, z] = parts(t);
    json!({"origin": o, "normal": z, "x_axis": x})
}

/// Why a joint whose sides have the same occurrence path is left out.
fn same_sides(path: &[OccurrenceUid]) -> String {
    if path.is_empty() {
        "both sides are the component's own geometry".to_owned()
    } else {
        "both sides are on the same occurrence".to_owned()
    }
}

/// Reasons in the order they first come, those that repeat counted
/// (`3 × a position inside …`).
fn counted(reasons: &[String]) -> String {
    let mut order: Vec<(&str, usize)> = Vec::new();
    for r in reasons {
        match order.iter_mut().find(|(x, _)| *x == r.as_str()) {
            Some((_, n)) => *n += 1,
            None => order.push((r.as_str(), 1)),
        }
    }
    order
        .iter()
        .map(|(r, n)| {
            if *n > 1 {
                format!("{n} × {r}")
            } else {
                (*r).to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("; ")
}

fn path_text(path: &[OccurrenceUid]) -> String {
    path.iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("/")
}

/// Mitcad's joint kind of a motion (`RevoluteJointMotion`,
/// `RevoluteJointType`).
fn joint_kind(motion: &Value) -> Option<&'static str> {
    let t = motion["_type"]
        .as_str()
        .or_else(|| motion["jointType"].as_str())?;
    let base = t
        .trim_end_matches("JointMotion")
        .trim_end_matches("JointType");
    Some(match base {
        "Rigid" => "rigid",
        "Revolute" => "revolute",
        "Slider" => "slider",
        "Cylindrical" => "cylindrical",
        "PinSlot" => "pin_slot",
        "Planar" => "planar",
        "Ball" => "ball",
        _ => return None,
    })
}

/// A slider's axis by the slot of its motion (`tx`, `ty`, `tz`).
fn slide_axis(motion: &Value) -> &'static str {
    match motion
        .pointer("/_f3d/motions/0/motion")
        .and_then(Value::as_str)
    {
        Some("tx") => "x",
        Some("ty") => "y",
        _ => "z",
    }
}

/// How far a length parameter's value may be from what its expression
/// says, mm: half a unit of its last digit where the expression is a
/// number with decimals and a length unit (`-2.646 mm`: 0.0005 mm).
fn rounding(parameter: &Value) -> Option<f64> {
    let expression = parameter["expression"].as_str()?.trim();
    let (number, unit) = expression.split_once(' ').unwrap_or((expression, "mm"));
    number.parse::<f64>().ok()?;
    let decimals = number.split_once('.').map(|(_, f)| f.len())?;
    let per_unit = match unit.trim() {
        "mm" => 1.0,
        "cm" => 10.0,
        "m" => 1000.0,
        "in" => 25.4,
        _ => return None,
    };
    (decimals > 0).then(|| 0.5 * 10f64.powi(-(decimals as i32)) * per_unit)
}

/// Mitcad's joint kind and slide axis of the mapping's names.
fn model_kind(kind: &str, axis: &str) -> Option<(JointKind, SlideAxis)> {
    let kind = serde_json::from_value(json!(kind)).ok()?;
    let slide = match axis {
        "x" => SlideAxis::X,
        "y" => SlideAxis::Y,
        _ => SlideAxis::Z,
    };
    Some((kind, slide))
}

/// A motion by its name.
fn motion_named(name: &str) -> Option<Motion> {
    serde_json::from_value(json!(name)).ok()
}

/// The motions a kind's rotation and slide limits bound (None: not known
/// which, or none).
fn limited_motions(kind: &str, axis: &str) -> (Option<&'static str>, Option<&'static str>) {
    match kind {
        "revolute" => (Some("rz"), None),
        "slider" => (
            None,
            Some(match axis {
                "x" => "tx",
                "y" => "ty",
                _ => "tz",
            }),
        ),
        "cylindrical" => (Some("rz"), Some("tz")),
        "pin_slot" => (Some("rz"), Some("tx")),
        _ => (None, None),
    }
}

/// A side's geometry: what gives its frame, with a frame override for the
/// parts it does not give.
struct SideGeometry {
    geometry: Value,
    frame_override: Option<Value>,
    /// A fixed plane: no geometry of the component gives the frame.
    fixed: bool,
    /// On the origin of the empty component standing for a component of
    /// another document.
    inserted: bool,
}

impl SideGeometry {
    /// A joint origin (`JointOrigin` of `features/joint.rs`) on `path`.
    fn origin(&self, path: &[OccurrenceUid]) -> Value {
        let mut v = json!({"geometry": self.geometry});
        if !path.is_empty() {
            v["occurrence"] = json!(path_text(path));
        }
        if let Some(o) = &self.frame_override {
            v["frame_override"] = o.clone();
        }
        v
    }
}

/// The parts of `resolved` that differ from `target`, as a frame override.
fn override_for(resolved: &Transform, target: &Transform) -> Option<Value> {
    let [ro, rx, _, rz] = parts(resolved);
    let [to, tx, _, tz] = parts(target);
    let size = 1.0 + ro.iter().chain(&to).fold(0.0_f64, |m, v| m.max(v.abs()));
    let mut o = Map::new();
    if geom::distance(ro, to) > LINEAR * size {
        o.insert("origin".to_owned(), json!(to));
    }
    let differs = |a: Vec3, b: Vec3| (0..3).any(|i| (a[i] - b[i]).abs() > ANGULAR);
    let turned = differs(rz, tz);
    if turned {
        o.insert("z_axis".to_owned(), json!(tz));
    }
    if turned || differs(rx, tx) {
        o.insert("x_axis".to_owned(), json!(tx));
    }
    (!o.is_empty()).then_some(Value::Object(o))
}

/// Construction geometry an entity of a key point or direction names
/// (origin planes, axes and point, imported construction features).
fn datum_ref<K: Kernel>(importer: &Importer<'_, K>, entity: &Value) -> Option<Value> {
    let origin = entity["origin"].as_str();
    match (entity["kind"].as_str()?, origin) {
        ("construction_point", Some("Origin")) => return Some(json!("origin")),
        ("construction_plane", Some(o @ ("XY" | "XZ" | "YZ"))) => {
            return Some(json!(o.to_lowercase()));
        }
        ("construction_axis", Some(o @ ("X" | "Y" | "Z"))) => {
            return Some(json!(o.to_lowercase()));
        }
        ("construction_point" | "construction_plane" | "construction_axis", _) => {}
        _ => return None,
    }
    let index = entity["timeline_index"].as_i64()?;
    importer
        .features
        .get(&index)
        .map(|uid| json!(uid.to_string()))
}

impl<K: crate::ImportKernel> Importer<'_, K> {
    /// A joint, an as-built joint, a joint origin or a ground item.
    pub(crate) fn assembly_item(&mut self, at: usize, index: i64, item: &TimelineItem) {
        let result = match item.object_type() {
            Some("Joint") => self.joint_item(at, index, item),
            Some("AsBuiltJoint") => self.as_built_item(at, index, item),
            Some("JointOrigin") => self.joint_origin_item(at, index, item),
            Some("GroundOccurrence") => self.ground_item(at, item),
            Some("Snapshot") => self.capture_item(at, index, item),
            Some("RigidGroup") => self.rigid_group_item(at, index, item),
            _ => Err("not an assembly item".to_owned()),
        };
        if let Err(reason) = result {
            self.report.joints.skipped += 1;
            self.note(at, Outcome::Skipped, reason);
        }
    }

    /// An occurrence reference of a joint item (`label` in messages) as
    /// Mitcad's path, with the component it starts in
    /// (`context_component`, else the item's).
    fn occurrence_path(
        &self,
        item: &TimelineItem,
        side: &Value,
        label: &str,
    ) -> Result<(ComponentUid, Vec<OccurrenceUid>), String> {
        let f3d = side
            .get("_f3d")
            .ok_or_else(|| format!("{label} was not decoded"))?;
        let levels = f3d["path"]
            .as_array()
            .ok_or_else(|| format!("{label}'s path was not decoded"))?;
        let mut path = Vec::new();
        for level in levels {
            let Some(id) = level.as_u64() else {
                return Err(format!("{label} is inside a component of another document"));
            };
            match self.components.occurrence(id) {
                Some(o) => path.push(o),
                None => {
                    return Err(format!(
                        "{label} (object {id}) is not in the occurrence tree"
                    ));
                }
            }
        }
        let owner = item.f3d.as_ref().and_then(|f| f.component);
        let component = f3d["context_component"]
            .as_u64()
            .or(owner)
            .and_then(|c| self.components.of_object(c))
            .unwrap_or(self.component);
        Ok((component, path))
    }

    /// The component an occurrence path from `component` ends in.
    fn path_end(&self, component: ComponentUid, path: &[OccurrenceUid]) -> ComponentUid {
        path.last()
            .and_then(|o| self.doc.assembly().occurrence(*o))
            .map_or(component, |o| o.component)
    }

    /// A path's placement into its component where the file places the
    /// occurrences at the end of its timeline (their last captured
    /// positions, else their stored transforms).
    fn file_path_transform(&self, path: &[OccurrenceUid]) -> Transform {
        let a = self.doc.assembly();
        path.iter().fold(Transform::IDENTITY, |t, o| {
            let stored = a
                .occurrence(*o)
                .map_or(Transform::IDENTITY, |x| x.transform);
            t.after(&self.captures.final_placement(*o, stored))
        })
    }

    /// `a`'s placement in `b`'s coordinates for an as-built joint: where
    /// the file places them at the end of its timeline, then where they
    /// are at the marker when that differs (the captured positions after
    /// it place them elsewhere).
    fn relatives(&self, a: &[OccurrenceUid], b: &[OccurrenceUid]) -> Vec<Transform> {
        let file = inverse(&self.file_path_transform(b)).after(&self.file_path_transform(a));
        let now = inverse(&self.doc.path_transform(b)).after(&self.doc.path_transform(a));
        if same(&file, &now) {
            vec![file]
        } else {
            vec![file, now]
        }
    }

    /// The occurrences' placements in their parents at the marker.
    fn placements(&self) -> HashMap<OccurrenceUid, Transform> {
        let a = self.doc.assembly();
        a.occurrences
            .iter()
            .filter_map(|o| Some((o.uid, self.doc.placement(o.uid)?)))
            .collect()
    }

    /// An occurrence the last feature moved from where it was (`before`)
    /// elsewhere than where the file places it at the end of its timeline
    /// (its last captured position, else its own transform), with how far
    /// from there.
    fn moved_occurrence(
        &self,
        before: &HashMap<OccurrenceUid, Transform>,
        index: i64,
    ) -> Option<String> {
        let a = self.doc.assembly();
        a.occurrences.iter().find_map(|o| {
            let p = self.doc.placement(o.uid)?;
            let file = self.captures.final_placement(o.uid, o.transform);
            let kept = before.get(&o.uid).is_some_and(|b| same(&p, b))
                || self
                    .captures
                    .placement_after(a, o.uid, index)
                    .is_some_and(|t| same(&p, &t));
            (!kept && !same(&p, &file)).then(|| {
                format!(
                    "it moves {} by {}",
                    a.occurrence_name(o.uid),
                    gap(&p, &file)
                )
            })
        })
    }

    /// Adds a definition to `component`; kept when it moves no occurrence
    /// elsewhere than where the file places it.
    fn add_unmoved(
        &mut self,
        def: &Value,
        name: Option<&str>,
        component: ComponentUid,
        index: i64,
    ) -> Result<FeatureUid, String> {
        let depth = self.doc.undo_depth();
        let before = self.placements();
        let uid = self.add_in(def, name, component)?;
        if let Some(why) = self.moved_occurrence(&before, index) {
            self.undo_to(depth);
            return Err(why);
        }
        Ok(uid)
    }

    /// The frame `geometry` gives in `component` (with `frame_override`), as
    /// a joint resolves it at the marker.
    fn resolved_frame(
        &self,
        component: ComponentUid,
        geometry: &Value,
        frame_override: Option<&Value>,
    ) -> Option<Transform> {
        let mut q = json!({"query": "joint_frame", "geometry": geometry,
            "component": component.to_string()});
        if let Some(o) = frame_override {
            q["frame_override"] = o.clone();
        }
        let answer: Value = serde_json::from_str(&self.doc.query(&q.to_string()).ok()?).ok()?;
        let l = &answer["local"];
        frame(
            vec3(&l["origin"])?,
            vec3(&l["x_axis"])?,
            vec3(&l["z_axis"])?,
        )
    }

    /// The circular edges and the planar, cylindrical, conical, toroidal
    /// and spherical faces of a body at the marker with their geometry:
    /// in one pass where the kernel has it (mitcad#87: finding each by its
    /// name takes as long as the whole list on large bodies), kept while
    /// the body's shape is the same.
    fn body_frame_entities(&self, body: BodyUid) -> std::sync::Arc<Vec<FrameEntity>> {
        let version = self.doc.body_version(body).unwrap_or_default();
        if let Some((v, entities)) = self.frame_entities.borrow().get(&body)
            && *v == version
        {
            return entities.clone();
        }
        let kernel = self.doc.kernel();
        let mut out = Vec::new();
        if let Some(shape) = self.doc.body_shape(body) {
            // Kernel calls in a row: the watchdog's progress (mitcad#82).
            crate::tick();
            match kernel.edge_geometries(shape) {
                Ok(all) => out.extend(all.into_iter().filter_map(|(name, curve)| {
                    let name = name?.parse::<EdgeName>().ok()?;
                    matches!(curve, CurveGeometry::Circle { .. })
                        .then_some(FrameEntity::Edge(name, curve))
                })),
                Err(_) => {
                    for e in kernel.edges(shape).unwrap_or_default() {
                        if let Some(name) = e
                            .name
                            .as_deref()
                            .filter(|_| e.curve == "circle")
                            .and_then(|n| n.parse::<EdgeName>().ok())
                            && let Ok(curve) = kernel.edge_geometry(shape, &name)
                        {
                            out.push(FrameEntity::Edge(name, curve));
                        }
                        crate::tick();
                    }
                }
            }
            let wanted = |s: &SurfaceGeometry| !matches!(s, SurfaceGeometry::Other { .. });
            crate::tick();
            match kernel.face_geometries(shape) {
                Ok(all) => out.extend(all.into_iter().filter_map(|(names, surface)| {
                    let name = names.first()?.parse::<FaceName>().ok()?;
                    wanted(&surface).then_some(FrameEntity::Face(name, surface))
                })),
                Err(_) => {
                    for f in kernel.faces(shape).unwrap_or_default() {
                        if !matches!(
                            f.surface.as_str(),
                            "plane" | "cylinder" | "cone" | "torus" | "sphere"
                        ) {
                            continue;
                        }
                        if let Some(name) = f.names.first().and_then(|n| n.parse::<FaceName>().ok())
                            && let Ok(surface) = kernel.face_geometry(shape, &name)
                        {
                            out.push(FrameEntity::Face(name, surface));
                        }
                        crate::tick();
                    }
                }
            }
        }
        let out = std::sync::Arc::new(out);
        self.frame_entities
            .borrow_mut()
            .insert(body, (version, out.clone()));
        out
    }

    /// Faces and edges of `component`'s bodies at the marker that give a
    /// frame through `target`'s origin along its z axis: circular edges
    /// centred there, planar faces through it across z, faces of
    /// revolution about z (edges first when `edges_first`).
    fn frame_entities(
        &self,
        component: ComponentUid,
        target: &Transform,
        edges_first: bool,
    ) -> Vec<Value> {
        let [o, _, _, z] = parts(target);
        let tol = LINEAR * (1.0 + o.iter().fold(0.0_f64, |m, v| m.max(v.abs())));
        let along =
            |d: Vec3| geom::unit(d).is_some_and(|d| geom::norm(geom::cross(d, z)) <= ANGULAR);
        let on_axis = |p: Vec3, d: Vec3| {
            geom::unit(d)
                .is_some_and(|d| geom::norm(geom::cross(geom::sub(o, p), d)) <= tol && along(d))
        };
        let edge_fits = |geometry: &CurveGeometry| {
            matches!(*geometry, CurveGeometry::Circle { center, normal, .. }
                if geom::distance(center, o) <= tol && along(normal))
        };
        let face_fits = |geometry: &SurfaceGeometry| match *geometry {
            SurfaceGeometry::Plane { origin, normal } => {
                along(normal)
                    && geom::unit(normal)
                        .is_some_and(|n| geom::dot(geom::sub(o, origin), n).abs() <= tol)
            }
            SurfaceGeometry::Cylinder { origin, axis, .. }
            | SurfaceGeometry::Cone { origin, axis, .. } => on_axis(origin, axis),
            SurfaceGeometry::Torus { center, axis, .. } => on_axis(center, axis),
            SurfaceGeometry::Sphere { center, .. } => geom::distance(center, o) <= tol,
            _ => false,
        };
        let (mut edges, mut faces) = (Vec::new(), Vec::new());
        for body in self.doc.component_bodies(component) {
            for entity in self.body_frame_entities(body).iter() {
                match entity {
                    FrameEntity::Edge(name, curve) if edge_fits(curve) => {
                        edges.push(json!({"body": body.to_string(), "edge": name.to_string()}))
                    }
                    FrameEntity::Face(name, surface) if face_fits(surface) => {
                        faces.push(json!({"body": body.to_string(), "face": name.to_string()}))
                    }
                    _ => {}
                }
            }
        }
        let mut out = if edges_first {
            [edges, faces].concat()
        } else {
            [faces, edges].concat()
        };
        out.truncate(MAX_ENTITIES);
        out
    }

    /// Geometry of `component` at the marker that gives `target` (see the
    /// module documentation): `named` first (a joint origin or construction
    /// geometry the side names), then faces and edges, else a fixed plane.
    fn side_geometry(
        &self,
        component: ComponentUid,
        target: &Transform,
        named: Vec<Value>,
        edges_first: bool,
    ) -> SideGeometry {
        // A component of another document: its origin (its geometry is
        // not in the file).
        let inserted = self.components.is_inserted(component);
        let mut candidates = if inserted {
            vec![json!("origin")]
        } else {
            named
        };
        candidates.extend(self.frame_entities(component, target, edges_first));
        if crate::tracing() {
            let [o, _, _, z] = parts(target);
            eprintln!(
                "import: a joint frame at {o:?} along {z:?} in {component}: {} candidates {}",
                candidates.len(),
                Value::Array(candidates.clone())
            );
        }
        for g in candidates {
            crate::tick();
            let Some(resolved) = self.resolved_frame(component, &g, None) else {
                continue;
            };
            let over = override_for(&resolved, target);
            if self
                .resolved_frame(component, &g, over.as_ref())
                .is_some_and(|f| same(&f, target))
            {
                return SideGeometry {
                    geometry: g,
                    frame_override: over,
                    fixed: false,
                    inserted,
                };
            }
        }
        SideGeometry {
            geometry: fixed_plane(target),
            frame_override: None,
            fixed: true,
            inserted: false,
        }
    }

    /// The geometry a `JointGeometry` names that is construction geometry,
    /// or the joint origin a side refers to; and whether its key point is
    /// on an edge.
    fn named_geometry(&self, g: &Value) -> (Vec<Value>, bool) {
        let mut named = Vec::new();
        if g["kind"] == "feature"
            && let Some(uid) = g["timeline_index"]
                .as_i64()
                .and_then(|i| self.features.get(&i))
        {
            named.push(json!(uid.to_string()));
        }
        let f3d = &g["_f3d"];
        let entities = [&g["entityOne"], &g["entityTwo"]].into_iter().chain(
            ["key_points", "directions"]
                .into_iter()
                .flat_map(|k| f3d[k].as_array().into_iter().flatten())
                .map(|p| &p["entity"]),
        );
        for e in entities {
            if let Some(r) = datum_ref(self, e)
                && !named.contains(&r)
            {
                named.push(r);
            }
        }
        let edge = f3d
            .pointer("/key_points/0/entity/kind")
            .or_else(|| g.pointer("/entityOne/kind"))
            .is_some_and(|k| k == "edge");
        (named, edge)
    }

    /// The limits of a joint's motion as Mitcad's (`min`, `max`, `rest`
    /// of the motions they bound), each with its value (mm or rad).
    fn joint_limits(&self, kind: &str, axis: &str, motion: &Value) -> Vec<Limit> {
        let (rotation, slide) = limited_motions(kind, axis);
        let mut out = Vec::new();
        for (key, target) in [("rotationLimits", rotation), ("slideLimits", slide)] {
            let (Some(m), Some(l)) = (target, motion.get(key).filter(|l| l.is_object())) else {
                continue;
            };
            let slot = |value: &str, enabled: &str| -> Option<(Value, f64)> {
                if l[enabled] != true {
                    return None;
                }
                let v = self.value(&reference(&l[value]))?;
                let raw = raw_value(&l[value])?;
                Some((v, if key == "slideLimits" { mm(raw) } else { raw }))
            };
            let limit = Limit {
                motion: m,
                min: slot("minimumValue", "isMinimumValueEnabled"),
                max: slot("maximumValue", "isMaximumValueEnabled"),
                rest: slot("restValue", "isRestValueEnabled"),
            };
            if limit.min.is_some() || limit.max.is_some() || limit.rest.is_some() {
                out.push(limit);
            }
        }
        out
    }

    /// The placement of a path in the joint's component at the marker, or
    /// where the file places its occurrences right after the item at
    /// `index` (`after`; levels without placements of their own where they
    /// are at the marker).
    fn path_world(&self, path: &[OccurrenceUid], index: i64, after: bool) -> Transform {
        let a = self.doc.assembly();
        path.iter().fold(Transform::IDENTITY, |t, o| {
            let at = after
                .then(|| self.captures.placement_after(a, *o, index))
                .flatten()
                .or_else(|| self.doc.placement(*o))
                .unwrap_or(Transform::IDENTITY);
            t.after(&at)
        })
    }

    /// The values a joint's free motions have where the file places its
    /// occurrences (at the marker, else right after its item): Mitcad's
    /// relation `frame_a = frame_b · motion(values) · alignment` with the
    /// sides' frames `local_a`, `local_b` in their paths' components. None:
    /// it holds at neither, with how far side one is from where it would
    /// be held (the alignment alone).
    #[allow(clippy::too_many_arguments)]
    fn joint_fit(
        &self,
        index: i64,
        (kind, slide): (JointKind, SlideAxis),
        (a_path, b_path): (&[OccurrenceUid], &[OccurrenceUid]),
        (local_a, local_b): (&Transform, &Transform),
        alignment: &Transform,
    ) -> Result<BTreeMap<Motion, f64>, String> {
        let mut first_gap = None;
        for after in [false, true] {
            let world_a = self.path_world(a_path, index, after).after(local_a);
            let world_b = self.path_world(b_path, index, after).after(local_b);
            let relative = inverse(&world_b).after(&world_a).after(&inverse(alignment));
            if let Some(values) = motion_values(kind, slide, &relative) {
                return Ok(values);
            }
            first_gap.get_or_insert_with(|| gap(&world_b.after(alignment), &world_a));
        }
        Err(format!(
            "it does not hold where the file places the occurrences: side one is {} from side \
             two{}",
            first_gap.unwrap_or_default(),
            if kind == JointKind::Rigid {
                String::new()
            } else {
                format!(" without its {kind} motion")
            }
        ))
    }

    /// [`Importer::joint_fit`] with the z offset the placements give
    /// instead of the file's (`theta`, `offset`, `flip` its alignment),
    /// where they differ by at most `tolerance` (mm) and nothing else
    /// differs: the values and that offset.
    #[allow(clippy::too_many_arguments)]
    fn rounded_offset_fit(
        &self,
        index: i64,
        (kind, slide): (JointKind, SlideAxis),
        (a_path, b_path): (&[OccurrenceUid], &[OccurrenceUid]),
        (local_a, local_b): (&Transform, &Transform),
        (theta, offset, flip): (f64, f64, bool),
        tolerance: f64,
    ) -> Option<(BTreeMap<Motion, f64>, f64)> {
        let alignment = alignment(theta, offset, flip);
        [false, true].into_iter().find_map(|after| {
            let world_a = self.path_world(a_path, index, after).after(local_a);
            let world_b = self.path_world(b_path, index, after).after(local_b);
            let relative = inverse(&world_b)
                .after(&world_a)
                .after(&inverse(&alignment));
            // relative = motion · Tz(δ): δ along the motion frame's z.
            let r = &relative.linear;
            let t = relative.translation;
            let delta: f64 = (0..3).map(|i| r[i][2] * t[i]).sum();
            if delta.abs() > tolerance {
                return None;
            }
            let motion = relative.after(&Transform::translation([0.0, 0.0, -delta]));
            motion_values(kind, slide, &motion).map(|values| (values, offset + delta))
        })
    }

    /// A joint: as a joint where it holds where the file places the
    /// occurrences, else kept as an as-built joint.
    fn joint_item(&mut self, at: usize, index: i64, item: &TimelineItem) -> Result<(), String> {
        let started = std::time::Instant::now();
        let d = detail(item);
        let motion = &d["jointMotion"];
        let kind = joint_kind(motion).ok_or("its motion was not decoded")?;
        let (component, a_path) =
            self.occurrence_path(item, &d["occurrenceOne"], "side one's occurrence")?;
        let (component_b, b_path) =
            self.occurrence_path(item, &d["occurrenceTwo"], "side two's occurrence")?;
        if component != component_b {
            return Err("its sides start in different components".to_owned());
        }
        if a_path == b_path {
            return Err(same_sides(&a_path));
        }
        // The sides' frames in the components their paths end in; the x and
        // y offsets move side two's origin.
        let frames = d.pointer("/_f3d/frames").and_then(Value::as_array);
        let side_frame = |k: usize, key: &str| {
            frames
                .and_then(|f| f.get(k))
                .and_then(matrix)
                .or_else(|| geometry_frame(&d[key]))
                .unwrap_or(Transform::IDENTITY)
        };
        let frame_a = side_frame(0, "geometryOrOriginOne");
        let shift = [
            raw_value(&d["offsetX"]).map_or(0.0, mm),
            raw_value(&d["offsetY"]).map_or(0.0, mm),
        ];
        let frame_b = side_frame(1, "geometryOrOriginTwo")
            .after(&Transform::translation([shift[0], shift[1], 0.0]));
        let flip = d
            .pointer("/_f3d/opposed")
            .and_then(Value::as_u64)
            .map(|o| o == 1)
            .or_else(|| d["isFlipped"].as_bool())
            .unwrap_or(false);
        let axis = slide_axis(motion);
        let model = model_kind(kind, axis).ok_or("its motion was not decoded")?;
        // The angle as the file has it (mitcad#81: the joint test model's
        // rigid joints at 30°, flipped and not), the z offset as stored.
        let theta = raw_value(&d["angle"]).unwrap_or(0.0);
        let offset = raw_value(&d["offset"]).map_or(0.0, mm);
        let fit = self
            .joint_fit(
                index,
                model,
                (&a_path, &b_path),
                (&frame_a, &frame_b),
                &alignment(theta, offset, flip),
            )
            .map(|values| (values, None))
            .or_else(|why| {
                // The offset the file stores rounded (mitcad#81): taken from
                // the placements where nothing else differs.
                let tolerance = rounding(&d["offset"]).ok_or(why.clone())?;
                self.rounded_offset_fit(
                    index,
                    model,
                    (&a_path, &b_path),
                    (&frame_a, &frame_b),
                    (theta, offset, flip),
                    tolerance,
                )
                .map(|(values, exact)| (values, Some(exact)))
                .ok_or(why)
            });
        let (fit, exact_offset) = match fit {
            Ok(fit) => fit,
            Err(why) => {
                // Nothing to try: no definition can hold (mitcad#87). The
                // sides go on the motion frame of the as-built joint only.
                let (named_b, edge_b) = self.named_geometry(&d["geometryOrOriginTwo"]);
                let side_b = if kind == "rigid" {
                    None
                } else {
                    let end = self.path_end(component, &b_path);
                    Some(self.side_geometry(end, &frame_b, named_b, edge_b))
                };
                let kept = self.keep_as_built(
                    at,
                    index,
                    item,
                    component,
                    (&a_path[..], &b_path[..]),
                    (kind, axis),
                    side_b.as_ref(),
                    &why,
                );
                trace!(
                    "{}: kept as an as-built joint in {:.3} s",
                    item.name().unwrap_or("?"),
                    started.elapsed().as_secs_f64()
                );
                return kept;
            }
        };
        let (named_a, edge_a) = self.named_geometry(&d["geometryOrOriginOne"]);
        let (named_b, edge_b) = self.named_geometry(&d["geometryOrOriginTwo"]);
        let side_a =
            self.side_geometry(self.path_end(component, &a_path), &frame_a, named_a, edge_a);
        let side_b =
            self.side_geometry(self.path_end(component, &b_path), &frame_b, named_b, edge_b);
        let sides_at = started.elapsed().as_secs_f64();

        let mut def = json!({"type": "joint", "kind": kind,
            "a": side_a.origin(&a_path), "b": side_b.origin(&b_path), "flip": flip});
        if kind == "slider" && axis != "z" {
            def["slide_axis"] = json!(axis);
        }
        let offset_value = self.value(&reference(&d["offset"]));
        if let Some(offset) = &offset_value {
            def["offset"] = offset.clone();
        }
        if let Some(angle) = self.value(&reference(&d["angle"])) {
            def["angle"] = angle;
        }
        // A rounded offset: its parameter (else the number) takes the value
        // the placements give.
        let depth = self.doc.undo_depth();
        let mut rounded_note = None;
        if let Some(exact) = exact_offset {
            match &offset_value {
                Some(Value::String(name)) if self.doc.set_parameter(name, exact).is_ok() => {}
                _ => def["offset"] = json!(exact),
            }
            rounded_note = Some(format!(
                "its offset {exact:.6} mm from the placements (the file stores {})",
                d["offset"]["expression"].as_str().unwrap_or("it rounded")
            ));
        }
        // The limits where the values are within them, with the values as
        // the position where the rest value differs; else without them.
        let limits = self.joint_limits(kind, axis, motion);
        let mut defs = Vec::new();
        let mut left_out = None;
        if !limits.is_empty() {
            let mut limited = def.clone();
            let mut out = Map::new();
            let mut position = Map::new();
            let mut outside = None;
            for l in &limits {
                let value = motion_named(l.motion)
                    .and_then(|m| fit.get(&m))
                    .copied()
                    .unwrap_or(0.0);
                let tolerance = 1e-9 * (1.0 + value.abs());
                if l.min.as_ref().is_some_and(|(_, v)| value < v - tolerance)
                    || l.max.as_ref().is_some_and(|(_, v)| value > v + tolerance)
                {
                    outside = Some(format!("{} is {value:.6} there", l.motion));
                }
                let mut limit = Map::new();
                for (k, slot) in [("min", &l.min), ("max", &l.max), ("rest", &l.rest)] {
                    if let Some((v, _)) = slot {
                        limit.insert(k.to_owned(), v.clone());
                    }
                }
                out.insert(l.motion.to_owned(), Value::Object(limit));
                if l.rest
                    .as_ref()
                    .is_some_and(|(_, r)| (r - value).abs() > tolerance)
                {
                    position.insert(l.motion.to_owned(), json!(value));
                }
            }
            limited["limits"] = Value::Object(out);
            if !position.is_empty() {
                limited["position"] = Value::Object(position);
            }
            match outside {
                None => defs.push((limited, None)),
                Some(why) => left_out = Some(format!("outside them: {why}")),
            }
        }
        defs.push((def, (!limits.is_empty()).then_some(())));

        let name = item.name();
        let mut why = String::new();
        let mut tries = 0;
        for (def, without_limits) in defs {
            tries += 1;
            match self.add_unmoved(&def, name, component, index) {
                Ok(uid) => {
                    let mut notes = Vec::new();
                    for (label, side) in [("one", &side_a), ("two", &side_b)] {
                        if side.fixed {
                            notes.push(format!("side {label} on a fixed frame"));
                        }
                        if side.inserted {
                            notes.push(format!(
                                "side {label} on the origin of a component of another \
                                 document"
                            ));
                        }
                    }
                    if without_limits.is_some() {
                        let reason = left_out.take().unwrap_or_else(|| why.clone());
                        notes.push(format!("its limits left out ({reason})"));
                    }
                    if shift.iter().any(|v| *v != 0.0) {
                        notes.push("the x and y offsets fixed in side two's frame".to_owned());
                    }
                    notes.extend(rounded_note.take());
                    let fixed = usize::from(side_a.fixed) + usize::from(side_b.fixed);
                    let candidate = Candidate::new(def).with_note(notes.join("; "));
                    self.accepted(at, index, &[uid], &candidate, None);
                    if fixed > 0 {
                        self.report.items[at].outcome = Outcome::Partial;
                    }
                    self.report.joints.joints += 1;
                    self.report.joints.fixed_sides += fixed;
                    self.report.joints.inserted_sides +=
                        usize::from(side_a.inserted) + usize::from(side_b.inserted);
                    trace!(
                        "{}: a joint in {:.3} s (sides {sides_at:.3} s, {tries} definitions \
                         added)",
                        name.unwrap_or("?"),
                        started.elapsed().as_secs_f64()
                    );
                    return Ok(());
                }
                Err(e) => why = e,
            }
        }
        self.undo_to(depth);
        let kept = self.keep_as_built(
            at,
            index,
            item,
            component,
            (&a_path[..], &b_path[..]),
            (kind, axis),
            Some(&side_b),
            &why,
        );
        trace!(
            "{}: kept as an as-built joint in {:.3} s ({tries} definitions added)",
            name.unwrap_or("?"),
            started.elapsed().as_secs_f64()
        );
        kept
    }

    /// A joint that does not hold where the file places the occurrences,
    /// kept as an as-built joint of its kind (`kind`, a slider's `axis`) at
    /// the stored placements (the motion frame side two's), else as a rigid
    /// one; with a warning.
    #[allow(clippy::too_many_arguments)]
    fn keep_as_built(
        &mut self,
        at: usize,
        index: i64,
        item: &TimelineItem,
        component: ComponentUid,
        (a, b): (&[OccurrenceUid], &[OccurrenceUid]),
        (kind, axis): (&str, &str),
        side_b: Option<&SideGeometry>,
        why: &str,
    ) -> Result<(), String> {
        let mut defs = Vec::new();
        for relative in self.relatives(a, b) {
            let base = json!({"type": "as_built_joint", "a": path_text(a), "b": path_text(b),
                "relative": matrix_rows(&relative)});
            if kind != "rigid"
                && let Some(side_b) = side_b
            {
                let mut def = base.clone();
                def["kind"] = json!(kind);
                if kind == "slider" && axis != "z" {
                    def["slide_axis"] = json!(axis);
                }
                def["origin"] = side_b.origin(b);
                defs.push(def);
            }
            let mut rigid = base;
            rigid["kind"] = json!("rigid");
            defs.push(rigid);
        }
        let mut last = String::new();
        for def in defs {
            match self.add_unmoved(&def, item.name(), component, index) {
                Ok(uid) => {
                    let name = item.name().unwrap_or("?");
                    let note = format!("kept as an as-built joint: {why}");
                    self.report.warnings.push(format!(
                        "{name} does not hold where the file places the occurrences ({why}): \
                         kept as an as-built joint"
                    ));
                    let candidate = Candidate::new(def).with_note(note);
                    self.accepted(at, index, &[uid], &candidate, None);
                    self.report.items[at].outcome = Outcome::Partial;
                    self.report.joints.kept_as_built += 1;
                    return Ok(());
                }
                Err(e) => last = e,
            }
        }
        Err(format!("{why}; as an as-built joint: {last}"))
    }

    /// An as-built joint: the occurrences joined where the file places
    /// them.
    fn as_built_item(&mut self, at: usize, index: i64, item: &TimelineItem) -> Result<(), String> {
        let d = detail(item);
        let motion = &d["jointMotion"];
        let kind = joint_kind(motion).ok_or("its motion was not decoded")?;
        let (component, a_path) =
            self.occurrence_path(item, &d["occurrenceOne"], "occurrence one")?;
        let (component_b, b_path) =
            self.occurrence_path(item, &d["occurrenceTwo"], "occurrence two")?;
        if component != component_b {
            return Err("its occurrences are in different components".to_owned());
        }
        if a_path == b_path {
            return Err(same_sides(&a_path));
        }
        let relatives = self.relatives(&a_path, &b_path);
        let relative = relatives[0];
        // The joint's frame as the file records it in each occurrence's
        // component.
        let records = d.pointer("/_f3d/placements").and_then(Value::as_array);
        let recorded = |k: usize| {
            records
                .and_then(|r| r.get(k))
                .and_then(|r| matrix(&r["frame"]))
        };
        let mut notes = Vec::new();
        let mut at_record = true;
        if let (Some(ra), Some(rb)) = (recorded(0), recorded(1)) {
            let recorded_relative = rb.after(&inverse(&ra));
            if !same(&recorded_relative, &relative) {
                at_record = false;
                let name = item.name().unwrap_or("?");
                self.report.warnings.push(format!(
                    "{name}: the placement it records differs from where the file places the \
                     occurrences ({}); the file's placements are kept",
                    gap(&recorded_relative, &relative)
                ));
                notes.push("the file's placements kept, not the recorded one".to_owned());
            }
        }
        let mut def = json!({"type": "as_built_joint", "kind": kind, "a": path_text(&a_path),
            "b": path_text(&b_path), "relative": matrix_rows(&relative)});
        let axis = slide_axis(motion);
        if kind == "slider" && axis != "z" {
            def["slide_axis"] = json!(axis);
        }
        // Definitions with what taking them adds to the note.
        let mut defs: Vec<(Value, Option<&str>)> = Vec::new();
        if kind != "rigid" {
            // The motion frame: the recorded frame on geometry of occurrence
            // two's component, else of occurrence one's, else fixed on two.
            match recorded(1) {
                Some(rb) => {
                    let (named, edge) = self.named_geometry(&d["geometry"]);
                    let end = self.path_end(component, &b_path);
                    let mut side = self.side_geometry(end, &rb, named.clone(), edge);
                    let mut path = &b_path;
                    if side.fixed
                        && let Some(ra) = recorded(0)
                    {
                        let end = self.path_end(component, &a_path);
                        let other = self.side_geometry(end, &ra, named, edge);
                        if !other.fixed {
                            (side, path) = (other, &a_path);
                        }
                    }
                    if side.fixed {
                        notes.push("its motion frame fixed".to_owned());
                    }
                    def["origin"] = side.origin(path);
                }
                None => notes.push("its motion frame not decoded: occurrence two's".to_owned()),
            }
            // Mitcad's values are 0 where the occurrences are, the file's at
            // the recorded placement: the limits hold where those agree,
            // with 0 as the position where the rest value is another.
            let limits = self.joint_limits(kind, axis, motion);
            if !limits.is_empty() {
                let outside = limits.iter().any(|l| {
                    l.min.as_ref().is_some_and(|(_, v)| *v > 1e-12)
                        || l.max.as_ref().is_some_and(|(_, v)| *v < -1e-12)
                });
                if !at_record {
                    notes.push(
                        "its limits left out (its value is not 0 where the file places the \
                         occurrences)"
                            .to_owned(),
                    );
                } else if outside {
                    notes.push("its limits left out (they do not hold its value 0)".to_owned());
                } else {
                    let mut limited = def.clone();
                    let mut out = Map::new();
                    let mut position = Map::new();
                    for l in &limits {
                        let mut limit = Map::new();
                        for (k, slot) in [("min", &l.min), ("max", &l.max), ("rest", &l.rest)] {
                            if let Some((v, _)) = slot {
                                limit.insert(k.to_owned(), v.clone());
                            }
                        }
                        out.insert(l.motion.to_owned(), Value::Object(limit));
                        if l.rest.as_ref().is_some_and(|(_, r)| r.abs() > 1e-12) {
                            position.insert(l.motion.to_owned(), json!(0.0));
                        }
                    }
                    limited["limits"] = Value::Object(out);
                    if !position.is_empty() {
                        limited["position"] = Value::Object(position);
                    }
                    defs.push((limited, None));
                    defs.push((def.clone(), Some("its limits left out")));
                }
            }
        }
        if defs.is_empty() {
            defs.push((def, None));
        }
        // Where the occurrences are at its point of the timeline, when the
        // file's end placements do not hold there.
        if let Some(now) = relatives.get(1) {
            let at_point: Vec<(Value, Option<&str>)> = defs
                .iter()
                .map(|(d, more)| {
                    let mut d = d.clone();
                    d["relative"] = json!(matrix_rows(now));
                    (
                        d,
                        more.or(Some("the placements at its point of the timeline")),
                    )
                })
                .collect();
            defs.extend(at_point);
        }
        let mut last = String::new();
        for (def, more) in defs {
            match self.add_unmoved(&def, item.name(), component, index) {
                Ok(uid) => {
                    notes.extend(more.map(|m| format!("{m} ({last})")));
                    let candidate = Candidate::new(def).with_note(notes.join("; "));
                    self.accepted(at, index, &[uid], &candidate, None);
                    self.report.joints.as_built += 1;
                    return Ok(());
                }
                Err(e) => last = e,
            }
        }
        Err(last)
    }

    /// A joint origin: a `joint_origin` with the file's angle and z offset
    /// on geometry that gives its frame before them, checked against the
    /// file's frame; else that frame fixed.
    fn joint_origin_item(
        &mut self,
        at: usize,
        index: i64,
        item: &TimelineItem,
    ) -> Result<(), String> {
        let d = detail(item);
        let g = &d["geometry"];
        let target = geometry_frame(g).ok_or("its frame was not decoded")?;
        let component = item
            .f3d
            .as_ref()
            .and_then(|f| f.component)
            .and_then(|c| self.components.of_object(c))
            .unwrap_or(self.component);
        let flip = d["isFlipped"].as_bool().unwrap_or(false);
        let angle = self.value(&reference(&d["angle"]));
        let offset = self.value(&reference(&d["offsetZ"]));
        let base = target.after(&inverse(&alignment(
            raw_value(&d["angle"]).unwrap_or(0.0),
            raw_value(&d["offsetZ"]).map_or(0.0, mm),
            flip,
        )));
        let (named, edge) = self.named_geometry(g);
        let side = self.side_geometry(component, &base, named, edge);
        let mut def = json!({"type": "joint_origin", "geometry": side.geometry});
        if let Some(o) = &side.frame_override {
            def["frame_override"] = o.clone();
        }
        if let Some(a) = angle {
            def["angle"] = a;
        }
        if let Some(o) = offset {
            def["offset"] = o;
        }
        if flip {
            def["flip"] = json!(true);
        }
        let fixed = json!({"type": "joint_origin", "geometry": fixed_plane(&target)});
        let mut last = String::new();
        for (def, note) in [
            (def, side.fixed.then_some("on a fixed frame")),
            (
                fixed,
                Some("a fixed frame (its offsets and angle left out)"),
            ),
        ] {
            let depth = self.doc.undo_depth();
            match self.add_in(&def, item.name(), component) {
                Ok(uid) => {
                    let plane = match self.doc.datum(uid) {
                        Some(Datum::Plane(p)) => {
                            frame(p.origin, p.x_axis, geom::cross(p.x_axis, p.y_axis))
                        }
                        _ => None,
                    };
                    if !plane.is_some_and(|p| same(&p, &target)) {
                        self.undo_to(depth);
                        last = "its frame is not the file's".to_owned();
                        continue;
                    }
                    let candidate = Candidate::new(def).with_note(note.unwrap_or_default());
                    self.accepted(at, index, &[uid], &candidate, None);
                    if note.is_some() {
                        self.report.items[at].outcome = Outcome::Partial;
                    }
                    self.report.joints.origins += 1;
                    return Ok(());
                }
                Err(e) => last = e,
            }
        }
        Err(last)
    }

    /// A captured position: a `capture_position` feature in each
    /// component whose occurrences it places, at the file's placements
    /// (`captures.rs`); `partial` where some are left out.
    fn capture_item(&mut self, at: usize, index: i64, item: &TimelineItem) -> Result<(), String> {
        let captured = self
            .captures
            .items
            .get(&index)
            .cloned()
            .ok_or("its positions were not decoded")?;
        let mut left_out = captured.left_out;
        if captured.positions.is_empty() {
            return Err(if left_out.is_empty() {
                "it places no occurrence".to_owned()
            } else {
                counted(&left_out)
            });
        }
        // The occurrences by the component they are placed in.
        let mut groups: BTreeMap<ComponentUid, Vec<Value>> = BTreeMap::new();
        for (o, t) in &captured.positions {
            let Some(parent) = self.doc.assembly().occurrence(*o).map(|x| x.parent) else {
                continue;
            };
            groups
                .entry(parent)
                .or_default()
                .push(json!({"occurrence": o.to_string(), "transform": matrix_rows(t)}));
        }
        let mut uids = Vec::new();
        let mut last = None;
        for (component, positions) in groups {
            let def = json!({"type": "capture_position", "positions": positions});
            match self.add_in(&def, item.name(), component) {
                Ok(uid) => uids.push(uid),
                Err(e) => {
                    let name = self.doc.assembly().name(component);
                    left_out.push(format!("the positions in {name}: {e}"));
                    last = Some(e);
                }
            }
        }
        if uids.is_empty() {
            return Err(last.unwrap_or_else(|| "it places no occurrence".to_owned()));
        }
        let note = if left_out.is_empty() {
            String::new()
        } else {
            format!("left out: {}", counted(&left_out))
        };
        let candidate = Candidate::new(json!({"type": "capture_position"})).with_note(note);
        self.accepted(at, index, &uids, &candidate, None);
        if !left_out.is_empty() {
            self.report.items[at].outcome = Outcome::Partial;
        }
        self.report.joints.positions += 1;
        Ok(())
    }

    /// A rigid group (mitcad#81): a `rigid_group` of the occurrences of its
    /// component its members are or are in (a joint moves the top-level
    /// occurrence of a path; members inside others, listed with "include
    /// children", and inside components of other documents move with
    /// them). A group with the component's own geometry joins its
    /// occurrences to it by rigid as-built joints.
    fn rigid_group_item(
        &mut self,
        at: usize,
        index: i64,
        item: &TimelineItem,
    ) -> Result<(), String> {
        let d = detail(item);
        let members = d["occurrences"]
            .as_array()
            .ok_or("its occurrences were not decoded")?;
        let mut component = None;
        let mut tops: Vec<OccurrenceUid> = Vec::new();
        let (mut own, mut below, mut inserted) = (false, 0, 0);
        for (k, m) in members.iter().enumerate() {
            let (c, path) = match self.occurrence_path(item, m, &format!("member {}", k + 1)) {
                Ok(found) => found,
                Err(e) if e.contains("inside a component of another document") => {
                    inserted += 1;
                    continue;
                }
                Err(e) => return Err(e),
            };
            if component.is_some_and(|x| x != c) {
                return Err("its members start in different components".to_owned());
            }
            component = Some(c);
            match path.first() {
                None => own = true,
                Some(top) => {
                    below += usize::from(path.len() > 1);
                    if !tops.contains(top) {
                        tops.push(*top);
                    }
                }
            }
        }
        let component = component.ok_or("it has no members")?;
        let mut notes = Vec::new();
        if below > 0 {
            notes.push(format!(
                "{below} of its members below the top level move with the occurrences they \
                 are in"
            ));
        }
        if inserted > 0 {
            notes.push(format!(
                "{inserted} of its members inside components of other documents move with \
                 them"
            ));
        }
        let mut uids = Vec::new();
        let depth = self.doc.undo_depth();
        let def = if own {
            notes.push("as rigid as-built joints to its component's own geometry".to_owned());
            for top in &tops {
                let relative = self.relatives(&[*top], &[])[0];
                let def = json!({"type": "as_built_joint", "kind": "rigid",
                    "a": path_text(&[*top]), "b": "", "relative": matrix_rows(&relative)});
                match self.add_unmoved(&def, item.name(), component, index) {
                    Ok(uid) => uids.push(uid),
                    Err(e) => {
                        self.undo_to(depth);
                        return Err(e);
                    }
                }
            }
            json!({"type": "as_built_joint", "kind": "rigid"})
        } else {
            if tops.len() < 2 {
                return Err(format!(
                    "it joins {} occurrence{} of its component",
                    tops.len(),
                    if tops.len() == 1 { "" } else { "s" }
                ));
            }
            let occurrences: Vec<Value> = tops.iter().map(|o| json!(o.to_string())).collect();
            let def = json!({"type": "rigid_group", "occurrences": occurrences});
            uids.push(self.add_unmoved(&def, item.name(), component, index)?);
            def
        };
        if uids.is_empty() {
            return Err("it joins no occurrence of its component".to_owned());
        }
        let candidate = Candidate::new(def).with_note(notes.join("; "));
        self.accepted(at, index, &uids, &candidate, None);
        self.report.joints.rigid_groups += 1;
        Ok(())
    }

    /// A ground item: its occurrence grounded.
    fn ground_item(&mut self, at: usize, item: &TimelineItem) -> Result<(), String> {
        let d = detail(item);
        let (_, path) = self.occurrence_path(item, &d["occurrence"], "the grounded occurrence")?;
        let o = *path.last().ok_or("it grounds no occurrence")?;
        let grounded = self
            .doc
            .assembly()
            .occurrence(o)
            .is_some_and(|x| x.grounded);
        if !grounded {
            self.doc
                .set_occurrence_grounded(o, true)
                .map_err(|e| e.to_string())?;
        }
        let name = self.doc.assembly().occurrence_name(o);
        self.note(at, Outcome::Parametric, format!("grounds {name}"));
        self.report.joints.grounded += 1;
        Ok(())
    }
}
