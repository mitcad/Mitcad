// SPDX-License-Identifier: MIT
//! The live queries that run as model jobs (mitcad#2540). A live
//! `model_query` reads the window's document on its UI thread; the
//! analysis queries that check exact geometry across many bodies or many
//! positions can run for minutes (whole-design interference, an assembly
//! path of a body flush against its neighbours). The application runs
//! these on the model's worker as a job instead, as its own analyses do:
//! the window stays responsive, shows the progress with its Cancel, the
//! client's `notifications/cancelled` ends them, and other model requests
//! to the design are answered busy meanwhile. Their result is the same as
//! read inline. Quick reads stay inline: a job stops the background
//! evaluation while it runs.

use serde_json::Value;

/// The queries (their `query`) that run as model jobs.
pub const JOB_QUERIES: &[&str] = &[
    // Every pair of bodies, by exact booleans.
    "interference",
    // Clearances and overlaps at each position of a path (mitcad#1737).
    "assembly_path",
    // The overlaps of the joints' contact pairs.
    "contact_check",
    // The print and shape analyses of whole meshes (mitcad#620,
    // mitcad#1519, mitcad#2166, mitcad#2240, mitcad#2518), which the
    // application's own panels already run as jobs.
    "print_overhangs",
    "print_thickness",
    "print_min_gap",
    "draft_analysis",
    "point_cloud_deviation",
    "min_radius_analysis",
];

/// The name of a query that runs as a model job, None for one read inline
/// (and for a request that is not a query object).
pub fn job_query(request: &Value) -> Option<&str> {
    let name = request.get("query")?.as_str()?;
    JOB_QUERIES.contains(&name).then_some(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn long_analyses_run_as_jobs() {
        assert_eq!(
            job_query(&json!({"query": "interference", "min_volume": 1})),
            Some("interference")
        );
        assert_eq!(
            job_query(
                &json!({"query": "assembly_path", "body": "F2.b0", "against": ["F3.b0"],
                              "slide": {"direction": [0, 0, 1], "distance": 0.01}, "steps": 1})
            ),
            Some("assembly_path")
        );
        assert_eq!(
            job_query(&json!({"query": "contact_check"})),
            Some("contact_check")
        );
    }

    #[test]
    fn quick_reads_stay_inline() {
        for name in ["bodies", "timeline", "measure", "document", "properties"] {
            assert_eq!(job_query(&json!({"query": name})), None, "{name}");
        }
        assert_eq!(job_query(&json!({"query": 3})), None);
        assert_eq!(job_query(&json!(["interference"])), None);
        assert_eq!(job_query(&json!({})), None);
    }

    #[test]
    fn every_job_query_is_a_query_of_the_api() {
        for name in JOB_QUERIES {
            assert!(
                mitcad_model_api::classes::query_class(name).is_some(),
                "{name} is not a query"
            );
        }
    }
}
