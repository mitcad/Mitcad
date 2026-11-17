// SPDX-License-Identifier: MIT
//! The JSON commands and queries of Forms (mitcad#1757; `commands.md`,
//! "Form: subdivision surfaces from a control cage"): `form.create` starts a
//! Form from a primitive cage (a box, a cylinder, a sphere, a plane, a face
//! of the model, or a cage given whole), the other `form.*` commands edit
//! its cage, each one undo step (`Document::edit_form`) or, with
//! `preview`, nothing but an answer with the edited cage and its limit
//! mesh. The `form_cage` query gives the whole cage for drawing and
//! picking, `form_limit` a triangle mesh of the limit surface of the
//! stored cage or of one given.
//!
//! Vertices, faces and edges are named by their ids in the whole cage: a
//! negative id is the mirror image of the stored element on a symmetric
//! cage; an edge is its two vertices.

use serde::Deserialize;
use serde_json::{Value, json};

use super::ApiError;
use super::dispatch::named_enum;
use mitcad_model::Document;
use mitcad_model::FeatureStatus;
use mitcad_model::features::form::cage_face;
use mitcad_model::features::{FeatureDef, FormDef, Operation, ValueInput};
use mitcad_model::ids::{BodyUid, FeatureUid};
use mitcad_model::kernel::Kernel;
use mitcad_model::subdiv::cage::polygon_normal;
use mitcad_model::subdiv::edit::{self, Made, Motion, Selection};
use mitcad_model::subdiv::mesh::Topology;
use mitcad_model::subdiv::{Cage, LimitMesh, SymmetryPlane, limit_mesh, primitive};
use mitcad_model::topo::FaceName;

/// The steps of the limit mesh a preview answers with.
const PREVIEW_LEVELS: usize = 3;

fn new_body() -> Operation {
    Operation::NewBody
}

fn ones3() -> [u32; 3] {
    [1, 1, 1]
}

fn ones2() -> [u32; 2] {
    [1, 1]
}

fn eight() -> u32 {
    8
}

fn one() -> u32 {
    1
}

fn two() -> u32 {
    2
}

fn half() -> f64 {
    0.5
}

fn preview_levels() -> usize {
    PREVIEW_LEVELS
}

/// The cage a Form starts from.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum PrimitiveInput {
    /// Length, width and height along x, y and z, each side cut into
    /// `segments`.
    Box {
        size: [f64; 3],
        #[serde(default = "ones3")]
        segments: [u32; 3],
    },
    /// About z: `segments` round, `rows` along the axis, a cap at each end.
    Cylinder {
        diameter: f64,
        height: f64,
        #[serde(default = "eight")]
        segments: u32,
        #[serde(default = "one")]
        rows: u32,
    },
    /// A box cage of `segments` a side with its vertices on the sphere.
    Sphere {
        diameter: f64,
        #[serde(default = "two")]
        segments: u32,
    },
    /// A flat sheet facing +z (an open cage: a surface body).
    Plane {
        size: [f64; 2],
        #[serde(default = "ones2")]
        segments: [u32; 2],
    },
    /// A planar face of a body: its outline as one face (a surface body),
    /// or with `thickness` a slab standing on the face.
    Face {
        body: BodyUid,
        face: FaceName,
        #[serde(default)]
        thickness: Option<f64>,
    },
    /// A cage given whole (an import, an agent's own).
    Cage { cage: Cage },
    /// The control cage kept with an imported body (an `.f3d` Form body),
    /// where it maps onto a Form's cage; it stays where it is.
    KeptCage { body: BodyUid },
}

/// An axis.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum AxisName {
    X,
    Y,
    Z,
}

impl AxisName {
    fn index(self) -> usize {
        match self {
            Self::X => 0,
            Self::Y => 1,
            Self::Z => 2,
        }
    }
}

/// A plane given by a point and a normal.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PlaneSpec {
    origin: [f64; 3],
    normal: [f64; 3],
}

/// A symmetry plane: square to an axis through a centre, or a plane.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(untagged)]
pub(super) enum PlaneInput {
    Axis(AxisName),
    Plane(PlaneSpec),
}

impl PlaneInput {
    fn plane(self, center: [f64; 3]) -> Result<SymmetryPlane, ApiError> {
        let (origin, normal) = match self {
            Self::Axis(axis) => {
                let mut normal = [0.0; 3];
                normal[axis.index()] = 1.0;
                (center, normal)
            }
            Self::Plane(plane) => (plane.origin, plane.normal),
        };
        SymmetryPlane::new(origin, normal).map_err(ApiError)
    }
}

/// `form.create`'s fields.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CreateForm {
    primitive: PrimitiveInput,
    /// Where a box, cylinder, sphere or plane is centred, and where an axis
    /// name puts the symmetry plane.
    #[serde(default)]
    center: [f64; 3],
    #[serde(default)]
    symmetry: Option<PlaneInput>,
    #[serde(default = "new_body")]
    operation: Operation,
    #[serde(default)]
    participants: Vec<BodyUid>,
    #[serde(default)]
    name: Option<String>,
}

/// A vertex put where it was dragged.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PositionInput {
    vertex: i64,
    at: [f64; 3],
}

named_enum! {
    #[derive(Debug, Deserialize)]
    #[serde(tag = "cmd", deny_unknown_fields)]
    pub(super) enum FormCommand {
        "form.create" => Create(Box<CreateForm>),
        "form.move" => Move {
            form: FeatureUid,
            #[serde(default)]
            vertices: Vec<i64>,
            #[serde(default)]
            edges: Vec<[i64; 2]>,
            #[serde(default)]
            faces: Vec<i64>,
            by: [f64; 3],
            #[serde(default)]
            preview: bool,
        },
        "form.rotate" => Rotate {
            form: FeatureUid,
            #[serde(default)]
            vertices: Vec<i64>,
            #[serde(default)]
            edges: Vec<[i64; 2]>,
            #[serde(default)]
            faces: Vec<i64>,
            /// The axis's direction; it runs through `origin`, else the
            /// selection's centre.
            axis: [f64; 3],
            #[serde(default)]
            origin: Option<[f64; 3]>,
            /// Radians, right-handed about the axis.
            angle: f64,
            #[serde(default)]
            preview: bool,
        },
        "form.scale" => Scale {
            form: FeatureUid,
            #[serde(default)]
            vertices: Vec<i64>,
            #[serde(default)]
            edges: Vec<[i64; 2]>,
            #[serde(default)]
            faces: Vec<i64>,
            /// One factor for every axis, or `factors` along x, y and z.
            #[serde(default)]
            factor: Option<f64>,
            #[serde(default)]
            factors: Option<[f64; 3]>,
            #[serde(default)]
            center: Option<[f64; 3]>,
            #[serde(default)]
            preview: bool,
        },
        "form.set_positions" => SetPositions {
            form: FeatureUid,
            positions: Vec<PositionInput>,
            #[serde(default)]
            preview: bool,
        },
        "form.extrude" => Extrude {
            form: FeatureUid,
            faces: Vec<i64>,
            distance: f64,
            #[serde(default)]
            preview: bool,
        },
        "form.insert_loop" => InsertLoop {
            form: FeatureUid,
            edge: [i64; 2],
            #[serde(default = "half")]
            at: f64,
            #[serde(default)]
            preview: bool,
        },
        "form.delete_loop" => DeleteLoop {
            form: FeatureUid,
            edge: [i64; 2],
            #[serde(default)]
            preview: bool,
        },
        "form.delete_faces" => DeleteFaces {
            form: FeatureUid,
            faces: Vec<i64>,
            #[serde(default)]
            preview: bool,
        },
        "form.fill_hole" => FillHole {
            form: FeatureUid,
            /// An edge of the opening (a boundary edge of the cage).
            edge: [i64; 2],
            /// A grid of quads when the opening has an even number of
            /// edges; else one face.
            #[serde(default)]
            quads: bool,
            #[serde(default)]
            preview: bool,
        },
        "form.bridge" => Bridge {
            form: FeatureUid,
            /// The boundary edges of both sides.
            edges: Vec<[i64; 2]>,
            /// Each edge stands for the whole opening it lies on.
            #[serde(default)]
            loops: bool,
            /// Rows of quads across the strip.
            #[serde(default = "one")]
            segments: u32,
            #[serde(default)]
            preview: bool,
        },
        "form.crease" => Crease {
            form: FeatureUid,
            #[serde(default)]
            edges: Vec<[i64; 2]>,
            /// Made corners (none with a sharpness of 0).
            #[serde(default)]
            vertices: Vec<i64>,
            /// Left out or null: sharp for good; 0: smooth again.
            #[serde(default)]
            sharpness: Option<f64>,
            #[serde(default)]
            preview: bool,
        },
        "form.symmetry" => Symmetry {
            form: FeatureUid,
            /// Null takes the plane away (the mirrored half becomes the
            /// cage's own).
            plane: Option<PlaneInput>,
            /// Where an axis name puts the plane; left out: the middle of
            /// the cage's box.
            #[serde(default)]
            center: Option<[f64; 3]>,
            #[serde(default)]
            preview: bool,
        },
        "form.set_cage" => SetCage {
            form: FeatureUid,
            cage: Cage,
            #[serde(default)]
            preview: bool,
        },
    }
}

/// Reads a `form.*` command; an unknown one's error lists the family's.
pub(super) fn parse_form_command(value: Value) -> Result<FormCommand, ApiError> {
    serde_json::from_value(value).map_err(|e| ApiError(format!("invalid Form command: {e}")))
}

/// The `form_cage` query's fields.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CageQuery {
    uid: FeatureUid,
}

/// The `form_limit` query's fields.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LimitQuery {
    #[serde(default)]
    uid: Option<FeatureUid>,
    #[serde(default)]
    cage: Option<Cage>,
    #[serde(default = "preview_levels")]
    levels: usize,
}

/// A segment count rounded up to an even one, or to a multiple of four.
fn even(count: u32, of: u32) -> u32 {
    count.div_ceil(of) * of
}

/// The primitive's cage, its segments made so that a symmetry plane
/// through its centre square to `axis` runs along its vertices.
fn primitive_cage<K: Kernel>(
    doc: &Document<K>,
    primitive: PrimitiveInput,
    center: [f64; 3],
    axis: Option<usize>,
) -> Result<Cage, ApiError> {
    let on = |k: usize| axis == Some(k);
    let cage = match primitive {
        PrimitiveInput::Box { size, segments } => {
            let segments = std::array::from_fn(|k| {
                if on(k) {
                    even(segments[k], 2)
                } else {
                    segments[k]
                }
            });
            primitive::box_cage(center, size, segments)
        }
        PrimitiveInput::Cylinder {
            diameter,
            height,
            segments,
            rows,
        } => {
            let segments = if on(0) || on(1) {
                even(segments, 4)
            } else {
                segments
            };
            let rows = if on(2) { even(rows, 2) } else { rows };
            primitive::cylinder_cage(center, diameter, height, segments, rows)
        }
        PrimitiveInput::Sphere { diameter, segments } => {
            let segments = if axis.is_some() {
                even(segments, 2)
            } else {
                segments
            };
            primitive::sphere_cage(center, diameter, segments)
        }
        PrimitiveInput::Plane { size, segments } => {
            let segments = std::array::from_fn(|k| {
                if on(k) {
                    even(segments[k], 2)
                } else {
                    segments[k]
                }
            });
            primitive::plane_cage(center, size, segments)
        }
        PrimitiveInput::Face {
            body,
            face,
            thickness,
        } => {
            let (outline, normal) = doc.face_outline_points(body, &face)?;
            primitive::polygon_cage(&outline, normal, thickness)
        }
        PrimitiveInput::Cage { cage } => cage.check().map(|()| cage),
        PrimitiveInput::KeptCage { body } => Ok(doc.kept_form_cage(body)?),
    };
    cage.map_err(|e| ApiError(format!("the Form's cage: {e}")))
}

fn create<K: Kernel>(doc: &mut Document<K>, create: CreateForm) -> Result<Value, ApiError> {
    let axis = match create.symmetry {
        Some(PlaneInput::Axis(axis)) => Some(axis.index()),
        _ => None,
    };
    let mut cage = primitive_cage(doc, create.primitive, create.center, axis)?;
    if let Some(plane) = create.symmetry {
        let plane = plane.plane(create.center)?;
        edit::set_symmetry(&mut cage, Some(plane))
            .map_err(|e| ApiError(format!("the Form's cage: {e}")))?;
    }
    let closed = cage.full().closed();
    let (vertices, faces) = (cage.vertices.len(), cage.faces.len());
    let def: FormDef<ValueInput> = FormDef::new(cage, create.operation, create.participants);
    let added = doc.add_feature(&FeatureDef::Form(def), create.name.as_deref())?;
    let mut value = json!({
        "uid": added.uid,
        "name": added.name,
        "vertices": vertices,
        "faces": faces,
        "closed": closed,
    });
    if let Some(FeatureStatus::Failed(error)) = doc.status(added.uid) {
        value["error"] = json!(error);
    }
    Ok(value)
}

fn made_json(made: &Made) -> Value {
    json!({
        "vertices": made.vertices,
        "faces": made.faces,
        "edges": made.edges,
        "removed_vertices": made.removed_vertices,
        "removed_faces": made.removed_faces,
    })
}

/// What an edit that opens or closes the cage made, with whether the
/// whole cage is closed now (mitcad#1941).
fn closed_json(made: &Made, cage: &Cage) -> Value {
    let mut value = made_json(made);
    value["closed"] = json!(cage.full().closed());
    value
}

/// Runs a cage edit as one undo step `<what> in <Form>`, or with `preview`
/// on a copy: then the answer has `preview`, the edited `cage` (as the
/// `form_cage` query gives it) and its `limit` mesh, and nothing changes.
fn edit_cage<K: Kernel>(
    doc: &mut Document<K>,
    form: FeatureUid,
    what: &str,
    preview: bool,
    change: impl FnOnce(&mut Cage) -> Result<Value, String>,
) -> Result<Value, ApiError> {
    if !preview {
        return Ok(doc.edit_form(form, what, change)?);
    }
    let name = doc
        .feature(form)
        .map(|entry| entry.name.clone())
        .unwrap_or_default();
    let mut cage = doc.form_def(form)?.cage.clone();
    let mut value = change(&mut cage).map_err(|e| ApiError(format!("{name}: {e}")))?;
    value["preview"] = json!(true);
    value["cage"] = cage_json(&cage, Some(form));
    value["limit"] = limit_json(&cage, PREVIEW_LEVELS)?;
    Ok(value)
}

/// Runs a `form.*` command.
pub(super) fn run_form_command<K: Kernel>(
    doc: &mut Document<K>,
    command: FormCommand,
) -> Result<Value, ApiError> {
    match command {
        FormCommand::Create(fields) => create(doc, *fields),
        FormCommand::Move {
            form,
            vertices,
            edges,
            faces,
            by,
            preview,
        } => {
            let selection = Selection {
                vertices,
                edges,
                faces,
            };
            edit_cage(doc, form, "Move", preview, move |cage| {
                edit::transform(cage, &selection, Motion::Translate(by))
                    .map(|moved| json!({"moved": moved}))
            })
        }
        FormCommand::Rotate {
            form,
            vertices,
            edges,
            faces,
            axis,
            origin,
            angle,
            preview,
        } => {
            let selection = Selection {
                vertices,
                edges,
                faces,
            };
            let motion = Motion::Rotate {
                origin,
                axis,
                angle,
            };
            edit_cage(doc, form, "Rotate", preview, move |cage| {
                edit::transform(cage, &selection, motion).map(|moved| json!({"moved": moved}))
            })
        }
        FormCommand::Scale {
            form,
            vertices,
            edges,
            faces,
            factor,
            factors,
            center,
            preview,
        } => {
            let factors = match (factor, factors) {
                (Some(f), None) => [f; 3],
                (None, Some(factors)) => factors,
                _ => {
                    return Err(ApiError(
                        "a scale takes either factor or factors".to_owned(),
                    ));
                }
            };
            let selection = Selection {
                vertices,
                edges,
                faces,
            };
            let motion = Motion::Scale { center, factors };
            edit_cage(doc, form, "Scale", preview, move |cage| {
                edit::transform(cage, &selection, motion).map(|moved| json!({"moved": moved}))
            })
        }
        FormCommand::SetPositions {
            form,
            positions,
            preview,
        } => {
            let positions: Vec<(i64, [f64; 3])> =
                positions.iter().map(|p| (p.vertex, p.at)).collect();
            edit_cage(doc, form, "Drag", preview, move |cage| {
                edit::set_positions(cage, &positions).map(|()| json!({}))
            })
        }
        FormCommand::Extrude {
            form,
            faces,
            distance,
            preview,
        } => edit_cage(doc, form, "Extrude", preview, move |cage| {
            edit::extrude(cage, &faces, distance).map(|made| made_json(&made))
        }),
        FormCommand::InsertLoop {
            form,
            edge,
            at,
            preview,
        } => edit_cage(doc, form, "Insert Edge Loop", preview, move |cage| {
            edit::insert_loop(cage, edge, at).map(|made| made_json(&made))
        }),
        FormCommand::DeleteLoop {
            form,
            edge,
            preview,
        } => edit_cage(doc, form, "Delete Edge Loop", preview, move |cage| {
            edit::delete_loop(cage, edge).map(|made| made_json(&made))
        }),
        FormCommand::DeleteFaces {
            form,
            faces,
            preview,
        } => edit_cage(doc, form, "Delete Faces", preview, move |cage| {
            edit::delete_faces(cage, &faces).map(|made| made_json(&made))
        }),
        FormCommand::FillHole {
            form,
            edge,
            quads,
            preview,
        } => edit_cage(doc, form, "Fill Hole", preview, move |cage| {
            let made = edit::fill_hole(cage, edge, quads)?;
            Ok(closed_json(&made, cage))
        }),
        FormCommand::Bridge {
            form,
            edges,
            loops,
            segments,
            preview,
        } => edit_cage(doc, form, "Bridge", preview, move |cage| {
            let made = edit::bridge(cage, &edges, loops, segments)?;
            Ok(closed_json(&made, cage))
        }),
        FormCommand::Crease {
            form,
            edges,
            vertices,
            sharpness,
            preview,
        } => edit_cage(doc, form, "Crease", preview, move |cage| {
            edit::crease(cage, &edges, &vertices, sharpness).map(|()| json!({}))
        }),
        FormCommand::Symmetry {
            form,
            plane,
            center,
            preview,
        } => {
            let middle = |cage: &Cage| {
                let points: Vec<[f64; 3]> = cage.full().mesh.points;
                let mut low = [f64::INFINITY; 3];
                let mut high = [f64::NEG_INFINITY; 3];
                for p in &points {
                    low = std::array::from_fn(|k| low[k].min(p[k]));
                    high = std::array::from_fn(|k| high[k].max(p[k]));
                }
                std::array::from_fn(|k| (low[k] + high[k]) / 2.0)
            };
            let center = match center {
                Some(center) => center,
                None => middle(&doc.form_def(form)?.cage),
            };
            let plane = plane.map(|p| p.plane(center)).transpose()?;
            edit_cage(doc, form, "Symmetry", preview, move |cage| {
                edit::set_symmetry(cage, plane).map(|()| json!({}))
            })
        }
        FormCommand::SetCage {
            form,
            cage: new,
            preview,
        } => edit_cage(doc, form, "Edit Cage", preview, move |cage| {
            new.check()?;
            let mut new = new;
            // No id the cage gave comes back.
            new.keep_ids_of(cage);
            *cage = new;
            Ok(json!({}))
        }),
    }
}

/// The whole cage for drawing and picking (the `form_cage` query): each
/// vertex with its id, position, whether it is a mirror image and a
/// corner; each edge with its two vertices, whether it is on the cage's
/// boundary and its crease (`creased`, and a `sharpness` unless it is
/// sharp for good); each face with its id, vertices, centre, normal and,
/// for a Form `uid`, the `name` that finds its faces on the body.
pub(super) fn cage_json(cage: &Cage, uid: Option<FeatureUid>) -> Value {
    let full = cage.full();
    let topology = Topology::of(&full.mesh);
    let vertices: Vec<Value> = full
        .mesh
        .points
        .iter()
        .enumerate()
        .map(|(i, at)| {
            json!({
                "id": full.vertex_ids[i],
                "at": at,
                "mirror": full.vertex_ids[i] < 0,
                "corner": full.mesh.corners[i],
            })
        })
        .collect();
    let edges: Vec<Value> = topology
        .edges
        .iter()
        .enumerate()
        .map(|(e, (a, b))| {
            let mut edge = json!({
                "vertices": [full.vertex_ids[*a], full.vertex_ids[*b]],
                "boundary": topology.edge_faces[e].len() == 1,
                "creased": false,
            });
            if let Some(s) = full.mesh.creases.get(&(*a, *b)) {
                edge["creased"] = json!(true);
                if s.is_finite() {
                    edge["sharpness"] = json!(s);
                }
            }
            edge
        })
        .collect();
    let faces: Vec<Value> = full
        .mesh
        .faces
        .iter()
        .enumerate()
        .map(|(f, lp)| {
            let points: Vec<[f64; 3]> = lp.iter().map(|v| full.mesh.points[*v]).collect();
            let (normal, center) = polygon_normal(&points);
            let mut face = json!({
                "id": full.face_ids[f],
                "vertices": lp.iter().map(|v| full.vertex_ids[*v]).collect::<Vec<_>>(),
                "center": center,
                "normal": normal,
                "mirror": full.face_ids[f] < 0,
            });
            if let Some(uid) = uid {
                face["name"] = json!(cage_face(uid, full.face_ids[f]).to_string());
            }
            face
        })
        .collect();
    json!({
        "vertices": vertices,
        "edges": edges,
        "faces": faces,
        "closed": topology.is_closed(),
        "symmetry": cage.symmetry,
    })
}

/// The `form_cage` query.
pub(super) fn form_cage<K: Kernel>(
    doc: &Document<K>,
    query: &CageQuery,
) -> Result<Value, ApiError> {
    let def = doc.form_def(query.uid)?;
    let name = doc
        .feature(query.uid)
        .map(|entry| entry.name.clone())
        .unwrap_or_default();
    let mut value = cage_json(&def.cage, Some(query.uid));
    value["uid"] = json!(query.uid);
    value["name"] = json!(name);
    value["stored"] = json!(def.cage);
    Ok(value)
}

fn mesh_json(mesh: &LimitMesh, closed: bool) -> Value {
    json!({
        "positions": mesh.positions.iter().flatten().collect::<Vec<_>>(),
        "normals": mesh.normals.iter().flatten().collect::<Vec<_>>(),
        "triangles": mesh.triangles.iter().flatten().collect::<Vec<_>>(),
        "faces": mesh.faces,
        "closed": closed,
    })
}

fn limit_json(cage: &Cage, levels: usize) -> Result<Value, ApiError> {
    let mesh = limit_mesh(cage, levels).map_err(|e| ApiError(format!("the Form's cage: {e}")))?;
    Ok(mesh_json(&mesh, cage.full().closed()))
}

/// The `form_limit` query: the limit mesh of a Form's cage or of a cage
/// given.
pub(super) fn form_limit<K: Kernel>(
    doc: &Document<K>,
    query: LimitQuery,
) -> Result<Value, ApiError> {
    let cage = match (query.cage, query.uid) {
        (Some(cage), None) => cage,
        (None, Some(uid)) => doc.form_def(uid)?.cage.clone(),
        _ => {
            return Err(ApiError(
                "form_limit takes either a Form's uid or a cage".to_owned(),
            ));
        }
    };
    limit_json(&cage, query.levels)
}
