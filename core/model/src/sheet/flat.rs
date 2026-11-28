// SPDX-License-Identifier: MIT
//! The derived whole-body Flat Pattern (mitcad#208).
//!
//! A Flat Pattern is a document item, not a timeline feature and not a body:
//! it is evaluated on demand from the body's current sheet state at the
//! timeline marker. Every bend is developed while the chosen panel stays
//! fixed; the outline is the union of all developed charts in that panel's
//! material coordinates, so it does not move with the body, and the bends
//! report their centre and tangent lines with their manufacturing data. The
//! developed solid validates the result; it is never added to the bodies.

use super::folded::develop::{
    bend_record, chart_coordinates, chart_point, check_overlaps, chord, domain_label, inverse,
    orient, planar_map, reconfigure, region_area, then,
};
use super::*;
use crate::kernel::Kernel;
use crate::recompute::Recomputed;
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// Changes when the development or its output changes meaning.
pub const FLAT_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum FlatXAxis {
    /// The fixed panel's own material x axis.
    #[default]
    PanelX,
}

fn front() -> SheetReferenceSide {
    SheetReferenceSide::Front
}

/// The persistent settings of one body's Flat Pattern.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlatPatternDef {
    /// The panel that stays fixed and defines the 2D frame.
    pub stationary: PanelId,
    #[serde(default)]
    pub x_axis: FlatXAxis,
    /// The material side the 2D drawing looks at; `back` mirrors it.
    #[serde(default = "front")]
    pub reference_side: SheetReferenceSide,
    /// The view shows the developed solid in place of the folded body
    /// (mitcad#210): a display choice, not part of the evaluation.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub shown: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FlatLine {
    pub start: [f64; 2],
    pub end: [f64; 2],
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FlatBend {
    pub bend: BendId,
    pub angle: f64,
    pub radius: f64,
    pub k_factor: f64,
    pub allowance: f64,
    /// `up` when the bend's inside is on the viewed reference side.
    pub direction: &'static str,
    /// Lines at `u = BA/2`, clipped to the bend's remaining material.
    pub centerlines: Vec<FlatLine>,
    /// Lines at `u = 0` and `u = BA`, clipped likewise.
    pub tangents: Vec<FlatLine>,
    /// A kerf bend's slit lattice (mitcad#1939): the slits across its
    /// developed strip, clipped to its material; empty for other bends.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub slits: Vec<FlatLine>,
}

/// A bead on the Flat Pattern (mitcad#2645): the blank stays flat, and the
/// bead is drawn as a form, its outline on its face and its centre lines,
/// in flat coordinates.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FlatBead {
    pub feature: crate::ids::FeatureUid,
    /// `up` when the bead stands out of the viewed side.
    pub direction: &'static str,
    pub outline: Vec<SegmentGeometry>,
    pub centerlines: Vec<SegmentGeometry>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FlatDomain {
    pub domain: SheetDomainRef,
    /// Chart coordinates to flat coordinates.
    pub to_flat: Isometry2,
}

/// An evaluated Flat Pattern.
#[derive(Debug, Clone)]
pub struct FlatPattern<S> {
    pub body: BodyUid,
    pub sheet_version: String,
    pub def: FlatPatternDef,
    pub thickness: f64,
    /// The flat 2D frame in the body's coordinates: the fixed panel's
    /// reference surface.
    pub frame: SketchFrame,
    /// Closed outer contours, each followed by its holes, in flat coordinates.
    pub regions: Vec<MaterialRegion>,
    pub area: f64,
    pub volume: f64,
    pub bends: Vec<FlatBend>,
    /// A Lofted Flange's lines where its flat regions meet regions formed
    /// smooth (mitcad#2512), drawn on their own layer.
    pub facet_lines: Vec<FlatLine>,
    /// The sheet's beads (mitcad#2645), drawn on their own layer.
    pub beads: Vec<FlatBead>,
    pub domains: Vec<FlatDomain>,
    /// The developed configuration the outline came from.
    pub developed: SheetState,
    /// The developed solid, for display; not a body of the design.
    pub solid: S,
}

/// Why there is no usable Flat Pattern: `unavailable`, `invalid` or
/// `blocked`, with the reasons. A sheet that is not valid says why in the
/// user's terms (`diagnostics`, shown in the Browser's Flat Pattern row:
/// the feature by its name, mitcad#1636); its stored diagnostics with the
/// features' ids are the `causes`, for agents. Every reason is in words
/// (mitcad#1639): the body is "this body", walls and features are named as
/// the timeline names them; the same reasons with the ids are the
/// `details`, for agents (empty when they say the same).
#[derive(Debug, Clone, PartialEq)]
pub struct FlatFailure {
    pub status: &'static str,
    pub diagnostics: Vec<String>,
    pub causes: Vec<SheetDiagnostic>,
    pub details: Vec<String>,
}

impl FlatFailure {
    fn new(status: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            diagnostics: vec![message.into()],
            causes: Vec::new(),
            details: Vec::new(),
        }
    }

    /// A failure a person reads as `shown`, which agents read as `detail`
    /// with the ids (mitcad#1639).
    pub(crate) fn told(status: &'static str, shown: String, detail: String) -> Self {
        let details = if detail == shown {
            Vec::new()
        } else {
            vec![detail]
        };
        Self {
            status,
            diagnostics: vec![shown],
            causes: Vec::new(),
            details,
        }
    }

    /// A failure of the development, whose `message` may name walls by
    /// their ids (`panel F6 slot 0`) or carry other ids and points: a
    /// person reads the walls named by their features (`name_walls`) and
    /// the rest in words (the model's cleaner, `FeatureError::from_text`);
    /// agents read it as it was.
    fn named(status: &'static str, message: &str, name: &dyn Fn(FeatureUid) -> String) -> Self {
        use crate::notice::FeatureError;
        let shown = FeatureError::from_text(&name_walls(message, name)).message;
        let detail = FeatureError::from_text(message).agent_text().to_owned();
        Self::told(status, shown, detail)
    }

    /// The reasons as agents read them (mitcad#1639): with the ids of the
    /// body, its walls and the features that caused them.
    pub fn agent_reasons(&self) -> String {
        let mut reasons = if self.details.is_empty() {
            self.diagnostics.join("; ")
        } else {
            self.details.join("; ")
        };
        for cause in &self.causes {
            use std::fmt::Write as _;
            let _ = write!(reasons, " [{}: {}]", cause.feature, cause.message);
        }
        reasons
    }
}

/// The cache key of an evaluation: everything the result depends on.
/// `name` gives a feature's name, which the reason of a sheet that is not
/// valid says (a renamed feature changes it).
pub(crate) fn fingerprint<S>(
    result: &Recomputed<S>,
    body: BodyUid,
    def: &FlatPatternDef,
    name: &dyn Fn(FeatureUid) -> String,
) -> u128 {
    // Showing the pattern changes nothing it is made of.
    let def = &FlatPatternDef {
        shown: false,
        ..def.clone()
    };
    let mut fp = crate::fingerprint::Fingerprint::new("sheet-flat-v1");
    let state = result.sheets.get(&body);
    let rules = state.and_then(|s| result.sheet_rules.get(&s.value.rules));
    let key = json!({
        "body": body,
        "def": def,
        "flat": FLAT_VERSION,
        "algorithm": ALGORITHM_VERSION,
        "state": state.map(|s| format!("{:032x}", s.version)),
        "shape": result.bodies.get(&body).map(|b| format!("{:032x}", b.version)),
        "owner": result.owners.get(&body),
        "rules": rules.map(|r| format!("{:032x}", r.version)),
        "reason": state
            .filter(|s| s.value.status() != "valid")
            .map(|s| refusal(&s.value, name)),
        // The names a failed development's reasons give its walls and
        // overlays (mitcad#1639).
        "names": state.map(|s| {
            let features: std::collections::BTreeSet<FeatureUid> = s
                .value
                .charts
                .iter()
                .map(|p| match p.domain {
                    SheetDomainRef::Panel(id) => id.feature,
                    SheetDomainRef::Bend(id) => id.feature,
                })
                .chain(s.value.overlays.iter().map(|o| o.feature))
                .collect();
            features.into_iter().map(name).collect::<Vec<_>>()
        }),
    });
    serde_json::to_writer(&mut fp, &key).expect("flat key serializes");
    fp.finish()
}

/// The sheet state of a body that is current and valid at the marker.
fn current<S>(
    result: &Recomputed<S>,
    body: BodyUid,
    name: &dyn Fn(FeatureUid) -> String,
) -> Result<(SheetState, f64, String), FlatFailure> {
    // In words for a person, the body's id for agents (mitcad#1639).
    let unavailable =
        |shown: &str, detail: String| FlatFailure::told("unavailable", shown.to_owned(), detail);
    let shape = result.bodies.get(&body).ok_or_else(|| {
        unavailable(
            "this body does not exist at the timeline marker",
            format!("body {body} does not exist at the timeline marker"),
        )
    })?;
    let state = result.sheets.get(&body).ok_or_else(|| {
        unavailable(
            "this body is not a sheet-metal body",
            format!("body {body} is not a sheet-metal body"),
        )
    })?;
    if state.value.shape_generation != format!("{:032x}", shape.version)
        || result.owners.get(&body) != Some(&state.value.owner)
    {
        return Err(FlatFailure::new(
            "unavailable",
            "sheet state does not match current geometry",
        ));
    }
    let status = state.value.status();
    if status != "valid" {
        return Err(FlatFailure {
            status: if status == "blocked" {
                "blocked"
            } else {
                "invalid"
            },
            // The one text a sheet feature's refusal says too (mitcad#1636).
            diagnostics: vec![refusal(&state.value, name)],
            causes: state
                .value
                .invalidated
                .iter()
                .chain(&state.value.blockers)
                .cloned()
                .collect(),
            details: Vec::new(),
        });
    }
    let rules = result
        .sheet_rules
        .get(&state.value.rules)
        .filter(|r| format!("{:032x}", r.version) == state.value.rules_version)
        .ok_or_else(|| {
            FlatFailure::new(
                "invalid",
                "the sheet's rules changed; its domains are stale",
            )
        })?;
    if state.value.algorithm != ALGORITHM_VERSION || state.value.charts.is_empty() {
        return Err(FlatFailure::new(
            "invalid",
            "sheet material charts require recomputation",
        ));
    }
    Ok((
        state.value.clone(),
        rules.value.thickness,
        format!("{:032x}", state.version),
    ))
}

/// Why a sheet with an overlay has no Flat Pattern
/// ([`SheetState::overlay_said`]): the feature that made the overlay by its
/// name and what it does; its id for agents (mitcad#1639, mitcad#1942).
fn overlay_refusal(state: &SheetState, words: &FeatureWords<'_>) -> Result<(), FlatFailure> {
    match state.overlay_said("the Flat Pattern", words) {
        Some((shown, detail)) => Err(FlatFailure::told("invalid", shown, detail)),
        None => Ok(()),
    }
}

fn line(map: Isometry2, u: f64, interval: [f64; 2]) -> FlatLine {
    FlatLine {
        start: map.point([u, interval[0]]),
        end: map.point([u, interval[1]]),
    }
}

/// Evaluates the Flat Pattern of `body` with `def` at the marker. `words`
/// name a feature, for the reason of a sheet that is not valid and the
/// walls a failed development names (mitcad#1639), and say what a fillet
/// or chamfer the sheet keeps does (mitcad#1942).
pub(crate) fn evaluate<K: Kernel>(
    kernel: &K,
    result: &Recomputed<K::Shape>,
    body: BodyUid,
    def: &FlatPatternDef,
    words: &FeatureWords<'_>,
) -> Result<FlatPattern<K::Shape>, FlatFailure> {
    let name = words.name;
    let (state, thickness, sheet_version) = current(result, body, name)?;
    let invalid = |m: String| FlatFailure::named("invalid", &m, name);
    overlay_refusal(&state, words)?;
    let fixed = SheetDomainRef::Panel(def.stationary);
    let fixed_embedding = state
        .charts
        .iter()
        .find(|p| p.domain == fixed)
        .map(|p| p.embedding.clone())
        .ok_or_else(|| {
            invalid(format!(
                "the fixed {} is not a material domain of this sheet",
                domain_label(fixed)
            ))
        })?;
    let mut developed = state.clone();
    // A curved panel's chart is its development already: laid in the
    // plane touching it (mitcad#2435).
    // A Lofted Flange's chart likewise (mitcad#2512).
    let curved_or_lofted = super::folded::curved::developed(&state, def.stationary, thickness)
        .or_else(|| super::folded::lofted::developed(&state, def.stationary, thickness));
    let fixed_embedding = match curved_or_lofted {
        Some(result) => {
            developed = result.map_err(invalid)?;
            developed
                .charts
                .iter()
                .find(|p| p.domain == fixed)
                .map(|p| p.embedding.clone())
                .ok_or_else(|| invalid("the curved panel was not developed".into()))?
        }
        None => {
            let all: BTreeMap<BendId, bool> = state.bends.iter().map(|b| (b.id, true)).collect();
            reconfigure(&mut developed, thickness, def.stationary, &all).map_err(invalid)?;
            fixed_embedding
        }
    };
    let mirror = Isometry2 {
        origin: [0.0, 0.0],
        x_axis: [-1.0, 0.0],
        y_axis: [0.0, 1.0],
    };
    let view = |map: Isometry2| match def.reference_side {
        SheetReferenceSide::Front => map,
        SheetReferenceSide::Back => then(map, mirror),
    };
    let mut domains = Vec::new();
    for patch in &developed.charts {
        // One plane and one front side: anything else is not a flat pattern.
        let map = planar_map(&patch.embedding, &fixed_embedding).map_err(|e| {
            invalid(format!(
                "{} does not develop flat: {e}",
                domain_label(patch.domain)
            ))
        })?;
        domains.push(FlatDomain {
            domain: patch.domain,
            to_flat: map,
        });
    }
    check_overlaps(kernel, &developed).map_err(invalid)?;
    let front_maps: BTreeMap<_, _> = domains.iter().map(|d| (d.domain, d.to_flat)).collect();
    let subject = developed
        .charts
        .iter()
        .find(|p| p.domain == fixed)
        .expect("fixed chart");
    let tools: Vec<MaterialRegion> = developed
        .charts
        .iter()
        .filter(|p| p.domain != fixed)
        .flat_map(|p| super::folded::mapped_regions(&p.regions, front_maps[&p.domain]))
        .collect();
    let mut outline = subject.regions.clone();
    if !tools.is_empty() {
        let union = kernel
            .sheet_trim_domain(&SheetTrimSpec {
                operation: SheetTrimOperation::Union,
                domains: &[TrimDomain {
                    domain: fixed,
                    regions: subject.regions.clone(),
                    to_operation: Isometry2::IDENTITY,
                }],
                tools: &tools,
                seams: &[],
                tolerance: TOLERANCE,
            })
            .map_err(|e| invalid(format!("the developed outline: {e}")))?;
        outline = union
            .domains
            .iter()
            .flat_map(|d| &d.regions)
            .map(|r| MaterialRegion {
                loops: r
                    .loops
                    .iter()
                    .map(|l| MaterialLoop {
                        edges: l
                            .edges
                            .iter()
                            .map(|e| MaterialEdge {
                                piece: e
                                    .lineage
                                    .first()
                                    .map(|s| match &s.source {
                                        TrimSource::Domain { boundary } => boundary.piece.clone(),
                                        TrimSource::Tool { piece } => piece.clone(),
                                    })
                                    .unwrap_or_else(|| {
                                        super::folded::generated(body.feature, e.token)
                                    }),
                                curve: e.curve.clone(),
                            })
                            .collect(),
                    })
                    .collect(),
            })
            .collect();
    }
    for region in &mut outline {
        orient(region).map_err(invalid)?;
    }
    if outline.len() != 1 {
        return Err(invalid(format!(
            "the developed material forms {} separate regions",
            outline.len()
        )));
    }
    let mut area = 0.0;
    for region in &outline {
        area += region_area(region).map_err(invalid)?;
    }
    let regions = match def.reference_side {
        SheetReferenceSide::Front => outline,
        SheetReferenceSide::Back => super::folded::mapped_regions(&outline, mirror),
    };
    let mut bends = Vec::new();
    for b in &state.bends {
        let record = bend_record(&state, b.id, thickness).map_err(invalid)?;
        let domain = SheetDomainRef::Bend(b.id);
        let patch = developed
            .charts
            .iter()
            .find(|p| p.domain == domain)
            .ok_or_else(|| invalid(format!("{} has no material chart", domain_label(domain))))?;
        let map = view(front_maps[&domain]);
        let ba = record.allowance;
        // Tangent lines lie on chart seams; clip them just inside the strip.
        let inset = 1e-6 * ba.max(1.0);
        let lines = |u: f64, at: f64| -> Result<Vec<FlatLine>, FlatFailure> {
            Ok(chord(&patch.regions, at)
                .map_err(invalid)?
                .into_iter()
                .map(|i| line(map, u, i))
                .collect())
        };
        let mut tangents = lines(0.0, inset)?;
        tangents.extend(lines(ba, ba - inset)?);
        let inside_front = record.folded_embedding.inner_is_front;
        bends.push(FlatBend {
            bend: b.id,
            angle: record.signed_angle,
            radius: record.radius,
            k_factor: record.k_factor,
            allowance: ba,
            direction: if inside_front == (def.reference_side == SheetReferenceSide::Front) {
                "up"
            } else {
                "down"
            },
            centerlines: lines(ba / 2.0, ba / 2.0)?,
            tangents,
            slits: match &b.lattice {
                Some(lattice) => {
                    let mut slits = Vec::new();
                    for (row, u) in lattice.rows(ba).into_iter().enumerate() {
                        let intervals = chord(&patch.regions, u).map_err(invalid)?;
                        slits.extend(
                            lattice
                                .row(row, &intervals)
                                .into_iter()
                                .map(|i| line(map, u, i)),
                        );
                    }
                    slits
                }
                None => Vec::new(),
            },
        });
    }
    // A Lofted Flange's folds between facets are its bend lines, the lines
    // where its flat regions meet formed ones its facet lines
    // (mitcad#2512).
    let mut facet_lines = Vec::new();
    for patch in &state.charts {
        let SheetEmbedding::Lofted(lofted) = &patch.embedding else {
            continue;
        };
        let map = view(front_maps[&patch.domain]);
        let SheetDomainRef::Panel(panel) = patch.domain else {
            continue;
        };
        let mut slot = 0;
        for l in &lofted.lines {
            let flat = FlatLine {
                start: map.point(l.start),
                end: map.point(l.end),
            };
            if !l.fold {
                facet_lines.push(flat);
                continue;
            }
            // A bend of the rules' radius (mitcad#2560): its allowance the
            // arc at the K-factor from its inside, its tangent lines.
            bends.push(FlatBend {
                bend: BendId {
                    feature: panel.feature,
                    slot,
                },
                angle: l.angle,
                radius: l.radius,
                k_factor: lofted.k_factor,
                allowance: if l.radius > 0.0 {
                    l.angle.abs() * (l.radius + lofted.k_factor * thickness)
                } else {
                    0.0
                },
                direction: if l.inside_front == (def.reference_side == SheetReferenceSide::Front) {
                    "up"
                } else {
                    "down"
                },
                centerlines: vec![flat],
                tangents: l
                    .tangents
                    .iter()
                    .map(|t| FlatLine {
                        start: map.point(t[0]),
                        end: map.point(t[1]),
                    })
                    .collect(),
                slits: Vec::new(),
            });
            slot += 1;
        }
    }
    // Beads as forms on the flat blank (mitcad#2645).
    let mut beads = Vec::new();
    for overlay in &state.overlays {
        let SheetOverlayKind::Bead(bead) = &overlay.kind else {
            continue;
        };
        let domain = SheetDomainRef::Panel(bead.panel);
        let map = view(
            *front_maps
                .get(&domain)
                .ok_or_else(|| invalid("the face of a bead is not developed".into()))?,
        );
        let section = bead.section.section().map_err(invalid)?;
        let outline = bead.outline(&section).map_err(invalid)?;
        // The side it stands out of: its face's when it stands out.
        let raised = match bead.section.direction {
            super::bead::BeadDirection::Out => bead.side,
            super::bead::BeadDirection::In => match bead.side {
                SheetReferenceSide::Front => SheetReferenceSide::Back,
                SheetReferenceSide::Back => SheetReferenceSide::Front,
            },
        };
        beads.push(FlatBead {
            feature: overlay.feature,
            direction: if raised == def.reference_side {
                "up"
            } else {
                "down"
            },
            outline: outline
                .iter()
                .map(|g| super::bead::mapped_geometry(g, map))
                .collect(),
            centerlines: bead
                .centre_lines()
                .iter()
                .map(|g| super::bead::mapped_geometry(g, map))
                .collect(),
        });
    }
    let geometry = kernel
        .sheet_shape(
            &SheetShapeSpec {
                thickness,
                patches: &developed.charts,
                ports: &developed.joins,
                tolerance: TOLERANCE,
            },
            &format!("{}:flat", body),
        )
        .map_err(|e| invalid(format!("the developed solid: {e}")))?;
    if geometry.solids.len() != 1 {
        return Err(invalid(format!(
            "the developed solid has {} pieces",
            geometry.solids.len()
        )));
    }
    let solid = geometry.solids.into_iter().next().expect("one").shape;
    let volume = kernel
        .mass_properties(&solid)
        .map(|m| m.volume)
        .unwrap_or(f64::NAN);
    let SheetEmbedding::Plane {
        front_frame,
        front_to_back_sign,
    } = fixed_embedding
    else {
        unreachable!("checked by the development");
    };
    let frame = match def.reference_side {
        SheetReferenceSide::Front => front_frame,
        SheetReferenceSide::Back => {
            let back = front_frame
                .normal()
                .map(|n| n * front_to_back_sign.sign() * thickness);
            SketchFrame {
                origin: std::array::from_fn(|i| front_frame.origin[i] + back[i]),
                x_axis: front_frame.x_axis.map(|v| -v),
                y_axis: front_frame.y_axis,
            }
        }
    };
    Ok(FlatPattern {
        body,
        sheet_version,
        def: def.clone(),
        thickness,
        frame,
        regions,
        area,
        volume,
        bends,
        facet_lines,
        beads,
        domains: domains
            .into_iter()
            .map(|d| FlatDomain {
                to_flat: view(d.to_flat),
                ..d
            })
            .collect(),
        developed,
        solid,
    })
}

impl<S> FlatPattern<S> {
    /// The box of its outline in flat coordinates: lower left and upper
    /// right corners (the Sheet Metal list's flat size, mitcad#1980).
    pub fn bounds(&self) -> Option<([f64; 2], [f64; 2])> {
        super::folded::bounds(&SheetPatch {
            domain: SheetDomainRef::Panel(self.def.stationary),
            regions: self.regions.clone(),
            embedding: SheetEmbedding::Plane {
                front_frame: SketchFrame::XY,
                front_to_back_sign: MaterialParity::Positive,
            },
        })
        .ok()
    }

    pub fn json(&self) -> Value {
        let (min, max) = self.bounds().unwrap_or(([f64::NAN; 2], [f64::NAN; 2]));
        let mut value = json!({
            "body": self.body,
            "status": "valid",
            "sheet_version": self.sheet_version,
            "stationary": self.def.stationary,
            "x_axis": self.def.x_axis,
            "reference_side": self.def.reference_side,
            "thickness": self.thickness,
            "frame": {"origin": self.frame.origin, "x_axis": self.frame.x_axis,
                      "y_axis": self.frame.y_axis, "normal": self.frame.normal()},
            "regions": self.regions,
            "area": self.area,
            "volume": self.volume,
            "bounds": {"min": min, "max": max},
            "bends": self.bends,
            "facet_lines": self.facet_lines,
            "beads": self.beads,
            "domains": self.domains,
            "diagnostics": [],
        });
        // A pattern laid flat by triangulation says how approximate it is
        // (mitcad#2442).
        if let Some(approximation) = self.approximation() {
            value["approximation"] = approximation;
        }
        value
    }

    /// How a pattern laid flat by triangulation differs from its faces, with
    /// its fold lines in flat coordinates (mitcad#2442); None for others.
    pub fn approximation(&self) -> Option<Value> {
        let to_flat = self
            .domains
            .iter()
            .find(|d| d.domain == SheetDomainRef::Panel(self.def.stationary))
            .map(|d| d.to_flat);
        super::folded::triangulated::summary(
            &self.developed,
            to_flat,
            self.def.reference_side == SheetReferenceSide::Back,
        )
    }

    /// The flat position of a point of the body, or the body point of a flat
    /// position: the correspondence in both directions.
    pub fn locate(
        &self,
        current: &SheetState,
        flat: Option<[f64; 2]>,
        model: Option<[f64; 3]>,
    ) -> Result<Value, String> {
        // A pattern laid flat by triangulation is no exact development of
        // the body (mitcad#2442).
        if self.developed.approximation.is_some() {
            return Err(
                "the pattern is laid flat approximately, by triangulation: its points have no \
                 exact place on the body"
                    .into(),
            );
        }
        let membership = |patch: &SheetPatch, uv: [f64; 2]| -> bool {
            patch.regions.iter().any(|r| contains(r, uv))
        };
        for d in &self.domains {
            let developed = self
                .developed
                .charts
                .iter()
                .find(|p| p.domain == d.domain)
                .expect("developed chart");
            let Some(patch) = current.charts.iter().find(|p| p.domain == d.domain) else {
                continue;
            };
            let (uv, depth) = match (flat, model) {
                (Some(p), None) => (inverse(d.to_flat).point(p), 0.0),
                (None, Some(x)) => match chart_coordinates(&patch.embedding, x, self.thickness) {
                    Some(found) => found,
                    None => continue,
                },
                _ => return Err("give either a flat or a model point".into()),
            };
            if !(-1e-6..=self.thickness + 1e-6).contains(&depth) || !membership(developed, uv) {
                continue;
            }
            return Ok(json!({
                "domain": d.domain,
                "chart": uv,
                "depth": depth,
                "flat": d.to_flat.point(uv),
                "model": chart_point(&patch.embedding, uv, depth, self.thickness),
            }));
        }
        Err("the point is not on the sheet's material".into())
    }
}

/// Point membership of a region, from dense samples of its exact curves.
pub(crate) fn contains(region: &MaterialRegion, p: [f64; 2]) -> bool {
    let mut inside = false;
    for l in &region.loops {
        let mut points = Vec::new();
        for e in &l.edges {
            let Ok(support) = e.curve.support() else {
                return false;
            };
            let n = if matches!(e.curve.geometry, SegmentGeometry::Line { .. }) {
                1
            } else {
                256
            };
            let (t0, t1) = (e.curve.range.first, e.curve.range.last);
            for i in 0..n {
                points.push(support.point(t0 + (t1 - t0) * f64::from(i) / f64::from(n)));
            }
        }
        // Points on a boundary belong to the material.
        if points
            .iter()
            .zip(points.iter().cycle().skip(1))
            .any(|(a, b)| segment_distance(*a, *b, p) <= 1e-7)
        {
            return true;
        }
        if crate::sketch::geometry::polygon_contains(&points, p) {
            inside = !inside;
        }
    }
    inside
}

fn segment_distance(a: [f64; 2], b: [f64; 2], p: [f64; 2]) -> f64 {
    use crate::sketch::geometry as g;
    let d = g::sub(b, a);
    let len = g::dot(d, d);
    let t = if len > 0.0 {
        (g::dot(g::sub(p, a), d) / len).clamp(0.0, 1.0)
    } else {
        0.0
    };
    g::dist(g::add(a, g::scale(d, t)), p)
}

/// The JSON of a failure; it has no usable export payload. The `causes`
/// (the sheet's stored diagnostics, with the features' ids) are for
/// agents; the `diagnostics` say why in words (mitcad#1636), the `details`
/// the same with the ids where they differ (mitcad#1639).
pub(crate) fn failure_json(body: BodyUid, failure: &FlatFailure) -> Value {
    let mut value =
        json!({"body": body, "status": failure.status, "diagnostics": failure.diagnostics});
    if !failure.causes.is_empty() {
        value["causes"] = json!(failure.causes);
    }
    if !failure.details.is_empty() {
        value["details"] = json!(failure.details);
    }
    value
}
