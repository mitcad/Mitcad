// SPDX-License-Identifier: MIT
//! Decals (mitcad#2244): an image put on a face of a body, such as a label,
//! a logo or a warning sign, listed in the browser's Decals folder and
//! saved in the design file with the image's bytes embedded as an image
//! reference's are (`image_references.rs`, the same storage and header
//! reading; no path to the file is kept). A decal changes no geometry:
//! inserting, editing, showing, hiding, renaming and deleting one is an
//! undo step that recomputes nothing.
//!
//! A decal lies on a planar, cylindrical or conical face, which it follows
//! by the face's topological name: the face is found again at the timeline
//! marker whenever the decal is asked for, and when it is gone the decal
//! says why instead of where it lies. The face's occurrence path, from the
//! design root, follows the edits that move occurrences or bodies
//! (`DocState::follow`, mitcad#2527): Reparent Occurrence, Move into
//! Component, a link that comes back with its root (mitcad#2441) and Save
//! Component as Design with Replace. Its place on the face is its centre
//! in the face's coordinates (mm), an angle about the face's normal and a
//! flip (mirrored left to right); its size is its width and height (mm),
//! kept in the image's proportions while Lock Aspect is on.
//!
//! The face's coordinates: on a planar face its plane's frame (the model
//! origin projected onto the plane is the origin, the default x axis, as
//! sketches on faces have it), turned so that its normal points out of
//! the material; on a cylindrical or conical face its development
//! (`mitcad_model::developed`, as a sketch on such a face has it), `x`
//! around the axis and `y` along it from the face's low end, so that the
//! decal wraps onto the face keeping its width along the developed face.
//! These start from the component's origin and axes, so Move Features
//! into an existing component placed elsewhere (mitcad#2555), which
//! rebuilds the bodies in the destination's coordinates, rewrites the
//! centres and angles of their decals (`Document::decals_rebased`) and
//! the decals lie where they lay. Seen from outside the material, a decal that is not flipped reads as
//! its image does in a picture viewer.
//!
//! The `decals` query gives each decal's corners and a grid of points on
//! the face with the image's coordinates there (`mesh`), and the triangles
//! of the grid that lie on the face (mitcad#2266, `cut.rs`): a decal is
//! cut at its face's edges, outer and inner loops alike, so that the view
//! and the renderer draw it only on its face. Its place and size stay as
//! they are.
//!
//! A decal goes at most once around a cylindrical or conical face: on a
//! cone, its angle about the apex of the developed face (a fan of
//! `2π·sin(half angle)`) is measured and given along the developed face at
//! the decal's widest row.

mod cut;

use std::collections::HashMap;
use std::f64::consts::TAU;

use mitcad_vec3::{add, dot, scale, sub, unit};

use cut::{Boundary, PieceCut, Unrolled, cut_piece};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::image_references::{EmbeddedImage, ImageSource};
use super::{Document, ModelError, invalid};
use crate::assembly::inverse;
use crate::datum::{DatumPlane, SurfaceGeometry, Vec3};
use crate::developed::{DevelopedSurface, Development};
use crate::features::geom_ref::{GeomRef, Resolver};
use crate::features::{OccurrencePath, is_false};
use crate::ids::{BodyUid, OccurrenceUid};
use crate::kernel::Kernel;
use crate::topo::TopoName;
use crate::transform::Transform;

/// The opacity of a new decal (0 to 1).
pub const DEFAULT_DECAL_OPACITY: f64 = 1.0;

/// At most this many pieces of the grid along each side of a decal on a
/// curved face.
const MAX_PIECES: usize = 64;

/// A piece of the grid on a curved face spans at most this angle about the
/// face's axis (radians, 6 degrees).
const PIECE_ANGLE: f64 = std::f64::consts::PI / 30.0;

/// An image put on a face.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Decal {
    pub name: String,
    /// Its light bulb; left out of files when true.
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub visible: bool,
    /// The face it lies on, a face of a body.
    pub face: GeomRef,
    /// The face's occurrence from the design root, for a body of a
    /// component.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub occurrence: Option<OccurrencePath>,
    /// Its centre in the face's coordinates, mm.
    #[serde(default, skip_serializing_if = "is_origin")]
    pub center: [f64; 2],
    /// Turned about the face's normal, degrees, counter-clockwise seen
    /// from outside the material.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub angle: f64,
    /// Its width and height on the face, mm (along the developed face on a
    /// curved one).
    pub width: f64,
    pub height: f64,
    /// Width and height kept in the image's proportions; left out of files
    /// when true.
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub lock_aspect: bool,
    /// Mirrored left to right.
    #[serde(default, skip_serializing_if = "is_false")]
    pub flip: bool,
    /// 0 (not seen) to 1 (opaque, the image's own transparency only); left
    /// out of files when 1.
    #[serde(default = "opaque", skip_serializing_if = "is_opaque")]
    pub opacity: f64,
    /// The name of the file it came from, for the user; no path.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub file_name: String,
    pub image: EmbeddedImage,
}

fn yes() -> bool {
    true
}

fn is_true(value: &bool) -> bool {
    *value
}

fn opaque() -> f64 {
    DEFAULT_DECAL_OPACITY
}

fn is_opaque(value: &f64) -> bool {
    *value == DEFAULT_DECAL_OPACITY
}

fn is_origin(center: &[f64; 2]) -> bool {
    *center == [0.0, 0.0]
}

fn is_zero(value: &f64) -> bool {
    *value == 0.0
}

/// Where a new decal goes: its face and, optionally, a point (design
/// coordinates) its centre is put on, the point of the face nearest to it.
#[derive(Debug, Clone, PartialEq)]
pub struct DecalTarget {
    pub face: GeomRef,
    pub occurrence: Option<OccurrencePath>,
    pub at: Option<Vec3>,
}

/// What an edit changes; `None` keeps a field as it is.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DecalEdit {
    pub face: Option<(GeomRef, Option<OccurrencePath>)>,
    pub center: Option<[f64; 2]>,
    pub angle: Option<f64>,
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub lock_aspect: Option<bool>,
    pub flip: Option<bool>,
    pub opacity: Option<f64>,
    /// Another image in its place, at the same place and width.
    pub image: Option<ImageSource>,
}

/// The kinds of face a decal goes on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecalSurface {
    Plane,
    Cylinder,
    Cone,
}

impl DecalSurface {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Plane => "plane",
            Self::Cylinder => "cylinder",
            Self::Cone => "cone",
        }
    }
}

/// A grid of points of a decal on its face, in design coordinates: one
/// more row than `rows` of one more point than `columns`, from the image's
/// top row to its
/// bottom row and each from its left column to its right column, with the
/// face's outward normal there and the image's coordinates (`u` from 0 at
/// its left edge to 1 at its right edge, `v` from 0 at its top edge to 1
/// at its bottom edge; a flipped decal's left edge lies on the right).
/// After the grid's points come the points where the face's edges cut it;
/// `triangles` (indices of points, counter-clockwise seen from outside the
/// material) cover the part of the grid on the face, all of it unless
/// `cut`.
#[derive(Debug, Clone, PartialEq)]
pub struct DecalMesh {
    pub columns: usize,
    pub rows: usize,
    pub points: Vec<Vec3>,
    pub normals: Vec<Vec3>,
    pub uvs: Vec<[f64; 2]>,
    pub triangles: Vec<[usize; 3]>,
    /// Part of the decal lies off its face.
    pub cut: bool,
}

impl DecalMesh {
    /// The corners: top left, top right, bottom right, bottom left.
    pub fn corners(&self) -> [Vec3; 4] {
        let last = self.columns;
        let width = self.columns + 1;
        let bottom = self.rows * width;
        [
            self.points[0],
            self.points[last],
            self.points[bottom + last],
            self.points[bottom],
        ]
    }

    fn json(&self) -> Value {
        json!({
            "columns": self.columns,
            "rows": self.rows,
            "points": self.points,
            "normals": self.normals,
            "uvs": self.uvs,
            "triangles": self.triangles,
            "cut": self.cut,
        })
    }
}

/// A face's coordinates in the design: a plane's frame with its normal out
/// of the material, or a curved face's development.
#[derive(Debug, Clone)]
enum FaceMap {
    Plane(DatumPlane),
    Curved(Development),
}

impl FaceMap {
    fn surface(&self) -> DecalSurface {
        match self {
            Self::Plane(_) => DecalSurface::Plane,
            Self::Curved(d) => match d.surface {
                DevelopedSurface::Cylinder { .. } => DecalSurface::Cylinder,
                DevelopedSurface::Cone { .. } => DecalSurface::Cone,
            },
        }
    }

    /// The point of the face at face coordinates.
    fn point(&self, p: [f64; 2]) -> Vec3 {
        match self {
            Self::Plane(plane) => add(
                plane.origin,
                add(scale(plane.x_axis, p[0]), scale(plane.y_axis, p[1])),
            ),
            Self::Curved(d) => d.point(p),
        }
    }

    /// The face's normal out of the material there.
    fn normal(&self, p: [f64; 2]) -> Vec3 {
        match self {
            Self::Plane(plane) => plane.normal(),
            Self::Curved(d) => d.outward(p),
        }
    }

    /// The face coordinates of the face's point nearest to `point`.
    fn coordinates_of(&self, point: Vec3) -> Option<[f64; 2]> {
        match self {
            Self::Plane(plane) => {
                let d = sub(point, plane.origin);
                Some([dot(d, plane.x_axis), dot(d, plane.y_axis)])
            }
            Self::Curved(d) => d.sketch_point_of(point, None),
        }
    }

    /// The face coordinates of the face's point nearest to `point`, on a
    /// curved face on the turn nearest to `near` (face coordinates).
    fn coordinates_near(&self, point: Vec3, near: [f64; 2]) -> Option<[f64; 2]> {
        match self {
            Self::Plane(_) => self.coordinates_of(point),
            Self::Curved(d) => d.sketch_point_of(point, Some(near)),
        }
    }

    /// The length of a piece of the grid on a curved face (mm); none on a
    /// plane, which needs no pieces.
    fn piece(&self, center: [f64; 2]) -> Option<f64> {
        match self {
            Self::Plane(_) => None,
            Self::Curved(d) => {
                // The radius at the decal's centre: a cone's grows along it.
                let at = d.point(center);
                let height = dot(sub(at, d.origin), d.axis);
                let radial = sub(sub(at, d.origin), scale(d.axis, height));
                let radius = mitcad_vec3::norm(radial).max(1e-3);
                Some(radius * PIECE_ANGLE)
            }
        }
    }

    /// Moved by an occurrence's placement (a rigid motion).
    fn moved(self, t: &Transform) -> Self {
        match self {
            Self::Plane(plane) => Self::Plane(DatumPlane {
                origin: t.apply_point(plane.origin),
                x_axis: t.apply_vector(plane.x_axis),
                y_axis: t.apply_vector(plane.y_axis),
            }),
            Self::Curved(mut d) => {
                d.origin = t.apply_point(d.origin);
                d.axis = t.apply_vector(d.axis);
                d.frame.origin = t.apply_point(d.frame.origin);
                d.frame.x_axis = t.apply_vector(d.frame.x_axis);
                d.frame.y_axis = t.apply_vector(d.frame.y_axis);
                Self::Curved(d)
            }
        }
    }
}

/// A decal's frame in the face's coordinates: its centre, the way its
/// columns grow (mirrored when flipped) and the way up its rows, unit
/// vectors.
#[derive(Debug, Clone, Copy)]
struct Placement {
    center: [f64; 2],
    right: [f64; 2],
    up: [f64; 2],
    size: [f64; 2],
}

impl Placement {
    fn of(center: [f64; 2], angle: f64, flip: bool, size: [f64; 2]) -> Self {
        let (sin, cos) = angle.to_radians().sin_cos();
        let sign = if flip { -1.0 } else { 1.0 };
        Self {
            center,
            right: [sign * cos, sign * sin],
            up: [-sin, cos],
            size,
        }
    }

    /// The face coordinates of the image's point at (u, v) (fractions from
    /// its top left corner).
    fn at(&self, [u, v]: [f64; 2]) -> [f64; 2] {
        let x = (u - 0.5) * self.size[0];
        let y = (0.5 - v) * self.size[1];
        [
            self.center[0] + self.right[0] * x + self.up[0] * y,
            self.center[1] + self.right[1] * x + self.up[1] * y,
        ]
    }

    /// The image's coordinates (u, v) at face coordinates `p`.
    fn uv_of(&self, p: [f64; 2]) -> [f64; 2] {
        let d = [p[0] - self.center[0], p[1] - self.center[1]];
        let x = d[0] * self.right[0] + d[1] * self.right[1];
        let y = d[0] * self.up[0] + d[1] * self.up[1];
        [x / self.size[0] + 0.5, 0.5 - y / self.size[1]]
    }
}

/// A cone's apex in its development's coordinates (the centre of the fan
/// the developed face lies in).
fn developed_apex(d: &crate::developed::Development, half_angle: f64) -> [f64; 2] {
    let height = dot(sub(d.frame.origin, d.origin), d.axis);
    [
        0.0,
        -height / half_angle.cos() * dot(d.frame.y_axis, d.axis),
    ]
}

/// Why a decal goes more than once around a cone whose developed face is a
/// fan about `apex` (face coordinates), if it does: its angle about the
/// apex, given along the developed face at its widest row (the row seen at
/// the widest angle from the apex). Otherwise the face point in the middle
/// of that angle, at the decal's centre's distance from the apex.
fn once_around_cone(
    apex: [f64; 2],
    half_angle: f64,
    placement: &Placement,
) -> Result<[f64; 2], String> {
    const ROWS: usize = 64;
    let fan = TAU * half_angle.sin();
    let rel = |p: [f64; 2]| [p[0] - apex[0], p[1] - apex[1]];
    let cross = |a: [f64; 2], b: [f64; 2]| a[0] * b[1] - a[1] * b[0];
    let dot2 = |a: [f64; 2], b: [f64; 2]| a[0] * b[0] + a[1] * b[1];
    let c = rel(placement.center);
    let uv = placement.uv_of(apex);
    if (0.0..=1.0).contains(&uv[0]) && (0.0..=1.0).contains(&uv[1]) {
        return Err(
            "the decal covers the conical face's apex, more than once around the face".to_owned(),
        );
    }
    let angles: Vec<f64> = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]
        .iter()
        .map(|&uv| {
            let p = rel(placement.at(uv));
            cross(c, p).atan2(dot2(c, p))
        })
        .collect();
    let low = angles.iter().copied().fold(f64::INFINITY, f64::min);
    let high = angles.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    if high - low > fan * (1.0 + 1e-9) {
        // The widest row and its middle's distance from the apex.
        let distance = (0..=ROWS)
            .map(|j| {
                let v = j as f64 / ROWS as f64;
                let (l, r) = (rel(placement.at([0.0, v])), rel(placement.at([1.0, v])));
                let seen = cross(l, r).atan2(dot2(l, r)).abs();
                let middle = [(l[0] + r[0]) / 2.0, (l[1] + r[1]) / 2.0];
                (seen, middle[0].hypot(middle[1]))
            })
            .max_by(|a, b| a.0.total_cmp(&b.0))
            .map_or(0.0, |(_, distance)| distance);
        return Err(format!(
            "the decal is {:.3} mm wide around the conical face at its widest row, more than once \
             around it ({:.3} mm there)",
            (high - low) * distance,
            fan * distance
        ));
    }
    let (sin, cos) = (0.5 * (low + high)).sin_cos();
    Ok([
        apex[0] + c[0] * cos - c[1] * sin,
        apex[1] + c[0] * sin + c[1] * cos,
    ])
}

/// The grid of a decal on a face whose boundary is `runs` (each a run of
/// points along an edge, design coordinates), with the triangles of it on
/// the face; all of it when the face gives no boundary.
fn decal_mesh(
    map: &FaceMap,
    placement: &Placement,
    runs: &[Vec<Vec3>],
) -> Result<DecalMesh, String> {
    let (columns, rows) = match map.piece(placement.center) {
        None => (1, 1),
        Some(piece) => {
            // Along the image's right and up: where the face curves, the
            // pieces it takes there (a cylinder curves only around it).
            let pieces = |along: [f64; 2], length: f64| {
                let across = match map.surface() {
                    DecalSurface::Cylinder => along[0].abs(),
                    _ => 1.0,
                };
                ((length * across / piece).ceil() as usize).clamp(1, MAX_PIECES)
            };
            (
                pieces(placement.right, placement.size[0]),
                pieces(placement.up, placement.size[1]),
            )
        }
    };
    if let FaceMap::Curved(d) = map
        && let DevelopedSurface::Cylinder { radius } = d.surface
    {
        // Once around at most: the corners' extent around the axis.
        let around: Vec<f64> = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]
            .iter()
            .map(|&uv| placement.at(uv)[0])
            .collect();
        let low = around.iter().copied().fold(f64::INFINITY, f64::min);
        let high = around.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let circumference = std::f64::consts::TAU * radius;
        if high - low > circumference * (1.0 + 1e-9) {
            return Err(format!(
                "the decal is {:.3} mm wide around the cylindrical face, more than once around it ({circumference:.3} mm)",
                high - low
            ));
        }
    }
    // How the face unrolls about the decal's middle; once around a cone
    // at most.
    let (unrolled, middle) = match map {
        FaceMap::Plane(_) => (Unrolled::Flat, placement.center),
        FaceMap::Curved(d) => match d.surface {
            DevelopedSurface::Cylinder { radius } => (
                Unrolled::Around {
                    period: TAU * radius,
                },
                placement.center,
            ),
            DevelopedSurface::Cone { half_angle } => {
                let apex = developed_apex(d, half_angle);
                let middle = once_around_cone(apex, half_angle, placement)?;
                (
                    Unrolled::Fan {
                        apex,
                        angle: TAU * half_angle.sin(),
                    },
                    middle,
                )
            }
        },
    };
    let mut mesh = DecalMesh {
        columns,
        rows,
        points: Vec::with_capacity((columns + 1) * (rows + 1)),
        normals: Vec::with_capacity((columns + 1) * (rows + 1)),
        uvs: Vec::with_capacity((columns + 1) * (rows + 1)),
        triangles: Vec::new(),
        cut: false,
    };
    // The grid's points in the face's coordinates, keyed for the points
    // the cut shares with them.
    let key = |p: [f64; 2]| ((p[0] * 1e7).round() as i64, (p[1] * 1e7).round() as i64);
    let mut flat = Vec::with_capacity(mesh.points.capacity());
    let mut known: HashMap<(i64, i64), usize> = HashMap::new();
    let add_point = |mesh: &mut DecalMesh, p: [f64; 2], uv: [f64; 2]| -> Result<usize, String> {
        let point = map.point(p);
        if !point.iter().all(|c| c.is_finite()) {
            return Err("the decal does not lie on its face (a cone's apex)".to_owned());
        }
        mesh.points.push(point);
        mesh.normals.push(map.normal(p));
        mesh.uvs.push(uv);
        Ok(mesh.points.len() - 1)
    };
    for j in 0..=rows {
        for i in 0..=columns {
            let uv = [i as f64 / columns as f64, j as f64 / rows as f64];
            let p = placement.at(uv);
            let index = add_point(&mut mesh, p, uv)?;
            known.entry(key(p)).or_insert(index);
            flat.push(p);
        }
    }
    let boundary = Boundary::new(unrolled, middle, runs, &|p, near| {
        map.coordinates_near(p, near)
    });
    let size = placement.size[0].max(placement.size[1]);
    let at = |row: usize, column: usize| row * (columns + 1) + column;
    let mut covered = 0.0;
    for row in 0..rows {
        for column in 0..columns {
            let corners = [
                at(row, column),
                at(row, column + 1),
                at(row + 1, column + 1),
                at(row + 1, column),
            ];
            let piece = corners.map(|k| flat[k]);
            let on_face = if boundary.is_empty() {
                PieceCut::Whole
            } else {
                cut_piece(&boundary, piece, size)
            };
            match on_face {
                PieceCut::Whole => {
                    // Two triangles counter-clockwise in the face's
                    // coordinates, which are seen from outside.
                    let [top_left, top_right, bottom_right, bottom_left] = corners;
                    let first = [piece[0], piece[3], piece[2]];
                    if cut::area(first) > 0.0 {
                        mesh.triangles.push([top_left, bottom_left, bottom_right]);
                        mesh.triangles.push([top_left, bottom_right, top_right]);
                    } else {
                        mesh.triangles.push([top_left, bottom_right, bottom_left]);
                        mesh.triangles.push([top_left, top_right, bottom_right]);
                    }
                    covered += 2.0 * cut::area(first).abs();
                }
                PieceCut::Triangles(triangles) => {
                    for triangle in triangles {
                        covered += cut::area(triangle);
                        let mut indices = [0; 3];
                        for (k, p) in triangle.into_iter().enumerate() {
                            indices[k] = match known.get(&key(p)) {
                                Some(&index) => index,
                                None => {
                                    let index = add_point(&mut mesh, p, placement.uv_of(p))?;
                                    known.insert(key(p), index);
                                    index
                                }
                            };
                        }
                        mesh.triangles.push(indices);
                    }
                }
                PieceCut::Off => {}
            }
        }
    }
    mesh.cut = covered < placement.size[0] * placement.size[1] * (1.0 - 1e-9);
    Ok(mesh)
}

/// Why a size is no size, if it is not.
fn check_length(what: &str, value: f64) -> Result<(), String> {
    if value > 0.0 && value.is_finite() {
        Ok(())
    } else {
        Err(format!(
            "the decal's {what} must be greater than zero, got {value}"
        ))
    }
}

/// The height of an image `pixels` large at `width`, in its proportions.
fn height_of(pixels: [u32; 2], width: f64) -> f64 {
    width * f64::from(pixels[1].max(1)) / f64::from(pixels[0].max(1))
}

/// The width and height an edit gives an image `pixels` large that is
/// `size` now: with Lock Aspect a width or a height given sets the other
/// in the image's proportions (both given must agree with them), without
/// it each is set alone.
fn sized(
    pixels: [u32; 2],
    size: [f64; 2],
    width: Option<f64>,
    height: Option<f64>,
    locked: bool,
) -> Result<[f64; 2], String> {
    if let Some(width) = width {
        check_length("width", width)?;
    }
    if let Some(height) = height {
        check_length("height", height)?;
    }
    if !locked {
        return Ok([width.unwrap_or(size[0]), height.unwrap_or(size[1])]);
    }
    Ok(match (width, height) {
        (Some(width), Some(height)) => {
            if (height - height_of(pixels, width)).abs() > 1e-6 * height.max(1.0) {
                return Err(format!(
                    "Lock Aspect keeps the image's proportions: give the decal's width or its height, not \
                     {width} by {height} (or turn Lock Aspect off)"
                ));
            }
            [width, height]
        }
        (Some(width), None) => [width, height_of(pixels, width)],
        (None, Some(height)) => [
            height * f64::from(pixels[0].max(1)) / f64::from(pixels[1].max(1)),
            height,
        ],
        (None, None) => [size[0], height_of(pixels, size[0])],
    })
}

impl Decal {
    /// Why the decal cannot be kept, if it cannot (not its face).
    pub(crate) fn check(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("the decal needs a name".to_owned());
        }
        if !matches!(self.face, GeomRef::Face { .. }) {
            return Err(format!("{}: a decal goes on a face of a body", self.name));
        }
        check_length("width", self.width).map_err(|e| format!("{}: {e}", self.name))?;
        check_length("height", self.height).map_err(|e| format!("{}: {e}", self.name))?;
        if !(0.0..=1.0).contains(&self.opacity) {
            return Err(format!(
                "{}: the opacity must be from 0 to 1, got {}",
                self.name, self.opacity
            ));
        }
        if !self.angle.is_finite() || !self.center.iter().all(|c| c.is_finite()) {
            return Err(format!(
                "{}: the center and the angle must be finite",
                self.name
            ));
        }
        Ok(())
    }

    /// Also checks that the embedded bytes are the image the fields say
    /// (a design file's).
    pub(crate) fn check_image(&self) -> Result<(), String> {
        self.check()?;
        self.image
            .check()
            .map_err(|e| format!("{}: {e}", self.name))
    }

    fn placement(&self) -> Placement {
        Placement::of(
            self.center,
            self.angle,
            self.flip,
            [self.width, self.height],
        )
    }

    fn apply(&mut self, edit: DecalEdit) -> Result<(), String> {
        if let Some((face, occurrence)) = edit.face {
            self.face = face;
            self.occurrence = occurrence;
        }
        let locked = edit.lock_aspect.unwrap_or(self.lock_aspect);
        if let Some(source) = edit.image {
            let (image, _) = source.embed()?;
            self.image = image;
            self.file_name = source.file_name;
            if locked {
                // The same width, the new image's proportions.
                self.height = height_of(self.image.pixels(), self.width);
            }
        }
        let size = sized(
            self.image.pixels(),
            [self.width, self.height],
            edit.width,
            edit.height,
            locked,
        )?;
        [self.width, self.height] = size;
        self.lock_aspect = locked;
        self.center = edit.center.unwrap_or(self.center);
        self.angle = edit.angle.unwrap_or(self.angle);
        self.flip = edit.flip.unwrap_or(self.flip);
        self.opacity = edit.opacity.unwrap_or(self.opacity);
        Ok(())
    }
}

impl<K: Kernel> Document<K> {
    /// The decals in the order they were inserted.
    pub fn decals(&self) -> &[Decal] {
        &self.state.decals
    }

    /// The coordinates of a decal's face at the marker, in design
    /// coordinates, with its boundary (runs of points along its edges, in
    /// design coordinates), or why the face is not there or takes no decal.
    fn decal_face(
        &self,
        face: &GeomRef,
        occurrence: Option<&OccurrencePath>,
    ) -> Result<(FaceMap, Vec<Vec<Vec3>>), String> {
        self.decal_face_rebased(face, occurrence, &Transform::IDENTITY)
    }

    /// [`Self::decal_face`] as the face's component would have the face's
    /// coordinates with its body rebuilt in other coordinates, `rebase`
    /// taking the component's coordinates to them, the face staying where
    /// it is in the design (Move Features into an existing component
    /// placed elsewhere, mitcad#2555): a planar face's origin and a curved
    /// face's reference direction and end start from that component's own
    /// origin and axes.
    fn decal_face_rebased(
        &self,
        face: &GeomRef,
        occurrence: Option<&OccurrencePath>,
        rebase: &Transform,
    ) -> Result<(FaceMap, Vec<Vec<Vec3>>), String> {
        let GeomRef::Face { body, face: name } = face else {
            return Err("a decal goes on a face of a body".to_owned());
        };
        let path = self.analysis_occurrence(face, occurrence, "decal's face")?;
        let transform = self.path_transform(&path.0);
        let placed = if *rebase == Transform::IDENTITY {
            transform
        } else {
            transform.after(&inverse(rebase))
        };
        let mut resolver = self.resolver();
        let shape = resolver.shape(*body)?;
        let found = crate::features::renamed::face_for(&mut resolver, &shape, name)?;
        let kernel = self.kernel();
        let geometry = kernel.face_geometry(&shape, &found);
        let map = match &geometry {
            Ok(SurfaceGeometry::Plane { origin, normal }) => {
                // Its normal out of the material, so that the decal reads
                // from outside.
                let outward = match kernel.face_plane(&shape, &found) {
                    Ok(plane) if dot(plane.normal, *normal) < 0.0 => scale(*normal, -1.0),
                    _ => *normal,
                };
                FaceMap::Plane(DatumPlane::on_plane(
                    rebase.apply_point(*origin),
                    rebase.apply_vector(outward),
                ))
            }
            Ok(SurfaceGeometry::Cylinder { .. } | SurfaceGeometry::Cone { .. }) | Err(_) => {
                match curved(kernel, &shape, &found, rebase) {
                    Some(map) => map?,
                    None => {
                        return Err(match &geometry {
                            Ok(other) => format!(
                                "a decal goes on a planar, cylindrical or conical face; face {name} is a {}",
                                other.kind()
                            ),
                            Err(e) => format!(
                                "face {name} of {} is not there at the timeline marker: {e}",
                                self.body_name(*body)
                            ),
                        });
                    }
                }
            }
            Ok(other) => {
                return Err(format!(
                    "a decal goes on a planar, cylindrical or conical face; face {name} is a {}",
                    other.kind()
                ));
            }
        };
        let planar = matches!(map, FaceMap::Plane(_));
        let runs = boundary_runs(kernel, &shape, &found, planar)
            .into_iter()
            .map(|run| run.into_iter().map(|p| transform.apply_point(p)).collect())
            .collect();
        Ok((map.moved(&placed), runs))
    }

    /// A decal's grid on its face at the marker, or why it has none.
    pub fn decal_mesh(&self, decal: &Decal) -> Result<(DecalSurface, DecalMesh), String> {
        let (map, runs) = self.decal_face(&decal.face, decal.occurrence.as_ref())?;
        let mesh = decal_mesh(&map, &decal.placement(), &runs)?;
        Ok((map.surface(), mesh))
    }

    /// The new centres and angles (by name) of the decals on faces of
    /// `bodies` placed through `from` (a path from the root) that keep them
    /// where they lie in the design once those bodies are rebuilt in other
    /// coordinates, `rebase` taking their component's coordinates to the
    /// new ones (Move Features into an existing component placed
    /// elsewhere, mitcad#2555). A face's coordinates start from its
    /// component's origin and axes (a planar face's origin, a curved face's
    /// reference direction and the end it is measured from), so the same
    /// face seen from another component's coordinates has its coordinates
    /// shifted and turned. Decals whose coordinates stay and decals whose
    /// face is not there at the marker are not listed.
    pub(crate) fn decals_rebased(
        &self,
        bodies: &[BodyUid],
        from: &[OccurrenceUid],
        rebase: &Transform,
    ) -> Vec<(String, [f64; 2], f64)> {
        let mut rebased = Vec::new();
        for decal in &self.state.decals {
            let GeomRef::Face { body, .. } = &decal.face else {
                continue;
            };
            if !bodies.contains(body) {
                continue;
            }
            let occurrence = decal.occurrence.as_ref();
            if !self
                .analysis_occurrence(&decal.face, occurrence, "decal's face")
                .is_ok_and(|path| path.0 == from)
            {
                continue;
            }
            let (Ok((old, _)), Ok((new, _))) = (
                self.decal_face(&decal.face, occurrence),
                self.decal_face_rebased(&decal.face, occurrence, rebase),
            ) else {
                continue;
            };
            if old.surface() != new.surface() {
                continue;
            }
            // The decal's centre and a point 1 mm along its columns (its
            // angle's direction), where they lie in the design, read in
            // the new coordinates: a face's coordinates in two frames
            // differ by a shift and a turn.
            let [x, y] = decal.center;
            let Some(center) = new.coordinates_near(old.point(decal.center), decal.center) else {
                continue;
            };
            let (sin, cos) = decal.angle.to_radians().sin_cos();
            let Some(ahead) = new.coordinates_near(old.point([x + cos, y + sin]), center) else {
                continue;
            };
            let center = if (center[0] - x).abs() < 1e-9 && (center[1] - y).abs() < 1e-9 {
                decal.center
            } else {
                center
            };
            let turned = (ahead[1] - center[1])
                .atan2(ahead[0] - center[0])
                .to_degrees();
            let mut turn = turned - decal.angle;
            turn -= 360.0 * (turn / 360.0).round();
            let angle = if turn.abs() < 1e-9 {
                decal.angle
            } else {
                let mut angle = decal.angle + turn;
                angle -= 360.0 * (angle / 360.0).round();
                angle
            };
            if center != decal.center || angle != decal.angle {
                rebased.push((decal.name.clone(), center, angle));
            }
        }
        rebased
    }

    /// The `decals` query: each decal's settings without its bytes, with
    /// its pixels, digest, the kind of its face, its corners and its grid
    /// at the marker (`corners`, `mesh`) or why its face is not there
    /// (`error`).
    pub fn decals_json(&self) -> Value {
        Value::Array(
            self.state
                .decals
                .iter()
                .map(|decal| {
                    let mut value = serde_json::to_value(decal).expect("decals serialize");
                    if let Some(fields) = value.as_object_mut() {
                        fields.remove("image");
                    }
                    value["visible"] = json!(decal.visible);
                    value["center"] = json!(decal.center);
                    value["angle"] = json!(decal.angle);
                    value["lock_aspect"] = json!(decal.lock_aspect);
                    value["flip"] = json!(decal.flip);
                    value["opacity"] = json!(decal.opacity);
                    value["file_name"] = json!(decal.file_name);
                    value["format"] = json!(decal.image.format);
                    value["pixels"] = json!(decal.image.pixels());
                    value["sha256"] = json!(decal.image.data.sha256());
                    match self.decal_mesh(decal) {
                        Ok((surface, mesh)) => {
                            value["surface"] = json!(surface.as_str());
                            value["corners"] = json!(mesh.corners());
                            value["mesh"] = mesh.json();
                        }
                        Err(error) => value["error"] = json!(error),
                    }
                    value
                })
                .collect(),
        )
    }

    /// The `decal_image` query: a decal's file, base64.
    pub fn decal_image_json(&self, name: &str) -> Result<Value, ModelError> {
        let decal = &self.state.decals[self.decal_index(name)?];
        Ok(json!({
            "name": decal.name,
            "format": decal.image.format,
            "pixels": decal.image.pixels(),
            "sha256": decal.image.data.sha256(),
            "data": decal.image.data.base64(),
        }))
    }

    /// The `decal_outline` query: where a decal of an image `pixels` large
    /// would lie with these settings (a panel's live result): the kind of
    /// face, its centre in the face's coordinates (`at` taken onto the
    /// face, for a panel's X and Y), its size, corners and grid. Without a
    /// width or height it is the width `default_width` (mm) wide.
    pub fn decal_outline(
        &self,
        target: &DecalTarget,
        pixels: [u32; 2],
        edit: &DecalEdit,
        default_width: f64,
    ) -> Result<Value, ModelError> {
        let (map, runs) = self
            .decal_face(&target.face, target.occurrence.as_ref())
            .map_err(|e| invalid(format!("the decal's face: {e}")))?;
        let center = match (edit.center, target.at) {
            (Some(center), _) => center,
            (None, Some(at)) => map
                .coordinates_of(at)
                .ok_or_else(|| invalid("the point is on the face's axis"))?,
            (None, None) => [0.0, 0.0],
        };
        check_length("width", default_width).map_err(invalid)?;
        let start = [default_width, height_of(pixels, default_width)];
        let size = sized(
            pixels,
            start,
            edit.width,
            edit.height,
            edit.lock_aspect.unwrap_or(true),
        )
        .map_err(invalid)?;
        let placement = Placement::of(
            center,
            edit.angle.unwrap_or(0.0),
            edit.flip.unwrap_or(false),
            size,
        );
        let mesh = decal_mesh(&map, &placement, &runs).map_err(invalid)?;
        Ok(json!({
            "surface": map.surface().as_str(),
            "center": center,
            "size": size,
            "corners": mesh.corners(),
            "mesh": mesh.json(),
        }))
    }

    /// Puts an image on a face, shown: centred on the face's point nearest
    /// to `target.at` (or the face's origin), as wide as the image's file
    /// states (or 96 pixels to the inch) and opaque unless `edit` says
    /// otherwise. Without a name it is `Decal1`, `Decal2`, ... (the next
    /// free number). Returns the name.
    pub fn insert_decal(
        &mut self,
        source: ImageSource,
        target: DecalTarget,
        edit: DecalEdit,
        name: Option<&str>,
    ) -> Result<String, ModelError> {
        let name = match name.map(str::trim).filter(|n| !n.is_empty()) {
            Some(name) => name.to_owned(),
            None => (1..)
                .map(|n| format!("Decal{n}"))
                .find(|name| self.decal_index(name).is_err())
                .expect("a free name"),
        };
        if self.decal_index(&name).is_ok() {
            return Err(invalid(format!("a decal {name} exists")));
        }
        let (map, runs) = self
            .decal_face(&target.face, target.occurrence.as_ref())
            .map_err(|e| invalid(format!("the decal's face: {e}")))?;
        let (image, pixel_size) = source.embed().map_err(invalid)?;
        let center = match target.at {
            Some(at) => map
                .coordinates_of(at)
                .ok_or_else(|| invalid("the point is on the face's axis"))?,
            None => [0.0, 0.0],
        };
        let width = f64::from(image.width) * pixel_size;
        let mut decal = Decal {
            name: name.clone(),
            visible: true,
            face: target.face,
            occurrence: target.occurrence,
            center,
            angle: 0.0,
            width,
            height: height_of(image.pixels(), width),
            lock_aspect: true,
            flip: false,
            opacity: DEFAULT_DECAL_OPACITY,
            file_name: source.file_name,
            image,
        };
        decal
            .apply(DecalEdit {
                face: None,
                image: None,
                ..edit
            })
            .map_err(invalid)?;
        decal.check().map_err(invalid)?;
        decal_mesh(&map, &decal.placement(), &runs).map_err(invalid)?;
        let label = format!("Insert {name}");
        self.apply_with(|state| {
            state.decals.push(decal);
            Ok((label, (), false))
        })?;
        Ok(name)
    }

    /// Changes a decal's settings, its face or its image. A change that
    /// changes nothing is no undo step.
    pub fn edit_decal(&mut self, name: &str, edit: DecalEdit) -> Result<(), ModelError> {
        let i = self.decal_index(name)?;
        let moved = edit.face.is_some();
        let mut decal = self.state.decals[i].clone();
        decal.apply(edit).map_err(invalid)?;
        decal.check().map_err(invalid)?;
        // On its face where the face is there (a decal whose face is gone
        // keeps its settings until it is put on another face).
        match self.decal_face(&decal.face, decal.occurrence.as_ref()) {
            Ok((map, runs)) => {
                decal_mesh(&map, &decal.placement(), &runs).map_err(invalid)?;
            }
            Err(e) if moved => return Err(invalid(format!("the decal's face: {e}"))),
            Err(_) => {}
        }
        if decal == self.state.decals[i] {
            return Ok(());
        }
        let label = format!("Edit {name}");
        self.apply_with(|state| {
            state.decals[i] = decal;
            Ok((label, (), false))
        })
    }

    /// Shows or hides a decal; an undo step `Show Decal1` / `Hide Decal1`
    /// unless nothing changes.
    pub fn set_decal_visible(&mut self, name: &str, visible: bool) -> Result<(), ModelError> {
        let i = self.decal_index(name)?;
        if self.state.decals[i].visible == visible {
            return Ok(());
        }
        let label = format!("{} {name}", if visible { "Show" } else { "Hide" });
        self.apply_with(|state| {
            state.decals[i].visible = visible;
            Ok((label, (), false))
        })
    }

    pub fn rename_decal(&mut self, name: &str, new_name: &str) -> Result<(), ModelError> {
        let i = self.decal_index(name)?;
        let new_name = new_name.trim();
        if new_name.is_empty() {
            return Err(invalid("the decal needs a name"));
        }
        if new_name == name {
            return Ok(());
        }
        if self.decal_index(new_name).is_ok() {
            return Err(invalid(format!("a decal {new_name} exists")));
        }
        let label = format!("Rename {name}");
        let new_name = new_name.to_owned();
        self.apply_with(|state| {
            state.decals[i].name = new_name;
            Ok((label, (), false))
        })
    }

    pub fn delete_decal(&mut self, name: &str) -> Result<(), ModelError> {
        let i = self.decal_index(name)?;
        let label = format!("Delete {name}");
        self.apply_with(|state| {
            state.decals.remove(i);
            Ok((label, (), false))
        })
    }

    fn decal_index(&self, name: &str) -> Result<usize, ModelError> {
        self.state
            .decals
            .iter()
            .position(|decal| decal.name == name)
            .ok_or_else(|| invalid(format!("there is no decal {name}")))
    }
}

/// A face's boundary, a decal is cut at (mitcad#2266): each of its edges
/// as a run of points along it, in the body's coordinates. A planar face's
/// boundary without the edges between pieces of it (`face_boundary`), any
/// other's every edge (`curves_of`: a seam does not cut, see `cut.rs`).
/// None when the kernel gives no edges: the decal is not cut.
fn boundary_runs<K: Kernel>(
    kernel: &K,
    shape: &K::Shape,
    face: &crate::topo::FaceName,
    planar: bool,
) -> Vec<Vec<Vec3>> {
    let named = if planar {
        kernel
            .face_boundary(shape, face)
            .ok()
            .filter(|edges| !edges.is_empty())
            .map(|edges| edges.into_iter().map(|(_, curve)| curve).collect())
    } else {
        None
    };
    let curves = named.unwrap_or_else(|| {
        kernel
            .curves_of(shape, &TopoName::Face(face.clone()))
            .unwrap_or_default()
    });
    curves
        .iter()
        .map(crate::features::path::samples)
        .filter(|run| run.len() >= 2)
        .collect()
}

/// The development of a cylindrical or conical face as a sketch on it has
/// it (`features/developed.rs`), in the body's coordinates: `x` around the
/// axis from the reference direction across it (the model X axis, else
/// Y), `y` along the axis counted towards +Z (else +Y, else +X) from the
/// face's low end (a cone's other end when the low end is its apex). None
/// when the face is neither; an error when it is one with no size.
fn curved<K: Kernel>(
    kernel: &K,
    shape: &K::Shape,
    face: &crate::topo::FaceName,
    rebase: &Transform,
) -> Option<Result<FaceMap, String>> {
    use crate::features::emboss::counted;
    if let Ok(cylinder) = kernel.face_cylinder(shape, face) {
        let Some(along) = unit(rebase.apply_vector(cylinder.axis.direction)) else {
            return Some(Err("the cylindrical face has no axis".to_owned()));
        };
        let origin = rebase.apply_point(cylinder.axis.origin);
        let up = counted(along);
        let start = if dot(up, along) > 0.0 {
            origin
        } else {
            add(origin, scale(along, cylinder.length))
        };
        return Some(
            Development::new(
                DevelopedSurface::Cylinder {
                    radius: cylinder.radius,
                },
                start,
                up,
                cylinder.internal,
                0.0,
                crate::datum::default_x_axis(up),
                0.0,
                up,
            )
            .map(FaceMap::Curved)
            .ok_or_else(|| "the cylindrical face has no size".to_owned()),
        );
    }
    if let Ok(cone) = kernel.face_cone(shape, face) {
        // Heights along the axis from the apex are the same in any
        // coordinates; the apex and the axis are taken into `rebase`'s.
        let apex = rebase.apply_point(cone.apex);
        let axis = rebase.apply_vector(cone.axis);
        let up = counted(axis);
        let curves = match kernel.curves_of(shape, &TopoName::Face(face.clone())) {
            Ok(curves) => curves,
            Err(e) => return Some(Err(format!("the conical face: {e}"))),
        };
        let heights: Vec<f64> = curves
            .iter()
            .flat_map(crate::features::path::samples)
            .map(|p| dot(sub(p, cone.apex), cone.axis))
            .collect();
        let low = heights.iter().copied().fold(f64::INFINITY, f64::min);
        let high = heights.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        if !(low.is_finite() && high.is_finite() && high > 0.0) {
            return Some(Err(
                "the conical face has no edges to measure it by".to_owned()
            ));
        }
        let size = high.abs().max(1.0);
        let (first, other) = if dot(up, axis) > 0.0 {
            (low, high)
        } else {
            (high, low)
        };
        let start = if first > 1e-6 * size { first } else { other };
        return Some(
            Development::new(
                DevelopedSurface::Cone {
                    half_angle: cone.half_angle,
                },
                apex,
                axis,
                cone.internal,
                start,
                crate::datum::default_x_axis(up),
                0.0,
                up,
            )
            .map(FaceMap::Curved)
            .ok_or_else(|| "the conical face lies only at its apex".to_owned()),
        );
    }
    None
}

/// The unit normal of the plane through three points, for tests.
#[cfg(test)]
fn normal_of(a: Vec3, b: Vec3, c: Vec3) -> Option<Vec3> {
    unit(mitcad_vec3::cross(sub(b, a), sub(c, a)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: Vec3, b: Vec3) -> bool {
        (0..3).all(|i| (a[i] - b[i]).abs() < 1e-9)
    }

    fn xy() -> FaceMap {
        FaceMap::Plane(DatumPlane {
            origin: [0.0, 0.0, 0.0],
            x_axis: [1.0, 0.0, 0.0],
            y_axis: [0.0, 1.0, 0.0],
        })
    }

    #[test]
    fn a_decal_on_a_plane_is_one_quad_turned_and_flipped() {
        let mesh = decal_mesh(
            &xy(),
            &Placement::of([0.0, 0.0], 0.0, false, [20.0, 10.0]),
            &[],
        )
        .unwrap();
        assert_eq!((mesh.columns, mesh.rows), (1, 1));
        let [top_left, top_right, bottom_right, _] = mesh.corners();
        assert!(near(top_left, [-10.0, 5.0, 0.0]));
        assert!(near(bottom_right, [10.0, -5.0, 0.0]));
        assert!(near(
            normal_of(top_left, bottom_right, top_right).unwrap(),
            [0.0, 0.0, 1.0]
        ));
        // Flipped: its left edge on the right.
        let flipped = decal_mesh(
            &xy(),
            &Placement::of([0.0, 0.0], 0.0, true, [20.0, 10.0]),
            &[],
        )
        .unwrap();
        assert!(near(flipped.corners()[0], [10.0, 5.0, 0.0]));
        // A quarter turn: its top edge on the left.
        let turned = decal_mesh(
            &xy(),
            &Placement::of([0.0, 0.0], 90.0, false, [20.0, 10.0]),
            &[],
        )
        .unwrap();
        assert!(near(turned.corners()[0], [-5.0, -10.0, 0.0]));
        assert_eq!(turned.uvs[0], [0.0, 0.0]);
    }

    #[test]
    fn locked_sizes_keep_the_proportions() {
        let pixels = [200, 100];
        assert_eq!(
            sized(pixels, [20.0, 10.0], Some(40.0), None, true).unwrap(),
            [40.0, 20.0]
        );
        assert_eq!(
            sized(pixels, [20.0, 10.0], None, Some(5.0), true).unwrap(),
            [10.0, 5.0]
        );
        assert_eq!(
            sized(pixels, [20.0, 7.0], None, None, true).unwrap(),
            [20.0, 10.0]
        );
        assert_eq!(
            sized(pixels, [20.0, 10.0], None, Some(30.0), false).unwrap(),
            [20.0, 30.0]
        );
        assert!(sized(pixels, [20.0, 10.0], Some(40.0), Some(40.0), true).is_err());
        assert!(sized(pixels, [20.0, 10.0], Some(0.0), None, false).is_err());
    }

    #[test]
    fn a_decal_wraps_onto_a_cylinder_keeping_its_width() {
        let z = [0.0, 0.0, 1.0];
        let development = Development::new(
            DevelopedSurface::Cylinder { radius: 10.0 },
            [0.0; 3],
            z,
            false,
            0.0,
            [1.0, 0.0, 0.0],
            0.0,
            z,
        )
        .unwrap();
        let rod = FaceMap::Curved(development);
        let placement = Placement::of([0.0, 5.0], 0.0, false, [20.0, 4.0]);
        let mesh = decal_mesh(&rod, &placement, &[]).unwrap();
        assert_eq!(mesh.rows, 1, "a cylinder curves only around it");
        assert!(mesh.columns >= 2);
        // Every point on the face, its top row's arc 20 mm long.
        for p in &mesh.points {
            assert!((p[0].hypot(p[1]) - 10.0).abs() < 1e-9);
        }
        let [left, right, ..] = mesh.corners();
        let swept = (left[0] * right[0] + left[1] * right[1]) / 100.0;
        assert!((swept.acos() * 10.0 - 20.0).abs() < 1e-9);
        // More than once around is refused.
        let around = Placement::of([0.0, 5.0], 0.0, false, [70.0, 4.0]);
        assert!(decal_mesh(&rod, &around, &[]).is_err());
    }

    /// The points the triangles use, and the triangles' area and normals
    /// (each must point along `outward(p)` at its first point).
    fn on_face(mesh: &DecalMesh, outward: impl Fn(Vec3) -> Vec3) -> (Vec<Vec3>, f64) {
        let mut used = Vec::new();
        let mut area = 0.0;
        for t in &mesh.triangles {
            let [a, b, c] = t.map(|k| mesh.points[k]);
            let n = mitcad_vec3::cross(sub(b, a), sub(c, a));
            assert!(
                dot(n, outward(a)) > 0.0,
                "{t:?} does not face out of the material"
            );
            area += 0.5 * mitcad_vec3::norm(n);
            used.extend([a, b, c]);
        }
        (used, area)
    }

    #[test]
    fn a_decal_is_cut_at_its_planar_faces_edges() {
        // A 20 x 20 face from the origin; the decal 10 x 6 centred on its
        // right edge, half on it.
        let square = vec![
            [0.0, 0.0, 0.0],
            [20.0, 0.0, 0.0],
            [20.0, 20.0, 0.0],
            [0.0, 20.0, 0.0],
            [0.0, 0.0, 0.0],
        ];
        for flip in [false, true] {
            let placement = Placement::of([20.0, 10.0], 0.0, flip, [10.0, 6.0]);
            let mesh = decal_mesh(&xy(), &placement, std::slice::from_ref(&square)).unwrap();
            assert!(mesh.cut);
            // Its place and size as they were.
            assert!(near(
                mesh.corners()[2],
                [if flip { 15.0 } else { 25.0 }, 7.0, 0.0]
            ));
            let (used, area) = on_face(&mesh, |_| [0.0, 0.0, 1.0]);
            assert!((area - 30.0).abs() < 1e-9, "{area}");
            assert!(used.iter().all(|p| p[0] <= 20.0 + 1e-9));
            // The image's middle column at the face's edge.
            let k = mesh
                .points
                .iter()
                .position(|p| near(*p, [20.0, 13.0, 0.0]))
                .unwrap();
            assert!((mesh.uvs[k][0] - 0.5).abs() < 1e-12 && mesh.uvs[k][1].abs() < 1e-12);
        }
        // Wholly on the face: its grid's two triangles; wholly off: none.
        let inside = decal_mesh(
            &xy(),
            &Placement::of([10.0, 10.0], 30.0, false, [6.0, 4.0]),
            std::slice::from_ref(&square),
        )
        .unwrap();
        assert!(!inside.cut);
        assert_eq!((inside.points.len(), inside.triangles.len()), (4, 2));
        let off = decal_mesh(
            &xy(),
            &Placement::of([40.0, 10.0], 0.0, false, [6.0, 4.0]),
            std::slice::from_ref(&square),
        )
        .unwrap();
        assert!(off.cut && off.triangles.is_empty());
    }

    #[test]
    fn a_decal_is_cut_at_a_cylinders_ends_not_at_its_seam() {
        let z = [0.0, 0.0, 1.0];
        let rod = FaceMap::Curved(
            Development::new(
                DevelopedSurface::Cylinder { radius: 10.0 },
                [0.0; 3],
                z,
                false,
                0.0,
                [1.0, 0.0, 0.0],
                0.0,
                z,
            )
            .unwrap(),
        );
        // The side from z = 0 to 30: its two circles and its seam on +X.
        let circle = |h: f64| -> Vec<Vec3> {
            (0..=48)
                .map(|k| {
                    let a = TAU * k as f64 / 48.0;
                    [10.0 * a.cos(), 10.0 * a.sin(), h]
                })
                .collect()
        };
        let runs = vec![
            circle(0.0),
            circle(30.0),
            vec![[10.0, 0.0, 0.0], [10.0, 0.0, 30.0]],
        ];
        let radial = |p: Vec3| [p[0], p[1], 0.0];
        // Across the seam: whole.
        let across = decal_mesh(
            &rod,
            &Placement::of([0.0, 15.0], 0.0, false, [20.0, 8.0]),
            &runs,
        )
        .unwrap();
        assert!(!across.cut);
        assert_eq!(across.triangles.len(), 2 * across.columns);
        // Past the top end by 2 of its 8 mm, and half a turn away on the
        // other side of where the face is unrolled.
        for x in [0.0, 10.0 * std::f64::consts::PI] {
            let top = decal_mesh(
                &rod,
                &Placement::of([x, 28.0], 0.0, false, [20.0, 8.0]),
                &runs,
            )
            .unwrap();
            assert!(top.cut);
            let (used, _) = on_face(&top, radial);
            assert!(used.iter().all(|p| p[2] <= 30.0 + 1e-9));
            assert!(used.iter().any(|p| (p[2] - 30.0).abs() < 1e-9));
            assert!(used.iter().any(|p| (p[2] - 24.0).abs() < 1e-9));
        }
    }

    #[test]
    fn a_decal_goes_at_most_once_around_a_cone() {
        // A cone from its apex at the origin up +Z, 10 degrees half angle,
        // its face from z = 20 to 60: unrolled, a fan of 62.5 degrees.
        let z = [0.0, 0.0, 1.0];
        let half_angle = 10f64.to_radians();
        let cone = FaceMap::Curved(
            Development::new(
                DevelopedSurface::Cone { half_angle },
                [0.0; 3],
                z,
                false,
                20.0,
                [1.0, 0.0, 0.0],
                0.0,
                z,
            )
            .unwrap(),
        );
        let on_cone = |p: Vec3| (p[0].hypot(p[1]) - p[2] * half_angle.tan()).abs() < 1e-6;
        // 20 mm wide 10 mm up the face: about 39 degrees of the fan.
        let mesh = decal_mesh(
            &cone,
            &Placement::of([0.0, 10.0], 0.0, false, [20.0, 4.0]),
            &[],
        )
        .unwrap();
        assert!(mesh.points.iter().all(|p| on_cone(*p)));
        // 60 mm wide: about 93 degrees, more than once around.
        let error = decal_mesh(
            &cone,
            &Placement::of([0.0, 10.0], 0.0, false, [60.0, 4.0]),
            &[],
        )
        .unwrap_err();
        assert!(
            error.contains(
                "wide around the conical face at its widest row, more than once around it"
            ),
            "{error}"
        );
        // Over the apex: all the way around.
        let apex = developed_apex(
            match &cone {
                FaceMap::Curved(d) => d,
                FaceMap::Plane(_) => unreachable!(),
            },
            half_angle,
        );
        assert!((apex[1] + 20.0 / half_angle.cos()).abs() < 1e-9, "{apex:?}");
        let error =
            decal_mesh(&cone, &Placement::of(apex, 0.0, false, [4.0, 4.0]), &[]).unwrap_err();
        assert!(error.contains("apex"), "{error}");
        // Cut at the face's low circle (z = 20, 0 along the face).
        let circle = |h: f64| -> Vec<Vec3> {
            (0..=48)
                .map(|k| {
                    let a = TAU * k as f64 / 48.0;
                    let r = h * half_angle.tan();
                    [r * a.cos(), r * a.sin(), h]
                })
                .collect()
        };
        let runs = vec![circle(20.0), circle(60.0)];
        let low = decal_mesh(
            &cone,
            &Placement::of([0.0, 1.0], 0.0, false, [10.0, 6.0]),
            &runs,
        )
        .unwrap();
        assert!(low.cut);
        let (used, area) = on_face(&low, |p| [p[0], p[1], -p[2] * half_angle.tan()]);
        // (The circle's points 7.5 degrees apart: chords of it within
        // 0.002 mm.)
        assert!(used.iter().all(|p| on_cone(*p) && p[2] >= 20.0 - 0.01));
        // Its 10 x 6 mm above the circle's arc in the development (20.31 mm
        // about the apex's image, through y = 0 under its centre): 42.07
        // mm².
        assert!((area - 42.07).abs() < 0.3, "{area}");
    }
}
