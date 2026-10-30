// SPDX-License-Identifier: MIT
//! The `hole_sizes` query (mitcad#903): the clearance holes of ISO 273 and
//! the head recesses (counterbore, countersink, counterdrill) of the ISO
//! metric screw sizes, from `core/model/data/hole_sizes.json`, which the
//! Hole panel fills its fields from; with a thread's `major` diameter, also
//! a tapped hole's countersink, and with its `thread` the screw size its
//! counterbore and counterdrill come from (mitcad#980).

use serde::Deserialize;
use serde_json::{Value, json};

use super::ApiError;
use mitcad_model::features::ThreadStandard;
use mitcad_model::features::hole_sizes::{
    self, COUNTERSINK_ANGLE_DEGREES, ClearanceFit, ScrewSize,
};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct HoleSizesQuery {
    /// One size (`M6`); every size when left out.
    #[serde(default)]
    size: Option<String>,
    /// A thread's major diameter (mm): the answer has its tapped hole's
    /// countersink too.
    #[serde(default)]
    major: Option<f64>,
    /// A tapped hole's thread (mitcad#980): the answer has its tapped
    /// hole's countersink (from the size's major diameter, unless `major`
    /// is given), the screw size of its nominal diameter and its bore.
    #[serde(default)]
    thread: Option<TappedThread>,
}

/// A thread as the `thread_sizes` query names it.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TappedThread {
    #[serde(default)]
    standard: ThreadStandard,
    size: String,
    /// `M6x1`: the bore (`thread_bore`) is its basic minor diameter.
    #[serde(default)]
    designation: Option<String>,
}

fn size_json(size: &ScrewSize) -> Value {
    let mut entry = json!({
        "size": size.size,
        "diameter": size.diameter,
        "clearance": {
            "fine": size.clearance_hole(ClearanceFit::Fine),
            "normal": size.clearance_hole(ClearanceFit::Normal),
            "coarse": size.clearance_hole(ClearanceFit::Coarse),
        },
    });
    if let Some((diameter, depth)) = size.counterbore {
        entry["counterbore"] = json!({"diameter": diameter, "depth": depth});
        // A counterdrill as the counterbore, its cone at 90 degrees.
        entry["counterdrill"] = json!({"diameter": diameter, "depth": depth,
                                       "angle": COUNTERSINK_ANGLE_DEGREES});
    }
    if let Some(diameter) = size.countersink {
        entry["countersink"] = json!({"diameter": diameter, "angle": COUNTERSINK_ANGLE_DEGREES});
    }
    if let Some(head) = size.iso_10642_head {
        entry["iso_10642_head"] = json!(head);
    }
    if let Some(sink) = size.iso_15065 {
        entry["iso_15065"] = json!(sink);
    }
    entry
}

pub(super) fn hole_sizes_json(query: &HoleSizesQuery) -> Result<Value, ApiError> {
    let sizes: Vec<Value> = match &query.size {
        Some(size) => vec![size_json(hole_sizes::lookup(size).map_err(ApiError)?)],
        None => hole_sizes::sizes().iter().map(size_json).collect(),
    };
    let fits: Vec<Value> = ClearanceFit::ALL
        .into_iter()
        .map(|fit| json!({"fit": fit, "title": fit.title()}))
        .collect();
    let [clearance, counterbore, head, countersink] = hole_sizes::sources();
    let mut answer = json!({
        "sizes": sizes,
        "fits": fits,
        "default_fit": ClearanceFit::default(),
        "default_size": "M6",
        "standards": {
            "clearance": clearance,
            "counterbore": counterbore,
            "countersink_head": head,
            "countersink": countersink,
        },
    });
    let mut major = query.major;
    if let Some(thread) = &query.thread {
        if let Some(size) = hole_sizes::of_thread(thread.standard, &thread.size) {
            answer["thread_size"] = size_json(size);
        }
        let designation = thread.designation.as_deref().unwrap_or_default();
        if let Some(bore) = hole_sizes::thread_bore(thread.standard, designation) {
            answer["thread_bore"] = json!(bore);
        }
        if major.is_none() {
            major = hole_sizes::thread_major(thread.standard, &thread.size);
        }
    }
    if let Some(major) = major {
        if !(major.is_finite() && major > 0.0) {
            return Err(ApiError(format!(
                "a thread's major diameter must be greater than zero, got {major}"
            )));
        }
        answer["tapped_countersink"] = json!({
            "diameter": hole_sizes::tapped_countersink(major),
            "angle": COUNTERSINK_ANGLE_DEGREES,
        });
    }
    Ok(answer)
}
