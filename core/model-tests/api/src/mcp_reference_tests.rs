// SPDX-License-Identifier: MIT
//! The command reference as MCP's agents see it (mitcad#1509), with the
//! names the backend permits: the sections they cannot use are left out,
//! and every permitted command, query and feature type keeps its rows or
//! its section (`mitcad_mcp::reference_losses`).

use mitcad_mcp::{INTERNAL_MARKER, Vocabulary, reference_losses, reference_text};
use mitcad_model_api::classes;

/// The names MCP permits, as the bridge gives them (`core/ffi/src/mcp.rs`).
fn permitted() -> Vocabulary {
    let owned =
        |names: &[&str]| -> Vec<String> { names.iter().map(|name| (*name).to_owned()).collect() };
    Vocabulary {
        queries: owned(&classes::external_queries()),
        commands: owned(&classes::external_commands()),
        features: owned(<mitcad_model::features::FeatureDef>::TYPE_NAMES),
    }
}

#[test]
fn every_permitted_name_keeps_its_reference_for_agents() {
    assert_eq!(reference_losses(&permitted()), Vec::<String>::new());
}

#[test]
fn the_agents_reference_leaves_out_what_they_cannot_use() {
    let agents = reference_text(&permitted());
    let whole = include_str!("../../../model-api/src/commands.md");
    let sections = |text: &str| {
        text.lines()
            .filter(|line| line.starts_with("## ") || line.starts_with("### "))
            .count()
    };
    // The sizes for the change's notes: `cargo test -- --nocapture`.
    println!(
        "the reference: {} bytes, {} sections; as agents see it: {} bytes, {} sections",
        whole.len(),
        sections(whole),
        agents.len(),
        sections(&agents),
    );
    assert!(agents.len() < whole.len());
    for heading in [
        "### Files",
        "### Version history",
        "## Remote repositories",
        "## Projects",
        "## Edit locks",
        "### Library commands",
        "### Suppliers' shops and product links",
        "## Fields of a newer MitCad's design",
        "### Linked components shown by their envelopes",
        "## Linked designs read again while open",
        "## Adding to the API",
        "## Recompute workers",
    ] {
        assert!(!agents.contains(&format!("\n{heading}")), "{heading}");
    }
    for heading in [
        "## Feature definitions",
        "## Sketch commands",
        "## Queries",
        "## Comments",
        "### The import's outcomes on the timeline",
        "## The model's items in drawing views",
        "## Intersection Curve",
    ] {
        assert!(agents.contains(&format!("\n{heading}\n")), "{heading}");
    }
    assert!(!agents.contains(INTERNAL_MARKER));
    assert!(!agents.contains("(mitcad#1416)\n"));
}
