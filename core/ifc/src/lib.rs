// SPDX-License-Identifier: MIT
//! IFC files (Industry Foundation Classes, ISO 16739-1) for the IFC import
//! (mitcad#273): the ISO 10303-21 text of IFC2X3, IFC4 and IFC4X3, also
//! zipped (`.ifczip`). No geometry is built here: the import builds it
//! through the model's kernel from what this crate decodes.
//!
//! - [`step`]: the records of the text.
//! - [`IfcFile`]: the header and the entity instances, read a record at a
//!   time; [`ReadOptions::keep`] leaves out the attributes of entities the
//!   import does not use (their types stay known).
//! - [`schema`]: the hand-written subset of the schemas this reader knows:
//!   the entities' kinds and the attributes it reads, for the three
//!   schemas, and a generic fallback for products of other types.
//! - [`units`]: the project's units, as millimetres and radians.
//! - [`model`]: the spatial structure, the products (elements, openings,
//!   grids, ...) with their placements and relationships, materials,
//!   styles and quantities.
//! - [`geometry`]: the representation items of a product's shape decoded
//!   into [`geometry::Item`]s (profiles, swept solids, booleans, breps, face
//!   sets, mapped items), in millimetres.
//!
//! The entity subset was written by hand from the specification's text
//! (buildingSMART's documentation of the three schemas); no schema file
//! or text generated from one is used. Other readers of the format were
//! neither copied nor linked.

pub mod geometry;
pub mod math;
pub mod model;
// Property sets and element quantities (mitcad#496).
pub mod properties;
pub mod schema;
pub mod step;
pub mod units;

use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::io::BufRead;
use std::path::Path;
use std::sync::Arc;

pub use step::Value;

/// An entity instance.
#[derive(Debug, Clone, PartialEq)]
pub struct Entity {
    pub id: u64,
    /// The type in upper case (`IFCWALL`); a complex instance's first
    /// partial entity.
    pub kind: Arc<str>,
    /// Its attributes in the schema's order (a complex instance's partial
    /// entities' attributes after each other). Empty when the reader left
    /// them out ([`ReadOptions::keep`]).
    pub args: Vec<Value>,
    /// A complex instance's partial entities, in the file's order.
    pub parts: Vec<Arc<str>>,
}

impl Entity {
    /// Attribute `i`, or `$` when there is none.
    pub fn arg(&self, i: usize) -> &Value {
        self.args.get(i).unwrap_or(&Value::Null)
    }

    pub fn is(&self, kind: &str) -> bool {
        &*self.kind == kind || self.parts.iter().any(|p| &**p == kind)
    }
}

/// The header section.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct Header {
    /// FILE_DESCRIPTION: the descriptions (the view definition is one) and
    /// the implementation level.
    pub description: Vec<String>,
    pub implementation_level: String,
    /// FILE_NAME.
    pub name: String,
    pub time_stamp: String,
    pub author: Vec<String>,
    pub organization: Vec<String>,
    pub preprocessor_version: String,
    pub originating_system: String,
    pub authorization: String,
    /// FILE_SCHEMA.
    pub schemas: Vec<String>,
}

impl Header {
    /// The model view definition named in the description
    /// (`ViewDefinition [ReferenceView_V1.2]`), if any.
    pub fn view_definition(&self) -> Option<String> {
        self.description.iter().find_map(|d| {
            let rest = d.trim().strip_prefix("ViewDefinition")?;
            let start = rest.find('[')?;
            let end = rest.rfind(']')?;
            (end > start).then(|| rest[start + 1..end].trim().to_owned())
        })
    }
}

/// What the reader keeps.
#[derive(Clone)]
pub struct ReadOptions {
    /// Whether to keep an entity type's attributes (upper case name);
    /// entities left out keep their type only.
    pub keep: fn(&str) -> bool,
}

impl ReadOptions {
    /// Everything.
    pub fn all() -> Self {
        Self { keep: |_| true }
    }

    /// What the import uses: everything but property sets and their values
    /// and the space boundaries' connection geometry (most of a large
    /// file's text besides the geometry).
    pub fn import() -> Self {
        Self {
            keep: schema::used_by_import,
        }
    }
}

impl Default for ReadOptions {
    fn default() -> Self {
        Self::all()
    }
}

/// Why a file cannot be read.
#[derive(Debug)]
pub enum IfcError {
    Io(std::io::Error),
    Zip(String),
    Step(step::StepError),
    Format(String),
}

impl fmt::Display for IfcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IfcError::Io(e) => write!(f, "{e}"),
            IfcError::Zip(e) => write!(f, "the .ifczip archive: {e}"),
            IfcError::Step(e) => write!(f, "{e}"),
            IfcError::Format(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for IfcError {}

impl From<std::io::Error> for IfcError {
    fn from(e: std::io::Error) -> Self {
        IfcError::Io(e)
    }
}

/// The schema of a file, from its FILE_SCHEMA.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum Schema {
    Ifc2x3,
    Ifc4,
    Ifc4x3,
    /// Another name (the reader goes on as with IFC4).
    Other,
}

impl Schema {
    pub fn of(name: &str) -> Self {
        let name = name.trim().to_ascii_uppercase();
        if name.starts_with("IFC2X3") {
            Schema::Ifc2x3
        } else if name.starts_with("IFC4X3") {
            Schema::Ifc4x3
        } else if name.starts_with("IFC4") {
            Schema::Ifc4
        } else {
            Schema::Other
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Schema::Ifc2x3 => "IFC2X3",
            Schema::Ifc4 => "IFC4",
            Schema::Ifc4x3 => "IFC4X3",
            Schema::Other => "other",
        }
    }
}

/// An IFC file read into memory: its header and entity instances.
#[derive(Debug, Clone)]
pub struct IfcFile {
    pub header: Header,
    /// The schema FILE_SCHEMA names (the first).
    pub schema: Schema,
    pub entities: HashMap<u64, Entity>,
    /// Instances per type.
    pub counts: BTreeMap<Arc<str>, usize>,
    /// Instances whose attributes were left out ([`ReadOptions::keep`]).
    pub left_out: usize,
    /// What the reader passed over: records it could not read, instance
    /// numbers given twice.
    pub warnings: Vec<String>,
    /// The bytes of IFC text read.
    pub bytes: u64,
}

/// The most warnings kept about records that cannot be read.
const MAX_WARNINGS: usize = 100;

impl IfcFile {
    /// Reads an `.ifc` file, or the `.ifc` file inside an `.ifczip`
    /// archive (by the file's content: a zip archive starts with `PK`).
    pub fn open(path: &Path) -> Result<Self, IfcError> {
        Self::open_with(path, &ReadOptions::all())
    }

    pub fn open_with(path: &Path, options: &ReadOptions) -> Result<Self, IfcError> {
        let mut file = std::fs::File::open(path)?;
        let mut magic = [0u8; 4];
        let n = std::io::Read::read(&mut file, &mut magic)?;
        if n == 4 && &magic == b"PK\x03\x04" {
            let archive =
                mitcad_zip::Archive::open(path).map_err(|e| IfcError::Zip(e.to_string()))?;
            let entries: Vec<_> = archive
                .entries()
                .iter()
                .filter(|e| !e.is_dir() && e.name.to_ascii_lowercase().ends_with(".ifc"))
                .collect();
            let entry = match entries.as_slice() {
                [one] => *one,
                [] => {
                    return Err(IfcError::Zip("it holds no .ifc file".to_owned()));
                }
                [first, ..] => *first,
            };
            // An IFC model may be large: the explicit opt-in to the zip
            // reader's large entry limit (mitcad#923).
            let data = archive
                .read_with_limit(entry, mitcad_zip::LARGE_ENTRY_LIMIT)
                .map_err(|e| IfcError::Zip(e.to_string()))?;
            let mut file = Self::read(std::io::Cursor::new(data), options)?;
            if entries.len() > 1 {
                file.warnings.push(format!(
                    "the archive holds {} .ifc files: {} was read",
                    entries.len(),
                    entry.name
                ));
            }
            return Ok(file);
        }
        let file = std::fs::File::open(path)?;
        Self::read(std::io::BufReader::with_capacity(1 << 16, file), options)
    }

    /// Reads IFC text.
    pub fn parse(text: &str) -> Result<Self, IfcError> {
        Self::read(text.as_bytes(), &ReadOptions::all())
    }

    /// Reads IFC text from a stream, a record at a time.
    pub fn read(reader: impl BufRead, options: &ReadOptions) -> Result<Self, IfcError> {
        let mut records = step::Records::new(reader);
        let mut file = IfcFile {
            header: Header::default(),
            schema: Schema::Other,
            entities: HashMap::new(),
            counts: BTreeMap::new(),
            left_out: 0,
            warnings: Vec::new(),
            bytes: 0,
        };
        let mut names: HashMap<String, Arc<str>> = HashMap::new();
        let mut intern = |name: String| -> Arc<str> {
            names
                .entry(name)
                .or_insert_with_key(|n| Arc::from(n.as_str()))
                .clone()
        };
        let mut started = false;
        let mut bad = 0usize;
        for record in records.by_ref() {
            let (offset, text) = match record {
                Ok(r) => r,
                Err(e) => {
                    if !started {
                        return Err(IfcError::Step(e));
                    }
                    file.warnings.push(format!("{e}"));
                    break;
                }
            };
            let record = match step::parse_record(offset, &text) {
                Ok(r) => r,
                Err(e) => {
                    bad += 1;
                    if bad <= MAX_WARNINGS {
                        file.warnings.push(format!("record not read: {e}"));
                    }
                    continue;
                }
            };
            match record {
                step::Record::Keyword(k) => {
                    if k == "ISO-10303-21" {
                        started = true;
                    } else if !started {
                        return Err(IfcError::Format(
                            "not an ISO 10303-21 file (it does not start with ISO-10303-21;)"
                                .to_owned(),
                        ));
                    }
                }
                step::Record::Header(name, args) => {
                    if !started {
                        return Err(IfcError::Format(
                            "not an ISO 10303-21 file (it does not start with ISO-10303-21;)"
                                .to_owned(),
                        ));
                    }
                    file.header_entity(&name, &args);
                }
                step::Record::Instance(id, name, args) => {
                    let kind = intern(name);
                    file.add(id, kind, Vec::new(), args, options);
                }
                step::Record::Complex(id, parts) => {
                    let names: Vec<Arc<str>> =
                        parts.iter().map(|(n, _)| intern(n.clone())).collect();
                    let args: Vec<Value> = parts.into_iter().flat_map(|(_, a)| a).collect();
                    let kind = names
                        .iter()
                        .find(|n| n.starts_with("IFC") && !n.ends_with("MEASUREWITHUNIT"))
                        .or(names.first())
                        .cloned()
                        .unwrap_or_else(|| intern("UNKNOWN".to_owned()));
                    file.add(id, kind, names, args, options);
                }
            }
        }
        if !started {
            return Err(IfcError::Format("the file is empty".to_owned()));
        }
        if bad > MAX_WARNINGS {
            file.warnings
                .push(format!("{} more records not read", bad - MAX_WARNINGS));
        }
        file.bytes = records.offset();
        file.schema = file
            .header
            .schemas
            .first()
            .map_or(Schema::Other, |s| Schema::of(s));
        Ok(file)
    }

    fn add(
        &mut self,
        id: u64,
        kind: Arc<str>,
        parts: Vec<Arc<str>>,
        args: Vec<Value>,
        options: &ReadOptions,
    ) {
        *self.counts.entry(kind.clone()).or_default() += 1;
        let args = if (options.keep)(&kind) {
            args
        } else {
            self.left_out += 1;
            Vec::new()
        };
        if self
            .entities
            .insert(
                id,
                Entity {
                    id,
                    kind,
                    args,
                    parts,
                },
            )
            .is_some()
            && self.warnings.len() < MAX_WARNINGS
        {
            self.warnings
                .push(format!("#{id} is given twice: the last one is kept"));
        }
    }

    fn header_entity(&mut self, name: &str, args: &[Value]) {
        let text = |v: &Value| v.as_str().unwrap_or_default().to_owned();
        let texts = |v: &Value| -> Vec<String> {
            match v {
                Value::List(l) => l.iter().map(text).collect(),
                other => other
                    .as_str()
                    .map(|s| vec![s.to_owned()])
                    .unwrap_or_default(),
            }
        };
        let arg = |i: usize| args.get(i).unwrap_or(&Value::Null);
        let h = &mut self.header;
        match name {
            "FILE_DESCRIPTION" => {
                h.description = texts(arg(0));
                h.implementation_level = text(arg(1));
            }
            "FILE_NAME" => {
                h.name = text(arg(0));
                h.time_stamp = text(arg(1));
                h.author = texts(arg(2));
                h.organization = texts(arg(3));
                h.preprocessor_version = text(arg(4));
                h.originating_system = text(arg(5));
                h.authorization = text(arg(6));
            }
            "FILE_SCHEMA" => h.schemas = texts(arg(0)),
            _ => {}
        }
    }

    pub fn get(&self, id: u64) -> Option<&Entity> {
        self.entities.get(&id)
    }

    /// The type of instance `id`, even when its attributes were left out.
    pub fn kind(&self, id: u64) -> Option<&str> {
        self.entities.get(&id).map(|e| &*e.kind)
    }

    /// The instances of a type (upper case), in the order of their numbers.
    pub fn of_kind(&self, kind: &str) -> Vec<&Entity> {
        let mut out: Vec<&Entity> = self.entities.values().filter(|e| e.is(kind)).collect();
        out.sort_by_key(|e| e.id);
        out
    }

    /// Every instance in the order of their numbers.
    pub fn sorted(&self) -> Vec<&Entity> {
        let mut out: Vec<&Entity> = self.entities.values().collect();
        out.sort_by_key(|e| e.id);
        out
    }

    /// The instance a value refers to.
    pub fn deref(&self, value: &Value) -> Option<&Entity> {
        value.as_ref().and_then(|id| self.get(id))
    }
}

// Small files for the tests of the reader and the import.
pub mod testdata;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_and_entities() {
        let file = IfcFile::parse(testdata::BUILDING).expect("reads");
        assert_eq!(file.schema, Schema::Ifc4);
        assert_eq!(
            file.header.view_definition().as_deref(),
            Some("ReferenceView_V1.2")
        );
        assert_eq!(file.header.originating_system, "Mitcad test");
        assert_eq!(file.kind(1), Some("IFCPROJECT"));
        assert!(file.counts[&Arc::from("IFCCARTESIANPOINT")] > 3);
        assert!(file.warnings.is_empty(), "{:?}", file.warnings);
    }

    #[test]
    fn options_leave_attributes_out() {
        let text = "ISO-10303-21;HEADER;FILE_SCHEMA(('IFC2X3'));ENDSEC;DATA;\n\
                    #1=IFCPROPERTYBOUNDEDVALUE('A',$,IFCREAL(2.),IFCREAL(1.),$,$);\n\
                    #2=IFCWALL('0123456789012345678901',$,$,$,$,$,$,$);ENDSEC;END-ISO-10303-21;";
        let file = IfcFile::read(text.as_bytes(), &ReadOptions::import()).expect("reads");
        assert_eq!(file.schema, Schema::Ifc2x3);
        assert_eq!(file.kind(1), Some("IFCPROPERTYBOUNDEDVALUE"));
        assert!(file.get(1).expect("kept").args.is_empty());
        assert_eq!(file.get(2).expect("kept").args.len(), 8);
        assert_eq!(file.left_out, 1);
    }

    #[test]
    fn not_step_text_is_refused() {
        assert!(IfcFile::parse("hello").is_err());
        assert!(IfcFile::parse("").is_err());
        let bad = IfcFile::parse(
            "ISO-10303-21;HEADER;ENDSEC;DATA;#1=IFCA(1,;#2=IFCB(2);#2=IFCB(3);ENDSEC;",
        )
        .expect("reads what it can");
        assert_eq!(bad.warnings.len(), 2, "{:?}", bad.warnings);
        assert_eq!(bad.get(2).expect("kept").args, vec![Value::Integer(3)]);
    }

    #[test]
    fn deeply_nested_record_is_skipped() {
        // mitcad#930: a record of 100 000 nested parentheses is a warning,
        // not a stack overflow; the records after it are read.
        let text = format!(
            "ISO-10303-21;HEADER;FILE_SCHEMA(('IFC4'));ENDSEC;DATA;\n\
             #1=IFCWALL({}));\n#2=IFCB(2);ENDSEC;END-ISO-10303-21;",
            "(".repeat(100_000)
        );
        let file = IfcFile::parse(&text).expect("reads what it can");
        assert_eq!(file.warnings.len(), 1, "{:?}", file.warnings);
        assert!(
            file.warnings[0].starts_with("record not read")
                && file.warnings[0].contains("nested too deeply"),
            "{:?}",
            file.warnings
        );
        assert!(file.get(1).is_none());
        assert_eq!(file.get(2).expect("kept").args, vec![Value::Integer(2)]);
    }

    #[test]
    fn ifczip_archives_open() {
        let dir = std::env::temp_dir().join(format!("mitcad-ifc-zip-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temporary folder");
        let path = dir.join("wall.ifczip");
        let data = mitcad_zip::stored_zip(&[("wall.ifc", testdata::BUILDING.as_bytes())]);
        std::fs::write(&path, data).expect("written");
        let file = IfcFile::open(&path).expect("reads");
        assert_eq!(file.kind(1), Some("IFCPROJECT"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
