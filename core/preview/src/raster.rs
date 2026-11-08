// SPDX-License-Identifier: MIT
//! A software rasterizer for preview images (mitcad#1486): the bodies'
//! display meshes shaded with their colours and a key light as the 3D view
//! lights them, their face boundaries and silhouettes drawn over them, on
//! the view's background.
//!
//! - Triangles are filled on a fixed-point grid (1/256 pixel) with exact
//!   integer edge functions and a top-left rule, so neighbouring triangles
//!   leave no cracks and no pixel twice; depth is the reciprocal of the
//!   distance (perspective) or the normalised distance (orthographic), with
//!   colours interpolated perspective-correct (Gouraud shading).
//! - Solids are culled where they face away (their triangles turn
//!   counter-clockwise seen from outside); sheets are drawn from both sides.
//! - Edges are a mesh's face boundaries (a triangle side only one triangle
//!   of its face has, compared by position, so a periodic face's seam is
//!   none) and, per view, its silhouettes (a side between a triangle facing
//!   the eye and one facing away); a mesh body also has its sharp sides.
//!   They are drawn against the faces' depth with the slack of the faces'
//!   slope over two samples, so that an edge on its own faces shows and one
//!   behind other faces does not.
//! - Pictures of at most [`SUPERSAMPLE_PIXELS`] pixels are drawn at twice
//!   the size and averaged, which smooths the edges.
//! - Transparent bodies are blended over the rest without writing depth.
//! - A section's cut and caps and highlighted edges (mitcad#1487): `cut`.
//! - An overlay's triangles (mitcad#1488) are drawn flat after the opaque
//!   bodies, a little off their surface, without changing their depth.
//!
//! One thread, a fixed order: the same input gives the same pixels.

use std::collections::{HashMap, HashSet};

use mitcad_vec3::{Vec3, add, cross, dot, norm, scale, sub, unit};

use crate::args::Background;
use crate::camera::{Camera, Projection};

// model_render's overlay drawn over the bodies (mitcad#1488).
mod overlay;

/// The largest picture (pixels) drawn with 2x2 samples a pixel.
pub const SUPERSAMPLE_PIXELS: u64 = 2_100_000;
/// The view's default background, top and bottom (sRGB).
pub const THEME_TOP: [u8; 3] = [247, 248, 250];
pub const THEME_BOTTOM: [u8; 3] = [223, 227, 234];
/// A body without an appearance (the view's `Body` colour role, sRGB).
pub const BODY_COLOR: [f64; 3] = [0.8543, 0.8756, 0.9063];
/// Edges (the view's `Edge` colour role, sRGB).
pub const EDGE_COLOR: [u8; 3] = [97, 101, 108];

/// Ambient light, the key light's diffuse and specular parts, as the
/// view's ambient light (0.35) and its directional light from the upper
/// left of the camera.
const AMBIENT: f64 = 0.35;
const DIFFUSE: f64 = 0.75;
const SPECULAR: f64 = 0.12;
const SHININESS: i32 = 32;
/// The key light's direction in the camera's frame (right, up, forward):
/// it shines right, down and away from the eye.
const LIGHT: [f64; 3] = [0.3, -0.5, 1.0];
/// A transparent body's opacity.
const TRANSPARENT_OPACITY: f64 = 0.25;
/// The sharp sides of a mesh body: facets turning by more than this.
const SHARP_DEGREES: f64 = 40.0;
/// Subpixel steps of the fill grid.
const SUB: i64 = 256;
/// The farthest a vertex is drawn from the picture's centre, pixels.
const GUARD: f64 = 1.0e6;

/// One body as placed, in the design's coordinates.
#[derive(Debug, Clone, Default)]
pub struct Mesh {
    pub vertices: Vec<Vec3>,
    /// A unit normal per vertex, out of the body.
    pub normals: Vec<Vec3>,
    /// Counter-clockwise seen from outside, face by face.
    pub triangles: Vec<[u32; 3]>,
    /// Each face's number of triangles, in order.
    pub face_triangles: Vec<u32>,
    /// Each face's colour (sRGB, 0 to 1).
    pub face_colors: Vec<[f64; 3]>,
    /// The edges' colour (sRGB).
    pub edge_color: [u8; 3],
    /// Drawn faintly over the rest.
    pub transparent: bool,
    /// A sheet: seen from both sides, nothing culled.
    pub double_sided: bool,
    /// A mesh body: facets, with its sharp sides as edges.
    pub faceted: bool,
    /// Its cut faces' colour under a section (mitcad#1487); None: its
    /// first face's.
    pub cap_color: Option<[f64; 3]>,
}

// A section and highlighted edges (mitcad#1487).
mod cut;
pub use cut::{ACCENT, ACCENT_RGB, Inspection};

/// How a triangle's samples are written.
#[derive(Clone, Copy)]
enum Paint {
    /// Shaded, with depth.
    Opaque,
    /// Shaded and blended over, without depth.
    Blend,
    /// A solid's cut face seen through a section: flat, hatched.
    Cap { base: [u8; 3], line: [u8; 3] },
}

/// What is drawn.
#[derive(Debug, Clone, Default)]
pub struct Scene {
    pub meshes: Vec<Mesh>,
    /// Drawn flat over the opaque meshes (`model_render`'s overlay,
    /// mitcad#1488).
    pub overlays: Vec<crate::overlay::OverlayMesh>,
}

/// A side between two triangles of a face: a silhouette where one faces
/// the eye and the other does not.
pub(crate) struct Fold {
    a: Vec3,
    b: Vec3,
    first: u32,
    second: u32,
}

/// A mesh's edges, found once for every picture of it.
pub(crate) struct MeshEdges {
    pub(crate) segments: Vec<[Vec3; 2]>,
    pub(crate) folds: Vec<Fold>,
    /// Each triangle's normal by its corners (not unit).
    facets: Vec<Vec3>,
}

/// A scene with its edges found, for pictures from several cameras or at
/// several sizes.
pub struct Prepared<'a> {
    scene: &'a Scene,
    edges: Vec<MeshEdges>,
}

/// Finds the meshes' edges.
pub fn prepare(scene: &Scene) -> Prepared<'_> {
    Prepared {
        scene,
        edges: scene.meshes.iter().map(mesh_edges).collect(),
    }
}

/// A position on a fine grid, which equal points share.
type Key = [i64; 3];
type SideKey = (Key, Key);

/// The grid positions are compared on, millimetres: points of one mesh
/// that the mesher made from one point (a seam's two copies, a pole's)
/// can differ in their last bits.
const KEY_GRID: f64 = 1e-6;

fn key(p: Vec3) -> Key {
    p.map(|c| (c / KEY_GRID).round() as i64)
}

fn side_key(a: Vec3, b: Vec3) -> SideKey {
    let (ka, kb) = (key(a), key(b));
    if ka <= kb { (ka, kb) } else { (kb, ka) }
}

pub(crate) fn mesh_edges(mesh: &Mesh) -> MeshEdges {
    let vertex = |i: u32| mesh.vertices.get(i as usize).copied().unwrap_or([0.0; 3]);
    let facets: Vec<Vec3> = mesh
        .triangles
        .iter()
        .map(|t| {
            let [a, b, c] = t.map(vertex);
            cross(sub(b, a), sub(c, a))
        })
        .collect();
    let mut segments = Vec::new();
    let mut folds = Vec::new();
    let mut drawn: HashSet<SideKey> = HashSet::new();
    let sharp = SHARP_DEGREES.to_radians().cos();
    // The faces' triangle ranges; what the counts leave is one more face.
    let mut ranges = Vec::with_capacity(mesh.face_triangles.len() + 1);
    let mut start = 0usize;
    for &count in &mesh.face_triangles {
        let end = (start + count as usize).min(mesh.triangles.len());
        ranges.push(start..end);
        start = end;
    }
    if start < mesh.triangles.len() {
        ranges.push(start..mesh.triangles.len());
    }
    for range in ranges {
        // The sides of the face's triangles in their order: the triangles
        // that have each (looked up by position, never iterated).
        let mut index: HashMap<SideKey, usize> = HashMap::new();
        let mut sides: Vec<(Vec3, Vec3, Vec<u32>)> = Vec::new();
        for t in range {
            let corners = mesh.triangles[t].map(vertex);
            let keys = corners.map(key);
            if keys[0] == keys[1] || keys[1] == keys[2] || keys[2] == keys[0] {
                // A triangle with two corners at one point (at a pole):
                // no area, and its sides are others' sides.
                continue;
            }
            for k in 0..3 {
                let (a, b) = (corners[k], corners[(k + 1) % 3]);
                let found = *index.entry(side_key(a, b)).or_insert_with(|| {
                    sides.push((a, b, Vec::new()));
                    sides.len() - 1
                });
                sides[found].2.push(t as u32);
            }
        }
        let seam = seam_sides(&sides);
        for (i, (a, b, triangles)) in sides.into_iter().enumerate() {
            if seam[i] {
                continue;
            }
            let boundary = match triangles.as_slice() {
                [first, second] => {
                    let (n1, n2) = (facets[*first as usize], facets[*second as usize]);
                    let turned = mesh.faceted
                        && norm(n1) > 0.0
                        && norm(n2) > 0.0
                        && dot(n1, n2) < sharp * norm(n1) * norm(n2);
                    // A sliver (a pole's triangle) has no side it faces:
                    // no silhouette.
                    let solid = |n: Vec3| {
                        let side = mitcad_vec3::distance(a, b);
                        norm(n) > 1e-9 * side * side
                    };
                    if !turned && solid(n1) && solid(n2) {
                        folds.push(Fold {
                            a,
                            b,
                            first: *first,
                            second: *second,
                        });
                    }
                    turned
                }
                _ => true,
            };
            if boundary && drawn.insert(side_key(a, b)) {
                segments.push([a, b]);
            }
        }
    }
    MeshEdges {
        segments,
        folds,
        facets,
    }
}

/// The sides of one triangle that lie along another's of the same face,
/// overlapping, from a shared end: where the mesher cut a seam into
/// pieces of different lengths on its two copies (near a sphere's pole),
/// a T-junction inside the face, not its boundary.
fn seam_sides(sides: &[(Vec3, Vec3, Vec<u32>)]) -> Vec<bool> {
    let mut seam = vec![false; sides.len()];
    let open: Vec<usize> = (0..sides.len())
        .filter(|&i| sides[i].2.len() == 1)
        .collect();
    let mut at: HashMap<Key, Vec<usize>> = HashMap::new();
    for &i in &open {
        at.entry(key(sides[i].0)).or_default().push(i);
        at.entry(key(sides[i].1)).or_default().push(i);
    }
    // `p` strictly inside the segment from `s` to `e`, on it.
    let inside = |s: Vec3, e: Vec3, p: Vec3| {
        let along = sub(e, s);
        let length = norm(along);
        if length <= 0.0 {
            return false;
        }
        let t = dot(sub(p, s), along) / (length * length);
        let off = norm(sub(sub(p, s), scale(along, t)));
        t > 1e-6 && t < 1.0 - 1e-6 && off <= 1e-6 * length + KEY_GRID
    };
    for &i in &open {
        let (a, b, _) = &sides[i];
        for (shared, far) in [(*a, *b), (*b, *a)] {
            let Some(others) = at.get(&key(shared)) else {
                continue;
            };
            for &j in others {
                if j == i {
                    continue;
                }
                let (c, d, _) = &sides[j];
                let other = if key(*c) == key(shared) { *d } else { *c };
                if inside(shared, far, other) || inside(shared, other, far) {
                    seam[i] = true;
                    seam[j] = true;
                }
            }
        }
    }
    seam
}

/// How the picture is drawn besides the camera.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Style {
    pub background: Background,
    pub edges: bool,
}

/// A picture: `channels` bytes a pixel (3: RGB, 4: RGBA with a
/// transparent background), rows from the top.
#[derive(Debug, Clone, PartialEq)]
pub struct Picture {
    pub width: u32,
    pub height: u32,
    pub channels: usize,
    pub pixels: Vec<u8>,
    /// Triangles drawn (not culled) and edge segments drawn.
    pub triangles: usize,
    pub edges: usize,
}

/// The camera's frame and projection onto the sample grid.
struct Projector {
    eye: Vec3,
    right: Vec3,
    up: Vec3,
    forward: Vec3,
    perspective: bool,
    /// Samples per unit of x / z (perspective) or per millimetre.
    focal: f64,
    cx: f64,
    cy: f64,
    near: f64,
    /// The orthographic depth range.
    z0: f64,
    z1: f64,
    light: Vec3,
    half: Vec3,
}

impl Projector {
    fn camera(&self, p: Vec3) -> Vec3 {
        let q = sub(p, self.eye);
        [dot(q, self.right), dot(q, self.up), dot(q, self.forward)]
    }

    /// Sample coordinates and depth key (larger: nearer) of a point in the
    /// camera's frame.
    fn screen(&self, c: Vec3) -> (f64, f64, f64) {
        let (x, y, key) = if self.perspective {
            (
                c[0] * self.focal / c[2],
                c[1] * self.focal / c[2],
                self.near / c[2],
            )
        } else {
            (
                c[0] * self.focal,
                c[1] * self.focal,
                (self.z1 - c[2]) / (self.z1 - self.z0),
            )
        };
        (
            self.cx + x.clamp(-GUARD, GUARD),
            self.cy - y.clamp(-GUARD, GUARD),
            key,
        )
    }

    /// Whether a triangle with this normal, at this corner, faces the eye.
    fn faces(&self, normal: Vec3, corner: Vec3) -> bool {
        if self.perspective {
            dot(normal, sub(corner, self.eye)) < 0.0
        } else {
            dot(normal, self.forward) < 0.0
        }
    }

    /// The shaded colour (sRGB, 0 to 1) of `base` with this normal.
    fn shade(&self, base: [f64; 3], normal: Vec3) -> [f64; 3] {
        let diffuse = (-dot(normal, self.light)).max(0.0);
        let specular = dot(normal, self.half).max(0.0).powi(SHININESS);
        base.map(|c| {
            let lit = srgb_to_linear(c) * (AMBIENT + DIFFUSE * diffuse) + SPECULAR * specular;
            linear_to_srgb(lit.clamp(0.0, 1.0))
        })
    }
}

/// An sRGB component (0 to 1) as linear light.
pub fn srgb_to_linear(c: f64) -> f64 {
    let c = c.clamp(0.0, 1.0);
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Linear light (0 to 1) as an sRGB component.
pub fn linear_to_srgb(c: f64) -> f64 {
    let c = c.clamp(0.0, 1.0);
    if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

fn byte(c: f64) -> u8 {
    (c.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// A vertex being drawn: its place in the camera's frame and its colour.
#[derive(Clone, Copy)]
struct Corner {
    c: Vec3,
    color: [f64; 3],
}

fn lerp_corner(a: Corner, b: Corner, t: f64) -> Corner {
    Corner {
        c: [0, 1, 2].map(|i| a.c[i] + (b.c[i] - a.c[i]) * t),
        color: [0, 1, 2].map(|i| a.color[i] + (b.color[i] - a.color[i]) * t),
    }
}

/// The sample buffers.
struct Target {
    width: usize,
    height: usize,
    color: Vec<[u8; 4]>,
    depth: Vec<f32>,
    slack: Vec<f32>,
    /// The sample shows a solid's cut face (mitcad#1487).
    cap: Vec<bool>,
    /// Samples a pixel along each side.
    samples: usize,
}

/// Draws the scene from the camera at `size` pixels.
pub fn render(prepared: &Prepared<'_>, camera: &Camera, size: [u32; 2], style: &Style) -> Picture {
    render_with(prepared, camera, size, style, &Inspection::default())
}

/// [`render`] with a section and highlighted edges (mitcad#1487).
pub fn render_with(
    prepared: &Prepared<'_>,
    camera: &Camera,
    size: [u32; 2],
    style: &Style,
    inspection: &Inspection,
) -> Picture {
    let [width, height] = size;
    let s: usize = if u64::from(width) * u64::from(height) <= SUPERSAMPLE_PIXELS {
        2
    } else {
        1
    };
    let (sw, sh) = (width as usize * s, height as usize * s);
    let forward = camera.direction();
    let up = camera.up;
    let right = cross(forward, up);
    let distance = norm(sub(camera.target, camera.eye)).max(1e-9);
    let perspective = camera.projection == Projection::Perspective;
    let focal = if perspective {
        sh as f64 / 2.0 / (camera.fov.to_radians() / 2.0).tan()
    } else {
        sh as f64 / camera.height
    };
    let light = unit(add(
        add(scale(right, LIGHT[0]), scale(up, LIGHT[1])),
        scale(forward, LIGHT[2]),
    ))
    .unwrap_or(forward);
    // Halfway between the way to the light and the way to the eye.
    let half = unit(sub(scale(light, -1.0), forward)).unwrap_or(forward);
    let mut projector = Projector {
        eye: camera.eye,
        right,
        up,
        forward,
        perspective,
        focal,
        cx: sw as f64 / 2.0,
        cy: sh as f64 / 2.0,
        near: distance * 1e-3,
        z0: 0.0,
        z1: 1.0,
        light,
        half,
    };
    if !perspective {
        let (mut z0, mut z1) = (f64::MAX, f64::MIN);
        for mesh in &prepared.scene.meshes {
            for &v in &mesh.vertices {
                let z = dot(sub(v, camera.eye), forward);
                z0 = z0.min(z);
                z1 = z1.max(z);
            }
        }
        if z0 <= z1 {
            projector.z0 = z0 - 1e-6;
            projector.z1 = z1 + 1e-6;
        }
    }
    let mut target = Target {
        width: sw,
        height: sh,
        color: vec![[0, 0, 0, 0]; sw * sh],
        depth: vec![f32::NEG_INFINITY; sw * sh],
        slack: vec![0.0; sw * sh],
        cap: vec![false; sw * sh],
        samples: s,
    };
    fill_background(&mut target, style.background, s);
    let clip = inspection
        .section
        .map(|section| cut::Clip::new(section, &projector));
    let mut triangles = 0usize;
    for pass_transparent in [false, true] {
        if pass_transparent {
            overlay::draw(
                &mut target,
                &projector,
                &prepared.scene.overlays,
                clip.as_ref(),
            );
        }
        for mesh in &prepared.scene.meshes {
            if mesh.transparent == pass_transparent {
                triangles += draw_mesh(
                    &mut target,
                    &projector,
                    mesh,
                    pass_transparent,
                    clip.as_ref(),
                );
            }
        }
        // The caps onto the plane, before the transparent bodies and the
        // edges test the depth.
        if !pass_transparent && let Some(clip) = &clip {
            cut::settle_caps(&mut target, &projector, clip);
        }
    }
    // An edge on the kept side of a section, the part of it there.
    let kept = |a: Vec3, b: Vec3| match &clip {
        Some(clip) => cut::clip_segment(&clip.world, a, b),
        None => Some((a, b)),
    };
    let mut edges = 0usize;
    if style.edges {
        for (mesh, found) in prepared.scene.meshes.iter().zip(&prepared.edges) {
            let color = mesh.edge_color;
            for [a, b] in &found.segments {
                if let Some((a, b)) = kept(*a, *b) {
                    edges += usize::from(draw_line(&mut target, &projector, a, b, color, s));
                }
            }
            if let Some(clip) = &clip
                && !mesh.transparent
            {
                for [a, b] in cut::outline(&clip.world, mesh) {
                    edges += usize::from(draw_line(&mut target, &projector, a, b, color, s));
                }
            }
            let corner = |t: u32| {
                mesh.triangles
                    .get(t as usize)
                    .and_then(|t| mesh.vertices.get(t[0] as usize))
                    .copied()
                    .unwrap_or([0.0; 3])
            };
            for fold in &found.folds {
                let first = projector.faces(found.facets[fold.first as usize], corner(fold.first));
                let second =
                    projector.faces(found.facets[fold.second as usize], corner(fold.second));
                if first != second
                    && let Some((a, b)) = kept(fold.a, fold.b)
                {
                    edges += usize::from(draw_line(&mut target, &projector, a, b, color, s));
                }
            }
        }
    }
    for line in &inspection.accent_lines {
        for pair in line.windows(2) {
            cut::draw_on_top(&mut target, &projector, pair[0], pair[1], s);
        }
    }
    let transparent = style.background == Background::Transparent;
    let channels = if transparent { 4 } else { 3 };
    let pixels = downsample(&target, width as usize, height as usize, s, channels);
    Picture {
        width,
        height,
        channels,
        pixels,
        triangles,
        edges,
    }
}

fn fill_background(target: &mut Target, background: Background, s: usize) {
    let rows = target.height / s;
    for y in 0..target.height {
        let color = match background {
            Background::Transparent => [0, 0, 0, 0],
            Background::Color([r, g, b]) => [r, g, b, 255],
            Background::Theme => {
                // Per picture row, so that the samples of a pixel agree.
                let row = (y / s) as i64;
                let span = (rows as i64 - 1).max(1);
                let mix = |top: u8, bottom: u8| -> u8 {
                    let (t, b) = (i64::from(top), i64::from(bottom));
                    (t + ((b - t) * row * 2 + span).div_euclid(2 * span)) as u8
                };
                [
                    mix(THEME_TOP[0], THEME_BOTTOM[0]),
                    mix(THEME_TOP[1], THEME_BOTTOM[1]),
                    mix(THEME_TOP[2], THEME_BOTTOM[2]),
                    255,
                ]
            }
        };
        let start = y * target.width;
        target.color[start..start + target.width].fill(color);
    }
}

/// Draws a mesh's triangles; how many were not culled.
fn draw_mesh(
    target: &mut Target,
    projector: &Projector,
    mesh: &Mesh,
    blend: bool,
    clip: Option<&cut::Clip>,
) -> usize {
    // Each face's corners' colours, lit per vertex.
    let mut face_of = vec![0usize; mesh.triangles.len()];
    let mut t = 0usize;
    for (f, &count) in mesh.face_triangles.iter().enumerate() {
        for _ in 0..count {
            if t < face_of.len() {
                face_of[t] = f;
                t += 1;
            }
        }
    }
    let last_face = mesh.face_triangles.len().saturating_sub(1);
    for slot in face_of.iter_mut().skip(t) {
        *slot = last_face;
    }
    let camera: Vec<Vec3> = mesh.vertices.iter().map(|&v| projector.camera(v)).collect();
    let color_of = |f: usize| {
        mesh.face_colors
            .get(f)
            .or(mesh.face_colors.last())
            .copied()
            .unwrap_or(BODY_COLOR)
    };
    // A solid's faces seen from behind past a section are its cut faces.
    let cap = match clip {
        Some(_) if !blend && !mesh.double_sided => Some(cut::cap_paint(
            projector,
            mesh.cap_color
                .or(mesh.face_colors.first().copied())
                .unwrap_or(BODY_COLOR),
        )),
        _ => None,
    };
    let mut drawn = 0usize;
    for (i, triangle) in mesh.triangles.iter().enumerate() {
        if triangle
            .iter()
            .any(|&v| v as usize >= mesh.vertices.len() || v as usize >= mesh.normals.len())
        {
            continue;
        }
        let [a, b, c] = triangle.map(|v| mesh.vertices[v as usize]);
        let facet = cross(sub(b, a), sub(c, a));
        let facing = projector.faces(facet, a);
        if !facing && !mesh.double_sided && cap.is_none() {
            continue;
        }
        drawn += 1;
        let base = color_of(face_of[i]);
        let corners = triangle.map(|v| {
            let n = mesh.normals[v as usize];
            // A sheet seen from behind is lit from that side.
            let n = if facing { n } else { scale(n, -1.0) };
            Corner {
                c: camera[v as usize],
                color: projector.shade(base, n),
            }
        });
        let paint = match cap {
            Some(cap) if !facing => cap,
            _ if blend => Paint::Blend,
            _ => Paint::Opaque,
        };
        match clip {
            Some(clip) => {
                for part in cut::clip_triangle(clip, corners) {
                    draw_triangle(target, projector, part, paint);
                }
            }
            None => draw_triangle(target, projector, corners, paint),
        }
    }
    drawn
}

/// Clips a triangle to the near plane (perspective) and fills it.
fn draw_triangle(target: &mut Target, projector: &Projector, corners: [Corner; 3], paint: Paint) {
    if !projector.perspective {
        fill(target, projector, corners, paint);
        return;
    }
    let near = projector.near;
    if corners.iter().all(|c| c.c[2] >= near) {
        fill(target, projector, corners, paint);
        return;
    }
    let mut kept: Vec<Corner> = Vec::with_capacity(4);
    for k in 0..3 {
        let (p, q) = (corners[k], corners[(k + 1) % 3]);
        let (pin, qin) = (p.c[2] >= near, q.c[2] >= near);
        if pin {
            kept.push(p);
        }
        if pin != qin {
            let t = (near - p.c[2]) / (q.c[2] - p.c[2]);
            kept.push(lerp_corner(p, q, t));
        }
    }
    for k in 1..kept.len().saturating_sub(1) {
        fill(target, projector, [kept[0], kept[k], kept[k + 1]], paint);
    }
}

/// Fills a triangle whose corners are in front of the eye.
fn fill(target: &mut Target, projector: &Projector, corners: [Corner; 3], paint: Paint) {
    let screen = corners.map(|c| projector.screen(c.c));
    let mut fixed = screen.map(|(x, y, _)| {
        (
            (x * SUB as f64).round() as i64,
            (y * SUB as f64).round() as i64,
        )
    });
    let mut order = [0usize, 1, 2];
    let area = |f: &[(i64, i64); 3]| {
        (f[1].0 - f[0].0) * (f[2].1 - f[0].1) - (f[2].0 - f[0].0) * (f[1].1 - f[0].1)
    };
    let mut doubled = area(&fixed);
    if doubled == 0 {
        return;
    }
    if doubled < 0 {
        order = [0, 2, 1];
        fixed = [fixed[0], fixed[2], fixed[1]];
        doubled = -doubled;
    }
    let keys = order.map(|i| screen[i].2);
    // The reciprocal distance for perspective-correct colours.
    let weights = order.map(|i| {
        if projector.perspective {
            1.0 / corners[i].c[2]
        } else {
            1.0
        }
    });
    let colors = order.map(|i| corners[i].color);
    // The depth's slope over two samples (the edges' slack).
    let (x0, y0) = (screen[order[0]].0, screen[order[0]].1);
    let (x1, y1) = (screen[order[1]].0, screen[order[1]].1);
    let (x2, y2) = (screen[order[2]].0, screen[order[2]].1);
    let denominator = (x1 - x0) * (y2 - y0) - (x2 - x0) * (y1 - y0);
    let slack = if denominator.abs() > 1e-12 {
        let dkdx =
            ((keys[1] - keys[0]) * (y2 - y0) - (keys[2] - keys[0]) * (y1 - y0)) / denominator;
        let dkdy =
            ((keys[2] - keys[0]) * (x1 - x0) - (keys[1] - keys[0]) * (x2 - x0)) / denominator;
        2.0 * (dkdx.abs() + dkdy.abs())
    } else {
        0.0
    } + 1e-5 * keys.iter().fold(0.0f64, |m, k| m.max(k.abs()));
    let slack = slack as f32;
    let min_x = fixed.iter().map(|p| p.0).min().unwrap_or(0);
    let max_x = fixed.iter().map(|p| p.0).max().unwrap_or(0);
    let min_y = fixed.iter().map(|p| p.1).min().unwrap_or(0);
    let max_y = fixed.iter().map(|p| p.1).max().unwrap_or(0);
    let half = SUB / 2;
    let first_x = ((min_x - half) + SUB - 1).div_euclid(SUB).max(0);
    let last_x = (max_x - half).div_euclid(SUB).min(target.width as i64 - 1);
    let first_y = ((min_y - half) + SUB - 1).div_euclid(SUB).max(0);
    let last_y = (max_y - half).div_euclid(SUB).min(target.height as i64 - 1);
    if first_x > last_x || first_y > last_y {
        return;
    }
    // Edge functions of the sides opposite each corner, with the top-left
    // rule: a sample on a side belongs to the triangle that owns the side.
    let sides = [(1usize, 2usize), (2, 0), (0, 1)];
    let mut start = [0i64; 3];
    let mut step_x = [0i64; 3];
    let mut step_y = [0i64; 3];
    let px0 = first_x * SUB + half;
    let py0 = first_y * SUB + half;
    for (k, &(a, b)) in sides.iter().enumerate() {
        let (ax, ay) = fixed[a];
        let (bx, by) = fixed[b];
        let (dx, dy) = (bx - ax, by - ay);
        let owned = dy > 0 || (dy == 0 && dx < 0);
        start[k] = dx * (py0 - ay) - dy * (px0 - ax) - i64::from(!owned);
        step_x[k] = -dy * SUB;
        step_y[k] = dx * SUB;
    }
    let area = doubled as f64;
    for py in first_y..=last_y {
        let row = py - first_y;
        let mut w = [0, 1, 2].map(|k| start[k] + step_y[k] * row);
        for px in first_x..=last_x {
            if w[0] >= 0 && w[1] >= 0 && w[2] >= 0 {
                let l = [0, 1, 2].map(|k| w[k] as f64 / area);
                let key = l[0] * keys[0] + l[1] * keys[1] + l[2] * keys[2];
                let index = py as usize * target.width + px as usize;
                if key as f32 > target.depth[index] {
                    let m = [0, 1, 2].map(|k| l[k] * weights[k]);
                    let total = m[0] + m[1] + m[2];
                    let color = [0, 1, 2].map(|i| {
                        (m[0] * colors[0][i] + m[1] * colors[1][i] + m[2] * colors[2][i]) / total
                    });
                    let pixel = &mut target.color[index];
                    match paint {
                        Paint::Blend => {
                            let o = TRANSPARENT_OPACITY;
                            for i in 0..3 {
                                pixel[i] =
                                    byte(color[i] * o + f64::from(pixel[i]) / 255.0 * (1.0 - o));
                            }
                            pixel[3] = byte(o + f64::from(pixel[3]) / 255.0 * (1.0 - o));
                        }
                        Paint::Opaque => {
                            *pixel = [byte(color[0]), byte(color[1]), byte(color[2]), 255];
                            target.depth[index] = key as f32;
                            target.slack[index] = slack;
                            target.cap[index] = false;
                        }
                        Paint::Cap { base, line } => {
                            let [r, g, b] =
                                if cut::hatched(px as usize, py as usize, target.samples) {
                                    line
                                } else {
                                    base
                                };
                            *pixel = [r, g, b, 255];
                            target.depth[index] = key as f32;
                            target.slack[index] = slack;
                            target.cap[index] = true;
                        }
                    }
                }
            }
            for k in 0..3 {
                w[k] += step_x[k];
            }
        }
    }
}

/// Draws a segment against the faces' depth; whether any of it is in the
/// picture.
fn draw_line(
    target: &mut Target,
    projector: &Projector,
    a: Vec3,
    b: Vec3,
    color: [u8; 3],
    s: usize,
) -> bool {
    let (mut ca, mut cb) = (projector.camera(a), projector.camera(b));
    if projector.perspective {
        let near = projector.near;
        if ca[2] < near && cb[2] < near {
            return false;
        }
        if ca[2] < near || cb[2] < near {
            let t = (near - ca[2]) / (cb[2] - ca[2]);
            let cut = [0, 1, 2].map(|i| ca[i] + (cb[i] - ca[i]) * t);
            if ca[2] < near {
                ca = cut;
            } else {
                cb = cut;
            }
        }
    }
    let (ax, ay, ak) = projector.screen(ca);
    let (bx, by, bk) = projector.screen(cb);
    let (w, h) = (target.width as f64, target.height as f64);
    let margin = s as f64 + 1.0;
    if (ax < -margin && bx < -margin)
        || (ay < -margin && by < -margin)
        || (ax > w + margin && bx > w + margin)
        || (ay > h + margin && by > h + margin)
    {
        return false;
    }
    let length = (bx - ax).abs().max((by - ay).abs());
    let steps = ((length * 2.0).ceil() as usize).clamp(1, 1 << 16);
    let reach = s as f64 / 2.0;
    let rgba = [color[0], color[1], color[2], 255];
    let mut any = false;
    for i in 0..=steps {
        let t = i as f64 / steps as f64;
        let x = ax + (bx - ax) * t;
        let y = ay + (by - ay) * t;
        let k = ak + (bk - ak) * t;
        let x0 = (x - reach).round() as i64;
        let y0 = (y - reach).round() as i64;
        for sy in y0..y0 + s as i64 {
            if sy < 0 || sy >= target.height as i64 {
                continue;
            }
            for sx in x0..x0 + s as i64 {
                if sx < 0 || sx >= target.width as i64 {
                    continue;
                }
                let index = sy as usize * target.width + sx as usize;
                if k >= f64::from(target.depth[index]) - f64::from(target.slack[index]) {
                    target.color[index] = rgba;
                    any = true;
                }
            }
        }
    }
    any
}

/// The picture from the samples: each pixel the mean of its s x s samples
/// (weighted by their cover where the background is transparent).
fn downsample(target: &Target, width: usize, height: usize, s: usize, channels: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(width * height * channels);
    let count = (s * s) as u32;
    for y in 0..height {
        for x in 0..width {
            let mut sum = [0u32; 4];
            let mut weighted = [0u32; 3];
            for dy in 0..s {
                for dx in 0..s {
                    let p = target.color[(y * s + dy) * target.width + x * s + dx];
                    for i in 0..4 {
                        sum[i] += u32::from(p[i]);
                    }
                    for i in 0..3 {
                        weighted[i] += u32::from(p[i]) * u32::from(p[3]);
                    }
                }
            }
            if channels == 4 {
                for value in weighted {
                    out.push(
                        (value + sum[3] / 2)
                            .checked_div(sum[3])
                            .map_or(0, |v| v as u8),
                    );
                }
                out.push(((sum[3] + count / 2) / count) as u8);
            } else {
                for value in sum.iter().take(3) {
                    out.push(((value + count / 2) / count) as u8);
                }
            }
        }
    }
    out
}
