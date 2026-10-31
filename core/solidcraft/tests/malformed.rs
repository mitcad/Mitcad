// SPDX-License-Identifier: MIT
//! Damaged `.solidcraft` designs (mitcad#939): a design or its container
//! cut short anywhere, a container of another format, and mesh blobs
//! whose declared counts exceed their bytes. Each is refused or read as
//! far as it goes, never a panic.

use mitcad_solidcraft::{SolidcraftFile, container, looks_like_design};
use mitcad_zip::stored_zip;

const DESIGN: &str = include_str!("designs/box-fillet-hole.solidcraft.json");

fn mesh(coords: u64, indices: u64) -> Vec<u8> {
    let mut mesh = b"SCMESH01".to_vec();
    mesh.extend(coords.to_le_bytes());
    mesh.extend(indices.to_le_bytes());
    for x in [0.0f64, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0] {
        mesh.extend(x.to_le_bytes());
    }
    for i in [0u32, 1, 2] {
        mesh.extend(i.to_le_bytes());
    }
    mesh
}

fn container_of(document: &str, format: &str) -> Vec<u8> {
    let version = format!("{{\"format\":\"{format}\"}}");
    stored_zip(&[
        ("container.json", version.as_bytes()),
        ("document.json", document.as_bytes()),
    ])
}

#[test]
fn every_cut_of_a_design_and_its_container_is_refused_or_read() {
    assert!(SolidcraftFile::from_json(DESIGN).is_ok());
    for cut in (0..DESIGN.len()).filter(|&i| DESIGN.is_char_boundary(i)) {
        let _ = SolidcraftFile::from_bytes(&DESIGN.as_bytes()[..cut]);
    }
    let full = container_of(DESIGN, "solidcraft-container/1");
    assert!(SolidcraftFile::from_bytes(&full).is_ok());
    for cut in 0..full.len() {
        let part = &full[..cut];
        let _ = looks_like_design(part);
        let _ = SolidcraftFile::from_bytes(part);
    }
}

#[test]
fn a_container_of_another_format_is_refused() {
    let other = container_of(DESIGN, "other-container/1");
    assert!(SolidcraftFile::from_bytes(&other).is_err());
    assert!(!looks_like_design(&other));
    assert!(SolidcraftFile::from_bytes(b"PK\x03\x04 not a zip").is_err());
    assert!(SolidcraftFile::from_bytes(b"\xff\xfe\0\0").is_err());
}

#[test]
fn mesh_blobs_declaring_more_than_they_hold_are_refused() {
    assert!(container::decode_mesh(&mesh(9, 3)).is_ok());
    for (coords, indices) in [
        (u64::MAX, 3),
        (9, u64::MAX),
        (u64::MAX / 8, u64::MAX / 4),
        (1 << 40, 3),
        (12, 3),
        (9, 6),
    ] {
        assert!(
            container::decode_mesh(&mesh(coords, indices)).is_err(),
            "{coords} coordinates, {indices} indices"
        );
    }
    let whole = mesh(9, 3);
    for cut in 0..whole.len() {
        assert!(container::decode_mesh(&whole[..cut]).is_err());
    }
}
