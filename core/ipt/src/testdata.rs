// SPDX-License-Identifier: MIT
//! Small `.ipt` files made by Mitcad's own writers, for tests here, in the
//! bridge and in the command line and UI tests (no third-party files are
//! needed): a compound file with the segment database's streams, a B-rep
//! segment whose B-rep records hold ASM bodies written by the `.f3d`
//! reader's ASM writer (`mitcad_f3d::testdata`), and the property sets the
//! import reads.

use crate::cfb::{Writer, guid_bytes};
use crate::props::{self, Value};
use crate::rse;
use crate::{
    BREP_RECORD_TYPE, MODEL_SETTINGS, PART_CLSID, PID_LENGTH_UNIT, PID_MATERIAL, PID_PART_NUMBER,
    PID_RELEASE, TRACKING_PROPERTIES,
};

/// Definitions segment records: (type, bytes).
pub type Records = Vec<([u8; 16], Vec<u8>)>;

/// What a test part holds.
pub struct TestPart<'a> {
    /// One B-rep record per ASM file.
    pub bodies: Vec<Vec<u8>>,
    pub part_number: &'a str,
    pub material: &'a str,
    /// The model settings' length unit code (11269: millimetres).
    pub length_unit: i64,
    /// The definitions segment's records (`testdesign`) and its major
    /// version; None for a segment of one record that is nothing.
    pub design: Option<(u8, Records)>,
}

/// The B-rep record of an ASM file: a 14-byte head, the file and an
/// 18-byte trailer, as in the files seen.
pub fn brep_record(asm: &[u8]) -> Vec<u8> {
    let mut r = vec![0, 0, 0, 0, 2, 0, 0x1D, 3, 0, 0, 0x2B, 0, 0, 0];
    r.extend_from_slice(asm);
    r.extend_from_slice(&[
        6, 0, 0, 0x80, 1, 0, 0, 0, 0, 0, 7, 0, 0, 0x80, 0xFF, 0xFF, 0xFF, 0xFF,
    ]);
    r
}

/// Writes the part file.
pub fn part_file(part: &TestPart) -> Vec<u8> {
    // Another record type before the B-rep's, so that the type index
    // matters.
    let types = [[0x11; 16], BREP_RECORD_TYPE];
    let mut records = vec![(0u8, vec![1, 2, 3, 4])];
    records.extend(part.bodies.iter().map(|asm| (1u8, brep_record(asm))));
    records.push((0, vec![5; 10]));
    let (m, b) = rse::write_segment("PmBRepSegment", [0x42; 16], &types, &records);
    // The definitions, or a segment without B-rep records.
    let (m2, b2) = match &part.design {
        Some((major, records)) => {
            let mut types: Vec<[u8; 16]> = Vec::new();
            let records: Vec<(u8, Vec<u8>, Vec<u8>)> = records
                .iter()
                .map(|(t, bytes)| {
                    let k = types.iter().position(|x| x == t).unwrap_or_else(|| {
                        types.push(*t);
                        types.len() - 1
                    });
                    (k as u8, bytes.clone(), Vec::new())
                })
                .collect();
            rse::write_segment_with("PmDCSegment", [0x43; 16], *major, &types, &records)
        }
        None => rse::write_segment(
            "PmDCSegment",
            [0x43; 16],
            &[[0x12; 16]],
            &[(0, vec![0; 20])],
        ),
    };
    let tracking = props::write(
        guid_bytes(TRACKING_PROPERTIES).expect("format id"),
        &[
            (PID_PART_NUMBER, Value::Text(part.part_number.into())),
            (PID_MATERIAL, Value::Text(part.material.into())),
            (PID_RELEASE, Value::Text("Mitcad test writer".into())),
        ],
    );
    let settings = props::write(
        guid_bytes(MODEL_SETTINGS).expect("format id"),
        &[(PID_LENGTH_UNIT, Value::Int(part.length_unit))],
    );
    let mut w = Writer::new();
    w.clsid("", guid_bytes(PART_CLSID).expect("class id"))
        .stream("\u{5}TrackingProperties", &tracking)
        .stream("\u{5}ModelSettings", &settings)
        .stream("RSeStorage/RSeSegInfo", &[0; 8])
        .stream("RSeStorage/Mtestbrepsegment", &m)
        .stream("RSeStorage/Btestbrepsegment", &b)
        .stream("RSeStorage/Mtestdcsegment", &m2)
        .stream("RSeStorage/Btestdcsegment", &b2);
    w.finish()
}

/// A part with two bodies (one B-rep record each): the 10 mm cube and the
/// cylinder of radius 10 mm and height 20 mm of `mitcad_f3d::testdata`,
/// part number `MITCAD-TEST-1`, material `Aluminum`, inches.
pub fn test_part() -> Vec<u8> {
    part_file(&TestPart {
        bodies: vec![
            mitcad_f3d::testdata::cube_blob(),
            mitcad_f3d::testdata::cylinder_blob(),
        ],
        part_number: "MITCAD-TEST-1",
        material: "Aluminum",
        length_unit: 11272,
        design: None,
    })
}

/// A part with the 10 mm cube as its body and the design that makes it
/// (`testdesign::cube_design`): a parameter, a sketch and an extrusion;
/// millimetres. The B-rep record has no ASM history.
pub fn test_part_with_design() -> Vec<u8> {
    part_file(&TestPart {
        bodies: vec![mitcad_f3d::testdata::cube_blob()],
        part_number: "MITCAD-TEST-2",
        material: "Steel",
        length_unit: 11269,
        design: Some((25, crate::testdesign::cube_design(25))),
    })
}
