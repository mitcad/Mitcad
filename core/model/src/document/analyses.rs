// SPDX-License-Identifier: MIT
//! Analyses kept in the document (mitcad#41): section analyses, print
//! analyses (mitcad#620), draft analyses (mitcad#2166), point cloud
//! deviations (mitcad#2240), minimum radius analyses (mitcad#2518),
//! minimum distances (mitcad#2519), the
//! surface quality analyses Zebra, Curvature Map, Isocurve Analysis and
//! Environment Map (mitcad#2517, `surface_analyses.rs`) and interference
//! checks (mitcad#2664), listed in the
//! browser's Analysis folder and saved in the design file. They change no
//! geometry: adding, editing, renaming, showing, hiding and deleting one is
//! an undo step that recomputes nothing.
//!
//! An analysis refers to geometry as features do (a plane reference, a
//! bed face, a pull direction), so a face reference follows the model by
//! its topological name; it is resolved at the timeline marker whenever it
//! is asked for, and an analysis whose plane, body, bed face or pull
//! reference is gone says why instead.
//!
//! At most one analysis is shown: showing one (adding it, or editing it
//! shown) hides the others, so the bodies are cut by one plane. An
//! interference check is not drawn in the view ([`AnalysisDef::is_drawn`]):
//! it is never shown, and adding or editing it leaves the shown one shown.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::{Document, ModelError, invalid};
use crate::analysis;
use crate::datum::{ANGULAR, DatumAxis, DatumPlane, OriginDatum, Vec3};
use crate::features::geom_ref::GeomRef;
use crate::features::{OccurrencePath, is_false};
use crate::ids::{BodyUid, ComponentUid};
use crate::joints::path_component;
use crate::kernel::Kernel;
use crate::print_analysis;
use crate::transform::Transform;

/// An analysis kept in the document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Analysis {
    pub name: String,
    /// Shown (its section cuts the bodies); left out of files when true.
    #[serde(default = "shown", skip_serializing_if = "is_shown")]
    pub visible: bool,
    #[serde(flatten)]
    pub def: AnalysisDef,
}

fn shown() -> bool {
    true
}

fn is_shown(visible: &bool) -> bool {
    *visible
}

/// What an analysis is, by its `type`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AnalysisDef {
    /// The bodies cut at a plane.
    Section(SectionAnalysis),
    /// Where bodies will print badly and what they will take (mitcad#620).
    Print(PrintAnalysis),
    /// The bodies' faces by their draft from a pull direction (mitcad#2166).
    Draft(DraftAnalysis),
    /// A point cloud's points by their distance from a body's faces
    /// (mitcad#2240).
    PointCloudDeviation(super::PointCloudDeviation),
    /// The smallest radius of curvature of faces, and the faces under a
    /// limit (mitcad#2518).
    MinRadius(MinRadiusAnalysis),
    /// The smallest distance between two selections, with a clearance
    /// (mitcad#2519).
    MinimumDistance(MinimumDistanceAnalysis),
    /// Surface quality analyses (mitcad#2517, `surface_analyses.rs`):
    /// stripes from a direction, the faces colored by their curvature,
    /// their iso curves, and an environment reflected in them.
    Zebra(super::ZebraAnalysis),
    Curvature(super::CurvatureAnalysis),
    Isocurve(super::IsocurveAnalysis),
    Environment(super::EnvironmentAnalysis),
    /// The pairs of bodies that overlap, and by how much, run again on its
    /// bodies whenever it is opened (mitcad#2664).
    Interference(InterferenceAnalysis),
}

impl AnalysisDef {
    /// Whether the analysis is drawn in the view while it is shown: every
    /// kind but an interference check (mitcad#2664), which is run when it
    /// is opened and is never shown.
    pub fn is_drawn(&self) -> bool {
        !matches!(self, Self::Interference(_))
    }

    /// The first part of default names (`Section1`, `Print1`, `Draft1`).
    fn name_stem(&self) -> &'static str {
        match self {
            Self::Section(_) => "Section",
            Self::Print(_) => "Print",
            Self::Draft(_) => "Draft",
            Self::PointCloudDeviation(_) => "Deviation",
            Self::MinRadius(_) => "MinRadius",
            Self::MinimumDistance(_) => "MinDistance",
            Self::Zebra(_) => "Zebra",
            Self::Curvature(_) => "Curvature",
            Self::Isocurve(_) => "Isocurves",
            Self::Environment(_) => "Environment",
            Self::Interference(_) => "Interference",
        }
    }
}

/// An Interference check (mitcad#2664): the pairs of `bodies` that overlap
/// and the volume of each overlap, as the `interference` query finds them,
/// kept so that the same check runs again whenever it is opened. Without
/// bodies it checks every body at the timeline marker when it runs, so the
/// bodies added since it was kept are checked too. A body that is gone is
/// said in words ([`Document::interference_check`]) until the check is
/// edited or an undo brings the body back.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InterferenceAnalysis {
    /// The bodies checked against each other; none: every body.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bodies: Vec<BodyUid>,
}

impl InterferenceAnalysis {
    /// Why the settings cannot be kept, if they cannot (not the geometry):
    /// a body listed twice, which would overlap itself.
    fn check(&self) -> Result<(), String> {
        let twice = self
            .bodies
            .iter()
            .enumerate()
            .any(|(i, body)| self.bodies[..i].contains(body));
        if twice {
            return Err("the interference check lists a body more than once".to_owned());
        }
        Ok(())
    }
}

/// What an interference check that is not drawn says when it is asked to
/// be shown (mitcad#2664).
const NOT_DRAWN: &str = "an interference check is not shown in the view: open it to run it";

/// A Minimum Distance analysis (mitcad#2519): the smallest distance between
/// two selections, each one or more picks of bodies, faces, edges,
/// vertices and components, measured as the `measure` query measures two
/// picks (the kernel's exact distance, in the design's coordinates) and
/// kept, so that it is measured again whenever the model changes. A pick
/// is a [`MeasurePick`] with a `body` (and a `face`, `edge` or `vertex` of
/// it), or with only an `occurrence`: a component as placed there, every
/// body in it and in the components inside it. With a `clearance` the
/// analysis says whether the selections come closer than it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MinimumDistanceAnalysis {
    pub first: Vec<super::MeasurePick>,
    pub second: Vec<super::MeasurePick>,
    /// mm, greater than zero; none: no clearance checked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clearance: Option<f64>,
}

impl MinimumDistanceAnalysis {
    /// Why the settings cannot be kept, if they cannot (not the geometry).
    fn check(&self) -> Result<(), String> {
        if self.first.is_empty() {
            return Err("the minimum distance needs a first selection".to_owned());
        }
        if self.second.is_empty() {
            return Err("the minimum distance needs a second selection".to_owned());
        }
        for pick in self.first.iter().chain(&self.second) {
            check_distance_pick(pick)?;
        }
        if let Some(clearance) = self.clearance {
            check_clearance(clearance)?;
        }
        Ok(())
    }
}

/// Why a pick cannot be one side of a minimum distance (mitcad#2519): a
/// datum, a face, edge or vertex without its body, or nothing at all.
pub fn check_distance_pick(pick: &super::MeasurePick) -> Result<(), String> {
    if pick.datum.is_some() {
        return Err(
            "a minimum distance is measured between bodies, faces, edges, vertices and \
             components, not datums"
                .to_owned(),
        );
    }
    let named = pick.face.is_some() || pick.edge.is_some() || pick.vertex.is_some();
    match (pick.body, &pick.occurrence) {
        (None, _) if named => Err("a face, edge or vertex needs its body".to_owned()),
        (None, None) => Err("a pick needs a body, or a component's occurrence".to_owned()),
        _ => Ok(()),
    }
}

/// Why a clearance (mm) is not one: finite and greater than zero.
pub fn check_clearance(clearance: f64) -> Result<(), String> {
    if clearance > 0.0 && clearance.is_finite() {
        Ok(())
    } else {
        Err(format!(
            "the clearance must be greater than zero, got {clearance}"
        ))
    }
}

/// The least draft a face needs when none is given, degrees.
pub const DEFAULT_DRAFT_ANGLE: f64 = 1.0;

fn default_draft_angle() -> f64 {
    DEFAULT_DRAFT_ANGLE
}

/// A Draft Analysis (mitcad#2166): each triangle of the bodies' meshes by
/// its draft from the pull direction, the angle between its normal and
/// the plane square to the pull: at least `angle` towards the pull (it
/// releases when pulled along it), less than `angle` either way (it needs
/// more draft), or at least `angle` the other way (it releases only when
/// pulled the opposite way: the other mould half). A face hidden by
/// another part of the body along the pull (an undercut) is not looked
/// for. The bodies are analysed where Print Analysis analyses them
/// ([`Document::print_placements`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DraftAnalysis {
    pub bodies: Vec<BodyUid>,
    /// The pull direction: the normal of a plane (an origin plane, a
    /// construction plane, a planar face, a fixed plane) or the direction
    /// of a line (an origin axis, a construction axis, a straight edge, a
    /// sketch line, a fixed axis); none: the design's +Z. Follows the model
    /// by its topological name.
    #[serde(default)]
    pub pull: Option<GeomRef>,
    /// The pull reference's occurrence, from the design root, as a
    /// section's `occurrence`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub occurrence: Option<OccurrencePath>,
    /// The pull direction reversed.
    #[serde(default, skip_serializing_if = "is_false")]
    pub flip: bool,
    /// The least draft a face needs, degrees, 0 to less than 90.
    #[serde(default = "default_draft_angle")]
    pub angle: f64,
}

impl DraftAnalysis {
    /// Why the settings cannot be kept, if they cannot (not the geometry).
    fn check(&self) -> Result<(), String> {
        if self.bodies.is_empty() {
            return Err("the draft analysis needs a body".to_owned());
        }
        check_draft_angle(self.angle)
    }
}

/// Why a draft angle (degrees) is not one: from 0 to less than 90.
pub fn check_draft_angle(angle: f64) -> Result<(), String> {
    if (0.0..90.0).contains(&angle) {
        Ok(())
    } else {
        Err(format!(
            "the draft angle must be from 0 to less than 90 degrees, got {angle}"
        ))
    }
}

/// A Minimum Radius analysis (mitcad#2518): the smallest radius of
/// curvature (one over the largest principal curvature,
/// [`Kernel::face_curvatures`]) of every face of `bodies` and of the faces
/// `faces`, and where it is; with a `limit`, the faces whose smallest
/// radius is under it (a milling tool's radius). A face that does not bend
/// has none. The faces are named by their topological names, as a Point
/// Cloud Deviation's; the bodies are analysed where Print Analysis analyses
/// them ([`Document::print_placements`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MinRadiusAnalysis {
    /// Bodies whose every face is analysed.
    #[serde(default)]
    pub bodies: Vec<BodyUid>,
    /// Faces analysed besides those of `bodies`: face references of any
    /// body (`{"body": "F2.b0", "face": "F2:side0"}`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub faces: Vec<GeomRef>,
    /// The faces whose smallest radius is under this (mm) are marked;
    /// none: no face is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<f64>,
}

impl MinRadiusAnalysis {
    /// Why the settings cannot be kept, if they cannot (not the geometry).
    fn check(&self) -> Result<(), String> {
        if self.bodies.is_empty() && self.faces.is_empty() {
            return Err("the minimum radius analysis needs a body or a face".to_owned());
        }
        if let Some(face) = self
            .faces
            .iter()
            .find(|face| !matches!(face, GeomRef::Face { .. }))
        {
            return Err(face.error(|what| format!("{what} is not a face of a body")));
        }
        match self.limit {
            Some(limit) => check_min_radius_limit(limit),
            None => Ok(()),
        }
    }

    /// The bodies analysed, each once: those of `bodies`, then those of
    /// `faces` not among them, in order; with each the faces named of it
    /// (empty: every face).
    pub fn targets(&self) -> Vec<(BodyUid, Vec<crate::topo::FaceName>)> {
        let mut targets: Vec<(BodyUid, Vec<crate::topo::FaceName>)> = Vec::new();
        for body in &self.bodies {
            if !targets.iter().any(|(b, _)| b == body) {
                targets.push((*body, Vec::new()));
            }
        }
        for reference in &self.faces {
            let GeomRef::Face { body, face } = reference else {
                continue;
            };
            if self.bodies.contains(body) {
                continue; // every face of it already
            }
            match targets.iter_mut().find(|(b, _)| b == body) {
                Some((_, faces)) => {
                    if !faces.contains(face) {
                        faces.push(face.clone());
                    }
                }
                None => targets.push((*body, vec![face.clone()])),
            }
        }
        targets
    }
}

/// Why a minimum radius analysis' limit (mm) is not one: greater than zero.
pub fn check_min_radius_limit(limit: f64) -> Result<(), String> {
    if limit > 0.0 && limit.is_finite() {
        Ok(())
    } else {
        Err(format!(
            "the radius limit must be greater than zero, got {limit}"
        ))
    }
}

/// A Print Analysis (mitcad#620): the overhangs, thin walls and print
/// estimate of bodies, printed in the direction the bed face gives (its
/// outward normal pointing down), or along the design's −Z on a bed at
/// each body's lowest point without one. The bed face only sets the
/// analysis' direction: no body moves. Fields left out take the defaults
/// of `print_analysis`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrintAnalysis {
    pub bodies: Vec<BodyUid>,
    /// A planar face of one of the bodies, lying on the bed; follows the
    /// model by its topological name.
    #[serde(default)]
    pub bed_face: Option<GeomRef>,
    #[serde(default)]
    pub overhang: OverhangCheck,
    #[serde(default)]
    pub thin_wall: ThinWallCheck,
    #[serde(default)]
    pub estimate: EstimateCheck,
    /// Narrow gaps between the bodies (mitcad#1519).
    #[serde(default)]
    pub min_gap: MinGapCheck,
}

fn yes() -> bool {
    true
}

fn default_angle() -> f64 {
    print_analysis::DEFAULT_ANGLE
}

fn default_minimum() -> f64 {
    print_analysis::DEFAULT_MINIMUM
}

/// Overhangs: faces steeper than `angle` degrees from the vertical.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OverhangCheck {
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default = "default_angle")]
    pub angle: f64,
}

impl Default for OverhangCheck {
    fn default() -> Self {
        Self {
            enabled: true,
            angle: print_analysis::DEFAULT_ANGLE,
        }
    }
}

/// Thin walls: walls thinner than `minimum` mm.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThinWallCheck {
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default = "default_minimum")]
    pub minimum: f64,
}

impl Default for ThinWallCheck {
    fn default() -> Self {
        Self {
            enabled: true,
            minimum: print_analysis::DEFAULT_MINIMUM,
        }
    }
}

fn default_line_width() -> f64 {
    print_analysis::DEFAULT_LINE_WIDTH
}

fn default_walls() -> u32 {
    print_analysis::DEFAULT_WALLS
}

fn default_layer_height() -> f64 {
    print_analysis::DEFAULT_LAYER_HEIGHT
}

fn default_top_bottom_layers() -> u32 {
    print_analysis::DEFAULT_TOP_BOTTOM_LAYERS
}

fn default_infill() -> f64 {
    print_analysis::DEFAULT_INFILL
}

fn default_filament_diameter() -> f64 {
    print_analysis::DEFAULT_FILAMENT_DIAMETER
}

/// The print estimate: the material (a material id; null for each body's
/// own, else PLA), the slicer's settings and a price per kilogram (null:
/// no cost).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EstimateCheck {
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default)]
    pub material: Option<String>,
    #[serde(default = "default_line_width")]
    pub line_width: f64,
    #[serde(default = "default_walls")]
    pub walls: u32,
    #[serde(default = "default_layer_height")]
    pub layer_height: f64,
    #[serde(default = "default_top_bottom_layers")]
    pub top_bottom_layers: u32,
    #[serde(default = "default_infill")]
    pub infill: f64,
    #[serde(default = "default_filament_diameter")]
    pub filament_diameter: f64,
    #[serde(default)]
    pub price_per_kg: Option<f64>,
}

impl Default for EstimateCheck {
    fn default() -> Self {
        let defaults = print_analysis::EstimateSettings::default();
        Self {
            enabled: true,
            material: None,
            line_width: defaults.line_width,
            walls: defaults.walls,
            layer_height: defaults.layer_height,
            top_bottom_layers: defaults.top_bottom_layers,
            infill: defaults.infill,
            filament_diameter: defaults.filament_diameter,
            price_per_kg: defaults.price_per_kg,
        }
    }
}

impl EstimateCheck {
    /// The numbers of the estimate.
    pub fn settings(&self) -> print_analysis::EstimateSettings {
        print_analysis::EstimateSettings {
            line_width: self.line_width,
            walls: self.walls,
            layer_height: self.layer_height,
            top_bottom_layers: self.top_bottom_layers,
            infill: self.infill,
            filament_diameter: self.filament_diameter,
            price_per_kg: self.price_per_kg,
        }
    }
}

/// Narrow gaps (mitcad#1519): pairs of the bodies, as they are placed,
/// apart but closer than `gap` mm, which fuse when printed in place.
/// `gap` null: the design's printed fit parameter `print_in_place_gap`,
/// else [`print_analysis::DEFAULT_MIN_GAP`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MinGapCheck {
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default)]
    pub gap: Option<f64>,
}

impl Default for MinGapCheck {
    fn default() -> Self {
        Self {
            enabled: true,
            gap: None,
        }
    }
}

impl PrintAnalysis {
    /// Why the settings cannot be kept, if they cannot (not the geometry).
    fn check(&self) -> Result<(), String> {
        if self.bodies.is_empty() {
            return Err("the print analysis needs a body".to_owned());
        }
        check_overhang_angle(self.overhang.angle)?;
        check_minimum_wall(self.thin_wall.minimum)?;
        if let Some(gap) = self.min_gap.gap {
            print_analysis::check_min_gap(gap)?;
        }
        self.estimate.settings().check()?;
        if let Some(material) = &self.estimate.material {
            print_material(material)?;
        }
        Ok(())
    }
}

/// Why an overhang angle (degrees from the vertical) is not one.
pub fn check_overhang_angle(angle: f64) -> Result<(), String> {
    if (0.0..=90.0).contains(&angle) {
        Ok(())
    } else {
        Err(format!(
            "the overhang angle must be from 0 to 90 degrees, got {angle}"
        ))
    }
}

/// Why a minimum wall (mm) is not one.
pub fn check_minimum_wall(minimum: f64) -> Result<(), String> {
    if minimum > 0.0 && minimum.is_finite() {
        Ok(())
    } else {
        Err(format!(
            "the minimum wall must be greater than zero, got {minimum}"
        ))
    }
}

/// A material of the library by its id (`pla`) or name (`PLA Plastic`).
pub fn print_material(material: &str) -> Result<&'static analysis::Material, String> {
    analysis::material(material)
        .or_else(|| {
            analysis::MATERIALS
                .iter()
                .find(|m| m.name.eq_ignore_ascii_case(material))
        })
        .ok_or_else(|| format!("there is no material {material}"))
}

/// The bodies shown cut at a plane: the half-space on the side the
/// section's normal points to is cut away.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SectionAnalysis {
    /// A plane reference: an origin plane, a construction plane, a planar
    /// face or a fixed plane.
    pub plane: GeomRef,
    /// The selected plane's occurrence, from the design root. Older
    /// analyses without one resolve a component reference only if unique.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub occurrence: Option<OccurrencePath>,
    /// How far the section is from the plane along its normal, in mm.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub offset: f64,
    /// The other side is cut away: the section's normal is the plane's
    /// reversed.
    #[serde(default, skip_serializing_if = "is_false")]
    pub flip: bool,
    /// How far the plane is turned about `axis` before the offset, in
    /// radians, right-handed about the axis' direction (mitcad#823).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub angle: f64,
    /// A line reference the plane turns about: a straight edge, a sketch
    /// line, a construction axis or an origin axis. Needed for an angle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub axis: Option<GeomRef>,
    /// The axis' occurrence, from the design root, as `occurrence` is the
    /// plane's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub axis_occurrence: Option<OccurrencePath>,
}

fn is_zero(value: &f64) -> bool {
    *value == 0.0
}

const NEEDS_AN_AXIS: &str = "an angle needs an axis to turn the plane about";

impl SectionAnalysis {
    /// Why the angle cannot be kept, if it cannot (not its geometry): not
    /// finite, or not zero without an axis to turn about. A face or an
    /// edge as the plane may have its own axis (mitcad#1769): whether it
    /// has is known only from its geometry ([`Document::section_plane`]).
    fn check_angle(&self) -> Result<(), String> {
        if !self.angle.is_finite() {
            return Err("the angle must be a finite angle".to_owned());
        }
        let may_have_axis = matches!(self.plane, GeomRef::Face { .. } | GeomRef::Edge { .. });
        if self.angle != 0.0 && self.axis.is_none() && !may_have_axis {
            return Err(NEEDS_AN_AXIS.to_owned());
        }
        Ok(())
    }
}

/// Where the turn of a section's plane about an axis (a face round about
/// it, a circular edge) starts (mitcad#1769), never from the view: the
/// normal of the first of the origin planes XY, XZ and YZ whose normal is
/// at least 45 degrees from the axis, made square to it. For an axis along
/// a model axis that is the first origin plane that contains it (XZ for
/// Z, XY for X and Y). At most one model axis is within 45 degrees of a
/// direction, so one of the three always is.
fn section_start_normal(axis: Vec3) -> Vec3 {
    let axis = mitcad_vec3::unit(axis).unwrap_or([0.0, 0.0, 1.0]);
    let candidates = [OriginDatum::Xy, OriginDatum::Xz, OriginDatum::Yz].map(|origin| {
        let crate::datum::Datum::Plane(plane) = origin.datum() else {
            unreachable!("an origin plane is a plane")
        };
        plane.normal()
    });
    let square = |n: Vec3| {
        mitcad_vec3::unit(mitcad_vec3::sub(
            n,
            mitcad_vec3::scale(axis, mitcad_vec3::dot(n, axis)),
        ))
    };
    candidates
        .iter()
        .find(|n| mitcad_vec3::dot(**n, axis).abs() <= std::f64::consts::FRAC_1_SQRT_2 + 1e-9)
        .and_then(|n| square(*n))
        // Not reached for a direction (see above); a zero axis as Z.
        .unwrap_or([0.0, 1.0, 0.0])
}

/// A section's plane in the design's coordinates: a point on it and the
/// unit normal of the side cut away.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SectionPlane {
    pub origin: Vec3,
    pub normal: Vec3,
}

impl Analysis {
    /// Why the analysis cannot be kept, if it cannot (not its geometry).
    pub(crate) fn check(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("the analysis needs a name".to_owned());
        }
        match &self.def {
            AnalysisDef::Section(section) => {
                if !section.offset.is_finite() {
                    return Err(format!("{}: the offset must be a finite length", self.name));
                }
                section
                    .check_angle()
                    .map_err(|e| format!("{}: {e}", self.name))?;
            }
            AnalysisDef::Print(print) => {
                print.check().map_err(|e| format!("{}: {e}", self.name))?;
            }
            AnalysisDef::Draft(draft) => {
                draft.check().map_err(|e| format!("{}: {e}", self.name))?;
            }
            AnalysisDef::PointCloudDeviation(deviation) => {
                deviation
                    .check()
                    .map_err(|e| format!("{}: {e}", self.name))?;
            }
            AnalysisDef::MinRadius(radius) => {
                radius.check().map_err(|e| format!("{}: {e}", self.name))?;
            }
            AnalysisDef::MinimumDistance(distance) => {
                distance
                    .check()
                    .map_err(|e| format!("{}: {e}", self.name))?;
            }
            // mitcad#2517
            AnalysisDef::Zebra(zebra) => {
                zebra.check().map_err(|e| format!("{}: {e}", self.name))?;
            }
            AnalysisDef::Curvature(curvature) => {
                curvature
                    .check()
                    .map_err(|e| format!("{}: {e}", self.name))?;
            }
            AnalysisDef::Isocurve(isocurve) => {
                isocurve
                    .check()
                    .map_err(|e| format!("{}: {e}", self.name))?;
            }
            AnalysisDef::Environment(environment) => {
                environment
                    .check()
                    .map_err(|e| format!("{}: {e}", self.name))?;
            }
            // mitcad#2664
            AnalysisDef::Interference(interference) => {
                interference
                    .check()
                    .map_err(|e| format!("{}: {e}", self.name))?;
            }
        }
        Ok(())
    }
}

impl<K: Kernel> Document<K> {
    /// Resolve an analysis reference in design coordinates. Modelling
    /// features still use resolve_plane in their own component coordinates.
    pub fn analysis_plane(
        &self,
        reference: &GeomRef,
        occurrence: Option<&OccurrencePath>,
    ) -> Result<DatumPlane, String> {
        let plane = self.resolve_plane(reference)?;
        let path = self.analysis_plane_occurrence(reference, occurrence)?;
        let transform = self.path_transform(&path.0);
        Ok(DatumPlane {
            origin: transform.apply_point(plane.origin),
            x_axis: transform.apply_vector(plane.x_axis),
            y_axis: transform.apply_vector(plane.y_axis),
        })
    }

    pub fn analysis_plane_occurrence(
        &self,
        reference: &GeomRef,
        occurrence: Option<&OccurrencePath>,
    ) -> Result<OccurrencePath, String> {
        self.analysis_occurrence(reference, occurrence, "section plane")
    }

    /// Resolve a section's axis reference in design coordinates
    /// (mitcad#823), through its occurrence as [`Self::analysis_plane`].
    #[doc(hidden)]
    pub fn analysis_axis(
        &self,
        reference: &GeomRef,
        occurrence: Option<&OccurrencePath>,
    ) -> Result<DatumAxis, String> {
        let axis = self.resolve_axis(reference)?;
        let path = self.analysis_axis_occurrence(reference, occurrence)?;
        let transform = self.path_transform(&path.0);
        let direction = mitcad_vec3::unit(transform.apply_vector(axis.direction))
            .ok_or_else(|| "the axis has no direction".to_owned())?;
        Ok(DatumAxis {
            origin: transform.apply_point(axis.origin),
            direction,
        })
    }

    #[doc(hidden)]
    pub fn analysis_axis_occurrence(
        &self,
        reference: &GeomRef,
        occurrence: Option<&OccurrencePath>,
    ) -> Result<OccurrencePath, String> {
        self.analysis_occurrence(reference, occurrence, "section axis")
    }

    /// The occurrence a section's plane or axis (`what`) is in: the given
    /// path when it owns the reference, else the owner's only placement,
    /// else the root for references of no component.
    pub(crate) fn analysis_occurrence(
        &self,
        reference: &GeomRef,
        occurrence: Option<&OccurrencePath>,
        what: &str,
    ) -> Result<OccurrencePath, String> {
        let owner = match reference {
            GeomRef::Face { body, .. } | GeomRef::Edge { body, .. } => self.body_component(*body),
            GeomRef::Datum(uid) | GeomRef::SketchCurves { sketch: uid, .. } => {
                self.feature(*uid).map(|f| f.component)
            }
            _ => None,
        };
        let path = if let Some(path) = occurrence {
            let component = path_component(self.assembly(), ComponentUid::ROOT, &path.0)?;
            if owner.is_some_and(|owner| owner != component) {
                return Err(format!("the selected occurrence does not own the {what}"));
            }
            path.0.clone()
        } else if let Some(owner) = owner {
            let paths = self.assembly().paths_to(owner);
            if paths.len() != 1 {
                return Err(format!("select an occurrence for the {what}'s component"));
            }
            paths[0].clone()
        } else {
            Vec::new()
        };
        Ok(OccurrencePath(path))
    }

    /// The analyses in the order they were added.
    pub fn analyses(&self) -> &[Analysis] {
        &self.state.analyses
    }

    /// The plane a section's plane reference places, before its Angle,
    /// Distance and Flip, in design coordinates, and the axis it turns
    /// about without an Axis of its own (mitcad#1769): a plane as it is; a
    /// face round about an axis or a circular edge, the plane that contains
    /// that axis, its turn about it starting at `section_start_normal`,
    /// with that axis; a straight edge, the plane through its middle normal
    /// to it. Through the reference's occurrence as [`Self::analysis_plane`].
    pub fn section_pick(
        &self,
        reference: &GeomRef,
        occurrence: Option<&OccurrencePath>,
    ) -> Result<(DatumPlane, Option<DatumAxis>), String> {
        use crate::features::geom_ref::{SectionPick, resolve_section_pick};
        let (plane, own_axis) = match resolve_section_pick(&mut self.resolver(), reference)? {
            SectionPick::Plane(plane) => (plane, None),
            SectionPick::Axis(axis) => {
                let normal = section_start_normal(axis.direction);
                let plane = DatumPlane {
                    origin: axis.origin,
                    x_axis: axis.direction,
                    y_axis: mitcad_vec3::cross(normal, axis.direction),
                };
                (plane, Some(axis))
            }
            SectionPick::Across { point, normal } => {
                (DatumPlane::from_normal(point, normal, None), None)
            }
        };
        let path = self.analysis_plane_occurrence(reference, occurrence)?;
        let transform = self.path_transform(&path.0);
        let plane = DatumPlane {
            origin: transform.apply_point(plane.origin),
            x_axis: transform.apply_vector(plane.x_axis),
            y_axis: transform.apply_vector(plane.y_axis),
        };
        let own_axis = own_axis
            .map(|axis| {
                mitcad_vec3::unit(transform.apply_vector(axis.direction))
                    .map(|direction| DatumAxis {
                        origin: transform.apply_point(axis.origin),
                        direction,
                    })
                    .ok_or_else(|| "the axis has no direction".to_owned())
            })
            .transpose()?;
        Ok((plane, own_axis))
    }

    /// The axis a section's plane turns about by its Angle (mitcad#823,
    /// #1769), in design coordinates: its Axis, else the axis of a face
    /// round about one or of a circular edge picked as its plane; None
    /// without either; or why it cannot be found.
    pub fn section_turn_axis(
        &self,
        section: &SectionAnalysis,
    ) -> Result<Option<DatumAxis>, String> {
        match self.section_axis(section)? {
            Some(axis) => Ok(Some(axis)),
            None => Ok(self
                .section_pick(&section.plane, section.occurrence.as_ref())?
                .1),
        }
    }

    /// A section's plane at the timeline marker, or why it cannot be found.
    pub fn section_plane(&self, section: &SectionAnalysis) -> Result<SectionPlane, String> {
        if !section.offset.is_finite() {
            return Err("the section offset must be finite".to_owned());
        }
        section.check_angle()?;
        let (plane, own_axis) = self.section_pick(&section.plane, section.occurrence.as_ref())?;
        let normal = plane.normal();
        let axis = match self.section_axis(section)? {
            Some(axis) => {
                if mitcad_vec3::norm(mitcad_vec3::cross(axis.direction, normal)) < ANGULAR {
                    return Err("the axis is along the plane's normal".to_owned());
                }
                Some(axis)
            }
            // A face round about an axis or a circular edge turns about its
            // own axis (mitcad#1769).
            None => own_axis,
        };
        let (base, n) = match axis {
            // Turned rigidly about the axis, then moved along the turned
            // normal (mitcad#823).
            Some(axis) => {
                let turn = Transform::rotation(axis.origin, axis.direction, section.angle)
                    .ok_or_else(|| "the axis has no direction".to_owned())?;
                (turn.apply_point(plane.origin), turn.apply_vector(normal))
            }
            None if section.angle != 0.0 => {
                return Err(NEEDS_AN_AXIS.to_owned());
            }
            None => (plane.origin, normal),
        };
        let origin = [0, 1, 2].map(|i| base[i] + n[i] * section.offset);
        // No -0 in the normal.
        let normal = n.map(|c| {
            if c == 0.0 {
                0.0
            } else if section.flip {
                -c
            } else {
                c
            }
        });
        Ok(SectionPlane { origin, normal })
    }

    /// A section's axis in design coordinates at the marker, None without
    /// one, or why it cannot be found.
    pub fn section_axis(&self, section: &SectionAnalysis) -> Result<Option<DatumAxis>, String> {
        section
            .axis
            .as_ref()
            .map(|axis| {
                self.analysis_axis(axis, section.axis_occurrence.as_ref())
                    .map_err(|e| format!("the axis: {e}"))
            })
            .transpose()
    }

    /// The `analyses` query: each analysis in its file form, with a
    /// section's plane at the marker (`section`) or why it is not there
    /// (`error`).
    pub fn analyses_json(&self) -> Value {
        Value::Array(
            self.state
                .analyses
                .iter()
                .map(|analysis| {
                    let mut value = serde_json::to_value(analysis).expect("analyses serialize");
                    value["visible"] = json!(analysis.visible);
                    match &analysis.def {
                        AnalysisDef::Section(section) => match self.section_plane(section) {
                            Ok(plane) => {
                                value["section"] =
                                    json!({"origin": plane.origin, "normal": plane.normal});
                            }
                            Err(error) => value["error"] = json!(error),
                        },
                        // The print direction at the marker, or why a body
                        // or the bed face is not there.
                        AnalysisDef::Print(print) => match self.print_direction(print) {
                            Ok(down) => value["down"] = json!(down),
                            Err(error) => value["error"] = json!(error),
                        },
                        // The pull direction at the marker, or why a body
                        // or the pull reference is not there.
                        AnalysisDef::Draft(draft) => match self.draft_pull(draft) {
                            Ok(pull) => value["pull_direction"] = json!(pull),
                            Err(error) => value["error"] = json!(error),
                        },
                        // Why its cloud or body is not there (mitcad#2240).
                        AnalysisDef::PointCloudDeviation(deviation) => {
                            if let Err(error) = self.point_cloud_deviation_check(deviation) {
                                value["error"] = json!(error);
                            }
                        }
                        // Why a body or a face is not there (mitcad#2518).
                        AnalysisDef::MinRadius(radius) => {
                            if let Err(error) = self.min_radius_check(radius) {
                                value["error"] = json!(error);
                            }
                        }
                        // Why a body or a component is not there
                        // (mitcad#2519); its distance is the
                        // `minimum_distance` query's.
                        AnalysisDef::MinimumDistance(distance) => {
                            if let Err(error) = self.minimum_distance_check(distance) {
                                value["error"] = json!(error);
                            }
                        }
                        // The stripes' direction at the marker, or why a
                        // body or the direction is not there (mitcad#2517).
                        AnalysisDef::Zebra(zebra) => match self.zebra_direction(zebra) {
                            Ok(direction) => value["stripe_direction"] = json!(direction),
                            Err(error) => value["error"] = json!(error),
                        },
                        // Why a body is not there (mitcad#2517).
                        AnalysisDef::Curvature(_)
                        | AnalysisDef::Isocurve(_)
                        | AnalysisDef::Environment(_) => {
                            if let Some((bodies, faces)) = analysis.def.surface_inputs()
                                && let Err(error) = self.surface_targets(bodies, faces)
                            {
                                value["error"] = json!(error);
                            }
                        }
                        // Why a body it checks is not there (mitcad#2664);
                        // its pairs are the `interference` query's.
                        AnalysisDef::Interference(interference) => {
                            if let Err(error) = self.interference_check(interference) {
                                value["error"] = json!(error);
                            }
                        }
                    }
                    value
                })
                .collect(),
        )
    }

    /// Why the definition cannot be kept now: its plane is no plane at the
    /// marker.
    fn check_analysis_def(&self, def: &AnalysisDef) -> Result<(), ModelError> {
        match def {
            AnalysisDef::Section(section) => {
                self.section_plane(section)
                    .map_err(|e| invalid(format!("the section's plane: {e}")))?;
            }
            AnalysisDef::Print(print) => {
                self.print_direction(print)
                    .map_err(|e| invalid(format!("the print analysis: {e}")))?;
            }
            AnalysisDef::Draft(draft) => {
                self.draft_pull(draft)
                    .map_err(|e| invalid(format!("the draft analysis: {e}")))?;
            }
            AnalysisDef::PointCloudDeviation(deviation) => {
                self.point_cloud_deviation_check(deviation)
                    .map_err(|e| invalid(format!("the deviation: {e}")))?;
            }
            AnalysisDef::MinRadius(radius) => {
                self.min_radius_check(radius)
                    .map_err(|e| invalid(format!("the minimum radius analysis: {e}")))?;
            }
            AnalysisDef::MinimumDistance(distance) => {
                self.minimum_distance_check(distance)
                    .map_err(|e| invalid(format!("the minimum distance: {e}")))?;
            }
            // mitcad#2517
            AnalysisDef::Zebra(zebra) => {
                self.zebra_direction(zebra)
                    .map_err(|e| invalid(format!("the zebra analysis: {e}")))?;
            }
            AnalysisDef::Curvature(_) | AnalysisDef::Isocurve(_) | AnalysisDef::Environment(_) => {
                if let Some((bodies, faces)) = def.surface_inputs() {
                    self.surface_targets(bodies, faces)
                        .map_err(|e| invalid(format!("the surface analysis: {e}")))?;
                }
            }
            // mitcad#2664
            AnalysisDef::Interference(interference) => {
                self.interference_check(interference)
                    .map_err(|e| invalid(format!("the interference check: {e}")))?;
            }
        }
        Ok(())
    }

    /// Why an interference check's bodies are not there at the marker, if
    /// they are not (mitcad#2664): in words, how many of them are gone,
    /// with what to do; an empty list (every body) always is.
    pub fn interference_check(&self, def: &InterferenceAnalysis) -> Result<(), String> {
        let gone = def
            .bodies
            .iter()
            .filter(|body| self.body_shape(**body).is_none())
            .count();
        match gone {
            0 => Ok(()),
            1 => Err(
                "one of its bodies is not in the design at the timeline marker: \
                 edit the check to choose its bodies again"
                    .to_owned(),
            ),
            n => Err(format!(
                "{n} of its bodies are not in the design at the timeline marker: \
                 edit the check to choose its bodies again"
            )),
        }
    }

    /// Why a minimum radius analysis' bodies or faces are not there at the
    /// marker, if they are not (mitcad#2518).
    pub fn min_radius_check(&self, def: &MinRadiusAnalysis) -> Result<(), String> {
        for (body, faces) in def.targets() {
            let Some(shape) = self.body_shape(body) else {
                return Err(format!("body {body} does not exist at the timeline marker"));
            };
            for face in &faces {
                // A kernel that cannot count faces leaves it to the query.
                if self.kernel().count_faces(shape, face).is_ok_and(|n| n == 0) {
                    return Err(format!(
                        "body {body} has no face {face} at the timeline marker"
                    ));
                }
            }
        }
        Ok(())
    }

    /// Why a minimum distance's picks are not there at the marker
    /// (mitcad#2519): a body that does not exist, a component's occurrence
    /// that does not, or a pick's occurrence that does not place its body.
    /// Its faces, edges and vertices are looked for when it is measured.
    pub fn minimum_distance_check(&self, distance: &MinimumDistanceAnalysis) -> Result<(), String> {
        for pick in distance.first.iter().chain(&distance.second) {
            check_distance_pick(pick)?;
            match (pick.body, &pick.occurrence) {
                (Some(body), occurrence) => {
                    if self.body_shape(body).is_none() {
                        return Err(format!("body {body} does not exist at the timeline marker"));
                    }
                    if let Some(path) = occurrence {
                        let component =
                            path_component(self.assembly(), ComponentUid::ROOT, &path.0)?;
                        if self
                            .body_component(body)
                            .is_some_and(|owner| owner != component)
                        {
                            return Err(format!("occurrence {path} does not place body {body}"));
                        }
                    }
                }
                (None, Some(path)) => {
                    path_component(self.assembly(), ComponentUid::ROOT, &path.0)?;
                }
                (None, None) => {}
            }
        }
        if let Some(clearance) = distance.clearance {
            check_clearance(clearance)?;
        }
        Ok(())
    }

    /// A draft analysis' pull direction (unit, design coordinates) at the
    /// marker, flipped when it says so, or why its bodies or its pull
    /// reference are not there (mitcad#2166).
    pub fn draft_pull(&self, draft: &DraftAnalysis) -> Result<Vec3, String> {
        for body in &draft.bodies {
            if self.body_shape(*body).is_none() {
                return Err(format!("body {body} does not exist at the timeline marker"));
            }
        }
        let pull = match &draft.pull {
            None => [0.0, 0.0, 1.0],
            Some(reference) => self
                .pull_direction(reference, draft.occurrence.as_ref())
                .map_err(|e| format!("the pull direction: {e}"))?,
        };
        // No -0 in the direction.
        Ok(pull.map(|c| {
            if c == 0.0 {
                0.0
            } else if draft.flip {
                -c
            } else {
                c
            }
        }))
    }

    /// The direction a pull reference gives, in design coordinates through
    /// its occurrence (as [`Self::analysis_plane`]): a planar face's or a
    /// fixed plane's normal; a straight edge's, a sketch line's or a fixed
    /// axis' direction; an origin or construction datum's normal when it is
    /// a plane, else its direction when it is an axis.
    pub fn pull_direction(
        &self,
        reference: &GeomRef,
        occurrence: Option<&OccurrencePath>,
    ) -> Result<Vec3, String> {
        let direction = match reference {
            GeomRef::Face { .. } | GeomRef::FixedPlane { .. } => {
                self.resolve_plane(reference)?.normal()
            }
            GeomRef::Edge { .. } | GeomRef::SketchCurves { .. } | GeomRef::FixedAxis { .. } => {
                self.resolve_axis(reference)?.direction
            }
            GeomRef::Mount { .. } => self.resolve_plane(reference)?.normal(),
            GeomRef::Origin(_) | GeomRef::Datum(_) => {
                let datum = match reference {
                    GeomRef::Origin(origin) => Some(origin.datum()),
                    GeomRef::Datum(uid) => self.datum(*uid),
                    _ => None,
                };
                match datum {
                    Some(crate::datum::Datum::Plane(plane)) => plane.normal(),
                    Some(crate::datum::Datum::Axis(axis)) => axis.direction,
                    Some(_) => {
                        return Err(
                            reference.error(|what| format!("{what} is neither a plane nor a line"))
                        );
                    }
                    // Gone at the marker: the resolver says why in words.
                    None => self.resolve_plane(reference)?.normal(),
                }
            }
            _ => {
                return Err(reference.error(|what| format!("{what} is neither a plane nor a line")));
            }
        };
        let path = self.analysis_occurrence(reference, occurrence, "pull direction")?;
        let transform = self.path_transform(&path.0);
        mitcad_vec3::unit(transform.apply_vector(direction))
            .ok_or_else(|| "the pull direction has no length".to_owned())
    }

    /// Where each body is printed from: its first shown placement in the
    /// design, else its first, else its own coordinates (mitcad#620).
    pub fn print_placements(&self, bodies: &[BodyUid]) -> Vec<Transform> {
        let instances = self.instances();
        bodies
            .iter()
            .map(|body| {
                let mut placed = instances.iter().filter(|i| i.body == *body);
                let first = placed.clone().next();
                placed
                    .find(|i| i.visible)
                    .or(first)
                    .map_or(Transform::IDENTITY, |i| i.transform)
            })
            .collect()
    }

    /// The gap (mm) under which bodies printed in place fuse, when none is
    /// given (mitcad#1519), and where it comes from: the design's user
    /// parameter `print_in_place_gap` when it is a length greater than
    /// zero (`"parameter"`), else [`print_analysis::DEFAULT_MIN_GAP`]
    /// (`"default"`).
    pub fn print_min_gap_default(&self) -> (f64, &'static str) {
        let params = self.parameters();
        params
            .find(crate::print_fits::PRINT_IN_PLACE_GAP)
            .and_then(|id| {
                let parameter = params.get(id)?;
                let value = params.value(id)?;
                (parameter.unit().dims() == crate::expr::Dims::LENGTH
                    && value > 0.0
                    && value.is_finite())
                .then_some(value)
            })
            .map_or((print_analysis::DEFAULT_MIN_GAP, "default"), |value| {
                (value, "parameter")
            })
    }

    /// The print direction (unit, towards the bed) and a point of the bed
    /// that a bed face gives, in design coordinates: the face's outward
    /// normal and its plane, where its body is printed from.
    pub fn print_bed(&self, bed_face: &GeomRef) -> Result<(Vec3, Vec3), String> {
        let GeomRef::Face { body, .. } = bed_face else {
            return Err("the bed face must be a planar face of a body".to_owned());
        };
        let plane = self
            .resolve_plane(bed_face)
            .map_err(|e| format!("the bed face: {e}"))?;
        let placement = self.print_placements(&[*body])[0];
        let down = mitcad_vec3::unit(placement.apply_vector(plane.normal()))
            .ok_or_else(|| "the bed face has no normal".to_owned())?;
        Ok((down, placement.apply_point(plane.origin)))
    }

    /// A print analysis' direction at the marker, or why its bodies or bed
    /// face are not there.
    pub fn print_direction(&self, print: &PrintAnalysis) -> Result<Vec3, String> {
        for body in &print.bodies {
            if self.body_shape(*body).is_none() {
                return Err(format!("body {body} does not exist at the timeline marker"));
            }
        }
        match &print.bed_face {
            None => Ok([0.0, 0.0, -1.0]),
            Some(face) => {
                if let GeomRef::Face { body, .. } = face
                    && !print.bodies.contains(body)
                {
                    return Err(format!(
                        "the bed face is on body {body}, which the analysis does not include"
                    ));
                }
                self.print_bed(face).map(|(down, _)| down)
            }
        }
    }

    /// Adds an analysis, shown, and hides the others; an interference check
    /// (mitcad#2664), which is not drawn, is added not shown and leaves the
    /// others as they are. Without a name it is `Section1`, `Section2`, ...
    /// (the next free number). Returns the name.
    pub fn add_analysis(
        &mut self,
        def: AnalysisDef,
        name: Option<&str>,
    ) -> Result<String, ModelError> {
        let name = match name.map(str::trim).filter(|n| !n.is_empty()) {
            Some(name) => name.to_owned(),
            None => {
                let stem = def.name_stem();
                (1..)
                    .map(|n| format!("{stem}{n}"))
                    .find(|name| self.analysis_index(name).is_err())
                    .expect("a free name")
            }
        };
        if self.analysis_index(&name).is_ok() {
            return Err(invalid(format!("an analysis {name} exists")));
        }
        let drawn = def.is_drawn();
        let analysis = Analysis {
            name: name.clone(),
            visible: drawn,
            def,
        };
        analysis.check().map_err(invalid)?;
        self.check_analysis_def(&analysis.def)?;
        let label = format!("Add {name}");
        self.apply_with(|state| {
            if drawn {
                for other in &mut state.analyses {
                    other.visible = false;
                }
            }
            state.analyses.push(analysis);
            Ok((label, (), false))
        })?;
        Ok(name)
    }

    /// Changes an analysis' definition; `visible` shows (hiding the others)
    /// or hides it too. An interference check (mitcad#2664) stays not
    /// shown: showing it is refused in words. A change that changes nothing
    /// is no undo step.
    pub fn edit_analysis(
        &mut self,
        name: &str,
        def: AnalysisDef,
        visible: Option<bool>,
    ) -> Result<(), ModelError> {
        let i = self.analysis_index(name)?;
        let old = &self.state.analyses[i];
        let visible = if def.is_drawn() {
            visible.unwrap_or(old.visible)
        } else if visible == Some(true) {
            return Err(invalid(NOT_DRAWN));
        } else {
            false
        };
        let unchanged = old.def == def
            && old.visible == visible
            && (!visible || self.state.analyses.iter().filter(|a| a.visible).count() <= 1);
        if unchanged {
            return Ok(());
        }
        let analysis = Analysis {
            name: name.to_owned(),
            visible,
            def,
        };
        analysis.check().map_err(invalid)?;
        self.check_analysis_def(&analysis.def)?;
        let label = format!("Edit {name}");
        self.apply_with(|state| {
            show_only(state, i, visible);
            state.analyses[i] = analysis;
            Ok((label, (), false))
        })
    }

    /// Shows (hiding the others) or hides an analysis; an undo step
    /// `Show Section1` / `Hide Section1` unless nothing changes. An
    /// interference check (mitcad#2664) is never shown: showing it is
    /// refused in words, hiding it changes nothing.
    pub fn set_analysis_visible(&mut self, name: &str, visible: bool) -> Result<(), ModelError> {
        let i = self.analysis_index(name)?;
        if visible && !self.state.analyses[i].def.is_drawn() {
            return Err(invalid(NOT_DRAWN));
        }
        let others_shown = self
            .state
            .analyses
            .iter()
            .enumerate()
            .any(|(j, a)| j != i && a.visible);
        if self.state.analyses[i].visible == visible && !(visible && others_shown) {
            return Ok(());
        }
        let label = format!("{} {name}", if visible { "Show" } else { "Hide" });
        self.apply_with(|state| {
            show_only(state, i, visible);
            state.analyses[i].visible = visible;
            Ok((label, (), false))
        })
    }

    pub fn rename_analysis(&mut self, name: &str, new_name: &str) -> Result<(), ModelError> {
        let i = self.analysis_index(name)?;
        let new_name = new_name.trim();
        if new_name.is_empty() {
            return Err(invalid("the analysis needs a name"));
        }
        if new_name == name {
            return Ok(());
        }
        if self.analysis_index(new_name).is_ok() {
            return Err(invalid(format!("an analysis {new_name} exists")));
        }
        let label = format!("Rename {name}");
        let new_name = new_name.to_owned();
        self.apply_with(|state| {
            state.analyses[i].name = new_name;
            Ok((label, (), false))
        })
    }

    pub fn delete_analysis(&mut self, name: &str) -> Result<(), ModelError> {
        let i = self.analysis_index(name)?;
        let label = format!("Delete {name}");
        self.apply_with(|state| {
            state.analyses.remove(i);
            Ok((label, (), false))
        })
    }

    fn analysis_index(&self, name: &str) -> Result<usize, ModelError> {
        self.state
            .analyses
            .iter()
            .position(|a| a.name == name)
            .ok_or_else(|| invalid(format!("there is no analysis {name}")))
    }
}

/// When analysis `i` is to be shown, the others are hidden.
fn show_only(state: &mut super::DocState, i: usize, visible: bool) {
    if visible {
        for (j, other) in state.analyses.iter_mut().enumerate() {
            if j != i {
                other.visible = false;
            }
        }
    }
}
