// SPDX-License-Identifier: MIT
//! Connectors at junctions (`build_connect`), panels in a frame
//! (`build_panel`) and removing BUILD parts (`build_remove`).
//!
//! A connector is placed with its first mate's frame on profile `a` (the
//! one whose end meets), on a's side that faces into the frame, at the
//! distance from b's face its second mate's frame lies at, so its other
//! arm lies on profile `b`; its fasteners (nut, screw, washer) go on their
//! frames, a nut into the slot, and the holes it needs are
//! cut into the profiles' derived components. A panel in a frame is sized
//! to the room between the profiles around the picked inner face plus the
//! slots' depth less the clearance (bound lengths keep it so), or keeps a
//! fixed width and says whether it fits; a panel on an outer face lies on
//! it, sized to the outer sides of the frame around it (bound lengths
//! through the profiles keep it so), with its clamps in the picked slot.

use std::collections::{BTreeMap, BTreeSet};
use std::f64::consts::PI;

use serde::Deserialize;
use serde_json::json;

use crate::geometry::{
    V3, add, box_along, dot, inverse, norm, parts, scale, sub, unit, x_axis, z_axis,
};
use crate::junctions::Junction;
use crate::parts::{PartInfo, source_state};
use crate::place::{
    PartInput, PlaceAt, Placed, SourceUse, add_bound, add_feature, bounds_of, current_bounds,
    detach, refuse, value_in,
};
use crate::prelude::*;
use crate::{BuildItem, BuildMeta, BuildRecord, BuildRole, LibrarySourceDef, PartRef};
use mitcad_model::expr::Unit;
use mitcad_model::features::bound_length::{BoundMode, BoundTo, RayDirection};
use mitcad_model::features::derived_part::{DerivedPartDef, Ends};
use mitcad_model::features::extrude::Operation;
use mitcad_model::features::{
    FeatureDef, FrameOverride, GeomRef, JointDef, JointOrigin, OccurrencePath, ValueInput,
};
use mitcad_model::ids::{ComponentUid, FeatureUid, OccurrenceUid};
use mitcad_model::internal::features::evaluate_source;
use mitcad_model::joints::{JointKind, Motion, SlideAxis};
use mitcad_model::kernel::{BoundingBox, Kernel};
use mitcad_model::library::LibraryRequest;
use mitcad_model::transform::Transform;
use mitcad_model::{DocState, Document, ModelError};

/// The fields of `build_connect`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConnectInput {
    /// A junction (`build_junctions`' id); a post base names `end` instead.
    #[serde(default)]
    pub junction: String,
    /// The connector or wood fastener; left out with `joint`.
    #[serde(default)]
    pub part: PartInput,
    #[serde(default = "yes")]
    pub fasteners: bool,
    #[serde(default = "yes")]
    pub machining: bool,
    #[serde(default)]
    pub name: Option<String>,
    // Timber construction (mitcad#478).
    /// How many screws or nails (default 2).
    #[serde(default)]
    pub count: Option<u32>,
    /// An angle bracket's side of member a: `x+`, `x-`, `y+`, `y-`.
    #[serde(default)]
    pub side: Option<String>,
    /// A post base's member end.
    #[serde(default)]
    pub end: Option<crate::wood::EndInput>,
    /// Joinery instead of a part.
    #[serde(default)]
    pub joint: Option<crate::joinery::JointInput>,
}

fn yes() -> bool {
    true
}

/// The fields of `build_panel`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PanelInput {
    pub at: PlaceAt,
    pub part: PartInput,
    #[serde(default)]
    pub thickness: Option<ValueInput>,
    #[serde(default)]
    pub parameters: BTreeMap<String, ValueInput>,
    #[serde(default)]
    pub name: Option<String>,
    /// A sheet on a face (mitcad#478): more faces it covers.
    #[serde(default)]
    pub faces: Vec<crate::BindInput>,
}

/// What `build_remove` removed.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RemoveResult {
    pub components: Vec<ComponentUid>,
    pub features: Vec<FeatureUid>,
    /// Bound lengths of other parts that ended on it, detached first.
    pub detached: Vec<FeatureUid>,
}

/// A part made from a source and placed by a joint: what connectors,
/// fasteners and panels add.
#[derive(Debug, Clone)]
pub struct Piece {
    pub source: SourceUse,
    pub values: BTreeMap<String, ValueInput>,
    pub name: String,
    /// The joint: its kind and axis, the piece's frame (a mount, or its
    /// design's origin when None), the other side, the flip, the angle and
    /// a slide.
    pub kind: JointKind,
    pub slide: SlideAxis,
    pub own: Option<String>,
    pub own_override: Option<FrameOverride>,
    pub b: JointOrigin,
    pub flip: bool,
    pub angle: f64,
    pub slide_value: Option<(Motion, ValueInput)>,
    pub record: BuildRecord,
}

/// Holes a connector or a part's own ends need: the profile, its frame and
/// the hole, as a hole feature's definition.
#[derive(Debug, Clone)]
pub struct Hole {
    pub component: ComponentUid,
    pub def: serde_json::Value,
}

/// The rule a connector is placed by (mitcad#1115).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectRule {
    /// Before mitcad#1115: on a's side facing b's own axis at every junction
    /// on b's side, from a's end; a nut flipped, a screw's head on the
    /// profile's face. A connector or fastener without `rule` in its record
    /// was placed so.
    First,
    /// Inside the frame at every corner, seated on b's face; a nut into the
    /// slot, a screw's head on the connector's wall.
    Current,
}

/// The `rule` in the record of a connector or fastener placed by the
/// current rule ([`ConnectRule::Current`]).
pub const CONNECT_RULE: u32 = 2;

/// A connector planned: the piece, its fasteners, its holes and where it
/// goes in the BUILD host's coordinates.
#[derive(Debug, Clone)]
pub struct ConnectPlan {
    pub connector: Piece,
    pub fasteners: Vec<Piece>,
    pub holes: Vec<Hole>,
    pub placement: Transform,
    /// The fasteners' frames there.
    pub fastener_frames: Vec<(String, Transform)>,
    pub warnings: Vec<String>,
    pub designation: String,
    pub junction: Junction,
}

/// A panel planned.
#[derive(Debug, Clone)]
pub struct PanelPlan {
    pub panel: Piece,
    pub bounds: Vec<PanelBound>,
    pub clamps: Vec<Piece>,
    pub fits: bool,
    pub reason: String,
    pub width: f64,
    pub height: f64,
    pub placement: Transform,
    pub warnings: Vec<String>,
    pub designation: String,
}

/// A bound length that sizes a panel: the parameter, the ray and its
/// target.
#[derive(Debug, Clone)]
pub struct PanelBound {
    pub parameter: String,
    pub from: JointOrigin,
    pub to: BoundTo,
    pub mode: BoundMode,
    pub add: f64,
    pub first: f64,
}

/// The joint origin of a part's frame in the BUILD host.
fn origin_of(path: &[OccurrenceUid], part: FeatureUid, name: &str) -> JointOrigin {
    JointOrigin {
        occurrence: OccurrencePath(path.to_vec()),
        geometry: GeomRef::Mount {
            feature: part,
            name: name.to_owned(),
        },
        frame_override: None,
    }
}

/// A library reference resolved against the library it came with: its
/// URL and version when it names the same library, else the library's
/// default branch.
pub(crate) fn request_of(reference: &PartRef, with: &LibrarySourceDef) -> PartInput {
    let same = reference.library == with.library.library || reference.library.is_empty();
    PartInput {
        library: Some(if reference.library.is_empty() {
            with.library.library.clone()
        } else {
            reference.library.clone()
        }),
        url: if !reference.url.is_empty() {
            reference.url.clone()
        } else if same {
            with.library.url.clone()
        } else {
            String::new()
        },
        rev: Some(if !reference.rev.is_empty() {
            reference.rev.clone()
        } else if same {
            with.library.rev.clone()
        } else {
            "HEAD".to_owned()
        }),
        component: Some(reference.component.clone()),
        config: reference.config.clone(),
        source: None,
        parameters: BTreeMap::new(),
    }
}

/// Makes a piece in `state`: its source (when new), component, occurrence,
/// derived part and joint; returns the component and the occurrence.
pub(crate) fn make_piece(
    state: &mut DocState,
    piece: &Piece,
    active: ComponentUid,
) -> Result<(ComponentUid, OccurrenceUid, FeatureUid), ModelError> {
    let id = if piece.source.new {
        match state.library_sources.find(
            &piece.source.def.library.library,
            &piece.source.def.library.component,
            &piece.source.def.library.rev,
        ) {
            Some(id) => id,
            None => {
                let mut def = piece.source.def.clone();
                def.id = state.library_sources.next_id();
                let id = def.id;
                state.library_sources.0.push(def);
                id
            }
        }
    } else {
        piece.source.def.id
    };
    let name = state.free_component_name(&piece.name);
    let made = state.assembly.create(Some(&name), None);
    let occurrence = state.assembly.place(made, active, Transform::IDENTITY);
    let part = add_feature(
        state,
        FeatureDef::DerivedPart(DerivedPartDef {
            source: id,
            config: piece.source.config.clone(),
            parameters: piece.values.clone(),
            ends: Ends { a: None, b: None },
            operation: Operation::NewBody,
            cuts: Vec::new(),
        }),
        made,
    )?;
    crate::parts::inherit_appearance(state, part, &piece.source.def);
    let mut position = BTreeMap::new();
    if let Some((motion, value)) = &piece.slide_value {
        position.insert(*motion, value.clone());
    }
    let own = match &piece.own {
        Some(name) => GeomRef::Mount {
            feature: part,
            name: name.clone(),
        },
        None => GeomRef::Origin(mitcad_model::datum::OriginDatum::Xy),
    };
    let joint = add_feature(
        state,
        FeatureDef::Joint(JointDef {
            kind: piece.kind,
            slide_axis: piece.slide,
            a: JointOrigin {
                occurrence: OccurrencePath(vec![occurrence]),
                geometry: own,
                frame_override: piece.own_override,
            },
            b: piece.b.clone(),
            offset: None,
            angle: (piece.angle.abs() > 1e-12).then_some(ValueInput::Number(piece.angle)),
            flip: piece.flip,
            limits: BTreeMap::new(),
            position,
        }),
        active,
    )?;
    let mut record = piece.record.clone();
    record.source = Some(id);
    record.part = Some(part);
    record.joint = Some(joint);
    state.assembly.component_mut(made).expect("made").build = Some(record);
    Ok((made, occurrence, part))
}

/// The new part's evaluated frames and box in its own coordinates.
pub(crate) fn evaluated<K: Kernel>(
    doc: &Document<K>,
    source: &SourceUse,
    values: &[(String, f64)],
) -> Result<(BTreeMap<String, Transform>, Option<BoundingBox>), ModelError> {
    let label = source.def.label();
    let state = source_state(&source.def, source.config.as_deref(), values)
        .map_err(|e| refuse(format!("{label}: {e}")))?;
    let evaluated =
        evaluate_source(doc.kernel(), &state).map_err(|e| refuse(format!("{label}: {e}")))?;
    let bounds = bounds_of(doc.kernel(), &evaluated.bodies);
    Ok((evaluated.mounts, bounds))
}

/// A part's frames by name and its bodies, in its own coordinates.
type FramesAndBodies<S> = (BTreeMap<String, Transform>, Vec<S>);

/// The new part's evaluated frames and bodies in its own coordinates.
pub(crate) fn evaluated_bodies<K: Kernel>(
    doc: &Document<K>,
    source: &SourceUse,
    values: &[(String, f64)],
) -> Result<FramesAndBodies<K::Shape>, ModelError> {
    let label = source.def.label();
    let state = source_state(&source.def, source.config.as_deref(), values)
        .map_err(|e| refuse(format!("{label}: {e}")))?;
    let evaluated =
        evaluate_source(doc.kernel(), &state).map_err(|e| refuse(format!("{label}: {e}")))?;
    Ok((evaluated.mounts, evaluated.bodies))
}

/// What a piece of a connector's hardware is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Hardware {
    Nut,
    Screw,
    Washer,
}

/// A piece of hardware on one of a connector's frames, read: what it is,
/// whether it is held by its `slot` frame (a nut), and its box in its own
/// coordinates.
struct Held {
    kind: Hardware,
    in_slot: bool,
    bounds: Option<BoundingBox>,
}

/// A connector's fastener as `build_upgrade` reads it: its component, its
/// joint and the joint's definition, and what it is.
type HeldJoint = (ComponentUid, FeatureUid, JointDef<ValueInput>, Held);

/// How each piece of hardware on a connector's frame `frame` (in the
/// connector's coordinates) is held there (mitcad#1115): its joint's flip
/// and its own frame's override, and whether the connector's wall was
/// found. A nut's slot frame points into the profile, as the connector's
/// frame does (docs/libraries.md, "The direction of a frame's z axis"):
/// the two agree, not flipped, so that the nut lies in the slot. A screw's
/// or a washer's origin is its head's underside (a washer's face), z out
/// of the head: flipped, its shank into the profile, and moved back by
/// the connector's wall behind the frame, so that it bears on the wall's
/// outer face; a screw also by the washer under it.
fn holds<K: Kernel>(
    doc: &Document<K>,
    bodies: &[K::Shape],
    frame: &Transform,
    held: &[Held],
) -> (Vec<(bool, Option<FrameOverride>)>, bool) {
    let loose: Vec<&Held> = held.iter().filter(|h| !h.in_slot).collect();
    // Under the head: beside the hole, at nine tenths of the head's radius.
    let radius = loose
        .iter()
        .filter_map(|h| h.bounds)
        .map(|b| {
            [b.min[0], b.max[0], b.min[1], b.max[1]]
                .into_iter()
                .fold(0.0_f64, |r, v| r.max(v.abs()))
        })
        .fold(0.0_f64, f64::max);
    let reach = bounds_of(doc.kernel(), bodies).map_or(0.0, |b| norm(sub(b.max, b.min)));
    let wall = if loose.is_empty() {
        None
    } else {
        wall_behind(doc, bodies, frame, 0.9 * radius, reach)
    };
    let washer = loose
        .iter()
        .find(|h| h.kind == Hardware::Washer)
        .and_then(|h| h.bounds)
        .map_or(0.0, |b| b.max[2] - b.min[2]);
    let out = held
        .iter()
        .map(|h| {
            if h.in_slot {
                return (false, None);
            }
            let back = wall.unwrap_or(0.0)
                + if h.kind == Hardware::Screw {
                    washer
                } else {
                    0.0
                };
            let shift = (back > 1e-9).then_some(FrameOverride {
                shift: Some([0.0, 0.0, -back]),
                ..FrameOverride::default()
            });
            (true, shift)
        })
        .collect();
    (out, wall.is_some() || loose.is_empty())
}

/// How thick the connector's wall is behind its frame `frame` (in its
/// coordinates), along the frame's -z, away from the profile (mitcad#1115):
/// of segments from eight points `radius` around the frame's origin (beside
/// a hole on its axis), each from half a millimetre in front of the
/// frame's plane ([`Kernel::segment_crossings`]: the `.f3d` import's,
/// mitcad#68), those that enter the material at the frame's plane, the
/// nearest point where one leaves it. None when none enters there.
fn wall_behind<K: Kernel>(
    doc: &Document<K>,
    bodies: &[K::Shape],
    frame: &Transform,
    radius: f64,
    reach: f64,
) -> Option<f64> {
    const LEAD: f64 = 0.5;
    let [origin, x, y, z] = parts(frame);
    let back = scale(z, -1.0);
    let span = LEAD + reach;
    let mut wall: Option<f64> = None;
    for k in 0..8 {
        let turn = f64::from(k) * PI / 4.0;
        let start = add(
            origin,
            add(scale(x, radius * turn.cos()), scale(y, radius * turn.sin())),
        );
        let from = sub(start, scale(back, LEAD));
        let to = add(start, scale(back, reach));
        for body in bodies {
            let Ok(hits) = doc.kernel().segment_crossings(body, from, to) else {
                continue;
            };
            // Distances from the frame's plane, along -z.
            let along: Vec<(f64, bool)> = hits
                .iter()
                .map(|(t, enters)| (t * span - LEAD, *enters))
                .collect();
            if let [(enter, true), (leave, false), ..] = along.as_slice()
                && enter.abs() <= 1e-3
            {
                wall = Some(wall.map_or(*leave, |w| w.min(*leave)));
            }
        }
    }
    wall
}

/// The own frame a fastener source is held by, and whether it is its
/// `slot` frame (a nut).
fn own_hold(used: &SourceUse, mounts: &BTreeMap<String, Transform>) -> (Option<String>, bool) {
    let own = crate::place::own_frame(&used.meta, mounts, &["slot", "hole", "face"])
        .or_else(|| mounts.keys().next().cloned());
    let in_slot = own.as_deref().is_some_and(|name| {
        used.meta
            .frames
            .iter()
            .any(|f| f.name == name && f.kind == "slot")
    });
    (own, in_slot)
}

/// A connector's fasteners on its frames (`placement` the connector's, for
/// the frames' places), held as [`holds`] says; what cannot be read goes
/// into `warnings`.
#[allow(clippy::type_complexity)]
fn fastener_pieces<K: Kernel>(
    doc: &Document<K>,
    source: &SourceUse,
    mounts: &BTreeMap<String, Transform>,
    bodies: &[K::Shape],
    placement: &Transform,
    warnings: &mut Vec<String>,
) -> Result<(Vec<Piece>, Vec<(String, Transform)>), ModelError> {
    let label = source.def.label();
    let connector = source.meta.connector.clone().unwrap_or_default();
    let mut pieces = Vec::new();
    let mut frames = Vec::new();
    for fastener in &connector.fasteners {
        let Some(frame) = mounts.get(&fastener.frame) else {
            warnings.push(format!("{label} has no frame {}", fastener.frame));
            continue;
        };
        frames.push((fastener.frame.clone(), placement.after(frame)));
        // All of the frame's hardware read first: the washer's thickness
        // and the head's size go into the seats.
        let mut read = Vec::new();
        for (kind, reference) in [
            (Hardware::Nut, &fastener.nut),
            (Hardware::Screw, &fastener.screw),
            (Hardware::Washer, &fastener.washer),
        ] {
            let Some(reference) = reference else {
                continue;
            };
            let part = request_of(reference, &source.def);
            let used = match doc.fastener_source(&part) {
                Ok(used) => used,
                Err(e) => {
                    warnings.push(format!("fastener {} is left out: {e}", reference.component));
                    continue;
                }
            };
            let (fmounts, fbounds) = evaluated(doc, &used, &[])?;
            let (own, in_slot) = own_hold(&used, &fmounts);
            read.push((
                Held {
                    kind,
                    in_slot,
                    bounds: fbounds,
                },
                used,
                own,
            ));
        }
        let held: Vec<Held> = read
            .iter()
            .map(|(h, _, _)| Held {
                kind: h.kind,
                in_slot: h.in_slot,
                bounds: h.bounds,
            })
            .collect();
        let (ways, found) = holds(doc, bodies, frame, &held);
        if !found {
            warnings.push(format!(
                "{label}: no wall found behind {}; the screw's head is at the profile's face",
                fastener.frame
            ));
        }
        for ((_, used, own), (flip, own_override)) in read.into_iter().zip(ways) {
            let flabel = used.def.label();
            let designation =
                crate::parts::designation(&used.def, used.config.as_deref(), &BTreeMap::new());
            pieces.push(Piece {
                name: if designation.is_empty() {
                    flabel
                } else {
                    designation
                },
                source: used,
                values: BTreeMap::new(),
                kind: JointKind::Rigid,
                slide: SlideAxis::Z,
                own,
                own_override,
                // On the connector, once it exists (its occurrence and part
                // fill in when it is made).
                b: JointOrigin {
                    occurrence: OccurrencePath::default(),
                    geometry: GeomRef::Mount {
                        feature: mitcad_model::ids::FeatureUid(0),
                        name: fastener.frame.clone(),
                    },
                    frame_override: None,
                },
                flip,
                angle: 0.0,
                slide_value: None,
                record: BuildRecord {
                    role: BuildRole::Fastener,
                    rule: Some(CONNECT_RULE),
                    ..BuildRecord::default()
                },
            });
        }
    }
    Ok((pieces, frames))
}

/// Whether two placements are the same within a nanometre.
fn same_place(a: &Transform, b: &Transform) -> bool {
    parts(a)
        .iter()
        .zip(parts(b).iter())
        .all(|(p, q)| (0..3).all(|i| (p[i] - q[i]).abs() <= 1e-6))
}

/// A source the design holds, as BUILD uses it; one without `build` (a
/// screw) as a fastener.
fn held_source<K: Kernel>(
    doc: &Document<K>,
    id: crate::SourceId,
    config: Option<String>,
) -> Option<SourceUse> {
    let part = PartInput {
        source: Some(id),
        config: config.clone(),
        ..PartInput::default()
    };
    if let Ok(used) = doc.source_use(&part) {
        return Some(used);
    }
    let def = doc.state().library_sources.get(id).cloned()?;
    Some(SourceUse {
        meta: BuildMeta {
            role: BuildRole::Fastener,
            ..BuildMeta::default()
        },
        def,
        new: false,
        config,
    })
}

/// A joint's definition in command form.
fn joint_input<K: Kernel>(doc: &Document<K>, joint: FeatureUid) -> Option<JointDef<ValueInput>> {
    let entry = doc.feature(joint)?;
    match crate::parts::as_input(&entry.def, doc.parameters()) {
        FeatureDef::Joint(def) => Some(def),
        _ => None,
    }
}

/// The document's BUILD methods of this block, as an extension trait
/// (mitcad#777): BUILD planning lives in a crate above the model.
pub trait ConnectBuild<K: Kernel> {
    /// Plans a connector at a junction.
    fn plan_connect(&self, input: &ConnectInput) -> Result<ConnectPlan, ModelError> {
        self.plan_connect_by(input, ConnectRule::Current)
    }

    /// Plans a connector at a junction by a placing rule (mitcad#1115):
    /// the current one, or the first one, to tell a connector it placed.
    fn plan_connect_by(
        &self,
        input: &ConnectInput,
        rule: ConnectRule,
    ) -> Result<ConnectPlan, ModelError>;

    /// Places the connectors and their fasteners that the first placing
    /// rule placed by the current one (mitcad#1115), in one undo step: a
    /// connector only where it is still where that rule put it (one moved
    /// since stays), a nut or a screw only while its joint is as that rule
    /// made it. Returns the components placed again; nothing changes (no
    /// recompute, no undo step) when there are none.
    fn build_upgrade(&mut self) -> Result<Vec<ComponentUid>, ModelError>;

    /// Places a connector at a junction (`build_connect`).
    fn build_connect(&mut self, input: &ConnectInput) -> Result<Placed, ModelError>;

    /// Plans a panel on a picked face of a frame's profile.
    fn plan_panel(&self, input: &PanelInput) -> Result<PanelPlan, ModelError>;

    /// Places a panel (`build_panel`).
    fn build_panel(&mut self, input: &PanelInput) -> Result<Placed, ModelError>;

    /// Removes a BUILD part (`build_remove`): the component, what it
    /// added, its bound lengths, and the bound lengths of other parts that
    /// end on it or start from it (detached first, so those parts keep their
    /// lengths). Parts placed on it stay where they are. With `set`, every
    /// member of the component's spacing set goes (mitcad#478).
    fn build_remove_with(&mut self, component: &str, set: bool)
    -> Result<RemoveResult, ModelError>;
}

impl<K: Kernel> ConnectBuild<K> for Document<K> {
    fn plan_connect_by(
        &self,
        input: &ConnectInput,
        rule: ConnectRule,
    ) -> Result<ConnectPlan, ModelError> {
        let junction = self
            .build_junctions(None)
            .into_iter()
            .find(|j| j.id == input.junction)
            .ok_or_else(|| refuse(format!("no junction {}", input.junction)))?;
        let source = self.source_use(&input.part)?;
        let label = source.def.label();
        if source.meta.role != BuildRole::Connector {
            return Err(refuse(format!("{label} is no connector")));
        }
        let connector = source.meta.connector.clone().unwrap_or_default();
        if !connector.junctions.is_empty()
            && !connector.junctions.iter().any(|k| *k == junction.kind)
        {
            return Err(refuse(format!(
                "{label} does not go on an {} junction (it goes on {})",
                junction.kind,
                connector.junctions.join(", ")
            )));
        }
        let a = self
            .part_info(junction.a.component)
            .ok_or_else(|| refuse("the junction's parts are gone"))?;
        let b = self
            .part_info(junction.b.component)
            .ok_or_else(|| refuse("the junction's parts are gone"))?;
        for profile in [&a, &b] {
            if !source.meta.fits(profile.meta.system.as_ref()) {
                return Err(refuse(format!(
                    "the systems do not fit: {label} fits {}, {} is {}",
                    source
                        .meta
                        .systems
                        .iter()
                        .map(|s| s.label())
                        .collect::<Vec<_>>()
                        .join(", "),
                    profile.name,
                    profile
                        .meta
                        .system
                        .as_ref()
                        .map_or_else(|| "of no system".to_owned(), |s| s.label())
                )));
            }
        }
        if let Some(envelope) = &connector.envelope {
            let room = junction.space;
            let need = [&envelope.x, &envelope.y, &envelope.z]
                .map(|v| v.as_ref().and_then(|v| v.millimetres()).unwrap_or(0.0));
            if (0..3).any(|i| need[i] > room[i] + 1e-6) {
                return Err(refuse(format!(
                    "{label} does not fit in the junction's inner corner"
                )));
            }
        }
        let (mounts, bodies) = evaluated_bodies(self, &source, &[])?;
        // The mates: the first on a, the second (or one named for b) on b.
        let mate_a = connector
            .mates
            .iter()
            .find(|m| !m.frame.ends_with('b'))
            .or_else(|| connector.mates.first())
            .cloned()
            .ok_or_else(|| refuse(format!("{label} lists no mates")))?;
        let mate_b = connector
            .mates
            .iter()
            .find(|m| m.frame != mate_a.frame)
            .cloned();
        let own_a = *mounts
            .get(&mate_a.frame)
            .ok_or_else(|| refuse(format!("{label} has no frame {}", mate_a.frame)))?;
        let a_path = self
            .placed_in_host(a.component)
            .ok_or_else(|| refuse(format!("{} is not placed in the BUILD host", a.name)))?;
        let a_part = a
            .part
            .ok_or_else(|| refuse(format!("{} has no derived part", a.name)))?;
        let a_end = junction.a.at.clone();
        let a_end_b = a_end == "end_b";
        // a's frame of the mate's kind facing the inner corner: the one
        // turned most towards b.
        let world_a = self.host_placement().after(&self.path_transform(&a_path));
        // The first rule took b's own axis on b's side, wherever along b
        // the junction was (mitcad#1115).
        let on_side = !matches!(junction.b.at.as_str(), "end_a" | "end_b");
        let b_axis = match rule {
            ConnectRule::First if on_side => world_of(self, &b).apply_vector([0.0, 0.0, 1.0]),
            _ => junction.b_axis,
        };
        let candidates: Vec<(&String, &Transform)> = a
            .mounts
            .iter()
            .filter(|(n, _)| a.frame_kind(n) == mate_a.to)
            .collect();
        let (a_frame_name, a_frame) = candidates
            .iter()
            .max_by(|x, y| {
                let d = |t: &Transform| dot(world_a.apply_vector(z_axis(t)), b_axis);
                d(x.1).total_cmp(&d(y.1))
            })
            .map(|(n, t)| ((*n).clone(), **t))
            .ok_or_else(|| refuse(format!("{} has no {} frame for {label}", a.name, mate_a.to)))?;
        // How far from the corner the first mate is, and which way its arm
        // to b turns.
        let (h, turn) = match &mate_b {
            Some(m) => match mounts.get(&m.frame) {
                Some(own_b) => {
                    let n_b = z_axis(own_b);
                    let h = dot(sub(own_a.translation, own_b.translation), n_b).abs();
                    let c = dot(x_axis(&own_a), n_b);
                    let turn = if a_end_b { c < 0.0 } else { c > 0.0 };
                    (h, turn)
                }
                None => (0.0, false),
            },
            None => (0.0, false),
        };
        // Where b's face towards a crosses a's slot, from a's end A: the
        // connector's corner seats there (mitcad#1115). On b's side that is
        // a's end itself; where two mitred ends meet, b's inner face lies
        // a section further in. Only along a slot: its x runs along a.
        let slot = world_a.after(&a_frame);
        let along = x_axis(&slot);
        let towards_b = if a_end_b { along } else { scale(along, -1.0) };
        let seat = if rule == ConnectRule::Current && a.frame_kind(&a_frame_name) == "slot" {
            face_along(self, &b, slot.translation, along, towards_b)
        } else {
            None
        };
        let (value, tx) = match (a_end_b, a.length) {
            (true, Some((id, l))) => {
                let back = seat.map_or(0.0, |s| snap(l - s));
                let d = h + back;
                (
                    ValueInput::Name(format!(
                        "{} - {}",
                        self.parameters().name(id),
                        mitcad_model::internal::build::round(d)
                    )),
                    l - d,
                )
            }
            (true, None) => (ValueInput::Number(h), h),
            (false, _) => {
                let d = h + seat.map_or(0.0, snap);
                (ValueInput::Number(d), d)
            }
        };
        let angle = if turn { PI } else { 0.0 };
        let frame_b = self
            .path_transform(&a_path)
            .after(&a_frame)
            .after(&Transform::translation([tx, 0.0, 0.0]));
        let placement = frame_b
            .after(&mitcad_model::internal::joints::alignment(angle, 0.0, true))
            .after(&inverse(&own_a));
        let designation =
            crate::parts::designation(&source.def, source.config.as_deref(), &BTreeMap::new());
        let name = input.name.clone().unwrap_or_else(|| {
            if designation.is_empty() {
                label.clone()
            } else {
                designation.clone()
            }
        });
        let connector_piece = Piece {
            source: source.clone(),
            values: BTreeMap::new(),
            name,
            kind: JointKind::Slider,
            slide: SlideAxis::X,
            own: Some(mate_a.frame.clone()),
            own_override: None,
            b: origin_of(&a_path, a_part, &a_frame_name),
            flip: true,
            angle,
            slide_value: Some((Motion::Tx, value)),
            record: BuildRecord {
                role: BuildRole::Connector,
                junction: Some(junction.id.clone()),
                rule: Some(CONNECT_RULE),
                ..BuildRecord::default()
            },
        };
        let mut warnings = Vec::new();
        // Its fasteners on their frames.
        let (fasteners, fastener_frames) = if input.fasteners {
            fastener_pieces(self, &source, &mounts, &bodies, &placement, &mut warnings)?
        } else {
            (Vec::new(), Vec::new())
        };
        // The holes it needs.
        let mut holes = Vec::new();
        if input.machining {
            for machining in &connector.machining {
                let (profile, path, at) = if machining.on == "b" {
                    (&b, self.placed_in_host(b.component), junction.b.at.clone())
                } else {
                    (&a, Some(a_path.clone()), a_end.clone())
                };
                let Some(_path) = path else {
                    continue;
                };
                let frame = match machining.frame.as_str() {
                    "end" if profile.frame_kind(&at) == "end" => Some(at.clone()),
                    "end" => nearest_end_frame(profile, &junction, &world_of(self, profile)),
                    _ if machining.on == "b" => Some(junction.b.at.clone()),
                    _ => Some(a_frame_name.clone()),
                };
                let Some(frame) = frame else {
                    warnings.push(format!(
                        "{} has no {} frame for a hole",
                        profile.name, machining.frame
                    ));
                    continue;
                };
                let Some(local) = profile.mount(&frame).copied() else {
                    continue;
                };
                // The hole's point in the profile: the frame's origin, along
                // a slot at the junction.
                let point = if profile.frame_kind(&frame) == "slot" {
                    let world = world_of(self, profile);
                    let junction_point = if machining.on == "b" {
                        junction.a.frame.translation
                    } else {
                        world.apply_point(local.translation)
                    };
                    let p = inverse(&world).apply_point(junction_point);
                    let along = dot(sub(p, local.translation), x_axis(&local));
                    let along = if machining.on == "b" { along } else { tx };
                    add(local.translation, scale(x_axis(&local), along))
                } else {
                    local.translation
                };
                if let Some(def) = hole_def(profile, &frame, point, &machining.hole) {
                    holes.push(Hole {
                        component: profile.component,
                        def,
                    });
                }
            }
        }
        Ok(ConnectPlan {
            connector: connector_piece,
            fasteners,
            holes,
            placement,
            fastener_frames,
            warnings,
            designation,
            junction,
        })
    }

    fn build_connect(&mut self, input: &ConnectInput) -> Result<Placed, ModelError> {
        // Timber (mitcad#478): joinery, wood fasteners, outer connectors.
        if input.joint.is_some() {
            return self.build_joinery(input);
        }
        if self.is_wood_connect(input) {
            return self.build_wood_connect(input);
        }
        let plan = self.plan_connect(input)?;
        let active = self.build_host();
        self.state().editable(active)?;
        let label = format!(
            "Connect {}",
            if plan.designation.is_empty() {
                plan.connector.name.clone()
            } else {
                plan.designation.clone()
            }
        );
        self.apply(|state| {
            let (made, occurrence, part) = make_piece(state, &plan.connector, active)?;
            let mut adds = Vec::new();
            for fastener in &plan.fasteners {
                let mut fastener = fastener.clone();
                fastener.b.occurrence = OccurrencePath(vec![occurrence]);
                if let GeomRef::Mount { feature, .. } = &mut fastener.b.geometry {
                    *feature = part;
                }
                fastener.record.added_by = Some(made);
                let (c, _, _) = make_piece(state, &fastener, active)?;
                adds.push(BuildItem::Component(c));
            }
            for hole in &plan.holes {
                let def: FeatureDef<ValueInput> = serde_json::from_value(hole.def.clone())
                    .map_err(|e| refuse(format!("the connector's hole: {e}")))?;
                let uid = add_feature(state, def, hole.component)?;
                adds.push(BuildItem::Feature(uid));
            }
            let record = state
                .assembly
                .component_mut(made)
                .and_then(|c| c.build.as_mut())
                .expect("made");
            record.adds = adds.clone();
            let joint = record.joint;
            let name = state.assembly.name(made);
            Ok((
                label,
                Placed {
                    component: Some(made),
                    occurrence: Some(occurrence),
                    name,
                    features: vec![part],
                    joint,
                    bound: Vec::new(),
                    adds,
                    warnings: plan.warnings.clone(),
                    place: "junction",
                    fits: None,
                    set: Vec::new(),
                },
            ))
        })
    }

    fn build_upgrade(&mut self) -> Result<Vec<ComponentUid>, ModelError> {
        // A connector or a connector's fastener without a rule in its
        // record was placed by the first rule.
        let first: Vec<(ComponentUid, BuildRecord)> = self
            .assembly()
            .components
            .iter()
            .filter_map(|c| Some((c.uid, c.build.clone()?)))
            .filter(|(_, r)| {
                r.rule.is_none()
                    && ((r.role == BuildRole::Connector && r.junction.is_some())
                        || (r.role == BuildRole::Fastener && r.added_by.is_some()))
            })
            .collect();
        if first.is_empty() {
            return Ok(Vec::new());
        }
        let connectors: BTreeSet<ComponentUid> = first
            .iter()
            .filter_map(|(c, r)| match r.role {
                BuildRole::Connector => Some(*c),
                _ => r.added_by,
            })
            .collect();
        // Where they are needs the geometry.
        self.recompute();
        let host = self.host_placement();
        let mut edits: Vec<(FeatureUid, JointDef<ValueInput>)> = Vec::new();
        let mut moved = Vec::new();
        let mut marked = Vec::new();
        for connector in connectors {
            let Some(info) = self.part_info(connector) else {
                continue;
            };
            let Some(id) = info.record.source else {
                continue;
            };
            let config = info.def.as_ref().and_then(|d| d.config.clone());
            let Some(source) = held_source(self, id, config.clone()) else {
                continue;
            };
            // Slot connectors only: timber's are placed by their own rules.
            if source.meta.role != BuildRole::Connector
                || source.meta.wood.is_some()
                || source.meta.connector.as_ref().is_some_and(|c| c.is_outer())
            {
                continue;
            }
            // The connector, while it is where the first rule put it.
            if info.record.rule.is_none()
                && let (Some(junction), Some(joint)) = (&info.record.junction, info.record.joint)
            {
                let input = ConnectInput {
                    junction: junction.clone(),
                    part: PartInput {
                        source: Some(id),
                        config: config.clone(),
                        ..PartInput::default()
                    },
                    fasteners: false,
                    machining: false,
                    ..ConnectInput::default()
                };
                let at = world_of(self, &info);
                if let (Ok(old), Ok(new)) = (
                    self.plan_connect_by(&input, ConnectRule::First),
                    self.plan_connect_by(&input, ConnectRule::Current),
                ) && same_place(&at, &host.after(&old.placement))
                {
                    if !same_place(&at, &host.after(&new.placement))
                        && let Some(mut def) = joint_input(self, joint)
                    {
                        let piece = &new.connector;
                        def.b = piece.b.clone();
                        def.flip = piece.flip;
                        def.angle =
                            (piece.angle.abs() > 1e-12).then_some(ValueInput::Number(piece.angle));
                        if let Some((motion, value)) = &piece.slide_value {
                            def.position.insert(*motion, value.clone());
                        }
                        edits.push((joint, def));
                        moved.push(connector);
                    }
                    marked.push(connector);
                }
            }
            // Its fasteners, while their joints are as the first rule made
            // them (flipped, held by their own frames as they are).
            let Ok((mounts, bodies)) = evaluated_bodies(self, &source, &[]) else {
                continue;
            };
            let washer_of = |frame: &str| -> Option<String> {
                let meta = source.meta.connector.as_ref()?;
                let f = meta.fasteners.iter().find(|f| f.frame == frame)?;
                Some(f.washer.as_ref()?.component.clone())
            };
            // Each frame's hardware: (component, joint, its definition,
            // what it is).
            let mut by_frame: BTreeMap<String, Vec<HeldJoint>> = BTreeMap::new();
            for (c, r) in first.iter().filter(|(_, r)| r.added_by == Some(connector)) {
                let (Some(joint), Some(fid)) = (r.joint, r.source) else {
                    continue;
                };
                let Some(def) = joint_input(self, joint) else {
                    continue;
                };
                if !def.flip || def.a.frame_override.is_some() {
                    continue;
                }
                let GeomRef::Mount { name: frame, .. } = &def.b.geometry else {
                    continue;
                };
                let fconfig = self
                    .part_info(*c)
                    .and_then(|i| i.def.and_then(|d| d.config));
                let Some(used) = held_source(self, fid, fconfig) else {
                    continue;
                };
                let Ok((_, fbounds)) = evaluated(self, &used, &[]) else {
                    continue;
                };
                let in_slot = match &def.a.geometry {
                    GeomRef::Mount { name, .. } => used
                        .meta
                        .frames
                        .iter()
                        .any(|f| f.name == *name && f.kind == "slot"),
                    _ => false,
                };
                let kind = if in_slot {
                    Hardware::Nut
                } else if washer_of(frame).is_some_and(|w| w == used.def.library.component) {
                    Hardware::Washer
                } else {
                    Hardware::Screw
                };
                by_frame.entry(frame.clone()).or_default().push((
                    *c,
                    joint,
                    def.clone(),
                    Held {
                        kind,
                        in_slot,
                        bounds: fbounds,
                    },
                ));
            }
            for (frame, hardware) in by_frame {
                let Some(at) = mounts.get(&frame) else {
                    continue;
                };
                let held: Vec<Held> = hardware
                    .iter()
                    .map(|(_, _, _, h)| Held {
                        kind: h.kind,
                        in_slot: h.in_slot,
                        bounds: h.bounds,
                    })
                    .collect();
                let (ways, _) = holds(self, &bodies, at, &held);
                for ((c, joint, mut def, _), (flip, own_override)) in hardware.into_iter().zip(ways)
                {
                    if def.flip != flip || def.a.frame_override != own_override {
                        def.flip = flip;
                        def.a.frame_override = own_override;
                        edits.push((joint, def));
                        moved.push(c);
                    }
                    marked.push(c);
                }
            }
        }
        if edits.is_empty() {
            return Ok(Vec::new());
        }
        self.apply(|state| {
            for (joint, def) in &edits {
                state.edit_feature(*joint, &FeatureDef::Joint(def.clone()))?;
            }
            for c in &marked {
                if let Some(record) = state
                    .assembly
                    .component_mut(*c)
                    .and_then(|c| c.build.as_mut())
                {
                    record.rule = Some(CONNECT_RULE);
                }
            }
            Ok(("Place connectors by the current rule".to_owned(), ()))
        })?;
        Ok(moved)
    }

    fn plan_panel(&self, input: &PanelInput) -> Result<PanelPlan, ModelError> {
        let source = self.source_use(&input.part)?;
        let label = source.def.label();
        if source.meta.role != BuildRole::Panel {
            return Err(refuse(format!("{label} is no panel")));
        }
        let panel = source.meta.panel.clone().unwrap_or_default();
        // A sheet laid on members' faces (mitcad#478).
        if panel.on_face() {
            return self.plan_face_sheet(input, source);
        }
        let geometry = input
            .at
            .geometry
            .clone()
            .ok_or_else(|| refuse("pick a face of one of the frame's profiles"))?;
        let path = self.path_from_host(&input.at.occurrence)?;
        let target = self
            .target(&path)
            .ok_or_else(|| refuse("pick a face of one of the frame's profiles"))?;
        let point_local = input
            .at
            .point
            .map(|p| inverse(&target.placement).apply_point(p));
        let face = match &geometry {
            GeomRef::Face { body, face } => Some((*body, face.clone())),
            _ => None,
        };
        let slot = match &geometry {
            GeomRef::Mount { name, .. } if target.info.frame_kind(name) == "slot" => name.clone(),
            _ => self.pick_slot_of(&target, face.as_ref(), point_local)?,
        };
        let s1 = target
            .frame(&slot)
            .ok_or_else(|| refuse(format!("{} has no frame {slot}", target.info.name)))?;
        let [o1, x1, y1, z1] = parts(&s1);
        let pick = input.at.point.unwrap_or(o1);
        let along_pick = dot(sub(pick, o1), x1);
        let mut warnings = Vec::new();
        // Thickness and the other values.
        let mut values = input.parameters.clone();
        let mut numbers: Vec<(String, f64)> = Vec::new();
        for (name, value) in &input.parameters {
            numbers.push((name.clone(), value_in(self, value, Unit::MM)?));
        }
        let design = source
            .def
            .design
            .state()
            .map_err(|e| refuse(format!("{label}: {e}")))?;
        let parameter =
            |name: &Option<String>| name.clone().filter(|n| design.parameters.find(n).is_some());
        if let Some(thickness) = &input.thickness {
            match panel
                .thickness
                .as_ref()
                .map(|t| t.text())
                .filter(|t| design.parameters.find(t.trim()).is_some())
            {
                Some(name) => {
                    numbers.push((name.trim().to_owned(), value_in(self, thickness, Unit::MM)?));
                    values.insert(name.trim().to_owned(), thickness.clone());
                }
                None => warnings.push(format!("{label} has a fixed thickness")),
            }
        }
        let width_param = parameter(&panel.width);
        let height_param = parameter(&panel.height);
        let depth = target.info.meta.slot_depth();
        let clearance = panel.clearance();
        let into = 2.0 * (depth - clearance);
        let parts_info = self.build_parts_info();
        let mut bounds = Vec::new();
        let fits;
        let reason;
        let width;
        let height;
        let mut clamp_span: Option<(f64, f64)> = None;
        let placement;
        if panel.on_surface() {
            // On the face, held by clamps, at the size of the frame around
            // it where its sizes are parameters: across from the outer
            // side of the picked profile to that of the profile across,
            // along between the outer sides of the profiles beside (bound
            // lengths through those profiles keep it so).
            let (_, pbounds) = evaluated(self, &source, &numbers)?;
            let b = pbounds.unwrap_or(BoundingBox {
                min: [0.0; 3],
                max: [0.0; 3],
            });
            let mut w = b.max[0] - b.min[0];
            let mut h = b.max[1] - b.min[1];
            let thickness = b.max[2] - b.min[2];
            let mut centre_u = along_pick;
            let mut centre_v = 0.0;
            match surface_extent(self, &parts_info, &target, &s1, along_pick)? {
                Some((across, along)) => {
                    centre_v = (across.min + across.max) / 2.0;
                    if let Some(name) = &height_param {
                        h = across.max - across.min;
                        values.insert(name.clone(), ValueInput::Number(h));
                        if let Some((from, to, behind)) = across.bound {
                            bounds.push(PanelBound {
                                parameter: name.clone(),
                                from,
                                to,
                                mode: BoundMode::Through,
                                add: behind,
                                first: h,
                            });
                        }
                    }
                    if along.bound.is_none() {
                        warnings.push(
                            "no profiles beside: the panel is as long as the profile".to_owned(),
                        );
                    }
                    centre_u = (along.min + along.max) / 2.0;
                    if let Some(name) = &width_param {
                        w = along.max - along.min;
                        values.insert(name.clone(), ValueInput::Number(w));
                        if let Some((from, to, behind)) = along.bound {
                            bounds.push(PanelBound {
                                parameter: name.clone(),
                                from,
                                to,
                                mode: BoundMode::Through,
                                add: behind,
                                first: w,
                            });
                        }
                    }
                }
                None => {
                    warnings.push("no frame around the face: the panel keeps its size".to_owned())
                }
            }
            width = w;
            height = h;
            let centre = add(
                add(add(o1, scale(x1, centre_u)), scale(y1, centre_v)),
                scale(z1, thickness / 2.0),
            );
            placement = mitcad_model::internal::joints::frame(centre, x1, z1);
            // The clamps hold its edge along the picked profile, within that
            // profile.
            let mut lo = centre_u - w / 2.0;
            let mut hi = centre_u + w / 2.0;
            if let Some(own) = self.part_bounds(target.info.component) {
                let (near, far) = box_along(&own, &target.placement, o1, x1);
                lo = lo.max(near);
                hi = hi.min(far);
            }
            clamp_span = Some((lo, hi));
            fits = true;
            reason = String::new();
        } else {
            // The profile across the frame, and those beside.
            let across = facing_slot(
                self,
                &parts_info,
                target.info.component,
                add(o1, scale(x1, along_pick)),
                z1,
                y1,
            )
            .ok_or_else(|| {
                refuse(format!(
                    "the face is not inside a frame: no profile faces {} across it",
                    target.info.name
                ))
            })?;
            let inner_h = dot(sub(across.frame.translation, o1), z1);
            let middle = add(add(o1, scale(x1, along_pick)), scale(z1, inner_h / 2.0));
            let left = facing_slot(
                self,
                &parts_info,
                target.info.component,
                middle,
                scale(x1, -1.0),
                y1,
            );
            let right = facing_slot(self, &parts_info, target.info.component, middle, x1, y1);
            let (inner_w, centre_x) = match (&left, &right) {
                (Some(l), Some(r)) => {
                    let dl = dot(sub(middle, l.frame.translation), x1);
                    let dr = dot(sub(r.frame.translation, middle), x1);
                    (dl + dr, along_pick + (dr - dl) / 2.0)
                }
                _ => {
                    warnings.push("no profiles beside: the panel keeps its width".to_owned());
                    (f64::NAN, along_pick)
                }
            };
            height = inner_h + into;
            width = if inner_w.is_finite() {
                inner_w + into
            } else {
                f64::NAN
            };
            // Values of the sizes, bound to the frame where they are
            // parameters; a fixed one must fit.
            let (pmounts, pbounds) = evaluated(self, &source, &numbers)?;
            let _ = pmounts;
            let actual = pbounds.map(|b| [b.max[0] - b.min[0], b.max[1] - b.min[1]]);
            let mut problems = Vec::new();
            let mut check_fixed = |what: &str, inner: f64, size: Option<f64>| {
                let (Some(size), true) = (size, inner.is_finite()) else {
                    return;
                };
                if size <= inner + 1e-6 {
                    problems.push(format!(
                        "its {what} of {} mm does not reach into the slots ({} mm between the \
                         profiles)",
                        mitcad_model::internal::build::round(size),
                        mitcad_model::internal::build::round(inner)
                    ));
                } else if size > inner + into + 1e-6 {
                    problems.push(format!(
                        "its {what} of {} mm is more than the {} mm the slots take",
                        mitcad_model::internal::build::round(size),
                        mitcad_model::internal::build::round(inner + into)
                    ));
                }
            };
            match &width_param {
                Some(name) if width.is_finite() => {
                    values.insert(name.clone(), ValueInput::Number(width));
                    if let (Some(l), Some(r)) = (&left, &right) {
                        bounds.push(PanelBound {
                            parameter: name.clone(),
                            from: l.origin.clone(),
                            to: BoundTo {
                                occurrence: r.origin.occurrence.clone(),
                                geometry: r.origin.geometry.clone(),
                            },
                            mode: BoundMode::Face,
                            add: into,
                            first: width,
                        });
                    }
                }
                Some(_) => {}
                None => check_fixed("width", inner_w, actual.map(|a| a[0])),
            }
            match &height_param {
                Some(name) => {
                    values.insert(name.clone(), ValueInput::Number(height));
                    bounds.push(PanelBound {
                        parameter: name.clone(),
                        from: origin_of(&path, target.part()?, &slot),
                        to: BoundTo {
                            occurrence: across.origin.occurrence.clone(),
                            geometry: across.origin.geometry.clone(),
                        },
                        mode: BoundMode::Face,
                        add: into,
                        first: height,
                    });
                }
                None => check_fixed("height", inner_h, actual.map(|a| a[1])),
            }
            fits = problems.is_empty();
            reason = problems.join("; ");
            let centre = add(add(o1, scale(x1, centre_x)), scale(z1, inner_h / 2.0));
            placement = mitcad_model::internal::joints::frame(centre, x1, y1);
        }
        // The middle of the design's box at its new sizes goes there (a
        // design drawn from its corner, or extruded from its XY plane, too).
        let sized: Vec<(String, f64)> = values
            .iter()
            .filter_map(|(n, v)| Some((n.clone(), value_in(self, v, Unit::MM).ok()?)))
            .collect();
        let placement = match evaluated(self, &source, &sized)?.1 {
            Some(b) => {
                let middle = std::array::from_fn(|i| -(b.min[i] + b.max[i]) / 2.0);
                placement.after(&Transform::translation(middle))
            }
            None => placement,
        };
        // Joined to the picked profile where it goes: its design's origin
        // on the frame of the panel's middle (x along the profile).
        let local = inverse(&target.placement).after(&placement);
        let [lo, lx, _, lz] = parts(&local);
        let b = JointOrigin {
            occurrence: OccurrencePath(path.clone()),
            geometry: GeomRef::Mount {
                feature: target.part()?,
                name: slot.clone(),
            },
            frame_override: Some(FrameOverride {
                origin: Some(lo),
                z_axis: Some(lz),
                x_axis: Some(lx),
                shift: None,
            }),
        };
        let designation_values: BTreeMap<String, f64> = values
            .iter()
            .filter_map(|(n, v)| Some((n.clone(), value_in(self, v, Unit::MM).ok()?)))
            .collect();
        let designation =
            crate::parts::designation(&source.def, source.config.as_deref(), &designation_values);
        let name = input.name.clone().unwrap_or_else(|| {
            if designation.is_empty() {
                label.clone()
            } else {
                designation.clone()
            }
        });
        // A surface panel's clamps, at its ends along the profile.
        let mut clamps = Vec::new();
        if panel.on_surface()
            && let Some(clamp) = &panel.clamp
        {
            let part = request_of(clamp, &source.def);
            match self.fastener_source(&part) {
                Ok(used) => {
                    let (cmounts, cbounds) = evaluated(self, &used, &[])?;
                    let own = crate::place::own_frame(&used.meta, &cmounts, &["slot", "face"])
                        .or_else(|| cmounts.keys().next().cloned());
                    // Inset by the clamp's own reach along the slot from
                    // its frame, so that it stays on the profile.
                    let reach = match (own.as_ref().and_then(|n| cmounts.get(n)), &cbounds) {
                        (Some(frame), Some(b)) => {
                            let (near, far) = box_along(
                                b,
                                &Transform::IDENTITY,
                                frame.translation,
                                x_axis(frame),
                            );
                            near.abs().max(far.abs())
                        }
                        _ => 0.0,
                    };
                    let (lo, hi) =
                        clamp_span.unwrap_or((along_pick - width / 2.0, along_pick + width / 2.0));
                    let at = if hi - lo > 2.0 * reach {
                        vec![lo + reach, hi - reach]
                    } else {
                        vec![(lo + hi) / 2.0]
                    };
                    for tx in at {
                        let record = BuildRecord {
                            role: BuildRole::Fastener,
                            ..BuildRecord::default()
                        };
                        clamps.push(Piece {
                            source: used.clone(),
                            values: BTreeMap::new(),
                            name: used.def.label(),
                            kind: JointKind::Slider,
                            slide: SlideAxis::X,
                            own: own.clone(),
                            own_override: None,
                            b: origin_of(&path, target.part()?, &slot),
                            flip: true,
                            angle: 0.0,
                            slide_value: Some((Motion::Tx, ValueInput::Number(tx))),
                            record,
                        });
                    }
                }
                Err(e) => warnings.push(format!("the clamps are left out: {e}")),
            }
        }
        Ok(PanelPlan {
            panel: Piece {
                source,
                values,
                name,
                kind: JointKind::Rigid,
                slide: SlideAxis::Z,
                own: None,
                own_override: None,
                b,
                flip: false,
                angle: 0.0,
                slide_value: None,
                record: BuildRecord {
                    role: BuildRole::Panel,
                    ..BuildRecord::default()
                },
            },
            bounds,
            clamps,
            fits,
            reason,
            width,
            height,
            placement,
            warnings,
            designation,
        })
    }

    fn build_panel(&mut self, input: &PanelInput) -> Result<Placed, ModelError> {
        let plan = self.plan_panel(input)?;
        let active = self.build_host();
        self.state().editable(active)?;
        let label = format!("Place {}", plan.panel.name);
        self.apply(|state| {
            let (made, occurrence, part) = make_piece(state, &plan.panel, active)?;
            let mut bound = Vec::new();
            for b in &plan.bounds {
                let ray = crate::place::RayPlan {
                    from: b.from.clone(),
                    shift: [0.0; 3],
                    shift_by_joint: false,
                    slide_extra: 0.0,
                    direction: RayDirection::default(),
                    start: [0.0; 3],
                    along: crate::geometry::Z,
                };
                let uid = add_bound(
                    state,
                    active,
                    part,
                    &b.parameter,
                    &ray,
                    None,
                    &b.to,
                    b.mode,
                    Some(ValueInput::Number(b.add)),
                    b.first,
                )?;
                bound.push(uid);
            }
            let mut adds = Vec::new();
            for clamp in &plan.clamps {
                let mut clamp = clamp.clone();
                clamp.record.added_by = Some(made);
                let (c, _, _) = make_piece(state, &clamp, active)?;
                adds.push(BuildItem::Component(c));
            }
            let record = state
                .assembly
                .component_mut(made)
                .and_then(|c| c.build.as_mut())
                .expect("made");
            record.bound = bound.clone();
            record.adds = adds.clone();
            let joint = record.joint;
            let name = state.assembly.name(made);
            Ok((
                label,
                Placed {
                    component: Some(made),
                    occurrence: Some(occurrence),
                    name,
                    features: vec![part],
                    joint,
                    bound,
                    adds,
                    warnings: plan.warnings.clone(),
                    place: "panel",
                    fits: Some((plan.fits, plan.reason.clone())),
                    set: Vec::new(),
                },
            ))
        })
    }

    fn build_remove_with(
        &mut self,
        component: &str,
        set: bool,
    ) -> Result<RemoveResult, ModelError> {
        let component = self.component_by_ref(component)?;
        let mut targets = vec![component];
        // A timber frame goes whole, with its fasteners (mitcad#479).
        let frame = self.frame_lead(component).filter(|_| set);
        // Its own component goes with it when nothing else is left in it
        // (mitcad#522).
        let mut frame_component = None;
        if let Some(lead) = frame {
            targets = self.frame_fasteners(lead);
            targets.extend(self.frame_members(lead));
            frame_component = self.frame_component(lead);
        } else if set {
            let lead = self
                .part_info(component)
                .and_then(|i| i.record.set)
                .unwrap_or(component);
            // With the fasteners it was fastened to its hosts with
            // (mitcad#497).
            targets = self.set_fasteners(lead);
            targets.extend(self.set_members(lead));
            if targets.is_empty() {
                targets = vec![component];
            }
        }
        let mut plans = Vec::new();
        for c in &targets {
            plans.push(self.plan_remove(*c)?);
        }
        let label = match plans.as_slice() {
            [one] => format!("Remove {}", one.name),
            _ => format!("Remove {} x{}", plans[0].name, plans.len()),
        };
        self.apply(|state| {
            let mut out = RemoveResult::default();
            for plan in &plans {
                let removed = apply_remove(state, plan)?;
                out.components.extend(removed.components);
                out.features.extend(removed.features);
                out.detached.extend(removed.detached);
            }
            if let Some(c) = frame_component.filter(|c| {
                state.assembly.exists(*c)
                    && state.assembly.children(*c).next().is_none()
                    && !state.features.iter().any(|f| f.component == *c)
            }) {
                out.features
                    .extend(state.delete_components(BTreeSet::from([c])));
                // Not listed in `components`: those are the BUILD parts removed.
            }
            Ok((label, out))
        })
    }
}

/// The document's BUILD methods of this block, as an extension trait
/// (mitcad#777): BUILD planning lives in a crate above the model.
pub(crate) trait ConnectCrateOps<K: Kernel> {
    /// A source for a part reference, also one without `build` (a screw
    /// of a fastener library).
    fn fastener_source(&self, part: &PartInput) -> Result<SourceUse, ModelError>;

    /// The slot of a picked face (see `place.rs`).
    fn pick_slot_of(
        &self,
        target: &crate::place::Target,
        face: Option<&(mitcad_model::ids::BodyUid, mitcad_model::topo::FaceName)>,
        point: Option<V3>,
    ) -> Result<String, ModelError>;

    /// What removing a BUILD part takes with it, planned against the
    /// document as it is.
    fn plan_remove(&self, component: ComponentUid) -> Result<RemovePlan, ModelError>;
}

impl<K: Kernel> ConnectCrateOps<K> for Document<K> {
    fn fastener_source(&self, part: &PartInput) -> Result<SourceUse, ModelError> {
        match self.source_use(part) {
            Ok(source) => Ok(source),
            Err(_) if part.source.is_none() => {
                let (Some(library), Some(component)) = (&part.library, &part.component) else {
                    return Err(refuse("a fastener names its library and component"));
                };
                // A source the design holds already, at that version: no
                // library is read (offline, or mitcad-cli without fetched
                // libraries; mitcad#1115).
                if let Some(rev) = &part.rev
                    && let Some(id) = self.state().library_sources.find(library, component, rev)
                {
                    let held = PartInput {
                        source: Some(id),
                        ..part.clone()
                    };
                    if let Ok(source) = self.source_use(&held) {
                        return Ok(source);
                    }
                    let def = self
                        .state()
                        .library_sources
                        .get(id)
                        .cloned()
                        .expect("found");
                    return Ok(SourceUse {
                        meta: BuildMeta {
                            role: BuildRole::Fastener,
                            ..BuildMeta::default()
                        },
                        def,
                        new: false,
                        config: part.config.clone(),
                    });
                }
                let resolver = self
                    .link_resolver()
                    .ok_or_else(|| refuse("no component libraries are set up here"))?;
                let read = resolver
                    .library_source(&LibraryRequest {
                        library: library.clone(),
                        url: part.url.clone(),
                        rev: part.rev.clone().unwrap_or_else(|| "HEAD".to_owned()),
                        component: component.clone(),
                    })
                    .map_err(refuse)?;
                let state = self.state();
                let (def, new) =
                    match state
                        .library_sources
                        .find(&read.library, &read.component, &read.rev)
                    {
                        Some(id) => (
                            state.library_sources.get(id).cloned().expect("found"),
                            false,
                        ),
                        None => (
                            LibrarySourceDef::from_library(state.library_sources.next_id(), &read)
                                .map_err(refuse)?,
                            true,
                        ),
                    };
                Ok(SourceUse {
                    meta: BuildMeta {
                        role: BuildRole::Fastener,
                        ..BuildMeta::default()
                    },
                    def,
                    new,
                    config: part.config.clone(),
                })
            }
            Err(e) => Err(e),
        }
    }

    fn pick_slot_of(
        &self,
        target: &crate::place::Target,
        face: Option<&(mitcad_model::ids::BodyUid, mitcad_model::topo::FaceName)>,
        point: Option<V3>,
    ) -> Result<String, ModelError> {
        let mut slots = target.info.slots();
        if let Some((body, face)) = face
            && let Some(shape) = self.body_shape(*body)
            && let Ok(mitcad_model::datum::SurfaceGeometry::Plane { normal, .. }) =
                self.kernel().face_geometry(shape, face)
            && let Some(n) = unit(normal)
        {
            slots.retain(|(_, t)| dot(z_axis(t), n) > 1.0 - 1e-6);
        }
        if let Some(p) = point {
            let distance = |t: &Transform| {
                let [o, x, _, _] = parts(t);
                let d = sub(p, o);
                norm(sub(d, scale(x, dot(d, x))))
            };
            slots.sort_by(|a, b| distance(a.1).total_cmp(&distance(b.1)));
        }
        slots
            .first()
            .map(|(n, _)| (*n).clone())
            .ok_or_else(|| refuse(format!("the face of {} has no slot", target.info.name)))
    }

    fn plan_remove(&self, component: ComponentUid) -> Result<RemovePlan, ModelError> {
        let info = self.part_info(component).ok_or_else(|| {
            refuse(format!(
                "{} is no BUILD part",
                self.assembly().name(component)
            ))
        })?;
        // What goes with it.
        let mut components = BTreeSet::from([component]);
        let mut features = BTreeSet::new();
        for item in &info.record.adds {
            match item {
                BuildItem::Component(c) => {
                    components.insert(*c);
                }
                BuildItem::Feature(f) => {
                    features.insert(*f);
                }
            }
        }
        // Fasteners it added that its record has lost.
        for c in &self.assembly().components {
            if c.build
                .as_ref()
                .is_some_and(|b| b.added_by == Some(component))
            {
                components.insert(c.uid);
            }
        }
        // Other parts' bound lengths that end on (or start from) what goes.
        let mut others = Vec::new();
        for part in self.build_parts_info() {
            if components.contains(&part.component) {
                continue;
            }
            for bound in &part.record.bound {
                let Some(entry) = self.feature(*bound) else {
                    continue;
                };
                let FeatureDef::BoundLength(b) = &entry.def else {
                    continue;
                };
                let ends_on = |path: &OccurrencePath| {
                    mitcad_model::internal::joints::path_component(
                        self.assembly(),
                        entry.component,
                        &path.0,
                    )
                    .is_ok_and(|c| components.contains(&c))
                };
                if (ends_on(&b.to.occurrence) || ends_on(&b.from.occurrence))
                    && let Some(p) = part.part
                {
                    others.push((part.component, *bound, p));
                }
            }
        }
        // Occurrences whose joints go with it keep where they are.
        let removed_parts: BTreeSet<FeatureUid> = components
            .iter()
            .filter_map(|c| self.part_info(*c).and_then(|i| i.part))
            .collect();
        let mut keep = Vec::new();
        for entry in self.features() {
            let FeatureDef::Joint(j) = &entry.def else {
                continue;
            };
            let on_removed = [&j.a, &j.b].iter().any(|o| {
                matches!(&o.geometry, GeomRef::Mount { feature, .. } if removed_parts.contains(feature))
            });
            if !on_removed {
                continue;
            }
            for side in [&j.a, &j.b] {
                if let Some(first) = side.occurrence.0.first()
                    && let Ok(c) = mitcad_model::internal::joints::path_component(
                        self.assembly(),
                        entry.component,
                        &side.occurrence.0,
                    )
                    && !components.contains(&c)
                    && let Some(t) = self.placement(*first)
                {
                    keep.push((*first, t));
                }
            }
        }
        // The joints that place what goes. Most go with the derived part
        // their frame is on; one on a design's XY (a panel's, a fastener's
        // without frames) names only the occurrence, which is deleted.
        for entry in self.features() {
            let FeatureDef::Joint(j) = &entry.def else {
                continue;
            };
            let places_removed = [&j.a, &j.b].iter().any(|side| {
                mitcad_model::internal::joints::path_component(
                    self.assembly(),
                    entry.component,
                    &side.occurrence.0,
                )
                .is_ok_and(|c| components.contains(&c))
            });
            if places_removed {
                features.insert(entry.uid);
            }
        }
        Ok(RemovePlan {
            component,
            name: info.name.clone(),
            components,
            features,
            others,
            keep,
        })
    }
}

/// What `build_remove` takes away (one part's): it and what it added, the
/// features that go with them, the other parts' bound lengths to detach
/// and the occurrences to keep where they are.
#[derive(Debug, Clone)]
pub(crate) struct RemovePlan {
    pub component: ComponentUid,
    pub name: String,
    pub components: BTreeSet<ComponentUid>,
    pub features: BTreeSet<FeatureUid>,
    pub others: Vec<(ComponentUid, FeatureUid, FeatureUid)>,
    pub keep: Vec<(OccurrenceUid, Transform)>,
}

/// Removes what a plan says, in `state`.
pub(crate) fn apply_remove(
    state: &mut DocState,
    plan: &RemovePlan,
) -> Result<RemoveResult, ModelError> {
    let components = &plan.components;
    let component = plan.component;
    let mut detached = Vec::new();
    for (other, bound, part) in &plan.others {
        if state.position(*bound).is_none() {
            continue;
        }
        detach(state, *other, *bound, *part)?;
        detached.push(*bound);
    }
    for (occurrence, t) in &plan.keep {
        if let Some(o) = state.assembly.occurrence_mut(*occurrence) {
            o.transform = *t;
        }
    }
    // A spacing set's lead hands the set to the next member (mitcad#478),
    // and joinery with the parts that go is taken out of the others.
    crate::spacing::hand_over_set(state, components);
    crate::spacing::drop_cuts_with(state, components)?;
    // A timber frame's lead likewise (mitcad#479).
    crate::frames::hand_over_frame(state, components);
    let mut removed = Vec::new();
    let features: BTreeSet<FeatureUid> = plan
        .features
        .iter()
        .copied()
        .filter(|f| state.position(*f).is_some())
        .collect();
    if !features.is_empty() {
        let mut all = features.clone();
        all.extend(state.dependents(&features));
        removed.extend(state.remove_features(&all));
    }
    for bound in current_bounds(state, component) {
        if !removed.contains(&bound) {
            removed.extend(state.remove_features(&BTreeSet::from([bound])));
        }
    }
    let alive: BTreeSet<ComponentUid> = components
        .iter()
        .copied()
        .filter(|c| state.assembly.exists(*c))
        .collect();
    removed.extend(state.delete_components(alive));
    Ok(RemoveResult {
        components: components.iter().copied().collect(),
        features: removed,
        detached,
    })
}

/// The document's BUILD methods of this block, as an extension trait
/// (mitcad#777): BUILD planning lives in a crate above the model.
pub trait SetMembersBuild<K: Kernel> {
    /// The members of a spacing set, by their lead (mitcad#478), in the
    /// order they were made.
    fn set_members(&self, lead: ComponentUid) -> Vec<ComponentUid>;
}

impl<K: Kernel> SetMembersBuild<K> for Document<K> {
    fn set_members(&self, lead: ComponentUid) -> Vec<ComponentUid> {
        self.assembly()
            .components
            .iter()
            .filter(|c| {
                c.build
                    .as_ref()
                    .is_some_and(|b| b.set == Some(lead) || (c.uid == lead && b.spacing.is_some()))
            })
            .map(|c| c.uid)
            .collect()
    }
}

/// A part's placement in the design.
pub(crate) fn world_of<K: Kernel>(doc: &Document<K>, part: &PartInfo) -> Transform {
    let path = doc.placed_in_host(part.component).unwrap_or_default();
    doc.host_placement().after(&doc.path_transform(&path))
}

/// How far along a line (from `origin` along the unit `along`) the side
/// face of `b` that faces back along it lies (mitcad#1115): of b's sides
/// (not its ends), the one whose outward normal points most against
/// `towards`, the way from the line into b. The sides are those of its
/// section, centred on its axis as the profile layout has it, else of its
/// body's box (a cut end can widen the box of a body that is not exact).
/// None when no side faces back or the line runs along that face.
fn face_along<K: Kernel>(
    doc: &Document<K>,
    b: &PartInfo,
    origin: V3,
    along: V3,
    towards: V3,
) -> Option<f64> {
    let bounds = match b.meta.section_size() {
        Some([w, t]) => BoundingBox {
            min: [-w / 2.0, -t / 2.0, 0.0],
            max: [w / 2.0, t / 2.0, 0.0],
        },
        None => doc.part_bounds(b.component)?,
    };
    let world = world_of(doc, b);
    let mut best: Option<(f64, V3, V3)> = None;
    for k in [0, 1] {
        for (sign, value) in [(1.0, bounds.max[k]), (-1.0, bounds.min[k])] {
            let mut normal = [0.0; 3];
            normal[k] = sign;
            let mut point = [0.0; 3];
            point[k] = value;
            let n = world.apply_vector(normal);
            let facing = -dot(n, towards);
            if best.is_none_or(|(f, _, _)| facing > f) {
                best = Some((facing, world.apply_point(point), n));
            }
        }
    }
    let (facing, point, n) = best?;
    let across = dot(along, n);
    if facing < 0.5 || across.abs() < 1e-6 {
        return None;
    }
    Some(dot(sub(point, origin), n) / across)
}

/// A distance within a nanometre of zero is zero: a value or an expression
/// of a connector's place stays as before where nothing changes it.
fn snap(value: f64) -> f64 {
    if value.abs() < 1e-6 { 0.0 } else { value }
}

/// The end frame of a profile nearest the junction.
fn nearest_end_frame(part: &PartInfo, junction: &Junction, world: &Transform) -> Option<String> {
    let at = junction.a.frame.translation;
    part.mounts
        .iter()
        .filter(|(n, _)| part.frame_kind(n) == "end")
        .min_by(|x, y| {
            let d = |t: &Transform| norm(sub(world.apply_point(t.translation), at));
            d(x.1).total_cmp(&d(y.1))
        })
        .map(|(n, _)| n.clone())
}

/// A hole feature's definition: at `point` on a frame's plane of a
/// profile, into it.
pub(crate) fn hole_def(
    part: &PartInfo,
    frame: &str,
    point: V3,
    hole: &crate::HoleMeta,
) -> Option<serde_json::Value> {
    let feature = part.part?;
    let diameter = hole.diameter.as_ref().and_then(|d| d.millimetres())?;
    let mut def = json!({
        "type": "hole",
        "placement": {"type": "plane", "plane": {"mount": format!("{feature}:mount({frame})")},
                      "points": [point]},
        "diameter": diameter,
        "extent": match hole.depth.as_ref().and_then(|d| d.millimetres()) {
            Some(depth) => json!({"type": "distance", "depth": depth}),
            None => json!({"type": "through_all"}),
        },
    });
    if let Some(thread) = hole.thread.as_ref().filter(|t| !t.trim().is_empty()) {
        def["thread"] = json!({"designation": thread.trim()});
    }
    Some(def)
}

/// A slot frame of another profile that faces a point across a ray: the
/// nearest one along `direction` from `start` whose z points back and that
/// lies in the plane across `normal` through `start`.
pub(crate) struct Facing {
    pub frame: Transform,
    pub origin: JointOrigin,
    /// The part's component and its placement in the BUILD host.
    pub component: ComponentUid,
    pub placement: Transform,
}

fn facing_slot<K: Kernel>(
    doc: &Document<K>,
    parts: &[PartInfo],
    except: ComponentUid,
    start: V3,
    direction: V3,
    normal: V3,
) -> Option<Facing> {
    let direction = unit(direction)?;
    let mut best: Option<(f64, Facing)> = None;
    for part in parts {
        if part.component == except || part.meta.role != BuildRole::Profile {
            continue;
        }
        let Some(path) = doc.placed_in_host(part.component) else {
            continue;
        };
        let Some(feature) = part.part else {
            continue;
        };
        let placement = doc.path_transform(&path);
        for (name, local) in part.slots() {
            let frame = placement.after(local);
            let [o, _, _, z] = crate::geometry::parts(&frame);
            if dot(z, direction) > -1.0 + 1e-6 {
                continue;
            }
            if dot(sub(o, start), normal).abs() > 1e-3 {
                continue;
            }
            let distance = dot(sub(o, start), direction);
            if distance <= 1e-9 {
                continue;
            }
            if best.as_ref().is_none_or(|(d, _)| distance < *d) {
                best = Some((
                    distance,
                    Facing {
                        frame,
                        origin: origin_of(&path, feature, name),
                        component: part.component,
                        placement,
                    },
                ));
            }
        }
    }
    best.map(|(_, f)| f)
}

/// A surface panel's range along one axis of the picked slot frame (from
/// its origin), and the bound length that keeps it: from a frame, through
/// the part of another, and what is added (the first part's side behind
/// its frame).
struct Span {
    min: f64,
    max: f64,
    bound: Option<(JointOrigin, BoundTo, f64)>,
}

/// The frame around the outer face a surface panel is picked on, in the
/// picked slot frame `s1` (x along the profile, y across): across, from the
/// outer side of the picked profile to the outer side of the nearest
/// profile whose slot faces its inner side; along, between the outer sides
/// of the profiles beside the room between them, else the picked profile's
/// length. None when no profile faces it across.
fn surface_extent<K: Kernel>(
    doc: &Document<K>,
    infos: &[PartInfo],
    target: &crate::place::Target,
    s1: &Transform,
    along_pick: f64,
) -> Result<Option<(Span, Span)>, ModelError> {
    let [o1, x1, y1, z1] = crate::geometry::parts(s1);
    let feature = target.part()?;
    let Some(own) = doc.part_bounds(target.info.component) else {
        return Ok(None);
    };
    let to_of = |f: &Facing| BoundTo {
        occurrence: f.origin.occurrence.clone(),
        geometry: f.origin.geometry.clone(),
    };
    // The picked profile's inner side: a slot square to the face and to
    // the profile, with a profile across it.
    let mut best: Option<(f64, V3, V3, String, Facing)> = None;
    for (name, local) in target.info.slots() {
        let frame = target.placement.after(local);
        let [o, _, _, z] = crate::geometry::parts(&frame);
        if dot(z, z1).abs() > 1e-6 || dot(z, x1).abs() > 1e-6 {
            continue;
        }
        let start = add(o, scale(x1, dot(sub(o1, o), x1) + along_pick));
        let Some(across) = facing_slot(doc, infos, target.info.component, start, z, z1) else {
            continue;
        };
        let distance = dot(sub(across.frame.translation, start), z);
        if best.as_ref().is_none_or(|b| distance < b.0) {
            best = Some((distance, start, z, name.clone(), across));
        }
    }
    let Some((inner, start, z, name, across)) = best else {
        return Ok(None);
    };
    let Some(across_box) = doc.part_bounds(across.component) else {
        return Ok(None);
    };
    let behind = box_along(&own, &target.placement, start, scale(z, -1.0)).1;
    let far = box_along(&across_box, &across.placement, start, z).1;
    let side = dot(z, y1);
    let v = dot(sub(start, o1), y1);
    let (a, b) = (v - side * behind, v + side * far);
    let across_span = Span {
        min: a.min(b),
        max: a.max(b),
        bound: Some((
            origin_of(&target.path, feature, &name),
            to_of(&across),
            behind,
        )),
    };
    // Beside the middle of the room across.
    let middle = add(start, scale(z, inner / 2.0));
    let minus_x = scale(x1, -1.0);
    let left = facing_slot(doc, infos, target.info.component, middle, minus_x, z1);
    let right = facing_slot(doc, infos, target.info.component, middle, x1, z1);
    let beside = match (&left, &right) {
        (Some(l), Some(r)) => doc
            .part_bounds(l.component)
            .zip(doc.part_bounds(r.component))
            .map(|(lb, rb)| {
                let u = dot(sub(middle, o1), x1);
                let left_far = box_along(&lb, &l.placement, middle, minus_x).1;
                let right_far = box_along(&rb, &r.placement, middle, x1).1;
                let left_behind = box_along(&lb, &l.placement, l.frame.translation, minus_x).1;
                Span {
                    min: u - left_far,
                    max: u + right_far,
                    bound: Some((l.origin.clone(), to_of(r), left_behind)),
                }
            }),
        _ => None,
    };
    let along = beside.unwrap_or_else(|| {
        let (near, far) = box_along(&own, &target.placement, o1, x1);
        Span {
            min: near,
            max: far,
            bound: None,
        }
    });
    Ok(Some((across_span, along)))
}
