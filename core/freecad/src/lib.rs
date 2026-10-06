// SPDX-License-Identifier: MIT
//! Reader for FreeCAD documents (`.FCStd`), without a geometry kernel.
//!
//! An `.FCStd` file is a zip archive ([`mitcad_zip`]) holding:
//!
//! - `Document.xml` ([`document`]): the document's properties, and every
//!   object (name, type, state) with its properties ([`value`]):
//!   placements ([`placement`]), links to other objects and documents,
//!   expressions, the file names of stored shapes;
//! - `GuiDocument.xml` ([`view`]), optional: visibility and colours;
//! - one OCCT B-rep per shape-bearing object (`PartShape5.brp`, from 1.0
//!   `Pad.Shape.brp`; `.bin` when saved binary): the object's result as
//!   FreeCAD last computed it, in the coordinates of the object's container;
//! - lists kept in files of their own ([`binary`]), element maps (1.0) and
//!   a thumbnail.
//!
//! Sketches' geometry and constraints are read on demand ([`sketch`]), and
//! so are spreadsheets' cells ([`spreadsheet`]) and expressions
//! ([`expression`]).
//!
//! Enumerations are stored as indices; [`version`] has the value lists of
//! each FreeCAD version. The format was learnt from FreeCAD's documentation
//! and from files saved by FreeCAD (`tools/freecad-export`); no FreeCAD
//! code is used.

pub mod binary;
pub mod document;
pub mod expression;
pub mod placement;
pub mod sketch;
pub mod spreadsheet;
pub mod value;
pub mod version;
pub mod view;
mod xml;

use std::fmt;
use std::path::{Path, PathBuf};

pub use document::{Document, Object, ObjectState, Property};
pub use mitcad_zip::{Archive, Entry, ZipError};
pub use placement::Placement;
pub use sketch::Sketch;
pub use value::{Color, Enumeration, Expression, LinkRef, Material, ShapeRef, SubName, Value};
pub use version::Version;
pub use view::{GuiDocument, ViewProvider};
pub use xml::Element;

/// The most bytes `Document.xml` or `GuiDocument.xml` may have.
const MAX_XML: u64 = 1 << 30;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FcstdError {
    Io(String),
    Zip(ZipError),
    /// No `Document.xml`: not a FreeCAD document.
    NoDocument,
    /// `Document.xml` or `GuiDocument.xml` does not parse.
    Xml {
        entry: String,
        message: String,
    },
}

impl fmt::Display for FcstdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FcstdError::Io(e) => write!(f, "{e}"),
            FcstdError::Zip(e) => write!(f, "{e}"),
            FcstdError::NoDocument => f.write_str("no Document.xml: not a FreeCAD document"),
            FcstdError::Xml { entry, message } => write!(f, "{entry}: {message}"),
        }
    }
}

impl std::error::Error for FcstdError {}

impl From<ZipError> for FcstdError {
    fn from(e: ZipError) -> Self {
        FcstdError::Zip(e)
    }
}

/// An opened `.FCStd` file: the archive and its parsed documents.
pub struct FcstdFile {
    archive: Archive,
    pub document: Document,
    /// `GuiDocument.xml`, when the file has one that parses.
    pub gui: Option<GuiDocument>,
    /// What could not be read (a view document that does not parse, list
    /// files that are missing or damaged); the rest is usable.
    pub warnings: Vec<String>,
    path: Option<PathBuf>,
}

impl FcstdFile {
    pub fn open(path: &Path) -> Result<FcstdFile, FcstdError> {
        let data =
            std::fs::read(path).map_err(|e| FcstdError::Io(format!("{}: {e}", path.display())))?;
        let mut file = FcstdFile::from_bytes(data)?;
        file.path = Some(path.to_path_buf());
        Ok(file)
    }

    pub fn from_bytes(data: Vec<u8>) -> Result<FcstdFile, FcstdError> {
        let archive = Archive::new(data)?;
        let text = |name: &str| -> Result<Option<String>, FcstdError> {
            let Some(entry) = archive.find(name) else {
                return Ok(None);
            };
            if entry.size > MAX_XML {
                return Err(FcstdError::Xml {
                    entry: name.to_owned(),
                    message: format!("{} bytes is too large", entry.size),
                });
            }
            let bytes = archive.read(entry)?;
            Ok(Some(String::from_utf8_lossy(&bytes).into_owned()))
        };
        let xml = text("Document.xml")?.ok_or(FcstdError::NoDocument)?;
        let mut document = Document::parse(&xml).map_err(|message| FcstdError::Xml {
            entry: "Document.xml".to_owned(),
            message,
        })?;
        let mut warnings = std::mem::take(&mut document.warnings);
        let mut read = |name: &str| archive.read_by_name(name).ok();
        warnings.extend(document.resolve_files(&mut read));
        let gui = match text("GuiDocument.xml") {
            Ok(Some(xml)) => match GuiDocument::parse(&xml) {
                Ok(mut gui) => {
                    warnings.extend(gui.resolve_files(&mut read));
                    Some(gui)
                }
                Err(e) => {
                    warnings.push(format!("GuiDocument.xml: {e}"));
                    None
                }
            },
            Ok(None) => None,
            Err(e) => {
                warnings.push(e.to_string());
                None
            }
        };
        Ok(FcstdFile {
            archive,
            document,
            gui,
            warnings,
            path: None,
        })
    }

    /// The file it was opened from.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn entries(&self) -> &[Entry] {
        self.archive.entries()
    }

    pub fn has_entry(&self, name: &str) -> bool {
        self.archive.find(name).is_some()
    }

    /// An entry's data.
    pub fn read(&self, name: &str) -> Result<Vec<u8>, FcstdError> {
        Ok(self.archive.read_by_name(name)?)
    }

    /// The stored shape of an object: its B-rep data (OCCT text or binary),
    /// None when it has none or the data is empty.
    pub fn shape_data(&self, object: &str) -> Result<Option<Vec<u8>>, FcstdError> {
        let Some(file) = self.document.object(object).and_then(Object::shape_file) else {
            return Ok(None);
        };
        let data = self.read(file)?;
        Ok((!data.is_empty()).then_some(data))
    }

    /// The view data of an object, when the file has it.
    pub fn view(&self, object: &str) -> Option<&ViewProvider> {
        self.gui.as_ref()?.get(object)
    }

    /// Whether an object is shown: the view's visibility, else the object's
    /// own (0.19 and later), else shown.
    pub fn visible(&self, object: &Object) -> bool {
        self.view(&object.name)
            .and_then(ViewProvider::visible)
            .or_else(|| object.visibility())
            .unwrap_or(true)
    }
}
