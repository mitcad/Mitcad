// SPDX-License-Identifier: MIT
//! `.solidcraft` designs (mitcad#450): SolidCraft's parametric design file,
//! plain JSON (`"format": "solidcraft/N"`) or a zip container of that JSON
//! (`document.json`) and its blobs (STEP text and meshes). No geometry.
//!
//! A design of any format is first upgraded to one format, [`PINNED`]
//! (50, the format of SolidCraft's release 0.40), by SolidCraft's own
//! upgrade steps (`third_party/solidcraft/format.rs`), so that the import
//! maps a single version. A SolidCraft drawing (a file of its own) is
//! refused with a message saying so. Feature inputs without parameter names get them as
//! SolidCraft gives them when it opens such a design, and components
//! without an occurrence get one at the identity.
//!
//! - [`document`]: the design: parameters, features (each with its kind's
//!   JSON, typed on demand by [`kinds`]), components, occurrences, the
//!   assembly's joints, sheet rules, materials and appearances.
//! - [`sketch`]: sketches: points, curves, constraints and dimensions.
//! - [`inputs`]: each feature's inputs in the order of its parameter names.
//! - [`expr`]: SolidCraft's expressions in Mitcad's expression language.

pub mod container;
pub mod document;
pub mod expr;
pub mod inputs;
pub mod kinds;
pub mod sketch;

#[path = "../../../third_party/solidcraft/format.rs"]
pub mod format;

// SolidCraft's plastic rule library (mitcad#459).
#[path = "../../../third_party/solidcraft/plastic.rs"]
pub mod plastic;

use std::path::Path;

use serde_json::Value;

pub use document::{Component, Design, Feature, Occurrence, Param};
pub use format::PINNED;

/// A design read: the document upgraded to [`PINNED`], with what the
/// upgrade did.
#[derive(Debug, Clone)]
pub struct SolidcraftFile {
    pub design: Design,
    /// The format the file was saved in (`solidcraft/N`; the legacy
    /// `solvecraft/N` likewise).
    pub saved_format: u32,
    /// The changes of a pending format the file has (`format_pending`).
    pub saved_pending: Vec<String>,
    /// The upgrade steps that ran, by name (`2 to 3`, a pending change).
    pub upgrades: Vec<String>,
    /// A zip container rather than plain JSON.
    pub container: bool,
}

/// The largest file read (SolidCraft's own cap on an expanded part).
pub const MAX_BYTES: usize = 256 << 20;

impl SolidcraftFile {
    /// Reads the design at `path`. A file with an extension other programs
    /// use too (`.sc`, [`has_shared_extension`]) is read only when its
    /// format key says it is a SolidCraft design ([`check_design`]), else
    /// refused with "Not a SolidCraft design: …" (mitcad#1622).
    pub fn open(path: &Path) -> Result<Self, String> {
        let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
        if meta.len() as usize > MAX_BYTES {
            return Err(format!("the file is larger than {} MiB", MAX_BYTES >> 20));
        }
        let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
        if has_shared_extension(path) {
            check_design(&bytes).map_err(|reason| format!("Not a SolidCraft design: {reason}"))?;
        }
        Self::from_bytes(&bytes)
    }

    /// A design from the file's bytes: a container when they start as a
    /// zip, else the JSON document.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.starts_with(b"PK\x03\x04") {
            let value = container::read(bytes)?;
            let mut file = Self::from_value(value)?;
            file.container = true;
            Ok(file)
        } else {
            let text = std::str::from_utf8(bytes)
                .map_err(|_| "not a SolidCraft design: the file is not UTF-8 text".to_owned())?;
            Self::from_json(text)
        }
    }

    pub fn from_json(text: &str) -> Result<Self, String> {
        let value: Value =
            serde_json::from_str(text).map_err(|e| format!("not a SolidCraft design: {e}"))?;
        Self::from_value(value)
    }

    /// Upgrades and reads a design's JSON.
    pub fn from_value(mut v: Value) -> Result<Self, String> {
        let Some(obj) = v.as_object_mut() else {
            return Err("not a SolidCraft design (expected a JSON object)".to_owned());
        };
        let version = match obj.get("format") {
            None => 1,
            // Drawings are files of their own (`solidcraft-drawing/N`): sheets
            // that draw designs saved elsewhere, with no model of their own.
            Some(Value::String(f)) if f.starts_with("solidcraft-drawing/") => {
                return Err(format!(
                    "a SolidCraft drawing (format `{f}`), not a design: this import reads designs; open the designs the drawing draws instead"
                ));
            }
            Some(Value::String(f)) => {
                version_of(f).ok_or_else(|| format!("not a SolidCraft design (format `{f}`)"))?
            }
            Some(_) => return Err("not a SolidCraft design (format is not a string)".to_owned()),
        };
        let pending: Option<Vec<String>> = match obj.remove("format_pending") {
            None => None,
            Some(Value::Array(list)) => Some(
                list.iter()
                    .map(|n| n.as_str().map(str::to_owned))
                    .collect::<Option<Vec<_>>>()
                    .ok_or("damaged design: `format_pending` is not a list of names")?,
            ),
            Some(_) => return Err("damaged design: `format_pending` is not a list".to_owned()),
        };
        let upgrades = format::upgrade(&mut v, version, pending.as_deref())?;
        format::name_all_feature_inputs(&mut v, &|f| inputs::inputs(f).len());
        let mut design: Design =
            serde_json::from_value(v).map_err(|e| format!("design file: {e}"))?;
        design.add_missing_occurrences();
        Ok(Self {
            design,
            saved_format: version,
            saved_pending: pending.unwrap_or_default(),
            upgrades,
            container: false,
        })
    }
}

/// The version in a format string (`solidcraft/N`, or the legacy
/// `solvecraft/N`).
pub fn version_of(s: &str) -> Option<u32> {
    s.strip_prefix("solidcraft/")
        .or_else(|| s.strip_prefix("solvecraft/"))?
        .parse()
        .ok()
}

/// The extensions of SolidCraft designs, lower case: `.sc`, which
/// SolidCraft saves designs as now (mitcad#1622), `.solidcraft`, and the
/// legacy `.solvecraft`.
pub const EXTENSIONS: [&str; 3] = ["sc", "solidcraft", "solvecraft"];

/// The extensions of [`EXTENSIONS`] that other programs use too: a file
/// with one is a design only when [`check_design`] passes.
pub const SHARED_EXTENSIONS: [&str; 1] = ["sc"];

fn extension_in(path: &Path, list: &[&str]) -> bool {
    path.extension()
        .and_then(|x| x.to_str())
        .is_some_and(|x| list.iter().any(|e| x.eq_ignore_ascii_case(e)))
}

/// Whether the path has a SolidCraft design's extension (any case).
pub fn has_design_extension(path: &Path) -> bool {
    extension_in(path, &EXTENSIONS)
}

/// Whether the path's extension is a SolidCraft one that other programs
/// use too (`.sc`, any case).
pub fn has_shared_extension(path: &Path) -> bool {
    extension_in(path, &SHARED_EXTENSIONS)
}

/// How many of a file's first bytes [`check_design`] reads for its format
/// key.
pub const HEAD_BYTES: usize = 64 << 10;

/// Whether a file's content is a SolidCraft design by its format key: a
/// design container (`container.json`'s `solidcraft-container/1`, which
/// needs the whole file), or a JSON object whose top-level `format`, within
/// its first [`HEAD_BYTES`], starts with `solidcraft/` (or the legacy
/// `solvecraft/`). The error says why not, for "Not a SolidCraft design: …"
/// (mitcad#1622).
pub fn check_design(bytes: &[u8]) -> Result<(), String> {
    if bytes.starts_with(b"PK\x03\x04") {
        return if container::is_container(bytes) {
            Ok(())
        } else {
            Err("the file is a zip archive but not a SolidCraft design container".to_owned())
        };
    }
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(HEAD_BYTES)]);
    let format = head_format(&head)?;
    if format.starts_with("solidcraft-drawing/") {
        Err("the file is a SolidCraft drawing; open the designs it draws instead".to_owned())
    } else if format.starts_with("solidcraft/") || format.starts_with("solvecraft/") {
        Ok(())
    } else {
        Err(format!(
            "the file's format is `{format}`, not a SolidCraft design's"
        ))
    }
}

/// [`check_design`] on the file at `path` for a path with a shared
/// extension ([`has_shared_extension`]); any other path passes unread. A
/// file that cannot be read passes too: the import says why. The error is
/// the whole message, "Not a SolidCraft design: …".
pub fn check_file(path: &Path) -> Result<(), String> {
    use std::io::Read;
    if !has_shared_extension(path) {
        return Ok(());
    }
    let Ok(file) = std::fs::File::open(path) else {
        return Ok(());
    };
    let mut head = Vec::new();
    if file.take(HEAD_BYTES as u64).read_to_end(&mut head).is_err() {
        return Ok(());
    }
    let bytes = if head.starts_with(b"PK\x03\x04") {
        // A container's directory is at its end: the whole file, within
        // the size the import reads.
        match std::fs::metadata(path) {
            Ok(meta) if meta.len() as usize <= MAX_BYTES => match std::fs::read(path) {
                Ok(all) => all,
                Err(_) => return Ok(()),
            },
            _ => return Ok(()),
        }
    } else {
        head
    };
    check_design(&bytes).map_err(|reason| format!("Not a SolidCraft design: {reason}"))
}

/// The top-level `format` string of the JSON object that `head` (a file's
/// first bytes) starts, read without parsing the rest: keys of nested
/// objects and strings in values are skipped. The error says why there is
/// none.
fn head_format(head: &str) -> Result<String, String> {
    let mut chars = head.chars().peekable();
    let skip_space = |chars: &mut std::iter::Peekable<std::str::Chars>| {
        while chars.peek().is_some_and(|c| c.is_whitespace()) {
            chars.next();
        }
    };
    // A string's text after its opening quote, escapes kept as written;
    // None when the head ends inside it.
    let string = |chars: &mut std::iter::Peekable<std::str::Chars>| -> Option<String> {
        let mut text = String::new();
        loop {
            match chars.next()? {
                '"' => return Some(text),
                '\\' => {
                    text.push('\\');
                    text.push(chars.next()?);
                }
                c => text.push(c),
            }
        }
    };
    let not_object = || "the file is not a JSON object".to_owned();
    let no_key = || "the file has no `format` key at its start".to_owned();
    skip_space(&mut chars);
    if chars.next() != Some('{') {
        return Err(not_object());
    }
    let mut depth = 1usize;
    let mut expect_key = true;
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                let text = string(&mut chars).ok_or_else(no_key)?;
                if depth == 1 && expect_key {
                    expect_key = false;
                    skip_space(&mut chars);
                    match chars.next() {
                        Some(':') => {}
                        Some(_) => return Err(not_object()),
                        None => return Err(no_key()),
                    }
                    if text == "format" {
                        skip_space(&mut chars);
                        return match chars.next() {
                            Some('"') => string(&mut chars).ok_or_else(no_key),
                            Some(_) => Err("the file's `format` is not a string".to_owned()),
                            None => Err(no_key()),
                        };
                    }
                }
            }
            '{' | '[' => depth += 1,
            '}' | ']' => {
                depth -= 1;
                if depth == 0 {
                    return Err(no_key());
                }
            }
            ',' if depth == 1 => expect_key = true,
            _ => {}
        }
    }
    Err(no_key())
}

/// Whether bytes look like a SolidCraft design: a container, or JSON with
/// a SolidCraft format string near its start.
pub fn looks_like_design(bytes: &[u8]) -> bool {
    if bytes.starts_with(b"PK\x03\x04") {
        return container::read(bytes).is_ok();
    }
    let head = &bytes[..bytes.len().min(4096)];
    let head = String::from_utf8_lossy(head);
    head.trim_start().starts_with('{')
        && (head.contains("\"solidcraft/") || head.contains("\"solvecraft/"))
}

#[cfg(test)]
mod tests;
