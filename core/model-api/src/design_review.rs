// SPDX-License-Identifier: MIT
//! Read-only modeling guidance from existing definition and result data.
//! Rule ids are shared by the complete review and per-command MCP hints.

use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;
use serde_json::{Value, json};

use super::ApiError;
use super::prelude::*;
use mitcad_model::Document;
use mitcad_model::expr::{Dims, ParamKind};
use mitcad_model::features::construction::PlaneDefinition;
use mitcad_model::features::geom_ref::GeomRef;
use mitcad_model::features::{FeatureDef, FeatureEntry, SketchPlane};
use mitcad_model::ids::{BodyUid, FeatureUid, OccurrenceUid};
use mitcad_model::kernel::Kernel;
use mitcad_model::notice::NoticeLevel;
use mitcad_model::parameters::ParamId;
use mitcad_model::print_plates::PlateTarget;
use mitcad_model::topo::TopoName;
use mitcad_model::{DocState, InstanceView, PrintTarget};

pub(super) const RULES: &[&str] = &[
    "failed-feature",
    "auto-fixable",
    "sketch-underconstrained",
    "sketch-fixed-heavy",
    "fixed-placement",
    "volatile-parent",
    "repeated-literal",
    "unused-parameter",
    "default-names",
    "ungrouped-timeline",
    "overhang",
    "thin-wall",
    "outside-build-volume",
    "min-gap",
    "stray-bodies",
    "no-bed-face",
    "no-print-profile",
    "split-profile",
];

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Scope {
    Name(String),
    Features(Vec<FeatureUid>),
}

impl Default for Scope {
    fn default() -> Self {
        Self::Name("all".to_owned())
    }
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Print {
    Enabled(bool),
    Named(String),
}

impl Default for Print {
    fn default() -> Self {
        Self::Named("auto".to_owned())
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ReviewQuery {
    #[serde(default)]
    scope: Scope,
    #[serde(default)]
    checks: Option<Vec<String>>,
    #[serde(default)]
    print: Print,
}

fn finding(
    rule: &str,
    level: NoticeLevel,
    target: impl ToString,
    message: impl Into<String>,
    advice: &str,
    topic: &str,
    anchor: &str,
) -> Value {
    json!({"rule":rule, "level":level.as_str(), "target":target.to_string(),
        "message":message.into(), "advice":advice,
        "guide":{"topic":topic,"anchor":anchor}})
}

fn fixed_plane<K: Kernel>(doc: &Document<K>, def: &FeatureDef) -> bool {
    let fixed_datum = |uid| {
        doc.feature(uid).is_some_and(|entry| {
            matches!(
                &entry.def, FeatureDef::ConstructionPlane(plane)
                    if matches!(plane.definition, PlaneDefinition::Fixed { .. })
            )
        })
    };
    let fixed = |reference: &Option<GeomRef>| match reference {
        Some(GeomRef::FixedPlane { .. }) => true,
        Some(GeomRef::Datum(uid)) => fixed_datum(*uid),
        _ => false,
    };
    let placed = |plane: &Option<GeomRef>,
                  at: Option<&mitcad_model::features::place_at::PlaceAt>| {
        fixed(plane) || at.is_some_and(|at| fixed(&at.plane) || fixed(&at.face))
    };
    match def {
        FeatureDef::Sketch(sketch) => match sketch.plane {
            SketchPlane::Construction(uid) => fixed_datum(uid),
            _ => false,
        },
        FeatureDef::Box(def) => placed(&def.plane, def.at.as_ref()),
        FeatureDef::Cylinder(def) => placed(&def.plane, def.at.as_ref()),
        FeatureDef::Sphere(def) => placed(&def.plane, def.at.as_ref()),
        FeatureDef::Torus(def) => placed(&def.plane, def.at.as_ref()),
        _ => false,
    }
}

/// Visit only geometric selections. A dependency on a modified body's
/// creator alone is not a selection of a volatile face or edge.
fn selected_topology(value: &Value, out: &mut BTreeSet<FeatureUid>) {
    fn name(value: &Value, out: &mut BTreeSet<FeatureUid>) {
        let Some(text) = value.as_str() else { return };
        match text.parse::<TopoName>() {
            Ok(TopoName::Face(face)) => out.extend(face.features()),
            Ok(TopoName::Edge(edge)) => out.extend(edge.features()),
            Ok(TopoName::Vertex(vertex)) => {
                out.extend(vertex.faces().iter().flat_map(|face| face.features()))
            }
            Err(_) => {}
        }
    }
    match value {
        Value::Object(object) => {
            for (key, child) in object {
                match key.as_str() {
                    "face" | "edge" | "vertex" => name(child, out),
                    "faces" | "edges" | "vertices" => {
                        if let Some(values) = child.as_array() {
                            for value in values {
                                name(value, out);
                            }
                        }
                    }
                    _ => {}
                }
                selected_topology(child, out);
            }
        }
        Value::Array(values) => {
            for value in values {
                selected_topology(value, out);
            }
        }
        _ => {}
    }
}

fn volatile(def: &FeatureDef) -> bool {
    matches!(
        def,
        FeatureDef::Fillet(_)
            | FeatureDef::Chamfer(_)
            | FeatureDef::RectangularPattern(_)
            | FeatureDef::CircularPattern(_)
            | FeatureDef::PathPattern(_)
            | FeatureDef::PatternAtPoints(_)
            | FeatureDef::Mirror(_)
    )
}

fn used_parameters<K: Kernel>(doc: &Document<K>) -> BTreeSet<ParamId> {
    let params = doc.parameters();
    let mut used: BTreeSet<ParamId> = doc.features().flat_map(|f| f.def.params()).collect();
    for parameter in params.iter() {
        used.extend(parameter.dependencies());
    }
    let state = doc.state();
    let texts = state
        .bindings
        .suppressed_when
        .values()
        .chain(state.bindings.occurrence_visible_when.values())
        .map(String::as_str);
    for name in DocState::expression_parameters(texts) {
        used.extend(params.find(&name));
    }
    for body in state.body_attributes.keys() {
        for name in state.body_binding_parameters(*body) {
            used.extend(params.find(&name));
        }
    }
    // Configuration inputs and their expressions can be useful before a
    // particular row is active. Count references and declared column names.
    fn config_uses(value: &Value, name: &str) -> bool {
        match value {
            Value::String(text) => {
                text == name
                    || mitcad_model::expr::Expr::parse(text)
                        .is_ok_and(|expr| expr.references().contains(&name))
            }
            Value::Array(values) => values.iter().any(|v| config_uses(v, name)),
            Value::Object(values) => values
                .iter()
                .any(|(key, v)| key == name || config_uses(v, name)),
            _ => false,
        }
    }
    let configurations = serde_json::to_value(&state.configurations).unwrap_or(Value::Null);
    for parameter in params.iter() {
        if config_uses(&configurations, parameter.name()) {
            used.insert(parameter.id());
        }
    }
    used
}

/// Whether a body name is a generated one, `Body<n>`, as the model names
/// new bodies (`display_names`, mitcad#291).
fn default_body_name(name: &str) -> bool {
    name.strip_prefix("Body")
        .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

/// `stray-bodies` (mitcad#1731): the pieces a feature split a body into
/// (`features/pieces.rs`) that still have default names while another
/// piece of the same split was named. A cut that splits a body leaves the
/// extra pieces as new bodies, easily missed until the bodies are listed.
fn stray_bodies<K: Kernel>(doc: &Document<K>, entry: &FeatureEntry, findings: &mut Vec<Value>) {
    let Some(groups) = doc
        .last_result()
        .results
        .iter()
        .find(|result| result.uid == entry.uid)
        .and_then(|result| result.output.as_ref())
        .and_then(|output| output.split_pieces.as_ref())
    else {
        return;
    };
    // The design keeps every body's name once a command has run, the
    // generated ones too: a default name is one of the generated form.
    let default = |body: BodyUid| default_body_name(&doc.body_name(body));
    let mut reported = BTreeSet::new();
    for group in groups.iter() {
        let pieces: Vec<BodyUid> = group
            .pieces
            .iter()
            .filter(|piece| !piece.gone && doc.body_shape(piece.body).is_some())
            .map(|piece| piece.body)
            .collect();
        let Some(named) = pieces.iter().find(|body| !default(**body)) else {
            continue;
        };
        for body in pieces.iter().filter(|body| default(**body)) {
            if reported.insert(*body) {
                findings.push(finding(
                    "stray-bodies",
                    NoticeLevel::Warning,
                    body,
                    format!(
                        "{} split {} and left {} as an extra body with a default name",
                        entry.name,
                        doc.body_name(*named),
                        doc.body_name(*body)
                    ),
                    "List the bodies: name the piece if it is wanted, or join or remove it if the split was not intended.",
                    "verification",
                    "measure-the-result",
                ));
            }
        }
    }
}

/// `split-profile` (mitcad#1731): a feature whose profile references a
/// sketch change found as several regions (a region split in pieces), or
/// as fewer regions than the feature selected (regions merged). The
/// feature builds with what was found and keeps only the region warning.
fn split_profile<K: Kernel>(doc: &Document<K>, entry: &FeatureEntry) -> Option<String> {
    let profiles = entry.def.info().profiles();
    // The references found under other keys: their sketch and how many
    // regions each was found as.
    let mut changed: Vec<(FeatureUid, usize)> = Vec::new();
    let mut found_regions = BTreeSet::new();
    for profile in profiles {
        // A reference that finds no region fails its feature, which
        // `failed-feature` reports.
        let Some(found) = doc
            .sketch_output(profile.sketch)
            .and_then(|output| output.found(&profile.region))
        else {
            continue;
        };
        if !found.exact {
            changed.push((profile.sketch, found.regions.len()));
        }
        found_regions.extend(
            found
                .regions
                .iter()
                .map(|region| (profile.sketch, region.profile.key.to_string())),
        );
    }
    let sketch_name = |uid: FeatureUid| {
        doc.feature(uid)
            .map_or_else(|| uid.to_string(), |sketch| sketch.name.clone())
    };
    if let Some((sketch, pieces)) = changed.iter().find(|(_, pieces)| *pieces > 1) {
        return Some(format!(
            "{} uses a profile region that a change in {} split into {} pieces",
            entry.name,
            sketch_name(*sketch),
            *pieces
        ));
    }
    let (sketch, _) = *changed.first()?;
    (found_regions.len() < profiles.len()).then(|| {
        format!(
            "{} selected {} profile regions, which a change in {} merged into {}",
            entry.name,
            profiles.len(),
            sketch_name(sketch),
            found_regions.len()
        )
    })
}

/// The complete query, also used by MCP hints with an explicit feature
/// scope, the three hint rule ids, and print disabled.
pub(super) fn review<K: Kernel>(doc: &Document<K>, query: &ReviewQuery) -> Result<Value, ApiError> {
    let selected: BTreeSet<FeatureUid> = match &query.scope {
        Scope::Name(name) if name == "all" => doc.features().map(|f| f.uid).collect(),
        Scope::Name(name) => {
            let component = doc.component_ref(name)?;
            doc.features()
                .filter(|f| f.component == component)
                .map(|f| f.uid)
                .collect()
        }
        Scope::Features(features) => {
            for uid in features {
                if doc.feature(*uid).is_none() {
                    return Err(ApiError(format!("no feature {uid}")));
                }
            }
            features.iter().copied().collect()
        }
    };
    if let Some(checks) = &query.checks {
        for check in checks {
            if !RULES.contains(&check.as_str()) {
                return Err(ApiError(format!(
                    "unknown design_review check {check}; checks: {}",
                    RULES.join(", ")
                )));
            }
        }
    }
    let enabled = |rule: &str| {
        query
            .checks
            .as_ref()
            .is_none_or(|checks| checks.iter().any(|c| c == rule))
    };
    let forced = match &query.print {
        Print::Enabled(enabled) => Some(*enabled),
        Print::Named(name) if name == "auto" => None,
        Print::Named(name) => {
            return Err(ApiError(format!(
                "design_review print must be auto, true or false, got {name}"
            )));
        }
    };
    let mut findings = Vec::new();
    let grouped: BTreeSet<FeatureUid> = doc
        .timeline_groups()
        .iter()
        .flat_map(|group| group.features.iter().copied())
        .collect();
    for item in doc
        .timeline()
        .into_iter()
        .filter(|item| selected.contains(&item.entry.uid))
    {
        let entry = item.entry;
        let active = !entry.suppressed
            && doc
                .state()
                .position(entry.uid)
                .is_some_and(|i| i < doc.marker());
        if enabled("failed-feature")
            && let Some(error) = item.error()
        {
            findings.push(finding(
                "failed-feature",
                NoticeLevel::Error,
                entry.uid,
                format!("{} failed: {error}", entry.name),
                "Repair the failed feature before adding dependent geometry.",
                "verification",
                "health",
            ));
        }
        if active && enabled("auto-fixable") && doc.auto_fixable(entry.uid) > 0 {
            findings.push(finding(
                "auto-fixable",
                NoticeLevel::Warning,
                entry.uid,
                format!("{} has references found under another name", entry.name),
                "Inspect the found references, then use auto_fix to retain the resolved names.",
                "verification",
                "health",
            ));
        }
        if active && let FeatureDef::Sketch(sketch) = &entry.def {
            if enabled("sketch-underconstrained")
                && let Some(output) = doc.sketch_output(entry.uid)
                && output.solved.status.dof > 0
            {
                findings.push(finding("sketch-underconstrained", NoticeLevel::Warning, entry.uid,
                    format!("{} has dof {}", entry.name, output.solved.status.dof),
                    "Dimension the intended sizes and constrain the remaining motion to stable references.", "sketches", "constraints"));
            }
            let fixed = sketch
                .entities
                .iter()
                .filter(|entity| entity.fixed && !entity.reference && entity.axis.is_none())
                .count();
            if enabled("sketch-fixed-heavy") && fixed > 0 {
                findings.push(finding("sketch-fixed-heavy", NoticeLevel::Warning, entry.uid,
                    format!("{} fixes {fixed} sketch entities in place", entry.name),
                    "Replace unnecessary fixed geometry with dimensions and geometric relations that express the design intent.", "sketches", "constraints"));
            }
        }
        if active && enabled("fixed-placement") && fixed_plane(doc, &entry.def) {
            findings.push(finding("fixed-placement", NoticeLevel::Warning, entry.uid,
                format!("{} is placed on a fixed plane", entry.name),
                "Pick an origin plane, associative construction plane or stable face so placement follows the model.", "references", "placement"));
        }
        if active
            && enabled("volatile-parent")
            && !matches!(&entry.def, FeatureDef::Base(_))
            && entry.def.references().features.iter().any(|uid| {
                doc.feature(*uid)
                    .is_some_and(|parent| volatile(&parent.def))
            })
        {
            let mut parents = BTreeSet::new();
            let definition = entry
                .def
                .map_params(&mut |_, id| Ok::<_, ()>(doc.parameters().name(*id)))
                .expect("parameter names never fail");
            selected_topology(
                &serde_json::to_value(&definition).map_err(|e| ApiError(e.to_string()))?,
                &mut parents,
            );
            for parent in parents
                .into_iter()
                .filter_map(|uid| doc.feature(uid))
                .filter(|f| volatile(&f.def))
            {
                findings.push(finding("volatile-parent", NoticeLevel::Warning, entry.uid,
                    format!("{} references geometry created by {} ({})", entry.name, parent.name, parent.uid),
                    "Use a stable base face, origin datum or construction reference instead of finishing or pattern geometry.", "references", "stable-parents"));
            }
        }
        if active && enabled("stray-bodies") {
            stray_bodies(doc, entry, &mut findings);
        }
        if active
            && enabled("split-profile")
            && let Some(message) = split_profile(doc, entry)
        {
            findings.push(finding(
                "split-profile",
                NoticeLevel::Warning,
                entry.uid,
                message,
                "Check that the feature still uses the intended area; select its profiles again, or use auto_fix to keep the regions found.",
                "sketches",
                "profiles-and-the-sketch-palette",
            ));
        }
        if enabled("default-names")
            && entry
                .name
                .strip_prefix(entry.def.base_name())
                .is_some_and(|suffix| {
                    !suffix.is_empty() && suffix.bytes().all(|c| c.is_ascii_digit())
                })
        {
            findings.push(finding(
                "default-names",
                NoticeLevel::Info,
                entry.uid,
                format!("{} still has its default feature name", entry.name),
                "Name the feature after its purpose so later edits are understandable.",
                "principles",
                "organisation",
            ));
        }
        if enabled("ungrouped-timeline") && !grouped.contains(&entry.uid) {
            findings.push(finding(
                "ungrouped-timeline",
                NoticeLevel::Info,
                entry.uid,
                format!("{} is outside every timeline group", entry.name),
                "Group related modeling stages with meaningful names.",
                "principles",
                "organisation",
            ));
        }
    }
    parameters(doc, &selected, &query.scope, &enabled, &mut findings);
    let mut errors = Vec::new();
    let mut contexts = Vec::new();
    let mut print_bodies = 0;
    let print_requested = [
        "overhang",
        "thin-wall",
        "outside-build-volume",
        "min-gap",
        "no-bed-face",
        "no-print-profile",
    ]
    .iter()
    .any(|rule| enabled(rule));
    if forced != Some(false) && print_requested {
        print_bodies = printing(
            doc,
            &selected,
            forced == Some(true),
            &enabled,
            &mut findings,
            &mut errors,
            &mut contexts,
        )?;
    }
    Ok(json!({"findings":findings, "errors":errors,
        "print":{"enabled":print_bodies > 0,"bodies":print_bodies,"contexts":contexts,
            "default_nozzle_diameter":0.4,"default_layer_height":0.2}}))
}

fn parameters<K: Kernel>(
    doc: &Document<K>,
    selected: &BTreeSet<FeatureUid>,
    scope: &Scope,
    enabled: &impl Fn(&str) -> bool,
    findings: &mut Vec<Value>,
) {
    let params = doc.parameters();
    if enabled("repeated-literal") {
        // Only actual length slots owned by features, not coordinates,
        // angles, quantities, arbitrary JSON numbers or evaluated formulas.
        let mut slots: BTreeMap<u64, Vec<(FeatureUid, String, f64)>> = BTreeMap::new();
        let mut seen = BTreeSet::new();
        for entry in doc.features().filter(|entry| selected.contains(&entry.uid)) {
            let _ = entry.def.map_params(&mut |slot, id| {
                if let Some(parameter) = params.get(*id)
                    && params.owner(*id) == Some(entry.uid)
                    && parameter.unit().dims() == Dims::LENGTH
                    && parameter.expr().is_plain_number()
                    && seen.insert(*id)
                {
                    let value = parameter.value();
                    let bits = if value == 0.0 {
                        0.0_f64.to_bits()
                    } else {
                        value.to_bits()
                    };
                    slots
                        .entry(bits)
                        .or_default()
                        .push((entry.uid, slot.to_owned(), value));
                }
                Ok::<_, ()>(*id)
            });
        }
        for repeated in slots.values().filter(|values| values.len() >= 3) {
            for (uid, slot, value) in repeated {
                findings.push(finding("repeated-literal", NoticeLevel::Info, *uid,
                    format!("{slot} repeats the literal length {value} mm in {} slots", repeated.len()),
                    "Use a shared named parameter when these equal lengths express the same design intent.", "principles", "parameters"));
            }
        }
    }
    // A global user parameter has no component owner. Only an all-design
    // review reports unused ones; a command/feature scope never does.
    if enabled("unused-parameter") && matches!(scope, Scope::Name(name) if name == "all") {
        let used = used_parameters(doc);
        for parameter in params
            .iter()
            .filter(|p| p.kind() == ParamKind::User && !used.contains(&p.id()))
        {
            findings.push(finding(
                "unused-parameter",
                NoticeLevel::Info,
                parameter.name(),
                format!("User parameter {} has no uses", parameter.name()),
                "Use it in the intended dimensions or remove it if it is no longer needed.",
                "principles",
                "parameters",
            ));
        }
    }
}

fn printing<K: Kernel>(
    doc: &Document<K>,
    selected: &BTreeSet<FeatureUid>,
    forced: bool,
    enabled: &impl Fn(&str) -> bool,
    findings: &mut Vec<Value>,
    errors: &mut Vec<Value>,
    contexts: &mut Vec<Value>,
) -> Result<usize, ApiError> {
    let mut checked = 0;
    // The checked bodies by the plate they print on (mitcad#1519): only
    // bodies printed together can fuse.
    let mut by_plate: BTreeMap<Option<usize>, Vec<BodyUid>> = BTreeMap::new();
    // mitcad#1731: the first checked body of each plate that prints without
    // a printer profile, and the occurrence items already looked at for a
    // bed face.
    let mut unprofiled: BTreeMap<usize, BodyUid> = BTreeMap::new();
    let mut bedless: BTreeSet<Vec<OccurrenceUid>> = BTreeSet::new();
    let instances = doc.instances();
    // The marker's final bodies may have been modified after their creator.
    // Scope includes the body's creator or any selected feature producing
    // that body in the retained result.
    let mut bodies: BTreeSet<_> = doc
        .bodies()
        .into_iter()
        .filter(|body| selected.contains(&body.uid.feature))
        .map(|body| body.uid)
        .collect();
    for uid in selected {
        if let Some(output) = doc
            .last_result()
            .results
            .iter()
            .find(|result| result.uid == *uid)
            .and_then(|result| result.output.as_ref())
        {
            bodies.extend(output.changes.iter().filter_map(|change| match change {
                mitcad_model::internal::recompute::Change::Set(body, _) => Some(*body),
                mitcad_model::internal::recompute::Change::Remove(_) => None,
            }));
        }
    }
    let visible: BTreeSet<_> = doc.bodies().into_iter().map(|body| body.uid).collect();
    bodies.retain(|body| visible.contains(body));
    for body in bodies {
        if doc.body_shape(body).is_none_or(|shape| {
            doc.kernel().body_kind(shape).is_ok_and(|kind| {
                !matches!(
                    kind,
                    mitcad_model::exchange::BodyKind::Solid
                        | mitcad_model::exchange::BodyKind::Mesh
                )
            })
        }) {
            continue;
        }
        // Use the same first shown placement (else first, else identity)
        // as print_placements and the existing print queries. Occurrence
        // settings and occurrence-assigned plates belong to that path.
        let first = instances.iter().find(|instance| instance.body == body);
        let path = instances
            .iter()
            .find(|instance| instance.body == body && instance.visible)
            .or(first)
            .map_or(&[][..], |instance| instance.path.as_slice());
        let settings = doc.effective_print(&PrintTarget::Body(body, Some(path.to_vec())));
        let bed = doc.print_bed_face(body);
        if !forced
            && bed.is_none()
            && doc.plate_of_body(body, Some(path)).is_none()
            && settings.printer.is_none()
        {
            continue;
        }
        checked += 1;
        by_plate
            .entry(doc.plate_of_body(body, Some(path)))
            .or_default()
            .push(body);
        if let Some(plate) = doc.plate_of_body(body, Some(path)) {
            if settings.printer.is_none() {
                unprofiled.entry(plate).or_insert(body);
            }
            if enabled("no-bed-face") {
                no_bed_face(doc, body, path, plate, &instances, &mut bedless, findings);
            }
        }
        let number = |key: &str, default| {
            settings
                .value(key)
                .and_then(|value| match value {
                    mitcad_model::print_keys::Value::Number(value) => Some(value),
                    _ => None,
                })
                .filter(|value| *value > 0.0)
                .unwrap_or(default)
        };
        let nozzle = number("nozzle_diameter", 0.4);
        contexts.push(json!({"body":body,
            "occurrence":path.iter().map(ToString::to_string).collect::<Vec<_>>().join("/"),
            "plate":doc.plate_of_body(body, Some(path)).map(|index| &doc.print_plates()[index].id),
            "printer":settings.printer.as_ref().map(|(id, _)| id),
            "nozzle_diameter":nozzle,"layer_height":number("layer_height", 0.2)}));
        for (rule, name) in [
            ("overhang", "print_overhangs"),
            ("thin-wall", "print_thickness"),
        ] {
            if !enabled(rule) {
                continue;
            }
            let mut request = json!({"query":name,"bodies":[body]});
            if rule == "overhang" {
                if let Some(bed) = &bed {
                    request["bed_face"] = json!({"body":body,"face":bed.face});
                }
            } else {
                request["minimum"] = json!(2.0 * nozzle);
                request["triangles"] = json!(false);
            }
            let answer: Value = serde_json::from_str(&doc.query(&request.to_string())?)
                .map_err(|e| ApiError(e.to_string()))?;
            if let Some(error) = answer.get("error") {
                errors.push(json!({"rule":rule,"target":body,"message":error}));
                continue;
            }
            let failed = if rule == "overhang" {
                answer["overhang_area"]
                    .as_f64()
                    .is_some_and(|area| area > 1e-6)
            } else {
                answer["thin_triangles"].as_u64().is_some_and(|n| n > 0)
            };
            if failed {
                let (message, advice) = if rule == "overhang" {
                    (
                        format!(
                            "{} has unsupported overhang area {} mm²",
                            doc.body_name(body),
                            answer["overhang_area"]
                        ),
                        "Check the print direction; reorient, split or revise unsupported geometry, then inspect the slicer preview.",
                    )
                } else {
                    (
                        format!(
                            "{} has walls thinner than {} mm",
                            doc.body_name(body),
                            2.0 * nozzle
                        ),
                        "Increase the thin walls or select a manufacturing process that can resolve them.",
                    )
                };
                findings.push(finding(
                    rule,
                    NoticeLevel::Warning,
                    body,
                    message,
                    advice,
                    "3d-printing",
                    "checks",
                ));
            }
        }
        if enabled("outside-build-volume") {
            let volume = doc.body_build_volume_json(body, path);
            if volume["bed"].is_null() {
                errors.push(json!({"rule":"outside-build-volume","target":body,
                    "message":"No printer bed is defined for this placement; build-volume fit was not checked."}));
            } else if let Some(outside) = volume["outside"].as_array() {
                for item in outside {
                    findings.push(finding("outside-build-volume", NoticeLevel::Warning,
                        item["body"].as_str().unwrap_or_default(),
                        format!("{} is outside the printer build volume: {}", item["name"], item["reasons"]),
                        "Check the printer and final plate layout; split or reposition pieces that do not fit.", "3d-printing", "checks"));
                }
            }
        }
    }
    if enabled("min-gap") {
        min_gaps(doc, &by_plate, findings, errors)?;
    }
    if enabled("no-print-profile") {
        for (plate, body) in unprofiled {
            let plate = &doc.print_plates()[plate];
            findings.push(finding(
                "no-print-profile",
                NoticeLevel::Warning,
                &plate.id,
                format!(
                    "{} prints without a printer profile ({} has none): its 3MF names a placeholder printer, which the slicer cannot slice with as it is",
                    plate.name,
                    doc.body_name(body)
                ),
                "Import the printer's slicer profile with import_print_profile and choose it as the printer before exporting the 3MF.",
                "3d-printing",
                "checks",
            ));
        }
    }
    Ok(checked)
}

/// `no-bed-face` (mitcad#1731): a checked body on a plate whose piece has
/// no usable print bed face, so the export orients it automatically
/// (`exchange/bed.rs`), which can lay an assembled piece on any flat face.
/// A body on its own item, or on none, is a piece of its own; one placed
/// through an occurrence item is a piece with the occurrence's other bodies,
/// which turns onto a bed face of any of them and is reported once.
fn no_bed_face<K: Kernel>(
    doc: &Document<K>,
    body: BodyUid,
    path: &[OccurrenceUid],
    plate: usize,
    instances: &[InstanceView],
    seen: &mut BTreeSet<Vec<OccurrenceUid>>,
    findings: &mut Vec<Value>,
) {
    let usable = |uid: BodyUid| doc.print_bed_face(uid).is_some_and(|bed| bed.plane.is_ok());
    let plates = doc.print_plates();
    let own = PlateTarget::Body(body);
    let occurrence = if plates.iter().any(|p| p.item(&own).is_some()) {
        None
    } else {
        plates[plate]
            .items
            .iter()
            .filter_map(|item| match &item.target {
                PlateTarget::Occurrence(o) if path.starts_with(o) => Some(o),
                _ => None,
            })
            .min_by_key(|o| o.len())
    };
    let message = match occurrence {
        None if !usable(body) => format!(
            "{} prints on {} without a print bed face, so the export orients it by itself",
            doc.body_name(body),
            plates[plate].name
        ),
        Some(o)
            if seen.insert(o.clone())
                && !instances
                    .iter()
                    .filter(|instance| instance.path.starts_with(o))
                    .any(|instance| usable(instance.body)) =>
        {
            format!(
                "{} prints on {} as assembled without a print bed face on any of its bodies, so the export orients it by itself",
                doc.plate_target_name(&PlateTarget::Occurrence(o.clone())),
                plates[plate].name
            )
        }
        _ => return,
    };
    findings.push(finding(
        "no-bed-face",
        NoticeLevel::Warning,
        body,
        message,
        "Assign the face that should lie on the bed with set_print_bed_face, so the export does not choose one.",
        "3d-printing",
        "orientation-and-strength",
    ));
}

/// `min-gap` (mitcad#1519): the checked bodies of each plate that are
/// apart but closer than the design's printed-in-place gap.
fn min_gaps<K: Kernel>(
    doc: &Document<K>,
    by_plate: &BTreeMap<Option<usize>, Vec<BodyUid>>,
    findings: &mut Vec<Value>,
    errors: &mut Vec<Value>,
) -> Result<(), ApiError> {
    for bodies in by_plate.values().filter(|bodies| bodies.len() > 1) {
        let request = json!({"query": "print_min_gap", "bodies": bodies});
        let answer: Value = serde_json::from_str(&doc.query(&request.to_string())?)
            .map_err(|e| ApiError(e.to_string()))?;
        if let Some(error) = answer.get("error") {
            errors.push(json!({"rule": "min-gap", "target": bodies[0], "message": error}));
            continue;
        }
        for pair in answer["pairs"].as_array().into_iter().flatten() {
            findings.push(finding(
                "min-gap",
                NoticeLevel::Warning,
                pair["a"].as_str().unwrap_or_default(),
                format!(
                    "{} and {} are {:.2} mm apart, closer than the {} mm gap that bodies printed in place need",
                    pair["names"][0].as_str().unwrap_or_default(),
                    pair["names"][1].as_str().unwrap_or_default(),
                    pair["distance"].as_f64().unwrap_or_default(),
                    answer["gap"],
                ),
                "Widen the gap between the bodies, or print them apart; check the slicer preview for a closed gap.",
                "3d-printing",
                "checks",
            ));
        }
    }
    Ok(())
}
