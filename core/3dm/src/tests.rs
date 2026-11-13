// SPDX-License-Identifier: MIT
//! The reader and the conversion on the sample model ([`testfile`]) and,
//! when `MITCAD_3DM_CORPUS` names a folder, on the `.3dm` files in it.

use crate::convert::{self, Converted};
use crate::geom::{Curve, Geometry};
use crate::model::ColorSource;
use crate::nurbs::Spline;
use crate::testfile;
use crate::{File3dm, archive};
use mitcad_asm::brep::{self, Body};
use mitcad_vec3::{add, cross, dot, norm, scale, sub};

fn sample() -> File3dm {
    File3dm::read(&testfile::sample()).expect("the sample reads")
}

fn object<'a>(file: &'a File3dm, name: &str) -> &'a Geometry {
    &file
        .objects
        .iter()
        .find(|o| o.attributes.name == name)
        .unwrap_or_else(|| panic!("an object named {name}"))
        .geometry
}

/// The start vertex of a coedge.
fn start(body: &Body, c: brep::Coedge) -> usize {
    let v = body.edges[c.edge].vertices;
    if c.forward { v[0] } else { v[1] }
}

fn end(body: &Body, c: brep::Coedge) -> usize {
    let v = body.edges[c.edge].vertices;
    if c.forward { v[1] } else { v[0] }
}

/// Every loop closes; the body is a closed solid by its own check.
fn assert_closed(c: &Converted) {
    assert!(c.issues.is_empty(), "{:?}", c.issues);
    let body = &c.body;
    for (fi, f) in body.faces.iter().enumerate() {
        for lp in &f.loops {
            for k in 0..lp.len() {
                assert_eq!(
                    end(body, lp[k]),
                    start(body, lp[(k + 1) % lp.len()]),
                    "face {fi}: its loop is not a chain"
                );
            }
        }
    }
    let check = body.check(1e-6);
    assert_eq!(check.free_edges, 0, "{check:?}");
    assert_eq!(check.unpaired_coedges, 0, "{check:?}");
    assert_eq!(check.vertex_mismatches, 0, "{check:?}");
    assert!(body.is_solid(), "not a solid");
}

/// A planar face's outward normal (its plane's, turned by `reversed`).
fn plane_normal(body: &Body, f: &brep::Face) -> Option<[f64; 3]> {
    match &body.surfaces[f.surface] {
        brep::Surface::Plane { normal, .. } => Some(if f.reversed {
            scale(*normal, -1.0)
        } else {
            *normal
        }),
        _ => None,
    }
}

/// The outer loop's area about the face's normal, from its vertices.
fn loop_area(body: &Body, lp: &[brep::Coedge], n: [f64; 3]) -> f64 {
    let pts: Vec<[f64; 3]> = lp
        .iter()
        .map(|&c| body.vertices[start(body, c)].point)
        .collect();
    let mut a = [0.0; 3];
    for k in 0..pts.len() {
        a = add(a, cross(pts[k], pts[(k + 1) % pts.len()]));
    }
    dot(a, n) / 2.0
}

#[test]
fn reads_the_header_settings_and_layers() {
    let file = sample();
    assert_eq!(file.version, 70);
    assert_eq!(file.writer, 2_382_395_020);
    assert_eq!(file.application, "MitCad test file");
    assert_eq!(file.units.name, "centimeters");
    assert_eq!(file.units.mm, 10.0);
    assert_eq!(file.units.symbol(), Some("cm"));
    assert_eq!(file.tolerance, 0.001);
    let names: Vec<&str> = file.layers.iter().map(|l| l.name.as_str()).collect();
    assert_eq!(names, ["Default", "Solids", "Meshes"]);
    assert_eq!(file.layer_path(1), "Solids");
    assert_eq!(file.layers[1].color, 0x0000_00FF);
    assert!(file.failed.is_empty(), "{:?}", file.failed);
    assert!(file.warnings.is_empty(), "{:?}", file.warnings);
    assert_eq!(file.objects.len(), 9);
    assert_eq!(file.definitions.len(), 1);
    assert_eq!(file.definitions[0].name, "Bolt");
    assert_eq!(
        file.definitions[0].objects,
        [testfile::record_id("bolt head")]
    );
}

#[test]
fn reads_the_attributes() {
    let file = sample();
    let cyl = file
        .objects
        .iter()
        .find(|o| o.attributes.name == "cylinder")
        .expect("the cylinder");
    assert_eq!(cyl.attributes.layer, 1);
    assert_eq!(cyl.attributes.color_source, ColorSource::Object);
    assert_eq!(file.object_color(&cyl.attributes, None), Some(0x0000_FFFF));
    let b = file
        .objects
        .iter()
        .find(|o| o.attributes.name == "box")
        .expect("the box");
    assert_eq!(file.object_color(&b.attributes, None), Some(0x0000_00FF));
    let head = file
        .objects
        .iter()
        .find(|o| o.attributes.name == "bolt head")
        .expect("the bolt head");
    assert_eq!(head.attributes.mode, 3);
    assert_eq!(head.attributes.id, testfile::record_id("bolt head"));
}

#[test]
fn a_damaged_file_is_read_as_far_as_it_goes() {
    let data = testfile::sample();
    // Cut short in the object table: the objects before the cut, and a
    // warning.
    let cut = File3dm::read(&data[..data.len() - 300]).expect("the start reads");
    assert!(cut.objects.len() < 9);
    assert!(
        cut.warnings
            .iter()
            .any(|w| w.0.contains("the object table ends early")),
        "{:?}",
        cut.warnings
    );
    assert_words_only(&cut);
    // Any byte changed: an error, a failed object or other values, never a
    // panic, in the reader or in the conversion of what it read.
    for at in (40..data.len()).step_by(7) {
        let mut d = data.clone();
        d[at] ^= 0xA5;
        if let Ok(file) = File3dm::read(&d) {
            assert_words_only(&file);
            convert_all(&file);
        }
    }
    assert!(File3dm::read(b"not a model").is_err());
}

/// The words of the file's warnings and failed objects, which a person
/// reads in the import report, hold no place: no offsets, byte counts,
/// typecodes or class ids (docs/ui-conventions.md, "What a message
/// says"); those are in the detail.
fn assert_words_only(file: &File3dm) {
    let words = file
        .warnings
        .iter()
        .chain(file.failed.iter().map(|f| &f.error))
        .map(|e| e.0.as_str());
    for w in words {
        // A typecode or a class id: a run of 8 hexadecimal digits or more.
        let hex = w
            .split(|c: char| !c.is_ascii_alphanumeric())
            .any(|t| t.len() >= 8 && t.chars().all(|c| c.is_ascii_hexdigit()));
        assert!(
            !w.contains("offset") && !w.contains(" bytes") && !hex,
            "a place in a warning's words: {w}"
        );
    }
}

/// Every object of the file converted as the import converts it (no
/// OCCT): B-reps and their render meshes, extrusions, surfaces, meshes and
/// curves.
fn convert_all(file: &File3dm) {
    let s = file.units.mm;
    for o in &file.objects {
        match &o.geometry {
            Geometry::Brep(b) => {
                let _ = convert::brep_body(b, s);
                let _ = convert::render_mesh(b).map(|m| m.triangles());
            }
            Geometry::Extrusion(e) => {
                let _ = convert::extrusion_body(e, s);
            }
            Geometry::Surface(srf) => {
                let _ = convert::surface_body(srf, s);
            }
            Geometry::Mesh(m) => {
                let _ = m.triangles();
            }
            Geometry::Curve(c) => {
                let _ = convert::to_spline(c).map(|spline| spline.to_brep(s));
            }
            Geometry::Instance(_) | Geometry::Other(_) => {}
        }
    }
}

/// The models of the test writer cut short at every length, and with
/// bytes overwritten by values that make lengths and counts large, and
/// floats not numbers: each is read as far as it goes or refused, and
/// what is read converts, without a panic (the reader gets untrusted
/// files).
#[test]
fn cut_short_and_damaged_models_never_panic() {
    for data in [testfile::sample(), testfile::block_in_itself()] {
        for len in (0..data.len()).step_by(3) {
            if let Ok(file) = File3dm::read(&data[..len]) {
                assert_words_only(&file);
                convert_all(&file);
            }
        }
        for at in (32..data.len()).step_by(5) {
            for value in [0xFF_u8, 0x7F, 0x80, 0x00] {
                let mut d = data.clone();
                d[at] = value;
                if let Ok(file) = File3dm::read(&d) {
                    assert_words_only(&file);
                    convert_all(&file);
                }
            }
            // Eight bytes at once: an i64 length, an f64 (NaN, infinity).
            for word in [u64::MAX, 0x7FF8_0000_0000_0000, 0x7FF0_0000_0000_0000] {
                if at + 8 <= data.len() {
                    let mut d = data.clone();
                    d[at..at + 8].copy_from_slice(&word.to_le_bytes());
                    if let Ok(file) = File3dm::read(&d) {
                        assert_words_only(&file);
                        convert_all(&file);
                    }
                }
            }
        }
    }
}

/// A curve written alone (a class chunk), read back.
fn read_back(c: &testfile::C) -> Result<Curve, archive::Error> {
    let mut w = testfile::Writer::default();
    testfile::write_curve(&mut w, c);
    let mut r = archive::Reader::new(&w.buf, 70);
    crate::geom::read_curve_object(&mut r).map(|c| c.expect("a curve"))
}

/// Polycurves nested 100 deep are refused where the chunks nest deeper
/// than the reader takes, instead of recursing without end.
#[test]
fn deeply_nested_curves_are_refused() {
    let mut c = testfile::C::Line([0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0]);
    for _ in 0..100 {
        c = testfile::C::Poly(vec![c], vec![0.0, 1.0]);
    }
    let e = read_back(&c).expect_err("refused");
    assert!(e.0.contains("nested"), "{e}");
    // Ten deep is read.
    let mut c = testfile::C::Line([0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0]);
    for _ in 0..10 {
        c = testfile::C::Poly(vec![c], vec![0.0, 1.0]);
    }
    assert!(matches!(read_back(&c), Ok(Curve::Poly { .. })));
}

/// Values the B-spline code relies on are checked as they are read: an
/// arc of more than a full turn (its spans would be counted in the
/// billions), knots that decrease or repeat more often than the degree,
/// an order OCCT does not take, polycurve parameters that do not
/// increase.
#[test]
fn invalid_curves_are_refused() {
    let arc = |angles: [f64; 2], domain: [f64; 2]| {
        testfile::C::Arc(
            [0.0; 3],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            1.0,
            angles,
            domain,
        )
    };
    assert!(read_back(&arc([0.0, std::f64::consts::PI], [0.0, 1.0])).is_ok());
    assert!(read_back(&arc([0.0, 1e300], [0.0, 1.0])).is_err());
    assert!(read_back(&arc([0.0, f64::NAN], [0.0, 1.0])).is_err());
    assert!(read_back(&arc([1.0, 0.0], [0.0, 1.0])).is_err());
    assert!(read_back(&arc([0.0, 1.0], [1.0, 1.0])).is_err());
    let points = vec![[0.0; 3], [1.0, 1.0, 0.0], [2.0, 1.0, 0.0], [3.0, 0.0, 0.0]];
    let nurbs = |order: usize, knots: Vec<f64>, cvs: Vec<[f64; 3]>| {
        read_back(&testfile::C::Nurbs(order, knots, cvs))
    };
    assert!(nurbs(4, vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0], points.clone()).is_ok());
    assert!(nurbs(4, vec![0.0, 0.0, 0.0, 1.0, 0.5, 1.0], points.clone()).is_err());
    assert!(nurbs(4, vec![0.0, 0.0, 0.0, 0.0, 1.0, 1.0], points.clone()).is_err());
    assert!(nurbs(4, vec![0.0; 6], points.clone()).is_err());
    assert!(
        nurbs(
            4,
            vec![0.0, 0.0, 0.0, f64::INFINITY, 1.0, 1.0],
            points.clone()
        )
        .is_err()
    );
    let many: Vec<[f64; 3]> = (0..30).map(|i| [f64::from(i), 0.0, 0.0]).collect();
    let knots: Vec<f64> = (0..28 + 30 - 2).map(f64::from).collect();
    assert!(nurbs(28, knots, many).is_err());
    let line = testfile::C::Line([0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0]);
    let poly =
        |params: Vec<f64>| read_back(&testfile::C::Poly(vec![line.clone(), line.clone()], params));
    assert!(poly(vec![0.0, 1.0, 2.0]).is_ok());
    assert!(poly(vec![0.0, 2.0, 1.0]).is_err());
    assert!(poly(vec![0.0, 1.0]).is_err());
    assert!(read_back(&testfile::C::Poly(Vec::new(), vec![0.0])).is_err());
}

/// A polycurve that claims more segments than its chunk can hold is
/// refused before anything is allocated for them.
#[test]
fn a_count_larger_than_its_chunk_is_refused() {
    let mut w = testfile::Writer::default();
    w.object(crate::geom::class::POLY_CURVE, |w| {
        w.chunk_version(1, 0);
        w.i32(i32::MAX);
        w.i32(0);
        w.i32(0);
        w.bbox();
        w.f64_array(&[0.0, 1.0]);
    });
    let mut r = archive::Reader::new(&w.buf, 70);
    let e = crate::geom::read_curve_object(&mut r).expect_err("refused");
    assert!(e.0.contains("count"), "{e}");
}

/// The block that holds an instance of itself reads as it is written:
/// its definition lists both of its objects, the instance among them.
#[test]
fn a_block_in_itself_reads() {
    let file = File3dm::read(&testfile::block_in_itself()).expect("it reads");
    assert!(file.failed.is_empty(), "{:?}", file.failed);
    assert_eq!(file.definitions.len(), 1);
    assert_eq!(file.definitions[0].id, testfile::LOOP);
    let Geometry::Instance(i) = object(&file, "loop self") else {
        panic!("an instance")
    };
    assert_eq!(i.definition, testfile::LOOP);
    assert!(
        file.definitions[0]
            .objects
            .contains(&testfile::record_id("loop self"))
    );
}

/// The sample's chunks carry the CRCs the format has: of each
/// chunk's own bytes, without the chunks nested in it.
#[test]
fn the_writer_computes_crcs_as_the_format_does() {
    let data = testfile::sample();
    let header = |at: usize| {
        let t = u32::from_le_bytes(data[at..at + 4].try_into().expect("4 bytes"));
        let v = i64::from_le_bytes(data[at + 4..at + 12].try_into().expect("8 bytes"));
        (t, v as usize)
    };
    let crc_at = |at: usize, len: usize| {
        u32::from_le_bytes(
            data[at + 12 + len - 4..at + 12 + len]
                .try_into()
                .expect("4 bytes"),
        )
    };
    let mut at = 32;
    let mut checked = 0;
    while at < data.len() {
        let (t, len) = header(at);
        if t & archive::TCODE_SHORT != 0 {
            at += 12;
            continue;
        }
        if t == archive::TCODE_SETTINGS_TABLE {
            // The units: nothing nested, the CRC of all its data.
            let (_, ulen) = header(at + 12);
            let body = &data[at + 24..at + 24 + ulen - 4];
            assert_eq!(crc_at(at + 12, ulen), archive::crc32(0, body));
            checked += 1;
        }
        if t == archive::TCODE_LAYER_TABLE {
            // A layer record holds only its object: the CRC of no bytes.
            let (rt, rlen) = header(at + 12);
            assert_eq!(rt, archive::TCODE_LAYER_RECORD);
            assert_eq!(crc_at(at + 12, rlen), 0);
            checked += 1;
        }
        at += 12 + len;
    }
    assert_eq!(checked, 2);
}

#[test]
fn a_box_comes_in_closed_with_its_faces_out() {
    let file = sample();
    let Geometry::Brep(b) = object(&file, "box") else {
        panic!("a B-rep")
    };
    assert_eq!((b.faces.len(), b.edges.len(), b.vertices.len()), (6, 12, 8));
    let c = convert::brep_body(b, file.units.mm);
    assert_closed(&c);
    // 20 by 30 by 40 mm; each face's normal away from the centre, its loop
    // counterclockwise about it.
    let center = [10.0, 15.0, 20.0];
    let body = &c.body;
    for f in &body.faces {
        let n = plane_normal(body, f).expect("a plane");
        let lp = &f.loops[0];
        let p = body.vertices[start(body, lp[0])].point;
        assert!(dot(sub(p, center), n) > 0.0, "a face facing in");
        assert!(
            loop_area(body, lp, n) > 0.0,
            "a loop clockwise about its face's normal"
        );
    }
    let xs: Vec<f64> = body.vertices.iter().map(|v| v.point[0]).collect();
    assert!(xs.iter().all(|&x| x == 0.0 || x == 20.0));
}

/// The file's own normal of a face whose outer loop runs along lines
/// (twice its area): the loop's sense by the vertices of its trims
/// (counterclockwise about the surface's normal in the format), turned
/// round where the face is reversed. It does not depend on how the
/// surface's frame or extents are read. None for a face whose outer loop
/// has a curved edge or no area.
fn straight_file_normal(b: &crate::geom::Brep, f: &crate::geom::BrepFace) -> Option<[f64; 3]> {
    use crate::geom::LoopType;
    let lp = f
        .loops
        .iter()
        .filter_map(|&l| b.loops.get(usize::try_from(l).ok()?))
        .find(|lp| lp.kind == LoopType::Outer)?;
    let mut n = [0.0; 3];
    let mut reach: f64 = 0.0;
    for &ti in &lp.trims {
        let t = b.trims.get(usize::try_from(ti).ok()?)?;
        let e = b.edges.get(usize::try_from(t.edge).ok()?)?;
        let curve = b.curves.get(usize::try_from(e.curve).ok()?)?.as_deref()?;
        if !matches!(curve, Curve::Line { .. }) {
            return None;
        }
        let p = b.vertices.get(usize::try_from(t.vertices[0]).ok()?)?.point;
        let q = b.vertices.get(usize::try_from(t.vertices[1]).ok()?)?.point;
        n = add(n, cross(p, q));
        reach = reach.max(norm(p)).max(norm(q));
    }
    if norm(n) <= 1e-9 * (1.0 + reach * reach) {
        return None;
    }
    Some(if f.reversed { scale(n, -1.0) } else { n })
}

/// A plane surface drawn facing up comes in facing up in each of the
/// forms the file may keep it in (mitcad#1738, `testfile::plane_brep`),
/// its loop counterclockwise about its normal: the face, not only its
/// surface, says which way it faces.
#[test]
fn a_plane_surface_faces_as_the_file_means() {
    let file = File3dm::read(&testfile::plane_surfaces()).expect("it reads");
    assert_eq!(file.objects.len(), 3);
    for form in 0..3 {
        let Geometry::Brep(b) = object(&file, &format!("sheet {form}")) else {
            panic!("a B-rep")
        };
        let means = straight_file_normal(b, &b.faces[0]).expect("a straight loop");
        assert!(means[2] > 0.0, "form {form}: the file's face faces down");
        let c = convert::brep_body(b, file.units.mm);
        assert!(c.issues.is_empty(), "form {form}: {:?}", c.issues);
        let body = &c.body;
        assert_eq!(body.faces.len(), 1);
        let n = plane_normal(body, &body.faces[0]).expect("a plane");
        assert!(n[2] > 0.5, "form {form}: it comes in facing down ({n:?})");
        assert!(
            loop_area(body, &body.faces[0].loops[0], n) > 0.0,
            "form {form}: its loop runs clockwise about its normal"
        );
    }
}

/// An untrimmed plane surface faces along its surface's normal: its
/// plane's normal, against it where an extent runs against its domain.
#[test]
fn an_untrimmed_plane_surface_faces_along_its_surface() {
    for (extents, up) in [
        ([[0.0, 2.0], [0.0, 3.0]], true),
        ([[2.0, 0.0], [0.0, 3.0]], false),
        ([[2.0, 0.0], [3.0, 0.0]], true),
    ] {
        let srf = crate::geom::Surface::Plane {
            plane: crate::geom::Plane {
                origin: [0.0; 3],
                x: [1.0, 0.0, 0.0],
                y: [0.0, 1.0, 0.0],
                z: [0.0, 0.0, 1.0],
            },
            domain: [[0.0, 1.0], [0.0, 1.0]],
            extents,
        };
        let c = convert::surface_body(&srf, 10.0).expect("it converts");
        let body = &c.body;
        let n = plane_normal(body, &body.faces[0]).expect("a plane");
        assert_eq!(n[2] > 0.0, up, "extents {extents:?}: {n:?}");
        assert!(
            loop_area(body, &body.faces[0].loops[0], n) > 0.0,
            "extents {extents:?}: its loop runs clockwise about its normal"
        );
        let check = body.check(1e-6);
        assert_eq!(check.vertex_mismatches, 0, "{check:?}");
    }
}

#[test]
fn a_cylinder_of_revolution_keeps_its_seam() {
    let file = sample();
    let Geometry::Brep(b) = object(&file, "cylinder") else {
        panic!("a B-rep")
    };
    let c = convert::brep_body(b, file.units.mm);
    assert!(c.issues.is_empty(), "{:?}", c.issues);
    let body = &c.body;
    assert_eq!(body.faces.len(), 3);
    // The side: the seam twice, once each way.
    let side = &body.faces[0];
    assert!(matches!(
        body.surfaces[side.surface],
        brep::Surface::Revolution { .. }
    ));
    assert!(!side.reversed);
    let seam: Vec<_> = side.loops[0].iter().filter(|c| c.edge == 2).collect();
    assert_eq!(seam.len(), 2);
    assert_ne!(seam[0].forward, seam[1].forward);
    let check = body.check(1e-6);
    assert_eq!(check.unpaired_coedges, 0, "{check:?}");
    // The caps: the bottom faces down, the top up; each loop
    // counterclockwise about its face's normal (judged by the circle's
    // direction, its edge's curve turning about z).
    for (fi, down) in [(1usize, true), (2, false)] {
        let f = &body.faces[fi];
        let n = plane_normal(body, f).expect("a plane");
        assert_eq!(n[2] < 0.0, down);
        let ce = f.loops[0][0];
        let e = &body.edges[ce.edge];
        let brep::Curve::Ellipse { normal, .. } = &body.curves[e.curve] else {
            panic!("a circle")
        };
        // The loop runs about the circle's normal when forward.
        let about = if ce.forward {
            *normal
        } else {
            scale(*normal, -1.0)
        };
        assert!(
            dot(about, n) > 0.0,
            "cap {fi} runs clockwise about its normal"
        );
    }
}

#[test]
fn an_extruded_plate_with_a_hole() {
    let file = sample();
    let Geometry::Extrusion(e) = object(&file, "plate") else {
        panic!("an extrusion")
    };
    assert_eq!(e.profile_count, 2);
    assert_eq!(e.cap, [true, true]);
    let c = convert::extrusion_body(e, file.units.mm).expect("it converts");
    assert_closed(&c);
    let body = &c.body;
    // Four sides, the hole's side and two caps.
    assert_eq!(body.faces.len(), 7);
    let center = [120.0, 15.0, 2.5];
    for f in &body.faces {
        if let Some(n) = plane_normal(body, f)
            && n[2].abs() > 0.5
        {
            // A cap: outward, its outer loop counterclockwise, its hole
            // clockwise.
            let p = body.vertices[start(body, f.loops[0][0])].point;
            assert!(dot(sub(p, center), n) > 0.0);
            assert_eq!(f.loops.len(), 2);
            assert!(loop_area(body, &f.loops[0], n) > 0.0);
        }
    }
    // The hole's side faces into the hole.
    let hole = body
        .faces
        .iter()
        .find(|f| match &body.surfaces[f.surface] {
            brep::Surface::Extrusion { curve, .. } => matches!(curve, brep::Curve::Ellipse { .. }),
            _ => false,
        })
        .expect("the hole's side");
    assert!(hole.reversed);
}

#[test]
fn a_round_rod_extrusion() {
    let file = sample();
    let Geometry::Extrusion(e) = object(&file, "rod") else {
        panic!("an extrusion")
    };
    let c = convert::extrusion_body(e, file.units.mm).expect("it converts");
    assert_closed(&c);
    assert_eq!(c.body.faces.len(), 3);
    assert!(!c.body.faces[0].reversed);
}

#[test]
fn a_compressed_mesh() {
    let file = sample();
    let Geometry::Mesh(m) = object(&file, "tetrahedron") else {
        panic!("a mesh")
    };
    assert_eq!(m.vertices.len(), 4);
    assert_eq!(m.vertices[3], [0.0, 0.0, 11.0]);
    assert_eq!(m.triangles().len(), 4);
}

#[test]
fn curves_and_instances_are_read() {
    let file = sample();
    assert!(
        matches!(object(&file, "spline"), Geometry::Curve(c) if matches!(**c, Curve::Nurbs(_)))
    );
    let Geometry::Instance(i) = object(&file, "bolt 2") else {
        panic!("an instance")
    };
    assert_eq!(i.definition, testfile::BOLT);
    assert_eq!(i.xform[0][3], 5.0);
    assert_eq!(i.xform[1][3], 20.0);
}

#[test]
fn polycurve_edges_split_into_segments() {
    // A loop of two edges: a polycurve of a line and an arc, and a line
    // back: three neutral edges.
    let line = Curve::Line {
        from: [0.0, 0.0, 0.0],
        to: [1.0, 0.0, 0.0],
        domain: [0.0, 1.0],
    };
    let arc = Curve::Arc {
        plane: crate::geom::Plane {
            origin: [1.0, 1.0, 0.0],
            x: [0.0, -1.0, 0.0],
            y: [1.0, 0.0, 0.0],
            z: [0.0, 0.0, 1.0],
        },
        radius: 1.0,
        angles: [0.0, std::f64::consts::FRAC_PI_2],
        domain: [0.0, 1.0],
    };
    let poly = Curve::Poly {
        segments: vec![line, arc],
        params: vec![0.0, 1.0, 2.0],
    };
    let mut out = Vec::new();
    convert::pieces(&poly, 0.5, 2.0, 1.0, &mut out).expect("pieces");
    assert_eq!(out.len(), 2);
    let a = out[0].curve.eval(out[0].t[0]);
    assert!((a[0] - 0.5).abs() < 1e-12 && a[1].abs() < 1e-12);
    let b = out[1].curve.eval(out[1].t[1]);
    assert!(
        (b[0] - 2.0).abs() < 1e-12 && (b[1] - 1.0).abs() < 1e-12,
        "{b:?}"
    );
}

#[test]
fn unclamped_curves_are_clamped_in_place() {
    // A periodic-style uniform cubic: the format's knots 0..=6, 5 points.
    let cvs = vec![
        [0.0, 0.0, 0.0, 1.0],
        [1.0, 2.0, 0.0, 1.0],
        [3.0, 2.0, 0.0, 1.0],
        [4.0, 0.0, 0.0, 1.0],
        [5.0, -1.0, 0.0, 1.0],
    ];
    let n = crate::geom::NurbsCurve {
        dim: 3,
        rational: false,
        order: 4,
        knots: vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0],
        cvs: cvs.clone(),
    };
    // The curve before clamping, by de Boor on the full knot vector.
    let full = Spline {
        degree: 3,
        knots: crate::nurbs::full_knots(&n.knots),
        cvs,
    };
    let clamped = Spline::from_nurbs(&n);
    assert_eq!(clamped.domain(), (2.0, 4.0));
    assert_eq!(&clamped.knots[..4], &[2.0; 4]);
    for t in [2.0, 2.3, 3.0, 3.7, 4.0] {
        let (a, b) = (full.eval(t), clamped.eval(t));
        assert!(norm(sub(a, b)) < 1e-12, "at {t}: {a:?} and {b:?}");
    }
    let b = clamped.to_brep(1.0);
    assert!(b.is_valid());
}

#[test]
fn polycurves_join_with_raised_degrees() {
    let arc = Spline::arc(
        &crate::geom::Plane {
            origin: [0.0; 3],
            x: [1.0, 0.0, 0.0],
            y: [0.0, 1.0, 0.0],
            z: [0.0, 0.0, 1.0],
        },
        2.0,
        [0.0, std::f64::consts::PI],
        [0.0, 1.0],
    );
    for t in [0.0, 0.25, 0.6, 1.0] {
        assert!((norm(arc.eval(t)) - 2.0).abs() < 1e-12);
    }
    let line = Spline::line([-2.0, 0.0, 0.0], [-2.0, -3.0, 0.0], 0.0, 3.0);
    let joined = Spline::join(vec![arc.clone(), line]).expect("joined");
    assert_eq!(joined.degree, 2);
    assert!(joined.to_brep(1.0).is_valid());
    // The arc's half still lies on the circle, the line's on the line.
    for t in [0.1, 0.5, 0.9] {
        assert!((norm(joined.eval(t)) - 2.0).abs() < 1e-12);
    }
    let p = joined.eval(2.5);
    assert!((p[0] + 2.0).abs() < 1e-12 && p[1] < 0.0);
    let elevated = arc.elevated(1);
    for t in [0.0, 0.3, 0.8] {
        assert!(norm(sub(elevated.eval(t), arc.eval(t))) < 1e-12);
    }
}

#[test]
fn uuids_print_as_text() {
    let u = archive::Uuid::parse("5EAF1119-0B51-11d4-BFFE-0010830122F0");
    assert_eq!(u.to_string(), "5EAF1119-0B51-11D4-BFFE-0010830122F0");
    assert_eq!(
        crate::geom::class::name(archive::Uuid::parse("F09BA4D9-455B-42C3-BA3B-E6CCACEF853B")),
        "SubD"
    );
}

/// The `.3dm` files of `MITCAD_3DM_CORPUS`: each reads without a failed
/// object, and each B-rep converts with every face on a surface and every
/// loop closed. Skipped without the variable.
#[test]
fn corpus() {
    let Ok(dir) = std::env::var("MITCAD_3DM_CORPUS") else {
        eprintln!("MITCAD_3DM_CORPUS not set: skipped");
        return;
    };
    // The folder and its subfolders.
    let mut files = Vec::new();
    let mut folders = vec![std::path::PathBuf::from(&dir)];
    while let Some(folder) = folders.pop() {
        for entry in std::fs::read_dir(&folder)
            .expect("the corpus folder")
            .filter_map(Result::ok)
        {
            let p = entry.path();
            if p.is_dir() {
                folders.push(p);
            } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("3dm")) {
                files.push(p);
            }
        }
    }
    files.sort();
    assert!(!files.is_empty(), "no .3dm files in {dir}");
    let mut plane_faces = 0;
    for path in files {
        let file = File3dm::open(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        eprintln!("{}: {}", path.display(), crate::summary(&file));
        assert!(
            file.failed.is_empty(),
            "{}: {:?}",
            path.display(),
            file.failed
        );
        for o in &file.objects {
            if let Geometry::Brep(b) = &o.geometry {
                let c = convert::brep_body(b, file.units.mm);
                let check = c.body.check(file.tolerance * file.units.mm * 10.0);
                eprintln!(
                    "  B-rep of {} faces: {} issues, {} skipped faces, {} unknown edges, {} singular trims; {check:?}",
                    b.faces.len(),
                    c.issues.len(),
                    c.skipped_faces,
                    c.unknown_edges,
                    c.degenerate_edges
                );
                assert_eq!(c.skipped_faces, 0, "{:?}", c.issues);
                assert_eq!(c.unknown_edges, 0, "{:?}", c.issues);
                assert_eq!(check.open_loops, 0);
                // Every face on a plane whose outer loop is straight faces
                // as the file means (mitcad#1738); with no face skipped the
                // body's faces are the file's, in order.
                for (fi, f) in b.faces.iter().enumerate() {
                    let n = c.body.faces.get(fi).and_then(|g| plane_normal(&c.body, g));
                    if let (Some(n), Some(means)) = (n, straight_file_normal(b, f)) {
                        assert!(
                            dot(n, means) > 0.0,
                            "{}: face {fi} comes in facing against the file's {means:?}",
                            path.display()
                        );
                        plane_faces += 1;
                    }
                }
            }
        }
    }
    eprintln!("{plane_faces} faces on planes face as their files mean");
}

/// A block definition of format version 8 keeps its id and name in a
/// chunk of another typecode, as flagged fields; it reads as the older
/// form does.
#[test]
fn a_version_8_block_definition_reads() {
    let file = File3dm::read(&testfile::sample_v8_block()).expect("it reads");
    assert!(file.warnings.is_empty(), "{:?}", file.warnings);
    assert_eq!(file.definitions.len(), 1);
    assert_eq!(file.definitions[0].id, testfile::BOLT);
    assert_eq!(file.definitions[0].name, "Bolt");
    assert_eq!(
        file.definitions[0].objects,
        [testfile::record_id("bolt head")]
    );
}

/// A sphere as files keep it: a half circle turned about the axis, one
/// face whose loop is its seam from pole to pole and back with a singular
/// trim at each pole. It is all of its surface: one face without loops on
/// the exact B-spline of the revolution, whose edge poles are the
/// sphere's poles and whose first column is the half circle.
#[test]
fn a_sphere_is_its_whole_surface() {
    use crate::geom::{
        Brep, BrepEdge, BrepFace, BrepLoop, BrepTrim, BrepVertex, LoopType, Plane, Surface,
        TrimType,
    };
    let pi = std::f64::consts::PI;
    let r = 2.0;
    let arc = Curve::Arc {
        plane: Plane {
            origin: [0.0; 3],
            x: [0.0, 0.0, -1.0],
            y: [1.0, 0.0, 0.0],
            z: [0.0, -1.0, 0.0],
        },
        radius: r,
        angles: [0.0, pi],
        domain: [0.0, pi * r],
    };
    let trim = |edge: i32, vertices: [i32; 2], reversed_3d: bool, kind: TrimType| BrepTrim {
        edge,
        vertices,
        reversed_3d,
        kind,
        face_loop: 0,
    };
    let b = Brep {
        curves: vec![Some(Box::new(arc.clone()))],
        surfaces: vec![Some(Box::new(Surface::Revolution {
            axis: [[0.0; 3], [0.0, 0.0, 1.0]],
            angles: [0.0, 2.0 * pi],
            transposed: false,
            curve: Box::new(arc),
        }))],
        vertices: vec![
            BrepVertex {
                point: [0.0, 0.0, -r],
                tolerance: 0.0,
            },
            BrepVertex {
                point: [0.0, 0.0, r],
                tolerance: 0.0,
            },
        ],
        edges: vec![BrepEdge {
            curve: 0,
            reversed: false,
            proxy: [0.0, pi * r],
            vertices: [0, 1],
            tolerance: 0.0,
        }],
        trims: vec![
            trim(-1, [0, 0], false, TrimType::Singular),
            trim(0, [0, 1], false, TrimType::Seam),
            trim(-1, [1, 1], false, TrimType::Singular),
            trim(0, [1, 0], true, TrimType::Seam),
        ],
        loops: vec![BrepLoop {
            trims: vec![0, 1, 2, 3],
            kind: LoopType::Outer,
            face: 0,
        }],
        faces: vec![BrepFace {
            loops: vec![0],
            surface: 0,
            reversed: false,
        }],
        is_solid: 1,
        render_meshes: Vec::new(),
    };
    let c = convert::brep_body(&b, 1.0);
    assert!(c.issues.is_empty(), "{:?}", c.issues);
    assert_eq!((c.skipped_faces, c.body.faces.len()), (0, 1));
    let face = &c.body.faces[0];
    assert!(face.loops.is_empty() && !face.reversed);
    let brep::Surface::BSpline(s) = &c.body.surfaces[face.surface] else {
        panic!("a B-spline")
    };
    assert!(s.is_valid());
    // u the angle, v along the half circle: the first and last pole of
    // each row are the poles; at u = 0 the row is the half circle's own.
    for i in 0..s.nu {
        let row = &s.poles[i * s.nv..(i + 1) * s.nv];
        assert!(norm(sub(row[0], [0.0, 0.0, -r])) < 1e-12, "{row:?}");
        assert!(norm(sub(row[s.nv - 1], [0.0, 0.0, r])) < 1e-12, "{row:?}");
    }
    let first: Vec<f64> = s.poles[..s.nv].iter().map(|p| p[1]).collect();
    assert!(first.iter().all(|y| y.abs() < 1e-12), "{first:?}");
}
