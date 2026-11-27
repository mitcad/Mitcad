// SPDX-License-Identifier: MIT
//! The dressup failure dump (mitcad#107, mitcad#322): with
//! `MITCAD_IMPORT_DRESSUPS` set to a directory, every fillet or chamfer
//! definition that fails in the geometry kernel is written there, to study
//! what the kernel does not build, and so is every offset, move or delete
//! of faces (`offset_face`, `move_face`, `delete_face`) that the kernel
//! fails (mitcad#2573: their failing inputs as geometry test cases). Per
//! design, `<dir>/<design>.jsonl` gets
//! one line per failure (the item, the definition, the error) and
//! `<dir>/<design>/T<item>-<n>.shape` the input body with its face names
//! (the kernel's stored-result bytes, `Kernel::shape_bytes`), which the
//! names in the definition refer to; `…-before.shape` and `…-after.shape`
//! are the stored bodies of the history's states before and after the item
//! nearest to it (by their measures), when the history has them. With
//! `MITCAD_IMPORT_DRESSUPS_ITEMS` (item indices separated by commas) the
//! items it names are written when taken too (mitcad#328): a fillet or
//! chamfer as above with its result as the input, any other item as every
//! body of the replay after it (`I<item>-replay-<k>.shape`) and of its
//! state (`I<item>-stored-<k>.shape`), to find the first item whose result
//! deviates from the file's. The files
//! are derived from the designs: keep the directory outside the repository,
//! like the corpora. `tools/f3d-rounding-specs.sh` writes it for every file
//! of the corpora, the rounding specs (mitcad#1398,
//! core/import/docs/rounding-specs.md).

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use mitcad_dump::TimelineItem;
use serde_json::json;

use crate::history::Sig;
use crate::{Candidate, ImportKernel, Importer, geometric};

/// The dump directory, none without `MITCAD_IMPORT_DRESSUPS`.
fn directory() -> Option<&'static Path> {
    static DIR: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();
    DIR.get_or_init(|| {
        std::env::var_os("MITCAD_IMPORT_DRESSUPS")
            .filter(|d| !d.is_empty())
            .map(PathBuf::from)
    })
    .as_deref()
}

/// The first this many failures of one item are written.
const PER_ITEM: usize = 4;

/// Past those, failures with an error not written before, up to this many
/// in all.
const PER_ITEM_NEW: usize = 12;

/// The definition types whose failures are written as dressups: fillets
/// and chamfers.
const DRESSUPS: [&str; 2] = ["fillet", "chamfer"];

/// The definition types whose kernel failures are written as face
/// operations (mitcad#2573): the offset, move and delete of a body's faces
/// (`geometry/src/faceops.cpp`).
const FACE_OPERATIONS: [&str; 3] = ["offset_face", "move_face", "delete_face"];

/// Whether a failed candidate of definitions `defs` with `error` is
/// written: fillets and chamfers that the kernel failed, or offsets, moves
/// and deletes of faces that it failed, each definition on a body of its
/// own (a candidate of several definitions has each written, the error
/// being one of theirs, mitcad#323).
fn written(error: &str, defs: &[serde_json::Value]) -> bool {
    if defs.is_empty() || defs.iter().any(|def| !def["body"].is_string()) {
        return false;
    }
    let all = |types: &[&str]| {
        defs.iter()
            .all(|def| def["type"].as_str().is_some_and(|t| types.contains(&t)))
    };
    if all(&DRESSUPS) {
        // ("the fillet would leave the body open: ...", mitcad#1336, is a
        // failure of the kernel's result too.)
        let error_of =
            |kind: &str| error.starts_with(kind) || error.starts_with(&format!("the {kind}"));
        return error_of("fillet") || error_of("chamfer");
    }
    all(&FACE_OPERATIONS) && face_operation_failure(error)
}

/// Whether an offset, move or delete of faces failed in the geometry
/// kernel (`geometry/src/faceops.cpp`): an invalid or open result, faces
/// the kernel's offset refuses or cannot rebuild the body around, or a gap
/// their neighbours do not close; not a face that was not found, nor a
/// result that is not the file's, nor another operation's failure (a
/// fillet, chamfer, boolean or taper that [`crate::kernel_failure`] counts
/// too is no failure of the face operation).
fn face_operation_failure(error: &str) -> bool {
    error.contains("produced an invalid shape")
        || error.contains("the kernel's offset")
        || error.contains("the faces cannot be")
        || error.contains("the faces around the moved faces")
}

/// A file name of a design's label.
fn file_name(label: &str) -> String {
    let name: String = label
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || "-_.".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect();
    if name.is_empty() {
        "design".to_owned()
    } else {
        name
    }
}

/// The items whose dressups are written when taken too
/// (`MITCAD_IMPORT_DRESSUPS_ITEMS`, item indices separated by commas): the
/// body the dressup made, beside the stored bodies of the states before and
/// after it, to find the first item whose result deviates from the file's.
fn taken_items() -> &'static [i64] {
    static ITEMS: std::sync::OnceLock<Vec<i64>> = std::sync::OnceLock::new();
    ITEMS.get_or_init(|| {
        std::env::var("MITCAD_IMPORT_DRESSUPS_ITEMS")
            .unwrap_or_default()
            .split(',')
            .filter_map(|s| s.trim().parse().ok())
            .collect()
    })
}

impl<K: ImportKernel> Importer<'_, K> {
    /// Candidate `c` of item `index` was taken: written to the dump (its
    /// result as the input) when `MITCAD_IMPORT_DRESSUPS_ITEMS` names the
    /// item.
    pub(crate) fn dump_dressup_taken(
        &mut self,
        index: i64,
        item: &TimelineItem,
        c: &Candidate,
        states: Option<geometric::States>,
    ) {
        let Some(dir) = directory() else { return };
        if !taken_items().contains(&index) {
            return;
        }
        let kind = c
            .defs
            .first()
            .and_then(|d| d["type"].as_str())
            .unwrap_or("");
        if c.defs.len() == 1 && (kind == "fillet" || kind == "chamfer") {
            let error = format!("{kind} taken (its result)");
            self.dump_dressup_failure(index, item, c, &error, states);
            return;
        }
        // Any other item: every body of the replay and of the state after it.
        let design = file_name(&self.report.label);
        if std::fs::create_dir_all(dir.join(&design)).is_err() {
            return;
        }
        let kernel = self.doc.kernel();
        for (k, b) in crate::body_scope::bodies(self.doc).iter().enumerate() {
            if let Ok(bytes) = kernel.shape_bytes(b.shape) {
                let _ = std::fs::write(
                    dir.join(format!("{design}/I{index}-replay-{k}.shape")),
                    bytes,
                );
            }
        }
        if let Some(states) = states
            && let Ok(bodies) = self.oracle.state(kernel, states.after)
        {
            for (k, (stored, _)) in bodies.iter().enumerate() {
                if let Ok(bytes) = kernel.shape_bytes(&stored.shape) {
                    let _ = std::fs::write(
                        dir.join(format!("{design}/I{index}-stored-{k}.shape")),
                        bytes,
                    );
                }
            }
        }
    }

    /// Candidate `c` of item `index` failed with `error` (`states`: the
    /// history's states before and after the item, when known): written to
    /// the dump when its definitions are fillets and chamfers, or offsets,
    /// moves and deletes of faces, that the geometry kernel failed
    /// ([`written`]).
    pub(crate) fn dump_dressup_failure(
        &mut self,
        index: i64,
        item: &TimelineItem,
        c: &Candidate,
        error: &str,
        states: Option<geometric::States>,
    ) {
        let Some(dir) = directory() else { return };
        if !written(error, &c.defs) {
            return;
        }
        let design = file_name(&self.report.label);
        // The errors written per item: the first few, then each new error
        // (a later candidate can fail otherwise, mitcad#323), up to a cap.
        type Seen = HashMap<(String, i64), Vec<String>>;
        static SEEN: Mutex<Option<Seen>> = Mutex::new(None);
        {
            let mut seen = SEEN
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let errors = seen
                .get_or_insert_with(HashMap::new)
                .entry((design.clone(), index))
                .or_default();
            let new = !errors.iter().any(|e| e == error);
            if errors.len() >= PER_ITEM_NEW || (errors.len() >= PER_ITEM && !new) {
                return;
            }
            errors.push(error.to_owned());
        }
        if std::fs::create_dir_all(dir.join(&design)).is_err() {
            return;
        }
        for def in &c.defs {
            self.dump_definition(dir, &design, index, item, c, def, error, states);
        }
    }

    /// Writes one fillet, chamfer or face-operation definition of a failed
    /// candidate.
    #[allow(clippy::too_many_arguments)]
    fn dump_definition(
        &mut self,
        dir: &Path,
        design: &str,
        index: i64,
        item: &TimelineItem,
        c: &Candidate,
        def: &serde_json::Value,
        error: &str,
        states: Option<geometric::States>,
    ) {
        let Some(body) = def["body"].as_str() else {
            return;
        };
        static COUNT: AtomicUsize = AtomicUsize::new(0);
        let n = COUNT.fetch_add(1, Ordering::Relaxed);
        let kernel = self.doc.kernel();
        let input = crate::body_scope::bodies(self.doc)
            .iter()
            .find(|b| b.uid.to_string() == body)
            .map(|b| b.shape);
        let mut files = serde_json::Map::new();
        let mut write = |suffix: &str, bytes: Vec<u8>| {
            let relative = format!("{design}/T{index}-{n}{suffix}.shape");
            if std::fs::write(dir.join(&relative), bytes).is_ok() {
                files.insert(
                    if suffix.is_empty() {
                        "input"
                    } else {
                        &suffix[1..]
                    }
                    .to_owned(),
                    json!(relative),
                );
            }
        };
        let sig = input.and_then(|s| Sig::of(kernel, s));
        if let Some(bytes) = input.and_then(|s| kernel.shape_bytes(s).ok()) {
            write("", bytes);
        }
        // The stored bodies of the states before and after it nearest to the
        // input body.
        let mut distances = serde_json::Map::new();
        if let (Some(states), Some(sig)) = (states, sig) {
            for (suffix, q) in [("-before", states.before), ("-after", Some(states.after))] {
                let Some(q) = q else { continue };
                let Ok(bodies) = self.oracle.state(kernel, q) else {
                    continue;
                };
                let nearest = bodies
                    .iter()
                    .min_by(|a, b| sig.distance(&a.1).total_cmp(&sig.distance(&b.1)));
                if let Some((stored, s)) = nearest {
                    distances.insert(suffix[1..].to_owned(), json!(sig.distance(s)));
                    if let Ok(bytes) = kernel.shape_bytes(&stored.shape) {
                        write(suffix, bytes);
                    }
                }
            }
        }
        // The values of the parameters the sizes (a dressup's sets) or the
        // distance (an offset of faces) name.
        let parameters = self.doc.parameters();
        let mut values = serde_json::Map::new();
        let sizes = def["sets"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|set| &set["size"])
            .chain(std::iter::once(&def["distance"]));
        for size in sizes {
            let mut named = Vec::new();
            crate::strings(size, &mut named);
            for name in named {
                if let Some(v) = parameters.find(name).and_then(|id| parameters.value(id)) {
                    values.insert(name.to_owned(), json!(v));
                }
            }
        }
        let line = json!({
            "values": values,
            "file": self.report.label,
            "item": index,
            "name": item.name(),
            "type": def["type"],
            "note": c.note,
            "error": error,
            "states": states.map(|s| json!([s.before, s.after])),
            "def": def,
            "files": files,
            "sig_distance": distances,
        });
        let path = dir.join(format!("{design}.jsonl"));
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            let _ = writeln!(f, "{line}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::written;
    use serde_json::json;

    /// The fillets and chamfers the kernel failed are written as before,
    /// and the offsets, moves and deletes of faces it failed now too
    /// (mitcad#2573), with the kernel's own messages of
    /// `geometry/src/faceops.cpp`; a face that was not found, a result that
    /// is not the file's or another type is not.
    #[test]
    fn kernel_failures_of_dressups_and_face_operations_are_written() {
        let fillet = json!({"type": "fillet", "body": "b1", "sets": []});
        let chamfer = json!({"type": "chamfer", "body": "b2", "sets": []});
        assert!(written(
            "fillet failed: the edges",
            std::slice::from_ref(&fillet)
        ));
        assert!(written(
            "the chamfer would leave the body open",
            &[fillet.clone(), chamfer.clone()]
        ));
        assert!(!written(
            "the extrusion does not cut into any participant body",
            std::slice::from_ref(&fillet)
        ));

        let offset = json!({"type": "offset_face", "body": "b1", "faces": ["F2"], "distance": 1.0});
        let delete = json!({"type": "delete_face", "body": "b1", "faces": ["F2", "F3"]});
        let moved = json!({"type": "move_face", "body": "b1", "faces": ["F2"]});
        for error in [
            "the kernel's offset of the body with F2 moved gave no shape: its rebuilt faces leave \
             free edges and the shell that does not close up was dropped",
            "the faces cannot be moved: the body rebuilt around them does not close up",
            "the kernel's offset refuses the body: the surface of F2 cannot be offset",
            "the faces cannot be moved by this distance: the offset surfaces cross",
            "the offset face produced an invalid shape",
        ] {
            assert!(written(error, std::slice::from_ref(&offset)), "{error}");
            assert!(written(error, std::slice::from_ref(&moved)), "{error}");
        }
        assert!(written(
            "the faces cannot be deleted: the neighbouring faces do not close the gap",
            std::slice::from_ref(&delete)
        ));
        // Not the kernel's failure.
        assert!(!written(
            "a face was not found",
            std::slice::from_ref(&delete)
        ));
        assert!(!written(
            crate::NOT_IN_HISTORY,
            std::slice::from_ref(&offset)
        ));
        assert!(!written(crate::OUT_OF_TIME, std::slice::from_ref(&offset)));
        // A face operation with a fillet's error, a fillet with a face
        // operation's, a mix of both, another type, a definition without
        // its body.
        assert!(!written("fillet failed", std::slice::from_ref(&offset)));
        assert!(!written(
            "the faces cannot be deleted: the neighbouring faces do not close the gap",
            std::slice::from_ref(&fillet)
        ));
        assert!(!written("fillet failed", &[fillet.clone(), offset.clone()]));
        assert!(!written(
            "the faces cannot be moved: the body rebuilt around them does not close up",
            &[json!({"type": "shell", "body": "b1"})]
        ));
        assert!(!written(
            "the faces cannot be deleted: the neighbouring faces do not close the gap",
            &[json!({"type": "delete_face", "faces": ["F2"]})]
        ));
        assert!(!written("fillet failed", &[]));
    }
}
