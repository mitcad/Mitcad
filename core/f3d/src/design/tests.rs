// SPDX-License-Identifier: MIT
//! Decoder tests on synthetic streams built byte by byte from the layouts
//! of the timeline format study.

use serde_json::{Value, json};

use super::classes::*;
use super::stream::{AttrValue, MetaStream, Segment};
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

/// Object id, class index, body, sub-chunk body.
type TestObject = (u64, usize, Vec<u8>, Option<Vec<u8>>);

/// A synthetic design: class table and objects.
#[derive(Default)]
pub(crate) struct Doc {
    classes: Vec<(String, String, u32)>,
    objects: Vec<TestObject>,
    external: Vec<(u64, String)>,
    form_b: bool,
}

impl Doc {
    pub(crate) fn class(&mut self, guid: &str, parent: &str, version: u32) -> usize {
        self.classes.push((guid.into(), parent.into(), version));
        self.classes.len() - 1
    }

    fn class_index(&self, guid: &str) -> usize {
        self.classes.iter().position(|c| c.0 == guid).unwrap()
    }

    pub(crate) fn obj(&mut self, id: u64, guid: &str, body: Vec<u8>) {
        let c = self.class_index(guid);
        self.objects.push((id, c, body, None));
    }

    fn obj_sub(&mut self, id: u64, guid: &str, body: Vec<u8>, sub: Vec<u8>) {
        let c = self.class_index(guid);
        self.objects.push((id, c, body, Some(sub)));
    }

    fn streams(&self) -> (Vec<u8>, Vec<u8>) {
        let mut bulk = Vec::new();
        let mut index = Vec::new();
        let mut subs = Vec::new();
        for (id, class, body, sub) in &self.objects {
            index.push((*id, bulk.len() as u64));
            bulk.extend(s8(&(256 + class).to_string()));
            bulk.extend(id.to_le_bytes());
            bulk.extend(s8(""));
            bulk.extend(body);
            if let Some(sub) = sub {
                subs.push((*id, bulk.len() as u64));
                bulk.extend(s8(&(256 + class).to_string()));
                bulk.extend(id.to_le_bytes());
                bulk.extend(sub);
            }
        }
        let mut m = s8("Design");
        m.extend(u32b(0));
        m.extend(s16("00000000-0000-0000-0000-000000000000"));
        if self.form_b {
            m.extend(u32b(1234));
            m.extend(u32b(8));
            m.extend(u32b(36));
            m.extend(u32b((2704 << 18) | (1 << 6)));
        } else {
            m.extend(u32b(0x0800_0000 | 20981));
            m.extend(u32b(5));
        }
        m.extend(s8("Design"));
        m.extend(s8(if self.form_b { "Design" } else { "" }));
        m.extend(u32b(1));
        m.extend(u32b(0));
        m.extend(u32b(self.classes.len() as u32));
        for (i, (guid, parent, version)) in self.classes.iter().enumerate() {
            m.extend(s8(guid));
            m.extend(s8(parent));
            m.extend(u32b(*version));
            m.extend(s8("Design"));
            let ids: Vec<u64> = self
                .objects
                .iter()
                .filter(|o| o.1 == i)
                .map(|o| o.0)
                .collect();
            m.extend(u32b(ids.len() as u32));
            for id in ids {
                m.extend(id.to_le_bytes());
            }
        }
        m.extend(u32b(0)); // named roots
        for list in [&index, &subs] {
            m.extend(u32b(list.len() as u32));
            for (id, off) in list.iter() {
                m.extend(id.to_le_bytes());
                m.extend(off.to_le_bytes());
            }
        }
        let next = self.objects.iter().map(|o| o.0).max().unwrap_or(0) + 1;
        m.extend(next.to_le_bytes());
        m.extend(u32b(self.external.len() as u32));
        for (key, urn) in &self.external {
            m.extend(key.to_le_bytes());
            for s in ["a", "b", urn.as_str(), "c"] {
                m.extend(s16(s));
            }
            m.extend(u32b(1));
        }
        if self.form_b {
            m.extend(u32b(2));
            m.extend(s8("Application"));
            m.extend(u32b(2));
            m.extend(s8("Server"));
            m.extend(u32b(6));
        }
        (m, bulk)
    }

    pub(crate) fn segment(&self) -> Segment {
        let (m, b) = self.streams();
        Segment::parse(&m, b).unwrap()
    }
}

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

#[test]
fn curve_kinds() {
    let seg = sample().segment();
    // The end of a curve's sketch-curve part: flags, the two f32 1.0 and
    // the sketch reference; the kind 22 bytes before the pair.
    let curve = |kind: u8| {
        let mut d = vec![7u8; 30];
        d.extend([0, 0, 0, 0, 0, 0, kind, 0, 0, 0, 1, 0, 0, 0, 0, 1]);
        d.extend([0; 12]);
        d.extend([0, 0, 0x80, 0x3f, 0, 0, 0x80, 0x3f]);
        d.extend([0; 6]);
        d
    };
    assert_eq!(sketch::curve_kind(&seg, &curve(0)), Some(0));
    assert_eq!(sketch::curve_kind(&seg, &curve(1)), Some(1));
    assert_eq!(sketch::curve_kind(&seg, &curve(2)), Some(2));
    assert_eq!(sketch::curve_kind(&seg, &curve(9)), None);
    // A reference where the flags would be: another layout.
    let mut d = curve(1);
    let at = d.len() - 6 - 8 - 24;
    d.splice(at..at + 11, r(32));
    assert_eq!(sketch::curve_kind(&seg, &d), None);
    assert_eq!(sketch::curve_kind(&seg, &[0; 40]), None);
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
    assert_eq!((dm.holder, dm.entities.clone()), (56, vec![50, 51]));
    assert_eq!(dm.expression.as_deref(), Some("10 mm"));
    assert_eq!(dm.text, Some([1.0, 0.5, 0.0]));
    assert_eq!(dm.flags.unwrap().len(), 21);
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
        json!([{"key": "1122334455667788", "urn": "urn:test:doc?version=1"}])
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
    let zip = crate::zip::stored_zip(&[
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
    let text = include_str!("../../tests/data/external_dump_box.json");
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
    // A hole through all at two points.
    for (id, x) in [(610, 1.0), (611, 5.0)] {
        d.obj(
            id,
            KEY_POINT,
            cat(&[&[0, 0], &[0; 21], &f64s(&[x, 1.0, 1.0]), &u32b(11)]),
        );
    }
    d.obj(
        600,
        HOLE,
        cat(&[
            &[0, 0],
            &u32b(0),
            &[1, 1, 1, 0],
            &u32b(2),
            &r(610),
            &r(611),
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
    // A mirror that makes one new body: not joined with its image.
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
            &0u64.to_le_bytes(),
            &tail(6, "Mirror", 1, "", [0, 0, 0], 404),
        ]),
    );
    for id in [401, 402, 404] {
        d.obj(id, HEALTH, vec![0, 0]);
    }
    d.obj(403, BODY_RECORD, vec![0, 0]);
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
    // A hole (class version 4) through all that keeps a depth.
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
    assert_eq!(hole["_f3d_through_all"], true);
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

    let seg = d.segment();
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
    for (input, rec) in [(711, 751), (712, 752), (721, 753), (731, 754)] {
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
    for (slot, input, key, role) in [(740, 750, 760, 17), (742, 752, 762, 17), (741, 70, 761, 65)] {
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
