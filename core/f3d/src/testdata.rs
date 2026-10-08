// SPDX-License-Identifier: MIT
//! Small ASM bodies written by our own writer, for tests here and in the
//! C++ bridge tests (no .f3d files are needed). Model units are
//! centimetres, as in .f3d files.

use crate::asm::writer::Writer;
use crate::brep::{self, P3};

/// A 1 cm cube at the origin: 8 vertices, 12 edges, 6 planar faces.
pub fn cube_blob() -> Vec<u8> {
    cube(false)
}

/// The cube of [`cube_blob`] with the names an extrusion gives (see
/// [`crate::names`]): the body `"301"`, the faces `"1"` (bottom), `"2"`
/// (top), `"3"` (y = 0), `"4"` (x = 1), `"5"` (y = 1) and `"6"` (x = 0), all
/// made by operation 301.
pub fn named_cube_blob() -> Vec<u8> {
    cube(true)
}

fn cube(named: bool) -> Vec<u8> {
    // Record layout: 0 asmheader, 1 body, 2 lump, 3 shell,
    // faces 4..10, loops 10..16, coedges 16..40, edges 40..52,
    // vertices 52..60, points 60..68, curves 68..80, surfaces 80..86.
    let corners = [
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [1.0, 1.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
        [1.0, 0.0, 1.0],
        [1.0, 1.0, 1.0],
        [0.0, 1.0, 1.0],
    ];
    let edges: [(usize, usize); 12] = [
        (0, 1),
        (1, 2),
        (2, 3),
        (3, 0),
        (4, 5),
        (5, 6),
        (6, 7),
        (7, 4),
        (0, 4),
        (1, 5),
        (2, 6),
        (3, 7),
    ];
    // Faces as vertex cycles, counter-clockwise seen from outside.
    let faces: [([usize; 4], P3, P3); 6] = [
        ([0, 3, 2, 1], [0.0, 0.0, -1.0], [0.0, 0.0, 0.0]),
        ([4, 5, 6, 7], [0.0, 0.0, 1.0], [0.0, 0.0, 1.0]),
        ([0, 1, 5, 4], [0.0, -1.0, 0.0], [0.0, 0.0, 0.0]),
        ([1, 2, 6, 5], [1.0, 0.0, 0.0], [1.0, 0.0, 0.0]),
        ([2, 3, 7, 6], [0.0, 1.0, 0.0], [0.0, 1.0, 0.0]),
        ([3, 0, 4, 7], [-1.0, 0.0, 0.0], [0.0, 0.0, 0.0]),
    ];
    let (face0, loop0, coedge0, edge0, vertex0, point0, curve0, surf0) =
        (4, 10, 16, 40, 52, 60, 68, 80);
    let mut w = Writer::new(2, 2);
    w.record("asmheader")
        .ptr(-1)
        .int(-1)
        .str("231.6.3.65535")
        .end();
    let attrib0 = 86;
    let attrib = |i: i32| if named { attrib0 + i } else { -1 };
    w.record("body")
        .ptr(attrib(6))
        .int(-1)
        .ptr(-1)
        .ptr(2)
        .ptr(-1)
        .ptr(-1)
        .end();
    w.record("lump").head().ptr(-1).ptr(3).ptr(1).end();
    w.record("shell")
        .head()
        .ptr(-1)
        .ptr(-1)
        .ptr(face0)
        .ptr(-1)
        .ptr(2)
        .end();
    for f in 0..6 {
        let next = if f < 5 { face0 + f + 1 } else { -1 };
        w.record("face")
            .ptr(attrib(f))
            .int(-1)
            .ptr(-1)
            .ptr(next)
            .ptr(loop0 + f)
            .ptr(3)
            .ptr(-1)
            .ptr(surf0 + f);
        w.bool(false).bool(false).end();
    }
    for f in 0..6 {
        w.record("loop")
            .head()
            .ptr(-1)
            .ptr(coedge0 + 4 * f)
            .ptr(face0 + f)
            .end();
    }
    let mut coedge_edges = Vec::new();
    for (f, (cycle, _, _)) in faces.iter().enumerate() {
        for k in 0..4 {
            let (a, b) = (cycle[k], cycle[(k + 1) % 4]);
            let (ei, rev) = edges
                .iter()
                .enumerate()
                .find_map(|(i, &(x, y))| {
                    if (x, y) == (a, b) {
                        Some((i, false))
                    } else if (x, y) == (b, a) {
                        Some((i, true))
                    } else {
                        None
                    }
                })
                .expect("every face side is an edge");
            coedge_edges.push((f, k, ei, rev));
        }
    }
    for &(f, k, ei, rev) in &coedge_edges {
        let base = coedge0 + 4 * f as i32;
        let next = base + ((k + 1) % 4) as i32;
        let prev = base + ((k + 3) % 4) as i32;
        let partner = coedge_edges
            .iter()
            .position(|&(f2, _, e2, _)| e2 == ei && f2 != f)
            .map(|p| coedge0 + p as i32)
            .expect("every edge has two faces");
        w.record("coedge")
            .head()
            .ptr(next)
            .ptr(prev)
            .ptr(partner)
            .ptr(edge0 + ei as i32);
        w.bool(rev).ptr(loop0 + f as i32).int(0).ptr(-1).end();
    }
    for (i, &(a, b)) in edges.iter().enumerate() {
        let coedge = coedge_edges
            .iter()
            .position(|&(_, _, e, _)| e == i)
            .expect("every edge has a coedge");
        w.record("edge")
            .head()
            .ptr(vertex0 + a as i32)
            .dbl(0.0)
            .ptr(vertex0 + b as i32)
            .dbl(1.0);
        w.ptr(coedge0 + coedge as i32)
            .ptr(curve0 + i as i32)
            .bool(false)
            .str("unknown")
            .end();
    }
    for i in 0..8 {
        let edge = edges
            .iter()
            .position(|&(a, b)| a == i || b == i)
            .expect("every corner has an edge");
        w.record("vertex")
            .head()
            .ptr(edge0 + edge as i32)
            .int(0)
            .ptr(point0 + i as i32)
            .end();
    }
    for p in corners {
        w.record("point").head().pos(p).end();
    }
    for &(a, b) in &edges {
        let d = brep::sub(corners[b], corners[a]);
        w.record("straight-curve")
            .head()
            .pos(corners[a])
            .vec(d)
            .bool(false)
            .bool(false)
            .end();
    }
    for (_, n, o) in faces {
        let u = if n[0] != 0.0 {
            [0.0, 1.0, 0.0]
        } else {
            [1.0, 0.0, 0.0]
        };
        w.record("plane-surface").head().pos(o).vec(n).vec(u);
        w.bool(false)
            .bool(false)
            .bool(false)
            .bool(false)
            .bool(false)
            .end();
    }
    if named {
        // Faces 0..6, then the body.
        for i in 0..7 {
            let (owner, kind_type, tag, kind) = if i < 6 {
                (face0 + i, 1, (i + 1).to_string(), 0)
            } else {
                (1, 3, "301".to_owned(), 7)
            };
            w.record("ATTRIB_CUSTOM-attrib")
                .ptr(-1)
                .int(-1)
                .ptr(-1)
                .ptr(-1)
                .ptr(owner)
                .str("generic_tag_attrib_def")
                .int(3)
                .int(3)
                .int(-1)
                .str("generic_tag_attrib_def ")
                .int(1)
                .int(kind_type)
                .str(&tag)
                .int(kind);
            if i < 6 {
                w.int(1).int(301);
            } else {
                w.int(0);
            }
            w.int(0).end();
        }
    }
    w.finish()
}

/// A cylinder of radius 1 cm and height 2 cm on the XY plane. Like ASM, the
/// side face has no seam: it is bounded by the two circles only.
pub fn cylinder_blob() -> Vec<u8> {
    use std::f64::consts::TAU;
    // 0 asmheader, 1 body, 2 lump, 3 shell, faces 4 bottom 5 top 6 side,
    // loops 7 bottom 8 top 9 side-bottom 10 side-top, coedges 11..15 in the
    // same order, edges 15 bottom 16 top, vertices 17 18, points 19 20,
    // circles 21 22, planes 23 24, cylinder 25.
    let mut w = Writer::new(2, 2);
    w.record("asmheader")
        .ptr(-1)
        .int(-1)
        .str("231.6.3.65535")
        .end();
    w.record("body").head().ptr(2).ptr(-1).ptr(-1).end();
    w.record("lump").head().ptr(-1).ptr(3).ptr(1).end();
    w.record("shell")
        .head()
        .ptr(-1)
        .ptr(-1)
        .ptr(4)
        .ptr(-1)
        .ptr(2)
        .end();
    // Faces: next, loop, shell, subshell, surface, sense, double-sided.
    w.record("face")
        .head()
        .ptr(5)
        .ptr(7)
        .ptr(3)
        .ptr(-1)
        .ptr(23)
        .bool(false)
        .bool(false)
        .end();
    w.record("face")
        .head()
        .ptr(6)
        .ptr(8)
        .ptr(3)
        .ptr(-1)
        .ptr(24)
        .bool(false)
        .bool(false)
        .end();
    w.record("face")
        .head()
        .ptr(-1)
        .ptr(9)
        .ptr(3)
        .ptr(-1)
        .ptr(25)
        .bool(false)
        .bool(false)
        .end();
    w.record("loop").head().ptr(-1).ptr(11).ptr(4).end();
    w.record("loop").head().ptr(-1).ptr(12).ptr(5).end();
    w.record("loop").head().ptr(10).ptr(13).ptr(6).end();
    w.record("loop").head().ptr(-1).ptr(14).ptr(6).end();
    // Coedges: next, previous, partner, edge, reversed, loop, int, pcurve.
    // The bottom face looks down, so it runs the bottom circle backwards.
    for (id, partner, edge, reversed, lp) in [
        (11, 13, 15, true, 7),
        (12, 14, 16, false, 8),
        (13, 11, 15, false, 9),
        (14, 12, 16, true, 10),
    ] {
        w.record("coedge")
            .head()
            .ptr(id)
            .ptr(id)
            .ptr(partner)
            .ptr(edge)
            .bool(reversed)
            .ptr(lp)
            .int(0)
            .ptr(-1)
            .end();
    }
    for (v, curve, coedge) in [(17, 21, 11), (18, 22, 12)] {
        w.record("edge")
            .head()
            .ptr(v)
            .dbl(0.0)
            .ptr(v)
            .dbl(TAU)
            .ptr(coedge)
            .ptr(curve)
            .bool(false)
            .str("convex")
            .end();
    }
    w.record("vertex").head().ptr(15).int(0).ptr(19).end();
    w.record("vertex").head().ptr(16).int(0).ptr(20).end();
    w.record("point").head().pos([1.0, 0.0, 0.0]).end();
    w.record("point").head().pos([1.0, 0.0, 2.0]).end();
    for z in [0.0, 2.0] {
        w.record("ellipse-curve")
            .head()
            .pos([0.0, 0.0, z])
            .vec([0.0, 0.0, 1.0])
            .vec([1.0, 0.0, 0.0])
            .dbl(1.0);
        w.bool(false).bool(false).end();
    }
    for (z, nz) in [(0.0, -1.0), (2.0, 1.0)] {
        w.record("plane-surface")
            .head()
            .pos([0.0, 0.0, z])
            .vec([0.0, 0.0, nz])
            .vec([1.0, 0.0, 0.0]);
        w.bool(false)
            .bool(false)
            .bool(false)
            .bool(false)
            .bool(false)
            .end();
    }
    // Cone with sin 0, cos 1: a cylinder with the normal away from the axis.
    w.record("cone-surface")
        .head()
        .pos([0.0; 3])
        .vec([0.0, 0.0, 1.0])
        .vec([1.0, 0.0, 0.0])
        .dbl(1.0);
    w.bool(false).bool(false).dbl(0.0).dbl(1.0).dbl(1.0);
    w.bool(false)
        .bool(false)
        .bool(false)
        .bool(false)
        .bool(false)
        .end();
    w.finish()
}

/// A cone of base radius 1 cm and height 2 cm on the XY plane, apex up.
/// As in ASM, the side face is bounded by the base circle and a loop with
/// one curve-less edge at the apex.
pub fn cone_blob() -> Vec<u8> {
    use std::f64::consts::TAU;
    // 0 asmheader, 1 body, 2 lump, 3 shell, faces 4 bottom 5 side,
    // loops 6 bottom 7 side-base 8 side-apex, coedges 9 10 11 in the same
    // order, edges 12 base 13 apex, vertices 14 base 15 apex, points 16 17,
    // circle 18, plane 19, cone 20.
    let mut w = Writer::new(2, 2);
    w.record("asmheader")
        .ptr(-1)
        .int(-1)
        .str("231.6.3.65535")
        .end();
    w.record("body").head().ptr(2).ptr(-1).ptr(-1).end();
    w.record("lump").head().ptr(-1).ptr(3).ptr(1).end();
    w.record("shell")
        .head()
        .ptr(-1)
        .ptr(-1)
        .ptr(4)
        .ptr(-1)
        .ptr(2)
        .end();
    for (next, lp, surface) in [(5, 6, 19), (-1, 7, 20)] {
        w.record("face")
            .head()
            .ptr(next)
            .ptr(lp)
            .ptr(3)
            .ptr(-1)
            .ptr(surface)
            .bool(false)
            .bool(false)
            .end();
    }
    w.record("loop").head().ptr(-1).ptr(9).ptr(4).end();
    w.record("loop").head().ptr(8).ptr(10).ptr(5).end();
    w.record("loop").head().ptr(-1).ptr(11).ptr(5).end();
    for (id, partner, edge, reversed, lp) in [
        (9, 10, 12, true, 6),
        (10, 9, 12, false, 7),
        (11, -1, 13, false, 8),
    ] {
        w.record("coedge")
            .head()
            .ptr(id)
            .ptr(id)
            .ptr(partner)
            .ptr(edge)
            .bool(reversed)
            .ptr(lp)
            .int(0)
            .ptr(-1)
            .end();
    }
    // The base circle; the apex edge has no curve.
    for (v0, v1, end, coedge, curve) in [(14, 14, TAU, 9, 18), (15, 15, 1.0, 11, -1)] {
        w.record("edge")
            .head()
            .ptr(v0)
            .dbl(0.0)
            .ptr(v1)
            .dbl(end)
            .ptr(coedge)
            .ptr(curve)
            .bool(false)
            .str("unknown")
            .end();
    }
    w.record("vertex").head().ptr(12).int(0).ptr(16).end();
    w.record("vertex").head().ptr(13).int(0).ptr(17).end();
    w.record("point").head().pos([1.0, 0.0, 0.0]).end();
    w.record("point").head().pos([0.0, 0.0, 2.0]).end();
    w.record("ellipse-curve")
        .head()
        .pos([0.0; 3])
        .vec([0.0, 0.0, 1.0])
        .vec([1.0, 0.0, 0.0])
        .dbl(1.0);
    w.bool(false).bool(false).end();
    w.record("plane-surface")
        .head()
        .pos([0.0; 3])
        .vec([0.0, 0.0, -1.0])
        .vec([1.0, 0.0, 0.0]);
    w.bool(false)
        .bool(false)
        .bool(false)
        .bool(false)
        .bool(false)
        .end();
    // The radius shrinks by 0.5 per unit of height: sin/cos = -0.5.
    let (sin, cos) = (-1.0 / 5f64.sqrt(), 2.0 / 5f64.sqrt());
    w.record("cone-surface")
        .head()
        .pos([0.0; 3])
        .vec([0.0, 0.0, 1.0])
        .vec([1.0, 0.0, 0.0])
        .dbl(1.0);
    w.bool(false).bool(false).dbl(sin).dbl(cos).dbl(1.0);
    w.bool(false)
        .bool(false)
        .bool(false)
        .bool(false)
        .bool(false)
        .end();
    w.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asm::AsmFile;
    use crate::convert::{Options, convert_file};

    #[test]
    fn converts_a_cube() {
        let data = cube_blob();
        let file = AsmFile::parse(&data).unwrap();
        assert!(file.complete);
        let bodies = convert_file(&file, &Options::default());
        assert_eq!(bodies.len(), 1);
        let b = &bodies[0];
        assert!(b.issues.is_empty(), "{:?}", b.issues);
        assert_eq!(b.body.faces.len(), 6);
        assert_eq!(b.body.edges.len(), 12);
        assert_eq!(b.body.vertices.len(), 8);
        assert!(b.check.is_clean(), "{:?}", b.check);
        assert_eq!(b.check.off_surface_vertices, 0);
        assert!(b.body.is_solid());
        // Centimetres to millimetres.
        assert!(
            b.body
                .vertices
                .iter()
                .any(|v| v.point == [10.0, 10.0, 10.0])
        );
    }

    #[test]
    fn reports_broken_loops() {
        let data = cube_blob();
        let file = AsmFile::parse(&data).unwrap();
        let mut b = convert_file(&file, &Options::default()).remove(0).body;
        b.faces[0].loops[0].swap(0, 1);
        assert_eq!(b.check(1e-3).open_loops, 1);
    }

    #[test]
    fn finds_the_body_of_faces_edges_and_vertices() {
        use crate::convert::owner_body;
        let data = cube_blob();
        let file = AsmFile::parse(&data).unwrap();
        // Records: 4 face, 10 loop, 16 coedge, 40 edge, 52 vertex.
        for r in [1, 2, 3, 4, 10, 16, 40, 52] {
            assert_eq!(owner_body(&file, r), Some(1), "record {r}");
        }
        assert_eq!(owner_body(&file, 60), None, "a point has no owner pointer");
    }

    #[test]
    fn splits_shells_into_connected_parts() {
        let data = cube_blob();
        let file = AsmFile::parse(&data).unwrap();
        let mut b = convert_file(&file, &Options::default()).remove(0).body;
        // Keep the bottom and top faces only: two parts without a shared edge.
        b.lumps[0].shells[0].faces = vec![0, 1];
        b.split_disconnected_shells();
        assert_eq!(b.lumps[0].shells.len(), 2);
        assert!(!b.is_solid());
    }

    #[test]
    fn converts_a_cylinder() {
        let data = cylinder_blob();
        let file = AsmFile::parse(&data).unwrap();
        let b = convert_file(&file, &Options::default()).remove(0);
        assert!(b.issues.is_empty(), "{:?}", b.issues);
        assert!(b.check.is_clean(), "{:?}", b.check);
        assert_eq!(b.check.off_surface_vertices, 0);
        assert!(b.body.is_solid());
        assert_eq!(b.body.faces[2].loops.len(), 2);
        assert_eq!(b.body.surfaces[2].kind(), "cylinder");
    }

    #[test]
    fn converts_a_cone_with_its_apex_as_a_point_loop() {
        let data = cone_blob();
        let file = AsmFile::parse(&data).unwrap();
        let b = convert_file(&file, &Options::default()).remove(0);
        assert!(b.issues.is_empty(), "{:?}", b.issues);
        assert!(b.check.is_clean(), "{:?}", b.check);
        assert_eq!(b.check.off_surface_vertices, 0);
        assert!(b.body.is_solid());
        assert_eq!(b.degenerate_edges, 1);
        let side = &b.body.faces[1];
        assert_eq!(b.body.surfaces[side.surface].kind(), "cone");
        assert_eq!(side.loops.len(), 1);
        assert_eq!(side.point_loops.len(), 1);
        assert_eq!(b.body.vertices[side.point_loops[0]].point, [0.0, 0.0, 20.0]);
    }
}
