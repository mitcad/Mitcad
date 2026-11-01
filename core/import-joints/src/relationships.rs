// SPDX-License-Identifier: MIT
//! The file's assembly relationships as MitCad's constraint sets
//! (mitcad#155; what the decoder gives: SCHEMA.md §5.3
//! `GeometricRelationship`, `TangentRelationship`).
//!
//! A relationship item becomes one `constraint_set` of its component, a
//! row per constraint the decoder reads. Like joints (`joints.rs`), a set
//! comes in only where it holds where the file places the occurrences:
//! after it is added every occurrence must be where it was, where the
//! file places it after the item or where it ends, within [`HELD`];
//! else the rows are added one by one and those that would move an
//! occurrence (or that the solver cannot hold with the others) are left
//! out with the reason.
//!
//! - **Sides.** The file's frames are in the components the sides' paths
//!   end in. A side goes on geometry of that component at the item's point
//!   of the timeline that gives the frame's plane, axis, circle or point
//!   (planar faces through the frame's origin across its z axis,
//!   cylindrical and conical faces about it, circular edges centred on it
//!   along it, spheres centred on it), the one nearest the point the file
//!   records on the picked face or edge first; else a fixed plane, axis or
//!   point with the frame (counted, the item `partial`). A side on a
//!   component of another document (an empty component here) is fixed
//!   geometry of that component with the file's frame (counted). A side
//!   inside a component of another document is left out.
//! - **Rows.** Side two is the row's `a`, side one its `b` (the file
//!   measures side two's plane from side one's), except that a point or an
//!   axis on a plane is `a` and the plane `b`:
//!   - two planes, or a plane and a circle: `align` (the file's offset
//!     along `b`'s normal);
//!   - a point or an axis and a plane: `align` (the point, the axis at
//!     the offset above the plane);
//!   - two axes, an axis and a circle: `align` (concentric; the file's
//!     offset between axes is 0 in every file read);
//!   - two circles: `insert` (the centres together, the planes the offset
//!     apart);
//!   - a centring of two faces on each side: `center` (side one's faces'
//!     middle on the plane midway between side two's);
//!   - an angle: `angle`.
//!
//!   `flip` and the offset's sign are those of the references as MitCad
//!   resolves them where the file places the occurrences (the
//!   `constraint_row` query's measures): a face's normal may point the
//!   other way than the file's frame, and the file's sense `either` is
//!   one of the two there. Planes and circles must be parallel there, and
//!   the offset the parameter or its negative (within [`HELD`]), else the
//!   row does not hold.
//! - **Tangent relationships.** A `tangent` row between the planar,
//!   cylindrical or spherical faces of the sides' components that the
//!   file's points lie on (`flip` where the bodies touch inside), added as
//!   a set is; else left out with the reason (MitCad's tangent rows take
//!   only those faces).

use serde_json::{Map, Value, json};

use mitcad_dump::{Reference as ParamRef, TimelineItem};
use mitcad_model::constraints::RowReference;
use mitcad_model::datum::{CurveGeometry, Datum, DatumPoint, SurfaceGeometry};
use mitcad_model::features::{FrameOverride, GeomRef};
use mitcad_model::{ComponentUid, OccurrenceUid, TopoName, Transform};

use mitcad_import_base::geom::{self, mm, mm3};
use mitcad_import_replay::frames::FrameEntity;
use mitcad_import_replay::report::Outcome;
use mitcad_import_replay::{Candidate, ImportKernel, Importer, InputProvenance};

use crate::joints::{ANGULAR, Joints, LINEAR, detail, frame, parts, path_text, same_sides, vec3};

use mitcad_vec3::Vec3;

/// How far, mm, an occurrence may be from where the file places it for a
/// row to hold there, and its offset from the file's: the file's placements
/// and frames agree to a few 1e-5 mm in the designs read.
const HELD: f64 = 1e-4;
/// How far, rad, the file's parallel planes and axes may be turned from
/// each other where it places the occurrences (a few 1e-6 rad in the
/// designs read), and an occurrence from where the file places it.
const TURNED: f64 = 1e-5;

/// What a frame of the file stands for.
///
/// `pub` only because the [`Relationships`] trait names it; the crate
/// does not export it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Point,
    Axis,
    Plane,
    Circle,
}

impl Kind {
    fn of(name: &str) -> Option<Kind> {
        Some(match name {
            "point" => Kind::Point,
            "axis" => Kind::Axis,
            "plane" => Kind::Plane,
            "circle" => Kind::Circle,
            _ => return None,
        })
    }

    fn as_str(self) -> &'static str {
        match self {
            Kind::Point => "point",
            Kind::Axis => "axis",
            Kind::Plane => "plane",
            Kind::Circle => "circle",
        }
    }
}

/// A reference found for one of the file's frames.
///
/// `pub` only because the [`Relationships`] trait names it; the crate
/// does not export it.
pub struct Reference {
    /// The row's reference (`a`, `b`, ...).
    origin: Value,
    /// Fixed geometry: nothing of the component gives the frame.
    fixed: bool,
    /// On a component of another document.
    inserted: bool,
}

/// One side of a row: the component the paths start in, the path and the
/// component it ends in.
///
/// `pub` only because the [`Relationships`] trait names it; the crate
/// does not export it.
pub struct Side {
    path: Vec<OccurrenceUid>,
    end: ComponentUid,
}

/// A row ready to add, with what the report counts.
///
/// `pub` only because the [`Relationships`] trait names it; the crate
/// does not export it.
pub struct Row {
    def: Value,
    fixed: usize,
    inserted: usize,
}

/// A row's reference (`Reference::origin`) as the model checks it: the
/// fields of the `constraint_row` query's `a` and `b` (mitcad#778).
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RowSide {
    #[serde(default)]
    occurrence: String,
    geometry: GeomRef,
    #[serde(default)]
    frame_override: Option<FrameOverride>,
}

impl RowSide {
    fn reference(&self) -> RowReference<'_> {
        RowReference {
            occurrence: &self.occurrence,
            geometry: &self.geometry,
            frame_override: self.frame_override,
        }
    }
}

/// The fixed geometry with a frame's plane, axis or point.
fn fixed(kind: Kind, t: &Transform) -> Option<Value> {
    let [o, x, _, z] = parts(t);
    Some(match kind {
        Kind::Plane => json!({"origin": o, "normal": z, "x_axis": x}),
        Kind::Axis => json!({"origin": o, "direction": z}),
        Kind::Point => json!({"point": o}),
        // No fixed circle: a circle needs an edge.
        Kind::Circle => return None,
    })
}

/// A frame of the file (a 4x4 matrix, cm) in millimetres: its origin, x
/// and z axes. Some of the file's frames turn x, y and z all round (a
/// mirror image); only the origin and the z axis (and x for a plane's
/// fixed geometry) are used.
fn file_frame(v: &Value) -> Option<Transform> {
    let m: [[f64; 4]; 4] = serde_json::from_value(v.clone()).ok()?;
    let col = |c: usize| [m[0][c], m[1][c], m[2][c]];
    frame(mm3(col(3)), col(0), col(2))
}

/// The negative of a value (a parameter's name or a number).
fn negated(v: &Value) -> Value {
    match v {
        Value::String(name) => json!(format!("-({name})")),
        Value::Number(n) => json!(-n.as_f64().unwrap_or(0.0)),
        other => other.clone(),
    }
}

/// The assembly relationships family of the replay (mitcad#155,
/// mitcad#785): relationship and tangent items as constraint sets. The
/// joints family reaches it through `Joints::assembly_item`.
pub trait Relationships<K: ImportKernel> {
    /// A relationship item: its constraints as one constraint set.
    fn relationship_item(&mut self, at: usize, index: i64, item: &TimelineItem);

    /// A row's definition in its component, or why it is left out.
    fn relation_row(&self, item: &TimelineItem, row: &Value)
    -> Result<(ComponentUid, Row), String>;

    /// Adds a constraint set to `component`; kept when every occurrence
    /// stays where it was, where the file places it after the item or
    /// where it ends, within `HELD` (else taken back, with how far one
    /// moved).
    fn add_held(
        &mut self,
        def: &Value,
        name: Option<&str>,
        component: ComponentUid,
        index: i64,
    ) -> Result<mitcad_model::FeatureUid, String>;

    /// What the `constraint_row` query measures between two references
    /// where the occurrences are (mitcad#778: asked of the model, without
    /// the api).
    fn measures(
        &self,
        component: ComponentUid,
        a: &Value,
        b: &Value,
    ) -> Result<Map<String, Value>, String>;

    /// The reference for one of the file's frames on a side.
    fn reference(
        &self,
        side: &Side,
        kind: Kind,
        frame: &Transform,
        point: Option<Vec3>,
    ) -> Result<Reference, String>;

    /// Faces and edges of `component`'s bodies at the marker that give the
    /// frame's plane, axis, circle or point, those nearest `point` first.
    fn geometry_for(
        &self,
        component: ComponentUid,
        kind: Kind,
        frame: &Transform,
        point: Option<Vec3>,
    ) -> Vec<Value>;

    /// How far a point (the component's coordinates, mm) is from a face or
    /// edge of a body, where the kernel measures it.
    fn distance_to(&self, body: mitcad_model::BodyUid, name: &TopoName, p: Vec3) -> Option<f64>;

    /// A tangent relationship item: a `tangent` row between the faces the
    /// file's points are on, where they are planar, cylindrical or
    /// spherical.
    fn tangent_item(&mut self, at: usize, index: i64, item: &TimelineItem);

    fn tangent_set(&mut self, at: usize, index: i64, item: &TimelineItem) -> Result<(), String>;

    /// A planar, cylindrical or spherical face of `component`'s bodies at
    /// the marker that `point` lies on, with its kind.
    fn face_at(
        &self,
        component: ComponentUid,
        point: Vec3,
    ) -> Option<(Value, &'static str, SurfaceGeometry)>;
}

impl<K: ImportKernel> Relationships<K> for Importer<'_, K> {
    fn relationship_item(&mut self, at: usize, index: i64, item: &TimelineItem) {
        let d = detail(item);
        let rows = d["rows"].as_array().cloned().unwrap_or_default();
        self.report.relationships.rows += rows.len();
        if rows.is_empty() {
            self.report
                .relationships
                .skipped("its constraints were not decoded");
            self.note(at, Outcome::Skipped, "its constraints were not decoded");
            return;
        }
        // Each row's definition, or why it is left out.
        let mut component = None;
        let mut ready: Vec<(usize, Row)> = Vec::new();
        let mut reasons: Vec<String> = Vec::new();
        for (k, row) in rows.iter().enumerate() {
            match self.relation_row(item, row) {
                Ok((c, r)) if component.is_none_or(|x| x == c) => {
                    component = Some(c);
                    ready.push((k, r));
                }
                Ok(_) => reasons.push(format!(
                    "row {}: its sides start in another component than row 1's",
                    k + 1
                )),
                Err(why) => {
                    self.report.relationships.skipped(&why_short(&why));
                    reasons.push(format!("row {}: {why}", k + 1));
                }
            }
        }
        let Some(component) = component else {
            self.note(at, Outcome::Skipped, reasons.join("; "));
            return;
        };
        // All rows, else as many as hold together (in order).
        let name = item.name();
        let set = |rows: &[&(usize, Row)]| {
            json!({"type": "constraint_set",
                "rows": rows.iter().map(|(_, r)| r.def.clone()).collect::<Vec<_>>()})
        };
        let all: Vec<&(usize, Row)> = ready.iter().collect();
        let mut uid = self.add_held(&set(&all), name, component, index).ok();
        let mut kept = if uid.is_some() { all } else { Vec::new() };
        if uid.is_none() {
            // The rows one by one onto those that held (each trial taken
            // back), then the set of those.
            for r in &ready {
                let mut trial = kept.clone();
                trial.push(r);
                let depth = self.doc.undo_depth();
                match self.add_held(&set(&trial), name, component, index) {
                    Ok(_) => {
                        self.undo_to(depth);
                        kept = trial;
                    }
                    Err(why) => {
                        self.report.relationships.skipped(&why_short(&why));
                        reasons.push(format!("row {}: {why}", r.0 + 1));
                    }
                }
            }
            if !kept.is_empty() {
                uid = self.add_held(&set(&kept), name, component, index).ok();
            }
        }
        let Some(uid) = uid else {
            self.note(at, Outcome::Skipped, reasons.join("; "));
            return;
        };
        let r = &mut self.report.relationships;
        r.sets += 1;
        r.translated += kept.len();
        let fixed: usize = kept.iter().map(|(_, r)| r.fixed).sum();
        r.fixed_sides += fixed;
        r.inserted_sides += kept.iter().map(|(_, r)| r.inserted).sum::<usize>();
        let mut notes = Vec::new();
        if fixed > 0 {
            notes.push(format!("{fixed} sides on fixed geometry"));
        }
        notes.extend(reasons.iter().cloned());
        let def = set(&kept);
        // The rows' kinds, sides and values are the file's; the flip of a
        // row and the sign of an offset are chosen by where the file places
        // the occurrences, and each side's geometry is found by its frame.
        let candidate = Candidate::new(def)
            .with_note(notes.join("; "))
            .with_provenance(InputProvenance::Inferred);
        self.accepted(at, index, &[uid], &candidate, None);
        if fixed > 0 || kept.len() < rows.len() {
            self.report.items[at].outcome = Outcome::Partial;
        }
    }

    fn relation_row(
        &self,
        item: &TimelineItem,
        row: &Value,
    ) -> Result<(ComponentUid, Row), String> {
        let kind = row["type"]
            .as_str()
            .ok_or("its constraint was not decoded")?;
        let geometry: Vec<Kind> = row["geometry"]
            .as_array()
            .map(|g| {
                g.iter()
                    .filter_map(|k| k.as_str().and_then(Kind::of))
                    .collect()
            })
            .unwrap_or_default();
        let frames: Vec<Transform> = row
            .pointer("/_f3d/frames")
            .and_then(Value::as_array)
            .map(|f| f.iter().filter_map(file_frame).collect())
            .unwrap_or_default();
        let points: Vec<Vec3> = row
            .pointer("/_f3d/points")
            .and_then(Value::as_array)
            .map(|p| p.iter().filter_map(vec3).map(mm3).collect())
            .unwrap_or_default();
        let n = frames.len();
        if n < 2 || geometry.len() != n {
            return Err("its geometry was not decoded".to_owned());
        }
        let (component, one) =
            self.occurrence_path(item, &row["occurrenceOne"], "side one's occurrence")?;
        let (component_two, two) =
            self.occurrence_path(item, &row["occurrenceTwo"], "side two's occurrence")?;
        if component != component_two {
            return Err("its sides start in different components".to_owned());
        }
        if one == two {
            return Err(same_sides(&one));
        }
        let sides = [
            Side {
                end: self.path_end(component, &one),
                path: one,
            },
            Side {
                end: self.path_end(component, &two),
                path: two,
            },
        ];
        // The frame's side: a centring's first two are side one's.
        let side_of = |k: usize| if n == 4 { k / 2 } else { k };
        let mut refs = Vec::new();
        for k in 0..n {
            let side = &sides[side_of(k)];
            refs.push(self.reference(side, geometry[k], &frames[k], points.get(k).copied())?);
        }
        let fixed = refs.iter().filter(|r| r.fixed).count();
        let inserted = refs.iter().filter(|r| r.inserted).count();
        let value = |key: &str| {
            row.get(key)
                .and_then(|r| serde_json::from_value::<ParamRef>(r.clone()).ok())
                .and_then(|r| self.value(&Some(r)))
        };
        let file_value = |key: &str| {
            row.pointer(&format!("/{key}/value"))
                .and_then(Value::as_f64)
        };
        let mut def = Map::new();
        let pair = (geometry[0], geometry[1]);
        let r = |k: usize| refs[k].origin.clone();
        match kind {
            "center" if n == 4 => {
                def.insert("type".into(), json!("center"));
                def.insert("a".into(), r(0));
                def.insert("a2".into(), r(1));
                def.insert("b".into(), r(2));
                def.insert("b2".into(), r(3));
            }
            "align" | "angle" if n == 2 => {
                // Side two is a, side one b; a point or an axis on a plane
                // is a and the plane b.
                let (a, b) = match pair {
                    (Kind::Point | Kind::Axis, Kind::Plane) => (0, 1),
                    _ => (1, 0),
                };
                def.insert("a".into(), r(a));
                def.insert("b".into(), r(b));
                let measures = self.measures(component, &r(a), &r(b))?;
                let angle = measures.get("angle").and_then(Value::as_f64).unwrap_or(0.0);
                let same_way = angle < std::f64::consts::FRAC_PI_2;
                let row_type = match (kind, pair) {
                    ("angle", _) => "angle",
                    (_, (Kind::Circle, Kind::Circle)) => "insert",
                    _ => "align",
                };
                def.insert("type".into(), json!(row_type));
                match row_type {
                    "angle" => {
                        let want = file_value("angle").unwrap_or(0.0);
                        let flip = if (angle - want).abs() <= ANGULAR.sqrt() {
                            false
                        } else if (std::f64::consts::PI - angle - want).abs() <= ANGULAR.sqrt() {
                            true
                        } else {
                            return Err(format!(
                                "it does not hold where the file places the occurrences: the \
                                 angle is {:.6} rad there",
                                angle
                            ));
                        };
                        if let Some(v) = value("angle") {
                            def.insert("angle".into(), v);
                        }
                        if flip {
                            def.insert("flip".into(), json!(true));
                        }
                    }
                    _ => {
                        let planar = matches!(pair.0, Kind::Plane | Kind::Circle)
                            && matches!(pair.1, Kind::Plane | Kind::Circle);
                        let on_plane = matches!(pair, (Kind::Point | Kind::Axis, Kind::Plane));
                        let axial = !planar && !on_plane;
                        // Planes, circles and axes parallel there.
                        let off = angle.min(std::f64::consts::PI - angle);
                        if (planar || axial) && off > TURNED {
                            return Err(format!(
                                "it does not hold where the file places the occurrences: the \
                                 {} are at {angle:.6} rad there",
                                if axial { "axes" } else { "normals" }
                            ));
                        }
                        // The same way (flip) or facing.
                        if planar && same_way {
                            def.insert("flip".into(), json!(true));
                        }
                        let want = file_value("offset").map_or(0.0, mm);
                        if axial {
                            if want.abs() > LINEAR {
                                return Err("an offset between axes".to_owned());
                            }
                        } else {
                            let measured = measures
                                .get("offset")
                                .and_then(Value::as_f64)
                                .ok_or("no offset measured")?;
                            let tol = HELD + 1e-6 * (measured.abs() + want.abs());
                            let v = value("offset").unwrap_or(json!(want));
                            if (measured - want).abs() <= tol {
                                if want != 0.0 {
                                    def.insert("offset".into(), v);
                                }
                            } else if (measured + want).abs() <= tol {
                                def.insert("offset".into(), negated(&v));
                            } else {
                                return Err(format!(
                                    "it does not hold where the file places the occurrences: \
                                     the offset is {measured:.6} mm there, the file's {want:.6} mm"
                                ));
                            }
                        }
                    }
                }
            }
            _ => {
                return Err(format!(
                    "{kind} of {} is no constraint row of MitCad's",
                    geometry
                        .iter()
                        .map(|g| g.as_str())
                        .collect::<Vec<_>>()
                        .join(" and ")
                ));
            }
        }
        Ok((
            component,
            Row {
                def: Value::Object(def),
                fixed,
                inserted,
            },
        ))
    }

    fn add_held(
        &mut self,
        def: &Value,
        name: Option<&str>,
        component: ComponentUid,
        index: i64,
    ) -> Result<mitcad_model::FeatureUid, String> {
        let depth = self.doc.undo_depth();
        let before = self.placements();
        let uid = self.add_in(def, name, component)?;
        let a = self.doc.assembly();
        let near = |p: &Transform, q: &Transform| {
            geom::distance(p.translation, q.translation) <= HELD
                && (0..3).all(|i| (0..3).all(|j| (p.linear[i][j] - q.linear[i][j]).abs() <= TURNED))
        };
        let moved = a.occurrences.iter().find_map(|o| {
            let p = self.doc.placement(o.uid)?;
            let file = self.captures.final_placement(o.uid, o.transform);
            let kept = before.get(&o.uid).is_some_and(|b| near(&p, b))
                || near(&p, &file)
                || self
                    .captures
                    .placement_after(a, o.uid, index)
                    .is_some_and(|t| near(&p, &t));
            (!kept).then(|| {
                format!(
                    "it moves {} by {}",
                    a.occurrence_name(o.uid),
                    crate::joints::gap(&p, &file)
                )
            })
        });
        if let Some(why) = moved {
            self.undo_to(depth);
            return Err(why);
        }
        Ok(uid)
    }

    fn measures(
        &self,
        component: ComponentUid,
        a: &Value,
        b: &Value,
    ) -> Result<Map<String, Value>, String> {
        let references = |e: String| format!("its references: {e}");
        if !self.doc.assembly().exists(component) {
            return Err(references(format!("no component {component}")));
        }
        let side = |v: &Value| {
            serde_json::from_value::<RowSide>(v.clone())
                .map_err(|e| references(format!("invalid query: {e}")))
        };
        let (a, b) = (side(a)?, side(b)?);
        let row = self
            .doc
            .constraint_row(component, &a.reference(), &b.reference(), None, None, None)
            .map_err(references)?;
        Ok(row
            .measures
            .iter()
            .map(|(k, v)| ((*k).to_owned(), json!(v)))
            .collect())
    }

    fn reference(
        &self,
        side: &Side,
        kind: Kind,
        frame: &Transform,
        point: Option<Vec3>,
    ) -> Result<Reference, String> {
        let with_path = |geometry: Value| {
            let mut v = json!({"geometry": geometry});
            if !side.path.is_empty() {
                v["occurrence"] = json!(path_text(&side.path));
            }
            v
        };
        if self.components.is_inserted(side.end) && !self.components.is_filled(side.end) {
            let g = fixed(kind, frame)
                .ok_or("a circle on a component of another document (no edge of it here)")?;
            return Ok(Reference {
                origin: with_path(g),
                fixed: false,
                inserted: true,
            });
        }
        match self
            .geometry_for(side.end, kind, frame, point)
            .into_iter()
            .next()
        {
            Some(g) => Ok(Reference {
                origin: with_path(g),
                fixed: false,
                inserted: false,
            }),
            None => {
                let g = fixed(kind, frame)
                    .ok_or("no circular edge of the component is where the file's circle is")?;
                // Nothing of a component filled from another document is
                // where the file's frame is (an origin plane of that
                // document, say): fixed geometry of that component, as of
                // an empty one.
                let inserted =
                    self.components.is_inserted(side.end) || self.doc.in_import_scope(side.end);
                Ok(Reference {
                    origin: with_path(g),
                    fixed: !inserted,
                    inserted,
                })
            }
        }
    }

    fn geometry_for(
        &self,
        component: ComponentUid,
        kind: Kind,
        frame: &Transform,
        point: Option<Vec3>,
    ) -> Vec<Value> {
        let [o, _, _, z] = parts(frame);
        let tol = LINEAR * (1.0 + o.iter().fold(0.0_f64, |m, v| m.max(v.abs())));
        let along =
            |d: Vec3| geom::unit(d).is_some_and(|d| geom::norm(geom::cross(d, z)) <= ANGULAR);
        let on_axis = |p: Vec3, d: Vec3| {
            geom::unit(d)
                .is_some_and(|d| geom::norm(geom::cross(geom::sub(o, p), d)) <= tol && along(d))
        };
        let mut found: Vec<(f64, Value)> = Vec::new();
        for body in self.doc.component_bodies(component) {
            for entity in self.body_frame_entities(body).iter() {
                let (fits, name) = match (kind, entity) {
                    (
                        Kind::Plane,
                        FrameEntity::Face(name, SurfaceGeometry::Plane { origin, normal }),
                    ) => (
                        along(*normal)
                            && geom::unit(*normal)
                                .is_some_and(|n| geom::dot(geom::sub(o, *origin), n).abs() <= tol),
                        TopoName::Face(name.clone()),
                    ),
                    (
                        Kind::Axis,
                        FrameEntity::Face(
                            name,
                            SurfaceGeometry::Cylinder { origin, axis, .. }
                            | SurfaceGeometry::Cone { origin, axis, .. },
                        ),
                    ) => (on_axis(*origin, *axis), TopoName::Face(name.clone())),
                    (
                        Kind::Circle,
                        FrameEntity::Edge(name, CurveGeometry::Circle { center, normal, .. }),
                    ) => (
                        geom::distance(*center, o) <= tol && along(*normal),
                        TopoName::Edge(name.clone()),
                    ),
                    (
                        Kind::Point,
                        FrameEntity::Face(name, SurfaceGeometry::Sphere { center, .. }),
                    ) => (
                        geom::distance(*center, o) <= tol,
                        TopoName::Face(name.clone()),
                    ),
                    _ => continue,
                };
                if !fits {
                    continue;
                }
                let distance = point
                    .and_then(|p| self.distance_to(body, &name, p))
                    .unwrap_or(f64::INFINITY);
                let v = match &name {
                    TopoName::Face(f) => json!({"body": body.to_string(), "face": f.to_string()}),
                    TopoName::Edge(e) => json!({"body": body.to_string(), "edge": e.to_string()}),
                    TopoName::Vertex(_) => continue,
                };
                found.push((distance, v));
                mitcad_import_replay::tick();
            }
        }
        found.sort_by(|a, b| a.0.total_cmp(&b.0));
        found.into_iter().map(|(_, v)| v).collect()
    }

    fn distance_to(&self, body: mitcad_model::BodyUid, name: &TopoName, p: Vec3) -> Option<f64> {
        let kernel = self.doc.kernel();
        let shape = mitcad_import_base::body_scope::shape(self.doc, body)?;
        let point = kernel
            .datum_shape(&Datum::Point(DatumPoint { point: p }), 0.0)
            .ok()?;
        let a = mitcad_model::analysis::Selection {
            shape,
            name: Some(name),
        };
        let b = mitcad_model::analysis::Selection {
            shape: &point,
            name: None,
        };
        kernel.measure_between(&a, &b).ok().map(|s| s.distance)
    }

    fn tangent_item(&mut self, at: usize, index: i64, item: &TimelineItem) {
        self.report.relationships.tangents += 1;
        if let Err(why) = self.tangent_set(at, index, item) {
            self.report.relationships.skipped(&why_short(&why));
            self.note(at, Outcome::Skipped, why);
        }
    }

    fn tangent_set(&mut self, at: usize, index: i64, item: &TimelineItem) -> Result<(), String> {
        let d = detail(item);
        let (component, one) =
            self.occurrence_path(item, &d["occurrenceOne"], "side one's occurrence")?;
        let (component_two, two) =
            self.occurrence_path(item, &d["occurrenceTwo"], "side two's occurrence")?;
        if component != component_two {
            return Err("its sides start in different components".to_owned());
        }
        if one == two {
            return Err(same_sides(&one));
        }
        let mut refs = Vec::new();
        let mut surfaces = Vec::new();
        for (label, path, key) in [("one", &one, "pointOne"), ("two", &two, "pointTwo")] {
            let point = vec3(&d[key])
                .map(mm3)
                .ok_or("its points were not decoded")?;
            let end = self.path_end(component, path);
            if self.components.is_inserted(end) && !self.components.is_filled(end) {
                return Err(format!(
                    "side {label} is on a component of another document (no face of it here)"
                ));
            }
            let (face, kind, surface) = self.face_at(end, point).ok_or_else(|| {
                format!(
                    "side {label}'s face at the file's point is not planar, cylindrical or \
                     spherical (MitCad's tangent takes those)"
                )
            })?;
            let mut v = json!({"geometry": face});
            if !path.is_empty() {
                v["occurrence"] = json!(path_text(path));
            }
            refs.push((v, kind));
            surfaces.push((surface, path.clone()));
        }
        let mut last = String::new();
        for flip in [false, true] {
            let mut row = json!({"type": "tangent", "a": refs[1].0, "b": refs[0].0});
            if flip {
                row["flip"] = json!(true);
            }
            let def = json!({"type": "constraint_set", "rows": [row]});
            match self.add_held(&def, item.name(), component, index) {
                Ok(uid) => {
                    // The faces are found at the file's points and the flip
                    // is the one that holds where the file places the
                    // occurrences.
                    let candidate = Candidate::new(def)
                        .with_note(format!("a {} touching a {}", refs[1].1, refs[0].1))
                        .with_provenance(InputProvenance::Inferred);
                    self.accepted(at, index, &[uid], &candidate, None);
                    let r = &mut self.report.relationships;
                    r.sets += 1;
                    return Ok(());
                }
                Err(why) => last = why,
            }
        }
        // Where the faces are: the angle between their z axes and how far
        // the second's origin is from the first's plane (or axis point).
        // How far apart the faces are where the file places the
        // occurrences right after the item and at the end.
        let gap = |after: bool| {
            let placed: Vec<SurfaceGeometry> = surfaces
                .iter()
                .map(|(s, path)| placed(s, &self.path_world(path, index, after)))
                .collect();
            surface_gap(&placed[0], &placed[1])
        };
        let mm_or_not =
            |g: Option<f64>| g.map_or("not parallel".to_owned(), |g| format!("{g:.6} mm"));
        Err(format!(
            "a tangent of a {} and a {}: {last} (the faces' gap {} where the file places the \
             occurrences after the item, {} at the end)",
            refs[1].1,
            refs[0].1,
            mm_or_not(gap(true)),
            mm_or_not(gap(false))
        ))
    }

    fn face_at(
        &self,
        component: ComponentUid,
        point: Vec3,
    ) -> Option<(Value, &'static str, SurfaceGeometry)> {
        let tol = LINEAR * (1.0 + point.iter().fold(0.0_f64, |m, v| m.max(v.abs())));
        for body in self.doc.component_bodies(component) {
            for entity in self.body_frame_entities(body).iter() {
                let FrameEntity::Face(name, surface) = entity else {
                    continue;
                };
                let (on, kind) = match *surface {
                    SurfaceGeometry::Plane { origin, normal } => (
                        geom::unit(normal)
                            .is_some_and(|n| geom::dot(geom::sub(point, origin), n).abs() <= tol),
                        "plane",
                    ),
                    SurfaceGeometry::Cylinder {
                        origin,
                        axis,
                        radius,
                    } => (
                        geom::unit(axis).is_some_and(|a| {
                            (geom::norm(geom::cross(geom::sub(point, origin), a)) - radius).abs()
                                <= tol
                        }),
                        "cylinder",
                    ),
                    SurfaceGeometry::Sphere { center, radius } => (
                        (geom::distance(point, center) - radius).abs() <= tol,
                        "sphere",
                    ),
                    _ => continue,
                };
                let topo = TopoName::Face(name.clone());
                if on
                    && self
                        .distance_to(body, &topo, point)
                        .is_some_and(|d| d <= tol)
                {
                    return Some((
                        json!({"body": body.to_string(), "face": name.to_string()}),
                        kind,
                        surface.clone(),
                    ));
                }
            }
        }
        None
    }
}

/// A plane, cylinder or sphere moved by `t`.
fn placed(s: &SurfaceGeometry, t: &Transform) -> SurfaceGeometry {
    match *s {
        SurfaceGeometry::Plane { origin, normal } => SurfaceGeometry::Plane {
            origin: t.apply_point(origin),
            normal: t.apply_vector(normal),
        },
        SurfaceGeometry::Cylinder {
            origin,
            axis,
            radius,
        } => SurfaceGeometry::Cylinder {
            origin: t.apply_point(origin),
            axis: t.apply_vector(axis),
            radius,
        },
        SurfaceGeometry::Sphere { center, radius } => SurfaceGeometry::Sphere {
            center: t.apply_point(center),
            radius,
        },
        ref other => other.clone(),
    }
}

/// How far two planes, cylinders or spheres are from touching (outside;
/// 0 when they touch), mm; None where planes and axes are not parallel
/// or perpendicular as touching needs.
fn surface_gap(a: &SurfaceGeometry, b: &SurfaceGeometry) -> Option<f64> {
    use SurfaceGeometry as S;
    let unit = |v: Vec3| geom::unit(v);
    // A point's distance from a line.
    let off_axis =
        |p: Vec3, o: Vec3, d: Vec3| unit(d).map(|d| geom::norm(geom::cross(geom::sub(p, o), d)));
    let parallel = |x: Vec3, y: Vec3| {
        unit(x)
            .zip(unit(y))
            .is_some_and(|(x, y)| geom::norm(geom::cross(x, y)) <= TURNED)
    };
    let gap = match (a, b) {
        (
            S::Plane { origin, normal },
            S::Plane {
                origin: o2,
                normal: n2,
            },
        ) => {
            if !parallel(*normal, *n2) {
                return None;
            }
            geom::dot(geom::sub(*o2, *origin), unit(*normal)?).abs()
        }
        (
            S::Plane { origin, normal },
            S::Cylinder {
                origin: c,
                axis,
                radius,
            },
        )
        | (
            S::Cylinder {
                origin: c,
                axis,
                radius,
            },
            S::Plane { origin, normal },
        ) => {
            let n = unit(*normal)?;
            if geom::dot(n, unit(*axis)?).abs() > TURNED {
                return None;
            }
            geom::dot(geom::sub(*c, *origin), n).abs() - radius
        }
        (S::Plane { origin, normal }, S::Sphere { center, radius })
        | (S::Sphere { center, radius }, S::Plane { origin, normal }) => {
            geom::dot(geom::sub(*center, *origin), unit(*normal)?).abs() - radius
        }
        (
            S::Cylinder {
                origin,
                axis,
                radius,
            },
            S::Cylinder {
                origin: o2,
                axis: a2,
                radius: r2,
            },
        ) => {
            if !parallel(*axis, *a2) {
                return None;
            }
            off_axis(*o2, *origin, *axis)? - radius - r2
        }
        (
            S::Cylinder {
                origin,
                axis,
                radius,
            },
            S::Sphere { center, radius: r2 },
        )
        | (
            S::Sphere { center, radius: r2 },
            S::Cylinder {
                origin,
                axis,
                radius,
            },
        ) => off_axis(*center, *origin, *axis)? - radius - r2,
        (
            S::Sphere { center, radius },
            S::Sphere {
                center: c2,
                radius: r2,
            },
        ) => geom::distance(*center, *c2) - radius - r2,
        _ => return None,
    };
    Some(gap)
}

/// A reason without its numbers, for the report's counts.
fn why_short(why: &str) -> String {
    let head = why.split(':').next().unwrap_or(why).trim();
    // Without object ids (`(object 226)`) and the side's number.
    let mut out = String::new();
    let mut rest = head;
    while let Some(i) = rest.find(" (object ") {
        out.push_str(&rest[..i]);
        rest = rest[i..].find(')').map_or("", |j| &rest[i + j + 1..]);
    }
    out.push_str(rest);
    out.replace("side one's", "a side's")
        .replace("side two's", "a side's")
        .replace("side one ", "a side ")
        .replace("side two ", "a side ")
}
