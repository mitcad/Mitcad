// SPDX-License-Identifier: MIT
//! The design container (SolidCraft's `docs/design-container.md`, version
//! 1): a zip with `container.json` (`{"format": "solidcraft-container/1"}`)
//! and `document.json`, the design's JSON with its large payloads moved to
//! `blobs/<sha-256>.<step|mesh|png|jpeg>`: an Import feature's `step`
//! (`step_blob`) and a mesh import's triangles (`mesh_blob`). The blobs are
//! put back into the JSON, so the rest of the reader sees one document.
//! The cached display meshes and evaluated B-rep (`cache/`) are not read:
//! the import rebuilds the design. Canvases' images are not read either.

use mitcad_zip::Archive;
use serde_json::{Value, json};

/// The largest blob expanded: SolidCraft's cap on a mesh blob (format 39).
pub const MAX_BLOB_BYTES: u64 = 512 << 20;

/// The container's JSON document with its blobs put back.
pub fn read(bytes: &[u8]) -> Result<Value, String> {
    let archive = Archive::new(bytes.to_vec()).map_err(|e| format!("design container: {e}"))?;
    let version: Value = serde_json::from_slice(
        &archive
            .read_by_name("container.json")
            .map_err(|e| format!("design container: container.json: {e}"))?,
    )
    .map_err(|e| format!("design container: container.json: {e}"))?;
    let format = version.get("format").and_then(Value::as_str).unwrap_or("");
    if !matches!(format, "solidcraft-container/1" | "solvecraft-container/1") {
        return Err(format!("unsupported design container `{format}`"));
    }
    let mut doc: Value = serde_json::from_slice(
        &archive
            .read_by_name("document.json")
            .map_err(|e| format!("design container: document.json: {e}"))?,
    )
    .map_err(|e| format!("design container: document.json: {e}"))?;
    let blob = |value: &Value, field: &str, extension: &str| -> Result<Vec<u8>, String> {
        let name = value
            .get(field)
            .and_then(Value::as_str)
            .ok_or_else(|| format!("design container: missing `{field}` reference"))?;
        let valid = name
            .strip_prefix("blobs/")
            .and_then(|n| n.strip_suffix(&format!(".{extension}")))
            .is_some_and(|h| h.len() == 64 && h.bytes().all(|b| b.is_ascii_hexdigit()));
        if !valid {
            return Err(format!("design container: invalid blob reference `{name}`"));
        }
        // A mesh blob may expand to 512 MiB (format 39), the file staying
        // within its 256 MiB.
        archive
            .read_by_name_with_limit(name, MAX_BLOB_BYTES)
            .map_err(|e| format!("design container: {name}: {e}"))
    };
    if let Some(Value::Array(features)) = doc.get_mut("features") {
        for f in features.iter_mut() {
            match f.get("type").and_then(Value::as_str) {
                Some("import") if f.get("step_blob").is_some() => {
                    let text = String::from_utf8(blob(f, "step_blob", "step")?)
                        .map_err(|_| "design container: a STEP blob is not text".to_owned())?;
                    if let Some(o) = f.as_object_mut() {
                        o.remove("step_blob");
                        o.insert("step".into(), Value::String(text));
                    }
                }
                Some("mesh_import") => {
                    if let Some(Value::Array(meshes)) = f.get_mut("meshes") {
                        for m in meshes.iter_mut() {
                            if m.get("mesh_blob").is_none() {
                                continue;
                            }
                            let (positions, triangles) =
                                decode_mesh(&blob(m, "mesh_blob", "mesh")?)?;
                            if let Some(o) = m.as_object_mut() {
                                o.remove("mesh_blob");
                                o.insert("positions".into(), json!(positions));
                                o.insert("triangles".into(), json!(triangles));
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
    Ok(doc)
}

/// A mesh blob: `SCMESH01`, the counts of coordinates and indices (u64
/// little-endian), the coordinates (f64) and the indices (u32).
pub fn decode_mesh(bytes: &[u8]) -> Result<(Vec<f64>, Vec<u32>), String> {
    let bad = || "design container: a damaged mesh blob".to_owned();
    if bytes.len() < 24 || &bytes[..8] != b"SCMESH01" {
        return Err(bad());
    }
    let count = |at: usize| -> Result<usize, String> {
        let b: [u8; 8] = bytes[at..at + 8].try_into().map_err(|_| bad())?;
        usize::try_from(u64::from_le_bytes(b)).map_err(|_| bad())
    };
    let (coords, indices) = (count(8)?, count(16)?);
    let size = coords
        .checked_mul(8)
        .and_then(|c| indices.checked_mul(4).and_then(|i| c.checked_add(i)))
        .and_then(|n| n.checked_add(24))
        .ok_or_else(bad)?;
    if size != bytes.len() || coords % 3 != 0 || indices % 3 != 0 {
        return Err(bad());
    }
    let mut positions = Vec::with_capacity(coords);
    for k in 0..coords {
        let at = 24 + 8 * k;
        let b: [u8; 8] = bytes[at..at + 8].try_into().map_err(|_| bad())?;
        let x = f64::from_le_bytes(b);
        if !x.is_finite() {
            return Err(bad());
        }
        positions.push(x);
    }
    let mut triangles = Vec::with_capacity(indices);
    let base = 24 + 8 * coords;
    for k in 0..indices {
        let at = base + 4 * k;
        let b: [u8; 4] = bytes[at..at + 4].try_into().map_err(|_| bad())?;
        let i = u32::from_le_bytes(b);
        if i as usize >= coords / 3 {
            return Err(bad());
        }
        triangles.push(i);
    }
    Ok((positions, triangles))
}
