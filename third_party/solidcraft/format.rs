// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 SolidCraft contributors
//
// Adapted from SolidCraft (https://github.com/tangency-labs/solidcraft, release
// v0.45.0, commit e8ce12ef78fe76281a04b7242a9ce059ff7d5067): the JSON
// upgrade steps of `crates/doc/src/format.rs` (`v2_to_v3`, `v3_to_v4`,
// `v4_to_v5`, `v5_to_v6`, `v13_to_v14` and their helpers), the upgrade path
// and the newer-file test of `crates/format/src/lib.rs` (`Formats::upgrade`,
// `Formats::parts`, `Family::newer`, `Family::canonical_applied_name`), the
// change files of `crates/doc/src/format/v23` to `v63` (their names, and the
// steps of `v28/two-sided-extrude-direction.rs`, `v29/face-pieces-by-place.rs`,
// `v35/inserted-file-features.rs`, `v40/feature-id-refs.rs`,
// `v49/join-to-body-apart.rs`, `v49/join-touching-bodies.rs`,
// `v49/thread-ends.rs`, `v49/thread-hand.rs`, `v50/thread-nominal-size.rs`,
// `v51/two-sided-extrude-fixed-sides.rs`, `v53/configuration-row-ids.rs`,
// `v53/hole-sketch-plane-start.rs`, `v56/pattern-names-by-source.rs`,
// `v57/extrude-to-object-keeps-picked-placements.rs`,
// `v57/sweep-walls-named-along-the-path.rs`,
// `v58/extrude-from-slanted-face.rs`, `v58/sketch-reference-placements.rs`,
// `v58/surface-sweep-exact-spline-path.rs`, `v62/sheet-edge-names.rs`,
// `v62/sheet-face-names.rs`, `v62/sheet-hem-gap.rs` and
// `v62/sheet-surface-convert.rs`, with their tests), the placement helpers
// of `crates/doc/src/assembly.rs` (`mat_mul`, `mat_inverse`,
// `transform_through`, `is_placement`), the helpers the format 58 sketch
// reference step uses (`face_anchor::plane_face_anchor`,
// `FaceAnchor::author_frame`, `EmbossFace::component_frame`,
// `Document::placement_frame`, `project::source_sketches`) and the
// feature-input naming of `crates/doc/src/params.rs`
// (`Document::name_all_feature_inputs`).
// Changes: the steps work on `serde_json::Value` and return
// `Result<(), String>`; the input count per feature comes from
// `crate::inputs` (Mitcad's port of the feature walk); the formats this
// import reads are written out here (`FOLDED`, `PENDING`) instead of being
// listed by a build script; a design a newer build saved is read with
// [`Newer`] said instead of refused. See README.md in this folder.

use serde_json::Value;

/// One change to the design format: a file of SolidCraft's
/// `crates/doc/src/format/v<N>/` or `pending/` folder, by its name (which a
/// design written while the change was pending lists in `format_pending`),
/// with the step that rewrites an older design as it opens, when it has
/// one.
pub struct Part {
    pub name: &'static str,
    pub step: Option<JsonStep>,
}

/// A change that needs no rewrite.
const fn known(name: &'static str) -> Part {
    Part { name, step: None }
}

/// A change whose step rewrites older designs.
const fn step(name: &'static str, step: JsonStep) -> Part {
    Part {
        name,
        step: Some(step),
    }
}

/// The last format whose upgrade step is a hand-written one.
const HAND_WRITTEN: u32 = 22;

/// The formats after [`HAND_WRITTEN`], each the changes it is made of, in
/// file-name order (SolidCraft's `v<N>/` folders).
pub const FOLDED: &[(u32, &[Part])] = &[
    (
        23,
        &[known("import-fcstd-3dm"), known("motion-link-coupling")],
    ),
    (24, &[known("edge-flange-corner-relief")]),
    (
        25,
        &[
            known("class-a-surfaces"),
            known("feature-pattern-alignment"),
            known("flipped-sketch-plane"),
            known("infinite-sketch-lines"),
            known("patch-continuity-and-points"),
            known("placed-plane-origins"),
            known("sketch-pattern-directions"),
            known("split-by-sketch-line"),
        ],
    ),
    (
        26,
        &[
            known("cam-couplings"),
            known("fillet-corner-and-loft-continuity"),
            known("hole-position-expressions"),
            known("mirror-placements"),
            known("sweep-guide-rails"),
        ],
    ),
    (
        27,
        &[
            known("boundary-surface-networks"),
            known("browser-folder-visibility"),
            known("cut-keeps-separate-pieces"),
            known("joint-origin-faces"),
            known("rib-normal-direction"),
            known("single-hole-sketch-point"),
        ],
    ),
    (
        28,
        &[
            known("component-metadata"),
            known("joint-expressions"),
            known("joint-origin-edge"),
            known("joint-origin-frame-x"),
            known("joint-origin-point-x"),
            known("joint-relations"),
            known("line-circle-distance"),
            known("loft-guide-rails"),
            known("sketch-point-expressions"),
            known("surface-blend-seam"),
            known("surface-trim-regions-curves-mutual"),
            step("two-sided-extrude-direction", two_sided_extrude_direction),
            known("vertex-edge-names"),
        ],
    ),
    (
        29,
        &[
            known("align-to-planes"),
            step("face-pieces-by-place", face_pieces_by_place),
            known("sheet-perforation-from-right"),
        ],
    ),
    (30, &[known("mirror-copies-join-component")]),
    (31, &[known("construction-axis-refs")]),
    (32, &[known("associative-motion-traces")]),
    (33, &[known("reserved-body-names")]),
    (34, &[known("sketch-circular-quantity-expression")]),
    (
        35,
        &[
            known("browser-view-state"),
            known("frame-members"),
            known("hole-reference-edges"),
            step("inserted-file-features", inserted_file_features),
            known("linked-face-names"),
            known("material-engineering-cards"),
            known("render-certified-nested-media"),
            known("render-cpu-budget"),
            known("render-cpu-progressive"),
            known("render-display-bloom"),
            known("render-finishing-linear-master"),
            known("render-hdr-resources"),
            known("render-homogeneous-volume"),
            known("render-independent-pilot-sampling"),
            known("render-job-intent"),
            known("render-layered-aging-photographic"),
            known("render-passive-stage"),
            known("render-portable-optical-bindings"),
            known("render-stable-aging-targets"),
            known("render-surface-bindings"),
            known("tapped-parallel-pipe-threads"),
            known("tapped-unified-threads"),
        ],
    ),
    (
        36,
        &[
            known("circle-edge-distances"),
            known("coil-profile-from-sketch"),
            known("plane-through-point"),
            known("recessed-hole-drill-point"),
            known("sheet-path-flange"),
            known("sheet-rule-angle-display"),
            known("tapped-hole-thread-depth"),
        ],
    ),
    (
        37,
        &[
            known("sheet-flange-bend-positions"),
            known("sheet-flange-height-from"),
            known("text-parameters"),
        ],
    ),
    (
        38,
        &[
            known("component-identification-colors"),
            known("joint-edge-axis"),
            known("sketch-pattern-expressions"),
        ],
    ),
    (
        39,
        &[
            known("axis-on-round-edges-and-faces"),
            known("large-mesh-blobs"),
            known("sketch-arc3d"),
            known("thread-handedness"),
        ],
    ),
    (
        40,
        &[
            known("cone-primitive"),
            step("feature-id-refs", feature_id_refs),
        ],
    ),
    (41, &[known("cylinder-reference-edges")]),
    (42, &[known("linked-body-properties")]),
    (43, &[known("point-clouds")]),
    (
        44,
        &[
            known("sketch-equation-curves"),
            known("timeline-joint-folders"),
        ],
    ),
    (
        45,
        &[
            known("browser-nested-groups"),
            known("point-cloud-properties"),
            known("print-faces"),
        ],
    ),
    (
        46,
        &[
            known("align-axis-mask"),
            known("equation-variable-unit"),
            known("link-file-digests"),
        ],
    ),
    (
        47,
        &[
            known("component-history-hidden"),
            known("face-anchor-placement"),
            known("plane-axis-point"),
        ],
    ),
    (
        48,
        &[
            known("circular-pattern-origin-expressions"),
            known("sheet-perforation-outline"),
            known("tangent-plane-reference"),
        ],
    ),
    (
        49,
        &[
            known("ball-patch-faces"),
            known("blend-tangent-chain"),
            known("continuity-g4"),
            known("draft-corner"),
            known("edge-nurbs-form"),
            known("face-nurbs-form"),
            step("join-to-body-apart", join_to_body_apart),
            step("join-touching-bodies", join_touching_bodies),
            known("pipe-threads"),
            known("repair-part"),
            known("ruled-far-handles"),
            known("spline-knots"),
            step("thread-ends", thread_ends),
            step("thread-hand", thread_hand),
            known("torus-tube-start"),
            known("wire-exact-curves"),
        ],
    ),
    (
        50,
        &[
            known("align-frames"),
            step("thread-nominal-size", thread_nominal_size),
        ],
    ),
    (
        51,
        &[
            known("bridged-counterbore"),
            known("canvas-lock"),
            known("cpu-output-presentation"),
            known("embedded-surface-textures"),
            known("fillet-turn"),
            known("glass-material-models"),
            known("hole-direction-own-faces"),
            known("intersection-curve-sketch-curves"),
            known("plane-at-angle-direction-only"),
            known("point-cloud-segments-and-triangles"),
            known("point-cloud-source"),
            known("revolve-axis-origin-expressions"),
            known("sketch-point-along"),
            known("transferred-profile-frame"),
            step(
                "two-sided-extrude-fixed-sides",
                two_sided_extrude_fixed_sides,
            ),
        ],
    ),
    (
        52,
        &[
            known("3d-wire-point-nodes"),
            known("component-origin-datums"),
            known("component-origin-visibility"),
            known("frame-end-caps"),
            known("joint-captured-positions"),
            known("joint-constraint-tag"),
            known("joint-origin-inside"),
            known("joint-plane-and-axis-snaps"),
            known("nested-face-name"),
            known("notes"),
            known("opus-quality"),
            known("render-indirect-clamp"),
            known("render-quality-controls"),
            known("revolve-axis-sketch"),
            known("sheet-flange-segments"),
            known("sketch-link-construction-point"),
            known("sketch-text-vertical-align"),
            known("sketch3d-profiles"),
            known("thread-continuous-flanks"),
            known("two-sided-cut-cap-names"),
        ],
    ),
    (
        53,
        &[
            step("configuration-row-ids", configuration_row_ids),
            step("hole-sketch-plane-start", hole_sketch_plane_start),
            known("sweep-path-on-body-edges"),
        ],
    ),
    (54, &[known("box-centred")]),
    (
        55,
        &[
            known("blend-definition-faces"),
            known("construct-snap-points"),
            known("plugin-data-namespace"),
            known("sketch-vertex-link-names"),
            known("solid-move-origin"),
            known("spline-tangent-handles"),
            known("split-face-loops"),
        ],
    ),
    (
        56,
        &[
            known("cell-grid-feature"),
            known("frame-member-locate"),
            known("frame-standard-sizes"),
            known("occurrence-move-expressions"),
            step("pattern-names-by-source", pattern_names_by_source),
        ],
    ),
    (
        57,
        &[
            known("cell-grid-keepout-clip"),
            known("cell-grid-margin"),
            known("cell-grid-snap-inset"),
            step(
                "extrude-to-object-keeps-picked-placements",
                extrude_to_object_keeps_picked_placements,
            ),
            known("hole-thread-class"),
            known("linked-configuration"),
            known("linked-cosmetic-threads"),
            known("move-copy-placements"),
            known("placement-expressions"),
            known("plane-at-angle-follows-named-axis"),
            known("saved-face-points"),
            step(
                "sweep-walls-named-along-the-path",
                sweep_walls_named_along_the_path,
            ),
            known("thread-multiple-faces"),
            known("thread-single-face-selection"),
        ],
    ),
    (
        58,
        &[
            known("bounding-solid-side-margins"),
            step("extrude-from-slanted-face", extrude_from_slanted_face),
            known("joint-lateral-offsets"),
            known("move-rotate-about-edge"),
            known("names-count-characters"),
            known("pattern-placements"),
            known("rib-extend-ends"),
            known("sketch-3d-constraints"),
            known("sketch-3d-dimensions"),
            known("sketch-3d-relations"),
            step(
                "sketch-reference-placements",
                sketch_reference_placements::step,
            ),
            known("split-planar-tool"),
            known("step-import-solid-items"),
            step(
                "surface-sweep-exact-spline-path",
                surface_sweep_exact_spline_path,
            ),
            known("whole-profile-holes-share-a-curve"),
        ],
    ),
    (
        59,
        &[
            known("bom-item-numbers"),
            known("bom-suppressed-parts"),
            known("drawings"),
            known("extrude-one-sided-direction"),
            known("named-view-roll-and-up"),
            known("occurrence-bom-type"),
            known("plane-at-angle-axis-origin-expressions"),
        ],
    ),
    (
        60,
        &[
            known("fillet-edge-radii"),
            known("import-records"),
            known("plugin-data-documents"),
        ],
    ),
    (61, &[known("import-record-copies")]),
    (
        62,
        &[
            known("import-record-paint"),
            known("rigid-groups"),
            step("sheet-edge-names", sheet_edge_names::step),
            step("sheet-face-names", sheet_face_names::step),
            step("sheet-hem-gap", sheet_hem_gap),
            known("sheet-recognize"),
            step("sheet-surface-convert", sheet_surface_convert),
        ],
    ),
    (
        63,
        &[
            known("bounding-solid-contents"),
            known("canvas-anchor"),
            known("closed-control-splines"),
            known("freeform-chain-offsets"),
            known("import-record-meshes"),
            known("link-inherited-metadata"),
            known("projected-curve-edge-names"),
            known("sheet-rule-material"),
            known("sweep-path-across-sketches"),
        ],
    ),
];

/// The changes no release of the pinned commit has a number for yet
/// (SolidCraft's `pending/` folder), in file-name order: they make the
/// format after the last of [`FOLDED`].
pub const PENDING: &[Part] = &[];

/// The format this import reads: the last of [`FOLDED`], or the next one
/// while changes are pending.
pub const PINNED: u32 = HAND_WRITTEN + FOLDED.len() as u32 + if PENDING.is_empty() { 0 } else { 1 };

/// The JSON steps up to [`HAND_WRITTEN`]: entry `i` turns format `i + 1`
/// into `i + 2`.
pub type JsonStep = fn(&mut Value) -> Result<(), String>;
const JSON_STEPS: [JsonStep; HAND_WRITTEN as usize - 1] = [
    |_| Ok(()),
    v2_to_v3,
    v3_to_v4,
    v4_to_v5,
    v5_to_v6,
    |_| Ok(()),
    |_| Ok(()),
    |_| Ok(()),
    |_| Ok(()),
    |_| Ok(()),
    |_| Ok(()),
    |_| Ok(()),
    v13_to_v14,
    |_| Ok(()),
    |_| Ok(()),
    |_| Ok(()),
    |_| Ok(()),
    |_| Ok(()),
    |_| Ok(()),
    |_| Ok(()),
    |_| Ok(()),
];

/// The changes format `version` is made of, if it is made of changes.
fn parts(version: u32) -> Option<&'static [Part]> {
    let after = version.checked_sub(HAND_WRITTEN + 1)? as usize;
    match FOLDED.get(after) {
        Some((_, parts)) => Some(parts),
        None if after == FOLDED.len() && !PENDING.is_empty() => Some(PENDING),
        None => None,
    }
}

/// Whether a change of any format is known here: a design's pending change
/// may have landed at a later number.
fn knows(name: &str) -> bool {
    FOLDED
        .iter()
        .flat_map(|(_, parts)| *parts)
        .chain(PENDING)
        .any(|p| p.name == name)
}

/// The name a change listed in a design's `format_pending` has here: a
/// format-29 build of SolidCraft wrote `physical-material-cards` for
/// `material-engineering-cards`.
fn canonical(version: u32, name: &str) -> &str {
    if version == 29 && name == "physical-material-cards" && knows("material-engineering-cards") {
        "material-engineering-cards"
    } else {
        name
    }
}

/// What says that a newer build of SolidCraft saved a design: a format
/// number past [`PINNED`], or changes (`format_pending`) this import does
/// not know (SolidCraft's `solidcraft_format::Newer`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Newer {
    /// The file's format.
    pub format: u32,
    /// The format this import reads ([`PINNED`]).
    pub reads_up_to: u32,
    /// The changes the file lists that this import does not know.
    pub unknown_changes: Vec<String>,
}

/// Whether a newer build saved a design of format `version` with the
/// changes `applied` (its `format_pending`), and what says so
/// (SolidCraft's `Family::newer`). Such a design is read as far as this
/// import reads it, not refused: its known changes' steps run, and a format
/// past [`PINNED`] has no steps to run.
pub fn newer(version: u32, applied: Option<&[String]>) -> Option<Newer> {
    let listed = applied
        .filter(|_| version > PINNED || parts(version).is_some())
        .unwrap_or_default();
    let unknown_changes: Vec<String> = listed
        .iter()
        .filter(|name| !knows(canonical(version, name)))
        .cloned()
        .collect();
    (version > PINNED || !unknown_changes.is_empty()).then_some(Newer {
        format: version,
        reads_up_to: PINNED,
        unknown_changes,
    })
}

/// Refuses only what is not a design: format 0 (SolidCraft's
/// `Family::take_version`). A newer format number, or a pending change this
/// import does not know, is read ([`newer`] says so).
fn check(version: u32) -> Result<(), String> {
    if version == 0 {
        return Err("not a SolidCraft design (format 0)".to_owned());
    }
    Ok(())
}

/// Brings a design of format `version` up to [`PINNED`] and gives the steps
/// that ran, by name (`2 to 3` for the hand-written ones, a change's name
/// for the others): for the import's report. `applied` is the design's
/// `format_pending`: the changes of its own format it already has, when it
/// was written while they were pending; the other changes of that format
/// run first. A design of a released format has no such list and has every
/// change of its format (a released format 28 is not upgraded to 28 again).
/// A change already applied never runs again at a later number. A design
/// past [`PINNED`] has no steps to run, and the changes a design lists that
/// this import does not know have none either ([`newer`]).
pub fn upgrade(
    v: &mut Value,
    version: u32,
    applied: Option<&[String]>,
) -> Result<Vec<String>, String> {
    check(version)?;
    let applied = applied.filter(|_| parts(version).is_some());
    let already = |p: &Part| {
        applied.is_some_and(|names| names.iter().any(|n| canonical(version, n) == p.name))
    };
    let mut ran = Vec::new();
    let mut run = |changes: &[Part], v: &mut Value| -> Result<(), String> {
        for p in changes.iter().filter(|p| !already(p)) {
            if let Some(step) = p.step {
                step(v)?;
                ran.push(p.name.to_owned());
            }
        }
        Ok(())
    };
    if applied.is_some() {
        run(parts(version).unwrap_or_default(), v)?;
    }
    let mut hand = Vec::new();
    for from in version..PINNED {
        match JSON_STEPS.get(from as usize - 1) {
            Some(step) => {
                step(v)?;
                hand.push(format!("{from} to {}", from + 1));
            }
            None => run(parts(from + 1).unwrap_or_default(), v)?,
        }
    }
    hand.append(&mut ran);
    Ok(hand)
}

/// The mark of a face or edge name saved before format 14.
pub const LEGACY_MARK: char = '?';

/// Stored face and edge names were worked out from geometry before format
/// 14; they get the legacy mark (`?`). Every feature also keeps naming the
/// pieces it splits a body into in the kernel's shell order.
fn v13_to_v14(v: &mut Value) -> Result<(), String> {
    if let Some(Value::Array(features)) = v.get_mut("features") {
        for f in features.iter_mut() {
            let Value::Object(o) = f else { continue };
            let kind = o.get("type").and_then(Value::as_str).map(str::to_string);
            if matches!(kind.as_deref(), Some("fillet" | "chamfer")) {
                mark_all(o.get_mut("edge_names"));
            }
            mark_all(o.get_mut("face_names"));
            if kind.as_deref() == Some("sketch")
                && let Some(Value::Array(links)) =
                    o.get_mut("sketch").and_then(|s| s.get_mut("links"))
            {
                links
                    .iter_mut()
                    .filter_map(|l| l.get_mut("face_name"))
                    .for_each(mark);
            }
            o.insert("shell_piece_order".into(), Value::Bool(true));
            references(f);
        }
    }
    if let Some(Value::Array(joints)) = v.get_mut("assembly").and_then(|a| a.get_mut("joints")) {
        for j in joints.iter_mut() {
            for side in ["a", "b"] {
                snap(j.get_mut(side));
            }
        }
    }
    if let Some(Value::Array(origins)) = v.get_mut("assembly").and_then(|a| a.get_mut("origins")) {
        origins.iter_mut().for_each(|o| snap(o.get_mut("origin")));
    }
    if let Some(Value::Array(faces)) = v.get_mut("appearances").and_then(|a| a.get_mut("faces")) {
        for face in faces
            .iter_mut()
            .filter(|f| f.get("strict_name").and_then(Value::as_bool) != Some(true))
        {
            if let Some(n) = face.get_mut("name") {
                mark(n);
            }
        }
    }
    Ok(())
}

/// Mark the `name` of every face or edge reference in `v`.
fn references(v: &mut Value) {
    match v {
        Value::Object(o) => {
            let reference = matches!(o.get("type").and_then(Value::as_str), Some("face" | "edge"))
                && o.contains_key("at");
            for (key, x) in o.iter_mut() {
                if reference && key == "name" {
                    mark(x);
                } else {
                    references(x);
                }
            }
        }
        Value::Array(a) => a.iter_mut().for_each(references),
        _ => {}
    }
}

/// Mark the face name of a joint origin's snap on a face.
fn snap(origin: Option<&mut Value>) {
    if let Some(snap) = origin.and_then(|o| o.get_mut("snap"))
        && matches!(
            snap.get("type").and_then(Value::as_str),
            Some("face_center" | "circle_center")
        )
        && let Some(n) = snap.get_mut("face")
    {
        mark(n);
    }
}

fn mark_all(names: Option<&mut Value>) {
    if let Some(Value::Array(names)) = names {
        names.iter_mut().for_each(mark);
    }
}

/// A stored name with the legacy mark (an empty name stays empty).
fn mark(n: &mut Value) {
    if let Value::String(s) = n
        && !s.is_empty()
        && !s.starts_with(LEGACY_MARK)
    {
        s.insert(0, LEGACY_MARK);
    }
}

/// Old XZ references keep their exact frame: they become explicit planes
/// with the axes they had (x = +X, y = +Z).
fn v5_to_v6(v: &mut Value) -> Result<(), String> {
    fn preserve(v: &mut Value) {
        match v {
            Value::Object(o) => {
                let old_xz = o.get("type").and_then(Value::as_str) == Some("origin")
                    && o.get("name").and_then(Value::as_str).is_some_and(|n| {
                        n.eq_ignore_ascii_case("XZ") || n.eq_ignore_ascii_case("FRONT")
                    });
                if old_xz {
                    *v = serde_json::json!({"type": "custom", "plane": {"origin": [0, 0, 0], "x_dir": [1, 0, 0], "y_dir": [0, 0, 1]}});
                } else {
                    o.values_mut().for_each(preserve);
                }
            }
            Value::Array(a) => a.iter_mut().for_each(preserve),
            _ => {}
        }
    }
    preserve(v);
    Ok(())
}

/// Features in designs with components keep reaching every body.
fn v4_to_v5(v: &mut Value) -> Result<(), String> {
    let has_components = v
        .get("components")
        .and_then(Value::as_array)
        .is_some_and(|c| !c.is_empty());
    if !has_components {
        return Ok(());
    }
    if let Some(Value::Array(fs)) = v.get_mut("features") {
        for f in fs.iter_mut() {
            if let Some(o) = f.as_object_mut()
                && o.get("type").and_then(Value::as_str) != Some("sketch")
                && o.get("participants").is_none()
            {
                o.insert("participants".into(), serde_json::json!(["*"]));
            }
        }
    }
    Ok(())
}

/// Joints placed with the old convention (aligned unless flipped) keep
/// their placement.
fn v3_to_v4(v: &mut Value) -> Result<(), String> {
    if let Some(Value::Array(js)) = v.get_mut("assembly").and_then(|a| a.get_mut("joints")) {
        for j in js.iter_mut() {
            if let Some(o) = j.as_object_mut() {
                let f = o.get("flip").and_then(Value::as_bool).unwrap_or(false);
                o.insert("flip".into(), Value::Bool(!f));
            }
        }
    }
    Ok(())
}

/// Body colours `{name: [r, g, b]}` become named, opaque body appearances.
fn v2_to_v3(v: &mut Value) -> Result<(), String> {
    let Some(Value::Object(old)) = v.get("appearances") else {
        return Ok(());
    };
    if !old.values().all(Value::is_array) {
        return Ok(());
    }
    let bodies: serde_json::Map<String, Value> = old
        .iter()
        .map(|(k, c)| {
            (
                k.clone(),
                serde_json::json!({"name": "Custom", "color": c, "opacity": 1.0}),
            )
        })
        .collect();
    if let Some(o) = v.as_object_mut() {
        o.insert(
            "appearances".into(),
            serde_json::json!({ "bodies": bodies }),
        );
    }
    Ok(())
}

/// A two-sided extrude follows its `direction`; older builds ignored a
/// `negative` direction beside a second distance, so such extrudes become
/// `positive`, which keeps their shape.
fn two_sided_extrude_direction(v: &mut Value) -> Result<(), String> {
    let Some(Value::Array(features)) = v.get_mut("features") else {
        return Ok(());
    };
    for feature in features {
        if !matches!(
            feature.get("type").and_then(Value::as_str),
            Some("extrude" | "surface_extrude")
        ) {
            continue;
        }
        if let Some(extent) = feature.get_mut("extent").and_then(Value::as_object_mut)
            && extent.get("distance2").is_some_and(|d| !d.is_null())
            && extent.get("direction").and_then(Value::as_str) == Some("negative")
        {
            extent.insert("direction".into(), Value::from("positive"));
        }
    }
    Ok(())
}

/// The pieces of a split face are numbered by place (format 29); an older
/// design's stored `#k` names mean the kernel's order, which every feature
/// of it keeps (`kernel_piece_order`).
fn face_pieces_by_place(v: &mut Value) -> Result<(), String> {
    let Some(Value::Array(features)) = v.get_mut("features") else {
        return Ok(());
    };
    for feature in features {
        if let Value::Object(o) = feature {
            o.insert("kernel_piece_order".into(), Value::Bool(true));
        }
    }
    Ok(())
}

/// The feature of a file inserted into a component (an import or a mesh
/// import) is marked `inserted` (format 35), except a copied component's
/// independent base.
fn inserted_file_features(v: &mut Value) -> Result<(), String> {
    let Some(Value::Array(features)) = v.get_mut("features") else {
        return Ok(());
    };
    for feature in features {
        let Some(object) = feature.as_object_mut() else {
            continue;
        };
        let in_component = object
            .get("component")
            .and_then(Value::as_u64)
            .is_some_and(|c| c != 0);
        let inserted = match object.get("type").and_then(Value::as_str) {
            Some("mesh_import") => true,
            Some("import") => {
                let base = object
                    .get("file")
                    .and_then(Value::as_str)
                    .is_some_and(|f| f.ends_with(" independent base"))
                    && object
                        .get("components")
                        .and_then(Value::as_array)
                        .is_none_or(Vec::is_empty);
                !base
            }
            _ => false,
        };
        if in_component && inserted {
            object.insert("inserted".into(), Value::Bool(true));
        }
    }
    Ok(())
}

/// A design's features as `(id, name, type)`, for resolving names the way
/// SolidCraft's `replay()` did (format 40).
type Features<'a> = &'a [(u64, String, String)];

/// `Document::find_feature` at the JSON level: an id written as a number,
/// then a case-insensitive name match.
fn find_feature(feats: Features, name: &str) -> Option<u64> {
    name.parse::<u64>()
        .ok()
        .and_then(|id| feats.iter().find(|(i, ..)| *i == id))
        .or_else(|| feats.iter().find(|(_, n, _)| n.eq_ignore_ascii_case(name)))
        .map(|(i, ..)| *i)
}

/// The feature a pattern or mirror seed named: an exact name match on a
/// feature that is not a sketch or a construction plane, else
/// [`find_feature`]; nothing is id 0, which dangles as the name did.
fn resolve_seed(feats: Features, name: &str) -> u64 {
    feats
        .iter()
        .find(|(_, n, t)| n == name && !matches!(t.as_str(), "sketch" | "construction_plane"))
        .map(|(i, ..)| *i)
        .or_else(|| find_feature(feats, name))
        .unwrap_or(0)
}

/// Rewrites every `{"type": "construction", "name": …}` plane reference in
/// `v` to the feature's id. `skip_axis` leaves a revolve's `axis` alone: a
/// construction axis has the same shape and is still named.
fn rewrite_planes(v: &mut Value, feats: Features, skip_axis: bool) {
    match v {
        Value::Object(o) => {
            if o.get("type").and_then(Value::as_str) == Some("construction") {
                if let Some(Value::String(name)) = o.get("name") {
                    let id = find_feature(feats, name).unwrap_or(0);
                    o.remove("name");
                    o.insert("feature".into(), Value::from(id));
                }
                return;
            }
            for (key, x) in o.iter_mut() {
                if !(skip_axis && key == "axis") {
                    rewrite_planes(x, feats, false);
                }
            }
        }
        Value::Array(a) => a.iter_mut().for_each(|x| rewrite_planes(x, feats, false)),
        _ => {}
    }
}

/// Pattern and mirror seeds and construction plane references are feature
/// ids (format 40): an older design's names are resolved once as it opens.
fn feature_id_refs(v: &mut Value) -> Result<(), String> {
    let feats: Vec<(u64, String, String)> = v
        .get("features")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_object)
        .filter_map(|f| {
            let id = f.get("id")?.as_u64()?;
            let name = f.get("name")?.as_str()?.to_string();
            let kind = f.get("type")?.as_str()?.to_string();
            Some((id, name, kind))
        })
        .collect();
    if let Some(Value::Array(features)) = v.get_mut("features") {
        for f in features.iter_mut() {
            let kind = f
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            if matches!(kind.as_str(), "pattern" | "mirror")
                && let Some(Value::Array(seeds)) = f.get_mut("features")
            {
                for s in seeds.iter_mut() {
                    if let Value::String(name) = s {
                        let id = resolve_seed(&feats, name);
                        *s = Value::from(id);
                    }
                }
            }
            let skip_axis = matches!(kind.as_str(), "revolve" | "surface_revolve");
            rewrite_planes(f, &feats, skip_axis);
        }
    }
    if let Some(o) = v.as_object_mut() {
        for (key, x) in o.iter_mut() {
            if key != "features" {
                rewrite_planes(x, &feats, false);
            }
        }
    }
    Ok(())
}

/// The first format whose Join extrudes To Object also joined the body they
/// run to.
const JOINS_TO_BODY: u32 = 40;

/// A Join extrude To Object of a design saved before format 40 keeps the
/// body it runs to apart (`to_body_apart`, format 49).
fn join_to_body_apart(v: &mut Value) -> Result<(), String> {
    // The file's own format is still in it: upgrades never rewrite
    // `format`, and a file without one is format 1.
    let version = match v.get("format").and_then(Value::as_str) {
        Some(format) => crate::version_of(format),
        None => Some(1),
    };
    if version.is_none_or(|n| n >= JOINS_TO_BODY) {
        return Ok(());
    }
    for feature in v
        .get_mut("features")
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
    {
        let join_extrude = feature.get("type").and_then(Value::as_str) == Some("extrude")
            && feature.get("operation").and_then(Value::as_str) == Some("join");
        if !join_extrude {
            continue;
        }
        if let Some(extent) = feature.get_mut("extent").and_then(Value::as_object_mut)
            && extent.get("to").is_some_and(|to| !to.is_null())
        {
            extent.insert("to_body_apart".into(), Value::Bool(true));
        }
    }
    Ok(())
}

/// Feature types that join without an `operation` saying so.
const JOINING: [&str; 8] = [
    "combine", "pattern", "mirror", "emboss", "rib", "boss", "snap_fit", "rest",
];

/// Every feature of an older design that can join keeps all its pieces in
/// one body (`touching: one_body`, format 49), as older builds did.
fn join_touching_bodies(v: &mut Value) -> Result<(), String> {
    let Some(Value::Array(features)) = v.get_mut("features") else {
        return Ok(());
    };
    for feature in features {
        let joins = feature
            .get("type")
            .and_then(Value::as_str)
            .is_some_and(|t| JOINING.contains(&t))
            || feature.get("operation").and_then(Value::as_str) == Some("join");
        if joins && let Some(f) = feature.as_object_mut() {
            f.insert("touching".into(), Value::from("one_body"));
        }
    }
    Ok(())
}

/// A thread saved while its ends were being made (format 49): `tip:
/// chamfer` becomes `both`, and an `ease_in` is dropped.
fn thread_ends(v: &mut Value) -> Result<(), String> {
    let Some(Value::Array(features)) = v.get_mut("features") else {
        return Ok(());
    };
    for feature in features {
        let Some(object) = feature.as_object_mut() else {
            continue;
        };
        if object.get("type").and_then(Value::as_str) != Some("thread") {
            continue;
        }
        if object.get("tip").and_then(Value::as_str) == Some("chamfer") {
            object.insert("tip".into(), Value::String("both".into()));
        }
        object.remove("ease_in");
    }
    Ok(())
}

/// Format 39's `handedness` of holes and threads is `hand` (format 49): a
/// left one stays left, a right one is the default.
fn thread_hand(v: &mut Value) -> Result<(), String> {
    let Some(Value::Array(features)) = v.get_mut("features") else {
        return Ok(());
    };
    for feature in features {
        let threaded = matches!(
            feature.get("type").and_then(Value::as_str),
            Some("hole" | "thread")
        );
        let Some(f) = feature.as_object_mut().filter(|_| threaded) else {
            continue;
        };
        match f.remove("handedness") {
            None => {}
            Some(Value::String(h)) if h == "right" => {}
            Some(Value::String(h)) if h == "left" => {
                f.insert("hand".into(), Value::from("left"));
            }
            Some(other) => {
                return Err(format!(
                    "a thread's `handedness` must be right or left, not {other}"
                ));
            }
        }
    }
    Ok(())
}

/// A thread of an older design without a `designation` keeps the size its
/// hole's tap drill gave it (`old_auto_size`, format 50).
fn thread_nominal_size(v: &mut Value) -> Result<(), String> {
    let Some(Value::Array(features)) = v.get_mut("features") else {
        return Ok(());
    };
    for feature in features {
        let Some(object) = feature.as_object_mut() else {
            continue;
        };
        if object.get("type").and_then(Value::as_str) != Some("thread") {
            continue;
        }
        if object.get("designation").is_none_or(Value::is_null) {
            object.insert("old_auto_size".into(), Value::Bool(true));
        }
    }
    Ok(())
}

/// A two-sided extrude with entered distances keeps its older reading
/// (format 51): the second side follows the first across the sketch plane
/// (`second_follows_first`). An extent to an object or through all already
/// lets it follow, and a symmetric one has no second side.
fn two_sided_extrude_fixed_sides(v: &mut Value) -> Result<(), String> {
    let Some(Value::Array(features)) = v.get_mut("features") else {
        return Ok(());
    };
    for feature in features {
        if !matches!(
            feature.get("type").and_then(Value::as_str),
            Some("extrude" | "surface_extrude")
        ) {
            continue;
        }
        let Some(extent) = feature.get_mut("extent").and_then(Value::as_object_mut) else {
            continue;
        };
        let plain = extent.get("distance2").is_some_and(|d| !d.is_null())
            && extent.get("to").is_none_or(Value::is_null)
            && !extent
                .get("through_all")
                .and_then(Value::as_bool)
                .unwrap_or(false)
            && extent.get("direction").and_then(Value::as_str) != Some("symmetric");
        if plain {
            extent.insert("second_follows_first".into(), Value::Bool(true));
        }
    }
    Ok(())
}

/// Configuration rows get fixed ids as an older design opens (format 53):
/// in row order from the design's `next_id`, past any id a row has, and
/// `next_id` moves past them.
fn configuration_row_ids(v: &mut Value) -> Result<(), String> {
    let saved = v.get("next_id").and_then(Value::as_u64).unwrap_or(0);
    let Some(rows) = v.pointer_mut("/configs/rows").and_then(Value::as_array_mut) else {
        return Ok(());
    };
    let past = |id: u64| {
        id.checked_add(1)
            .ok_or_else(|| "damaged design: the id space is exhausted".to_owned())
    };
    let mut next = saved.max(1);
    if let Some(largest) = rows.iter().filter_map(|row| row.get("id")?.as_u64()).max() {
        next = next.max(past(largest)?);
    }
    let first = next;
    for row in rows.iter_mut().filter_map(Value::as_object_mut) {
        if !row.contains_key("id") {
            row.insert("id".into(), next.into());
            next = past(next)?;
        }
    }
    if next != first
        && let Some(design) = v.as_object_mut()
    {
        design.insert("next_id".into(), next.into());
    }
    Ok(())
}

/// A hole at sketch points (not a Single hole) of an older design keeps
/// starting at the body's surface (`old_surface_start`, format 53), where a
/// newer one starts at the sketch's plane when that lies inside a body.
fn hole_sketch_plane_start(v: &mut Value) -> Result<(), String> {
    let Some(Value::Array(features)) = v.get_mut("features") else {
        return Ok(());
    };
    for feature in features {
        if feature.get("type").and_then(Value::as_str) != Some("hole") {
            continue;
        }
        let at_points = feature
            .get("points")
            .and_then(Value::as_object)
            .is_some_and(|points| {
                !points
                    .get("single")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
            });
        if let (true, Some(object)) = (at_points, feature.as_object_mut()) {
            object.insert("old_surface_start".into(), Value::Bool(true));
        }
    }
    Ok(())
}

/// A body Pattern of several bodies with stored copy names keeps reading
/// them in the order the copies were made (`pattern_names_in_order`,
/// format 56).
fn pattern_names_by_source(v: &mut Value) -> Result<(), String> {
    let Some(Value::Array(features)) = v.get_mut("features") else {
        return Ok(());
    };
    for feature in features {
        let Value::Object(o) = feature else { continue };
        let several = o.get("type").and_then(Value::as_str) == Some("pattern")
            && o.get("bodies")
                .and_then(Value::as_array)
                .is_some_and(|bodies| bodies.len() > 1)
            && o.get("body_names")
                .and_then(Value::as_array)
                .is_some_and(|names| !names.is_empty());
        if several {
            o.insert("pattern_names_in_order".into(), Value::Bool(true));
        }
    }
    Ok(())
}

/// A 4×4 transform as SolidCraft stores it (`m[col][row]`).
type Mat = [[f64; 4]; 4];

const IDENTITY: Mat = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
];

/// `a` after `b` (SolidCraft's `assembly::mat_mul`).
fn mat_mul(a: &Mat, b: &Mat) -> Mat {
    let mut r = [[0.0; 4]; 4];
    for (c, col) in r.iter_mut().enumerate() {
        for (row, cell) in col.iter_mut().enumerate() {
            *cell = (0..4).map(|k| a[k][row] * b[c][k]).sum();
        }
    }
    r
}

/// The inverse of an affine transform (SolidCraft's
/// `assembly::mat_inverse`).
fn mat_inverse(m: &Mat) -> Option<Mat> {
    let a = |r: usize, c: usize| m[c][r];
    let det = a(0, 0) * (a(1, 1) * a(2, 2) - a(1, 2) * a(2, 1))
        - a(0, 1) * (a(1, 0) * a(2, 2) - a(1, 2) * a(2, 0))
        + a(0, 2) * (a(1, 0) * a(2, 1) - a(1, 1) * a(2, 0));
    if det.abs() < 1e-14 || !det.is_finite() {
        return None;
    }
    let inv = |r: usize, c: usize| -> f64 {
        // Cofactor of (c, r) / det.
        let (r0, r1) = match c {
            0 => (1, 2),
            1 => (0, 2),
            _ => (0, 1),
        };
        let (c0, c1) = match r {
            0 => (1, 2),
            1 => (0, 2),
            _ => (0, 1),
        };
        let minor = a(r0, c0) * a(r1, c1) - a(r0, c1) * a(r1, c0);
        let sign = if (r + c).is_multiple_of(2) { 1.0 } else { -1.0 };
        sign * minor / det
    };
    let mut out = IDENTITY;
    for (c, col) in out.iter_mut().enumerate().take(3) {
        for (r, cell) in col.iter_mut().enumerate().take(3) {
            *cell = inv(r, c);
        }
    }
    let t = [m[3][0], m[3][1], m[3][2]];
    let it: [f64; 3] =
        std::array::from_fn(|i| out[0][i] * t[0] + out[1][i] * t[1] + out[2][i] * t[2]);
    out[3] = [-it[0], -it[1], -it[2], 1.0];
    Some(out)
}

/// An occurrence as the step reads it: what places a component.
#[derive(serde::Deserialize)]
struct Placing {
    id: u64,
    component: u64,
    #[serde(default)]
    parent: u64,
    #[serde(default = "identity")]
    transform: Mat,
}

fn identity() -> Mat {
    IDENTITY
}

/// A component's world placement through its first occurrence and its
/// parents' (SolidCraft's `assembly::transform_through`).
fn transform_through(component: u64, occurrences: &[Placing]) -> Mat {
    let mut m = IDENTITY;
    let mut c = component;
    for _ in 0..1000 {
        if c == 0 {
            break;
        }
        let Some(o) = occurrences.iter().find(|o| o.component == c) else {
            break;
        };
        m = mat_mul(&o.transform, &m);
        c = o.parent;
    }
    m
}

/// An extrude that ends To Object or starts From Object of an older design
/// gets the placements the file holds as those it measures against
/// (`picked_placements`, format 57): where each other component stands
/// relative to the extrude's own.
fn extrude_to_object_keeps_picked_placements(v: &mut Value) -> Result<(), String> {
    let mut occurrences: Vec<Placing> = match v.get("occurrences").cloned() {
        None | Some(Value::Null) => Vec::new(),
        Some(value) => match serde_json::from_value(value) {
            Ok(occurrences) => occurrences,
            Err(_) => return Ok(()),
        },
    };
    // A component an older file never placed is placed at the identity as
    // the file opens, so that is where it stands.
    let missing: Vec<(u64, u64)> = v
        .get("components")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|c| {
            Some((
                c.get("id")?.as_u64()?,
                c.get("parent").and_then(Value::as_u64).unwrap_or(0),
            ))
        })
        .filter(|(id, _)| *id != 0 && !occurrences.iter().any(|o| o.component == *id))
        .collect();
    for (component, parent) in missing {
        let id = occurrences.iter().map(|o| o.id).max().unwrap_or(0) + 1;
        occurrences.push(Placing {
            id,
            component,
            parent,
            transform: IDENTITY,
        });
    }
    if occurrences.is_empty() {
        return Ok(());
    }
    let mut components: Vec<u64> = occurrences.iter().map(|o| o.component).collect();
    components.push(0);
    components.sort_unstable();
    components.dedup();
    let worlds: Vec<(u64, Mat)> = components
        .iter()
        .map(|c| (*c, transform_through(*c, &occurrences)))
        .collect();
    let world_of = |component: u64| {
        worlds
            .iter()
            .find(|(c, _)| *c == component)
            .map(|(_, m)| *m)
            .unwrap_or_else(|| transform_through(component, &occurrences))
    };
    for feature in v
        .get_mut("features")
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
    {
        if feature.get("type").and_then(Value::as_str) != Some("extrude") {
            continue;
        }
        let own = feature
            .get("component")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let Some(inverse) = mat_inverse(&world_of(own)) else {
            continue;
        };
        let Some(extent) = feature.get_mut("extent").and_then(Value::as_object_mut) else {
            continue;
        };
        let picks = ["to", "from"]
            .iter()
            .any(|key| extent.get(*key).is_some_and(|p| !p.is_null()));
        if !picks || extent.contains_key("picked_placements") {
            continue;
        }
        let pins: Vec<Value> = worlds
            .iter()
            .filter(|(c, _)| *c != own)
            .map(|(c, m)| serde_json::json!({"component": c, "placement": mat_mul(&inverse, m)}))
            .collect();
        extent.insert("picked_placements".into(), Value::Array(pins));
    }
    Ok(())
}

/// A sketch link to a vertex, an edge or a face of a sweep along several
/// curves drops a saved name built on one of its walls (format 57): the
/// link is found by its point; a vertex or an edge link is marked
/// `name_dropped`. A link a sketch's plane was placed by (`auto_face`)
/// keeps its face name.
fn sweep_walls_named_along_the_path(v: &mut Value) -> Result<(), String> {
    let Some(Value::Array(features)) = v.get_mut("features") else {
        return Ok(());
    };
    // The sweeps along more than one curve, by id.
    let sweeps: Vec<u64> = features
        .iter()
        .filter(|f| f.get("type").and_then(Value::as_str) == Some("sweep"))
        .filter(|f| {
            ["path", "path_edges"].iter().any(|k| {
                f.get(*k)
                    .and_then(Value::as_array)
                    .is_some_and(|p| p.len() > 1)
            })
        })
        .filter_map(|f| f.get("id").and_then(Value::as_u64))
        .collect();
    if sweeps.is_empty() {
        return Ok(());
    }
    for feature in features.iter_mut() {
        let Some(Value::Array(links)) = feature.pointer_mut("/sketch/links") else {
            continue;
        };
        for link in links.iter_mut() {
            let Value::Object(link) = link else { continue };
            let placed = link.get("auto_face").and_then(Value::as_bool) == Some(true);
            for key in ["vertex_name", "edge_name", "face_name"] {
                if key == "face_name" && placed {
                    continue;
                }
                if link
                    .get(key)
                    .and_then(Value::as_str)
                    .is_some_and(|name| sweeps.iter().any(|id| names_face_of(name, *id)))
                {
                    link.remove(key);
                    if key != "face_name" {
                        link.insert("name_dropped".into(), Value::Bool(true));
                    }
                }
            }
        }
    }
    Ok(())
}

/// Whether the persistent name `name` holds a face of feature `id`
/// (`F<id>:`).
fn names_face_of(name: &str, id: u64) -> bool {
    let tag = format!("F{id}:");
    name.match_indices(&tag).any(|(at, _)| {
        !name
            .get(..at)
            .and_then(|before| before.chars().next_back())
            .is_some_and(|c| c.is_ascii_alphanumeric())
    })
}

/// An extrude From Object on a flat face at a slant to the extrusion
/// starts on that face since format 58; every saved extrude with a From
/// Object is marked `from_level`, which keeps the older reading: a plane
/// parallel to the sketch at the level where the face meets the middle of
/// the profiles.
fn extrude_from_slanted_face(v: &mut Value) -> Result<(), String> {
    let Some(Value::Array(features)) = v.get_mut("features") else {
        return Ok(());
    };
    for feature in features {
        if feature.get("type").and_then(Value::as_str) != Some("extrude") {
            continue;
        }
        let Some(extent) = feature.get_mut("extent").and_then(Value::as_object_mut) else {
            continue;
        };
        if extent.get("from").is_some_and(|p| !p.is_null()) {
            extent.insert("from_level".into(), Value::Bool(true));
        }
    }
    Ok(())
}

/// A surface sweep along a 3D sketch spline follows the spline itself
/// since format 58 (one wall along it); every saved surface sweep whose
/// path lists a 3D sketch curve is marked `spline_as_samples`, which keeps
/// the older walls along the lines between the spline's samples.
fn surface_sweep_exact_spline_path(v: &mut Value) -> Result<(), String> {
    let Some(Value::Array(features)) = v.get_mut("features") else {
        return Ok(());
    };
    // Each sketch's 3D curve ids, by the sketch feature's id.
    let wires: Vec<(u64, Vec<String>)> = features
        .iter()
        .filter_map(|f| {
            let id = f.get("id")?.as_u64()?;
            let ids = f
                .get("sketch")?
                .get("wires")?
                .as_array()?
                .iter()
                .filter_map(|w| w.get("id")?.as_str().map(str::to_owned))
                .collect();
            Some((id, ids))
        })
        .collect();
    for feature in features.iter_mut() {
        if feature.get("type").and_then(Value::as_str) != Some("surface_sweep") {
            continue;
        }
        let Some(sketch) = feature.get("path_sketch").and_then(Value::as_u64) else {
            continue;
        };
        let Some((_, ids)) = wires.iter().find(|(id, _)| *id == sketch) else {
            continue;
        };
        let on_wire = feature
            .get("path")
            .and_then(Value::as_array)
            .is_some_and(|p| {
                p.iter()
                    .any(|c| c.as_str().is_some_and(|c| ids.iter().any(|w| w == c)))
            });
        if on_wire && let Some(f) = feature.as_object_mut() {
            f.insert("spline_as_samples".into(), Value::Bool(true));
        }
    }
    Ok(())
}

/// A hem's `gap` and a sheet metal rule's `hem_gap` are the space between
/// the hem's leg and the face it folds over since format 62 (the bend's
/// inner radius is half of it); older designs read the value as the
/// radius itself, so each saved value is doubled (`2 * (…)`), and a rule
/// saved without `hem_gap` gets twice the old default.
fn sheet_hem_gap(v: &mut Value) -> Result<(), String> {
    // The default `hem_gap` a rule had when it was the inner radius,
    // doubled.
    const OLD_DEFAULT_DOUBLED: &str = "0.02 mm";
    /// `expr` read as twice itself.
    fn doubled(expr: &str) -> String {
        format!("2 * ({expr})")
    }
    for feature in v
        .get_mut("features")
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
    {
        if feature.get("type").and_then(Value::as_str) != Some("sheet_hem") {
            continue;
        }
        if let Some(Value::String(gap)) = feature.get_mut("gap") {
            *gap = doubled(gap);
        }
    }
    for rule in v
        .pointer_mut("/sheet/rules")
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
    {
        let Some(rule) = rule.as_object_mut() else {
            continue;
        };
        match rule.get_mut("hem_gap") {
            Some(Value::String(gap)) => *gap = doubled(gap),
            _ => {
                rule.insert("hem_gap".into(), Value::String(OLD_DEFAULT_DOUBLED.into()));
            }
        }
    }
    Ok(())
}

/// A conversion to sheet metal without bend edges (and without the faces,
/// engravings or side only format 62 saves) converted a plate as one sheet
/// from the face it picked: since format 62 it is Recognize of that plate
/// from that face under its rule (`sheet_recognize`). One with bend edges
/// stays a conversion.
fn sheet_surface_convert(v: &mut Value) -> Result<(), String> {
    for feature in v
        .get_mut("features")
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
    {
        let Some(f) = feature.as_object_mut() else {
            continue;
        };
        if f.get("type").and_then(Value::as_str) != Some("sheet_convert") {
            continue;
        }
        let listed = |key: &str| {
            f.get(key)
                .and_then(Value::as_array)
                .is_some_and(|l| !l.is_empty())
        };
        let side = f
            .get("direction")
            .and_then(Value::as_str)
            .is_some_and(|d| !d.is_empty());
        if listed("bends") || listed("faces") || listed("engrave") || side {
            continue;
        }
        f.remove("bends");
        f.insert("type".into(), Value::String("sheet_recognize".into()));
    }
    Ok(())
}

/// Whether `m` is a placement an occurrence may have (SolidCraft's
/// `assembly::is_placement`): finite, a rotation (orthonormal axes, not
/// mirrored) and a translation within 1e9 mm, its last column `0, 0, 0, 1`.
fn is_placement(m: &Mat) -> bool {
    let axis = |i: usize| [m[i][0], m[i][1], m[i][2]];
    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let cross = |a: [f64; 3], b: [f64; 3]| {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    };
    let (x, y, z) = (axis(0), axis(1), axis(2));
    m.iter().flatten().all(|v| v.is_finite())
        && m[3][..3].iter().all(|v| v.abs() <= 1e9)
        && [x, y, z].iter().all(|v| (dot(*v, *v) - 1.0).abs() < 1e-6)
        && dot(x, y).abs() < 1e-6
        && dot(x, z).abs() < 1e-6
        && dot(y, z).abs() < 1e-6
        && dot(cross(x, y), z) > 0.0
        && [m[0][3], m[1][3], m[2][3]].iter().all(|v| v.abs() < 1e-9)
        && (m[3][3] - 1.0).abs() < 1e-9
}

/// Format 58: sketch links save `sketch_placements` (the source sketches'
/// placements relative to the sketch's own component) and a sketch's
/// construction plane of another component saves its relative
/// `placement`. A design saved before captures them as it opens, keeping
/// the geometry it was built with; root and same-component references stay
/// live (SolidCraft's `v58/sketch-reference-placements.rs`).
mod sketch_reference_placements {
    use std::collections::{BTreeMap, BTreeSet};

    use serde_json::{Value, json};

    use super::{IDENTITY, Mat, Placing, is_placement, mat_inverse, mat_mul, transform_through};

    /// The most sketches one link reads (`project::MAX_SKETCH_PLACEMENTS`).
    const MAX_SKETCH_PLACEMENTS: usize = 1_000;

    /// The assembly as the step reads it: the occurrences, with one at the
    /// identity for each component the file never placed.
    struct Context {
        occurrences: Vec<Placing>,
    }

    impl Context {
        fn component_transform(&self, component: u64) -> Mat {
            transform_through(component, &self.occurrences)
        }

        /// The world frame of one occurrence path from the root
        /// (`Document::placement_frame`).
        fn placement_frame(&self, path: &[u64]) -> Result<Mat, String> {
            if path.len() > 64 || self.occurrences.len() > 100_000 {
                return Err("the placement path exceeds work limits".to_owned());
            }
            let mut frame = IDENTITY;
            let mut component = 0;
            let mut seen = BTreeSet::new();
            for id in path {
                if *id == 0 || !seen.insert(*id) {
                    return Err("invalid placement path".to_owned());
                }
                let occurrence = self
                    .occurrences
                    .iter()
                    .find(|o| o.id == *id && o.parent == component)
                    .ok_or("the placement no longer exists")?;
                frame = mat_mul(&frame, &occurrence.transform);
                component = occurrence.component;
            }
            Ok(frame)
        }

        /// The frame of component `owner` through `path`
        /// (`EmbossFace::component_frame`).
        fn component_frame(&self, owner: u64, path: &[u64]) -> Result<Mat, String> {
            let frame = self.placement_frame(path)?;
            match path.split_last() {
                None => Ok(self.component_transform(owner)),
                Some((last, _)) => {
                    let component = self
                        .occurrences
                        .iter()
                        .find(|o| o.id == *last)
                        .map(|o| o.component);
                    if component == Some(owner) {
                        Ok(frame)
                    } else {
                        Err("the path belongs to another component".to_owned())
                    }
                }
            }
        }
    }

    /// The face anchor of a sketch plane, also beneath an offset, a plane
    /// through a point, an angle or a flip (`face_anchor::plane_face_anchor`):
    /// its `author_path`, when the anchor reads as one.
    fn anchor_author_path(plane: &Value) -> Option<Vec<u64>> {
        let mut at = plane;
        for _ in 0..=64 {
            match at.get("type").and_then(Value::as_str)? {
                "face" => {
                    let anchor = at.get("anchor").filter(|a| !a.is_null())?;
                    // An anchor reads with its paths (`FaceAnchor`).
                    anchor.get("path")?.as_array()?;
                    return anchor
                        .get("author_path")?
                        .as_array()?
                        .iter()
                        .map(Value::as_u64)
                        .collect();
                }
                "offset" | "through_point" | "at_angle" | "flipped" => at = at.get("base")?,
                _ => return None,
            }
        }
        None
    }

    /// The sketches a link's source reads, the two sides of an
    /// intersection too (`project::source_sketches`). None for a source
    /// that does not read as one.
    fn source_sketches(source: &Value) -> Result<Option<BTreeSet<u64>>, String> {
        let mut sketches = BTreeSet::new();
        let mut pending = vec![(source, 0usize)];
        let mut work = 0;
        while let Some((source, depth)) = pending.pop() {
            work += 1;
            if depth > 64 || work > MAX_SKETCH_PLACEMENTS {
                return Err("too many nested sketch projection references".to_owned());
            }
            let text = |key: &str| source.get(key).and_then(Value::as_str);
            match source.get("type").and_then(Value::as_str) {
                Some("sketch_curve" | "sketch_point" | "on_surface") => {
                    let named = text("curve").or_else(|| text("point"));
                    match (source.get("sketch").and_then(Value::as_u64), named) {
                        (Some(sketch), Some(_)) => {
                            sketches.insert(sketch);
                        }
                        _ => return Ok(None),
                    }
                }
                Some("intersection") => match (source.get("a"), source.get("b")) {
                    (Some(a), Some(b)) => pending.extend([(a, depth + 1), (b, depth + 1)]),
                    _ => return Ok(None),
                },
                Some(_) => {}
                None => return Ok(None),
            }
        }
        Ok(Some(sketches))
    }

    fn relative(context: &Context, inverse: &Mat, owner: u64) -> Result<Mat, String> {
        let placement = mat_mul(inverse, &context.component_transform(owner));
        if !is_placement(&placement) {
            return Err("captured sketch reference placement is invalid".to_owned());
        }
        Ok(placement)
    }

    fn capture_plane(
        plane: &mut Value,
        author: u64,
        inverse: &Mat,
        context: &Context,
        owners: &BTreeMap<u64, u64>,
        depth: usize,
    ) -> Result<(), String> {
        if depth > 64 {
            return Err("sketch plane reference is too deep".to_owned());
        }
        if plane.get("type").and_then(Value::as_str) == Some("construction")
            && plane.get("placement").is_none_or(Value::is_null)
            && let Some(owner) = plane
                .get("feature")
                .and_then(Value::as_u64)
                .and_then(|id| owners.get(&id))
                .filter(|owner| **owner != author)
        {
            let placement = relative(context, inverse, *owner)?;
            if let Some(plane) = plane.as_object_mut() {
                plane.insert("placement".into(), json!(placement));
            }
        }
        if let Some(base) = plane.get_mut("base") {
            capture_plane(base, author, inverse, context, owners, depth + 1)?;
        }
        Ok(())
    }

    pub(super) fn step(value: &mut Value) -> Result<(), String> {
        // Only the assembly's frames are needed; a feature this reader
        // does not know does not keep the known sketches from it.
        let read = |key: &str| value.get(key).cloned().unwrap_or_else(|| json!([]));
        let Ok(components) = serde_json::from_value::<Vec<Value>>(read("components")) else {
            return Ok(());
        };
        let Ok(mut occurrences) = serde_json::from_value::<Vec<Placing>>(read("occurrences"))
        else {
            return Ok(());
        };
        let missing: Vec<(u64, u64)> = components
            .iter()
            .filter_map(|c| {
                Some((
                    c.get("id")?.as_u64()?,
                    c.get("parent").and_then(Value::as_u64).unwrap_or(0),
                ))
            })
            .filter(|(id, _)| *id != 0 && !occurrences.iter().any(|o| o.component == *id))
            .collect();
        for (component, parent) in missing {
            let id = occurrences
                .iter()
                .map(|o| o.id)
                .max()
                .unwrap_or(0)
                .checked_add(1)
                .ok_or("too many items in the design")?;
            occurrences.push(Placing {
                id,
                component,
                parent,
                transform: IDENTITY,
            });
        }
        let context = Context { occurrences };
        let owners: BTreeMap<u64, u64> = value
            .get("features")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|feature| {
                Some((
                    feature.get("id")?.as_u64()?,
                    feature
                        .get("component")
                        .and_then(Value::as_u64)
                        .unwrap_or(0),
                ))
            })
            .collect();
        for feature in value
            .get_mut("features")
            .and_then(Value::as_array_mut)
            .into_iter()
            .flatten()
        {
            if feature.get("type").and_then(Value::as_str) != Some("sketch") {
                continue;
            }
            let author = feature
                .get("component")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            if author == 0 {
                continue;
            }
            let frame = match feature.get("plane").and_then(anchor_author_path) {
                // A dangling attachment stays the feature's error, not a
                // reason to refuse the file (`FaceAnchor::author_frame`).
                Some(path) if path.is_empty() => continue,
                Some(path) => match context.component_frame(author, &path) {
                    Ok(frame) => frame,
                    Err(_) => continue,
                },
                None => context.component_transform(author),
            };
            let inverse = mat_inverse(&frame).ok_or("projection author frame is invalid")?;
            if let Some(plane) = feature.get_mut("plane") {
                capture_plane(plane, author, &inverse, &context, &owners, 0)?;
            }
            for link in feature
                .get_mut("sketch")
                .and_then(|s| s.get_mut("links"))
                .and_then(Value::as_array_mut)
                .into_iter()
                .flatten()
            {
                if link.get("sketch_placements").is_some() {
                    continue;
                }
                let Some(source) = link.get("source") else {
                    continue;
                };
                let Some(sketches) = source_sketches(source)? else {
                    continue;
                };
                let mut pins = Vec::new();
                for sketch in sketches {
                    let Some(owner) = owners.get(&sketch).filter(|owner| **owner != author) else {
                        continue;
                    };
                    pins.push(json!({"sketch": sketch, "placement": relative(&context, &inverse, *owner)?}));
                }
                if !pins.is_empty()
                    && let Some(link) = link.as_object_mut()
                {
                    link.insert("sketch_placements".into(), Value::Array(pins));
                }
            }
        }
        Ok(())
    }
}

/// Format 62: a flange's or a hem's picked edges are saved by the model's
/// names for them; an older design's sheet names (`base:<line>:<side>`,
/// `F<f>.<k>:<edge>:<side>`) of an edge the design says what made become
/// the model's names (SolidCraft's `v62/sheet-edge-names.rs`, whose
/// comment says which).
mod sheet_edge_names {
    use serde_json::Value;

    /// The features that make a sheet.
    const MAKERS: [&str; 5] = [
        "sheet_base",
        "sheet_contour",
        "sheet_convert",
        "sheet_recognize",
        "sheet_surface_convert",
    ];

    /// The features that make a sheet of a body they convert.
    const CONVERSIONS: [&str; 3] = ["sheet_convert", "sheet_recognize", "sheet_surface_convert"];

    /// How deep a flange stands on flanges before it, at most, whose names
    /// are followed.
    const DEPTH: usize = 64;

    /// A sketch line: its id and its ends.
    type Line = (String, [f64; 2], [f64; 2]);

    fn kind(f: &Value) -> Option<&str> {
        f.get("type").and_then(Value::as_str)
    }

    fn id(f: &Value) -> Option<u64> {
        f.get("id").and_then(Value::as_u64)
    }

    /// Whether feature `f` may copy a body, and a sheet with it.
    fn copies(f: &Value) -> bool {
        matches!(kind(f), Some("pattern" | "mirror"))
            || (kind(f) == Some("move") && f.get("copy").and_then(Value::as_bool) == Some(true))
    }

    /// The base flange whose sheet the edges feature `at` picked lie on, as
    /// the saved design says.
    fn base_of(features: &[Value], at: usize) -> Option<&Value> {
        let f = features.get(at)?;
        let base = |x: &Value| kind(x) == Some("sheet_base");
        if let Some(body) = f
            .get("body")
            .and_then(Value::as_str)
            .filter(|b| !b.is_empty())
        {
            let keeps = |x: &&Value| {
                x.get("body_names")
                    .and_then(Value::as_array)
                    .is_some_and(|n| n.iter().any(|n| n.as_str() == Some(body)))
            };
            let converts = |x: &Value| {
                kind(x).is_some_and(|k| CONVERSIONS.contains(&k))
                    && x.get("body").and_then(Value::as_str) == Some(body)
            };
            let kept: Vec<&Value> = features.iter().filter(keeps).collect();
            if let [one] = kept.as_slice()
                && base(one)
                && !features.iter().any(converts)
            {
                return Some(*one);
            }
        }
        let before = features.get(..at)?;
        let makers: Vec<&Value> = before
            .iter()
            .filter(|x| kind(x).is_some_and(|k| MAKERS.contains(&k)))
            .collect();
        match makers.as_slice() {
            [one] if base(one) && !before.iter().any(copies) => Some(*one),
            _ => None,
        }
    }

    /// The lines of sketch feature `sketch` (not its construction lines):
    /// each one's id and its ends as the sketch saved them.
    fn lines(features: &[Value], sketch: u64) -> Vec<Line> {
        let Some(s) = features
            .iter()
            .find(|f| kind(f) == Some("sketch") && id(f) == Some(sketch))
            .and_then(|f| f.get("sketch"))
        else {
            return Vec::new();
        };
        let points = s
            .get("points")
            .and_then(Value::as_array)
            .map_or(&[][..], Vec::as_slice);
        let at = |c: &Value, end: &str| -> Option<[f64; 2]> {
            let pos = points
                .get(usize::try_from(c.get(end)?.as_u64()?).ok()?)?
                .get("pos")?;
            Some([pos.get(0)?.as_f64()?, pos.get(1)?.as_f64()?])
        };
        s.get("curves")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter(|c| {
                kind(c) == Some("line")
                    && c.get("construction").and_then(Value::as_bool) != Some(true)
            })
            .filter_map(|c| Some((c.get("id")?.as_str()?.to_owned(), at(c, "a")?, at(c, "b")?)))
            .collect()
    }

    /// Whether the sketch line `line` is one of `lines` and no other lies in
    /// line with it (within 1e-6 mm).
    fn alone(lines: &[Line], line: &str) -> bool {
        let Some((_, a, b)) = lines.iter().find(|(id, _, _)| id == line) else {
            return false;
        };
        let off = |p: [f64; 2], s: [f64; 2], e: [f64; 2]| {
            let (tx, ty) = (e[0] - s[0], e[1] - s[1]);
            ((p[0] - s[0]) * ty - (p[1] - s[1]) * tx).abs() / tx.hypot(ty)
        };
        let along = |p: [f64; 2], q: [f64; 2], s: [f64; 2], e: [f64; 2]| {
            off(p, s, e) < 1e-6 && off(q, s, e) < 1e-6
        };
        (b[0] - a[0]).hypot(b[1] - a[1]) > 1e-9
            && lines
                .iter()
                .filter(|(id, _, _)| id != line)
                .all(|(_, p, q)| !along(*p, *q, *a, *b) && !along(*a, *b, *p, *q))
    }

    /// The sheet of base flange `base` made from its sketch's `lines`, with
    /// the design's `features` that bent it.
    struct Sheet<'a> {
        features: &'a [Value],
        base: u64,
        lines: Vec<Line>,
    }

    impl Sheet<'_> {
        /// The model's names for the two faces that meet at the edge the
        /// sheet named `name`, as the feature at `at` saved it: the rim
        /// along it and its panel's cap on its side.
        fn faces(&self, name: &str, at: usize, depth: usize) -> Option<(String, String)> {
            let (edge, side) = name.rsplit_once(':')?;
            if !matches!(side, "top" | "bottom") || depth > DEPTH {
                return None;
            }
            let b = self.base;
            if let Some(line) = edge.strip_prefix("base:") {
                return alone(&self.lines, line)
                    .then(|| (format!("F{b}:side:{line}"), format!("F{b}:{side}")));
            }
            let (tag, rim) = edge.split_once(':')?;
            let (f, k) = tag.strip_prefix('F')?.split_once('.')?;
            let digits = |s: &str| !s.is_empty() && s.bytes().all(|c| c.is_ascii_digit());
            if !digits(f) || !digits(k) {
                return None;
            }
            let (f, k): (u64, usize) = (f.parse().ok()?, k.parse().ok()?);
            // The flange's end rims are named from its bend's anchor,
            // which runs the edge the other way.
            let rim = match rim {
                "far" => "far",
                "side0" => "end1",
                "side1" => "end0",
                _ => return None,
            };
            let made = self
                .features
                .get(..at)?
                .iter()
                .position(|x| id(x) == Some(f))?;
            let feature = self.features.get(made)?;
            if base_of(self.features, made).is_some_and(|x| id(x) != Some(b)) {
                return None;
            }
            let edges = feature.get("edges").and_then(Value::as_array)?;
            if k >= edges.len() {
                return None;
            }
            let whole = |e: &Value| {
                e.get("segments")
                    .and_then(Value::as_array)
                    .is_none_or(Vec::is_empty)
            };
            let one_each = match kind(feature) {
                Some("sheet_hem") => true,
                Some("sheet_flange") => edges.iter().take(k + 1).all(whole),
                _ => false,
            };
            if !one_each {
                return None;
            }
            let picked = feature.get("edge_names")?.get(k)?.as_str()?;
            let (on, _) = self.faces(picked, made, depth + 1)?;
            Some((format!("F{f}:{rim}:{on}"), format!("F{f}:{side}:{on}")))
        }

        /// The model's name for the edge the sheet named `name`.
        fn renamed(&self, name: &str, at: usize) -> Option<String> {
            let (rim, cap) = self.faces(name, at, 0)?;
            let mut two = [rim, cap];
            two.sort();
            Some(two.join("|"))
        }
    }

    pub(super) fn step(v: &mut Value) -> Result<(), String> {
        let Some(features) = v.get("features").and_then(Value::as_array) else {
            return Ok(());
        };
        // Every name is read as the design saved it before any is
        // rewritten.
        let mut renamed: Vec<(usize, usize, String)> = Vec::new();
        for (at, f) in features.iter().enumerate() {
            if !matches!(kind(f), Some("sheet_flange" | "sheet_hem")) {
                continue;
            }
            let Some(names) = f.get("edge_names").and_then(Value::as_array) else {
                continue;
            };
            let Some(base) = base_of(features, at) else {
                continue;
            };
            let (Some(b), Some(sketch)) = (id(base), base.get("sketch").and_then(Value::as_u64))
            else {
                continue;
            };
            let sheet = Sheet {
                features,
                base: b,
                lines: lines(features, sketch),
            };
            for (k, name) in names.iter().enumerate() {
                if let Some(new) = name.as_str().and_then(|n| sheet.renamed(n, at)) {
                    renamed.push((at, k, new));
                }
            }
        }
        for (at, k, new) in renamed {
            if let Some(slot) = v.pointer_mut(&format!("/features/{at}/edge_names/{k}")) {
                *slot = Value::String(new);
            }
        }
        Ok(())
    }
}

/// Format 62: a sheet's faces are named by the model; an older design's
/// sheet names `sheet:<body>:base:<side>` of a face the design says what
/// made become the model's names (SolidCraft's `v62/sheet-face-names.rs`).
mod sheet_face_names {
    use std::collections::BTreeMap;

    use serde_json::Value;

    /// What made the panel `base` of a sheet body: a base flange (its id
    /// and its sketch's line ids), or a conversion with bend edges (its id
    /// and the model's name for the face it fixed).
    enum Base {
        Flange { feature: u64, lines: Vec<String> },
        Converted { feature: u64, face: String },
    }

    fn kind(f: &Value) -> Option<&str> {
        f.get("type").and_then(Value::as_str)
    }

    fn id(f: &Value) -> Option<u64> {
        f.get("id").and_then(Value::as_u64)
    }

    /// The ids of the lines of sketch feature `sketch` (construction lines
    /// name no rim).
    fn lines(features: &[Value], sketch: u64) -> Vec<String> {
        let curves = features
            .iter()
            .find(|f| kind(f) == Some("sketch") && id(f) == Some(sketch))
            .and_then(|f| f.pointer("/sketch/curves"));
        curves
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter(|c| {
                c.get("type").and_then(Value::as_str) == Some("line")
                    && c.get("construction").and_then(Value::as_bool) != Some(true)
            })
            .filter_map(|c| c.get("id").and_then(Value::as_str).map(str::to_owned))
            .collect()
    }

    /// What made the panel `base` of each body the design says it of.
    fn bases(v: &Value) -> BTreeMap<String, Base> {
        let features = v
            .get("features")
            .and_then(Value::as_array)
            .map_or(&[][..], Vec::as_slice);
        let mut keepers: BTreeMap<&str, Vec<&Value>> = BTreeMap::new();
        let mut conversions: BTreeMap<&str, Vec<&Value>> = BTreeMap::new();
        for f in features {
            for name in f
                .get("body_names")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
            {
                keepers.entry(name).or_default().push(f);
            }
            if kind(f) == Some("sheet_convert")
                && let Some(body) = f.get("body").and_then(Value::as_str)
            {
                conversions.entry(body).or_default().push(f);
            }
        }
        let mut out = BTreeMap::new();
        for (body, kept) in &keepers {
            if let ([f], None) = (kept.as_slice(), conversions.get(body))
                && kind(f) == Some("sheet_base")
                && let (Some(feature), Some(sketch)) =
                    (id(f), f.get("sketch").and_then(Value::as_u64))
            {
                out.insert(
                    (*body).to_owned(),
                    Base::Flange {
                        feature,
                        lines: lines(features, sketch),
                    },
                );
            }
        }
        for (body, converted) in &conversions {
            let flange = keepers
                .get(body)
                .is_some_and(|kept| kept.iter().any(|f| kind(f) == Some("sheet_base")));
            if let [f] = converted.as_slice()
                && !flange
                && f.get("bends")
                    .and_then(Value::as_array)
                    .is_some_and(|b| !b.is_empty())
                && let (Some(feature), Some(face)) =
                    (id(f), f.pointer("/face_names/0").and_then(Value::as_str))
                && !face.is_empty()
                && !face.starts_with('?')
            {
                out.insert(
                    (*body).to_owned(),
                    Base::Converted {
                        feature,
                        face: face.to_owned(),
                    },
                );
            }
        }
        out
    }

    /// The body, panel and side of a sheet's own name for a face,
    /// `sheet:<body>:<panel>:<side>`.
    fn name_parts(name: &str) -> Option<(&str, &str, &str)> {
        let rest = name.strip_prefix("sheet:")?;
        let (head, side) = rest.rsplit_once(':')?;
        let (body, panel) = head.rsplit_once(':')?;
        (!body.is_empty() && !panel.is_empty() && !side.is_empty()).then_some((body, panel, side))
    }

    /// The model's name for the face the sheet's own name `name` names.
    fn renamed(name: &str, bases: &BTreeMap<String, Base>) -> Option<String> {
        let (body, "base", side) = name_parts(name)? else {
            return None;
        };
        match bases.get(body)? {
            Base::Flange { feature, lines } => match side.strip_prefix("rim.") {
                None if matches!(side, "top" | "bottom") => Some(format!("F{feature}:{side}")),
                Some(line) if lines.iter().any(|l| l == line) => {
                    Some(format!("F{feature}:side:{line}"))
                }
                _ => None,
            },
            Base::Converted { feature, face } => match side {
                "bottom" => Some(face.clone()),
                "top" => Some(format!("F{feature}:bottom:{face}")),
                _ => None,
            },
        }
    }

    fn rename_in(n: &mut Value, rename: &dyn Fn(&str) -> Option<String>) {
        if let Value::String(s) = n
            && let Some(r) = rename(s.as_str())
        {
            *s = r;
        }
    }

    /// Each name of a face in `v` through `rename`: a feature's
    /// `face_names`, and the `name` of every face reference (`"type":
    /// "face"` with `at`).
    fn faces(v: &mut Value, rename: &dyn Fn(&str) -> Option<String>) {
        match v {
            Value::Object(o) => {
                let reference =
                    o.get("type").and_then(Value::as_str) == Some("face") && o.contains_key("at");
                for (key, x) in o.iter_mut() {
                    if reference && key == "name" {
                        rename_in(x, rename);
                    } else if key == "face_names"
                        && let Value::Array(names) = x
                    {
                        names.iter_mut().for_each(|n| rename_in(n, rename));
                    } else {
                        faces(x, rename);
                    }
                }
            }
            Value::Array(a) => a.iter_mut().for_each(|x| faces(x, rename)),
            _ => {}
        }
    }

    pub(super) fn step(v: &mut Value) -> Result<(), String> {
        let made = bases(v);
        if !made.is_empty() {
            faces(v, &|name| renamed(name, &made));
        }
        Ok(())
    }
}

/// Gives every feature input without a parameter name the next free `d<n>`
/// (`Document::name_all_feature_inputs`): names of designs written before
/// feature inputs had names, as SolidCraft fills them in when it opens
/// them. `count` gives a feature's number of inputs.
pub fn name_all_feature_inputs(v: &mut Value, count: &dyn Fn(&Value) -> usize) {
    use std::collections::BTreeMap;
    let mut taken: BTreeMap<String, usize> = BTreeMap::new();
    let params = v
        .get("params")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for p in &params {
        if let Some(n) = p.get("name").and_then(Value::as_str) {
            *taken.entry(n.to_owned()).or_default() += 1;
        }
    }
    let Some(Value::Array(features)) = v.get_mut("features") else {
        return;
    };
    for f in features.iter() {
        for n in f
            .get("param_names")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if let Some(n) = n.as_str() {
                *taken.entry(n.to_owned()).or_default() += 1;
            }
        }
    }
    let mut next = 1usize;
    for f in features.iter_mut() {
        let n = count(f);
        let mut names: Vec<String> = f
            .get("param_names")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .map(|x| x.as_str().unwrap_or_default().to_owned())
                    .collect()
            })
            .unwrap_or_default();
        for k in 0..n {
            if names.get(k).is_some_and(|name| !name.is_empty()) {
                continue;
            }
            let name = loop {
                let name = format!("d{next}");
                next += 1;
                if !taken.contains_key(&name) {
                    break name;
                }
            };
            if names.len() <= k {
                names.resize(k + 1, String::new());
            }
            names[k] = name.clone();
            taken.insert(name, 1);
        }
        for name in names.drain(n.min(names.len())..) {
            if let Some(count) = taken.get_mut(&name) {
                *count -= 1;
                if *count == 0 {
                    taken.remove(&name);
                    if let Some(i) = name.strip_prefix('d').and_then(|s| s.parse::<usize>().ok())
                        && name == format!("d{i}")
                        && i > 0
                    {
                        next = next.min(i);
                    }
                }
            }
        }
        if let Some(o) = f.as_object_mut() {
            if names.is_empty() {
                o.remove("param_names");
            } else {
                o.insert("param_names".into(), Value::from(names));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Every format after the hand-written ones is listed, in order, and the
    /// pinned one follows from them.
    #[test]
    fn the_formats_follow_on() {
        for (k, (n, parts)) in FOLDED.iter().enumerate() {
            assert_eq!(*n, HAND_WRITTEN + 1 + k as u32);
            assert!(!parts.is_empty(), "{n}");
            assert!(parts.windows(2).all(|w| w[0].name < w[1].name), "{n}");
        }
        assert_eq!(PINNED, 63);
        assert!(parts(22).is_none());
        assert_eq!(parts(28).map(<[Part]>::len), Some(13));
        // SolidCraft 0.43.0's folders: 65 changes in formats 51 to 57.
        let later: usize = (51..=57).filter_map(parts).map(<[Part]>::len).sum();
        assert_eq!(later, 65);
        // SolidCraft 0.45.0's folders: 42 changes in formats 58 to 63,
        // seven of them with a step.
        let newest: Vec<&Part> = (58..=63).filter_map(parts).flatten().collect();
        assert_eq!(newest.len(), 42);
        assert_eq!(newest.iter().filter(|p| p.step.is_some()).count(), 7);
        assert!(parts(PINNED + 1).is_none());
    }

    #[test]
    fn saved_extrudes_from_an_object_keep_starting_at_its_level() {
        let mut v = json!({"features": [
            {"type": "extrude", "extent": {"distance": "5", "from": [1.0, 2.0, 3.0]}},
            {"type": "extrude", "extent": {"distance": "5", "from": null}},
            {"type": "extrude", "extent": {"distance": "5", "to": [1.0, 2.0, 3.0]}},
            {"type": "revolve", "extent": {"from": [1.0, 2.0, 3.0]}}
        ]});
        extrude_from_slanted_face(&mut v).unwrap();
        let marked: Vec<bool> = v["features"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| f["extent"]["from_level"] == json!(true))
            .collect();
        assert_eq!(marked, [true, false, false, false]);
        let mut empty = json!({"name": "Untitled"});
        extrude_from_slanted_face(&mut empty).unwrap();
        assert_eq!(empty, json!({"name": "Untitled"}));
    }

    #[test]
    fn saved_surface_sweeps_along_3d_curves_keep_their_sampled_walls() {
        let mut v = json!({"features": [
            {"id": 2, "type": "sketch", "sketch": {"wires": [{"id": "w1"}]}},
            {"id": 3, "type": "sketch", "sketch": {"curves": [{"id": "l1"}]}},
            {"id": 4, "type": "surface_sweep", "path_sketch": 2, "path": ["w1"]},
            {"id": 5, "type": "surface_sweep", "path_sketch": 3, "path": ["l1"]},
            {"id": 6, "type": "surface_sweep", "path_body": "Body1", "path_edges": [[0.0, 0.0, 0.0]]},
            {"id": 7, "type": "sweep", "path_sketch": 2, "path": ["w1"]}
        ]});
        surface_sweep_exact_spline_path(&mut v).unwrap();
        let marked: Vec<bool> = v["features"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| f["spline_as_samples"] == json!(true))
            .collect();
        assert_eq!(marked, [false, false, true, false, false, false]);
        let mut empty = json!({"name": "Untitled"});
        surface_sweep_exact_spline_path(&mut empty).unwrap();
        assert_eq!(empty, json!({"name": "Untitled"}));
    }

    #[test]
    fn saved_hem_gaps_double_and_a_rule_without_one_gets_twice_the_old_default() {
        let mut v = json!({
            "features": [
                {"type": "sheet_hem", "edges": [[0, 0, 0]], "length": "10 mm", "gap": "0.5 mm"},
                {"type": "sheet_hem", "edges": [[0, 0, 0]], "length": "10 mm"},
                {"type": "extrude", "gap": "1 mm"},
            ],
            "sheet": {"rules": [{"name": "A", "hem_gap": "T / 4"}, {"name": "B"}]},
        });
        sheet_hem_gap(&mut v).unwrap();
        assert_eq!(v["features"][0]["gap"], json!("2 * (0.5 mm)"));
        assert!(
            v["features"][1].get("gap").is_none(),
            "a hem with no gap of its own takes its rule's"
        );
        assert_eq!(v["features"][2]["gap"], json!("1 mm"), "only hems");
        assert_eq!(v["sheet"]["rules"][0]["hem_gap"], json!("2 * (T / 4)"));
        assert_eq!(v["sheet"]["rules"][1]["hem_gap"], json!("0.02 mm"));
    }

    #[test]
    fn a_plate_conversion_is_recognize_of_the_plate_and_one_with_bend_edges_stays() {
        let tray = json!({
            "id": 6, "name": "C3", "type": "sheet_convert", "body": "Tray", "face": [50, 30, 0],
            "bends": [[50, 0, 0]], "face_names": ["F5:-z"],
        });
        let mut v = json!({
            "features": [
                {"id": 2, "name": "C1", "type": "sheet_convert", "body": "Plate", "face": [50, 30, 2], "rule": "R1"},
                {"id": 4, "name": "C2", "type": "sheet_convert", "body": "Slab", "face": [0, 0, 1], "bends": []},
                tray.clone(),
                {"id": 7, "name": "C4", "type": "sheet_convert", "body": "Part", "face": [0, 0, 1], "faces": [[0, 0, 0]]},
                {"id": 8, "name": "E1", "type": "extrude", "body": "Plate"},
            ],
        });
        sheet_surface_convert(&mut v).unwrap();
        assert_eq!(
            v["features"][0],
            json!({"id": 2, "name": "C1", "type": "sheet_recognize", "body": "Plate", "face": [50, 30, 2], "rule": "R1"})
        );
        // An empty list of bend edges is none: a plate too.
        assert_eq!(
            v["features"][1],
            json!({"id": 4, "name": "C2", "type": "sheet_recognize", "body": "Slab", "face": [0, 0, 1]})
        );
        assert_eq!(v["features"][2], tray);
        assert_eq!(v["features"][3]["type"], json!("sheet_convert"));
        assert_eq!(
            v["features"][4]["type"],
            json!("extrude"),
            "only conversions"
        );
    }

    /// A design of two components, the second placed 100 along x, and a
    /// sketch in the first whose plane and links read the second's
    /// geometry.
    fn foreign_references(plane: Value, author_path: Option<Value>) -> Value {
        let mut placed = IDENTITY;
        placed[3][0] = 100.0;
        let mut sketch = json!({"id": 3, "name": "Drawn", "type": "sketch", "component": 1,
        "plane": plane,
        "sketch": {"links": [
            {"id": "j1", "source": {"type": "sketch_point", "sketch": 2, "point": "p1"}},
            {"id": "j2", "source": {"type": "intersection",
                "a": {"type": "sketch_curve", "sketch": 4, "curve": "c1"},
                "b": {"type": "sketch_curve", "sketch": 2, "curve": "c2"}}},
            {"id": "j3", "source": {"type": "edge", "body": "B", "at": [0, 0, 0]}},
            {"id": "j4", "source": {"type": "sketch_point", "sketch": 2, "point": "p1"},
             "sketch_placements": []}
        ]}});
        if let Some(path) = author_path {
            sketch["plane"]["anchor"]["author_path"] = path;
        }
        json!({
        "components": [{"id": 1, "name": "A"}, {"id": 5, "name": "B"}],
        "occurrences": [{"id": 10, "component": 5, "parent": 0, "name": "B:1", "transform": placed}],
        "features": [
            {"id": 2, "name": "Other", "type": "sketch", "component": 5},
            {"id": 6, "name": "Plane", "type": "construction_plane", "component": 5},
            sketch,
            {"id": 4, "name": "Own", "type": "sketch", "component": 1},
            {"id": 7, "name": "Root", "type": "sketch", "component": 0,
             "plane": {"type": "construction", "feature": 6}}
        ]})
    }

    #[test]
    fn foreign_sketch_references_keep_the_placements_they_were_drawn_at() {
        let plane = json!({"type": "offset", "distance": "5",
            "base": {"type": "construction", "feature": 6}});
        let mut v = foreign_references(plane, None);
        sketch_reference_placements::step(&mut v).unwrap();
        // Component A, never placed, stands at the identity; B 100 along x.
        let mut relative = IDENTITY;
        relative[3][0] = 100.0;
        let sketch = &v["features"][2];
        assert_eq!(sketch["plane"]["base"]["placement"], json!(relative));
        assert!(sketch["plane"].get("placement").is_none());
        let links = &sketch["sketch"]["links"];
        assert_eq!(
            links[0]["sketch_placements"],
            json!([{"sketch": 2, "placement": relative}])
        );
        // An intersection reads both sketches; its own component's stays
        // live.
        assert_eq!(
            links[1]["sketch_placements"],
            json!([{"sketch": 2, "placement": relative}])
        );
        assert!(links[2].get("sketch_placements").is_none());
        assert_eq!(links[3]["sketch_placements"], json!([]), "kept as saved");
        // A root sketch's references stay live.
        assert!(v["features"][4]["plane"].get("placement").is_none());
    }

    #[test]
    fn a_face_attached_sketch_measures_from_its_author_path() {
        let face = json!({"type": "face", "at": [0, 0, 0], "name": "F9:+z",
            "anchor": {"component": 0, "path": [], "author_path": []}});
        // No author path: the attachment's own error, nothing captured.
        let mut v = foreign_references(face.clone(), Some(json!([])));
        sketch_reference_placements::step(&mut v).unwrap();
        assert!(
            v["features"][2]["sketch"]["links"][0]
                .get("sketch_placements")
                .is_none()
        );
        // A path that no longer exists: likewise.
        let mut v = foreign_references(face.clone(), Some(json!([99])));
        sketch_reference_placements::step(&mut v).unwrap();
        assert!(
            v["features"][2]["sketch"]["links"][0]
                .get("sketch_placements")
                .is_none()
        );
        // Through B's occurrence the author frame is B's, so B's sketch is
        // where the author stands; the path must lead to the author's own
        // component, which B's does not.
        let mut v = foreign_references(face, Some(json!([10])));
        sketch_reference_placements::step(&mut v).unwrap();
        assert!(
            v["features"][2]["sketch"]["links"][0]
                .get("sketch_placements")
                .is_none()
        );
        let mut empty = json!({"name": "Untitled"});
        sketch_reference_placements::step(&mut empty).unwrap();
        assert_eq!(empty, json!({"name": "Untitled"}));
    }

    /// A sketch feature of straight lines between points saved where they
    /// end.
    fn lines_sketch(id: u64, lines: &[(&str, [f64; 2], [f64; 2], bool)]) -> Value {
        let mut points = vec![json!({"id": "origin", "pos": [0.0, 0.0], "fixed": true})];
        let mut curves = Vec::new();
        for (line, a, b, construction) in lines {
            points.push(json!({"id": format!("{line}.start"), "pos": a, "fixed": false}));
            points.push(json!({"id": format!("{line}.end"), "pos": b, "fixed": false}));
            curves.push(json!({"id": line, "type": "line", "a": points.len() - 2,
                "b": points.len() - 1, "construction": construction}));
        }
        json!({"id": id, "name": format!("Sketch{id}"), "type": "sketch",
            "plane": {"type": "origin", "name": "XY"},
            "sketch": {"points": points, "curves": curves, "constraints": []}})
    }

    /// A 100 x 60 rectangle from the origin, `l1` along the front.
    const RECTANGLE: [(&str, [f64; 2], [f64; 2], bool); 4] = [
        ("l1", [0.0, 0.0], [100.0, 0.0], false),
        ("l2", [100.0, 0.0], [100.0, 60.0], false),
        ("l3", [100.0, 60.0], [0.0, 60.0], false),
        ("l4", [0.0, 60.0], [0.0, 0.0], false),
    ];

    fn edge_names_after(features: Value) -> Vec<Value> {
        let mut v = json!({"features": features});
        sheet_edge_names::step(&mut v).unwrap();
        v["features"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| f["edge_names"].clone())
            .collect()
    }

    #[test]
    fn a_flanges_and_a_hems_edges_take_the_models_names() {
        let mut lines = RECTANGLE.to_vec();
        lines.push(("l5", [0.0, 30.0], [100.0, 30.0], true));
        let split = json!({"point": [0, 30, 1], "segments": [{"from": 0, "to": 0.5},
            {"from": 0.5, "to": 1, "height": "5"}]});
        let names = edge_names_after(json!([
            lines_sketch(1, &lines),
            {"id": 2, "name": "BaseFlange1", "type": "sheet_base", "sketch": 1, "profiles": {"type": "all"}},
            {"id": 3, "name": "EdgeFlange1", "type": "sheet_flange", "edges": [[50, 0, 1], [100, 30, 0]],
             "edge_names": ["base:l1:top", "base:l2:bottom"]},
            {"id": 4, "name": "Hem1", "type": "sheet_hem", "edges": [[50, 1, 20], [100, 0, 10], [100, 60, 10]],
             "edge_names": ["F3.0:far:top", "F3.1:side0:bottom", "F3.1:side1:top"]},
            {"id": 5, "name": "EdgeFlange2", "type": "sheet_flange", "edges": [[50, -3, 15]],
             "edge_names": ["F4.0:far:bottom"]},
            {"id": 6, "name": "EdgeFlange3", "type": "sheet_flange", "edges": [split, [50, 60, 1]],
             "edge_names": ["base:l4:top", "base:l3:top"]},
            {"id": 7, "name": "Hem2", "type": "sheet_hem", "edges": [[0, 0, 0], [0, 0, 0], [0, 0, 0],
                [0, 0, 0], [0, 0, 0], [0, 0, 0], [0, 0, 0], [0, 0, 0]],
             "edge_names": ["F6.1:far:top", "F6.0:far:top", "F3.0:bend:top", "F9.0:far:top",
                "base:l5:top", "base:l9:top", "", "F2.0:far:top"]},
            {"id": 8, "name": "Fillet1", "type": "fillet", "edge_names": ["base:l1:top"]},
            {"id": 9, "name": "EdgeFlange4", "type": "sheet_flange", "edges": [[50, 0, 0]],
             "edge_names": ["base:l1:bottom"]},
        ]));
        assert_eq!(
            names[2],
            json!(["F2:side:l1|F2:top", "F2:bottom|F2:side:l2"])
        );
        assert_eq!(names[8], json!(["F2:bottom|F2:side:l1"]));
        assert_eq!(
            names[3],
            json!([
                "F3:far:F2:side:l1|F3:top:F2:side:l1",
                "F3:bottom:F2:side:l2|F3:end1:F2:side:l2",
                "F3:end0:F2:side:l2|F3:top:F2:side:l2"
            ])
        );
        assert_eq!(
            names[4],
            json!(["F4:bottom:F3:far:F2:side:l1|F4:far:F3:far:F2:side:l1"])
        );
        assert_eq!(names[5], json!(["F2:side:l4|F2:top", "F2:side:l3|F2:top"]));
        assert_eq!(
            names[6],
            json!([
                "F6.1:far:top",
                "F6.0:far:top",
                "F3.0:bend:top",
                "F9.0:far:top",
                "base:l5:top",
                "base:l9:top",
                "",
                "F2.0:far:top"
            ])
        );
        assert_eq!(names[7], json!(["base:l1:top"]));
        assert_eq!(names[0], Value::Null);
    }

    #[test]
    fn edge_names_on_a_sheet_the_design_does_not_say_or_along_a_notch_are_left() {
        let base = |id: u64, sketch: u64, body: Option<&str>| {
            let mut f = json!({"id": id, "name": format!("BaseFlange{id}"), "type": "sheet_base",
                "sketch": sketch, "profiles": {"type": "all"}});
            if let Some(body) = body {
                f["body_names"] = json!([body]);
            }
            f
        };
        let flange = |id: u64, body: Option<&str>, name: &str| {
            let mut f = json!({"id": id, "name": format!("EdgeFlange{id}"), "type": "sheet_flange",
                "edges": [[50, 0, 1]], "edge_names": [name]});
            if let Some(body) = body {
                f["body"] = json!(body);
            }
            f
        };
        let names = edge_names_after(json!([
            lines_sketch(1, &RECTANGLE),
            base(2, 1, Some("Plate")),
            lines_sketch(3, &RECTANGLE),
            base(4, 3, None),
            flange(5, Some("Plate"), "base:l1:top"),
            flange(6, None, "base:l1:top"),
            flange(7, Some("Body2"), "base:l1:top"),
            flange(8, Some("Plate"), "F5.0:far:top"),
        ]));
        assert_eq!(names[4], json!(["F2:side:l1|F2:top"]));
        assert_eq!(names[5], json!(["base:l1:top"]));
        assert_eq!(names[6], json!(["base:l1:top"]));
        assert_eq!(names[7], json!(["F5:far:F2:side:l1|F5:top:F2:side:l1"]));
        // A mirror that may have copied the one base flange's body.
        let names = edge_names_after(json!([
            lines_sketch(1, &RECTANGLE),
            base(2, 1, Some("Plate")),
            {"id": 3, "name": "Mirror1", "type": "mirror", "bodies": ["Plate"]},
            flange(4, None, "base:l1:top"),
            flange(5, Some("Plate"), "base:l1:top"),
        ]));
        assert_eq!(
            (&names[3], &names[4]),
            (&json!(["base:l1:top"]), &json!(["F2:side:l1|F2:top"]))
        );
        // A front edge notched between x = 40 and 60: l1 and l5 lie in line.
        let notched = [
            ("l1", [0.0, 0.0], [40.0, 0.0], false),
            ("l2", [40.0, 0.0], [40.0, 20.0], false),
            ("l3", [40.0, 20.0], [60.0, 20.0], false),
            ("l4", [60.0, 20.0], [60.0, 0.0], false),
            ("l5", [60.0, 0.0], [100.0, 0.0], false),
            ("l6", [100.0, 0.0], [100.0, 60.0], false),
            ("l7", [100.0, 60.0], [0.0, 60.0], false),
            ("l8", [0.0, 60.0], [0.0, 0.0], false),
        ];
        let names = edge_names_after(json!([
            lines_sketch(1, &notched),
            base(2, 1, None),
            {"id": 3, "name": "EdgeFlange3", "type": "sheet_flange", "edges": [[80, 0, 1], [100, 30, 1], [50, 20, 1]],
             "edge_names": ["base:l1:top", "base:l6:top", "base:l3:top"]},
        ]));
        assert_eq!(
            names[2],
            json!(["base:l1:top", "F2:side:l6|F2:top", "F2:side:l3|F2:top"])
        );
        // A converted sheet: no base flange's.
        let names = edge_names_after(json!([
            {"id": 1, "name": "Box1", "type": "box", "body_names": ["Tray"]},
            {"id": 2, "name": "C1", "type": "sheet_convert", "body": "Tray", "face": [50, 30, 0],
             "bends": [[50, 0, 0]], "edge_names": ["F1:-y|F1:-z"]},
            flange(3, Some("Tray"), "F2.0:far:top"),
            flange(4, None, "F2.0:far:top"),
        ]));
        assert_eq!(names[1], json!(["F1:-y|F1:-z"]));
        assert_eq!(
            (&names[2], &names[3]),
            (&json!(["F2.0:far:top"]), &json!(["F2.0:far:top"]))
        );
    }

    #[test]
    fn the_faces_of_a_base_flanges_panel_and_a_converted_fixed_face_take_the_models_names() {
        let face = |name: &str| -> Value {
            json!({"type": "face", "plane": {}, "at": [0, 0, 0], "name": name})
        };
        let curves = json!([
            {"id": "l1", "type": "line", "a": 1, "b": 2, "construction": false},
            {"id": "l2", "type": "line", "a": 2, "b": 3, "construction": true},
            {"id": "a1", "type": "arc", "a": 3, "b": 1},
        ]);
        let picked = json!([
            "sheet:Plate:base:rim.l1",
            "sheet:Plate:base:rim.l2",
            "sheet:Plate:base:rim.edge0",
            "sheet:Plate:F6.0:top",
            ""
        ]);
        let mut v = json!({
            "features": [
                {"id": 1, "name": "Base", "type": "sketch", "plane": {"type": "origin", "name": "XY"},
                 "sketch": {"curves": curves}},
                {"id": 2, "name": "Flange1", "type": "sheet_base", "sketch": 1, "body_names": ["Plate"]},
                {"id": 3, "name": "Top", "type": "sketch", "plane": face("sheet:Plate:base:top")},
                {"id": 4, "name": "Below", "type": "sketch", "plane": {"type": "offset",
                 "base": face("sheet:Plate:base:bottom"), "distance": "5"}},
                {"id": 5, "name": "Hole1", "type": "hole", "face_names": picked},
                {"id": 6, "name": "Flange2", "type": "sheet_flange", "edge_names": ["base:l1:top"]},
                {"id": 7, "name": "Box1", "type": "box", "body_names": ["Tray"]},
                {"id": 8, "name": "C1", "type": "sheet_convert", "body": "Tray", "face": [50, 30, 0],
                 "bends": [[50, 0, 0]], "face_names": ["F7:-z"]},
                {"id": 9, "name": "sheet:Plate:base:top", "type": "construction_plane",
                 "plane": face("sheet:Tray:base:bottom")},
                {"id": 10, "name": "Inside", "type": "sketch", "plane": face("sheet:Tray:base:top")},
                {"id": 11, "name": "Flange3", "type": "sheet_base", "sketch": 1, "body_names": ["Twice"]},
                {"id": 12, "name": "Join1", "type": "join", "body_names": ["Twice"]},
                {"id": 13, "name": "Convert2", "type": "sheet_convert", "body": "Slab", "face": [0, 0, 1]},
                {"id": 14, "name": "Late", "type": "sketch", "plane": face("sheet:Twice:base:top")},
                {"id": 15, "name": "OnSlab", "type": "sketch", "plane": face("sheet:Slab:base:top")},
            ],
        });
        sheet_face_names::step(&mut v).unwrap();
        let f = &v["features"];
        assert_eq!(f[2]["plane"]["name"], json!("F2:top"));
        assert_eq!(f[3]["plane"]["base"]["name"], json!("F2:bottom"));
        assert_eq!(
            f[4]["face_names"],
            json!([
                "F2:side:l1",
                "sheet:Plate:base:rim.l2",
                "sheet:Plate:base:rim.edge0",
                "sheet:Plate:F6.0:top",
                ""
            ])
        );
        assert_eq!(f[5]["edge_names"], json!(["base:l1:top"]));
        assert_eq!(f[8]["plane"]["name"], json!("F7:-z"));
        assert_eq!(f[8]["name"], json!("sheet:Plate:base:top"));
        assert_eq!(f[9]["plane"]["name"], json!("F8:bottom:F7:-z"));
        assert_eq!(f[7]["face_names"], json!(["F7:-z"]));
        assert_eq!(f[13]["plane"]["name"], json!("sheet:Twice:base:top"));
        assert_eq!(f[14]["plane"]["name"], json!("sheet:Slab:base:top"));
    }

    #[test]
    fn older_reversed_two_sided_extrudes_keep_their_shape() {
        let mut v = json!({"features": [
            {"type": "extrude", "extent": {"distance": "10 mm", "direction": "negative", "distance2": "5 mm"}},
            {"type": "surface_extrude", "extent": {"distance": "10 mm", "direction": "negative", "distance2": "5 mm"}},
            {"type": "extrude", "extent": {"distance": "10 mm", "direction": "negative"}},
            {"type": "extrude", "extent": {"distance": "10 mm", "direction": "symmetric", "distance2": "5 mm"}},
            {"type": "revolve", "extent": {"direction": "negative", "distance2": "5 mm"}},
            {"type": "sketch"}
        ]});
        two_sided_extrude_direction(&mut v).unwrap();
        let dirs: Vec<Option<&str>> = v["features"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| f["extent"]["direction"].as_str())
            .collect();
        assert_eq!(
            dirs,
            [
                Some("positive"),
                Some("positive"),
                Some("negative"),
                Some("symmetric"),
                Some("negative"),
                None
            ]
        );
        let mut empty = json!({"name": "Untitled"});
        two_sided_extrude_direction(&mut empty).unwrap();
        assert_eq!(empty, json!({"name": "Untitled"}));
    }

    #[test]
    fn older_features_keep_their_piece_numbering() {
        let mut v = json!({"features": [{"type": "sketch"}, {"type": "extrude"}]});
        face_pieces_by_place(&mut v).unwrap();
        assert!(
            v["features"]
                .as_array()
                .unwrap()
                .iter()
                .all(|f| f["kernel_piece_order"] == true)
        );
        let mut empty = json!({});
        face_pieces_by_place(&mut empty).unwrap();
        assert_eq!(empty, json!({}));
    }

    #[test]
    fn older_inserted_files_are_marked_but_copied_bases_are_not() {
        let mut v = json!({"features": [
            {"type": "import", "file": "parts.step", "component": 2},
            {"type": "mesh_import", "file": "bracket.stl", "component": 5},
            {"type": "import", "file": "Block independent base", "component": 7},
            {"type": "import", "file": "opened.step"},
            {"type": "extrude", "component": 2}
        ]});
        inserted_file_features(&mut v).unwrap();
        let marked: Vec<bool> = v["features"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| f["inserted"] == json!(true))
            .collect();
        assert_eq!(marked, [true, true, false, false, false]);
    }

    /// A sketch and an extrude sharing the name `Boss`, a construction
    /// plane, a construction axis and a lower-cased extrude `boss`.
    fn features() -> Vec<Value> {
        vec![
            json!({"id": 1, "name": "Boss", "type": "sketch"}),
            json!({"id": 2, "name": "Boss", "type": "extrude"}),
            json!({"id": 3, "name": "Mid", "type": "construction_plane"}),
            json!({"id": 4, "name": "Axis1", "type": "construction_axis"}),
            json!({"id": 5, "name": "boss", "type": "extrude"}),
        ]
    }

    #[test]
    fn seeds_resolve_as_replay_resolved_them() {
        let mut feats = features();
        feats.push(json!({"id": 6, "name": "Copies", "type": "pattern",
            "features": ["Boss", "mid", "5", "Gone"], "pattern": {"type": "circular"}}));
        let mut v = json!({"format": "solidcraft/36", "features": feats});
        feature_id_refs(&mut v).unwrap();
        // `Boss`: the exact name on a feature that is not a sketch; `mid`:
        // case-insensitive; `5`: an id written as a number; `Gone`: id 0.
        assert_eq!(v["features"][5]["features"], json!([2, 3, 5, 0]));
    }

    #[test]
    fn construction_planes_become_ids_and_dangle_as_zero() {
        let mut feats = features();
        feats.push(json!({"id": 7, "name": "On Mid", "type": "sketch",
            "plane": {"type": "construction", "name": "MID"}}));
        feats.push(json!({"id": 8, "name": "Lost", "type": "sketch",
            "plane": {"type": "offset", "distance": "5", "base": {"type": "construction", "name": "Gone"}}}));
        feats.push(json!({"id": 9, "name": "Turn", "type": "revolve",
            "axis": {"type": "construction", "name": "Axis1"}}));
        let mut d = json!({"format": "solidcraft/36", "features": feats,
            "canvases": [{"id": 1, "plane": {"type": "construction", "name": "Mid"}}]});
        feature_id_refs(&mut d).unwrap();
        assert_eq!(
            d["features"][5]["plane"],
            json!({"type": "construction", "feature": 3})
        );
        assert_eq!(
            d["features"][6]["plane"]["base"],
            json!({"type": "construction", "feature": 0})
        );
        // A revolve's axis names a construction axis, still by name.
        assert_eq!(
            d["features"][7]["axis"],
            json!({"type": "construction", "name": "Axis1"})
        );
        assert_eq!(
            d["canvases"][0]["plane"],
            json!({"type": "construction", "feature": 3})
        );
    }

    #[test]
    fn only_joins_to_an_object_saved_before_format_40_keep_the_body_apart() {
        let design = |format: &str| {
            json!({"format": format, "features": [
                {"type": "extrude", "operation": "join", "extent": {"distance": "0", "to": [0.0, 0.0, 30.0]}},
                {"type": "extrude", "operation": "join", "extent": {"distance": "10 mm"}},
                {"type": "extrude", "operation": "cut", "extent": {"distance": "0", "to": [0.0, 0.0, 30.0]}},
            ]})
        };
        let apart = |v: &Value| -> Vec<bool> {
            v["features"]
                .as_array()
                .unwrap()
                .iter()
                .map(|f| f["extent"]["to_body_apart"] == json!(true))
                .collect()
        };
        let mut old = design("solidcraft/39");
        join_to_body_apart(&mut old).unwrap();
        assert_eq!(apart(&old), [true, false, false]);
        for newer in ["solidcraft/40", "solidcraft/46"] {
            let mut v = design(newer);
            join_to_body_apart(&mut v).unwrap();
            assert_eq!(apart(&v), [false, false, false], "{newer}");
        }
    }

    #[test]
    fn older_joins_keep_touching_pieces_in_one_body() {
        let mut v = json!({"features": [
            {"type": "extrude", "operation": "join"},
            {"type": "extrude", "operation": "cut"},
            {"type": "combine", "operation": "cut"},
            {"type": "pattern"},
            {"type": "fillet"}
        ]});
        join_touching_bodies(&mut v).unwrap();
        let one: Vec<bool> = v["features"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| f["touching"] == json!("one_body"))
            .collect();
        assert_eq!(one, [true, false, true, true, false]);
    }

    #[test]
    fn a_chamfered_tip_becomes_both_ends_and_the_start_runout_goes() {
        let mut v = json!({"features": [
            {"type": "thread", "designation": "M10", "modeled": true, "tip": "chamfer", "ease_in": "standard", "ease_out": "short"},
            {"type": "thread", "designation": "M10", "modeled": true},
            {"type": "chamfer", "tip": "chamfer"}
        ]});
        thread_ends(&mut v).unwrap();
        assert_eq!(v["features"][0]["tip"], json!("both"));
        assert!(v["features"][0].get("ease_in").is_none());
        assert_eq!(v["features"][0]["ease_out"], json!("short"));
        assert!(v["features"][1].get("tip").is_none());
        assert_eq!(v["features"][2]["tip"], json!("chamfer"), "only threads");
    }

    #[test]
    fn handedness_becomes_hand() {
        let mut v = json!({"features": [
            {"type": "thread", "handedness": "left"},
            {"type": "hole", "handedness": "right"},
            {"type": "extrude", "handedness": "left"}
        ]});
        thread_hand(&mut v).unwrap();
        assert_eq!(v["features"][0]["hand"], json!("left"));
        assert!(v["features"][0].get("handedness").is_none());
        assert!(v["features"][1].get("hand").is_none());
        assert!(v["features"][1].get("handedness").is_none());
        assert_eq!(
            v["features"][2]["handedness"],
            json!("left"),
            "only threads and holes"
        );
        let mut bad = json!({"features": [{"type": "hole", "handedness": "both"}]});
        assert!(thread_hand(&mut bad).is_err());
    }

    #[test]
    fn an_automatic_thread_keeps_the_old_rule_and_a_named_one_is_left_alone() {
        let mut v = json!({"features": [
            {"type": "thread", "face": [6, 0, 5]},
            {"type": "thread", "designation": "M12"},
            {"type": "hole", "thread": "M12"}
        ]});
        thread_nominal_size(&mut v).unwrap();
        assert_eq!(v["features"][0]["old_auto_size"], json!(true));
        assert!(v["features"][1].get("old_auto_size").is_none());
        assert!(v["features"][2].get("old_auto_size").is_none());
    }

    /// A design of a released format has every change of it; one written
    /// while they were pending gets the steps of those it does not list,
    /// and those of every later format; one listing a change nobody knows
    /// is refused.
    #[test]
    fn a_format_runs_the_changes_a_design_lacks() {
        let two_sided = || {
            json!({"format": "solidcraft/28", "features": [{"type": "extrude",
                "extent": {"distance": "10", "direction": "negative", "distance2": "5"}}]})
        };
        let mut released = two_sided();
        let ran = upgrade(&mut released, 28, None).unwrap();
        assert!(!ran.iter().any(|r| r == "two-sided-extrude-direction"));
        assert!(ran.iter().any(|r| r == "face-pieces-by-place"));
        assert_eq!(released["features"][0]["extent"]["direction"], "negative");
        let mut pending = two_sided();
        let listed = ["vertex-edge-names".to_owned()];
        upgrade(&mut pending, 28, Some(&listed[..])).unwrap();
        assert_eq!(pending["features"][0]["extent"]["direction"], "positive");
        let mut had = two_sided();
        let listed = ["two-sided-extrude-direction".to_owned()];
        upgrade(&mut had, 28, Some(&listed[..])).unwrap();
        assert_eq!(had["features"][0]["extent"]["direction"], "negative");
        // The four changes format 28 gained at its release are known.
        let listed = ["surface-blend-seam".to_owned()];
        assert!(upgrade(&mut two_sided(), 28, Some(&listed[..])).is_ok());
        assert!(newer(28, Some(&listed[..])).is_none());
        // A change nobody here knows: read as far as this import reads it,
        // and said to be newer.
        let future = ["a-change-from-the-future".to_owned()];
        let mut later = two_sided();
        let ran = upgrade(&mut later, 28, Some(&future[..])).unwrap();
        assert!(ran.iter().any(|r| r == "two-sided-extrude-direction"));
        assert_eq!(
            newer(28, Some(&future[..])),
            Some(Newer {
                format: 28,
                reads_up_to: PINNED,
                unknown_changes: future.to_vec()
            })
        );
        // A list beside a format of hand-written steps means nothing.
        assert!(upgrade(&mut json!({}), 12, Some(&future[..])).is_ok());
        assert!(newer(12, Some(&future[..])).is_none());
        // A newer number has no steps to run.
        let mut past = two_sided();
        assert!(upgrade(&mut past, PINNED + 1, None).unwrap().is_empty());
        assert_eq!(past, two_sided());
        let said = newer(PINNED + 1, Some(&future[..])).unwrap();
        assert_eq!((said.format, said.reads_up_to), (64, 63));
        assert_eq!(said.unknown_changes, future);
        assert!(newer(PINNED, None).is_none());
        assert!(upgrade(&mut json!({}), 0, None).is_err());
        // The renamed card change of format 29 reads as its canonical name.
        let alias = ["physical-material-cards".to_owned()];
        assert!(newer(29, Some(&alias[..])).is_none());
        assert!(newer(30, Some(&alias[..])).is_some());
    }

    #[test]
    fn saved_two_sided_extrudes_keep_their_sides_and_distances() {
        let mut v = json!({"features": [
            {"type": "extrude", "extent": {"distance": "-10 mm", "distance2": "5 mm"}},
            {"type": "extrude", "extent": {"distance": "-10 mm", "direction": "negative", "distance2": "5 mm"}},
            {"type": "extrude", "extent": {"distance": "-0 mm", "distance2": "5 mm"}},
            {"type": "extrude", "extent": {"distance": "-width", "distance2": "10 mm"}},
            {"type": "extrude", "extent": {"distance": "-10 mm"}},
            {"type": "extrude", "extent": {"distance": "-10 mm", "through_all": true, "distance2": "5 mm"}},
            {"type": "extrude", "extent": {"distance": "-10 mm", "to": [0.0, 0.0, 1.0], "distance2": "5 mm"}},
            {"type": "sketch"}
        ]});
        let before = v.clone();
        two_sided_extrude_fixed_sides(&mut v).unwrap();
        let features = v["features"].as_array().unwrap();
        let marked: Vec<bool> = features
            .iter()
            .map(|f| f["extent"]["second_follows_first"] == json!(true))
            .collect();
        assert_eq!(marked, [true, true, true, true, false, false, false, false]);
        for (f, b) in features.iter().zip(before["features"].as_array().unwrap()) {
            assert_eq!(
                (&f["extent"]["distance"], &f["extent"]["direction"]),
                (&b["extent"]["distance"], &b["extent"]["direction"])
            );
        }
        let mut empty = json!({"name": "Untitled"});
        two_sided_extrude_fixed_sides(&mut empty).unwrap();
        assert_eq!(empty, json!({"name": "Untitled"}));
    }

    fn config_design(next_id: Option<u64>, rows: Value) -> Value {
        let mut v =
            json!({"name": "Plate", "configs": {"columns": [], "rows": rows, "active": "Long"}});
        if let Some(next) = next_id {
            v["next_id"] = next.into();
        }
        v
    }

    fn row_ids(v: &Value) -> Vec<Option<u64>> {
        v["configs"]["rows"].as_array().map_or(Vec::new(), |rows| {
            rows.iter().map(|row| row["id"].as_u64()).collect()
        })
    }

    /// Rows get ids in order from the saved counter, past any id a row has,
    /// and the counter moves past them; the active row stays named.
    #[test]
    fn an_older_designs_rows_get_ids_in_order_from_its_counter() {
        let rows = json!([{"name": "Short", "cells": []}, {"name": "Long", "cells": []}]);
        let mut v = config_design(Some(7), rows.clone());
        configuration_row_ids(&mut v).unwrap();
        assert_eq!(
            (row_ids(&v), &v["next_id"], &v["configs"]["active"]),
            (vec![Some(7), Some(8)], &json!(9), &json!("Long"))
        );
        // No counter: from 1.
        let mut v = config_design(None, rows);
        configuration_row_ids(&mut v).unwrap();
        assert_eq!(
            (row_ids(&v), &v["next_id"]),
            (vec![Some(1), Some(2)], &json!(3))
        );
        // A row that has an id keeps it, and the others go past it.
        let mut v = config_design(
            Some(2),
            json!([{"name": "Short", "cells": []}, {"id": 40, "name": "Long", "cells": []}]),
        );
        configuration_row_ids(&mut v).unwrap();
        assert_eq!(
            (row_ids(&v), &v["next_id"]),
            (vec![Some(41), Some(40)], &json!(42))
        );
        // Nothing to give: left as it is.
        for mut v in [
            json!({"name": "Plate", "next_id": 3}),
            config_design(Some(3), json!([{"id": 1, "name": "Long", "cells": []}])),
            config_design(None, json!([])),
        ] {
            let before = v.clone();
            configuration_row_ids(&mut v).unwrap();
            assert_eq!(v, before);
        }
        // A counter at the end of the id space gives no id.
        let mut v = config_design(Some(u64::MAX), json!([{"name": "Long", "cells": []}]));
        let e = configuration_row_ids(&mut v).unwrap_err();
        assert!(e.contains("id space is exhausted"), "{e}");
    }

    #[test]
    fn saved_holes_at_sketch_points_keep_starting_at_the_surface() {
        let mut v = json!({"features": [
            {"type": "hole", "points": {"sketch": 3, "ids": ["p1"]}},
            {"type": "hole", "points": {"sketch": 3, "ids": ["p1", "p2"], "flip": true}},
            {"type": "hole", "points": {"sketch": 3, "ids": ["p1"], "single": true}},
            {"type": "hole", "position": [0.0, 0.0, 10.0]},
            {"type": "extrude", "points": {"sketch": 3, "ids": ["p1"]}}
        ]});
        hole_sketch_plane_start(&mut v).unwrap();
        let marked: Vec<bool> = v["features"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| f["old_surface_start"] == json!(true))
            .collect();
        assert_eq!(marked, [true, true, false, false, false]);
        let mut empty = json!({"name": "Untitled"});
        hole_sketch_plane_start(&mut empty).unwrap();
        assert_eq!(empty, json!({"name": "Untitled"}));
    }

    #[test]
    fn saved_patterns_of_several_bodies_keep_reading_their_names_in_order() {
        let names = json!([
            "Base (2)", "Base (3)", "Base (4)", "Post (2)", "Post (3)", "Post (4)"
        ]);
        let mut v = json!({"features": [
            {"type": "pattern", "bodies": ["Base", "Post"], "body_names": names, "features": []},
            {"type": "pattern", "bodies": ["Base"], "body_names": ["Base (2)"], "features": []},
            {"type": "pattern", "bodies": ["Base", "Post"], "features": []},
            {"type": "pattern", "bodies": [], "features": [3, 4]},
            {"type": "box", "body_names": ["Base"]}
        ]});
        pattern_names_by_source(&mut v).unwrap();
        let features = v["features"].as_array().unwrap();
        let marked: Vec<bool> = features
            .iter()
            .map(|f| f["pattern_names_in_order"] == json!(true))
            .collect();
        assert_eq!(marked, [true, false, false, false, false]);
        assert_eq!(features[0]["body_names"], names);
    }

    /// An extrude to or from an object gets the other components'
    /// placements relative to its own, as the file holds them; one without
    /// an object, one that has them and a revolve are left alone.
    #[test]
    fn older_extrudes_to_an_object_keep_the_placements_they_were_built_at() {
        let moved = |x: f64| json!([[1, 0, 0, 0], [0, 1, 0, 0], [0, 0, 1, 0], [x, 0, 0, 1]]);
        let mut v = json!({
        "components": [{"id": 1, "name": "Frame"}, {"id": 2, "name": "Drawer"}, {"id": 3, "name": "Loose"}],
        "occurrences": [{"id": 4, "component": 1, "name": "Frame:1", "transform": moved(10.0)},
                        {"id": 5, "component": 2, "name": "Drawer:1", "transform": moved(25.0)}],
        "features": [
            {"type": "extrude", "component": 2, "extent": {"distance": "0", "to": [0.0, 0.0, 5.0]}},
            {"type": "extrude", "component": 2, "extent": {"distance": "4"}},
            {"type": "extrude", "extent": {"distance": "0", "from": [0.0, 0.0, 1.0], "picked_placements": []}},
            {"type": "revolve", "extent": {"to": [0.0, 0.0, 5.0]}}
        ]});
        extrude_to_object_keeps_picked_placements(&mut v).unwrap();
        let pins = v["features"][0]["extent"]["picked_placements"]
            .as_array()
            .unwrap()
            .clone();
        // The root, the frame and the never-placed component, each relative
        // to the drawer's placement.
        let shift = |c: u64| {
            pins.iter()
                .find(|p| p["component"] == json!(c))
                .and_then(|p| p["placement"][3][0].as_f64())
        };
        assert_eq!(pins.len(), 3, "{pins:?}");
        assert_eq!(shift(0), Some(-25.0));
        assert_eq!(shift(1), Some(-15.0));
        assert_eq!(shift(3), Some(-25.0));
        assert!(
            v["features"][1]["extent"]
                .get("picked_placements")
                .is_none()
        );
        assert_eq!(v["features"][2]["extent"]["picked_placements"], json!([]));
        assert!(
            v["features"][3]["extent"]
                .get("picked_placements")
                .is_none()
        );
        // A design without components has nothing to keep.
        let mut alone = json!({"features": [{"type": "extrude", "extent": {"distance": "0", "to": [0.0, 0.0, 5.0]}}]});
        let before = alone.clone();
        extrude_to_object_keeps_picked_placements(&mut alone).unwrap();
        assert_eq!(alone, before);
    }

    #[test]
    fn links_to_walls_of_a_sweep_along_several_curves_drop_their_saved_names() {
        let mut v = json!({"features": [
            {"id": 8, "type": "sweep", "path": ["l1", "a1"]},
            {"id": 9, "type": "sweep", "path": ["l1"]},
            {"id": 10, "type": "sketch", "sketch": {"links": [
                {"id": "j1", "vertex_name": "F8:side:l1#1|F8:side:l2|F4:top"},
                {"id": "j2", "edge_name": "F33:top#[F8:side:l2]|F33:side:l1"},
                {"id": "j3", "vertex_name": "F9:side:l1|F9:side:l2|F9:start"},
                {"id": "j4", "edge_name": "F18:side:l1|F4:top"},
                {"id": "j5", "auto_face": true, "face_name": "F8:side:l1#1"},
                {"id": "j6", "face_name": "F8:side:l1#2"}
            ]}}
        ]});
        sweep_walls_named_along_the_path(&mut v).unwrap();
        let links = v.pointer("/features/2/sketch/links").unwrap();
        assert_eq!(links[0].get("vertex_name"), None);
        assert_eq!(
            links[1].get("edge_name"),
            None,
            "a name built beside a wall of the sweep"
        );
        assert!(links[0]["name_dropped"] == json!(true) && links[1]["name_dropped"] == json!(true));
        assert_eq!(
            links[2]["vertex_name"],
            json!("F9:side:l1|F9:side:l2|F9:start")
        );
        assert_eq!(
            links[3]["edge_name"],
            json!("F18:side:l1|F4:top"),
            "F18 is not F8"
        );
        assert_eq!(
            links[4]["face_name"],
            json!("F8:side:l1#1"),
            "a sketch plane's own face"
        );
        assert_eq!(links[5].get("face_name"), None);
        assert!(
            [2, 3, 4, 5]
                .iter()
                .all(|i| links[*i].get("name_dropped").is_none())
        );
    }
}
