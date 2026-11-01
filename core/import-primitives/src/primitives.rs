// SPDX-License-Identifier: MIT
//! Box, cylinder, sphere, torus and coil primitives as Mitcad features
//! (mitcad#25).
//!
//! The file places a primitive by a matrix in the frame of the plane or
//! face it stands on ([`mitcad_dump::PrimitiveDetail`]): the
//! world placement is `frame * placement`, in the component's coordinates.
//! Mitcad places a primitive on a plane (an origin plane, a construction
//! plane, a planar face) by a point of the plane's own frame, so the
//! placement is expressed in the frame Mitcad gives the plane the file
//! names ([`SketchPlane`]'s frames). A box is Mitcad's where its edges run
//! along the plane's axes and its base lies in the plane, a cylinder,
//! sphere or torus where the axis is the plane's normal and the base or
//! centre lies in the plane, and a coil where it rises along the normal and
//! starts on the plane's x axis: elsewhere the item is not translated.

use mitcad_dump::{CoilDetail, Mat4, PrimitiveDetail, Reference};
use mitcad_model::datum::Datum;
use mitcad_model::features::SketchPlane;
use mitcad_model::{FeatureUid, SketchFrame};
use serde_json::{Value, json};

use mitcad_import_replay::geom::{self, mm, mm3};
use mitcad_import_replay::internal::refs;
use mitcad_import_replay::report::InputProvenance;
use mitcad_import_replay::{Candidate, ImportKernel, Importer};

/// How closely a placement must meet the plane's axes: unit vectors.
const AXIS_TOLERANCE: f64 = 1e-6;
/// How closely a base or centre must lie in the plane, mm.
const PLANE_TOLERANCE: f64 = 1e-4;

/// Row-major 4x4 matrix product `a * b`.
fn multiply(a: &Mat4, b: &Mat4) -> Mat4 {
    let mut out = [[0.0; 4]; 4];
    for (i, row) in out.iter_mut().enumerate() {
        for (j, v) in row.iter_mut().enumerate() {
            *v = (0..4).map(|k| a[i][k] * b[k][j]).sum();
        }
    }
    out
}

const IDENTITY: Mat4 = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
];

/// The negated value of a parameter name or a number.
fn negated(v: &Value) -> Value {
    match v {
        Value::String(s) => json!(format!("-({s})")),
        v => json!(-v.as_f64().unwrap_or(0.0)),
    }
}

/// A primitive's own frame in the coordinates of a plane's frame
/// (millimetres): where its origin is and where its axes point.
///
/// `pub` only because the [`Primitives`] trait names it; the crate does
/// not export it.
pub struct Placed {
    origin: [f64; 3],
    x: [f64; 3],
    y: [f64; 3],
    z: [f64; 3],
}

impl Placed {
    /// `world` (the file's, cm) in `plane`'s frame.
    fn new(world: &Mat4, plane: &SketchFrame) -> Self {
        let n = geom::normal(plane);
        let local = |v: [f64; 3]| {
            [
                geom::dot(v, plane.x_axis),
                geom::dot(v, plane.y_axis),
                geom::dot(v, n),
            ]
        };
        let col = |c: usize| [world[0][c], world[1][c], world[2][c]];
        Placed {
            origin: local(geom::sub(mm3(col(3)), plane.origin)),
            x: local(col(0)),
            y: local(col(1)),
            z: local(col(2)),
        }
    }

    /// The sign of the axis `a` along the plane's axis `k` (0 x, 1 y, 2 the
    /// normal), when it lies along it; None otherwise.
    fn along(a: [f64; 3], k: usize) -> Option<f64> {
        (a[k].abs() > 1.0 - AXIS_TOLERANCE).then(|| a[k].signum())
    }

    /// Whether the origin lies in the plane.
    fn on_plane(&self) -> bool {
        self.origin[2].abs() < PLANE_TOLERANCE
    }
}

/// The primitives family of the replay (mitcad#25, mitcad#785): boxes,
/// cylinders, spheres, tori and coils placed on a plane. The core reaches it
/// through `Families::primitive` and `Families::coil_primitive`.
pub trait Primitives<K: ImportKernel> {
    /// The frame Mitcad gives a plane reference's plane (see
    /// [`Importer::plane_options`]'s values).
    fn plane_frame_of(&self, option: &Value) -> Option<SketchFrame>;

    /// The planes a primitive may stand on with the frames Mitcad gives
    /// them: what the plane reference names; where the replay has no such
    /// face (the history state does not have it), the planar faces of the
    /// replay in the plane the file's frame (`frame`, cm) gives.
    fn standing_planes(
        &self,
        plane: &Option<Reference>,
        frame: &Mat4,
    ) -> Result<(Vec<(Value, SketchFrame)>, bool), String>;

    /// The file's value of a size parameter in mm, with its definition value.
    fn size(&self, r: &Option<Reference>, what: &str) -> Result<(f64, Value), String>;

    /// A box, cylinder, sphere or torus of its decoded fields.
    fn primitive(
        &mut self,
        index: i64,
        object_type: &str,
        d: &PrimitiveDetail,
    ) -> Result<Vec<Candidate>, String>;

    fn box_def(&self, d: &PrimitiveDetail, at: &Placed) -> Result<Value, String>;

    fn cylinder_def(&self, d: &PrimitiveDetail, at: &Placed) -> Result<Value, String>;

    fn sphere_def(&self, d: &PrimitiveDetail, at: &Placed) -> Result<Value, String>;

    fn torus_def(&self, d: &PrimitiveDetail, at: &Placed) -> Result<Value, String>;

    /// A coil placed on a plane (the `.f3d` coils: type, section, position
    /// and rotation decoded).
    fn coil_primitive(&mut self, index: i64, d: &CoilDetail) -> Result<Vec<Candidate>, String>;

    /// Whether a taper angle is absent or zero.
    fn is_zero_angle(r: &Option<Reference>) -> bool;
}

impl<K: ImportKernel> Primitives<K> for Importer<'_, K> {
    fn plane_frame_of(&self, option: &Value) -> Option<SketchFrame> {
        match option {
            Value::String(s) => match s.as_str() {
                "xy" => SketchPlane::Xy.origin_frame(),
                "xz" => SketchPlane::Xz.origin_frame(),
                "yz" => SketchPlane::Yz.origin_frame(),
                uid => match self.doc.datum(uid.parse::<FeatureUid>().ok()?)? {
                    Datum::Plane(p) => Some(p.frame()),
                    _ => None,
                },
            },
            Value::Object(m) => {
                let (body, face) = (m.get("body")?.as_str()?, m.get("face")?.as_str()?);
                let (_, _, plane) = refs::planar_faces(self.doc)
                    .into_iter()
                    .find(|(b, f, _)| b.to_string() == body && f.to_string() == face)?;
                Some(SketchPlane::face_frame(plane.origin, plane.normal))
            }
            _ => None,
        }
    }

    fn standing_planes(
        &self,
        plane: &Option<Reference>,
        frame: &Mat4,
    ) -> Result<(Vec<(Value, SketchFrame)>, bool), String> {
        let named: Vec<_> = self
            .plane_options(plane.as_ref())
            .into_iter()
            .filter_map(|o| self.plane_frame_of(&o).map(|f| (o, f)))
            .collect();
        if !named.is_empty() {
            return Ok((named, false));
        }
        let wanted = geom::frame_of(frame).ok_or("the plane's frame was not decoded")?;
        let n = geom::normal(&wanted);
        let found: Vec<_> = refs::planar_faces(self.doc)
            .into_iter()
            .filter(|(_, _, p)| {
                geom::dot(p.normal, n) > 1.0 - AXIS_TOLERANCE
                    && geom::coplanar(&wanted, p.origin, p.normal, PLANE_TOLERANCE)
            })
            .map(|(body, face, p)| {
                (
                    json!({"body": body.to_string(), "face": face.to_string()}),
                    SketchPlane::face_frame(p.origin, p.normal),
                )
            })
            .collect();
        if found.is_empty() {
            return Err("the plane or face it stands on was not found".to_owned());
        }
        Ok((found, true))
    }

    fn size(&self, r: &Option<Reference>, what: &str) -> Result<(f64, Value), String> {
        let v = r
            .as_ref()
            .and_then(Reference::parameter)
            .and_then(|p| p.value)
            .ok_or_else(|| format!("no {what}"))?;
        Ok((mm(v), self.value_or(r, what)?))
    }

    fn primitive(
        &mut self,
        index: i64,
        object_type: &str,
        d: &PrimitiveDetail,
    ) -> Result<Vec<Candidate>, String> {
        let (planes, by_geometry) = self.standing_planes(&d.plane, &d.frame.unwrap_or(IDENTITY))?;
        let world = multiply(
            &d.frame.unwrap_or(IDENTITY),
            &d.placement.unwrap_or(IDENTITY),
        );
        let operation = Self::operation(d.operation.as_deref());
        let participant_options = self.participant_options_in(
            index,
            &d.participant_bodies,
            operation,
            Some(self.component),
        );
        let mut candidates = Vec::new();
        let mut why = String::new();
        for (plane, frame) in &planes {
            let at = Placed::new(&world, frame);
            let def = match object_type {
                "BoxFeature" => self.box_def(d, &at),
                "CylinderFeature" => self.cylinder_def(d, &at),
                "SphereFeature" => self.sphere_def(d, &at),
                "TorusFeature" => self.torus_def(d, &at),
                t => Err(format!("{t} is not a primitive")),
            };
            match def {
                Ok(mut def) => {
                    def["plane"] = plane.clone();
                    def["operation"] = json!(operation);
                    let mut c = Candidate::new(def);
                    c.input_provenance = if by_geometry {
                        InputProvenance::Inferred
                    } else {
                        InputProvenance::Read
                    };
                    candidates.extend(mitcad_import_replay::internal::features::with_participants(
                        &c,
                        &participant_options,
                    ));
                }
                Err(e) => why = e,
            }
        }
        if candidates.is_empty() {
            return Err(why);
        }
        Ok(candidates)
    }

    fn box_def(&self, d: &PrimitiveDetail, at: &Placed) -> Result<Value, String> {
        for r in [&d.offset_distance_x, &d.offset_distance_y] {
            if r.as_ref()
                .and_then(Reference::parameter)
                .and_then(|p| p.value)
                .is_some_and(|v| v != 0.0)
            {
                return Err("a box with an offset is not translated".to_owned());
            }
        }
        let (dx, dxv) = self.size(&d.distance_x, "width")?;
        let (dy, dyv) = self.size(&d.distance_y, "length")?;
        let (_, dzv) = self.size(&d.distance_z, "height")?;
        if dx <= 0.0 || dy <= 0.0 {
            return Err("a box with a width or length that is not positive".to_owned());
        }
        if !at.on_plane() {
            return Err("a box whose base is not in the plane".to_owned());
        }
        let up = Placed::along(at.z, 2).ok_or("a box that does not stand on the plane")?;
        // The own x axis along the plane's x axis (width along it), or the
        // own y axis.
        let ((su, sv), (lu, wv), (length, width)) =
            if let (Some(a), Some(b)) = (Placed::along(at.x, 0), Placed::along(at.y, 1)) {
                ((a, b), (dx, dy), (dxv, dyv))
            } else if let (Some(a), Some(b)) = (Placed::along(at.y, 0), Placed::along(at.x, 1)) {
                ((a, b), (dy, dx), (dyv, dxv))
            } else {
                return Err("a box whose edges do not run along the plane's axes".to_owned());
            };
        let corner = [
            at.origin[0] + if su < 0.0 { -lu } else { 0.0 },
            at.origin[1] + if sv < 0.0 { -wv } else { 0.0 },
        ];
        // The height goes along the plane's normal, against it when negative.
        let height = if up < 0.0 { negated(&dzv) } else { dzv };
        Ok(
            json!({"type": "box", "corner": corner, "length": length, "width": width,
            "height": height}),
        )
    }

    fn cylinder_def(&self, d: &PrimitiveDetail, at: &Placed) -> Result<Value, String> {
        let (_, diameter) = self.size(&d.diameter, "diameter")?;
        let (_, height) = self.size(&d.height, "height")?;
        if !at.on_plane() {
            return Err("a cylinder whose base is not in the plane".to_owned());
        }
        let up = Placed::along(at.z, 2).ok_or("a cylinder that does not stand on the plane")?;
        let height = if up < 0.0 { negated(&height) } else { height };
        Ok(
            json!({"type": "cylinder", "center": [at.origin[0], at.origin[1]],
            "diameter": diameter, "height": height}),
        )
    }

    fn sphere_def(&self, d: &PrimitiveDetail, at: &Placed) -> Result<Value, String> {
        let (_, diameter) = self.size(&d.diameter, "diameter")?;
        if !at.on_plane() {
            return Err("a sphere whose center is not in the plane".to_owned());
        }
        Ok(
            json!({"type": "sphere", "center": [at.origin[0], at.origin[1]],
            "diameter": diameter}),
        )
    }

    fn torus_def(&self, d: &PrimitiveDetail, at: &Placed) -> Result<Value, String> {
        let (_, diameter) = self.size(&d.major_diameter, "diameter")?;
        let (_, section) = self.size(&d.minor_diameter, "section diameter")?;
        if !at.on_plane() {
            return Err("a torus whose center is not in the plane".to_owned());
        }
        Placed::along(at.z, 2).ok_or("a torus whose axis is not the plane's normal")?;
        let position = match d.torus_position.as_deref() {
            Some("InsideTorusPosition") => "inside",
            Some("OutsideTorusPosition") => "outside",
            Some("CenterTorusPosition") => "on_center",
            _ => return Err("the torus' position was not decoded".to_owned()),
        };
        let mut def = json!({"type": "torus", "center": [at.origin[0], at.origin[1]],
            "diameter": diameter, "section_diameter": section});
        if position != "on_center" {
            def["position"] = json!(position);
        }
        Ok(def)
    }

    fn coil_primitive(&mut self, index: i64, d: &CoilDetail) -> Result<Vec<Candidate>, String> {
        let (planes, by_geometry) = self.standing_planes(&d.plane, &d.frame.unwrap_or(IDENTITY))?;
        let world = multiply(
            &d.frame.unwrap_or(IDENTITY),
            &d.placement.unwrap_or(IDENTITY),
        );
        let operation = Self::operation(d.operation.as_deref());
        let participant_options = self.participant_options_in(
            index,
            &d.participant_bodies,
            operation,
            Some(self.component),
        );
        let (_, diameter) = self.size(&d.diameter, "diameter")?;
        let (_, section_size) = self.size(&d.section_size, "section size")?;
        let value = |what: &str, r: &Option<Reference>| self.value_or(r, what);
        let helix = match d.coil_type.as_deref() {
            Some("PitchAndRevolutionCoilType") => json!({"type": "revolutions_and_pitch",
                "revolutions": value("revolutions", &d.revolutions)?,
                "pitch": value("pitch", &d.pitch)?}),
            Some("RevolutionAndHeightCoilType") => json!({"type": "revolutions_and_height",
                "revolutions": value("revolutions", &d.revolutions)?,
                "height": value("height", &d.height)?}),
            Some("PitchAndHeightCoilType") => json!({"type": "height_and_pitch",
                "height": value("height", &d.height)?,
                "pitch": value("pitch", &d.pitch)?}),
            Some("SpiralCoilType") => json!({"type": "spiral",
                "revolutions": value("revolutions", &d.revolutions)?,
                "pitch": value("pitch", &d.pitch)?}),
            other => return Err(format!("coil type {other:?} was not decoded")),
        };
        let section = match d.section_type.as_deref() {
            Some("CircularCoilSectionType") => None,
            Some("SquareCoilSectionType") => Some("square"),
            Some("TriangleExternalCoilSectionType") => Some("triangular_external"),
            Some("TriangleInternalCoilSectionType") => Some("triangular_internal"),
            other => return Err(format!("coil section {other:?} was not decoded")),
        };
        let position = match d.section_position.as_deref() {
            Some("InsideCoilSectionPosition") => Some("inside"),
            Some("CenterCoilSectionPosition") => None,
            Some("OutsideCoilSectionPosition") => Some("outside"),
            other => return Err(format!("coil position {other:?} was not decoded")),
        };
        // The default rotation is right-handed: Mitcad's counter-clockwise
        // turn about the normal.
        let clockwise = match d.is_clockwise {
            Some(right_handed) => !right_handed,
            None => return Err("the coil's rotation was not decoded".to_owned()),
        };
        let mut candidates = Vec::new();
        let mut why = String::new();
        for (plane, frame) in &planes {
            let at = Placed::new(&world, frame);
            if !at.on_plane() {
                why = "a coil whose start is not in the plane".to_owned();
                continue;
            }
            if Placed::along(at.z, 2) != Some(1.0) {
                why = "a coil that does not rise along the plane's normal".to_owned();
                continue;
            }
            if Placed::along(at.x, 0) != Some(1.0) {
                why = "a coil that does not start on the plane's x axis".to_owned();
                continue;
            }
            let mut def = json!({"type": "coil", "plane": plane,
                "center": [at.origin[0], at.origin[1]], "diameter": diameter, "helix": helix,
                "section_size": section_size, "operation": operation});
            if clockwise {
                def["clockwise"] = json!(true);
            }
            if let Some(s) = section {
                def["section"] = json!(s);
            }
            if let Some(p) = position {
                def["section_position"] = json!(p);
            }
            if d.coil_type.as_deref() != Some("SpiralCoilType")
                && !Self::is_zero_angle(&d.angle)
                && let Some(a) = self.value(&d.angle)
            {
                def["angle"] = a;
            }
            let mut c = Candidate::new(def);
            c.input_provenance = if by_geometry {
                InputProvenance::Inferred
            } else {
                InputProvenance::Read
            };
            candidates.extend(mitcad_import_replay::internal::features::with_participants(
                &c,
                &participant_options,
            ));
        }
        if candidates.is_empty() {
            return Err(why);
        }
        Ok(candidates)
    }

    fn is_zero_angle(r: &Option<Reference>) -> bool {
        r.as_ref()
            .and_then(Reference::parameter)
            .and_then(|p| p.value)
            .is_none_or(|v| v == 0.0)
    }
}
