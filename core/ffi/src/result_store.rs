// SPDX-License-Identifier: MIT
//! The result store for C++ (P7d, `mitcad_model::store`): the application
//! and `mitcad-cli` give a document a store folder; recomputes then take
//! results from it, and `persist_results` writes what was costly to
//! evaluate.

use std::collections::HashMap;
use std::path::Path;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use cxx::SharedPtr;
use mitcad_model::store::{self, GcReport, PersistReport, ResultStore};
use mitcad_model::{BodyUid, FinalBodies};
use serde_json::json;

use crate::Document;
use crate::kernel::Shape;
use crate::kernel::shape::ffi::ShapeList;

fn gc_json(report: &GcReport) -> String {
    json!({"files": report.files, "bytes": report.bytes, "removed": report.removed,
           "removed_bytes": report.removed_bytes})
    .to_string()
}

pub fn result_store_gc(dir: &str, budget_mb: u64) -> String {
    gc_json(&store::gc(
        Path::new(dir),
        budget_mb.saturating_mul(1024 * 1024),
    ))
}

pub fn result_store_clear(dir: &str) -> String {
    gc_json(&store::clear(Path::new(dir)))
}

/// The `cache` query's answer with the process's memory (`process`:
/// `resident`, `peak_resident`, `heap` bytes); other answers as they are.
pub fn with_process_memory(query: &str, answer: String) -> String {
    // Queries come many times a change; only this one is parsed again.
    let is_cache = query.contains("\"cache\"")
        && serde_json::from_str::<serde_json::Value>(query)
            .is_ok_and(|q| q.get("query").and_then(|v| v.as_str()) == Some("cache"));
    if !is_cache {
        return answer;
    }
    let Ok(mut value) = serde_json::from_str::<serde_json::Value>(&answer) else {
        return answer;
    };
    let memory = crate::kernel::persist::ffi::process_memory();
    value["process"] = json!({"resident": memory.resident, "peak_resident": memory.peak_resident,
                              "heap": memory.heap});
    value.to_string()
}

impl Document {
    pub(crate) fn set_memory_cache_budget(&mut self, megabytes: u64) {
        self.0
            .set_memory_budget((megabytes > 0).then(|| megabytes.saturating_mul(1024 * 1024)));
    }

    pub(crate) fn set_result_store(&mut self, dir: &str, build_id: &str, label: &str) {
        self.0.set_result_store(
            (!dir.is_empty()).then(|| ResultStore::new(dir, build_id).with_label(label)),
        );
    }

    pub(crate) fn persist_results(&mut self, min_ms: f64) -> String {
        let min = Duration::from_secs_f64(min_ms.max(0.0) / 1000.0);
        persist_json(&self.0.persist_results(min))
    }

    pub(crate) fn hand_off_results(&self, dir: &str, build_id: &str, label: &str) -> String {
        let store = ResultStore::new(dir, build_id).with_label(label);
        persist_json(&self.0.hand_off_results(&store))
    }

    pub(crate) fn take_results_to_write(&self, min_ms: f64) -> Box<ResultWrites> {
        let min = Duration::from_secs_f64(min_ms.max(0.0) / 1000.0);
        Box::new(ResultWrites(Some(self.0.take_results_to_write(min))))
    }

    // Display meshes kept in the store (mitcad#858).

    pub(crate) fn restore_display_meshes(&self, request: &str) -> String {
        let start = Instant::now();
        let (bodies, options) = mesh_request(request);
        let mut restored = Vec::new();
        let mut faces = 0;
        if let Some(store) = self.0.result_store() {
            let before = store::stats().mesh_read_bytes;
            for uid in bodies {
                let (Some(version), Some(shape)) =
                    (self.0.body_version(uid), self.0.body_shape(uid))
                else {
                    continue;
                };
                if let Some(given) =
                    store.restore_display_mesh(self.0.kernel(), version, &options, shape)
                {
                    restored.push(uid.to_string());
                    faces += given;
                }
            }
            // (The counters are the process's: other documents may read
            // meanwhile, rarely while one opens.)
            let bytes = store::stats().mesh_read_bytes.saturating_sub(before);
            return json!({"restored": restored, "faces": faces, "bytes": bytes,
                          "ms": start.elapsed().as_secs_f64() * 1000.0})
            .to_string();
        }
        json!({"restored": restored, "faces": faces, "bytes": 0, "ms": 0.0}).to_string()
    }

    pub(crate) fn persist_display_meshes(&self, request: &str) -> String {
        let start = Instant::now();
        let (bodies, options) = mesh_request(request);
        let (mut stored, mut bytes, mut errors) = (0, 0, Vec::new());
        if let Some(store) = self.0.result_store() {
            for uid in bodies {
                let (Some(version), Some(shape)) =
                    (self.0.body_version(uid), self.0.body_shape(uid))
                else {
                    continue;
                };
                match store.write_display_mesh(self.0.kernel(), version, &options, shape) {
                    Ok(written) => {
                        stored += 1;
                        bytes += written;
                    }
                    Err(error) => errors.push(format!("{uid}: {error}")),
                }
            }
        }
        json!({"stored": stored, "bytes": bytes, "failed": errors.len(), "errors": errors,
               "ms": start.elapsed().as_secs_f64() * 1000.0})
        .to_string()
    }

    pub(crate) fn display_mesh_write(&self, request: &str) -> Box<DisplayMeshWrite> {
        let (bodies, options) = mesh_request(request);
        let versions = bodies
            .into_iter()
            .filter_map(|uid| Some((uid.to_string(), self.0.body_version(uid)?)))
            .collect();
        Box::new(DisplayMeshWrite {
            store: self.0.result_store().cloned(),
            options,
            versions,
        })
    }

    pub(crate) fn display_feed(&mut self, options: &str) -> Box<DisplayFeed> {
        let feed = FinalBodies::new();
        self.0.feed_final_bodies(feed.clone());
        Box::new(DisplayFeed {
            feed,
            store: self.0.result_store().cloned(),
            options: options.to_owned(),
            left_to_store: AtomicUsize::new(0),
        })
    }
}

/// Results taken from a document for its result store, written on another
/// thread (mitcad#1796): an autosave takes them on the UI thread and its
/// writer writes them.
pub struct ResultWrites(Option<mitcad_model::ResultWrites>);

impl ResultWrites {
    pub(crate) fn count(&self) -> usize {
        self.0.as_ref().map_or(0, mitcad_model::ResultWrites::len)
    }

    pub(crate) fn write_taken(&mut self) -> String {
        persist_json(&self.0.take().map(|w| w.write()).unwrap_or_default())
    }
}

/// The bodies of an open's recompute that no later feature changes, as
/// they become so (mitcad#881), for the application's mesher thread: the
/// recompute puts them in on the model's thread, the mesher takes them on
/// its own. A body whose display meshes the result store keeps is left to
/// the store (read once the recompute is over).
pub struct DisplayFeed {
    feed: FinalBodies<SharedPtr<Shape>>,
    store: Option<ResultStore>,
    options: String,
    left_to_store: AtomicUsize,
}

impl DisplayFeed {
    pub(crate) fn take(&self, wait_ms: u32, mut shapes: Pin<&mut ShapeList>) -> Vec<String> {
        let mut uids = Vec::new();
        for body in self.feed.take(Duration::from_millis(u64::from(wait_ms))) {
            if self
                .store
                .as_ref()
                .is_some_and(|store| store.has_display_mesh(body.version, &self.options))
            {
                self.left_to_store.fetch_add(1, Ordering::Relaxed);
                continue;
            }
            uids.push(body.uid.to_string());
            shapes.as_mut().push(body.shape);
        }
        uids
    }

    pub(crate) fn is_done(&self) -> bool {
        self.feed.is_done()
    }

    pub(crate) fn given(&self) -> usize {
        self.feed.given()
    }

    pub(crate) fn left_to_store(&self) -> usize {
        self.left_to_store.load(Ordering::Relaxed)
    }

    pub(crate) fn changed_later(&self) -> usize {
        self.feed.changed_later()
    }
}

/// Display meshes written after an open (mitcad#880): the document's store
/// and the bodies' versions as the open's job left them, so that the
/// application's writer stores the triangulations it took there on its own
/// thread, whatever the document does meanwhile (changed, closed).
pub struct DisplayMeshWrite {
    store: Option<ResultStore>,
    options: String,
    versions: HashMap<String, u128>,
}

impl DisplayMeshWrite {
    pub(crate) fn has_body(&self, uid: &str) -> bool {
        self.store.is_some() && self.versions.contains_key(uid)
    }

    pub(crate) fn write_body(&self, uid: &str, mesh: &[u8]) -> Result<u64, String> {
        let (Some(store), Some(version)) = (&self.store, self.versions.get(uid)) else {
            return Err(format!("{uid}: no store or no such body"));
        };
        store
            .write_display_mesh_data(*version, &self.options, mesh)
            .map_err(|error| format!("{uid}: {error}"))
    }
}

/// The bodies and the meshing options of a display mesh request; bodies
/// whose uids do not parse are left out.
fn mesh_request(request: &str) -> (Vec<BodyUid>, String) {
    let value: serde_json::Value = serde_json::from_str(request).unwrap_or_default();
    let bodies = value["bodies"]
        .as_array()
        .map(|bodies| {
            bodies
                .iter()
                .filter_map(|uid| uid.as_str()?.parse().ok())
                .collect()
        })
        .unwrap_or_default();
    let options = value["options"].as_str().unwrap_or_default().to_owned();
    (bodies, options)
}

fn persist_json(report: &PersistReport) -> String {
    json!({"results": report.results, "shapes": report.shapes, "bytes": report.bytes,
           "quick": report.quick, "failed": report.failed, "errors": report.errors,
           "ms": report.time.as_secs_f64() * 1000.0})
    .to_string()
}
