// SPDX-License-Identifier: MIT
//! The document inside the zip container of an `.f3d` file: what each
//! entry is, the design segments, the body blobs and the documents of an
//! `.f3z` package.

use std::path::Path;

use crate::zip::{Archive, Entry, Method, ZipError};

/// What a zip entry holds, judged by its path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EntryKind {
    Directory,
    /// `Manifest.dat` of the document or of an asset folder.
    Manifest,
    /// `Properties.dat`: u32 length + JSON.
    Properties,
    /// `BulkStream.dat` / `MetaStream.dat` of a segment.
    SegmentStream {
        segment: String,
        meta: bool,
    },
    /// ASM body blob `Breps.BlobParts/BREP.<guid>.smb` (`history` for `.smbh`).
    BodyBlob {
        guid: String,
        history: bool,
    },
    /// Protein material or appearance.
    Material,
    Preview,
    /// `OGS.BlobFolder/OGS/DefaultScene/<name>`: display meshes and the scene.
    DisplayMesh {
        name: String,
    },
    Image,
    DesignConfiguration,
    /// A document inside an `.f3z` package.
    NestedDocument,
    Json,
    Other,
}

/// One entry with its classification.
#[derive(Clone, Debug)]
pub struct EntryInfo {
    pub name: String,
    pub size: u64,
    pub compressed_size: u64,
    pub method: Method,
    /// Asset folder (`FusionAssetName[Active]`, `Animation`), if any.
    pub asset: Option<String>,
    pub kind: EntryKind,
}

/// A segment (folder with `BulkStream.dat` and `MetaStream.dat`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Segment {
    pub asset: String,
    /// Folder name: `Design1` (older files), `FusionDesignSegmentType1`, ...
    pub name: String,
    /// Segment type: the name without the trailing number (`Design`,
    /// `FusionDesignSegmentType`, `FusionBrowserSegmentType`, ...).
    pub kind: String,
    pub bulk: Option<String>,
    pub meta: Option<String>,
}

impl Segment {
    /// The design segment holds the timeline.
    pub fn is_design(&self) -> bool {
        self.kind == "Design" || self.kind == "FusionDesignSegmentType"
    }
}

/// A body blob of the design.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BodyBlob {
    pub entry: String,
    pub asset: String,
    pub guid: String,
    /// `.smbh`: the blob carries ASM history.
    pub history: bool,
}

/// Where a design stream names a blob (`BREP.<guid>.smb` in UTF-16).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlobReference {
    pub blob: String,
    pub stream: String,
    pub offset: usize,
    /// The nearest UTF-16 strings before the reference (newest last),
    /// without GUIDs and blob names. A component's `.smb`/`.smbh` pair
    /// follows the component's name, so the last one is usually the
    /// component (or a feature such as `CreateComponent`); a heuristic
    /// until the design stream is decoded.
    pub context: Vec<String>,
}

/// UTF-16LE runs of printable ASCII (at least two characters) ending before
/// `end`, scanning back at most `window` bytes.
fn utf16_strings_before(data: &[u8], end: usize, window: usize) -> Vec<String> {
    let start = end.saturating_sub(window) & !1;
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut i = start + (end - start) % 2;
    while i + 1 < end {
        let (lo, hi) = (data[i], data[i + 1]);
        if hi == 0 && (0x20..0x7f).contains(&lo) {
            cur.push(lo as char);
        } else if !cur.is_empty() {
            if cur.len() >= 2 {
                out.push(std::mem::take(&mut cur));
            }
            cur.clear();
        }
        i += 2;
    }
    if cur.len() >= 2 {
        out.push(cur);
    }
    out
}

fn is_guid_like(s: &str) -> bool {
    let core = s.trim_end_matches('$');
    core.len() >= 32 && core.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

/// An opened `.f3d` or `.f3z` file.
pub struct F3dFile {
    archive: Archive,
}

fn classify(name: &str) -> (Option<String>, EntryKind) {
    if name.ends_with('/') {
        return (
            name.split('/')
                .next()
                .filter(|_| name.matches('/').count() > 1)
                .map(String::from),
            EntryKind::Directory,
        );
    }
    let parts: Vec<&str> = name.split('/').collect();
    let file = *parts.last().unwrap_or(&"");
    let asset = (parts.len() > 1).then(|| parts[0].to_string());
    let kind = if parts.len() == 1 {
        match file {
            "Manifest.dat" => EntryKind::Manifest,
            "Properties.dat" => EntryKind::Properties,
            f if f.ends_with(".f3d") => EntryKind::NestedDocument,
            f if f.ends_with(".json") => EntryKind::Json,
            _ => EntryKind::Other,
        }
    } else if parts.len() == 2 && file == "Manifest.dat" {
        EntryKind::Manifest
    } else if parts.len() == 3 && (file == "BulkStream.dat" || file == "MetaStream.dat") {
        EntryKind::SegmentStream {
            segment: parts[1].to_string(),
            meta: file == "MetaStream.dat",
        }
    } else if parts.contains(&"Breps.BlobParts")
        && (file.ends_with(".smb") || file.ends_with(".smbh"))
    {
        let history = file.ends_with(".smbh");
        let stem = file.trim_end_matches(".smbh").trim_end_matches(".smb");
        let guid = stem.strip_prefix("BREP.").unwrap_or(stem).to_string();
        EntryKind::BodyBlob { guid, history }
    } else if parts.contains(&"ProteinAssets.BlobParts") {
        EntryKind::Material
    } else if parts.contains(&"Previews") {
        EntryKind::Preview
    } else if parts.contains(&"OGS.BlobFolder") {
        EntryKind::DisplayMesh {
            name: file.to_string(),
        }
    } else if parts.contains(&"Images.BlobParts") {
        EntryKind::Image
    } else if parts.contains(&"DesignConfigurationTable.BlobParts") {
        EntryKind::DesignConfiguration
    } else if file.ends_with(".json") {
        EntryKind::Json
    } else {
        EntryKind::Other
    };
    (asset, kind)
}

fn utf16(s: &str) -> Vec<u8> {
    s.encode_utf16().flat_map(|c| c.to_le_bytes()).collect()
}

fn find_all(hay: &[u8], needle: &[u8]) -> Vec<usize> {
    let mut out = Vec::new();
    if needle.is_empty() || hay.len() < needle.len() {
        return out;
    }
    let first = needle[0];
    let mut i = 0;
    while i + needle.len() <= hay.len() {
        if hay[i] == first && &hay[i..i + needle.len()] == needle {
            out.push(i);
            i += needle.len();
        } else {
            i += 1;
        }
    }
    out
}

impl F3dFile {
    pub fn open(path: &Path) -> Result<F3dFile, ZipError> {
        Ok(F3dFile {
            archive: Archive::open(path)?,
        })
    }

    pub fn from_bytes(data: Vec<u8>) -> Result<F3dFile, ZipError> {
        Ok(F3dFile {
            archive: Archive::new(data)?,
        })
    }

    pub fn archive(&self) -> &Archive {
        &self.archive
    }

    pub fn entries(&self) -> Vec<EntryInfo> {
        self.archive
            .entries()
            .iter()
            .map(|e: &Entry| {
                let (asset, kind) = classify(&e.name);
                EntryInfo {
                    name: e.name.clone(),
                    size: e.size,
                    compressed_size: e.compressed_size,
                    method: e.method,
                    asset,
                    kind,
                }
            })
            .collect()
    }

    pub fn read(&self, name: &str) -> Result<Vec<u8>, ZipError> {
        self.archive.read_by_name(name)
    }

    /// Documents of an `.f3z` package (empty for an `.f3d`).
    pub fn documents(&self) -> Vec<String> {
        self.entries()
            .into_iter()
            .filter(|e| e.kind == EntryKind::NestedDocument)
            .map(|e| e.name)
            .collect()
    }

    /// The root document named by `Manifest.json` of an `.f3z` package.
    pub fn root_document(&self) -> Option<String> {
        let json = String::from_utf8(self.read("Manifest.json").ok()?).ok()?;
        let key = "\"root\"";
        let rest = &json[json.find(key)? + key.len()..];
        let start = rest.find('"')? + 1;
        let end = start + rest[start..].find('"')?;
        Some(rest[start..end].to_string())
    }

    pub fn open_document(&self, name: &str) -> Result<F3dFile, ZipError> {
        F3dFile::from_bytes(self.read(name)?)
    }

    /// The JSON of `Properties.dat` (after its u32 length), if present.
    pub fn properties(&self) -> Option<String> {
        let data = self.read("Properties.dat").ok()?;
        if data.len() <= 4 {
            return None;
        }
        let n = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
        let json = data.get(4..4 + n.min(data.len() - 4))?;
        String::from_utf8(json.to_vec())
            .ok()
            .filter(|s| !s.is_empty())
    }

    pub fn segments(&self) -> Vec<Segment> {
        let mut out: Vec<Segment> = Vec::new();
        for e in self.entries() {
            if let EntryKind::SegmentStream { segment, meta } = &e.kind {
                let asset = e.asset.clone().unwrap_or_default();
                let idx = match out
                    .iter()
                    .position(|s| s.asset == asset && &s.name == segment)
                {
                    Some(i) => i,
                    None => {
                        let kind = segment
                            .trim_end_matches(|c: char| c.is_ascii_digit())
                            .to_string();
                        out.push(Segment {
                            asset,
                            name: segment.clone(),
                            kind,
                            bulk: None,
                            meta: None,
                        });
                        out.len() - 1
                    }
                };
                if *meta {
                    out[idx].meta = Some(e.name.clone());
                } else {
                    out[idx].bulk = Some(e.name.clone());
                }
            }
        }
        out
    }

    pub fn body_blobs(&self) -> Vec<BodyBlob> {
        self.entries()
            .into_iter()
            .filter_map(|e| match e.kind {
                EntryKind::BodyBlob { guid, history } => Some(BodyBlob {
                    entry: e.name,
                    asset: e.asset.unwrap_or_default(),
                    guid,
                    history,
                }),
                _ => None,
            })
            .collect()
    }

    /// Places where the design segment's streams name each body blob.
    pub fn blob_references(&self) -> Vec<BlobReference> {
        let blobs = self.body_blobs();
        let mut out = Vec::new();
        for seg in self.segments().iter().filter(|s| s.is_design()) {
            for stream in [&seg.bulk, &seg.meta].into_iter().flatten() {
                let Ok(data) = self.read(stream) else {
                    continue;
                };
                for b in &blobs {
                    let file = b.entry.rsplit('/').next().unwrap_or(&b.entry);
                    // Match the name followed by a non-letter, so that ".smb"
                    // does not also match ".smbh".
                    let needle = utf16(file);
                    for off in find_all(&data, &needle) {
                        let after = data.get(off + needle.len()..off + needle.len() + 2);
                        if after == Some(&[b'h', 0][..]) {
                            continue;
                        }
                        let mut context: Vec<String> = utf16_strings_before(&data, off, 4096)
                            .into_iter()
                            .filter(|s| !is_guid_like(s) && !s.starts_with("BREP."))
                            .collect();
                        let keep = context.len().saturating_sub(3);
                        context.drain(..keep);
                        out.push(BlobReference {
                            blob: b.entry.clone(),
                            stream: stream.clone(),
                            offset: off,
                            context,
                        });
                    }
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_entries() {
        let (a, k) = classify("FusionAssetName[Active]/Breps.BlobParts/BREP.1dc4-x.smbh");
        assert_eq!(a.as_deref(), Some("FusionAssetName[Active]"));
        assert_eq!(
            k,
            EntryKind::BodyBlob {
                guid: "1dc4-x".into(),
                history: true
            }
        );
        let (_, k) = classify("FusionAssetName[Active]/FusionDesignSegmentType1/MetaStream.dat");
        assert_eq!(
            k,
            EntryKind::SegmentStream {
                segment: "FusionDesignSegmentType1".into(),
                meta: true
            }
        );
        assert_eq!(classify("Manifest.dat").1, EntryKind::Manifest);
        assert_eq!(classify("0c99.f3d").1, EntryKind::NestedDocument);
        assert_eq!(
            classify("FusionAssetName[Active]/OGS.BlobFolder/OGS/DefaultScene/world").1,
            EntryKind::DisplayMesh {
                name: "world".into()
            }
        );
    }

    #[test]
    fn finds_segments_blobs_and_references() {
        let mut bulk = vec![0u8; 4];
        bulk.extend(utf16("Bracket"));
        bulk.extend([0u8; 4]);
        let first = bulk.len();
        bulk.extend(utf16("BREP.abc.smb"));
        bulk.extend(utf16("BREP.abc.smbh"));
        let zip = crate::zip::stored_zip(&[
            ("Properties.dat", &[2, 0, 0, 0, b'{', b'}']),
            ("A/Design1/BulkStream.dat", &bulk),
            ("A/Design1/MetaStream.dat", b"Design"),
            ("A/Breps.BlobParts/BREP.abc.smb", b"x"),
            ("A/Breps.BlobParts/BREP.abc.smbh", b"y"),
        ]);
        let f = F3dFile::from_bytes(zip).unwrap();
        assert_eq!(f.properties().as_deref(), Some("{}"));
        let segs = f.segments();
        assert_eq!(segs.len(), 1);
        assert!(segs[0].is_design());
        assert_eq!(segs[0].kind, "Design");
        assert_eq!(f.body_blobs().len(), 2);
        let refs = f.blob_references();
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0].offset, first);
        assert_eq!(refs[0].context, vec!["Bracket".to_string()]);
        assert!(refs[1].blob.ends_with(".smbh"));
        assert_eq!(refs[1].context, vec!["Bracket".to_string()]);
    }
}
