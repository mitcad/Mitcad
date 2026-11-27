// SPDX-License-Identifier: MIT
//! Damaged DXF files (mitcad#939): cut short anywhere, not DXF text at
//! all, or declaring counts far beyond what they hold. Each is read as far
//! as it goes or refused, never a panic, and a declared count reserves
//! nothing.

use mitcad_dxf::read;

/// DXF text of group pairs.
fn dxf(pairs: &[(i32, &str)]) -> String {
    pairs
        .iter()
        .map(|(code, value)| format!("{code}\n{value}\n"))
        .collect()
}

/// A file with a header, a block and entities of most kinds.
fn file() -> String {
    dxf(&[
        (0, "SECTION"),
        (2, "HEADER"),
        (9, "$INSUNITS"),
        (70, "4"),
        (0, "ENDSEC"),
        (0, "SECTION"),
        (2, "BLOCKS"),
        (0, "BLOCK"),
        (8, "0"),
        (2, "BOLT"),
        (10, "0"),
        (20, "0"),
        (0, "CIRCLE"),
        (8, "Holes"),
        (10, "0"),
        (20, "0"),
        (40, "2"),
        (0, "ENDBLK"),
        (0, "ENDSEC"),
        (0, "SECTION"),
        (2, "ENTITIES"),
        (0, "LINE"),
        (8, "0"),
        (10, "0"),
        (20, "0"),
        (11, "10"),
        (21, "5"),
        (0, "LWPOLYLINE"),
        (8, "0"),
        (90, "3"),
        (70, "1"),
        (10, "0"),
        (20, "0"),
        (42, "0.5"),
        (10, "10"),
        (20, "0"),
        (10, "10"),
        (20, "10"),
        (0, "SPLINE"),
        (8, "0"),
        (70, "8"),
        (71, "3"),
        (72, "0"),
        (73, "0"),
        (74, "4"),
        (11, "0"),
        (21, "0"),
        (11, "1"),
        (21, "2"),
        (11, "3"),
        (21, "1"),
        (11, "4"),
        (21, "3"),
        (0, "INSERT"),
        (8, "0"),
        (2, "BOLT"),
        (10, "20"),
        (20, "20"),
        (70, "3"),
        (71, "2"),
        (44, "10"),
        (45, "10"),
        (0, "ENDSEC"),
        (0, "EOF"),
    ])
}

#[test]
fn every_cut_of_a_file_is_read_as_far_as_it_goes_or_refused() {
    let full = file();
    let whole = read(full.as_bytes()).unwrap();
    for cut in 0..full.len() {
        if let Ok(drawing) = read(&full.as_bytes()[..cut]) {
            assert!(
                drawing.entities.len() <= whole.entities.len(),
                "cut at {cut}: {} entities of {}",
                drawing.entities.len(),
                whole.entities.len()
            );
        }
    }
}

#[test]
fn other_files_are_not_dxf() {
    // A PNG image, a zip archive and a compound file start as no DXF
    // group code does.
    assert!(read(b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR").is_err());
    assert!(read(b"PK\x03\x04\x14\0\0\0\x08\0").is_err());
    assert!(read(b"\xd0\xcf\x11\xe0\xa1\xb1\x1a\xe1\0\0").is_err());
}

#[test]
fn declared_counts_beyond_the_file_reserve_nothing() {
    // A polyline declaring 2^31 - 1 vertices with two, a spline declaring
    // a billion knots and control points with a handful, and an array of
    // 2^31 - 1 by 2^31 - 1 cells (cut to the array and read budgets).
    let text = dxf(&[
        (0, "SECTION"),
        (2, "BLOCKS"),
        (0, "BLOCK"),
        (8, "0"),
        (2, "DOT"),
        (10, "0"),
        (20, "0"),
        (0, "POINT"),
        (10, "0"),
        (20, "0"),
        (0, "ENDBLK"),
        (0, "ENDSEC"),
        (0, "SECTION"),
        (2, "ENTITIES"),
        (0, "LWPOLYLINE"),
        (8, "0"),
        (90, "2147483647"),
        (10, "0"),
        (20, "0"),
        (10, "1"),
        (20, "1"),
        (0, "SPLINE"),
        (8, "0"),
        (70, "8"),
        (71, "3"),
        (72, "1000000000"),
        (73, "1000000000"),
        (40, "0"),
        (40, "1"),
        (10, "0"),
        (20, "0"),
        (10, "1"),
        (20, "1"),
        (0, "INSERT"),
        (8, "0"),
        (2, "DOT"),
        (10, "0"),
        (20, "0"),
        (70, "2147483647"),
        (71, "2147483647"),
        (44, "1"),
        (45, "1"),
        (0, "ENDSEC"),
        (0, "EOF"),
    ]);
    let drawing = read(text.as_bytes()).unwrap();
    assert!(drawing.entities.len() <= mitcad_dxf::READ_BUDGET as usize);
}
