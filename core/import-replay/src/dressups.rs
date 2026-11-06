// SPDX-License-Identifier: MIT
//! The dressup failure dump (mitcad#107, mitcad#322): with
//! `MITCAD_IMPORT_DRESSUPS` set to a directory, every fillet or chamfer
//! definition that fails in the geometry kernel is written there, to study
//! what the kernel does not build. Per design, `<dir>/<design>.jsonl` gets
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
//! like the corpora.

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
    /// the dump when it is one fillet or chamfer definition that the
    /// geometry kernel failed.
    pub(crate) fn dump_dressup_failure(
        &mut self,
        index: i64,
        item: &TimelineItem,
        c: &Candidate,
        error: &str,
        states: Option<geometric::States>,
    ) {
        let Some(dir) = directory() else { return };
        // ("the fillet would leave the body open: ...", mitcad#1336, is a
        // failure of the kernel's result too.)
        let error_of =
            |kind: &str| error.starts_with(kind) || error.starts_with(&format!("the {kind}"));
        if !(error_of("fillet") || error_of("chamfer")) {
            return;
        }
        // (A candidate of several definitions, each on its own body: each
        // is written, the error being one of theirs, mitcad#323.)
        if c.defs.is_empty()
            || c.defs.iter().any(|def| {
                (def["type"] != "fillet" && def["type"] != "chamfer") || !def["body"].is_string()
            })
        {
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

    /// Writes one fillet or chamfer definition of a failed candidate.
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
        // The values of the parameters the sizes name.
        let parameters = self.doc.parameters();
        let mut values = serde_json::Map::new();
        for set in def["sets"].as_array().into_iter().flatten() {
            let mut named = Vec::new();
            crate::strings(&set["size"], &mut named);
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
