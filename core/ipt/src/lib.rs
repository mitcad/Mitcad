// SPDX-License-Identifier: MIT
//! Reader for `.ipt` part files: the bodies stored in the file and its
//! document properties. See `README.md` for the format as far as it is
//! implemented.
//!
//! Layers, bottom up:
//! - [`cfb`]: the compound file container (Microsoft's published MS-CFB
//!   specification), with a writer for tests.
//! - [`props`]: property set streams (MS-OLEPS): part number, material,
//!   the saving release, model units.
//! - [`rse`]: the segment database: segments of meta and data streams,
//!   their record tables and records.
//! - The B-rep record of the B-rep segment holds an ASM binary file, which
//!   the `.f3d` reader's ASM code parses and converts (`mitcad_f3d::asm`,
//!   `mitcad_f3d::convert`).
//!
//! The format knowledge comes from Microsoft's specifications for the
//! container and the property sets, and from inspecting files for the rest.

pub mod cfb;
pub mod dc;
pub mod design;
pub mod features;
pub mod params;
pub mod profile;
pub mod props;
pub mod rse;
pub mod sketch;
#[doc(hidden)]
pub mod testdata;
#[doc(hidden)]
pub mod testdesign;

use std::fmt;

use cfb::{CompoundFile, guid_text};
use mitcad_f3d::convert;

/// The class id of a part file's root storage.
pub const PART_CLSID: &str = "4d29b490-49b2-11d0-93c3-7e0706000000";

/// The record type of the B-rep record (the bodies as an ASM binary file
/// after a 14-byte head), as stored.
pub const BREP_RECORD_TYPE: [u8; 16] = [
    0x5C, 0x59, 0x45, 0xF6, 0xD5, 0x11, 0x33, 0x13, 0x10, 0x00, 0x60, 0xA6, 0xBB, 0xA6, 0x47, 0xB5,
];

/// Bytes before the ASM data in a B-rep record.
pub const BREP_RECORD_HEAD: usize = 14;

/// The property set with the part's tracking properties (part number,
/// material, the release that saved the file).
pub const TRACKING_PROPERTIES: &str = "32853f0f-3444-11d1-9e93-0060b03c1ca6";
/// Its property ids.
pub const PID_PART_NUMBER: u32 = 5;
pub const PID_MATERIAL: u32 = 20;
pub const PID_RELEASE: u32 = 67;

/// The property set with the model's settings; property 8 is the length
/// unit (see [`length_unit`]).
pub const MODEL_SETTINGS: &str = "bb586990-af3e-11d3-95a9-00a0c9b6e37a";
pub const PID_LENGTH_UNIT: u32 = 8;

/// The ASM magic of the B-rep record's data.
const ASM_MAGIC: &[u8] = b"ASM BinaryFile";

#[derive(Clone, Debug, PartialEq)]
pub enum IptError {
    Container(cfb::CfbError),
    /// A compound file without the segment database.
    NotPart(String),
    Segment(String),
}

impl fmt::Display for IptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IptError::Container(e) => write!(f, "{e}"),
            IptError::NotPart(e) => write!(f, "not a part file: {e}"),
            IptError::Segment(e) => f.write_str(e),
        }
    }
}

impl std::error::Error for IptError {}

impl From<cfb::CfbError> for IptError {
    fn from(e: cfb::CfbError) -> Self {
        IptError::Container(e)
    }
}

/// The length unit of a unit code of the model settings, as Mitcad's unit
/// symbol. Millimetres (11269) and inches (11272) are confirmed by files;
/// the others follow the same enumeration.
pub fn length_unit(code: i64) -> Option<&'static str> {
    Some(match code {
        11268 => "cm",
        11269 => "mm",
        11270 => "m",
        11271 => "um",
        11272 => "in",
        11273 => "ft",
        11274 => "yd",
        11275 => "mi",
        _ => return None,
    })
}

/// A segment of the database: its meta and data streams.
#[derive(Clone, Debug)]
pub struct Segment {
    /// The segment's name from its meta stream (`PmBRepSegment`).
    pub name: String,
    pub id: [u8; 16],
    /// The stream names' common part (`M<stream>`, `B<stream>`).
    pub stream: String,
    pub meta: rse::MetaHeader,
}

/// A segment's records, decompressed.
pub struct SegmentData {
    pub tables: rse::Tables,
    pub data: Vec<u8>,
    pub compression: rse::Compression,
    /// The records, or why they could not be split.
    pub records: Result<Vec<rse::Record>, String>,
}

/// The ASM data of a B-rep record.
#[derive(Clone, Debug)]
pub struct BrepRecord {
    pub segment: String,
    /// The record's position in its segment (None when it was found by
    /// scanning the data because the records could not be split).
    pub record: Option<usize>,
    /// The ASM file: the record from its 14th byte (the parser stops at
    /// its end marker, before the record's trailer).
    pub asm: Vec<u8>,
}

impl BrepRecord {
    /// Where the bodies come from, for reports: `PmBRepSegment#107`.
    pub fn place(&self) -> String {
        match self.record {
            Some(r) => format!("{}#{r}", self.segment),
            None => format!("{}@scan", self.segment),
        }
    }
}

/// The document properties the import uses.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DocumentInfo {
    pub part_number: Option<String>,
    pub material: Option<String>,
    /// The release that saved the file, as the file states it.
    pub release: Option<String>,
    /// The model's length unit code and its symbol when known.
    pub length_unit_code: Option<i64>,
    pub length_unit: Option<&'static str>,
}

/// An opened `.ipt` file.
pub struct IptFile {
    pub container: CompoundFile,
    pub segments: Vec<Segment>,
    /// Every property set stream at the root: its name and sets.
    pub properties: Vec<(String, Vec<props::PropertySet>)>,
    /// Streams that could not be read as what their name says, with why.
    pub warnings: Vec<String>,
}

impl IptFile {
    pub fn open(path: &std::path::Path) -> Result<IptFile, IptError> {
        let data = std::fs::read(path).map_err(|e| {
            IptError::Container(cfb::CfbError::Io(format!("{}: {e}", path.display())))
        })?;
        IptFile::parse(data)
    }

    pub fn parse(data: Vec<u8>) -> Result<IptFile, IptError> {
        let container = CompoundFile::parse(data)?;
        if container.find("/RSeStorage").is_none() {
            return Err(IptError::NotPart("no segment database (RSeStorage)".into()));
        }
        let mut warnings = Vec::new();
        let mut segments = Vec::new();
        let metas: Vec<(String, String)> = container
            .children("/RSeStorage")
            .filter_map(|(path, _)| {
                let name = path.rsplit('/').next()?;
                let stream = name.strip_prefix('M')?;
                Some((path.to_string(), stream.to_string()))
            })
            .collect();
        for (path, stream) in metas {
            if container.find(&format!("/RSeStorage/B{stream}")).is_none() {
                continue;
            }
            let meta = container.read_path(&path)?;
            match rse::meta_header(&meta) {
                Ok(header) => segments.push(Segment {
                    name: header.name.clone(),
                    id: header.id,
                    stream,
                    meta: header,
                }),
                Err(e) => warnings.push(format!("{path}: {e}")),
            }
        }
        segments.sort_by(|a, b| a.name.cmp(&b.name).then(a.stream.cmp(&b.stream)));
        let mut properties = Vec::new();
        let streams: Vec<String> = container
            .children("")
            .filter(|(p, e)| p.starts_with("/\u{5}") && e.kind == cfb::EntryKind::Stream)
            .map(|(p, _)| p.to_string())
            .collect();
        for path in streams {
            let data = container.read_path(&path)?;
            match props::parse(&data) {
                Ok(sets) => properties.push((path[2..].to_string(), sets)),
                Err(e) => warnings.push(format!("{}: {e}", &path[2..])),
            }
        }
        Ok(IptFile {
            container,
            segments,
            properties,
            warnings,
        })
    }

    /// The root storage's class id is a part's.
    pub fn is_part(&self) -> bool {
        guid_text(&self.container.root().clsid) == PART_CLSID
    }

    /// The property set with this format id, if any.
    pub fn property_set(&self, fmtid: &str) -> Option<&props::PropertySet> {
        self.properties
            .iter()
            .flat_map(|(_, sets)| sets)
            .find(|s| guid_text(&s.fmtid) == fmtid)
    }

    pub fn document(&self) -> DocumentInfo {
        let tracking = self.property_set(TRACKING_PROPERTIES);
        let text = |id| {
            tracking
                .and_then(|s| s.text(id))
                .map(str::trim)
                .filter(|t| !t.is_empty())
                .map(str::to_string)
        };
        let code = self
            .property_set(MODEL_SETTINGS)
            .and_then(|s| s.int(PID_LENGTH_UNIT));
        DocumentInfo {
            part_number: text(PID_PART_NUMBER),
            material: text(PID_MATERIAL),
            release: text(PID_RELEASE),
            length_unit_code: code,
            length_unit: code.and_then(length_unit),
        }
    }

    /// A segment's records. Tables that cannot be read leave the tables
    /// empty and the records with the error; the data is still there.
    pub fn segment_data(&self, segment: &Segment) -> Result<SegmentData, IptError> {
        let what = |e: rse::RseError| IptError::Segment(format!("{}: {e}", segment.name));
        let meta = self
            .container
            .read_path(&format!("/RSeStorage/M{}", segment.stream))?;
        let raw = rse::decompress(&meta[segment.meta.data_offset..], segment.meta.compression)
            .map_err(what)?;
        let stream = self
            .container
            .read_path(&format!("/RSeStorage/B{}", segment.stream))?;
        let (start, compression) = rse::data_start(&stream).map_err(what)?;
        let data = rse::decompress(&stream[start..], compression).map_err(what)?;
        let (tables, records) = match rse::tables(&raw) {
            Ok(tables) => {
                let records = rse::records(&data, &tables).map_err(|e| e.to_string());
                (tables, records)
            }
            Err(e) => (rse::Tables::default(), Err(format!("tables: {e}"))),
        };
        Ok(SegmentData {
            tables,
            data,
            compression,
            records,
        })
    }

    /// The definitions segment (`PmDCSegment`): parameters, sketches and
    /// features; None when the file has none.
    pub fn definitions(&self) -> Result<Option<dc::Definitions>, IptError> {
        let Some(segment) = self.segments.iter().find(|s| s.name == "PmDCSegment") else {
            return Ok(None);
        };
        let content = self.segment_data(segment)?;
        let records = content
            .records
            .map_err(|e| IptError::Segment(format!("{}: {e}", segment.name)))?;
        Ok(Some(dc::Definitions::new(
            segment.meta.major(),
            content.data,
            &content.tables,
            &records,
        )))
    }

    /// The B-rep records of the B-rep segments (`PmBRepSegment`): records
    /// of [`BREP_RECORD_TYPE`] whose data from byte 14 is an ASM binary
    /// file. When a segment's records cannot be split, its data is scanned
    /// for the ASM magic instead (the record is then None).
    pub fn brep_records(&self) -> Result<Vec<BrepRecord>, IptError> {
        let mut out = Vec::new();
        let mut problems = Vec::new();
        for segment in self
            .segments
            .iter()
            .filter(|s| s.name.ends_with("BRepSegment"))
        {
            let content = self.segment_data(segment)?;
            let data = &content.data;
            match &content.records {
                Ok(records) => {
                    for r in records {
                        if content.tables.types[r.type_index()].id != BREP_RECORD_TYPE {
                            continue;
                        }
                        match data[r.range.clone()].get(BREP_RECORD_HEAD..) {
                            Some(asm) if asm.starts_with(ASM_MAGIC) => out.push(BrepRecord {
                                segment: segment.name.clone(),
                                record: Some(r.index),
                                asm: asm.to_vec(),
                            }),
                            _ => problems.push(format!(
                                "{}#{}: a B-rep record without an ASM binary file (an older format?)",
                                segment.name, r.index
                            )),
                        }
                    }
                }
                Err(e) => {
                    let mut at = 0;
                    let mut found = false;
                    while let Some(p) = find(&data[at..], ASM_MAGIC) {
                        out.push(BrepRecord {
                            segment: segment.name.clone(),
                            record: None,
                            asm: data[at + p..].to_vec(),
                        });
                        found = true;
                        at += p + ASM_MAGIC.len();
                    }
                    if !found {
                        problems.push(format!("{}: {e}", segment.name));
                    }
                }
            }
        }
        if out.is_empty() {
            return Err(IptError::Segment(if problems.is_empty() {
                "no B-rep segment with bodies".into()
            } else {
                problems.join("; ")
            }));
        }
        Ok(out)
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// Options of the ASM conversion for `.ipt` bodies: model units are
/// centimetres, as in `.f3d` files.
pub fn convert_options() -> convert::Options {
    convert::Options::default()
}

/// Parses and converts the bodies of each B-rep record.
pub fn read_bodies(records: &[BrepRecord]) -> Vec<Result<mitcad_f3d::BlobBodies, String>> {
    let options = convert_options();
    records
        .iter()
        .map(|r| {
            mitcad_f3d::read_blob(&r.place(), &r.asm, &options)
                .map_err(|e| format!("{}: {e}", r.place()))
        })
        .collect()
}

/// The number of states of the ASM history the B-rep record carries (the
/// states kept for rolling the part back), when it has one.
pub fn history_states(asm: &[u8]) -> Option<usize> {
    let file = mitcad_f3d::asm::AsmFile::parse(asm).ok()?;
    mitcad_f3d::asm::history::History::parse(&file)
        .ok()
        .flatten()
        .map(|h| h.states.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_test_part() {
        let f = IptFile::parse(testdata::test_part()).unwrap();
        assert!(f.is_part());
        assert!(f.warnings.is_empty(), "{:?}", f.warnings);
        let names: Vec<&str> = f.segments.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["PmBRepSegment", "PmDCSegment"]);
        assert_eq!(
            f.document(),
            DocumentInfo {
                part_number: Some("MITCAD-TEST-1".into()),
                material: Some("Aluminum".into()),
                release: Some("Mitcad test writer".into()),
                length_unit_code: Some(11272),
                length_unit: Some("in"),
            }
        );
        let records = f.brep_records().unwrap();
        let places: Vec<String> = records.iter().map(BrepRecord::place).collect();
        assert_eq!(places, ["PmBRepSegment#1", "PmBRepSegment#2"]);
        let bodies = read_bodies(&records);
        assert_eq!(bodies.len(), 2);
        for blob in bodies {
            let blob = blob.unwrap();
            assert_eq!(blob.bodies.len(), 1);
            assert!(blob.truncated.is_none());
            assert!(blob.bodies[0].issues.is_empty());
            assert!(blob.bodies[0].check.is_clean());
        }
        // The cube is 1 cm: 10 mm after conversion.
        let converted = read_bodies(&records[..1]).remove(0).unwrap();
        let cube = &converted.bodies[0].body;
        assert!(
            cube.vertices
                .iter()
                .any(|v| (v.point[0] - 10.0).abs() < 1e-9)
        );
        assert_eq!(history_states(&records[0].asm), None);
    }

    #[test]
    fn scans_a_segment_whose_records_do_not_split() {
        // The B-rep segment's block table says 1 byte for the first record:
        // the records cannot be split, so the data is scanned for ASM files.
        let asm = mitcad_f3d::testdata::cube_blob();
        let types = [BREP_RECORD_TYPE];
        let (m, b) = rse::write_segment(
            "PmBRepSegment",
            [1; 16],
            &types,
            &[(0, testdata::brep_record(&asm))],
        );
        let header = rse::meta_header(&m).unwrap();
        let raw = rse::decompress(&m[header.data_offset..], header.compression).unwrap();
        let mut tables = rse::tables(&raw).unwrap();
        tables.blocks[0] = 0x8000_0001;
        // Rewrite the meta stream with the damaged table.
        let mut t = Vec::new();
        t.extend_from_slice(&raw[..18]);
        t.extend_from_slice(&tables.blocks[0].to_le_bytes());
        t.extend_from_slice(&raw[22..]);
        let mut damaged = m[..header.data_offset].to_vec();
        damaged.extend_from_slice(&miniz_oxide::deflate::compress_to_vec_zlib(&t, 6));
        let mut w = cfb::Writer::new();
        w.stream("RSeStorage/Mx", &damaged)
            .stream("RSeStorage/Bx", &b);
        let f = IptFile::parse(w.finish()).unwrap();
        assert!(!f.is_part());
        let records = f.brep_records().unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].record, None);
        assert_eq!(records[0].place(), "PmBRepSegment@scan");
        let blob = read_bodies(&records).remove(0).unwrap();
        assert_eq!(blob.bodies.len(), 1);
        assert!(blob.bodies[0].check.is_clean());
    }

    #[test]
    fn refuses_files_without_bodies() {
        let mut w = cfb::Writer::new();
        w.stream("Other", b"data");
        assert!(matches!(
            IptFile::parse(w.finish()),
            Err(IptError::NotPart(_))
        ));
        // A B-rep record without an ASM file.
        let types = [BREP_RECORD_TYPE];
        let (m, b) = rse::write_segment("PmBRepSegment", [1; 16], &types, &[(0, vec![0; 40])]);
        let mut w = cfb::Writer::new();
        w.stream("RSeStorage/Mx", &m).stream("RSeStorage/Bx", &b);
        let f = IptFile::parse(w.finish()).unwrap();
        let e = f.brep_records().unwrap_err().to_string();
        assert!(e.contains("without an ASM binary file"), "{e}");
        assert_eq!(
            IptFile::parse(vec![0; 10]).err(),
            Some(IptError::Container(cfb::CfbError::NotCompound))
        );
    }

    #[test]
    fn maps_length_units() {
        assert_eq!(length_unit(11269), Some("mm"));
        assert_eq!(length_unit(11272), Some("in"));
        assert_eq!(length_unit(1), None);
    }
}
