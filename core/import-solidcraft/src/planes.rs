// SPDX-License-Identifier: MIT
//! The file's planes (`PlaneRef`) as MitCad plane references, and the
//! frame a sketch on one has.
//!
//! - The origin planes are MitCad's (XY and XZ have the same axes in both;
//!   YZ's sketch axes differ, which the sketch's frame takes up).
//! - An offset plane, a plane turned about a line in it and a construction
//!   plane are construction planes, so they follow their parameters; their
//!   bases first. A face is the face found at the file's pick point; the
//!   plane the file stored for it gives the sketch its frame on it.
//! - Midplanes, planes through two edges or three points, tangent planes
//!   and planes along a path are the construction planes of the same
//!   definitions; a plane through a line square to another plane, a plane
//!   turned about a line outside it and a plane placed by expressions are
//!   fixed where they are now (the report says so).
//!
//! A sketch's frame: SolidCraft's frame of the plane (origin and axes,
//! component coordinates) expressed in the frame MitCad gives the plane.
//! Where SolidCraft's frame is not stored or derived exactly (a midplane's
//! or a tangent plane's), MitCad's frame is taken, with a note.

use mitcad_model::{ComponentUid, Datum, GeomRef, Kernel};
use mitcad_solidcraft::Feature;
use mitcad_solidcraft::inputs::Kind;
use mitcad_solidcraft::kinds::{GeoRef, Plane, PlaneRef};
use serde_json::{Value, json};

use super::Importer;
use mitcad_import_base::geom::{add, cross, dot, norm, scale, sub, unit};

pub(crate) use mitcad_vec3::Vec3;

/// A frame: origin and unit axes (normal x × y), component coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Frame {
    pub o: Vec3,
    pub x: Vec3,
    pub y: Vec3,
}

impl Frame {
    pub const XY: Frame = Frame {
        o: [0.0; 3],
        x: [1.0, 0.0, 0.0],
        y: [0.0, 1.0, 0.0],
    };

    pub fn normal(&self) -> Vec3 {
        cross(self.x, self.y)
    }

    pub fn of(p: &Plane) -> Frame {
        let x = unit(p.x_dir).unwrap_or([1.0, 0.0, 0.0]);
        let n = unit(cross(x, p.y_dir)).unwrap_or([0.0, 0.0, 1.0]);
        Frame {
            o: p.origin,
            x,
            y: cross(n, x),
        }
    }

    pub fn point(&self, p: [f64; 2]) -> Vec3 {
        add(self.o, add(scale(self.x, p[0]), scale(self.y, p[1])))
    }

    pub fn direction(&self, d: [f64; 2]) -> Vec3 {
        add(scale(self.x, d[0]), scale(self.y, d[1]))
    }

    /// `other` in this frame's coordinates (x, y, normal), as a sketch's
    /// `frame`; None when they are the same.
    pub fn relative(&self, other: &Frame) -> Option<(Vec3, Vec3, Vec3)> {
        let n = self.normal();
        let local = |v: Vec3| [dot(v, self.x), dot(v, self.y), dot(v, n)];
        let (o, x, y) = (local(sub(other.o, self.o)), local(other.x), local(other.y));
        let same = norm(o) < 1e-9
            && norm(sub(x, [1.0, 0.0, 0.0])) < 1e-12
            && norm(sub(y, [0.0, 1.0, 0.0])) < 1e-12;
        (!same).then_some((o, x, y))
    }

    /// Turned by `angle` (right-handed) about the line through `origin`
    /// along `axis`.
    fn rotated(&self, origin: Vec3, axis: Vec3, angle: f64) -> Frame {
        let k = unit(axis).unwrap_or([0.0, 0.0, 1.0]);
        let rot = |v: Vec3| {
            let (c, s) = (angle.cos(), angle.sin());
            add(
                add(scale(v, c), scale(cross(k, v), s)),
                scale(k, dot(k, v) * (1.0 - c)),
            )
        };
        Frame {
            o: add(origin, rot(sub(self.o, origin))),
            x: rot(self.x),
            y: rot(self.y),
        }
    }
}

/// SolidCraft's origin planes (`crates/geom/src/plane.rs`).
fn origin_frame(name: &str) -> Option<(&'static str, Frame)> {
    match name.to_ascii_uppercase().as_str() {
        "XY" | "TOP" => Some(("xy", Frame::XY)),
        "XZ" | "FRONT" => Some((
            "xz",
            Frame {
                o: [0.0; 3],
                x: [1.0, 0.0, 0.0],
                y: [0.0, 0.0, -1.0],
            },
        )),
        "YZ" | "RIGHT" => Some((
            "yz",
            Frame {
                o: [0.0; 3],
                x: [0.0, 1.0, 0.0],
                y: [0.0, 0.0, 1.0],
            },
        )),
        _ => None,
    }
}

/// A plane made: its MitCad reference and SolidCraft's frame of it.
pub(crate) struct PlaneMade {
    pub geom: Value,
    pub world: Option<Frame>,
    pub notes: Vec<String>,
}

/// A sketch's plane: the reference, the sketch's frame in it.
pub(crate) struct SketchPlane {
    pub geom: Value,
    pub frame: Option<(Vec3, Vec3, Vec3)>,
    pub world: Option<Frame>,
    pub notes: Vec<String>,
}

impl<K: Kernel> Importer<'_, '_, K> {
    pub(super) fn plane_for_sketch<'n>(
        &mut self,
        p: &PlaneRef,
        names: &mut impl Iterator<Item = &'n str>,
        component: ComponentUid,
    ) -> Result<SketchPlane, String> {
        let mut made = self.plane_ref(p, names, component, None)?;
        let mitcad = self.mitcad_frame(&made.geom)?;
        let world = match made.world {
            Some(w) => w,
            None => {
                made.notes
                    .push("the sketch's frame on its plane is MitCad's".to_owned());
                mitcad
            }
        };
        Ok(SketchPlane {
            frame: mitcad.relative(&world),
            geom: made.geom,
            world: Some(world),
            notes: made.notes,
        })
    }

    /// MitCad's frame of a plane reference now.
    pub(super) fn mitcad_frame(&self, geom: &Value) -> Result<Frame, String> {
        let g: GeomRef =
            serde_json::from_value(geom.clone()).map_err(|e| format!("plane {geom}: {e}"))?;
        let p = self.doc.resolve_plane(&g)?;
        Ok(Frame {
            o: p.origin,
            x: p.x_axis,
            y: p.y_axis,
        })
    }

    /// A plane as a MitCad reference; `final_name` names the construction
    /// plane it becomes, when it is the feature of the file itself (else
    /// helpers are hidden). `names` are the parameter names of the plane's
    /// offsets and angles, outermost first.
    pub(super) fn plane_ref<'n>(
        &mut self,
        p: &PlaneRef,
        names: &mut impl Iterator<Item = &'n str>,
        component: ComponentUid,
        final_name: Option<&str>,
    ) -> Result<PlaneMade, String> {
        let mut notes = Vec::new();
        let made = match p {
            PlaneRef::Origin { name } => {
                let (g, frame) =
                    origin_frame(name).ok_or_else(|| format!("no origin plane {name}"))?;
                if let Some(n) = final_name {
                    let def = json!({"type": "construction_plane",
                        "definition": {"type": "offset", "plane": g, "distance": 0}});
                    let uid = self.add(&def, Some(n), component)?;
                    return Ok(PlaneMade {
                        geom: json!(uid.to_string()),
                        world: Some(frame),
                        notes,
                    });
                }
                PlaneMade {
                    geom: json!(g),
                    world: Some(frame),
                    notes,
                }
            }
            PlaneRef::Custom { plane } => {
                let frame = Frame::of(plane);
                self.fixed_plane(&frame, component, final_name)?
            }
            PlaneRef::Placed {
                origin,
                x_dir,
                y_dir,
            } => {
                let o = [
                    self.params
                        .value(self.doc, origin[0].as_str(), Kind::Length)?,
                    self.params
                        .value(self.doc, origin[1].as_str(), Kind::Length)?,
                    self.params
                        .value(self.doc, origin[2].as_str(), Kind::Length)?,
                ];
                let frame = Frame::of(&Plane {
                    origin: o,
                    x_dir: *x_dir,
                    y_dir: *y_dir,
                });
                let mut m = self.fixed_plane(&frame, component, final_name)?;
                m.notes.push(
                    "a plane placed by expressions is fixed where they put it now".to_owned(),
                );
                m
            }
            PlaneRef::Offset { base, distance } => {
                let name = names.next();
                let base = self.plane_ref(base, names, component, None)?;
                notes.extend(base.notes);
                let slot = self.params.slot(self.doc, name, distance.as_str())?;
                let def = json!({"type": "construction_plane",
                    "definition": {"type": "offset", "plane": base.geom, "distance": slot}});
                let uid = self.add_plane(&def, component, final_name)?;
                let d = self
                    .params
                    .value(self.doc, distance.as_str(), Kind::Length)?;
                PlaneMade {
                    geom: json!(uid),
                    world: base.world.map(|w| Frame {
                        o: add(w.o, scale(w.normal(), d)),
                        ..w
                    }),
                    notes,
                }
            }
            PlaneRef::AtAngle {
                base,
                axis_origin,
                axis_dir,
                angle,
            } => {
                let name = names.next();
                let base = self.plane_ref(base, names, component, None)?;
                notes.extend(base.notes);
                let a = self.params.value(self.doc, angle.as_str(), Kind::Angle)?;
                let world = base.world.map(|w| w.rotated(*axis_origin, *axis_dir, a));
                // MitCad turns a plane about a line in it.
                let in_plane = base.world.is_some_and(|w| {
                    let n = w.normal();
                    dot(sub(*axis_origin, w.o), n).abs() < 1e-6
                        && unit(*axis_dir).is_some_and(|d| dot(d, n).abs() < 1e-9)
                });
                if in_plane {
                    let slot = self.params.slot(self.doc, name, angle.as_str())?;
                    let def = json!({"type": "construction_plane", "definition": {"type": "angle",
                        "line": {"origin": axis_origin, "direction": axis_dir},
                        "angle": slot, "plane": base.geom}});
                    let uid = self.add_plane(&def, component, final_name)?;
                    PlaneMade {
                        geom: json!(uid),
                        world,
                        notes,
                    }
                } else {
                    let w = world.ok_or(
                        "a plane turned about a line outside it, on a plane without a frame",
                    )?;
                    let mut m = self.fixed_plane(&w, component, final_name)?;
                    m.notes.extend(notes);
                    m.notes.push(
                        "a plane turned about a line outside it is fixed where it is now"
                            .to_owned(),
                    );
                    m
                }
            }
            PlaneRef::Construction { name } => {
                let f = self.file_feature_named(name)?;
                let uid = self
                    .made
                    .get(&f.id)
                    .and_then(|m| m.last())
                    .ok_or_else(|| format!("construction plane {name} did not come in"))?;
                let world = self.planes.get(&f.id).copied();
                if let Some(n) = final_name {
                    let def = json!({"type": "construction_plane",
                        "definition": {"type": "offset", "plane": uid.to_string(), "distance": 0}});
                    let made = self.add(&def, Some(n), component)?;
                    return Ok(PlaneMade {
                        geom: json!(made.to_string()),
                        world,
                        notes,
                    });
                }
                PlaneMade {
                    geom: json!(uid.to_string()),
                    world,
                    notes,
                }
            }
            PlaneRef::Flipped { base, about_y } => {
                let base = self.plane_ref(base, names, component, final_name)?;
                let world = base.world.map(|w| {
                    if *about_y {
                        Frame {
                            x: scale(w.x, -1.0),
                            ..w
                        }
                    } else {
                        Frame {
                            y: scale(w.y, -1.0),
                            ..w
                        }
                    }
                });
                PlaneMade {
                    geom: base.geom,
                    world,
                    notes: base.notes,
                }
            }
            PlaneRef::Face {
                plane, at, name, ..
            } => {
                let (body, face) = self.face_named(component, None, *at, name.as_deref())?;
                let geom = json!({"body": body.to_string(), "face": face});
                if let Some(n) = final_name {
                    let def = json!({"type": "construction_plane",
                        "definition": {"type": "offset", "plane": geom, "distance": 0}});
                    let uid = self.add(&def, Some(n), component)?;
                    return Ok(PlaneMade {
                        geom: json!(uid.to_string()),
                        world: Some(Frame::of(plane)),
                        notes,
                    });
                }
                PlaneMade {
                    geom,
                    world: Some(Frame::of(plane)),
                    notes,
                }
            }
            PlaneRef::Midplane { a, b } => {
                let a = self.geo(a, component, &mut notes)?;
                let b = self.geo(b, component, &mut notes)?;
                self.constructed(
                    json!({"type": "midplane", "plane1": a, "plane2": b}),
                    component,
                    final_name,
                    notes,
                )?
            }
            PlaneRef::TwoEdges { a, b } => {
                let a = self.geo(a, component, &mut notes)?;
                let b = self.geo(b, component, &mut notes)?;
                self.constructed(
                    json!({"type": "two_edges", "line1": a, "line2": b}),
                    component,
                    final_name,
                    notes,
                )?
            }
            PlaneRef::ThreePoints { a, b, c } => {
                let a = self.geo(a, component, &mut notes)?;
                let b = self.geo(b, component, &mut notes)?;
                let c = self.geo(c, component, &mut notes)?;
                self.constructed(
                    json!({"type": "three_points", "point1": a, "point2": b, "point3": c}),
                    component,
                    final_name,
                    notes,
                )?
            }
            PlaneRef::Tangent { face, at } => {
                let face = self.geo(face, component, &mut notes)?;
                self.constructed(
                    json!({"type": "tangent_at_point", "face": face, "point": {"point": at}}),
                    component,
                    final_name,
                    notes,
                )?
            }
            PlaneRef::AlongPath { path, t } => {
                let name = names.next();
                let path = self.path_of(path, component, &mut notes)?;
                let slot = self.params.slot(self.doc, name, t.as_str())?;
                self.constructed(
                    json!({"type": "along_path", "path": path, "distance": slot, "physical": false}),
                    component,
                    final_name,
                    notes,
                )?
            }
            PlaneRef::Perpendicular { plane, line } => {
                let pg = self.geo(plane, component, &mut notes)?;
                let lg = self.geo(line, component, &mut notes)?;
                let pg: GeomRef = serde_json::from_value(pg).map_err(|e| e.to_string())?;
                let lg: GeomRef = serde_json::from_value(lg).map_err(|e| e.to_string())?;
                let p = self.doc.resolve_plane(&pg)?;
                let l = self.doc.resolve_axis(&lg)?;
                let n = cross(p.x_axis, p.y_axis);
                let normal =
                    unit(cross(l.direction, n)).ok_or("the line is square to the plane")?;
                let x = unit(l.direction).ok_or("the line has no direction")?;
                let frame = Frame {
                    o: l.origin,
                    x,
                    y: cross(normal, x),
                };
                let mut m = self.fixed_plane(&frame, component, final_name)?;
                m.notes.extend(notes);
                m.notes.push(
                    "a plane through a line square to a plane is fixed where it is now".to_owned(),
                );
                m
            }
        };
        Ok(made)
    }

    /// A construction plane of a MitCad definition, with MitCad's frame.
    fn constructed(
        &mut self,
        definition: Value,
        component: ComponentUid,
        final_name: Option<&str>,
        notes: Vec<String>,
    ) -> Result<PlaneMade, String> {
        let def = json!({"type": "construction_plane", "definition": definition});
        let uid = self.add_plane(&def, component, final_name)?;
        let world = self
            .doc
            .datum(uid.parse().map_err(|_| "a feature id")?)
            .and_then(|d| match d {
                Datum::Plane(p) => Some(Frame {
                    o: p.origin,
                    x: p.x_axis,
                    y: p.y_axis,
                }),
                _ => None,
            });
        let mut notes = notes;
        if final_name.is_none() {
            notes.push("the frame of the plane is MitCad's".to_owned());
        }
        Ok(PlaneMade {
            geom: json!(uid),
            world: if final_name.is_some() { world } else { None },
            notes,
        })
    }

    /// A fixed plane (`fixed` construction plane).
    fn fixed_plane(
        &mut self,
        frame: &Frame,
        component: ComponentUid,
        final_name: Option<&str>,
    ) -> Result<PlaneMade, String> {
        let def = json!({"type": "construction_plane",
            "definition": {"type": "fixed", "origin": frame.o, "x_axis": frame.x, "y_axis": frame.y}});
        let uid = self.add_plane(&def, component, final_name)?;
        Ok(PlaneMade {
            geom: json!(uid),
            world: Some(*frame),
            notes: Vec::new(),
        })
    }

    /// Adds a construction plane: the feature of the file (named) or a
    /// hidden helper. Its id as a plane reference.
    fn add_plane(
        &mut self,
        def: &Value,
        component: ComponentUid,
        final_name: Option<&str>,
    ) -> Result<String, String> {
        let uid = match final_name {
            Some(n) => self.add(def, Some(n), component)?,
            None => self.add_helper(def, component)?,
        };
        Ok(uid.to_string())
    }

    /// A plane for a feature (a mirror's, a split's): a fixed plane where
    /// the file's plane is fixed, else as for a sketch.
    pub(super) fn feature_plane(
        &mut self,
        f: &Feature,
        p: &PlaneRef,
        pointer: &str,
        component: ComponentUid,
        notes: &mut Vec<String>,
    ) -> Result<Value, String> {
        if let PlaneRef::Custom { plane } = p {
            let w = Frame::of(plane);
            return Ok(json!({"origin": w.o, "normal": w.normal(), "x_axis": w.x}));
        }
        let names = self.input_names(f, pointer);
        let mut it = names.iter().map(String::as_str);
        let made = self.plane_ref(p, &mut it, component, None)?;
        notes.extend(made.notes);
        Ok(made.geom)
    }

    /// The parameter names of the inputs under a JSON pointer of a feature
    /// (its plane's offsets and angles), in walk order.
    pub(super) fn input_names(&self, f: &Feature, pointer: &str) -> Vec<String> {
        mitcad_solidcraft::inputs::inputs(&f.def)
            .into_iter()
            .zip(f.param_names.iter())
            .filter(|(i, _)| i.pointer.starts_with(pointer))
            .map(|(_, n)| n.clone())
            .collect()
    }

    /// A feature of the file by name.
    pub(super) fn file_feature_named(&self, name: &str) -> Result<Feature, String> {
        self.design
            .features
            .iter()
            .find(|f| f.name == name)
            .cloned()
            .ok_or_else(|| format!("no feature {name}"))
    }

    /// A construction reference as a MitCad reference.
    pub(super) fn geo(
        &mut self,
        g: &GeoRef,
        component: ComponentUid,
        notes: &mut Vec<String>,
    ) -> Result<Value, String> {
        Ok(match g {
            GeoRef::Point { p } => json!({"point": p}),
            GeoRef::Origin => json!("origin"),
            GeoRef::Vertex { at, .. } => match self.vertex_at(component, *at) {
                Ok((body, vertex)) => json!({"body": body.to_string(), "vertex": vertex}),
                Err(_) => {
                    notes.push(format!("the vertex at {at:?} is a fixed point"));
                    json!({"point": at})
                }
            },
            GeoRef::Edge { at, name } => {
                let (body, edge) = self.edge_named(component, None, *at, name.as_deref())?;
                json!({"body": body.to_string(), "edge": edge})
            }
            GeoRef::Face { at, name } => {
                let (body, face) = self.face_named(component, None, *at, name.as_deref())?;
                json!({"body": body.to_string(), "face": face})
            }
            GeoRef::Plane { plane } => {
                let made = self.plane_ref(plane, &mut std::iter::empty(), component, None)?;
                notes.extend(made.notes);
                made.geom
            }
            GeoRef::World { axis } => json!(axis.to_ascii_lowercase()),
            GeoRef::Axis { name } | GeoRef::ConstructPoint { name } => {
                let f = self.file_feature_named(name)?;
                let uid = self
                    .made
                    .get(&f.id)
                    .and_then(|m| m.last())
                    .ok_or_else(|| format!("{name} did not come in"))?;
                json!(uid.to_string())
            }
            GeoRef::SketchPoint { sketch, point } => {
                let s = self
                    .sketches
                    .get(sketch)
                    .ok_or("the sketch did not come in")?;
                json!({"sketch": s.uid.to_string(), "point": s.point(point)?})
            }
            GeoRef::SketchCurve { sketch, curve } => {
                let s = self
                    .sketches
                    .get(sketch)
                    .ok_or("the sketch did not come in")?;
                json!({"sketch": s.uid.to_string(), "curve": s.curve(curve)?})
            }
        })
    }

    /// A path (an edge or a sketch curve) as a MitCad path reference.
    pub(super) fn path_of(
        &mut self,
        g: &GeoRef,
        component: ComponentUid,
        _notes: &mut Vec<String>,
    ) -> Result<Value, String> {
        Ok(match g {
            GeoRef::Edge { at, .. } => {
                let (body, edge) = self.edge_at(component, None, *at)?;
                json!({"body": body.to_string(), "edges": [edge]})
            }
            GeoRef::SketchCurve { sketch, curve } => {
                let s = self
                    .sketches
                    .get(sketch)
                    .ok_or("the sketch did not come in")?;
                json!({"sketch": s.uid.to_string(), "curve": s.curve(curve)?})
            }
            other => return Err(format!("{other:?} is not a path")),
        })
    }
}
