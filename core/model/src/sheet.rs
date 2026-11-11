// SPDX-License-Identifier: MIT
//! Immutable material domains paired with the current named body shape.

use crate::ids::{BodyUid, ComponentUid, FeatureUid};
use crate::notice::{Notice, NoticeLevel};
use crate::profile::{SegmentGeometry, SketchFrame};
use crate::transform::Transform;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

mod adapter;
// The sheet-domain adapter of ordinary features (mitcad#242).
mod rederive;
// Patterns and mirrors of sheet features (mitcad#242).
pub(crate) mod replay;
pub(crate) use adapter::planar_joins;
// Methods of the kernel's sheet types that need the model (mitcad#777).
pub(crate) use adapter::{BoundedCurve2Ext, MaterialRegionExt, SheetPatchExt};
pub(crate) use rederive::adapt;
pub(crate) mod folded;
// Mirrors and patterns of sheet bodies, as new bodies or joined
// (mitcad#1450).
pub(crate) mod body_copy;
// The derived whole-body Flat Pattern (mitcad#208).
pub mod flat;
// Its DXF drawing and picks resolved to semantic references (mitcad#210).
pub mod flat_dxf;
pub mod pick;
// The material charts the kernel's sheet operations take and give, below
// the model (mitcad#777).
pub use mitcad_model_base::sheet_charts::*;

pub const TOLERANCE: f64 = 1.0e-7;
pub const ALGORITHM_VERSION: u32 = 2;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SheetSegment {
    pub id: SheetCurveId,
    pub geometry: SegmentGeometry,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SheetLoop {
    pub segments: Vec<SheetSegment>,
}

/// First loop is the exterior, subsequent loops are holes. Curves are exact
/// sketch curves, not a sampled silhouette or a bounding rectangle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SheetRegion {
    pub loops: Vec<SheetLoop>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SheetPlanarPatch {
    pub id: SheetDomainId,
    pub frame: SketchFrame,
    pub region: SheetRegion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SheetReferenceSide {
    Front,
    Back,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SheetDiagnostic {
    pub feature: FeatureUid,
    pub message: String,
}

/// A change of a sheet's material its domains cannot hold (mitcad#242),
/// made again on the solid every sheet feature after it rebuilds.
/// `removed` is the volume it removed when it was made; a rebuild in which
/// it would remove more (material added later in its way) is refused.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SheetOverlay {
    pub feature: FeatureUid,
    #[serde(flatten)]
    pub kind: SheetOverlayKind,
    pub removed: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SheetOverlayKind {
    /// A straight cut (`sheet_cut` mode `straight`) whose walls are not
    /// normal to the material where it crosses a bend or an inclined panel:
    /// its profiles swept along `direction` from `start` to `end`.
    Straight {
        frame: SketchFrame,
        profiles: Vec<(crate::topo::RegionKey, SheetRegion)>,
        direction: [f64; 3],
        start: f64,
        end: f64,
    },
    /// An ordinary fillet or chamfer on the sheet (a countersink at a hole's
    /// edge, a rounded surface edge): the material it removed and added (its
    /// body before it against its result), moved by `placement` since.
    Change {
        placement: Transform,
        /// The body the feature changed, when it is not the sheet's own: a
        /// copy of a sheet body (a mirror or pattern of it) reads the
        /// change from the body it was made on (mitcad#1462).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source: Option<BodyUid>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SheetState {
    pub schema: u32,
    pub algorithm: u32,
    pub body: BodyUid,
    pub owner: ComponentUid,
    /// Hex fingerprint of the exact shape paired with these domains.
    pub shape_generation: String,
    pub rules: FeatureUid,
    pub rules_version: String,
    pub panels: Vec<SheetPlanarPatch>,
    #[serde(default)]
    pub charts: Vec<SheetPatch>,
    #[serde(default)]
    pub bends: Vec<BendDomain>,
    #[serde(default)]
    pub joins: Vec<ShapePort>,
    #[serde(default)]
    pub opened: Vec<OpenedBend>,
    #[serde(default)]
    pub lineage: Vec<CurveLineage>,
    /// Straight cuts the domains cannot hold (mitcad#242), in order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub overlays: Vec<SheetOverlay>,
    pub invalidated: Option<SheetDiagnostic>,
    pub blockers: Vec<SheetDiagnostic>,
}

impl SheetState {
    pub fn status(&self) -> &'static str {
        if self.invalidated.is_some() {
            "invalid"
        } else if !self.blockers.is_empty() {
            "blocked"
        } else {
            "valid"
        }
    }

    /// Its diagnostics as notices (mitcad#1040), each about the feature
    /// that made it: what invalidated the sheet (its flat state is gone:
    /// an error) and what blocks it (a warning).
    pub fn notices(&self) -> Vec<Notice> {
        let notice = |level, d: &SheetDiagnostic| {
            Notice::new(level, d.message.as_str()).with_target(d.feature)
        };
        self.invalidated
            .iter()
            .map(|d| notice(NoticeLevel::Error, d))
            .chain(
                self.blockers
                    .iter()
                    .map(|d| notice(NoticeLevel::Warning, d)),
            )
            .collect()
    }

    pub(crate) fn transformed(&self, transform: &Transform) -> Self {
        let mut next = self.clone();
        for panel in &mut next.panels {
            panel.frame = transform.apply_frame(&panel.frame);
        }
        for chart in &mut next.charts {
            chart.embedding = chart.embedding.transformed(transform);
        }
        for bend in &mut next.bends {
            if let SheetEmbedding::Cylinder(cylinder) =
                SheetEmbedding::Cylinder(bend.folded_embedding.clone()).transformed(transform)
            {
                bend.folded_embedding = cylinder;
            }
        }
        for overlay in &mut next.overlays {
            match &mut overlay.kind {
                SheetOverlayKind::Straight {
                    frame, direction, ..
                } => {
                    *frame = transform.apply_frame(frame);
                    *direction = transform.apply_vector(*direction);
                }
                SheetOverlayKind::Change { placement, .. } => {
                    *placement = transform.after(placement);
                }
            }
        }
        next
    }

    /// Why the sheet has no developed form: a change it keeps as an overlay
    /// (mitcad#242).
    pub(crate) fn overlay_refusal(&self, what: &str) -> Result<(), String> {
        match self.overlays.first() {
            Some(o) => Err(format!(
                "{what} needs the sheet's developed form, which {} does not have ({})",
                o.feature,
                match o.kind {
                    SheetOverlayKind::Straight { .. } =>
                        "a straight cut through a bend or an inclined panel",
                    SheetOverlayKind::Change { .. } =>
                        "a fillet or chamfer the material domains do not hold",
                }
            )),
            None => Ok(()),
        }
    }
}

/// Every replacement either supplies domains, deliberately transforms them,
/// or invalidates them. Absence also invalidates an existing sheet entry.
// (One change per changed body and feature: the size of a set state is not
// worth a box at every construction site.)
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum SheetChange {
    Set(SheetState),
    Transform(Transform),
    Invalidate(String),
    Remove,
}

pub(crate) type SheetStates = BTreeMap<BodyUid, crate::recompute::Versioned<SheetState>>;
pub(crate) type SheetRules =
    BTreeMap<FeatureUid, crate::recompute::Versioned<crate::features::sheet::SheetRulesDef<f64>>>;

fn version(state: &SheetState) -> u128 {
    let mut fp = crate::fingerprint::Fingerprint::new("sheet-state-v1");
    serde_json::to_writer(&mut fp, state).expect("sheet state serializes");
    fp.finish()
}

const ORDINARY_MODELLING: &str =
    "ordinary modeling replaced this body without a sheet-domain adapter";

/// The reason an invalidation stored, without the adapter's wording
/// (mitcad#1628): what about the feature's result the sheet cannot hold.
pub(crate) fn user_reason(message: &str) -> String {
    if message == ORDINARY_MODELLING {
        return "the feature has no sheet-metal form".into();
    }
    message
        .strip_prefix(rederive::NOT_DERIVED)
        .and_then(|rest| rest.strip_prefix(": "))
        .unwrap_or(message)
        .to_string()
}

/// Why a sheet that is not valid cannot be used, in the user's terms
/// (mitcad#1628, mitcad#1636, the rule of mitcad#1327): the feature that
/// took its sheet meaning away (or blocks it) by its name, and the stored
/// reason with walls named by their features; an invalid sheet points to
/// Sheet Cut, which cuts across bends and keeps the sheet. `name` gives a
/// feature's name as the timeline shows it. The one text both a sheet
/// feature's refusal (`EvalContext::require_sheet`) and the Flat Pattern's
/// reason (`flat::evaluate`, the Browser's Flat Pattern row) say; the ids
/// stay in the stored diagnostics (`sheet_state`, the Flat Pattern's
/// `causes`).
pub(crate) fn refusal(state: &SheetState, name: &dyn Fn(FeatureUid) -> String) -> String {
    let named = |message: &str| name_walls(message, name);
    if let Some(d) = &state.invalidated {
        let reason = user_reason(&d.message);
        return named(&format!(
            "this body is no longer sheet metal: {} changed it in a way its sheet metal \
             cannot hold ({reason}); to cut across bends, use Sheet Cut instead",
            name(d.feature)
        ));
    }
    match state.blockers.first() {
        Some(d) => named(&format!(
            "this body's sheet metal is blocked: {} failed on it",
            name(d.feature)
        )),
        None => "this body's sheet metal is valid".to_string(),
    }
}

/// A message with the walls it names (`panel F6 slot 0`, `bend F4 slot 0`:
/// two walls that meet, mitcad#1409) named by their features as the
/// timeline shows them: `SheetFlange3`, a further wall `SheetFlange3 (wall
/// 2)`, a bend `SheetFlange1 (bend)`. `name` gives a feature's name.
pub(crate) fn name_walls(message: &str, name: &dyn Fn(FeatureUid) -> String) -> String {
    let digits = |text: &str| text.bytes().take_while(u8::is_ascii_digit).count();
    let mut named = String::new();
    let mut rest = message;
    loop {
        let found = ["panel F", "bend F"]
            .iter()
            .filter_map(|key| rest.find(key).map(|at| (at, *key)))
            .min();
        let Some((at, key)) = found else {
            named.push_str(rest);
            return named;
        };
        named.push_str(&rest[..at]);
        let after = &rest[at + key.len()..];
        let feature_digits = digits(after);
        let slot_text = after[feature_digits..].strip_prefix(" slot ");
        let parsed = slot_text.and_then(|slot_text| {
            let slot_digits = digits(slot_text);
            let feature = after[..feature_digits].parse::<u64>().ok()?;
            let slot = slot_text[..slot_digits].parse::<u64>().ok()?;
            Some((feature, slot, &slot_text[slot_digits..]))
        });
        let Some((feature, slot, tail)) = parsed else {
            named.push_str(&rest[at..at + key.len()]);
            rest = after;
            continue;
        };
        let feature_name = name(FeatureUid(feature));
        let panel = key.starts_with("panel");
        named.push_str(&match (panel, slot) {
            (true, 0) => feature_name,
            (true, k) => format!("{feature_name} (wall {})", k + 1),
            (false, 0) => format!("{feature_name} (bend)"),
            (false, k) => format!("{feature_name} (bend {})", k + 1),
        });
        rest = tail;
    }
}

pub(crate) fn publish(
    states: &mut SheetStates,
    body: BodyUid,
    owner: ComponentUid,
    generation: u128,
    feature: FeatureUid,
    change: Option<&SheetChange>,
) {
    let previous = states.get(&body).map(|v| v.value.clone());
    let next = match change {
        Some(SheetChange::Set(state)) => Some(state.clone()),
        Some(SheetChange::Transform(t)) if t.is_rigid() => previous.map(|s| s.transformed(t)),
        Some(SheetChange::Remove) => None,
        _ => previous.map(|mut s| {
            // The operation that first took the sheet meaning away stays
            // named (mitcad#211: the imports' notes name it).
            if s.invalidated.is_none() {
                let message = match change {
                    Some(SheetChange::Invalidate(message)) => message.clone(),
                    _ => ORDINARY_MODELLING.into(),
                };
                s.invalidated = Some(SheetDiagnostic { feature, message });
            }
            s
        }),
    };
    if let Some(mut state) = next {
        state.body = body;
        state.owner = owner;
        state.shape_generation = format!("{generation:032x}");
        states.insert(
            body,
            crate::recompute::Versioned {
                version: version(&state),
                value: state,
            },
        );
    } else {
        states.remove(&body);
    }
}

pub(crate) fn block(
    states: &mut SheetStates,
    bodies: &[BodyUid],
    feature: FeatureUid,
    message: &str,
) {
    for body in bodies {
        if let Some(state) = states.get_mut(body) {
            state.value.blockers.retain(|b| b.feature != feature);
            state.value.blockers.push(SheetDiagnostic {
                feature,
                message: message.into(),
            });
            state.version = version(&state.value);
        }
    }
}

pub(crate) fn state_query<S>(
    result: &crate::recompute::Recomputed<S>,
    body: Option<BodyUid>,
) -> serde_json::Value {
    use serde_json::json;
    let one = |body: BodyUid| {
        let Some(state) = result.sheets.get(&body) else {
            return json!({"body":body,"status":"unavailable"});
        };
        let Some(shape) = result.bodies.get(&body) else {
            return json!({"body":body,"status":"unavailable"});
        };
        if state.value.shape_generation != format!("{:032x}", shape.version)
            || result.owners.get(&body) != Some(&state.value.owner)
        {
            return json!({"body":body,"status":"unavailable","diagnostics":["sheet state does not match current geometry"]});
        }
        let mut value = json!(state.value);
        value["status"] = json!(state.value.status());
        value["version"] = json!(format!("{:032x}", state.version));
        value["diagnostics"] = json!(
            state
                .value
                .invalidated
                .iter()
                .chain(&state.value.blockers)
                .map(|d| &d.message)
                .collect::<Vec<_>>()
        );
        // The diagnostics with their levels (mitcad#1040).
        value["notices"] = crate::notice::to_json(&state.value.notices());
        value
    };
    body.map_or_else(
        || json!(result.sheets.keys().map(|b| one(*b)).collect::<Vec<_>>()),
        one,
    )
}

pub(crate) fn rules_query<S>(
    result: &crate::recompute::Recomputed<S>,
    feature: Option<FeatureUid>,
) -> serde_json::Value {
    use serde_json::json;
    let one = |feature: FeatureUid| match result.sheet_rules.get(&feature) {
        Some(rules) => {
            json!({"feature":feature,"status":"valid","version":format!("{:032x}",rules.version),"rules":rules.value})
        }
        None => json!({"feature":feature,"status":"unavailable"}),
    };
    feature.map_or_else(
        || {
            json!(
                result
                    .sheet_rules
                    .keys()
                    .map(|f| one(*f))
                    .collect::<Vec<_>>()
            )
        },
        one,
    )
}

/// Copies and generic split pieces are ordinary solids until an explicit
/// sheet adapter exists. Keep that invalidity observable on the new body.
pub(crate) fn complete_changes<K: crate::kernel::Kernel>(
    ctx: &crate::features::EvalContext<'_, K>,
    output: &mut crate::features::FeatureOutput<K::Shape>,
) {
    let source = ctx.reads.iter().find_map(|r| match r {
        crate::recompute::Read::Sheet(body, _) => ctx.env.sheets.get(body),
        _ => None,
    });
    for change in &output.changes {
        if let crate::features::BodyChange::Set(body, _) = change
            && !ctx.env.sheets.contains_key(body)
            && !output.sheets.contains_key(body)
            && let Some(source) = source
        {
            let mut state = source.value.clone();
            state.body = *body;
            state.panels.clear();
            state.charts.clear();
            state.bends.clear();
            state.joins.clear();
            state.opened.clear();
            state.lineage.clear();
            state.overlays.clear();
            state.invalidated = Some(SheetDiagnostic {
                feature: ctx.uid,
                message: "a generic copy or split has no sheet-domain adapter".into(),
            });
            output.sheets.insert(*body, SheetChange::Set(state));
        }
    }
}

/// Declares intended mutation targets before resolving parameters or calling
/// the kernel. Failures therefore still block the affected material and keep
/// dependency reads, including empty automatic target sets.
pub(crate) fn footprint<K: crate::kernel::Kernel>(
    def: &crate::features::FeatureDef,
    ctx: &mut crate::features::EvalContext<'_, K>,
) {
    use crate::features::{FeatureDef as F, PatternObjects};
    let mut bodies = Vec::new();
    let mut sources = Vec::new();
    let mut all = false;
    if let Some(tool) = def.info().tool_use()
        && !tool.operation.makes_bodies()
    {
        all = tool.participants.is_empty();
        bodies.extend(tool.participants);
    }
    let objects = match def {
        F::SheetFace(d) => {
            bodies.extend(d.body);
            None
        }
        F::SheetFlange(d) => {
            bodies.push(d.body);
            None
        }
        F::SheetBend(d) => {
            bodies.extend([d.body, d.moving_body.unwrap_or(d.body)]);
            None
        }
        F::SheetFold(d) => {
            bodies.push(d.body);
            None
        }
        F::SheetCornerRound(d) => {
            bodies.push(d.body);
            None
        }
        F::SheetCornerChamfer(d) => {
            bodies.push(d.body);
            None
        }
        F::SheetUnfold(d) => {
            bodies.push(d.body);
            None
        }
        F::SheetRefold(d) => {
            bodies.push(d.body);
            None
        }
        F::SheetCornerRelief(d) => {
            bodies.push(d.body);
            None
        }
        F::SheetCut(d) => {
            bodies.push(d.body);
            None
        }
        F::SheetPerforate(d) => {
            bodies.push(d.body);
            None
        }
        F::SheetConvert(d) => {
            bodies.push(d.body);
            None
        }
        F::Fillet(d) => {
            bodies.push(d.body);
            None
        }
        F::Chamfer(d) => {
            bodies.push(d.body);
            None
        }
        F::Shell(d) => {
            bodies.push(d.body);
            None
        }
        F::Draft(d) => {
            bodies.push(d.body);
            None
        }
        F::OffsetFace(d) => {
            bodies.push(d.body);
            None
        }
        F::DeleteFace(d) => {
            bodies.push(d.body);
            None
        }
        F::ReplaceFace(d) => {
            bodies.push(d.body);
            None
        }
        F::SplitFace(d) => {
            bodies.push(d.body);
            None
        }
        F::SplitBody(d) => {
            bodies.extend(&d.bodies);
            None
        }
        F::Move(d) => {
            if d.copy {
                sources.extend(&d.bodies);
            } else {
                bodies.extend(&d.bodies);
            }
            None
        }
        F::Align(d) => {
            bodies.extend(&d.bodies);
            None
        }
        F::Scale(d) => {
            bodies.extend(&d.bodies);
            None
        }
        F::ComponentFromBodies(d) => {
            bodies.extend(&d.bodies);
            None
        }
        F::BodyTransfer(d) => {
            bodies.extend(&d.bodies);
            None
        }
        F::RemoveBody(d) => {
            bodies.extend(&d.bodies);
            None
        }
        F::Combine(d) => {
            bodies.push(d.target);
            None
        }
        F::Thread(d) if d.modeled => {
            bodies.extend(d.faces.iter().map(|f| f.body));
            None
        }
        F::Base(d) => {
            bodies.extend(&d.replaces);
            None
        }
        F::Hole(d) => {
            all = d.participants.is_empty();
            bodies.extend(&d.participants);
            None
        }
        // Patterns that join copies of bodies to them change those bodies
        // (mitcad#539), as a mirror's `combine` does.
        F::RectangularPattern(d) => {
            if d.combine
                && let PatternObjects::Bodies { bodies: selected } = &d.objects
            {
                bodies.extend(selected);
            }
            Some(&d.objects)
        }
        F::CircularPattern(d) => {
            if d.combine
                && let PatternObjects::Bodies { bodies: selected } = &d.objects
            {
                bodies.extend(selected);
            }
            Some(&d.objects)
        }
        F::PathPattern(d) => {
            if d.combine
                && let PatternObjects::Bodies { bodies: selected } = &d.objects
            {
                bodies.extend(selected);
            }
            Some(&d.objects)
        }
        F::ScalePattern(d) => {
            if d.combine
                && let PatternObjects::Bodies { bodies: selected } = &d.objects
            {
                bodies.extend(selected);
            }
            Some(&d.objects)
        }
        F::Mirror(d) => {
            if d.combine
                && let PatternObjects::Bodies { bodies: selected } = &d.objects
            {
                bodies.extend(selected);
            }
            Some(&d.objects)
        }
        F::PatternAtPoints(d) => Some(&d.objects),
        F::Extrude(d) if d.active_cut_scope().is_some() => {
            all = false;
            let scope = d.active_cut_scope().expect("checked");
            ctx.reads.push(crate::recompute::Read::Assembly(
                crate::features::extrude_scope::assembly_key(&ctx.env),
            ));
            let components: Vec<_> = std::iter::once(ComponentUid::ROOT)
                .chain(ctx.env.assembly.components.iter().map(|c| c.uid))
                .collect();
            for component in components {
                // Discovery reads the set even when empty: a repaired or new
                // participant invalidates the failure cache and its blockers.
                let candidates = ctx.bodies_in(component);
                let selected = match &scope.targets {
                    crate::features::extrude_scope::TargetScope::Automatic { excluded, .. } => {
                        !excluded.iter().any(|t| t.component == component)
                    }
                    crate::features::extrude_scope::TargetScope::Explicit { included, .. } => {
                        included.iter().any(|t| t.component == component)
                    }
                };
                if selected {
                    bodies.extend(
                        candidates
                            .into_iter()
                            .map(|(b, _)| b)
                            .filter(|b| !scope.excluded_bodies().iter().any(|e| e.body == *b)),
                    );
                }
            }
            None
        }
        // Boundary Fill (mitcad#457): its participants, and the tools it
        // removes.
        F::BoundaryFill(d) => {
            if !d.operation.makes_bodies() {
                all = d.participants.is_empty();
                bodies.extend(&d.participants);
            }
            if d.remove_tools {
                bodies.extend(&d.tools);
            }
            None
        }
        // Plastic part features (mitcad#459); Rest and Snap Fit say so by
        // their tool use.
        F::Boss(d) => {
            bodies.push(d.body);
            None
        }
        F::Lip(d) => {
            bodies.push(d.body);
            None
        }
        F::Groove(d) => {
            bodies.push(d.body);
            None
        }
        _ => None,
    };
    if let Some(objects) = objects {
        match objects {
            PatternObjects::Bodies { bodies: selected } => sources.extend(selected),
            PatternObjects::Faces { body, .. } => bodies.push(*body),
            PatternObjects::Features { features } => {
                for uid in features {
                    if let Some(entry) = ctx.env.feature(*uid).cloned() {
                        footprint(&entry.def, ctx);
                    }
                }
            }
            PatternObjects::Components { .. } => {}
        }
    }
    if all {
        bodies.extend(ctx.bodies().into_iter().map(|(b, _)| b));
    }
    for body in sources {
        ctx.sheet(body);
    }
    bodies.sort();
    bodies.dedup();
    for body in bodies {
        if let Some(owner) = ctx.env.owners.get(&body).copied() {
            let version = ctx
                .env
                .components
                .get(&owner)
                .and_then(|b| b.get(&body))
                .map(|b| b.version);
            ctx.reads
                .push(crate::recompute::Read::BodyIn(owner, body, version));
        } else {
            ctx.reads.push(crate::recompute::Read::Body(body, None));
        }
        ctx.sheet(body);
        ctx.sheet_footprint.insert(body);
    }
}
