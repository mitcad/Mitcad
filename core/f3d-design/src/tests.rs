// SPDX-License-Identifier: MIT
//! Decoder tests on synthetic streams built byte by byte from the layouts
//! of the timeline format study.

use serde_json::{Value, json};

use super::classes::*;
use super::stream::{AttrValue, MetaStream};
use super::*;

const ROOT: &str = "98542EB9-A4F2-4137-A808-DBB5B3CD6159";
/// A class standing in for the point usage class (any class will do).
const USAGE: &str = "0000AAAA-0000-0000-0000-000000000001";

fn s8(s: &str) -> Vec<u8> {
    let mut v = (s.len() as u32).to_le_bytes().to_vec();
    v.extend(s.as_bytes());
    v
}

fn s16(s: &str) -> Vec<u8> {
    let units: Vec<u16> = s.encode_utf16().collect();
    let mut v = (units.len() as u32).to_le_bytes().to_vec();
    v.extend(units.iter().flat_map(|u| u.to_le_bytes()));
    v
}

fn r(id: u64) -> Vec<u8> {
    let mut v = vec![1];
    v.extend(id.to_le_bytes());
    v.extend([0, 0]);
    v
}

fn u32b(v: u32) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

fn f64s(v: &[f64]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

fn cat(parts: &[&[u8]]) -> Vec<u8> {
    parts.concat()
}

pub(crate) use super::writer::DesignWriter as Doc;

/// A timeline item tail: `i32 result_no | str16 base | u32 index | u32 0 |
/// str16 custom | u32 0 | flags | ref health`.
fn tail(
    result_no: i32,
    base: &str,
    index: u32,
    custom: &str,
    flags: [u8; 3],
    health: u64,
) -> Vec<u8> {
    cat(&[
        &result_no.to_le_bytes(),
        &s16(base),
        &u32b(index),
        &u32b(0),
        &s16(custom),
        &u32b(0),
        &flags,
        &r(health),
        &[0],
    ])
}

/// A class-version-7 parameter: `00 00 | gap0 | number | holder | expr |
/// gap1 | role | comment | unit | name | value | 00 ref list`.
#[allow(clippy::too_many_arguments)]
fn parameter(
    number: u32,
    holder: Option<u64>,
    expr: &str,
    role: &str,
    comment: &str,
    unit: &str,
    name: &str,
    value: f64,
) -> Vec<u8> {
    cat(&[
        &[0, 0],
        &[0; 10],
        &u32b(number),
        &holder.map_or(vec![0], r),
        &s16(expr),
        &[0; 9],
        &s16(role),
        &s16(comment),
        &s16(unit),
        &s16(name),
        &f64s(&[value]),
        &[0],
        &r(32),
    ])
}

/// A design with a sketch (two points, a line, a constraint and a
/// dimension on the XY plane), an extrude, parameters, two components,
/// occurrences, a body blob name and an external document.
fn sample() -> Doc {
    let mut d = Doc::default();
    for (g, p, v) in [
        (ROOT, "", 0),
        (TIMELINE, ROOT, 3),
        (FEATURE_MANAGER, ROOT, 1),
        (COMPONENT, ROOT, 1),
        (SKETCH_FEATURE, ROOT, 1),
        (EXTRUDE, ROOT, 8),
        (HEALTH, ROOT, 1),
        (PARAMETER_HOLDER, ROOT, 1),
        (PARAMETER, ROOT, 7),
        (PARAMETER_LIST, ROOT, 1),
        (SKETCH, ROOT, 18),
        (SKETCH_TRANSFORM, ROOT, 1),
        (SKETCH_POINT, ROOT, 11),
        (USAGE, ROOT, 1),
        (SKETCH_CURVE, ROOT, 1),
        (SKETCH_LINE, SKETCH_CURVE, 2),
        (SKETCH_CONSTRAINT, ROOT, 1),
        (SKETCH_DIMENSION, ROOT, 1),
        (LINEAR_DIMENSION, SKETCH_DIMENSION, 1),
        (ENTITY_REF, ROOT, 1),
        (REF_TARGET, ROOT, 1),
        (ORIGIN_PLANE, ROOT, 1),
        (PROFILE_SOURCE, ROOT, 1),
        (PROFILE, ROOT, 1),
        (PROFILE_ID, ROOT, 1),
        (OCCURRENCE, ROOT, 1),
        (OCCURRENCE_CONTAINER, ROOT, 1),
        (BREP_REF, ROOT, 1),
        (BLOB_HOLDER, ROOT, 1),
    ] {
        d.class(g, p, v);
    }
    d.obj(1, FEATURE_MANAGER, cat(&[&[0, 0], &r(2)]));
    d.obj(2, COMPONENT, cat(&[&[0, 0], &s16("0_2"), &s16("Bracket")]));
    d.obj(
        3,
        TIMELINE,
        cat(&[&[0, 0], &r(1), &u32b(2), &r(10), &r(20)]),
    );
    d.obj(5, COMPONENT, cat(&[&[0, 0], &s16("Pin")]));
    d.obj(
        10,
        SKETCH_FEATURE,
        cat(&[&[0, 0], &r(60), &tail(-1, "Sketch", 1, "", [0, 0, 0], 11)]),
    );
    d.obj(11, HEALTH, vec![0, 0]);
    d.obj(
        20,
        EXTRUDE,
        cat(&[
            &[0, 0],
            &[0, 0, 0],
            &u32b(4),
            &u32b(1),
            &u32b(2),
            &[0, 1],
            &u32b(0),
            &f64s(&[0.0, 0.0, 1.0]),
            &[0, 0],
            &r(70),
            &r(71),
            &r(30),
            &tail(3, "Extrude", 1, "Boss", [0, 1, 0], 21),
        ]),
    );
    d.obj(21, HEALTH, vec![0, 0]);
    d.obj(30, PARAMETER_HOLDER, cat(&[&[1], &u32b(1), &r(20), &[0]]));
    let p = parameter(1, Some(30), "10 mm", "AlongDistance", "", "mm", "d1", 1.0);
    d.obj(31, PARAMETER, p);
    d.obj(32, PARAMETER_LIST, vec![0, 0]);
    let p = parameter(
        2,
        None,
        "d1 * 2",
        "User Parameter",
        "note",
        "mm",
        "width",
        2.0,
    );
    d.obj(33, PARAMETER, p);
    let p = parameter(4, Some(30), "0 deg", "TaperAngle", "", "deg", "d3", 0.0);
    d.obj(34, PARAMETER, p);
    d.obj_sub(
        40,
        SKETCH,
        cat(&[
            &[0, 0],
            &[1],
            &u32b(1),
            &r(54),
            &[1],
            &u32b(1),
            &r(55),
            &r(10),
        ]),
        cat(&[&r(41), &u32b(3), &r(50), &r(51), &r(52)]),
    );
    let m = [
        1.0, 0.0, 0.0, 1.0, 0.0, 1.0, 0.0, 2.0, 0.0, 0.0, 1.0, 3.0, 0.0, 0.0, 0.0, 1.0,
    ];
    d.obj(
        41,
        SKETCH_TRANSFORM,
        cat(&[&[0, 0], &[0], &f64s(&m), &[0], &r(2)]),
    );
    for (id, x) in [(50, 0.0), (51, 2.0)] {
        d.obj(
            id,
            SKETCH_POINT,
            cat(&[
                &[0, 1],
                &u32b(1),
                &s8("pt_tag"),
                &s8("IntrinsicMetaTypeuint64"),
                &7u64.to_le_bytes(),
                &r(53),
                &[0, 0, 0, 0, 1, 0, 1, 0],
                &f64s(&[x, 0.0, 0.0]),
                &r(40),
            ]),
        );
    }
    d.obj(
        52,
        SKETCH_LINE,
        cat(&[
            &[0, 0],
            &f64s(&[0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, -1.0]),
            &r(51),
            &r(50),
            &r(40),
        ]),
    );
    d.obj(53, USAGE, vec![0, 0]);
    d.obj(
        54,
        SKETCH_CONSTRAINT,
        cat(&[
            &[0, 0],
            &r(40),
            &0x40u64.to_le_bytes(),
            &u32b(1),
            &r(52),
            &[0],
        ]),
    );
    let mut flags = vec![0u8; 21];
    flags[15] = 0x40;
    d.obj(
        55,
        LINEAR_DIMENSION,
        cat(&[
            &[0, 0],
            &s16("10 mm"),
            &f64s(&[1.0, 0.5, 0.0]),
            &flags,
            &r(56),
            &u32b(2),
            &r(50),
            &r(51),
            &[0],
        ]),
    );
    d.obj(56, PARAMETER_HOLDER, cat(&[&[1], &u32b(1), &r(10), &[0]]));
    let p = parameter(
        3,
        Some(56),
        "10 mm",
        "Linear Dimension-1",
        "",
        "mm",
        "d2",
        1.0,
    );
    d.obj(57, PARAMETER, p);
    d.obj(60, ENTITY_REF, cat(&[&[0, 0], &r(61)]));
    d.obj(61, REF_TARGET, cat(&[&[0, 0], &62u64.to_le_bytes()]));
    d.obj(
        62,
        ORIGIN_PLANE,
        cat(&[
            &[0, 0],
            &s16("XY"),
            &f64s(&[-5.0, -5.0]),
            &[0],
            &f64s(&[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 5.0, 5.0]),
        ]),
    );
    d.obj(70, PROFILE_SOURCE, cat(&[&[0, 0], &s16("40")]));
    d.obj(
        71,
        PROFILE,
        cat(&[&[0, 0], &r(20), &u32b(2), &r(72), &r(73)]),
    );
    d.obj(72, PROFILE_ID, vec![0, 0]);
    d.obj(73, PROFILE_ID, vec![0, 0]);
    let t = [
        1.0, 0.0, 0.0, 10.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ];
    d.obj(
        80,
        OCCURRENCE,
        cat(&[&[0, 0], &r(5), &[0], &f64s(&t), &u32b(0), &r(81)]),
    );
    d.obj(81, OCCURRENCE_CONTAINER, cat(&[&[0, 0], &r(80), &r(2)]));
    d.obj(82, OCCURRENCE, cat(&[&[0, 0], &r(2), &[1]]));
    d.obj(90, BREP_REF, cat(&[&[0, 0], &s16("BREP.abc.smb")]));
    d.obj(91, BLOB_HOLDER, cat(&[&[0, 0], &r(90), &r(2)]));
    d.external
        .push((0x1122_3344_5566_7788, "urn:test:doc?version=1".into()));
    d
}

#[test]
fn meta_stream_forms() {
    let mut d = sample();
    let (m, _) = d.streams();
    let meta = MetaStream::parse(&m).unwrap();
    assert!(!meta.is_form_b());
    assert_eq!(meta.writer(), "2.0.20981");
    assert_eq!(meta.writer_build(), Some(20981));
    assert_eq!(meta.classes.len(), 29);
    assert_eq!(meta.classes[15].parent, SKETCH_CURVE);
    assert_eq!(meta.index.len(), d.objects.len());
    assert_eq!(meta.subchunks.len(), 1);
    assert_eq!(meta.next_id, Some(92));
    assert_eq!(meta.external[0].urn, "urn:test:doc?version=1");
    assert!(meta.tail_ok);

    d.form_b = true;
    let (m, _) = d.streams();
    let meta = MetaStream::parse(&m).unwrap();
    assert!(meta.is_form_b());
    assert_eq!(meta.writer(), "2704.1.36");
    assert_eq!(meta.writer_build(), None);
    assert_eq!(
        meta.feature_versions,
        vec![("Application".into(), 2), ("Server".into(), 6)]
    );
    assert!(meta.tail_ok);

    // Truncated class table: an error, not a panic.
    assert!(MetaStream::parse(&m[..60]).is_err());
}

#[test]
fn objects_classes_and_references() {
    let seg = sample().segment();
    assert!(!seg.long_refs);
    // Every object's tag is 256 + its class index, its id the index id.
    for o in &seg.objects {
        let h = seg.header(o).unwrap();
        assert_eq!(
            (h.class_index(), h.id, h.name.as_str()),
            (o.class, o.id, "")
        );
    }
    assert_eq!(seg.guid_of(52), Some(SKETCH_LINE));
    assert_eq!(
        seg.chain(SKETCH_LINE),
        vec![SKETCH_LINE, SKETCH_CURVE, ROOT]
    );
    assert!(seg.is_kind_of(52, SKETCH_CURVE));
    assert!(!seg.is_kind_of(50, SKETCH_CURVE));
    let timeline = seg.objects_of(TIMELINE).next().unwrap();
    let d = seg.data(timeline);
    let ids = seg.ref_ids(d);
    assert_eq!(ids, vec![1, 10, 20]);
    // A reference to an unknown id is not a reference.
    assert!(seg.ref_at(&r(999), 0).is_none());

    // Root part with an attribute.
    let pt = seg.object(50).unwrap();
    let root = seg.root_part(seg.data(pt)).unwrap();
    assert_eq!(root.refs, vec![]);
    assert_eq!(root.attrs[0].key, "pt_tag");
    assert_eq!(root.attrs[0].value, AttrValue::U64(7));

    // Sub-chunk: main part ends where it starts.
    let s = seg.object(40).unwrap();
    assert_eq!(seg.main_end(s), seg.sub_start(s));
    assert!(seg.sub_start(s) > 0);
}

#[test]
fn long_and_external_references() {
    let mut seg = sample().segment();
    // Through an assembly context: 01 id 01 key 00 str8(guid) 00.
    let mut ext = vec![1];
    ext.extend(5u64.to_le_bytes());
    ext.push(1);
    ext.extend(0xabcdu64.to_le_bytes());
    ext.push(0);
    ext.extend(s8(COMPONENT));
    ext.push(0);
    let er = seg.ref_at(&ext, 0).unwrap();
    assert_eq!((er.id, er.context, er.end), (5, Some(0xabcd), ext.len()));
    // The GUID must name the object's class.
    let mut bad = ext.clone();
    bad[23] = b'0';
    assert!(seg.ref_at(&bad, 0).is_none());

    // Long form (writer 2.0.5119): 01 id str8(guid) 00 00.
    seg.long_refs = true;
    let mut long = vec![1];
    long.extend(2u64.to_le_bytes());
    long.extend(s8(&COMPONENT.to_lowercase()));
    long.extend([0, 0]);
    let lr = seg.ref_at(&long, 0).unwrap();
    assert_eq!((lr.id, lr.end), (2, long.len()));
    assert!(seg.ref_at(&cat(&[&r(2)]), 0).is_none());
}

#[test]
fn parameters_timeline_and_tails() {
    let seg = sample().segment();
    let dec = decode::decode(&seg);
    assert_eq!(dec.parameter_failures, 0);
    let names: Vec<&str> = dec.parameters.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["d1", "width", "d2", "d3"]);
    let d1 = &dec.parameters[0];
    assert_eq!(
        (
            d1.number,
            d1.holder,
            d1.expression.as_str(),
            d1.role.as_str()
        ),
        (1, Some(30), "10 mm", "AlongDistance")
    );
    assert_eq!((d1.unit.as_str(), d1.value, d1.list), ("mm", 1.0, Some(32)));
    assert_eq!(d1.comment.as_deref(), Some(""));
    assert_eq!(d1.owner_feature, Some(20));
    assert_eq!(d1.gap0.len(), 10);
    let width = &dec.parameters[1];
    assert!(width.is_user());
    assert_eq!(width.comment.as_deref(), Some("note"));
    assert_eq!(dec.parameters[2].owner_feature, Some(10));

    assert_eq!(dec.timelines_total, 1);
    assert_eq!(dec.timeline.len(), 2);
    let sk = &dec.timeline[0];
    assert_eq!(sk.name.as_deref(), Some("Sketch1"));
    assert_eq!(sk.type_name, Some("SketchFeature"));
    let ex = &dec.timeline[1];
    assert_eq!(ex.name.as_deref(), Some("Boss"));
    let t = ex.tail.as_ref().unwrap();
    assert_eq!((t.result_no, t.index, t.flags), (3, 1, [0, 1, 0]));
    let f = ex.extrude.unwrap();
    assert_eq!(
        (f.operation(), f.extent_a, f.extent_b),
        (Some("NewBody"), 1, 2)
    );
    assert_eq!(f.direction, Some(1.0));
    assert_eq!(f.vector, Some([0.0, 0.0, 1.0]));

    assert_eq!(dec.components.len(), 2);
    assert_eq!(dec.components[0].name.as_deref(), Some("Bracket"));
    let blob = &dec.brep_blobs[0];
    assert_eq!((blob.id, blob.file.as_str()), (90, "BREP.abc.smb"));
    assert_eq!((blob.holder, blob.component), (Some(91), Some(2)));
    let cv = dec.coverage;
    assert_eq!(cv.objects, seg.objects.len() as u64);
    assert_eq!(cv.root_part_ok, cv.objects);
    assert!(cv.decoded_bytes > 0 && cv.token_bytes > cv.framing_bytes);
}

/// A class-version-8 parameter (2026 writers): `00 00 | 00 | u32 6 | 00 |
/// str16 comment | number | holder | expr | 10 flag bytes | role | "" |
/// unit | name | value | 00 ref list`.
#[allow(clippy::too_many_arguments)]
fn parameter_v8(
    number: u32,
    holder: Option<u64>,
    expr: &str,
    role: &str,
    comment: &str,
    unit: &str,
    name: &str,
    value: f64,
) -> Vec<u8> {
    let user = u8::from(holder.is_none());
    cat(&[
        &[0, 0],
        &[0],
        &u32b(6),
        &[0],
        &s16(comment),
        &u32b(number),
        &holder.map_or(vec![0], r),
        &s16(expr),
        &[0, 0, 0, 0, 0, 0, 0, 0, 0, user],
        &s16(role),
        &s16(""),
        &s16(unit),
        &s16(name),
        &f64s(&[value]),
        &[0],
        &r(32),
    ])
}

#[test]
fn parameters_of_class_version_8() {
    let mut d = Doc::default();
    d.class(ROOT, "", 0);
    d.class(PARAMETER, ROOT, 8);
    d.class(PARAMETER_LIST, ROOT, 4);
    d.class(PARAMETER_HOLDER, ROOT, 0);
    d.obj(30, PARAMETER_HOLDER, vec![0, 0]);
    d.obj(32, PARAMETER_LIST, vec![0, 0]);
    let p = parameter_v8(1, Some(30), "5 mm", "Radius", "", "mm", "d1", 0.5);
    d.obj(40, PARAMETER, p);
    let p = parameter_v8(
        2,
        None,
        "60 mm",
        "User Parameter",
        "block width",
        "mm",
        "width",
        6.0,
    );
    d.obj(41, PARAMETER, p);
    // A driven dimension's: the flag 4 bytes before the role.
    let expr = "1.500001 mm";
    let mut p = parameter_v8(
        3,
        Some(30),
        expr,
        "Linear Dimension-2",
        "",
        "mm",
        "d3",
        0.15,
    );
    let at = 2 + 6 + 4 + 4 + 11 + 4 + 2 * expr.len() + 6;
    p[at] = 1;
    d.obj(42, PARAMETER, p);
    let dec = decode::decode(&d.segment());
    let driven: Vec<bool> = dec.parameters.iter().map(|p| p.is_driven()).collect();
    assert_eq!(driven, [false, false, true]);
    let dec = decode::Decoded {
        parameters: dec.parameters[..2].to_vec(),
        ..dec
    };
    assert_eq!(dec.parameter_failures, 0);
    let p: Vec<_> = dec
        .parameters
        .iter()
        .map(|p| {
            (
                p.number,
                p.holder,
                p.expression.as_str(),
                p.role.as_str(),
                p.comment.as_deref(),
                p.name.as_str(),
                p.value,
                p.list,
            )
        })
        .collect();
    assert_eq!(
        p,
        [
            (1, Some(30), "5 mm", "Radius", Some(""), "d1", 0.5, Some(32)),
            (
                2,
                None,
                "60 mm",
                "User Parameter",
                Some("block width"),
                "width",
                6.0,
                Some(32)
            ),
        ]
    );
    assert!(dec.parameters[1].is_user());
}

/// A class-version-9 parameter (writer 2705) has one more `u32 0` in front of
/// the `u32 6` of the version 8 layout.
#[test]
fn parameters_of_class_version_9() {
    let mut d = Doc::default();
    d.class(ROOT, "", 0);
    d.class(PARAMETER, ROOT, 9);
    d.class(PARAMETER_LIST, ROOT, 4);
    d.class(PARAMETER_HOLDER, ROOT, 0);
    d.obj(30, PARAMETER_HOLDER, vec![0, 0]);
    d.obj(32, PARAMETER_LIST, vec![0, 0]);
    let mut p = parameter_v8(1, Some(30), "5 mm", "Radius", "", "mm", "d1", 0.5);
    // After the two root bytes and the byte before the `u32 6`.
    p.splice(3..3, [0, 0, 0, 0]);
    d.obj(40, PARAMETER, p);
    let dec = decode::decode(&d.segment());
    assert_eq!(dec.parameter_failures, 0);
    assert_eq!(dec.parameters.len(), 1);
    let q = &dec.parameters[0];
    assert_eq!(
        (q.number, q.expression.as_str(), q.name.as_str(), q.value),
        (1, "5 mm", "d1", 0.5)
    );
}

/// A class-version-10 parameter (writer 2706) has the version 9 layout with
/// `u32 5` in place of the `u32 6` and one more byte in the flags after the
/// expression, the user flag still the last of them.
#[test]
fn parameters_of_class_version_10() {
    let mut d = Doc::default();
    d.class(ROOT, "", 0);
    d.class(PARAMETER, ROOT, 10);
    d.class(PARAMETER_LIST, ROOT, 4);
    d.class(PARAMETER_HOLDER, ROOT, 0);
    d.obj(30, PARAMETER_HOLDER, vec![0, 0]);
    d.obj(32, PARAMETER_LIST, vec![0, 0]);
    let v10 = |number, holder, expr: &str, role, name, value| {
        let mut p = parameter_v8(number, holder, expr, role, "", "mm", name, value);
        p.splice(3..3, [0, 0, 0, 0]);
        p[7] = 5;
        // The flags start after the expression.
        let at = 3 + 4 + 4 + 1 + 4 + 4 + holder.map_or(1, |_| 11) + 4 + 2 * expr.len();
        p.insert(at, 0);
        p
    };
    d.obj(
        40,
        PARAMETER,
        v10(1, Some(30), "Bore", "Diameter Dimension-2", "d1", 4.5),
    );
    d.obj(
        41,
        PARAMETER,
        v10(2, None, "64 mm", "User Parameter", "Height", 6.4),
    );
    let dec = decode::decode(&d.segment());
    assert_eq!(dec.parameter_failures, 0);
    let p: Vec<_> = dec
        .parameters
        .iter()
        .map(|p| {
            (
                p.number,
                p.holder,
                p.expression.as_str(),
                p.role.as_str(),
                p.name.as_str(),
                p.value,
                p.list,
                p.gap1.len(),
            )
        })
        .collect();
    assert_eq!(
        p,
        [
            (
                1,
                Some(30),
                "Bore",
                "Diameter Dimension-2",
                "d1",
                4.5,
                Some(32),
                11
            ),
            (
                2,
                None,
                "64 mm",
                "User Parameter",
                "Height",
                6.4,
                Some(32),
                11
            ),
        ]
    );
    assert!(dec.parameters[1].is_user());
    assert_eq!(dec.parameters[1].gap1[10], 1);
}

/// A hole whose sub-items list the hole itself (`u32 1 | ref self`): the
/// zeros before that reference, read from 20 bytes later, look like a tail
/// with an empty base name, the index 256 and no sub-items, whose `u32`
/// after the index (zero on every item) is the reference's first bytes.
#[test]
fn a_tail_listing_the_item_itself_keeps_its_name() {
    const HOLE: &str = "1C037A07-4A15-43F6-ABFC-BBF61B9038D4";
    let mut d = Doc::default();
    for (g, p, v) in [
        (ROOT, "", 0),
        (TIMELINE, ROOT, 3),
        (FEATURE_MANAGER, ROOT, 1),
        (HOLE, ROOT, 7),
        (HEALTH, ROOT, 1),
    ] {
        d.class(g, p, v);
    }
    d.obj(1, FEATURE_MANAGER, vec![0, 0]);
    d.obj(3, TIMELINE, cat(&[&[0, 0], &r(1), &u32b(1), &r(600)]));
    d.obj(
        600,
        HOLE,
        cat(&[
            &[0, 0],
            &8i32.to_le_bytes(),
            &s16("Hole"),
            &u32b(1),
            &u32b(0),
            &s16(""),
            &u32b(1),
            &r(600),
            &[0, 0, 0],
            &r(601),
            &[0],
        ]),
    );
    d.obj(601, HEALTH, vec![0, 0]);
    let dec = decode::decode(&d.segment());
    let item = &dec.timeline[0];
    let tail = item.tail.as_ref().unwrap();
    assert_eq!(item.name.as_deref(), Some("Hole1"));
    assert_eq!((tail.result_no, tail.index, tail.after_index), (8, 1, 0));
    assert_eq!(tail.sub_features, [Some(600)]);
}

/// A tapped hole's modelled thread is a sub-item with a result number of
/// its own after the hole's: the hole's last operation (mitcad#719). A
/// cosmetic thread's is not taken.
#[test]
fn a_modelled_thread_of_a_hole_is_its_last_operation() {
    const HOLE: &str = "1C037A07-4A15-43F6-ABFC-BBF61B9038D4";
    // A thread sub-item (class version 5) after its root part, then its tail.
    let thread = |modeled: u8| {
        cat(&[
            &[0, 0],
            &f64s(&[60.0]),
            &[0],
            &u32b(0),
            &s16("6H"),
            &s16("M10x1.5"),
            &s16("M10x1.5"),
            &s16("10.0"),
            &s16("ISO Metric profile"),
            &[1],
            &u32b(2),
            &f64s(&[1.0198, 0.8526]),
            &[modeled],
            &f64s(&[0.15, 0.9116]),
            &r(600),
            &r(653),
            &r(654),
            &[1],
            &[0; 4],
            &tail(7, "Thread", 1, "", [0, 0, 0], 652),
        ])
    };
    let mut results = Vec::new();
    for modeled in [1, 0] {
        let mut d = sample();
        d.class(HOLE, ROOT, 7);
        d.class(THREAD, ROOT, 5);
        d.objects.retain(|o| o.0 != 3);
        d.obj(
            3,
            TIMELINE,
            cat(&[&[0, 0], &r(1), &u32b(3), &r(10), &r(20), &r(600)]),
        );
        d.obj(
            600,
            HOLE,
            cat(&[
                &[0, 0],
                &6i32.to_le_bytes(),
                &s16("Hole"),
                &u32b(1),
                &u32b(0),
                &s16(""),
                &u32b(2),
                &r(600),
                &r(650),
                &[0, 0, 0],
                &r(601),
                &[0],
            ]),
        );
        d.obj(601, HEALTH, vec![0, 0]);
        d.obj(650, THREAD, thread(modeled));
        d.obj(652, HEALTH, vec![0, 0]);
        let (m, b) = d.streams();
        let v = serde_json::to_value(Design::parse(&m, b).unwrap().dump("x", "Design1")).unwrap();
        let hole = &v["timeline"]["items"][2];
        assert_eq!(hole["detail"]["thread"]["isModeled"], modeled == 1);
        results.push((
            hole["_f3d"]["result_no"].clone(),
            hole["_f3d"]["last_result_no"].clone(),
        ));
    }
    assert_eq!(results, [(json!(6), json!(7)), (json!(6), Value::Null)]);
}

/// A taper pipe thread (mitcad#733), as a hole's `1/2 - 14 NPT` and `Rc
/// 1/2` threads store it: the angle 0, the tap drill as both diameters, the
/// pitch diameter 0 and the taper's five numbers after the handedness byte.
/// The hole's tail lists no sub-items: the thread names the hole. Its
/// cosmetic operations taper the bore, so they end the hole's state.
#[test]
fn a_taper_pipe_thread_names_its_hole_and_its_taper() {
    const HOLE: &str = "1C037A07-4A15-43F6-ABFC-BBF61B9038D4";
    let taper = [0.062_479_7, 0.145_142_9, 1.747_52, 1.357_12, 1.892_40];
    let thread = cat(&[
        &[0, 0],
        &f64s(&[0.0]),
        &[0],
        &u32b(0),
        &s16(""),
        &s16(""),
        &s16("1/2 - 14 NPT"),
        &s16("1/2"),
        &s16("NPT"),
        &[1],
        &u32b(2),
        &f64s(&[1.747_52, 1.747_52]),
        &[0],
        &f64s(&[2.54 / 14.0, 0.0]),
        &r(600),
        &r(653),
        &r(654),
        &[1],
        &f64s(&taper),
        &u32b(1),
        &tail(7, "Thread", 1, "", [0, 0, 0], 652),
    ]);
    let mut d = sample();
    d.class(HOLE, ROOT, 7);
    d.class(THREAD, ROOT, 5);
    d.objects.retain(|o| o.0 != 3);
    d.obj(
        3,
        TIMELINE,
        cat(&[&[0, 0], &r(1), &u32b(3), &r(10), &r(20), &r(600)]),
    );
    d.obj(
        600,
        HOLE,
        cat(&[
            &[0, 0],
            &6i32.to_le_bytes(),
            &s16("Hole"),
            &u32b(1),
            &u32b(0),
            &s16(""),
            &u32b(0),
            &[0, 0, 0],
            &r(601),
            &[0],
        ]),
    );
    d.obj(601, HEALTH, vec![0, 0]);
    d.obj(650, THREAD, thread);
    d.obj(652, HEALTH, vec![0, 0]);
    // (The two references before the handedness byte.)
    d.obj(653, HEALTH, vec![0, 0]);
    d.obj(654, HEALTH, vec![0, 0]);
    let (m, b) = d.streams();
    let v = serde_json::to_value(Design::parse(&m, b).unwrap().dump("x", "Design1")).unwrap();
    let hole = &v["timeline"]["items"][2];
    let info = &hole["detail"]["tappedHoleInfo"];
    assert_eq!(hole["detail"]["holeTapType"], "TappedHoleTapType", "{hole}");
    assert_eq!(info["threadDesignation"], "1/2 - 14 NPT");
    assert_eq!(info["threadType"], "NPT");
    assert_eq!(info["isTapered"], true);
    assert_eq!(info["isRightHanded"], true);
    let numbers = &info["_f3d_taper"];
    for (key, want) in [
        "angle",
        "height",
        "tapDrill",
        "length",
        "smallPitchDiameter",
    ]
    .into_iter()
    .zip(taper)
    {
        assert_eq!(numbers[key], want, "{key}");
    }
    assert_eq!(hole["detail"]["thread"]["isModeled"], false);
    assert_eq!(hole["_f3d"]["last_result_no"], 7);
}

/// A text parameter keeps a default expression with a unit (`1 mm`) in the
/// string after its role: it is no comment, and the parameter has no number.
#[test]
fn text_parameters_have_no_comment_from_the_default_expression() {
    let mut d = Doc::default();
    d.class(ROOT, "", 0);
    d.class(PARAMETER, ROOT, 8);
    d.class(PARAMETER_LIST, ROOT, 4);
    d.class(PARAMETER_HOLDER, ROOT, 0);
    d.obj(30, PARAMETER_HOLDER, vec![0, 0]);
    d.obj(32, PARAMETER_LIST, vec![0, 0]);
    let mut p = parameter_v8(1, Some(30), "text", "Text1-Text", "note", "Text", "d1", 0.0);
    // The string after the role: `1 mm` instead of the empty one.
    let at = p
        .windows(s16("Text1-Text").len())
        .position(|w| w == s16("Text1-Text"))
        .unwrap()
        + s16("Text1-Text").len();
    p.splice(at..at + 4, s16("1 mm"));
    d.obj(40, PARAMETER, p);
    let dec = decode::decode(&d.segment());
    assert_eq!(dec.parameter_failures, 0);
    let p = &dec.parameters[0];
    assert_eq!((p.unit.as_str(), p.name.as_str()), ("Text", "d1"));
    assert_eq!(p.comment.as_deref(), Some("note"));
    // Class version 7: the comment place holds the same string.
    let mut d = Doc::default();
    d.class(ROOT, "", 0);
    d.class(PARAMETER, ROOT, 7);
    d.class(PARAMETER_LIST, ROOT, 1);
    d.class(PARAMETER_HOLDER, ROOT, 0);
    d.obj(30, PARAMETER_HOLDER, vec![0, 0]);
    d.obj(32, PARAMETER_LIST, vec![0, 0]);
    d.obj(
        40,
        PARAMETER,
        parameter(1, Some(30), "text", "Text1-Text", "1 mm", "Text", "d1", 0.0),
    );
    let dec = decode::decode(&d.segment());
    assert_eq!(dec.parameter_failures, 0);
    assert_eq!(dec.parameters[0].comment, None);
}

/// A sketch's own name (`ref anchor | str16 name | 0 | u32 n | ref[n]`) is
/// the item's name when the user gave one, and its absence means the default
/// name even where the item's tail still holds a custom one.
#[test]
fn sketch_items_are_named_by_their_sketch() {
    for (name, custom, want) in [
        ("Plate", "", "Plate"),
        ("Plate", "stale", "Plate"),
        ("", "stale", "Sketch1"),
        ("", "", "Sketch1"),
    ] {
        let mut d = Doc::default();
        for (g, v) in [
            (ROOT, 0),
            (TIMELINE, 3),
            (FEATURE_MANAGER, 1),
            (SKETCH_FEATURE, 1),
            (HEALTH, 1),
            (SKETCH, 18),
            (SKETCH_NAME_ANCHOR, 1),
            (USAGE, 1),
        ] {
            d.class(g, if g == ROOT { "" } else { ROOT }, v);
        }
        d.obj(1, FEATURE_MANAGER, cat(&[&[0, 0], &r(2)]));
        d.obj(3, TIMELINE, cat(&[&[0, 0], &r(1), &u32b(1), &r(10)]));
        d.obj(
            10,
            SKETCH_FEATURE,
            cat(&[
                &[0, 0],
                &r(60),
                &tail(-1, "Sketch", 1, custom, [0, 0, 0], 11),
            ]),
        );
        d.obj(11, HEALTH, vec![0, 0]);
        d.obj(50, USAGE, vec![0, 0]);
        d.obj(51, USAGE, vec![0, 0]);
        d.obj(60, SKETCH_NAME_ANCHOR, vec![0, 0]);
        d.obj(
            40,
            SKETCH,
            cat(&[
                &[0, 0],
                &r(10),
                &[0, 0, 0, 0, 0, 0, 1],
                &r(60),
                &s16(name),
                &[0],
                &u32b(2),
                &r(50),
                &r(51),
            ]),
        );
        let seg = d.segment();
        let sk = seg.object(40).unwrap();
        assert_eq!(sketch::sketch_name(&seg, sk).as_deref(), Some(name));
        let dec = decode::decode(&seg);
        assert_eq!(
            dec.timeline[0].name.as_deref(),
            Some(want),
            "{name:?} {custom:?}"
        );
    }
}

/// A component without a name of its own: the text before it is the
/// material's.
#[test]
fn component_name_is_the_text_after_the_last_identifier() {
    let guid = "2F4C1849-1A5A-4F6C-A086-8DD445CBF94B";
    let object = |name: &str| {
        cat(&[
            &[0, 0, 0],
            &s16(guid),
            &s16(guid),
            &u32b(0),
            &s16("PrismMaterial-018"),
            &[1, 1, 5, 0, 0, 0, 0, 0, 0, 0, 0],
            &s16(guid),
            &s16(name),
            &[0, 1, 9],
        ])
    };
    assert_eq!(
        decode::component_name(&object("Bracket")).as_deref(),
        Some("Bracket")
    );
    assert_eq!(decode::component_name(&object("")), None);
}

/// A curve's flags (mitcad#33, mitcad#186) in a design whose sketch-curve
/// class has version 9: fixed, linked and the kind before the block, which
/// can hold references (here the offset chain `32`).
#[test]
fn curve_flags_in_a_design() {
    let mut doc = Doc::default();
    doc.class(ROOT, "", 0);
    doc.class(SKETCH_CURVE, ROOT, 9);
    doc.class(OFFSET_CHAIN, ROOT, 0);
    doc.obj(32, OFFSET_CHAIN, vec![0, 0]);
    let seg = doc.segment();
    let curve = |kind: u8, fixed: u8, linked: u8, refs: &[u64]| {
        let mut d = vec![7u8; 30];
        d.extend([fixed, 1, linked, 0, 0, kind, 0, 0, 0]);
        d.push(1);
        d.extend(u32b(refs.len() as u32));
        for &id in refs {
            d.extend(r(id));
        }
        d.extend([0; 13]);
        d.extend([0, 0, 0x80, 0x3f, 0, 0, 0x80, 0x3f]);
        d.extend([0; 5]);
        d
    };
    let flags = |d: &[u8]| sketch::curve_flags(&seg, d).map(|f| (f.kind, f.fixed, f.linked));
    assert_eq!(flags(&curve(0, 0, 0, &[])), Some((0, false, false)));
    assert_eq!(flags(&curve(1, 0, 0, &[])), Some((1, false, false)));
    assert_eq!(flags(&curve(2, 1, 0, &[])), Some((2, true, false)));
    assert_eq!(flags(&curve(0, 0, 1, &[])), Some((0, false, true)));
    // The block's references move nothing that is read from its start.
    let with_chain = curve(1, 1, 1, &[32]);
    assert_eq!(flags(&with_chain), Some((1, true, true)));
    assert_eq!(
        sketch::curve_flags(&seg, &with_chain).map(|f| f.refs),
        Some(vec![32])
    );
    // Values that are no flags, and no sketch-curve part at all.
    assert_eq!(flags(&curve(9, 0, 0, &[])), None);
    assert_eq!(flags(&curve(0, 2, 0, &[])), None);
    assert_eq!(flags(&[0; 40]), None);
}
#[test]
fn sketch_geometry() {
    let seg = sample().segment();
    let pts = sketch::points(&seg);
    assert_eq!(pts[&51].xyz, [2.0, 0.0, 0.0]);
    assert_eq!(pts[&51].flags, [0, 0, 0, 0, 1, 0, 1, 0]);
    let ln = sketch::lines(&seg)[&52];
    assert_eq!((ln.start_point, ln.end_point), (Some(50), Some(51)));
    assert_eq!(ln.end(), [2.0, 0.0, 0.0]);
    assert_eq!(ln.normal, Some([0.0, 0.0, -1.0]));
    let s = seg.object(40).unwrap();
    assert_eq!(sketch::entity_list(&seg, s), vec![50, 51, 52]);
    assert_eq!(sketch::main_lists(&seg, s), (vec![54], vec![55]));
    let c = sketch::parse_constraint(&seg, seg.object(54).unwrap()).unwrap();
    assert_eq!((c.sketch, c.mask, c.entities), (40, 0x40, vec![52]));
    let dm = sketch::parse_dimension(&seg, seg.object(55).unwrap()).unwrap();
    assert_eq!((dm.holder, dm.entities.clone()), (Some(56), vec![50, 51]));
    assert_eq!(dm.expression.as_deref(), Some("10 mm"));
    assert_eq!(dm.text, Some([1.0, 0.5, 0.0]));
    assert_eq!(dm.flags.unwrap().len(), 21);
}

/// A sketch spline's fit-point and handle lists (mitcad#186): the points
/// between its ends, the handle lines, each list closing with the
/// reference back to the spline; a spline without them has none.
#[test]
fn spline_fit_point_and_handle_lists() {
    let mut doc = sample();
    doc.class(SKETCH_SPLINE, SKETCH_CURVE, 3);
    doc.class(SPLINE_FIT_POINTS, ROOT, 1);
    doc.class(SPLINE_HANDLES, ROOT, 0);
    doc.class(SKETCH_LINE_3, SKETCH_LINE, 0);
    for (id, x) in [(170, 0.0), (171, 3.0), (172, 1.5)] {
        doc.obj(
            id,
            SKETCH_POINT,
            cat(&[
                &[0, 1],
                &u32b(1),
                &s8("pt_tag"),
                &s8("IntrinsicMetaTypeuint64"),
                &7u64.to_le_bytes(),
                &r(53),
                &[0, 0, 0, 0, 1, 0, 1, 0],
                &f64s(&[x, 0.0, 0.0]),
                &r(40),
            ]),
        );
    }
    let counted = |v: &[f64], width: usize| {
        cat(&[
            &u32b((v.len() / width) as u32),
            &u32b((v.len() / width) as u32),
            &u32b(8),
            &f64s(v),
        ])
    };
    let nurbs = cat(&[
        &u32b(3),
        &f64s(&[1e-9]),
        &counted(&[0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0], 1),
        &counted(&[], 1),
        &counted(
            &[0.0, 0.0, 0.0, 1.0, 2.0, 0.0, 2.0, 2.0, 0.0, 3.0, 0.0, 0.0],
            3,
        ),
        &[0x11, 0xea],
    ]);
    // Spline 173 has both lists (174: one fit point, 175: one handle line 176),
    // spline 177 none.
    doc.obj(
        173,
        SKETCH_SPLINE,
        cat(&[&[0, 0], &r(174), &r(175), &r(170), &r(171), &nurbs]),
    );
    doc.obj(174, SPLINE_FIT_POINTS, cat(&[&[0, 0], &r(172), &r(173)]));
    doc.obj(175, SPLINE_HANDLES, cat(&[&[0, 0], &r(176), &r(173)]));
    doc.obj(176, SKETCH_LINE_3, vec![0, 0]);
    doc.obj(
        177,
        SKETCH_SPLINE,
        cat(&[&[0, 0], &r(170), &r(171), &nurbs]),
    );
    let seg = doc.segment();
    let splines = sketch::splines(&seg, &sketch::points(&seg));
    let s = &splines[&173];
    assert_eq!((s.start_point, s.end_point), (Some(170), Some(171)));
    assert_eq!(
        (s.fit_points.clone(), s.handles.clone()),
        (vec![172], vec![176])
    );
    assert!(s.has_fit_points());
    let plain = &splines[&177];
    assert!(plain.fit_points.is_empty() && plain.handles.is_empty());
    assert!(!plain.has_fit_points());
}

/// A line of the subclass `16DEFC4D` (mitcad#178): its own members (two
/// point references) after the root part, the line's in the sub-chunk
/// after the chunk's header.
#[test]
fn line_subclass_in_its_sub_chunk() {
    let mut doc = sample();
    doc.class(SKETCH_LINE_2, SKETCH_LINE, 0);
    let geometry = [1.0, 2.0, 0.0, 3.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, -1.0];
    doc.obj_sub(
        58,
        SKETCH_LINE_2,
        cat(&[&[0, 0], &r(50), &r(51)]),
        cat(&[&u32b(0), &f64s(&geometry), &r(51), &r(50), &r(40)]),
    );
    let seg = doc.segment();
    let ln = sketch::lines(&seg)[&58];
    assert_eq!(ln.start, [1.0, 2.0, 0.0]);
    assert_eq!(ln.end(), [4.0, 2.0, 0.0]);
    assert_eq!((ln.end_point, ln.start_point), (Some(51), Some(50)));
}

/// A text box's corner point (`00D157C2`) keeps its members in its
/// sub-chunk; a sub-chunk index entry that points past the object is no
/// sub-chunk to read, not a slice past the data (mitcad#934).
#[test]
fn a_sub_chunk_offset_past_the_object_is_skipped() {
    let mut doc = Doc::default();
    doc.class(ROOT, "", 0);
    doc.class(SKETCH_POINT, ROOT, 11);
    doc.class(SKETCH_POINT_2, SKETCH_POINT, 1);
    doc.class(USAGE, ROOT, 1);
    doc.obj(53, USAGE, vec![0, 0]);
    doc.obj_sub(
        59,
        SKETCH_POINT_2,
        vec![0, 0],
        cat(&[
            &u32b(0),
            &r(53),
            &[0, 0, 0, 0, 1, 0, 1, 0],
            &f64s(&[1.0, 2.0, 0.0]),
        ]),
    );
    doc.obj(60, USAGE, vec![0, 0]);
    let mut seg = doc.segment();
    // As written, the point is read from its sub-chunk.
    assert_eq!(sketch::points(&seg)[&59].xyz, [1.0, 2.0, 0.0]);
    let i = seg.objects.iter().position(|o| o.id == 59).unwrap();
    let (offset, end) = (seg.objects[i].offset, seg.objects[i].end);
    let len = (end - offset) as usize;
    // Past the object (into the next one) and past the whole stream.
    for past in [end + 4, end + 1_000_000] {
        seg.objects[i].subchunk = Some(past);
        let o = &seg.objects[i];
        assert_eq!((seg.sub_start(o), seg.main_end(o)), (len, len));
        assert!(sketch::points(&seg).is_empty());
        assert!(sketch::lines(&seg).is_empty());
        assert!(sketch::entity_list(&seg, o).is_empty());
    }
}

/// A sketch's sub-chunk of 100 000 back-to-back references. A reference
/// `01 id 00 00` whose id has the bytes 6 and 7 `FF FF` reads, with its
/// two closing zeros, as the count 65 535 before the next reference; in
/// the last 65 535 references no such count can be filled, so every
/// candidate list there fails, and reading each one afresh took about
/// 2 × 10^9 reference reads. Each offset is now read once (mitcad#934).
#[test]
fn entity_list_of_many_references_is_linear() {
    const N: usize = 100_000;
    const TAIL: usize = 65_535;
    let (plain, counting): (u64, u64) = (7, 0xFFFF_0000_0000_0007);
    let ids: Vec<u64> = (0..N)
        .map(|i| if i >= N - TAIL { counting } else { plain })
        .collect();
    let refs: Vec<u8> = ids.iter().flat_map(|&id| r(id)).collect();
    let sketch_with = |count: u32| {
        let mut doc = Doc::default();
        doc.class(ROOT, "", 0);
        doc.class(SKETCH, ROOT, 18);
        doc.class(USAGE, ROOT, 1);
        doc.obj(plain, USAGE, vec![0, 0]);
        doc.obj(counting, USAGE, vec![0, 0]);
        doc.obj_sub(40, SKETCH, vec![0, 0], cat(&[&u32b(count), &refs]));
        doc.segment()
    };
    // A count of zero first: no list starts before the references.
    let seg = sketch_with(0);
    let s = seg.object(40).unwrap();
    let started = std::time::Instant::now();
    assert!(sketch::entity_list(&seg, s).is_empty());
    let elapsed = started.elapsed();
    assert!(
        elapsed < std::time::Duration::from_millis(100),
        "{elapsed:?}"
    );
    // With the count of 100 000 first, the whole run is the list.
    let seg = sketch_with(N as u32);
    assert_eq!(sketch::entity_list(&seg, seg.object(40).unwrap()), ids);
}

/// A driven dimension (mitcad#178): a null reference (one zero byte)
/// where the parameter holder would be, then its entities; its string is
/// the value it shows. The sketch's dimension comes in driven.
#[test]
fn driven_dimension() {
    let mut doc = sample();
    let mut flags = vec![0u8; 21];
    flags[15] = 0x40;
    let body = cat(&[
        &[0, 0],
        &s16("2.00 mm"),
        &f64s(&[1.0, 0.5, 0.0]),
        &flags,
        &[0],
        &u32b(2),
        &r(50),
        &r(51),
        &[0],
    ]);
    let o = doc
        .objects
        .iter_mut()
        .find(|o| o.0 == 55)
        .expect("the sample's dimension");
    o.2 = body;
    let seg = doc.segment();
    let dm = sketch::parse_dimension(&seg, seg.object(55).unwrap()).unwrap();
    assert_eq!((dm.holder, dm.entities.clone()), (None, vec![50, 51]));
    assert_eq!(dm.expression.as_deref(), Some("2.00 mm"));
    assert_eq!(dm.text, Some([1.0, 0.5, 0.0]));
    let (infos, _) = sketch::sketch_details(&seg);
    let d = &infos[0].detail.dimensions.as_ref().unwrap()[0];
    assert_eq!(d.dimension_type.as_deref(), Some("SketchLinearDimension"));
    assert_eq!(d.is_driving, Some(false));
    let f = d.f3d.as_ref().unwrap();
    assert_eq!(f.expression.as_deref(), Some("2.00 mm"));
}

/// A distance between a line and a point with no parameter holder is a
/// driving dimension (the reference dumps' offset dimensions without a
/// parameter), not a driven one.
#[test]
fn offset_dimension_without_a_holder_is_driving() {
    let mut doc = sample();
    let mut flags = vec![0u8; 21];
    flags[15] = 0x40;
    let body = cat(&[
        &[0, 0],
        &s16("2.00 mm"),
        &f64s(&[1.0, 0.5, 0.0]),
        &flags,
        &[0],
        &u32b(2),
        &r(52),
        &r(50),
        &[0],
    ]);
    let o = doc
        .objects
        .iter_mut()
        .find(|o| o.0 == 55)
        .expect("the sample's dimension");
    o.2 = body;
    let seg = doc.segment();
    let (infos, _) = sketch::sketch_details(&seg);
    let d = &infos[0].detail.dimensions.as_ref().unwrap()[0];
    assert_eq!(d.dimension_type.as_deref(), Some("SketchOffsetDimension"));
    assert_eq!(d.is_driving, None);
}

fn sample_dump() -> Value {
    let (m, b) = sample().streams();
    let design = Design::parse(&m, b).unwrap();
    serde_json::to_value(design.dump("sample.f3d", "Design1")).unwrap()
}

#[test]
fn dump_matches_the_schema_shape() {
    let v = sample_dump();
    assert_eq!(v["schema"], "mitcad-f3d-dump");
    assert_eq!(
        v["source"],
        json!({"mode": "f3d_stream", "file": "sample.f3d", "segment": "Design1"})
    );
    assert_eq!(v["document"]["root_component"], "Bracket");
    assert_eq!(v["document"]["design_type"], "ParametricDesignType");
    assert_eq!(
        v["components"],
        json!([{"name": "Bracket", "is_root": true, "_f3d": {"object_id": 2}},
               {"name": "Pin", "is_root": false, "_f3d": {"object_id": 5}}])
    );
    let items = &v["timeline"]["items"];
    assert_eq!(items[0]["name"], "Sketch1");
    assert_eq!(items[0]["objectType"], "Sketch");
    let sk = &items[0]["detail"];
    assert_eq!(sk["name"], "Sketch1");
    assert_eq!(
        sk["referencePlane"],
        json!({"kind": "construction_plane", "name": "XY", "origin": "XY", "timeline_index": null,
               "geometry": {"type": "Plane", "origin": [0.0, 0.0, 0.0], "normal": [0.0, 0.0, 1.0],
                            "uDirection": [1.0, 0.0, 0.0], "vDirection": [0.0, 1.0, 0.0]}})
    );
    assert_eq!(sk["origin"], json!([1.0, 2.0, 3.0]));
    assert_eq!(sk["model_frame"]["origin"], json!([1.0, 2.0, 3.0]));
    assert_eq!(sk["model_frame"]["z_axis"], json!([0.0, 0.0, 1.0]));
    assert_eq!(
        sk["model_frame"]["sketch_to_model"][1],
        json!([0.0, 1.0, 0.0, 2.0])
    );
    // The stored transform's direction is not settled: not written.
    assert!(sk.get("transform").is_none());
    assert_eq!(sk["xDirection"], json!([1.0, 0.0, 0.0]));
    assert_eq!(
        sk["counts"],
        json!({"points": 2, "curves": 1, "texts": 0, "constraints": 1, "dimensions": 1})
    );
    assert_eq!(
        sk["points"],
        json!([{"id": "p0", "xyz": [0.0, 0.0, 0.0], "connected": ["c0"]},
               {"id": "p1", "xyz": [2.0, 0.0, 0.0], "connected": ["c0"]}])
    );
    assert_eq!(
        sk["curves"][0],
        json!({"id": "c0", "type": "SketchLine", "startSketchPoint": "p0", "endSketchPoint": "p1",
               "length": 2.0, "geometry": {"type": "Line3D", "startPoint": [0.0, 0.0, 0.0],
                                           "endPoint": [2.0, 0.0, 0.0]}})
    );
    assert_eq!(
        sk["constraints"][0],
        json!({"id": "k0", "type": "HorizontalConstraint", "refs": {"entities": ["c0"]},
               "_f3d": {"mask": 64}})
    );
    let dim = &sk["dimensions"][0];
    assert_eq!(dim["type"], "SketchLinearDimension");
    assert_eq!(
        dim["props"]["orientation"],
        "HorizontalDimensionOrientation"
    );
    assert_eq!(dim["textPosition"], json!([1.0, 0.5, 0.0]));
    assert_eq!(dim["refs"]["entities"], json!(["p0", "p1"]));
    assert_eq!(
        dim["parameter"],
        json!({"kind": "parameter", "name": "d2", "expression": "10 mm", "value": 1.0, "unit": "mm"})
    );

    let ex = &items[1];
    assert_eq!(
        (ex["name"].as_str(), ex["objectType"].as_str()),
        (Some("Boss"), Some("ExtrudeFeature"))
    );
    let det = &ex["detail"];
    assert_eq!(det["operation"], "NewBodyFeatureOperation");
    assert_eq!(det["extentType"], "OneSideFeatureExtentType");
    assert_eq!(det["extentOne"]["_type"], "DistanceExtentDefinition");
    assert_eq!(det["extentOne"]["distance"]["name"], "d1");
    assert_eq!(det["taperAngleOne"]["expression"], "0 deg");
    assert_eq!(
        det["profile"],
        json!([{"kind": "profile", "sketch": "Sketch1", "sketch_timeline_index": 0},
               {"kind": "profile", "sketch": "Sketch1", "sketch_timeline_index": 0}])
    );
    assert_eq!(ex["_f3d"]["flags"], "000100");
    // (It has a result: not suppressed, mitcad#180.)
    assert!(ex.get("isSuppressed").is_none());
    assert_eq!(ex["_f3d"]["extrude"]["operation"], "NewBody");

    let p = &v["parameters"];
    assert_eq!(
        p["user"],
        json!([{"name": "width", "expression": "d1 * 2", "value": 2.0, "unit": "mm",
                "comment": "note", "dependents": []}])
    );
    let model = p["model"].as_array().unwrap();
    assert_eq!(model[0]["dependents"], json!(["width"]));
    assert_eq!(
        model[0]["createdBy"],
        json!({"kind": "feature", "objectType": "ExtrudeFeature", "name": "Boss", "timeline_index": 1})
    );
    assert_eq!(
        model[1]["createdBy"],
        json!({"kind": "sketch_dimension", "sketch": "Sketch1", "sketch_timeline_index": 0,
               "id": "d0", "objectType": "SketchLinearDimension"})
    );

    let occ = &v["occurrences"];
    assert_eq!(occ.as_array().unwrap().len(), 1);
    assert_eq!(occ[0]["component"], "Pin");
    assert_eq!(occ[0]["isReferencedComponent"], false);
    assert_eq!(occ[0]["transform"][0], json!([1.0, 0.0, 0.0, 10.0]));
    assert_eq!(
        occ[0]["_f3d"],
        json!({"object_id": 80, "component_object": 5})
    );

    let f3d = &v["_f3d"];
    assert_eq!(f3d["format"]["writer"], "2.0.20981");
    assert_eq!(
        f3d["brep_blobs"],
        json!([{"id": 90, "file": "BREP.abc.smb", "holder": 91, "component": 2}])
    );
    assert_eq!(
        f3d["external_documents"],
        json!([{"key": "1122334455667788", "urn": "urn:test:doc?version=1", "version": 1,
               "guids": ["a", "b"], "document_guid": "c"}])
    );
    assert!(f3d.get("non_finite").is_none());
}

#[test]
fn dump_round_trips_through_the_ir() {
    let v = sample_dump();
    let dump: Dump = serde_json::from_value(v.clone()).unwrap();
    assert_eq!(serde_json::to_value(&dump).unwrap(), v);
    let items = dump.timeline_items();
    let ir::Detail::Extrude(ex) = items[1].detail.as_ref().unwrap() else {
        panic!("extrude detail");
    };
    let d1 = ex.extent_one.as_ref().unwrap().distance.as_ref().unwrap();
    assert_eq!(d1.parameter().unwrap().value, Some(1.0));
    assert!(matches!(items[0].detail, Some(ir::Detail::Sketch(_))));
    assert_eq!(dump.all_parameters().count(), 4);
}

#[test]
fn non_finite_values_are_null_and_listed() {
    let mut d = sample();
    // A line with an infinite delta: its length is not finite.
    let line = d.objects.iter_mut().find(|o| o.0 == 52).unwrap();
    line.2[2 + 24..2 + 32].copy_from_slice(&f64::INFINITY.to_le_bytes());
    let (m, b) = d.streams();
    let v = serde_json::to_value(Design::parse(&m, b).unwrap().dump("x", "Design1")).unwrap();
    let curve = &v["timeline"]["items"][0]["detail"]["curves"][0];
    assert_eq!(curve["length"], Value::Null);
    assert_eq!(
        v["_f3d"]["non_finite"],
        json!([
            ".timeline.items[].detail.curves[].geometry.endPoint[]",
            ".timeline.items[].detail.curves[].length"
        ])
    );
}

/// Truncated and corrupted streams decode without panicking.
#[test]
fn damaged_streams_do_not_panic() {
    let (m, b) = sample().streams();
    for cut in (0..m.len()).step_by(3) {
        let _ = MetaStream::parse(&m[..cut]);
    }
    for cut in (0..b.len()).step_by(5) {
        let design = Design::parse(&m, b[..cut].to_vec()).unwrap();
        let _ = serde_json::to_value(design.dump("x", "Design1")).unwrap();
    }
    // Byte flips (a fixed linear congruential sequence).
    let mut x: u64 = 0x2545_f491_4f6c_dd1d;
    for _ in 0..300 {
        let mut damaged = b.clone();
        for _ in 0..8 {
            x = x.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            let at = (x >> 33) as usize % damaged.len();
            damaged[at] = (x >> 17) as u8;
        }
        let design = Design::parse(&m, damaged).unwrap();
        let _ = serde_json::to_value(design.dump("x", "Design1")).unwrap();
    }
}

#[test]
fn direct_design_without_timeline() {
    let mut d = Doc::default();
    d.class(ROOT, "", 0);
    d.class(COMPONENT, ROOT, 1);
    d.class(OCCURRENCE, ROOT, 1);
    d.obj(1, COMPONENT, cat(&[&[0, 0], &s16("Part")]));
    d.obj(2, OCCURRENCE, cat(&[&[0, 0], &r(1), &[1]]));
    let (m, b) = d.streams();
    let v = serde_json::to_value(Design::parse(&m, b).unwrap().dump("x", "Design1")).unwrap();
    assert_eq!(v["document"]["design_type"], "DirectDesignType");
    assert_eq!(v["document"]["root_component"], "Part");
    assert_eq!(
        v["timeline"],
        json!({"available": false, "count": 0, "items": []})
    );
    assert_eq!(v["occurrences"], json!([]));
}

/// Timeline items beyond the reference decoder (mitcad#43): an
/// occurrence item named after its occurrence's component, a group by its
/// own name, a known assembly class by its type; an unknown class stays
/// without a type.
#[test]
fn occurrence_items_groups_and_assembly_items_are_named() {
    const RELATIONSHIP: &str = "D0F69AAA-7BA0-4C9C-8A6D-7D01ADB39593";
    const UNKNOWN: &str = "0000BBBB-0000-0000-0000-000000000002";
    let mut d = sample();
    for (g, v) in [
        (OCCURRENCE_ITEM, 2),
        (OCCURRENCE_REF, 0),
        (GROUP, 1),
        (RELATIONSHIP, 1),
        (UNKNOWN, 0),
    ] {
        d.class(g, ROOT, v);
    }
    d.objects.retain(|o| o.0 != 3);
    d.obj(
        3,
        TIMELINE,
        cat(&[
            &[0, 0],
            &r(1),
            &u32b(6),
            &r(10),
            &r(20),
            &r(110),
            &r(100),
            &r(120),
            &r(130),
        ]),
    );
    // The occurrence item refers to occurrence 80 of component Pin.
    d.obj(
        100,
        OCCURRENCE_ITEM,
        cat(&[&[0, 0], &r(101), &tail(-1, "", 1, "", [0, 0, 0], 102)]),
    );
    d.obj(101, OCCURRENCE_REF, cat(&[&[0, 0], &r(80), &r(100)]));
    d.obj(102, HEALTH, vec![0, 0]);
    d.obj(
        110,
        GROUP,
        cat(&[&[0, 0, 0, 0], &u32b(2), &r(100), &r(120), &s16("Fixings")]),
    );
    d.obj(
        120,
        RELATIONSHIP,
        cat(&[
            &[0, 0],
            &tail(-1, "GeometricRelationship", 1, "", [0, 0, 0], 121),
        ]),
    );
    d.obj(121, HEALTH, vec![0, 0]);
    d.obj(
        130,
        UNKNOWN,
        cat(&[&[0, 0], &tail(-1, "Mystery", 2, "", [0, 0, 0], 131)]),
    );
    d.obj(131, HEALTH, vec![0, 0]);
    let (m, b) = d.streams();
    let v = serde_json::to_value(Design::parse(&m, b).unwrap().dump("x", "Design1")).unwrap();
    let items = v["timeline"]["items"].as_array().unwrap();
    let got: Vec<(&str, Option<&str>)> = items[2..]
        .iter()
        .map(|i| (i["name"].as_str().unwrap(), i["objectType"].as_str()))
        .collect();
    assert_eq!(
        got,
        [
            ("Fixings", Some("Group")),
            ("Pin", Some("Occurrence")),
            ("GeometricRelationship1", Some("GeometricRelationship")),
            ("Mystery2", None),
        ]
    );
    // The occurrence the occurrence item made (mitcad#75).
    assert_eq!(
        items[3]["detail"]["occurrence"]["_f3d"]["path"],
        json!([80])
    );
}

/// An occurrence of a component without a name of its own is named
/// `Component<n>`, the number being the one in its item's tail.
#[test]
fn unnamed_components_get_their_default_name() {
    let mut d = sample();
    for (g, v) in [(OCCURRENCE_ITEM, 2), (OCCURRENCE_REF, 0)] {
        d.class(g, ROOT, v);
    }
    d.objects.retain(|o| o.0 != 3 && o.0 != 5);
    d.obj(5, COMPONENT, cat(&[&[0, 0], &s16("")]));
    d.obj(
        3,
        TIMELINE,
        cat(&[&[0, 0], &r(1), &u32b(3), &r(10), &r(20), &r(100)]),
    );
    d.obj(
        100,
        OCCURRENCE_ITEM,
        cat(&[&[0, 0], &r(101), &tail(-1, "", 3, "", [0, 0, 0], 102)]),
    );
    d.obj(101, OCCURRENCE_REF, cat(&[&[0, 0], &r(80), &r(100)]));
    d.obj(102, HEALTH, vec![0, 0]);
    let (m, b) = d.streams();
    let v = serde_json::to_value(Design::parse(&m, b).unwrap().dump("x", "Design1")).unwrap();
    let items = v["timeline"]["items"].as_array().unwrap();
    assert_eq!(items[2]["name"], "Component3");
    let names: Vec<&str> = v["components"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|c| c["name"].as_str())
        .collect();
    assert!(names.contains(&"Component3") && names.contains(&"Bracket"));
}

/// A sweep's and a loft's inputs (mitcad#34): the operation, the profile
/// of the profile source, a path of a sketch curve named by its ids, and a
/// loft's sections (a profile and a sketch point), end conditions and
/// centre line.
#[test]
fn sweep_and_loft_inputs() {
    const SWEEP: &str = "FCBB1707-4450-46B3-9E65-0F61682EA8CA";
    let mut d = sample();
    for (g, v) in [
        (SWEEP, 6),
        (BODY_INPUT, 1),
        (SKETCH_CURVE_ID, 0),
        (SKETCH_POINT_ID, 0),
        (LOFT, 9),
        (LOFT_SECTION, 1),
    ] {
        d.class(g, ROOT, v);
    }
    d.objects.retain(|o| ![3, 50, 51, 52].contains(&o.0));
    d.obj(
        3,
        TIMELINE,
        cat(&[&[0, 0], &r(1), &u32b(4), &r(10), &r(20), &r(100), &r(200)]),
    );
    // The sketch's entities with the ids inputs name them by.
    let tag =
        |key: &str, v: u64| cat(&[&s8(key), &s8("IntrinsicMetaTypeuint64"), &v.to_le_bytes()]);
    for (id, x, t) in [(50, 0.0, 7), (51, 2.0, 8)] {
        d.obj(
            id,
            SKETCH_POINT,
            cat(&[
                &[0, 1],
                &u32b(1),
                &tag("pt_tag", t),
                &r(53),
                &[0, 0, 0, 0, 1, 0, 1, 0],
                &f64s(&[x, 0.0, 0.0]),
                &r(40),
            ]),
        );
    }
    d.obj(
        52,
        SKETCH_LINE,
        cat(&[
            &[0, 1],
            &u32b(2),
            &tag("crv_primary_id", 103),
            &tag("crv_secondary_id", 0),
            &f64s(&[0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, -1.0]),
            &r(51),
            &r(50),
            &r(40),
        ]),
    );
    // The sweep: operation, header, the parameter holders, no guide
    // surfaces, the path, no rail, the profile input, taper and twist.
    d.obj(
        100,
        SWEEP,
        cat(&[
            &[0, 0],
            &u32b(4),
            &[1],
            &u32b(3),
            &[0, 0, 1, 1, 1],
            &f64s(&[1.0, 0.0, 0.0]),
            &r(101),
            &r(102),
            &r(103),
            &r(104),
            &[0],
            &r(110),
            &[0],
            &r(105),
            &r(106),
            &r(107),
            &u32b(2),
            &r(111),
            &r(110),
            &tail(5, "Sweep", 1, "", [0, 0, 0], 108),
        ]),
    );
    for h in [101, 102, 103, 104, 106, 107] {
        d.obj(h, PARAMETER_HOLDER, cat(&[&[1], &u32b(1), &r(100), &[0]]));
    }
    d.obj(105, PROFILE_ID, vec![0, 0]);
    d.obj(108, HEALTH, vec![0, 0]);
    let p = parameter(10, Some(103), "1", "AlongDistance", "", "", "d10", 1.0);
    d.obj(109, PARAMETER, p);
    d.obj(110, BODY_INPUT, cat(&[&[0, 0], &u32b(1), &r(112)]));
    d.obj(111, PROFILE_SOURCE, cat(&[&[0, 0], &s16("40"), &r(114)]));
    d.obj(112, ENTITY_REF, cat(&[&[0, 0], &r(113)]));
    d.obj(
        113,
        SKETCH_CURVE_ID,
        cat(&[
            &[0, 0],
            &0u64.to_le_bytes(),
            &40u64.to_le_bytes(),
            &103u64.to_le_bytes(),
        ]),
    );
    d.obj(114, PROFILE_ID, vec![0, 0]);
    // The loft: flags, a join, the centre line, the last section's tangent
    // point condition, no rails, the sections, the first one's free end.
    d.obj(
        200,
        LOFT,
        cat(&[
            &[0, 0],
            &[1, 1, 1, 1],
            &u32b(1),
            &r(110),
            &(-1i32).to_le_bytes(),
            &0u64.to_le_bytes(),
            &[0],
            &u32b(5),
            &r(206),
            &u32b(0),
            &[0],
            &u32b(1),
            &[0],
            &r(105),
            &u32b(2),
            &r(220),
            &r(221),
            &(-1i32).to_le_bytes(),
            &0u64.to_le_bytes(),
            &[0],
            &u32b(0),
            &[0],
            &u32b(0),
            &u32b(0),
            &tail(6, "Loft", 1, "", [0, 0, 0], 207),
        ]),
    );
    d.obj(206, PARAMETER_HOLDER, cat(&[&[1], &u32b(1), &r(200), &[0]]));
    d.obj(207, HEALTH, vec![0, 0]);
    let p = parameter(11, Some(206), "1.5", "End Weight", "", "", "d11", 1.5);
    d.obj(208, PARAMETER, p);
    for (s, list, kind) in [(220, 222, 2), (221, 223, 4)] {
        d.obj(
            s,
            LOFT_SECTION,
            cat(&[&[0, 0], &0u64.to_le_bytes(), &r(200), &r(list), &u32b(kind)]),
        );
    }
    d.obj(222, BODY_INPUT, cat(&[&[0, 0], &u32b(1), &r(111)]));
    d.obj(223, BODY_INPUT, cat(&[&[0, 0], &u32b(1), &r(224)]));
    d.obj(224, ENTITY_REF, cat(&[&[0, 0], &r(225)]));
    d.obj(
        225,
        SKETCH_POINT_ID,
        cat(&[&[0, 0], &40u64.to_le_bytes(), &8u64.to_le_bytes()]),
    );
    let (m, b) = d.streams();
    let v = serde_json::to_value(Design::parse(&m, b).unwrap().dump("x", "Design1")).unwrap();
    let items = v["timeline"]["items"].as_array().unwrap();
    let curve = json!({"kind": "sketch_entity", "objectType": "SketchLine", "sketch": "Sketch1",
                       "sketch_timeline_index": 0, "id": "c0"});
    let profile = json!({"kind": "profile", "sketch": "Sketch1", "sketch_timeline_index": 0});
    let sweep = &items[2]["detail"];
    assert_eq!(items[2]["objectType"], "SweepFeature");
    assert_eq!(sweep["operation"], "NewBodyFeatureOperation");
    assert_eq!(sweep["profile"], json!([profile]));
    assert_eq!(
        sweep["path"],
        json!([{"_type": "PathEntity", "entity": curve}])
    );
    assert_eq!(sweep["distanceOne"]["name"], "d10");
    assert!(sweep.get("guideRail").is_none() && sweep.get("guideSurfaces").is_none());
    let loft = &items[3]["detail"];
    assert_eq!(items[3]["objectType"], "LoftFeature");
    assert_eq!(loft["operation"], "JoinFeatureOperation");
    assert_eq!(
        loft["centerLineOrRails"],
        json!([[{"_type": "PathEntity", "entity": curve}]])
    );
    assert_eq!(loft["centerLineOrRails.isCenterLine"], true);
    let sections = loft["loftSections"].as_array().unwrap();
    assert_eq!(sections.len(), 2);
    assert_eq!(sections[0]["entity"], profile);
    assert_eq!(
        sections[0]["endCondition"],
        json!({"_type": "LoftFreeEndCondition"})
    );
    assert_eq!(
        sections[1]["entity"],
        json!({"kind": "sketch_entity", "objectType": "SketchPoint", "sketch": "Sketch1",
               "sketch_timeline_index": 0, "id": "p1"})
    );
    assert_eq!(
        sections[1]["endCondition"]["_type"],
        "LoftPointTangentEndCondition"
    );
    assert_eq!(sections[1]["endCondition"]["weight"]["value"], 1.5);
}

#[test]
fn file_level_api() {
    let (m, b) = sample().streams();
    let zip = mitcad_zip::stored_zip(&[
        ("FusionAssetName[Active]/Design1/MetaStream.dat", &m),
        ("FusionAssetName[Active]/Design1/BulkStream.dat", &b),
    ]);
    let doc = F3dFile::from_bytes(zip).unwrap();
    let s = find_design_streams(&doc).unwrap().unwrap();
    assert_eq!(s.segment_dir, "Design1");
    let fd = decode_document(&doc, "a.f3d").unwrap();
    // In two parts, the inputs found on threads (mitcad#103): the same.
    let mut parts = decode_streams(&doc, "a.f3d").unwrap();
    resolve_inputs(&mut parts, &doc, 4);
    assert_eq!(
        serde_json::to_value(&parts.dump).unwrap(),
        serde_json::to_value(&fd.dump).unwrap()
    );
    assert_eq!(fd.dump.unwrap().timeline_items().len(), 2);
    assert_eq!(
        short_name(Path::new(
            "dir/Part v3.0c99fb88-aaaa-bbbb-cccc-0123456789ab.f3d"
        )),
        "Part v3.f3d"
    );
    assert_eq!(short_name(Path::new("x.f3d")), "x.f3d");
}

/// An external dump (keys, `_error` values, references) reads
/// into the IR and writes back unchanged.
#[test]
fn external_dump_reads_and_round_trips() {
    let text = include_str!("../../f3d/tests/data/external_dump_box.json");
    let v: Value = serde_json::from_str(text).unwrap();
    let dump = Dump::from_json(text).unwrap();
    assert_eq!(serde_json::to_value(&dump).unwrap(), v);
    let items = dump.timeline_items();
    assert!(!items.is_empty());
    for it in items {
        match (it.object_type(), &it.detail) {
            (Some("Sketch"), Some(ir::Detail::Sketch(s))) => {
                assert!(s.points.as_ref().is_some_and(|p| !p.is_empty()));
            }
            (Some("ExtrudeFeature"), Some(ir::Detail::Extrude(e))) => {
                assert!(e.operation.is_some());
            }
            (Some("ConstructionPlane"), Some(ir::Detail::ConstructionPlane(p))) => {
                // `definition` failed in its producer: its `_error` is kept.
                let def = p.definition.as_ref().unwrap();
                assert!(def.definition_type.is_none() && def.other.contains_key("_error"));
                assert_eq!(p.geometry.as_ref().unwrap().origin, Some([0.0, 0.0, 3.0]));
            }
            (t, d) => panic!("unexpected detail for {t:?}: {d:?}"),
        }
    }
}

/// A face recipe naming one face by a tag and its operations.
fn face_recipe(tag: &str, ops: &[i32]) -> Vec<u8> {
    let mut d = cat(&[
        &[0, 0],
        &u32b(1),
        &u32b(3),
        &u32b(1),
        &u32b(1),
        &s8(tag),
        &u32b(0),
    ]);
    d.extend(u32b(ops.len() as u32));
    for o in ops {
        d.extend(o.to_le_bytes());
    }
    d.extend(cat(&[&u32b(0), &u32b(0), &s8("face_recipe_data")]));
    d
}

/// A level of an occurrence path: the occurrence's, its document's and its
/// component's GUIDs, then the document and component it sits in.
fn context_level(occurrence: &str, component: &str, context: &str) -> Vec<u8> {
    const DOC: &str = "dddddddd-0000-0000-0000-000000000000";
    cat(&[
        &[0, 0],
        &u32b(1),
        &s16(occurrence),
        &s16(DOC),
        &s16(component),
        &[2, 0, 0, 0, 0, 0, 0, 0],
        &s16(DOC),
        &s16(context),
        &u32b(2),
    ])
}

/// Joints (mitcad#66): a joint between two occurrences, side one a frame
/// built on a face, side two a frame without a stored matrix, with its
/// alignment parameters, a revolute motion with rotation limits, the
/// occurrence of each side by its path, and a ground item.
#[test]
fn joints_and_ground_items() {
    const RECIPE: &str = "7ACC2A03-0261-4879-A14A-A93D661A5BDC";
    const ROOT_GUID: &str = "22222222-0000-0000-0000-000000000002";
    const PIN_GUID: &str = "55555555-0000-0000-0000-000000000005";
    const OCC_A: &str = "a0a0a0a0-0000-0000-0000-000000000080";
    const OCC_B: &str = "b0b0b0b0-0000-0000-0000-000000000083";
    let mut d = sample();
    for (g, v) in [
        (JOINT, 4),
        (GROUND_OCCURRENCE, 1),
        (JOINT_STATE, 5),
        (PLACEMENT, 5),
        (CONTEXT_PATH, 1),
        (CONTEXT_LEVEL, 4),
        (FRAME_INPUT, 9),
        (KEY_POINT, 3),
        (DIRECTION_INPUT, 2),
        (FACE_REF, 2),
        (RECIPE, 1),
    ] {
        d.class(g, ROOT, v);
    }
    d.objects.retain(|o| ![2, 3, 5, 80, 82].contains(&o.0));
    d.obj(
        2,
        COMPONENT,
        cat(&[&[0, 0], &s16(ROOT_GUID), &s16("Bracket")]),
    );
    d.obj(5, COMPONENT, cat(&[&[0, 0], &s16(PIN_GUID), &s16("Pin")]));
    let t = [
        1.0, 0.0, 0.0, 10.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ];
    for (id, guid) in [(80, OCC_A), (83, OCC_B)] {
        d.obj(
            id,
            OCCURRENCE,
            cat(&[
                &[0, 0],
                &r(5),
                &[0],
                &f64s(&t),
                &u32b(0),
                &r(81),
                &s16(guid),
            ]),
        );
    }
    d.obj(82, OCCURRENCE, cat(&[&[0, 0], &r(2), &[1]]));
    d.obj(
        3,
        TIMELINE,
        cat(&[&[0, 0], &r(1), &u32b(4), &r(10), &r(20), &r(300), &r(400)]),
    );
    // Side one: a frame on a face, at (1, 2, 3) with z down.
    let m1 = [
        1.0, 0.0, 0.0, 1.0, 0.0, -1.0, 0.0, 2.0, 0.0, 0.0, -1.0, 3.0, 0.0, 0.0, 0.0, 1.0,
    ];
    d.obj(
        310,
        FRAME_INPUT,
        cat(&[&[0, 0], &[0; 8], &f64s(&m1), &[0; 4], &r(311), &r(313)]),
    );
    d.obj(
        311,
        KEY_POINT,
        cat(&[
            &[0, 0],
            &[0; 21],
            &f64s(&[1.0, 2.0, 3.0]),
            &u32b(11),
            &r(312),
        ]),
    );
    d.obj(312, FACE_REF, cat(&[&[0, 0], &r(314)]));
    d.obj(
        313,
        DIRECTION_INPUT,
        cat(&[
            &[0, 0],
            &u32b(1),
            &f64s(&[1.0, 2.0, 3.0, 0.0, 0.0, -1.0, 0.0, 0.0]),
            &u32b(6),
            &r(312),
        ]),
    );
    d.obj(314, RECIPE, face_recipe("3", &[5]));
    d.obj(320, FRAME_INPUT, cat(&[&[0, 0], &[0; 8]]));
    // The placements: side one aligned, side two; their paths.
    for (id, path, level, guid) in [(330, 331, 332, OCC_A), (335, 336, 337, OCC_B)] {
        d.obj(
            id,
            PLACEMENT,
            cat(&[&[0, 0], &[0; 4], &[0], &f64s(&m1), &[0], &r(300), &r(path)]),
        );
        d.obj(
            path,
            CONTEXT_PATH,
            cat(&[&[0, 0], &[1], &u32b(1), &r(level)]),
        );
        d.obj(
            level,
            CONTEXT_LEVEL,
            context_level(guid, PIN_GUID, ROOT_GUID),
        );
    }
    // The state: two occurrences, one free rotation at 0.5, revolute.
    let mut value = vec![0u8; 51];
    value[23..31].copy_from_slice(&0.5f64.to_le_bytes());
    value[47] = 2;
    d.obj(
        340,
        JOINT_STATE,
        cat(&[
            &[1],
            &u32b(2),
            &r(80),
            &u32b(1),
            &r(83),
            &u32b(0),
            &[0],
            &[0; 8],
            &u32b(1),
            &value,
            &[0; 12],
            &u32b(1),
            &u32b(1),
            &[0],
            &u32b(2),
            &[0],
        ]),
    );
    // Parameters: the alignment and the rotation limits.
    let roles = [
        ("alignAngle", "180 deg", std::f64::consts::PI, "deg"),
        ("alignOffsetZ", "2 mm", 0.2, "mm"),
        ("alignOffsetX", "0 mm", 0.0, "mm"),
        ("alignOffsetY", "0 mm", 0.0, "mm"),
        ("RotateMinimum", "0 deg", 0.0, "deg"),
        (
            "RotateMaximum",
            "90 deg",
            std::f64::consts::FRAC_PI_2,
            "deg",
        ),
    ];
    for (k, (role, expr, value, unit)) in roles.iter().enumerate() {
        let h = 350 + 2 * k as u64;
        d.obj(h, PARAMETER_HOLDER, cat(&[&[1], &u32b(1), &r(300), &[0]]));
        let name = format!("d{}", 10 + k);
        d.obj(
            h + 1,
            PARAMETER,
            parameter(10 + k as u32, Some(h), expr, role, "", unit, &name, *value),
        );
    }
    let holders: Vec<u8> = (0..4).flat_map(|k| r(350 + 2 * k)).collect();
    d.obj(
        300,
        JOINT,
        cat(&[
            &[0, 0],
            &[1],
            &[0; 10],
            &r(310),
            &[0; 6],
            &[0],
            &f64s(&m1),
            &r(320),
            &[0; 6],
            &[1],
            &s16("cccccccc-0000-0000-0000-000000000000"),
            &u32b(0),
            &holders,
            &[0; 6],
            &u32b(2),
            &r(330),
            &r(335),
            &r(340),
            &tail(-1, "Assemble", 1, "", [0, 0, 0], 301),
        ]),
    );
    d.obj(301, HEALTH, vec![0, 0]);
    // A ground item naming occurrence 83.
    d.obj(410, PLACEMENT, cat(&[&[0, 0], &[1], &r(400), &r(336)]));
    d.obj(
        400,
        GROUND_OCCURRENCE,
        cat(&[
            &[0, 0],
            &[1],
            &r(410),
            &tail(-1, "", 1, "Pin1", [0, 0, 0], 401),
        ]),
    );
    d.obj(401, HEALTH, vec![0, 0]);

    let (m, b) = d.streams();
    let v = serde_json::to_value(Design::parse(&m, b).unwrap().dump("x", "Design1")).unwrap();
    let items = v["timeline"]["items"].as_array().unwrap();
    let joint = &items[2];
    assert_eq!(joint["objectType"], "Joint");
    let j = &joint["detail"];
    assert_eq!(j["isFlipped"], true);
    assert_eq!(j["_f3d"]["opposed"], 1);
    assert_eq!(j["angle"]["name"], "d10");
    assert_eq!(j["offset"]["value"], 0.2);
    let one = &j["geometryOrOriginOne"];
    assert_eq!(one["_type"], "JointGeometry");
    assert_eq!(one["origin"], json!([1.0, 2.0, 3.0]));
    assert_eq!(one["thirdAxisVector"], json!([0.0, 0.0, -1.0]));
    assert_eq!(one["entityOne"]["kind"], "face");
    assert_eq!(one["entityOne"]["_f3d"]["entities"][0][0]["tag"], "3");
    assert_eq!(
        one["_f3d"]["key_points"][0]["point"],
        json!([1.0, 2.0, 3.0])
    );
    assert_eq!(
        one["_f3d"]["directions"][0]["direction"],
        json!([0.0, 0.0, -1.0])
    );
    // Side two stores no matrix: the identity of its component.
    assert_eq!(j["geometryOrOriginTwo"]["origin"], json!([0.0, 0.0, 0.0]));
    assert_eq!(j["_f3d"]["frames"][1][0], json!([1.0, 0.0, 0.0, 0.0]));
    assert_eq!(j["_f3d"]["frames"][0][1], json!([0.0, -1.0, 0.0, 2.0]));
    assert_eq!(j["occurrenceOne"]["_f3d"]["path"], json!([80]));
    assert_eq!(j["occurrenceTwo"]["_f3d"]["path"], json!([83]));
    assert_eq!(j["occurrenceTwo"]["_f3d"]["context_component"], 2);
    assert_eq!(j["occurrenceTwo"]["component"], "Pin");
    let motion = &j["jointMotion"];
    assert_eq!(motion["_type"], "RevoluteJointMotion");
    assert_eq!(motion["_f3d"]["motions"][0]["motion"], "rz");
    assert_eq!(motion["_f3d"]["motions"][0]["limits"][0], 0.5);
    assert_eq!(motion["rotationLimits"]["maximumValue"]["name"], "d15");
    assert_eq!(motion["rotationLimits"]["isMinimumValueEnabled"], true);
    assert!(motion["slideLimits"].is_null());
    // The ground item grounds occurrence 83 only.
    let ground = &items[3];
    assert_eq!(ground["objectType"], "GroundOccurrence");
    assert_eq!(ground["detail"]["occurrence"]["_f3d"]["path"], json!([83]));
    let grounded: Vec<(u64, bool)> = v["occurrences"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| {
            (
                n["_f3d"]["object_id"].as_u64().unwrap(),
                n["isGrounded"] == true,
            )
        })
        .collect();
    assert!(grounded.contains(&(83, true)) && grounded.contains(&(80, false)));
    // A motion whose free motions do not fit its type is left out.
    assert!(super::build::tests_support::kind_fits(1, 1));
    assert!(!super::build::tests_support::kind_fits(0, 1));
}

const OCC_A: &str = "a0a0a0a0-0000-0000-0000-000000000080";
const INSIDE: &str = "e0e0e0e0-0000-0000-0000-0000000000e0";

/// Captured positions (mitcad#75), class versions 6 and 5: a snapshot puts
/// occurrence 80 at a matrix and an occurrence inside a component of
/// another document at the identity; the second one's path level names
/// two occurrences, the one in the file and one inside the other
/// document.
#[test]
fn captured_positions() {
    for (version, gap) in [(6, 8), (5, 4)] {
        let v = captured_position_design(version, gap);
        let snapshot = &v["timeline"]["items"][2];
        assert_eq!(snapshot["objectType"], "Snapshot");
        let positions = snapshot["detail"]["positions"].as_array().unwrap();
        assert_eq!(positions.len(), 2, "version {version}");
        let one = &positions[0];
        assert_eq!(one["occurrence"]["_f3d"]["path"], json!([80]));
        assert_eq!(one["occurrence"]["_f3d"]["context_component"], 2);
        assert_eq!(one["transform"][0], json!([0.0, -1.0, 0.0, 0.0]));
        assert_eq!(one["transform"][2], json!([0.0, 0.0, 1.0, 2.0]));
        let two = &positions[1];
        assert_eq!(two["occurrence"]["_f3d"]["path"], json!([80, null]));
        assert_eq!(
            two["occurrence"]["_f3d"]["path_guids"],
            json!([OCC_A, INSIDE])
        );
        assert_eq!(two["transform"][0], json!([1.0, 0.0, 0.0, 0.0]));
    }
}

/// The design of [`captured_positions`]: the snapshot's class `version`,
/// `gap` bytes between its root part and its count.
fn captured_position_design(version: u32, gap: usize) -> Value {
    const ROOT_GUID: &str = "22222222-0000-0000-0000-000000000002";
    const PIN_GUID: &str = "55555555-0000-0000-0000-000000000005";
    const DOC: &str = "dddddddd-0000-0000-0000-000000000000";
    let mut d = sample();
    for (g, v) in [
        (SNAPSHOT, version),
        (PLACEMENT, 5),
        (CONTEXT_PATH, 1),
        (CONTEXT_LEVEL, 4),
    ] {
        d.class(g, ROOT, v);
    }
    d.objects.retain(|o| ![2, 3, 80].contains(&o.0));
    d.obj(
        2,
        COMPONENT,
        cat(&[&[0, 0], &s16(ROOT_GUID), &s16("Bracket")]),
    );
    let t = [
        1.0, 0.0, 0.0, 10.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ];
    d.obj(
        80,
        OCCURRENCE,
        cat(&[
            &[0, 0],
            &r(5),
            &[0],
            &f64s(&t),
            &u32b(0),
            &r(81),
            &s16(OCC_A),
        ]),
    );
    d.obj(
        3,
        TIMELINE,
        cat(&[&[0, 0], &r(1), &u32b(3), &r(10), &r(20), &r(500)]),
    );
    // Turned a quarter about z, 2 cm up.
    let m = [
        0.0, -1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 2.0, 0.0, 0.0, 0.0, 1.0,
    ];
    d.obj(
        500,
        SNAPSHOT,
        cat(&[
            &[0, 0],
            &vec![0; gap],
            &u32b(2),
            &[0],
            &r(510),
            &[0],
            &f64s(&m),
            &[0],
            &r(520),
            &[1],
            &u32b(0),
            &tail(-1, "Position", 1, "", [0, 0, 0], 501),
        ]),
    );
    d.obj(501, HEALTH, vec![0, 0]);
    for (placement, path) in [(510, 511), (520, 521)] {
        d.obj(
            placement,
            PLACEMENT,
            cat(&[&[0, 0], &[1], &r(500), &r(path)]),
        );
        d.obj(
            path,
            CONTEXT_PATH,
            cat(&[&[0, 0], &[1], &u32b(1), &r(path + 1)]),
        );
    }
    d.obj(
        512,
        CONTEXT_LEVEL,
        context_level(OCC_A, PIN_GUID, ROOT_GUID),
    );
    d.obj(
        522,
        CONTEXT_LEVEL,
        cat(&[
            &[0, 0],
            &u32b(2),
            &s16(OCC_A),
            &s16(INSIDE),
            &s16(DOC),
            &s16(PIN_GUID),
            &[2, 0, 0, 0, 0, 0, 0, 0],
            &s16(DOC),
            &s16(ROOT_GUID),
            &u32b(2),
        ]),
    );
    let (mb, b) = d.streams();
    serde_json::to_value(Design::parse(&mb, b).unwrap().dump("x", "Design1")).unwrap()
}

/// A combine's operation, kept tools, target and tools, and a hole's
/// points, type and extent through all (mitcad#67).
#[test]
fn combine_and_hole_selections() {
    const RECIPE: &str = "7ACC2A03-0261-4879-A14A-A93D661A5BDC";
    const COMBINE: &str = "2A94257F-2020-4B19-9A68-103A2672F1B7";
    const HOLE: &str = "1C037A07-4A15-43F6-ABFC-BBF61B9038D4";
    const SLOT_KEY: &str = "2DF7DA30-A4E4-4260-AFD1-D9C7154CB44B";
    let mut d = sample();
    for (g, v) in [
        (COMBINE, 1),
        (HOLE, 7),
        (BODY_INPUT, 1),
        (BODY_REF, 1),
        (BODY_RECORD, 2),
        (PLACEMENT, 5),
        (KEY_POINT, 3),
        (RECIPE, 1),
        (SLOT_KEY, 1),
    ] {
        d.class(g, ROOT, v);
    }
    d.objects.retain(|o| o.0 != 3);
    d.obj(
        3,
        TIMELINE,
        cat(&[&[0, 0], &r(1), &u32b(4), &r(10), &r(20), &r(500), &r(600)]),
    );
    let body = |tag: &str| {
        let mut b = face_recipe(tag, &[3]);
        let n = b.len();
        b.truncate(n - 20);
        b.extend(s8("body_recipe_data"));
        b
    };
    for (input, refer, recipe, tag) in [(510, 511, 512, "1"), (520, 521, 522, "2")] {
        d.obj(input, BODY_INPUT, cat(&[&[0, 0], &u32b(1), &r(refer)]));
        d.obj(refer, BODY_REF, cat(&[&[0, 0], &r(recipe)]));
        d.obj(recipe, RECIPE, body(tag));
    }
    d.obj(530, BODY_RECORD, vec![0, 0]);
    d.obj(531, PLACEMENT, vec![0, 0]);
    // Cut, tools not kept: tools [510], consumed [530], target 520.
    d.obj(
        500,
        COMBINE,
        cat(&[
            &[0, 0],
            &u32b(1),
            &u32b(0),
            &[0, 0],
            &u32b(1),
            &r(531),
            &u32b(0x132),
            &u32b(0),
            &u32b(1),
            &r(510),
            &[0; 8],
            &u32b(1),
            &r(530),
            &u32b(1),
            &r(520),
            &u32b(4),
            &r(510),
            &r(511),
            &r(520),
            &r(521),
            &tail(5, "Combine", 1, "", [0, 0, 0], 501),
        ]),
    );
    d.obj(501, HEALTH, vec![0, 0]);
    // A hole through all at two points: a slot of role 5 holds the bodies.
    for (id, x) in [(610, 1.0), (611, 5.0)] {
        d.obj(
            id,
            KEY_POINT,
            cat(&[&[0, 0], &[0; 21], &f64s(&[x, 1.0, 1.0]), &u32b(11)]),
        );
    }
    d.obj(
        540,
        BODY_INPUT,
        cat(&[
            &[0, 0],
            &u32b(1),
            &r(511),
            &[0; 6],
            &r(541),
            &u32b(0),
            &u32b(5),
            &[0; 4],
        ]),
    );
    d.obj(541, SLOT_KEY, vec![0, 0]);
    d.obj(
        600,
        HOLE,
        cat(&[
            &[0, 0],
            &u32b(0),
            &[1, 1, 1, 0],
            &u32b(3),
            &r(610),
            &r(611),
            &r(540),
            &tail(6, "Hole", 1, "", [0, 0, 0], 601),
        ]),
    );
    d.obj(601, HEALTH, vec![0, 0]);
    let (m, b) = d.streams();
    let v = serde_json::to_value(Design::parse(&m, b).unwrap().dump("x", "Design1")).unwrap();
    let items = v["timeline"]["items"].as_array().unwrap();
    let c = &items[2]["detail"];
    assert_eq!(c["operation"], "CutFeatureOperation");
    assert_eq!(c["isKeepToolBodies"], false);
    assert_eq!(c["targetBody"]["_f3d"]["entities"][0][0]["tag"], "2");
    assert_eq!(c["toolBodies"][0]["_f3d"]["entities"][0][0]["tag"], "1");
    let h = &items[3]["detail"];
    assert_eq!(h["holeType"], "SimpleHoleType");
    assert_eq!(h["position"], json!([1.0, 1.0, 1.0]));
    assert_eq!(
        h["_f3d_positions"],
        json!([[1.0, 1.0, 1.0], [5.0, 1.0, 1.0]])
    );
    assert_eq!(h["extentDefinition"]["_type"], "AllExtentDefinition");
}

#[test]
fn revolve_pattern_mirror_and_hole_inputs() {
    // mitcad#96: a revolution's operation, axis line and profile loops, a
    // mirror's new bodies, a circular pattern's axis line and a hole's
    // through-all flag beside its kept depth.
    const REVOLVE: &str = "E3849A15-2FC6-42A0-AF3A-2F1D7273B406";
    const MIRROR: &str = "D1728651-3640-4CCF-8083-AF7703013978";
    const CIRCULAR: &str = "11F1A5CE-2B57-4476-8480-6994621493C9";
    const HOLE: &str = "1C037A07-4A15-43F6-ABFC-BBF61B9038D4";
    let mut d = sample();
    for (g, v) in [
        (REVOLVE, 2),
        (MIRROR, 0),
        (CIRCULAR, 0),
        (HOLE, 4),
        (SKETCH_CURVE_ID, 0),
        (PROFILE_LOOPS, 1),
        (REVOLVE_PROFILE, 0),
        (BODY_RECORD, 2),
        (BODY_INPUT, 1),
        (DIRECTION_INPUT, 4),
    ] {
        d.class(g, ROOT, v);
    }
    d.objects.retain(|o| ![3, 52].contains(&o.0));
    d.obj(
        3,
        TIMELINE,
        cat(&[
            &[0, 0],
            &r(1),
            &u32b(6),
            &r(10),
            &r(20),
            &r(300),
            &r(400),
            &r(500),
            &r(600),
        ]),
    );
    let tag =
        |key: &str, v: u64| cat(&[&s8(key), &s8("IntrinsicMetaTypeuint64"), &v.to_le_bytes()]);
    d.obj(
        52,
        SKETCH_LINE,
        cat(&[
            &[0, 1],
            &u32b(2),
            &tag("crv_primary_id", 103),
            &tag("crv_secondary_id", 0),
            &f64s(&[0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, -1.0]),
            &r(51),
            &r(50),
            &r(40),
        ]),
    );
    // The revolution: a cut about the sketch's line, of the profile whose
    // one loop is that line's piece.
    d.obj(
        300,
        REVOLVE,
        cat(&[
            &[0, 0],
            &u32b(2),
            &u32b(2),
            &r(310),
            &r(311),
            &r(312),
            &tail(5, "Revolve", 1, "", [0, 0, 0], 301),
        ]),
    );
    d.obj(301, HEALTH, vec![0, 0]);
    d.obj(
        310,
        REVOLVE_PROFILE,
        cat(&[&[0, 0], &r(300), &u32b(1), &r(315)]),
    );
    d.obj(315, PROFILE_ID, vec![0, 0]);
    d.obj(311, PROFILE_SOURCE, cat(&[&[0, 0], &r(314), &s16("40")]));
    let mut record = u32b(2);
    record.extend(103u64.to_le_bytes());
    record.extend(0u64.to_le_bytes());
    record.extend(u32b(0));
    record.extend(u32b(1));
    record.extend(u32b(1));
    record.extend(0u64.to_le_bytes());
    d.obj(
        314,
        PROFILE_LOOPS,
        cat(&[
            &[0, 0],
            &r(311),
            &u32b(1),
            &u32b(1),
            &u32b(1),
            &record,
            &[1],
            &u32b(0),
        ]),
    );
    d.obj(312, ENTITY_REF, cat(&[&[0, 0], &r(313)]));
    d.obj(
        313,
        SKETCH_CURVE_ID,
        cat(&[
            &[0, 0],
            &0u64.to_le_bytes(),
            &40u64.to_le_bytes(),
            &103u64.to_le_bytes(),
        ]),
    );
    // A mirror that makes one new body: not joined with its image. Its
    // feature selection stays decoded even though its plane is absent.
    d.obj(
        400,
        MIRROR,
        cat(&[
            &[0, 0],
            &r(401),
            &r(402),
            &u32b(0),
            &u32b(1),
            &r(403),
            &u32b(0),
            &u32b(1),
            &r(405),
            &tail(6, "Mirror", 1, "", [0, 0, 0], 404),
        ]),
    );
    for id in [401, 402, 404] {
        d.obj(id, HEALTH, vec![0, 0]);
    }
    d.obj(403, BODY_RECORD, vec![0, 0]);
    d.obj(405, BODY_INPUT, cat(&[&[0, 0], &u32b(1), &r(406)]));
    d.obj(406, ENTITY_REF, cat(&[&[0, 0], &r(407)]));
    d.obj(407, REF_TARGET, cat(&[&[0, 0], &20u64.to_le_bytes()]));
    // A circular pattern about a construction axis off the origin, whose
    // direction input stores the direction times the axis' length.
    d.obj(
        500,
        CIRCULAR,
        cat(&[
            &[0, 0],
            &u32b(1),
            &r(501),
            &tail(7, "C-Pattern", 1, "", [0, 0, 0], 502),
        ]),
    );
    d.obj(502, HEALTH, vec![0, 0]);
    d.obj(
        501,
        DIRECTION_INPUT,
        cat(&[
            &[0, 0],
            &u32b(0),
            &f64s(&[1.0, 2.0, 0.0, 0.0, 0.0, 5.0, 0.0, 0.0]),
            &u32b(7),
        ]),
    );
    // A hole (class version 4) with a depth and no slot of role 5: a
    // distance, whatever its bytes.
    d.obj(
        600,
        HOLE,
        cat(&[
            &[0, 0],
            &[1, 1, 1, 0, 1, 0, 0, 0],
            &tail(8, "Hole", 1, "", [0, 0, 0], 601),
        ]),
    );
    d.obj(601, HEALTH, vec![0, 0]);
    d.obj(602, PARAMETER_HOLDER, cat(&[&[1], &u32b(1), &r(600), &[0]]));
    let p = parameter(20, Some(602), "6 mm", "HoleDepth", "", "mm", "d20", 0.6);
    d.obj(603, PARAMETER, p);
    let (m, b) = d.streams();
    let v = serde_json::to_value(Design::parse(&m, b).unwrap().dump("x", "Design1")).unwrap();
    let items = v["timeline"]["items"].as_array().unwrap();
    let rev = &items[2]["detail"];
    assert_eq!(items[2]["objectType"], "RevolveFeature");
    assert_eq!(rev["operation"], "CutFeatureOperation");
    assert_eq!(
        rev["axis"],
        json!({"kind": "sketch_entity", "objectType": "SketchLine", "sketch": "Sketch1",
               "sketch_timeline_index": 0, "id": "c0"})
    );
    assert_eq!(rev["profile"].as_array().unwrap().len(), 1);
    assert_eq!(
        rev["_f3d_profile_loops"],
        json!([{"loops": [{"outer": true, "curves": [{"primary": 103, "secondary": 0,
                 "tag": 2, "reversed": false, "piece": 1, "pieces": 1, "id": "c0"}]}],
                "children": []}])
    );
    assert_eq!(items[3]["detail"]["isCombine"], false);
    assert_eq!(items[3]["detail"]["inputEntities"][0]["timeline_index"], 1);
    assert_eq!(
        items[3]["detail"]["patternEntityType"],
        "FeaturesPatternType"
    );
    assert!(items[3]["detail"]["mirrorPlane"].is_null());
    let pattern = &items[4]["detail"];
    assert_eq!(
        pattern["_f3d_axis"],
        json!({"origin": [1.0, 2.0, 0.0], "direction": [0.0, 0.0, 1.0]})
    );
    let hole = &items[5]["detail"];
    assert_eq!(
        hole["extentDefinition"]["_type"],
        "DistanceExtentDefinition"
    );
    // The byte after the root part that the earlier decoder took for a
    // through-all flag decides nothing: the hole has no slot of role 5.
    assert!(hole["_f3d_through_all"].is_null());
}

/// Bodies named by the item that made them (mitcad#96): a body input's
/// record names its producer, and the body's index is the record's
/// position among the producer's own records (else among its records in
/// object order). A body without a recipe that decodes still comes in;
/// a join's or cut's body inputs are its participants.
#[test]
fn bodies_by_the_items_that_made_them() {
    const RECIPE: &str = "7ACC2A03-0261-4879-A14A-A93D661A5BDC";
    const COMBINE: &str = "2A94257F-2020-4B19-9A68-103A2672F1B7";
    const HOLE: &str = "1C037A07-4A15-43F6-ABFC-BBF61B9038D4";
    let mut d = sample();
    for (g, v) in [
        (COMBINE, 1),
        (HOLE, 7),
        (BODY_INPUT, 1),
        (BODY_REF, 1),
        (BODY_RECORD, 2),
        (PLACEMENT, 5),
        (RECIPE, 1),
    ] {
        d.class(g, ROOT, v);
    }
    d.objects.retain(|o| o.0 != 3);
    d.obj(
        3,
        TIMELINE,
        cat(&[
            &[0, 0],
            &r(1),
            &u32b(5),
            &r(10),
            &r(20),
            &r(500),
            &r(600),
            &r(700),
        ]),
    );
    let mut recipe = face_recipe("2", &[3]);
    let n = recipe.len();
    recipe.truncate(n - 20);
    recipe.extend(s8("body_recipe_data"));
    // The tool names no recipe, only its record; the target both.
    d.obj(510, BODY_INPUT, cat(&[&[0, 0], &u32b(1), &r(511)]));
    d.obj(511, BODY_REF, cat(&[&[0, 0], &r(541)]));
    d.obj(520, BODY_INPUT, cat(&[&[0, 0], &u32b(1), &r(521)]));
    d.obj(521, BODY_REF, cat(&[&[0, 0], &r(522), &r(550)]));
    d.obj(522, RECIPE, recipe);
    // Records: two of the extrusion's (not listed by it: object order),
    // two of the hole's (listed in the other order).
    for (id, producer) in [(540, 20), (541, 20), (550, 600), (551, 600)] {
        d.obj(id, BODY_RECORD, cat(&[&[0, 0], &r(producer)]));
    }
    d.obj(531, PLACEMENT, vec![0, 0]);
    d.obj(
        500,
        COMBINE,
        cat(&[
            &[0, 0],
            &u32b(0),
            &u32b(0),
            &[0, 0],
            &u32b(1),
            &r(531),
            &u32b(0x132),
            &u32b(0),
            &u32b(1),
            &r(510),
            &[0; 8],
            &u32b(1),
            &r(541),
            &u32b(1),
            &r(520),
            &u32b(4),
            &r(510),
            &r(511),
            &r(520),
            &r(521),
            &tail(5, "Combine", 1, "", [0, 0, 0], 501),
        ]),
    );
    d.obj(501, HEALTH, vec![0, 0]);
    d.obj(
        600,
        HOLE,
        cat(&[
            &[0, 0],
            &u32b(0),
            &[1, 1, 1, 0],
            &r(551),
            &r(550),
            &u32b(0),
            &tail(6, "Hole", 1, "", [0, 0, 0], 601),
        ]),
    );
    d.obj(601, HEALTH, vec![0, 0]);
    // A cut whose participant is the extrusion's second body.
    d.obj(701, BODY_REF, cat(&[&[0, 0], &r(541)]));
    d.obj(
        700,
        EXTRUDE,
        cat(&[
            &[0, 0],
            &[0, 0, 0],
            &u32b(2),
            &u32b(1),
            &u32b(2),
            &[0, 1],
            &u32b(0),
            &f64s(&[0.0, 0.0, 1.0]),
            &[0, 0],
            &u32b(1),
            &r(701),
            &tail(7, "Extrude", 2, "", [0, 0, 0], 702),
        ]),
    );
    d.obj(702, HEALTH, vec![0, 0]);
    let (m, b) = d.streams();
    let v = serde_json::to_value(Design::parse(&m, b).unwrap().dump("x", "Design1")).unwrap();
    let items = v["timeline"]["items"].as_array().unwrap();
    let c = &items[2]["detail"];
    assert_eq!(c["operation"], "JoinFeatureOperation");
    let tool = &c["toolBodies"][0]["_f3d"];
    assert_eq!(
        (&tool["producer"], &tool["body_index"]),
        (&json!(1), &json!(1))
    );
    assert!(tool.get("recipe").is_none() && tool.get("entities").is_none());
    let target = &c["targetBody"]["_f3d"];
    assert_eq!(target["entities"][0][0]["tag"], "2");
    assert_eq!(
        (&target["producer"], &target["body_index"]),
        (&json!(3), &json!(1))
    );
    let x = &items[4]["detail"];
    assert_eq!(x["operation"], "CutFeatureOperation");
    let p = &x["participantBodies"][0]["_f3d"];
    assert_eq!((&p["producer"], &p["body_index"]), (&json!(1), &json!(1)));
}

/// A revolution's flag bytes (the API reference dumps' 103 revolutions and
/// the controlled models): `u32 operation | u32 2 | u8 lead | [u8 project
/// axis] | u8 solid | u8 symmetric | u8 two sides | [second holder or 00] |
/// holder`, the project-axis byte in class version 2 only.
#[test]
fn revolve_record_flags() {
    const REVOLVE: &str = "E3849A15-2FC6-42A0-AF3A-2F1D7273B406";
    let dump_of = |version: u32, bytes: &[u8]| {
        let mut d = sample();
        d.class(REVOLVE, ROOT, version);
        d.objects.retain(|o| o.0 != 3);
        d.obj(
            3,
            TIMELINE,
            cat(&[&[0, 0], &r(1), &u32b(3), &r(10), &r(20), &r(300)]),
        );
        d.obj(
            300,
            REVOLVE,
            cat(&[
                &[0, 0],
                &u32b(2),
                &u32b(2),
                bytes,
                &[0],
                &r(302),
                &tail(5, "Revolve", 1, "", [0, 0, 0], 301),
            ]),
        );
        d.obj(301, HEALTH, vec![0, 0]);
        d.obj(302, PARAMETER_HOLDER, cat(&[&[1], &u32b(1), &r(300), &[0]]));
        let p = parameter(
            20,
            Some(302),
            "90 deg",
            "AlongAngle",
            "",
            "deg",
            "d20",
            1.57,
        );
        d.obj(303, PARAMETER, p);
        let (m, b) = d.streams();
        let v = serde_json::to_value(Design::parse(&m, b).unwrap().dump("x", "Design1")).unwrap();
        v["timeline"]["items"][2]["detail"].clone()
    };
    // Version 2: lead, project axis, solid, symmetric, two sides.
    let detail = dump_of(2, &[0, 1, 1, 0, 0]);
    assert_eq!(detail["isProjectAxis"], true);
    assert_eq!(detail["isSolid"], true);
    assert_eq!(detail["extentDefinition"]["isSymmetric"], false);
    assert_eq!(detail["extentDefinition"]["_type"], "AngleExtentDefinition");
    let detail = dump_of(2, &[1, 0, 1, 1, 0]);
    assert_eq!(detail["isProjectAxis"], false);
    assert_eq!(detail["extentDefinition"]["isSymmetric"], true);
    // Two sides: the two angles are the extent's.
    let detail = dump_of(2, &[0, 1, 1, 0, 1]);
    assert_eq!(
        detail["extentDefinition"]["_type"],
        "TwoSidesAngleExtentDefinition"
    );
    // Version 1 has no project-axis byte.
    let detail = dump_of(1, &[0, 1, 0, 0]);
    assert!(detail["isProjectAxis"].is_null());
    assert_eq!(detail["isSolid"], true);
    assert_eq!(detail["extentDefinition"]["isSymmetric"], false);
    // A byte that is no flag: nothing is read.
    let detail = dump_of(2, &[0, 1, 7, 0, 0]);
    assert!(detail["isSolid"].is_null());
    assert!(detail["extentDefinition"]["isSymmetric"].is_null());
}

/// A shell's two bytes after the root part: the tangent-chain flag and the
/// shell type (1 rounded, 0 sharp).
#[test]
fn shell_record_flags() {
    const SHELL: &str = "E8063467-BC05-4688-8196-AECB90F0EFEE";
    for (bytes, chain, kind) in [
        ([1u8, 1u8], json!(true), json!("RoundedOffsetShellType")),
        ([1, 0], json!(true), json!("SharpOffsetShellType")),
        ([0, 0], json!(false), json!("SharpOffsetShellType")),
        ([2, 0], json!(null), json!(null)),
    ] {
        let mut d = sample();
        d.class(SHELL, ROOT, 3);
        d.objects.retain(|o| o.0 != 3);
        d.obj(
            3,
            TIMELINE,
            cat(&[&[0, 0], &r(1), &u32b(3), &r(10), &r(20), &r(300)]),
        );
        d.obj(
            300,
            SHELL,
            cat(&[
                &[0, 0],
                &bytes,
                &[0; 8],
                &tail(5, "Shell", 1, "", [0, 0, 0], 301),
            ]),
        );
        d.obj(301, HEALTH, vec![0, 0]);
        let (m, b) = d.streams();
        let v = serde_json::to_value(Design::parse(&m, b).unwrap().dump("x", "Design1")).unwrap();
        let detail = &v["timeline"]["items"][2]["detail"];
        assert_eq!(detail["isTangentChain"], chain, "{bytes:?}");
        assert_eq!(detail["shellType"], kind, "{bytes:?}");
    }
}

/// A termination body is not a participant. Missing or unresolved slots
/// retain the older input-list reading; an explicit empty slot is kept.
#[test]
fn extrusion_participants_use_their_slot() {
    const SLOT_KEY: &str = "2DF7DA30-A4E4-4260-AFD1-D9C7154CB44B";
    for (selected, expected) in [
        (None, vec![701, 702]),
        (Some(vec![701]), vec![701]),
        (Some(vec![]), vec![]),
        (Some(vec![703]), vec![701, 702]),
    ] {
        let mut d = sample();
        for (g, v) in [
            (BODY_INPUT, 1),
            (BODY_REF, 1),
            (BODY_RECORD, 2),
            (SLOT_KEY, 1),
        ] {
            d.class(g, ROOT, v);
        }
        d.objects.retain(|o| o.0 != 3);
        d.obj(
            3,
            TIMELINE,
            cat(&[&[0, 0], &r(1), &u32b(3), &r(10), &r(20), &r(700)]),
        );
        // Two distinct bodies made by the preceding extrusion.
        for (input, record) in [(701, 710), (702, 711)] {
            d.obj(input, BODY_REF, cat(&[&[0, 0], &r(record)]));
            d.obj(record, BODY_RECORD, cat(&[&[0, 0], &r(20)]));
        }
        d.obj(703, BODY_REF, vec![0, 0]);
        let mut refs = vec![r(701), r(702)];
        for (slot, key, role, inputs) in [
            (720, 730, decode::PARTICIPANT_ROLE, selected),
            (721, 731, 5, Some(vec![702])),
        ] {
            let Some(inputs) = inputs else { continue };
            let mut body = cat(&[&[0, 0], &u32b(inputs.len() as u32)]);
            body.extend(inputs.into_iter().flat_map(r));
            body.extend(cat(&[&[0; 6], &r(key), &u32b(0), &u32b(role), &[0; 4]]));
            d.obj(slot, BODY_INPUT, body);
            d.obj(key, SLOT_KEY, vec![0, 0]);
            refs.push(r(slot));
        }
        d.obj(
            700,
            EXTRUDE,
            cat(&[
                &[0, 0],
                &[0, 0, 0],
                &u32b(2),
                &u32b(1),
                &u32b(0),
                &[0, 1],
                &u32b(0),
                &f64s(&[0.0, 0.0, 1.0]),
                &[0, 0],
                &u32b(refs.len() as u32),
                &refs.concat(),
                &tail(7, "Extrude", 2, "", [0, 0, 0], 704),
            ]),
        );
        d.obj(704, HEALTH, vec![0, 0]);
        let (m, b) = d.streams();
        let v = serde_json::to_value(Design::parse(&m, b).unwrap().dump("x", "Design1")).unwrap();
        let participants = v["timeline"]["items"][2]["detail"]["participantBodies"]
            .as_array()
            .unwrap();
        let actual: Vec<_> = participants
            .iter()
            .map(|p| p["_f3d"]["object_id"].as_u64().unwrap())
            .collect();
        assert_eq!(actual, expected);
    }
}

/// An extrusion up to a face (mitcad#96): the flag byte after the extent
/// codes, the direction vector after a `u32 1`, and the input slots whose
/// roles say which input is the extent's object (17) and which the profile
/// (65).
#[test]
fn extrusion_up_to_a_face() {
    const RECIPE: &str = "7ACC2A03-0261-4879-A14A-A93D661A5BDC";
    const SLOT_KEY: &str = "2DF7DA30-A4E4-4260-AFD1-D9C7154CB44B";
    let mut d = sample();
    for g in [BODY_INPUT, FACE_REF, RECIPE, SLOT_KEY] {
        d.class(g, ROOT, 1);
    }
    d.objects.retain(|o| o.0 != 3);
    d.obj(
        3,
        TIMELINE,
        cat(&[&[0, 0], &r(1), &u32b(3), &r(10), &r(20), &r(700)]),
    );
    d.obj(
        700,
        EXTRUDE,
        cat(&[
            &[0, 0],
            &[0, 0, 0],
            &u32b(2),
            &u32b(1),
            &u32b(1),
            &[1, 1],
            &u32b(1),
            &f64s(&[0.0, 0.0, -1.0]),
            &[0, 0],
            &r(70),
            &r(730),
            &r(740),
            &r(741),
            &tail(7, "Extrude", 2, "", [0, 0, 0], 701),
        ]),
    );
    d.obj(701, HEALTH, vec![0, 0]);
    d.obj(730, PARAMETER_HOLDER, cat(&[&[1], &u32b(1), &r(700), &[0]]));
    let p = parameter(5, Some(730), "0 mm", "Side1Offset", "", "mm", "d5", 0.0);
    d.obj(731, PARAMETER, p);
    for (slot, input, key, role) in [(740, 750, 760, 17), (741, 70, 761, 65)] {
        d.obj(
            slot,
            BODY_INPUT,
            cat(&[
                &[0, 0],
                &u32b(1),
                &r(input),
                &[0; 6],
                &r(key),
                &u32b(0),
                &u32b(role),
                &[0; 4],
            ]),
        );
        d.obj(key, SLOT_KEY, vec![0, 0]);
    }
    d.obj(750, FACE_REF, cat(&[&[0, 0], &r(751)]));
    d.obj(751, RECIPE, face_recipe("9", &[5]));
    // Symmetric extrusions, half and whole length: `u8 0 | 1 or 2 | 14 × 0
    // | u32 1` before the first input slot.
    for (id, length) in [(800, 2u8), (810, 1)] {
        let mut gap = vec![0, length];
        gap.extend([0; 14]);
        d.obj(
            id,
            EXTRUDE,
            cat(&[
                &[0, 0],
                &[0, 0, 0],
                &u32b(4),
                &u32b(3),
                &u32b(2),
                &[0, 1],
                &u32b(0),
                &f64s(&[0.0, 0.0, 1.0]),
                &r(30),
                &gap,
                &u32b(1),
                &r(741),
                &tail(8, "Extrude", 3, "", [0, 0, 0], 701),
            ]),
        );
    }

    // One-sided distance extrusions with the thin-wall block (mitcad#186):
    // `7 slots | u32 1 | 9 × 0 | u32 0 | 6 × 0 | slot | 5 bytes | u8 thin
    // | u32 location | slot | 5 × 0 | u32 1 | input slot`; a thin one names
    // the thickness's parameter holder, a solid one has no slot there.
    for (id, thin, location, holder) in [
        (900, 1u8, 1u32, Some(730)),
        (910, 0, 0, None),
        (920, 1, 2, Some(730)),
        // Inconsistent: thin without a holder, solid with a location.
        (930, 1, 0, None),
        (940, 0, 2, None),
    ] {
        let slot = holder.map_or(vec![0], r);
        d.obj(
            id,
            EXTRUDE,
            cat(&[
                &[0, 0],
                &[0, 0, 0],
                &u32b(1),
                &u32b(1),
                &u32b(2),
                &[0, 1],
                &u32b(0),
                &f64s(&[0.0, 0.0, 1.0]),
                &[0; 7],
                &u32b(1),
                &[0; 9],
                &u32b(0),
                &[0; 6],
                &[0],
                &[0; 5],
                &[thin],
                &u32b(location),
                &slot,
                &[0; 5],
                &u32b(1),
                &r(741),
                &tail(8, "Extrude", 3, "", [0, 0, 0], 701),
            ]),
        );
    }

    // The words between the two sides (mitcad#186): nine zero bytes in the
    // older writer, `u8 0 | u32 a | u32 b` in the newer one (`a` 2 for a
    // side through all, `b` 1 for two sides and symmetric extents).
    let words = |a: u32, b: u32| cat(&[&[0], &u32b(a), &u32b(b)]);
    for (id, code, one, middle, two) in [
        (950, 2u32, 1u32, words(0, 1), 1u32),
        (951, 3, 1, vec![0; 9], 1),
        (952, 3, 1, words(0, 1), 0),
        (953, 2, 4, words(2, 1), 4),
        (954, 1, 4, words(2, 0), 0),
        // Not a layout of any writer.
        (955, 2, 1, words(7, 1), 1),
    ] {
        d.obj(
            id,
            EXTRUDE,
            cat(&[
                &[0, 0],
                &[0, 0, 0],
                &u32b(4),
                &u32b(code),
                &u32b(1),
                &[0, 1],
                &u32b(0),
                &f64s(&[0.0, 0.0, 1.0]),
                &[0; 7],
                &u32b(one),
                &middle,
                &u32b(two),
                &[0; 6],
                &[0],
                &[0; 5],
                &[0],
                &u32b(0),
                &[0],
                &[0; 5],
                &u32b(1),
                &r(741),
                &tail(8, "Extrude", 3, "", [0, 0, 0], 701),
            ]),
        );
    }

    // Sides up to an object: the byte after the object's reference is its
    // nearest-solution flag; the two-sided layout holds the second side's the
    // same way (the API reference dumps: `[m1] [3 × 0] [u32 1] [u32 2] [u8 0]
    // [ref] [m2]`).
    let reference = r(741);
    for (id, code, middle) in [
        (960, 1u32, cat(&[&reference, &[1], &[0; 20]])),
        (
            961,
            2,
            cat(&[
                &reference,
                &[0],
                &[0; 3],
                &u32b(1),
                &u32b(2),
                &[0],
                &reference,
                &[1],
                &[0; 6],
            ]),
        ),
        (962, 1, cat(&[&reference, &[5], &[0; 20]])),
    ] {
        d.obj(
            id,
            EXTRUDE,
            cat(&[
                &[0, 0],
                &[0, 0, 0],
                &u32b(4),
                &u32b(code),
                &u32b(1),
                &[0, 1],
                &u32b(0),
                &f64s(&[0.0, 0.0, 1.0]),
                &[0; 7],
                &u32b(2),
                &middle,
                &[0; 6],
                &[0],
                &[0; 5],
                &[0],
                &u32b(0),
                &[0],
                &[0; 5],
                &u32b(1),
                &r(741),
                &tail(8, "Extrude", 3, "", [0, 0, 0], 701),
            ]),
        );
    }

    // An extrusion that starts on a face (mitcad#641): the start face's
    // selection takes the place of the first byte after the root part, and
    // the fields follow it (a cut, one side by a distance, start 2).
    d.obj(
        970,
        EXTRUDE,
        cat(&[
            &[0, 0],
            &r(741),
            &[0, 1],
            &u32b(2),
            &u32b(1),
            &u32b(2),
            &[0, 1],
            &u32b(2),
            &f64s(&[0.0, 0.0, 1.0]),
            &[0; 7],
            &u32b(1),
            &[0; 9],
            &u32b(0),
            &[0; 6],
            &[0],
            &[0; 5],
            &[0],
            &u32b(0),
            &[0],
            &[0; 5],
            &u32b(1),
            &r(741),
            &tail(8, "Extrude", 4, "", [0, 0, 0], 701),
        ]),
    );

    let seg = d.segment();
    let started = decode::extrude_fields(&seg, seg.object(970).unwrap()).unwrap();
    assert_eq!(
        (
            started.operation_code,
            started.extent_a,
            started.extent_b,
            started.start,
            started.start_entity,
            started.vector
        ),
        (2, 1, 2, Some(2), Some(741), Some([0.0, 0.0, 1.0]))
    );
    let sides = |id| decode::extrude_fields(&seg, seg.object(id).unwrap()).and_then(|f| f.sides);
    assert_eq!(sides(950), Some([1, 1]));
    assert_eq!(sides(951), Some([1, 1]));
    assert_eq!(sides(952), Some([1, 0]));
    assert_eq!(sides(953), Some([4, 4]));
    assert_eq!(sides(954), Some([4, 0]));
    assert_eq!(sides(955), None);
    let minimum =
        |id| decode::extrude_fields(&seg, seg.object(id).unwrap()).map(|f| (f.sides, f.minimum));
    assert_eq!(minimum(960), Some((Some([2, 0]), [Some(true), None])));
    assert_eq!(
        minimum(961),
        Some((Some([2, 0]), [Some(false), Some(true)]))
    );
    assert_eq!(minimum(962), Some((Some([2, 0]), [None, None])));
    assert_eq!(minimum(954).map(|m| m.1), Some([None, None]));
    let thin = |id| {
        let f = decode::extrude_fields(&seg, seg.object(id).unwrap()).unwrap();
        (f.thin, f.thin_location, f.thin_holder, f.sides)
    };
    assert_eq!(thin(900), (Some(true), Some(1), Some(730), Some([1, 0])));
    assert_eq!(thin(910), (Some(false), Some(0), None, Some([1, 0])));
    assert_eq!(thin(920), (Some(true), Some(2), Some(730), Some([1, 0])));
    assert_eq!(thin(930), (None, None, None, Some([1, 0])));
    assert_eq!(thin(940), (None, None, None, Some([1, 0])));
    let dec = decode::decode(&seg);
    let f = dec.timeline[2].extrude.unwrap();
    assert_eq!((f.extent_a, f.extent_b, f.flag), (1, 1, Some(1)));
    assert_eq!(f.vector, Some([0.0, 0.0, -1.0]));
    // The sample's distance extrusion: flag 0, no slots.
    assert_eq!(dec.timeline[1].extrude.unwrap().flag, Some(0));
    assert!(decode::extent_slots(&seg, 20).is_empty());
    let length =
        |id| decode::extrude_fields(&seg, seg.object(id).unwrap()).and_then(|f| f.full_length);
    assert_eq!(
        (length(800), length(810), length(20)),
        (Some(false), Some(true), None)
    );
    let slots = decode::extent_slots(&seg, 700);
    assert_eq!(slots.len(), 2);
    assert!(slots[0].is_to_object() && slots[0].inputs == [750]);
    assert_eq!((slots[1].role, slots[1].inputs.as_slice()), (65, &[70][..]));

    let (m, b) = d.streams();
    let v = serde_json::to_value(Design::parse(&m, b).unwrap().dump("x", "Design1")).unwrap();
    let item = &v["timeline"]["items"][2];
    assert_eq!(item["_f3d"]["extrude"]["flag"], 1);
    assert_eq!(item["_f3d"]["extrude"]["slot_roles"], json!([17, 65]));
    let one = &item["detail"]["extentOne"];
    assert_eq!(one["_type"], "ToEntityExtentDefinition");
    assert_eq!(one["offset"]["name"], "d5");
    assert_eq!(one["entity"]["kind"], "face");
    assert_eq!(one["entity"]["_f3d"]["object_id"], 750);
    assert_eq!(one["entity"]["_f3d"]["entities"][0][0]["tag"], "9");
    // The distance extrusion keeps its extent and names no object.
    let one = &v["timeline"]["items"][1]["detail"]["extentOne"];
    assert_eq!(one["_type"], "DistanceExtentDefinition");
    assert!(one.get("entity").is_none());
}

/// A recipe naming entities by `(tag, ops)` names, one name each.
fn recipe(kind: &str, entities: &[(&str, i32)], second: &[(&str, i32)]) -> Vec<u8> {
    let list = |es: &[(&str, i32)]| {
        let mut b = u32b(es.len() as u32);
        for (tag, op) in es {
            b.extend(cat(&[
                &u32b(1),
                &s8(tag),
                &u32b(0),
                &u32b(1),
                &op.to_le_bytes(),
                &u32b(0),
            ]));
        }
        b
    };
    cat(&[
        &[0, 0],
        &u32b(1),
        &u32b(3),
        &list(entities),
        &list(second),
        &s8(&format!("{kind}_recipe_data")),
        &[0xff; 8],
    ])
}

#[test]
fn fillet_tail_and_face_selections() {
    // mitcad#96: a fillet that refers to a health object near its start
    // too (its tail ends at the last one); its set of an edge and a face
    // in two groups (8: edges, 16: faces) that share the holders after
    // them; a group of a face the next group repeats (9) left out; an edge
    // recipe with a second entity list; an edge named first by its own
    // (negative) tag.
    const FILLET: &str = "A07D5F17-68CB-464D-9935-BF68E98A865F";
    const RECIPE: &str = "7ACC2A03-0261-4879-A14A-A93D661A5BDC";
    const SELECTS: &str = "2DF7DA30-0000-0000-0000-000000000000";
    const OTHER: &str = "4A557CE2-0000-0000-0000-000000000000";
    let mut d = sample();
    for (g, v) in [
        (FILLET, 3),
        (RECIPE, 1),
        (FACE_REF, 2),
        (BODY_INPUT, 1),
        (BODY_RECORD, 2),
        (SELECTS, 1),
        (OTHER, 1),
    ] {
        d.class(g, ROOT, v);
    }
    d.objects.retain(|o| o.0 != 3);
    d.obj(
        3,
        TIMELINE,
        cat(&[&[0, 0], &r(1), &u32b(3), &r(10), &r(20), &r(700)]),
    );
    d.obj(
        700,
        FILLET,
        cat(&[
            &[0, 0],
            &r(701),
            &u32b(0),
            &r(702),
            &[0, 1, 0, 0, 0],
            &r(710),
            &u32b(9),
            &r(710),
            &r(711),
            &r(712),
            &r(720),
            &r(721),
            &r(730),
            &r(731),
            &r(740),
            &r(741),
            &tail(9, "Fillet", 1, "", [0, 0, 0], 703),
        ]),
    );
    d.obj(701, OTHER, vec![0, 0]);
    d.obj(702, HEALTH, vec![0, 0]);
    d.obj(703, HEALTH, vec![0, 0]);
    d.obj(709, SELECTS, vec![0, 0]);
    let set = |k: u32| {
        cat(&[
            &[0, 0],
            &u32b(0),
            &[0, 0, 1, 0, 0, 0],
            &r(709),
            &u32b(0),
            &u32b(k),
        ])
    };
    d.obj(710, BODY_INPUT, set(8));
    d.obj(720, BODY_INPUT, set(9));
    d.obj(730, BODY_INPUT, set(16));
    // The first edge refers to its body's record, made by the second item
    // of the timeline (the others to none).
    d.obj(760, BODY_RECORD, cat(&[&[0, 0], &r(20)]));
    d.obj(711, FACE_REF, cat(&[&[0, 0], &r(751), &r(760)]));
    for (input, rec) in [(712, 752), (721, 753), (731, 754)] {
        d.obj(input, FACE_REF, cat(&[&[0, 0], &r(rec)]));
    }
    d.obj(
        751,
        RECIPE,
        recipe("edge", &[("3", 301), ("4", 301)], &[("", 340)]),
    );
    d.obj(
        752,
        RECIPE,
        recipe("edge", &[("-1029", 416), ("3", 301), ("1", 301)], &[]),
    );
    d.obj(753, RECIPE, recipe("face", &[("2", 301)], &[]));
    d.obj(
        754,
        RECIPE,
        recipe("bounded_face", &[("2", 301), ("3", 301), ("4", 301)], &[]),
    );
    d.obj(740, PARAMETER_HOLDER, cat(&[&[1], &u32b(1), &r(700), &[0]]));
    d.obj(741, PARAMETER_HOLDER, cat(&[&[1], &u32b(1), &r(700), &[0]]));
    let p = parameter(30, Some(740), "1 mm", "Radius", "", "mm", "d30", 0.1);
    d.obj(742, PARAMETER, p);
    let (m, b) = d.streams();
    let v = serde_json::to_value(Design::parse(&m, b).unwrap().dump("x", "Design1")).unwrap();
    let item = &v["timeline"]["items"][2];
    assert_eq!(item["objectType"], "FilletFeature");
    assert_eq!(item["name"], "Fillet1");
    assert_eq!(item["_f3d"]["result_no"], 9);
    let sets = item["detail"]["edgeSets"].as_array().unwrap();
    assert_eq!(sets.len(), 1);
    let edges = sets[0]["edges"].as_array().unwrap();
    let kinds: Vec<(&str, &str)> = edges
        .iter()
        .map(|e| {
            (
                e["kind"].as_str().unwrap(),
                e["_f3d"]["recipe"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        kinds,
        [("edge", "edge"), ("edge", "edge"), ("face", "bounded_face")]
    );
    // The second list is left out; the edge's own name stays first.
    assert_eq!(edges[0]["_f3d"]["entities"].as_array().unwrap().len(), 2);
    assert_eq!(edges[1]["_f3d"]["entities"][0][0]["tag"], "-1029");
    // The edge on a body says which item made the body; an edge without a
    // record says nothing.
    assert_eq!(edges[0]["_f3d"]["producer"], 1);
    assert_eq!(edges[0]["_f3d"]["body_index"], 0);
    assert!(edges[1]["_f3d"].get("producer").is_none());
}

#[test]
fn dressup_sets_follow_holder_links_after_reordering() {
    const FILLET: &str = "A07D5F17-68CB-464D-9935-BF68E98A865F";
    const CHAMFER: &str = "F757D611-217B-4B72-9C41-B617BDCE43DA";
    const RECIPE: &str = "7ACC2A03-0261-4879-A14A-A93D661A5BDC";
    const SELECTS: &str = "2DF7DA30-A4E4-4260-AFD1-D9C7154CB44B";
    // The fillet's set records of the three layouts (the class version of
    // the set, a block of zeros before its input) and the chamfer's.
    for (class, set_class, role, size_key, version, gap) in [
        (FILLET, FILLET_SET, "Radius", "radius", 10, 0),
        (FILLET, FILLET_SET, "Radius", "radius", 10, 20),
        (FILLET, FILLET_SET, "Radius", "radius", 9, 0),
        (FILLET, FILLET_SET, "Radius", "radius", 4, 0),
        (CHAMFER, CHAMFER_SET, "Distance", "distance", 2, 0),
    ] {
        let mut d = sample();
        for (g, v) in [
            (class, 3),
            (set_class, version),
            (BODY_INPUT, 1),
            (FACE_REF, 2),
            (RECIPE, 1),
            (SELECTS, 1),
        ] {
            d.class(g, ROOT, v);
        }
        d.objects.retain(|o| o.0 != 3);
        d.obj(
            3,
            TIMELINE,
            cat(&[&[0, 0], &r(1), &u32b(3), &r(10), &r(20), &r(700)]),
        );
        let leading = if class == FILLET {
            cat(&[&u32b(2), &r(731), &r(730), &u32b(0), &[1]])
        } else {
            cat(&[&u32b(1), &u32b(2), &r(731), &r(730)])
        };
        // Parameter 740 precedes every input group; the option records and
        // the groups are in the opposite order to the size parameters.
        d.obj(
            700,
            class,
            cat(&[
                &[0, 0],
                &leading,
                &u32b(6),
                &r(740),
                &r(720),
                &r(721),
                &r(741),
                &r(710),
                &r(711),
                &tail(9, "Dressup", 1, "", [0, 0, 0], 703),
            ]),
        );
        d.obj(703, HEALTH, vec![0, 0]);
        d.obj(709, SELECTS, vec![0, 0]);
        for (group, edge, rec, option, holder, chain) in [
            (710, 711, 751, 730, 740, false),
            (720, 721, 752, 731, 741, true),
        ] {
            d.obj(
                group,
                BODY_INPUT,
                cat(&[
                    &[0, 0],
                    &u32b(1),
                    &r(edge),
                    &[0, 0, 1, 0, 0, 0],
                    &r(709),
                    &u32b(0),
                    &u32b(8),
                ]),
            );
            d.obj(edge, FACE_REF, cat(&[&[0, 0], &r(rec)]));
            d.obj(
                rec,
                RECIPE,
                recipe(
                    "edge",
                    &[(if chain { "4" } else { "3" }, 301), ("1", 301)],
                    &[],
                ),
            );
            let body = if class == FILLET {
                let chain = u8::from(chain);
                let options: &[u8] = match version {
                    10 => &[0, 0, 1, 0, 0, chain, 0, 0],
                    9 => &[0, 1, 0, chain, 0, 0],
                    _ => &[0, 1, 0, chain, 0],
                };
                cat(&[
                    &[0, 0],
                    options,
                    &vec![0; gap],
                    &r(group),
                    &[0; 23],
                    &r(holder),
                    &[0; 12],
                ])
            } else {
                // The byte after the root part is the tangent-chain option.
                cat(&[
                    &[0, 0],
                    &[u8::from(chain), 0, 0],
                    &r(holder),
                    &[0],
                    &u32b(1),
                    &r(group),
                ])
            };
            d.obj(option, set_class, body);
            d.obj(
                holder,
                PARAMETER_HOLDER,
                cat(&[&[1], &u32b(1), &r(700), &[0]]),
            );
        }
        for (id, number, holder, name, value) in
            [(742, 30, 740, "small", 0.1), (743, 40, 741, "large", 0.2)]
        {
            d.obj(
                id,
                PARAMETER,
                parameter(number, Some(holder), "1 mm", role, "", "mm", name, value),
            );
        }
        let read = |d: &Doc| {
            let (meta, bulk) = d.streams();
            serde_json::to_value(Design::parse(&meta, bulk).unwrap().dump("x", "Design1")).unwrap()
        };
        let v = read(&d);
        let sets = v["timeline"]["items"][2]["detail"]["edgeSets"]
            .as_array()
            .unwrap();
        assert_eq!(sets.len(), 2);
        assert_eq!(sets[0][size_key]["name"], "small");
        assert_eq!(sets[1][size_key]["name"], "large");
        assert_eq!(sets[0]["edges"][0]["_f3d"]["object_id"], 711);
        assert_eq!(sets[1]["edges"][0]["_f3d"]["object_id"], 721);
        assert_eq!(sets[0]["isTangentChain"], false);
        assert_eq!(sets[1]["isTangentChain"], true);
        if class == FILLET {
            assert_eq!(sets[0]["continuity"], "TangentSurfaceContinuityType");
        } else {
            // A chamfer's sets carry their own option; the feature's is
            // set only where the sets agree.
            assert_eq!(sets[0]["isTangentChain"], false);
            assert_eq!(sets[1]["isTangentChain"], true);
            assert!(
                v["timeline"]["items"][2]["detail"]
                    .get("isTangentChain")
                    .is_none_or(Value::is_null)
            );
            // Equal distances have no side to flip.
            assert!(sets[0].get("isFlipped").is_none());
        }
        // A truncated counted list must not become a partial selection.
        let input = d.objects.iter_mut().find(|o| o.0 == 710).unwrap();
        input.2[2..6].copy_from_slice(&2u32.to_le_bytes());
        let v = read(&d);
        assert!(v["timeline"]["items"][2]["detail"]["edgeSets"][0]["edges"].is_null());
    }
}

/// A two-sided extrusion with both sides up to faces (mitcad#104): a slot
/// of role 17 for each side, which go in order to the sides the extent
/// parameters make extents up to an entity (`Side1Offset`, `Side2Offset`).
#[test]
fn two_sided_extrusion_up_to_two_faces() {
    const RECIPE: &str = "7ACC2A03-0261-4879-A14A-A93D661A5BDC";
    const SLOT_KEY: &str = "2DF7DA30-A4E4-4260-AFD1-D9C7154CB44B";
    let mut d = sample();
    for g in [BODY_INPUT, FACE_REF, RECIPE, SLOT_KEY] {
        d.class(g, ROOT, 1);
    }
    d.objects.retain(|o| o.0 != 3);
    d.obj(
        3,
        TIMELINE,
        cat(&[&[0, 0], &r(1), &u32b(3), &r(10), &r(20), &r(700)]),
    );
    // Cut (2), two sides (2), the second code 1, flag 0.
    d.obj(
        700,
        EXTRUDE,
        cat(&[
            &[0, 0],
            &[0, 0, 0],
            &u32b(2),
            &u32b(2),
            &u32b(1),
            &[0, 1],
            &u32b(0),
            &f64s(&[1.0, 0.0, 0.0]),
            &[0, 0],
            &r(70),
            &r(730),
            &r(740),
            &r(742),
            &r(741),
            &tail(7, "Extrude", 2, "", [0, 0, 0], 701),
        ]),
    );
    d.obj(701, HEALTH, vec![0, 0]);
    d.obj(730, PARAMETER_HOLDER, cat(&[&[1], &u32b(1), &r(700), &[0]]));
    let p = parameter(5, Some(730), "0 mm", "Side1Offset", "", "mm", "d5", 0.0);
    d.obj(731, PARAMETER, p);
    let p = parameter(6, Some(730), "2 mm", "Side2Offset", "", "mm", "d6", 0.2);
    d.obj(732, PARAMETER, p);
    // The slots keep the second side first (the API reference dumps).
    for (slot, input, key, role) in [(740, 752, 760, 17), (742, 750, 762, 17), (741, 70, 761, 65)] {
        d.obj(
            slot,
            BODY_INPUT,
            cat(&[
                &[0, 0],
                &u32b(1),
                &r(input),
                &[0; 6],
                &r(key),
                &u32b(0),
                &u32b(role),
                &[0; 4],
            ]),
        );
        d.obj(key, SLOT_KEY, vec![0, 0]);
    }
    d.obj(750, FACE_REF, cat(&[&[0, 0], &r(751)]));
    d.obj(751, RECIPE, face_recipe("9", &[5]));
    d.obj(752, FACE_REF, cat(&[&[0, 0], &r(753)]));
    d.obj(753, RECIPE, face_recipe("4", &[6]));

    let (m, b) = d.streams();
    let v = serde_json::to_value(Design::parse(&m, b).unwrap().dump("x", "Design1")).unwrap();
    let item = &v["timeline"]["items"][2];
    assert_eq!(item["_f3d"]["extrude"]["slot_roles"], json!([17, 17, 65]));
    let detail = &item["detail"];
    assert_eq!(detail["extentType"], "TwoSidesFeatureExtentType");
    for (key, offset, object, tag) in [("extentOne", "d5", 750, "9"), ("extentTwo", "d6", 752, "4")]
    {
        let side = &detail[key];
        assert_eq!(side["_type"], "ToEntityExtentDefinition", "{key}");
        assert_eq!(side["offset"]["name"], offset);
        assert_eq!(side["entity"]["_f3d"]["object_id"], object);
        assert_eq!(side["entity"]["_f3d"]["entities"][0][0]["tag"], tag);
    }
}

/// A move's transform object that stores no matrix is the identity; one
/// that stores its matrix, or another layout, is not read as such.
#[test]
fn a_move_without_a_stored_matrix_is_the_identity() {
    let mut d = sample();
    d.class(MOVE_DATA, ROOT, 2);
    let with_matrix = |m: &[f64; 16]| {
        cat(&[
            &[0, 0],
            &[0; 22],
            &u32b(5),
            &f64s(m),
            &[0, 0, 1, 0, 0, 0, 0],
            &r(1),
        ])
    };
    // `05 00 00 00 | 01 00 00 01 00 00 00 00 | ref`: no matrix.
    d.obj(
        900,
        MOVE_DATA,
        cat(&[
            &[0, 0],
            &[0; 22],
            &u32b(5),
            &[1, 0, 0, 1, 0, 0, 0, 0],
            &r(1),
        ]),
    );
    let shifted = [
        1.0, 0.0, 0.0, 0.5, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ];
    d.obj(901, MOVE_DATA, with_matrix(&shifted));
    // The marker without a reference after it.
    d.obj(
        902,
        MOVE_DATA,
        cat(&[
            &[0, 0],
            &[0; 22],
            &u32b(5),
            &[1, 0, 0, 1, 0, 0, 0, 0],
            &[0; 4],
        ]),
    );
    // A root part whose only attribute is a text of 0x7FFF_FFFF characters
    // in a few bytes: no root part, so not the identity (its end once lay
    // past the data, mitcad#934).
    d.obj(
        903,
        MOVE_DATA,
        cat(&[
            &[0, 1],
            &u32b(1),
            &s8("name"),
            &s8("IntrinsicMetaTypeIString"),
            &u32b(0x7FFF_FFFF),
            &[0x41, 0, 0x42, 0],
        ]),
    );
    let seg = d.segment();
    assert!(build::identity_move_data(&seg, 900));
    assert!(!build::identity_move_data(&seg, 901));
    assert!(!build::identity_move_data(&seg, 902));
    assert_eq!(seg.root_part(seg.data_of(903)), None);
    assert!(!build::identity_move_data(&seg, 903));
    // The same attribute with its true count is read, ending at the data's
    // end.
    let text = cat(&[
        &s8("256"),
        &903u64.to_le_bytes(),
        &s8(""),
        &[0, 1],
        &u32b(1),
        &s8("name"),
        &s8("IntrinsicMetaTypeIString"),
        &s16("AB"),
    ]);
    let root = seg.root_part(&text).unwrap();
    assert_eq!(root.attrs[0].value, AttrValue::Text("AB".into()));
    assert_eq!(root.end, text.len());
    assert!(!build::identity_move_data(&seg, 1));
}

/// A combine that consumes a tool of another component through a body
/// removal there (mitcad#104): `u32 r | r refs` to `RemoveBodyFeature`
/// objects after the operation, then the usual layout without body
/// records.
#[test]
fn combine_with_body_removals_in_other_components() {
    const RECIPE: &str = "7ACC2A03-0261-4879-A14A-A93D661A5BDC";
    const COMBINE: &str = "2A94257F-2020-4B19-9A68-103A2672F1B7";
    const REMOVE: &str = "4A782808-FBDD-4351-A10A-F7AE5693D330";
    let mut d = sample();
    for (g, v) in [
        (COMBINE, 1),
        (REMOVE, 0),
        (BODY_INPUT, 1),
        (BODY_REF, 1),
        (PLACEMENT, 5),
        (RECIPE, 1),
    ] {
        d.class(g, ROOT, v);
    }
    d.objects.retain(|o| o.0 != 3);
    d.obj(
        3,
        TIMELINE,
        cat(&[&[0, 0], &r(1), &u32b(3), &r(10), &r(20), &r(500)]),
    );
    let body = |tag: &str| {
        let mut b = face_recipe(tag, &[3]);
        let n = b.len();
        b.truncate(n - 20);
        b.extend(s8("body_recipe_data"));
        b
    };
    for (input, refer, recipe, tag) in [(510, 511, 512, "1"), (520, 521, 522, "2")] {
        d.obj(input, BODY_INPUT, cat(&[&[0, 0], &u32b(1), &r(refer)]));
        d.obj(refer, BODY_REF, cat(&[&[0, 0], &r(recipe)]));
        d.obj(recipe, RECIPE, body(tag));
    }
    d.obj(540, REMOVE, vec![0, 0]);
    d.obj(531, PLACEMENT, vec![0, 0]);
    // Join, tools not kept: tools [510] removed by 540, target 520.
    d.obj(
        500,
        COMBINE,
        cat(&[
            &[0, 0],
            &u32b(0),
            &u32b(1),
            &r(540),
            &[0, 0],
            &u32b(1),
            &r(531),
            &u32b(0x195),
            &u32b(0),
            &u32b(1),
            &r(510),
            &[0; 8],
            &u32b(0),
            &u32b(1),
            &r(520),
            &u32b(4),
            &r(510),
            &r(511),
            &r(520),
            &r(521),
            &tail(5, "Combine", 1, "", [0, 0, 0], 501),
        ]),
    );
    d.obj(501, HEALTH, vec![0, 0]);
    let (m, b) = d.streams();
    let v = serde_json::to_value(Design::parse(&m, b).unwrap().dump("x", "Design1")).unwrap();
    let c = &v["timeline"]["items"][2]["detail"];
    assert_eq!(c["operation"], "JoinFeatureOperation");
    assert_eq!(c["isKeepToolBodies"], false);
    assert_eq!(c["targetBody"]["_f3d"]["entities"][0][0]["tag"], "2");
    assert_eq!(c["toolBodies"][0]["_f3d"]["entities"][0][0]["tag"], "1");
}

// Assembly relationships (mitcad#155).

/// A design with two occurrences of Pin (80 and 83) placed by the
/// placements 630 and 635, and the classes of relationships; `row_version`
/// is the class version of the rows' base class, `constraint_version` that
/// of the constraints.
fn relationship_sample(row_version: u32, constraint_version: u32) -> Doc {
    const ROOT_GUID: &str = "22222222-0000-0000-0000-000000000002";
    const PIN_GUID: &str = "55555555-0000-0000-0000-000000000005";
    const OCC_B: &str = "b0b0b0b0-0000-0000-0000-000000000083";
    let mut d = sample();
    for (g, p, v) in [
        (PLACEMENT, ROOT, 5),
        (CONTEXT_PATH, ROOT, 1),
        (CONTEXT_LEVEL, ROOT, 4),
        (GEOMETRIC_RELATIONSHIP, ROOT, 1),
        (TANGENT_RELATIONSHIP, ROOT, 0),
        (RELATION_ROW, ROOT, row_version),
        (ALIGN_ROW, RELATION_ROW, 0),
        (GEOM_CONSTRAINT, ROOT, constraint_version),
        (CONSTRAINT_DATA, ROOT, 0),
        (VALUE_DATA, CONSTRAINT_DATA, 1),
        (ALIGN_DATA, VALUE_DATA, 0),
        (KEY_POINT, ROOT, 3),
    ] {
        d.class(g, p, v);
    }
    d.objects.retain(|o| ![2, 3, 5, 80, 82].contains(&o.0));
    d.obj(
        2,
        COMPONENT,
        cat(&[&[0, 0], &s16(ROOT_GUID), &s16("Bracket")]),
    );
    d.obj(5, COMPONENT, cat(&[&[0, 0], &s16(PIN_GUID), &s16("Pin")]));
    let t = [
        1.0, 0.0, 0.0, 10.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ];
    for (id, guid) in [(80, OCC_A), (83, OCC_B)] {
        d.obj(
            id,
            OCCURRENCE,
            cat(&[
                &[0, 0],
                &r(5),
                &[0],
                &f64s(&t),
                &u32b(0),
                &r(81),
                &s16(guid),
            ]),
        );
    }
    d.obj(82, OCCURRENCE, cat(&[&[0, 0], &r(2), &[1]]));
    d.obj(
        3,
        TIMELINE,
        cat(&[&[0, 0], &r(1), &u32b(3), &r(10), &r(20), &r(600)]),
    );
    for (id, path, level, guid) in [(630, 631, 632, OCC_A), (635, 636, 637, OCC_B)] {
        d.obj(
            id,
            PLACEMENT,
            cat(&[&[0, 0], &[0; 4], &[1], &r(600), &r(path)]),
        );
        d.obj(
            path,
            CONTEXT_PATH,
            cat(&[&[0, 0], &[1], &u32b(1), &r(level)]),
        );
        d.obj(
            level,
            CONTEXT_LEVEL,
            context_level(guid, PIN_GUID, ROOT_GUID),
        );
    }
    // The offset parameter.
    d.obj(650, PARAMETER_HOLDER, cat(&[&[1], &u32b(1), &r(600), &[0]]));
    d.obj(
        651,
        PARAMETER,
        parameter(20, Some(650), "-0.5 mm", "Offset", "", "mm", "d20", -0.05),
    );
    d
}

/// Two frames: side one's plane z = 0.6 facing up, side two's z = 0
/// facing down (cm).
const REL_FRAMES: [[f64; 16]; 2] = [
    [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.6, 0.0, 0.0, 0.0, 1.0,
    ],
    [
        1.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ],
];

/// The relationship item 600 with one row 610 and a health object.
fn relationship_item(d: &mut Doc, row: Vec<u8>) {
    d.obj(610, RELATION_ROW, row);
    d.obj(
        600,
        GEOMETRIC_RELATIONSHIP,
        cat(&[
            &[0, 0],
            &r(610),
            &r(630),
            &r(635),
            &tail(-1, "GeometricRelationship", 1, "", [0, 0, 0], 601),
        ]),
    );
    d.obj(601, HEALTH, vec![0, 0]);
}

/// The data part of a constraint: two points, the sense, two planes and
/// their frames.
fn relation_data(sense: u32) -> Vec<u8> {
    cat(&[
        &u32b(2),
        &f64s(&[0.4, -0.4, 0.6, -1.2, -0.5, 0.0]),
        &u32b(sense),
        &u32b(2),
        &u32b(2),
        &u32b(2),
        &u32b(2),
        &[0],
        &f64s(&REL_FRAMES[0]),
        &[0],
        &f64s(&REL_FRAMES[1]),
    ])
}

fn relationship_rows(d: &Doc) -> Value {
    let (m, b) = d.streams();
    let v = serde_json::to_value(Design::parse(&m, b).unwrap().dump("x", "Design1")).unwrap();
    let item = &v["timeline"]["items"][2];
    assert_eq!(item["objectType"], "GeometricRelationship");
    item["detail"]["rows"].clone()
}

/// A relationship's row (mitcad#155): an alignment of two planes with an
/// offset parameter, its constraint of class version 7 referring to its
/// data record; the sides' occurrences by the placements' paths.
#[test]
fn relationship_rows_from_data_records() {
    let mut d = relationship_sample(4, 7);
    // The row: its own fields, then the base's (sense, key points, side
    // one's placement, the constraint, the inputs, side two's).
    let row = cat(&[
        &[0, 0],
        &[0],
        &u32b(5),
        &[0, 0],
        &r(650),
        &u32b(1),
        &u32b(0),
        &u32b(1),
        &r(630),
        &r(620),
        &u32b(0),
        &u32b(0),
        &u32b(0),
        &u32b(1),
        &r(635),
        &[1, 0, 0],
    ]);
    d.objects.retain(|o| o.0 != 610);
    d.obj(
        620,
        GEOM_CONSTRAINT,
        cat(&[&[0, 0], &r(621), &[0], &u32b(0)]),
    );
    d.obj(
        621,
        ALIGN_DATA,
        cat(&[
            &[0, 0],
            &u32b(5),
            &[0, 0, 0, 0],
            &f64s(&[0.0, 0.0, -0.05]),
            &relation_data(1),
            &s16("Constraint 1"),
            &r(620),
            &[0],
            &u32b(2),
            &s16("Face"),
            &s16("Face"),
            &u32b(2),
            &[1, 1],
        ]),
    );
    relationship_item(&mut d, row);
    // The row is of the derived class.
    let c = d.class_index(ALIGN_ROW);
    d.objects.iter_mut().find(|o| o.0 == 610).unwrap().1 = c;
    let rows = relationship_rows(&d);
    let row = &rows[0];
    assert_eq!(row["type"], "align");
    assert_eq!(row["geometry"], json!(["plane", "plane"]));
    assert_eq!(row["sense"], "aligned");
    assert_eq!(row["offset"]["name"], "d20");
    assert_eq!(row["occurrenceOne"]["_f3d"]["path"], json!([80]));
    assert_eq!(row["occurrenceTwo"]["_f3d"]["path"], json!([83]));
    assert_eq!(row["_f3d"]["type_code"], 5);
    assert_eq!(row["_f3d"]["values"], json!([0.0, 0.0, -0.05]));
    assert_eq!(row["_f3d"]["frames"][0][2], json!([0.0, 0.0, 1.0, 0.6]));
    assert_eq!(row["_f3d"]["frames"][1][2], json!([0.0, 0.0, -1.0, 0.0]));
    assert_eq!(row["_f3d"]["selections"], json!(["Face", "Face"]));
}

/// A row of an older file (mitcad#155): the constraint of class version 6
/// holds its data itself, its sense and type at its end.
#[test]
fn relationship_rows_with_their_own_data() {
    let mut d = relationship_sample(3, 6);
    let row = cat(&[
        &[0, 0],
        &r(630),
        &u32b(2),
        &r(635),
        &[0, 0],
        &[0, 0],
        &u32b(0),
        &r(650),
        &[0, 0],
        &r(620),
        &u32b(0),
        &u32b(0),
        &[0],
        &u32b(6),
        &[1, 0, 0],
    ]);
    d.obj(
        620,
        GEOM_CONSTRAINT,
        cat(&[
            &[0, 0],
            &[0, 0, 1],
            &s16("Constraint 1"),
            &u32b(0),
            &relation_data(1),
            &f64s(&[0.0, 0.0]),
            &[0; 20],
            &u32b(2),
            &s16("Face"),
            &s16("Face"),
            &u32b(2),
            &u32b(6),
            &u32b(2),
            &[1, 1],
        ]),
    );
    relationship_item(&mut d, row);
    let rows = relationship_rows(&d);
    let row = &rows[0];
    assert_eq!(row["type"], "align");
    assert_eq!(row["geometry"], json!(["plane", "plane"]));
    // The sense at the end (2, opposed), not the number after the points.
    assert_eq!(row["sense"], "opposed");
    assert_eq!(row["_f3d"]["type_code"], 6);
    assert_eq!(row["offset"]["value"], -0.05);
    assert_eq!(row["occurrenceOne"]["_f3d"]["path"], json!([80]));
}

/// A tangent relationship (mitcad#155): each side's occurrence and the
/// point picked on its face.
#[test]
fn tangent_relationship() {
    let mut d = relationship_sample(4, 7);
    d.objects.retain(|o| o.0 != 3);
    d.obj(
        3,
        TIMELINE,
        cat(&[&[0, 0], &r(1), &u32b(3), &r(10), &r(20), &r(700)]),
    );
    for (id, p) in [(710, [1.0, 2.0, 3.0]), (711, [4.0, 5.0, 6.0])] {
        d.obj(
            id,
            KEY_POINT,
            cat(&[&[0, 0], &[0; 21], &f64s(&p), &u32b(11)]),
        );
    }
    d.obj(
        700,
        TANGENT_RELATIONSHIP,
        cat(&[
            &[0, 0],
            &r(710),
            &r(711),
            &u32b(2),
            &r(630),
            &r(635),
            &tail(-1, "Tangent Relationship", 1, "", [0, 0, 0], 701),
        ]),
    );
    d.obj(701, HEALTH, vec![0, 0]);
    let (m, b) = d.streams();
    let v = serde_json::to_value(Design::parse(&m, b).unwrap().dump("x", "Design1")).unwrap();
    let item = &v["timeline"]["items"][2];
    assert_eq!(item["objectType"], "TangentRelationship");
    let t = &item["detail"];
    assert_eq!(t["pointOne"], json!([1.0, 2.0, 3.0]));
    assert_eq!(t["pointTwo"], json!([4.0, 5.0, 6.0]));
    assert_eq!(t["occurrenceOne"]["_f3d"]["path"], json!([80]));
    assert_eq!(t["occurrenceTwo"]["_f3d"]["path"], json!([83]));
}

/// mitcad#169: an extrusion of component Bracket that also cuts Pin's
/// bodies has a second extrusion in Pin's feature list, with its own
/// operation; the design's table of item parts links the two. The part
/// comes as an item of its own right after the timeline's items, at its
/// item's point of the timeline, named after it.
#[test]
fn parts_of_items_in_other_components() {
    let mut d = sample();
    d.class(FEATURE_LIST, ROOT, 0);
    d.class(ITEM_PARTS, ROOT, 0);
    // Bracket's feature list: its manager and both timeline items.
    d.obj(100, FEATURE_LIST, cat(&[&[0, 0], &r(1), &r(10), &r(20)]));
    // Pin's manager and feature list with the part.
    d.obj(101, FEATURE_MANAGER, cat(&[&[0, 0], &r(5)]));
    d.obj(102, FEATURE_LIST, cat(&[&[0, 0], &r(101), &r(25)]));
    d.obj(
        25,
        EXTRUDE,
        cat(&[
            &[0, 0],
            &[0, 0, 0],
            &u32b(2),
            &u32b(1),
            &u32b(2),
            &[0, 1],
            &u32b(0),
            &f64s(&[0.0, 0.0, 1.0]),
            &[0, 0],
            &r(70),
            &r(71),
            &tail(9, "Extrude", 1, "", [0, 0, 0], 26),
        ]),
    );
    d.obj(26, HEALTH, vec![0, 0]);
    let key = "0262235f-99d9-4433-a71f-3e56970b4c63";
    d.obj(
        103,
        ITEM_PARTS,
        cat(&[
            &[0, 0],
            &u32b(1),
            &s16(key),
            &r(20),
            &u32b(1),
            &s16(key),
            &u32b(2),
            &r(20),
            &r(25),
        ]),
    );
    let (m, b) = d.streams();
    let v = serde_json::to_value(Design::parse(&m, b).unwrap().dump("x", "Design1")).unwrap();
    let items = v["timeline"]["items"].as_array().unwrap();
    assert_eq!(items.len(), 3);
    let part = &items[2];
    assert_eq!(part["index"], 2);
    assert_eq!(part["name"], "Boss");
    assert_eq!(part["objectType"], "ExtrudeFeature");
    assert_eq!(part["component"], "Pin");
    assert_eq!(part["_f3d"]["part_of"], 1);
    assert_eq!(part["_f3d"]["result_no"], 9);
    assert_eq!(part["_f3d"]["object_id"], 25);
    assert_eq!(part["_f3d"]["extrude"]["operation"], "Cut");
    assert!(items[1]["_f3d"].get("part_of").is_none());
}

/// Construction points, planes and axes by what they are made from
/// (mitcad#186): a point at a vertex (its position from the key point
/// input), a plane through a vertex, a point and a vertex, a plane
/// tangent to a face at a vertex, an axis through a round face and one
/// through two vertices.
#[test]
fn construction_points_planes_and_axes_by_their_inputs() {
    const KEY: &str = KEY_POINT;
    const RECIPE: &str = "7ACC2A03-0261-4879-A14A-A93D661A5BDC";
    let mut d = sample();
    for (g, v) in [
        (CONSTRUCTION_POINT, 1),
        (CONSTRUCTION_PLANE, 3),
        (CONSTRUCTION_AXIS, 0),
        (ORIGIN_POINT, 0),
        (FACE_REF, 3),
        (RECIPE, 3),
        (KEY, 3),
        (DIRECTION_INPUT, 0),
        (INPUT_SLOT, 1),
    ] {
        d.class(g, ROOT, v);
    }
    d.objects.retain(|o| ![3].contains(&o.0));
    let ids = [10, 20, 100, 300, 400, 500, 600];
    let mut timeline = cat(&[&[0, 0], &r(1), &u32b(ids.len() as u32)]);
    for id in ids {
        timeline.extend(r(id));
    }
    d.obj(3, TIMELINE, timeline);
    // A vertex input, its recipe, and a face input of a cylinder.
    let vertex = |d: &mut Doc, input: u64, tags: &[&str]| {
        d.obj(input, FACE_REF, cat(&[&[0, 0], &r(input + 1)]));
        let names: Vec<(&str, i32)> = tags.iter().map(|t| (*t, 301)).collect();
        d.obj(input + 1, RECIPE, recipe("vertex", &names, &[]));
    };
    let face = |d: &mut Doc, input: u64| {
        d.obj(input, FACE_REF, cat(&[&[0, 0], &r(input + 1)]));
        d.obj(input + 1, RECIPE, face_recipe("3", &[301]));
    };
    let key_point = |d: &mut Doc, id: u64, at: [f64; 3]| {
        d.obj(id, KEY, cat(&[&[0, 0], &[0; 21], &f64s(&at), &u32b(10)]));
    };
    // The point at the vertex (6, 0, 0) cm; its own point object 101.
    d.obj(101, ORIGIN_POINT, vec![0, 0]);
    d.obj(
        100,
        CONSTRUCTION_POINT,
        cat(&[
            &[0, 0],
            &[1],
            &u32b(0),
            &r(101),
            &u32b(2),
            &r(110),
            &r(111),
            &tail(-1, "WorkPoint", 1, "", [0, 0, 0], 102),
        ]),
    );
    d.obj(102, HEALTH, vec![0, 0]);
    key_point(&mut d, 110, [6.0, 0.0, 0.0]);
    vertex(&mut d, 111, &["1", "2", "3"]);
    // A plane through that vertex's point, and two vertices.
    d.obj(103, REF_TARGET, cat(&[&[0, 0], &101u64.to_le_bytes()]));
    d.obj(104, ENTITY_REF, cat(&[&[0, 0], &r(103)]));
    d.obj(
        300,
        CONSTRUCTION_PLANE,
        cat(&[
            &[0, 0],
            &[1],
            &u32b(2),
            &u32b(5),
            &r(320),
            &r(321),
            &r(104),
            &r(323),
            &r(324),
            &tail(-1, "WorkPlane", 1, "", [0, 0, 0], 301),
        ]),
    );
    d.obj(301, HEALTH, vec![0, 0]);
    d.obj(320, INPUT_SLOT, cat(&[&[0, 0], &r(321)]));
    vertex(&mut d, 321, &["4", "5", "6"]);
    d.obj(323, INPUT_SLOT, cat(&[&[0, 0], &r(324)]));
    vertex(&mut d, 324, &["7", "8", "9"]);
    // A plane tangent to a face at a vertex.
    d.obj(
        400,
        CONSTRUCTION_PLANE,
        cat(&[
            &[0, 0],
            &[1],
            &u32b(9),
            &u32b(2),
            &r(410),
            &r(412),
            &tail(-1, "WorkPlane", 2, "", [0, 0, 0], 401),
        ]),
    );
    d.obj(401, HEALTH, vec![0, 0]);
    face(&mut d, 410);
    vertex(&mut d, 412, &["1", "2"]);
    // An axis through a round face, with its line (a point and a
    // displacement); and one through two vertices.
    let line = |d: &mut Doc, id: u64, o: [f64; 3], v: [f64; 3]| {
        d.obj(
            id,
            DIRECTION_INPUT,
            cat(&[
                &[0, 0],
                &u32b(0),
                &f64s(&o),
                &f64s(&v),
                &f64s(&[0.0, 0.0]),
                &u32b(1),
            ]),
        );
    };
    d.obj(
        500,
        CONSTRUCTION_AXIS,
        cat(&[
            &[0, 0],
            &u32b(2),
            &r(510),
            &r(511),
            &tail(-1, "WorkAxis", 1, "", [0, 0, 0], 501),
        ]),
    );
    d.obj(501, HEALTH, vec![0, 0]);
    line(&mut d, 510, [1.0, 2.0, 0.0], [0.0, 0.0, 1.0]);
    face(&mut d, 511);
    d.obj(
        600,
        CONSTRUCTION_AXIS,
        cat(&[
            &[0, 0],
            &u32b(5),
            &r(610),
            &r(620),
            &r(612),
            &r(630),
            &r(614),
            &tail(-1, "WorkAxis", 2, "", [0, 0, 0], 601),
        ]),
    );
    d.obj(601, HEALTH, vec![0, 0]);
    line(&mut d, 610, [0.0, 0.0, 0.0], [1.0, 0.0, 0.0]);
    vertex(&mut d, 620, &["1", "2", "3"]);
    key_point(&mut d, 612, [0.0, 0.0, 0.0]);
    vertex(&mut d, 630, &["4", "5", "6"]);
    key_point(&mut d, 614, [3.0, 0.0, 0.0]);

    let (m, b) = d.streams();
    let v = serde_json::to_value(Design::parse(&m, b).unwrap().dump("x", "Design1")).unwrap();
    let items = v["timeline"]["items"].as_array().unwrap();
    let vertex_of = |e: &Value, tags: usize| {
        assert_eq!(e["kind"], "vertex", "{e}");
        assert_eq!(e["_f3d"]["recipe"], "vertex");
        assert_eq!(e["_f3d"]["entities"].as_array().unwrap().len(), tags);
    };

    let point = &items[2];
    assert_eq!(point["objectType"], "ConstructionPoint");
    assert_eq!(point["_f3d"]["datum_code"], 0);
    assert_eq!(point["detail"]["geometry"], json!([6.0, 0.0, 0.0]));
    let def = &point["detail"]["definition"];
    assert_eq!(def["_type"], "ConstructionPointPointDefinition");
    vertex_of(&def["pointEntity"], 3);

    let plane = &items[3];
    assert_eq!(plane["_f3d"]["datum_code"], 2);
    let def = &plane["detail"]["definition"];
    assert_eq!(def["_type"], "ConstructionPlaneThreePointsDefinition");
    vertex_of(&def["pointEntityOne"], 3);
    assert_eq!(
        def["pointEntityTwo"],
        json!({"kind": "construction_point", "name": "WorkPoint1", "origin": null,
               "timeline_index": 2})
    );
    vertex_of(&def["pointEntityThree"], 3);

    let tangent = &items[4]["detail"]["definition"];
    assert_eq!(items[4]["_f3d"]["datum_code"], 9);
    assert_eq!(
        tangent["_type"],
        "ConstructionPlaneTangentAtPointDefinition"
    );
    assert_eq!(tangent["tangentFace"]["kind"], "face");
    vertex_of(&tangent["pointEntity"], 2);

    let round = &items[5]["detail"];
    assert_eq!(
        round["geometry"],
        json!({"type": "InfiniteLine3D", "origin": [1.0, 2.0, 0.0], "direction": [0.0, 0.0, 1.0]})
    );
    assert_eq!(
        round["definition"]["_type"],
        "ConstructionAxisCircularFaceDefinition"
    );
    assert_eq!(round["definition"]["circularFace"]["kind"], "face");

    let two = &items[6]["detail"]["definition"];
    assert_eq!(two["_type"], "ConstructionAxisTwoPointDefinition");
    vertex_of(&two["pointEntityOne"], 3);
    vertex_of(&two["pointEntityTwo"], 3);
}

/// The bytes of a sketch text record (`E0618268`, mitcad#186) after its
/// header: root part with two properties, then the members of
/// [`sketch::parse_text`]'s layout up to the owning sketch's reference.
fn text_record(slots: Option<(u64, u64)>, kind: u32, matrix: Option<[f64; 16]>) -> Vec<u8> {
    let attr = |key: &str, value: u64| {
        cat(&[
            &s8(key),
            &s8("IntrinsicMetaTypeuint64"),
            &value.to_le_bytes(),
        ])
    };
    let slot = |at: usize| slots.map_or(vec![], |s| r([s.0, s.1][at]));
    let tail = match matrix {
        Some(m) => cat(&[&[1], &f64s(&m), &[0; 30]]),
        None => cat(&[&u32b(1), &u32b(256), &u32b(0), &u32b(0), &[0; 15]]),
    };
    cat(&[
        &[0, 1],
        &u32b(2),
        &attr("textex_tag", 101),
        &attr("txt_tag_base", 300),
        &[1],
        &f64s(&[1.0]),
        &[0; 12],
        &1.0f32.to_le_bytes(),
        &s16("Arial"),
        &[0],
        &f64s(&[0.3]),
        &slot(0),
        &u32b(3),
        &[0, 0, 0],
        &s16("1 mm"),
        &slot(1),
        &u32b(1),
        &[0],
        &400i32.to_le_bytes(),
        &u32b(kind),
        &tail,
        &r(40),
    ])
}

#[test]
fn sketch_text_records() {
    let mut doc = Doc::default();
    for (g, p, v) in [
        (ROOT, "", 0),
        (SKETCH, ROOT, 18),
        (SKETCH_TEXT, ROOT, 0),
        (USAGE, ROOT, 1),
    ] {
        doc.class(g, p, v);
    }
    doc.obj(40, SKETCH, vec![0, 0]);
    doc.obj(91, USAGE, vec![0, 0]);
    doc.obj(92, USAGE, vec![0, 0]);
    let turned = [
        -1.0, 0.0, 0.0, 2.5, 0.0, -1.0, 0.0, -3.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ];
    // With its parameter references and placement matrix, without both,
    // and a text on a path.
    doc.obj(
        80,
        SKETCH_TEXT,
        text_record(Some((91, 92)), 0, Some(turned)),
    );
    doc.obj(81, SKETCH_TEXT, text_record(None, 0, None));
    doc.obj(82, SKETCH_TEXT, text_record(None, 1, None));
    let seg = doc.segment();
    let read = |id: u64| sketch::parse_text(&seg, seg.object(id).unwrap()).unwrap();

    let t = read(80);
    assert_eq!((t.text.as_str(), t.font.as_str()), ("1 mm", "Arial"));
    assert_eq!(
        (t.height, t.horizontal, t.vertical, t.weight),
        (0.3, 3, 1, 400)
    );
    assert_eq!((t.height_holder, t.content_holder), (Some(91), Some(92)));
    assert_eq!(t.matrix, Some(turned));
    // The placement is in the value the dump gets: the angle of the
    // matrix's first column and its translation.
    let v = sketch::text_value(&seg, 80, "t0", &t, &[], &sketch::IdTable::default());
    assert_eq!(v["position"], json!([2.5, -3.0, 0.0]));
    assert!((v["angle"].as_f64().unwrap() - std::f64::consts::PI).abs() < 1e-12);
    assert_eq!(
        v["definition"]["horizontalAlignment"],
        "CenterHorizontalAlignment"
    );
    assert_eq!(v["definition"]["verticalAlignment"], "TopVerticalAlignment");
    assert_eq!(v["textStyle"], 0);

    let t = read(81);
    assert_eq!(
        (t.height_holder, t.content_holder, t.matrix),
        (None, None, None)
    );
    assert_eq!(t.text, "1 mm");
    let v = sketch::text_value(&seg, 81, "t1", &t, &[], &sketch::IdTable::default());
    assert!(v.get("position").is_none() && v.get("angle").is_none());

    let t = read(82);
    assert_eq!(t.kind, 1);
}

/// A reference to an object of another document through an assembly
/// context: `01 id 01 key 00 str8(guid) 00`.
fn context_ref(id: u64, key: u64, guid: &str) -> Vec<u8> {
    cat(&[
        &[1],
        &id.to_le_bytes(),
        &[1],
        &key.to_le_bytes(),
        &[0],
        &s8(guid),
        &[0],
    ])
}

/// An occurrence of a component of another document whose id is no object
/// of this design (mitcad#196): in the occurrence tree with its document's
/// key and URN, its transform and its GUIDs; the external document with its
/// version and GUIDs; and listed by `referenced_documents`.
#[test]
fn occurrence_of_a_component_of_another_document() {
    let mut d = sample();
    let key = 0x0102_0304_0506_0708;
    let t = [
        1.0, 0.0, 0.0, 5.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ];
    d.obj(
        83,
        OCCURRENCE,
        cat(&[
            &[0, 0],
            &context_ref(0x7777, key, COMPONENT),
            &[0],
            &f64s(&t),
            &u32b(0),
            &s16("124bea26-b9ef-4785-962e-5f51a19652ce"),
            &r(84),
        ]),
    );
    d.obj(84, OCCURRENCE_CONTAINER, cat(&[&[0, 0], &r(83), &r(2)]));
    d.externals.push(super::writer::WriterExternal {
        key,
        guids: [
            "7A3F46D6-B137-489D-84BA-42A1B0ED13DF".into(),
            "f7d6678d-a812-4537-98bd-7db7eea2ea52".into(),
        ],
        urn: "urn:test:fs.file:vf.part?version=3".into(),
        document_guid: "6bb3e201-b763-4428-a07d-12d3975a4494".into(),
    });
    let (m, b) = d.streams();
    let design = Design::parse(&m, b).unwrap();
    let v = serde_json::to_value(design.dump("assembly.f3d", "Design1")).unwrap();
    let occ = v["occurrences"].as_array().unwrap();
    let ext = occ
        .iter()
        .find(|o| o["isReferencedComponent"] == true)
        .expect("the occurrence of the other document's component");
    assert_eq!(ext["transform"][0], json!([1.0, 0.0, 0.0, 5.0]));
    assert_eq!(
        ext["_f3d"],
        json!({"object_id": 83, "component_object": 0x7777,
               "guids": ["124bea26-b9ef-4785-962e-5f51a19652ce"],
               "external_key": "0102030405060708",
               "external_document": "urn:test:fs.file:vf.part?version=3"})
    );
    assert_eq!(
        v["_f3d"]["external_documents"][1],
        json!({"key": "0102030405060708", "urn": "urn:test:fs.file:vf.part?version=3",
               "version": 3,
               "guids": ["7a3f46d6-b137-489d-84ba-42a1b0ed13df",
                         "f7d6678d-a812-4537-98bd-7db7eea2ea52"],
               "document_guid": "6bb3e201-b763-4428-a07d-12d3975a4494"})
    );
    // A reference with a key that is no external document is none.
    let seg = d.segment();
    assert!(
        seg.context_ref_at(&context_ref(0x7777, 99, COMPONENT), 0)
            .is_none()
    );
    assert_eq!(super::build::urn_version("urn:x?version=12"), Some(12));
    assert_eq!(super::build::urn_version("urn:x"), None);
}

/// An input slot's role key carries the placement of the occurrence the
/// entity it holds is in (mitcad#196): 16 doubles, a row-major 4 x 4
/// transform in cm.
#[test]
fn a_slot_carries_the_placement_of_its_entitys_occurrence() {
    const SLOT_KEY: &str = "2DF7DA30-A4E4-4260-AFD1-D9C7154CB44B";
    let mut d = sample();
    for (g, v) in [(BODY_INPUT, 1), (BODY_REF, 1), (SLOT_KEY, 1)] {
        d.class(g, ROOT, v);
    }
    // The entity's reference, and a slot holding it with its key.
    d.obj(511, BODY_REF, vec![0, 0]);
    d.obj(
        510,
        BODY_INPUT,
        cat(&[
            &[0, 0],
            &u32b(1),
            &r(511),
            &[0; 6],
            &r(541),
            &u32b(0),
            &u32b(17),
            &[0; 4],
        ]),
    );
    let matrix = [
        0.0, 1.0, 0.0, -1.5, //
        0.0, 0.0, -1.0, 0.25, //
        -1.0, 0.0, 0.0, 101.5, //
        0.0, 0.0, 0.0, 1.0,
    ];
    d.obj(
        541,
        SLOT_KEY,
        cat(&[&[0, 0], &[0], &f64s(&matrix), &[1, 0]]),
    );
    // A slot whose key is no placement (not a rotation) gives nothing.
    d.obj(521, BODY_REF, vec![0, 0]);
    d.obj(
        520,
        BODY_INPUT,
        cat(&[
            &[0, 0],
            &u32b(1),
            &r(521),
            &[0; 6],
            &r(542),
            &u32b(0),
            &u32b(17),
            &[0; 4],
        ]),
    );
    d.obj(
        542,
        SLOT_KEY,
        cat(&[&[0, 0], &[0], &f64s(&[2.0; 16]), &[1, 0]]),
    );
    let (mb, b) = d.streams();
    let seg = Segment::parse(&mb, b).unwrap();
    let contexts = decode::slot_contexts(&seg);
    let found = contexts.get(&511).expect("the entity's placement");
    assert_eq!(found[0], [0.0, 1.0, 0.0, -1.5]);
    assert_eq!(found[2], [-1.0, 0.0, 0.0, 101.5]);
    assert_eq!(found[3], [0.0, 0.0, 0.0, 1.0]);
    assert!(!contexts.contains_key(&521));
}

/// Occurrence light bulbs: one [`HIDDEN_OCCURRENCE`] object per hidden
/// occurrence path, its last reference list the path from the root down;
/// one whose path list is empty hides the one occurrence it refers to (a
/// top-level occurrence of another document's component); one whose path
/// goes on inside another document hides nothing here. Occurrences without
/// one keep their light bulbs unknown.
#[test]
fn hidden_occurrences_turn_their_light_bulbs_off() {
    let key = 0x0102_0304_0506_0708;
    let ext = |d: &mut Doc, id: u64, guid: &str| {
        d.obj(
            id,
            OCCURRENCE,
            cat(&[
                &[0, 0],
                &context_ref(0x7777, key, COMPONENT),
                &[1],
                &u32b(0),
                &s16(guid),
                &r(84),
            ]),
        );
    };
    let make = |hidden: &[Vec<u8>]| {
        let mut d = sample();
        d.class(HIDDEN_OCCURRENCE, ROOT, 1);
        ext(&mut d, 83, "124bea26-b9ef-4785-962e-5f51a19652ce");
        ext(&mut d, 85, "d3c5a1a0-0a57-4e0f-9a37-5f0e3c2d9b11");
        d.obj(
            84,
            OCCURRENCE_CONTAINER,
            cat(&[&[0, 0], &r(83), &r(85), &r(2)]),
        );
        d.externals.push(super::writer::WriterExternal {
            key,
            guids: [
                "7A3F46D6-B137-489D-84BA-42A1B0ED13DF".into(),
                "f7d6678d-a812-4537-98bd-7db7eea2ea52".into(),
            ],
            urn: "urn:test:fs.file:vf.part?version=3".into(),
            document_guid: "6bb3e201-b763-4428-a07d-12d3975a4494".into(),
        });
        for (i, data) in hidden.iter().enumerate() {
            d.obj(100 + i as u64, HIDDEN_OCCURRENCE, data.clone());
        }
        let (m, b) = d.streams();
        let v = serde_json::to_value(Design::parse(&m, b).unwrap().dump("x", "Design1")).unwrap();
        let bulbs: Vec<(u64, Value)> = v["occurrences"]
            .as_array()
            .unwrap()
            .iter()
            .map(|o| {
                (
                    o["_f3d"]["object_id"].as_u64().unwrap(),
                    o.get("isLightBulbOn").cloned().unwrap_or(Value::Null),
                )
            })
            .collect();
        bulbs
    };
    // The path list after the slots: `u32 n | n × ref` (here [80]).
    let path_of_80 = cat(&[
        &[0, 0],
        &u32b(2),
        &r(5),
        &u32b(20),
        &r(80),
        &[1, 0, 0, 0, 0, 0],
        &u32b(1),
        &r(80),
        &[2],
        &u32b(1),
    ]);
    // No path list, one occurrence of this file: that one (83).
    let only_83 = cat(&[
        &[0, 0],
        &u32b(2),
        &context_ref(0x7777, key, COMPONENT),
        &u32b(5),
        &r(83),
        &[4, 0, 0, 0, 0, 0],
        &u32b(0),
    ]);
    // A path inside the other document (85, then an occurrence there).
    let inside_85 = cat(&[
        &[0, 0],
        &u32b(3),
        &r(85),
        &[0, 0, 0, 0],
        &u32b(2),
        &r(85),
        &context_ref(0x5555, key, OCCURRENCE),
    ]);
    let bulbs = make(&[path_of_80, only_83, inside_85]);
    let bulb = |id: u64| {
        bulbs
            .iter()
            .find(|(o, _)| *o == id)
            .map(|(_, b)| b.clone())
            .unwrap()
    };
    assert_eq!(bulb(80), json!(false), "{bulbs:?}");
    assert_eq!(bulb(83), json!(false), "{bulbs:?}");
    assert_eq!(bulb(85), Value::Null, "{bulbs:?}");
    // Without the objects every light bulb stays unknown.
    assert!(make(&[]).iter().all(|(_, b)| b.is_null()));
}

/// A derive item (mitcad#324): the source design (an assembly of another
/// document, by its root component), the deriving design's placement in
/// it, one body of a third document with the properties that stay linked
/// and the placement of its occurrence; and a derive that lists no
/// entities.
#[test]
fn derive_items_name_their_source_bodies_and_placements() {
    const DERIVED_BODY_FEATURE: &str = "B1E5B971-C007-48C9-8FF5-F4CCD91F87A4";
    let mut d = sample();
    for (g, p, v) in [
        (DERIVED_CONTEXT, ROOT, 3),
        (PLACEMENT, ROOT, 5),
        (BASE_FEATURE, ROOT, 4),
        (DERIVED_BODY_FEATURE, BASE_FEATURE, 10),
        (BODY, ROOT, 1),
    ] {
        d.class(g, p, v);
    }
    d.objects.retain(|o| o.0 != 3);
    let (assembly, part) = (0x1111_2222_3333_4444u64, 0x5555_6666_7777_8888u64);
    for (key, urn, guid) in [
        (
            assembly,
            "urn:test:fs.file:vf.assembly?version=10",
            "74c61ef7-0894-4b42-8532-24fb6b04aa22",
        ),
        (
            part,
            "urn:test:fs.file:vf.part?version=1",
            "9016787c-cb67-4d88-bd1a-387e9083cdb3",
        ),
    ] {
        d.externals.push(super::writer::WriterExternal {
            key,
            guids: [
                "7A3F46D6-B137-489D-84BA-42A1B0ED13DF".into(),
                "f7d6678d-a812-4537-98bd-7db7eea2ea52".into(),
            ],
            urn: urn.into(),
            document_guid: guid.into(),
        });
    }
    d.obj(
        3,
        TIMELINE,
        cat(&[&[0, 0], &r(1), &u32b(4), &r(10), &r(20), &r(500), &r(600)]),
    );
    // The design in the assembly: turned half a turn about x, at (2, 1, 0).
    let design = [
        1.0, 0.0, 0.0, 2.0, 0.0, -1.0, 0.0, 1.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ];
    // The body's occurrence: at (-4, 1, 0).
    let body = [
        1.0, 0.0, 0.0, -4.0, 0.0, 1.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ];
    for (id, m, item) in [(501, design, 500), (502, body, 500)] {
        d.obj(
            id,
            PLACEMENT,
            cat(&[&[0, 0], &[0; 4], &[0], &f64s(&m), &[0], &r(item)]),
        );
    }
    d.obj(503, PLACEMENT, cat(&[&[0, 0], &[1], &r(500)]));
    d.obj(510, DERIVED_BODY_FEATURE, cat(&[&[0, 0], &r(520), &r(500)]));
    d.obj(520, BODY, vec![0, 0]);
    d.obj(509, HEALTH, vec![0, 0]);
    let source = |entities: &[u8], placements: (u64, u64)| {
        cat(&[
            &[0, 0],
            &s16("120a1fde-0048-4244-9d5e-d8b4593296a3"),
            &r(placements.0),
            &[0; 8],
            &s16("74c61ef7-0894-4b42-8532-24fb6b04aa22"),
            &context_ref(3, assembly, COMPONENT),
            &s16("Assembly"),
            &s16("Assembly"),
            &[1],
            &[0; 8],
            &r(placements.1),
            &[0; 8],
            entities,
        ])
    };
    let entity = cat(&[
        &u32b(1),
        &r(510),
        &[0],
        &u32b(1),
        &context_ref(2495, part, BODY),
        &[0],
        &u32b(2),
        &s8("Geometry"),
        &s8("Transform"),
        &[0, 0, 0],
        &context_ref(2495, part, BODY),
        &r(502),
    ]);
    d.obj(
        500,
        DERIVED_CONTEXT,
        cat(&[
            &source(&entity, (501, 503)),
            &tail(-1, "DerivedContext", 1, "", [0, 0, 0], 509),
        ]),
    );
    d.obj(
        600,
        DERIVED_CONTEXT,
        cat(&[
            &source(&u32b(0), (501, 503)),
            &tail(-1, "DerivedContext", 2, "", [0, 0, 0], 509),
        ]),
    );
    let (m, b) = d.streams();
    let v = serde_json::to_value(Design::parse(&m, b).unwrap().dump("x", "Design1")).unwrap();
    let items = v["timeline"]["items"].as_array().unwrap();
    let derive = &items[2];
    assert_eq!(derive["objectType"], "DerivedContext");
    let detail = &derive["detail"];
    assert_eq!(detail["source"]["name"], "Assembly", "{derive}");
    assert_eq!(
        detail["source"]["_f3d"],
        json!({"key": "1111222233334444", "urn": "urn:test:fs.file:vf.assembly?version=10",
               "document_guid": "74c61ef7-0894-4b42-8532-24fb6b04aa22",
               "component_object": 3})
    );
    assert_eq!(
        detail["placements"][0]["transform"][1],
        json!([0.0, -1.0, 0.0, 1.0])
    );
    let entities = detail["entities"].as_array().unwrap();
    assert_eq!(entities.len(), 1);
    let e = &entities[0];
    assert_eq!(e["kind"], "body");
    assert_eq!(e["placement"]["transform"][0], json!([1.0, 0.0, 0.0, -4.0]));
    assert_eq!(e["_f3d"]["object_id"], 2495);
    assert_eq!(e["_f3d"]["key"], "5555666677778888");
    assert_eq!(e["_f3d"]["urn"], "urn:test:fs.file:vf.part?version=1");
    assert_eq!(
        e["_f3d"]["document_guid"],
        "9016787c-cb67-4d88-bd1a-387e9083cdb3"
    );
    assert_eq!(e["_f3d"]["feature"], 510);
    assert_eq!(e["_f3d"]["bodies"], json!([520]));
    assert_eq!(e["_f3d"]["links"], json!(["Geometry", "Transform"]));
    // A derive that lists none brings nothing.
    let empty = &items[3]["detail"];
    assert_eq!(empty["entities"], json!([]));
    assert_eq!(empty["source"]["name"], "Assembly");
}
