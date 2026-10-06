// SPDX-License-Identifier: MIT
//! ASM binary files, the `.smb`/`.smbh` body blobs of an `.f3d` design;
//! see `ASM_FORMAT.md` for what is known about the format.

pub mod file;
pub mod geom;
pub mod history;
pub mod token;
pub mod writer;

pub use file::{AsmError, AsmFile, Header, Record};
pub use token::Token;

use std::collections::BTreeMap;

/// Record types whose layout the converter understands.
pub const KNOWN_TYPES: &[&str] = &[
    "asmheader",
    "body",
    "lump",
    "shell",
    "face",
    "loop",
    "coedge",
    "tcoedge-coedge",
    "edge",
    "tedge-edge",
    "vertex",
    "tvertex-vertex",
    "point",
    "transform",
    "straight-curve",
    "ellipse-curve",
    "intcurve-curve",
    "pcurve",
    "plane-surface",
    "cone-surface",
    "sphere-surface",
    "torus-surface",
    "spline-surface",
];

/// Coverage of one file: record and subtype counts, and which are not
/// understood.
#[derive(Clone, Debug, Default)]
pub struct Coverage {
    pub record_types: BTreeMap<String, usize>,
    /// Subtype object names (`exact_int_cur`, `rb_blend_spl_sur`, ...).
    pub subtypes: BTreeMap<String, usize>,
    /// Record types that are neither known nor attributes or history.
    pub unknown_types: BTreeMap<String, usize>,
}

impl Coverage {
    pub fn of(file: &AsmFile) -> Coverage {
        let mut c = Coverage::default();
        for r in &file.records {
            *c.record_types.entry(r.type_name.clone()).or_insert(0) += 1;
            let ignorable = r.in_history
                || r.base_type() == "attrib"
                || r.type_name.ends_with("-attrib")
                || r.type_name == "delta_state"
                || r.type_name == "Begin-of-ASM-History-Data";
            if !ignorable && !KNOWN_TYPES.contains(&r.type_name.as_str()) {
                *c.unknown_types.entry(r.type_name.clone()).or_insert(0) += 1;
            }
        }
        for &i in &file.subtypes {
            if let Some(Token::Ident(name)) = file.tokens.get(i + 1) {
                *c.subtypes.entry(name.clone()).or_insert(0) += 1;
            }
        }
        c
    }
}
