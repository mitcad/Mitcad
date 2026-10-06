// SPDX-License-Identifier: MIT
//! The result store for C++ (P7d, `mitcad_model::store`): the application
//! and `mitcad-cli` give a document a store folder; recomputes then take
//! results from it, and `persist_results` writes what was costly to
//! evaluate.

use std::path::Path;
use std::time::Duration;

use mitcad_model::store::{self, GcReport, ResultStore};
use serde_json::json;

use crate::Document;

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
        let report = self.0.persist_results(min);
        json!({"results": report.results, "shapes": report.shapes, "bytes": report.bytes,
               "failed": report.failed, "errors": report.errors,
               "ms": report.time.as_secs_f64() * 1000.0})
        .to_string()
    }
}
