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
/// `mitcad_f3d::names`): the body `"301"`, the faces `"1"` (bottom), `"2"`
/// (top), `"3"` (y = 0), `"4"` (x = 1), `"5"` (y = 1) and `"6"` (x = 0), all
/// made by operation 301.
pub fn named_cube_blob() -> Vec<u8> {
    cube(true)
}

/// The cube of [`cube_blob`] with an ASM history of three operations,
/// newest first: state 3 moved the corner (1, 1, 1) from (1, 1, 0.5),
/// state 2 changed nothing, state 1 created the body. Rolled back one or
/// two states, the corner is at (1, 1, 0.5); three, there is no body.
pub fn cube_with_history_blob() -> Vec<u8> {
    // The live records 0..86, the history section (records 86..90: its
    // header and the three states; its end marker takes no record), and
    // the corner point's copy, record 90 and pointer 86 (the history
    // section takes no pointer numbers).
    let mut w = cube_writer(false);
    let (corner, copy) = (66, 86);
    w.record("Begin-of-ASM-History-Data")
        .ident("history_stream")
        .int(3)
        .int(3)
        .int(0)
        .int(3)
        .ptr(-1)
        .ptr(0)
        .ptr(2)
        .ptr(-1)
        .end();
    // State 3 (index 0): newer -1, older 1, itself 0; the corner changed.
    w.record("delta_state")
        .int(3)
        .int(1)
        .int(0)
        .ptr(-1)
        .ptr(1)
        .ptr(0)
        .ptr(-1)
        .ptr(0);
    w.bool(false).int(1).ptr(0).int(2);
    w.int(1).ptr(copy).ptr(corner).int(0).int(0).int(0).end();
    // State 2 (index 1): no bulletins.
    w.record("delta_state")
        .int(2)
        .int(1)
        .int(0)
        .ptr(0)
        .ptr(2)
        .ptr(1)
        .ptr(-1)
        .ptr(0);
    w.bool(false).int(0).int(0).end();
    // State 1 (index 2): created the body.
    w.record("delta_state")
        .int(1)
        .int(1)
        .int(0)
        .ptr(1)
        .ptr(-1)
        .ptr(2)
        .ptr(-1)
        .ptr(0);
    w.bool(false).int(1).ptr(2).int(2);
    w.int(1).ptr(-1).ptr(1).int(0).int(0).int(0).end();
    w.record("End-of-ASM-History-Section");
    w.record("point").head().pos([1.0, 1.0, 0.5]).end();
    w.finish()
}

/// The named cube of [`named_cube_blob`] with an ASM history of three
/// operations, newest first: state 3 changed only attributes and an
/// attribute pointer (it changed the first face's name attribute, made
/// another one and changed the corner (0, 0, 0)'s attribute pointer),
/// state 2 moved the corner (1, 1, 1) from (1, 1, 0.5), state 1 created
/// the body (mitcad#675).
pub fn named_cube_with_attribute_history_blob() -> Vec<u8> {
    // The live records 0..93 (the cube's 0..86, its name attributes
    // 86..93), a new attribute 93, the history section (records 94..98:
    // its header and the three states), and the copies: of attribute 86
    // (record 98, pointer 94), of point 60 (99, pointer 95) and of point
    // 66 (100, pointer 96).
    let mut w = cube_writer(true);
    let attribute = |w: &mut Writer, owner: i32, tag: &str| {
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
            .int(1)
            .str(tag)
            .int(0)
            .int(1)
            .int(301)
            .int(0)
            .end();
    };
    attribute(&mut w, 4, "7");
    w.record("Begin-of-ASM-History-Data")
        .ident("history_stream")
        .int(3)
        .int(3)
        .int(0)
        .int(3)
        .ptr(-1)
        .ptr(0)
        .ptr(2)
        .ptr(-1)
        .end();
    // State 3 (index 0): attribute 86 changed, attribute 93 made, point
    // 60's attribute pointer changed.
    w.record("delta_state")
        .int(3)
        .int(1)
        .int(0)
        .ptr(-1)
        .ptr(1)
        .ptr(0)
        .ptr(-1)
        .ptr(0);
    w.bool(false).int(1).ptr(0).int(2);
    w.int(1).ptr(94).ptr(86);
    w.int(1).ptr(-1).ptr(93);
    w.int(1).ptr(95).ptr(60);
    w.int(0).int(0).int(0).end();
    // State 2 (index 1): the corner (1, 1, 1) moved.
    w.record("delta_state")
        .int(2)
        .int(1)
        .int(0)
        .ptr(0)
        .ptr(2)
        .ptr(1)
        .ptr(-1)
        .ptr(0);
    w.bool(false).int(1).ptr(1).int(2);
    w.int(1).ptr(96).ptr(66).int(0).int(0).int(0).end();
    // State 1 (index 2): created the body.
    w.record("delta_state")
        .int(1)
        .int(1)
        .int(0)
        .ptr(1)
        .ptr(-1)
        .ptr(2)
        .ptr(-1)
        .ptr(0);
    w.bool(false).int(1).ptr(2).int(2);
    w.int(1).ptr(-1).ptr(1).int(0).int(0).int(0).end();
    w.record("End-of-ASM-History-Section");
    attribute(&mut w, 4, "8");
    w.record("point")
        .ptr(86)
        .int(-1)
        .ptr(-1)
        .pos([0.0, 0.0, 0.0])
        .end();
    w.record("point").head().pos([1.0, 1.0, 0.5]).end();
    w.finish()
}

fn cube(named: bool) -> Vec<u8> {
    cube_writer(named).finish()
}

/// The cube of [`cube_blob`] whose point records hold no position, as when
/// a reader loses the end of a body: its faces have their surfaces, but
/// none of its vertices, edges or loops can be read, so no face can be
/// built (a DWG import went on without such a body, mitcad#589).
pub fn cube_without_points_blob() -> Vec<u8> {
    cube_writer_with(false, false).finish()
}

/// The cube's records written, before the end of the data.
fn cube_writer(named: bool) -> Writer {
    cube_writer_with(named, true)
}

/// The cube of [`cube_blob`] whose faces carry these keys in their
/// records' second header field (their history ids; the `.ipt` graphics
/// faces name faces by them, mitcad#1367), in the order bottom, top,
/// y = 0, x = 1, y = 1, x = 0.
pub fn keyed_cube_blob(keys: [i32; 6]) -> Vec<u8> {
    cube_writer_keyed(false, true, keys).finish()
}

/// [`cube_writer`], with or without the points' positions.
fn cube_writer_with(named: bool, positions: bool) -> Writer {
    cube_writer_keyed(named, positions, [-1; 6])
}

/// [`cube_writer_with`], the faces with these keys ([`keyed_cube_blob`]).
fn cube_writer_keyed(named: bool, positions: bool, keys: [i32; 6]) -> Writer {
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
            .int(keys[f as usize])
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
        w.record("point").head();
        if positions {
            w.pos(p);
        }
        w.end();
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
    w
}

/// A cylinder of radius 1 cm and height 2 cm on the XY plane. Like ASM, the
/// side face has no seam: it is bounded by the two circles only.
pub fn cylinder_blob() -> Vec<u8> {
    cylinder_blob_with(CylinderVariant::default())
}

/// What [`cylinder_blob_with`] changes in [`cylinder_blob`]
/// (mitcad_private#198).
#[derive(Clone, Copy, Debug, Default)]
pub struct CylinderVariant {
    /// The side's surface a spline of a subtype the reader does not know.
    pub unknown_side: bool,
    /// The top circle a curve of a subtype the reader does not know.
    pub unknown_top: bool,
    /// The side face in a subshell of the shell (record 26).
    pub side_in_subshell: bool,
}

/// [`cylinder_blob`] with the changes `variant` names.
pub fn cylinder_blob_with(variant: CylinderVariant) -> Vec<u8> {
    use std::f64::consts::TAU;
    // 0 asmheader, 1 body, 2 lump, 3 shell, faces 4 bottom 5 top 6 side,
    // loops 7 bottom 8 top 9 side-bottom 10 side-top, coedges 11..15 in the
    // same order, edges 15 bottom 16 top, vertices 17 18, points 19 20,
    // circles 21 22, planes 23 24, cylinder 25 (and the subshell 26).
    let subshell = if variant.side_in_subshell { 26 } else { -1 };
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
        .ptr(subshell)
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
        .ptr(if variant.side_in_subshell { -1 } else { 6 })
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
        .ptr(subshell)
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
        if z > 0.0 && variant.unknown_top {
            w.record("intcurve-curve")
                .head()
                .bool(false)
                .sub_start()
                .ident("mystery_int_cur")
                .int(1)
                .sub_end();
            w.bool(false).bool(false).end();
            continue;
        }
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
    if variant.unknown_side {
        w.record("spline-surface")
            .head()
            .bool(false)
            .sub_start()
            .ident("mystery_spl_sur")
            .int(1)
            .sub_end();
        w.bool(false).bool(false).bool(false).bool(false).end();
    } else {
        // Cone with sin 0, cos 1: a cylinder with the normal away from the
        // axis.
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
    }
    if variant.side_in_subshell {
        // Parent, next, child, first face, wire.
        w.record("subshell")
            .head()
            .ptr(3)
            .ptr(-1)
            .ptr(-1)
            .ptr(6)
            .ptr(-1)
            .end();
    }
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

/// A solid bounded by planar polygons: its corners and its faces as
/// cycles of corners, counter-clockwise seen from outside.
struct Polyhedron {
    corners: Vec<P3>,
    faces: Vec<Vec<usize>>,
}

impl Polyhedron {
    /// The 1 cm cube of [`cube_blob`].
    fn cube() -> Self {
        Polyhedron {
            corners: vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [1.0, 1.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
                [1.0, 0.0, 1.0],
                [1.0, 1.0, 1.0],
                [0.0, 1.0, 1.0],
            ],
            faces: vec![
                vec![0, 3, 2, 1],
                vec![4, 5, 6, 7],
                vec![0, 1, 5, 4],
                vec![1, 2, 6, 5],
                vec![2, 3, 7, 6],
                vec![3, 0, 4, 7],
            ],
        }
    }

    /// The cube with its edge from (1, 0, 1) to (1, 1, 1) bevelled by
    /// `d` cm: a planar face 45 degrees to its sides in its place.
    fn bevelled_cube(d: f64) -> Self {
        Polyhedron {
            corners: vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [1.0, 1.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
                [1.0 - d, 0.0, 1.0],
                [1.0 - d, 1.0, 1.0],
                [0.0, 1.0, 1.0],
                [1.0, 0.0, 1.0 - d],
                [1.0, 1.0, 1.0 - d],
            ],
            faces: vec![
                vec![0, 3, 2, 1],
                vec![4, 5, 6, 7],
                vec![0, 1, 8, 5, 4],
                vec![1, 2, 9, 8],
                vec![2, 3, 7, 6, 9],
                vec![3, 0, 4, 7],
                vec![5, 8, 9, 6],
            ],
        }
    }

    /// Its edges, each in the direction its first face takes it.
    fn edges(&self) -> Vec<(usize, usize)> {
        let mut out: Vec<(usize, usize)> = Vec::new();
        for cycle in &self.faces {
            for k in 0..cycle.len() {
                let (a, b) = (cycle[k], cycle[(k + 1) % cycle.len()]);
                if !out.contains(&(a, b)) && !out.contains(&(b, a)) {
                    out.push((a, b));
                }
            }
        }
        out
    }

    /// How many records [`Polyhedron::write`] writes after the header.
    fn records(&self) -> usize {
        let (f, e, v) = (self.faces.len(), self.edges().len(), self.corners.len());
        let coedges: usize = self.faces.iter().map(Vec::len).sum();
        3 + f + f + coedges + e + v + v + e + f
    }

    /// Writes its records: body, lump, shell, faces, loops, coedges,
    /// edges, vertices, points, curves and surfaces. `refer` gives the
    /// pointer of each record by its place (1 the body, 2 the lump, 3 the
    /// shell, then in that order), for the pointer fields.
    fn write(&self, w: &mut Writer, refer: &dyn Fn(usize) -> i32) {
        let edges = self.edges();
        let (f, e, v) = (self.faces.len(), edges.len(), self.corners.len());
        let coedges: usize = self.faces.iter().map(Vec::len).sum();
        let face0 = 4;
        let loop0 = face0 + f;
        let coedge0 = loop0 + f;
        let edge0 = coedge0 + coedges;
        let vertex0 = edge0 + e;
        let point0 = vertex0 + v;
        let curve0 = point0 + v;
        let surface0 = curve0 + e;
        let p = |k: usize| refer(k);
        w.record("body").head().ptr(p(2)).ptr(-1).ptr(-1).end();
        w.record("lump").head().ptr(-1).ptr(p(3)).ptr(p(1)).end();
        w.record("shell")
            .head()
            .ptr(-1)
            .ptr(-1)
            .ptr(p(face0))
            .ptr(-1)
            .ptr(p(2))
            .end();
        for i in 0..f {
            let next = if i + 1 < f { p(face0 + i + 1) } else { -1 };
            w.record("face")
                .head()
                .ptr(next)
                .ptr(p(loop0 + i))
                .ptr(p(3))
                .ptr(-1)
                .ptr(p(surface0 + i));
            w.bool(false).bool(false).end();
        }
        // The coedges of each face in its cycle's order: (face, edge, reversed).
        let mut sides: Vec<(usize, usize, bool)> = Vec::new();
        let mut first = Vec::new();
        for (i, cycle) in self.faces.iter().enumerate() {
            first.push(sides.len());
            for k in 0..cycle.len() {
                let (a, b) = (cycle[k], cycle[(k + 1) % cycle.len()]);
                let (j, reversed) = edges
                    .iter()
                    .enumerate()
                    .find_map(|(j, &(x, y))| {
                        if (x, y) == (a, b) {
                            Some((j, false))
                        } else if (x, y) == (b, a) {
                            Some((j, true))
                        } else {
                            None
                        }
                    })
                    .expect("every side of a face is an edge");
                sides.push((i, j, reversed));
            }
        }
        for (i, start) in first.iter().enumerate() {
            w.record("loop")
                .head()
                .ptr(-1)
                .ptr(p(coedge0 + start))
                .ptr(p(face0 + i))
                .end();
        }
        for (s, &(i, j, reversed)) in sides.iter().enumerate() {
            let n = self.faces[i].len();
            let k = s - first[i];
            let next = coedge0 + first[i] + (k + 1) % n;
            let prev = coedge0 + first[i] + (k + n - 1) % n;
            let partner = sides
                .iter()
                .enumerate()
                .position(|(t, &(i2, j2, _))| j2 == j && (i2 != i || t != s))
                .expect("every edge has two faces");
            w.record("coedge")
                .head()
                .ptr(p(next))
                .ptr(p(prev))
                .ptr(p(coedge0 + partner))
                .ptr(p(edge0 + j));
            w.bool(reversed).ptr(p(loop0 + i)).int(0).ptr(-1).end();
        }
        for (j, &(a, b)) in edges.iter().enumerate() {
            let side = sides
                .iter()
                .position(|&(_, j2, _)| j2 == j)
                .expect("every edge has a coedge");
            w.record("edge")
                .head()
                .ptr(p(vertex0 + a))
                .dbl(0.0)
                .ptr(p(vertex0 + b))
                .dbl(1.0);
            w.ptr(p(coedge0 + side))
                .ptr(p(curve0 + j))
                .bool(false)
                .str("unknown")
                .end();
        }
        for c in 0..v {
            let edge = edges
                .iter()
                .position(|&(a, b)| a == c || b == c)
                .expect("every corner has an edge");
            w.record("vertex")
                .head()
                .ptr(p(edge0 + edge))
                .int(0)
                .ptr(p(point0 + c))
                .end();
        }
        for &corner in &self.corners {
            w.record("point").head().pos(corner).end();
        }
        for &(a, b) in &edges {
            let d = brep::sub(self.corners[b], self.corners[a]);
            w.record("straight-curve")
                .head()
                .pos(self.corners[a])
                .vec(d)
                .bool(false)
                .bool(false)
                .end();
        }
        for cycle in &self.faces {
            // The normal by Newell's rule, along the first side.
            let mut normal = [0.0; 3];
            for k in 0..cycle.len() {
                let (a, b) = (
                    self.corners[cycle[k]],
                    self.corners[cycle[(k + 1) % cycle.len()]],
                );
                normal[0] += (a[1] - b[1]) * (a[2] + b[2]);
                normal[1] += (a[2] - b[2]) * (a[0] + b[0]);
                normal[2] += (a[0] - b[0]) * (a[1] + b[1]);
            }
            let normal = brep::normalize(normal);
            let o = self.corners[cycle[0]];
            let u = brep::normalize(brep::sub(self.corners[cycle[1]], o));
            w.record("plane-surface").head().pos(o).vec(normal).vec(u);
            w.bool(false)
                .bool(false)
                .bool(false)
                .bool(false)
                .bool(false)
                .end();
        }
    }
}

/// A 1 cm cube whose edge from (1, 0, 1) to (1, 1, 1) is bevelled by `d`
/// cm, with an ASM history of two operations, newest first: state 2
/// bevelled the edge (it replaced every face of the cube), state 1 created
/// the cube of [`cube_blob`]. Rolled back one state it is that cube; two,
/// there is no body. For a test of a rounding of that edge whose result
/// is not the file's (mitcad#379).
pub fn bevelled_cube_history_blob(d: f64) -> Vec<u8> {
    let live = Polyhedron::bevelled_cube(d);
    let old = Polyhedron::cube();
    // Pointers: the live records 0 (header) ..n, then the copies of the
    // cube's records (the history section takes no pointer numbers):
    // the body's, lump's and shell's copies stand for those entities as
    // they were, the others for the cube's own entities, which the bevel
    // deleted.
    let n = 1 + live.records();
    let mut w = Writer::new(2, 2);
    w.record("asmheader")
        .ptr(-1)
        .int(-1)
        .str("231.6.3.65535")
        .end();
    live.write(&mut w, &|k| k as i32);
    w.record("Begin-of-ASM-History-Data")
        .ident("history_stream")
        .int(2)
        .int(2)
        .int(0)
        .int(2)
        .ptr(-1)
        .ptr(0)
        .ptr(1)
        .ptr(-1)
        .end();
    let copy = |k: usize| (n + k - 1) as i32;
    // State 2 (index 0): newer -1, older 1, itself 0.
    w.record("delta_state")
        .int(2)
        .int(1)
        .int(0)
        .ptr(-1)
        .ptr(1)
        .ptr(0)
        .ptr(-1)
        .ptr(0);
    w.bool(false).int(1).ptr(0).int(2);
    for k in 1..=3 {
        w.int(1).ptr(copy(k)).ptr(k as i32);
    }
    for k in 4..=old.records() {
        w.int(1).ptr(copy(k)).ptr(-1);
    }
    for k in 4..=live.records() {
        w.int(1).ptr(-1).ptr(k as i32);
    }
    w.int(0).int(0).int(0).end();
    // State 1 (index 1): created the body.
    w.record("delta_state")
        .int(1)
        .int(1)
        .int(0)
        .ptr(0)
        .ptr(-1)
        .ptr(1)
        .ptr(-1)
        .ptr(0);
    w.bool(false).int(1).ptr(1).int(2);
    for k in 1..=3 {
        w.int(1).ptr(-1).ptr(k);
    }
    w.int(0).int(0).int(0).end();
    w.record("End-of-ASM-History-Section");
    old.write(&mut w, &|k| if k <= 3 { k as i32 } else { copy(k) });
    w.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asm::AsmFile;
    use crate::convert::{Options, convert_file};

    #[test]
    fn a_bevelled_cube_rolls_back_to_the_cube() {
        let data = bevelled_cube_history_blob(0.2);
        let file = AsmFile::parse(&data).unwrap();
        let history = crate::asm::history::History::parse(&file)
            .unwrap()
            .expect("a history");
        assert_eq!(history.states.len(), 2);
        assert_eq!(history.states[0].id, 2);
        let b = convert_file(&file, &Options::default()).remove(0);
        assert!(b.issues.is_empty(), "{:?}", b.issues);
        assert!(b.check.is_clean(), "{:?}", b.check);
        assert!(b.body.is_solid());
        assert_eq!(b.body.faces.len(), 7);
        // Rolled back one state: the cube.
        let (record, _) = crate::convert::body_records(&file)[0];
        let view = history.view(1);
        let cube = crate::convert::convert_body_at(&file, record, &Options::default(), Some(&view));
        assert!(cube.issues.is_empty(), "{:?}", cube.issues);
        assert!(cube.check.is_clean(), "{:?}", cube.check);
        assert!(cube.body.is_solid());
        assert_eq!(cube.body.faces.len(), 6);
        assert_eq!(cube.body.edges.len(), 12);
    }

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
        // Keep the bottom and top faces only: two parts without a shared
        // edge, side by side: a lump each.
        let mut two = b.clone();
        two.lumps[0].shells[0].faces = vec![0, 1];
        two.split_disconnected_shells();
        assert_eq!(two.lumps.len(), 2);
        assert_eq!(two.lumps[0].shells.len(), 1);
        assert_eq!(two.lumps[1].shells[0].faces, vec![1]);
        assert!(!two.is_solid());
        // A shell besides another one (a void) splits within its lump.
        b.lumps[0].shells[0].faces = vec![0, 1];
        b.lumps[0].shells.push(crate::brep::Shell {
            faces: vec![2],
            wire_edges: Vec::new(),
        });
        b.split_disconnected_shells();
        assert_eq!(b.lumps.len(), 1);
        assert_eq!(b.lumps[0].shells.len(), 3);
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
    fn reads_the_faces_of_subshells() {
        // mitcad_private#198: before, a shell's subshells were not read.
        let data = cylinder_blob_with(CylinderVariant {
            side_in_subshell: true,
            ..CylinderVariant::default()
        });
        let file = AsmFile::parse(&data).unwrap();
        let b = convert_file(&file, &Options::default()).remove(0);
        assert!(b.issues.is_empty(), "{:?}", b.issues);
        assert_eq!(b.body.faces.len(), 3);
        assert_eq!(b.body.lumps[0].shells[0].faces.len(), 3);
        assert!(b.check.is_clean(), "{:?}", b.check);
        assert!(b.body.is_solid());
    }

    #[test]
    fn keeps_faces_and_edges_of_unknown_geometry() {
        // mitcad_private#198: the side's surface does not decode. Before,
        // the face was left out and the shell open; now it is kept, with
        // its loops, on an unknown surface: the topology is closed, the
        // builder leaves the face out (no solid).
        let data = cylinder_blob_with(CylinderVariant {
            unknown_side: true,
            ..CylinderVariant::default()
        });
        let file = AsmFile::parse(&data).unwrap();
        let b = convert_file(&file, &Options::default()).remove(0);
        assert_eq!(b.body.faces.len(), 3);
        assert_eq!(b.skipped_faces, 1);
        let side = &b.body.faces[2];
        assert_eq!(b.body.surfaces[side.surface].kind(), "unknown");
        assert_eq!(side.loops.len(), 2);
        assert!(b.check.is_clean(), "{:?}", b.check);
        assert_eq!(b.check.free_edges, 0);
        assert!(b.body.shell_is_closed(&b.body.lumps[0].shells[0]));
        assert!(!b.body.is_solid());
        assert!(
            b.issues.iter().any(|i| i.starts_with("surface 25 ")),
            "{:?}",
            b.issues
        );
        let flat = crate::flat::FlatBody::from_body(&b.body);
        assert_eq!(
            flat.surfaces[side.surface].kind,
            crate::flat::SURFACE_UNKNOWN
        );
        assert!(!flat.shells[0].closed);

        // The top circle does not decode: before, its edge left both its
        // loops; now it is kept on an unknown curve.
        let data = cylinder_blob_with(CylinderVariant {
            unknown_top: true,
            ..CylinderVariant::default()
        });
        let file = AsmFile::parse(&data).unwrap();
        let b = convert_file(&file, &Options::default()).remove(0);
        assert_eq!((b.skipped_faces, b.unknown_edges), (0, 1));
        assert_eq!(b.body.edges.len(), 2);
        let top = &b.body.edges[1];
        assert_eq!(b.body.curves[top.curve].kind(), "unknown");
        assert_eq!(top.t, [0.0, std::f64::consts::TAU]);
        assert_eq!(
            b.body.faces[1].loops,
            vec![vec![brep::Coedge {
                edge: 1,
                forward: true
            }]]
        );
        assert!(b.check.is_clean(), "{:?}", b.check);
        assert!(
            b.issues
                .iter()
                .filter(|i| i.starts_with("edge 16 "))
                .count()
                == 1,
            "{:?}",
            b.issues
        );
        assert!(crate::convert::edge_curve(&file, 1, 16, &Options::default(), None).is_err());
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
