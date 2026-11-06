// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 SolidCraft contributors
//
// Adapted from SolidCraft (https://github.com/tangency-labs/solidcraft, release
// v0.40.0-rc.2, commit 777f960999cbd256ae8f53b3a042cccd19703ba6): the JSON
// upgrade steps of `crates/doc/src/format.rs` (`v2_to_v3`, `v3_to_v4`,
// `v4_to_v5`, `v5_to_v6`, `v13_to_v14` and their helpers), its upgrade path
// (`Formats::upgrade`, `Formats::check`, `Formats::parts`,
// `Formats::canonical_applied_name`), the change files of
// `crates/doc/src/format/v23` to `v50` (their names, and the steps of
// `v28/two-sided-extrude-direction.rs`, `v29/face-pieces-by-place.rs`,
// `v35/inserted-file-features.rs`, `v40/feature-id-refs.rs`,
// `v49/join-to-body-apart.rs`, `v49/join-touching-bodies.rs`,
// `v49/thread-ends.rs`, `v49/thread-hand.rs` and
// `v50/thread-nominal-size.rs`, with their tests) and the feature-input
// naming of `crates/doc/src/params.rs` (`Document::name_all_feature_inputs`).
// Changes: the steps work on `serde_json::Value` and return
// `Result<(), String>`; the input count per feature comes from
// `crate::inputs` (Mitcad's port of the feature walk); the formats this
// import reads are written out here (`FOLDED`, `PENDING`) instead of being
// listed by a build script. See README.md in this folder.

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

/// Refuses a design this import cannot read: format 0, a newer number, or
/// a pending change (`applied`) it does not know, which only a newer build
/// writes. A list beside a format that is not made of changes means
/// nothing.
fn check(version: u32, applied: Option<&[String]>) -> Result<(), String> {
    if version == 0 || version > PINNED {
        return Err(format!(
            "file format {version}: this import reads SolidCraft designs up to format {PINNED}"
        ));
    }
    let applied = applied
        .filter(|_| parts(version).is_some())
        .unwrap_or_default();
    match applied.iter().find(|n| !knows(canonical(version, n))) {
        Some(unknown) => Err(format!(
            "file format {version} with `{unknown}`, a change newer than this import knows"
        )),
        None => Ok(()),
    }
}

/// Brings a design of format `version` up to [`PINNED`] and gives the steps
/// that ran, by name (`2 to 3` for the hand-written ones, a change's name
/// for the others): for the import's report. `applied` is the design's
/// `format_pending`: the changes of its own format it already has, when it
/// was written while they were pending; the other changes of that format
/// run first. A design of a released format has no such list and has every
/// change of its format (a released format 28 is not upgraded to 28 again).
/// A change already applied never runs again at a later number.
pub fn upgrade(
    v: &mut Value,
    version: u32,
    applied: Option<&[String]>,
) -> Result<Vec<String>, String> {
    check(version, applied)?;
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
        assert_eq!(PINNED, 50);
        assert!(parts(22).is_none());
        assert_eq!(parts(28).map(<[Part]>::len), Some(13));
        assert!(parts(PINNED + 1).is_none());
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
        let future = ["a-change-from-the-future".to_owned()];
        let e = upgrade(&mut two_sided(), 28, Some(&future[..])).unwrap_err();
        assert!(e.contains("newer"), "{e}");
        // A list beside a format of hand-written steps means nothing.
        assert!(upgrade(&mut json!({}), 12, Some(&future[..])).is_ok());
        let e = upgrade(&mut json!({}), PINNED + 1, None).unwrap_err();
        assert!(e.contains("up to format 50"), "{e}");
        assert!(upgrade(&mut json!({}), 0, None).is_err());
        // The renamed card change of format 29 reads as its canonical name.
        let alias = ["physical-material-cards".to_owned()];
        assert!(upgrade(&mut json!({}), 29, Some(&alias[..])).is_ok());
        assert!(upgrade(&mut json!({}), 30, Some(&alias[..])).is_err());
    }
}
