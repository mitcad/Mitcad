// SPDX-License-Identifier: MIT
//! Small `.iam` assemblies made by Mitcad's own writers, for tests here, in
//! the bridge and in the command line tests: the records the assembly
//! reader reads (occurrences with their keys, flags and labels, document
//! descriptors, placements) and the `UFRxDoc` stream with the referenced
//! files and the occurrence table, in the layouts `README.md` describes.

use std::path::Path;

use crate::assembly::{
    ASSEMBLY_CLSID, DESCRIPTOR, DISPLAY, FLAG_GROUNDED, FLAG_HIDDEN, FLAG_SUPPRESSED, OCCURRENCE,
    OCCURRENCE_KEY, PLACEMENT, REFERENCES_STREAM,
};
use crate::cfb::{CompoundFile, EntryKind, Writer, guid_bytes};
use crate::dc::LABEL;
use crate::props::{self, Value};
use crate::rse;
use crate::{MODEL_SETTINGS, PID_LENGTH_UNIT};

/// Records of a segment: (type, bytes).
type Records = Vec<([u8; 16], Vec<u8>)>;

/// An occurrence of a test assembly.
#[derive(Clone, Debug)]
pub struct TestOccurrence {
    /// The referenced file's id ([`TestAssembly::references`]).
    pub reference: u32,
    pub key: u32,
    pub instance: u32,
    /// Its placement (cm), row-major.
    pub matrix: [[f64; 4]; 4],
    pub flags: u32,
    pub label: Option<String>,
    /// The referenced document's version id.
    pub document: [u8; 16],
    /// The referenced document's range box (cm).
    pub range: [[f64; 3]; 2],
}

/// What a test assembly holds.
#[derive(Clone, Debug)]
pub struct TestAssembly {
    /// Its own path as saved.
    pub saved_path: String,
    /// (id, saved path) of each referenced file.
    pub references: Vec<(u32, String)>,
    pub occurrences: Vec<TestOccurrence>,
    /// The model settings' length unit code (11269: millimetres).
    pub length_unit: i64,
}

fn text(out: &mut Vec<u8>, s: &str) {
    let units: Vec<u16> = s.encode_utf16().collect();
    out.extend_from_slice(&(units.len() as u32).to_le_bytes());
    for u in units {
        out.extend_from_slice(&u.to_le_bytes());
    }
}

fn u32s(out: &mut Vec<u8>, values: &[u32]) {
    for v in values {
        out.extend_from_slice(&v.to_le_bytes());
    }
}

fn f64s(out: &mut Vec<u8>, values: &[f64]) {
    for v in values {
        out.extend_from_slice(&v.to_le_bytes());
    }
}

/// A one-based reference to record `i`, with the flag bit.
fn reference(i: usize) -> u32 {
    (i as u32 + 1) | 0x8000_0000
}

/// The 22-byte header (segment major 25).
fn header(out: &mut Vec<u8>, id: u16, flags: u32) {
    u32s(out, &[0]);
    out.extend_from_slice(&id.to_le_bytes());
    u32s(out, &[0, flags, 0, u32::from(id)]);
}

/// A masked matrix: every element stored but the last row (0, 0, 0, 1).
fn matrix(out: &mut Vec<u8>, m: &[[f64; 4]; 4]) {
    u32s(out, &[0x7000_8000]);
    for row in &m[..3] {
        f64s(out, row);
    }
}

fn apply(m: &[[f64; 4]; 4], p: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|r| m[r][0] * p[0] + m[r][1] * p[1] + m[r][2] * p[2] + m[r][3])
}

/// The occurrence records of the definitions segment and the reference
/// segment's descriptors and placements.
fn segments(a: &TestAssembly) -> (Records, Records) {
    let mut dc: Vec<([u8; 16], Vec<u8>)> = vec![([0x21; 16], vec![0; 30])];
    let mut rx: Vec<([u8; 16], Vec<u8>)> = vec![([0x31; 16], vec![0; 20])];
    for (k, o) in a.occurrences.iter().enumerate() {
        let id = 10 + k as u16;
        let occurrence = dc.len();
        // The proxy (a suppressed occurrence's names no key), the label.
        let proxy = occurrence + 1;
        let mut b = Vec::new();
        header(&mut b, id, 0x0000_0200 | o.flags);
        u32s(&mut b, &[u32::MAX, k as u32 + 3, reference(proxy)]);
        dc.push((OCCURRENCE, b));
        let mut p = Vec::new();
        header(&mut p, id + 100, 0x0000_0200);
        p.extend_from_slice(&[0xFF; 8]);
        if o.flags & FLAG_SUPPRESSED != 0 {
            dc.push(([0x22; 16], p));
        } else {
            p.extend_from_slice(&[0, 2]);
            u32s(&mut p, &[o.key]);
            text(&mut p, "DCx");
            p.extend_from_slice(&[1, 0]);
            dc.push((OCCURRENCE_KEY, p));
        }
        if let Some(label) = &o.label {
            let mut l = Vec::new();
            l.extend_from_slice(&[0; 14]);
            u32s(&mut l, &[reference(occurrence), 0, 0, 0]);
            l.extend_from_slice(&2u16.to_le_bytes());
            l.extend_from_slice(&0x3000u16.to_le_bytes());
            u32s(&mut l, &[0]);
            text(&mut l, label);
            l.extend_from_slice(&[0; 16]);
            dc.push((LABEL, l));
        }
        if o.flags & FLAG_SUPPRESSED != 0 {
            continue;
        }
        // The descriptor and the placement.
        let descriptor = rx.len();
        let mut d = Vec::new();
        u32s(&mut d, &[0]);
        d.extend_from_slice(&id.to_le_bytes());
        d.extend_from_slice(&[0, 2]);
        u32s(&mut d, &[o.key]);
        text(&mut d, "AmRx");
        d.extend_from_slice(&[1, 0]);
        u32s(&mut d, &[0]);
        d.extend_from_slice(&o.document);
        d.extend_from_slice(&[0x55; 32]);
        // The bodies: a solid with the range box, a shown group of two
        // surfaces within it and a hidden surface outside it, each with
        // its range box and colour.
        u32s(&mut d, &[0x3000_0006, 3, 0]);
        u32s(
            &mut d,
            &[0x3000_0002, 2, 0x3000_0002, 1, 1, 0x1000_0000, 2, 0x01],
        );
        f64s(&mut d, &o.range[0]);
        f64s(&mut d, &o.range[1]);
        text(&mut d, "160,160,160");
        u32s(&mut d, &[85, 0x3000_0002, 2, 2, 0x1000_0000, 85, 0x22]);
        f64s(&mut d, &o.range[0]);
        f64s(&mut d, &o.range[0]);
        text(&mut d, "");
        u32s(&mut d, &[3500, 0x3000_0002, 1, 1, 0x1000_0000, 3500, 0x2a]);
        f64s(&mut d, &o.range[1].map(|v| v + 1.0));
        f64s(&mut d, &o.range[1].map(|v| v + 2.0));
        text(&mut d, "");
        text(&mut d, "Default");
        rx.push((DESCRIPTOR, d));
        let mut m = Vec::new();
        u32s(&mut m, &[0]);
        m.extend_from_slice(&id.to_le_bytes());
        u32s(&mut m, &[reference(descriptor)]);
        matrix(&mut m, &o.matrix);
        let center = apply(
            &o.matrix,
            std::array::from_fn(|i| (o.range[0][i] + o.range[1][i]) / 2.0),
        );
        f64s(
            &mut m,
            &[
                0.0,
                0.0,
                1.0,
                o.matrix[0][0],
                o.matrix[1][0],
                o.matrix[2][0],
                center[0],
                center[1],
                center[2],
            ],
        );
        rx.push((PLACEMENT, m));
    }
    (dc, rx)
}

/// The graphics segment's display transforms of the loaded occurrences.
fn graphics(a: &TestAssembly) -> Vec<([u8; 16], Vec<u8>)> {
    let mut out = vec![([0x41; 16], vec![0; 12])];
    for o in a
        .occurrences
        .iter()
        .filter(|o| o.flags & FLAG_SUPPRESSED == 0)
    {
        let mut g = vec![0; 15];
        matrix(&mut g, &o.matrix);
        g.extend_from_slice(&[0, 7]);
        u32s(&mut g, &[o.key]);
        text(&mut g, "GRx");
        g.extend_from_slice(&[1, 0]);
        out.push((DISPLAY, g));
    }
    out
}

/// The `UFRxDoc` stream: the own path, the files and the occurrence table.
fn references_stream(a: &TestAssembly) -> Vec<u8> {
    let mut s = Vec::new();
    s.extend_from_slice(&[0x0C, 0, 0x18, 0, 0, 0, 0, 0]);
    text(&mut s, &a.saved_path);
    u32s(&mut s, &[0, 1]);
    text(&mut s, "Default");
    text(&mut s, "DesignView");
    u32s(&mut s, &[2]);
    text(&mut s, "Default");
    s.extend_from_slice(&[0; 6]);
    let clsid = crate::assembly::stream_guid(&guid_bytes(ASSEMBLY_CLSID).expect("class id"));
    s.extend_from_slice(&clsid);
    s.extend_from_slice(&clsid);
    u32s(&mut s, &[0, a.references.len() as u32, 0, 0]);
    for (id, path) in &a.references {
        text(&mut s, path);
        u32s(&mut s, &[0x1C]);
        text(&mut s, "");
        s.extend_from_slice(&[0, 0]);
        text(&mut s, "");
        s.extend_from_slice(&[0; 8]);
        s.extend_from_slice(&[0x66; 16]);
        s.extend_from_slice(&[0x77; 16]);
        u32s(&mut s, &[*id, 4, 1, 1]);
    }
    // The end of the list, the occurrence table.
    text(&mut s, "");
    s.extend_from_slice(&[0, 0]);
    let loaded: Vec<&TestOccurrence> = a
        .occurrences
        .iter()
        .filter(|o| o.flags & FLAG_SUPPRESSED == 0)
        .collect();
    u32s(&mut s, &[loaded.len() as u32, 0]);
    for (k, o) in loaded.iter().enumerate() {
        u32s(&mut s, &[o.reference, o.key, k as u32 + 1, o.instance]);
        s.extend_from_slice(&[0; 8]);
        s.extend_from_slice(&[1, 0, 0, 0x11, 0, 0, 0, 0x11]);
        s.extend_from_slice(&[0x11; 4]);
        s.extend_from_slice(&o.document);
        s.extend_from_slice(&[0; 9]);
    }
    s
}

/// Writes the assembly file.
pub fn assembly_file(a: &TestAssembly) -> Vec<u8> {
    let (dc, rx) = segments(a);
    let write = |name: &str, id: u8, records: &[([u8; 16], Vec<u8>)]| {
        let mut types: Vec<[u8; 16]> = Vec::new();
        let records: Vec<(u8, Vec<u8>, Vec<u8>)> = records
            .iter()
            .map(|(t, b)| {
                let k = types.iter().position(|x| x == t).unwrap_or_else(|| {
                    types.push(*t);
                    types.len() - 1
                });
                (k as u8, b.clone(), Vec::new())
            })
            .collect();
        rse::write_segment_with(name, [id; 16], 25, &types, &records)
    };
    let (m1, b1) = write(crate::assembly::DEFINITIONS_SEGMENT, 0x51, &dc);
    let (m2, b2) = write(crate::assembly::REFERENCES_SEGMENT, 0x52, &rx);
    let (m3, b3) = write(crate::assembly::GRAPHICS_SEGMENT, 0x53, &graphics(a));
    let settings = props::write(
        guid_bytes(MODEL_SETTINGS).expect("format id"),
        &[(PID_LENGTH_UNIT, Value::Int(a.length_unit))],
    );
    let mut w = Writer::new();
    w.clsid("", guid_bytes(ASSEMBLY_CLSID).expect("class id"))
        .stream("\u{5}ModelSettings", &settings)
        .stream("RSeStorage/Mtestdcsegment", &m1)
        .stream("RSeStorage/Btestdcsegment", &b1)
        .stream("RSeStorage/Mtestrxsegment", &m2)
        .stream("RSeStorage/Btestrxsegment", &b2)
        .stream("RSeStorage/Mtestgraphicssegment", &m3)
        .stream("RSeStorage/Btestgraphicssegment", &b3)
        .stream(REFERENCES_STREAM, &references_stream(a));
    w.finish()
}

/// A part file with a `UFRxDoc` stream that lists its version id (as a
/// part's own list does), the other streams as they are.
pub fn with_document_id(part: &[u8], id: [u8; 16]) -> Vec<u8> {
    let mut list = vec![0; 40];
    list.extend_from_slice(&id);
    list.extend_from_slice(&[0; 8]);
    with_stream(part, REFERENCES_STREAM, &list)
}

/// A compound file with one more stream (or one written anew), the other
/// streams as they are.
pub fn with_stream(file: &[u8], stream: &str, bytes: &[u8]) -> Vec<u8> {
    let file = CompoundFile::parse(file.to_vec()).expect("a compound file");
    let mut w = Writer::new();
    w.clsid("", file.root().clsid);
    for (path, entry) in file.entries() {
        if entry.kind == EntryKind::Stream
            && path.trim_start_matches('/') != stream.trim_start_matches('/')
        {
            let data = file.read(entry).expect("readable");
            w.stream(path, &data);
        }
    }
    w.stream(stream, bytes);
    w.finish()
}

/// A rotation about z by `degrees`, then a translation (cm).
pub fn placement(degrees: f64, t: [f64; 3]) -> [[f64; 4]; 4] {
    let (s, c) = degrees.to_radians().sin_cos();
    [
        [c, -s, 0.0, t[0]],
        [s, c, 0.0, t[1]],
        [0.0, 0.0, 1.0, t[2]],
        [0.0, 0.0, 0.0, 1.0],
    ]
}

/// The version ids of the test parts.
pub const CUBE_ID: [u8; 16] = [0xC1; 16];
pub const PAIR_ID: [u8; 16] = [0xC2; 16];
pub const SUB_ID: [u8; 16] = [0xC3; 16];

/// The test project's top assembly, as saved on another machine in
/// `C:\Elsewhere\Project`: two occurrences of the cube part (one rotated
/// and moved, one hidden), the sub-assembly (grounded), a part that is
/// missing, a suppressed occurrence and a part saved under another path
/// that is found by its name.
pub fn top_assembly() -> TestAssembly {
    let cube = [[0.0, 0.0, 0.0], [1.0, 1.0, 1.0]];
    let pair = [[-1.0, -1.0, 0.0], [1.0, 1.0, 2.0]];
    let occurrence = |reference, key, instance, matrix, flags, document, range| TestOccurrence {
        reference,
        key,
        instance,
        matrix,
        flags,
        label: None,
        document,
        range,
    };
    TestAssembly {
        saved_path: r"C:\Elsewhere\Project\top.iam".into(),
        references: vec![
            (2, r"C:\Elsewhere\Project\cube.ipt".into()),
            (3, r"C:\Elsewhere\Project\sub\sub.iam".into()),
            (5, r"C:\Elsewhere\Library\missing.ipt".into()),
            (6, r"D:\Other\Place\found.ipt".into()),
        ],
        occurrences: vec![
            occurrence(2, 2, 1, placement(90.0, [5.0, 0.0, 0.0]), 0, CUBE_ID, cube),
            occurrence(
                2,
                3,
                2,
                placement(0.0, [0.0, 0.0, 3.0]),
                FLAG_HIDDEN,
                CUBE_ID,
                cube,
            ),
            occurrence(
                3,
                4,
                1,
                placement(0.0, [0.0, 10.0, 0.0]),
                FLAG_GROUNDED,
                SUB_ID,
                pair,
            ),
            occurrence(
                5,
                5,
                1,
                placement(0.0, [20.0, 0.0, 0.0]),
                0,
                [0xC4; 16],
                cube,
            ),
            occurrence(
                2,
                6,
                3,
                placement(0.0, [0.0, 0.0, 0.0]),
                FLAG_SUPPRESSED,
                CUBE_ID,
                cube,
            ),
            TestOccurrence {
                label: Some("Found by name".into()),
                ..occurrence(
                    6,
                    7,
                    1,
                    placement(180.0, [0.0, -5.0, 0.0]),
                    0,
                    PAIR_ID,
                    pair,
                )
            },
        ],
        length_unit: 11269,
    }
}

/// The sub-assembly, saved in `C:\Elsewhere\Project\sub`: two occurrences
/// of the pair part in `..\parts`.
pub fn sub_assembly() -> TestAssembly {
    let pair = [[-1.0, -1.0, 0.0], [1.0, 1.0, 2.0]];
    TestAssembly {
        saved_path: r"C:\Elsewhere\Project\sub\sub.iam".into(),
        references: vec![(1, r"C:\Elsewhere\Project\parts\pair.ipt".into())],
        occurrences: (0..2)
            .map(|k| TestOccurrence {
                reference: 1,
                key: k + 1,
                instance: k + 1,
                matrix: placement(45.0 * f64::from(k), [3.0 * f64::from(k), 0.0, 0.0]),
                flags: 0,
                label: None,
                document: PAIR_ID,
                range: pair,
            })
            .collect(),
        length_unit: 11269,
    }
}

/// An assembly of placements that mirror or scale (mitcad#1903), saved in
/// `C:\Elsewhere\Project`: the cube as it is, mirrored in x and moved
/// 5 cm along x, scaled by 2 and moved 20 cm along y, and the
/// sub-assembly mirrored in y and moved 10 cm along -y.
pub fn mirrored_assembly() -> TestAssembly {
    let cube = [[0.0, 0.0, 0.0], [1.0, 1.0, 1.0]];
    let pair = [[-1.0, -1.0, 0.0], [1.0, 1.0, 2.0]];
    let diagonal = |d: [f64; 3], t: [f64; 3]| {
        [
            [d[0], 0.0, 0.0, t[0]],
            [0.0, d[1], 0.0, t[1]],
            [0.0, 0.0, d[2], t[2]],
            [0.0, 0.0, 0.0, 1.0],
        ]
    };
    let occurrence = |reference, key, instance, matrix, document, range| TestOccurrence {
        reference,
        key,
        instance,
        matrix,
        flags: 0,
        label: None,
        document,
        range,
    };
    TestAssembly {
        saved_path: r"C:\Elsewhere\Project\mirrored.iam".into(),
        references: vec![
            (2, r"C:\Elsewhere\Project\cube.ipt".into()),
            (3, r"C:\Elsewhere\Project\sub\sub.iam".into()),
        ],
        occurrences: vec![
            occurrence(2, 2, 1, placement(0.0, [0.0; 3]), CUBE_ID, cube),
            occurrence(
                2,
                3,
                2,
                diagonal([-1.0, 1.0, 1.0], [5.0, 0.0, 0.0]),
                CUBE_ID,
                cube,
            ),
            occurrence(
                2,
                4,
                3,
                diagonal([2.0, 2.0, 2.0], [0.0, 20.0, 0.0]),
                CUBE_ID,
                cube,
            ),
            occurrence(
                3,
                5,
                1,
                diagonal([1.0, -1.0, 1.0], [0.0, -10.0, 0.0]),
                SUB_ID,
                pair,
            ),
        ],
        length_unit: 11269,
    }
}

/// The version ids of the converted assembly's parts without bodies.
pub const EMPTY_ID: [u8; 16] = [0xC5; 16];
pub const BARE_ID: [u8; 16] = [0xC6; 16];
pub const LISTED_ID: [u8; 16] = [0xC7; 16];

/// Where the converted assembly keeps its translation report: an embedded
/// object's OLE 1.0 package, where the converted assembly of the test files
/// keeps it.
pub const CONVERTED_REPORT_STREAM: &str = "RSeStorage/RSeEmbeddings/Embedding 1/\u{1}Ole10Native";

/// An assembly converted from a building model (mitcad#1913), saved in
/// `C:\Elsewhere\Project`: the cube, two occurrences of a part the
/// conversion made without bodies (`empty.ipt`), a part without bodies
/// its report does not list (`bare.ipt`) and a part whose result lists a
/// body its B-rep record does not hold (`listed.ipt`).
pub fn converted_assembly() -> TestAssembly {
    let cube = [[0.0, 0.0, 0.0], [1.0, 1.0, 1.0]];
    let occurrence = |reference, key, x: f64, document| TestOccurrence {
        reference,
        key,
        instance: 1,
        matrix: placement(0.0, [x, 0.0, 0.0]),
        flags: 0,
        label: None,
        document,
        range: cube,
    };
    TestAssembly {
        saved_path: r"C:\Elsewhere\Project\converted.iam".into(),
        references: vec![
            (2, r"C:\Elsewhere\Project\cube.ipt".into()),
            (3, r"C:\Elsewhere\Project\empty.ipt".into()),
            (4, r"C:\Elsewhere\Project\bare.ipt".into()),
            (5, r"C:\Elsewhere\Project\listed.ipt".into()),
        ],
        occurrences: vec![
            occurrence(2, 2, 0.0, CUBE_ID),
            occurrence(3, 3, 5.0, EMPTY_ID),
            TestOccurrence {
                instance: 2,
                ..occurrence(3, 4, 10.0, EMPTY_ID)
            },
            occurrence(4, 5, 15.0, BARE_ID),
            occurrence(5, 6, 20.0, LISTED_ID),
        ],
        length_unit: 11269,
    }
}

/// The translation report the converted assembly keeps: from a `.rvt`
/// file, the assembly and the cube made without messages, the empty part
/// with the message the conversion gives a part it read no bodies for.
pub fn converted_report() -> crate::translation::Report {
    use crate::translation::{Component, Message, Report};
    let made = |name: &str, status: &str, messages: &[&str]| Component {
        name: name.into(),
        status: status.into(),
        messages: messages
            .iter()
            .map(|m| Message {
                severity: "Warning".into(),
                text: (*m).into(),
            })
            .collect(),
    };
    Report {
        source: Some(r"C:\Elsewhere\Models\building.rvt".into()),
        translator: Some("Building model".into()),
        components: vec![
            made("converted.iam", "Successful", &[]),
            made("cube.ipt", "Successful", &[]),
            made("empty.ipt", "Warning", &["No Bodies Read From File."]),
        ],
    }
}

/// The converted assembly's file, its translation report embedded.
pub fn converted_file() -> Vec<u8> {
    let page = utf16(&write_html(&converted_report()));
    with_stream(
        &assembly_file(&converted_assembly()),
        CONVERTED_REPORT_STREAM,
        &write_package("converted.htm", &page),
    )
}

/// Writes the test project into `dir`: `top.iam`, `mirrored.iam`,
/// `converted.iam`, `cube.ipt` (the cube with its design), `sub/sub.iam`,
/// `parts/pair.ipt` (the cube and the cylinder, bodies only),
/// `elsewhere/found.ipt` (the same pair) and the converted assembly's
/// parts without bodies, `empty.ipt`, `bare.ipt` and `listed.ipt`; the
/// missing part is not written.
pub fn write_test_project(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir.join("sub"))?;
    std::fs::create_dir_all(dir.join("parts"))?;
    std::fs::create_dir_all(dir.join("elsewhere"))?;
    std::fs::write(dir.join("top.iam"), assembly_file(&top_assembly()))?;
    std::fs::write(
        dir.join("mirrored.iam"),
        assembly_file(&mirrored_assembly()),
    )?;
    std::fs::write(
        dir.join("sub").join("sub.iam"),
        assembly_file(&sub_assembly()),
    )?;
    std::fs::write(
        dir.join("cube.ipt"),
        with_document_id(&crate::testdata::test_part_with_design(), CUBE_ID),
    )?;
    let pair = with_document_id(&crate::testdata::test_part(), PAIR_ID);
    std::fs::write(dir.join("parts").join("pair.ipt"), &pair)?;
    std::fs::write(dir.join("elsewhere").join("found.ipt"), &pair)?;
    std::fs::write(dir.join("converted.iam"), converted_file())?;
    let empty = crate::testdata::test_empty_part();
    std::fs::write(dir.join("empty.ipt"), with_document_id(&empty, EMPTY_ID))?;
    std::fs::write(dir.join("bare.ipt"), with_document_id(&empty, BARE_ID))?;
    std::fs::write(
        dir.join("listed.ipt"),
        with_document_id(
            &crate::testdata::test_part_listing_a_missing_body(),
            LISTED_ID,
        ),
    )?;
    Ok(())
}

/// A page in the layout of a translation report (mitcad#1913): the header
/// with the source file and the translator, and a row per document with
/// its messages.
pub fn write_html(report: &crate::translation::Report) -> String {
    use crate::translation::REPORT_NAMESPACE;
    let mut html = format!(
        "<html {REPORT_NAMESPACE}=\"urn:test\">\r\n<head><title>Translation Report</title></head>\r\n<body>\
         <table><tr><td class=\"header-heading\">Source File:</td><td>{}</td></tr>\
         <tr><td class=\"header-heading\">Translator Type:</td><td>{}</td></tr></table>\
         <table><tr><th>Name</th><th>File</th><th>Status</th></tr>",
        report.source.as_deref().unwrap_or_default(),
        report.translator.as_deref().unwrap_or_default(),
    );
    for (i, c) in report.components.iter().enumerate() {
        html += &format!(
            "<tr class=\"row-style0\" id=\"component-row{i}\"><td><div>[+]</div></td><td>{}</td>\
             <td><a href=\"C:\\test\\{0}\">C:\\test\\{0}</a></td><td><div>{}</div></td></tr>",
            c.name, c.status
        );
        html += &format!(
            "<tr id=\"component-details{i}\" style=\"display: none\"><td><p></p><table>\
             <tr><th>Severity</th><th>Entity Id</th><th>Message</th></tr>"
        );
        for m in &c.messages {
            html += &format!(
                "<tr><td><div>{}</div></td><td>0</td><td>{}</td></tr>",
                m.severity, m.text
            );
        }
        html += "</table><p></p></td></tr>";
    }
    html + "</table></body></html>\r\n"
}

/// An OLE 1.0 package stream holding `data` as the file `label`.
pub fn write_package(label: &str, data: &[u8]) -> Vec<u8> {
    let path = format!("C:\\test\\{label}");
    let mut body = 2u16.to_le_bytes().to_vec();
    for text in [label, path.as_str()] {
        body.extend_from_slice(text.as_bytes());
        body.push(0);
    }
    body.extend_from_slice(&0x0003_0000u32.to_le_bytes());
    body.extend_from_slice(&(path.len() as u32 + 1).to_le_bytes());
    body.extend_from_slice(path.as_bytes());
    body.push(0);
    body.extend_from_slice(&(data.len() as u32).to_le_bytes());
    body.extend_from_slice(data);
    let mut out = (body.len() as u32).to_le_bytes().to_vec();
    out.extend_from_slice(&body);
    out
}

/// The page as UTF-16 with a byte order mark, as reports are kept.
pub fn utf16(text: &str) -> Vec<u8> {
    let mut out = vec![0xFF, 0xFE];
    for unit in text.encode_utf16() {
        out.extend_from_slice(&unit.to_le_bytes());
    }
    out
}
