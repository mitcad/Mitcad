// SPDX-License-Identifier: MIT
//! The caches of computed results for diagnostics (P7d): the `cache` query
//! tells what the memory cache holds and did, what the result store did
//! (and, with `disk`, holds), and where the last recompute's results came
//! from. Sizes are bytes; what the memory cache takes is estimated.

use std::collections::BTreeMap;

use serde_json::{Value, json};

use crate::document::Document;
use crate::ids::FeatureUid;
use crate::kernel::Kernel;
use crate::store;

fn ms(time: std::time::Duration) -> f64 {
    time.as_secs_f64() * 1000.0
}

/// Totals by a key, the largest first: (key, items, results, bytes).
fn largest_first(totals: BTreeMap<String, (u64, u64, u64)>, key: &str, items: &str) -> Value {
    let mut rows: Vec<(String, (u64, u64, u64))> = totals.into_iter().collect();
    rows.sort_by(|a, b| b.1.2.cmp(&a.1.2).then(a.0.cmp(&b.0)));
    Value::Array(
        rows.into_iter()
            .map(|(name, (count, results, bytes))| {
                json!({key: name, items: count, "results": results, "bytes": bytes})
            })
            .collect(),
    )
}

impl<K: Kernel> Document<K> {
    fn feature_label(&self, uid: FeatureUid) -> Value {
        self.feature(uid).map_or(Value::Null, |f| json!(f.name))
    }

    pub(super) fn cache_json(&self, disk: bool) -> Value {
        let cache = self.cache();
        let stats = cache.stats;
        let usage = cache.usage();
        let mut by_type: BTreeMap<String, (u64, u64, u64)> = BTreeMap::new();
        for row in &usage {
            let total = by_type.entry(row.type_name.to_owned()).or_default();
            total.0 += 1;
            total.1 += row.results as u64;
            total.2 += row.bytes;
        }
        let bytes = cache.bytes();
        let memory = json!({
            "bytes": bytes,
            "budget": cache.budget(),
            "used": cache.budget().filter(|b| *b > 0).map(|b| bytes as f64 / b as f64),
            "results": usage.iter().map(|r| r.results).sum::<usize>(),
            "features": usage.len(),
            "lookups": stats.lookups,
            "hits": stats.hits,
            "misses": stats.lookups - stats.hits,
            "restored": stats.restored,
            "evicted": stats.evicted,
            "evicted_bytes": stats.evicted_bytes,
            "by_feature": usage.iter().map(|r| json!({
                "uid": r.uid, "name": self.feature_label(r.uid), "type": r.type_name,
                "results": r.results, "bytes": r.bytes})).collect::<Vec<_>>(),
            "by_type": largest_first(by_type, "type", "features"),
        });

        // Where the last recompute's results came from, in timeline order.
        let result = self.last_result();
        let evaluated: BTreeMap<FeatureUid, f64> = result
            .evaluated
            .iter()
            .zip(&result.times)
            .map(|(uid, time)| (*uid, ms(*time)))
            .collect();
        let restored: BTreeMap<FeatureUid, f64> = result
            .restored
            .iter()
            .zip(&result.restore_times)
            .map(|(uid, time)| (*uid, ms(*time)))
            .collect();
        let mut cached = 0;
        let features: Vec<Value> = result
            .results
            .iter()
            .map(|r| {
                let (source, time) = if let Some(time) = evaluated.get(&r.uid) {
                    ("evaluated", *time)
                } else if let Some(time) = restored.get(&r.uid) {
                    ("store", *time)
                } else if r.output.is_some() {
                    cached += 1;
                    ("cache", 0.0)
                } else {
                    (r.status.as_str(), 0.0)
                };
                json!({"uid": r.uid, "name": self.feature_label(r.uid),
                       "type": self.feature(r.uid).map(|f| f.def.type_name()),
                       "source": source, "ms": time,
                       "evaluated_ms": r.output.as_ref().map(|o| ms(o.time))})
            })
            .collect();
        let last = json!({
            "evaluated": result.evaluated.len(),
            "restored": result.restored.len(),
            "cached": cached,
            "evaluate_ms": evaluated.values().sum::<f64>(),
            "restore_ms": restored.values().sum::<f64>(),
            "features": features,
        });

        let store = self.result_store().map(|s| {
            let stats = store::stats();
            let mut value = json!({
                "dir": s.dir().to_string_lossy(),
                "build_id": s.build_id(),
                "lookups": stats.lookups, "hits": stats.hits, "misses": stats.misses,
                "damaged": stats.damaged, "other_build": stats.other_build,
                "read_bytes": stats.read_bytes, "read_ms": stats.read_ns as f64 / 1.0e6,
                "writes": stats.writes, "write_bytes": stats.write_bytes,
                "write_ms": stats.write_ns as f64 / 1.0e6, "write_errors": stats.write_errors,
            });
            if disk {
                value["disk"] = disk_json(&store::usage(s.dir(), s.build_id()));
            }
            value
        });
        json!({"memory": memory, "store": store, "last_recompute": last})
    }
}

/// What the store holds: totals, and this build's results by document,
/// by feature and by type, the largest first.
fn disk_json(usage: &store::StoreUsage) -> Value {
    let mut by_document: BTreeMap<String, (u64, u64, u64)> = BTreeMap::new();
    let mut by_type: BTreeMap<String, (u64, u64, u64)> = BTreeMap::new();
    for entry in &usage.entries {
        for (totals, key) in [
            (&mut by_document, &entry.document),
            (&mut by_type, &entry.type_name),
        ] {
            let total = totals.entry(key.clone()).or_default();
            total.0 += 1;
            total.1 += u64::from(entry.variants);
            total.2 += entry.bytes;
        }
    }
    let mut features: Vec<&store::StoredEntry> = usage.entries.iter().collect();
    features.sort_by(|a, b| b.bytes.cmp(&a.bytes).then(a.feature.cmp(&b.feature)));
    json!({
        "files": usage.files,
        "bytes": usage.bytes,
        "other_builds": {"files": usage.other_files, "bytes": usage.other_bytes},
        "damaged": usage.damaged,
        "by_document": largest_first(by_document, "document", "features"),
        "by_type": largest_first(by_type, "type", "features"),
        "by_feature": features.iter().map(|e| json!({
            "document": e.document, "uid": e.feature, "name": e.name, "type": e.type_name,
            "results": e.variants, "bytes": e.bytes})).collect::<Vec<_>>(),
    })
}
