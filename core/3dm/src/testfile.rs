// SPDX-License-Identifier: MIT
//! A writer of small `.3dm` archives (version 70) for the tests: the
//! chunks and objects as the format writes them, of the classes the reader
//! takes. [`sample`] is a model with a box B-rep (its faces' normals
//! partly against their planes', as files keep them), a cylinder B-rep on
//! a surface of revolution with a seam, an extruded plate with a round
//! hole, a cylinder extrusion, a mesh, a curve and a block with two
//! instances, on layers of their own, in centimetres.

use crate::archive::{
    TCODE_ANONYMOUS_CHUNK, TCODE_CLASS, TCODE_CLASS_DATA, TCODE_CLASS_UUID, TCODE_CRC,
    TCODE_ENDOFFILE, TCODE_ENDOFTABLE, TCODE_INSTANCE_DEFINITION_RECORD,
    TCODE_INSTANCE_DEFINITION_TABLE, TCODE_LAYER_RECORD, TCODE_LAYER_TABLE, TCODE_OBJECT_RECORD,
    TCODE_OBJECT_RECORD_ATTRIBUTES, TCODE_OBJECT_RECORD_END, TCODE_OBJECT_RECORD_TYPE,
    TCODE_OBJECT_TABLE, TCODE_PROPERTIES_TABLE, TCODE_PROPERTIES_WRITER_VERSION,
    TCODE_SETTINGS_TABLE, TCODE_SETTINGS_UNITSANDTOLS, TCODE_SHORT, Uuid, crc32,
};
use crate::geom::{P3, class};

/// The archive's bytes, built chunk by chunk.
#[derive(Default)]
pub struct Writer {
    pub buf: Vec<u8>,
    /// For each open chunk, the ranges of the chunks nested in it, which
    /// its CRC leaves out.
    nested: Vec<Vec<(usize, usize)>>,
}

impl Writer {
    pub fn u8(&mut self, v: u8) {
        self.buf.push(v);
    }
    pub fn bool(&mut self, v: bool) {
        self.buf.push(u8::from(v));
    }
    pub fn i32(&mut self, v: i32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    pub fn u32(&mut self, v: u32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    pub fn u16(&mut self, v: u16) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    pub fn f32(&mut self, v: f32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    pub fn f64(&mut self, v: f64) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }
    pub fn point(&mut self, p: P3) {
        for v in p {
            self.f64(v);
        }
    }
    pub fn interval(&mut self, a: f64, b: f64) {
        self.f64(a);
        self.f64(b);
    }
    pub fn uuid(&mut self, u: Uuid) {
        self.buf.extend_from_slice(&u.0);
    }
    pub fn wstring(&mut self, s: &str) {
        if s.is_empty() {
            self.u32(0);
            return;
        }
        let units: Vec<u16> = s.encode_utf16().collect();
        self.u32(units.len() as u32 + 1);
        for u in units {
            self.u16(u);
        }
        self.u16(0);
    }
    pub fn chunk_version(&mut self, major: u8, minor: u8) {
        self.u8(major << 4 | minor);
    }
    pub fn i32_array(&mut self, a: &[i32]) {
        self.i32(a.len() as i32);
        for &v in a {
            self.i32(v);
        }
    }
    pub fn f64_array(&mut self, a: &[f64]) {
        self.i32(a.len() as i32);
        for &v in a {
            self.f64(v);
        }
    }
    pub fn bbox(&mut self) {
        self.point([0.0; 3]);
        self.point([0.0; 3]);
    }
    pub fn plane(&mut self, origin: P3, x: P3, y: P3) {
        let z = mitcad_vec3::cross(x, y);
        self.point(origin);
        self.point(x);
        self.point(y);
        self.point(z);
        let d = -mitcad_vec3::dot(z, origin);
        for v in [z[0], z[1], z[2], d] {
            self.f64(v);
        }
    }

    /// A short chunk.
    pub fn short(&mut self, typecode: u32, value: i64) {
        let at = self.buf.len();
        self.u32(typecode | TCODE_SHORT);
        self.buf.extend_from_slice(&value.to_le_bytes());
        if let Some(open) = self.nested.last_mut() {
            open.push((at, self.buf.len()));
        }
    }

    /// A chunk with its data and, when the typecode asks for one, the CRC
    /// of its own bytes (without the chunks nested in it, as the format
    /// computes it).
    pub fn chunk(&mut self, typecode: u32, body: impl FnOnce(&mut Writer)) {
        let header = self.buf.len();
        self.u32(typecode);
        let at = self.buf.len();
        self.buf.extend_from_slice(&[0; 8]);
        let start = self.buf.len();
        self.nested.push(Vec::new());
        body(self);
        let inner = self.nested.pop().unwrap_or_default();
        if typecode & TCODE_CRC != 0 {
            let mut crc = 0;
            let mut from = start;
            for (a, b) in inner {
                crc = crc32(crc, &self.buf[from..a]);
                from = b;
            }
            crc = crc32(crc, &self.buf[from..]);
            self.u32(crc);
        }
        let len = (self.buf.len() - start) as i64;
        self.buf[at..at + 8].copy_from_slice(&len.to_le_bytes());
        if let Some(open) = self.nested.last_mut() {
            open.push((header, self.buf.len()));
        }
    }

    /// An anonymous chunk with a version byte.
    pub fn anonymous(&mut self, major: u8, minor: u8, body: impl FnOnce(&mut Writer)) {
        self.chunk(TCODE_ANONYMOUS_CHUNK, |w| {
            w.chunk_version(major, minor);
            body(w);
        });
    }

    /// An anonymous chunk with a version of two ints.
    pub fn versioned(&mut self, major: i32, minor: i32, body: impl FnOnce(&mut Writer)) {
        self.chunk(TCODE_ANONYMOUS_CHUNK, |w| {
            w.i32(major);
            w.i32(minor);
            body(w);
        });
    }

    /// An object: its class's UUID and its data.
    pub fn object(&mut self, uuid: Uuid, data: impl FnOnce(&mut Writer)) {
        self.chunk(TCODE_CLASS, |w| {
            w.chunk(TCODE_CLASS_UUID, |w| w.uuid(uuid));
            w.chunk(TCODE_CLASS_DATA, data);
            w.short(0x0002_7FFF, 0);
        });
    }
}

/// A curve to write.
#[derive(Clone, Debug)]
pub enum C {
    Line(P3, P3, [f64; 2]),
    /// Centre, x and y axes, radius, angles, domain.
    Arc(P3, P3, P3, f64, [f64; 2], [f64; 2]),
    /// Order, the format's knots, control points (non-rational).
    Nurbs(usize, Vec<f64>, Vec<P3>),
    /// Segments and their parameters.
    Poly(Vec<C>, Vec<f64>),
}

pub fn write_curve(w: &mut Writer, c: &C) {
    match c {
        C::Line(a, b, d) => w.object(class::LINE_CURVE, |w| {
            w.chunk_version(1, 0);
            w.point(*a);
            w.point(*b);
            w.interval(d[0], d[1]);
            w.i32(3);
        }),
        C::Arc(o, x, y, r, angles, d) => w.object(class::ARC_CURVE, |w| {
            w.chunk_version(1, 0);
            w.plane(*o, *x, *y);
            w.f64(*r);
            for _ in 0..3 {
                w.point([0.0; 3]);
            }
            w.interval(angles[0], angles[1]);
            w.interval(d[0], d[1]);
            w.i32(3);
        }),
        C::Nurbs(order, knots, cvs) => w.object(class::NURBS_CURVE, |w| {
            w.chunk_version(1, 0);
            w.i32(3);
            w.i32(0);
            w.i32(*order as i32);
            w.i32(cvs.len() as i32);
            w.i32(0);
            w.i32(0);
            w.bbox();
            w.f64_array(knots);
            w.i32(cvs.len() as i32);
            for p in cvs {
                w.point(*p);
            }
        }),
        C::Poly(segments, params) => w.object(class::POLY_CURVE, |w| {
            w.chunk_version(1, 0);
            w.i32(segments.len() as i32);
            w.i32(0);
            w.i32(0);
            w.bbox();
            w.f64_array(params);
            for s in segments {
                write_curve(w, s);
            }
        }),
    }
}

/// A surface to write.
#[derive(Clone, Debug)]
pub enum S {
    /// Origin, x and y axes, domains and extents.
    Plane(P3, P3, P3, [[f64; 2]; 2], [[f64; 2]; 2]),
    /// Axis, angles, transposed, curve.
    Rev([P3; 2], [f64; 2], bool, C),
}

fn write_surface(w: &mut Writer, s: &S) {
    match s {
        S::Plane(o, x, y, domain, extents) => w.object(class::PLANE_SURFACE, |w| {
            w.chunk_version(1, 1);
            w.plane(*o, *x, *y);
            w.interval(domain[0][0], domain[0][1]);
            w.interval(domain[1][0], domain[1][1]);
            w.interval(extents[0][0], extents[0][1]);
            w.interval(extents[1][0], extents[1][1]);
        }),
        S::Rev(axis, angles, transposed, curve) => w.object(class::REV_SURFACE, |w| {
            w.chunk_version(2, 0);
            w.point(axis[0]);
            w.point(axis[1]);
            w.interval(angles[0], angles[1]);
            w.interval(angles[0], angles[1]);
            w.bbox();
            w.i32(i32::from(*transposed));
            w.u8(1);
            write_curve(w, curve);
        }),
    }
}

/// A B-rep to write: as the format keeps it.
#[derive(Clone, Debug, Default)]
pub struct B {
    pub curves: Vec<C>,
    pub surfaces: Vec<S>,
    pub vertices: Vec<P3>,
    /// Curve, reversed, proxy domain, vertices.
    pub edges: Vec<(i32, bool, [f64; 2], [i32; 2])>,
    /// Edge, vertices, reversed against the edge, type, loop.
    pub trims: Vec<(i32, [i32; 2], bool, i32, i32)>,
    /// Trims, type, face.
    pub loops: Vec<(Vec<i32>, i32, i32)>,
    /// Loops, surface, reversed.
    pub faces: Vec<(Vec<i32>, i32, bool)>,
    pub is_solid: i32,
}

fn array_of<T>(w: &mut Writer, items: &[T], write: impl Fn(&mut Writer, &T)) {
    w.anonymous(1, 0, |w| {
        w.i32(items.len() as i32);
        for item in items {
            w.i32(1);
            write(w, item);
        }
    });
}

pub fn write_brep(w: &mut Writer, b: &B) {
    w.object(class::BREP, |w| {
        w.chunk_version(3, 2);
        array_of::<C>(w, &[], write_curve);
        array_of(w, &b.curves, write_curve);
        array_of(w, &b.surfaces, write_surface);
        w.anonymous(1, 0, |w| {
            w.i32(b.vertices.len() as i32);
            for (i, p) in b.vertices.iter().enumerate() {
                w.i32(i as i32);
                w.point(*p);
                w.i32_array(&[]);
                w.f64(0.0);
            }
        });
        w.anonymous(1, 0, |w| {
            w.i32(b.edges.len() as i32);
            for (i, (c, rev, proxy, v)) in b.edges.iter().enumerate() {
                w.i32(i as i32);
                w.i32(*c);
                w.i32(i32::from(*rev));
                w.interval(proxy[0], proxy[1]);
                w.i32(v[0]);
                w.i32(v[1]);
                w.i32_array(&[]);
                w.f64(0.001);
                w.interval(proxy[0], proxy[1]);
            }
        });
        w.anonymous(1, 0, |w| {
            w.i32(b.trims.len() as i32);
            for (i, (e, v, rev, kind, lp)) in b.trims.iter().enumerate() {
                w.i32(i as i32);
                w.i32(-1);
                w.interval(0.0, 1.0);
                w.i32(*e);
                w.i32(v[0]);
                w.i32(v[1]);
                w.i32(i32::from(*rev));
                w.i32(*kind);
                w.i32(0);
                w.i32(*lp);
                w.f64(0.0);
                w.f64(0.0);
                w.interval(0.0, 1.0);
                w.buf.extend_from_slice(&[0; 32]);
                w.f64(0.0);
                w.f64(0.0);
            }
        });
        w.anonymous(1, 0, |w| {
            w.i32(b.loops.len() as i32);
            for (i, (trims, kind, face)) in b.loops.iter().enumerate() {
                w.i32(i as i32);
                w.i32_array(trims);
                w.i32(*kind);
                w.i32(*face);
            }
        });
        w.anonymous(1, 0, |w| {
            w.i32(b.faces.len() as i32);
            for (i, (loops, surface, rev)) in b.faces.iter().enumerate() {
                w.i32(i as i32);
                w.i32_array(loops);
                w.i32(*surface);
                w.i32(i32::from(*rev));
                w.i32(0);
            }
        });
        w.bbox();
        // No render or analysis meshes.
        w.chunk(TCODE_ANONYMOUS_CHUNK, |w| {
            for _ in &b.faces {
                w.u8(0);
            }
        });
        w.chunk(TCODE_ANONYMOUS_CHUNK, |w| {
            for _ in &b.faces {
                w.u8(0);
            }
        });
        w.i32(b.is_solid);
    });
}

/// A box from `lo` to `hi` as files keep it: one plane per side, its
/// edges lines, each face's trims counterclockwise about its plane's
/// normal; the planes of the bottom, front and left sides face in, those
/// faces reversed.
pub fn box_brep(lo: P3, hi: P3) -> B {
    let v = |i: usize| {
        [
            if i & 1 != 0 { hi[0] } else { lo[0] },
            if i & 2 != 0 { hi[1] } else { lo[1] },
            if i & 4 != 0 { hi[2] } else { lo[2] },
        ]
    };
    let vertices: Vec<P3> = (0..8).map(v).collect();
    // The sides: the corners counterclockwise about the outward normal,
    // and the plane's normal (in or out).
    let sides: [([usize; 4], bool); 6] = [
        ([0, 2, 3, 1], true),  // bottom (z = lo), plane facing up: in
        ([4, 5, 7, 6], false), // top
        ([0, 1, 5, 4], true),  // front (y = lo)
        ([2, 6, 7, 3], false), // back
        ([0, 4, 6, 2], true),  // left (x = lo)
        ([1, 3, 7, 5], false), // right
    ];
    let mut b = B {
        vertices,
        is_solid: 1,
        ..Default::default()
    };
    let mut edge_of = std::collections::HashMap::new();
    for (fi, (corners, inward)) in sides.iter().enumerate() {
        // The loop about the plane's normal: reversed when the plane faces
        // in.
        let mut loop_corners = corners.to_vec();
        if *inward {
            loop_corners.reverse();
        }
        let (p0, p1, p3) = (v(loop_corners[0]), v(loop_corners[1]), v(loop_corners[3]));
        let x = mitcad_vec3::unit(mitcad_vec3::sub(p1, p0)).expect("a side");
        let y = mitcad_vec3::unit(mitcad_vec3::sub(p3, p0)).expect("a side");
        let ex = mitcad_vec3::distance(p1, p0);
        let ey = mitcad_vec3::distance(p3, p0);
        b.surfaces.push(S::Plane(
            p0,
            x,
            y,
            [[0.0, ex], [0.0, ey]],
            [[0.0, ex], [0.0, ey]],
        ));
        let li = b.loops.len() as i32;
        let mut trims = Vec::new();
        for k in 0..4 {
            let (a, c) = (loop_corners[k], loop_corners[(k + 1) % 4]);
            let key = (a.min(c), a.max(c));
            let ei = *edge_of.entry(key).or_insert_with(|| {
                b.curves.push(C::Line(v(key.0), v(key.1), [0.0, 1.0]));
                b.edges.push((
                    (b.curves.len() - 1) as i32,
                    false,
                    [0.0, 1.0],
                    [key.0 as i32, key.1 as i32],
                ));
                (b.edges.len() - 1) as i32
            });
            b.trims.push((ei, [a as i32, c as i32], a != key.0, 2, li));
            trims.push((b.trims.len() - 1) as i32);
        }
        b.loops.push((trims, 1, fi as i32));
        b.faces.push((vec![li], fi as i32, *inward));
    }
    b
}

/// A square plane surface trimmed by its four edges, 2 by 2 from `at` in
/// the XY plane, drawn facing up (+z), in one of three forms a file may
/// keep it in (mitcad#1738): 0 its plane's frame facing up; 1 its frame
/// facing down (x and y swapped) and the face reversed; 2 its frame facing
/// up with the u extents running against the domain (the surface's normal
/// down) and the face reversed. Its trims run counterclockwise in the
/// surface's parameters, so about the surface's normal, as files keep
/// them.
pub fn plane_brep(form: usize, at: P3) -> B {
    let corner = |dx: f64, dy: f64| [at[0] + dx, at[1] + dy, at[2]];
    let corners = [
        corner(0.0, 0.0),
        corner(2.0, 0.0),
        corner(2.0, 2.0),
        corner(0.0, 2.0),
    ];
    let both = [[0.0, 2.0], [0.0, 2.0]];
    let (surface, reversed) = match form {
        0 => (
            S::Plane(at, [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], both, both),
            false,
        ),
        1 => (
            S::Plane(at, [0.0, 1.0, 0.0], [1.0, 0.0, 0.0], both, both),
            true,
        ),
        _ => (
            S::Plane(
                corner(2.0, 0.0),
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                both,
                [[0.0, -2.0], [0.0, 2.0]],
            ),
            true,
        ),
    };
    // The surface's normal is up in form 0, down in the others: the loop
    // runs counterclockwise about it.
    let order: [usize; 4] = if form == 0 {
        [0, 1, 2, 3]
    } else {
        [0, 3, 2, 1]
    };
    let mut b = B {
        vertices: corners.to_vec(),
        surfaces: vec![surface],
        ..Default::default()
    };
    for k in 0..4 {
        b.curves
            .push(C::Line(corners[k], corners[(k + 1) % 4], [0.0, 2.0]));
        b.edges.push((
            k as i32,
            false,
            [0.0, 2.0],
            [k as i32, ((k + 1) % 4) as i32],
        ));
    }
    let mut trims = Vec::new();
    for k in 0..4 {
        let (a, c) = (order[k], order[(k + 1) % 4]);
        // The edge between the two corners, along it or against it.
        let (edge, against) = if (a + 1) % 4 == c {
            (a, false)
        } else {
            (c, true)
        };
        b.trims
            .push((edge as i32, [a as i32, c as i32], against, 1, 0));
        trims.push(k as i32);
    }
    b.loops.push((trims, 1, 0));
    b.faces.push((vec![0], 0, reversed));
    b
}

/// A cylinder of `radius` from z 0 to `height` on a surface of revolution
/// (a line turned about z) with a seam, its caps planes; its 3D curves are
/// arcs and lines; the edge of the top circle is reversed against its
/// curve.
pub fn cylinder_brep(radius: f64, height: f64) -> B {
    let tau = std::f64::consts::TAU;
    let mut b = B {
        vertices: vec![[radius, 0.0, 0.0], [radius, 0.0, height]],
        is_solid: 1,
        ..Default::default()
    };
    // The side: the line (r, 0, 0)-(r, 0, h) turned about z; u the angle,
    // v up: its normal faces out.
    b.surfaces.push(S::Rev(
        [[0.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
        [0.0, tau],
        false,
        C::Line([radius, 0.0, 0.0], [radius, 0.0, height], [0.0, height]),
    ));
    // The caps: the bottom's plane faces up (the face is reversed), the
    // top's up.
    let ext = [[-radius, radius], [-radius, radius]];
    b.surfaces.push(S::Plane(
        [0.0; 3],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        ext,
        ext,
    ));
    b.surfaces.push(S::Plane(
        [0.0, 0.0, height],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        ext,
        ext,
    ));
    // Curves: the bottom circle, the top circle (counterclockwise about
    // z), the seam line.
    let circle = |z: f64| {
        C::Arc(
            [0.0, 0.0, z],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            radius,
            [0.0, tau],
            [0.0, tau],
        )
    };
    b.curves.push(circle(0.0));
    b.curves.push(circle(height));
    b.curves.push(C::Line(
        [radius, 0.0, 0.0],
        [radius, 0.0, height],
        [0.0, height],
    ));
    // Edges: bottom circle, top circle reversed (clockwise about z), seam.
    b.edges.push((0, false, [0.0, tau], [0, 0]));
    b.edges.push((1, true, [0.0, tau], [1, 1]));
    b.edges.push((2, false, [0.0, height], [0, 1]));
    // The side's loop, counterclockwise in (angle, height): bottom
    // circle, seam at 2 pi up, top circle back, seam at 0 down.
    b.trims.push((0, [0, 0], false, 2, 0));
    b.trims.push((2, [0, 1], false, 3, 0));
    b.trims.push((1, [1, 1], false, 2, 0)); // the top edge runs clockwise: along the trim
    b.trims.push((2, [1, 0], true, 3, 0));
    b.loops.push((vec![0, 1, 2, 3], 1, 0));
    // The bottom cap, its plane facing up: counterclockwise about z.
    b.trims.push((0, [0, 0], false, 2, 1));
    b.loops.push((vec![4], 1, 1));
    // The top cap: counterclockwise about z, against its clockwise edge.
    b.trims.push((1, [1, 1], true, 2, 2));
    b.loops.push((vec![5], 1, 2));
    b.faces.push((vec![0], 0, false));
    b.faces.push((vec![1], 1, true));
    b.faces.push((vec![2], 2, false));
    b
}

pub fn write_extrusion(
    w: &mut Writer,
    profile: &C,
    profiles: i32,
    path: [P3; 2],
    up: P3,
    cap: bool,
) {
    w.object(class::EXTRUSION, |w| {
        w.versioned(1, 2, |w| {
            write_curve(w, profile);
            w.point(path[0]);
            w.point(path[1]);
            w.interval(0.0, 1.0);
            w.point(up);
            w.bool(false);
            w.bool(false);
            w.point([0.0, 0.0, 1.0]);
            w.point([0.0, 0.0, 1.0]);
            w.interval(0.0, 1.0);
            w.bool(false);
            w.i32(profiles);
            w.bool(cap);
            w.bool(cap);
        });
    });
}

/// A mesh (version 3, its vertices compressed).
pub fn write_mesh(w: &mut Writer, vertices: &[P3], faces: &[[u32; 4]]) {
    w.object(class::MESH, |w| {
        w.chunk_version(3, 0);
        w.i32(vertices.len() as i32);
        w.i32(faces.len() as i32);
        for _ in 0..4 {
            w.interval(0.0, 1.0);
        }
        w.f64(1.0);
        w.f64(1.0);
        for _ in 0..16 {
            w.f32(0.0);
        }
        w.i32(1);
        w.u8(0);
        for _ in 0..4 {
            w.u8(0);
        }
        w.i32(4);
        for f in faces {
            for &i in f {
                w.u32(i);
            }
        }
        let mut raw = Vec::new();
        for p in vertices {
            for &c in p {
                raw.extend_from_slice(&(c as f32).to_le_bytes());
            }
        }
        w.u32(raw.len() as u32);
        w.u32(crc32(0, &raw));
        w.u8(1);
        let deflated = miniz_oxide::deflate::compress_to_vec_zlib(&raw, 6);
        w.chunk(TCODE_ANONYMOUS_CHUNK, |w| {
            w.buf.extend_from_slice(&deflated)
        });
        for _ in 0..4 {
            w.u32(0); // no normals, textures, curvatures, colours
        }
    });
}

/// An object record: its geometry and its attributes (layer, name, colour
/// from the object or its layer, mode).
pub fn record(
    w: &mut Writer,
    layer: i32,
    name: &str,
    color: Option<u32>,
    mode: u8,
    geometry: impl FnOnce(&mut Writer),
) {
    w.chunk(TCODE_OBJECT_RECORD, |w| {
        w.short(TCODE_OBJECT_RECORD_TYPE & !TCODE_SHORT, 0);
        geometry(w);
        w.chunk(TCODE_OBJECT_RECORD_ATTRIBUTES, |w| {
            w.chunk_version(2, 0);
            w.uuid(record_id(name));
            w.i32(layer);
            if !name.is_empty() {
                w.u8(1);
                w.wstring(name);
            }
            if let Some(c) = color {
                w.u8(6);
                w.u32(c);
            }
            if mode != 0 {
                w.u8(12);
                w.u8(mode);
            }
            if color.is_some() {
                w.u8(13);
                w.u8(1);
            }
            w.u8(0);
        });
        w.short(TCODE_OBJECT_RECORD_END & !TCODE_SHORT, 0);
    });
}

/// The id [`record`] gives an object of that name.
pub fn record_id(name: &str) -> Uuid {
    let mut b = [0u8; 16];
    b[..name.len().min(16)].copy_from_slice(&name.as_bytes()[..name.len().min(16)]);
    b[15] = 1;
    Uuid(b)
}

fn layer(w: &mut Writer, index: i32, name: &str, color: u32) {
    w.chunk(TCODE_LAYER_RECORD, |w| {
        w.object(class::LAYER, |w| {
            w.chunk_version(1, 6);
            w.i32(0);
            w.i32(index);
            w.i32(0);
            w.i32(-1);
            w.i32(0);
            w.u32(color);
            w.buf.extend_from_slice(&[0; 20]);
            w.wstring(name);
            w.bool(true);
            w.i32(-1);
            w.u32(0);
            w.f64(0.0);
            w.bool(false);
            let mut id = [0u8; 16];
            id[0] = 0x10 + index as u8;
            w.uuid(Uuid(id));
            w.uuid(Uuid::NIL);
            w.bool(true);
        });
    });
}

fn definition(w: &mut Writer, id: Uuid, name: &str, objects: &[Uuid]) {
    definition_in(w, id, name, objects, false);
}

/// A block definition; with `flagged`, its id and name as files of format
/// version 8 keep them: a chunk of their own (`TCODE_ANONYMOUS_CHUNK` +
/// 2), five fields each after a byte that says whether it is there.
fn definition_in(w: &mut Writer, id: Uuid, name: &str, objects: &[Uuid], flagged: bool) {
    w.chunk(TCODE_INSTANCE_DEFINITION_RECORD, |w| {
        w.object(class::INSTANCE_DEFINITION, |w| {
            w.versioned(1, 0, |w| {
                if flagged {
                    w.chunk(TCODE_ANONYMOUS_CHUNK + 2, |w| {
                        w.i32(1);
                        w.i32(0);
                        w.bool(false);
                        w.bool(true);
                        w.uuid(id);
                        w.bool(false);
                        w.bool(true);
                        w.i32(0);
                        w.bool(true);
                        w.wstring(name);
                    });
                } else {
                    w.versioned(1, 0, |w| {
                        w.u32(1 | 8);
                        w.uuid(id);
                        w.wstring(name);
                    });
                }
                w.i32(1);
                w.versioned(1, 0, |w| {
                    w.u32(2);
                    w.f64(0.001);
                    w.wstring("");
                });
                w.wstring("");
                w.wstring("");
                w.wstring("");
                w.bbox();
                w.bool(true);
                w.i32(objects.len() as i32);
                for o in objects {
                    w.uuid(*o);
                }
                w.bool(false);
            });
        });
    });
}

fn instance(w: &mut Writer, def: Uuid, at: P3) {
    w.object(class::INSTANCE_REF, |w| {
        w.chunk_version(1, 0);
        w.uuid(def);
        let m = [
            [1.0, 0.0, 0.0, at[0]],
            [0.0, 1.0, 0.0, at[1]],
            [0.0, 0.0, 1.0, at[2]],
            [0.0, 0.0, 0.0, 1.0],
        ];
        for row in m {
            for v in row {
                w.f64(v);
            }
        }
        w.bbox();
    });
}

/// The block definition's id in [`sample`].
pub const BOLT: Uuid = Uuid::parse("B0B0B0B0-0000-4000-8000-000000000001");

/// The id of the block in [`block_in_itself`].
pub const LOOP: Uuid = Uuid::parse("B0B0B0B0-0000-4000-8000-000000000002");

/// A model of the test writer by name: `""` the [`sample`],
/// `block-in-itself` [`block_in_itself`], `plane-surfaces`
/// [`plane_surfaces`]; None for another name.
pub fn test_case(name: &str) -> Option<Vec<u8>> {
    match name {
        "" => Some(sample()),
        "block-in-itself" => Some(block_in_itself()),
        "plane-surfaces" => Some(plane_surfaces()),
        _ => None,
    }
}

/// A model in centimetres of three plane surfaces 2 by 2 at z 0, side by
/// side 3 apart along x, each drawn facing up in one of the forms of
/// [`plane_brep`] ("sheet 0" to "sheet 2", on layer Solids), and nothing
/// else: the lowest bodies lie in the ground's plane of a render
/// (mitcad#1738).
pub fn plane_surfaces() -> Vec<u8> {
    model(
        |_| {},
        |w| {
            for form in 0..3 {
                let name = format!("sheet {form}");
                record(w, 1, &name, None, 0, |w| {
                    write_brep(w, &plane_brep(form, [3.0 * form as f64, 0.0, 0.0]))
                });
            }
        },
    )
}

/// A model in centimetres whose block "Loop" holds a box and an instance
/// of itself, placed once: the import makes the block a component with
/// its box and leaves the instance inside it out, instead of placing the
/// block without end.
pub fn block_in_itself() -> Vec<u8> {
    model(
        |w| {
            definition(
                w,
                LOOP,
                "Loop",
                &[record_id("loop box"), record_id("loop self")],
            )
        },
        |w| {
            record(w, 1, "loop box", None, 3, |w| {
                write_brep(w, &box_brep([0.0; 3], [1.0, 1.0, 1.0]))
            });
            record(w, 1, "loop self", None, 3, |w| {
                instance(w, LOOP, [2.0, 0.0, 0.0])
            });
            record(w, 1, "loop 1", None, 0, |w| {
                instance(w, LOOP, [0.0, 0.0, 0.0])
            });
        },
    )
}

/// The sample model with its block definition in the form of format
/// version 8 (its name and id in a flagged chunk).
pub fn sample_v8_block() -> Vec<u8> {
    model(
        |w| definition_in(w, BOLT, "Bolt", &[record_id("bolt head")], true),
        sample_objects,
    )
}

/// The sample model (see the module's documentation), in centimetres.
pub fn sample() -> Vec<u8> {
    model(
        |w| definition(w, BOLT, "Bolt", &[record_id("bolt head")]),
        sample_objects,
    )
}

/// A model in centimetres with the sample's settings and layers, its
/// block definitions and its objects as given.
fn model(definitions: impl FnOnce(&mut Writer), objects: impl FnOnce(&mut Writer)) -> Vec<u8> {
    let mut w = Writer::default();
    w.buf.extend_from_slice(b"3D Geometry File Format       70");
    w.chunk(0x0000_0001, |w| {
        w.buf.extend_from_slice(b"MitCad test file\n\0")
    });
    w.chunk(TCODE_PROPERTIES_TABLE, |w| {
        w.short(
            TCODE_PROPERTIES_WRITER_VERSION & !TCODE_SHORT,
            2_382_395_020,
        );
        w.short(TCODE_ENDOFTABLE & !TCODE_SHORT, 0);
    });
    w.chunk(TCODE_SETTINGS_TABLE, |w| {
        w.chunk(TCODE_SETTINGS_UNITSANDTOLS, |w| {
            w.i32(102);
            w.u32(3); // centimetres
            w.f64(0.001);
            w.f64(0.017);
            w.f64(0.01);
            w.i32(0);
            w.i32(3);
            w.f64(0.01);
            w.wstring("");
        });
        w.short(TCODE_ENDOFTABLE & !TCODE_SHORT, 0);
    });
    w.chunk(TCODE_LAYER_TABLE, |w| {
        layer(w, 0, "Default", 0x0000_0000);
        layer(w, 1, "Solids", 0x0000_00FF);
        layer(w, 2, "Meshes", 0x0000_FF00);
        w.short(TCODE_ENDOFTABLE & !TCODE_SHORT, 0);
    });
    w.chunk(TCODE_INSTANCE_DEFINITION_TABLE, |w| {
        definitions(w);
        w.short(TCODE_ENDOFTABLE & !TCODE_SHORT, 0);
    });
    w.chunk(TCODE_OBJECT_TABLE, |w| {
        objects(w);
        w.short(TCODE_ENDOFTABLE & !TCODE_SHORT, 0);
    });
    let size = w.buf.len() as u64 + 4 + 8 + 8;
    w.chunk(TCODE_ENDOFFILE, |w| {
        w.buf.extend_from_slice(&size.to_le_bytes())
    });
    w.buf
}

/// The sample's objects.
fn sample_objects(w: &mut Writer) {
    record(w, 1, "box", None, 0, |w| {
        write_brep(w, &box_brep([0.0; 3], [2.0, 3.0, 4.0]))
    });
    record(w, 1, "cylinder", Some(0x0000_FFFF), 0, |w| {
        write_brep(w, &cylinder_brep(1.0, 2.0))
    });
    // A plate 4 by 3 by 0.5 with a round hole of radius 0.5: the outer
    // profile a polycurve of lines, the hole a circle.
    let rect = C::Poly(
        vec![
            C::Line([0.0, 0.0, 0.0], [4.0, 0.0, 0.0], [0.0, 4.0]),
            C::Line([4.0, 0.0, 0.0], [4.0, 3.0, 0.0], [0.0, 3.0]),
            C::Line([4.0, 3.0, 0.0], [0.0, 3.0, 0.0], [0.0, 4.0]),
            C::Line([0.0, 3.0, 0.0], [0.0, 0.0, 0.0], [0.0, 3.0]),
        ],
        vec![0.0, 4.0, 7.0, 11.0, 14.0],
    );
    let tau = std::f64::consts::TAU;
    let hole = C::Arc(
        [2.0, 1.5, 0.0],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        0.5,
        [0.0, tau],
        [0.0, tau],
    );
    let both = C::Poly(vec![rect, hole], vec![0.0, 14.0, 14.0 + tau]);
    record(w, 1, "plate", None, 0, |w| {
        write_extrusion(
            w,
            &both,
            2,
            [[10.0, 0.0, 0.0], [10.0, 0.0, 0.5]],
            [0.0, 1.0, 0.0],
            true,
        )
    });
    let circle = C::Arc(
        [0.0; 3],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        0.5,
        [0.0, tau],
        [0.0, tau],
    );
    record(w, 1, "rod", None, 0, |w| {
        write_extrusion(
            w,
            &circle,
            1,
            [[-5.0, 0.0, 0.0], [-5.0, 0.0, 3.0]],
            [0.0, 1.0, 0.0],
            true,
        )
    });
    let tet = [
        [0.0, 0.0, 10.0],
        [1.0, 0.0, 10.0],
        [0.0, 1.0, 10.0],
        [0.0, 0.0, 11.0],
    ];
    record(w, 2, "tetrahedron", None, 0, |w| {
        write_mesh(
            w,
            &tet,
            &[[0, 2, 1, 1], [0, 1, 3, 3], [1, 2, 3, 3], [2, 0, 3, 3]],
        )
    });
    record(w, 0, "spline", None, 0, |w| {
        write_curve(
            w,
            &C::Nurbs(
                4,
                vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
                vec![[0.0; 3], [1.0, 1.0, 0.0], [2.0, 1.0, 0.0], [3.0, 0.0, 0.0]],
            ),
        )
    });
    // The block's geometry (mode 3) and two instances of it.
    record(w, 1, "bolt head", None, 3, |w| {
        write_brep(w, &box_brep([0.0; 3], [1.0, 1.0, 1.0]))
    });
    record(w, 1, "bolt 1", None, 0, |w| {
        instance(w, BOLT, [0.0, 20.0, 0.0])
    });
    record(w, 1, "bolt 2", None, 0, |w| {
        instance(w, BOLT, [5.0, 20.0, 0.0])
    });
}
