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
struct Doc {
    classes: Vec<(String, String, u32)>,
    objects: Vec<TestObject>,
    external: Vec<(u64, String)>,
    form_b: bool,
}

impl Doc {
    fn class(&mut self, guid: &str, parent: &str, version: u32) -> usize {
        self.classes.push((guid.into(), parent.into(), version));
        self.classes.len() - 1
    }

    fn class_index(&self, guid: &str) -> usize {
        self.classes.iter().position(|c| c.0 == guid).unwrap()
    }

    fn obj(&mut self, id: u64, guid: &str, body: Vec<u8>) {
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

    fn segment(&self) -> Segment {
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
            &[0],
            &f64s(&[1.0]),
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
